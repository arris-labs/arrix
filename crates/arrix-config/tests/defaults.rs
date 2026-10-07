//! `[defaults."<type>"]` in `settings.toml` (ADR-0015): checked against the
//! type's parameters, seeding a new feature's form, a bad entry one
//! diagnostic on its line that falls back to the type's own default.

use arrix_config::{ConfigDiagnostic, MemorySource, ParamDefaults, SETTINGS_TOML, Settings};
use arrix_core::{Diagnostic, QuantityKind, Severity};
use arrix_plugin_api::{FeatureTypeSpec, ParamSpec};

fn spur() -> FeatureTypeSpec {
    let param = |name: &str, kind, default: &str| ParamSpec {
        name: name.into(),
        title: name.into(),
        kind,
        default: default.into(),
        shown_when: None,
    };
    FeatureTypeSpec {
        id: "gears.spur".into(),
        version: 1,
        title: "Spur gear".into(),
        params: vec![
            param("teeth", QuantityKind::Count, "20"),
            param("module", QuantityKind::Length, "1 mm"),
            param("pressure_angle", QuantityKind::Angle, "20 deg"),
        ],
        inputs: Vec::new(),
        outputs: Vec::new(),
        choices: vec![],
    }
}

/// A stand-in for `arrix-doc`'s checker, which this crate may not depend
/// on: a number with an optional unit, whose unit decides the kind.
fn check(text: &str, kind: QuantityKind) -> Result<(), Diagnostic> {
    let fail = |code: &str, message: &str| {
        Err(Diagnostic::new(Severity::Error, code, message).expect("a code"))
    };
    let mut parts = text.split_whitespace();
    if parts.next().and_then(|n| n.parse::<f64>().ok()).is_none() {
        return fail("expr.syntax", "not a number");
    }
    let have = match parts.next() {
        None => QuantityKind::Count,
        Some("mm") => QuantityKind::Length,
        Some("deg") => QuantityKind::Angle,
        Some(_) => return fail("expr.unknown-unit", "unknown unit"),
    };
    if parts.next().is_some() {
        return fail("expr.syntax", "trailing text");
    }
    if have == kind {
        Ok(())
    } else {
        fail("expr.unit-mismatch", "wrong kind")
    }
}

fn load(text: &str) -> (ParamDefaults, Vec<ConfigDiagnostic>) {
    let source = MemorySource::with([(SETTINGS_TOML, text)]);
    ParamDefaults::load(&source, &[spur()], &check)
}

fn form(defaults: &ParamDefaults) -> Vec<(String, String)> {
    defaults.effective_params(&spur())
}

fn pair(name: &str, expr: &str) -> (String, String) {
    (name.to_owned(), expr.to_owned())
}

#[test]
fn without_a_file_or_a_section_the_form_is_the_types_own() {
    let own = [
        pair("teeth", "20"),
        pair("module", "1 mm"),
        pair("pressure_angle", "20 deg"),
    ];
    let (d, diagnostics) = ParamDefaults::load(&MemorySource::default(), &[spur()], &check);
    assert!(diagnostics.is_empty());
    assert_eq!(form(&d), own);
    let (d, diagnostics) = load("[ui]\ntheme = \"light\"\n");
    assert!(diagnostics.is_empty());
    assert_eq!(form(&d), own);
}

#[test]
fn a_users_default_seeds_the_form_and_the_rest_stay() {
    let (d, diagnostics) = load("[defaults.\"gears.spur\"]\nmodule = \"1.5 mm\"\nteeth = \"24\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        form(&d),
        [
            pair("teeth", "24"),
            pair("module", "1.5 mm"),
            pair("pressure_angle", "20 deg")
        ]
    );
    assert!(d.is_overridden("gears.spur", "module"));
    assert!(!d.is_overridden("gears.spur", "pressure_angle"));
}

#[test]
fn settings_leaves_the_defaults_section_alone() {
    let text = "[defaults.\"gears.spur\"]\nmodule = \"1.5 mm\"\n";
    let (_, diagnostics) = Settings::load(&MemorySource::with([(SETTINGS_TOML, text)]));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn each_bad_entry_is_one_diagnostic_on_its_line_and_keeps_the_types_default() {
    let (d, diagnostics) = load(
        "[defaults.\"gears.nope\"]\nx = \"1\"\n\
         [defaults.\"gears.spur\"]\nmodle = \"1 mm\"\nmodule = \"1 mm +\"\n\
         teeth = \"2 mm\"\npressure_angle = 30\n",
    );
    let found: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.diagnostic.code.as_str(), d.line.expect("a line")))
        .collect();
    assert_eq!(
        found,
        [
            ("config.unknown-feature-type", 1),
            ("config.unknown-key", 4),
            ("config.expr.syntax", 5),
            ("config.expr.unit-mismatch", 6),
            ("config.type", 7),
        ]
    );
    assert!(diagnostics[1].diagnostic.message.contains("teeth, module"));
    assert!(diagnostics.iter().all(|d| d.file == SETTINGS_TOML));
    assert_eq!(form(&d)[1], pair("module", "1 mm"));
    assert_eq!(form(&d)[0], pair("teeth", "20"));
}

#[test]
fn a_defaults_that_is_not_a_table_is_one_diagnostic() {
    let (_, diagnostics) = load("defaults = 3\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].diagnostic.code.as_str(), "config.type");
    let (_, diagnostics) = load("[defaults]\n\"gears.spur\" = 3\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
}
