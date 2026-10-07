//! `core.revolve` (docs/DATA-MODEL.md §The core feature types): a sketch
//! region turned about an `axis` into a new body, or joined to or cut from
//! the body its `target` input names, as `core.extrude` is.

use arrix_core::{Diagnostic, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};

use crate::core_choice::{choice, shown_when};
use crate::core_extrude::{Sweep, place};
use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// `core.revolve`, by its `mode` choice as `core.extrude`'s: `new` (the
/// default), `join` or `cut`. Its `angle` defaults to a full turn, which
/// the kernel gives no end caps; a partial turn is positive and at most a
/// full one.
pub struct CoreRevolve {
    spec: FeatureTypeSpec,
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl CoreRevolve {
    pub fn new() -> Self {
        let input = |name: &str, title: &str, kind, shown| InputSpec {
            name: name.into(),
            title: title.into(),
            kind,
            list: false,
            shown_when: shown,
        };
        Self {
            spec: FeatureTypeSpec {
                id: "core.revolve".into(),
                version: 1,
                title: "Revolve".into(),
                params: vec![ParamSpec {
                    name: "angle".into(),
                    title: "Angle".into(),
                    kind: QuantityKind::Angle,
                    default: "360 deg".into(),
                    shown_when: None,
                }],
                inputs: vec![
                    input("region", "Sketch region", InputKind::Region, None),
                    input("axis", "Axis to turn about", InputKind::Axis, None),
                    input(
                        "target",
                        "Body to join to or cut",
                        InputKind::Body,
                        shown_when("mode", &["join", "cut"]),
                    ),
                ],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: Some("target".into()),
                }],
                choices: vec![choice(
                    "mode",
                    "Mode",
                    &[("new", "New body"), ("join", "Join"), ("cut", "Cut")],
                    Some("new"),
                )],
            },
        }
    }
}

impl Default for CoreRevolve {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for CoreRevolve {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let mode = args.choices.get("mode").map_or("new", String::as_str);
        let Some(InputValue::Region(profile)) = args.input("region").map(|i| &i.value) else {
            return Err(error(
                "revolve.no-region",
                "a revolve needs a sketch region",
            ));
        };
        let Some(InputValue::Axis(axis)) = args.input("axis").map(|i| &i.value) else {
            return Err(error(
                "revolve.no-axis",
                "a revolve needs an axis: a sketch line or a straight edge",
            ));
        };
        let target = match args.input("target").map(|i| &i.value) {
            Some(InputValue::Body(b)) => Some(b),
            _ => None,
        };
        let angle = args.param("angle").unwrap_or(std::f64::consts::TAU);
        if !(angle > 0.0 && angle <= std::f64::consts::TAU + 1e-12) {
            return Err(error(
                "revolve.angle",
                "a revolve turns by more than nothing and at most a full turn",
            ));
        }
        let (body, kept) = place(
            kernel,
            Sweep {
                ty: "revolve",
                what: "a revolve",
            },
            mode,
            target,
            |k| k.revolve(profile, axis, angle),
        )?;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(body),
            }],
            // A join or a cut keeps its tool for a pattern or a mirror
            // of it to copy (ADR-0013).
            tool: kept,
        }
        .into())
    }
}
