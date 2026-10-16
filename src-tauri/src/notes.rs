// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's calls: saving and creating files, drafts, images, rename with link rewrite, the
//! trash, rich copy and Export PDF. Vault writes are refused while read-only is on, and re-index
//! the files they touch at once (edits.rs).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use base64::Engine;
use brainstead_core::drafts::{Draft, Drafts};
use brainstead_core::rename::{self, RenamePlan};
use brainstead_core::trash::{self, TrashEntry};
use brainstead_core::{images, write};
use regex::Regex;
use tauri::{AppHandle, Manager, State};

use crate::edits::{writable, writing, written, EditError, Res};
use crate::vault::VaultService;
use crate::AppState;

pub fn drafts_dir(data: &Path) -> PathBuf {
    data.join("drafts")
}

fn root(svc: &VaultService) -> Res<PathBuf> {
    Ok(svc.root().ok_or("No vault is open.".to_string())?)
}

/// Saves the editor's text if the file is still at `base`; the draft goes once it's saved.
#[tauri::command]
pub async fn doc_save(app: AppHandle, path: String, content: String, base: String) -> Res<String> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let abs = app.state::<VaultService>().resolve_path(&path)?;
        refuse_not_utf8(&abs, &path)?;
        let v = write::save_file(&abs, &content, &base)?;
        app.state::<Drafts>().discard(&path);
        written(&app, &[abs]);
        Ok(v)
    })
    .await
}

/// The editor's text of a file that isn't UTF-8 is a lossy decoding of it: saving it would replace
/// every byte it couldn't read, so it isn't saved.
fn refuse_not_utf8(abs: &Path, path: &str) -> Res<()> {
    match std::fs::read(abs) {
        Ok(b) if std::str::from_utf8(&b).is_err() => Err(write::WriteError::Invalid(crate::not_utf8(path)).into()),
        _ => Ok(()),
    }
}

#[tauri::command]
pub async fn doc_create(app: AppHandle, path: String, content: String) -> Res<String> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let abs = app.state::<VaultService>().resolve_path(&path)?;
        let v = write::create_file(&abs, &content)?;
        written(&app, &[abs]);
        Ok(v)
    })
    .await
}

/// Makes a new vault called `name` inside `parent`, filled with the starter notes, and returns its
/// path. The window then opens it, read-write: unlike a folder the user chose, it isn't their data
/// yet.
#[tauri::command]
pub async fn vault_create(parent: String, name: String) -> Result<String, String> {
    crate::blocking(move || {
        let dir = brainstead_core::starter::create(Path::new(&parent), &name, chrono::Local::now().naive_local())?;
        crate::applog!("vault: made a new vault with the starter notes");
        Ok(dir.to_string_lossy().into_owned())
    })
    .await
}

/// Adds the starter vault's example notes that the open vault doesn't have (Settings › Vault);
/// nothing already there is changed. Returns how many were added.
#[tauri::command]
pub async fn vault_add_examples(app: AppHandle) -> Res<usize> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let root = root(&app.state::<VaultService>())?;
        let added = brainstead_core::starter::add_missing(&root, chrono::Local::now().naive_local())?;
        written(&app, &added);
        Ok(added.len())
    })
    .await
}

#[tauri::command]
pub fn draft_write(drafts: State<Drafts>, draft: Draft) -> Result<(), String> {
    drafts.write(&draft)
}

#[tauri::command]
pub fn draft_read(drafts: State<Drafts>, path: String) -> Option<Draft> {
    drafts.read(&path)
}

#[tauri::command]
pub fn draft_discard(drafts: State<Drafts>, path: String) {
    drafts.discard(&path)
}

#[tauri::command]
pub fn drafts_list(drafts: State<Drafts>) -> Vec<Draft> {
    drafts.list()
}

fn save_image(app: &AppHandle, name: &str, bytes: &[u8]) -> Res<String> {
    writable(&app.state::<AppState>())?;
    let svc = app.state::<VaultService>();
    let root = root(&svc)?;
    let rel = images::save(&root, name, bytes, &images::stamp_now())?;
    written(app, &[root.join(&rel)]);
    Ok(rel)
}

/// A pasted image (base64 bytes) into `images/`. Returns its vault-relative path.
#[tauri::command]
pub async fn image_save(app: AppHandle, name: String, base64: String) -> Res<String> {
    writing(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64.trim())
            .map_err(|e| EditError::from(format!("That image couldn't be read: {e}")))?;
        save_image(&app, &name, &bytes)
    })
    .await
}

/// An image an assistant added (the MCP add_image tool): saved to `images/` as a paste is, and
/// recorded in Changes, where Revert moves it to the Trash.
#[tauri::command]
pub async fn image_add(
    app: AppHandle,
    name: String,
    base64: String,
    reason: Option<String>,
    origin: brainstead_core::proposals::Origin,
    model: Option<String>,
) -> Res<crate::changes::Outcome> {
    writing(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64.trim())
            .map_err(|e| EditError::from(format!("That image couldn't be read: {e}")))?;
        if bytes.len() > images::MAX_BYTES {
            return Err(EditError::from(format!("That image is over {} MB.", images::MAX_BYTES >> 20)));
        }
        let rel = save_image(&app, &name, &bytes)?;
        crate::changes::record_image(&app, &rel, reason.as_deref().unwrap_or(""), origin, model)
    })
    .await
}

/// An image dropped from Finder (a full path) copied into `images/`.
#[tauri::command]
pub async fn image_import(app: AppHandle, abs: String) -> Res<String> {
    writing(move || {
        let p = PathBuf::from(&abs);
        let bytes = std::fs::read(&p).map_err(|e| EditError::from(format!("Couldn't read {abs}: {e}")))?;
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        save_image(&app, &name, &bytes)
    })
    .await
}

/// One file dropped onto Sources: where it went, or why it didn't.
#[derive(serde::Serialize)]
pub struct Imported {
    pub name: String,
    pub path: Option<String>,
    pub error: Option<String>,
}

/// Files dropped onto the Sources screen (full paths) copied into `sources/`.
#[tauri::command]
pub async fn sources_import(app: AppHandle, paths: Vec<String>) -> Res<Vec<Imported>> {
    writable(&app.state::<AppState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app.state::<VaultService>())?;
        let mut out = vec![];
        let mut wrote = vec![];
        for p in paths {
            let from = PathBuf::from(&p);
            let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(p.clone());
            match brainstead_core::webcapture::import_file(&root, &from) {
                Ok(rel) => {
                    wrote.push(root.join(&rel));
                    out.push(Imported { name, path: Some(rel), error: None });
                }
                Err(e) => out.push(Imported { name, path: None, error: Some(e.to_string()) }),
            }
        }
        written(&app, &wrote);
        Ok(out)
    })
    .await
    .map_err(|e| EditError::from(e.to_string()))?
}

/// Runs `f` with what a rename needs from the index: the files linking to `path`, and whether a
/// link target resolves to it.
pub(crate) fn with_links<T>(svc: &VaultService, path: &str, f: impl FnOnce(&[String], &dyn Fn(&str) -> bool) -> Res<T>) -> Res<T> {
    let mut out = None;
    svc.with_index(|ix, _| {
        let linkers = ix.linkers(path)?;
        let resolves = |t: &str| ix.resolve(&[t.to_string()]).ok().and_then(|v| v.into_iter().next().flatten()).as_deref() == Some(path);
        out = Some(f(&linkers, &resolves));
        Ok(())
    })?;
    out.unwrap()
}

#[tauri::command]
pub async fn rename_preview(app: AppHandle, path: String, to: String) -> Res<RenamePlan> {
    tauri::async_runtime::spawn_blocking(move || {
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        with_links(&svc, &path, |linkers, resolves| Ok(rename::plan(&root, &path, &to, linkers, resolves)?))
    })
    .await
    .map_err(|e| EditError::from(e.to_string()))?
}

/// Carries out the plan shown in the preview; refuses if anything changed since. Returns the new path.
#[tauri::command]
pub async fn rename_commit(app: AppHandle, plan: RenamePlan) -> Res<String> {
    tauri::async_runtime::spawn_blocking(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        with_links(&svc, &plan.from, |linkers, resolves| Ok(rename::commit(&root, &plan, linkers, resolves)?))?;
        app.state::<Drafts>().rename(&plan.from, &plan.to);
        moved(&app, &plan.from, Some(&plan.to), &plan.to);
        let mut paths: Vec<PathBuf> = plan.changes.iter().map(|c| root.join(&c.path)).collect();
        paths.extend([root.join(&plan.from), root.join(&plan.to)]);
        paths.dedup();
        written(&app, &paths);
        Ok(plan.to)
    })
    .await
    .map_err(|e| EditError::from(e.to_string()))?
}

#[tauri::command]
pub async fn trash_move(app: AppHandle, path: String) -> Res<TrashEntry> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        let layer = svc.with_index(|ix, _| Ok(ix.summary(&path)?.map(|s| s.layer))).ok().flatten().unwrap_or_else(|| "note".into());
        let e = trash::move_to_trash(&root, &path, &layer)?;
        // Unsaved edits go into the trash with the note, and come back if it's restored.
        app.state::<Drafts>().rename(&path, &trashed_draft(&e.id));
        moved(&app, &path, None, &trashed_draft(&e.id));
        written(&app, &[root.join(&path)]);
        Ok(e)
    })
    .await
}

/// Previews a file in the trash with Quick Look, where it lies in `.trash/<id>/`.
#[tauri::command]
pub fn trash_quick_look(app: AppHandle, svc: State<VaultService>, id: String) -> Res<()> {
    let root = root(&svc)?;
    let e = trash::list(&root).into_iter().find(|e| e.id == id).ok_or_else(|| "That item isn't in the trash any more.".to_string())?;
    let file = root.join(trash::TRASH_DIR).join(&e.id).join(&e.basename);
    let w = app.get_webview_window("main").ok_or_else(|| "No window to show Quick Look over.".to_string())?;
    Ok(crate::platform::quick_look(&w, &file)?)
}

#[tauri::command]
pub async fn trash_list(app: AppHandle) -> Res<Vec<TrashEntry>> {
    crate::blocking(move || Ok(trash::list(&root(&app.state::<VaultService>())?))).await
}

#[tauri::command]
pub async fn trash_restore(app: AppHandle, id: String, r#as: Option<String>) -> Res<String> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let root = root(&app.state::<VaultService>())?;
        let to = trash::restore(&root, &id, r#as.as_deref().filter(|s| !s.is_empty()))?;
        crate::gtd::forget_clarified(&to);
        app.state::<Drafts>().rename(&trashed_draft(&id), &to);
        written(&app, &[root.join(&to)]);
        Ok(to)
    })
    .await
}

#[tauri::command]
pub async fn trash_delete(app: AppHandle, id: String) -> Res<()> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        trash::delete(&root(&app.state::<VaultService>())?, &id)?;
        app.state::<Drafts>().discard(&trashed_draft(&id));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn trash_empty(app: AppHandle) -> Res<usize> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let root = root(&app.state::<VaultService>())?;
        let ids: Vec<String> = trash::list(&root).into_iter().map(|e| e.id).collect();
        let n = trash::empty(&root)?;
        for id in ids {
            app.state::<Drafts>().discard(&trashed_draft(&id));
        }
        Ok(n)
    })
    .await
}

/// Where a trashed note's unsaved edits wait: a draft keyed by its trash entry, which `drafts_list`
/// leaves out.
/// Tells the windows a file was renamed (`to`) or trashed (None), whoever did it: a screen, an
/// assistant, a run or a revert in Changes. The main window follows it there (src/moves.ts), and
/// hands unsaved edits still in its editor to the draft at `draft`.
fn moved(app: &AppHandle, from: &str, to: Option<&str>, draft: &str) {
    let _ = tauri::Emitter::emit(app, "file-moved", serde_json::json!({ "from": from, "to": to, "draft": draft }));
}

fn trashed_draft(id: &str) -> String {
    format!("{}{id}", brainstead_core::drafts::TRASHED)
}

static IMG_SRC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<img\b[^>]*?\ssrc=)(["'])((?:asset://localhost|https?://asset\.localhost)/[^"']+)(["'])"#).unwrap());

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime(p: &Path) -> &'static str {
    match p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

/// The file an asset-protocol URL names. macOS's URLs carry the path after the host
/// (`asset://localhost/Users/maya/x.png`, sometimes without its leading `/`); Windows' carry the
/// whole path percent-encoded, drive and all (`http://asset.localhost/C%3A%5CUsers%5Cmaya%5Cx.png`),
/// which takes no leading `/`.
fn asset_path(url: &str) -> PathBuf {
    let path = url.split_once("localhost/").map(|(_, p)| p).unwrap_or("");
    let path = percent_decode(path.split(['?', '#']).next().unwrap_or(""));
    PathBuf::from(if path.starts_with('/') || brainstead_core::paths::has_drive(&path) { path } else { format!("/{path}") })
}

/// `html` with each vault image (an asset-protocol URL) inlined as a data: URI, so it survives
/// the paste into another app. Images outside the vault are left as they are.
pub fn inline_images(html: &str, root: &Path) -> String {
    let root = brainstead_core::paths::real_path(root);
    IMG_SRC
        .replace_all(html, |c: &regex::Captures| {
            let abs = asset_path(&c[3]);
            match brainstead_core::paths::canonical(&abs)
                .ok()
                .filter(|p| p.starts_with(&root))
                .and_then(|p| std::fs::read(&p).ok().map(|b| (p, b)))
            {
                Some((p, bytes)) => format!(
                    "{}{}data:{};base64,{}{}",
                    &c[1],
                    &c[2],
                    mime(&p),
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                    &c[4]
                ),
                None => c[0].to_string(),
            }
        })
        .into_owned()
}

/// Rich copy: the rendered document's HTML (images inlined here, so WebKit's wait for a user
/// gesture doesn't matter) and its plain text.
#[tauri::command]
pub async fn copy_rich(app: AppHandle, html: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = app.state::<VaultService>().root();
        let html = match root {
            Some(r) => inline_images(&html, &r),
            None => html,
        };
        match crate::platform::copy_html(&html, &text) {
            Ok(()) => Ok(()),
            Err(_) => {
                use tauri_plugin_clipboard_manager::ClipboardExt;
                app.clipboard().write_text(text).map_err(|e| e.to_string())
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Prints the main window's page to a PDF at `dest` (from the save dialog), and waits for it.
#[tauri::command]
pub async fn export_pdf(app: AppHandle, dest: String, footer: Option<String>, forced_breaks: Option<bool>) -> Result<(), String> {
    let dest = PathBuf::from(dest);
    if dest.exists() {
        std::fs::remove_file(&dest).map_err(|e| e.to_string())?;
    }
    let w = app.get_webview_window("main").ok_or("The main window isn't open.")?;
    let d = dest.clone();
    tauri::async_runtime::spawn_blocking(move || crate::platform::print_to_pdf(&w, &d)).await.map_err(|e| e.to_string())??;
    // Done when the file is there and has stopped growing.
    let mut last = None;
    for _ in 0..240 {
        tokio_sleep(250).await;
        let size = std::fs::metadata(&dest).ok().map(|m| m.len()).filter(|n| *n > 0);
        if size.is_some() && size == last {
            // A forced page break (a split table) leaves WebKit's print a blank page at the end.
            if forced_breaks == Some(true) {
                let d = dest.clone();
                tauri::async_runtime::spawn_blocking(move || crate::platform::drop_blank_last_page(&d))
                    .await
                    .map_err(|e| e.to_string())??;
            }
            return match footer {
                Some(t) => tauri::async_runtime::spawn_blocking(move || crate::platform::stamp_pdf_footer(&dest, &t))
                    .await
                    .map_err(|e| e.to_string())?,
                None => Ok(()),
            };
        }
        last = size;
    }
    Err("The PDF didn't finish in time.".into())
}

async fn tokio_sleep(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_millis(ms))).await;
}

/// Templater's `tp.system.clipboard()`: the clipboard's text, read here (WebKit would show a
/// "Paste" callout for `navigator.clipboard.read`).
#[tauri::command]
pub fn clipboard_read(app: AppHandle) -> String {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().unwrap_or_default()
}

/// Templater's user scripts, from the folder Settings › Notes names (vault-relative).
#[tauri::command]
pub async fn user_scripts(app: AppHandle, folder: String) -> Result<Vec<brainstead_core::scripts::UserScript>, String> {
    crate::blocking(move || {
        let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
        brainstead_core::scripts::user_scripts(&root, &folder)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_isnt_utf8_isnt_saved_over() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("latin1.txt");
        std::fs::write(&f, b"caf\xe9").unwrap();
        assert!(refuse_not_utf8(&f, "latin1.txt").is_err());
        std::fs::write(&f, "café").unwrap();
        assert!(refuse_not_utf8(&f, "latin1.txt").is_ok());
        assert!(refuse_not_utf8(&d.path().join("new.md"), "new.md").is_ok());
    }

    #[test]
    fn inlines_vault_images_only() {
        let d = tempfile::tempdir().unwrap();
        let root = brainstead_core::paths::canonical(d.path()).unwrap();
        std::fs::create_dir_all(root.join("images")).unwrap();
        std::fs::write(root.join("images/a b.png"), [1u8, 2, 3]).unwrap();
        let enc = root.join("images/a b.png").to_string_lossy().replace(' ', "%20");
        let html = format!(
            r#"<p><img alt="x" src="asset://localhost/{}"> <img src="http://asset.localhost/etc/hosts"></p>"#,
            enc.trim_start_matches('/')
        );
        let out = inline_images(&html, &root);
        assert!(out.contains(r#"src="data:image/png;base64,AQID""#), "{out}");
        assert!(out.contains(r#"src="http://asset.localhost/etc/hosts""#));
    }

    #[test]
    fn asset_urls_from_either_system() {
        assert_eq!(
            asset_path("asset://localhost/Users/maya/Vault/images/a%20b.png?x=1"),
            PathBuf::from("/Users/maya/Vault/images/a b.png")
        );
        assert_eq!(asset_path("asset://localhost//Users/maya/a.png"), PathBuf::from("/Users/maya/a.png"));
        assert_eq!(
            asset_path("http://asset.localhost/C%3A%5CUsers%5Cmaya%5CVault%5Cimages%5Ca%20b.png"),
            PathBuf::from(r"C:\Users\maya\Vault\images\a b.png")
        );
    }
}
