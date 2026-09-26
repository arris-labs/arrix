//! A sketch being authored: the sketch and the minter its new ids come
//! from. The author of a command mints the ids it carries
//! (docs/DATA-MODEL.md §Identifiers); this is that author's side, for a
//! client's sketch mode and for tests. The record itself, [`Sketch`],
//! mints nothing.

use std::ops::{Deref, DerefMut};

use arrix_core::IdMinter;

use crate::constraint::{Constraint, ConstraintPriority, ConstraintRecord};
use crate::entity::{Entity, Point};
use crate::ids::{ConstraintId, EntityId, PointId, SketchEntityId};
use crate::sketch::Sketch;

/// A [`Sketch`] and an [`IdMinter`]. Derefs to the sketch, so everything
/// that reads or solves a sketch takes a draft as well.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub sketch: Sketch,
    pub ids: IdMinter,
}

impl Deref for Draft {
    type Target = Sketch;

    fn deref(&self) -> &Sketch {
        &self.sketch
    }
}

impl DerefMut for Draft {
    fn deref_mut(&mut self) -> &mut Sketch {
        &mut self.sketch
    }
}

impl Draft {
    /// An empty sketch whose ids come from `ids`.
    pub fn new(ids: IdMinter) -> Self {
        Self::with_sketch(Sketch::new(), ids)
    }

    /// An empty sketch whose ids come from a minter seeded with `seed`
    /// (tests pass a fixed one).
    pub fn seeded(seed: u64) -> Self {
        Self::new(IdMinter::new(seed))
    }

    pub fn with_sketch(sketch: Sketch, ids: IdMinter) -> Self {
        Self { sketch, ids }
    }

    /// The next id no record in the sketch has. A minted id the sketch
    /// already holds (a minter re-seeded over its own ids) is skipped, not
    /// reused.
    pub fn mint(&mut self) -> SketchEntityId {
        loop {
            let id = self.ids.mint();
            if !self.sketch.contains_id(id) {
                return id;
            }
        }
    }

    pub fn add_point(&mut self, point: Point) -> PointId {
        let id = self.mint();
        self.sketch
            .insert_point(id, point)
            .expect("a fresh id is free");
        id
    }

    /// Adds an entity on points already in the sketch.
    ///
    /// # Panics
    /// If a point it names is not in the sketch.
    pub fn add_entity(&mut self, entity: Entity) -> EntityId {
        let id = self.mint();
        if let Err(e) = self.sketch.insert_entity(id, entity) {
            panic!("add_entity: {e}");
        }
        id
    }

    /// Adds a record for `constraint` with the given flags.
    ///
    /// # Panics
    /// If the constraint names a point or entity not in the sketch.
    pub fn add_constraint_with_full_options(
        &mut self,
        constraint: Constraint,
        is_driving: bool,
        is_active: bool,
        name: Option<String>,
        priority: ConstraintPriority,
    ) -> ConstraintId {
        let id = self.mint();
        let record = ConstraintRecord {
            is_driving,
            is_active,
            name,
            priority,
            ..ConstraintRecord::new(id, constraint)
        };
        if let Err(e) = self.sketch.insert_constraint(record) {
            panic!("add_constraint: {e}");
        }
        id
    }

    pub fn add_constraint_with_options(
        &mut self,
        constraint: Constraint,
        is_driving: bool,
        is_active: bool,
        name: Option<String>,
    ) -> ConstraintId {
        self.add_constraint_with_full_options(
            constraint,
            is_driving,
            is_active,
            name,
            ConstraintPriority::Primary,
        )
    }

    /// Adds a driving, active constraint.
    pub fn add_constraint(&mut self, constraint: Constraint) -> ConstraintId {
        self.add_constraint_with_options(constraint, true, true, None)
    }

    /// Adds a reference (driven) measurement.
    pub fn add_reference_constraint(&mut self, constraint: Constraint) -> ConstraintId {
        self.add_constraint_with_options(constraint, false, true, None)
    }

    /// Two free points and a line between them.
    pub fn add_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> (PointId, PointId, EntityId) {
        let start = self.add_point(Point::new(x1, y1));
        let end = self.add_point(Point::new(x2, y2));
        let line = self.add_entity(Entity::Line { start, end });
        (start, end, line)
    }

    /// A centre point and a circle on it.
    pub fn add_circle(&mut self, cx: f64, cy: f64, radius: f64) -> (PointId, EntityId) {
        let center = self.add_point(Point::new(cx, cy));
        let circle = self.add_entity(Entity::Circle {
            center,
            radius: radius.abs(),
        });
        (center, circle)
    }

    /// Centre, start and end points and the arc on them.
    pub fn add_arc(
        &mut self,
        cx: f64,
        cy: f64,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
    ) -> (PointId, PointId, PointId, EntityId) {
        let center = self.add_point(Point::new(cx, cy));
        let start = self.add_point(Point::new(sx, sy));
        let end = self.add_point(Point::new(ex, ey));
        let arc = self.add_entity(Entity::Arc { center, start, end });
        (center, start, end, arc)
    }

    /// Centre, major-axis end and minor radius: an ellipse.
    #[cfg(feature = "conics")]
    pub fn add_ellipse(
        &mut self,
        cx: f64,
        cy: f64,
        mx: f64,
        my: f64,
        minor_radius: f64,
    ) -> (PointId, PointId, EntityId) {
        let center = self.add_point(Point::new(cx, cy));
        let major_axis_end = self.add_point(Point::new(mx, my));
        let entity = self.add_entity(Entity::Ellipse {
            center,
            major_axis_end,
            minor_radius: minor_radius.abs(),
        });
        (center, major_axis_end, entity)
    }

    /// An elliptic arc: the ellipse's three fields, then its ends.
    #[cfg(feature = "conics")]
    #[allow(clippy::too_many_arguments)]
    pub fn add_arc_of_ellipse(
        &mut self,
        cx: f64,
        cy: f64,
        mx: f64,
        my: f64,
        minor_radius: f64,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
    ) -> (PointId, PointId, PointId, PointId, EntityId) {
        let center = self.add_point(Point::new(cx, cy));
        let major_axis_end = self.add_point(Point::new(mx, my));
        let start = self.add_point(Point::new(sx, sy));
        let end = self.add_point(Point::new(ex, ey));
        let entity = self.add_entity(Entity::ArcOfEllipse {
            center,
            major_axis_end,
            minor_radius: minor_radius.abs(),
            start,
            end,
        });
        (center, major_axis_end, start, end, entity)
    }

    #[cfg(feature = "conics")]
    pub fn add_bspline(
        &mut self,
        control_points: Vec<PointId>,
        knots: Vec<f64>,
        degree: usize,
        periodic: bool,
    ) -> EntityId {
        self.add_entity(Entity::BSpline {
            control_points,
            knots,
            degree,
            periodic,
        })
    }
}
