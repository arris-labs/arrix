//! The document: parameters, the DAG, features, commands, undo, the
//! evaluator and the `.arrx` format (docs/DATA-MODEL.md).

mod assembly;
mod authority;
mod bodies;
mod command;
mod core_blend;
mod core_boolean;
mod core_choice;
mod core_extrude;
mod core_hole;
mod core_import;
mod core_mirror;
mod core_pattern;
mod core_revolve;
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
mod zip;

pub use assembly::{Anchor, Assembly, Component, ComponentOf, Joint, JointKind, Pose};
pub use authority::{AuthorId, Authority, Change, CommandEnvelope, Generation, Outcome, Rejected};
pub use command::{Applied, Command, CommandError, FeatureEdit, apply};
pub use core_blend::CoreBlend;
pub use core_boolean::CoreBoolean;
pub use core_extrude::CoreExtrude;
pub use core_hole::CoreHole;
pub use core_import::CoreImportStep;
pub use core_mirror::CoreMirror;
pub use core_pattern::CorePattern;
pub use core_revolve::CoreRevolve;
pub use core_types::{CoreSketch, DatumPlane, WORLD_PLANES};
pub use dag::{Dag, DagError, Node};
pub use document::{
    Document, FeatureRecord, FeatureTypeId, ImportedFile, Invalid, InvalidFeatureTypeId, Param,
    Part, is_param_name,
};
#[cfg(feature = "schema")]
pub use eval::eval_line_schema;
pub use eval::{
    AssemblyOutcome, BodyLine, BodyMeasures, BodySlot, EVAL_FORMAT, EvalEvent, EvalLine,
    EvalStatus, Evaluation, Evaluator, ExportError, ExportedFile, FailedCall, FeatureLine,
    FeatureOutcome, FinalBody, FrozenInfo, FrozenReason, InputHash, JointOutcome, ParamLine,
    PlacedComponent, RegionView, Run, SavedRecipe, SketchView, SlotView, Sweep, SweepError,
    SweepPoint, eval, export, run, sweep,
};
pub use frozen::{BlobRef, Frozen, FrozenResult, InvalidBlobRef};
pub use open::{DOCUMENT_JSON, DocumentSource, MemorySource, OpenError, PARAMS_JSON, SCHEMA, open};
pub use registry::{
    FeatureArgs, FeatureType, FileArg, FileKernel, Registry, RegistryError, SketchArgs, TypeOutput,
};
pub use save::{save, to_json};
#[cfg(feature = "schema")]
pub use session::protocol_schemas;
pub use session::{
    DocHash, EventStream, LocalSession, PROTOCOL_VERSION, ParamValues, PluginLoader, PreviewBody,
    PreviewId, PreviewTag, Replica, ReplicaError, Request, Session, SessionEvent,
};
pub use zip::{ZipError, from_zip, open_zip, save_zip, to_zip};
