//! Construction geometry is invisible to region detection and to
//! open-profile validation, and to nothing else: the solver still
//! constrains against it (docs/DATA-MODEL.md §Sketches). That a draw tool
//! still snaps to it is inference's, M3's.

use arrix_sketch::{Constraint, Draft, Entity, Point, PointId, ValidationOptions, find_profiles};

/// A 100×60 mm CCW plate with one shared point per corner.
fn plate() -> Draft {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::new(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.1, 0.0));
    let p2 = s.add_point(Point::new(0.1, 0.06));
    let p3 = s.add_point(Point::new(0.0, 0.06));
    for (start, end) in [(p0, p1), (p1, p2), (p2, p3), (p3, p0)] {
        s.add_entity(Entity::Line { start, end });
    }
    s
}

#[test]
fn a_construction_circle_crossing_the_plate_is_no_profile_and_no_hole() {
    let mut s = plate();
    // Centred on the plate's right edge. As profile geometry its disc is a
    // profile of its own (loops are not intersected); as construction it is
    // nothing at all.
    let (_, circle) = s.add_circle(0.1, 0.03, 0.02);
    assert!(
        find_profiles(&s).len() > 1,
        "as profile geometry the circle should add a region"
    );

    s.set_construction(circle, true);
    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 1, "{profiles:#?}");
    assert!(profiles[0].holes.is_empty(), "{profiles:#?}");
    assert!(
        profiles[0].outer.edges.iter().all(|e| e.entity != circle),
        "{profiles:#?}"
    );
    let report = s.validate_sketch(&ValidationOptions::default());
    assert_eq!(report.closed_profiles_count, 1);
    assert!(report.is_valid_for_extrusion, "{report:?}");
}

#[test]
fn an_open_construction_line_has_no_open_vertices() {
    let mut s = plate();
    let (_, _, line) = s.add_line(0.02, 0.03, 0.08, 0.03);
    assert_eq!(s.detect_open_vertices().len(), 2);

    s.set_construction(line, true);
    assert!(s.detect_open_vertices().is_empty());
    let report = s.validate_sketch(&ValidationOptions::default());
    assert!(report.is_valid_for_extrusion, "{report:?}");
}

#[test]
fn a_profile_edge_joined_only_to_construction_is_still_open() {
    let mut s = Draft::seeded(1);
    let (_, end, _) = s.add_line(0.0, 0.0, 0.05, 0.0);
    let guide_start = s.add_point(Point::new(0.05, 0.0));
    let guide_end = s.add_point(Point::new(0.05, 0.05));
    let guide = s.add_entity(Entity::Line {
        start: guide_start,
        end: guide_end,
    });
    s.add_constraint(Constraint::Coincident {
        a: end,
        b: guide_start,
    });
    s.set_construction(guide, true);
    let open: Vec<PointId> = s
        .detect_open_vertices()
        .iter()
        .map(|v| v.point_id)
        .collect();
    assert!(open.contains(&end), "{open:?}");
}

#[test]
fn holes_on_a_construction_bolt_circle_are_the_plate_holes() {
    let mut s = plate();
    let (bolt_centre, bolt) = s.add_circle(0.05, 0.03, 0.02);
    s.set_construction(bolt, true);
    s.point_mut(bolt_centre).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: bolt,
        value: 0.02,
    });

    // Drawn a little off the bolt circle, so the solve has to pull each onto
    // it through `PointOnCircle` against the construction circle.
    let mut holes = Vec::new();
    for (x, y) in [(0.072, 0.03), (0.05, 0.051), (0.029, 0.03), (0.05, 0.008)] {
        let (centre, hole) = s.add_circle(x, y, 0.0015);
        s.add_constraint(Constraint::PointOnCircle {
            point: centre,
            circle: bolt,
        });
        holes.push((centre, hole));
    }
    let result = s.solve();
    assert!(result.converged, "{result:?}");
    for &(centre, _) in &holes {
        let p = s.point(centre).unwrap();
        let r = (p.x - 0.05).hypot(p.y - 0.03);
        assert!(
            (r - 0.02).abs() < 1e-9,
            "hole centre {p:?} is off the bolt circle"
        );
    }

    // The plate, largest first, then each hole's own disc — and nothing for
    // the bolt circle.
    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 5, "{profiles:#?}");
    assert!(
        profiles
            .iter()
            .all(|p| p.outer.edges.iter().all(|e| e.entity != bolt)),
        "{profiles:#?}"
    );
    let mut hole_entities: Vec<_> = profiles[0]
        .holes
        .iter()
        .map(|h| h.edges[0].entity)
        .collect();
    hole_entities.sort();
    let mut want: Vec<_> = holes.iter().map(|&(_, hole)| hole).collect();
    want.sort();
    assert_eq!(hole_entities, want);
}
