//! Call records: every kernel call described as a value before it runs
//! (docs/ARCHITECTURE.md §The kernel choke point, "Call records"). Plain
//! data with no Arris type in it, so the evaluator can keep them beside a
//! diagnostic and a later step can export one as an Arris fixture recipe.

use arrix_core::{FeatureId, PersistentName, Profile};
use serde::{Deserialize, Serialize};

/// The Arris release this crate is built against, as `Cargo.lock` pins it.
/// A record carries it so a fixture says which kernel failed.
pub const ARRIS_VERSION: &str = "0.2.0";

/// The index of a record in its evaluation's list, in call order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallIndex(pub u32);

/// One kernel call. `operands` are the calls whose output bodies this one
/// reads, by index: until body bytes (Arris ask A2) give a body a content
/// hash, a body operand is named by the call that made it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KernelCall {
    pub op: KernelOp,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operands: Vec<CallIndex>,
    pub precision: CallPrecision,
    pub arris_version: String,
}

/// The operation and its arguments, in ArriX's own types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum KernelOp {
    /// `profile` swept along its plane's normal by `distance` metres; a
    /// negative distance sweeps against it. `feature` roots the names.
    Extrude {
        feature: FeatureId,
        profile: Profile,
        distance: f64,
    },
    /// The outward frame of a planar face of the operand body.
    FaceFrame { face: PersistentName },
    /// Volume, area and centroid of the operand body.
    MassProperties,
}

/// The model tolerances a call ran under, in the fields an Arris fixture
/// recipe's `precision` takes; the rest are Arris's defaults.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallPrecision {
    pub default_tolerance: f64,
}
