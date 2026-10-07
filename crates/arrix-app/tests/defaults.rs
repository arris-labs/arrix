//! A feature parameter's configured default reaches the app as the initial
//! form of a new feature (docs/UI-RENDERING.md §Configuration). Logic only.

use arrix_app::ArrixApp;
use arrix_config::{MemorySource, SETTINGS_TOML};
use egui_kittest::Harness;

fn app(settings: &str) -> Harness<'static, ArrixApp> {
    let source = MemorySource::with([(SETTINGS_TOML, settings)]);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(move |cc| ArrixApp::with_config(cc, &source));
    harness.run();
    harness
}

fn module(h: &Harness<'_, ArrixApp>) -> String {
    let form = h.state().effective_params("gears.spur").expect("a type");
    form.into_iter().find(|(n, _)| n == "module").unwrap().1
}

#[test]
fn a_new_features_form_starts_from_the_users_default() {
    let h = app("[defaults.\"gears.spur\"]\nmodule = \"1.5 mm\"\n");
    assert_eq!(module(&h), "1.5 mm");
    assert!(h.state().debug_state().config_diagnostics.is_empty());
    assert_eq!(h.state().effective_params("gears.nope"), None);
}

#[test]
fn a_bad_default_is_a_diagnostic_and_the_types_own_stays() {
    let h = app("[defaults.\"gears.spur\"]\nmodule = \"1.5 deg\"\n");
    assert_eq!(module(&h), "1 mm");
    let d = h.state().debug_state().config_diagnostics;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].diagnostic.code.as_str(), "config.expr.unit-mismatch");
    assert_eq!((d[0].file.as_str(), d[0].line), (SETTINGS_TOML, Some(2)));
}
