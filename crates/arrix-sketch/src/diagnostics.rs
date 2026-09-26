//! DoF count, over-constraint detection, and conflicting-constraint
//! reporting: an over-constrained sketch names the constraints to blame
//! (docs/UI-RENDERING.md §Sketch mode).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::constraint::Constraint;
use crate::entity::Entity;
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;
use crate::solver::{self, RESIDUAL_TOL, SolveResult, SolverSystemAnalysis};

/// Per-entity and per-point degree-of-freedom constraint state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityConstraintState {
    /// Entity or point is free to move (under-constrained) -> Cyan Blue / Yellow in UI.
    UnderConstrained,
    /// Entity or point is fully constrained / locked -> White / Dark in UI.
    FullyConstrained,
    /// Entity or point is fixed by a Fix constraint or fixed anchor -> Violet in UI.
    Fixed,
    /// Entity or point is involved in conflicting / over-constrained equations -> Bright Red in UI.
    Conflicting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SketchStatus {
    /// Residuals are small; DoF may still be > 0 (under-constrained is fine).
    Ok,
    /// Residuals small and DoF == 0.
    FullyConstrained,
    /// The constraint set is inconsistent: residual is left that cannot
    /// drop, because the unsatisfied equations depend on others
    /// (`SolverSystemAnalysis::dependent_unsatisfied`) — more independent
    /// residuals than free variables, or redundant-and-wrong. The fix is to
    /// remove or relax a constraint.
    OverConstrained,
    /// Residual is left, but every unsatisfied equation is independent: the
    /// set could be met, the solver stopped short of it — out of
    /// iterations, or started too far from a solution. The fix is to move
    /// the geometry nearer the intended shape, not to delete anything.
    DidNotConverge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostics {
    pub status: SketchStatus,
    pub dof: i32,
    pub residual_norm: f64,
    pub rank: usize,
    pub n_vars: usize,
    pub n_residuals: usize,
    /// Constraints whose residual magnitude stands out — the ones the UI
    /// should highlight when status is [`SketchStatus::OverConstrained`].
    pub conflicting: Vec<ConstraintId>,
    /// Constraints that are redundant (satisfied equations that do not contribute to rank).
    pub redundant: Vec<ConstraintId>,
    /// Constraint states for every entity in the sketch.
    pub entity_states: BTreeMap<EntityId, EntityConstraintState>,
    /// Constraint states for every point in the sketch.
    pub point_states: BTreeMap<PointId, EntityConstraintState>,
    pub message: String,
}

impl Diagnostics {
    /// Reads a solve's outcome: the verdict, the constraints to blame, the
    /// redundant ones, and a state for every point and entity, all from one
    /// rank analysis of the system at the solved geometry.
    pub fn analyze(sketch: &Sketch, solve: &SolveResult) -> Self {
        let analysis = solver::analyze_system(sketch);
        let status = verdict(solve, &analysis);
        let conflicting = if status == SketchStatus::OverConstrained {
            worst_residuals(solver::constraint_residuals(sketch))
        } else {
            Vec::new()
        };
        let (bad_points, bad_entities) = footprint(sketch, &conflicting);
        let point_states = point_states(sketch, &analysis, &bad_points);
        let entity_states = entity_states(sketch, &analysis, &point_states, &bad_entities);
        let message = message(sketch, &status, solve, &conflicting);
        Self {
            status,
            dof: solve.dof,
            residual_norm: solve.residual_norm,
            rank: solve.rank,
            n_vars: solve.n_vars,
            n_residuals: solve.n_residuals,
            conflicting,
            redundant: analysis.redundant_constraints,
            entity_states,
            point_states,
            message,
        }
    }

    /// Solve then analyse — the one-shot the UI calls after an edit.
    pub fn evaluate(sketch: &mut Sketch) -> (SolveResult, Self) {
        let result = solver::solve(sketch);
        let diag = Self::analyze(sketch, &result);
        (result, diag)
    }

    pub fn entity_state(&self, id: EntityId) -> EntityConstraintState {
        self.entity_states
            .get(&id)
            .copied()
            .unwrap_or(EntityConstraintState::UnderConstrained)
    }

    pub fn point_state(&self, id: PointId) -> EntityConstraintState {
        self.point_states
            .get(&id)
            .copied()
            .unwrap_or(EntityConstraintState::UnderConstrained)
    }

    pub fn is_redundant(&self, id: ConstraintId) -> bool {
        self.redundant.contains(&id)
    }

    pub fn is_conflicting(&self, id: ConstraintId) -> bool {
        self.conflicting.contains(&id)
    }
}

fn verdict(solve: &SolveResult, analysis: &SolverSystemAnalysis) -> SketchStatus {
    if solve.residual_norm > RESIDUAL_TOL * 1e3 {
        // Residual is left; whether it *could* drop separates the two
        // verdicts. At a stationary point with residual left, `Jᵀr = 0`
        // makes the unsatisfied rows linearly dependent: a conflict. Rows
        // that are all independent could still be reduced: the solver
        // stopped short.
        if analysis.dependent_unsatisfied.is_empty() {
            SketchStatus::DidNotConverge
        } else {
            SketchStatus::OverConstrained
        }
    } else if solve.dof < 0 {
        SketchStatus::OverConstrained
    } else if solve.dof == 0 {
        SketchStatus::FullyConstrained
    } else {
        SketchStatus::Ok
    }
}

/// The worst offenders of an over-constrained sketch: anything above half
/// the peak residual, at most eight, or nothing when every residual is
/// tiny.
fn worst_residuals(mut ranked: Vec<(ConstraintId, f64)>) -> Vec<ConstraintId> {
    let max_res = ranked.iter().map(|(_, m)| *m).fold(0.0_f64, f64::max);
    if max_res <= RESIDUAL_TOL * 10.0 {
        return Vec::new();
    }
    let threshold = (max_res * 0.5).max(RESIDUAL_TOL * 100.0);
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
        .into_iter()
        .filter(|&(_, mag)| mag >= threshold)
        .map(|(id, _)| id)
        .take(8)
        .collect()
}

/// The points and entities the conflicting constraints name, with the
/// points of every entity named.
fn footprint(
    sketch: &Sketch,
    conflicting: &[ConstraintId],
) -> (BTreeSet<PointId>, BTreeSet<EntityId>) {
    let mut points = BTreeSet::new();
    let mut entities = BTreeSet::new();
    for r in conflicting
        .iter()
        .filter_map(|id| sketch.constraints.get(id))
    {
        points.extend(r.constraint.referenced_points());
        for eid in r.constraint.referenced_entities() {
            entities.insert(eid);
            if let Some(e) = sketch.entities.get(&eid) {
                points.extend(e.point_ids());
            }
        }
    }
    (points, entities)
}

/// Whether each solver variable is pinned by the constraints, keyed by
/// variable.
fn var_locks(analysis: &SolverSystemAnalysis) -> BTreeMap<solver::Var, bool> {
    analysis
        .vars
        .iter()
        .copied()
        .zip(analysis.var_constrained.iter().copied())
        .collect()
}

fn point_states(
    sketch: &Sketch,
    analysis: &SolverSystemAnalysis,
    conflicting: &BTreeSet<PointId>,
) -> BTreeMap<PointId, EntityConstraintState> {
    let locks = var_locks(analysis);
    let fixed_by_constraint: BTreeSet<PointId> = sketch
        .constraints
        .values()
        .filter(|r| r.is_active && r.is_driving)
        .filter_map(|r| match r.constraint {
            Constraint::Fix { point, .. } => Some(point),
            _ => None,
        })
        .collect();
    let locked = |v| locks.get(&v).copied().unwrap_or(true);
    sketch
        .points
        .iter()
        .map(|(&pid, pt)| {
            let state = if conflicting.contains(&pid) {
                EntityConstraintState::Conflicting
            } else if pt.fixed || fixed_by_constraint.contains(&pid) {
                EntityConstraintState::Fixed
            } else if locked(solver::Var::PointX(pid)) && locked(solver::Var::PointY(pid)) {
                EntityConstraintState::FullyConstrained
            } else {
                EntityConstraintState::UnderConstrained
            };
            (pid, state)
        })
        .collect()
}

fn entity_states(
    sketch: &Sketch,
    analysis: &SolverSystemAnalysis,
    points: &BTreeMap<PointId, EntityConstraintState>,
    conflicting: &BTreeSet<EntityId>,
) -> BTreeMap<EntityId, EntityConstraintState> {
    let locks = var_locks(analysis);
    sketch
        .entities
        .iter()
        .map(|(&eid, entity)| {
            let state = if conflicting.contains(&eid) {
                EntityConstraintState::Conflicting
            } else {
                let point_states = entity.point_ids().into_iter().map(|p| {
                    points
                        .get(&p)
                        .copied()
                        .unwrap_or(EntityConstraintState::UnderConstrained)
                });
                combine(
                    point_states,
                    scalar_var(entity, eid).map(|v| locks.get(&v).copied().unwrap_or(false)),
                )
            };
            (eid, state)
        })
        .collect()
}

/// The entity's own solver variable besides its points: a circle's radius,
/// an ellipse's minor radius.
fn scalar_var(entity: &Entity, eid: EntityId) -> Option<solver::Var> {
    match entity {
        Entity::Circle { .. } => Some(solver::Var::CircleRadius(eid)),
        #[cfg(feature = "conics")]
        Entity::Ellipse { .. } | Entity::ArcOfEllipse { .. } => {
            Some(solver::Var::EllipseMinorRadius(eid))
        }
        _ => None,
    }
}

/// An entity's state from its points' and its scalar variable's: any
/// conflicting point makes it conflicting; all points fixed, with no
/// scalar of its own, makes it fixed (a scalar is pinned by a dimension,
/// never by a fix); all points fixed or fully constrained, and the scalar
/// locked, makes it fully constrained.
fn combine(
    points: impl Iterator<Item = EntityConstraintState>,
    scalar_locked: Option<bool>,
) -> EntityConstraintState {
    use EntityConstraintState as S;
    let (mut all_fixed, mut all_held) = (true, true);
    for st in points {
        match st {
            S::Conflicting => return S::Conflicting,
            S::Fixed => {}
            S::FullyConstrained => all_fixed = false,
            S::UnderConstrained => (all_fixed, all_held) = (false, false),
        }
    }
    match scalar_locked {
        None if all_fixed => S::Fixed,
        None if all_held => S::FullyConstrained,
        Some(true) if all_held => S::FullyConstrained,
        _ => S::UnderConstrained,
    }
}

fn message(
    sketch: &Sketch,
    status: &SketchStatus,
    solve: &SolveResult,
    conflicting: &[ConstraintId],
) -> String {
    match status {
        SketchStatus::Ok => format!("under-constrained ({} DoF)", solve.dof),
        SketchStatus::FullyConstrained => "fully constrained".into(),
        SketchStatus::DidNotConverge => format!(
            "did not converge (residual {:.3e}) — move the geometry nearer its intended shape",
            solve.residual_norm
        ),
        SketchStatus::OverConstrained if conflicting.is_empty() => format!(
            "over-constrained (residual {:.3e}, DoF {})",
            solve.residual_norm, solve.dof
        ),
        SketchStatus::OverConstrained => {
            let names: Vec<String> = conflicting
                .iter()
                .filter_map(|id| {
                    sketch.constraints.get(id).map(|r| match &r.name {
                        Some(name) => format!("{name} ({id})"),
                        None => id.to_string(),
                    })
                })
                .collect();
            format!("over-constrained — conflict near: {}", names.join(", "))
        }
    }
}
