//! `core.boolean` (docs/DATA-MODEL.md §The core feature types): one
//! `target` body and a list of `tools` bodies, fused, cut or intersected by
//! the `op` choice, one kernel call per tool. `target` is modified, each
//! tool consumed (ADR-0007).

use arrix_core::{Diagnostic, Severity, SlotName};
use arrix_plugin_api::{
    Body, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    SlotKind, SlotOutput, SlotSpec,
};

use crate::core_choice::choice;
use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// `core.boolean`: `op` is `fuse` (the default), `cut` or `common`. Its one
/// output slot, `body`, is the target slot's next version; each tool's slot
/// ends here, since a body input nothing modifies is a tool. Tools are
/// applied in the list's order, each a boolean of the feature's own number
/// (ADR-0014). Version 2: version 1 had one `tool` input.
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
                version: 2,
                title: "Boolean".into(),
                params: Vec::new(),
                inputs: vec![
                    InputSpec {
                        name: "target".into(),
                        title: "Body to modify".into(),
                        kind: InputKind::Body,
                        list: false,
                        shown_when: None,
                    },
                    InputSpec {
                        name: "tools".into(),
                        title: "Bodies to fuse in, cut away or intersect with".into(),
                        kind: InputKind::Body,
                        list: true,
                        shown_when: None,
                    },
                ],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: Some("target".into()),
                }],
                choices: vec![choice(
                    "op",
                    "Operation",
                    &[("fuse", "Fuse"), ("cut", "Cut"), ("common", "Intersect")],
                    Some("fuse"),
                )],
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
        let Some(InputValue::Body(target)) = args.input("target").map(|i| &i.value) else {
            return Err(error(
                "boolean.no-body",
                "a boolean needs a body as its target",
            ));
        };
        let tools = args
            .inputs_named("tools")
            .map(|i| match &i.value {
                InputValue::Body(b) => Ok(b),
                _ => Err(error("boolean.no-body", "a boolean's tools are bodies")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if tools.is_empty() {
            return Err(error(
                "boolean.no-body",
                "a boolean needs at least one body as a tool",
            ));
        }
        let apply: fn(&mut dyn Kernel, &Body, &Body) -> Result<Body, Diagnostic> =
            match args.choices.get("op").map_or("fuse", String::as_str) {
                "fuse" => |k, a, b| k.fuse(a, b),
                "cut" => |k, a, b| k.cut(a, b),
                "common" => |k, a, b| k.common(a, b),
                other => {
                    return Err(error(
                        "feature.choice",
                        format!("`{other}` is not a boolean operation: fuse, cut or common"),
                    ));
                }
            };
        let mut result = apply(kernel, target, tools[0])?;
        for tool in &tools[1..] {
            result = apply(kernel, &result, tool)?;
        }
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(result),
            }],
            tool: None,
        }
        .into())
    }
}
