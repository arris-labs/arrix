//! The sketch record: points, entities, constraints and the construction
//! set, keyed by the ids their authors minted (docs/DATA-MODEL.md
//! §Sketches). It mints nothing itself; [`crate::Draft`] is the authoring
//! side that does.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::constraint::{Constraint, ConstraintPriority, ConstraintRecord};
use crate::entity::{Entity, Point};
use crate::ids::{ConstraintId, EntityId, PointId, SketchEntityId};

/// Why a record could not join a sketch. The sketch is unchanged.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SketchError {
    #[error("the sketch already holds a record with id {0}")]
    IdTaken(SketchEntityId),
    #[error("{by} names point {point}, which the sketch does not hold")]
    MissingPoint { by: SketchEntityId, point: PointId },
    #[error("{by} names entity {entity}, which the sketch does not hold")]
    MissingEntity {
        by: SketchEntityId,
        entity: EntityId,
    },
    #[error("the record keyed {key} says its id is {id}")]
    KeyMismatch {
        key: SketchEntityId,
        id: SketchEntityId,
    },
}

/// A 2D sketch on a plane, in plane-local metres. The hosting `core.sketch`
/// feature maps the plane into space.
///
/// Every reference inside resolves: an entity's points and a constraint's
/// points and entities are in the sketch, construction marks name entities
/// it holds, and one id names one record across all three maps. The
/// inserts and a deserialised sketch are checked; removal sweeps what
/// referenced the removed record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SketchRecord")]
pub struct Sketch {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) points: BTreeMap<PointId, Point>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) entities: BTreeMap<EntityId, Entity>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) constraints: BTreeMap<ConstraintId, ConstraintRecord>,
    /// Construction geometry: solved, constrained and snapped to like any
    /// other entity, but never part of a profile. A set beside `entities`,
    /// swept by every entity removal.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub(crate) construction: BTreeSet<EntityId>,
}

/// The unchecked wire form a [`Sketch`] is read through.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchRecord {
    #[serde(default)]
    points: BTreeMap<PointId, Point>,
    #[serde(default)]
    entities: BTreeMap<EntityId, Entity>,
    #[serde(default)]
    constraints: BTreeMap<ConstraintId, ConstraintRecord>,
    #[serde(default)]
    construction: BTreeSet<EntityId>,
}

impl TryFrom<SketchRecord> for Sketch {
    type Error = SketchError;

    fn try_from(r: SketchRecord) -> Result<Self, SketchError> {
        let mut sketch = Sketch::new();
        for (id, point) in r.points {
            sketch.insert_point(id, point)?;
        }
        for (id, entity) in r.entities {
            sketch.insert_entity(id, entity)?;
        }
        for (key, record) in r.constraints {
            if key != record.id {
                return Err(SketchError::KeyMismatch { key, id: record.id });
            }
            sketch.insert_constraint(record)?;
        }
        for id in r.construction {
            if !sketch.set_construction(id, true) {
                return Err(SketchError::MissingEntity { by: id, entity: id });
            }
        }
        Ok(sketch)
    }
}

impl Sketch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any record in the sketch has this id.
    pub fn contains_id(&self, id: SketchEntityId) -> bool {
        self.points.contains_key(&id)
            || self.entities.contains_key(&id)
            || self.constraints.contains_key(&id)
    }

    fn fresh(&self, id: SketchEntityId) -> Result<(), SketchError> {
        if self.contains_id(id) {
            return Err(SketchError::IdTaken(id));
        }
        Ok(())
    }

    fn check_point(&self, by: SketchEntityId, point: PointId) -> Result<(), SketchError> {
        if !self.points.contains_key(&point) {
            return Err(SketchError::MissingPoint { by, point });
        }
        Ok(())
    }

    pub fn insert_point(&mut self, id: PointId, point: Point) -> Result<(), SketchError> {
        self.fresh(id)?;
        self.points.insert(id, point);
        Ok(())
    }

    /// Adds an entity whose points are already in the sketch.
    pub fn insert_entity(&mut self, id: EntityId, entity: Entity) -> Result<(), SketchError> {
        self.fresh(id)?;
        for p in entity.point_ids() {
            self.check_point(id, p)?;
        }
        self.entities.insert(id, entity);
        Ok(())
    }

    /// Adds a constraint whose points and entities are already in the
    /// sketch.
    pub fn insert_constraint(&mut self, record: ConstraintRecord) -> Result<(), SketchError> {
        let id = record.id;
        self.fresh(id)?;
        let refs = record.constraint.refs();
        for p in refs.points {
            self.check_point(id, p)?;
        }
        for entity in refs.entities {
            if !self.entities.contains_key(&entity) {
                return Err(SketchError::MissingEntity { by: id, entity });
            }
        }
        self.constraints.insert(id, record);
        Ok(())
    }

    fn record_mut(&mut self, id: ConstraintId) -> Option<&mut ConstraintRecord> {
        self.constraints.get_mut(&id)
    }

    pub fn set_constraint_priority(
        &mut self,
        id: ConstraintId,
        priority: ConstraintPriority,
    ) -> bool {
        self.record_mut(id).map(|r| r.priority = priority).is_some()
    }

    pub fn set_constraint_driving(&mut self, id: ConstraintId, is_driving: bool) -> bool {
        self.record_mut(id)
            .map(|r| r.is_driving = is_driving)
            .is_some()
    }

    pub fn set_constraint_active(&mut self, id: ConstraintId, is_active: bool) -> bool {
        self.record_mut(id)
            .map(|r| r.is_active = is_active)
            .is_some()
    }

    pub fn set_constraint_name(&mut self, id: ConstraintId, name: Option<String>) -> bool {
        self.record_mut(id).map(|r| r.name = name).is_some()
    }

    pub fn set_constraint_display_pos(
        &mut self,
        id: ConstraintId,
        pos: Option<(f64, f64)>,
    ) -> bool {
        self.record_mut(id).map(|r| r.display_pos = pos).is_some()
    }

    /// Stores (or, with `None`, clears) the expression source that drives a
    /// record's value. Opaque here: neither parsed nor checked against the
    /// record's kind; the document resolves it, and ignores it on a
    /// reference or inactive record.
    pub fn set_constraint_expr(&mut self, id: ConstraintId, expr: Option<String>) -> bool {
        self.record_mut(id).map(|r| r.expr = expr).is_some()
    }

    /// The expression source driving a record's value, if one does.
    pub fn constraint_expr(&self, id: ConstraintId) -> Option<&str> {
        self.constraints.get(&id)?.expr.as_deref()
    }

    /// Every point, by id. Read-only: see [`Sketch::point_mut`] to edit one.
    pub fn points(&self) -> &BTreeMap<PointId, Point> {
        &self.points
    }

    /// Every entity, by id.
    pub fn entities(&self) -> &BTreeMap<EntityId, Entity> {
        &self.entities
    }

    /// Every constraint record, by id.
    pub fn constraints(&self) -> &BTreeMap<ConstraintId, ConstraintRecord> {
        &self.constraints
    }

    pub fn point(&self, id: PointId) -> Option<&Point> {
        self.points.get(&id)
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Edits one point in place: its coordinates and `fixed` flag, never
    /// its id.
    pub fn point_mut(&mut self, id: PointId) -> Option<&mut Point> {
        self.points.get_mut(&id)
    }

    /// Edits one entity's scalar geometry in place (a circle's radius).
    /// The points it names must stay in the sketch; a change of shape is a
    /// removal and an insert.
    pub fn entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    /// Edits every point in place. The set of points stays fixed.
    pub fn points_mut(&mut self) -> impl Iterator<Item = &mut Point> {
        self.points.values_mut()
    }

    pub fn get_constraint_record(&self, id: ConstraintId) -> Option<&ConstraintRecord> {
        self.constraints.get(&id)
    }

    pub fn get_constraint(&self, id: ConstraintId) -> Option<&Constraint> {
        self.constraints.get(&id).map(|r| &r.constraint)
    }

    /// Edits a constraint's values in place (a dimension, a `Fix`
    /// target). What it references must stay in the sketch.
    pub fn get_constraint_mut(&mut self, id: ConstraintId) -> Option<&mut Constraint> {
        self.constraints.get_mut(&id).map(|r| &mut r.constraint)
    }

    /// Marks an entity as construction geometry, or clears the mark. Returns
    /// `false`, and changes nothing, when the sketch has no such entity.
    pub fn set_construction(&mut self, id: EntityId, construction: bool) -> bool {
        if !self.entities.contains_key(&id) {
            return false;
        }
        if construction {
            self.construction.insert(id);
        } else {
            self.construction.remove(&id);
        }
        true
    }

    pub fn is_construction(&self, id: EntityId) -> bool {
        self.construction.contains(&id)
    }

    /// Every construction entity, by id.
    pub fn construction(&self) -> &BTreeSet<EntityId> {
        &self.construction
    }

    /// Removes a point and every entity and constraint built on it.
    pub fn remove_point(&mut self, id: PointId) {
        self.points.remove(&id);
        let gone: Vec<EntityId> = self
            .entities
            .iter()
            .filter(|(_, e)| e.point_ids().contains(&id))
            .map(|(&e, _)| e)
            .collect();
        self.constraints.retain(|_, r| !r.constraint.uses_point(id));
        for e in gone {
            self.remove_entity(e);
        }
    }

    /// Removes an entity, its construction mark and every constraint that
    /// names it. Its points stay.
    pub fn remove_entity(&mut self, id: EntityId) {
        self.entities.remove(&id);
        self.construction.remove(&id);
        self.constraints
            .retain(|_, r| !r.constraint.uses_entity(id));
    }

    pub fn remove_constraint(&mut self, id: ConstraintId) -> bool {
        self.constraints.remove(&id).is_some()
    }

    /// The nearest point to `(x, y)` within `tol`, for a pick.
    pub fn hit_point(&self, x: f64, y: f64, tol: f64) -> Option<PointId> {
        let mut best: Option<(PointId, f64)> = None;
        for (id, p) in &self.points {
            let d = (p.x - x).hypot(p.y - y);
            if d <= tol && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((*id, d));
            }
        }
        best.map(|(id, _)| id)
    }

    /// The nearest line to `(x, y)` within `tol`.
    pub fn hit_line(&self, x: f64, y: f64, tol: f64) -> Option<EntityId> {
        let mut best: Option<(EntityId, f64)> = None;
        for (id, entity) in &self.entities {
            let Entity::Line { start, end } = entity else {
                continue;
            };
            let a = &self.points[start];
            let b = &self.points[end];
            let d = dist_point_segment(x, y, a.x, a.y, b.x, b.y);
            if d <= tol && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((*id, d));
            }
        }
        best.map(|(id, _)| id)
    }
}

fn dist_point_segment(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-24 {
        return (px - ax).hypot(py - ay);
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0);
    let qx = ax + t * dx;
    let qy = ay + t * dy;
    (px - qx).hypot(py - qy)
}
