//! The drag's tangent step (docs/UI-RENDERING.md §Sketch mode): the
//! Gauss–Newton step for the pin's rows inside the null space of the
//! primary Jacobian, solved densely over the variables the simple rows
//! leave free.
//!
//! Most rows of a drawn sketch are a horizontal, a vertical or a
//! coincidence: two entries, equal and opposite, so to first order the two
//! variables move together. A row with one entry (a point on an axis) holds
//! its variable still. Those rows are eliminated before anything dense is
//! built: the variables they tie are merged, the ones they hold dropped,
//! and each merged group of `k` is one column scaled by `1/√k`. That map
//! has orthonormal columns and spans exactly the null space of the
//! eliminated rows, so the reduced problem's minimum-norm step is the full
//! problem's, and the dense work shrinks with the sketch's freedom instead
//! of its size. Only exact negation merges; a row that merely looks close
//! stays a row.

use std::collections::BTreeMap;

use crate::constraint::ConstraintPriority;
use crate::dsu::Dsu;
use crate::sketch::Sketch;

use super::algorithms::{dot, solve_normal};
use super::jacobian::jacobian;
use super::linalg::row_space_basis;
use super::system::System;

/// The smallest step that, to first order, leaves every primary residual
/// where it is and cuts the pin's error the most: the Gauss–Newton step for
/// the drag-priority rows, projected onto the primary Jacobian's null
/// space. `None` if the projected system cannot be solved.
pub(super) fn tangent_step(sys: &System, sketch: &Sketch, x: &[f64]) -> Option<Vec<f64>> {
    let n = sys.n_vars();
    let rows: Vec<Vec<(usize, f64)>> = jacobian(sys, sketch, x)
        .rows
        .iter()
        .map(|row| summed(row, n))
        .collect();
    let r = sys.residuals_of(sketch);
    let reduction = Reduction::of(&rows, &sys.residual_priorities, n);
    let (mut primary, mut soft, mut e) = (Vec::new(), Vec::new(), Vec::new());
    for (i, ((row, p), ri)) in rows.iter().zip(&sys.residual_priorities).zip(r).enumerate() {
        if *p == ConstraintPriority::Primary {
            if !reduction.eliminated[i] {
                primary.push(reduction.dense(row));
            }
        } else {
            soft.push(reduction.dense(row));
            e.push(ri);
        }
    }
    let u = projected_least_squares(&primary, soft, &e)?;
    Some(reduction.expand(&u, n))
}

/// A sparse row with its duplicate columns summed and its zeros dropped,
/// in column order.
fn summed(row: &[(usize, f64)], n: usize) -> Vec<(usize, f64)> {
    let mut acc = BTreeMap::new();
    for &(col, val) in row {
        if col < n {
            *acc.entry(col).or_insert(0.0) += val;
        }
    }
    acc.into_iter().filter(|(_, v)| *v != 0.0).collect()
}

/// The variables the simple primary rows leave free, as reduced columns.
struct Reduction {
    /// Each variable's reduced column and its `1/√k` weight; `None` if a
    /// row holds it still.
    column: Vec<Option<(usize, f64)>>,
    n_reduced: usize,
    /// Which rows the reduction absorbed: they are zero in every step it
    /// can express.
    eliminated: Vec<bool>,
}

impl Reduction {
    fn of(rows: &[Vec<(usize, f64)>], priorities: &[ConstraintPriority], n: usize) -> Self {
        let mut groups = Dsu::new();
        let mut held = vec![false; n];
        let mut eliminated = vec![false; rows.len()];
        for (i, (row, p)) in rows.iter().zip(priorities).enumerate() {
            if *p != ConstraintPriority::Primary {
                continue;
            }
            match row.as_slice() {
                [(a, _)] => held[*a] = true,
                [(a, va), (b, vb)] if *va == -*vb => {
                    groups.union(*a, *b);
                }
                _ => continue,
            }
            eliminated[i] = true;
        }
        let root: Vec<usize> = (0..n).map(|v| groups.find(v)).collect();
        let mut root_held = vec![false; n];
        let mut size = vec![0usize; n];
        for v in 0..n {
            root_held[root[v]] |= held[v];
            size[root[v]] += 1;
        }
        let mut index = vec![None; n];
        let mut n_reduced = 0;
        let column = (0..n)
            .map(|v| {
                let g = root[v];
                if root_held[g] {
                    return None;
                }
                let k = *index[g].get_or_insert_with(|| {
                    n_reduced += 1;
                    n_reduced - 1
                });
                Some((k, 1.0 / (size[g] as f64).sqrt()))
            })
            .collect();
        Self {
            column,
            n_reduced,
            eliminated,
        }
    }

    /// `row` times the reduction's map, dense over the reduced columns.
    fn dense(&self, row: &[(usize, f64)]) -> Vec<f64> {
        let mut out = vec![0.0; self.n_reduced];
        for &(col, val) in row {
            if let Some((k, w)) = self.column[col] {
                out[k] += val * w;
            }
        }
        out
    }

    /// A reduced step back over all `n` variables.
    fn expand(&self, u: &[f64], n: usize) -> Vec<f64> {
        (0..n)
            .map(|v| self.column[v].map_or(0.0, |(k, w)| u[k] * w))
            .collect()
    }
}

/// The minimum-norm `t` minimising `|soft t + e|` with `primary t = 0`.
fn projected_least_squares(
    primary: &[Vec<f64>],
    mut soft: Vec<Vec<f64>>,
    e: &[f64],
) -> Option<Vec<f64>> {
    let n = soft.first().map_or(0, Vec::len);
    let basis = row_space_basis(primary);
    for row in &mut soft {
        for q in &basis {
            let d = dot(row, q);
            row.iter_mut().zip(q).for_each(|(a, b)| *a -= d * b);
        }
    }
    // Minimum-norm least squares of `A t = −e`, solved in an orthonormal
    // basis `P` of `A`'s row space: `t = Pᵀ y` with `(A Pᵀ) y = −e`. A
    // direction the constraints lock is simply not in `P`; damping
    // `A Aᵀ` instead would divide round-off by the damping and step along it.
    let p = row_space_basis(&soft);
    let r = p.len();
    if r == 0 {
        return None;
    }
    let m: Vec<Vec<f64>> = soft
        .iter()
        .map(|row| p.iter().map(|q| dot(row, q)).collect())
        .collect();
    let mut mtm = vec![0.0; r * r];
    let mut y = vec![0.0; r];
    for (mi, ei) in m.iter().zip(e) {
        for a in 0..r {
            y[a] -= mi[a] * ei;
            for b in 0..r {
                mtm[a * r + b] += mi[a] * mi[b];
            }
        }
    }
    if !solve_normal(&mut mtm, &mut y, r) {
        return None;
    }
    let mut t = vec![0.0; n];
    for (q, ya) in p.iter().zip(&y) {
        t.iter_mut().zip(q).for_each(|(ti, qi)| *ti += ya * qi);
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Constraint;
    use crate::draft::Draft;
    use crate::solver::system::Pin;

    /// The full problem, unreduced: what the reduction must equal.
    fn unreduced(sys: &System, sketch: &Sketch, x: &[f64]) -> Option<Vec<f64>> {
        let dense = jacobian(sys, sketch, x).to_dense();
        let r = sys.residuals_of(sketch);
        let (mut primary, mut soft, mut e) = (Vec::new(), Vec::new(), Vec::new());
        for ((row, p), ri) in dense.into_iter().zip(&sys.residual_priorities).zip(r) {
            if *p == ConstraintPriority::Primary {
                primary.push(row);
            } else {
                soft.push(row);
                e.push(ri);
            }
        }
        projected_least_squares(&primary, soft, &e)
    }

    /// Simple rows (a coincidence, a horizontal, a point on an axis) beside
    /// nonlinear ones (a length, an angle): the reduced step is the full
    /// one to round-off, which is all that separates the drag's positions
    /// from the unreduced solver's (about 1e-8 m after 60 frames of a
    /// nonlinear drag, against the 1 µm length tolerance).
    #[test]
    fn the_reduced_step_is_the_full_one() {
        let mut s = Draft::seeded(3);
        let (a0, a1, base) = s.add_line(0.0, 0.0, 0.05, 0.0);
        let (b0, tip, arm) = s.add_line(0.0, 0.001, 0.04, 0.025);
        s.point_mut(a0).unwrap().fixed = true;
        s.add_constraint(Constraint::Coincident { a: a0, b: b0 });
        s.add_constraint(Constraint::Horizontal { line: base });
        s.add_constraint(Constraint::Angle {
            a: base,
            b: arm,
            value: 0.5,
        });
        let (c0, c1, l) = s.add_line(0.04, 0.025, 0.07, 0.03);
        s.add_constraint(Constraint::Coincident { a: tip, b: c0 });
        s.add_constraint(Constraint::Horizontal { line: l });
        s.add_constraint(Constraint::Distance {
            a: c0,
            b: c1,
            value: 0.03,
        });
        let _ = a1;
        let pins = [Pin::Point {
            point: c1,
            x: 0.08,
            y: 0.04,
        }];
        let sys = System::build_with_pins(&s, &pins);
        let x = sys.pack(&s);
        let reduced = tangent_step(&sys, &s, &x).expect("a step");
        let full = unreduced(&sys, &s, &x).expect("a step");
        let scale = full.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(scale > 1e-4, "the pin pulls: {full:?}");
        for (r, f) in reduced.iter().zip(&full) {
            assert!((r - f).abs() <= 1e-12 * scale, "{reduced:?} vs {full:?}");
        }
        let eliminated = Reduction::of(
            &jacobian(&sys, &s, &x)
                .rows
                .iter()
                .map(|row| summed(row, sys.n_vars()))
                .collect::<Vec<_>>(),
            &sys.residual_priorities,
            sys.n_vars(),
        );
        assert!(
            eliminated.n_reduced < sys.n_vars(),
            "the simple rows were eliminated"
        );
    }
}
