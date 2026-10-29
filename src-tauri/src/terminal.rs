// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Workspace's terminal sessions: the user's own `claude`, `codex` or shell in a pty, in a
//! folder outside the vault, through their login shell. Output goes to the window as `term-output`
//! (base64, gathered for about 16 ms or 32 KB, with its byte offset in the session's output) and
//! the last 512 KB are kept for redrawing, with the offset they end at; keystrokes go to a writer
//! thread per session, so a session that isn't reading never holds up the window. Claude
//! Code's hooks (`Brainstead --hook`, workspace.rs) write `<data>/workspace/events.jsonl`, which is
//! read here twice a second to move each session between working, waiting and idle
//! (brainstead_core::workspace::apply). A Claude Code session in an outside terminal started with
//! the same command shows up through the same events, state only.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use brainstead_core::workspace::{self as ws, Alert, HookEvent, Program, SessionInfo, SessionState};
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::platform;

/// The output kept per session for `term_buffer`.
const BUFFER_MAX: usize = 512 * 1024;
/// Output is sent to the window once this much has gathered, or after `FLUSH_AFTER`.
const FLUSH_BYTES: usize = 32 * 1024;
const FLUSH_AFTER: Duration = Duration::from_millis(16);
/// How long a stopped session has to end before it's killed.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// An outside session heard nothing from in this long is no longer listed.
const EXTERNAL_STALE_MS: i64 = 12 * 3600 * 1000;
/// The events file is started afresh once it's this big, at launch or while running.
const EVENTS_MAX: u64 = 5 * 1024 * 1024;
/// How far back from the cut a trimmed buffer looks for a line's end to start at, so a redraw
/// doesn't begin inside an escape sequence.
const TRIM_LOOKBACK: usize = 4 * 1024;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// `<data>/workspace/`, where the settings file and the events live.
pub fn dir() -> PathBuf {
    platform::data_dir().join("workspace")
}

pub fn events_file() -> PathBuf {
    dir().join("events.jsonl")
}

pub fn settings_file() -> PathBuf {
    dir().join("claude-settings.json")
}

struct Pty {
    master: Box<dyn MasterPty + Send>,
    /// Keystrokes, for the session's writer thread.
    input: mpsc::Sender<Vec<u8>>,
}

/// The output kept for redrawing, and how many bytes the session has written in all: the offset
/// `data` ends at.
#[derive(Default)]
struct Tail {
    data: Vec<u8>,
    end: u64,
}

struct Term {
    info: SessionInfo,
    /// The terminal, until the process has ended or the session was closed (which hangs it up);
    /// None for a session in an outside terminal.
    pty: Option<Pty>,
    pid: Option<u32>,
    /// Until the process is seen to end.
    running: bool,
    /// Closed by the user: no longer listed, and forgotten once its process has ended.
    closed: bool,
    tail: Arc<Mutex<Tail>>,
    /// Tells the reader thread to stop, though something the session started holds the terminal.
    stop_read: Arc<AtomicBool>,
    /// When its last hook event came, for dropping outside sessions gone quiet.
    last_event: i64,
}

impl Term {
    /// The session ends its reading and hangs up its terminal.
    fn hang_up(&mut self) {
        self.stop_read.store(true, Ordering::SeqCst);
        self.pty = None;
    }
}

/// Every session this run of the app, live and ended.
#[derive(Default)]
pub struct Terminals {
    terms: Mutex<Vec<Term>>,
    next: AtomicU64,
}

impl Terminals {
    fn with<T>(&self, id: &str, f: impl FnOnce(&mut Term) -> Result<T, String>) -> Result<T, String> {
        let mut g = crate::lock(&self.terms);
        let t = g.iter_mut().find(|t| t.info.id == id).ok_or("That session isn't there any more.")?;
        f(t)
    }

    /// Sessions running in Brainstead now, for the quit and update checks.
    pub fn live(&self) -> usize {
        crate::lock(&self.terms).iter().filter(|t| t.running).count()
    }

    fn running(&self, id: &str) -> bool {
        crate::lock(&self.terms).iter().any(|t| t.info.id == id && t.running)
    }

    /// On quit: every session's processes are asked to stop and its terminal hung up, so none is
    /// left behind.
    pub fn kill_all(&self) {
        for t in crate::lock(&self.terms).iter_mut().filter(|t| t.running) {
            if let Some(pid) = t.pid {
                let fg = t.pty.as_ref().and_then(|p| platform::pty_foreground(&*p.master, pid));
                platform::stop_group(pid);
                fg.into_iter().for_each(platform::stop_group);
            }
            t.hang_up();
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    folder: String,
    program: Program,
    resume: Option<String>,
    prompt: Option<String>,
    model: Option<String>,
    worktree: Option<String>,
}

#[derive(Serialize, Clone)]
struct Output<'a> {
    id: &'a str,
    data: String,
    /// How many bytes of the session's output came before this chunk.
    offset: u64,
}

/// What `term_buffer` returns (src/workspaceApi.ts `buffer`): the kept output, base64, and the
/// offset it ends at, so the window can drop `term-output` chunks it already has.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct OutputTail {
    data: String,
    end: u64,
}

#[derive(Serialize, Clone)]
struct Exit<'a> {
    id: &'a str,
    code: Option<i32>,
}

/// A folder as the user typed it: `~` expanded, no trailing separator.
pub fn folder_path(folder: &str) -> PathBuf {
    let f = folder.trim();
    let p = match f.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            dirs::home_dir().unwrap_or_default().join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(f),
    };
    let s = p.to_string_lossy();
    let t = s.trim_end_matches(['/', '\\']);
    if t.is_empty() {
        p
    } else {
        PathBuf::from(t)
    }
}

/// A folder a session may start in: there, a folder, and not in the vault.
fn check_folder(app: &AppHandle, folder: &str) -> Result<PathBuf, String> {
    let p = folder_path(folder);
    if !p.is_dir() {
        return Err(format!("{} isn't there, or isn't a folder.", p.display()));
    }
    if let Some(v) = app.state::<crate::vault::VaultService>().root() {
        if ws::in_vault(&v, &p) {
            return Err(ws::IN_VAULT.into());
        }
    }
    Ok(p)
}

/// The command line a session runs (claude or codex with Brainstead's MCP server and, for
/// Claude Code, the guard), or None for a shell.
fn launch_line(
    app: &AppHandle,
    folder: &Path,
    program: Program,
    resume: Option<&str>,
    prompt: Option<&str>,
    model: Option<&str>,
    worktree: Option<&str>,
) -> Option<String> {
    let cli = match program {
        Program::Claude => brainstead_core::ask::Cli::Claude,
        Program::Codex => brainstead_core::ask::Cli::Codex,
        Program::Shell => return None,
    };
    // Found where Ask finds it; else by name, which the login shell looks up on its PATH.
    let mut argv: Vec<String> = match crate::ask::find(cli) {
        Some(p) => std::iter::once(p.exe.to_string_lossy().into_owned()).chain(p.prefix_args).collect(),
        None => vec![cli.bin().to_string()],
    };
    let mcp = crate::ask::mcp_command(&[]);
    let settings = settings_file();
    let mcp_config = mcp.as_ref().filter(|_| program == Program::Claude).and_then(write_mcp_config);
    let vault_inside = app.state::<crate::vault::VaultService>().root().is_some_and(|v| ws::in_vault(folder, &v));
    argv.extend(ws::launch_args(&ws::Launch {
        program,
        settings: &settings,
        mcp: mcp.as_ref(),
        mcp_config: mcp_config.as_deref(),
        resume,
        prompt,
        model,
        worktree,
        vault_inside,
    }));
    Some(platform::command_line(&argv))
}

/// `<data>/workspace/mcp.json`, Claude Code's `--mcp-config` for Brainstead's server, written when
/// it has changed; None (no Brainstead tools) if it can't be.
fn write_mcp_config(m: &brainstead_core::ask::Mcp) -> Option<PathBuf> {
    let file = dir().join("mcp.json");
    let text = serde_json::to_string_pretty(&ws::mcp_config(m)).ok()?;
    if std::fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()) {
        return Some(file);
    }
    let r = std::fs::create_dir_all(dir()).and_then(|_| {
        brainstead_core::write::write_atomic(&file, text.as_bytes(), false).map_err(|e| std::io::Error::other(e.to_string()))
    });
    match r {
        Ok(()) => Some(file),
        Err(e) => {
            crate::applog!("workspace: couldn't write {}: {e}", file.display());
            None
        }
    }
}

fn shell(app: &AppHandle) -> PathBuf {
    let over = crate::lock(&app.state::<crate::AppState>().settings).workspace_shell.clone();
    platform::login_shell(over.as_deref())
}

fn emit_state(app: &AppHandle, info: &SessionInfo) {
    let _ = app.emit("term-state", info);
}

#[tauri::command]
pub async fn term_start(app: AppHandle, req: StartRequest) -> Result<SessionInfo, String> {
    crate::blocking(move || start(&app, req)).await
}

fn start(app: &AppHandle, req: StartRequest) -> Result<SessionInfo, String> {
    let folder = check_folder(app, &req.folder)?;
    if req.program == Program::Claude {
        crate::workspace::write_claude_settings(app);
    }
    let some = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let (resume, prompt, model, worktree) = (some(&req.resume), some(&req.prompt), some(&req.model), some(&req.worktree));
    let line = launch_line(app, &folder, req.program, resume.as_deref(), prompt.as_deref(), model.as_deref(), worktree.as_deref());
    let argv = platform::shell_argv(&shell(app), line.as_deref());

    let pair = portable_pty::native_pty_system()
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("Couldn't open a terminal: {e}"))?;
    let terms = app.state::<Arc<Terminals>>().inner().clone();
    let id = format!("term-{}-{}", now_ms(), terms.next.fetch_add(1, Ordering::SeqCst));
    let mut cmd = CommandBuilder::from_argv(argv.iter().map(Into::into).collect());
    cmd.cwd(&folder);
    cmd.env("BRAINSTEAD_TERM", &id);
    cmd.env("BRAINSTEAD_FOLDER", &folder);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Started from inside a Claude Code session (a developer's), claude would think it's nested.
    for k in ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"] {
        cmd.env_remove(k);
    }
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("Couldn't start the session: {e}"))?;
    // Only the child holds the terminal's other end, so reading ends when it does.
    drop(pair.slave);
    let reader = platform::PtyReader::new(&*pair.master).map_err(|e| format!("Couldn't read the session: {e}"))?;
    let writer = pair.master.take_writer().map_err(|e| format!("Couldn't write to the session: {e}"))?;
    let pid = child.process_id();
    let (input, keys) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || write_pty(writer, keys));

    let mut info = SessionInfo::new(&id, &folder, req.program, now_ms());
    info.title = prompt.as_deref().and_then(ws::title_of);
    info.project = crate::workspace::project_for(app, &folder);
    if req.program == Program::Claude {
        info.claude_session_id = resume.clone();
    }
    let tail = Arc::new(Mutex::new(Tail::default()));
    let stop_read = Arc::new(AtomicBool::new(false));
    crate::lock(&terms.terms).push(Term {
        info: info.clone(),
        pty: Some(Pty { master: pair.master, input }),
        pid,
        running: true,
        closed: false,
        tail: tail.clone(),
        stop_read: stop_read.clone(),
        last_event: now_ms(),
    });
    crate::workspace::note_recent(app, &folder);
    crate::applog!("workspace: started {} ({:?}) in {}", id, req.program, folder.display());
    emit_state(app, &info);

    // Reading on one thread, sending on another, so a slow window never stalls the program.
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    {
        let stop = stop_read.clone();
        std::thread::spawn(move || read_pty(reader, tx, &stop));
    }
    let (done_tx, done_rx) = mpsc::channel::<()>();
    {
        let (app, id) = (app.clone(), id.clone());
        std::thread::spawn(move || {
            send_output(&app, &id, rx, &tail);
            let _ = done_tx.send(());
        });
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let code = child.wait().ok().map(|s| s.exit_code() as i32);
        // The last of the output first, unless something it started holds the terminal open;
        // then the reading stops and the terminal is hung up, which ends whatever still holds it.
        let _ = done_rx.recv_timeout(Duration::from_millis(300));
        stop_read.store(true, Ordering::SeqCst);
        let terms = app.state::<Arc<Terminals>>();
        let info = {
            let mut g = crate::lock(&terms.terms);
            let i = g.iter().position(|t| t.info.id == id);
            i.map(|i| {
                let t = &mut g[i];
                t.hang_up();
                t.running = false;
                t.info.state = SessionState::Exited;
                t.info.exit_code = code;
                let shown = (!t.closed).then(|| t.info.clone());
                if t.closed {
                    g.remove(i);
                }
                shown
            })
        };
        if let Some(Some(info)) = info {
            emit_state(&app, &info);
            let _ = app.emit("term-exit", Exit { id: &id, code });
        }
        crate::applog!("workspace: {id} ended ({code:?})");
    });
    Ok(info)
}

fn read_pty(mut reader: platform::PtyReader, tx: mpsc::Sender<Vec<u8>>, stop: &AtomicBool) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match reader.read(&mut buf, stop) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        }
    }
}

/// The session's keystrokes, written in order, until the session ends.
fn write_pty(mut writer: Box<dyn Write + Send>, keys: mpsc::Receiver<Vec<u8>>) {
    for k in keys {
        if writer.write_all(&k).and_then(|_| writer.flush()).is_err() {
            break;
        }
    }
}

/// `bytes` added to the kept output; returns the offset they start at.
fn keep(tail: &Mutex<Tail>, bytes: &[u8]) -> u64 {
    let mut t = crate::lock(tail);
    let at = t.end;
    t.data.extend_from_slice(bytes);
    t.end += bytes.len() as u64;
    trim(&mut t.data);
    at
}

/// The kept output cut to its last `BUFFER_MAX` bytes, in steps rather than on every chunk, and at
/// the start of a line when one starts shortly before the cut.
fn trim(b: &mut Vec<u8>) {
    if b.len() <= BUFFER_MAX + BUFFER_MAX / 4 {
        return;
    }
    let cut = b.len() - BUFFER_MAX;
    let from = cut.saturating_sub(TRIM_LOOKBACK);
    let cut = b[from..cut].iter().rposition(|x| *x == b'\n').map_or(cut, |k| from + k + 1);
    b.drain(..cut);
}

/// Gathers output into `term-output` events and keeps the last `BUFFER_MAX` bytes.
fn send_output(app: &AppHandle, id: &str, rx: mpsc::Receiver<Vec<u8>>, tail: &Mutex<Tail>) {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut pending: Vec<u8> = Vec::new();
    let mut offset = 0u64;
    let mut since: Option<Instant> = None;
    let flush = |pending: &mut Vec<u8>, offset: u64| {
        if !pending.is_empty() {
            let _ = app.emit("term-output", Output { id, data: b64.encode(&pending), offset });
            pending.clear();
        }
    };
    loop {
        let wait = since.map_or(Duration::from_secs(3600), |s| FLUSH_AFTER.saturating_sub(s.elapsed()));
        match rx.recv_timeout(wait) {
            Ok(bytes) => {
                let at = keep(tail, &bytes);
                if pending.is_empty() {
                    offset = at;
                }
                pending.extend_from_slice(&bytes);
                since.get_or_insert_with(Instant::now);
                if pending.len() >= FLUSH_BYTES || since.is_some_and(|s| s.elapsed() >= FLUSH_AFTER) {
                    flush(&mut pending, offset);
                    since = None;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush(&mut pending, offset);
                since = None;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush(&mut pending, offset);
                return;
            }
        }
    }
}

/// Keystrokes from the terminal (xterm's onData), handed to the session's writer thread: never
/// waits on a session that isn't reading.
#[tauri::command]
pub fn term_write(terms: State<Arc<Terminals>>, id: String, data: String) -> Result<(), String> {
    terms.with(&id, |t| {
        let p = t.pty.as_ref().ok_or("That session has ended.")?;
        p.input.send(data.into_bytes()).map_err(|_| "That session has ended.".to_string())
    })
}

#[tauri::command]
pub fn term_resize(terms: State<Arc<Terminals>>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    terms.with(&id, |t| match t.pty.as_ref() {
        Some(p) => {
            p.master.resize(PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())
        }
        None => Ok(()),
    })
}

/// Asks the session's processes to end (SIGTERM to its process group, and to the terminal's
/// foreground job), and if any are still there after a few seconds ends them (SIGKILL) and hangs
/// up the terminal. Closing hides it at once and hangs up at once, since nothing will show its
/// output; it's forgotten when its process is seen to end.
fn stop(terms: Arc<Terminals>, id: &str, close: bool) -> Result<(), String> {
    let target = {
        let mut g = crate::lock(&terms.terms);
        let i = g.iter().position(|t| t.info.id == id).ok_or("That session isn't there any more.")?;
        let t = &mut g[i];
        if close && !t.running {
            g.remove(i);
            return Ok(());
        }
        t.closed |= close;
        let target = t.pid.filter(|_| t.running).map(|pid| (pid, t.pty.as_ref().and_then(|p| platform::pty_foreground(&*p.master, pid))));
        if close {
            t.hang_up();
        }
        target
    };
    let Some((pid, fg)) = target else { return Ok(()) };
    platform::stop_group(pid);
    fg.into_iter().for_each(platform::stop_group);
    let id = id.to_string();
    std::thread::spawn(move || {
        let lingering = || terms.running(&id) || platform::alive(pid, true);
        let until = Instant::now() + STOP_GRACE;
        while Instant::now() < until && lingering() {
            std::thread::sleep(Duration::from_millis(100));
        }
        if lingering() {
            crate::applog!("workspace: {id} didn't stop when asked; ending it");
            platform::kill_group(pid);
            fg.into_iter().for_each(platform::kill_group);
            let _ = terms.with(&id, |t| {
                t.hang_up();
                Ok(())
            });
        }
    });
    Ok(())
}

#[tauri::command]
pub fn term_stop(app: AppHandle, id: String) -> Result<(), String> {
    stop(app.state::<Arc<Terminals>>().inner().clone(), &id, false)
}

/// Stops it if it's running, and forgets it: hidden from the list at once, gone once it ends.
#[tauri::command]
pub fn term_close(app: AppHandle, id: String) -> Result<(), String> {
    stop(app.state::<Arc<Terminals>>().inner().clone(), &id, true)
}

pub fn sessions(terms: &Terminals) -> Vec<SessionInfo> {
    let now = now_ms();
    crate::lock(&terms.terms)
        .iter()
        .filter(|t| !t.closed && (!t.info.external || now - t.last_event < EXTERNAL_STALE_MS))
        .map(|t| t.info.clone())
        .collect()
}

#[tauri::command]
pub fn term_sessions(terms: State<Arc<Terminals>>) -> Vec<SessionInfo> {
    sessions(&terms)
}

#[tauri::command]
pub fn term_buffer(terms: State<Arc<Terminals>>, id: String) -> Result<OutputTail, String> {
    let tail = terms.with(&id, |t| Ok(t.tail.clone()))?;
    Ok(output_tail(&tail))
}

fn output_tail(tail: &Mutex<Tail>) -> OutputTail {
    let t = crate::lock(tail);
    OutputTail { data: base64::engine::general_purpose::STANDARD.encode(&t.data), end: t.end }
}

#[tauri::command]
pub fn term_seen(app: AppHandle, id: String) -> Result<(), String> {
    let info = app.state::<Arc<Terminals>>().with(&id, |t| {
        let was = t.info.unseen;
        t.info.unseen = false;
        Ok(was.then(|| t.info.clone()))
    })?;
    if let Some(i) = info {
        emit_state(&app, &i);
    }
    Ok(())
}

fn parse_program(program: &str) -> Result<Program, String> {
    serde_json::from_value(serde_json::Value::String(program.to_string())).map_err(|_| format!("“{program}” isn't claude, codex or shell."))
}

/// The line to paste into an outside terminal: into the folder, then the same guarded launch.
fn external_command(app: &AppHandle, folder: &str, resume: Option<&str>, program: &str) -> Result<(PathBuf, String), String> {
    let program = parse_program(program)?;
    let folder = check_folder(app, folder)?;
    if program == Program::Claude {
        crate::workspace::write_claude_settings(app);
    }
    let line = match launch_line(app, &folder, program, resume, None, None, None) {
        Some(l) => platform::in_folder(&folder, &l),
        None => platform::in_folder(&folder, &platform::command_line(&[shell(app).to_string_lossy().into_owned()])),
    };
    Ok((folder, line))
}

#[tauri::command]
pub fn term_external_command(app: AppHandle, folder: String, resume: Option<String>, program: String) -> Result<String, String> {
    external_command(&app, &folder, resume.as_deref(), &program).map(|(_, l)| l)
}

#[tauri::command]
pub fn term_open_external(app: AppHandle, folder: String, resume: Option<String>, program: String) -> Result<(), String> {
    let (folder, line) = external_command(&app, &folder, resume.as_deref(), &program)?;
    platform::open_terminal(&shell(&app), &folder, &line)
}

// --- the hooks' events ---------------------------------------------------------------------

/// Reads new lines of the events file twice a second, from where it ends now, and applies each
/// to its session. A long file is started afresh, at launch and while running.
pub fn start_events(app: &AppHandle) {
    let file = events_file();
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > EVENTS_MAX) {
        let _ = std::fs::rename(&file, old_events(&file));
    }
    let mut offset = std::fs::metadata(&file).map_or(0, |m| m.len());
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        let mut evs = read_events(&file, &mut offset);
        evs.extend(rotate_events(&file, &mut offset, EVENTS_MAX));
        for ev in evs {
            on_event(&app, &ev);
        }
    });
}

fn old_events(file: &Path) -> PathBuf {
    file.with_extension("old.jsonl")
}

/// The whole lines of the events file after `offset`, which moves past them; from the start
/// again when the file has shrunk (it was started afresh). A line still being written waits.
fn read_events(file: &Path, offset: &mut u64) -> Vec<HookEvent> {
    let len = std::fs::metadata(file).map_or(0, |m| m.len());
    if len < *offset {
        *offset = 0;
    }
    if len == *offset {
        return Vec::new();
    }
    let Ok(mut f) = std::fs::File::open(file) else { return Vec::new() };
    let mut bytes = Vec::new();
    if std::io::Seek::seek(&mut f, std::io::SeekFrom::Start(*offset)).is_err() || f.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return Vec::new() };
    *offset += end as u64 + 1;
    String::from_utf8_lossy(&bytes[..end]).lines().filter_map(ws::parse_event).collect()
}

/// Once the events file is past `max` and read to its end, it's moved aside and a new one starts
/// (the hooks create it); lines a hook added as it moved are read from the old one.
fn rotate_events(file: &Path, offset: &mut u64, max: u64) -> Vec<HookEvent> {
    let len = std::fs::metadata(file).map_or(0, |m| m.len());
    if len <= max || len != *offset {
        return Vec::new();
    }
    let old = old_events(file);
    if std::fs::rename(file, &old).is_err() {
        return Vec::new();
    }
    let late = read_events(&old, offset);
    *offset = 0;
    late
}

fn on_event(app: &AppHandle, ev: &HookEvent) {
    let terms = app.state::<Arc<Terminals>>();
    let mut created = false;
    let (info, alert) = {
        let mut g = crate::lock(&terms.terms);
        let found = match &ev.term {
            Some(t) => g.iter().position(|x| &x.info.id == t && !x.info.external),
            None => {
                let Some(sid) = &ev.session_id else { return };
                match g.iter().position(|x| x.info.external && &x.info.id == sid) {
                    Some(i) => Some(i),
                    None if ev.event == "SessionEnd" => None,
                    None => {
                        let folder = PathBuf::from(ev.cwd.clone().unwrap_or_default());
                        let mut info = SessionInfo::new(sid, &folder, Program::Claude, ev.t);
                        info.external = true;
                        info.project = crate::workspace::project_for(app, &folder);
                        g.push(Term {
                            info,
                            pty: None,
                            pid: None,
                            running: false,
                            closed: false,
                            tail: Default::default(),
                            stop_read: Default::default(),
                            last_event: ev.t,
                        });
                        created = true;
                        Some(g.len() - 1)
                    }
                }
            }
        };
        let Some(i) = found else { return };
        let t = &mut g[i];
        let before = t.info.clone();
        let alert = ws::apply(&mut t.info, ev);
        t.last_event = ev.t.max(t.last_event);
        if t.info == before && !created {
            return;
        }
        (t.info.clone(), alert)
    };
    emit_state(app, &info);
    if let Some(a) = alert {
        notify(app, &info, a, ev.message.as_deref());
    }
}

/// A system notification when a session needs the user and the window isn't in front.
fn notify(app: &AppHandle, info: &SessionInfo, alert: Alert, message: Option<&str>) {
    let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
    if focused || crate::scene().is_some() {
        return;
    }
    let body = match alert {
        Alert::Waiting => message.map(str::to_string).unwrap_or_else(|| "It's waiting for you.".into()),
        Alert::Finished => "It finished its turn.".into(),
    };
    let title = match &info.title {
        Some(t) => format!("{} · {t}", info.name),
        None => info.name.clone(),
    };
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_as_typed() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(folder_path("~"), home);
        assert_eq!(folder_path("~/repos/orbit-app/"), home.join("repos/orbit-app"));
        assert_eq!(folder_path(" /r/acme// "), PathBuf::from("/r/acme"));
        assert_eq!(folder_path("~acme"), PathBuf::from("~acme"));
    }

    #[test]
    fn programs_by_name() {
        assert_eq!(parse_program("codex").unwrap(), Program::Codex);
        assert!(parse_program("emacs").unwrap_err().contains("emacs"));
    }

    /// The pty and the login shell, end to end: output comes back, and the exit code with it.
    #[cfg(unix)]
    #[test]
    fn a_session_runs_in_a_pty() {
        let pair = portable_pty::native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let argv = platform::shell_argv(Path::new("/bin/sh"), Some("echo \"$BRAINSTEAD_TERM in $(pwd)\"; exit 3"));
        let mut cmd = CommandBuilder::from_argv(argv.iter().map(Into::into).collect());
        let dir = tempfile::tempdir().unwrap();
        cmd.cwd(dir.path());
        cmd.env("BRAINSTEAD_TERM", "term-1");
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let reader = platform::PtyReader::new(&*pair.master).unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || read_pty(reader, tx, &AtomicBool::new(false)));
        assert_eq!(child.wait().unwrap().exit_code(), 3);
        let mut out = Vec::new();
        while let Ok(b) = rx.recv_timeout(Duration::from_secs(2)) {
            out.extend(b);
            if String::from_utf8_lossy(&out).contains('\n') {
                break;
            }
        }
        let out = String::from_utf8_lossy(&out);
        assert!(out.contains("term-1 in "), "{out}");
    }

    /// Something the session started in the background holds the terminal open after the session
    /// has ended: the reader still stops when told, the hang-up and the group kill end it, and no
    /// thread is left waiting.
    #[cfg(unix)]
    #[test]
    fn a_session_ends_though_a_grandchild_holds_the_terminal() {
        let pair = portable_pty::native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let argv = platform::shell_argv(Path::new("/bin/sh"), Some("trap '' HUP; sleep 60 & echo started; exit 0"));
        let mut child = pair.slave.spawn_command(CommandBuilder::from_argv(argv.iter().map(Into::into).collect())).unwrap();
        drop(pair.slave);
        let pid = child.process_id().unwrap();
        let reader = platform::PtyReader::new(&*pair.master).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        {
            let stop = stop.clone();
            std::thread::spawn(move || {
                read_pty(reader, tx, &stop);
                let _ = done_tx.send(());
            });
        }
        assert_eq!(child.wait().unwrap().exit_code(), 0);
        // The sleep still has the terminal: reading hasn't ended by itself.
        assert!(done_rx.recv_timeout(Duration::from_millis(400)).is_err());
        assert!(platform::alive(pid, true), "the sleep is still in the session's group");
        stop.store(true, Ordering::SeqCst);
        done_rx.recv_timeout(Duration::from_secs(2)).expect("the reader stops when told");
        drop(pair.master);
        platform::kill_group(pid);
        let until = Instant::now() + Duration::from_secs(3);
        while platform::alive(pid, true) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!platform::alive(pid, true), "nothing is left in the group");
        let out: Vec<u8> = rx.try_iter().flatten().collect();
        assert!(String::from_utf8_lossy(&out).contains("started"));
    }

    /// Keystrokes go through the session's writer thread, in order, and come back from `cat`.
    #[cfg(unix)]
    #[test]
    fn keystrokes_go_through_a_writer_thread() {
        let pair = portable_pty::native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut child = pair.slave.spawn_command(CommandBuilder::from_argv(vec!["/bin/cat".into()])).unwrap();
        drop(pair.slave);
        let reader = platform::PtyReader::new(&*pair.master).unwrap();
        let (input, keys) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn({
            let w = pair.master.take_writer().unwrap();
            move || write_pty(w, keys)
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        {
            let stop = stop.clone();
            std::thread::spawn(move || read_pty(reader, tx, &stop));
        }
        for k in ["Orbit ", "App", "\n"] {
            input.send(k.as_bytes().to_vec()).unwrap();
        }
        let mut out = Vec::new();
        let until = Instant::now() + Duration::from_secs(3);
        while !String::from_utf8_lossy(&out).contains("Orbit App") && Instant::now() < until {
            if let Ok(b) = rx.recv_timeout(Duration::from_millis(100)) {
                out.extend(b);
            }
        }
        assert!(String::from_utf8_lossy(&out).contains("Orbit App"), "{}", String::from_utf8_lossy(&out));
        let _ = child.kill();
        let _ = child.wait();
        stop.store(true, Ordering::SeqCst);
    }

    #[test]
    fn output_carries_its_offset_and_the_buffer_where_it_ends() {
        let tail = Mutex::new(Tail::default());
        assert_eq!(keep(&tail, b"hello "), 0);
        assert_eq!(keep(&tail, b"world"), 6);
        let t = output_tail(&tail);
        assert_eq!(t.end, 11);
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(&t.data).unwrap(), b"hello world");
        assert_eq!(serde_json::to_value(&t).unwrap()["end"], 11);
        // Past the limit, it's cut to the start of a line, and the offsets keep counting.
        let line = [b'x'; 99].iter().chain(b"\n").copied().collect::<Vec<u8>>();
        let mut at = 11;
        for _ in 0..(BUFFER_MAX * 2 / 100) {
            assert_eq!(keep(&tail, &line), at);
            at += 100;
        }
        let g = crate::lock(&tail);
        assert_eq!(g.end, at);
        assert!(g.data.len() <= BUFFER_MAX + BUFFER_MAX / 4 && g.data.len() >= BUFFER_MAX);
        assert_eq!(g.data[0], b'x');
        assert_eq!(g.data.len() % 100, 0, "starts at a line");
    }

    #[test]
    fn a_trim_with_no_line_nearby_cuts_where_it_must() {
        let mut b = vec![b'x'; BUFFER_MAX + BUFFER_MAX / 4 + 1];
        trim(&mut b);
        assert_eq!(b.len(), BUFFER_MAX);
        let mut small = b"abc".to_vec();
        trim(&mut small);
        assert_eq!(small, b"abc");
    }

    #[test]
    fn the_events_file_is_followed_through_truncation_and_rotation() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("events.jsonl");
        let line = |e: &str| format!("{{\"t\":1,\"event\":\"{e}\",\"sessionId\":\"s1\"}}\n");
        let mut offset = 0;
        std::fs::write(&file, line("Stop") + &line("Notification") + "{\"t\":1,\"ev").unwrap();
        let evs: Vec<String> = read_events(&file, &mut offset).into_iter().map(|e| e.event).collect();
        assert_eq!(evs, ["Stop", "Notification"]);
        assert!(read_events(&file, &mut offset).is_empty());
        // Started afresh by someone else: shorter than where it was read to.
        std::fs::write(&file, line("SessionStart")).unwrap();
        let evs: Vec<String> = read_events(&file, &mut offset).into_iter().map(|e| e.event).collect();
        assert_eq!(evs, ["SessionStart"]);
        // Past the limit and read to the end: moved aside, and the next lines come from the new one.
        assert!(rotate_events(&file, &mut offset, 10).is_empty());
        assert_eq!(offset, 0);
        assert!(!file.exists() && old_events(&file).exists());
        std::fs::write(&file, line("Stop")).unwrap();
        assert_eq!(read_events(&file, &mut offset).len(), 1);
        // Not read to the end yet: left alone.
        std::fs::write(&file, line("Stop") + &line("Stop")).unwrap();
        let mut o = 3;
        assert!(rotate_events(&file, &mut o, 10).is_empty());
        assert!(file.exists());
    }
}
