//! `core.mirror` (docs/DATA-MODEL.md §The core feature types): the tools of
//! features that join or cut, applied to a `target` body with each
//! source's own operation, or, with no tools, the target reflected and
//! fused into itself, about a `plane` input (ADR-0013). The reflection is
//! instance 1 of the feature.

use arrix_core::{Diagnostic, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    SlotKind, SlotOutput, SlotSpec,
};

use crate::core_pattern::{apply, sources};
use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

/// `core.mirror`.
pub struct CoreMirror {
    spec: FeatureTypeSpec,
}

impl CoreMirror {
    pub fn new() -> Self {
        let input = |name: &str, title: &str, kind, list| InputSpec {
            name: name.into(),
            title: title.into(),
            kind,
            list,
            shown_when: None,
        };
        Self {
            spec: FeatureTypeSpec {
                id: "core.mirror".into(),
                version: 1,
                title: "Mirror".into(),
                params: vec![],
                inputs: vec![
                    input("target", "Body to mirror onto", InputKind::Body, false),
                    input("tools", "Features to mirror", InputKind::Tool, true),
                    input("plane", "Mirror plane", InputKind::Plane, false),
                ],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: Some("target".into()),
                }],
                choices: vec![],
            },
        }
    }
}

impl Default for CoreMirror {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for CoreMirror {
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
                "mirror.no-target",
                "a mirror needs the body to mirror onto",
            ));
        };
        let Some(InputValue::Plane(plane)) = args.input("plane").map(|i| &i.value) else {
            return Err(error(
                "mirror.no-plane",
                "a mirror needs a plane: a datum plane, a sketch plane or a planar face",
            ));
        };
        let (origin, normal) = (plane.origin(), plane.z_axis());
        let sources = sources(args, target);
        let body = apply(kernel, "mirror", target, &sources, 1, &|k, _, source| {
            k.mirror(source, origin, normal)
        })?;
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
