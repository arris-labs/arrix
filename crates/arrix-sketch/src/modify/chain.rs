//! Chain discovery for offset: from one drawn line or arc, the run of curves that join it end to end.
//!
//! A chain grows both ways from the picked curve while exactly two of the
//! sketch's chain curves meet at the point it has reached. It stops at an
//! end (one curve), at a branch (three or more), or when it closes on the
//! curve it started from. A circle is a chain of one. Only curves that can
//! be offset count, at either end of that test: a conic, a B-spline or a lone point is neither a link nor a branch, and a
//! construction curve is a link only in a chain that was picked from a
//! construction curve, so a drawn chain never picks up the construction
//! anchors an earlier offset left beside it and a construction chain never
//! reaches into drawn geometry.
//!
//! The chain has a direction: the picked curve's own, start to end. Each
//! link is read in it, so a curve the walk meets end-first is `reversed`.

use std::collections::{BTreeMap, BTreeSet};

use super::ModifyError;
use crate::arrangement::curve::Curve;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

/// One curve of a [`Chain`], in the chain's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub entity: EntityId,
    /// The point the chain enters the curve at, and the one it leaves by. A
    /// circle has neither.
    pub from: Option<PointId>,
    pub to: Option<PointId>,
    /// The chain runs against the curve's own direction (`end` to `start`).
    pub reversed: bool,
}

/// A run of drawn lines and arcs joined end to end, or one circle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    /// In the chain's direction, which is the picked curve's own.
    pub links: Vec<Link>,
    /// The last link leaves at the first link's entry point.
    pub closed: bool,
}

impl Chain {
    pub fn entities(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.links.iter().map(|l| l.entity)
    }
}

/// The chain `start` belongs to.
pub fn find_chain(sketch: &Sketch, start: EntityId) -> Result<Chain, ModifyError> {
    let construction = sketch.is_construction(start);
    let curves = CurveTable::new(sketch, construction);
    match sketch.entities.get(&start) {
        Some(Entity::Circle { .. }) if curves.usable(start) => {
            return Ok(Chain {
                links: vec![Link {
                    entity: start,
                    from: None,
                    to: None,
                    reversed: false,
                }],
                closed: true,
            });
        }
        Some(Entity::Line { .. } | Entity::Arc { .. }) if curves.usable(start) => {}
        _ => return Err(ModifyError::NotAChain),
    }
    let ends = curves.ends[&start];

    let mut seen = BTreeSet::from([start]);
    let mut links = vec![Link {
        entity: start,
        from: Some(ends.0),
        to: Some(ends.1),
        reversed: false,
    }];

    // Forward from the picked curve's end.
    let mut closed = false;
    loop {
        let last = *links.last().unwrap();
        let Some(next) = curves.only_other(last.to.unwrap(), last.entity) else {
            break;
        };
        if next == start {
            closed = true;
            break;
        }
        if !seen.insert(next) {
            break;
        }
        let (a, b) = curves.ends[&next];
        let reversed = b == last.to.unwrap();
        links.push(Link {
            entity: next,
            from: last.to,
            to: Some(if reversed { a } else { b }),
            reversed,
        });
    }

    // Backward from its start, unless the loop is already closed.
    if !closed {
        loop {
            let first = links[0];
            let Some(prev) = curves.only_other(first.from.unwrap(), first.entity) else {
                break;
            };
            if !seen.insert(prev) {
                break;
            }
            let (a, b) = curves.ends[&prev];
            let reversed = a == first.from.unwrap();
            links.insert(
                0,
                Link {
                    entity: prev,
                    from: Some(if reversed { b } else { a }),
                    to: first.from,
                    reversed,
                },
            );
        }
    }
    Ok(Chain { links, closed })
}

/// The chain curves of one construction flag, and which of them meet at each
/// point.
struct CurveTable {
    /// Each usable line and arc's `(start, end)`.
    ends: BTreeMap<EntityId, (PointId, PointId)>,
    /// Usable circles.
    circles: BTreeSet<EntityId>,
    at: BTreeMap<PointId, Vec<EntityId>>,
}

impl CurveTable {
    fn new(sketch: &Sketch, construction: bool) -> Self {
        let mut table = Self {
            ends: BTreeMap::new(),
            circles: BTreeSet::new(),
            at: BTreeMap::new(),
        };
        for (&id, entity) in &sketch.entities {
            if sketch.is_construction(id) != construction
                || Curve::from_entity(sketch, id).is_none()
            {
                continue;
            }
            match entity {
                Entity::Line { start, end } | Entity::Arc { start, end, .. } if start != end => {
                    table.ends.insert(id, (*start, *end));
                    table.at.entry(*start).or_default().push(id);
                    table.at.entry(*end).or_default().push(id);
                }
                Entity::Circle { .. } => {
                    table.circles.insert(id);
                }
                _ => {}
            }
        }
        table
    }

    fn usable(&self, id: EntityId) -> bool {
        self.ends.contains_key(&id) || self.circles.contains(&id)
    }

    /// The one curve other than `not` that meets `at`; none at an end (no
    /// other) or a branch (two or more).
    fn only_other(&self, at: PointId, not: EntityId) -> Option<EntityId> {
        let mut others = self.at.get(&at)?.iter().filter(|&&e| e != not);
        let first = others.next()?;
        others.next().is_none().then_some(*first)
    }
}
