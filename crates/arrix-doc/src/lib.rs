//! The document: parameters, the DAG, features, commands, undo, the
//! evaluator and the `.arrx` format (docs/DATA-MODEL.md).

mod eval;
pub mod expr;
mod open;

pub use eval::{BodyLine, EvalLine, EvalStatus, FeatureLine, SweepPoint, eval};
pub use open::{DOCUMENT_JSON, Document, DocumentSource, MemorySource, OpenError, SCHEMA, open};
