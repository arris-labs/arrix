//! The packed variable/residual system: which sketch DoF become solver
//! variables, how a `Sketch` packs into and unpacks out of `x`, the
//! constraint-graph partition into independent subsystems, and the small
//! accessors the [`super::eval`] evaluators read geometry through.

use arrix_core::LENGTH_TOLERANCE;

use crate::constraint::{Constraint, ConstraintPriority};
use crate::dsu::Dsu;
use crate::entity::Entity;
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

use super::eval::{ConstraintEval, eval_constraint};
use super::*;

#[derive(Debug, Clone)]
pub struct System {
    pub vars: Vec<Var>,
    pub point_col: std::collections::BTreeMap<PointId, usize>,
    pub radius_col: std::collections::BTreeMap<EntityId, usize>,
    pub minor_radius_col: std::collections::BTreeMap<EntityId, usize>,
    /// Who each residual row belongs to, in residual order.
    pub residual_owners: Vec<ResidualOwner>,
    pub blocked_targets: std::collections::BTreeMap<EntityId, BlockedTarget>,
    /// Constraint priority for each residual row.
    pub residual_priorities: Vec<ConstraintPriority>,
    /// Active driving constraint IDs included in this system.
    pub constraint_ids: Vec<ConstraintId>,
    /// Arc entities with implicit equal-radii residual included in this system.
    pub arc_entity_ids: Vec<EntityId>,
    /// Drag pins, soft rows after the arcs': two for a point, one for a
    /// radius. Empty outside a drag.
    pub pins: Vec<Pin>,
}

/// What a drag holds toward its target: a residual block of its own, never
/// a stored constraint, so a drag frame spends no `ConstraintId` and
/// rebuilds nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pin {
    /// A point held toward `(x, y)`: two rows.
    Point { point: PointId, x: f64, y: f64 },
    /// A circle's radius held toward `value`: one row. A rim drag pulls
    /// this, and nothing else.
    Radius { circle: EntityId, value: f64 },
}

impl Pin {
    /// How many rows the pin adds.
    pub(crate) fn rows(&self) -> usize {
        match self {
            Self::Point { .. } => 2,
            Self::Radius { .. } => 1,
        }
    }

    /// The pin's first variable column in `sys`, or `None` when what it
    /// pins is not a variable there (a fixed point).
    pub(crate) fn col(&self, sys: &System) -> Option<usize> {
        match self {
            Self::Point { point, .. } => sys.point_col.get(point).copied(),
            Self::Radius { circle, .. } => sys.radius_col.get(circle).copied(),
        }
    }

    fn owner(&self, slot: usize) -> ResidualOwner {
        match *self {
            Self::Point { point, .. } => ResidualOwner::Pin { point, slot },
            Self::Radius { circle, .. } => ResidualOwner::RadiusPin(circle),
        }
    }
}

/// A blocked entity's frozen pose, one arm per entity.
fn blocked_target(sketch: &Sketch, entity: &Entity) -> Option<BlockedTarget> {
    let pt = |id: &PointId| sketch.points.get(id);
    match entity {
        Entity::Point(point) => {
            let p = pt(point)?;
            Some(BlockedTarget::Point { x: p.x, y: p.y })
        }
        Entity::Line { start, end } => {
            let (s, e) = (pt(start)?, pt(end)?);
            Some(BlockedTarget::Line {
                x1: s.x,
                y1: s.y,
                x2: e.x,
                y2: e.y,
            })
        }
        Entity::Circle { center, radius } => {
            let c = pt(center)?;
            Some(BlockedTarget::Circle {
                cx: c.x,
                cy: c.y,
                radius: *radius,
            })
        }
        Entity::Arc { center, start, end } => {
            let (c, s, e) = (pt(center)?, pt(start)?, pt(end)?);
            Some(BlockedTarget::Arc {
                cx: c.x,
                cy: c.y,
                sx: s.x,
                sy: s.y,
                ex: e.x,
                ey: e.y,
            })
        }
        #[cfg(feature = "conics")]
        Entity::Ellipse {
            center,
            major_axis_end,
            minor_radius,
        } => {
            let (c, m) = (pt(center)?, pt(major_axis_end)?);
            Some(BlockedTarget::Ellipse {
                cx: c.x,
                cy: c.y,
                mx: m.x,
                my: m.y,
                minor_radius: *minor_radius,
            })
        }
        #[cfg(feature = "conics")]
        Entity::ArcOfEllipse {
            center,
            major_axis_end,
            minor_radius,
            start,
            end,
        } => {
            let (c, m, s, e) = (pt(center)?, pt(major_axis_end)?, pt(start)?, pt(end)?);
            Some(BlockedTarget::ArcOfEllipse {
                cx: c.x,
                cy: c.y,
                mx: m.x,
                my: m.y,
                minor_radius: *minor_radius,
                sx: s.x,
                sy: s.y,
                ex: e.x,
                ey: e.y,
            })
        }
        #[cfg(feature = "conics")]
        Entity::BSpline { control_points, .. } => Some(BlockedTarget::BSpline {
            control_points: control_points
                .iter()
                .filter_map(|p| pt(p).map(|q| (q.x, q.y)))
                .collect(),
        }),
    }
}

impl System {
    pub fn build(sketch: &Sketch) -> Self {
        Self::build_with_pins(sketch, &[])
    }

    /// [`System::build`] plus a drag-priority residual block per pin whose
    /// point or radius is free; a pin on a fixed point has nothing to pull.
    pub fn build_with_pins(sketch: &Sketch, pins: &[Pin]) -> Self {
        let mut vars = Vec::new();
        let mut point_col = std::collections::BTreeMap::new();
        for (id, point) in &sketch.points {
            if !point.fixed {
                point_col.insert(*id, vars.len());
                vars.push(Var::PointX(*id));
                vars.push(Var::PointY(*id));
            }
        }
        let mut radius_col = std::collections::BTreeMap::new();
        // Only a *native* ellipse has a free minor radius; without `conics`
        // there is no such entity, so this map stays empty.
        #[cfg_attr(not(feature = "conics"), allow(unused_mut))]
        let mut minor_radius_col = std::collections::BTreeMap::new();
        for (id, entity) in &sketch.entities {
            match entity {
                Entity::Circle { .. } => {
                    radius_col.insert(*id, vars.len());
                    vars.push(Var::CircleRadius(*id));
                }
                #[cfg(feature = "conics")]
                Entity::Ellipse { .. } | Entity::ArcOfEllipse { .. } => {
                    minor_radius_col.insert(*id, vars.len());
                    vars.push(Var::EllipseMinorRadius(*id));
                }
                _ => {}
            }
        }

        let mut blocked_targets = std::collections::BTreeMap::new();
        for record in sketch.constraints.values() {
            if !record.is_active || !record.is_driving {
                continue;
            }
            if let Constraint::Block { entity } = &record.constraint
                && let Some(ent) = sketch.entities.get(entity)
                && let Some(target) = blocked_target(sketch, ent)
            {
                blocked_targets.insert(*entity, target);
            }
        }

        let mut residual_owners = Vec::new();
        let mut residual_priorities = Vec::new();
        let mut constraint_ids = Vec::new();
        for (cid, record) in &sketch.constraints {
            if !record.is_active || !record.is_driving {
                continue;
            }
            constraint_ids.push(*cid);
            let count = constraint_residual_count(&record.constraint, sketch);
            for slot in 0..count {
                residual_owners.push(ResidualOwner::Constraint { id: *cid, slot });
                residual_priorities.push(record.priority);
            }
        }
        let mut arc_entity_ids = Vec::new();
        // Implicit equal-radii residual for every arc.
        for (eid, entity) in &sketch.entities {
            if matches!(entity, Entity::Arc { .. }) {
                arc_entity_ids.push(*eid);
                residual_owners.push(ResidualOwner::ArcRadii(*eid));
                residual_priorities.push(ConstraintPriority::Primary);
            }
        }
        let pins: Vec<Pin> = pins
            .iter()
            .filter(|pin| match pin {
                Pin::Point { point, .. } => point_col.contains_key(point),
                Pin::Radius { circle, .. } => radius_col.contains_key(circle),
            })
            .copied()
            .collect();
        for pin in &pins {
            for slot in 0..pin.rows() {
                residual_owners.push(pin.owner(slot));
                residual_priorities.push(ConstraintPriority::Drag);
            }
        }

        Self {
            vars,
            point_col,
            radius_col,
            minor_radius_col,
            residual_owners,
            blocked_targets,
            residual_priorities,
            constraint_ids,
            arc_entity_ids,
            pins,
        }
    }

    /// Moves every pin on `point` to `(x, y)`; the rows stay where they are.
    pub fn set_pin_target(&mut self, point: PointId, x: f64, y: f64) {
        for pin in &mut self.pins {
            if let Pin::Point {
                point: p,
                x: px,
                y: py,
            } = pin
                && *p == point
            {
                (*px, *py) = (x, y);
            }
        }
    }

    /// Moves every radius pin on `circle` to `value`.
    pub fn set_radius_target(&mut self, circle: EntityId, value: f64) {
        for pin in &mut self.pins {
            if let Pin::Radius {
                circle: c,
                value: v,
            } = pin
                && *c == circle
            {
                *v = value;
            }
        }
    }

    /// The same variables with only the primary rows: no pins, no
    /// drag-priority constraints.
    pub fn primary_only(&self, sketch: &Sketch) -> System {
        let cids: Vec<ConstraintId> = self
            .constraint_ids
            .iter()
            .copied()
            .filter(|cid| {
                sketch
                    .constraints
                    .get(cid)
                    .is_some_and(|r| r.priority == ConstraintPriority::Primary)
            })
            .collect();
        let (residual_owners, residual_priorities, blocked_targets) =
            self.subsystem_rows(sketch, &cids, &self.arc_entity_ids, &[]);
        System {
            vars: self.vars.clone(),
            point_col: self.point_col.clone(),
            radius_col: self.radius_col.clone(),
            minor_radius_col: self.minor_radius_col.clone(),
            residual_owners,
            residual_priorities,
            blocked_targets,
            constraint_ids: cids,
            arc_entity_ids: self.arc_entity_ids.clone(),
            pins: Vec::new(),
        }
    }

    pub fn n_vars(&self) -> usize {
        self.vars.len()
    }

    pub fn n_residuals(&self) -> usize {
        self.residual_owners.len()
    }

    pub fn pack(&self, sketch: &Sketch) -> Vec<f64> {
        self.vars
            .iter()
            .map(|v| match *v {
                Var::PointX(id) => sketch.points[&id].x,
                Var::PointY(id) => sketch.points[&id].y,
                Var::CircleRadius(id) => match &sketch.entities[&id] {
                    Entity::Circle { radius, .. } => *radius,
                    _ => 0.0,
                },
                #[cfg(feature = "conics")]
                Var::EllipseMinorRadius(id) => match &sketch.entities[&id] {
                    Entity::Ellipse { minor_radius, .. }
                    | Entity::ArcOfEllipse { minor_radius, .. } => *minor_radius,
                    _ => 0.0,
                },
            })
            .collect()
    }

    pub fn unpack_into(&self, x: &[f64], sketch: &mut Sketch) {
        for (i, v) in self.vars.iter().enumerate() {
            set_var(sketch, *v, x[i]);
        }
    }

    pub fn residuals_of(&self, sketch: &Sketch) -> Vec<f64> {
        let mut eval = ConstraintEval::residuals_only();
        for &cid in &self.constraint_ids {
            if let Some(record) = sketch.constraints.get(&cid)
                && record.is_active
                && record.is_driving
            {
                eval_constraint(
                    sketch,
                    &record.constraint,
                    Some(&self.blocked_targets),
                    &mut eval,
                );
            }
        }
        let mut out = eval.take_residuals();
        for &eid in &self.arc_entity_ids {
            if let Some(Entity::Arc { center, start, end }) = sketch.entities.get(&eid) {
                let c = &sketch.points[center];
                let s = &sketch.points[start];
                let e = &sketch.points[end];
                let r_s = c.distance_to(s);
                let r_e = c.distance_to(e);
                out.push(r_s - r_e);
            }
        }
        for pin in &self.pins {
            match *pin {
                Pin::Point { point, x, y } => {
                    let p = &sketch.points[&point];
                    out.push(p.x - x);
                    out.push(p.y - y);
                }
                Pin::Radius { circle, value } => {
                    let radius = match sketch.entities.get(&circle) {
                        Some(Entity::Circle { radius, .. }) => *radius,
                        _ => value,
                    };
                    out.push(radius - value);
                }
            }
        }
        out
    }

    /// The residual owners, priorities and blocked targets one subsystem
    /// inherits from the constraints, arcs and pins assigned to it.
    #[allow(clippy::type_complexity)]
    fn subsystem_rows(
        &self,
        sketch: &Sketch,
        cids: &[ConstraintId],
        eids: &[EntityId],
        pins: &[Pin],
    ) -> (
        Vec<ResidualOwner>,
        Vec<ConstraintPriority>,
        std::collections::BTreeMap<EntityId, BlockedTarget>,
    ) {
        let mut owners = Vec::new();
        let mut priorities = Vec::new();
        let mut targets = std::collections::BTreeMap::new();
        for &cid in cids {
            if let Some(record) = sketch.constraints.get(&cid) {
                let count = constraint_residual_count(&record.constraint, sketch);
                for slot in 0..count {
                    owners.push(ResidualOwner::Constraint { id: cid, slot });
                    priorities.push(record.priority);
                }
                if let Constraint::Block { entity } = record.constraint
                    && let Some(target) = self.blocked_targets.get(&entity)
                {
                    targets.insert(entity, target.clone());
                }
            }
        }
        for &eid in eids {
            owners.push(ResidualOwner::ArcRadii(eid));
            priorities.push(ConstraintPriority::Primary);
        }
        for pin in pins {
            for slot in 0..pin.rows() {
                owners.push(pin.owner(slot));
                priorities.push(ConstraintPriority::Drag);
            }
        }
        (owners, priorities, targets)
    }

    /// Partitions the system into decoupled connected subsystems using a bipartite
    /// variable-constraint graph.
    pub fn partition(&self, sketch: &Sketch) -> Vec<System> {
        let n = self.n_vars();
        if n == 0 {
            return vec![self.clone()];
        }

        let mut dsu = Dsu::new();

        // Group X and Y variables of the same point together
        for &col in self.point_col.values() {
            if col + 1 < n {
                dsu.union(col, col + 1);
            }
        }

        let mut constraint_cols: std::collections::BTreeMap<ConstraintId, Vec<usize>> =
            std::collections::BTreeMap::new();
        // Which columns a constraint couples comes straight out of its
        // evaluator's sparsity mode — no throwaway Jacobian to learn it from.
        let mut sparsity = ConstraintEval::sparsity(self);
        for &cid in &self.constraint_ids {
            if let Some(record) = sketch.constraints.get(&cid) {
                eval_constraint(
                    sketch,
                    &record.constraint,
                    Some(&self.blocked_targets),
                    &mut sparsity,
                );
                let mut cols = sparsity.take_cols();
                cols.retain(|&col| col < n);
                cols.sort_unstable();
                cols.dedup();
                for w in cols.windows(2) {
                    dsu.union(w[0], w[1]);
                }
                constraint_cols.insert(cid, cols);
            }
        }

        let mut arc_cols: std::collections::BTreeMap<EntityId, Vec<usize>> =
            std::collections::BTreeMap::new();
        for &eid in &self.arc_entity_ids {
            if let Some(Entity::Arc { center, start, end }) = sketch.entities.get(&eid) {
                let mut cols = Vec::new();
                for p in [*center, *start, *end] {
                    if let Some(&c) = self.point_col.get(&p) {
                        cols.push(c);
                        cols.push(c + 1);
                    }
                }
                cols.sort_unstable();
                cols.dedup();
                for w in cols.windows(2) {
                    dsu.union(w[0], w[1]);
                }
                arc_cols.insert(eid, cols);
            }
        }

        // A pin's columns are its point's, already one component, or its
        // circle's one radius column.
        let mut component_pins: std::collections::BTreeMap<usize, Vec<Pin>> =
            std::collections::BTreeMap::new();
        for pin in &self.pins {
            if let Some(col) = pin.col(self) {
                let root = dsu.find(col);
                component_pins.entry(root).or_default().push(*pin);
            }
        }

        let mut component_vars: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for var_idx in 0..n {
            let root = dsu.find(var_idx);
            component_vars.entry(root).or_default().push(var_idx);
        }

        let mut unattached_constraints = Vec::new();
        let mut unattached_arcs = Vec::new();

        let mut component_constraints: std::collections::BTreeMap<usize, Vec<ConstraintId>> =
            std::collections::BTreeMap::new();
        for (&cid, cols) in &constraint_cols {
            if let Some(&first_col) = cols.first() {
                let root = dsu.find(first_col);
                component_constraints.entry(root).or_default().push(cid);
            } else {
                unattached_constraints.push(cid);
            }
        }

        let mut component_arcs: std::collections::BTreeMap<usize, Vec<EntityId>> =
            std::collections::BTreeMap::new();
        for (&eid, cols) in &arc_cols {
            if let Some(&first_col) = cols.first() {
                let root = dsu.find(first_col);
                component_arcs.entry(root).or_default().push(eid);
            } else {
                unattached_arcs.push(eid);
            }
        }

        if component_vars.len() <= 1
            && unattached_constraints.is_empty()
            && unattached_arcs.is_empty()
        {
            return vec![self.clone()];
        }

        let mut subsystems = Vec::new();

        for (&root, var_indices) in &component_vars {
            let mut sub_vars = Vec::with_capacity(var_indices.len());
            let mut sub_point_col = std::collections::BTreeMap::new();
            let mut sub_radius_col = std::collections::BTreeMap::new();
            #[cfg_attr(not(feature = "conics"), allow(unused_mut))]
            let mut sub_minor_radius_col = std::collections::BTreeMap::new();

            for (local_idx, &global_idx) in var_indices.iter().enumerate() {
                let v = self.vars[global_idx];
                sub_vars.push(v);
                match v {
                    Var::PointX(p) => {
                        sub_point_col.insert(p, local_idx);
                    }
                    Var::PointY(_) => {}
                    Var::CircleRadius(e) => {
                        sub_radius_col.insert(e, local_idx);
                    }
                    #[cfg(feature = "conics")]
                    Var::EllipseMinorRadius(e) => {
                        sub_minor_radius_col.insert(e, local_idx);
                    }
                }
            }

            let sub_cids = component_constraints.remove(&root).unwrap_or_default();
            let sub_eids = component_arcs.remove(&root).unwrap_or_default();
            let sub_pins = component_pins.remove(&root).unwrap_or_default();

            let (sub_residual_owners, sub_residual_priorities, sub_blocked_targets) =
                self.subsystem_rows(sketch, &sub_cids, &sub_eids, &sub_pins);

            subsystems.push(System {
                vars: sub_vars,
                point_col: sub_point_col,
                radius_col: sub_radius_col,
                minor_radius_col: sub_minor_radius_col,
                residual_owners: sub_residual_owners,
                residual_priorities: sub_residual_priorities,
                blocked_targets: sub_blocked_targets,
                constraint_ids: sub_cids,
                arc_entity_ids: sub_eids,
                pins: sub_pins,
            });
        }

        if !unattached_constraints.is_empty() || !unattached_arcs.is_empty() {
            let (sub_residual_owners, sub_residual_priorities, sub_blocked_targets) =
                self.subsystem_rows(sketch, &unattached_constraints, &unattached_arcs, &[]);

            subsystems.push(System {
                vars: Vec::new(),
                point_col: std::collections::BTreeMap::new(),
                radius_col: std::collections::BTreeMap::new(),
                minor_radius_col: std::collections::BTreeMap::new(),
                residual_owners: sub_residual_owners,
                residual_priorities: sub_residual_priorities,
                blocked_targets: sub_blocked_targets,
                constraint_ids: unattached_constraints,
                arc_entity_ids: unattached_arcs,
                pins: Vec::new(),
            });
        }

        subsystems
    }
}

pub fn set_var(sketch: &mut Sketch, v: Var, value: f64) {
    match v {
        Var::PointX(id) => {
            if let Some(p) = sketch.points.get_mut(&id) {
                p.x = value;
            }
        }
        Var::PointY(id) => {
            if let Some(p) = sketch.points.get_mut(&id) {
                p.y = value;
            }
        }
        Var::CircleRadius(id) => {
            if let Some(Entity::Circle { radius, .. }) = sketch.entities.get_mut(&id) {
                // Keep radius positive; a sign flip mid-solve is not
                // meaningful geometry.
                *radius = value.abs().max(LENGTH_TOLERANCE);
            }
        }
        #[cfg(feature = "conics")]
        Var::EllipseMinorRadius(id) => match sketch.entities.get_mut(&id) {
            Some(Entity::Ellipse { minor_radius, .. })
            | Some(Entity::ArcOfEllipse { minor_radius, .. }) => {
                *minor_radius = value.abs().max(LENGTH_TOLERANCE);
            }
            _ => {}
        },
    }
}

pub(crate) fn point(sketch: &Sketch, id: PointId) -> &crate::entity::Point {
    &sketch.points[&id]
}

pub(crate) fn reflect(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> (f64, f64) {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < LENGTH_TOLERANCE * LENGTH_TOLERANCE {
        return (px, py);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let qx = ax + t * dx;
    let qy = ay + t * dy;
    (2.0 * qx - px, 2.0 * qy - py)
}

pub(crate) fn point_line_signed_distance(
    sketch: &Sketch,
    p: PointId,
    line: EntityId,
) -> Option<f64> {
    let (start, end) = sketch.entities.get(&line)?.line_ends()?;
    let a = point(sketch, start);
    let b = point(sketch, end);
    let pt = point(sketch, p);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt().max(LENGTH_TOLERANCE);
    Some(((pt.x - a.x) * dy - (pt.y - a.y) * dx) / len)
}

pub(crate) fn entity_center(sketch: &Sketch, id: EntityId) -> Option<PointId> {
    sketch.entities.get(&id)?.circle_center()
}

pub(crate) fn entity_radius(sketch: &Sketch, id: EntityId) -> Option<f64> {
    match sketch.entities.get(&id)? {
        Entity::Circle { radius, .. } => Some(*radius),
        Entity::Arc { center, start, .. } => {
            let c = point(sketch, *center);
            let s = point(sketch, *start);
            Some(c.distance_to(s))
        }
        _ => None,
    }
}

#[cfg(feature = "conics")]
pub(crate) fn entity_ellipse_info(
    sketch: &Sketch,
    id: EntityId,
) -> Option<(PointId, PointId, f64)> {
    match sketch.entities.get(&id)? {
        #[cfg(feature = "conics")]
        Entity::Ellipse {
            center,
            major_axis_end,
            minor_radius,
        }
        | Entity::ArcOfEllipse {
            center,
            major_axis_end,
            minor_radius,
            ..
        } => Some((*center, *major_axis_end, *minor_radius)),
        _ => None,
    }
}

#[cfg(feature = "conics")]
pub(crate) fn entity_minor_radius(sketch: &Sketch, id: EntityId) -> Option<f64> {
    entity_ellipse_info(sketch, id).map(|(_, _, b)| b)
}

#[cfg(feature = "conics")]
pub(crate) fn entity_bspline_info(
    sketch: &Sketch,
    id: EntityId,
) -> Option<(&[PointId], &[f64], usize, bool)> {
    match sketch.entities.get(&id)? {
        #[cfg(feature = "conics")]
        Entity::BSpline {
            control_points,
            knots,
            degree,
            periodic,
        } => Some((
            control_points.as_slice(),
            knots.as_slice(),
            *degree,
            *periodic,
        )),
        _ => None,
    }
}

#[allow(clippy::type_complexity)]
#[cfg(feature = "conics")]
pub(crate) fn point_on_ellipse_eval(
    px: f64,
    py: f64,
    cx: f64,
    cy: f64,
    mx: f64,
    my: f64,
    b: f64,
) -> (f64, ((f64, f64), (f64, f64), (f64, f64), f64)) {
    let vx = mx - cx;
    let vy = my - cy;
    let s = (vx * vx + vy * vy).max(1e-12);
    let b_eff = b.abs().max(1e-6);
    let dx = px - cx;
    let dy = py - cy;
    let d1 = dx * vx + dy * vy;
    let d2 = dx * vy - dy * vx;
    let t1 = d1 / (s * s);
    let t2 = d2 / (s * b_eff * b_eff);
    let r = d1 * t1 + d2 * t2 - 1.0;

    let d_px = 2.0 * t1 * vx + 2.0 * t2 * vy;
    let d_py = 2.0 * t1 * vy - 2.0 * t2 * vx;

    let d_mx = 2.0 * t1 * dx
        - 4.0 * d1 * d1 * vx / (s * s * s)
        - 2.0 * t2 * dy
        - 2.0 * d2 * d2 * vx / (s * s * b_eff * b_eff);
    let d_my = 2.0 * t1 * dy - 4.0 * d1 * d1 * vy / (s * s * s) + 2.0 * t2 * dx
        - 2.0 * d2 * d2 * vy / (s * s * b_eff * b_eff);

    let d_cx = -d_px - d_mx;
    let d_cy = -d_py - d_my;
    let d_b = -2.0 * d2 * d2 / (s * b_eff * b_eff * b_eff);

    (r, ((d_px, d_py), (d_cx, d_cy), (d_mx, d_my), d_b))
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
#[cfg(feature = "conics")]
pub(crate) fn tangent_line_ellipse_eval(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    mx: f64,
    my: f64,
    b: f64,
) -> (f64, ((f64, f64), (f64, f64), (f64, f64), (f64, f64), f64)) {
    let del_x = bx - ax;
    let del_y = by - ay;
    let l = (del_x * del_x + del_y * del_y).sqrt().max(1e-12);
    let inv_l = 1.0 / l;

    let k = -(cx - ax) * del_y + (cy - ay) * del_x;
    let sigma = if k >= 0.0 { 1.0 } else { -1.0 };

    let vx = mx - cx;
    let vy = my - cy;
    let s = (vx * vx + vy * vy).max(1e-12);
    let b_eff = b.abs().max(1e-6);
    let b2_over_s = (b_eff * b_eff) / s;

    let n1 = -vx * del_y + vy * del_x;
    let n2 = vx * del_x + vy * del_y;
    let q_sq = n1 * n1 + b2_over_s * n2 * n2;
    let q = q_sq.sqrt().max(1e-12);

    let num = sigma * k - q;
    let r = num * inv_l;

    let dk_dc_x = -del_y;
    let dk_dc_y = del_x;
    let dk_da_x = by - cy;
    let dk_da_y = cx - bx;
    let dk_db_x = cy - ay;
    let dk_db_y = ax - cx;

    let dq_dn1 = n1 / q;
    let dq_dn2 = (b2_over_s * n2) / q;
    let dq_ds = (-b2_over_s * n2 * n2) / (2.0 * s * q);
    let dq_db = (b_eff * n2 * n2) / (s * q);

    let dq_dm_x = dq_dn1 * (-del_y) + dq_dn2 * del_x + dq_ds * 2.0 * vx;
    let dq_dm_y = dq_dn1 * del_x + dq_dn2 * del_y + dq_ds * 2.0 * vy;
    let dq_dc_x = -dq_dm_x;
    let dq_dc_y = -dq_dm_y;

    let dq_db_x = dq_dn1 * vy + dq_dn2 * vx;
    let dq_db_y = dq_dn1 * (-vx) + dq_dn2 * vy;
    let dq_da_x = -dq_db_x;
    let dq_da_y = -dq_db_y;

    let dl_db_x = del_x * inv_l;
    let dl_db_y = del_y * inv_l;
    let dl_da_x = -dl_db_x;
    let dl_da_y = -dl_db_y;

    let d_res = |d_num: f64, d_l: f64| (d_num - r * d_l) * inv_l;

    let d_cx = d_res(sigma * dk_dc_x - dq_dc_x, 0.0);
    let d_cy = d_res(sigma * dk_dc_y - dq_dc_y, 0.0);
    let d_mx = d_res(-dq_dm_x, 0.0);
    let d_my = d_res(-dq_dm_y, 0.0);
    let d_ax = d_res(sigma * dk_da_x - dq_da_x, dl_da_x);
    let d_ay = d_res(sigma * dk_da_y - dq_da_y, dl_da_y);
    let d_bx = d_res(sigma * dk_db_x - dq_db_x, dl_db_x);
    let d_by = d_res(sigma * dk_db_y - dq_db_y, dl_db_y);
    let d_b_val = d_res(-dq_db, 0.0);

    (
        r,
        (
            (d_ax, d_ay),
            (d_bx, d_by),
            (d_cx, d_cy),
            (d_mx, d_my),
            d_b_val,
        ),
    )
}
