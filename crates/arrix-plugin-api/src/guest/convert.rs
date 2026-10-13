//! The world's types and the API's, converted both ways: the `Mirror` of
//! every type, and the resolved inputs and outputs of a feature, whose
//! bodies move through a [`WitKernel`]'s table. Exhaustive destructuring
//! and matches on both sides fail to compile when a field or case is added
//! to either (`tests/wit_equality.rs`).

use super::bindings::arrix::plugin::types as wt;
use super::bindings::exports::arrix::plugin::feature as wf;
use super::kernel::WitKernel;
use crate as api;
use crate::{
    Axis, Body, CurveKey, DVec2, DVec3, Diagnostic, FeatureId, Frame, Id, Motion, ParamId,
    PersistentName, PluginId, Profile, ProfileLoop, ProfileSegment, Quantity, QuantityKind,
    RecordId, Ref, RegionKey, Severity, SketchEntityId, SlotName,
};

/// A Rust type of the API and its type in the world. `from_wit` panics on
/// a value the Rust type refuses: the WIT form of a valid value is valid,
/// and that is what the round trips check.
pub trait Mirror: Sized {
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

impl<T: Mirror> Mirror for Option<T> {
    type Wit = Option<T::Wit>;
    fn to_wit(&self) -> Self::Wit {
        self.as_ref().map(T::to_wit)
    }
    fn from_wit(w: Self::Wit) -> Self {
        w.map(T::from_wit)
    }
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
            QuantityKind::Density => wt::QuantityKind::Density,
        }
    }
    fn from_wit(w: wt::QuantityKind) -> Self {
        match w {
            wt::QuantityKind::Length => QuantityKind::Length,
            wt::QuantityKind::Angle => QuantityKind::Angle,
            wt::QuantityKind::Count => QuantityKind::Count,
            wt::QuantityKind::Ratio => QuantityKind::Ratio,
            wt::QuantityKind::Mass => QuantityKind::Mass,
            wt::QuantityKind::Density => QuantityKind::Density,
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
            Ref::Region { feature, key } => wt::Reference::Region(wt::RegionRef {
                feature: feature.0.0,
                entities: key.entities.iter().map(|e| e.0.0).collect(),
                sample: (key.sample[0], key.sample[1]),
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
            wt::Reference::Region(wt::RegionRef {
                feature,
                entities,
                sample,
            }) => Ref::Region {
                feature: FeatureId(Id(feature)),
                key: RegionKey {
                    entities: entities
                        .into_iter()
                        .map(|e| SketchEntityId(Id(e)))
                        .collect(),
                    sample: [sample.0, sample.1],
                },
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

impl Mirror for Axis {
    type Wit = wt::Axis;
    fn to_wit(&self) -> wt::Axis {
        let Axis { origin, direction } = *self;
        wt::Axis {
            origin: origin.to_wit(),
            direction: direction.to_wit(),
        }
    }
    fn from_wit(w: wt::Axis) -> Self {
        let wt::Axis { origin, direction } = w;
        Axis {
            origin: DVec3::from_wit(origin),
            direction: DVec3::from_wit(direction),
        }
    }
}

impl Mirror for Motion {
    type Wit = wt::Motion;
    fn to_wit(&self) -> wt::Motion {
        let Motion {
            translation,
            axis,
            angle,
        } = *self;
        wt::Motion {
            translation: translation.to_wit(),
            axis: axis.to_wit(),
            angle,
        }
    }
    fn from_wit(w: wt::Motion) -> Self {
        let wt::Motion {
            translation,
            axis,
            angle,
        } = w;
        Motion {
            translation: DVec3::from_wit(translation),
            axis: DVec3::from_wit(axis),
            angle,
        }
    }
}

impl Mirror for api::Point {
    type Wit = wt::Point;
    fn to_wit(&self) -> wt::Point {
        let api::Point { at, normal } = *self;
        wt::Point {
            at: at.to_wit(),
            normal: normal.map(|n| n.to_wit()),
        }
    }
    fn from_wit(w: wt::Point) -> Self {
        let wt::Point { at, normal } = w;
        api::Point {
            at: DVec3::from_wit(at),
            normal: normal.map(DVec3::from_wit),
        }
    }
}

impl Mirror for api::ToolOp {
    type Wit = wf::ToolOp;
    fn to_wit(&self) -> wf::ToolOp {
        match self {
            api::ToolOp::Fuse => wf::ToolOp::Fuse,
            api::ToolOp::Cut => wf::ToolOp::Cut,
        }
    }
    fn from_wit(w: wf::ToolOp) -> Self {
        match w {
            wf::ToolOp::Fuse => api::ToolOp::Fuse,
            wf::ToolOp::Cut => api::ToolOp::Cut,
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
            shown_when,
        } = self;
        wf::ParamSpec {
            name: name.clone(),
            title: title.clone(),
            kind: kind.to_wit(),
            default: default.clone(),
            shown_when: shown_when.to_wit(),
        }
    }
    fn from_wit(w: wf::ParamSpec) -> Self {
        let wf::ParamSpec {
            name,
            title,
            kind,
            default,
            shown_when,
        } = w;
        api::ParamSpec {
            name,
            title,
            kind: QuantityKind::from_wit(kind),
            default,
            shown_when: Option::from_wit(shown_when),
        }
    }
}

impl Mirror for api::ShownWhen {
    type Wit = wf::ShownWhen;
    fn to_wit(&self) -> wf::ShownWhen {
        let api::ShownWhen { choice, values } = self;
        wf::ShownWhen {
            choice: choice.clone(),
            values: values.clone(),
        }
    }
    fn from_wit(w: wf::ShownWhen) -> Self {
        let wf::ShownWhen { choice, values } = w;
        api::ShownWhen { choice, values }
    }
}

impl Mirror for api::ChoiceOption {
    type Wit = wf::ChoiceOption;
    fn to_wit(&self) -> wf::ChoiceOption {
        let api::ChoiceOption { name, title } = self;
        wf::ChoiceOption {
            name: name.clone(),
            title: title.clone(),
        }
    }
    fn from_wit(w: wf::ChoiceOption) -> Self {
        let wf::ChoiceOption { name, title } = w;
        api::ChoiceOption { name, title }
    }
}

impl Mirror for api::ChoiceSpec {
    type Wit = wf::ChoiceSpec;
    fn to_wit(&self) -> wf::ChoiceSpec {
        let api::ChoiceSpec {
            name,
            title,
            options,
            default,
        } = self;
        wf::ChoiceSpec {
            name: name.clone(),
            title: title.clone(),
            options: options.to_wit(),
            default: default.clone(),
        }
    }
    fn from_wit(w: wf::ChoiceSpec) -> Self {
        let wf::ChoiceSpec {
            name,
            title,
            options,
            default,
        } = w;
        api::ChoiceSpec {
            name,
            title,
            options: Vec::from_wit(options),
            default,
        }
    }
}

impl Mirror for api::ChoiceValue {
    type Wit = wf::ChoiceValue;
    fn to_wit(&self) -> wf::ChoiceValue {
        let api::ChoiceValue { name, value } = self;
        wf::ChoiceValue {
            name: name.clone(),
            value: value.clone(),
        }
    }
    fn from_wit(w: wf::ChoiceValue) -> Self {
        let wf::ChoiceValue { name, value } = w;
        api::ChoiceValue { name, value }
    }
}

impl Mirror for api::InputKind {
    type Wit = wf::InputKind;
    fn to_wit(&self) -> wf::InputKind {
        match self {
            api::InputKind::Plane => wf::InputKind::Plane,
            api::InputKind::Region => wf::InputKind::Region,
            api::InputKind::Body => wf::InputKind::Body,
            api::InputKind::Edge => wf::InputKind::Edge,
            api::InputKind::Point => wf::InputKind::Point,
            api::InputKind::Axis => wf::InputKind::Axis,
            api::InputKind::Tool => wf::InputKind::Tool,
        }
    }
    fn from_wit(w: wf::InputKind) -> Self {
        match w {
            wf::InputKind::Plane => api::InputKind::Plane,
            wf::InputKind::Region => api::InputKind::Region,
            wf::InputKind::Body => api::InputKind::Body,
            wf::InputKind::Edge => api::InputKind::Edge,
            wf::InputKind::Point => api::InputKind::Point,
            wf::InputKind::Axis => api::InputKind::Axis,
            wf::InputKind::Tool => api::InputKind::Tool,
        }
    }
}

impl Mirror for api::InputSpec {
    type Wit = wf::InputSpec;
    fn to_wit(&self) -> wf::InputSpec {
        let api::InputSpec {
            name,
            title,
            kind,
            list,
            shown_when,
        } = self;
        wf::InputSpec {
            name: name.clone(),
            title: title.clone(),
            kind: kind.to_wit(),
            list: *list,
            shown_when: shown_when.to_wit(),
        }
    }
    fn from_wit(w: wf::InputSpec) -> Self {
        let wf::InputSpec {
            name,
            title,
            kind,
            list,
            shown_when,
        } = w;
        api::InputSpec {
            name,
            title,
            kind: api::InputKind::from_wit(kind),
            list,
            shown_when: Option::from_wit(shown_when),
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
        let api::SlotSpec {
            name,
            kind,
            modifies,
        } = self;
        wf::SlotSpec {
            name: name.to_string(),
            kind: kind.to_wit(),
            modifies: modifies.clone(),
        }
    }
    fn from_wit(w: wf::SlotSpec) -> Self {
        let wf::SlotSpec {
            name,
            kind,
            modifies,
        } = w;
        api::SlotSpec {
            name: SlotName::new(name).unwrap(),
            kind: api::SlotKind::from_wit(kind),
            modifies,
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
            choices,
            inputs,
            outputs,
        } = self;
        wf::FeatureTypeSpec {
            id: id.clone(),
            version: *version,
            title: title.clone(),
            params: params.to_wit(),
            choices: choices.to_wit(),
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
            choices,
            inputs,
            outputs,
        } = w;
        api::FeatureTypeSpec {
            id,
            version,
            title,
            params: Vec::from_wit(params),
            choices: Vec::from_wit(choices),
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

/// Inputs may hold bodies, which are resources: a body input is a handle
/// of the feature's own (ADR-0008), so inputs convert through the body
/// table of the evaluation, as outputs do.
pub fn input_to_wit(input: &api::ResolvedInput, bodies: &mut WitKernel) -> wf::ResolvedInput {
    let api::ResolvedInput { name, value } = input;
    let value = match value {
        api::InputValue::Plane(f) => wf::InputValue::Plane(f.to_wit()),
        api::InputValue::Region(p) => wf::InputValue::Region(p.to_wit()),
        api::InputValue::Body(b) => wf::InputValue::Body(bodies.take(b)),
        api::InputValue::Edge(n) => wf::InputValue::Edge(n.to_wit()),
        api::InputValue::Point(p) => wf::InputValue::Point(p.to_wit()),
        api::InputValue::Axis(a) => wf::InputValue::Axis(a.to_wit()),
        api::InputValue::Tool(api::Tool { body, op }) => wf::InputValue::Tool(wf::Tool {
            body: bodies.take(body),
            op: op.to_wit(),
        }),
    };
    wf::ResolvedInput {
        name: name.clone(),
        value,
    }
}

pub fn input_from_wit(w: wf::ResolvedInput, bodies: &mut WitKernel) -> api::ResolvedInput {
    let wf::ResolvedInput { name, value } = w;
    let value = match value {
        wf::InputValue::Plane(f) => api::InputValue::Plane(Frame::from_wit(f)),
        wf::InputValue::Region(p) => api::InputValue::Region(Profile::from_wit(p)),
        wf::InputValue::Body(b) => api::InputValue::Body(bodies.put(b)),
        wf::InputValue::Edge(n) => api::InputValue::Edge(PersistentName::from_wit(n)),
        wf::InputValue::Point(p) => api::InputValue::Point(api::Point::from_wit(p)),
        wf::InputValue::Axis(a) => api::InputValue::Axis(Axis::from_wit(a)),
        wf::InputValue::Tool(wf::Tool { body, op }) => api::InputValue::Tool(api::Tool {
            body: bodies.put(body),
            op: api::ToolOp::from_wit(op),
        }),
    };
    api::ResolvedInput { name, value }
}

/// Outputs hold bodies, which are resources: they move, never copy, so
/// they convert by value through the body table of the evaluation.
pub fn output_to_wit(out: api::FeatureOutput, bodies: &mut WitKernel) -> wf::FeatureOutput {
    let api::FeatureOutput { slots, tool } = out;
    let tool = tool.map(|api::Tool { body, op }| wf::Tool {
        body: bodies.take(&body),
        op: op.to_wit(),
    });
    let slots = slots
        .into_iter()
        .map(|api::SlotOutput { name, value }| wf::SlotOutput {
            name: name.to_string(),
            value: match value {
                api::OutputValue::Body(b) => wf::OutputValue::Body(bodies.take(&b)),
                api::OutputValue::Plane(f) => wf::OutputValue::Plane(f.to_wit()),
            },
        })
        .collect();
    wf::FeatureOutput { slots, tool }
}

pub fn output_from_wit(out: wf::FeatureOutput) -> api::FeatureOutput {
    let wf::FeatureOutput { slots, tool } = out;
    let tool = tool.map(|wf::Tool { body, op }| api::Tool {
        body: Body::from_handle(body.handle()),
        op: api::ToolOp::from_wit(op),
    });
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
    api::FeatureOutput { slots, tool }
}
