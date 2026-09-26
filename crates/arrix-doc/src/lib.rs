//! The document: parameters, the DAG, features, commands, undo, the
//! evaluator and the `.arrx` format (docs/DATA-MODEL.md).

mod authority;
mod command;
mod core_types;
mod dag;
mod document;
mod eval;
pub mod expr;
mod open;
mod registry;
mod save;

pub use authority::{AuthorId, Authority, Change, CommandEnvelope, Generation, Outcome, Rejected};
pub use command::{Applied, Command, CommandError, FeatureEdit, apply};
pub use core_types::{DatumPlane, WORLD_PLANES};
pub use dag::{Dag, DagError, Node};
pub use document::{
    Document, FeatureRecord, FeatureTypeId, Invalid, InvalidFeatureTypeId, Param, Part,
    is_param_name,
};
pub use eval::{
    BodyLine, BodyMeasures, EvalEvent, EvalLine, EvalStatus, Evaluation, Evaluator, FeatureLine,
    FeatureOutcome, InputHash, ParamLine, SlotView, SweepPoint, eval,
};
pub use open::{DOCUMENT_JSON, DocumentSource, MemorySource, OpenError, PARAMS_JSON, SCHEMA, open};
pub use registry::{FeatureArgs, FeatureType, Registry, RegistryError};
pub use save::{save, to_json};
