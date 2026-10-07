//! A rebound key runs its command (docs/UI-RENDERING.md §The shell).
//! Logic only, so no GPU adapter is needed.

use arrix_app::ArrixApp;
use arrix_config::{KEYBINDINGS_TOML, MemorySource};
use egui::{Key, Modifiers};
use egui_kittest::Harness;

fn app(keybindings: &str) -> Harness<'static, ArrixApp> {
    let source = MemorySource::with([(KEYBINDINGS_TOML, keybindings)]);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(move |cc| ArrixApp::with_config(cc, &source));
    harness.run();
    harness
}

fn open_tab(h: &Harness<'_, ArrixApp>) -> String {
    h.state()
        .debug_state()
        .shell
        .expect("a frame ran")
        .ribbon
        .active
}

const REBIND: &str = "[[bind]]\nkey = \"ctrl+2\"\ncommand = \"core.ribbon.part\"\n";

#[test]
fn a_rebound_key_runs_its_command() {
    let mut h = app(REBIND);
    assert_eq!(open_tab(&h), "core.sketch");
    h.key_press_modifiers(Modifiers::CTRL, Key::Num2);
    h.run();
    assert_eq!(open_tab(&h), "core.part");
}

#[test]
fn an_unbound_key_does_nothing_and_the_other_modifier_set_is_another_key() {
    let mut h = app(REBIND);
    h.key_press(Key::Num2);
    h.run();
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::Num2);
    h.run();
    assert_eq!(open_tab(&h), "core.sketch");
}

#[test]
fn a_broken_binding_is_a_diagnostic_and_the_app_runs() {
    let h = app("[[bind]]\nkey = \"ctrl+2\"\ncommand = \"core.nope\"\n");
    let state = h.state().debug_state();
    assert_eq!(state.config_diagnostics.len(), 1);
    assert_eq!(state.config_diagnostics[0].file, KEYBINDINGS_TOML);
    assert_eq!(state.config_diagnostics[0].line, Some(3));
    assert!(state.shell.is_some());
}

/// The `text` context (a focused text field keeps its keys) is held by
/// `arrix-config`'s keymap tests; the shell has no text field until the
/// palette (plans/configurable-shell step 5) adds one to press against.
#[test]
fn a_plain_key_fires_while_no_text_field_has_focus() {
    let mut h = app("[[bind]]\nkey = \"a\"\ncommand = \"core.ribbon.part\"\n");
    h.key_press(Key::A);
    h.run();
    assert_eq!(open_tab(&h), "core.part");
}
