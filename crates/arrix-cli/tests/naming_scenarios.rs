//! M2's accept at the document level (docs/ROADMAP.md §M2, docs/DATA-MODEL.md
//! §Persistent naming), headless and by commands alone through
//! `LocalSession`: one part, a plate with a pocket, a counterbored hole
//! patterned linearly, a boss, a fillet on a top edge and a sketch that
//! projects the plate's top edges, then five edits, each followed by a
//! check that the downstream `Ref::Topo`s (datum planes standing on the
//! faces they name) resolve and the faces keep their names. The opposite
//! cases, a referenced entity gone, end in `ref.lost` with candidates
//! while the features that do not read it evaluate. Every command is
//! undone to the bytes of the save before it. It also writes
//! `tests/docs/naming/`.
//!
//! The scenarios keep clear of the known Arris gaps (docs/BACKLOG.md): a
//! hole across a fillet, a split piece's order on a redraw, and a
//! counterbored copy centred on the plate's edge, which is why the count
//! goes no higher than 4.

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, Diagnostic, FeatureId, Frame, IdMinter, NameRoot, NameStep, ParamId, PartId,
    PersistentName, QuantityKind, Ref, RegionKey, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, Document, FeatureEdit, FeatureOutcome, FeatureRecord, FeatureTypeId, Param,
    Part, Request, SketchView, SlotView, expr::Expr, open, save,
};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, Sketch, SketchEdit};
use common::{Client, scenario_directory};

const MM: f64 = 1e-3;

/// Every id the scenario's commands create, from the test's fixed seed.
struct Ids {
    author: AuthorId,
    w: ParamId,
    t: ParamId,
    n: ParamId,
    part: PartId,
    plate_sketch: FeatureId,
    plate: FeatureId,
    top: FeatureId,
    pocket_sketch: FeatureId,
    pocket: FeatureId,
    hole_sketch: FeatureId,
    hole: FeatureId,
    pattern: FeatureId,
    boss_sketch: FeatureId,
    boss: FeatureId,
    fillet: FeatureId,
    projection: FeatureId,
    on_side: FeatureId,
    on_floor_1: FeatureId,
    on_floor_2: FeatureId,
    on_boss: FeatureId,
    // Made by the insert scenario.
    slot_sketch: FeatureId,
    slot: FeatureId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(2027);
        Ids {
            author: AuthorId(m.next_id()),
            w: m.mint(),
            t: m.mint(),
            n: m.mint(),
            part: m.mint(),
            plate_sketch: m.mint(),
            plate: m.mint(),
            top: m.mint(),
            pocket_sketch: m.mint(),
            pocket: m.mint(),
            hole_sketch: m.mint(),
            hole: m.mint(),
            pattern: m.mint(),
            boss_sketch: m.mint(),
            boss: m.mint(),
            fillet: m.mint(),
            projection: m.mint(),
            on_side: m.mint(),
            on_floor_1: m.mint(),
            on_floor_2: m.mint(),
            on_boss: m.mint(),
            slot_sketch: m.mint(),
            slot: m.mint(),
        }
    }

    /// The plate's top face, as the extrude names it.
    fn plate_top(&self) -> PersistentName {
        sweep(TopoKind::Face, self.plate, SweepPartName::EndCap)
    }

    /// The face the plate's side along `line` is, and the edge it ends
    /// in at the top.
    fn side(&self, line: EntityId) -> PersistentName {
        sweep(
            TopoKind::Face,
            self.plate,
            SweepPartName::Side(CurveKey(line.0)),
        )
    }

    fn top_edge(&self, line: EntityId) -> Ref {
        Ref::Topo(sweep(
            TopoKind::Edge,
            self.plate,
            SweepPartName::EndEdge(CurveKey(line.0)),
        ))
    }

    /// The boss's top face.
    fn boss_top(&self) -> PersistentName {
        sweep(TopoKind::Face, self.boss, SweepPartName::EndCap)
    }

    /// Copy `i` of the hole's counterbore floor: the tool's third face as
    /// copy `i` of the pattern, cut as the pattern's `i`-th boolean.
    fn floor(&self, i: u32) -> PersistentName {
        let tool = PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Revolve {
                feature: self.hole,
                index: 0,
                part: SweepPartName::Side(CurveKey(arrix_core::Id(3))),
            },
            chain: vec![],
        };
        let instance = NameStep::Instance {
            feature: self.pattern,
            index: i,
        };
        let copy = PersistentName {
            chain: vec![instance.clone()],
            ..tool.clone()
        };
        PersistentName {
            chain: vec![
                instance,
                NameStep::Generated {
                    feature: self.pattern,
                    boolean: i - 1,
                    from: vec![copy],
                },
            ],
            ..tool
        }
    }
}

fn sweep(kind: TopoKind, feature: FeatureId, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            index: 0,
            feature,
            part,
        },
        chain: vec![],
    }
}

fn slot(feature: FeatureId, name: &str) -> Ref {
    Ref::Slot {
        feature,
        slot: SlotName::new(name).unwrap(),
    }
}

fn record(id: FeatureId, ty: &str, name: &str) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(ty).unwrap(),
        type_version: 1,
        name: name.into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

fn with(mut r: FeatureRecord, params: &[(&str, &str)], inputs: &[(&str, Ref)]) -> FeatureRecord {
    r.params = params
        .iter()
        .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
        .collect();
    r.inputs = inputs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone().into()))
        .collect();
    r
}

fn sketch_feature(id: FeatureId, name: &str, plane: Option<Ref>, sketch: Sketch) -> FeatureRecord {
    let mut r = record(id, "core.sketch", name);
    r.sketch = Some(Box::new(sketch));
    r.inputs
        .extend(plane.map(|p| ("plane".to_string(), p.into())));
    r
}

fn view(c: &Client, sketch: FeatureId) -> &SketchView {
    match c.outcome(sketch) {
        FeatureOutcome::Ok { slots, .. } => match slots.values().next() {
            Some(SlotView::Sketch(v)) => v,
            other => panic!("feature {sketch}: {other:?}"),
        },
        other => panic!("feature {sketch}: {other:?}"),
    }
}

fn volume(c: &Client, feature: FeatureId) -> f64 {
    match c.outcome(feature) {
        FeatureOutcome::Ok { slots, .. } => match slots.get(&SlotName::new("body").unwrap()) {
            Some(SlotView::Body(m)) => m.volume * 1e9,
            other => panic!("feature {feature}: {other:?}"),
        },
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn failure(c: &Client, feature: FeatureId) -> &Diagnostic {
    match c.outcome(feature) {
        FeatureOutcome::Failed { diagnostic, .. } => diagnostic,
        other => panic!("feature {feature}: expected a failure, got {other:?}"),
    }
}

/// A rectangle from (`x`, `y`) of `w` × `h` mm, four lines anticlockwise
/// from the bottom.
fn rectangle(
    draft: &mut Draft,
    (x, y, w, h): (f64, f64, f64, f64),
) -> ([arrix_sketch::PointId; 4], [EntityId; 4]) {
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let p = corners.map(|(cx, cy)| draft.add_point(Point::new(cx * MM, cy * MM)));
    let sides = [0, 1, 2, 3].map(|i| {
        let line = draft.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
        draft.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
        line
    });
    (p, sides)
}

fn plain_rectangle(r: (f64, f64, f64, f64)) -> Sketch {
    let mut d = Draft::seeded(21);
    rectangle(&mut d, r);
    d.sketch
}

/// The plate's sketch: `w` × 30 mm from the origin, the corner fixed.
fn plate_sketch() -> (Sketch, [EntityId; 4]) {
    let mut d = Draft::seeded(7);
    let (p, sides) = rectangle(&mut d, (0.0, 0.0, 50.0, 30.0));
    d.point_mut(p[0]).unwrap().fixed = true;
    let width = d.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: 50.0 * MM,
    });
    d.set_constraint_expr(width, Some("w".into()));
    d.add_constraint(Constraint::Distance {
        a: p[1],
        b: p[2],
        value: 30.0 * MM,
    });
    assert!(d.solve().converged);
    (d.sketch, sides)
}

/// The hole's sketch: its one point at (12, 15) mm, and a 10 mm line along
/// +x from the origin, the pattern's direction.
fn hole_sketch() -> (Sketch, arrix_sketch::PointId, EntityId) {
    let mut d = Draft::seeded(9);
    let hole = d.add_point(Point::new(12.0 * MM, 15.0 * MM));
    let (_, _, line) = d.add_line(0.0, 0.0, 10.0 * MM, 0.0);
    (d.sketch, hole, line)
}

/// The sketch on the top plane that projects the plate's bottom, top and
/// left top edges (the right one is filleted).
fn projection_sketch(i: &Ids, sides: [EntityId; 4]) -> (Sketch, [EntityId; 3]) {
    let mut d = Draft::seeded(8);
    let projected = [sides[0], sides[2], sides[3]].map(|side| {
        let (_, _, line) = d.add_line(0.0, 0.0, 0.001, 0.0);
        d.set_source(line, Some(i.top_edge(side))).unwrap();
        line
    });
    (d.sketch, projected)
}

fn region_of(c: &Client, sketch: FeatureId) -> RegionKey {
    let v = view(c, sketch);
    assert_eq!(v.regions.len(), 1, "{sketch}");
    v.regions[0].key.clone()
}

fn extrude(
    id: FeatureId,
    name: &str,
    sketch: FeatureId,
    key: RegionKey,
    mode: &str,
    distance: &str,
    target: Option<Ref>,
) -> FeatureRecord {
    let mut r = with(
        record(id, "core.extrude", name),
        &[("distance", distance)],
        &[(
            "region",
            Ref::Region {
                feature: sketch,
                key,
            },
        )],
    );
    r.choices.insert("mode".into(), mode.into());
    r.inputs
        .extend(target.map(|t| ("target".to_string(), t.into())));
    r
}

fn datum(id: FeatureId, name: &str, on: Ref) -> FeatureRecord {
    with(
        record(id, "core.datum-plane", name),
        &[("offset", "0 mm")],
        &[("plane", on)],
    )
}

struct Built {
    sides: [EntityId; 4],
    point: arrix_sketch::PointId,
    direction: EntityId,
    projected: [EntityId; 3],
}

/// Appends `record` to the part's history.
fn add(c: &mut Client, i: &Ids, record: FeatureRecord) {
    let at = c.replica.document().parts()[&i.part].history.len();
    c.submit(Command::AddFeature {
        part: i.part,
        at,
        record,
    });
}

/// Criterion 1: the plate (50 × 30 × 5 mm), the plane on its top, a
/// pocket, a counterbored hole patterned three times, a boss, a fillet on
/// the plate's right top edge, the projection sketch and the probes.
fn build(c: &mut Client, i: &Ids) -> Built {
    let params = [
        (i.w, "w", QuantityKind::Length, "50 mm"),
        (i.t, "t", QuantityKind::Length, "5 mm"),
        (i.n, "n", QuantityKind::Count, "3"),
    ];
    for (param, name, kind, expr) in params {
        let record = Param {
            name: name.into(),
            kind,
            expr: Expr::parse(expr).unwrap(),
        };
        c.submit(Command::AddParam { param, record });
    }
    let part = Part {
        id: i.part,
        name: "Plate".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
    };
    c.submit(Command::AddPart { part });

    let (sketch, sides) = plate_sketch();
    add(
        c,
        i,
        sketch_feature(i.plate_sketch, "Plate sketch", None, sketch),
    );
    let key = region_of(c, i.plate_sketch);
    add(
        c,
        i,
        extrude(i.plate, "Plate", i.plate_sketch, key, "new", "t", None),
    );
    let on_plate = || Some(slot(i.plate, "body"));
    add(c, i, datum(i.top, "Top", Ref::Topo(i.plate_top())));
    let top = || Some(slot(i.top, "plane"));

    // A pocket 2 mm deep at the right, away from the hole's copies.
    let pocket = plain_rectangle((38.0, 3.0, 8.0, 5.0));
    add(
        c,
        i,
        sketch_feature(i.pocket_sketch, "Pocket sketch", top(), pocket),
    );
    let key = region_of(c, i.pocket_sketch);
    add(
        c,
        i,
        extrude(
            i.pocket,
            "Pocket",
            i.pocket_sketch,
            key,
            "cut",
            "-2 mm",
            on_plate(),
        ),
    );

    // The hole at (12, 15) mm, patterned 9 mm apart along the sketch line.
    let (hole, point, direction) = hole_sketch();
    add(
        c,
        i,
        sketch_feature(i.hole_sketch, "Hole sketch", top(), hole),
    );
    let mut hole = with(
        record(i.hole, "core.hole", "Hole"),
        &[
            ("diameter", "4 mm"),
            ("depth", "4 mm"),
            ("cb-diameter", "8 mm"),
            ("cb-depth", "2 mm"),
        ],
        &[("target", slot(i.plate, "body"))],
    );
    hole.choices.insert("kind".into(), "counterbore".into());
    hole.inputs.insert(
        "points".into(),
        vec![Ref::Sketch {
            feature: i.hole_sketch,
            entity: point,
        }]
        .into(),
    );
    add(c, i, hole);
    let mut pattern = with(
        record(i.pattern, "core.pattern-linear", "Pattern"),
        &[("count", "n"), ("spacing", "9 mm")],
        &[
            ("target", slot(i.plate, "body")),
            (
                "direction",
                Ref::Sketch {
                    feature: i.hole_sketch,
                    entity: direction,
                },
            ),
        ],
    );
    pattern
        .inputs
        .insert("tools".into(), vec![Ref::Feature(i.hole)].into());
    add(c, i, pattern);

    // A boss 3 mm high at the far corner, joined.
    let boss = plain_rectangle((38.0, 20.0, 8.0, 7.0));
    add(
        c,
        i,
        sketch_feature(i.boss_sketch, "Boss sketch", top(), boss),
    );
    let key = region_of(c, i.boss_sketch);
    add(
        c,
        i,
        extrude(
            i.boss,
            "Boss",
            i.boss_sketch,
            key,
            "join",
            "3 mm",
            on_plate(),
        ),
    );

    // The fillet, 1 mm, on the right top edge.
    add(
        c,
        i,
        with(
            record(i.fillet, "core.fillet", "Fillet"),
            &[("radius", "1 mm")],
            &[("target", slot(i.plate, "body"))],
        ),
    );
    c.submit(Command::EditFeature {
        feature: i.fillet,
        edit: FeatureEdit {
            inputs: [("edges".into(), Some(vec![i.top_edge(sides[1])].into()))].into(),
            ..FeatureEdit::default()
        },
    });

    let (sketch, projected) = projection_sketch(i, sides);
    add(
        c,
        i,
        sketch_feature(i.projection, "Top edges", top(), sketch),
    );

    // The probes: a plane standing on a face each, by its name.
    let probes = [
        (i.on_side, "On right side", i.side(sides[1])),
        (i.on_floor_1, "On floor 1", i.floor(1)),
        (i.on_floor_2, "On floor 2", i.floor(2)),
        (i.on_boss, "On boss top", i.boss_top()),
    ];
    for (id, name, face) in probes {
        add(c, i, datum(id, name, Ref::Topo(face)));
    }
    Built {
        sides,
        point,
        direction,
        projected,
    }
}

/// Every feature evaluated, none failed.
fn all_ok(c: &Client, i: &Ids) {
    for f in everything(c, i) {
        assert!(
            matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
            "{f}: {:?}",
            c.outcome(f)
        );
    }
}

fn everything(c: &Client, i: &Ids) -> Vec<FeatureId> {
    c.replica.document().parts()[&i.part].history.clone()
}

/// The probes' frames (a plane's origin is where its stand-in face puts it:
/// the world origin's foot on a boss top, the face's own point elsewhere): the right side's, copy 1's floor, copy 2's floor
/// and the boss top's.
fn probes(c: &Client, i: &Ids) -> [Frame; 4] {
    [i.on_side, i.on_floor_1, i.on_floor_2, i.on_boss].map(|f| c.plane(f))
}

fn at(frame: &Frame, x: f64, z: f64) -> bool {
    let o = frame.origin();
    (o.x - x * MM).abs() < 1e-9 && (o.z - z * MM).abs() < 1e-9
}

/// What the plate holds, mm³: the plate less the pocket and the holes plus
/// the boss.
fn plate_volume(holes: f64) -> f64 {
    let hole = std::f64::consts::PI * (16.0 * 2.0 + 4.0 * 2.0);
    7500.0 - 80.0 - holes * hole + 8.0 * 7.0 * 3.0
}

/// The fillet takes the quarter-round corner off the 30 mm edge.
fn fillet_cut(length: f64) -> f64 {
    (1.0 - std::f64::consts::FRAC_PI_4) * length
}

#[test]
fn the_naming_scenarios_by_command() {
    let i = Ids::new();
    let mut c = Client::new(i.author);

    // 1. Built by commands alone, every feature evaluated.
    let b = build(&mut c, &i);
    let built = c.replica.generation();
    let scenario = c.saves[built.0 as usize].clone();
    all_ok(&c, &i);
    assert!(
        near(volume(&c, i.boss), plate_volume(3.0)),
        "{}",
        volume(&c, i.boss)
    );
    assert!(near(
        volume(&c, i.fillet),
        plate_volume(3.0) - fillet_cut(30.0)
    ));
    let start = probes(&c, &i);
    assert!(at(&start[0], 50.0, 0.0), "{:?}", start[0]);
    assert!(at(&start[1], 21.0, 3.0), "{:?}", start[1]);
    assert!(at(&start[2], 30.0, 3.0), "{:?}", start[2]);
    assert!(at(&start[3], 0.0, 8.0), "{:?}", start[3]);

    // 2. The width edited under the fillet: it follows the edge, the side
    // face's plane moves with it, every other name holds.
    c.set(i.w, "60 mm");
    all_ok(&c, &i);
    assert!(near(
        volume(&c, i.boss),
        plate_volume(3.0) + 10.0 * 30.0 * 5.0
    ));
    assert!(near(
        volume(&c, i.fillet),
        plate_volume(3.0) + 10.0 * 30.0 * 5.0 - fillet_cut(30.0)
    ));
    let now = probes(&c, &i);
    assert!(at(&now[0], 60.0, 0.0), "{:?}", now[0]);
    assert_eq!(
        now[1..],
        start[1..],
        "the pattern and the boss did not move"
    );
    c.set(i.w, "50 mm");

    // 3. The hole moved across the plate, clear of the fillet: its copies
    // move with it, their faces keep their names.
    let moved = {
        let stored = c.replica.document().feature(i.hole_sketch).unwrap().2;
        let stored = stored.sketch.as_deref().unwrap().clone();
        let mut draft = Draft::with_sketch(stored.clone(), IdMinter::new(77));
        draft.point_mut(b.point).unwrap().x = 14.0 * MM;
        SketchEdit::diff(&stored, &draft)
    };
    c.submit(Command::SketchEdit {
        feature: i.hole_sketch,
        edit: moved,
    });
    all_ok(&c, &i);
    let now = probes(&c, &i);
    assert!(at(&now[1], 23.0, 3.0), "{:?}", now[1]);
    assert!(at(&now[2], 32.0, 3.0), "{:?}", now[2]);
    assert_eq!((now[0], now[3]), (start[0], start[3]));
    assert!(near(volume(&c, i.boss), plate_volume(3.0)));

    // 4. The pattern's count raised, then lowered: the copies that stay
    // keep their faces, the one that goes is lost, with candidates, and
    // what does not read it evaluates.
    c.set(i.n, "4");
    all_ok(&c, &i);
    assert!(near(volume(&c, i.boss), plate_volume(4.0)));
    let four = probes(&c, &i);
    assert_eq!((four[1], four[2]), (now[1], now[2]));
    c.set(i.n, "2");
    assert!(near(volume(&c, i.boss), plate_volume(2.0)));
    let lost = failure(&c, i.on_floor_2).clone();
    assert_eq!(lost.code.as_str(), "ref.lost", "{lost:?}");
    assert!(lost.refs.contains(&Ref::Topo(i.floor(2))));
    assert!(!lost.candidates.is_empty(), "{lost:?}");
    for f in [i.on_side, i.on_floor_1, i.on_boss, i.projection, i.fillet] {
        assert!(
            matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
            "{f}: {:?}",
            c.outcome(f)
        );
    }
    assert_eq!(c.plane(i.on_floor_1), now[1]);
    c.set(i.n, "3");
    all_ok(&c, &i);

    // 5. A feature inserted before the filleted edge: a second pocket, a
    // sketch and an extrude, in front of the fillet. Nothing downstream
    // is renamed.
    let before = probes(&c, &i);
    let at_fillet = c.replica.document().parts()[&i.part]
        .history
        .iter()
        .position(|f| *f == i.fillet)
        .unwrap();
    let pocket = plain_rectangle((38.0, 11.0, 8.0, 5.0));
    c.submit(Command::AddFeature {
        part: i.part,
        at: at_fillet,
        record: sketch_feature(
            i.slot_sketch,
            "Slot sketch",
            Some(slot(i.top, "plane")),
            pocket,
        ),
    });
    let key = region_of(&c, i.slot_sketch);
    c.submit(Command::AddFeature {
        part: i.part,
        at: at_fillet + 1,
        record: extrude(
            i.slot,
            "Slot",
            i.slot_sketch,
            key,
            "cut",
            "-1 mm",
            Some(slot(i.plate, "body")),
        ),
    });
    all_ok(&c, &i);
    assert_eq!(probes(&c, &i), before);
    assert!(near(
        volume(&c, i.fillet),
        plate_volume(3.0) - 8.0 * 5.0 - fillet_cut(30.0)
    ));

    // 6. A sketch curve redrawn between its two kept neighbours: the
    // plate's bottom side removed and drawn again between the same
    // points. What stands on the other sides and on the other features
    // keeps its name; the bottom top edge, which the projection sketch
    // holds, is lost, with candidates, and the plate and the probes
    // evaluate.
    let redrawn = {
        let stored = c.replica.document().feature(i.plate_sketch).unwrap().2;
        let stored = stored.sketch.as_deref().unwrap().clone();
        let mut draft = Draft::with_sketch(stored.clone(), IdMinter::new(99));
        let (start, end) = draft.entity(b.sides[0]).unwrap().line_ends().unwrap();
        draft.remove_entity(b.sides[0]);
        let line = draft.add_entity(Entity::Line { start, end });
        draft.add_constraint(Constraint::Horizontal { line });
        (SketchEdit::diff(&stored, &draft), line)
    };
    c.submit(Command::SketchEdit {
        feature: i.plate_sketch,
        edit: redrawn.0,
    });
    let key = region_of(&c, i.plate_sketch);
    c.submit(Command::EditFeature {
        feature: i.plate,
        edit: FeatureEdit {
            inputs: [(
                "region".into(),
                Some(
                    Ref::Region {
                        feature: i.plate_sketch,
                        key,
                    }
                    .into(),
                ),
            )]
            .into(),
            ..FeatureEdit::default()
        },
    });
    let lost = failure(&c, i.projection).clone();
    assert_eq!(lost.code.as_str(), "ref.lost", "{lost:?}");
    assert!(lost.refs.contains(&i.top_edge(b.sides[0])), "{lost:?}");
    assert!(lost.candidates.contains(&i.top_edge(redrawn.1)), "{lost:?}");
    for f in everything(&c, &i) {
        if f != i.projection {
            assert!(
                matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
                "{f}: {:?}",
                c.outcome(f)
            );
        }
    }
    assert_eq!(probes(&c, &i), before, "the other sides and features hold");
    let _ = (b.direction, b.projected);

    // 7. Saves are deterministic and round-trip; every command undone
    // returns the bytes of the save before it, and redo walks back.
    let last = c.replica.generation();
    assert_eq!(save(c.replica.document()).0, c.saves[last.0 as usize].0);
    let reopened: Document = open(&c.saves[last.0 as usize]).unwrap();
    assert_eq!(save(&reopened).0, c.saves[last.0 as usize].0);
    for back in (0..last.0).rev() {
        c.request(Request::Undo { author: i.author });
        assert!(
            save(c.replica.document()).0 == c.saves[back as usize].0,
            "undone to generation {back}"
        );
    }
    for forward in 1..=last.0 {
        c.request(Request::Redo { author: i.author });
        assert!(
            save(c.replica.document()).0 == c.saves[forward as usize].0,
            "redone to generation {forward}"
        );
    }

    scenario_directory("naming", &scenario);
}
