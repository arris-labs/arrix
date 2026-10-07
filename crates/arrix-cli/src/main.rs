//! `arrix`: the headless CLI, to open, evaluate, export, run and test
//! documents (docs/CONCURRENCY-WASM.md §Batch evaluation).

mod config;
mod describe;
mod plugins;
mod source;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use source::DirSource;

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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Eval {
            doc,
            without,
            save_fixtures,
        } => eval(&doc, &without, save_fixtures.as_deref()),
        Command::Describe { json, dir } => describe::run(dir.as_deref(), json),
        Command::Config {
            command: ConfigCommand::Check { dir },
        } => config::check(dir.as_deref()),
    }
}

fn eval(
    doc: &std::path::Path,
    without: &[String],
    save_fixtures: Option<&std::path::Path>,
) -> ExitCode {
    let name = doc.to_string_lossy();
    let mut registry = plugins::registry();
    for id in without {
        let removed = arrix_core::PluginId::new(id.as_str())
            .map(|plugin| registry.remove_plugin(&plugin))
            .unwrap_or(false);
        if !removed {
            eprintln!("arrix eval: --without-plugin {id}: no such plugin in this build");
            return ExitCode::from(2);
        }
    }
    let line = if doc.is_file() {
        std::fs::read(doc)
            .map_err(|e| e.to_string())
            .and_then(|bytes| arrix_doc::from_zip(&bytes).map_err(|e| e.to_string()))
            .and_then(|files| arrix_doc::run(&name, &files, &registry).map_err(|e| e.to_string()))
    } else if doc.is_dir() {
        arrix_doc::run(&name, &DirSource::new(doc), &registry).map_err(|e| e.to_string())
    } else {
        eprintln!("arrix eval: {name}: no such directory or file");
        return ExitCode::from(2);
    };
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
