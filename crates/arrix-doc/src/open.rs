//! Opening a document from its unzipped form (docs/DATA-MODEL.md §File
//! format). This crate reads no files: a [`DocumentSource`] hands it bytes,
//! a directory on native (`arrix-cli`), memory in tests and the browser.

use std::collections::BTreeMap;

use arrix_core::PartId;
use serde::de::DeserializeOwned;

use crate::document::{Document, Invalid, Part};

/// The schema this build reads and writes.
pub const SCHEMA: u64 = 1;

/// The header every document has.
pub const DOCUMENT_JSON: &str = "document.json";

/// The parameters; absent in a document that has none written.
pub const PARAMS_JSON: &str = "params.json";

/// Where a document's files come from. Paths are relative and
/// `/`-separated, as inside the `.arrx` zip.
pub trait DocumentSource {
    /// The bytes of `path`, or `None` when there is no such file.
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>>;
    /// The names of the files directly under `dir`, sorted; empty when
    /// there is no such directory.
    fn list(&self, dir: &str) -> std::io::Result<Vec<String>>;
}

/// A document's files in memory, keyed by path.
#[derive(Clone, Debug, Default)]
pub struct MemorySource(pub BTreeMap<String, Vec<u8>>);

impl DocumentSource for MemorySource {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.0.get(path).cloned())
    }

    fn list(&self, dir: &str) -> std::io::Result<Vec<String>> {
        let prefix = format!("{dir}/");
        Ok(self
            .0
            .keys()
            .filter_map(|p| p.strip_prefix(&prefix))
            .filter(|rest| !rest.contains('/'))
            .map(str::to_owned)
            .collect())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("no {DOCUMENT_JSON}: not a document")]
    NotADocument,
    #[error("{path} is not valid JSON: {message}")]
    Malformed { path: String, message: String },
    #[error("{DOCUMENT_JSON} has no integer `schema`")]
    NoSchema,
    #[error(
        "the document's schema is {found}, newer than this build's {SCHEMA}; open it with a newer ArriX"
    )]
    NewerSchema { found: u64 },
    #[error("the document's schema is {found}, which no ArriX wrote")]
    UnknownSchema { found: u64 },
    #[error("{path} is not a document file: {message}")]
    Unexpected { path: String, message: String },
    #[error("the document is malformed: {0}")]
    Invalid(#[from] Invalid),
}

fn read(source: &impl DocumentSource, path: &str) -> Result<Option<Vec<u8>>, OpenError> {
    source.read(path).map_err(|source| OpenError::Io {
        path: path.into(),
        source,
    })
}

fn read_json<T: DeserializeOwned>(
    source: &impl DocumentSource,
    path: &str,
) -> Result<Option<T>, OpenError> {
    let Some(bytes) = read(source, path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| OpenError::Malformed {
            path: path.into(),
            message: e.to_string(),
        })
}

/// Opens the document `source` holds: its header, parameters and parts,
/// checked as a command's result is (`Document::validate`).
pub fn open(source: &impl DocumentSource) -> Result<Document, OpenError> {
    let header: serde_json::Value =
        read_json(source, DOCUMENT_JSON)?.ok_or(OpenError::NotADocument)?;
    let schema = header
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .ok_or(OpenError::NoSchema)?;
    match schema {
        SCHEMA => {}
        found if found > SCHEMA => return Err(OpenError::NewerSchema { found }),
        found => return Err(OpenError::UnknownSchema { found }),
    }
    let mut doc = Document {
        params: read_json(source, PARAMS_JSON)?.unwrap_or_default(),
        ..Document::default()
    };
    let parts = source.list("parts").map_err(|source| OpenError::Io {
        path: "parts".into(),
        source,
    })?;
    for file in parts {
        let path = format!("parts/{file}");
        let id: PartId = file
            .strip_suffix(".json")
            .and_then(|stem| stem.parse().ok())
            .ok_or_else(|| OpenError::Unexpected {
                path: path.clone(),
                message: "a part is `parts/<part-id>.json`".into(),
            })?;
        let part: Part = read_json(source, &path)?.expect("listed, so present");
        if part.id != id {
            return Err(OpenError::Unexpected {
                message: format!("it holds part {}", part.id),
                path,
            });
        }
        doc.parts.insert(id, part);
    }
    doc.validate()?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(files: &[(&str, &str)]) -> MemorySource {
        MemorySource(
            files
                .iter()
                .map(|(p, s)| (p.to_string(), s.as_bytes().to_vec()))
                .collect(),
        )
    }

    #[test]
    fn opens_an_empty_document() {
        let d = open(&doc(&[(DOCUMENT_JSON, r#"{"schema": 1}"#)])).unwrap();
        assert_eq!(d, Document::default());
    }

    #[test]
    fn refuses_a_newer_schema_naming_both() {
        let err = open(&doc(&[(DOCUMENT_JSON, r#"{"schema": 2}"#)])).unwrap_err();
        assert!(matches!(err, OpenError::NewerSchema { found: 2 }));
        let message = err.to_string();
        assert!(
            message.contains("is 2") && message.contains("build's 1"),
            "{message}"
        );
    }

    #[test]
    fn refuses_what_is_not_a_document() {
        let err = |files: &[(&str, &str)]| open(&doc(files)).unwrap_err();
        assert!(matches!(err(&[]), OpenError::NotADocument));
        assert!(matches!(
            err(&[(DOCUMENT_JSON, "{")]),
            OpenError::Malformed { .. }
        ));
        assert!(matches!(
            err(&[(DOCUMENT_JSON, r#"{"schema": "1"}"#)]),
            OpenError::NoSchema
        ));
        assert!(matches!(
            err(&[(DOCUMENT_JSON, r#"{"schema": 0}"#)]),
            OpenError::UnknownSchema { found: 0 }
        ));
        let header = (DOCUMENT_JSON, r#"{"schema": 1}"#);
        assert!(matches!(
            err(&[header, ("parts/0000000000001.json", "{}")]),
            OpenError::Malformed { path, .. } if path == "parts/0000000000001.json"
        ));
        assert!(matches!(
            err(&[header, ("parts/notes.txt", "")]),
            OpenError::Unexpected { path, .. } if path == "parts/notes.txt"
        ));
        let part =
            r#"{"id":"0000000000002","name":"P","history":[],"features":{},"rollback":null}"#;
        assert!(matches!(
            err(&[header, ("parts/0000000000001.json", part)]),
            OpenError::Unexpected { message, .. } if message.contains("0000000000002")
        ));
        let dup = r#"{"0000000000001":{"name":"w","kind":"length","expr":"1 mm"},
                      "0000000000002":{"name":"w","kind":"length","expr":"2 mm"}}"#;
        assert!(matches!(
            err(&[header, (PARAMS_JSON, dup)]),
            OpenError::Invalid(Invalid::DuplicateParam(_))
        ));
        let extra = r#"{"0000000000001":{"name":"w","kind":"length","expr":"1 mm","unit":"mm"}}"#;
        assert!(
            matches!(
                err(&[header, (PARAMS_JSON, extra)]),
                OpenError::Malformed { .. }
            ),
            "an unknown field is refused, never dropped on the next save"
        );
    }

    #[test]
    fn a_memory_source_lists_one_level() {
        let src = doc(&[("parts/a.json", ""), ("parts/b/c.json", ""), ("partsx", "")]);
        assert_eq!(src.list("parts").unwrap(), ["a.json"]);
        assert!(src.list("blobs").unwrap().is_empty());
    }
}
