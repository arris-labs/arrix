//! `arrix`: the headless CLI, to open, evaluate, export, run and test
//! documents (docs/CONCURRENCY-WASM.md §Batch evaluation).

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
        /// A document directory (the unzipped form of an `.arrx`).
        doc: PathBuf,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Eval { doc } => eval(&doc),
    }
}

fn eval(doc: &std::path::Path) -> ExitCode {
    let name = doc.to_string_lossy();
    if !doc.is_dir() {
        eprintln!("arrix eval: {name}: no such directory");
        return ExitCode::from(2);
    }
    let registry = arrix_doc::Registry::with_core_types();
    let line = match arrix_doc::eval(&name, &DirSource::new(doc), &registry) {
        Ok(line) => line,
        Err(err) => {
            eprintln!("arrix eval: {name}: {err}");
            return ExitCode::from(2);
        }
    };
    let json = serde_json::to_string(&line).expect("an eval line is plain data");
    if writeln!(std::io::stdout(), "{json}").is_err() {
        return ExitCode::from(2);
    }
    match line.status {
        arrix_doc::EvalStatus::Ok => ExitCode::SUCCESS,
        arrix_doc::EvalStatus::Failed => ExitCode::from(1),
    }
}
