//! The WIT world and the Rust traits held equal (docs/PLUGINS.md §One
//! interface; the C1 risk "the WIT world and the Tier 0 traits drift apart
//! before C3 builds Tier 1", docs/ROADMAP.md).
//!
//! Three holds, each catching what the others cannot:
//!
//! 1. **Types.** Every type of the world converts to its Rust type and
//!    back, with exhaustive destructuring and matches on both sides, so a
//!    field or case added to either fails to compile; every case round
//!    trips at run time.
//! 2. **Functions.** `feature`'s `Guest` is implemented by forwarding to a
//!    Rust `Feature`, and a Rust `Feature` by forwarding to the `Guest`;
//!    the Rust `Kernel` is implemented over the world's imports. A method
//!    or function added on either side fails to compile.
//! 3. **Inventory.** The world as parsed is exactly the types and functions
//!    this file covers, so a WIT addition the bindings would silently
//!    absorb (an unused record, a new import) fails here.

use std::collections::BTreeSet;
use std::path::Path;

use arrix_plugin_api as api;
use arrix_plugin_api::{
    Body, CurveKey, DVec2, DVec3, Diagnostic, FeatureId, Frame, Id, ParamId, PersistentName,
    PluginId, Profile, ProfileLoop, ProfileSegment, Quantity, QuantityKind, RecordId, Ref,
    Severity, SketchEntityId, SlotName,
};

mod wit {
    wit_bindgen::generate!({ path: "wit", world: "plugin" });

    pub use arrix::plugin::{kernel, types};
    pub use exports::arrix::plugin::feature;
}

use wit::feature as wf;
use wit::types as wt;

/// A Rust type of the API and its type in the world. `from_wit` panics on
/// a value the Rust type refuses: the WIT form of a valid value is valid,
/// and that is what the round trips check.
trait Mirror: Sized {
    type Wit;
    fn to_wit(&self) -> Self::Wit;
    fn from_wit(w: Self::Wit) -> Self;
}

impl<T: Mirror> Mirror for Vec<T> {
    type Wit = Vec<T::Wit>;
    fn to_wit(&self) -> Self::Wit {
        self.iter().map(T::to_wit).collect()
    }
    fn from_wit(w: Self::Wit) -> Self {
        w.into_iter().map(T::from_wit).collect()
    }
}

fn round_trip<T: Mirror + PartialEq + std::fmt::Debug>(v: &T) {
    assert_eq!(&T::from_wit(v.to_wit()), v);
}

// ---- types ----------------------------------------------------------------

impl Mirror for DVec2 {
    type Wit = wt::Vec2;
    fn to_wit(&self) -> wt::Vec2 {
        let DVec2 { x, y } = *self;
        wt::Vec2 { x, y }
    }
    fn from_wit(w: wt::Vec2) -> Self {
        let wt::Vec2 { x, y } = w;
        DVec2 { x, y }
    }
}

impl Mirror for DVec3 {
    type Wit = wt::Vec3;
    fn to_wit(&self) -> wt::Vec3 {
        let DVec3 { x, y, z } = *self;
        wt::Vec3 { x, y, z }
    }
    fn from_wit(w: wt::Vec3) -> Self {
        let wt::Vec3 { x, y, z } = w;
        DVec3 { x, y, z }
    }
}

impl Mirror for Frame {
    type Wit = wt::Frame;
    fn to_wit(&self) -> wt::Frame {
        wt::Frame {
            origin: self.origin().to_wit(),
            x_axis: self.x_axis().to_wit(),
            z_axis: self.z_axis().to_wit(),
        }
    }
    fn from_wit(w: wt::Frame) -> Self {
        let wt::Frame {
            origin,
            x_axis,
            z_axis,
        } = w;
        Frame::new(
            DVec3::from_wit(origin),
            DVec3::from_wit(x_axis),
            DVec3::from_wit(z_axis),
        )
        .unwrap()
    }
}

impl Mirror for QuantityKind {
    type Wit = wt::QuantityKind;
    fn to_wit(&self) -> wt::QuantityKind {
        match self {
            QuantityKind::Length => wt::QuantityKind::Length,
            QuantityKind::Angle => wt::QuantityKind::Angle,
            QuantityKind::Count => wt::QuantityKind::Count,
            QuantityKind::Ratio => wt::QuantityKind::Ratio,
            QuantityKind::Mass => wt::QuantityKind::Mass,
        }
    }
    fn from_wit(w: wt::QuantityKind) -> Self {
        match w {
            wt::QuantityKind::Length => QuantityKind::Length,
            wt::QuantityKind::Angle => QuantityKind::Angle,
            wt::QuantityKind::Count => QuantityKind::Count,
            wt::QuantityKind::Ratio => QuantityKind::Ratio,
            wt::QuantityKind::Mass => QuantityKind::Mass,
        }
    }
}

impl Mirror for Quantity {
    type Wit = wt::Quantity;
    fn to_wit(&self) -> wt::Quantity {
        let Quantity { kind, si } = *self;
        wt::Quantity {
            kind: kind.to_wit(),
            si,
        }
    }
    fn from_wit(w: wt::Quantity) -> Self {
        let wt::Quantity { kind, si } = w;
        Quantity::si(QuantityKind::from_wit(kind), si)
    }
}

impl Mirror for PersistentName {
    type Wit = wt::PersistentRef;
    fn to_wit(&self) -> wt::PersistentRef {
        wt::PersistentRef {
            encoded: self.to_string(),
        }
    }
    fn from_wit(w: wt::PersistentRef) -> Self {
        let wt::PersistentRef { encoded } = w;
        encoded.parse().unwrap()
    }
}

impl Mirror for Ref {
    type Wit = wt::Reference;
    fn to_wit(&self) -> wt::Reference {
        match self {
            Ref::Param(p) => wt::Reference::Param(p.0.0),
            Ref::Feature(f) => wt::Reference::Feature(f.0.0),
            Ref::Slot { feature, slot } => wt::Reference::Slot(wt::SlotRef {
                feature: feature.0.0,
                slot: slot.to_string(),
            }),
            Ref::Topo(name) => wt::Reference::Topo(name.to_wit()),
            Ref::Sketch { feature, entity } => wt::Reference::Sketch(wt::SketchRef {
                feature: feature.0.0,
                entity: entity.0.0,
            }),
            Ref::Plugin { plugin, record } => wt::Reference::Plugin(wt::PluginRef {
                plugin: plugin.to_string(),
                record: record.0.0,
            }),
        }
    }
    fn from_wit(w: wt::Reference) -> Self {
        match w {
            wt::Reference::Param(p) => Ref::Param(ParamId(Id(p))),
            wt::Reference::Feature(f) => Ref::Feature(FeatureId(Id(f))),
            wt::Reference::Slot(wt::SlotRef { feature, slot }) => Ref::Slot {
                feature: FeatureId(Id(feature)),
                slot: SlotName::new(slot).unwrap(),
            },
            wt::Reference::Topo(name) => Ref::Topo(PersistentName::from_wit(name)),
            wt::Reference::Sketch(wt::SketchRef { feature, entity }) => Ref::Sketch {
                feature: FeatureId(Id(feature)),
                entity: SketchEntityId(Id(entity)),
            },
            wt::Reference::Plugin(wt::PluginRef { plugin, record }) => Ref::Plugin {
                plugin: PluginId::new(plugin).unwrap(),
                record: RecordId(Id(record)),
            },
        }
    }
}

impl Mirror for Severity {
    type Wit = wt::Severity;
    fn to_wit(&self) -> wt::Severity {
        match self {
            Severity::Info => wt::Severity::Info,
            Severity::Warning => wt::Severity::Warning,
            Severity::Error => wt::Severity::Error,
        }
    }
    fn from_wit(w: wt::Severity) -> Self {
        match w {
            wt::Severity::Info => Severity::Info,
            wt::Severity::Warning => Severity::Warning,
            wt::Severity::Error => Severity::Error,
        }
    }
}

impl Mirror for Diagnostic {
    type Wit = wt::Diagnostic;
    fn to_wit(&self) -> wt::Diagnostic {
        let Diagnostic {
            severity,
            code,
            message,
            refs,
            candidates,
        } = self;
        wt::Diagnostic {
            severity: severity.to_wit(),
            code: code.to_string(),
            message: message.clone(),
            refs: refs.to_wit(),
            candidates: candidates.to_wit(),
        }
    }
    fn from_wit(w: wt::Diagnostic) -> Self {
        let wt::Diagnostic {
            severity,
            code,
            message,
            refs,
            candidates,
        } = w;
        Diagnostic::new(Severity::from_wit(severity), &code, message)
            .unwrap()
            .with_refs(Vec::from_wit(refs))
            .with_candidates(Vec::from_wit(candidates))
    }
}

impl Mirror for ProfileSegment {
    type Wit = wt::ProfileSegment;
    fn to_wit(&self) -> wt::ProfileSegment {
        match *self {
            ProfileSegment::Line { key, to } => wt::ProfileSegment::Line(wt::LineSegment {
                key: key.0.0,
                to: to.to_wit(),
            }),
            ProfileSegment::Arc { key, to, via } => wt::ProfileSegment::Arc(wt::ArcSegment {
                key: key.0.0,
                to: to.to_wit(),
                via: via.to_wit(),
            }),
        }
    }
    fn from_wit(w: wt::ProfileSegment) -> Self {
        match w {
            wt::ProfileSegment::Line(wt::LineSegment { key, to }) => ProfileSegment::Line {
                key: CurveKey(Id(key)),
                to: DVec2::from_wit(to),
            },
            wt::ProfileSegment::Arc(wt::ArcSegment { key, to, via }) => ProfileSegment::Arc {
                key: CurveKey(Id(key)),
                to: DVec2::from_wit(to),
                via: DVec2::from_wit(via),
            },
        }
    }
}

impl Mirror for ProfileLoop {
    type Wit = wt::ProfileLoop;
    fn to_wit(&self) -> wt::ProfileLoop {
        match self {
            ProfileLoop::Circle {
                key,
                center,
                radius,
            } => wt::ProfileLoop::Circle(wt::CircleLoop {
                key: key.0.0,
                center: center.to_wit(),
                radius: *radius,
            }),
            ProfileLoop::Path { start, segments } => wt::ProfileLoop::Path(wt::PathLoop {
                start: start.to_wit(),
                segments: segments.to_wit(),
            }),
        }
    }
    fn from_wit(w: wt::ProfileLoop) -> Self {
        match w {
            wt::ProfileLoop::Circle(wt::CircleLoop {
                key,
                center,
                radius,
            }) => ProfileLoop::Circle {
                key: CurveKey(Id(key)),
                center: DVec2::from_wit(center),
                radius,
            },
            wt::ProfileLoop::Path(wt::PathLoop { start, segments }) => ProfileLoop::Path {
                start: DVec2::from_wit(start),
                segments: Vec::from_wit(segments),
            },
        }
    }
}

impl Mirror for Profile {
    type Wit = wt::Profile;
    fn to_wit(&self) -> wt::Profile {
        wt::Profile {
            plane: self.plane().to_wit(),
            outer: self.outer().to_wit(),
            holes: self.holes().to_vec().to_wit(),
        }
    }
    fn from_wit(w: wt::Profile) -> Self {
        let wt::Profile {
            plane,
            outer,
            holes,
        } = w;
        Profile::new(
            Frame::from_wit(plane),
            ProfileLoop::from_wit(outer),
            Vec::from_wit(holes),
        )
        .unwrap()
    }
}

impl Mirror for api::MassProperties {
    type Wit = wt::MassProperties;
    fn to_wit(&self) -> wt::MassProperties {
        let api::MassProperties {
            volume,
            area,
            centroid,
        } = *self;
        wt::MassProperties {
            volume,
            area,
            centroid: centroid.to_wit(),
        }
    }
    fn from_wit(w: wt::MassProperties) -> Self {
        let wt::MassProperties {
            volume,
            area,
            centroid,
        } = w;
        api::MassProperties {
            volume,
            area,
            centroid: DVec3::from_wit(centroid),
        }
    }
}

// ---- feature types ----------------------------------------------------------

impl Mirror for api::ParamSpec {
    type Wit = wf::ParamSpec;
    fn to_wit(&self) -> wf::ParamSpec {
        let api::ParamSpec {
            name,
            title,
            kind,
            default,
        } = self;
        wf::ParamSpec {
            name: name.clone(),
            title: title.clone(),
            kind: kind.to_wit(),
            default: default.clone(),
        }
    }
    fn from_wit(w: wf::ParamSpec) -> Self {
        let wf::ParamSpec {
            name,
            title,
            kind,
            default,
        } = w;
        api::ParamSpec {
            name,
            title,
            kind: QuantityKind::from_wit(kind),
            default,
        }
    }
}

impl Mirror for api::InputKind {
    type Wit = wf::InputKind;
    fn to_wit(&self) -> wf::InputKind {
        match self {
            api::InputKind::Plane => wf::InputKind::Plane,
        }
    }
    fn from_wit(w: wf::InputKind) -> Self {
        match w {
            wf::InputKind::Plane => api::InputKind::Plane,
        }
    }
}

impl Mirror for api::InputSpec {
    type Wit = wf::InputSpec;
    fn to_wit(&self) -> wf::InputSpec {
        let api::InputSpec { name, title, kind } = self;
        wf::InputSpec {
            name: name.clone(),
            title: title.clone(),
            kind: kind.to_wit(),
        }
    }
    fn from_wit(w: wf::InputSpec) -> Self {
        let wf::InputSpec { name, title, kind } = w;
        api::InputSpec {
            name,
            title,
            kind: api::InputKind::from_wit(kind),
        }
    }
}

impl Mirror for api::SlotKind {
    type Wit = wf::SlotKind;
    fn to_wit(&self) -> wf::SlotKind {
        match self {
            api::SlotKind::Body => wf::SlotKind::Body,
            api::SlotKind::Plane => wf::SlotKind::Plane,
        }
    }
    fn from_wit(w: wf::SlotKind) -> Self {
        match w {
            wf::SlotKind::Body => api::SlotKind::Body,
            wf::SlotKind::Plane => api::SlotKind::Plane,
        }
    }
}

impl Mirror for api::SlotSpec {
    type Wit = wf::SlotSpec;
    fn to_wit(&self) -> wf::SlotSpec {
        let api::SlotSpec { name, kind } = self;
        wf::SlotSpec {
            name: name.to_string(),
            kind: kind.to_wit(),
        }
    }
    fn from_wit(w: wf::SlotSpec) -> Self {
        let wf::SlotSpec { name, kind } = w;
        api::SlotSpec {
            name: SlotName::new(name).unwrap(),
            kind: api::SlotKind::from_wit(kind),
        }
    }
}

impl Mirror for api::FeatureTypeSpec {
    type Wit = wf::FeatureTypeSpec;
    fn to_wit(&self) -> wf::FeatureTypeSpec {
        let api::FeatureTypeSpec {
            id,
            version,
            title,
            params,
            inputs,
            outputs,
        } = self;
        wf::FeatureTypeSpec {
            id: id.clone(),
            version: *version,
            title: title.clone(),
            params: params.to_wit(),
            inputs: inputs.to_wit(),
            outputs: outputs.to_wit(),
        }
    }
    fn from_wit(w: wf::FeatureTypeSpec) -> Self {
        let wf::FeatureTypeSpec {
            id,
            version,
            title,
            params,
            inputs,
            outputs,
        } = w;
        api::FeatureTypeSpec {
            id,
            version,
            title,
            params: Vec::from_wit(params),
            inputs: Vec::from_wit(inputs),
            outputs: Vec::from_wit(outputs),
        }
    }
}

impl Mirror for api::ParamValue {
    type Wit = wf::ParamValue;
    fn to_wit(&self) -> wf::ParamValue {
        let api::ParamValue { name, value } = self;
        wf::ParamValue {
            name: name.clone(),
            value: value.to_wit(),
        }
    }
    fn from_wit(w: wf::ParamValue) -> Self {
        let wf::ParamValue { name, value } = w;
        api::ParamValue {
            name,
            value: Quantity::from_wit(value),
        }
    }
}

impl Mirror for api::ResolvedInput {
    type Wit = wf::ResolvedInput;
    fn to_wit(&self) -> wf::ResolvedInput {
        let api::ResolvedInput { name, value } = self;
        let value = match value {
            api::InputValue::Plane(f) => wf::InputValue::Plane(f.to_wit()),
        };
        wf::ResolvedInput {
            name: name.clone(),
            value,
        }
    }
    fn from_wit(w: wf::ResolvedInput) -> Self {
        let wf::ResolvedInput { name, value } = w;
        let value = match value {
            wf::InputValue::Plane(f) => api::InputValue::Plane(Frame::from_wit(f)),
        };
        api::ResolvedInput { name, value }
    }
}

/// Outputs hold bodies, which are resources: they move, never copy, so
/// they convert by value through the body table of the evaluation.
fn output_to_wit(out: api::FeatureOutput, bodies: &mut WitKernel) -> wf::FeatureOutput {
    let api::FeatureOutput { slots } = out;
    let slots = slots
        .into_iter()
        .map(|api::SlotOutput { name, value }| wf::SlotOutput {
            name: name.to_string(),
            value: match value {
                api::OutputValue::Body(b) => wf::OutputValue::Body(bodies.take(b)),
                api::OutputValue::Plane(f) => wf::OutputValue::Plane(f.to_wit()),
            },
        })
        .collect();
    wf::FeatureOutput { slots }
}

fn output_from_wit(out: wf::FeatureOutput) -> api::FeatureOutput {
    let wf::FeatureOutput { slots } = out;
    let slots = slots
        .into_iter()
        .map(|wf::SlotOutput { name, value }| api::SlotOutput {
            name: SlotName::new(name).unwrap(),
            value: match value {
                wf::OutputValue::Body(b) => api::OutputValue::Body(Body::from_handle(b.handle())),
                wf::OutputValue::Plane(f) => api::OutputValue::Plane(Frame::from_wit(f)),
            },
        })
        .collect();
    api::FeatureOutput { slots }
}

// ---- functions --------------------------------------------------------------

/// The Rust `Kernel` over the world's imports: what a Tier 1 guest built on
/// the Rust traits would run on. Bodies are kept in a table and handed out
/// by index. Natively the imports are unreachable, so this is held equal
/// at compile time only.
#[derive(Default)]
struct WitKernel {
    bodies: Vec<Option<wit::kernel::Body>>,
}

impl WitKernel {
    fn put(&mut self, body: wit::kernel::Body) -> Body {
        self.bodies.push(Some(body));
        Body::from_handle(u32::try_from(self.bodies.len() - 1).unwrap())
    }

    fn get(&self, body: &Body) -> &wit::kernel::Body {
        self.bodies[body.handle() as usize].as_ref().unwrap()
    }

    fn take(&mut self, body: Body) -> wit::kernel::Body {
        self.bodies[body.handle() as usize].take().unwrap()
    }
}

impl api::Kernel for WitKernel {
    fn extrude(&mut self, profile: &Profile, distance: f64) -> Result<Body, Diagnostic> {
        match wit::kernel::extrude(&profile.to_wit(), distance) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn names(&mut self, body: &Body) -> Result<Vec<PersistentName>, Diagnostic> {
        wit::kernel::names(self.get(body))
            .map(Vec::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn face_frame(&mut self, body: &Body, face: &PersistentName) -> Result<Frame, Diagnostic> {
        wit::kernel::face_frame(self.get(body), &face.to_wit())
            .map(Frame::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn measure(&mut self, body: &Body) -> Result<api::MassProperties, Diagnostic> {
        wit::kernel::measure(self.get(body))
            .map(api::MassProperties::from_wit)
            .map_err(Diagnostic::from_wit)
    }
}

/// A test plugin: one feature type, a plane offset from its input, so it
/// evaluates without the kernel and runs natively through both adapters.
struct Offset;

fn offset_type() -> api::FeatureTypeSpec {
    api::FeatureTypeSpec {
        id: "test.offset".into(),
        version: 1,
        title: "Offset plane".into(),
        params: vec![api::ParamSpec {
            name: "distance".into(),
            title: "Distance".into(),
            kind: QuantityKind::Length,
            default: "10 mm".into(),
        }],
        inputs: vec![api::InputSpec {
            name: "plane".into(),
            title: "Plane".into(),
            kind: api::InputKind::Plane,
        }],
        outputs: vec![api::SlotSpec {
            name: SlotName::new("plane").unwrap(),
            kind: api::SlotKind::Plane,
        }],
    }
}

impl api::Feature for Offset {
    fn describe(&self) -> Vec<api::FeatureTypeSpec> {
        vec![offset_type()]
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn api::Kernel,
        type_id: &str,
        params: &[api::ParamValue],
        inputs: &[api::ResolvedInput],
    ) -> Result<api::FeatureOutput, Diagnostic> {
        let refuse = |what: &str| {
            Err(Diagnostic::new(Severity::Error, "test.offset.input", what.to_owned()).unwrap())
        };
        if type_id != "test.offset" {
            return refuse("unknown type");
        }
        let (Some(distance), Some(api::InputValue::Plane(plane))) = (
            params.iter().find(|p| p.name == "distance"),
            inputs.iter().find(|i| i.name == "plane").map(|i| i.value),
        ) else {
            return refuse("missing distance or plane");
        };
        Ok(api::FeatureOutput {
            slots: vec![api::SlotOutput {
                name: SlotName::new("plane").unwrap(),
                value: api::OutputValue::Plane(plane.offset(distance.value.si)),
            }],
        })
    }
}

/// The world's `feature` export implemented by forwarding to a Rust
/// `Feature`, as a Tier 1 guest on the Rust traits would be.
struct Exported;

impl wf::Guest for Exported {
    fn describe() -> Vec<wf::FeatureTypeSpec> {
        api::Feature::describe(&Offset).to_wit()
    }

    fn evaluate(
        type_id: String,
        params: Vec<wf::ParamValue>,
        inputs: Vec<wf::ResolvedInput>,
    ) -> Result<wf::FeatureOutput, wt::Diagnostic> {
        let mut kernel = WitKernel::default();
        let out = api::Feature::evaluate(
            &Offset,
            &mut kernel,
            &type_id,
            &Vec::from_wit(params),
            &Vec::from_wit(inputs),
        );
        match out {
            Ok(out) => Ok(output_to_wit(out, &mut kernel)),
            Err(d) => Err(d.to_wit()),
        }
    }
}

/// A Rust `Feature` implemented by forwarding to the world's export, as
/// the host sees a Tier 1 plugin.
struct Imported;

impl api::Feature for Imported {
    fn describe(&self) -> Vec<api::FeatureTypeSpec> {
        Vec::from_wit(<Exported as wf::Guest>::describe())
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn api::Kernel,
        type_id: &str,
        params: &[api::ParamValue],
        inputs: &[api::ResolvedInput],
    ) -> Result<api::FeatureOutput, Diagnostic> {
        let (params, inputs) = (params.to_vec().to_wit(), inputs.to_vec().to_wit());
        <Exported as wf::Guest>::evaluate(type_id.to_owned(), params, inputs)
            .map(output_from_wit)
            .map_err(Diagnostic::from_wit)
    }
}

// ---- tests ------------------------------------------------------------------

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn tilted() -> Frame {
    Frame::new(
        DVec3::new(0.01, -0.02, 0.5),
        DVec3::new(0.0, 0.6, 0.8),
        DVec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

#[test]
fn every_type_round_trips_through_the_world() {
    for v in [DVec3::ZERO, DVec3::new(1e-6, -2.5, 3e3)] {
        round_trip(&v);
    }
    for f in [Frame::WORLD_XY, Frame::WORLD_YZ, Frame::WORLD_ZX, tilted()] {
        round_trip(&f);
    }
    for kind in [
        QuantityKind::Length,
        QuantityKind::Angle,
        QuantityKind::Count,
        QuantityKind::Ratio,
        QuantityKind::Mass,
    ] {
        round_trip(&Quantity::si(kind, 0.012));
    }

    let face: PersistentName = format!("face:sweep.{}.side.{}", Id(7), Id(9))
        .parse()
        .unwrap();
    let refs = vec![
        Ref::Param(ParamId(Id(1))),
        Ref::Feature(FeatureId(Id(2))),
        Ref::Slot {
            feature: FeatureId(Id(3)),
            slot: SlotName::new("body").unwrap(),
        },
        Ref::Topo(face.clone()),
        Ref::Sketch {
            feature: FeatureId(Id(4)),
            entity: SketchEntityId(Id(5)),
        },
        Ref::Plugin {
            plugin: PluginId::new("gears").unwrap(),
            record: RecordId(Id(u64::MAX)),
        },
    ];
    round_trip(&face);
    round_trip(&refs);
    for severity in [Severity::Info, Severity::Warning, Severity::Error] {
        round_trip(&Diagnostic::new(severity, "ref.lost", "gone").unwrap());
    }
    round_trip(
        &Diagnostic::new(Severity::Error, "kernel.degenerate.not-positive", "no")
            .unwrap()
            .with_refs(refs.clone())
            .with_candidates(refs.into_iter().rev()),
    );

    let path = ProfileLoop::Path {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            ProfileSegment::Line {
                key: key(1),
                to: DVec2::new(0.04, 0.0),
            },
            ProfileSegment::Arc {
                key: key(2),
                to: DVec2::new(0.0, 0.0),
                via: DVec2::new(0.02, 0.01),
            },
        ],
    };
    let hole = ProfileLoop::Circle {
        key: key(3),
        center: DVec2::new(0.02, 0.003),
        radius: 0.001,
    };
    round_trip(&Profile::new(tilted(), path, vec![hole]).unwrap());
    round_trip(&api::MassProperties {
        volume: 1e-6,
        area: 6e-4,
        centroid: DVec3::new(0.0, 0.0, 0.005),
    });

    round_trip(&offset_type());
    round_trip(&api::SlotSpec {
        name: SlotName::new("body").unwrap(),
        kind: api::SlotKind::Body,
    });
}

#[test]
fn a_feature_behaves_the_same_through_the_world() {
    use api::Feature;

    assert_eq!(Imported.describe(), Offset.describe());

    let params = [api::ParamValue {
        name: "distance".into(),
        value: Quantity::si(QuantityKind::Length, 0.002),
    }];
    let inputs = [api::ResolvedInput {
        name: "plane".into(),
        value: api::InputValue::Plane(tilted()),
    }];
    let mut kernel = WitKernel::default();
    for type_id in ["test.offset", "test.other"] {
        let direct = Offset.evaluate(&mut kernel, type_id, &params, &inputs);
        let through = Imported.evaluate(&mut kernel, type_id, &params, &inputs);
        assert_eq!(through, direct, "{type_id}");
    }
}

/// Every type and function of the world, by interface. Each is covered
/// above: a type by a `Mirror` impl (or by `output_*_wit` for outputs,
/// `WitKernel` for `body`), a function by `WitKernel` or `Exported`.
const COVERED: &[(&str, &[&str], &[&str])] = &[
    (
        "types",
        &[
            "id",
            "vec2",
            "vec3",
            "frame",
            "quantity-kind",
            "quantity",
            "persistent-ref",
            "slot-ref",
            "sketch-ref",
            "plugin-ref",
            "reference",
            "severity",
            "diagnostic",
            "line-segment",
            "arc-segment",
            "profile-segment",
            "circle-loop",
            "path-loop",
            "profile-loop",
            "profile",
            "mass-properties",
        ],
        &[],
    ),
    (
        "kernel",
        &["body"],
        &["extrude", "names", "face-frame", "measure"],
    ),
    (
        "feature",
        &[
            "param-spec",
            "input-kind",
            "input-spec",
            "slot-kind",
            "slot-spec",
            "feature-type-spec",
            "param-value",
            "input-value",
            "resolved-input",
            "output-value",
            "slot-output",
            "feature-output",
        ],
        &["describe", "evaluate"],
    ),
];

#[test]
fn the_world_is_exactly_what_is_covered() {
    let mut resolve = wit_parser::Resolve::default();
    let wit = Path::new(env!("CARGO_MANIFEST_DIR")).join("wit");
    let (package, _) = resolve.push_dir(&wit).unwrap();
    let pkg = &resolve.packages[package];
    assert_eq!(pkg.name.namespace, "arrix");
    assert_eq!(pkg.name.name, "plugin");
    assert_eq!(
        pkg.name
            .version
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some(api::API_VERSION),
        "the WIT package's version is the crate's"
    );
    assert_eq!(
        std::fs::read_to_string(wit.join("arrix-plugin.wit")).unwrap(),
        api::WIT
    );

    let world = &resolve.worlds[pkg.worlds["plugin"]];
    let names = |items: &wit_parser::IndexMap<_, wit_parser::WorldItem>| {
        items
            .keys()
            .map(|k| resolve.name_world_key(k))
            .collect::<BTreeSet<_>>()
    };
    // `kernel` is imported for its own sake and by `feature`'s `use`;
    // `types` only through the `use`s.
    assert_eq!(
        names(&world.imports),
        BTreeSet::from(["arrix:plugin/kernel@0.1.0", "arrix:plugin/types@0.1.0"].map(String::from))
    );
    assert_eq!(
        names(&world.exports),
        BTreeSet::from(["arrix:plugin/feature@0.1.0".to_owned()])
    );

    let mut found = BTreeSet::new();
    for (_, iface) in pkg.interfaces.iter() {
        let iface = &resolve.interfaces[*iface];
        let name = iface.name.clone().unwrap();
        // A `use` makes an alias of the used type; it is covered where it
        // is defined.
        for (ty, id) in &iface.types {
            let used = matches!(
                resolve.types[*id].kind,
                wit_parser::TypeDefKind::Type(wit_parser::Type::Id(_))
            );
            if !used {
                found.insert((name.clone(), "type", ty.clone()));
            }
        }
        for f in iface.functions.keys() {
            found.insert((name.clone(), "func", f.clone()));
        }
    }
    let covered = COVERED
        .iter()
        .flat_map(|(iface, types, funcs)| {
            let t = types
                .iter()
                .map(move |t| (iface.to_string(), "type", t.to_string()));
            let f = funcs
                .iter()
                .map(move |f| (iface.to_string(), "func", f.to_string()));
            t.chain(f)
        })
        .collect::<BTreeSet<_>>();
    let uncovered: Vec<_> = found.difference(&covered).collect();
    let gone: Vec<_> = covered.difference(&found).collect();
    assert!(
        uncovered.is_empty() && gone.is_empty(),
        "in the world but not covered here: {uncovered:?}; covered here but not in the world: {gone:?}"
    );
}
