//! The joint's create form in the property card (plans/c2-m2 step 9,
//! ADR-0030 §5): what is selected is the parent, the next pick in the
//! scene is the child, OK is one `AddJoint`. Its edit form and the lost
//! anchor are `tests/joint_form.rs`.

use arrix_core::{ComponentId, Id};
use arrix_ui::FieldValue;
use egui_kittest::kittest::Queryable as _;

use crate::component_picks::{open_with_second_servo, servo_2};
use crate::{harness, snapshot};

const BRACKET: &str = "C4VR764XTFX0Z";

fn joints(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> Vec<serde_json::Value> {
    let saved = h.state().saved().expect("a document is open").0;
    let (_, bytes) = saved
        .iter()
        .find(|(p, _)| p.starts_with("assemblies/"))
        .expect("an assembly is saved");
    let assembly: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assembly["joints"]
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect()
}

/// Clicks near `centre` until the pick is a face or an edge of `path`: a
/// vertex fits no anchor.
fn pick_face_or_edge(
    h: &mut egui_kittest::Harness<'_, arrix_app::ArrixApp>,
    centre: [f64; 3],
    component: ComponentId,
) {
    let at = h.state().screen_point(centre).expect("in front");
    for dy in [
        0.0f32, -6.0, 6.0, -12.0, 12.0, -20.0, 20.0, -30.0, 30.0, -45.0, 45.0,
    ] {
        for dx in [0.0f32, -8.0, 8.0, -16.0, 16.0, -30.0, 30.0, -45.0, 45.0] {
            crate::viewport::click(h, at + egui::vec2(dx, dy));
            if let Some(hit) = h.state().last_click()
                && hit.path == [component]
                && hit.name.kind != arrix_core::TopoKind::Vertex
            {
                return;
            }
        }
    }
    panic!("no face or edge of {component:?} found near {centre:?}");
}

#[test]
fn a_joint_is_a_draft_filled_by_two_picks_until_ok() {
    harness::with_app("a_joint_is_a_draft_filled_by_two_picks_until_ok", |h| {
        open_with_second_servo(h);
        h.set_render_every_step(true);
        let bracket = ComponentId(BRACKET.parse::<Id>().expect("the bracket's id"));
        let centre = h
            .state()
            .debug_state()
            .instances
            .iter()
            .find(|i| i.path == [BRACKET.to_owned()])
            .expect("the bracket's instance")
            .origin;
        let servo = h
            .state()
            .debug_state()
            .instances
            .iter()
            .find(|i| i.path == [servo_2().0.to_string()])
            .expect("the second servo's instance")
            .origin;

        // What is selected when the command runs is the parent.
        pick_face_or_edge(h, centre, bracket);
        h.get_by_label("Assembly").click();
        h.run_steps(3);
        assert!(h.state().debug_state().selection.is_some(), "kept");
        h.run_steps(3);
        h.get_by_label("Revolute joint").click();
        h.run_steps(4);
        let form = h.state().debug_state().shell.unwrap().form.unwrap();
        assert!(form.create && form.title == "Joint");
        let field = |name: &str| form.fields.iter().find(|f| f.name == name).unwrap();
        let FieldValue::Reference { values, active, .. } = &field("parent").value else {
            panic!("parent is a reference")
        };
        assert_eq!(values.len(), 1, "the selection seeded the parent");
        assert!(!active);
        let FieldValue::Reference { active, .. } = &field("child").value else {
            panic!("child is a reference")
        };
        assert!(active, "the child is asked for next");
        assert_eq!(joints(h).len(), 1, "a draft adds nothing");

        // The next pick in the scene is the child.
        pick_face_or_edge(h, servo, servo_2());
        h.run_steps(3);
        let form = h.state().debug_state().shell.unwrap().form.unwrap();
        let FieldValue::Reference { values, .. } = &form
            .fields
            .iter()
            .find(|f| f.name == "child")
            .unwrap()
            .value
        else {
            panic!("child is a reference")
        };
        assert_eq!(values.len(), 1);
        snapshot::check(h, "joint_form");

        // OK is one AddJoint.
        h.get_by_label("OK").click();
        h.run_steps(5);
        assert_eq!(h.state().debug_state().gesture_commands, Some(1));
        let made = joints(h);
        assert_eq!(made.len(), 2);
        let new = made.iter().find(|j| j["name"] == "revolute").unwrap();
        // The fields left alone are zeros in their units, not bare numbers
        // (which the evaluator refuses as an angle).
        assert_eq!(new["value"], "0 deg");
        assert_eq!(new["offset"], "0 deg");
        assert_eq!(new["parent"]["path"], serde_json::json!([BRACKET]));
        assert_eq!(
            new["child"]["path"],
            serde_json::json!([servo_2().0.to_string()])
        );
    });
}
