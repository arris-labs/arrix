//! A document with an assembly opens looking at it, each placed component's
//! body drawn at the pose the evaluator gave it, and a tab can look at a
//! part instead (plans/c2-m2 step 4, ADR-0030 §1 and §2).

use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn near(got: [f64; 3], want: [f64; 3]) -> bool {
    got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-9)
}

fn settled(h: &mut Harness<'_>) {
    harness::wait_settled(h);
    h.run();
}

/// `tests/docs/assembly` opens in its assembly: the bracket grounded at the
/// origin and the servo where the joint put it (upside down, shaft down
/// the bore: C2 M1's accept), one instance each of two bodies; looking at
/// the bracket draws it alone and the instances go.
#[test]
fn an_assembly_opens_drawn_at_its_poses() {
    harness::with_app("an_assembly_opens_drawn_at_its_poses", |h| {
        harness::open_settled(h, "tests/docs/assembly");
        let state = h.state().debug_state();
        let context = state.context.as_ref().expect("a document with an assembly");
        assert_eq!(
            (context.kind.as_str(), context.name.as_str()),
            ("assembly", "Servo on bracket")
        );
        assert_eq!(state.bodies.len(), 2, "one mesh per body");
        assert_eq!(state.instances.len(), 2);
        let [bracket, servo] = &state.instances[..] else {
            unreachable!()
        };
        assert!(near(bracket.origin, [0.0; 3]) && near(bracket.z_axis, [0.0, 0.0, 1.0]));
        assert!(
            near(servo.origin, [0.004, 0.01, 0.041]),
            "{:?}",
            servo.origin
        );
        assert!(near(servo.x_axis, [1.0, 0.0, 0.0]) && near(servo.z_axis, [0.0, 0.0, -1.0]));
        assert_ne!(bracket.path, servo.path);
        snapshot::check(h, "assembly_view");

        let before = harness::render(h);
        h.state_mut()
            .set_context("Bracket")
            .expect("a part by name");
        settled(h);
        let state = h.state().debug_state();
        assert_eq!(
            state.context.as_ref().map(|c| c.kind.as_str()),
            Some("part")
        );
        assert!(state.instances.is_empty());
        assert!(
            harness::render(h) != before,
            "the servo is gone from the view"
        );

        assert!(h.state_mut().set_context("nothing").is_err());
        h.state_mut()
            .set_context("Servo on bracket")
            .expect("by name");
        settled(h);
        assert_eq!(h.state().debug_state().instances.len(), 2);
    });
}
