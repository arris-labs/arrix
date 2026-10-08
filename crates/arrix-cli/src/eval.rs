//! `arrix eval`: documents in, one JSON line each out, in the order the
//! paths were given and a directory's documents sorted by path
//! (docs/CONCURRENCY-WASM.md §Batch evaluation).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_doc::{EvalStatus, Registry};

use crate::load::{Loaded, registry_without};

pub struct Args<'a> {
    pub paths: &'a [PathBuf],
    pub without: &'a [String],
    pub save_fixtures: Option<&'a Path>,
}

/// What became of one document: its line was printed (clean or with a
/// failed feature), or it could not be read.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Clean,
    Failed,
    Unreadable,
}

impl Status {
    fn code(self) -> ExitCode {
        match self {
            Status::Clean => ExitCode::SUCCESS,
            Status::Failed => ExitCode::from(1),
            Status::Unreadable => ExitCode::from(2),
        }
    }
}

pub fn run(args: &Args) -> ExitCode {
    let registry = match registry_without(args.without) {
        Ok(registry) => registry,
        Err(err) => {
            eprintln!("arrix eval: {err}");
            return ExitCode::from(2);
        }
    };
    let mut worst = Status::Clean;
    let mut documents = Vec::new();
    for path in args.paths {
        match discover(path) {
            Ok(found) => documents.extend(found),
            Err(err) => {
                eprintln!("arrix eval: {}: {err}", path.display());
                worst = Status::Unreadable;
            }
        }
    }
    for doc in &documents {
        worst = worst.max(one(doc, &registry, args.save_fixtures));
    }
    worst.code()
}

/// Evaluates one document and prints its line.
fn one(doc: &Path, registry: &Registry, save_fixtures: Option<&Path>) -> Status {
    let name = doc.to_string_lossy();
    let run = Loaded::open(doc)
        .and_then(|source| arrix_doc::run(&name, &source, registry).map_err(|e| e.to_string()));
    let run = match run {
        Ok(run) => run,
        Err(err) => {
            eprintln!("arrix eval: {name}: {err}");
            return Status::Unreadable;
        }
    };
    if let Some(dir) = save_fixtures
        && let Err(err) = save_recipes(dir, &run.failures)
    {
        eprintln!("arrix eval: --save-fixtures {}: {err}", dir.display());
        return Status::Unreadable;
    }
    let json = serde_json::to_string(&run.line).expect("an eval line is plain data");
    if writeln!(std::io::stdout(), "{json}").is_err() {
        return Status::Unreadable;
    }
    match run.line.status {
        EvalStatus::Ok => Status::Clean,
        EvalStatus::Failed => Status::Failed,
    }
}

/// The documents `path` names: itself when it is a file or holds a
/// `document.json`; otherwise the `.arrx` files and document directories
/// below it, sorted by path. A directory with none is an error.
fn discover(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() || path.join(arrix_doc::DOCUMENT_JSON).is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err("no such directory or file".into());
    }
    let mut found = Vec::new();
    search(path, &mut found)?;
    if found.is_empty() {
        return Err(
            "no document here: no document.json, and no .arrx or document directory below".into(),
        );
    }
    Ok(found)
}

fn search(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = std::fs::read_dir(dir)
        .and_then(|read| {
            read.map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            if entry.join(arrix_doc::DOCUMENT_JSON).is_file() {
                found.push(entry);
            } else {
                search(&entry, found)?;
            }
        } else if entry.extension().is_some_and(|e| e == "arrx") {
            found.push(entry);
        }
    }
    Ok(())
}

/// Writes each failure's recipe under `dir`, one directory per recipe; one
/// that has none is said on stderr and the rest are still written.
fn save_recipes(dir: &Path, failures: &[arrix_doc::FailedCall]) -> std::io::Result<()> {
    for failure in failures {
        match &failure.recipe {
            Ok(recipe) => {
                let at = dir.join(&recipe.name);
                std::fs::create_dir_all(&at)?;
                std::fs::write(at.join("fixture.json"), &recipe.json)?;
                eprintln!("arrix eval: saved {}", at.display());
            }
            Err(err) => eprintln!(
                "arrix eval: feature {} ({}) has no recipe: {err}",
                failure.feature, failure.code
            ),
        }
    }
    Ok(())
}
