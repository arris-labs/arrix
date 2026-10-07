//! The line and rectangle tools with inference (plans/sketch-mode step 7,
//! docs/UI-RENDERING.md §Sketch mode): what the preview shows is what the
//! click commits, by the commit rule, and a click that adds geometry is one
//! command. Logic only, driven by keys and the pointer.

use arrix_app::ArrixApp;
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable as _};

const MM: f64 = 1e-3;

/// An empty document with a new sketch on world XY open in sketch mode.
fn sketching() -> Harness<'static, ArrixApp> {
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
fn wait(h: &mut Harness<'_, ArrixApp>, what: &str, done: impl Fn(&Harness<'_, ArrixApp>) -> bool) {
    let started = web_time::Instant::now();
    while !done(h) {
        h.step();
        assert!(started.elapsed().as_secs() < 60, "{what}: timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn settle(h: &mut Harness<'_, ArrixApp>) {
    wait(h, "the document settled", |h| {
        h.state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.settled)
    });
    h.run();
}

fn at(h: &Harness<'_, ArrixApp>, uv: [f64; 2]) -> egui::Pos2 {
    h.state()
        .sketch_point([uv[0] * MM, uv[1] * MM])
        .expect("the sketch is drawn")
}

/// Moves the pointer to `uv` (millimetres) and lets a few frames pass.
fn hover(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(3);
}

fn click(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
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

fn gesture(h: &Harness<'_, ArrixApp>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn preview(h: &Harness<'_, ArrixApp>) -> arrix_ui::SketchPreviewView {
    h.state()
        .debug_state()
        .sketch_view
        .and_then(|v| v.preview)
        .expect("a drawing tool's preview")
}

/// What the preview's glyphs commit, sorted.
fn shown(h: &Harness<'_, ArrixApp>) -> Vec<String> {
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
struct Stored {
    points: usize,
    lines: usize,
    constraints: Vec<String>,
}

fn stored(h: &Harness<'_, ArrixApp>) -> Stored {
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
fn drawn(h: &mut Harness<'_, ArrixApp>, curves: usize) {
    wait(h, "the edit was drawn", |h| {
        h.state()
            .debug_state()
            .sketch_view
            .is_some_and(|v| v.curves.len() == curves)
    });
}

fn tool(h: &Harness<'_, ArrixApp>) -> Option<String> {
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

#[test]
fn a_segment_is_one_command_holding_what_its_preview_showed() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.line"));
    click(&mut h, [10.0, 10.0]);
    assert_eq!(gesture(&h), Some(0), "the start is not geometry yet");
    // Nearly level: the H lock snaps it level.
    hover(&mut h, [40.0, 10.4]);
    let p = preview(&h);
    assert_eq!(p.cursor[1], 0.01, "snapped level with the start");
    assert!((p.cursor[0] - 0.04).abs() < 1e-5, "{:?}", p.cursor);
    assert_eq!(p.curves.len(), 1, "the pending segment");
    assert_eq!(shown(&h), ["horizontal"]);
    click(&mut h, [40.0, 10.4]);
    assert_eq!(gesture(&h), Some(1), "one command for the segment");
    let s = stored(&h);
    assert_eq!((s.points, s.lines), (2, 1));
    assert_eq!(s.constraints, ["horizontal"], "glyph for glyph");
    drawn(&mut h, 1);
}

/// From the origin along the X axis: the H lock and the axis both show,
/// and the axis, which the lock and the origin already imply, is drawn
/// and not committed.
#[test]
fn a_redundant_inference_is_drawn_and_not_committed() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    hover(&mut h, [0.3, -0.2]);
    assert_eq!(shown(&h), ["coincident_to_datum"]);
    click(&mut h, [0.3, -0.2]);
    hover(&mut h, [30.0, 0.4]);
    assert_eq!(shown(&h), ["horizontal", "point_on_datum"]);
    click(&mut h, [30.0, 0.4]);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(
        stored(&h).constraints,
        ["coincident_to_datum", "horizontal"],
        "the axis is redundant"
    );
}

#[test]
fn a_chain_goes_on_until_a_double_click_or_escape() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [10.0, 10.0]);
    click(&mut h, [40.0, 20.0]);
    assert_eq!(gesture(&h), Some(1));
    drawn(&mut h, 1);
    // The next segment starts where the last ended: one new point.
    click(&mut h, [50.0, 40.0]);
    assert_eq!(gesture(&h), Some(1));
    drawn(&mut h, 2);
    assert_eq!(stored(&h).points, 3);
    // A second click on the chain's end, as a double-click makes, ends it
    // and draws nothing.
    click(&mut h, [50.0, 40.0]);
    assert_eq!(gesture(&h), Some(0));
    hover(&mut h, [70.0, 45.0]);
    assert!(preview(&h).curves.is_empty(), "nothing is pending");

    // Escape ends a chain, then leaves the tool, then the sketch.
    click(&mut h, [-10.0, -10.0]);
    hover(&mut h, [-30.0, -15.0]);
    assert_eq!(preview(&h).curves.len(), 1);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(0));
    assert!(preview(&h).curves.is_empty());
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.line"));
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.select"));
    assert!(h.state().debug_state().sketch.is_some());
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().debug_state().sketch.is_none(), "finished");
}

#[test]
fn a_chain_closed_on_its_first_point_ends_there() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    for (uv, curves) in [([10.0, 10.0], 0), ([40.0, 10.0], 1), ([40.0, 30.0], 2)] {
        click(&mut h, uv);
        if curves > 0 {
            drawn(&mut h, curves);
        }
    }
    hover(&mut h, [10.3, 10.2]);
    assert!(
        shown(&h).contains(&"coincident".to_owned()),
        "{:?}",
        shown(&h)
    );
    click(&mut h, [10.3, 10.2]);
    assert_eq!(gesture(&h), Some(1));
    drawn(&mut h, 3);
    hover(&mut h, [60.0, 60.0]);
    assert!(preview(&h).curves.is_empty(), "the chain closed and ended");
    assert!(stored(&h).constraints.contains(&"coincident".to_owned()));
}

#[test]
fn a_rectangle_is_one_command_and_undo_takes_it_back_byte_for_byte() {
    let mut h = sketching();
    let before = h.state().saved().unwrap().0;
    h.key_press(Key::R);
    h.run_steps(2);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.rectangle"));
    click(&mut h, [20.0, 15.0]);
    assert_eq!(gesture(&h), Some(0));
    hover(&mut h, [50.0, 35.0]);
    assert_eq!(preview(&h).curves.len(), 4, "the four sides");
    click(&mut h, [50.0, 35.0]);
    assert_eq!(gesture(&h), Some(1));
    let s = stored(&h);
    assert_eq!((s.points, s.lines), (4, 4), "the corners are shared");
    assert_eq!(
        s.constraints,
        ["horizontal", "horizontal", "vertical", "vertical"]
    );
    drawn(&mut h, 4);

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(h.state().saved().unwrap().0, before, "byte for byte");
    drawn(&mut h, 0);
}

#[test]
fn a_rectangle_snapped_to_the_origin_is_pinned_there() {
    let mut h = sketching();
    h.key_press(Key::R);
    h.run_steps(2);
    click(&mut h, [0.2, 0.3]);
    hover(&mut h, [-30.0, -20.0]);
    click(&mut h, [-30.0, -20.0]);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(
        stored(&h).constraints,
        [
            "coincident_to_datum",
            "horizontal",
            "horizontal",
            "vertical",
            "vertical"
        ]
    );
    drawn(&mut h, 4);
    let view = h.state().debug_state().sketch_view.unwrap();
    assert!(view.points.iter().any(|p| p.at == [0.0, 0.0]));
}

/// A point acquired by a dwell is an alignment source: far from it, level
/// with it, the cursor shows a guide and commits `horizontal_points`.
#[test]
fn a_dwell_acquires_a_point_as_an_alignment_source() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [10.0, 10.0]);
    click(&mut h, [30.0, 30.0]);
    drawn(&mut h, 1);
    // The tool again, afresh: what the draw acquired is let go.
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.key_press(Key::L);
    h.run_steps(2);
    // Off the end, level with it: not acquired, so no guide.
    hover(&mut h, [-40.0, 30.3]);
    assert!(preview(&h).guides.is_empty());
    // The pointer rests on the far end of the first segment past the dwell.
    hover(&mut h, [10.0, 10.0]);
    h.run_steps(25);
    hover(&mut h, [-40.0, 10.3]);
    let p = preview(&h);
    assert_eq!(p.guides.len(), 1, "{p:?}");
    assert_eq!(shown(&h), ["horizontal_points"]);
    assert_eq!(p.cursor[1], 0.01, "snapped level");
}

#[test]
fn alt_suppresses_inference() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [10.0, 10.0]);
    hover(&mut h, [40.0, 10.4]);
    assert_eq!(shown(&h), ["horizontal"]);
    h.event(egui::Event::ModifiersChanged(Modifiers::ALT));
    hover(&mut h, [40.0, 10.4]);
    assert!(shown(&h).is_empty(), "{:?}", preview(&h));
    assert!((preview(&h).cursor[1] - 0.0104).abs() < 1e-4, "not snapped");
}

/// Whether two angles are one direction, a turn apart or not.
fn same_angle(a: f64, b: f64) -> bool {
    let turn = std::f64::consts::TAU;
    ((a - b) / turn).round().mul_add(-turn, a - b).abs() < 1e-6
}

/// A circle is two clicks: its centre, then a point on the rim. The
/// preview is the circle, and a point the rim passes through is bound.
#[test]
fn a_circle_is_one_command_holding_what_its_preview_showed() {
    let mut h = sketching();
    h.key_press(Key::C);
    h.run_steps(2);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.circle"));
    click(&mut h, [20.0, 20.0]);
    assert_eq!(gesture(&h), Some(0), "the centre is not geometry yet");
    hover(&mut h, [35.0, 20.0]);
    let p = preview(&h);
    match p.curves.as_slice() {
        [arrix_ui::CurveShape::Circle { centre, radius }] => {
            assert!((centre[0] - 0.02).abs() < 1e-6 && (centre[1] - 0.02).abs() < 1e-6);
            assert!((radius - 0.015).abs() < 1e-6, "{radius}");
        }
        other => panic!("{other:?}"),
    }
    assert!(p.glyphs.is_empty(), "free rim: nothing to bind");
    click(&mut h, [35.0, 20.0]);
    assert_eq!(gesture(&h), Some(1));
    let s = stored(&h);
    assert_eq!((s.points, s.lines), (1, 1));
    assert!(s.constraints.is_empty(), "{:?}", s.constraints);
    drawn(&mut h, 1);
}

/// A centre on the origin is pinned there; a rim click on an existing point
/// makes the circle pass through it, as the glyph said.
#[test]
fn a_circle_binds_its_centre_and_a_point_on_its_rim() {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [30.0, 40.0]);
    click(&mut h, [50.0, 60.0]);
    drawn(&mut h, 1);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.key_press(Key::C);
    h.run_steps(2);
    hover(&mut h, [0.2, 0.1]);
    assert_eq!(shown(&h), ["coincident_to_datum"]);
    click(&mut h, [0.2, 0.1]);
    hover(&mut h, [30.2, 40.1]);
    assert_eq!(shown(&h), ["point_on_circle"], "{:?}", preview(&h));
    click(&mut h, [30.2, 40.1]);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(
        stored(&h).constraints,
        ["coincident_to_datum", "point_on_circle"]
    );
    drawn(&mut h, 2);
}

/// Three points make an arc: the first two its ends, the third a point it
/// passes through, and the arc runs counter-clockwise from its start.
#[test]
fn an_arc_is_three_clicks_and_one_command() {
    let mut h = sketching();
    h.key_press(Key::A);
    h.run_steps(2);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.arc"));
    click(&mut h, [10.0, 0.0]);
    hover(&mut h, [-10.0, 0.0]);
    assert_eq!(preview(&h).curves.len(), 1, "the chord");
    click(&mut h, [-10.0, 0.0]);
    assert_eq!(gesture(&h), Some(0));
    hover(&mut h, [0.0, 10.0]);
    match preview(&h).curves.as_slice() {
        [arrix_ui::CurveShape::Arc { centre, radius, .. }] => {
            assert!(centre[0].abs() < 1e-6 && centre[1].abs() < 1e-6);
            assert!((radius - 0.01).abs() < 1e-6);
        }
        other => panic!("{other:?}"),
    }
    click(&mut h, [0.0, 10.0]);
    assert_eq!(gesture(&h), Some(1));
    let s = stored(&h);
    assert_eq!((s.points, s.lines), (3, 1), "two ends and a centre");
    drawn(&mut h, 1);
    let view = h.state().debug_state().sketch_view.unwrap();
    match &view.curves[0].shape {
        // Through the top: from the right end counter-clockwise to the left.
        arrix_ui::CurveShape::Arc { start, end, .. } => {
            assert!(same_angle(*start, 0.0), "{start}");
            assert!(same_angle(*end, std::f64::consts::PI), "{end}");
        }
        other => panic!("{other:?}"),
    }
}

/// Through the bottom instead: the same ends, and the arc starts at the
/// other one.
#[test]
fn an_arc_through_the_other_side_starts_at_its_other_end() {
    let mut h = sketching();
    h.key_press(Key::A);
    h.run_steps(2);
    for uv in [[10.0, 0.0], [-10.0, 0.0], [0.0, -10.0]] {
        click(&mut h, uv);
    }
    assert_eq!(gesture(&h), Some(1));
    drawn(&mut h, 1);
    let view = h.state().debug_state().sketch_view.unwrap();
    match &view.curves[0].shape {
        arrix_ui::CurveShape::Arc { start, end, .. } => {
            assert!(same_angle(*start, std::f64::consts::PI), "{start}");
            assert!(same_angle(*end, 0.0), "{end}");
        }
        other => panic!("{other:?}"),
    }
}

/// Three points on a line make no arc: the click is refused and nothing is
/// drawn.
#[test]
fn collinear_arc_points_draw_nothing() {
    let mut h = sketching();
    h.key_press(Key::A);
    h.run_steps(2);
    for uv in [[10.0, 0.0], [-10.0, 0.0], [0.0, 0.0]] {
        click(&mut h, uv);
    }
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(stored(&h).lines, 0);
}

/// A line from outside a circle that ends where the snap showed a tangent
/// commits `Tangent` alone: the end is on the rim, and not also a point on
/// it.
#[test]
fn a_tangent_snap_from_a_lines_end_commits_tangent_alone() {
    let mut h = sketching();
    h.key_press(Key::C);
    h.run_steps(2);
    click(&mut h, [10.0, 5.0]);
    click(&mut h, [30.0, 5.0]);
    drawn(&mut h, 1);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [60.0, 30.0]);
    // The tangent point from (60, 30) on the circle of radius 20.
    let (away_u, away_v): (f64, f64) = (60.0 - 10.0, 30.0 - 5.0);
    let d = away_u.hypot(away_v);
    let base = away_v.atan2(away_u);
    let half = (20.0 / d).acos();
    let t = [
        10.0 + 20.0 * (base + half).cos(),
        5.0 + 20.0 * (base + half).sin(),
    ];
    hover(&mut h, [t[0] + 0.1, t[1] + 0.1]);
    assert_eq!(shown(&h), ["tangent"], "{:?}", preview(&h));
    click(&mut h, [t[0] + 0.1, t[1] + 0.1]);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(stored(&h).constraints, ["tangent"]);
}

fn press(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
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

fn release(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
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
fn with_rectangle() -> Harness<'static, ArrixApp> {
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

fn point_at(h: &Harness<'_, ArrixApp>, uv: [f64; 2]) -> Option<[f64; 2]> {
    let (u, v) = (uv[0] * MM, uv[1] * MM);
    h.state()
        .debug_state()
        .sketch_view?
        .points
        .iter()
        .find(|p| (p.at[0] - u).abs() < 1e-6 && (p.at[1] - v).abs() < 1e-6)
        .map(|p| p.at)
}

/// A drag is many frames and one command, issued at the release; the
/// solver follows the cursor in between and the overlay shows it.
#[test]
fn a_drag_is_one_command_at_the_release_and_undo_takes_it_back() {
    let mut h = with_rectangle();
    let before = h.state().saved().unwrap().0;
    press(&mut h, [50.0, 35.0]);
    for to in [[55.0, 40.0], [60.0, 45.0], [70.0, 50.0]] {
        let pos = at(&h, to);
        h.event(egui::Event::PointerMoved(pos));
        h.step();
    }
    let state = h.state().debug_state();
    let drag = state.sketch.and_then(|m| m.drag).expect("a drag is on");
    assert!(drag.frames >= 3, "{drag:?}");
    assert!(!drag.blocked);
    assert_eq!(
        state.shell.unwrap().status.hint,
        "Dragging a point.",
        "the status line says what is dragged"
    );
    assert!(
        point_at(&h, [70.0, 50.0]).is_some(),
        "the overlay follows the cursor before the release"
    );
    assert_eq!(h.state().saved().unwrap().0, before, "nothing stored yet");
    release(&mut h, [70.0, 50.0]);
    assert_eq!(gesture(&h), Some(1), "one command for the drag");
    let s = stored(&h);
    assert_eq!((s.points, s.lines), (4, 4));
    assert_eq!(
        s.constraints,
        ["horizontal", "horizontal", "vertical", "vertical"],
        "dragging a corner keeps the sides level and plumb"
    );
    assert!(h.state().debug_state().sketch.unwrap().drag.is_none());
    assert_ne!(h.state().saved().unwrap().0, before);

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().saved().unwrap().0, before, "byte for byte");
}

/// A click without movement is a click: it selects and issues nothing. A
/// drag that ends where it began issues nothing either.
#[test]
fn a_click_or_a_drag_that_moves_nothing_issues_no_command() {
    let mut h = with_rectangle();
    click(&mut h, [50.0, 35.0]);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(
        h.state().debug_state().sketch_view.unwrap().selected.len(),
        1
    );
    press(&mut h, [50.0, 35.0]);
    for to in [[60.0, 45.0], [50.0, 35.0]] {
        let pos = at(&h, to);
        h.event(egui::Event::PointerMoved(pos));
        h.step();
    }
    release(&mut h, [50.0, 35.0]);
    assert_eq!(gesture(&h), Some(0), "back where it began");
}

/// A point pinned to the origin stays there: the drag stops where the
/// constraint holds it, and nothing is issued.
#[test]
fn a_pinned_point_does_not_drag() {
    let mut h = sketching();
    h.key_press(Key::R);
    h.run_steps(2);
    click(&mut h, [0.2, 0.3]);
    click(&mut h, [30.0, 20.0]);
    drawn(&mut h, 4);
    h.key_press(Key::Escape);
    h.run_steps(3);
    let before = h.state().saved().unwrap().0;
    press(&mut h, [0.0, 0.0]);
    let pos = at(&h, [-20.0, -20.0]);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    assert!(point_at(&h, [0.0, 0.0]).is_some(), "it stays at the origin");
    release(&mut h, [-20.0, -20.0]);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(h.state().saved().unwrap().0, before);
}

/// A body drag carries a curve with it: the rectangle's top side, grabbed
/// at its middle, moves up, and the corners' sides follow.
#[test]
fn a_curve_drags_by_its_body() {
    let mut h = with_rectangle();
    press(&mut h, [35.0, 35.0]);
    let pos = at(&h, [35.0, 45.0]);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    h.event(egui::Event::PointerMoved(at(&h, [35.0, 45.0])));
    h.step();
    assert_eq!(
        h.state().debug_state().shell.unwrap().status.hint,
        "Dragging a curve."
    );
    release(&mut h, [35.0, 45.0]);
    assert_eq!(gesture(&h), Some(1));
    assert!(point_at(&h, [50.0, 45.0]).is_some(), "the corner followed");
}
