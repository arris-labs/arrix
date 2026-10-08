//! A finished evaluation's final bodies as an exchange file
//! (docs/ARCHITECTURE.md §The protocol boundary, `Export`): the bodies a
//! user sees, in the document's order, through the kernel's recorded
//! export.

use arrix_core::{Diagnostic, ExportFormat, Severity};

use super::{BodySlot, EvalLine, Evaluator, FinalBody, host};
use crate::open::{DocumentSource, OpenError, open};
use crate::registry::Registry;

/// A document exported headless: the file, and the evaluation it came
/// from as `arrix eval` prints it, so a caller can tell a clean document
/// from one some feature of which failed (its bodies are the ones that
/// evaluated).
#[derive(Clone, Debug, PartialEq)]
pub struct ExportedFile {
    pub bytes: Vec<u8>,
    pub line: EvalLine,
}

/// Why a document could not be exported.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error("{}: {}", .0.code, .0.message)]
    Export(Diagnostic),
}

/// Opens and evaluates like [`eval`](super::eval), then writes `bodies`
/// (every final body when empty) as `format`, as the evaluator's
/// [`export`](Evaluator::export) does.
pub fn export(
    doc: &str,
    source: &impl DocumentSource,
    registry: &Registry,
    bodies: &[BodySlot],
    format: ExportFormat,
) -> Result<ExportedFile, ExportError> {
    let document = open(source)?;
    let mut evaluator = Evaluator::new(registry.clone());
    let evaluation = evaluator.evaluate(&document);
    let bytes = evaluator
        .export(bodies, format)
        .map_err(ExportError::Export)?;
    Ok(ExportedFile {
        bytes,
        line: evaluation.line(doc, &document),
    })
}

fn error(code: &str, message: String) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl Evaluator {
    /// The latest evaluation's final bodies named in `bodies` (every one
    /// when it is empty) written as `format`. They go in the document's
    /// order whatever the order asked: the same selection, the same bytes.
    /// `export.unknown-body` names a slot that is no final body, and
    /// `export.no-bodies` says there is nothing to write; the kernel's
    /// refusals are `kernel.export.*`.
    pub fn export(
        &mut self,
        bodies: &[BodySlot],
        format: ExportFormat,
    ) -> Result<Vec<u8>, Diagnostic> {
        if let Some(missing) = bodies
            .iter()
            .find(|want| !self.finals.iter().any(|b| &b.slot == *want))
        {
            return Err(error(
                "export.unknown-body",
                format!(
                    "{}.{} is not a body of the document",
                    missing.feature, missing.slot
                ),
            ));
        }
        let chosen: Vec<&FinalBody> = self
            .finals
            .iter()
            .filter(|b| bodies.is_empty() || bodies.contains(&b.slot))
            .collect();
        if chosen.is_empty() {
            return Err(error(
                "export.no-bodies",
                "the document has no body to export".into(),
            ));
        }
        let handles: Vec<_> = chosen.iter().map(|b| b.body).collect();
        let written = match format {
            ExportFormat::Step => self.kernel.export_step(&handles).map(String::into_bytes),
            ExportFormat::Stl => self.kernel.export_stl(&handles),
            ExportFormat::Obj => self.kernel.export_obj(&handles).map(String::into_bytes),
        };
        written.map_err(|e| host::kernel_diagnostic(&e))
    }
}
