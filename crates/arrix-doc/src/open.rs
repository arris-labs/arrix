//! Opening a document from its unzipped form (docs/DATA-MODEL.md §File
//! format). This crate reads no files: a [`DocumentSource`] hands it bytes,
//! a directory on native (`arrix-cli`), memory in tests and the browser.

use std::collections::BTreeMap;

use arrix_core::{FeatureId, PartId};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::document::{Document, Invalid, Part};
use crate::frozen::BlobRef;
use crate::migrate::{self, Tree};

/// The schema this build reads and writes.
pub const SCHEMA: u64 = 2;

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
    #[error("migrating from schema {from} to {}: {message}", .from + 1)]
    Migration { from: u64, message: String },
    #[error("{blob}, which {feature} points at, is not in the document")]
    BlobMissing { blob: BlobRef, feature: FeatureId },
    #[error("{blob}, which {feature} points at, does not hash to its name")]
    BlobCorrupt { blob: BlobRef, feature: FeatureId },
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

impl OpenError {
    /// The diagnostic code (docs/DATA-MODEL.md §Evaluation) of the
    /// failures that have one.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            OpenError::Migration { .. } => Some("doc.migration"),
            OpenError::BlobMissing { .. } => Some("blob.missing"),
            OpenError::BlobCorrupt { .. } => Some("blob.corrupt"),
            _ => None,
        }
    }
}

fn read_json(source: &impl DocumentSource, path: &str) -> Result<Option<Value>, OpenError> {
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

fn from_tree<T: DeserializeOwned>(path: &str, value: Value) -> Result<T, OpenError> {
    serde_json::from_value(value).map_err(|e| OpenError::Malformed {
        path: path.into(),
        message: e.to_string(),
    })
}

/// Opens the document `source` holds: its header, parameters and parts,
/// migrated forward to this build's schema, its frozen results' blobs
/// checked against their names, and the whole checked as a command's
/// result is (`Document::validate`).
pub fn open(source: &impl DocumentSource) -> Result<Document, OpenError> {
    open_with(source, migrate::CHAIN)
}

fn open_with(
    source: &impl DocumentSource,
    chain: &[migrate::Migration],
) -> Result<Document, OpenError> {
    let header = read_json(source, DOCUMENT_JSON)?.ok_or(OpenError::NotADocument)?;
    let schema = header
        .get("schema")
        .and_then(Value::as_u64)
        .ok_or(OpenError::NoSchema)?;
    let current = chain.len() as u64 + 1;
    match schema {
        found if found > current => return Err(OpenError::NewerSchema { found }),
        0 => return Err(OpenError::UnknownSchema { found: 0 }),
        _ => {}
    }
    let mut tree = Tree::from([(DOCUMENT_JSON.to_owned(), header)]);
    if let Some(params) = read_json(source, PARAMS_JSON)? {
        tree.insert(PARAMS_JSON.to_owned(), params);
    }
    let parts = source.list("parts").map_err(|source| OpenError::Io {
        path: "parts".into(),
        source,
    })?;
    let mut files = Vec::new();
    for file in parts {
        let path = format!("parts/{file}");
        let id: PartId = file
            .strip_suffix(".json")
            .and_then(|stem| stem.parse().ok())
            .ok_or_else(|| OpenError::Unexpected {
                path: path.clone(),
                message: "a part is `parts/<part-id>.json`".into(),
            })?;
        tree.insert(
            path.clone(),
            read_json(source, &path)?.expect("listed, so present"),
        );
        files.push((path, id));
    }
    migrate::run(&mut tree, schema, chain).map_err(|e| OpenError::Migration {
        from: e.from,
        message: e.message,
    })?;
    let mut doc = Document {
        params: match tree.remove(PARAMS_JSON) {
            Some(v) => from_tree(PARAMS_JSON, v)?,
            None => Default::default(),
        },
        ..Document::default()
    };
    for (path, id) in files {
        let part: Part = from_tree(&path, tree.remove(&path).expect("read above"))?;
        if part.id != id {
            return Err(OpenError::Unexpected {
                message: format!("it holds part {}", part.id),
                path,
            });
        }
        doc.parts.insert(id, part);
    }
    load_blobs(source, &mut doc)?;
    doc.validate()?;
    Ok(doc)
}

/// Reads the blobs the frozen results point at, and only those: a blob no
/// record points at is dropped, as the next save would.
fn load_blobs(source: &impl DocumentSource, doc: &mut Document) -> Result<(), OpenError> {
    let mut blobs = BTreeMap::new();
    for part in doc.parts.values() {
        for record in part.features.values() {
            for blob in record.frozen.iter().flat_map(|f| f.blobs()) {
                if blobs.contains_key(blob) {
                    continue;
                }
                let feature = record.id;
                let bytes = read(source, &blob.path())?.ok_or_else(|| OpenError::BlobMissing {
                    blob: blob.clone(),
                    feature,
                })?;
                if !blob.matches(&bytes) {
                    return Err(OpenError::BlobCorrupt {
                        blob: blob.clone(),
                        feature,
                    });
                }
                blobs.insert(blob.clone(), bytes);
            }
        }
    }
    doc.blobs = blobs;
    Ok(())
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
        let err = open(&doc(&[(DOCUMENT_JSON, r#"{"schema": 3}"#)])).unwrap_err();
        assert!(matches!(err, OpenError::NewerSchema { found: 3 }));
        let message = err.to_string();
        assert!(
            message.contains("is 3") && message.contains("build's 2"),
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

    const PART: &str = "0000000000001";

    /// A one-feature part whose feature carries `frozen` (a JSON object).
    fn frozen_part(frozen: &str) -> String {
        format!(
            r#"{{"id":"{PART}","name":"P","history":["0000000000002"],"rollback":null,
              "features":{{"0000000000002":{{"id":"0000000000002","type_id":"gears.spur",
              "type_version":1,"name":"g","params":{{}},"inputs":{{}},"suppressed":false,
              "frozen":{frozen}}}}}}}"#
        )
    }

    fn frozen_json(body: &BlobRef, names: &BlobRef) -> String {
        format!(
            r#"{{"input_hash":"{}","plugin_version":"0.1.0","slots":{{"body":"{body}"}},"names":"{names}"}}"#,
            "ab".repeat(32)
        )
    }

    fn with_frozen(blobs: &[(&BlobRef, &[u8])]) -> (MemorySource, BlobRef, BlobRef) {
        let body = BlobRef::of(b"body bytes", "arrisbody");
        let names = BlobRef::of(b"{}", "json");
        let mut src = doc(&[
            (DOCUMENT_JSON, r#"{"schema": 2}"#),
            (
                &format!("parts/{PART}.json"),
                &frozen_part(&frozen_json(&body, &names)),
            ),
        ]);
        for (blob, bytes) in blobs {
            src.0.insert(blob.path(), bytes.to_vec());
        }
        (src, body, names)
    }

    #[test]
    fn a_frozen_result_opens_with_its_blobs_and_drops_the_rest() {
        let (mut src, body, names) = with_frozen(&[]);
        src.0.insert(body.path(), b"body bytes".to_vec());
        src.0.insert(names.path(), b"{}".to_vec());
        let stray = BlobRef::of(b"stray", "json");
        src.0.insert(stray.path(), b"stray".to_vec());
        let d = open(&src).unwrap();
        assert_eq!(d.blob(&body), Some(&b"body bytes"[..]));
        assert_eq!(d.blob(&stray), None);
        let saved = crate::save(&d);
        assert!(saved.0.contains_key(&body.path()) && saved.0.contains_key(&names.path()));
        assert!(
            !saved.0.contains_key(&stray.path()),
            "unreferenced blobs are dropped"
        );
        assert_eq!(open(&saved).unwrap(), d);
        assert_eq!(crate::save(&open(&saved).unwrap()).0, saved.0);
    }

    #[test]
    fn a_missing_or_corrupt_blob_is_named() {
        let (mut src, body, names) = with_frozen(&[]);
        src.0.insert(names.path(), b"{}".to_vec());
        let e = open(&src).unwrap_err();
        assert!(
            matches!(&e, OpenError::BlobMissing { blob, .. } if *blob == body),
            "{e}"
        );
        assert_eq!(e.code(), Some("blob.missing"));
        src.0.insert(body.path(), b"other bytes".to_vec());
        let e = open(&src).unwrap_err();
        assert!(
            matches!(&e, OpenError::BlobCorrupt { blob, .. } if *blob == body),
            "{e}"
        );
        assert_eq!(e.code(), Some("blob.corrupt"));
    }

    #[test]
    fn a_blob_name_is_a_hash_and_an_extension() {
        assert!(BlobRef::new("../x.json").is_err());
        assert!(BlobRef::new("a".repeat(64)).is_err());
        assert!(BlobRef::new(format!("{}.JSON", "a".repeat(64))).is_err());
        assert!(BlobRef::new(format!("{}.json", "A".repeat(64))).is_err());
        assert!(BlobRef::new(format!("{}.json", "a".repeat(64))).is_ok());
    }

    #[test]
    fn a_failing_migration_names_its_link() {
        fn boom(_: &mut Tree) -> Result<(), String> {
            Err("boom".into())
        }
        let e = open_with(&doc(&[(DOCUMENT_JSON, r#"{"schema": 1}"#)]), &[boom]).unwrap_err();
        assert_eq!(e.code(), Some("doc.migration"));
        assert_eq!(e.to_string(), "migrating from schema 1 to 2: boom");
    }

    #[test]
    fn a_schema_1_document_opens_as_schema_2() {
        let d = open(&doc(&[(DOCUMENT_JSON, r#"{"schema": 1}"#)])).unwrap();
        let saved = crate::save(&d);
        assert_eq!(saved.0[DOCUMENT_JSON], b"{\n  \"schema\": 2\n}\n");
    }

    #[test]
    fn a_memory_source_lists_one_level() {
        let src = doc(&[("parts/a.json", ""), ("parts/b/c.json", ""), ("partsx", "")]);
        assert_eq!(src.list("parts").unwrap(), ["a.json"]);
        assert!(src.list("blobs").unwrap().is_empty());
    }
}
