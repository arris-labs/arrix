//! One id space for a sketch's points, curves and constraints: the
//! document's [`SketchEntityId`], minted by the author of the command that
//! creates the record (docs/DATA-MODEL.md §Identifiers), never by this
//! crate. The three names say which map an id is looked up in; they are
//! one type, so an id is unique across all three.

pub use arrix_core::SketchEntityId;

/// A point's id.
pub type PointId = SketchEntityId;
/// A curve's id (a line, arc, circle, or a lone point entity).
pub type EntityId = SketchEntityId;
/// A constraint's id.
pub type ConstraintId = SketchEntityId;
