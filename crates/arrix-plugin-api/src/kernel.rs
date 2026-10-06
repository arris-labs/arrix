//! The kernel host service a plugin imports (the world's `kernel`).

use arrix_core::{Axis, DVec3, Diagnostic, Frame, Motion, PersistentName, Profile};

/// A body made in one evaluation: an opaque handle, valid only in the
/// `Kernel` that returned it. It is not `Clone`: like the world's `own<body>`
/// resource, a handle placed in an output slot is given up.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Body(u32);

impl Body {
    /// The host's handle. A plugin only ever receives bodies from a `Kernel`.
    pub fn from_handle(handle: u32) -> Self {
        Self(handle)
    }

    pub fn handle(&self) -> u32 {
        self.0
    }
}

/// A body's mass properties at unit density, in SI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    /// Cubic metres.
    pub volume: f64,
    /// Square metres.
    pub area: f64,
    pub centroid: DVec3,
}

/// Kernel operations with ArriX's signatures. The host implements it over
/// `arrix-kernel`; every call is recorded, and everything built is named as
/// the evaluating feature's (docs/PLUGINS.md §Features, "Naming").
pub trait Kernel {
    /// `profile` swept `distance` metres along its plane's normal, or
    /// against it when negative.
    fn extrude(&mut self, profile: &Profile, distance: f64) -> Result<Body, Diagnostic>;

    /// `profile` turned `angle` radians (at most a full turn) about `axis`,
    /// in the world. Counted with the extrudes in the feature's sweep
    /// numbering (ADR-0011).
    fn revolve(&mut self, profile: &Profile, axis: &Axis, angle: f64) -> Result<Body, Diagnostic>;

    /// Every face, edge and vertex of `body`, by name.
    fn names(&mut self, body: &Body) -> Result<Vec<PersistentName>, Diagnostic>;

    /// The outward frame of the planar face `face` of `body`.
    fn face_frame(&mut self, body: &Body, face: &PersistentName) -> Result<Frame, Diagnostic>;

    /// Volume, area and centroid of `body`.
    fn measure(&mut self, body: &Body) -> Result<MassProperties, Diagnostic>;

    /// `target` joined with `tool`: a new body, the operands left as they
    /// were. What the union left alone keeps its name.
    fn fuse(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic>;

    /// `target` less `tool`, as [`fuse`](Kernel::fuse).
    fn cut(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic>;

    /// What `target` and `tool` have in common, as [`fuse`](Kernel::fuse).
    fn common(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic>;

    /// `body` with `edges` rounded at `radius` metres. The faces the blend
    /// trims keep their names; what it made is named from the edges. A
    /// feature's booleans and blends are numbered in call order, which the
    /// host counts (ADR-0014).
    fn fillet(
        &mut self,
        body: &Body,
        edges: &[PersistentName],
        radius: f64,
    ) -> Result<Body, Diagnostic>;

    /// `body` with `edges` cut flat at `distance` metres, as
    /// [`fillet`](Kernel::fillet).
    fn chamfer(
        &mut self,
        body: &Body,
        edges: &[PersistentName],
        distance: f64,
    ) -> Result<Body, Diagnostic>;

    /// `body` moved by `motion`, every entity named as copy `index` of the
    /// calling feature, `index` from 1: raising a pattern's count renames
    /// no copy that stays (ADR-0010). `body` stays valid.
    fn copy(&mut self, index: u32, body: &Body, motion: &Motion) -> Result<Body, Diagnostic>;

    /// `body` reflected in the plane through `origin` with `normal`, named
    /// as copy 1 of the calling feature. `body` stays valid.
    fn mirror(&mut self, body: &Body, origin: DVec3, normal: DVec3) -> Result<Body, Diagnostic>;
}
