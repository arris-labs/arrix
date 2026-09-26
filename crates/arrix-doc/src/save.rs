//! Writing a document's directory form (docs/DATA-MODEL.md §File format).
//! Deterministic: keys sorted, two-space indent, `\n` line ends, a trailing
//! newline, floats in shortest round-trip form, so the same document gives
//! the same bytes, and saving what was loaded gives what was read.

use serde::Serialize;

use crate::document::Document;
use crate::open::{DOCUMENT_JSON, MemorySource, PARAMS_JSON, SCHEMA};

/// `value` as the file format writes JSON. Going through `Value` sorts the
/// keys (its map is ordered, `serde_json`'s `preserve_order` being off);
/// `serde_json` writes floats shortest-first.
pub fn to_json(value: &impl Serialize) -> Vec<u8> {
    let tree = serde_json::to_value(value).expect("document types serialise to JSON");
    let mut out = serde_json::to_vec_pretty(&tree).expect("a Value always serialises");
    out.push(b'\n');
    out
}

/// The files of `doc`'s directory form, by path. The caller writes them,
/// and removes any other file a previous save left.
pub fn save(doc: &Document) -> MemorySource {
    let mut files = MemorySource::default();
    let header = serde_json::json!({ "schema": SCHEMA });
    files.0.insert(DOCUMENT_JSON.into(), to_json(&header));
    files.0.insert(PARAMS_JSON.into(), to_json(&doc.params));
    for (id, part) in &doc.parts {
        files.0.insert(format!("parts/{id}.json"), to_json(part));
    }
    files
}

#[cfg(test)]
mod tests;
