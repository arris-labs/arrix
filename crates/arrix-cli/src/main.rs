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
mod serve;
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
        #[arg(required_unless_present = "schema", value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Print the line's JSON Schema (draft 2020-12) and exit. A line
        /// carries `"format"` (ADR-0026).
        #[arg(long)]
        schema: bool,
        /// Start the Tier 2 plugin in this directory (repeatable): one
        /// holding an `arrix-plugin.toml`, whose command is run there.
        #[arg(long = "plugin", value_name = "DIR")]
        plugins: Vec<PathBuf>,
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
        /// End with one JSON line, `{"summary":[{"category","count","docs",
        /// "plugins"}]}`: the failures by category, in category order.
        #[arg(long)]
        summary: bool,
        /// Add `"timings":{"wall_ms","lines"}` to each line: the wall time
        /// of the line's whole document (all its sweep points). The lines
        /// are no longer deterministic.
        #[arg(long)]
        timings: bool,
        /// Evaluate this many documents at once, each on its own evaluator
        /// and model (never in parallel inside one). The output and the
        /// exit status are `--jobs 1`'s.
        #[arg(long, value_name = "N", default_value_t = 1)]
        jobs: usize,
        /// Stop any kernel call at this many steps (Arris's step budget): a
        /// feature it stops fails as `kernel.interrupted.budget`. The same
        /// on every machine and run, unlike a timeout.
        #[arg(long, value_name = "STEPS")]
        budget: Option<u64>,
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
        /// Start the Tier 2 plugin in this directory (repeatable): one
        /// holding an `arrix-plugin.toml`, whose command is run there.
        #[arg(long = "plugin", value_name = "DIR")]
        plugins: Vec<PathBuf>,
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
        /// Start the Tier 2 plugin in this directory (repeatable) and list
        /// it, and its feature types, with the compiled-in ones.
        #[arg(long = "plugin", value_name = "DIR")]
        plugins: Vec<PathBuf>,
    },
    /// The session protocol (ADR-0023). `--stdio` serves one client one
    /// document as JSON-RPC, a line per message: `initialize`, then the
    /// `session.*` methods, with every event a `session.event`
    /// notification. `--schema` prints its JSON Schemas: `{"protocol",
    /// "request", "outcome", "rejected", "event", "methods"}`, each draft
    /// 2020-12, the event's as `serve` writes it (bulk as base64). Exit
    /// status: 0 at the end of the stream, 2 when the document could not be
    /// opened.
    Serve {
        /// Print the protocol's schemas and exit.
        #[arg(long)]
        schema: bool,
        /// Serve the session on stdin and stdout.
        #[arg(long)]
        stdio: bool,
        /// A document directory or an `.arrx` to open; an empty document
        /// by default.
        doc: Option<PathBuf>,
        /// A Tier 2 plugin directory to start with the session
        /// (repeatable); `session.load_plugin` adds more later.
        #[arg(long = "plugin")]
        plugins: Vec<PathBuf>,
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
        Command::Eval { schema: true, .. } => eval::print_schema(),
        Command::Eval {
            paths,
            plugins,
            without,
            save_fixtures,
            sweep,
            summary,
            timings,
            jobs,
            budget,
            schema: _,
        } => eval::run(&eval::Args {
            paths: &paths,
            plugins: &plugins,
            without: &without,
            save_fixtures: save_fixtures.as_deref(),
            sweep: sweep.as_deref(),
            summary,
            timings,
            jobs,
            budget,
        }),
        Command::Export {
            doc,
            format,
            bodies,
            out,
            plugins,
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
            plugins: &plugins,
            without: &without,
        }),
        Command::Describe { json, dir, plugins } => describe::run(dir.as_deref(), &plugins, json),
        Command::Serve {
            schema,
            stdio,
            doc,
            plugins,
        } => serve::run(&serve::Args {
            schema,
            stdio,
            doc: doc.as_deref(),
            plugins: &plugins,
        }),
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
