//! The wgpu renderer: the scene drawn into an offscreen target, which an
//! `egui_wgpu` paint callback copies into the egui frame at the viewport's
//! rect (docs/UI-RENDERING.md §The viewport). Offscreen, because depth
//! testing needs a depth attachment and egui's pass has none.
//!
//! One [`Renderer`] serves every open document: what differs per document
//! is its [`Scene`] and its camera, which [`Renderer::paint`] takes, and
//! the pick it may ask for, which it hands back.

mod callback;
mod highlight;
mod mesh;
mod pipelines;
mod readback;

use crate::camera::{OrbitCamera, Projection};
use crate::pick::{PickRegion, body_bits, body_pickable};
use crate::scene::Scene;
use callback::{BodyDraw, ViewportCallback};
use egui_wgpu::wgpu;
pub use highlight::Highlights;
use highlight::{HighlightDraw, Style};
pub use mesh::GpuMesh;
use pipelines::{DEPTH_FORMAT, PICK_FORMAT, Pipelines};
pub use readback::PendingPick;
use readback::{PickPass, PickTarget};

/// The camera as every shader sees it (group 0). The background reads the
/// basis and `params` to shade the world direction each pixel looks along.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub up: [f32; 4],
    pub right: [f32; 4],
    pub forward: [f32; 4],
    /// `(is_ortho, aspect, is_dark, pixel)`: `pixel` is a pixel's height in
    /// clip space, which the paint sets from the target (`new` leaves 0).
    pub params: [f32; 4],
}

impl CameraUniforms {
    pub fn new(camera: &OrbitCamera, aspect: f32, dark: bool) -> Self {
        let view_proj = camera.view_proj(aspect);
        let eye = camera.eye();
        let (forward, right, up) = camera.basis();
        let flag = |b: bool| if b { 1.0 } else { 0.0 };
        CameraUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            eye: eye.extend(1.0).to_array(),
            up: up.extend(0.0).to_array(),
            right: right.extend(0.0).to_array(),
            forward: forward.extend(0.0).to_array(),
            params: [
                flag(camera.projection == Projection::Orthographic),
                aspect,
                flag(dark),
                0.0,
            ],
        }
    }
}

/// What each body's draw reads (group 1): its model matrix, its pick
/// bits (`crate::pick`) in `pick[0]`, and its own colour in `tint` (alpha 1)
/// when a material gives it one.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct BodyUniforms {
    model: [[f32; 4]; 4],
    pick: [u32; 4],
    tint: [f32; 4],
}

/// A body's uniforms, before alignment.
const MODEL_SIZE: u64 = std::mem::size_of::<BodyUniforms>() as u64;

/// Every body's uniforms in one buffer at a fixed stride, bound once with a
/// dynamic offset per draw.
struct ModelUniforms {
    layout: wgpu::BindGroupLayout,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// [`MODEL_SIZE`] rounded up to the device's uniform offset alignment.
    stride: u64,
    capacity: usize,
}

impl ModelUniforms {
    fn new(device: &wgpu::Device) -> Self {
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("arrix-viewport model"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // The fragment stage reads the body's own colour.
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(MODEL_SIZE),
                },
                count: None,
            }],
        });
        let stride = MODEL_SIZE.div_ceil(align) * align;
        let capacity = 8;
        let (buffer, bind_group) = Self::allocate(device, &layout, stride, capacity);
        ModelUniforms {
            layout,
            buffer,
            bind_group,
            stride,
            capacity,
        }
    }

    /// Room for `count` bodies, re-allocated only when outgrown.
    fn reserve(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.capacity {
            return;
        }
        self.capacity = count.next_power_of_two();
        (self.buffer, self.bind_group) =
            Self::allocate(device, &self.layout, self.stride, self.capacity);
    }

    fn allocate(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        stride: u64,
        capacity: usize,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("arrix-viewport models"),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("arrix-viewport models"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(MODEL_SIZE),
                }),
            }],
        });
        (buffer, bind_group)
    }
}

/// The offscreen colour and depth the scene is drawn into, and the pick
/// target, sized to the viewport's rect in physical pixels.
struct Offscreen {
    size: (u32, u32),
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    blit_bind_group: wgpu::BindGroup,
    pick: PickTarget,
}

/// The pipelines and buffers the viewport draws with; one per window.
pub struct Renderer {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    pipelines: Pipelines,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    models: ModelUniforms,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    offscreen: Option<Offscreen>,
}

impl Renderer {
    /// The renderer on `device`, drawing for a frame of `format`: the
    /// render state's target format.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("arrix-viewport camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("arrix-viewport camera"),
            size: std::mem::size_of::<CameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("arrix-viewport camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let texture = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("arrix-viewport blit"),
            entries: &[
                texture(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                texture(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let models = ModelUniforms::new(device);
        let pipelines =
            Pipelines::new(device, format, &camera_layout, &models.layout, &blit_layout);
        // Nearest, default: the target is the rect's size in pixels, so the
        // copy is one texel to one pixel and filtering would only blur.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("arrix-viewport blit"),
            ..Default::default()
        });
        Renderer {
            device: device.clone(),
            format,
            pipelines,
            camera_buffer,
            camera_bind_group,
            models,
            blit_layout,
            sampler,
            offscreen: None,
        }
    }

    /// The device a [`Scene<GpuMesh>`] uploads to.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Lets the read-backs that have arrived answer their picks, without
    /// waiting for any: once a frame, before [`PendingPick::poll`].
    pub fn poll(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    /// Draws `scene` from `camera` into `rect` of `painter`'s layer: the
    /// work is done when egui renders the frame, through the callback this
    /// adds. `highlights` are the entities restyled over it, hover under
    /// selection; a name no visible body holds draws nothing. The scene's
    /// preview, if any, is drawn tinted in place of the bodies it replaces,
    /// which are still what a pick lands on. With `pick`, a point in `rect`, the frame also draws the pick
    /// pass and reads back the region about it: the pick returned is
    /// answered frames later ([`Renderer::poll`]). An empty scene picks
    /// nothing and returns none.
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        scene: &Scene<GpuMesh>,
        camera: &OrbitCamera,
        highlights: Highlights<'_>,
        pick: Option<egui::Pos2>,
    ) -> Option<PendingPick> {
        let ppp = painter.ctx().pixels_per_point();
        let size = (
            ((rect.width() * ppp).round() as u32).max(1),
            ((rect.height() * ppp).round() as u32).max(1),
        );
        self.ensure_offscreen(size);
        let offscreen = self.offscreen.as_ref()?;
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        let dark = painter.ctx().global_style().visuals.dark_mode;

        // The document's visible instances, then the preview's bodies: the
        // pick ids and highlight indices count the document's alone.
        let count = scene.visible().count();
        let total = count + scene.preview().count();
        self.models.reserve(&self.device, total);
        let stride = self.models.stride as usize;
        let mut model_data = vec![0u8; stride * total];
        let mut bodies = Vec::with_capacity(total);
        let entries = scene
            .visible()
            .map(|d| (d.entry, d.model, false))
            .chain(scene.preview().map(|e| (e, e.model, true)));
        for (i, (entry, model, preview)) in entries.enumerate() {
            let at = i * stride;
            let uniforms = BodyUniforms {
                model: model.to_cols_array_2d(),
                pick: [if preview { 0 } else { body_bits(i) }, 0, 0, 0],
                tint: entry.colour.map_or([0.0; 4], |[r, g, b]| [r, g, b, 1.0]),
            };
            model_data[at..at + MODEL_SIZE as usize].copy_from_slice(bytemuck::bytes_of(&uniforms));
            let mesh = &entry.mesh;
            bodies.push(BodyDraw {
                model_offset: at as u32,
                vertex_buffer: mesh.vertex_buffer.clone(),
                index_buffer: mesh.index_buffer.clone(),
                index_count: mesh.index_count,
                face_ids: mesh.face_ids.clone(),
                edge_buffer: mesh.edge_buffer.clone(),
                edge_vertex_count: mesh.edge_vertex_count,
                edge_ids: mesh.edge_ids.clone(),
                point_buffer: mesh.point_buffer.clone(),
                point_ids: mesh.point_ids.clone(),
                point_count: mesh.point_count,
                pickable: !preview && body_pickable(i),
                drawn: preview || !scene.replaced(&entry.key),
            });
        }

        let (pick, pending) = match pick.filter(|_| count > 0) {
            Some(at) => {
                let local = (at - rect.min) * ppp;
                let cursor = (local.x.max(0.0) as u32, local.y.max(0.0) as u32);
                let region = PickRegion::around(cursor, size);
                let (pass, pending) =
                    PickPass::new(&self.device, &offscreen.pick, region, scene.pick_bodies());
                (Some(pass), Some(pending))
            }
            None => (None, None),
        };

        let highlights = [
            (highlights.hover, Style::Hover),
            (highlights.selection, Style::Selection),
        ]
        .into_iter()
        .chain(
            highlights
                .marked
                .iter()
                .map(|n| (Some(n), Style::Selection)),
        )
        .filter_map(|(hit, style)| {
            let hit = hit?;
            let at = scene.locate(&hit.path, &hit.name)?;
            let drawn = scene.visible().nth(at.body)?;
            let model_offset = (at.body * stride) as u32;
            HighlightDraw::new(&drawn.entry.mesh, at, model_offset, style)
        })
        .chain(scene.visible().enumerate().filter_map(|(i, d)| {
            let marked = highlights.components.iter().any(|p| d.path.starts_with(p));
            marked
                .then(|| HighlightDraw::body(&d.entry.mesh, (i * stride) as u32, Style::Selection))
        }))
        .chain(scene.preview().enumerate().map(|(i, entry)| {
            HighlightDraw::body(&entry.mesh, ((count + i) * stride) as u32, Style::Preview)
        }))
        .collect();

        let mut camera_uniforms = CameraUniforms::new(camera, aspect, dark);
        camera_uniforms.params[3] = 2.0 / size.1 as f32;
        let callback = ViewportCallback {
            camera: camera_uniforms,
            camera_buffer: self.camera_buffer.clone(),
            camera_bind_group: self.camera_bind_group.clone(),
            pipelines: self.pipelines.clone(),
            bodies,
            model_buffer: self.models.buffer.clone(),
            model_bind_group: self.models.bind_group.clone(),
            model_data,
            color_view: offscreen.color_view.clone(),
            depth_view: offscreen.depth_view.clone(),
            blit_bind_group: offscreen.blit_bind_group.clone(),
            pick,
            highlights,
        };
        painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
        pending
    }

    fn ensure_offscreen(&mut self, size: (u32, u32)) {
        if self.offscreen.as_ref().map(|o| o.size) == Some(size) {
            return;
        }
        let texture = |label, format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let color_view = view(&texture(
            "arrix-viewport colour",
            self.format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        ));
        let depth = || {
            view(&texture(
                "arrix-viewport depth",
                DEPTH_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ))
        };
        let depth_view = depth();
        let pick_texture = texture(
            "arrix-viewport pick",
            PICK_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let pick = PickTarget {
            view: view(&pick_texture),
            texture: pick_texture,
            depth_view: depth(),
        };
        let blit_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("arrix-viewport blit"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.offscreen = Some(Offscreen {
            size,
            color_view,
            depth_view,
            blit_bind_group,
            pick,
        });
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::*;

    #[test]
    fn the_uniforms_carry_the_cameras_matrices_and_flags() {
        let mut camera = OrbitCamera::default();
        let u = CameraUniforms::new(&camera, 2.0, true);
        assert_eq!(u.view_proj, camera.view_proj(2.0).to_cols_array_2d());
        assert_eq!(u.params, [0.0, 2.0, 1.0, 0.0]);
        let round =
            Mat4::from_cols_array_2d(&u.view_proj) * Mat4::from_cols_array_2d(&u.inv_view_proj);
        assert!(round.abs_diff_eq(Mat4::IDENTITY, 1e-3), "{round}");
        assert_eq!(Vec3::from_slice(&u.eye[..3]), camera.eye());

        camera.toggle_projection();
        assert_eq!(
            CameraUniforms::new(&camera, 1.0, false).params[..3],
            [1.0, 1.0, 0.0]
        );
    }

    #[test]
    fn the_uniform_layout_is_what_the_shaders_declare() {
        // Two mat4x4 and five vec4, no padding: WGSL's uniform layout.
        assert_eq!(std::mem::size_of::<CameraUniforms>(), 2 * 64 + 5 * 16);
        // A mat4x4, a vec4<u32> and a vec4<f32>: the shaders' `Body`.
        assert_eq!(MODEL_SIZE, 64 + 16 + 16);
    }
}
