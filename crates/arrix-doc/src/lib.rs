//! The document: parameters, the DAG, features, commands, undo, the
//! evaluator and the `.arrx` format (docs/DATA-MODEL.md).

mod authority;
mod bodies;
mod command;
mod core_boolean;
mod core_extrude;
mod core_types;
mod dag;
mod document;
mod eval;
mod executor;
pub mod expr;
mod frozen;
mod migrate;
mod open;
mod registry;
mod save;
mod session;

pub use authority::{AuthorId, Authority, Change, CommandEnvelope, Generation, Outcome, Rejected};
pub use command::{Applied, Command, CommandError, FeatureEdit, apply};
pub use core_boolean::CoreBoolean;
pub use core_extrude::CoreExtrude;
pub use core_types::{CoreSketch, DatumPlane, WORLD_PLANES};
pub use dag::{Dag, DagError, Node};
pub use document::{
    Document, FeatureRecord, FeatureTypeId, Invalid, InvalidFeatureTypeId, Param, Part,
    is_param_name,
};
pub use eval::{
    BodyLine, BodyMeasures, EvalEvent, EvalLine, EvalStatus, Evaluation, Evaluator, FeatureLine,
    FeatureOutcome, InputHash, ParamLine, RegionView, SketchView, SlotView, SweepPoint, eval,
};
pub use frozen::{BlobRef, FrozenResult, InvalidBlobRef};
pub use open::{DOCUMENT_JSON, DocumentSource, MemorySource, OpenError, PARAMS_JSON, SCHEMA, open};
pub use registry::{FeatureArgs, FeatureType, Registry, RegistryError, SketchArgs, TypeOutput};
pub use save::{save, to_json};
pub use session::{
    DocHash, EventStream, LocalSession, ParamValues, Replica, ReplicaError, Request, Session,
    SessionEvent,
};
