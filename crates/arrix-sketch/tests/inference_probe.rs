//! Probes for inference's commit rule (`plans/sketch-mode` step 2):
//! whether `Sketch::check_candidate`, run over a click's inferences one at
//! a time, is what the draw-time engine can commit through. The answers
//! are the rule written into `inference`'s module doc, established here,
//! not assumed:
//!
//! (a) a template drawn on the axis offers more constraints than it needs,
//!     and the filter keeps exactly one of them;
//! (b) an alignment in the coordinate the line's H lock already set is
//!     `Redundant`, in the other one it is `Ok`, and which of two
//!     equivalent candidates survives is decided by priority order alone;
//!     the search puts the lock first, so the lock is the one kept;
//! (c) `PointOnLine` and `PointOnCircle` hold a point on the line's
//!     extension and on an arc's full circle, so on-curve and extension
//!     snaps can commit them;
//! (d) `Tangent { line, circle }` with the line's end on the rim, no shared
//!     point, converges at the click and does not flip under a drag;
//! (e) what one verdict costs on a fully constrained plate: the preview's
//!     cache miss;
//! (f) `Parallel` and `Perpendicular` on the drawn line follow a turning
//!     reference without flipping, and a tangent snap commits `Tangent`
//!     alone: with the end on the rim as well, the two are one first-order
//!     row at the touching point.
//!
//! Millimetre-scale shapes throughout; lengths are in metres.

// `tests/` is never built for wasm (docs/CONCURRENCY-WASM.md §The browser
// build), so the std clock is fine here.
#[allow(clippy::disallowed_types, reason = "native-only verdict clock")]
use std::time::Instant;

use std::f64::consts::FRAC_PI_2;

use arrix_sketch::{
    CandidateVerdict, Constraint, DatumEntity, Diagnostics, Draft, DragSession, Entity, EntityId,
    Inferred, Point, PointId, Sketch, SketchStatus, SnapQuery, commit_inferences, snap_search,
};

const MM: f64 = 1e-3;

fn xy(s: &Sketch, p: PointId) -> (f64, f64) {
    let p = s.point(p).expect("the point exists");
    (p.x, p.y)
}

fn dof(s: &mut Sketch) -> i32 {
    let (result, d) = Diagnostics::evaluate(s);
    assert!(result.converged, "{}", d.message);
    d.dof
}

/// The rule under test: a click's inferences, in priority order, each
/// checked against the sketch with the earlier kept ones already in. `Ok`
/// is added, `Redundant` and `Conflicting` are dropped. Returns every
/// verdict, in order.
fn bind(s: &mut Draft, candidates: &[Constraint]) -> Vec<CandidateVerdict> {
    candidates
        .iter()
        .map(|c| {
            let verdict = s.check_candidate(c);
            if verdict.is_ok() {
                s.add_constraint(c.clone());
                s.solve();
            }
            verdict
        })
        .collect()
}

// (a) The slot on the axis.

struct Slot {
    sketch: Draft,
    centres: [PointId; 2],
    axis: EntityId,
}

/// What the slot template builds for a horizontal axis, sized: a
/// construction axis between the centres, two horizontal sides, each arc's
/// ends vertical over its centre, the length and one arc's diameter
/// driven. Drawn from (20, 0) to (40, 0), 5 mm wide.
fn slot() -> Slot {
    let (x0, x1, r) = (20.0 * MM, 40.0 * MM, 2.5 * MM);
    let mut s = Draft::seeded(1);
    let [c0, c1] = [x0, x1].map(|x| s.add_point(Point::new(x, 0.0)));
    let axis = s.add_entity(Entity::Line { start: c0, end: c1 });
    s.set_construction(axis, true);
    let [j0, j1, j2, j3] =
        [(x0, r), (x1, r), (x1, -r), (x0, -r)].map(|(x, y)| s.add_point(Point::new(x, y)));
    let top = s.add_entity(Entity::Line { start: j0, end: j1 });
    let bottom = s.add_entity(Entity::Line { start: j2, end: j3 });
    s.add_entity(Entity::Arc {
        center: c0,
        start: j0,
        end: j3,
    });
    let right = s.add_entity(Entity::Arc {
        center: c1,
        start: j2,
        end: j1,
    });
    s.add_constraint(Constraint::Horizontal { line: top });
    s.add_constraint(Constraint::Horizontal { line: bottom });
    for (a, b) in [(c0, j0), (c0, j3), (c1, j1), (c1, j2)] {
        s.add_constraint(Constraint::VerticalPoints { a, b });
    }
    s.add_constraint(Constraint::Distance {
        a: c0,
        b: c1,
        value: x1 - x0,
    });
    s.add_constraint(Constraint::Diameter {
        target: right,
        value: 2.0 * r,
    });
    Slot {
        sketch: s,
        centres: [c0, c1],
        axis,
    }
}

/// The click on the axis offers `PointOnDatum` for the start, and the end
/// click offers `PointOnDatum` plus the axis' own `Horizontal`. The slot
/// already keeps its centres level, so only the first survives, and the
/// result has exactly the DoF of the slot tied to the axis by hand.
#[test]
fn a_slot_on_the_axis_keeps_one_of_its_three_candidates() {
    let Slot {
        mut sketch,
        centres: [c0, c1],
        axis,
    } = slot();
    assert_eq!(dof(&mut sketch), 2, "a sized slot floats in u and v");

    let on_axis = |point| Constraint::PointOnDatum {
        point,
        datum: DatumEntity::AxisX,
    };
    let verdicts = bind(
        &mut sketch,
        &[
            on_axis(c0),
            on_axis(c1),
            Constraint::Horizontal { line: axis },
        ],
    );
    assert_eq!(verdicts[0], CandidateVerdict::Ok);
    assert!(
        matches!(verdicts[1], CandidateVerdict::Redundant(_)),
        "{verdicts:?}"
    );
    assert!(
        matches!(verdicts[2], CandidateVerdict::Redundant(_)),
        "{verdicts:?}"
    );

    let mut by_hand = slot().sketch;
    by_hand.add_constraint(on_axis(c0));
    assert_eq!(dof(&mut sketch), dof(&mut by_hand));
    assert_eq!(dof(&mut sketch), 1, "the slot slides along the axis only");
    let diag = Diagnostics::evaluate(&mut sketch).1;
    assert!(diag.redundant.is_empty(), "{:?}", diag.redundant);
    assert!(diag.conflicting.is_empty(), "{:?}", diag.conflicting);
}

/// The same three in any order still keep one and leave 1 DoF: priority
/// decides which, never how many.
#[test]
fn a_slot_on_the_axis_keeps_one_in_any_order() {
    let Slot {
        centres: [c0, c1],
        axis,
        ..
    } = slot();
    let on_axis = |point| Constraint::PointOnDatum {
        point,
        datum: DatumEntity::AxisX,
    };
    let horizontal = Constraint::Horizontal { line: axis };
    for order in [
        [horizontal.clone(), on_axis(c0), on_axis(c1)],
        [on_axis(c1), horizontal.clone(), on_axis(c0)],
    ] {
        let mut sketch = slot().sketch;
        let verdicts = bind(&mut sketch, &order);
        // The axis' `Horizontal` is implied by the slot before anything
        // ties it to the axis.
        let kept = verdicts.iter().filter(|v| v.is_ok()).count();
        assert_eq!(kept, 1, "{order:?}: {verdicts:?}");
        let pinned_v = verdicts
            .iter()
            .zip(&order)
            .any(|(v, c)| v.is_ok() && matches!(c, Constraint::PointOnDatum { .. }));
        assert!(pinned_v, "{order:?}: {verdicts:?}");
        assert_eq!(dof(&mut sketch), 1, "{order:?}");
    }
}

// (b) A chain end aligned with the chain start, the line H-locked.

/// A chain from p0 = (0, 0) to p1 = (30, 0), its first segment horizontal,
/// and a second segment p1 → p2 carrying on along the same row to
/// (60, 0): the H lock and an alignment guide to the chain start claim
/// the same coordinate.
fn level_chain() -> (Draft, [PointId; 3], EntityId) {
    let mut s = Draft::seeded(1);
    let [p0, p1, p2] = [0.0, 30.0, 60.0].map(|x| s.add_point(Point::new(x * MM, 0.0)));
    let first = s.add_entity(Entity::Line { start: p0, end: p1 });
    let second = s.add_entity(Entity::Line { start: p1, end: p2 });
    s.add_constraint(Constraint::Horizontal { line: first });
    s.solve();
    (s, [p0, p1, p2], second)
}

#[test]
fn an_alignment_the_h_lock_already_set_is_redundant() {
    let (mut s, [p0, _, p2], second) = level_chain();
    let before = dof(&mut s);
    let verdicts = bind(
        &mut s,
        &[
            Constraint::Horizontal { line: second },
            Constraint::HorizontalPoints { a: p2, b: p0 },
        ],
    );
    assert_eq!(verdicts[0], CandidateVerdict::Ok);
    assert!(
        matches!(verdicts[1], CandidateVerdict::Redundant(_)),
        "{verdicts:?}"
    );
    assert_eq!(dof(&mut s), before - 1);

    // Swapped, the guide is kept and the lock is the redundant one.
    let (mut s, [p0, _, p2], second) = level_chain();
    let verdicts = bind(
        &mut s,
        &[
            Constraint::HorizontalPoints { a: p2, b: p0 },
            Constraint::Horizontal { line: second },
        ],
    );
    assert_eq!(verdicts[0], CandidateVerdict::Ok);
    assert!(
        matches!(verdicts[1], CandidateVerdict::Redundant(_)),
        "{verdicts:?}"
    );
    assert_eq!(dof(&mut s), before - 1);
}

/// (b) through the engine: a level chain 10 mm above the axis, its second
/// segment's end searched near (60, 10) with the chain start acquired. The
/// search shows the H lock and the guide to p0, the lock first; the commit
/// keeps the lock and drops the guide the overlay drew.
#[test]
fn the_search_puts_the_lock_first_and_the_commit_drops_the_guide() {
    let mut s = Draft::seeded(1);
    let [p0, p1, p2] = [0.0, 30.0, 60.0].map(|x| s.add_point(Point::new(x * MM, 10.0 * MM)));
    let first = s.add_entity(Entity::Line { start: p0, end: p1 });
    s.add_constraint(Constraint::Horizontal { line: first });
    let snap = snap_search(
        &s,
        &SnapQuery {
            cursor: (60.0 * MM, 10.3 * MM),
            tol: 0.5 * MM,
            direction_from: Some(xy(&s, p1)),
            excluded: Some(p2),
            acquired_points: &[p0],
            acquired_lines: &[],
            relations: true,
        },
    );
    assert_eq!(
        snap.inferences,
        [Inferred::Horizontal, Inferred::AlignedH(p0)],
        "{snap:?}"
    );
    assert_eq!(snap.uv, (60.0 * MM, 10.0 * MM));
    assert_eq!(snap.guides.len(), 1, "the guide to p0 is drawn");

    // The click: the end where the snap put it, the segment drawn.
    s.point_mut(p2)
        .expect("the end")
        .set_pos(snap.uv.0, snap.uv.1);
    let second = s.add_entity(Entity::Line { start: p1, end: p2 });
    let bound: Vec<_> = snap
        .inferences
        .iter()
        .filter_map(|i| i.bind(&s, p2, Some(second)))
        .collect();
    assert_eq!(bound.len(), 2);
    let before = dof(&mut s);
    let verdicts = commit_inferences(&mut s, &bound);
    assert_eq!(verdicts[0], CandidateVerdict::Ok);
    assert!(
        matches!(verdicts[1], CandidateVerdict::Redundant(_)),
        "{verdicts:?}"
    );
    assert_eq!(dof(&mut s), before - 1);
    assert_eq!(s.constraints().len(), 2, "the guide is not committed");
}

/// The L-bracket's closing corner: p3 is placed H-locked off p2 and in
/// line with the chain start p0 — the guide claims u, the lock v. Both
/// are kept.
#[test]
fn an_alignment_in_the_other_coordinate_is_kept() {
    let mut s = Draft::seeded(1);
    let [p0, p1, p2, p3] = [(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (0.0, 40.0)]
        .map(|(x, y)| s.add_point(Point::new(x * MM, y * MM)));
    let lines = [(p0, p1), (p1, p2), (p2, p3)]
        .map(|(start, end)| s.add_entity(Entity::Line { start, end }));
    s.add_constraint(Constraint::Horizontal { line: lines[0] });
    s.add_constraint(Constraint::Vertical { line: lines[1] });
    s.solve();
    let before = dof(&mut s);
    let verdicts = bind(
        &mut s,
        &[
            Constraint::Horizontal { line: lines[2] },
            Constraint::VerticalPoints { a: p3, b: p0 },
        ],
    );
    assert_eq!(verdicts, [CandidateVerdict::Ok, CandidateVerdict::Ok]);
    assert_eq!(dof(&mut s), before - 2);
}

// (c) On-curve past the segment.

/// A point snapped onto a line's extension, 10 mm past its end: the
/// residual is the infinite line's, so it is satisfied where it stands,
/// independent (`Ok`), and dragging the point along keeps it on the row
/// without pulling it back onto the segment.
#[test]
fn point_on_line_holds_on_the_extension() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::fixed(20.0 * MM, 10.0 * MM));
    let line = s.add_entity(Entity::Line { start: a, end: b });
    let p = s.add_point(Point::new(40.0 * MM, 20.0 * MM));
    let on = Constraint::PointOnLine { point: p, line };
    assert_eq!(bind(&mut s, &[on]), [CandidateVerdict::Ok]);
    let (x, y) = xy(&s, p);
    assert!((x - 40.0 * MM).abs() < 1e-9 && (y - 20.0 * MM).abs() < 1e-9);

    let mut drag = DragSession::new(&s, p);
    for i in 1..=20 {
        let t = 40.0 + i as f64;
        drag.step(&mut s, t * MM, 0.5 * t * MM + 1.0 * MM);
    }
    let (x, y) = xy(&s, p);
    assert!((y - 0.5 * x).abs() < 1e-7, "off the line at ({x}, {y})");
    assert!(x > 55.0 * MM, "pulled back toward the segment: x = {x}");
}

/// A point snapped onto an arc's circle, diametrically opposite the arc:
/// satisfied, independent, and held on the circle, not the arc.
#[test]
fn point_on_circle_holds_on_an_arcs_full_circle() {
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.0, 0.0));
    let start = s.add_point(Point::fixed(10.0 * MM, 0.0));
    let end = s.add_point(Point::fixed(0.0, 10.0 * MM));
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start,
        end,
    });
    let p = s.add_point(Point::new(
        -10.0 * MM / 2.0_f64.sqrt(),
        -10.0 * MM / 2.0_f64.sqrt(),
    ));
    let on = Constraint::PointOnCircle {
        point: p,
        circle: arc,
    };
    assert_eq!(bind(&mut s, &[on]), [CandidateVerdict::Ok]);

    let mut drag = DragSession::new(&s, p);
    for i in 0..=20 {
        let a = 1.25 * std::f64::consts::PI + 0.02 * i as f64;
        drag.step(&mut s, 12.0 * MM * a.cos(), 12.0 * MM * a.sin());
    }
    let (x, y) = xy(&s, p);
    assert!((x.hypot(y) - 10.0 * MM).abs() < 1e-7, "r = {}", x.hypot(y));
    assert!(y < 0.0, "stayed opposite the arc");
}

// (d) Tangent, line to rim, no shared point.

/// A Ø20 circle about a fixed centre and a free line whose end sits on its
/// top: the click offers the end on the rim and the tangency. Both are
/// independent, the result is solved, and dragging the line's far end
/// around keeps it tangent on the same side — the centre stays below the
/// line.
#[test]
fn tangent_line_to_rim_converges_and_does_not_flip() {
    let r = 10.0 * MM;
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.0, 0.0));
    let circle = s.add_entity(Entity::Circle {
        center: c,
        radius: r,
    });
    s.add_constraint(Constraint::Diameter {
        target: circle,
        value: 2.0 * r,
    });
    // A click 0.3 mm off true tangency, as a pixel tolerance would leave it.
    let far = s.add_point(Point::new(-30.0 * MM, 10.3 * MM));
    let end = s.add_point(Point::new(0.0, r));
    let line = s.add_entity(Entity::Line { start: far, end });
    s.solve();

    let verdicts = bind(
        &mut s,
        &[
            Constraint::PointOnCircle { point: end, circle },
            Constraint::Tangent { line, circle },
        ],
    );
    assert_eq!(verdicts, [CandidateVerdict::Ok, CandidateVerdict::Ok]);
    let (_, d) = Diagnostics::evaluate(&mut s);
    assert!(
        d.conflicting.is_empty() && d.redundant.is_empty(),
        "{}",
        d.message
    );

    // Centre below the line, as drawn: the sign of (far → end) × (far → c).
    let side = |s: &Sketch| {
        let (f, e) = (xy(s, far), xy(s, end));
        (e.0 - f.0) * (0.0 - f.1) - (e.1 - f.1) * (0.0 - f.0)
    };
    let drawn = side(&s).signum();
    let tangent_gap = |s: &Sketch| {
        let (f, e) = (xy(s, far), xy(s, end));
        let (dx, dy) = (e.0 - f.0, e.1 - f.1);
        ((dx * (0.0 - f.1) - dy * (0.0 - f.0)).abs() / dx.hypot(dy) - r).abs()
    };
    assert!(tangent_gap(&s) < 1e-7, "gap {}", tangent_gap(&s));

    // Swing the far end from the left, over the top, to the right.
    let mut drag = DragSession::new(&s, far);
    for i in 0..=40 {
        let a = std::f64::consts::PI * (0.9 - 0.8 * i as f64 / 40.0);
        let frame = drag.step(&mut s, 30.0 * MM * a.cos(), 30.0 * MM * a.sin());
        assert!(frame.converged, "frame {i}");
        assert_eq!(side(&s).signum(), drawn, "flipped at frame {i}");
        assert!(tangent_gap(&s) < 1e-6, "frame {i}: gap {}", tangent_gap(&s));
        let (ex, ey) = xy(&s, end);
        assert!(
            (ex.hypot(ey) - r).abs() < 1e-6,
            "frame {i}: end off the rim"
        );
    }
}

// (f) Relations on the drawn line's direction: parallel and perpendicular
// to a line, tangent to an arc.

/// A reference line from a fixed end, 30 mm long, whose far end the test
/// swings; and a drawn line from a fixed start, 20 mm long, its end free
/// and nearly parallel, bound `Parallel` — what a click leaves.
fn reference_and_parallel() -> (Draft, PointId, [PointId; 2], [PointId; 2]) {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(28.5 * MM, 9.3 * MM));
    let reference = s.add_entity(Entity::Line { start: a, end: b });
    s.add_constraint(Constraint::Distance {
        a,
        b,
        value: 30.0 * MM,
    });
    let start = s.add_point(Point::fixed(10.0 * MM, 30.0 * MM));
    // Along the reference, 0.5° off, as the angle tolerance leaves it.
    let (rx, ry) = b_dir(&s, a, b);
    let off = 0.5_f64.to_radians();
    let (dx, dy) = (
        rx * off.cos() - ry * off.sin(),
        rx * off.sin() + ry * off.cos(),
    );
    let end = s.add_point(Point::new(
        10.0 * MM + 20.0 * MM * dx,
        30.0 * MM + 20.0 * MM * dy,
    ));
    let drawn = s.add_entity(Entity::Line { start, end });
    s.add_constraint(Constraint::Distance {
        a: start,
        b: end,
        value: 20.0 * MM,
    });
    s.solve();
    let verdicts = bind(
        &mut s,
        &[Constraint::Parallel {
            a: drawn,
            b: reference,
        }],
    );
    assert_eq!(verdicts, [CandidateVerdict::Ok]);
    (s, b, [a, b], [start, end])
}

/// The unit direction from `a` to `b`.
fn b_dir(s: &Sketch, a: PointId, b: PointId) -> (f64, f64) {
    let ((ax, ay), (bx, by)) = (xy(s, a), xy(s, b));
    let n = (bx - ax).hypot(by - ay);
    ((bx - ax) / n, (by - ay) / n)
}

/// Swings the reference's far end through 70° and checks at each frame
/// that `holds` (the relation's residual) stays zero and `sign` (which way
/// round the drawn line lies) never changes.
fn swing_reference(
    s: &mut Sketch,
    b: PointId,
    reference: [PointId; 2],
    drawn: [PointId; 2],
    holds: fn((f64, f64), (f64, f64)) -> f64,
    sign: fn((f64, f64), (f64, f64)) -> f64,
) {
    let dir = |s: &Sketch, [p, q]: [PointId; 2]| b_dir(s, p, q);
    assert!(holds(dir(s, reference), dir(s, drawn)).abs() < 1e-7);
    let drawn_sign = sign(dir(s, reference), dir(s, drawn)).signum();
    let mut drag = DragSession::new(s, b);
    for i in 0..=35 {
        let a = (18.0 + 2.0 * i as f64).to_radians();
        let frame = drag.step(s, 30.0 * MM * a.cos(), 30.0 * MM * a.sin());
        assert!(frame.converged, "frame {i}");
        let (r, d) = (dir(s, reference), dir(s, drawn));
        assert!(holds(r, d).abs() < 1e-6, "frame {i}: {}", holds(r, d));
        assert_eq!(sign(r, d).signum(), drawn_sign, "flipped at frame {i}");
    }
}

fn cross(r: (f64, f64), d: (f64, f64)) -> f64 {
    r.0 * d.1 - r.1 * d.0
}

fn dot2(r: (f64, f64), d: (f64, f64)) -> f64 {
    r.0 * d.0 + r.1 * d.1
}

/// Drawn parallel: the relation holds as the reference turns, and the
/// drawn line keeps pointing the same way along it.
#[test]
fn parallel_converges_and_does_not_flip() {
    let (mut s, b, reference, drawn) = reference_and_parallel();
    swing_reference(&mut s, b, reference, drawn, cross, dot2);
}

/// Drawn perpendicular: likewise, on the same side of the reference.
#[test]
fn perpendicular_converges_and_does_not_flip() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(28.5 * MM, 9.3 * MM));
    let reference = s.add_entity(Entity::Line { start: a, end: b });
    s.add_constraint(Constraint::Distance {
        a,
        b,
        value: 30.0 * MM,
    });
    let start = s.add_point(Point::fixed(10.0 * MM, 30.0 * MM));
    let (rx, ry) = b_dir(&s, a, b);
    // A quarter turn left, 0.5° off, as the angle tolerance leaves it.
    let off = 0.5_f64.to_radians();
    let (nx, ny) = (
        -ry * off.cos() - rx * off.sin(),
        rx * off.cos() - ry * off.sin(),
    );
    let end = s.add_point(Point::new(
        10.0 * MM + 20.0 * MM * nx,
        30.0 * MM + 20.0 * MM * ny,
    ));
    let line = s.add_entity(Entity::Line { start, end });
    s.add_constraint(Constraint::Distance {
        a: start,
        b: end,
        value: 20.0 * MM,
    });
    s.solve();
    let verdicts = bind(
        &mut s,
        &[Constraint::Perpendicular {
            a: line,
            b: reference,
        }],
    );
    assert_eq!(verdicts, [CandidateVerdict::Ok]);
    swing_reference(&mut s, b, [a, b], [start, end], dot2, cross);
}

/// The end on the rim *and* the tangency, at the touching point exactly —
/// where the tangent snap puts the end — are one first-order row twice:
/// the second is `Redundant`, and the DoF count is off by one until the
/// point moves ~1e-7 m away. (d)'s click 0.3 mm off tangency hid this,
/// and "no shared point" does not escape it, since the end *is* the
/// touching point. `Tangent` alone is sound there: `Ok`, and one DoF gone.
#[test]
fn at_the_touching_point_tangent_goes_in_alone() {
    let r = 10.0 * MM;
    let setup = || {
        let mut s = Draft::seeded(1);
        let c = s.add_point(Point::fixed(0.0, 0.0));
        let circle = s.add_entity(Entity::Circle {
            center: c,
            radius: r,
        });
        s.add_constraint(Constraint::Diameter {
            target: circle,
            value: 2.0 * r,
        });
        let far = s.add_point(Point::new(-30.0 * MM, r));
        let end = s.add_point(Point::new(0.0, r));
        let line = s.add_entity(Entity::Line { start: far, end });
        (s, circle, end, line)
    };

    let (mut s, circle, end, line) = setup();
    let verdicts = bind(
        &mut s,
        &[
            Constraint::PointOnCircle { point: end, circle },
            Constraint::Tangent { line, circle },
        ],
    );
    assert!(
        matches!(verdicts[1], CandidateVerdict::Redundant(_)),
        "{verdicts:?}"
    );

    let (mut s, circle, _, line) = setup();
    let before = dof(&mut s);
    let verdicts = bind(&mut s, &[Constraint::Tangent { line, circle }]);
    assert_eq!(verdicts, [CandidateVerdict::Ok]);
    assert_eq!(dof(&mut s), before - 1);
}

/// What the tangent snap commits on an arc: `Tangent` alone, the end at
/// the touching point. It converges, and a drag of the far end keeps the
/// arc's centre on the side it was drawn — as (d) for a circle.
#[test]
fn tangent_to_an_arc_converges_and_does_not_flip() {
    let r = 10.0 * MM;
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.0, 0.0));
    let a0 = s.add_point(Point::fixed(r, 0.0));
    let a1 = s.add_point(Point::fixed(0.0, r));
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start: a0,
        end: a1,
    });
    // Tangent at 60°, drawn in from the upper left.
    let t = 60.0_f64.to_radians();
    let touch = (r * t.cos(), r * t.sin());
    let far = s.add_point(Point::new(
        touch.0 - 30.0 * MM * t.sin(),
        touch.1 + 30.0 * MM * t.cos(),
    ));
    let end = s.add_point(Point::new(touch.0, touch.1));
    let line = s.add_entity(Entity::Line { start: far, end });
    s.solve();
    let verdicts = bind(&mut s, &[Constraint::Tangent { line, circle: arc }]);
    assert_eq!(verdicts, [CandidateVerdict::Ok]);
    let side = |s: &Sketch| {
        let (f, e) = (xy(s, far), xy(s, end));
        (e.0 - f.0) * (0.0 - f.1) - (e.1 - f.1) * (0.0 - f.0)
    };
    let gap = |s: &Sketch| {
        let (f, e) = (xy(s, far), xy(s, end));
        let (dx, dy) = (e.0 - f.0, e.1 - f.1);
        ((dx * (0.0 - f.1) - dy * (0.0 - f.0)).abs() / dx.hypot(dy) - r).abs()
    };
    let drawn = side(&s).signum();
    let mut drag = DragSession::new(&s, far);
    let (fx, fy) = xy(&s, far);
    for i in 0..=20 {
        let k = i as f64 / 20.0;
        let frame = drag.step(&mut s, fx - 10.0 * MM * k, fy - 5.0 * MM * k);
        assert!(frame.converged, "frame {i}");
        assert_eq!(side(&s).signum(), drawn, "flipped at frame {i}");
        assert!(gap(&s) < 1e-6, "frame {i}: gap {}", gap(&s));
    }
}

// (e) What one verdict costs on the plate.

/// A motor-mount plate, fully constrained, with a line mid-draw beside it — two free points for a
/// candidate to land on.
fn plate() -> (Draft, [PointId; 4], PointId, [PointId; 2], PointId) {
    let (w, h) = (100.0 * MM, 60.0 * MM);
    let mut s = Draft::seeded(1);
    let p = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| s.add_point(Point::new(x, y)));
    let lines: Vec<_> = (0..4)
        .map(|i| {
            s.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    s.add_constraint(Constraint::CoincidentToDatum {
        point: p[0],
        datum: DatumEntity::Origin,
    });
    s.add_constraint(Constraint::Horizontal { line: lines[0] });
    s.add_constraint(Constraint::Vertical { line: lines[1] });
    s.add_constraint(Constraint::Horizontal { line: lines[2] });
    s.add_constraint(Constraint::Vertical { line: lines[3] });
    s.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: w,
    });
    s.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[3],
        value: h,
    });

    let (center, bolt) = s.add_circle(w / 2.0, h / 2.0, 18.0 * MM);
    s.set_construction(bolt, true);
    s.add_constraint(Constraint::HorizontalDistance {
        a: p[0],
        b: center,
        value: w / 2.0,
    });
    s.add_constraint(Constraint::VerticalDistance {
        a: p[0],
        b: center,
        value: h / 2.0,
    });
    s.add_constraint(Constraint::Diameter {
        target: bolt,
        value: 36.0 * MM,
    });
    let off = 18.0 * MM / 2.0_f64.sqrt();
    let holes = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .map(|(sx, sy)| s.add_circle(w / 2.0 + sx * off, h / 2.0 + sy * off, 1.6 * MM));
    for &(point, _) in &holes {
        s.add_constraint(Constraint::PointOnCircle {
            point,
            circle: bolt,
        });
    }
    for pair in holes.windows(2) {
        s.add_constraint(Constraint::Equal {
            a: pair[0].1,
            b: pair[1].1,
        });
        s.add_constraint(Constraint::AnglePoints {
            a: pair[0].0,
            vertex: center,
            b: pair[1].0,
            value: FRAC_PI_2,
        });
    }
    s.add_constraint(Constraint::HorizontalPoints {
        a: holes[0].0,
        b: holes[1].0,
    });
    s.add_constraint(Constraint::Diameter {
        target: holes[0].1,
        value: 3.2 * MM,
    });

    let (cx, cy, hl, sr) = (w / 2.0, h / 2.0, 10.0 * MM, 3.0 * MM);
    let [s0, s1, s2, s3, c_left, c_right] = [
        (cx - hl, cy - sr),
        (cx + hl, cy - sr),
        (cx + hl, cy + sr),
        (cx - hl, cy + sr),
        (cx - hl, cy),
        (cx + hl, cy),
    ]
    .map(|(x, y)| s.add_point(Point::new(x, y)));
    let bottom = s.add_entity(Entity::Line { start: s0, end: s1 });
    let right_arc = s.add_entity(Entity::Arc {
        center: c_right,
        start: s1,
        end: s2,
    });
    let top = s.add_entity(Entity::Line { start: s2, end: s3 });
    s.add_entity(Entity::Arc {
        center: c_left,
        start: s3,
        end: s0,
    });
    s.add_constraint(Constraint::Horizontal { line: bottom });
    s.add_constraint(Constraint::Horizontal { line: top });
    for (a, b) in [(c_right, s1), (c_right, s2), (c_left, s3), (c_left, s0)] {
        s.add_constraint(Constraint::VerticalPoints { a, b });
    }
    s.add_constraint(Constraint::SymmetricPoints {
        a: c_left,
        b: c_right,
        center,
    });
    s.add_constraint(Constraint::Distance {
        a: c_left,
        b: c_right,
        value: 2.0 * hl,
    });
    s.add_constraint(Constraint::Radius {
        target: right_arc,
        value: sr,
    });

    let (_, d) = Diagnostics::evaluate(&mut s);
    assert_eq!(d.status, SketchStatus::FullyConstrained, "{}", d.message);

    // Mid-draw: a free line off the plate's right edge.
    let (_, free_end, _) = s.add_line(120.0 * MM, 10.0 * MM, 140.0 * MM, h / 2.0);
    (s, p, center, [c_left, c_right], free_end)
}

/// The median wall-clock cost of `check_candidate` on the plate for each
/// verdict it can give. The preview pays this once per new candidate set
/// (the verdict cache's miss); the number goes to
/// `docs/CONCURRENCY-WASM.md` §Budgets. Asserted only against
/// a generous ceiling — a debug build, a busy CI runner — so the test
/// states the verdicts and prints the cost; the frame-budget comparison is
/// made on the printed release figure.
#[test]
fn a_verdict_on_the_plate_costs_under_a_frame() {
    let (s, corners, center, [c_left, _], free_end) = plate();
    let cases = [
        (
            "Ok",
            Constraint::HorizontalPoints {
                a: free_end,
                b: center,
            },
        ),
        (
            "Redundant",
            Constraint::HorizontalPoints {
                a: c_left,
                b: center,
            },
        ),
        (
            "Conflicting",
            Constraint::HorizontalPoints {
                a: corners[0],
                b: corners[3],
            },
        ),
    ];
    for (name, candidate) in cases {
        let verdict = s.check_candidate(&candidate);
        let matches = match name {
            "Ok" => verdict.is_ok(),
            "Redundant" => matches!(verdict, CandidateVerdict::Redundant(_)),
            _ => matches!(verdict, CandidateVerdict::Conflicting(_)),
        };
        assert!(matches, "{name}: {verdict:?}");

        let mut times: Vec<f64> = (0..21)
            .map(|_| {
                let start = Instant::now();
                let _ = s.check_candidate(&candidate);
                start.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        times.sort_by(f64::total_cmp);
        let median = times[times.len() / 2];
        println!("check_candidate on the plate, {name}: median {median:.3} ms");
        assert!(median < 50.0, "{name}: {median} ms");
    }
}
