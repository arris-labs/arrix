//! `arrix-app [<document> ...]`: each path is a document directory or an
//! `.arrx`, opened as a tab.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let paths: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
    eframe::run_native(
        "ArriX",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            let mut app = arrix_app::ArrixApp::native(cc);
            for path in &paths {
                if let Err(err) = app.open_path(path) {
                    eprintln!("arrix-app: {err}");
                }
            }
            Ok(Box::new(app))
        }),
    )
}

// The browser build loads the library from a host page (C4); the binary
// still has to compile for the workspace-wide wasm build.
#[cfg(target_arch = "wasm32")]
fn main() {}
