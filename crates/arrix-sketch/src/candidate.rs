//! The insert-time pre-check: what a constraint *would* do to the sketch, asked before it is added, so an
//! over-constraining dimension can be offered as a driven one and a
//! contradicting relation refused — instead of the sketch turning red and
//! the user hunting for what they just broke. The modify operations keep
//! each row they add only on its `Ok`.
//!
//! No new analysis: the candidate goes onto a clone and the verdict is read
//! off [`solver::analyze_system`]'s leverage scores, the same ones
//! [`crate::Diagnostics`] reports a committed system with.

use std::collections::BTreeSet;

use arrix_core::Id;

use crate::constraint::{Constraint, ConstraintRecord};
use crate::ids::{ConstraintId, SketchEntityId};
use crate::sketch::Sketch;
use crate::solver::{self, RESIDUAL_TOL};

/// What adding one constraint would do to a sketch. The ids are the
/// constraints already in the sketch that the verdict is *about* — what
/// already implies the candidate, or what it cannot be met together with.
/// They can be empty: geometry that cannot move (fixed points)
/// over-determines a candidate with no constraint to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateVerdict {
    /// The candidate removes degrees of freedom, or at least cannot be
    /// shown not to: a verdict the analysis cannot prove is `Ok`.
    Ok,
    /// Already true and already enforced: the candidate's equations are
    /// satisfied as the sketch stands and depend on these constraints'.
    Redundant(Vec<ConstraintId>),
    /// Cannot be met: with the candidate in, the solver is left with
    /// residual on equations that depend on one another — these and the
    /// candidate's.
    Conflicting(Vec<ConstraintId>),
}

impl CandidateVerdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

/// A satisfied equation, as [`solver::analyze_system`] draws the line.
const SATISFIED_TOL: f64 = RESIDUAL_TOL * 100.0;

impl Sketch {
    /// The verdict on adding `candidate` as an active driving constraint.
    /// `self` is not changed, and is taken to be solved — the working state
    /// every gesture leaves behind.
    ///
    /// Two questions, split by whether the candidate already holds. If it
    /// does, nothing would move, so its rows are tested for dependency at
    /// the pose as it stands: dependent is [`CandidateVerdict::Redundant`].
    /// If it does not, the clone is solved: residual left on dependent rows
    /// that were not there before is [`CandidateVerdict::Conflicting`].
    /// Anything else is `Ok` — including a solve that merely ran out of
    /// iterations (`SketchStatus::DidNotConverge`), which is no proof of a
    /// conflict, and a candidate whose own Jacobian rows vanish at the pose
    /// (a tangency at a shared endpoint has no first-order residual): a
    /// zero row "depends" on everything and says nothing.
    pub fn check_candidate(&self, candidate: &Constraint) -> CandidateVerdict {
        let before = solver::analyze_system(self);
        let mut trial = self.clone();
        let id = unused_id(&trial);
        trial
            .insert_constraint(ConstraintRecord::new(id, candidate.clone()))
            .expect("the candidate names what the sketch holds");

        let satisfied = solver::constraint_residuals(&trial)
            .into_iter()
            .find(|(cid, _)| *cid == id)
            .is_none_or(|(_, magnitude)| magnitude <= SATISFIED_TOL);

        if satisfied {
            if !is_carried(&trial, id) {
                // Every point it touches is immovable: true, and enforced
                // by whatever fixed them.
                return CandidateVerdict::Redundant(Vec::new());
            }
            let after = solver::analyze_system(&trial);
            if !after.redundant_constraints.contains(&id) || has_vanishing_rows(&trial, id) {
                return CandidateVerdict::Ok;
            }
            return CandidateVerdict::Redundant(newly_listed(
                &after.redundant_constraints,
                &before.redundant_constraints,
                id,
            ));
        }

        let result = trial.solve();
        if result.residual_norm <= RESIDUAL_TOL * 1e3 {
            return CandidateVerdict::Ok;
        }
        let after = solver::analyze_system(&trial);
        let others = newly_listed(
            &after.dependent_unsatisfied,
            &before.dependent_unsatisfied,
            id,
        );
        if after.dependent_unsatisfied.contains(&id) || !others.is_empty() {
            CandidateVerdict::Conflicting(others)
        } else {
            CandidateVerdict::Ok
        }
    }
}

/// An id no record in `sketch` holds, for a record that lives only on a
/// trial clone and never reaches a document: the lowest free one, so the
/// check reads no minter.
fn unused_id(sketch: &Sketch) -> SketchEntityId {
    (0..)
        .map(|n| SketchEntityId::from(Id(n)))
        .find(|id| !sketch.contains_id(*id))
        .expect("a sketch holds fewer than 2^64 records")
}

/// The constraints `after` lists that `before` did not, the candidate
/// itself left out: what the candidate's arrival made dependent.
fn newly_listed(
    after: &[ConstraintId],
    before: &[ConstraintId],
    candidate: ConstraintId,
) -> Vec<ConstraintId> {
    let before: BTreeSet<_> = before.iter().collect();
    after
        .iter()
        .filter(|cid| **cid != candidate && !before.contains(cid))
        .copied()
        .collect()
}

/// Whether a subsystem with anything to move carries `id` —
/// `System::partition` sets a constraint with no free variable under it
/// aside.
fn is_carried(sketch: &Sketch, id: ConstraintId) -> bool {
    solver::System::build(sketch)
        .partition(sketch)
        .iter()
        .any(|sub| sub.n_vars() > 0 && sub.constraint_ids.contains(&id))
}

/// Whether every Jacobian row `id` owns is numerically zero at the current
/// pose, measured against the largest entry any row has. Dense, because a
/// sparse row may spell a zero as entries that cancel.
fn has_vanishing_rows(sketch: &Sketch, id: ConstraintId) -> bool {
    let sys = solver::System::build(sketch);
    let x = sys.pack(sketch);
    let rows = solver::jacobian(&sys, sketch, &x).to_dense();
    let peak = |row: &Vec<f64>| row.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
    let scale = rows.iter().map(peak).fold(0.0_f64, f64::max);
    let mut own = sys
        .residual_owners
        .iter()
        .zip(&rows)
        .filter(|(owner, _)| owner.constraint_id() == Some(id))
        .peekable();
    own.peek().is_some() && own.all(|(_, row)| peak(row) <= scale * 1e-6)
}
