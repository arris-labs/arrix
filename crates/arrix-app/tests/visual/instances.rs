//! One body drawn as several instances, each at its own pose, and a pick
//! that says which instance it landed on (plans/c2-m2 step 2, ADR-0030 §2).

use arrix_core::{ComponentId, DVec3, Frame, Id};
use arrix_viewport::Instance;

use crate::viewport::click;
use crate::{fixture, harness};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn component(n: u64) -> ComponentId {
    ComponentId(Id(n))
}

/// The fixture's box drawn twice from one upload: as component 1 moved
/// 0.6 m along −X, and as component 2 moved 0.6 m along +X and turned a
/// quarter about Z.
fn show_two(h: &mut Harness<'_>) {
    let at = |x: f64, x_axis| Frame::new(DVec3::new(x, 0.0, 0.0), x_axis, DVec3::Z).unwrap();
    h.state_mut()
        .show_mesh(&fixture::box_key(), &fixture::box_mesh([0.5, 0.3, 0.2]));
    h.state_mut().show_instances(Some(vec![
        Instance {
            path: vec![component(1)],
            key: fixture::box_key(),
            pose: at(-0.6, DVec3::X),
        },
        Instance {
            path: vec![component(2)],
            key: fixture::box_key(),
            pose: at(0.6, DVec3::Y),
        },
    ]));
    h.run();
}

/// Where `p` is drawn, in physical pixels.
fn texel(h: &Harness<'_>, p: [f64; 3]) -> (u32, u32) {
    let at = h.state().screen_point(p).expect("in front of the camera");
    let ppp = h.ctx.pixels_per_point();
    ((at.x * ppp) as u32, (at.y * ppp) as u32)
}

/// Both instances draw, lit, where their poses put them, and nothing at
/// the origin between them; a click on the second resolves to its path,
/// one on the first to its own.
#[test]
fn two_instances_of_one_body() {
    harness::with_app("two_instances_of_one_body", |h| {
        h.run();
        let empty = harness::render(h);
        show_two(h);
        let drawn = harness::render(h);
        let changed = |h: &Harness<'_>, p| {
            let (x, y) = texel(h, p);
            empty.get_pixel(x, y) != drawn.get_pixel(x, y)
        };
        assert!(changed(h, [-0.6, 0.0, 0.1]), "the first box");
        assert!(changed(h, [0.6, 0.0, 0.1]), "the second box");
        assert!(!changed(h, [0.0, 0.0, 0.0]), "nothing between them");
        // The second is turned: its long side runs along Y, so it covers a
        // point 0.2 m off its centre in Y that the first does not.
        assert!(changed(h, [0.6, 0.2, 0.1]), "the turned box");
        assert!(!changed(h, [-0.6, 0.2, 0.1]), "the box not turned");

        h.set_render_every_step(true);
        for (x, c) in [(0.6, 2), (-0.6, 1)] {
            let at = h.state().screen_point([x, 0.0, 0.1]).unwrap();
            click(h, at);
            let hit = h.state().last_click().expect("a face of the box");
            assert_eq!(hit.path, [component(c)], "at x = {x}");
            assert_eq!(hit.name.kind, arrix_core::TopoKind::Face);
        }
    });
}
