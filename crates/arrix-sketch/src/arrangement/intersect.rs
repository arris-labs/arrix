//! Closed-form intersection of two sketch curves, at `LENGTH_TOLERANCE`.
//!
//! Lines, circles and circular arcs only: conics and B-splines are never
//! intersected or split.
//!
//! Every hit the geometry has is reported, including one that lands on a
//! curve's own endpoint — a T-junction is exactly that, and it is the
//! splitter, not this module, that decides
//! whether a hit cuts anything. Two degeneracies answer with the endpoints
//! that lie inside the other curve rather than with a crossing point:
//! collinear overlapping lines, and two arcs of the same circle. Those are
//! the only places such a pair can be cut. A tangency is one hit, not two.

use arrix_core::LENGTH_TOLERANCE;

use super::curve::{Curve, along, cross, dist, dot, norm, sub, unit};
use crate::ids::EntityId;
use crate::sketch::Sketch;

/// Two lines whose `|sin θ|` is below this are parallel: they either overlap
/// collinearly or never meet, and the crossing formula would answer with
/// noise. A micrometre over a metre is 1e-6 rad, so this is three decades
/// below anything a sketch means to be a crossing.
const PARALLEL_SIN: f64 = 1.0e-9;

/// Every intersection of the two entities, in sketch `(u, v)` meters,
/// deduplicated at `LENGTH_TOLERANCE`. Empty when either entity is not a
/// line, circle or arc, when they never meet, or when they are the same
/// entity.
pub fn entity_intersections(sketch: &Sketch, a: EntityId, b: EntityId) -> Vec<[f64; 2]> {
    if a == b {
        return Vec::new();
    }
    match (Curve::from_entity(sketch, a), Curve::from_entity(sketch, b)) {
        (Some(ca), Some(cb)) => intersect(&ca, &cb),
        _ => Vec::new(),
    }
}

/// Every intersection of two curves, deduplicated at `LENGTH_TOLERANCE`.
pub(crate) fn intersect(a: &Curve, b: &Curve) -> Vec<[f64; 2]> {
    // Each helper answers for the *unbounded* line or full circle, plus, for
    // the two overlapping cases, the candidate endpoints; the extent filter
    // below is what cuts those down to this pair's own hits.
    let candidates = match (a.circle(), b.circle()) {
        (None, None) => line_line(a.line().unwrap(), b.line().unwrap()),
        (None, Some(cb)) => line_circle(a.line().unwrap(), cb),
        (Some(ca), None) => line_circle(b.line().unwrap(), ca),
        (Some(ca), Some(cb)) => circle_circle(a, b, ca, cb),
    };

    let mut hits: Vec<[f64; 2]> = Vec::new();
    for p in candidates {
        if a.contains(p) && b.contains(p) && !hits.iter().any(|&q| dist(q, p) <= LENGTH_TOLERANCE) {
            hits.push(p);
        }
    }
    hits
}

/// Where the *unbounded* carriers of two curves cross: the whole line, the
/// whole circle. [`intersect`] is the same question cut down to each curve's
/// extent; an offset needs the carriers, since a corner it moves to can lie
/// past the end of the curve it is a corner of. A circle or arc is read as
/// the full circle (so a concentric pair never meets), and a pair of parallel
/// lines never does.
pub(crate) fn carrier_crossings(a: &Curve, b: &Curve) -> Vec<[f64; 2]> {
    let full = |c: &Curve| {
        c.circle()
            .map(|(center, radius)| Curve::Circle { center, radius })
    };
    let candidates = match (full(a), full(b)) {
        (None, None) => line_line(a.line().unwrap(), b.line().unwrap()),
        (None, Some(cb)) => line_circle(a.line().unwrap(), cb.circle().unwrap()),
        (Some(ca), None) => line_circle(b.line().unwrap(), ca.circle().unwrap()),
        (Some(ca), Some(cb)) => circle_circle(&ca, &cb, ca.circle().unwrap(), cb.circle().unwrap()),
    };
    let mut hits: Vec<[f64; 2]> = Vec::new();
    for p in candidates {
        if !hits.iter().any(|&q| dist(q, p) <= LENGTH_TOLERANCE) {
            hits.push(p);
        }
    }
    hits
}

fn line_line(l1: ([f64; 2], [f64; 2]), l2: ([f64; 2], [f64; 2])) -> Vec<[f64; 2]> {
    let ((a1, b1), (a2, b2)) = (l1, l2);
    let (d1, d2) = (sub(b1, a1), sub(b2, a2));
    let denom = cross(d1, d2);
    if denom.abs() > PARALLEL_SIN * norm(d1) * norm(d2) {
        return vec![along(a1, d1, cross(sub(a2, a1), d2) / denom)];
    }
    // Parallel. Apart by more than the tolerance they never meet; collinear
    // they overlap along a stretch whose ends are where one segment's
    // endpoint falls inside the other.
    if cross(d1, sub(a2, a1)).abs() > LENGTH_TOLERANCE * norm(d1) {
        return Vec::new();
    }
    vec![a1, b1, a2, b2]
}

fn line_circle(l: ([f64; 2], [f64; 2]), (c, r): ([f64; 2], f64)) -> Vec<[f64; 2]> {
    let (a, b) = l;
    let u = unit(sub(b, a));
    let w = sub(c, a);
    // Signed distance from the centre to the line, and the foot's parameter.
    let h = cross(u, w);
    let foot = along(a, u, dot(u, w));
    let gap = h.abs() - r;
    if gap > LENGTH_TOLERANCE {
        Vec::new()
    } else if gap.abs() <= LENGTH_TOLERANCE {
        // Tangent: one root, at the foot. The quadratic's discriminant is
        // zero here to within rounding, so asking it for two roots would
        // answer with two copies of this point a few nanometres apart.
        vec![foot]
    } else {
        let k = (r * r - h * h).max(0.0).sqrt();
        vec![along(foot, u, -k), along(foot, u, k)]
    }
}

fn circle_circle(
    a: &Curve,
    b: &Curve,
    (c1, r1): ([f64; 2], f64),
    (c2, r2): ([f64; 2], f64),
) -> Vec<[f64; 2]> {
    let d = dist(c1, c2);
    if d <= LENGTH_TOLERANCE {
        // Concentric: the same circle when the radii agree, and then the only
        // places the pair can be cut are the two arcs' own ends. Different
        // radii never meet, and two full circles have no ends.
        if (r1 - r2).abs() <= LENGTH_TOLERANCE {
            return a.endpoints().into_iter().chain(b.endpoints()).collect();
        }
        return Vec::new();
    }
    let (sum, diff) = (r1 + r2, (r1 - r2).abs());
    if d > sum + LENGTH_TOLERANCE || d < diff - LENGTH_TOLERANCE {
        return Vec::new();
    }
    let u = unit(sub(c2, c1));
    let foot = along(c1, u, (d * d + r1 * r1 - r2 * r2) / (2.0 * d));
    if (d - sum).abs() <= LENGTH_TOLERANCE || (d - diff).abs() <= LENGTH_TOLERANCE {
        // Tangent, externally or internally: one root, on the centre line.
        return vec![foot];
    }
    let k = (r1 * r1 - dist(c1, foot).powi(2)).max(0.0).sqrt();
    let n = [-u[1], u[0]];
    vec![along(foot, n, -k), along(foot, n, k)]
}
