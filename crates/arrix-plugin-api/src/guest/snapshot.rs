//! The Rust `Snapshot` over the world's `document` import, and the types
//! of `document` and `exchange` converted both ways. Natively the imports
//! are unreachable, so [`WitSnapshot`] is held equal at compile time only
//! (`tests/wit_equality.rs`).

use super::bindings::document as wit_doc;
use super::bindings::exchange as wx;
use super::convert::Mirror;
use crate as api;
use crate::{
    AssemblyId, ComponentId, DVec3, Diagnostic, FeatureId, Frame, Id, JointId, PartId, SlotName,
};

/// The Rust `Snapshot` over the world's imports. A guest that names no
/// exporter never builds one, so its component does not import `document`
/// (ADR-0037 §2).
#[derive(Default)]
pub struct WitSnapshot;

impl api::Snapshot for WitSnapshot {
    fn parts(&mut self) -> Vec<api::PartInfo> {
        Vec::from_wit(wit_doc::parts())
    }

    fn mesh(&mut self, part: PartId, body: &api::BodyRef) -> Result<api::Triangles, Diagnostic> {
        wit_doc::mesh(part.0.0, &body.to_wit())
            .map(api::Triangles::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn mass(&mut self, part: PartId) -> Result<api::Inertial, Diagnostic> {
        wit_doc::mass(part.0.0)
            .map(api::Inertial::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn assemblies(&mut self) -> Vec<api::AssemblyInfo> {
        Vec::from_wit(wit_doc::assemblies())
    }
}

fn path_to_wit(path: &[ComponentId]) -> Vec<u64> {
    path.iter().map(|c| c.0.0).collect()
}

fn path_from_wit(path: Vec<u64>) -> Vec<ComponentId> {
    path.into_iter().map(|c| ComponentId(Id(c))).collect()
}

impl Mirror for api::Rgb {
    type Wit = wit_doc::Rgb;
    fn to_wit(&self) -> wit_doc::Rgb {
        let api::Rgb { red, green, blue } = *self;
        wit_doc::Rgb { red, green, blue }
    }
    fn from_wit(w: wit_doc::Rgb) -> Self {
        let wit_doc::Rgb { red, green, blue } = w;
        api::Rgb { red, green, blue }
    }
}

impl Mirror for api::MaterialInfo {
    type Wit = wit_doc::MaterialInfo;
    fn to_wit(&self) -> wit_doc::MaterialInfo {
        let api::MaterialInfo { name, color } = self;
        wit_doc::MaterialInfo {
            name: name.clone(),
            color: color.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::MaterialInfo) -> Self {
        let wit_doc::MaterialInfo { name, color } = w;
        api::MaterialInfo {
            name,
            color: api::Rgb::from_wit(color),
        }
    }
}

impl Mirror for api::BodyRef {
    type Wit = wit_doc::BodyRef;
    fn to_wit(&self) -> wit_doc::BodyRef {
        let api::BodyRef { feature, slot } = self;
        wit_doc::BodyRef {
            feature: feature.0.0,
            slot: slot.to_string(),
        }
    }
    fn from_wit(w: wit_doc::BodyRef) -> Self {
        let wit_doc::BodyRef { feature, slot } = w;
        api::BodyRef {
            feature: FeatureId(Id(feature)),
            slot: SlotName::new(slot).unwrap(),
        }
    }
}

impl Mirror for api::BodyInfo {
    type Wit = wit_doc::BodyInfo;
    fn to_wit(&self) -> wit_doc::BodyInfo {
        let api::BodyInfo { body, material } = self;
        wit_doc::BodyInfo {
            body: body.to_wit(),
            material: material.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::BodyInfo) -> Self {
        let wit_doc::BodyInfo { body, material } = w;
        api::BodyInfo {
            body: api::BodyRef::from_wit(body),
            material: Option::from_wit(material),
        }
    }
}

impl Mirror for api::FeatureInfo {
    type Wit = wit_doc::FeatureInfo;
    fn to_wit(&self) -> wit_doc::FeatureInfo {
        let api::FeatureInfo {
            id,
            type_id,
            name,
            choices,
            params,
        } = self;
        wit_doc::FeatureInfo {
            id: id.0.0,
            type_id: type_id.clone(),
            name: name.clone(),
            choices: choices.to_wit(),
            params: params.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::FeatureInfo) -> Self {
        let wit_doc::FeatureInfo {
            id,
            type_id,
            name,
            choices,
            params,
        } = w;
        api::FeatureInfo {
            id: FeatureId(Id(id)),
            type_id,
            name,
            choices: Vec::from_wit(choices),
            params: Vec::from_wit(params),
        }
    }
}

impl Mirror for api::PartInfo {
    type Wit = wit_doc::PartInfo;
    fn to_wit(&self) -> wit_doc::PartInfo {
        let api::PartInfo {
            id,
            name,
            bodies,
            features,
        } = self;
        wit_doc::PartInfo {
            id: id.0.0,
            name: name.clone(),
            bodies: bodies.to_wit(),
            features: features.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::PartInfo) -> Self {
        let wit_doc::PartInfo {
            id,
            name,
            bodies,
            features,
        } = w;
        api::PartInfo {
            id: PartId(Id(id)),
            name,
            bodies: Vec::from_wit(bodies),
            features: Vec::from_wit(features),
        }
    }
}

impl Mirror for api::Triangles {
    type Wit = wit_doc::Triangles;
    fn to_wit(&self) -> wit_doc::Triangles {
        let api::Triangles {
            positions,
            normals,
            indices,
        } = self;
        wit_doc::Triangles {
            positions: positions.clone(),
            normals: normals.clone(),
            indices: indices.clone(),
        }
    }
    fn from_wit(w: wit_doc::Triangles) -> Self {
        let wit_doc::Triangles {
            positions,
            normals,
            indices,
        } = w;
        api::Triangles {
            positions,
            normals,
            indices,
        }
    }
}

impl Mirror for api::InertiaTensor {
    type Wit = wit_doc::InertiaTensor;
    fn to_wit(&self) -> wit_doc::InertiaTensor {
        let api::InertiaTensor {
            xx,
            xy,
            xz,
            yy,
            yz,
            zz,
        } = *self;
        wit_doc::InertiaTensor {
            xx,
            xy,
            xz,
            yy,
            yz,
            zz,
        }
    }
    fn from_wit(w: wit_doc::InertiaTensor) -> Self {
        let wit_doc::InertiaTensor {
            xx,
            xy,
            xz,
            yy,
            yz,
            zz,
        } = w;
        api::InertiaTensor {
            xx,
            xy,
            xz,
            yy,
            yz,
            zz,
        }
    }
}

impl Mirror for api::Inertial {
    type Wit = wit_doc::Inertial;
    fn to_wit(&self) -> wit_doc::Inertial {
        let api::Inertial {
            mass,
            centroid,
            inertia,
        } = *self;
        wit_doc::Inertial {
            mass,
            centroid: centroid.to_wit(),
            inertia: inertia.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::Inertial) -> Self {
        let wit_doc::Inertial {
            mass,
            centroid,
            inertia,
        } = w;
        api::Inertial {
            mass,
            centroid: DVec3::from_wit(centroid),
            inertia: api::InertiaTensor::from_wit(inertia),
        }
    }
}

impl Mirror for api::JointKind {
    type Wit = wit_doc::JointKind;
    fn to_wit(&self) -> wit_doc::JointKind {
        match self {
            api::JointKind::Fixed => wit_doc::JointKind::Fixed,
            api::JointKind::Revolute => wit_doc::JointKind::Revolute,
            api::JointKind::Prismatic => wit_doc::JointKind::Prismatic,
        }
    }
    fn from_wit(w: wit_doc::JointKind) -> Self {
        match w {
            wit_doc::JointKind::Fixed => api::JointKind::Fixed,
            wit_doc::JointKind::Revolute => api::JointKind::Revolute,
            wit_doc::JointKind::Prismatic => api::JointKind::Prismatic,
        }
    }
}

impl Mirror for api::ComponentOf {
    type Wit = wit_doc::ComponentOf;
    fn to_wit(&self) -> wit_doc::ComponentOf {
        match self {
            api::ComponentOf::Part(p) => wit_doc::ComponentOf::Part(p.0.0),
            api::ComponentOf::Assembly(a) => wit_doc::ComponentOf::Assembly(a.0.0),
        }
    }
    fn from_wit(w: wit_doc::ComponentOf) -> Self {
        match w {
            wit_doc::ComponentOf::Part(p) => api::ComponentOf::Part(PartId(Id(p))),
            wit_doc::ComponentOf::Assembly(a) => api::ComponentOf::Assembly(AssemblyId(Id(a))),
        }
    }
}

impl Mirror for api::ComponentInfo {
    type Wit = wit_doc::ComponentInfo;
    fn to_wit(&self) -> wit_doc::ComponentInfo {
        let api::ComponentInfo {
            path,
            name,
            of,
            pose,
            diagnostics,
        } = self;
        wit_doc::ComponentInfo {
            path: path_to_wit(path),
            name: name.clone(),
            of: of.to_wit(),
            pose: pose.to_wit(),
            diagnostics: diagnostics.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::ComponentInfo) -> Self {
        let wit_doc::ComponentInfo {
            path,
            name,
            of,
            pose,
            diagnostics,
        } = w;
        api::ComponentInfo {
            path: path_from_wit(path),
            name,
            of: api::ComponentOf::from_wit(of),
            pose: Frame::from_wit(pose),
            diagnostics: Vec::from_wit(diagnostics),
        }
    }
}

impl Mirror for api::JointPlacement {
    type Wit = wit_doc::JointPlacement;
    fn to_wit(&self) -> wit_doc::JointPlacement {
        let api::JointPlacement {
            parent_anchor,
            child_anchor,
            value,
            diagnostics,
        } = self;
        wit_doc::JointPlacement {
            parent_anchor: parent_anchor.to_wit(),
            child_anchor: child_anchor.to_wit(),
            value: *value,
            diagnostics: diagnostics.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::JointPlacement) -> Self {
        let wit_doc::JointPlacement {
            parent_anchor,
            child_anchor,
            value,
            diagnostics,
        } = w;
        api::JointPlacement {
            parent_anchor: Frame::from_wit(parent_anchor),
            child_anchor: Frame::from_wit(child_anchor),
            value,
            diagnostics: Vec::from_wit(diagnostics),
        }
    }
}

impl Mirror for api::JointState {
    type Wit = wit_doc::JointState;
    fn to_wit(&self) -> wit_doc::JointState {
        match self {
            api::JointState::Placed(p) => wit_doc::JointState::Placed(p.to_wit()),
            api::JointState::Failed(d) => wit_doc::JointState::Failed(d.to_wit()),
        }
    }
    fn from_wit(w: wit_doc::JointState) -> Self {
        match w {
            wit_doc::JointState::Placed(p) => {
                api::JointState::Placed(api::JointPlacement::from_wit(p))
            }
            wit_doc::JointState::Failed(d) => api::JointState::Failed(Diagnostic::from_wit(d)),
        }
    }
}

impl Mirror for api::JointInfo {
    type Wit = wit_doc::JointInfo;
    fn to_wit(&self) -> wit_doc::JointInfo {
        let api::JointInfo {
            id,
            name,
            kind,
            parent,
            child,
            limits,
            state,
        } = self;
        wit_doc::JointInfo {
            id: id.0.0,
            name: name.clone(),
            kind: kind.to_wit(),
            parent: path_to_wit(parent),
            child: path_to_wit(child),
            limits: limits.map(|[lo, hi]| (lo, hi)),
            state: state.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::JointInfo) -> Self {
        let wit_doc::JointInfo {
            id,
            name,
            kind,
            parent,
            child,
            limits,
            state,
        } = w;
        api::JointInfo {
            id: JointId(Id(id)),
            name,
            kind: api::JointKind::from_wit(kind),
            parent: path_from_wit(parent),
            child: path_from_wit(child),
            limits: limits.map(|(lo, hi)| [lo, hi]),
            state: api::JointState::from_wit(state),
        }
    }
}

impl Mirror for api::AssemblyInfo {
    type Wit = wit_doc::AssemblyInfo;
    fn to_wit(&self) -> wit_doc::AssemblyInfo {
        let api::AssemblyInfo {
            id,
            name,
            components,
            joints,
        } = self;
        wit_doc::AssemblyInfo {
            id: id.0.0,
            name: name.clone(),
            components: components.to_wit(),
            joints: joints.to_wit(),
        }
    }
    fn from_wit(w: wit_doc::AssemblyInfo) -> Self {
        let wit_doc::AssemblyInfo {
            id,
            name,
            components,
            joints,
        } = w;
        api::AssemblyInfo {
            id: AssemblyId(Id(id)),
            name,
            components: Vec::from_wit(components),
            joints: Vec::from_wit(joints),
        }
    }
}

// ---- exchange -----------------------------------------------------------------

impl Mirror for api::SubjectKind {
    type Wit = wx::SubjectKind;
    fn to_wit(&self) -> wx::SubjectKind {
        match self {
            api::SubjectKind::Part => wx::SubjectKind::Part,
            api::SubjectKind::Assembly => wx::SubjectKind::Assembly,
        }
    }
    fn from_wit(w: wx::SubjectKind) -> Self {
        match w {
            wx::SubjectKind::Part => api::SubjectKind::Part,
            wx::SubjectKind::Assembly => api::SubjectKind::Assembly,
        }
    }
}

impl Mirror for api::Subject {
    type Wit = wx::Subject;
    fn to_wit(&self) -> wx::Subject {
        match self {
            api::Subject::Part(p) => wx::Subject::Part(p.0.0),
            api::Subject::Assembly(a) => wx::Subject::Assembly(a.0.0),
        }
    }
    fn from_wit(w: wx::Subject) -> Self {
        match w {
            wx::Subject::Part(p) => api::Subject::Part(PartId(Id(p))),
            wx::Subject::Assembly(a) => api::Subject::Assembly(AssemblyId(Id(a))),
        }
    }
}

impl Mirror for api::ExporterSpec {
    type Wit = wx::ExporterSpec;
    fn to_wit(&self) -> wx::ExporterSpec {
        let api::ExporterSpec {
            id,
            title,
            extension,
            subject,
            params,
            choices,
        } = self;
        wx::ExporterSpec {
            id: id.clone(),
            title: title.clone(),
            extension: extension.clone(),
            subject: subject.to_wit(),
            params: params.to_wit(),
            choices: choices.to_wit(),
        }
    }
    fn from_wit(w: wx::ExporterSpec) -> Self {
        let wx::ExporterSpec {
            id,
            title,
            extension,
            subject,
            params,
            choices,
        } = w;
        api::ExporterSpec {
            id,
            title,
            extension,
            subject: api::SubjectKind::from_wit(subject),
            params: Vec::from_wit(params),
            choices: Vec::from_wit(choices),
        }
    }
}

impl Mirror for api::ExportFile {
    type Wit = wx::File;
    fn to_wit(&self) -> wx::File {
        let api::ExportFile { name, bytes } = self;
        wx::File {
            name: name.clone(),
            bytes: bytes.clone(),
        }
    }
    fn from_wit(w: wx::File) -> Self {
        let wx::File { name, bytes } = w;
        api::ExportFile { name, bytes }
    }
}
