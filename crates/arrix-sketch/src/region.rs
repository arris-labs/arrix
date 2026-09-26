//! Closed-loop region detection: the faces of the sketch's planar
//! arrangement, as profiles with holes (docs/DATA-MODEL.md §Sketches).
//!
//! This module is the facade; [`crate::arrangement`] does the work. Curves
//! that cross without sharing a point are intersected and cut into pieces, a
//! tangent-ordered half-edge walk bounds the faces those pieces make, and
//! [`crate::arrangement::nest`] sorts the faces into profiles and holes. A
//! region is therefore whatever the drawing actually encloses:
//! a circle crossed by a chord is a cap and a D, neither of which the
//! coincidence walk this replaced could see at all.
//!
//! Ellipses are the exception. A conic has no arrangement curve — it is
//! never intersected, split or walked —
//! so each closed one contributes its own sampled disc and reversed rim, and
//! nests by the same rule. An *open* conic bounds nothing, as an unwalked
//! curve must.

use serde::{Deserialize, Serialize};

use crate::arrangement::RegionOutline;
use crate::ids::EntityId;
use crate::sketch::Sketch;

/// One loop edge's own geometry, when it is a **piece** of a split entity
/// rather than the whole of it. In the sketch's own `(u, v)`; arcs run CCW
/// from `start_angle` by `sweep`, the direction the entity itself runs in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeGeom {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        sweep: f64,
    },
}

/// Which piece of a split entity a loop edge is, and what that piece trimmed
/// out of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePiece {
    /// Position along the entity from its own start, numbered from 0 — the
    /// index a side face is named with.
    pub index: u32,
    /// Already trimmed, so a consumer applies nothing but [`LoopEdge`]'s
    /// traversal direction.
    pub geom: EdgeGeom,
}

/// One oriented use of a sketch curve in a loop.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopEdge {
    pub entity: EntityId,
    /// `true` = traverse start→end (or the curve's native, CCW direction).
    pub forward: bool,
    /// `None` when the edge is the whole entity — which is every edge of a
    /// sketch whose curves do not cross, so an uncut sketch's loops, and the
    /// face names built from them, are exactly what they always were.
    pub piece: Option<EdgePiece>,
}

/// A simple closed boundary, CCW for outer loops and CW for holes when the
/// sketch's Y axis points up (standard computational-geometry convention).
#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub edges: Vec<LoopEdge>,
    /// Signed area in m² (positive = CCW). Closed-form: an arc contributes
    /// its exact term, not a sampled rim.
    pub signed_area: f64,
}

/// A face: one outer loop plus zero or more hole loops.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub outer: Loop,
    pub holes: Vec<Loop>,
}

/// Which face of the arrangement a feature was given.
///
/// Every entity bounding the face, its holes' included, sorted, plus a
/// point inside it. The entity set alone does not name a face — a circle
/// and its chord bound the cap and the D alike — so the sample is what
/// tells two faces of the same curves apart. Holes are in the key: a plate
/// whose bore is deleted is another region, the plain rectangle, and a key
/// that resolved to it would be the nearest-region fallback
/// docs/DATA-MODEL.md §Sketches forbids. The price is the other direction:
/// a hole drawn inside a region later makes it another region too, and the
/// feature holding the key is re-picked.
///
/// A key is geometric where it has to be, and that is its one residue: an
/// edit that sweeps a boundary *across* the sample moves the key onto the
/// face on the other side. The sample is taken with as much clearance from
/// the boundary as the face allows, which is what keeps an ordinary
/// dimension edit on the right side of it, but nothing here makes a
/// coordinate into an identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionKey {
    /// Sorted and deduplicated, so two keys of one face compare equal
    /// whichever order their loops were walked in.
    pub entities: Vec<EntityId>,
    /// A point inside the face, in the sketch's own `(u, v)`.
    pub sample: [f64; 2],
}

impl RegionKey {
    pub fn new(entities: impl IntoIterator<Item = EntityId>, sample: [f64; 2]) -> RegionKey {
        let mut entities: Vec<EntityId> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        RegionKey { entities, sample }
    }

    /// The region this key names *now*, or `None` when the sketch no longer
    /// has it: its entities are gone, they no longer bound one face
    /// together, or the sample fell outside every face they do bound. A
    /// feature holding such a key fails soft with a diagnostic and is
    /// re-picked — never a silent fall back to another region.
    ///
    /// Exactly one region or none: a key that two faces answer to is as
    /// lost as one that none does (docs/DATA-MODEL.md §Sketches).
    pub fn resolve(&self, sketch: &Sketch) -> Option<Region> {
        let mut found = keyed_faces(sketch).into_iter().filter(|(region, rims)| {
            region.key.entities == self.entities && rims.contains(self.sample)
        });
        let (region, _) = found.next()?;
        found.next().is_none().then_some(region)
    }
}

/// One face of the arrangement: what it encloses, and the key that names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub profile: Profile,
    pub key: RegionKey,
}

/// Every region in the sketch, largest-area first — the same faces and the
/// same order [`find_profiles`] reports, each with the key a feature stores
/// to come back to it.
pub fn find_regions(sketch: &Sketch) -> Vec<Region> {
    keyed_faces(sketch)
        .into_iter()
        .map(|(region, _)| region)
        .collect()
}

/// All profiles found in the sketch, largest-area first.
pub fn find_profiles(sketch: &Sketch) -> Vec<Profile> {
    find_regions(sketch)
        .into_iter()
        .map(|region| region.profile)
        .collect()
}

/// Every region with the boundary polylines it was keyed from, largest-area
/// first: [`find_regions`] with the geometry a caller needs to *draw* a
/// region or to test a point against it, which the loops alone do not carry
/// (the overlay's shading and its hover, docs/UI-RENDERING.md §Sketch mode).
/// Resolving a stored key goes through the same function, so a key
/// is tested against exactly the rims that produced one.
pub fn find_region_outlines(sketch: &Sketch) -> Vec<(Region, RegionOutline)> {
    keyed_faces(sketch)
}

fn keyed_faces(sketch: &Sketch) -> Vec<(Region, RegionOutline)> {
    let mut loops = crate::arrangement::face_loops(sketch);
    loops.extend(conic_loops(sketch));
    crate::arrangement::nest(loops)
}

/// Closed conics close by themselves, outside the arrangement: each is a CCW
/// disc and its CW twin, the same pair the walk gives a circle, so the
/// nesting rule treats both kinds alike. Construction geometry bounds
/// nothing.
#[cfg(feature = "conics")]
fn conic_loops(sketch: &Sketch) -> Vec<crate::arrangement::FaceLoop> {
    use std::f64::consts::{PI, TAU};

    use arrix_core::LENGTH_TOLERANCE;

    use crate::arrangement::{FaceLoop, SAMPLES_PER_TURN};
    use crate::entity::Entity;

    let mut loops = Vec::new();
    for (id, entity) in sketch.profile_entities() {
        let Entity::Ellipse {
            center,
            major_axis_end,
            minor_radius,
        } = entity
        else {
            continue;
        };
        let major = sketch.points[center].distance_to(&sketch.points[major_axis_end]);
        if major <= LENGTH_TOLERANCE || *minor_radius <= LENGTH_TOLERANCE {
            continue;
        }
        let at = ellipse_point(
            sketch.points[center].pos(),
            sketch.points[major_axis_end].pos(),
            *minor_radius,
        );
        let ccw: Vec<[f64; 2]> = (0..SAMPLES_PER_TURN)
            .map(|i| at(TAU * i as f64 / SAMPLES_PER_TURN as f64))
            .collect();
        let area = PI * major * minor_radius;
        for forward in [true, false] {
            let mut rim = ccw.clone();
            if !forward {
                rim.reverse();
            }
            loops.push(FaceLoop {
                lp: Loop {
                    edges: vec![LoopEdge {
                        entity: *id,
                        forward,
                        piece: None,
                    }],
                    signed_area: if forward { area } else { -area },
                },
                rim,
            });
        }
    }
    loops
}

#[cfg(not(feature = "conics"))]
fn conic_loops(_sketch: &Sketch) -> Vec<crate::arrangement::FaceLoop> {
    Vec::new()
}

/// The point at parametric angle `tau` on the ellipse centred at `c` whose
/// major axis ends at `m`.
#[cfg(feature = "conics")]
fn ellipse_point(c: [f64; 2], m: [f64; 2], minor_radius: f64) -> impl Fn(f64) -> [f64; 2] {
    let major_radius = (m[0] - c[0]).hypot(m[1] - c[1]);
    let (sin_th, cos_th) = (m[1] - c[1]).atan2(m[0] - c[0]).sin_cos();
    move |tau| {
        let (lx, ly) = (major_radius * tau.cos(), minor_radius * tau.sin());
        [
            c[0] + lx * cos_th - ly * sin_th,
            c[1] + lx * sin_th + ly * cos_th,
        ]
    }
}
