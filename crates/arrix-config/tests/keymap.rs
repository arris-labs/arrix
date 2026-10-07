//! `keybindings.toml` over the commands' default keys (ADR-0015): a user
//! binding replaces the defaults, an empty list unbinds, and each clash,
//! unknown command and unparsable key is one diagnostic with its line.

use arrix_config::{
    Chord, CommandRegistry, ConfigDiagnostic, KEYBINDINGS_TOML, Keymap, MemorySource,
};

fn registry() -> CommandRegistry {
    CommandRegistry::new(&[])
}

fn load(text: &str) -> (Keymap, Vec<ConfigDiagnostic>) {
    Keymap::load(&MemorySource::with([(KEYBINDINGS_TOML, text)]), &registry())
}

fn found(diagnostics: &[ConfigDiagnostic]) -> Vec<(&str, u32)> {
    diagnostics
        .iter()
        .map(|d| {
            assert_eq!(d.file, KEYBINDINGS_TOML);
            (d.diagnostic.code.as_str(), d.line.expect("a line"))
        })
        .collect()
}

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

#[test]
fn no_file_and_an_empty_file_give_the_defaults() {
    let (none, d) = Keymap::load(&MemorySource::default(), &registry());
    assert!(d.is_empty());
    assert_eq!(none, Keymap::defaults(&registry()));
    let (empty, d) = load("");
    assert!(d.is_empty());
    assert_eq!(empty, none);
    assert_eq!(
        none.resolve(&chord("ctrl+k"), &[]),
        Some("core.palette.open")
    );
}

#[test]
fn a_user_binding_replaces_the_default_keys() {
    let (map, d) = load("[[bind]]\nkey = \"ctrl+p\"\ncommand = \"core.palette.open\"\n");
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(map.keys_of("core.palette.open"), ["ctrl+p"]);
    assert_eq!(map.resolve(&chord("ctrl+k"), &[]), None);
    assert_eq!(
        map.resolve(&chord("ctrl+p"), &[]),
        Some("core.palette.open")
    );
}

#[test]
fn a_list_binds_several_keys_and_an_empty_one_unbinds() {
    let (map, d) = load(
        "[[bind]]\nkey = [\"ctrl+p\", \"f1\"]\ncommand = \"core.palette.open\"\n\n\
         [[bind]]\nkey = []\ncommand = \"core.config.reload\"\n",
    );
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(map.keys_of("core.palette.open"), ["ctrl+p", "f1"]);
    let (map, _) = load("[[bind]]\nkey = []\ncommand = \"core.palette.open\"\n");
    assert!(map.keys_of("core.palette.open").is_empty());
}

#[test]
fn a_bad_entry_is_one_diagnostic_on_its_line_and_the_rest_applies() {
    let text = "\
[[bind]]
key = \"ctrl+q\"
command = \"core.nope\"

[[bind]]
key = [\"ctrl+shift+\", \"f2\"]
command = \"core.config.reload\"

[[bind]]
key = \"f3\"
command = \"core.config.reload\"
when = \"mars\"

[[bind]]
command = \"core.config.reload\"

[[bind]]
key = \"f4\"
command = \"core.panel.tree.toggle\"
colour = 1
";
    let (map, d) = load(text);
    assert_eq!(
        found(&d),
        [
            ("config.unknown-command", 3),
            ("config.bad-key", 6),
            ("config.unknown-context", 12),
            ("config.missing-key", 14),
            ("config.unknown-key", 20),
        ]
    );
    // The good key of the list, and the entry with only an unknown key.
    assert_eq!(map.keys_of("core.config.reload"), ["f2"]);
    assert_eq!(map.keys_of("core.panel.tree.toggle"), ["f4"]);
}

#[test]
fn two_commands_on_one_key_in_one_context_clash() {
    let text = "\
[[bind]]
key = \"f5\"
command = \"core.config.reload\"

[[bind]]
key = \"f5\"
command = \"core.panel.tree.toggle\"

[[bind]]
key = \"f5\"
command = \"core.panel.tree.toggle\"
when = \"sketch\"

[[bind]]
key = \"f5\"
command = \"core.config.reload\"
";
    let (map, d) = load(text);
    // The same key under another context is no clash, nor is a repeat.
    assert_eq!(found(&d), [("config.key-conflict", 6)]);
    assert_eq!(map.resolve(&chord("f5"), &[]), Some("core.config.reload"));
    assert_eq!(
        map.resolve(&chord("f5"), &["sketch"]),
        Some("core.panel.tree.toggle")
    );
}

#[test]
fn a_user_key_taking_a_default_one_leaves_that_command_without_it() {
    let (map, d) = load("[[bind]]\nkey = \"ctrl+k\"\ncommand = \"core.config.reload\"\n");
    assert_eq!(found(&d), [("config.key-conflict", 2)]);
    assert_eq!(
        map.resolve(&chord("ctrl+k"), &[]),
        Some("core.config.reload")
    );
    assert!(map.keys_of("core.palette.open").is_empty());
}

#[test]
fn a_syntax_error_and_a_wrong_shape_are_diagnostics() {
    let (map, d) = load("bind = 3\n[[bind]\n");
    assert!(
        d.iter()
            .any(|d| d.diagnostic.code.as_str() == "config.syntax")
    );
    assert!(
        d.iter()
            .any(|d| d.diagnostic.code.as_str() == "config.type")
    );
    assert_eq!(map, Keymap::defaults(&registry()));
}

#[test]
fn a_context_binding_beats_a_global_one_and_text_keeps_its_keys() {
    let (map, d) = load(
        "[[bind]]\nkey = \"enter\"\ncommand = \"core.config.reload\"\n\n\
         [[bind]]\nkey = \"enter\"\ncommand = \"core.palette.open\"\nwhen = \"palette\"\n\n\
         [[bind]]\nkey = \"f6\"\ncommand = \"core.panel.tree.toggle\"\nwhen = \"text\"\n",
    );
    assert!(d.is_empty(), "{d:?}");
    let enter = chord("enter");
    assert_eq!(map.resolve(&enter, &["part"]), Some("core.config.reload"));
    assert_eq!(
        map.resolve(&enter, &["part", "palette"]),
        Some("core.palette.open")
    );
    assert_eq!(map.resolve(&enter, &["part", "text"]), None);
    assert_eq!(
        map.resolve(&chord("f6"), &["part", "text"]),
        Some("core.panel.tree.toggle")
    );
}
