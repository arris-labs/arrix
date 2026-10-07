//! The relation passes: what the pending segment is to another
//! curve. Its direction parallel or perpendicular to an acquired line, or
//! perpendicular off a line whose end it starts on, commits `Parallel` or
//! `Perpendicular` on the drawn line; its end where the segment touches a
//! circle or arc commits `Tangent`.
//!
//! **Tangent alone.** The end is placed at the touching point but not held
//! on the rim: `PointOnCircle` there is the same first-order row as
//! `Tangent` (`tests/inference_probe.rs` (f)), so the filter would drop
//! one and the DoF count would be off by one.
//!
//! **Priority.** The H/V lock wins: a direction relation is searched only
//! where no lock applies, so it never bends a level line, and a relation
//! to a level or plumb line *is* that lock, committed as the line's own
//! orientation. A direction relation refines an alignment guide or an axis
//! only where its ray crosses them within the snap radius. Tangency places
//! the end outright, after an intersection and before a quadrant. Each
//! relation is run through `tests/inference_probe.rs` for convergence and
//! flips.

use crate::arrangement::curve::{dist, dot, sub};
use crate::constraint::DatumEntity;
use crate::entity::Entity;
use crate::sketch::Sketch;
use arrix_core::LENGTH_TOLERANCE;

use super::ANGLE_TOL;
use super::curves::{Candidate, nearest};
use super::search::{GlyphKind, Inferred, Snap, SnapQuery};

/// The ray a direction relation put the cursor on: from the pending start,
/// along a unit direction.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ray {
    from: [f64; 2],
    dir: [f64; 2],
}

/// Tangency: the end placed where the segment from the pending start
/// touches a circle or arc, committing `Tangent` alone (see the module
/// doc). Only with the start outside the circle by more than the snap
/// radius. True when it placed the cursor.
pub(super) fn tangent(query: &SnapQuery, curves: &[Candidate], out: &mut Snap) -> bool {
    let Some(start) = query.direction_from.filter(|_| query.relations) else {
        return false;
    };
    let s = [start.0, start.1];
    let touches = curves.iter().flat_map(|c| {
        let points: Vec<_> = c
            .curve
            .circle()
            .and_then(|(centre, r)| {
                let away = sub(s, centre);
                let d = away[0].hypot(away[1]);
                (d > r + query.tol).then(|| {
                    let base = away[1].atan2(away[0]);
                    let half = (r / d).acos();
                    [base + half, base - half]
                        .map(|t| [centre[0] + r * t.cos(), centre[1] + r * t.sin()])
                })
            })
            .into_iter()
            .flatten()
            .filter(|p| c.curve.contains(*p))
            .map(|p| (p, c.id))
            .collect();
        points
    });
    let Some((p, circle)) = nearest(query, touches) else {
        return false;
    };
    out.place(p);
    out.infer_all(GlyphKind::Tangent, out.uv, &[Inferred::Tangent(circle)]);
    true
}

/// A direction relation for the pending segment ending at `out.uv`, where
/// no H/V lock applies: the closest within the draw-time angle tolerance
/// wins. It moves the end onto the relation's ray — along the coordinate a
/// guide left free, or not at all if the ray misses the cursor there.
pub(super) fn direction(sketch: &Sketch, query: &SnapQuery, out: &mut Snap) -> Option<Ray> {
    let start = query.direction_from.filter(|_| query.relations)?;
    let from = [start.0, start.1];
    let band = sub([out.uv.0, out.uv.1], from);
    let len = band[0].hypot(band[1]);
    if len <= query.tol {
        return None;
    }
    let sin_tol = ANGLE_TOL.sin();
    let (_, inferred, dir) = references(sketch, query, from)
        .into_iter()
        .filter_map(|(inferred, d)| {
            // The perpendicular's direction, or the line's own, signed to
            // point where the band does.
            let d = match inferred {
                Inferred::Perpendicular(_) => [-d[1], d[0]],
                _ => d,
            };
            let along = dot(band, d);
            let d = if along < 0.0 { [-d[0], -d[1]] } else { d };
            let sin = (band[0] * d[1] - band[1] * d[0]).abs() / len;
            (sin < sin_tol).then_some((sin, inferred, d))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))?;
    let ray = Ray { from, dir };
    let cursor = [query.cursor.0, query.cursor.1];
    let p = match out.locked {
        (false, false) => {
            let t = dot(band, dir);
            [from[0] + dir[0] * t, from[1] + dir[1] * t]
        }
        (true, false) => ray.at_u(out.uv.0)?,
        (false, true) => ray.at_v(out.uv.1)?,
        (true, true) => return None,
    };
    if out.locked != (false, false) && dist(p, cursor) > query.tol {
        return None;
    }
    out.uv = (p[0], p[1]);
    out.locked = (true, true);
    let kind = match inferred {
        Inferred::Parallel(_) => GlyphKind::Parallel,
        _ => GlyphKind::Perpendicular,
    };
    out.infer_all(kind, out.uv, &[inferred]);
    Some(ray)
}

/// What the segment from `from` may relate to, each with its line's unit
/// direction: parallel and perpendicular to every acquired line, and
/// perpendicular off a line with an end at `from`. A line a dragged point
/// ends is none.
fn references(sketch: &Sketch, query: &SnapQuery, from: [f64; 2]) -> Vec<(Inferred, [f64; 2])> {
    let mut out = Vec::new();
    for (id, entity) in sketch.entities() {
        let Entity::Line { start, end } = *entity else {
            continue;
        };
        if query.excluded.is_some_and(|p| p == start || p == end) {
            continue;
        }
        let (Some(a), Some(b)) = (sketch.point(start), sketch.point(end)) else {
            continue;
        };
        let (a, b) = ([a.x, a.y], [b.x, b.y]);
        let d = sub(b, a);
        let n = d[0].hypot(d[1]);
        if n < LENGTH_TOLERANCE {
            continue;
        }
        let d = [d[0] / n, d[1] / n];
        let acquired = query.acquired_lines.contains(id);
        let starts_on = dist(a, from) <= LENGTH_TOLERANCE || dist(b, from) <= LENGTH_TOLERANCE;
        if acquired {
            out.push((Inferred::Parallel(*id), d));
        }
        if acquired || starts_on {
            out.push((Inferred::Perpendicular(*id), d));
        }
    }
    out
}

impl Ray {
    /// Where the ray reaches `u`, ahead of its start.
    fn at_u(&self, u: f64) -> Option<[f64; 2]> {
        let t = (u - self.from[0]) / self.dir[0];
        (self.dir[0].abs() > 1e-9 && t > 0.0).then(|| [u, self.from[1] + self.dir[1] * t])
    }

    /// Where the ray reaches `v`, ahead of its start.
    fn at_v(&self, v: f64) -> Option<[f64; 2]> {
        let t = (v - self.from[1]) / self.dir[1];
        (self.dir[1].abs() > 1e-9 && t > 0.0).then(|| [self.from[0] + self.dir[0] * t, v])
    }

    /// The sketch axes on a relation's ray: the end moves along the ray onto
    /// an axis it crosses within the snap radius, and is on it too.
    pub(super) fn onto_axes(&self, query: &SnapQuery, out: &mut Snap) {
        let near = |p: [f64; 2]| dist(p, [out.uv.0, out.uv.1]) <= query.tol;
        if let Some(p) = self.at_v(0.0).filter(|p| near(*p)) {
            out.uv = (p[0], 0.0);
            out.infer_all(
                GlyphKind::AxisU,
                out.uv,
                &[Inferred::OnAxis(DatumEntity::AxisX)],
            );
        } else if let Some(p) = self.at_u(0.0).filter(|p| near(*p)) {
            out.uv = (0.0, p[1]);
            out.infer_all(
                GlyphKind::AxisV,
                out.uv,
                &[Inferred::OnAxis(DatumEntity::AxisY)],
            );
        }
    }
}
