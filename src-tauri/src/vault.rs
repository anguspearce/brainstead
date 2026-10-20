// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The open vault: its index, kept current by the file watcher. Indexing runs on its own thread;
//! the window hears about it through `index-status` and `vault-changed` events.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use brainstead_core::index::Changes;
use brainstead_core::vault::Vault;
use brainstead_core::{Index, IndexStats};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// "none" (no vault chosen), "indexing", "ready" or "error".
    pub state: &'static str,
    pub vault_path: Option<String>,
    pub error: Option<String>,
    pub stats: IndexStats,
}

struct Open {
    vault: Vault,
    index: Arc<Mutex<Index>>,
    /// Files by name, for embeds; built when first asked for and dropped when the vault changes.
    assets: Arc<Mutex<Option<Arc<brainstead_core::assets::Assets>>>>,
    _watcher: Option<brainstead_core::watch::Watcher>,
}

#[derive(Default)]
pub struct VaultService {
    open: Mutex<Option<Open>>,
    status: Mutex<Status>,
    /// Bumped on every (re)start, so a slow first index of an old vault can't overwrite the new one.
    generation: Mutex<u64>,
}

fn emit_status(app: &AppHandle, s: &Status) {
    let _ = app.emit("index-status", s);
}

/// The status `start` gives before indexing: "indexing" with the vault's path, or "none".
fn first_status(vault_path: Option<String>) -> Status {
    Status { state: if vault_path.is_some() { "indexing" } else { "none" }, vault_path, ..Default::default() }
}

impl VaultService {
    /// The service before `start` has run, already saying what `start` will. Tauri makes the
    /// windows before the app's setup reaches `start`, and on Windows WebView2 answers the main
    /// window's calls while the other windows are still being made, so the window can ask for the
    /// status first. An empty state there throws in the sidebar and leaves the window blank.
    pub fn opening(vault_path: Option<String>) -> Self {
        VaultService { status: Mutex::new(first_status(vault_path)), ..Default::default() }
    }

    pub fn status(&self) -> Status {
        crate::lock(&self.status).clone()
    }

    fn set_status(&self, app: &AppHandle, gen: u64, s: Status) {
        if *crate::lock(&self.generation) != gen {
            return;
        }
        *crate::lock(&self.status) = s.clone();
        emit_status(app, &s);
    }

    /// Opens the vault (or closes it, with None) and indexes it in the background. `rebuild`
    /// empties the index first.
    pub fn start(&self, app: &AppHandle, vault_path: Option<String>, excluded: Vec<String>, rebuild: bool) {
        let gen = {
            let mut g = crate::lock(&self.generation);
            *g += 1;
            *g
        };
        // The old watcher stops here.
        *crate::lock(&self.open) = None;
        self.set_status(app, gen, first_status(vault_path.clone()));
        let Some(path) = vault_path else { return };
        let app = app.clone();
        std::thread::Builder::new()
            .name("vault-open".into())
            .spawn(move || {
                let svc = app.state::<VaultService>();
                let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| open(&app, &path, excluded, rebuild)))
                    .unwrap_or_else(|p| Err(format!("Indexing the vault stopped on an error: {}", panic_text(&p))));
                match opened {
                    Ok(o) => {
                        let stats = crate::lock(&o.index).stats().unwrap_or_default();
                        let (index, vault) = (o.index.clone(), o.vault.clone());
                        if *crate::lock(&svc.generation) == gen {
                            *crate::lock(&svc.open) = Some(o);
                        }
                        svc.set_status(&app, gen, Status { state: "ready", vault_path: Some(path), error: None, stats });
                        read_images(&app, gen, &index, &vault);
                    }
                    Err(e) => {
                        svc.set_status(&app, gen, Status { state: "error", vault_path: Some(path), error: Some(e), ..Default::default() })
                    }
                }
            })
            .expect("can start a thread");
    }

    /// Runs `f` on the open index, or fails when no vault is open (yet).
    pub fn with_index<T>(&self, f: impl FnOnce(&Index, &Vault) -> Result<T, String>) -> Result<T, String> {
        let (index, vault) = {
            let g = crate::lock(&self.open);
            let o = g.as_ref().ok_or("The vault isn't open yet.")?;
            (o.index.clone(), o.vault.clone())
        };
        let ix = crate::lock(&index);
        f(&ix, &vault)
    }

    /// Re-indexes files Brainstead has just written.
    pub fn refresh(&self, paths: &[PathBuf]) -> Result<Changes, String> {
        let (index, vault) = {
            let g = crate::lock(&self.open);
            let o = g.as_ref().ok_or("The vault isn't open yet.")?;
            (o.index.clone(), o.vault.clone())
        };
        let ch = crate::lock(&index).refresh(&vault, paths)?;
        Ok(ch)
    }

    /// Where a file embedded by name is (`![[diagram.png]]`).
    pub fn find_asset(&self, name: &str) -> Result<Option<String>, String> {
        let (cell, vault) = {
            let g = crate::lock(&self.open);
            let o = g.as_ref().ok_or("The vault isn't open yet.")?;
            (o.assets.clone(), o.vault.clone())
        };
        let assets = {
            let mut c = crate::lock(&cell);
            c.get_or_insert_with(|| Arc::new(brainstead_core::assets::Assets::scan(&vault.root, &vault.excluded))).clone()
        };
        Ok(assets.find(name).map(String::from))
    }

    /// A vault-relative path as a full path, refusing anything that would leave the vault.
    pub fn resolve_path(&self, rel: &str) -> Result<PathBuf, String> {
        let root = self.root().ok_or("No vault is open.")?;
        if !brainstead_core::paths::is_vault_relative(rel) {
            return Err(format!("Not a path in the vault: {rel}"));
        }
        Ok(root.join(rel))
    }

    /// The vault's root folder, when one is open.
    pub fn root(&self) -> Option<PathBuf> {
        crate::lock(&self.open).as_ref().map(|o| o.vault.root.clone())
    }
}

fn open(app: &AppHandle, path: &str, excluded: Vec<String>, rebuild: bool) -> Result<Open, String> {
    let root = PathBuf::from(path);
    // Reading the folder is what Full Disk Access (or the folder's own permission) allows.
    std::fs::read_dir(&root).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("The vault folder isn't there any more: {path}"),
        std::io::ErrorKind::PermissionDenied => {
            "Brainstead isn't allowed to read the vault folder. Give it Full Disk Access, or choose the folder again.".into()
        }
        _ => format!("Couldn't read the vault folder: {e}"),
    })?;
    // FSEvents reports real paths, so the vault is known by its real path too (on Windows without
    // the `\\?\` prefix, under which nested vault-relative paths don't join).
    let root = brainstead_core::paths::real_path(&root);
    if let Err(e) = app.asset_protocol_scope().allow_directory(&root, true) {
        crate::applog!("asset scope: {e}");
    }
    let vault = Vault::new(root.clone(), excluded);
    let mut ix = Index::open(&crate::platform::data_dir().join("index.db"))?;
    // Images are read after the vault opens (`read_images`): Vision's first read in a session
    // takes a while, and the vault would wait for it. What's kept by content hash is used now.
    ix.set_text_cache(brainstead_core::extract::TextCache::new(text_dir(), Some(crate::platform::pdf_text)));
    let root_s = root.to_string_lossy().to_string();
    if rebuild || ix.meta("vault_root")?.as_deref() != Some(root_s.as_str()) {
        ix.clear()?;
        ix.set_meta("vault_root", &root_s)?;
    }
    // Excluded folders changed since last time: drop what's now excluded (sync removes what walk no longer finds).
    ix.sync(&vault)?;
    ix.set_text_cache(text_cache());
    let index = Arc::new(Mutex::new(ix));
    let assets: Arc<Mutex<Option<Arc<brainstead_core::assets::Assets>>>> = Default::default();
    let watcher = {
        let (index, vault, app, assets) = (index.clone(), vault.clone(), app.clone(), assets.clone());
        let text = text_cache();
        brainstead_core::watch::watch(&root, move |paths| {
            // A file that makes the parser panic is logged and skipped with its batch; the watcher
            // and the app carry on (the index's transaction rolls back as the panic unwinds).
            let batch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                *crate::lock(&assets) = None;
                // PDF, Office and image text is read before the index is locked, so the window's calls
                // don't wait on it; the update then finds it kept by content hash.
                extract_ahead(&text, &paths);
                crate::lock(&index).update(&vault, &paths)
            }));
            match batch {
                Ok(Ok(ch)) if !ch.is_empty() => changed(&app, &index, ch),
                Ok(Ok(_)) => {}
                Ok(Err(e)) => crate::applog!("index update: {e}"),
                Err(p) => crate::applog!("index update panicked on {} file(s), first {:?}: {}", paths.len(), paths.first(), panic_text(&p)),
            }
        })
        .map_err(|e| crate::applog!("vault watcher: {e}"))
        .ok()
    };
    Ok(Open { vault, index, assets, _watcher: watcher })
}

/// Reads the text of the PDF, Office and image files in a batch into the text cache, outside the index lock.
fn extract_ahead(cache: &brainstead_core::extract::TextCache, paths: &[PathBuf]) {
    for p in paths {
        if brainstead_core::extract::kind_of(&p.to_string_lossy()).is_none() || !p.is_file() {
            continue;
        }
        if let Ok(bytes) = std::fs::read(p) {
            cache.get(p, &brainstead_core::write::version(&bytes));
        }
    }
}

/// Reads the text in the vault's images that isn't kept yet, after the vault has opened, and
/// re-indexes them a few at a time so search finds them. Stops when another vault opens.
fn read_images(app: &AppHandle, gen: u64, index: &Arc<Mutex<Index>>, vault: &Vault) {
    let cache = text_cache();
    let svc = app.state::<VaultService>();
    let unread: Vec<PathBuf> = vault
        .walk()
        .into_iter()
        .filter(|f| brainstead_core::extract::kind_of(&f.rel) == Some("image"))
        .filter(|f| {
            std::fs::read(&f.abs)
                .is_ok_and(|b| brainstead_core::extract::TextCache::lookup(&cache.dir, &brainstead_core::write::version(&b)).is_none())
        })
        .map(|f| f.abs)
        .collect();
    for batch in unread.chunks(20) {
        if *crate::lock(&svc.generation) != gen {
            return;
        }
        extract_ahead(&cache, batch);
        let ch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::lock(index).refresh(vault, batch)));
        match ch {
            Ok(Ok(ch)) if !ch.is_empty() => changed(app, index, ch),
            Ok(Ok(_)) => {}
            Ok(Err(e)) => crate::applog!("image text: {e}"),
            Err(p) => crate::applog!("image text panicked: {}", panic_text(&p)),
        }
    }
}

/// A panic's message, for the log.
pub(crate) fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "(no message)".into())
}

fn changed(app: &AppHandle, index: &Arc<Mutex<Index>>, ch: Changes) {
    let _ = app.emit("vault-changed", &ch);
    crate::knowledge::vault_changed(app);
    let svc = app.state::<VaultService>();
    let stats = crate::lock(index).stats().unwrap_or_default();
    let mut s = crate::lock(&svc.status);
    s.stats = stats;
    emit_status(app, &s);
}

/// Where PDF, Office and image sources' text is kept, by content hash (stage 7b).
pub fn text_dir() -> std::path::PathBuf {
    crate::platform::data_dir().join("text")
}

/// The text cache with this system's PDF and image readers. Screenshot mode reads no images: the
/// shots don't need their text, and many apps starting at once each ran text recognition (decided
/// 2026-10-04).
pub fn text_cache() -> brainstead_core::extract::TextCache {
    let image: Option<brainstead_core::extract::ImageReader> =
        if crate::scene().is_some() { None } else { Some(crate::platform::image_text) };
    brainstead_core::extract::TextCache::new(text_dir(), Some(crate::platform::pdf_text)).with_image(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window can read the status before `start` runs, and the sidebar draws one of the four
    /// states: never the empty one `Status::default()` has.
    #[test]
    fn the_status_before_start_is_what_start_sets_first() {
        let s = VaultService::opening(Some("/vault".into())).status();
        assert_eq!((s.state, s.vault_path.as_deref()), ("indexing", Some("/vault")));
        let s = VaultService::opening(None).status();
        assert_eq!((s.state, s.vault_path), ("none", None));
    }
}
