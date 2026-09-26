//! The half-edge walk over the arrangement's pieces.
//!
//! Every piece is traversed twice, once each way, and the two traversals are
//! twins. Around a vertex the outgoing half-edges are sorted CCW by their
//! **tangent** — the direction the piece actually leaves in, not the chord to
//! its far end — with curvature breaking the tie between two that leave in
//! the same direction. Arriving along a half-edge, the walk takes the
//! outgoing one just clockwise of its twin: the sharpest left turn, which
//! keeps the face on the left, so a bounded face closes CCW and a connected
//! component's outside closes CW.
//!
//! Tangent order is what the chord order of the coincidence walk could not
//! do. Two pieces of a cut circle share both endpoints, so their chords are
//! the same direction and the walk had no way to tell the cap from the D; their
//! tangents are mirror images.
//!
//! `next` is a permutation of the half-edges by construction — twinning is a
//! bijection and so is "the predecessor at this vertex" — so its cycles
//! partition the half-edges into faces with nothing left over. Which of them
//! bound material is [`super::faces`]' question, not this module's.

use arrix_core::LENGTH_TOLERANCE;

use super::curve::{Curve, dist, wrap_tau};
use super::split::{Piece, split_all};
use crate::ids::EntityId;
use crate::sketch::Sketch;

/// Two departures closer than this in angle are the same direction, and
/// curvature decides between them. A microradian over a 20 mm feature is
/// 20 pm — three decades under `LENGTH_TOLERANCE`, so nothing this coarse
/// separates two directions the sketch means to be different, and nothing
/// finer survives a tangency the solver satisfies to its residual tolerance
/// rather than exactly.
const ANGLE_TOLERANCE: f64 = 1.0e-6;

/// One directed traversal of one piece. `piece` indexes
/// [`Arrangement::pieces`]; the twin of half-edge `h` is `h ^ 1`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HalfEdge {
    pub(crate) piece: usize,
    /// `true` = along the piece's own parameter, `0 → 1`.
    pub(crate) forward: bool,
    /// The half-edge continuing the face on this one's left.
    pub(crate) next: usize,
}

/// The sketch as a planar arrangement: the pieces every curve was cut into,
/// and the half-edge permutation whose cycles are its faces.
pub(crate) struct Arrangement {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) edges: Vec<HalfEdge>,
}

impl Arrangement {
    pub(crate) fn build(sketch: &Sketch) -> Arrangement {
        let pieces = split_all(sketch);

        // Vertices are geometric: a piece's ends, merged at `LENGTH_TOLERANCE`
        // with every other end that lands there. The coincidence walk took
        // them from `Coincident` constraints instead, which agrees on a
        // solved sketch and claims a vertex where an unsolved one still has a
        // gap — and a gap is what the user sees, so the geometry wins.
        let mut vertices: Vec<[f64; 2]> = Vec::new();
        let mut ends: Vec<(usize, usize)> = Vec::with_capacity(pieces.len());
        for piece in &pieces {
            let from = vertex_at(&mut vertices, piece.curve.point_at(0.0));
            let to = vertex_at(&mut vertices, piece.curve.point_at(1.0));
            ends.push((from, to));
        }

        let mut edges: Vec<HalfEdge> = Vec::with_capacity(pieces.len() * 2);
        let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); vertices.len()];
        for (i, &(from, to)) in ends.iter().enumerate() {
            outgoing[from].push(edges.len());
            edges.push(HalfEdge {
                piece: i,
                forward: true,
                next: usize::MAX,
            });
            outgoing[to].push(edges.len());
            edges.push(HalfEdge {
                piece: i,
                forward: false,
                next: usize::MAX,
            });
        }

        // CCW around the vertex by departure, so "the next one clockwise" is
        // well defined.
        for list in &mut outgoing {
            list.sort_by(|&a, &b| {
                let (ka, kb) = (departure(&pieces, &edges, a), departure(&pieces, &edges, b));
                ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
            });
        }
        let mut rank = vec![0usize; edges.len()];
        for (v, list) in outgoing.iter().enumerate() {
            for (i, &h) in list.iter().enumerate() {
                rank[h] = i;
                debug_assert_eq!(vertex_of(&ends, h), v);
            }
        }

        for (h, edge) in edges.iter_mut().enumerate() {
            // The twin leaves the vertex this half-edge arrives at.
            let twin = h ^ 1;
            let list = &outgoing[vertex_of(&ends, twin)];
            edge.next = list[(rank[twin] + list.len() - 1) % list.len()];
        }

        Arrangement { pieces, edges }
    }

    /// Every face boundary, as the half-edge indices it traverses in order.
    /// Each half-edge belongs to exactly one.
    pub(crate) fn faces(&self) -> Vec<Vec<usize>> {
        let mut seen = vec![false; self.edges.len()];
        let mut faces = Vec::new();
        for start in 0..self.edges.len() {
            if seen[start] {
                continue;
            }
            let mut face = Vec::new();
            let mut h = start;
            while !seen[h] {
                seen[h] = true;
                face.push(h);
                h = self.edges[h].next;
            }
            faces.push(face);
        }
        faces
    }

    /// The piece a half-edge traverses, and which way.
    pub(crate) fn curve_of(&self, h: usize) -> (&Curve, bool) {
        let edge = &self.edges[h];
        (&self.pieces[edge.piece].curve, edge.forward)
    }

    /// Which piece of which entity a half-edge traverses.
    pub(crate) fn piece_of(&self, h: usize) -> &Piece {
        &self.pieces[self.edges[h].piece]
    }

    /// The exact signed area a face's boundary encloses: positive CCW for a
    /// bounded face, negative CW for a component's outside.
    pub(crate) fn signed_area(&self, face: &[usize]) -> f64 {
        face.iter()
            .map(|&h| {
                let (curve, forward) = self.curve_of(h);
                curve.signed_area_term(forward)
            })
            .sum()
    }
}

/// Which vertex half-edge `h` leaves from.
fn vertex_of(ends: &[(usize, usize)], h: usize) -> usize {
    let (from, to) = ends[h / 2];
    if h.is_multiple_of(2) { from } else { to }
}

/// How half-edge `h` leaves its vertex: the CCW angle of its outgoing
/// tangent, bucketed at [`ANGLE_TOLERANCE`], then its signed curvature. A
/// piece that bends left out of the vertex occupies directions just above
/// the shared tangent and one that bends right just below, which is exactly
/// ascending curvature.
fn departure(pieces: &[Piece], edges: &[HalfEdge], h: usize) -> (i64, f64) {
    let edge = &edges[h];
    let curve = &pieces[edge.piece].curve;
    let tangent = if edge.forward {
        curve.tangent_at(0.0)
    } else {
        let t = curve.tangent_at(1.0);
        [-t[0], -t[1]]
    };
    let angle = wrap_tau(tangent[1].atan2(tangent[0]));
    (
        (angle / ANGLE_TOLERANCE).round() as i64,
        curve.curvature(edge.forward),
    )
}

/// The index of the vertex at `p`, appending it when nothing is there yet.
fn vertex_at(vertices: &mut Vec<[f64; 2]>, p: [f64; 2]) -> usize {
    match vertices
        .iter()
        .position(|&q| dist(q, p) <= LENGTH_TOLERANCE)
    {
        Some(i) => i,
        None => {
            vertices.push(p);
            vertices.len() - 1
        }
    }
}

/// One piece as a face boundary traverses it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrangementEdge {
    pub entity: EntityId,
    /// Which piece of the entity, `None` when it was not split — the same
    /// index [`super::entity_pieces`] reports.
    pub piece: Option<u32>,
    /// `true` = along the entity's own direction.
    pub forward: bool,
    /// A point halfway along the piece, which tells two pieces of one entity
    /// apart however the loop reached them.
    pub mid: [f64; 2],
}

/// One face boundary of the arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementLoop {
    /// The pieces it traverses, in order.
    pub edges: Vec<ArrangementEdge>,
    /// Exact signed area in m²: positive for a bounded face, negative for a
    /// connected component's outside.
    pub signed_area: f64,
}

/// Every face boundary the arrangement walks, largest signed area first.
///
/// This is the walk's own surface, tested directly; which of these faces is
/// a profile and which is a hole is [`crate::region`]'s question.
pub fn arrangement_loops(sketch: &Sketch) -> Vec<ArrangementLoop> {
    let arrangement = Arrangement::build(sketch);
    let mut loops: Vec<ArrangementLoop> = arrangement
        .faces()
        .into_iter()
        .map(|face| ArrangementLoop {
            signed_area: arrangement.signed_area(&face),
            edges: face
                .into_iter()
                .map(|h| {
                    let piece = &arrangement.pieces[arrangement.edges[h].piece];
                    ArrangementEdge {
                        entity: piece.entity,
                        piece: piece.index,
                        forward: arrangement.edges[h].forward,
                        mid: piece.curve.point_at(0.5),
                    }
                })
                .collect(),
        })
        .collect();
    loops.sort_by(|a, b| b.signed_area.total_cmp(&a.signed_area));
    loops
}
