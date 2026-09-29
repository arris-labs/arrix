//! The `.arrx` zip (docs/DATA-MODEL.md §File format): a document's
//! directory form in one file. The writer is deterministic: entries in
//! sorted path order, a fixed timestamp, no extra fields, no directory
//! entries, JSON deflated and blobs stored. The reader takes what the
//! writer gives, and what an ordinary zip tool makes of the same tree.
//! Written here rather than taken from a crate: the format's subset is
//! small, and pure-Rust deflate and CRC-32 build for wasm32 without C and
//! give the same bytes on every target.

use std::collections::BTreeMap;

use crate::document::Document;
use crate::open::{DOCUMENT_JSON, MemorySource, OpenError, PARAMS_JSON, open};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const UTF8: u16 = 0x0800;
const ENCRYPTED: u16 = 0x0001;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// 1980-01-01 00:00:00, the zip epoch.
const DATE: u16 = 0x0021;
/// A single entry may not inflate past this, whatever it declares.
const MAX_ENTRY: u32 = 1 << 30;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ZipError {
    #[error("not a zip file: {0}")]
    Malformed(&'static str),
    #[error("the zip holds {0:?} twice")]
    Duplicate(String),
    #[error(
        "{0:?} is not part of a document: it is not `document.json`, `params.json`, or under `parts/`, `plugins/` or `blobs/`"
    )]
    OutsideTree(String),
    #[error("{0:?}: a compression method other than stored and deflate, or encryption")]
    Unsupported(String),
    #[error("{0:?} does not inflate to what the zip declares, or fails its checksum")]
    Corrupt(String),
    #[error("a document this large is not written (no zip64): {0}")]
    TooLarge(&'static str),
}

/// Whether `path` may be an entry of an `.arrx`: a file of the document
/// tree, `/`-separated, with no empty, `.` or `..` component.
fn in_tree(path: &str) -> bool {
    let clean = !path.is_empty()
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains('\\'));
    let top = path.split('/').next().unwrap_or_default();
    clean
        && match top {
            DOCUMENT_JSON | PARAMS_JSON => !path.contains('/'),
            "parts" | "plugins" | "blobs" => path.contains('/'),
            _ => false,
        }
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// `files` as an `.arrx`. The same files give the same bytes.
pub fn to_zip(files: &MemorySource) -> Result<Vec<u8>, ZipError> {
    let too_large = ZipError::TooLarge;
    if files.0.len() > usize::from(u16::MAX) {
        return Err(too_large("more than 65535 files"));
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (path, bytes) in &files.0 {
        assert!(in_tree(path), "{path:?} is not a document path");
        let (method, data) = if path.starts_with("blobs/") {
            (STORED, bytes.clone())
        } else {
            (DEFLATED, miniz_oxide::deflate::compress_to_vec(bytes, 6))
        };
        let crc = crc32fast::hash(bytes);
        let size = u32::try_from(bytes.len()).map_err(|_| too_large("a file over 4 GiB"))?;
        let csize = u32::try_from(data.len()).map_err(|_| too_large("a file over 4 GiB"))?;
        let offset = u32::try_from(out.len()).map_err(|_| too_large("over 4 GiB"))?;
        let needed = if method == STORED { 10 } else { 20 };
        let name = path.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| too_large("a path over 64 KiB"))?;

        put32(&mut out, LOCAL);
        put16(&mut out, needed);
        put16(&mut out, UTF8);
        put16(&mut out, method);
        put16(&mut out, 0);
        put16(&mut out, DATE);
        put32(&mut out, crc);
        put32(&mut out, csize);
        put32(&mut out, size);
        put16(&mut out, name_len);
        put16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(&data);

        put32(&mut central, CENTRAL);
        put16(&mut central, needed);
        put16(&mut central, needed);
        put16(&mut central, UTF8);
        put16(&mut central, method);
        put16(&mut central, 0);
        put16(&mut central, DATE);
        put32(&mut central, crc);
        put32(&mut central, csize);
        put32(&mut central, size);
        put16(&mut central, name_len);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name);
    }
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_large("over 4 GiB"))?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_large("over 4 GiB"))?;
    out.extend_from_slice(&central);
    let count = files.0.len() as u16;
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, cd_size);
    put32(&mut out, cd_offset);
    put16(&mut out, 0);
    Ok(out)
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn at(&self, pos: usize, len: usize) -> Result<&[u8], ZipError> {
        pos.checked_add(len)
            .and_then(|end| self.0.get(pos..end))
            .ok_or(ZipError::Malformed("an entry runs past the end"))
    }

    fn u16(&self, pos: usize) -> Result<u16, ZipError> {
        Ok(u16::from_le_bytes(self.at(pos, 2)?.try_into().unwrap()))
    }

    fn u32(&self, pos: usize) -> Result<u32, ZipError> {
        Ok(u32::from_le_bytes(self.at(pos, 4)?.try_into().unwrap()))
    }
}

/// The document files inside an `.arrx`, by path. An entry outside the
/// document tree, a path twice, a checksum that fails, or a method other
/// than stored and deflate refuses the whole zip.
pub fn from_zip(bytes: &[u8]) -> Result<MemorySource, ZipError> {
    let r = Reader(bytes);
    let end = (0..=bytes.len().saturating_sub(22))
        .rev()
        .take(usize::from(u16::MAX) + 22)
        .find(|&p| r.u32(p) == Ok(END))
        .ok_or(ZipError::Malformed("no end-of-directory record"))?;
    if r.u16(end + 4)? != 0 || r.u16(end + 6)? != 0 {
        return Err(ZipError::Malformed("a zip in several parts"));
    }
    let count = usize::from(r.u16(end + 10)?);
    let mut at = r.u32(end + 16)? as usize;
    let mut files = BTreeMap::new();
    for _ in 0..count {
        if r.u32(at)? != CENTRAL {
            return Err(ZipError::Malformed("a bad directory entry"));
        }
        let flags = r.u16(at + 8)?;
        let method = r.u16(at + 10)?;
        let crc = r.u32(at + 16)?;
        let csize = r.u32(at + 20)? as usize;
        let size = r.u32(at + 24)?;
        let name_len = usize::from(r.u16(at + 28)?);
        let extra_len = usize::from(r.u16(at + 30)?);
        let comment_len = usize::from(r.u16(at + 32)?);
        let local = r.u32(at + 42)? as usize;
        let name = std::str::from_utf8(r.at(at + 46, name_len)?)
            .map_err(|_| ZipError::Malformed("a path that is not UTF-8"))?
            .to_owned();
        at += 46 + name_len + extra_len + comment_len;

        if let Some(dir) = name.strip_suffix('/') {
            if size != 0 || !in_tree(&format!("{dir}/x")) {
                return Err(ZipError::OutsideTree(name));
            }
            continue;
        }
        if !in_tree(&name) {
            return Err(ZipError::OutsideTree(name));
        }
        if flags & ENCRYPTED != 0 || !matches!(method, STORED | DEFLATED) {
            return Err(ZipError::Unsupported(name));
        }
        if r.u32(local)? != LOCAL {
            return Err(ZipError::Malformed("a bad local header"));
        }
        let start = local + 30 + usize::from(r.u16(local + 26)?) + usize::from(r.u16(local + 28)?);
        let data = r.at(start, csize)?;
        let content = if method == STORED {
            data.to_vec()
        } else {
            let limit = size.min(MAX_ENTRY) as usize;
            miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit)
                .map_err(|_| ZipError::Corrupt(name.clone()))?
        };
        if content.len() != size as usize || crc32fast::hash(&content) != crc {
            return Err(ZipError::Corrupt(name));
        }
        if files.insert(name.clone(), content).is_some() {
            return Err(ZipError::Duplicate(name));
        }
    }
    Ok(MemorySource(files))
}

/// `doc` as an `.arrx`: `save`, zipped.
pub fn save_zip(doc: &Document) -> Result<Vec<u8>, ZipError> {
    to_zip(&crate::save(doc))
}

/// Opens the document an `.arrx` holds, as `open` does its directory form.
pub fn open_zip(bytes: &[u8]) -> Result<Document, OpenError> {
    open(&from_zip(bytes)?)
}

#[cfg(test)]
mod tests;
