//! A body's edges and vertices projected onto a plane, as the sketcher
//! draws them (docs/DATA-MODEL.md §Sketches): a point, a line, an arc or
//! a circle in the plane's own `(u, v)`. Arris projects a conic seen
//! obliquely to an ellipse and a NURBS to a NURBS; the sketch has no
//! entity for either, so those are refused here, by name.

use std::f64::consts::TAU;

use arris::geom::Curve2;
use arris::math::Interval;
use arris::ops::query::Projection;
use arrix_core::DVec2;
use serde::{Deserialize, Serialize};

/// One projected edge or vertex, in metres in the plane's `(u, v)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Projected {
    Point(DVec2),
    Line {
        start: DVec2,
        end: DVec2,
    },
    /// Anticlockwise from `start` to `end` about `center`, whichever way
    /// the edge ran.
    Arc {
        center: DVec2,
        start: DVec2,
        end: DVec2,
    },
    Circle {
        center: DVec2,
        radius: f64,
    },
}

/// The sketch's entity for `projection`; `Err` names the curve kind the
/// sketch has none for.
pub(crate) fn projected(projection: &Projection, closed: bool) -> Result<Projected, &'static str> {
    let uv = |p: arris::math::Point2| DVec2::new(p.x, p.y);
    let (curve, range) = match projection {
        Projection::Vertex { point, .. } => return Ok(Projected::Point(uv(*point))),
        Projection::Edge { curve, range, .. } => (curve, *range),
    };
    match curve {
        Curve2::Line { .. } => Ok(Projected::Line {
            start: uv(curve.point(range.lo())),
            end: uv(curve.point(range.hi())),
        }),
        Curve2::Circle { frame, radius } => {
            let center = uv(frame.to_world(arris::math::Point2::origin()));
            if closed || is_full_turn(range) {
                return Ok(Projected::Circle {
                    center,
                    radius: *radius,
                });
            }
            let at_lo = curve.eval(range.lo());
            let (a, b) = (uv(at_lo.point), uv(curve.point(range.hi())));
            let anticlockwise = (a - center).perp_dot(DVec2::new(at_lo.d1.x, at_lo.d1.y)) > 0.0;
            let (start, end) = if anticlockwise { (a, b) } else { (b, a) };
            Ok(Projected::Arc { center, start, end })
        }
        Curve2::Ellipse { .. } => Err("an ellipse"),
        Curve2::Nurbs(_) => Err("a NURBS curve"),
    }
}

fn is_full_turn(range: Interval) -> bool {
    range.length() >= TAU - 1e-12
}
