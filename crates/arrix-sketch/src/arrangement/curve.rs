//! The geometry the arrangement works in: a sketch entity reduced to a line,
//! a circle or a circular arc, with the parameterisation every other module
//! here measures along.
//!
//! Conics and B-splines have no [`Curve`]:
//! they are never intersected, split or walked, and [`crate::region`] keeps
//! sampling them as it always has.
//!
//! The parameter `t` runs `0 → 1` along the entity's own direction, the one
//! `region::curve_samples` draws it in: start to end for a line or an arc,
//! and CCW from angle 0 for a circle. Pieces are numbered along it and
//! side-face names with them, so it is the one ordering the arrangement
//! has.

use std::f64::consts::TAU;

use arrix_core::LENGTH_TOLERANCE;

use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

/// A sketch curve reduced to what the arrangement needs. Arcs run CCW from
/// `start_angle` by `sweep`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Curve {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    /// Closed: every angle is in its domain, and it has no endpoints.
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        /// In `(0, TAU]`, CCW. A piece of a circle cut once is a full turn.
        sweep: f64,
    },
}

impl Curve {
    /// The entity's geometry, or `None` when it is not a line, circle or arc,
    /// or is degenerate (zero length, zero radius). Construction geometry is
    /// not filtered here — the arrangement reads the sketch through
    /// `profile_entities` and never offers one.
    pub(crate) fn from_entity(sketch: &Sketch, id: EntityId) -> Option<Curve> {
        let pos = |p: &PointId| sketch.points.get(p).map(|q| q.pos());
        match sketch.entities.get(&id)? {
            Entity::Line { start, end } => {
                let (a, b) = (pos(start)?, pos(end)?);
                (dist(a, b) > LENGTH_TOLERANCE).then_some(Curve::Line { a, b })
            }
            Entity::Circle { center, radius } => {
                let center = pos(center)?;
                (*radius > LENGTH_TOLERANCE).then_some(Curve::Circle {
                    center,
                    radius: *radius,
                })
            }
            Entity::Arc { center, start, end } => {
                let (c, s, e) = (pos(center)?, pos(start)?, pos(end)?);
                let radius = dist(c, s);
                if radius <= LENGTH_TOLERANCE {
                    return None;
                }
                let start_angle = angle_at(c, s);
                let sweep = wrap_tau(angle_at(c, e) - start_angle);
                // Ends meeting ends is a full turn, as it is for the sampler
                // in `region` — never an arc of nothing.
                if sweep <= LENGTH_TOLERANCE / radius {
                    return Some(Curve::Circle { center: c, radius });
                }
                Some(Curve::Arc {
                    center: c,
                    radius,
                    start_angle,
                    sweep,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn line(&self) -> Option<([f64; 2], [f64; 2])> {
        match *self {
            Curve::Line { a, b } => Some((a, b)),
            _ => None,
        }
    }

    pub(crate) fn circle(&self) -> Option<([f64; 2], f64)> {
        match *self {
            Curve::Line { .. } => None,
            Curve::Circle { center, radius } | Curve::Arc { center, radius, .. } => {
                Some((center, radius))
            }
        }
    }

    /// Closed curves have no ends to cut at and no direction to come from.
    pub(crate) fn is_closed(&self) -> bool {
        matches!(self, Curve::Circle { .. })
    }

    /// The curve's own ends: none for a full circle.
    pub(crate) fn endpoints(&self) -> Vec<[f64; 2]> {
        match *self {
            Curve::Line { a, b } => vec![a, b],
            Curve::Circle { .. } => Vec::new(),
            Curve::Arc { .. } => vec![self.point_at(0.0), self.point_at(1.0)],
        }
    }

    /// How long the curve is, in meters — the scale a parameter tolerance is
    /// read at.
    pub(crate) fn length(&self) -> f64 {
        match *self {
            Curve::Line { a, b } => dist(a, b),
            Curve::Circle { radius, .. } => TAU * radius,
            Curve::Arc { radius, sweep, .. } => radius * sweep,
        }
    }

    /// `LENGTH_TOLERANCE` expressed in this curve's parameter.
    pub(crate) fn param_tolerance(&self) -> f64 {
        LENGTH_TOLERANCE / self.length()
    }

    /// The point at parameter `t`; `t` outside `[0, 1]` extrapolates along
    /// the line or around the circle.
    pub(crate) fn point_at(&self, t: f64) -> [f64; 2] {
        match *self {
            Curve::Line { a, b } => along(a, sub(b, a), t),
            Curve::Circle { center, radius } => point_on(center, radius, TAU * t),
            Curve::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => point_on(center, radius, start_angle + sweep * t),
        }
    }

    /// Where `p`, taken to lie on the underlying line or circle, falls in
    /// this curve's parameter. `[0, 1)` for a closed curve; a point before an
    /// open curve's start answers above 1, since the angle it is found at
    /// wraps the long way round.
    pub(crate) fn param_at(&self, p: [f64; 2]) -> f64 {
        match *self {
            Curve::Line { a, b } => {
                let d = sub(b, a);
                dot(sub(p, a), d) / dot(d, d)
            }
            Curve::Circle { center, .. } => wrap_tau(angle_at(center, p)) / TAU,
            Curve::Arc {
                center,
                start_angle,
                sweep,
                ..
            } => wrap_tau(angle_at(center, p) - start_angle) / sweep,
        }
    }

    /// The stretch of this curve between two parameters, as a curve of its
    /// own. `t0 == 0` and `t1 == 1` give the curve back unchanged, so an
    /// uncut circle stays a circle.
    pub(crate) fn sub_curve(&self, t0: f64, t1: f64) -> Curve {
        if t0 == 0.0 && t1 == 1.0 {
            return *self;
        }
        match *self {
            Curve::Line { .. } => Curve::Line {
                a: self.point_at(t0),
                b: self.point_at(t1),
            },
            Curve::Circle { center, radius } => Curve::Arc {
                center,
                radius,
                start_angle: TAU * t0,
                sweep: TAU * (t1 - t0),
            },
            Curve::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => Curve::Arc {
                center,
                radius,
                start_angle: start_angle + sweep * t0,
                sweep: sweep * (t1 - t0),
            },
        }
    }

    /// The unit tangent at parameter `t`, pointing along the curve's own
    /// direction. This is what orders half-edges around a vertex — a chord
    /// to the piece's far end answers with the wrong direction wherever a
    /// curve bends.
    pub(crate) fn tangent_at(&self, t: f64) -> [f64; 2] {
        match *self {
            Curve::Line { a, b } => unit(sub(b, a)),
            Curve::Circle { .. } => tangent_on(TAU * t),
            Curve::Arc {
                start_angle, sweep, ..
            } => tangent_on(start_angle + sweep * t),
        }
    }

    /// The curvature a traversal sees, signed so that turning left is
    /// positive: zero along a line, `±1/r` around a circle or an arc. It
    /// breaks the tie between two pieces that leave a vertex in the same
    /// direction, which is every tangency.
    pub(crate) fn curvature(&self, forward: bool) -> f64 {
        match self.circle() {
            None => 0.0,
            Some((_, radius)) => {
                if forward {
                    1.0 / radius
                } else {
                    -1.0 / radius
                }
            }
        }
    }

    /// This curve's exact term in `½∮(x dy − y dx)` — its contribution to
    /// the signed area of a loop traversing it in `forward`. Exact for arcs,
    /// where sampling the rim would cost a percent of the area per region.
    pub(crate) fn signed_area_term(&self, forward: bool) -> f64 {
        let (s, e) = (self.point_at(0.0), self.point_at(1.0));
        let (s, e) = if forward { (s, e) } else { (e, s) };
        let Some((c, radius)) = self.circle() else {
            return 0.5 * cross(s, e);
        };
        let sweep = match *self {
            Curve::Arc { sweep, .. } => sweep,
            _ => TAU,
        };
        let swept = if forward { sweep } else { -sweep };
        0.5 * (radius * radius * swept + c[0] * (e[1] - s[1]) - c[1] * (e[0] - s[0]))
    }

    /// Is `p`, already known to lie on the underlying line or circle, inside
    /// this curve's own extent? Generous by `LENGTH_TOLERANCE` at both ends,
    /// so a hit on an endpoint counts.
    pub(crate) fn contains(&self, p: [f64; 2]) -> bool {
        match *self {
            Curve::Line { .. } => {
                let tol = self.param_tolerance();
                (-tol..=1.0 + tol).contains(&self.param_at(p))
            }
            Curve::Circle { .. } => true,
            Curve::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => {
                let tol = LENGTH_TOLERANCE / radius;
                let rel = wrap_tau(angle_at(center, p) - start_angle);
                rel <= sweep + tol || rel >= TAU - tol
            }
        }
    }
}

pub(crate) fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub(crate) fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub(crate) fn norm(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

pub(crate) fn unit(a: [f64; 2]) -> [f64; 2] {
    let n = norm(a);
    [a[0] / n, a[1] / n]
}

pub(crate) fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// `p + t·d`.
pub(crate) fn along(p: [f64; 2], d: [f64; 2], t: f64) -> [f64; 2] {
    [p[0] + d[0] * t, p[1] + d[1] * t]
}

pub(crate) fn angle_at(center: [f64; 2], p: [f64; 2]) -> f64 {
    (p[1] - center[1]).atan2(p[0] - center[0])
}

/// The CCW unit tangent of a circle at `angle`.
pub(crate) fn tangent_on(angle: f64) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [-sin, cos]
}

pub(crate) fn point_on(center: [f64; 2], radius: f64, angle: f64) -> [f64; 2] {
    [
        center[0] + radius * angle.cos(),
        center[1] + radius * angle.sin(),
    ]
}

/// `angle` folded into `[0, TAU)`.
pub(crate) fn wrap_tau(angle: f64) -> f64 {
    let a = angle % TAU;
    if a < 0.0 { a + TAU } else { a }
}
