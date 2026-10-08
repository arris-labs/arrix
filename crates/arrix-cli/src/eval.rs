//! `arrix eval`: documents in, one JSON line each out, in the order the
//! paths were given and a directory's documents sorted by path
//! (docs/CONCURRENCY-WASM.md §Batch evaluation).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use web_time::Instant;

use arrix_doc::{EvalStatus, Registry, Run, Sweep};

use crate::load::{Loaded, registry_without};

pub struct Args<'a> {
    pub paths: &'a [PathBuf],
    pub without: &'a [String],
    pub save_fixtures: Option<&'a Path>,
    /// The `--sweep` flag as typed.
    pub sweep: Option<&'a str>,
    /// `--summary`: a last line with the failure histogram.
    pub summary: bool,
    /// `--jobs`: documents evaluated at once.
    pub jobs: usize,
    /// `--budget`: the steps each kernel call may take.
    pub budget: Option<u64>,
    /// `--timings`: each line carries the wall time of its document.
    pub timings: bool,
}

/// One row of the failure histogram: a category, how often it was hit, the
/// documents that hit it and the plugins whose feature types did.
#[derive(Default)]
struct Row {
    category: String,
    count: usize,
    docs: BTreeSet<String>,
    plugins: BTreeSet<String>,
}

/// The histogram, by category. A failed feature counts under its category;
/// a failed parameter under its diagnostic's code, with no plugin.
#[derive(Default)]
struct Summary(BTreeMap<String, Row>);

impl Summary {
    fn add(&mut self, line: &arrix_doc::EvalLine) {
        let mut hit = |category: &str, plugin: Option<&str>| {
            let row = self.0.entry(category.to_string()).or_default();
            row.category = category.to_string();
            row.count += 1;
            row.docs.insert(line.doc.clone());
            row.plugins.extend(plugin.map(str::to_string));
        };
        for param in &line.params {
            if let Some(diagnostic) = &param.diagnostic {
                hit(&diagnostic.code.to_string(), None);
            }
        }
        for feature in &line.features {
            if let Some(category) = &feature.category {
                let plugin = feature.feature_type.split('.').next();
                hit(category, plugin);
            }
        }
    }

    fn print(&self) -> std::io::Result<()> {
        let rows: Vec<_> = self
            .0
            .values()
            .map(|r| {
                serde_json::json!({
                    "category": r.category,
                    "count": r.count,
                    "docs": r.docs,
                    "plugins": r.plugins,
                })
            })
            .collect();
        let json = serde_json::json!({ "summary": rows });
        writeln!(std::io::stdout(), "{json}")
    }
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
    let sweep = match args.sweep.map(str::parse::<Sweep>).transpose() {
        Ok(sweep) => sweep,
        Err(err) => {
            eprintln!("arrix eval: {err}");
            return ExitCode::from(2);
        }
    };
    let mut worst = Status::Clean;
    let mut summary = Summary::default();
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
    let jobs = args.jobs.clamp(1, documents.len().max(1));
    let mut emit = |index: usize, done: Done| {
        worst = worst.max(report(&documents[index], done, args, &mut summary));
    };
    if jobs == 1 {
        for (index, doc) in documents.iter().enumerate() {
            emit(index, evaluate(doc, &registry, sweep.as_ref(), args.budget));
        }
    } else {
        in_order(
            &documents,
            jobs,
            &registry,
            sweep.as_ref(),
            args.budget,
            &mut emit,
        );
    }
    if args.summary && summary.print().is_err() {
        worst = Status::Unreadable;
    }
    worst.code()
}

/// One document evaluated: its runs (the sweep's points, or the one), or
/// why it could not be read, and the wall time it took.
struct Done {
    runs: Result<Vec<Run>, String>,
    wall_ms: f64,
}

/// Evaluates one document, or each point of the sweep, on its own
/// evaluator and model.
fn evaluate(doc: &Path, registry: &Registry, sweep: Option<&Sweep>, budget: Option<u64>) -> Done {
    let name = doc.to_string_lossy();
    let started = Instant::now();
    let runs = Loaded::open(doc).and_then(|source| match sweep {
        Some(sweep) => {
            arrix_doc::sweep(&name, &source, registry, sweep, budget).map_err(|e| e.to_string())
        }
        None => arrix_doc::run(&name, &source, registry, budget)
            .map(|run| vec![run])
            .map_err(|e| e.to_string()),
    });
    Done {
        runs,
        wall_ms: started.elapsed().as_secs_f64() * 1e3,
    }
}

/// Evaluates the documents on `jobs` threads, one at a time each, and hands
/// the results to `emit` in the documents' order whichever finishes first:
/// the output is `--jobs 1`'s, byte for byte (bar `--timings`).
fn in_order(
    documents: &[PathBuf],
    jobs: usize,
    registry: &Registry,
    sweep: Option<&Sweep>,
    budget: Option<u64>,
    emit: &mut dyn FnMut(usize, Done),
) {
    let next = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let (tx, next) = (tx.clone(), &next);
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(doc) = documents.get(index) else {
                        break;
                    };
                    if tx
                        .send((index, evaluate(doc, registry, sweep, budget)))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut waiting = BTreeMap::new();
        let mut want = 0;
        for (index, done) in rx {
            waiting.insert(index, done);
            while let Some(done) = waiting.remove(&want) {
                emit(want, done);
                want += 1;
            }
        }
    });
}

/// Prints a document's lines, or says why it has none; the worst of them is
/// its status.
fn report(doc: &Path, done: Done, args: &Args, summary: &mut Summary) -> Status {
    let runs = match done.runs {
        Ok(runs) => runs,
        Err(err) => {
            eprintln!("arrix eval: {}: {err}", doc.to_string_lossy());
            return Status::Unreadable;
        }
    };
    let timings = args.timings.then_some((done.wall_ms, runs.len()));
    let mut worst = Status::Clean;
    for run in &runs {
        summary.add(&run.line);
        worst = worst.max(print(run, args.save_fixtures, timings));
    }
    worst
}

/// Prints the run's line; with `timings` (the wall time of the whole
/// document, sweep points included, and its line count) as a `timings`
/// field, the one thing in a line that is not deterministic.
fn print(run: &Run, save_fixtures: Option<&Path>, timings: Option<(f64, usize)>) -> Status {
    if let Some(dir) = save_fixtures
        && let Err(err) = save_recipes(dir, &run.failures)
    {
        eprintln!("arrix eval: --save-fixtures {}: {err}", dir.display());
        return Status::Unreadable;
    }
    let mut json = serde_json::to_string(&run.line).expect("an eval line is plain data");
    if let Some((wall_ms, lines)) = timings {
        // Appended to the text, so the fields keep the line's own order.
        json.pop();
        json.push_str(&format!(
            ",\"timings\":{{\"wall_ms\":{wall_ms},\"lines\":{lines}}}}}"
        ));
    }
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
