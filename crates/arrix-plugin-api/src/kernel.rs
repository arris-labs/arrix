//! The kernel host service a plugin imports (the world's `kernel`).

use arrix_core::{DVec3, Diagnostic, Frame, PersistentName, Profile};

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

    /// Every face, edge and vertex of `body`, by name.
    fn names(&mut self, body: &Body) -> Result<Vec<PersistentName>, Diagnostic>;

    /// The outward frame of the planar face `face` of `body`.
    fn face_frame(&mut self, body: &Body, face: &PersistentName) -> Result<Frame, Diagnostic>;

    /// Volume, area and centroid of `body`.
    fn measure(&mut self, body: &Body) -> Result<MassProperties, Diagnostic>;
}
