//! Kinds measured against the sketch's own datum geometry (origin and the two
//! axes) rather than against another entity.

use crate::constraint::DatumEntity;

use super::*;

pub(super) fn point_on_datum(
    sketch: &Sketch,
    p: PointId,
    datum: DatumEntity,
    out: &mut ConstraintEval<'_>,
) {
    let pt = point(sketch, p);
    match datum {
        DatumEntity::Origin => {
            out.row(pt.x, |g| g.pt(p, 1.0, 0.0));
            out.row(pt.y, |g| g.pt(p, 0.0, 1.0));
        }
        DatumEntity::AxisX => out.row(pt.y, |g| g.pt(p, 0.0, 1.0)),
        DatumEntity::AxisY => out.row(pt.x, |g| g.pt(p, 1.0, 0.0)),
    }
}

pub(super) fn distance_to_datum(
    sketch: &Sketch,
    p: PointId,
    datum: DatumEntity,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let pt = point(sketch, p);
    match datum {
        DatumEntity::Origin => {
            let d = (pt.x * pt.x + pt.y * pt.y).sqrt();
            out.measure(d);
            out.row(d - value, |g| {
                if d >= LENGTH_TOLERANCE {
                    g.pt(p, pt.x / d, pt.y / d);
                }
            });
        }
        DatumEntity::AxisX => {
            out.measure(pt.y.abs());
            // `signum()` would answer +1 at exactly zero; the sign convention
            // here is "negative only below the axis".
            let s = if pt.y < 0.0 { -1.0 } else { 1.0 };
            out.row(pt.y.abs() - value, |g| g.pt(p, 0.0, s));
        }
        DatumEntity::AxisY => {
            out.measure(pt.x.abs());
            let s = if pt.x < 0.0 { -1.0 } else { 1.0 };
            out.row(pt.x.abs() - value, |g| g.pt(p, s, 0.0));
        }
    }
}

pub(super) fn angle_with_datum(
    sketch: &Sketch,
    line: EntityId,
    datum: DatumEntity,
    value: f64,
    out: &mut ConstraintEval<'_>,
) {
    let Some((start, end)) = sketch.entities.get(&line).and_then(|e| e.line_ends()) else {
        out.zeros(1);
        return;
    };
    let a = point(sketch, start);
    let b = point(sketch, end);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt().max(LENGTH_TOLERANCE);
    let (ux, uy) = (dx / len, dy / len);
    let angle = match datum {
        DatumEntity::AxisX | DatumEntity::Origin => uy.atan2(ux),
        DatumEntity::AxisY => (-ux).atan2(uy),
    };
    // Signed, like the residual: a reference dimension reporting `|angle|`
    // would name a value that does *not* zero this constraint's residual.
    out.measure(angle);
    let mut diff = (angle - value) % std::f64::consts::TAU;
    if diff > std::f64::consts::PI {
        diff -= std::f64::consts::TAU;
    } else if diff < -std::f64::consts::PI {
        diff += std::f64::consts::TAU;
    }
    out.row(diff, |g| {
        let len2 = (dx * dx + dy * dy).max(LENGTH_TOLERANCE * LENGTH_TOLERANCE);
        g.pt(end, -dy / len2, dx / len2);
        g.pt(start, dy / len2, -dx / len2);
    });
}

pub(super) fn symmetric_across_datum(
    sketch: &Sketch,
    a: PointId,
    b: PointId,
    datum: DatumEntity,
    out: &mut ConstraintEval<'_>,
) {
    let pa = point(sketch, a);
    let pb = point(sketch, b);
    match datum {
        DatumEntity::AxisX => {
            out.row(pa.x - pb.x, |g| {
                g.pt(a, 1.0, 0.0);
                g.pt(b, -1.0, 0.0);
            });
            out.row(pa.y + pb.y, |g| {
                g.pt(a, 0.0, 1.0);
                g.pt(b, 0.0, 1.0);
            });
        }
        DatumEntity::AxisY => {
            out.row(pa.x + pb.x, |g| {
                g.pt(a, 1.0, 0.0);
                g.pt(b, 1.0, 0.0);
            });
            out.row(pa.y - pb.y, |g| {
                g.pt(a, 0.0, 1.0);
                g.pt(b, 0.0, -1.0);
            });
        }
        DatumEntity::Origin => {
            out.row(pa.x + pb.x, |g| {
                g.pt(a, 1.0, 0.0);
                g.pt(b, 1.0, 0.0);
            });
            out.row(pa.y + pb.y, |g| {
                g.pt(a, 0.0, 1.0);
                g.pt(b, 0.0, 1.0);
            });
        }
    }
}
