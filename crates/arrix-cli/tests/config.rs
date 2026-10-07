//! The user's configuration against the real expression checker and the
//! compiled-in plugins (docs/UI-RENDERING.md §Configuration): a default
//! for a plugin's parameter, and that config never reaches `evaluate`.

mod common;

use arrix_config::{MemorySource, ParamDefaults, SETTINGS_TOML};
use arrix_doc::expr::check_constant;
use common::{registry, workspace};

const MODULE_SET: &str = "[defaults.\"gears.spur\"]\nmodule = \"1.5 mm\"\n";

fn types() -> Vec<arrix_plugin_api::FeatureTypeSpec> {
    let registry = registry();
    registry
        .ids()
        .filter_map(|id| registry.get(id))
        .map(|t| t.spec().clone())
        .collect()
}

fn defaults(text: &str) -> (ParamDefaults, Vec<arrix_config::ConfigDiagnostic>) {
    ParamDefaults::load(
        &MemorySource::with([(SETTINGS_TOML, text)]),
        &types(),
        &check_constant,
    )
}

fn spur() -> arrix_plugin_api::FeatureTypeSpec {
    types().into_iter().find(|t| t.id == "gears.spur").unwrap()
}

#[test]
fn the_real_checker_accepts_a_length_and_names_what_is_wrong_with_the_rest() {
    let (d, diagnostics) = defaults(MODULE_SET);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let form = d.effective_params(&spur());
    assert!(form.contains(&("module".to_owned(), "1.5 mm".to_owned())));
    assert!(form.contains(&("teeth".to_owned(), "20".to_owned())));

    let (_, diagnostics) = defaults(
        "[defaults.\"gears.spur\"]\nmodule = \"1.5 +\"\nwidth = \"3 deg\"\nteeth = \"2.5\"\n\
         pressure_angle = \"module\"\n",
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.diagnostic.code.as_str(), d.line))
        .collect();
    assert_eq!(
        codes,
        [
            ("config.expr.syntax", Some(2)),
            ("config.expr.unit-mismatch", Some(3)),
            ("config.expr.not-whole", Some(4)),
            ("config.expr.unknown-name", Some(5)),
        ]
    );
}

/// A parameter default only seeds a new feature's form. A document's
/// evaluation takes no config, and a type's own spec, which `evaluate`'s
/// inputs are checked against, is the same with or without it.
#[test]
fn no_feature_evaluates_differently_under_a_config() {
    let eval = || {
        let mut files = std::collections::BTreeMap::new();
        common::read_tree(
            &workspace().join("tests/docs/gear-on-plane"),
            "",
            &mut files,
        );
        let source = arrix_doc::MemorySource(files);
        let run = arrix_doc::run("gear-on-plane", &source, &registry()).unwrap();
        serde_json::to_string(&run.line).unwrap()
    };
    let before = eval();
    let (d, diagnostics) = defaults(MODULE_SET);
    assert!(diagnostics.is_empty());
    assert!(d.is_overridden("gears.spur", "module"));
    assert_eq!(eval(), before);
    let own = spur();
    assert_eq!(
        own.params
            .iter()
            .find(|p| p.name == "module")
            .unwrap()
            .default,
        "1 mm"
    );
}
