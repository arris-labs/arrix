//! The planar arrangement regions are found in.
//!
//! Curves that cross without sharing a point are intersected ([`intersect`])
//! and cut into pieces ([`split`]); [`walk`] orders those pieces by tangent
//! angle around each vertex and walks the faces they bound; [`faces`] says
//! which of those faces is a profile and which is a hole of which.
//! [`crate::region`] stays the facade the document reads.
//!
//! [`curve`] holds the geometry all of them measure in.

pub(crate) mod curve;
mod faces;
mod intersect;
mod split;
mod walk;

pub use faces::RegionOutline;
#[cfg(feature = "conics")]
pub(crate) use faces::SAMPLES_PER_TURN;
pub(crate) use faces::{FaceLoop, face_loops, nest};
pub use intersect::entity_intersections;
pub(crate) use intersect::{carrier_crossings, intersect};
pub use split::{EntityPiece, entity_pieces};
pub(crate) use split::{Piece, split_all};
pub use walk::{ArrangementEdge, ArrangementLoop, arrangement_loops};
