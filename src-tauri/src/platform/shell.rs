// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Workspace's shell: which one a session runs through, how a command line is written for it,
//! and opening the system's own terminal on a command. Unix shells are run as login shells
//! (`-l -c`), so PATH, nvm and direnv are as in the user's own terminal; on Windows PowerShell.

use std::path::{Path, PathBuf};

/// The shell sessions run through: `over` (Settings), else the user's login shell.
pub fn login_shell(over: Option<&str>) -> PathBuf {
    if let Some(s) = over.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(s);
    }
    #[cfg(unix)]
    {
        if let Some(s) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) {
            return PathBuf::from(s);
        }
        // An app opened from the Finder may have no SHELL: the account's own, from the user database.
        // SAFETY: getpwuid returns a pointer into static storage or null; it's read at once, on this thread.
        unsafe {
            let pw = libc::getpwuid(libc::getuid());
            if !pw.is_null() && !(*pw).pw_shell.is_null() {
                let s = std::ffi::CStr::from_ptr((*pw).pw_shell).to_string_lossy().into_owned();
                if !s.is_empty() {
                    return PathBuf::from(s);
                }
            }
        }
        PathBuf::from(if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/sh" })
    }
    #[cfg(windows)]
    {
        super::find_program("pwsh", &[]).map(|p| p.exe).unwrap_or_else(|| PathBuf::from("powershell.exe"))
    }
}

/// One argument as the shell reads it back unchanged: single-quoted (PowerShell doubles a quote
/// inside; a Unix shell closes, escapes and reopens it).
pub fn quote(s: &str) -> String {
    #[cfg(windows)]
    {
        format!("'{}'", s.replace('\'', "''"))
    }
    #[cfg(not(windows))]
    {
        if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c)) {
            return s.to_string();
        }
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// A program and its arguments as one command line for the shell.
pub fn command_line(argv: &[String]) -> String {
    let words: Vec<String> = argv.iter().map(|a| quote(a)).collect();
    // PowerShell runs a quoted program name only through the call operator.
    if cfg!(windows) {
        format!("& {}", words.join(" "))
    } else {
        words.join(" ")
    }
}

/// `cmd` run in `folder`, as one line to paste into a terminal.
pub fn in_folder(folder: &Path, cmd: &str) -> String {
    let f = quote(&folder.to_string_lossy());
    if cfg!(windows) {
        format!("Set-Location -LiteralPath {f}; {cmd}")
    } else {
        format!("cd {f} && {cmd}")
    }
}

/// The program and arguments that run `cmd` through `shell` as a login shell, or with no command
/// the shell itself, interactive.
pub fn shell_argv(shell: &Path, cmd: Option<&str>) -> Vec<String> {
    let mut v = vec![shell.to_string_lossy().into_owned()];
    if cfg!(windows) {
        v.push("-NoLogo".into());
        if let Some(c) = cmd {
            v.extend(["-Command".into(), c.to_string()]);
        }
    } else {
        v.push("-l".into());
        if let Some(c) = cmd {
            v.extend(["-c".into(), c.to_string()]);
        }
    }
    v
}

/// Opens the system's terminal running `cmd` (already `in_folder`), leaving a shell open after it:
/// Terminal through a `.command` file on macOS; Windows Terminal, else a PowerShell window; on
/// Linux the Debian alternative `x-terminal-emulator`, else a common one.
pub fn open_terminal(shell: &Path, folder: &Path, cmd: &str) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("Couldn't open a terminal: {e}");
    #[cfg(target_os = "macos")]
    {
        let _ = folder;
        use std::os::unix::fs::PermissionsExt;
        let dir = super::data_dir().join("workspace");
        std::fs::create_dir_all(&dir).map_err(fail)?;
        // Each opening has its own file; yesterday's are gone.
        let day = std::time::Duration::from_secs(86_400);
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a > day);
            if old && e.path().extension().is_some_and(|x| x == "command") {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let file = dir.join(format!("open-{stamp}.command"));
        let sh = quote(&shell.to_string_lossy());
        let script = format!("#!/bin/sh\nexec {sh} -l -c {}\n", quote(&format!("{cmd}; exec {sh} -l")));
        std::fs::write(&file, script).map_err(fail)?;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
        std::process::Command::new("open").args(["-a", "Terminal"]).arg(&file).spawn().map_err(fail)?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let sh = shell.to_string_lossy().into_owned();
        let args = ["-NoLogo", "-NoExit", "-Command", cmd];
        if super::find_program("wt", &[]).is_some() {
            std::process::Command::new("wt.exe").arg("-d").arg(folder).arg(&sh).args(args).spawn().map_err(fail)?;
        } else {
            std::process::Command::new("cmd").args(["/c", "start", ""]).arg(&sh).args(args).current_dir(folder).spawn().map_err(fail)?;
        }
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let sh = shell.to_string_lossy().into_owned();
        let run = format!("{cmd}; exec {} -l", quote(&sh));
        let tries: [(&str, &[&str]); 5] = [
            ("x-terminal-emulator", &["-e"]),
            ("gnome-terminal", &["--"]),
            ("konsole", &["-e"]),
            ("xfce4-terminal", &["-x"]),
            ("xterm", &["-e"]),
        ];
        let mut last = None;
        for (term, pre) in tries {
            match std::process::Command::new(term).args(pre).arg(&sh).args(["-l", "-c", &run]).current_dir(folder).spawn() {
                Ok(_) => return Ok(()),
                Err(e) => last = Some(e),
            }
        }
        Err(fail(last.unwrap_or_else(|| std::io::Error::other("no terminal found"))))
    }
}

/// Stops a process group at once (SIGKILL), for a session that didn't end when asked.
pub fn kill_group(pid: u32) {
    #[cfg(unix)]
    // SAFETY: kill(2) takes no pointers; a negative pid addresses the process group.
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
    #[cfg(windows)]
    super::stop_group(pid);
}

/// Whether a process is still there, or with `group` any process in its group: kill(pid, 0), which
/// signals nothing. On Windows it's taken as gone: `stop_group` there already ends the whole tree
/// (taskkill /T /F), and a pid used again by another program is never signalled.
pub fn alive(pid: u32, group: bool) -> bool {
    #[cfg(unix)]
    {
        let target = if group { -(pid as libc::pid_t) } else { pid as libc::pid_t };
        // SAFETY: kill(2) with signal 0 takes no pointers and only checks the process exists.
        let r = unsafe { libc::kill(target, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        let _ = (pid, group);
        false
    }
}

/// A terminal session's foreground process group, when it isn't the session's own: a job an
/// interactive shell started, which the shell's group signal doesn't reach.
pub fn pty_foreground(master: &dyn portable_pty::MasterPty, pid: u32) -> Option<u32> {
    #[cfg(unix)]
    {
        master.process_group_leader().filter(|g| *g > 0 && *g as u32 != pid).map(|g| g as u32)
    }
    #[cfg(windows)]
    {
        let _ = (master, pid);
        None
    }
}

/// A pty's output, read on its own thread. On Unix it waits for output a quarter of a second at a
/// time, so the thread ends when it's told to even while something the session started still
/// holds the terminal open (its own copy of the pty is then closed, and with the master gone the
/// terminal hangs up). On Windows, closing the pseudoconsole ends the read.
pub struct PtyReader {
    #[cfg(unix)]
    file: std::fs::File,
    #[cfg(windows)]
    inner: Box<dyn std::io::Read + Send>,
}

impl PtyReader {
    pub fn new(master: &dyn portable_pty::MasterPty) -> std::io::Result<PtyReader> {
        #[cfg(unix)]
        {
            use std::os::fd::{BorrowedFd, OwnedFd};
            let fd = master.as_raw_fd().ok_or_else(|| std::io::Error::other("the terminal has no descriptor"))?;
            // SAFETY: the master is open for the length of this call; the copy is owned from here.
            let owned: OwnedFd = unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?;
            Ok(PtyReader { file: std::fs::File::from(owned) })
        }
        #[cfg(windows)]
        {
            Ok(PtyReader { inner: master.try_clone_reader().map_err(|e| std::io::Error::other(e.to_string()))? })
        }
    }

    /// Bytes read into `buf`; 0 at the end of the output, or once `stop` is set.
    pub fn read(&mut self, buf: &mut [u8], stop: &std::sync::atomic::AtomicBool) -> std::io::Result<usize> {
        use std::io::Read;
        use std::sync::atomic::Ordering;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            loop {
                if stop.load(Ordering::SeqCst) {
                    return Ok(0);
                }
                let mut p = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
                // SAFETY: one valid pollfd, for the length of the call.
                let r = unsafe { libc::poll(&mut p, 1, 250) };
                if r < 0 {
                    let e = std::io::Error::last_os_error();
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                }
                if r > 0 {
                    // Output, or the other end closed (a read then ends it: EIO on Linux).
                    return match self.file.read(buf) {
                        Ok(n) => Ok(n),
                        Err(e) if e.raw_os_error() == Some(libc::EIO) => Ok(0),
                        Err(e) => Err(e),
                    };
                }
            }
        }
        #[cfg(windows)]
        {
            if stop.load(Ordering::SeqCst) {
                return Ok(0);
            }
            self.inner.read(buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn command_lines_quote_what_the_shell_would_change() {
        assert_eq!(quote("claude"), "claude");
        assert_eq!(quote("/Users/maya/Orbit App"), "'/Users/maya/Orbit App'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote(r#"{"a":1}"#), r#"'{"a":1}'"#);
        let line = command_line(&["claude".into(), "--resume".into(), "abc".into(), "Fix Maya's bug".into()]);
        assert_eq!(line, "claude --resume abc 'Fix Maya'\\''s bug'");
        assert_eq!(in_folder(Path::new("/r/orbit app"), "claude"), "cd '/r/orbit app' && claude");
        assert_eq!(shell_argv(Path::new("/bin/zsh"), Some("claude")), ["/bin/zsh", "-l", "-c", "claude"]);
        assert_eq!(shell_argv(Path::new("/bin/zsh"), None), ["/bin/zsh", "-l"]);
        assert_eq!(login_shell(Some(" /opt/fish ")), PathBuf::from("/opt/fish"));
        assert!(!login_shell(None).as_os_str().is_empty());
    }

    /// The quoting survives a real shell: each argument comes back as it went in.
    #[cfg(unix)]
    #[test]
    fn a_shell_reads_the_arguments_back() {
        let args: Vec<String> =
            ["printf", "%s\\n", "plain", "with space", "it's", r#"{"mcpServers":{"x":1}}"#, "$HOME", "a;b"].map(String::from).to_vec();
        let out = std::process::Command::new("/bin/sh").args(["-c", &command_line(&args)]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "plain\nwith space\nit's\n{\"mcpServers\":{\"x\":1}}\n$HOME\na;b\n");
    }
}
