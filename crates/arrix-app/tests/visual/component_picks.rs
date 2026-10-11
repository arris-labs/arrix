//! A pick in an assembly lands on an entity of a component, by its path:
//! the second of two instances of one part is not the first, the status
//! line says which component, and Escape clears the pick (plans/c2-m2 step
//! 6, ADR-0030 §2).

use arrix_core::{ComponentId, Id, PartId};
use arrix_doc::Command;
use egui_kittest::kittest::Queryable as _;

use crate::viewport::click;
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

const SERVO: &str = "ESTFRCC4SPS9K";

fn servo_2() -> ComponentId {
    ComponentId(Id(99))
}

/// `tests/docs/assembly` with a second servo component grounded 80 mm
/// along X from the origin, beside the bracket and the hinged servo.
fn open_with_second_servo(h: &mut Harness<'_>) -> arrix_core::AssemblyId {
    harness::open_settled(h, "tests/docs/assembly");
    let context = h
        .state()
        .debug_state()
        .context
        .expect("an assembly context");
    let assembly: arrix_core::AssemblyId = context.id.parse().expect("an assembly id");
    let part: PartId = SERVO.parse().expect("the servo part");
    let component: arrix_doc::Component = serde_json::from_value(serde_json::json!({
        "id": servo_2(),
        "name": "servo 2",
        "of": { "part": part },
        "pose": { "position": ["80 mm", "0 mm", "0 mm"], "rpy": ["0 deg", "0 deg", "0 deg"] },
    }))
    .expect("a component");
    let changed = h
        .state_mut()
        .submit(Command::AddComponent {
            assembly,
            component: Box::new(component),
        })
        .expect("the component is added");
    assert!(changed);
    harness::wait_settled(h);
    // Looking again, now the third is placed, fits the camera to all three.
    h.state_mut()
        .set_context("Servo on bracket")
        .expect("by name");
    h.run();
    h.state_mut().skip_animations();
    assembly
}

/// Clicks around `centre` until a click lands on something of `path`.
fn click_on(h: &mut Harness<'_>, centre: [f64; 3], path: &[ComponentId]) -> egui::Pos2 {
    let at = h.state().screen_point(centre).expect("in front");
    for dy in [0.0f32, -6.0, 6.0, -12.0, 12.0, -20.0, 20.0, -30.0, 30.0] {
        for dx in [0.0f32, -8.0, 8.0, -16.0, 16.0, -30.0, 30.0] {
            let there = at + egui::vec2(dx, dy);
            click(h, there);
            if h.state().last_click().is_some_and(|hit| hit.path == path) {
                return there;
            }
        }
    }
    panic!("no click near {centre:?} landed on {path:?}");
}

/// A click on the second servo selects its path and not the first's; the
/// hover and the status line name the component; Escape clears the pick.
#[test]
fn a_pick_names_the_component_it_is_of() {
    harness::with_app("a_pick_names_the_component_it_is_of", |h| {
        open_with_second_servo(h);
        let state = h.state().debug_state();
        assert_eq!(state.instances.len(), 3, "bracket and two servos");
        let second = state
            .instances
            .iter()
            .find(|i| i.path == [servo_2().0.to_string()])
            .expect("the second servo's instance");
        let centre = second.origin;

        h.set_render_every_step(true);
        let on = click_on(h, centre, &[servo_2()]);
        let state = h.state().debug_state();
        assert_eq!(state.selection_path, [servo_2().0.to_string()]);
        assert!(state.selection.is_some());

        // The pointer resting on it hovers it, and the status line says
        // where it is.
        h.hover_at(on + egui::vec2(1.0, 0.0));
        for _ in 0..200 {
            h.step();
            if !h.state().debug_state().hover_path.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        h.run_steps(2);
        let state = h.state().debug_state();
        assert_eq!(state.hover_path, [servo_2().0.to_string()]);
        assert!(h.query_by_label_contains("in servo 2").is_some());
        snapshot::check(h, "component_picks");

        // Escape clears the selection.
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        let state = h.state().debug_state();
        assert!(state.selection.is_none() && state.selection_path.is_empty());
    });
}
