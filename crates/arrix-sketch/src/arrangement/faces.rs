//! Faces and nesting: which of the arrangement's face boundaries bounds
//! material, and which is a hole of which.
//!
//! [`super::walk`] hands back every face: the bounded ones CCW, each
//! connected component's outside CW. A CCW face is a profile's outer
//! boundary. A CW face is a hole of the smallest outer that shares no entity
//! with it and contains its whole rim — a component's outside sitting inside
//! another component's material. The one rule covers a circle in a plate, a
//! washer, and a plate inside a ring alike; a CW face that shares an entity
//! with its candidate is that outer's own other side, never material to
//! remove from it.
//!
//! There is no `edges.len() >= 3` floor any more: a disc is one edge and a D
//! is two, and both are regions. The
//! floor only ever existed because the coincidence walk could not tell a
//! two-edge face from a curve walked out and back.
//!
//! Areas are the walk's closed-form ones. The sampled rim is read for
//! nothing but the containment tests, where an inscribed polygon is
//! conservative in the direction that matters — it never claims a hole is
//! inside an outer it pokes out of.
//!
//! The other containment test is a face's own [`RegionKey`]: a point inside
//! it, which is what tells two faces bounded by the same curves apart. That sample is
//! guaranteed-interior, never a centroid — a centroid of a D or an L lies
//! outside the face it is supposed to name.

use std::collections::BTreeSet;
use std::f64::consts::TAU;

use arrix_core::LENGTH_TOLERANCE;

use super::curve::Curve;
use super::walk::Arrangement;
use crate::ids::EntityId;
use crate::region::{EdgeGeom, EdgePiece, Loop, LoopEdge, Profile, Region, RegionKey};
use crate::sketch::Sketch;

/// Samples per full turn of a curved edge — the density every rim is built
/// at, whether it is a whole circle or a piece of one.
///
/// The containment tests only ask that it be conservative, and 8 per piece
/// was. The *drawing* asks for more: the overlay shades a region by
/// projecting these rims, and an inscribed
/// 16-gon left a visibly faceted fill inside a circle's own smooth stroke.
/// 64 per turn puts the fill under a third of a pixel inside the stroke at
/// the zoom a 20 mm circle fills the viewport at.
pub(crate) const SAMPLES_PER_TURN: usize = 64;

/// One candidate loop and the polyline its containment is tested with.
pub(crate) struct FaceLoop {
    pub(crate) lp: Loop,
    /// The boundary in traversal order, without its closing point.
    pub(crate) rim: Vec<[f64; 2]>,
}

/// One finished face's boundaries as closed polylines in the sketch's own
/// `(u, v)`, each without its repeated closing point.
///
/// It is what a point is tested against — to find the face's [`RegionKey`]
/// sample, and to resolve a stored key back to it — and it is also what a
/// caller with no curve maths of its own draws the face as: the overlay
/// shades a region by projecting these to screen space
/// (docs/UI-RENDERING.md §Sketch mode).
#[derive(Debug, Clone, PartialEq)]
pub struct RegionOutline {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
}

impl RegionOutline {
    /// Inside the outer boundary and outside every hole.
    pub fn contains(&self, p: [f64; 2]) -> bool {
        polygon_contains(&self.outer, p) && self.holes.iter().all(|h| !polygon_contains(h, p))
    }
}

/// Every face the arrangement bounds, in the walk's own order.
pub(crate) fn face_loops(sketch: &Sketch) -> Vec<FaceLoop> {
    let arrangement = Arrangement::build(sketch);
    arrangement
        .faces()
        .into_iter()
        .map(|face| {
            let signed_area = arrangement.signed_area(&face);
            let mut rim = Vec::new();
            let edges = face
                .iter()
                .map(|&h| {
                    let (curve, forward) = arrangement.curve_of(h);
                    rim.extend(rim_samples(curve, forward));
                    let piece = arrangement.piece_of(h);
                    LoopEdge {
                        entity: piece.entity,
                        forward,
                        piece: piece.index.map(|index| EdgePiece {
                            index,
                            geom: edge_geom(curve),
                        }),
                    }
                })
                .collect();
            FaceLoop {
                lp: Loop { edges, signed_area },
                rim,
            }
        })
        .collect()
}

/// Loops to regions: CCW outers largest first, each CW loop nested into the
/// tightest outer that contains it, and anything enclosing no area dropped —
/// which is every dangling stub the walk traverses out and back. Each
/// finished face is keyed as it is built, and its rims come back with it so
/// a stored key can be resolved against the same polylines the key was cut
/// from.
pub(crate) fn nest(loops: Vec<FaceLoop>) -> Vec<(Region, RegionOutline)> {
    let min_area = LENGTH_TOLERANCE * LENGTH_TOLERANCE;
    let (mut outers, mut holes) = (Vec::new(), Vec::new());
    for face in loops {
        if face.lp.signed_area > min_area {
            outers.push(face);
        } else if face.lp.signed_area < -min_area {
            holes.push(face);
        }
    }
    // Largest first. Areas equal to within the tolerance squared (a pattern
    // of equal holes, whose computed areas differ in the last bits) are
    // ordered by the entities that bound them, so the order is the sketch's
    // and never the walk's or the rounding's.
    let rank = |f: &FaceLoop| {
        let mut ids: Vec<EntityId> = f.lp.edges.iter().map(|e| e.entity).collect();
        ids.sort_unstable();
        ((f.lp.signed_area / min_area).round(), ids)
    };
    outers.sort_by(|a, b| {
        let ((area_a, ids_a), (area_b, ids_b)) = (rank(a), rank(b));
        area_b.total_cmp(&area_a).then_with(|| ids_a.cmp(&ids_b))
    });

    let mut faces: Vec<(Profile, RegionOutline)> = outers
        .into_iter()
        .map(|face| {
            (
                Profile {
                    outer: face.lp,
                    holes: Vec::new(),
                },
                RegionOutline {
                    outer: face.rim,
                    holes: Vec::new(),
                },
            )
        })
        .collect();

    // Outers are largest first, so the last one that qualifies is the
    // tightest. The whole rim has to be inside, not one sample: a circle's
    // centre can sit in the plate while its rim crosses the edge.
    for hole in holes {
        let entities: BTreeSet<EntityId> = hole.lp.edges.iter().map(|e| e.entity).collect();
        let container = faces.iter().rposition(|(profile, rims)| {
            profile
                .outer
                .edges
                .iter()
                .all(|e| !entities.contains(&e.entity))
                && hole.rim.iter().all(|&p| polygon_contains(&rims.outer, p))
        });
        if let Some(i) = container {
            faces[i].0.holes.push(hole.lp);
            faces[i].1.holes.push(hole.rim);
        }
    }

    faces
        .into_iter()
        .map(|(profile, rims)| {
            // A face too thin for any scan line to find a span inside is
            // degenerate; keying it on a rim point costs nothing and simply
            // does not resolve, which is the fail-soft answer.
            let sample = interior_sample(&rims).unwrap_or(rims.outer[0]);
            let bounding = std::iter::once(&profile.outer)
                .chain(&profile.holes)
                .flat_map(|lp| lp.edges.iter().map(|e| e.entity));
            let key = RegionKey::new(bounding, sample);
            (Region { profile, key }, rims)
        })
        .collect()
}

/// A point well inside the face — the sample a [`RegionKey`] names it by.
///
/// Horizontal scan lines, one between each pair of adjacent rim ordinates,
/// each cut into the spans that are inside the outer rim and outside every
/// hole. The winner is the span with the most clearance in both directions:
/// half its own width, against its distance to the face's top and bottom.
/// The widest span alone is not enough — for a circular cap the line hugging
/// its chord is the widest one, and a chord that then moves a little crosses
/// the sample and the key resolves to the face on the other side.
fn interior_sample(rims: &RegionOutline) -> Option<[f64; 2]> {
    let mut ys: Vec<f64> = rims.outer.iter().map(|p| p[1]).collect();
    ys.extend(rims.holes.iter().flatten().map(|p| p[1]));
    ys.sort_by(|a, b| a.total_cmp(b));
    ys.dedup_by(|a, b| (*a - *b).abs() <= LENGTH_TOLERANCE);
    let (bottom, top) = (*ys.first()?, *ys.last()?);

    let mut best: Option<([f64; 2], f64)> = None;
    for pair in ys.windows(2) {
        let y = 0.5 * (pair[0] + pair[1]);
        let Some((x, width)) = widest_span(rims, y) else {
            continue;
        };
        let clearance = (0.5 * width).min(y - bottom).min(top - y);
        if best.is_none_or(|(_, so_far)| clearance > so_far) {
            best = Some(([x, y], clearance));
        }
    }
    best.map(|(p, _)| p)
}

/// The widest stretch of the scan line at `y` that is inside the face, as
/// its midpoint and its width. Even-odd again: the crossings of the outer
/// rim and of every hole go in one list, so the spans left inside a washer
/// come out as two.
fn widest_span(rims: &RegionOutline, y: f64) -> Option<(f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    for rim in std::iter::once(&rims.outer).chain(&rims.holes) {
        for (a, b) in rim.iter().zip(rim.iter().cycle().skip(1)) {
            // The straddle test guarantees `b[1] != a[1]` in the division.
            if (a[1] > y) != (b[1] > y) {
                xs.push(a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]));
            }
        }
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    xs.as_chunks::<2>()
        .0
        .iter()
        .map(|span| (0.5 * (span[0] + span[1]), span[1] - span[0]))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// A piece as the loop edge reports its geometry: already trimmed, so a
/// consumer applies nothing but the traversal direction. A circle no cut
/// touched is a full `TAU` sweep from angle 0.
fn edge_geom(curve: &Curve) -> EdgeGeom {
    match *curve {
        Curve::Line { a, b } => EdgeGeom::Line { a, b },
        Curve::Circle { center, radius } => EdgeGeom::Arc {
            center,
            radius,
            start_angle: 0.0,
            sweep: TAU,
        },
        Curve::Arc {
            center,
            radius,
            start_angle,
            sweep,
        } => EdgeGeom::Arc {
            center,
            radius,
            start_angle,
            sweep,
        },
    }
}

/// The piece sampled in traversal order, its start included and its end left
/// to the next edge.
fn rim_samples(curve: &Curve, forward: bool) -> Vec<[f64; 2]> {
    let n = match *curve {
        Curve::Line { .. } => 1,
        Curve::Circle { .. } => SAMPLES_PER_TURN,
        // Proportional, so a rim's density does not depend on how many
        // pieces the curve happens to have been cut into.
        Curve::Arc { sweep, .. } => {
            ((SAMPLES_PER_TURN as f64 * sweep / TAU).ceil() as usize).max(2)
        }
    };
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            curve.point_at(if forward { t } else { 1.0 - t })
        })
        .collect()
}

/// Even-odd ray casting against a closed polygon given without its closing
/// point.
pub(crate) fn polygon_contains(pts: &[[f64; 2]], p: [f64; 2]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in pts.iter().zip(pts.iter().cycle().skip(1)) {
        // The straddle test guarantees `b[1] != a[1]` in the division.
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
