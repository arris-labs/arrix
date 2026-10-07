//! A body's [`RenderMesh`] in GPU buffers: what [`crate::Scene`] holds per
//! body on the real renderer.

use arrix_core::RenderMesh;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt as _;

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

/// One body's buffers: the faces' triangles, indexed, and the edges as a
/// line list.
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub edge_buffer: wgpu::Buffer,
    pub edge_vertex_count: u32,
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
            edge_buffer: buffer(
                "arrix-viewport edges",
                bytemuck::cast_slice(&edges),
                wgpu::BufferUsages::VERTEX,
            ),
            edge_vertex_count: edges.len() as u32,
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
    use arrix_core::{DVec3, FeatureId, Id, MeshRange, NameRoot, PersistentName, TopoKind};

    use super::*;

    fn edge(start: u32, end: u32) -> MeshRange {
        MeshRange {
            name: PersistentName {
                kind: TopoKind::Edge,
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
}
