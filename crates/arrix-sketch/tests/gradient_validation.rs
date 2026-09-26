use std::collections::{BTreeSet, HashSet};

use arrix_sketch::{
    Constraint, Draft, Entity, EntityId, Pin, Point, PointId, Sketch, System,
    constraint_residual_count, jacobian, set_var,
};

fn add_pt(sk: &mut Draft, x: f64, y: f64) -> PointId {
    sk.add_point(Point::new(x, y))
}

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0xdeadbeef } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_f64_bipolar(&mut self) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64);
        2.0 * u - 1.0
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64);
        lo + (hi - lo) * u
    }
}

fn variant_tag(c: &Constraint) -> &'static str {
    match *c {
        Constraint::Coincident { .. } => "Coincident",
        Constraint::Horizontal { .. } => "Horizontal",
        Constraint::Vertical { .. } => "Vertical",
        Constraint::HorizontalPoints { .. } => "HorizontalPoints",
        Constraint::VerticalPoints { .. } => "VerticalPoints",
        Constraint::Parallel { .. } => "Parallel",
        Constraint::Perpendicular { .. } => "Perpendicular",
        Constraint::Equal { .. } => "Equal",
        Constraint::Concentric { .. } => "Concentric",
        Constraint::Tangent { .. } => "Tangent",
        Constraint::TangentCircles { .. } => "TangentCircles",
        Constraint::Symmetric { .. } => "Symmetric",
        Constraint::Distance { .. } => "Distance",
        Constraint::HorizontalDistance { .. } => "HorizontalDistance",
        Constraint::VerticalDistance { .. } => "VerticalDistance",
        Constraint::DistancePointLine { .. } => "DistancePointLine",
        Constraint::DistanceParallelLines { .. } => "DistanceParallelLines",
        Constraint::Angle { .. } => "Angle",
        Constraint::Radius { .. } => "Radius",
        Constraint::Diameter { .. } => "Diameter",
        Constraint::PointOnLine { .. } => "PointOnLine",
        Constraint::PointOnCircle { .. } => "PointOnCircle",
        Constraint::Midpoint { .. } => "Midpoint",
        Constraint::Fix { .. } => "Fix",
        Constraint::SymmetricPoints { .. } => "SymmetricPoints",
        Constraint::PointOnPerpBisector { .. } => "PointOnPerpBisector",
        Constraint::ArcLength { .. } => "ArcLength",
        Constraint::Block { .. } => "Block",
        Constraint::AnglePoints { .. } => "AnglePoints",
        Constraint::DistanceToAxisX { .. } => "DistanceToAxisX",
        Constraint::DistanceToAxisY { .. } => "DistanceToAxisY",
        Constraint::DistanceCircleCircle { .. } => "DistanceCircleCircle",
        Constraint::DistancePointCircle { .. } => "DistancePointCircle",
        #[cfg(feature = "snells-law")]
        Constraint::SnellsLaw { .. } => "SnellsLaw",
        Constraint::PointOnDatum { .. } => "PointOnDatum",
        Constraint::DistanceToDatum { .. } => "DistanceToDatum",
        Constraint::AngleWithDatum { .. } => "AngleWithDatum",
        Constraint::SymmetricAcrossDatum { .. } => "SymmetricAcrossDatum",
        Constraint::CoincidentToDatum { .. } => "CoincidentToDatum",
        #[cfg(feature = "conics")]
        Constraint::PointOnEllipse { .. } => "PointOnEllipse",
        #[cfg(feature = "conics")]
        Constraint::TangentLineEllipse { .. } => "TangentLineEllipse",
        #[cfg(feature = "conics")]
        Constraint::InternalAlignment { .. } => "InternalAlignment",
        #[cfg(feature = "conics")]
        Constraint::MinorRadius { .. } => "MinorRadius",
        #[cfg(feature = "conics")]
        Constraint::MajorRadius { .. } => "MajorRadius",
        #[cfg(feature = "conics")]
        Constraint::PointOnBSpline { .. } => "PointOnBSpline",
        #[cfg(feature = "conics")]
        Constraint::BSplineTangent { .. } => "BSplineTangent",
        #[cfg(feature = "conics")]
        Constraint::BSplineCurvature { .. } => "BSplineCurvature",
    }
}

const ALL_TAGS: &[&str] = &[
    "Coincident",
    "Horizontal",
    "Vertical",
    "HorizontalPoints",
    "VerticalPoints",
    "Parallel",
    "Perpendicular",
    "Equal",
    "Concentric",
    "Tangent",
    "TangentCircles",
    "Symmetric",
    "Distance",
    "HorizontalDistance",
    "VerticalDistance",
    "DistancePointLine",
    "DistanceParallelLines",
    "Angle",
    "Radius",
    "Diameter",
    "PointOnLine",
    "PointOnCircle",
    "Midpoint",
    "Fix",
    "SymmetricPoints",
    "PointOnPerpBisector",
    "ArcLength",
    "Block",
    "AnglePoints",
    "DistanceToAxisX",
    "DistanceToAxisY",
    "DistanceCircleCircle",
    "DistancePointCircle",
    "PointOnDatum",
    "DistanceToDatum",
    "AngleWithDatum",
    "SymmetricAcrossDatum",
    "CoincidentToDatum",
];

/// The `conics`-gated half of the coverage list, so the exhaustiveness check
/// below shrinks with the enum rather than failing on a default build.
#[cfg(feature = "conics")]
const CONIC_TAGS: &[&str] = &[
    "PointOnEllipse",
    "TangentLineEllipse",
    "InternalAlignment",
    "MinorRadius",
    "MajorRadius",
    "PointOnBSpline",
    "BSplineTangent",
    "BSplineCurvature",
];
#[cfg(not(feature = "conics"))]
const CONIC_TAGS: &[&str] = &[];
#[cfg(feature = "snells-law")]
const SNELL_TAGS: &[&str] = &["SnellsLaw"];
#[cfg(not(feature = "snells-law"))]
const SNELL_TAGS: &[&str] = &[];

fn check_well_conditioned(sketch: &Sketch, is_shared_point_fixture: bool) {
    for ent in sketch.entities().values() {
        if let Entity::Line { start, end } = ent {
            let d = sketch.points()[start].distance_to(&sketch.points()[end]);
            assert!(d >= 0.010, "line length {d} < 10 mm");
        }
        if let Entity::Circle { radius, .. } = ent {
            assert!(*radius >= 0.010, "circle radius {radius} < 10 mm");
        }
    }

    if !is_shared_point_fixture {
        let pt_ids: Vec<PointId> = sketch.points().keys().copied().collect();
        for i in 0..pt_ids.len() {
            for j in (i + 1)..pt_ids.len() {
                let p1 = &sketch.points()[&pt_ids[i]];
                let p2 = &sketch.points()[&pt_ids[j]];
                let d = p1.distance_to(p2);
                assert!(
                    d >= 0.001,
                    "points {:?} and {:?} are too close: {d} < 1 mm",
                    pt_ids[i],
                    pt_ids[j]
                );
            }
        }
    }

    for (pid, pt) in sketch.points() {
        assert!(
            pt.x.abs() >= 0.005,
            "point {:?} x is too close to zero: {}",
            pid,
            pt.x
        );
        assert!(
            pt.y.abs() >= 0.005,
            "point {:?} y is too close to zero: {}",
            pid,
            pt.y
        );
    }

    // Shared-point fixtures are deliberately degenerate (a circle centered on
    // a line endpoint, a point mirrored across a line it sits on): they exist
    // to exercise duplicate-column accumulation, and sit *on* cusps whose
    // dependence collapses to zero on both sides. Margins do not apply.
    if !is_shared_point_fixture {
        check_branch_margins(sketch);
    }
}

/// The premise that makes finite differences a valid oracle here: no jittered
/// instance may cross a branch, a sign flip, or a clamp. Every fixture must sit
/// far enough from each cusp its constraints select on that `JITTER_AMP` cannot
/// reach it. A badly chosen fixture fails loudly here rather than flaking in
/// the differential comparison.
fn check_branch_margins(sketch: &Sketch) {
    /// Angle margin from 0, ±π/2 and π — 15°.
    const ANGLE_MARGIN: f64 = 15.0 * std::f64::consts::PI / 180.0;

    let pt = |id: &PointId| sketch.points()[id].clone();
    let dir = |e: &EntityId| match &sketch.entities()[e] {
        Entity::Line { start, end } => {
            let (a, b) = (pt(start), pt(end));
            (b.x - a.x, b.y - a.y)
        }
        _ => panic!("expected a line"),
    };
    let radius = |e: &EntityId| match &sketch.entities()[e] {
        Entity::Circle { radius, .. } => *radius,
        Entity::Arc { center, start, .. } => pt(center).distance_to(&pt(start)),
        _ => panic!("expected a circle or arc"),
    };
    let center = |e: &EntityId| match &sketch.entities()[e] {
        Entity::Circle { center, .. } | Entity::Arc { center, .. } => pt(center),
        _ => panic!("expected a circle or arc"),
    };
    let signed_dist = |p: &PointId, line: &EntityId| {
        let Entity::Line { start, .. } = &sketch.entities()[line] else {
            panic!("expected a line");
        };
        let (a, q) = (pt(start), pt(p));
        let (dx, dy) = dir(line);
        let l = (dx * dx + dy * dy).sqrt();
        ((q.x - a.x) * dy - (q.y - a.y) * dx) / l
    };
    // Signed angle from u to v, as `atan2(cross, dot)` — exactly what the
    // residuals compute.
    let signed_angle =
        |u: (f64, f64), v: (f64, f64)| (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1);
    // Which angles are cusps depends on the residual's own branch, so each
    // caller names them: `Parallel` *wants* to sit near 0 and only breaks at
    // ±π/2 (the `dot >= 0` switch), while `Perpendicular` wants π/2 and breaks
    // at 0 and ±π.
    let assert_angle = |theta: f64, cusps: &[(f64, &str)], what: &str| {
        for &(cusp, name) in cusps {
            assert!(
                (theta.abs() - cusp).abs() >= ANGLE_MARGIN,
                "{what}: angle {theta} is within 15° of {name}"
            );
        }
    };
    const CUSP_ZERO: (f64, &str) = (0.0, "0");
    const CUSP_HALF_PI: (f64, &str) = (std::f64::consts::FRAC_PI_2, "π/2");
    const CUSP_PI: (f64, &str) = (std::f64::consts::PI, "π");
    // Two radii compared by a residual must differ by ≥ 20 % of the larger.
    let assert_radii_differ = |ra: f64, rb: f64, what: &str| {
        assert!(
            (ra - rb).abs() >= 0.2 * ra.max(rb),
            "{what}: radii {ra} and {rb} differ by < 20 %"
        );
    };

    for record in sketch.constraints().values() {
        match record.constraint {
            // `angle_err` switches formula at `dot == 0`, i.e. at ±π/2.
            Constraint::Parallel { a, b } => {
                assert_angle(signed_angle(dir(&a), dir(&b)), &[CUSP_HALF_PI], "Parallel");
            }
            // `angle_err` switches at `theta == 0`, and jumps at ±π.
            Constraint::Perpendicular { a, b } => {
                assert_angle(
                    signed_angle(dir(&a), dir(&b)),
                    &[CUSP_ZERO, CUSP_PI],
                    "Perpendicular",
                );
            }
            // `.abs()` on the angle: a cusp at 0, and the ±π branch cut.
            Constraint::Angle { a, b, .. } => {
                assert_angle(
                    signed_angle(dir(&a), dir(&b)),
                    &[CUSP_ZERO, CUSP_PI],
                    "Angle",
                );
            }
            Constraint::AnglePoints { a, vertex, b, .. } => {
                let (v, pa, pb) = (pt(&vertex), pt(&a), pt(&b));
                let theta = signed_angle((pa.x - v.x, pa.y - v.y), (pb.x - v.x, pb.y - v.y));
                assert_angle(theta, &[CUSP_ZERO, CUSP_PI], "AnglePoints");
            }
            Constraint::ArcLength { arc, .. } => {
                let Entity::Arc { center, start, end } = &sketch.entities()[&arc] else {
                    panic!("ArcLength on a non-arc");
                };
                let (c, s, e) = (pt(center), pt(start), pt(end));
                // The residual wraps a negative sweep by +2π, so the cusp is at
                // a raw `atan2` of 0 — where the sweep flips between ~0 and ~2π.
                let raw = signed_angle((s.x - c.x, s.y - c.y), (e.x - c.x, e.y - c.y));
                assert!(
                    raw.abs() >= ANGLE_MARGIN,
                    "ArcLength: sweep {raw} is within 15° of the 0/2π wrap"
                );
            }
            Constraint::Equal { a, b } => {
                // Only the radius formulation has a cusp-free margin to keep;
                // line/line is smooth in the lengths.
                if !matches!(sketch.entities()[&a], Entity::Line { .. }) {
                    assert!(
                        radius(&a) >= 0.010 && radius(&b) >= 0.010,
                        "Equal: radius < 10 mm"
                    );
                }
            }
            Constraint::TangentCircles { a, b } => {
                let d = center(&a).distance_to(&center(&b));
                let (ra, rb) = (radius(&a), radius(&b));
                // The residual picks whichever of |d − (ra+rb)| and
                // |d − |ra−rb|| is smaller; the two must not be close.
                let ext = (d - (ra + rb)).abs();
                let int = (d - (ra - rb).abs()).abs();
                assert!(
                    (ext - int).abs() >= 0.2 * d,
                    "TangentCircles: internal/external branch margin {} < 20 % of d = {d}",
                    (ext - int).abs()
                );
                // `sgn(ra − rb)` is only read on the internal branch.
                if int < ext {
                    assert_radii_differ(ra, rb, "TangentCircles (internal)");
                }
            }
            Constraint::DistanceCircleCircle { a, b, value } => {
                let d = center(&a).distance_to(&center(&b));
                let (ra, rb) = (radius(&a), radius(&b));
                let ext_clearance = d - (ra + rb);
                let int_clearance = (ra - rb).abs() - d;
                let ext = (ext_clearance - value).abs();
                let int = (int_clearance - value).abs();
                assert!(
                    (ext - int).abs() >= 0.2 * d,
                    "DistanceCircleCircle: clearance branch margin {} < 20 % of d = {d}",
                    (ext - int).abs()
                );
                if int_clearance >= 0.0 && int < ext {
                    assert_radii_differ(ra, rb, "DistanceCircleCircle (internal)");
                }
            }
            Constraint::DistancePointCircle {
                point: p, circle, ..
            } => {
                let d = pt(&p).distance_to(&center(&circle));
                assert!(
                    (d - radius(&circle)).abs() >= 0.005,
                    "DistancePointCircle: |d − r| < 5 mm (the .abs() cusp)"
                );
            }
            Constraint::Tangent { line, circle } => {
                assert!(
                    signed_dist(&entity_center_point(sketch, circle), &line).abs() >= 0.005,
                    "Tangent: |signed point-line distance| < 5 mm (sign freeze cusp)"
                );
            }
            Constraint::DistancePointLine { point: p, line, .. } => {
                assert!(
                    signed_dist(&p, &line).abs() >= 0.005,
                    "DistancePointLine: |signed distance| < 5 mm (sign freeze cusp)"
                );
            }
            Constraint::DistanceParallelLines { a, b, .. } => {
                let Entity::Line { start, .. } = &sketch.entities()[&b] else {
                    panic!("expected a line");
                };
                assert!(
                    signed_dist(start, &a).abs() >= 0.005,
                    "DistanceParallelLines: |signed distance| < 5 mm (sign freeze cusp)"
                );
            }
            Constraint::HorizontalDistance { a, b, .. } => {
                assert!(
                    (pt(&a).x - pt(&b).x).abs() >= 0.005,
                    "HorizontalDistance: |Δx| < 5 mm (sign freeze cusp)"
                );
            }
            Constraint::VerticalDistance { a, b, .. } => {
                assert!(
                    (pt(&a).y - pt(&b).y).abs() >= 0.005,
                    "VerticalDistance: |Δy| < 5 mm (sign freeze cusp)"
                );
            }
            _ => {}
        }
    }
}

fn entity_center_point(sketch: &Sketch, e: EntityId) -> PointId {
    match &sketch.entities()[&e] {
        Entity::Circle { center, .. } | Entity::Arc { center, .. } => *center,
        _ => panic!("expected a circle or arc"),
    }
}

fn validate_fixture(nominal: &Sketch, is_shared: bool, seed: u64) {
    check_well_conditioned(nominal, is_shared);
    let mut rng = XorShift64::new(seed);
    const K: usize = 32;
    const JITTER_AMP: f64 = 0.0005; // 0.5 mm

    for iter in 0..=K {
        let mut sk = nominal.clone();
        if iter > 0 {
            for pt in sk.points_mut() {
                if !pt.fixed {
                    pt.x += JITTER_AMP * rng.next_f64_bipolar();
                    pt.y += JITTER_AMP * rng.next_f64_bipolar();
                }
            }
            let ids: Vec<_> = sk.entities().keys().copied().collect();
            for id in ids {
                match sk.entity_mut(id).unwrap() {
                    Entity::Circle { radius, .. } => {
                        *radius += JITTER_AMP * rng.next_f64_bipolar();
                    }
                    #[cfg(feature = "conics")]
                    Entity::Ellipse { minor_radius, .. }
                    | Entity::ArcOfEllipse { minor_radius, .. } => {
                        *minor_radius += JITTER_AMP * rng.next_f64_bipolar();
                    }
                    _ => {}
                }
            }
        }

        let sys = System::build(&sk);
        let x = sys.pack(&sk);
        let n = sys.n_vars();
        let m = sys.n_residuals();

        // 1. Numerical finite differences with h = 1e-6
        const H: f64 = 1e-6;
        let mut j_fd = vec![vec![0.0; n]; m];
        let mut sk_perturbed = sk.clone();
        for col in 0..n {
            let v = sys.vars[col];
            let base = x[col];

            set_var(&mut sk_perturbed, v, base + H);
            let r_plus = sys.residuals_of(&sk_perturbed);

            set_var(&mut sk_perturbed, v, base - H);
            let r_minus = sys.residuals_of(&sk_perturbed);

            set_var(&mut sk_perturbed, v, base);

            for row in 0..m {
                j_fd[row][col] = (r_plus[row] - r_minus[row]) / (2.0 * H);
            }
        }

        // 2. Jacobian evaluation
        let j_rows = jacobian(&sys, &sk, &x);
        let j_dense = j_rows.to_dense();

        // 3. Differential check: |ja - jfd| <= 1e-7 + 1e-6 * |jfd|
        for r in 0..m {
            for c in 0..n {
                let ja = j_dense[r][c];
                let jfd = j_fd[r][c];
                let diff = (ja - jfd).abs();
                let tol = 1e-7 + 1e-6 * jfd.abs();
                let constraints_debug: Vec<_> =
                    sk.constraints().values().map(|r| &r.constraint).collect();
                assert!(
                    diff <= tol,
                    "Gradient mismatch for constraints {:?} at row {r}, col {c} (var {:?}): analytic={ja}, fd={jfd}, diff={diff}, tol={tol}",
                    constraints_debug,
                    sys.vars[c]
                );
            }
        }

        // 4. Sparsity pattern checks:
        // Every nonzero analytic entry belongs to constraint support
        let mut row_offset = 0;
        for record in sk.constraints().values() {
            if !record.is_active || !record.is_driving {
                continue;
            }
            let count = constraint_residual_count(&record.constraint, &sk);
            let mut support_set = BTreeSet::new();
            for p in record.constraint.referenced_points() {
                if let Some(&col) = sys.point_col.get(&p) {
                    support_set.insert(col);
                    support_set.insert(col + 1);
                }
            }
            for e in record.constraint.referenced_entities() {
                if let Some(ent) = sk.entity(e) {
                    for p in ent.point_ids() {
                        if let Some(&col) = sys.point_col.get(&p) {
                            support_set.insert(col);
                            support_set.insert(col + 1);
                        }
                    }
                    if let Some(&col) = sys.radius_col.get(&e) {
                        support_set.insert(col);
                    }
                    if let Some(&col) = sys.minor_radius_col.get(&e) {
                        support_set.insert(col);
                    }
                }
            }

            for r_idx in 0..count {
                let row = row_offset + r_idx;
                for &(col, val) in &j_rows.rows[row] {
                    if val.abs() > 1e-12 {
                        assert!(
                            support_set.contains(&col),
                            "Analytic nonzero entry at row {row}, col {col} (var {:?}) not in constraint support",
                            sys.vars[col]
                        );
                    }
                }
            }
            row_offset += count;
        }

        // Every column with |Jfd| > 1e-7 has a nonzero entry
        for r in 0..m {
            for c in 0..n {
                if j_fd[r][c].abs() > 1e-7 {
                    let constraints_debug: Vec<_> =
                        sk.constraints().values().map(|r| &r.constraint).collect();
                    assert!(
                        j_dense[r][c].abs() > 1e-12,
                        "Missing analytic entry for constraints {:?} at row {r}, col {c} (var {:?}) where |Jfd| = {}",
                        constraints_debug,
                        sys.vars[c],
                        j_fd[r][c].abs()
                    );
                }
            }
        }
    }
}

#[test]
fn test_gradient_validation_all_variants() {
    let mut fixture_tags = HashSet::new();

    // Helper macro to record tag and validate fixture
    macro_rules! check_sketch {
        ($sk:expr, $is_shared:expr, $seed:expr) => {{
            let sk = &$sk;
            for r in sk.constraints().values() {
                fixture_tags.insert(variant_tag(&r.constraint));
            }
            validate_fixture(sk, $is_shared, $seed);
        }};
        ($sk:expr, $seed:expr) => {
            check_sketch!($sk, false, $seed)
        };
    }

    // 1. Coincident
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        let p2 = add_pt(&mut sk, 0.04, 0.05);
        sk.add_constraint(Constraint::Coincident { a: p1, b: p2 });
        check_sketch!(sk, 101);
    }

    // 2. Horizontal
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.01, 0.02, 0.06, 0.03);
        sk.add_constraint(Constraint::Horizontal { line: l });
        check_sketch!(sk, 102);
    }

    // 3. Vertical
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.02, 0.01, 0.03, 0.07);
        sk.add_constraint(Constraint::Vertical { line: l });
        check_sketch!(sk, 103);
    }

    // 4. HorizontalPoints
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.01, 0.04);
        let p2 = add_pt(&mut sk, 0.07, 0.05);
        sk.add_constraint(Constraint::HorizontalPoints { a: p1, b: p2 });
        check_sketch!(sk, 104);
    }

    // 5. VerticalPoints
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.03, 0.01);
        let p2 = add_pt(&mut sk, 0.04, 0.08);
        sk.add_constraint(Constraint::VerticalPoints { a: p1, b: p2 });
        check_sketch!(sk, 105);
    }

    // 6. Parallel
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l1) = sk.add_line(0.01, 0.02, 0.05, 0.05);
        let (_, _, l2) = sk.add_line(0.02, 0.01, 0.06, 0.04);
        sk.add_constraint(Constraint::Parallel { a: l1, b: l2 });
        check_sketch!(sk, 106);
    }

    // 7. Perpendicular
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l1) = sk.add_line(0.01, 0.02, 0.05, 0.05);
        let (_, _, l2) = sk.add_line(0.02, 0.06, 0.05, 0.02);
        sk.add_constraint(Constraint::Perpendicular { a: l1, b: l2 });
        check_sketch!(sk, 107);
    }

    // 8. Equal (Line/Line, Circle/Circle, Arc/Arc, Circle/Arc)
    {
        // Line/Line
        let mut sk = Draft::seeded(1);
        let (_, _, l1) = sk.add_line(0.01, 0.01, 0.04, 0.05);
        let (_, _, l2) = sk.add_line(0.05, 0.02, 0.08, 0.06);
        sk.add_constraint(Constraint::Equal { a: l1, b: l2 });
        check_sketch!(sk, 108);

        // Circle/Circle
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.03, 0.02);
        let (_, cr2) = sk.add_circle(0.08, 0.08, 0.025);
        sk.add_constraint(Constraint::Equal { a: cr1, b: cr2 });
        check_sketch!(sk, 109);

        // Arc/Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a1) = sk.add_arc(0.03, 0.03, 0.05, 0.03, 0.03, 0.05);
        let (_, _, _, a2) = sk.add_arc(0.08, 0.08, 0.10, 0.08, 0.08, 0.10);
        sk.add_constraint(Constraint::Equal { a: a1, b: a2 });
        check_sketch!(sk, 110);

        // Circle/Arc
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.03, 0.02);
        let (_, _, _, a2) = sk.add_arc(0.08, 0.08, 0.10, 0.08, 0.08, 0.10);
        sk.add_constraint(Constraint::Equal { a: cr1, b: a2 });
        check_sketch!(sk, 111);
    }

    // 9. Concentric (Circle/Circle, Arc/Arc, Circle/Arc)
    {
        // Circle/Circle
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.05, 0.05, 0.02);
        let (_, cr2) = sk.add_circle(0.055, 0.048, 0.035);
        sk.add_constraint(Constraint::Concentric { a: cr1, b: cr2 });
        check_sketch!(sk, 112);

        // Arc/Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a1) = sk.add_arc(0.05, 0.05, 0.07, 0.05, 0.05, 0.07);
        let (_, _, _, a2) = sk.add_arc(0.052, 0.048, 0.085, 0.048, 0.052, 0.085);
        sk.add_constraint(Constraint::Concentric { a: a1, b: a2 });
        check_sketch!(sk, 113);

        // Circle/Arc
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.05, 0.05, 0.02);
        let (_, _, _, a2) = sk.add_arc(0.052, 0.048, 0.085, 0.048, 0.052, 0.085);
        sk.add_constraint(Constraint::Concentric { a: cr1, b: a2 });
        check_sketch!(sk, 114);
    }

    // 10. Tangent (Line/Circle, Line/Arc, Shared Point)
    {
        // Line/Circle
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.01, 0.05, 0.08, 0.05);
        let (_, cr) = sk.add_circle(0.04, 0.08, 0.03);
        sk.add_constraint(Constraint::Tangent {
            line: l,
            circle: cr,
        });
        check_sketch!(sk, 115);

        // Line/Arc
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.01, 0.05, 0.08, 0.05);
        let (_, _, _, a) = sk.add_arc(0.04, 0.08, 0.04, 0.05, 0.07, 0.08);
        sk.add_constraint(Constraint::Tangent { line: l, circle: a });
        check_sketch!(sk, 116);

        // Shared point: line start is circle center
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.04, 0.08);
        let p2 = add_pt(&mut sk, 0.09, 0.08);
        let l = sk.add_entity(Entity::Line { start: p1, end: p2 });
        let cr = sk.add_entity(Entity::Circle {
            center: p1,
            radius: 0.03,
        });
        sk.add_constraint(Constraint::Tangent {
            line: l,
            circle: cr,
        });
        check_sketch!(sk, true, 117);

        // Line/Arc joined at the line's end, which is the arc's start: the
        // first-order `(e − c)·d̂` row, off-axis and off-tangent.
        let mut sk = Draft::seeded(1);
        let a = add_pt(&mut sk, 0.012, 0.047);
        let e = add_pt(&mut sk, 0.043, 0.052);
        let c = add_pt(&mut sk, 0.038, 0.083);
        let f = add_pt(&mut sk, 0.068, 0.079);
        let l = sk.add_entity(Entity::Line { start: a, end: e });
        let arc = sk.add_entity(Entity::Arc {
            center: c,
            start: e,
            end: f,
        });
        sk.add_constraint(Constraint::Tangent {
            line: l,
            circle: arc,
        });
        check_sketch!(sk, 1180);
    }

    // 11. TangentCircles (Ext/Int, Arc/Arc, Circle/Arc)
    {
        // Circle/Circle External
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.04, 0.02);
        let (_, cr2) = sk.add_circle(0.08, 0.04, 0.03);
        sk.add_constraint(Constraint::TangentCircles { a: cr1, b: cr2 });
        check_sketch!(sk, 118);

        // Circle/Circle Internal
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.04, 0.05);
        let (_, cr2) = sk.add_circle(0.05, 0.04, 0.03);
        sk.add_constraint(Constraint::TangentCircles { a: cr1, b: cr2 });
        check_sketch!(sk, 119);

        // Arc/Arc External
        let mut sk = Draft::seeded(1);
        let (_, _, _, a1) = sk.add_arc(0.03, 0.04, 0.05, 0.04, 0.03, 0.06);
        let (_, _, _, a2) = sk.add_arc(0.09, 0.04, 0.09, 0.07, 0.12, 0.04);
        sk.add_constraint(Constraint::TangentCircles { a: a1, b: a2 });
        check_sketch!(sk, 120);

        // Circle/Arc
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.04, 0.02);
        let (_, _, _, a2) = sk.add_arc(0.09, 0.04, 0.09, 0.07, 0.12, 0.04);
        sk.add_constraint(Constraint::TangentCircles { a: cr1, b: a2 });
        check_sketch!(sk, 121);
    }

    // 12. Symmetric (Normal & Shared Point)
    {
        // Normal
        let mut sk = Draft::seeded(1);
        let (_, _, mirror) = sk.add_line(0.02, 0.01, 0.02, 0.08);
        let pa = add_pt(&mut sk, 0.05, 0.04);
        let pb = add_pt(&mut sk, 0.01, 0.04);
        sk.add_constraint(Constraint::Symmetric {
            a: pa,
            b: pb,
            mirror,
        });
        check_sketch!(sk, 122);

        // Shared point: pa is mirror.start
        let mut sk = Draft::seeded(1);
        let ms = add_pt(&mut sk, 0.02, 0.01);
        let me = add_pt(&mut sk, 0.02, 0.08);
        let mirror = sk.add_entity(Entity::Line { start: ms, end: me });
        let pb = add_pt(&mut sk, 0.01, 0.04);
        sk.add_constraint(Constraint::Symmetric {
            a: ms,
            b: pb,
            mirror,
        });
        check_sketch!(sk, true, 123);
    }

    // 13. Distance
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        let p2 = add_pt(&mut sk, 0.06, 0.07);
        sk.add_constraint(Constraint::Distance {
            a: p1,
            b: p2,
            value: 0.0565,
        });
        check_sketch!(sk, 124);
    }

    // 14. HorizontalDistance
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        let p2 = add_pt(&mut sk, 0.07, 0.08);
        sk.add_constraint(Constraint::HorizontalDistance {
            a: p1,
            b: p2,
            value: 0.05,
        });
        check_sketch!(sk, 125);
    }

    // 15. VerticalDistance
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        let p2 = add_pt(&mut sk, 0.07, 0.08);
        sk.add_constraint(Constraint::VerticalDistance {
            a: p1,
            b: p2,
            value: 0.05,
        });
        check_sketch!(sk, 126);
    }

    // 16. DistancePointLine
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.01, 0.02, 0.07, 0.02);
        let p = add_pt(&mut sk, 0.04, 0.06);
        sk.add_constraint(Constraint::DistancePointLine {
            point: p,
            line: l,
            value: 0.04,
        });
        check_sketch!(sk, 127);
    }

    // 17. DistanceParallelLines
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l1) = sk.add_line(0.01, 0.02, 0.06, 0.02);
        let (_, _, l2) = sk.add_line(0.02, 0.05, 0.07, 0.05);
        sk.add_constraint(Constraint::DistanceParallelLines {
            a: l1,
            b: l2,
            value: 0.03,
        });
        check_sketch!(sk, 128);
    }

    // 18. Angle
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l1) = sk.add_line(0.01, 0.02, 0.05, 0.02);
        let (_, _, l2) = sk.add_line(0.02, 0.03, 0.05, 0.07);
        sk.add_constraint(Constraint::Angle {
            a: l1,
            b: l2,
            value: 0.93,
        });
        check_sketch!(sk, 129);
    }

    // 19. Radius (Circle / Arc)
    {
        // Circle
        let mut sk = Draft::seeded(1);
        let (_, cr) = sk.add_circle(0.04, 0.04, 0.025);
        sk.add_constraint(Constraint::Radius {
            target: cr,
            value: 0.025,
        });
        check_sketch!(sk, 130);

        // Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a) = sk.add_arc(0.04, 0.04, 0.065, 0.04, 0.04, 0.065);
        sk.add_constraint(Constraint::Radius {
            target: a,
            value: 0.025,
        });
        check_sketch!(sk, 131);
    }

    // 20. Diameter (Circle / Arc)
    {
        // Circle
        let mut sk = Draft::seeded(1);
        let (_, cr) = sk.add_circle(0.04, 0.04, 0.025);
        sk.add_constraint(Constraint::Diameter {
            target: cr,
            value: 0.050,
        });
        check_sketch!(sk, 132);

        // Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a) = sk.add_arc(0.04, 0.04, 0.065, 0.04, 0.04, 0.065);
        sk.add_constraint(Constraint::Diameter {
            target: a,
            value: 0.050,
        });
        check_sketch!(sk, 133);
    }

    // 21. PointOnLine
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.01, 0.02, 0.07, 0.06);
        let p = add_pt(&mut sk, 0.04, 0.04);
        sk.add_constraint(Constraint::PointOnLine { point: p, line: l });
        check_sketch!(sk, 134);
    }

    // 22. PointOnCircle (Circle / Arc)
    {
        // Circle
        let mut sk = Draft::seeded(1);
        let (_, cr) = sk.add_circle(0.04, 0.04, 0.03);
        let p = add_pt(&mut sk, 0.04, 0.07);
        sk.add_constraint(Constraint::PointOnCircle {
            point: p,
            circle: cr,
        });
        check_sketch!(sk, 135);

        // Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a) = sk.add_arc(0.04, 0.04, 0.07, 0.04, 0.04, 0.07);
        let p = add_pt(&mut sk, 0.0612, 0.0612);
        sk.add_constraint(Constraint::PointOnCircle {
            point: p,
            circle: a,
        });
        check_sketch!(sk, 136);
    }

    // 23. Midpoint
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.02, 0.02, 0.08, 0.06);
        let p = add_pt(&mut sk, 0.05, 0.04);
        sk.add_constraint(Constraint::Midpoint { point: p, line: l });
        check_sketch!(sk, 137);
    }

    // 24. Fix
    {
        let mut sk = Draft::seeded(1);
        let p = add_pt(&mut sk, 0.04, 0.05);
        sk.add_constraint(Constraint::Fix {
            point: p,
            x: 0.04,
            y: 0.05,
        });
        check_sketch!(sk, 138);
    }

    // 25. SymmetricPoints (Normal & Shared Point)
    {
        // Normal
        let mut sk = Draft::seeded(1);
        let pa = add_pt(&mut sk, 0.02, 0.03);
        let pb = add_pt(&mut sk, 0.08, 0.07);
        let pc = add_pt(&mut sk, 0.05, 0.05);
        sk.add_constraint(Constraint::SymmetricPoints {
            a: pa,
            b: pb,
            center: pc,
        });
        check_sketch!(sk, 139);

        // Shared point: pa == pc
        let mut sk = Draft::seeded(1);
        let pa = add_pt(&mut sk, 0.05, 0.05);
        let pb = add_pt(&mut sk, 0.05, 0.05);
        sk.add_constraint(Constraint::SymmetricPoints {
            a: pa,
            b: pb,
            center: pa,
        });
        check_sketch!(sk, true, 140);
    }

    // 26. PointOnPerpBisector
    {
        let mut sk = Draft::seeded(1);
        let pa = add_pt(&mut sk, 0.02, 0.02);
        let pb = add_pt(&mut sk, 0.08, 0.02);
        let p = add_pt(&mut sk, 0.05, 0.06);
        sk.add_constraint(Constraint::PointOnPerpBisector {
            point: p,
            a: pa,
            b: pb,
        });
        check_sketch!(sk, 141);
    }

    // 27. ArcLength
    {
        let mut sk = Draft::seeded(1);
        let (_, _, _, a) = sk.add_arc(0.05, 0.05, 0.08, 0.05, 0.05, 0.08);
        sk.add_constraint(Constraint::ArcLength {
            arc: a,
            value: 0.03 * std::f64::consts::FRAC_PI_2,
        });
        check_sketch!(sk, 142);
    }

    // 28. Block (Point, Line, Circle, Arc)
    {
        // Point
        let mut sk = Draft::seeded(1);
        let p = add_pt(&mut sk, 0.04, 0.05);
        let ent = sk.add_entity(Entity::Point(p));
        sk.add_constraint(Constraint::Block { entity: ent });
        check_sketch!(sk, 143);

        // Line
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.02, 0.02, 0.07, 0.06);
        sk.add_constraint(Constraint::Block { entity: l });
        check_sketch!(sk, 144);

        // Circle
        let mut sk = Draft::seeded(1);
        let (_, cr) = sk.add_circle(0.04, 0.04, 0.03);
        sk.add_constraint(Constraint::Block { entity: cr });
        check_sketch!(sk, 145);

        // Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a) = sk.add_arc(0.04, 0.04, 0.07, 0.04, 0.04, 0.07);
        sk.add_constraint(Constraint::Block { entity: a });
        check_sketch!(sk, 146);
    }

    // 29. AnglePoints
    {
        let mut sk = Draft::seeded(1);
        let pa = add_pt(&mut sk, 0.06, 0.03);
        let pv = add_pt(&mut sk, 0.03, 0.03);
        let pb = add_pt(&mut sk, 0.05, 0.07);
        sk.add_constraint(Constraint::AnglePoints {
            a: pa,
            vertex: pv,
            b: pb,
            value: 0.93,
        });
        check_sketch!(sk, 147);
    }

    // 30. DistanceToAxisX
    {
        let mut sk = Draft::seeded(1);
        let p = add_pt(&mut sk, 0.04, 0.05);
        sk.add_constraint(Constraint::DistanceToAxisX {
            point: p,
            value: 0.04,
        });
        check_sketch!(sk, 148);
    }

    // 31. DistanceToAxisY
    {
        let mut sk = Draft::seeded(1);
        let p = add_pt(&mut sk, 0.04, 0.05);
        sk.add_constraint(Constraint::DistanceToAxisY {
            point: p,
            value: 0.05,
        });
        check_sketch!(sk, 149);
    }

    // 32. DistanceCircleCircle (Ext/Int, Circle/Arc)
    {
        // Circle/Circle External
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.04, 0.02);
        let (_, cr2) = sk.add_circle(0.09, 0.04, 0.025);
        sk.add_constraint(Constraint::DistanceCircleCircle {
            a: cr1,
            b: cr2,
            value: 0.015,
        });
        check_sketch!(sk, 150);

        // Circle/Circle Internal
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.04, 0.06);
        let (_, cr2) = sk.add_circle(0.06, 0.04, 0.02);
        sk.add_constraint(Constraint::DistanceCircleCircle {
            a: cr1,
            b: cr2,
            value: 0.01,
        });
        check_sketch!(sk, 151);

        // Arc/Arc External
        let mut sk = Draft::seeded(1);
        let (_, _, _, a1) = sk.add_arc(0.03, 0.04, 0.05, 0.04, 0.03, 0.06);
        let (_, _, _, a2) = sk.add_arc(0.09, 0.04, 0.09, 0.065, 0.115, 0.04);
        sk.add_constraint(Constraint::DistanceCircleCircle {
            a: a1,
            b: a2,
            value: 0.015,
        });
        check_sketch!(sk, 152);

        // Circle/Arc
        let mut sk = Draft::seeded(1);
        let (_, cr1) = sk.add_circle(0.03, 0.04, 0.02);
        let (_, _, _, a2) = sk.add_arc(0.09, 0.04, 0.09, 0.065, 0.115, 0.04);
        sk.add_constraint(Constraint::DistanceCircleCircle {
            a: cr1,
            b: a2,
            value: 0.015,
        });
        check_sketch!(sk, 153);
    }

    // 33. DistancePointCircle (Circle / Arc)
    {
        // Circle
        let mut sk = Draft::seeded(1);
        let (_, cr) = sk.add_circle(0.04, 0.04, 0.025);
        let p = add_pt(&mut sk, 0.04, 0.08);
        sk.add_constraint(Constraint::DistancePointCircle {
            point: p,
            circle: cr,
            value: 0.015,
        });
        check_sketch!(sk, 154);

        // Arc
        let mut sk = Draft::seeded(1);
        let (_, _, _, a) = sk.add_arc(0.04, 0.04, 0.065, 0.04, 0.04, 0.065);
        let p = add_pt(&mut sk, 0.04, 0.08);
        sk.add_constraint(Constraint::DistancePointCircle {
            point: p,
            circle: a,
            value: 0.015,
        });
        check_sketch!(sk, 155);
    }

    // 34. SnellsLaw (Line boundary, Circle boundary, Shared point)
    #[cfg(feature = "snells-law")]
    {
        // Line boundary
        let mut sk = Draft::seeded(1);
        let s1 = add_pt(&mut sk, 0.02, 0.05);
        let v = add_pt(&mut sk, 0.04, 0.04);
        let e2 = add_pt(&mut sk, 0.06, 0.02);
        let (_, _, b) = sk.add_line(0.04, 0.01, 0.04, 0.07);
        sk.add_constraint(Constraint::SnellsLaw {
            ray1_start: s1,
            ray1_end: v,
            ray2_end: e2,
            boundary: b,
            ratio: 1.33,
        });
        check_sketch!(sk, 156);

        // Circle boundary
        let mut sk = Draft::seeded(1);
        let s1 = add_pt(&mut sk, 0.02, 0.05);
        let v = add_pt(&mut sk, 0.04, 0.04);
        let e2 = add_pt(&mut sk, 0.06, 0.02);
        let (_, b) = sk.add_circle(0.04, 0.01, 0.03);
        sk.add_constraint(Constraint::SnellsLaw {
            ray1_start: s1,
            ray1_end: v,
            ray2_end: e2,
            boundary: b,
            ratio: 1.33,
        });
        check_sketch!(sk, 157);

        // Shared point: vertex is boundary circle center
        let mut sk = Draft::seeded(1);
        let s1 = add_pt(&mut sk, 0.02, 0.05);
        let v = add_pt(&mut sk, 0.04, 0.04);
        let e2 = add_pt(&mut sk, 0.06, 0.02);
        let b = sk.add_entity(Entity::Circle {
            center: v,
            radius: 0.03,
        });
        sk.add_constraint(Constraint::SnellsLaw {
            ray1_start: s1,
            ray1_end: v,
            ray2_end: e2,
            boundary: b,
            ratio: 1.33,
        });
        check_sketch!(sk, true, 158);
    }

    // 35. PointOnDatum (Origin, AxisX, AxisY)
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        sk.add_constraint(Constraint::PointOnDatum {
            point: p1,
            datum: arrix_sketch::DatumEntity::AxisX,
        });
        check_sketch!(sk, 159);

        let mut sk2 = Draft::seeded(1);
        let p2 = add_pt(&mut sk2, 0.02, 0.03);
        sk2.add_constraint(Constraint::PointOnDatum {
            point: p2,
            datum: arrix_sketch::DatumEntity::AxisY,
        });
        check_sketch!(sk2, 160);

        let mut sk3 = Draft::seeded(1);
        let p3 = add_pt(&mut sk3, 0.02, 0.03);
        sk3.add_constraint(Constraint::PointOnDatum {
            point: p3,
            datum: arrix_sketch::DatumEntity::Origin,
        });
        check_sketch!(sk3, 161);
    }

    // 36. DistanceToDatum (Origin, AxisX, AxisY)
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        sk.add_constraint(Constraint::DistanceToDatum {
            point: p1,
            datum: arrix_sketch::DatumEntity::AxisX,
            value: 0.03,
        });
        check_sketch!(sk, 162);

        let mut sk2 = Draft::seeded(1);
        let p2 = add_pt(&mut sk2, 0.02, 0.03);
        sk2.add_constraint(Constraint::DistanceToDatum {
            point: p2,
            datum: arrix_sketch::DatumEntity::AxisY,
            value: 0.02,
        });
        check_sketch!(sk2, 163);

        let mut sk3 = Draft::seeded(1);
        let p3 = add_pt(&mut sk3, 0.02, 0.03);
        sk3.add_constraint(Constraint::DistanceToDatum {
            point: p3,
            datum: arrix_sketch::DatumEntity::Origin,
            value: 0.036,
        });
        check_sketch!(sk3, 164);
    }

    // 37. AngleWithDatum (AxisX, AxisY)
    {
        let mut sk = Draft::seeded(1);
        let (_, _, l) = sk.add_line(0.01, 0.01, 0.05, 0.04);
        sk.add_constraint(Constraint::AngleWithDatum {
            line: l,
            datum: arrix_sketch::DatumEntity::AxisX,
            value: 0.6,
        });
        check_sketch!(sk, 165);

        let mut sk2 = Draft::seeded(1);
        let (_, _, l2) = sk2.add_line(0.01, 0.01, 0.05, 0.04);
        sk2.add_constraint(Constraint::AngleWithDatum {
            line: l2,
            datum: arrix_sketch::DatumEntity::AxisY,
            value: 0.9,
        });
        check_sketch!(sk2, 166);
    }

    // 38. SymmetricAcrossDatum (AxisX, AxisY, Origin)
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        let p2 = add_pt(&mut sk, 0.02, -0.03);
        sk.add_constraint(Constraint::SymmetricAcrossDatum {
            a: p1,
            b: p2,
            datum: arrix_sketch::DatumEntity::AxisX,
        });
        check_sketch!(sk, 167);

        let mut sk2 = Draft::seeded(1);
        let pa = add_pt(&mut sk2, 0.02, 0.03);
        let pb = add_pt(&mut sk2, -0.02, 0.03);
        sk2.add_constraint(Constraint::SymmetricAcrossDatum {
            a: pa,
            b: pb,
            datum: arrix_sketch::DatumEntity::AxisY,
        });
        check_sketch!(sk2, 168);

        let mut sk3 = Draft::seeded(1);
        let pc1 = add_pt(&mut sk3, 0.02, 0.03);
        let pc2 = add_pt(&mut sk3, -0.02, -0.03);
        sk3.add_constraint(Constraint::SymmetricAcrossDatum {
            a: pc1,
            b: pc2,
            datum: arrix_sketch::DatumEntity::Origin,
        });
        check_sketch!(sk3, 169);
    }

    // 39. CoincidentToDatum (Origin, AxisX, AxisY)
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.03);
        sk.add_constraint(Constraint::CoincidentToDatum {
            point: p1,
            datum: arrix_sketch::DatumEntity::Origin,
        });
        check_sketch!(sk, 170);

        let mut sk2 = Draft::seeded(1);
        let p2 = add_pt(&mut sk2, 0.02, 0.03);
        sk2.add_constraint(Constraint::CoincidentToDatum {
            point: p2,
            datum: arrix_sketch::DatumEntity::AxisX,
        });
        check_sketch!(sk2, 171);

        let mut sk3 = Draft::seeded(1);
        let p3 = add_pt(&mut sk3, 0.02, 0.03);
        sk3.add_constraint(Constraint::CoincidentToDatum {
            point: p3,
            datum: arrix_sketch::DatumEntity::AxisY,
        });
        check_sketch!(sk3, 172);
    }

    // 40. PointOnEllipse
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, _, e) = sk.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        let p = add_pt(&mut sk, 0.04, 0.07);
        sk.add_constraint(Constraint::PointOnEllipse {
            point: p,
            ellipse: e,
        });
        check_sketch!(sk, 173);
    }

    // 41. TangentLineEllipse
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, _, e) = sk.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        let (_, _, l) = sk.add_line(0.01, 0.07, 0.07, 0.07);
        sk.add_constraint(Constraint::TangentLineEllipse {
            line: l,
            ellipse: e,
        });
        check_sketch!(sk, 174);
    }

    // 42. InternalAlignment
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, _, e) = sk.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        let f1 = add_pt(&mut sk, 0.08, 0.04);
        sk.add_constraint(Constraint::InternalAlignment {
            ellipse: e,
            alignment: arrix_sketch::AlignmentKind::Focus1(f1),
        });
        check_sketch!(sk, 175);

        let mut sk2 = Draft::seeded(1);
        let (_, _, e2) = sk2.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        let min_end = add_pt(&mut sk2, 0.04, 0.07);
        sk2.add_constraint(Constraint::InternalAlignment {
            ellipse: e2,
            alignment: arrix_sketch::AlignmentKind::MinorRadiusEnd(min_end),
        });
        check_sketch!(sk2, 176);
    }

    // 43. MinorRadius
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, _, e) = sk.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        sk.add_constraint(Constraint::MinorRadius {
            ellipse: e,
            value: 0.03,
        });
        check_sketch!(sk, 177);
    }

    // 44. MajorRadius
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, _, e) = sk.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        sk.add_constraint(Constraint::MajorRadius {
            ellipse: e,
            value: 0.05,
        });
        check_sketch!(sk, 178);
    }

    // 45. PointOnBSpline
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, bspline) =
            sk.add_clamped_bspline(&[[0.01, 0.02], [0.03, 0.06], [0.06, 0.06], [0.08, 0.02]], 3);
        let p_eval = add_pt(&mut sk, 0.045, 0.045);
        sk.add_constraint(Constraint::PointOnBSpline {
            point: p_eval,
            bspline,
            u: 0.5,
        });
        check_sketch!(sk, 179);
    }

    // 46. BSplineTangent
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, bspline) =
            sk.add_clamped_bspline(&[[0.01, 0.02], [0.03, 0.06], [0.06, 0.06], [0.08, 0.02]], 3);
        let (_, _, l) = sk.add_line(0.02, 0.06, 0.07, 0.06);
        sk.add_constraint(Constraint::BSplineTangent {
            line: l,
            bspline,
            u: 0.5,
        });
        check_sketch!(sk, 180);
    }

    // 47. BSplineCurvature
    #[cfg(feature = "conics")]
    {
        let mut sk = Draft::seeded(1);
        let (_, bspline) =
            sk.add_clamped_bspline(&[[0.01, 0.02], [0.03, 0.06], [0.06, 0.06], [0.08, 0.02]], 3);
        sk.add_constraint(Constraint::BSplineCurvature {
            bspline,
            u: 0.5,
            value: 20.0,
        });
        check_sketch!(sk, 181);
    }

    // Block on Ellipse and BSpline
    #[cfg(feature = "conics")]
    {
        let mut sk_be = Draft::seeded(1);
        let (_, _, e) = sk_be.add_ellipse(0.04, 0.04, 0.09, 0.04, 0.03);
        sk_be.add_constraint(Constraint::Block { entity: e });
        check_sketch!(sk_be, 182);

        let mut sk_bs = Draft::seeded(1);
        let (_, bspline) =
            sk_bs.add_clamped_bspline(&[[0.01, 0.02], [0.03, 0.06], [0.06, 0.06], [0.08, 0.02]], 3);
        sk_bs.add_constraint(Constraint::Block { entity: bspline });
        check_sketch!(sk_bs, 183);
    }

    // Exhaustiveness check: every constraint variant compiled in must have
    // been visited.
    let all_tags_set: HashSet<&str> = ALL_TAGS
        .iter()
        .chain(CONIC_TAGS)
        .chain(SNELL_TAGS)
        .copied()
        .collect();
    assert_eq!(
        fixture_tags, all_tags_set,
        "Not all constraint variants were tested in fixtures!"
    );
}

#[test]
fn test_degenerate_cases() {
    // 1. CircleRadius var at negative x: column flips sign
    {
        let mut sk = Draft::seeded(1);
        let (_, circle) = sk.add_circle(0.04, 0.04, 0.03);
        let _blk = sk.add_constraint(Constraint::Block { entity: circle });

        let sys = System::build(&sk);
        let mut x = sys.pack(&sk);
        let r_col = sys.radius_col[&circle];
        x[r_col] = -0.03;

        let j_rows = jacobian(&sys, &sk, &x);
        let j = j_rows.to_dense();
        assert_eq!(j[2][r_col], -1.0);
    }

    // 2. CircleRadius var at |x| <= LENGTH_TOLERANCE: column is zero
    {
        let mut sk = Draft::seeded(1);
        let (_, circle) = sk.add_circle(0.04, 0.04, 0.03);
        let _blk = sk.add_constraint(Constraint::Block { entity: circle });

        let sys = System::build(&sk);
        let mut x = sys.pack(&sk);
        let r_col = sys.radius_col[&circle];
        x[r_col] = 1e-12;
        // `jacobian`'s contract is that the sketch is already unpacked at `x`;
        // `unpack_into` floors the radius at LENGTH_TOLERANCE, exactly as the
        // solver would mid-iteration.
        sys.unpack_into(&x, &mut sk);

        let j_rows = jacobian(&sys, &sk, &x);
        let j = j_rows.to_dense();
        assert_eq!(j[2][r_col], 0.0);
    }

    // 3. Block with no snapshot: rows entirely empty
    {
        let mut sk = Draft::seeded(1);
        let p = add_pt(&mut sk, 0.04, 0.05);
        let ent = sk.add_entity(Entity::Point(p));
        let _cid = sk.add_constraint(Constraint::Block { entity: ent });
        let mut sys = System::build(&sk);
        sys.blocked_targets.clear(); // remove snapshot

        let x = sys.pack(&sk);
        let j_rows = jacobian(&sys, &sk, &x);
        assert_eq!(j_rows.rows.len(), 2);
        for row in &j_rows.rows {
            assert!(row.is_empty());
        }
    }

    // 4. A constraint on a missing entity: the sketch refuses it on insert
    //    (tests/model.rs), so the Jacobian never meets one.

    // 5. Zero-length mirror line: `reflect` short-circuits to R = P, so the
    //    rows are `a − b` and the mirror endpoints drop out entirely.
    {
        let mut sk = Draft::seeded(1);
        let ms = add_pt(&mut sk, 0.03, 0.03);
        let me = add_pt(&mut sk, 0.03, 0.03);
        let mirror = sk.add_entity(Entity::Line { start: ms, end: me });
        let pa = add_pt(&mut sk, 0.05, 0.04);
        let pb = add_pt(&mut sk, 0.01, 0.06);
        sk.add_constraint(Constraint::Symmetric {
            a: pa,
            b: pb,
            mirror,
        });

        let sys = System::build(&sk);
        let x = sys.pack(&sk);
        let j = jacobian(&sys, &sk, &x).to_dense();
        let (ca, cb) = (sys.point_col[&pa], sys.point_col[&pb]);
        let (cs, ce) = (sys.point_col[&ms], sys.point_col[&me]);
        assert_eq!(j[0][ca], 1.0);
        assert_eq!(j[0][cb], -1.0);
        assert_eq!(j[1][ca + 1], 1.0);
        assert_eq!(j[1][cb + 1], -1.0);
        for row in &j {
            for c in [cs, cs + 1, ce, ce + 1] {
                assert_eq!(row[c], 0.0, "degenerate mirror endpoint must not appear");
            }
        }
    }

    // 6. Coincident points under `Distance`: `d < LENGTH_TOLERANCE`, so the row
    //    is empty rather than NaN (`u = (a − b)/d` is undefined there).
    {
        let mut sk = Draft::seeded(1);
        let a = add_pt(&mut sk, 0.04, 0.05);
        let b = add_pt(&mut sk, 0.04, 0.05);
        sk.add_constraint(Constraint::Distance { a, b, value: 0.02 });

        let sys = System::build(&sk);
        let x = sys.pack(&sk);
        let j_rows = jacobian(&sys, &sk, &x);
        assert_eq!(j_rows.rows.len(), 1);
        assert!(
            j_rows.rows[0].is_empty(),
            "Distance at d = 0 must emit no gradient, got {:?}",
            j_rows.rows[0]
        );
    }

    // 7. Fixed points: no columns in Jacobian
    {
        let mut sk = Draft::seeded(1);
        let p1 = add_pt(&mut sk, 0.02, 0.02);
        let p2 = add_pt(&mut sk, 0.05, 0.02);
        sk.point_mut(p1).unwrap().fixed = true;
        sk.add_constraint(Constraint::Coincident { a: p1, b: p2 });

        let sys = System::build(&sk);
        assert_eq!(sys.n_vars(), 2);
        let x = sys.pack(&sk);
        let j_rows = jacobian(&sys, &sk, &x);
        let j = j_rows.to_dense();
        assert_eq!(j[0][0], -1.0);
        assert_eq!(j[0][1], 0.0);
        assert_eq!(j[1][0], 0.0);
        assert_eq!(j[1][1], -1.0);
    }
}

// ---------------------------------------------------------------------------
// Randomized stress pass
// ---------------------------------------------------------------------------
//
// The fixture suite above is hand-built and therefore hand-limited: every
// fixture sits in the first quadrant, and each one picks exactly one side of
// every frozen sign and branch. This pass keeps the *topology* fixed (random
// topology degenerates far too often to be a useful oracle) and randomizes only
// the geometry — across all four quadrants, both signs of every `.signum()`
// freeze, and both sides of every branch selector.
//
// Because random geometry can land arbitrarily close to a cusp, finite
// differences are not unconditionally a valid oracle here. So each entry is
// differenced at two step sizes and compared only where the two agree: a cusp
// makes them diverge, and that entry is skipped rather than asserted. This is
// what lets the geometry be unconstrained without the invariants
// `check_branch_margins` enforces for the curated fixtures.

/// Two-sided differences over a single variable column.
fn fd_jacobian(sys: &System, sketch: &Sketch, x: &[f64], h: f64) -> Vec<Vec<f64>> {
    let n = sys.n_vars();
    let m = sys.n_residuals();
    let mut j = vec![vec![0.0; n]; m];
    let mut perturbed = sketch.clone();
    for col in 0..n {
        let v = sys.vars[col];
        let base = x[col];
        set_var(&mut perturbed, v, base + h);
        let r_plus = sys.residuals_of(&perturbed);
        set_var(&mut perturbed, v, base - h);
        let r_minus = sys.residuals_of(&perturbed);
        set_var(&mut perturbed, v, base);
        for row in 0..m {
            j[row][col] = (r_plus[row] - r_minus[row]) / (2.0 * h);
        }
    }
    j
}

/// Compares the analytic Jacobian against finite differences everywhere the
/// finite differences are trustworthy. Returns `(checked, skipped)`.
fn stress_compare(sketch: &Sketch, label: &str, stats: &mut (usize, usize)) {
    let sys = System::build(sketch);
    let x = sys.pack(sketch);
    let (n, m) = (sys.n_vars(), sys.n_residuals());
    if n == 0 || m == 0 {
        return;
    }

    let analytic = jacobian(&sys, sketch, &x).to_dense();
    let coarse = fd_jacobian(&sys, sketch, &x, 1e-6);
    let fine = fd_jacobian(&sys, sketch, &x, 1e-7);

    for r in 0..m {
        for c in 0..n {
            let (a, b_coarse, b_fine) = (analytic[r][c], coarse[r][c], fine[r][c]);
            // Two step sizes disagreeing means the residual is not locally
            // smooth in this variable — a cusp, a branch edge, or a clamp.
            // Finite differences are not an oracle there; the curated fixtures
            // cover those cases with exact expected values instead.
            if (b_coarse - b_fine).abs() > 1e-4 * (1.0 + b_coarse.abs()) {
                stats.1 += 1;
                continue;
            }
            stats.0 += 1;
            assert!(
                (a - b_coarse).abs() <= 2e-6 * (1.0 + b_coarse.abs()),
                "{label}: row {r} col {c} (var {:?}): analytic = {a}, fd = {b_coarse} \
                 (fd at h/10 = {b_fine})",
                sys.vars[c]
            );
        }
    }
}

#[test]
fn test_gradient_stress_randomized_geometry() {
    const TRIALS: usize = 300;
    let mut rng = XorShift64::new(0x1234_5678_9abc_def1);
    let mut stats = (0usize, 0usize);

    let pt = |sk: &mut Draft, rng: &mut XorShift64| {
        sk.add_point(Point::new(rng.range(-0.1, 0.1), rng.range(-0.1, 0.1)))
    };
    let coord = |rng: &mut XorShift64| rng.range(-0.1, 0.1);

    for i in 0..TRIALS {
        // Point-only constraints.
        {
            let mut sk = Draft::seeded(1);
            let a = pt(&mut sk, &mut rng);
            let b = pt(&mut sk, &mut rng);
            let v = pt(&mut sk, &mut rng);
            sk.add_constraint(Constraint::Coincident { a, b });
            sk.add_constraint(Constraint::HorizontalPoints { a, b });
            sk.add_constraint(Constraint::VerticalPoints { a, b });
            sk.add_constraint(Constraint::Distance {
                a,
                b,
                value: rng.range(0.01, 0.1),
            });
            sk.add_constraint(Constraint::HorizontalDistance { a, b, value: 0.03 });
            sk.add_constraint(Constraint::VerticalDistance { a, b, value: 0.03 });
            sk.add_constraint(Constraint::DistanceToAxisX {
                point: a,
                value: 0.02,
            });
            sk.add_constraint(Constraint::DistanceToAxisY {
                point: a,
                value: 0.02,
            });
            sk.add_constraint(Constraint::SymmetricPoints { a, b, center: v });
            sk.add_constraint(Constraint::PointOnPerpBisector { point: v, a, b });
            sk.add_constraint(Constraint::AnglePoints {
                a,
                vertex: v,
                b,
                value: 0.5,
            });
            sk.add_constraint(Constraint::Fix {
                point: v,
                x: 0.01,
                y: 0.02,
            });
            stress_compare(&sk, &format!("points #{i}"), &mut stats);
        }

        // Lines: angles, distances, reflection, blocking.
        {
            let mut sk = Draft::seeded(1);
            let (s1, e1, l1) = sk.add_line(
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
            );
            let (_, _, l2) = sk.add_line(
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
            );
            let q = pt(&mut sk, &mut rng);
            sk.add_constraint(Constraint::Horizontal { line: l1 });
            sk.add_constraint(Constraint::Vertical { line: l1 });
            sk.add_constraint(Constraint::Parallel { a: l1, b: l2 });
            sk.add_constraint(Constraint::Perpendicular { a: l1, b: l2 });
            sk.add_constraint(Constraint::Angle {
                a: l1,
                b: l2,
                value: 0.5,
            });
            sk.add_constraint(Constraint::Equal { a: l1, b: l2 });
            sk.add_constraint(Constraint::PointOnLine { point: q, line: l1 });
            sk.add_constraint(Constraint::DistancePointLine {
                point: q,
                line: l1,
                value: 0.02,
            });
            sk.add_constraint(Constraint::DistanceParallelLines {
                a: l1,
                b: l2,
                value: 0.02,
            });
            sk.add_constraint(Constraint::Midpoint { point: q, line: l2 });
            // Mirrors that share a point with the thing being mirrored.
            sk.add_constraint(Constraint::Symmetric {
                a: s1,
                b: q,
                mirror: l2,
            });
            sk.add_constraint(Constraint::Symmetric {
                a: q,
                b: e1,
                mirror: l1,
            });
            sk.add_constraint(Constraint::Block { entity: l2 });
            stress_compare(&sk, &format!("lines #{i}"), &mut stats);
        }

        // Circles and arcs: both clearance branches, both signs of sgn(ra − rb),
        // and the Circle/Arc radius duality in every position.
        {
            let mut sk = Draft::seeded(1);
            let (_, c1) = sk.add_circle(coord(&mut rng), coord(&mut rng), rng.range(0.005, 0.06));
            let (_, c2) = sk.add_circle(coord(&mut rng), coord(&mut rng), rng.range(0.005, 0.06));
            let (_, _, _, a1) = sk.add_arc(
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
            );
            let (_, _, l) = sk.add_line(
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
            );
            let q = pt(&mut sk, &mut rng);
            for &(x, y) in &[(c1, c2), (c1, a1), (a1, c2)] {
                sk.add_constraint(Constraint::Equal { a: x, b: y });
                sk.add_constraint(Constraint::Concentric { a: x, b: y });
                sk.add_constraint(Constraint::TangentCircles { a: x, b: y });
                sk.add_constraint(Constraint::DistanceCircleCircle {
                    a: x,
                    b: y,
                    value: 0.01,
                });
            }
            for &e in &[c1, a1] {
                sk.add_constraint(Constraint::Radius {
                    target: e,
                    value: 0.02,
                });
                sk.add_constraint(Constraint::Diameter {
                    target: e,
                    value: 0.04,
                });
                sk.add_constraint(Constraint::Tangent { line: l, circle: e });
                sk.add_constraint(Constraint::PointOnCircle {
                    point: q,
                    circle: e,
                });
                sk.add_constraint(Constraint::DistancePointCircle {
                    point: q,
                    circle: e,
                    value: 0.01,
                });
            }
            sk.add_constraint(Constraint::ArcLength {
                arc: a1,
                value: 0.02,
            });
            sk.add_constraint(Constraint::Block { entity: c1 });
            sk.add_constraint(Constraint::Block { entity: a1 });
            stress_compare(&sk, &format!("circles #{i}"), &mut stats);
        }

        // Refraction against every boundary kind, including the fallback
        // constant normal for a boundary that is neither line nor circle.
        #[cfg(feature = "snells-law")]
        {
            let mut sk = Draft::seeded(1);
            let s1 = pt(&mut sk, &mut rng);
            let v = pt(&mut sk, &mut rng);
            let e2 = pt(&mut sk, &mut rng);
            let (_, _, l) = sk.add_line(
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
            );
            let (_, c) = sk.add_circle(coord(&mut rng), coord(&mut rng), rng.range(0.005, 0.06));
            let (_, _, _, arc) = sk.add_arc(
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
                coord(&mut rng),
            );
            let point_entity = sk.add_entity(Entity::Point(v));
            for &boundary in &[l, c, arc, point_entity] {
                sk.add_constraint(Constraint::SnellsLaw {
                    ray1_start: s1,
                    ray1_end: v,
                    ray2_end: e2,
                    boundary,
                    ratio: 1.33,
                });
            }
            sk.add_constraint(Constraint::Block {
                entity: point_entity,
            });
            stress_compare(&sk, &format!("refraction #{i}"), &mut stats);
        }

        // Duplicate-column accumulation: every shape where one point reaches a
        // residual through two different paths, so its column is emitted twice
        // and must be summed rather than overwritten.
        {
            let mut sk = Draft::seeded(1);
            let centre = pt(&mut sk, &mut rng);
            let far = pt(&mut sk, &mut rng);
            let third = pt(&mut sk, &mut rng);
            let line = sk.add_entity(Entity::Line {
                start: centre,
                end: far,
            });
            let circle = sk.add_entity(Entity::Circle {
                center: centre,
                radius: rng.range(0.005, 0.05),
            });
            let arc = sk.add_entity(Entity::Arc {
                center: centre,
                start: far,
                end: third,
            });
            // Circle centred on one of the line's own endpoints.
            sk.add_constraint(Constraint::Tangent { line, circle });
            sk.add_constraint(Constraint::Tangent { line, circle: arc });
            sk.add_constraint(Constraint::PointOnCircle {
                point: centre,
                circle,
            });
            sk.add_constraint(Constraint::PointOnCircle {
                point: far,
                circle: arc,
            });
            // A point mirrored across a line it is an endpoint of.
            sk.add_constraint(Constraint::Symmetric {
                a: centre,
                b: far,
                mirror: line,
            });
            sk.add_constraint(Constraint::Symmetric {
                a: far,
                b: centre,
                mirror: line,
            });
            sk.add_constraint(Constraint::Midpoint {
                point: centre,
                line,
            });
            sk.add_constraint(Constraint::PointOnPerpBisector {
                point: centre,
                a: centre,
                b: far,
            });
            // A constraint whose two operands are the same entity.
            sk.add_constraint(Constraint::Parallel { a: line, b: line });
            sk.add_constraint(Constraint::Angle {
                a: line,
                b: line,
                value: 0.3,
            });
            sk.add_constraint(Constraint::Equal { a: line, b: line });
            // A refraction whose vertex is its own boundary's centre *and*
            // whose outgoing ray ends where the incoming one started.
            #[cfg(feature = "snells-law")]
            sk.add_constraint(Constraint::SnellsLaw {
                ray1_start: far,
                ray1_end: centre,
                ray2_end: far,
                boundary: circle,
                ratio: 1.33,
            });
            // Self-referential point constraints.
            sk.add_constraint(Constraint::Coincident { a: third, b: third });
            sk.add_constraint(Constraint::SymmetricPoints {
                a: third,
                b: third,
                center: third,
            });
            stress_compare(&sk, &format!("shared columns #{i}"), &mut stats);
        }
    }

    let (checked, skipped) = stats;
    println!("stress: {checked} entries checked, {skipped} skipped as non-smooth");
    // Guards against the comparison silently degenerating into a no-op if a
    // future change makes the smoothness filter reject everything.
    assert!(
        checked > 100_000 && skipped * 100 < checked,
        "stress pass checked too little: {checked} checked, {skipped} skipped"
    );
}

/// The drag pins' rows: a point pin's
/// two and a rim drag's radius pin, against finite differences, beside a
/// constraint that couples them to other columns.
#[test]
fn drag_pin_rows_match_finite_differences() {
    let mut sketch = Draft::seeded(1);
    let (center, circle) = sketch.add_circle(0.010, 0.020, 0.015);
    let (a, _, line) = sketch.add_line(-0.030, -0.010, 0.040, 0.005);
    sketch.add_constraint(Constraint::Tangent { line, circle });
    let sys = System::build_with_pins(
        &sketch,
        &[
            Pin::Point {
                point: a,
                x: -0.020,
                y: 0.0,
            },
            Pin::Radius {
                circle,
                value: 0.025,
            },
        ],
    );
    assert_eq!(
        sys.n_residuals(),
        1 + 2 + 1,
        "the tangent, then 2 + 1 pin rows"
    );
    let x = sys.pack(&sketch);
    let analytic = jacobian(&sys, &sketch, &x).to_dense();
    let fd = fd_jacobian(&sys, &sketch, &x, 1e-7);
    for (r, (ar, fr)) in analytic.iter().zip(&fd).enumerate() {
        for (c, (a, f)) in ar.iter().zip(fr).enumerate() {
            assert!(
                (a - f).abs() <= 1e-6 * (1.0 + f.abs()),
                "row {r} col {c} ({:?}): analytic {a}, fd {f}",
                sys.vars[c]
            );
        }
    }
    let _ = center;
}
