//! The pick pass and its read-back: the ids drawn into the pick target, a
//! small region about the cursor copied out, mapped asynchronously and
//! polled each frame, never waited on (docs/UI-RENDERING.md §The viewport).
//! The same path on native and wasm.

use std::sync::{Arc, Mutex};

use egui_wgpu::wgpu;

use super::callback::BodyDraw;
use super::pipelines::Pipelines;
use crate::pick::{PICK_REGION, PickHit, PickRegion, resolve_region};
use crate::scene::PickBodies;

/// A buffer copy's rows are 256-byte aligned; a region's row is far less.
const ROW_STRIDE: u32 = 256;

/// The ids of the region, row by row, once the map has completed.
type Slot = Arc<Mutex<Option<Vec<u32>>>>;

/// A pick asked for and not yet answered: its region, the bodies its pass
/// drew, and where the mapped ids land.
pub struct PendingPick {
    pub region: PickRegion,
    pub bodies: PickBodies,
    result: Slot,
}

impl PendingPick {
    /// What the region resolved to, once the read-back has arrived:
    /// `Some(None)` is an answer too, that nothing is under the cursor.
    /// `None` while it is still on its way. Resolve the hit against the
    /// scene with [`crate::Scene::resolve`] and [`PendingPick::bodies`].
    pub fn poll(&self) -> Option<Option<PickHit>> {
        let ids = self.result.lock().ok()?.take()?;
        Some(resolve_region(&ids, &self.region))
    }
}

/// The pick target, sized with the offscreen colour.
pub struct PickTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
}

/// One frame's pick pass, carried by the paint callback.
pub struct PickPass {
    pub region: PickRegion,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    pub buffer: wgpu::Buffer,
    result: Slot,
}

impl PickPass {
    /// A pass reading `region` of `target` back, and the pick that waits
    /// on it.
    pub fn new(
        device: &wgpu::Device,
        target: &PickTarget,
        region: PickRegion,
        bodies: PickBodies,
    ) -> (PickPass, PendingPick) {
        let result: Slot = Arc::default();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("arrix-viewport pick read-back"),
            size: u64::from(ROW_STRIDE * PICK_REGION),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let pass = PickPass {
            region,
            texture: target.texture.clone(),
            view: target.view.clone(),
            depth_view: target.depth_view.clone(),
            buffer,
            result: result.clone(),
        };
        let pending = PendingPick {
            region,
            bodies,
            result,
        };
        (pass, pending)
    }

    /// Draws every body's ids, copies the region out and asks for the map.
    /// On its own encoder, submitted here: the map may only be asked for
    /// once the copy that fills the buffer is submitted, and egui's encoder
    /// is submitted after `prepare` returns.
    pub fn run(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &Pipelines,
        camera: &wgpu::BindGroup,
        models: &wgpu::BindGroup,
        bodies: &[BodyDraw],
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("arrix-viewport pick"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("arrix-viewport pick pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 0 is nothing (`crate::pick`).
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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
            pass.set_bind_group(0, camera, &[]);
            let pickable = || bodies.iter().filter(|b| b.pickable);
            // Every face before the first edge, every edge before the first
            // vertex: each tests against all the faces.
            pass.set_pipeline(&pipelines.pick_faces);
            for body in pickable() {
                pass.set_bind_group(1, models, &[body.model_offset]);
                pass.set_vertex_buffer(0, body.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, body.face_ids.slice(..));
                pass.set_index_buffer(body.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..body.index_count, 0, 0..1);
            }
            pass.set_pipeline(&pipelines.pick_edges);
            for body in pickable() {
                pass.set_bind_group(1, models, &[body.model_offset]);
                pass.set_vertex_buffer(0, body.edge_buffer.slice(..));
                pass.set_vertex_buffer(1, body.edge_ids.slice(..));
                pass.draw(0..body.edge_vertex_count, 0..1);
            }
            pass.set_pipeline(&pipelines.pick_points);
            for body in pickable() {
                pass.set_bind_group(1, models, &[body.model_offset]);
                pass.set_vertex_buffer(0, body.point_buffer.slice(..));
                pass.set_vertex_buffer(1, body.point_ids.slice(..));
                pass.draw(0..body.point_count, 0..1);
            }
        }
        let r = self.region;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: r.origin.0,
                    y: r.origin.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_STRIDE),
                    rows_per_image: Some(r.height),
                },
            },
            wgpu::Extent3d {
                width: r.width,
                height: r.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        // Answered whenever the device is next polled (`Renderer::poll`).
        let result = self.result.clone();
        let buffer = self.buffer.clone();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapped| {
                if mapped.is_err() {
                    return;
                }
                let ids = match buffer.slice(..).get_mapped_range() {
                    Ok(data) => unpack(&data, r),
                    Err(_) => return,
                };
                buffer.unmap();
                if let Ok(mut slot) = result.lock() {
                    *slot = Some(ids);
                }
            });
    }
}

/// The region's ids out of the mapped rows, row by row.
fn unpack(data: &[u8], region: PickRegion) -> Vec<u32> {
    let mut ids = Vec::with_capacity((region.width * region.height) as usize);
    for row in 0..region.height as usize {
        for col in 0..region.width as usize {
            let at = row * ROW_STRIDE as usize + col * 4;
            let bytes = data.get(at..at + 4).and_then(|b| b.try_into().ok());
            ids.push(bytes.map_or(0, u32::from_le_bytes));
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_padded_rows_unpack_to_the_region() {
        let region = PickRegion::around((0, 0), (2, 2));
        let mut data = vec![0u8; ROW_STRIDE as usize * 2];
        data[..8].copy_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
        data[256..264].copy_from_slice(&[3, 0, 0, 0, 4, 0, 0, 0]);
        assert_eq!(unpack(&data, region), [1, 2, 3, 4]);
    }
}
