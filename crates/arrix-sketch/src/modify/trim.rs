//! Trim: delete the piece of a curve between its crossings.
//!
//! The pieces are the arrangement's own (`arrangement::split_all`), so what
//! trim removes is exactly an edge the region walk would have run over. The
//! picked piece goes, and the rest of the curve stays under its id:
//!
//! - **An end piece** of a line or an arc moves that end to the cut.
//! - **A middle piece** splits the curve. The piece that holds its `start`
//!   keeps the id; the far piece is a new entity with a fresh id, and a
//!   split arc's two pieces share its centre point.
//! - **A piece of a circle** turns it into an `Arc` under the same id, the
//!   rest of the rim, CCW from the cut after the piece to the cut before it.
//! - **A curve with no crossing** is one piece, and trimming it deletes it.
//!
//! **New ends are honest.** An end cut where another curve crosses is held
//! there: on the cutter's own endpoint when the cut is one, which makes the
//! two share a point, and otherwise by `PointOnLine` or `PointOnCircle` on
//! the cutter, kept only on `check_candidate`'s `Ok`, as drawing does. A point the trimmed curve no longer uses, and no other entity
//! does, is removed with the constraints on it.
//!
//! What happens to the *other* constraints on the curve is the transfer
//! table's business ([`super::transfer`]): kept on the id, respelled,
//! copied onto the far piece or dropped — and a split's far piece is joined
//! to the kept piece's carrier, so the two stay one line or one circle.

use arrix_core::LENGTH_TOLERANCE;

use super::ModifyError;
use super::transfer::{self, Cut, CutEntity};
use crate::arrangement::curve::dist;
use crate::arrangement::{Piece, split_all};
use crate::constraint::Constraint;
use crate::draft::Draft;
use crate::entity::{Entity, Point};
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

/// What a trim did, by id. Extend and break report in the same shape
/// ([`super::extend`], [`super::break_at`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TrimEdit {
    /// The trimmed entity. It still exists under this id unless `deleted`.
    pub entity: EntityId,
    /// The whole entity went: it had no crossing to trim between.
    pub deleted: bool,
    /// The far piece of a curve trimmed in the middle, under a fresh id.
    pub split_off: Option<EntityId>,
    /// The ends the cut left, each at a crossing, in the order they were
    /// made.
    pub ends: Vec<PointId>,
    /// The `PointOnLine`/`PointOnCircle` that hold those ends on their
    /// cutters.
    pub held: Vec<ConstraintId>,
    /// Points removed because nothing used them any more.
    pub removed_points: Vec<PointId>,
    /// The rows that hold a split's far piece on the kept piece's carrier:
    /// `PointOnLine` on both its ends, or `PointOnCircle` on an arc's start.
    pub joined: Vec<ConstraintId>,
    /// Constraints rewritten by the transfer table, under their own ids.
    pub retargeted: Vec<ConstraintId>,
    /// Copies the table put on the far piece.
    pub copied: Vec<ConstraintId>,
    /// Constraints removed, by the table or with a removed point — never
    /// silently: the caller reports them.
    pub dropped: Vec<ConstraintId>,
}

/// Trims the piece of `entity` nearest `at`, in sketch `(u, v)` meters.
pub fn trim(sketch: &mut Draft, entity: EntityId, at: [f64; 2]) -> Result<TrimEdit, ModifyError> {
    if sketch.is_construction(entity) {
        return Err(ModifyError::NotTrimmable);
    }
    let pieces: Vec<Piece> = split_all(sketch)
        .into_iter()
        .filter(|piece| piece.entity == entity)
        .collect();
    let Some(k) = (0..pieces.len()).min_by(|&a, &b| {
        let d = |i: usize| pieces[i].curve.distance_to(at);
        d(a).total_cmp(&d(b))
    }) else {
        return Err(ModifyError::NotTrimmable);
    };
    let old = sketch.entities[&entity].clone();
    let mut edit = TrimEdit {
        entity,
        deleted: false,
        split_off: None,
        ends: Vec::new(),
        held: Vec::new(),
        removed_points: Vec::new(),
        joined: Vec::new(),
        retargeted: Vec::new(),
        copied: Vec::new(),
        dropped: Vec::new(),
    };

    let (n, piece) = (pieces.len(), pieces[k]);
    // Each cut end: where it is, and what cut it there.
    let cut = |end: usize| (piece.curve.point_at(end as f64), piece.cut_by[end]);
    let mut holds = Vec::new();
    let cut_kind = if n == 1 {
        // Its constraints go through the table, which drops them all.
        sketch.entities.remove(&entity);
        sketch.construction.remove(&entity);
        edit.deleted = true;
        Cut::Deleted
    } else {
        match old {
            Entity::Line { end, .. } | Entity::Arc { end, .. } if k == 0 => {
                let new = end_at(sketch, cut(1), &mut holds);
                set_ends(sketch, entity, new, end);
                edit.ends.push(new);
                Cut::Shortened
            }
            Entity::Line { start, .. } | Entity::Arc { start, .. } if k == n - 1 => {
                let new = end_at(sketch, cut(0), &mut holds);
                set_ends(sketch, entity, start, new);
                edit.ends.push(new);
                Cut::Shortened
            }
            Entity::Line { start, end } | Entity::Arc { start, end, .. } => {
                let (a, b) = (
                    end_at(sketch, cut(0), &mut holds),
                    end_at(sketch, cut(1), &mut holds),
                );
                set_ends(sketch, entity, start, a);
                let mut far = old.clone();
                if let Entity::Line { start, .. } | Entity::Arc { start, .. } = &mut far {
                    *start = b;
                }
                if let Entity::Line { end: e, .. } | Entity::Arc { end: e, .. } = &mut far {
                    *e = end;
                }
                edit.split_off = Some(sketch.add_entity(far));
                edit.ends.extend([a, b]);
                Cut::Split
            }
            Entity::Circle { center, .. } => {
                // What is left runs CCW from the piece's end round to its
                // start.
                let (a, b) = (
                    end_at(sketch, cut(1), &mut holds),
                    end_at(sketch, cut(0), &mut holds),
                );
                sketch.entities.insert(
                    entity,
                    Entity::Arc {
                        center,
                        start: a,
                        end: b,
                    },
                );
                edit.ends.extend([a, b]);
                Cut::Opened
            }
            _ => unreachable!("the arrangement splits only lines, circles and arcs"),
        }
    };

    let removed = orphans(sketch, &old.point_ids());
    let carried = transfer::carry(
        sketch,
        &CutEntity {
            entity,
            cut: cut_kind,
            line: matches!(old, Entity::Line { .. }),
            far: edit.split_off,
            ends: match old {
                Entity::Line { start, end } | Entity::Arc { start, end, .. } => Some([start, end]),
                _ => None,
            },
            removed: removed.clone(),
        },
    );
    (edit.retargeted, edit.copied, edit.dropped) =
        (carried.retargeted, carried.copied, carried.dropped);
    for p in &removed {
        sketch.remove_point(*p);
    }
    edit.removed_points = removed;
    edit.held = hold(sketch, holds);
    if let Some(far) = edit.split_off {
        edit.joined = transfer::join(sketch, entity, far);
    }
    Ok(edit)
}

/// The point a cut end lands on: the cutter's own endpoint when the cut is
/// one, or a new point there, queued in `holds` to be held on the cutter.
pub(super) fn end_at(
    sketch: &mut Draft,
    (pos, by): ([f64; 2], Option<EntityId>),
    holds: &mut Vec<(PointId, EntityId)>,
) -> PointId {
    let by = by.expect("a cut end of a split curve has a cutter");
    let shared = match &sketch.entities[&by] {
        Entity::Line { start, end } | Entity::Arc { start, end, .. } => [*start, *end]
            .into_iter()
            .find(|p| dist(sketch.points[p].pos(), pos) <= LENGTH_TOLERANCE),
        _ => None,
    };
    shared.unwrap_or_else(|| {
        let point = sketch.add_point(Point::new(pos[0], pos[1]));
        holds.push((point, by));
        point
    })
}

/// Holds each new end on its cutter, keeping each row only on
/// `check_candidate`'s `Ok`.
pub(super) fn hold(sketch: &mut Draft, holds: Vec<(PointId, EntityId)>) -> Vec<ConstraintId> {
    let mut held = Vec::new();
    for (point, by) in holds {
        let row = match sketch.entities.get(&by) {
            Some(Entity::Line { .. }) => Constraint::PointOnLine { point, line: by },
            _ => Constraint::PointOnCircle { point, circle: by },
        };
        if sketch.check_candidate(&row).is_ok() {
            held.push(sketch.add_constraint(row));
        }
    }
    held
}

/// Rewrites a line's or an arc's ends in place, keeping an arc's centre.
pub(super) fn set_ends(sketch: &mut Draft, entity: EntityId, new_start: PointId, new_end: PointId) {
    if let Some(Entity::Line { start, end } | Entity::Arc { start, end, .. }) =
        sketch.entities.get_mut(&entity)
    {
        *start = new_start;
        *end = new_end;
    }
}

/// Those of `points` no entity uses any more.
pub(super) fn orphans(sketch: &Sketch, points: &[PointId]) -> Vec<PointId> {
    points
        .iter()
        .copied()
        .filter(|p| !sketch.entities.values().any(|e| e.point_ids().contains(p)))
        .collect()
}
