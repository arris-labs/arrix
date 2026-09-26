//! The drag path (docs/UI-RENDERING.md §Sketch mode): a [`DragSession`]
//! built once when a drag starts, stepped once per frame toward the
//! cursor.
//!
//! The priority is hierarchical. The real constraints are hard and the pin
//! is minimised in what they leave free: each iteration takes the
//! Gauss–Newton step for the pin inside the null space of the primary
//! Jacobian, then retracts onto the constraints with the ordinary solver,
//! and keeps the step only if the pin error fell. A target the geometry
//! cannot reach is where the point stops — no driving dimension gives.
//!
//! A two-stage weighted solve (pin and constraints together, then the
//! constraints alone) was the alternative. It was measured, and is not
//! enough: residuals mix metres and radians, so a weighted first stage
//! lands where the units say, and the second stage only projects that
//! compromise back. On a 50 mm arm held at 30°, that leaves the tip about 45 µm
//! short of the point of its ray nearest the cursor.

use crate::constraint::ConstraintPriority;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

use super::algorithms::{
    SubsystemSolveResult, dot, primary_residual_norm, solve_normal, solve_partitioned,
    solve_subsystem, solve_system,
};
use super::guard::BranchGuard;
use super::jacobian::jacobian;
use super::linalg::row_space_basis;
use super::residuals::residual_norm;
use super::system::{Pin, System};
use super::{SolveResult, SolverOptions};

/// Tangent-step-and-retract rounds one frame may spend on its pin.
const MAX_FOLLOW_ITERATIONS: usize = 32;

/// One drag, from press to release. It owns the [`System`] — the sketch's
/// rows plus the pins' — so a frame moves the pins' targets and solves,
/// building nothing and adding no constraint.
#[derive(Debug, Clone)]
pub struct DragSession {
    system: System,
    grab: Grab,
    options: SolverOptions,
    guard: BranchGuard,
    /// The variables of the last frame that converged, or of the sketch as
    /// the drag found it until one has: what a failed frame puts back.
    last_good: Vec<f64>,
    /// Whether `last_good` satisfies the constraints: false only until the
    /// first converged frame of a drag that started inconsistent.
    last_good_converged: bool,
}

/// What a drag holds, and how the cursor moves it.
#[derive(Debug, Clone)]
enum Grab {
    /// Points carried by the cursor's travel from `from`, each pinned
    /// toward where it stood at the press plus that travel: one point for
    /// a point drag, a line's two ends or an arc's centre and ends for a
    /// body drag.
    Points {
        from: [f64; 2],
        points: Vec<(PointId, [f64; 2])>,
    },
    /// A circle's rim: its radius pinned toward the cursor's distance from
    /// the centre, and nothing else.
    Rim { circle: EntityId, center: PointId },
}

/// What one drag frame did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragFrame {
    /// Whether the geometry the frame left behind satisfies the constraints.
    /// A point stopped short of an unreachable target converged. After a
    /// blocked frame this describes the last good state, which is false
    /// only for a sketch the drag found inconsistent.
    pub converged: bool,
    /// The frame's solve failed and the last good state was put back: the
    /// cursor moved on and the geometry stayed — "can't move further".
    pub blocked: bool,
}

impl DragSession {
    /// A session dragging `point`, pinned where it stands until the first
    /// [`DragSession::step`]. The sketch's constraints must not change
    /// while the session lives.
    pub fn new(sketch: &Sketch, point: PointId) -> Self {
        let (x, y) = sketch.points.get(&point).map_or((0.0, 0.0), |p| (p.x, p.y));
        let grab = Grab::Points {
            from: [x, y],
            points: vec![(point, [x, y])],
        };
        Self::with_grab(sketch, grab)
    }

    /// A session dragging `entity` by its body, grabbed at `grab` in sketch
    /// `(u, v)`. A line's two ends and an arc's centre and ends translate
    /// together with the cursor, so the line keeps its length and the arc
    /// its radius and sweep unless a constraint says otherwise. A circle is
    /// grabbed by its rim, which pulls only its radius; a driving `Radius`
    /// holds it where it is. Any other entity drags its points like a line.
    pub fn new_entity(sketch: &Sketch, entity: EntityId, grab: [f64; 2]) -> Self {
        let grab = match sketch.entities.get(&entity) {
            Some(Entity::Circle { center, .. }) => Grab::Rim {
                circle: entity,
                center: *center,
            },
            Some(Entity::Arc { center, start, end }) => {
                points_grab(sketch, grab, &[*center, *start, *end])
            }
            Some(e) => points_grab(sketch, grab, &e.point_ids()),
            None => points_grab(sketch, grab, &[]),
        };
        Self::with_grab(sketch, grab)
    }

    fn with_grab(sketch: &Sketch, grab: Grab) -> Self {
        let pins: Vec<Pin> = match &grab {
            Grab::Points { points, .. } => points
                .iter()
                .map(|&(point, [x, y])| Pin::Point { point, x, y })
                .collect(),
            Grab::Rim { circle, .. } => {
                let value = match sketch.entities.get(circle) {
                    Some(Entity::Circle { radius, .. }) => *radius,
                    _ => 0.0,
                };
                vec![Pin::Radius {
                    circle: *circle,
                    value,
                }]
            }
        };
        let system = System::build_with_pins(sketch, &pins);
        let options = SolverOptions {
            sketch_scaled_trust: true,
            ..SolverOptions::default()
        };
        let primary =
            primary_residual_norm(&system.residuals_of(sketch), &system.residual_priorities);
        Self {
            guard: BranchGuard::new(&system, sketch),
            last_good: system.pack(sketch),
            last_good_converged: primary < options.residual_tol * 100.0,
            system,
            grab,
            options,
        }
    }

    /// One frame: what the drag holds pulled toward the cursor at `(x, y)`
    /// as far as the constraints let it go. A frame whose solve fails is rolled back to
    /// the last good state and reports `blocked`, so the sketch never holds
    /// a half-solved iterate between frames.
    pub fn step(&mut self, sketch: &mut Sketch, x: f64, y: f64) -> DragFrame {
        let started_good = self.last_good_converged;
        let (res, blocked) = self.frame(sketch, x, y);
        DragFrame {
            converged: if blocked { started_good } else { res.converged },
            blocked,
        }
    }

    /// [`Self::step`] with the solver's own account of the frame, which on
    /// a blocked frame describes the solve that was rolled back.
    pub(crate) fn frame(&mut self, sketch: &mut Sketch, x: f64, y: f64) -> (SolveResult, bool) {
        let guard = self.guard.clone();
        let res = self.solve(sketch, x, y);
        if res.converged {
            self.last_good = self.system.pack(sketch);
            self.last_good_converged = true;
            (res, false)
        } else {
            self.system.unpack_into(&self.last_good, sketch);
            self.guard = guard;
            (res, true)
        }
    }

    fn solve(&mut self, sketch: &mut Sketch, x: f64, y: f64) -> SolveResult {
        crate::instrument::note_solve();
        match &self.grab {
            Grab::Points { from, points } => {
                let (dx, dy) = (x - from[0], y - from[1]);
                for &(point, [px, py]) in points {
                    self.system.set_pin_target(point, px + dx, py + dy);
                }
            }
            Grab::Rim { circle, center } => {
                let c = sketch.points.get(center).map_or((0.0, 0.0), |p| (p.x, p.y));
                self.system
                    .set_radius_target(*circle, (x - c.0).hypot(y - c.1));
            }
        }
        let guard = &mut self.guard;
        solve_partitioned(
            &self.system,
            sketch,
            &self.options,
            |sub, sketch, options| {
                if sub.pins.is_empty() {
                    solve_subsystem(sub, sketch, options)
                } else {
                    follow_pin(sub, sketch, options, guard)
                }
            },
        )
    }
}

/// The points of a body drag, each with where it stands at the press.
fn points_grab(sketch: &Sketch, from: [f64; 2], ids: &[PointId]) -> Grab {
    Grab::Points {
        from,
        points: ids
            .iter()
            .filter_map(|id| sketch.points.get(id).map(|p| (*id, [p.x, p.y])))
            .collect(),
    }
}

/// The norm of `sys`'s drag-priority rows at the sketch's current state.
fn soft_error(sys: &System, sketch: &Sketch) -> f64 {
    let r = sys.residuals_of(sketch);
    let soft: Vec<f64> = r
        .iter()
        .zip(&sys.residual_priorities)
        .filter(|(_, p)| **p != ConstraintPriority::Primary)
        .map(|(v, _)| *v)
        .collect();
    residual_norm(&soft)
}

/// The subsystem holding the pin: first onto the constraints, then along
/// them toward the target, one retracted tangent step at a time. A step
/// that flips an orientation the drag started with is rejected like one
/// that raised the pin error, so the point stops short of the flip.
fn follow_pin(
    sys: &System,
    sketch: &mut Sketch,
    options: &SolverOptions,
    guard: &mut BranchGuard,
) -> SubsystemSolveResult {
    let primary = sys.primary_only(sketch);
    let mut res = solve_system(&primary, sketch, options);
    if !res.converged {
        return res;
    }
    let mut err = soft_error(sys, sketch);
    // The first step goes no further than the target is away.
    let mut radius = err;
    for _ in 0..MAX_FOLLOW_ITERATIONS {
        if err < options.residual_tol {
            break;
        }
        let x = sys.pack(sketch);
        let Some(mut t) = tangent_step(sys, sketch, &x) else {
            break;
        };
        let len = residual_norm(&t);
        if len < options.step_tol {
            break;
        }
        if len > radius {
            t.iter_mut().for_each(|v| *v *= radius / len);
        }
        let trial: Vec<f64> = x.iter().zip(&t).map(|(a, b)| a + b).collect();
        sys.unpack_into(&trial, sketch);
        let retracted = solve_system(&primary, sketch, options);
        let trial_err = soft_error(sys, sketch);
        if retracted.converged && trial_err < err && guard.admit(sketch) {
            err = trial_err;
            res = SubsystemSolveResult {
                iterations: res.iterations + retracted.iterations,
                ..retracted
            };
            // Near a singular pose the geometry moves much further than
            // the pin error shrinks, so the radius grows past it.
            radius = 2.0 * len.min(radius);
        } else {
            sys.unpack_into(&x, sketch);
            radius = 0.5 * len.min(radius);
            if radius < options.step_tol {
                break;
            }
        }
    }
    res
}

/// The smallest step that, to first order, leaves every primary residual
/// where it is and cuts the pin's error the most: the Gauss–Newton step for
/// the drag-priority rows, projected onto the primary Jacobian's null
/// space. `None` if the projected system cannot be solved.
fn tangent_step(sys: &System, sketch: &Sketch, x: &[f64]) -> Option<Vec<f64>> {
    let n = sys.n_vars();
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
    let basis = row_space_basis(&primary);
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
    for (mi, ei) in m.iter().zip(&e) {
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
