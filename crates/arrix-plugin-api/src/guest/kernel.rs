//! The Rust `Kernel` over the world's `kernel` import.

use super::bindings::kernel as wit_kernel;
use super::convert::Mirror;
use crate as api;
use crate::{Axis, Body, DVec3, Diagnostic, Frame, Motion, PersistentName, Profile};

/// The Rust `Kernel` over the world's imports: what a Tier 1 guest built on
/// the Rust traits would run on. Bodies are kept in a table and handed out
/// by index. Natively the imports are unreachable, so this is held equal
/// at compile time only.
#[derive(Default)]
pub struct WitKernel {
    bodies: Vec<Option<wit_kernel::Body>>,
}

impl WitKernel {
    pub fn put(&mut self, body: wit_kernel::Body) -> Body {
        self.bodies.push(Some(body));
        Body::from_handle(u32::try_from(self.bodies.len() - 1).unwrap())
    }

    pub fn get(&self, body: &Body) -> &wit_kernel::Body {
        self.bodies[body.handle() as usize].as_ref().unwrap()
    }

    pub fn take(&mut self, body: &Body) -> wit_kernel::Body {
        self.bodies[body.handle() as usize].take().unwrap()
    }
}

impl api::Kernel for WitKernel {
    fn extrude(&mut self, profile: &Profile, distance: f64) -> Result<Body, Diagnostic> {
        match wit_kernel::extrude(&profile.to_wit(), distance) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn revolve(&mut self, profile: &Profile, axis: &Axis, angle: f64) -> Result<Body, Diagnostic> {
        match wit_kernel::revolve(&profile.to_wit(), axis.to_wit(), angle) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn names(&mut self, body: &Body) -> Result<Vec<PersistentName>, Diagnostic> {
        wit_kernel::names(self.get(body))
            .map(Vec::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn face_frame(&mut self, body: &Body, face: &PersistentName) -> Result<Frame, Diagnostic> {
        wit_kernel::face_frame(self.get(body), &face.to_wit())
            .map(Frame::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn measure(&mut self, body: &Body) -> Result<api::MassProperties, Diagnostic> {
        wit_kernel::measure(self.get(body))
            .map(api::MassProperties::from_wit)
            .map_err(Diagnostic::from_wit)
    }

    fn fuse(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic> {
        match wit_kernel::fuse(self.get(target), self.get(tool)) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn cut(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic> {
        match wit_kernel::cut(self.get(target), self.get(tool)) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn common(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic> {
        match wit_kernel::common(self.get(target), self.get(tool)) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn fillet(
        &mut self,
        body: &Body,
        edges: &[PersistentName],
        radius: f64,
    ) -> Result<Body, Diagnostic> {
        match wit_kernel::fillet(self.get(body), &edges.to_vec().to_wit(), radius) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn chamfer(
        &mut self,
        body: &Body,
        edges: &[PersistentName],
        distance: f64,
    ) -> Result<Body, Diagnostic> {
        match wit_kernel::chamfer(self.get(body), &edges.to_vec().to_wit(), distance) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn copy(&mut self, index: u32, body: &Body, motion: &Motion) -> Result<Body, Diagnostic> {
        match wit_kernel::copy(index, self.get(body), motion.to_wit()) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }

    fn mirror(&mut self, body: &Body, origin: DVec3, normal: DVec3) -> Result<Body, Diagnostic> {
        match wit_kernel::mirror(self.get(body), origin.to_wit(), normal.to_wit()) {
            Ok(b) => Ok(self.put(b)),
            Err(d) => Err(Diagnostic::from_wit(d)),
        }
    }
}
