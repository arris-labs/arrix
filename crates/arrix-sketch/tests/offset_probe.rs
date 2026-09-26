//! Which constraint set makes an offset.
//!
//! The copies here are built by hand, at exactly the geometry an offset
//! would write, and each is measured: degrees of freedom, redundancy, a
//! solve that moves nothing, and a `d` edit. The design's intended set is
//! `Parallel` + `DistanceParallelLines` per line, `Concentric` +
//! `DistanceCircleCircle` per arc or circle, `Tangent` where the original
//! has one, and a construction anchor at each free end of an open chain. What
//! it pins is the row set `modify::offset` writes.

use std::f64::consts::PI;

use arrix_sketch::Draft;
use arrix_sketch::{
    Constraint, ConstraintId, Diagnostics, Entity, EntityId, Point, PointId, Sketch, SketchStatus,
};

const W: f64 = 0.040;
const H: f64 = 0.030;
const R: f64 = 0.010;
const D: f64 = 0.003;
const TOL: f64 = 1e-9;

/// The original, the copy and the rows that tie them.
struct Probe {
    s: Draft,
    /// `(original, copy)` per curve.
    pairs: Vec<(EntityId, EntityId)>,
    /// The distance rows, so a `d` edit can rewrite them.
    dists: Vec<ConstraintId>,
    /// Every point of the copy, to read where it ended up.
    copy_points: Vec<PointId>,
    d: f64,
}

impl Probe {
    fn new(s: Draft) -> Self {
        Self {
            s,
            pairs: Vec::new(),
            dists: Vec::new(),
            copy_points: Vec::new(),
            d: 0.0,
        }
    }

    fn pt(&mut self, x: f64, y: f64) -> PointId {
        let p = self.s.add_point(Point::new(x, y));
        self.copy_points.push(p);
        p
    }

    /// Records that `copy` is `orig`'s offset by `d`. The rows are written by
    /// [`Probe::commit`], in the order the design says.
    fn tie(&mut self, orig: EntityId, copy: EntityId, d: f64) {
        self.pairs.push((orig, copy));
        self.d = d;
    }

    /// Writes the design's rows: each arc or circle's `Concentric` and
    /// `DistanceCircleCircle`, then the `Tangent` joints, then each line's
    /// `Parallel` and `DistanceParallelLines`. With `filter`, a row that
    /// `check_candidate` does not call `Ok` is left out, and the log says so.
    fn commit(&mut self, joints: &[(EntityId, EntityId)], filter: bool) -> Vec<String> {
        let mut log = Vec::new();
        let mut rows: Vec<(Constraint, bool)> = Vec::new();
        for &(orig, copy) in &self.pairs {
            if !matches!(self.s.entity(orig), Some(Entity::Line { .. })) {
                rows.push((Constraint::Concentric { a: orig, b: copy }, false));
                rows.push((
                    Constraint::DistanceCircleCircle {
                        a: orig,
                        b: copy,
                        value: self.d,
                    },
                    true,
                ));
            }
        }
        for &(line, circle) in joints {
            rows.push((Constraint::Tangent { line, circle }, false));
        }
        for &(orig, copy) in &self.pairs {
            if matches!(self.s.entity(orig), Some(Entity::Line { .. })) {
                rows.push((Constraint::Parallel { a: orig, b: copy }, false));
                rows.push((
                    Constraint::DistanceParallelLines {
                        a: orig,
                        b: copy,
                        value: self.d,
                    },
                    true,
                ));
            }
        }
        for (row, is_dist) in rows {
            let verdict = self.s.check_candidate(&row);
            if filter && !verdict.is_ok() {
                log.push(format!("dropped {row:?}: {verdict:?}"));
                continue;
            }
            let id = self.s.add_constraint(row);
            if is_dist {
                self.dists.push(id);
            }
        }
        log
    }

    fn set_d(&mut self, d: f64) {
        for id in self.dists.clone() {
            match self.s.get_constraint_mut(id).unwrap() {
                Constraint::DistanceParallelLines { value, .. }
                | Constraint::DistanceCircleCircle { value, .. } => *value = d,
                other => panic!("not a distance row: {other:?}"),
            }
        }
    }
}

struct Meas {
    dof: i32,
    status: SketchStatus,
    redundant: usize,
    conflicting: usize,
    converged: bool,
    moved: f64,
}

fn positions(s: &Sketch) -> Vec<(PointId, [f64; 2])> {
    s.points().iter().map(|(id, p)| (*id, p.pos())).collect()
}

/// Solves once and reports what moved.
fn measure(s: &mut Draft) -> Meas {
    let before = positions(s);
    let (r, d) = Diagnostics::evaluate(s);
    let moved = before
        .iter()
        .map(|(id, p)| {
            let q = s.point(*id).unwrap().pos();
            (p[0] - q[0]).hypot(p[1] - q[1])
        })
        .fold(0.0, f64::max);
    Meas {
        dof: d.dof,
        status: d.status,
        redundant: d.redundant.len(),
        conflicting: d.conflicting.len(),
        converged: r.converged,
        moved,
    }
}

fn show(what: &str, m: &Meas) {
    println!(
        "{what}: dof {} status {:?} redundant {} conflicting {} converged {} moved {:.3e}",
        m.dof, m.status, m.redundant, m.conflicting, m.converged, m.moved
    );
}

fn pos(s: &Sketch, p: PointId) -> [f64; 2] {
    s.point(p).unwrap().pos()
}

// ---- closed rectangle ----------------------------------------------------

/// A 40 × 30 rectangle from a fixed origin, four H/V lines and two typed
/// sizes, and its copy `d` inside.
fn rectangle(d: f64) -> (Probe, [PointId; 4]) {
    let mut s = Draft::seeded(1);
    let corners = [(0.0, 0.0), (W, 0.0), (W, H), (0.0, H)];
    let p: Vec<PointId> = corners
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            s.add_point(if i == 0 {
                Point::fixed(x, y)
            } else {
                Point::new(x, y)
            })
        })
        .collect();
    let l: Vec<EntityId> = (0..4)
        .map(|i| {
            s.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    for (i, &line) in l.iter().enumerate() {
        s.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
    s.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: W,
    });
    s.add_constraint(Constraint::Distance {
        a: p[1],
        b: p[2],
        value: H,
    });
    let mut probe = Probe::new(s);
    let inner = [(d, d), (W - d, d), (W - d, H - d), (d, H - d)];
    let q: Vec<PointId> = inner.iter().map(|&(x, y)| probe.pt(x, y)).collect();
    for i in 0..4 {
        let copy = probe.s.add_entity(Entity::Line {
            start: q[i],
            end: q[(i + 1) % 4],
        });
        probe.tie(l[i], copy, d.abs());
    }
    probe.commit(&[], true);
    (probe, [q[0], q[1], q[2], q[3]])
}

#[test]
fn closed_rectangle_offset_adds_no_dof_and_moves_nothing() {
    let (mut probe, q) = rectangle(D);
    let m = measure(&mut probe.s);
    show("closed rectangle", &m);
    assert!(m.converged);
    assert_eq!(m.dof, 0, "original 0 + copy 0");
    assert_eq!(m.redundant, 0);
    assert_eq!(m.conflicting, 0);
    assert!(m.moved <= TOL, "a solve moves nothing");
    assert_eq!(m.status, SketchStatus::FullyConstrained);

    // A `d` edit re-shapes the copy, and the original stays put.
    let orig = positions(&probe.s)
        .into_iter()
        .filter(|(id, _)| !probe.copy_points.contains(id))
        .collect::<Vec<_>>();
    probe.set_d(2.0 * D);
    let m = measure(&mut probe.s);
    show("closed rectangle, d 3 -> 6", &m);
    assert!(m.converged);
    assert_eq!(m.dof, 0);
    let want = [(2.0 * D, 2.0 * D), (W - 2.0 * D, H - 2.0 * D)];
    let (a, c) = (pos(&probe.s, q[0]), pos(&probe.s, q[2]));
    assert!((a[0] - want[0].0).hypot(a[1] - want[0].1) <= TOL, "{a:?}");
    assert!((c[0] - want[1].0).hypot(c[1] - want[1].1) <= TOL, "{c:?}");
    for (id, p) in orig {
        let now = pos(&probe.s, id);
        assert!((p[0] - now[0]).hypot(p[1] - now[1]) <= TOL, "{id} moved");
    }
}

#[test]
fn closed_rectangle_offset_outward_is_the_same_rows() {
    let (mut probe, q) = rectangle(-D);
    let m = measure(&mut probe.s);
    show("closed rectangle, outward", &m);
    assert!(m.converged && m.dof == 0 && m.redundant == 0 && m.moved <= TOL);
    probe.set_d(2.0 * D);
    let m = measure(&mut probe.s);
    assert!(m.converged && m.dof == 0);
    let a = pos(&probe.s, q[0]);
    assert!((a[0] + 2.0 * D).hypot(a[1] + 2.0 * D) <= TOL, "{a:?}");
}

#[test]
fn closed_rectangle_offset_past_the_half_width_inverts_and_is_still_satisfied() {
    // Inward by 16 mm on a 30 mm side: `DistanceParallelLines` is sign-free,
    // so the copy is honoured as an inside-out rectangle, not refused. The
    // tool's `TooLarge` guards creation; an edit of the dimension does not.
    let (mut probe, q) = rectangle(D);
    probe.set_d(0.016);
    let m = measure(&mut probe.s);
    show("closed rectangle, d 3 -> 16", &m);
    assert!(m.converged && m.dof == 0);
    let (a, c) = (pos(&probe.s, q[0]), pos(&probe.s, q[2]));
    assert!(
        (a[1] - 0.016).abs() <= TOL && (c[1] - 0.014).abs() <= TOL,
        "{a:?} {c:?}"
    );
    assert!(c[1] < a[1], "the copy's top edge is below its bottom edge");
}

// ---- rounded rectangle ---------------------------------------------------

/// The four lines and four tangent arcs of a rounded rectangle: 40 × 30
/// with R10 corners, fully constrained by H/V, `Tangent` at
/// all eight joints, four radii, a fixed first centre and the centre spacing.
/// Its copy is `d` inside: the same centres, radius `R − d`.
fn rounded(d: f64, filter: bool) -> (Probe, Vec<String>) {
    let mut s = Draft::seeded(1);
    let centres = [(R, R), (W - R, R), (W - R, H - R), (R, H - R)];
    let c: Vec<PointId> = centres
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            s.add_point(if i == 0 {
                Point::fixed(x, y)
            } else {
                Point::new(x, y)
            })
        })
        .collect();
    // Tangent points, CCW from the bottom line's start: t0..t7.
    let t = [
        (R, 0.0),
        (W - R, 0.0),
        (W, R),
        (W, H - R),
        (W - R, H),
        (R, H),
        (0.0, H - R),
        (0.0, R),
    ];
    let tp: Vec<PointId> = t
        .iter()
        .map(|&(x, y)| s.add_point(Point::new(x, y)))
        .collect();
    let lines: Vec<EntityId> = [(0, 1), (2, 3), (4, 5), (6, 7)]
        .iter()
        .map(|&(a, b)| {
            s.add_entity(Entity::Line {
                start: tp[a],
                end: tp[b],
            })
        })
        .collect();
    // Arc i's centre, start and end tangent points.
    let arc_spec = [(1, 1, 2), (2, 3, 4), (3, 5, 6), (0, 7, 0)];
    let arcs: Vec<EntityId> = arc_spec
        .iter()
        .map(|&(centre, a, b)| {
            s.add_entity(Entity::Arc {
                center: c[centre],
                start: tp[a],
                end: tp[b],
            })
        })
        .collect();
    for (i, &l) in lines.iter().enumerate() {
        s.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line: l }
        } else {
            Constraint::Vertical { line: l }
        });
    }
    // Line i joins arc i-1 (at its start) and arc i (at its end).
    for i in 0..4 {
        for arc in [arcs[(i + 3) % 4], arcs[i]] {
            s.add_constraint(Constraint::Tangent {
                line: lines[i],
                circle: arc,
            });
        }
    }
    for &a in &arcs {
        s.add_constraint(Constraint::Radius {
            target: a,
            value: R,
        });
    }
    s.add_constraint(Constraint::HorizontalDistance {
        a: c[0],
        b: c[1],
        value: W - 2.0 * R,
    });
    s.add_constraint(Constraint::VerticalDistance {
        a: c[1],
        b: c[2],
        value: H - 2.0 * R,
    });

    let mut probe = Probe::new(s);
    // The copy: each tangent point slides toward its centre by `d`.
    let owner = [0usize, 1, 1, 2, 2, 3, 3, 0];
    let tq: Vec<PointId> = t
        .iter()
        .enumerate()
        .map(|(k, &(x, y))| {
            let (cx, cy) = centres[owner[k]];
            let f = (R - d) / R;
            probe.pt(cx + (x - cx) * f, cy + (y - cy) * f)
        })
        .collect();
    let cq: Vec<PointId> = centres.iter().map(|&(x, y)| probe.pt(x, y)).collect();
    let mut copy_lines = Vec::new();
    for (i, (a, b)) in [(0, 1), (2, 3), (4, 5), (6, 7)].into_iter().enumerate() {
        let l = probe.s.add_entity(Entity::Line {
            start: tq[a],
            end: tq[b],
        });
        probe.tie(lines[i], l, d.abs());
        copy_lines.push(l);
    }
    let mut joints = Vec::new();
    for (i, &(centre, a, b)) in arc_spec.iter().enumerate() {
        let ar = probe.s.add_entity(Entity::Arc {
            center: cq[centre],
            start: tq[a],
            end: tq[b],
        });
        probe.tie(arcs[i], ar, d.abs());
        joints.push((copy_lines[i], ar));
        joints.push((copy_lines[(i + 1) % 4], ar));
    }
    let log = probe.commit(&joints, filter);
    (probe, log)
}

fn rows_of(s: &Sketch) -> usize {
    s.constraints().len()
}

#[test]
fn rounded_rectangle_needs_no_line_rows() {
    // Every row: the rows are met, the copy is fully constrained, and the
    // analysis calls sixteen of them dependent: at a tangent joint the line's
    // rows and the arc's cross at a double root.
    let (mut p, log) = rounded(D, false);
    assert!(log.is_empty());
    assert_eq!(rows_of(&p.s), 42);
    let m = measure(&mut p.s);
    show("rounded, every row", &m);
    assert!(m.converged && m.dof == 0 && m.moved <= TOL);
    assert_eq!(m.redundant, 16, "the tie rows are flagged");

    // Filtered in the design's order (arcs, tangents, lines), the four
    // `Parallel` and four `DistanceParallelLines` fall away: eight rows,
    // and nothing is left redundant.
    let (mut p, log) = rounded(D, true);
    assert_eq!(log.len(), 8, "{log:#?}");
    assert!(log.iter().all(|l| l.contains("Parallel")));
    assert_eq!(rows_of(&p.s), 34);
    let m = measure(&mut p.s);
    show("rounded, filtered", &m);
    assert!(m.converged && m.dof == 0 && m.moved <= TOL);
    assert_eq!((m.redundant, m.conflicting), (0, 0));
    assert_eq!(m.status, SketchStatus::FullyConstrained);
}

fn arc_radii(p: &Probe) -> Vec<f64> {
    p.pairs
        .iter()
        .filter_map(|&(_, copy)| match p.s.entity(copy) {
            Some(Entity::Arc { center, start, .. }) => {
                let (c, a) = (pos(&p.s, *center), pos(&p.s, *start));
                Some((c[0] - a[0]).hypot(c[1] - a[1]))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn rounded_rectangle_edits() {
    // The lines follow the arcs through tangency: their own rows are gone.
    let (mut p, _) = rounded(D, true);
    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    show("rounded filtered, d 3 -> 6", &m);
    assert!(m.converged && m.dof == 0 && m.redundant == 0);
    for r in arc_radii(&p) {
        assert!((r - (R - 2.0 * D)).abs() <= TOL, "{r}");
    }
    // The bottom copy line now sits 6 mm above the original.
    let (bottom_copy, _) = (p.pairs[0].1, ());
    let Some(Entity::Line { start, .. }) = p.s.entity(bottom_copy) else {
        panic!("not a line")
    };
    assert!((pos(&p.s, *start)[1] - 2.0 * D).abs() <= TOL);

    // Past the radius (12 mm on an R10 corner) the copy cannot exist and the
    // solver says so; it does not flip the arc to the outside.
    let (mut p, _) = rounded(D, true);
    p.set_d(0.012);
    let m = measure(&mut p.s);
    show("rounded filtered, d 3 -> 12", &m);
    assert!(
        !m.converged,
        "an inward distance past the radius fails soft"
    );
    assert!(
        arc_radii(&p).iter().all(|&r| r < 1e-5),
        "radius collapsed, not flipped"
    );
}

#[test]
fn rounded_rectangle_outward() {
    // Outward by 3 mm: radius R + d, the same rows.
    let (mut p, _) = rounded(-D, true);
    let m = measure(&mut p.s);
    show("rounded outward, filtered", &m);
    assert!(m.converged && m.dof == 0 && m.redundant == 0 && m.moved <= TOL);
    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    assert!(m.converged && m.dof == 0);
    for r in arc_radii(&p) {
        assert!((r - (R + 2.0 * D)).abs() <= TOL, "{r}");
    }
}

// ---- open chains ---------------------------------------------------------

/// A construction segment from an original end to its copy, added as the
/// free-end anchor.
fn anchor(probe: &mut Probe, from: PointId, to: PointId) -> EntityId {
    let seg = probe.s.add_entity(Entity::Line {
        start: from,
        end: to,
    });
    probe.s.set_construction(seg, true);
    seg
}

/// Two lines, 40 long then 30 up, from a fixed origin; the copy is `d` to
/// the left of the direction of travel (inside the L's corner).
fn two_lines(d: f64, anchors: bool) -> (Probe, [PointId; 3]) {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(W, 0.0));
    let p2 = s.add_point(Point::new(W, H));
    let a = s.add_entity(Entity::Line { start: p0, end: p1 });
    let b = s.add_entity(Entity::Line { start: p1, end: p2 });
    s.add_constraint(Constraint::Horizontal { line: a });
    s.add_constraint(Constraint::Vertical { line: b });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: W,
    });
    s.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: H,
    });
    let mut probe = Probe::new(s);
    let q0 = probe.pt(0.0, d);
    let q1 = probe.pt(W - d, d);
    let q2 = probe.pt(W - d, H);
    let qa = probe.s.add_entity(Entity::Line { start: q0, end: q1 });
    let qb = probe.s.add_entity(Entity::Line { start: q1, end: q2 });
    probe.tie(a, qa, d);
    probe.tie(b, qb, d);
    probe.commit(&[], true);
    if anchors {
        let s0 = anchor(&mut probe, p0, q0);
        probe
            .s
            .add_constraint(Constraint::Perpendicular { a: s0, b: a });
        let s2 = anchor(&mut probe, p2, q2);
        probe
            .s
            .add_constraint(Constraint::Perpendicular { a: s2, b });
    }
    (probe, [q0, q1, q2])
}

#[test]
fn open_two_line_chain_needs_an_anchor_at_each_free_end() {
    // Without them the two free ends slide along their lines.
    let (mut p, _) = two_lines(D, false);
    let m = measure(&mut p.s);
    show("open two lines, no anchors", &m);
    assert_eq!((m.dof, m.redundant), (2, 0));

    // A cheaper existing row does not hold one: the end is `d` from the
    // original end and on a line `d` away, which is a tangency, a double
    // root. It leaves both degrees of freedom and calls itself redundant.
    let (mut p, q) = two_lines(D, false);
    // The fixed origin, the chain's free start.
    let p0 = p.s.hit_point(0.0, 0.0, 1e-9).unwrap();
    p.s.add_constraint(Constraint::Distance {
        a: p0,
        b: q[0],
        value: D,
    });
    let m = measure(&mut p.s);
    show("open two lines, Distance(end, copy end)", &m);
    assert_eq!(m.dof, 2);
    assert_eq!(m.redundant, 2);

    // A construction segment `Perpendicular` to the end curve does.
    let (mut p, q) = two_lines(D, true);
    let m = measure(&mut p.s);
    show("open two lines, anchored", &m);
    assert!(m.converged && m.moved <= TOL);
    assert_eq!((m.dof, m.redundant, m.conflicting), (0, 0, 0));
    assert_eq!(m.status, SketchStatus::FullyConstrained);

    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    assert!(m.converged && m.dof == 0);
    let (q0, q2) = (pos(&p.s, q[0]), pos(&p.s, q[2]));
    assert!(
        q0[0].abs() <= TOL && (q0[1] - 2.0 * D).abs() <= TOL,
        "{q0:?}"
    );
    assert!(
        (q2[0] - (W - 2.0 * D)).abs() <= TOL && (q2[1] - H).abs() <= TOL,
        "{q2:?}"
    );
}

/// A line 30 long, then a tangent quarter arc of R10 turning left: the copy
/// is `d` to the left (inside the turn), radius `R − d`.
fn line_and_arc(d: f64, filter: bool) -> (Probe, Vec<String>) {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.030, 0.0));
    let c = s.add_point(Point::new(0.030, R));
    // A 60° sweep: ending at 90° would put the end at the arc's extremum,
    // where a distance to an axis is a double root of its own.
    let end = (0.030 + R * (PI / 6.0).cos(), R - R * (PI / 6.0).sin());
    let p2 = s.add_point(Point::new(end.0, end.1));
    let line = s.add_entity(Entity::Line { start: p0, end: p1 });
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start: p1,
        end: p2,
    });
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.030,
    });
    s.add_constraint(Constraint::Radius {
        target: arc,
        value: R,
    });
    s.add_constraint(Constraint::Tangent { line, circle: arc });
    s.add_constraint(Constraint::DistanceToAxisX {
        point: p2,
        value: end.0,
    });
    let mut probe = Probe::new(s);
    let q0 = probe.pt(0.0, d);
    let q1 = probe.pt(0.030, d);
    let cq = probe.pt(0.030, R);
    let f = (R - d) / R;
    let q2 = probe.pt(0.030 + (end.0 - 0.030) * f, R + (end.1 - R) * f);
    let ql = probe.s.add_entity(Entity::Line { start: q0, end: q1 });
    let qa = probe.s.add_entity(Entity::Arc {
        center: cq,
        start: q1,
        end: q2,
    });
    probe.tie(line, ql, d);
    probe.tie(arc, qa, d);
    let log = probe.commit(&[(ql, qa)], filter);
    let s0 = anchor(&mut probe, p0, q0);
    probe
        .s
        .add_constraint(Constraint::Perpendicular { a: s0, b: line });
    // The arc end's anchor: original end, copy end and centre are collinear.
    let s2 = anchor(&mut probe, p2, q2);
    probe
        .s
        .add_constraint(Constraint::PointOnLine { point: c, line: s2 });
    (probe, log)
}

#[test]
fn arc_chain_with_one_tangent_joint() {
    // Every row: fully constrained, three rows dependent.
    let (mut p, _) = line_and_arc(D, false);
    let m = measure(&mut p.s);
    show("line + tangent arc, every row", &m);
    assert!(m.converged && m.dof == 0 && m.moved <= TOL && m.redundant > 0);

    // Filtered: the line's `DistanceParallelLines` is implied by its
    // `Parallel`, the joint's `Tangent` and the arc's radius. An arc end is
    // anchored by a construction segment with the centre on its line.
    let (mut p, log) = line_and_arc(D, true);
    assert_eq!(log.len(), 1, "{log:#?}");
    assert!(log[0].contains("DistanceParallelLines"));
    let m = measure(&mut p.s);
    show("line + tangent arc, filtered", &m);
    assert!(m.converged && m.moved <= TOL);
    assert_eq!((m.dof, m.redundant, m.conflicting), (0, 0, 0));

    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    assert!(m.converged && m.dof == 0 && m.redundant == 0);
    assert!((arc_radii(&p)[0] - (R - 2.0 * D)).abs() <= TOL);
}

// ---- circle --------------------------------------------------------------

fn circle(d: f64) -> (Probe, EntityId) {
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.020, 0.015));
    let orig = s.add_entity(Entity::Circle {
        center: c,
        radius: R,
    });
    s.add_constraint(Constraint::Radius {
        target: orig,
        value: R,
    });
    let mut probe = Probe::new(s);
    let cq = probe.pt(0.020, 0.015);
    let copy = probe.s.add_entity(Entity::Circle {
        center: cq,
        radius: R - d,
    });
    probe.tie(orig, copy, d.abs());
    probe.commit(&[], true);
    (probe, copy)
}

fn circle_radius(p: &Probe, copy: EntityId) -> f64 {
    match p.s.entity(copy) {
        Some(Entity::Circle { radius, .. }) => *radius,
        other => panic!("not a circle: {other:?}"),
    }
}

#[test]
fn circle_offset() {
    let (mut p, copy) = circle(D);
    let m = measure(&mut p.s);
    show("circle", &m);
    assert!(m.converged && m.moved <= TOL);
    assert_eq!((m.dof, m.redundant, m.conflicting), (0, 0, 0));
    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    assert!(m.converged && m.dof == 0);
    assert!((circle_radius(&p, copy) - (R - 2.0 * D)).abs() <= TOL);

    // Outward: `DistanceCircleCircle` reads the same for a concentric pair.
    let (mut p, copy) = circle(-D);
    let m = measure(&mut p.s);
    assert!(m.converged && m.dof == 0 && m.moved <= TOL);
    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    assert!(m.converged && m.dof == 0);
    assert!((circle_radius(&p, copy) - (R + 2.0 * D)).abs() <= TOL);
}

#[test]
fn circle_offset_past_the_radius_does_not_flip() {
    // `DistanceCircleCircle` is `||r₁ − r₂| − d|` for a concentric pair, so
    // 12 mm has a root at r₂ = 22 mm as well as at −2 mm. An edit walks from
    // 7 mm toward 0 and stops there: it does not jump the branch.
    let (mut p, copy) = circle(D);
    p.set_d(0.012);
    let m = measure(&mut p.s);
    show("circle, d 3 -> 12", &m);
    assert!(!m.converged);
    assert!(circle_radius(&p, copy) < 1e-5);
}

// ---- one rounded corner among sharp ones -------------------------------

/// 40 × 30 with only the bottom-right corner rounded (R10): three sharp
/// corners, two tangent joints. Its copy is `d` inside.
fn one_round_corner(d: f64, filter: bool) -> (Probe, Vec<String>) {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let t1 = s.add_point(Point::new(W - R, 0.0));
    let t2 = s.add_point(Point::new(W, R));
    let c = s.add_point(Point::new(W - R, R));
    let p3 = s.add_point(Point::new(W, H));
    let p4 = s.add_point(Point::new(0.0, H));
    let bottom = s.add_entity(Entity::Line { start: p0, end: t1 });
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start: t1,
        end: t2,
    });
    let right = s.add_entity(Entity::Line { start: t2, end: p3 });
    let top = s.add_entity(Entity::Line { start: p3, end: p4 });
    let left = s.add_entity(Entity::Line { start: p4, end: p0 });
    for (line, horizontal) in [(bottom, true), (top, true), (right, false), (left, false)] {
        s.add_constraint(if horizontal {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
    s.add_constraint(Constraint::Tangent {
        line: bottom,
        circle: arc,
    });
    s.add_constraint(Constraint::Tangent {
        line: right,
        circle: arc,
    });
    s.add_constraint(Constraint::Radius {
        target: arc,
        value: R,
    });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: t1,
        value: W - R,
    });
    s.add_constraint(Constraint::VerticalDistance {
        a: p0,
        b: p4,
        value: H,
    });
    let mut probe = Probe::new(s);
    let q0 = probe.pt(d, d);
    let qt1 = probe.pt(W - R, d);
    let qt2 = probe.pt(W - d, R);
    let qc = probe.pt(W - R, R);
    let q3 = probe.pt(W - d, H - d);
    let q4 = probe.pt(d, H - d);
    let qb = probe.s.add_entity(Entity::Line {
        start: q0,
        end: qt1,
    });
    let qa = probe.s.add_entity(Entity::Arc {
        center: qc,
        start: qt1,
        end: qt2,
    });
    let qr = probe.s.add_entity(Entity::Line {
        start: qt2,
        end: q3,
    });
    let qt = probe.s.add_entity(Entity::Line { start: q3, end: q4 });
    let ql = probe.s.add_entity(Entity::Line { start: q4, end: q0 });
    probe.tie(bottom, qb, d);
    probe.tie(arc, qa, d);
    probe.tie(right, qr, d);
    probe.tie(top, qt, d);
    probe.tie(left, ql, d);
    let log = probe.commit(&[(qb, qa), (qr, qa)], filter);
    (probe, log)
}

#[test]
fn one_rounded_corner_among_sharp_ones() {
    // Only the two lines that meet the arc lose their distance row; the
    // `Parallel` rows and the two sharp-cornered lines keep theirs.
    let (mut p, log) = one_round_corner(D, false);
    assert!(log.is_empty());
    let m = measure(&mut p.s);
    show("one rounded corner, every row", &m);
    assert!(m.converged && m.dof == 0 && m.moved <= TOL && m.redundant > 0);

    let (mut p, log) = one_round_corner(D, true);
    assert_eq!(log.len(), 2, "{log:#?}");
    assert!(log.iter().all(|l| l.contains("DistanceParallelLines")));
    let m = measure(&mut p.s);
    show("one rounded corner, filtered", &m);
    assert!(m.converged && m.moved <= TOL);
    assert_eq!((m.dof, m.redundant, m.conflicting), (0, 0, 0));
    assert_eq!(m.status, SketchStatus::FullyConstrained);

    p.set_d(2.0 * D);
    let m = measure(&mut p.s);
    show("one rounded corner, filtered, d 3 -> 6", &m);
    assert!(m.converged && m.dof == 0 && m.redundant == 0);
}
