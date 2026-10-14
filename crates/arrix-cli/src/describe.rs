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

/// A plugin as `describe` lists it; a Tier 1 one says its component and fuel, a Tier 2 one how it is run; both their capabilities.
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
    }
    if let Some(tier1) = &m.tier1 {
        plugin["component"] = json!(tier1.component);
        plugin["fuel"] = json!(tier1.fuel.unwrap_or(arrix_plugin_host::DEFAULT_FUEL));
    }
    if m.tier != 0 {
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
        exporters,
        plugin_commands,
        tier2,
        config,
    } = loaded;
    // A `--plugin` directory takes the place of a compiled-in plugin of its id.
    let plugins: Vec<_> = plugins::manifests()
        .iter()
        .filter(|m| !tier2.iter().any(|d| d.id == m.id))
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
    // The core's formats are exporters too (ADR-0037 §10), written natively
    // from the final bodies; a plugin's follow, in id order.
    let core = arrix_core::ExportFormat::ALL.iter().map(|f| {
        json!({
            "id": format!("core.{}", f.extension()),
            "title": f.extension().to_uppercase(),
            "extension": f.extension(),
            "subject": "document",
            "params": [],
            "choices": [],
        })
    });
    let exporter_list: Vec<_> = core
        .chain(exporters.iter().map(|e| {
            let params: Vec<_> = e
                .params
                .iter()
                .map(|p| {
                    json!({"name": p.name, "title": p.title, "kind": p.kind, "default": p.default})
                })
                .collect();
            let choices: Vec<_> = e
                .choices
                .iter()
                .map(|c| {
                    let options: Vec<_> = c.options.iter().map(|o| o.name.as_str()).collect();
                    json!({"name": c.name, "title": c.title, "default": c.default, "options": options})
                })
                .collect();
            json!({
                "id": e.id,
                "title": e.title,
                "extension": e.extension,
                "subject": format!("{:?}", e.subject).to_lowercase(),
                "params": params,
                "choices": choices,
            })
        }))
        .collect();
    let plugin_command_list: Vec<_> = plugin_commands.iter().map(plugin_command_json).collect();
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
        "exporters": exporter_list,
        "plugin_commands": plugin_command_list,
        "diagnostics": config.diagnostics,
    })
}

/// A plugin's command (ADR-0038): where it sits and its form.
fn plugin_command_json(c: &arrix_plugin_api::CommandSpec) -> Value {
    let params: Vec<_> = c
        .params
        .iter()
        .map(|p| json!({"name": p.name, "title": p.title, "kind": p.kind, "default": p.default}))
        .collect();
    let choices: Vec<_> = c
        .choices
        .iter()
        .map(|ch| {
            let options: Vec<_> = ch.options.iter().map(|o| o.name.as_str()).collect();
            json!({"name": ch.name, "title": ch.title, "default": ch.default, "options": options})
        })
        .collect();
    let inputs: Vec<_> = c
        .inputs
        .iter()
        .map(|i| json!({"name": i.name, "title": i.title, "kind": i.kind, "list": i.list}))
        .collect();
    json!({
        "id": c.id,
        "title": c.title,
        "group": c.group,
        "params": params,
        "choices": choices,
        "inputs": inputs,
        "max_new": c.max_new,
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
    for e in d["exporters"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "\nexporter {} ({}): a {} to .{}",
            text(&e["id"]),
            text(&e["title"]),
            text(&e["subject"]),
            text(&e["extension"])
        );
        for p in e["params"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {:<16} {}  (default {})",
                text(&p["name"]),
                text(&p["kind"]),
                text(&p["default"])
            );
        }
        for c in e["choices"].as_array().into_iter().flatten() {
            let options: Vec<_> = c["options"]
                .as_array()
                .into_iter()
                .flatten()
                .map(text)
                .collect();
            let _ = writeln!(
                out,
                "  {:<16} one of {}  (default {})",
                text(&c["name"]),
                options.join(", "),
                text(&c["default"])
            );
        }
    }
    for c in d["plugin_commands"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "\ncommand {} ({}): in the group {}",
            text(&c["id"]),
            text(&c["title"]),
            text(&c["group"])
        );
        for i in c["inputs"].as_array().into_iter().flatten() {
            let list = if i["list"] == true { " (a list)" } else { "" };
            let _ = writeln!(
                out,
                "  {:<16} picks {}{list}",
                text(&i["name"]),
                text(&i["kind"])
            );
        }
        for p in c["params"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {:<16} {}  (default {})",
                text(&p["name"]),
                text(&p["kind"]),
                text(&p["default"])
            );
        }
        for ch in c["choices"].as_array().into_iter().flatten() {
            let options: Vec<_> = ch["options"]
                .as_array()
                .into_iter()
                .flatten()
                .map(text)
                .collect();
            let _ = writeln!(
                out,
                "  {:<16} one of {}  (default {})",
                text(&ch["name"]),
                options.join(", "),
                text(&ch["default"])
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
