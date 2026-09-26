//! `arrix`: the headless CLI, to open, evaluate, export, run and test
//! documents (docs/CONCURRENCY-WASM.md §Batch evaluation).

use clap::Parser;

#[derive(Parser)]
#[command(name = "arrix", version, about = "ArriX, headless")]
struct Cli {}

fn main() {
    let Cli {} = Cli::parse();
}
