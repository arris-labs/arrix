//! `Constraint::Block`: pin every degree of freedom of one entity to the pose
//! [`System::build`] froze into a [`BlockedTarget`].
//!
//! The row count belongs to the *entity* (`constraint_residual_count`), so
//! every path that cannot produce real rows still pays that many zeros.

use super::*;

pub(super) fn block(
    sketch: &Sketch,
    entity: EntityId,
    blocked_targets: Option<&BlockedTargets>,
    out: &mut ConstraintEval<'_>,
) {
    let count = constraint_residual_count(&Constraint::Block { entity }, sketch);
    let Some(target) = blocked_targets.and_then(|bt| bt.get(&entity)) else {
        out.zeros(count);
        return;
    };
    match target {
        BlockedTarget::Point { x, y } => match sketch.entities.get(&entity) {
            Some(Entity::Point(p)) => pin_point(sketch, *p, (*x, *y), out),
            _ => out.zeros(count),
        },
        BlockedTarget::Line { x1, y1, x2, y2 } => {
            match sketch.entities.get(&entity).map(|e| e.line_ends()) {
                Some(Some((start, end))) => {
                    pin_point(sketch, start, (*x1, *y1), out);
                    pin_point(sketch, end, (*x2, *y2), out);
                }
                _ => out.zeros(count),
            }
        }
        BlockedTarget::Circle { cx, cy, radius } => match sketch.entities.get(&entity) {
            Some(Entity::Circle { center, radius: r }) => {
                pin_point(sketch, *center, (*cx, *cy), out);
                out.row(r - radius, |g| g.radius_var(entity, 1.0));
            }
            _ => out.zeros(count),
        },
        BlockedTarget::Arc {
            cx,
            cy,
            sx,
            sy,
            ex,
            ey,
        } => match sketch.entities.get(&entity) {
            Some(Entity::Arc { center, start, end }) => {
                pin_point(sketch, *center, (*cx, *cy), out);
                pin_point(sketch, *start, (*sx, *sy), out);
                pin_point(sketch, *end, (*ex, *ey), out);
            }
            _ => out.zeros(count),
        },
        #[cfg(feature = "conics")]
        BlockedTarget::Ellipse {
            cx,
            cy,
            mx,
            my,
            minor_radius,
        } => match entity_ellipse_info(sketch, entity) {
            Some((c_id, m_id, b)) => {
                pin_point(sketch, c_id, (*cx, *cy), out);
                pin_point(sketch, m_id, (*mx, *my), out);
                out.row(b - minor_radius, |g| g.minor_radius_var(entity, 1.0));
            }
            None => out.zeros(count),
        },
        #[cfg(feature = "conics")]
        BlockedTarget::ArcOfEllipse {
            cx,
            cy,
            mx,
            my,
            minor_radius,
            sx,
            sy,
            ex,
            ey,
        } => match elliptic_arc_points(sketch, entity) {
            Some((c_id, m_id, b, start, end)) => {
                pin_point(sketch, c_id, (*cx, *cy), out);
                pin_point(sketch, m_id, (*mx, *my), out);
                out.row(b - minor_radius, |g| g.minor_radius_var(entity, 1.0));
                pin_point(sketch, start, (*sx, *sy), out);
                pin_point(sketch, end, (*ex, *ey), out);
            }
            None => out.zeros(count),
        },
        #[cfg(feature = "conics")]
        BlockedTarget::BSpline {
            control_points: target_pts,
        } => match entity_bspline_info(sketch, entity) {
            // Frozen alongside the entity, so the two lists are the same
            // length; `zip` is the belt-and-braces.
            Some((cps, _, _, _)) => {
                for (&p_id, &(tx, ty)) in cps.iter().zip(target_pts) {
                    pin_point(sketch, p_id, (tx, ty), out);
                }
            }
            None => out.zeros(count),
        },
    }
}

/// Two rows pinning one point to a frozen position.
fn pin_point(sketch: &Sketch, p: PointId, (x, y): (f64, f64), out: &mut ConstraintEval<'_>) {
    let pt = point(sketch, p);
    out.row(pt.x - x, |g| g.pt(p, 1.0, 0.0));
    out.row(pt.y - y, |g| g.pt(p, 0.0, 1.0));
}

/// Centre, major-axis end, minor radius and the two sweep endpoints of an
/// elliptic arc.
#[cfg(feature = "conics")]
fn elliptic_arc_points(
    sketch: &Sketch,
    entity: EntityId,
) -> Option<(PointId, PointId, f64, PointId, PointId)> {
    match sketch.entities.get(&entity)? {
        Entity::ArcOfEllipse {
            center,
            major_axis_end,
            minor_radius,
            start,
            end,
        } => Some((*center, *major_axis_end, *minor_radius, *start, *end)),
        _ => None,
    }
}
