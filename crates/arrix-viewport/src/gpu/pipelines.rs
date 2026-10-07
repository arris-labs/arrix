//! The render pipelines: one builder, each pipeline a few lines of what
//! differs (docs/UI-RENDERING.md §The viewport).

use egui_wgpu::wgpu;

use super::mesh::{LineVertex, Vertex, id_layout};

/// The depth format of the offscreen target.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The pick target's format: one id per pixel (`crate::pick`).
pub const PICK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

/// What sets one pipeline apart from the others.
struct Spec<'a> {
    label: &'a str,
    shader: &'a str,
    /// The vertex entry point; the fragment's is `fs_main`.
    entry: &'a str,
    /// The target's format.
    format: wgpu::TextureFormat,
    layouts: &'a [&'a wgpu::BindGroupLayout],
    vertex: &'a [wgpu::VertexBufferLayout<'static>],
    topology: wgpu::PrimitiveTopology,
    /// `None` draws without a depth attachment (the blit, in egui's pass).
    depth: Option<(wgpu::CompareFunction, bool)>,
}

fn build(device: &wgpu::Device, spec: Spec) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(spec.label),
        source: wgpu::ShaderSource::Wgsl(spec.shader.into()),
    });
    let layouts: Vec<_> = spec.layouts.iter().copied().map(Some).collect();
    let buffers: Vec<_> = spec.vertex.iter().cloned().map(Some).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(spec.label),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(spec.label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(spec.entry),
            buffers: &buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(spec.format.into())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: spec.topology,
            // The depth test hides surfaces, so winding does not matter.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: spec.depth.map(|(compare, write)| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Every pipeline the viewport draws with.
#[derive(Clone)]
pub struct Pipelines {
    pub background: wgpu::RenderPipeline,
    pub faces: wgpu::RenderPipeline,
    pub edges: wgpu::RenderPipeline,
    pub blit: wgpu::RenderPipeline,
    /// The pick pass's faces, edges and vertices, into [`PICK_FORMAT`].
    pub pick_faces: wgpu::RenderPipeline,
    pub pick_edges: wgpu::RenderPipeline,
    pub pick_points: wgpu::RenderPipeline,
}

impl Pipelines {
    /// `camera` is group 0 of all but the blit; `model` is group 1 of the
    /// per-body ones; `blit` is the blit's only group.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        camera: &wgpu::BindGroupLayout,
        model: &wgpu::BindGroupLayout,
        blit: &wgpu::BindGroupLayout,
    ) -> Self {
        let triangles = wgpu::PrimitiveTopology::TriangleList;
        let pick = include_str!("shaders/pick.wgsl");
        let lines = [LineVertex::layout(), id_layout()];
        Pipelines {
            // A full-screen triangle behind everything.
            background: build(
                device,
                Spec {
                    label: "arrix-viewport background",
                    shader: include_str!("shaders/background.wgsl"),
                    layouts: &[camera],
                    entry: "vs_main",
                    format,
                    vertex: &[],
                    topology: triangles,
                    depth: Some((wgpu::CompareFunction::Always, false)),
                },
            ),
            faces: build(
                device,
                Spec {
                    label: "arrix-viewport faces",
                    shader: include_str!("shaders/scene.wgsl"),
                    layouts: &[camera, model],
                    entry: "vs_main",
                    format,
                    vertex: &[Vertex::layout()],
                    topology: triangles,
                    depth: Some((wgpu::CompareFunction::Less, true)),
                },
            ),
            // Tested against the faces, writing nothing: an edge never hides
            // another edge.
            edges: build(
                device,
                Spec {
                    label: "arrix-viewport edges",
                    shader: include_str!("shaders/edges.wgsl"),
                    layouts: &[camera, model],
                    entry: "vs_main",
                    format,
                    vertex: &[LineVertex::layout()],
                    topology: wgpu::PrimitiveTopology::LineList,
                    depth: Some((wgpu::CompareFunction::LessEqual, false)),
                },
            ),
            blit: build(
                device,
                Spec {
                    label: "arrix-viewport blit",
                    shader: include_str!("shaders/blit.wgsl"),
                    layouts: &[blit],
                    entry: "vs_main",
                    format,
                    vertex: &[],
                    topology: triangles,
                    depth: None,
                },
            ),
            pick_faces: build(
                device,
                Spec {
                    label: "arrix-viewport pick faces",
                    shader: pick,
                    entry: "vs_surface",
                    format: PICK_FORMAT,
                    layouts: &[camera, model],
                    vertex: &[Vertex::position_layout(), id_layout()],
                    topology: triangles,
                    depth: Some((wgpu::CompareFunction::Less, true)),
                },
            ),
            // As the edges are drawn: over the faces, so an edge wins the
            // pixels it shows on.
            pick_edges: build(
                device,
                Spec {
                    label: "arrix-viewport pick edges",
                    shader: pick,
                    entry: "vs_line",
                    format: PICK_FORMAT,
                    layouts: &[camera, model],
                    vertex: &lines,
                    topology: wgpu::PrimitiveTopology::LineList,
                    depth: Some((wgpu::CompareFunction::LessEqual, false)),
                },
            ),
            pick_points: build(
                device,
                Spec {
                    label: "arrix-viewport pick points",
                    shader: pick,
                    entry: "vs_line",
                    format: PICK_FORMAT,
                    layouts: &[camera, model],
                    vertex: &lines,
                    topology: wgpu::PrimitiveTopology::PointList,
                    depth: Some((wgpu::CompareFunction::LessEqual, false)),
                },
            ),
        }
    }
}
