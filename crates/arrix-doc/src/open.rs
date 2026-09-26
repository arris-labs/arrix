//! Opening a document from its unzipped form (docs/DATA-MODEL.md §File
//! format). This crate reads no files: a [`DocumentSource`] hands it bytes,
//! a directory on native (`arrix-cli`), memory in tests and the browser.

use std::collections::BTreeMap;

/// The schema this build reads and writes.
pub const SCHEMA: u64 = 1;

/// The header every document has.
pub const DOCUMENT_JSON: &str = "document.json";

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
    #[error("{path}: parts are read from C1's M1; this build opens only an empty document")]
    Unsupported { path: String },
}

/// An opened document. So far only the empty one opens: its header is
/// checked and nothing else is interpreted (plans/c1-m0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub schema: u64,
}

fn read(source: &impl DocumentSource, path: &str) -> Result<Option<Vec<u8>>, OpenError> {
    source.read(path).map_err(|source| OpenError::Io {
        path: path.into(),
        source,
    })
}

/// Opens the document `source` holds.
pub fn open(source: &impl DocumentSource) -> Result<Document, OpenError> {
    let bytes = read(source, DOCUMENT_JSON)?.ok_or(OpenError::NotADocument)?;
    let header: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| OpenError::Malformed {
            path: DOCUMENT_JSON.into(),
            message: e.to_string(),
        })?;
    let schema = header
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .ok_or(OpenError::NoSchema)?;
    match schema {
        SCHEMA => {}
        found if found > SCHEMA => return Err(OpenError::NewerSchema { found }),
        found => return Err(OpenError::UnknownSchema { found }),
    }
    let parts = source.list("parts").map_err(|source| OpenError::Io {
        path: "parts".into(),
        source,
    })?;
    if let Some(part) = parts.first() {
        return Err(OpenError::Unsupported {
            path: format!("parts/{part}"),
        });
    }
    Ok(Document { schema })
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
        assert_eq!(d, Document { schema: 1 });
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
        let with_part = err(&[
            (DOCUMENT_JSON, r#"{"schema": 1}"#),
            ("parts/0000000000001.json", "{}"),
        ]);
        assert!(
            matches!(&with_part, OpenError::Unsupported { path } if path == "parts/0000000000001.json"),
            "{with_part:?}"
        );
    }

    #[test]
    fn a_memory_source_lists_one_level() {
        let src = doc(&[("parts/a.json", ""), ("parts/b/c.json", ""), ("partsx", "")]);
        assert_eq!(src.list("parts").unwrap(), ["a.json"]);
        assert!(src.list("blobs").unwrap().is_empty());
    }
}
