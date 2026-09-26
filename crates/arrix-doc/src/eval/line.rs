//! Batch evaluation's output: one JSON line per document
//! (docs/CONCURRENCY-WASM.md §Batch evaluation). Here, beside the document,
//! so `arrix eval` and the tests share one code path.

use arrix_core::{Diagnostic, FeatureId, ParamId, PartId};
use serde::{Deserialize, Serialize};

use super::{Evaluation, Evaluator, FeatureOutcome, SlotView};
use crate::document::Document;
use crate::open::{DocumentSource, OpenError, open};
use crate::registry::Registry;

/// One document's evaluation, as `arrix eval` prints it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvalLine {
    /// The document as the caller named it.
    pub doc: String,
    /// The sweep point, `null` outside a sweep.
    pub sweep: Option<SweepPoint>,
    pub status: EvalStatus,
    pub params: Vec<ParamLine>,
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
    /// Every parameter and every feature not suppressed or rolled back
    /// evaluated.
    Ok,
    /// At least one parameter or feature failed.
    Failed,
}

/// One parameter's value in SI, or why it has none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamLine {
    pub id: ParamId,
    pub name: String,
    pub value: Option<f64>,
    pub diagnostic: Option<Diagnostic>,
}

/// One feature's outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureLine {
    pub id: FeatureId,
    #[serde(rename = "type")]
    pub feature_type: String,
    /// `ok`, `failed`, `suppressed` or `rolled-back`.
    pub status: String,
    /// The failure category, a stable key (docs/ARCHITECTURE.md §The kernel
    /// choke point).
    pub category: Option<String>,
    pub diagnostic: Option<Diagnostic>,
}

/// One body's measures, in SI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyLine {
    pub part: PartId,
    pub feature: FeatureId,
    pub slot: String,
    pub volume: f64,
    pub area: f64,
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
}

/// Opens the document `source` holds and evaluates it with `registry`'s
/// feature types, named `doc` in the line.
pub fn eval(
    doc: &str,
    source: &impl DocumentSource,
    registry: &Registry,
) -> Result<EvalLine, OpenError> {
    let document = open(source)?;
    let evaluation = Evaluator::new(registry.clone()).evaluate(&document);
    Ok(evaluation.line(doc, &document))
}

impl Evaluation {
    /// This evaluation as `arrix eval` prints it: parameters by id,
    /// features and their bodies in evaluation order.
    pub fn line(&self, name: &str, doc: &Document) -> EvalLine {
        let params: Vec<ParamLine> = self
            .params
            .iter()
            .map(|(id, v)| ParamLine {
                id: *id,
                name: doc.params()[id].name.clone(),
                value: v.as_ref().ok().map(|q| q.si),
                diagnostic: v.as_ref().err().cloned(),
            })
            .collect();
        let mut features = Vec::new();
        let mut bodies = Vec::new();
        for e in &self.events {
            let (part, _, record) = doc
                .feature(e.feature)
                .expect("evaluated from this document");
            let (status, diagnostic) = match &e.outcome {
                FeatureOutcome::Ok { slots, .. } => {
                    for (slot, view) in slots {
                        if let SlotView::Body(m) = view {
                            bodies.push(BodyLine {
                                part: part.id,
                                feature: e.feature,
                                slot: slot.to_string(),
                                volume: m.volume,
                                area: m.area,
                                faces: m.faces,
                                edges: m.edges,
                                vertices: m.vertices,
                            });
                        }
                    }
                    ("ok", None)
                }
                FeatureOutcome::Failed { diagnostic, .. } => ("failed", Some(diagnostic.clone())),
                FeatureOutcome::Suppressed => ("suppressed", None),
                FeatureOutcome::RolledBack => ("rolled-back", None),
            };
            features.push(FeatureLine {
                id: e.feature,
                feature_type: record.type_id.to_string(),
                status: status.into(),
                category: diagnostic.as_ref().map(|d| d.code.to_string()),
                diagnostic,
            });
        }
        let failed = params.iter().any(|p| p.diagnostic.is_some())
            || features.iter().any(|f| f.status == "failed");
        EvalLine {
            doc: name.into(),
            sweep: None,
            status: if failed {
                EvalStatus::Failed
            } else {
                EvalStatus::Ok
            },
            params,
            features,
            bodies,
        }
    }
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
        let line = eval("empty", &src, &Registry::with_core_types()).unwrap();
        assert_eq!(
            serde_json::to_string(&line).unwrap(),
            r#"{"doc":"empty","sweep":null,"status":"ok","params":[],"features":[],"bodies":[]}"#
        );
    }
}
