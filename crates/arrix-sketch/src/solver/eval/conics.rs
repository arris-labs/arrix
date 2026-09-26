//! Ellipse and B-spline kinds. The ellipse residual/gradient pairs come out of
//! `point_on_ellipse_eval` / `tangent_line_ellipse_eval`, which already return
//! value and derivatives together.
//!
//! Behind the off-by-default `conics` feature — no UI path creates the
//! entities or constraints these serve (docs/DATA-MODEL.md §Sketches).
//! `--all-features` keeps the math under test.

use crate::constraint::AlignmentKind;

use super::*;

pub(super) fn point_on_ellipse(
    sketch: &Sketch,
    p: PointId,
    ellipse: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let Some((c_id, m_id, b)) = entity_ellipse_info(sketch, ellipse) else {
        out.zeros(1);
        return;
    };
    let pt = point(sketch, p);
    let c = point(sketch, c_id);
    let m = point(sketch, m_id);
    let (r, ((d_px, d_py), (d_cx, d_cy), (d_mx, d_my), d_b)) =
        point_on_ellipse_eval(pt.x, pt.y, c.x, c.y, m.x, m.y, b);
    out.row(r, |g| {
        g.pt(p, d_px, d_py);
        g.pt(c_id, d_cx, d_cy);
        g.pt(m_id, d_mx, d_my);
        g.minor_radius_var(ellipse, d_b);
    });
}

pub(super) fn tangent_line_ellipse(
    sketch: &Sketch,
    line: EntityId,
    ellipse: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let (Some((start, end)), Some((c_id, m_id, b))) = (
        sketch.entities.get(&line).and_then(|e| e.line_ends()),
        entity_ellipse_info(sketch, ellipse),
    ) else {
        out.zeros(1);
        return;
    };
    let a = point(sketch, start);
    let b_pt = point(sketch, end);
    let c = point(sketch, c_id);
    let m = point(sketch, m_id);
    let (r, ((d_ax, d_ay), (d_bx, d_by), (d_cx, d_cy), (d_mx, d_my), d_b)) =
        tangent_line_ellipse_eval(a.x, a.y, b_pt.x, b_pt.y, c.x, c.y, m.x, m.y, b);
    out.row(r, |g| {
        g.pt(start, d_ax, d_ay);
        g.pt(end, d_bx, d_by);
        g.pt(c_id, d_cx, d_cy);
        g.pt(m_id, d_mx, d_my);
        g.minor_radius_var(ellipse, d_b);
    });
}

/// The ellipse frame every `InternalAlignment` row is expressed in:
/// centre C, the vector v = C→M along the major axis, s = |v|², and the minor
/// radius clamped inside the major one.
struct EllipseFrame {
    c_id: PointId,
    m_id: PointId,
    ellipse: EntityId,
    cx: f64,
    cy: f64,
    vx: f64,
    vy: f64,
    s: f64,
    b_eff: f64,
}

/// A focus sits at C ± µv, with µ = sqrt(1 − b²/s).
fn focus_rows(
    sketch: &Sketch,
    f: &EllipseFrame,
    sign: f64,
    p: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let mu = (1.0 - (f.b_eff * f.b_eff) / f.s).max(0.0).sqrt();
    let (dmu_ds, dmu_db) = if mu > 1e-9 {
        (
            (f.b_eff * f.b_eff) / (2.0 * mu * f.s * f.s),
            -f.b_eff / (mu * f.s),
        )
    } else {
        (0.0, 0.0)
    };
    let pt = point(sketch, p);
    out.row(pt.x - (f.cx + sign * mu * f.vx), |g| {
        let d_mx = -sign * (mu + f.vx * dmu_ds * 2.0 * f.vx);
        let d_my = -sign * f.vx * dmu_ds * 2.0 * f.vy;
        g.pt(p, 1.0, 0.0);
        g.pt(f.c_id, -1.0 - d_mx, -d_my);
        g.pt(f.m_id, d_mx, d_my);
        g.minor_radius_var(f.ellipse, -sign * f.vx * dmu_db);
    });
    out.row(pt.y - (f.cy + sign * mu * f.vy), |g| {
        let d_mx = -sign * f.vy * dmu_ds * 2.0 * f.vx;
        let d_my = -sign * (mu + f.vy * dmu_ds * 2.0 * f.vy);
        g.pt(p, 0.0, 1.0);
        g.pt(f.c_id, -d_mx, -1.0 - d_my);
        g.pt(f.m_id, d_mx, d_my);
        g.minor_radius_var(f.ellipse, -sign * f.vy * dmu_db);
    });
}

/// The minor-axis endpoint sits at C + λ·v⊥, with λ = b/sqrt(s).
fn minor_axis_rows(
    sketch: &Sketch,
    f: &EllipseFrame,
    b: f64,
    p: PointId,
    out: &mut ConstraintEval<'_>,
) {
    let root_s = f.s.sqrt();
    let lambda = b / root_s;
    let dlam_ds = -b / (2.0 * f.s * root_s);
    let dlam_db = 1.0 / root_s;
    let pt = point(sketch, p);
    out.row(pt.x - (f.cx - lambda * f.vy), |g| {
        let d_mx = f.vy * dlam_ds * 2.0 * f.vx;
        let d_my = lambda + f.vy * dlam_ds * 2.0 * f.vy;
        g.pt(p, 1.0, 0.0);
        g.pt(f.c_id, -1.0 - d_mx, -d_my);
        g.pt(f.m_id, d_mx, d_my);
        g.minor_radius_var(f.ellipse, f.vy * dlam_db);
    });
    out.row(pt.y - (f.cy + lambda * f.vx), |g| {
        let d_mx = -lambda - f.vx * dlam_ds * 2.0 * f.vx;
        let d_my = -f.vx * dlam_ds * 2.0 * f.vy;
        g.pt(p, 0.0, 1.0);
        g.pt(f.c_id, -d_mx, -1.0 - d_my);
        g.pt(f.m_id, d_mx, d_my);
        g.minor_radius_var(f.ellipse, -f.vx * dlam_db);
    });
}

pub(super) fn internal_alignment(
    sketch: &Sketch,
    ellipse: EntityId,
    alignment: AlignmentKind,
    out: &mut ConstraintEval<'_>,
) {
    let Some((c_id, m_id, b)) = entity_ellipse_info(sketch, ellipse) else {
        out.zeros(2);
        return;
    };
    let c = point(sketch, c_id);
    let m = point(sketch, m_id);
    let (vx, vy) = (m.x - c.x, m.y - c.y);
    let s = (vx * vx + vy * vy).max(1e-12);
    let frame = EllipseFrame {
        c_id,
        m_id,
        ellipse,
        cx: c.x,
        cy: c.y,
        vx,
        vy,
        s,
        b_eff: b.abs().min(s.sqrt() - 1e-12).max(0.0),
    };
    match alignment {
        AlignmentKind::Focus1(p) => focus_rows(sketch, &frame, 1.0, p, out),
        AlignmentKind::Focus2(p) => focus_rows(sketch, &frame, -1.0, p, out),
        AlignmentKind::MinorRadiusEnd(p) => minor_axis_rows(sketch, &frame, b, p, out),
        AlignmentKind::MajorRadiusEnd(p) => {
            let pt = point(sketch, p);
            out.row(pt.x - m.x, |g| {
                g.pt(p, 1.0, 0.0);
                g.pt(m_id, -1.0, 0.0);
            });
            out.row(pt.y - m.y, |g| {
                g.pt(p, 0.0, 1.0);
                g.pt(m_id, 0.0, -1.0);
            });
        }
    }
}

pub(super) fn minor_radius(
    sketch: &Sketch,
    ellipse: EntityId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let b = entity_minor_radius(sketch, ellipse);
    if let Some(b) = b {
        out.measure(b);
    }
    out.row(b.unwrap_or(0.0) - value, |g| {
        g.minor_radius_var(ellipse, 1.0);
    });
}

pub(super) fn major_radius(
    sketch: &Sketch,
    ellipse: EntityId,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let info = entity_ellipse_info(sketch, ellipse);
    let Some((c_id, m_id, _)) = info else {
        out.row(-value, |_| {});
        return;
    };
    let c = point(sketch, c_id);
    let m = point(sketch, m_id);
    let (dx, dy) = (m.x - c.x, m.y - c.y);
    let a = (dx * dx + dy * dy).sqrt();
    out.measure(a);
    out.row(a - value, |g| {
        let a = a.max(LENGTH_TOLERANCE);
        let (ux, uy) = (dx / a, dy / a);
        g.pt(m_id, ux, uy);
        g.pt(c_id, -ux, -uy);
    });
}

pub(super) fn point_on_bspline(
    sketch: &Sketch,
    p: PointId,
    bspline: EntityId,
    u: f64,
    out: &mut ConstraintEval<'_>,
) {
    let Some((cps, knots, degree, _)) = entity_bspline_info(sketch, bspline) else {
        out.zeros(2);
        return;
    };
    let pt = point(sketch, p);
    let pts: Vec<[f64; 2]> = cps.iter().map(|&pid| point(sketch, pid).pos()).collect();
    let actual = crate::bspline::evaluate_bspline_point(&pts, degree, knots, u);
    let basis = || crate::bspline::evaluate_bspline_basis(degree, knots, u);
    out.row(pt.x - actual[0], |g| {
        let basis = basis();
        g.pt(p, 1.0, 0.0);
        for (i, &pid) in cps.iter().enumerate() {
            if i < basis.len() {
                g.pt(pid, -basis[i], 0.0);
            }
        }
    });
    out.row(pt.y - actual[1], |g| {
        let basis = basis();
        g.pt(p, 0.0, 1.0);
        for (i, &pid) in cps.iter().enumerate() {
            if i < basis.len() {
                g.pt(pid, 0.0, -basis[i]);
            }
        }
    });
}

pub(super) fn bspline_tangent(
    sketch: &Sketch,
    bspline: EntityId,
    u: f64,
    line: EntityId,
    out: &mut ConstraintEval<'_>,
) {
    let (Some((cps, knots, degree, _)), Some((start, end))) = (
        entity_bspline_info(sketch, bspline),
        sketch.entities.get(&line).and_then(|e| e.line_ends()),
    ) else {
        out.zeros(1);
        return;
    };
    let a = point(sketch, start);
    let b = point(sketch, end);
    let pts: Vec<[f64; 2]> = cps.iter().map(|&pid| point(sketch, pid).pos()).collect();
    let (_, d1, _) = crate::bspline::evaluate_bspline_derivatives(&pts, degree, knots, u);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    out.row(d1[0] * dy - d1[1] * dx, |g| {
        let (_, n1, _) = crate::bspline::evaluate_bspline_basis_derivs(degree, knots, u);
        for (i, &pid) in cps.iter().enumerate() {
            if i < n1.len() {
                g.pt(pid, n1[i] * dy, -n1[i] * dx);
            }
        }
        g.pt(start, d1[1], -d1[0]);
        g.pt(end, -d1[1], d1[0]);
    });
}

pub(super) fn bspline_curvature(
    sketch: &Sketch,
    bspline: EntityId,
    u: f64,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let Some((cps, knots, degree, _)) = entity_bspline_info(sketch, bspline) else {
        out.zeros(1);
        return;
    };
    let pts: Vec<[f64; 2]> = cps.iter().map(|&pid| point(sketch, pid).pos()).collect();
    let kappa = crate::bspline::evaluate_bspline_curvature(&pts, degree, knots, u);
    out.measure(kappa);
    out.row(kappa - value, |g| {
        let (_, d1, d2) = crate::bspline::evaluate_bspline_derivatives(&pts, degree, knots, u);
        let (_, n1, n2) = crate::bspline::evaluate_bspline_basis_derivs(degree, knots, u);
        let (tx, ty) = (d1[0], d1[1]);
        let (ax, ay) = (d2[0], d2[1]);
        let w = tx * ay - ty * ax;
        let l_sq = (tx * tx + ty * ty).max(1e-12);
        let l3 = l_sq * l_sq.sqrt();
        let l5 = l3 * l_sq;
        for (i, &pid) in cps.iter().enumerate() {
            if i < n1.len() {
                let dw_dx = n1[i] * ay - ty * n2[i];
                let dw_dy = tx * n2[i] - n1[i] * ax;
                g.pt(
                    pid,
                    dw_dx / l3 - 3.0 * w * tx * n1[i] / l5,
                    dw_dy / l3 - 3.0 * w * ty * n1[i] / l5,
                );
            }
        }
    });
}
