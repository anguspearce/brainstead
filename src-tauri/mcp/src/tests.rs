// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The server against a temp copy of the fixture vault, indexed as the app indexes it.

use super::*;

struct Fixture {
    _tmp: tempfile::TempDir,
    ctx: Ctx,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let vault = tmp.path().join("vault");
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault");
    for e in walkdir::WalkDir::new(&from) {
        let e = e.unwrap();
        let dest = vault.join(e.path().strip_prefix(&from).unwrap());
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
        } else {
            std::fs::copy(e.path(), &dest).unwrap();
        }
    }
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), serde_json::to_vec(&json!({"vaultPath": vault})).unwrap()).unwrap();
    let mut ix = Index::open(&data.join("index.db")).unwrap();
    ix.sync(&brainstead_core::vault::Vault::new(vault.clone(), vec![])).unwrap();
    drop(ix);
    let ctx = Ctx::from_args(&["--chat".into(), "chat-1".into(), "--model".into(), "claude:sonnet".into()], data).unwrap();
    let ctx = Ctx { today: chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(), ..ctx };
    Fixture { _tmp: tmp, ctx }
}

fn rpc(ctx: &Ctx, id: i64, method: &str, params: Value) -> Value {
    handle(ctx, json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap()
}

/// A tool's text, and whether it was an error.
fn tool(ctx: &Ctx, name: &str, args: Value) -> (String, bool) {
    let r = rpc(ctx, 1, "tools/call", json!({"name": name, "arguments": args}));
    let res = &r["result"];
    (res["content"][0]["text"].as_str().unwrap().to_string(), res["isError"].as_bool().unwrap_or(false))
}

fn ok(ctx: &Ctx, name: &str, args: Value) -> String {
    let (t, err) = tool(ctx, name, args);
    assert!(!err, "{name}: {t}");
    t
}

fn refused(ctx: &Ctx, name: &str, args: Value) -> String {
    let (t, err) = tool(ctx, name, args);
    assert!(err, "{name} should have been refused: {t}");
    t
}

#[test]
fn speaks_the_protocol() {
    let f = fixture();
    let init = rpc(&f.ctx, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "brainstead");
    assert_eq!(init["result"]["capabilities"]["tools"]["listChanged"], true);
    // Unknown versions get the newest.
    let init = rpc(&f.ctx, 2, "initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(init["result"]["protocolVersion"], VERSIONS[0]);
    // Notifications get no answer.
    assert!(handle(&f.ctx, json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
    let list = rpc(&f.ctx, 3, "tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, brainstead_core::ask::MCP_TOOLS, "core's list (for Copilot) must match");
    // Every tool has a title and all four hints; reads say so.
    for t in list["result"]["tools"].as_array().unwrap() {
        let h = &t["annotations"];
        assert!(t["title"].is_string() && h["title"].is_string(), "{}", t["name"]);
        for k in ["readOnlyHint", "destructiveHint", "idempotentHint", "openWorldHint"] {
            assert!(h[k].is_boolean(), "{} has no {k}", t["name"]);
        }
    }
    let hint =
        |n: &str, k: &str| list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == n).unwrap()["annotations"][k].clone();
    assert_eq!(hint("search", "readOnlyHint"), true);
    assert_eq!(hint("edit_task", "readOnlyHint"), false);
    assert_eq!(hint("changes", "destructiveHint"), true);
    assert_eq!(hint("list_changes", "readOnlyHint"), true);
    assert_eq!((hint("stop_run", "destructiveHint"), hint("stop_run", "idempotentHint")), (json!(true), json!(true)));
    // Every list_* tool only reads.
    for t in list["result"]["tools"].as_array().unwrap() {
        if t["name"].as_str().unwrap().starts_with("list_") {
            assert_eq!(t["annotations"]["readOnlyHint"], true, "{}", t["name"]);
        }
    }
    assert_eq!(hint("edit_page", "destructiveHint"), false);
    assert_eq!(hint("start_run", "openWorldHint"), true);
    // An unknown tool is a protocol error; arguments that don't fit, and a tool that runs and
    // can't do it, say so in the result, for the model to correct its call.
    let r = rpc(&f.ctx, 8, "tools/call", json!({"name": "nope", "arguments": {}}));
    assert_eq!(r["error"]["code"], -32602);
    let r = rpc(&f.ctx, 9, "tools/call", json!({"name": "search", "arguments": {"query": 3}}));
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("inputSchema"));
    let r = rpc(&f.ctx, 10, "tools/call", json!({"name": "read_section", "arguments": {"page": "Nowhere at all"}}));
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(rpc(&f.ctx, 4, "nope", json!({}))["error"]["code"], -32601);
    // Over stdio: one line in, one line out.
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
    let mut out = Vec::new();
    serve(&f.ctx, &input[..], &mut out).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}\n");
}

/// A new build takes over the session: the same session for Changes, still unattended, and the
/// client told to fetch the tools again rather than initialize.
#[cfg(unix)]
#[test]
fn hands_the_session_to_a_new_build() {
    let f = fixture();
    assert!(resumed(&f.ctx).is_none(), "nothing to tell before initialize");
    rpc(&f.ctx, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {"roots": {}}, "clientInfo": {"name": "t"}}));
    f.ctx.unattended.store(true, std::sync::atomic::Ordering::SeqCst);
    let handed = serde_json::to_string(&reload::Resume::of(&f.ctx)).unwrap();

    let mut next = fixture();
    assert_ne!(next.ctx.session, f.ctx.session);
    serde_json::from_str::<reload::Resume>(&handed).unwrap().apply(&mut next.ctx);
    assert_eq!(next.ctx.session, f.ctx.session);
    assert!(next.ctx.unattended.load(std::sync::atomic::Ordering::SeqCst));
    let init = next.ctx.init.lock().unwrap().clone().unwrap();
    assert_eq!((init["protocolVersion"].as_str(), init["clientInfo"]["name"].as_str()), (Some("2025-03-26"), Some("t")));
    assert_eq!(resumed(&next.ctx).unwrap()["method"], "notifications/tools/list_changed");
}

#[test]
fn reading_tools() {
    let f = fixture();
    let s = ok(&f.ctx, "search", json!({"query": "soft launch"}));
    assert!(s.contains("wiki/entities/Orbit App.md"), "{s}");
    let only_sources = ok(&f.ctx, "search", json!({"query": "orbit", "layers": ["sources"]}));
    assert!(!only_sources.contains("wiki/"), "{only_sources}");
    assert!(refused(&f.ctx, "search", json!({"query": "x", "layers": ["mail"]})).contains("Not a part"));

    let sec = ok(&f.ctx, "read_section", json!({"page": "[[Orbit App]]", "heading": "Current state"}));
    assert!(sec.contains("Soft launch to staff") && !sec.contains("Pilot ran"), "{sec}");
    let missing = refused(&f.ctx, "read_section", json!({"page": "Orbit App", "heading": "Risks"}));
    assert!(missing.contains("Current state") && missing.contains("History"));
    assert!(refused(&f.ctx, "read_section", json!({"page": "No Such Page"})).contains("no page called"));

    let bl = ok(&f.ctx, "backlinks", json!({"page": "Orbit App"}));
    assert!(bl.contains("wiki/concepts/Hub Platform.md"), "{bl}");
    // A page at a time: how many, which are shown and the next offset.
    let n = bl.lines().filter(|l| l.starts_with("- ")).count();
    assert!(n > 1, "{bl}");
    let one = ok(&f.ctx, "backlinks", json!({"page": "Orbit App", "limit": 1}));
    assert!(one.contains(&format!("{n} links; 1–1 shown, offset 1 for the next:")), "{one}");
    assert_eq!(one.lines().filter(|l| l.starts_with("- ")).count(), 1);
    let last = ok(&f.ctx, "backlinks", json!({"page": "Orbit App", "limit": 1, "offset": n - 1}));
    assert!(last.contains(&format!("{n}–{n} shown:")), "{last}");
    let hub = ok(&f.ctx, "backlinks", json!({"page": "Orbit App", "query": "hub platform"}));
    assert!(hub.contains("matching “hub platform”") && hub.contains("Hub Platform.md"), "{hub}");
    let shape = ok(&f.ctx, "page_shape", json!({"limit": 1}));
    assert!(shape.contains("Those that need the user first, then the rest:"), "{shape}");
    // Every listing tool takes the same paging.
    let list = rpc(&f.ctx, 1, "tools/list", json!({}));
    for t in list["result"]["tools"].as_array().unwrap() {
        let paged = t["inputSchema"]["properties"]["offset"].is_object();
        assert_eq!(paged, PAGED.contains(&t["name"].as_str().unwrap()), "{}", t["name"]);
    }

    assert!(ok(&f.ctx, "resolve_entity", json!({"name": "OA"})).contains("by an alias"));
    assert!(ok(&f.ctx, "resolve_entity", json!({"name": "Orbit App"})).contains("by its file name"));
    assert!(ok(&f.ctx, "resolve_entity", json!({"name": "Orbit Ap"})).contains("wiki/entities/Orbit App.md"));

    let pending = ok(&f.ctx, "pending_sources", json!({}));
    assert!(pending.contains("- sources/Programme Update Steerco 2026-09-28.pdf · New"), "{pending}");
    assert!(pending.contains("still to ingest (New or Changed)"), "{pending}");
    // The Sources screen's Status: an ingested one only with all or ingested.
    let all = ok(&f.ctx, "pending_sources", json!({"status": "all"}));
    let ingested = ok(&f.ctx, "pending_sources", json!({"status": "ingested"}));
    assert!(all.lines().count() > pending.lines().count(), "{all}");
    assert!(ingested.lines().skip(1).all(|l| l.ends_with("· Ingested")) && ingested.lines().count() > 1, "{ingested}");
    assert!(refused(&f.ctx, "pending_sources", json!({"status": "pending"})).contains("status is all"));
    // Images are ingested too (their text read from the picture), so they're listed.
    assert!(pending.contains("sources/Whiteboard photo.png"), "{pending}");
    // A file ingest can't read isn't one to ingest, as the Sources screen gives it no Ingest; all marks it.
    std::fs::write(f.ctx.vault.join("sources/Diagram.svg"), "<svg/>").unwrap();
    std::fs::write(f.ctx.vault.join("sources/Bundle.zip"), "PK").unwrap();
    let mut ix = Index::open(&f.ctx.data.join("index.db")).unwrap();
    ix.sync(&brainstead_core::vault::Vault::new(f.ctx.vault.clone(), vec![])).unwrap();
    drop(ix);
    let pending = ok(&f.ctx, "pending_sources", json!({}));
    assert!(!pending.contains("Diagram.svg") && !pending.contains("Bundle.zip"), "{pending}");
    assert!(!ok(&f.ctx, "pending_sources", json!({"status": "new"})).contains("Bundle.zip"));
    let all = ok(&f.ctx, "pending_sources", json!({"status": "all"}));
    assert!(all.contains("- sources/Bundle.zip · New · can't be ingested"), "{all}");
    let lint = ok(&f.ctx, "lint", json!({"page": "Hub Platform"}));
    assert!(lint.contains("[[Missing Concept]]"), "{lint}");
    // Each check names its id; the grey ones say they aren't counted, and safe fixes are marked.
    let all = ok(&f.ctx, "lint", json!({}));
    assert!(all.contains("; check missing-pages)"), "{all}");
    assert!(all.lines().any(|l| l.contains("[safe fix]")), "{all}");
    assert!(all.lines().filter(|l| l.contains("check stale-pages")).all(|l| l.contains("not counted")), "{all}");
    // Page shape's items are reshape_pages', so none is a safe fix, as in the app's report.
    let shape = all.split("\n\n").find(|s| s.contains("; check page-shape)")).expect("a page shape check in the fixture");
    assert!(shape.lines().count() > 1 && !shape.contains("[safe fix]"), "{shape}");
}

#[test]
fn lint_asks_the_app_when_its_open() {
    let f = fixture();
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("No issues."), error: None });
    assert_eq!(ok(&f.ctx, "lint", json!({})), "No issues.");
    assert_eq!(app.join().unwrap().action, "health.lint");
}

#[test]
fn create_template_is_a_new_file_in_templates() {
    let f = fixture();
    let (_, c) = submitted(&f, "create_template", json!({"name": "Retro", "content": "# <% tp.file.title %>\n\n## What went well"}));
    assert_eq!((c["kind"].as_str(), c["page"].as_str()), (Some("new"), Some("Templates/Retro.md")));
    assert!(c["instruction"]["content"].as_str().unwrap().ends_with("went well\n"));
    assert!(refused(&f.ctx, "create_template", json!({"name": "Meeting", "content": "x"})).contains("exists already"));
    assert!(refused(&f.ctx, "create_template", json!({"name": "a/b", "content": "x"})).contains("without a folder"));
    assert!(refused(&f.ctx, "create_template", json!({"name": "x", "content": " "})).contains("content"));
}

#[test]
fn example_notes_are_added_as_new_notes_only_where_missing() {
    let f = fixture();
    let now = f.ctx.today.and_time(chrono::Local::now().time());
    let all = brainstead_core::starter::files(now);
    // Every example note there but Start here: only that one is sent, as a new note.
    for (rel, text) in &all {
        let abs = f.ctx.vault.join(rel);
        if rel != "Start here.md" && !abs.exists() {
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(abs, text).unwrap();
        }
    }
    let (t, c) = submitted(&f, "add_example_notes", json!({}));
    assert!(t.starts_with("1 example note:\n- Start here.md: Changed it."), "{t}");
    assert_eq!((c["kind"].as_str(), c["page"].as_str()), (Some("new"), Some("Start here.md")));
    std::fs::write(f.ctx.vault.join("Start here.md"), "# Start here\n").unwrap();
    assert!(refused(&f.ctx, "add_example_notes", json!({})).contains("all there already"));
}

/// Runs a change tool against a stand-in app; its reply, and the change the app was sent.
#[test]
fn facts_from_kept_claims() {
    let f = fixture();
    let page = "wiki/entities/Orbit App.md";
    assert!(ok(&f.ctx, "facts", json!({"page": "Orbit App"})).starts_with("No kept facts match on wiki/entities/Orbit App.md"));
    let kept = |value: &str, as_of: &str, source: &str| brainstead_core::claims::Kept {
        subject: "Orbit App".into(),
        attribute: "go_live_date".into(),
        value: value.into(),
        as_of: Some(as_of.into()),
        quote: format!("launch {value}"),
        source: source.into(),
        recorded: "2026-10-02".into(),
        ..Default::default()
    };
    let one = brainstead_core::claims::with_source(None, page, "a.md", &[kept("14 October", "2026-09-01", "a.md")]);
    let two = brainstead_core::claims::with_source(one.as_deref(), page, "b.md", &[kept("28 November", "2026-09-28", "b.md")]).unwrap();
    let file = f.ctx.vault.join(brainstead_core::claims::path(page).unwrap());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, two).unwrap();
    let t = ok(&f.ctx, "facts", json!({"subject": "orbit app", "attribute": "launch date"}));
    let latest = t.lines().find(|l| l.starts_with("- Orbit App — go_live_date")).unwrap();
    assert!(latest.contains("28 November (as of 2026-09-28)"), "{t}");
    assert!(t.contains("superseded: 14 October"), "{t}");
}

fn submitted(f: &Fixture, name: &str, args: Value) -> (String, Value) {
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Changed it. Revert it in Changes."), error: None });
    let t = ok(&f.ctx, name, args);
    let req = app.join().unwrap();
    assert_eq!(req.action, "change.submit");
    (t, req.args["change"].clone())
}

/// The page's text once the change's instruction is run on it as it is.
fn made(f: &Fixture, c: &Value) -> String {
    let i: Instruction = serde_json::from_value(c["instruction"].clone()).unwrap();
    let page = c["page"].as_str().unwrap();
    let now = std::fs::read_to_string(f.ctx.vault.join(page)).ok();
    i.text(page, now.as_deref()).unwrap()
}

#[test]
fn edit_page_sends_a_checked_change() {
    let f = fixture();
    let page = f.ctx.vault.join("wiki/entities/Orbit App.md");
    let before = std::fs::read_to_string(&page).unwrap();
    let (t, c) = submitted(
        &f,
        "edit_page",
        json!({
            "page": "Orbit App",
            "title": "Launch date from the steerco",
            "section": "Current state",
            "content": "Soft launch to staff is planned for 28 November ([[Roadmap Update 2026-09-18]]).",
            "quotes": [{"source": "Roadmap Update 2026-09-18", "text": "Staff soft launch remains on track"}]
        }),
    );
    assert!(t.contains("Revert it in Changes"), "{t}");
    // The server writes nothing itself: the app makes the change.
    assert_eq!(std::fs::read_to_string(&page).unwrap(), before);
    assert_eq!(c["page"], "wiki/entities/Orbit App.md");
    assert_eq!(c["kind"], "edit");
    assert_eq!(c["origin"]["chat"], "chat-1");
    assert_eq!(c["origin"]["run"], "chat-1");
    assert!(c["origin"]["trigger"].is_null());
    assert_eq!(c["model"], "claude:sonnet");
    assert_eq!(c["quotes"][0]["checked"], true);
    assert!(made(&f, &c).contains("planned for 28 November"));
    // unattended marks it scheduled.
    let (_, c) = submitted(
        &f,
        "edit_page",
        json!({"page": "Orbit App", "title": "x", "edits": [{"find": "Soft launch to staff", "replace": "Staff soft launch"}], "unattended": true}),
    );
    assert_eq!(c["origin"]["trigger"], "scheduled");
}

#[test]
fn edit_page_flags_a_quote_and_refuses_what_cant_be_done() {
    let f = fixture();
    // A quote that isn't in its source: still sent, flagged.
    let (t, c) = submitted(
        &f,
        "edit_page",
        json!({"page": "Orbit App", "title": "x", "edits": [{"find": "14 November", "replace": "28 November"}],
               "quotes": [{"source": "Roadmap Update 2026-09-18", "text": "launch moved to 28 November"}]}),
    );
    // Marked on the quote, which the app's checks flag: not flagged a second time here.
    assert!(t.contains("isn't in") && c.get("flags").is_none(), "{t} {c}");
    assert_eq!(c["quotes"][0]["checked"], false);
    assert!(refused(&f.ctx, "edit_page", json!({"page": "Orbit App", "title": "x", "edits": [{"find": "Nope", "replace": "y"}]}))
        .contains("isn't in the page"));
    assert!(refused(&f.ctx, "edit_page", json!({"page": "Orbit App", "title": "x"})).contains("exactly one"));
    assert!(refused(&f.ctx, "edit_page", json!({"page": "Orbit App", "title": "x", "section": "A", "content": "b", "edits": []}))
        .contains("exactly one"));
    assert!(refused(&f.ctx, "edit_page", json!({"page": "notes/New.md", "title": "x", "content": "# New\n"}))
        .contains("A new note is create_note"));
    assert!(refused(&f.ctx, "edit_page", json!({"page": "wiki/entities/New.md", "title": "x", "section": "A", "content": "b"}))
        .contains("whole text"));
    assert!(refused(&f.ctx, "edit_page", json!({"page": "../outside.md", "title": "x", "content": "y"})).contains("Not a path"));
    let broken = refused(
        &f.ctx,
        "edit_page",
        json!({"page": "Orbit App", "title": "x", "edits": [{"find": "type: entity", "replace": "type: [entity"}]}),
    );
    assert!(broken.contains("properties"), "{broken}");
}

#[test]
fn new_pages_and_unchecked_quotes() {
    let f = fixture();
    let (t, c) = submitted(
        &f,
        "edit_page",
        json!({"page": "wiki/entities/Northwind.md", "title": "New supplier page",
               "content": "---\ntype: entity\nname: Northwind\n---\n\n## Current state\n\nSupplies hardware; see [[Orbit App]] and [[Ghost Page]].\n",
               "quotes": [{"source": "Programme Update Steerco 2026-09-28.pdf", "anchor": "page=2", "text": "Northwind"}]}),
    );
    assert!(t.contains("couldn't be checked") && t.contains("[[Ghost Page]] goes to no page"), "{t}");
    assert_eq!(c["kind"], "new");
    assert_eq!(c["instruction"]["op"], "page");
    assert!(c["quotes"][0]["checked"].is_null());
    assert_eq!(c["warnings"], json!(["[[Ghost Page]] goes to no page"]));
}

#[test]
fn create_task_sends_a_line() {
    let f = fixture();
    let (t, c) = submitted(
        &f,
        "create_task",
        json!({"text": "Call Northwind about the pilot", "contexts": ["calls", "office"], "effort": "15m", "due": "2026-10-09"}),
    );
    assert!(t.contains("Revert"), "{t}");
    assert_eq!((c["page"].as_str(), c["kind"].as_str()), (Some("Me. To Do List.md"), Some("task")));
    assert_eq!(
        c["instruction"],
        json!({"op": "add_task", "line": "- [ ] Call Northwind about the pilot #context/calls #context/office [effort:: 15m] 📅 2026-10-09"})
    );
    let (_, c) = submitted(&f, "create_task", json!({"text": "Book the venue", "project": "Orbit App launch"}));
    assert_eq!(c["page"], "Project. Orbit App launch.md");
    assert!(made(&f, &c).contains("- [ ] Book the venue"));
    // In Follow-ups, as New task there does: with its tag, before the fields.
    let (_, c) = submitted(&f, "create_task", json!({"text": "Ask Lena about the venue", "view": "followups", "due": "2026-10-09"}));
    assert_eq!(c["instruction"]["line"], "- [ ] Ask Lena about the venue #followup 📅 2026-10-09");
    // Deferred until and starts, as edit_task sets them.
    let (_, c) = submitted(&f, "create_task", json!({"text": "Book the bus", "defer": "2026-10-12", "start": "2026-10-14"}));
    assert_eq!(c["instruction"]["line"], "- [ ] Book the bus ⏳ 2026-10-12 🛫 2026-10-14");
    assert!(refused(&f.ctx, "create_task", json!({"text": "x", "view": "today"})).contains("view is followups"));
    assert!(refused(&f.ctx, "create_task", json!({"text": "x", "project": "Nowhere"})).contains("no project"));
    assert!(refused(&f.ctx, "create_task", json!({"text": "x", "effort": "lots"})).contains("Not an effort"));
    // Its description says where a task with no project lands: the Inbox, to clarify.
    let list = tools();
    let d = list.iter().find(|t| t["name"] == "create_task").unwrap()["description"].as_str().unwrap().to_string();
    assert!(d.contains("which is the Inbox"), "{d}");
}

#[test]
fn needs_an_index() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("settings.json"), r#"{"vaultPath": "/nowhere"}"#).unwrap();
    let ctx = Ctx::from_args(&[], tmp.path().to_path_buf()).unwrap();
    assert!(refused(&ctx, "search", json!({"query": "x"})).contains("open Brainstead"));
    assert!(Ctx::from_args(&["--data".into(), tmp.path().join("none").to_string_lossy().into_owned()], PathBuf::new()).is_err());
}

#[test]
fn create_note_sends_a_named_note() {
    let f = fixture();
    let (t, c) = submitted(
        &f,
        "create_note",
        json!({"type": "Meeting", "title": "Orbit App pilot review", "date": "2026-10-03", "content": "# Orbit App pilot review\n\n## Notes\n- Lena: the pilot went well."}),
    );
    assert!(t.contains("Revert"), "{t}");
    assert_eq!((c["kind"].as_str(), c["page"].as_str()), (Some("new"), Some("Meeting. Orbit App pilot review - 2026-10-03.md")));
    assert!(c["instruction"]["content"].as_str().unwrap().ends_with("went well.\n"));
    // In a folder that's there; the name's odd characters go.
    let (_, c) = submitted(&f, "create_note", json!({"type": "Idea", "title": "Victor's #1 [plan]", "folder": "Victor", "content": "x"}));
    assert_eq!(c["page"], "Victor/Idea. Victor's 1 plan.md");
    assert!(refused(&f.ctx, "create_note", json!({"title": "x", "folder": "sources", "content": "x"})).contains("not in sources/"));
    assert!(refused(&f.ctx, "create_note", json!({"title": "x", "folder": "wiki/entities", "content": "x"})).contains("edit_page"));
    assert!(refused(&f.ctx, "create_note", json!({"title": "x", "folder": "Nowhere", "content": "x"})).contains("no folder"));
    refused(&f.ctx, "create_note", json!({"title": "x", "folder": "../out", "content": "x"}));
    assert!(refused(&f.ctx, "create_note", json!({"title": "x", "date": "3 Oct", "content": "x"})).contains("YYYY-MM-DD"));
    assert!(refused(&f.ctx, "create_note", json!({"title": "##", "content": "x"})).contains("title"));
    let exists = refused(&f.ctx, "create_note", json!({"type": "Project", "title": "Garden", "content": "x"}));
    assert!(exists.contains("exists already") && exists.contains("edit_page"), "{exists}");
    // From a template, the same checks, before the app is asked to run it.
    let tpl = |extra: Value| {
        let mut a = json!({"title": "Venue", "template": "Idea"});
        a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        refused(&f.ctx, "create_note", a)
    };
    assert!(tpl(json!({"folder": "wiki/entities"})).contains("edit_page"));
    assert!(tpl(json!({"folder": "Nowhere"})).contains("no folder"));
    tpl(json!({"folder": "../out"}));
    assert!(tpl(json!({"date": "3 Oct"})).contains("YYYY-MM-DD"));
    assert!(tpl(json!({"title": "##"})).contains("title"));
}

#[test]
fn edit_page_changes_a_note() {
    let f = fixture();
    let (_, c) = submitted(
        &f,
        "edit_page",
        json!({"page": "Idea. Accents", "title": "Add a line", "edits": [{"find": "Links to", "replace": "This links to"}]}),
    );
    assert_eq!((c["kind"].as_str(), c["page"].as_str()), (Some("edit"), Some("Idea. Accents.md")));
    assert!(made(&f, &c).contains("This links to"));
}

#[test]
fn help_answers_from_the_apps_own_help() {
    let f = fixture();
    let list = ok(&f.ctx, "help", json!({}));
    assert!(list.contains("settings-general") && list.contains("guide-start"));
    let topic = ok(&f.ctx, "help", json!({"topic": "settings-general"}));
    assert!(topic.starts_with("# Settings › General"));
    let found = ok(&f.ctx, "help", json!({"query": "change the quick capture shortcut"}));
    assert!(found.to_lowercase().contains("capture"), "{found}");
    assert!(refused(&f.ctx, "help", json!({"topic": "nope"})).contains("No help topic"));
}

#[test]
fn shortcuts_are_named_as_the_system_names_them() {
    let f = fixture();
    let list = rpc(&f.ctx, 1, "tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    let status = tools.iter().find(|t| t["name"] == "app_status").unwrap()["description"].as_str().unwrap().to_string();
    let keys = ok(&f.ctx, "help", json!({"topic": "keyboard"}));
    if cfg!(windows) {
        assert!(status.contains("what Ctrl+Z would undo"), "{status}");
        let all = list.to_string() + &keys;
        assert!(!all.contains(['⌘', '⌥', '⇧', '⌃']), "a macOS glyph left on Windows");
    } else {
        assert!(status.contains("what ⌘Z would undo"), "{status}");
        assert!(keys.contains('⌘'), "{keys}");
    }
}

#[test]
fn rate_limited() {
    let f = fixture();
    for _ in 0..120 {
        tool(&f.ctx, "help", json!({"topic": "tasks"}));
    }
    assert!(refused(&f.ctx, "help", json!({"topic": "tasks"})).contains("calls in a minute"));
}

/// A stand-in for the app: says it's up, and answers the next request with what `answer` makes.
fn fake_app(
    data: PathBuf,
    answer: impl Fn(&bridge::Request) -> bridge::Reply + Send + 'static,
) -> std::thread::JoinHandle<bridge::Request> {
    let d = bridge::dir(&data);
    std::fs::create_dir_all(&d).unwrap();
    bridge::write_alive(&d.join(bridge::ALIVE), 1).unwrap();
    std::thread::spawn(move || loop {
        if let Some((p, r)) = bridge::take_requests(&d.join(bridge::REQUESTS)).into_iter().next() {
            bridge::write_reply(&d.join(bridge::REPLIES), &r.id, &answer(&r)).unwrap();
            std::fs::remove_file(p).unwrap();
            return r;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    })
}

#[test]
fn actions_go_to_the_app() {
    let f = fixture();
    let app =
        fake_app(f.ctx.data.clone(), |r| bridge::Reply { ok: true, result: json!(format!("Ticked {}", r.args["task"])), error: None });
    let t = ok(&f.ctx, "edit_task", json!({"task": "Me. To Do List.md:12", "text": "Call Sam", "done": true}));
    let req = app.join().unwrap();
    assert_eq!(req.action, "task.edit");
    assert_eq!(req.args["done"], true);
    assert_eq!(t, "Ticked \"Me. To Do List.md:12\"");
    // The app says why it couldn't.
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply::err("There's no task at Me. To Do List.md:99 any more."));
    assert!(refused(&f.ctx, "list_tasks", json!({"view": "next"})).contains("no task"));
    app.join().unwrap();
    // Not running, and no app to open (the separate binary): it says to open it.
    std::fs::remove_file(bridge::dir(&f.ctx.data).join(bridge::ALIVE)).unwrap();
    assert!(refused(&f.ctx, "list_inbox", json!({})).contains("isn't open"));
}

#[test]
fn renames_trash_and_deletes_are_sent() {
    let f = fixture();
    let (_, c) = submitted(&f, "rename_note", json!({"page": "Idea. Accents", "type": "Idea", "title": "Accents and diacritics"}));
    assert_eq!((c["page"].as_str(), c["instruction"]["to"].as_str()), (Some("Idea. Accents.md"), Some("Idea. Accents and diacritics.md")));
    assert!(
        refused(&f.ctx, "rename_note", json!({"page": "Idea. Accents", "type": "Idea", "title": "Accents"})).contains("its name already")
    );
    assert!(
        refused(&f.ctx, "rename_note", json!({"page": "Idea. Accents", "type": "Project", "title": "Garden"})).contains("exists already")
    );
    // A source keeps its own extension, as the file menu's Rename does, given with it or not.
    for title in ["Steerco pack", "Steerco pack.pdf", "Steerco pack.PDF"] {
        let (_, c) = submitted(
            &f,
            "rename_note",
            json!({"page": "sources/Programme Update Steerco 2026-09-28.pdf", "title": title, "date": "2026-09-28"}),
        );
        assert_eq!(c["instruction"]["to"].as_str(), Some("sources/Steerco pack - 2026-09-28.pdf"), "{title}");
    }
    // preview lists the links it would rewrite, as the Rename box does, and renames nothing.
    let t = ok(
        &f.ctx,
        "rename_note",
        json!({"page": "Project. Orbit App launch", "type": "Project", "title": "Orbit App rollout", "preview": true}),
    );
    assert!(t.contains("will be updated:") && t.contains("→ ") && t.contains("[[Project. Orbit App rollout"), "{t}");
    assert!(f.ctx.vault.join("Project. Orbit App launch.md").exists());
    let (_, c) = submitted(&f, "trash_note", json!({"page": "Idea. Accents", "reason": "Old"}));
    assert_eq!((c["kind"].as_str(), c["instruction"]["op"].as_str()), (Some("trash"), Some("trash")));
    // A task's line taken out, found by its text even when the line number is off.
    let todo = std::fs::read_to_string(f.ctx.vault.join("Me. To Do List.md")).unwrap();
    let (n, line) = todo.lines().enumerate().find(|(_, l)| l.contains("Walk through the Dashboard")).unwrap();
    let (_, c) =
        submitted(&f, "delete_task", json!({"task": format!("Me. To Do List.md:{}", n + 5), "text": "Walk through the Dashboard"}));
    // Where it is now goes with it, so the app takes out that line and not another the same.
    assert_eq!(c["instruction"], json!({"op": "delete_line", "line": line, "at": n}));
    assert!(!made(&f, &c).contains("Walk through the Dashboard"));
    assert!(refused(&f.ctx, "delete_task", json!({"task": "Me. To Do List.md:1", "text": "no such task"})).contains("no task"));
}

/// The page each tool sends for `spelling`, which names Templates/Meeting.md: edit_page,
/// rename_note (and where it renames to) and trash_note.
fn sent_pages(f: &Fixture, spelling: &str) -> [String; 4] {
    let page = |c: &Value, k: &str| c[k].as_str().unwrap_or_default().to_string();
    let (_, edit) = submitted(f, "edit_page", json!({"page": spelling, "title": "x", "section": "Notes", "content": "Agenda first."}));
    let (_, rename) = submitted(f, "rename_note", json!({"page": spelling, "title": "Meeting note"}));
    let (_, trash) = submitted(f, "trash_note", json!({"page": spelling}));
    [page(&edit, "page"), page(&rename, "page"), page(&rename["instruction"], "to"), page(&trash, "page")]
}

#[test]
fn a_page_is_sent_as_the_file_system_spells_it() {
    let f = fixture();
    let want = ["Templates/Meeting.md", "Templates/Meeting.md", "Templates/Meeting note.md", "Templates/Meeting.md"].map(String::from);
    assert_eq!(sent_pages(&f, "Templates/Meeting.md"), want);
    // A system that doesn't mind case (Windows, macOS by default) names the file in any case.
    if f.ctx.vault.join("TEMPLATES/MEETING.MD").is_file() {
        assert_eq!(sent_pages(&f, "templates/meeting.md"), want);
        assert!(refused(&f.ctx, "create_note", json!({"title": "Retro", "folder": "templates", "content": "x"})).contains("can't go in"));
    }
}

#[cfg(windows)]
#[test]
fn every_windows_spelling_of_a_template_is_sent_as_its_own_path() {
    let f = fixture();
    let want = ["Templates/Meeting.md", "Templates/Meeting.md", "Templates/Meeting note.md", "Templates/Meeting.md"].map(String::from);
    let mut spellings = vec!["Templates\\Meeting.md".to_string(), "templates\\MEETING.md".into()];
    match paths::short_name(&f.ctx.vault.join("Templates")) {
        Some(short) => spellings.extend([format!("{short}/Meeting.md"), format!("{short}\\Meeting")]),
        None => eprintln!("This volume makes no 8.3 names, so the short spelling isn't tried."),
    }
    for s in &spellings {
        assert_eq!(sent_pages(&f, s), want, "{s}");
    }
    // Win32 drops a trailing dot, so these name the template too; they're refused.
    for s in ["Templates./Meeting.md", "Templates/Meeting.md.", "Templates::$INDEX_ALLOCATION/Meeting.md"] {
        for tool_name in ["edit_page", "rename_note", "trash_note"] {
            let t = refused(&f.ctx, tool_name, json!({"page": s, "title": "x", "section": "Notes", "content": "y"}));
            assert!(t.contains("Not a path in the vault"), "{tool_name} {s}: {t}");
        }
    }
    if let Some(short) = paths::short_name(&f.ctx.vault.join("Templates")) {
        assert!(refused(&f.ctx, "create_note", json!({"title": "Retro", "folder": short, "content": "x"})).contains("can't go in"));
    }
}

#[test]
fn summaries_by_day_or_the_latest() {
    let f = fixture();
    std::fs::write(
        f.ctx.vault.join("Me. Daily Summaries - 2026-10.md"),
        "> log\n\n## Daily summary 2026-10-02\n\nShipped the pilot.\n\n---\n\n## Daily summary 2026-10-01\n\nPlanned the pilot.\n",
    )
    .unwrap();
    // A week from before the rename, in the old file under the old heading.
    std::fs::write(f.ctx.vault.join("Me. Weekly Reviews - 2026-09.md"), "## Weekly review 2026-W39\n\nA quiet week.\n").unwrap();
    let latest = ok(&f.ctx, "summary", json!({}));
    assert!(
        latest.starts_with("The daily summary for 2026-10-02") && latest.contains("Shipped") && !latest.contains("Planned"),
        "{latest}"
    );
    let day = ok(&f.ctx, "summary", json!({"kind": "daily", "day": "2026-10-01"}));
    assert!(day.contains("Planned the pilot") && !day.contains("Shipped"), "{day}");
    let week = ok(&f.ctx, "summary", json!({"kind": "weekly", "day": "2026-09-23"}));
    assert!(week.contains("2026-W39") && week.contains("A quiet week"), "{week}");
    assert!(ok(&f.ctx, "summary", json!({"kind": "weekly"})).contains("A quiet week"));
    assert!(refused(&f.ctx, "summary", json!({"day": "2026-09-29"})).contains("no daily summary for 2026-09-29"));
    assert!(refused(&f.ctx, "summary", json!({"kind": "monthly"})).contains("kind is daily or weekly"));
}

#[test]
fn saving_a_chat_goes_to_the_app() {
    let f = fixture();
    let app = fake_app(f.ctx.data.clone(), |r| bridge::Reply {
        ok: true,
        result: json!(format!("{} {}", r.action, r.args["chat"])),
        error: None,
    });
    assert_eq!(ok(&f.ctx, "save_chat", json!({"chat": "Orbit App launch"})), "chat.save \"Orbit App launch\"");
    app.join().unwrap();
}

#[test]
fn the_weekly_review_goes_to_the_app() {
    let f = fixture();
    let app =
        fake_app(f.ctx.data.clone(), |r| bridge::Reply { ok: true, result: json!(format!("{} {}", r.action, r.args["id"])), error: None });
    assert_eq!(ok(&f.ctx, "weekly_suggestion", json!({"id": "s1", "action": "skip"})), "weekly.suggestion \"s1\"");
    assert_eq!(app.join().unwrap().args["action"], "skip");
    let app = fake_app(f.ctx.data.clone(), |r| bridge::Reply { ok: true, result: json!(r.action), error: None });
    assert_eq!(ok(&f.ctx, "weekly_review", json!({})), "weekly.status");
    app.join().unwrap();
}

#[test]
fn task_inbox_and_project_edits_carry_the_session() {
    let f = fixture();
    for (tool_name, action, args) in [
        ("edit_task", "task.edit", json!({"task": "Me. To Do List.md:12", "text": "Call Sam", "done": true, "unattended": true})),
        ("move_task", "task.move", json!({"task": "Me. To Do List.md:12", "text": "Call Sam", "after": "Me. To Do List.md:13"})),
        ("clarify_inbox", "inbox.clarify", json!({"item": "task:14", "becomes": "next"})),
        ("create_project", "project.create", json!({"name": "Orbit App beta"})),
        ("update_project", "project.update", json!({"project": "Orbit App launch", "status": "completed"})),
        ("rename_area", "project.rename_area", json!({"area": "Work", "to": "Acme"})),
        // A restore and a saved contradictions report are changes too, held unattended when a check fails.
        ("restore_from_trash", "trash", json!({"id": "abc-123456", "unattended": true})),
        ("contradictions", "contradictions", json!({"action": "save", "unattended": true})),
    ] {
        let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Done."), error: None });
        ok(&f.ctx, tool_name, args.clone());
        let req = app.join().unwrap();
        assert_eq!(req.action, action);
        // Recorded in Changes as this chat's, scheduled when nobody's watching.
        assert_eq!(req.args["origin"]["chat"], "chat-1", "{tool_name}");
        let unattended = args.get("unattended").is_some();
        assert_eq!(req.args["origin"]["trigger"].as_str() == Some("scheduled"), unattended, "{tool_name}");
    }
}

#[test]
fn held_changes_are_accepted_only_while_the_user_is_there() {
    let f = fixture();
    // Attended: it goes to the app, which accepts only a change held for a failed check.
    let app = fake_app(f.ctx.data.clone(), |r| bridge::Reply { ok: true, result: json!(format!("{}", r.args["action"])), error: None });
    ok(&f.ctx, "changes", json!({"action": "accept", "id": "1-1-1"}));
    assert_eq!(app.join().unwrap().args["action"], "accept");
    // Said unattended: refused, and from then on for the whole session.
    assert!(refused(&f.ctx, "changes", json!({"action": "accept", "id": "1-1-1", "unattended": true})).contains("nobody is watching"));
    assert!(refused(&f.ctx, "changes", json!({"action": "accept_run", "group": "chat-1"})).contains("nobody is watching"));
    assert!(refused(&f.ctx, "weekly_start_over", json!({})).contains("unattended"));
    assert!(refused(&f.ctx, "moving_over", json!({"action": "retire"})).contains("unattended"));
    // Reading still works.
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Nothing held."), error: None });
    ok(&f.ctx, "list_changes", json!({"action": "list"}));
    app.join().unwrap();
}

#[test]
fn every_tool_takes_unattended() {
    // The instructions ask for unattended: true on every call in a session nobody is watching, and
    // the schemas allow no other properties, so each one lists it.
    for t in tools() {
        assert_eq!(t["inputSchema"]["properties"]["unattended"]["type"], "boolean", "{}", t["name"]);
    }
    // A read tool takes it too, and it holds for the rest of the session.
    let f = fixture();
    assert!(ok(&f.ctx, "search", json!({"query": "soft launch", "unattended": true})).contains("Orbit App"));
    assert!(refused(&f.ctx, "weekly_start_over", json!({})).contains("unattended"));
}

#[test]
fn moving_over_and_starting_the_review_over_go_to_the_app() {
    let f = fixture();
    for (tool_name, action, args) in
        [("moving_over", "moving_over", json!({"action": "retire"})), ("weekly_start_over", "weekly.start_over", json!({}))]
    {
        let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Done."), error: None });
        ok(&f.ctx, tool_name, args);
        assert_eq!(app.join().unwrap().action, action);
    }
    // The checklist is a read of its own; ticking is moving_over's.
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Moving over…"), error: None });
    ok(&f.ctx, "list_moving_over", json!({"item": "otherStopped"}));
    let req = app.join().unwrap();
    assert_eq!((req.action.as_str(), req.args["action"].as_str(), req.args.get("item")), ("moving_over", Some("list"), None));
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Ticked."), error: None });
    ok(&f.ctx, "moving_over", json!({"action": "tick", "item": "otherStopped"}));
    assert_eq!(app.join().unwrap().args["item"], "otherStopped");
    assert!(refused(&f.ctx, "list_moving_over", json!({"action": "retire"})).contains("which tool does that"));
    assert!(refused(&f.ctx, "moving_over", json!({})).contains("Give action"));
}

#[test]
fn a_long_page_or_section_is_shown_in_part() {
    let page = format!(
        "# Orbit App\n\nThe staff app.\n\n## Current state\n\nLaunch 28 November.\n\n{}## Oct 2026\n\nRecon moves to 16 Oct.\n",
        (1..=60).map(|i| format!("## Sync {i}\n\n{}\n\n", "Nothing new. ".repeat(60))).collect::<String>()
    );
    let f = fit(&page, page.chars().count(), true);
    assert!(f.starts_with("[") && f.contains("shown in part"));
    assert!(f.contains("Launch 28 November.") && f.contains("Recon moves to 16 Oct.") && f.contains("## Sync 1\n[… not shown"));
    // No headings: its start and its end.
    let log = format!("Started.\n{}Latest entry.\n", "- an entry\n".repeat(5_000));
    let f = fit(&log, log.chars().count(), false);
    assert!(f.contains("Started.") && f.ends_with("Latest entry.\n") && f.chars().count() < MAX_CHARS);
}

#[test]
fn the_instructions_fit_what_clients_keep() {
    // Claude Code keeps about 2,000 characters of a server's instructions.
    assert!(INSTRUCTIONS.chars().count() < 2000, "{} characters", INSTRUCTIONS.chars().count());
}

#[test]
fn every_parameter_says_what_it_is() {
    fn walk(tool: &str, props: &Value) {
        for (k, v) in props.as_object().unwrap() {
            assert!(v["description"].is_string() || v.get("enum").is_some(), "{tool}.{k} has no description");
            if let Some(p) = v["items"].get("properties") {
                walk(tool, p);
            }
        }
    }
    for t in tools() {
        walk(t["name"].as_str().unwrap(), &t["inputSchema"]["properties"]);
        assert!(t["description"].as_str().unwrap().len() > 120, "{}'s description is too short to say when to use it", t["name"]);
    }
}

#[test]
fn listings_give_their_rows_as_data() {
    let f = fixture();
    // search, here: its files as data, matching its outputSchema's required fields.
    let r = rpc(&f.ctx, 1, "tools/call", json!({"name": "search", "arguments": {"query": "soft launch"}}));
    let s = &r["result"]["structuredContent"];
    assert!(s["total"].as_u64().unwrap() >= 1 && s["items"][0]["path"].is_string(), "{s}");
    let list = rpc(&f.ctx, 2, "tools/list", json!({}));
    let search = list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "search").unwrap().clone();
    for k in search["outputSchema"]["required"].as_array().unwrap() {
        assert!(s.get(k.as_str().unwrap()).is_some(), "search's data has no {k}");
    }
    // A short snippet unless detail.
    let short = ok(&f.ctx, "search", json!({"query": "soft launch"}));
    let long = ok(&f.ctx, "search", json!({"query": "soft launch", "detail": true}));
    assert!(short.len() <= long.len());
    // From the app: its text and data are passed on as they are.
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply {
        ok: true,
        result: json!({"text": "The Trash is empty.", "structured": {"total": 0, "items": []}}),
        error: None,
    });
    let r = rpc(&f.ctx, 3, "tools/call", json!({"name": "list_trash", "arguments": {}}));
    let req = app.join().unwrap();
    assert_eq!((req.action.as_str(), req.args["action"].as_str()), ("trash", Some("list")));
    assert_eq!(r["result"]["content"][0]["text"], "The Trash is empty.");
    assert_eq!(r["result"]["structuredContent"], json!({"total": 0, "items": []}));
}

#[test]
fn a_read_tool_never_changes_anything() {
    let f = fixture();
    // Each list_* tool keeps to its own actions; asking it to change something is refused here,
    // before the app hears of it.
    for (tool_name, args) in [
        ("list_changes", json!({"action": "revert", "id": "1"})),
        ("list_chats", json!({"action": "trash", "chat": "x"})),
        ("list_chats", json!({"action": "pin", "chat": "x"})),
        ("list_trash", json!({"action": "restore", "id": "t1"})),
        ("list_settings", json!({"action": "set", "key": "theme", "value": "dark"})),
        ("list_suggestions", json!({"action": "accept", "id": "s1"})),
    ] {
        assert!(refused(&f.ctx, tool_name, args).contains("which tool does that"), "{tool_name}");
    }
    // And a change tool gives no list.
    assert!(refused(&f.ctx, "changes", json!({"action": "list"})).contains("not list"));
    assert!(refused(&f.ctx, "changes", json!({"action": "history"})).contains("list_changes history reads it"));
    assert!(refused(&f.ctx, "bookmarks", json!({})).contains("list_bookmarks"));
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
    ok(&f.ctx, "bookmarks", json!({"remove": "Gone note"}));
    assert_eq!(app.join().unwrap().args["remove"], "Gone note");
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
    ok(&f.ctx, "list_bookmarks", json!({"remove": "Gone note"}));
    assert!(app.join().unwrap().args.get("remove").is_none());
    // The read tool sends the app's own action, with what only the change tool takes dropped.
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
    ok(&f.ctx, "list_settings", json!({"key": "theme"}));
    let req = app.join().unwrap();
    assert_eq!((req.action.as_str(), req.args["action"].as_str(), req.args.get("key")), ("settings", Some("get"), None));
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
    ok(&f.ctx, "settings", json!({"key": "theme", "value": "dark"}));
    assert_eq!(app.join().unwrap().args["action"], "set");
    // list_saved_searches' query is its filter, so it reaches the app; the saving arguments don't.
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
    ok(&f.ctx, "list_saved_searches", json!({"query": "launch", "name": "x", "delete": true}));
    let req = app.join().unwrap();
    assert_eq!((req.args["query"].as_str(), req.args.get("name"), req.args.get("delete")), (Some("launch"), None, None));
    // triage_bookmarks only asks the model: the list is list_bookmarks'.
    assert!(refused(&f.ctx, "triage_bookmarks", json!({})).contains("list_bookmarks"));
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
    ok(&f.ctx, "triage_bookmarks", json!({"items": [{"target": "a", "path": "a.md"}]}));
    assert_eq!(app.join().unwrap().action, "triage.suggest");
}

#[test]
fn rebuilding_the_index_and_pinning_a_chat_go_to_the_app() {
    let f = fixture();
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Rebuilding the index"), error: None });
    ok(&f.ctx, "rebuild_index", json!({}));
    assert_eq!(app.join().unwrap().action, "index.rebuild");
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Pinned"), error: None });
    ok(&f.ctx, "chats", json!({"action": "pin", "chat": "Orbit App plan"}));
    let req = app.join().unwrap();
    assert_eq!((req.action.as_str(), req.args["action"].as_str()), ("chats", Some("pin")));
    // The new arguments are in the schemas, so a client can pass them.
    let list = tools();
    let props = |n: &str| list.iter().find(|t| t["name"] == n).unwrap()["inputSchema"]["properties"].clone();
    for (tool_name, arg) in [
        ("list_tasks", "effort"),
        ("list_tasks", "group"),
        ("list_inbox", "suggest"),
        ("fix_name", "files"),
        ("activity", "action"),
        ("open", "transcript"),
        ("open", "thread"),
        ("open", "document"),
    ] {
        assert!(props(tool_name).get(arg).is_some(), "{tool_name} takes {arg}");
    }
    let screens = props("open")["screen"]["enum"].clone();
    for s in ["meeting", "reply", "doc_check"] {
        assert!(screens.as_array().unwrap().iter().any(|x| x == s), "open on {s}");
    }
}

#[test]
fn search_takes_the_screens_order() {
    let f = fixture();
    // Each hit's day, from detail's `(layer, day)`.
    let days = |order: &str| -> Vec<String> {
        let out = ok(&f.ctx, "search", json!({"query": "Orbit", "order": order, "detail": true, "limit": 30}));
        out.lines()
            .filter(|l| l.ends_with(')') && l.contains(". "))
            .filter_map(|l| l.rsplit(", ").next().map(|d| d.trim_end_matches(')').to_string()))
            .collect()
    };
    let latest = days("latest");
    assert!(latest.len() > 2, "{latest:?}");
    assert!(latest.windows(2).all(|w| w[0] >= w[1]), "{latest:?}");
    let oldest = days("oldest");
    assert!(oldest.windows(2).all(|w| w[0] <= w[1]), "{oldest:?}");
    assert!(refused(&f.ctx, "search", json!({"query": "Orbit", "order": "newest"})).contains("order is best_match"));
}

#[test]
fn search_since_is_the_screens_date_range() {
    let f = fixture();
    // since adds `since:` to the query, as the screen's date range does: by the note's own date.
    let all = ok(&f.ctx, "search", json!({"query": "Orbit", "order": "latest", "detail": true, "limit": 30}));
    let since = ok(&f.ctx, "search", json!({"query": "Orbit", "since": "2026-10-03", "order": "latest", "detail": true, "limit": 30}));
    let typed = ok(&f.ctx, "search", json!({"query": "Orbit since:2026-10-03", "order": "latest", "detail": true, "limit": 30}));
    assert_eq!(since.replace(" since:2026-10-03", ""), typed.replace(" since:2026-10-03", ""));
    let days = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| l.ends_with(')') && l.contains(". "))
            .filter_map(|l| l.rsplit(", ").next().map(|d| d.trim_end_matches(')').to_string()))
            .collect()
    };
    assert!(!days(&since).is_empty() && days(&since).len() < days(&all).len(), "{since}");
    assert!(days(&since).iter().all(|d| d.as_str() >= "2026-10-03"), "{since}");
}

#[test]
fn the_re_audits_tools_and_arguments_go_to_the_app() {
    let f = fixture();
    for (tool_name, args, action) in [
        ("capture", json!({"kind": "thought", "text": "Ask Lena about the venue"}), "inbox.capture"),
        ("capture_extensions", json!({}), "capture.status"),
        ("glance", json!({}), "glance"),
        ("list_assistants", json!({"look_again": true}), "assistants.found"),
        ("provenance", json!({"source": "sources/Roadmap Update 2026-09-18.md"}), "source.provenance"),
        ("doc_check", json!({"start_register": true}), "doccheck.register"),
        ("doc_check", json!({"document": "sources/Roadmap Update 2026-09-18.md", "save": true}), "doccheck.run"),
    ] {
        let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
        ok(&f.ctx, tool_name, args);
        let req = app.join().unwrap();
        assert_eq!(req.action, action, "{tool_name}");
        // A capture and a doc check's note are this session's changes in Changes.
        if tool_name == "capture" || tool_name == "doc_check" {
            assert_eq!(req.args["origin"]["kind"], "chat", "{tool_name}");
        }
    }
    assert!(refused(&f.ctx, "create_note", json!({"title": "Venue", "content": "x", "test_run": true})).contains("give template"));
    let list = tools();
    let props = |n: &str| list.iter().find(|t| t["name"] == n).unwrap()["inputSchema"]["properties"].clone();
    for (tool_name, arg) in [
        ("edit_task", "words"),
        ("edit_task", "followup"),
        ("create_task", "view"),
        ("create_task", "defer"),
        ("create_task", "start"),
        ("fix_name", "ask_about_each_file"),
        ("save_chat", "chat"),
        ("fix_name", "files_to_leave_alone"),
        ("fix_name", "where_its_from"),
        ("automated_tools", "folder_contains"),
        ("automated_tools", "first_message_starts_with"),
        ("list_transcripts", "show"),
        ("pending_sources", "status"),
        ("search", "order"),
        ("activity", "order"),
        ("create_note", "test_run"),
        ("move_task", "project"),
        ("provenance", "source"),
        ("moving_over", "item"),
    ] {
        assert!(props(tool_name).get(arg).is_some(), "{tool_name} takes {arg}");
    }
    // No old names left beside the screen's.
    for (tool_name, arg) in [("save_chat", "title"), ("automated_tools", "folder"), ("automated_tools", "opening")] {
        assert!(props(tool_name).get(arg).is_none(), "{tool_name} has no {arg}");
    }
    assert_eq!(props("draft_reply")["tone"]["enum"], json!(["brief", "warm", "formal"]));
    assert_eq!(props("start_run")["type"]["enum"], json!(["Meeting", "1-1", "Workshop", "Interview"]));
}

#[test]
fn a_notes_own_look_goes_to_the_app_with_its_action() {
    let f = fixture();
    for (tool_name, args, action) in [
        ("list_note_looks", json!({"page": "Orbit App", "theme": "modern"}), "list"),
        ("note_look", json!({"action": "set", "page": "Orbit App", "theme": "modern", "colour": "green"}), "set"),
        ("note_look", json!({"action": "use_defaults_for_all"}), "use_defaults_for_all"),
    ] {
        let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("ok"), error: None });
        ok(&f.ctx, tool_name, args);
        let req = app.join().unwrap();
        assert_eq!((req.action.as_str(), req.args["action"].as_str()), ("note_look", Some(action)), "{tool_name}");
        // The read tool never passes on what only the change tool takes.
        if tool_name == "list_note_looks" {
            assert!(req.args.get("theme").is_none());
        }
    }
    assert!(refused(&f.ctx, "list_note_looks", json!({"action": "set"})).contains("list_note_looks does list"));
    assert!(refused(&f.ctx, "note_look", json!({"page": "Orbit App"})).contains("Give action"));
}

#[test]
fn trash_note_sends_a_transcript_to_the_app() {
    // So its links are pointed at its meeting note first (src/meetingFlow.ts's trashTranscript).
    assert!(is_transcript_page("sources/Teams. Transcript. Steerco - 2026-10-02.md"));
    assert!(is_transcript_page("sources/meetings/Teams. Transcript. Steerco - 2026-10-02.md"));
    assert!(!is_transcript_page("sources/Teams. Chat. Standup - 2026-10-03.md"));
    assert!(!is_transcript_page("notes/Teams. Transcript. Steerco.md"));
}

#[test]
fn add_image_sends_the_bytes_to_the_app() {
    use base64::Engine as _;
    let f = fixture();
    let png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
    let outside = f._tmp.path().join("Launch plan.jpg");
    std::fs::write(&outside, png).unwrap();
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Added images/Launch-plan.png."), error: None });
    let t = ok(&f.ctx, "add_image", json!({"path": outside, "reason": "For the launch note"}));
    let req = app.join().unwrap();
    assert_eq!(req.action, "image.add");
    assert!(t.contains("Added images/Launch-plan.png"), "{t}");
    // Its type is read from the bytes, not its name.
    assert_eq!(req.args["name"], "Launch plan.png");
    assert_eq!(base64::engine::general_purpose::STANDARD.decode(req.args["base64"].as_str().unwrap()).unwrap(), png);
    assert_eq!(req.args["origin"]["chat"], "chat-1");

    let b64 = base64::engine::general_purpose::STANDARD.encode(b"<svg xmlns='http://www.w3.org/2000/svg'/>");
    let app = fake_app(f.ctx.data.clone(), |_| bridge::Reply { ok: true, result: json!("Added."), error: None });
    ok(&f.ctx, "add_image", json!({"base64": b64, "name": "diagram"}));
    assert_eq!(app.join().unwrap().args["name"], "diagram.svg");

    assert!(refused(&f.ctx, "add_image", json!({})).contains("Give path"));
    assert!(refused(&f.ctx, "add_image", json!({"base64": b64})).contains("Give the image a name"));
    assert!(refused(&f.ctx, "add_image", json!({"path": outside, "base64": b64})).contains("not both"));
    let pdf = f._tmp.path().join("report.pdf");
    std::fs::write(&pdf, b"%PDF-1.7").unwrap();
    assert!(refused(&f.ctx, "add_image", json!({"path": pdf})).contains("isn't an image"));
    let inside = f.ctx.vault.join("images/in-vault.png");
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, png).unwrap();
    assert!(refused(&f.ctx, "add_image", json!({"path": inside})).contains("![[images/in-vault.png]]"));
}
