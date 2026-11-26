// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Watches the vault folder (FSEvents on macOS, through `notify`) and hands over the paths that
//! changed in batches: a batch is sent once events stop for `QUIET`, or after `MAX_WAIT` while they
//! keep coming (a sync client writing hundreds of files).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher as _};

const QUIET: Duration = Duration::from_millis(300);
const MAX_WAIT: Duration = Duration::from_secs(2);

/// Stops watching when dropped.
pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

/// What the watcher hands over: the paths that changed, or word that changes were missed (the
/// system's event buffer overflowed, or the watcher reported an error), so the whole vault wants
/// reading again.
#[derive(Debug, PartialEq, Eq)]
pub enum Batch {
    Paths(Vec<PathBuf>),
    Rescan,
}

/// What one event from the system means: its paths, or None for "changes were missed". An error
/// counts as missed too: after one nothing says which files it was about.
fn signals(res: notify::Result<notify::Event>) -> Vec<Option<PathBuf>> {
    match res {
        Ok(ev) if ev.need_rescan() => vec![None],
        Ok(ev) if matches!(ev.kind, notify::EventKind::Access(_)) => Vec::new(),
        Ok(ev) => ev.paths.into_iter().map(Some).collect(),
        Err(_) => vec![None],
    }
}

/// Paths gathered into a batch; one "changes were missed" makes the whole batch a rescan, which
/// covers its paths too.
#[derive(Default)]
struct Gathered {
    paths: BTreeSet<PathBuf>,
    rescan: bool,
}

impl Gathered {
    fn add(&mut self, signal: Option<PathBuf>) {
        match signal {
            Some(p) => {
                self.paths.insert(p);
            }
            None => self.rescan = true,
        }
    }

    fn batch(self) -> Batch {
        if self.rescan {
            Batch::Rescan
        } else {
            Batch::Paths(self.paths.into_iter().collect())
        }
    }
}

pub fn watch(root: &Path, on_batch: impl Fn(Batch) + Send + 'static) -> Result<Watcher, String> {
    let (tx, rx) = mpsc::channel::<Option<PathBuf>>();
    let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        for s in signals(res) {
            let _ = tx.send(s);
        }
    })
    .map_err(|e| e.to_string())?;
    inner.watch(root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("vault-watch".into())
        .spawn(move || {
            // Ends when the watcher (and with it the sender) is dropped.
            while let Ok(first) = rx.recv() {
                let mut batch = Gathered::default();
                batch.add(first);
                let started = Instant::now();
                loop {
                    let left = MAX_WAIT.saturating_sub(started.elapsed());
                    if left.is_zero() {
                        break;
                    }
                    match rx.recv_timeout(QUIET.min(left)) {
                        Ok(s) => batch.add(s),
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            on_batch(batch.batch());
                            return;
                        }
                    }
                }
                on_batch(batch.batch());
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Watcher { _inner: inner })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_changes() {
        let t = tempfile::tempdir().unwrap();
        // FSEvents reports the real path (/private/var/... for /var/...).
        let root = crate::paths::canonical(t.path()).unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = watch(&root, move |b| {
            if let Batch::Paths(b) = b {
                tx.send(b).unwrap()
            }
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(root.join("a.md"), "one").unwrap();
        std::fs::write(root.join("b.md"), "two").unwrap();
        let mut seen = BTreeSet::new();
        let until = Instant::now() + Duration::from_secs(5);
        while seen.len() < 2 && Instant::now() < until {
            if let Ok(b) = rx.recv_timeout(Duration::from_millis(500)) {
                seen.extend(b.into_iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())));
            }
        }
        assert!(seen.contains("a.md") && seen.contains("b.md"), "{seen:?}");
    }

    /// A dropped-events notice or a watcher error asks for the whole vault to be read again,
    /// whatever else is in the batch; without one the batch is its paths.
    #[test]
    fn missed_changes_ask_for_a_rescan() {
        let change = |p: &str| notify::Event::new(notify::EventKind::Any).add_path(PathBuf::from(p));
        assert_eq!(signals(Ok(change("/v/a.md"))), vec![Some(PathBuf::from("/v/a.md"))]);
        assert_eq!(signals(Ok(change("/v/a.md").set_flag(notify::event::Flag::Rescan))), vec![None]);
        assert_eq!(signals(Err(notify::Error::generic("overflow"))), vec![None]);
        let access = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Any)).add_path(PathBuf::from("/v/a.md"));
        assert!(signals(Ok(access)).is_empty());

        let mut g = Gathered::default();
        g.add(Some(PathBuf::from("/v/b.md")));
        g.add(Some(PathBuf::from("/v/a.md")));
        g.add(Some(PathBuf::from("/v/a.md")));
        assert_eq!(g.batch(), Batch::Paths(vec![PathBuf::from("/v/a.md"), PathBuf::from("/v/b.md")]));
        let mut g = Gathered::default();
        g.add(Some(PathBuf::from("/v/a.md")));
        g.add(None);
        assert_eq!(g.batch(), Batch::Rescan);
    }
}
