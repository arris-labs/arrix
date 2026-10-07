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

fn palette(h: &Harness<'_, ArrixApp>) -> Option<arrix_ui::PaletteView> {
    h.state().debug_state().shell.expect("a frame ran").palette
}

fn type_text(h: &mut Harness<'_, ArrixApp>, text: &str) {
    h.event(egui::Event::Text(text.to_owned()));
    h.run();
}

fn open_palette(h: &mut Harness<'_, ArrixApp>) {
    h.key_press_modifiers(Modifiers::CTRL, Key::K);
    h.run();
}

#[test]
fn ctrl_k_opens_the_palette_and_escape_closes_it() {
    let mut h = app("");
    assert!(palette(&h).is_none());
    open_palette(&mut h);
    let p = palette(&h).expect("the palette opened");
    assert_eq!(p.query, "");
    assert!(p.results.len() > 1);
    h.key_press(Key::Escape);
    h.run();
    assert!(palette(&h).is_none());
    // Ctrl+K again, in the palette, closes it too.
    open_palette(&mut h);
    open_palette(&mut h);
    assert!(palette(&h).is_none());
}

#[test]
fn typing_searches_and_enter_runs_the_selected_result() {
    let mut h = app("");
    open_palette(&mut h);
    type_text(&mut h, "part tab");
    let p = palette(&h).unwrap();
    assert_eq!(p.query, "part tab");
    let ids: Vec<_> = p.results.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["core.ribbon.part"]);
    assert!(p.results[0].enabled);
    h.key_press(Key::Enter);
    h.run();
    assert!(palette(&h).is_none(), "running a result closes the palette");
    assert_eq!(open_tab(&h), "core.part");
}

#[test]
fn the_arrow_keys_move_the_selection() {
    let mut h = app("");
    open_palette(&mut h);
    type_text(&mut h, "tab");
    assert_eq!(palette(&h).unwrap().selected, 0);
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(palette(&h).unwrap().selected, 1);
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(open_tab(&h), "core.part");
    open_palette(&mut h);
    type_text(&mut h, "tab");
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(palette(&h).unwrap().selected, 0);
}

#[test]
fn a_command_that_does_not_apply_stays_listed_but_disabled() {
    let mut h = app("");
    open_palette(&mut h);
    type_text(&mut h, "spur");
    let p = palette(&h).unwrap();
    assert_eq!(p.results.len(), 1);
    assert!(!p.results[0].enabled, "no document is open");
    h.key_press(Key::Enter);
    h.run();
    assert!(palette(&h).is_some(), "a disabled result does not run");
}

#[test]
fn the_palette_shows_a_commands_effective_keys() {
    let mut h = app("[[bind]]\nkey = \"ctrl+2\"\ncommand = \"core.ribbon.part\"\n");
    open_palette(&mut h);
    type_text(&mut h, "part tab");
    assert_eq!(palette(&h).unwrap().results[0].keys, ["ctrl+2"]);
}

#[test]
fn global_keys_do_not_fire_while_typing_in_the_palette() {
    let mut h = app("[[bind]]\nkey = \"p\"\ncommand = \"core.ribbon.part\"\n");
    open_palette(&mut h);
    h.key_press(Key::P);
    type_text(&mut h, "p");
    assert_eq!(open_tab(&h), "core.sketch");
    assert_eq!(palette(&h).unwrap().query, "p");
}
