//! The paint callback: `prepare` draws the scene into the offscreen target,
//! `paint` copies it into egui's pass at the viewport's rect.

use egui_wgpu::wgpu;

use super::CameraUniforms;

/// What one body's draw binds.
pub struct BodyDraw {
    pub model_offset: u32,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub edge_buffer: wgpu::Buffer,
    pub edge_vertex_count: u32,
}

/// One frame of the viewport, as handles: wgpu's are reference-counted, so
/// cloning them into the callback copies no GPU memory.
pub struct ViewportCallback {
    pub camera: CameraUniforms,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub pipelines: super::pipelines::Pipelines,
    /// Every visible body, in scene order.
    pub bodies: Vec<BodyDraw>,
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
    /// This frame's model matrices, packed at the uniform stride.
    pub model_data: Vec<u8>,
    pub color_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    pub blit_bind_group: wgpu::BindGroup,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        _resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&self.camera));
        if !self.model_data.is_empty() {
            queue.write_buffer(&self.model_buffer, 0, &self.model_data);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("arrix-viewport scene pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // The background covers every pixel.
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_pipeline(&self.pipelines.background);
        pass.draw(0..3, 0..1);

        // A draw per body at its own model offset: N bodies cost N draws,
        // never a merge.
        pass.set_pipeline(&self.pipelines.faces);
        for body in &self.bodies {
            pass.set_bind_group(1, &self.model_bind_group, &[body.model_offset]);
            pass.set_vertex_buffer(0, body.vertex_buffer.slice(..));
            pass.set_index_buffer(body.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..body.index_count, 0, 0..1);
        }
        // Every face is in the depth buffer before the first edge tests
        // against it.
        pass.set_pipeline(&self.pipelines.edges);
        for body in &self.bodies {
            pass.set_bind_group(1, &self.model_bind_group, &[body.model_offset]);
            pass.set_vertex_buffer(0, body.edge_buffer.slice(..));
            pass.draw(0..body.edge_vertex_count, 0..1);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        _resources: &egui_wgpu::CallbackResources,
    ) {
        pass.set_pipeline(&self.pipelines.blit);
        pass.set_bind_group(0, &self.blit_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
