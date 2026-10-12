//! The Assembly tab's commands (ADR-0030 §6): importing a STEP file as a
//! new part, and making an assembly. Each is one gesture and one command.

use std::collections::BTreeMap;
use std::path::Path;

use arrix_core::{AssemblyId, FeatureId, PartId};
use arrix_doc::{Assembly, BlobRef, Command, FeatureRecord, FeatureTypeId, ImportedFile, Part};

use crate::ArrixApp;
use crate::context::Context;

/// `servo.step` as the part and feature it makes are named: `Servo`.
fn title_of(file: &Path) -> String {
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Import");
    let mut chars = stem.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

impl ArrixApp {
    /// Replaces the dialog the import asks (a scenario answers it).
    pub fn set_open_dialog(&mut self, dialog: Box<dyn crate::file_dialog::OpenDialog>) {
        self.open_dialog = dialog;
    }

    /// `core.import-step.create`: asks which file, then adds it, a part and
    /// the `core.import-step` feature that reads it as one `Group`; the tab
    /// then looks at the new part. A cancelled dialog does nothing.
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
        let title = title_of(&path);
        let version = self
            .feature_types
            .iter()
            .find(|t| t.id == "core.import-step")
            .map_or(1, |t| t.version);
        let part = PartId(self.ids.next_id());
        let feature = FeatureId(self.ids.next_id());
        let record = FeatureRecord {
            id: feature,
            type_id: FeatureTypeId::new("core.import-step").expect("a literal type id"),
            type_version: version,
            name: title.clone(),
            params: BTreeMap::new(),
            choices: BTreeMap::new(),
            inputs: BTreeMap::new(),
            suppressed: false,
            sketch: None,
            frozen: None,
            file: Some(ImportedFile {
                name: file_name.clone(),
                blob: BlobRef::of(&bytes, "step"),
            }),
        };
        let command = Command::Group {
            label: format!("Import {file_name}"),
            commands: vec![
                Command::AddFile { bytes },
                Command::AddPart {
                    part: Part {
                        id: part,
                        name: title,
                        history: Vec::new(),
                        features: BTreeMap::new(),
                        rollback: None,
                        material: None,
                        body_materials: Default::default(),
                    },
                },
                Command::AddFeature {
                    part,
                    at: 0,
                    record,
                },
            ],
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match document.submit(command) {
            Ok(_) => {
                document.view.component_selection = None;
                document.set_context(Context::Part(part));
                self.session.notice = None;
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

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
