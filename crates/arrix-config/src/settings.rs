//! The settings: one spec per id, the built-in defaults, and `settings.toml`
//! laid over them. A key that is unknown, of the wrong type or lost to a
//! syntax error keeps the value below it and is one diagnostic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use toml::de::{DeTable, DeValue};

use crate::spanned::{File, Parsed, in_file_order, read_text, type_name};
use crate::{ConfigDiagnostic, ConfigSource, SETTINGS_TOML};

/// What a setting holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingKind {
    Bool,
    String,
    StringList,
}

/// A setting's value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    String(String),
    StringList(Vec<String>),
}

/// A built-in default, as a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultValue {
    Bool(bool),
    Str(&'static str),
    StrList(&'static [&'static str]),
}

impl DefaultValue {
    pub fn value(self) -> SettingValue {
        match self {
            Self::Bool(b) => SettingValue::Bool(b),
            Self::Str(s) => SettingValue::String(s.to_owned()),
            Self::StrList(l) => {
                SettingValue::StringList(l.iter().map(|s| (*s).to_owned()).collect())
            }
        }
    }
}

/// One setting. Its id is its dotted path in `settings.toml` and is
/// stable (ADR-0016, constraint 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingSpec {
    pub id: &'static str,
    pub kind: SettingKind,
    pub default: DefaultValue,
    pub doc: &'static str,
}

/// Every setting there is, sorted by id.
pub const SETTINGS: &[SettingSpec] = &[
    SettingSpec {
        id: "ui.panel.properties.visible",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(true),
        doc: "Whether the property panel is shown.",
    },
    SettingSpec {
        id: "ui.panel.tree.visible",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(true),
        doc: "Whether the feature tree is shown.",
    },
    SettingSpec {
        id: "ui.ribbon.hidden-tabs",
        kind: SettingKind::StringList,
        default: DefaultValue::StrList(&[]),
        doc: "Ribbon tabs not shown, by tab id; their commands stay in the palette.",
    },
    SettingSpec {
        id: "ui.theme",
        kind: SettingKind::String,
        default: DefaultValue::Str("dark"),
        doc: "The theme: a built-in one (`dark`, `light`) or `themes/<name>.toml`.",
    },
];

fn spec(id: &str) -> Option<&'static SettingSpec> {
    SETTINGS.iter().find(|s| s.id == id)
}

/// Whether `path` is a table on the way to a setting.
fn is_prefix(path: &str) -> bool {
    SETTINGS.iter().any(|s| {
        s.id.strip_prefix(path)
            .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// The effective settings: the defaults with each layer laid over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    values: BTreeMap<&'static str, SettingValue>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            values: SETTINGS.iter().map(|s| (s.id, s.default.value())).collect(),
        }
    }
}

impl Settings {
    /// The built-in layer, then the user's `settings.toml` from `source`.
    pub fn load(source: &impl ConfigSource) -> (Self, Vec<ConfigDiagnostic>) {
        let mut settings = Self::default();
        let mut diagnostics = Vec::new();
        if let Some(text) = read_text(source, SETTINGS_TOML, &mut diagnostics) {
            diagnostics = settings.apply(SETTINGS_TOML, &text);
        }
        (settings, diagnostics)
    }

    /// Lays one file's settings over these, and returns its diagnostics.
    pub fn apply(&mut self, path: &str, text: &str) -> Vec<ConfigDiagnostic> {
        let file = File { path, text };
        let parsed = file.parse();
        let mut diagnostics = parsed.diagnostics(file);
        self.apply_table(file, &parsed, "", parsed.table.get_ref(), &mut diagnostics);
        in_file_order(&mut diagnostics);
        diagnostics
    }

    fn apply_table(
        &mut self,
        file: File<'_>,
        parsed: &Parsed<'_>,
        prefix: &str,
        table: &DeTable<'_>,
        diagnostics: &mut Vec<ConfigDiagnostic>,
    ) {
        for (key, value) in table.iter() {
            // The feature types' defaults are `ParamDefaults`', whose
            // checks need the types.
            if prefix.is_empty() && key.get_ref() == "defaults" {
                continue;
            }
            let path = if prefix.is_empty() {
                key.get_ref().to_string()
            } else {
                format!("{prefix}.{}", key.get_ref())
            };
            if let Some(spec) = spec(&path) {
                if parsed.is_broken(key, value) {
                    continue;
                }
                match read(spec, value.get_ref()) {
                    Ok(v) => {
                        self.values.insert(spec.id, v);
                    }
                    Err((at, message)) => {
                        let span = at.unwrap_or_else(|| value.span());
                        diagnostics.push(file.error(span, "config.type", message));
                    }
                }
            } else if is_prefix(&path) {
                match value.get_ref() {
                    DeValue::Table(t) => self.apply_table(file, parsed, &path, t, diagnostics),
                    other => diagnostics.push(file.error(
                        value.span(),
                        "config.type",
                        format!("`{path}` is a table of settings, not {}", type_name(other)),
                    )),
                }
            } else {
                diagnostics.push(file.error(
                    key.span(),
                    "config.unknown-key",
                    format!("`{path}` is not a setting"),
                ));
            }
        }
    }

    /// The value of setting `id`; `None` only for an id that isn't one.
    pub fn get(&self, id: &str) -> Option<&SettingValue> {
        self.values.get(id)
    }

    /// The value of a boolean setting; `false` for an id that isn't one.
    pub fn bool(&self, id: &str) -> bool {
        matches!(self.get(id), Some(SettingValue::Bool(true)))
    }

    /// The value of a string setting; empty for an id that isn't one.
    pub fn string(&self, id: &str) -> &str {
        match self.get(id) {
            Some(SettingValue::String(s)) => s,
            _ => "",
        }
    }

    /// The value of a string-list setting; empty for an id that isn't one.
    pub fn string_list(&self, id: &str) -> &[String] {
        match self.get(id) {
            Some(SettingValue::StringList(l)) => l,
            _ => &[],
        }
    }
}

type ReadError = (Option<std::ops::Range<usize>>, String);

/// `value` as `spec`'s kind, or where and why it isn't one.
fn read(spec: &SettingSpec, value: &DeValue<'_>) -> Result<SettingValue, ReadError> {
    let wrong = |expected: &str, got: &DeValue<'_>| {
        (
            None,
            format!("`{}` is {expected}, not {}", spec.id, type_name(got)),
        )
    };
    match (spec.kind, value) {
        (SettingKind::Bool, DeValue::Boolean(b)) => Ok(SettingValue::Bool(*b)),
        (SettingKind::String, DeValue::String(s)) => Ok(SettingValue::String(s.to_string())),
        (SettingKind::StringList, DeValue::Array(items)) => items
            .iter()
            .map(|item| match item.get_ref() {
                DeValue::String(s) => Ok(s.to_string()),
                other => Err((
                    Some(item.span()),
                    format!(
                        "`{}` is a list of strings, and this item is {}",
                        spec.id,
                        type_name(other)
                    ),
                )),
            })
            .collect::<Result<_, _>>()
            .map(SettingValue::StringList),
        (SettingKind::Bool, other) => Err(wrong("a boolean", other)),
        (SettingKind::String, other) => Err(wrong("a string", other)),
        (SettingKind::StringList, other) => Err(wrong("a list of strings", other)),
    }
}
