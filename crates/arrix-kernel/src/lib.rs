//! The only crate naming Arris types: operations, naming from provenance,
//! render meshes, body bytes and call records (docs/ARCHITECTURE.md §The
//! kernel choke point).
//!
//! Built so far: the `Kernel` service's first slice (`extrude`,
//! `face_frame`, `mass_properties`), names from the extrude's provenance,
//! and a `KernelCall` record made before every call.

mod convert;
mod error;
mod naming;
mod record;

use arris::math::Precision;
use arris::topo::{Body, EntityId, Model};
use arrix_core::{DVec3, FeatureId, Frame, PersistentName, Profile, TopoKind};

pub use error::KernelError;
pub use naming::BodyNames;
pub use record::{ARRIS_VERSION, CallIndex, CallPrecision, KernelCall, KernelOp};

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
    made_by: CallIndex,
    names: BodyNames,
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
    /// metres (SI inside, `SEED.md` §8.2).
    pub const DEFAULT_TOLERANCE: f64 = 1e-6;

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

    /// Records a call before it runs.
    fn begin(&mut self, op: KernelOp, operands: Vec<CallIndex>) -> CallIndex {
        let call = CallIndex(u32::try_from(self.records.len()).expect("under 2³² calls"));
        self.records.push(KernelCall {
            op,
            operands,
            precision: CallPrecision {
                default_tolerance: self.model.precision().default_tolerance,
            },
            arris_version: ARRIS_VERSION.to_owned(),
        });
        call
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
            Vec::new(),
        );
        let direction = convert::vec3(profile.plane().z_axis() * distance.signum());
        let (body, provenance) = arris::ops::extrude(
            &mut self.model,
            &convert::profile(profile),
            direction,
            distance.abs(),
        )
        .map_err(|e| KernelError::op(call, &e))?;
        let names = naming::extrude_names(&self.model, body, feature, profile, &provenance)
            .map_err(|message| KernelError::Naming { call, message })?;
        let handle = KernelBody(u32::try_from(self.bodies.len()).expect("under 2³² bodies"));
        self.bodies.push(Some(BodyEntry {
            body,
            made_by: call,
            names,
        }));
        Ok(handle)
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
        let (target, made_by) = (entry.body, entry.made_by);
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
        let call = self.begin(KernelOp::FaceFrame { face: face.clone() }, vec![made_by]);
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
        let entry = self.entry(body)?;
        let (target, made_by) = (entry.body, entry.made_by);
        let call = self.begin(KernelOp::MassProperties, vec![made_by]);
        let p = arris::ops::measure::mass_properties(&self.model, target)
            .map_err(|e| KernelError::op(call, &e))?;
        Ok(MassProperties {
            volume: p.volume,
            area: p.area,
            centroid: convert::dpoint3(p.centroid),
        })
    }
}
