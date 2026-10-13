//! A batch evaluation's failed kernel calls as Arris fixture recipes
//! (docs/ARCHITECTURE.md §The kernel choke point, "Fixture export"), for
//! `arrix eval --save-fixtures`. Here, beside the line, so the CLI only
//! writes files.

use arrix_core::{DiagnosticCode, FeatureId};
use arrix_kernel::{FixtureError, FixtureRecipe, KernelCall};

use super::{EvalLine, Evaluation, Evaluator, FeatureOutcome};
use crate::open::{DocumentSource, OpenError, open};
use crate::registry::Registry;

/// A recipe ready to be written as `<name>/fixture.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedRecipe {
    /// The directory name: the diagnostic's code and the first twelve hex
    /// digits of the BLAKE3 of `json`, so equal recipes share one.
    pub name: String,
    pub json: String,
    /// The files its `step` steps read, each by the name the recipe gives
    /// it, written beside `fixture.json`.
    pub files: Vec<(String, Vec<u8>)>,
}

/// One feature that failed at a kernel call, and what became of its record.
#[derive(Clone, Debug, PartialEq)]
pub struct FailedCall {
    pub feature: FeatureId,
    pub code: DiagnosticCode,
    /// The recipe, or why the record has none (a frozen body in its
    /// closure, `FixtureError::NoStep`).
    pub recipe: Result<SavedRecipe, FixtureError>,
}

/// An evaluation as `arrix eval` prints it, with the recipes of its kernel
/// failures.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub line: EvalLine,
    pub failures: Vec<FailedCall>,
}

/// Opens and evaluates like [`eval`](super::eval), keeping each failed
/// kernel call as a recipe. `budget` caps the steps of each kernel call
/// (`Kernel::set_budget`).
pub fn run(
    doc: &str,
    source: &impl DocumentSource,
    registry: &Registry,
    budget: Option<u64>,
) -> Result<Run, OpenError> {
    let document = open(source)?;
    let mut evaluator = Evaluator::new(registry.clone());
    evaluator.set_budget(budget);
    let evaluation = evaluator.evaluate(&document);
    let failures = failures(&evaluation, evaluator.kernel_calls());
    Ok(Run {
        line: evaluation.line(doc, &document),
        failures,
    })
}

/// The recipe of every feature that failed at a kernel call, in evaluation
/// order. A failure with no kernel call (a bad parameter, an unknown type)
/// has no record and is not listed.
pub(crate) fn failures(evaluation: &Evaluation, records: &[KernelCall]) -> Vec<FailedCall> {
    evaluation
        .events
        .iter()
        .filter_map(|e| {
            let FeatureOutcome::Failed {
                diagnostic,
                call_index: Some(call),
                ..
            } = &e.outcome
            else {
                return None;
            };
            // A spent budget is the caller's cap, not a kernel failure: its
            // record replays to success, so there is nothing to file.
            if diagnostic.code.as_str() == "kernel.interrupted.budget" {
                return None;
            }
            let recipe = FixtureRecipe::of(records, *call, &diagnostic.code).map(|r| {
                let json = r.to_json();
                let hash = blake3::hash(json.as_bytes()).to_hex();
                SavedRecipe {
                    name: format!("{}-{}", diagnostic.code, &hash.as_str()[..12]),
                    json,
                    files: r
                        .files()
                        .map(|(name, bytes)| (name.to_owned(), bytes.to_vec()))
                        .collect(),
                }
            });
            Some(FailedCall {
                feature: e.feature,
                code: diagnostic.code.clone(),
                recipe,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use arrix_core::{Diagnostic, Severity};
    use arrix_kernel::{BodyHash, CallIndex, CallPrecision, KernelOp};

    use super::*;
    use crate::eval::EvalEvent;

    fn record(op: KernelOp, sources: Vec<CallIndex>) -> KernelCall {
        KernelCall {
            op,
            operands: sources.iter().map(|_| BodyHash::of(b"body")).collect(),
            sources,
            precision: CallPrecision {
                default_tolerance: 1e-6,
            },
            arris_version: "0.8.0".into(),
        }
    }

    fn failed(feature: u64, call: Option<u32>) -> EvalEvent {
        EvalEvent {
            feature: FeatureId(arrix_core::Id(feature)),
            outcome: FeatureOutcome::Failed {
                diagnostic: Diagnostic::new(Severity::Error, "kernel.degenerate.x", "x").unwrap(),
                call: None,
                call_index: call.map(CallIndex),
            },
        }
    }

    #[test]
    fn a_failure_over_a_frozen_body_is_reported_and_the_rest_are_listed() {
        let records = [
            record(
                KernelOp::FromBytes {
                    bytes: BodyHash::of(b"frozen"),
                },
                vec![],
            ),
            record(KernelOp::MassProperties, vec![CallIndex(0)]),
        ];
        let evaluation = Evaluation {
            params: Default::default(),
            events: vec![failed(1, Some(1)), failed(2, None)],
            assemblies: Default::default(),
            masses: Default::default(),
        };
        let listed = failures(&evaluation, &records);
        let [one] = listed.as_slice() else {
            panic!("{listed:?}")
        };
        assert_eq!(one.feature, FeatureId(arrix_core::Id(1)));
        assert_eq!(
            one.recipe,
            Err(FixtureError::NoStep {
                call: CallIndex(0),
                op: "from_bytes"
            })
        );
    }
}
