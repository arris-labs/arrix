//! Dimensional kinds: every one of these measures something and compares it
//! against `value`, so every one calls [`ConstraintEval::measure`] with the
//! measurement its own residual used.

use super::*;

pub(super) fn distance(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let pa = point(sketch, a);
    let pb = point(sketch, b);
    let (dx, dy) = (pa.x - pb.x, pa.y - pb.y);
    let d = (dx * dx + dy * dy).sqrt();
    out.measure(d);
    out.row(d - value, |g| {
        if d >= LENGTH_TOLERANCE {
            let (ux, uy) = (dx / d, dy / d);
            g.pt(a, ux, uy);
            g.pt(b, -ux, -uy);
        }
    });
}

pub(super) fn horizontal_distance(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let dx = point(sketch, a).x - point(sketch, b).x;
    out.measure(dx.abs());
    out.row(dx.abs() - value, |g| {
        g.pt(a, dx.signum(), 0.0);
        g.pt(b, -dx.signum(), 0.0);
    });
}

pub(super) fn vertical_distance(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let dy = point(sketch, a).y - point(sketch, b).y;
    out.measure(dy.abs());
    out.row(dy.abs() - value, |g| {
        g.pt(a, 0.0, dy.signum());
        g.pt(b, 0.0, -dy.signum());
    });
}

pub(super) fn distance_point_line(
    sketch: &Sketch,
    p: PointId,
    line: EntityId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let ends = sketch.entities.get(&line).and_then(|e| e.line_ends());
    let signed = point_line_signed_distance(sketch, p, line);
    if let Some(s) = signed {
        out.measure(s.abs());
    }
    out.row(signed.map_or(0.0, f64::abs) - value, |g| {
        if let (Some((start, end)), Some(s)) = (ends, signed) {
            g.signed_dist(sketch, p, start, end, s.signum());
        }
    });
}

pub(super) fn distance_parallel_lines(
    sketch: &Sketch,
    a: EntityId,
    b: EntityId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    // Measured from `b`'s start point to the infinite line through `a`.
    let from = sketch
        .entities
        .get(&b)
        .and_then(|e| e.line_ends())
        .map(|(start, _)| start);
    let ends_a = sketch.entities.get(&a).and_then(|e| e.line_ends());
    let signed = from.and_then(|start| point_line_signed_distance(sketch, start, a));
    if let Some(s) = signed {
        out.measure(s.abs());
    }
    out.row(signed.map_or(0.0, f64::abs) - value, |g| {
        if let (Some(start), Some((a_start, a_end)), Some(s)) = (from, ends_a, signed) {
            g.signed_dist(sketch, start, a_start, a_end, s.signum());
        }
    });
}

pub(super) fn angle(
    sketch: &Sketch,
    a: EntityId,
    b: EntityId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let Some((ends_a, ends_b, da, db)) = line_pair_dirs(sketch, a, b) else {
        out.zeros(1);
        return;
    };
    let theta = signed_angle(da, db);
    out.measure(theta.abs());
    out.row(theta.abs() - value, |g| {
        push_angle_grad(g, ends_a, ends_b, da, db, theta.signum());
    });
}

pub(super) fn angle_points(
    sketch: &Sketch,
    a: PointId,
    vertex: PointId,
    b: PointId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let pa = point(sketch, a);
    let pv = point(sketch, vertex);
    let pb = point(sketch, b);
    let v1 = (pa.x - pv.x, pa.y - pv.y);
    let v2 = (pb.x - pv.x, pb.y - pv.y);
    let theta = signed_angle(v1, v2);
    out.measure(theta.abs());
    out.row(theta.abs() - value, |g| {
        let s = theta.signum();
        let (d_da, d_db) = angle_grad_vectors(v1.0, v1.1, v2.0, v2.1);
        g.pt(a, s * d_da.0, s * d_da.1);
        g.pt(b, s * d_db.0, s * d_db.1);
        g.pt(vertex, -s * (d_da.0 + d_db.0), -s * (d_da.1 + d_db.1));
    });
}

pub(super) fn radius(sketch: &Sketch, target: EntityId, value: f64, out: &mut ConstraintEval<'_>) {
    let r = entity_radius(sketch, target);
    if let Some(r) = r {
        out.measure(r);
    }
    out.row(r.unwrap_or(0.0) - value, |g| {
        g.entity_radius(sketch, target, 1.0);
    });
}

pub(super) fn diameter(
    sketch: &Sketch,
    target: EntityId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let r = entity_radius(sketch, target);
    if let Some(r) = r {
        out.measure(2.0 * r);
    }
    out.row(2.0 * r.unwrap_or(0.0) - value, |g| {
        g.entity_radius(sketch, target, 2.0);
    });
}

pub(super) fn arc_length(sketch: &Sketch, arc: EntityId, value: f64, out: &mut ConstraintEval<'_>) {
    let Some((center, start, end)) = arc_points(sketch, arc) else {
        out.zeros(1);
        return;
    };
    let c = point(sketch, center);
    let s = point(sketch, start);
    let e = point(sketch, end);
    let ds = (s.x - c.x, s.y - c.y);
    let de = (e.x - c.x, e.y - c.y);
    let rs = (ds.0 * ds.0 + ds.1 * ds.1).sqrt();
    let re = (de.0 * de.0 + de.1 * de.1).sqrt();
    // Sweep CCW from start to end, as `region.rs` tessellates it.
    let mut theta = signed_angle(ds, de);
    if theta < 0.0 {
        theta += std::f64::consts::TAU;
    }
    let r_avg = 0.5 * (rs.max(LENGTH_TOLERANCE) + re.max(LENGTH_TOLERANCE));
    out.measure(r_avg * theta);
    out.row(r_avg * theta - value, |g| {
        let unit = |v: (f64, f64), r: f64| {
            if r >= LENGTH_TOLERANCE {
                (v.0 / r, v.1 / r)
            } else {
                (0.0, 0.0)
            }
        };
        let (us_x, us_y) = unit(ds, rs);
        let (ue_x, ue_y) = unit(de, re);
        let (d_da, d_db) = angle_grad_vectors(ds.0, ds.1, de.0, de.1);
        let half_theta = 0.5 * theta;
        let ds_x = half_theta * us_x + r_avg * d_da.0;
        let ds_y = half_theta * us_y + r_avg * d_da.1;
        let de_x = half_theta * ue_x + r_avg * d_db.0;
        let de_y = half_theta * ue_y + r_avg * d_db.1;
        g.pt(start, ds_x, ds_y);
        g.pt(end, de_x, de_y);
        g.pt(center, -(ds_x + de_x), -(ds_y + de_y));
    });
}

pub(super) fn distance_to_axis_x(
    sketch: &Sketch,
    p: PointId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let x = point(sketch, p).x;
    out.measure(x.abs());
    out.row(x.abs() - value, |g| g.pt(p, x.signum(), 0.0));
}

pub(super) fn distance_to_axis_y(
    sketch: &Sketch,
    p: PointId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let y = point(sketch, p).y;
    out.measure(y.abs());
    out.row(y.abs() - value, |g| g.pt(p, 0.0, y.signum()));
}

pub(super) fn distance_circle_circle(
    sketch: &Sketch,
    a: EntityId,
    b: EntityId,
    value: f64,
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
    // Clearance is measured through the gap for disjoint circles and through
    // the annulus for nested ones — whichever `value` is asking about.
    let external = d - (ra + rb);
    let internal = (ra - rb).abs() - d;
    let is_internal = internal >= 0.0 && (internal - value).abs() < (external - value).abs();
    let clearance = if is_internal { internal } else { external };
    out.measure(clearance);
    out.row(clearance - value, |g| {
        let sign = if is_internal { -1.0 } else { 1.0 };
        if d >= LENGTH_TOLERANCE {
            let (ux, uy) = (dx / d, dy / d);
            g.pt(ca, sign * ux, sign * uy);
            g.pt(cb, -sign * ux, -sign * uy);
        }
        if is_internal {
            let sigma = (ra - rb).signum();
            g.entity_radius(sketch, a, sigma);
            g.entity_radius(sketch, b, -sigma);
        } else {
            g.entity_radius(sketch, a, -1.0);
            g.entity_radius(sketch, b, -1.0);
        }
    });
}

pub(super) fn distance_point_circle(
    sketch: &Sketch,
    p: PointId,
    circle: EntityId,
    value: f64,
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
    let clearance = (d - radius).abs();
    out.measure(clearance);
    out.row(clearance - value, |g| {
        let s = (d - radius).signum();
        if d >= LENGTH_TOLERANCE {
            let (ux, uy) = (dx / d, dy / d);
            g.pt(p, s * ux, s * uy);
            g.pt(center, -s * ux, -s * uy);
        }
        g.entity_radius(sketch, circle, -s);
    });
}
