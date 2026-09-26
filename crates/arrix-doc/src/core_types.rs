//! The built-in feature types (docs/DATA-MODEL.md §The core feature
//! types), registered under `core.` through the same `FeatureType` shape a
//! plugin's feature is adapted onto.

use arrix_core::{Diagnostic, Frame, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};

use crate::registry::{FeatureArgs, FeatureType};

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

    fn base(args: &FeatureArgs) -> Result<Frame, Diagnostic> {
        if let Some(other) = args.choices.keys().find(|k| *k != "world") {
            return Err(error(
                "feature.unknown-choice",
                format!("a datum plane has no choice `{other}`"),
            ));
        }
        let world = args.choices.get("world");
        match (args.input("plane"), world) {
            (Some(_), Some(_)) => Err(error(
                "datum-plane.two-bases",
                "a datum plane stands on its input plane or on a world plane, not both",
            )),
            (Some(input), None) => match input.value {
                InputValue::Plane(frame) => Ok(frame),
            },
            (None, world) => {
                let word = world.map_or("xy", String::as_str);
                WORLD_PLANES
                    .iter()
                    .find(|(w, _)| *w == word)
                    .map(|(_, f)| *f)
                    .ok_or_else(|| {
                        error(
                            "datum-plane.world",
                            format!("`{word}` is not a world plane: xy, yz or zx"),
                        )
                    })
            }
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
    ) -> Result<FeatureOutput, Diagnostic> {
        let offset = args.param("offset").unwrap_or(0.0);
        let plane = Self::base(args)?.offset(offset);
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Plane(plane),
            }],
        })
    }
}
