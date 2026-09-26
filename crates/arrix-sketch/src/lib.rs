//! The sketch model and its constraint solver (docs/UI-RENDERING.md §Sketch
//! mode, docs/DATA-MODEL.md §Sketches). Pure math: no kernel, no egui.
//!
//! Ported from the predecessor prototype (`SEED.md` §8.1) and held to
//! ArriX's rules on the way in: ids are the document's `SketchEntityId`s,
//! minted by a command's author ([`Draft`]); SI inside; the constraint
//! kinds a document can hold are the ones M3's sketch mode reaches, and
//! the rest sit behind the `conics` and `snells-law` features.

mod constraint;
mod draft;
mod entity;
mod ids;
mod sketch;

#[cfg(feature = "conics")]
pub use constraint::AlignmentKind;
pub use constraint::{
    Constraint, ConstraintPriority, ConstraintRecord, ConstraintRefs, DatumEntity,
};
pub use draft::Draft;
pub use entity::{Entity, Point};
pub use ids::{ConstraintId, EntityId, PointId, SketchEntityId};
pub use sketch::{Sketch, SketchError};
