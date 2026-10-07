//! Constraint inference on draw (docs/UI-RENDERING.md §Sketch mode): the
//! snap search the sketch overlay previews and the draw click commits
//! ([`search`]), and the [`Inference`]s it binds once the click has placed
//! its points. Pure geometry in plane-local metres: the UI supplies the
//! tolerance in pixels turned metres, and the dwell that acquires
//! alignment sources from its own clock.
//!
//! **The commit rule.** A click's inferences are bound after the geometry
//! they land on, and after the structure a template gives it, is in the
//! sketch. They are bound one at a time, in priority order, each checked
//! with [`Sketch::check_candidate`] against the sketch with the earlier
//! ones already in: `Ok` is added, `Redundant` and `Conflicting` are
//! dropped. So drawing never over-constrains, and priority decides *which*
//! of two equivalent candidates is kept, never how many.
//! `tests/inference_probe.rs` establishes it: a template on the axis
//! offers more candidates than it needs and keeps exactly one; an
//! alignment in the coordinate a line's H lock already set is `Redundant`,
//! one in the other coordinate is `Ok`; `PointOnLine` and `PointOnCircle`
//! hold on a line's extension and an arc's full circle; `Tangent { line,
//! circle }` with the line's end on the rim and no shared point converges
//! and does not flip under a drag, but at the touching point exactly the
//! two are one first-order row, so a tangent snap commits `Tangent` alone;
//! `Parallel` and `Perpendicular` on the drawn line follow a turning
//! reference without flipping. One verdict on a fully constrained plate
//! costs well under a frame (docs/CONCURRENCY-WASM.md §Budgets).

use crate::candidate::CandidateVerdict;
use crate::constraint::{Constraint, DatumEntity};
use crate::draft::Draft;
use crate::entity::Entity;
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

mod curves;
mod relations;
mod search;

pub use search::{GlyphKind, Inferred, Snap, SnapGlyph, SnapGuide, SnapQuery, snap_search};

/// A snap/inference suggestion the draw tool can apply.
#[derive(Debug, Clone, PartialEq)]
pub enum Inference {
    /// Snap the free point onto an existing point and add coincident.
    Coincident {
        moving: PointId,
        target: PointId,
        at: [f64; 2],
    },
    /// Snap the free point onto the sketch origin and pin it there with
    /// `CoincidentToDatum` — the part's reference corner or centre, which
    /// otherwise costs a pick, a Shift-pick and a button after the draw.
    Origin { moving: PointId },
    /// Snap the free point to the midpoint of an existing line.
    Midpoint {
        moving: PointId,
        line: EntityId,
        at: [f64; 2],
    },
    /// Snap the free point onto an existing line entity.
    PointOnLine {
        moving: PointId,
        line: EntityId,
        at: [f64; 2],
    },
    /// Snap the free point onto an existing circle/arc entity.
    PointOnCircle {
        moving: PointId,
        circle: EntityId,
        at: [f64; 2],
    },
    /// The line is close to horizontal — suggest H constraint + snap Y.
    Horizontal {
        line: EntityId,
        /// Adjusted endpoint positions `(start, end)`.
        start: [f64; 2],
        end: [f64; 2],
    },
    /// The line is close to vertical.
    Vertical {
        line: EntityId,
        start: [f64; 2],
        end: [f64; 2],
    },
    /// Level with `source`, as an alignment guide showed:
    /// `HorizontalPoints`.
    AlignedH { moving: PointId, source: PointId },
    /// Plumb with `source`: `VerticalPoints`.
    AlignedV { moving: PointId, source: PointId },
    /// On a sketch axis — `AxisX` or `AxisY`: `PointOnDatum`.
    OnAxis { moving: PointId, datum: DatumEntity },
    /// The drawn `line` parallel to `to`, as its direction snapped:
    /// `Parallel`.
    Parallel { line: EntityId, to: EntityId },
    /// Perpendicular to `to`: `Perpendicular`.
    Perpendicular { line: EntityId, to: EntityId },
    /// The drawn `line` tangent to `circle` (a circle or arc), its end
    /// placed at the touching point but not held there: `Tangent`.
    Tangent { line: EntityId, circle: EntityId },
}

impl Inference {
    /// The constraint this commits.
    pub fn constraint(&self) -> Constraint {
        match *self {
            Self::Coincident { moving, target, .. } => Constraint::Coincident {
                a: moving,
                b: target,
            },
            Self::Origin { moving } => Constraint::coincident_to_datum(moving, DatumEntity::Origin),
            Self::Midpoint { moving, line, .. } => Constraint::Midpoint {
                point: moving,
                line,
            },
            Self::PointOnLine { moving, line, .. } => Constraint::PointOnLine {
                point: moving,
                line,
            },
            Self::PointOnCircle { moving, circle, .. } => Constraint::PointOnCircle {
                point: moving,
                circle,
            },
            Self::Horizontal { line, .. } => Constraint::Horizontal { line },
            Self::Vertical { line, .. } => Constraint::Vertical { line },
            Self::AlignedH { moving, source } => Constraint::HorizontalPoints {
                a: source,
                b: moving,
            },
            Self::AlignedV { moving, source } => Constraint::VerticalPoints {
                a: source,
                b: moving,
            },
            Self::OnAxis { moving, datum } => Constraint::PointOnDatum {
                point: moving,
                datum,
            },
            Self::Parallel { line, to } => Constraint::Parallel { a: line, b: to },
            Self::Perpendicular { line, to } => Constraint::Perpendicular { a: line, b: to },
            Self::Tangent { line, circle } => Constraint::Tangent { line, circle },
        }
    }

    /// Moves the geometry to where the constraint holds, without adding it.
    fn place(&self, sketch: &mut Sketch) {
        let x_of = |sketch: &Sketch, id: PointId| sketch.points.get(&id).map(|p| p.x);
        let y_of = |sketch: &Sketch, id: PointId| sketch.points.get(&id).map(|p| p.y);
        match *self {
            Self::Coincident { moving, at, .. }
            | Self::Midpoint { moving, at, .. }
            | Self::PointOnLine { moving, at, .. }
            | Self::PointOnCircle { moving, at, .. } => {
                set_pos(sketch, moving, Some(at[0]), Some(at[1]));
            }
            Self::Origin { moving } => set_pos(sketch, moving, Some(0.0), Some(0.0)),
            Self::Horizontal { line, start, end } | Self::Vertical { line, start, end } => {
                if let Some(&Entity::Line { start: s, end: e }) = sketch.entities.get(&line) {
                    set_pos(sketch, s, Some(start[0]), Some(start[1]));
                    set_pos(sketch, e, Some(end[0]), Some(end[1]));
                }
            }
            Self::AlignedH { moving, source } => {
                let y = y_of(sketch, source);
                set_pos(sketch, moving, None, y);
            }
            Self::AlignedV { moving, source } => {
                let x = x_of(sketch, source);
                set_pos(sketch, moving, x, None);
            }
            Self::OnAxis { moving, datum } => match datum {
                DatumEntity::AxisX => set_pos(sketch, moving, None, Some(0.0)),
                DatumEntity::AxisY => set_pos(sketch, moving, Some(0.0), None),
                DatumEntity::Origin => set_pos(sketch, moving, Some(0.0), Some(0.0)),
            },
            // The snap put the end on the relation's ray or tangent point;
            // the solve after the click settles what the pixel left.
            Self::Parallel { .. } | Self::Perpendicular { .. } | Self::Tangent { .. } => {}
        }
    }
}

/// Moves `id` to `(x, y)`, keeping a coordinate given as `None`.
fn set_pos(sketch: &mut Sketch, id: PointId, x: Option<f64>, y: Option<f64>) {
    if let Some(p) = sketch.points.get_mut(&id) {
        p.set_pos(x.unwrap_or(p.x), y.unwrap_or(p.y));
    }
}

/// The one tolerance that is not a distance: how far off level or plumb
/// (about 2°) a pending segment still takes the H/V lock or a direction
/// relation. Distances are the snap radius, in pixels turned metres by the
/// caller, so what a click commits is what the preview showed at that zoom.
pub const ANGLE_TOL: f64 = 2.0 * std::f64::consts::PI / 180.0;

/// The commit rule (see the module doc): each inference in order, checked
/// with [`Sketch::check_candidate`] against the sketch with the earlier kept
/// ones already in; `Ok` is applied, `Redundant` and `Conflicting` are
/// dropped. Returns every verdict, in order.
pub fn commit(sketch: &mut Draft, inferences: &[Inference]) -> Vec<CandidateVerdict> {
    inferences
        .iter()
        .map(|inference| {
            let verdict = sketch.check_candidate(&inference.constraint());
            if verdict.is_ok() {
                apply(sketch, inference);
            }
            verdict
        })
        .collect()
}

/// Applies an inference unchecked: moves the geometry to where it holds and
/// adds its constraint.
pub fn apply(sketch: &mut Draft, inference: &Inference) -> ConstraintId {
    inference.place(sketch);
    sketch.add_constraint(inference.constraint())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(sketch: &Sketch, cursor: (f64, f64)) -> Vec<Inferred> {
        snap_search(
            sketch,
            &SnapQuery {
                cursor,
                tol: 5e-4,
                direction_from: None,
                excluded: None,
                acquired_points: &[],
                acquired_lines: &[],
                relations: false,
            },
        )
        .inferences
    }

    #[test]
    fn a_point_then_a_midpoint_is_found() {
        let mut sketch = Draft::seeded(1);
        let (p1, _p2, line) = sketch.add_line(0.0, 0.0, 0.10, 0.0);
        assert_eq!(
            search(&sketch, (0.0002, 0.0001)),
            vec![Inferred::OnPoint(p1)]
        );
        assert_eq!(
            search(&sketch, (0.0501, 0.0002)),
            vec![Inferred::Midpoint(line)]
        );
    }

    /// Near the origin with nothing there, a point is pinned to the origin
    /// datum; once a point sits there, the next one snaps onto that point
    /// instead, so the origin is never pinned twice.
    #[test]
    fn a_point_near_the_origin_is_pinned_to_it_once() {
        let mut sketch = Draft::seeded(1);
        let cursor = (0.0003, -0.0002);
        assert_eq!(search(&sketch, cursor), vec![Inferred::Origin]);
        let first = sketch.add_point(crate::entity::Point::new(cursor.0, cursor.1));
        let inference = Inferred::Origin.bind(&sketch, first, None).unwrap();
        apply(&mut sketch, &inference);
        let p = &sketch.points[&first];
        assert_eq!((p.x, p.y), (0.0, 0.0));

        assert_eq!(
            search(&sketch, (0.0001, 0.0001)),
            vec![Inferred::OnPoint(first)]
        );
    }
}
