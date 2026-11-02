// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ask: the command line for each AI CLI (Claude Code, Codex, Antigravity, GitHub Copilot, and
//! Codex again for Ollama's local models) and the reading of what it streams back, ported from a
//! sibling app's assistant. Nothing here
//! starts a process, so every CLI's flags and stream are tested from recorded lines. The app
//! crate (src-tauri/src/ask.rs) finds the CLIs, runs them and sends the events to the window.
//!
//! Every CLI runs with read and search tools only, never with permission checks skipped: no model
//! writes to the vault (its writes come through Brainstead's MCP tools, recorded in Changes).

use serde::Serialize;

/// Which CLI answers, from the model id: `agy:` Antigravity, `copilot:` Copilot, `ollama:` a model
/// Ollama serves on this computer (run by Codex, pointed at Ollama), `claude…` Claude Code
/// (`claude:sonnet` or a full id such as `claude-sonnet-5`), the rest Codex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cli {
    Claude,
    Codex,
    Antigravity,
    Copilot,
    /// Ollama's local models, through Codex's own Ollama provider (`--oss`): Codex's agent loop,
    /// sandbox and sessions, Ollama's model. Its own entry so it has its own settings key
    /// (`effort.ollama`) and row under Found on this computer.
    Ollama,
}

pub const CLAUDE_PREFIX: &str = "claude:";
pub const AGY_PREFIX: &str = "agy:";
pub const COPILOT_PREFIX: &str = "copilot:";
/// Copilot's Auto, which picks a model per request and fails any run given a reasoning effort.
pub const COPILOT_AUTO: &str = "copilot:auto";
pub const OLLAMA_PREFIX: &str = "ollama:";
/// Antigravity's agent, written into its working folder for each run.
pub const AGY_AGENT: &str = "brainstead-ask";

impl Cli {
    pub fn route(model: &str) -> Cli {
        if model.starts_with(AGY_PREFIX) {
            Cli::Antigravity
        } else if model.starts_with(COPILOT_PREFIX) {
            Cli::Copilot
        } else if model.starts_with(OLLAMA_PREFIX) {
            Cli::Ollama
        } else if model.starts_with("claude") {
            Cli::Claude
        } else {
            Cli::Codex
        }
    }
    /// The program's name, as installed.
    pub fn bin(self) -> &'static str {
        match self {
            Cli::Claude => "claude",
            Cli::Codex | Cli::Ollama => "codex",
            Cli::Antigravity => "agy",
            Cli::Copilot => "copilot",
        }
    }
    /// Its key in the settings (`effort.<key>`) and to the window, as serde names it.
    pub fn key(self) -> &'static str {
        match self {
            Cli::Claude => "claude",
            Cli::Codex => "codex",
            Cli::Antigravity => "antigravity",
            Cli::Copilot => "copilot",
            Cli::Ollama => "ollama",
        }
    }
    /// The name people know it by, for messages.
    pub fn label(self) -> &'static str {
        match self {
            Cli::Claude => "Claude Code",
            Cli::Codex => "Codex",
            Cli::Antigravity => "Antigravity",
            Cli::Copilot => "GitHub Copilot",
            Cli::Ollama => "Ollama",
        }
    }
    /// Where installers put it under the home folder, besides `~/.local/bin` (`.local\bin\<name>.exe`
    /// on Windows, where npm's `%APPDATA%\npm\<name>.cmd` is looked at too).
    pub fn extra_paths(self) -> &'static [&'static str] {
        match self {
            #[cfg(not(windows))]
            Cli::Claude => &[".claude/local/claude"],
            // Claude Code's local install: npm's shim beside its own node_modules.
            #[cfg(windows)]
            Cli::Claude => &[".claude/local/claude.cmd", ".claude/local/node_modules/.bin/claude.cmd"],
            _ => &[],
        }
    }
    /// How to search with its own tools, added to the instructions.
    fn tools_hint(self) -> &'static str {
        match self {
            Cli::Claude => "Use Grep and Glob to find the notes that matter and Read only what you need; several Read or Grep calls can go in one turn.",
            Cli::Codex | Cli::Ollama => "Search with rg and print only the relevant lines, covering several files in one command where you can. Read nothing outside the vault.",
            Cli::Antigravity => "Search with grep_search, find files with find_by_name, and open them with view_file, several at once where you can. Read nothing outside the vault.",
            Cli::Copilot => "Search with grep, find files with glob, and open them with view, several at once where you can. Read nothing outside the vault.",
        }
    }
}

/// The MCP server's name, as each CLI is told it.
pub const MCP_NAME: &str = "brainstead";
/// Its tools, for the CLIs that list each one (Copilot).
pub const MCP_TOOLS: [&str; 87] = [
    "search",
    "read_section",
    "backlinks",
    "resolve_entity",
    "facts",
    "pending_sources",
    "provenance",
    "lint",
    "help",
    "list_tasks",
    "edit_task",
    "move_task",
    "create_task",
    "delete_task",
    "list_inbox",
    "clarify_inbox",
    "capture",
    "list_projects",
    "create_project",
    "update_project",
    "move_project",
    "rename_area",
    "edit_page",
    "create_note",
    "add_image",
    "rename_note",
    "trash_note",
    "list_trash",
    "restore_from_trash",
    "list_bookmarks",
    "bookmarks",
    "list_saved_searches",
    "saved_searches",
    "list_changes",
    "changes",
    "start_run",
    "run_status",
    "stop_run",
    "summary",
    "weekly_review",
    "weekly_suggestion",
    "weekly_start_over",
    "weekly_step",
    "save_chat",
    "list_chats",
    "chats",
    "chat_effort",
    "list_transcripts",
    "fix_health",
    "ignore_issue",
    "health_issue",
    "list_contradictions",
    "contradictions",
    "page_shape",
    "reshape_pages",
    "write_current_state",
    "fix_name",
    "triage_bookmarks",
    "draft_reply",
    "doc_check",
    "compare_docs",
    "activity",
    "graph",
    "list_suggestions",
    "suggestions",
    "list_moving_over",
    "moving_over",
    "list_automated_tools",
    "automated_tools",
    "list_task_lists",
    "task_lists",
    "list_settings",
    "settings",
    "list_note_looks",
    "note_look",
    "import_sources",
    "create_template",
    "open",
    "add_example_notes",
    "capture_extensions",
    "teams_sync",
    "pull_teams_outlook",
    "glance",
    "app_status",
    "list_assistants",
    "rebuild_index",
    "check_updates",
];

/// How to start Brainstead's MCP server (stage 7a): the app's own binary with `--mcp …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mcp {
    pub command: String,
    pub args: Vec<String>,
}

impl Cli {
    /// Whether Ask gives it Brainstead's MCP server. Antigravity's agents take no MCP servers of
    /// their own, so it reads with its own tools only.
    pub fn takes_mcp(self) -> bool {
        self != Cli::Antigravity
    }
    /// Whether it's shown a request's images: Claude Code reads them with Read, Codex has them
    /// attached. Copilot and Antigravity take none from a script, so they get an image's text only;
    /// nor does an Ollama model, since few of them see pictures and Codex fails the turn on one that
    /// doesn't.
    pub fn sees_images(self) -> bool {
        matches!(self, Cli::Claude | Cli::Codex)
    }
    /// The effort levels Settings › AI assistants offers for it, lowest first, as its CLI names
    /// them: Claude Code's and Antigravity's `--effort`, Codex's `model_reasoning_effort`,
    /// Copilot's `--reasoning-effort`. Which a model takes is up to the CLI: Claude Code runs a
    /// level a model lacks at the highest it has below it, the others may refuse it. Ollama takes
    /// Codex's `model_reasoning_effort` as a thinking level by name, where a model names its
    /// levels (gpt-oss: low, medium, high; `ollama::Show::efforts` says which); a model that only
    /// thinks or not ignores the name, so none is offered for it.
    pub fn efforts(self) -> &'static [&'static str] {
        match self {
            Cli::Codex => &["low", "medium", "high", "xhigh"],
            Cli::Ollama => &["low", "medium", "high"],
            Cli::Claude | Cli::Antigravity | Cli::Copilot => &["low", "medium", "high", "xhigh", "max"],
        }
    }
}

/// Ollama's server, as Brainstead and Codex reach it. Nothing here makes a request: the app crate
/// does (src-tauri/src/ollama.rs) and hands the replies here to read.
pub mod ollama {
    /// Where to read how to install it (How to install).
    pub const INSTALL_URL: &str = "https://ollama.com/download";
    /// The host Ollama's own tools use when `OLLAMA_HOST` is unset.
    pub const DEFAULT_HOST: &str = "http://localhost:11434";

    /// The server's base URL from `OLLAMA_HOST` (the variable Ollama's own tools read), as Ollama
    /// reads it: the scheme is optional (http), so is the port (11434), and a bare port or
    /// `0.0.0.0` (the server listening on every interface) means this computer. None or an empty
    /// value is the default host. Never ends in a slash.
    pub fn host(var: Option<&str>) -> String {
        let v = var.map(str::trim).unwrap_or_default();
        if v.is_empty() {
            return DEFAULT_HOST.to_string();
        }
        let (scheme, rest) = match v.split_once("://") {
            Some((s, r)) => (s.to_string(), r),
            None => ("http".to_string(), v),
        };
        let rest = rest.trim_end_matches('/');
        // `host:port`, `:port`, `host`, `[::1]:port`.
        let (mut h, p) = match rest.rsplit_once(':') {
            Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, Some(p)),
            _ => (rest, None),
        };
        if h.is_empty() || h == "0.0.0.0" {
            h = "localhost";
        }
        let port = p.unwrap_or("11434");
        format!("{scheme}://{h}:{port}")
    }

    /// Codex's base URL for the Ollama provider at `host` (`CODEX_OSS_BASE_URL`): its
    /// OpenAI-compatible endpoint.
    pub fn codex_base_url(host: &str) -> String {
        format!("{}/v1", host.trim_end_matches('/'))
    }

    /// A model Ollama has pulled, from `GET /api/tags`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Tag {
        pub name: String,
        /// "4.0B", when the server says.
        pub parameters: Option<String>,
    }

    /// The models in a `GET /api/tags` reply, in the server's order.
    pub fn parse_tags(reply: &serde_json::Value) -> Vec<Tag> {
        reply["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let name = m["name"].as_str().or(m["model"].as_str())?.to_string();
                let parameters = m["details"]["parameter_size"].as_str().filter(|p| !p.is_empty()).map(str::to_string);
                Some(Tag { name, parameters })
            })
            .collect()
    }

    /// The server's version from `GET /api/version`.
    pub fn parse_version(reply: &serde_json::Value) -> Option<String> {
        reply["version"].as_str().filter(|v| !v.is_empty()).map(str::to_string)
    }

    /// What `POST /api/show` says a model can do.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct Show {
        /// It calls tools, which Codex's agent loop (and Brainstead's MCP tools) need.
        pub tools: bool,
        /// Its thinking levels by name (gpt-oss: low, medium, high), in the server's order; empty
        /// when it only thinks or not (`thinking.values` of true and false), or doesn't think.
        pub levels: Vec<String>,
    }

    impl Show {
        /// The effort levels Brainstead offers for the model: the ones it names that Codex's
        /// `model_reasoning_effort` has too, lowest first as `Cli::Ollama.efforts()` orders them.
        pub fn efforts(&self) -> Vec<String> {
            super::Cli::Ollama.efforts().iter().filter(|l| self.levels.iter().any(|v| v == *l)).map(|l| l.to_string()).collect()
        }
    }

    pub fn parse_show(reply: &serde_json::Value) -> Show {
        let tools = reply["capabilities"].as_array().into_iter().flatten().filter_map(|c| c.as_str()).any(|c| c == "tools");
        let levels = reply["thinking"]["values"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).map(str::to_string).collect();
        Show { tools, levels }
    }
}

/// The effort a run on `cli` goes at: the job's own (Settings › AI assistants › Models by job,
/// `jobEffort.<job>`), else the assistant's (`effort.<cli>`), each only when the CLI offers it;
/// None leaves it to the CLI's own default for the model. A new Ask chat starts from it with no
/// job's, so a job's level never becomes a chat's. src/askState.ts `resolveEffort` is the same
/// rule, kept beside this one by the same cases in both tests.
pub fn resolve_effort<'a>(cli: Cli, job: Option<&'a str>, assistant: Option<&'a str>) -> Option<&'a str> {
    [job, assistant].into_iter().flatten().find(|e| cli.efforts().contains(e))
}

/// A background job, by its key in Settings › AI assistants › Models by job (`jobModels.<key>`,
/// `jobEffort.<key>`); src/askState.ts `MODEL_JOBS` lists the same keys, held together by
/// tests/fixtures/job-settings.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// The daily and weekly summaries.
    Summaries,
    /// The weekly review's preparation.
    Weekprep,
    Find,
    /// Ingests, the daily check's too.
    Ingest,
    /// Meeting notes from transcripts.
    Meeting,
    /// Clarify's suggestions in the Inbox.
    Clarify,
    /// Contradiction checks, the daily check's and Write Current state.
    Contradictions,
    /// Triage, Draft reply and Doc check.
    Skills,
    /// Ask's next-message suggestions.
    Suggest,
}

impl Job {
    pub const ALL: [Job; 9] =
        [Job::Summaries, Job::Weekprep, Job::Find, Job::Ingest, Job::Meeting, Job::Clarify, Job::Contradictions, Job::Skills, Job::Suggest];
    pub fn key(self) -> &'static str {
        match self {
            Job::Summaries => "summaries",
            Job::Weekprep => "weekprep",
            Job::Find => "find",
            Job::Ingest => "ingest",
            Job::Meeting => "meeting",
            Job::Clarify => "clarify",
            Job::Contradictions => "contradictions",
            Job::Skills => "skills",
            Job::Suggest => "suggest",
        }
    }
}

type Ui = serde_json::Map<String, serde_json::Value>;

/// A setting's text, an empty one counting as unset.
fn text(v: Option<&serde_json::Value>) -> Option<&str> {
    v.and_then(serde_json::Value::as_str).filter(|v| !v.is_empty())
}

/// The model `job` (a `Job` key) runs on, from the window's settings: its own (`jobModels.<job>`),
/// else, for the weekly review's preparation, the summaries', else new chats' (`askModel`), else
/// Claude's Sonnet. src/askState.ts `jobModelOf` is the same rule; both tests read
/// tests/fixtures/job-settings.json.
pub fn job_model(ui: &Ui, job: &str) -> String {
    let own = |j: &str| text(ui.get("jobModels").and_then(|m| m.get(j)));
    own(job)
        .or_else(|| if job == "weekprep" { own("summaries") } else { None })
        .or_else(|| text(ui.get("askModel")))
        .unwrap_or("claude:sonnet")
        .to_string()
}

/// The job's own effort (`jobEffort.<job>`) while its model's assistant offers it, else None, as
/// Models by job and list_settings show it (src/askState.ts `jobOwnEffort`).
pub fn job_own_effort<'a>(ui: &'a Ui, job: &str) -> Option<&'a str> {
    let cli = Cli::route(&job_model(ui, job));
    text(ui.get("jobEffort").and_then(|m| m.get(job))).filter(|e| cli.efforts().contains(e))
}

/// The effort `job` runs at on `model` (`resolve_effort`): its own while `model` is the job's
/// (`job_model`), else its assistant's (`effort.<cli>`), each only when the assistant offers it. A
/// job's level goes with the model it was picked beside: a quick pass on another model (the
/// contradiction check's extract and Write Current state, on Claude's Haiku) runs at the
/// assistant's. The weekly review's preparation never takes the summaries' effort, even on their model.
pub fn job_effort<'a>(ui: &'a Ui, job: &str, model: &str) -> Option<&'a str> {
    let cli = Cli::route(model);
    let own = text(ui.get("jobEffort").and_then(|m| m.get(job))).filter(|_| job_model(ui, job) == model);
    resolve_effort(cli, own, text(ui.get("effort").and_then(|m| m.get(cli.key()))))
}

/// `level` as a model whose own levels are `model_levels` takes it (None: its CLI doesn't list a
/// model's levels, so the CLI decides): the level itself, else the highest of the CLI's levels
/// below it that the model has, else none, the model's own default.
pub fn fit_effort<'a>(cli: Cli, level: &'a str, model_levels: Option<&[String]>) -> Option<&'a str> {
    let Some(has) = model_levels else { return Some(level) };
    if has.iter().any(|l| l == level) {
        return Some(level);
    }
    let all = cli.efforts();
    let at = all.iter().position(|l| *l == level)?;
    all[..at].iter().rev().find(|l| has.iter().any(|h| h == *l)).copied()
}

/// A run that failed before answering with an error naming the effort: the CLI or model refused
/// the level (or, on an older CLI, the option), so a background job runs again without it.
pub fn effort_refused(o: &Outcome) -> bool {
    o.text.trim().is_empty() && o.error.as_deref().is_some_and(|e| e.to_ascii_lowercase().contains("effort"))
}

/// Claude Code's memory (`~/.claude/projects/<folder>/memory/`), the one place a chat may write.
/// An Edit rule covers every file-writing tool (Claude Code says so of Write rules).
pub const CLAUDE_MEMORY_EDIT: &str = "Edit(~/.claude/projects/*/memory/**)";

/// What every chat is told about the app and the vault; `today` is the local date. With
/// `tools`, the CLI has Brainstead's MCP tools, which make its changes (recorded in Changes).
pub fn system_prompt(today: &str, tools: bool) -> String {
    format!(
        "You are the assistant inside Brainstead, a personal app for tasks (GTD), notes and a wiki kept as plain markdown files in one folder, the vault, which is your working directory. Today is {today}.\n\
The vault's layers: notes at the top level and in folders, named `Type. Title - YYYY-MM-DD.md` (for example `Meeting. Orbit App Steerco - 2026-09-30.md` or `1-1. Theo - 2026-09-07.md`); the wiki in `wiki/` (entities, concepts, summaries), sources in `sources/` (captured emails, documents, transcripts), templates in `Templates/`. \
Tasks are `- [ ]` lines with dates as emoji: 📅 due, ⏳ scheduled (deferred until), 🛫 start, ➕ created, ✅ done; `#followup`, `#waiting-for` and `#someday-maybe` mark kinds of task. `Me. To Do List.md` and `Me. Scratchpad.md` are the main lists.\n\
{can}\n\
Link notes you mention as [[wikilinks]] with the note's file name without `.md`, so the app can open them. Answer plainly and briefly unless asked for depth; say when the vault has nothing on a point rather than guessing.",
        can = if tools {
            "You can't change files in the vault yourself (you may keep your own memory up to date: it lives outside the vault); Brainstead's tools do what its screens do. Read with search, read_section, backlinks, resolve_entity, lint, list_tasks, list_inbox and list_projects. When the user asks, the tools make the change straight away, each recorded in Brainstead's Changes screen where the user can revert it: task changes (edit_task, move_task), clarifying the Inbox, projects, and prose, renames and deletes: edit_page (any page, or a new wiki page), create_note, create_task, rename_note, trash_note, delete_task. Runs (start_run: ingest, summaries, the daily check, contradictions, meeting notes) record their own changes there. Bookmarks, saved searches and Knowledge health's safe fixes are made at once too, undoable with ⌘Z in the app rather than in Changes. A change to a template (Templates/, or renaming or trashing one), one that adds code that runs (a script, a Tasks function, a Templater tag), or one that changes a system note's header is held for the user, and only they can accept it, in the app: you can't. Change only what the user asked for. Quote the sources a change rests on word for word: a quote that isn't in its source is flagged to the user. list_changes lists what was changed and what's held for the user, and changes reverts one when the user asks. Say what you changed."
        } else {
            "You can read and search the vault, and nothing else: you can't change any file in it (your own memory, outside the vault, you may keep up to date). If asked to change something, say what you would change and where, so the user can do it."
        }
    )
}

/// Ask's chats answer questions about the app from its help (core/src/help.rs): through the MCP
/// server's `help` tool, or, for Antigravity, which takes none, from the files copied into `help/`
/// in its working folder (`AGY_HELP_DIR`). None for a CLI that has neither.
pub fn help_hint(cli: Cli, mcp: bool) -> Option<&'static str> {
    if cli == Cli::Antigravity {
        Some("Questions about Brainstead itself (how to do something in the app, what a screen, setting or shortcut does) are answered from its help, not the vault: read help/index.md in your working folder, then the topic files it lists, and answer from them, naming where things are (Settings › Vault, the Inbox screen). If the help doesn't cover it, say so rather than guessing.")
    } else if mcp {
        Some("Questions about Brainstead itself (how to do something in the app, what a screen, setting or shortcut does) are answered from its help, not the vault: call Brainstead's help tool (with a query, or a topic from its list) and answer from what it says, naming where things are (Settings › Vault, the Inbox screen). If the help doesn't cover it, say so rather than guessing.")
    } else {
        None
    }
}

/// What the next-turn suggestion is told: it reads the vault but changes nothing, and replies with
/// one message the user might send next.
pub const NEXT_SYSTEM: &str = "You suggest the user's next message in a chat about their vault (plain markdown notes, tasks and a wiki in your working directory). You may read and search the vault to make the suggestion specific, but never change anything. Reply with the one message only.";

/// The prompt for the next-turn suggestion: the chat's title and its last few exchanges, each cut
/// to its end, newest last.
pub fn next_prompt(title: &str, turns: &[(String, String)]) -> String {
    // The end of a long answer is where it lands (its conclusion, what it offers to do next).
    let cut = |s: &str, n: usize| {
        let len = s.chars().count();
        if len <= n {
            s.to_string()
        } else {
            format!("…{}", s.chars().skip(len - n).collect::<String>())
        }
    };
    let mut p = format!(
        "Chat: {title}\n\nSuggest the single most useful next message the user could send: a natural follow-up that goes deeper, acts on the answer (a task, a note, a wiki edit) or checks something it left open. Write it as the user would type it, in the first person, at most 20 words, specific to what was discussed (name the people, notes or projects). No quotes, no preamble, no list.\n"
    );
    let start = turns.len().saturating_sub(3);
    for (user, assistant) in &turns[start..] {
        p.push_str(&format!("\nUser: {}\n\nAssistant: {}\n", cut(user, 1500), cut(assistant, 4000)));
    }
    p
}

/// The suggestion in a model's reply: its first line of text, without quotes, a label or a list
/// marker. None when there's nothing usable (empty, or too long to be one message).
pub fn parse_next(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches(['-', '*', '•', '>', ' ']);
    let line = ["Suggestion:", "Next message:", "Next:", "User:"].iter().fold(line, |l, p| l.strip_prefix(p).unwrap_or(l)).trim();
    let line = line.trim_matches(['"', '\'', '“', '”', '`']).trim();
    (!line.is_empty() && line.chars().count() <= 200).then(|| line.to_string())
}

/// The folder, inside Antigravity's working folder, that holds the help files.
pub const AGY_HELP_DIR: &str = "help";

/// One question to ask.
pub struct Request<'a> {
    pub prompt: &'a str,
    pub model: &'a str,
    /// The CLI's own session (Claude, Copilot), thread (Codex) or conversation (Antigravity) to resume.
    pub session: Option<&'a str>,
    pub system: &'a str,
    /// The vault, for CLIs that don't run in it (Antigravity runs in its own folder and adds it).
    pub vault: &'a str,
    /// Brainstead's MCP server, for the CLIs that take it.
    pub mcp: Option<&'a Mcp>,
    /// Images to show the model (an ingested image, as `platform::image_for_model` made it), for
    /// the CLIs that see them (`Cli::sees_images`); the others ignore them.
    pub images: &'a [std::path::PathBuf],
    /// The effort level set for this CLI in Settings › AI assistants. None, or a level not in
    /// `Cli::efforts`, passes none, so the CLI uses its own default for the model.
    pub effort: Option<&'a str>,
    /// Ollama's server, for `Cli::Ollama` (`ollama::host`); the others ignore it.
    pub ollama_host: &'a str,
}

fn strip<'a>(model: &'a str, prefix: &str) -> &'a str {
    model.strip_prefix(prefix).unwrap_or(model)
}

/// How to start a CLI for one question: the arguments after the program's name, what to write to
/// its standard input before closing it (None: no input), and the environment variables it needs
/// (Codex reads Ollama's address from one).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Launch {
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub env: Vec<(String, String)>,
}

/// The longest command line, in UTF-16 units, sent to a CLI on Windows. CreateProcess takes at
/// most 32,767 and longer fails with os error 206; this leaves room for the program's path and
/// the quoting std adds. A `.cmd` shim allows only 8,191, which is why Claude Code and Codex take
/// the prompt on standard input instead.
pub const WINDOWS_COMMAND_LINE_MAX: usize = 30_000;

/// `args`' length as a Windows command line: each argument quoted and followed by a space.
fn command_line_len(args: &[String]) -> usize {
    args.iter().map(|a| a.encode_utf16().count() + 3).sum()
}

/// A clear error in place of os error 206 when `args` won't fit on a Windows command line.
fn fits_command_line(cli: Cli, args: &[String], windows: bool) -> Result<(), String> {
    let n = command_line_len(args);
    if windows && n > WINDOWS_COMMAND_LINE_MAX {
        return Err(format!(
            "This request is too long for {} on Windows ({n} characters; it takes about {WINDOWS_COMMAND_LINE_MAX}). Shorten it, or use Claude Code or Codex, which take any length.",
            cli.label()
        ));
    }
    Ok(())
}

/// Claude Code's permission rule for editing anything under the folder `dir`. An absolute path in
/// a rule starts with `//`; on Windows Claude Code matches paths in POSIX form, `C:\Users\maya`
/// as `/c/Users/maya`. So `/Users/maya/Vault` gives `Edit(//Users/maya/Vault/**)` and
/// `C:\Users\maya\Vault` gives `Edit(//c/Users/maya/Vault/**)`.
pub fn claude_edit_rule(dir: &str) -> String {
    let p = dir.replace('\\', "/");
    let p = p.strip_prefix("//?/").unwrap_or(&p).trim_end_matches('/');
    let posix = match crate::paths::has_drive(p) {
        true => format!("/{}{}", p[..1].to_ascii_lowercase(), &p[2..]),
        false if p.starts_with('/') => p.to_string(),
        false => format!("/{p}"),
    };
    format!("Edit(/{posix}/**)")
}

/// How to start the CLI for `r`. Claude Code and Codex take the prompt on standard input (`-p` and
/// `exec` with no prompt argument, or `-`), so its length is never limited by the command line;
/// Antigravity and Copilot take it as an argument. An Ollama model is Codex's launch with its
/// Ollama provider picked (`--oss --local-provider ollama`, and `model_provider`, without which
/// Codex 0.166 starts the thread on Ollama but runs the turn on OpenAI) and the server's address in
/// `CODEX_OSS_BASE_URL`, the one variable its provider reads.
pub fn args(cli: Cli, r: &Request) -> Result<Launch, String> {
    let instructions = format!("{}\n\n{}", r.system, cli.tools_hint());
    let s = |x: &str| x.to_string();
    let mut a: Vec<String> = Vec::new();
    let mut stdin = None;
    let mut env = Vec::new();
    let images: Vec<String> = r.images.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let effort = r.effort.filter(|e| cli.efforts().contains(e));
    match cli {
        Cli::Claude => {
            // An image is read with Read from its own folder, added to the ones Claude Code may read.
            let prompt = match images.as_slice() {
                [] => r.prompt.to_string(),
                [one] => format!("{}\n\nThe image is at {one}: view it with the Read tool before you answer.", r.prompt),
                many => format!("{}\n\nThe images are at {}: view each with the Read tool before you answer.", r.prompt, many.join(", ")),
            };
            // `-p` without a prompt reads it from standard input.
            a.extend([s("-p"), s("--model"), s(strip(r.model, CLAUDE_PREFIX))]);
            if let Some(e) = effort {
                a.extend([s("--effort"), s(e)]);
            }
            stdin = Some(prompt);
            let mut dirs: Vec<String> = r.images.iter().filter_map(|p| p.parent()).map(|d| d.to_string_lossy().into_owned()).collect();
            dirs.dedup();
            for d in dirs {
                a.extend([s("--add-dir"), d]);
            }
            a.extend(["--output-format", "stream-json", "--verbose", "--include-partial-messages", "--strict-mcp-config"].map(s));
            // Reads in the working directory need no permission; anything else would ask, and print
            // mode refuses what it would have to ask for. The mode is pinned in case the user's
            // settings bypass it. Skill lets `/name` run the vault's skills; what they need beyond
            // reading is refused.
            // Allowed without asking: Brainstead's tools, when it has them (they read, or ask the app
            // to make a change, which it records in Changes), and writing Claude Code's own memory, which is outside the vault (decided
            // with the user 2026-10-03: a chat must be able to remember what it's told).
            let mut allowed = vec![CLAUDE_MEMORY_EDIT.to_string()];
            if let Some(m) = r.mcp {
                // Only Brainstead's server (--strict-mcp-config).
                let cfg = serde_json::json!({"mcpServers": {MCP_NAME: {"type": "stdio", "command": m.command, "args": m.args}}});
                a.extend([s("--mcp-config"), cfg.to_string()]);
                allowed.insert(0, format!("mcp__{MCP_NAME}"));
            }
            a.extend([s("--allowedTools"), allowed.join(",")]);
            // The vault is never written by a chat, even if a setting would allow it: changes go
            // through Brainstead's MCP tools.
            a.extend([s("--disallowedTools"), claude_edit_rule(r.vault)]);
            a.extend(["--tools", "Read,Grep,Glob,Skill,Edit,Write", "--permission-mode", "default", "--append-system-prompt"].map(s));
            a.push(instructions);
            if let Some(id) = r.session {
                a.extend(["--resume", id].map(s));
            }
        }
        Cli::Codex | Cli::Ollama => {
            a.push(s("exec"));
            if r.session.is_some() {
                a.push(s("resume"));
            }
            a.extend(["--json", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules"].map(s));
            if cli == Cli::Ollama {
                a.extend(
                    ["--oss", "--local-provider", "ollama", "-c", "model_provider=\"ollama\"", "-m", strip(r.model, OLLAMA_PREFIX)].map(s),
                );
                env.push(("CODEX_OSS_BASE_URL".to_string(), ollama::codex_base_url(r.ollama_host)));
            } else {
                a.extend(["-m", r.model].map(s));
            }
            // Attached to the prompt (`-i, --image <FILE>...`; one per flag, so none swallows the next).
            a.extend(images.iter().map(|i| format!("--image={i}")));
            // -c values are TOML; a JSON string is a valid TOML basic string.
            let dev = serde_json::to_string(&instructions).map_err(|e| e.to_string())?;
            a.extend([s("-c"), s("sandbox_mode=\"read-only\""), s("-c"), format!("developer_instructions={dev}")]);
            // `--ignore-user-config` leaves out the user's own config.toml, so its effort too.
            if let Some(e) = effort {
                a.extend([s("-c"), format!("model_reasoning_effort=\"{e}\"")]);
            }
            if let Some(m) = r.mcp {
                // TOML values; JSON strings and arrays of strings are valid TOML.
                let cmd = serde_json::to_string(&m.command).map_err(|e| e.to_string())?;
                let args = serde_json::to_string(&m.args).map_err(|e| e.to_string())?;
                a.extend([s("-c"), format!("mcp_servers.{MCP_NAME}.command={cmd}")]);
                a.extend([s("-c"), format!("mcp_servers.{MCP_NAME}.args={args}")]);
                // Exec mode asks for nothing, so a tool needing approval fails; Brainstead's don't
                // write to the vault.
                a.extend([s("-c"), format!("mcp_servers.{MCP_NAME}.default_tools_approval_mode=\"approve\"")]);
            }
            a.push(s("--"));
            if let Some(id) = r.session {
                a.push(s(id));
            }
            // `-` reads the prompt from standard input.
            a.push(s("-"));
            stdin = Some(s(r.prompt));
        }
        Cli::Antigravity => {
            a.extend(["-p", r.prompt, "--agent", AGY_AGENT, "--model", strip(r.model, AGY_PREFIX)].map(s));
            a.extend(["--add-dir", r.vault, "--sandbox", "--disable-slash-commands", "--output-format", "stream-json"].map(s));
            if let Some(e) = effort {
                a.extend([s("--effort"), s(e)]);
            }
            if let Some(id) = r.session {
                a.extend(["--conversation", id].map(s));
            }
        }
        Cli::Copilot => {
            // It takes no system prompt: the instructions open the first message; a resumed
            // session has them already.
            let prompt = match r.session {
                Some(_) => r.prompt.to_string(),
                None => format!("<instructions>\n{instructions}\n</instructions>\n\n{}", r.prompt),
            };
            a.extend([s("-p"), prompt]);
            a.extend(["--model", strip(r.model, COPILOT_PREFIX), "--output-format", "json", "--stream", "on"].map(s));
            if let Some(e) = effort {
                a.extend([s("--reasoning-effort"), s(e)]);
            }
            a.extend(["--no-custom-instructions", "--disable-builtin-mcps", "--no-auto-update", "--disallow-temp-dir"].map(s));
            // Never an empty list: `--available-tools=` would allow every tool.
            let mut tools = s("view,grep,glob");
            if let Some(m) = r.mcp {
                let cfg =
                    serde_json::json!({"mcpServers": {MCP_NAME: {"type": "local", "command": m.command, "args": m.args, "tools": ["*"]}}});
                a.extend([s("--additional-mcp-config"), cfg.to_string(), format!("--allow-tool={MCP_NAME}")]);
                for t in MCP_TOOLS {
                    tools.push_str(&format!(",{MCP_NAME}-{t}"));
                }
            }
            a.push(format!("--available-tools={tools}"));
            if let Some(id) = r.session {
                a.push(format!("--resume={id}"));
            }
        }
    }
    fits_command_line(cli, &a, cfg!(windows))?;
    Ok(Launch { args: a, stdin, env })
}

/// Antigravity's agent: only its read and search tools (a custom agent's list is enforced; the
/// CLI has no flag for it), and the instructions as its system prompt.
pub fn agy_agent(system: &str) -> String {
    let tools = "\n  - view_file\n  - grep_search\n  - find_by_name\n  - list_dir";
    format!(
        "---\nname: {AGY_AGENT}\ndescription: Brainstead's Ask, read-only\nmainAgent: true\ninheritMcp: false\ntools:{tools}\n---\n# Instructions\n{system}\n\n{}\n",
        Cli::Antigravity.tools_hint()
    )
}

/// What a CLI's stream says, one line at a time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    /// More of the answer.
    Text { text: String },
    /// What it's doing: "Reading Meeting. Orbit App Steerco".
    Status { text: String },
    /// Claude: how full its context is, and its size.
    Usage { tokens: u64, window: Option<u64> },
    /// Claude compacted the conversation.
    Compacted,
}

/// How a turn ended.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub text: String,
    pub session: Option<String>,
    pub error: Option<String>,
}

/// A note's name as the user knows it, from a path a tool opened: "Meeting. Orbit App Steerco".
fn note_name(path: &str) -> Option<String> {
    let name = path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next()?;
    let name = name.strip_suffix(".md").unwrap_or(name);
    (!name.is_empty()).then(|| name.to_string())
}

const SEARCHING: &str = "Searching the vault";

/// What a Brainstead tool call is doing, for the status line.
fn brainstead_status(tool: &str, args: &serde_json::Value) -> Option<String> {
    let page = || args["page"].as_str().map(|p| note_name(p.trim_start_matches("[[").trim_end_matches("]]")).unwrap_or_default());
    Some(match tool {
        "search" => SEARCHING.into(),
        "read_section" => page().map(|p| format!("Reading {p}")).unwrap_or_else(|| "Reading".into()),
        "backlinks" => page().map(|p| format!("Finding links to {p}")).unwrap_or_else(|| "Finding links".into()),
        "resolve_entity" => args["name"].as_str().map(|n| format!("Looking up {n}")).unwrap_or_else(|| "Looking up a name".into()),
        "pending_sources" => "Checking the sources".into(),
        "lint" => "Checking the wiki".into(),
        "edit_page" => page().map(|p| format!("Changing {p}")).unwrap_or_else(|| "Changing a page".into()),
        "create_task" => "Adding a task".into(),
        _ => return None,
    })
}
const LOOKING: &str = "Looking through the vault";

/// Reads one CLI's stream. Feed it each stdout line, then `finish` with whether the process
/// succeeded and its stderr.
pub struct Parser {
    cli: Cli,
    text: String,
    session: Option<String>,
    error: Option<String>,
    /// Antigravity's step or Copilot's message the last text came in, to break between replies.
    block: Option<String>,
}

impl Parser {
    pub fn new(cli: Cli) -> Parser {
        Parser { cli, text: String::new(), session: None, error: None, block: None }
    }

    /// Text written after a search is a new block; without a break it runs on from the text
    /// before it ("…in the steerco note.Across the meetings…").
    fn separate(&mut self, out: &mut Vec<Event>) {
        if !self.text.is_empty() && !self.text.ends_with("\n\n") {
            let sep = if self.text.ends_with('\n') { "\n" } else { "\n\n" };
            self.text.push_str(sep);
            out.push(Event::Text { text: sep.into() });
        }
    }

    fn push(&mut self, t: &str, out: &mut Vec<Event>) {
        self.text.push_str(t);
        out.push(Event::Text { text: t.into() });
    }

    pub fn feed(&mut self, line: &str) -> Vec<Event> {
        let mut out = Vec::new();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return out };
        match self.cli {
            Cli::Claude => self.claude(&v, &mut out),
            Cli::Codex | Cli::Ollama => self.codex(&v, &mut out),
            Cli::Antigravity => self.agy(&v, &mut out),
            Cli::Copilot => self.copilot(&v, &mut out),
        }
        out
    }

    fn claude(&mut self, v: &serde_json::Value, out: &mut Vec<Event>) {
        match v["type"].as_str() {
            Some("stream_event") => {
                let ev = &v["event"];
                if ev["type"] == "content_block_start" && ev["content_block"]["type"] == "text" {
                    self.separate(out);
                }
                if ev["type"] == "content_block_delta" && ev["delta"]["type"] == "text_delta" {
                    if let Some(t) = ev["delta"]["text"].as_str() {
                        self.push(t, out);
                    }
                }
            }
            // A whole assistant turn: its tool calls say what it's looking at, its usage how full the context is.
            Some("assistant") => {
                let m = &v["message"];
                for c in m["content"].as_array().into_iter().flatten().filter(|c| c["type"] == "tool_use") {
                    let input = &c["input"];
                    let what = match c["name"].as_str() {
                        Some("Read") => input["file_path"].as_str().and_then(note_name).map(|s| format!("Reading {s}")),
                        Some("Grep") => Some(SEARCHING.into()),
                        Some("Glob") => Some(LOOKING.into()),
                        Some("Skill") => input["skill"].as_str().map(|s| format!("Running /{s}")),
                        Some(n) => n.strip_prefix(&format!("mcp__{MCP_NAME}__")).and_then(|t| brainstead_status(t, input)),
                        _ => None,
                    };
                    if let Some(text) = what {
                        out.push(Event::Status { text });
                    }
                }
                let u = &m["usage"];
                if u.is_object() {
                    let n = |k: &str| u[k].as_u64().unwrap_or(0);
                    let tokens = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                    if tokens > 0 {
                        out.push(Event::Usage { tokens, window: None });
                    }
                }
            }
            Some("system") => {
                if let Some(s) = v["session_id"].as_str() {
                    self.session = Some(s.into());
                }
                if v["subtype"] == "compact_boundary" {
                    out.push(Event::Compacted);
                }
            }
            Some("result") => {
                if let Some(s) = v["session_id"].as_str() {
                    self.session = Some(s.into());
                }
                if v["is_error"].as_bool() == Some(true) {
                    self.error = Some(v["result"].as_str().unwrap_or("Claude returned an error").into());
                } else if self.text.is_empty() {
                    if let Some(r) = v["result"].as_str() {
                        self.text = r.into();
                        out.push(Event::Text { text: r.into() });
                    }
                }
                let window = v["modelUsage"].as_object().and_then(|m| m.values().filter_map(|x| x["contextWindow"].as_u64()).max());
                if let Some(w) = window {
                    out.push(Event::Usage { tokens: 0, window: Some(w) });
                }
            }
            _ => {}
        }
    }

    fn codex(&mut self, v: &serde_json::Value, out: &mut Vec<Event>) {
        match v["type"].as_str() {
            Some("thread.started") => {
                if let Some(s) = v["thread_id"].as_str() {
                    self.session = Some(s.into());
                }
            }
            Some("item.started") if v["item"]["type"] == "mcp_tool_call" && v["item"]["server"] == MCP_NAME => {
                if let Some(text) = v["item"]["tool"].as_str().and_then(|t| brainstead_status(t, &v["item"]["arguments"])) {
                    out.push(Event::Status { text });
                }
            }
            Some("item.started") if v["item"]["type"] == "command_execution" => {
                let cmd = v["item"]["command"].as_str().unwrap_or("");
                let file = cmd.split(['\'', '"']).find(|p| p.ends_with(".md")).and_then(note_name);
                out.push(Event::Status { text: file.map(|f| format!("Reading {f}")).unwrap_or_else(|| SEARCHING.into()) });
            }
            // Messages come whole. When it searches first, earlier ones are progress notes; each
            // is shown as it comes, separated, and the turn's text is all of them.
            Some("item.completed") if v["item"]["type"] == "agent_message" => {
                if let Some(t) = v["item"]["text"].as_str() {
                    self.separate(out);
                    self.push(t, out);
                }
            }
            Some("turn.failed") => self.error = Some(v["error"]["message"].as_str().unwrap_or("Codex returned an error").into()),
            Some("error") => self.error = Some(v["message"].as_str().unwrap_or("Codex returned an error").into()),
            _ => {}
        }
    }

    fn agy(&mut self, v: &serde_json::Value, out: &mut Vec<Event>) {
        match v["event"].as_str() {
            Some("step_update") => {
                let s = &v["step_update"];
                if let Some(c) = s["conversation_id"].as_str() {
                    self.session = Some(c.into());
                }
                match s["step_type"].as_str() {
                    Some("agent_response") => {
                        let Some(t) = s["text_delta"].as_str().filter(|t| !t.is_empty()) else { return };
                        let step = s["step_index"].as_i64().map(|i| i.to_string());
                        if self.block.is_some() && self.block != step {
                            self.separate(out);
                        }
                        self.block = step;
                        self.push(t, out);
                    }
                    Some("tool") if s["state"] == "ACTIVE" => {
                        let p = &s["tool_info"]["parameters"];
                        let what = match s["tool_name"].as_str() {
                            Some("view_file") => p["AbsolutePath"].as_str().and_then(note_name).map(|f| format!("Reading {f}")),
                            Some("grep_search") => Some(SEARCHING.into()),
                            Some("find_by_name") | Some("list_dir") => Some(LOOKING.into()),
                            _ => None,
                        };
                        if let Some(text) = what {
                            out.push(Event::Status { text });
                        }
                    }
                    _ => {}
                }
            }
            Some("result") => {
                let r = &v["result"];
                if let Some(c) = r["conversation_id"].as_str() {
                    self.session = Some(c.into());
                }
                if self.text.is_empty() {
                    if let Some(t) = r["response"].as_str().filter(|t| !t.is_empty()) {
                        self.push(t, out);
                    }
                }
                // A refused action (a command, a web page) ends the turn with no answer.
                let denied: Vec<&str> =
                    r["denied_actions"].as_array().into_iter().flatten().filter_map(|d| d["display_name"].as_str()).collect();
                if self.text.trim().is_empty() && !denied.is_empty() {
                    self.error = Some(format!(
                        "Antigravity stopped without answering: it tried something Ask doesn't allow ({}). Try asking again, or another model.",
                        denied.join(", ")
                    ));
                } else if r["status"].as_str().is_some_and(|s| s != "SUCCESS") && self.text.is_empty() {
                    self.error = Some(format!("Antigravity returned an error ({})", r["status"].as_str().unwrap_or("")));
                }
            }
            _ => {}
        }
    }

    fn copilot(&mut self, v: &serde_json::Value, out: &mut Vec<Event>) {
        let d = &v["data"];
        match v["type"].as_str() {
            Some("assistant.message_delta") => {
                let Some(t) = d["deltaContent"].as_str().filter(|t| !t.is_empty()) else { return };
                let id = d["messageId"].as_str().map(str::to_string);
                if self.block.is_some() && self.block != id {
                    self.separate(out);
                }
                self.block = id;
                self.push(t, out);
            }
            Some("tool.execution_start") => {
                let what = match d["toolName"].as_str() {
                    Some("view") => d["arguments"]["path"].as_str().and_then(note_name).map(|f| format!("Reading {f}")),
                    Some("grep") => Some(SEARCHING.into()),
                    Some("glob") => Some(LOOKING.into()),
                    Some(n) => n.strip_prefix(&format!("{MCP_NAME}-")).and_then(|t| brainstead_status(t, &d["arguments"])),
                    _ => None,
                };
                if let Some(text) = what {
                    out.push(Event::Status { text });
                }
            }
            Some("session.error") => self.error = Some(d["message"].as_str().unwrap_or("Copilot returned an error").into()),
            Some("result") => {
                if let Some(s) = v["sessionId"].as_str() {
                    self.session = Some(s.into());
                }
            }
            _ => {}
        }
    }

    /// The turn's end. `ok`: the process exited successfully; `stderr`: what it printed there.
    pub fn finish(self, ok: bool, stderr: &str) -> Outcome {
        let mut error = self.error;
        // Codex and Copilot report transient trouble (a dropped stream they reconnect) as errors
        // too, so one only counts when no answer came.
        if matches!(self.cli, Cli::Codex | Cli::Ollama | Cli::Copilot) && !self.text.trim().is_empty() {
            error = None;
        }
        if error.is_none() && self.text.trim().is_empty() && (!ok || matches!(self.cli, Cli::Antigravity | Cli::Copilot)) {
            let msg = stderr.trim();
            error = Some(if msg.is_empty() || ok {
                format!("{} stopped without answering", self.cli.label())
            } else {
                signed_out(self.cli, msg)
            });
        }
        Outcome { text: self.text, session: self.session, error }
    }
}

/// An error that means the CLI isn't signed in gets a hint saying how (the previous app's check).
fn signed_out(cli: Cli, msg: &str) -> String {
    let m = msg.to_lowercase();
    let auth =
        ["not logged in", "please log in", "login required", "unauthorized", "invalid api key", "authentication", "/login", "sign in"]
            .iter()
            .any(|k| m.contains(k));
    if auth {
        format!("{} isn't signed in. Open Terminal, run `{}` and sign in, then try again.\n\n{msg}", cli.label(), cli.bin())
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_turn_prompt_keeps_the_last_three_exchanges() {
        let turns: Vec<(String, String)> = (1..=5).map(|i| (format!("q{i}"), format!("a{i}"))).collect();
        let p = next_prompt("Orbit App pilot", &turns);
        assert!(p.starts_with("Chat: Orbit App pilot"));
        assert!(!p.contains("q2") && p.contains("q3") && p.contains("a5"));
        let long = next_prompt("t", &[("x".repeat(2000), "y".into())]);
        assert!(long.contains(&format!("User: …{}\n", "x".repeat(1500))));
        // A long answer keeps its end, where it lands.
        let answer = format!("{}The launch moves to 28 November.", "y".repeat(5000));
        assert!(next_prompt("t", &[("q".into(), answer)]).contains("The launch moves to 28 November."));
    }

    #[test]
    fn next_turn_reply_is_cleaned() {
        assert_eq!(parse_next("\n\"What did Lena decide?\"\n").as_deref(), Some("What did Lena decide?"));
        assert_eq!(parse_next("Suggestion: Draft a nudge to Maya").as_deref(), Some("Draft a nudge to Maya"));
        assert_eq!(parse_next("- Add a task for it").as_deref(), Some("Add a task for it"));
        assert_eq!(parse_next("   \n "), None);
        assert_eq!(parse_next(&"w ".repeat(150)), None);
    }

    fn req<'a>(model: &'a str, session: Option<&'a str>) -> Request<'a> {
        Request {
            prompt: "What's due?",
            model,
            session,
            system: "SYS",
            vault: "/v",
            mcp: None,
            images: &[],
            effort: None,
            ollama_host: ollama::DEFAULT_HOST,
        }
    }

    fn feed(cli: Cli, lines: &[&str]) -> (Vec<Event>, Parser) {
        let mut p = Parser::new(cli);
        let ev = lines.iter().flat_map(|l| p.feed(l)).collect();
        (ev, p)
    }

    fn text(ev: &[Event]) -> String {
        ev.iter().filter_map(|e| if let Event::Text { text } = e { Some(text.as_str()) } else { None }).collect()
    }

    #[test]
    fn routes_by_model_id() {
        assert_eq!(Cli::route("claude:sonnet"), Cli::Claude);
        assert_eq!(Cli::route("claude-sonnet-5"), Cli::Claude);
        assert_eq!(Cli::route("agy:claude-opus"), Cli::Antigravity);
        assert_eq!(Cli::route("copilot:auto"), Cli::Copilot);
        assert_eq!(Cli::route("ollama:gpt-oss:20b"), Cli::Ollama);
        assert_eq!(Cli::route("gpt-5.5"), Cli::Codex);
        // Ollama runs through Codex's program, under its own key and label.
        assert_eq!((Cli::Ollama.bin(), Cli::Ollama.key(), Cli::Ollama.label()), ("codex", "ollama", "Ollama"));
    }

    #[test]
    fn ollama_args() {
        let host = "http://localhost:11434";
        let l = args(Cli::Ollama, &Request { ollama_host: host, ..req("ollama:gpt-oss:20b", None) }).unwrap();
        let a = &l.args;
        assert_eq!(a[0], "exec");
        // Codex's own Ollama provider, told to use it for the turn too, with the model as Ollama names it.
        assert!(a.windows(3).any(|w| w == ["--oss", "--local-provider", "ollama"]), "{a:?}");
        assert!(a.windows(2).any(|w| w == ["-c", "model_provider=\"ollama\""]), "{a:?}");
        assert!(a.windows(2).any(|w| w == ["-m", "gpt-oss:20b"]), "{a:?}");
        assert!(!a.iter().any(|x| x.starts_with("ollama:")));
        // Everything else as Codex: read-only, the instructions, the prompt on stdin.
        assert!(a.windows(2).any(|w| w == ["-c", "sandbox_mode=\"read-only\""]));
        assert!(a.iter().any(|x| x.starts_with("developer_instructions=\"SYS\\n\\n")));
        assert_eq!(a[a.len() - 2..], ["--", "-"]);
        assert_eq!(l.stdin.as_deref(), Some("What's due?"));
        // The server's address reaches Codex in the one variable its provider reads.
        assert_eq!(l.env, vec![("CODEX_OSS_BASE_URL".to_string(), "http://localhost:11434/v1".to_string())]);
        let far = args(Cli::Ollama, &Request { ollama_host: "http://gpu-box:11434", ..req("ollama:qwen3:4b", Some("t1")) }).unwrap();
        assert_eq!(far.env[0].1, "http://gpu-box:11434/v1");
        assert_eq!(far.args[..2], ["exec", "resume"]);
        assert_eq!(far.args[far.args.len() - 3..], ["--", "t1", "-"]);
        // Codex's own launch has none of it.
        let c = args(Cli::Codex, &req("gpt-5", None)).unwrap();
        assert!(c.env.is_empty() && !c.args.iter().any(|x| x == "--oss"));
        // An Ollama model gets its level as Codex's model_reasoning_effort, from Ollama's names only.
        let e = args(Cli::Ollama, &Request { effort: Some("high"), ..req("ollama:gpt-oss:20b", None) }).unwrap().args;
        assert!(e.windows(2).any(|w| w == ["-c", "model_reasoning_effort=\"high\""]), "{e:?}");
        let x = args(Cli::Ollama, &Request { effort: Some("xhigh"), ..req("ollama:gpt-oss:20b", None) }).unwrap().args.join(" ");
        assert!(!x.contains("effort"), "{x}");
        // Images aren't attached: the model may not see them, and Codex fails the turn if it doesn't.
        assert!(!Cli::Ollama.sees_images());
    }

    #[test]
    fn ollama_host_as_ollama_reads_it() {
        for (var, host) in [
            (None, "http://localhost:11434"),
            (Some(""), "http://localhost:11434"),
            (Some("0.0.0.0"), "http://localhost:11434"),
            (Some("0.0.0.0:11435"), "http://localhost:11435"),
            (Some(":11435"), "http://localhost:11435"),
            (Some("gpu-box"), "http://gpu-box:11434"),
            (Some("gpu-box:8080"), "http://gpu-box:8080"),
            (Some("https://ollama.example.com"), "https://ollama.example.com:11434"),
            (Some("http://127.0.0.1:11434/"), "http://127.0.0.1:11434"),
            (Some("[::1]:11434"), "http://[::1]:11434"),
        ] {
            assert_eq!(ollama::host(var), host, "{var:?}");
        }
        assert_eq!(ollama::codex_base_url("http://localhost:11434"), "http://localhost:11434/v1");
    }

    #[test]
    fn ollama_tags_and_show() {
        let tags = serde_json::json!({"models": [
            {"name": "qwen3:4b", "model": "qwen3:4b", "size": 2497293931u64, "details": {"family": "qwen3", "parameter_size": "4.0B"}},
            {"name": "gpt-oss:20b", "model": "gpt-oss:20b", "details": {"parameter_size": ""}}
        ]});
        let t = ollama::parse_tags(&tags);
        assert_eq!(t.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["qwen3:4b", "gpt-oss:20b"]);
        assert_eq!(t[0].parameters.as_deref(), Some("4.0B"));
        assert_eq!(t[1].parameters, None);
        assert!(ollama::parse_tags(&serde_json::json!({})).is_empty());
        assert_eq!(ollama::parse_version(&serde_json::json!({"version": "0.35.1"})).as_deref(), Some("0.35.1"));
        // qwen3 calls tools and thinks, but only on or off: no level to offer.
        let qwen = ollama::parse_show(
            &serde_json::json!({"capabilities": ["completion", "tools", "thinking"], "thinking": {"values": [true], "default": true}}),
        );
        assert_eq!(qwen, ollama::Show { tools: true, levels: vec![] });
        assert!(qwen.efforts().is_empty());
        // gpt-oss names its levels, offered in Codex's order whatever the server's.
        let oss = ollama::parse_show(
            &serde_json::json!({"capabilities": ["completion", "tools", "thinking"], "thinking": {"values": ["high", "medium", "low"], "default": "medium"}}),
        );
        assert!(oss.tools);
        assert_eq!(oss.efforts(), ["low", "medium", "high"]);
        // A level Codex doesn't name isn't offered; a model without tools can't run Codex's loop.
        let odd =
            ollama::parse_show(&serde_json::json!({"capabilities": ["completion", "vision"], "thinking": {"values": ["minimal", "high"]}}));
        assert!(!odd.tools);
        assert_eq!(odd.efforts(), ["high"]);
        assert_eq!(ollama::parse_show(&serde_json::json!({})), ollama::Show::default());
    }

    #[test]
    fn no_cli_skips_permissions_or_gets_write_tools() {
        for (m, s) in [
            ("claude:opus", None),
            ("claude:opus", Some("s1")),
            ("gpt-5", None),
            ("gpt-5", Some("t")),
            ("agy:x", None),
            ("copilot:auto", Some("c")),
        ] {
            let a = args(Cli::route(m), &req(m, s)).unwrap().args;
            let all = a.join(" ");
            assert!(!all.contains("dangerously"), "{all}");
            assert!(!all.contains("bypassPermissions") && !all.contains("--yolo") && !all.contains("--allow-all"), "{all}");
            assert!(!a.iter().any(|x| x == "--available-tools="), "{all}");
            assert!(!all.contains("Bash"), "{all}");
            // Writing only to Claude Code's memory, never to the vault.
            if Cli::route(m) == Cli::Claude {
                let al = &a[a.iter().position(|x| x == "--allowedTools").unwrap() + 1];
                assert!(al.split(',').all(|t| t.starts_with("mcp__") || t == CLAUDE_MEMORY_EDIT), "{al}");
                let d = a.iter().position(|x| x == "--disallowedTools").unwrap();
                assert_eq!(a[d + 1], "Edit(//v/**)");
            } else {
                assert!(!all.contains("Write") && !all.contains("Edit"), "{all}");
            }
        }
    }

    fn mcp() -> Mcp {
        Mcp { command: "/app/Brainstead".into(), args: vec!["--mcp".into(), "--chat".into(), "c1".into()] }
    }

    #[test]
    fn mcp_server_for_each_cli() {
        let m = mcp();
        let with = |model: &str| {
            let r = Request { mcp: Some(&m), ..req(model, None) };
            args(Cli::route(model), &r).unwrap().args
        };
        let c = with("claude:sonnet");
        let i = c.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&c[i + 1]).unwrap();
        assert_eq!(cfg["mcpServers"]["brainstead"]["command"], "/app/Brainstead");
        assert!(c.windows(2).any(|w| w[0] == "--allowedTools" && w[1].starts_with("mcp__brainstead,")));
        assert!(c.contains(&"--strict-mcp-config".to_string()));
        let x = with("gpt-5");
        assert!(x.contains(&"mcp_servers.brainstead.command=\"/app/Brainstead\"".to_string()));
        assert!(x.contains(&r#"mcp_servers.brainstead.args=["--mcp","--chat","c1"]"#.to_string()));
        assert!(x.contains(&"mcp_servers.brainstead.default_tools_approval_mode=\"approve\"".to_string()));
        // The prompt is still read last, after `--`, from standard input.
        assert_eq!(x[x.len() - 2..], ["--", "-"]);
        let p = with("copilot:auto");
        assert!(p.contains(&"--allow-tool=brainstead".to_string()));
        let tools = p.iter().find(|x| x.starts_with("--available-tools=")).unwrap();
        assert!(
            tools.starts_with("--available-tools=view,grep,glob,brainstead-search,")
                && tools.ends_with(
                    ",brainstead-glance,brainstead-app_status,brainstead-list_assistants,brainstead-rebuild_index,brainstead-check_updates"
                )
        );
        // Antigravity doesn't take it: the app doesn't pass one, and the system prompt says so.
        assert!(!Cli::Antigravity.takes_mcp());
        assert!(system_prompt("Friday", true).contains("edit_page"));
        assert!(!system_prompt("Friday", false).contains("edit_page"));
        assert!(help_hint(Cli::Claude, true).unwrap().contains("help tool"));
        assert!(help_hint(Cli::Antigravity, false).unwrap().contains("help/index.md"));
        assert_eq!(help_hint(Cli::Codex, false), None);
        // Still nothing that writes (beyond Claude Code's memory) or skips permissions.
        for a in [c, x, p] {
            let all = a.join(" ");
            assert!(!all.contains("dangerously") && !all.contains("--allow-all") && !all.contains("Bash"), "{all}");
        }
    }

    #[test]
    fn brainstead_tool_status() {
        let (ev, _) = feed(
            Cli::Claude,
            &[
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__brainstead__edit_page","input":{"page":"wiki/entities/Orbit App.md"}}]}}"#,
            ],
        );
        assert!(ev.contains(&Event::Status { text: "Changing Orbit App".into() }));
        let (ev, _) = feed(
            Cli::Codex,
            &[
                r#"{"type":"item.started","item":{"type":"mcp_tool_call","server":"brainstead","tool":"read_section","arguments":{"page":"[[Orbit App]]"}}}"#,
            ],
        );
        assert_eq!(ev, [Event::Status { text: "Reading Orbit App".into() }]);
        let (ev, _) =
            feed(Cli::Copilot, &[r#"{"type":"tool.execution_start","data":{"toolName":"brainstead-search","arguments":{"query":"x"}}}"#]);
        assert_eq!(ev, [Event::Status { text: "Searching the vault".into() }]);
        assert_eq!(note_name(r"C:\Users\maya\Vault\wiki\Orbit App.md").as_deref(), Some("Orbit App"));
        assert_eq!(note_name("/Users/maya/Vault/wiki/Orbit App.md").as_deref(), Some("Orbit App"));
    }

    #[test]
    fn claude_args() {
        let l = args(Cli::Claude, &req("claude:sonnet", Some("abc"))).unwrap();
        let a = l.args;
        assert_eq!(a[..3], ["-p", "--model", "sonnet"]);
        assert_eq!(l.stdin.as_deref(), Some("What's due?"));
        assert!(!a.iter().any(|x| x.contains("What's due?")));
        let i = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[i + 1], "Read,Grep,Glob,Skill,Edit,Write");
        assert!(a.windows(2).any(|w| w == ["--permission-mode", "default"]));
        assert!(a.iter().any(|x| x.starts_with("SYS\n\nUse Grep")));
        assert_eq!(a[a.len() - 2..], ["--resume", "abc"]);
        // A full model id passes through as it is.
        let b = args(Cli::Claude, &req("claude-sonnet-5", None)).unwrap().args;
        assert_eq!(b[2], "claude-sonnet-5");
    }

    #[test]
    fn claude_edit_guard_from_either_system() {
        assert_eq!(claude_edit_rule("/Users/maya/Vault/"), "Edit(//Users/maya/Vault/**)");
        assert_eq!(claude_edit_rule(r"C:\Users\maya\Vault"), "Edit(//c/Users/maya/Vault/**)");
        assert_eq!(claude_edit_rule(r"\\?\D:\Notes\Vault\"), "Edit(//d/Notes/Vault/**)");
        let a = args(Cli::Claude, &Request { vault: r"C:\Users\maya\Vault", ..req("claude:sonnet", None) }).unwrap().args;
        assert!(a.windows(2).any(|w| w == ["--disallowedTools", "Edit(//c/Users/maya/Vault/**)"]));
    }

    #[test]
    fn long_prompts_go_on_stdin_for_claude_and_codex() {
        let long = "x".repeat(150_000);
        for model in ["claude:sonnet", "gpt-5"] {
            let l = args(Cli::route(model), &Request { prompt: &long, ..req(model, Some("s1")) }).unwrap();
            assert_eq!(l.stdin.as_deref(), Some(long.as_str()));
            assert!(command_line_len(&l.args) < WINDOWS_COMMAND_LINE_MAX);
        }
        // Copilot and Antigravity take it as an argument: too long for Windows is a clear error.
        for model in ["copilot:auto", "agy:gemini"] {
            let cli = Cli::route(model);
            let l = args(cli, &Request { prompt: &long, ..req(model, None) });
            if cfg!(windows) {
                assert!(l.unwrap_err().contains("too long"));
            } else {
                assert_eq!(l.unwrap().stdin, None);
            }
            let a = args(cli, &req(model, None)).unwrap().args;
            assert!(fits_command_line(cli, &a, true).is_ok());
            let mut big = a.clone();
            big.push(long.clone());
            assert!(fits_command_line(cli, &big, true).unwrap_err().contains(cli.label()));
            assert!(fits_command_line(cli, &big, false).is_ok());
        }
    }

    #[test]
    fn images_for_the_clis_that_see_them() {
        let imgs = [std::path::PathBuf::from("/data/ask/img/abc.jpg")];
        let l = args(Cli::Claude, &Request { images: &imgs, ..req("claude:sonnet", None) }).unwrap();
        let p = l.stdin.unwrap_or_default();
        assert!(p.starts_with("What's due?\n\nThe image is at /data/ask/img/abc.jpg: view it with the Read tool"), "{p}");
        assert!(l.args.windows(2).any(|w| w == ["--add-dir", "/data/ask/img"]));
        let c = args(Cli::Codex, &Request { images: &imgs, ..req("gpt-5", None) }).unwrap().args;
        assert!(c.contains(&"--image=/data/ask/img/abc.jpg".to_string()));
        assert_eq!(c[c.len() - 2..], ["--", "-"]);
        for (cli, model) in [(Cli::Copilot, "copilot:auto"), (Cli::Antigravity, "agy:gemini")] {
            assert!(!cli.sees_images());
            let all = args(cli, &Request { images: &imgs, ..req(model, None) }).unwrap().args.join(" ");
            assert!(!all.contains("abc.jpg"), "{all}");
        }
        assert!(Cli::Claude.sees_images() && Cli::Codex.sees_images());
    }

    #[test]
    fn codex_args() {
        let a = args(Cli::Codex, &req("gpt-5", None)).unwrap().args;
        assert_eq!(a[0], "exec");
        assert!(a.windows(2).any(|w| w == ["-c", "sandbox_mode=\"read-only\""]));
        assert!(a.iter().any(|x| x.starts_with("developer_instructions=\"SYS\\n\\n")));
        assert_eq!(a[a.len() - 2..], ["--", "-"]);
        let b = args(Cli::Codex, &req("gpt-5", Some("t1"))).unwrap();
        assert_eq!(b.args[..2], ["exec", "resume"]);
        assert_eq!(b.args[b.args.len() - 3..], ["--", "t1", "-"]);
        assert_eq!(b.stdin.as_deref(), Some("What's due?"));
    }

    #[test]
    fn agy_args_and_agent() {
        let a = args(Cli::Antigravity, &req("agy:gemini-3", Some("conv"))).unwrap().args;
        assert!(a.windows(2).any(|w| w == ["--agent", AGY_AGENT]));
        assert!(a.windows(2).any(|w| w == ["--model", "gemini-3"]));
        assert!(a.windows(2).any(|w| w == ["--add-dir", "/v"]));
        assert!(a.contains(&"--sandbox".to_string()));
        assert_eq!(a[a.len() - 2..], ["--conversation", "conv"]);
        let md = agy_agent("SYS");
        assert!(md.contains("tools:\n  - view_file\n  - grep_search\n  - find_by_name\n  - list_dir\n---"));
        assert!(!md.contains("run_command") && !md.contains("search_web"));
    }

    #[test]
    fn copilot_args() {
        let a = args(Cli::Copilot, &req("copilot:auto", None)).unwrap().args;
        assert!(a[1].starts_with("<instructions>\nSYS\n\n") && a[1].ends_with("</instructions>\n\nWhat's due?"));
        assert!(a.contains(&"--available-tools=view,grep,glob".to_string()));
        assert!(a.windows(2).any(|w| w == ["--model", "auto"]));
        let b = args(Cli::Copilot, &req("copilot:auto", Some("s9"))).unwrap().args;
        assert_eq!(b[1], "What's due?");
        assert_eq!(b.last().unwrap(), "--resume=s9");
    }

    #[test]
    fn a_jobs_own_effort_goes_with_its_model() {
        let ui = serde_json::json!({
            "jobModels": {"contradictions": "claude:opus"},
            "jobEffort": {"contradictions": "max"},
            "effort": {"claude": "low"}
        });
        let ui = ui.as_object().unwrap();
        assert_eq!(job_effort(ui, "contradictions", "claude:opus"), Some("max"));
        // The extract pass and Write Current state, on Haiku, run at Claude Code's.
        assert_eq!(job_effort(ui, "contradictions", "claude:haiku"), Some("low"));
    }

    #[test]
    fn effort_resolves_job_then_assistant() {
        // The same cases as askState.test.ts "resolves a run's effort".
        assert_eq!(resolve_effort(Cli::Claude, Some("high"), Some("low")), Some("high"));
        assert_eq!(resolve_effort(Cli::Claude, None, Some("low")), Some("low"));
        assert_eq!(resolve_effort(Cli::Claude, None, None), None);
        // A level the CLI doesn't offer falls through: Codex takes no max.
        assert_eq!(resolve_effort(Cli::Codex, Some("max"), Some("medium")), Some("medium"));
        assert_eq!(resolve_effort(Cli::Codex, Some("max"), None), None);
    }

    #[test]
    fn job_model_and_effort_follow_the_shared_cases() {
        // src/askState.test.ts reads the same file.
        let f: serde_json::Value = serde_json::from_str(include_str!("../../../tests/fixtures/job-settings.json")).unwrap();
        let keys: Vec<&str> = Job::ALL.iter().map(|j| j.key()).collect();
        assert_eq!(serde_json::json!(keys), f["jobs"]);
        for cli in [Cli::Claude, Cli::Codex, Cli::Antigravity, Cli::Copilot, Cli::Ollama] {
            assert_eq!(serde_json::to_value(cli).unwrap(), cli.key());
            assert_eq!(serde_json::json!(cli.efforts()), f["levels"][cli.key()], "{cli:?}");
        }
        for c in f["cases"].as_array().unwrap() {
            let (name, ui, job) = (&c["name"], c["settings"].as_object().unwrap(), c["job"].as_str().unwrap());
            let model = job_model(ui, job);
            assert_eq!(model, c["model"], "{name}");
            assert_eq!(serde_json::json!(job_own_effort(ui, job)), c["own"], "{name}");
            assert_eq!(serde_json::json!(job_effort(ui, job, &model)), c["effort"], "{name}");
        }
    }

    #[test]
    fn a_level_the_model_lacks_falls_back() {
        let has = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // A CLI that doesn't list a model's levels decides itself.
        assert_eq!(fit_effort(Cli::Claude, "max", None), Some("max"));
        assert_eq!(fit_effort(Cli::Codex, "xhigh", Some(&has(&["low", "medium", "high", "xhigh"]))), Some("xhigh"));
        // The highest below that the model has, else its default.
        assert_eq!(fit_effort(Cli::Codex, "xhigh", Some(&has(&["minimal", "low", "medium", "high"]))), Some("high"));
        assert_eq!(fit_effort(Cli::Codex, "medium", Some(&has(&["high"]))), None);
        assert_eq!(fit_effort(Cli::Codex, "low", Some(&[])), None);
        // A refusal names the effort and comes before any answer.
        let o = |text: &str, e: &str| Outcome { text: text.into(), error: Some(e.into()), ..Default::default() };
        assert!(effort_refused(&o("", "error: unknown option '--effort'")));
        assert!(effort_refused(&o(" ", "Unsupported value: 'xhigh' for reasoning.effort with this model")));
        assert!(!effort_refused(&o("Half an answer", "--effort high: rate limited")));
        assert!(!effort_refused(&o("", "Not signed in.")));
        assert!(!effort_refused(&Outcome::default()));
    }

    #[test]
    fn effort_only_when_set() {
        for (model, flag) in [
            ("claude:sonnet", ["--effort", "high"]),
            ("gpt-5", ["-c", "model_reasoning_effort=\"high\""]),
            ("agy:gemini-3", ["--effort", "high"]),
            ("copilot:auto", ["--reasoning-effort", "high"]),
        ] {
            let cli = Cli::route(model);
            let set = args(cli, &Request { effort: Some("high"), ..req(model, None) }).unwrap().args;
            assert!(set.windows(2).any(|w| w == flag), "{set:?}");
            // Unset, or a level the CLI doesn't name, passes none: the CLI's own default.
            for effort in [None, Some("ultra"), Some("")] {
                let a = args(cli, &Request { effort, ..req(model, None) }).unwrap().args.join(" ");
                assert!(!a.contains("effort"), "{a}");
            }
        }
        assert!(!Cli::Codex.efforts().contains(&"max"));
        // Codex's still reads its prompt last.
        let c = args(Cli::Codex, &Request { effort: Some("low"), ..req("gpt-5", None) }).unwrap().args;
        assert_eq!(c[c.len() - 2..], ["--", "-"]);
    }

    #[test]
    fn claude_stream() {
        let (ev, p) = feed(
            Cli::Claude,
            &[
                r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Let me look."}}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/v/Meeting. Orbit App Steerco - 2026-09-30.md"}}],"usage":{"input_tokens":10,"cache_read_input_tokens":1200,"cache_creation_input_tokens":30}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Two tasks are due."}}}"#,
                r#"{"type":"system","subtype":"compact_boundary","session_id":"s1"}"#,
                r#"{"type":"result","subtype":"success","is_error":false,"result":"Two tasks are due.","session_id":"s2","modelUsage":{"claude-sonnet-5":{"contextWindow":200000}}}"#,
            ],
        );
        assert_eq!(text(&ev), "Let me look.\n\nTwo tasks are due.");
        assert!(ev.contains(&Event::Status { text: "Reading Meeting. Orbit App Steerco - 2026-09-30".into() }));
        assert!(ev.contains(&Event::Usage { tokens: 1240, window: None }));
        assert!(ev.contains(&Event::Usage { tokens: 0, window: Some(200000) }));
        assert!(ev.contains(&Event::Compacted));
        let o = p.finish(true, "");
        assert_eq!(o, Outcome { text: "Let me look.\n\nTwo tasks are due.".into(), session: Some("s2".into()), error: None });
    }

    #[test]
    fn claude_error_and_signed_out() {
        let (_, p) = feed(Cli::Claude, &[r#"{"type":"result","is_error":true,"result":"Prompt is too long"}"#]);
        assert_eq!(p.finish(false, "").error.as_deref(), Some("Prompt is too long"));
        let (_, p) = feed(Cli::Claude, &[]);
        let e = p.finish(false, "Invalid API key · Please run /login").error.unwrap();
        assert!(e.starts_with("Claude Code isn't signed in."), "{e}");
        let (_, p) = feed(Cli::Claude, &[]);
        assert_eq!(p.finish(false, "").error.as_deref(), Some("Claude Code stopped without answering"));
    }

    #[test]
    fn codex_stream() {
        let (ev, p) = feed(
            Cli::Codex,
            &[
                r#"{"type":"thread.started","thread_id":"t1"}"#,
                r#"{"type":"item.started","item":{"type":"command_execution","command":"rg -n due 'Me. To Do List.md'"}}"#,
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"Searching first."}}"#,
                r#"{"type":"error","message":"stream disconnected, reconnecting"}"#,
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"Nothing is due."}}"#,
            ],
        );
        assert_eq!(ev[0], Event::Status { text: "Reading Me. To Do List".into() });
        assert_eq!(text(&ev), "Searching first.\n\nNothing is due.");
        assert_eq!(
            p.finish(true, ""),
            Outcome { text: "Searching first.\n\nNothing is due.".into(), session: Some("t1".into()), error: None }
        );
        let (_, p) = feed(Cli::Codex, &[r#"{"type":"turn.failed","error":{"message":"quota"}}"#]);
        assert_eq!(p.finish(false, "").error.as_deref(), Some("quota"));
    }

    #[test]
    fn agy_stream() {
        let (ev, p) = feed(
            Cli::Antigravity,
            &[
                r#"{"event":"step_update","step_update":{"conversation_id":"c1","step_type":"agent_response","step_index":1,"text_delta":"Looking."}}"#,
                r#"{"event":"step_update","step_update":{"step_type":"tool","state":"ACTIVE","tool_name":"grep_search","tool_info":{"parameters":{}}}}"#,
                r#"{"event":"step_update","step_update":{"step_type":"agent_response","step_index":3,"text_delta":"Found it."}}"#,
                r#"{"event":"result","result":{"conversation_id":"c1","status":"SUCCESS","response":"Found it."}}"#,
            ],
        );
        assert_eq!(text(&ev), "Looking.\n\nFound it.");
        assert!(ev.contains(&Event::Status { text: "Searching the vault".into() }));
        assert_eq!(p.finish(true, "").session.as_deref(), Some("c1"));
        let (_, p) = feed(
            Cli::Antigravity,
            &[r#"{"event":"result","result":{"status":"SUCCESS","denied_actions":[{"display_name":"run_command"}]}}"#],
        );
        assert!(p.finish(true, "").error.unwrap().contains("run_command"));
    }

    #[test]
    fn copilot_stream() {
        let (ev, p) = feed(
            Cli::Copilot,
            &[
                r#"{"type":"assistant.message_delta","data":{"messageId":"m1","deltaContent":"One."}}"#,
                r#"{"type":"tool.execution_start","data":{"toolName":"view","arguments":{"path":"/v/wiki/entities/Orbit App.md"}}}"#,
                r#"{"type":"assistant.message_delta","data":{"messageId":"m2","deltaContent":"Two."}}"#,
                r#"{"type":"result","sessionId":"cs"}"#,
            ],
        );
        assert_eq!(text(&ev), "One.\n\nTwo.");
        assert!(ev.contains(&Event::Status { text: "Reading Orbit App".into() }));
        assert_eq!(p.finish(true, ""), Outcome { text: "One.\n\nTwo.".into(), session: Some("cs".into()), error: None });
        let (_, p) = feed(Cli::Copilot, &[]);
        assert_eq!(p.finish(true, "").error.as_deref(), Some("GitHub Copilot stopped without answering"));
    }
}
