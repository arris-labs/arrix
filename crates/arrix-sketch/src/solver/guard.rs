//! The branch guard (docs/UI-RENDERING.md §Sketch mode): the orientations
//! a drag must not flip, recorded when it starts.
//!
//! Several residuals are sign-free — `|θ| − value`, `|d| − r` — so both
//! readings of an orientation satisfy them. A [`super::DragSession`] frame
//! follows the constraints and so never *jumps* between the readings, but it
//! can still walk from one to the other through a degenerate state that
//! satisfies both: an arm dragged through its fixed end, a circle shrunk
//! through its tangent line to the radius floor, an arc end slid round
//! through its start. The guard only decides whether a step is accepted; no
//! residual changes.
//!
//! Guarded, because each was measured flipping along a path: the side of
//! an `Angle`/`AnglePoints`, the side of a `Tangent` circle's centre, and an
//! arc's sweep. Not guarded, because it cannot flip along a path:
//! `TangentCircles` stalls at the radius floor before the tie between its
//! readings, and a driving distance with a value above zero (`Horizontal-`/
//! `VerticalDistance`, `DistancePointLine`, `DistanceCircleCircle`) has no
//! solution between its two readings to walk through.

use std::f64::consts::{PI, TAU};

use arrix_core::LENGTH_TOLERANCE;

use crate::constraint::Constraint;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

use super::eval::{line_pair_dirs, signed_angle};
use super::system::{System, entity_center, point, point_line_signed_distance};

/// A signed quantity whose sign is an orientation the residual ignores.
#[derive(Debug, Clone, Copy)]
enum Side {
    Angle {
        a: EntityId,
        b: EntityId,
    },
    AnglePoints {
        a: PointId,
        vertex: PointId,
        b: PointId,
    },
    Tangent {
        line: EntityId,
        circle: EntityId,
    },
}

impl Side {
    fn of(c: &Constraint) -> Option<Self> {
        match *c {
            Constraint::Angle { a, b, .. } => Some(Self::Angle { a, b }),
            Constraint::AnglePoints { a, vertex, b, .. } => {
                Some(Self::AnglePoints { a, vertex, b })
            }
            Constraint::Tangent { line, circle } => Some(Self::Tangent { line, circle }),
            _ => None,
        }
    }

    /// The signed quantity, zero when degenerate: an angle with an arm
    /// shorter than [`LENGTH_TOLERANCE`] has no direction to hold.
    fn read(self, sketch: &Sketch) -> f64 {
        let angle = |da: (f64, f64), db: (f64, f64)| {
            if da.0.hypot(da.1) < LENGTH_TOLERANCE || db.0.hypot(db.1) < LENGTH_TOLERANCE {
                0.0
            } else {
                signed_angle(da, db)
            }
        };
        match self {
            Self::Angle { a, b } => {
                line_pair_dirs(sketch, a, b).map_or(0.0, |(_, _, da, db)| angle(da, db))
            }
            Self::AnglePoints { a, vertex, b } => {
                let (pa, pv, pb) = (point(sketch, a), point(sketch, vertex), point(sketch, b));
                angle((pa.x - pv.x, pa.y - pv.y), (pb.x - pv.x, pb.y - pv.y))
            }
            Self::Tangent { line, circle } => entity_center(sketch, circle)
                .and_then(|c| point_line_signed_distance(sketch, c, line))
                .unwrap_or(0.0),
        }
    }
}

/// `-1`, `0` or `1` — unlike `f64::signum`, zero has no side.
fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// The counter-clockwise sweep from an arc's start to its end, in `[0, 2π)`.
fn sweep(sketch: &Sketch, [c, s, e]: [PointId; 3]) -> f64 {
    let (c, s, e) = (point(sketch, c), point(sketch, s), point(sketch, e));
    ((e.y - c.y).atan2(e.x - c.x) - (s.y - c.y).atan2(s.x - c.x)).rem_euclid(TAU)
}

/// Whether an arc's end sits on its start — the degenerate state its sweep
/// wraps through.
fn closed(sketch: &Sketch, [_, s, e]: [PointId; 3]) -> bool {
    point(sketch, s).distance_to(point(sketch, e)) < LENGTH_TOLERANCE
}

/// The orientations of one drag, recorded at its start.
#[derive(Debug, Clone, Default)]
pub(crate) struct BranchGuard {
    /// Each guarded side with its sign at the start. A side that starts
    /// degenerate (sign zero) has no orientation to keep and is left out.
    sides: Vec<(Side, f64)>,
    /// Each arc's points and its sweep, unwrapped: followed step by step,
    /// so crossing the start shows as leaving `(0, 2π)` rather than as a
    /// wrap.
    sweeps: Vec<([PointId; 3], f64)>,
}

impl BranchGuard {
    pub(crate) fn new(sys: &System, sketch: &Sketch) -> Self {
        let sides = sys
            .constraint_ids
            .iter()
            .filter_map(|id| Side::of(&sketch.constraints.get(id)?.constraint))
            .map(|side| (side, sign(side.read(sketch))))
            .filter(|(_, s)| *s != 0.0)
            .collect();
        let sweeps = sys
            .arc_entity_ids
            .iter()
            .filter_map(|id| match sketch.entities.get(id)? {
                crate::entity::Entity::Arc { center, start, end } => Some([*center, *start, *end]),
                _ => None,
            })
            .map(|arc| {
                let s = sweep(sketch, arc);
                (arc, if s == 0.0 { TAU } else { s })
            })
            .collect();
        Self { sides, sweeps }
    }

    /// Whether the sketch still has every orientation the drag started
    /// with. The degenerate state between the readings counts as a flip:
    /// an arm stepped exactly onto its fixed end has no angle left for the
    /// constraints to hold, and no later frame could pull it out. On `true` the arcs' sweeps are followed to it; on `false` the
    /// guard is left as it was, for the caller to put the last state back.
    /// A step must turn no arc by half a circle or more, or its sweep
    /// cannot be followed — the retraction's trust region keeps them short.
    pub(crate) fn admit(&mut self, sketch: &Sketch) -> bool {
        if self
            .sides
            .iter()
            .any(|(side, s)| sign(side.read(sketch)) != *s)
        {
            return false;
        }
        let followed: Vec<f64> = self
            .sweeps
            .iter()
            .map(|(arc, last)| {
                let turn = (sweep(sketch, *arc) - last + PI).rem_euclid(TAU) - PI;
                last + turn
            })
            .collect();
        let wrapped = |((arc, _), s): (&([PointId; 3], f64), &f64)| {
            *s <= 0.0 || *s >= TAU || closed(sketch, *arc)
        };
        if self.sweeps.iter().zip(&followed).any(wrapped) {
            return false;
        }
        for ((_, last), s) in self.sweeps.iter_mut().zip(followed) {
            *last = s;
        }
        true
    }
}
