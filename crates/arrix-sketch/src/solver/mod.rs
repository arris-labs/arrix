//! The sketch constraint solver: Powell's DogLeg by default, Levenberg–
//! Marquardt selectable, over independent subsystems
//! (docs/UI-RENDERING.md §Sketch mode).
//!
//! Under-constrained is normal: the solver minimises movement from the
//! current (dragged) state via soft damping on free variables. Over-
//! constrained / conflicting systems leave a high residual and are reported
//! through [`crate::diagnostics`].
//!
//! The per-constraint math lives in [`eval`] — one definition per kind,
//! serving residuals, the analytical Jacobian ([`jacobian`]), the
//! sparsity [`System::partition`] decomposes on, and the measurement a
//! reference dimension displays ([`measured_value`]).

use crate::constraint::Constraint;
use crate::entity::Entity;
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

/// Absolute residual convergence threshold, in meters (or radians for
/// angles — same order of magnitude for sketch-sized geometry).
pub const RESIDUAL_TOL: f64 = 1e-9;
/// Stop if the parameter step is smaller than this.
const STEP_TOL: f64 = 1e-12;
const MAX_ITERATIONS: usize = 80;
/// Finite-difference step for the numerical Jacobian.
const FD_STEP: f64 = 1e-7;
/// Initial LM damping; scaled relative to `JᵀJ` diagonal.
const LAMBDA0: f64 = 1e-3;

#[derive(Debug, Clone, PartialEq)]
pub struct SolveResult {
    pub converged: bool,
    pub iterations: usize,
    pub residual_norm: f64,
    /// Soft estimate: `n_vars − rank(J)`. Negative means over-constrained
    /// (more independent residuals than free variables).
    pub dof: i32,
    pub rank: usize,
    pub n_vars: usize,
    pub n_residuals: usize,
}

/// Non-linear least-squares algorithm choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SolverAlgorithm {
    /// Powell's DogLeg trust-region algorithm combining Gauss-Newton and Steepest Descent.
    #[default]
    DogLeg,
    /// Levenberg-Marquardt damped least-squares.
    LevenbergMarquardt,
}

/// Configuration options for the sketch solver.
#[derive(Debug, Clone)]
pub struct SolverOptions {
    pub algorithm: SolverAlgorithm,
    pub max_iterations: usize,
    pub residual_tol: f64,
    pub step_tol: f64,
    /// Start DogLeg's trust region at a tenth of the sketch's size instead
    /// of at 1 m. The drag path sets it; `solve` does not, because on
    /// a rescaled plate (a parameter edit's jump) it changes the outcome.
    pub sketch_scaled_trust: bool,
}

impl Default for SolverOptions {
    fn default() -> Self {
        Self {
            algorithm: SolverAlgorithm::DogLeg,
            max_iterations: MAX_ITERATIONS,
            residual_tol: RESIDUAL_TOL,
            step_tol: STEP_TOL,
            sketch_scaled_trust: false,
        }
    }
}

/// Sparse rows as unsorted (column, value) pairs. Duplicates are legal and are
/// summed on consumption.
#[derive(Debug, Clone, PartialEq)]
pub struct JacobianRows {
    pub rows: Vec<Vec<(usize, f64)>>,
    pub n_cols: usize,
}

impl JacobianRows {
    pub fn to_dense(&self) -> Vec<Vec<f64>> {
        let mut dense = vec![vec![0.0; self.n_cols]; self.rows.len()];
        for (r_idx, row) in self.rows.iter().enumerate() {
            for &(col, val) in row {
                if col < self.n_cols {
                    dense[r_idx][col] += val;
                }
            }
        }
        dense
    }
}

/// Who a residual row belongs to.
///
/// Arcs carry an implicit equal-radii row that no user constraint owns; it
/// used to be tagged with a synthetic `ConstraintId(entity | 1<<63)`, which
/// every reader then had to know not to look up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResidualOwner {
    /// Row `slot` of a user constraint's residual block.
    Constraint { id: ConstraintId, slot: usize },
    /// An arc's implicit `|C→S| == |C→E|` row.
    ArcRadii(EntityId),
    /// A drag pin's row: `slot` 0 is x, 1 is y ([`system::Pin::Point`]).
    Pin { point: PointId, slot: usize },
    /// A rim drag's row on a circle's radius ([`system::Pin::Radius`]).
    RadiusPin(EntityId),
}

impl ResidualOwner {
    /// The owning constraint, or `None` for an implicit row.
    pub fn constraint_id(self) -> Option<ConstraintId> {
        match self {
            Self::Constraint { id, .. } => Some(id),
            Self::ArcRadii(_) | Self::Pin { .. } | Self::RadiusPin(_) => None,
        }
    }
}

/// One free scalar in the packed variable vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Var {
    PointX(PointId),
    PointY(PointId),
    CircleRadius(EntityId),
    #[cfg(feature = "conics")]
    EllipseMinorRadius(EntityId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockedTarget {
    Point {
        x: f64,
        y: f64,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Circle {
        cx: f64,
        cy: f64,
        radius: f64,
    },
    Arc {
        cx: f64,
        cy: f64,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
    },
    #[cfg(feature = "conics")]
    Ellipse {
        cx: f64,
        cy: f64,
        mx: f64,
        my: f64,
        minor_radius: f64,
    },
    #[cfg(feature = "conics")]
    ArcOfEllipse {
        cx: f64,
        cy: f64,
        mx: f64,
        my: f64,
        minor_radius: f64,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
    },
    #[cfg(feature = "conics")]
    BSpline {
        control_points: Vec<(f64, f64)>,
    },
}

pub fn constraint_residual_count(c: &Constraint, sketch: &Sketch) -> usize {
    match c {
        // A `Block` freezes every DoF the entity has, so its row count is the
        // entity's, not the constraint's.
        Constraint::Block { entity } => match sketch.entities.get(entity) {
            Some(Entity::Point(_)) => 2,
            Some(Entity::Line { .. }) => 4,
            Some(Entity::Circle { .. }) => 3,
            Some(Entity::Arc { .. }) => 6,
            #[cfg(feature = "conics")]
            Some(Entity::Ellipse { .. }) => 5,
            #[cfg(feature = "conics")]
            Some(Entity::ArcOfEllipse { .. }) => 9,
            #[cfg(feature = "conics")]
            Some(Entity::BSpline { control_points, .. }) => 2 * control_points.len(),
            None => 0,
        },
        _ => c.residual_count(),
    }
}

pub(crate) mod algorithms;
pub(crate) mod drag;
pub(crate) mod eval;
pub(crate) mod guard;
pub(crate) mod jacobian;
pub(crate) mod linalg;
pub(crate) mod residuals;
pub(crate) mod system;

pub use algorithms::{solve, solve_with_drag, solve_with_options};
pub use drag::{DragFrame, DragSession};
pub use eval::measured_value;
pub use jacobian::{jacobian, jacobian_finite_difference, var_chain_scale};
pub use linalg::{SolverSystemAnalysis, analyze_system, matrix_rank, row_space_basis};
pub use residuals::constraint_residuals;
pub use system::{Pin, System, set_var};
