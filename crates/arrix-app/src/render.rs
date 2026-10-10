//! The headless render (ADR-0025, docs/UI-RENDERING.md §Visual debugging):
//! the real app on its own `egui::Context`, painted through
//! `egui_wgpu::Renderer` into a texture and read back, with no window. It
//! drives the app as the visual harness does (the same input, style and
//! frame step), so a render and a scenario of the same state agree, and
//! pixels are deterministic on a CPU adapter (lavapipe) only.

use std::sync::Arc;
use std::time::Duration;

use arrix_config::MemorySource;
use eframe::egui_wgpu::{self, RenderState, ScreenDescriptor, wgpu};

use crate::ArrixApp;

/// What `render` draws into.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// The window, in points (one pixel each).
    pub size: egui::Vec2,
    /// Plugins left out of the build, as `arrix eval --without-plugin`.
    pub without: Vec<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            size: egui::vec2(1440.0, 900.0),
            without: Vec::new(),
        }
    }
}

/// A rendered frame and what the app believed it drew.
pub struct Rendered {
    /// RGBA, row-major, `size` pixels.
    pub pixels: Vec<u8>,
    pub size: [u32; 2],
    /// `debug_state()` as JSON.
    pub debug_state: String,
    /// The wgpu adapter it was drawn on: name, backend and driver.
    pub adapter: String,
}

/// How long a document may take to evaluate and mesh before the render
/// gives up on it.
const SETTLE: Duration = Duration::from_secs(120);

/// The longest the GPU may take to finish a frame or a readback.
const GPU_WAIT: Duration = Duration::from_secs(30);

/// The steps egui is given to stop asking for repaints once the document
/// has settled, as the harness's `run` allows.
const MAX_STEPS: usize = 4;

/// The harness's frame step (`egui_kittest`'s default), so a frame's
/// animations advance alike.
const STEP_DT: f32 = 0.25;

/// Opens the document at `path` (a document directory or an `.arrx`) in
/// the app on its built-in configuration, waits until it is evaluated and
/// every body meshed refined, and renders one frame.
pub fn render_document(
    path: &std::path::Path,
    options: &RenderOptions,
) -> Result<Rendered, String> {
    render_with(options, |app| {
        let without: Vec<&str> = options.without.iter().map(String::as_str).collect();
        app.open_path_without(path, &without)
    })
}

/// Renders the app at `options.size` after `setup` has run on it and the
/// active document, if any, has settled.
pub fn render_with(
    options: &RenderOptions,
    setup: impl FnOnce(&mut ArrixApp) -> Result<(), String>,
) -> Result<Rendered, String> {
    let state = render_state()?;
    let adapter = {
        let info = state.adapter.get_info();
        format!("{} ({:?}, {})", info.name, info.backend, info.driver_info)
    };
    let ctx = egui::Context::default();
    let mut app = ArrixApp::headless(&ctx, Some(&state), &MemorySource::default());
    let mut frames = Frames::new(&ctx, &state, options.size);
    frames.step(&mut app);
    frames.settle(&mut app);
    setup(&mut app)?;
    let started = web_time::Instant::now();
    loop {
        frames.step(&mut app);
        if app.debug_state().evaluation.is_none_or(|e| e.settled) {
            break;
        }
        if started.elapsed() > SETTLE {
            return Err(format!(
                "the document did not settle in {} s",
                SETTLE.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    app.skip_animations();
    frames.settle(&mut app);
    let (pixels, size) = frames.paint()?;
    Ok(Rendered {
        pixels,
        size,
        debug_state: app.debug_state_json(),
        adapter,
    })
}

/// A wgpu device with no surface, on a CPU adapter when there is one and
/// otherwise on the best other: the harness's preference, so both draw on
/// lavapipe where it is installed.
fn render_state() -> Result<RenderState, String> {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup
        .instance_descriptor
        .backends
        .remove(wgpu::Backends::BROWSER_WEBGPU);
    setup.native_adapter_selector = Some(Arc::new(|adapters, _surface| {
        let rank = |a: &wgpu::Adapter| {
            let info = a.get_info();
            let device = match info.device_type {
                wgpu::DeviceType::Cpu => 0,
                wgpu::DeviceType::DiscreteGpu => 1,
                _ => 2,
            };
            let backend = match info.backend {
                wgpu::Backend::Metal => 0,
                wgpu::Backend::Vulkan => 1,
                wgpu::Backend::Dx12 => 2,
                wgpu::Backend::Gl => 4,
                _ => 6,
            };
            (device, backend)
        };
        adapters
            .iter()
            .min_by_key(|a| rank(a))
            .cloned()
            .ok_or_else(|| "no wgpu adapter".to_owned())
    }));
    let setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    let instance = pollster::block_on(setup.new_instance());
    pollster::block_on(RenderState::create(
        &egui_wgpu::WgpuConfiguration {
            wgpu_setup: setup,
            ..Default::default()
        },
        &instance,
        None,
        egui_wgpu::RendererOptions::PREDICTABLE,
    ))
    .map_err(|err| format!("no wgpu device to render on: {err}"))
}

/// The app's frames on one context, as the harness steps them.
struct Frames<'a> {
    ctx: &'a egui::Context,
    state: &'a RenderState,
    input: egui::RawInput,
    output: Option<egui::FullOutput>,
}

impl<'a> Frames<'a> {
    fn new(ctx: &'a egui::Context, state: &'a RenderState, size: egui::Vec2) -> Self {
        ctx.set_theme(egui::Theme::Dark);
        ctx.set_os(egui::os::OperatingSystem::Nix);
        ctx.all_styles_mut(|style| {
            style.visuals.text_cursor.blink = false;
            style.scroll_animation = egui::style::ScrollAnimation::none();
            style.animation_time = 0.0;
        });
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        if let Some(root) = input.viewports.get_mut(&egui::ViewportId::ROOT) {
            root.native_pixels_per_point = Some(1.0);
        }
        Self {
            ctx,
            state,
            input,
            output: None,
        }
    }

    /// One frame: the app's logic and layout, and its textures uploaded.
    fn step(&mut self, app: &mut ArrixApp) {
        let mut input = self.input.clone();
        input.predicted_dt = STEP_DT;
        let mut output = self.ctx.run_ui(input, |ui| app.show(ui));
        let mut renderer = self.state.renderer.write();
        #[expect(clippy::iter_over_hash_type, reason = "each id once; order is moot")]
        for (id, images) in output.textures_delta.set.drain() {
            for image in images {
                renderer.update_texture(&self.state.device, &self.state.queue, id, &image);
            }
        }
        #[expect(clippy::iter_over_hash_type, reason = "freeing is order-free")]
        for id in output.textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
        self.output = Some(output);
    }

    /// Steps until egui stops asking for a repaint, up to [`MAX_STEPS`].
    fn settle(&mut self, app: &mut ArrixApp) {
        for _ in 0..MAX_STEPS {
            self.step(app);
            let wants = self.output.as_ref().is_some_and(|o| {
                o.viewport_output
                    .get(&egui::ViewportId::ROOT)
                    .is_some_and(|v| v.repaint_delay.is_zero())
            });
            if !wants {
                return;
            }
        }
    }

    /// Paints the last frame into a texture and reads it back as RGBA.
    fn paint(&self) -> Result<(Vec<u8>, [u32; 2]), String> {
        let output = self.output.as_ref().ok_or("no frame was run")?;
        let state = self.state;
        let ppp = self.ctx.pixels_per_point();
        let size = self.ctx.content_rect().size() * ppp;
        let screen = ScreenDescriptor {
            pixels_per_point: ppp,
            size_in_pixels: [size.x.round() as u32, size.y.round() as u32],
        };
        let tessellated = self.ctx.tessellate(output.shapes.clone(), ppp);
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("arrix render"),
            });
        let mut renderer = state.renderer.write();
        let user_buffers = renderer.update_buffers(
            &state.device,
            &state.queue,
            &mut encoder,
            &tessellated,
            &screen,
        );
        let texture = state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("arrix render target"),
            size: wgpu::Extent3d {
                width: screen.size_in_pixels[0],
                height: screen.size_in_pixels[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: state.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("arrix render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &tessellated, &screen);
        }
        drop(renderer);
        state
            .queue
            .submit(user_buffers.into_iter().chain([encoder.finish()]));
        let pixels = read_back(state, &texture)?;
        Ok((pixels, screen.size_in_pixels))
    }
}

/// The texture's pixels as RGBA rows, without wgpu's row padding.
fn read_back(state: &RenderState, texture: &wgpu::Texture) -> Result<Vec<u8>, String> {
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let row = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = row.div_ceil(align) * align;
    let buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("arrix render readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("arrix render readback"),
        });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded as u32),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    let submission = state.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| drop(sender.send(r)));
    state
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(GPU_WAIT),
        })
        .map_err(|err| format!("the GPU did not finish the frame: {err}"))?;
    receiver
        .recv()
        .map_err(|err| err.to_string())?
        .map_err(|err| format!("the frame could not be read back: {err}"))?;
    let mapped = slice
        .get_mapped_range()
        .map_err(|err| format!("the frame could not be read back: {err}"))?;
    let mut pixels = Vec::with_capacity(row * height);
    for line in mapped.chunks_exact(padded) {
        pixels.extend_from_slice(&line[..row]);
    }
    if matches!(
        texture.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for px in pixels.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
    }
    Ok(pixels)
}
