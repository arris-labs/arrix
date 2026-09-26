//! The sketch model and its constraint solver (docs/UI-RENDERING.md §Sketch
//! mode, docs/DATA-MODEL.md §Sketches). Pure math: no kernel, no egui.
//!
//! Ported from the predecessor prototype (`SEED.md` §8.1) and held to
//! ArriX's rules on the way in: ids are the document's `SketchEntityId`s,
//! minted by a command's author ([`Draft`]); SI inside; the constraint
//! kinds a document can hold are the ones M3's sketch mode reaches, and
//! the rest sit behind the `conics` and `snells-law` features.

mod arrangement;
#[cfg(feature = "conics")]
pub mod bspline;
mod constraint;
mod diagnostics;
mod draft;
mod dsu;
mod entity;
mod ids;
pub mod instrument;
mod region;
mod sketch;
mod solver;
mod validation;

pub use arrangement::{
    ArrangementEdge, ArrangementLoop, EntityPiece, RegionOutline, arrangement_loops,
    entity_intersections, entity_pieces,
};
#[cfg(feature = "conics")]
pub use constraint::AlignmentKind;
pub use constraint::{
    Constraint, ConstraintPriority, ConstraintRecord, ConstraintRefs, DatumEntity,
};
pub use diagnostics::{Diagnostics, EntityConstraintState, SketchStatus};
pub use draft::Draft;
pub use entity::{Entity, Point};
pub use ids::{ConstraintId, EntityId, PointId, SketchEntityId};
pub use region::{
    EdgeGeom, EdgePiece, Loop, LoopEdge, Profile, Region, RegionKey, find_profiles,
    find_region_outlines, find_regions,
};
pub use sketch::{Sketch, SketchError};
pub use solver::{
    DragFrame, DragSession, RESIDUAL_TOL, ResidualOwner, SolveResult, SolverAlgorithm,
    SolverOptions, SolverSystemAnalysis, Var, analyze_system, constraint_residual_count,
    constraint_residuals, matrix_rank, measured_value, row_space_basis, solve, solve_with_drag,
    solve_with_options,
};
pub use validation::{
    DegenerateEntity, DegenerateKind, OpenGap, OpenVertex, SketchValidation, ValidationOptions,
    ValidationReport,
};

// Test utilities for the gradient suite and the benchmarks, not public API.
#[doc(hidden)]
pub use solver::{
    JacobianRows, Pin, System, jacobian, jacobian_finite_difference, set_var, var_chain_scale,
};
