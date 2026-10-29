// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Workspace's rules, with no Tauri in them: the Claude Code settings a guarded session starts
//! with, the guard its PreToolUse hook asks (may this tool call change the vault?), the folders a
//! session can start in, and the hook's event lines and what each does to a session's state.
//! The app (src-tauri/src/terminal.rs, workspace.rs) runs the sessions; the hook (`Brainstead
//! --hook`) answers Claude Code from here.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::projects::{self, ProjectLink};

/// Why a tool call that would change the vault was refused.
pub const REASON: &str = "The vault changes only through Brainstead's tools (edit_page, create_note, create_task…), so every change is in Changes with Revert. Use those instead.";

/// Why a session can't start in a folder.
pub const IN_VAULT: &str = "That folder is in your vault. The vault changes only through Brainstead, so use Ask there.";

/// The Claude Code tools that write a file named in their input.
const FILE_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// The hooks a guarded session runs, with their matchers. SessionEnd is beyond what the guard
/// needs: it tells the app that a session in an outside terminal has ended.
const HOOKS: [(&str, Option<&str>); 7] = [
    ("PreToolUse", Some("Bash|Edit|Write|MultiEdit|NotebookEdit")),
    ("PostToolUse", Some("Edit|Write|MultiEdit|NotebookEdit")),
    ("UserPromptSubmit", None),
    ("Stop", None),
    ("Notification", None),
    ("SessionStart", None),
    ("SessionEnd", None),
];

/// The hook's command line: this program with `--hook`, quoted for a path with spaces.
pub fn hook_command(exe: &Path) -> String {
    format!("\"{}\" --hook", exe.display())
}

/// `<data>/workspace/claude-settings.json`: the vault denied to Claude Code's file tools, and
/// every hook through `exe --hook`.
pub fn claude_settings(vault: &Path, exe: &Path) -> Value {
    // Ask's rule names the vault as Claude Code matches it; the same target serves each tool.
    let rule = crate::ask::claude_edit_rule(&vault.to_string_lossy());
    let target = rule.strip_prefix("Edit").unwrap_or(&rule).to_string();
    let deny: Vec<String> = FILE_TOOLS.iter().map(|t| format!("{t}{target}")).collect();
    let hook = json!([{ "type": "command", "command": hook_command(exe) }]);
    let mut hooks = serde_json::Map::new();
    for (event, matcher) in HOOKS {
        let entry = match matcher {
            Some(m) => json!([{ "matcher": m, "hooks": hook }]),
            None => json!([{ "hooks": hook }]),
        };
        hooks.insert(event.to_string(), entry);
    }
    json!({ "permissions": { "deny": deny }, "hooks": hooks })
}

/// The vault folder from `<data>/settings.json`, as the MCP server reads it; None when there's no
/// settings file or no vault chosen.
pub fn vault_from_settings(data: &Path) -> Option<PathBuf> {
    let v: Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).ok()?).ok()?;
    v.get("vaultPath").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from)
}

// --- paths ---------------------------------------------------------------------------------

/// `~`, `$HOME` and `${HOME}` at the start of `p` expanded; on Windows, Git Bash's `/c/…` read as `C:\…`.
fn expand_home(p: &str) -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        for pre in ["~", "${HOME}", "$HOME"] {
            if p == pre {
                return home;
            }
            if let Some(rest) = p.strip_prefix(pre).and_then(|r| r.strip_prefix('/').or_else(|| r.strip_prefix('\\'))) {
                return home.join(rest);
            }
        }
    }
    #[cfg(windows)]
    {
        let b = p.as_bytes();
        if b.len() >= 2 && b[0] == b'/' && b[1].is_ascii_alphabetic() && (b.len() == 2 || b[2] == b'/') {
            return PathBuf::from(format!("{}:\\{}", (b[1] as char).to_ascii_uppercase(), p.get(3..).unwrap_or("")));
        }
    }
    PathBuf::from(p)
}

/// `.` and `..` taken out without touching the disk.
fn normalise(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// A path's parts, last first (a stack to walk).
fn parts_rev(p: &Path) -> Vec<std::ffi::OsString> {
    let mut v: Vec<std::ffi::OsString> = p.components().map(|c| c.as_os_str().to_os_string()).collect();
    v.reverse();
    v
}

/// `p` as it really is, walked a part at a time as the system does it: each symlink is followed
/// where it's met, so a `..` after one leaves the folder it points to (`link/../a.md`), and past
/// the first part that isn't there the rest is taken as written. A dangling symlink is followed
/// too, since writing through it creates its target. The part that exists is then canonicalised,
/// for the spellings only the system knows (Windows' short names and junctions).
fn real(p: &Path) -> PathBuf {
    let mut stack = parts_rev(p);
    let mut out = PathBuf::new();
    let mut exists = true;
    let mut hops = 0;
    while let Some(part) = stack.pop() {
        match Path::new(&part).components().next() {
            Some(Component::CurDir) | None => {}
            Some(Component::ParentDir) => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            Some(Component::Normal(_)) => {
                let next = out.join(&part);
                if exists {
                    match std::fs::symlink_metadata(&next) {
                        Ok(m) if m.file_type().is_symlink() && hops < 40 => {
                            hops += 1;
                            if let Ok(t) = std::fs::read_link(&next) {
                                // Relative to the link's own folder, which is `out`.
                                if t.is_absolute() {
                                    out = PathBuf::new();
                                }
                                stack.extend(parts_rev(&t));
                                continue;
                            }
                            out = next;
                        }
                        Ok(_) => out = next,
                        Err(_) => {
                            exists = false;
                            out = next;
                        }
                    }
                } else {
                    out = next;
                }
            }
            Some(_) => out.push(&part),
        }
    }
    // The deepest existing ancestor's canonical path, then the rest (which has no `..` left).
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut base = out.as_path();
    loop {
        if let Ok(r) = dunce::canonicalize(base) {
            let mut o = r;
            for part in rest.iter().rev() {
                o.push(part);
            }
            return o;
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                base = parent;
            }
            _ => return out,
        }
    }
}

/// A path as a command or tool gives it, made absolute from `cwd`, with `~` expanded and
/// symlinks and `..` resolved as the system would.
pub fn resolve(cwd: &Path, p: &str) -> PathBuf {
    let e = expand_home(p);
    real(&if e.is_absolute() { e } else { cwd.join(e) })
}

/// Paths compared as the file system usually does: without case on macOS (APFS) and Windows, and
/// on Windows with either slash.
fn fold(s: &str) -> String {
    let s = if cfg!(windows) { s.replace('\\', "/") } else { s.to_string() };
    if cfg!(any(target_os = "macos", windows)) {
        s.to_lowercase()
    } else {
        s
    }
}

/// Whether `p` is `root` or inside it, a component at a time (so `/vault2` isn't in `/vault`),
/// ignoring case where the file system usually does.
fn within(p: &Path, root: &Path) -> bool {
    let parts = |x: &Path| -> Vec<String> {
        x.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| fold(c.as_os_str().to_string_lossy().trim_start_matches(r"\\?\")))
            .collect()
    };
    let (p, r) = (parts(p), parts(root));
    !r.is_empty() && p.starts_with(&r)
}

/// The vault's real path, as `within` compares it.
fn vault_real(vault: &Path) -> PathBuf {
    real(&expand_home(&vault.to_string_lossy()))
}

/// Whether the folder `path` is the vault or inside it: where sessions can't start.
pub fn in_vault(vault: &Path, path: &Path) -> bool {
    within(&real(path), &vault_real(vault))
}

// --- the guard -----------------------------------------------------------------------------

/// Whether a tool call in a session working in `cwd` would change the vault: Some(reason) to
/// refuse it. Reading the vault is always allowed, and anything outside it.
pub fn guard(vault: &Path, cwd: &Path, tool_name: &str, tool_input: &Value) -> Option<String> {
    let v = vault_real(vault);
    let hit = if FILE_TOOLS.contains(&tool_name) {
        ["file_path", "notebook_path", "path"]
            .iter()
            .filter_map(|k| tool_input.get(*k).and_then(Value::as_str))
            .any(|f| within(&resolve(cwd, f), &v))
    } else if tool_name == "Bash" {
        tool_input.get("command").and_then(Value::as_str).is_some_and(|c| bash_writes(vault, &v, cwd, c))
    } else {
        false
    };
    hit.then(|| REASON.to_string())
}

/// Whether a shell command could change the vault. A command that reaches the vault (a word that
/// names it, a folder in it) may run only programs that read; one that doesn't is always allowed.
/// Words with a variable in them count as the vault once the command mentions it anywhere.
fn bash_writes(vault: &Path, v: &Path, cwd: &Path, cmd: &str) -> bool {
    let spellings = spellings(vault, v);
    let start = Cwd::Known(real(cwd));
    let mut sh = Shell { vault: v, spellings: &spellings, mention: false };
    sh.mention = sh.cwd_in(&start) || sh.mentions_text(&start, cmd);
    let mut touched = false;
    let hit = sh.denies(&start, cmd, 0, &mut touched);
    if hit || sh.mention || !touched {
        return hit;
    }
    // It reached the vault in a way its text doesn't show (`cd ..` three times): again, with its
    // variables counting as the vault.
    sh.mention = true;
    sh.denies(&start, cmd, 0, &mut touched)
}

/// How a command's text may name the vault: its path as set and as it really is, and from home;
/// folded as `fold` compares.
fn spellings(vault: &Path, real: &Path) -> Vec<String> {
    let mut out = vec![vault.to_string_lossy().trim_end_matches(['/', '\\']).to_string(), real.to_string_lossy().into_owned()];
    if let Some(rel) = dirs::home_dir().and_then(|h| real.strip_prefix(dunce::canonicalize(&h).unwrap_or(h)).ok().map(Path::to_path_buf)) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        out.extend([format!("~/{rel}"), format!("$HOME/{rel}"), format!("${{HOME}}/{rel}")]);
        if cfg!(windows) {
            out.extend([format!("$env:USERPROFILE/{rel}"), format!("%USERPROFILE%/{rel}")]);
        }
    }
    // Git Bash's `/c/Users/…` for `C:\Users\…`.
    if cfg!(windows) {
        for s in out.clone() {
            let b = s.as_bytes();
            if b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
                out.push(format!("/{}{}", (b[0] as char).to_ascii_lowercase(), s[2..].replace('\\', "/")));
            }
        }
    }
    let mut out: Vec<String> = out.iter().map(|s| fold(s)).filter(|s| s.len() > 1).collect();
    out.sort();
    out.dedup();
    out
}

/// A shell word, or what separates or redirects them.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    /// `;`, `&&`, `||`, `&`, a newline, a parenthesis.
    Sep,
    /// `|`: the next command reads this one's output.
    Pipe,
    /// `>`, `>>`, `>|`, `&>`, `2>`, `<>`: the next word is written.
    Out,
    /// `<`: the next word is read.
    In,
    /// `<<`: the next word ends a here-document.
    Heredoc,
    /// `<<<`: the next word is the command's input.
    HereStr,
    /// A here-document's delimiter: its text is `bodies[n]`.
    Delim(usize),
    /// A `$(…)`, backtick or `<(…)` command inside the current one.
    Sub(String),
}

/// A command's tokens and its here-documents' text. Close enough to the shell's own reading for
/// the guard: quotes, escapes, redirections, here-documents and comments. A `$(…)` leaves a `$`
/// in its word, since what it stands for isn't known.
fn lex(s: &str) -> (Vec<Tok>, Vec<String>) {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    let mut toks = Vec::new();
    let mut bodies: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    // Here-documents whose text starts at the next newline: (delimiter, body).
    let mut pending: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    let flush =
        |toks: &mut Vec<Tok>, word: &mut String, in_word: &mut bool, pending: &mut Vec<(String, usize)>, bodies: &mut Vec<String>| {
            if *in_word {
                let w = std::mem::take(word);
                if toks.last() == Some(&Tok::Heredoc) {
                    bodies.push(String::new());
                    pending.push((w, bodies.len() - 1));
                    toks.push(Tok::Delim(bodies.len() - 1));
                } else {
                    toks.push(Tok::Word(w));
                }
                *in_word = false;
            }
        };
    macro_rules! flush {
        () => {
            flush(&mut toks, &mut word, &mut in_word, &mut pending, &mut bodies)
        };
    }
    while i < n {
        let c = cs[i];
        match c {
            '\\' => {
                if cs.get(i + 1).is_some_and(|x| *x != '\n') {
                    word.push(cs[i + 1]);
                    in_word = true;
                }
                i += 2;
            }
            '\'' => {
                in_word = true;
                i += 1;
                while i < n && cs[i] != '\'' {
                    word.push(cs[i]);
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                in_word = true;
                i += 1;
                while i < n && cs[i] != '"' {
                    if cs[i] == '\\' && cs.get(i + 1).is_some_and(|x| matches!(x, '"' | '\\' | '$' | '`')) {
                        word.push(cs[i + 1]);
                        i += 2;
                    } else if cs[i] == '$' && cs.get(i + 1) == Some(&'(') {
                        let (inner, j) = closing(&cs, i + 2, '(', ')');
                        toks.push(Tok::Sub(inner));
                        word.push('$');
                        i = j;
                    } else if cs[i] == '`' {
                        let (inner, j) = closing(&cs, i + 1, '`', '`');
                        toks.push(Tok::Sub(inner));
                        word.push('$');
                        i = j;
                    } else {
                        word.push(cs[i]);
                        i += 1;
                    }
                }
                i += 1;
            }
            '$' if cs.get(i + 1) == Some(&'(') => {
                let (inner, j) = closing(&cs, i + 2, '(', ')');
                toks.push(Tok::Sub(inner));
                word.push('$');
                in_word = true;
                i = j;
            }
            '`' => {
                let (inner, j) = closing(&cs, i + 1, '`', '`');
                toks.push(Tok::Sub(inner));
                word.push('$');
                in_word = true;
                i = j;
            }
            '#' if !in_word => {
                while i < n && cs[i] != '\n' {
                    i += 1;
                }
            }
            ' ' | '\t' | '\r' => {
                flush!();
                i += 1;
            }
            '\n' => {
                flush!();
                toks.push(Tok::Sep);
                i += 1;
                // A here-document's lines are its text, not commands.
                for (delim, id) in std::mem::take(&mut pending) {
                    let mut body = String::new();
                    while i < n {
                        let end = (i..n).find(|&k| cs[k] == '\n').unwrap_or(n);
                        let line: String = cs[i..end].iter().collect();
                        i = end + 1;
                        if line.trim_start_matches('\t').trim_end() == delim {
                            break;
                        }
                        body.push_str(&line);
                        body.push('\n');
                    }
                    bodies[id] = body;
                }
            }
            '&' if cs.get(i + 1) == Some(&'>') => {
                flush!();
                toks.push(Tok::Out);
                i += if cs.get(i + 2) == Some(&'>') { 3 } else { 2 };
            }
            '|' => {
                flush!();
                match cs.get(i + 1) {
                    Some('|') => {
                        toks.push(Tok::Sep);
                        i += 2;
                    }
                    Some('&') => {
                        toks.push(Tok::Pipe);
                        i += 2;
                    }
                    _ => {
                        toks.push(Tok::Pipe);
                        i += 1;
                    }
                }
            }
            ';' | '&' | '(' | ')' => {
                flush!();
                toks.push(Tok::Sep);
                i += 1;
            }
            '>' => {
                // `2>`: the digits are the descriptor, not a word.
                if in_word && !word.is_empty() && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    in_word = false;
                } else {
                    flush!();
                }
                i += 1;
                if cs.get(i) == Some(&'(') {
                    // `>(cmd)`: a command reading what's written.
                    let (inner, j) = closing(&cs, i + 1, '(', ')');
                    toks.push(Tok::Sub(inner));
                    i = j;
                    continue;
                }
                if matches!(cs.get(i), Some('>') | Some('|') | Some('&')) {
                    i += 1;
                }
                toks.push(Tok::Out);
            }
            '<' => {
                flush!();
                i += 1;
                match cs.get(i) {
                    Some('<') => {
                        i += 1;
                        if cs.get(i) == Some(&'<') {
                            i += 1;
                            toks.push(Tok::HereStr);
                        } else {
                            if cs.get(i) == Some(&'-') {
                                i += 1;
                            }
                            toks.push(Tok::Heredoc);
                        }
                    }
                    Some('(') => {
                        let (inner, j) = closing(&cs, i + 1, '(', ')');
                        toks.push(Tok::Sub(inner));
                        i = j;
                    }
                    // `<>`: opened for reading and writing.
                    Some('>') => {
                        i += 1;
                        toks.push(Tok::Out);
                    }
                    _ => toks.push(Tok::In),
                }
            }
            _ => {
                word.push(c);
                in_word = true;
                i += 1;
            }
        }
    }
    flush!();
    (toks, bodies)
}

/// The text from `start` to its closing `close` (nested `open`s counted), and the index after it.
fn closing(cs: &[char], start: usize, open: char, close: char) -> (String, usize) {
    let mut depth = 1;
    let mut i = start;
    while i < cs.len() {
        let c = cs[i];
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == close && (open == close || depth == 1) {
            return (cs[start..i].iter().collect(), i + 1);
        }
        if open != close {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
            }
        }
        i += 1;
    }
    (cs[start.min(cs.len())..].iter().collect(), cs.len())
}

/// The `$(…)` and backtick commands in a here-document's text, which the shell runs.
fn body_subs(body: &str) -> Vec<String> {
    let cs: Vec<char> = body.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '$' && cs.get(i + 1) == Some(&'(') {
            let (inner, j) = closing(&cs, i + 2, '(', ')');
            out.push(inner);
            i = j;
        } else if cs[i] == '`' {
            let (inner, j) = closing(&cs, i + 1, '`', '`');
            out.push(inner);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// One simple command: its words, what it writes and reads, its here-documents' text, the
/// commands inside it, and the pipeline it's part of.
#[derive(Debug, Default)]
struct Seg {
    words: Vec<String>,
    outs: Vec<String>,
    ins: Vec<String>,
    bodies: Vec<String>,
    subs: Vec<String>,
    pipeline: usize,
}

fn segments(toks: Vec<Tok>, bodies: &[String]) -> Vec<Seg> {
    let mut out = vec![Seg::default()];
    let mut pipeline = 0;
    let mut next: Option<Tok> = None;
    for t in toks {
        let seg = out.last_mut().expect("one segment at least");
        match t {
            Tok::Word(w) => match next.take() {
                Some(Tok::Out) => seg.outs.push(w),
                Some(Tok::In) => seg.ins.push(w),
                Some(Tok::HereStr) => seg.bodies.push(w),
                Some(_) => {}
                None => seg.words.push(w),
            },
            Tok::Delim(id) => {
                next = None;
                let b = bodies.get(id).cloned().unwrap_or_default();
                seg.subs.extend(body_subs(&b));
                seg.bodies.push(b);
            }
            Tok::Sub(s) => seg.subs.push(s),
            Tok::Sep | Tok::Pipe => {
                if t == Tok::Sep {
                    pipeline += 1;
                }
                next = None;
                out.push(Seg { pipeline, ..Default::default() });
            }
            t => next = Some(t),
        }
    }
    out.retain(|s| !(s.words.is_empty() && s.outs.is_empty() && s.ins.is_empty() && s.bodies.is_empty() && s.subs.is_empty()));
    out
}

/// Shells, whose `-c` command is checked as a command of its own.
const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "dash", "ksh", "fish"];
/// Interpreters besides `python*`, whose code may do anything.
const INTERPRETERS: [&str; 10] = ["node", "nodejs", "ruby", "perl", "deno", "bun", "osascript", "pwsh", "powershell", "php"];
/// Words before a command that aren't its program.
const KEYWORDS: [&str; 10] = ["if", "then", "else", "elif", "do", "while", "until", "!", "{", "coproc"];
/// Words that start something other than a command: a list of words, or the end of a block.
const NOT_COMMANDS: [&str; 8] = ["for", "select", "case", "function", "fi", "done", "esac", "}"];
/// Programs that only read, whatever they're given: what may run where the vault is in reach.
const READERS: [&str; 52] = [
    "cat",
    "head",
    "tail",
    "grep",
    "egrep",
    "fgrep",
    "ag",
    "ls",
    "stat",
    "file",
    "wc",
    "du",
    "diff",
    "cmp",
    "md5",
    "md5sum",
    "shasum",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "realpath",
    "readlink",
    "basename",
    "dirname",
    "echo",
    "printf",
    "test",
    "[",
    "[[",
    "cut",
    "tr",
    "column",
    "cd",
    "pushd",
    "popd",
    "pwd",
    "true",
    "false",
    ":",
    "export",
    "unset",
    "local",
    "declare",
    "typeset",
    "set",
    "shopt",
    "exit",
    "return",
    "sleep",
    "which",
    "type",
    "read",
];
/// Output targets that aren't files.
const DEVICES: [&str; 7] = ["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty", "nul", "$null", "-"];

fn is_interpreter(p: &str) -> bool {
    p.starts_with("python") || INTERPRETERS.contains(&p)
}

fn is_flag(a: &str) -> bool {
    a.len() > 1 && a.starts_with('-')
}

fn is_assignment(a: &str) -> bool {
    a.split_once('=').is_some_and(|(k, _)| {
        !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// A program's name as written (`/bin/rm` is `rm`, `python3.12` is `python3`, `RM.EXE` is `rm`).
fn program(w: &str) -> String {
    let base = w.rsplit(['/', '\\']).next().unwrap_or(w).to_lowercase();
    let base = [".exe", ".cmd", ".bat"].iter().find_map(|x| base.strip_suffix(x)).unwrap_or(&base).to_string();
    match base.split_once('.') {
        Some((p, v)) if v.chars().all(|c| c.is_ascii_digit() || c == '.') => p.to_string(),
        _ => base,
    }
}

/// A program that runs the one after it, and its options that take a value: (short letters,
/// long names).
fn wrapper(p: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match p {
        "sudo" => {
            ("ughpCDrtUT", &["user", "group", "host", "prompt", "close-from", "chdir", "role", "type", "other-user", "command-timeout"])
        }
        "doas" => ("uC", &[]),
        "env" => ("uC", &["unset", "chdir"]),
        "nice" => ("n", &["adjustment"]),
        "time" => ("fo", &["format", "output"]),
        "timeout" => ("sk", &["signal", "kill-after"]),
        "xargs" => ("ILnPdsEa", &["max-args", "max-procs", "delimiter", "arg-file", "max-lines", "max-chars", "eof", "process-slot-var"]),
        "stdbuf" => ("ioe", &["input", "output", "error"]),
        "ionice" => ("cnp", &["class", "classdata", "pid"]),
        "exec" => ("a", &[]),
        "command" | "builtin" | "nohup" => ("", &[]),
        _ => return None,
    })
}

/// What a simple command runs.
#[derive(Debug, PartialEq)]
enum Run<'a> {
    /// Only assignments, or not a command (`for x in …`, `done`).
    Nothing,
    /// A program, its arguments, and files its wrappers write (`time -o`).
    Prog(String, &'a [String], Vec<&'a str>),
    /// Something whose program can't be told (`env -S '…'`).
    Unclear,
}

/// The program a simple command runs and its arguments, past `VAR=x`, keywords like `then`, and
/// wrappers like `sudo -u me` or `timeout 5` with their options.
fn command_of(words: &[String]) -> Run<'_> {
    let mut writes = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        if is_assignment(w) || KEYWORDS.contains(&w) {
            i += 1;
            continue;
        }
        if NOT_COMMANDS.contains(&w) {
            return Run::Nothing;
        }
        let p = program(w);
        let Some((short, long)) = wrapper(&p) else { return Run::Prog(p, &words[i + 1..], writes) };
        i += 1;
        // timeout's duration.
        let mut skip = usize::from(p == "timeout");
        while i < words.len() {
            let a = words[i].as_str();
            if a == "--" {
                i += 1;
                break;
            }
            if p == "env" && is_assignment(a) {
                i += 1;
                continue;
            }
            if p == "env" && (a.starts_with("-S") || a.starts_with("--split-string")) {
                return Run::Unclear;
            }
            if !is_flag(a) {
                if skip > 0 {
                    skip -= 1;
                    i += 1;
                    continue;
                }
                break;
            }
            let value = if let Some(l) = a.strip_prefix("--") {
                match l.split_once('=') {
                    Some((k, v)) => {
                        if p == "time" && k == "output" {
                            writes.push(v);
                        }
                        false
                    }
                    None => long.contains(&l),
                }
            } else {
                // `-n 5`: a letter that takes a value, last in its cluster, takes the next word.
                let letters = &a[1..];
                let at = letters.char_indices().find(|(_, c)| short.contains(*c));
                if let Some((k, 'o')) = at.filter(|_| p == "time") {
                    if k + 1 < letters.len() {
                        writes.push(&letters[k + 1..]);
                    }
                }
                at.is_some_and(|(k, _)| k + 1 == letters.len())
            };
            if value {
                if p == "time" && (a == "-o" || a == "--output") {
                    if let Some(v) = words.get(i + 1) {
                        writes.push(v);
                    }
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        if p == "xargs" && i >= words.len() {
            return Run::Prog("echo".into(), &[], writes);
        }
    }
    Run::Nothing
}

/// The `-c` command a shell is given, if it's given one.
fn shell_code(args: &[String]) -> Option<&str> {
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if matches!(a, "-o" | "+o" | "-O" | "+O" | "--rcfile" | "--init-file") {
            i += 2;
            continue;
        }
        if !is_flag(a) {
            return None;
        }
        if !a.starts_with("--") && a[1..].contains('c') {
            return args.get(i + 1).map(String::as_str);
        }
        i += 1;
    }
    None
}

/// An interpreter's code given on its command line (`python3 -c`, `node -e`, `perl -ne`), or None
/// when it runs a script or reads its code from standard input. Also whether perl or ruby edit
/// their files in place (`-i`, `-pi.bak`, not `-MList::Util`).
fn inline_code(p: &str, args: &[String]) -> (Option<String>, bool) {
    let pwsh = p == "pwsh" || p == "powershell";
    let perlish = p == "perl" || p == "ruby";
    let mut code: Vec<&str> = Vec::new();
    let mut in_place = false;
    let mut i = 0;
    if p == "deno" {
        return match args.first().map(String::as_str) {
            Some("eval") => (args.get(1).cloned(), false),
            _ => (None, false),
        };
    }
    while i < args.len() {
        let a = args[i].as_str();
        if pwsh && (a.eq_ignore_ascii_case("-command") || a.eq_ignore_ascii_case("-c")) {
            code.extend(args[i + 1..].iter().map(String::as_str));
            break;
        }
        if !is_flag(a) {
            // A script, or (after code) its arguments.
            break;
        }
        let is_code = match p {
            _ if p.starts_with("python") => a == "-c",
            "node" | "nodejs" | "bun" => matches!(a, "-e" | "--eval" | "-p" | "--print"),
            "osascript" => a == "-e",
            "php" => a == "-r",
            _ if perlish => {
                !a.starts_with("--") && !a.starts_with("-M") && !a.starts_with("-m") && !a.starts_with("-I") && a.ends_with(['e', 'E'])
            }
            _ => false,
        };
        if perlish && !a.starts_with("--") && !["-M", "-m", "-I", "-r", "-x", "-C"].iter().any(|f| a.starts_with(f)) && a[1..].contains('i')
        {
            in_place = true;
        }
        if is_code {
            if let Some(c) = args.get(i + 1) {
                code.push(c);
            }
            i += 2;
            continue;
        }
        let value = match p {
            _ if p.starts_with("python") => matches!(a, "-W" | "-X"),
            "node" | "nodejs" | "bun" => matches!(a, "-r" | "--require" | "--import"),
            _ => false,
        };
        // `python -m`: a module, as unknown as a script.
        if p.starts_with("python") && a == "-m" {
            return (None, in_place);
        }
        i += if value { 2 } else { 1 };
    }
    ((!code.is_empty()).then(|| code.join("\n")), in_place)
}

/// Code that could write, delete or move a file, or run something that does: what makes an
/// interpreter's code that names the vault a change to it. Reading (`open(p)`, `print`,
/// `sys.stdout.write`) isn't.
fn writes_code(p: &str, code: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    static PERL: OnceLock<regex::Regex> = OnceLock::new();
    static PWSH: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(concat!(
            // open(…, 'w'), open(…, mode="a+"), perl's open(F, '>x').
            r#"\bopen\w*\s*\(.*?,\s*(?:mode\s*=\s*)?['"](?:[rbtU]*[wax+][rbtwax+U]*['"]|\s*(?:\+[<>]|>|\||-\|))"#,
            // Path.open('w'), perl's `open my $f, '>', …`.
            r#"|\.open\s*\(\s*(?:mode\s*=\s*)?['"][rbtU]*[wax+]"#,
            r#"|\bopen\s+(?:my\s+)?\$?\w+\s*,\s*['"]\s*(?:\+[<>]|>|\||-\|)"#,
            r#"|\b(?:write_text|write_bytes|writeFile\w*|appendFile\w*|createWriteStream|copyFile\w*|copytree|copy2|copyfile|rmtree|unlink\w*|rmdir\w*|rmSync|mkdir\w*|makedirs|removedirs|rename\w*|truncate\w*|symlink\w*|symlink_to|hardlink_to|chmod\w*|chown\w*|lchown|utime\w*|sysopen|mkfifo|mknod)\s*\("#,
            r#"|\.(?:touch|link)\s*\("#,
            r#"|\bshutil\.|\bos\.(?:remove|unlink|rename|renames|replace|makedirs|mkdir|rmdir|removedirs|truncate|symlink|link|chmod|chown|utime|system|popen|exec\w*|spawn\w*|fork\w*|posix_spawn\w*|open|mkfifo|mknod|write)\b"#,
            r#"|\bO_(?:WRONLY|RDWR|CREAT|APPEND|TRUNC)\b"#,
            r#"|\b(?:subprocess|child_process|pty|ctypes|importlib|FileUtils)\b|__import__"#,
            r#"|(?:^|[^.\w])(?:exec|eval|compile|getattr|system|popen|execSync|execFile\w*|spawn\w*|fork|Function|import)\s*\("#,
            r#"|\b(?:File|IO|Dir|Pathname)\.(?:write|binwrite|delete|unlink|rename|truncate|symlink|link|chmod|chown|utime|mkdir|rmdir)\b"#,
            r#"|\bDeno\.(?:write\w*|remove\w*|rename\w*|mkdir\w*|create\w*|truncate\w*|symlink\w*|link\w*|copyFile\w*|chmod\w*|chown\w*|run|Command|open\w*|makeTemp\w*|utime\w*)\b"#,
            r#"|\bBun\.(?:write|spawn|\$)"#,
            r#"|\b(?:fs|fsp|promises)\.(?:rm|cp|write\w*|append\w*|unlink\w*|rename\w*|mkdir\w*|rmdir\w*|copy\w*|truncate\w*|symlink\w*|link\w*|chmod\w*|chown\w*|utimes\w*|open\w*|createWriteStream|mkdtemp\w*)\b"#,
            r#"|\bdo\s+shell\s+script\b"#,
        ))
        .expect("a valid pattern")
    });
    if re.is_match(code) {
        return true;
    }
    if p == "perl" || p == "ruby" {
        // Calls without parentheses, and commands in backticks or qx.
        let perl = PERL.get_or_init(|| {
            regex::Regex::new(
                r#"\b(?:unlink|rename|mkdir|rmdir|truncate|symlink|link|chmod|chown|utime|system|exec|qx|syscall|sysopen)\b|`|%x"#,
            )
            .expect("a valid pattern")
        });
        return perl.is_match(code);
    }
    if p == "pwsh" || p == "powershell" {
        let ps = PWSH.get_or_init(|| {
            regex::Regex::new(r#"(?i)\b(?:Set-Content|Add-Content|Out-File|Remove-Item|Move-Item|Copy-Item|New-Item|Rename-Item|Clear-Content|Clear-Item|Set-Item|Invoke-Expression|iex|Start-Process|Invoke-Item|Export-\w+|Tee-Object|rm|del|erase|mv|move|cp|copy|ni|ri|mi|ci|sc|ac|rd|rmdir|mkdir|md)\b|>|\[(?:System\.)?IO\.|\.Delete\s*\("#)
                .expect("a valid pattern")
        });
        return ps.is_match(code);
    }
    false
}

/// Whether a program only reads, with these arguments: the allowlist for a command that reaches
/// the vault.
fn read_only(p: &str, args: &[String]) -> bool {
    let any = |f: &dyn Fn(&str) -> bool| args.iter().any(|a| f(a));
    let short = |a: &str, c: char| a.starts_with('-') && !a.starts_with("--") && a[1..].contains(c);
    match p {
        _ if READERS.contains(&p) => true,
        "less" | "more" => !any(&|a| a.starts_with("-o") || a.starts_with("-O") || a.to_lowercase().starts_with("--log-file")),
        "rg" => !any(&|a| a == "--pre" || a.starts_with("--pre=")),
        "tree" => !any(&|a| a == "-o"),
        "find" => !any(&|a| matches!(a, "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir" | "-fls") || a.starts_with("-fprint")),
        "jq" => !any(&|a| a == "-i" || a.starts_with("--in-place")),
        "sort" => !any(&|a| short(a, 'o') || a.starts_with("--output")),
        "uniq" => {
            // `uniq in out` writes out.
            let mut n = 0;
            let mut i = 0;
            while i < args.len() {
                let a = args[i].as_str();
                if matches!(a, "-f" | "-s" | "-w") {
                    i += 2;
                    continue;
                }
                if !is_flag(a) {
                    n += 1;
                }
                i += 1;
            }
            n <= 1
        }
        "git" => git_reads(args),
        _ => false,
    }
}

/// git commands that only read: status, log, show, diff, blame, grep, ls-files, rev-parse,
/// cat-file, and branch listing. No `-c` (a config can run anything), no `--output`.
fn git_reads(args: &[String]) -> bool {
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-C" | "--git-dir" | "--work-tree" | "--namespace" => i += 2,
            "-c" | "--config-env" => return false,
            _ if a.starts_with("--exec-path") || a.starts_with("--config-env") => return false,
            _ if is_flag(a) => i += 1,
            _ => break,
        }
    }
    let Some(sub) = args.get(i) else { return true };
    let rest = &args[i + 1..];
    if rest.iter().any(|a| a.starts_with("--output")) {
        return false;
    }
    match sub.as_str() {
        "status" | "log" | "show" | "diff" | "blame" | "ls-files" | "rev-parse" | "cat-file" => true,
        "grep" => !rest.iter().any(|a| a == "-O" || a.starts_with("--open-files-in-pager")),
        "branch" => {
            let changes = rest.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "-d" | "-D"
                        | "-m"
                        | "-M"
                        | "-c"
                        | "-C"
                        | "-f"
                        | "-u"
                        | "--delete"
                        | "--move"
                        | "--copy"
                        | "--force"
                        | "--unset-upstream"
                        | "--edit-description"
                ) || a.starts_with("--set-upstream")
            });
            // A name after `branch` makes a branch, unless it's listing (`--list`, `--contains x`).
            let listing = rest.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "-l" | "--list"
                        | "-a"
                        | "--all"
                        | "-r"
                        | "--remotes"
                        | "--contains"
                        | "--no-contains"
                        | "--merged"
                        | "--no-merged"
                        | "--points-at"
                )
            });
            !changes && (listing || rest.iter().all(|a| is_flag(a)))
        }
        _ => false,
    }
}

/// The folder a command runs in, as far as the guard can follow it.
#[derive(Debug, Clone)]
enum Cwd {
    Known(PathBuf),
    /// Lost track (`cd -` with nothing before it, `cd "$X"` in a command that doesn't mention the
    /// vault): relative paths can't be told, and aren't taken as the vault.
    Unknown,
    /// `cd "$X"` in a command that mentions the vault: taken as the vault.
    Suspect,
}

struct Shell<'a> {
    vault: &'a Path,
    spellings: &'a [String],
    /// The command line mentions the vault somewhere, so a word with a variable may be it.
    mention: bool,
}

impl Shell<'_> {
    fn cwd_in(&self, cwd: &Cwd) -> bool {
        match cwd {
            Cwd::Known(p) => within(p, self.vault),
            Cwd::Unknown => false,
            Cwd::Suspect => true,
        }
    }

    /// `w` resolved from `cwd`, or None when that can't be told.
    fn path(cwd: &Cwd, w: &str) -> Option<PathBuf> {
        let e = expand_home(w);
        if e.is_absolute() {
            return Some(real(&e));
        }
        match cwd {
            Cwd::Known(p) => Some(real(&p.join(e))),
            _ => None,
        }
    }

    fn inside(&self, cwd: &Cwd, w: &str) -> bool {
        if w.is_empty() {
            return false;
        }
        match Self::path(cwd, w) {
            Some(p) => within(&p, self.vault),
            None => matches!(cwd, Cwd::Suspect),
        }
    }

    /// A word that names the vault: as a path, as the value of `--flag=path` or `of=path`, or with
    /// a variable in it when the command mentions the vault.
    fn names(&self, cwd: &Cwd, w: &str) -> bool {
        if self.mention && w.contains('$') {
            return true;
        }
        if !is_flag(w) && self.inside(cwd, w) {
            return true;
        }
        w.split_once('=').is_some_and(|(_, v)| self.inside(cwd, v))
    }

    /// Text that spells out the vault's path (not just a longer name starting with it).
    fn spells(&self, text: &str) -> bool {
        let t = fold(text);
        self.spellings.iter().any(|s| {
            t.match_indices(s.as_str()).any(|(i, _)| {
                let before = t[..i].chars().next_back();
                let after = t[i + s.len()..].chars().next();
                !before.is_some_and(|c| c.is_alphanumeric() || "._-~$".contains(c))
                    && !after.is_some_and(|c| c.is_alphanumeric() || "._-".contains(c))
            })
        })
    }

    /// Text that mentions the vault: spelled out, or any word of it that is a path into it
    /// (`X=../../vault`, code given to an interpreter).
    fn mentions_text(&self, cwd: &Cwd, text: &str) -> bool {
        if self.spells(text) {
            return true;
        }
        let sep = |c: char| c.is_whitespace() || "'\"`;&|()<>=,{}[]".contains(c) || (cfg!(not(windows)) && c == ':');
        let mut seen = std::collections::HashSet::new();
        text.split(sep)
            .filter(|w| !w.is_empty() && !w.contains('$') && !w.starts_with('-') && w.len() < 1024)
            .filter(|w| seen.insert(*w))
            .take(2000)
            .any(|w| self.inside(cwd, w))
    }

    /// Whether `cmd`, run in `cwd`, could change the vault. `touched` is set when any part of it
    /// reached the vault.
    fn denies(&self, cwd: &Cwd, cmd: &str, depth: usize, touched: &mut bool) -> bool {
        if depth > 8 {
            // Too deeply nested to follow.
            return self.mention;
        }
        let (toks, bodies) = lex(cmd);
        let segs = segments(toks, &bodies);
        // Each command's folder, what it runs and whether it reaches the vault, in order.
        let mut cwd = cwd.clone();
        let mut prev: Option<Cwd> = None;
        let mut stack: Vec<Cwd> = Vec::new();
        let mut plans: Vec<(Cwd, Run, bool)> = Vec::new();
        for seg in &segs {
            let run = command_of(&seg.words);
            let mut reach = self.cwd_in(&cwd)
                || seg.words.iter().chain(&seg.outs).chain(&seg.ins).any(|w| self.names(&cwd, w))
                || self.spells(&seg.words.join(" "))
                || seg.bodies.iter().any(|b| self.spells(b));
            if let Run::Prog(p, args, _) = &run {
                let p = p.as_str();
                if is_interpreter(p) || SHELLS.contains(&p) {
                    reach = reach || self.mentions_text(&cwd, &args.join(" ")) || seg.bodies.iter().any(|b| self.mentions_text(&cwd, b));
                    // Code from standard input, which the guard can't see.
                    reach = reach || (self.mention && args.iter().all(|a| is_flag(a) || a == "-") && seg.bodies.is_empty());
                }
                if p == "git" {
                    reach = reach || self.cwd_in(&git_dir(&cwd, args));
                }
                if matches!(p, "cd" | "pushd" | "popd") {
                    let target = args.iter().find(|a| !is_flag(a)).map(String::as_str);
                    let here = cwd.clone();
                    let next = match (p, target) {
                        ("popd", _) => stack.pop().unwrap_or(Cwd::Unknown),
                        ("pushd", None) => Cwd::Unknown,
                        (_, None) => dirs::home_dir().map_or(Cwd::Unknown, Cwd::Known),
                        (_, Some("-")) => prev.clone().unwrap_or(Cwd::Unknown),
                        (_, Some(a)) if a.starts_with('+') && p == "pushd" => Cwd::Unknown,
                        (_, Some(a)) if a.contains('$') => {
                            if self.mention {
                                Cwd::Suspect
                            } else {
                                Cwd::Unknown
                            }
                        }
                        (_, Some(a)) => match Self::path(&cwd, a) {
                            Some(d) => Cwd::Known(d),
                            None => cwd.clone(),
                        },
                    };
                    if p == "pushd" {
                        stack.push(here.clone());
                    }
                    plans.push((here.clone(), run, reach));
                    prev = Some(here);
                    cwd = next;
                    continue;
                }
            }
            plans.push((cwd.clone(), run, reach));
        }
        // A pipeline reaches the vault as a whole: `ls vault | xargs rm`.
        let mut piped = std::collections::HashSet::new();
        for (seg, (_, _, reach)) in segs.iter().zip(&plans) {
            if *reach {
                piped.insert(seg.pipeline);
            }
        }
        for (seg, (cwd, run, reach)) in segs.iter().zip(&plans) {
            for s in &seg.subs {
                if self.denies(cwd, s, depth + 1, touched) {
                    return true;
                }
            }
            for o in &seg.outs {
                let fd = o.chars().all(|c| c.is_ascii_digit()) || DEVICES.contains(&o.to_lowercase().as_str()) || o.starts_with("/dev/fd/");
                if !fd && self.names(cwd, o) {
                    *touched = true;
                    return true;
                }
            }
            let reach = *reach || piped.contains(&seg.pipeline);
            if reach {
                *touched = true;
            }
            let hit = match run {
                Run::Nothing => false,
                Run::Unclear => reach,
                Run::Prog(p, args, writes) => {
                    writes.iter().any(|w| self.names(cwd, w)) || self.program_denies(p, args, seg, cwd, reach, depth, touched)
                }
            };
            if hit {
                return true;
            }
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn program_denies(&self, p: &str, args: &[String], seg: &Seg, cwd: &Cwd, reach: bool, depth: usize, touched: &mut bool) -> bool {
        if SHELLS.contains(&p) {
            // Its own commands are checked as commands, wherever it runs.
            let code = shell_code(args);
            if code.is_some_and(|c| self.denies(cwd, c, depth + 1, touched)) {
                return true;
            }
            if seg.bodies.iter().any(|b| self.denies(cwd, b, depth + 1, touched)) {
                return true;
            }
            // In the vault's reach, nothing it reads that the guard can't see.
            return reach && (code.is_none() || !seg.bodies.is_empty());
        }
        if p == "eval" {
            return self.denies(cwd, &args.join(" "), depth + 1, touched) || (reach && args.is_empty());
        }
        if p == "find" && !reach {
            // The commands it runs, on a stand-in file.
            return args.iter().enumerate().filter(|(_, a)| matches!(a.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir")).any(|(i, _)| {
                let cmd: Vec<String> = args[i + 1..]
                    .iter()
                    .take_while(|a| *a != ";" && *a != "+" && *a != "\\;")
                    .map(|a| if a == "{}" { "x".to_string() } else { format!("'{}'", a.replace('\'', "'\\''")) })
                    .collect();
                self.denies(cwd, &cmd.join(" "), depth + 1, touched)
            });
        }
        if !reach {
            return false;
        }
        if is_interpreter(p) {
            // Only code on its command line that writes nothing; never a script or a here-document.
            let (code, in_place) = inline_code(p, args);
            return in_place || !seg.bodies.is_empty() || code.is_none_or(|c| writes_code(p, &c));
        }
        !read_only(p, args)
    }
}

/// The folder git works in, through its `-C`s.
fn git_dir(cwd: &Cwd, args: &[String]) -> Cwd {
    let mut dir = cwd.clone();
    let mut i = 0;
    while i < args.len() && is_flag(&args[i]) {
        if args[i] == "-C" {
            if let Some(d) = args.get(i + 1) {
                dir = Shell::path(&dir, d).map_or(Cwd::Unknown, Cwd::Known);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    dir
}

// --- folders -------------------------------------------------------------------------------

/// A place to start a session, for the Workspace's list (src/workspaceApi.ts `Folder`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub path: String,
    pub name: String,
    /// "root", "repo" or "folder".
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Filled in by the app, which runs git.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectLink>,
    pub inside_vault: bool,
}

/// A folder's name in the list: its own, or `~` for the home folder.
pub fn folder_name(path: &Path) -> String {
    if dirs::home_dir().is_some_and(|h| h == path) {
        return "~".into();
    }
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned())
}

pub fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// A repo's checked-out branch from `.git/HEAD` (or a worktree's, through its `.git` file),
/// without running git; a detached head is its commit's first seven characters.
pub fn branch(repo: &Path) -> Option<String> {
    let dot = repo.join(".git");
    let git_dir = if dot.is_file() {
        let s = std::fs::read_to_string(&dot).ok()?;
        let d = PathBuf::from(s.trim().strip_prefix("gitdir:")?.trim());
        if d.is_absolute() {
            d
        } else {
            repo.join(d)
        }
    } else {
        dot
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(r) => Some(r.trim().strip_prefix("refs/heads/").unwrap_or(r.trim()).to_string()),
        None if head.len() >= 7 => Some(head[..7].to_string()),
        None => None,
    }
}

/// Folders not looked in for repos.
fn skipped(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "vendor" | "dist" | "build")
}

/// The git repos one or two levels under `root`, by path. A repo's own folders aren't looked in.
fn repos_under(root: &Path) -> Vec<PathBuf> {
    let dirs_in = |d: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !skipped(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        v
    };
    let mut out = Vec::new();
    for d in dirs_in(root) {
        if is_repo(&d) {
            out.push(d);
        } else {
            out.extend(dirs_in(&d).into_iter().filter(|x| is_repo(x)));
        }
    }
    out
}

/// What the Workspace lists: each root followed by its repos, then pinned folders and then recent
/// ones not already listed. `recent` is newest first with when a session last started there.
/// A folder appears once, carrying whether it's pinned and when it was last used.
pub fn discover(roots: &[String], pinned: &[String], recent: &[(String, i64)], vault: Option<&Path>) -> Vec<Folder> {
    // The project notes are read once for the whole list.
    let claims = vault.map(projects::folder_claims).unwrap_or_default();
    discover_with(roots, pinned, recent, vault, &claims)
}

/// `discover` with the project notes' folders already read (the app has them in its index).
pub fn discover_with(
    roots: &[String],
    pinned: &[String],
    recent: &[(String, i64)],
    vault: Option<&Path>,
    claims: &[projects::FolderClaim],
) -> Vec<Folder> {
    let vault_r = vault.map(vault_real);
    let home = dirs::home_dir();
    let key = |p: &Path| normalise(p).to_string_lossy().trim_end_matches(['/', '\\']).to_string();
    let pinned_keys: Vec<String> = pinned.iter().map(|p| key(&expand_home(p))).collect();
    let make = |path: &Path, kind: &str, root: Option<&Path>| -> Folder {
        let k = key(path);
        let repo = is_repo(path);
        Folder {
            path: k.clone(),
            name: folder_name(path),
            kind: if kind == "folder" && repo { "repo".into() } else { kind.into() },
            root: root.map(&key),
            branch: if repo { branch(path) } else { None },
            dirty: None,
            pinned: pinned_keys.contains(&k),
            recent: recent.iter().find(|(p, _)| key(&expand_home(p)) == k).map(|(_, t)| *t).filter(|t| *t > 0),
            project: projects::pick_project(claims, path, home.as_deref()),
            inside_vault: vault_r.as_ref().is_some_and(|v| within(&real(&normalise(path)), v)),
        }
    };
    let mut out: Vec<Folder> = Vec::new();
    let push = |f: Folder, out: &mut Vec<Folder>| {
        if !out.iter().any(|x| x.path == f.path) {
            out.push(f);
        }
    };
    for r in roots {
        let r = expand_home(r);
        push(make(&r, "root", None), &mut out);
        for repo in repos_under(&r) {
            push(make(&repo, "repo", Some(&r)), &mut out);
        }
    }
    for p in pinned.iter().chain(recent.iter().map(|(p, _)| p)) {
        let p = expand_home(p);
        if p.is_dir() {
            push(make(&p, "folder", None), &mut out);
        }
    }
    out
}

/// `list` with `path` first and no copy of it, at most `max` long: the recent folders' upkeep.
pub fn bump(list: &[String], path: &str, max: usize) -> Vec<String> {
    let mut out = vec![path.to_string()];
    out.extend(list.iter().filter(|p| p.as_str() != path).cloned());
    out.truncate(max);
    out
}

// --- launching ---------------------------------------------------------------------------

/// What a session runs, besides the folder.
#[derive(Debug, Clone, Copy)]
pub struct Launch<'a> {
    pub program: Program,
    /// `<data>/workspace/claude-settings.json`.
    pub settings: &'a Path,
    /// Brainstead's MCP server.
    pub mcp: Option<&'a crate::ask::Mcp>,
    /// `<data>/workspace/mcp.json`, holding `mcp_config(mcp)` for Claude Code: a path, not the
    /// JSON itself, since Windows PowerShell 5.1 strips the quotes inside an argument it passes on.
    pub mcp_config: Option<&'a Path>,
    pub resume: Option<&'a str>,
    pub prompt: Option<&'a str>,
    pub model: Option<&'a str>,
    pub worktree: Option<&'a str>,
    /// The vault is inside the folder, so Codex's workspace sandbox would let it write there.
    pub vault_inside: bool,
}

/// Claude Code's `--mcp-config` for Brainstead's server, as Ask gives it.
pub fn mcp_config(m: &crate::ask::Mcp) -> Value {
    json!({ "mcpServers": { crate::ask::MCP_NAME: { "type": "stdio", "command": m.command, "args": m.args } } })
}

/// A TOML string for Codex's `-c`: a literal string in single quotes, which has no escapes and so
/// no `"` for Windows PowerShell 5.1 to strip; a JSON (TOML basic) string only for a value with a
/// `'` or a line break in it, which there would lose its quotes (Ask passes it the same way, and
/// Codex then takes the value as a plain string).
fn toml_str(s: &str) -> String {
    if s.contains(['\'', '\n', '\r']) {
        serde_json::to_string(s).unwrap_or_default()
    } else {
        format!("'{s}'")
    }
}

/// The arguments after the program's name; none for a shell, which runs as it is. The user's own
/// MCP servers load as well as Brainstead's (no `--strict-mcp-config`). The prompt comes after
/// `--`, so one starting with `-` isn't read as an option.
pub fn launch_args(l: &Launch) -> Vec<String> {
    fn some(x: Option<&str>) -> Option<&str> {
        x.map(str::trim).filter(|x| !x.is_empty())
    }
    let s = |x: &str| x.to_string();
    let mut a: Vec<String> = Vec::new();
    let prompt = l.prompt.filter(|p| !p.trim().is_empty());
    match l.program {
        Program::Claude => {
            a.extend([s("--settings"), l.settings.to_string_lossy().into_owned()]);
            if let (Some(_), Some(cfg)) = (l.mcp, l.mcp_config) {
                a.extend([s("--mcp-config"), cfg.to_string_lossy().into_owned()]);
            }
            for (flag, v) in [("--resume", l.resume), ("--model", l.model), ("--worktree", l.worktree)] {
                if let Some(v) = some(v) {
                    a.extend([s(flag), s(v)]);
                }
            }
            if let Some(p) = prompt {
                a.extend([s("--"), s(p)]);
            }
        }
        Program::Codex => {
            a.extend([s("--sandbox"), s(if l.vault_inside { "read-only" } else { "workspace-write" })]);
            if let Some(m) = l.mcp {
                let args: Vec<String> = m.args.iter().map(|x| toml_str(x)).collect();
                let name = crate::ask::MCP_NAME;
                a.extend([
                    s("-c"),
                    format!("mcp_servers.{name}.command={}", toml_str(&m.command)),
                    s("-c"),
                    format!("mcp_servers.{name}.args=[{}]", args.join(",")),
                ]);
            }
            if let Some(m) = some(l.model) {
                a.extend([s("-m"), s(m)]);
            }
            let resume = some(l.resume);
            if resume.is_some() {
                a.push(s("resume"));
            }
            if resume.is_some() || prompt.is_some() {
                a.push(s("--"));
            }
            a.extend(resume.map(s));
            a.extend(prompt.map(s));
        }
        Program::Shell => {}
    }
    a
}

// --- events --------------------------------------------------------------------------------

/// One line of `<data>/workspace/events.jsonl`, as the hook writes it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HookEvent {
    pub t: i64,
    /// The Workspace session (`BRAINSTEAD_TERM`); None for a session in an outside terminal.
    pub term: Option<String>,
    pub event: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub tool: Option<String>,
    pub file: Option<String>,
    pub prompt: Option<String>,
    pub message: Option<String>,
}

/// The line the hook writes for Claude Code's hook input `input`.
pub fn event_from_input(input: &Value, term: Option<String>, t: i64) -> HookEvent {
    let s = |v: Option<&Value>| v.and_then(Value::as_str).filter(|x| !x.is_empty()).map(str::to_string);
    let ti = input.get("tool_input");
    HookEvent {
        t,
        term: term.filter(|x| !x.is_empty()),
        event: s(input.get("hook_event_name")).unwrap_or_default(),
        session_id: s(input.get("session_id")),
        cwd: s(input.get("cwd")),
        tool: s(input.get("tool_name")),
        file: s(ti.and_then(|t| t.get("file_path"))).or_else(|| s(ti.and_then(|t| t.get("notebook_path")))),
        prompt: s(input.get("prompt")).map(|p| p.chars().take(200).collect()),
        message: s(input.get("message")),
    }
}

/// An events line, or None for one that isn't (a half-written line, another version's).
pub fn parse_event(line: &str) -> Option<HookEvent> {
    serde_json::from_str::<HookEvent>(line.trim()).ok().filter(|e| !e.event.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Program {
    Claude,
    Codex,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Starting,
    Working,
    Waiting,
    Idle,
    Exited,
}

/// A Workspace session as the window sees it (src/workspaceApi.ts `SessionInfo`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub folder: String,
    pub name: String,
    pub program: Program,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub state: SessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub started: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectLink>,
    pub files_touched: Vec<String>,
    pub unseen: bool,
    pub external: bool,
}

impl SessionInfo {
    pub fn new(id: &str, folder: &Path, program: Program, started: i64) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            folder: folder.to_string_lossy().into_owned(),
            name: folder_name(folder),
            program,
            title: None,
            state: SessionState::Starting,
            exit_code: None,
            started,
            claude_session_id: None,
            project: None,
            files_touched: Vec::new(),
            unseen: false,
            external: false,
        }
    }
}

/// A session's title from its first prompt: the first line's first 60 characters.
pub fn title_of(prompt: &str) -> Option<String> {
    let line = prompt.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().take(60).collect())
}

/// What a state change is worth telling the user about when they aren't looking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    /// It's asking something (a permission, a question).
    Waiting,
    /// It finished its turn.
    Finished,
}

/// What a hook event does to its session: working while it answers or runs a tool, waiting when it
/// asks something, idle when its turn is done; the files it wrote, and its Claude Code session id.
/// Returns what to alert, if anything.
pub fn apply(s: &mut SessionInfo, ev: &HookEvent) -> Option<Alert> {
    if let Some(id) = &ev.session_id {
        if s.claude_session_id.as_deref() != Some(id) && (ev.event == "SessionStart" || s.claude_session_id.is_none()) {
            s.claude_session_id = Some(id.clone());
        }
    }
    if ev.event == "PostToolUse" {
        if let Some(f) = &ev.file {
            if !s.files_touched.contains(f) {
                s.files_touched.push(f.clone());
            }
        }
    }
    if s.state == SessionState::Exited {
        return None;
    }
    match ev.event.as_str() {
        "UserPromptSubmit" => {
            s.state = SessionState::Working;
            s.unseen = false;
            if s.title.is_none() {
                s.title = ev.prompt.as_deref().and_then(title_of);
            }
            None
        }
        "PreToolUse" | "PostToolUse" => {
            // Answered: what it was waiting for is no longer news.
            if s.state == SessionState::Waiting {
                s.unseen = false;
            }
            if ev.event == "PreToolUse" || s.state == SessionState::Waiting {
                s.state = SessionState::Working;
            }
            None
        }
        // Claude Code also sends one when a finished turn has sat unanswered a while: still idle.
        "Notification" if s.state == SessionState::Idle => None,
        "Notification" => {
            s.state = SessionState::Waiting;
            s.unseen = true;
            Some(Alert::Waiting)
        }
        "Stop" => {
            s.state = SessionState::Idle;
            s.unseen = true;
            Some(Alert::Finished)
        }
        "SessionStart" => {
            if s.state == SessionState::Starting {
                s.state = SessionState::Idle;
            }
            None
        }
        // A session in Brainstead ends when its process does, with its exit code.
        "SessionEnd" if s.external => {
            s.state = SessionState::Exited;
            None
        }
        _ => None,
    }
}

// --- the hook ------------------------------------------------------------------------------

/// What the hook prints for Claude Code's hook input: a refusal for a PreToolUse that would change
/// the vault, the project's context for SessionStart, nothing otherwise. Without a vault, nothing.
pub fn hook_reply(vault: Option<&Path>, input: &Value) -> Option<Value> {
    let vault = vault?;
    let cwd = input.get("cwd").and_then(Value::as_str).map(PathBuf::from).or_else(|| std::env::current_dir().ok())?;
    match input.get("hook_event_name").and_then(Value::as_str)? {
        "PreToolUse" => {
            let tool = input.get("tool_name").and_then(Value::as_str).unwrap_or("");
            let reason = guard(vault, &cwd, tool, input.get("tool_input").unwrap_or(&Value::Null))?;
            Some(json!({ "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }}))
        }
        "SessionStart" => Some(json!({ "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": session_context(vault, &cwd),
        }})),
        _ => None,
    }
}

/// The longest `session_context` may be: Claude Code shows the model all of it at every start.
const CONTEXT_MAX: usize = 1500;

/// What a guarded Claude Code session is told when it starts in `cwd`: the project the folder
/// belongs to (its name, note, status, what done looks like and how many next actions are open),
/// and how to record what it finds through Brainstead's tools. Read from the files, so it works
/// with the app closed.
pub fn session_context(vault: &Path, cwd: &Path) -> String {
    let mut lines: Vec<String> = Vec::new();
    match projects::for_folder(vault, cwd) {
        Some(p) => {
            let text = std::fs::read_to_string(vault.join(&p.path)).unwrap_or_default();
            let fm = crate::frontmatter::split(&text).data;
            let status = crate::frontmatter::string(&fm, "status").and_then(|s| projects::parse_status(&s)).unwrap_or("active");
            lines.push(format!(
                "This folder is the work of the project “{}” in the user's Brainstead vault: its note is [[{}]] ({status}).",
                p.name,
                crate::filename::stem(&p.path)
            ));
            if let Some(o) = crate::frontmatter::string(&fm, "outcome") {
                lines.push(format!("Done looks like: {}", o.chars().take(300).collect::<String>()));
            }
            let n = next_actions(&text);
            lines.push(format!("It has {n} open next action{} (list_tasks shows them).", if n == 1 { "" } else { "s" }));
        }
        None => lines
            .push("This folder isn't linked to a project in the user's Brainstead vault (a project note's `folders:` links one).".into()),
    }
    lines.extend([
        "The vault changes only through Brainstead's MCP tools (mcp__brainstead__*), so every change is in Changes with Revert: never edit it with Edit, Write or the shell.".to_string(),
        "Record findings with edit_page on the project note's ## Notes, or create_note \"Finding. <title>\" linking the project.".to_string(),
        "Add tasks with create_task linking the project, and tick them with edit_task.".to_string(),
        "Record decisions in the project note's ## Notes with edit_page, with the date and why.".to_string(),
    ]);
    let mut out = lines.join("\n");
    if out.chars().count() > CONTEXT_MAX {
        out = out.chars().take(CONTEXT_MAX - 1).collect::<String>() + "…";
    }
    out
}

/// The open tasks under a project note's `## Next actions`, as a count.
fn next_actions(text: &str) -> usize {
    let mut inside = false;
    let mut n = 0;
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            inside = h.trim().eq_ignore_ascii_case(projects::NEXT_ACTIONS);
            continue;
        }
        let t = line.trim_start();
        if inside && (t.starts_with("- [ ]") || t.starts_with("* [ ]")) && !t.contains("#waiting-for") && !t.contains("#someday-maybe") {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fx {
        _t: tempfile::TempDir,
        vault: PathBuf,
        work: PathBuf,
    }

    fn fx() -> Fx {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        let vault = base.join("vault");
        let work = base.join("repos").join("orbit-app");
        std::fs::create_dir_all(vault.join("notes")).unwrap();
        std::fs::write(vault.join("a.md"), "# A\n").unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(base.join("vault2")).unwrap();
        Fx { _t: t, vault, work }
    }

    fn bash(f: &Fx, cmd: &str) -> bool {
        guard(&f.vault, &f.work, "Bash", &json!({ "command": cmd })).is_some()
    }

    fn edit(f: &Fx, tool: &str, path: &str) -> bool {
        guard(&f.vault, &f.work, tool, &json!({ "file_path": path, "old_string": "a", "new_string": "b" })).is_some()
    }

    #[test]
    fn settings_deny_the_vault_and_run_every_hook() {
        let s = claude_settings(Path::new("/Users/maya/Vault"), Path::new("/Applications/Brainstead.app/Contents/MacOS/Brainstead"));
        assert_eq!(
            s["permissions"]["deny"],
            json!([
                "Edit(//Users/maya/Vault/**)",
                "Write(//Users/maya/Vault/**)",
                "MultiEdit(//Users/maya/Vault/**)",
                "NotebookEdit(//Users/maya/Vault/**)"
            ])
        );
        let cmd = "\"/Applications/Brainstead.app/Contents/MacOS/Brainstead\" --hook";
        assert_eq!(s["hooks"]["PreToolUse"][0]["matcher"], "Bash|Edit|Write|MultiEdit|NotebookEdit");
        assert_eq!(s["hooks"]["PreToolUse"][0]["hooks"][0], json!({ "type": "command", "command": cmd }));
        assert_eq!(s["hooks"]["PostToolUse"][0]["matcher"], "Edit|Write|MultiEdit|NotebookEdit");
        for e in ["UserPromptSubmit", "Stop", "Notification", "SessionStart"] {
            assert_eq!(s["hooks"][e][0]["hooks"][0]["command"], cmd, "{e}");
            assert!(s["hooks"][e][0].get("matcher").is_none());
        }
    }

    #[test]
    fn file_tools_are_refused_inside_the_vault_only() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        assert!(edit(&f, "Edit", &format!("{v}/a.md")));
        assert!(edit(&f, "Write", &format!("{v}/notes/new.md")));
        assert!(edit(&f, "MultiEdit", &format!("{v}/a.md")));
        assert!(guard(&f.vault, &f.work, "NotebookEdit", &json!({ "notebook_path": format!("{v}/n.ipynb") })).is_some());
        // The vault itself, and a path that only shares its prefix.
        assert!(edit(&f, "Write", &v));
        assert!(!edit(&f, "Write", &format!("{v}2/a.md")));
        assert!(!edit(&f, "Edit", &f.work.join("src/main.rs").to_string_lossy()));
        assert!(!edit(&f, "Edit", "src/main.rs"));
        // Relative, through `..`, and with `.` on the way.
        assert!(edit(&f, "Edit", "../../vault/a.md"));
        assert!(edit(&f, "Edit", "./../.././vault/notes/../a.md"));
        assert!(!edit(&f, "Edit", "../../vault2/a.md"));
        // Reading is fine; other tools aren't looked at.
        assert!(guard(&f.vault, &f.work, "Read", &json!({ "file_path": format!("{v}/a.md") })).is_none());
        assert_eq!(guard(&f.vault, &f.work, "Edit", &json!({ "file_path": format!("{v}/a.md") })).as_deref(), Some(REASON));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_into_the_vault_are_followed() {
        let f = fx();
        std::os::unix::fs::symlink(&f.vault, f.work.join("notes-link")).unwrap();
        std::os::unix::fs::symlink(f.vault.join("a.md"), f.work.join("a-link.md")).unwrap();
        std::os::unix::fs::symlink(f.vault.join("later.md"), f.work.join("dangling.md")).unwrap();
        assert!(edit(&f, "Edit", "notes-link/a.md"));
        assert!(edit(&f, "Write", "notes-link/new/deep.md"));
        assert!(edit(&f, "Edit", "a-link.md"));
        assert!(edit(&f, "Write", "dangling.md"));
        assert!(bash(&f, "echo x > notes-link/a.md"));
        // A vault set through a symlink is the same vault.
        let link = f.work.parent().unwrap().join("vault-link");
        std::os::unix::fs::symlink(&f.vault, &link).unwrap();
        assert!(guard(&link, &f.work, "Edit", &json!({ "file_path": f.vault.join("a.md") })).is_some());
        assert!(guard(&f.vault, &f.work, "Edit", &json!({ "file_path": link.join("a.md") })).is_some());
    }

    #[test]
    fn home_paths_are_expanded() {
        let Some(home) = dirs::home_dir().and_then(|h| dunce::canonicalize(h).ok()) else { return };
        let vault = home.join("brainstead-test-vault-that-isnt-there");
        let work = std::env::temp_dir();
        let g = |tool: &str, input: Value| guard(&vault, &work, tool, &input).is_some();
        assert!(g("Edit", json!({ "file_path": "~/brainstead-test-vault-that-isnt-there/a.md" })));
        assert!(g("Bash", json!({ "command": "echo x > ~/brainstead-test-vault-that-isnt-there/a.md" })));
        assert!(g("Bash", json!({ "command": "rm \"$HOME/brainstead-test-vault-that-isnt-there/a.md\"" })));
        assert!(g("Bash", json!({ "command": "python3 -c \"open('~/brainstead-test-vault-that-isnt-there/a.md','w')\"" })));
        assert!(!g("Edit", json!({ "file_path": "~/elsewhere/a.md" })));
    }

    #[test]
    fn shell_writes_into_the_vault_are_refused() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for cmd in [
            format!("echo x > {v}/a.md"),
            format!("echo x >> '{v}/a.md'"),
            format!("echo x 2> {v}/err.log"),
            format!("cat a b &> {v}/out"),
            format!("rm -rf {v}/notes"),
            format!("rm -rf {v}"),
            format!("mv {v}/a.md /tmp/a.md"),
            format!("cp README.md {v}/notes/"),
            format!("cp -t {v}/notes README.md"),
            format!("echo x | tee {v}/a.md"),
            format!("touch {v}/b.md"),
            format!("mkdir -p {v}/new"),
            format!("rmdir {v}/notes"),
            format!("ln -s /etc/hosts {v}/hosts"),
            format!("sed -i 's/a/b/' {v}/a.md"),
            format!("sed -i.bak -e 's/a/b/' {v}/a.md"),
            format!("perl -pi -e 's/a/b/' {v}/a.md"),
            format!("truncate -s 0 {v}/a.md"),
            format!("chmod 600 {v}/a.md"),
            format!("dd if=/dev/zero of={v}/a.md"),
            format!("git -C {v} commit -am wip"),
            format!("git -C {v} checkout -- a.md"),
            format!("git --git-dir={v}/.git --work-tree={v} reset --hard"),
            format!("cd {v} && git stash"),
            format!("cd {v}; rm a.md"),
            format!("cd {v}\nrm a.md"),
            format!("python3 -c \"open('{v}/a.md', 'w').write('x')\""),
            format!("node -e \"require('fs').writeFileSync('{v}/a.md', 'x')\""),
            format!("bash -c 'rm {v}/a.md'"),
            format!("sh -lc \"echo x > {v}/a.md\""),
            format!("eval rm {v}/a.md"),
            format!("echo $(rm {v}/a.md)"),
            format!("echo `rm {v}/a.md`"),
            format!("sudo rm {v}/a.md"),
            format!("FOO=1 env BAR=2 rm {v}/a.md"),
            format!("/bin/rm {v}/a.md"),
            format!("find {v} -name '*.tmp' -delete"),
            format!("find {v} -name '*.tmp' -exec rm {{}} +"),
            format!("ls {v}/*.tmp | xargs rm"),
            format!("cat <<EOF > {v}/a.md\nhello\nEOF"),
            "echo x > ../../vault/a.md".to_string(),
            "rm ../../vault/a.md".to_string(),
        ] {
            assert!(bash(&f, &cmd), "{cmd}");
        }
    }

    #[test]
    fn shell_reads_and_writes_elsewhere_are_allowed() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for cmd in [
            format!("cat {v}/a.md"),
            format!("grep -r Orbit {v}"),
            format!("ls -la {v}/notes"),
            format!("rg -n 'Maya' {v} > /tmp/hits.txt"),
            format!("head -n 5 {v}/a.md 2>/dev/null"),
            format!("cat {v}/a.md 2>&1 | less"),
            format!("git -C {v} log --oneline"),
            format!("git -C {v} status"),
            format!("git -C {v} diff"),
            format!("python3 -c \"print(open('{v}/a.md').read())\""),
            format!("find {v} -name '*.md'"),
            format!("cat {v}/a.md | xargs echo"),
            format!("echo x > {v}2/a.md"),
            format!("rm -rf {v}2"),
            format!("cd {v} && ls"),
            format!("cd {v}; cd /tmp; rm -f x.txt"),
            format!("cat <<EOF > /tmp/note.md\nrm {v}/a.md\nEOF"),
            format!("echo '# rm {v}/a.md' > notes.txt"),
            "rm -rf target".to_string(),
            "echo x > out.txt && mv out.txt build/".to_string(),
            "git commit -am 'Orbit App: fix login'".to_string(),
            "sed -i 's/a/b/' src/main.rs".to_string(),
            "npm test 2>&1 | tail -20".to_string(),
        ] {
            assert!(!bash(&f, &cmd), "{cmd}");
        }
    }

    /// Where the vault is in reach, only programs that read may run; elsewhere anything.
    #[test]
    fn only_readers_run_where_the_vault_is_in_reach() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for cmd in [
            format!("cp {v}/a.md /tmp/a.md"),
            format!("sed -n '1,5p' {v}/a.md"),
            format!("awk '{{print > \"x\"}}' {v}/a.md"),
            format!("sort -o {v}/a.md {v}/a.md"),
            format!("sort -no {v}/a.md {v}/a.md"),
            format!("uniq {v}/a.md {v}/b.md"),
            format!("jq -i . {v}/x.json"),
            format!("find {v} -fprint {v}/list"),
            format!("find {v} -execdir cat {{}} +"),
            format!("rg --pre=./x foo {v}"),
            format!("less -o {v}/log {v}/a.md"),
            format!("git -C {v} branch -D main"),
            format!("git -C {v} branch new-one"),
            format!("git -C {v} -c core.pager=x log"),
            format!("git -C {v} diff --output={v}/d.txt"),
            format!("git -C {v} gc"),
            format!("cat {v}/a.md | sh"),
            format!("ls {v} | while read f; do rm \"$f\"; done"),
            format!("env -S 'rm x' {v}/a.md"),
            format!("time -o {v}/t.txt ls"),
        ] {
            assert!(bash(&f, &cmd), "{cmd}");
        }
        for cmd in [
            format!("cat {v}/a.md | wc -l"),
            format!("ls {v} | sort | uniq -c"),
            format!("grep -rn Orbit {v} | cut -d: -f1 | sort -u > /tmp/files.txt"),
            format!("stat {v}/a.md && md5sum {v}/a.md && du -sh {v}"),
            format!("diff {v}/a.md /tmp/a.md; cmp {v}/a.md /tmp/a.md"),
            format!("test -f {v}/a.md && echo yes"),
            format!("[ -d {v}/notes ] || printf 'no\\n'"),
            format!("jq . {v}/x.json"),
            format!("tree -L 2 {v}"),
            format!("realpath {v}/a.md; readlink -f {v}; basename {v}/a.md; dirname {v}/a.md"),
            format!("git -C {v} branch"),
            format!("git -C {v} branch --list 'f*'"),
            format!("git -C {v} blame a.md"),
            format!("git -C {v} show HEAD:a.md | head"),
            format!("git -C {v} ls-files | wc -l"),
            format!("git -C {v} rev-parse HEAD; git -C {v} cat-file -p HEAD"),
            format!("cd {v} && git status && git log -3"),
            // Commands that don't mention the vault run as they are.
            "npm run build && rm -rf dist".to_string(),
            "python3 tools/gen.py > out.json".to_string(),
            "cat <<EOF | sh\necho hi\nEOF".to_string(),
            "find . -name '*.o' -delete".to_string(),
            "ls | xargs rm".to_string(),
            "D=/tmp/x; rm -rf \"$D\"".to_string(),
            "sed -i 's/a/b/' src/main.rs && cp a b".to_string(),
        ] {
            assert!(!bash(&f, &cmd), "{cmd}");
        }
    }

    #[test]
    fn here_documents_belong_to_their_command() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for cmd in [
            format!("python3 <<EOF\nopen('{v}/a.md', 'w').write('x')\nEOF"),
            format!("python3 - <<'PY'\nprint(open('{v}/a.md').read())\nPY"),
            format!("node <<EOF\nconsole.log('{v}')\nEOF"),
            format!("bash <<EOF\nrm {v}/a.md\nEOF"),
            format!("cat <<EOF | python3\nimport os; os.remove('{v}/a.md')\nEOF"),
            "python3 <<EOF\nopen('../../vault/a.md', 'w')\nEOF".to_string(),
            format!("python3 <<< \"open('{v}/a.md','w')\""),
            format!("cat <<EOF\n$(rm {v}/a.md)\nEOF"),
        ] {
            assert!(bash(&f, &cmd), "{cmd}");
        }
        for cmd in [
            format!("cat <<EOF > /tmp/x.md\nopen('{v}/a.md', 'w')\nEOF"),
            "python3 - <<'PY'\nprint(1)\nPY".to_string(),
            format!("python3 <<EOF\nprint(1)\nEOF\ncat {v}/a.md"),
        ] {
            assert!(!bash(&f, &cmd), "{cmd}");
        }
    }

    #[test]
    fn variables_count_as_the_vault_once_its_mentioned() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for cmd in [
            format!("V={v}; rm $V/a.md"),
            format!("F={v}/a.md; echo x > \"$F\""),
            format!("D={v}; cd \"$D\"; rm a.md"),
            format!("for f in {v}/*.md; do mv \"$f\" /tmp; done"),
            format!("rm $(ls {v}/*.tmp)"),
            "X=../../vault; rm -rf $X/notes".to_string(),
            "cd ..; cd ..; cd vault; rm a.md".to_string(),
            "cd ..\ncd ..\nT=vault/a.md\nrm $T".to_string(),
        ] {
            assert!(bash(&f, &cmd), "{cmd}");
        }
        for cmd in [format!("V={v}; cat $V/a.md | grep x"), format!("echo $HOME; ls {v}"), "rm -f \"$TMPDIR/x\"".to_string()] {
            assert!(!bash(&f, &cmd), "{cmd}");
        }
    }

    #[test]
    fn wrappers_with_options_find_the_real_program() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for w in [
            "nice -n 5",
            "nice -10",
            "sudo -u me",
            "sudo -E",
            "env -u X",
            "env VAR=x",
            "time -p",
            "timeout 5",
            "timeout -s KILL -k 1 5",
            "command",
            "exec",
            "nohup",
            "stdbuf -o0",
            "stdbuf -o L",
            "nice -n 5 sudo -u me timeout 5",
        ] {
            assert!(bash(&f, &format!("{w} rm {v}/a.md")), "{w}");
            assert!(!bash(&f, &format!("{w} cat {v}/a.md")), "{w}");
        }
        assert!(bash(&f, &format!("ls {v} | xargs -I {{}} rm {{}}")));
        assert!(bash(&f, &format!("ls {v} | xargs -n 1 -P 4 rm")));
        assert!(!bash(&f, &format!("ls {v} | xargs -I {{}} cat {{}}")));
        assert!(!bash(&f, &format!("ls {v} | xargs")));
    }

    #[test]
    fn unsure_folders_are_unknown_not_the_vault() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        // `cd -` goes back where it was; with nowhere known to go back to, the folder is unknown.
        assert!(!bash(&f, &format!("cd {v}; cd -; rm a.md")));
        assert!(!bash(&f, "cd -; rm a.md"));
        assert!(!bash(&f, "popd; rm a.md"));
        assert!(!bash(&f, &format!("pushd {v}; popd; rm a.md")));
        assert!(bash(&f, &format!("cd {v}; pushd /tmp; popd; rm a.md")));
        assert!(!bash(&f, "cd \"$(git rev-parse --show-toplevel)\" && rm -rf target"));
        // Already in the vault: only readers.
        let g = |cmd: &str| guard(&f.vault, &f.vault.join("notes"), "Bash", &json!({ "command": cmd })).is_some();
        assert!(g("rm a.md"));
        assert!(g("python3 x.py"));
        assert!(!g("cat ../a.md && ls"));
        assert!(!g("cd /tmp && rm -f x"));
    }

    #[test]
    fn interpreters_code_is_read_for_writes() {
        let f = fx();
        let v = f.vault.to_string_lossy().into_owned();
        for cmd in [
            format!("python3 -c \"import sys; sys.stdout.write(open('{v}/a.md').read())\""),
            format!("python3 -c \"print(open('{v}/a.md', encoding='utf-8').read())\""),
            format!("node -e \"console.log(require('fs').readFileSync('{v}/a.md', 'utf8'))\""),
            format!("perl -MList::Util=sum -ne 'print if /x/' {v}/a.md"),
            format!("perl -Mstrict -lne 'print length' {v}/a.md"),
            format!("ruby -e 'puts File.read(ARGV[0])' {v}/a.md"),
        ] {
            assert!(!bash(&f, &cmd), "{cmd}");
        }
        for cmd in [
            format!("python3 -c \"from pathlib import Path; Path('{v}/a.md').write_text('x')\""),
            format!("python3 -c \"import shutil; shutil.copy('/tmp/a', '{v}/a.md')\""),
            format!("python3 -c \"import os; os.remove('{v}/a.md')\""),
            format!("python3 -c \"import os; os.makedirs('{v}/new')\""),
            format!("python3 -c \"import subprocess; subprocess.run(['ls', '{v}'])\""),
            format!("python3 -c \"open('{v}/a.md', mode='a')\""),
            format!("python3 -c \"open('{v}/a.md', 'r+')\""),
            format!("python3 script.py {v}"),
            format!("python3 -m http.server -d {v}"),
            format!("perl -i -pe 's/a/b/' {v}/a.md"),
            format!("perl -e 'unlink \"{v}/a.md\"'"),
            format!("perl -e 'open(my $f, \">\", \"{v}/a.md\")'"),
            format!("ruby -e 'File.write(\"{v}/a.md\", \"x\")'"),
            format!("node -e \"require('fs').rmSync('{v}/notes', {{ recursive: true }})\""),
            format!("osascript -e 'do shell script \"rm {v}/a.md\"'"),
            format!("deno eval \"Deno.writeTextFileSync('{v}/a.md', 'x')\""),
        ] {
            assert!(bash(&f, &cmd), "{cmd}");
        }
    }

    /// `link/..` is the folder the link's target is in, as the system resolves it.
    #[cfg(unix)]
    #[test]
    fn symlinks_are_followed_before_dot_dot() {
        let f = fx();
        // work/into-notes -> vault/notes: `into-notes/../a.md` is vault/a.md, not work/a.md.
        std::os::unix::fs::symlink(f.vault.join("notes"), f.work.join("into-notes")).unwrap();
        assert!(edit(&f, "Write", "into-notes/../a.md"));
        assert!(bash(&f, "rm into-notes/../a.md"));
        assert_eq!(resolve(&f.work, "into-notes/../a.md"), f.vault.join("a.md"));
        // vault/notes/out -> work: `out/../x` is beside work, outside the vault.
        std::os::unix::fs::symlink(&f.work, f.vault.join("notes/out")).unwrap();
        let v = f.vault.to_string_lossy().into_owned();
        assert!(!edit(&f, "Write", &format!("{v}/notes/out/../x.md")));
        assert!(edit(&f, "Write", &format!("{v}/notes/out/../../vault/a.md")));
        // Past a part that isn't there, `..` is taken as written.
        assert!(edit(&f, "Write", &format!("{v}/nope/../a.md")));
        assert!(!edit(&f, "Write", &format!("{v}/nope/../../elsewhere.md")));
    }

    /// macOS's and Windows' file systems ignore case, and so does the guard there.
    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn case_is_ignored_where_the_file_system_ignores_it() {
        let f = fx();
        let upper = f.vault.to_string_lossy().to_uppercase();
        assert!(bash(&f, &format!("rm {upper}/A.MD")));
        assert!(edit(&f, "Write", &format!("{upper}/A.MD")));
    }

    #[cfg(windows)]
    #[test]
    fn either_slash_on_windows() {
        let f = fx();
        let fwd = f.vault.to_string_lossy().replace('\\', "/");
        assert!(bash(&f, &format!("rm {fwd}/a.md")));
        assert!(bash(&f, &format!("python -c \"open('{fwd}/a.md', 'w')\"")));
        assert!(edit(&f, "Write", &format!("{fwd}/a.md")));
    }

    #[test]
    fn the_hook_refuses_with_the_deny_answer_and_needs_a_vault() {
        let f = fx();
        let input = json!({
            "hook_event_name": "PreToolUse",
            "session_id": "s1",
            "cwd": f.work,
            "tool_name": "Write",
            "tool_input": { "file_path": f.vault.join("a.md") },
        });
        let r = hook_reply(Some(&f.vault), &input).unwrap();
        assert_eq!(r["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(r["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(r["hookSpecificOutput"]["permissionDecisionReason"], REASON);
        assert!(hook_reply(None, &input).is_none());
        let ok = json!({ "hook_event_name": "PreToolUse", "cwd": f.work, "tool_name": "Write", "tool_input": { "file_path": "src/a.rs" } });
        assert!(hook_reply(Some(&f.vault), &ok).is_none());
        assert!(hook_reply(Some(&f.vault), &json!({ "hook_event_name": "Stop", "cwd": f.work })).is_none());
    }

    #[test]
    fn vault_from_the_settings_file() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(vault_from_settings(t.path()), None);
        std::fs::write(t.path().join("settings.json"), r#"{"vaultPath": "  "}"#).unwrap();
        assert_eq!(vault_from_settings(t.path()), None);
        std::fs::write(t.path().join("settings.json"), r#"{"vaultPath": "/Users/maya/Vault", "theme": "dark"}"#).unwrap();
        assert_eq!(vault_from_settings(t.path()), Some(PathBuf::from("/Users/maya/Vault")));
    }

    fn git_repo(dir: &Path, head: &str) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), head).unwrap();
    }

    #[test]
    fn folders_are_roots_their_repos_then_pinned_and_recent() {
        let f = fx();
        let repos = f.work.parent().unwrap().to_path_buf();
        git_repo(&f.work, "ref: refs/heads/main\n");
        git_repo(&repos.join("acme").join("billing"), "3f2a9c1d0e\n");
        std::fs::create_dir_all(repos.join("acme").join("docs")).unwrap();
        git_repo(&repos.join("acme").join("deep").join("three"), "ref: refs/heads/x\n");
        git_repo(&repos.join(".hidden"), "ref: refs/heads/x\n");
        // A worktree's `.git` is a file naming its git folder.
        let wt = repos.join("orbit-wt");
        std::fs::create_dir_all(f.work.join(".git/worktrees/wt")).unwrap();
        std::fs::write(f.work.join(".git/worktrees/wt/HEAD"), "ref: refs/heads/feature/login\n").unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", f.work.join(".git/worktrees/wt").display())).unwrap();
        let notes = f.vault.join("notes");
        let elsewhere = f.vault.parent().unwrap().join("vault2");
        let r = repos.to_string_lossy().into_owned();
        let got = discover(
            std::slice::from_ref(&r),
            &[f.work.to_string_lossy().into_owned(), notes.to_string_lossy().into_owned()],
            &[(elsewhere.to_string_lossy().into_owned(), 1_700_000_000_000), (f.work.to_string_lossy().into_owned(), 1_600_000_000_000)],
            Some(&f.vault),
        );
        #[allow(clippy::type_complexity)]
        let rows: Vec<(&str, &str, Option<&str>, bool, Option<i64>, bool)> =
            got.iter().map(|x| (x.name.as_str(), x.kind.as_str(), x.branch.as_deref(), x.pinned, x.recent, x.inside_vault)).collect();
        assert_eq!(
            rows,
            [
                ("repos", "root", None, false, None, false),
                ("billing", "repo", Some("3f2a9c1"), false, None, false),
                ("orbit-app", "repo", Some("main"), true, Some(1_600_000_000_000), false),
                ("orbit-wt", "repo", Some("feature/login"), false, None, false),
                ("notes", "folder", None, true, None, true),
                ("vault2", "folder", None, false, Some(1_700_000_000_000), false),
            ]
        );
        assert_eq!(got[1].root.as_deref(), Some(r.as_str()));
        let j = serde_json::to_value(&got[4]).unwrap();
        assert_eq!(j["insideVault"], true);
        assert!(j.get("dirty").is_none() && j.get("root").is_none());
    }

    #[test]
    fn recent_folders_are_newest_first_and_twenty_at_most() {
        let list: Vec<String> = (0..20).map(|i| format!("/r/{i}")).collect();
        let b = bump(&list, "/r/5", 20);
        assert_eq!(b[0], "/r/5");
        assert_eq!(b.len(), 20);
        assert_eq!(b.iter().filter(|p| *p == "/r/5").count(), 1);
        let b = bump(&list, "/new", 20);
        assert_eq!((b[0].as_str(), b.len(), b.last().unwrap().as_str()), ("/new", 20, "/r/18"));
    }

    #[test]
    fn in_vault_is_the_vault_and_below() {
        let f = fx();
        assert!(in_vault(&f.vault, &f.vault));
        assert!(in_vault(&f.vault, &f.vault.join("notes")));
        assert!(in_vault(&f.vault, &f.vault.join("notes").join("..").join("notes")));
        assert!(!in_vault(&f.vault, &f.work));
        assert!(!in_vault(&f.vault, &f.vault.parent().unwrap().join("vault2")));
    }

    #[test]
    fn a_session_starts_knowing_its_project() {
        let f = fx();
        std::fs::write(
            f.vault.join("Project. Orbit App.md"),
            "---\nstatus: active\noutcome: Orbit App's login works for Maya and Lena\n---\n\n## Next actions\n\n- [ ] Fix login\n- [ ] Ask Acme #waiting-for\n- [x] Old\n- [ ] Write tests\n\n## Notes\n\n- [ ] not a next action\n",
        )
        .unwrap();
        let c = session_context(&f.vault, &f.work);
        assert!(c.contains("“Orbit App”") && c.contains("[[Project. Orbit App]] (active)"), "{c}");
        assert!(c.contains("Done looks like: Orbit App's login works for Maya and Lena"), "{c}");
        assert!(c.contains("It has 2 open next actions"), "{c}");
        assert!(c.contains("create_task") && c.contains("edit_page"), "{c}");
        assert!(c.chars().count() <= CONTEXT_MAX);
        let other = f.vault.parent().unwrap().join("vault2");
        assert!(session_context(&f.vault, &other).contains("isn't linked to a project"));
        let r = hook_reply(Some(&f.vault), &json!({ "hook_event_name": "SessionStart", "cwd": f.work })).unwrap();
        assert_eq!(r["hookSpecificOutput"]["hookEventName"], "SessionStart");
        assert_eq!(r["hookSpecificOutput"]["additionalContext"], c);
        // The folder list links it too.
        git_repo(&f.work, "ref: refs/heads/main\n");
        let got = discover(&[f.work.parent().unwrap().to_string_lossy().into_owned()], &[], &[], Some(&f.vault));
        let orbit = got.iter().find(|x| x.name == "orbit-app").unwrap();
        assert_eq!(orbit.project.as_ref().map(|p| p.name.as_str()), Some("Orbit App"));
    }

    #[test]
    fn launch_lines_for_each_program() {
        let mcp = crate::ask::Mcp { command: "/app/Brainstead".into(), args: vec!["--mcp".into(), "--data".into(), "/d".into()] };
        let base = Launch {
            program: Program::Claude,
            settings: Path::new("/d/workspace/claude-settings.json"),
            mcp: Some(&mcp),
            mcp_config: Some(Path::new("/d/workspace/mcp.json")),
            resume: None,
            prompt: None,
            model: None,
            worktree: None,
            vault_inside: false,
        };
        let a = launch_args(&base);
        assert_eq!(a, ["--settings", "/d/workspace/claude-settings.json", "--mcp-config", "/d/workspace/mcp.json"]);
        assert_eq!(mcp_config(&mcp)["mcpServers"]["brainstead"]["args"], json!(["--mcp", "--data", "/d"]));
        assert!(!a.iter().any(|x| x == "--strict-mcp-config" || x.contains('"')));
        let a = launch_args(&Launch {
            resume: Some("abc"),
            model: Some("opus"),
            worktree: Some("login"),
            prompt: Some("-v is broken"),
            ..base
        });
        assert_eq!(a[4..], ["--resume", "abc", "--model", "opus", "--worktree", "login", "--", "-v is broken"]);
        let a = launch_args(&Launch { resume: Some(" "), prompt: Some(""), ..base });
        assert_eq!(a.len(), 4);
        let codex = Launch { program: Program::Codex, resume: Some("r1"), prompt: Some("Go on"), ..base };
        let a = launch_args(&codex);
        assert_eq!(a[..2], ["--sandbox", "workspace-write"]);
        assert!(a.contains(&"mcp_servers.brainstead.command='/app/Brainstead'".to_string()));
        assert!(a.contains(&"mcp_servers.brainstead.args=['--mcp','--data','/d']".to_string()));
        // No `"` for PowerShell 5.1 to strip.
        assert!(!a.iter().any(|x| x.contains('"')), "{a:?}");
        assert_eq!(a[a.len() - 4..], ["resume", "--", "r1", "Go on"]);
        let b = launch_args(&Launch { resume: None, ..codex });
        assert_eq!(b[b.len() - 2..], ["--", "Go on"]);
        assert!(!b.contains(&"resume".to_string()));
        assert_eq!(launch_args(&Launch { vault_inside: true, ..codex })[1], "read-only");
        assert_eq!(toml_str(r"C:\Program Files\Brainstead.exe"), r"'C:\Program Files\Brainstead.exe'");
        assert_eq!(toml_str("it's"), r#""it's""#);
        assert!(launch_args(&Launch { program: Program::Shell, ..base }).is_empty());
    }

    fn ev(event: &str) -> HookEvent {
        HookEvent { t: 1, event: event.into(), session_id: Some("c-1".into()), ..Default::default() }
    }

    #[test]
    fn events_lines_round_trip() {
        let input = json!({
            "hook_event_name": "PostToolUse",
            "session_id": "abc",
            "cwd": "/repos/orbit-app",
            "transcript_path": "/x.jsonl",
            "tool_name": "Write",
            "tool_input": { "file_path": "/repos/orbit-app/src/a.rs", "content": "x" },
            "prompt": "p".repeat(300),
        });
        let e = event_from_input(&input, Some("term-1".into()), 42);
        assert_eq!(e.prompt.as_ref().unwrap().len(), 200);
        assert_eq!(e.file.as_deref(), Some("/repos/orbit-app/src/a.rs"));
        let line = serde_json::to_string(&e).unwrap();
        assert!(line.contains("\"sessionId\":\"abc\""));
        assert_eq!(parse_event(&line), Some(e));
        let outside = event_from_input(&json!({ "hook_event_name": "Stop", "session_id": "abc" }), None, 1);
        assert!(serde_json::to_string(&outside).unwrap().contains("\"term\":null"));
        let nb = event_from_input(
            &json!({ "hook_event_name": "PostToolUse", "tool_input": { "notebook_path": "/n.ipynb" } }),
            Some(String::new()),
            1,
        );
        assert_eq!((nb.file.as_deref(), nb.term), (Some("/n.ipynb"), None));
        assert_eq!(parse_event("{\"t\": 1, \"ev"), None);
        assert_eq!(parse_event("{}"), None);
    }

    #[test]
    fn a_session_follows_its_hooks() {
        let mut s = SessionInfo::new("term-1", Path::new("/repos/orbit-app"), Program::Claude, 0);
        assert_eq!(apply(&mut s, &ev("SessionStart")), None);
        assert_eq!((s.state, s.claude_session_id.as_deref()), (SessionState::Idle, Some("c-1")));
        let prompt = HookEvent {
            prompt: Some("\n  Fix the login bug in Orbit App, which Maya reported on Monday morning\nmore".into()),
            ..ev("UserPromptSubmit")
        };
        apply(&mut s, &prompt);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.title.as_deref(), Some("Fix the login bug in Orbit App, which Maya reported on Monda"));
        // A later prompt doesn't rename it.
        apply(&mut s, &HookEvent { prompt: Some("And the tests".into()), ..ev("UserPromptSubmit") });
        assert!(s.title.unwrap().starts_with("Fix"));
        s.title = Some("Fix".into());
        assert_eq!(apply(&mut s, &ev("Notification")), Some(Alert::Waiting));
        assert_eq!((s.state, s.unseen), (SessionState::Waiting, true));
        apply(&mut s, &ev("PreToolUse"));
        assert_eq!((s.state, s.unseen), (SessionState::Working, false));
        let write = |f: &str| HookEvent { file: Some(f.into()), ..ev("PostToolUse") };
        apply(&mut s, &write("/repos/orbit-app/src/a.rs"));
        apply(&mut s, &write("/repos/orbit-app/src/a.rs"));
        apply(&mut s, &write("/repos/orbit-app/src/b.rs"));
        assert_eq!(s.files_touched, ["/repos/orbit-app/src/a.rs", "/repos/orbit-app/src/b.rs"]);
        assert_eq!(apply(&mut s, &ev("Stop")), Some(Alert::Finished));
        assert_eq!((s.state, s.unseen), (SessionState::Idle, true));
        // The "waiting for your input" reminder after a finished turn leaves it idle.
        assert_eq!(apply(&mut s, &ev("Notification")), None);
        assert_eq!(s.state, SessionState::Idle);
        // /clear starts a new Claude Code session in the same terminal.
        apply(&mut s, &HookEvent { session_id: Some("c-2".into()), ..ev("SessionStart") });
        assert_eq!(s.claude_session_id.as_deref(), Some("c-2"));
        apply(&mut s, &HookEvent { session_id: Some("c-3".into()), ..ev("Stop") });
        assert_eq!(s.claude_session_id.as_deref(), Some("c-2"));
        // An ended session stays ended; SessionEnd ends only an outside one.
        apply(&mut s, &ev("SessionEnd"));
        assert_eq!(s.state, SessionState::Idle);
        s.state = SessionState::Exited;
        apply(&mut s, &ev("UserPromptSubmit"));
        assert_eq!(s.state, SessionState::Exited);
        let mut x = SessionInfo { external: true, ..SessionInfo::new("c-9", Path::new("/r"), Program::Claude, 0) };
        apply(&mut x, &ev("SessionEnd"));
        assert_eq!(x.state, SessionState::Exited);
    }

    #[test]
    fn session_info_has_the_windows_shape() {
        let mut s = SessionInfo::new("term-1", Path::new("/repos/orbit-app"), Program::Codex, 5);
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["program"], "codex");
        assert_eq!(j["state"], "starting");
        assert_eq!(j["name"], "orbit-app");
        assert_eq!(j["filesTouched"], json!([]));
        assert!(j.get("title").is_none() && j.get("exitCode").is_none());
        s.exit_code = Some(0);
        s.state = SessionState::Exited;
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!((j["exitCode"].as_i64(), j["state"].as_str()), (Some(0), Some("exited")));
        let p: Program = serde_json::from_value(json!("shell")).unwrap();
        assert_eq!(p, Program::Shell);
    }
}
