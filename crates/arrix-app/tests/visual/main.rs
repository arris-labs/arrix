//! The visual scenarios (docs/UI-RENDERING.md §Visual debugging).

mod accept;
mod camera;
mod coarse;
mod export;
mod fixture;
mod form_diagnostics;
mod form_pick;
mod forms_accept;
mod frozen_badge;
mod harness;
mod highlight;
mod m3_accept;
mod preview;
mod render_cli;
mod render_parity;
mod sketch;
mod sketch_accept;
mod snapshot;
mod viewport;

/// The shell at startup: no document content, the Sketch tab open.
#[test]
fn empty_shell() {
    harness::with_app("empty_shell", |h| snapshot::check(h, "empty_shell"));
}

/// Frames are a function of the app's state alone. Established on lavapipe
/// in plans/c1-m0 step 4: identical pixels from two app instances, from a
/// re-render after further frames, across processes, and with 1, 4 or all
/// of lavapipe's threads. So the coarse frame's tolerance only has to absorb
/// a different Mesa version, not run-to-run noise.
#[test]
fn empty_shell_renders_identically() {
    let mut first = None;
    harness::with_app("empty_shell_renders_identically", |h| {
        let a = harness::render(h);
        h.run_steps(10);
        let again = harness::render(h);
        assert!(a == again, "re-rendering the same state changed pixels");
        first = Some((a, h.state().debug_state_json()));
    });
    harness::with_app("empty_shell_renders_identically", |h| {
        let (a, state) = first.take().expect("first instance ran");
        assert!(
            harness::render(h) == a,
            "a second app instance rendered different pixels"
        );
        assert_eq!(h.state().debug_state_json(), state);
    });
}

/// Two docked bars and the viewport tile the window: the ribbon on top, the
/// tab strip over the status line at the bottom, the viewport between them,
/// and no card on an empty start (ADR-0017).
#[test]
fn empty_shell_tiles_the_window() {
    harness::with_app("empty_shell_tiles_the_window", |h| {
        let state = h.state().debug_state();
        let [w, hgt] = state.window.expect("a frame ran");
        let l = state.layout.expect("a frame ran");
        assert_eq!(
            [w, hgt],
            [f64::from(harness::SIZE.x), f64::from(harness::SIZE.y)]
        );
        assert_eq!(l.ribbon, [0.0, 0.0, w, l.viewport[1]]);
        assert_eq!(l.viewport, [0.0, l.ribbon[3], w, l.tabs[1]]);
        assert_eq!(l.tabs, [0.0, l.viewport[3], w, l.status[1]]);
        assert_eq!(l.status, [0.0, l.tabs[3], w, hgt]);
        assert_eq!(
            (l.tree, l.properties),
            (None, None),
            "no card on the empty start"
        );
        let shell = state.shell.expect("a frame ran");
        assert!(shell.documents.tabs.is_empty());
        assert!(shell.status.hint.contains("opens the command palette"));
    });
}

/// A pointer over a ribbon button puts its title, keys and what it needs in
/// the status line, a disabled button's included.
#[test]
fn the_status_line_helps_with_what_is_hovered() {
    use egui_kittest::kittest::Queryable as _;
    harness::with_app("the_status_line_helps_with_what_is_hovered", |h| {
        h.get_by_label("New sketch").hover();
        h.run();
        let hint = h
            .state()
            .debug_state()
            .shell
            .expect("a frame ran")
            .status
            .hint;
        assert_eq!(hint, "New sketch: needs an open document");
    });
}

/// Both cards toggled on, by the commands' keys: the tree over the viewport's
/// left, the properties over its right, each inside it, and toggled off again.
#[test]
fn cards_float_over_the_viewport() {
    let source = arrix_config::MemorySource::with([(
        "keybindings.toml",
        "[[bind]]\nkey = \"f4\"\ncommand = \"core.panel.tree.toggle\"\n\
         [[bind]]\nkey = \"f5\"\ncommand = \"core.panel.properties.toggle\"\n",
    )]);
    harness::with_config("cards_float_over_the_viewport", &source, |h| {
        h.key_press(egui::Key::F4);
        h.run();
        h.key_press(egui::Key::F5);
        h.run();
        let l = h.state().debug_state().layout.expect("a frame ran");
        let (tree, properties) = (
            l.tree.expect("tree card"),
            l.properties.expect("property card"),
        );
        let v = l.viewport;
        assert!(tree[0] > v[0] && tree[1] > v[1] && tree[2] < properties[0]);
        assert!(properties[2] < v[2] && properties[3] < v[3] && tree[3] < v[3]);
        snapshot::check(h, "cards_shown");
        h.key_press(egui::Key::F4);
        h.run();
        let l = h.state().debug_state().layout.expect("a frame ran");
        assert_eq!(l.tree, None);
        assert!(l.properties.is_some());
    });
}

/// `ui.panel.*` decide a card without a toggle, and a toggle wins over them.
#[test]
fn the_panel_settings_decide_the_cards() {
    let source = arrix_config::MemorySource::with([(
        "settings.toml",
        "[ui.panel]\ntree = \"shown\"\nproperties = \"hidden\"\n",
    )]);
    harness::with_config("the_panel_settings_decide_the_cards", &source, |h| {
        let l = h.state().debug_state().layout.expect("a frame ran");
        assert!(l.tree.is_some());
        assert_eq!(l.properties, None);
    });
}

/// The command palette open on a query: its field, its results with their
/// keys, and a disabled result.
#[test]
fn palette_open() {
    harness::with_app("palette_open", |h| {
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::K);
        h.run();
        h.event(egui::Event::Text("new".to_owned()));
        h.run();
        snapshot::check(h, "palette_open");
    });
}

/// The built-in light theme: the same shell in the other look.
#[test]
fn light_theme() {
    let source = arrix_config::MemorySource::with([("settings.toml", "[ui]\ntheme = \"light\"\n")]);
    harness::with_config("light_theme", &source, |h| {
        snapshot::check(h, "light_theme")
    });
}

/// Copies the files of `from` over `to`, directories included.
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("create a directory");
    for entry in std::fs::read_dir(from).expect("read a fixture directory") {
        let entry = entry.expect("a directory entry");
        let at = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_tree(&entry.path(), &at);
        } else {
            std::fs::copy(entry.path(), at).expect("copy a fixture");
        }
    }
}

/// The acceptance of plans/configurable-shell: the app starts on an empty
/// config directory, the user's files (`tests/config/user`) arrive and
/// `core.config.reload` applies them, and then one wrong key leaves the app
/// up with that key on its default and one diagnostic.
#[test]
fn configured_shell() {
    let config = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/config");
    let temp = std::env::temp_dir().join(format!("arrix-configured-shell-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create the config directory");
    // The reload command is on a key the user's files don't move.
    std::fs::write(
        temp.join("keybindings.toml"),
        "[[bind]]\nkey = \"ctrl+r\"\ncommand = \"core.config.reload\"\n",
    )
    .expect("write the starting keymap");
    harness::with_config_dir("configured_shell", &temp, |h| {
        assert_eq!(h.state().debug_state().theme, "dark");
        copy_tree(&config.join("user"), &temp);
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::R);
        h.run();
        let d = h.state().debug_state();
        assert_eq!(d.theme, "mine");
        assert!(
            d.config_diagnostics.is_empty(),
            "{:?}",
            d.config_diagnostics
        );
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::K);
        h.run();
        assert!(h.state().debug_state().shell.unwrap().palette.is_none());
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::P);
        h.run();
        snapshot::check(h, "configured_shell");

        // One wrong key, then a reload.
        std::fs::copy(
            config.join("user-broken/settings.toml"),
            temp.join("settings.toml"),
        )
        .expect("write the broken settings");
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().debug_state().shell.unwrap().palette.is_none());
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::R);
        h.run();
        let d = h.state().debug_state();
        assert_eq!(d.theme, "dark", "the broken key has its default");
        assert_eq!(d.config_diagnostics.len(), 1);
        assert_eq!(d.config_diagnostics[0].file, "settings.toml");
        assert_eq!(d.config_diagnostics[0].line, Some(4));
        let shell = d.shell.expect("a frame ran");
        assert_eq!(shell.status.diagnostics, 1);
        assert!(!shell.ribbon.tabs.iter().any(|t| t.id == "core.part"));
    });
    let _ = std::fs::remove_dir_all(&temp);
}
