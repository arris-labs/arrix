//! The only crate naming Arris types: operations, naming from provenance,
//! render meshes, body bytes and call records (docs/ARCHITECTURE.md §The
//! kernel choke point).
//!
//! Built so far: the `Kernel` service's first slice (`extrude`, `fuse`,
//! `cut`, `face_frame`, `mass_properties`, `to_bytes`, `from_bytes`), names
//! from the extrude's and the booleans' provenance, and a `KernelCall`
//! record made before every call.

mod bytes;
mod convert;
mod error;
mod naming;
mod record;

use arris::Control;
use arris::io::body::BodyError;
use arris::math::Precision;
use arris::topo::{Body, EntityId, Model};
use arrix_core::{DVec3, FeatureId, Frame, PersistentName, Profile, TopoKind};

pub use bytes::{BodyBytes, NameEntry, NameTable};
pub use error::KernelError;
pub use naming::BodyNames;
pub use record::{ARRIS_VERSION, BodyHash, CallIndex, CallPrecision, KernelCall, KernelOp};

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
    body: Body,
    names: BodyNames,
    /// Of its canonical bytes, once an operand needed it; a body never
    /// changes, so it is made once.
    hash: Option<BodyHash>,
}

/// A kernel context: an Arris model, the bodies made in it, and the
/// record of every call, in order. The evaluator keeps one across
/// evaluations and [`retain`](Kernel::retain)s what its cache holds.
pub struct Kernel {
    model: Model,
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

    /// Records a call before it runs, its operands by content hash.
    fn begin(&mut self, op: KernelOp, operands: &[KernelBody]) -> Result<CallIndex, KernelError> {
        for &operand in operands {
            self.entry(operand)?;
        }
        let call = CallIndex(u32::try_from(self.records.len()).expect("under 2³² calls"));
        self.records.push(KernelCall {
            op,
            operands: Vec::new(),
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

    fn entry(&self, body: KernelBody) -> Result<&BodyEntry, KernelError> {
        self.bodies
            .get(body.0 as usize)
            .and_then(Option::as_ref)
            .ok_or(KernelError::UnknownBody(body))
    }

    /// `profile` swept `distance` metres along its plane's normal, or
    /// against it when negative; its entities named as `feature`'s sweep.
    pub fn extrude(
        &mut self,
        feature: FeatureId,
        profile: &Profile,
        distance: f64,
    ) -> Result<KernelBody, KernelError> {
        let call = self.begin(
            KernelOp::Extrude {
                feature,
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
        let names = naming::extrude_names(&self.model, body, feature, profile, &provenance)
            .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(body, names))
    }

    /// `target` ∪ `tool`, named as `feature`'s: what the union left alone
    /// keeps its name, a piece of an entity is `mod.<feature>.<k>` of it,
    /// what it made is `gen.<feature>[…]` of its origins. The operands
    /// stay valid.
    pub fn fuse(
        &mut self,
        feature: FeatureId,
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        self.boolean(KernelOp::Fuse { feature }, feature, target, tool)
    }

    /// `target` less `tool`, named as [`fuse`](Kernel::fuse)'s.
    pub fn cut(
        &mut self,
        feature: FeatureId,
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        self.boolean(KernelOp::Cut { feature }, feature, target, tool)
    }

    fn boolean(
        &mut self,
        op: KernelOp,
        feature: FeatureId,
        target: KernelBody,
        tool: KernelBody,
    ) -> Result<KernelBody, KernelError> {
        let a = self.entry(target)?.body;
        let b = self.entry(tool)?.body;
        let run = match op {
            KernelOp::Fuse { .. } => arris::ops::fuse,
            KernelOp::Cut { .. } => arris::ops::cut,
            _ => unreachable!("fuse and cut alone come here"),
        };
        let call = self.begin(op, &[target, tool])?;
        let (body, provenance) =
            run(&mut self.model, a, b, &Control::NONE).map_err(|e| KernelError::op(call, &e))?;
        let operands = [&self.entry(target)?.names, &self.entry(tool)?.names];
        let names = naming::boolean_names(&self.model, body, feature, operands, &provenance)
            .map_err(|message| KernelError::Naming { call, message })?;
        Ok(self.push(body, names))
    }

    fn push(&mut self, body: Body, names: BodyNames) -> KernelBody {
        let handle = KernelBody(u32::try_from(self.bodies.len()).expect("under 2³² bodies"));
        self.bodies.push(Some(BodyEntry {
            body,
            names,
            hash: None,
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
        Ok(self.push(body, names))
    }
}
