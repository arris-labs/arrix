//! Constraints from the selection (plans/sketch-mode step 10,
//! docs/UI-RENDERING.md §Sketch mode): each kind is enabled for a selection
//! that fits it and is one command, checked against the sketch first; a
//! redundant or conflicting one is refused with the reason and no command.

mod common;

use common::*;
use egui::{Key, Modifiers};

const PARALLEL: &str = "core.sketch.constrain.parallel";
const HORIZONTAL: &str = "core.sketch.constrain.horizontal";

fn select_both_lines(h: &mut egui_kittest::Harness<'_, arrix_app::ArrixApp>) {
    click(h, [25.0, 15.0]);
    click_with(h, [30.0, 50.0], Modifiers::SHIFT);
    assert_eq!(gesture(h), Some(0), "selecting is no command");
}

#[test]
fn a_kind_is_enabled_only_for_a_selection_that_fits() {
    let mut h = with_two_lines();
    assert_eq!(enabled(&h, PARALLEL), Some(false), "nothing selected");
    click(&mut h, [25.0, 15.0]);
    assert_eq!(enabled(&h, PARALLEL), Some(false), "one line");
    assert_eq!(enabled(&h, HORIZONTAL), Some(true), "one line");
    click_with(&mut h, [30.0, 50.0], Modifiers::SHIFT);
    assert_eq!(enabled(&h, PARALLEL), Some(true), "two lines");
    assert_eq!(enabled(&h, "core.sketch.constrain.tangent"), Some(false));
    assert_eq!(enabled(&h, "core.sketch.constrain.coincident"), Some(false));
    click_with(&mut h, [10.0, 10.0], Modifiers::SHIFT);
    assert_eq!(enabled(&h, PARALLEL), Some(false), "two lines and a point");
}

#[test]
fn parallel_is_one_command_and_a_second_one_is_refused() {
    let mut h = with_two_lines();
    let before = h.state().saved().unwrap().0;
    select_both_lines(&mut h);
    run_command(&mut h, "Parallel");
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(stored(&h).constraints, ["parallel"]);

    // Already enforced: nothing is added and the status line says why.
    run_command(&mut h, "Parallel");
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(stored(&h).constraints, ["parallel"]);
    assert_eq!(
        hint(&h),
        "The sketch already enforces that: nothing was added."
    );

    // Perpendicular to what is parallel cannot hold.
    run_command(&mut h, "Perpendicular");
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(stored(&h).constraints, ["parallel"]);
    assert!(hint(&h).contains("cannot hold together"), "{}", hint(&h));

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().saved().unwrap().0, before, "byte for byte");
}

#[test]
fn horizontal_moves_the_line_and_a_key_makes_it() {
    let mut h = with_two_lines();
    click(&mut h, [25.0, 15.0]);
    h.key_press(Key::H);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(stored(&h).constraints, ["horizontal"]);
    let view = h.state().debug_state().sketch_view.unwrap();
    let levels: Vec<_> = view
        .points
        .iter()
        .filter(|p| p.at[1] < 0.03)
        .map(|p| p.at[1])
        .collect();
    assert_eq!(levels.len(), 2);
    assert!((levels[0] - levels[1]).abs() < 1e-9, "{levels:?}");
    // Both lines at once: one command, the one already level refused
    // quietly, the other made.
    click_with(&mut h, [30.0, 50.0], Modifiers::SHIFT);
    h.key_press(Key::H);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(stored(&h).constraints, ["horizontal", "horizontal"]);
}

#[test]
fn points_coincide_align_and_pin() {
    let mut h = with_two_lines();
    click(&mut h, [40.0, 20.0]);
    click_with(&mut h, [15.0, 40.0], Modifiers::SHIFT);
    run_command(&mut h, "Align V");
    assert_eq!(gesture(&h), Some(1), "{}", hint(&h));
    run_command(&mut h, "Coincident");
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(
        stored(&h).constraints,
        ["coincident", "vertical_points"],
        "the alignment before it, and kept"
    );
    // The joined point on the origin.
    click(&mut h, [10.0, 10.0]);
    run_command(&mut h, "On origin");
    assert_eq!(gesture(&h), Some(1));
    assert!(
        stored(&h)
            .constraints
            .contains(&"coincident_to_datum".to_owned())
    );
}

#[test]
fn a_line_and_a_circle_are_tangent() {
    let mut h = sketching();
    h.key_press(Key::C);
    h.run_steps(2);
    click(&mut h, [30.0, 30.0]);
    click(&mut h, [50.0, 30.0]);
    drawn(&mut h, 1);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.key_press(Key::L);
    h.run_steps(2);
    click(&mut h, [5.0, 70.0]);
    click(&mut h, [60.0, 80.0]);
    drawn(&mut h, 2);
    for _ in 0..2 {
        h.key_press(Key::Escape);
        h.run_steps(2);
    }
    click(&mut h, [50.0, 30.0]);
    click_with(&mut h, [30.0, 75.0], Modifiers::SHIFT);
    assert_eq!(enabled(&h, "core.sketch.constrain.tangent"), Some(true));
    run_command(&mut h, "Tangent");
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(stored(&h).constraints, ["tangent"]);
}

#[test]
fn construction_toggles_the_selected_curves() {
    let mut h = with_two_lines();
    assert_eq!(enabled(&h, "core.sketch.construction"), Some(false));
    click(&mut h, [25.0, 15.0]);
    h.key_press(Key::Q);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    let flags = |h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>| -> Vec<bool> {
        h.state()
            .debug_state()
            .sketch_view
            .unwrap()
            .curves
            .iter()
            .map(|c| c.construction)
            .collect()
    };
    assert_eq!(flags(&h).iter().filter(|c| **c).count(), 1);
    h.key_press(Key::Q);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(flags(&h).iter().filter(|c| **c).count(), 0, "and back");
}
