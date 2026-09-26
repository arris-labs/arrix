//! Modify operations: edits that change a sketch's topology, not just its
//! pose (docs/DATA-MODEL.md §Sketches).
//!
//! Each one is a pure edit of a [`Draft`](crate::Draft), whose minter
//! supplies the ids of what it adds; the client turns the difference into
//! one command. None is solver surface: they add entities and constraints
//! of the kinds that already exist. Every one checks before it writes, so
//! an error leaves the sketch untouched.
//!
//! Ids survive where geometry survives: a shortened line keeps its
//! `EntityId`, so the side face it sweeps keeps its name and the features
//! that reference it keep resolving (`SEED.md` §8.2, no silent re-bind).

mod chain;
mod extend;
mod mirror;
mod offset;
mod preview;
mod transfer;
mod trim;

pub use chain::{Chain, Link, find_chain};
pub use extend::{break_at, extend};
pub use mirror::{MirrorAxis, MirrorEdit, mirror};
pub use offset::{OffsetEdit, offset, offset_distance_at, offset_preview};
pub use preview::{break_preview, curve_samples, extend_preview, pick_curve, trim_preview};
pub use transfer::{Cut, Verdict, verdict};
pub use trim::{TrimEdit, trim};

/// Why a modify operation refused, leaving the sketch as it was. The app
/// shows it as a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ModifyError {
    /// An offset distance would collapse a curve of the chain (a line to
    /// nothing, an arc past its centre) or leave two of its curves without
    /// a crossing.
    #[error("too large for the geometry")]
    TooLarge,
    /// An offset distance of zero or not a number. It is signed, so a
    /// negative one is a side, not an error.
    #[error("the distance must be a number other than zero")]
    NotPositive,
    /// Not a line, circle or arc the arrangement cuts: construction
    /// geometry, a conic, a B-spline, or no such entity.
    #[error("only drawn lines, circles and arcs can be trimmed")]
    NotTrimmable,
    /// Extend found no curve along the end's continuation, or break found
    /// no crossing on the curve.
    #[error("there is no crossing to reach")]
    NoCrossing,
    /// A full circle has no end to extend, and one crossing does not open
    /// it into two pieces.
    #[error("a full circle has no end to extend or break at")]
    Closed,
    /// Offset needs a drawn line, arc or circle, or a chain of them: not a
    /// point, a conic, or a curve that folds back on itself at a joint.
    #[error("only drawn lines, arcs and circles can be offset")]
    NotAChain,
    /// Mirror needs a line or a datum axis (not the origin, not a
    /// zero-length line) to reflect across.
    #[error("mirror across a line or a datum axis")]
    NoAxis,
    /// Mirror found nothing to copy: an empty selection, or only the axis,
    /// conics, and lines or circles on the axis.
    #[error("select the lines, arcs, circles or points to mirror")]
    NothingToMirror,
}
