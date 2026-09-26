//! A sketch edit: whole-record puts and removals, the payload of the
//! document's `SketchEdit` command (docs/DATA-MODEL.md §Commands and undo,
//! §Sketches). Like a feature edit, an entry puts the record under its id
//! and `null` removes it, and the inverse is the same shape holding what
//! each entry replaced. Applying one never solves: the author solved, and
//! the positions it produced are points in the edit (ADR-0005).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::constraint::ConstraintRecord;
use crate::entity::{Entity, Point};
use crate::ids::{ConstraintId, EntityId, PointId, SketchEntityId};
use crate::sketch::{Sketch, SketchError, SketchRecord};

/// What a sketch edit changes; an id it leaves out is kept. In `points`,
/// `entities` and `constraints` an entry puts the record under its id, or
/// with `null` removes it; in `construction` it sets or clears the mark.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchEdit {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub points: BTreeMap<PointId, Option<Point>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entities: BTreeMap<EntityId, Option<Entity>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constraints: BTreeMap<ConstraintId, Option<ConstraintRecord>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub construction: BTreeMap<EntityId, bool>,
}

impl SketchEdit {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
            && self.entities.is_empty()
            && self.constraints.is_empty()
            && self.construction.is_empty()
    }

    /// Applies the edit to `sketch` and returns its inverse. The result is
    /// checked whole, as a sketch read from a file is: a removal that
    /// leaves a record naming what it removed is refused, never swept, and
    /// `sketch` is unchanged.
    pub fn apply(&self, sketch: &mut Sketch) -> Result<SketchEdit, SketchError> {
        let mut record = SketchRecord {
            points: sketch.points.clone(),
            entities: sketch.entities.clone(),
            constraints: sketch.constraints.clone(),
            construction: sketch.construction.clone(),
        };
        let inverse = SketchEdit {
            points: swap(&mut record.points, &self.points),
            entities: swap(&mut record.entities, &self.entities),
            constraints: swap(&mut record.constraints, &self.constraints),
            construction: self
                .construction
                .iter()
                .map(|(&id, &on)| {
                    let was = if on {
                        !record.construction.insert(id)
                    } else {
                        record.construction.remove(&id)
                    };
                    (id, was)
                })
                .collect(),
        };
        *sketch = Sketch::try_from(record)?;
        Ok(inverse)
    }

    /// The edit that turns `before` into `after`: how a client makes one
    /// command of a gesture it ran on its own copy (a drag's solve, a
    /// trim).
    pub fn diff(before: &Sketch, after: &Sketch) -> SketchEdit {
        SketchEdit {
            points: changed(&before.points, &after.points),
            entities: changed(&before.entities, &after.entities),
            constraints: changed(&before.constraints, &after.constraints),
            construction: before
                .construction
                .symmetric_difference(&after.construction)
                .map(|id| (*id, after.construction.contains(id)))
                .collect(),
        }
    }

    /// Removes the records `ids` name and everything that names them: the
    /// entities on a removed point, the constraints on a removed point or
    /// entity, a removed entity's construction mark.
    pub fn remove(sketch: &Sketch, ids: &BTreeSet<SketchEntityId>) -> SketchEdit {
        let mut after = sketch.clone();
        for id in ids {
            after.remove_constraint(*id);
            after.remove_entity(*id);
            after.remove_point(*id);
        }
        Self::diff(sketch, &after)
    }
}

/// Puts or removes each entry, returning what each was before.
fn swap<V: Clone>(
    map: &mut BTreeMap<SketchEntityId, V>,
    edits: &BTreeMap<SketchEntityId, Option<V>>,
) -> BTreeMap<SketchEntityId, Option<V>> {
    edits
        .iter()
        .map(|(id, v)| {
            let old = match v {
                Some(v) => map.insert(*id, v.clone()),
                None => map.remove(id),
            };
            (*id, old)
        })
        .collect()
}

/// The entries of `after` that differ from `before`, and `null` for each
/// id `after` lacks.
fn changed<V: Clone + PartialEq>(
    before: &BTreeMap<SketchEntityId, V>,
    after: &BTreeMap<SketchEntityId, V>,
) -> BTreeMap<SketchEntityId, Option<V>> {
    let gone = before
        .keys()
        .filter(|id| !after.contains_key(id))
        .map(|id| (*id, None));
    let put = after
        .iter()
        .filter(|(id, v)| before.get(id) != Some(v))
        .map(|(id, v)| (*id, Some(v.clone())));
    gone.chain(put).collect()
}
