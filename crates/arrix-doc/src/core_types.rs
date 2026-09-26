//! The built-in feature types (docs/DATA-MODEL.md §The core feature
//! types), registered under `core.` through the same `FeatureType` shape a
//! plugin's feature is adapted onto.

use arrix_core::{Diagnostic, FeatureId, Frame, QuantityKind, Ref, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};
use arrix_sketch::{
    ConstraintId, Diagnostics, RESIDUAL_TOL, Sketch, SketchStatus, constraint_residuals,
    find_regions,
};

use crate::eval::{RegionView, SketchView};
use crate::registry::{FeatureArgs, FeatureType, SketchArgs, TypeOutput};

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

/// `core.datum-plane`: a plane `offset` along the normal of its `plane`
/// input (a datum's plane slot or a planar face), or, with no input, of the
/// world plane its `world` choice names (`xy`, the default, `yz` or `zx`).
pub struct DatumPlane {
    spec: FeatureTypeSpec,
}

/// The world planes a datum plane may stand on, by the word its `world`
/// choice holds.
pub const WORLD_PLANES: [(&str, Frame); 3] = [
    ("xy", Frame::WORLD_XY),
    ("yz", Frame::WORLD_YZ),
    ("zx", Frame::WORLD_ZX),
];

impl DatumPlane {
    pub fn new() -> Self {
        let slot = |s: &str| SlotName::new(s).expect("a literal slot name");
        Self {
            spec: FeatureTypeSpec {
                id: "core.datum-plane".into(),
                version: 1,
                title: "Datum plane".into(),
                params: vec![ParamSpec {
                    name: "offset".into(),
                    title: "Offset".into(),
                    kind: QuantityKind::Length,
                    default: "0 mm".into(),
                }],
                inputs: vec![InputSpec {
                    name: "plane".into(),
                    title: "Plane or planar face".into(),
                    kind: InputKind::Plane,
                }],
                outputs: vec![SlotSpec {
                    name: slot("plane"),
                    kind: SlotKind::Plane,
                }],
            },
        }
    }
}

/// The plane a datum plane or a sketch stands on: its `plane` input, or,
/// with none, the world plane its `world` choice names (`xy` by default).
/// `ty` is the type's name, which prefixes its codes (`sketch.world`).
fn stand_on(args: &FeatureArgs, ty: &str, what: &str) -> Result<Frame, Diagnostic> {
    if let Some(other) = args.choices.keys().find(|k| *k != "world") {
        return Err(error(
            "feature.unknown-choice",
            format!("{what} has no choice `{other}`"),
        ));
    }
    let world = args.choices.get("world");
    match (args.input("plane"), world) {
        (Some(_), Some(_)) => Err(error(
            &format!("{ty}.two-bases"),
            format!("{what} stands on its input plane or on a world plane, not both"),
        )),
        (Some(input), None) => match &input.value {
            InputValue::Plane(frame) => Ok(*frame),
            InputValue::Region(_) => Err(error(
                "input.wrong-kind",
                format!("{what} stands on a plane, not a region"),
            )),
        },
        (None, world) => {
            let word = world.map_or("xy", String::as_str);
            WORLD_PLANES
                .iter()
                .find(|(w, _)| *w == word)
                .map(|(_, f)| *f)
                .ok_or_else(|| {
                    error(
                        &format!("{ty}.world"),
                        format!("`{word}` is not a world plane: xy, yz or zx"),
                    )
                })
        }
    }
}

impl Default for DatumPlane {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for DatumPlane {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let offset = args.param("offset").unwrap_or(0.0);
        let plane = stand_on(args, "datum-plane", "a datum plane")?.offset(offset);
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Plane(plane),
            }],
        }
        .into())
    }
}

/// `core.sketch`: its record's sketch on the plane of its `plane` input,
/// or of the world plane its `world` choice names (`xy` by default),
/// solved from the stored positions with its dimensions resolved
/// (ADR-0005). Its one slot, `sketch`, is the solved sketch and its
/// regions as keyed profiles. A solve that conflicts or does not converge
/// fails the feature, naming the constraints it could not meet.
pub struct CoreSketch {
    spec: FeatureTypeSpec,
    slots: [SlotName; 1],
}

impl CoreSketch {
    pub fn new() -> Self {
        Self {
            spec: FeatureTypeSpec {
                id: "core.sketch".into(),
                version: 1,
                title: "Sketch".into(),
                params: vec![],
                inputs: vec![InputSpec {
                    name: "plane".into(),
                    title: "Plane or planar face".into(),
                    kind: InputKind::Plane,
                }],
                outputs: vec![],
            },
            slots: [SlotName::new("sketch").expect("a literal slot name")],
        }
    }
}

impl Default for CoreSketch {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for CoreSketch {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn sketch_slots(&self) -> &[SlotName] {
        &self.slots
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let plane = stand_on(args, "sketch", "a sketch")?;
        let Some(SketchArgs { feature, sketch }) = &args.sketch else {
            return Err(error(
                "sketch.missing",
                "the sketch feature holds no sketch",
            ));
        };
        let mut sketch = sketch.clone();
        let (_, solved) = Diagnostics::evaluate(&mut sketch);
        unmet(*feature, &sketch, &solved)?;
        let regions = find_regions(&sketch)
            .into_iter()
            .map(|region| {
                let profile = region
                    .to_profile(&sketch, plane)
                    .map_err(|e| error("sketch.profile", e.to_string()))?;
                Ok(RegionView {
                    key: region.key,
                    profile,
                })
            })
            .collect::<Result<_, Diagnostic>>()?;
        Ok(TypeOutput {
            output: FeatureOutput::default(),
            sketches: vec![(
                self.slots[0].clone(),
                SketchView {
                    plane,
                    dof: solved.dof,
                    redundant: solved.redundant,
                    sketch,
                    regions,
                },
            )],
        })
    }
}

/// A solve that did not meet its constraints, as the feature's failure:
/// a conflict names the constraints that cannot be met together, a solve
/// that stopped short the ones it left unmet.
fn unmet(feature: FeatureId, sketch: &Sketch, solved: &Diagnostics) -> Result<(), Diagnostic> {
    let named = |ids: Vec<ConstraintId>| {
        ids.into_iter()
            .map(move |entity| Ref::Sketch { feature, entity })
    };
    let (code, ids) = match solved.status {
        SketchStatus::Ok | SketchStatus::FullyConstrained => return Ok(()),
        SketchStatus::OverConstrained => ("sketch.conflict", solved.conflicting.clone()),
        SketchStatus::DidNotConverge => (
            "sketch.not-converged",
            constraint_residuals(sketch)
                .into_iter()
                .filter(|(_, r)| *r > RESIDUAL_TOL * 1e3)
                .map(|(id, _)| id)
                .collect(),
        ),
    };
    Err(error(code, solved.message.clone()).with_refs(named(ids)))
}

#[cfg(test)]
mod tests {
    use arrix_core::Id;
    use arrix_sketch::{Constraint, Draft, Point, SolverOptions, solve_with_options};

    use super::*;

    /// A solver out of iterations short of a set it could meet: the
    /// feature fails naming the constraint it left unmet.
    #[test]
    fn a_solve_stopped_short_names_what_it_left_unmet() {
        let mut d = Draft::seeded(1);
        let a = d.add_point(Point::fixed(0.0, 0.0));
        let b = d.add_point(Point::new(0.05, 0.0));
        let far = d.add_constraint(Constraint::Distance { a, b, value: 0.10 });
        let options = SolverOptions {
            max_iterations: 0,
            ..SolverOptions::default()
        };
        let result = solve_with_options(&mut d, &options);
        let solved = Diagnostics::analyze(&d, &result);
        let feature = FeatureId(Id(3));
        let failure = unmet(feature, &d, &solved).unwrap_err();
        assert_eq!(failure.code.as_str(), "sketch.not-converged");
        assert_eq!(
            failure.refs,
            [Ref::Sketch {
                feature,
                entity: far
            }]
        );
    }
}
