//! `core.import-step` (docs/DATA-MODEL.md §The core feature types,
//! ADR-0029): one solid of the STEP file the record keeps, read in metres
//! and named from the file's own entities. It has no inputs; the file is
//! the record's, a blob in the document.

use arrix_core::{Diagnostic, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, Kernel, OutputValue, ParamSpec, SlotKind, SlotOutput, SlotSpec,
};

use crate::registry::{FeatureArgs, FeatureType, FileArg, FileKernel, TypeOutput};

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

/// `core.import-step`.
pub struct CoreImportStep {
    spec: FeatureTypeSpec,
}

impl CoreImportStep {
    pub fn new() -> Self {
        Self {
            spec: FeatureTypeSpec {
                id: "core.import-step".into(),
                version: 1,
                title: "Import STEP".into(),
                params: vec![ParamSpec {
                    name: "solid".into(),
                    title: "Solid of the file".into(),
                    kind: QuantityKind::Count,
                    default: "1".into(),
                    shown_when: None,
                }],
                inputs: vec![],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: None,
                }],
                choices: vec![],
            },
        }
    }
}

impl Default for CoreImportStep {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureType for CoreImportStep {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(&self, _: &mut dyn Kernel, _: &FeatureArgs) -> Result<TypeOutput, Diagnostic> {
        Err(no_file())
    }

    fn reads_file(&self) -> bool {
        true
    }

    fn evaluate_file(
        &self,
        kernel: &mut dyn FileKernel,
        args: &FeatureArgs,
        file: Option<FileArg<'_>>,
    ) -> Result<TypeOutput, Diagnostic> {
        let file = file.ok_or_else(no_file)?;
        let asked = args.param("solid").unwrap_or(1.0);
        let solid = if asked.fract() == 0.0 && (1.0..=f64::from(u32::MAX)).contains(&asked) {
            asked as u32
        } else {
            return Err(error(
                "import.no-solid",
                format!("the solid of {} is counted from 1, not {asked}", file.name),
            ));
        };
        let body = kernel.import_step(file.bytes, solid)?;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(body),
            }],
            tool: None,
        }
        .into())
    }
}

fn no_file() -> Diagnostic {
    error(
        "import.no-file",
        "the feature keeps no file to import: add a STEP file to the document",
    )
}
