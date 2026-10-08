//! C1's accept scenario (docs/ROADMAP.md §C1), headless and by commands
//! alone through `LocalSession` with `gears` compiled in: a bracket, one
//! part, a plate with a pocket, a counterbored hole patterned linearly, two
//! filleted top edges, a spur gear on a datum plane joined to the plate and
//! an edge of the gear's start face filleted downstream, by persistent
//! name. It also writes `tests/docs/bracket/`, the document `arrix eval`
//! prints a golden line for.
//!
//! The layout keeps clear of the known Arris gaps (docs/BACKLOG.md): the
//! holes and the pocket stand away from the fillets, and the gear's edge is
//! one of those that fillets alone (plans/c1-m4 §Open questions, step 1).

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, FeatureId, IdMinter, MeshDetail, NameRoot, ParamId, PartId, PersistentName,
    QuantityKind, Ref, RegionKey, RenderMesh, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, Document, Evaluator, FeatureOutcome, FeatureRecord, FeatureTypeId, Param,
    Part, Session, SketchView, SlotView, expr::Expr, open,
};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, Sketch};
use common::{Client, scenario_directory};

const MM: f64 = 1e-3;

/// Every id the scenario's commands create, from the test's fixed seed.
struct Ids {
    author: AuthorId,
    teeth: ParamId,
    m: ParamId,
    w: ParamId,
    t: ParamId,
    part: PartId,
    plate_sketch: FeatureId,
    plate: FeatureId,
    top: FeatureId,
    pocket_sketch: FeatureId,
    pocket: FeatureId,
    hole_sketch: FeatureId,
    hole: FeatureId,
    pattern: FeatureId,
    edges: FeatureId,
    gear: FeatureId,
    join: FeatureId,
    gear_edge: FeatureId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(2028);
        Ids {
            author: AuthorId(m.next_id()),
            teeth: m.mint(),
            m: m.mint(),
            w: m.mint(),
            t: m.mint(),
            part: m.mint(),
            plate_sketch: m.mint(),
            plate: m.mint(),
            top: m.mint(),
            pocket_sketch: m.mint(),
            pocket: m.mint(),
            hole_sketch: m.mint(),
            hole: m.mint(),
            pattern: m.mint(),
            edges: m.mint(),
            gear: m.mint(),
            join: m.mint(),
            gear_edge: m.mint(),
        }
    }

    fn plate_top(&self) -> PersistentName {
        sweep(TopoKind::Face, self.plate, SweepPartName::EndCap)
    }

    /// The plate's top edge along `line`.
    fn top_edge(&self, line: EntityId) -> Ref {
        Ref::Topo(sweep(
            TopoKind::Edge,
            self.plate,
            SweepPartName::EndEdge(CurveKey(line.0)),
        ))
    }

    /// An edge of the gear's start face that fillets at 0.3 mm
    /// (`frozen_faces.rs` picks the same one in the slice).
    fn gear_edge(&self) -> PersistentName {
        format!("edge:sweep.{}.start-edge.0000000000KH3", self.gear)
            .parse()
            .unwrap()
    }

    /// Every feature of the part, in history order.
    fn features(&self) -> [FeatureId; 12] {
        [
            self.plate_sketch,
            self.plate,
            self.top,
            self.pocket_sketch,
            self.pocket,
            self.hole_sketch,
            self.hole,
            self.pattern,
            self.edges,
            self.gear,
            self.join,
            self.gear_edge,
        ]
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

/// A body slot's volume in cubic millimetres.
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

/// The plate's sketch: `w` × 40 mm from (-30, -20) mm, the corner fixed.
fn plate_sketch() -> (Sketch, [EntityId; 4]) {
    let mut d = Draft::seeded(7);
    let (p, sides) = rectangle(&mut d, (-30.0, -20.0, 60.0, 40.0));
    d.point_mut(p[0]).unwrap().fixed = true;
    let width = d.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: 60.0 * MM,
    });
    d.set_constraint_expr(width, Some("w".into()));
    d.add_constraint(Constraint::Distance {
        a: p[1],
        b: p[2],
        value: 40.0 * MM,
    });
    assert!(d.solve().converged);
    (d.sketch, sides)
}

/// The hole's sketch: its one point at (20, -12) mm, and a 10 mm line along
/// +y from the origin, the pattern's direction.
fn hole_sketch() -> (Sketch, arrix_sketch::PointId, EntityId) {
    let mut d = Draft::seeded(9);
    let hole = d.add_point(Point::new(20.0 * MM, -12.0 * MM));
    let (_, _, line) = d.add_line(0.0, 0.0, 0.0, 10.0 * MM);
    (d.sketch, hole, line)
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

/// Appends `record` to the part's history.
fn add(c: &mut Client, i: &Ids, record: FeatureRecord) {
    let at = c.replica.document().parts()[&i.part].history.len();
    c.submit(Command::AddFeature {
        part: i.part,
        at,
        record,
    });
}

/// Criterion 1: the plate (60 × 40 × 5 mm) with a pocket at the left, a
/// counterbored hole patterned three times along +y at the right, its
/// right and far top edges filleted, then the gear on the top plane,
/// joined, and an edge of the gear's start face filleted.
fn build(c: &mut Client, i: &Ids) {
    let params = [
        (i.teeth, "teeth", QuantityKind::Count, "20"),
        (i.m, "m", QuantityKind::Length, "1 mm"),
        (i.w, "w", QuantityKind::Length, "60 mm"),
        (i.t, "t", QuantityKind::Length, "5 mm"),
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
        name: "Bracket".into(),
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
    add(
        c,
        i,
        with(
            record(i.top, "core.datum-plane", "Top"),
            &[("offset", "0 mm")],
            &[("plane", Ref::Topo(i.plate_top()))],
        ),
    );
    let top = || Some(slot(i.top, "plane"));

    // A pocket 2 mm deep at the left.
    let pocket = plain_rectangle((-28.0, -6.0, 8.0, 12.0));
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

    // The hole at (20, -12) mm, three copies 10 mm apart along +y.
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
        &[("count", "3"), ("spacing", "10 mm")],
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

    // The right and the far top edges, 1 mm.
    let mut edges = with(
        record(i.edges, "core.fillet", "Edges"),
        &[("radius", "1 mm")],
        &[("target", slot(i.plate, "body"))],
    );
    edges.inputs.insert(
        "edges".into(),
        vec![i.top_edge(sides[1]), i.top_edge(sides[2])].into(),
    );
    add(c, i, edges);

    // The gear on the top plane, joined, and its start edge filleted.
    add(
        c,
        i,
        with(
            record(i.gear, "gears.spur", "Spur gear"),
            &[("teeth", "teeth"), ("module", "m"), ("width", "8 mm")],
            &[("plane", slot(i.top, "plane"))],
        ),
    );
    let mut join = with(
        record(i.join, "core.boolean", "Join"),
        &[],
        &[("target", slot(i.plate, "body"))],
    );
    join.type_version = 2;
    join.inputs
        .insert("tools".into(), vec![slot(i.gear, "body")].into());
    join.choices.insert("op".into(), "fuse".into());
    add(c, i, join);
    let mut fillet = with(
        record(i.gear_edge, "core.fillet", "Gear edge"),
        &[("radius", "0.3 mm")],
        &[("target", slot(i.plate, "body"))],
    );
    fillet
        .inputs
        .insert("edges".into(), vec![Ref::Topo(i.gear_edge())].into());
    add(c, i, fillet);
}

/// Every feature evaluated, none failed.
fn all_ok(c: &Client, i: &Ids) {
    for f in i.features() {
        assert!(
            matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
            "{f}: {:?}",
            c.outcome(f)
        );
    }
}

/// The final body's faces by name, each with the box of its triangles in
/// micrometres (rounded, so a face that did not move compares equal).
fn faces(doc: &Document) -> BTreeMap<PersistentName, [i64; 6]> {
    let mut e = Evaluator::new(common::registry());
    e.evaluate(doc);
    let finals = e.final_bodies().to_vec();
    assert_eq!(finals.len(), 1, "one body is left");
    let mesh = e.mesh(&finals[0], MeshDetail::Refined).unwrap();
    face_boxes(&mesh)
}

fn face_boxes(mesh: &RenderMesh) -> BTreeMap<PersistentName, [i64; 6]> {
    mesh.faces
        .iter()
        .map(|face| {
            let mut lo = [f64::MAX; 3];
            let mut hi = [f64::MIN; 3];
            for t in &mesh.triangles[face.range()] {
                for &v in t {
                    let p = mesh.world(mesh.positions[v as usize]);
                    for (k, x) in [p.x, p.y, p.z].into_iter().enumerate() {
                        lo[k] = lo[k].min(x);
                        hi[k] = hi[k].max(x);
                    }
                }
            }
            let um = |x: f64| (x * 1e6).round() as i64;
            (
                face.name.clone(),
                [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]].map(um),
            )
        })
        .collect()
}

#[test]
fn the_bracket_is_built_by_commands_and_survives_a_width_edit() {
    let i = Ids::new();
    let mut c = Client::new(i.author);

    // 1. Built by commands alone, every feature evaluated.
    build(&mut c, &i);
    let built = c.replica.generation();
    let scenario = c.saves[built.0 as usize].clone();
    all_ok(&c, &i);
    // The plate 60 × 40 × 5 mm, less the pocket and the three
    // counterbored holes (4 mm through 5 mm less the 8 mm × 2 mm
    // counterbore, whose cylinder replaces the bore's first 2 mm).
    let pi = std::f64::consts::PI;
    let plate = 60.0 * 40.0 * 5.0;
    let pocket = 8.0 * 12.0 * 2.0;
    let hole = pi * 4.0 * 4.0 * 2.0 + pi * 2.0 * 2.0 * 2.0;
    assert!(near(volume(&c, i.plate), plate));
    assert!(near(volume(&c, i.pocket), plate - pocket));
    assert!(near(volume(&c, i.pattern), plate - pocket - 3.0 * hole));
    let cut = (1.0 - pi / 4.0) * 1.0;
    let edges = volume(&c, i.edges);
    // The two fillets meet at a corner, whose blend is a tenth of a cubic
    // millimetre off the straight runs'.
    let straight = plate - pocket - 3.0 * hole - cut * (40.0 + 60.0);
    assert!((edges - straight).abs() < 0.2, "{edges} vs {straight}");
    let gear = volume(&c, i.gear);
    assert!(near(volume(&c, i.join), edges + gear), "joined");
    // The gear's start edge, filleted at 0.3 mm, adds a sliver of volume to
    // the concave corner at the plate.
    assert!(volume(&c, i.gear_edge) > volume(&c, i.join));

    let narrow = volume(&c, i.gear_edge);
    let before = faces(&open(&c.session.save()).unwrap());
    let untouched = |name: &PersistentName| {
        let f = name.root.feature();
        [i.gear, i.hole, i.pocket, i.pattern].contains(&f)
    };
    assert!(before.keys().filter(|n| untouched(n)).count() > 40);

    // 2. The plate's width edited: every feature re-evaluates, every
    // reference resolves, and the faces the edit did not touch keep their
    // names and their places.
    c.set(i.w, "70 mm");
    all_ok(&c, &i);
    for f in [i.plate, i.pocket, i.pattern, i.edges, i.join, i.gear_edge] {
        assert!(!c.cached(f), "{f} re-evaluated");
    }
    let wider = volume(&c, i.gear_edge);
    let grown = 10.0 * 40.0 * 5.0 - cut * 10.0;
    assert!(near(wider - narrow, grown), "{wider} - {narrow}");
    let after = faces(&open(&c.session.save()).unwrap());
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>(),
        "no face lost or renamed"
    );
    for (name, place) in before.iter().filter(|(n, _)| untouched(n)) {
        // A curved face is meshed to a chord that follows the body's size,
        // so its box may differ by a few micrometres.
        let drift = (0..6).map(|k| (after[name][k] - place[k]).abs()).max();
        assert!(
            drift < Some(20),
            "{name:?} moved: {place:?} to {:?}",
            after[name]
        );
    }
    assert!(
        before.iter().any(|(n, p)| after[n] != *p),
        "the edit moved something"
    );

    scenario_directory("bracket", &scenario);
    eval_golden("bracket.jsonl");
}

/// `arrix eval tests/docs/bracket` prints the golden line: the scenario's
/// volumes, areas and counts (the saved document carries no frozen
/// result, so `gears` is evaluated live). Rewritten with
/// `UPDATE_SNAPSHOTS=1`.
fn eval_golden(file: &str) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(common::workspace())
        .args(["eval", "tests/docs/bracket"])
        .output()
        .expect("run arrix");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(file);
    let stdout = String::from_utf8(out.stdout).unwrap();
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &stdout).unwrap();
    }
    let want = std::fs::read_to_string(&path).expect("read the golden");
    assert_eq!(
        stdout, want,
        "{file}: rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
}
