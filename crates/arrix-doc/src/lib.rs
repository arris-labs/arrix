//! The document: parameters, the DAG, features, commands, undo, the
//! evaluator and the `.arrx` format (docs/DATA-MODEL.md).

mod dag;
mod document;
mod eval;
pub mod expr;
mod open;
mod registry;

pub use dag::{Dag, DagError, Node};
pub use document::{
    Document, FeatureRecord, FeatureTypeId, Invalid, InvalidFeatureTypeId, Param, Part,
    is_param_name,
};
pub use eval::{BodyLine, EvalLine, EvalStatus, FeatureLine, SweepPoint, eval};
pub use open::{DOCUMENT_JSON, DocumentSource, MemorySource, OpenError, SCHEMA, open};
pub use registry::{FeatureType, Registry, RegistryError};
