//! Themes: colours by role, from `themes/<name>.toml` or one of the two
//! built-ins, which are embedded files in the same format (ADR-0015). A
//! role a theme leaves out takes the built-in theme's colour for it, and
//! that is a warning; a colour that doesn't resolve keeps the built-in
//! one and is an error. Failing soft either way.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use toml::de::{DeTable, DeValue};

use crate::spanned::{File, in_file_order, read_text, type_name};
use crate::{ConfigDiagnostic, ConfigSource, SETTINGS_TOML, THEMES_DIR};

/// Every colour role there is, with what it colours.
pub const THEME_ROLES: &[(&str, &str)] = &[
    (
        "panel",
        "The background of the ribbon, the panels and the status bar.",
    ),
    (
        "window",
        "The background of floating windows, the palette's included.",
    ),
    (
        "field",
        "The background of text fields and of the viewport.",
    ),
    ("text", "Labels and other text that does nothing."),
    ("text-widget", "The text of buttons and tabs at rest."),
    (
        "text-hover",
        "The text of a button or tab under the pointer.",
    ),
    ("text-active", "The text of a button or tab being pressed."),
    (
        "border",
        "Separators and the outline of panels and windows.",
    ),
    ("button", "The background of a button or tab at rest."),
    (
        "button-hover",
        "The background of a button or tab under the pointer.",
    ),
    (
        "button-active",
        "The background of a button or tab being pressed.",
    ),
    ("accent", "The background of a selection."),
    ("accent-text", "The outline of a selection."),
    ("link", "Links."),
    ("warn", "Warnings."),
    ("error", "Errors."),
];

/// The built-in themes' names; `ui.theme` also takes the name of a file
/// under `themes/`.
pub const BUILTIN_THEMES: &[&str] = &["dark", "light"];

const DARK: &str = include_str!("../themes/dark.toml");
const LIGHT: &str = include_str!("../themes/light.toml");

/// Which of the toolkit's two looks a theme starts from: what its colours
/// don't cover (hover outlines, shadows) follows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    Dark,
    Light,
}

impl Base {
    fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// An sRGB colour with alpha, written `#rrggbb` or `#rrggbbaa`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub [u8; 4]);

impl Color {
    /// `#rgb`, `#rrggbb` or `#rrggbbaa`, or why it isn't one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || format!("`{text}` is not a colour (#rgb, #rrggbb or #rrggbbaa)");
        let digits = text.strip_prefix('#').ok_or_else(bad)?;
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let n = |i: usize, len: usize| u8::from_str_radix(&digits[i..i + len], 16).expect("hex");
        match digits.len() {
            3 => Ok(Self([n(0, 1) * 17, n(1, 1) * 17, n(2, 1) * 17, 255])),
            6 => Ok(Self([n(0, 2), n(2, 2), n(4, 2), 255])),
            8 => Ok(Self([n(0, 2), n(2, 2), n(4, 2), n(6, 2)])),
            _ => Err(bad()),
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")?;
        if a != 255 {
            write!(f, "{a:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// A theme with every role set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Theme {
    pub name: String,
    pub base: Base,
    colors: BTreeMap<&'static str, Color>,
}

impl Theme {
    /// `dark` or `light`.
    pub fn builtin(name: &str) -> Option<Self> {
        let text = match name {
            "dark" => DARK,
            "light" => LIGHT,
            _ => return None,
        };
        let (theme, diagnostics) =
            Self::parse(name, &format!("{THEMES_DIR}/{name}.toml"), text, true);
        assert!(
            diagnostics.is_empty(),
            "built-in theme {name}: {diagnostics:?}"
        );
        Some(theme)
    }

    /// The theme `name` names: a built-in one, or `themes/<name>.toml`
    /// from `source`. One that can't be had is `dark`, with the reason.
    pub fn load(name: &str, source: &impl ConfigSource) -> (Self, Vec<ConfigDiagnostic>) {
        if let Some(theme) = Self::builtin(name) {
            return (theme, Vec::new());
        }
        let fallback = || Self::builtin("dark").expect("dark is built in");
        let setting = |message: String| {
            ConfigDiagnostic::error(SETTINGS_TOML, "", None, "config.unknown-theme", message)
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            let message = format!(
                "`ui.theme` is `{name}`, which is not a theme name (letters, digits, - and _)"
            );
            return (fallback(), vec![setting(message)]);
        }
        let path = format!("{THEMES_DIR}/{name}.toml");
        let mut diagnostics = Vec::new();
        let Some(text) = read_text(source, &path, &mut diagnostics) else {
            if diagnostics.is_empty() {
                let message = format!(
                    "`ui.theme` is `{name}`, which is neither `dark`, `light` nor a file {path}"
                );
                diagnostics.push(setting(message));
            }
            return (fallback(), diagnostics);
        };
        Self::parse(name, &path, &text, false)
    }

    /// One theme file over its base's built-in colours. A built-in file
    /// starts from nothing, and has to say every role itself.
    fn parse(name: &str, path: &str, text: &str, builtin: bool) -> (Self, Vec<ConfigDiagnostic>) {
        let file = File { path, text };
        let parsed = file.parse();
        let mut diagnostics = parsed.diagnostics(file);
        let table = parsed.table.get_ref();
        let mut base = Base::Dark;
        let mut vars: BTreeMap<String, (String, std::ops::Range<usize>)> = BTreeMap::new();
        let mut colors_at = None;
        for (key, value) in table.iter() {
            if parsed.is_broken(key, value) {
                continue;
            }
            match (key.get_ref().as_ref(), value.get_ref()) {
                ("base", DeValue::String(s)) => match s.as_ref() {
                    "dark" => base = Base::Dark,
                    "light" => base = Base::Light,
                    other => diagnostics.push(file.error(
                        value.span(),
                        "config.type",
                        format!("`base` is `dark` or `light`, not `{other}`"),
                    )),
                },
                ("vars", DeValue::Table(t)) => {
                    for (k, v) in t.iter() {
                        match v.get_ref() {
                            DeValue::String(s) => {
                                vars.insert(k.get_ref().to_string(), (s.to_string(), v.span()));
                            }
                            other => diagnostics.push(file.error(
                                v.span(),
                                "config.type",
                                format!("a var is a colour string, not {}", type_name(other)),
                            )),
                        }
                    }
                }
                ("colors", DeValue::Table(_)) => colors_at = Some((key.span(), value)),
                (name @ ("base" | "vars" | "colors"), other) => diagnostics.push(file.error(
                    value.span(),
                    "config.type",
                    format!("`{name}` is {}, not {}", expected(name), type_name(other)),
                )),
                (other, _) => diagnostics.push(file.error(
                    key.span(),
                    "config.unknown-key",
                    format!("`{other}` is not a key of a theme (base, vars, colors)"),
                )),
            }
        }
        let mut colors: BTreeMap<&'static str, Color> = if builtin {
            BTreeMap::new()
        } else {
            Self::builtin(base.name())
                .expect("a base is built in")
                .colors
        };
        let mut set = Vec::new();
        if let Some((_, value)) = colors_at
            && let DeValue::Table(t) = value.get_ref()
        {
            read_colors(
                file,
                &parsed,
                t,
                &vars,
                &mut colors,
                &mut set,
                &mut diagnostics,
            );
        }
        let missing: Vec<_> = THEME_ROLES
            .iter()
            .map(|(r, _)| *r)
            .filter(|r| !set.contains(r))
            .collect();
        if !builtin && !missing.is_empty() {
            let at = colors_at.map_or(0, |(span, _)| span.start);
            diagnostics.push(file.warning(
                at..at,
                "config.theme-role-missing",
                format!(
                    "no colour for {}; {} has them",
                    missing.join(", "),
                    base.name()
                ),
            ));
        }
        in_file_order(&mut diagnostics);
        let theme = Self {
            name: name.to_owned(),
            base,
            colors,
        };
        (theme, diagnostics)
    }

    /// The colour of `role`; every theme has all of them.
    pub fn color(&self, role: &str) -> Color {
        *self
            .colors
            .get(role)
            .unwrap_or_else(|| panic!("`{role}` is not a theme role"))
    }

    /// Each role with its colour, in role order.
    pub fn colors(&self) -> impl Iterator<Item = (&'static str, Color)> + '_ {
        THEME_ROLES.iter().map(|(r, _)| (*r, self.color(r)))
    }
}

fn expected(key: &str) -> &'static str {
    if key == "base" { "a string" } else { "a table" }
}

fn read_colors(
    file: File<'_>,
    parsed: &crate::spanned::Parsed<'_>,
    table: &DeTable<'_>,
    vars: &BTreeMap<String, (String, std::ops::Range<usize>)>,
    colors: &mut BTreeMap<&'static str, Color>,
    set: &mut Vec<&'static str>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    for (key, value) in table.iter() {
        if parsed.is_broken(key, value) {
            continue;
        }
        let Some((role, _)) = THEME_ROLES
            .iter()
            .find(|(r, _)| *r == key.get_ref().as_ref())
        else {
            diagnostics.push(file.error(
                key.span(),
                "config.unknown-key",
                format!("`{}` is not a colour role", key.get_ref()),
            ));
            continue;
        };
        let DeValue::String(text) = value.get_ref() else {
            diagnostics.push(file.error(
                value.span(),
                "config.type",
                format!(
                    "`{role}` is a colour string, not {}",
                    type_name(value.get_ref())
                ),
            ));
            continue;
        };
        match resolve(text, vars, &mut Vec::new()) {
            Ok(color) => {
                colors.insert(role, color);
                set.push(role);
            }
            Err(message) => diagnostics.push(file.error(value.span(), "config.bad-color", message)),
        }
    }
}

/// A colour: a literal, or `$name` of a var that is one in turn.
fn resolve(
    text: &str,
    vars: &BTreeMap<String, (String, std::ops::Range<usize>)>,
    seen: &mut Vec<String>,
) -> Result<Color, String> {
    let Some(name) = text.strip_prefix('$') else {
        return Color::parse(text);
    };
    if seen.iter().any(|s| s == name) {
        seen.push(name.to_owned());
        return Err(format!(
            "the vars go round in a circle: {}",
            seen.join(" -> ")
        ));
    }
    seen.push(name.to_owned());
    let (value, _) = vars
        .get(name)
        .ok_or_else(|| format!("`${name}` is not a var of this theme"))?;
    resolve(value, vars, seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemorySource;

    fn user(text: &str) -> (Theme, Vec<ConfigDiagnostic>) {
        let source = MemorySource::with([("themes/mine.toml", text)]);
        Theme::load("mine", &source)
    }

    fn codes(d: &[ConfigDiagnostic]) -> Vec<(&str, Option<u32>)> {
        d.iter()
            .map(|d| (d.diagnostic.code.as_str(), d.line))
            .collect()
    }

    #[test]
    fn the_built_in_themes_have_every_role() {
        for name in BUILTIN_THEMES {
            let theme = Theme::builtin(name).unwrap();
            assert_eq!(theme.colors().count(), THEME_ROLES.len(), "{name}");
        }
        assert_eq!(Theme::builtin("dark").unwrap().base, Base::Dark);
        assert_eq!(Theme::builtin("light").unwrap().base, Base::Light);
    }

    #[test]
    fn colours_parse_and_print() {
        assert_eq!(Color::parse("#fa0").unwrap(), Color([0xff, 0xaa, 0, 255]));
        assert_eq!(Color::parse("#10203040").unwrap().to_string(), "#10203040");
        assert_eq!(Color::parse("#102030").unwrap().to_string(), "#102030");
        for bad in ["102030", "#12", "#gg0000", "red", "#1234567"] {
            assert!(Color::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_theme_overrides_its_base_and_vars_chain() {
        let (theme, d) = user(
            "base = \"light\"\n[vars]\na = \"$b\"\nb = \"#102030\"\n[colors]\npanel = \"$a\"\n",
        );
        assert_eq!(theme.base, Base::Light);
        assert_eq!(theme.color("panel"), Color([0x10, 0x20, 0x30, 255]));
        assert_eq!(
            theme.color("text"),
            Theme::builtin("light").unwrap().color("text")
        );
        // One warning, on the `[colors]` line, naming what fell back.
        assert_eq!(codes(&d), [("config.theme-role-missing", Some(5))]);
        assert_eq!(d[0].diagnostic.severity, arrix_core::Severity::Warning);
        assert!(d[0].diagnostic.message.contains("window"));
    }

    #[test]
    fn a_complete_theme_gives_no_diagnostics() {
        let mut text = String::from("[colors]\n");
        for (role, _) in THEME_ROLES {
            text.push_str(&format!("{role} = \"#123456\"\n"));
        }
        let (theme, d) = user(&text);
        assert!(d.is_empty(), "{d:?}");
        assert_eq!(theme.name, "mine");
        assert!(
            theme
                .colors()
                .all(|(_, c)| c == Color([0x12, 0x34, 0x56, 255]))
        );
    }

    #[test]
    fn bad_colours_keep_the_built_in_one_and_name_their_line() {
        let (theme, d) = user(
            "[vars]\nx = \"$y\"\ny = \"$x\"\n[colors]\npanel = \"blue\"\nfield = \"$x\"\nwindw = \"#000\"\ntext = 5\nlink = \"$nope\"\n",
        );
        let dark = Theme::builtin("dark").unwrap();
        assert_eq!(theme.color("panel"), dark.color("panel"));
        assert_eq!(
            codes(&d),
            [
                ("config.theme-role-missing", Some(4)),
                ("config.bad-color", Some(5)),
                ("config.bad-color", Some(6)),
                ("config.unknown-key", Some(7)),
                ("config.type", Some(8)),
                ("config.bad-color", Some(9)),
            ]
        );
    }

    #[test]
    fn structure_errors_are_diagnostics() {
        let (_, d) = user("base = \"blue\"\nextra = 1\nvars = 3\ncolors = []\n");
        assert_eq!(
            codes(&d),
            [
                ("config.theme-role-missing", Some(1)),
                ("config.type", Some(1)),
                ("config.unknown-key", Some(2)),
                ("config.type", Some(3)),
                ("config.type", Some(4)),
            ]
        );
    }

    #[test]
    fn a_theme_that_cannot_be_had_is_dark_with_the_reason() {
        let source = MemorySource::default();
        let (theme, d) = Theme::load("nope", &source);
        assert_eq!(theme.name, "dark");
        assert_eq!(codes(&d), [("config.unknown-theme", None)]);
        assert_eq!(d[0].file, "settings.toml");
        let (theme, d) = Theme::load("../x", &source);
        assert_eq!(theme.name, "dark");
        assert_eq!(codes(&d), [("config.unknown-theme", None)]);
        let (theme, d) = Theme::load("light", &source);
        assert_eq!(theme.name, "light");
        assert!(d.is_empty());
    }
}
