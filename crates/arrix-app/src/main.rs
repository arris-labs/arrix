#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "ArriX",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(arrix_app::ArrixApp))),
    )
}

// The browser build loads the library from a host page (C4); the binary
// still has to compile for the workspace-wide wasm build.
#[cfg(target_arch = "wasm32")]
fn main() {}
