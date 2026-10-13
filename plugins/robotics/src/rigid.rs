//! Rigid transforms in the plugin's own arithmetic: a rotation as rows, a
//! translation. Only `+ - * /`, `sqrt` and `arrix_plugin_api::math`, so the
//! same bits come out at every tier (ADR-0036).

use arrix_plugin_api::{Frame, math};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    /// Rows of the rotation matrix; the columns are the frame's axes.
    pub r: [[f64; 3]; 3],
    pub t: [f64; 3],
}

impl Rigid {
    pub const IDENTITY: Rigid = Rigid {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    /// The transform that carries the world's axes onto the frame's.
    pub fn of(frame: &Frame) -> Self {
        let (o, x, y, z) = (
            frame.origin(),
            frame.x_axis(),
            frame.y_axis(),
            frame.z_axis(),
        );
        Self {
            r: [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]],
            t: [o.x, o.y, o.z],
        }
    }

    /// A turn of `angle` about z.
    pub fn about_z(angle: f64) -> Self {
        let (s, c) = math::sincos(angle);
        Self {
            r: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }

    /// A slide of `distance` along z.
    pub fn along_z(distance: f64) -> Self {
        Self {
            t: [0.0, 0.0, distance],
            ..Self::IDENTITY
        }
    }

    /// `self` after `other`: `other` is expressed in `self`'s frame.
    pub fn then(&self, other: &Rigid) -> Rigid {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.r[i][k] * other.r[k][j]).sum();
            }
        }
        Rigid {
            r,
            t: self.apply(other.t),
        }
    }

    pub fn inverse(&self) -> Rigid {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = self.r[j][i];
            }
        }
        let inv = Rigid { r, t: [0.0; 3] };
        let t = inv.apply(self.t);
        Rigid {
            r,
            t: [-t[0], -t[1], -t[2]],
        }
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let row = |i: usize| self.r[i][0] * p[0] + self.r[i][1] * p[1] + self.r[i][2] * p[2];
        [row(0) + self.t[0], row(1) + self.t[1], row(2) + self.t[2]]
    }

    /// URDF's fixed-axis roll, pitch and yaw: `R = Rz(yaw) Ry(pitch)
    /// Rx(roll)`.
    pub fn rpy(&self) -> [f64; 3] {
        let r = &self.r;
        let cos_pitch = math::hypot(r[0][0], r[1][0]);
        let pitch = math::atan2(-r[2][0], cos_pitch);
        if cos_pitch > 1e-9 {
            [
                math::atan2(r[2][1], r[2][2]),
                pitch,
                math::atan2(r[1][0], r[0][0]),
            ]
        } else {
            // Gimbal lock: the yaw is folded into the roll.
            [math::atan2(-r[1][2], r[1][1]), pitch, 0.0]
        }
    }

    /// Whether every entry is within `tol` of `other`'s.
    pub fn near(&self, other: &Rigid, tol: f64) -> bool {
        let rows = self.r.iter().flatten().zip(other.r.iter().flatten());
        let ts = self.t.iter().zip(other.t.iter());
        rows.chain(ts).all(|(a, b)| (a - b).abs() <= tol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Rz(yaw) Ry(pitch) Rx(roll)`.
    fn from_rpy([roll, pitch, yaw]: [f64; 3]) -> Rigid {
        let (sr, cr) = math::sincos(roll);
        let (sp, cp) = math::sincos(pitch);
        let (sy, cy) = math::sincos(yaw);
        Rigid {
            r: [
                [cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr],
                [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr],
                [-sp, cp * sr, cp * cr],
            ],
            t: [0.0; 3],
        }
    }

    #[test]
    fn rpy_round_trips() {
        for rpy in [[0.3, -0.4, 2.0], [-2.5, 1.0, -0.7], [0.0, 0.0, 0.0]] {
            assert!(
                from_rpy(rpy).near(&from_rpy(from_rpy(rpy).rpy()), 1e-12),
                "{rpy:?}"
            );
        }
        // Gimbal lock: a different triple, the same rotation.
        let locked = from_rpy([0.4, std::f64::consts::FRAC_PI_2, 0.9]);
        assert!(locked.near(&from_rpy(locked.rpy()), 1e-12));
    }

    #[test]
    fn the_inverse_undoes_a_transform() {
        let a = Rigid::about_z(0.7).then(&Rigid::along_z(0.2));
        let a = Rigid {
            t: [1.0, 2.0, 3.0],
            ..a
        };
        assert!(a.then(&a.inverse()).near(&Rigid::IDENTITY, 1e-12));
    }
}
