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

pub fn watch(root: &Path, on_batch: impl Fn(Vec<PathBuf>) + Send + 'static) -> Result<Watcher, String> {
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if matches!(ev.kind, notify::EventKind::Access(_)) {
                return;
            }
            for p in ev.paths {
                let _ = tx.send(p);
            }
        }
    })
    .map_err(|e| e.to_string())?;
    inner.watch(root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("vault-watch".into())
        .spawn(move || {
            // Ends when the watcher (and with it the sender) is dropped.
            while let Ok(first) = rx.recv() {
                let mut batch = BTreeSet::from([first]);
                let started = Instant::now();
                loop {
                    let left = MAX_WAIT.saturating_sub(started.elapsed());
                    if left.is_zero() {
                        break;
                    }
                    match rx.recv_timeout(QUIET.min(left)) {
                        Ok(p) => {
                            batch.insert(p);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            on_batch(batch.into_iter().collect());
                            return;
                        }
                    }
                }
                on_batch(batch.into_iter().collect());
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
        let _w = watch(&root, move |b| tx.send(b).unwrap()).unwrap();
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
}
