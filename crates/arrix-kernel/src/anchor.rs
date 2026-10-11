//! An assembly anchor's frame (ADR-0029 §3): the frame a joint attaches
//! at, from an axis entity of a part's final body and an optional origin
//! face.

use arris::geom::{Curve, Surface};
use arris::topo::{Body, EntityId, FaceId};
use arrix_core::{DVec3, Frame, PersistentName, TopoKind};

use crate::{CallIndex, Kernel, KernelBody, KernelError, KernelOp, convert};

impl Kernel {
    /// The frame an anchor names on `body` (ADR-0029 §3). `axis` gives z
    /// and a point:
    ///
    /// - a cylindrical face: its axis as Arris orients the surface, at the
    ///   face's lowest extent along it (the axis point nearest its vertex
    ///   least far along z), so the point does not depend on how the
    ///   cylinder was made;
    /// - a planar face: its outward normal at the origin of its frame
    ///   ([`face_frame`](Kernel::face_frame));
    /// - a circular edge: its circle's normal as Arris orients it, at its
    ///   centre;
    /// - a straight edge: its line as Arris orients it, at the edge's
    ///   start along it.
    ///
    /// `origin`, a planar face, moves the point to where the axis meets its
    /// plane. x is the origin face's u projected square to z, else the
    /// world axis least aligned with z (X before Y before Z) projected.
    /// Any other face or edge (a cone, a B-spline cylinder, an ellipse),
    /// a non-planar origin face, or an axis parallel to the origin's plane
    /// is `kernel.invalid-input`, naming the entity; the call is recorded.
    pub fn anchor_frame(
        &mut self,
        body: KernelBody,
        axis: &PersistentName,
        origin: Option<&PersistentName>,
    ) -> Result<Frame, KernelError> {
        let axis_id = self.named(body, axis, None)?;
        let origin_id = match origin {
            Some(name) => Some(self.face_named(body, name)?),
            None => None,
        };
        let target = self.entry(body)?.body;
        let call = self.begin(
            KernelOp::AnchorFrame {
                axis: axis.clone(),
                origin: origin.cloned(),
            },
            &[body],
        )?;
        let (z, point) = match axis_id {
            EntityId::Face(id) => self.face_axis(call, target, id, axis)?,
            EntityId::Edge(id) => self.edge_axis_point(call, id, axis)?,
            _ => unreachable!("`named` admits faces and edges alone"),
        };
        let (point, u) = match (origin, origin_id) {
            (Some(name), Some(id)) => {
                let plane = self.planar_frame(call, target, id, name)?;
                let normal = convert::dvec3(plane.z().into_inner());
                let across = z.dot(normal);
                if across.abs() < 1e-9 {
                    return Err(KernelError::invalid_input(
                        call,
                        format!("the axis {axis} runs parallel to the origin face {name}"),
                    ));
                }
                let t = (convert::dpoint3(plane.origin()) - point).dot(normal) / across;
                let u = convert::dvec3(plane.x().into_inner());
                (point + z * t, Some(u))
            }
            _ => (point, None),
        };
        let x = u
            .and_then(|u| square_to(u, z))
            .unwrap_or_else(|| least_aligned(z));
        Frame::new(point, x, z).map_err(|e| KernelError::invalid_input(call, e.to_string()))
    }

    /// The face or edge `name` names in `body`; a vertex is
    /// `kernel.wrong-kind`, as a face was wanted.
    fn named(
        &self,
        body: KernelBody,
        name: &PersistentName,
        only: Option<TopoKind>,
    ) -> Result<EntityId, KernelError> {
        match self.entry(body)?.names.entity(name) {
            Some(id @ EntityId::Face(_)) if only != Some(TopoKind::Edge) => Ok(id),
            Some(id @ EntityId::Edge(_)) if only.is_none() => Ok(id),
            Some(_) => Err(KernelError::WrongKind {
                name: name.clone(),
                wanted: only.unwrap_or(TopoKind::Face),
                found: name.kind,
            }),
            None => Err(KernelError::UnknownName { name: name.clone() }),
        }
    }

    fn face_named(&self, body: KernelBody, name: &PersistentName) -> Result<FaceId, KernelError> {
        match self.named(body, name, Some(TopoKind::Face))? {
            EntityId::Face(id) => Ok(id),
            _ => unreachable!("only faces are admitted"),
        }
    }

    /// A cylindrical face's axis at the face's lowest extent along it, or
    /// a planar face's outward normal at its frame's origin.
    fn face_axis(
        &self,
        call: CallIndex,
        body: Body,
        id: FaceId,
        name: &PersistentName,
    ) -> Result<(DVec3, DVec3), KernelError> {
        let face = self
            .model
            .face(id)
            .map_err(|e| KernelError::op(call, &e.into()))?;
        let surface = self
            .model
            .surface(face.surface())
            .map_err(|e| KernelError::op(call, &e.into()))?;
        match surface {
            Surface::Cylinder { frame, .. } => {
                let origin = convert::dpoint3(frame.origin());
                let z = convert::dvec3(frame.z().into_inner());
                let mut lowest = f64::INFINITY;
                for coedge in face.loops().iter().flat_map(|l| l.coedges()) {
                    let edge = self
                        .model
                        .edge(coedge.edge())
                        .map_err(|e| KernelError::op(call, &e.into()))?;
                    for v in [edge.start(), edge.end()] {
                        let p = self
                            .model
                            .vertex(v)
                            .map_err(|e| KernelError::op(call, &e.into()))?
                            .point();
                        lowest = lowest.min((convert::dpoint3(p) - origin).dot(z));
                    }
                }
                if !lowest.is_finite() {
                    return Err(KernelError::invalid_input(
                        call,
                        format!("the cylindrical face {name} has no vertex to place its point"),
                    ));
                }
                Ok((z, origin + z * lowest))
            }
            Surface::Plane { .. } => {
                let frame = self.planar_frame(call, body, id, name)?;
                Ok((
                    convert::dvec3(frame.z().into_inner()),
                    convert::dpoint3(frame.origin()),
                ))
            }
            other => Err(KernelError::invalid_input(
                call,
                format!(
                    "{name} is a {:?} face; an anchor's axis is a cylindrical or planar face, \
                     or a circular or straight edge",
                    other.kind()
                ),
            )),
        }
    }

    /// A straight edge's line at its start along it, or a circular edge's
    /// normal at its centre.
    fn edge_axis_point(
        &self,
        call: CallIndex,
        id: arris::topo::EdgeId,
        name: &PersistentName,
    ) -> Result<(DVec3, DVec3), KernelError> {
        let edge = self
            .model
            .edge(id)
            .map_err(|e| KernelError::op(call, &e.into()))?;
        let curve = edge
            .curve()
            .and_then(|(curve, range)| Some((self.model.curve(curve).ok()?, range)));
        match curve {
            Some((Curve::Line { origin, direction }, range)) => {
                let z = convert::dvec3(direction.into_inner());
                Ok((z, convert::dpoint3(*origin) + z * range.lo()))
            }
            Some((Curve::Circle { frame, .. }, _)) => Ok((
                convert::dvec3(frame.z().into_inner()),
                convert::dpoint3(frame.origin()),
            )),
            Some((other, _)) => Err(KernelError::invalid_input(
                call,
                format!(
                    "{name} is a {:?} edge; an anchor's axis is a cylindrical or planar face, \
                     or a circular or straight edge",
                    other.kind()
                ),
            )),
            None => Err(KernelError::invalid_input(
                call,
                format!("{name} is a degenerate edge, with no curve"),
            )),
        }
    }
}

impl Kernel {
    /// The frame of the planar face `id` of `body`, outward; any other
    /// face is `kernel.invalid-input`.
    fn planar_frame(
        &self,
        call: CallIndex,
        body: Body,
        id: FaceId,
        name: &PersistentName,
    ) -> Result<arris::math::Frame, KernelError> {
        let face = self
            .model
            .face(id)
            .map_err(|e| KernelError::op(call, &e.into()))?;
        let surface = self
            .model
            .surface(face.surface())
            .map_err(|e| KernelError::op(call, &e.into()))?;
        if !matches!(surface, Surface::Plane { .. }) {
            return Err(KernelError::invalid_input(
                call,
                format!("{name} is a {:?} face, not a planar one", surface.kind()),
            ));
        }
        self.outward_frame(call, body, id, name)
    }

    /// Arris's frame of the face `id` as `body` uses it, so its normal
    /// points out of the material.
    pub(crate) fn outward_frame(
        &self,
        call: CallIndex,
        body: Body,
        id: FaceId,
        name: &PersistentName,
    ) -> Result<arris::math::Frame, KernelError> {
        let used = self
            .model
            .faces(body)
            .map_err(|e| KernelError::op(call, &e.into()))?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| KernelError::Naming {
                call,
                message: format!("{name} is not a face of the body it was named in"),
            })?;
        arris::ops::query::face_frame(&self.model, used).map_err(|e| KernelError::op(call, &e))
    }
}

/// `v` with its part along unit `z` taken away, normalised; `None` when
/// nothing is left.
fn square_to(v: DVec3, z: DVec3) -> Option<DVec3> {
    let v = v - z * v.dot(z);
    (v.length() > 1e-9).then(|| v.normalize())
}

/// The world axis least aligned with unit `z`, X before Y before Z on a
/// tie, projected square to it.
fn least_aligned(z: DVec3) -> DVec3 {
    let mut best = DVec3::X;
    for axis in [DVec3::Y, DVec3::Z] {
        if axis.dot(z).abs() < best.dot(z).abs() {
            best = axis;
        }
    }
    square_to(best, z).expect("a world axis least aligned with a unit z is not along it")
}
