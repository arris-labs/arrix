//! Five feature types with no inputs and no parameters: `fine` extrudes a
//! square, and the others fail in the way their name says. The host's
//! sandbox has to turn each into that feature's diagnostic.

use arrix_plugin_api::{
    Body, CurveKey, DVec2, Diagnostic, Feature, FeatureOutput, FeatureTypeSpec, Frame, Id, Kernel,
    OutputValue, ParamValue, Profile, ProfileLoop, ProfileSegment, ChoiceValue, ResolvedInput,
    Severity, SlotKind, SlotName, SlotOutput, SlotSpec,
};

const TYPES: [&str; 5] = [
    "misbehave.fine",
    "misbehave.loop",
    "misbehave.panic",
    "misbehave.memory",
    "misbehave.kernel",
];

pub struct Misbehave;

fn body_slot() -> SlotName {
    SlotName::new("body").expect("a literal slot name")
}

/// A 10 mm square on the world's XY plane, extruded 5 mm.
fn extrude(kernel: &mut dyn Kernel) -> Result<Body, Diagnostic> {
    let corner = |k: u64, x: f64, y: f64| ProfileSegment::Line {
        key: CurveKey(Id(k)),
        to: DVec2::new(x, y),
    };
    let outer = ProfileLoop::Path {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            corner(1, 0.01, 0.0),
            corner(2, 0.01, 0.01),
            corner(3, 0.0, 0.01),
            corner(4, 0.0, 0.0),
        ],
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).expect("a square is a profile");
    kernel.extrude(&profile, 0.005)
}

impl Feature for Misbehave {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        TYPES
            .iter()
            .map(|id| FeatureTypeSpec {
                id: (*id).into(),
                version: 1,
                title: (*id).into(),
                params: vec![],
                choices: vec![],
                inputs: vec![],
                outputs: vec![SlotSpec {
                    name: body_slot(),
                    kind: SlotKind::Body,
                    modifies: None,
                }],
            })
            .collect()
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        type_id: &str,
        _params: &[ParamValue],
        _choices: &[ChoiceValue],
        _inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic> {
        match type_id {
            // `kernel` is the same call under a manifest that refuses it.
            "misbehave.fine" | "misbehave.kernel" => Ok(FeatureOutput {
                slots: vec![SlotOutput {
                    name: body_slot(),
                    value: OutputValue::Body(extrude(kernel)?),
                }],
                tool: None,
            }),
            "misbehave.loop" => {
                let mut n = 0u64;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            "misbehave.panic" => panic!("misbehave.panic was asked to panic"),
            "misbehave.memory" => {
                let mut held = Vec::new();
                loop {
                    // Reserved and never written, so the memory runs out long before
                    // the fuel does.
                    held.push(Vec::<u8>::with_capacity(1 << 20));
                    std::hint::black_box(&held);
                }
            }
            _ => Err(Diagnostic::new(
                Severity::Error,
                "misbehave.unknown-type",
                format!("misbehave has no feature type {type_id}"),
            )
            .expect("a literal code is well-formed")),
        }
    }
}

#[cfg(all(feature = "guest", target_arch = "wasm32"))]
arrix_plugin_api::export_plugin!(Misbehave);
