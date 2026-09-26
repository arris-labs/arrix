//! The sketch datums: the origin and the two axes as constraint targets.

use arrix_sketch::{Constraint, DatumEntity, Draft, Point};

#[test]
fn test_point_on_datum_origin_and_axes() {
    let mut sk = Draft::seeded(1);
    let p1 = sk.add_point(Point::new(0.05, 0.05));
    sk.add_constraint(Constraint::point_on_datum(p1, DatumEntity::Origin));

    let res = sk.solve();
    assert!(res.converged);
    assert!((sk.points()[&p1].x).abs() < 1e-6);
    assert!((sk.points()[&p1].y).abs() < 1e-6);

    let mut sk2 = Draft::seeded(1);
    let p2 = sk2.add_point(Point::new(0.05, 0.05));
    sk2.add_constraint(Constraint::point_on_datum(p2, DatumEntity::AxisX));
    let res2 = sk2.solve();
    assert!(res2.converged);
    assert!((sk2.points()[&p2].y).abs() < 1e-6);

    let mut sk3 = Draft::seeded(1);
    let p3 = sk3.add_point(Point::new(0.05, 0.05));
    sk3.add_constraint(Constraint::point_on_datum(p3, DatumEntity::AxisY));
    let res3 = sk3.solve();
    assert!(res3.converged);
    assert!((sk3.points()[&p3].x).abs() < 1e-6);
}

#[test]
fn test_distance_to_datum_and_angle_with_datum() {
    let mut sk = Draft::seeded(1);
    let p = sk.add_point(Point::new(0.02, 0.03));
    sk.add_constraint(Constraint::distance_to_datum(p, DatumEntity::AxisX, 0.04));
    sk.add_constraint(Constraint::distance_to_datum(p, DatumEntity::AxisY, 0.06));

    let res = sk.solve();
    assert!(res.converged);
    assert!((sk.points()[&p].y.abs() - 0.04).abs() < 1e-6);
    assert!((sk.points()[&p].x.abs() - 0.06).abs() < 1e-6);

    // Line at angle with Datum AxisX
    let (s, e, line) = sk.add_line(0.0, 0.0, 0.05, 0.02);
    sk.add_constraint(Constraint::point_on_datum(s, DatumEntity::Origin));
    let target_angle = 45.0_f64.to_radians();
    sk.add_constraint(Constraint::angle_with_datum(
        line,
        DatumEntity::AxisX,
        target_angle,
    ));
    let target_len = 0.10;
    sk.add_constraint(Constraint::Distance {
        a: s,
        b: e,
        value: target_len,
    });

    let res2 = sk.solve();
    assert!(res2.converged);
    let ep = &sk.points()[&e];
    let expected_x = target_len * target_angle.cos();
    let expected_y = target_len * target_angle.sin();
    assert!((ep.x - expected_x).abs() < 1e-5);
    assert!((ep.y - expected_y).abs() < 1e-5);
}

#[test]
fn test_symmetric_across_datum() {
    let mut sk = Draft::seeded(1);
    let p1 = sk.add_point(Point::new(0.03, 0.04));
    let p2 = sk.add_point(Point::new(0.05, -0.01));
    sk.add_constraint(Constraint::symmetric_across_datum(
        p1,
        p2,
        DatumEntity::AxisX,
    ));
    sk.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.10,
    });

    let res = sk.solve();
    assert!(res.converged);
    assert!((sk.points()[&p1].x - sk.points()[&p2].x).abs() < 1e-6);
    assert!((sk.points()[&p1].y + sk.points()[&p2].y).abs() < 1e-6);
    assert!((sk.points()[&p1].y.abs() - 0.05).abs() < 1e-6);
}
