//! The evaluated document, read (the world's `document`; ADR-0037;
//! docs/PLUGINS.md §One interface). Plain data in SI, Z-up, with
//! `arrix-core`'s ids, and a [`Snapshot`] to fetch it by function.

use arrix_core::{
    AssemblyId, ComponentId, DVec3, Diagnostic, FeatureId, Frame, JointId, PartId, SlotName,
};

/// A colour: red, green and blue in 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialInfo {
    pub name: String,
    pub color: Rgb,
}

/// A final body of a part: the body slot a feature made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyRef {
    pub feature: FeatureId,
    pub slot: SlotName,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyInfo {
    pub body: BodyRef,
    /// None for a body of a part with no material.
    pub material: Option<MaterialInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartInfo {
    pub id: PartId,
    pub name: String,
    pub bodies: Vec<BodyInfo>,
}

/// A refined mesh in the part's frame, chord 1e-4 m (ADR-0037 §4): three
/// `f32` per vertex in `positions` and `normals`, three `u32` per triangle
/// in `indices`, counter-clockwise seen from outside.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Triangles {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
}

/// A symmetric inertia tensor, the entries of the matrix (`xy` is the
/// negated product of inertia), in kg m².
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InertiaTensor {
    pub xx: f64,
    pub xy: f64,
    pub xz: f64,
    pub yy: f64,
    pub yz: f64,
    pub zz: f64,
}

/// Mass in kg, the centre of mass in the part's frame, and the inertia
/// about it with the part's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inertial {
    pub mass: f64,
    pub centroid: DVec3,
    pub inertia: InertiaTensor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JointKind {
    Fixed,
    Revolute,
    Prismatic,
}

/// What a component places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentOf {
    Part(PartId),
    Assembly(AssemblyId),
}

/// A component where its assembly places it.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentInfo {
    /// The component ids from the assembly down; a nested assembly's own
    /// components follow it.
    pub path: Vec<ComponentId>,
    pub name: String,
    pub of: ComponentOf,
    /// Where the origin and axes of its part or nested assembly land, in
    /// the assembly's frame.
    pub pose: Frame,
    /// Why it is not where it was authored to be (`assembly.unplaced`).
    pub diagnostics: Vec<Diagnostic>,
}

/// Where a solved joint put its anchors, each in the frame of the
/// component its path ends at. The motion is about their z axes
/// (ADR-0029 §3).
#[derive(Clone, Debug, PartialEq)]
pub struct JointPlacement {
    pub parent_anchor: Frame,
    pub child_anchor: Frame,
    /// Value plus offset, in radians or metres; zero for a fixed joint.
    pub value: f64,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JointState {
    Placed(JointPlacement),
    Failed(Diagnostic),
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointInfo {
    pub id: JointId,
    pub name: String,
    pub kind: JointKind,
    /// The paths of the components the anchors are on, from the assembly
    /// down.
    pub parent: Vec<ComponentId>,
    pub child: Vec<ComponentId>,
    /// Lower and upper, in SI.
    pub limits: Option<[f64; 2]>,
    pub state: JointState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyInfo {
    pub id: AssemblyId,
    pub name: String,
    pub components: Vec<ComponentInfo>,
    pub joints: Vec<JointInfo>,
}

/// The evaluated document, as an exporter reads it: valid for one export,
/// against the generation it was made on. Every read is pure.
pub trait Snapshot {
    fn parts(&mut self) -> Vec<PartInfo>;

    /// The refined mesh of one final body of `part`.
    fn mesh(&mut self, part: PartId, body: &BodyRef) -> Result<Triangles, Diagnostic>;

    /// The part's mass properties, or the `mass.*` diagnostic.
    fn mass(&mut self, part: PartId) -> Result<Inertial, Diagnostic>;

    fn assemblies(&mut self) -> Vec<AssemblyInfo>;
}
