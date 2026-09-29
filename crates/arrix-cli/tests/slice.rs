//! M1's accept scenario, the live half (docs/ROADMAP.md §M1), headless and
//! by commands alone through `LocalSession` with `gears` compiled in: a
//! sketched plate extruded, a datum plane on its top face, a spur gear on
//! that plane fused to the plate, and a pocket sketched on the joined top
//! face (a `mod` name) and cut. It also writes `tests/docs/slice/`, the
//! document `arrix eval` prints a golden line for.

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    DVec3, FeatureId, IdMinter, NameRoot, ParamId, PartId, PersistentName, QuantityKind, Ref,
    RegionKey, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, Document, FeatureOutcome, FeatureRecord, FeatureTypeId, Param, Part,
    Request, SketchView, SlotView, expr::Expr, open, save,
};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, SketchEdit};
use common::{Client, scenario_directory};

const MM: f64 = 1e-3;

/// Every id the scenario's commands create, from the test's fixed seed.
struct Ids {
    author: AuthorId,
    teeth: ParamId,
    m: ParamId,
    w: ParamId,
    h: ParamId,
    t: ParamId,
    part: PartId,
    plate_sketch: FeatureId,
    plate: FeatureId,
    base: FeatureId,
    gear: FeatureId,
    join: FeatureId,
    pocket_sketch: FeatureId,
    pocket: FeatureId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(2026);
        Ids {
            author: AuthorId(m.next_id()),
            teeth: m.mint(),
            m: m.mint(),
            w: m.mint(),
            h: m.mint(),
            t: m.mint(),
            part: m.mint(),
            plate_sketch: m.mint(),
            plate: m.mint(),
            base: m.mint(),
            gear: m.mint(),
            join: m.mint(),
            pocket_sketch: m.mint(),
            pocket: m.mint(),
        }
    }

    /// The plate's top face as the extrude names it, and as it is named
    /// once the join has holed it.
    fn plate_top(&self) -> PersistentName {
        PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Sweep {
                feature: self.plate,
                part: SweepPartName::EndCap,
            },
            chain: vec![],
        }
    }

    fn joined_top(&self) -> PersistentName {
        format!("face:sweep.{}.end-cap/mod.{}.0", self.plate, self.join)
            .parse()
            .unwrap()
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
    }
}

fn with(mut r: FeatureRecord, params: &[(&str, &str)], inputs: &[(&str, Ref)]) -> FeatureRecord {
    r.params = params
        .iter()
        .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
        .collect();
    r.inputs = inputs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
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
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// A sketch feature on `plane` (the world XY without one), with a
/// rectangle of `w` × `h` whose corner `(x, y)` is fixed, all in the
/// sketch's own millimetres; `w_expr` and `h_expr` are its dimensions'
/// expressions, if any. Returns the key of its one region.
fn rectangle_sketch(
    c: &mut Client,
    (feature, at): (FeatureId, usize),
    part: PartId,
    plane: Option<Ref>,
    (x, y, w, h): (f64, f64, f64, f64),
    exprs: (Option<&str>, Option<&str>),
) -> RegionKey {
    let mut r = record(feature, "core.sketch", &format!("Sketch {at}"));
    r.sketch = Some(Box::default());
    r.inputs.extend(plane.map(|p| ("plane".to_string(), p)));
    c.submit(Command::AddFeature {
        part,
        at,
        record: r,
    });
    let mut draft = Draft::seeded(7);
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let p = corners.map(|(cx, cy)| draft.add_point(Point::new(cx * MM, cy * MM)));
    let sides: Vec<EntityId> = (0..4)
        .map(|i| {
            draft.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    draft.point_mut(p[0]).unwrap().fixed = true;
    for (i, line) in sides.iter().enumerate() {
        draft.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line: *line }
        } else {
            Constraint::Vertical { line: *line }
        });
    }
    for (a, b, value, expr) in [(0, 1, w, exprs.0), (1, 2, h, exprs.1)] {
        let id = draft.add_constraint(Constraint::Distance {
            a: p[a],
            b: p[b],
            value: value * MM,
        });
        draft.set_constraint_expr(id, expr.map(String::from));
    }
    assert!(draft.solve().converged);
    let stored = {
        let (_, _, record) = c.replica.document().feature(feature).unwrap();
        record.sketch.as_deref().unwrap().clone()
    };
    let edit = SketchEdit::diff(&stored, &draft);
    c.submit(Command::SketchEdit { feature, edit });
    let v = view(c, feature);
    assert_eq!((v.dof, v.regions.len()), (0, 1));
    v.regions[0].key.clone()
}

fn extrude(
    id: FeatureId,
    name: &str,
    sketch: FeatureId,
    key: RegionKey,
    mode: &str,
) -> FeatureRecord {
    let mut r = with(
        record(id, "core.extrude", name),
        &[],
        &[(
            "region",
            Ref::Region {
                feature: sketch,
                key,
            },
        )],
    );
    r.choices.insert("mode".into(), mode.into());
    r
}

/// Criterion 1: parameters, the plate, the datum plane on its top face,
/// the gear on it, the join, and the pocket on the joined top face.
fn build(c: &mut Client, i: &Ids) {
    let params = [
        (i.teeth, "teeth", QuantityKind::Count, "20"),
        (i.m, "m", QuantityKind::Length, "1 mm"),
        (i.w, "w", QuantityKind::Length, "40 mm"),
        (i.h, "h", QuantityKind::Length, "30 mm"),
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
        name: "Plate and gear".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
    };
    c.submit(Command::AddPart { part });

    let key = rectangle_sketch(
        c,
        (i.plate_sketch, 0),
        i.part,
        None,
        (-20.0, -15.0, 40.0, 30.0),
        (Some("w"), Some("h")),
    );
    let mut plate = extrude(i.plate, "Plate", i.plate_sketch, key, "new");
    plate
        .params
        .insert("distance".into(), Expr::parse("t").unwrap());
    c.submit(Command::AddFeature {
        part: i.part,
        at: 1,
        record: plate,
    });
    let base = with(
        record(i.base, "core.datum-plane", "Plate top"),
        &[("offset", "0 mm")],
        &[("plane", Ref::Topo(i.plate_top()))],
    );
    let gear = with(
        record(i.gear, "gears.spur", "Spur gear"),
        &[("teeth", "teeth"), ("module", "m"), ("width", "8 mm")],
        &[("plane", slot(i.base, "plane"))],
    );
    let mut join = with(
        record(i.join, "core.boolean", "Join"),
        &[],
        &[
            ("target", slot(i.plate, "body")),
            ("tool", slot(i.gear, "body")),
        ],
    );
    join.choices.insert("op".into(), "fuse".into());
    for (at, record) in [(2, base), (3, gear), (4, join)] {
        c.submit(Command::AddFeature {
            part: i.part,
            at,
            record,
        });
    }
    let key = rectangle_sketch(
        c,
        (i.pocket_sketch, 5),
        i.part,
        Some(Ref::Topo(i.joined_top())),
        (0.0, 0.0, 6.0, 6.0),
        (None, None),
    );
    let mut pocket = extrude(i.pocket, "Pocket", i.pocket_sketch, key, "cut");
    pocket
        .params
        .insert("distance".into(), Expr::parse("-6 mm").unwrap());
    pocket.inputs.insert("target".into(), slot(i.plate, "body"));
    c.submit(Command::AddFeature {
        part: i.part,
        at: 6,
        record: pocket,
    });
}

/// Every feature evaluated, none failed.
fn all_ok(c: &Client, i: &Ids) {
    for f in [
        i.plate_sketch,
        i.plate,
        i.base,
        i.gear,
        i.join,
        i.pocket_sketch,
        i.pocket,
    ] {
        assert!(
            matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
            "{f}: {:?}",
            c.outcome(f)
        );
    }
}

#[test]
fn the_slice_live() {
    let i = Ids::new();
    let mut c = Client::new(i.author);

    // 1. Built by commands alone.
    build(&mut c, &i);
    let built = c.replica.generation();
    let scenario = c.saves[built.0 as usize].clone();
    all_ok(&c, &i);
    let gear = volume(&c, i.gear);
    assert!(near(volume(&c, i.plate), 6000.0));
    assert!(
        near(volume(&c, i.join), 6000.0 + gear),
        "{}",
        volume(&c, i.join)
    );
    let pocket = 6.0 * 6.0 * 5.0;
    assert!(
        near(volume(&c, i.pocket), 6000.0 + gear - pocket),
        "{}",
        volume(&c, i.pocket)
    );
    let top = view(&c, i.pocket_sketch).plane;
    assert!((top.origin().z - 5.0 * MM).abs() < 1e-12, "{top:?}");
    assert!((top.z_axis() - DVec3::Z).length() < 1e-12);

    // 2. The plate's width edited: everything after it re-evaluates and
    // every reference resolves, the pocket's plane through the join.
    c.set(i.w, "50 mm");
    all_ok(&c, &i);
    assert!(!c.cached(i.plate) && !c.cached(i.join) && !c.cached(i.pocket));
    assert!(near(volume(&c, i.plate), 7500.0));
    assert!(near(volume(&c, i.pocket), 7500.0 + gear - pocket));
    c.set(i.w, "40 mm");
    assert!(near(volume(&c, i.pocket), 6000.0 + gear - pocket));

    // 3. Saves are deterministic and round-trip; undo and redo walk the
    // saved bytes back and forth.
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

    scenario_directory("slice", &scenario);
}
