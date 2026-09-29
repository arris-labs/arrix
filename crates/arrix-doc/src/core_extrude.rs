//! `core.extrude` (docs/DATA-MODEL.md §The core feature types): a sketch
//! region extruded a signed `distance` into a new body, or joined to or cut
//! from the body its `target` input names (ADR-0007).

use arrix_core::{Diagnostic, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};

use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// `core.extrude`, by its `mode` choice: `new` (the default) makes a body
/// of its own, `join` fuses the extrusion into its `target` and `cut`
/// removes it from there. In the last two its one output slot, `body`, is
/// the target slot's next version; in `new` it is a slot of its own, since
/// `target` is then unset (§Bodies across features).
pub struct CoreExtrude {
    spec: FeatureTypeSpec,
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl CoreExtrude {
    pub fn new() -> Self {
        Self {
            spec: FeatureTypeSpec {
                id: "core.extrude".into(),
                version: 1,
                title: "Extrude".into(),
                params: vec![ParamSpec {
                    name: "distance".into(),
                    title: "Distance".into(),
                    kind: QuantityKind::Length,
                    default: "10 mm".into(),
                }],
                inputs: vec![
                    InputSpec {
                        name: "region".into(),
                        title: "Sketch region".into(),
                        kind: InputKind::Region,
                    },
                    InputSpec {
                        name: "target".into(),
                        title: "Body to join to or cut".into(),
                        kind: InputKind::Body,
                    },
                ],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: Some("target".into()),
                }],
            },
        }
    }
}

impl Default for CoreExtrude {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for CoreExtrude {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        if let Some(other) = args.choices.keys().find(|k| *k != "mode") {
            return Err(error(
                "feature.unknown-choice",
                format!("an extrude has no choice `{other}`"),
            ));
        }
        let mode = args.choices.get("mode").map_or("new", String::as_str);
        let Some(InputValue::Region(profile)) = args.input("region").map(|i| &i.value) else {
            return Err(error(
                "extrude.no-region",
                "an extrude needs a sketch region",
            ));
        };
        let target = match args.input("target").map(|i| &i.value) {
            Some(InputValue::Body(b)) => Some(b),
            _ => None,
        };
        let distance = args.param("distance").unwrap_or(0.0);
        let body = match (mode, target) {
            ("new", None) => kernel.extrude(profile, distance)?,
            ("new", Some(_)) => {
                return Err(error(
                    "extrude.stray-target",
                    "a new body has no target to join to or cut; clear the target or choose join or cut",
                ));
            }
            ("join" | "cut", None) => {
                return Err(error(
                    "extrude.no-target",
                    format!("an extrude in {mode} mode needs the body to {mode}"),
                ));
            }
            ("join", Some(target)) => {
                let tool = kernel.extrude(profile, distance)?;
                kernel.fuse(target, &tool)?
            }
            ("cut", Some(target)) => {
                let tool = kernel.extrude(profile, distance)?;
                kernel.cut(target, &tool)?
            }
            (other, _) => {
                return Err(error(
                    "extrude.mode",
                    format!("`{other}` is not an extrude mode: new, join or cut"),
                ));
            }
        };
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(body),
            }],
        }
        .into())
    }
}
