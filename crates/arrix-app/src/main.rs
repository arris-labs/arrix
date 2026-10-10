//! `arrix-app [<document> ...]`: each path is a document directory or an
//! `.arrx`, opened as a tab. `arrix-app render <document> -o <png>` draws
//! one headless instead (ADR-0025).
#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    use clap::Parser as _;
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Render(args)) => render(&args),
        None => match run(cli.documents) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("arrix-app: {err}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Parser)]
#[command(name = "arrix-app", args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Documents to open, each a directory or an `.arrx`.
    documents: Vec<std::path::PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Subcommand)]
enum Command {
    /// Renders a document headless to a PNG, and what the app drew as JSON.
    Render(RenderArgs),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Args)]
struct RenderArgs {
    /// The document: a directory or an `.arrx`.
    document: std::path::PathBuf,
    /// The PNG to write.
    #[arg(short, long)]
    output: std::path::PathBuf,
    /// Where to write `debug_state()`'s JSON, with the adapter drawn on.
    #[arg(long)]
    json: Option<std::path::PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
fn run(paths: Vec<std::path::PathBuf>) -> eframe::Result<()> {
    eframe::run_native(
        "ArriX",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            let mut app = arrix_app::ArrixApp::native(cc);
            for path in &paths {
                if let Err(err) = app.open_path(path) {
                    eprintln!("arrix-app: {err}");
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn render(args: &RenderArgs) -> std::process::ExitCode {
    use arrix_app::render::{RenderOptions, render_document};
    let done = render_document(&args.document, &RenderOptions::default()).and_then(|r| {
        let [w, h] = r.size;
        image::RgbaImage::from_raw(w, h, r.pixels)
            .ok_or_else(|| "the frame has the wrong size".to_owned())?
            .save(&args.output)
            .map_err(|err| format!("{}: {err}", args.output.display()))?;
        if let Some(json) = &args.json {
            let state: serde_json::Value =
                serde_json::from_str(&r.debug_state).map_err(|err| err.to_string())?;
            let out = serde_json::json!({ "adapter": r.adapter, "debug_state": state });
            let text = serde_json::to_string_pretty(&out).map_err(|err| err.to_string())?;
            std::fs::write(json, format!("{text}\n"))
                .map_err(|err| format!("{}: {err}", json.display()))?;
        }
        Ok(())
    });
    match done {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("arrix-app render: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

// The browser build loads the library from a host page (C4); the binary
// still has to compile for the workspace-wide wasm build.
#[cfg(target_arch = "wasm32")]
fn main() {}
