//! `core.config.reload` and the poll of the config directory
//! (docs/UI-RENDERING.md §Configuration). Logic only: against a temporary
//! directory, with the real files.

use std::path::{Path, PathBuf};

use arrix_app::{ArrixApp, config_files::ConfigDir};
use arrix_config::{KEYBINDINGS_TOML, SETTINGS_TOML};
use egui::{Key, Modifiers};
use egui_kittest::Harness;

const RELOAD_KEY: &str = "[[bind]]\nkey = \"ctrl+r\"\ncommand = \"core.config.reload\"\n";

struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("arrix-reload-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let at = self.0.join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn app(dir: &Path) -> Harness<'static, ArrixApp> {
    let dir = dir.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(move |cc| ArrixApp::with_config_source(cc, Box::new(ConfigDir::new(dir))));
    harness.run();
    harness
}

fn shown_tabs(h: &Harness<'_, ArrixApp>) -> Vec<String> {
    let shell = h.state().debug_state().shell.expect("a frame was drawn");
    shell.ribbon.tabs.iter().map(|t| t.id.clone()).collect()
}

fn reload(h: &mut Harness<'_, ArrixApp>) {
    h.key_press_modifiers(Modifiers::CTRL, Key::R);
    h.run();
}

#[test]
fn reload_applies_an_edit_and_a_broken_key_keeps_its_default() {
    let temp = Temp::new("edit");
    temp.write(KEYBINDINGS_TOML, RELOAD_KEY);
    let mut h = app(&temp.0);
    assert!(shown_tabs(&h).contains(&"core.part".to_owned()));
    assert!(
        h.state()
            .debug_state()
            .keymap
            .iter()
            .any(|b| b.command == "core.config.reload"),
        "the file's key is in force"
    );

    temp.write(
        SETTINGS_TOML,
        "[ui.ribbon]\nhidden-tabs = [\"core.part\"]\n",
    );
    reload(&mut h);
    assert!(!shown_tabs(&h).contains(&"core.part".to_owned()));
    assert!(h.state().debug_state().config_diagnostics.is_empty());

    temp.write(SETTINGS_TOML, "[ui]\ntheme = 3\n");
    reload(&mut h);
    let d = h.state().debug_state();
    assert_eq!(d.theme, "dark");
    assert_eq!(d.config_diagnostics.len(), 1);
    assert_eq!(d.config_diagnostics[0].line, Some(2));
    assert!(shown_tabs(&h).contains(&"core.part".to_owned()));
    let status = d.shell.unwrap().status;
    assert_eq!(status.diagnostics, 1, "the status bar counts config ones");
}

#[test]
fn an_unreadable_file_is_the_defaults_and_one_diagnostic() {
    let temp = Temp::new("unreadable");
    // A directory where the file should be: reading it fails, not "missing".
    std::fs::create_dir(temp.0.join(SETTINGS_TOML)).unwrap();
    let h = app(&temp.0);
    let d = h.state().debug_state();
    assert_eq!(d.theme, "dark");
    assert_eq!(d.config_diagnostics.len(), 1);
    assert_eq!(
        d.config_diagnostics[0].diagnostic.code.as_str(),
        "config.io"
    );
    assert_eq!(d.config_diagnostics[0].file, SETTINGS_TOML);
}

#[test]
fn a_changed_file_is_noticed_by_the_poll() {
    let temp = Temp::new("poll");
    let mut h = app(&temp.0);
    temp.write(SETTINGS_TOML, "[ui]\ntheme = \"light\"\n");
    // The poll compares about once a second of egui's clock.
    for _ in 0..40 {
        h.step();
        if h.state().debug_state().theme == "light" {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    panic!("the edit was never noticed");
}
