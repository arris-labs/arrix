//! `arrix import`: a STEP file's product tree as a new document directory
//! (ADR-0032), through `arrix_doc::import_products`, the path the app's
//! `Import…` takes too.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_core::IdMinter;
use arrix_doc::{Document, Imported};
use arrix_kernel::Kernel;

pub struct Args<'a> {
    pub step: &'a Path,
    pub out: &'a Path,
}

pub fn run(args: &Args) -> ExitCode {
    match import(args) {
        Ok(status) => status,
        Err(err) => {
            eprintln!("arrix import: {err}");
            ExitCode::from(2)
        }
    }
}

fn import(args: &Args) -> Result<ExitCode, String> {
    let name = args.step.display();
    let bytes = std::fs::read(args.step).map_err(|e| format!("{name}: {e}"))?;
    if args.out.exists() && std::fs::read_dir(args.out).is_ok_and(|mut d| d.next().is_some()) {
        return Err(format!(
            "{}: not empty; import into a new directory",
            args.out.display()
        ));
    }
    let structure = Kernel::new()
        .read_step_structure(&bytes)
        .map_err(|e| format!("{name}: {} ({})", e, e.code()))?;
    let file_name = args
        .step
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("import.step");
    // The same file gives the same ids, so an import is reproducible.
    let mut ids = IdMinter::new(seed(&bytes));
    let document = Document::default();
    let Imported {
        command,
        roots,
        notes,
    } = arrix_doc::import_products(&document, file_name, bytes, &structure, &mut ids)
        .map_err(|e| format!("{name}: {e} ({})", e.code()))?;
    let document = arrix_doc::apply(&document, &command)
        .map_err(|e| format!("{name}: {e}"))?
        .document;
    write(&document, args.out)?;
    for note in &notes {
        eprintln!("arrix import: {name}: {} ({})", note.message, note.code);
    }
    let line = serde_json::json!({
        "imported": {
            "doc": args.out.to_string_lossy(),
            "parts": document.parts().len(),
            "assemblies": document.assemblies().len(),
            "materials": document.materials().len(),
            "roots": roots.len(),
            "notes": notes.len(),
        }
    });
    println!("{line}");
    Ok(ExitCode::SUCCESS)
}

/// FNV-1a over the file: any fixed function of the bytes would do.
fn seed(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The document's directory form under `out`.
fn write(document: &Document, out: &Path) -> Result<(), String> {
    for (path, bytes) in arrix_doc::save(document).0 {
        let at: PathBuf = out.join(&path);
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&at, bytes).map_err(|e| format!("{}: {e}", at.display()))?;
    }
    Ok(())
}
