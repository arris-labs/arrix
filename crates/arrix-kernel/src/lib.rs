//! The only crate naming Arris types: operations, naming from provenance,
//! render meshes, body bytes and call records (docs/ARCHITECTURE.md §The
//! kernel choke point).
//!
//! Built so far: the `Kernel` service's first slice (`extrude`, `revolve`,
//! `fuse`, `cut`, `common`, `fillet`, `chamfer`, `copy`, `mirror`,
//! `project`, `face_frame`, `mass_properties`, `tessellate`, `to_bytes`,
//! `from_bytes`),
//! names from the sweeps' and the booleans' provenance, a
//! `KernelCall` record made before every call, and a record's export as
//! an Arris fixture recipe (`FixtureRecipe`).

mod bytes;
mod convert;
mod error;
mod fixture;
mod mesh;
mod naming;
mod project;
mod record;

use arris::Control;
use arris::io::body::BodyError;
use arris::math::Precision;
use arris::topo::{Body, EntityId, Model};
use arrix_core::{Axis, DVec3, FeatureId, Frame, PersistentName, Profile, TopoKind};

pub use arrix_core::Motion;
pub use bytes::{BodyBytes, NameEntry, NameTable};
pub use error::KernelError;
pub use fixture::{FixtureError, FixtureRecipe};
pub use naming::BodyNames;
pub use project::Projected;
pub use record::{ARRIS_VERSION, BodyHash, CallIndex, CallPrecision, KernelCall, KernelOp};

/// Arris's `fillet` and `chamfer`, one signature.
type BlendOp = fn(
    &mut Model,
    Body,
    &[arris::topo::Edge],
    f64,
    &Control<'_>,
) -> Result<(Body, arris::topo::Provenance), arris::ops::OpError>;

/// A body made during one evaluation, opaque outside this crate and valid
/// only in the `Kernel` that returned it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KernelBody(u32);

/// A body's mass properties at unit density, in SI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    /// Cubic metres.
    pub volume: f64,
    /// Square metres.
    pub area: f64,
    pub centroid: DVec3,
}

struct BodyEntry {
    pub(crate) body: Body,
    pub(crate) names: BodyNames,
    /// Of its canonical bytes, once an operand needed it; a body never
    /// changes, so it is made once.
    hash: Option<BodyHash>,
    /// The call that made it.
    made_by: CallIndex,
}

/// A kernel context: an Arris model, the bodies made in it, and the
/// record of every call, in order. The evaluator keeps one across
/// evaluations and [`retain`](Kernel::retain)s what its cache holds.
pub struct Kernel {
    pub(crate) model: Model,
    records: Vec<KernelCall>,
    /// By handle; `None` once released.
    bodies: Vec<Option<BodyEntry>>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    /// The model's point tolerance: a micrometre, since a model is in
    /// metres (SI inside, `SEED.md` §8.2). The same number the sketcher
    /// compares lengths with, so it lives in `arrix-core`.
    pub const DEFAULT_TOLERANCE: f64 = arrix_core::LENGTH_TOLERANCE;

    pub fn new() -> Self {
        let precision = Precision {
            default_tolerance: Self::DEFAULT_TOLERANCE,
            ..Precision::DEFAULT
        };
        Self {
            model: Model::new(precision).expect("the metre precision is consistent"),
            records: Vec::new(),
            bodies: Vec::new(),
        }
    }

    /// Every call made so far, failed ones included, in call order.
    pub fn records(&self) -> &[KernelCall] {
        &self.records
    }

    pub fn record(&self, call: CallIndex) -> Option<&KernelCall> {
        self.records.get(call.0 as usize)
    }

    /// Records a call before it runs, its operands by content hash and by
    /// the calls that made them.
    pub(crate) fn begin(
        &mut self,
        op: KernelOp,
        operands: &[KernelBody],
    ) -> Result<CallIndex, KernelError> {
        let mut sources = Vec::with_capacity(operands.len());
        for &operand in operands {
            sources.push(self.entry(operand)?.made_by);
        }
        let call = CallIndex(u32::try_from(self.records.len()).expect("under 2³² calls"));
        self.records.push(KernelCall {
            op,
            operands: Vec::new(),
            sources,
            precision: CallPrecision {
                default_tolerance: self.model.precision().default_tolerance,
            },
            arris_version: ARRIS_VERSION.to_owned(),
        });
        for &operand in operands {
            let hash = self
                .content_hash(operand)
                .map_err(|e| KernelError::bytes(call, &e))?;
            self.records[call.0 as usize].operands.push(hash);
        }
        Ok(call)
    }

    /// `body`'s hash, made on first use. The handle is known.
    fn content_hash(&mut self, body: KernelBody) -> Result<BodyHash, BodyError> {
        let entry = self.entry(body).expect("a known operand");
        if let Some(hash) = &entry.hash {
            return Ok(hash.clone());
        }
        let hash = BodyHash::of(&bytes::encode(&self.model, entry.body, &entry.names)?.bytes);
        if let Some(Some(entry)) = self.bodies.get_mut(body.0 as usize) {
            entry.hash = Some(hash.clone());
        }
        Ok(hash)
    }

    pub(crate) fn entry(&self, body: KernelBody) -> Result<&BodyEntry, KernelError> {
        self.bodies
            .get(body.0 as usize)
            .and_then(Option::as_ref)
            .ok_or(KernelError::UnknownBody(body))
    }

    /// `profile` swept `distance` metres along its plane's normal, or
    /// against it when negative; its entities named as `feature`'s sweep
    /// number `index` (ADR-0011).
    pub fn extrude(
        &mut self,
        feature: FeatureId,
        index: u32,
        profile: &Profile,
        distance: f64,
    ) -> Result<KernelBody, KernelError> {
        let call = self.begin(
            KernelOp::Extrude {
                feature,
                index,
                profile: profile.clone(),
                distance,
            },
            &[],
        )?;
        let direction = convert::vec3(profile.plane().z_axis() * distance.signum());
        let (body, provenance) = arris::ops::extrude(
            &mut self.model,
            &convert::profile(profile),
            direction,
            distance.abs(),
            &Control::NONE,
        )
        .map_err(|e| KernelError::op(call, &e))?;
        let names = naming::sweep_names(
            &self.model,
            body,
            naming::Sweep::Extrude,
            (feature, index),
            profile,
            &provenance,
        )
        .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(call, body, names))
    }

    /// `profile` revolved `angle` radians (at most a full turn) about the
    /// axis through `axis_origin` along `axis_direction`, both in the
    /// world; its entities named as `feature`'s revolve number `index`,
    /// counted with the extrudes (ADR-0011). A full turn has no end cap
    /// and its seams are start edges.
    pub fn revolve(
        &mut self,
        feature: FeatureId,
        index: u32,
        profile: &Profile,
        (axis_origin, axis_direction): (DVec3, DVec3),
        angle: f64,
    ) -> Result<KernelBody, KernelError> {
        let call = self.begin(
            KernelOp::Revolve {
                feature,
                index,
                profile: profile.clone(),
                axis_origin,
                axis_direction,
                angle,
            },
            &[],
        )?;
        let axis = arris::math::Axis::new(
            arris::math::Point3::from(convert::vec3(axis_origin)),
            convert::vec3(axis_direction),
        )
        .map_err(|e| KernelError::invalid_input(call, e.to_string()))?;
        let (body, provenance) = arris::ops::revolve(
            &mut self.model,
            &convert::profile(profile),
            axis,
            angle,
            &Control::NONE,
        )
        .map_err(|e| KernelError::op(call, &e))?;
        let names = naming::sweep_names(
            &self.model,
            body,
            naming::Sweep::Revolve,
            (feature, index),
            profile,
            &provenance,
        )
        .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(call, body, names))
    }

    /// `target` ∪ `tool`, named as `feature`'s boolean number `boolean`
    /// (from 0 in call order, ADR-0014): what the union left alone keeps
    /// its name, a piece of an entity is `mod.<feature>.<boolean>.<k>` of
    /// it, what it made is `gen.<feature>.<boolean>[…]` of its origins,
    /// the number not written when 0. The operands stay valid.
    pub fn fuse(
        &mut self,
        feature: FeatureId,
        boolean: u32,
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        self.boolean(
            KernelOp::Fuse { feature, boolean },
            (feature, boolean),
            target,
            tool,
        )
    }

    /// `target` less `tool`, named as [`fuse`](Kernel::fuse)'s.
    pub fn cut(
        &mut self,
        feature: FeatureId,
        boolean: u32,
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        self.boolean(
            KernelOp::Cut { feature, boolean },
            (feature, boolean),
            target,
            tool,
        )
    }

    /// `target` ∩ `tool`, named as [`fuse`](Kernel::fuse)'s.
    pub fn common(
        &mut self,
        feature: FeatureId,
        boolean: u32,
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        self.boolean(
            KernelOp::Common { feature, boolean },
            (feature, boolean),
            target,
            tool,
        )
    }

    fn boolean(
        &mut self,
        op: KernelOp,
        feature: (FeatureId, u32),
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        let a = self.entry(target)?.body;
        let b = self.entry(tool)?.body;
        let run = match op {
            KernelOp::Fuse { .. } => arris::ops::fuse,
            KernelOp::Cut { .. } => arris::ops::cut,
            KernelOp::Common { .. } => arris::ops::common,
            _ => unreachable!("fuse, cut and common alone come here"),
        };
        let call = self.begin(op, &[target, tool])?;
        let (body, provenance) =
            run(&mut self.model, a, b, &Control::NONE).map_err(|e| KernelError::op(call, &e))?;
        let operands = [&self.entry(target)?.names, &self.entry(tool)?.names];
        let names = naming::boolean_names(&self.model, body, feature, &operands, &provenance)
            .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(call, body, names))
    }

    /// `body` with `edges` rounded at `radius` metres, named as `feature`'s
    /// boolean number `boolean` (docs/DATA-MODEL.md §Persistent naming): the blend faces and what
    /// they made at their ends are `gen.<feature>[…]` of the edges, every
    /// trimmed neighbour keeps its name. `body` stays valid.
    pub fn fillet(
        &mut self,
        feature: FeatureId,
        boolean: u32,
        body: KernelBody,
        edges: &[PersistentName],
        radius: f64,
    ) -> Result<KernelBody, KernelError> {
        let op = |at| KernelOp::Fillet {
            feature,
            boolean,
            edges: edges.to_vec(),
            at,
            radius,
        };
        self.blend(
            op,
            (feature, boolean),
            body,
            edges,
            arris::ops::fillet,
            radius,
        )
    }

    /// `body` with `edges` cut flat at `distance` metres, named as
    /// [`fillet`](Kernel::fillet)'s.
    pub fn chamfer(
        &mut self,
        feature: FeatureId,
        boolean: u32,
        body: KernelBody,
        edges: &[PersistentName],
        distance: f64,
    ) -> Result<KernelBody, KernelError> {
        let op = |at| KernelOp::Chamfer {
            feature,
            boolean,
            edges: edges.to_vec(),
            at,
            distance,
        };
        self.blend(
            op,
            (feature, boolean),
            body,
            edges,
            arris::ops::chamfer,
            distance,
        )
    }

    /// `body` moved by `motion`, every entity named as copy `index` of
    /// `feature` (`inst.<feature>.<index>`, `index` from 1), so a
    /// pattern's count can rise and fall without renaming a copy that
    /// stays. `body` stays valid.
    pub fn copy(
        &mut self,
        feature: FeatureId,
        index: u32,
        body: KernelBody,
        motion: Motion,
    ) -> Result<KernelBody, KernelError> {
        let source = self.entry(body)?.body;
        let call = self.begin(
            KernelOp::Copy {
                feature,
                index,
                motion,
            },
            &[body],
        )?;
        let axis =
            arris::math::UnitVec3::try_new(convert::vec3(motion.axis), 0.0).ok_or_else(|| {
                KernelError::invalid_input(call, "a rotation axis of no length".to_owned())
            })?;
        let rotation = arris::math::nalgebra::UnitQuaternion::from_axis_angle(&axis, motion.angle);
        let isometry = arris::math::Isometry::new(rotation, convert::vec3(motion.translation));
        let (copy, provenance) =
            arris::ops::transform(&mut self.model, source, &isometry, &Control::NONE)
                .map_err(|e| KernelError::op(call, &e))?;
        self.instance(call, copy, (feature, index), body, &provenance)
    }

    /// `body` reflected in the plane through `origin` with `normal`, named
    /// as instance 1 of `feature`. `body` stays valid.
    pub fn mirror(
        &mut self,
        feature: FeatureId,
        body: KernelBody,
        origin: DVec3,
        normal: DVec3,
    ) -> Result<KernelBody, KernelError> {
        let source = self.entry(body)?.body;
        let call = self.begin(
            KernelOp::Mirror {
                feature,
                origin,
                normal,
            },
            &[body],
        )?;
        let plane = arris::math::Reflection::new(
            arris::math::Point3::from(convert::vec3(origin)),
            convert::vec3(normal),
        )
        .map_err(|e| KernelError::invalid_input(call, e.to_string()))?;
        let (image, provenance) =
            arris::ops::mirror(&mut self.model, source, &plane, &Control::NONE)
                .map_err(|e| KernelError::op(call, &e))?;
        self.instance(call, image, (feature, 1), body, &provenance)
    }

    fn instance(
        &mut self,
        call: CallIndex,
        made: Body,
        instance: (FeatureId, u32),
        from: KernelBody,
        provenance: &arris::topo::Provenance,
    ) -> Result<KernelBody, KernelError> {
        let names = naming::instance_names(
            &self.model,
            made,
            instance,
            &self.entry(from)?.names,
            provenance,
        )
        .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(call, made, names))
    }

    fn blend(
        &mut self,
        op: impl FnOnce(Vec<DVec3>) -> KernelOp,
        feature: (FeatureId, u32),
        body: KernelBody,
        edges: &[PersistentName],
        run: BlendOp,
        size: f64,
    ) -> Result<KernelBody, KernelError> {
        let entry = self.entry(body)?;
        let target = entry.body;
        let mut ids = Vec::with_capacity(edges.len());
        for name in edges {
            match entry.names.entity(name) {
                Some(EntityId::Edge(id)) => ids.push(id),
                Some(_) => {
                    return Err(KernelError::WrongKind {
                        name: name.clone(),
                        wanted: TopoKind::Edge,
                        found: name.kind,
                    });
                }
                None => return Err(KernelError::UnknownName { name: name.clone() }),
            }
        }
        let at = ids.iter().filter_map(|&id| self.edge_point(id)).collect();
        let call = self.begin(op(at), &[body])?;
        let handles = self
            .model
            .edges(target)
            .map_err(|e| KernelError::op(call, &e.into()))?;
        // In the order the names were given; an id the body lacks cannot
        // happen, its table covers the body.
        let handles: Vec<_> = ids
            .iter()
            .filter_map(|id| handles.iter().find(|e| e.id == *id).cloned())
            .collect();
        let (blended, provenance) = run(&mut self.model, target, &handles, size, &Control::NONE)
            .map_err(|e| KernelError::op(call, &e))?;
        let names = naming::boolean_names(
            &self.model,
            blended,
            feature,
            &[&self.entry(body)?.names],
            &provenance,
        )
        .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(call, blended, names))
    }

    /// A point on edge `id`: its curve at mid-parameter. `None` for an
    /// edge with no curve, which a fixture recipe then cannot name.
    fn edge_point(&self, id: arris::topo::EdgeId) -> Option<DVec3> {
        let (curve, range) = self.model.edge(id).ok()?.curve()?;
        let point = self.model.curve(curve).ok()?.point(range.midpoint());
        Some(convert::dpoint3(point))
    }

    fn push(&mut self, made_by: CallIndex, body: Body, names: BodyNames) -> KernelBody {
        let handle = KernelBody(u32::try_from(self.bodies.len()).expect("under 2³² bodies"));
        self.bodies.push(Some(BodyEntry {
            body,
            names,
            hash: None,
            made_by,
        }));
        handle
    }

    /// Releases every body but `keep`, and the model's entities only they
    /// used. A released handle is unknown from then on; handles are never
    /// reused. Records are kept: a later failure's operands name them.
    pub fn retain(&mut self, keep: &std::collections::BTreeSet<KernelBody>) {
        let mut live = Vec::new();
        for (i, slot) in self.bodies.iter_mut().enumerate() {
            let handle = KernelBody(u32::try_from(i).expect("under 2³² bodies"));
            match slot {
                Some(entry) if keep.contains(&handle) => live.push(entry.body),
                _ => *slot = None,
            }
        }
        self.model
            .retain(&live)
            .expect("every live body is in the model");
    }

    /// Every face, edge and vertex of `body`, by name.
    pub fn names(&self, body: KernelBody) -> Result<&BodyNames, KernelError> {
        Ok(&self.entry(body)?.names)
    }

    /// The outward frame of the planar face `face` of `body`: its normal
    /// points out of the material.
    pub fn face_frame(
        &mut self,
        body: KernelBody,
        face: &PersistentName,
    ) -> Result<Frame, KernelError> {
        let entry = self.entry(body)?;
        let target = entry.body;
        let id = match entry.names.entity(face) {
            Some(EntityId::Face(id)) => id,
            Some(_) => {
                return Err(KernelError::WrongKind {
                    name: face.clone(),
                    wanted: TopoKind::Face,
                    found: face.kind,
                });
            }
            None => return Err(KernelError::UnknownName { name: face.clone() }),
        };
        let call = self.begin(KernelOp::FaceFrame { face: face.clone() }, &[body])?;
        // The face as the body uses it, so the frame is outward.
        let used = self
            .model
            .faces(target)
            .map_err(|e| KernelError::op(call, &e.into()))?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| KernelError::Naming {
                call,
                message: format!("{face} is not a face of the body it was named in"),
            })?;
        let frame = arris::ops::query::face_frame(&self.model, used)
            .map_err(|e| KernelError::op(call, &e))?;
        Ok(convert::frame_back(&frame))
    }

    /// Where the vertex `vertex` of `body` is, in the world.
    pub fn vertex_point(
        &mut self,
        body: KernelBody,
        vertex: &PersistentName,
    ) -> Result<DVec3, KernelError> {
        let entry = self.entry(body)?;
        let id = match entry.names.entity(vertex) {
            Some(EntityId::Vertex(id)) => id,
            Some(_) => {
                return Err(KernelError::WrongKind {
                    name: vertex.clone(),
                    wanted: TopoKind::Vertex,
                    found: vertex.kind,
                });
            }
            None => {
                return Err(KernelError::UnknownName {
                    name: vertex.clone(),
                });
            }
        };
        let call = self.begin(
            KernelOp::VertexPoint {
                vertex: vertex.clone(),
            },
            &[body],
        )?;
        let point = self
            .model
            .vertex(id)
            .map_err(|e| KernelError::op(call, &e.into()))?
            .point();
        Ok(convert::dpoint3(point))
    }

    /// The line the straight edge `edge` of `body` lies on: a point of it
    /// and its unit direction, as Arris orients it. A curved edge is
    /// `kernel.invalid-input`.
    pub fn edge_axis(
        &mut self,
        body: KernelBody,
        edge: &PersistentName,
    ) -> Result<Axis, KernelError> {
        let entry = self.entry(body)?;
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
        let call = self.begin(KernelOp::EdgeAxis { edge: edge.clone() }, &[body])?;
        let curve = self
            .model
            .edge(id)
            .map_err(|e| KernelError::op(call, &e.into()))?
            .curve()
            .and_then(|(curve, _)| self.model.curve(curve).ok());
        match curve {
            Some(arris::geom::Curve::Line { origin, direction }) => Ok(Axis {
                origin: convert::dpoint3(*origin),
                direction: convert::dvec3(direction.into_inner()),
            }),
            _ => Err(KernelError::invalid_input(
                call,
                format!("{edge} is not a straight edge"),
            )),
        }
    }

    /// The edges and vertices `names` of `body` projected orthogonally
    /// onto `plane`, in the order given, in the plane's `(u, v)`. An edge
    /// seen end-on is `kernel.degenerate.projection-collapses`; one whose
    /// image is neither a line nor a circle's arc is
    /// `kernel.unsupported.projection`, since a sketch has no entity for
    /// it. Both are recorded failures, naming the edge.
    pub fn project(
        &mut self,
        body: KernelBody,
        names: &[PersistentName],
        plane: &Frame,
    ) -> Result<Vec<Projected>, KernelError> {
        let entry = self.entry(body)?;
        let mut shapes = Vec::with_capacity(names.len());
        for name in names {
            let id = match entry.names.entity(name) {
                Some(id @ (EntityId::Edge(_) | EntityId::Vertex(_))) => id,
                Some(_) => {
                    return Err(KernelError::WrongKind {
                        name: name.clone(),
                        wanted: TopoKind::Edge,
                        found: name.kind,
                    });
                }
                None => return Err(KernelError::UnknownName { name: name.clone() }),
            };
            shapes.push(arris::topo::Shape {
                id,
                orientation: arris::topo::Orientation::Forward,
            });
        }
        let call = self.begin(
            KernelOp::Project {
                names: names.to_vec(),
                plane: *plane,
            },
            &[body],
        )?;
        let projections =
            arris::ops::query::project_to_plane(&self.model, &shapes, &convert::frame(plane))
                .map_err(|e| KernelError::op(call, &e))?;
        let mut out = Vec::with_capacity(names.len());
        for (name, (projection, shape)) in names.iter().zip(projections.iter().zip(&shapes)) {
            let closed = match shape.id {
                EntityId::Edge(id) => self
                    .model
                    .edge(id)
                    .map_err(|e| KernelError::op(call, &e.into()))?
                    .is_closed(),
                _ => false,
            };
            let projected = project::projected(projection, closed).map_err(|what| {
                KernelError::unsupported(call, "projection", format!("{name} projects to {what}"))
            })?;
            out.push(projected);
        }
        Ok(out)
    }

    pub fn mass_properties(&mut self, body: KernelBody) -> Result<MassProperties, KernelError> {
        let target = self.entry(body)?.body;
        let call = self.begin(KernelOp::MassProperties, &[body])?;
        let p = arris::ops::measure::mass_properties(&self.model, target, &Control::NONE)
            .map_err(|e| KernelError::op(call, &e))?;
        Ok(MassProperties {
            volume: p.volume,
            area: p.area,
            centroid: convert::dpoint3(p.centroid),
        })
    }

    /// `body` as Arris body bytes, the same for the same body in any
    /// evaluation, and the table of its names (each to its entity's index
    /// in the written body).
    pub fn to_bytes(&mut self, body: KernelBody) -> Result<BodyBytes, KernelError> {
        let call = self.begin(KernelOp::ToBytes, &[body])?;
        let entry = self.entry(body)?;
        bytes::encode(&self.model, entry.body, &entry.names)
            .map_err(|e| KernelError::bytes(call, &e))
    }

    /// The body in `bytes` ([`to_bytes`](Kernel::to_bytes)'s), a new body
    /// of this kernel named by `table`. A table that does not cover the
    /// body exactly, one name to one entity, is refused as `kernel.naming`.
    pub fn from_bytes(
        &mut self,
        bytes: &[u8],
        table: &NameTable,
    ) -> Result<KernelBody, KernelError> {
        let call = self.begin(
            KernelOp::FromBytes {
                bytes: BodyHash::of(bytes),
            },
            &[],
        )?;
        let (body, names) = bytes::decode(&mut self.model, bytes, table)
            .map_err(|e| KernelError::bytes(call, &e))?;
        let names = names.map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(call, body, names))
    }
}
