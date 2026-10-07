//! `arrix config schema`: JSON Schemas for the three kinds of config file,
//! so an editor with taplo completes and checks them (`#:schema` at the top
//! of a file). They are built from the same specs the loader checks against,
//! so they cannot drift from it (ADR-0015).

use std::process::ExitCode;

use arrix_config::{BUILTIN_THEMES, CONTEXTS, CommandRegistry, SETTINGS, SettingKind, THEME_ROLES};
use arrix_plugin_api::FeatureTypeSpec;
use serde_json::{Map, Value, json};

use crate::plugins;

/// The version of the output's shape.
const SCHEMA: u32 = 1;

/// The file kinds `config schema` takes, with their file names.
pub const KINDS: &[&str] = &["settings", "keybindings", "theme"];

pub fn run(kind: Option<&str>) -> ExitCode {
    let registry = plugins::registry();
    let types = plugins::feature_types(&registry);
    let commands = CommandRegistry::new(&types);
    let all = schemas(&commands, &types);
    let out = match kind {
        None => all,
        Some(kind) => match all.get(kind) {
            Some(one) => one.clone(),
            None => {
                eprintln!(
                    "arrix config schema: no such file kind `{kind}` ({})",
                    KINDS.join(", ")
                );
                return ExitCode::from(2);
            }
        },
    };
    use std::io::Write as _;
    let text = serde_json::to_string(&out).expect("a schema is plain data");
    if writeln!(std::io::stdout(), "{text}").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// `{"schema": 1, "settings": …, "keybindings": …, "theme": …}`.
pub fn schemas(commands: &CommandRegistry, types: &[FeatureTypeSpec]) -> Value {
    json!({
        "schema": SCHEMA,
        "settings": settings(commands, types),
        "keybindings": keybindings(commands),
        "theme": theme(),
    })
}

fn draft(title: &str, doc: &str, mut body: Map<String, Value>) -> Value {
    body.insert(
        "$schema".into(),
        json!("http://json-schema.org/draft-07/schema#"),
    );
    body.insert("title".into(), json!(title));
    body.insert("description".into(), json!(doc));
    Value::Object(body)
}

fn object(properties: Map<String, Value>) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("type".into(), json!("object"));
    m.insert("additionalProperties".into(), json!(false));
    m.insert("properties".into(), Value::Object(properties));
    m
}

fn settings(commands: &CommandRegistry, types: &[FeatureTypeSpec]) -> Value {
    let mut root = Map::new();
    for s in SETTINGS {
        let mut leaf = match s.kind {
            SettingKind::Bool => json!({"type": "boolean"}),
            SettingKind::String => json!({"type": "string"}),
            SettingKind::StringList => json!({"type": "array", "items": {"type": "string"}}),
        };
        leaf["description"] = json!(s.doc);
        leaf["default"] = json!(s.default.value());
        match s.id {
            "ui.theme" => {
                leaf["examples"] = json!(BUILTIN_THEMES);
            }
            "ui.ribbon.hidden-tabs" => {
                let tabs: Vec<_> = commands.tabs.iter().map(|t| t.id.as_str()).collect();
                leaf["items"]["enum"] = json!(tabs);
            }
            _ => {}
        }
        let mut at = &mut root;
        let mut parts = s.id.split('.').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                at.insert(part.into(), leaf);
                break;
            }
            let child = at
                .entry(part)
                .or_insert_with(|| Value::Object(object(Map::new())));
            at = child["properties"].as_object_mut().expect("an object");
        }
    }
    let mut defaults = Map::new();
    for ty in types {
        let mut params = Map::new();
        for p in &ty.params {
            params.insert(
                p.name.clone(),
                json!({
                    "type": "string",
                    "description": format!(
                        "{}: an expression of kind {:?}; built in {}.",
                        p.title, p.kind, p.default
                    ),
                    "default": p.default,
                }),
            );
        }
        let mut one = object(params);
        one.insert(
            "description".into(),
            json!(format!("Default parameters of a new `{}`.", ty.title)),
        );
        defaults.insert(ty.id.clone(), Value::Object(one));
    }
    let mut defaults_table = object(defaults);
    defaults_table.insert(
        "description".into(),
        json!("A feature type's parameter defaults; they seed a new feature's form only."),
    );
    root.insert("defaults".into(), Value::Object(defaults_table));
    draft(
        "ArriX settings.toml",
        "User settings, laid over the built-in defaults.",
        object(root),
    )
}

fn keybindings(commands: &CommandRegistry) -> Value {
    let ids: Vec<_> = commands.commands.iter().map(|c| c.id.as_str()).collect();
    let contexts: Vec<_> = CONTEXTS.iter().map(|(name, _)| *name).collect();
    let bind = object(
        [
            (
                "key".to_owned(),
                json!({
                    "type": "string",
                    "description": "Modifiers (ctrl, shift, alt, super) and a key, joined by +: `ctrl+shift+k`.",
                }),
            ),
            (
                "command".to_owned(),
                json!({"type": "string", "enum": ids, "description": "The command's stable id."}),
            ),
            (
                "when".to_owned(),
                json!({"type": "string", "enum": contexts, "description": "The keymap context the binding applies in; everywhere when absent."}),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let mut bind = bind;
    bind.insert("required".into(), json!(["key", "command"]));
    draft(
        "ArriX keybindings.toml",
        "A binding replaces its command's default keys.",
        object(
            [("bind".to_owned(), json!({"type": "array", "items": bind}))]
                .into_iter()
                .collect(),
        ),
    )
}

fn theme() -> Value {
    let colour = json!({
        "type": "string",
        "pattern": r"^(#[0-9a-fA-F]{3}|#[0-9a-fA-F]{6}|#[0-9a-fA-F]{8}|\$[A-Za-z0-9_-]+)$",
        "description": "#rgb, #rrggbb, #rrggbbaa, or `$var`.",
    });
    let roles: Map<String, Value> = THEME_ROLES
        .iter()
        .map(|(role, doc)| {
            let mut c = colour.clone();
            c["description"] = json!(format!(
                "{doc} {}",
                colour["description"].as_str().unwrap_or("")
            ));
            ((*role).to_owned(), c)
        })
        .collect();
    let mut vars = Map::new();
    vars.insert("type".into(), json!("object"));
    vars.insert("additionalProperties".into(), json!({"type": "string"}));
    draft(
        "ArriX theme",
        "A theme: colours by role over a built-in base.",
        object(
            [
                (
                    "base".to_owned(),
                    json!({"enum": ["dark", "light"], "description": "The look the roles don't cover."}),
                ),
                ("vars".to_owned(), Value::Object(vars)),
                ("colors".to_owned(), Value::Object(object(roles))),
            ]
            .into_iter()
            .collect(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_and_role_is_in_its_schema() {
        let registry = plugins::registry();
        let types = plugins::feature_types(&registry);
        let s = schemas(&CommandRegistry::new(&types), &types);
        assert!(
            s["settings"]["properties"]["ui"]["properties"]["ribbon"]["properties"]["hidden-tabs"]
                .is_object()
        );
        assert!(s["settings"]["properties"]["defaults"]["properties"]["gears.spur"]["properties"]["module"].is_object());
        assert_eq!(
            s["theme"]["properties"]["colors"]["properties"]
                .as_object()
                .unwrap()
                .len(),
            THEME_ROLES.len()
        );
        assert!(
            s["keybindings"]["properties"]["bind"]["items"]["properties"]["command"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!("core.palette.open"))
        );
    }
}
