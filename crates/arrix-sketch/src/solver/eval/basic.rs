//! Coincidence, alignment, tangency and mirror kinds — everything whose
//! residual is pure geometry with no dimension value attached.

use super::*;

pub(super) fn coincident(sketch: &Sketch, a: PointId, b: PointId, out: &mut ConstraintEval<'_>) {
    let pa = point(sketch, a);
    let pb = point(sketch, b);
    out.row(pa.x - pb.x, |g| {
        g.pt(a, 1.0, 0.0);
        g.pt(b, -1.0, 0.0);
    });
    out.row(pa.y - pb.y, |g| {
        g.pt(a, 0.0, 1.0);
        g.pt(b, 0.0, -1.0);
    });
}

pub(super) fn horizontal(sketch: &Sketch, line: EntityId, out: &mut ConstraintEval<'_>) {
    let Some((start, end)) = sketch.entities.get(&line).and_then(|e| e.line_ends()) else {
        out.zeros(1);
        return;
    };
    let dy = point(sketch, end).y - point(sketch, start).y;
    out.row(dy, |g| {
        g.pt(start, 0.0, -1.0);
        g.pt(end, 0.0, 1.0);
    });
}

pub(super) fn vertical(sketch: &Sketch, line: EntityId, out: &mut ConstraintEval<'_>) {
    let Some((start, end)) = sketch.entities.get(&line).and_then(|e| e.line_ends()) else {
        out.zeros(1);
        return;
    };
    let dx = point(sketch, end).x - point(sketch, start).x;
    out.row(dx, |g| {
        g.pt(start, -1.0, 0.0);
        g.pt(end, 1.0, 0.0);
    });
}

pub(super) fn horizontal_points(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let dy = point(sketch, a).y - point(sketch, b).y;
    out.row(dy, |g| {
        g.pt(a, 0.0, 1.0);
        g.pt(b, 0.0, -1.0);
    });
}

pub(super) fn vertical_points(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let dx = point(sketch, a).x - point(sketch, b).x;
    out.row(dx, |g| {
        g.pt(a, 1.0, 0.0);
        g.pt(b, -1.0, 0.0);
    });
}

pub(super) fn parallel(sketch: &Sketch, a: EntityId, b: EntityId, out: &mut ConstraintEval<'_>) {
    let Some((ends_a, ends_b, da, db)) = line_pair_dirs(sketch, a, b) else {
        out.zeros(1);
        return;
    };
    // Angle error to nearest parallel direction (0 or ±π).
    let theta = signed_angle(da, db);
    let dot = da.0 * db.0 + da.1 * db.1;
    let angle_err = if dot >= 0.0 {
        theta
    } else if theta > 0.0 {
        theta - std::f64::consts::PI
    } else {
        theta + std::f64::consts::PI
    };
    out.row(angle_err, |g| {
        push_angle_grad(g, ends_a, ends_b, da, db, 1.0);
    });
}

pub(super) fn perpendicular(
    sketch: &Sketch,
    a: EntityId,
    b: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let Some((ends_a, ends_b, da, db)) = line_pair_dirs(sketch, a, b) else {
        out.zeros(1);
        return;
    };
    let theta = signed_angle(da, db);
    let angle_err = if theta >= 0.0 {
        theta - std::f64::consts::FRAC_PI_2
    } else {
        theta + std::f64::consts::FRAC_PI_2
    };
    out.row(angle_err, |g| {
        push_angle_grad(g, ends_a, ends_b, da, db, 1.0);
    });
}

pub(super) fn equal(sketch: &Sketch, a: EntityId, b: EntityId, out: &mut ConstraintEval<'_>) {
    // Two lines compare lengths; anything else compares radii.
    if let Some((ends_a, ends_b, da, db)) = line_pair_dirs(sketch, a, b) {
        let la = (da.0 * da.0 + da.1 * da.1).sqrt();
        let lb = (db.0 * db.0 + db.1 * db.1).sqrt();
        out.row(la - lb, |g| {
            if la >= LENGTH_TOLERANCE {
                let (ux, uy) = (da.0 / la, da.1 / la);
                g.pt(ends_a.1, ux, uy);
                g.pt(ends_a.0, -ux, -uy);
            }
            if lb >= LENGTH_TOLERANCE {
                let (ux, uy) = (db.0 / lb, db.1 / lb);
                g.pt(ends_b.1, -ux, -uy);
                g.pt(ends_b.0, ux, uy);
            }
        });
        return;
    }
    let ra = entity_radius(sketch, a).unwrap_or(0.0);
    let rb = entity_radius(sketch, b).unwrap_or(0.0);
    out.row(ra - rb, |g| {
        g.entity_radius(sketch, a, 1.0);
        g.entity_radius(sketch, b, -1.0);
    });
}

pub(super) fn concentric(sketch: &Sketch, a: EntityId, b: EntityId, out: &mut ConstraintEval<'_>) {
    let (Some(ca), Some(cb)) = (entity_center(sketch, a), entity_center(sketch, b)) else {
        out.zeros(2);
        return;
    };
    let pa = point(sketch, ca);
    let pb = point(sketch, cb);
    out.row(pa.x - pb.x, |g| {
        g.pt(ca, 1.0, 0.0);
        g.pt(cb, -1.0, 0.0);
    });
    out.row(pa.y - pb.y, |g| {
        g.pt(ca, 0.0, 1.0);
        g.pt(cb, 0.0, -1.0);
    });
}

pub(super) fn tangent(
    sketch: &Sketch,
    line: EntityId,
    circle: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let ends = sketch.entities.get(&line).and_then(|e| e.line_ends());
    let (Some((start, end)), Some(center), Some(radius)) = (
        ends,
        entity_center(sketch, circle),
        entity_radius(sketch, circle),
    ) else {
        out.zeros(1);
        return;
    };
    if let Some(joint) = shared_endpoint(sketch, (start, end), circle) {
        tangent_at_joint(sketch, (start, end), center, joint, out);
        return;
    }
    let signed = point_line_signed_distance(sketch, center, line).unwrap_or(0.0);
    out.row(signed.abs() - radius, |g| {
        g.signed_dist(sketch, center, start, end, signed.signum());
        g.entity_radius(sketch, circle, -1.0);
    });
}

/// The line end that is also an end of the arc, by identity. A circle has
/// no ends, so it never has one.
fn shared_endpoint(
    sketch: &Sketch,
    (start, end): (PointId, PointId),
    circle: EntityId,
) -> Option<PointId> {
    let (_, a0, a1) = arc_points(sketch, circle)?;
    [start, end].into_iter().find(|&p| p == a0 || p == a1)
}

/// `Tangent` where the line and the arc meet at `joint`: the radius to the
/// joint is perpendicular to the line, `(e − c)·d̂`. `|dist| − r` is
/// `r·(1 − cos θ)` in the joint angle there, second order, so its gradient
/// vanishes at tangency and the joint pins nothing.
/// This row is first order; with `e` on both curves it holds exactly when
/// `|dist| = r` does.
fn tangent_at_joint(
    sketch: &Sketch,
    (start, end): (PointId, PointId),
    center: PointId,
    joint: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let (a, b) = (point(sketch, start), point(sketch, end));
    let (e, c) = (point(sketch, joint), point(sketch, center));
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let l = vx.hypot(vy);
    if l < LENGTH_TOLERANCE {
        out.zeros(1);
        return;
    }
    let (dx, dy) = (vx / l, vy / l);
    let (ux, uy) = (e.x - c.x, e.y - c.y);
    let f = ux * dx + uy * dy;
    out.row(f, |g| {
        g.pt(joint, dx, dy);
        g.pt(center, -dx, -dy);
        // ∂f/∂v = (u − f·d̂) / |v|, with v = end − start.
        let (wx, wy) = ((ux - f * dx) / l, (uy - f * dy) / l);
        g.pt(end, wx, wy);
        g.pt(start, -wx, -wy);
    });
}

pub(super) fn tangent_circles(
    sketch: &Sketch,
    a: EntityId,
    b: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let (Some(ca), Some(cb), Some(ra), Some(rb)) = (
        entity_center(sketch, a),
        entity_center(sketch, b),
        entity_radius(sketch, a),
        entity_radius(sketch, b),
    ) else {
        out.zeros(1);
        return;
    };
    let pa = point(sketch, ca);
    let pb = point(sketch, cb);
    let (dx, dy) = (pa.x - pb.x, pa.y - pb.y);
    let d = (dx * dx + dy * dy).sqrt();
    // Internal tangency (one circle inside the other) whenever it is the
    // closer of the two configurations.
    let external = d - (ra + rb);
    let internal = d - (ra - rb).abs();
    let is_internal = internal.abs() < external.abs();
    out.row(if is_internal { internal } else { external }, |g| {
        if d >= LENGTH_TOLERANCE {
            let (ux, uy) = (dx / d, dy / d);
            g.pt(ca, ux, uy);
            g.pt(cb, -ux, -uy);
        }
        if is_internal {
            let sigma = (ra - rb).signum();
            g.entity_radius(sketch, a, -sigma);
            g.entity_radius(sketch, b, sigma);
        } else {
            g.entity_radius(sketch, a, -1.0);
            g.entity_radius(sketch, b, -1.0);
        }
    });
}

pub(super) fn symmetric(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    mirror: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    // Reflect `a` across the mirror line and demand it equals `b`.
    let Some((start, end)) = sketch.entities.get(&mirror).and_then(|e| e.line_ends()) else {
        out.zeros(2);
        return;
    };
    let pa = point(sketch, a);
    let pb = point(sketch, b);
    let ms = point(sketch, start);
    let me = point(sketch, end);
    let (rx, ry) = reflect(pa.x, pa.y, ms.x, ms.y, me.x, me.y);
    let (dr_dp, dr_da, dr_db) = reflect_grad_matrices(pa.x, pa.y, ms.x, ms.y, me.x, me.y);
    out.row(rx - pb.x, |g| {
        g.pt(a, dr_dp.0.0, dr_dp.0.1);
        g.pt(start, dr_da.0.0, dr_da.0.1);
        g.pt(end, dr_db.0.0, dr_db.0.1);
        g.pt(b, -1.0, 0.0);
    });
    out.row(ry - pb.y, |g| {
        g.pt(a, dr_dp.1.0, dr_dp.1.1);
        g.pt(start, dr_da.1.0, dr_da.1.1);
        g.pt(end, dr_db.1.0, dr_db.1.1);
        g.pt(b, 0.0, -1.0);
    });
}

pub(super) fn symmetric_points(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    center: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let pa = point(sketch, a);
    let pb = point(sketch, b);
    let pc = point(sketch, center);
    out.row(pa.x + pb.x - 2.0 * pc.x, |g| {
        g.pt(a, 1.0, 0.0);
        g.pt(b, 1.0, 0.0);
        g.pt(center, -2.0, 0.0);
    });
    out.row(pa.y + pb.y - 2.0 * pc.y, |g| {
        g.pt(a, 0.0, 1.0);
        g.pt(b, 0.0, 1.0);
        g.pt(center, 0.0, -2.0);
    });
}

pub(super) fn point_on_perp_bisector(
    sketch: &Sketch,
    p: PointId,
    a: PointId,
    b: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let pt = point(sketch, p);
    let pa = point(sketch, a);
    let pb = point(sketch, b);
    let da2 = (pt.x - pa.x).powi(2) + (pt.y - pa.y).powi(2);
    let db2 = (pt.x - pb.x).powi(2) + (pt.y - pb.y).powi(2);
    let num = da2 - db2;
    let (dx, dy) = (pa.x - pb.x, pa.y - pb.y);
    let lab = (dx * dx + dy * dy).sqrt();
    out.row(num / lab.max(LENGTH_TOLERANCE), |g| {
        if lab >= LENGTH_TOLERANCE {
            let inv_lab = 1.0 / lab;
            let uab_x = dx * inv_lab;
            let uab_y = dy * inv_lab;

            // dN/dP = 2(B - A) => d(N/L)/dP = -2 * u_AB
            g.pt(p, -2.0 * uab_x, -2.0 * uab_y);

            // dN/dA = -2(P - A), dL/dA = u_AB
            let scale_l = num * inv_lab * inv_lab;
            g.pt(
                a,
                -2.0 * (pt.x - pa.x) * inv_lab - scale_l * uab_x,
                -2.0 * (pt.y - pa.y) * inv_lab - scale_l * uab_y,
            );

            // dN/dB = +2(P - B), dL/dB = -u_AB
            g.pt(
                b,
                2.0 * (pt.x - pb.x) * inv_lab + scale_l * uab_x,
                2.0 * (pt.y - pb.y) * inv_lab + scale_l * uab_y,
            );
        } else {
            let inv_lab = 1.0 / LENGTH_TOLERANCE;
            g.pt(
                p,
                2.0 * (pb.x - pa.x) * inv_lab,
                2.0 * (pb.y - pa.y) * inv_lab,
            );
            g.pt(
                a,
                -2.0 * (pt.x - pa.x) * inv_lab,
                -2.0 * (pt.y - pa.y) * inv_lab,
            );
            g.pt(
                b,
                2.0 * (pt.x - pb.x) * inv_lab,
                2.0 * (pt.y - pb.y) * inv_lab,
            );
        }
    });
}

pub(super) fn point_on_line(
    sketch: &Sketch,
    p: PointId,
    line: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let Some((start, end)) = sketch.entities.get(&line).and_then(|e| e.line_ends()) else {
        out.zeros(1);
        return;
    };
    let signed = point_line_signed_distance(sketch, p, line).unwrap_or(0.0);
    out.row(signed, |g| g.signed_dist(sketch, p, start, end, 1.0));
}

pub(super) fn point_on_circle(
    sketch: &Sketch,
    p: PointId,
    circle: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let (Some(center), Some(radius)) =
        (entity_center(sketch, circle), entity_radius(sketch, circle))
    else {
        out.zeros(1);
        return;
    };
    let c = point(sketch, center);
    let pt = point(sketch, p);
    let (dx, dy) = (pt.x - c.x, pt.y - c.y);
    let d = (dx * dx + dy * dy).sqrt();
    out.row(d - radius, |g| {
        if d >= LENGTH_TOLERANCE {
            let (ux, uy) = (dx / d, dy / d);
            g.pt(p, ux, uy);
            g.pt(center, -ux, -uy);
        }
        g.entity_radius(sketch, circle, -1.0);
    });
}

pub(super) fn midpoint(sketch: &Sketch, p: PointId, line: EntityId, out: &mut ConstraintEval<'_>) {
    let Some((start, end)) = sketch.entities.get(&line).and_then(|e| e.line_ends()) else {
        out.zeros(2);
        return;
    };
    let a = point(sketch, start);
    let b = point(sketch, end);
    let mp = point(sketch, p);
    out.row(mp.x - 0.5 * (a.x + b.x), |g| {
        g.pt(p, 1.0, 0.0);
        g.pt(start, -0.5, 0.0);
        g.pt(end, -0.5, 0.0);
    });
    out.row(mp.y - 0.5 * (a.y + b.y), |g| {
        g.pt(p, 0.0, 1.0);
        g.pt(start, 0.0, -0.5);
        g.pt(end, 0.0, -0.5);
    });
}

pub(super) fn fix(sketch: &Sketch, p: PointId, x: f64, y: f64, out: &mut ConstraintEval<'_>) {
    let pt = point(sketch, p);
    out.row(pt.x - x, |g| g.pt(p, 1.0, 0.0));
    out.row(pt.y - y, |g| g.pt(p, 0.0, 1.0));
}

#[cfg(feature = "snells-law")]
/// Unit normal of the refracting boundary at the vertex `v`.
fn boundary_normal(sketch: &Sketch, boundary: EntityId, v: (f64, f64)) -> (f64, f64) {
    match sketch.entities.get(&boundary) {
        Some(Entity::Line { start, end }) => {
            let a = point(sketch, *start);
            let b = point(sketch, *end);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt().max(LENGTH_TOLERANCE);
            (-dy / len, dx / len)
        }
        Some(Entity::Circle { center, .. }) | Some(Entity::Arc { center, .. }) => {
            let c = point(sketch, *center);
            let (dx, dy) = (v.0 - c.x, v.1 - c.y);
            let len = (dx * dx + dy * dy).sqrt().max(LENGTH_TOLERANCE);
            (dx / len, dy / len)
        }
        _ => (0.0, 1.0),
    }
}

#[cfg(feature = "snells-law")]
#[allow(clippy::too_many_arguments)]
pub(super) fn snells_law(
    sketch: &Sketch,
    ray1_start: PointId,
    ray1_end: PointId,
    ray2_end: PointId,
    boundary: EntityId,
    ratio: f64,
    out: &mut ConstraintEval<'_>,
) {
    let s1 = point(sketch, ray1_start);
    let v = point(sketch, ray1_end); // vertex
    let e2 = point(sketch, ray2_end);

    let (r1x, r1y) = (v.x - s1.x, v.y - s1.y);
    let l1 = (r1x * r1x + r1y * r1y).sqrt();
    let (r2x, r2y) = (e2.x - v.x, e2.y - v.y);
    let l2 = (r2x * r2x + r2y * r2y).sqrt();

    let (nx, ny) = boundary_normal(sketch, boundary, (v.x, v.y));

    // sin(theta) = u cross N; the residual floors the ray lengths, the
    // gradient zeroes the direction outright, both at LENGTH_TOLERANCE.
    let sin = |rx: f64, ry: f64, l: f64| {
        let l = l.max(LENGTH_TOLERANCE);
        (rx / l) * ny - (ry / l) * nx
    };
    out.measure_ratio(sin(r1x, r1y, l1), sin(r2x, r2y, l2));
    out.row(sin(r1x, r1y, l1) - ratio * sin(r2x, r2y, l2), |g| {
        let d_sin1_dr1 = unit_vec_grad(ny, -nx, r1x, r1y);
        g.pt(ray1_end, d_sin1_dr1.0, d_sin1_dr1.1);
        g.pt(ray1_start, -d_sin1_dr1.0, -d_sin1_dr1.1);

        let d_sin2_dr2 = unit_vec_grad(-ratio * ny, ratio * nx, r2x, r2y);
        g.pt(ray2_end, d_sin2_dr2.0, d_sin2_dr2.1);
        g.pt(ray1_end, -d_sin2_dr2.0, -d_sin2_dr2.1);

        let unit = |rx: f64, ry: f64, l: f64| {
            if l >= LENGTH_TOLERANCE {
                (rx / l, ry / l)
            } else {
                (0.0, 0.0)
            }
        };
        let (u1x, u1y) = unit(r1x, r1y, l1);
        let (u2x, u2y) = unit(r2x, r2y, l2);
        let gnx = -u1y + ratio * u2y;
        let gny = u1x - ratio * u2x;

        match sketch.entities.get(&boundary) {
            Some(Entity::Line { start, end }) => {
                let a = point(sketch, *start);
                let b = point(sketch, *end);
                let d_res_dd = unit_vec_grad(gny, -gnx, b.x - a.x, b.y - a.y);
                g.pt(*end, d_res_dd.0, d_res_dd.1);
                g.pt(*start, -d_res_dd.0, -d_res_dd.1);
            }
            Some(Entity::Circle { center, .. }) | Some(Entity::Arc { center, .. }) => {
                let c = point(sketch, *center);
                let d_res_dd = unit_vec_grad(gnx, gny, v.x - c.x, v.y - c.y);
                g.pt(ray1_end, d_res_dd.0, d_res_dd.1);
                g.pt(*center, -d_res_dd.0, -d_res_dd.1);
            }
            _ => {}
        }
    });
}
