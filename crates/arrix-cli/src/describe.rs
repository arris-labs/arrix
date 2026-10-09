//! `arrix describe`: everything there is to configure and what it is now,
//! for a user's agent that has not read the source (ADR-0016, constraint 3).
//! The JSON is the interface and carries its version; the text is for
//! people.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_config::{BUILTIN_THEMES, CONTEXTS, SETTINGS, THEME_ROLES};
use serde_json::{Value, json};

use crate::config::{Loaded, load};
use crate::plugins;
use arrix_plugin_host::Manifest;

/// A plugin as `describe` lists it; a Tier 2 one also says how it is run.
fn plugin_json(m: &Manifest) -> Value {
    let mut plugin = json!({
        "id": m.id.as_str(),
        "version": m.version.to_string(),
        "title": m.title,
        "tier": m.tier,
    });
    if m.tier == 2 {
        plugin["command"] = json!(m.command);
        plugin["timeout_seconds"] = json!(
            m.timeout
                .unwrap_or(arrix_plugin_host::DEFAULT_TIMEOUT)
                .as_secs_f64()
        );
        plugin["capabilities"] = json!({
            "kernel": m.capabilities.kernel,
            "document_read": m.capabilities.document_read,
            "document_write": m.capabilities.document_write,
            "files": format!("{:?}", m.capabilities.files).to_lowercase(),
            "network": m.capabilities.network,
        });
    }
    plugin
}

/// The version of the JSON's shape; a breaking change to it raises this.
const SCHEMA: u32 = 1;

pub fn run(dir: Option<&Path>, plugin_dirs: &[PathBuf], as_json: bool) -> ExitCode {
    let loaded = match load(dir, plugin_dirs) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("arrix describe: {err}");
            return ExitCode::from(2);
        }
    };
    let description = describe(&loaded);
    let text = if as_json {
        serde_json::to_string(&description).expect("a description is plain data")
    } else {
        render(&description)
    };
    use std::io::Write as _;
    if writeln!(std::io::stdout(), "{text}").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// The description of `loaded`, as the JSON `describe --json` prints.
pub fn describe(loaded: &Loaded) -> Value {
    let Loaded {
        dir,
        commands,
        types,
        tier2,
        config,
    } = loaded;
    let plugins: Vec<_> = plugins::manifests()
        .iter()
        .chain(tier2)
        .map(plugin_json)
        .collect();
    let command_list: Vec<_> = commands
        .commands
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "title": c.title,
                "ribbon": c.ribbon,
                "keys": config.keymap.keys_of(&c.id),
                "default_keys": c.keys,
                "when": c.when,
                "needs_document": c.needs_document,
                "listed": c.listed,
            })
        })
        .collect();
    let settings: Vec<_> = SETTINGS
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "kind": s.kind,
                "default": s.default.value(),
                "value": config.settings.get(s.id),
                "doc": s.doc,
            })
        })
        .collect();
    let roles: Vec<_> = THEME_ROLES
        .iter()
        .map(|(role, doc)| json!({"role": role, "doc": doc, "color": config.theme.color(role)}))
        .collect();
    let contexts: Vec<_> = CONTEXTS
        .iter()
        .map(|(name, doc)| json!({"name": name, "doc": doc}))
        .collect();
    let feature_types: Vec<_> = types
        .iter()
        .map(|ty| {
            let effective = config.defaults.effective_params(ty);
            let params: Vec<_> = ty
                .params
                .iter()
                .zip(effective)
                .map(|(p, (_, now))| {
                    json!({
                        "name": p.name,
                        "title": p.title,
                        "kind": p.kind,
                        "default": p.default,
                        "effective": now,
                    })
                })
                .collect();
            json!({"id": ty.id, "title": ty.title, "params": params})
        })
        .collect();
    json!({
        "schema": SCHEMA,
        "config_dir": dir.as_ref().map(|d| d.display().to_string()),
        "plugins": plugins,
        "tabs": commands.tabs,
        "commands": command_list,
        "bindings": config.keymap.bindings(),
        "keymap_contexts": contexts,
        "settings": settings,
        "theme": {
            "builtin": BUILTIN_THEMES,
            "effective": config.theme.name,
            "base": config.theme.base,
            "roles": roles,
        },
        "feature_types": feature_types,
        "diagnostics": config.diagnostics,
    })
}

/// The description for a reader: one section per kind of thing.
fn render(d: &Value) -> String {
    let mut out = String::new();
    let dir = d["config_dir"]
        .as_str()
        .unwrap_or("none: the built-in configuration");
    let _ = writeln!(out, "config directory: {dir}");
    section(&mut out, "plugins", &d["plugins"], |p| {
        format!(
            "{} {} (tier {}): {}",
            p["id"], p["version"], p["tier"], p["title"]
        )
    });
    section(&mut out, "commands", &d["commands"], |c| {
        let keys: Vec<_> = c["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        format!(
            "{:<32} {:<26} {}",
            text(&c["id"]),
            keys.join(", "),
            text(&c["title"])
        )
    });
    section(&mut out, "settings", &d["settings"], |s| {
        format!(
            "{:<30} = {}  (default {})  {}",
            text(&s["id"]),
            s["value"],
            s["default"],
            text(&s["doc"])
        )
    });
    section(&mut out, "theme roles", &d["theme"]["roles"], |r| {
        format!(
            "{:<14} {}  {}",
            text(&r["role"]),
            text(&r["color"]),
            text(&r["doc"])
        )
    });
    section(&mut out, "keymap contexts", &d["keymap_contexts"], |c| {
        format!("{:<8} {}", text(&c["name"]), text(&c["doc"]))
    });
    for ty in d["feature_types"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "\nfeature type {} ({})",
            text(&ty["id"]),
            text(&ty["title"])
        );
        for p in ty["params"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {:<16} {} = {}  (default {})",
                text(&p["name"]),
                text(&p["kind"]),
                text(&p["effective"]),
                text(&p["default"])
            );
        }
    }
    section(&mut out, "diagnostics", &d["diagnostics"], |x| {
        x.to_string()
    });
    out.trim_end().to_owned()
}

fn section(out: &mut String, title: &str, items: &Value, line: impl Fn(&Value) -> String) {
    let items = items.as_array().map_or(&[][..], Vec::as_slice);
    if items.is_empty() && title == "diagnostics" {
        return;
    }
    let _ = writeln!(out, "\n{title}");
    for item in items {
        let _ = writeln!(out, "  {}", line(item));
    }
}

/// A JSON string without its quotes; anything else as JSON.
fn text(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
}
