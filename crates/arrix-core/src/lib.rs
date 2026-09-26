//! Units and quantities, ids, plain math, persistent-reference types and
//! `Diagnostic`: the vocabulary every other ArriX crate shares
//! (docs/ARCHITECTURE.md §Crates).

mod diagnostic;
mod id;
mod plugin;
pub mod units;

pub use diagnostic::{Diagnostic, DiagnosticCode, InvalidCode, Severity};
pub use id::{FeatureId, ID_LEN, Id, IdError, IdMinter, ParamId, PartId, RecordId, SketchEntityId};
pub use plugin::{InvalidPluginId, PluginId};
pub use units::{Quantity, QuantityKind, Unit, UnitMismatch};
