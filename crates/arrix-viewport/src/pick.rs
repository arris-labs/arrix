//! ID-buffer picking, the GPU-free half (docs/UI-RENDERING.md §The viewport):
//! the `u32` each face, edge and vertex draws into the `R32Uint` target,
//! and how a read-back region around the cursor resolves to one entity.
//!
//! An id is the entity's kind in the top bits, the body's index among the
//! scene's visible bodies, and the entity's index in its mesh's range
//! table. The kind and the entity index are in the vertex data, uploaded
//! once per mesh; the body index comes from the body's per-draw uniform, so
//! hiding a body (which renumbers the ones after it) re-uploads nothing.
//! `0`, the target's clear value, is "nothing": every real id has a kind.

use arrix_core::TopoKind;

const INDEX_BITS: u32 = 20;
const BODY_BITS: u32 = 10;
const KIND_SHIFT: u32 = INDEX_BITS + BODY_BITS;
const INDEX_MASK: u32 = (1 << INDEX_BITS) - 1;
const BODY_MASK: u32 = (1 << BODY_BITS) - 1;

/// How many visible bodies can be told apart under the cursor; a body past
/// them draws into the target as nothing.
pub const MAX_PICK_BODIES: usize = BODY_MASK as usize + 1;
/// How many faces (or edges, or vertices) of one body can be told apart;
/// an entity past them draws as nothing.
pub const MAX_PICK_ENTITIES: usize = INDEX_MASK as usize + 1;

/// The edge of the square of pixels read back around the cursor, in
/// physical pixels: odd, so the cursor's pixel is its centre. An edge or a
/// vertex is a pixel thin, and the square is what makes it a target a
/// hand can hit.
pub const PICK_REGION: u32 = 7;

fn kind_bits(kind: TopoKind) -> u32 {
    match kind {
        TopoKind::Face => 1,
        TopoKind::Edge => 2,
        TopoKind::Vertex => 3,
    }
}

/// The part of an id a mesh's vertex data carries: its kind and its index
/// in the mesh. `0` for an entity past [`MAX_PICK_ENTITIES`].
pub fn entity_bits(kind: TopoKind, index: usize) -> u32 {
    if index >= MAX_PICK_ENTITIES {
        return 0;
    }
    (kind_bits(kind) << KIND_SHIFT) | index as u32
}

/// The part of an id a body's uniform carries: its index among the
/// visible bodies. The shader ORs it in only where the vertex data is not
/// `0`, so an unpickable entity stays nothing.
pub fn body_bits(body: usize) -> u32 {
    if body >= MAX_PICK_BODIES {
        return 0;
    }
    (body as u32) << INDEX_BITS
}

/// Whether bodies past [`MAX_PICK_BODIES`] are left out of the pick pass.
pub fn body_pickable(body: usize) -> bool {
    body < MAX_PICK_BODIES
}

/// What one pixel of the target holds: an entity of a body, by indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickHit {
    pub kind: TopoKind,
    /// The body's index among the visible bodies when the pass was drawn.
    pub body: usize,
    /// The entity's index in its mesh's faces, edges or vertices.
    pub index: usize,
}

/// The id `hit` draws as.
pub fn encode(hit: PickHit) -> u32 {
    let entity = entity_bits(hit.kind, hit.index);
    if entity == 0 || !body_pickable(hit.body) {
        return 0;
    }
    entity | body_bits(hit.body)
}

/// The entity an id names; `None` for the clear value.
pub fn decode(id: u32) -> Option<PickHit> {
    let kind = match id >> KIND_SHIFT {
        1 => TopoKind::Face,
        2 => TopoKind::Edge,
        3 => TopoKind::Vertex,
        _ => return None,
    };
    Some(PickHit {
        kind,
        body: ((id >> INDEX_BITS) & BODY_MASK) as usize,
        index: (id & INDEX_MASK) as usize,
    })
}

/// The square read back around the cursor, clamped to the target: it
/// slides inward at an edge rather than shrinking, unless the target is
/// smaller than it. Physical pixels, top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickRegion {
    pub origin: (u32, u32),
    pub width: u32,
    pub height: u32,
    /// The cursor's pixel, inside the region.
    pub cursor: (u32, u32),
}

impl PickRegion {
    /// The region about `cursor` in a target of `size` pixels, the cursor
    /// clamped into it first.
    pub fn around(cursor: (u32, u32), size: (u32, u32)) -> Self {
        let size = (size.0.max(1), size.1.max(1));
        let cursor = (cursor.0.min(size.0 - 1), cursor.1.min(size.1 - 1));
        let width = PICK_REGION.min(size.0);
        let height = PICK_REGION.min(size.1);
        let origin = (
            cursor.0.saturating_sub(PICK_REGION / 2).min(size.0 - width),
            cursor
                .1
                .saturating_sub(PICK_REGION / 2)
                .min(size.1 - height),
        );
        PickRegion {
            origin,
            width,
            height,
            cursor,
        }
    }
}

/// The entity a read-back region resolves to: a vertex over an edge over
/// a face anywhere in the region, then the nearest to the cursor's pixel,
/// then the first in row order. `ids` is the region row by row.
pub fn resolve_region(ids: &[u32], region: &PickRegion) -> Option<PickHit> {
    let priority = |kind| match kind {
        TopoKind::Vertex => 2,
        TopoKind::Edge => 1,
        TopoKind::Face => 0,
    };
    let mut best: Option<(u8, u32, PickHit)> = None;
    for row in 0..region.height {
        for col in 0..region.width {
            let Some(&id) = ids.get((row * region.width + col) as usize) else {
                continue;
            };
            let Some(hit) = decode(id) else {
                continue;
            };
            let dx = (region.origin.0 + col).abs_diff(region.cursor.0);
            let dy = (region.origin.1 + row).abs_diff(region.cursor.1);
            let rank = (priority(hit.kind), dx * dx + dy * dy);
            let better = best.is_none_or(|(p, d, _)| rank.0 > p || (rank.0 == p && rank.1 < d));
            if better {
                best = Some((rank.0, rank.1, hit));
            }
        }
    }
    best.map(|(_, _, hit)| hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(kind: TopoKind, body: usize, index: usize) -> PickHit {
        PickHit { kind, body, index }
    }

    #[test]
    fn every_kind_round_trips_and_never_encodes_to_nothing() {
        for h in [
            hit(TopoKind::Face, 0, 0),
            hit(TopoKind::Face, 3, 41),
            hit(TopoKind::Edge, 0, 7),
            hit(TopoKind::Vertex, MAX_PICK_BODIES - 1, MAX_PICK_ENTITIES - 1),
        ] {
            let id = encode(h);
            assert_ne!(id, 0, "{h:?} must not read as the clear value");
            assert_eq!(decode(id), Some(h));
            assert_eq!(id, entity_bits(h.kind, h.index) | body_bits(h.body));
        }
        assert_eq!(decode(0), None);
    }

    #[test]
    fn what_does_not_fit_draws_as_nothing() {
        assert_eq!(entity_bits(TopoKind::Face, MAX_PICK_ENTITIES), 0);
        assert_eq!(encode(hit(TopoKind::Edge, MAX_PICK_BODIES, 0)), 0);
        assert!(!body_pickable(MAX_PICK_BODIES));
    }

    #[test]
    fn the_region_is_centred_and_slides_inward_at_the_edges() {
        let r = PickRegion::around((50, 40), (100, 80));
        assert_eq!((r.origin, r.width, r.height), ((47, 37), 7, 7));
        let corner = PickRegion::around((0, 79), (100, 80));
        assert_eq!(corner.origin, (0, 73));
        assert_eq!(corner.cursor, (0, 79));
        let outside = PickRegion::around((500, 500), (100, 80));
        assert_eq!(outside.cursor, (99, 79));
        let tiny = PickRegion::around((1, 1), (3, 2));
        assert_eq!((tiny.origin, tiny.width, tiny.height), ((0, 0), 3, 2));
    }

    /// A 3 × 3 region about its centre.
    fn region() -> PickRegion {
        PickRegion {
            origin: (10, 10),
            width: 3,
            height: 3,
            cursor: (11, 11),
        }
    }

    #[test]
    fn a_vertex_beats_an_edge_beats_a_face_anywhere_in_the_region() {
        let face = encode(hit(TopoKind::Face, 0, 4));
        let edge = encode(hit(TopoKind::Edge, 0, 2));
        let vertex = encode(hit(TopoKind::Vertex, 1, 9));
        let mut ids = vec![face; 9];
        assert_eq!(
            resolve_region(&ids, &region()),
            Some(hit(TopoKind::Face, 0, 4))
        );
        ids[0] = edge;
        assert_eq!(
            resolve_region(&ids, &region()).map(|h| h.kind),
            Some(TopoKind::Edge),
            "an edge in the corner beats the face under the cursor"
        );
        ids[8] = vertex;
        assert_eq!(
            resolve_region(&ids, &region()),
            Some(hit(TopoKind::Vertex, 1, 9))
        );
        assert_eq!(resolve_region(&[0; 9], &region()), None);
    }

    #[test]
    fn among_equals_the_nearest_to_the_cursor_wins() {
        let mut ids = vec![0; 9];
        ids[0] = encode(hit(TopoKind::Face, 0, 1)); // a corner
        ids[5] = encode(hit(TopoKind::Face, 0, 2)); // beside the centre
        assert_eq!(resolve_region(&ids, &region()).map(|h| h.index), Some(2));
        ids[4] = encode(hit(TopoKind::Face, 1, 3)); // under the cursor
        assert_eq!(
            resolve_region(&ids, &region()),
            Some(hit(TopoKind::Face, 1, 3))
        );
    }
}
