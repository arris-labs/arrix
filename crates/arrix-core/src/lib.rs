//! Units and quantities, ids, plain math, persistent-reference types,
//! render meshes and `Diagnostic`: the vocabulary every other ArriX crate shares
//! (docs/ARCHITECTURE.md §Crates).

mod diagnostic;
mod export;
pub mod geom;
mod id;
mod mesh;
mod naming;
mod plugin;
mod reference;
pub mod units;

pub use diagnostic::{Diagnostic, DiagnosticCode, InvalidCode, Severity};
pub use export::ExportFormat;
pub use geom::{Axis, Frame, Motion, Profile, ProfileLoop, ProfileSegment};
pub use glam::{DMat3, DVec2, DVec3};
pub use id::{
    AssemblyId, ComponentId, CurveKey, FeatureId, ID_LEN, Id, IdError, IdMinter, JointId, ParamId,
    PartId, RecordId, SketchEntityId,
};
pub use mesh::{MeshDetail, MeshPoint, MeshRange, RenderMesh};
pub use naming::{
    MAX_NAME_DEPTH, NameError, NameRoot, NameStep, PersistentName, SweepPartName, TopoKind,
};
pub use plugin::{InvalidPluginId, PluginId};
pub use reference::{Input, InvalidSlotName, Ref, RegionKey, SlotName};
pub use units::{LENGTH_TOLERANCE, Quantity, QuantityKind, Unit, UnitMismatch};
