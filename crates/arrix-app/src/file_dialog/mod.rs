//! Where a file is chosen and written (docs/ARCHITECTURE.md §Gates, the
//! wasm lint's one place for `std::fs` beside the config files): the
//! save dialog behind a seam, so a scenario answers it, and the write of
//! the bytes that were asked for. The browser build hands bytes to a
//! download instead (`web_files`).

use std::path::{Path, PathBuf};

/// Asks the user where to save an export.
pub trait SaveDialog {
    /// The path chosen for a file suggested as `name`, or `None` when the
    /// user cancelled.
    fn choose(&mut self, name: &str) -> Option<PathBuf>;
}

/// Asks the user which file to import.
pub trait OpenDialog {
    /// The path chosen, or `None` when the user cancelled.
    fn choose(&mut self) -> Option<PathBuf>;
}

/// The platform's save dialog, filtered to the exchange formats. On wasm
/// there is none: the export is a download named as suggested, and the
/// import opens the page's file picker, whose bytes `web_files` takes in
/// later.
#[derive(Default)]
pub struct NativeDialog;

#[cfg(not(target_arch = "wasm32"))]
impl SaveDialog for NativeDialog {
    fn choose(&mut self, name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Export")
            .set_file_name(name)
            .add_filter("STEP", &["step", "stp"])
            .add_filter("STL", &["stl"])
            .add_filter("OBJ", &["obj"])
            .save_file()
    }
}

#[cfg(target_arch = "wasm32")]
impl SaveDialog for NativeDialog {
    fn choose(&mut self, name: &str) -> Option<PathBuf> {
        Some(PathBuf::from(name))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl OpenDialog for NativeDialog {
    fn choose(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Import STEP")
            .add_filter("STEP", &["step", "stp"])
            .pick_file()
    }
}

#[cfg(target_arch = "wasm32")]
impl OpenDialog for NativeDialog {
    fn choose(&mut self) -> Option<PathBuf> {
        crate::web_files::pick();
        None
    }
}

/// Reads the file at `path`; the error says where and why, for the user.
pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes `bytes` to `path`; the error says where and why, for the user.
#[cfg(not(target_arch = "wasm32"))]
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Hands `bytes` to the browser as a download named by `path`'s file name.
#[cfg(target_arch = "wasm32")]
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("export");
    crate::web_files::download(name, bytes)
}
