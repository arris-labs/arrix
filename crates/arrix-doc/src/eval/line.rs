//! Batch evaluation's output: one JSON line per document
//! (docs/CONCURRENCY-WASM.md §Batch evaluation). Here, beside the document,
//! so `arrix eval` and the tests share one code path.

use arrix_core::{AssemblyId, Diagnostic, FeatureId, JointId, MaterialId, ParamId, PartId};
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Evaluation, Evaluator, FeatureOutcome, JointOutcome, PlacedComponent, SlotView};
use crate::document::Document;
use crate::mass::{BodyMass, MassOutcome};
use crate::open::{DocumentSource, OpenError, open};
use crate::registry::Registry;

/// The version of the eval line (ADR-0026): changes inside it are
/// additive only.
pub const EVAL_FORMAT: u32 = 1;

fn eval_format() -> u32 {
    EVAL_FORMAT
}

/// One document's evaluation, as `arrix eval` prints it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EvalLine {
    /// The line's version, [`EVAL_FORMAT`].
    #[serde(default = "eval_format")]
    pub format: u32,
    /// The document as the caller named it.
    pub doc: String,
    /// The sweep point, `null` outside a sweep.
    pub sweep: Option<SweepPoint>,
    pub status: EvalStatus,
    pub params: Vec<ParamLine>,
    pub features: Vec<FeatureLine>,
    pub bodies: Vec<BodyLine>,
    /// Every assembly as solved, in id order; absent when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assemblies: Vec<AssemblyLine>,
    /// The document's materials, in id order; absent when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<MaterialLine>,
    /// Mass, centroid and inertia per body, part and assembly (ADR-0031);
    /// absent when there is no body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<MassLine>,
}

/// One material: its density in kg/m³ (none when unset or invalid, which
/// the bodies of it say) and its colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MaterialLine {
    pub id: MaterialId,
    pub name: String,
    pub density: Option<f64>,
    pub appearance: [f64; 3],
}

/// The mass results, in SI: kilograms, metres, kg m² (about the centre of
/// mass).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MassLine {
    /// Every final body, in the part's frame.
    pub bodies: Vec<BodyMass>,
    pub parts: Vec<PartMassLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assemblies: Vec<AssemblyMassLine>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PartMassLine {
    pub id: PartId,
    #[serde(flatten)]
    pub outcome: MassOutcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AssemblyMassLine {
    pub id: AssemblyId,
    #[serde(flatten)]
    pub outcome: MassOutcome,
}

/// One assembly's solve: where each component is, in SI, and how each
/// joint ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AssemblyLine {
    pub id: AssemblyId,
    pub name: String,
    pub components: Vec<PlacedComponent>,
    pub joints: BTreeMap<JointId, JointOutcome>,
}

/// A point of `--sweep`: what was swept as the flag named it, the point's
/// place and its value in SI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SweepPoint {
    pub target: String,
    pub index: usize,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParamLine {
    pub id: ParamId,
    pub name: String,
    pub value: Option<f64>,
    pub diagnostic: Option<Diagnostic>,
}

/// One feature's outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FeatureLine {
    pub id: FeatureId,
    #[serde(rename = "type")]
    pub feature_type: String,
    /// `ok`, `frozen`, `frozen-stale`, `failed`, `suppressed` or
    /// `rolled-back`.
    pub status: String,
    /// The failure category, a stable key (docs/ARCHITECTURE.md §The kernel
    /// choke point).
    pub category: Option<String>,
    pub diagnostic: Option<Diagnostic>,
    /// What an evaluated feature has to say that did not stop it: why it is
    /// frozen, that it reads a stale result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

/// One body's measures, in SI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// The eval line's JSON Schema, draft 2020-12 (ADR-0026).
#[cfg(feature = "schema")]
pub fn eval_line_schema() -> serde_json::Value {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<EvalLine>()
        .to_value()
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
            let mut notes = Vec::new();
            let (status, diagnostic) = match &e.outcome {
                FeatureOutcome::Ok {
                    slots,
                    frozen,
                    diagnostics,
                    ..
                } => {
                    notes = diagnostics.clone();
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
                    let status = match frozen {
                        None => "ok",
                        Some(f) if f.stale => "frozen-stale",
                        Some(_) => "frozen",
                    };
                    let reason = frozen.and_then(|f| {
                        notes
                            .iter()
                            .find(|d| d.code.as_str() == f.reason.code())
                            .cloned()
                    });
                    (status, reason)
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
                diagnostics: notes,
            });
        }
        let assemblies: Vec<AssemblyLine> = self
            .assemblies
            .iter()
            .map(|(id, outcome)| AssemblyLine {
                id: *id,
                name: doc.assemblies()[id].name.clone(),
                components: outcome.components.clone(),
                joints: outcome.joints.clone(),
            })
            .collect();
        let materials: Vec<MaterialLine> = doc
            .materials()
            .values()
            .map(|m| MaterialLine {
                id: m.id,
                name: m.name.clone(),
                density: self.masses.densities.get(&m.id).copied(),
                appearance: m.appearance.0,
            })
            .collect();
        let masses = &self.masses;
        let mass = (!masses.bodies.is_empty()).then(|| MassLine {
            bodies: masses.bodies.clone(),
            parts: masses
                .parts
                .iter()
                .map(|(id, outcome)| PartMassLine {
                    id: *id,
                    outcome: outcome.clone(),
                })
                .collect(),
            assemblies: masses
                .assemblies
                .iter()
                .map(|(id, outcome)| AssemblyMassLine {
                    id: *id,
                    outcome: outcome.clone(),
                })
                .collect(),
        });
        let failed = params.iter().any(|p| p.diagnostic.is_some())
            || features.iter().any(|f| f.status == "failed")
            || assemblies.iter().any(|a| {
                a.joints
                    .values()
                    .any(|j| matches!(j, JointOutcome::Failed { .. }))
            });
        EvalLine {
            format: EVAL_FORMAT,
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
            assemblies,
            materials,
            mass,
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
            r#"{"format":1,"doc":"empty","sweep":null,"status":"ok","params":[],"features":[],"bodies":[]}"#
        );
    }
}
