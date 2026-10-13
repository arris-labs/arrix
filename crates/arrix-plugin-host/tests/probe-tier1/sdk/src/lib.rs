//! The probe's guest SDK (plans/c3-m1 step 2): the world's `feature` export
//! over a Rust `Feature`, and the Rust `Kernel` over the world's `kernel`
//! import. Enough of the adapter for `gears`: `describe`, and `evaluate`
//! with plane inputs and a body output through `extrude`. Step 4 moves the
//! whole adapter of `arrix-plugin-api/tests/wit_equality.rs` into the API.

use std::marker::PhantomData;

use arrix_plugin_api as api;
use arrix_plugin_api::{DVec2, DVec3, Diagnostic, Frame, Profile, ProfileLoop, ProfileSegment};

pub mod bindings {
    wit_bindgen::generate!({
        path: "../../../../arrix-plugin-api/wit",
        world: "plugin",
        pub_export_macro: true,
        export_macro_name: "__probe_export_world",
    });
}

use bindings::arrix::plugin::kernel as wk;
use bindings::arrix::plugin::types as wt;
use bindings::exports::arrix::plugin::feature as wf;

/// What `export_plugin!` names: the plugin's `Feature`.
pub trait Plugin {
    fn feature() -> &'static dyn api::Feature;
}

/// The world's `feature` export over a [`Plugin`].
pub struct Exported<P>(PhantomData<P>);

/// Exports `$plugin`, a unit struct implementing `Feature`, as the world's
/// `feature` interface.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:path) => {
        struct __ArrixPlugin;
        impl $crate::Plugin for __ArrixPlugin {
            fn feature() -> &'static dyn $crate::__api::Feature {
                &$plugin
            }
        }
        type __ArrixExported = $crate::Exported<__ArrixPlugin>;
        $crate::bindings::__probe_export_world!(__ArrixExported with_types_in $crate::bindings);
    };
}

#[doc(hidden)]
pub use arrix_plugin_api as __api;

fn vec2(v: DVec2) -> wt::Vec2 {
    wt::Vec2 { x: v.x, y: v.y }
}

fn vec3(v: DVec3) -> wt::Vec3 {
    wt::Vec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

fn frame(f: &Frame) -> wt::Frame {
    wt::Frame {
        origin: vec3(f.origin()),
        x_axis: vec3(f.x_axis()),
        z_axis: vec3(f.z_axis()),
    }
}

fn frame_from(w: wt::Frame) -> Frame {
    let v = |v: wt::Vec3| DVec3::new(v.x, v.y, v.z);
    Frame::new(v(w.origin), v(w.x_axis), v(w.z_axis)).expect("the host's frames are valid")
}

fn kind(k: api::QuantityKind) -> wt::QuantityKind {
    match k {
        api::QuantityKind::Length => wt::QuantityKind::Length,
        api::QuantityKind::Angle => wt::QuantityKind::Angle,
        api::QuantityKind::Count => wt::QuantityKind::Count,
        api::QuantityKind::Ratio => wt::QuantityKind::Ratio,
        api::QuantityKind::Mass => wt::QuantityKind::Mass,
        api::QuantityKind::Density => wt::QuantityKind::Density,
    }
}

fn kind_from(k: wt::QuantityKind) -> api::QuantityKind {
    match k {
        wt::QuantityKind::Length => api::QuantityKind::Length,
        wt::QuantityKind::Angle => api::QuantityKind::Angle,
        wt::QuantityKind::Count => api::QuantityKind::Count,
        wt::QuantityKind::Ratio => api::QuantityKind::Ratio,
        wt::QuantityKind::Mass => api::QuantityKind::Mass,
        wt::QuantityKind::Density => api::QuantityKind::Density,
    }
}

fn shown_when(s: &Option<api::ShownWhen>) -> Option<wf::ShownWhen> {
    s.as_ref().map(|s| wf::ShownWhen {
        choice: s.choice.clone(),
        values: s.values.clone(),
    })
}

fn spec(s: api::FeatureTypeSpec) -> wf::FeatureTypeSpec {
    wf::FeatureTypeSpec {
        id: s.id,
        version: s.version,
        title: s.title,
        params: s
            .params
            .iter()
            .map(|p| wf::ParamSpec {
                name: p.name.clone(),
                title: p.title.clone(),
                kind: kind(p.kind),
                default: p.default.clone(),
                shown_when: shown_when(&p.shown_when),
            })
            .collect(),
        choices: s
            .choices
            .iter()
            .map(|c| wf::ChoiceSpec {
                name: c.name.clone(),
                title: c.title.clone(),
                options: c
                    .options
                    .iter()
                    .map(|o| wf::ChoiceOption {
                        name: o.name.clone(),
                        title: o.title.clone(),
                    })
                    .collect(),
                default: c.default.clone(),
            })
            .collect(),
        inputs: s
            .inputs
            .iter()
            .map(|i| wf::InputSpec {
                name: i.name.clone(),
                title: i.title.clone(),
                kind: match i.kind {
                    api::InputKind::Plane => wf::InputKind::Plane,
                    api::InputKind::Region => wf::InputKind::Region,
                    api::InputKind::Body => wf::InputKind::Body,
                    api::InputKind::Edge => wf::InputKind::Edge,
                    api::InputKind::Point => wf::InputKind::Point,
                    api::InputKind::Axis => wf::InputKind::Axis,
                    api::InputKind::Tool => wf::InputKind::Tool,
                },
                list: i.list,
                shown_when: shown_when(&i.shown_when),
            })
            .collect(),
        outputs: s
            .outputs
            .iter()
            .map(|o| wf::SlotSpec {
                name: o.name.to_string(),
                kind: match o.kind {
                    api::SlotKind::Body => wf::SlotKind::Body,
                    api::SlotKind::Plane => wf::SlotKind::Plane,
                },
                modifies: o.modifies.clone(),
            })
            .collect(),
    }
}

/// The probe carries a diagnostic's severity, code and message; `gears`
/// gives no references.
fn diagnostic(d: &Diagnostic) -> wt::Diagnostic {
    wt::Diagnostic {
        severity: match d.severity {
            api::Severity::Info => wt::Severity::Info,
            api::Severity::Warning => wt::Severity::Warning,
            api::Severity::Error => wt::Severity::Error,
        },
        code: d.code.to_string(),
        message: d.message.clone(),
        refs: vec![],
        candidates: vec![],
    }
}

fn diagnostic_from(d: wt::Diagnostic) -> Diagnostic {
    let severity = match d.severity {
        wt::Severity::Info => api::Severity::Info,
        wt::Severity::Warning => api::Severity::Warning,
        wt::Severity::Error => api::Severity::Error,
    };
    Diagnostic::new(severity, &d.code, d.message).expect("the host's codes are well-formed")
}

fn profile_loop(l: &ProfileLoop) -> wt::ProfileLoop {
    match l {
        ProfileLoop::Circle {
            key,
            center,
            radius,
        } => wt::ProfileLoop::Circle(wt::CircleLoop {
            key: key.0.0,
            center: vec2(*center),
            radius: *radius,
        }),
        ProfileLoop::Path { start, segments } => wt::ProfileLoop::Path(wt::PathLoop {
            start: vec2(*start),
            segments: segments
                .iter()
                .map(|s| match *s {
                    ProfileSegment::Line { key, to } => wt::ProfileSegment::Line(wt::LineSegment {
                        key: key.0.0,
                        to: vec2(to),
                    }),
                    ProfileSegment::Arc { key, to, via } => {
                        wt::ProfileSegment::Arc(wt::ArcSegment {
                            key: key.0.0,
                            to: vec2(to),
                            via: vec2(via),
                        })
                    }
                })
                .collect(),
        }),
    }
}

fn profile(p: &Profile) -> wt::Profile {
    wt::Profile {
        plane: frame(p.plane()),
        outer: profile_loop(p.outer()),
        holes: p.holes().iter().map(profile_loop).collect(),
    }
}

/// The Rust `Kernel` over the world's import; bodies in a table by index.
#[derive(Default)]
struct Kernel {
    bodies: Vec<Option<wk::Body>>,
}

impl Kernel {
    fn put(&mut self, body: wk::Body) -> api::Body {
        self.bodies.push(Some(body));
        api::Body::from_handle(u32::try_from(self.bodies.len() - 1).expect("few bodies"))
    }

    fn take(&mut self, body: &api::Body) -> wk::Body {
        self.bodies[body.handle() as usize]
            .take()
            .expect("a body is given up once")
    }
}

fn unprobed() -> Diagnostic {
    Diagnostic::new(
        api::Severity::Error,
        "probe.unprobed",
        "the probe's adapter has extrude only",
    )
    .expect("a literal code")
}

impl api::Kernel for Kernel {
    fn extrude(&mut self, p: &Profile, distance: f64) -> Result<api::Body, Diagnostic> {
        match wk::extrude(&profile(p), distance) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(diagnostic_from(d)),
        }
    }

    fn revolve(&mut self, _: &Profile, _: &api::Axis, _: f64) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn names(&mut self, _: &api::Body) -> Result<Vec<api::PersistentName>, Diagnostic> {
        Err(unprobed())
    }

    fn face_frame(&mut self, _: &api::Body, _: &api::PersistentName) -> Result<Frame, Diagnostic> {
        Err(unprobed())
    }

    fn measure(&mut self, _: &api::Body) -> Result<api::MassProperties, Diagnostic> {
        Err(unprobed())
    }

    fn fuse(&mut self, _: &api::Body, _: &api::Body) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn cut(&mut self, _: &api::Body, _: &api::Body) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn common(&mut self, _: &api::Body, _: &api::Body) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn fillet(
        &mut self,
        _: &api::Body,
        _: &[api::PersistentName],
        _: f64,
    ) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn chamfer(
        &mut self,
        _: &api::Body,
        _: &[api::PersistentName],
        _: f64,
    ) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn copy(&mut self, _: u32, _: &api::Body, _: &api::Motion) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }

    fn mirror(&mut self, _: &api::Body, _: DVec3, _: DVec3) -> Result<api::Body, Diagnostic> {
        Err(unprobed())
    }
}

impl<P: Plugin> wf::Guest for Exported<P> {
    fn describe() -> Vec<wf::FeatureTypeSpec> {
        P::feature().describe().into_iter().map(spec).collect()
    }

    fn evaluate(
        type_id: String,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
        inputs: Vec<wf::ResolvedInput>,
    ) -> Result<wf::FeatureOutput, wt::Diagnostic> {
        let params: Vec<_> = params
            .into_iter()
            .map(|p| api::ParamValue {
                name: p.name,
                value: api::Quantity::si(kind_from(p.value.kind), p.value.si),
            })
            .collect();
        let choices: Vec<_> = choices
            .into_iter()
            .map(|c| api::ChoiceValue {
                name: c.name,
                value: c.value,
            })
            .collect();
        let mut kernel = Kernel::default();
        let mut resolved = Vec::new();
        for i in inputs {
            let value = match i.value {
                wf::InputValue::Plane(f) => api::InputValue::Plane(frame_from(f)),
                wf::InputValue::Body(b) => api::InputValue::Body(kernel.put(b)),
                _ => return Err(diagnostic(&unprobed())),
            };
            resolved.push(api::ResolvedInput {
                name: i.name,
                value,
            });
        }
        let out = P::feature()
            .evaluate(&mut kernel, &type_id, &params, &choices, &resolved)
            .map_err(|d| diagnostic(&d))?;
        let mut slots = Vec::new();
        for s in out.slots {
            let value = match s.value {
                api::OutputValue::Body(b) => wf::OutputValue::Body(kernel.take(&b)),
                api::OutputValue::Plane(f) => wf::OutputValue::Plane(frame(&f)),
            };
            slots.push(wf::SlotOutput {
                name: s.name.to_string(),
                value,
            });
        }
        if out.tool.is_some() {
            return Err(diagnostic(&unprobed()));
        }
        Ok(wf::FeatureOutput { slots, tool: None })
    }
}
