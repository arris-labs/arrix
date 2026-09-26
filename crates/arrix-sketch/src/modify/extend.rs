//! Extend and break: the two edits that move
//! or add an end without deleting a piece.
//!
//! - **Extend** runs the end of a line or an arc nearest a pick on along the
//!   curve's own continuation — the line's ray, the rest of the arc's circle
//!   — to the first curve it meets, and ends it there. With nothing to meet
//!   it is `NoCrossing`, and the sketch is untouched.
//! - **Break** splits a line or an arc at the crossing nearest a pick and
//!   deletes nothing. It is a trim's `Cut::Split` whose two cuts are one
//!   point: the piece holding `start` keeps the id, the far piece gets a
//!   fresh one, and the two share the cut as their joint.
//!
//! Both leave their new end as honest as a trim does, on the cutter's own
//! endpoint or held on the cutter, and both hand the other constraints to
//! the transfer table: an extended curve is `Cut::Shortened` (one end moved
//! to a crossing; the carrier did not move), a broken one `Cut::Split`. An
//! extended end gets a **new point**: the old one may be shared with a
//! neighbour, which stays where it is, and when it is not, it goes with the
//! constraints on it — they measured a point that moved.
//!
//! A full circle has no end to extend, and one crossing does not break it
//! into two pieces: both are `Closed`.

use std::f64::consts::TAU;

use arrix_core::LENGTH_TOLERANCE;

use super::ModifyError;
use super::transfer::{self, Cut, CutEntity};
use super::trim::{TrimEdit, end_at, orphans, set_ends};
use crate::arrangement::curve::{Curve, along, dist, sub, unit};
use crate::arrangement::{Piece, intersect, split_all};
use crate::draft::Draft;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

/// Extends the end of `entity` nearest `at`, in sketch `(u, v)` meters, to
/// the first curve along its continuation.
pub fn extend(sketch: &mut Draft, entity: EntityId, at: [f64; 2]) -> Result<TrimEdit, ModifyError> {
    let (old, curve) = open_curve(sketch, entity)?;
    let (start, end) = match old {
        Entity::Line { start, end } | Entity::Arc { start, end, .. } => (start, end),
        _ => unreachable!("open_curve admits lines and arcs"),
    };
    let (
        _,
        Hit {
            from_end,
            at: hit,
            by,
        },
    ) = continuation_hit(sketch, entity, &curve, at).ok_or(ModifyError::NoCrossing)?;

    let mut holds = Vec::new();
    let new = end_at(sketch, (hit, Some(by)), &mut holds);
    let moved = if from_end { end } else { start };
    if from_end {
        set_ends(sketch, entity, start, new);
    } else {
        set_ends(sketch, entity, new, end);
    }
    let removed = orphans(sketch, &[moved]);
    let mut edit = finish(sketch, entity, &old, Cut::Shortened, None, removed, holds);
    edit.ends.push(new);
    Ok(edit)
}

/// Breaks `entity` at the crossing nearest `at`, keeping both pieces.
pub fn break_at(
    sketch: &mut Draft,
    entity: EntityId,
    at: [f64; 2],
) -> Result<TrimEdit, ModifyError> {
    let (old, _) = open_curve(sketch, entity)?;
    let pieces: Vec<Piece> = split_all(sketch)
        .into_iter()
        .filter(|piece| piece.entity == entity)
        .collect();
    // The crossings are where one piece ends and the next starts.
    let Some(k) = (1..pieces.len()).min_by(|&a, &b| {
        let d = |i: usize| dist(at, pieces[i].curve.point_at(0.0));
        d(a).total_cmp(&d(b))
    }) else {
        return Err(ModifyError::NoCrossing);
    };
    let (start, end) = match old {
        Entity::Line { start, end } | Entity::Arc { start, end, .. } => (start, end),
        _ => unreachable!("open_curve admits lines and arcs"),
    };

    let mut holds = Vec::new();
    let cut = (pieces[k].curve.point_at(0.0), pieces[k].cut_by[0]);
    let joint = end_at(sketch, cut, &mut holds);
    set_ends(sketch, entity, start, joint);
    let mut far = old.clone();
    if let Entity::Line { start, end: e } | Entity::Arc { start, end: e, .. } = &mut far {
        (*start, *e) = (joint, end);
    }
    let far = sketch.add_entity(far);
    let mut edit = finish(
        sketch,
        entity,
        &old,
        Cut::Split,
        Some(far),
        Vec::new(),
        holds,
    );
    edit.split_off = Some(far);
    edit.ends.push(joint);
    edit.joined = transfer::join(sketch, entity, far);
    Ok(edit)
}

/// Where an extension ends: which end runs on, the point it reaches, and
/// the curve it meets there.
pub(super) struct Hit {
    pub from_end: bool,
    pub at: [f64; 2],
    pub by: EntityId,
}

/// What extending the end of `curve` nearest `at` adds, and where it ends —
/// the stretch the hover previews and the hit the edit ends on.
pub(super) fn continuation_hit(
    sketch: &Sketch,
    entity: EntityId,
    curve: &Curve,
    at: [f64; 2],
) -> Option<(Curve, Hit)> {
    let from_end = dist(at, curve.point_at(1.0)) <= dist(at, curve.point_at(0.0));
    let ext = continuation(sketch, curve, from_end);
    let (t, hit, by) = first_hit(sketch, entity, &ext, from_end)?;
    // An arc's continuation back past its start runs CCW *to* the start.
    let reach = match ext {
        Curve::Arc { .. } if !from_end => ext.sub_curve(t, 1.0),
        _ => ext.sub_curve(0.0, t),
    };
    Some((
        reach,
        Hit {
            from_end,
            at: hit,
            by,
        },
    ))
}

/// The entity and its curve, when it is a drawn line or arc.
pub(super) fn open_curve(
    sketch: &Sketch,
    entity: EntityId,
) -> Result<(Entity, Curve), ModifyError> {
    if sketch.is_construction(entity) {
        return Err(ModifyError::NotTrimmable);
    }
    match (
        sketch.entities.get(&entity),
        Curve::from_entity(sketch, entity),
    ) {
        (Some(Entity::Circle { .. }), _) => Err(ModifyError::Closed),
        (Some(e @ (Entity::Line { .. } | Entity::Arc { .. })), Some(curve))
            if !curve.is_closed() =>
        {
            Ok((e.clone(), curve))
        }
        _ => Err(ModifyError::NotTrimmable),
    }
}

/// The first point where the continuation `ext` (past the end, or with
/// `from_end` false, back past the start) meets another drawn curve: its
/// parameter along `ext`, the point, and the curve. A hit on the end
/// itself, or on an arc's own far end, is no extension.
fn first_hit(
    sketch: &Sketch,
    entity: EntityId,
    ext: &Curve,
    from_end: bool,
) -> Option<(f64, [f64; 2], EntityId)> {
    let others: Vec<(EntityId, Curve)> = sketch
        .profile_entities()
        .filter(|(id, _)| **id != entity)
        .filter_map(|(id, _)| Curve::from_entity(sketch, *id).map(|c| (*id, c)))
        .collect();
    let tol = ext.param_tolerance();
    // How far along the continuation a hit lies: an arc's continuation back
    // past its start runs the other way round the rest of the circle.
    let along_ext = |t: f64| match ext {
        Curve::Arc { .. } if !from_end => 1.0 - t,
        _ => t,
    };
    others
        .iter()
        .flat_map(|(id, other)| intersect(ext, other).into_iter().map(move |p| (p, *id)))
        .map(|(p, id)| (ext.param_at(p), p, id))
        .filter(|(t, _, _)| *t > tol && *t < 1.0 - tol)
        .min_by(|a, b| {
            along_ext(a.0)
                .total_cmp(&along_ext(b.0))
                .then(a.2.cmp(&b.2))
        })
}

/// The stretch an end can be extended over: for a line, a segment along its
/// ray long enough to reach anything in the sketch; for an arc, the rest of
/// its circle.
fn continuation(sketch: &Sketch, curve: &Curve, from_end: bool) -> Curve {
    match *curve {
        Curve::Line { a, b } => {
            let (origin, dir) = if from_end {
                (b, unit(sub(b, a)))
            } else {
                (a, unit(sub(a, b)))
            };
            let reach = reach_from(sketch, origin);
            Curve::Line {
                a: origin,
                b: along(origin, dir, reach),
            }
        }
        Curve::Arc {
            center,
            radius,
            start_angle,
            sweep,
        } => Curve::Arc {
            center,
            radius,
            start_angle: start_angle + sweep,
            sweep: TAU - sweep,
        },
        Curve::Circle { .. } => unreachable!("open_curve admits no circle"),
    }
}

/// A distance past which no curve in the sketch lies, seen from `origin`:
/// the farthest point plus the largest radius, with a millimetre spare.
fn reach_from(sketch: &Sketch, origin: [f64; 2]) -> f64 {
    let far = sketch
        .points
        .values()
        .map(|p| dist(origin, p.pos()))
        .fold(0.0, f64::max);
    let radius = sketch
        .entities
        .values()
        .filter_map(|e| match e {
            Entity::Circle { radius, .. } => Some(*radius),
            _ => None,
        })
        .fold(0.0, f64::max);
    // An arc's radius is a distance between two of its points, so `far`
    // already covers it.
    far + radius + 1.0e-3 + LENGTH_TOLERANCE
}

/// The bookkeeping extend and break share with trim: the transfer table,
/// the removed points and the holds on the cutters.
fn finish(
    sketch: &mut Draft,
    entity: EntityId,
    old: &Entity,
    cut: Cut,
    far: Option<EntityId>,
    removed: Vec<PointId>,
    holds: Vec<(PointId, EntityId)>,
) -> TrimEdit {
    let carried = transfer::carry(
        sketch,
        &CutEntity {
            entity,
            cut,
            line: matches!(old, Entity::Line { .. }),
            far,
            ends: match *old {
                Entity::Line { start, end } | Entity::Arc { start, end, .. } => Some([start, end]),
                _ => None,
            },
            removed: removed.clone(),
        },
    );
    for p in &removed {
        sketch.remove_point(*p);
    }
    let held = super::trim::hold(sketch, holds);
    TrimEdit {
        entity,
        deleted: false,
        split_off: None,
        ends: Vec::new(),
        held,
        removed_points: removed,
        joined: Vec::new(),
        retargeted: carried.retargeted,
        copied: carried.copied,
        dropped: carried.dropped,
    }
}
