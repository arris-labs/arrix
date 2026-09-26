//! Residual-side entry points.
//!
//! The formulas themselves live in [`super::eval`] — one definition per
//! constraint kind, shared with the Jacobian and with reference dimensions.
//! What is left here is what *asks* for residuals.

use crate::ids::ConstraintId;
use crate::sketch::Sketch;

use super::eval::{ConstraintEval, eval_constraint};
use super::system::*;

/// Per-constraint residual magnitudes after a solve — used by diagnostics
/// to point at the conflicting set.
pub fn constraint_residuals(sketch: &Sketch) -> Vec<(ConstraintId, f64)> {
    let sys = System::build(sketch);
    let mut eval = ConstraintEval::residuals_only();
    sketch
        .constraints
        .iter()
        .filter(|(_, r)| r.is_active && r.is_driving)
        .map(|(id, r)| {
            eval_constraint(sketch, &r.constraint, Some(&sys.blocked_targets), &mut eval);
            (*id, residual_norm(&eval.take_residuals()))
        })
        .collect()
}

pub(crate) fn residual_norm(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum::<f64>().sqrt()
}
