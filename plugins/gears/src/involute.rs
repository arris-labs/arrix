//! A spur gear's outline: standard involute teeth (addendum `m`, dedendum
//! `1.25 m`, no profile shift, no backlash), each involute flank as
//! circular arcs within a stated tolerance of the true curve.
//!
//! Everything here is a pure function of its arguments, so the outline,
//! and with it every curve key, is the same on every evaluation.

use std::f64::consts::PI;

use arrix_plugin_api::{CurveKey, DVec2, Id, ProfileLoop, ProfileSegment};

/// How far an arc may stray from the involute it stands for, as a fraction
/// of the module: 1 µm at `m = 1 mm`. The plugin's choice, not a kernel
/// setting.
pub const TOLERANCE_PER_MODULE: f64 = 1e-3;

/// How many times a flank piece may be halved to meet the tolerance: at
/// most 64 arcs a flank.
const MAX_DEPTH: u32 = 6;

/// Samples per arc when measuring its error.
const SAMPLES: usize = 16;

/// A gear's parameters, in SI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spur {
    pub teeth: u32,
    /// Metres.
    pub module: f64,
    /// Radians.
    pub pressure_angle: f64,
}

/// Why no outline exists for the parameters.
#[derive(Clone, Debug, PartialEq)]
pub enum OutlineError {
    /// The teeth meet in a point before the tip circle.
    Pointed,
    /// Neighbouring teeth touch at the root circle.
    NoGap,
    /// A flank piece still strays past the tolerance after the last split.
    Tolerance,
}

/// A point in the gear's polar coordinates: radius, then angle
/// anticlockwise from the plane's X axis.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Polar(f64, f64);

impl Polar {
    fn point(self) -> DVec2 {
        DVec2::new(self.0 * self.1.cos(), self.0 * self.1.sin())
    }

    /// Rotated by `angle` (mirrored first when `mirror`).
    fn place(self, angle: f64, mirror: bool) -> DVec2 {
        let a = if mirror { -self.1 } else { self.1 };
        Polar(self.0, angle + a).point()
    }
}

fn inv(phi: f64) -> f64 {
    phi.tan() - phi
}

/// One side of a tooth centred on angle 0, from the root side outwards:
/// the ends of each piece and a point in its middle, all on the true curve.
struct Flank {
    /// The radial line below the base circle, when the root is inside it.
    radial: Option<(Polar, Polar)>,
    /// Each involute arc: (from, via, to).
    arcs: Vec<(Polar, Polar, Polar)>,
}

impl Spur {
    pub fn pitch_radius(&self) -> f64 {
        self.module * f64::from(self.teeth) / 2.0
    }

    pub fn base_radius(&self) -> f64 {
        self.pitch_radius() * self.pressure_angle.cos()
    }

    pub fn tip_radius(&self) -> f64 {
        self.pitch_radius() + self.module
    }

    pub fn root_radius(&self) -> f64 {
        self.pitch_radius() - 1.25 * self.module
    }

    pub fn tolerance(&self) -> f64 {
        self.module * TOLERANCE_PER_MODULE
    }

    /// The involute at roll parameter `t` (`tan` of its pressure angle),
    /// on the side of a tooth centred at 0 that faces clockwise.
    fn involute(&self, t: f64) -> Polar {
        let rho = self.base_radius() * (1.0 + t * t).sqrt();
        let half = PI / (2.0 * f64::from(self.teeth)) + inv(self.pressure_angle);
        Polar(rho, -(half - (t - t.atan())))
    }

    fn roll(&self, rho: f64) -> f64 {
        ((rho / self.base_radius()).powi(2) - 1.0).max(0.0).sqrt()
    }

    /// The largest distance from the involute between `t0` and `t1` to the
    /// circle through `a`, `m`, `b`.
    fn arc_error(&self, t0: f64, t1: f64, a: Polar, m: Polar, b: Polar) -> f64 {
        let Some((center, radius)) = circle(a.point(), m.point(), b.point()) else {
            return f64::INFINITY;
        };
        (1..SAMPLES)
            .map(|i| {
                let s = t0 + (t1 - t0) * i as f64 / SAMPLES as f64;
                ((self.involute(s).point() - center).length() - radius).abs()
            })
            .fold(0.0, f64::max)
    }

    /// Arcs for the involute from `t0` to `t1`, halving until each is
    /// within the tolerance.
    fn fit(&self, t0: f64, t1: f64, depth: u32, out: &mut Vec<(Polar, Polar, Polar)>) -> bool {
        let tm = (t0 + t1) / 2.0;
        let (a, m, b) = (self.involute(t0), self.involute(tm), self.involute(t1));
        if self.arc_error(t0, t1, a, m, b) <= self.tolerance() {
            out.push((a, m, b));
            return true;
        }
        depth < MAX_DEPTH && self.fit(t0, tm, depth + 1, out) && self.fit(tm, t1, depth + 1, out)
    }

    fn flank(&self) -> Result<Flank, OutlineError> {
        let (rb, rf, ra) = (self.base_radius(), self.root_radius(), self.tip_radius());
        let start = rf.max(rb);
        let mut arcs = Vec::new();
        if !self.fit(self.roll(start), self.roll(ra), 0, &mut arcs) {
            return Err(OutlineError::Tolerance);
        }
        let radial = (rf < rb).then(|| {
            let base = self.involute(0.0);
            (Polar(rf, base.1), base)
        });
        Ok(Flank { radial, arcs })
    }

    /// The outline, anticlockwise from tooth 1's root, tooth 1 centred on
    /// the plane's X axis. Keys: tooth `n` (from 1) holds `n·1000 + k`,
    /// `k` 1 and 2 the rising and falling radial lines, 3 the tip, 4 the
    /// root arc after the tooth, `100 + j` and `200 + j` the rising and
    /// falling flanks' arcs from the root outwards.
    pub fn outline(&self) -> Result<ProfileLoop, OutlineError> {
        let flank = self.flank()?;
        let tip = flank.arcs.last().expect("a flank has an arc").2;
        let foot = flank.radial.map_or(flank.arcs[0].0, |(f, _)| f);
        if tip.1 >= 0.0 {
            return Err(OutlineError::Pointed);
        }
        let pitch = 2.0 * PI / f64::from(self.teeth);
        if pitch / 2.0 + foot.1 <= 0.0 {
            return Err(OutlineError::NoGap);
        }
        let arcs = u32::try_from(flank.arcs.len()).expect("at most 64 arcs a flank");
        let mut segments = Vec::new();
        for n in 1..=self.teeth {
            let gamma = pitch * f64::from(n - 1);
            let key = |k: u32| CurveKey(Id(u64::from(n) * 1000 + u64::from(k)));
            let line = |k, to: Polar, mirror| ProfileSegment::Line {
                key: key(k),
                to: to.place(gamma, mirror),
            };
            let arc = |k, via: Polar, to: Polar, mirror| ProfileSegment::Arc {
                key: key(k),
                to: to.place(gamma, mirror),
                via: via.place(gamma, mirror),
            };
            if let Some((_, top)) = flank.radial {
                segments.push(line(1, top, false));
            }
            for (j, (_, via, to)) in (0..arcs).zip(&flank.arcs) {
                segments.push(arc(100 + j, *via, *to, false));
            }
            segments.push(arc(3, Polar(tip.0, 0.0), tip, true));
            for (j, (from, via, _)) in (0..arcs).zip(&flank.arcs).rev() {
                segments.push(arc(200 + j, *via, *from, true));
            }
            if let Some((bottom, _)) = flank.radial {
                segments.push(line(2, bottom, true));
            }
            let via = Polar(foot.0, pitch / 2.0).place(gamma, false);
            // The last root arc ends exactly where the loop starts.
            let to = if n == self.teeth {
                foot.point()
            } else {
                Polar(foot.0, foot.1 + pitch).place(gamma, false)
            };
            segments.push(ProfileSegment::Arc {
                key: key(4),
                to,
                via,
            });
        }
        Ok(ProfileLoop::Path {
            start: foot.point(),
            segments,
        })
    }
}

/// The circle through three points, `None` when they are collinear.
pub fn circle(a: DVec2, b: DVec2, c: DVec2) -> Option<(DVec2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < f64::EPSILON * a.length_squared().max(1e-300) {
        return None;
    }
    let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
    let center = DVec2::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    Some((center, (a - center).length()))
}

#[cfg(test)]
mod tests;
