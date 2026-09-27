//! `gears.spur`: a spur gear of `teeth` involute teeth of `module`, with a
//! `pressure_angle`, extruded `width` along the normal of its `plane`.
//! Its axis is the plane's normal through the plane's origin, and tooth 1
//! is centred on the plane's X axis. No bore in 0.1: a hole is a later
//! feature's.

use arrix_plugin_api::{
    Diagnostic, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel,
    OutputValue, ParamSpec, ParamValue, Profile, QuantityKind, ResolvedInput, Severity, SlotKind,
    SlotName, SlotOutput, SlotSpec,
};

use crate::involute::{OutlineError, Spur};

pub const TYPE_ID: &str = "gears.spur";

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

fn body_slot() -> SlotName {
    SlotName::new("body").expect("a literal slot name")
}

pub fn spec() -> FeatureTypeSpec {
    let param = |name: &str, title: &str, kind, default: &str| ParamSpec {
        name: name.into(),
        title: title.into(),
        kind,
        default: default.into(),
    };
    FeatureTypeSpec {
        id: TYPE_ID.into(),
        version: 1,
        title: "Spur gear".into(),
        params: vec![
            param("teeth", "Teeth", QuantityKind::Count, "20"),
            param("module", "Module", QuantityKind::Length, "1 mm"),
            param("width", "Face width", QuantityKind::Length, "8 mm"),
            param(
                "pressure_angle",
                "Pressure angle",
                QuantityKind::Angle,
                "20 deg",
            ),
        ],
        inputs: vec![InputSpec {
            name: "plane".into(),
            title: "Plane".into(),
            kind: InputKind::Plane,
        }],
        outputs: vec![SlotSpec {
            name: body_slot(),
            kind: SlotKind::Body,
            modifies: None,
        }],
    }
}

fn param(params: &[ParamValue], name: &str) -> Result<f64, Diagnostic> {
    params
        .iter()
        .find(|p| p.name == name)
        .map(|p| p.value.si)
        .ok_or_else(|| error("gears.param", format!("no value for `{name}`")))
}

/// Teeth from 6, a positive module and width, a pressure angle strictly
/// between 0 and 45°.
fn gear(params: &[ParamValue]) -> Result<(Spur, f64), Diagnostic> {
    let teeth = param(params, "teeth")?;
    let module = param(params, "module")?;
    let width = param(params, "width")?;
    let angle = param(params, "pressure_angle")?;
    if !(6.0..=f64::from(u32::MAX)).contains(&teeth) {
        return Err(error(
            "gears.teeth",
            format!("a spur gear has 6 teeth or more, not {teeth}"),
        ));
    }
    if module <= 0.0 {
        return Err(error("gears.module", "the module must be positive"));
    }
    if width <= 0.0 {
        return Err(error("gears.width", "the face width must be positive"));
    }
    if !(angle > 0.0 && angle < std::f64::consts::FRAC_PI_4) {
        return Err(error(
            "gears.pressure-angle",
            "the pressure angle must be between 0 and 45 deg",
        ));
    }
    let spur = Spur {
        teeth: teeth as u32,
        module,
        pressure_angle: angle,
    };
    Ok((spur, width))
}

pub fn evaluate(
    kernel: &mut dyn Kernel,
    params: &[ParamValue],
    inputs: &[ResolvedInput],
) -> Result<FeatureOutput, Diagnostic> {
    let (spur, width) = gear(params)?;
    let plane = match inputs.iter().find(|i| i.name == "plane") {
        Some(ResolvedInput {
            value: InputValue::Plane(f),
            ..
        }) => *f,
        // The host resolves the input as the spec's `plane`; a region is
        // never handed over, but the API has the word.
        _ => return Err(error("gears.plane", "a spur gear needs a plane")),
    };
    let outline = spur.outline().map_err(|e| match e {
        OutlineError::Pointed => error(
            "gears.pointed",
            "the teeth come to a point inside the tip circle: more teeth or a smaller \
             pressure angle",
        ),
        OutlineError::NoGap => error("gears.no-gap", "neighbouring teeth touch at the root"),
        OutlineError::Tolerance => error(
            "gears.tolerance",
            "the involute flanks cannot be fitted with arcs within the tolerance",
        ),
    })?;
    let profile =
        Profile::new(plane, outline, vec![]).map_err(|e| error("gears.profile", e.to_string()))?;
    let body = kernel.extrude(&profile, width)?;
    Ok(FeatureOutput {
        slots: vec![SlotOutput {
            name: body_slot(),
            value: OutputValue::Body(body),
        }],
    })
}
