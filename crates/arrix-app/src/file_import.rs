//! The shell's `Import…` (ADR-0032 §2): asks which file, has the session
//! read its product tree on the worker (a request, not a command, and never
//! the kernel on this thread), and when the tree comes back turns it into
//! the one `Group` that adds the file's parts, assemblies and materials.
//! A file of one part is that part; an assembly file is its assemblies.

use arrix_core::Diagnostic;
use arrix_doc::{ComponentOf, Session, StepTree, import_products};

use crate::ArrixApp;
use crate::context::Context;
use crate::documents::OpenDocument;

/// A file asked for and its tree not yet heard.
struct Pending {
    tag: u64,
    file_name: String,
    bytes: Vec<u8>,
}

/// The imports of one document in flight, and those heard and not yet
/// turned into a command, in the order asked.
#[derive(Default)]
pub struct Imports {
    next: u64,
    waiting: Vec<Pending>,
    heard: Vec<(Pending, Result<StepTree, Diagnostic>)>,
}

impl Imports {
    /// A `StepTree` event: the answer to the request tagged `tag`.
    pub(crate) fn heard(&mut self, tag: u64, tree: &Result<StepTree, Diagnostic>) {
        if let Some(at) = self.waiting.iter().position(|p| p.tag == tag) {
            let pending = self.waiting.remove(at);
            self.heard.push((pending, tree.clone()));
        }
    }
}

impl OpenDocument {
    fn request_tree(&mut self, file_name: String, bytes: Vec<u8>) -> Result<(), String> {
        let tag = self.imports.next;
        self.imports.next += 1;
        self.session()
            .read_step_tree(tag, bytes.clone())
            .map_err(|e| e.to_string())?;
        self.imports.waiting.push(Pending {
            tag,
            file_name,
            bytes,
        });
        Ok(())
    }
}

impl ArrixApp {
    /// Replaces the dialog the import asks (a scenario answers it).
    pub fn set_open_dialog(&mut self, dialog: Box<dyn crate::file_dialog::OpenDialog>) {
        self.open_dialog = dialog;
    }

    /// `core.import-step.create`: asks which file, then asks the session
    /// for its tree. A cancelled dialog does nothing.
    pub(crate) fn import_step(&mut self) {
        let Some(path) = self.open_dialog.choose() else {
            return;
        };
        let bytes = match crate::file_dialog::read(&path) {
            Ok(bytes) => bytes,
            Err(reason) => {
                self.session.notice = Some(reason);
                return;
            }
        };
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("import.step")
            .to_owned();
        if let Some(document) = self.documents.get_mut(self.active)
            && let Err(reason) = document.request_tree(file_name, bytes)
        {
            self.session.notice = Some(reason);
        }
    }

    /// Turns the trees the sessions have answered into their one command
    /// each, each its own gesture; what went wrong is the notice, what was
    /// left out too.
    pub(crate) fn finish_imports(&mut self) {
        let mut heard = Vec::new();
        for (index, document) in self.documents.iter_mut().enumerate() {
            heard.extend(
                std::mem::take(&mut document.imports.heard)
                    .into_iter()
                    .map(|(pending, tree)| (index, pending, tree)),
            );
        }
        for (index, pending, tree) in heard {
            self.gesture("import tree", |app| app.apply_import(index, pending, tree));
        }
    }

    fn apply_import(&mut self, index: usize, pending: Pending, tree: Result<StepTree, Diagnostic>) {
        let Some(document) = self.documents.get_mut(index) else {
            return;
        };
        let result = tree
            .map_err(|d| format!("import failed: {}: {}", d.code, d.message))
            .and_then(|tree| {
                import_products(
                    document.replica.document(),
                    &pending.file_name,
                    pending.bytes,
                    &tree,
                    &mut self.ids,
                )
                .map_err(|e| format!("import failed: {}: {e}", e.code()))
            })
            .and_then(|imported| {
                let left_out = imported.notes.first().map(|d| d.message.clone());
                let first = imported.roots.first().copied();
                document.submit(imported.command)?;
                Ok((first, left_out))
            });
        match result {
            Ok((first, left_out)) => {
                document.view.component_selection = None;
                match first {
                    Some(ComponentOf::Part(part)) => document.set_context(Context::Part(part)),
                    Some(ComponentOf::Assembly(assembly)) => {
                        document.set_context(Context::Assembly(assembly));
                    }
                    None => {}
                }
                self.session.notice = left_out;
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }
}
