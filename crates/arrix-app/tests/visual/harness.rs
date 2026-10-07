//! Shared scaffolding for the visual scenarios and the scratch render
//! (docs/UI-RENDERING.md §Visual debugging, ADR-0001).
//!
//! Pitfalls encoded here, so no one relearns them: `Harness::step` runs
//! egui's logic pass only and does no GPU work, so anything that depends on
//! a rendered frame needs [`pump_rendered`]; scenarios run one at a time,
//! because concurrent software devices are not safe; and no wall clock may
//! reach a drawn pixel.

use arrix_app::ArrixApp;
use arrix_config::{ConfigSource, MemorySource};
use egui_kittest::Harness;

/// The window every scenario renders into. The goldens encode it.
pub const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

/// The reference adapter's name, or `None` when this machine has none.
///
/// The reference is a CPU adapter, Mesa's lavapipe (C1 risk register):
/// `egui_kittest` prefers one, and a hardware GPU rasterises differently, so
/// a scenario rendered on one would be compared against pixels it cannot
/// reproduce. Probing first also turns `egui_kittest`'s "Failed to create
/// render state" panic into a readable skip.
pub fn reference_adapter() -> Option<String> {
    use egui_wgpu::wgpu;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    adapters
        .iter()
        .map(|a| a.get_info())
        .find(|info| info.device_type == wgpu::DeviceType::Cpu)
        .map(|info| format!("{} ({:?}, {})", info.name, info.backend, info.driver_info))
}

/// Whether to run, with the skip said out loud when not. With
/// `ARRIX_REQUIRE_GPU=1` (CI, which installs lavapipe) a missing adapter
/// fails instead: a suite that silently skips everywhere looks like coverage.
fn can_render(what: &str) -> bool {
    if reference_adapter().is_some() {
        return true;
    }
    assert!(
        !env_flag("ARRIX_REQUIRE_GPU"),
        "{what}: no CPU wgpu adapter, and ARRIX_REQUIRE_GPU is set"
    );
    // Written to the process's stderr, not through `eprintln!`: the test
    // runner captures the macro's output of a passing test, which would make
    // the skip silent.
    use std::io::Write as _;
    let _ = writeln!(
        std::io::stderr(),
        "SKIPPING {what}: no CPU wgpu adapter on this machine. Install Mesa's lavapipe \
         (`mesa-vulkan-drivers`) to run the visual suite."
    );
    false
}

/// Whether the environment variable `name` is set to something other than
/// empty, `0` or `false`.
pub fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !matches!(v.as_str(), "" | "0" | "false"))
}

/// Serialises everything that renders: concurrent lavapipe devices at this
/// size have crashed inside the driver under `cargo test --workspace`.
fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A panicking scenario poisons the lock; the ones after it should report
    // their own result, not "poisoned mutex".
    GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Builds the real app over `egui_kittest`'s wgpu renderer, exactly as
/// `eframe::run_native` would construct it, and settles it.
fn app_harness(size: egui::Vec2, source: &impl ConfigSource) -> Harness<'static, ArrixApp> {
    let mut harness = Harness::builder()
        .with_size(size)
        .wgpu()
        .build_eframe(|cc| ArrixApp::with_config(cc, source));
    settle(&mut harness);
    harness
}

/// Steps until egui stops asking for repaints. Logic only: see
/// [`pump_rendered`] for anything that needs the GPU to have run.
pub fn settle(harness: &mut Harness<'_, ArrixApp>) {
    harness.run();
}

/// Opens the document at `path`, relative to the workspace root, and steps
/// until its session has said everything about it: evaluated, and every
/// body meshed refined (`debug_state().evaluation.settled`). The session's
/// threads run on their own clock, so this polls; it fails after a minute.
pub fn open_settled(harness: &mut Harness<'_, ArrixApp>, path: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    harness
        .state_mut()
        .open_path(&root.join(path))
        .unwrap_or_else(|err| panic!("{err}"));
    wait_settled(harness);
}

/// Steps until the active document is settled.
pub fn wait_settled(harness: &mut Harness<'_, ArrixApp>) {
    let started = web_time::Instant::now();
    loop {
        // `run` would fail on the repaints the pending document asks for.
        harness.step();
        let settled = harness
            .state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.settled);
        if settled {
            harness.run();
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "the document did not settle: {}",
            harness.state().debug_state_json()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Steps and renders `frames` frames.
#[allow(dead_code, reason = "no scenario needs rendered input frames yet")]
pub fn pump_rendered(harness: &mut Harness<'_, ArrixApp>, frames: usize) {
    for _ in 0..frames {
        harness.step();
        harness.render().expect("render");
    }
}

/// Runs `body` against the real app at [`SIZE`], skipping without a
/// reference adapter.
pub fn with_app(name: &str, body: impl FnOnce(&mut Harness<'_, ArrixApp>)) {
    with_config(name, &MemorySource::default(), body);
}

/// [`with_app`] on the config files `source` holds.
pub fn with_config(
    name: &str,
    source: &impl ConfigSource,
    body: impl FnOnce(&mut Harness<'_, ArrixApp>),
) {
    if !can_render(name) {
        return;
    }
    let _gpu = gpu_lock();
    let mut harness = app_harness(SIZE, source);
    body(&mut harness);
}

/// [`with_app`] on the config directory `dir`, which the app keeps to
/// reload from: the scenario edits the files, then runs `core.config.reload`.
pub fn with_config_dir(
    name: &str,
    dir: &std::path::Path,
    body: impl FnOnce(&mut Harness<'_, ArrixApp>),
) {
    if !can_render(name) {
        return;
    }
    let _gpu = gpu_lock();
    let dir = dir.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(SIZE)
        .wgpu()
        .build_eframe(move |cc| {
            ArrixApp::with_config_source(cc, Box::new(arrix_app::config_files::ConfigDir::new(dir)))
        });
    settle(&mut harness);
    body(&mut harness);
}

/// Renders the current frame.
pub fn render(harness: &mut Harness<'_, ArrixApp>) -> image::RgbaImage {
    harness.render().expect("render")
}

/// Draws a state to `target/visual-scratch/scratch.{png,json}` and compares
/// against nothing: the way to look before pinning a scenario.
#[allow(
    dead_code,
    reason = "used by the `visual_scratch` target, which includes this file by path"
)]
pub fn scratch(body: impl FnOnce(&mut Harness<'_, ArrixApp>)) {
    with_app("scratch render", |harness| {
        body(harness);
        let dir = target_dir().join("visual-scratch");
        std::fs::create_dir_all(&dir).expect("create target/visual-scratch");
        let png = dir.join("scratch.png");
        let json = dir.join("scratch.json");
        render(harness).save(&png).expect("write scratch.png");
        std::fs::write(&json, format!("{}\n", harness.state().debug_state_json()))
            .expect("write scratch.json");
        println!(
            "scratch render on {}:\n  {}\n  {}",
            reference_adapter().unwrap_or_default(),
            png.display(),
            json.display()
        );
    });
}

/// The workspace's `target/`, printed without `..` so the paths are
/// clickable.
pub fn target_dir() -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&dir).ok();
    dir.canonicalize().unwrap_or(dir)
}
