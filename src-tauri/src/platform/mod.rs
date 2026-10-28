// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything that differs between operating systems lives here, so the rest of the app builds
//! for Windows and Linux too. macOS has the full implementation; elsewhere the fallbacks are
//! simple or say the feature isn't there yet.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

#[cfg(target_os = "macos")]
mod about;
#[cfg(windows)]
mod image_win;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod now_playing;
#[cfg(target_os = "macos")]
mod quick_look;
#[cfg(windows)]
mod snap_win;
#[cfg(target_os = "macos")]
mod speech;
#[cfg(target_os = "macos")]
mod spell;
#[cfg(windows)]
#[path = "spell_win.rs"]
mod spell;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as imp;

mod tray;
pub use tray::{place_tray_window, tray_icon, tray_window_resized};

/// Where Brainstead keeps settings.json and index.db: `~/Library/Application Support/Brainstead/`
/// on macOS, `%APPDATA%\Brainstead` on Windows. `BRAINSTEAD_DATA` overrides it (screenshots,
/// tests). The rule is in core, which the standalone MCP server shares.
pub fn data_dir() -> PathBuf {
    brainstead_core::paths::data_dir()
}

/// The About panel with `link` under the copyright, clickable, opening `url`: macOS's standard
/// panel. Elsewhere the menu keeps the platform's own About item, so this does nothing. On the
/// main thread.
pub fn about_panel(name: &str, version: &str, copyright: &str, icon_png: &[u8], link: &str, url: &str) {
    #[cfg(target_os = "macos")]
    about::show(name, version, copyright, icon_png, link, url);
    #[cfg(not(target_os = "macos"))]
    let _ = (name, version, copyright, icon_png, link, url);
}

/// Where the page's maximise button is (x, y, width, height in physical pixels of the window's
/// client area), so Windows 11 offers its snap layouts over it (platform/snap_win.rs); `None` when
/// it isn't shown. Nothing elsewhere: macOS has its own button, Linux its own title bar.
pub fn set_max_button(w: &tauri::WebviewWindow, rect: Option<(i32, i32, i32, i32)>) {
    #[cfg(windows)]
    snap_win::set_max_button(w, rect);
    #[cfg(not(windows))]
    let _ = (w, rect);
}

/// A quick preview of a file without opening it: the Quick Look panel over `w` on macOS (as Finder's
/// space bar shows it), the file's default app elsewhere.
pub fn quick_look(w: &tauri::WebviewWindow, path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let ns = w.ns_window().map_err(|e| e.to_string())? as usize;
        let file = path.to_path_buf();
        w.run_on_main_thread(move || {
            let mtm = objc2::MainThreadMarker::new().expect("on the main thread");
            // Tauri's own NSWindow, alive as long as the window is.
            let window = unsafe { &*(ns as *const objc2_app_kit::NSWindow) };
            quick_look::show(mtm, window, &file);
        })
        .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = w;
        imp::quick_look(path)
    }
}

/// Shows the window once its webview is painted in `rgb`, so no white frame flashes first (a
/// sibling app's approach). On macOS the webview's own under-page colour is set and its background
/// switched off before showing; if the webview never answers, the window is shown anyway. `focus`
/// brings it to the front.
pub fn show_painted(w: &tauri::WebviewWindow, rgb: (u8, u8, u8), focus: bool) {
    let (r, g, b) = rgb;
    let _ = w.set_background_color(Some(tauri::window::Color(r, g, b, 255)));
    #[cfg(target_os = "macos")]
    {
        let shown = w.clone();
        let _ = w.with_webview(move |wv| {
            unsafe {
                use objc2::runtime::AnyObject;
                let webview: *mut AnyObject = wv.inner() as *mut AnyObject;
                let color =
                    objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0);
                let _: () = objc2::msg_send![webview, setUnderPageBackgroundColor: &*color];
                let no = objc2_foundation::NSNumber::new_bool(false);
                let key = objc2_foundation::NSString::from_str("drawsBackground");
                let _: () = objc2::msg_send![webview, setValue: &*no, forKey: &*key];
            }
            let _ = shown.show();
            if focus {
                let _ = shown.set_focus();
            }
        });
        let late = w.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            if matches!(late.is_visible(), Ok(false)) {
                let _ = late.show();
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = w.show();
        if focus {
            let _ = w.set_focus();
        }
    }
}

/// Makes a window invisible and click-through while it still draws, for screenshots
/// (tools/screenshots.py captures its contents): nothing flashes on screen. macOS only; elsewhere
/// the window shows as usual.
pub fn make_unseen(w: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ns) = w.ns_window() {
        // Tauri's own NSWindow, alive as long as the window is; setup runs on the main thread.
        let window = unsafe { &*(ns as *const objc2_app_kit::NSWindow) };
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
    }
    // WebKit treats an invisible window as hidden: no animation frames, so CodeMirror never places
    // its tooltips. WKWebView's private switch for that, when it has it.
    #[cfg(target_os = "macos")]
    let _ = w.with_webview(|wv| unsafe {
        use objc2::runtime::{AnyObject, Sel};
        let webview = wv.inner() as *mut AnyObject;
        let sel = Sel::register(c"_setWindowOcclusionDetectionEnabled:");
        let can: bool = objc2::msg_send![webview, respondsToSelector: sel];
        if can {
            let _: () = objc2::msg_send![webview, _setWindowOcclusionDetectionEnabled: false];
        }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = w;
}

/// Saves what the window's webview shows to `dest` as a TIFF at twice its size, for screenshots taken unseen
/// (make_unseen: an invisible window's own capture is blank, its webview's snapshot isn't). The file
/// appears when WebKit has drawn it. macOS only.
pub fn snapshot(w: &tauri::WebviewWindow, dest: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let dest = dest.to_path_buf();
        w.with_webview(move |wv| unsafe {
            use objc2::runtime::AnyObject;
            let webview = wv.inner() as *mut AnyObject;
            let done = block2::RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
                if image.is_null() {
                    return;
                }
                let tiff: *mut AnyObject = objc2::msg_send![image, TIFFRepresentation];
                if !tiff.is_null() {
                    let path = objc2_foundation::NSString::from_str(&dest.to_string_lossy());
                    let _: bool = objc2::msg_send![tiff, writeToFile: &*path, atomically: true];
                }
            });
            // Twice the view's width, as a Retina screen draws it: the docs scale it down, the
            // product page keeps it.
            let config: *mut AnyObject = objc2::msg_send![objc2::class!(WKSnapshotConfiguration), new];
            let bounds: objc2_foundation::NSRect = objc2::msg_send![webview, bounds];
            let width = objc2_foundation::NSNumber::new_f64(bounds.size.width * 2.0);
            let _: () = objc2::msg_send![config, setSnapshotWidth: &*width];
            let _: () = objc2::msg_send![webview, takeSnapshotWithConfiguration: config, completionHandler: &*done];
        })
        .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (w, dest);
        Err("Snapshots are macOS only.".into())
    }
}

/// Gives the front back to the app that had it before a window of ours was shown over it: macOS
/// hides the app; elsewhere hiding the window already does that.
pub fn give_back_front(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Puts the app in the Dock or takes it out, so it runs from the menu bar alone (macOS); elsewhere
/// there's no Dock to leave.
pub fn set_in_dock(app: &tauri::AppHandle, shown: bool) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(if shown { tauri::ActivationPolicy::Regular } else { tauri::ActivationPolicy::Accessory });
    #[cfg(not(target_os = "macos"))]
    let _ = (app, shown);
}

/// Whether macOS opened the app as a login item (the launch event's `keyAELaunchedAsLogInItem`).
/// False when it can't tell, so the window shows as it always has.
pub fn launched_at_login() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::{AnyClass, AnyObject};
        // 'prdt' (keyAEPropData) and 'lgit' (keyAELaunchedAsLogInItem), as four-character codes.
        const PROP_DATA: u32 = u32::from_be_bytes(*b"prdt");
        const LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");
        let Some(cls) = AnyClass::get(c"NSAppleEventManager") else { return false };
        unsafe {
            let mgr: *mut AnyObject = objc2::msg_send![cls, sharedAppleEventManager];
            if mgr.is_null() {
                return false;
            }
            let ev: *mut AnyObject = objc2::msg_send![mgr, currentAppleEvent];
            if ev.is_null() {
                return false;
            }
            let prop: *mut AnyObject = objc2::msg_send![ev, paramDescriptorForKeyword: PROP_DATA];
            if prop.is_null() {
                return false;
            }
            let code: u32 = objc2::msg_send![prop, enumCodeValue];
            code == LOGIN_ITEM
        }
    }
    #[cfg(not(target_os = "macos"))]
    false
}

/// Brings the app to the front: coming back from the menu bar alone, focusing the window can
/// leave it behind whatever the user was working in (macOS).
pub fn activate() {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::{AnyClass, AnyObject};
        let Some(cls) = AnyClass::get(c"NSApplication") else { return };
        unsafe {
            let nsapp: *mut AnyObject = objc2::msg_send![cls, sharedApplication];
            if !nsapp.is_null() {
                let _: () = objc2::msg_send![nsapp, activateIgnoringOtherApps: true];
            }
        }
    }
}

/// Shows the file selected in Finder (or the system's file manager).
/// Whether the app opens at login; None when the system can't say (Settings hides the switch).
pub fn open_at_login() -> Option<bool> {
    imp::open_at_login()
}

pub fn set_open_at_login(on: bool) -> Result<(), String> {
    imp::set_open_at_login(on)
}

pub fn reveal(path: &Path) -> Result<(), String> {
    imp::reveal(path)
}

/// Whether Brainstead has Full Disk Access: Some(false) when a protected file can't be opened,
/// None when there was nothing to test against. Always Some(true) where there's no such thing.
pub fn full_disk_access() -> Option<bool> {
    imp::full_disk_access()
}

/// Opens System Settings at Privacy & Security › Full Disk Access.
pub fn open_full_disk_access_settings() -> Result<(), String> {
    imp::open_full_disk_access_settings()
}

/// HTML and plain text onto the clipboard together (rich copy).
pub fn copy_html(html: &str, text: &str) -> Result<(), String> {
    imp::copy_html(html, text)
}

/// Adds the running footer (title, "Page N of M") to every page of an exported PDF. Elsewhere
/// the PDF is left as it is.
/// A PDF's text, one string per page: PDFKit on macOS, a pure-Rust reader elsewhere. The index
/// takes it as its PDF reader (`brainstead_core::extract::PdfReader`).
pub fn pdf_text(path: &Path) -> Result<Vec<String>, String> {
    #[cfg(target_os = "macos")]
    return macos::pdf_text(path);
    #[cfg(not(target_os = "macos"))]
    return pdf_extract::extract_text_by_pages(path).map_err(|e| e.to_string());
}

/// The text in an image (OCR), one line per line found: Vision's text recognition on macOS, which
/// reads HEIC too, and Windows' own on Windows (image_win.rs). Elsewhere it isn't available yet. The index takes it as its image reader
/// (`brainstead_core::extract::ImageReader`).
pub fn image_text(path: &Path) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    return macos::image_text(path);
    #[cfg(windows)]
    return image_win::image_text(path);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        Err("Reading the text in images isn't available on this system yet.".into())
    }
}

/// The image as a model is shown it: a JPEG at most 2000 pixels across, kept as
/// `ask/img/<sha>.jpg` in the app data folder (`image_dir`): sips on macOS, Windows' imaging on
/// Windows. Elsewhere it isn't available yet.
pub fn image_for_model(path: &Path, sha: &str) -> Result<PathBuf, String> {
    let dest = image_dir().join(format!("{sha}.jpg"));
    if dest.is_file() {
        return Ok(dest);
    }
    std::fs::create_dir_all(image_dir()).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    return macos::image_for_model(path, &dest).map(|_| dest);
    #[cfg(windows)]
    return image_win::image_for_model(path, &dest).map(|_| dest);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        Err("Showing images to a model isn't available on this system yet.".into())
    }
}

/// Where `image_for_model` keeps the images it makes.
pub fn image_dir() -> PathBuf {
    data_dir().join("ask").join("img")
}

/// Removes the PDF's last page when it has no text: the blank page WebKit's print adds after a
/// forced page break (Export PDF's split tables). Only called when the export forced one.
pub fn drop_blank_last_page(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::drop_blank_last_page(path);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Ok(())
    }
}

pub fn stamp_pdf_footer(path: &Path, title: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::stamp_pdf_footer(path, title);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, title);
        Ok(())
    }
}

/// Prints the window's page, with its @media print styles, to a PDF at `dest`. Returns once the
/// printing has started; the file appears when it's done.
pub fn print_to_pdf(w: &tauri::WebviewWindow, dest: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let dest = dest.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        w.with_webview(move |wv| {
            // SAFETY: Tauri hands over the live WKWebView, on the main thread.
            let r = unsafe { macos::print_to_pdf(wv.inner() as *mut objc2::runtime::AnyObject, &dest) };
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10)).map_err(|_| "Printing didn't start.".to_string())?
    }
    #[cfg(windows)]
    {
        let dest = dest.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        w.with_webview(move |wv| {
            let _ = tx.send(windows_print_to_pdf(&wv, &dest));
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10)).map_err(|_| "Printing didn't start.".to_string())?
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (w, dest);
        Err("Export PDF isn't available on this system yet.".into())
    }
}

/// WebView2's PrintToPdf, on the main thread: the paper the user's locale uses (A4, or Letter), half
/// an inch all round as on a Mac, no browser header or footer (the page's print style draws the
/// title and "Page N of M" in the bottom margin, src/notes/actions.ts) and no backgrounds, as
/// WebKit's print. Chromium repeats a long table's header row itself.
#[cfg(windows)]
fn windows_print_to_pdf(wv: &tauri::webview::PlatformWebview, dest: &Path) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Environment6, ICoreWebView2_7};
    use webview2_com::PrintToPdfCompletedHandler;
    use windows_core::{Interface, HSTRING};
    let old = |_| "This computer's WebView2 is too old to print to PDF: update Microsoft Edge WebView2 Runtime.".to_string();
    // SAFETY: COM calls on the live webview Tauri hands over, on the main thread it lives on.
    unsafe {
        let core = wv.controller().CoreWebView2().map_err(|e| e.to_string())?;
        let core: ICoreWebView2_7 = core.cast().map_err(old)?;
        let env: ICoreWebView2Environment6 = wv.environment().cast().map_err(old)?;
        let s = env.CreatePrintSettings().map_err(|e| e.to_string())?;
        let (pw, ph) = if paper_is_letter() { (8.5, 11.0) } else { (8.27, 11.69) };
        s.SetPageWidth(pw).map_err(|e| e.to_string())?;
        s.SetPageHeight(ph).map_err(|e| e.to_string())?;
        for set in [s.SetMarginTop(0.5), s.SetMarginBottom(0.5), s.SetMarginLeft(0.5), s.SetMarginRight(0.5)] {
            set.map_err(|e| e.to_string())?;
        }
        s.SetShouldPrintHeaderAndFooter(false).map_err(|e| e.to_string())?;
        s.SetShouldPrintBackgrounds(false).map_err(|e| e.to_string())?;
        let done = PrintToPdfCompletedHandler::create(Box::new(|r, ok| {
            if r.is_err() || !ok {
                crate::applog!("export pdf: WebView2's PrintToPdf failed ({r:?})");
            }
            Ok(())
        }));
        core.PrintToPdf(&HSTRING::from(dest.to_string_lossy().as_ref()), &s, &done).map_err(|e| e.to_string())
    }
}

/// Whether the user's locale prints on US Letter (LOCALE_IPAPERSIZE 1) rather than A4.
#[cfg(windows)]
fn paper_is_letter() -> bool {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_IPAPERSIZE, LOCALE_RETURN_NUMBER};
    let mut n = [0u16; 2];
    // SAFETY: a null locale name is the user's default; the buffer is two UTF-16 units, the DWORD
    // LOCALE_RETURN_NUMBER writes.
    let got = unsafe { GetLocaleInfoEx(std::ptr::null(), LOCALE_IPAPERSIZE | LOCALE_RETURN_NUMBER, n.as_mut_ptr(), 2) };
    got > 0 && (n[0] as u32 | (n[1] as u32) << 16) == 1
}

/// A `Command` for a console program, started without a console window: a GUI app on Windows
/// otherwise flashes one up for each program it runs (CREATE_NO_WINDOW). A plain `Command`
/// elsewhere.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Starts a program as the leader of its own process group, so stopping it also stops whatever it
/// started (node, rg, a shell). Elsewhere it's started as it is.
pub fn spawn_group(cmd: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
}

/// Asks a process group started by `spawn_group` to stop (SIGTERM to the whole group).
pub fn stop_group(pid: u32) {
    #[cfg(unix)]
    // SAFETY: kill(2) takes no pointers; a negative pid addresses the process group.
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        let _ = command("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).output();
    }
}

/// Finds an installed command-line program. A GUI app doesn't get the login shell's PATH, so it
/// looks where installers put programs (`extra` is under the home folder), then asks a login shell.
/// On Windows it looks for `.local\bin\<name>.exe` (Claude Code's own installer), `extra`, and
/// npm's `%APPDATA%\npm\<name>.cmd`, then asks `where`, taking an `.exe` before a `.cmd` and never
/// the extensionless shell script npm installs beside it.
pub fn find_program(name: &str, extra: &[&str]) -> Option<Program> {
    locate(name, extra).map(Program::new)
}

/// An installed program and how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// Where it was found, as shown to the user.
    pub path: PathBuf,
    /// What is started, and the arguments that go before the caller's. On Windows an npm `.cmd`
    /// shim is replaced by what it runs (node and its script, or a program), so arguments with line
    /// breaks or quotes reach the CLI intact and the process stopped is the CLI's own.
    pub exe: PathBuf,
    pub prefix_args: Vec<String>,
}

impl Program {
    fn new(path: PathBuf) -> Program {
        #[cfg(windows)]
        {
            let node = || {
                let out = command("where").arg("node").output().ok()?;
                brainstead_core::paths::pick_program(&String::from_utf8_lossy(&out.stdout))
            };
            if let Some((exe, prefix_args)) = brainstead_core::paths::resolve_npm_shim(&path, node) {
                return Program { path, exe, prefix_args };
            }
        }
        Program { exe: path.clone(), path, prefix_args: Vec::new() }
    }

    /// A `command` that starts it, with its leading arguments.
    pub fn command(&self) -> Command {
        let mut cmd = command(&self.exe);
        cmd.args(&self.prefix_args);
        cmd
    }

    /// Whether it starts through cmd.exe (a `.cmd` or `.bat` that couldn't be resolved), where an
    /// argument with a line break can't be passed: std refuses it as invalid input.
    pub fn through_cmd(&self) -> bool {
        self.exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
    }
}

fn locate(name: &str, extra: &[&str]) -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    #[cfg(not(windows))]
    let places: Vec<PathBuf> = {
        let mut places = vec![home.join(".local/bin").join(name)];
        places.extend(extra.iter().map(|p| home.join(p)));
        places.extend([PathBuf::from("/opt/homebrew/bin").join(name), PathBuf::from("/usr/local/bin").join(name)]);
        places
    };
    #[cfg(windows)]
    let places: Vec<PathBuf> = {
        let mut places = vec![home.join(".local").join("bin").join(format!("{name}.exe"))];
        places.extend(extra.iter().map(|p| home.join(p)));
        places.extend(dirs::data_dir().map(|d| d.join("npm").join(format!("{name}.cmd"))));
        places
    };
    if let Some(p) = places.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    #[cfg(unix)]
    {
        let out = command("/bin/zsh").args(["-lc", &format!("command -v {name}")]).output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").trim().to_string();
        (!s.is_empty() && Path::new(&s).is_file()).then(|| PathBuf::from(s))
    }
    #[cfg(windows)]
    {
        let out = command("where").arg(name).output().ok()?;
        brainstead_core::paths::pick_program(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Spelling and grammar (the editor's marks): macOS's checker, or on Windows its spell checker
/// with Harper's grammar (spell_win.rs); nothing elsewhere yet.
#[derive(serde::Serialize)]
pub struct GrammarIssue {
    pub start: usize,
    pub len: usize,
    pub description: String,
    pub corrections: Vec<String>,
}

/// The English spelling languages the computer has, and the system's own.
pub fn spell_languages() -> (Vec<String>, String) {
    #[cfg(any(target_os = "macos", windows))]
    return (spell::english_languages(), spell::system_language());
    #[cfg(not(any(target_os = "macos", windows)))]
    (vec![], String::new())
}

pub fn spell_check(text: &str, lang: Option<&str>) -> Vec<(usize, usize)> {
    #[cfg(any(target_os = "macos", windows))]
    return spell::check(text, lang);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (text, lang);
        vec![]
    }
}

pub fn spell_grammar(text: &str, lang: Option<&str>) -> Vec<GrammarIssue> {
    #[cfg(any(target_os = "macos", windows))]
    return spell::grammar(text, lang)
        .into_iter()
        .map(|g| GrammarIssue { start: g.start, len: g.len, description: g.description, corrections: g.corrections })
        .collect();
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (text, lang);
        vec![]
    }
}

pub fn spell_guesses(word: &str, lang: Option<&str>) -> Vec<String> {
    #[cfg(any(target_os = "macos", windows))]
    return spell::guesses(word, lang);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (word, lang);
        vec![]
    }
}

/// Gets grammar ready off the main thread, where the window asks for it: on Windows, Harper's
/// checks take a few seconds to make the first time. Nothing elsewhere.
pub fn spell_warm(lang: Option<String>) {
    #[cfg(windows)]
    std::thread::spawn(move || {
        let _ = spell::grammar("", lang.as_deref());
    });
    #[cfg(not(windows))]
    let _ = lang;
}

/// Learn (true) or ignore until the app quits (false).
pub fn spell_accept(word: &str, learn: bool) {
    #[cfg(any(target_os = "macos", windows))]
    if learn {
        spell::learn(word)
    } else {
        spell::ignore(word)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = (word, learn);
}

// Reading aloud (src/speech.ts): the system's voices, speaking, pausing and stopping, Now Playing
// with the media keys, and keeping the screen awake while reading. macOS has them; elsewhere they
// do nothing and there are no voices: on Windows the window reads aloud through the web view's own
// speech instead (src/speech/webSpeech.ts), without Now Playing or keeping awake.

/// A voice the system can speak with.
#[derive(serde::Serialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub lang: String,
    /// 1 default, 2 Enhanced, 3 Premium.
    pub quality: isize,
    pub default: bool,
}

/// What the synthesiser says back: "word" (`char` and `len`, the word's UTF-16 range) or "end".
/// Only macOS's synthesiser sends them.
#[cfg(target_os = "macos")]
#[derive(Clone, serde::Serialize)]
pub struct SpeechEvent {
    pub id: u64,
    pub kind: &'static str,
    pub char: usize,
    pub len: usize,
}

pub fn speech_voices() -> Vec<Voice> {
    #[cfg(target_os = "macos")]
    return speech::voices();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

pub fn speech_speak(app: &tauri::AppHandle, id: u64, text: String, voice: Option<String>, rate: f32) {
    #[cfg(target_os = "macos")]
    speech::speak(app, id, text, voice, rate);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, id, text, voice, rate);
}

pub fn speech_pause(app: &tauri::AppHandle, on: bool) {
    #[cfg(target_os = "macos")]
    speech::pause(app, on);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, on);
}

pub fn speech_stop(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    speech::stop(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Says what's being read in Now Playing (none clears it), so the media keys come here.
pub fn now_playing(app: &tauri::AppHandle, title: Option<String>, playing: bool) {
    #[cfg(target_os = "macos")]
    now_playing::state(app, title, playing);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, title, playing);
}

/// Keeps the display and the system from sleeping while `on`: one `caffeinate` child on macOS,
/// which ends with the app (`-w`) if it quits first.
pub fn keep_awake(on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::Mutex;
        static CHILD: Mutex<Option<Child>> = Mutex::new(None);
        let mut c = CHILD.lock().unwrap_or_else(|e| e.into_inner());
        if on {
            // One at a time; one that ended (killed from outside) is replaced.
            if let Some(ch) = c.as_mut() {
                if ch.try_wait().ok().flatten().is_none() {
                    return Ok(());
                }
            }
            let child = Command::new("/usr/bin/caffeinate")
                .args(["-d", "-i", "-w", &std::process::id().to_string()])
                .spawn()
                .map_err(|e| e.to_string())?;
            *c = Some(child);
        } else if let Some(mut ch) = c.take() {
            let _ = ch.kill();
            let _ = ch.wait();
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on;
        Ok(())
    }
}

/// A browser and the capture host's registration with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHost {
    /// The browser's name (Chrome, Edge, Brave…).
    pub browser: &'static str,
    /// The browser can find a manifest for the host.
    pub registered: bool,
    /// What it finds is `manifest` as given (this copy of Brainstead, not another build).
    pub current: bool,
}

/// Registers native messaging host `name`, described by `manifest`, with each installed Chromium
/// browser where it isn't registered as that already; with `write` false it only reports. macOS and
/// Linux: a `<name>.json` manifest in each browser's own NativeMessagingHosts folder. Windows: one
/// manifest in the app data folder, which each browser's
/// `HKCU\Software\<vendor>\NativeMessagingHosts\<name>` key names.
pub fn register_native_host(name: &str, manifest: &serde_json::Value, write: bool) -> Vec<NativeHost> {
    #[cfg(windows)]
    return imp::register_native_host(name, manifest, write);
    #[cfg(not(windows))]
    imp::native_host_dirs()
        .into_iter()
        .map(|(browser, dir)| {
            let (registered, current) = manifest_file(&dir.join(format!("{name}.json")), manifest, write);
            NativeHost { browser, registered, current }
        })
        .collect()
}

/// Writes `want` to `file`, with its folder, unless it holds that already or `write` is false.
/// Whether a manifest is there, and whether it is `want`.
fn manifest_file(file: &Path, want: &serde_json::Value, write: bool) -> (bool, bool) {
    let have: Option<serde_json::Value> = std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let mut current = have.as_ref() == Some(want);
    if !current && write && file.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) {
        if let Ok(b) = serde_json::to_vec_pretty(want) {
            current = brainstead_core::write::write_atomic(file, &b, false).is_ok();
        }
    }
    (current || have.is_some(), current)
}

/// Puts the menu bar up. macOS has one menu bar for the app. Elsewhere a menu set on the app is
/// attached to every window, the frameless capture and menu windows too, so it goes on the main
/// window alone.
pub fn set_menu_bar(app: &tauri::AppHandle, menu: tauri::menu::Menu<tauri::Wry>) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_menu(menu)?;
    #[cfg(not(target_os = "macos"))]
    if let Some(main) = tauri::Manager::get_webview_window(app, "main") {
        main.set_menu(menu)?;
    }
    Ok(())
}

/// Whether releases carry an update for this system (`crate::updates`): the macOS package
/// (tools/update_files.py) and the Windows installer (tools/update_windows.py, from
/// .github/workflows/windows-release.yml). Linux has none, so the updater doesn't look there.
pub fn release_updates() -> bool {
    cfg!(any(target_os = "macos", windows))
}

/// Deletes the folders Windows' updater leaves in Temp, one per update installed
/// ("Brainstead-1.3.0-updater-Zu7DmM", holding the installer it ran). By the next start that
/// installer has done its work. Nothing to do elsewhere.
pub fn remove_update_installers(name: &str) {
    #[cfg(windows)]
    {
        let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else { return };
        let prefix = format!("{name}-");
        for e in dir.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with(&prefix) && n.contains("-updater-") && e.path().is_dir() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    #[cfg(not(windows))]
    let _ = name;
}

/// Which of the features that aren't on every system this one has: one table in core, which the
/// window (`platform_features`) and the MCP server's `app_status` both read.
pub use brainstead_core::features::{platform_features, PlatformFeatures};

/// A secret kept for Brainstead (the Teams and Outlook sign-in, `sync/auth.rs`): the login
/// Keychain on macOS, under the service "Brainstead" (or "Brainstead (dev)" for a dev build, so it
/// leaves the installed app's alone). None when there's none. Elsewhere there's no store yet.
pub fn secret_get(account: &str) -> Option<String> {
    imp::secret_get(&secret_service(), account)
}

/// Keeps `value` as the secret `account`, replacing what was there.
pub fn secret_set(account: &str, value: &str) -> Result<(), String> {
    imp::secret_set(&secret_service(), account, value)
}

/// Forgets the secret `account`; fine when there's none.
pub fn secret_delete(account: &str) -> Result<(), String> {
    imp::secret_delete(&secret_service(), account)
}

fn secret_service() -> String {
    if cfg!(debug_assertions) {
        "Brainstead (dev)".into()
    } else {
        "Brainstead".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_manifest_is_written_once_and_then_current() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hosts").join("com.example.host.json");
        let want = serde_json::json!({"name": "com.example.host", "path": "/x"});
        assert_eq!(manifest_file(&file, &want, false), (false, false));
        assert!(!file.exists());
        assert_eq!(manifest_file(&file, &want, true), (true, true));
        assert_eq!(manifest_file(&file, &want, false), (true, true));
        let other = serde_json::json!({"name": "com.example.host", "path": "/y"});
        assert_eq!(manifest_file(&file, &other, false), (true, false));
    }

    /// Off macOS a PDF's text comes from pdf-extract, which won't read a file without its
    /// cross-reference table, as PDFKit will: the fixture's sample has to be a well-formed PDF.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn reads_the_fixture_pdf_as_the_index_does() {
        let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/vault/sources/Programme Update Steerco 2026-09-28.pdf");
        let pages = pdf_text(&pdf).unwrap();
        assert!(pages.concat().contains("Programme update, page 1"), "{pages:?}");
    }
}
