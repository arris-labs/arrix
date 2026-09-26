//! Plain geometry that crosses the kernel and plugin boundaries: frames and
//! keyed profiles, in SI (docs/ARCHITECTURE.md §The kernel choke point).
//! No kernel type appears here; `arrix-kernel` translates both ways.

use std::collections::BTreeSet;

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

use crate::CurveKey;

/// How far from orthogonal a frame's two given axes may be, as the cosine
/// of the angle between them. Tighter than any modelling tolerance, so a
/// frame built from unit vectors by rotation always passes.
const ORTHOGONAL_COSINE: f64 = 1e-9;

/// A right-handed frame: an origin and unit X and Z axes, Y = Z × X. A
/// plane is the frame's XY plane, its normal Z.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawFrame", into = "RawFrame")]
pub struct Frame {
    origin: DVec3,
    x_axis: DVec3,
    z_axis: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct RawFrame {
    origin: DVec3,
    x_axis: DVec3,
    z_axis: DVec3,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FrameError {
    #[error("a frame's {0} is not finite")]
    NotFinite(&'static str),
    #[error("a frame's {0} has zero length")]
    ZeroAxis(&'static str),
    #[error("a frame's X and Z axes are not orthogonal (cosine {0})")]
    NotOrthogonal(f64),
}

impl Frame {
    /// World XY: the origin, X along +x, normal +z (Z-up, `SEED.md` §8.2).
    pub const WORLD_XY: Frame = Frame {
        origin: DVec3::ZERO,
        x_axis: DVec3::X,
        z_axis: DVec3::Z,
    };
    /// World YZ: X along +y, normal +x.
    pub const WORLD_YZ: Frame = Frame {
        origin: DVec3::ZERO,
        x_axis: DVec3::Y,
        z_axis: DVec3::X,
    };
    /// World ZX: X along +z, normal +y.
    pub const WORLD_ZX: Frame = Frame {
        origin: DVec3::ZERO,
        x_axis: DVec3::Z,
        z_axis: DVec3::Y,
    };

    /// A frame from an origin and two orthogonal axes of any length, which
    /// are normalised.
    pub fn new(origin: DVec3, x_axis: DVec3, z_axis: DVec3) -> Result<Self, FrameError> {
        for (what, v) in [("origin", origin), ("X axis", x_axis), ("Z axis", z_axis)] {
            if !v.is_finite() {
                return Err(FrameError::NotFinite(what));
            }
        }
        let x = x_axis
            .try_normalize()
            .ok_or(FrameError::ZeroAxis("X axis"))?;
        let z = z_axis
            .try_normalize()
            .ok_or(FrameError::ZeroAxis("Z axis"))?;
        let cosine = x.dot(z);
        if cosine.abs() > ORTHOGONAL_COSINE {
            return Err(FrameError::NotOrthogonal(cosine));
        }
        Ok(Self {
            origin,
            x_axis: x,
            z_axis: z,
        })
    }

    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    pub fn x_axis(&self) -> DVec3 {
        self.x_axis
    }

    pub fn y_axis(&self) -> DVec3 {
        self.z_axis.cross(self.x_axis)
    }

    pub fn z_axis(&self) -> DVec3 {
        self.z_axis
    }

    /// The same frame moved `distance` metres along its Z.
    pub fn offset(&self, distance: f64) -> Self {
        Self {
            origin: self.origin + self.z_axis * distance,
            ..*self
        }
    }

    /// A point of the frame's plane, from its (u, v) coordinates.
    pub fn point(&self, uv: DVec2) -> DVec3 {
        self.origin + self.x_axis * uv.x + self.y_axis() * uv.y
    }
}

impl TryFrom<RawFrame> for Frame {
    type Error = FrameError;

    fn try_from(raw: RawFrame) -> Result<Self, FrameError> {
        Frame::new(raw.origin, raw.x_axis, raw.z_axis)
    }
}

impl From<Frame> for RawFrame {
    fn from(f: Frame) -> Self {
        RawFrame {
            origin: f.origin,
            x_axis: f.x_axis,
            z_axis: f.z_axis,
        }
    }
}

/// A planar region to sweep: an outer loop and holes, in the (u, v)
/// coordinates of `plane`, every curve keyed. Whether the loops close,
/// nest and avoid each other is the kernel's to check; this type checks
/// only what naming needs: numbers finite, radii positive, keys unique.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawProfile", into = "RawProfile")]
pub struct Profile {
    plane: Frame,
    outer: ProfileLoop,
    holes: Vec<ProfileLoop>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct RawProfile {
    plane: Frame,
    outer: ProfileLoop,
    holes: Vec<ProfileLoop>,
}

/// One closed loop: a full circle, or a chain of segments from `start`
/// that returns there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLoop {
    Circle {
        key: CurveKey,
        center: DVec2,
        radius: f64,
    },
    Path {
        start: DVec2,
        segments: Vec<ProfileSegment>,
    },
}

/// A segment of a path, from where the previous one ended. Its key names
/// the side face it sweeps, and the vertex it starts at names the edges
/// and vertices there.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSegment {
    Line {
        key: CurveKey,
        to: DVec2,
    },
    /// A circular arc through `via` to `to`.
    Arc {
        key: CurveKey,
        to: DVec2,
        via: DVec2,
    },
}

impl ProfileSegment {
    pub fn key(&self) -> CurveKey {
        match self {
            ProfileSegment::Line { key, .. } | ProfileSegment::Arc { key, .. } => *key,
        }
    }

    pub fn end(&self) -> DVec2 {
        match self {
            ProfileSegment::Line { to, .. } | ProfileSegment::Arc { to, .. } => *to,
        }
    }
}

impl ProfileLoop {
    /// The loop's curve keys, in walking order.
    pub fn keys(&self) -> Vec<CurveKey> {
        match self {
            ProfileLoop::Circle { key, .. } => vec![*key],
            ProfileLoop::Path { segments, .. } => segments.iter().map(|s| s.key()).collect(),
        }
    }

    fn check(&self) -> Result<(), ProfileError> {
        match self {
            ProfileLoop::Circle { center, radius, .. } => {
                if !center.is_finite() || !radius.is_finite() {
                    return Err(ProfileError::NotFinite);
                }
                if *radius <= 0.0 {
                    return Err(ProfileError::Radius(*radius));
                }
            }
            ProfileLoop::Path { start, segments } => {
                if segments.is_empty() {
                    return Err(ProfileError::EmptyPath);
                }
                let finite = start.is_finite()
                    && segments.iter().all(|s| match s {
                        ProfileSegment::Line { to, .. } => to.is_finite(),
                        ProfileSegment::Arc { to, via, .. } => to.is_finite() && via.is_finite(),
                    });
                if !finite {
                    return Err(ProfileError::NotFinite);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ProfileError {
    #[error("a profile coordinate is not finite")]
    NotFinite,
    #[error("a profile circle's radius is {0}, not positive")]
    Radius(f64),
    #[error("a profile path has no segments")]
    EmptyPath,
    #[error("the curve key {0} appears twice in one profile")]
    DuplicateKey(CurveKey),
}

impl Profile {
    pub fn new(
        plane: Frame,
        outer: ProfileLoop,
        holes: Vec<ProfileLoop>,
    ) -> Result<Self, ProfileError> {
        let mut seen = BTreeSet::new();
        for l in std::iter::once(&outer).chain(&holes) {
            l.check()?;
            for key in l.keys() {
                if !seen.insert(key) {
                    return Err(ProfileError::DuplicateKey(key));
                }
            }
        }
        Ok(Self {
            plane,
            outer,
            holes,
        })
    }

    pub fn plane(&self) -> &Frame {
        &self.plane
    }

    pub fn outer(&self) -> &ProfileLoop {
        &self.outer
    }

    pub fn holes(&self) -> &[ProfileLoop] {
        &self.holes
    }

    /// Every loop, the outer first.
    pub fn loops(&self) -> impl Iterator<Item = &ProfileLoop> {
        std::iter::once(&self.outer).chain(&self.holes)
    }
}

impl TryFrom<RawProfile> for Profile {
    type Error = ProfileError;

    fn try_from(raw: RawProfile) -> Result<Self, ProfileError> {
        Profile::new(raw.plane, raw.outer, raw.holes)
    }
}

impl From<Profile> for RawProfile {
    fn from(p: Profile) -> Self {
        RawProfile {
            plane: p.plane,
            outer: p.outer,
            holes: p.holes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Id;

    fn key(n: u64) -> CurveKey {
        CurveKey(Id(n))
    }

    #[test]
    fn a_frame_is_right_handed_and_normalised() {
        let f = Frame::new(DVec3::new(0.0, 0.0, 0.01), DVec3::X * 3.0, DVec3::Z * 0.5).unwrap();
        assert_eq!(f.x_axis(), DVec3::X);
        assert_eq!(f.y_axis(), DVec3::Y);
        assert_eq!(f.offset(0.002).origin(), DVec3::new(0.0, 0.0, 0.012));
        assert_eq!(Frame::WORLD_YZ.y_axis(), DVec3::Z);
        assert_eq!(Frame::WORLD_ZX.y_axis(), DVec3::X);
        assert_eq!(f.point(DVec2::new(1.0, 2.0)), DVec3::new(1.0, 2.0, 0.01));
    }

    #[test]
    fn a_frame_refuses_what_is_not_one() {
        let o = DVec3::ZERO;
        assert_eq!(
            Frame::new(o, DVec3::ZERO, DVec3::Z),
            Err(FrameError::ZeroAxis("X axis"))
        );
        assert!(matches!(
            Frame::new(o, DVec3::X, DVec3::new(1.0, 0.0, 1.0)),
            Err(FrameError::NotOrthogonal(_))
        ));
        assert_eq!(
            Frame::new(DVec3::NAN, DVec3::X, DVec3::Z),
            Err(FrameError::NotFinite("origin"))
        );
        let bad = r#"{"origin":[0,0,0],"x_axis":[1,0,0],"z_axis":[1,0,0]}"#;
        assert!(serde_json::from_str::<Frame>(bad).is_err());
    }

    #[test]
    fn a_frame_crosses_as_json() {
        let json = serde_json::to_string(&Frame::WORLD_XY).unwrap();
        assert_eq!(
            json,
            r#"{"origin":[0.0,0.0,0.0],"x_axis":[1.0,0.0,0.0],"z_axis":[0.0,0.0,1.0]}"#
        );
        assert_eq!(
            serde_json::from_str::<Frame>(&json).unwrap(),
            Frame::WORLD_XY
        );
    }

    fn plate() -> ProfileLoop {
        let p = |x, y| DVec2::new(x, y);
        ProfileLoop::Path {
            start: p(0.0, 0.0),
            segments: vec![
                ProfileSegment::Line {
                    key: key(1),
                    to: p(0.04, 0.0),
                },
                ProfileSegment::Arc {
                    key: key(2),
                    to: p(0.04, 0.03),
                    via: p(0.05, 0.015),
                },
                ProfileSegment::Line {
                    key: key(3),
                    to: p(0.0, 0.03),
                },
                ProfileSegment::Line {
                    key: key(4),
                    to: p(0.0, 0.0),
                },
            ],
        }
    }

    #[test]
    fn a_profile_checks_its_keys_and_numbers() {
        let bore = ProfileLoop::Circle {
            key: key(5),
            center: DVec2::new(0.02, 0.015),
            radius: 0.004,
        };
        let p = Profile::new(Frame::WORLD_XY, plate(), vec![bore.clone()]).unwrap();
        let keys: Vec<_> = p.loops().flat_map(|l| l.keys()).collect();
        assert_eq!(keys, (1..=5).map(key).collect::<Vec<_>>());
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Profile>(&json).unwrap(), p);

        let twice = ProfileLoop::Circle {
            key: key(2),
            center: DVec2::ZERO,
            radius: 0.001,
        };
        assert_eq!(
            Profile::new(Frame::WORLD_XY, plate(), vec![twice]),
            Err(ProfileError::DuplicateKey(key(2)))
        );
        let flat = ProfileLoop::Circle {
            key: key(9),
            center: DVec2::ZERO,
            radius: 0.0,
        };
        assert_eq!(
            Profile::new(Frame::WORLD_XY, flat, vec![]),
            Err(ProfileError::Radius(0.0))
        );
        let empty = ProfileLoop::Path {
            start: DVec2::ZERO,
            segments: vec![],
        };
        assert_eq!(
            Profile::new(Frame::WORLD_XY, empty, vec![]),
            Err(ProfileError::EmptyPath)
        );
        let nan = ProfileLoop::Circle {
            key: key(9),
            center: DVec2::NAN,
            radius: 1.0,
        };
        assert_eq!(
            Profile::new(Frame::WORLD_XY, nan, vec![]),
            Err(ProfileError::NotFinite)
        );
        let dup = json.replace(&key(5).to_string(), &key(1).to_string());
        assert!(serde_json::from_str::<Profile>(&dup).is_err());
    }
}
