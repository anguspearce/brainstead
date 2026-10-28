// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! One copy of the app per user session, decided before anything else starts (Windows).
//!
//! tauri-plugin-single-instance decides inside the Tauri builder: it creates a named mutex, and a
//! copy that finds the mutex already there looks for the first copy's hidden message window, hands
//! its arguments over through it (WM_COPYDATA) and exits. The window is made only after the mutex,
//! so a copy started in that gap finds the mutex taken and no window, and the plugin lets it run as
//! a full instance, by which point it has already opened the log, read the settings and started the
//! index on its way to the builder. Eight launches in one second (Defender scans each start, so they
//! overlap) leave two copies, the second with no capture shortcut.
//!
//! So the decision is made here, first thing in `run`, on the ownership of a second named mutex
//! (`<identifier>-launch`): the first copy takes it and keeps it until its process ends; any other
//! copy waits for the plugin's window in the first copy, hands its arguments over through the
//! plugin's own channel (so `tray::second_launch` runs as it would for the plugin) and exits without
//! touching the data folder. The plugin stays, as the channel and for Linux.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HWND, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, SendMessageTimeoutW, SMTO_NORMAL, WM_COPYDATA,
};

/// The plugin's payload kind for a working directory and arguments joined by `|` and ended by a
/// NUL: `WMCOPYDATA_SINGLE_INSTANCE_DATA`, the only kind 2.5.2 takes and one later versions keep
/// taking (2.5.4 adds a NUL-separated kind, 1543, that 2.5.2 ignores).
const PAYLOAD: usize = 1542;
/// How long a second copy waits for the first copy's window. The first copy is on its way to the
/// builder (settings, the log, the index) through Defender's scan of each start; a copy that is
/// still windowless after this long is stuck, and running a second one beats doing nothing.
const WINDOW_WAIT: Duration = Duration::from_secs(60);
/// How long the first copy gets to take the arguments: the plugin's own timeout.
const FORWARD_TIMEOUT_MS: u32 = 10_000;

/// Returns in the first copy, which then owns the lock until its process ends. In any other copy
/// it hands this process's working directory and arguments to the first copy and exits. When the
/// lock can't be made at all the plugin decides later, as it did before.
pub fn claim(identifier: &str) {
    let mutex_name = wide(&format!("{identifier}-launch"));
    // The plugin's class and window names (`-sic`, `-siw`), without its `semver` feature.
    let class_name = wide(&format!("{identifier}-sic"));
    let window_name = wide(&format!("{identifier}-siw"));
    // SAFETY: the names are NUL-terminated UTF-16 that outlive the call; no attributes.
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if mutex.is_null() {
        return;
    }
    let deadline = Instant::now() + WINDOW_WAIT;
    loop {
        // Ownership, not existence: a copy that has exited (or crashed) has released it, however
        // many copies still hold a handle, so a launch right after a quit takes over cleanly.
        // SAFETY: `mutex` is a handle this process owns.
        match unsafe { WaitForSingleObject(mutex, 0) } {
            // The handle is kept open, and the ownership with it, for the life of the process.
            WAIT_OBJECT_0 | WAIT_ABANDONED => return,
            WAIT_TIMEOUT => {}
            _ => return,
        }
        // SAFETY: as above.
        let hwnd = unsafe { FindWindowW(class_name.as_ptr(), window_name.as_ptr()) };
        if !hwnd.is_null() {
            forward(hwnd);
            // SAFETY: closing a handle this process owns and doesn't use again.
            unsafe { CloseHandle(mutex) };
            std::process::exit(0);
        }
        if Instant::now() >= deadline {
            // SAFETY: as above.
            unsafe { CloseHandle(mutex) };
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Hands this process's working directory and arguments to the first copy's plugin window, which
/// runs the plugin's callback with them. Lets the first copy bring its window forward first:
/// Windows grants that to the process the user started, and it can pass the right on.
fn forward(hwnd: HWND) {
    let mut pid = 0;
    // SAFETY: `hwnd` is a window found a moment ago; a stale one makes the calls fail, not crash.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != 0 {
        // SAFETY: a process id.
        unsafe { AllowSetForegroundWindow(pid) };
    }
    let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
    let data = payload(&cwd, std::env::args_os().map(|a| a.to_string_lossy().into_owned()));
    let cds = COPYDATASTRUCT { dwData: PAYLOAD, cbData: data.len() as u32, lpData: data.as_ptr() as *mut _ };
    // SAFETY: `cds` and `data` outlive the synchronous send; the receiver copies the bytes.
    unsafe {
        SendMessageTimeoutW(hwnd, WM_COPYDATA, 0, &cds as *const _ as isize, SMTO_NORMAL, FORWARD_TIMEOUT_MS, std::ptr::null_mut());
    }
}

/// The plugin's wire format: the working directory, then each argument (the program first), with a
/// `|` between them and a NUL at the end (the receiver reads a C string). A `|` can't be in a
/// Windows path, and no argument of the app's carries one.
fn payload(cwd: &str, args: impl Iterator<Item = String>) -> Vec<u8> {
    let mut data = cwd.to_owned();
    for arg in args {
        data.push('|');
        data.push_str(&arg);
    }
    data.push('\0');
    data.into_bytes()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::payload;

    #[test]
    fn payload_is_cwd_then_args_pipe_joined_nul_ended() {
        let args = ["C:\\app\\Brainstead.exe".to_owned(), "--quit".to_owned()];
        assert_eq!(payload("C:\\work", args.into_iter()), b"C:\\work|C:\\app\\Brainstead.exe|--quit\0");
        assert_eq!(payload("C:\\work", std::iter::empty()), b"C:\\work\0");
    }
}
