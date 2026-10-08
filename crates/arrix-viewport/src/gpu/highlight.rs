//! Hover and selection as restyled index ranges: the ranges of a body's
//! buffers that one face, edge or vertex occupies, and the draws that
//! restyle them over the scene (docs/UI-RENDERING.md §The viewport).
//!
//! Nothing is re-uploaded to highlight: a face is a range of the index
//! buffer, drawn again in a tint; an edge is a range of the line list; a
//! face's outline is a range of one extra line list, made once per mesh
//! from the boundary of the face's triangles.

use std::collections::BTreeMap;
use std::ops::Range;

use arrix_core::{PersistentName, RenderMesh, TopoKind};
use egui_wgpu::wgpu;

use super::mesh::GpuMesh;
use crate::scene::Located;

/// Which style a highlight is drawn in; the shader's `style_color` indexes
/// by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Hover = 0,
    Selection = 1,
    /// A preview's body, tinted whole (ADR-0019).
    Preview = 2,
}

/// The names the viewport highlights this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Highlights<'a> {
    pub hover: Option<&'a PersistentName>,
    pub selection: Option<&'a PersistentName>,
    /// More entities drawn as the selection is: what a form's reference
    /// fields hold.
    pub marked: &'a [PersistentName],
}

/// Where each entity of a mesh sits in its body's buffers.
#[derive(Debug, Default, PartialEq)]
pub struct HighlightRanges {
    /// Per face: its range of the index buffer.
    pub faces: Vec<Range<u32>>,
    /// Per edge: its range of the edge line list's vertices.
    pub edges: Vec<Range<u32>>,
    /// Per face: its range of the outline list's vertices.
    pub outlines: Vec<Range<u32>>,
    /// Every face's boundary, a segment per boundary side of a triangle,
    /// face after face.
    pub outline_lines: Vec<[f32; 3]>,
}

impl HighlightRanges {
    pub fn of(mesh: &RenderMesh) -> Self {
        let limit = (mesh.triangles.len() * 3) as u32;
        let faces = mesh
            .faces
            .iter()
            .map(|f| (f.start * 3).min(limit)..(f.end * 3).min(limit))
            .collect();
        // As `mesh::edge_lines` lays them out: two vertices a segment.
        let mut at = 0;
        let edges = mesh
            .edges
            .iter()
            .map(|edge| {
                let segments = mesh
                    .edge_points
                    .get(edge.range())
                    .map_or(0, |p| p.len().saturating_sub(1)) as u32;
                let range = at..at + 2 * segments;
                at = range.end;
                range
            })
            .collect();
        let mut outline_lines = Vec::new();
        let outlines = mesh
            .faces
            .iter()
            .map(|face| {
                let start = outline_lines.len() as u32;
                let triangles = mesh.triangles.get(face.range()).unwrap_or_default();
                for [a, b] in boundary(triangles) {
                    for corner in [a, b] {
                        let p = mesh.positions.get(corner as usize).copied();
                        outline_lines.push(p.unwrap_or_default());
                    }
                }
                start..outline_lines.len() as u32
            })
            .collect();
        HighlightRanges {
            faces,
            edges,
            outlines,
            outline_lines,
        }
    }
}

/// The sides that belong to one triangle of `triangles` and no other: the
/// boundary of the patch they make, in the order they are met.
fn boundary(triangles: &[[u32; 3]]) -> Vec<[u32; 2]> {
    let sides = |t: &[u32; 3]| [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]];
    let key = |[a, b]: [u32; 2]| (a.min(b), a.max(b));
    let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for side in triangles.iter().flat_map(sides) {
        *uses.entry(key(side)).or_default() += 1;
    }
    triangles
        .iter()
        .flat_map(sides)
        .filter(|&side| uses[&key(side)] == 1)
        .collect()
}

/// One restyled piece of a body.
pub enum Piece {
    /// A face's triangles, tinted over the shaded face.
    Fill {
        vertices: wgpu::Buffer,
        indices: wgpu::Buffer,
        range: Range<u32>,
    },
    /// A range of a line list: an edge, or a face's outline.
    Lines {
        buffer: wgpu::Buffer,
        range: Range<u32>,
    },
    /// A range of a point list: a vertex.
    Points {
        buffer: wgpu::Buffer,
        range: Range<u32>,
    },
}

/// One highlighted entity as the callback draws it.
pub struct HighlightDraw {
    /// Its body's offset into the model uniforms.
    pub model_offset: u32,
    pub style: Style,
    pub pieces: Vec<Piece>,
}

impl HighlightDraw {
    /// Every face of `mesh` tinted, as a preview's body is drawn.
    pub fn body(mesh: &GpuMesh, model_offset: u32, style: Style) -> Self {
        HighlightDraw {
            model_offset,
            style,
            pieces: vec![Piece::Fill {
                vertices: mesh.vertex_buffer.clone(),
                indices: mesh.index_buffer.clone(),
                range: 0..mesh.index_count,
            }],
        }
    }

    /// The draw for the entity `at` locates in `mesh`, whose uniforms sit at
    /// `model_offset`; none for an entity the mesh holds no range for.
    pub fn new(mesh: &GpuMesh, at: Located, model_offset: u32, style: Style) -> Option<Self> {
        let ranges = &mesh.ranges;
        let pieces = match at.kind {
            TopoKind::Face => vec![
                Piece::Fill {
                    vertices: mesh.vertex_buffer.clone(),
                    indices: mesh.index_buffer.clone(),
                    range: ranges.faces.get(at.index)?.clone(),
                },
                Piece::Lines {
                    buffer: mesh.outline_buffer.clone(),
                    range: ranges.outlines.get(at.index)?.clone(),
                },
            ],
            TopoKind::Edge => vec![Piece::Lines {
                buffer: mesh.edge_buffer.clone(),
                range: ranges.edges.get(at.index)?.clone(),
            }],
            TopoKind::Vertex => {
                let index = u32::try_from(at.index).ok()?;
                if index >= mesh.point_count {
                    return None;
                }
                vec![Piece::Points {
                    buffer: mesh.point_buffer.clone(),
                    range: index..index + 1,
                }]
            }
        };
        Some(HighlightDraw {
            model_offset,
            style,
            pieces,
        })
    }
}

#[cfg(test)]
mod tests {
    use arrix_core::{DVec3, FeatureId, Id, MeshRange, NameRoot};

    use super::*;

    fn range(kind: TopoKind, start: u32, end: u32) -> MeshRange {
        MeshRange {
            name: PersistentName {
                kind,
                root: NameRoot::Frozen {
                    feature: FeatureId(Id(1)),
                    index: start,
                },
                chain: vec![],
            },
            start,
            end,
        }
    }

    /// Two squares of two triangles each: positions 0..4 and 4..8.
    fn mesh() -> RenderMesh {
        let square = |z: f32| [[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]];
        RenderMesh {
            origin: DVec3::ZERO,
            chord: 1e-3,
            positions: square(0.0).into_iter().chain(square(1.0)).collect(),
            normals: vec![[0.0, 0.0, 1.0]; 8],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            faces: vec![range(TopoKind::Face, 0, 2), range(TopoKind::Face, 2, 4)],
            // A three-point arc and a straight edge, then one without points.
            edge_points: vec![
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [5.0; 3],
                [6.0; 3],
            ],
            edges: vec![
                range(TopoKind::Edge, 0, 3),
                range(TopoKind::Edge, 3, 5),
                range(TopoKind::Edge, 9, 12),
            ],
            vertices: vec![],
        }
    }

    #[test]
    fn a_face_is_its_triangles_index_range() {
        let ranges = HighlightRanges::of(&mesh());
        assert_eq!(ranges.faces, [0..6, 6..12]);
    }

    #[test]
    fn a_range_past_the_triangles_is_clamped_to_them() {
        let mut m = mesh();
        m.faces.push(range(TopoKind::Face, 3, 99));
        let ranges = HighlightRanges::of(&m);
        assert_eq!(ranges.faces[2], 9..12);
    }

    #[test]
    fn an_edge_is_its_segments_range_of_the_line_list() {
        let ranges = HighlightRanges::of(&mesh());
        // The arc's two segments, the line's one, and none past the points.
        assert_eq!(ranges.edges, [0..4, 4..6, 6..6]);
        assert_eq!(
            ranges.edges.last().map(|r| r.end as usize),
            Some(super::super::mesh::edge_lines(&mesh()).len()),
            "the ranges end where the line list does"
        );
    }

    #[test]
    fn a_faces_outline_is_the_sides_only_one_of_its_triangles_has() {
        let ranges = HighlightRanges::of(&mesh());
        // A square's four sides; the diagonal is shared by both triangles.
        assert_eq!(ranges.outlines, [0..8, 8..16]);
        let first: Vec<[f32; 3]> = ranges.outline_lines[..8].to_vec();
        assert!(
            !first.windows(2).step_by(2).any(|w| {
                let diagonal = [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
                (w[0] == diagonal[0] && w[1] == diagonal[1])
                    || (w[0] == diagonal[1] && w[1] == diagonal[0])
            }),
            "the diagonal is inside the face, not on its boundary: {first:?}"
        );
        assert_eq!(first[0], [0.0, 0.0, 0.0], "met in triangle order");
    }

    #[test]
    fn a_face_without_triangles_has_an_empty_outline() {
        let mut m = mesh();
        m.faces.push(range(TopoKind::Face, 4, 4));
        let ranges = HighlightRanges::of(&m);
        assert_eq!(ranges.outlines[2], 16..16);
    }
}
