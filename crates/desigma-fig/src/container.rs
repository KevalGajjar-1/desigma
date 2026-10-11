//! `.fig` container: optional ZIP wrapper, `fig-kiwi` header and chunked,
//! compressed schema + message.

use crate::FigError;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

/// Hard cap on any single decompressed chunk / zip entry (zip-bomb guard).
const MAX_INFLATED: u64 = 2 << 30;

pub const MAGIC_KIWI: &[u8; 8] = b"fig-kiwi";
pub const MAGIC_JAM: &[u8; 8] = b"fig-jam.";

#[derive(Debug, Default)]
pub struct Archive {
    /// Raw `canvas.fig` bytes (the fig-kiwi container).
    pub canvas: Vec<u8>,
    /// `images/<hash>` entries keyed by file name.
    pub images: BTreeMap<String, Vec<u8>>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Compression {
    Deflate,
    Zstd,
}

/// Opens either a ZIP-wrapped `.fig` or a bare fig-kiwi container.
pub fn open(bytes: &[u8]) -> Result<Archive, FigError> {
    if bytes.starts_with(b"PK") {
        return open_zip(bytes);
    }
    if bytes.starts_with(MAGIC_KIWI) || bytes.starts_with(MAGIC_JAM) {
        return Ok(Archive { canvas: bytes.to_vec(), ..Default::default() });
    }
    Err(FigError::Format("missing fig-kiwi magic or zip header".into()))
}

fn read_limited(r: impl Read) -> Result<Vec<u8>, FigError> {
    let mut out = Vec::new();
    r.take(MAX_INFLATED + 1).read_to_end(&mut out).map_err(|e| FigError::Format(format!("read error: {e}")))?;
    if out.len() as u64 > MAX_INFLATED {
        return Err(FigError::Format("decompressed data too large".into()));
    }
    Ok(out)
}

fn open_zip(bytes: &[u8]) -> Result<Archive, FigError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| FigError::Format(format!("zip: {e}")))?;
    let mut arch = Archive::default();
    let mut have_canvas = false;
    for i in 0..zip.len() {
        let Ok(file) = zip.by_index(i) else { continue };
        if file.is_dir() {
            continue;
        }
        let Ok(name) = file.name().map(|n| n.replace('\\', "/")) else { continue };
        if name == "canvas.fig" || name.ends_with("/canvas.fig") {
            arch.canvas = read_limited(file)?;
            have_canvas = true;
        } else if name == "meta.json" {
            if let Ok(data) = read_limited(file) {
                arch.meta = serde_json::from_slice(&data).ok();
            }
        } else if let Some(h) = name.strip_prefix("images/") {
            if !h.is_empty() && !h.contains('/') {
                if let Ok(data) = read_limited(file) {
                    arch.images.insert(h.to_string(), data);
                }
            }
        }
    }
    if !have_canvas {
        return Err(FigError::Format("zip has no canvas.fig".into()));
    }
    Ok(arch)
}

/// Splits a fig-kiwi container into decompressed `(schema, message)` chunks.
pub fn read_chunks(canvas: &[u8]) -> Result<(Vec<u8>, Vec<u8>), FigError> {
    if canvas.len() < 12 || !(canvas.starts_with(MAGIC_KIWI) || canvas.starts_with(MAGIC_JAM)) {
        return Err(FigError::Format("not a fig-kiwi container".into()));
    }
    let mut pos = 12; // magic + u32 version
    let mut chunks: Vec<&[u8]> = Vec::new();
    while pos + 4 <= canvas.len() && chunks.len() < 8 {
        let len = u32::from_le_bytes([canvas[pos], canvas[pos + 1], canvas[pos + 2], canvas[pos + 3]]) as usize;
        pos += 4;
        let end = pos.checked_add(len).filter(|&e| e <= canvas.len());
        let Some(end) = end else {
            return Err(FigError::Format(format!("chunk {} is truncated", chunks.len())));
        };
        chunks.push(&canvas[pos..end]);
        pos = end;
    }
    if chunks.len() < 2 {
        return Err(FigError::Format("container has fewer than two chunks".into()));
    }
    Ok((decompress(chunks[0])?, decompress(chunks[1])?))
}

pub fn container_version(canvas: &[u8]) -> Option<u32> {
    let b = canvas.get(8..12)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Detects zstd by magic, otherwise raw DEFLATE (falling back to zlib wrapping).
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, FigError> {
    if data.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        let dec = ruzstd::decoding::StreamingDecoder::new(data).map_err(|e| FigError::Format(format!("zstd: {e}")))?;
        return read_limited(dec);
    }
    match read_limited(flate2::read::DeflateDecoder::new(data)) {
        Ok(v) => Ok(v),
        Err(e) => {
            // Some producers use zlib framing.
            read_limited(flate2::read::ZlibDecoder::new(data)).map_err(|_| e)
        }
    }
}

pub fn compress(data: &[u8], how: Compression) -> Vec<u8> {
    use std::io::Write;
    match how {
        Compression::Deflate => {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            let _ = e.write_all(data);
            e.finish().unwrap_or_default()
        }
        Compression::Zstd => ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest),
    }
}

/// Builds a fig-kiwi container (header, version, schema chunk, message chunk).
pub fn write_container(schema: &[u8], message: &[u8], how: Compression, version: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC_KIWI);
    out.extend_from_slice(&version.to_le_bytes());
    for chunk in [schema, message] {
        let c = compress(chunk, how);
        out.extend_from_slice(&(c.len() as u32).to_le_bytes());
        out.extend_from_slice(&c);
    }
    out
}

/// Wraps a canvas and images into the ZIP layout Figma uses for "Save local copy".
pub fn write_zip(canvas: &[u8], images: &BTreeMap<String, Vec<u8>>, meta: Option<&serde_json::Value>) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let _ = w.start_file("canvas.fig", opts);
    let _ = w.write_all(canvas);
    if let Some(m) = meta {
        let _ = w.start_file("meta.json", opts);
        let _ = w.write_all(m.to_string().as_bytes());
    }
    for (k, v) in images {
        let _ = w.start_file(format!("images/{k}"), opts);
        let _ = w.write_all(v);
    }
    w.finish().map(|c| c.into_inner()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_roundtrips() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        for how in [Compression::Deflate, Compression::Zstd] {
            assert_eq!(decompress(&compress(&data, how)).unwrap(), data, "{how:?}");
        }
    }

    #[test]
    fn container_roundtrip_and_zip() {
        let c = write_container(b"schema", b"message", Compression::Zstd, 15);
        assert_eq!(container_version(&c), Some(15));
        let (s, m) = read_chunks(&c).unwrap();
        assert_eq!((s.as_slice(), m.as_slice()), (&b"schema"[..], &b"message"[..]));
        let mut imgs = BTreeMap::new();
        imgs.insert("abcd".to_string(), vec![1, 2, 3]);
        let z = write_zip(&c, &imgs, Some(&serde_json::json!({"file_name": "x"})));
        let a = open(&z).unwrap();
        assert_eq!(a.canvas, c);
        assert_eq!(a.images["abcd"], vec![1, 2, 3]);
        assert_eq!(a.meta.unwrap()["file_name"], "x");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(open(b"").is_err());
        assert!(open(b"PK\x03\x04junk").is_err());
        let c = write_container(b"s", b"m", Compression::Deflate, 1);
        for n in 0..c.len() {
            let _ = read_chunks(&c[..n]);
        }
        assert!(decompress(&[0x28, 0xB5, 0x2F, 0xFD, 1, 2, 3]).is_err());
    }
}
