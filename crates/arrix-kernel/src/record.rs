//! Call records: every kernel call described as a value before it runs
//! (docs/ARCHITECTURE.md §The kernel choke point, "Call records"). Plain
//! data with no Arris type in it, so the evaluator can keep them beside a
//! diagnostic and a later step can export one as an Arris fixture recipe.

use arrix_core::{DVec3, FeatureId, PersistentName, Profile};
use serde::{Deserialize, Serialize};

/// The Arris release this crate is built against, as `Cargo.lock` pins it.
/// A record carries it so a fixture says which kernel failed.
pub const ARRIS_VERSION: &str = "0.5.0";

/// The index of a record in its evaluation's list, in call order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallIndex(pub u32);

/// A body's content hash: the BLAKE3 of its canonical Arris body bytes, as
/// 64 lower-case hex digits. Two bodies of the same shape and names hash
/// alike whichever evaluation made them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BodyHash(String);

impl BodyHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One kernel call. `operands` are the bodies this one reads, by content
/// hash, in operand order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KernelCall {
    pub op: KernelOp,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operands: Vec<BodyHash>,
    pub precision: CallPrecision,
    pub arris_version: String,
}

/// The operation and its arguments, in ArriX's own types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum KernelOp {
    /// `profile` swept along its plane's normal by `distance` metres; a
    /// negative distance sweeps against it. `feature` roots the names, as
    /// its sweep number `index` (ADR-0011).
    Extrude {
        feature: FeatureId,
        #[serde(default)]
        index: u32,
        profile: Profile,
        distance: f64,
    },
    /// `profile` revolved `angle` radians about the world axis through
    /// `axis_origin` along `axis_direction`; `feature` and `index` root the
    /// names as for `Extrude`.
    Revolve {
        feature: FeatureId,
        index: u32,
        profile: Profile,
        axis_origin: DVec3,
        axis_direction: DVec3,
        angle: f64,
    },
    /// The union of the two operand bodies, target first; `feature` names
    /// the pieces and what the union generated.
    Fuse { feature: FeatureId },
    /// The first operand less the second; `feature` as for `Fuse`.
    Cut { feature: FeatureId },
    /// The operand body with `edges` rounded at `radius` metres; `feature`
    /// names the blend's entities.
    Fillet {
        feature: FeatureId,
        edges: Vec<PersistentName>,
        radius: f64,
    },
    /// The operand body with `edges` cut flat at `distance` metres.
    Chamfer {
        feature: FeatureId,
        edges: Vec<PersistentName>,
        distance: f64,
    },
    /// The outward frame of a planar face of the operand body.
    FaceFrame { face: PersistentName },
    /// Volume, area and centroid of the operand body.
    MassProperties,
    /// The operand body as Arris body bytes and a name table.
    ToBytes,
    /// Body bytes read into the model; `bytes` is their hash. The operand
    /// list is empty: the bytes are the input.
    FromBytes { bytes: BodyHash },
}

/// The model tolerances a call ran under, in the fields an Arris fixture
/// recipe's `precision` takes; the rest are Arris's defaults.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallPrecision {
    pub default_tolerance: f64,
}
