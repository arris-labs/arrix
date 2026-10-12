//! Mass, centroid and inertia: the stored shape of a result and the pure
//! math that moves and sums it (ADR-0031 §4, docs/DATA-MODEL.md
//! §Evaluation). The kernel gives a body's volume, centroid and inertia at
//! unit density; a density scales them, a frame carries them, and the
//! parallel-axis theorem sums them. No kernel here.

use arrix_core::{DMat3, DVec3, Diagnostic, Frame, MaterialId};
use serde::{Deserialize, Serialize};

/// Mass, centre of mass and inertia of something, in SI and in the frame
/// it is given in: kilograms, metres, kg m². The inertia is about the
/// centre of mass, with the frame's axes, in the physical convention (the
/// diagonal holds the moments, the off-diagonal the negated products).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Inertial {
    pub mass: f64,
    pub centroid: [f64; 3],
    pub inertia: [[f64; 3]; 3],
}

fn mat(rows: [[f64; 3]; 3]) -> DMat3 {
    // `DMat3` is column-major.
    DMat3::from_cols(
        DVec3::new(rows[0][0], rows[1][0], rows[2][0]),
        DVec3::new(rows[0][1], rows[1][1], rows[2][1]),
        DVec3::new(rows[0][2], rows[1][2], rows[2][2]),
    )
}

fn rows(m: DMat3) -> [[f64; 3]; 3] {
    std::array::from_fn(|r| std::array::from_fn(|c| m.col(c)[r]))
}

/// `|r|² E − r rᵀ`: what a point mass at `r` adds to the inertia, per
/// unit mass.
fn shift(r: DVec3) -> DMat3 {
    DMat3::from_diagonal(DVec3::splat(r.length_squared()))
        - DMat3::from_cols(r * r.x, r * r.y, r * r.z)
}

impl Inertial {
    /// A body of `density` kg/m³ whose unit-density properties are
    /// `volume`, `centroid` and `inertia`.
    pub fn of_body(density: f64, volume: f64, centroid: DVec3, inertia: [[f64; 3]; 3]) -> Inertial {
        Inertial {
            mass: density * volume,
            centroid: centroid.to_array(),
            inertia: rows(mat(inertia) * density),
        }
    }

    /// The same, seen from `frame`'s children: the thing is in `frame`'s
    /// own coordinates and the result is in the frame `frame` is given in.
    pub fn moved(&self, frame: &Frame) -> Inertial {
        let rotation = DMat3::from_cols(frame.x_axis(), frame.y_axis(), frame.z_axis());
        let centroid = rotation * DVec3::from_array(self.centroid) + frame.origin();
        Inertial {
            mass: self.mass,
            centroid: centroid.to_array(),
            inertia: rows(rotation * mat(self.inertia) * rotation.transpose()),
        }
    }

    /// The inertia about `point`, with the same axes, by the
    /// parallel-axis theorem.
    pub fn inertia_about(&self, point: [f64; 3]) -> [[f64; 3]; 3] {
        let r = DVec3::from_array(point) - DVec3::from_array(self.centroid);
        rows(mat(self.inertia) + shift(r) * self.mass)
    }

    /// Everything in `parts` as one: the masses added, the centroid their
    /// weighted mean, the inertia the parts' about it. `None` for no mass.
    pub fn sum(parts: &[Inertial]) -> Option<Inertial> {
        let mass: f64 = parts.iter().map(|p| p.mass).sum();
        if parts.is_empty() || mass <= 0.0 {
            return None;
        }
        let centroid = parts
            .iter()
            .map(|p| DVec3::from_array(p.centroid) * p.mass)
            .sum::<DVec3>()
            / mass;
        let inertia = parts
            .iter()
            .map(|p| mat(p.inertia_about(centroid.to_array())))
            .fold(DMat3::ZERO, |a, b| a + b);
        Some(Inertial {
            mass,
            centroid: centroid.to_array(),
            inertia: rows(inertia),
        })
    }
}

/// One final body's mass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BodyMass {
    pub part: arrix_core::PartId,
    pub slot: crate::eval::BodySlot,
    /// Cubic metres.
    pub volume: f64,
    /// The centre of the volume in the part's frame, which is the centre of
    /// mass for a uniform body; known without a material.
    pub centroid: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<MaterialId>,
    /// In the part's frame; none when the body has no density
    /// (`diagnostic` says why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<Inertial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<Diagnostic>,
}

/// A part's or an assembly's total: what could be summed, and whether
/// something could not (a body without a material, a component unplaced).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MassOutcome {
    /// Cubic metres, of every body, whether or not it has a mass.
    pub volume: f64,
    /// In the part's or assembly's frame; none when nothing has a mass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<Inertial>,
    /// Something is left out of `mass`: it is a lower bound.
    pub partial: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-12;

    fn near(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) {
        for r in 0..3 {
            for c in 0..3 {
                assert!(
                    (a[r][c] - b[r][c]).abs() <= EPS * (1.0 + b[r][c].abs()),
                    "{a:?} != {b:?}"
                );
            }
        }
    }

    fn diag(x: f64, y: f64, z: f64) -> [[f64; 3]; 3] {
        [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, z]]
    }

    /// A box a x b x c of density d: m = d a b c, Ixx = m (b² + c²) / 12.
    fn cuboid(d: f64, a: f64, b: f64, c: f64, at: DVec3) -> Inertial {
        let m = d * a * b * c;
        Inertial {
            mass: m,
            centroid: at.to_array(),
            inertia: diag(
                m * (b * b + c * c) / 12.0,
                m * (a * a + c * c) / 12.0,
                m * (a * a + b * b) / 12.0,
            ),
        }
    }

    #[test]
    fn a_density_scales_unit_properties() {
        let unit = diag(2.0, 3.0, 4.0);
        let i = Inertial::of_body(7850.0, 0.5, DVec3::new(1.0, 2.0, 3.0), unit);
        assert_eq!(i.mass, 3925.0);
        assert_eq!(i.centroid, [1.0, 2.0, 3.0]);
        near(i.inertia, diag(15700.0, 23550.0, 31400.0));
    }

    #[test]
    fn the_parallel_axis_theorem_moves_the_axis() {
        // A cylinder (m, r) about a parallel axis d away: m (r²/2 + d²).
        let (m, r, d) = (3.0, 0.1, 0.4);
        let cyl = Inertial {
            mass: m,
            centroid: [0.0; 3],
            inertia: diag(0.0, 0.0, m * r * r / 2.0),
        };
        let about = cyl.inertia_about([d, 0.0, 0.0]);
        assert!((about[2][2] - m * (r * r / 2.0 + d * d)).abs() < EPS);
        assert!((about[1][1] - m * d * d).abs() < EPS, "the y moment too");
    }

    #[test]
    fn two_boxes_sum_about_their_common_centre() {
        // Two equal cubes (side s, mass m) centred at x = -h and x = +h:
        // Ixx = 2 * m s²/6, Iyy = Izz = 2 * (m s²/6 + m h²).
        let (s, h, d) = (0.1, 0.3, 1000.0);
        let m = d * s * s * s;
        let a = cuboid(d, s, s, s, DVec3::new(-h, 0.0, 0.0));
        let b = cuboid(d, s, s, s, DVec3::new(h, 0.0, 0.0));
        let both = Inertial::sum(&[a, b]).unwrap();
        assert!((both.mass - 2.0 * m).abs() < EPS);
        assert_eq!(both.centroid, [0.0; 3]);
        let own = m * s * s / 6.0;
        near(
            both.inertia,
            diag(2.0 * own, 2.0 * (own + m * h * h), 2.0 * (own + m * h * h)),
        );
    }

    #[test]
    fn a_frame_rotates_the_inertia_and_moves_the_centre() {
        // A bar along x, turned a quarter about z, lies along y: its moment
        // about x and y swap, its centre goes to (0, 1, 0) + the origin.
        let bar = cuboid(1000.0, 0.4, 0.02, 0.02, DVec3::new(1.0, 0.0, 0.0));
        let frame = Frame::new(DVec3::new(0.0, 0.0, 5.0), DVec3::Y, DVec3::Z).unwrap();
        let moved = bar.moved(&frame);
        for (got, want) in moved.centroid.iter().zip([0.0, 1.0, 5.0]) {
            assert!((got - want).abs() < EPS);
        }
        assert!((moved.inertia[0][0] - bar.inertia[1][1]).abs() < EPS);
        assert!((moved.inertia[1][1] - bar.inertia[0][0]).abs() < EPS);
        assert!((moved.inertia[2][2] - bar.inertia[2][2]).abs() < EPS);
        assert_eq!(moved.mass, bar.mass);
    }

    #[test]
    fn nothing_with_no_mass_sums_to_none() {
        assert_eq!(Inertial::sum(&[]), None);
    }
}
