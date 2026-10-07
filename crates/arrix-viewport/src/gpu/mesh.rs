//! A body's [`RenderMesh`] in GPU buffers: what [`crate::Scene`] holds per
//! body on the real renderer.

use arrix_core::{RenderMesh, TopoKind};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt as _;

use super::highlight::HighlightRanges;
use crate::pick::entity_bits;
use crate::scene::{BodyKey, BodyPayload};

/// A corner of a face's triangle: where it is and the face's normal there.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl Vertex {
    const ATTRS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

impl Vertex {
    const POSITION: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    /// The same buffer read for its positions alone: the pick pass's.
    pub fn position_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            attributes: &Self::POSITION,
            ..Self::layout()
        }
    }
}

/// An end of an edge's line segment.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
}

impl LineVertex {
    const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

/// A vertex's pick bits, a second vertex buffer beside the positions
/// (`crate::pick`).
pub fn id_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![1 => Uint32];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<u32>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}

/// A body's buffers: the faces' triangles, indexed, the edges as a line
/// list and the vertices as points, each beside the pick bits of its
/// entity.
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    /// Per vertex of `vertex_buffer`: its face's pick bits.
    pub face_ids: wgpu::Buffer,
    pub edge_buffer: wgpu::Buffer,
    pub edge_vertex_count: u32,
    /// Per vertex of `edge_buffer`: its edge's pick bits.
    pub edge_ids: wgpu::Buffer,
    /// The body's vertices, a point each, and their pick bits.
    pub point_buffer: wgpu::Buffer,
    pub point_ids: wgpu::Buffer,
    pub point_count: u32,
    /// Every face's boundary as a line list, a range of it a face's
    /// outline (`HighlightRanges::outlines`).
    pub outline_buffer: wgpu::Buffer,
    /// Where each face, edge and outline is in the buffers above.
    pub ranges: HighlightRanges,
}

impl BodyPayload for GpuMesh {
    type Context = wgpu::Device;

    fn upload(device: &wgpu::Device, _: &BodyKey, mesh: &RenderMesh) -> Self {
        let vertices: Vec<Vertex> = mesh
            .positions
            .iter()
            .zip(&mesh.normals)
            .map(|(&position, &normal)| Vertex { position, normal })
            .collect();
        let indices: Vec<u32> = mesh.triangles.iter().flatten().copied().collect();
        let edges = edge_lines(mesh);
        let ids = PickIds::of(mesh);
        let ranges = HighlightRanges::of(mesh);
        let outline: Vec<LineVertex> = ranges
            .outline_lines
            .iter()
            .map(|&position| LineVertex { position })
            .collect();
        let points: Vec<LineVertex> = mesh
            .vertices
            .iter()
            .map(|v| LineVertex {
                position: v.position,
            })
            .collect();
        let buffer = |label: &str, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: non_empty(contents),
                usage,
            })
        };
        GpuMesh {
            vertex_buffer: buffer(
                "arrix-viewport vertices",
                bytemuck::cast_slice(&vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            index_buffer: buffer(
                "arrix-viewport indices",
                bytemuck::cast_slice(&indices),
                wgpu::BufferUsages::INDEX,
            ),
            index_count: indices.len() as u32,
            face_ids: buffer(
                "arrix-viewport face ids",
                bytemuck::cast_slice(&ids.faces),
                wgpu::BufferUsages::VERTEX,
            ),
            edge_buffer: buffer(
                "arrix-viewport edges",
                bytemuck::cast_slice(&edges),
                wgpu::BufferUsages::VERTEX,
            ),
            edge_vertex_count: edges.len() as u32,
            edge_ids: buffer(
                "arrix-viewport edge ids",
                bytemuck::cast_slice(&ids.edges),
                wgpu::BufferUsages::VERTEX,
            ),
            point_buffer: buffer(
                "arrix-viewport points",
                bytemuck::cast_slice(&points),
                wgpu::BufferUsages::VERTEX,
            ),
            point_ids: buffer(
                "arrix-viewport point ids",
                bytemuck::cast_slice(&ids.points),
                wgpu::BufferUsages::VERTEX,
            ),
            point_count: points.len() as u32,
            outline_buffer: buffer(
                "arrix-viewport outlines",
                bytemuck::cast_slice(&outline),
                wgpu::BufferUsages::VERTEX,
            ),
            ranges,
        }
    }
}

/// The pick bits of every vertex the pick pass draws, parallel to the
/// buffers they sit beside.
#[derive(Debug, PartialEq)]
pub struct PickIds {
    /// Per position: a vertex belongs to one face (`RenderMesh`), so
    /// tagging each with its face's bits is exact.
    pub faces: Vec<u32>,
    /// Per line-list vertex of [`edge_lines`].
    pub edges: Vec<u32>,
    /// Per vertex of the body.
    pub points: Vec<u32>,
}

impl PickIds {
    pub fn of(mesh: &RenderMesh) -> Self {
        let mut faces = vec![0; mesh.positions.len()];
        for (index, face) in mesh.faces.iter().enumerate() {
            let bits = entity_bits(TopoKind::Face, index);
            for triangle in mesh.triangles.get(face.range()).unwrap_or_default() {
                for &corner in triangle {
                    if let Some(id) = faces.get_mut(corner as usize) {
                        *id = bits;
                    }
                }
            }
        }
        let mut edges = Vec::new();
        for (index, edge) in mesh.edges.iter().enumerate() {
            let segments = mesh
                .edge_points
                .get(edge.range())
                .map_or(0, |p| p.len().saturating_sub(1));
            let bits = entity_bits(TopoKind::Edge, index);
            edges.extend(std::iter::repeat_n(bits, 2 * segments));
        }
        let points = (0..mesh.vertices.len())
            .map(|index| entity_bits(TopoKind::Vertex, index))
            .collect();
        PickIds {
            faces,
            edges,
            points,
        }
    }
}

/// Each edge's polyline as a line list: a segment per consecutive pair of
/// its points, so edges never join one another's ends.
pub fn edge_lines(mesh: &RenderMesh) -> Vec<LineVertex> {
    let mut lines = Vec::new();
    for edge in &mesh.edges {
        let Some(points) = mesh.edge_points.get(edge.range()) else {
            continue;
        };
        for pair in points.windows(2) {
            lines.push(LineVertex { position: pair[0] });
            lines.push(LineVertex { position: pair[1] });
        }
    }
    lines
}

/// wgpu refuses a zero-sized buffer; a body with no edges (a sphere) still
/// gets one, four bytes that nothing draws.
fn non_empty(bytes: &[u8]) -> &[u8] {
    if bytes.is_empty() { &[0; 4] } else { bytes }
}

#[cfg(test)]
mod tests {
    use arrix_core::{DVec3, FeatureId, Id, MeshPoint, MeshRange, NameRoot, PersistentName};

    use super::*;

    fn name(kind: TopoKind, index: u32) -> PersistentName {
        PersistentName {
            kind,
            root: NameRoot::Frozen {
                feature: FeatureId(Id(1)),
                index,
            },
            chain: vec![],
        }
    }

    fn edge(start: u32, end: u32) -> MeshRange {
        MeshRange {
            name: name(TopoKind::Edge, start),
            start,
            end,
        }
    }

    #[test]
    fn each_polyline_becomes_its_own_segments() {
        let mesh = RenderMesh {
            origin: DVec3::ZERO,
            chord: 1e-3,
            positions: vec![],
            normals: vec![],
            triangles: vec![],
            faces: vec![],
            // A three-point arc, then a straight edge.
            edge_points: vec![
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [5.0; 3],
                [6.0; 3],
            ],
            edges: vec![edge(0, 3), edge(3, 5)],
            vertices: vec![],
        };
        let ends: Vec<[f32; 3]> = edge_lines(&mesh).iter().map(|v| v.position).collect();
        assert_eq!(
            ends,
            vec![
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [5.0; 3],
                [6.0; 3],
            ],
            "no segment joins the arc's end to the line's start"
        );
    }

    #[test]
    fn every_vertex_carries_its_entitys_pick_bits() {
        let face = |start, end| MeshRange {
            name: name(TopoKind::Face, start),
            start,
            end,
        };
        // Two faces of one triangle each, then a face past the triangles.
        let mesh = RenderMesh {
            origin: DVec3::ZERO,
            chord: 1e-3,
            positions: vec![[0.0; 3]; 6],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
            faces: vec![face(0, 1), face(1, 2), face(2, 9)],
            edge_points: vec![[0.0; 3], [1.0; 3], [2.0; 3], [5.0; 3], [6.0; 3]],
            edges: vec![edge(0, 3), edge(3, 5)],
            vertices: vec![MeshPoint {
                name: name(TopoKind::Vertex, 0),
                position: [1.0; 3],
            }],
        };
        let ids = PickIds::of(&mesh);
        let f = |i| entity_bits(TopoKind::Face, i);
        assert_eq!(ids.faces, [f(0), f(0), f(0), f(1), f(1), f(1)]);
        let e = |i| entity_bits(TopoKind::Edge, i);
        assert_eq!(ids.edges, [e(0), e(0), e(0), e(0), e(1), e(1)]);
        assert_eq!(ids.edges.len(), edge_lines(&mesh).len());
        assert_eq!(ids.points, [entity_bits(TopoKind::Vertex, 0)]);
    }
}
