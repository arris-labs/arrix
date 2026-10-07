//! Trim and extend (plans/sketch-mode step 12, docs/UI-RENDERING.md
//! §Sketch mode): a hover previews the stretch the click removes or adds,
//! and the click is one command that does exactly that.

mod common;

use arrix_ui::CurveShape;
use common::*;
use egui::{Key, Modifiers};

/// A level line `(10,20)-(50,20)` crossed by a plumb one `(30,5)-(30,40)`,
/// and a short level line `(10,30)-(20,30)` that stops short of the plumb
/// one; the select tool back.
fn crossed() -> egui_kittest::Harness<'static, arrix_app::ArrixApp> {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    for (a, b, n) in [
        ([10.0, 20.0], [50.0, 20.0], 1),
        ([30.0, 5.0], [30.0, 40.0], 2),
        ([10.0, 30.0], [20.0, 30.0], 3),
    ] {
        click(&mut h, a);
        click(&mut h, b);
        drawn(&mut h, n);
        h.key_press(Key::Escape);
        h.run_steps(2);
    }
    h
}

/// The ends of the previewed stretch, in millimetres, sorted.
fn previewed(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> Vec<[f64; 2]> {
    let mut ends: Vec<[f64; 2]> = preview(h)
        .curves
        .iter()
        .flat_map(|c| match c {
            CurveShape::Line { from, to } => [*from, *to],
            _ => panic!("a line was expected"),
        })
        .map(|p| p.map(|x| (x / MM * 1e3).round() / 1e3))
        .collect();
    ends.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ends.dedup();
    ends
}

#[test]
fn a_trim_removes_what_the_hover_showed_in_one_command() {
    let mut h = crossed();
    let before = h.state().saved().unwrap().0;
    h.key_press(Key::T);
    h.run_steps(2);
    hover(&mut h, [40.0, 20.0]);
    assert_eq!(previewed(&h), [[30.0, 20.0], [50.0, 20.0]]);
    assert!(point_at(&h, [50.0, 20.0]).is_some());

    click(&mut h, [40.0, 20.0]);
    assert_eq!(gesture(&h), Some(1));
    wait(&mut h, "the trim was drawn", |h| {
        point_at(h, [50.0, 20.0]).is_none()
    });
    assert!(
        point_at(&h, [10.0, 20.0]).is_some(),
        "the far end is untouched"
    );
    assert!(
        point_at(&h, [30.0, 20.0]).is_some(),
        "the line ends at the crossing"
    );

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(
        h.state().saved().unwrap().0,
        before,
        "undo is byte for byte"
    );
}

#[test]
fn an_extend_adds_what_the_hover_showed_in_one_command() {
    let mut h = crossed();
    h.key_press(Key::E);
    h.run_steps(2);
    hover(&mut h, [19.0, 30.0]);
    assert_eq!(previewed(&h), [[20.0, 30.0], [30.0, 30.0]]);

    click(&mut h, [19.0, 30.0]);
    assert_eq!(gesture(&h), Some(1));
    wait(&mut h, "the extension was drawn", |h| {
        point_at(h, [30.0, 30.0]).is_some()
    });
    assert!(point_at(&h, [20.0, 30.0]).is_none(), "the end moved");
    assert!(point_at(&h, [10.0, 30.0]).is_some());
}

#[test]
fn a_click_that_changes_nothing_issues_nothing() {
    let mut h = crossed();
    h.key_press(Key::T);
    h.run_steps(2);
    click(&mut h, [60.0, 50.0]);
    assert_eq!(gesture(&h), Some(0), "empty space");

    // The plumb line's end has no crossing along its continuation but the
    // level lines': extending it upward finds none.
    h.key_press(Key::E);
    h.run_steps(2);
    hover(&mut h, [30.0, 38.0]);
    assert!(
        h.state()
            .debug_state()
            .sketch_view
            .is_some_and(|v| v.preview.is_none()),
        "no stretch to show"
    );
    click(&mut h, [30.0, 38.0]);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(hint(&h), "there is no crossing to reach.");
}

const MIRROR: &str = "core.sketch.mirror";

#[test]
fn an_offset_is_previewed_as_it_is_typed_and_is_one_command() {
    let mut h = with_rectangle();
    let before = h.state().saved().unwrap().0;
    h.key_press(Key::O);
    h.run_steps(2);
    click(&mut h, [35.0, 15.0]);
    assert!(field_open(&h), "a click on the chain opens the distance");
    assert_eq!(gesture(&h), Some(0), "opening the field is no command");

    // Typed, previewed, not yet drawn: the loop 5 mm outside.
    h.event(egui::Event::Text("5 mm".to_owned()));
    h.run_steps(3);
    let corners = previewed(&h);
    for corner in [[15.0, 10.0], [55.0, 10.0], [55.0, 40.0], [15.0, 40.0]] {
        assert!(corners.contains(&corner), "{corner:?} in {corners:?}");
        assert!(point_at(&h, corner).is_none(), "the preview draws no point");
    }

    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(gesture(&h), Some(1));
    wait(&mut h, "the offset was drawn", |h| {
        point_at(h, [15.0, 10.0]).is_some()
    });
    for corner in corners {
        assert!(point_at(&h, corner).is_some(), "{corner:?} is committed");
    }
    assert!(!field_open(&h));

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(
        h.state().saved().unwrap().0,
        before,
        "undo is byte for byte"
    );
}

#[test]
fn an_offset_of_nothing_a_chain_can_take_issues_nothing() {
    let mut h = with_rectangle();
    h.key_press(Key::O);
    h.run_steps(2);
    click(&mut h, [35.0, 15.0]);
    h.event(egui::Event::Text("0 mm".to_owned()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(0));
    assert!(field_open(&h), "the field stays with the reason");
    assert!(field_error(&h).is_some_and(|e| e.contains("other than zero")));
}

#[test]
fn a_mirror_copies_the_selection_about_the_picked_line_in_one_command() {
    let mut h = with_two_lines();
    assert_eq!(enabled(&h, MIRROR), Some(false), "nothing selected");
    let before = h.state().saved().unwrap().0;
    click(&mut h, [25.0, 15.0]);
    assert_eq!(enabled(&h, MIRROR), Some(true));
    h.key_press(Key::M);
    h.run_steps(2);
    hover(&mut h, [30.0, 50.0]);
    let shown = preview(&h).curves.len();
    assert_eq!(shown, 1, "the copy of the selected line");
    let ends = previewed(&h);
    assert_eq!(ends.len(), 2);

    click(&mut h, [30.0, 50.0]);
    assert_eq!(gesture(&h), Some(1));
    drawn(&mut h, 3);
    for end in ends {
        assert!(point_at(&h, end).is_some(), "{end:?} is where it was shown");
    }
    assert_eq!(stored(&h).lines, 3);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.select"));

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(
        h.state().saved().unwrap().0,
        before,
        "undo is byte for byte"
    );
}
