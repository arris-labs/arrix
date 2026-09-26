//! A region as the kernel sweeps it: an `arrix_core::Profile` on the
//! sketch's plane whose every curve is keyed by the sketch entity it came
//! from, so a side face is named by the entity that swept it
//! (docs/DATA-MODEL.md §Persistent naming).
//!
//! **The key rule.** A curve that is a whole entity is keyed by the
//! entity's id. A curve that is a piece of an entity the arrangement cut is
//! keyed by [`curve_key`] of the entity and the piece's number along it,
//! so an entity bounding one region in two pieces gives two keys, and a
//! profile never holds a key twice. An uncrossed sketch keys every curve
//! by its entity id alone. Cutting an entity that was whole (a line drawn
//! across it) re-keys its curves, and what named them is a lost reference,
//! re-picked rather than re-bound.

use arrix_core::{CurveKey, DVec2, Frame, Id};

use crate::arrangement::curve::Curve;
use crate::ids::EntityId;
use crate::region::{EdgeGeom, Loop, LoopEdge, Region};
use crate::sketch::Sketch;

/// Why a region could not become a profile.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RegionProfileError {
    /// A conic bounds the region, and a profile carries only lines and
    /// circular arcs.
    #[error("entity {0} is not a line, arc or circle a profile can carry")]
    Unsupported(EntityId),
    #[error(transparent)]
    Profile(#[from] arrix_core::geom::ProfileError),
}

/// The key of the curve a region's edge is: the entity's own id when the
/// edge is the whole entity, and for piece `index` of a cut entity the
/// entity's id mixed with the piece number by SplitMix64's finaliser. The
/// mix is a bijection, so two pieces of one entity never share a key; the
/// value is pinned by a test, since documents name faces by it.
pub fn curve_key(entity: EntityId, piece: Option<u32>) -> CurveKey {
    let Some(index) = piece else {
        return CurveKey(entity.0);
    };
    let mut z = entity.0.0 ^ (u64::from(index) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    CurveKey(Id(z ^ (z >> 31)))
}

impl Region {
    /// This region on `plane`, whose X and Y are the sketch's u and v: the
    /// outer loop and the holes as the arrangement walked them, every curve
    /// keyed by [`curve_key`].
    pub fn to_profile(
        &self,
        sketch: &Sketch,
        plane: Frame,
    ) -> Result<arrix_core::Profile, RegionProfileError> {
        let outer = profile_loop(sketch, &self.profile.outer)?;
        let holes = self
            .profile
            .holes
            .iter()
            .map(|l| profile_loop(sketch, l))
            .collect::<Result<_, _>>()?;
        Ok(arrix_core::Profile::new(plane, outer, holes)?)
    }
}

fn uv(p: [f64; 2]) -> DVec2 {
    DVec2::new(p[0], p[1])
}

/// An edge's own curve: the piece's when the entity was cut, the entity's
/// otherwise.
fn edge_curve(sketch: &Sketch, edge: &LoopEdge) -> Result<Curve, RegionProfileError> {
    match edge.piece {
        Some(piece) => Ok(match piece.geom {
            EdgeGeom::Line { a, b } => Curve::Line { a, b },
            EdgeGeom::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => Curve::Arc {
                center,
                radius,
                start_angle,
                sweep,
            },
        }),
        None => Curve::from_entity(sketch, edge.entity)
            .ok_or(RegionProfileError::Unsupported(edge.entity)),
    }
}

fn profile_loop(sketch: &Sketch, lp: &Loop) -> Result<arrix_core::ProfileLoop, RegionProfileError> {
    let curves: Vec<(CurveKey, Curve, bool)> = lp
        .edges
        .iter()
        .map(|e| {
            let key = curve_key(e.entity, e.piece.map(|p| p.index));
            Ok((key, edge_curve(sketch, e)?, e.forward))
        })
        .collect::<Result<_, RegionProfileError>>()?;
    // A loop of one closed curve: a whole circle, or a circle cut at one
    // point, whose one piece is a full turn.
    if let [(key, curve, _)] = curves[..]
        && let Some((center, radius)) = curve.circle()
        && (curve.is_closed() || curve.length() >= std::f64::consts::TAU * radius - 1e-12)
    {
        return Ok(arrix_core::ProfileLoop::Circle {
            key,
            center: uv(center),
            radius,
        });
    }
    // Along the walk: a backward edge runs from its curve's end.
    let at = |curve: &Curve, forward: bool, t: f64| {
        uv(curve.point_at(if forward { t } else { 1.0 - t }))
    };
    let (_, first, forward) = curves.first().expect("a loop has an edge");
    let start = at(first, *forward, 0.0);
    let segments = curves
        .iter()
        .map(|(key, curve, forward)| match curve {
            Curve::Line { .. } => arrix_core::ProfileSegment::Line {
                key: *key,
                to: at(curve, *forward, 1.0),
            },
            Curve::Arc { .. } | Curve::Circle { .. } => arrix_core::ProfileSegment::Arc {
                key: *key,
                to: at(curve, *forward, 1.0),
                via: at(curve, *forward, 0.5),
            },
        })
        .collect();
    Ok(arrix_core::ProfileLoop::Path { start, segments })
}
