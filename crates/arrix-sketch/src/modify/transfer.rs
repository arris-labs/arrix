//! The transfer table: what a trim does to the constraints on the curve it
//! cut (docs/DATA-MODEL.md §Sketches).
//!
//! Three rules decide every constraint, in order:
//!
//! 1. **A constraint that names a point the trim removed is dropped.** The
//!    point is gone; nothing else stands where it stood.
//! 2. **A `Tangent` whose joint went with a split's far piece follows it.**
//!    The row is first order only where the line and the arc share an end,
//!    so it names the piece that still does.
//! 3. **A constraint that names the cut entity follows [`verdict`].** Most
//!    kinds measure the *carrier* — the infinite line or the full circle —
//!    which a trim does not move, so they are kept on the id. What measured
//!    the *span* (`Equal` between lines, `ArcLength`, `Midpoint`) no longer
//!    can, and is dropped or, where the original ends both survive,
//!    respelled over them.
//!
//! The far piece of a split curve is **joined** to the piece that kept the
//! id: both ends of a line's far piece `PointOnLine` on the kept piece, a
//! split arc's far start `PointOnCircle` on it (the two already share a
//! centre). That puts both pieces on one carrier, so every carrier kind kept
//! on the id binds the far piece too, and copying `Horizontal`, `Parallel`,
//! `Radius` and the rest onto it would only add redundant rows. The one kind
//! copied is `Block`, which freezes a pose rather than a carrier.

use crate::constraint::Constraint;
use crate::draft::Draft;
use crate::entity::Entity;
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

/// How a trim changed the entity a constraint names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// One end of a line or an arc moved to a crossing.
    Shortened,
    /// A middle piece went: the id keeps the piece holding `start`, and the
    /// far piece is a new entity.
    Split,
    /// A circle lost a piece and became an arc under the same id.
    Opened,
    /// The whole entity went: it had no crossing.
    Deleted,
}

/// What happens to one constraint that names a cut entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// It stays as it was, on the same id.
    Kept,
    /// It is rewritten to name what now carries its meaning, under the same
    /// constraint id — or left as it was when that is still the kept piece.
    Retargeted,
    /// It stays, and a copy goes onto the far piece.
    Copied,
    /// It is removed and reported.
    Dropped,
}

/// The table, for a constraint `c` naming an entity that `cut` changed.
/// Only its kind is read. `line` says whether that entity is a line (an arc
/// or circle otherwise). A constraint naming a removed point, or a
/// `Tangent` that follows its joint, never reaches it.
pub fn verdict(c: &Constraint, cut: Cut, line: bool) -> Verdict {
    use Constraint as K;
    match (c, cut) {
        (_, Cut::Deleted) => Verdict::Dropped,
        // What measured the span: a line's length, an arc's sweep.
        (K::ArcLength { .. }, _) => Verdict::Dropped,
        (K::Equal { .. }, _) if line => Verdict::Dropped,
        // A split keeps both original ends, so the old midpoint is still
        // theirs; a shortened line lost one of them.
        (K::Midpoint { .. }, Cut::Split) => Verdict::Retargeted,
        (K::Midpoint { .. }, _) => Verdict::Dropped,
        (K::Block { .. }, Cut::Split) => Verdict::Copied,
        // Everything else measures the carrier, which a trim does not move.
        _ => Verdict::Kept,
    }
}

/// What a trim did to the cut entity, for [`carry`].
pub(super) struct CutEntity {
    pub entity: EntityId,
    pub cut: Cut,
    pub line: bool,
    /// The far piece of a split.
    pub far: Option<EntityId>,
    /// The entity's `start` and `end` before the cut, for a line or an arc.
    pub ends: Option<[PointId; 2]>,
    /// Points the cut leaves unused, about to be removed.
    pub removed: Vec<PointId>,
}

/// The constraints [`carry`] touched, by what it did to them.
#[derive(Debug, Default)]
pub(super) struct Carried {
    pub retargeted: Vec<ConstraintId>,
    pub copied: Vec<ConstraintId>,
    pub dropped: Vec<ConstraintId>,
}

/// Applies the three rules to every constraint in `sketch`, after the cut's
/// geometry is written and before its removed points go.
pub(super) fn carry(sketch: &mut Draft, cut: &CutEntity) -> Carried {
    let mut carried = Carried::default();
    let mut copies = Vec::new();
    let ids: Vec<ConstraintId> = sketch.constraints.keys().copied().collect();
    for cid in ids {
        let record = &sketch.constraints[&cid];
        let c = &record.constraint;
        if cut.removed.iter().any(|p| c.uses_point(*p)) {
            carried.dropped.push(cid);
            continue;
        }
        if !c.uses_entity(cut.entity) {
            continue;
        }
        if let Some(new) = follow_joint(sketch, c, cut) {
            sketch.constraints.get_mut(&cid).expect("listed").constraint = new;
            carried.retargeted.push(cid);
            continue;
        }
        match verdict(c, cut.cut, cut.line) {
            Verdict::Kept => {}
            Verdict::Dropped => carried.dropped.push(cid),
            Verdict::Retargeted => {
                let new = respell(c, cut).expect("the table retargets only what it can respell");
                sketch.constraints.get_mut(&cid).expect("listed").constraint = new;
                carried.retargeted.push(cid);
            }
            Verdict::Copied => {
                let far = cut.far.expect("a split has a far piece");
                let mut copy = record.clone();
                copy.constraint = rename_entity(c, cut.entity, far);
                copies.push(copy);
            }
        }
    }
    for cid in &carried.dropped {
        sketch.remove_constraint(*cid);
    }
    for record in copies {
        carried.copied.push(sketch.add_constraint_with_full_options(
            record.constraint,
            record.is_driving,
            record.is_active,
            record.name,
            record.priority,
        ));
    }
    carried
}

/// The respelling of a `Retargeted` constraint over what survived.
fn respell(c: &Constraint, cut: &CutEntity) -> Option<Constraint> {
    match *c {
        Constraint::Midpoint { point, .. } => {
            let [a, b] = cut.ends?;
            Some(Constraint::SymmetricPoints {
                a,
                b,
                center: point,
            })
        }
        _ => None,
    }
}

/// Rule 2: a `Tangent` on a split curve, renamed onto the far piece when
/// that piece, and not the kept one, shares an end with its partner.
fn follow_joint(sketch: &Sketch, c: &Constraint, cut: &CutEntity) -> Option<Constraint> {
    let Constraint::Tangent { line, circle } = *c else {
        return None;
    };
    let far = cut.far?;
    let partner = if line == cut.entity { circle } else { line };
    let ends = |id: EntityId| -> Vec<PointId> {
        match sketch.entities.get(&id) {
            Some(Entity::Line { start, end } | Entity::Arc { start, end, .. }) => {
                vec![*start, *end]
            }
            _ => Vec::new(),
        }
    };
    let joint = |piece: EntityId| ends(partner).iter().any(|p| ends(piece).contains(p));
    (joint(far) && !joint(cut.entity)).then(|| rename_entity(c, cut.entity, far))
}

/// `c` with every mention of entity `from` replaced by `to`.
fn rename_entity(c: &Constraint, from: EntityId, to: EntityId) -> Constraint {
    let swap = |e: EntityId| if e == from { to } else { e };
    match *c {
        Constraint::Block { entity } => Constraint::Block {
            entity: swap(entity),
        },
        Constraint::Tangent { line, circle } => Constraint::Tangent {
            line: swap(line),
            circle: swap(circle),
        },
        ref other => other.clone(),
    }
}

/// Joins a split's far piece to the carrier of the piece that kept the id,
/// each row kept only on `check_candidate`'s `Ok`. A point the kept piece
/// already has as an end — a break's joint — is on its carrier by
/// construction and gets no row.
pub(super) fn join(sketch: &mut Draft, kept: EntityId, far: EntityId) -> Vec<ConstraintId> {
    let own = sketch
        .entities
        .get(&kept)
        .map(Entity::point_ids)
        .unwrap_or_default();
    let rows: Vec<Constraint> = match sketch.entities.get(&far) {
        Some(Entity::Line { start, end }) => [*start, *end]
            .into_iter()
            .map(|point| Constraint::PointOnLine { point, line: kept })
            .collect(),
        Some(Entity::Arc { start, .. }) => vec![Constraint::PointOnCircle {
            point: *start,
            circle: kept,
        }],
        _ => Vec::new(),
    };
    rows.into_iter()
        .filter(|row| !own.iter().any(|p| row.uses_point(*p)))
        .filter_map(|row| {
            sketch
                .check_candidate(&row)
                .is_ok()
                .then(|| sketch.add_constraint(row))
        })
        .collect()
}
