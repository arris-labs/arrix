//! `arrix export`: a document's final bodies as STEP, STL or OBJ, through
//! `arrix_doc::export` (the path the app's export takes too).

use std::path::Path;
use std::process::ExitCode;

use arrix_core::{ExportFormat, FeatureId, SlotName};
use arrix_doc::{BodySlot, Document};

use crate::load::{Loaded, registry_without};

pub struct Args<'a> {
    pub doc: &'a Path,
    pub format: ExportFormat,
    pub bodies: &'a [String],
    pub out: &'a Path,
    pub without: &'a [String],
}

pub fn run(args: &Args) -> ExitCode {
    match export(args) {
        Ok(status) => status,
        Err(err) => {
            eprintln!("arrix export: {err}");
            ExitCode::from(2)
        }
    }
}

fn export(args: &Args) -> Result<ExitCode, String> {
    let name = args.doc.to_string_lossy();
    let registry = registry_without(args.without)?;
    let source = Loaded::open(args.doc).map_err(|e| format!("{name}: {e}"))?;
    let document = arrix_doc::open(&source).map_err(|e| format!("{name}: {e}"))?;
    let bodies = args
        .bodies
        .iter()
        .map(|spec| body_slot(&document, spec))
        .collect::<Result<Vec<_>, _>>()?;
    let exported = arrix_doc::export(&name, &source, &registry, &bodies, args.format)
        .map_err(|e| format!("{name}: {e}"))?;
    std::fs::write(args.out, &exported.bytes)
        .map_err(|e| format!("{}: {e}", args.out.display()))?;
    // A feature that failed leaves its bodies out of the file: say so.
    Ok(match exported.line.status {
        arrix_doc::EvalStatus::Ok => ExitCode::SUCCESS,
        arrix_doc::EvalStatus::Failed => {
            eprintln!(
                "arrix export: {name}: some features failed; the file holds the bodies that evaluated"
            );
            ExitCode::from(1)
        }
    })
}

/// `<feature>.<slot>`, the feature by its id or its name in any part.
fn body_slot(document: &Document, spec: &str) -> Result<BodySlot, String> {
    let (feature, slot) = spec
        .rsplit_once('.')
        .ok_or_else(|| format!("--body {spec}: expected <feature>.<slot>"))?;
    let slot = SlotName::new(slot).map_err(|e| format!("--body {spec}: {e}"))?;
    let by_id = feature.parse::<FeatureId>().ok().filter(|id| {
        document
            .parts()
            .values()
            .any(|part| part.features.contains_key(id))
    });
    let mut named = document
        .parts()
        .values()
        .flat_map(|part| part.features.values())
        .filter(|f| f.name == feature)
        .map(|f| f.id);
    let id = match (by_id, named.next(), named.next()) {
        (Some(id), _, _) | (None, Some(id), None) => id,
        (None, Some(_), Some(_)) => {
            return Err(format!(
                "--body {spec}: {feature} names several features; use its id"
            ));
        }
        (None, None, _) => return Err(format!("--body {spec}: no such feature")),
    };
    Ok(BodySlot { feature: id, slot })
}
