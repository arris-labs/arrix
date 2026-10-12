//! The Assembly tab's command that makes an assembly (ADR-0030 §6): one
//! gesture, one command. Importing a STEP file is `file_import`.

use std::collections::BTreeMap;

use arrix_core::AssemblyId;
use arrix_doc::{Assembly, Command};

use crate::ArrixApp;
use crate::context::Context;

impl ArrixApp {
    /// `core.assembly.create`: an empty assembly, which the tab then looks
    /// at.
    pub(crate) fn create_assembly(&mut self) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let used = |n: &str| {
            document
                .replica
                .document()
                .assemblies()
                .values()
                .any(|a| a.name == n)
        };
        let name = (1..)
            .map(|i| format!("Assembly {i}"))
            .find(|n| !used(n))
            .expect("an unbounded range has a free name");
        let id = AssemblyId(self.ids.next_id());
        let command = Command::AddAssembly {
            assembly: Assembly {
                id,
                name,
                components: BTreeMap::new(),
                joints: BTreeMap::new(),
            },
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match document.submit(command) {
            Ok(_) => {
                document.view.component_selection = None;
                document.set_context(Context::Assembly(id));
                self.session.notice = None;
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }
}
