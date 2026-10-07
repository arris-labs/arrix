//! The curve passes of the snap search, after point, origin and midpoint
//! and before alignment: an intersection of two curves, a
//! tangency (`relations`), a circle's quadrant, a point on a curve, and a
//! point on an acquired line's extension. Each places the cursor outright
//! and commits existing kinds: `PointOnLine`/`PointOnCircle`, and for a quadrant `HorizontalPoints` or
//! `VerticalPoints` with the centre. Construction curves count: they are
//! what a sketch is laid out on.

use crate::arrangement::curve::{Curve, dist, dot, sub};
use crate::arrangement::intersect;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

use super::search::{GlyphKind, Inferred, Snap, SnapQuery};

/// A curve the search may snap to: its entity, geometry, and centre point
/// for a circle or arc.
pub(super) struct Candidate {
    pub(super) id: EntityId,
    pub(super) curve: Curve,
    centre: Option<PointId>,
}

/// Every line, circle and arc none of whose points is being dragged.
fn candidates(sketch: &Sketch, query: &SnapQuery) -> Vec<Candidate> {
    sketch
        .entities()
        .iter()
        .filter(|(_, entity)| {
            let points: Vec<PointId> = match **entity {
                Entity::Line { start, end } => vec![start, end],
                Entity::Circle { center, .. } => vec![center],
                Entity::Arc { center, start, end } => vec![center, start, end],
                _ => Vec::new(),
            };
            !points.iter().any(|p| Some(*p) == query.excluded)
        })
        .filter_map(|(id, entity)| {
            Some(Candidate {
                id: *id,
                curve: Curve::from_entity(sketch, *id)?,
                centre: entity.circle_center(),
            })
        })
        .collect()
}

/// The nearest of `hits` within the snap radius of the cursor.
pub(super) fn nearest<T>(
    query: &SnapQuery,
    hits: impl Iterator<Item = ([f64; 2], T)>,
) -> Option<([f64; 2], T)> {
    let cursor = [query.cursor.0, query.cursor.1];
    hits.map(|(p, t)| (dist(p, cursor), p, t))
        .filter(|(d, _, _)| *d <= query.tol)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p, t)| (p, t))
}

/// The foot of the cursor on the curve's underlying line or circle, and
/// whether it falls inside the curve's own extent.
fn foot(curve: &Curve, cursor: [f64; 2]) -> Option<([f64; 2], bool)> {
    let p = match curve.line() {
        Some((a, b)) => {
            let d = sub(b, a);
            let t = dot(sub(cursor, a), d) / dot(d, d);
            [a[0] + d[0] * t, a[1] + d[1] * t]
        }
        None => {
            let (c, r) = curve.circle()?;
            let away = sub(cursor, c);
            let n = away[0].hypot(away[1]);
            if n <= 0.0 {
                return None;
            }
            [c[0] + away[0] * r / n, c[1] + away[1] * r / n]
        }
    };
    Some((p, curve.contains(p)))
}

/// Runs the curve passes; true when one placed the cursor.
pub(super) fn snap_to_curves(sketch: &Sketch, query: &SnapQuery, out: &mut Snap) -> bool {
    let curves = candidates(sketch, query);
    let cursor = [query.cursor.0, query.cursor.1];

    // Intersections of two curves: on both.
    let pairs = curves.iter().enumerate().flat_map(|(i, a)| {
        curves[i + 1..].iter().flat_map(move |b| {
            intersect(&a.curve, &b.curve)
                .into_iter()
                .map(move |p| (p, (a.id, b.id)))
        })
    });
    if let Some((p, (a, b))) = nearest(query, pairs) {
        out.place(p);
        out.infer_all(
            GlyphKind::Intersection,
            out.uv,
            &[Inferred::OnCurve(a), Inferred::OnCurve(b)],
        );
        return true;
    }

    // The pending segment tangent to a circle or arc it ends on.
    if super::relations::tangent(query, &curves, out) {
        return true;
    }

    // A circle's quadrants: on it, and level or plumb with its centre.
    let quadrants = curves.iter().flat_map(|c| {
        let quads = match (c.curve.circle(), c.centre) {
            (Some((centre, r)), Some(pid)) => [
                ([centre[0] + r, centre[1]], true),
                ([centre[0] - r, centre[1]], true),
                ([centre[0], centre[1] + r], false),
                ([centre[0], centre[1] - r], false),
            ]
            .into_iter()
            .filter(|(p, _)| c.curve.contains(*p))
            .map(|(p, level)| (p, (c.id, pid, level)))
            .collect(),
            _ => Vec::new(),
        };
        quads.into_iter()
    });
    if let Some((p, (circle, centre, level))) = nearest(query, quadrants) {
        out.place(p);
        let aligned = if level {
            Inferred::AlignedH(centre)
        } else {
            Inferred::AlignedV(centre)
        };
        out.infer_all(
            GlyphKind::Quadrant,
            out.uv,
            &[Inferred::OnCurve(circle), aligned],
        );
        return true;
    }

    // On a curve. Where the pending segment's H/V lock applies and its ray
    // meets the curve within the radius, the end goes there and is both.
    let on = curves.iter().filter_map(|c| {
        let (p, inside) = foot(&c.curve, cursor)?;
        inside.then_some((p, c))
    });
    if let Some((p, c)) = nearest(query, on) {
        if let Some((q, orientation)) = locked_on(query, &c.curve) {
            out.place(q);
            let kind = if orientation == Inferred::Horizontal {
                GlyphKind::Horizontal
            } else {
                GlyphKind::Vertical
            };
            out.infer_all(GlyphKind::OnCurve, out.uv, &[Inferred::OnCurve(c.id)]);
            out.infer_all(kind, out.uv, &[orientation]);
        } else {
            out.place(p);
            out.infer_all(GlyphKind::OnCurve, out.uv, &[Inferred::OnCurve(c.id)]);
        }
        return true;
    }

    // On an acquired line's extension, beyond its ends: the guide runs from
    // the nearer end.
    let extensions = curves
        .iter()
        .filter(|c| query.acquired_lines.contains(&c.id))
        .filter_map(|c| {
            let (a, b) = c.curve.line()?;
            let (p, inside) = foot(&c.curve, cursor)?;
            let from = if dist(p, a) < dist(p, b) { a } else { b };
            (!inside).then_some((p, (c.id, from)))
        });
    if let Some((p, (line, from))) = nearest(query, extensions) {
        out.place(p);
        let index = out.infer_all(GlyphKind::OnCurve, out.uv, &[Inferred::OnCurve(line)]);
        out.guide([(from[0], from[1]), out.uv], index[0]);
        return true;
    }
    false
}

/// Where the pending segment's H/V lock ray meets `curve` within the snap
/// radius of the cursor, and which lock it is.
fn locked_on(query: &SnapQuery, curve: &Curve) -> Option<([f64; 2], Inferred)> {
    let start = query.direction_from?;
    let orientation = super::search::lock_at(start, query.cursor, query.tol)?;
    let reach = dist([start.0, start.1], [query.cursor.0, query.cursor.1]) * 2.0 + query.tol;
    let ray = if orientation == Inferred::Horizontal {
        Curve::Line {
            a: [start.0 - reach, start.1],
            b: [start.0 + reach, start.1],
        }
    } else {
        Curve::Line {
            a: [start.0, start.1 - reach],
            b: [start.0, start.1 + reach],
        }
    };
    let hits = intersect(curve, &ray).into_iter().map(|p| (p, ()));
    nearest(query, hits).map(|(p, ())| (p, orientation))
}
