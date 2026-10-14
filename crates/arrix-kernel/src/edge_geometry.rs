//! An edge's geometry as a plugin command reads it (ADR-0038 §3): its
//! curve and the outward normals of the planar faces it bounds.

use arris::geom::{Curve, Surface};
use arris::topo::EntityId;
use arrix_core::{DVec3, PersistentName, TopoKind};

use crate::{Kernel, KernelBody, KernelError, KernelOp, convert};

/// An edge's curve, in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeCurve {
    /// A circle as Arris orients it: its centre, unit normal and radius.
    Circle {
        centre: DVec3,
        axis: DVec3,
        radius: f64,
    },
    /// A straight edge: its two ends.
    Line { start: DVec3, end: DVec3 },
    /// Any other curve, or none.
    Other,
}

/// What [`Kernel::edge_geometry`] answers.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeGeometry {
    pub curve: EdgeCurve,
    /// The outward normals of the planar faces the edge bounds, in the
    /// order of the faces' names.
    pub face_normals: Vec<DVec3>,
}

impl Kernel {
    /// The geometry of the edge `edge` of `body`, in the world: a recorded
    /// call, so a plugin command reads a hole's size and side without a
    /// kernel call of its own. A face of the edge that is not planar has
    /// no normal listed.
    pub fn edge_geometry(
        &mut self,
        body: KernelBody,
        edge: &PersistentName,
    ) -> Result<EdgeGeometry, KernelError> {
        let entry = self.entry(body)?;
        let target = entry.body;
        let id = match entry.names.entity(edge) {
            Some(EntityId::Edge(id)) => id,
            Some(_) => {
                return Err(KernelError::WrongKind {
                    name: edge.clone(),
                    wanted: TopoKind::Edge,
                    found: edge.kind,
                });
            }
            None => return Err(KernelError::UnknownName { name: edge.clone() }),
        };
        let faces: Vec<_> = entry
            .names
            .pairs()
            .filter_map(|(name, entity)| match entity {
                EntityId::Face(f) => Some((name.clone(), f)),
                _ => None,
            })
            .collect();
        let call = self.begin(KernelOp::EdgeGeometry { edge: edge.clone() }, &[body])?;
        let op = |e: arris::topo::NotFound| KernelError::op(call, &e.into());
        let edge_view = self.model.edge(id).map_err(op)?;
        let curve = match edge_view
            .curve()
            .and_then(|(curve, _)| self.model.curve(curve).ok())
        {
            Some(Curve::Circle { frame, radius }) => EdgeCurve::Circle {
                centre: convert::dpoint3(frame.origin()),
                axis: convert::dvec3(frame.z().into_inner()),
                radius: *radius,
            },
            Some(Curve::Line { .. }) => {
                let end = |v| {
                    self.model
                        .vertex(v)
                        .map(|v| convert::dpoint3(v.point()))
                        .map_err(|e| KernelError::op(call, &e.into()))
                };
                EdgeCurve::Line {
                    start: end(edge_view.start())?,
                    end: end(edge_view.end())?,
                }
            }
            _ => EdgeCurve::Other,
        };
        let mut face_normals = Vec::new();
        for (name, face_id) in faces {
            let face = self
                .model
                .face(face_id)
                .map_err(|e| KernelError::op(call, &e.into()))?;
            let bounded = face
                .loops()
                .iter()
                .flat_map(|l| l.coedges())
                .any(|c| c.edge() == id);
            if !bounded {
                continue;
            }
            let planar = matches!(
                self.model
                    .surface(face.surface())
                    .map_err(|e| KernelError::op(call, &e.into()))?,
                Surface::Plane { .. }
            );
            if planar {
                let frame = self.outward_frame(call, target, face_id, &name)?;
                face_normals.push(convert::dvec3(frame.z().into_inner()));
            }
        }
        Ok(EdgeGeometry {
            curve,
            face_normals,
        })
    }
}
