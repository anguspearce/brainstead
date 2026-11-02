// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screenshot scenes on Windows (tools/screenshots.py): a window kept where no screen shows it
//! yet still drawn by WebView2, and its page saved as a PNG with CapturePreview.
//!
//! WebView2 draws only a window that is shown (WS_VISIBLE): a hidden one renders nothing, and a
//! layered one at alpha 0 is not worth trusting with it. So the window is shown, but moved past
//! the right edge of the virtual screen, where no monitor is (WebView2 keeps drawing there), made
//! unfocusable (WS_EX_NOACTIVATE, so showing it takes nothing from the window in front) and kept
//! off the taskbar and out of Alt+Tab.

use std::path::Path;

use webview2_com::CapturePreviewCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
use windows::Win32::Foundation::{HGLOBAL, HWND, S_OK};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{IStream, STREAM_SEEK_SET};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, SM_CXVIRTUALSCREEN, SM_XVIRTUALSCREEN, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

/// Moves the window past the right edge of the virtual screen and keeps it from the focus, the
/// taskbar and Alt+Tab (see the module's notes). Before the window is shown; on the main thread.
pub fn make_unseen(w: &tauri::WebviewWindow) {
    // Tao's own flags, which it keeps: WS_EX_NOACTIVATE from the window not being focusable, and
    // the taskbar's button withdrawn.
    let _ = w.set_focusable(false);
    let _ = w.set_skip_taskbar(true);
    // SAFETY: GetSystemMetrics takes no pointers.
    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) } + 64;
    let _ = w.set_position(tauri::PhysicalPosition::new(x, 0));
    // WS_EX_TOOLWINDOW in place of WS_EX_APPWINDOW keeps the window out of Alt+Tab, but Tao
    // recomputes the extended styles from its flags, which have no such notion, when it shows the
    // window; so they are set once the window is visible, from a thread that waits for that.
    let shown = w.clone();
    std::thread::spawn(move || {
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if matches!(shown.is_visible(), Ok(true)) {
                let w = shown.clone();
                let _ = shown.run_on_main_thread(move || tool_window(&w));
                return;
            }
        }
    });
}

fn tool_window(w: &tauri::WebviewWindow) {
    let Ok(hwnd) = w.hwnd().map(|h| h.0 as isize) else { return };
    let hwnd = HWND(hwnd as _);
    // SAFETY: the window's own HWND, alive as long as the window is; a plain style read and write.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex & !(WS_EX_APPWINDOW.0 as isize)) | WS_EX_TOOLWINDOW.0 as isize);
    }
}

/// Saves what the window's webview shows to `dest` as a PNG at the window's own size in device
/// pixels (the display's scale times its size in points), written whole once WebView2 has
/// captured it. The capture runs on the main thread; this returns once it is asked for.
pub fn snapshot(w: &tauri::WebviewWindow, dest: &Path) -> Result<(), String> {
    let dest = dest.to_path_buf();
    w.with_webview(move |wv| {
        // SAFETY: COM calls on the live webview Tauri hands over, on the main thread it lives on.
        if let Err(e) = unsafe { capture(&wv, &dest) } {
            crate::applog!("snapshot: {e}");
        }
    })
    .map_err(|e| e.to_string())
}

unsafe fn capture(wv: &tauri::webview::PlatformWebview, dest: &Path) -> Result<(), String> {
    let core = wv.controller().CoreWebView2().map_err(|e| e.to_string())?;
    // A growable stream in memory, freed with its last reference; the handler reads it back.
    let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).map_err(|e| e.to_string())?;
    let read = stream.clone();
    let dest = dest.to_path_buf();
    let done = CapturePreviewCompletedHandler::create(Box::new(move |r| {
        // SAFETY: the completed handler runs on the webview's thread once the stream is written.
        match r.map_err(|e| e.to_string()).and_then(|()| unsafe { read_all(&read) }).and_then(|png| write_whole(&dest, &png)) {
            Ok(()) => {}
            Err(e) => crate::applog!("snapshot: CapturePreview failed ({e})"),
        }
        Ok(())
    }));
    core.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &done).map_err(|e| e.to_string())
}

/// Everything in the stream, from its start.
unsafe fn read_all(stream: &IStream) -> Result<Vec<u8>, String> {
    stream.Seek(0, STREAM_SEEK_SET, None).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut got = 0u32;
        let hr = stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut got));
        if hr.is_err() {
            return Err(hr.message());
        }
        if got == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&buf[..got as usize]);
        // S_FALSE: the end of the stream came within this read.
        if hr != S_OK {
            return Ok(out);
        }
    }
}

/// Writes the file whole: to a sibling first, then renamed into place, so a reader polling for it
/// (tools/screenshots.py) never sees a partial PNG.
fn write_whole(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let part = dest.with_extension("part");
    std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}
