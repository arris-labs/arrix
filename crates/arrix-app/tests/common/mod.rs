//! What the sketch-mode tests share: a document with a sketch open, the
//! pointer in plane millimetres, and what the app says it holds.
#![allow(dead_code, reason = "each test file uses what it needs")]

use arrix_app::ArrixApp;
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable as _};

pub const MM: f64 = 1e-3;

/// An empty document with a new sketch on world XY open in sketch mode.
pub fn sketching() -> Harness<'static, ArrixApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_step_dt(0.02)
        .build_eframe(|cc| ArrixApp::new(cc));
    h.run();
    h.state_mut().seed_ids(11);
    h.key_press_modifiers(Modifiers::CTRL, Key::N);
    settle(&mut h);
    h.get_by_label("New sketch").click();
    h.run_steps(3);
    wait(&mut h, "the camera turned", |h| {
        h.state().debug_state().sketch.is_some_and(|m| m.turned)
    });
    h.state_mut().skip_animations();
    h.run_steps(3);
    h
}

/// Steps until `done`, a minute at most: evaluations arrive on the
/// session's own threads.
pub fn wait(
    h: &mut Harness<'_, ArrixApp>,
    what: &str,
    done: impl Fn(&Harness<'_, ArrixApp>) -> bool,
) {
    let started = web_time::Instant::now();
    while !done(h) {
        h.step();
        assert!(started.elapsed().as_secs() < 60, "{what}: timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub fn settle(h: &mut Harness<'_, ArrixApp>) {
    wait(h, "the document settled", |h| {
        h.state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.settled)
    });
    h.run();
}

pub fn at(h: &Harness<'_, ArrixApp>, uv: [f64; 2]) -> egui::Pos2 {
    h.state()
        .sketch_point([uv[0] * MM, uv[1] * MM])
        .expect("the sketch is drawn")
}

/// Moves the pointer to `uv` (millimetres) and lets a few frames pass.
pub fn hover(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(3);
}

pub fn click(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

pub fn gesture(h: &Harness<'_, ArrixApp>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

pub fn preview(h: &Harness<'_, ArrixApp>) -> arrix_ui::SketchPreviewView {
    h.state()
        .debug_state()
        .sketch_view
        .and_then(|v| v.preview)
        .expect("a drawing tool's preview")
}

/// What the preview's glyphs commit, sorted.
pub fn shown(h: &Harness<'_, ArrixApp>) -> Vec<String> {
    let mut kinds: Vec<_> = preview(h)
        .glyphs
        .into_iter()
        .flat_map(|g| g.commits)
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// The stored sketch: its records' counts and its constraints' kinds,
/// sorted.
pub struct Stored {
    pub points: usize,
    pub lines: usize,
    pub constraints: Vec<String>,
}

pub fn stored(h: &Harness<'_, ArrixApp>) -> Stored {
    let saved = h.state().saved().expect("a document is open").0;
    let sketch = saved
        .iter()
        .filter(|(path, _)| path.starts_with("parts/"))
        .find_map(|(_, bytes)| {
            let part: serde_json::Value = serde_json::from_slice(bytes).ok()?;
            let features = part["features"].as_object()?;
            features.values().find_map(|f| f.get("sketch").cloned())
        })
        .expect("a sketch is stored");
    let count = |key: &str| sketch[key].as_object().map_or(0, |o| o.len());
    let mut constraints: Vec<String> = sketch["constraints"]
        .as_object()
        .map(|o| {
            o.values()
                .filter_map(|r| r["constraint"].as_object()?.keys().next().cloned())
                .collect()
        })
        .unwrap_or_default();
    constraints.sort();
    Stored {
        points: count("points"),
        lines: count("entities"),
        constraints,
    }
}

/// Waits until the overlay draws `curves` curves: the edit's evaluation
/// is in.
pub fn drawn(h: &mut Harness<'_, ArrixApp>, curves: usize) {
    wait(h, "the edit was drawn", |h| {
        h.state()
            .debug_state()
            .sketch_view
            .is_some_and(|v| v.curves.len() == curves)
    });
}

pub fn tool(h: &Harness<'_, ArrixApp>) -> Option<String> {
    h.state()
        .debug_state()
        .shell?
        .ribbon
        .groups
        .iter()
        .flat_map(|g| &g.commands)
        .find(|c| c.active)
        .map(|c| c.id.clone())
}
pub fn press(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

pub fn release(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.run_steps(3);
}

/// A rectangle (20,15)-(50,35) drawn, then the select tool.
pub fn with_rectangle() -> Harness<'static, ArrixApp> {
    let mut h = sketching();
    h.key_press(Key::R);
    h.run_steps(2);
    click(&mut h, [20.0, 15.0]);
    click(&mut h, [50.0, 35.0]);
    drawn(&mut h, 4);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.select"));
    h
}

pub fn point_at(h: &Harness<'_, ArrixApp>, uv: [f64; 2]) -> Option<[f64; 2]> {
    let (u, v) = (uv[0] * MM, uv[1] * MM);
    h.state()
        .debug_state()
        .sketch_view?
        .points
        .iter()
        .find(|p| (p.at[0] - u).abs() < 1e-6 && (p.at[1] - v).abs() < 1e-6)
        .map(|p| p.at)
}

/// A click with `modifiers` held, as the select tool takes it.
pub fn click_with(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2], modifiers: Modifiers) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
        h.step();
    }
    h.run_steps(3);
}

/// Whether the ribbon shows the command enabled, `None` when it is not in
/// the open tab.
pub fn enabled(h: &Harness<'_, ArrixApp>, command: &str) -> Option<bool> {
    h.state()
        .debug_state()
        .shell?
        .ribbon
        .groups
        .iter()
        .flat_map(|g| &g.commands)
        .find(|c| c.id == command)
        .map(|c| c.enabled)
}

/// The status line's words.
pub fn hint(h: &Harness<'_, ArrixApp>) -> String {
    h.state()
        .debug_state()
        .shell
        .map(|s| s.status.hint)
        .unwrap_or_default()
}

/// Two free lines, `(10,10)-(40,20)` and `(15,40)-(45,60)`, drawn and the
/// select tool back.
pub fn with_two_lines() -> Harness<'static, ArrixApp> {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [10.0, 10.0]);
    click(&mut h, [40.0, 20.0]);
    drawn(&mut h, 1);
    h.key_press(Key::Escape);
    h.run_steps(2);
    click(&mut h, [15.0, 40.0]);
    click(&mut h, [45.0, 60.0]);
    drawn(&mut h, 2);
    for _ in 0..2 {
        h.key_press(Key::Escape);
        h.run_steps(2);
    }
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.select"));
    h
}

/// Runs the command titled `title` through the palette: the way to a
/// button the ribbon has scrolled out of sight.
pub fn run_command(h: &mut Harness<'_, ArrixApp>, title: &str) {
    h.key_press_modifiers(Modifiers::CTRL, Key::K);
    h.run_steps(2);
    h.event(egui::Event::Text(title.to_owned()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}
