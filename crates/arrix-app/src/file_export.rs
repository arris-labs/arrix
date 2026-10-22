//! The shell's export (docs/ARCHITECTURE.md §The protocol boundary,
//! `Export`): a request to the session, not a command, so it makes no
//! generation and no gesture count. The file comes back as an event, later
//! than the request, and is written where the dialog said.

use std::collections::VecDeque;
use std::path::PathBuf;

use arrix_core::{Diagnostic, ExportFormat};
use arrix_doc::{BodySlot, Session};

use arrix_viewport::Hit;

use crate::ArrixApp;
use crate::context::Context;
use crate::documents::OpenDocument;

/// The exports of one document in flight, and those heard and not yet
/// written, in the order asked.
#[derive(Default)]
pub struct Exports {
    waiting: VecDeque<PathBuf>,
    heard: Vec<(PathBuf, Result<Vec<u8>, Diagnostic>)>,
}

impl Exports {
    /// An `Exported` event: the oldest request's answer.
    pub(crate) fn heard(&mut self, bytes: &Result<Vec<u8>, Diagnostic>) {
        if let Some(path) = self.waiting.pop_front() {
            self.heard.push((path, bytes.clone()));
        }
    }

    /// Whether a request is asked and its file not yet heard: the app keeps
    /// drawing frames until it is, since the answer wakes nothing.
    pub(crate) fn pending(&self) -> bool {
        !self.waiting.is_empty()
    }

    fn take(&mut self) -> Vec<(PathBuf, Result<Vec<u8>, Diagnostic>)> {
        std::mem::take(&mut self.heard)
    }
}

impl OpenDocument {
    /// The bodies the selection is in, as the export names them; empty when
    /// nothing is selected, which exports every body.
    fn selected_bodies(&self) -> Vec<BodySlot> {
        let Some(Hit { name, .. }) = &self.view.selection else {
            return Vec::new();
        };
        self.view
            .scene
            .entries()
            .filter(|entry| entry.names.find(name).is_some())
            .map(|entry| BodySlot {
                feature: entry.key.feature,
                slot: entry.key.slot.clone(),
            })
            .collect()
    }

    /// Asks the session to write the selected bodies (all, with none
    /// selected) as `format` to `path` once it has them.
    ///
    /// In an assembly's context a STEP file is the assembly with its
    /// product structure (ADR-0032 §5), whatever is selected.
    fn request_export(&mut self, path: PathBuf, format: ExportFormat) -> Result<(), String> {
        let asked = match (self.context(), format) {
            (Some(Context::Assembly(assembly)), ExportFormat::Step) => {
                self.session().export_assembly(assembly)
            }
            _ => {
                let bodies = self.selected_bodies();
                self.session().export(bodies, format)
            }
        };
        asked.map_err(|e| e.to_string())?;
        self.exports.waiting.push_back(path);
        Ok(())
    }
}

impl ArrixApp {
    /// Replaces the dialog the export asks (a scenario answers it).
    pub fn set_save_dialog(&mut self, dialog: Box<dyn crate::file_dialog::SaveDialog>) {
        self.dialog = dialog;
    }

    /// `core.file.export`: asks where, then asks the session for the file.
    /// The extension names the format; a cancelled dialog does nothing.
    pub(crate) fn export_file(&mut self) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let stem = std::path::Path::new(&document.title)
            .file_stem()
            .map_or_else(|| "document".into(), |s| s.to_string_lossy().into_owned());
        let name = format!("{}.step", stem.replace(' ', "-").to_lowercase());
        let Some(path) = self.dialog.choose(&name) else {
            return;
        };
        let format = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(ExportFormat::from_extension);
        let Some(format) = format else {
            self.session.notice =
                Some(format!("{}: export as .step, .stl or .obj", path.display()));
            return;
        };
        if let Some(document) = self.documents.get_mut(self.active)
            && let Err(reason) = document.request_export(path, format)
        {
            self.session.notice = Some(reason);
        }
    }

    /// Writes the exports the sessions have answered; what went wrong is
    /// the notice.
    pub(crate) fn write_exports(&mut self) {
        for document in &mut self.documents {
            for (path, bytes) in document.exports.take() {
                let result = bytes
                    .map_err(|d| format!("export failed: {}: {}", d.code, d.message))
                    .and_then(|bytes| {
                        crate::file_dialog::write(&path, &bytes).map(|()| bytes.len())
                    });
                self.session.notice = Some(match result {
                    Ok(n) => format!("Exported {n} bytes to {}", path.display()),
                    Err(reason) => reason,
                });
            }
        }
    }
}
