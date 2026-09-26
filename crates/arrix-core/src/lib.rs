//! Units and quantities, ids, plain math, persistent-reference types and
//! `Diagnostic`: the vocabulary every other ArriX crate shares
//! (docs/ARCHITECTURE.md §Crates).

mod diagnostic;
pub mod geom;
mod id;
mod naming;
mod plugin;
mod reference;
pub mod units;

pub use diagnostic::{Diagnostic, DiagnosticCode, InvalidCode, Severity};
pub use geom::{Frame, Profile, ProfileLoop, ProfileSegment};
pub use glam::{DVec2, DVec3};
pub use id::{
    CurveKey, FeatureId, ID_LEN, Id, IdError, IdMinter, ParamId, PartId, RecordId, SketchEntityId,
};
pub use naming::{
    MAX_NAME_DEPTH, NameError, NameRoot, NameStep, PersistentName, SweepPartName, TopoKind,
};
pub use plugin::{InvalidPluginId, PluginId};
pub use reference::{InvalidSlotName, Ref, RegionKey, SlotName};
pub use units::{LENGTH_TOLERANCE, Quantity, QuantityKind, Unit, UnitMismatch};
