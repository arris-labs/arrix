//! Mirror a selection across a line or a datum axis.
//!
//! Each distinct point of the selection is mirrored once, across shapes, so
//! two lines that share a corner share the mirrored corner. A point on the
//! axis is not copied: the mirrored curves share it, and it is held on the
//! axis (`PointOnLine` or `PointOnDatum`) where [`Sketch::check_candidate`]
//! says the hold is `Ok`, which it is not when something already holds it.
//!
//! Each copied point is held by `Symmetric` (a line axis) or
//! `SymmetricAcrossDatum` (a datum axis), and a circle's radius by
//! `Equal(orig, copy)`. That is the whole row set: symmetry pins every copy
//! variable, so a mirror adds no degree of freedom and none of these rows can
//! be redundant, since each pins a variable nothing else touches.
//!
//! A reflection flips an arc's sense, and an arc runs counter-clockwise from
//! `start` to `end`, so the copy's `start` is the mirror of the original's
//! `end`. A line or circle that lies on the axis is its own mirror and is not
//! copied.

use std::collections::BTreeMap;

use arrix_core::LENGTH_TOLERANCE;

use super::ModifyError;
use crate::constraint::{Constraint, DatumEntity};
use crate::draft::Draft;
use crate::entity::{Entity, Point};
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

/// What a mirror reflects across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    /// A drawn or construction line: its whole carrier, not only the segment.
    Line(EntityId),
    /// [`DatumEntity::AxisX`] or [`DatumEntity::AxisY`]; the origin is not an
    /// axis.
    Datum(DatumEntity),
}

/// What a mirror wrote, by id.
#[derive(Debug, Clone, PartialEq)]
pub struct MirrorEdit {
    /// `(original, copy)` per mirrored entity, in selection order.
    pub pairs: Vec<(EntityId, EntityId)>,
    /// `(original, copy)` per mirrored point, in the order first met.
    pub points: Vec<(PointId, PointId)>,
    /// Points on the axis, shared by original and copy rather than copied.
    pub on_axis: Vec<PointId>,
    /// The rows written, in order: the holds on the axis, then one symmetry
    /// row per copied point, then a circle's `Equal`.
    pub constraints: Vec<ConstraintId>,
    /// Holds the sketch already implied: not added, reported so a refusal is
    /// never silent.
    pub dropped: Vec<Constraint>,
    /// Selected entities that were not mirrored: the axis itself, conics
    /// and B-splines, and a line or circle on the axis.
    pub skipped: Vec<EntityId>,
}

/// Copies `selection` across `axis` and ties every copied point to its
/// original by symmetry. An error leaves the sketch untouched.
pub fn mirror(
    sketch: &mut Draft,
    selection: &[EntityId],
    axis: MirrorAxis,
) -> Result<MirrorEdit, ModifyError> {
    let reflect = Reflection::of(sketch, axis)?;
    let on_axis = |p: PointId| reflect.holds(sketch, p);

    let mut kept = Vec::new();
    let mut skipped = Vec::new();
    for &id in selection {
        if kept.contains(&id) || skipped.contains(&id) {
            continue;
        }
        let own_mirror = match sketch.entities.get(&id) {
            _ if axis == MirrorAxis::Line(id) => true,
            Some(Entity::Point(p)) => on_axis(*p),
            Some(Entity::Line { start, end }) => on_axis(*start) && on_axis(*end),
            Some(Entity::Circle { center, .. }) => on_axis(*center),
            Some(Entity::Arc { .. }) => false,
            _ => true, // missing, conic, B-spline
        };
        if own_mirror {
            skipped.push(id);
        } else {
            kept.push(id);
        }
    }
    if kept.is_empty() {
        return Err(ModifyError::NothingToMirror);
    }

    // Every point of the kept entities, in the order first met, split into
    // the ones on the axis and the ones to copy.
    let mut shared = Vec::new();
    let mut copied: Vec<PointId> = Vec::new();
    for id in &kept {
        for p in defining_points(&sketch.entities[id]) {
            if shared.contains(&p) || copied.contains(&p) {
                continue;
            }
            if on_axis(p) {
                shared.push(p);
            } else {
                copied.push(p);
            }
        }
    }

    let mut image: BTreeMap<PointId, PointId> = shared.iter().map(|&p| (p, p)).collect();
    let mut points = Vec::with_capacity(copied.len());
    for &p in &copied {
        let at = reflect.apply(sketch.points[&p].pos());
        let copy = sketch.add_point(Point::new(at[0], at[1]));
        image.insert(p, copy);
        points.push((p, copy));
    }

    let mut pairs = Vec::with_capacity(kept.len());
    for &id in &kept {
        let m = |p: &PointId| image[p];
        let copy = match &sketch.entities[&id] {
            Entity::Point(p) => Entity::Point(m(p)),
            Entity::Line { start, end } => Entity::Line {
                start: m(start),
                end: m(end),
            },
            Entity::Circle { center, radius } => Entity::Circle {
                center: m(center),
                radius: *radius,
            },
            Entity::Arc { center, start, end } => Entity::Arc {
                center: m(center),
                start: m(end),
                end: m(start),
            },
            #[cfg(feature = "conics")]
            _ => unreachable!("only native entities are kept"),
        };
        let copy = sketch.add_entity(copy);
        let construction = sketch.is_construction(id);
        sketch.set_construction(copy, construction);
        pairs.push((id, copy));
    }

    let mut constraints = Vec::new();
    let mut dropped = Vec::new();
    for &point in &shared {
        let row = reflect.hold(point);
        if sketch.check_candidate(&row).is_ok() {
            constraints.push(sketch.add_constraint(row));
        } else {
            dropped.push(row);
        }
    }
    for &(a, b) in &points {
        constraints.push(sketch.add_constraint(reflect.symmetric(a, b)));
    }
    for &(a, b) in &pairs {
        if matches!(sketch.entities[&a], Entity::Circle { .. }) {
            constraints.push(sketch.add_constraint(Constraint::Equal { a, b }));
        }
    }

    Ok(MirrorEdit {
        pairs,
        points,
        on_axis: shared,
        constraints,
        dropped,
        skipped,
    })
}

/// The points an entity's pose is made of, which is what a mirror copies.
fn defining_points(entity: &Entity) -> Vec<PointId> {
    match *entity {
        Entity::Point(p) => vec![p],
        Entity::Line { start, end } => vec![start, end],
        Entity::Circle { center, .. } => vec![center],
        Entity::Arc { center, start, end } => vec![center, start, end],
        #[cfg(feature = "conics")]
        _ => Vec::new(),
    }
}

/// The reflection across the axis, and the rows that speak of it.
struct Reflection {
    axis: MirrorAxis,
    /// A point on the axis and its unit direction.
    origin: [f64; 2],
    dir: [f64; 2],
}

impl Reflection {
    fn of(sketch: &Sketch, axis: MirrorAxis) -> Result<Self, ModifyError> {
        let (origin, dir) = match axis {
            MirrorAxis::Datum(DatumEntity::AxisX) => ([0.0, 0.0], [1.0, 0.0]),
            MirrorAxis::Datum(DatumEntity::AxisY) => ([0.0, 0.0], [0.0, 1.0]),
            MirrorAxis::Datum(DatumEntity::Origin) => return Err(ModifyError::NoAxis),
            MirrorAxis::Line(id) => {
                let Some(Entity::Line { start, end }) = sketch.entities.get(&id) else {
                    return Err(ModifyError::NoAxis);
                };
                let (a, b) = (sketch.points[start].pos(), sketch.points[end].pos());
                let d = [b[0] - a[0], b[1] - a[1]];
                let len = d[0].hypot(d[1]);
                if len <= LENGTH_TOLERANCE {
                    return Err(ModifyError::NoAxis);
                }
                (a, [d[0] / len, d[1] / len])
            }
        };
        Ok(Self { axis, origin, dir })
    }

    fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        let v = [p[0] - self.origin[0], p[1] - self.origin[1]];
        let along = v[0] * self.dir[0] + v[1] * self.dir[1];
        let foot = [
            self.origin[0] + along * self.dir[0],
            self.origin[1] + along * self.dir[1],
        ];
        [2.0 * foot[0] - p[0], 2.0 * foot[1] - p[1]]
    }

    /// Is `p` on the axis, by position or by a row that holds it there?
    fn holds(&self, sketch: &Sketch, p: PointId) -> bool {
        let at = sketch.points[&p].pos();
        let q = self.apply(at);
        if (q[0] - at[0]).hypot(q[1] - at[1]) <= 2.0 * LENGTH_TOLERANCE {
            return true;
        }
        sketch
            .constraints()
            .values()
            .any(|r| match (&r.constraint, self.axis) {
                (Constraint::PointOnLine { point, line }, MirrorAxis::Line(axis)) => {
                    *point == p && *line == axis
                }
                (
                    Constraint::PointOnDatum { point, datum }
                    | Constraint::CoincidentToDatum { point, datum },
                    MirrorAxis::Datum(axis),
                ) => *point == p && (*datum == axis || *datum == DatumEntity::Origin),
                _ => false,
            })
    }

    fn hold(&self, point: PointId) -> Constraint {
        match self.axis {
            MirrorAxis::Line(line) => Constraint::PointOnLine { point, line },
            MirrorAxis::Datum(datum) => Constraint::PointOnDatum { point, datum },
        }
    }

    fn symmetric(&self, a: PointId, b: PointId) -> Constraint {
        match self.axis {
            MirrorAxis::Line(mirror) => Constraint::Symmetric { a, b, mirror },
            MirrorAxis::Datum(datum) => Constraint::SymmetricAcrossDatum { a, b, datum },
        }
    }
}
