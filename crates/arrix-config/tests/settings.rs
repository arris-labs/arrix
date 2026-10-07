//! `settings.toml` over the defaults (ADR-0015): each unknown key, wrong
//! type and syntax error is one diagnostic naming its file and line, and
//! keeps the default.

use arrix_config::{
    ConfigDiagnostic, ConfigSource, MemorySource, SETTINGS, SETTINGS_TOML, SettingValue, Settings,
};

fn load(text: &str) -> (Settings, Vec<ConfigDiagnostic>) {
    Settings::load(&MemorySource::with([(SETTINGS_TOML, text)]))
}

/// `(code, line)` of each diagnostic, all of them in `settings.toml`.
fn found(diagnostics: &[ConfigDiagnostic]) -> Vec<(&str, u32)> {
    diagnostics
        .iter()
        .map(|d| {
            assert_eq!(d.file, SETTINGS_TOML);
            (d.diagnostic.code.as_str(), d.line.expect("a line"))
        })
        .collect()
}

#[test]
fn the_defaults_are_every_spec_and_nothing_else() {
    let settings = Settings::default();
    for spec in SETTINGS {
        assert_eq!(
            settings.get(spec.id),
            Some(&spec.default.value()),
            "{}",
            spec.id
        );
    }
    assert_eq!(settings.get("ui.nope"), None);
    assert_eq!(settings.string("ui.theme"), "dark");
    assert_eq!(settings.string("ui.panel.tree"), "auto");
    assert!(settings.string_list("ui.ribbon.hidden-tabs").is_empty());
    let mut ids: Vec<_> = SETTINGS.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        SETTINGS.iter().map(|s| s.id).collect::<Vec<_>>(),
        "sorted by id"
    );
}

#[test]
fn no_file_and_a_clean_file_give_no_diagnostics() {
    let (settings, diagnostics) = Settings::load(&MemorySource::default());
    assert_eq!(settings, Settings::default());
    assert!(diagnostics.is_empty());

    let (settings, diagnostics) = load(
        "# mine\n\
         [ui]\n\
         theme = \"mine\"\n\
         ribbon.hidden-tabs = [\"core.part\"]\n\
         \n\
         panel.tree = \"hidden\"\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(settings.string("ui.theme"), "mine");
    assert_eq!(settings.string_list("ui.ribbon.hidden-tabs"), ["core.part"]);
    assert_eq!(settings.string("ui.panel.tree"), "hidden");
    assert_eq!(
        settings.string("ui.panel.properties"),
        "auto",
        "an absent key keeps its default"
    );
}

#[test]
fn the_user_layer_lies_over_the_defaults_and_a_later_layer_over_it() {
    let mut settings = Settings::default();
    assert!(settings.apply("a.toml", "ui.theme = \"light\"").is_empty());
    assert_eq!(settings.string("ui.theme"), "light");
    assert!(settings.apply("b.toml", "ui.theme = \"mine\"").is_empty());
    assert_eq!(settings.string("ui.theme"), "mine");
    // A bad key keeps what the layers below gave it, not the built-in.
    let diagnostics = settings.apply("c.toml", "ui.theme = 3");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].file, "c.toml");
    assert_eq!(settings.string("ui.theme"), "mine");
}

#[test]
fn an_unknown_key_is_one_diagnostic_on_its_line() {
    let (settings, diagnostics) = load("[ui]\ntheme = \"light\"\nthem = \"dark\"\n[nope]\nx = 1\n");
    assert_eq!(
        found(&diagnostics),
        [("config.unknown-key", 3), ("config.unknown-key", 4)]
    );
    assert!(diagnostics[0].diagnostic.message.contains("`ui.them`"));
    assert!(diagnostics[1].diagnostic.message.contains("`nope`"));
    assert_eq!(diagnostics[0].column, Some(1));
    assert_eq!(
        settings.string("ui.theme"),
        "light",
        "the rest of the file applies"
    );
}

#[test]
fn a_wrong_type_is_one_diagnostic_and_keeps_the_default() {
    let (settings, diagnostics) = load(
        "[ui]\n\
         theme = 3\n\
         ribbon.hidden-tabs = [\"gears\", 7]\n\
         panel.tree = true\n\
         panel.properties = \"sometimes\"\n",
    );
    assert_eq!(
        found(&diagnostics),
        [
            ("config.type", 2),
            ("config.type", 3),
            ("config.type", 4),
            ("config.type", 5)
        ],
        "in the file's order"
    );
    let messages: Vec<_> = diagnostics
        .iter()
        .map(|d| d.diagnostic.message.as_str())
        .collect();
    assert!(messages[0].contains("`ui.theme` is a string, not an integer"));
    assert!(messages[1].contains("this item is an integer"));
    assert_eq!(diagnostics[1].column, Some(32), "the item itself is named");
    assert!(messages[2].contains("`ui.panel.tree` is a string, not a boolean"));
    assert!(messages[3].contains(
        "`ui.panel.properties` is one of \"auto\", \"shown\", \"hidden\", not \"sometimes\""
    ));
    assert_eq!(settings, Settings::default());

    let (_, diagnostics) = load("ui.panel = true\n");
    assert_eq!(found(&diagnostics), [("config.type", 1)]);
    assert!(
        diagnostics[0]
            .diagnostic
            .message
            .contains("`ui.panel` is a table of settings, not a boolean")
    );
}

#[test]
fn a_syntax_error_is_a_diagnostic_and_the_rest_of_the_file_applies() {
    let (settings, diagnostics) = load("[ui]\ntheme = \"light\"\nribbon.hidden-tabs = [\"a\"\n");
    let codes = found(&diagnostics);
    assert!(!codes.is_empty());
    assert!(
        codes
            .iter()
            .all(|&(code, line)| code == "config.syntax" && line >= 3),
        "{codes:?}"
    );
    assert_eq!(settings.string("ui.theme"), "light");
    assert!(settings.string_list("ui.ribbon.hidden-tabs").is_empty());
}

#[test]
fn an_unreadable_or_non_utf8_file_is_a_diagnostic_without_a_line() {
    struct Broken;
    impl ConfigSource for Broken {
        fn read(&self, _: &str) -> std::io::Result<Option<Vec<u8>>> {
            Err(std::io::Error::other("denied"))
        }
    }
    let (settings, diagnostics) = Settings::load(&Broken);
    assert_eq!(settings, Settings::default());
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].diagnostic.code.as_str(), "config.io");
    assert_eq!(diagnostics[0].line, None);

    let mut source = MemorySource::default();
    source.0.insert(SETTINGS_TOML.into(), vec![0xff, b'=']);
    let (_, diagnostics) = Settings::load(&source);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].diagnostic.code.as_str(), "config.syntax");
    assert_eq!(diagnostics[0].line, None);
}

#[test]
fn a_value_crosses_as_plain_json() {
    let json = |v: &SettingValue| serde_json::to_string(v).unwrap();
    assert_eq!(json(&SettingValue::Bool(true)), "true");
    assert_eq!(json(&SettingValue::String("dark".into())), "\"dark\"");
    assert_eq!(
        json(&SettingValue::StringList(vec!["gears".into()])),
        "[\"gears\"]"
    );
}

#[test]
fn the_retired_visible_keys_name_their_replacement() {
    let (settings, diagnostics) = load(
        "[ui.panel.tree]\n\
         visible = false\n\
         [ui.panel]\n\
         properties.visible = true\n",
    );
    assert_eq!(
        found(&diagnostics),
        [("config.renamed", 2), ("config.renamed", 4)]
    );
    assert!(
        diagnostics[0]
            .diagnostic
            .message
            .contains("`ui.panel.tree.visible` is now `ui.panel.tree`")
    );
    assert!(
        diagnostics[1]
            .diagnostic
            .message
            .contains("`ui.panel.properties`")
    );
    assert_eq!(settings, Settings::default(), "nothing is carried over");
}
