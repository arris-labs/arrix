//! `core.boolean` in its M1 form (docs/DATA-MODEL.md §The core feature
//! types): one `target` body and one `tool` body, fused or cut by the `op`
//! choice. `target` is modified, `tool` consumed (ADR-0007).

use arrix_core::{Diagnostic, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    SlotKind, SlotOutput, SlotSpec,
};

use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// `core.boolean`: `op` is `fuse` (the default) or `cut`. Its one output
/// slot, `body`, is the target slot's next version; the tool's slot ends
/// here, since a body input nothing modifies is a tool.
pub struct CoreBoolean {
    spec: FeatureTypeSpec,
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl CoreBoolean {
    pub fn new() -> Self {
        Self {
            spec: FeatureTypeSpec {
                id: "core.boolean".into(),
                version: 1,
                title: "Boolean".into(),
                params: Vec::new(),
                inputs: vec![
                    InputSpec {
                        name: "target".into(),
                        title: "Body to modify".into(),
                        kind: InputKind::Body,
                    },
                    InputSpec {
                        name: "tool".into(),
                        title: "Body to fuse in or cut away".into(),
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

impl Default for CoreBoolean {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for CoreBoolean {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        if let Some(other) = args.choices.keys().find(|k| *k != "op") {
            return Err(error(
                "feature.unknown-choice",
                format!("a boolean has no choice `{other}`"),
            ));
        }
        let body = |name: &str| match args.input(name).map(|i| &i.value) {
            Some(InputValue::Body(b)) => Ok(b),
            _ => Err(error(
                "boolean.no-body",
                format!("a boolean needs a body as its {name}"),
            )),
        };
        let (target, tool) = (body("target")?, body("tool")?);
        let result = match args.choices.get("op").map_or("fuse", String::as_str) {
            "fuse" => kernel.fuse(target, tool)?,
            "cut" => kernel.cut(target, tool)?,
            other => {
                return Err(error(
                    "boolean.op",
                    format!("`{other}` is not a boolean operation: fuse or cut"),
                ));
            }
        };
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(result),
            }],
        }
        .into())
    }
}
