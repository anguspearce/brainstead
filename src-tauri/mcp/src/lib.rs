// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Brainstead's MCP server, over stdio: newline-delimited JSON-RPC, as every CLI that Ask runs
//! speaks it. Each CLI starts it (the app's own binary with `--mcp`, or `brainstead-mcp` from a
//! terminal), so it works whether Brainstead is open or not.
//!
//! It reads: the vault's files, and the app's index opened read-only. It never changes the vault
//! itself: a change (edit_page, create_note, a task, a rename…) is checked here, then sent to the
//! app, which makes it or holds it for the user and records it in Changes (decided 2026-10-05).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use brainstead_core::changes::{self, Instruction};
use brainstead_core::proposals::{self, Kind, Origin, Patch, Quote, SourceText};
use brainstead_core::search::SearchRequest;
use brainstead_core::{bridge, extract, filename, images, lint, paths, projects, write, Index};
use serde::Deserialize;
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

/// Protocol versions it speaks, newest first; it answers with the client's when it's one of them.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// The most text one tool result carries.
const MAX_CHARS: usize = 30_000;
const TODO_LIST: &str = "Me. To Do List.md";

/// What every session is told first. Clients cut it short (Claude Code at about 2,000 characters),
/// so it stays under that; each tool's description says the rest.
const INSTRUCTIONS: &str = "Brainstead's tools over the user's vault (notes, a wiki in wiki/, sources in sources/), doing what the app's screens do. \
The list_* tools and the others marked read-only only read; help answers questions about the app itself. \
Every change to a note (tasks, the Inbox, projects, edit_page, create_note, add_image, rename_note, trash_note) is made at once and recorded in Brainstead's Changes, where the user can revert it. \
A change to Templates/, one that adds code that runs (a script, a Tasks function, a Templater tag) or one that changes a system note's header is always held for the user to accept in the app. \
Other tools change things at once; each one's description says how it's undone. \
Change only what the user asked for. While the vault is read-only (app_status says) every change is refused. \
In a session nobody is watching (a loop, a scheduled agent), pass unattended: true on every call: a change that fails a check is then held for the user, and for the rest of the session nothing held can be accepted and weekly_start_over and moving_over are refused. \
changes accepts a held change only while the user is there, and only one held because a check failed. \
Quote sources word for word in edit_page's quotes: a quote that isn't in its source is flagged. \
Tools marked as needing the app open Brainstead when it isn't running.";

/// Where the server works.
#[derive(Debug)]
pub struct Ctx {
    /// The app data folder: `settings.json`, `index.db`, `text/` (what was read from sources) and
    /// `bridge/` (requests to the running app).
    pub data: PathBuf,
    pub vault: PathBuf,
    /// The Ask chat that started it, and its model, for Changes.
    pub chat: Option<String>,
    pub model: Option<String>,
    /// Today, for `updated:` (tests fix it).
    pub today: chrono::NaiveDate,
    /// The app's own program, to open it for an action when it isn't running; None from the
    /// separate `brainstead-mcp` binary and in tests.
    pub app: Option<PathBuf>,
    /// When the last calls came, for the rate limit.
    pub calls: std::sync::Mutex<Vec<std::time::Instant>>,
    /// This server's session, grouping a terminal's changes in Changes when no chat started it.
    pub session: String,
    /// A call in this session said nobody is watching: from then on it can't accept held changes.
    pub unattended: std::sync::atomic::AtomicBool,
    /// What `initialize` agreed, handed on to a new build of the server (`reload`).
    pub init: std::sync::Mutex<Option<Value>>,
}

impl Ctx {
    /// From the command line (`--data <dir>`, `--chat <id>`, `--model <id>`), the vault being the one
    /// in the data folder's `settings.json`. `default_data` is the app data folder when no `--data`.
    pub fn from_args(args: &[String], default_data: PathBuf) -> Result<Ctx, String> {
        let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        let data = opt("--data").map(PathBuf::from).unwrap_or(default_data);
        let settings: Value = std::fs::read(data.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or_else(|| format!("No Brainstead settings in {}: open Brainstead and choose the vault first.", data.display()))?;
        let vault = settings
            .get("vaultPath")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .ok_or("Brainstead has no vault chosen yet.")?;
        // Started as the app (`Brainstead --mcp`): the same program opens the app.
        let app = std::env::current_exe().ok().filter(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            !stem.starts_with("brainstead-") && !stem.starts_with("brainstead_")
        });
        Ok(Ctx {
            data,
            vault,
            chat: opt("--chat"),
            model: opt("--model"),
            today: chrono::Local::now().date_naive(),
            app,
            calls: Default::default(),
            session: format!("session-{}", proposals::new_id()),
            unattended: Default::default(),
            init: Default::default(),
        })
    }

    fn index(&self) -> Result<Index, String> {
        Index::open_read_only(&self.data.join("index.db"))
    }
}

/// Answers requests from `input`, one JSON message per line, until it closes.
pub fn serve(ctx: &Ctx, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        answer(ctx, &line?, &mut output)?;
    }
    Ok(())
}

/// Answers one line, if it needs an answer.
fn answer(ctx: &Ctx, line: &str, output: &mut impl Write) -> std::io::Result<()> {
    if line.trim().is_empty() {
        return Ok(());
    }
    let reply = match serde_json::from_str::<Value>(line) {
        Ok(Value::Array(batch)) => {
            let out: Vec<Value> = batch.into_iter().filter_map(|m| handle(ctx, m)).collect();
            (!out.is_empty()).then_some(Value::Array(out))
        }
        Ok(m) => handle(ctx, m),
        Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("Parse error: {e}")}})),
    };
    if let Some(r) = reply {
        writeln!(output, "{r}")?;
        output.flush()?;
    }
    Ok(())
}

/// What a server started by a new build of itself tells the client first: the tools may have
/// changed. Nothing when the client hadn't initialized.
pub fn resumed(ctx: &Ctx) -> Option<Value> {
    ctx.init.lock().unwrap().as_ref()?;
    Some(json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}))
}

/// `serve` on stdin and stdout, reading stdin unbuffered so that, when a new build replaces the
/// program, it can exec it between requests with nothing read and unanswered (`reload`).
#[cfg(unix)]
fn serve_stdio(ctx: &Ctx) -> std::io::Result<()> {
    let mut output = std::io::stdout().lock();
    if let Some(n) = resumed(ctx) {
        writeln!(output, "{n}")?;
        output.flush()?;
    }
    let mut watch = reload::Watch::new();
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        while let Some(end) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=end).collect();
            answer(ctx, String::from_utf8_lossy(&line[..end]).trim_end_matches('\r'), &mut output)?;
        }
        // Every request read is answered; part of the next one would be lost.
        if buf.is_empty() {
            if let Some(w) = watch.as_mut() {
                if w.due() {
                    w.exec(&reload::Resume::of(ctx));
                }
            }
        }
        if !reload::readable(reload::CHECK_EVERY)? {
            continue;
        }
        let n = reload::read(&mut chunk)?;
        if n == 0 {
            return answer(ctx, &String::from_utf8_lossy(&buf), &mut output);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// One message; None for notifications, which get no answer.
pub fn handle(ctx: &Ctx, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(VERSIONS[0]);
            let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
            *ctx.init.lock().unwrap() = Some(
                json!({"protocolVersion": version, "clientInfo": params.get("clientInfo"), "capabilities": params.get("capabilities")}),
            );
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": true}},
                "serverInfo": {"name": "brainstead", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut list = Value::Array(tools());
            keys_in_descriptions(&mut list);
            Ok(json!({"tools": list}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(ctx, name, args) {
                Ok(out) => {
                    let mut r = json!({"content": [{"type": "text", "text": cap(out.text)}], "isError": false});
                    if let Some(s) = out.structured {
                        r["structuredContent"] = s;
                    }
                    Ok(r)
                }
                Err(CallError::Tool(e)) => Ok(json!({"content": [{"type": "text", "text": e}], "isError": true})),
                Err(CallError::Unknown(e)) => Err((-32602, e)),
            }
        }
        "prompts/list" => Ok(json!({"prompts": []})),
        "resources/list" => Ok(json!({"resources": []})),
        "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
        _ => Err((-32601, format!("Method not found: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
    })
}

fn cap(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_CHARS).collect();
    format!("{cut}\n\n[Cut at {MAX_CHARS} characters. Ask for less: one section, or fewer results.]")
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

/// What a tool does to the vault, for its annotations (MCP's tool hints).
#[derive(Clone, Copy)]
enum Effect {
    /// Reads only.
    Read,
    /// Changes the vault straight away, undoably (⌘Z in the app, Revert in Changes).
    Change,
    /// Can write over or take out what's in the vault: deletes, accepting and reverting changes.
    Destroy,
    /// Starts work by a model on the user's account.
    Run,
}

/// Every tool takes `unattended`, so a client that passes it on every call, as the instructions
/// ask, isn't turned away by a schema that allows no other properties. Said once, it holds for the
/// whole session (D-20261005-14).
fn unattended() -> Value {
    json!({"type": "boolean", "description": "true when nobody is watching this session (a loop, a scheduled agent): a change that fails a check is held for the user instead of made, and from then on nothing held can be accepted and weekly_start_over and moving_over are refused."})
}

/// `text` with its shortcuts as this system names them. The help and the tools write them with
/// macOS glyphs (⌘Z), which Windows names in words (Ctrl+Z), as the window does.
fn keys(text: &str) -> String {
    #[cfg(windows)]
    return brainstead_core::help::windows_keys(text);
    #[cfg(not(windows))]
    text.to_string()
}

/// Every `description` in `v` (the tools and their inputs) through `keys`.
fn keys_in_descriptions(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                match x {
                    Value::String(s) if k == "description" => *s = keys(s),
                    _ => keys_in_descriptions(x),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(keys_in_descriptions),
        _ => {}
    }
}

fn tool(name: &str, title: &str, effect: Effect, description: &str, mut input: Value) -> Value {
    if let Some(props) = input.get_mut("properties").and_then(Value::as_object_mut) {
        props.entry("unattended").or_insert_with(unattended);
    }
    let (read_only, destructive, idempotent, open_world) = match effect {
        Effect::Read => (true, false, true, false),
        Effect::Change => (false, false, false, false),
        Effect::Destroy => (false, true, false, false),
        Effect::Run => (false, false, false, true),
    };
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": idempotent,
            "openWorldHint": open_world,
        },
    })
}

const EDIT_PAGE: &str = "Changes one page (a note, a wiki page, a project: any page in the vault), or makes a new wiki page (needs the app); a new note is create_note. The change is made at once and recorded in Brainstead's Changes, where the user can revert it. Give exactly one of: section + content (that section's new text, without its heading; a section that isn't there is added at the end, or on a wiki entity or concept page where the page shape puts it: a topical one above the Timeline, a summing one such as Summary or Status as the Current state), edits (exact find and replace pairs, each find text appearing once in the page), or content alone (the whole page; required for a new page, whose path must be wiki/entities/<Name>.md, wiki/concepts/<Name>.md or wiki/summaries/<Name>.md). Keep what's in the text you replace that you aren't changing: links, tags, and block ids such as ^q3 at a line's end. Cite what the change rests on in quotes, copied word for word from the source; cite in the text as [[Source#Heading]] or [[Source.pdf#page=6]]; a PDF quote with anchor page=N must be on that page. A quote that isn't in its source is flagged to the user.";

/// The tools, as `tools/list` gives them. Those marked "needs the app" run in Brainstead itself
/// (the server opens it when it isn't running), so they behave as the screens do. What only reads
/// is a tool of its own (list_changes beside changes…), so a client can let it run unasked.
pub fn tools() -> Vec<Value> {
    use Effect::*;
    let page = json!({"type": "string", "description": "A page's name (Orbit App), its [[link]], or its path in the vault (wiki/entities/Orbit App.md)."});
    let task = json!({"type": "string", "description": "The task's id from list_tasks: path:line, as in `Me. To Do List.md:12`."});
    let task_text = json!({"type": "string", "description": "The task's text as list_tasks showed it (or enough of it), to make sure it's the same task if lines moved."});
    let date = json!({"type": ["string", "null"], "pattern": DATE, "description": "YYYY-MM-DD, or null to clear it."});
    let day = |what: &str| json!({"type": "string", "pattern": DATE, "description": format!("YYYY-MM-DD: {what}")});
    let reason = json!({"type": "string", "description": "Why, briefly: shown with the change in Changes."});
    let detail =
        json!({"type": "boolean", "description": "true for the full rows (see the description); a shorter line each when left out."});
    let layers = json!({"type": "array", "items": {"type": "string", "enum": ["notes", "wiki", "sources", "templates"]}, "description": "Only these parts of the vault; all when left out."});
    let edit_schema = schema(
        json!({
            "page": page,
            "title": {"type": "string", "description": "One line for Changes: what the change does."},
            "reason": reason,
            "section": {"type": "string", "description": "With content: the heading of the section to replace (its text only, without #s)."},
            "content": {"type": "string", "description": "With section, that section's new text without its heading; alone, the whole page."},
            "edits": {"type": "array", "description": "Find and replace pairs, each find text appearing exactly once in the page.", "items": schema(json!({
                "find": {"type": "string", "description": "Text as it is in the page now, word for word."},
                "replace": {"type": "string", "description": "What it becomes."}
            }), &["find", "replace"])},
            "quotes": {"type": "array", "description": "What the change rests on, quoted from its sources.", "items": schema(json!({
                "source": {"type": "string", "description": "The source's name or path."},
                "anchor": {"type": "string", "description": "A heading in it, or page=N for a PDF."},
                "text": {"type": "string", "description": "Word for word from the source."}
            }), &["source", "text"])}
        }),
        &["page", "title"],
    );
    let mut all = vec![
        tool("search", "Search the vault", Read,
            "Full-text search over the vault, as Brainstead's Search does. Returns the best matching files, each with its path, title, the heading of the best passage and a snippet of it. Use it to find where something is written; read_section then reads the page. detail gives the whole snippet with each file's part of the vault and date.",
            schema(json!({
                "query": {"type": "string", "description": "Words, \"exact phrases\", +required, -excluded and #tag, as Search takes them: soft launch +Orbit."},
                "layers": layers,
                "since": day("only files from this day on, by the note's own date (else the day it last changed), as Search's date range adds since: to the query."),
                "limit": {"type": "integer", "minimum": 1, "maximum": 30, "description": "How many files, at most 30. Default 10."},
                "order": {"type": "string", "enum": ["best_match", "latest", "oldest"], "description": "Search's Order: best_match (Best match, the default), latest (Latest) or oldest (Oldest), by the note's date or the day it last changed."},
                "detail": detail
            }), &["query"])),
        tool("read_section", "Read a page", Read,
            "Reads a page, or one section of it by heading (to the next heading of the same level). A long page without a heading lists its headings, so read it a section at a time. For a PDF or Office source, heading picks a page (page=6), slide (Slide 2) or sheet by name.",
            schema(json!({"page": page, "heading": {"type": "string", "description": "The section's heading text (Current state), or for a PDF or Office file a page, slide or sheet. The whole page when left out."}}), &["page"])),
        tool("backlinks", "Pages linking here", Read,
            "The notes and pages that link to a page, each with the line its link is on. Use it to see where something is mentioned before renaming or changing it, or to find the notes about an entity. Links are resolved as the app resolves them: by name, alias or path.",
            schema(json!({"page": page}), &["page"])),
        tool("resolve_entity", "Which page a name means", Read,
            "Which page a name means: by file name, then alias, then a close spelling. Use it before creating a page, so you don't make a second page for something that has one.",
            schema(json!({"name": {"type": "string", "description": "The name as written: Orbit, Orbit App, Lena."}}), &["name"])),
        tool("facts", "Facts from the wiki", Read,
            "The facts ingest checked against their sources and kept with each wiki page: per subject and attribute, the latest value with its as of date, quote, source and Timeline entry, then the values it superseded. Answers \"what's the latest go_live_date for Orbit App\" without reading pages. Give a page, a subject, an attribute (go_live_date, owner, status, role…; synonyms count), or any mix. Facts are kept from the first ingest that checks them, so a page may have none yet.",
            schema(json!({
                "page": page,
                "subject": {"type": "string", "description": "What the fact is about, by name: Orbit App."},
                "attribute": {"type": "string", "description": "Which fact: go_live_date, owner, status, role… (synonyms count)."}
            }), &[])),
        tool("pending_sources", "Sources not yet in the wiki", Read,
            "Files in sources/ with their Status, as the Sources screen gives it: New (no wiki page cites it yet in its sources: property), Changed (changed since the pages citing it) or Ingested. Without status, the ones still to ingest, New and Changed: notes, PDFs, Office files and images (whose text is read from the picture), each path ready to pass to start_run's ingest (a Teams transcript is written up with start_run meeting_note instead); a file an ingest can't read is never listed as one to ingest, and status all marks it. An empty list means the wiki is up to date with Sources.",
            schema(json!({"status": {"type": "string", "enum": ["all", "new", "changed", "ingested"], "description": "The Sources screen's Status: only New, Changed or Ingested sources, or all; New and Changed when left out."}}), &[])),
        tool("provenance", "A source's provenance", Read,
            "A source's Provenance card, as Sources and the source's own page show it (needs the app): whether it's ingested and by which run (when, its status and model), its size and when it changed, what the search index holds of it (passages indexed, its SHA-256), and the wiki pages that cite it with the passages they quote from it (three a page, as the card shows). Re-ingest is start_run ingest; Move to the Trash is trash_note.",
            schema(json!({"source": {"type": "string", "description": "The source, by its path in the vault (sources/…), as pending_sources gives it."}}), &["source"])),
        tool("lint", "Knowledge health report", Read,
            "Knowledge health's checks on the wiki, as its screen lists them when Brainstead is open (ignored issues and pairs marked not duplicates left out, and the checks only the app runs: claims with no citation and system notes missing their header), else from the vault. Each check says whether it's counted (the grey ones, only worth a look, aren't) and each item marked [safe fix] can be fixed with fix_health. Wiki checks: missing pages, broken sources, orphan pages, missing cross-links, stale updated: dates, unlogged writes, sources not yet ingested and images nothing uses. More checks: possible duplicates, stale pages others rely on, sources changed since they were cited, pages not in the page shape (reshape_pages) and pages with no Current state (write_current_state). For one page when given. With the app, the whole report starts with the screen's summary card: how many Need a decision, how many more are safe to fix, and the trend (the first and last day's count, and the highest). With the app, a missing page or broken source also says, as its row does, when the file it names is in the Trash (since when, and the id for restore_from_trash) and Did you mean, the note it most likely meant (the meeting note a trashed transcript became, or one a slip away), for health_issue link_to. Missing pages, broken sources (link_to), duplicates, unused images and orphan pages (add_to_index) have health_issue for their buttons.",
            schema(json!({"page": page}), &[])),
        tool("help", "Brainstead's help", Read,
            "Brainstead's own help: how the app works and how to use it (screens, settings, shortcuts, getting-started guides). For questions about the app itself, not the vault. No arguments lists the topics; `topic` reads one (and `section` one part of it); `query` finds the best matching sections.",
            schema(json!({
                "topic": {"type": "string", "description": "A topic's id or title, from the list."},
                "section": {"type": "string", "description": "One section of the topic, by its title."},
                "query": {"type": "string", "description": "Words to search the help for: change capture shortcut."}
            }), &[])),
        // Tasks.
        tool("list_tasks", "List tasks", Read,
            "Tasks as Brainstead's lists show them (needs the app). Each line gives the task, its dates and chips (priority and #tags among them), and its id (path:line) to act on it with edit_task, move_task or delete_task. Narrow with view, project, context, effort, query (every word somewhere in the task's words, note, heading, project, #tags or contexts, accents ignored, as the Tasks screen's search matches) or a saved list rather than raising limit; offset gives the next page, as the reply says. group puts them under headings, as the Tasks screen's Group by does.",
            schema(json!({
                "view": {"type": "string", "enum": ["today", "next", "followups", "waiting", "deferred", "someday", "done-this-week", "done-last-week", "all"], "description": "today: the Today screen's bands, Overdue, Due today, Deferred until now and Waiting for, each under its label (a task can be in two), leaving out tasks deferred to a later day. next: open tasks that aren't follow-ups, waiting, someday, deferred or still in the Inbox (the default). deferred: the Deferred list, tasks deferred to a later day. all: every open task."},
                "project": {"type": "string", "description": "Only this project's tasks, by its name."},
                "context": {"type": "string", "description": "Only tasks with this context: calls, office…"},
                "effort": {"type": "integer", "enum": [15, 30, 60], "description": "Only tasks of this many minutes or less, as the Tasks screen's effort filter (15 min or less, 30 min or less, 1 hour or less); any effort when left out."},
                "group": {"type": "string", "enum": ["none", "project", "context", "due"], "description": "Group by, as the Tasks screen does: under each project, context or due date (Overdue, Today, This week, Later, No due date). Default none."},
                "detail": {"type": "boolean", "description": "Also the heading, effort, created and done dates."},
                "list": {"type": "string", "description": "A saved list's name (list_task_lists): its view, context, effort and grouping, instead of view, context, effort and group."}
            }), &[])),
        tool("edit_task", "Change a task", Change,
            "Changes a task, as the task screens do (needs the app): made straight away and recorded in Changes, where the user can revert it (⌘Z undoes it too). Give only the fields to change; several at once is fine, made as one change. Ticking a recurring task adds its next occurrence, as in the app.",
            schema(json!({
                "task": task, "text": task_text,
                "words": {"type": "string", "description": "The task's new words, as renaming it on the Tasks screen does: its dates, fields and tags stay as they are."},
                "done": {"type": "boolean", "description": "true ticks it (with today's ✅ date), false unticks it."},
                "due": date, "defer": date, "start": date, "created": date,
                "priority": {"type": "string", "enum": ["highest", "high", "medium", "low", "lowest", "none"], "description": "none takes the priority off."},
                "contexts": {"type": "array", "items": {"type": "string"}, "description": "The task's contexts, replacing what it has: [\"calls\"]. [] clears them."},
                "effort": {"type": ["string", "null"], "description": "15m, 1h, 1h30m, 2d; null clears it."},
                "project": {"type": ["string", "null"], "description": "A project's name; null takes it out of its project."},
                "waiting": {"type": "boolean", "description": "true marks it waiting for someone (#waiting-for), false no longer."},
                "followup": {"type": "boolean", "description": "true makes it a follow-up (#followup, in the Follow-ups list), false no longer."},
                "status": {"type": "string", "enum": ["cancelled", "open", "in_progress"], "description": "cancelled drops it without deleting the line."}
            }), &["task", "text"])),
        tool("move_task", "Reorder a task", Change,
            "Moves a task in the manual order of Next actions, Follow-ups, Waiting for or Someday / maybe (needs the app), next to another task in the same list, as dragging it there does: between its neighbours in that list, or the list renumbered when some of it has no rank yet or there's no room. With project, it orders that project's Next actions instead, as dragging on the project's page does. Give after or before, with the other task's id from list_tasks. The move is recorded in Changes, where the user can revert it. To change what a task is (its dates, project or status), use edit_task.",
            schema(json!({
                "task": task, "text": task_text,
                "after": {"type": "string", "description": "The id of the task it should come after."},
                "before": {"type": "string", "description": "The id of the task it should come before."},
                "project": {"type": "string", "description": "A project's name (Orbit App launch) or its note: order the task in that project's Next actions, as on the project's page, rather than in the Tasks screen's lists."}
            }), &["task", "text"])),
        tool("create_task", "Add a task", Change,
            "Adds a task (needs the app): to a project's Next actions when a project is given, else to the To Do list's Other section, which is the Inbox: a task there with no context and none of the lists' tags waits in the Inbox to be clarified (list_inbox), as on the Tasks screen. Write it as one line starting with a verb. view adds it to one of the Tasks screen's lists as New task there does, with that list's tag (Follow-ups #followup, Waiting for #waiting-for, Someday / maybe #someday-maybe). It's recorded in Changes, where the user can revert it. To capture something still to be clarified, use capture; add only what the user asked for.",
            schema(json!({
                "text": {"type": "string", "description": "The task, one line, starting with a verb."},
                "view": {"type": "string", "enum": ["followups", "waiting", "someday"], "description": "The Tasks screen's list it's added in, as list_tasks names them: its tag goes on the task. Next actions when left out."},
                "project": {"type": "string", "description": "A project's name (Orbit App launch) or its note."},
                "contexts": {"type": "array", "items": {"type": "string"}, "description": "Where or with what, as edit_task takes them: [\"calls\"], [\"computer\", \"office\"]."},
                "effort": {"type": "string", "description": "15m, 1h, 1h30m, 2d."},
                "due": day("the due date."),
                "defer": day("Deferred until: the day it shows in Next actions from, as edit_task's defer."),
                "start": day("the start date, as edit_task's start."),
                "reason": reason
            }), &["text"])),
        tool("delete_task", "Delete a task", Destroy,
            "Takes a task's line out of its note (needs the app); revertable in Changes. Only when the user asks: to finish a task, tick it with edit_task instead; to drop it, set status cancelled.",
            schema(json!({"task": task, "text": task_text, "reason": reason}), &["task", "text"])),
        // Inbox and projects.
        tool("list_inbox", "List the Inbox", Read,
            "What's waiting in the Inbox to clarify (needs the app): captures from Outlook and Teams and other sources not yet ingested, Scratchpad thoughts and tasks under Other on the To Do list. Each row has its id (task:14, capture:…) for clarify_inbox. Clarified items aren't listed. With suggest, the model suggests what each of the first 20 rows shown is, as the Inbox's Suggest does; nothing changes, and accepting one is clarify_inbox with its values.",
            schema(json!({"suggest": {"type": "boolean", "description": "true asks the model what each of the first 20 rows shown is (a run on the user's account): becomes, text, project, context, effort, due and why, as clarify_inbox takes them."}}), &[])),
        tool("clarify_inbox", "Clarify an Inbox item", Change,
            "Decides what an Inbox item is, as the Inbox screen does (needs the app); its edits to notes are recorded in Changes, where the user can revert them, and held together when one is held. next and waiting write a task (with project, under the project's Next actions, or its Waiting for for waiting, as the Inbox files them; next needs a project or a context, as the Inbox's form does); someday writes a #someday-maybe task; project makes a new project from it; done ticks or clears it; reference files it in a note (note or new_note), or keeps a capture in Sources; ingest ingests a capture into the wiki; delete takes it out (a capture goes to the Trash). Each kind of item offers only the Inbox's choices for it: a Teams transcript only ingest, reference or delete (its meeting note is start_run meeting_note); an email or chat thread's reply is draft_reply; a capture isn't done (reference keeps it in Sources), and only an email or chat thread can be waiting.",
            schema(json!({
                "item": {"type": "string", "description": "The item's id from list_inbox, as in task:14."},
                "becomes": {"type": "string", "enum": ["next", "waiting", "someday", "project", "done", "reference", "ingest", "delete"], "description": "What it is, as the Inbox's buttons name it."},
                "text": {"type": "string", "description": "The task's text, if it should read differently: start with a verb."},
                "project": {"type": "string", "description": "For next, waiting or someday: the active project it belongs to, by name (the Inbox offers only active ones)."},
                "context": {"type": "string", "description": "For next, waiting or someday: where or with what, as in calls or office."},
                "effort": {"type": "string", "description": "15m, 1h…"},
                "due": day("the task's due date."),
                "project_name": {"type": "string", "description": "For project: the new project's name (the item's text when left out)."},
                "area": {"type": "string", "description": "For project: the area of responsibility it sits under."},
                "done_looks_like": {"type": "string", "description": "For project: what done looks like, one sentence."},
                "note": {"type": "string", "description": "For reference: the path of the note to add it to as a bullet."},
                "new_note": {"type": "string", "description": "For reference: a new note's title to hold it."}
            }), &["item", "becomes"])),
        tool("capture", "Quick capture", Change,
            "Captures something to clarify later, as Quick capture does (needs the app): a task goes to the Inbox (the top of the To Do list's Other section), a thought to the Scratchpad under today's date and time; both then wait in the Inbox (list_inbox). The capture box's shorthand works: due:, defer:, start: with a date or a word such as tomorrow, @context and effort:. Recorded in Changes, where the user can revert it. Capture only what the user asked for; a task they've decided on is create_task.",
            schema(json!({
                "kind": {"type": "string", "enum": ["task", "thought"], "description": "Quick capture's Task (to the Inbox) or Thought (to the Scratchpad)."},
                "text": {"type": "string", "description": "What to capture: a task on one line, or a thought (several lines are fine)."},
                "reason": reason
            }), &["kind", "text"])),
        tool("list_projects", "List projects", Read,
            "Every project, as the Projects screen lists them (needs the app), in its order (by area, as dragged there or with move_project, then A to Z): its name, status and count of next actions, flagged as the screen flags an active one (Stuck: no next action; Quiet: nothing has happened for two weeks), with its note's path. detail adds the area, what done looks like and the waiting, someday and done counts. Statuses are the screen's tabs: active, on-hold, someday, completed. Use the name with list_tasks' project, create_task or update_project.",
            schema(json!({"detail": detail}), &[])),
        tool("create_project", "Create a project", Change,
            "Makes a project note, as the Projects screen does (needs the app). It's named and laid out as the vault's other projects are, and recorded in Changes, where the user can revert it. Add its first tasks with create_task and the project's name.",
            schema(json!({
                "name": {"type": "string", "description": "The project's name, as its note will be titled: Orbit App launch."},
                "status": {"type": "string", "enum": ["active", "on-hold", "someday", "completed"], "description": "Default active."},
                "area": {"type": "string", "description": "The area of responsibility it sits under."},
                "done_looks_like": {"type": "string", "description": "What done looks like, one sentence."}
            }), &["name"])),
        tool("update_project", "Change a project", Change,
            "Sets a project's status, area or what done looks like (needs the app), as its page on the Projects screen does. Give only what changes; null clears area or done_looks_like. Recorded in Changes, where the user can revert it. To rename a project, as clicking its name on its page does, use rename_note on its note with type Project.",
            schema(json!({
                "project": {"type": "string", "description": "The project's name."},
                "status": {"type": "string", "enum": ["active", "on-hold", "someday", "completed"], "description": "The Projects screen's tabs; completed is its Mark complete (the Completed tab)."},
                "area": {"type": ["string", "null"], "description": "The area of responsibility; null clears it."},
                "done_looks_like": {"type": ["string", "null"], "description": "What done looks like, one sentence; null clears it."}
            }), &["project"])),
        tool("move_project", "Reorder a project or an area", Change,
            "Puts a project in order among its area's projects, or an area among the areas, on the Projects screen (needs the app), as dragging it there does. Give project or area, and after or before: another project in the same area, or another area. The order is Brainstead's own setting, not written to any note, so it isn't in Changes; move it again to undo. Projects and areas never moved come after, A to Z, and No area stays last; list_projects lists them in this order.",
            schema(json!({
                "project": {"type": "string", "description": "The project's name, to move it among its area's projects."},
                "area": {"type": "string", "description": "An area, to move it among the areas."},
                "after": {"type": "string", "description": "The project (or area) it should come after."},
                "before": {"type": "string", "description": "The project (or area) it should come before."}
            }), &[])),
        tool("rename_area", "Rename an area", Change,
            "Renames an area of responsibility on every project under it, whatever its status (needs the app), as the pencil on the area's band on the Projects screen does; the projects' notes are changed together. Giving an area that exists merges the two. To move one project to another area, use update_project. Recorded in Changes, where the user can revert it.",
            schema(json!({
                "area": {"type": "string", "description": "The area as it is now, as list_projects' detail gives it."},
                "to": {"type": "string", "description": "Its new name."}
            }), &["area", "to"])),
        // Notes and pages.
        tool("edit_page", "Change a page", Change, EDIT_PAGE, edit_schema),
        tool("create_note", "Make a new note", Change,
            "Makes a new note (not a wiki page: edit_page makes those), needs the app; recorded in Changes, where the user can revert it. Brainstead names it the vault's way, `Type. Title - YYYY-MM-DD.md`, from type, title and date. Either give content (write it as the vault's other notes of that type are written: read the type's template in Templates/ with read_section and follow its properties and headings), or give template to have Brainstead run that template as New note does, answering its questions from answers in order (with test_run, only shown, as the template editor's Test run does); other notes the template makes (tp.file.create_new) are new notes too, made or held together with this one. Once the note is made, the template's finishing steps (tp.hooks.on_all_templates_executed) run, as after New note. To change a note that exists, use edit_page.",
            schema(json!({
                "type": {"type": "string", "description": "The note's type, as the vault's names use it: Meeting, 1-1, Idea, Project, Ask… Leave out for a plain title."},
                "title": {"type": "string", "description": "The note's title, the part of its name after the type."},
                "date": {"type": "string", "pattern": DAY_OR_MONTH, "description": "YYYY-MM-DD (or YYYY-MM), for notes about a day: meetings, 1-1s, journals."},
                "folder": {"type": "string", "description": "A folder in the vault to put it in; the top level when left out. Not sources/, wiki/ or Templates/."},
                "content": {"type": "string", "description": "The whole note: properties (frontmatter) if any, then the body. Not with template."},
                "template": {"type": "string", "description": "A template's name in Templates/ (Meeting, 1-1…) to make the note from, instead of content."},
                "answers": {"type": "array", "items": {"anyOf": [{"type": "string"}, {"type": "array", "items": {"type": "string"}}, {"type": "null"}]}, "description": "With template: answers to its questions, in the order it asks them: text, a choice by its label, or for a question that takes several choices a list of their labels. null is the screen's Cancel (no answer; the template carries on), as \"\" is for a question with choices."},
                "test_run": {"type": "boolean", "description": "With template: the template editor's Test run, the note it would make shown and nothing written (other notes it would make only listed)."},
                "reason": reason
            }), &["title"])),
        tool("add_image", "Add an image to the vault", Change,
            "Saves an image into the vault's images/ folder, as pasting one into a note does (needs the app), for a note to embed: it answers with the image's path and the embed to put in the note's text with create_note or edit_page, ![[images/<name>]]. Give path (an image file on this computer) or base64 (the image's bytes) with name. Its type is read from its bytes: png, jpg, gif, webp, bmp, avif or svg, up to 20 MB. It never writes over an image: a name that's taken gets -2, -3… Recorded in Changes, where Revert moves it to Brainstead's Trash.",
            schema(json!({
                "path": {"type": "string", "description": "The image file's path on this computer (absolute, ~/…, or relative to this session's folder). Not with base64."},
                "base64": {"type": "string", "description": "The image's bytes, base64-encoded. Not with path."},
                "name": {"type": "string", "description": "Its file name, such as launch-plan.png (the extension is set from the bytes). With path, the file's own name when left out."},
                "reason": reason
            }), &[])),
        tool("rename_note", "Rename a note", Change,
            "Renames a note or any file in the vault in its folder, with the links to it rewritten (needs the app); revertable in Changes. Give the new name's parts; Brainstead builds `Type. Title - date` and keeps the file's own extension, as the file menu's Rename does (a source report.pdf stays a PDF). With preview, only the links it would rewrite are listed, as the Rename box lists them. A rename that touches Templates/ is held for the user.",
            schema(json!({
                "page": page,
                "type": {"type": "string", "description": "The new name's type: Meeting, Idea… Leave out for a plain title."},
                "title": {"type": "string", "description": "The new name's title."},
                "date": {"type": "string", "pattern": DAY_OR_MONTH, "description": "The new name's date, YYYY-MM-DD or YYYY-MM, if it has one."},
                "preview": {"type": "boolean", "description": "Only list the links that would be rewritten, file by file, as the Rename box does; nothing is renamed."},
                "reason": reason
            }), &["page", "title"])),
        tool("trash_note", "Move a note to the Trash", Destroy,
            "Moves a note or page to Brainstead's Trash (needs the app), only when the user asks. A Teams transcript's links are first pointed at its meeting note, when it has one, as the app does. It can be put back with restore_from_trash or reverted in Changes. To see what's in the Trash, use list_trash.",
            schema(json!({"page": page, "reason": reason}), &["page"])),
        tool("list_trash", "What's in the Trash", Read,
            "Lists what's in Brainstead's Trash, newest first, with how many items it holds and their size as the Trash screen gives it (needs the app). Each row has its id for restore_from_trash. Moving something to the Trash is trash_note.",
            schema(json!({}), &[])),
        tool("restore_from_trash", "Restore from the Trash", Change,
            "Puts an entry from Brainstead's Trash back where it was, as the Trash's Restore does (needs the app). When something is at that path already, give to for another path, as Restore as… does. The id comes from list_trash. It's a change in Changes, with Revert (back to the Trash); a restore into Templates/, or of a note with code that runs, is held for the user to accept in the app.",
            schema(json!({
                "id": {"type": "string", "description": "The entry's id from list_trash."},
                "to": {"type": "string", "description": "The vault path to put it back under, instead of where it was."}
            }), &["id"])),
        tool("list_bookmarks", "Bookmarks", Read,
            "Lists the bookmarks as the sidebar shows them (needs the app): each one's title and the note it points at, or missing when that note is gone; one whose note hasn't changed in two weeks (and wasn't kept) says untouched N days, as Triage flags it. Bookmark, unbookmark or keep one with bookmarks; triage_bookmarks asks the model what to do with the ones given.",
            schema(json!({}), &[])),
        tool("bookmarks", "Bookmark a note", Destroy,
            "With page, bookmarks a note or takes its bookmark off (needs the app), as a note's Bookmark does (a note, .md or .txt, that's there); with remove, takes a bookmark off by its target, as Triage's Archive and Remove do (one whose note is gone too); with keep, keeps a bookmark as triage's Keep does, so it won't need triage for two weeks. Undoable with ⌘Z in the app. Only as the user asked; list_bookmarks lists them.",
            schema(json!({"page": {"type": "string", "description": "A note's path to bookmark, or unbookmark when it has one."}, "remove": {"type": "string", "description": "A bookmark's target to take off, from list_bookmarks."}, "keep": {"type": "string", "description": "A bookmark's target to keep: it won't need triage for two weeks."}}), &[])),
        tool("list_saved_searches", "Saved searches", Read,
            "Lists the saved searches, as Search's sidebar shows them (needs the app): each one's name, query and the layers it searches (all when none are given). Run one with search, its query and those layers, as clicking it does; save or delete one with saved_searches.",
            schema(json!({}), &[])),
        tool("saved_searches", "Save or delete a search", Destroy,
            "With name and query, saves a search as Search's Save search does (a name that's taken, or a query saved already, is refused; layers are notes, wiki and sources, as Search's filters); with name and delete, deletes one, as its x does (needs the app). Undoable with ⌘Z in the app. Only as the user asked; list_saved_searches lists them.",
            schema(json!({
                "name": {"type": "string", "description": "The saved search's name."},
                "query": {"type": "string", "description": "To save: the search, as search's query takes it."},
                "delete": {"type": "boolean", "description": "Delete the saved search called name."},
                "layers": layers
            }), &["name"])),
        // What agents changed.
        tool("list_changes", "What assistants changed", Read,
            "What assistants and Brainstead's runs changed, and the changes held for the user, as the Changes screen shows them (needs the app). list gives them as one list, the held ones first and then those made, newest first, a page at a time (limit, offset and query page through the whole list), grouped by run, each with its id and its run's group; show gives one change's diff, quotes and flags; history says how long Changes keeps its history. detail adds each change's kind, who made it and why. Act on them with changes.",
            schema(json!({
                "action": {"type": "string", "enum": ["list", "show", "history"], "description": "Default list."},
                "id": {"type": "string", "description": "For show: the change's id from list."},
                "page": {"type": "string", "description": "For list: only this page's changes, by its path."},
                "detail": detail
            }), &[])),
        tool("changes", "Accept, reject or revert changes", Destroy,
            "Decides on what assistants and runs changed (needs the app); list_changes lists them. accept makes a held change (on the page as it is now; a meeting note's follow-up, ingesting it and trashing its transcript, is then offered to the user in the app, as the screen's Accept does, and the reply says so) and reject turns it down; accept_run and reject_run do every held change in a run (its group), and with query only those of them the query finds, as Changes' Accept all and Reject all do with its search on; revert undoes a change that was made, also after later edits, and says when its lines have been edited since; revert_run reverts every change a run made, or with query those the query finds (Changes' Revert all); history with days or mb sets how long Changes keeps its history. accept and accept_run work only while the user is there (never with unattended) and only for a change held because a check failed: one held because it changes a template, adds code that runs, changes a system note's header, renames or trashes a template (or rewrites links in one), or comes from a job set to hold its changes is the user's to accept, in the app. Accept, reject, revert or change the history only as the user asked.",
            schema(json!({
                "action": {"type": "string", "enum": ["accept", "reject", "accept_run", "reject_run", "revert", "revert_run", "history"]},
                "id": {"type": "string", "description": "For accept, reject and revert: the change's id from list_changes."},
                "group": {"type": "string", "description": "For accept_run, reject_run and revert_run: the run's group from list_changes."},
                "query": {"type": "string", "description": "For accept_run, reject_run and revert_run: only the run's changes with every word of this in their title, page, the page it moved to, origin or model (Changes' search)."},
                "days": {"type": "integer", "enum": [30, 90, 180, 365], "description": "For history: keep this many days."},
                "mb": {"type": "integer", "enum": [100, 250, 500, 1000, 2000], "description": "For history: keep at most this many MB."}
            }), &["action"])),
        // Runs and the wiki.
        tool("start_run", "Start a run", Run,
            "Starts work Brainstead does with a model on the user's account, as its screens do (needs the app): ingest sources into the wiki (notes, PDFs, Office files or images; their changes are made and recorded in Changes), the daily check, the daily or weekly summary (daily_summary, weekly_summary; date picks the day or week; summary reads them), the weekly review's preparation (weekly_prep: suggestions for each step of the review's week, a paused review's own as on the screen, nothing changed until one is accepted; weekly_review lists them), a look through the last 90 days of notes for tasks and projects (find_tasks; list_suggestions lists what it found), a contradictions check, or a meeting note from a Teams transcript (list_transcripts; once made, the note is ingested and the transcript moved to the Trash, as the user's settings say; with ingest_existing, the note that's there already is kept as it is instead and followed through the same way, as the card's Ingest the existing note does). Write Current state has its own tool, write_current_state. It returns once started; run_status shows progress and stop_run stops it.",
            schema(json!({
                "run": {"type": "string", "enum": ["ingest", "daily_check", "daily_summary", "weekly_summary", "weekly_prep", "find_tasks", "contradictions", "meeting_note"], "description": "Which run."},
                "sources": {"type": "array", "items": {"type": "string"}, "description": "For ingest: vault paths to ingest, in sources/ or notes (pending_sources lists those not yet ingested): notes and text, PDFs, Office files and images; anything else is refused. All are checked before any is queued. Every ingest waits in one queue with the app's own, one at a time, oldest source first (by the date in its name, else the file's), so the order given and how many calls make no difference."},
                "transcript": {"type": "string", "description": "For meeting_note: the transcript's path."},
                "ingest_existing": {"type": "boolean", "description": "For meeting_note: no new note; the one there already (list_transcripts says when there is one; name, type and date pick another) is ingested and the transcript moved to the Trash, as the user's settings say."},
                "type": {"type": "string", "enum": ["Meeting", "1-1", "Workshop", "Interview"], "description": "For meeting_note: the note's type, as the screen offers them, if Brainstead guessed wrong."},
                "name": {"type": "string", "description": "For meeting_note: the meeting's name, or who the 1-1 was with."},
                "date": {"type": "string", "pattern": DATE, "description": "For meeting_note: the meeting's day, YYYY-MM-DD; needed when list_transcripts says the date is only the capture day (ask the user). For daily_summary: the day to summarise (default yesterday); for weekly_summary: a day in a week that has ended (default the week just ended, as Run now; this week is refused)."}
            }), &["run"])),
        tool("run_status", "How runs are going", Read,
            "How Brainstead's runs are going (needs the app): ingests running and lately done, the daily check, the daily and weekly summaries, the weekly review's preparation, Find tasks and projects, the contradictions check, meeting notes being written and Write Current state. Each says whether it's running, done or failed, with its progress; ingests and meeting notes give their ids, for stop_run, the meeting notes lately finished or failed are listed with the note made, and an ingest or meeting note says what the checks left out of it and why. Then the last eight summary runs, as Settings › Jobs & schedule's Recent runs lists them: kind, day, when, how it was started, its status and its note, and the id of its change in Changes (changes revert with that id undoes it, as Recent runs' Undo does). A summary run and the weekly review's last preparation give their chat with the model (Jobs' Open), which list_chats with action read and that chat reads. With query, the ingests listed are those the Sources screen's Ingest runs search finds (every word in the source or meeting note's name, its state, model or day), up to 20. Use it after start_run, rather than starting a run again; stop_run stops one.",
            schema(json!({"query": {"type": "string", "description": "Words to find ingest runs by, as the Ingest runs pane's search box: Steerco failed."}}), &[])),
        tool("stop_run", "Stop a run", Destroy,
            "Stops a run that's going, or an ingest or meeting note waiting its turn (needs the app), as its Stop button does. What it already changed stays, listed in Changes; the part not yet done is dropped, so start it again with start_run to finish. Stopping a run that isn't going does nothing. Only when the user asks.",
            schema(json!({"run": {"type": "string", "enum": ["ingest", "daily_check", "daily_summary", "weekly_summary", "weekly_prep", "find_tasks", "contradictions", "meeting_note", "write_current_state"], "description": "Which run, as run_status names it."}, "id": {"type": "string", "description": "For ingest or meeting_note: the run's id from run_status, running or waiting; else the one running, or when none is, the next waiting."}}), &["run"])),
        // Summaries and the weekly review.
        tool("summary", "A daily or weekly summary", Read,
            "The daily or weekly summary Brainstead wrote (what the user did that day or week, from their notes, tasks and log): the latest on file, or the one for day. For \"what did I do yesterday\", give kind daily and yesterday's date.",
            schema(json!({
                "kind": {"type": "string", "enum": ["daily", "weekly"], "description": "Default daily."},
                "day": {"type": "string", "description": "YYYY-MM-DD: that day's summary, or for weekly the summary of the week it's in (2026-W40 works too). The latest when left out."}
            }), &[])),
        tool("weekly_review", "The weekly review", Read,
            "The guided weekly review (needs the app): when it's scheduled and its suggestions are next prepared, the week it's for (on a Monday or Tuesday the week just ended, otherwise this one; a paused review keeps its week), its progress (steps done, the step it's on, notes and decisions so far), the path of the week's own review note once it's finished (Me. Weekly Review - YYYY-Www.md, what the user did and wrote; read it with read_section), and the prepared suggestions not yet accepted or skipped, grouped by step, each with its id, text, source and what accepting does. weekly_suggestion accepts or skips one; start_run weekly_prep prepares them again.",
            schema(json!({}), &[])),
        tool("weekly_suggestion", "Accept or skip a weekly review suggestion", Destroy,
            "Accepts a prepared weekly review suggestion, doing what the Weekly review's button does (a task added, ticked, deferred or reworded, or an Inbox item clarified, undoable with ⌘Z in the app; a link added to a note, made as a change recorded in Changes, where the user can revert it); or skips it. Either way it's hidden from the review for good and an accepted one goes in the review's log (needs the app). Only in a review that's started (weekly_step start), as the screen shows them in one. Accept or skip only what the user asked for.",
            schema(json!({
                "id": {"type": "string", "description": "The suggestion's id from weekly_review."},
                "action": {"type": "string", "enum": ["accept", "skip"]}
            }), &["id", "action"])),
        tool("weekly_start_over", "Start the weekly review over", Destroy,
            "Starts the weekly review over, as its Start over button does (needs the app): the paused review's progress, notes, decisions and handled suggestions are dropped, and it starts again from step 1 for this week. What it already changed in the vault stays, and there's no undo. Only when the user asks: refused once any call in the session passed unattended: true.",
            schema(json!({}), &[])),
        tool("weekly_step", "Do the weekly review", Change,
            "Goes through the guided weekly review as its screen does (needs the app); weekly_review says where it is. start starts this week's review, or carries on the paused one; done marks the step it's on done and goes on to the next (Next), or with step goes to that step (1 to 11); notes sets the review's notes (Your notes), or with log adds one line to its decisions; finish saves it as Finish and save does, in the week's review note (Me. Weekly Review - YYYY-Www.md, undoable with ⌘Z in the app), and ends it. Only as the user asked.",
            schema(json!({
                "action": {"type": "string", "enum": ["start", "done", "notes", "finish"]},
                "step": {"type": "integer", "minimum": 1, "maximum": 11, "description": "For done: go to this step instead of the next."},
                "text": {"type": "string", "description": "For notes: the notes, in full."},
                "log": {"type": "string", "description": "For notes: a line to add to the decisions instead."}
            }), &["action"])),
        tool("save_chat", "Save an Ask chat", Change,
            "Saves one of Brainstead's Ask chats to the vault as a note (needs the app): an open one as Ask's Save does, after which it stays up to date there as the chat goes on, or a closed one as History's Save does. Unsaved chats stay in Brainstead's app data, and closed ones that aren't saved or pinned are deleted once 20 newer ones are in History. Save only when the user asks.",
            schema(json!({
                "chat": {"type": "string", "description": "The chat's file or title, from list_chats. The chat open in Ask when left out."}
            }), &[])),
        tool("list_chats", "Ask's chats", Read,
            "Brainstead's Ask chats (needs the app): list gives them, newest first, with each one's file, whether it's saved in the vault and whether it's pinned; read gives one's messages. Rename, pin or trash one with chats; save one to the vault with save_chat.",
            schema(json!({
                "action": {"type": "string", "enum": ["list", "read"], "description": "Default list."},
                "chat": {"type": "string", "description": "For read: the chat's file or title, from list."}
            }), &[])),
        tool("chats", "Rename, pin or trash a chat", Destroy,
            "Changes one of Brainstead's Ask chats (needs the app): rename renames it (its tab and, when saved, its note); pin and unpin are History's Pin and Unpin (a pinned chat is kept whatever happens; closed chats that aren't saved or pinned are deleted once 20 newer ones are in History); trash moves it to the Trash, as History's Move to the Trash does. list_chats lists them. Only when the user asks.",
            schema(json!({
                "action": {"type": "string", "enum": ["rename", "pin", "unpin", "trash"]},
                "chat": {"type": "string", "description": "The chat's file or title, from list_chats."},
                "title": {"type": "string", "description": "For rename: the new title."}
            }), &["action", "chat"])),
        tool("list_transcripts", "Transcripts to write up", Read,
            "Teams meeting transcripts in Sources still without a meeting note (needs the app), or with show all every transcript, the done ones saying why (ingested, or linked from a note), as the screen's Show › All does. Each row has the transcript's path and what Brainstead guesses it is: its type, its name or who it was with, and its date, flagged when the date is only the day it was captured; as the screen marks them, the note it would make when that exists already, what to check with the user, and whether a note is being drafted or was drafted from it. Write one up with start_run meeting_note, giving the right date when it's flagged, and another name or date when its note exists.",
            schema(json!({"show": {"type": "string", "enum": ["to_do", "all"], "description": "The screen's Show: to_do (To do, the default) or all (All)."}}), &[])),
        tool("fix_health", "Fix Knowledge health issues", Change,
            "Applies Knowledge health's safe fixes (needs the app): missing cross-links, stale updated: dates, unlogged writes' log lines and system notes' lost headers; all of them, or the items named (their text as lint gives it). Undoable. Page shape items are reshape_pages'. Issues that need judgement aren't safe fixes: make those with edit_page.",
            schema(json!({"items": {"type": "array", "items": {"type": "string"}, "description": "Only these issues, by their text as lint gives it; every safe fix when left out."}}), &[])),
        tool("ignore_issue", "Ignore a Knowledge health issue", Change,
            "Knowledge health's Ignore (needs the app): stops listing an issue that has no fix (its text as lint gives it) until its page changes, or for a claim with no citation until its line changes; only when the user asked, and not for missing pages, duplicates, unused images, sources not yet ingested or page shape, which have buttons of their own (health_issue, start_run ingest, reshape_pages). With all and check (stale-pages or uncited-claims), Ignore all: every issue that check lists, as its Ignore all button does. With show_again, Show again: the ignored issues listed again, of one check (its id) or all. Ignored issues are left out of the app's count and of lint while the app is open.",
            schema(json!({
                "item": {"type": "string", "description": "The issue's text, as lint gives it."},
                "show_again": {"type": "boolean", "description": "List ignored issues again instead."},
                "all": {"type": "boolean", "description": "Ignore every issue of check (stale-pages or uncited-claims), as Ignore all does."},
                "check": {"type": "string", "description": "With all: stale-pages or uncited-claims. With show_again: the check's id (orphans, stale-pages…); every check when left out."}
            }), &[])),
        tool("health_issue", "A Knowledge health issue's button", Destroy,
            "Does what an issue's own button on Knowledge health does (needs the app), the issue named by its text as lint gives it. create (Create, on a missing page): writes the page as a new entity page, as Create does (a concept page is edit_page's, in wiki/concepts/); link_to (Link to…, on a missing page): points every link to the missing page at an existing page, given as to; not_duplicates (Not duplicates, on a possible duplicate): the pair isn't listed again; move_to_trash (Move to the Trash, on an image nothing uses; restore_from_trash puts it back); link_to also points a broken source (a page citing a file that isn't there) at a file that is; add_to_index (Add them to index.md, on Orphan pages, no item) brings index.md's catalogue up to date now, which links every wiki page. A missing page whose file is in the Trash has no create, as the screen has no Create for it: put it back with restore_from_trash (lint gives the id), or link_to another page; the same goes for a broken source. A page made is undone with ⌘Z in the app; each relinked page is a change in Changes, with Revert. Only as the user asked.",
            schema(json!({
                "action": {"type": "string", "enum": ["create", "link_to", "not_duplicates", "move_to_trash", "add_to_index"]},
                "item": {"type": "string", "description": "The issue's text, as lint gives it (not for add_to_index)."},
                "to": {"type": "string", "description": "For link_to: the page the links should point at, by name or path."}
            }), &["action"])),
        tool("list_contradictions", "Contradictions", Read,
            "The Contradictions screen's findings (needs the app): when the last check ran, and as the screen's banners say, why it failed or which fixes it couldn't make; then every finding, in the screen's order (real ones by severity, then unclear, then those settled), each with its id, the claims that clash (page, value, as-of date and quote), the model's verdict, and the judge's fix with its change in Changes (waiting there for the user, or made, with the change's id), as the screen's Judge's view and The fix is in Changes show them. Settle one or save the report with contradictions; start_run contradictions runs the check again.",
            schema(json!({}), &[])),
        tool("contradictions", "Settle a contradiction", Change,
            "Acts on the Contradictions screen (needs the app): mark settles a finding as its buttons do (resolved: Mark resolved; ignored: Ignore, not a real contradiction, so it isn't flagged again); save writes the report as a new note, Contradictions - YYYY-MM-DD, as Save report as note does, recorded in Changes. list_contradictions lists the findings. Mark only as the user asked.",
            schema(json!({
                "action": {"type": "string", "enum": ["mark", "save"]},
                "id": {"type": "string", "description": "For mark: the finding's id from list_contradictions."},
                "as": {"type": "string", "enum": ["resolved", "ignored"], "description": "For mark."}
            }), &["action"])),
        tool("page_shape", "Page shape", Read,
            "Knowledge health's Page shape check: which wiki entity and concept pages aren't in the page shape (opening text, Current state, topical sections, a Timeline of `### YYYY-MM-DD — title` entries newest first, each with a Source: [[…]] line, then See also), which of them Reshape pages can do by itself and why the rest need the user. With page, that page's report: the headings it would rewrite and the lines it would add.",
            schema(json!({"page": page}), &[])),
        tool("reshape_pages", "Reshape pages", Change,
            "Reshape pages, as Knowledge health's Page shape check does it (needs the app): every page that can be reshaped by itself, or the pages named (as proposed, even those that need the user; only when the user asked). Sections move whole and dated headings are rewritten; no text is lost. Each page is one change in Changes, in one run the user can revert alone or all together (changes revert_run).",
            schema(json!({"pages": {"type": "array", "items": page, "description": "Only these pages, by name or path."}}), &[])),
        tool("write_current_state", "Write Current state", Run,
            "Knowledge health's Write Current state (needs the app): for wiki pages with a Timeline but no Current state (lint lists them under Pages with no Current state), the cheap model writes one from each page's opening and newest Timeline entries only, checked (no links the page doesn't have) and made as a change in one Changes run. Runs in the background: limit does only that many first (a sample to look at), pages only those named; status says how the run is going; stop stops it after the page it's on.",
            schema(json!({
                "pages": {"type": "array", "items": page, "description": "Only these pages; every page without a Current state when left out."},
                "limit": {"type": "integer", "minimum": 1, "description": "Only this many pages first, as a sample to look at."},
                "status": {"type": "boolean", "description": "true only says how the run is going; nothing is started."},
                "stop": {"type": "boolean", "description": "true stops the run after the page it's on."}
            }), &[])),
        tool("fix_name", "Fix a name everywhere", Change,
            "Corrects a misspelt name across the notes and wiki, as Fix name does (needs the app); sources are left alone, and so are the files to leave alone; with ask_about_each_file, as Ask about each file, the notes to rewrite start unticked. Without apply it only lists the files the name is in, as the screen does, with what happens to each (Rewrite, Add as alias, Left alone, Guarded: someone else; a name in the file name is renamed separately) and how many will change; with apply true it changes the files (undoable), or with files only those, as ticking files in Fix name does. The correction is remembered for future captures, transcripts and ingests unless remember is false.",
            schema(json!({
                "written_as": {"type": "string", "description": "Written as: the name as it's misspelt, Lenna."},
                "correct_spelling": {"type": "string", "description": "Correct spelling: the name as it should be, Lena."},
                "apply": {"type": "boolean", "description": "true makes the change; left out, it only says what would change."},
                "files": {"type": "array", "items": {"type": "string"}, "description": "With apply: only these files, by their path as the preview lists them (the ticked files); every file the preview would change when left out."},
                "files_to_leave_alone": {"type": "array", "items": {"type": "string"}, "description": "Files to leave alone, when the misspelling is also a real name: part of each file's name (Lina Park); those files aren't changed."},
                "ask_about_each_file": {"type": "boolean", "description": "Ask about each file, as on the screen: every note to rewrite starts unticked, so with apply only those given in files (read each first) are rewritten, and the remembered correction is marked ambiguous, so an ingest never applies it on its own."},
                "remember": {"type": "boolean", "description": "Remember this correction: also correct it in future captures, transcripts and ingests. On unless false, as on the screen."},
                "where_its_from": {"type": "string", "description": "With remember: where it's from, kept with the correction (confirmed today via a 1-1)."}
            }), &["written_as", "correct_spelling"])),
        // The skill screens.
        tool("triage_bookmarks", "Triage bookmarks", Run,
            "Asks the model what to do with each bookmark given, as the Triage screen does (needs the app), in its choices: Keep, Ingest into the wiki (with the page to make), Make a task (with the task), Archive, or Remove for one whose note is gone; nothing is changed. list_bookmarks lists them, with how long since each changed. Act on a suggestion as the user decides, as its button does: Keep is bookmarks keep; Archive and Remove are bookmarks remove (by the bookmark's target); Ingest into the wiki is edit_page making the page, and Make a task is capture with kind task (to the Inbox), each then taking the bookmark off with bookmarks remove, as the screen's do.",
            schema(json!({"items": {"type": "array", "minItems": 1, "description": "The bookmarks to ask about, as list_bookmarks gives them.", "items": schema(json!({
                "target": {"type": "string", "description": "The bookmark's target, from list_bookmarks."},
                "path": {"type": "string", "description": "The note it points at, from list_bookmarks."}
            }), &["target", "path"])}}), &["items"])),
        tool("draft_reply", "Draft a reply", Run,
            "Drafts a reply to an email thread in sources/ (thread) or to pasted text, as the Draft reply screen does (needs the app). It says how fully the draft answers the thread. With current and change, it revises that reply as asked, as the screen's Ask for a change box does, keeping the rest of it. Nothing is sent and nothing in the vault changes: give the draft to the user.",
            schema(json!({
                "thread": {"type": "string", "description": "The email thread's path in sources/."},
                "text": {"type": "string", "description": "Instead of thread: the message to reply to, pasted."},
                "tone": {"type": "string", "enum": ["brief", "warm", "formal"], "description": "The screen's Tone: brief (as short as it can be while still answering, the default), warm (friendly and personal) or formal (courteous and complete, for someone senior or outside the team)."},
                "current": {"type": "string", "description": "To revise: the reply as it stands, with any edits the user made."},
                "change": {"type": "string", "description": "To revise: what to change, as the user asked (shorter, mention the date…)."}
            }), &[])),
        tool("doc_check", "Check a document", Run,
            "Checks a document, as Doc check does (needs the app): against the version in force of a governing document in the canonical docs register, or against the feedback the user gave on its last version. The findings come in the screen's words, as Copy findings gives them (Conflict, Not covered, Superseded term, Beyond scope, Agrees; the version In force and those not used). Without document it gives the register, each version's key and status (In force, Draft, Superseded), so you can pick governing_document from it; with start_register and no register yet, it's Start the register (Me. Canonical Docs.md, with its empty table). save is Save as note: the findings as a new note, Doc check. <name> - YYYY-MM-DD. Those two are recorded in Changes, where the user can revert them; otherwise nothing in the vault changes. Against the version in force, it first looks at the two documents' openings and stops there, checking nothing and saving nothing, when they aren't about the same thing. A finished check is kept, as the screen keeps it: while neither document has changed it's given again (saying when it was made) rather than run again, unless fresh.",
            schema(json!({"document": {"type": "string", "description": "Document to check, by path."}, "save": {"type": "boolean", "description": "With document: Save as note, the findings saved as a new note once the check is done."}, "fresh": {"type": "boolean", "description": "Check again even when a kept result still stands (neither document changed since)."}, "start_register": {"type": "boolean", "description": "Without document: Start the register, when there's none yet."}, "governing_document": {"type": "string", "description": "For version_in_force: the governing document, by its key in the register; the register's first when left out, as the screen's picker starts with."}, "mode": {"type": "string", "enum": ["version_in_force", "earlier_feedback"], "description": "The screen's modes: version_in_force (Against the version in force, the default) lists where the document differs from the governing document; earlier_feedback (Against my earlier feedback) checks each point of the user's feedback on the last version against this one."}}), &[])),
        tool("compare_docs", "Compare two documents", Run,
            "Compares any two documents, A and B, neither taking precedence, as Compare's Any two documents tab does (needs the app); for a document against the version in force, use doc_check. look_for differences (the default) lists where they differ, what's only in A or only in B, and what agrees; what_changed lists what was added, removed and changed from B (the earlier version) to A (the newer). It first looks at their openings and stops there, comparing nothing, when they aren't about the same thing. The findings come in the screen's words, as Copy findings gives them. A finished comparison is kept: while neither document has changed it's given again rather than run again, unless fresh. save is Save as note (Compare. A vs B - YYYY-MM-DD; nothing is saved when they aren't about the same thing), recorded in Changes, where the user can revert it; otherwise nothing in the vault changes.",
            schema(json!({
                "document_a": {"type": "string", "description": "Document A, by path (the newer version for what_changed)."},
                "document_b": {"type": "string", "description": "Document B, by path (the earlier version for what_changed)."},
                "look_for": {"type": "string", "enum": ["differences", "what_changed"], "description": "differences (the default) or what_changed."},
                "fresh": {"type": "boolean", "description": "Compare again even when a kept result still stands."},
                "save": {"type": "boolean", "description": "Save as note, once it's compared."}
            }), &["document_a", "document_b"])),
        // Around the vault.
        tool("activity", "Recent activity", Read,
            "log.md, newest first: what Brainstead and the assistants changed in the vault (needs the app), a line an entry with its date, action and title. action keeps one kind of entry, as the Activity screen's action chips do; query keeps the entries with all its words in their title, description or action's label, as its search does, best matches first or with order latest the newest first. With date, the day picked as on the screen: the files changed that day, then that day's entries (action and query narrow the entries, not the files). detail adds each entry's time and description.",
            schema(json!({"date": day("the files changed that day and its entries."), "action": {"type": "string", "description": "Only entries of this action, as the log names it or its chip on the Activity screen does: ingest, review, trash…"}, "order": {"type": "string", "enum": ["best_match", "latest"], "description": "With query, the screen's order of search results: best_match (Best match, the default) or latest (Latest). Newest first without query."}, "detail": detail}), &[])),
        tool("graph", "Links around a page", Read,
            "The pages linked to and from a page (by name, [[link]] or path), to a depth of 1 to 3 (needs the app), as the Graph screen draws it; the whole wiki without page. Like the screen it keeps the nearest 400 pages, and says so when there were more. It lists the links between them, one `from → to` a line. hide leaves out kinds of page, as the screen's chips do (the page itself stays). detail lists the pages too, each with its type, how far it is from the page and how many links it has.",
            schema(json!({"page": page, "depth": {"type": "integer", "minimum": 1, "maximum": 3, "description": "How many links away to go. Default 2, as the Graph screen starts."}, "hide": {"type": "array", "items": {"type": "string", "enum": ["Entities", "Sources and summaries", "Concepts", "Other wiki", "Notes", "Not written yet"]}, "description": "The kind chips turned off: those pages and their links are left out."}, "detail": detail}), &[])),
        tool("list_suggestions", "Suggested tasks and projects", Read,
            "Find tasks and projects' suggestions (needs the app): tasks and projects a read of the user's notes from the last 90 days found, each with the note and quote it rests on and its id, and how the last look went. Accept or skip one with suggestions; start_run find_tasks looks again.",
            schema(json!({}), &[])),
        tool("suggestions", "Accept or skip a suggestion", Destroy,
            "Acts on a Find tasks and projects suggestion (needs the app). accept makes it (a task added to its project's Next actions or the To Do list, a project note with its first next actions) as a change recorded in Changes, where the user can revert it (⌘Z in the app undoes it too, as Found's Undo does), with text to reword it (or a project's name) and done_looks_like for a project's; skip leaves it out. Neither comes back. Accept or skip only what the user asked for.",
            schema(json!({
                "action": {"type": "string", "enum": ["accept", "skip"]},
                "id": {"type": "string", "description": "The suggestion's id from list_suggestions."},
                "text": {"type": "string", "description": "For accept: the task's wording, or the project's name, if it should differ."},
                "done_looks_like": {"type": "string", "description": "For accept on a project: what done looks like, if it should differ."}
            }), &["action", "id"])),
        tool("list_moving_over", "Moving over's checklist", Read,
            "Settings › General › Moving over's checklist (needs the app), as the screen lists it while the previous app's own files are in the vault: each item with its id, whether it's done, whether Brainstead checks it itself or the user ticks it, and what it says. moving_over ticks the user's items and retires the previous app's skills.",
            schema(json!({}), &[])),
        tool("moving_over", "Moving over from another app", Destroy,
            "Settings › General › Moving over (needs the app). tick and untick tick an item the user ticks by hand (otherSummaries, otherExtensions, otherStopped, as list_moving_over gives them), only once the user says it's done. retire is Retire them: the previous app's skills (.claude/skills/) and scripts (scripts/) go to Brainstead's Trash (restorable from there), the name corrections are kept, and CLAUDE.md tells agents to use these tools; refused, as on the screen, until The other app is stopped is ticked. It says what it removed. Only when the user asks: refused once any call in the session passed unattended: true.",
            schema(json!({
                "action": {"type": "string", "enum": ["tick", "untick", "retire"]},
                "item": {"type": "string", "description": "For tick and untick: the item's id from list_moving_over (otherStopped)."}
            }), &["action"])),
        tool("list_automated_tools", "Tools that start sessions", Read,
            "The user's own tools that start Claude Code sessions (Settings › Jobs & schedule), whose sessions the daily summary counts as automated instead of as the user's work (needs the app). It lists them, then as suggestions the folders whose recent sessions were nearly all one prompt and done. Add or remove one with automated_tools.",
            schema(json!({}), &[])),
        tool("automated_tools", "Add or remove a tool that starts sessions", Destroy,
            "Changes the user's list of tools that start Claude Code sessions (Settings › Jobs & schedule; needs the app): add lists one (a name, and folder_contains and/or first_message_starts_with, the screen's Folder contains and First message starts with), replacing one of the same name; remove takes one off by name. list_automated_tools lists them. Only when the user asks.",
            schema(json!({
                "action": {"type": "string", "enum": ["add", "remove"]},
                "name": {"type": "string", "description": "The tool's name."},
                "folder_contains": {"type": "string", "description": "For add: Folder contains, its sessions run in a folder whose path contains this."},
                "first_message_starts_with": {"type": "string", "description": "For add: First message starts with, the words its sessions' first message starts with (case doesn't matter)."}
            }), &["action", "name"])),
        tool("list_task_lists", "Saved task lists", Read,
            "Tasks' saved lists (needs the app): each one's name, view, context, effort and grouping. list_tasks with list shows one's tasks; save or remove one with task_lists.",
            schema(json!({}), &[])),
        tool("task_lists", "Save or remove a task list", Destroy,
            "Saves or removes one of Tasks' saved lists: a view with its context, effort and grouping, by name (needs the app). save saves one, as Save this list… does (replacing one of the same name); remove takes one off. list_task_lists lists them. Only when the user asks.",
            schema(json!({
                "action": {"type": "string", "enum": ["save", "remove"]},
                "name": {"type": "string", "description": "The list's name."},
                "view": {"type": "string", "enum": ["next", "followups", "waiting", "deferred", "someday", "done-this-week", "done-last-week"], "description": "For save: one of the Tasks screen's lists, as list_tasks names it (today and all can't be saved). Default next."},
                "context": {"type": "string", "description": "For save: a context such as @calls; none or left out for any context."},
                "effort": {"type": "integer", "enum": [15, 30, 60], "description": "For save: only tasks of this many minutes or less; any effort when left out."},
                "group": {"type": "string", "enum": ["none", "project", "context", "due"], "description": "For save: default none."}
            }), &["action", "name"])),
        tool("list_settings", "Brainstead's settings", Read,
            "Brainstead's settings that settings can change, as the Settings screen shows them (needs the app): each one's key, pane, label and value, and for a choice its choices; a note's Text size, set from a note's top bar, is among them. The vault, Read-only and the folders left out aren't listed: they're the user's to change, in the app.",
            schema(json!({}), &[])),
        tool("settings", "Change a setting", Change,
            "Changes one of Brainstead's settings by its key from list_settings, as the Settings screen does (needs the app). The value is checked against the setting's kind: true or false, a time HH:MM, a day (mon…sun), a number from its choices, one of the choices list_settings gives by id or name (a model, a voice, a spelling language such as British English, a document theme or colour), a note's Text size (12 to 24), the quick capture shortcut (Control, Alt, Shift or Super, then a key: Control+Alt+Space; it says whether the system registered it), or text; null puts a setting with a default choice back to it (As new chats, As macOS, System voice, As the app). Change only what the user asked for; ⌘Z doesn't undo it.",
            schema(json!({
                "key": {"type": "string", "description": "The setting's key from list_settings."},
                "value": {"type": ["boolean", "number", "string", "null"], "description": "The new value, of the kind list_settings shows: true or false, a number, a time HH:MM, a day, one of its choices, or text; null for its default choice."}
            }), &["key", "value"])),
        tool("list_note_looks", "Notes' own looks", Read,
            "A note's own look, as its Look button shows it (needs the app): with page, that note's theme and colour and whether they're its own or the defaults; without, the notes with a look of their own, as Settings › Notes › Notes with their own look counts them, and the defaults. A note's own look is kept in Brainstead's settings by the note's path, never in the note. note_look changes it.",
            schema(json!({"page": page}), &[])),
        tool("note_look", "Change a note's look", Change,
            "Changes a note's own look, as the note's Look button does (needs the app), in Brainstead's settings, not the note: set gives the note at page its own theme or colour (or both; a choice that matches the default isn't kept), use_defaults is its Use the defaults, and use_defaults_for_all is Settings › Notes' Use the defaults for all, every note's own look dropped. ⌘Z doesn't undo it. list_note_looks reads them; the defaults themselves are settings docStyle and docAccent. Only when the user asks.",
            schema(json!({
                "action": {"type": "string", "enum": ["set", "use_defaults", "use_defaults_for_all"]},
                "page": page,
                "theme": {"type": "string", "description": "For set: the Look menu's Theme, by id or name: brainstead (Brainstead), business, editorial, modern, minimal, report or technical."},
                "colour": {"type": "string", "description": "For set: the Look menu's Colour, by id or name: brainstead (Brainstead blue), blue (Dark blue), lightblue (Light blue), green, orange, yellow, red, gray or darkgray (Dark gray)."}
            }), &["action"])),
        tool("import_sources", "Add files to Sources", Change,
            "Copies files from the Mac into sources/, as Sources' Import does (needs the app): PDFs, Office files, images, notes and the rest, up to 200 MB each. New sources are then ingested as they arrive when the user has that on (Settings › AI assistants). Only files the user named.",
            schema(json!({"paths": {"type": "array", "items": {"type": "string"}, "description": "Absolute paths of the files to add."}}), &["paths"])),
        tool("create_template", "New template", Change,
            "Makes a new template in Templates/, as Templates' New template does (needs the app). A template is always held for the user to accept in Changes, since templates can run code. To change one that exists, use edit_page (held too).",
            schema(json!({
                "name": {"type": "string", "description": "The template's name, without .md."},
                "content": {"type": "string", "description": "The whole template: properties, headings and Templater tags as the vault's other templates have them."},
                "reason": reason
            }), &["name", "content"])),
        tool("open", "Open Brainstead", Read,
            "Opens Brainstead (starting it when it isn't running) and brings its window to the front, on a screen, a Settings pane, a note or a search when given; it changes nothing. meeting (Meeting note from a transcript) opens on transcript when given, reply (Draft a reply) on thread, doc_check (Check a document) on document, ready for the user to go on there; edit opens the page in Edit; graph with page opens Graph around that page; focus opens the page (or the one showing) in focus mode, as ⌘. does; image shows an image full size over the window, as a click on one in a note does. Only when the user asks to see something.",
            schema(json!({
                "screen": {"type": "string", "enum": ["today", "inbox", "tasks", "projects", "weekly_review", "changes", "ask", "search", "notes", "wiki", "sources", "templates", "graph", "knowledge_health", "contradictions", "activity", "triage", "meeting", "reply", "doc_check", "compare", "trash", "settings"], "description": "The screen, as the sidebar or the command palette names it; the one showing when left out."},
                "pane": {"type": "string", "enum": ["general", "notes", "vault", "assistants", "jobs", "capture", "permissions", "about"], "description": "With screen settings: the pane."},
                "page": page,
                "lines": {"type": "array", "items": {"type": "string"}, "description": "With page: lines of it to highlight, scrolled to the first, each as its text (as lint gives a claim with no citation)."},
                "query": {"type": "string", "description": "With screen search: what to search for."},
                "transcript": {"type": "string", "description": "With screen meeting: the transcript's path (list_transcripts)."},
                "thread": {"type": "string", "description": "With screen reply: the email thread's path in sources/, as draft_reply takes it."},
                "document": {"type": "string", "description": "With screen doc_check: the document to check, by path, as doc_check takes it."},
                "edit": {"type": "boolean", "description": "With page: opens it in Edit rather than View, as a project's Edit button does (a source, or any page while the vault is read-only, opens in View)."},
                "focus": {"type": "boolean", "description": "With page, or a note, wiki page or source showing: focus mode (⌘.), just the text across the window with Read aloud, Text size and Exit focus above it; false leaves it."},
                "image": {"type": "string", "description": "An image in the vault, by its name (diagram.png) or path (images/diagram.png): shown full size over the window, as a click on it in a note shows it. Escape closes it. Not with the others."}
            }), &[])),
        tool("add_example_notes", "Add the example notes", Change,
            "Settings › Vault's Add the example notes (needs the app): the notes a new vault is made with, which show what Brainstead can do, each added as a new note recorded in Changes, where the user can revert it; only those that aren't in the vault, and refused when they're all there already, as the button says. Nothing already there is changed. The example templates are held for the user to accept, as every change to Templates/ is. Only when the user asks.",
            schema(json!({"reason": reason}), &[])),
        tool("capture_extensions", "Capture extensions", Read,
            "Settings › Capture extensions, as the pane shows it (needs the app): the folder the Outlook and Teams extensions are loaded from, the browsers and whether each knows Brainstead's capture host, and the last captures, each with what it was, the file it made in sources/ or why it was refused.",
            schema(json!({}), &[])),
        tool("glance", "The vault at a glance", Read,
            "Glance's counts, as Activity and Knowledge health show them (needs the app): notes and wiki pages by type and by tag, most first, and the files the most others link to.",
            schema(json!({}), &[])),
        tool("app_status", "Brainstead's state", Read,
            "The vault, whether it's read-only (then nothing can be changed until the user switches it off in Settings › Vault), the index as Settings › Vault shows it (its counts, or why it failed), Full Disk Access, Brainstead's version, its app data folder and its website, whether Brainstead runs the daily and weekly summaries, whether the previous app's skills are still in the vault (moving_over retires them), and what ⌘Z would undo (needs the app). Then features: which of Quick Look, spelling, grammar, Read aloud, Export PDF, rich copy, Full Disk Access, Open at login and the text in images this system has; the window has no button or setting for the rest, so don't offer them.",
            schema(json!({}), &[])),
        tool("list_assistants", "AI assistants found", Read,
            "Settings › AI assistants' Found on this computer, as the pane lists it (needs the app): Claude Code, Codex, Antigravity and Copilot, each with its version, how many models it offers and where it is, or that it isn't installed (with where to read how to install it) or lists no models (then the user runs it in Terminal and signs in). look_again looks for them again, as the pane's Look again does, after one is installed or signed in. Then the pane's other groups: Brainstead's tools' command for each assistant found, to use them from Terminal, and Skills Brainstead does now (the vault's skills whose screen Brainstead has).",
            schema(json!({"look_again": {"type": "boolean", "description": "true looks for the assistants again first, as Look again does."}}), &[])),
        tool("rebuild_index", "Rebuild the index", Run,
            "Reads every file in the vault again and rebuilds Brainstead's index from scratch, as Settings › Vault's Rebuild index does (needs the app); the files aren't changed. It returns once started; app_status says how the index stands. For when search, tasks or links seem out of date with the files; only when the user asks.",
            schema(json!({}), &[])),
    ];
    let outputs = outputs();
    for t in &mut all {
        let name = t["name"].as_str().unwrap_or("").to_string();
        if PAGED.contains(&name.as_str()) {
            paging(&mut t["inputSchema"]["properties"]);
        }
        if IDEMPOTENT.contains(&name.as_str()) {
            t["annotations"]["idempotentHint"] = json!(true);
        }
        if let Some(o) = outputs.get(name.as_str()) {
            t["outputSchema"] = o.clone();
        }
    }
    all
}

/// A day, and a day or a month, as the date arguments take them.
const DATE: &str = r"^\d{4}-\d{2}-\d{2}$";
const DAY_OR_MONTH: &str = r"^\d{4}-\d{2}(-\d{2})?$";

/// The tools that list something: each gives how many there are and a page of them at a time.
const PAGED: [&str; 20] = [
    "list_tasks",
    "pending_sources",
    "backlinks",
    "list_inbox",
    "list_projects",
    "list_trash",
    "list_bookmarks",
    "list_saved_searches",
    "list_changes",
    "list_chats",
    "list_transcripts",
    "list_contradictions",
    "page_shape",
    "activity",
    "graph",
    "list_suggestions",
    "list_automated_tools",
    "list_task_lists",
    "list_settings",
    "list_note_looks",
];

/// Tools that change something but do the same the second time: setting a value, stopping a run.
const IDEMPOTENT: [&str; 5] = ["settings", "stop_run", "update_project", "task_lists", "move_project"];

/// A page of rows as data: how many there are, how many match `query`, where this page starts and
/// the next one, and the rows (src/mcpActions.ts's listing gives the same).
fn page_schema(item: Value) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total": {"type": "integer", "description": "How many there are."},
            "matching": {"type": "integer", "description": "How many have query's words."},
            "offset": {"type": "integer"},
            "next_offset": {"type": ["integer", "null"], "description": "The offset for the next page; null on the last."},
            "items": {"type": "array", "items": {"type": "object", "properties": item}}
        },
        "required": ["total", "items"]
    })
}

/// The listing tools' structuredContent, by tool. A tool with a schema here gives its rows as data
/// as well as text, every time it succeeds.
fn outputs() -> std::collections::HashMap<&'static str, Value> {
    let s = |t: &str| json!({"type": t});
    let sn = || json!({"type": ["string", "null"]});
    let change = json!({"id": s("string"), "title": s("string"), "page": s("string"), "kind": s("string"), "status": s("string"), "flags": {"type": "array", "items": s("string")}, "origin": s("string"), "group": s("string"), "reason": s("string"), "created": s("string")});
    let mut changes = json!({"type": "object", "properties": {
        "held": page_schema(change.clone()),
        "made": page_schema(change),
        "change": {"type": "object", "description": "show: the change, with its diff and quotes."},
        "history": {"type": "object", "properties": {"days": s("integer"), "mb": s("integer")}}
    }});
    changes["description"] = json!("list gives held and made: one page of the list (held first, then made), split in two, each with its own total and matching and the same offset and next_offset; show gives change; history gives history.");
    let mut chats = page_schema(
        json!({"file": s("string"), "title": s("string"), "updated": s("string"), "saved": s("boolean"), "pinned": s("boolean")}),
    );
    chats["properties"]["chat"] = json!({"type": "object", "description": "read: the chat's title and messages."});
    chats["required"] = json!([]);
    std::collections::HashMap::from([
        (
            "search",
            page_schema(
                json!({"path": s("string"), "title": s("string"), "layer": s("string"), "date": s("string"), "heading": s("string"), "snippet": s("string")}),
            ),
        ),
        (
            "list_tasks",
            page_schema(
                json!({"id": s("string"), "text": s("string"), "path": s("string"), "due": sn(), "defer": {"type": ["string", "null"], "description": "Deferred until, as edit_task's defer sets it."}, "start": sn(), "done": s("boolean"), "doneOn": sn(), "project": sn(), "contexts": {"type": "array", "items": s("string")}, "tags": {"type": "array", "items": s("string"), "description": "Its #tags other than contexts, without the #."}, "priority": {"type": ["string", "null"], "enum": ["highest", "high", "medium", "low", "lowest", null], "description": "As edit_task's priority; null for none."}, "status": {"type": "string", "enum": ["open", "in_progress", "done", "cancelled"]}, "created": sn(), "effort": sn(), "heading": sn(), "group": {"type": ["string", "null"], "description": "With group: the heading it's under, as the Tasks screen shows it."}, "band": {"type": ["string", "null"], "description": "With view today and no group: the Today screen's band it's in (Overdue, Due today, Deferred until now, Waiting for)."}}),
            ),
        ),
        (
            "list_inbox",
            page_schema(
                json!({"id": s("string"), "kind": s("string"), "text": s("string"), "path": s("string"), "stamp": sn(), "suggestion": {"type": ["object", "null"], "description": "With suggest: what the model suggests it becomes, with text, project, context, effort, due and why."}}),
            ),
        ),
        (
            "list_projects",
            page_schema(
                json!({"name": s("string"), "path": s("string"), "status": s("string"), "area": sn(), "done_looks_like": sn(), "flag": {"type": ["string", "null"], "enum": ["stuck", "quiet", null], "description": "The Projects screen's flag on an active project: stuck (Stuck, no next action) or quiet (Quiet, nothing has happened for two weeks)."}, "next": s("integer"), "waiting": s("integer"), "someday": s("integer"), "done": s("integer")}),
            ),
        ),
        ("list_trash", {
            let mut t = page_schema(
                json!({"id": s("string"), "path": s("string"), "layer": s("string"), "deleted": s("string"), "bytes": s("integer")}),
            );
            t["properties"]["bytes"] = json!({"type": "integer", "description": "Everything in the Trash, in bytes."});
            t["properties"]["size"] = json!({"type": "string", "description": "The same, as the Trash screen says it."});
            t
        }),
        ("list_changes", changes),
        (
            "activity",
            page_schema(
                json!({"date": s("string"), "time": sn(), "action": s("string"), "title": s("string"), "description": s("string"), "path": s("string")}),
            ),
        ),
        ("list_chats", chats),
        (
            "list_bookmarks",
            page_schema(
                json!({"title": s("string"), "target": s("string"), "path": sn(), "days": {"type": ["integer", "null"], "description": "Days since its note changed."}, "stale": s("boolean")}),
            ),
        ),
        (
            "app_status",
            json!({"type": "object", "properties": {"features": {"type": "object", "description": "What this system has (platform: macos, windows or linux); false is hidden in the window.", "properties": {
                "platform": s("string"), "quickLook": s("boolean"), "spelling": s("boolean"), "grammar": s("boolean"), "readAloud": s("boolean"), "exportPdf": s("boolean"), "richCopy": s("boolean"), "fullDiskAccess": s("boolean"), "openAtLogin": s("boolean"), "imageText": s("boolean")
            }}}}),
        ),
        (
            "list_transcripts",
            page_schema(
                json!({"path": s("string"), "type": sn(), "name": sn(), "date": sn(), "dateCheck": s("boolean"), "done": {"type": ["string", "null"], "description": "With show all: why it's done, ingested or linked (from a note); null while it's to do."}, "exists": {"type": ["string", "null"], "description": "The note it would make, when that note exists already (start_run meeting_note refuses it unless the name or date is changed)."}, "ask": {"type": "array", "items": s("string"), "description": "What the screen asks the user to check about it."}, "drafted": {"type": ["string", "null"], "enum": ["drafting", "drafted", null], "description": "drafting while a note is being drafted from it, drafted once one was (see Changes)."}}),
            ),
        ),
    ])
}

/// A listing tool's paging: limit, offset and query, as src/mcpActions.ts's pagedRows reads them.
fn paging(props: &mut Value) {
    let Some(p) = props.as_object_mut() else { return };
    p.insert("limit".into(), json!({"type": "integer", "minimum": 1, "maximum": 500, "description": "For a list: how many to give (each tool has its own default, said in its reply)."}));
    p.insert("offset".into(), json!({"type": "integer", "minimum": 0, "description": "For a list: how many to skip, for the next page; the reply says which offset comes next."}));
    p.insert("query".into(), json!({"type": "string", "description": "For a list: only the rows with all of these words."}));
}

/// Rows a page at a time, as the app's listing tools give them (src/mcpActions.ts's pagedRows).
fn page_of<'a, T>(rows: &'a [T], text: impl Fn(&T) -> String, a: &Paging, noun: (&str, &str), def: usize) -> (Vec<&'a T>, String) {
    let words: Vec<String> = a.query.as_deref().unwrap_or("").to_lowercase().split_whitespace().map(String::from).collect();
    let hit: Vec<&T> = rows
        .iter()
        .filter(|r| {
            let t = text(r).to_lowercase();
            words.iter().all(|w| t.contains(w))
        })
        .collect();
    let limit = a.limit.unwrap_or(def).clamp(1, 500);
    let offset = a.offset.unwrap_or(0);
    let shown: Vec<&T> = hit.iter().skip(offset).take(limit).copied().collect();
    let n = |k: usize| format!("{k} {}", if k == 1 { noun.0 } else { noun.1 });
    let mut head =
        if words.is_empty() { n(rows.len()) } else { format!("{} matching “{}”, of {}", n(hit.len()), words.join(" "), rows.len()) };
    if offset >= hit.len() && !hit.is_empty() {
        head.push_str(&format!("; none from offset {offset}"));
    } else if shown.len() < hit.len() {
        let end = offset + shown.len();
        head.push_str(&format!("; {}–{end} shown", offset + 1));
        if end < hit.len() {
            head.push_str(&format!(", offset {end} for the next"));
        }
    }
    (shown, head)
}

/// limit, offset and query, as every listing tool takes them.
#[derive(Deserialize, Default)]
struct Paging {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    query: Option<String>,
}

/// Why a call failed: a tool there's no such thing as (a JSON-RPC error, as MCP asks), or anything
/// else, arguments that don't fit included (a result marked as an error, which the model reads and
/// can correct its call from).
enum CallError {
    Unknown(String),
    Tool(String),
}

/// What a tool gives back: its text, and for a listing tool with an output schema, the same rows
/// as data (MCP's structuredContent).
struct Out {
    text: String,
    structured: Option<Value>,
}

impl From<String> for Out {
    fn from(text: String) -> Self {
        Out { text, structured: None }
    }
}

impl From<String> for CallError {
    fn from(e: String) -> Self {
        CallError::Tool(e)
    }
}

/// Calls in the last minute, to keep a runaway loop from hammering the vault (MCP asks servers to
/// rate-limit tool calls).
const PER_MINUTE: usize = 120;

fn call(ctx: &Ctx, name: &str, args: Value) -> Result<Out, CallError> {
    // Arguments that don't fit are the tool's error, so the model reads why and can call again.
    fn parse<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, CallError> {
        serde_json::from_value(v)
            .map_err(|e| CallError::Tool(format!("Those arguments don't fit: {e}. The tool's inputSchema says what it takes.")))
    }
    if !tools().iter().any(|t| t["name"] == name) {
        return Err(CallError::Unknown(format!("Unknown tool: {name}")));
    }
    {
        let mut calls = ctx.calls.lock().unwrap();
        let now = std::time::Instant::now();
        calls.retain(|t| now.duration_since(*t) < std::time::Duration::from_secs(60));
        if calls.len() >= PER_MINUTE {
            return Err(CallError::Tool(format!(
                "Over {PER_MINUTE} calls in a minute: wait a moment, and do more in each call where you can."
            )));
        }
        calls.push(now);
    }
    let mut obj = if args.is_null() { json!({}) } else { args };
    if !obj.is_object() {
        return Err(CallError::Tool("Arguments are an object of named values, as the tool's inputSchema says.".into()));
    }
    let unattended = obj.get("unattended").and_then(Value::as_bool) == Some(true);
    if unattended {
        ctx.unattended.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // Once a call has said nobody's watching, the whole session is (D-20261005-09).
    let unwatched = ctx.unattended.load(std::sync::atomic::Ordering::Relaxed);
    let act = |action: &str, a: Value| -> Result<Out, CallError> { Ok(act(ctx, action, a)?) };
    // The app records these in Changes as this session's (D-20261005-10).
    if [
        "edit_task",
        "move_task",
        "clarify_inbox",
        "capture",
        "create_project",
        "update_project",
        "rename_area",
        "doc_check",
        "compare_docs",
        "restore_from_trash",
        "contradictions",
    ]
    .contains(&name)
    {
        obj["origin"] = serde_json::to_value(origin(ctx, unattended)).unwrap_or_default();
    }
    match name {
        "search" => Ok(search(ctx, parse(obj)?)?),
        "read_section" => Ok(read_section(ctx, parse(obj)?)?.into()),
        "backlinks" => Ok(backlinks(ctx, parse(obj)?)?.into()),
        "resolve_entity" => Ok(resolve_entity(ctx, parse(obj)?)?.into()),
        "facts" => Ok(facts(ctx, parse(obj)?)?.into()),
        "pending_sources" => Ok(pending_sources(ctx, parse(obj)?)?.into()),
        "provenance" => act("source.provenance", obj),
        // The app's report when it's open, as the screen shows it; else the core's checks.
        "lint" if bridge::app_alive(&ctx.data) => act("health.lint", obj),
        "lint" => Ok(lint_tool(ctx, parse(obj)?)?.into()),
        "help" => Ok(help(parse(obj)?)?.into()),
        "edit_page" => Ok(edit_page(ctx, parse(obj)?)?.into()),
        "create_task" => Ok(create_task(ctx, parse(obj)?)?.into()),
        "delete_task" => Ok(delete_task(ctx, parse(obj)?)?.into()),
        "create_note" => {
            let a: NoteArgs = parse(obj.clone())?;
            if a.template.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                // The same checks as a plain note's, before the template runs.
                new_note_path(ctx, &a)?;
                let mut obj = obj;
                obj["origin"] = serde_json::to_value(origin(ctx, a.unattended)).unwrap_or_default();
                act("note.from_template", obj)
            } else {
                Ok(create_note(ctx, a)?.into())
            }
        }
        "add_image" => Ok(add_image(ctx, parse(obj)?)?.into()),
        "rename_note" => Ok(rename_note(ctx, parse(obj)?)?.into()),
        "trash_note" => {
            let a: TrashArgs = parse(obj.clone())?;
            let rel = existing(ctx, &ctx.index()?, &a.page)?;
            // A Teams transcript: in the app, its links pointed at its meeting note first, as every
            // way of trashing one does (src/meetingFlow.ts's trashTranscript).
            if is_transcript_page(&rel) {
                let mut obj = obj;
                obj["page"] = json!(rel);
                obj["origin"] = serde_json::to_value(origin(ctx, a.unattended)).unwrap_or_default();
                act("note.trash_transcript", obj)
            } else {
                Ok(trash_note(ctx, a)?.into())
            }
        }
        "list_tasks" => act("tasks.list", obj),
        "edit_task" => act("task.edit", obj),
        "move_task" => act("task.move", obj),
        "list_inbox" => act("inbox.list", obj),
        "clarify_inbox" => act("inbox.clarify", obj),
        "capture" => act("inbox.capture", obj),
        "list_projects" => act("projects.list", obj),
        "create_project" => act("project.create", obj),
        "update_project" => act("project.update", obj),
        "rename_area" => act("project.rename_area", obj),
        "move_project" => act("project.move", obj),
        // The read tools and the change tools beside them share the app's action, each held to its
        // own actions, so a read tool never changes anything.
        "list_trash" => act("trash", acting(obj, name, &["list"], "list", &[])?),
        "restore_from_trash" => act("trash", acting(obj, name, &["restore"], "restore", &[])?),
        "list_bookmarks" => act("bookmarks", without(obj, &["page", "keep", "remove"])),
        "bookmarks" => {
            let given = |k: &str| obj.get(k).and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty());
            if !given("page") && !given("keep") && !given("remove") {
                return Err(CallError::Tool(
                    "Give page (to bookmark or unbookmark a note), keep or remove. list_bookmarks lists them.".into(),
                ));
            }
            act("bookmarks", obj)
        }
        // query is the listing's filter here, not a search to save.
        "list_saved_searches" => act("saved_searches", without(obj, &["name", "delete", "layers"])),
        "saved_searches" => {
            let deleting = obj.get("delete").and_then(Value::as_bool) == Some(true);
            if !deleting && obj.get("query").and_then(Value::as_str).is_none_or(|q| q.trim().is_empty()) {
                return Err(CallError::Tool(
                    "Give query to save the search, or delete: true to delete it. list_saved_searches lists them.".into(),
                ));
            }
            act("saved_searches", obj)
        }
        "list_changes" => act("changes", acting(obj, name, &["list", "show", "history"], "list", &["days", "mb"])?),
        "changes" => {
            let action = obj.get("action").and_then(Value::as_str).unwrap_or("");
            if (action == "accept" || action == "accept_run") && unwatched {
                return Err(CallError::Tool(
                    "Nothing held can be accepted in a session nobody is watching: the user accepts held changes on Brainstead's Changes screen.".into(),
                ));
            }
            if action == "history" && obj.get("days").is_none() && obj.get("mb").is_none() {
                return Err(CallError::Tool(
                    "Give days or mb to set how long Changes keeps its history; list_changes history reads it.".into(),
                ));
            }
            act(
                "changes",
                acting(obj, name, &["accept", "reject", "accept_run", "reject_run", "revert", "revert_run", "history"], "", &["page"])?,
            )
        }
        "weekly_start_over" | "moving_over" if unwatched => {
            Err(CallError::Tool("That's only for when the user is there and asks for it, not in an unattended session.".into()))
        }
        "weekly_start_over" => act("weekly.start_over", obj),
        "list_moving_over" => act("moving_over", acting(obj, name, &["list"], "list", &["item"])?),
        "moving_over" => act("moving_over", acting(obj, name, &["tick", "untick", "retire"], "", &[])?),
        "start_run" => act("run.start", obj),
        "run_status" => act("run.status", obj),
        "stop_run" => act("run.stop", obj),
        "list_transcripts" => act("meeting.transcripts", obj),
        "summary" => Ok(summary(ctx, parse(obj)?)?.into()),
        "weekly_review" => act("weekly.status", obj),
        "weekly_suggestion" => act("weekly.suggestion", obj),
        "save_chat" => act("chat.save", obj),
        "weekly_step" => act("weekly.step", obj),
        "list_chats" => act("chats", acting(obj, name, &["list", "read"], "list", &["title"])?),
        "chats" => act("chats", acting(obj, name, &["rename", "pin", "unpin", "trash"], "", &[])?),
        "health_issue" => act("health.issue", obj),
        "list_contradictions" => act("contradictions", acting(obj, name, &["list"], "list", &["id", "as"])?),
        "contradictions" => act("contradictions", acting(obj, name, &["mark", "save"], "", &[])?),
        "list_task_lists" => act("task_lists", acting(obj, name, &["list"], "list", &["name", "view", "context", "effort", "group"])?),
        "task_lists" => act("task_lists", acting(obj, name, &["save", "remove"], "", &[])?),
        "list_settings" => act("settings", acting(obj, name, &["get"], "get", &["key", "value"])?),
        "settings" => act("settings", acting(obj, name, &["set"], "set", &[])?),
        "list_note_looks" => act("note_look", acting(obj, name, &["list"], "list", &["theme", "colour"])?),
        "note_look" => act("note_look", acting(obj, name, &["set", "use_defaults", "use_defaults_for_all"], "", &[])?),
        "import_sources" => act("sources.import", obj),
        "create_template" => Ok(create_template(ctx, parse(obj)?)?.into()),
        "add_example_notes" => Ok(add_example_notes(ctx, parse(obj)?)?.into()),
        "fix_health" => act("health.fix", obj),
        "ignore_issue" => act("health.ignore", obj),
        "page_shape" => {
            let paging: Paging = parse(obj.clone())?;
            Ok(page_shape_tool(ctx, obj.get("page").and_then(Value::as_str), &paging)?.into())
        }
        "reshape_pages" => act("health.reshape", obj),
        "write_current_state" => act("health.current_state", obj),
        "fix_name" => act("fix_name", obj),
        "triage_bookmarks" => {
            if obj.get("items").and_then(Value::as_array).is_none_or(|a| a.is_empty()) {
                return Err(CallError::Tool("Give items: the bookmarks to ask about, from list_bookmarks.".into()));
            }
            act("triage.suggest", obj)
        }
        "draft_reply" => act("reply.draft", obj),
        "compare_docs" => act("compare.run", obj),
        "doc_check" => {
            let run = obj.get("document").and_then(Value::as_str).is_some_and(|c| !c.trim().is_empty());
            act(if run { "doccheck.run" } else { "doccheck.register" }, obj)
        }
        "activity" => act("activity", obj),
        "graph" => act("graph", obj),
        "open" => act("open", obj),
        "capture_extensions" => act("capture.status", obj),
        "list_assistants" => act("assistants.found", obj),
        "glance" => act("glance", obj),
        "app_status" => {
            // Then the system's features (brainstead_core::features), the table the window hides by.
            let mut out = act("status", obj)?;
            let f = brainstead_core::features::platform_features();
            out.text.push('\n');
            out.text.push_str(&f.describe());
            out.structured = Some(json!({"features": f}));
            Ok(out)
        }
        "rebuild_index" => act("index.rebuild", obj),
        "list_automated_tools" => {
            act("automated", acting(obj, name, &["list"], "list", &["name", "folder_contains", "first_message_starts_with"])?)
        }
        "automated_tools" => act("automated", acting(obj, name, &["add", "remove"], "", &[])?),
        "list_suggestions" => act("suggestions", acting(obj, name, &["list"], "list", &["id", "text", "done_looks_like"])?),
        "suggestions" => act("suggestions", acting(obj, name, &["accept", "skip"], "", &[])?),
        _ => Err(CallError::Unknown(format!("Unknown tool: {name}"))),
    }
}

/// Holds a tool to its own actions in the app's shared one: `action` must be one of `allowed`
/// (`default` when it's left out, or required when `default` is empty), and the arguments only
/// the other tool takes are dropped.
fn acting(mut obj: Value, tool: &str, allowed: &[&str], default: &str, drop: &[&str]) -> Result<Value, CallError> {
    let action = obj.get("action").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| default.to_string());
    if !allowed.contains(&action.as_str()) {
        let list = allowed.join(", ");
        return Err(CallError::Tool(if action.is_empty() {
            format!("Give action: {list}.")
        } else {
            format!("{tool} does {list}, not {action}: the tools/list descriptions say which tool does that.")
        }));
    }
    obj["action"] = json!(action);
    Ok(without(obj, drop))
}

/// The arguments without these.
fn without(mut obj: Value, keys: &[&str]) -> Value {
    if let Some(m) = obj.as_object_mut() {
        for k in keys {
            m.remove(*k);
        }
    }
    obj
}

/// How long the server waits for the app to answer one action.
const ACT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(150);

/// Runs an action in the app (src/mcpActions.ts, through core/src/bridge.rs), opening the app
/// first when it isn't running. A text result is given back as it is, a listing as its text and
/// data; anything else as JSON.
fn act(ctx: &Ctx, action: &str, args: Value) -> Result<Out, String> {
    open_app(ctx)?;
    let req = bridge::Request { id: bridge::new_id(), action: action.into(), args, chat: ctx.chat.clone(), model: ctx.model.clone() };
    let r = bridge::ask(&ctx.data, &req, ACT_TIMEOUT);
    if !r.ok {
        return Err(r.error.unwrap_or_else(|| "It didn't work.".into()));
    }
    Ok(match r.result {
        Value::String(s) => s.into(),
        Value::Null => String::from("Done.").into(),
        // A listing's text and its rows as data (src/mcpActions.ts's listed).
        Value::Object(mut m) if m.get("text").is_some_and(Value::is_string) && m.contains_key("structured") => Out {
            text: m.remove("text").and_then(|t| t.as_str().map(String::from)).unwrap_or_default(),
            structured: m.remove("structured"),
        },
        v => serde_json::to_string_pretty(&v).unwrap_or_default().into(),
    })
}

/// Opens Brainstead when it isn't running (the app's own binary, with the same app data folder)
/// and waits for it to say it's up.
fn open_app(ctx: &Ctx) -> Result<(), String> {
    if bridge::app_alive(&ctx.data) {
        return Ok(());
    }
    let exe = ctx.app.as_ref().ok_or("Brainstead isn't open. Open it, then try again.")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.env("BRAINSTEAD_DATA", &ctx.data)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: the app keeps running when the assistant's session ends.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    let spawned = {
        use std::os::windows::process::CommandExt;
        // No console, its own process group (a Ctrl+C in the assistant's terminal doesn't reach
        // it), and out of the session's job object, so closing the session doesn't end the app.
        // A job that doesn't allow breaking away refuses the start (access denied); then it's
        // started inside the job.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB).spawn().or_else(|_| {
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            cmd.spawn()
        })
    };
    #[cfg(not(windows))]
    let spawned = cmd.spawn();
    spawned.map_err(|e| format!("Brainstead isn't open, and opening it failed: {e}. Open it, then try again."))?;
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(45) {
        if bridge::app_alive(&ctx.data) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Err("Brainstead was opened but didn't start in time. Try again in a moment.".into())
}

#[derive(Deserialize)]
struct TaskRefArgs {
    task: String,
    text: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

/// A task's words, without its tags, dates, fields and ids, to compare what a model gives with
/// the line (src/mcpActions.ts has the same rule).
fn task_words(s: &str) -> String {
    static NOISE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"#[^\s#]+|[📅⏳🛫➕✅❌]\u{fe0f}?\s*\d{4}-\d{2}-\d{2}|[🔺⏫🔼🔽⏬🔁🏁⛔🆔]\u{fe0f}?|[\[(]\w+::[^\])]*[\])]|\^[\w-]+|\[\[([^\]|]*\|)?|\]\]|^\s*[-*+]\s+\[.\]\s*")
            .unwrap()
    });
    NOISE.replace_all(s, " ").split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// `path:line` (1-based) and the task's text to the line in the file, as the app's own lookup.
fn find_task(ctx: &Ctx, id: &str, text: &str) -> Result<(String, usize, String), String> {
    let (path, n) = id.rsplit_once(':').ok_or_else(|| format!("“{id}” isn't a task id: use the path:line that list_tasks gives."))?;
    let n: usize = n.parse().map_err(|_| format!("“{id}” isn't a task id: use the path:line that list_tasks gives."))?;
    brainstead_core::trash::safe_rel(path).map_err(|e| e.to_string())?;
    let body = read_text(ctx, path).map_err(|_| format!("{path} isn't in the vault."))?;
    let want = task_words(text);
    let lines: Vec<&str> = body.lines().collect();
    let fits = |l: &str| {
        brainstead_core::tasks::parse_line(l, 0).is_some_and(|t| {
            let have = task_words(&t.text);
            want.is_empty() || have.contains(&want) || (!have.is_empty() && want.contains(&have))
        })
    };
    if let Some(l) = n.checked_sub(1).and_then(|i| lines.get(i)).filter(|l| fits(l)) {
        return Ok((path.to_string(), n - 1, l.to_string()));
    }
    let hits: Vec<usize> = (0..lines.len()).filter(|&i| fits(lines[i])).collect();
    match hits.as_slice() {
        [i] => Ok((path.to_string(), *i, lines[*i].to_string())),
        _ => Err(format!("There's no task at {id} reading “{text}” any more. List the tasks again for its current place.")),
    }
}

fn delete_task(ctx: &Ctx, a: TaskRefArgs) -> Result<String, String> {
    let (path, at, line) = find_task(ctx, &a.task, &a.text)?;
    let text = brainstead_core::tasks::parse_line(&line, 0).map(|t| t.text).unwrap_or_default();
    let c = change(
        ctx,
        &path,
        Kind::Task,
        &format!("Delete “{text}”"),
        &a.reason,
        a.unattended,
        Instruction::DeleteLine { line, at: Some(at) },
    );
    send(ctx, c, vec![])
}

/// A change for the app (src/changes.rs's `Submit`), from this session.
fn change(ctx: &Ctx, page: &str, kind: Kind, title: &str, reason: &str, unattended: bool, instruction: Instruction) -> Value {
    json!({
        "page": page,
        "kind": kind,
        "title": title,
        "reason": reason,
        "instruction": instruction,
        "origin": origin(ctx, unattended),
        "model": ctx.model,
    })
}

/// Sends a change to the app, which makes it or holds it; its reply, and `notes` after it.
fn send(ctx: &Ctx, change: Value, notes: Vec<String>) -> Result<String, String> {
    let mut out = act(ctx, "change.submit", json!({ "change": change }))?.text;
    for n in notes {
        out.push_str(&format!("\n{n}"));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct RenameArgs {
    page: String,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    title: String,
    #[serde(default)]
    date: Option<String>,
    /// Only say which links would be rewritten, as the Rename box lists them.
    #[serde(default)]
    preview: bool,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

fn rename_note(ctx: &Ctx, a: RenameArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let rel = existing(ctx, &ix, &a.page)?;
    // The file keeps its own extension, as the file menu's Rename does: a source report.pdf
    // renamed is still a PDF (a title given with that extension isn't doubled).
    let ext = Path::new(&rel).extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    let title = match a.title.trim().len().checked_sub(ext.len()) {
        Some(cut) if !ext.is_empty() && a.title.trim().is_char_boundary(cut) && a.title.trim()[cut..].eq_ignore_ascii_case(&ext) => {
            &a.title.trim()[..cut]
        }
        _ => a.title.trim(),
    };
    let name = filename::compose(a.kind.as_deref(), title, a.date.as_deref()).ok_or("The new name needs a title.")?;
    let name = format!("{}{ext}", name.strip_suffix(".md").unwrap_or(&name));
    let to = match rel.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{name}"),
        None => name,
    };
    if to == rel {
        return Err("That's its name already.".into());
    }
    if ctx.vault.join(&to).exists() && !to.eq_ignore_ascii_case(&rel) {
        return Err(format!("{to} exists already."));
    }
    if a.preview {
        return rename_preview(ctx, &ix, &rel, &to);
    }
    let c =
        change(ctx, &rel, Kind::Rename, &format!("Rename to {}", filename::stem(&to)), &a.reason, a.unattended, Instruction::Rename { to });
    send(ctx, c, vec![])
}

/// The links a rename would rewrite, as the Rename box lists them: how many in how many files,
/// then each file with its lines before and after. Nothing is changed.
fn rename_preview(ctx: &Ctx, ix: &Index, rel: &str, to: &str) -> Result<String, String> {
    let linkers = ix.linkers(rel).map_err(|e| e.to_string())?;
    let resolves = |t: &str| ix.resolve(&[t.to_string()]).ok().and_then(|v| v.into_iter().next().flatten()).as_deref() == Some(rel);
    let plan = brainstead_core::rename::plan(&ctx.vault, rel, to, &linkers, &resolves).map_err(|e| e.to_string())?;
    let mut files: Vec<(&str, Vec<&brainstead_core::rename::LinkChange>)> = Vec::new();
    for c in &plan.changes {
        match files.iter_mut().find(|(f, _)| *f == c.path) {
            Some((_, cs)) => cs.push(c),
            None => files.push((&c.path, vec![c])),
        }
    }
    let n = |k: usize, w: &str| format!("{k} {w}{}", if k == 1 { "" } else { "s" });
    if files.is_empty() {
        return Ok(format!("Renaming {rel} to {to} rewrites no links: no other file links here. Call again without preview to rename it."));
    }
    let mut out = vec![format!("Renaming {rel} to {to}: {} in {} will be updated:", n(plan.changes.len(), "link"), n(files.len(), "file"))];
    for (f, cs) in &files {
        out.push(format!("- {f} ({})", n(cs.len(), "link")));
        for c in cs {
            out.push(format!("  line {}: {} → {}", c.line + 1, c.before.trim(), c.after.trim()));
        }
    }
    out.push("Call again without preview to rename it.".into());
    Ok(out.join("\n"))
}

#[derive(Deserialize)]
struct ImageArgs {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    base64: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

/// The image's bytes and its file name, from a path or base64, checked before it's sent to the
/// app: an image by its bytes, small enough, not in the vault already.
fn image_input(ctx: &Ctx, a: &ImageArgs) -> Result<(Vec<u8>, String), String> {
    use base64::Engine as _;
    let given = a.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let (bytes, name) = match (a.path.as_deref().map(str::trim).filter(|p| !p.is_empty()), a.base64.as_deref()) {
        (Some(_), Some(_)) => return Err("Give path or base64, not both.".into()),
        (None, None) => return Err("Give path (an image file) or base64 (its bytes) with name.".into()),
        (Some(p), None) => {
            let abs = match p.strip_prefix("~/") {
                Some(rest) => std::env::var_os("HOME").map(PathBuf::from).ok_or("There's no home folder to find ~/ in.")?.join(rest),
                None => PathBuf::from(p),
            };
            let abs = std::fs::canonicalize(&abs).map_err(|e| format!("Couldn't find {p}: {e}"))?;
            if let Some(rel) = std::fs::canonicalize(&ctx.vault).ok().and_then(|v| abs.strip_prefix(v).ok().map(Path::to_path_buf)) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                return Err(format!("That image is in the vault already, at {rel}: embed it with ![[{rel}]]."));
            }
            let len = std::fs::metadata(&abs).map_err(|e| format!("Couldn't read {p}: {e}"))?.len();
            if !abs.is_file() || len > images::MAX_BYTES as u64 {
                return Err(if abs.is_file() {
                    format!("{p} is over {} MB.", images::MAX_BYTES >> 20)
                } else {
                    format!("{p} isn't a file.")
                });
            }
            let bytes = std::fs::read(&abs).map_err(|e| format!("Couldn't read {p}: {e}"))?;
            let own = abs.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (bytes, given.map(str::to_string).unwrap_or(own))
        }
        (None, Some(b64)) => {
            let bytes =
                base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| format!("base64 isn't valid base64: {e}"))?;
            (bytes, given.ok_or("Give the image a name, such as launch-plan.png.")?.to_string())
        }
    };
    if bytes.len() > images::MAX_BYTES {
        return Err(format!("That image is over {} MB.", images::MAX_BYTES >> 20));
    }
    let ext = images::sniff(&bytes).ok_or("That isn't an image: Brainstead adds png, jpg, gif, webp, bmp, avif and svg.")?;
    let base = name.rsplit('/').next().unwrap_or(&name);
    let stem = match base.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => base,
    };
    Ok((bytes, format!("{stem}.{ext}")))
}

/// Pasting an image: saved to images/ by the app and recorded in Changes.
fn add_image(ctx: &Ctx, a: ImageArgs) -> Result<String, String> {
    use base64::Engine as _;
    let (bytes, name) = image_input(ctx, &a)?;
    let args = json!({
        "name": name,
        "base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        "reason": a.reason,
        "origin": origin(ctx, a.unattended),
    });
    Ok(act(ctx, "image.add", args)?.text)
}

#[derive(Deserialize)]
struct TrashArgs {
    page: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

/// A Teams transcript in sources/ or a folder in it, as src/meetingFlow.ts's isTranscriptPath has it.
fn is_transcript_page(rel: &str) -> bool {
    rel.starts_with("sources/") && filename::stem(rel).starts_with("Teams. Transcript.")
}

fn trash_note(ctx: &Ctx, a: TrashArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let rel = existing(ctx, &ix, &a.page)?;
    let c =
        change(ctx, &rel, Kind::Trash, &format!("Move {} to the Trash", filename::stem(&rel)), &a.reason, a.unattended, Instruction::Trash);
    send(ctx, c, vec![])
}

#[derive(Deserialize)]
struct NoteArgs {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    title: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    content: Option<String>,
    /// With these, the app runs the template (`note.from_template`) instead.
    #[serde(default)]
    template: Option<String>,
    /// Text, or a list of choices for a question that takes several.
    #[serde(default)]
    #[allow(dead_code)]
    answers: Vec<Value>,
    #[serde(default)]
    test_run: bool,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

/// Where a new note goes, checked as a plain note's and a template's alike: the date's shape, a
/// folder that's safe, not sources/, wiki/ or Templates/, and there, and a name a note can have.
fn new_note_path(ctx: &Ctx, a: &NoteArgs) -> Result<String, String> {
    if let Some(d) = a.date.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        let ok = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
            || chrono::NaiveDate::parse_from_str(&format!("{d}-01"), "%Y-%m-%d").is_ok();
        if !ok {
            return Err(format!("The date {d} isn't YYYY-MM-DD or YYYY-MM."));
        }
    }
    let name = filename::compose(a.kind.as_deref(), &a.title, a.date.as_deref()).ok_or("The note needs a title.")?;
    let mut folder = a.folder.as_deref().unwrap_or("").trim().trim_matches(paths::SEPARATORS).to_string();
    if !folder.is_empty() {
        // As the file system spells it, so `templates` and `TEMPLA~1` are Templates/.
        folder = paths::vault_rel(&ctx.vault, &folder)?;
        let first = folder.split('/').next().unwrap_or("");
        if folder.split('/').any(|s| s.starts_with('.')) || proposals::NOT_FOR_NOTES.iter().any(|n| n.eq_ignore_ascii_case(first)) {
            return Err(format!("A note can't go in {folder}/: not in sources/, wiki/ (edit_page makes wiki pages) or Templates/."));
        }
        if !ctx.vault.join(&folder).is_dir() {
            return Err(format!("There's no folder {folder}/ in the vault."));
        }
    }
    let rel = if folder.is_empty() { name } else { format!("{folder}/{name}") };
    proposals::valid_new_note(&rel)?;
    Ok(rel)
}

fn create_note(ctx: &Ctx, a: NoteArgs) -> Result<String, String> {
    if a.test_run {
        return Err("test_run is the Test run of a template: give template too.".into());
    }
    let rel = new_note_path(ctx, &a)?;
    if ctx.vault.join(&rel).exists() {
        return Err(format!("{rel} exists already. To change it, use edit_page."));
    }
    let content = a
        .content
        .filter(|c| !c.trim().is_empty())
        .ok_or("Give content (the whole note), or template to make it from one of the vault's templates.")?;
    let mut after = content.replace("\r\n", "\n");
    if !after.ends_with('\n') {
        after.push('\n');
    }
    let title = format!("New note {}", filename::stem(&rel));
    let c = change(ctx, &rel, Kind::New, &title, &a.reason, a.unattended, Instruction::Page { content: after });
    send(ctx, c, vec![])
}

#[derive(Deserialize)]
struct TemplateArgs {
    name: String,
    content: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

/// Templates' New template: a new file in Templates/, which the app always holds for the user.
fn create_template(ctx: &Ctx, a: TemplateArgs) -> Result<String, String> {
    let name = a.name.trim().trim_end_matches(".md").trim();
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return Err("Give the template a name, without a folder.".into());
    }
    if name.contains(['\\', ':', '*', '?', '"', '<', '>', '|', '\n']) {
        return Err(format!("{name} isn't a name a template can have."));
    }
    let rel = format!("Templates/{name}.md");
    if ctx.vault.join(&rel).exists() {
        return Err(format!("{rel} exists already. To change it, use edit_page."));
    }
    if a.content.trim().is_empty() {
        return Err("Give content: the whole template.".into());
    }
    let mut after = a.content.replace("\r\n", "\n");
    if !after.ends_with('\n') {
        after.push('\n');
    }
    let c = change(ctx, &rel, Kind::New, &format!("New template {name}"), &a.reason, a.unattended, Instruction::Page { content: after });
    send(ctx, c, vec![])
}

#[derive(Deserialize)]
struct ReasonArgs {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

/// Settings › Vault's Add the example notes: the starter notes the vault doesn't have, each a new
/// note sent as a change (core/src/starter.rs's add_missing writes the same ones straight away).
fn add_example_notes(ctx: &Ctx, a: ReasonArgs) -> Result<String, String> {
    let now = ctx.today.and_time(chrono::Local::now().time());
    let missing: Vec<(String, String)> =
        brainstead_core::starter::files(now).into_iter().filter(|(rel, _)| !ctx.vault.join(rel).exists()).collect();
    if missing.is_empty() {
        return Err("The example notes are all there already: nothing to add.".into());
    }
    let mut out = format!("{} example note{}:", missing.len(), if missing.len() == 1 { "" } else { "s" });
    let mut sent = 0;
    let mut first_error = None;
    for (rel, text) in missing {
        let c = change(
            ctx,
            &rel,
            Kind::New,
            &format!("Example note {}", filename::stem(&rel)),
            &a.reason,
            a.unattended,
            Instruction::Page { content: text },
        );
        let said = match send(ctx, c, vec![]) {
            Ok(m) => {
                sent += 1;
                m
            }
            Err(e) => {
                let said = format!("not added: {e}");
                first_error.get_or_insert(e);
                said
            }
        };
        out.push_str(&format!("\n- {rel}: {said}"));
    }
    // None of them sent (the vault read-only, the app gone): the call failed.
    if sent == 0 {
        return Err(first_error.unwrap_or_default());
    }
    out.push_str("\nStart here lists them.");
    Ok(out)
}

#[derive(Deserialize)]
struct HelpArgs {
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    query: Option<String>,
}

/// The app's help (core/src/help.rs), with its shortcuts as this system names them; it needs no vault.
fn help(a: HelpArgs) -> Result<String, String> {
    let some = |s: Option<String>| s.filter(|x| !x.trim().is_empty());
    match (some(a.topic), some(a.query)) {
        (Some(t), _) => brainstead_core::help::read(&t, a.section.as_deref()),
        (None, Some(q)) => Ok(brainstead_core::help::search(&q, 5)),
        (None, None) => Ok(brainstead_core::help::list()),
    }
    .map(|t| keys(&t))
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    layers: Vec<String>,
    since: Option<String>,
    limit: Option<usize>,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    detail: bool,
}

/// How much of a passage search shows without detail.
const SNIPPET: usize = 160;

fn search(ctx: &Ctx, a: SearchArgs) -> Result<Out, String> {
    let ix = ctx.index()?;
    let layers = a
        .layers
        .iter()
        .map(|l| match l.trim_end_matches('s') {
            "note" => Ok("note".to_string()),
            "wiki" => Ok("wiki".to_string()),
            "source" => Ok("source".to_string()),
            "template" => Ok("template".to_string()),
            _ => Err(format!("Not a part of the vault: {l}. Use notes, wiki, sources or templates.")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let since = match a.since.as_deref() {
        Some(d) => Some(chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| format!("Not a date: {d}. Use YYYY-MM-DD."))?),
        None => None,
    };
    let limit = a.limit.unwrap_or(10).clamp(1, 30);
    // since is the screen's date range: `since:` added to the query unless it has its own (src/Search.tsx),
    // so the search itself goes by the note's own date, as `since:` does.
    let q = match since {
        Some(d) if !a.query.split_whitespace().any(|w| w.starts_with("since:")) => format!("{} since:{d}", a.query),
        _ => a.query.clone(),
    };
    let res = ix.search(&SearchRequest { q, layers, limit: 200 })?;
    let day = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&chrono::Local).date_naive());
    let mut hits: Vec<_> = res.hits.iter().collect();
    // The screen's Order: as ranked, or by the note's date (else the day it last changed), as
    // src/Search.tsx's sortHits orders them.
    let when = |h: &&brainstead_core::search::Hit| {
        (h.file.date.clone().or_else(|| day(h.file.mtime).map(|d| d.to_string())).unwrap_or_default(), h.file.mtime)
    };
    match a.order.as_deref().unwrap_or("best_match") {
        "best_match" => {}
        "latest" => hits.sort_by_key(|h| std::cmp::Reverse(when(h))),
        "oldest" => hits.sort_by_key(when),
        o => return Err(format!("order is best_match, latest or oldest, not {o}.")),
    }
    let mut items = Vec::new();
    let mut out = if hits.is_empty() {
        format!("Nothing in the vault matches {}.", a.query)
    } else {
        format!("{} of {} matching files for {}:\n", hits.len().min(limit), hits.len(), a.query)
    };
    for (i, h) in hits.iter().take(limit).enumerate() {
        let when = h.file.date.clone().or_else(|| day(h.file.mtime).map(|d| d.to_string())).unwrap_or_default();
        let snip: String = h.snippet.iter().map(|s| s.text.as_str()).collect();
        let snip = snip.split_whitespace().collect::<Vec<_>>().join(" ");
        if a.detail {
            out.push_str(&format!("\n{}. {} — {} ({}, {when})\n", i + 1, h.file.path, h.file.title, h.file.layer));
        } else {
            out.push_str(&format!("\n{}. {} — {}\n", i + 1, h.file.path, h.file.title));
        }
        if !h.heading.is_empty() {
            out.push_str(&format!("   § {}\n", h.heading));
        }
        let shown = if a.detail || snip.chars().count() <= SNIPPET {
            snip.clone()
        } else {
            format!("{}…", snip.chars().take(SNIPPET).collect::<String>().trim_end())
        };
        if !shown.is_empty() {
            out.push_str(&format!("   {shown}\n"));
        }
        items.push(
            json!({"path": h.file.path, "title": h.file.title, "layer": h.file.layer, "date": when, "heading": h.heading, "snippet": snip}),
        );
    }
    let structured = json!({"total": hits.len(), "matching": hits.len(), "offset": 0, "next_offset": null, "items": items});
    Ok(Out { text: out, structured: Some(structured) })
}

/// What a page argument names.
enum PageRef {
    /// A file in the vault, and its text when it's text.
    Found(String),
    /// A path that isn't there.
    NotThere(String),
}

/// `[[Orbit App#Current state|the app]]` → `Orbit App`.
fn bare(page: &str) -> String {
    let t = page.trim().trim_start_matches("![[").trim_start_matches("[[").trim_end_matches("]]");
    let t = t.split('|').next().unwrap_or(t);
    let t = t.split('#').next().unwrap_or(t);
    t.trim().trim_start_matches('/').nfc().collect()
}

fn find_page(ctx: &Ctx, ix: &Index, page: &str) -> Result<PageRef, String> {
    let p = bare(page);
    if p.is_empty() {
        return Err("Which page? None was given.".into());
    }
    if p.contains(paths::SEPARATORS) || p.ends_with(".md") {
        // The page by the file's own spelling, which the app's rules for changes go by: on Windows
        // `templates\meeting.md` and `TEMPLA~1/Meeting.md` are `Templates/Meeting.md`.
        let with_ext = if Path::new(&p).extension().is_some() { p.clone() } else { format!("{p}.md") };
        for cand in [&p, &with_ext] {
            let rel = paths::vault_rel(&ctx.vault, cand)?;
            if ctx.vault.join(&rel).is_file() {
                return Ok(PageRef::Found(rel));
            }
        }
        return Ok(PageRef::NotThere(paths::vault_rel(&ctx.vault, &with_ext)?));
    }
    match ix.resolve(std::slice::from_ref(&p))?.into_iter().next().flatten() {
        Some(rel) => Ok(PageRef::Found(rel)),
        None => Err(format!(
            "There's no page called {p}. resolve_entity finds close names; a new page is proposed by its path, as wiki/entities/{p}.md."
        )),
    }
}

fn existing(ctx: &Ctx, ix: &Index, page: &str) -> Result<String, String> {
    match find_page(ctx, ix, page)? {
        PageRef::Found(rel) => Ok(rel),
        PageRef::NotThere(rel) => Err(format!("There's no file at {rel}.")),
    }
}

fn is_text(rel: &str) -> bool {
    brainstead_core::vault::is_note_ext(rel) || rel.ends_with(".txt")
}

fn read_text(ctx: &Ctx, rel: &str) -> Result<String, String> {
    if !is_text(rel) {
        return Err(format!("{rel} isn't a text file: read it with read_section."));
    }
    std::fs::read(ctx.vault.join(rel)).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| format!("Couldn't read {rel}: {e}"))
}

#[derive(Deserialize)]
struct ReadArgs {
    page: String,
    heading: Option<String>,
}

fn headings(text: &str) -> Vec<(usize, usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let md = brainstead_core::markdown::lines(text, 0);
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !md.get(*i).is_some_and(|l| l.code))
        .filter_map(|(i, l)| brainstead_core::markdown::heading(l.trim_end_matches('\r')).map(|(n, t)| (i, n, t.to_string())))
        .collect()
}

/// A page or section within the result's limit. One with headings is shown as an outline with its
/// opening, summing-up and newest sections in full (`pageview::view`); one without keeps its start
/// and most of its end (`pageview::excerpt`), as what's new is mostly at the end. Either says how
/// to read the rest.
fn fit(text: &str, len: usize, has_headings: bool) -> String {
    let budget = MAX_CHARS - 2_000;
    if len <= budget {
        return text.to_string();
    }
    let (shown, how) = if has_headings {
        (brainstead_core::pageview::view(text, budget).text, "Sections shown only by their heading: ask for one by its heading.")
    } else {
        (brainstead_core::pageview::excerpt(text, budget), "The middle isn't shown: search for words in it to find what you need.")
    };
    format!("[{len} characters in all, so shown in part. {how}]\n\n{shown}")
}

fn read_section(ctx: &Ctx, a: ReadArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let rel = existing(ctx, &ix, &a.page)?;
    if !is_text(&rel) {
        return read_parts(ctx, &ix, &rel, a.heading.as_deref());
    }
    let text = read_text(ctx, &rel)?;
    let hs = headings(&text);
    let list = || hs.iter().map(|(_, n, t)| format!("{}{t}", "  ".repeat(n.saturating_sub(1)))).collect::<Vec<_>>().join("\n");
    match a.heading.as_deref().map(|h| h.trim().trim_start_matches('#').trim()).filter(|h| !h.is_empty()) {
        None => Ok(format!("{rel}\n\n{}", fit(&text, text.chars().count(), !hs.is_empty()))),
        Some(h) => {
            let Some(at) = hs.iter().position(|(_, _, t)| t.eq_ignore_ascii_case(h)) else {
                return Err(format!("{rel} has no heading “{h}”. Its headings:\n{}", list()));
            };
            let (start, level, _) = hs[at];
            let end = hs[at + 1..].iter().find(|(_, n, _)| *n <= level).map(|(i, _, _)| *i);
            let lines: Vec<&str> = text.split('\n').collect();
            let part = lines[start..end.unwrap_or(lines.len())].join("\n");
            let subs = hs[at + 1..].iter().take_while(|(_, n, _)| *n > level).count() > 0;
            Ok(format!("{rel}\n\n{}", fit(part.trim_end(), part.chars().count(), subs).trim_end()))
        }
    }
}

/// A PDF or Office file: one page, slide or sheet by its name ("page=6", "Page 6", "Slide 2",
/// a sheet's name), or all of it with each part headed.
fn read_parts(ctx: &Ctx, ix: &Index, rel: &str, which: Option<&str>) -> Result<String, String> {
    let x = extracted(ctx, ix, rel).ok_or_else(|| format!("Brainstead has no text for {rel} (it may be a scan, or not indexed yet)."))?;
    let names = || x.parts.iter().map(|p| p.label.clone()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(", ");
    match which.map(str::trim).filter(|w| !w.is_empty()) {
        Some(w) => {
            let page = proposals::page_anchor(w).map(|n| format!("Page {n}"));
            let want = page.as_deref().unwrap_or(w);
            let p = x
                .parts
                .iter()
                .find(|p| p.label.eq_ignore_ascii_case(want))
                .ok_or_else(|| format!("{rel} has no “{w}”. It has: {}", names()))?;
            Ok(format!("{rel} · {}\n\n{}", p.label, p.text))
        }
        None => {
            let all: Vec<String> =
                x.parts.iter().map(|p| if p.label.is_empty() { p.text.clone() } else { format!("## {}\n\n{}", p.label, p.text) }).collect();
            let all = all.join("\n\n");
            if all.chars().count() > MAX_CHARS {
                return Ok(format!("{rel} is long ({} characters). Ask for one part: {}", all.chars().count(), names()));
            }
            Ok(format!("{rel}\n\n{all}"))
        }
    }
}

#[derive(Deserialize)]
struct BacklinkArgs {
    page: String,
    #[serde(flatten)]
    paging: Paging,
}

fn backlinks(ctx: &Ctx, a: BacklinkArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let rel = existing(ctx, &ix, &a.page)?;
    let Some(mut meta) = ix.doc_meta(&rel)? else { return Err(format!("{rel} isn't in Brainstead's index yet.")) };
    brainstead_core::read::fill_context(&ctx.vault, &mut meta.backlinks);
    if meta.backlinks.is_empty() {
        return Ok(format!("Nothing links to {rel}."));
    }
    let (shown, head) = page_of(&meta.backlinks, |b| format!("{} {}", b.path, b.context), &a.paging, ("link", "links"), 50);
    let mut out = format!("{rel}: {head}:\n");
    for b in shown {
        let line = b.line.map(|n| format!(" (line {})", n + 1)).unwrap_or_default();
        out.push_str(&format!("- {}{line}: {}\n", b.path, b.context));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}

fn resolve_entity(ctx: &Ctx, a: NameArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let name = bare(&a.name);
    if name.is_empty() {
        return Err("Which name? None was given.".into());
    }
    let r = brainstead_core::names::resolve(&ix, &name)?;
    if let Some((rel, how)) = r.exact {
        let how = if how == "alias" { "an alias" } else { "its file name" };
        return Ok(format!("{name} is {rel} (by {how})."));
    }
    if r.close.is_empty() {
        return Ok(format!("No page is called {name}, or anything close."));
    }
    let mut out = format!("No page is called exactly {name}. Close:\n");
    for (p, s) in &r.close {
        out.push_str(&format!("- {p} ({:.0}% alike)\n", s * 100.0));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct FactsArgs {
    page: Option<String>,
    subject: Option<String>,
    attribute: Option<String>,
}

/// One kept claim as a line: value, when, quote, source, entry.
fn fact_line(c: &brainstead_core::claims::Kept) -> String {
    let when = c.as_of.as_deref().map(|d| format!(" (as of {d})")).unwrap_or_default();
    let entry = c.entry.as_deref().map(|e| format!(", entry “{e}”")).unwrap_or_default();
    let name = brainstead_core::lint::stem(brainstead_core::lint::name_of(&c.source));
    format!("{}{when}: “{}” from [[{name}]]{entry}, recorded {}", c.value, c.quote, c.recorded)
}

fn facts(ctx: &Ctx, a: FactsArgs) -> Result<String, String> {
    let page = match a.page.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(p) => Some(existing(ctx, &ctx.index()?, p)?),
        None => None,
    };
    let files: Vec<_> =
        brainstead_core::claims::all(&ctx.vault).into_iter().filter(|f| page.as_ref().is_none_or(|p| *p == f.page)).collect();
    let mut out = String::new();
    for f in &files {
        let found = brainstead_core::claims::facts(f, a.subject.as_deref(), a.attribute.as_deref());
        if found.is_empty() {
            continue;
        }
        out.push_str(&format!("{}:\n", f.page));
        for x in found {
            out.push_str(&format!("- {} — {}: {}\n", x.subject, x.attribute, fact_line(&x.latest)));
            for e in &x.earlier {
                out.push_str(&format!("  - superseded: {}\n", fact_line(e)));
            }
        }
    }
    if out.is_empty() {
        let on = page.map(|p| format!(" on {p}")).unwrap_or_default();
        return Ok(format!(
            "No kept facts match{on}. Facts are kept from the first ingest that checks them, so older pages may have none: read the page (read_section) instead."
        ));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct SummaryArgs {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    day: Option<String>,
}

/// A daily or weekly summary block, read from the monthly files (either name, before or after the
/// rename): that day's or week's, or the latest within the look-back.
fn summary(ctx: &Ctx, a: SummaryArgs) -> Result<String, CallError> {
    use brainstead_core::reviews::{self, target, ReviewKind, Target};
    let kind = match a.kind.as_deref().map(str::trim).unwrap_or("daily") {
        "" | "daily" => ReviewKind::Daily,
        "weekly" => ReviewKind::Weekly,
        k => return Err(CallError::Tool(format!("kind is daily or weekly, not {k}."))),
    };
    let read = |p: &str| std::fs::read_to_string(ctx.vault.join(p)).ok();
    let name = if kind == ReviewKind::Daily { "daily" } else { "weekly" };
    let found = match a.day.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => {
            reviews::latest(kind, ctx.today, read).ok_or_else(|| format!("There's no {name} summary from the last few weeks on file."))?
        }
        Some(d) => {
            let t = Target::parse(d).map_err(|_| CallError::Tool(format!("day is YYYY-MM-DD (or a week, 2026-W40), not {d}.")))?;
            let t = match (kind, t) {
                (ReviewKind::Weekly, Target::Day(day)) => Target::week_of(day),
                (ReviewKind::Daily, Target::Week { .. }) => {
                    return Err(CallError::Tool("A daily summary is for a day: give YYYY-MM-DD.".into()))
                }
                (_, t) => t,
            };
            let w = target::window(t);
            let (file, heading) = w.locate(read);
            let text = read(&file).and_then(|f| reviews::latest::block_of(&f, &heading)).ok_or_else(|| {
                format!("There's no {name} summary for {} on file. start_run {name}_summary with date writes one.", w.label())
            })?;
            reviews::Latest { file, heading, label: w.label().to_string(), text }
        }
    };
    Ok(format!("The {name} summary for {}, from {}:\n\n{}", found.label, found.file, found.text))
}

#[derive(Deserialize)]
struct PendingArgs {
    #[serde(default)]
    status: Option<String>,
    #[serde(flatten)]
    paging: Paging,
}

/// Sources as the Sources screen gives their Status: New (no wiki page cites it), Changed (newer
/// than the pages citing it, Knowledge health's check) or Ingested. Without status, the ones still
/// to ingest: New and Changed.
fn pending_sources(ctx: &Ctx, a: PendingArgs) -> Result<String, String> {
    let want = a.status.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if want.is_some_and(|w| !["all", "new", "changed", "ingested"].contains(&w)) {
        return Err("status is all, new, changed or ingested, as the Sources screen's Status; New and Changed when left out.".into());
    }
    let ix = ctx.index()?;
    let files = ix.list(Some("source"))?;
    let paths: Vec<String> = files.into_iter().map(|f| f.path).collect();
    let ingested = ix.ingested(&paths)?;
    let report = lint::run(&ctx.vault, ctx.today, &Default::default());
    let changed: std::collections::HashSet<String> =
        report.check("changed-sources").map(|c| c.items.iter().filter_map(|i| i.page.clone()).collect()).unwrap_or_default();
    let rows: Vec<(String, &str)> = paths
        .into_iter()
        .zip(ingested)
        .map(|(p, ing)| {
            let st = if changed.contains(&p) {
                "changed"
            } else if ing {
                "ingested"
            } else {
                "new"
            };
            (p, st)
        })
        // A file ingest can't read (a zip, an SVG…) is never one to ingest, as the Sources screen
        // offers it no Ingest; Show all lists it, marked.
        .filter(|(p, st)| match want {
            None => *st != "ingested" && extract::ingestable(p),
            Some("all") => true,
            Some(w) => *st == w && extract::ingestable(p),
        })
        .collect();
    let label = |st: &str| match st {
        "new" => "New",
        "changed" => "Changed (since the pages citing it)",
        _ => "Ingested",
    };
    if rows.is_empty() {
        return Ok(match want {
            None => "Every source is ingested: a wiki page cites each, and none has changed since.".into(),
            Some(w) => format!("No sources with status {w}."),
        });
    }
    let (shown, head) = page_of(&rows, |(p, st)| format!("{p} {st}"), &a.paging, ("source", "sources"), 100);
    let what = if want.is_none() { ", still to ingest (New or Changed)" } else { "" };
    let mut out = format!("{head}{what}:\n");
    for (p, st) in shown {
        let name = p.rsplit('/').next().unwrap_or(p);
        let note = if !extract::ingestable(p) {
            " · can't be ingested (not text, a PDF, an Office file or an image)"
        } else if brainstead_core::meeting::is_transcript(name) {
            " · a transcript: start_run meeting_note writes it up"
        } else {
            ""
        };
        out.push_str(&format!("- {p} · {}{note}\n", label(st)));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct LintArgs {
    page: Option<String>,
}

/// The page shape dry run, for the vault or one page.
fn page_shape_tool(ctx: &Ctx, page: Option<&str>, paging: &Paging) -> Result<String, String> {
    use brainstead_core::pageshape;
    if let Some(p) = page.filter(|p| !p.trim().is_empty()) {
        let rel = existing(ctx, &ctx.index()?, p)?;
        if !pageshape::shaped(&rel) {
            return Ok(format!("{rel} isn't a wiki entity or concept page: only those have the page shape."));
        }
        let text = read_text(ctx, &rel)?;
        let files = pageshape::vault_files(&ctx.vault);
        let (r, _) = pageshape::one(&rel, &text, &pageshape::Sources::from_paths(files.iter().map(String::as_str)));
        if r.in_shape {
            return Ok(format!("{rel} is in the page shape ({} Timeline entries).", r.report.entries));
        }
        let rp = &r.report;
        let mut out = vec![if !rp.broken.is_empty() {
            format!("{rel} can't be reshaped by script: {}.", rp.broken.join("; "))
        } else if r.auto {
            format!("{rel} isn't in the page shape; reshape_pages can reshape it by itself ({} Timeline entries).", rp.entries)
        } else {
            format!("{rel} isn't in the page shape and needs the user first: {}.", rp.reasons.join("; "))
        }];
        if !rp.headings.is_empty() {
            out.push("Headings it would rewrite:".into());
            out.extend(rp.headings.iter().take(40).map(|(o, n)| format!("- {o}  →  {n}")));
            if rp.headings.len() > 40 {
                out.push(format!("- …and {} more", rp.headings.len() - 40));
            }
        }
        if !rp.added.is_empty() {
            out.push("Lines it would add:".into());
            out.extend(rp.added.iter().take(40).map(|l| format!("- {l}")));
            if rp.added.len() > 40 {
                out.push(format!("- …and {} more", rp.added.len() - 40));
            }
        }
        if !rp.removed.is_empty() {
            out.push("Lines it would drop (repeated in a merged See also):".into());
            out.extend(rp.removed.iter().map(|l| format!("- {l}")));
        }
        out.extend(rp.notes.iter().map(|n| format!("Note: {n}")));
        return Ok(out.join("\n"));
    }
    let all = pageshape::survey(&ctx.vault);
    let out_of: Vec<_> = all.iter().map(|x| &x.0).filter(|r| !r.in_shape).collect();
    if out_of.is_empty() {
        return Ok(format!("All {} wiki entity and concept pages are in the page shape.", all.len()));
    }
    let auto: Vec<&str> = out_of.iter().filter(|r| r.auto).map(|r| r.path.as_str()).collect();
    let mut out = vec![format!(
        "{} of {} pages aren't in the page shape: {} can be reshaped by itself (reshape_pages), {} need the user first.",
        out_of.len(),
        all.len(),
        auto.len(),
        out_of.len() - auto.len()
    )];
    // Every page out of the shape, those that need the user first; give page for one's detail.
    let why = |r: &pageshape::PageResult| {
        if r.auto {
            "reshape_pages can do it by itself".to_string()
        } else if r.report.broken.is_empty() {
            r.report.reasons.join("; ")
        } else {
            format!("can't be reshaped: {}", r.report.broken.join("; "))
        }
    };
    let mut rows = out_of.clone();
    rows.sort_by_key(|r| (r.auto, r.path.clone()));
    let (shown, head) = page_of(&rows, |r| format!("{} {}", r.path, why(r)), paging, ("page", "pages"), 50);
    out.push(format!("Those that need the user first, then the rest: {head}:"));
    out.extend(shown.into_iter().map(|r| format!("- {}: {}", r.path, why(r))));
    Ok(out.join("\n"))
}

fn lint_tool(ctx: &Ctx, a: LintArgs) -> Result<String, String> {
    let only = match a.page.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(p) => Some(existing(ctx, &ctx.index()?, p)?),
        None => None,
    };
    let r = lint::run(&ctx.vault, ctx.today, &Default::default());
    let mut out = String::new();
    for c in &r.checks {
        let items: Vec<&lint::Item> =
            c.items.iter().filter(|i| only.as_ref().is_none_or(|p| i.page.as_ref() == Some(p) || i.pages.contains(p))).collect();
        if items.is_empty() {
            continue;
        }
        let counted = if lint::ADVISORY.contains(&c.id) { ", not counted: only worth a look" } else { "" };
        out.push_str(&format!("\n{} ({}{counted}; check {}):\n", c.title, items.len(), c.id));
        for i in items.iter().take(40) {
            // Page shape's items are reshape_pages', not fix_health's, as the app's report has them.
            let safe = i.safe && c.id != "page-shape";
            out.push_str(&format!("- {}{}\n", i.text, if safe { " [safe fix]" } else { "" }));
        }
        if items.len() > 40 {
            out.push_str(&format!("- …and {} more\n", items.len() - 40));
        }
    }
    Ok(if out.is_empty() {
        "No issues.".into()
    } else {
        format!("Knowledge health{}:\n{out}", only.map(|p| format!(" for {p}")).unwrap_or_default())
    })
}

#[derive(Deserialize)]
struct ProposeArgs {
    page: String,
    title: String,
    #[serde(default)]
    reason: String,
    section: Option<String>,
    content: Option<String>,
    edits: Option<Vec<proposals::Edit>>,
    #[serde(default)]
    quotes: Vec<QuoteArg>,
    #[serde(default)]
    unattended: bool,
}

#[derive(Deserialize)]
struct QuoteArg {
    source: String,
    anchor: Option<String>,
    text: String,
}

/// Where this session's changes come from: the Ask chat that started it, or this terminal session;
/// scheduled when the call says nobody is watching.
fn origin(ctx: &Ctx, unattended: bool) -> Origin {
    Origin {
        kind: "chat".into(),
        chat: ctx.chat.clone(),
        label: ctx.chat.is_none().then(|| "Terminal session".into()),
        run: Some(ctx.chat.clone().unwrap_or_else(|| ctx.session.clone())),
        trigger: unattended.then(|| "scheduled".into()),
    }
}

fn edit_page(ctx: &Ctx, a: ProposeArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let patch = match (a.section, a.content, a.edits) {
        (Some(section), Some(content), None) => Patch::Section { section, content },
        (None, None, Some(edits)) => Patch::Replace { edits },
        (None, Some(content), None) => Patch::Content { content },
        _ => return Err("Give exactly one of: section and content, edits, or content alone.".into()),
    };
    let (rel, before) = match find_page(ctx, &ix, &a.page)? {
        PageRef::Found(rel) => {
            let text = read_text(ctx, &rel)?;
            (rel, Some(text))
        }
        PageRef::NotThere(rel) => {
            proposals::valid_new_page(&rel)?;
            if !matches!(patch, Patch::Content { .. }) {
                return Err(format!("{rel} doesn't exist yet: give its whole text in content."));
            }
            (rel, None)
        }
    };
    // Checked here, so the model hears at once what's wrong; the app runs it again on the page as
    // it is when it's made.
    let mut after = match &patch {
        // A heading the page doesn't have goes where the page shape puts it.
        Patch::Section { section, content } => {
            brainstead_core::pageshape::with_section(&rel, before.as_deref().unwrap_or(""), section, content)?
        }
        _ => proposals::patched(before.as_deref().unwrap_or(""), &patch)?,
    };
    if let Some(b) = &before {
        after = proposals::bump_updated(b, &after, &ctx.today.to_string());
        if after.replace("\r\n", "\n") == b.replace("\r\n", "\n") {
            return Err("That changes nothing in the page.".into());
        }
        if brainstead_core::frontmatter::split(&after).error.is_some() && brainstead_core::frontmatter::split(b).error.is_none() {
            return Err("The page's properties wouldn't read any more: check the frontmatter you wrote.".into());
        }
    }
    let mut quotes: Vec<Quote> =
        a.quotes.into_iter().map(|q| Quote { source: q.source, anchor: q.anchor, text: q.text, checked: None, path: None }).collect();
    let problems = proposals::mark_quotes(&mut quotes, |s| source_text(ctx, &ix, s))?;
    let resolves =
        |t: &str| ix.resolve(&[t.to_string()]).ok().and_then(|v| v.into_iter().next().flatten()).is_some() || ctx.vault.join(t).exists();
    let ghosts = proposals::new_ghost_links(before.as_deref().unwrap_or(""), &after, resolves);
    let warnings: Vec<String> = ghosts.iter().map(|g| format!("[[{g}]] goes to no page")).collect();
    let (kind, instruction) = match &before {
        Some(b) => (
            Kind::Edit,
            match patch {
                Patch::Section { section, content } => Instruction::Section { section, content },
                Patch::Replace { edits } => Instruction::Replace { edits },
                // The whole page given: kept as the changes it makes, so it doesn't write over what
                // changed meanwhile.
                Patch::Content { .. } => changes::from_texts(Some(b), &after),
            },
        ),
        None => (Kind::New, Instruction::Page { content: after.clone() }),
    };
    // `updated:` moved too: the section or edits alone wouldn't, so the change is the texts'.
    let instruction = match (&before, &instruction) {
        (Some(b), Instruction::Section { .. } | Instruction::Replace { .. })
            if instruction.text(&rel, Some(b)).ok().as_deref() != Some(after.as_str()) =>
        {
            changes::from_texts(Some(b), &after)
        }
        _ => instruction,
    };
    let mut c = change(ctx, &rel, kind, &a.title, &a.reason, a.unattended, instruction);
    c["quotes"] = serde_json::to_value(&quotes).unwrap_or_default();
    c["warnings"] = json!(warnings);
    // A quote that isn't in its source is marked on the quote; the app's checks flag it, once.
    let mut notes: Vec<String> = problems.iter().map(|p| format!("Note: {p}")).collect();
    let unchecked = quotes.iter().filter(|q| q.checked.is_none()).count();
    if unchecked > 0 {
        notes.push(format!("{unchecked} quote(s) couldn't be checked: Brainstead has no text for their source."));
    }
    notes.extend(warnings.iter().map(|w| format!("Note: {w}.")));
    send(ctx, c, notes)
}

/// A quote's source: its path, and its text when Brainstead reads it.
fn source_text(ctx: &Ctx, ix: &Index, source: &str) -> (Option<String>, SourceText) {
    match find_page(ctx, ix, source) {
        Ok(PageRef::Found(rel)) => {
            let t = if is_text(&rel) {
                read_text(ctx, &rel).map(SourceText::Text).unwrap_or(SourceText::Missing)
            } else {
                match extracted(ctx, ix, &rel) {
                    Some(x) => SourceText::Parts(x),
                    None => SourceText::Unchecked,
                }
            };
            (Some(rel), t)
        }
        _ => (None, SourceText::Missing),
    }
}

/// A PDF or Office file's text, as the app indexed it; None when it hasn't (or can't).
fn extracted(ctx: &Ctx, ix: &Index, rel: &str) -> Option<brainstead_core::extract::Extracted> {
    ix.extracted(rel, &ctx.data.join("text")).ok().flatten().filter(|x| !x.parts.is_empty())
}

#[derive(Deserialize)]
struct TaskArgs {
    text: String,
    #[serde(default)]
    view: Option<String>,
    project: Option<String>,
    #[serde(default)]
    contexts: Vec<String>,
    effort: Option<String>,
    due: Option<String>,
    /// Deferred until, as edit_task names it.
    defer: Option<String>,
    start: Option<String>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    unattended: bool,
}

fn create_task(ctx: &Ctx, a: TaskArgs) -> Result<String, String> {
    let ix = ctx.index()?;
    let text = a.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim_start_matches("- [ ]").trim().to_string();
    if text.is_empty() {
        return Err("The task has no text.".into());
    }
    // The list's tag, as New task in that list adds it on the Tasks screen (taskModel's VIEWS).
    let tag = match a.view.as_deref().map(str::trim).unwrap_or("") {
        "" => None,
        "followups" => Some("#followup"),
        "waiting" => Some("#waiting-for"),
        "someday" => Some("#someday-maybe"),
        v => return Err(format!("view is followups, waiting or someday (Next actions when left out), not {v}.")),
    };
    let mut line = match tag {
        Some(tag) if !text.split_whitespace().any(|w| w.eq_ignore_ascii_case(tag)) => format!("- [ ] {text} {tag}"),
        _ => format!("- [ ] {text}"),
    };
    let contexts: Vec<String> = a.contexts.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
    if !contexts.is_empty() {
        line = write::with_contexts(&line, &contexts).map_err(|e| e.to_string())?;
    }
    if let Some(e) = a.effort.as_deref().filter(|e| !e.trim().is_empty()) {
        line = write::with_effort(&line, Some(e)).map_err(|e| e.to_string())?;
    }
    // Its dates, as edit_task sets them: due, deferred until and starts.
    for (d, kind) in [(&a.due, write::DateKind::Due), (&a.defer, write::DateKind::Scheduled), (&a.start, write::DateKind::Start)] {
        if let Some(d) = d.as_deref().filter(|d| !d.trim().is_empty()) {
            line = write::with_date(&line, kind, Some(d.trim())).map_err(|e| e.to_string())?;
        }
    }
    let dest = match a.project.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(p) => {
            let name = bare(p);
            let cands = [name.clone(), format!("{}{name}", projects::PREFIX)];
            let found = ix.resolve(&cands)?.into_iter().flatten().find(|r| projects::is_project_path(r));
            found.ok_or_else(|| format!("There's no project called {name}."))?
        }
        None => TODO_LIST.to_string(),
    };
    read_text(ctx, &dest).map_err(|_| format!("{dest} isn't in the vault."))?;
    let c = change(ctx, &dest, Kind::Task, &format!("Add “{text}”"), &a.reason, a.unattended, Instruction::AddTask { line, heading: None });
    send(ctx, c, vec![])
}

/// Runs the server on stdin and stdout; the process's exit code.
pub fn main_with(args: &[String], default_data: PathBuf) -> i32 {
    let ctx = match Ctx::from_args(args, default_data) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("brainstead mcp: {e}");
            return 1;
        }
    };
    #[cfg(unix)]
    let served = {
        let mut ctx = ctx;
        if let Some(r) = reload::Resume::take() {
            r.apply(&mut ctx);
        }
        serve_stdio(&ctx)
    };
    #[cfg(not(unix))]
    let served = serve(&ctx, std::io::stdin().lock(), std::io::stdout().lock());
    match served {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("brainstead mcp: {e}");
            1
        }
    }
}

#[cfg(unix)]
pub mod reload;
#[cfg(test)]
mod tests;
