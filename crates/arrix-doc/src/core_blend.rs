//! `core.fillet` and `core.chamfer` (docs/DATA-MODEL.md §The core feature
//! types): the edges of one `target` body, named by `Ref::Topo`, rounded at
//! a `radius` or cut flat at a `distance`. The slot is the target's next
//! version (ADR-0007), the faces a blend trims keep their names and what it
//! made is named from the edges.

use arrix_core::{Diagnostic, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};

use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// Which blend a [`CoreBlend`] makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blend {
    Fillet,
    Chamfer,
}

/// `core.fillet` or `core.chamfer`: one type, two kernel calls. A fillet's
/// size parameter is `radius`, a chamfer's `distance`; both default to 1 mm.
pub struct CoreBlend {
    blend: Blend,
    spec: FeatureTypeSpec,
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl CoreBlend {
    pub fn fillet() -> Self {
        Self::new(Blend::Fillet)
    }

    pub fn chamfer() -> Self {
        Self::new(Blend::Chamfer)
    }

    fn new(blend: Blend) -> Self {
        let (id, title, size, size_title) = match blend {
            Blend::Fillet => ("core.fillet", "Fillet", "radius", "Radius"),
            Blend::Chamfer => ("core.chamfer", "Chamfer", "distance", "Distance"),
        };
        Self {
            blend,
            spec: FeatureTypeSpec {
                id: id.into(),
                version: 1,
                title: title.into(),
                params: vec![ParamSpec {
                    name: size.into(),
                    title: size_title.into(),
                    kind: QuantityKind::Length,
                    default: "1 mm".into(),
                }],
                inputs: vec![
                    InputSpec {
                        name: "target".into(),
                        title: "Body to blend".into(),
                        kind: InputKind::Body,
                        list: false,
                    },
                    InputSpec {
                        name: "edges".into(),
                        title: "Edges".into(),
                        kind: InputKind::Edge,
                        list: true,
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

    /// The type's name as a code prefix and in messages: `fillet`.
    fn noun(&self) -> &'static str {
        match self.blend {
            Blend::Fillet => "fillet",
            Blend::Chamfer => "chamfer",
        }
    }
}

impl FeatureType for CoreBlend {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let noun = self.noun();
        if let Some(other) = args.choices.keys().next() {
            return Err(error(
                "feature.unknown-choice",
                format!("a {noun} has no choice `{other}`"),
            ));
        }
        let Some(InputValue::Body(target)) = args.input("target").map(|i| &i.value) else {
            return Err(error(
                &format!("{noun}.no-target"),
                format!("a {noun} needs the body to blend"),
            ));
        };
        let edges: Vec<_> = args
            .inputs_named("edges")
            .filter_map(|i| match &i.value {
                InputValue::Edge(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        if edges.is_empty() {
            return Err(error(
                &format!("{noun}.no-edges"),
                format!("a {noun} needs at least one edge"),
            ));
        }
        let body = match self.blend {
            Blend::Fillet => kernel.fillet(target, &edges, args.param("radius").unwrap_or(0.0))?,
            Blend::Chamfer => {
                kernel.chamfer(target, &edges, args.param("distance").unwrap_or(0.0))?
            }
        };
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(body),
            }],
            tool: None,
        }
        .into())
    }
}
