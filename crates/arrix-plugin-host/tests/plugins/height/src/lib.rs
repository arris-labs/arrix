//! One feature type, `height.block`: a 10 mm square extruded 5 mm, or 12.5
//! mm in a build with the `tall` feature. The same plugin before and after
//! a rebuild, for a reload to tell apart.

use arrix_plugin_api::{
    Body, ChoiceValue, CurveKey, DVec2, Diagnostic, Feature, FeatureOutput, FeatureTypeSpec,
    Frame, Id, Kernel, OutputValue, ParamValue, Profile, ProfileLoop, ProfileSegment,
    ResolvedInput, SlotKind, SlotName, SlotOutput, SlotSpec,
};

pub struct Height;

#[cfg(feature = "tall")]
const HEIGHT: f64 = 0.0125;
#[cfg(not(feature = "tall"))]
const HEIGHT: f64 = 0.005;

fn body_slot() -> SlotName {
    SlotName::new("body").expect("a literal slot name")
}

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
    kernel.extrude(&profile, HEIGHT)
}

impl Feature for Height {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        vec![FeatureTypeSpec {
            id: "height.block".into(),
            version: 1,
            title: "Block".into(),
            params: vec![],
            choices: vec![],
            inputs: vec![],
            outputs: vec![SlotSpec {
                name: body_slot(),
                kind: SlotKind::Body,
                modifies: None,
            }],
        }]
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        _type_id: &str,
        _params: &[ParamValue],
        _choices: &[ChoiceValue],
        _inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic> {
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: body_slot(),
                value: OutputValue::Body(extrude(kernel)?),
            }],
            tool: None,
        })
    }
}

#[cfg(all(feature = "guest", target_arch = "wasm32"))]
arrix_plugin_api::export_plugin!(Height);
