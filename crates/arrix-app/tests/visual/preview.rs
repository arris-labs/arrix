//! The create form's preview (plans/forms step 9, ADR-0019): a draft
//! complete enough to evaluate is previewed in the viewport, issuing no
//! command; OK commits the body the preview showed, and Cancel leaves the
//! document as it was. Logic through the debug state; no golden.

use arrix_app::{ArrixApp, DebugState};
use arrix_ui::{FeatureState, FormView};
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable as _;

use crate::harness;
use crate::viewport::click as click_viewport;

type Harness<'a> = egui_kittest::Harness<'a, ArrixApp>;

fn form(h: &Harness<'_>) -> Option<FormView> {
    h.state().debug_state().shell?.form
}

fn gestures(h: &Harness<'_>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

/// Steps until the preview is heard and drawn refined.
fn wait_preview(h: &mut Harness<'_>) -> DebugState {
    let started = web_time::Instant::now();
    loop {
        h.step();
        let state = h.state().debug_state();
        if state.preview.as_ref().is_some_and(|p| p.settled) {
            return state;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "the preview did not settle: {}",
            h.state().debug_state_json()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn open_extrude(h: &mut Harness<'_>) {
    h.get_all_by_label("Part").next().unwrap().click();
    h.run_steps(3);
    h.get_by_label("New extrude").click();
    h.run_steps(3);
}

/// Picks the plate sketch's region into the form's region field.
fn pick_region(h: &mut Harness<'_>) {
    let at = h
        .state()
        .screen_point([0.01, 0.01, 0.0])
        .expect("in front of the camera");
    click_viewport(h, at);
    h.run_steps(3);
}

/// Types `text` into the form's text field `n` and commits it.
fn type_into(h: &mut Harness<'_>, n: usize, text: &str) {
    h.get_all_by_role(Role::TextInput).nth(n).unwrap().focus();
    h.run_steps(2);
    h.key_press_modifiers(
        egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        egui::Key::A,
    );
    h.run_steps(2);
    h.get_all_by_role(Role::TextInput)
        .nth(n)
        .unwrap()
        .type_text(text);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

#[test]
fn an_extrude_previewed_is_the_body_ok_commits() {
    harness::with_app("form_preview", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.set_render_every_step(true);
        let before = h.state().debug_state().bodies.len();

        // No region yet: nothing to evaluate, so no preview.
        open_extrude(h);
        assert!(h.state().debug_state().preview.is_none());
        assert_eq!(form(h).expect("a form").state, None);

        // The region picked: the preview draws the new body, tinted, and
        // hides nothing, since a new body changes none of the others.
        pick_region(h);
        assert_eq!(gestures(h), Some(0), "a pick into a draft issues none");
        let state = wait_preview(h);
        let preview = state.preview.expect("a preview");
        assert_eq!(preview.bodies.len(), 1, "{preview:?}");
        assert!(preview.replaced.is_empty(), "{preview:?}");
        assert_eq!(state.bodies.len(), before, "the document's bodies stay");
        let header = form(h).expect("a form");
        assert_eq!(header.state, Some(FeatureState::Ok));
        assert_eq!(header.diagnostic, None);

        // A new distance is previewed again: still no command.
        type_into(h, 1, "12 mm");
        assert_eq!(gestures(h), Some(0), "a field of a draft issues none");
        let preview = wait_preview(h).preview.expect("a preview");
        let shown = &preview.bodies[0];
        let height = shown.bounds.expect("bounds")[5] - shown.bounds.expect("bounds")[2];
        assert!((height - 0.012).abs() < 1e-6, "{shown:?}");

        // OK: one command, and the committed body is the one previewed.
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert_eq!(gestures(h), Some(1));
        harness::wait_settled(h);
        let state = h.state().debug_state();
        assert!(state.preview.is_none(), "OK clears the preview");
        assert_eq!(state.bodies.len(), before + 1);
        let committed = state
            .bodies
            .iter()
            .find(|b| b.feature == shown.feature && b.slot == shown.slot)
            .expect("the previewed body, committed");
        assert_eq!(
            (committed.triangles, committed.bounds, committed.chord_um),
            (shown.triangles, shown.bounds, shown.chord_um)
        );
    });
}

#[test]
fn a_join_preview_replaces_its_target_and_cancel_drops_it() {
    harness::with_app("form_preview_cancel", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.set_render_every_step(true);
        let before = h.state().debug_state().bodies;

        open_extrude(h);
        pick_region(h);
        wait_preview(h);
        // Join, with the plate as the target: the plate is replaced.
        h.get_all_by_role(Role::ComboBox).next().unwrap().click();
        h.run_steps(2);
        // Two nodes say Join while the popup is open: its option is last.
        h.get_all_by_label("Join").last().unwrap().click();
        h.run_steps(3);
        h.get_by_label("Body to join to or cut").click();
        h.run_steps(3);
        let top = h
            .state()
            .screen_point([-0.016, -0.011, 0.005])
            .expect("in front of the camera");
        click_viewport(h, top);
        h.run_steps(3);
        let state = wait_preview(h);
        let preview = state.preview.expect("a preview");
        assert_eq!(preview.replaced.len(), 1, "{preview:?}");
        assert_eq!(preview.bodies.len(), 1, "{preview:?}");
        assert_eq!(gestures(h), Some(0));

        h.get_by_label("Cancel").click();
        h.run_steps(3);
        assert_eq!(gestures(h), Some(0), "Cancel issues none");
        let state = h.state().debug_state();
        assert!(state.preview.is_none(), "Cancel clears the preview");
        assert_eq!(state.bodies.len(), before.len());
    });
}
