//! Analytical Jacobian assembly.
//!
//! The derivatives themselves are written down once, next to the residual they
//! belong to, in [`super::eval`]; this module packs what that yields into
//! [`JacobianRows`], applies the radius chain factor, and keeps a
//! finite-difference Jacobian alongside for tests.

use arrix_core::LENGTH_TOLERANCE;

use crate::entity::Entity;
use crate::sketch::Sketch;

use super::eval::{ConstraintEval, Grad, eval_constraint};
use super::system::*;
use super::*;

pub fn var_chain_scale(sys: &System, x: &[f64]) -> Vec<f64> {
    sys.vars
        .iter()
        .zip(x)
        .map(|(v, &xi)| match v {
            Var::PointX(_) | Var::PointY(_) => 1.0,
            #[cfg(feature = "conics")]
            Var::EllipseMinorRadius(_) if xi.abs() <= LENGTH_TOLERANCE => 0.0,
            #[cfg(feature = "conics")]
            Var::EllipseMinorRadius(_) => xi.signum(),
            Var::CircleRadius(_) if xi.abs() <= LENGTH_TOLERANCE => 0.0,
            Var::CircleRadius(_) => xi.signum(),
        })
        .collect()
}

/// `jacobian`'s debug-only contract check: does `sketch` currently hold the
/// geometry `x` unpacks to?
pub(crate) fn is_unpacked_at(sys: &System, sketch: &Sketch, x: &[f64]) -> bool {
    if x.len() != sys.n_vars() {
        return false;
    }
    sys.vars.iter().zip(x).all(|(v, &xi)| match *v {
        Var::PointX(id) => sketch.points.get(&id).is_some_and(|p| p.x == xi),
        Var::PointY(id) => sketch.points.get(&id).is_some_and(|p| p.y == xi),
        // `set_var` takes `|x|` and floors it at `LENGTH_TOLERANCE`, so compare
        // magnitudes with that slack — but nothing further away.
        Var::CircleRadius(id) => match sketch.entities.get(&id) {
            Some(Entity::Circle { radius, .. }) => {
                (radius.abs() - xi.abs()).abs() <= LENGTH_TOLERANCE
            }
            _ => false,
        },
        #[cfg(feature = "conics")]
        Var::EllipseMinorRadius(id) => match sketch.entities.get(&id) {
            Some(Entity::Ellipse { minor_radius, .. })
            | Some(Entity::ArcOfEllipse { minor_radius, .. }) => {
                (minor_radius.abs() - xi.abs()).abs() <= LENGTH_TOLERANCE
            }
            _ => false,
        },
    })
}

/// Computes the Jacobian rows for the sketch constraint system at point `x`.
///
/// Contract: `sketch` must already be unpacked at `x` (see
/// [`System::unpack_into`]). Every arm reads geometry straight off `sketch`
/// and writes derivatives in geometry terms; the only place `x` itself is
/// consulted is [`var_chain_scale`], which supplies the
/// `radius = |x|.max(tol)` chain factor that geometry alone cannot recover.
pub fn jacobian(sys: &System, sketch: &Sketch, x: &[f64]) -> JacobianRows {
    debug_assert!(
        is_unpacked_at(sys, sketch, x),
        "jacobian(): sketch is not unpacked at x — every derivative would be \
         linearized at the wrong point"
    );

    let n = sys.n_vars();
    let m = sys.n_residuals();
    let chain_scale = var_chain_scale(sys, x);

    let mut eval = ConstraintEval::with_jacobian(sys);
    for &cid in &sys.constraint_ids {
        if let Some(record) = sketch.constraints.get(&cid)
            && record.is_active
            && record.is_driving
        {
            eval_constraint(
                sketch,
                &record.constraint,
                Some(&sys.blocked_targets),
                &mut eval,
            );
        }
    }
    let mut rows = eval.into_rows();

    // The implicit equal-radii row every arc carries, appended in the same
    // order `System::build` reserved owners for them.
    for &eid in &sys.arc_entity_ids {
        if let Some(Entity::Arc { center, start, end }) = sketch.entities.get(&eid) {
            let mut arc_row = Vec::new();
            let mut g = Grad::new(sys, &mut arc_row);
            let c = &sketch.points[center];
            let s = &sketch.points[start];
            let e = &sketch.points[end];
            let ds_x = s.x - c.x;
            let ds_y = s.y - c.y;
            let rs = (ds_x * ds_x + ds_y * ds_y).sqrt();
            let (us_x, us_y) = if rs >= LENGTH_TOLERANCE {
                (ds_x / rs, ds_y / rs)
            } else {
                (0.0, 0.0)
            };
            let de_x = e.x - c.x;
            let de_y = e.y - c.y;
            let re = (de_x * de_x + de_y * de_y).sqrt();
            let (ue_x, ue_y) = if re >= LENGTH_TOLERANCE {
                (de_x / re, de_y / re)
            } else {
                (0.0, 0.0)
            };
            g.pt(*start, us_x, us_y);
            g.pt(*end, -ue_x, -ue_y);
            g.pt(*center, ue_x - us_x, ue_y - us_y);
            rows.push(arc_row);
        }
    }

    // A pin's rows, after the arcs' as `System::build_with_pins` put them:
    // two on a point's columns, one on a radius'.
    for pin in &sys.pins {
        let col = pin.col(sys).expect("build_with_pins keeps only free pins");
        for slot in 0..pin.rows() {
            rows.push(vec![(col + slot, 1.0)]);
        }
    }

    // `radius = |x|.max(tol)`: the one chain factor geometry alone cannot
    // recover, so it is applied here rather than inside any evaluator.
    for row in &mut rows {
        for (col, val) in row.iter_mut() {
            *val *= chain_scale[*col];
        }
    }

    debug_assert_eq!(rows.len(), m);
    JacobianRows { rows, n_cols: n }
}

/// Numerical Jacobian: central differences on each free variable.
#[doc(hidden)]
pub fn jacobian_finite_difference(sys: &System, sketch: &mut Sketch, x: &[f64]) -> Vec<Vec<f64>> {
    let m = sys.n_residuals();
    let n = sys.n_vars();
    let mut j = vec![vec![0.0; n]; m];
    if n == 0 || m == 0 {
        return j;
    }

    let mut x_plus = x.to_vec();
    let mut x_minus = x.to_vec();
    for col in 0..n {
        x_plus[col] = x[col] + FD_STEP;
        x_minus[col] = x[col] - FD_STEP;

        sys.unpack_into(&x_plus, sketch);
        let r_plus = sys.residuals_of(sketch);
        sys.unpack_into(&x_minus, sketch);
        let r_minus = sys.residuals_of(sketch);

        for row in 0..m {
            j[row][col] = (r_plus[row] - r_minus[row]) / (2.0 * FD_STEP);
        }

        x_plus[col] = x[col];
        x_minus[col] = x[col];
    }
    // Restore the base state.
    sys.unpack_into(x, sketch);
    j
}
