//! Where a file is chosen and written (docs/ARCHITECTURE.md §Gates, the
//! wasm lint's one place for `std::fs` beside the config files): the
//! save dialog behind a seam, so a scenario answers it, and the write of
//! the bytes that were asked for. The browser build hands bytes to a
//! download instead, in C4.

use std::path::{Path, PathBuf};

/// Asks the user where to save an export.
pub trait SaveDialog {
    /// The path chosen for a file suggested as `name`, or `None` when the
    /// user cancelled.
    fn choose(&mut self, name: &str) -> Option<PathBuf>;
}

/// The platform's save dialog, filtered to the exchange formats; none on
/// wasm, where it answers cancelled.
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
    fn choose(&mut self, _name: &str) -> Option<PathBuf> {
        None
    }
}

/// Writes `bytes` to `path`; the error says where and why, for the user.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
