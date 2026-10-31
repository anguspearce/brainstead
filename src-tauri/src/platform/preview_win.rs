// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! A file previewed inside the window on Windows, where a Mac has Quick Look: the shell's preview
//! handler for the file's type (what Explorer's preview pane shows; Office registers one for Word,
//! Excel and PowerPoint, Windows its own for text, RTF, fonts and mail), hosted in a native window
//! of ours placed where the page says its panel sits. The page draws the frame (the name, Open in
//! its app, Close) and the handler draws the content, so this module knows nothing of the panel but
//! its rectangle.
//!
//! Every handler runs out of process, as Explorer runs it: created with `CLSCTX_LOCAL_SERVER` only,
//! so COM starts the handler's own server (Office's run in WINWORD, EXCEL and POWERPNT) or, for a
//! handler that is a DLL, the surrogate its AppID names (`DllSurrogate`: Windows' prevhost.exe).
//! No handler's code is loaded into Brainstead, so a crash or a hang in one can't take the app
//! with it, and a document from a mail or a chat is parsed in that other process. The handler's
//! window becomes a child of ours across processes. A handler registered with no way to run out
//! of process (`hostable`) isn't offered: the file opens in its own app, as for a type with no
//! handler.
//!
//! PowerPoint's handler draws its slides only for a host the way Explorer has one: created and
//! called on the main thread, in a window that isn't inside the web view's. Called from another
//! thread, or with its window a child of the main window, it opens the file and shows its scroll
//! bar but draws no slide (Word's draws either way). So the host is a popup owned by the main
//! window, made on the main thread: it stays above the window, hides when it's minimised, and
//! follows it as it moves, resizes, hides and shows (`follow`, a hook on the window's events).
//! `open`, called off the main thread, posts the host what to do and waits for the answer, so the
//! caller learns whether the preview is up. While the handler's process opens the document
//! (seconds, for Office), COM's wait on the main thread keeps the window painting, moving and
//! resizing, but Tauri's event loop, and so the page's commands, wait for the call (`Book::busy`):
//! hence a deadline on every call.
//!
//! The host never takes the activation, so the main window and the page keep the focus, and with
//! it the keys that close the panel (`give_back_keys` says when a click hands them to the
//! document). The handler is released when the page closes the panel or the app quits
//! (`shutdown`). The host is not hidden when the window loses the focus: the focus goes when the
//! user clicks into the preview or while Office starts to draw it, which would hide it with no way
//! back but a click on the window. Which handler a type has is read from the registry as Explorer
//! reads it (`handler_for`), which is what `available` answers, so the page opens its panel only
//! for a file that can be previewed and opens the rest in their own app. A file downloaded from
//! the web opens in its app too, as Explorer won't preview it (`refused`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tauri::Manager;
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::POINT;
use windows::Win32::Foundation::{COLORREF, CO_E_APPNOTFOUND, HINSTANCE, HWND, LPARAM, LRESULT, RECT, REGDB_E_CLASSNOTREG, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect,
    SetWindowRgn, HDC, PAINTSTRUCT, RGN_DIFF,
};
use windows::Win32::System::Com::{
    CLSIDFromString, CoCancelCall, CoCreateInstance, CoDisableCallCancellation, CoEnableCallCancellation, CoInitializeEx,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, STGM_READ, STGM_SHARE_DENY_NONE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithFile, IInitializeWithStream};
use windows::Win32::UI::Shell::{
    IInitializeWithItem, IPreviewHandler, IPreviewHandlerVisuals, IShellItem, SHCreateItemFromParsingName, SHCreateStreamOnFileEx,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetGUIThreadInfo, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsChild,
    IsIconic, IsWindowVisible, PostMessageW, RegisterClassExW, SendNotifyMessageW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, GUITHREADINFO, GWLP_USERDATA, GW_OWNER, OBJID_WINDOW,
    SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WINEVENT_OUTOFCONTEXT, WM_APP, WM_CREATE,
    WM_DESTROY, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT, WNDCLASSEXW, WS_CLIPCHILDREN, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// The shell extension handler category for preview handlers: `shellex\{this}` under a type's key
/// names the handler's CLSID.
const PREVIEW_HANDLER: &str = "{8895b1c6-b41f-4c1c-a562-0d564250836f}";

/// Where the page's panel body is (x, y, width, height in physical pixels of the window's client
/// area) and its colours (`#rrggbb`), which a handler that draws its own background takes.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub bg: String,
    pub fg: String,
    /// The radius of the panel's bottom corners, in physical pixels: the child is cut to them, so
    /// a handler drawing to its edges doesn't square them off.
    #[serde(default)]
    pub radius: i32,
    /// The panel this is: the page numbers each panel it opens, so a close that arrives before
    /// its open (they travel different routes) still wins, and a move or close from a panel that
    /// has gone never touches the next one.
    #[serde(default)]
    pub id: u64,
    /// The page shows a dialog of its own over the panel (the Quit question, unsaved edits, an
    /// update): the host hides until it's gone, as it would cover it.
    #[serde(default)]
    pub hidden: bool,
    /// Rectangles of the body (x, y, width, height, physical pixels) where the page shows a
    /// message over the panel: the host is cut away there, so the message shows through.
    #[serde(default)]
    pub holes: Vec<[i32; 4]>,
}

/// The registry as the lookup reads it: HKEY_CLASSES_ROOT (which merges the machine's and the
/// user's classes), and the user's own choice of program for a type. A trait so the walk can be
/// tested against a table.
pub trait Classes {
    /// The value `name` ("" for the default) of `HKCR\key`.
    fn value(&self, key: &str, name: &str) -> Option<String>;
    /// The ProgID the user chose for `ext` (Explorer's Open with › Always), if any.
    fn user_choice(&self, ext: &str) -> Option<String>;
}

/// The CLSID of the preview handler for files with `ext` (".docx"), in Explorer's order: the
/// user's chosen ProgID, the extension's own `shellex`, its ProgID's (through `CurVer`), then the
/// system's associations for the extension and for its perceived type (text, image…). None when
/// the type has no handler, which Explorer's preview pane shows as "No preview available".
pub fn handler_for(ext: &str, reg: &dyn Classes) -> Option<String> {
    let ext = ext.to_ascii_lowercase();
    let under = |key: &str| reg.value(&format!("{key}\\shellex\\{PREVIEW_HANDLER}"), "").and_then(clsid);
    if let Some(c) = reg.user_choice(&ext).and_then(|p| under(&p)) {
        return Some(c);
    }
    if let Some(c) = under(&ext) {
        return Some(c);
    }
    if let Some(mut progid) = reg.value(&ext, "") {
        if let Some(cur) = reg.value(&format!("{progid}\\CurVer"), "") {
            progid = cur;
        }
        if let Some(c) = under(&progid) {
            return Some(c);
        }
    }
    if let Some(c) = under(&format!("SystemFileAssociations\\{ext}")) {
        return Some(c);
    }
    reg.value(&ext, "PerceivedType").and_then(|p| under(&format!("SystemFileAssociations\\{p}")))
}

/// A registry value as a CLSID, braces on: `CLSIDFromString` wants them, and a handler is
/// sometimes registered without.
fn clsid(v: String) -> Option<String> {
    let v = v.trim().trim_matches(|c: char| c == '{' || c == '}');
    (v.len() == 36 && v.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit())).then(|| format!("{{{v}}}"))
}

/// The real registry.
struct Registry;

impl Classes for Registry {
    fn value(&self, key: &str, name: &str) -> Option<String> {
        let hkcr = winreg::RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT);
        hkcr.open_subkey(key).and_then(|k| k.get_value(name)).ok()
    }
    fn user_choice(&self, ext: &str) -> Option<String> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        hkcu.open_subkey(format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\UserChoice"))
            .and_then(|k| k.get_value("ProgId"))
            .ok()
    }
}

/// The extension of `name` as the registry keys it, ".docx"; None for a name without one.
fn ext_of(name: &str) -> Option<String> {
    Path::new(name).extension().map(|e| format!(".{}", e.to_string_lossy().to_ascii_lowercase()))
}

/// Whether COM can run the handler `clsid` out of process, as Explorer runs it: in a server of its
/// own (`LocalServer32`, as Office's are) or in the surrogate its AppID names (`DllSurrogate`,
/// prevhost.exe for Windows' own and most others'), under the 64-bit or the 32-bit registration.
pub fn hostable(clsid: &str, reg: &dyn Classes) -> bool {
    ["CLSID", "WOW6432Node\\CLSID"].iter().any(|root| {
        let key = format!("{root}\\{clsid}");
        reg.value(&format!("{key}\\LocalServer32"), "").is_some()
            || reg.value(&key, "AppID").and_then(self::clsid).is_some_and(|app| {
                ["AppID", "WOW6432Node\\AppID"].iter().any(|a| reg.value(&format!("{a}\\{app}"), "DllSurrogate").is_some())
            })
    })
}

/// Whether Windows has a preview handler for files named like `name` that can run out of process.
pub fn available(name: &str) -> bool {
    ext_of(name).and_then(|e| handler_for(&e, &Registry)).is_some_and(|c| hostable(&c, &Registry))
}

/// What the panel says of a file it won't preview because it came from the web; it opens in its
/// app instead, where Office shows it in Protected View.
const FROM_WEB: &str = "This file was downloaded from the internet, and Windows doesn't preview such files, \
                        as Explorer's preview pane doesn't: it opened in its app instead.";

/// Why the panel won't preview `path`, which then opens in its app: Explorer's preview pane
/// shows no file marked as from the internet or a restricted site (the mark of the web, since
/// Windows' October 2025 update), as a handler reading one could hand the file's links the
/// user's network credentials; this does as Explorer does. None for a file it previews.
pub fn refused(path: &Path) -> Option<String> {
    zone(path).filter(|z| *z >= 3).map(|_| FROM_WEB.to_string())
}

/// The zone Windows marked a downloaded file with, from its `Zone.Identifier` stream: 3 the
/// internet, 4 a restricted site, below that this computer or the local network. None unmarked.
fn zone(path: &Path) -> Option<u32> {
    let mut marked = path.as_os_str().to_os_string();
    marked.push(":Zone.Identifier");
    std::fs::read(&marked).ok().and_then(|b| zone_id(&b))
}

/// The `ZoneId` in a `Zone.Identifier` stream, which browsers write as plain text and some tools
/// as UTF-16 with its byte-order mark.
fn zone_id(bytes: &[u8]) -> Option<u32> {
    let text = match bytes {
        [0xFF, 0xFE, rest @ ..] => {
            let units: Vec<u16> =
                rest.iter().step_by(2).zip(rest.iter().skip(1).step_by(2)).map(|(&lo, &hi)| u16::from_le_bytes([lo, hi])).collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        if k.trim().eq_ignore_ascii_case("ZoneId") {
            v.trim().parse().ok()
        } else {
            None
        }
    })
}

// What `open` and the page's commands post to the host window. Each carries nothing: what to do
// waits in a slot (`PENDING`, `MOVED`) that the host empties, so a message lost with a window that
// is gone leaks nothing and leaves no caller waiting.
const MSG_OPEN: u32 = WM_APP + 1;
const MSG_MOVE: u32 = WM_APP + 2;
const MSG_CLOSE: u32 = WM_APP + 3;
/// From a call's watchdog: the call (its number in wParam) missed its deadline.
const MSG_LATE: u32 = WM_APP + 4;

/// The app, for giving the page the keys back (`give_back_keys`).
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

struct OpenRequest {
    path: std::path::PathBuf,
    frame: Frame,
    reply: Sender<Result<bool, String>>,
}

/// The open waiting for the host. A newer one replaces it, and the replaced caller hears at once
/// (its reply is dropped): the page shows one preview at a time.
static PENDING: Mutex<Option<OpenRequest>> = Mutex::new(None);
/// Where the page last put the panel, waiting for the host.
static MOVED: Mutex<Option<Frame>> = Mutex::new(None);

/// The host window, once made (0 before, and again if it is destroyed); as a number, since a
/// handle isn't Send. Read without locking: the page's moves and closes arrive on the main thread,
/// which must never wait on `host`'s caller, itself waiting for the main thread to make the window.
static HOST: AtomicIsize = AtomicIsize::new(0);
/// Taken by the one caller making the host window; `open` is never on the main thread.
static STARTING: Mutex<()> = Mutex::new(());

/// The longest `open` waits for the host's answer. The host answers well before (its calls into
/// the handler have deadlines); this is for a main thread that stops dispatching altogether.
const BACKSTOP: Duration = Duration::from_secs(180);

/// The answer to an open whose panel the page has closed already: it shows nothing of it.
const CLOSED_TEXT: &str = "The preview was closed.";

/// Opens `path` in its preview handler in the panel at `frame`, and returns once the handler has
/// drawn it (or failed to). False when COM can't run the handler out of process after all (the
/// caller opens the file in its app instead); Err when the type has no handler or the handler
/// refused the file.
pub fn open(w: &tauri::WebviewWindow, path: &Path, frame: Frame) -> Result<bool, String> {
    if closed(frame.id) {
        return Err(CLOSED_TEXT.to_string());
    }
    let host = host(w)?;
    let (reply, answer) = channel();
    *crate::lock(&PENDING) = Some(OpenRequest { path: path.to_path_buf(), frame, reply });
    if let Err(e) = post(host, MSG_OPEN) {
        crate::lock(&PENDING).take();
        return Err(e);
    }
    match answer.recv_timeout(BACKSTOP) {
        Ok(r) => r,
        Err(RecvTimeoutError::Disconnected) => Err("The preview was closed before the file opened.".to_string()),
        Err(RecvTimeoutError::Timeout) => Err("The preview window stopped answering.".to_string()),
    }
}

/// The panel moved or resized (the window did, or the page's zoom changed).
pub fn move_to(frame: Frame) {
    *crate::lock(&MOVED) = Some(frame);
    if let Some(host) = current() {
        let _ = post(host, MSG_MOVE);
    }
}

/// The page closed its panel `id`: the handler is unloaded and released, the host hidden. Kept
/// even before the host exists, so an open of that panel still on its way is refused.
pub fn close(id: u64) {
    crate::lock(&CLOSED).close(id);
    if let Some(host) = current() {
        let _ = post(host, MSG_CLOSE);
    }
}

/// The panels the page closed lately, newest last.
static CLOSED: Mutex<Closed> = Mutex::new(Closed(VecDeque::new()));

/// The last few panels closed: enough to cover any open or close still on its way.
struct Closed(VecDeque<u64>);

impl Closed {
    const KEPT: usize = 32;

    fn close(&mut self, id: u64) {
        if !self.0.contains(&id) {
            self.0.push_back(id);
        }
        while self.0.len() > Self::KEPT {
            self.0.pop_front();
        }
    }

    fn has(&self, id: u64) -> bool {
        self.0.contains(&id)
    }
}

fn closed(id: u64) -> bool {
    crate::lock(&CLOSED).has(id)
}

fn current() -> Option<isize> {
    match HOST.load(Ordering::Acquire) {
        0 => None,
        h => Some(h),
    }
}

fn post(host: isize, msg: u32) -> Result<(), String> {
    unsafe { PostMessageW(Some(HWND(host as _)), msg, WPARAM(0), LPARAM(0)) }.map_err(|e| format!("The preview window is gone: {e}"))
}

/// The host window, made on the main thread on first use: a popup owned by the main window, hidden
/// until a preview is up.
fn host(w: &tauri::WebviewWindow) -> Result<isize, String> {
    if let Some(h) = current() {
        return Ok(h);
    }
    let _starting = crate::lock(&STARTING);
    if let Some(h) = current() {
        return Ok(h);
    }
    let _ = APP.set(w.app_handle().clone());
    let parent = w.hwnd().map_err(|e| e.to_string())?.0 as isize;
    let (tx, rx) = channel();
    w.run_on_main_thread(move || {
        let _ = tx.send(unsafe { make(parent) });
    })
    .map_err(|e| e.to_string())?;
    let hwnd = rx.recv().map_err(|_| "The preview window couldn't be made.".to_string())??;
    HOST.store(hwnd, Ordering::Release);
    Ok(hwnd)
}

const CLASS: PCWSTR = w!("BRAINSTEAD_PREVIEW");

/// The host's bookkeeping, apart from Win32 and COM so its rules can be tested.
#[derive(Debug, Default)]
struct Book {
    /// A call into the handler is in flight. The handler's process answers it while COM waits on
    /// the main thread. That wait delivers sent messages, incoming calls and the window events
    /// `follow` hooks, but not posted messages nor Tauri's event loop, so the page's commands, and
    /// with them its opens, moves and closes, wait for the call to end; one that came first waits
    /// in its slot (or `close_after`), since a second call into a busy handler would hang both.
    busy: bool,
    /// A handler is up or opening.
    showing: bool,
    /// The panel it's for.
    id: u64,
    /// The page closed the panel during a call: the handler goes once the call ends.
    close_after: bool,
    /// Where the panel is, its latest place even during a call, kept so the host can follow the
    /// window.
    at: Option<Frame>,
    /// The colours the handler was last given (background, text), apart from `at`, which can
    /// take a theme change during a call before the handler can.
    painted: Option<(String, String)>,
    /// The host has been put over the panel for this open, once the handler has the file (`show`);
    /// until then the page's "Opening the preview…" shows.
    placed: bool,
    /// How far the panel's body sits from the main window's client area's right and bottom edges,
    /// from the page's last place taken between calls: the body keeps them as the window resizes,
    /// so the host can follow a resize during a call (`follow_resize`).
    insets: Option<(i32, i32)>,
    /// A call into the handler missed its deadline: this host is done with (`abandon`).
    abandoned: bool,
}

/// What letting go of the host for good does with its handler (`Book::teardown`).
#[derive(Debug, PartialEq)]
enum Teardown {
    /// No handler, or one abandoned already, which is never called again.
    Nothing,
    /// A call into the handler is in flight, and this runs inside COM's wait for it: a second call
    /// would hang both, so the call is cancelled and the handler abandoned.
    Leave,
    /// Unloaded and released, each with a deadline.
    Unload,
}

/// What the host does with a message, by the bookkeeping alone.
#[derive(Debug, PartialEq)]
enum Step {
    /// Nothing now: a call is in flight, and its end takes this up.
    Wait,
    Go,
    /// Nothing at all: it's for a panel that isn't the one up.
    Skip,
}

impl Book {
    /// An open came in, for a panel the page has closed already or not.
    fn open(&mut self, f: &Frame, closed: bool) -> Step {
        if self.busy {
            return Step::Wait;
        }
        if closed {
            return Step::Skip;
        }
        self.showing = true;
        self.id = f.id;
        self.close_after = false;
        self.at = Some(f.clone());
        self.painted = None;
        self.placed = false;
        Step::Go
    }

    /// The open's calls ended, and its panel is closed by now or not: whether its handler stays up.
    fn opened(&mut self, ok: bool, closed: bool) -> bool {
        let keep = ok && !closed && !self.close_after;
        self.close_after = false;
        self.showing = keep;
        if !keep {
            self.at = None;
        }
        keep
    }

    /// The page closed a panel, the one up or opening (`closed`) or another: Go takes the handler
    /// down now.
    fn close(&mut self, closed: bool) -> Step {
        if !self.showing || !closed {
            return Step::Skip;
        }
        if self.busy {
            self.close_after = true;
            return Step::Wait;
        }
        self.showing = false;
        self.at = None;
        Step::Go
    }

    /// Whether the host is on screen, with the main window on screen or not (`owner`): only while
    /// a preview is up or drawing and not closed meanwhile, never without the main window and
    /// never under a dialog of the page's own.
    fn visible(&self, owner: bool) -> bool {
        self.showing && self.placed && !self.close_after && owner && !self.at.as_ref().is_some_and(|f| f.hidden)
    }

    /// Whether `f` (the panel's place, for the preview up) brings colours other than those the
    /// handler has: the theme changed.
    fn recoloured(&self, f: &Frame) -> bool {
        self.showing && self.id == f.id && self.painted.as_ref().is_some_and(|(bg, fg)| *bg != f.bg || *fg != f.fg)
    }

    /// The handler was given `f`'s colours.
    fn paint(&mut self, f: &Frame) {
        self.painted = Some((f.bg.clone(), f.fg.clone()));
    }

    /// What letting go of the host for good (the app quitting, the host destroyed) does with its
    /// handler, if it has one (`handler`).
    fn teardown(&self, handler: bool) -> Teardown {
        if self.abandoned {
            Teardown::Nothing
        } else if self.busy {
            Teardown::Leave
        } else if handler {
            Teardown::Unload
        } else {
            Teardown::Nothing
        }
    }

    /// The page's place `at` was taken with the main window's client area `client` (width,
    /// height) as it is now: the body's distance from its right and bottom edges is kept.
    fn anchor(&mut self, client: (i32, i32)) {
        self.insets = self.at.as_ref().map(|f| (client.0 - f.x - f.w, client.1 - f.y - f.h));
    }

    /// The main window's client area is now `client` during a call into the handler, when the
    /// page's own place can't arrive: Tauri serves the page's commands from its event loop, which
    /// COM's wait for the call doesn't run, while the window's own events still reach the host.
    /// The body keeps its insets (the page lays it out so), so the new place is known: Some when
    /// the host must be resized to it.
    fn follow_resize(&mut self, client: (i32, i32)) -> Option<Frame> {
        if !(self.busy && self.showing && self.placed) {
            return None;
        }
        let (right, bottom) = self.insets?;
        let at = self.at.as_mut()?;
        let (w, h) = ((client.0 - at.x - right).max(1), (client.1 - at.y - bottom).max(1));
        if (w, h) == (at.w, at.h) {
            return None;
        }
        at.w = w;
        at.h = h;
        Some(at.clone())
    }

    /// The page moved the panel, or a dialog of its own hid it: whether there's a preview to
    /// move. Taken before the handler has the file too, so the open places the host where the
    /// panel is by then.
    fn moved(&mut self, f: &Frame) -> bool {
        let here = self.showing && f.id == self.id;
        if here {
            self.at = Some(f.clone());
        }
        here
    }
}

/// What the host window knows: the handler showing (None between previews), the panel's
/// background, painted under the handler while it opens and where it draws nothing, and the
/// bookkeeping. Shared (`Rc`) between the window and every message being handled, and borrowed
/// only between calls out (never across a call into the handler or a Win32 call that sends the
/// window a message), since those dispatch the window's other messages re-entrantly.
struct State {
    handler: Option<IPreviewHandler>,
    bg: COLORREF,
    book: Book,
    /// The answer the open under way owes its caller.
    reply: Option<Sender<Result<bool, String>>>,
    /// The type being previewed (".pptx"), for what the page says when its handler is late.
    what: String,
    /// The calls into the handler so far, so the watchdog's message names the one it timed.
    calls: u64,
}

type Shared = Rc<RefCell<State>>;

thread_local! {
    /// Set on the thread that made the host (the main thread), the only one that may touch it.
    static ON_HOST_THREAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The app is quitting: the handler showing is unloaded and released, each call with `QUIT` as
/// its deadline, so its server lets go of the file now rather than when COM notices this process
/// has gone. A handler with a call in flight has that call cancelled and is left (`Teardown`).
/// Tauri hides its windows and ends the process without destroying them, so the host's own
/// teardown (`WM_DESTROY`) doesn't run on a quit. A quit asked for during a call is served when
/// the call ends, at its deadline at the latest, as Tauri's event loop waits for it. Only on the
/// host's thread (the app's exit event runs on the main thread); elsewhere it does nothing, and
/// the process's end lets go instead.
pub fn shutdown() {
    let Some(host) = current().map(|h| HWND(h as _)) else { return };
    if !ON_HOST_THREAD.with(|on| on.get()) {
        return;
    }
    unsafe {
        if let Some(st) = state(host) {
            let_go(host, &st, QUIT);
        }
    }
}

/// Lets go of the host's handler for good (`Teardown` says how): nothing calls into it after.
unsafe fn let_go(hwnd: HWND, st: &Shared, limit: Duration) {
    let step = st.try_borrow().map_or(Teardown::Leave, |s| s.book.teardown(s.handler.is_some()));
    match step {
        Teardown::Nothing => {}
        Teardown::Leave => {
            // On this thread, inside COM's wait for the call (the host destroyed during it):
            // cancelling it ends the wait at once rather than at its deadline.
            let _ = CoCancelCall(0, 0);
            abandon(hwnd, st);
        }
        Teardown::Unload => unload(hwnd, st, limit),
    }
    if let Ok(mut s) = st.try_borrow_mut() {
        s.book.abandoned = true;
    }
}

/// Makes the host window, on the main thread, whose message loop then runs it for the life of the
/// app. The handler lives on this thread too and is only ever touched from the window's messages.
unsafe fn make(parent: isize) -> Result<isize, String> {
    // The main thread's apartment is single-threaded already (the web view's): this only joins it.
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().map_err(|e| e.to_string())?;
    let hwnd = create(HWND(parent as _))?;
    ON_HOST_THREAD.with(|on| on.set(true));
    // This process's windows' moves and visibility, out of context: delivered on this thread.
    SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_LOCATIONCHANGE, None, Some(follow), std::process::id(), 0, WINEVENT_OUTOFCONTEXT);
    Ok(hwnd.0 as isize)
}

/// The main window moved, resized, hid or showed: the host goes with it while a preview is up
/// (Windows itself hides and shows it round a minimise), and during a call takes the panel's new
/// size from the window (`Book::follow_resize`). Only moves it, never calls the handler, so it's
/// safe while a call into the handler is in flight, when these events still arrive.
unsafe extern "system" fn follow(_: HWINEVENTHOOK, event: u32, hwnd: HWND, object: i32, _: i32, _: u32, _: u32) {
    let Some(host) = current().map(|h| HWND(h as _)) else { return };
    if object != OBJID_WINDOW.0 || GetWindow(host, GW_OWNER).ok() != Some(hwnd) {
        return;
    }
    let Some(st) = state(host) else { return };
    let owner = event != EVENT_OBJECT_HIDE && owner_shown(host);
    // Resized during a call into the handler: the host takes the panel's new size from the window.
    let client = owner_client(host);
    let resized = st.try_borrow_mut().ok().and_then(|mut s| s.book.follow_resize(client).map(|f| (f, s.book.visible(owner))));
    if let Some((f, show)) = resized {
        return place(host, &f, show);
    }
    let Some((f, show)) =
        st.try_borrow().ok().and_then(|s| s.book.at.clone().filter(|_| s.book.showing).map(|f| (f, s.book.visible(owner))))
    else {
        return;
    };
    if show {
        let pt = on_screen(host, &f);
        // Its place in the z-order stays where Windows keeps an owned window: just above its owner.
        let _ = SetWindowPos(host, None, pt.x, pt.y, 0, 0, SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE | SWP_SHOWWINDOW);
    } else if IsWindowVisible(host).as_bool() {
        let _ = ShowWindow(host, SW_HIDE);
    }
}

/// Whether the main window is on screen: shown (not hidden to the tray) and not minimised. The
/// host is never shown without it, or it would stand alone on the desktop.
unsafe fn owner_shown(host: HWND) -> bool {
    GetWindow(host, GW_OWNER).is_ok_and(|o| IsWindowVisible(o).as_bool() && !IsIconic(o).as_bool())
}

/// The main window's client area (width, height), in the physical pixels the frame is in.
unsafe fn owner_client(host: HWND) -> (i32, i32) {
    let mut rc = RECT::default();
    if let Ok(owner) = GetWindow(host, GW_OWNER) {
        let _ = GetClientRect(owner, &mut rc);
    }
    (rc.right - rc.left, rc.bottom - rc.top)
}

/// The panel's top left on the screen: the frame is in the main window's client pixels.
unsafe fn on_screen(host: HWND, f: &Frame) -> POINT {
    let mut pt = POINT { x: f.x, y: f.y };
    if let Ok(owner) = GetWindow(host, GW_OWNER) {
        let _ = ClientToScreen(owner, &mut pt);
    }
    pt
}

unsafe fn create(parent: HWND) -> Result<HWND, String> {
    let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    // Already registered is the same class: fine.
    RegisterClassExW(&class);
    let st: Shared = Rc::new(RefCell::new(State {
        handler: None,
        bg: COLORREF(0x00FFFFFF),
        book: Book::default(),
        reply: None,
        what: String::new(),
        calls: 0,
    }));
    CreateWindowExW(
        // Never activated, and not in the taskbar or Alt+Tab.
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        CLASS,
        w!("BRAINSTEAD_PREVIEW_WINDOW"),
        // Clipped against the handler's own child windows; `parent` owns it.
        WS_POPUP | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        Some(parent),
        None,
        Some(HINSTANCE::from(instance)),
        Some(Rc::into_raw(st) as _),
    )
    .map_err(|e| format!("The preview window couldn't be made: {e}"))
}

/// The host's state, a reference of its own: it outlives the window's, so a message handled while
/// the window is destroyed under it (inside a call into the handler) never touches freed memory.
unsafe fn state(hwnd: HWND) -> Option<Shared> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>;
    if p.is_null() {
        return None;
    }
    Rc::increment_strong_count(p);
    Some(Rc::from_raw(p))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `#rrggbb` as GDI's COLORREF (0x00bbggrr); white for anything else.
fn colour(hex: &str) -> COLORREF {
    let v = hex.trim_start_matches('#');
    match (v.len(), u32::from_str_radix(v, 16)) {
        (6, Ok(rgb)) => COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | (rgb >> 16)),
        _ => COLORREF(0x00FFFFFF),
    }
}

/// Puts the host over the panel, cut to its rounded bottom corners and round the page's messages
/// over it (`holes`), and shows or hides it (`show`, from `Book::visible`). The region reaches
/// above the top, so only the bottom corners are rounded. Its z-order is left alone: Windows keeps an owned window just above its owner, and
/// raising it would put it over whatever covers the main window. Never calls the handler.
unsafe fn place(hwnd: HWND, f: &Frame, show: bool) {
    let pt = on_screen(hwnd, f);
    let shown = if show { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW };
    let _ = SetWindowPos(hwnd, None, pt.x, pt.y, f.w, f.h, SWP_NOACTIVATE | SWP_NOZORDER | shown);
    let r = f.radius.max(0);
    // The window owns the region once set; None (no rounding, no holes) clears it.
    let rgn = match (r > 0, f.holes.is_empty()) {
        (true, _) => Some(CreateRoundRectRgn(0, -2 * r, f.w + 1, f.h + 1, 2 * r, 2 * r)),
        (false, false) => Some(CreateRectRgn(0, 0, f.w, f.h)),
        (false, true) => None,
    };
    if let Some(rgn) = rgn {
        for &[x, y, w, h] in &f.holes {
            let hole = CreateRectRgn(x, y, x + w, y + h);
            CombineRgn(Some(rgn), Some(rgn), Some(hole), RGN_DIFF);
            let _ = DeleteObject(hole.into());
        }
    }
    let _ = SetWindowRgn(hwnd, rgn, true);
}

/// How long a call into the handler may take before it's given up: Office's first start of its
/// server, and its open of a large document, take seconds, never this long.
const SLOW: Duration = Duration::from_secs(30);
/// The same for a call that only hands the handler a window, a size or colours.
const QUICK: Duration = Duration::from_secs(10);
/// The same for letting go of the handler as the app quits: nobody waits for the answer.
const QUIT: Duration = Duration::from_secs(2);

/// `d`, or in a debug build what `BRAINSTEAD_PREVIEW_DEADLINE_MS` says, so a missed deadline can
/// be tried against a real handler.
fn deadline(d: Duration) -> Duration {
    let set = cfg!(debug_assertions).then(|| std::env::var("BRAINSTEAD_PREVIEW_DEADLINE_MS").ok()).flatten();
    set.and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(d)
}

/// A call into the handler missed its deadline.
#[derive(Debug)]
struct Late;

/// Why an open failed.
enum Fail {
    Late,
    Said(String),
}

impl From<String> for Fail {
    fn from(s: String) -> Self {
        Fail::Said(s)
    }
}

impl From<Late> for Fail {
    fn from(_: Late) -> Self {
        Fail::Late
    }
}

/// What the page shows when a handler missed its deadline.
fn late_text(what: &str) -> String {
    let what = if what.is_empty() { "this type of" } else { what };
    format!("Windows' preview for {what} files didn't answer, so Brainstead stopped waiting for it. Open in its app shows the file.")
}

/// Runs `f`, a call into the handler, with the host marked busy and a deadline (`limit`), and has
/// the host take up what came in meanwhile once it ends: an open or a move waiting in its slot, or a
/// close. One post each per call, never a message re-posting itself while it waits.
///
/// Past the deadline a watchdog thread cancels the call (COM's call cancellation, turned on for
/// this thread only during the call, so the main thread's other COM calls keep their speed) and
/// sends `MSG_LATE` (sent, not posted: COM's wait delivers sent messages, not posted ones). A
/// cancelled call returns at once, and the caller abandons the handler; if it still doesn't,
/// `MSG_LATE`, run inside COM's wait for the call, abandons it from there.
unsafe fn call<T>(hwnd: HWND, st: &Shared, limit: Duration, f: impl FnOnce() -> T) -> Result<T, Late> {
    let n = {
        let mut s = st.borrow_mut();
        // An abandoned handler is never called again.
        if s.book.abandoned {
            return Err(Late);
        }
        s.book.busy = true;
        s.calls += 1;
        s.calls
    };
    let late = Arc::new(AtomicBool::new(false));
    let (done, wait) = channel::<()>();
    let tid = GetWindowThreadProcessId(hwnd, None);
    let (host, flag, limit) = (hwnd.0 as isize, late.clone(), deadline(limit));
    std::thread::spawn(move || {
        if let Err(RecvTimeoutError::Timeout) = wait.recv_timeout(limit) {
            flag.store(true, Ordering::Release);
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = CoCancelCall(tid, 0);
            let _ = SendNotifyMessageW(HWND(host as _), MSG_LATE, WPARAM(n as usize), LPARAM(0));
        }
    });
    let _ = CoEnableCallCancellation(None);
    let r = f();
    let _ = CoDisableCallCancellation(None);
    drop(done);
    let close = {
        let mut s = st.borrow_mut();
        s.book.busy = false;
        s.book.close_after
    };
    let waiting = [(crate::lock(&PENDING).is_some(), MSG_OPEN), (crate::lock(&MOVED).is_some(), MSG_MOVE), (close, MSG_CLOSE)];
    for (_, msg) in waiting.into_iter().filter(|(w, _)| *w) {
        let _ = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0));
    }
    if late.load(Ordering::Acquire) {
        Err(Late)
    } else {
        Ok(r)
    }
}

/// A call into the handler missed its deadline. The handler is never called again, not even to
/// be released (that call could hang as the first did), so its reference is leaked; the host is
/// hidden and let go, so the next open makes a new one; and the open waiting on it, and any open
/// queued behind it, hear why at once.
unsafe fn abandon(hwnd: HWND, st: &Shared) {
    let (h, reply, what) = {
        let mut s = st.borrow_mut();
        s.book.abandoned = true;
        s.book.showing = false;
        (s.handler.take(), s.reply.take(), s.what.clone())
    };
    std::mem::forget(h);
    crate::applog!("preview: the {what} handler missed its deadline; abandoned");
    if let Some(r) = reply {
        let _ = r.send(Err(late_text(&what)));
    }
    let _ = ShowWindow(hwnd, SW_HIDE);
    if HOST.compare_exchange(hwnd.0 as isize, 0, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        if let Some(p) = crate::lock(&PENDING).take() {
            let _ = p.reply.send(Err(late_text(&what)));
        }
    }
}

/// Lets go of the handler showing, if any: unloaded (it drops the document), then released (a
/// surrogate exits on its own once nothing holds its handler), each with `limit` as its deadline.
unsafe fn unload(hwnd: HWND, st: &Shared, limit: Duration) {
    let h = st.borrow_mut().handler.take();
    let Some(h) = h else { return };
    if call(hwnd, st, limit, || h.Unload()).is_err() {
        std::mem::forget(h);
        return abandon(hwnd, st);
    }
    if call(hwnd, st, limit, move || drop(h)).is_err() {
        abandon(hwnd, st);
    }
}

/// Creates the handler for `path`'s type, hands it the file and has it draw in the host. The
/// handler is told the file by path when it takes one (Office's do), else by stream, else by
/// shell item; the first it accepts wins. Every call has a deadline (`call`).
unsafe fn show(hwnd: HWND, st: &Shared, path: &Path, f: &Frame) -> Result<bool, Fail> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = ext_of(&name).ok_or_else(|| "This file has no type to preview by.".to_string())?;
    st.borrow_mut().what = ext.clone();
    let clsid = handler_for(&ext, &Registry).ok_or_else(|| format!("Windows has no preview for {ext} files."))?;
    let guid = CLSIDFromString(PCWSTR(wide(&clsid).as_ptr())).map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    // Out of process only, as Explorer does (the module's doc says why). A handler COM can't run
    // that way, though `available` read it as one it could, opens in its app instead.
    let handler: IPreviewHandler = match call(hwnd, st, SLOW, || CoCreateInstance(&guid, None, CLSCTX_LOCAL_SERVER))? {
        Ok(h) => h,
        Err(e) if e.code() == REGDB_E_CLASSNOTREG || e.code() == CO_E_APPNOTFOUND => return Ok(false),
        Err(e) => return Err(format!("Windows couldn't start the preview for {ext} files: {}", e.message()).into()),
    };
    // Held by the host from here, so a failure below releases it as a close does, and a missed
    // deadline leaks it (`abandon`); this clone's release is local to the process.
    st.borrow_mut().handler = Some(handler.clone());
    crate::applog!("preview: {name}: handler {clsid} up in {:?}", t.elapsed());
    // A handler wants the path with backslashes: Office's refuse a forward-slash one.
    let spelled = path.to_string_lossy().replace('/', "\\");
    let wpath = wide(&spelled);
    let mut errs = Vec::new();
    let inited = call(hwnd, st, SLOW, || {
        let by_file = match handler.cast::<IInitializeWithFile>() {
            Ok(i) => i.Initialize(PCWSTR(wpath.as_ptr()), STGM_READ.0).map_err(|e| errs.push(e.message())).is_ok(),
            Err(_) => false,
        };
        by_file
            || match handler.cast::<IInitializeWithStream>() {
                Ok(i) => SHCreateStreamOnFileEx(PCWSTR(wpath.as_ptr()), (STGM_READ | STGM_SHARE_DENY_NONE).0, 0, false, None)
                    .and_then(|s| i.Initialize(&s, STGM_READ.0))
                    .map_err(|e| errs.push(e.message()))
                    .is_ok(),
                Err(_) => false,
            }
            || match handler.cast::<IInitializeWithItem>() {
                Ok(i) => SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(wpath.as_ptr()), None)
                    .and_then(|item| i.Initialize(&item, STGM_READ.0))
                    .map_err(|e| errs.push(e.message()))
                    .is_ok(),
                Err(_) => false,
            }
    })?;
    if !inited {
        let why = errs.into_iter().find(|e| !e.is_empty()).unwrap_or_else(|| "it refused the file".to_string());
        return Err(format!("The preview couldn't open this file: {why}.").into());
    }
    crate::applog!("preview: {name}: file given in {:?}", t.elapsed());
    // The panel's latest place: the page may have moved it, recoloured it or hidden it for a
    // dialog while the handler started and read the file.
    let at = st.borrow().book.at.clone().unwrap_or_else(|| f.clone());
    let bg = colour(&at.bg);
    st.borrow_mut().bg = bg;
    call(hwnd, st, QUICK, || {
        if let Ok(v) = handler.cast::<IPreviewHandlerVisuals>() {
            let _ = v.SetBackgroundColor(bg);
            let _ = v.SetTextColor(colour(&at.fg));
        }
    })?;
    let (owner, client) = (owner_shown(hwnd), owner_client(hwnd));
    let show = {
        let mut s = st.borrow_mut();
        s.book.paint(&at);
        s.book.placed = true;
        s.book.anchor(client);
        s.book.visible(owner)
    };
    place(hwnd, &at, show);
    let rc = RECT { left: 0, top: 0, right: at.w, bottom: at.h };
    call(hwnd, st, QUICK, || handler.SetWindow(hwnd, &rc))?.map_err(|e| e.message())?;
    crate::applog!("preview: {name}: window set in {:?}", t.elapsed());
    call(hwnd, st, SLOW, || handler.DoPreview())?.map_err(|e| format!("The preview couldn't draw this file: {}.", e.message()))?;
    crate::applog!("preview: {name}: drawn in {:?}", t.elapsed());
    Ok(true)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let bg = state(hwnd).and_then(|st| st.try_borrow().ok().map(|s| s.bg)).unwrap_or(COLORREF(0x00FFFFFF));
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let brush = CreateSolidBrush(bg);
            FillRect(HDC(wparam.0 as _), &rc, brush);
            let _ = DeleteObject(brush.into());
            LRESULT(1)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        MSG_OPEN => {
            if let Some(st) = state(hwnd).filter(|st| !st.borrow().book.abandoned) {
                open_next(hwnd, &st);
            }
            LRESULT(0)
        }
        MSG_MOVE => {
            if let Some(st) = state(hwnd).filter(|st| !st.borrow().book.abandoned) {
                move_next(hwnd, &st);
            }
            LRESULT(0)
        }
        MSG_CLOSE => {
            if let Some(st) = state(hwnd).filter(|st| !st.borrow().book.abandoned) {
                let id = st.borrow().book.id;
                let step = st.borrow_mut().book.close(closed(id));
                // Hidden at once; the handler goes now, or when a call in flight ends.
                if step != Step::Skip {
                    // The focus was in the preview (a click into it): back to the page, which
                    // would otherwise have no key.
                    let refocus = focus_in(hwnd);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    if refocus {
                        give_back_keys();
                    }
                }
                if step == Step::Go {
                    unload(hwnd, &st, QUICK);
                }
            }
            LRESULT(0)
        }
        MSG_LATE => {
            // The watchdog's word that call `wparam` missed its deadline, sent so that COM's wait
            // delivers it. Still in flight here means the cancel didn't end it.
            if let Some(st) = state(hwnd) {
                let stuck = st.try_borrow().is_ok_and(|s| s.book.busy && !s.book.abandoned && s.calls == wparam.0 as u64);
                if stuck {
                    abandon(hwnd, &st);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // Before the handler's window, a child of this one, goes with it: unloaded first, as
            // Explorer does, with the deadline every call has, or left if a call is in flight.
            if let Some(st) = state(hwnd) {
                let_go(hwnd, &st, QUICK);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>;
            if !p.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                // The window's own reference; a message still being handled keeps its own. A
                // handler still held is one `let_go` left, whose release could hang too.
                let st = Rc::from_raw(p);
                if let Some(h) = st.try_borrow_mut().ok().and_then(|mut s| s.handler.take()) {
                    std::mem::forget(h);
                }
            }
            if HOST.compare_exchange(hwnd.0 as isize, 0, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                // An open still waiting hears now that it won't be shown, rather than at the backstop.
                crate::lock(&PENDING).take();
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// The keys go back to the page: Esc and Space, which close the panel there, reach it again. The
/// host never takes the focus, but a click into the document gives it to the handler's window, in
/// its own process, as in Explorer's preview pane; keys then go to the document (Space scrolls)
/// until the user clicks the page, as Brainstead sees no key of another process. Office can also
/// take the focus as it starts drawing, which an open hands back (`open_next`).
fn give_back_keys() {
    if let Some(w) = APP.get().and_then(|a| a.get_webview_window("main")) {
        let _ = w.set_focus();
    }
}

/// Whether the keyboard focus is in the host or a window inside it (the handler's, in its own
/// process), in the thread that has the foreground.
unsafe fn focus_in(host: HWND) -> bool {
    let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
    GetGUIThreadInfo(0, &mut gi).is_ok() && !gi.hwndFocus.is_invalid() && (gi.hwndFocus == host || IsChild(host, gi.hwndFocus).as_bool())
}

/// Takes up the open waiting in `PENDING`, unless a call is in flight (its end posts again).
unsafe fn open_next(hwnd: HWND, st: &Shared) {
    if st.borrow().book.busy {
        return;
    }
    let Some(req) = crate::lock(&PENDING).take() else { return };
    unload(hwnd, st, QUICK);
    if st.borrow().book.abandoned {
        let what = st.borrow().what.clone();
        let _ = req.reply.send(Err(late_text(&what)));
        return;
    }
    match st.borrow_mut().book.open(&req.frame, closed(req.frame.id)) {
        Step::Go => {}
        Step::Skip => {
            let _ = req.reply.send(Err(CLOSED_TEXT.to_string()));
            return;
        }
        Step::Wait => {
            // A call began while the previous handler was let go: back in the slot, unless a
            // newer open took it meanwhile.
            let mut slot = crate::lock(&PENDING);
            if slot.is_none() {
                *slot = Some(req);
            }
            return;
        }
    }
    {
        let mut s = st.borrow_mut();
        s.bg = colour(&req.frame.bg);
        // Kept with the host, so a missed deadline can answer for the open (`abandon`).
        s.reply = Some(req.reply);
    }
    let r = show(hwnd, st, &req.path, &req.frame);
    if let Err(Fail::Said(e)) = &r {
        crate::applog!("preview: {}: {e}", req.path.display());
    }
    if matches!(r, Err(Fail::Late)) || st.borrow().book.abandoned {
        // Abandoned already when the watchdog's message came first, else now.
        if !st.borrow().book.abandoned {
            abandon(hwnd, st);
        }
        return;
    }
    let keep = st.borrow_mut().book.opened(matches!(r, Ok(true)), closed(req.frame.id));
    // Office can take the focus as it starts drawing: the page keeps the keys, so Esc and Space
    // still close the panel.
    let refocus = focus_in(hwnd);
    if !keep {
        unload(hwnd, st, QUICK);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    if refocus {
        give_back_keys();
    }
    let r = match r {
        // Closed while it opened: the page has moved on, and says nothing more.
        Ok(true) if !keep => Err(CLOSED_TEXT.to_string()),
        Ok(here) => Ok(here),
        Err(Fail::Said(e)) => Err(e),
        Err(Fail::Late) => Err(late_text("")),
    };
    let reply = st.borrow_mut().reply.take();
    if let Some(reply) = reply {
        let _ = reply.send(r);
    }
}

/// Takes up the panel's latest place from `MOVED`, unless a call is in flight (its end posts
/// again). The page's places can't arrive during a call anyway (`Book::follow_resize` says why).
unsafe fn move_next(hwnd: HWND, st: &Shared) {
    if st.borrow().book.busy {
        return;
    }
    let Some(f) = crate::lock(&MOVED).take() else { return };
    let owner = owner_shown(hwnd);
    let client = owner_client(hwnd);
    let (here, show) = {
        let mut s = st.borrow_mut();
        let here = s.book.moved(&f);
        s.book.anchor(client);
        (here && s.book.placed, s.book.visible(owner))
    };
    if !here {
        return;
    }
    place(hwnd, &f, show);
    let recolour = st.borrow().book.recoloured(&f);
    let Some(h) = st.borrow().handler.clone() else { return };
    let rc = RECT { left: 0, top: 0, right: f.w, bottom: f.h };
    let mut done = call(hwnd, st, QUICK, || h.SetRect(&rc)).is_ok();
    // Light and dark switched: the host's own background and the handler's colours follow.
    if done && recolour {
        let bg = colour(&f.bg);
        st.borrow_mut().bg = bg;
        let _ = InvalidateRect(Some(hwnd), None, true);
        done = call(hwnd, st, QUICK, || {
            if let Ok(v) = h.cast::<IPreviewHandlerVisuals>() {
                let _ = v.SetBackgroundColor(bg);
                let _ = v.SetTextColor(colour(&f.fg));
            }
        })
        .is_ok();
        if done {
            st.borrow_mut().book.paint(&f);
        }
    }
    if !done {
        std::mem::forget(h);
        abandon(hwnd, st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A registry as a table of `key\name` (lower-cased, as the registry ignores case) to value.
    struct Fake {
        values: HashMap<String, String>,
        choice: HashMap<String, String>,
    }

    /// `key\shellex\{the preview handler category}`.
    fn shellex(key: &str) -> String {
        format!("{key}\\shellex\\{PREVIEW_HANDLER}")
    }

    fn fake(values: &[(&str, &str, &str)], choice: &[(&str, &str)]) -> Fake {
        Fake {
            values: values
                .iter()
                .map(|(k, n, v)| (format!("{}\\{}", k.to_ascii_lowercase(), n.to_ascii_lowercase()), v.to_string()))
                .collect(),
            choice: choice.iter().map(|(e, p)| (e.to_string(), p.to_string())).collect(),
        }
    }

    impl Classes for Fake {
        fn value(&self, key: &str, name: &str) -> Option<String> {
            self.values.get(&format!("{}\\{}", key.to_ascii_lowercase(), name.to_ascii_lowercase())).cloned()
        }
        fn user_choice(&self, ext: &str) -> Option<String> {
            self.choice.get(ext).cloned()
        }
    }

    const WORD: &str = "{84F66100-FF7C-4fb4-B0C0-02CD7FB668FE}";
    const TXT: &str = "{1531d583-8375-4d3f-b5fb-d23bbd169f22}";

    #[test]
    fn office_registers_under_the_extension() {
        let docx = shellex(".docx");
        let reg = fake(&[(".docx", "", "Word.Document.12"), (docx.as_str(), "", WORD)], &[]);
        assert_eq!(handler_for(".docx", &reg).as_deref(), Some(WORD));
        assert_eq!(handler_for(".DOCX", &reg).as_deref(), Some(WORD), "the registry ignores case");
    }

    #[test]
    fn through_the_progid_and_its_current_version() {
        let rtf = shellex("rtffile.2");
        let reg = fake(
            &[(".rtf", "", "rtffile"), ("rtffile\\CurVer", "", "rtffile.2"), (rtf.as_str(), "", "a42c2ccb-67d3-46fa-abe6-7d2f3488c7a3")],
            &[],
        );
        assert_eq!(handler_for(".rtf", &reg).as_deref(), Some("{a42c2ccb-67d3-46fa-abe6-7d2f3488c7a3}"), "braces put on");
    }

    #[test]
    fn the_users_choice_wins_over_the_extension() {
        let (txt, other) = (shellex(".txt"), shellex("Other.Editor"));
        let reg = fake(&[(txt.as_str(), "", TXT), (other.as_str(), "", WORD)], &[(".txt", "Other.Editor")]);
        assert_eq!(handler_for(".txt", &reg).as_deref(), Some(WORD));
        // A chosen program with no handler of its own falls through to the extension's.
        let reg = fake(&[(txt.as_str(), "", TXT)], &[(".txt", "Other.Editor")]);
        assert_eq!(handler_for(".txt", &reg).as_deref(), Some(TXT));
    }

    #[test]
    fn system_associations_by_extension_then_perceived_type() {
        let log = shellex("SystemFileAssociations\\.log");
        let reg = fake(&[(log.as_str(), "", TXT)], &[]);
        assert_eq!(handler_for(".log", &reg).as_deref(), Some(TXT));
        let text = shellex("SystemFileAssociations\\text");
        let reg = fake(&[(".ini", "", "inifile"), (".ini", "PerceivedType", "text"), (text.as_str(), "", TXT)], &[]);
        assert_eq!(handler_for(".ini", &reg).as_deref(), Some(TXT));
    }

    #[test]
    fn no_handler_and_bad_values() {
        let bad = shellex(".bad");
        let reg = fake(&[(".zip", "", "CompressedFolder"), (bad.as_str(), "", "not a clsid")], &[]);
        assert_eq!(handler_for(".zip", &reg), None);
        assert_eq!(handler_for(".bad", &reg), None);
        assert_eq!(handler_for(".nothing", &reg), None);
    }

    #[test]
    fn hosted_out_of_process_or_not_at_all() {
        let (word, txt, dll, app) = (
            "{84F66100-FF7C-4fb4-B0C0-02CD7FB668FE}",
            TXT,
            "{11111111-2222-3333-4444-555555555555}",
            "{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}",
        );
        let reg = fake(
            &[
                (&format!("CLSID\\{word}\\LocalServer32"), "", "WINWORD.EXE"),
                (&format!("CLSID\\{txt}"), "AppID", app),
                (&format!("AppID\\{app}"), "DllSurrogate", "prevhost.exe"),
                (&format!("CLSID\\{dll}\\InprocServer32"), "", "handler.dll"),
            ],
            &[],
        );
        assert!(hostable(word, &reg), "its own server");
        assert!(hostable(txt, &reg), "in the surrogate its AppID names");
        assert!(!hostable(dll, &reg), "a DLL with no surrogate would load into Brainstead");
        let reg = fake(&[(&format!("WOW6432Node\\CLSID\\{dll}"), "AppID", app), (&format!("AppID\\{app}"), "DllSurrogate", "")], &[]);
        assert!(hostable(dll, &reg), "a 32-bit handler in the default surrogate");
    }

    fn frame(id: u64, x: i32) -> Frame {
        Frame { x, y: 40, w: 800, h: 600, bg: "#ffffff".into(), fg: "#101820".into(), radius: 0, id, hidden: false, holes: vec![] }
    }

    #[test]
    fn a_close_during_a_call_waits_for_its_end_and_then_wins() {
        let mut b = Book::default();
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        b.busy = true;
        assert_eq!(b.close(true), Step::Wait, "never a second call into a busy handler");
        assert_eq!(b.open(&frame(2, 20), false), Step::Wait, "an open waits in its slot too");
        b.busy = false;
        assert!(!b.opened(true, false), "the open that was closed under it doesn't stay up");
        assert!(!b.showing && b.at.is_none() && !b.close_after);
    }

    #[test]
    fn an_open_stays_up_and_follows_moves() {
        let mut b = Book::default();
        assert!(!b.moved(&frame(1, 5)), "nothing to move before an open");
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        assert!(b.opened(true, false));
        assert!(b.moved(&frame(1, 30)));
        assert_eq!(b.at.as_ref().map(|f| f.x), Some(30));
        assert_eq!(b.close(true), Step::Go);
        assert!(!b.showing && b.at.is_none());
        assert_eq!(b.open(&frame(2, 10), false), Step::Go);
        assert!(!b.opened(false, false), "a failed open leaves nothing up");
    }

    #[test]
    fn never_on_screen_without_the_main_window() {
        let mut b = Book::default();
        assert!(!b.visible(true), "nothing up");
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        assert!(!b.visible(true), "not before the handler has the file: the page says it is opening");
        b.placed = true;
        assert!(b.visible(true), "then shown while it draws, so the handler draws into it");
        assert!(!b.visible(false), "an open that ends while the window is in the tray stays hidden");
        assert!(b.opened(true, false));
        assert!(!b.visible(false), "a move or scale change while hidden doesn't show it");
        b.busy = true;
        assert_eq!(b.close(true), Step::Wait);
        assert!(!b.visible(true), "closed mid-call: hidden though the call goes on");
    }

    #[test]
    fn a_theme_change_recolours_the_preview_up() {
        let mut b = Book::default();
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        b.paint(&frame(1, 10));
        assert!(b.opened(true, false));
        assert!(!b.recoloured(&frame(1, 30)), "a move alone");
        let dark = Frame { bg: "#1b1d22".into(), fg: "#e8e6e1".into(), ..frame(1, 10) };
        assert!(b.recoloured(&dark));
        assert!(!b.recoloured(&Frame { id: 2, ..dark.clone() }), "another panel's");
        assert!(b.moved(&dark));
        assert!(b.recoloured(&dark), "moved, but the handler hasn't the colours yet");
        b.paint(&dark);
        assert!(!b.recoloured(&dark), "once taken");
    }

    #[test]
    fn a_resize_during_a_slow_call_is_followed_from_the_window() {
        let mut b = Book::default();
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        // Office's first start: the page's text shows, and the host isn't placed yet.
        b.busy = true;
        assert!(b.follow_resize((1000, 800)).is_none(), "nothing on screen to resize");
        // The handler has the file: the host is put over the panel (800x600 at 10,40 in a
        // 1000x800 client area) and DoPreview is in flight for seconds.
        b.placed = true;
        b.anchor((1000, 800));
        assert_eq!(b.insets, Some((190, 160)));
        let f = b.follow_resize((900, 700)).expect("the window shrank during the call");
        assert_eq!((f.x, f.y, f.w, f.h), (10, 40, 700, 500), "the body keeps its insets");
        assert!(b.follow_resize((900, 700)).is_none(), "a move alone");
        b.busy = false;
        assert!(b.follow_resize((800, 600)).is_none(), "between calls the page's own place comes");
        assert!(b.opened(true, false));
    }

    #[test]
    fn hidden_while_the_page_shows_a_dialog_over_it() {
        let mut b = Book::default();
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        b.placed = true;
        assert!(b.opened(true, false));
        assert!(b.moved(&Frame { hidden: true, ..frame(1, 10) }));
        assert!(!b.visible(true), "the Quit question is up");
        assert!(b.moved(&frame(1, 10)));
        assert!(b.visible(true), "and back once it's answered");
    }

    #[test]
    fn a_close_that_arrives_before_its_open_wins() {
        let mut gone = Closed(VecDeque::new());
        gone.close(7);
        let mut b = Book::default();
        assert_eq!(b.open(&frame(7, 10), gone.has(7)), Step::Skip, "the panel closed before its open came");
        assert!(!b.showing);
        // Closed while it opened, with no call in flight to see it: the end of the open does.
        assert_eq!(b.open(&frame(8, 10), gone.has(8)), Step::Go);
        gone.close(8);
        assert!(!b.opened(true, gone.has(8)));
    }

    #[test]
    fn a_close_or_move_from_a_panel_gone_leaves_the_next_alone() {
        let mut b = Book::default();
        assert_eq!(b.open(&frame(2, 10), false), Step::Go);
        assert!(b.opened(true, false));
        // Panel 1's close (panel 2 isn't closed) and its last move arrive late.
        assert_eq!(b.close(false), Step::Skip);
        assert!(!b.moved(&frame(1, 99)));
        assert!(b.showing && b.at.as_ref().is_some_and(|f| f.x == 10));
        let mut gone = Closed(VecDeque::new());
        for id in 0..100 {
            gone.close(id);
        }
        assert!(gone.has(99) && !gone.has(0), "only the last few are kept");
        assert_eq!(gone.0.len(), Closed::KEPT);
    }

    #[test]
    fn letting_go_never_calls_a_busy_or_abandoned_handler() {
        let mut b = Book::default();
        assert_eq!(b.teardown(false), Teardown::Nothing, "nothing up");
        assert_eq!(b.open(&frame(1, 10), false), Step::Go);
        b.busy = true;
        assert_eq!(b.teardown(false), Teardown::Leave, "quitting while the handler starts: the call is cancelled, nothing called");
        assert_eq!(b.teardown(true), Teardown::Leave, "quitting while it draws: the same");
        b.busy = false;
        assert!(b.opened(true, false));
        assert_eq!(b.teardown(true), Teardown::Unload, "up and idle: unloaded and released, each with a deadline");
        b.abandoned = true;
        assert_eq!(b.teardown(true), Teardown::Nothing, "a handler that missed a deadline is never called again");
    }

    #[test]
    fn a_late_handler_is_named_and_its_deadline_holds() {
        assert!(late_text(".pptx").starts_with("Windows' preview for .pptx files didn't answer"));
        assert!(late_text("").contains("this type of files"));
        assert!(late_text(".docx").ends_with("Open in its app shows the file."));
        if std::env::var("BRAINSTEAD_PREVIEW_DEADLINE_MS").is_err() {
            assert_eq!(deadline(SLOW), SLOW);
        }
        assert!(3 * SLOW + 4 * QUICK < BACKSTOP, "open's own wait outlasts every call of an unload and an open");
    }

    #[test]
    fn a_file_from_the_web_opens_in_its_app_as_explorer_has_it() {
        assert_eq!(zone_id(b"[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.com/\r\n"), Some(3));
        assert_eq!(zone_id(b"[ZoneTransfer]\nzoneid = 4\n"), Some(4), "any case and spacing");
        let utf16: Vec<u8> =
            [0xFF, 0xFE].into_iter().chain("[ZoneTransfer]\r\nZoneId=3\r\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(zone_id(&utf16), Some(3), "UTF-16 with its mark");
        assert_eq!(zone_id(b"[ZoneTransfer]\r\n"), None);
        // A real file and its stream (NTFS, in a temp folder, never the vault).
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Orbit App plan.docx");
        std::fs::write(&file, b"not really a docx").unwrap();
        assert_eq!(refused(&file), None, "made here: previewed");
        let stream = |zone: u32| {
            let mut s = file.as_os_str().to_os_string();
            s.push(":Zone.Identifier");
            std::fs::write(s, format!("[ZoneTransfer]\r\nZoneId={zone}\r\n")).unwrap();
        };
        stream(2);
        assert_eq!(refused(&file), None, "a trusted site: previewed");
        stream(3);
        assert!(refused(&file).is_some_and(|why| why.contains("downloaded from the internet")), "the internet: its app");
    }

    #[test]
    fn names_and_colours() {
        assert_eq!(ext_of("Orbit App.docx").as_deref(), Some(".docx"));
        assert_eq!(ext_of("notes.Backup.XLSX").as_deref(), Some(".xlsx"));
        assert_eq!(ext_of("Makefile"), None);
        assert_eq!(colour("#ff8000").0, 0x0080ff);
        assert_eq!(colour("nope").0, 0xffffff);
    }
}
