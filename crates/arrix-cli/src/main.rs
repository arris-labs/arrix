//! `arrix`: the headless CLI, to open, evaluate, export, run and test
//! documents (docs/CONCURRENCY-WASM.md §Batch evaluation).

mod catalog;
mod config;
mod describe;
mod export;
mod load;
mod plugins;
mod schema;
mod source;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "arrix", version, about = "ArriX, headless")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Evaluate a document and print one JSON line for it. Exit status: 0
    /// when every feature evaluated, 1 when one failed, 2 when the document
    /// could not be read.
    Eval {
        /// A document directory (the unzipped form of an `.arrx`), or an
        /// `.arrx` file.
        doc: PathBuf,
        /// Evaluate as a build without this plugin would (repeatable): its
        /// features evaluate frozen.
        #[arg(long = "without-plugin", value_name = "ID")]
        without: Vec<String>,
        /// Write each failed kernel call as an Arris fixture recipe,
        /// `<DIR>/<code>-<hash>/fixture.json`. A failure whose recipe
        /// cannot be made (a frozen body among its operands) is reported on
        /// stderr and skipped.
        #[arg(long = "save-fixtures", value_name = "DIR")]
        save_fixtures: Option<PathBuf>,
    },
    /// Write a document's final bodies (or the `--body` ones) as a STEP,
    /// STL or OBJ file, in metres. Exit status: 0 when written from a clean
    /// document, 1 when written but some feature failed, 2 when the
    /// document could not be read or exported.
    Export {
        /// A document directory, or an `.arrx` file.
        doc: PathBuf,
        /// The file format.
        #[arg(long, value_enum)]
        format: Format,
        /// Only this body, `<feature>.<slot>` with the feature by id or
        /// name (repeatable); by default every final body.
        #[arg(long = "body", value_name = "FEATURE.SLOT")]
        bodies: Vec<String>,
        /// The file to write.
        #[arg(short, long, value_name = "FILE")]
        out: PathBuf,
        /// Evaluate as a build without this plugin would (repeatable).
        #[arg(long = "without-plugin", value_name = "ID")]
        without: Vec<String>,
    },
    /// Print everything there is to configure and its effective value: the
    /// commands and their keys, the settings, the theme roles, the feature
    /// types' parameters, the compiled-in plugins. Exit status: 0, or 2 when
    /// `--dir` is not a directory.
    Describe {
        /// One JSON object (`"schema": 1`) instead of text.
        #[arg(long)]
        json: bool,
        /// The config directory to read; by default `$ARRIX_CONFIG_DIR`, else
        /// the platform's.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Work with the config files.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    Step,
    Stl,
    Obj,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Validate the config files and print one JSON line per diagnostic,
    /// each with its file and line. Exit status: 0 when they are clean, 1
    /// when they are not, 2 when `--dir` is not a directory.
    Check {
        /// The config directory to read; by default `$ARRIX_CONFIG_DIR`, else
        /// the platform's.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Print JSON Schemas for the config files (`settings`, `keybindings`,
    /// `theme`; all three by default), for an editor with taplo to complete
    /// and check them. Exit status: 0, or 2 for an unknown kind.
    Schema {
        /// One file kind.
        kind: Option<String>,
    },
    /// Print the generated reference, as `docs/CONFIG-CATALOG.md` holds it.
    Catalog,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Eval {
            doc,
            without,
            save_fixtures,
        } => eval(&doc, &without, save_fixtures.as_deref()),
        Command::Export {
            doc,
            format,
            bodies,
            out,
            without,
        } => export::run(&export::Args {
            doc: &doc,
            format: match format {
                Format::Step => arrix_core::ExportFormat::Step,
                Format::Stl => arrix_core::ExportFormat::Stl,
                Format::Obj => arrix_core::ExportFormat::Obj,
            },
            bodies: &bodies,
            out: &out,
            without: &without,
        }),
        Command::Describe { json, dir } => describe::run(dir.as_deref(), json),
        Command::Config {
            command: ConfigCommand::Check { dir },
        } => config::check(dir.as_deref()),
        Command::Config {
            command: ConfigCommand::Schema { kind },
        } => schema::run(kind.as_deref()),
        Command::Config {
            command: ConfigCommand::Catalog,
        } => catalog::run(),
    }
}

fn eval(
    doc: &std::path::Path,
    without: &[String],
    save_fixtures: Option<&std::path::Path>,
) -> ExitCode {
    let name = doc.to_string_lossy();
    let registry = match load::registry_without(without) {
        Ok(registry) => registry,
        Err(err) => {
            eprintln!("arrix eval: {err}");
            return ExitCode::from(2);
        }
    };
    let line = load::Loaded::open(doc)
        .and_then(|source| arrix_doc::run(&name, &source, &registry).map_err(|e| e.to_string()));
    let run = match line {
        Ok(run) => run,
        Err(err) => {
            eprintln!("arrix eval: {name}: {err}");
            return ExitCode::from(2);
        }
    };
    if let Some(dir) = save_fixtures
        && let Err(err) = save_recipes(dir, &run.failures)
    {
        eprintln!("arrix eval: --save-fixtures {}: {err}", dir.display());
        return ExitCode::from(2);
    }
    let line = run.line;
    let json = serde_json::to_string(&line).expect("an eval line is plain data");
    if writeln!(std::io::stdout(), "{json}").is_err() {
        return ExitCode::from(2);
    }
    match line.status {
        arrix_doc::EvalStatus::Ok => ExitCode::SUCCESS,
        arrix_doc::EvalStatus::Failed => ExitCode::from(1),
    }
}

/// Writes each failure's recipe under `dir`, one directory per recipe; one
/// that has none is said on stderr and the rest are still written.
fn save_recipes(dir: &std::path::Path, failures: &[arrix_doc::FailedCall]) -> std::io::Result<()> {
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
