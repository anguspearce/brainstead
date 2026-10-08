// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::process::Command;

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn native_host_dirs() -> Vec<(&'static str, PathBuf)> {
    let base = home().join("Library/Application Support");
    [
        ("Chrome", "Google/Chrome"),
        ("Chrome Beta", "Google/Chrome Beta"),
        ("Chrome Canary", "Google/Chrome Canary"),
        ("Chromium", "Chromium"),
        ("Edge", "Microsoft Edge"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Vivaldi", "Vivaldi"),
    ]
    .into_iter()
    .filter(|(_, d)| base.join(d).is_dir())
    .map(|(n, d)| (n, base.join(d).join("NativeMessagingHosts")))
    .collect()
}

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

/// Whether Brainstead opens at login (SMAppService.mainApp: macOS 13 and later, the switch in
/// System Settings › General › Login Items too). Some(true) when enabled, Some(false) when not,
/// None outside an app bundle (`make dev`), which SMAppService can't register. As in the sibling
/// apps, "requires approval" counts as off (the user switched it off in System Settings), and
/// availability is the bundle, not the status: macOS says "not found" for a bundle never registered.
pub fn open_at_login() -> Option<bool> {
    let svc = login_service()?;
    in_app_bundle().then(|| login_status(svc) == 1)
}

/// SMAppServiceStatus: 0 not registered, 1 enabled, 2 requires approval, 3 not found.
fn login_status(svc: *mut objc2::runtime::AnyObject) -> isize {
    unsafe { objc2::msg_send![svc, status] }
}

fn login_service() -> Option<*mut objc2::runtime::AnyObject> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    let cls = AnyClass::get(c"SMAppService")?;
    let svc: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
    (!svc.is_null()).then_some(svc)
}

/// Whether this process runs from inside a `.app` bundle (…/Name.app/Contents/MacOS/exe).
fn in_app_bundle() -> bool {
    std::env::current_exe().ok().and_then(|p| p.ancestors().nth(3).map(|a| a.extension().is_some_and(|e| e == "app"))).unwrap_or(false)
}

/// Turns it on or off. When macOS wants the user to approve it, System Settings opens at Login Items.
pub fn set_open_at_login(on: bool) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    let svc = login_service().ok_or("Opening at login needs macOS 13 or later.")?;
    let mut err: *mut AnyObject = std::ptr::null_mut();
    let ok: bool = unsafe {
        if on {
            msg_send![svc, registerAndReturnError: &mut err]
        } else {
            msg_send![svc, unregisterAndReturnError: &mut err]
        }
    };
    if on && login_status(svc) == 2 {
        let _ = Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.LoginItems-Settings.extension").status();
        return Ok(());
    }
    if ok {
        return Ok(());
    }
    let why: Option<objc2::rc::Retained<objc2_foundation::NSString>> =
        if err.is_null() { None } else { unsafe { msg_send![err, localizedDescription] } };
    Err(why.map(|w| w.to_string()).unwrap_or_else(|| "macOS refused.".into()))
}

pub fn reveal(path: &Path) -> Result<(), String> {
    let st = Command::new("/usr/bin/open").arg("-R").arg(path).status().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("Finder couldn't show {}", path.display()))
    }
}

/// Opening (not just looking up) a file only Full Disk Access unlocks. TCC's own database is there
/// on every Mac; Safari's bookmarks are the usual fallback.
pub fn full_disk_access() -> Option<bool> {
    let h = home();
    for p in [h.join("Library/Application Support/com.apple.TCC/TCC.db"), h.join("Library/Safari/Bookmarks.plist")] {
        match std::fs::File::open(&p) {
            Ok(_) => return Some(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Some(false),
        }
    }
    None
}

pub fn open_full_disk_access_settings() -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// HTML and plain text onto the clipboard, as one item, so a rich-text paste (Mail, Word, Teams)
/// gets the formatting and a plain one the text.
pub fn copy_html(html: &str, text: &str) -> Result<(), String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    #[allow(unused_unsafe)]
    let ok = unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(html), NSPasteboardTypeHTML)
            && pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString)
    };
    if ok {
        Ok(())
    } else {
        Err("Couldn't put that on the clipboard.".into())
    }
}

/// Starts printing the page in `webview` (a WKWebView), with its @media print styles, to a PDF
/// at `dest`. Runs as a sheet on the window, without the print panel: the synchronous
/// `runOperation` prints blank pages from a WKWebView (§5.3). Call on the main thread. The PDF
/// appears at `dest` when it's done.
///
/// # Safety
/// `webview` must be a live WKWebView.
pub unsafe fn print_to_pdf(webview: *mut objc2::runtime::AnyObject, dest: &Path) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Sel};
    use objc2_app_kit::{NSPrintInfo, NSPrintJobSavingURL, NSPrintOperation, NSPrintSaveJob, NSPrintingPaginationMode};
    use objc2_foundation::{NSRect, NSString, NSURL};

    let shared = NSPrintInfo::sharedPrintInfo();
    let info: Retained<NSPrintInfo> = msg_send![&*shared, copy];
    info.setJobDisposition(NSPrintSaveJob);
    let url = NSURL::fileURLWithPath(&NSString::from_str(&dest.to_string_lossy()));
    let dict = info.dictionary();
    let _: () = msg_send![&*dict, setObject: &*url, forKey: NSPrintJobSavingURL];
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticallyCentered(false);
    // 0.5in all round, as the previous app's @page; the footer goes in the bottom margin.
    info.setTopMargin(36.0);
    info.setBottomMargin(36.0);
    info.setLeftMargin(36.0);
    info.setRightMargin(36.0);

    let op: Option<Retained<NSPrintOperation>> = msg_send![webview, printOperationWithPrintInfo: &*info];
    let op = op.ok_or("WebKit couldn't print this page.")?;
    op.setShowsPrintPanel(false);
    op.setShowsProgressPanel(false);
    // The operation's view starts with no size, which prints nothing: give it the webview's.
    let bounds: NSRect = msg_send![webview, bounds];
    let view = op.view();
    if let Some(v) = view {
        v.setFrame(bounds);
    }
    let window: *mut AnyObject = msg_send![webview, window];
    if window.is_null() {
        return Err("The window isn't open.".into());
    }
    let none: Option<Sel> = None;
    let _: () = msg_send![
        &*op,
        runOperationModalForWindow: window,
        delegate: std::ptr::null::<AnyObject>(),
        didRunSelector: none,
        contextInfo: std::ptr::null_mut::<std::ffi::c_void>()
    ];
    Ok(())
}

#[link(name = "PDFKit", kind = "framework")]
unsafe extern "C" {
    static PDFAnnotationSubtypeFreeText: &'static objc2_foundation::NSString;
    static PDFDocumentBurnInAnnotationsOption: &'static objc2_foundation::NSString;
}

/// A PDF's text, one string per page, by PDFKit. Safe from any thread: each call has its own
/// autorelease pool.
pub fn pdf_text(path: &Path) -> Result<Vec<String>, String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSString, NSURL};

    objc2::rc::autoreleasepool(|_| unsafe {
        let cls = AnyClass::get(c"PDFDocument").ok_or("PDFKit isn't available.")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let doc: Option<Retained<AnyObject>> = msg_send![msg_send![cls, alloc], initWithURL: &*url];
        let doc = doc.ok_or("Not a readable PDF.")?;
        let locked: bool = msg_send![&*doc, isLocked];
        if locked {
            return Err("The PDF is password-protected.".into());
        }
        let n: usize = msg_send![&*doc, pageCount];
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let page: Option<Retained<AnyObject>> = msg_send![&*doc, pageAtIndex: i];
            let text: Option<Retained<NSString>> = match page {
                Some(p) => msg_send![&*p, string],
                None => None,
            };
            out.push(text.map(|t| t.to_string()).unwrap_or_default());
        }
        Ok(out)
    })
}

#[link(name = "Vision", kind = "framework")]
unsafe extern "C" {}

/// The text in an image by Vision's text recognition (accurate, with language correction), one
/// line per line found, top to bottom. ImageIO reads the file, so HEIC, WebP and GIF work too.
/// Safe from any thread: each call has its own request and autorelease pool.
pub fn image_text(path: &Path) -> Result<String, String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};

    objc2::rc::autoreleasepool(|_| unsafe {
        let req_cls = AnyClass::get(c"VNRecognizeTextRequest").ok_or("Vision isn't available.")?;
        let handler_cls = AnyClass::get(c"VNImageRequestHandler").ok_or("Vision isn't available.")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let req: Option<Retained<AnyObject>> = msg_send![msg_send![req_cls, alloc], init];
        let req = req.ok_or("Couldn't start reading the image.")?;
        // VNRequestTextRecognitionLevelAccurate.
        let _: () = msg_send![&*req, setRecognitionLevel: 0isize];
        let _: () = msg_send![&*req, setUsesLanguageCorrection: true];
        let options = NSDictionary::<AnyObject, AnyObject>::new();
        let handler: Option<Retained<AnyObject>> = msg_send![msg_send![handler_cls, alloc], initWithURL: &*url, options: &*options];
        let handler = handler.ok_or("Not a readable image.")?;
        let reqs = NSArray::<AnyObject>::from_slice(&[&*req]);
        let ok: bool = msg_send![&*handler, performRequests: &*reqs, error: std::ptr::null_mut::<*mut AnyObject>()];
        if !ok {
            return Err("Not a readable image.".into());
        }
        let results: Option<Retained<NSArray<AnyObject>>> = msg_send![&*req, results];
        let mut lines = Vec::new();
        for obs in results.iter().flat_map(|r| r.iter()) {
            let top: Option<Retained<NSArray<AnyObject>>> = msg_send![&*obs, topCandidates: 1usize];
            let Some(best) = top.and_then(|t| t.firstObject()) else { continue };
            let text: Option<Retained<NSString>> = msg_send![&*best, string];
            if let Some(t) = text {
                lines.push(t.to_string());
            }
        }
        Ok(lines.join("\n"))
    })
}

/// Writes the image as a JPEG at most 2000 pixels across to `dest`, by `sips` (never larger than it was).
pub fn image_for_model(path: &Path, dest: &Path) -> Result<(), String> {
    const MAX: u32 = 2000;
    let run = |c: &mut Command| c.output().map_err(|e| format!("Couldn't run sips: {e}"));
    let size = run(Command::new("/usr/bin/sips").args(["-g", "pixelWidth", "-g", "pixelHeight"]).arg(path))?;
    let listed = String::from_utf8_lossy(&size.stdout).into_owned();
    let px = |key: &str| listed.lines().find_map(|l| l.trim().strip_prefix(key).and_then(|v| v.trim().parse::<u32>().ok())).unwrap_or(0);
    let mut cmd = Command::new("/usr/bin/sips");
    cmd.args(["-s", "format", "jpeg"]);
    if px("pixelWidth:").max(px("pixelHeight:")) > MAX {
        cmd.args(["-Z", &MAX.to_string()]);
    }
    let out = run(cmd.arg(path).arg("--out").arg(dest))?;
    if out.status.success() && dest.is_file() {
        Ok(())
    } else {
        let _ = std::fs::remove_file(dest);
        Err(format!("Couldn't prepare the image: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Removes the last page when it has no text (see `platform::drop_blank_last_page`).
pub fn drop_blank_last_page(path: &Path) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSString, NSURL};

    objc2::rc::autoreleasepool(|_| unsafe {
        let cls = AnyClass::get(c"PDFDocument").ok_or("PDFKit isn't available.")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let doc: Option<Retained<AnyObject>> = msg_send![msg_send![cls, alloc], initWithURL: &*url];
        let doc = doc.ok_or("Couldn't open the exported PDF.")?;
        let n: usize = msg_send![&*doc, pageCount];
        if n < 2 {
            return Ok(());
        }
        let page: Option<Retained<AnyObject>> = msg_send![&*doc, pageAtIndex: n - 1];
        let Some(page) = page else { return Ok(()) };
        let text: Option<Retained<NSString>> = msg_send![&*page, string];
        if text.is_some_and(|t| !t.to_string().trim().is_empty()) {
            return Ok(());
        }
        let _: () = msg_send![&*doc, removePageAtIndex: n - 1];
        let ok: bool = msg_send![&*doc, writeToURL: &*url];
        if ok {
            Ok(())
        } else {
            Err("Couldn't tidy the exported PDF.".into())
        }
    })
}

/// Puts the previous app's running footer on every page of the PDF at `path`: the title bottom
/// left and "Page N of M" bottom right, 8pt grey, burnt into the page (WebKit's print has no
/// page counters, so it's added afterwards).
pub fn stamp_pdf_footer(path: &Path, title: &str) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};

    let title: String =
        if title.chars().count() > 90 { format!("{}…", title.chars().take(89).collect::<String>()) } else { title.into() };
    unsafe {
        let cls = |n: &str| AnyClass::get(&std::ffi::CString::new(n).unwrap()).ok_or(format!("{n} isn't available."));
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let doc: Option<Retained<AnyObject>> = msg_send![msg_send![cls("PDFDocument")?, alloc], initWithURL: &*url];
        let doc = doc.ok_or("Couldn't open the exported PDF.")?;
        let n: usize = msg_send![&*doc, pageCount];
        let font: Option<Retained<AnyObject>> = msg_send![cls("NSFont")?, fontWithName: &*NSString::from_str("Avenir Next"), size: 8.0f64];
        let font = match font {
            Some(f) => f,
            None => msg_send![cls("NSFont")?, systemFontOfSize: 8.0f64],
        };
        let grey =
            objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(0x8a as f64 / 255.0, 0x92 as f64 / 255.0, 0x9c as f64 / 255.0, 1.0);
        let clear = objc2_app_kit::NSColor::clearColor();
        let border: Retained<AnyObject> = msg_send![msg_send![cls("PDFBorder")?, alloc], init];
        let _: () = msg_send![&*border, setLineWidth: 0.0f64];
        for i in 0..n {
            let page: Option<Retained<AnyObject>> = msg_send![&*doc, pageAtIndex: i];
            let Some(page) = page else { continue };
            let b: NSRect = msg_send![&*page, boundsForBox: 0isize];
            let (x0, w) = (b.origin.x + 36.0, b.size.width - 72.0);
            // A free-text annotation pads its text, so the box is well over the 8pt line, or the
            // letters' tails are cut off.
            let y = b.origin.y + 8.0;
            for (text, right) in [(title.clone(), false), (format!("Page {} of {}", i + 1, n), true)] {
                let rect = NSRect::new(NSPoint::new(x0, y), NSSize::new(w, 20.0));
                let a: Retained<AnyObject> = msg_send![
                    msg_send![cls("PDFAnnotation")?, alloc],
                    initWithBounds: rect,
                    forType: PDFAnnotationSubtypeFreeText,
                    withProperties: std::ptr::null::<AnyObject>()
                ];
                let _: () = msg_send![&*a, setContents: &*NSString::from_str(&text)];
                let _: () = msg_send![&*a, setFont: &*font];
                let _: () = msg_send![&*a, setFontColor: &*grey];
                let _: () = msg_send![&*a, setColor: &*clear];
                let _: () = msg_send![&*a, setBorder: &*border];
                // Right is 2 on Apple silicon and 1 on Intel (NSTextAlignment follows iOS's values there).
                let align = if right { objc2_app_kit::NSTextAlignment::Right } else { objc2_app_kit::NSTextAlignment::Left };
                let _: () = msg_send![&*a, setAlignment: align];
                let _: () = msg_send![&*page, addAnnotation: &*a];
            }
        }
        let yes = objc2_foundation::NSNumber::new_bool(true);
        let opts: Retained<AnyObject> =
            msg_send![cls("NSDictionary")?, dictionaryWithObject: &*yes, forKey: PDFDocumentBurnInAnnotationsOption];
        let ok: bool = msg_send![&*doc, writeToURL: &*url, withOptions: &*opts];
        if ok {
            Ok(())
        } else {
            Err("Couldn't add the page footer to the PDF.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSString, NSURL};

    /// A one-page blank A4 PDF.
    const BLANK: &str = "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n";

    #[test]
    fn reads_the_text_in_an_image_and_shrinks_it_for_a_model() {
        // Vision and PDFKit share CoreGraphics: not alongside the PDF tests (it once hung there).
        let _g = PDFKIT.lock().unwrap_or_else(|e| e.into_inner());
        let png = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/vault/sources/Whiteboard photo.png");
        // Vision is a system service that has, now and then, never answered: fail rather than hang.
        let (tx, rx) = std::sync::mpsc::channel();
        let p = png.clone();
        std::thread::spawn(move || {
            let _ = tx.send(super::image_text(&p));
        });
        let text = rx.recv_timeout(std::time::Duration::from_secs(120)).expect("Vision gave no text within two minutes").unwrap();
        assert!(text.contains("Orbit App launch: 14 Oct"), "{text}");
        assert!(text.find("Orbit App") < text.find("Owner: Maya"), "top to bottom: {text}");
        assert!(super::image_text(&png.with_file_name("Roadmap Update 2026-09-18.md")).is_err());
        let d = tempfile::tempdir().unwrap();
        let jpg = d.path().join("a.jpg");
        super::image_for_model(&png, &jpg).unwrap();
        assert_eq!(&std::fs::read(&jpg).unwrap()[..2], [0xff, 0xd8], "a JPEG");
    }

    #[test]
    fn reads_the_login_item_without_changing_it() {
        // A test binary isn't an app bundle: macOS has no login item for it.
        assert_ne!(super::open_at_login(), Some(true));
    }

    /// PDFKit isn't safe to use from two test threads at once.
    static PDFKIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn drops_a_blank_last_page_only() {
        let _g = PDFKIT.lock().unwrap_or_else(|e| e.into_inner());
        let two = "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R 4 0 R]/Count 2>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]>>endobj\n4 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n";
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.pdf");
        std::fs::write(&p, two).unwrap();
        super::drop_blank_last_page(&p).unwrap();
        assert_eq!(super::pdf_text(&p).unwrap().len(), 1);
        // One page is never removed; a last page with text stays.
        super::drop_blank_last_page(&p).unwrap();
        assert_eq!(super::pdf_text(&p).unwrap().len(), 1);
        std::fs::write(&p, two).unwrap();
        super::stamp_pdf_footer(&p, "Kept").unwrap();
        super::drop_blank_last_page(&p).unwrap();
        assert_eq!(super::pdf_text(&p).unwrap().len(), 2);
    }

    #[test]
    fn stamps_the_footer_into_every_page() {
        let _g = PDFKIT.lock().unwrap_or_else(|e| e.into_inner());
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.pdf");
        std::fs::write(&p, BLANK).unwrap();
        super::stamp_pdf_footer(&p, "Meeting. Orbit App Steerco - 2026-09-30").unwrap();
        let text = unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
            let cls = AnyClass::get(c"PDFDocument").unwrap();
            let doc: Retained<AnyObject> = msg_send![msg_send![cls, alloc], initWithURL: &*url];
            let s: Retained<NSString> = msg_send![&*doc, string];
            let n: usize = msg_send![&*doc, pageCount];
            let page: Retained<AnyObject> = msg_send![&*doc, pageAtIndex: 0usize];
            let anns: Retained<AnyObject> = msg_send![&*page, annotations];
            let count: usize = msg_send![&*anns, count];
            assert_eq!((n, count), (1, 0), "one page, footer burnt in rather than left as annotations");
            // The page number sits at the right margin (it was centred on Apple silicon).
            let found: Retained<AnyObject> = msg_send![&*doc, findString: &*NSString::from_str("Page 1 of 1"), withOptions: 0usize];
            let sel: Retained<AnyObject> = msg_send![&*found, firstObject];
            let r: objc2_foundation::NSRect = msg_send![&*sel, boundsForPage: &*page];
            assert!(r.origin.x + r.size.width > 595.0 - 36.0 - 12.0, "page number not at the right: {r:?}");
            assert!(r.origin.y >= 2.0, "footer runs off the page: {r:?}");
            s.to_string()
        };
        assert!(text.contains("Page 1 of 1"), "{text}");
        assert!(text.contains("Orbit App Steerco"), "{text}");
    }
}
