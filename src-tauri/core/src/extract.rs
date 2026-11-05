// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text from the sources that aren't markdown: PDFs and images (by the readers the app passes in,
//! PDFKit and Vision's text recognition on macOS, so this crate needs no platform code), Word
//! documents, PowerPoint decks and Excel workbooks. Ported from the previous app's `source-text.ts` and `ooxml.ts`, rules included:
//! hidden sheets and hidden (filtered-out) rows are left out, struck-through rows are marked
//! `[struck]`, date-formatted cells read as dates, and archives over 64 MB or text over 2M
//! characters are refused or cut. The text is kept in the app data folder by the file's content
//! hash (`text/<sha>.json`), never in the vault.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
/// What an Office file's parts may come to unzipped, all told: a small file can unzip to gigabytes.
const MAX_UNZIPPED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 2_000_000;
const TRUNCATED: &str = "[extraction truncated]";
const BUILTIN_DATE_FMTS: [i64; 12] = [14, 15, 16, 17, 18, 19, 20, 21, 22, 45, 46, 47];

/// Reads a PDF's text, one string per page.
pub type PdfReader = fn(&Path) -> Result<Vec<String>, String>;
/// Reads the text in an image (OCR), one line per line of text it found.
pub type ImageReader = fn(&Path) -> Result<String, String>;

/// The image formats read for their text: those the app shows (`IMAGE_EXT` in src/md/wikilinks.ts)
/// but SVG, which is drawn, not photographed.
pub const IMAGE_EXTS: [&str; 8] = ["png", "jpg", "jpeg", "heic", "webp", "gif", "bmp", "avif"];
/// Images bigger than this aren't read.
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// One part of a source: a page, a slide, a sheet, or the whole document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    /// "Page 6", "Slide 3", the sheet's name, or "" for a Word document.
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extracted {
    /// pdf, docx, pptx, xlsx or image.
    pub kind: String,
    pub parts: Vec<Part>,
}

impl Extracted {
    /// All of it, parts separated by blank lines.
    pub fn text(&self) -> String {
        self.parts.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n\n")
    }
    /// One page (from 1), for a `page=N` anchor.
    pub fn page(&self, n: usize) -> Option<&str> {
        let want = format!("Page {n}");
        self.parts.iter().find(|p| p.label == want).map(|p| p.text.as_str())
    }
}

/// The file kinds this reads, by extension.
pub fn kind_of(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "pdf" => Some("pdf"),
        "docx" => Some("docx"),
        "pptx" => Some("pptx"),
        "xlsx" => Some("xlsx"),
        e if IMAGE_EXTS.contains(&e) => Some("image"),
        _ => None,
    }
}

/// Whether ingest can read a source: markdown and text, or a kind this reads. The Sources screen's
/// `ingestable` (src/Lists.tsx) is the same rule, for the window; the MCP server's pending_sources
/// leaves the rest out with this.
pub fn ingestable(rel: &str) -> bool {
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    matches!(ext.as_deref(), Some("md" | "txt")) || kind_of(rel).is_some()
}

/// Reads a file's text. PDFs need `pdf` and images `image`; without one they're an error.
pub fn extract(path: &Path, pdf: Option<PdfReader>, image: Option<ImageReader>) -> Result<Extracted, String> {
    let rel = path.to_string_lossy();
    let kind = kind_of(&rel).ok_or("Not a PDF, Office file or image.")?;
    let parts = match kind {
        "image" => {
            let read = image.ok_or("Reading the text in images isn't available on this system.")?;
            if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_IMAGE_BYTES {
                return Err("Too big to read (over 64 MB).".into());
            }
            let text = tidy(&read(path)?);
            if text.is_empty() {
                vec![]
            } else {
                vec![Part { label: String::new(), text }]
            }
        }
        "pdf" => {
            let read = pdf.ok_or("PDF text isn't available on this system.")?;
            read(path)?
                .into_iter()
                .enumerate()
                .map(|(i, t)| Part { label: format!("Page {}", i + 1), text: tidy(&t) })
                .filter(|p| !p.text.is_empty())
                .collect()
        }
        _ => {
            let mut zip = open_archive(path)?;
            let parts = match kind {
                "docx" => docx(&mut zip)?,
                "pptx" => pptx(&mut zip)?,
                _ => xlsx(&mut zip)?,
            };
            if zip.over {
                return Err("Too big to read (over 256 MB unzipped); its PDF export reads better.".into());
            }
            parts
        }
    };
    Ok(Extracted { kind: kind.into(), parts: cap(parts) })
}

/// Collapses runs of spaces and blank lines that PDF text is full of.
fn tidy(t: &str) -> String {
    let lines: Vec<String> = t.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    let mut out = String::new();
    let mut blank = false;
    for l in lines {
        if l.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank { "\n\n" } else { "\n" });
        }
        blank = false;
        out.push_str(&l);
    }
    out
}

fn cap(parts: Vec<Part>) -> Vec<Part> {
    let mut total = 0;
    let mut out = Vec::new();
    for mut p in parts {
        let n = p.text.chars().count();
        if total + n > MAX_TEXT_CHARS {
            let keep = MAX_TEXT_CHARS.saturating_sub(total);
            p.text = format!("{}\n{TRUNCATED}", p.text.chars().take(keep).collect::<String>());
            out.push(p);
            break;
        }
        total += n;
        out.push(p);
    }
    out
}

/// An Office file's archive, with what's left of `MAX_UNZIPPED_BYTES` to read from it.
struct Zip {
    archive: zip::ZipArchive<std::fs::File>,
    left: u64,
    /// A part didn't fit in what was left: the text is incomplete.
    over: bool,
}

impl std::ops::Deref for Zip {
    type Target = zip::ZipArchive<std::fs::File>;
    fn deref(&self) -> &Self::Target {
        &self.archive
    }
}

impl std::ops::DerefMut for Zip {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.archive
    }
}

fn open_archive(path: &Path) -> Result<Zip, String> {
    let md = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if md.len() > MAX_ARCHIVE_BYTES {
        return Err("Too big to read (over 64 MB); its PDF export reads better.".into());
    }
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let archive = zip::ZipArchive::new(f).map_err(|e| format!("Not a readable Office file: {e}"))?;
    Ok(Zip { archive, left: MAX_UNZIPPED_BYTES, over: false })
}

/// One part of the archive as text; "" when it isn't there, or when reading it would go over
/// what's left to unzip (which marks the archive `over`).
fn part(zip: &mut Zip, name: &str) -> String {
    if zip.over {
        return String::new();
    }
    let left = zip.left;
    let Ok(f) = zip.archive.by_name(name) else { return String::new() };
    let mut bytes = Vec::new();
    // One byte more than is left, to tell "fits exactly" from "too big".
    let _ = f.take(left + 1).read_to_end(&mut bytes);
    if bytes.len() as u64 > left {
        zip.over = true;
        return String::new();
    }
    zip.left -= bytes.len() as u64;
    String::from_utf8(bytes).unwrap_or_default()
}

static ENTITY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);").unwrap());
static ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"([\w:.-]+)\s*=\s*"([^"]*)""#).unwrap());

fn decode(t: &str) -> String {
    ENTITY
        .replace_all(t, |c: &regex::Captures| {
            let ent = &c[1];
            if let Some(n) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                return u32::from_str_radix(n, 16).ok().and_then(char::from_u32).map_or(c[0].to_string(), |ch| ch.to_string());
            }
            if let Some(n) = ent.strip_prefix('#') {
                return n.parse::<u32>().ok().and_then(char::from_u32).map_or(c[0].to_string(), |ch| ch.to_string());
            }
            match ent {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" => "'",
                _ => return c[0].to_string(),
            }
            .to_string()
        })
        .into_owned()
}

fn attrs(tag: &str) -> HashMap<String, String> {
    ATTR.captures_iter(tag).map(|c| (c[1].to_string(), decode(&c[2]))).collect()
}

fn element(local: &str) -> Regex {
    // The name must end there (`<a:p>`, not `<a:pPr>` or `<p:sld>`): attributes start with a space.
    Regex::new(&format!(r"(?s)<(?:[\w.-]+:)?{local}((?:\s[^>]*?)?)(?:/>|>(.*?)</(?:[\w.-]+:)?{local}>)")).unwrap()
}

/// Every `<local>`'s attributes and inner XML, self-closing ones with "".
fn blocks(xml: &str, local: &str) -> Vec<(String, String)> {
    element(local).captures_iter(xml).map(|c| (c[1].to_string(), c.get(2).map_or(String::new(), |m| m.as_str().to_string()))).collect()
}

/// The text of every `<local>` in order, joined.
fn text_of(xml: &str, local: &str) -> String {
    blocks(xml, local).iter().map(|(_, inner)| decode(inner)).collect()
}

fn resolve(base: &str, target: &str) -> String {
    if let Some(t) = target.strip_prefix('/') {
        return t.to_string();
    }
    let mut out: Vec<&str> = Vec::new();
    for seg in base.split('/').chain(target.split('/')) {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// `rIdN` → (target part, type).
fn rels(xml: &str, base: &str) -> HashMap<String, (String, String)> {
    blocks(xml, "Relationship")
        .into_iter()
        .filter_map(|(a, _)| {
            let a = attrs(&a);
            let target = a.get("Target")?;
            if a.get("TargetMode").is_some_and(|m| m == "External") {
                return None;
            }
            Some((a.get("Id")?.clone(), (resolve(base, target), a.get("Type").cloned().unwrap_or_default())))
        })
        .collect()
}

fn docx(zip: &mut Zip) -> Result<Vec<Part>, String> {
    let xml = part(zip, "word/document.xml");
    if xml.is_empty() {
        return Err("No document text in it.".into());
    }
    let paras: Vec<String> = blocks(&xml, "p").iter().map(|(_, p)| text_of(p, "t").trim().to_string()).filter(|t| !t.is_empty()).collect();
    Ok(vec![Part { label: String::new(), text: paras.join("\n") }])
}

fn paragraphs(xml: &str) -> Vec<String> {
    blocks(xml, "p").iter().map(|(_, p)| text_of(p, "t").trim().to_string()).filter(|t| !t.is_empty()).collect()
}

fn pptx(zip: &mut Zip) -> Result<Vec<Part>, String> {
    let pres = part(zip, "ppt/presentation.xml");
    let r = rels(&part(zip, "ppt/_rels/presentation.xml.rels"), "ppt");
    let ids = blocks(&pres, "sldIdLst").into_iter().next().map(|(_, b)| b).unwrap_or_default();
    let mut slides: Vec<String> = blocks(&ids, "sldId")
        .iter()
        .filter_map(|(a, _)| r.get(attrs(a).get("r:id")?).map(|(t, _)| t.clone()))
        .filter(|t| zip.by_name(t).is_ok())
        .collect();
    if slides.is_empty() {
        let num = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>().parse::<u64>().unwrap_or(0);
        slides = zip
            .file_names()
            .filter_map(Result::ok)
            .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml") && !n[16..].contains('/'))
            .map(|n| n.into_owned())
            .collect();
        slides.sort_by_key(|s| num(s));
    }
    let mut out = Vec::new();
    for (i, s) in slides.iter().enumerate() {
        let body = paragraphs(&part(zip, s));
        let (dir, file) = s.rsplit_once('/').unwrap_or(("", s));
        let srels = rels(&part(zip, &format!("{dir}/_rels/{file}.rels")), dir);
        let notes_part = srels.values().find(|(_, t)| t.ends_with("/relationships/notesSlide")).map(|(p, _)| p.clone());
        let notes = notes_part.map(|p| paragraphs(&part(zip, &p))).unwrap_or_default();
        if body.is_empty() && notes.is_empty() {
            continue;
        }
        let mut lines = body;
        if !notes.is_empty() {
            lines.push("Speaker notes:".into());
            lines.extend(notes);
        }
        out.push(Part { label: format!("Slide {}", i + 1), text: lines.join("\n") });
    }
    Ok(out)
}

struct Styles {
    strike: Vec<bool>,
    date: Vec<bool>,
}

fn is_date_format(code: &str) -> bool {
    let a = Regex::new(r#""[^"]*""#).unwrap().replace_all(code, "");
    let b = Regex::new(r"\[[^\]]*\]").unwrap().replace_all(&a, "");
    let c = Regex::new(r"\\.").unwrap().replace_all(&b, "");
    c.chars().any(|ch| matches!(ch.to_ascii_lowercase(), 'y' | 'm' | 'd' | 'h' | 's'))
}

fn styles(xml: &str) -> Styles {
    let strike_re = Regex::new(r"<(?:[\w.-]+:)?strike[\s/>]").unwrap();
    let font_strike: Vec<bool> = blocks(xml, "font").iter().map(|(_, f)| strike_re.is_match(f)).collect();
    let custom: HashMap<i64, bool> = blocks(xml, "numFmt")
        .iter()
        .filter_map(|(a, _)| {
            let a = attrs(a);
            Some((a.get("numFmtId")?.parse().ok()?, is_date_format(a.get("formatCode")?)))
        })
        .collect();
    let xfs = blocks(xml, "cellXfs").into_iter().next().map(|(_, b)| b).unwrap_or_default();
    let mut strike = Vec::new();
    let mut date = Vec::new();
    for (a, _) in blocks(&xfs, "xf") {
        let a = attrs(&a);
        let font: Option<usize> = a.get("fontId").and_then(|v| v.parse().ok());
        strike.push(font.and_then(|f| font_strike.get(f).copied()).unwrap_or(false));
        let fmt: Option<i64> = a.get("numFmtId").and_then(|v| v.parse().ok());
        date.push(fmt.is_some_and(|f| custom.get(&f).copied().unwrap_or(BUILTIN_DATE_FMTS.contains(&f))));
    }
    Styles { strike, date }
}

/// An Excel serial day as a date (and time when it has one).
/// None when it's no date chrono can hold (a number formatted as a date, out of range).
fn serial_date(serial: f64, epoch_1904: bool) -> Option<String> {
    let days = if epoch_1904 { serial + 1462.0 } else { serial };
    let ms = days * 86_400_000.0;
    if !ms.is_finite() || ms.abs() >= i64::MAX as f64 {
        return None;
    }
    let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    let t = base.checked_add_signed(chrono::Duration::try_milliseconds(ms.round() as i64)?)?;
    Some(if days.fract() == 0.0 { t.format("%Y-%m-%d").to_string() } else { t.format("%Y-%m-%d %H:%M").to_string() })
}

fn xlsx(zip: &mut Zip) -> Result<Vec<Part>, String> {
    let wb = part(zip, "xl/workbook.xml");
    if wb.is_empty() {
        return Err("No workbook in it.".into());
    }
    let r = rels(&part(zip, "xl/_rels/workbook.xml.rels"), "xl");
    let epoch_1904 = Regex::new(r#"<(?:[\w.-]+:)?workbookPr\b[^>]*\bdate1904\s*=\s*"(?:1|true)""#).unwrap().is_match(&wb);
    let shared: Vec<String> = blocks(&part(zip, "xl/sharedStrings.xml"), "si").iter().map(|(_, si)| text_of(si, "t")).collect();
    let st = styles(&part(zip, "xl/styles.xml"));
    let sheets_block = blocks(&wb, "sheets").into_iter().next().map(|(_, b)| b).unwrap_or_else(|| wb.clone());
    let mut out = Vec::new();
    for (a, _) in blocks(&sheets_block, "sheet") {
        let a = attrs(&a);
        if matches!(a.get("state").map(String::as_str), Some("hidden" | "veryHidden")) {
            continue;
        }
        let Some((path, _)) = a.get("r:id").or_else(|| a.get("id")).and_then(|id| r.get(id)) else { continue };
        let xml = part(zip, path);
        let mut rows = Vec::new();
        for (ra, body) in blocks(&xml, "row") {
            let ra = attrs(&ra);
            if matches!(ra.get("hidden").map(String::as_str), Some("1" | "true")) || body.is_empty() {
                continue;
            }
            let mut values = Vec::new();
            let mut struck = false;
            for (ca, cx) in blocks(&body, "c") {
                let ca = attrs(&ca);
                let style: Option<usize> = ca.get("s").and_then(|v| v.parse().ok());
                let raw = text_of(&cx, "v");
                let v = match ca.get("t").map(String::as_str).unwrap_or("n") {
                    "inlineStr" => text_of(&cx, "t").trim().to_string(),
                    "e" => String::new(),
                    "s" => raw.trim().parse::<usize>().ok().and_then(|i| shared.get(i)).map(|s| s.trim().to_string()).unwrap_or_default(),
                    "str" => raw.trim().to_string(),
                    "b" => if raw == "1" { "TRUE" } else { "FALSE" }.to_string(),
                    _ => match (raw.trim().parse::<f64>(), style) {
                        (Ok(n), Some(s)) if st.date.get(s).copied().unwrap_or(false) => {
                            serial_date(n, epoch_1904).unwrap_or_else(|| raw.trim().to_string())
                        }
                        _ => raw.trim().to_string(),
                    },
                };
                if v.is_empty() {
                    continue;
                }
                if style.and_then(|s| st.strike.get(s).copied()).unwrap_or(false) {
                    struck = true;
                }
                values.push(v);
            }
            if values.is_empty() {
                continue;
            }
            let line = values.join("\t");
            rows.push(if struck { format!("{line}\t[struck]") } else { line });
        }
        if !rows.is_empty() {
            out.push(Part { label: a.get("name").cloned().unwrap_or_else(|| "Sheet".into()), text: rows.join("\n") });
        }
    }
    Ok(out)
}

/// Extracted text kept by content hash in the app data folder.
#[derive(Clone)]
pub struct TextCache {
    pub dir: PathBuf,
    pub pdf: Option<PdfReader>,
    pub image: Option<ImageReader>,
}

impl TextCache {
    pub fn new(dir: PathBuf, pdf: Option<PdfReader>) -> Self {
        TextCache { dir, pdf, image: None }
    }

    /// Reads images' text with `image` too.
    pub fn with_image(mut self, image: Option<ImageReader>) -> Self {
        self.image = image;
        self
    }

    fn file(&self, sha: &str) -> PathBuf {
        self.dir.join(format!("{sha}.json"))
    }

    /// What's kept for this content, without reading the file (the MCP server's way).
    pub fn lookup(dir: &Path, sha: &str) -> Option<Extracted> {
        std::fs::read(dir.join(format!("{sha}.json"))).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    /// The file's text: kept, or read now and kept. A file that can't be read is kept as having
    /// no parts, so it isn't tried again until it changes. One there's no reader for here (an
    /// image, without `with_image`) isn't kept, so a cache that has the reader still reads it.
    pub fn get(&self, path: &Path, sha: &str) -> Extracted {
        if let Some(x) = Self::lookup(&self.dir, sha) {
            return x;
        }
        let kind = kind_of(&path.to_string_lossy()).unwrap_or("").to_string();
        let unread = Extracted { kind: kind.clone(), parts: vec![] };
        if (kind == "image" && self.image.is_none()) || (kind == "pdf" && self.pdf.is_none()) {
            return unread;
        }
        let x = extract(path, self.pdf, self.image).unwrap_or(unread);
        if std::fs::create_dir_all(&self.dir).is_ok() {
            if let Ok(j) = serde_json::to_vec(&x) {
                let _ = crate::write::write_atomic(&self.file(sha), &j, false);
            }
        }
        x
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ingestable_as_the_sources_screen_has_it() {
        for p in ["sources/a.md", "sources/b.TXT", "sources/c.pdf", "sources/d.docx", "sources/e.png", "sources/f.HEIC"] {
            assert!(ingestable(p), "{p}");
        }
        for p in ["sources/g.svg", "sources/h.zip", "sources/i.vtt", "sources/noext"] {
            assert!(!ingestable(p), "{p}");
        }
    }

    use super::*;
    use std::io::Write;

    fn make_zip(path: &Path, files: &[(&str, &str)]) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        for (name, body) in files {
            z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn word_paragraphs() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Plan.docx");
        make_zip(
            &p,
            &[(
                "word/document.xml",
                r#"<w:document><w:body><w:p><w:r><w:t>Launch &amp; comms</w:t></w:r></w:p><w:p/><w:p><w:r><w:t xml:space="preserve">Owner: </w:t></w:r><w:r><w:t>Maya</w:t></w:r></w:p></w:body></w:document>"#,
            )],
        );
        let x = extract(&p, None, None).unwrap();
        assert_eq!(x.kind, "docx");
        assert_eq!(x.parts[0].text, "Launch & comms\nOwner: Maya");
    }

    #[test]
    fn deck_slides_in_order_with_notes() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Deck.pptx");
        make_zip(
            &p,
            &[
                (
                    "ppt/presentation.xml",
                    r#"<p:presentation><p:sldIdLst><p:sldId id="1" r:id="rId2"/><p:sldId id="2" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
                ),
                (
                    "ppt/_rels/presentation.xml.rels",
                    r#"<Relationships><Relationship Id="rId1" Type="x/slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="x/slide" Target="slides/slide2.xml"/></Relationships>"#,
                ),
                ("ppt/slides/slide1.xml", r#"<p:sld><a:p><a:r><a:t>Second</a:t></a:r></a:p></p:sld>"#),
                ("ppt/slides/slide2.xml", r#"<p:sld><a:p><a:r><a:t>Fir</a:t></a:r><a:r><a:t>st</a:t></a:r></a:p></p:sld>"#),
                (
                    "ppt/slides/_rels/slide2.xml.rels",
                    r#"<Relationships><Relationship Id="rId9" Type="http://x/relationships/notesSlide" Target="../notesSlides/notesSlide7.xml"/></Relationships>"#,
                ),
                ("ppt/notesSlides/notesSlide7.xml", r#"<p:notes><a:p><a:r><a:t>Say hello</a:t></a:r></a:p></p:notes>"#),
            ],
        );
        let x = extract(&p, None, None).unwrap();
        assert_eq!(
            x.parts,
            [
                Part { label: "Slide 1".into(), text: "First\nSpeaker notes:\nSay hello".into() },
                Part { label: "Slide 2".into(), text: "Second".into() },
            ]
        );
    }

    #[test]
    fn workbook_rules() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Roster.xlsx");
        make_zip(
            &p,
            &[
                (
                    "xl/workbook.xml",
                    r#"<workbook><sheets><sheet name="Team" sheetId="1" r:id="rId1"/><sheet name="Old" sheetId="2" state="hidden" r:id="rId2"/></sheets></workbook>"#,
                ),
                (
                    "xl/_rels/workbook.xml.rels",
                    r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#,
                ),
                ("xl/sharedStrings.xml", r#"<sst><si><t>Maya</t></si><si><t>Lena</t></si><si><t>Theo</t></si></sst>"#),
                (
                    "xl/styles.xml",
                    r#"<styleSheet><fonts><font><sz/></font><font><strike/></font></fonts><cellXfs count="3"><xf fontId="0" numFmtId="0"/><xf fontId="1" numFmtId="0"/><xf fontId="0" numFmtId="14"/></cellXfs></styleSheet>"#,
                ),
                (
                    "xl/worksheets/sheet1.xml",
                    r#"<worksheet><sheetData><row r="1"><c t="s"><v>0</v></c><c s="2"><v>45292</v></c></row><row r="2"><c t="s" s="1"><v>1</v></c></row><row r="3" hidden="1"><c t="s"><v>2</v></c></row></sheetData></worksheet>"#,
                ),
                (
                    "xl/worksheets/sheet2.xml",
                    r#"<worksheet><sheetData><row><c t="inlineStr"><is><t>secret</t></is></c></row></sheetData></worksheet>"#,
                ),
            ],
        );
        let x = extract(&p, None, None).unwrap();
        assert_eq!(x.parts, [Part { label: "Team".into(), text: "Maya\t2024-01-01\nLena\t[struck]".into() }]);
    }

    #[test]
    fn pdf_pages_and_the_cache() {
        fn reader(_: &Path) -> Result<Vec<String>, String> {
            Ok(vec!["Programme   update\n\n\nStaff launch moves".into(), "".into(), "Page three".into()])
        }
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Steerco.pdf");
        std::fs::write(&p, b"%PDF").unwrap();
        assert!(extract(&p, None, None).is_err());
        let c = TextCache::new(t.path().join("text"), Some(reader));
        let x = c.get(&p, "abc");
        assert_eq!(x.parts[0], Part { label: "Page 1".into(), text: "Programme update\n\nStaff launch moves".into() });
        assert_eq!(x.page(3), Some("Page three"));
        assert_eq!(x.page(2), None);
        assert_eq!(TextCache::lookup(&t.path().join("text"), "abc"), Some(x));
    }

    #[test]
    fn images_by_their_text_and_the_cache() {
        fn reader(_: &Path) -> Result<String, String> {
            Ok("Orbit App   launch:\n\n\n14 Oct".into())
        }
        fn blank(_: &Path) -> Result<String, String> {
            Ok("  \n".into())
        }
        assert_eq!(kind_of("sources/Whiteboard.HEIC"), Some("image"));
        assert_eq!(kind_of("sources/Board.jpeg"), Some("image"));
        assert_eq!(kind_of("Attachments/Diagram.svg"), None);
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Whiteboard.png");
        std::fs::write(&p, b"png").unwrap();
        assert!(extract(&p, None, None).is_err());
        let x = extract(&p, None, Some(reader)).unwrap();
        assert_eq!(
            x,
            Extracted { kind: "image".into(), parts: vec![Part { label: String::new(), text: "Orbit App launch:\n\n14 Oct".into() }] }
        );
        assert!(extract(&p, None, Some(blank)).unwrap().parts.is_empty());
        // Without a reader nothing is kept, so a cache that has one still reads it.
        let dir = t.path().join("text");
        assert!(TextCache::new(dir.clone(), None).get(&p, "img").parts.is_empty());
        assert_eq!(TextCache::lookup(&dir, "img"), None);
        let c = TextCache::new(dir.clone(), None).with_image(Some(reader));
        assert_eq!(c.get(&p, "img"), x);
        assert_eq!(TextCache::lookup(&dir, "img"), Some(x));
    }

    #[test]
    fn date_formats() {
        assert!(is_date_format("dd/mm/yyyy"));
        assert!(!is_date_format(r#"0.00"m""#));
        assert_eq!(serial_date(45292.5, false).as_deref(), Some("2024-01-01 12:00"));
        // Out of chrono's range, or not a number at all: no date, and no panic.
        for n in [1e300, -1e300, 1e15, f64::NAN, f64::INFINITY] {
            assert_eq!(serial_date(n, false), None);
        }
    }

    #[test]
    fn unzipped_size_is_capped() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Big.docx");
        let body = format!("<w:document><w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>", "a".repeat(4096));
        make_zip(&p, &[("word/document.xml", &body)]);
        let mut zip = open_archive(&p).unwrap();
        zip.left = 1000;
        assert!(docx(&mut zip).is_err() || zip.over);
        assert!(zip.over);
        let mut zip = open_archive(&p).unwrap();
        zip.left = body.len() as u64;
        assert_eq!(docx(&mut zip).unwrap()[0].text.len(), 4096);
        assert!(!zip.over);
    }
}
