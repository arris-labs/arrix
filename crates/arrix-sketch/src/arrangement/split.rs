//! Splitting: every non-construction curve cut into the pieces the walk runs
//! over.
//!
//! A piece is the stretch of one entity between two consecutive cuts, and
//! the cuts are the entity's intersections with every other curve in the
//! sketch. T-junctions need no pass of their own: an endpoint resting on
//! another curve *is* an intersection of the two, and it is reported as one
//! (`intersect`'s module docs). Conics and B-splines have no curve, so they
//! neither cut nor are cut, exactly as before.
//!
//! Pieces are ordered along the entity's own parameter and numbered from 0,
//! which is the index a side face is named with. An
//! entity that yields a single piece is unsplit and numbered `None`, so a
//! sketch with no crossings names its faces exactly as it does today —
//! including a circle merely touched by a tangent, which gains a vertex but
//! no second piece.
//!
//! **Where the numbering starts** is a naming question, not a geometric
//! one. An open curve starts at its own start point, which is a persistent
//! `PointId` and does not move. A closed one
//! has no such point — its parameter origin is angle 0, a coordinate — so
//! numbering from there would let a dimension edit that merely sweeps a
//! crossing past `(R, 0)` swap the D and the cap under a stored
//! side-face name, which is the silent re-bind `SEED.md` §8.2 forbids. A circle
//! therefore numbers its pieces from the crossing that comes first along
//! the **lowest-id entity that cuts it** ([`Cut::origin_key`]): ids are
//! persistent and never reused, so that origin follows the cutting curve
//! continuously instead of jumping when the geometry passes a coordinate.
//! Two residues are accepted rather than fixed — two closed curves cutting
//! each other put each one's origin on the other's parameter, and adding or
//! removing a crossing renumbers, though there every new piece is a subset
//! of the piece it subdivides (the plan's *Open questions*).

use super::curve::Curve;
use super::intersect::intersect;
use crate::ids::EntityId;
use crate::sketch::Sketch;

/// One cut on a curve, and which crossing put it there: where it falls in
/// this curve's parameter, plus the entity that cuts and where the same hit
/// falls in *that* entity's parameter.
#[derive(Debug, Clone, Copy)]
struct Cut {
    t: f64,
    by: EntityId,
    by_t: f64,
}

impl Cut {
    /// The order a closed curve picks its numbering origin by: the
    /// lowest-id cutting entity, and along that entity, its own first
    /// crossing. Both halves are properties of *which* curves meet, never of
    /// where they meet, which is what makes the piece index survive a
    /// dimension edit (module docs).
    fn origin_key(&self) -> (EntityId, f64) {
        (self.by, self.by_t)
    }

    fn origin_cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (a, b) = (self.origin_key(), other.origin_key());
        a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1))
    }
}

/// One piece of a split entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Piece {
    pub(crate) entity: EntityId,
    /// Position along the entity from its own start, `None` when the entity
    /// was not split.
    pub(crate) index: Option<u32>,
    /// The piece's own geometry, trimmed out of the entity's.
    pub(crate) curve: Curve,
    /// The entity that cuts this piece's start and end, `None` where the
    /// piece ends at the entity's own end. Where several cut at one point it
    /// is the lowest-id of them (`merge_coincident`).
    pub(crate) cut_by: [Option<EntityId>; 2],
}

/// One piece as the corpus reads it: where it starts, where it ends, and a
/// point strictly inside it, which tells the two pieces of a cut circle
/// apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityPiece {
    pub index: Option<u32>,
    pub start: [f64; 2],
    pub mid: [f64; 2],
    pub end: [f64; 2],
}

/// The pieces one entity is cut into, in order along it. Empty when the
/// entity is construction geometry, is not a line, circle or arc, or is not
/// in the sketch.
pub fn entity_pieces(sketch: &Sketch, entity: EntityId) -> Vec<EntityPiece> {
    split_all(sketch)
        .into_iter()
        .filter(|piece| piece.entity == entity)
        .map(|piece| EntityPiece {
            index: piece.index,
            start: piece.curve.point_at(0.0),
            mid: piece.curve.point_at(0.5),
            end: piece.curve.point_at(1.0),
        })
        .collect()
}

/// Every non-construction curve in the sketch, cut at every crossing.
pub(crate) fn split_all(sketch: &Sketch) -> Vec<Piece> {
    let curves: Vec<(EntityId, Curve)> = sketch
        .profile_entities()
        .filter_map(|(id, _)| Curve::from_entity(sketch, *id).map(|curve| (*id, curve)))
        .collect();

    let mut cuts: Vec<Vec<Cut>> = vec![Vec::new(); curves.len()];
    for i in 0..curves.len() {
        for j in (i + 1)..curves.len() {
            for hit in intersect(&curves[i].1, &curves[j].1) {
                let (ti, tj) = (curves[i].1.param_at(hit), curves[j].1.param_at(hit));
                cuts[i].push(Cut {
                    t: ti,
                    by: curves[j].0,
                    by_t: tj,
                });
                cuts[j].push(Cut {
                    t: tj,
                    by: curves[i].0,
                    by_t: ti,
                });
            }
        }
    }

    curves
        .into_iter()
        .zip(cuts)
        .flat_map(|((id, curve), cuts)| pieces_of(id, curve, cuts))
        .collect()
}

/// One curve and the parameters it was cut at, as pieces.
fn pieces_of(entity: EntityId, curve: Curve, cuts: Vec<Cut>) -> Vec<Piece> {
    let bounds = piece_bounds(&curve, cuts);
    let single = bounds.len() == 1;
    bounds
        .into_iter()
        .enumerate()
        .map(|(i, ((t0, by0), (t1, by1)))| Piece {
            entity,
            index: (!single).then_some(i as u32),
            curve: curve.sub_curve(t0, t1),
            cut_by: [by0, by1],
        })
        .collect()
}

/// One end of a piece's span: its parameter, and the entity cutting there
/// (`None` at an open curve's own end).
type Bound = (f64, Option<EntityId>);

/// The parameter span of each piece, in order. Always at least one span: an
/// uncut curve is one piece of itself, `0 → 1`.
fn piece_bounds(curve: &Curve, mut cuts: Vec<Cut>) -> Vec<(Bound, Bound)> {
    let tol = curve.param_tolerance();
    if !curve.is_closed() {
        // A cut on the curve's own end bounds nothing; so does one off the
        // end, which is where a hit with the *underlying* line or circle
        // that misses this curve's extent lands.
        cuts.retain(|c| c.t > tol && c.t < 1.0 - tol);
    }
    cuts.sort_by(|a, b| a.t.total_cmp(&b.t));
    let mut cuts = merge_coincident(cuts, tol);
    if cuts.is_empty() {
        return vec![((0.0, None), (1.0, None))];
    }
    if !curve.is_closed() {
        let ends: Vec<Bound> = std::iter::once((0.0, None))
            .chain(cuts.iter().map(|c| (c.t, Some(c.by))))
            .chain(std::iter::once((1.0, None)))
            .collect();
        return ends
            .iter()
            .copied()
            .zip(ends.iter().copied().skip(1))
            .collect();
    }
    // A closed curve is cut into the spans between consecutive cuts, the last
    // of them wrapping past the parameter origin back to the first. Two cuts
    // either side of that origin are one cut, not two.
    if cuts.len() > 1 && cuts[0].t + 1.0 - cuts[cuts.len() - 1].t <= tol {
        let last = cuts.pop().expect("len > 1");
        if last.origin_cmp(&cuts[0]).is_lt() {
            cuts[0] = Cut {
                t: cuts[0].t,
                ..last
            };
        }
    }
    let n = cuts.len();
    let spans: Vec<(Bound, Bound)> = (0..n)
        .map(|i| {
            let next = &cuts[(i + 1) % n];
            let t1 = if i + 1 == n { next.t + 1.0 } else { next.t };
            ((cuts[i].t, Some(cuts[i].by)), (t1, Some(next.by)))
        })
        .collect();
    // Which span is piece 0: the one leaving the numbering origin. Rotating
    // reorders the pieces without moving any of them — each span is already
    // a well-formed `t0 → t1` of its own.
    let origin = (0..n)
        .min_by(|&a, &b| cuts[a].origin_cmp(&cuts[b]))
        .expect("a cut closed curve has at least one span");
    let mut spans = spans;
    spans.rotate_left(origin);
    spans
}

/// Cuts at one point are one cut. The survivor keeps the group's parameter
/// and its **smallest** [`Cut::origin_key`], so a vertex where three curves
/// meet picks its origin the same way a plain crossing does, whichever order
/// the pairs were intersected in.
fn merge_coincident(cuts: Vec<Cut>, tol: f64) -> Vec<Cut> {
    let mut merged: Vec<Cut> = Vec::with_capacity(cuts.len());
    for cut in cuts {
        match merged.last_mut() {
            Some(prev) if (cut.t - prev.t).abs() <= tol => {
                if cut.origin_cmp(prev).is_lt() {
                    *prev = Cut { t: prev.t, ..cut };
                }
            }
            _ => merged.push(cut),
        }
    }
    merged
}
