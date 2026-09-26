//! Batch evaluation's output: one JSON line per document
//! (docs/CONCURRENCY-WASM.md §Batch evaluation). Here, beside the document,
//! so `arrix eval` and the tests share one code path.

use arrix_core::{Diagnostic, FeatureId, PartId};
use serde::{Deserialize, Serialize};

use crate::open::{DocumentSource, OpenError, open};

/// One document's evaluation, as `arrix eval` prints it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvalLine {
    /// The document as the caller named it.
    pub doc: String,
    /// The sweep point, `null` outside a sweep.
    pub sweep: Option<SweepPoint>,
    pub status: EvalStatus,
    pub features: Vec<FeatureLine>,
    pub bodies: Vec<BodyLine>,
}

/// A point of `--sweep`. Its shape comes with the flag (M4); until then no
/// line has one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SweepPoint {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvalStatus {
    /// Every feature evaluated.
    Ok,
    /// At least one feature failed.
    Failed,
}

/// One feature's outcome (filled from M1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureLine {
    pub id: FeatureId,
    #[serde(rename = "type")]
    pub feature_type: String,
    pub status: String,
    /// The failure category, a stable key (docs/ARCHITECTURE.md §The kernel
    /// choke point).
    pub category: Option<String>,
    pub diagnostic: Option<Diagnostic>,
}

/// One body's measures, in SI (filled from M1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyLine {
    pub part: PartId,
    pub slot: String,
    pub volume: f64,
    pub area: f64,
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
}

/// Opens and evaluates the document `source` holds, named `doc` in the line.
pub fn eval(doc: &str, source: &impl DocumentSource) -> Result<EvalLine, OpenError> {
    open(source)?;
    Ok(EvalLine {
        doc: doc.into(),
        sweep: None,
        status: EvalStatus::Ok,
        features: Vec::new(),
        bodies: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::{DOCUMENT_JSON, MemorySource};

    #[test]
    fn an_empty_document_evaluates_to_an_ok_line() {
        let mut src = MemorySource::default();
        src.0
            .insert(DOCUMENT_JSON.into(), br#"{"schema": 1}"#.to_vec());
        let line = eval("empty", &src).unwrap();
        assert_eq!(
            serde_json::to_string(&line).unwrap(),
            r#"{"doc":"empty","sweep":null,"status":"ok","features":[],"bodies":[]}"#
        );
    }
}
