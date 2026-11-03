// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ask: runs an AI CLI already installed and signed in on this computer (Claude Code, Codex,
//! Antigravity or GitHub Copilot, or Codex on a model Ollama serves) in the vault, read-only, and
//! streams its answer to the window
//! as `ask-event` and `ask-done` events keyed by chat id. The command lines and the stream reading
//! are in brainstead_core::ask; this side finds the programs, runs them and keeps the chats.
//!
//! Chats are saved by the app, never the model: as `Chat. <title>.md` in the vault, as the
//! previous app saves them (brainstead_core::chats), or, while Brainstead is read-only, in `chats/` in
//! the app data folder until a save can reach the vault.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use brainstead_core::ask::{self, Cli, Event, Outcome, Request};
use brainstead_core::chats::{self, Chat, Summary};
use brainstead_core::{trash, write};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::edits::{writable, written, Res};
use crate::platform;
use crate::vault::VaultService;
use crate::AppState;

/// The turns in flight and the CLIs answering them, by chat id (an Ask chat's, or a job run's),
/// under one lock. A Stop while a turn is in flight (for an Ask chat, from the moment `ask_send`
/// takes the message) is noted, and a CLI that registers after it (`admit`) is stopped at once.
#[derive(Default)]
pub struct Running(Mutex<Runs>);

#[derive(Default)]
struct Runs {
    /// Each CLI answering now, by chat id and process id.
    children: Vec<(String, u32)>,
    /// The chats with a turn in flight, counted: an Ask chat's from `ask_send` taking the message
    /// (`try_begin`), and each run's from before it starts (`fitted_turn`), until after its last, so
    /// also between a refused run and its retry, when no CLI is answering.
    turns: HashMap<String, usize>,
    /// The chats Stop was pressed on during the turn in flight: no further run of it starts, and
    /// a CLI it starts after the Stop is stopped at once. Forgotten when the turn ends.
    stopped: HashSet<String>,
}

impl Runs {
    /// Whether `chat` has a turn in flight or a CLI answering it.
    fn busy(&self, chat: &str) -> bool {
        self.turns.contains_key(chat) || self.children.iter().any(|(c, _)| c == chat)
    }
}

/// A turn in flight on a chat (`Running::begin`), ended when dropped: on every return, and on a
/// panic, so a chat never stays busy.
struct Turn {
    running: Arc<Running>,
    chat: String,
}

impl Drop for Turn {
    fn drop(&mut self) {
        self.running.end(&self.chat);
    }
}

impl Running {
    /// The state, also after a panic elsewhere held the lock (a `Turn` ends while unwinding).
    fn lock(&self) -> std::sync::MutexGuard<'_, Runs> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Starts a turn on `chat`: it reads busy, and a Stop on it is noted, until the `Turn` drops.
    /// Turns nest: the chat stays busy until the last one drops.
    fn begin(self: &Arc<Self>, chat: &str) -> Turn {
        *self.lock().turns.entry(chat.to_string()).or_default() += 1;
        Turn { running: self.clone(), chat: chat.to_string() }
    }
    /// `begin`, unless `chat` is busy (a turn in flight or a CLI answering it); checked and begun
    /// under one lock, so two sends to a chat can't both start a turn.
    fn try_begin(self: &Arc<Self>, chat: &str) -> Option<Turn> {
        let mut r = self.lock();
        if r.busy(chat) {
            return None;
        }
        *r.turns.entry(chat.to_string()).or_default() += 1;
        Some(Turn { running: self.clone(), chat: chat.to_string() })
    }
    fn end(&self, chat: &str) {
        let mut r = self.lock();
        let left = r.turns.get_mut(chat).map(|n| {
            *n -= 1;
            *n
        });
        if left == Some(0) {
            r.turns.remove(chat);
            r.stopped.remove(chat);
        }
    }
    /// Registers a CLI answering `chat`; true when Stop was already pressed on its turn, and the
    /// caller stops it at once.
    fn add(&self, chat: &str, pid: u32) -> bool {
        let mut r = self.lock();
        r.children.push((chat.to_string(), pid));
        r.stopped.contains(chat)
    }
    /// Registers the CLI `pid` answering `chat` (`add`), and stops it with `stop` (its process
    /// group) at once when Stop was already pressed on the turn.
    fn admit(&self, chat: &str, pid: u32, stop: impl FnOnce(u32)) {
        if self.add(chat, pid) {
            stop(pid);
        }
    }
    fn remove(&self, pid: u32) {
        self.lock().children.retain(|(_, p)| *p != pid);
    }
    /// Whether `chat` has a turn in flight or a CLI answering it (`try_begin` checks the same).
    #[cfg(test)]
    fn busy(&self, chat: &str) -> bool {
        self.lock().busy(chat)
    }
    pub fn cancel(&self, chat: &str) {
        for pid in self.stopping(chat) {
            platform::stop_group(pid);
        }
    }
    /// The processes answering `chat`. The chat is noted as stopped when it is busy, with a CLI
    /// answering or between runs of its turn.
    fn stopping(&self, chat: &str) -> Vec<u32> {
        let mut r = self.lock();
        let pids: Vec<u32> = r.children.iter().filter(|(c, _)| c == chat).map(|(_, p)| *p).collect();
        if !pids.is_empty() || r.turns.contains_key(chat) {
            r.stopped.insert(chat.to_string());
        }
        pids
    }
    /// Whether Stop was pressed on `chat` during its turn in flight.
    fn stopped(&self, chat: &str) -> bool {
        self.lock().stopped.contains(chat)
    }
    /// Whether any turn is in flight or any CLI is answering (an Ask chat, or a run's model call).
    pub fn any(&self) -> bool {
        let r = self.lock();
        !r.children.is_empty() || !r.turns.is_empty()
    }
    /// On quit: nothing is left running once the app has gone, and no turn starts another run.
    pub fn kill_all(&self) {
        let pids: Vec<u32> = {
            let mut r = self.lock();
            let chats: Vec<String> = r.turns.keys().cloned().collect();
            r.stopped.extend(chats);
            r.children.iter().map(|(_, p)| *p).collect()
        };
        for pid in pids {
            platform::stop_group(pid);
        }
    }
}

pub fn find(cli: Cli) -> Option<platform::Program> {
    platform::find_program(cli.bin(), cli.extra_paths())
}

#[derive(Serialize, Clone)]
pub struct Model {
    id: String,
    name: String,
    /// The model's own effort levels, where its assistant knows them per model (Ollama: the
    /// thinking levels a model names, none for one that only thinks or not); None where the
    /// assistant's list stands for every model.
    #[serde(skip_serializing_if = "Option::is_none")]
    efforts: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct CliInfo {
    cli: Cli,
    /// Where it is: the program, or for Ollama its server's address. None when it can't be used,
    /// and `note` says why.
    path: Option<String>,
    version: Option<String>,
    /// Codex's and Antigravity's come from the signed-in account; Claude's are its aliases;
    /// Ollama's are the models it has pulled that call tools.
    models: Vec<Model>,
    /// The effort levels Settings › AI assistants offers for it (`Cli::efforts`).
    efforts: &'static [&'static str],
    /// A line for its row under Found on this computer: for Ollama, what it runs through and the
    /// models left out, or why it can't be used (its server not answering, Codex missing, or
    /// `OLLAMA_HOST` naming another computer).
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    /// Ollama: `OLLAMA_HOST` names a server Brainstead doesn't use, and `note` says so. The row is
    /// neither found nor missing, so it offers no How to install.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    refused: bool,
}

fn version(bin: &platform::Program) -> Option<String> {
    let out = bin.command().arg("--version").stdin(Stdio::null()).output().ok()?;
    // The first line only: Copilot adds "Run 'copilot update' to check for updates."
    String::from_utf8_lossy(&out.stdout).lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.trim_end_matches('.').to_string())
}

fn models(cli: Cli, bin: &platform::Program) -> Vec<Model> {
    let m = |id: String, name: &str| Model { id, name: name.to_string(), efforts: None };
    match cli {
        Cli::Claude => [("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")]
            .into_iter()
            .map(|(a, n)| m(format!("{}{a}", ask::CLAUDE_PREFIX), n))
            .collect(),
        Cli::Codex => {
            let Some(v) = codex_models() else { return Vec::new() };
            let mut ms: Vec<(i64, Model)> = v["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|x| x["visibility"] == "list")
                .filter_map(|x| {
                    let id = x["slug"].as_str()?.to_string();
                    let name = x["display_name"].as_str().unwrap_or(&id).to_string();
                    Some((x["priority"].as_i64().unwrap_or(i64::MAX), Model { id, name, efforts: None }))
                })
                .collect();
            ms.sort_by_key(|(p, _)| *p);
            ms.into_iter().map(|(_, x)| x).collect()
        }
        Cli::Antigravity => {
            let Ok(out) = bin.command().arg("models").stdin(Stdio::null()).output() else { return Vec::new() };
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.split_once('\t'))
                .map(|(id, name)| m(format!("{}{}", ask::AGY_PREFIX, id.trim()), name.trim()))
                .collect()
        }
        // Auto takes no level (`model_efforts`), so the Effort menus are hidden for it.
        Cli::Copilot => vec![Model { efforts: model_efforts(ask::COPILOT_AUTO), ..m(ask::COPILOT_AUTO.to_string(), "Copilot (Auto)") }],
        // Ollama's come from its server, not a program (`ollama_info`).
        Cli::Ollama => Vec::new(),
    }
}

/// Ollama's row: usable when its server is on this computer and answers, and Codex, which runs its
/// models, is installed. The models are the ones pulled that call tools (Codex's agent loop needs
/// them), each with the effort levels it names; the note says what it runs through and counts the
/// models left out, or says what's missing, or why `OLLAMA_HOST` isn't used.
fn ollama_info() -> CliInfo {
    let mut info = CliInfo {
        cli: Cli::Ollama,
        path: None,
        version: None,
        models: Vec::new(),
        efforts: Cli::Ollama.efforts(),
        note: None,
        refused: false,
    };
    let host = match crate::ollama::host() {
        Ok(h) => h,
        Err(why) => {
            info.note = Some(why);
            info.refused = true;
            return info;
        }
    };
    let codex = find(Cli::Codex);
    let codex_version = codex.as_ref().and_then(version);
    let server = crate::ollama::server(&host);
    let Some((version, tags)) = server else {
        info.note = Some(match codex {
            Some(_) => format!("Ollama isn't running at {host}, or isn't installed. Start it (or install it), then click Look again."),
            None => format!(
                "Ollama isn't running at {host}, or isn't installed, and its models run through Codex, which isn't installed either."
            ),
        });
        return info;
    };
    info.version = version;
    if codex.is_none() {
        info.note = Some(format!("Ollama is running at {host}, but its models run through Codex, which isn't installed."));
        return info;
    }
    let mut without_tools = Vec::new();
    for t in tags {
        let show = crate::ollama::show(&host, &t.name).unwrap_or_default();
        if !show.tools {
            without_tools.push(t.name);
            continue;
        }
        let name = match &t.parameters {
            Some(p) => format!("{} ({p})", t.name),
            None => t.name.clone(),
        };
        info.models.push(Model { id: format!("{}{}", ask::OLLAMA_PREFIX, t.name), name, efforts: Some(show.efforts()) });
    }
    info.path = Some(host.to_string());
    // `codex --version` says "codex-cli 0.166.0"; the row already says Codex.
    let mut note =
        format!("Through Codex{}", codex_version.map(|v| format!(" {}", v.trim_start_matches("codex-cli "))).unwrap_or_default());
    if !info.models.is_empty() {
        note.push_str(
            ". For Ask and runs started by hand, never scheduled jobs; jobs do best on a model of about 14B or more (gpt-oss:20b, qwen3:14b), a small one misses things",
        );
    }
    if !without_tools.is_empty() {
        note.push_str(&format!(
            ". Not listed, as {} can't call tools, which Codex's agent loop needs: {}",
            if without_tools.len() == 1 { "it" } else { "they" },
            without_tools.join(", ")
        ));
    }
    info.note = Some(note);
    info
}

/// The list Codex caches for the signed-in account; None until Codex has run once.
fn codex_models() -> Option<serde_json::Value> {
    let home = dirs::home_dir().unwrap_or_default();
    serde_json::from_str(&std::fs::read_to_string(home.join(".codex/models_cache.json")).ok()?).ok()
}

/// The effort levels `model` takes, where they are known per model: Codex's cache lists them
/// (`supported_reasoning_levels`), Ollama's server says which levels a model names (none, so no
/// level, for one that only thinks or not), and Copilot's Auto takes none (`ask::COPILOT_AUTO`).
/// None for the others, and for a model the cache or the server doesn't have.
fn model_efforts(model: &str) -> Option<Vec<String>> {
    if model == ask::COPILOT_AUTO {
        return Some(Vec::new());
    }
    if Cli::route(model) == Cli::Ollama {
        return crate::ollama::show(&crate::ollama::host().ok()?, model.strip_prefix(ask::OLLAMA_PREFIX)?).map(|s| s.efforts());
    }
    if Cli::route(model) != Cli::Codex {
        return None;
    }
    let v = codex_models()?;
    let m = v["models"].as_array()?.iter().find(|x| x["slug"] == model)?;
    let levels = m["supported_reasoning_levels"].as_array()?;
    Some(levels.iter().filter_map(|l| l["effort"].as_str().or(l.as_str()).map(str::to_string)).collect())
}

/// Which CLIs are installed, their versions and models. Slow (it runs each one), so off the main thread.
#[tauri::command]
pub async fn ask_clis() -> Vec<CliInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        [Cli::Claude, Cli::Codex, Cli::Antigravity, Cli::Copilot]
            .into_iter()
            .map(|cli| {
                let bin = find(cli);
                CliInfo {
                    cli,
                    efforts: cli.efforts(),
                    version: bin.as_ref().and_then(version),
                    models: bin.as_ref().map(|b| models(cli, b)).unwrap_or_default(),
                    path: bin.map(|p| p.path.to_string_lossy().into_owned()),
                    note: None,
                    refused: false,
                }
            })
            .chain(std::iter::once(ollama_info()))
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct EventMsg<'a> {
    chat_id: &'a str,
    event: &'a Event,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DoneMsg {
    pub chat_id: String,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// The effort `job` runs at on `model`: the one set for it (`set_effort`), as the model takes it
/// (`fitted`). An Ask chat runs at its own instead, which starts as the assistant's, fitted alike.
pub fn job_effort(app: &AppHandle, job: ask::Job, model: &str) -> Option<String> {
    fitted(model, set_effort(app, job, model).as_deref())
}

/// The effort set for `job` on `model` (`ask::job_effort`): its own from Settings › AI assistants ›
/// Models by job (`jobEffort.<job>`), else the assistant's (`effort.<cli>`), else none.
fn set_effort(app: &AppHandle, job: ask::Job, model: &str) -> Option<String> {
    let st = app.state::<crate::AppState>();
    let s = st.settings.lock().unwrap();
    ask::job_effort(&s.ui, job.key(), model).map(str::to_string)
}

/// `level` as `model` takes it: a level the model doesn't list (Codex's models cache says which it
/// takes) runs at the highest below it that it has (`ask::fit_effort`), so an assistant-wide level
/// can't fail every chat and scheduled job on that model.
fn fitted(model: &str, level: Option<&str>) -> Option<String> {
    level.and_then(|e| ask::fit_effort(Cli::route(model), e, model_efforts(model).as_deref())).map(str::to_string)
}

/// The model a run of `job` uses (`ask::job_run_model`): its own from Settings › AI assistants
/// (`jobModels`), else, for the weekly review's preparation, the summaries', else the default for
/// new chats (`askModel`), else Claude's Sonnet. A `scheduled` run (the scheduler started it, or an
/// assistant nobody is watching passed `unattended`) never goes on an Ollama model, which runs only
/// in Ask and runs started by hand: it goes on `ask::scheduled_fallback` instead, and the log says so.
pub fn job_model(app: &AppHandle, job: &str, scheduled: bool) -> String {
    let st = app.state::<crate::AppState>();
    let s = st.settings.lock().unwrap();
    let model = ask::job_run_model(&s.ui, job, scheduled);
    let set = ask::job_model(&s.ui, job);
    if model != set {
        crate::applog!(
            "ask: {job} is a scheduled run and Ollama runs only in Ask and runs started by hand, so it runs on {model} instead of {set}"
        );
    }
    model
}

pub fn today() -> String {
    chrono::Local::now().format("%A %-d %B %Y").to_string()
}

/// The app's help as files for Antigravity to read, with an index; rewritten only when changed.
fn write_agy_help(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let index = brainstead_core::help::list();
    let files = brainstead_core::help::files().iter().map(|(n, t)| (n.to_string(), t.to_string()));
    for (name, text) in std::iter::once(("index.md".to_string(), index)).chain(files) {
        let p = dir.join(&name);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&p, text)?;
        }
    }
    Ok(())
}

/// Runs one turn and waits for it, calling `on` with each event (the window gets them too).
/// How a chat's CLI starts Brainstead's MCP server: this program with `--mcp`, told the data
/// folder, the chat and its model, so its proposals say where they came from.
/// Each Ask chat's latest turn: None while it's answering, then how it ended. A window that missed
/// the turn's `ask-done` (once one stayed on Writing… after the CLI had finished) picks the answer up
/// from here with `ask_outcome`.
#[derive(Default)]
pub struct Turns(Mutex<HashMap<String, Option<DoneMsg>>>);

impl Turns {
    fn start(&self, chat: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(chat.to_string(), None);
    }
    fn finish(&self, done: &DoneMsg) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(done.chat_id.clone(), Some(done.clone()));
    }
    fn get(&self, chat: &str) -> TurnState {
        match self.0.lock().unwrap_or_else(|e| e.into_inner()).get(chat) {
            Some(None) => TurnState { running: true, done: None },
            Some(Some(d)) => TurnState { running: false, done: Some(d.clone()) },
            None => TurnState { running: false, done: None },
        }
    }
}

/// A chat's latest turn for the window: answering, or how it ended (neither when Brainstead has no
/// record of one).
#[derive(Serialize)]
pub struct TurnState {
    running: bool,
    done: Option<DoneMsg>,
}

pub fn mcp_server(chat_id: &str, model: &str) -> Option<ask::Mcp> {
    let exe = std::env::current_exe().ok()?;
    let data = platform::data_dir();
    Some(ask::Mcp {
        command: exe.to_string_lossy().into_owned(),
        args: ["--mcp", "--data", &data.to_string_lossy(), "--chat", chat_id, "--model", model].map(str::to_string).to_vec(),
    })
}

/// Runs one turn of `job` and waits for it, calling `on` with each event (the window gets them
/// too). With `mcp`, a CLI that takes MCP servers gets Brainstead's.
#[allow(clippy::too_many_arguments)]
pub fn run(
    app: &AppHandle,
    job: ask::Job,
    chat_id: &str,
    prompt: &str,
    model: &str,
    session: Option<&str>,
    system: &str,
    mcp: Option<&ask::Mcp>,
    on: &mut dyn FnMut(&Event),
) -> Outcome {
    run_with_images(app, job, chat_id, prompt, model, session, system, mcp, &[], on)
}

/// `run`, showing the model these images when its CLI sees images (`Cli::sees_images`).
#[allow(clippy::too_many_arguments)]
pub fn run_with_images(
    app: &AppHandle,
    job: ask::Job,
    chat_id: &str,
    prompt: &str,
    model: &str,
    session: Option<&str>,
    system: &str,
    mcp: Option<&ask::Mcp>,
    images: &[PathBuf],
    on: &mut dyn FnMut(&Event),
) -> Outcome {
    let set = set_effort(app, job, model);
    run_fitted(app, job.key(), chat_id, prompt, model, session, system, mcp, images, set.as_deref(), on)
}

/// Runs a turn of `what` (a job's key, or "chat") at `set` as the model takes it (`fitted`), for a
/// background job and an Ask chat alike.
#[allow(clippy::too_many_arguments)]
fn run_fitted(
    app: &AppHandle,
    what: &str,
    chat_id: &str,
    prompt: &str,
    model: &str,
    session: Option<&str>,
    system: &str,
    mcp: Option<&ask::Mcp>,
    images: &[PathBuf],
    set: Option<&str>,
    on: &mut dyn FnMut(&Event),
) -> Outcome {
    let running = app.state::<Arc<Running>>().inner().clone();
    fitted_turn(
        &running,
        what,
        chat_id,
        model,
        set,
        || fitted(model, set),
        |effort| run_at(app, chat_id, prompt, model, session, system, mcp, images, effort, &mut *on),
    )
}

/// `run_fitted`'s turn on `chat_id`: `run` at the level `fit` gives, and once more without one when
/// that level is refused. The chat reads busy, and a Stop on it is noted, for the whole turn, so a
/// Stop between the two runs means the second never starts.
fn fitted_turn(
    running: &Arc<Running>,
    what: &str,
    chat_id: &str,
    model: &str,
    set: Option<&str>,
    fit: impl FnOnce() -> Option<String>,
    mut run: impl FnMut(Option<&str>) -> Outcome,
) -> Outcome {
    let _turn = running.begin(chat_id);
    let effort = fit();
    if set != effort.as_deref() {
        crate::applog!(
            "ask: {what} on {model}: the model doesn't take {} effort, so it runs at {}",
            set.unwrap_or_default(),
            effort.as_deref().unwrap_or("its default")
        );
    }
    let out = run(effort.as_deref());
    // A level the CLI or model refused (one the models cache doesn't cover, or an older CLI without
    // the option): once more at the model's own default, rather than failing every time. A run the
    // user stopped isn't started again, whatever its error says.
    match effort {
        Some(e) if ask::effort_refused(&out) && !running.stopped(chat_id) => {
            crate::applog!("ask: {what} on {model} refused {e} effort, so it ran again without one: {}", out.error.unwrap_or_default());
            run(None)
        }
        _ => out,
    }
}

/// One run of the CLI at exactly `effort` (None: the CLI's own default).
#[allow(clippy::too_many_arguments)]
fn run_at(
    app: &AppHandle,
    chat_id: &str,
    prompt: &str,
    model: &str,
    session: Option<&str>,
    system: &str,
    mcp: Option<&ask::Mcp>,
    images: &[PathBuf],
    effort: Option<&str>,
    on: &mut dyn FnMut(&Event),
) -> Outcome {
    let fail = |e: String| Outcome { error: Some(e), ..Default::default() };
    let cli = Cli::route(model);
    // Only a server on this computer: a refused OLLAMA_HOST fails the run before Codex starts.
    let ollama_host = match cli {
        Cli::Ollama => match crate::ollama::host() {
            Ok(h) => Some(h),
            Err(refused) => return fail(refused),
        },
        _ => None,
    };
    let Some(bin) = find(cli) else {
        return fail(format!("{} isn't installed, or couldn't be found. Install it and sign in, then try again.", cli.label()));
    };
    let Some(vault) = app.state::<VaultService>().root() else { return fail("No vault is open.".into()) };
    let vault_s = vault.to_string_lossy().into_owned();
    let mcp = mcp.filter(|_| cli.takes_mcp());
    let req = Request { prompt, model, session, system, vault: &vault_s, mcp, images, effort, ollama_host: ollama_host.as_ref() };
    let launch = match ask::args(cli, &req) {
        Ok(l) => l,
        Err(e) => return fail(e),
    };
    // Antigravity runs in a folder of its own holding its read-only agent, and adds the vault;
    // the others run in the vault, so Claude Code reads its CLAUDE.md and skills.
    let cwd = if cli == Cli::Antigravity {
        let d = platform::data_dir().join("ask").join("agy");
        let agent = d.join(".agents").join("agents");
        if let Err(e) = std::fs::create_dir_all(&agent)
            .and_then(|_| std::fs::write(agent.join(format!("{}.md", ask::AGY_AGENT)), ask::agy_agent(system)))
            .and_then(|_| write_agy_help(&d.join(ask::AGY_HELP_DIR)))
        {
            return fail(format!("Couldn't set up Antigravity: {e}"));
        }
        d
    } else {
        vault.clone()
    };
    let mut cmd = bin.command();
    let stdin = if launch.stdin.is_some() { Stdio::piped() } else { Stdio::null() };
    cmd.current_dir(&cwd)
        .args(&launch.args)
        .envs(launch.env.iter().map(|(k, v)| (k, v)))
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match platform::spawn_group(&mut cmd) {
        Ok(c) => c,
        // Through cmd.exe (a `.cmd` that isn't an npm shim it can read), std refuses an argument
        // with a line break as invalid input.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput && bin.through_cmd() => {
            return fail(format!(
                "Couldn't start {}: it's installed as a script ({}) that can't be given text with line breaks. Install its own program instead (for Claude Code, its native installer), then try again.",
                cli.label(),
                bin.path.display()
            ))
        }
        Err(e) => return fail(format!("Couldn't start {}: {e}", cli.label())),
    };
    let fed = feed_stdin(&mut child, launch.stdin);
    let running = app.state::<Arc<Running>>().inner().clone();
    // A Stop pressed during this turn before the CLI was registered stops it now.
    running.admit(chat_id, child.id(), platform::stop_group);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    // Read on a thread from the start: a full pipe would stall the CLI.
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = std::io::Read::read_to_string(&mut e, &mut s);
        }
        s
    });
    // A run that hangs is stopped, so a background job (the daily check, contradictions, an ingest) never
    // stays "running".
    let pid = child.id();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watchdog = {
        let timed_out = timed_out.clone();
        std::thread::spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = done_rx.recv_timeout(RUN_TIMEOUT) {
                timed_out.store(true, std::sync::atomic::Ordering::SeqCst);
                platform::stop_group(pid);
            }
        })
    };
    let mut parser = ask::Parser::new(cli);
    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            for ev in parser.feed(&line) {
                on(&ev);
                let _ = app.emit("ask-event", EventMsg { chat_id, event: &ev });
            }
        }
    }
    let ok = child.wait().is_ok_and(|s| s.success());
    let _ = fed.map(|t| t.join());
    let _ = done_tx.send(());
    let _ = watchdog.join();
    running.remove(pid);
    let mut outcome = parser.finish(ok, &err.join().unwrap_or_default());
    if timed_out.load(std::sync::atomic::Ordering::SeqCst) {
        outcome.error = Some(format!("{} took longer than {} minutes, so Brainstead stopped it.", cli.label(), RUN_TIMEOUT.as_secs() / 60));
    }
    outcome
}

/// Writes `input` to the child's standard input on a thread of its own, then closes it so the CLI
/// sees the end of its prompt. On a thread because a CLI may write a pipe full of output before
/// it has read all its input, and the caller is the one reading that output.
fn feed_stdin(child: &mut std::process::Child, input: Option<String>) -> Option<std::thread::JoinHandle<()>> {
    let (mut pipe, input) = (child.stdin.take()?, input?);
    Some(std::thread::spawn(move || {
        let _ = std::io::Write::write_all(&mut pipe, input.as_bytes());
    }))
}

/// The longest a model run may take before it's stopped.
const RUN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Asks in the background; the answer comes as events. `effort` is the chat's own (None: the CLI's
/// own default), whatever is set for the CLI in Settings, fitted to the model as a job's is.
#[tauri::command]
pub fn ask_send(
    app: AppHandle,
    chat_id: String,
    prompt: String,
    model: String,
    session: Option<String>,
    effort: Option<String>,
) -> Result<(), String> {
    // The turn starts here, before the thread does: a Stop pressed from now on is noted, and the
    // run it would start is stopped at once (`Running::admit`).
    let Some(turn) = app.state::<Arc<Running>>().try_begin(&chat_id) else {
        return Err("This chat is still answering.".into());
    };
    let turns = app.state::<Arc<Turns>>().inner().clone();
    turns.start(&chat_id);
    std::thread::spawn(move || {
        // Whatever happens, the turn ends: a panic is an error in the chat (and in the log), never
        // a chat left answering.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mcp = mcp_server(&chat_id, &model);
            let propose = mcp.is_some() && Cli::route(&model).takes_mcp();
            let cli = Cli::route(&model);
            let mut system = ask::system_prompt(&today(), propose);
            if let Some(h) = ask::help_hint(cli, mcp.is_some() && cli.takes_mcp()) {
                system = format!("{system}\n{h}");
            }
            let prompt = brainstead_core::workflows::expand(&prompt).unwrap_or(prompt);
            run_fitted(
                &app,
                "chat",
                &chat_id,
                &prompt,
                &model,
                session.as_deref(),
                &system,
                mcp.as_ref(),
                &[],
                effort.as_deref(),
                &mut |_| {},
            )
        }))
        .unwrap_or_else(|_| Outcome {
            error: Some("Brainstead stopped this answer because something went wrong; its log has the details (Settings › About).".into()),
            ..Default::default()
        });
        // The turn ends before the window hears of it, so the chat takes its next message at once.
        drop(turn);
        let done = DoneMsg { chat_id, outcome };
        turns.finish(&done);
        let how = done.outcome.error.as_deref().map(|e| format!(" with an error: {e}")).unwrap_or_default();
        match app.emit("ask-done", &done) {
            Ok(()) => crate::applog!("ask: chat {} answered{how}", done.chat_id),
            Err(e) => crate::applog!("ask: chat {} answered{how}, but the window couldn't be told: {e}", done.chat_id),
        }
    });
    Ok(())
}

/// How a chat's latest turn stands, for a window that may have missed its `ask-done`.
#[tauri::command]
pub fn ask_outcome(turns: State<Arc<Turns>>, chat_id: String) -> TurnState {
    turns.get(&chat_id)
}

/// A line from the window's Ask into the app's log.
#[tauri::command]
pub fn ask_log(line: String) {
    crate::applog!("ask: {line}");
}

#[tauri::command]
pub fn ask_cancel(running: State<Arc<Running>>, chat_id: String) {
    running.cancel(&chat_id);
}

/// A short title for a chat from its first exchange, by Claude's smallest model when Claude Code
/// is installed (as the previous app names them); None otherwise, and the window uses the question.
#[tauri::command]
pub async fn chat_title(user: String, assistant: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bin = find(Cli::Claude)?;
        let cut = |s: &str| s.chars().take(2000).collect::<String>();
        let prompt = format!(
            "Name this conversation in 3 to 6 words, plain title case, no quotes, no punctuation at the end. Reply with the title only.\n\nUser: {}\n\nAssistant: {}",
            cut(&user),
            cut(&assistant)
        );
        // The prompt goes on standard input, as Ask's (`-p` without a prompt reads it there).
        let mut cmd = bin.command();
        cmd.current_dir(platform::data_dir())
            .args(["-p", "--model", "haiku", "--tools", "", "--strict-mcp-config", "--permission-mode", "default"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().ok()?;
        let fed = feed_stdin(&mut child, Some(prompt));
        let out = child.wait_with_output().ok();
        let _ = fed.map(|t| t.join());
        let out = out.filter(|o| o.status.success())?;
        let t = String::from_utf8_lossy(&out.stdout).lines().map(str::trim).find(|l| !l.is_empty())?.trim_matches(['"', '\'', '.']).to_string();
        let t = chats::clean(&t);
        (!t.is_empty() && t.chars().count() <= 80).then_some(t)
    })
    .await
    .ok()
    .flatten()
}

/// A suggestion for the user's next message in a chat, by the model set for next-turn suggestions
/// (Settings › AI assistants): `turns` are the chat's exchanges, oldest first. None when it's
/// switched off, the assistant isn't there, or the reply isn't usable.
#[tauri::command]
pub async fn ask_next(app: AppHandle, title: String, turns: Vec<(String, String)>) -> Option<String> {
    let on = app.state::<crate::AppState>().settings.lock().unwrap().ui.get("askSuggest").and_then(|v| v.as_bool()).unwrap_or(true);
    if !on || turns.is_empty() {
        return None;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let model = job_model(&app, "suggest", false);
        let id = format!("next-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        let out = run(&app, ask::Job::Suggest, &id, &ask::next_prompt(&title, &turns), &model, None, ask::NEXT_SYSTEM, None, &mut |_| {});
        if out.error.is_some() {
            return None;
        }
        ask::parse_next(&out.text)
    })
    .await
    .ok()
    .flatten()
}

// --- saved chats ---------------------------------------------------------------------------

/// Where chats wait while the vault is read-only.
fn local_dir() -> PathBuf {
    platform::data_dir().join("chats")
}

/// A chat file's place: in the vault, or (prefixed `local:`) in the app data folder.
const LOCAL: &str = "local:";

/// A saved chat's latest turns while the vault couldn't be written (read-only), under its vault
/// name: read in place of the note, and written to the vault by its next save once it can be.
fn pending_dir() -> PathBuf {
    platform::data_dir().join("chats-pending")
}

fn iso(t: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn now_iso() -> String {
    iso(std::time::SystemTime::now())
}

fn read_dir_chats(dir: &Path, prefix: &str) -> Vec<Chat> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !chats::is_chat_file(&name) {
                return None;
            }
            let text = std::fs::read_to_string(e.path()).ok()?;
            let mtime = e.metadata().and_then(|m| m.modified()).map(iso).unwrap_or_default();
            Some(chats::parse(&format!("{prefix}{name}"), &text, &mtime))
        })
        .collect()
}

fn all_chats(app: &AppHandle) -> Vec<Chat> {
    let mut all = app.state::<VaultService>().root().map(|r| read_dir_chats(&r, "")).unwrap_or_default();
    // A saved chat's turns waiting for the vault to be writable again are what's shown.
    for p in read_dir_chats(&pending_dir(), "") {
        if let Some(v) = all.iter_mut().find(|v| v.summary.filename == p.summary.filename) {
            *v = p;
        }
    }
    // A local chat already saved to the vault (same id) isn't listed twice.
    for c in read_dir_chats(&local_dir(), LOCAL) {
        if !all.iter().any(|v| v.summary.id == c.summary.id) {
            all.push(c);
        }
    }
    all.sort_by(|a, b| b.summary.updated_at.cmp(&a.summary.updated_at));
    all
}

#[tauri::command]
pub async fn chats_list(app: AppHandle) -> Vec<Summary> {
    tauri::async_runtime::spawn_blocking(move || all_chats(&app).into_iter().map(|c| c.summary).collect()).await.unwrap_or_default()
}

fn abs_of(app: &AppHandle, filename: &str) -> Result<PathBuf, String> {
    match filename.strip_prefix(LOCAL) {
        Some(n) if chats::is_chat_file(n) => Ok(local_dir().join(n)),
        Some(_) => Err("That isn't a chat.".into()),
        None if chats::is_chat_file(filename) => app.state::<VaultService>().resolve_path(filename),
        None => Err("That isn't a chat.".into()),
    }
}

#[tauri::command]
pub fn chat_read(app: AppHandle, filename: String) -> Result<Chat, String> {
    let abs = abs_of(&app, &filename)?;
    let pending = pending_dir().join(&filename);
    let abs = if !filename.starts_with(LOCAL) && pending.exists() { pending } else { abs };
    let text = std::fs::read_to_string(&abs).map_err(|_| format!("{} isn't there any more.", chats::title_of(&filename)))?;
    let mtime = std::fs::metadata(&abs).and_then(|m| m.modified()).map(iso).unwrap_or_default();
    Ok(chats::parse(&filename, &text, &mtime))
}

/// Writes a chat. It stays in the app data folder until it's saved (`keep`, Ask's Save): then it
/// goes to the vault as a note, and later writes keep it there. While the vault can't be written
/// a saved chat's turns wait in the app data folder under its vault name, so it stays saved, and
/// the note is left as it was until the next save that can write it. A new chat (no file name)
/// gets one from its title. Returns its summary as written.
#[tauri::command]
pub fn chat_save(app: AppHandle, chat: Chat, keep: Option<bool>) -> Res<Summary> {
    save_chat(&app, chat, keep.unwrap_or(false))
}

pub fn save_chat(app: &AppHandle, chat: Chat, keep: bool) -> Res<Summary> {
    let app = app.clone();
    let mut c = chat;
    let root = app.state::<VaultService>().root();
    let old = (!c.summary.filename.is_empty()).then(|| c.summary.filename.clone());
    let saved = old.as_deref().is_some_and(|f| !f.starts_with(LOCAL));
    if keep {
        writable(&app.state::<AppState>())?;
        if root.is_none() {
            return Err("No vault is open.".to_string().into());
        }
    }
    let vault_ok = writable(&app.state::<AppState>()).is_ok();
    if let (Some(name), true, false, Some(_)) = (old.as_deref(), saved, vault_ok, &root) {
        return keep_pending(&abs_of(&app, name)?, c, name);
    }
    let (dir, prefix) = match (&root, vault_ok && (keep || saved)) {
        (Some(r), true) => (r.clone(), ""),
        _ => (local_dir(), LOCAL),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let old_abs = old.as_deref().map(|f| abs_of(&app, f)).transpose()?;
    // Kept in place when it's already where it belongs; a saved chat moves to the vault.
    let name = match &old {
        Some(f) if f.starts_with(LOCAL) == !prefix.is_empty() => f.strip_prefix(LOCAL).unwrap_or(f).to_string(),
        _ => chats::filename_for(&c.summary.title, |n| dir.join(n).exists()),
    };
    if let Some(prev) = old_abs.as_ref().filter(|p| p.exists()) {
        if let Ok(text) = std::fs::read_to_string(prev) {
            let was = chats::parse_summary("", &text, "");
            c.summary.created_at = was.created_at;
        }
    }
    if c.summary.created_at.is_empty() {
        c.summary.created_at = now_iso();
    }
    c.summary.updated_at = now_iso();
    c.summary.filename = format!("{prefix}{name}");
    c.summary.title = chats::title_of(&name);
    let abs = dir.join(&name);
    if prefix.is_empty() {
        write::check_conflict_copies(&abs)?;
    }
    write::write_atomic(&abs, chats::serialize(&c).as_bytes(), false)?;
    // A local copy moved to the vault goes; a note in the vault is never removed from here.
    if let Some(prev) = old_abs.filter(|p| *p != abs && p.starts_with(local_dir())) {
        let _ = std::fs::remove_file(prev);
    }
    if prefix.is_empty() {
        let _ = std::fs::remove_file(pending_dir().join(&name));
        written(&app, &[abs]);
    } else if c.summary.state != "pinned" {
        // Unsaved chats beyond twenty, other than the open tabs, are deleted: they were never in
        // the vault. Saved chats are never cleared.
        let open: Vec<String> = crate::lock(&app.state::<AppState>().settings)
            .ui
            .get("askTabs")
            .and_then(|t| t.as_array())
            .map(|t| t.iter().filter_map(|x| x["id"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        let local: Vec<Summary> = read_dir_chats(&local_dir(), LOCAL)
            .into_iter()
            .map(|c| c.summary)
            .filter(|s| s.id != c.summary.id && !open.contains(&s.id))
            .collect();
        for f in chats::beyond_retention(&local) {
            if let Some(n) = f.strip_prefix(LOCAL) {
                let _ = std::fs::remove_file(local_dir().join(n));
            }
        }
    }
    Ok(c.summary)
}

/// A saved chat written while the vault is read-only: its turns kept under its vault name until the
/// vault can be written, so it stays saved (no `local:` copy, no "Title 2" later).
fn keep_pending(note: &Path, mut c: Chat, name: &str) -> Res<Summary> {
    let dir = pending_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let abs = dir.join(name);
    let prev = if abs.exists() { abs.clone() } else { note.to_path_buf() };
    if let Ok(text) = std::fs::read_to_string(&prev) {
        c.summary.created_at = chats::parse_summary("", &text, "").created_at;
    }
    if c.summary.created_at.is_empty() {
        c.summary.created_at = now_iso();
    }
    c.summary.updated_at = now_iso();
    c.summary.filename = name.to_string();
    c.summary.title = chats::title_of(name);
    write::write_atomic(&abs, chats::serialize(&c).as_bytes(), false)?;
    Ok(c.summary)
}

/// Renames a chat; in the vault through the rename that rewrites links to it. Returns the new file name.
#[tauri::command]
pub fn chat_rename(app: AppHandle, filename: String, title: String) -> Res<String> {
    let to = chats::filename_for(&title, |_| false);
    if let Some(n) = filename.strip_prefix(LOCAL) {
        let dir = local_dir();
        let to = chats::filename_for(&title, |x| x != n && dir.join(x).exists());
        std::fs::rename(dir.join(n), dir.join(&to)).map_err(|e| e.to_string())?;
        return Ok(format!("{LOCAL}{to}"));
    }
    writable(&app.state::<AppState>())?;
    let svc = app.state::<VaultService>();
    let root = svc.root().ok_or("No vault is open.".to_string())?;
    if to != filename && root.join(&to).exists() {
        return Err(format!("There's already a chat called “{}”.", chats::title_of(&to)).into());
    }
    crate::notes::with_links(&svc, &filename, |linkers, resolves| {
        let plan = brainstead_core::rename::plan(&root, &filename, &to, linkers, resolves)?;
        Ok(brainstead_core::rename::commit(&root, &plan, linkers, resolves)?)
    })?;
    // Turns still waiting for the vault go with it.
    let pending = pending_dir().join(&filename);
    if pending.exists() {
        let _ = std::fs::rename(&pending, pending_dir().join(&to));
    }
    written(&app, &[root.join(&filename), root.join(&to)]);
    Ok(to)
}

/// Moves a chat to the vault's trash (a local one is deleted).
#[tauri::command]
pub fn chat_trash(app: AppHandle, filename: String) -> Res<()> {
    if let Some(n) = filename.strip_prefix(LOCAL) {
        return Ok(std::fs::remove_file(local_dir().join(n)).map_err(|e| e.to_string())?);
    }
    writable(&app.state::<AppState>())?;
    let root = app.state::<VaultService>().root().ok_or("No vault is open.".to_string())?;
    trash::move_to_trash(&root, &filename, "note")?;
    let _ = std::fs::remove_file(pending_dir().join(&filename));
    written(&app, &[root.join(&filename)]);
    Ok(())
}

#[derive(Serialize)]
pub struct Skill {
    name: String,
    description: String,
    /// "brainstead" for the app's own workflows (any CLI), "vault" for the vault's skills (Claude Code).
    source: &'static str,
    /// A vault skill Brainstead now does on a screen of its own: the screen.
    #[serde(skip_serializing_if = "Option::is_none")]
    opens: Option<&'static str>,
}

/// The vault's skills that Brainstead has taken over (stage 7b), and the screen that does each.
pub const HANDED_OVER: [(&str, &str); 9] = [
    ("ingest", "sources"),
    ("triage-bookmarks", "triage"),
    ("draft-reply", "reply"),
    ("doc-check", "doccheck"),
    ("fix-name", "fixname"),
    ("find-contradictions", "contradictions"),
    ("lint-wiki", "health"),
    ("new-meeting-note", "meeting"),
    ("multi-new-meeting-notes", "meeting"),
];

/// The vault's Claude Code skills (`.claude/skills/<name>/SKILL.md`), for the `/` picker, by name.
#[tauri::command]
pub fn skills_list(svc: State<VaultService>) -> Vec<Skill> {
    let ours = brainstead_core::workflows::WORKFLOWS.iter().map(|(n, d, _)| Skill {
        name: n.to_string(),
        description: d.to_string(),
        source: "brainstead",
        opens: None,
    });
    let Some(root) = svc.root() else { return ours.collect() };
    let mut out: Vec<Skill> = std::fs::read_dir(root.join(".claude").join("skills"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path().join("SKILL.md")).ok()?;
            let d = brainstead_core::frontmatter::split(&text).data;
            let name = d["name"].as_str().map(str::to_string).unwrap_or_else(|| e.file_name().to_string_lossy().into_owned());
            // The first sentence is enough for a one-line list.
            let full = d["description"].as_str().unwrap_or("").trim();
            let description = full.split_inclusive(". ").next().unwrap_or(full).trim().trim_end_matches('.').to_string();
            let opens = HANDED_OVER.iter().find(|(s, _)| *s == name).map(|(_, screen)| *screen);
            Some(Skill { name, description, source: "vault", opens })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    // The app's own first; a vault skill of the same name is hidden behind it.
    let mut all: Vec<Skill> = ours.collect();
    out.retain(|s| !all.iter().any(|a| a.name == s.name));
    all.extend(out);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings' Found row and the MCP assistants tool read `refused` as a warning: present only
    /// when OLLAMA_HOST was refused, and left out of every other row.
    #[test]
    fn cli_info_carries_refused_only_when_set() {
        let row = |refused| CliInfo {
            cli: Cli::Ollama,
            path: None,
            version: None,
            models: Vec::new(),
            efforts: Cli::Ollama.efforts(),
            note: Some("OLLAMA_HOST is gpu-box:11434.".into()),
            refused,
        };
        let on = serde_json::to_value(row(true)).unwrap();
        assert_eq!(on["refused"], serde_json::json!(true));
        assert_eq!(on["note"], "OLLAMA_HOST is gpu-box:11434.");
        let off = serde_json::to_value(row(false)).unwrap();
        assert!(off.get("refused").is_none(), "{off}");
        assert_eq!(off["cli"], "ollama");
    }

    /// Claude Code installed through npm on this computer is started without cmd.exe, and answers
    /// `--version` that way. Runs only where `%APPDATA%\npm\claude.cmd` is installed.
    #[cfg(windows)]
    #[test]
    fn npm_claude_runs_without_cmd() {
        let Some(shim) = dirs::data_dir().map(|d| d.join("npm").join("claude.cmd")).filter(|p| p.is_file()) else { return };
        let p = platform::find_program("claude", &[]).unwrap();
        assert!(!p.through_cmd(), "{p:?}");
        if p.path == shim {
            assert_ne!(p.exe, shim);
        }
        let v = version(&p).unwrap_or_default();
        assert!(v.contains("Claude Code"), "{v}");
    }

    #[test]
    fn stop_is_noted_only_while_a_turn_is_in_flight() {
        let r = Arc::new(Running::default());
        // Nothing in flight: a Stop has nothing to note, and nothing is busy.
        assert!(r.stopping("c1").is_empty());
        assert!(!r.stopped("c1"));
        assert!(!r.busy("c1") && !r.any());
        let (t1, t2) = (r.begin("c1"), r.begin("c2"));
        assert!(!r.add("c1", 41));
        assert!(!r.add("c2", 42));
        assert_eq!(r.stopping("c1"), [41]);
        assert!(r.stopped("c1") && !r.stopped("c2"));
        r.remove(41);
        r.remove(42);
        drop((t1, t2));
        // The turn over: not busy, and its Stop doesn't carry to the chat's next turn.
        assert!(!r.busy("c1") && !r.any());
        assert!(!r.stopped("c1"));
    }

    #[test]
    fn a_turn_between_runs_is_busy_and_takes_a_stop() {
        let r = Arc::new(Running::default());
        let turn = r.begin("c1");
        // No CLI answering (the refused run gone, the retry not yet started): still busy.
        assert!(r.busy("c1") && r.any() && !r.busy("c2"));
        assert!(r.stopping("c1").is_empty());
        assert!(r.stopped("c1"));
        // A CLI the turn starts after the Stop is told to stop at once.
        assert!(r.add("c1", 41));
        r.remove(41);
        assert!(r.busy("c1"));
        drop(turn);
        assert!(!r.busy("c1") && !r.any() && !r.stopped("c1"));
        // On quit, a turn between runs is noted as stopped too.
        let turn = r.begin("c2");
        r.kill_all();
        assert!(r.stopped("c2"));
        drop(turn);
    }

    #[test]
    fn copilot_auto_runs_without_an_effort() {
        assert_eq!(model_efforts(ask::COPILOT_AUTO), Some(Vec::new()));
        assert_eq!(fitted(ask::COPILOT_AUTO, Some("xhigh")), None);
        // A named Copilot model takes the level as set.
        assert_eq!(model_efforts("copilot:claude-sonnet-5.5"), None);
        assert_eq!(fitted("copilot:claude-sonnet-5.5", Some("max")).as_deref(), Some("max"));
    }

    fn refused() -> Outcome {
        Outcome { error: Some("error: unsupported reasoning effort 'max'".into()), ..Default::default() }
    }

    #[test]
    fn a_refused_effort_runs_once_more_without_it() {
        let r = Arc::new(Running::default());
        let mut levels = Vec::new();
        let out = fitted_turn(
            &r,
            "chat",
            "c1",
            "gpt-5",
            Some("max"),
            || Some("max".into()),
            |e| {
                levels.push(e.map(str::to_string));
                assert!(r.busy("c1"));
                if e.is_some() {
                    refused()
                } else {
                    Outcome { text: "Done.".into(), ..Default::default() }
                }
            },
        );
        assert_eq!(levels, [Some("max".to_string()), None]);
        assert_eq!(out.text, "Done.");
        assert!(!r.busy("c1"));
    }

    #[test]
    fn a_stop_between_a_refused_run_and_its_retry_ends_the_turn() {
        let r = Arc::new(Running::default());
        let mut runs = 0;
        let out = fitted_turn(
            &r,
            "chat",
            "c1",
            "gpt-5",
            Some("max"),
            || Some("max".into()),
            |_| {
                runs += 1;
                assert!(r.busy("c1"));
                // The refused run's CLI has exited when Stop is pressed: no process to stop.
                assert!(r.stopping("c1").is_empty());
                refused()
            },
        );
        assert_eq!(runs, 1);
        assert!(ask::effort_refused(&out));
        // The turn over, the chat takes the next send.
        assert!(!r.busy("c1") && !r.stopped("c1"));
    }

    #[test]
    fn a_stop_after_a_send_and_before_its_first_run_stops_that_run() {
        let r = Arc::new(Running::default());
        // ask_send has taken the message: the chat is busy and a second send is refused.
        let sent = r.try_begin("c1").expect("the chat is free");
        assert!(r.try_begin("c1").is_none() && r.busy("c1"));
        // Stop before the thread reaches the turn's first run: nothing to stop yet, but noted.
        r.cancel("c1");
        let mut stopped_at_once = Vec::new();
        let out = fitted_turn(
            &r,
            "chat",
            "c1",
            "gpt-5",
            None,
            || None,
            |_| {
                r.admit("c1", 41, |pid| stopped_at_once.push(pid));
                r.remove(41);
                Outcome::default()
            },
        );
        assert_eq!(stopped_at_once, [41]);
        assert!(out.error.is_none());
        // Still the send's turn until its thread ends; then the chat is free and the Stop forgotten.
        assert!(r.busy("c1") && r.stopped("c1"));
        drop(sent);
        assert!(!r.busy("c1") && !r.stopped("c1"));
        assert!(r.try_begin("c1").is_some());
    }

    /// A real process group, as run_at registers a CLI: started after a Stop on its turn, it is
    /// stopped rather than left to answer.
    #[test]
    fn a_cli_registered_after_a_stop_is_stopped() {
        // Exercises platform::stop_group on a real process, not a stand-in.
        let r = Arc::new(Running::default());
        let turn = r.begin("c1");
        r.cancel("c1");
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "ping -n 60 127.0.0.1"]);
            c
        } else {
            let mut c = std::process::Command::new("sleep");
            c.arg("60");
            c
        };
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = platform::spawn_group(&mut cmd).unwrap();
        let started = std::time::Instant::now();
        r.admit("c1", child.id(), platform::stop_group);
        let status = child.wait().unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(30), "{:?}", started.elapsed());
        assert!(!status.success());
        r.remove(child.id());
        drop(turn);
        // Without a Stop, a CLI registers and runs on.
        let turn = r.begin("c2");
        r.admit("c2", 42, |_| panic!("stopped without a Stop"));
        r.remove(42);
        drop(turn);
    }

    #[test]
    fn a_turn_is_running_until_it_ends_and_then_keeps_its_answer() {
        let t = Turns::default();
        assert!(!t.get("c1").running && t.get("c1").done.is_none());
        t.start("c1");
        assert!(t.get("c1").running);
        t.finish(&DoneMsg { chat_id: "c1".into(), outcome: Outcome { text: "Done.".into(), ..Default::default() } });
        let s = t.get("c1");
        assert!(!s.running);
        assert_eq!(s.done.unwrap().outcome.text, "Done.");
        t.start("c1");
        assert!(t.get("c1").running && t.get("c1").done.is_none());
    }
}
