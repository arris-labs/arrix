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
    /// Where to write the JSON: its format, the adapter drawn on, and
    /// `debug_state()`.
    #[arg(long)]
    json: Option<std::path::PathBuf>,
    /// The view to draw from, once the document has settled.
    #[arg(long, value_enum)]
    view: Option<arrix_app::render::View>,
    /// The window, in pixels.
    #[arg(long, value_name = "WxH", default_value = "1440x900", value_parser = parse_size)]
    size: (u32, u32),
    /// Start the Tier 2 plugin in this directory (repeatable).
    #[arg(long = "plugin", value_name = "DIR")]
    plugins: Vec<std::path::PathBuf>,
    /// Leave this plugin out (repeatable): its features open frozen.
    #[arg(long = "without-plugin", value_name = "ID")]
    without: Vec<String>,
}

/// `WxH`, each between 1 and 8192 pixels.
#[cfg(not(target_arch = "wasm32"))]
fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("`{text}` is not WxH, each 1 to 8192 pixels");
    let (w, h) = text.split_once(['x', 'X']).ok_or_else(bad)?;
    let side = |s: &str| {
        s.parse::<u32>()
            .ok()
            .filter(|n| (1..=8192).contains(n))
            .ok_or_else(bad)
    };
    Ok((side(w)?, side(h)?))
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
    let options = RenderOptions {
        size: egui::vec2(args.size.0 as f32, args.size.1 as f32),
        plugins: args.plugins.clone(),
        without: args.without.clone(),
        view: args.view,
    };
    let done = render_document(&args.document, &options).and_then(|r| {
        if let Some(json) = &args.json {
            std::fs::write(json, r.json()?).map_err(|err| format!("{}: {err}", json.display()))?;
        }
        let [w, h] = r.size;
        image::RgbaImage::from_raw(w, h, r.pixels)
            .ok_or_else(|| "the frame has the wrong size".to_owned())?
            .save(&args.output)
            .map_err(|err| format!("{}: {err}", args.output.display()))
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
