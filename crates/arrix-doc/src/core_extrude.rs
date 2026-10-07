//! `core.extrude` (docs/DATA-MODEL.md §The core feature types): a sketch
//! region extruded a signed `distance` into a new body, or joined to or cut
//! from the body its `target` input names (ADR-0007).

use arrix_core::{Diagnostic, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    Body, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec, Tool, ToolOp,
};

use crate::core_choice::{choice, shown_when};
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

/// Which type is placing its sweep: its code prefix and what its
/// messages call it.
#[derive(Clone, Copy)]
pub(crate) struct Sweep {
    pub ty: &'static str,
    pub what: &'static str,
}

/// A swept solid by a `mode` choice, `new`, `join` or `cut`, with the
/// body the type's `target` input names (`None` when unset): `make` builds
/// the solid, which `new` returns as it is and `join` and `cut` fuse into
/// or cut from the target. A join or a cut also returns its tool, kept for
/// a pattern or a mirror to copy (ADR-0013). Shared by `core.extrude` and
/// `core.revolve`, whose codes differ by `ty`.
pub(crate) fn place(
    kernel: &mut dyn Kernel,
    Sweep { ty, what }: Sweep,
    mode: &str,
    target: Option<&Body>,
    make: impl FnOnce(&mut dyn Kernel) -> Result<Body, Diagnostic>,
) -> Result<(Body, Option<Tool>), Diagnostic> {
    match (mode, target) {
        ("new", None) => Ok((make(kernel)?, None)),
        ("new", Some(_)) => Err(error(
            &format!("{ty}.stray-target"),
            "a new body has no target to join to or cut; clear the target or choose join or cut",
        )),
        ("join" | "cut", None) => Err(error(
            &format!("{ty}.no-target"),
            format!("{what} in {mode} mode needs the body to {mode}"),
        )),
        ("join", Some(target)) => {
            let tool = make(kernel)?;
            let joined = kernel.fuse(target, &tool)?;
            let op = ToolOp::Fuse;
            Ok((joined, Some(Tool { body: tool, op })))
        }
        ("cut", Some(target)) => {
            let tool = make(kernel)?;
            let cut = kernel.cut(target, &tool)?;
            let op = ToolOp::Cut;
            Ok((cut, Some(Tool { body: tool, op })))
        }
        (other, _) => Err(error(
            "feature.choice",
            format!("`{other}` is not a mode of {what}: new, join or cut"),
        )),
    }
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
                    shown_when: None,
                }],
                inputs: vec![
                    InputSpec {
                        name: "region".into(),
                        title: "Sketch region".into(),
                        kind: InputKind::Region,
                        list: false,
                        shown_when: None,
                    },
                    InputSpec {
                        name: "target".into(),
                        title: "Body to join to or cut".into(),
                        kind: InputKind::Body,
                        list: false,
                        shown_when: shown_when("mode", &["join", "cut"]),
                    },
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
        let (body, kept) = place(
            kernel,
            Sweep {
                ty: "extrude",
                what: "an extrude",
            },
            mode,
            target,
            |k| k.extrude(profile, distance),
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
