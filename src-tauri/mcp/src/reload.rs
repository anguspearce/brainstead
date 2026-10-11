// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Picking up a new build. When the program on disk is replaced (an update, `make install-app`),
//! the server execs the new one in its place, on the same stdin and stdout, at a moment with no
//! request in hand, so the CLI's session carries on and is told the tools may have changed
//! (D-20261011-02). Unix only; elsewhere the server keeps the build it started with.

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Ctx;

/// What a server hands the build that replaces it, as JSON.
pub const RESUME: &str = "BRAINSTEAD_MCP_RESUME";
/// How often the program on disk is looked at, idle or between requests.
pub const CHECK_EVERY: Duration = Duration::from_secs(3);

/// The session a server hands on: the CLI has already initialized it and won't again.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    pub session: String,
    pub unattended: bool,
    /// What `initialize` agreed (protocol version, the client and its capabilities); None when
    /// the client hadn't initialized yet.
    pub init: Option<Value>,
}

impl Resume {
    pub fn of(ctx: &Ctx) -> Resume {
        Resume {
            session: ctx.session.clone(),
            unattended: ctx.unattended.load(std::sync::atomic::Ordering::SeqCst),
            init: ctx.init.lock().unwrap().clone(),
        }
    }

    /// Carries the session on in `ctx`.
    pub fn apply(self, ctx: &mut Ctx) {
        ctx.session = self.session;
        ctx.unattended = self.unattended.into();
        ctx.init = self.init.into();
    }

    /// The one handed to this process, if it replaced an older build; taken out of the
    /// environment so the programs it starts don't see it.
    pub fn take() -> Option<Resume> {
        let text = std::env::var(RESUME).ok()?;
        std::env::remove_var(RESUME);
        serde_json::from_str(&text).map_err(|e| eprintln!("brainstead mcp: couldn't read the session handed on: {e}")).ok()
    }
}

/// The program file, enough to tell it was replaced (a new inode) or rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
}

/// None while there's no whole program there: missing, not executable, or a bundle without its
/// Info.plist yet.
fn stamp(exe: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(exe).ok()?;
    if !m.is_file() || m.mode() & 0o111 == 0 || m.len() == 0 {
        return None;
    }
    let macos = exe.parent().filter(|d| d.file_name().is_some_and(|n| n == "MacOS"));
    if macos.is_some_and(|d| !d.with_file_name("Info.plist").is_file()) {
        return None;
    }
    Some(Stamp { dev: m.dev(), ino: m.ino(), len: m.len(), mtime: (m.mtime(), m.mtime_nsec()) })
}

/// Watches the program this process was started from.
pub struct Watch {
    exe: PathBuf,
    running: Stamp,
    /// The new program as last seen: it's taken once it's the same on two checks running, as an
    /// update writes it piecemeal.
    seen: Option<Stamp>,
    /// A new program that wouldn't start, not tried again until it changes.
    failed: Option<Stamp>,
    last: Instant,
}

impl Watch {
    pub fn new() -> Option<Watch> {
        Watch::of(std::env::current_exe().ok()?)
    }

    fn of(exe: PathBuf) -> Option<Watch> {
        let running = stamp(&exe)?;
        Some(Watch { exe, running, seen: None, failed: None, last: Instant::now() })
    }

    /// True when a new program has settled on disk; looks at most every CHECK_EVERY.
    pub fn due(&mut self) -> bool {
        if self.last.elapsed() < CHECK_EVERY {
            return false;
        }
        self.last = Instant::now();
        self.check()
    }

    fn check(&mut self) -> bool {
        let now = stamp(&self.exe);
        let settled = now.is_some() && now == self.seen;
        self.seen = now.filter(|s| *s != self.running && Some(*s) != self.failed);
        settled && self.seen.is_some()
    }

    /// Starts the new program in this process's place, with the same arguments and `resume`.
    /// It only comes back when that fails, and then this build carries on.
    pub fn exec(&mut self, resume: &Resume) {
        eprintln!("brainstead mcp: {} was replaced; starting the new build", self.exe.display());
        let e = exec(&self.exe, resume);
        eprintln!("brainstead mcp: the new build didn't start ({e}); carrying on with this one");
        self.failed = self.seen.take();
    }
}

/// execv, not Command's execvp, which runs a file that isn't a program with the shell instead.
fn exec(exe: &Path, resume: &Resume) -> std::io::Error {
    let c = |s: &std::ffi::OsStr| std::ffi::CString::new(s.as_bytes()).map_err(|_| std::io::Error::other("a NUL in the arguments"));
    let args: Result<Vec<_>, _> = std::iter::once(exe.as_os_str().to_owned()).chain(std::env::args_os().skip(1)).map(|a| c(&a)).collect();
    let args = match args {
        Ok(a) => a,
        Err(e) => return e,
    };
    let mut argv: Vec<*const libc::c_char> = args.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    std::env::set_var(RESUME, serde_json::to_string(resume).unwrap_or_default());
    unsafe { libc::execv(args[0].as_ptr(), argv.as_ptr()) };
    let e = std::io::Error::last_os_error();
    std::env::remove_var(RESUME);
    e
}

/// Waits up to `limit` for stdin to have something to read (or to close); false when it timed out.
pub fn readable(limit: Duration) -> std::io::Result<bool> {
    let mut fd = libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLIN, revents: 0 };
    let ms = limit.as_millis().min(i32::MAX as u128) as i32;
    match unsafe { libc::poll(&mut fd, 1, ms) } {
        -1 => {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                Ok(false)
            } else {
                Err(e)
            }
        }
        0 => Ok(false),
        _ if fd.revents & libc::POLLNVAL != 0 => Err(std::io::Error::other("stdin isn't open")),
        _ => Ok(true),
    }
}

/// Reads what's waiting on stdin, unbuffered, so nothing is left in this process that the next
/// build wouldn't see; 0 when it closed.
pub fn read(buf: &mut [u8]) -> std::io::Result<usize> {
    loop {
        let n = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
        if n >= 0 {
            return Ok(n as usize);
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_a_new_program_to_settle() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("prog");
        let write = |text: &str| {
            let _ = std::fs::remove_file(&exe);
            std::fs::write(&exe, text).unwrap();
            std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        };
        write("old");
        let mut w = Watch::of(exe.clone()).unwrap();
        assert!(!w.check(), "unchanged");
        // Mid-replacement: gone, then there but changing.
        std::fs::remove_file(&exe).unwrap();
        assert!(!w.check());
        write("new, half");
        assert!(!w.check(), "seen once");
        write("new, whole");
        assert!(!w.check(), "changed since");
        assert!(w.check(), "the same twice running");
        // One that wouldn't start isn't tried again until it changes.
        w.failed = w.seen.take();
        assert!(!w.check() && !w.check());
        write("newer");
        assert!(!w.check());
        assert!(w.check());
    }

    #[test]
    fn a_bundle_needs_its_info_plist() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Brainstead.app/Contents/MacOS");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Brainstead");
        std::fs::write(&exe, "x").unwrap();
        std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        assert!(stamp(&exe).is_none());
        std::fs::write(tmp.path().join("Brainstead.app/Contents/Info.plist"), "<plist/>").unwrap();
        assert!(stamp(&exe).is_some());
    }

    #[test]
    fn a_program_that_wont_start_comes_back() {
        let e = exec(Path::new("/nonexistent/brainstead"), &Resume::default());
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        // Not a program: an error, not the shell running it.
        let tmp = tempfile::tempdir().unwrap();
        let junk = tmp.path().join("junk");
        std::fs::write(&junk, "exit 0").unwrap();
        std::fs::set_permissions(&junk, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        assert_eq!(exec(&junk, &Resume::default()).raw_os_error(), Some(libc::ENOEXEC));
        assert!(std::env::var_os(RESUME).is_none());
    }
}
