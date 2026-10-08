// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! `make evals`: Brainstead's workflows run for real (ingest, meeting notes, contradictions, the
//! wiki question, fix name, the weekly review's preparation) on a temp copy of the fixture vault,
//! each checked in code and then by a model judge against the scenario's expected behaviour
//! (`scenarios.json`, ported from the previous app's skills' `evals.json`). It calls real models
//! through the CLIs installed here, so it's not part of `make check`. Writes `target/evals/report.md`; exits 1 on a failure.
//!
//!     cargo run -p brainstead-evals -- [--model claude:haiku] [--judge claude:sonnet] [name …]

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use brainstead_core::ask::{self, Cli, Mcp, Request};
use brainstead_core::{contradictions as c, fixname, ingest, meeting, names};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Clone)]
struct Scenario {
    name: String,
    kind: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    transcript: Option<Named>,
    #[serde(default)]
    plant: Option<Planted>,
    /// For contradictions: a page's kept claims file planted as well (`wiki/.claims/…`).
    #[serde(default)]
    claims: Option<Planted>,
    /// For ingest: a fact the kept claims must give as latest.
    #[serde(default)]
    fact: Option<Fact>,
    /// For ingest: this wiki page made long first (`pad_page`), as real hub pages are.
    #[serde(default)]
    pad: Option<String>,
    /// For ingest: ingest the source a second time once the first's changes are made.
    #[serde(default)]
    twice: bool,
    #[serde(default)]
    question: String,
    #[serde(default)]
    wrong: String,
    #[serde(default)]
    right: String,
    #[serde(default)]
    expect: Vec<String>,
    /// A model this scenario needs instead of the run's (ingest needs more than Haiku); `--model`
    /// given on the command line wins.
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize, Clone)]
struct Named {
    name: String,
    text: String,
}

#[derive(Deserialize, Clone)]
struct Fact {
    page: String,
    subject: String,
    attribute: String,
    /// What the latest value must contain.
    value: String,
}

#[derive(Deserialize, Clone)]
struct Planted {
    path: String,
    text: String,
}

/// What a scenario made, for the judge, and what the code checks said.
struct Outcome {
    output: String,
    failures: Vec<String>,
}

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn copy_vault(to: &Path) {
    let from = here().join("../../tests/fixtures/vault");
    for e in walkdir::WalkDir::new(&from).into_iter().flatten() {
        let dest = to.join(e.path().strip_prefix(&from).unwrap());
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
        } else {
            std::fs::copy(e.path(), &dest).unwrap();
        }
    }
}

/// Finds a CLI as a terminal would: a login shell's `command -v`, or on Windows `where`, taking an
/// `.exe` before a `.cmd` and running what an npm shim runs rather than the shim
/// (brainstead_core::paths, the app's rule too). The program and its leading arguments.
fn find(cli: Cli) -> Option<(PathBuf, Vec<String>)> {
    #[cfg(unix)]
    {
        let out = Command::new("/bin/zsh").args(["-lc", &format!("command -v {}", cli.bin())]).output().ok()?;
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!p.is_empty()).then(|| (PathBuf::from(p), Vec::new()))
    }
    #[cfg(windows)]
    {
        let where_ = |name: &str| {
            let out = Command::new("where").arg(name).output().ok()?;
            brainstead_core::paths::pick_program(&String::from_utf8_lossy(&out.stdout))
        };
        let p = where_(cli.bin())?;
        Some(brainstead_core::paths::resolve_npm_shim(&p, || where_("node")).unwrap_or((p, Vec::new())))
    }
}

/// One turn with a CLI, as Ask runs it; the answer's text.
fn ask_model(model: &str, prompt: &str, cwd: &Path, mcp: Option<&Mcp>) -> Result<String, String> {
    ask_model_with(model, prompt, cwd, mcp, false)
}

/// As `ask_model`; with `help`, told about the app's help as an Ask chat is (`ask::help_hint`).
fn ask_model_with(model: &str, prompt: &str, cwd: &Path, mcp: Option<&Mcp>, help: bool) -> Result<String, String> {
    let cli = Cli::route(model);
    let (bin, prefix) = find(cli).ok_or_else(|| format!("{} isn't installed.", cli.label()))?;
    let mut system = ask::system_prompt(&chrono::Local::now().format("%A %-d %B %Y").to_string(), mcp.is_some());
    if let Some(h) = ask::help_hint(cli, mcp.is_some() && cli.takes_mcp()).filter(|_| help) {
        system = format!("{system}\n{h}");
    }
    let vault = cwd.to_string_lossy().into_owned();
    let launch = ask::args(
        cli,
        &Request { prompt, model, session: None, system: &system, vault: &vault, mcp: mcp.filter(|_| cli.takes_mcp()), images: &[] },
    )?;
    // Antigravity runs as Ask runs it: in a folder of its own holding its read-only agent and the
    // help, with the vault added.
    let agy = tempfile::tempdir().map_err(|e| e.to_string())?;
    let run_in = if cli == Cli::Antigravity {
        let agent = agy.path().join(".agents/agents");
        std::fs::create_dir_all(&agent).map_err(|e| e.to_string())?;
        std::fs::write(agent.join(format!("{}.md", ask::AGY_AGENT)), ask::agy_agent(&system)).map_err(|e| e.to_string())?;
        let help = agy.path().join(ask::AGY_HELP_DIR);
        std::fs::create_dir_all(&help).map_err(|e| e.to_string())?;
        std::fs::write(help.join("index.md"), brainstead_core::help::list()).map_err(|e| e.to_string())?;
        for (n, t) in brainstead_core::help::files() {
            std::fs::write(help.join(n), t).map_err(|e| e.to_string())?;
        }
        agy.path().to_path_buf()
    } else {
        cwd.to_path_buf()
    };
    let mut child = Command::new(bin)
        .current_dir(&run_in)
        .args(&prefix)
        .args(&launch.args)
        .stdin(if launch.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // The prompt, for the CLIs that read it from standard input, written on a thread and closed.
    let fed = child.stdin.take().zip(launch.stdin).map(|(mut pipe, input)| {
        std::thread::spawn(move || {
            let _ = std::io::Write::write_all(&mut pipe, input.as_bytes());
        })
    });
    let mut p = ask::Parser::new(cli);
    for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        p.feed(&line);
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut err);
    }
    let ok = child.wait().is_ok_and(|s| s.success());
    let _ = fed.map(|t| t.join());
    let out = p.finish(ok, &err);
    match out.error {
        Some(e) => Err(e),
        None => Ok(out.text),
    }
}

fn wiki_pages(root: &Path) -> Vec<(String, Vec<String>)> {
    c::pages(root).into_iter().map(|(r, _, a)| (r, a)).collect()
}

/// A page grown long: a long `sources:` list, and a Timeline of forty weekly syncs of invented
/// history, newest first, after its topics, so it's shown to the model in part.
fn pad_page(root: &Path, rel: &str) {
    let path = root.join(rel);
    let page = std::fs::read_to_string(&path).unwrap_or_default();
    let sources: String = (1..=120).map(|i| format!("  - \"[[Weekly sync {i:03} - Orbit App]]\"\n")).collect();
    let page = page.replacen("sources:\n", &format!("sources:\n{sources}"), 1);
    let start = chrono::NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    let timeline: String = (0..40)
        .map(|i| {
            let d = start - chrono::Duration::days(7 * i);
            format!(
                "### {d} — Weekly sync {:03}\nSource: [[Weekly sync {:03} - Orbit App]]\n\n{}\n\n",
                40 - i,
                40 - i,
                "The team walked the board, cleared the review queue and noted no change to the plan. ".repeat(14)
            )
        })
        .collect();
    let page = format!("{}\n\n## Timeline\n\n{}", page.trim_end(), timeline.trim_end());
    std::fs::write(path, page + "\n").unwrap();
}

/// What one ingest of the source would change: the model's answer, checked.
fn ingest_once(s: &Scenario, root: &Path, model: &str) -> Result<(ingest::Source, Vec<ingest::Planned>, Vec<ingest::Dropped>), String> {
    let text = std::fs::read_to_string(root.join(&s.source)).unwrap_or_default();
    let src = ingest::Source { rel: s.source.clone(), text: ingest::SourceText::Text(text.clone()) };
    let m = names::mentions(&text, &wiki_pages(root));
    let pages: Vec<(String, String)> =
        m.iter().filter_map(|x| std::fs::read_to_string(root.join(&x.page)).ok().map(|t| (x.page.clone(), t))).collect();
    let answer =
        ask_model(model, &ingest::prompt(&src, &pages, &brainstead_core::catalogue::render(root), "Friday 2 October 2026"), root, None)
            .and_then(|a| ingest::parse_answer(&a))?;
    let read = |r: &str| std::fs::read_to_string(root.join(r)).ok();
    let resolve =
        |n: &str| m.iter().find(|x| brainstead_core::lint::stem(brainstead_core::lint::name_of(&x.page)) == n).map(|x| x.page.clone());
    let (planned, dropped) = ingest::plan(&src, &answer, read, resolve, "2026-10-02", ingest::page_budget(pages.len()));
    Ok((src, planned, dropped))
}

/// The planned changes' claims kept, as the app keeps them with each change (D-20261006-16).
fn keep_claims(root: &Path, src: &ingest::Source, planned: &[ingest::Planned]) {
    for p in planned.iter().filter(|p| !p.claims.is_empty()) {
        let Some(rel) = brainstead_core::claims::path(&p.page) else { continue };
        let kept: Vec<_> =
            p.claims.iter().map(|c| brainstead_core::claims::Kept::from_claim(c, &src.rel, p.entry.as_deref(), "2026-10-02")).collect();
        let file = root.join(rel);
        let now = std::fs::read_to_string(&file).ok();
        if let Some(t) = brainstead_core::claims::with_source(now.as_deref(), &p.page, &src.rel, &kept) {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, t).unwrap();
        }
    }
}

fn run_ingest(s: &Scenario, root: &Path, model: &str) -> Outcome {
    let mut failures = Vec::new();
    if let Some(p) = &s.pad {
        pad_page(root, p);
    }
    if let Some(p) = &s.plant {
        std::fs::write(root.join(&p.path), &p.text).unwrap();
    }
    let files = brainstead_core::pageshape::vault_files(root);
    let sources = brainstead_core::pageshape::Sources::from_paths(files.iter().map(String::as_str));
    let is_source = |t: &str| sources.is_source(t) || t.starts_with("Weekly sync");
    let (src, mut planned, mut dropped) = match ingest_once(s, root, model) {
        Ok(x) => x,
        Err(e) => return Outcome { output: e.clone(), failures: vec![e] },
    };
    let mut output = String::new();
    if s.twice {
        // The first ingest's changes made, then the same source again.
        for p in &planned {
            output.push_str(&format!("## First ingest: {} ({:?})\n\n{}\n\n", p.page, p.kind, p.after));
            let to = root.join(&p.page);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::write(to, &p.after).unwrap();
        }
        keep_claims(root, &src, &planned);
        match ingest_once(s, root, model) {
            Ok((_, p2, d2)) => (planned, dropped) = (p2, d2),
            Err(e) => return Outcome { output: e.clone(), failures: vec![e] },
        }
    }
    if planned.is_empty() {
        failures.push("No page change survived the checks.".into());
    }
    let cite = format!("Source: [[{}]]", src.link());
    for p in &planned {
        if !p.page.starts_with("wiki/") {
            failures.push(format!("A change outside the wiki: {}", p.page));
        }
        if !p.after.contains(&format!("[[{}]]", src.link())) {
            failures.push(format!("{} doesn't cite the source.", p.page));
        }
        if brainstead_core::pageshape::shaped(&p.page) {
            if !brainstead_core::pageshape::in_shape(&p.after, &is_source) {
                failures.push(format!("{} is left out of the page shape.", p.page));
            }
            if p.after.matches(&cite).count() > 1 {
                let lines: Vec<&str> = p.after.lines().filter(|l| l.starts_with("### ") || l.contains(&cite)).collect();
                failures.push(format!("{} has more than one timeline entry from the source: {}", p.page, lines.join(" / ")));
            }
        }
    }
    // The claims kept: on disk with each page, one set per source (a second ingest replaces it).
    keep_claims(root, &src, &planned);
    for p in planned.iter().filter(|p| !p.claims.is_empty()) {
        let Some(rel) = brainstead_core::claims::path(&p.page) else { continue };
        let text = std::fs::read_to_string(root.join(&rel)).ok();
        let n = brainstead_core::claims::of_source(text.as_deref(), &src.rel).len();
        if n != p.claims.len() {
            failures.push(format!("{rel} has {n} claims from the source, not the {} the change kept.", p.claims.len()));
        }
    }
    if let Some(f) = &s.fact {
        let text = brainstead_core::claims::path(&f.page).and_then(|r| std::fs::read_to_string(root.join(r)).ok()).unwrap_or_default();
        let found = brainstead_core::claims::facts(&brainstead_core::claims::parse(&text), Some(&f.subject), Some(&f.attribute));
        match found.first() {
            Some(x) if x.latest.value.contains(&f.value) => {
                output.push_str(&format!("Fact: {} {} is {} (as of {:?})\n\n", x.subject, x.attribute, x.latest.value, x.latest.as_of));
            }
            Some(x) => failures.push(format!("The latest {} for {} is {:?}, not {:?}.", f.attribute, f.subject, x.latest.value, f.value)),
            None => failures.push(format!("No kept {} for {} on {}.", f.attribute, f.subject, f.page)),
        }
    }
    let summary = planned.iter().any(|p| p.page.starts_with("wiki/summaries/"));
    if src.is_journal() && summary {
        failures.push("A summary page for a journal note.".into());
    }
    if !src.is_journal() && !summary && !s.twice {
        failures.push("No summary page for a document.".into());
    }
    for p in &planned {
        output.push_str(&format!(
            "## Proposed{}: {} ({:?})\n\n{}\n\n",
            if s.twice { " by the second ingest" } else { "" },
            p.page,
            p.kind,
            p.after
        ));
    }
    for d in &dropped {
        output.push_str(&format!("Dropped {}: {}\n", d.page, d.reason));
        if d.reason.contains("shown only part of") {
            failures.push(format!("It tried to rewrite a section of {} it was shown only part of.", d.page));
        }
    }
    Outcome { output, failures }
}

fn run_meeting(s: &Scenario, root: &Path, model: &str) -> Outcome {
    let mut failures = Vec::new();
    let t = s.transcript.clone().unwrap();
    std::fs::write(root.join("sources").join(&t.name), &t.text).unwrap();
    let inf = meeting::infer(&t.name, &t.text, s.owner.as_deref(), |_| vec![], |f| root.join(f).exists());
    if !inf.confident {
        failures.push(format!("Not sure of the note: {:?}", inf.ask));
    }
    let (Some(ty), Some(name), Some(file)) = (inf.note_type.clone(), inf.name.clone(), inf.suggested_filename.clone()) else {
        return Outcome { output: format!("{inf:?}"), failures: vec!["No note inferred.".into()] };
    };
    let tpl = std::fs::read_to_string(root.join(format!("Templates/{ty}.md"))).unwrap_or_default();
    let sc = meeting::scaffold(&tpl, &name);
    let note = match ask_model(model, &meeting::prompt(&file, &sc, &t.text, s.owner.as_deref()), root, None)
        .and_then(|a| meeting::note_from_answer(&a))
    {
        Ok(n) => n,
        Err(e) => return Outcome { output: e.clone(), failures: vec![e] },
    };
    for l in sc.lines().filter(|l| l.starts_with('#')) {
        if !note.lines().any(|n| n.trim_end() == l.trim_end()) {
            failures.push(format!("The heading {l:?} is gone."));
        }
    }
    if note.contains("Teams. Transcript") {
        failures.push("It cites the transcript.".into());
    }
    if note.lines().any(|l| l.trim_start().starts_with("# ")) && !sc.lines().any(|l| l.starts_with("# ")) {
        failures.push("A top-level # heading inside the note.".into());
    }
    Outcome { output: format!("{file}\n\n{note}"), failures }
}

fn run_contradictions(s: &Scenario, root: &Path, model: &str, judge_model: &str) -> Outcome {
    let mut failures = Vec::new();
    for p in [&s.plant, &s.claims].into_iter().flatten() {
        let to = root.join(&p.path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, &p.text).unwrap();
    }
    let data = tempfile::tempdir().unwrap();
    let st = c::State::new(data.path());
    let pages = c::pages(root);
    let texts: HashMap<String, String> = pages.iter().map(|(r, t, _)| (r.clone(), t.clone())).collect();
    let all: Vec<String> = pages.iter().map(|p| p.0.clone()).collect();
    let extract_model = if model.starts_with("claude") { "claude:haiku" } else { model };
    let answer = match ask_model(extract_model, &c::extract_prompt(&all, &texts, &pages), root, None) {
        Ok(a) => a,
        Err(e) => return Outcome { output: e.clone(), failures: vec![e] },
    };
    let mut by: HashMap<String, Vec<c::Claim>> = all.iter().map(|p| (p.clone(), vec![])).collect();
    for raw in c::parse_lines::<c::Claim>(&answer) {
        if let (Some(t), Some(v)) = (texts.get(&raw.page), by.get_mut(&raw.page)) {
            if let Ok(ok) = c::validate(&raw, t) {
                v.push(ok);
            }
        }
    }
    for (p, cl) in by {
        st.cache(&p, &texts[&p], cl).unwrap();
    }
    let (claims, _) = c::current(&st, &pages);
    let claims = c::with_kept(claims, &brainstead_core::claims::all(root), &pages);
    let clashes = c::find_clashes(&claims, &c::name_index(&wiki_pages(root)), c::WINDOW_DAYS);
    let launch: Vec<&c::Clash> = clashes.iter().filter(|x| x.subject == "Orbit App" && x.attribute == "go_live_date").collect();
    if launch.is_empty() {
        failures.push("The planted launch-date clash wasn't found.".into());
    }
    let mut output = format!("{} claims, {} clashes\n", claims.len(), clashes.len());
    if !clashes.is_empty() {
        let known: HashMap<String, c::Clash> = clashes.iter().map(|x| (x.id.clone(), x.clone())).collect();
        match ask_model(judge_model, &c::judge_prompt(&clashes), root, None) {
            Ok(a) => {
                for v in c::parse_lines::<c::Verdict>(&a) {
                    if let Ok(v) = c::check_verdict(v, &known, "2026-10-02") {
                        output.push_str(&format!("Verdict {}: {} {} — {}; fix: {:?}\n", v.id, v.verdict, v.severity, v.summary, v.patch));
                        if launch.iter().any(|l| l.id == v.id) {
                            if !["contradiction", "evolution"].contains(&v.verdict.as_str()) {
                                failures.push(format!("The launch clash judged {}", v.verdict));
                            }
                            if v.patch.as_ref().is_none_or(|p| !p.page.starts_with("wiki/")) {
                                failures.push("No fix on a wiki page for the launch clash.".into());
                            }
                        }
                    }
                }
            }
            Err(e) => failures.push(e),
        }
    }
    for x in &clashes {
        output.push_str(&format!(
            "Clash {} {}: {:?}\n",
            x.subject,
            x.attribute,
            x.claims.iter().map(|y| (&y.page, &y.value)).collect::<Vec<_>>()
        ));
    }
    Outcome { output, failures }
}

fn run_wiki(s: &Scenario, root: &Path, model: &str) -> Outcome {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("settings.json"), serde_json::to_vec(&serde_json::json!({"vaultPath": root})).unwrap()).unwrap();
    let mut ix = brainstead_core::Index::open(&data.path().join("index.db")).unwrap();
    ix.sync(&brainstead_core::vault::Vault::new(root.to_path_buf(), vec![])).unwrap();
    drop(ix);
    let mcp = Mcp {
        command: std::env::current_exe().unwrap().to_string_lossy().into_owned(),
        args: vec!["--mcp".into(), "--data".into(), data.path().to_string_lossy().into_owned()],
    };
    let prompt = brainstead_core::workflows::expand(&format!("/wiki {}", s.question)).unwrap();
    match ask_model(model, &prompt, root, Some(&mcp)) {
        Ok(a) => {
            let failures =
                if s.question.contains("Orbit") && !a.contains("[[") { vec!["No [[citation]] in the answer.".into()] } else { vec![] };
            Outcome { output: a, failures }
        }
        Err(e) => Outcome { output: e.clone(), failures: vec![e] },
    }
}

/// A question about the app itself, answered from its help: through the MCP server's `help` tool,
/// or, for Antigravity, from the files in `help/` in its own folder (`ask_model_with`).
fn run_help(s: &Scenario, root: &Path, model: &str) -> Outcome {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("settings.json"), serde_json::to_vec(&serde_json::json!({"vaultPath": root})).unwrap()).unwrap();
    let mut ix = brainstead_core::Index::open(&data.path().join("index.db")).unwrap();
    ix.sync(&brainstead_core::vault::Vault::new(root.to_path_buf(), vec![])).unwrap();
    drop(ix);
    let mcp = Mcp {
        command: std::env::current_exe().unwrap().to_string_lossy().into_owned(),
        args: vec!["--mcp".into(), "--data".into(), data.path().to_string_lossy().into_owned()],
    };
    match ask_model_with(model, &s.question, root, Some(&mcp), true) {
        Ok(a) => {
            let failures =
                s.right.split('|').filter(|w| !w.is_empty() && !a.contains(w)).map(|w| format!("The answer doesn't say “{w}”.")).collect();
            Outcome { output: a, failures }
        }
        Err(e) => Outcome { output: e.clone(), failures: vec![e] },
    }
}

fn run_fixname(s: &Scenario, root: &Path) -> Outcome {
    let r = fixname::Request {
        wrong: s.wrong.clone(),
        right: s.right.clone(),
        right_page: None,
        guards: vec![],
        ambiguous: false,
        note: "eval".into(),
        skip_substitution: false,
    };
    let rows = fixname::audit(root, &r);
    let mut failures = Vec::new();
    if rows.iter().any(|x| x.layer == "source" && x.action == "rewrite") {
        failures.push("It would rewrite a source.".into());
    }
    // The corrections file lives in the app data folder: here, beside the vault copy.
    let subs = root.with_file_name("data").join(names::SUBSTITUTIONS);
    if let Err(e) = fixname::apply(root, &subs, &r, &rows) {
        failures.push(e.to_string());
    }
    for e in walkdir::WalkDir::new(root).into_iter().flatten().filter(|e| e.file_type().is_file()) {
        if std::fs::read_to_string(e.path())
            .is_ok_and(|t| t.contains(&format!("{} {}", s.right, s.right.split_whitespace().last().unwrap_or(""))))
        {
            failures.push(format!("A doubled surname in {}", e.path().display()));
        }
    }
    if names::load(&subs).iter().all(|x| x.wrong != s.wrong) {
        failures.push("The correction wasn't registered.".into());
    }
    Outcome { output: format!("{} files planned", rows.len()), failures }
}

/// The weekly review's preparation for 2026-W40 on Friday 2 October, from the fixture vault: the
/// answer through the app's own checks (`prep::parse`: steps, actions, quotes, nothing that's
/// already a task).
fn run_weekprep(s: &Scenario, root: &Path, model: &str) -> Outcome {
    use brainstead_core::reviews::prep::{self, Action};
    for p in [&s.plant, &s.claims].into_iter().flatten() {
        let to = root.join(&p.path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, &p.text).unwrap();
    }
    let data = tempfile::tempdir().unwrap();
    let mut ix = brainstead_core::Index::open(&data.path().join("index.db")).unwrap();
    ix.sync(&brainstead_core::vault::Vault::new(root.to_path_buf(), vec![])).unwrap();
    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
    let now = today.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp_millis();
    let inbox = brainstead_core::inbox::items(root);
    let inputs = match prep::gather(root, &ix, "2026-W40", today, now, &inbox) {
        Ok(i) => i,
        Err(e) => return Outcome { output: e.clone(), failures: vec![e] },
    };
    let answer = match ask_model(model, &prep::prompt(&inputs, "Friday 2 October 2026"), root, None) {
        Ok(a) => a,
        Err(e) => return Outcome { output: e.clone(), failures: vec![e] },
    };
    let (got, dropped) = match prep::parse(&answer, &inputs, |p| std::fs::read_to_string(root.join(p)).ok()) {
        Ok(x) => x,
        Err(e) => return Outcome { output: answer, failures: vec![e] },
    };
    let mut failures = Vec::new();
    if got.is_empty() {
        failures.push("No suggestion survived the checks.".into());
    }
    let steps: std::collections::BTreeSet<&str> = got.iter().map(|g| g.step.as_str()).collect();
    if steps.len() < 3 {
        failures.push(format!("Suggestions in only {} steps; dropped: {}", steps.len(), dropped.join("; ")));
    }
    if dropped.len() > got.len() {
        failures.push(format!("More dropped ({}) than kept ({}): {}", dropped.len(), got.len(), dropped.join("; ")));
    }
    let mut output = String::new();
    for g in &got {
        let action = match &g.action {
            None => String::new(),
            Some(Action::Add { text, .. }) => format!(" → add “{text}”"),
            Some(Action::Tick { task }) => format!(" → tick “{}”", task.text),
            Some(Action::Defer { task, date }) => format!(" → defer “{}” to {date}", task.text),
            Some(Action::Waiting { task }) => format!(" → waiting for “{}”", task.text),
            Some(Action::Someday { task }) => format!(" → someday “{}”", task.text),
            Some(Action::Edit { task, text }) => format!(" → reword “{}” as “{text}”", task.text),
            Some(Action::Clarify { item, becomes, .. }) => format!(" → clarify “{}” as {becomes}", item.text),
            Some(Action::Link { path, phrase, target }) => format!(" → link “{phrase}” in {path} to {target}"),
        };
        let from = g.source.as_ref().map(|s| format!(" (from {}: “{}”)", s.path, s.quote)).unwrap_or_default();
        output.push_str(&format!("- [{}] {}{from}{action}\n", g.step, g.text));
    }
    for d in &dropped {
        output.push_str(&format!("Dropped: {d}\n"));
    }
    Outcome { output, failures }
}

/// The judge's word on whether the output does what's expected.
fn judge(model: &str, s: &Scenario, output: &str, cwd: &Path) -> (bool, String) {
    if s.expect.is_empty() {
        return (true, "No behaviour to judge.".into());
    }
    let prompt = format!(
        "You are grading an AI workflow's output against what it should do. Be strict but fair: judge only the listed behaviours.\n\nScenario: {}\n\nExpected behaviour:\n{}\n\nOutput:\n\n{}\n\nAnswer with JSON only: {{\"pass\": true or false, \"reasons\": \"one or two sentences\"}}",
        s.name,
        s.expect.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"),
        output.chars().take(30_000).collect::<String>()
    );
    match ask_model(model, &prompt, cwd, None) {
        Ok(a) => {
            let t = a.trim();
            let json = t.find('{').and_then(|i| t.rfind('}').map(|j| &t[i..=j])).unwrap_or("{}");
            let v: Value = serde_json::from_str(json).unwrap_or_default();
            (v["pass"].as_bool().unwrap_or(false), v["reasons"].as_str().unwrap_or(t).to_string())
        }
        Err(e) => (false, format!("The judge failed: {e}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The wiki scenario's CLI starts this program as Brainstead's MCP server.
    if args.first().map(String::as_str) == Some("--mcp") {
        std::process::exit(brainstead_mcp::main_with(&args[1..], PathBuf::new()));
    }
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let chosen = opt("--model");
    let model = chosen.clone().unwrap_or_else(|| "claude:haiku".into());
    let judge_model = opt("--judge").unwrap_or_else(|| "claude:sonnet".into());
    let only: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|(_, a)| a)
        .collect();
    let scenarios: Vec<Scenario> = serde_json::from_str(&std::fs::read_to_string(here().join("scenarios.json")).unwrap()).unwrap();
    let mut report = format!(
        "# Brainstead evals\n\nModel {model}, judge {judge_model}, {}.\n\n| Scenario | Checks | Judge | Notes |\n|---|---|---|---|\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M")
    );
    let mut failed = 0;
    for s in scenarios.iter().filter(|s| only.is_empty() || only.iter().any(|o| s.name.contains(o.as_str()))) {
        eprintln!("… {}", s.name);
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("vault");
        copy_vault(&root);
        let model = if chosen.is_none() { s.model.clone().unwrap_or_else(|| model.clone()) } else { model.clone() };
        let o = match s.kind.as_str() {
            "ingest" => run_ingest(s, &root, &model),
            "meeting" => run_meeting(s, &root, &model),
            "contradictions" => run_contradictions(s, &root, &model, &model),
            "wiki" => run_wiki(s, &root, &model),
            "help" => run_help(s, &root, &model),
            "fixname" => run_fixname(s, &root),
            "weekprep" => run_weekprep(s, &root, &model),
            k => Outcome { output: String::new(), failures: vec![format!("Unknown kind {k}")] },
        };
        let (pass, why) =
            if o.failures.is_empty() { judge(&judge_model, s, &o.output, &root) } else { (false, "Not judged: the checks failed.".into()) };
        let ok = o.failures.is_empty() && pass;
        if !ok {
            failed += 1;
        }
        let checks = if o.failures.is_empty() { "pass".to_string() } else { o.failures.join("; ") };
        report.push_str(&format!(
            "| {}{} | {} | {} | {} |\n",
            s.name,
            s.model.as_deref().filter(|_| chosen.is_none()).map(|m| format!(" ({m})")).unwrap_or_default(),
            checks.replace('|', "/"),
            if pass { "pass" } else { "fail" },
            why.replace('|', "/").replace('\n', " ")
        ));
        eprintln!("  {} — {}", if ok { "PASS" } else { "FAIL" }, if o.failures.is_empty() { &why } else { &checks });
    }
    let dir = here().join("../target/evals");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("report.md"), &report);
    println!("{report}");
    std::process::exit(i32::from(failed > 0));
}
