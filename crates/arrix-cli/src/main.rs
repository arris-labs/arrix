//! `arrix`: the headless CLI, to open, evaluate, export, run and test
//! documents (docs/CONCURRENCY-WASM.md §Batch evaluation).

mod catalog;
mod config;
mod describe;
mod eval;
mod export;
mod load;
mod plugins;
mod schema;
mod source;

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
    /// Evaluate documents and print one JSON line for each, in the order
    /// given. Exit status: 0 when every feature of every document
    /// evaluated, 1 when one failed, 2 when a document could not be read
    /// (the others still print).
    Eval {
        /// Document directories (the unzipped form of an `.arrx`) and
        /// `.arrx` files. A directory with no `document.json` is searched:
        /// its `.arrx` files and document directories are evaluated, sorted
        /// by path.
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
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
        /// Evaluate each document at N values of one number, a line per
        /// point: `<param>=<from>..<to>:<n>` for a document parameter,
        /// `<feature>.<field>=<from>..<to>:<n>` for a feature's numeric
        /// field (the feature by name or id). The ends are constant
        /// expressions (`10 mm`).
        #[arg(long, value_name = "TARGET=FROM..TO:N")]
        sweep: Option<String>,
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
            paths,
            without,
            save_fixtures,
            sweep,
        } => eval::run(&eval::Args {
            paths: &paths,
            without: &without,
            save_fixtures: save_fixtures.as_deref(),
            sweep: sweep.as_deref(),
        }),
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
