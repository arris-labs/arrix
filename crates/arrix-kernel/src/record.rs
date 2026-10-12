//! Call records: every kernel call described as a value before it runs
//! (docs/ARCHITECTURE.md §The kernel choke point, "Call records"). Plain
//! data with no Arris type in it, so the evaluator can keep them beside a
//! diagnostic and [`fixture`](crate::fixture) can export one as an Arris
//! fixture recipe.

use arrix_core::{
    DVec3, ExportFormat, FeatureId, Frame, MeshDetail, Motion, PersistentName, Profile,
};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// The Arris release this crate is built against, as `Cargo.lock` pins it.
/// A record carries it so a fixture says which kernel failed.
pub const ARRIS_VERSION: &str = "0.6.0";

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

/// A file's content hash: the BLAKE3 of its bytes, as 64 lower-case hex
/// digits, which is how the document names the blob an import reads.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileHash(String);

impl FileHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The bytes of a file a call read, shared by every record of it, so a
/// fixture recipe can write the file beside itself. Not serialised: a
/// record read back holds none, and its recipe is
/// `FixtureError::NoFile`.
#[derive(Clone, Default)]
pub struct FileBytes(pub(crate) Option<Arc<[u8]>>);

impl FileBytes {
    pub fn get(&self) -> Option<&[u8]> {
        self.0.as_deref()
    }
}

impl PartialEq for FileBytes {
    /// The same file or none: the hash beside it says which.
    fn eq(&self, other: &Self) -> bool {
        self.0.is_some() == other.0.is_some()
    }
}

impl std::fmt::Debug for FileBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(bytes) => write!(f, "FileBytes({} bytes)", bytes.len()),
            None => f.write_str("FileBytes(none)"),
        }
    }
}

/// A solid of an exchange file: its `#id` and which placement of it, from
/// 0 (Arris's `FileEntity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileSolid {
    pub id: u64,
    pub instance: u32,
}

/// One kernel call. `operands` are the bodies this one reads, by content
/// hash, in operand order; `sources` the calls that made them, in the same
/// order, which is how a fixture recipe finds an operand's own recipe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KernelCall {
    pub op: KernelOp,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operands: Vec<BodyHash>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<CallIndex>,
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
    /// the pieces and what the union generated, and `boolean` is the place
    /// of the call among `feature`'s booleans (ADR-0014).
    Fuse {
        feature: FeatureId,
        #[serde(default)]
        boolean: u32,
    },
    /// The first operand less the second; `feature` and `boolean` as for
    /// `Fuse`.
    Cut {
        feature: FeatureId,
        #[serde(default)]
        boolean: u32,
    },
    /// The intersection of the two operands; `feature` and `boolean` as for
    /// `Fuse`.
    Common {
        feature: FeatureId,
        #[serde(default)]
        boolean: u32,
    },
    /// The operand body with `edges` rounded at `radius` metres; `feature`
    /// names the blend's entities, `boolean` as for `Fuse`. `at` holds a point on each edge, in the
    /// same order (its curve at mid-parameter), which is how a fixture
    /// recipe names an edge.
    Fillet {
        feature: FeatureId,
        #[serde(default)]
        boolean: u32,
        edges: Vec<PersistentName>,
        #[serde(default)]
        at: Vec<DVec3>,
        radius: f64,
    },
    /// The operand body with `edges` cut flat at `distance` metres; `at` as
    /// for `Fillet`.
    Chamfer {
        feature: FeatureId,
        #[serde(default)]
        boolean: u32,
        edges: Vec<PersistentName>,
        #[serde(default)]
        at: Vec<DVec3>,
        distance: f64,
    },
    /// The operand body moved by `motion`, its entities named as copy
    /// `index` of `feature` (`inst.<feature>.<index>`).
    Copy {
        feature: FeatureId,
        index: u32,
        motion: Motion,
    },
    /// The operand body reflected in the plane through `origin` with
    /// `normal`; its entities are instance 1 of `feature`.
    Mirror {
        feature: FeatureId,
        origin: DVec3,
        normal: DVec3,
    },
    /// The edges and vertices `names` of the operand body projected onto
    /// `plane`.
    Project {
        names: Vec<PersistentName>,
        plane: Frame,
    },
    /// The outward frame of a planar face of the operand body.
    FaceFrame { face: PersistentName },
    /// Where a vertex of the operand body is.
    VertexPoint { vertex: PersistentName },
    /// The line a straight edge of the operand body lies on.
    EdgeAxis { edge: PersistentName },
    /// An assembly anchor's frame on the operand body: z and a point from
    /// `axis`, a face or an edge, moved onto the planar face `origin`
    /// (ADR-0029 §3).
    AnchorFrame {
        axis: PersistentName,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<PersistentName>,
    },
    /// Volume, area and centroid of the operand body.
    MassProperties,
    /// The operand body as a render mesh at `detail`, which came to
    /// `chord` metres on this body.
    Tessellate { detail: MeshDetail, chord: f64 },
    /// The operand body as Arris body bytes and a name table.
    ToBytes,
    /// The operand bodies, in operand order, written as `format`: the
    /// exact B-rep for STEP, the refined meshes for STL and OBJ.
    Export { format: ExportFormat },
    /// Solid number `solid` (from 1, in the reader's order) of the STEP
    /// file whose hash is `file`, read in metres; `feature` roots the
    /// names (`import.<feature>.<entity>`). `entity` is the solid's file
    /// entity, once the read found it; `bytes` the file, for a recipe.
    ImportStep {
        feature: FeatureId,
        file: FileHash,
        solid: u32,
        /// Where the solid is read: at its placement in the file's
        /// assembly, or in its product's own frame (ADR-0032 §1).
        #[serde(default, skip_serializing_if = "ImportFrame::is_world")]
        frame: ImportFrame,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity: Option<FileSolid>,
        #[serde(skip)]
        bytes: FileBytes,
    },
    /// The product structure of the STEP file whose hash is `file`, read
    /// in metres (ADR-0032); the solids it read are dropped again.
    ReadStepStructure {
        file: FileHash,
        #[serde(skip)]
        bytes: FileBytes,
    },
    /// Body bytes read into the model; `bytes` is their hash. The operand
    /// list is empty: the bytes are the input.
    FromBytes { bytes: BodyHash },
}

/// Where an imported solid sits (ADR-0032 §1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportFrame {
    /// At its placement in the file's assembly, as the reader bakes it.
    #[default]
    World,
    /// In its product's own frame: the inverse of the placement composed
    /// from the root, so a product placed twice is one shape twice.
    Product,
}

impl ImportFrame {
    pub(crate) fn is_world(&self) -> bool {
        *self == ImportFrame::World
    }
}

/// The model tolerances a call ran under, in the fields an Arris fixture
/// recipe's `precision` takes; the rest are Arris's defaults.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallPrecision {
    pub default_tolerance: f64,
}
