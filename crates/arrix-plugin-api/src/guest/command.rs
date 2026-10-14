//! The types of the world's `commands` converted both ways (ADR-0038).

use super::bindings::commands as wc;
use super::convert::Mirror;
use crate as api;
use crate::{AssemblyId, ComponentId, DVec3, FeatureId, Id, JointId, MaterialId, PartId};

fn path_to_wit(path: &[ComponentId]) -> Vec<u64> {
    path.iter().map(|c| c.0.0).collect()
}

fn path_from_wit(path: Vec<u64>) -> Vec<ComponentId> {
    path.into_iter().map(|c| ComponentId(Id(c))).collect()
}

impl Mirror for api::CommandSpec {
    type Wit = wc::CommandSpec;
    fn to_wit(&self) -> wc::CommandSpec {
        let api::CommandSpec {
            id,
            title,
            group,
            params,
            choices,
            inputs,
            max_new,
        } = self;
        wc::CommandSpec {
            id: id.clone(),
            title: title.clone(),
            group: group.clone(),
            params: params.to_wit(),
            choices: choices.to_wit(),
            inputs: inputs.to_wit(),
            max_new: *max_new,
        }
    }
    fn from_wit(w: wc::CommandSpec) -> Self {
        let wc::CommandSpec {
            id,
            title,
            group,
            params,
            choices,
            inputs,
            max_new,
        } = w;
        api::CommandSpec {
            id,
            title,
            group,
            params: Vec::from_wit(params),
            choices: Vec::from_wit(choices),
            inputs: Vec::from_wit(inputs),
            max_new,
        }
    }
}

impl Mirror for api::Context {
    type Wit = wc::Context;
    fn to_wit(&self) -> wc::Context {
        match self {
            api::Context::Part(p) => wc::Context::Part(p.0.0),
            api::Context::Assembly(a) => wc::Context::Assembly(a.0.0),
        }
    }
    fn from_wit(w: wc::Context) -> Self {
        match w {
            wc::Context::Part(p) => api::Context::Part(PartId(Id(p))),
            wc::Context::Assembly(a) => api::Context::Assembly(AssemblyId(Id(a))),
        }
    }
}

impl Mirror for api::CircleEdge {
    type Wit = wc::CircleEdge;
    fn to_wit(&self) -> wc::CircleEdge {
        let api::CircleEdge {
            centre,
            axis,
            radius,
        } = self;
        wc::CircleEdge {
            centre: centre.to_wit(),
            axis: axis.to_wit(),
            radius: *radius,
        }
    }
    fn from_wit(w: wc::CircleEdge) -> Self {
        let wc::CircleEdge {
            centre,
            axis,
            radius,
        } = w;
        api::CircleEdge {
            centre: DVec3::from_wit(centre),
            axis: DVec3::from_wit(axis),
            radius,
        }
    }
}

impl Mirror for api::LineEdge {
    type Wit = wc::LineEdge;
    fn to_wit(&self) -> wc::LineEdge {
        let api::LineEdge { start, end } = self;
        wc::LineEdge {
            start: start.to_wit(),
            end: end.to_wit(),
        }
    }
    fn from_wit(w: wc::LineEdge) -> Self {
        let wc::LineEdge { start, end } = w;
        api::LineEdge {
            start: DVec3::from_wit(start),
            end: DVec3::from_wit(end),
        }
    }
}

impl Mirror for api::EdgeCurve {
    type Wit = wc::EdgeCurve;
    fn to_wit(&self) -> wc::EdgeCurve {
        match self {
            api::EdgeCurve::Circle(c) => wc::EdgeCurve::Circle(c.to_wit()),
            api::EdgeCurve::Line(l) => wc::EdgeCurve::Line(l.to_wit()),
            api::EdgeCurve::Other => wc::EdgeCurve::Other,
        }
    }
    fn from_wit(w: wc::EdgeCurve) -> Self {
        match w {
            wc::EdgeCurve::Circle(c) => api::EdgeCurve::Circle(api::CircleEdge::from_wit(c)),
            wc::EdgeCurve::Line(l) => api::EdgeCurve::Line(api::LineEdge::from_wit(l)),
            wc::EdgeCurve::Other => api::EdgeCurve::Other,
        }
    }
}

impl Mirror for api::ResolvedEdge {
    type Wit = wc::ResolvedEdge;
    fn to_wit(&self) -> wc::ResolvedEdge {
        let api::ResolvedEdge {
            path,
            name,
            curve,
            face_normals,
        } = self;
        wc::ResolvedEdge {
            path: path_to_wit(path),
            name: name.to_wit(),
            curve: curve.to_wit(),
            face_normals: face_normals.to_wit(),
        }
    }
    fn from_wit(w: wc::ResolvedEdge) -> Self {
        let wc::ResolvedEdge {
            path,
            name,
            curve,
            face_normals,
        } = w;
        api::ResolvedEdge {
            path: path_from_wit(path),
            name: api::PersistentName::from_wit(name),
            curve: api::EdgeCurve::from_wit(curve),
            face_normals: Vec::from_wit(face_normals),
        }
    }
}

impl Mirror for api::CommandInputValue {
    type Wit = wc::CommandInputValue;
    fn to_wit(&self) -> wc::CommandInputValue {
        match self {
            api::CommandInputValue::Edge(e) => wc::CommandInputValue::Edge(e.to_wit()),
        }
    }
    fn from_wit(w: wc::CommandInputValue) -> Self {
        match w {
            wc::CommandInputValue::Edge(e) => {
                api::CommandInputValue::Edge(api::ResolvedEdge::from_wit(e))
            }
        }
    }
}

impl Mirror for api::CommandInput {
    type Wit = wc::CommandInput;
    fn to_wit(&self) -> wc::CommandInput {
        let api::CommandInput { name, value } = self;
        wc::CommandInput {
            name: name.clone(),
            value: value.to_wit(),
        }
    }
    fn from_wit(w: wc::CommandInput) -> Self {
        let wc::CommandInput { name, value } = w;
        api::CommandInput {
            name,
            value: api::CommandInputValue::from_wit(value),
        }
    }
}

impl Mirror for api::Expression {
    type Wit = wc::Expression;
    fn to_wit(&self) -> wc::Expression {
        match self {
            api::Expression::Si(q) => wc::Expression::Si(q.to_wit()),
            api::Expression::Text(t) => wc::Expression::Text(t.clone()),
        }
    }
    fn from_wit(w: wc::Expression) -> Self {
        match w {
            wc::Expression::Si(q) => api::Expression::Si(api::Quantity::from_wit(q)),
            wc::Expression::Text(t) => api::Expression::Text(t),
        }
    }
}

fn triple_to_wit(e: &[api::Expression; 3]) -> (wc::Expression, wc::Expression, wc::Expression) {
    let [a, b, c] = e;
    (a.to_wit(), b.to_wit(), c.to_wit())
}

fn triple_from_wit(w: (wc::Expression, wc::Expression, wc::Expression)) -> [api::Expression; 3] {
    let (a, b, c) = w;
    [
        api::Expression::from_wit(a),
        api::Expression::from_wit(b),
        api::Expression::from_wit(c),
    ]
}

impl Mirror for api::NamedExpression {
    type Wit = wc::NamedExpression;
    fn to_wit(&self) -> wc::NamedExpression {
        let api::NamedExpression { name, expression } = self;
        wc::NamedExpression {
            name: name.clone(),
            expression: expression.to_wit(),
        }
    }
    fn from_wit(w: wc::NamedExpression) -> Self {
        let wc::NamedExpression { name, expression } = w;
        api::NamedExpression {
            name,
            expression: api::Expression::from_wit(expression),
        }
    }
}

impl Mirror for api::NamedInput {
    type Wit = wc::NamedInput;
    fn to_wit(&self) -> wc::NamedInput {
        let api::NamedInput { name, refs, many } = self;
        wc::NamedInput {
            name: name.clone(),
            refs: refs.to_wit(),
            many: *many,
        }
    }
    fn from_wit(w: wc::NamedInput) -> Self {
        let wc::NamedInput { name, refs, many } = w;
        api::NamedInput {
            name,
            refs: Vec::from_wit(refs),
            many,
        }
    }
}

impl Mirror for api::FeatureDraft {
    type Wit = wc::FeatureDraft;
    fn to_wit(&self) -> wc::FeatureDraft {
        let api::FeatureDraft {
            id,
            type_id,
            name,
            params,
            choices,
            inputs,
        } = self;
        wc::FeatureDraft {
            id: id.0.0,
            type_id: type_id.clone(),
            name: name.clone(),
            params: params.to_wit(),
            choices: choices.to_wit(),
            inputs: inputs.to_wit(),
        }
    }
    fn from_wit(w: wc::FeatureDraft) -> Self {
        let wc::FeatureDraft {
            id,
            type_id,
            name,
            params,
            choices,
            inputs,
        } = w;
        api::FeatureDraft {
            id: FeatureId(Id(id)),
            type_id,
            name,
            params: Vec::from_wit(params),
            choices: Vec::from_wit(choices),
            inputs: Vec::from_wit(inputs),
        }
    }
}

impl Mirror for api::PartDraft {
    type Wit = wc::PartDraft;
    fn to_wit(&self) -> wc::PartDraft {
        let api::PartDraft {
            id,
            name,
            features,
            material,
        } = self;
        wc::PartDraft {
            id: id.0.0,
            name: name.clone(),
            features: features.to_wit(),
            material: material.map(|m| m.0.0),
        }
    }
    fn from_wit(w: wc::PartDraft) -> Self {
        let wc::PartDraft {
            id,
            name,
            features,
            material,
        } = w;
        api::PartDraft {
            id: PartId(Id(id)),
            name,
            features: Vec::from_wit(features),
            material: material.map(|m| MaterialId(Id(m))),
        }
    }
}

impl Mirror for api::AddFeature {
    type Wit = wc::AddFeature;
    fn to_wit(&self) -> wc::AddFeature {
        let api::AddFeature { part, at, feature } = self;
        wc::AddFeature {
            part: part.0.0,
            at: *at,
            feature: feature.to_wit(),
        }
    }
    fn from_wit(w: wc::AddFeature) -> Self {
        let wc::AddFeature { part, at, feature } = w;
        api::AddFeature {
            part: PartId(Id(part)),
            at,
            feature: api::FeatureDraft::from_wit(feature),
        }
    }
}

impl Mirror for api::Pose {
    type Wit = wc::Pose;
    fn to_wit(&self) -> wc::Pose {
        let api::Pose { position, rpy } = self;
        wc::Pose {
            position: triple_to_wit(position),
            rpy: triple_to_wit(rpy),
        }
    }
    fn from_wit(w: wc::Pose) -> Self {
        let wc::Pose { position, rpy } = w;
        api::Pose {
            position: triple_from_wit(position),
            rpy: triple_from_wit(rpy),
        }
    }
}

impl Mirror for api::ComponentDraft {
    type Wit = wc::ComponentDraft;
    fn to_wit(&self) -> wc::ComponentDraft {
        let api::ComponentDraft { id, name, of, pose } = self;
        wc::ComponentDraft {
            id: id.0.0,
            name: name.clone(),
            of: of.to_wit(),
            pose: pose.to_wit(),
        }
    }
    fn from_wit(w: wc::ComponentDraft) -> Self {
        let wc::ComponentDraft { id, name, of, pose } = w;
        api::ComponentDraft {
            id: ComponentId(Id(id)),
            name,
            of: api::ComponentOf::from_wit(of),
            pose: Option::from_wit(pose),
        }
    }
}

impl Mirror for api::ComponentChange {
    type Wit = wc::ComponentChange;
    fn to_wit(&self) -> wc::ComponentChange {
        let api::ComponentChange {
            assembly,
            component,
        } = self;
        wc::ComponentChange {
            assembly: assembly.0.0,
            component: component.to_wit(),
        }
    }
    fn from_wit(w: wc::ComponentChange) -> Self {
        let wc::ComponentChange {
            assembly,
            component,
        } = w;
        api::ComponentChange {
            assembly: AssemblyId(Id(assembly)),
            component: api::ComponentDraft::from_wit(component),
        }
    }
}

impl Mirror for api::Anchor {
    type Wit = wc::Anchor;
    fn to_wit(&self) -> wc::Anchor {
        let api::Anchor { path, axis, origin } = self;
        wc::Anchor {
            path: path_to_wit(path),
            axis: axis.to_wit(),
            origin: origin.to_wit(),
        }
    }
    fn from_wit(w: wc::Anchor) -> Self {
        let wc::Anchor { path, axis, origin } = w;
        api::Anchor {
            path: path_from_wit(path),
            axis: api::PersistentName::from_wit(axis),
            origin: Option::from_wit(origin),
        }
    }
}

impl Mirror for api::JointDraft {
    type Wit = wc::JointDraft;
    fn to_wit(&self) -> wc::JointDraft {
        let api::JointDraft {
            id,
            name,
            kind,
            parent,
            child,
            value,
            offset,
            limits,
            flip,
        } = self;
        wc::JointDraft {
            id: id.0.0,
            name: name.clone(),
            kind: kind.to_wit(),
            parent: parent.to_wit(),
            child: child.to_wit(),
            value: value.to_wit(),
            offset: offset.to_wit(),
            limits: limits.as_ref().map(|[lo, hi]| (lo.to_wit(), hi.to_wit())),
            flip: *flip,
        }
    }
    fn from_wit(w: wc::JointDraft) -> Self {
        let wc::JointDraft {
            id,
            name,
            kind,
            parent,
            child,
            value,
            offset,
            limits,
            flip,
        } = w;
        api::JointDraft {
            id: JointId(Id(id)),
            name,
            kind: api::JointKind::from_wit(kind),
            parent: api::Anchor::from_wit(parent),
            child: api::Anchor::from_wit(child),
            value: api::Expression::from_wit(value),
            offset: api::Expression::from_wit(offset),
            limits: limits
                .map(|(lo, hi)| [api::Expression::from_wit(lo), api::Expression::from_wit(hi)]),
            flip,
        }
    }
}

impl Mirror for api::JointChange {
    type Wit = wc::JointChange;
    fn to_wit(&self) -> wc::JointChange {
        let api::JointChange { assembly, joint } = self;
        wc::JointChange {
            assembly: assembly.0.0,
            joint: joint.to_wit(),
        }
    }
    fn from_wit(w: wc::JointChange) -> Self {
        let wc::JointChange { assembly, joint } = w;
        api::JointChange {
            assembly: AssemblyId(Id(assembly)),
            joint: api::JointDraft::from_wit(joint),
        }
    }
}

impl Mirror for api::MaterialDraft {
    type Wit = wc::MaterialDraft;
    fn to_wit(&self) -> wc::MaterialDraft {
        let api::MaterialDraft {
            id,
            name,
            density,
            color,
        } = self;
        wc::MaterialDraft {
            id: id.0.0,
            name: name.clone(),
            density: density.to_wit(),
            color: color.to_wit(),
        }
    }
    fn from_wit(w: wc::MaterialDraft) -> Self {
        let wc::MaterialDraft {
            id,
            name,
            density,
            color,
        } = w;
        api::MaterialDraft {
            id: MaterialId(Id(id)),
            name,
            density: Option::from_wit(density),
            color: api::Rgb::from_wit(color),
        }
    }
}

impl Mirror for api::AssignMaterial {
    type Wit = wc::AssignMaterial;
    fn to_wit(&self) -> wc::AssignMaterial {
        let api::AssignMaterial {
            part,
            body,
            material,
        } = self;
        wc::AssignMaterial {
            part: part.0.0,
            body: body.to_wit(),
            material: material.map(|m| m.0.0),
        }
    }
    fn from_wit(w: wc::AssignMaterial) -> Self {
        let wc::AssignMaterial {
            part,
            body,
            material,
        } = w;
        api::AssignMaterial {
            part: PartId(Id(part)),
            body: Option::from_wit(body),
            material: material.map(|m| MaterialId(Id(m))),
        }
    }
}

impl Mirror for api::PluginCommand {
    type Wit = wc::PluginCommand;
    fn to_wit(&self) -> wc::PluginCommand {
        match self {
            api::PluginCommand::AddPart(p) => wc::PluginCommand::AddPart(p.to_wit()),
            api::PluginCommand::AddFeature(f) => wc::PluginCommand::AddFeature(f.to_wit()),
            api::PluginCommand::AddComponent(c) => wc::PluginCommand::AddComponent(c.to_wit()),
            api::PluginCommand::EditComponent(c) => wc::PluginCommand::EditComponent(c.to_wit()),
            api::PluginCommand::AddJoint(j) => wc::PluginCommand::AddJoint(j.to_wit()),
            api::PluginCommand::EditJoint(j) => wc::PluginCommand::EditJoint(j.to_wit()),
            api::PluginCommand::AddMaterial(m) => wc::PluginCommand::AddMaterial(m.to_wit()),
            api::PluginCommand::AssignMaterial(m) => wc::PluginCommand::AssignMaterial(m.to_wit()),
        }
    }
    fn from_wit(w: wc::PluginCommand) -> Self {
        match w {
            wc::PluginCommand::AddPart(p) => {
                api::PluginCommand::AddPart(api::PartDraft::from_wit(p))
            }
            wc::PluginCommand::AddFeature(f) => {
                api::PluginCommand::AddFeature(api::AddFeature::from_wit(f))
            }
            wc::PluginCommand::AddComponent(c) => {
                api::PluginCommand::AddComponent(api::ComponentChange::from_wit(c))
            }
            wc::PluginCommand::EditComponent(c) => {
                api::PluginCommand::EditComponent(api::ComponentChange::from_wit(c))
            }
            wc::PluginCommand::AddJoint(j) => {
                api::PluginCommand::AddJoint(api::JointChange::from_wit(j))
            }
            wc::PluginCommand::EditJoint(j) => {
                api::PluginCommand::EditJoint(api::JointChange::from_wit(j))
            }
            wc::PluginCommand::AddMaterial(m) => {
                api::PluginCommand::AddMaterial(api::MaterialDraft::from_wit(m))
            }
            wc::PluginCommand::AssignMaterial(m) => {
                api::PluginCommand::AssignMaterial(api::AssignMaterial::from_wit(m))
            }
        }
    }
}

impl Mirror for api::CommandGroup {
    type Wit = wc::CommandGroup;
    fn to_wit(&self) -> wc::CommandGroup {
        let api::CommandGroup { label, commands } = self;
        wc::CommandGroup {
            label: label.clone(),
            commands: commands.to_wit(),
        }
    }
    fn from_wit(w: wc::CommandGroup) -> Self {
        let wc::CommandGroup { label, commands } = w;
        api::CommandGroup {
            label,
            commands: Vec::from_wit(commands),
        }
    }
}
