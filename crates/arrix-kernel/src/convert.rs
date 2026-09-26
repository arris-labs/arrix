//! The glam ↔ nalgebra boundary: `arrix-core`'s plain geometry to Arris's
//! and back. Every crossing is one of these functions.

use arris::geom;
use arris::math::{self as am, Point2, Point3, Vec3};
use arrix_core::{DVec2, DVec3, Frame, Profile, ProfileLoop, ProfileSegment};

pub(crate) fn point2(p: DVec2) -> Point2 {
    Point2::new(p.x, p.y)
}

pub(crate) fn vec3(v: DVec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

pub(crate) fn dvec3(v: Vec3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}

pub(crate) fn dpoint3(p: Point3) -> DVec3 {
    dvec3(p.coords)
}

/// Arris's frame of `f`. `arrix_core::Frame` is orthonormal to within
/// its own check, so Arris's re-orthonormalisation moves nothing that
/// matters and cannot fail.
pub(crate) fn frame(f: &Frame) -> am::Frame {
    am::Frame::new(
        Point3::from(vec3(f.origin())),
        vec3(f.z_axis()),
        vec3(f.x_axis()),
    )
    .expect("an arrix-core frame is finite and orthonormal")
}

/// `arrix_core`'s frame of an Arris one, which is orthonormal to rounding.
pub(crate) fn frame_back(f: &am::Frame) -> Frame {
    Frame::new(
        dpoint3(f.origin()),
        dvec3(f.x().into_inner()),
        dvec3(f.z().into_inner()),
    )
    .expect("an Arris frame is finite and orthonormal")
}

/// Arris's profile of `p`: the same loops in the same order, the keys
/// dropped. Loop `i` and segment `j` here are Arris's `loop_index` and
/// `segment`, which is how names find their keys again.
pub(crate) fn profile(p: &Profile) -> geom::Profile {
    geom::Profile {
        plane: frame(p.plane()),
        outer: profile_loop(p.outer()),
        holes: p.holes().iter().map(profile_loop).collect(),
    }
}

fn profile_loop(l: &ProfileLoop) -> geom::ProfileLoop {
    match l {
        ProfileLoop::Circle { center, radius, .. } => geom::ProfileLoop::Circle {
            center: point2(*center),
            radius: *radius,
        },
        ProfileLoop::Path { start, segments } => geom::ProfileLoop::Path {
            start: point2(*start),
            segments: segments
                .iter()
                .map(|s| match s {
                    ProfileSegment::Line { to, .. } => geom::ProfileSegment::LineTo(point2(*to)),
                    ProfileSegment::Arc { to, via, .. } => geom::ProfileSegment::ArcTo {
                        to: point2(*to),
                        via: point2(*via),
                    },
                })
                .collect(),
        },
    }
}
