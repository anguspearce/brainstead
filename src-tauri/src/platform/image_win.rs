// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Images on Windows, as macos.rs has them: the text in one (Windows' own text recognition, the
//! Snipping Tool's, in the user's languages) and the copy a model is shown (a JPEG at most 2000
//! pixels across). Both read the file through Windows' imaging, so PNG, JPEG, GIF, BMP and TIFF
//! work, and HEIC and WebP where their free extensions are installed. Nothing leaves the PC.

use std::path::Path;
use windows::core::Interface;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapEncoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode,
    ExifOrientationMode,
};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataReader, DataWriter, IRandomAccessStream, InMemoryRandomAccessStream};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows_future::IAsyncOperation;

/// The longest side of the image a model is shown, as on macOS.
const FOR_MODEL: u32 = 2000;

fn err(what: &str) -> impl Fn(windows::core::Error) -> String + '_ {
    move |e| format!("{what}: {}", e.message())
}

/// The file's bytes as a stream Windows' imaging reads, set up for this thread.
fn stream_of(path: &Path) -> Result<IRandomAccessStream, String> {
    // SAFETY: COM for this thread (a background one; already set up on the main thread).
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read the image: {e}"))?;
    let s = InMemoryRandomAccessStream::new().map_err(err("Couldn't read the image"))?;
    let w = DataWriter::CreateDataWriter(&s.GetOutputStreamAt(0).map_err(err("Couldn't read the image"))?)
        .map_err(err("Couldn't read the image"))?;
    w.WriteBytes(&bytes).map_err(err("Couldn't read the image"))?;
    w.StoreAsync().and_then(|op| op.cast::<IAsyncOperation<u32>>()).and_then(|op| op.join()).map_err(err("Couldn't read the image"))?;
    w.DetachStream().map_err(err("Couldn't read the image"))?;
    s.Seek(0).map_err(err("Couldn't read the image"))?;
    s.cast().map_err(err("Couldn't read the image"))
}

fn decoder(path: &Path) -> Result<BitmapDecoder, String> {
    let s = stream_of(path)?;
    BitmapDecoder::CreateAsync(&s).and_then(|op| op.join()).map_err(|e| format!("Windows can't open this kind of image ({})", e.message()))
}

/// Scales the image's longest side to `max`, when it's longer.
fn fit(d: &BitmapDecoder, max: u32) -> Result<(BitmapTransform, u32, u32), String> {
    let (w, h) =
        (d.OrientedPixelWidth().map_err(err("Couldn't read the image"))?, d.OrientedPixelHeight().map_err(err("Couldn't read the image"))?);
    let t = BitmapTransform::new().map_err(err("Couldn't read the image"))?;
    let longest = w.max(h).max(1);
    if longest <= max {
        return Ok((t, w, h));
    }
    let (sw, sh) = (((w as u64 * max as u64) / longest as u64).max(1) as u32, ((h as u64 * max as u64) / longest as u64).max(1) as u32);
    // The transform scales the image as stored; it's turned upright after.
    let turned = d.PixelWidth().ok() != Some(w);
    t.SetScaledWidth(if turned { sh } else { sw }).map_err(err("Couldn't scale the image"))?;
    t.SetScaledHeight(if turned { sw } else { sh }).map_err(err("Couldn't scale the image"))?;
    t.SetInterpolationMode(BitmapInterpolationMode::Fant).map_err(err("Couldn't scale the image"))?;
    Ok((t, sw, sh))
}

/// The text in an image, one line per line found, in reading order. Windows reads it in the first
/// of the user's languages it has text recognition for.
pub fn image_text(path: &Path) -> Result<String, String> {
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|_| "Windows has no text recognition for your languages: add one in Settings › Time & language › Language.".to_string())?;
    let d = decoder(path)?;
    let (t, _, _) = fit(&d, OcrEngine::MaxImageDimension().unwrap_or(10_000))?;
    let bitmap = d
        .GetSoftwareBitmapTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &t,
            ExifOrientationMode::RespectExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .and_then(|op| op.join())
        .map_err(err("Couldn't read the image"))?;
    let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()).map_err(err("Couldn't read the text in the image"))?;
    let lines = result.Lines().map_err(err("Couldn't read the text in the image"))?;
    Ok(lines.into_iter().filter_map(|l| l.Text().ok()).map(|t| t.to_string()).collect::<Vec<_>>().join("\n"))
}

/// The image as a model is shown it: a JPEG at `dest`, its longest side at most 2000 pixels,
/// turned upright.
pub fn image_for_model(path: &Path, dest: &Path) -> Result<(), String> {
    let d = decoder(path)?;
    let (t, w, h) = fit(&d, FOR_MODEL)?;
    let pixels = d
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Ignore,
            &t,
            ExifOrientationMode::RespectExifOrientation,
            ColorManagementMode::ColorManageToSRgb,
        )
        .and_then(|op| op.join())
        .and_then(|p| p.DetachPixelData())
        .map_err(err("Couldn't prepare the image"))?;
    let out = InMemoryRandomAccessStream::new().map_err(err("Couldn't prepare the image"))?;
    let enc = BitmapEncoder::JpegEncoderId()
        .and_then(|id| BitmapEncoder::CreateAsync(id, &out))
        .and_then(|op| op.join())
        .map_err(err("Couldn't prepare the image"))?;
    enc.SetPixelData(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Ignore, w, h, 96.0, 96.0, &pixels)
        .map_err(err("Couldn't prepare the image"))?;
    enc.FlushAsync().and_then(|op| op.join()).map_err(err("Couldn't prepare the image"))?;
    let size = out.Size().map_err(err("Couldn't prepare the image"))? as u32;
    let r = DataReader::CreateDataReader(&out.GetInputStreamAt(0).map_err(err("Couldn't prepare the image"))?)
        .map_err(err("Couldn't prepare the image"))?;
    r.LoadAsync(size)
        .and_then(|op| op.cast::<IAsyncOperation<u32>>())
        .and_then(|op| op.join())
        .map_err(err("Couldn't prepare the image"))?;
    let mut bytes = vec![0u8; size as usize];
    r.ReadBytes(&mut bytes).map_err(err("Couldn't prepare the image"))?;
    std::fs::write(dest, bytes).map_err(|e| {
        let _ = std::fs::remove_file(dest);
        format!("Couldn't save the prepared image: {e}")
    })
}

#[cfg(test)]
mod tests {
    /// A small PNG: 3000 x 1000, white.
    fn png(dir: &std::path::Path) -> std::path::PathBuf {
        let p = dir.join("wide.png");
        let (w, h) = (3000u32, 1000u32);
        let mut raw = Vec::new();
        for _ in 0..h {
            raw.push(0u8);
            raw.extend(std::iter::repeat_n(255u8, w as usize * 3));
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let chunk = |out: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut c = kind.to_vec();
            c.extend_from_slice(data);
            out.extend_from_slice(&c);
            out.extend_from_slice(&crc32(&c).to_be_bytes());
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &zlib_stored(&raw));
        chunk(&mut out, b"IEND", &[]);
        std::fs::write(&p, out).unwrap();
        p
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    /// zlib with stored (uncompressed) blocks.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        let chunks: Vec<&[u8]> = data.chunks(65_535).collect();
        for (i, c) in chunks.iter().enumerate() {
            out.push(if i + 1 == chunks.len() { 1 } else { 0 });
            out.extend_from_slice(&(c.len() as u16).to_le_bytes());
            out.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
            out.extend_from_slice(c);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &x in data {
            a = (a + x as u32) % 65_521;
            b = (b + a) % 65_521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }

    #[test]
    fn shrinks_a_wide_image_to_a_jpeg_for_a_model() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.jpg");
        super::image_for_model(&png(tmp.path()), &dest).unwrap();
        let d = super::decoder(&dest).unwrap();
        assert_eq!((d.PixelWidth().unwrap(), d.PixelHeight().unwrap()), (2000, 666));
        assert_eq!(&std::fs::read(&dest).unwrap()[..2], b"\xff\xd8");
    }

    #[test]
    fn a_blank_image_has_no_text_and_a_non_image_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        match super::image_text(&png(tmp.path())) {
            Ok(t) => assert!(t.trim().is_empty(), "{t}"),
            // A PC with no text recognition language says so.
            Err(e) => assert!(e.contains("text recognition"), "{e}"),
        }
        let txt = tmp.path().join("x.png");
        std::fs::write(&txt, "not an image").unwrap();
        assert!(super::image_for_model(&txt, &tmp.path().join("y.jpg")).is_err());
    }
}
