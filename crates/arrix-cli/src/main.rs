//! `arrix`: the headless CLI, to open, evaluate, export, run and test
//! documents (docs/CONCURRENCY-WASM.md §Batch evaluation).

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
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Eval { doc, without } => eval(&doc, &without),
    }
}

fn eval(doc: &std::path::Path, without: &[String]) -> ExitCode {
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
            .and_then(|files| arrix_doc::eval(&name, &files, &registry).map_err(|e| e.to_string()))
    } else if doc.is_dir() {
        arrix_doc::eval(&name, &DirSource::new(doc), &registry).map_err(|e| e.to_string())
    } else {
        eprintln!("arrix eval: {name}: no such directory or file");
        return ExitCode::from(2);
    };
    let line = match line {
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
