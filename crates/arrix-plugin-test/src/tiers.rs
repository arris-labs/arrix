//! The kit for an author's `cargo test`: a plugin compiled in (Tier 0) or
//! built as a component (Tier 1), run against its own `[test]` table.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrix_doc::Registry;
use arrix_plugin_api::{Commands, Exporter, Feature};
use arrix_plugin_host::{
    Manifest, manifest_in, register_dir, register_tier0, register_tier0_commands,
    register_tier0_exporters,
};

use crate::kit::{Check, Env, Kit};

/// What a Tier 0 plugin contributes: the trait object its manifest's
/// `contributes` table is checked against.
#[derive(Clone)]
pub enum Tier0 {
    Feature(Arc<dyn Feature>),
    Exporter(Arc<dyn Exporter>),
    Commands(Arc<dyn Commands>),
}

/// The checks of one plugin, in the order the kit runs them.
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    /// Panics naming every failed check; the test an author writes is
    /// `run_tier0(..).unwrap().assert_passed()`.
    pub fn assert_passed(&self) {
        let failed: Vec<String> = self
            .checks
            .iter()
            .filter_map(|c| {
                let detail = c.result.as_ref().err()?;
                Some(format!("{} on {}: {detail}", c.check, c.doc))
            })
            .collect();
        assert!(failed.is_empty(), "{}", failed.join("\n"));
        assert!(!self.checks.is_empty(), "the kit ran no check");
    }
}

fn register(registry: &mut Registry, manifest: &str, plugin: &Tier0) -> Result<(), String> {
    match plugin {
        Tier0::Feature(f) => register_tier0(registry, manifest, f.clone()).map(drop),
        Tier0::Exporter(e) => register_tier0_exporters(registry, manifest, e.clone()).map(drop),
        Tier0::Commands(c) => register_tier0_commands(registry, manifest, c.clone()).map(drop),
    }
    .map_err(|e| e.to_string())
}

/// Takes the plugins of `without` out of `registry`, as `--without-plugin`.
fn leave_out(registry: &mut Registry, without: &[String]) -> Result<(), String> {
    for id in without {
        let removed = arrix_core::PluginId::new(id.as_str())
            .map(|plugin| registry.remove_plugin(&plugin))
            .unwrap_or(false);
        if !removed {
            return Err(format!("without {id}: no such plugin in this run"));
        }
    }
    Ok(())
}

/// Loads each plugin directory of `with`, a directory taking the place of a
/// plugin of its id already in `registry`.
fn load_dirs(registry: &mut Registry, with: &[PathBuf]) -> Result<(), String> {
    for dir in with {
        let named = |e: &dyn std::fmt::Display| format!("plugin {}: {e}", dir.display());
        let manifest = manifest_in(dir).map_err(|e| named(&e))?;
        registry.remove_plugin(&manifest.id);
        register_dir(registry, dir).map_err(|e| named(&e))?;
    }
    Ok(())
}

/// The kit on a plugin compiled in: `dir` is the plugin project (its
/// fixtures are relative to it), `manifest` its `arrix-plugin.toml` as
/// `include_str!` reads it. The inner error lists the setup diagnostics
/// (a fixture missing, a manifest that does not parse).
pub fn run_tier0(dir: &Path, manifest: &str, plugin: Tier0) -> Result<Report, Vec<String>> {
    let parsed = Manifest::parse(manifest).map_err(|e| vec![e.to_string()])?;
    let text = manifest.to_owned();
    let env = Env {
        registry: Box::new(move |with, without| {
            let mut registry = Registry::with_core_types();
            register(&mut registry, &text, &plugin)?;
            load_dirs(&mut registry, with)?;
            leave_out(&mut registry, without)?;
            Ok(registry)
        }),
        compiled: vec![parsed.id],
    };
    Kit::open(env, dir, &[]).map(|kit| Report {
        checks: kit.checks(),
    })
}

/// The kit on a plugin built as a component, `dir` holding its manifest and
/// the `.wasm` it names (the build is the author's: `cargo build --release
/// --target wasm32-unknown-unknown --features guest`).
pub fn run_tier1(dir: &Path) -> Result<Report, Vec<String>> {
    let env = Env {
        registry: Box::new(|with, without| {
            let mut registry = Registry::with_core_types();
            load_dirs(&mut registry, with)?;
            leave_out(&mut registry, without)?;
            Ok(registry)
        }),
        compiled: Vec::new(),
    };
    Kit::open(env, dir, &[]).map(|kit| Report {
        checks: kit.checks(),
    })
}
