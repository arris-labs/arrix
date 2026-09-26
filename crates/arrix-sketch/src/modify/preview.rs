//! What a trim, an extension or a break would touch, read without writing:
//! the curve a pick lands on, and the stretch of it the edit removes, adds
//! or splits off, as a polyline the app projects for the hover preview
//! (docs/UI-RENDERING.md §Sketch mode).
//!
//! Each preview comes from the same pieces and the same first hit the edit
//! itself uses, so what the hover shows is what the click does.

use arrix_core::LENGTH_TOLERANCE;

use super::extend::{continuation_hit, open_curve};
use crate::arrangement::curve::{Curve, dist};
use crate::arrangement::{Piece, split_all};
use crate::ids::EntityId;
use crate::sketch::Sketch;

/// Samples per full turn of a previewed circle or arc; a line is its two
/// ends.
const SAMPLES_PER_TURN: f64 = 64.0;

/// The drawn line, circle or arc nearest `at` within `tol` meters, the one
/// a trim, extend or break click there acts on. Construction geometry,
/// conics and B-splines are never picked.
pub fn pick_curve(sketch: &Sketch, at: [f64; 2], tol: f64) -> Option<EntityId> {
    sketch
        .profile_entities()
        .filter_map(|(id, _)| Some((*id, Curve::from_entity(sketch, *id)?.distance_to(at))))
        .filter(|(_, d)| *d <= tol)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

/// The piece of `entity` a trim at `at` removes: the whole curve when
/// nothing crosses it.
pub fn trim_preview(sketch: &Sketch, entity: EntityId, at: [f64; 2]) -> Option<Vec<[f64; 2]>> {
    if sketch.is_construction(entity) {
        return None;
    }
    let pieces = pieces_of(sketch, entity);
    let piece = pieces
        .iter()
        .min_by(|a, b| a.curve.distance_to(at).total_cmp(&b.curve.distance_to(at)))?;
    Some(samples(&piece.curve))
}

/// The stretch an extension at `at` adds: from the end nearest `at` to
/// the first curve along its continuation. `None` where extend would
/// refuse.
pub fn extend_preview(sketch: &Sketch, entity: EntityId, at: [f64; 2]) -> Option<Vec<[f64; 2]>> {
    let (_, curve) = open_curve(sketch, entity).ok()?;
    let (reach, _) = continuation_hit(sketch, entity, &curve, at)?;
    Some(samples(&reach))
}

/// The piece a break at `at` splits off as a new entity: from the crossing
/// nearest `at` to the curve's own end. `None` where break would refuse.
pub fn break_preview(sketch: &Sketch, entity: EntityId, at: [f64; 2]) -> Option<Vec<[f64; 2]>> {
    let (_, curve) = open_curve(sketch, entity).ok()?;
    let pieces = pieces_of(sketch, entity);
    let k = (1..pieces.len()).min_by(|&a, &b| {
        let d = |i: usize| dist(at, pieces[i].curve.point_at(0.0));
        d(a).total_cmp(&d(b))
    })?;
    let t = curve.param_at(pieces[k].curve.point_at(0.0));
    Some(samples(&curve.sub_curve(t, 1.0)))
}

/// A drawn or construction line, circle or arc as a polyline, for a preview
/// of what a copy tool writes. `None` for anything else.
pub fn curve_samples(sketch: &Sketch, entity: EntityId) -> Option<Vec<[f64; 2]>> {
    Curve::from_entity(sketch, entity).map(|curve| samples(&curve))
}

fn pieces_of(sketch: &Sketch, entity: EntityId) -> Vec<Piece> {
    split_all(sketch)
        .into_iter()
        .filter(|piece| piece.entity == entity)
        .collect()
}

/// A curve as a polyline, fine enough to read as a curve at any zoom a
/// sketch is drawn at. A circle's closes on its first point.
fn samples(curve: &Curve) -> Vec<[f64; 2]> {
    let n = match *curve {
        Curve::Line { .. } => 1,
        Curve::Circle { .. } => SAMPLES_PER_TURN as usize,
        Curve::Arc { sweep, .. } => (SAMPLES_PER_TURN * sweep / std::f64::consts::TAU)
            .ceil()
            .max(2.0) as usize,
    };
    let mut points: Vec<[f64; 2]> = (0..=n)
        .map(|i| curve.point_at(i as f64 / n as f64))
        .collect();
    points.dedup_by(|a, b| dist(*a, *b) <= LENGTH_TOLERANCE);
    points
}
