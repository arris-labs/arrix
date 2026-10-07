//! The render pipelines: one builder, each pipeline a few lines of what
//! differs (docs/UI-RENDERING.md §The viewport).

use egui_wgpu::wgpu;

use super::mesh::{LineVertex, Vertex};

/// The depth format of the offscreen target.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// What sets one pipeline apart from the others.
struct Spec<'a> {
    label: &'a str,
    shader: &'a str,
    layouts: &'a [&'a wgpu::BindGroupLayout],
    vertex: Option<wgpu::VertexBufferLayout<'static>>,
    topology: wgpu::PrimitiveTopology,
    /// `None` draws without a depth attachment (the blit, in egui's pass).
    depth: Option<(wgpu::CompareFunction, bool)>,
}

fn build(device: &wgpu::Device, format: wgpu::TextureFormat, spec: Spec) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(spec.label),
        source: wgpu::ShaderSource::Wgsl(spec.shader.into()),
    });
    let layouts: Vec<_> = spec.layouts.iter().copied().map(Some).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(spec.label),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let buffers = [spec.vertex];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(spec.label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: if buffers[0].is_some() { &buffers } else { &[] },
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(format.into())],
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
        Pipelines {
            // A full-screen triangle behind everything.
            background: build(
                device,
                format,
                Spec {
                    label: "arrix-viewport background",
                    shader: include_str!("shaders/background.wgsl"),
                    layouts: &[camera],
                    vertex: None,
                    topology: triangles,
                    depth: Some((wgpu::CompareFunction::Always, false)),
                },
            ),
            faces: build(
                device,
                format,
                Spec {
                    label: "arrix-viewport faces",
                    shader: include_str!("shaders/scene.wgsl"),
                    layouts: &[camera, model],
                    vertex: Some(Vertex::layout()),
                    topology: triangles,
                    depth: Some((wgpu::CompareFunction::Less, true)),
                },
            ),
            // Tested against the faces, writing nothing: an edge never hides
            // another edge.
            edges: build(
                device,
                format,
                Spec {
                    label: "arrix-viewport edges",
                    shader: include_str!("shaders/edges.wgsl"),
                    layouts: &[camera, model],
                    vertex: Some(LineVertex::layout()),
                    topology: wgpu::PrimitiveTopology::LineList,
                    depth: Some((wgpu::CompareFunction::LessEqual, false)),
                },
            ),
            blit: build(
                device,
                format,
                Spec {
                    label: "arrix-viewport blit",
                    shader: include_str!("shaders/blit.wgsl"),
                    layouts: &[blit],
                    vertex: None,
                    topology: triangles,
                    depth: None,
                },
            ),
        }
    }
}
