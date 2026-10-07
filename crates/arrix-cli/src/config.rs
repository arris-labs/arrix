//! `arrix config check` and what `arrix describe` shares with it: finding
//! the config directory, and loading what it says over the built-in layer
//! with the compiled-in plugins' feature types (ADR-0015).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_config::{CommandRegistry, Config, Os, config_dir};
use arrix_plugin_api::FeatureTypeSpec;

use crate::plugins;
use crate::source::DirSource;

/// Everything `describe` and `check` work on.
pub struct Loaded {
    /// The directory read, or `None` for the built-in configuration alone.
    pub dir: Option<PathBuf>,
    pub commands: CommandRegistry,
    pub types: Vec<FeatureTypeSpec>,
    pub config: Config,
}

/// `--dir`, else `$ARRIX_CONFIG_DIR`, else the platform's. An explicit
/// directory that isn't one is an error; a default that isn't there is just
/// the built-in configuration, since most users have no files.
pub fn load(dir: Option<&Path>) -> Result<Loaded, String> {
    let dir = match dir {
        Some(dir) if !dir.is_dir() => {
            return Err(format!("{}: no such directory", dir.display()));
        }
        Some(dir) => Some(dir.to_path_buf()),
        None => config_dir(Os::current(), |name| std::env::var(name).ok()).filter(|d| d.is_dir()),
    };
    let registry = plugins::registry();
    let types = plugins::feature_types(&registry);
    let commands = CommandRegistry::new(&types);
    let config = match &dir {
        Some(dir) => Config::load(
            &DirSource::new(dir),
            &commands,
            &types,
            &arrix_doc::expr::check_constant,
        ),
        None => Config::load(
            &arrix_config::MemorySource::default(),
            &commands,
            &types,
            &arrix_doc::expr::check_constant,
        ),
    };
    Ok(Loaded {
        dir,
        commands,
        types,
        config,
    })
}

/// Prints one JSON line per diagnostic. Exit status: 0 when the files are
/// clean, 1 when they are not, 2 when the directory can't be read.
pub fn check(dir: Option<&Path>) -> ExitCode {
    let loaded = match load(dir) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("arrix config check: {err}");
            return ExitCode::from(2);
        }
    };
    let mut out = std::io::stdout().lock();
    for diagnostic in &loaded.config.diagnostics {
        let json = serde_json::to_string(diagnostic).expect("a diagnostic is plain data");
        if writeln!(out, "{json}").is_err() {
            return ExitCode::from(2);
        }
    }
    if loaded.config.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
