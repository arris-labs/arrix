//! The keymap: the commands' default keys with `keybindings.toml` laid over
//! them. A user binding replaces its command's defaults, an empty list
//! unbinds it, and a key that is unparsable, unknown or taken is one
//! diagnostic and is left out (failing soft, ADR-0015).

use std::fmt;

use serde::Serialize;
use toml::Spanned;
use toml::de::DeValue;

use crate::spanned::{File, in_file_order, read_text, type_name};
use crate::{CommandRegistry, ConfigDiagnostic, ConfigSource, KEYBINDINGS_TOML};

/// The keymap contexts a binding's `when` names, with what each means. A
/// binding with no `when` applies everywhere, except in `text`. Where two
/// active contexts bind the same key, the more specific one wins: `text`
/// over `sketch-tool` over `sketch`.
pub const CONTEXTS: &[(&str, &str)] = &[
    (
        "part",
        "the document is shown and no sketch is being edited",
    ),
    ("sketch", "a sketch is being edited"),
    (
        "sketch-tool",
        "a sketch is being edited with a drawing tool; its keys win over `sketch`'s",
    ),
    ("palette", "the command palette is open"),
    (
        "text",
        "a text field has focus; only bindings that name it fire",
    ),
];

/// The named keys, besides letters, digits and `f1` to `f12`.
const NAMED_KEYS: &[&str] = &[
    "enter",
    "escape",
    "tab",
    "space",
    "backspace",
    "delete",
    "insert",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
];

/// A key with its modifiers: `ctrl+shift+k`. The key's name is the lower-case
/// one egui's `Key::name` gives, which is how the app builds one from an
/// event.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_: bool,
    pub key: String,
}

impl Chord {
    /// `ctrl+shift+k`, in any order and case, or why it isn't a chord.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut chord = Self::default();
        let parts: Vec<_> = text.split('+').map(|p| p.trim().to_lowercase()).collect();
        let (key, mods) = parts.split_last().expect("split gives one part at least");
        for m in mods {
            let slot = match m.as_str() {
                "ctrl" => &mut chord.ctrl,
                "shift" => &mut chord.shift,
                "alt" => &mut chord.alt,
                "super" => &mut chord.super_,
                other => {
                    return Err(format!(
                        "`{other}` is not a modifier (ctrl, shift, alt, super)"
                    ));
                }
            };
            if std::mem::replace(slot, true) {
                return Err(format!("the modifier `{m}` is repeated"));
            }
        }
        if !is_key(key) {
            return Err(if key.is_empty() {
                "no key after the modifiers".to_owned()
            } else {
                format!("`{key}` is not a key (a letter, a digit, f1 to f12, or a named key)")
            });
        }
        chord.key.clone_from(key);
        Ok(chord)
    }
}

fn is_key(name: &str) -> bool {
    let mut chars = name.chars();
    let single = matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_lowercase() || c.is_ascii_digit());
    let function = name
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n) && !name.starts_with("f0"));
    single || function || NAMED_KEYS.contains(&name)
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "ctrl"),
            (self.shift, "shift"),
            (self.alt, "alt"),
            (self.super_, "super"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl Serialize for Chord {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// One key bound to one command in one context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Binding {
    pub key: Chord,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

/// The effective bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

/// One `[[bind]]` entry that passed its own checks.
struct Entry {
    command: String,
    when: Option<String>,
    /// Each key with where it is written.
    keys: Vec<(Chord, std::ops::Range<usize>)>,
}

impl Keymap {
    /// The commands' own default keys.
    pub fn defaults(registry: &CommandRegistry) -> Self {
        let bindings = registry
            .commands
            .iter()
            .flat_map(|c| {
                c.keys.iter().map(|k| Binding {
                    key: Chord::parse(k).expect("a default key parses"),
                    command: c.id.clone(),
                    when: c.when.clone(),
                })
            })
            .collect();
        Self { bindings }
    }

    /// The defaults, then the user's `keybindings.toml` from `source`.
    pub fn load(
        source: &impl ConfigSource,
        registry: &CommandRegistry,
    ) -> (Self, Vec<ConfigDiagnostic>) {
        let mut diagnostics = Vec::new();
        let Some(text) = read_text(source, KEYBINDINGS_TOML, &mut diagnostics) else {
            return (Self::defaults(registry), diagnostics);
        };
        let keymap = Self::apply(registry, KEYBINDINGS_TOML, &text, &mut diagnostics);
        in_file_order(&mut diagnostics);
        (keymap, diagnostics)
    }

    fn apply(
        registry: &CommandRegistry,
        path: &str,
        text: &str,
        diagnostics: &mut Vec<ConfigDiagnostic>,
    ) -> Self {
        let file = File { path, text };
        let parsed = file.parse();
        diagnostics.extend(parsed.diagnostics(file));
        let mut entries = Vec::new();
        for (key, value) in parsed.table.get_ref().iter() {
            match (key.get_ref().as_ref(), value.get_ref()) {
                ("bind", DeValue::Array(items)) => {
                    for item in items {
                        if !parsed.is_broken(item, item) {
                            entries.extend(entry(file, registry, item, diagnostics));
                        }
                    }
                }
                ("bind", other) => diagnostics.push(file.error(
                    value.span(),
                    "config.type",
                    format!(
                        "`bind` is a list of `[[bind]]` tables, not {}",
                        type_name(other)
                    ),
                )),
                (other, _) => diagnostics.push(file.error(
                    key.span(),
                    "config.unknown-key",
                    format!("`{other}` is not a key of keybindings.toml (only `[[bind]]`)"),
                )),
            }
        }
        merge(file, registry, entries, diagnostics)
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The keys a command has, in `ctrl+k` form, for display.
    pub fn keys_of(&self, command: &str) -> Vec<String> {
        self.bindings
            .iter()
            .filter(|b| b.command == command)
            .map(|b| b.key.to_string())
            .collect()
    }

    /// The command `chord` runs while `contexts` are active, given most
    /// specific first. A binding that names a context beats one that
    /// doesn't, and of two that do, the one whose context comes first in
    /// `contexts`; in the `text` context only bindings that name it fire,
    /// so a text field keeps its keys.
    pub fn resolve(&self, chord: &Chord, contexts: &[&str]) -> Option<&str> {
        let typing = contexts.contains(&"text");
        let bound = || self.bindings.iter().filter(|b| &b.key == chord);
        contexts
            .iter()
            .find_map(|c| bound().find(|b| b.when.as_deref() == Some(*c)))
            .or_else(|| bound().rfind(|b| b.when.is_none() && !typing))
            .map(|b| b.command.as_str())
    }
}

/// One `[[bind]]` table, checked on its own: its command and context exist,
/// its keys parse. A key that doesn't is dropped alone; the entry stays so
/// that an emptied list still unbinds the command.
fn entry(
    file: File<'_>,
    registry: &CommandRegistry,
    item: &Spanned<DeValue<'_>>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Option<Entry> {
    let DeValue::Table(table) = item.get_ref() else {
        diagnostics.push(file.error(
            item.span(),
            "config.type",
            format!("a `[[bind]]` is a table, not {}", type_name(item.get_ref())),
        ));
        return None;
    };
    let (mut command, mut when, mut keys, mut ok) = (None, None, None, true);
    for (k, v) in table.iter() {
        match (k.get_ref().as_ref(), v.get_ref()) {
            ("command", DeValue::String(s)) => command = Some((s.to_string(), v.span())),
            ("when", DeValue::String(s)) => when = Some((s.to_string(), v.span())),
            ("key", DeValue::String(_) | DeValue::Array(_)) => keys = Some(v),
            (name @ ("command" | "when" | "key"), other) => {
                ok = false;
                let want = if name == "key" {
                    "a string or a list of strings"
                } else {
                    "a string"
                };
                diagnostics.push(file.error(
                    v.span(),
                    "config.type",
                    format!("`{name}` is {want}, not {}", type_name(other)),
                ));
            }
            (other, _) => {
                diagnostics.push(file.error(
                    k.span(),
                    "config.unknown-key",
                    format!("`{other}` is not a key of `[[bind]]` (key, command, when)"),
                ));
            }
        }
    }
    let Some((command, command_span)) = command else {
        if ok {
            diagnostics.push(file.error(
                item.span(),
                "config.missing-key",
                "a `[[bind]]` needs a `command`",
            ));
        }
        return None;
    };
    let Some(keys) = keys else {
        if ok {
            diagnostics.push(file.error(
                item.span(),
                "config.missing-key",
                "a `[[bind]]` needs a `key` (an empty list unbinds)",
            ));
        }
        return None;
    };
    if !ok {
        return None;
    }
    if registry.command(&command).is_none() {
        diagnostics.push(file.error(
            command_span,
            "config.unknown-command",
            format!("`{command}` is not a command"),
        ));
        return None;
    }
    if let Some((w, span)) = &when
        && !CONTEXTS.iter().any(|(name, _)| name == w)
    {
        diagnostics.push(file.error(
            span.clone(),
            "config.unknown-context",
            format!("`{w}` is not a keymap context ({})", context_names()),
        ));
        return None;
    }
    Some(Entry {
        command,
        when: when.map(|(w, _)| w),
        keys: chords(file, keys, diagnostics),
    })
}

fn context_names() -> String {
    CONTEXTS
        .iter()
        .map(|(n, _)| *n)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The chords of a `key` value, each with its span; one that doesn't parse
/// is a diagnostic and is left out.
fn chords(
    file: File<'_>,
    value: &Spanned<DeValue<'_>>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Vec<(Chord, std::ops::Range<usize>)> {
    let items: Vec<&Spanned<DeValue<'_>>> = match value.get_ref() {
        DeValue::Array(items) => items.iter().collect(),
        _ => vec![value],
    };
    let mut out = Vec::new();
    for item in items {
        let DeValue::String(text) = item.get_ref() else {
            diagnostics.push(file.error(
                item.span(),
                "config.type",
                format!("a key is a string, not {}", type_name(item.get_ref())),
            ));
            continue;
        };
        match Chord::parse(text) {
            Ok(chord) => out.push((chord, item.span())),
            Err(why) => diagnostics.push(file.error(item.span(), "config.bad-key", why)),
        }
    }
    out
}

/// The defaults of commands the user didn't rebind, and the user's bindings,
/// with each clash one diagnostic. The user's bindings go first, so a clash
/// with a default takes the key from it.
fn merge(
    file: File<'_>,
    registry: &CommandRegistry,
    entries: Vec<Entry>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Keymap {
    let mut bindings: Vec<Binding> = Vec::new();
    for e in &entries {
        for (chord, span) in &e.keys {
            let clash = bindings
                .iter()
                .find(|b| &b.key == chord && b.when == e.when);
            match clash {
                Some(b) if b.command == e.command => {}
                Some(b) => diagnostics.push(file.error(
                    span.clone(),
                    "config.key-conflict",
                    format!("`{chord}` is already bound to `{}`", b.command),
                )),
                None => bindings.push(Binding {
                    key: chord.clone(),
                    command: e.command.clone(),
                    when: e.when.clone(),
                }),
            }
        }
    }
    let user_len = bindings.len();
    for d in Keymap::defaults(registry).bindings {
        if entries.iter().any(|e| e.command == d.command) {
            continue;
        }
        let taken_by = bindings[..user_len]
            .iter()
            .position(|b| b.key == d.key && b.when == d.when);
        if let Some(i) = taken_by {
            let span = entries
                .iter()
                .flat_map(|e| &e.keys)
                .find(|(c, _)| *c == d.key)
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
            diagnostics.push(file.error(
                span,
                "config.key-conflict",
                format!(
                    "`{}` takes the key of `{}`, which is left without it",
                    bindings[i].command, d.command
                ),
            ));
        } else {
            bindings.push(d);
        }
    }
    Keymap { bindings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_in_any_order_and_case() {
        let c = Chord::parse("Shift+CTRL+k").unwrap();
        assert_eq!(c.to_string(), "ctrl+shift+k");
        assert_eq!(Chord::parse("f12").unwrap().key, "f12");
        assert_eq!(Chord::parse("pageup").unwrap().key, "pageup");
    }

    #[test]
    fn what_is_not_a_chord_says_why() {
        for bad in [
            "",
            "ctrl+",
            "f13",
            "f0",
            "ctrl+ctrl+k",
            "hyper+k",
            "kk",
            "ctrl+k+j",
            "é",
        ] {
            assert!(Chord::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_default_key_is_a_chord_and_defaults_do_not_clash() {
        let registry = CommandRegistry::new(&[]);
        let map = Keymap::defaults(&registry);
        let mut seen = std::collections::BTreeSet::new();
        for b in map.bindings() {
            assert!(
                seen.insert((b.key.clone(), b.when.clone())),
                "{b:?} clashes"
            );
        }
        assert!(!seen.is_empty());
    }
}
