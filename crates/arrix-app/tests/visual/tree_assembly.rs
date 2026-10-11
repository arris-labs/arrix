//! The tree card's assemblies: components nested with how each is placed,
//! joints with how each solved, and a click that switches what the tab
//! looks at or marks a component's instances (plans/c2-m2 step 5,
//! ADR-0030).

use arrix_ui::{JointState, Placement};
use egui_kittest::kittest::Queryable as _;

use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

/// A primary click on the nth label that reads `text`.
fn click_label(h: &mut Harness<'_>, text: &str, nth: usize) {
    let pos = h
        .get_all_by_label(text)
        .nth(nth)
        .unwrap_or_else(|| panic!("no label {nth} reads {text:?}"))
        .rect()
        .center();
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

fn kind(h: &Harness<'_>) -> Option<String> {
    h.state().debug_state().context.map(|c| c.kind)
}

/// The scenario's tree lists the assembly with its bracket grounded and its
/// servo placed by the hinge, and the hinge solved; a click on a part's
/// name or an assembly's switches the tab, a click on a component selects
/// it and tints its instance.
#[test]
fn the_tree_lists_assemblies_and_switches_the_context() {
    harness::with_app("the_tree_lists_assemblies_and_switches_the_context", |h| {
        harness::open_settled(h, "tests/docs/assembly");
        h.run_steps(5);
        let tree = h.state().debug_state().shell.expect("a frame ran").tree;
        let [assembly] = &tree.assemblies[..] else {
            panic!("{:?}", tree.assemblies)
        };
        assert!(assembly.current && tree.parts.iter().all(|p| !p.current));
        let placed: Vec<_> = assembly
            .components
            .iter()
            .map(|c| (c.name.as_str(), c.placement, c.joint.as_deref()))
            .collect();
        assert_eq!(
            placed,
            [
                ("bracket", Placement::Grounded, None),
                ("servo", Placement::ByJoint, Some("hinge")),
            ]
        );
        let [hinge] = &assembly.joints[..] else {
            panic!("{:?}", assembly.joints)
        };
        assert_eq!(
            (hinge.kind.as_str(), hinge.state),
            ("revolute", JointState::Ok)
        );
        snapshot::check(h, "tree_assembly");

        // A part's name looks at it, an assembly's looks back.
        click_label(h, "Bracket", 1);
        assert_eq!(kind(h).as_deref(), Some("part"));
        let tree = h.state().debug_state().shell.expect("a frame ran").tree;
        assert!(tree.parts[0].current && !tree.assemblies[0].current);
        click_label(h, "Servo on bracket", 0);
        assert_eq!(kind(h).as_deref(), Some("assembly"));

        // A component selects, and its instance is tinted.
        h.state_mut().skip_animations();
        let before = harness::render(h);
        click_label(h, "servo", 0);
        let tree = h.state().debug_state().shell.expect("a frame ran").tree;
        let selected: Vec<_> = tree.assemblies[0]
            .components
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(selected, ["servo"]);
        h.run_steps(2);
        assert!(harness::render(h) != before, "the servo is tinted");
    });
}
