//! M1's scenario document (plans/boundary-m1 step 8): a plate, a hexagonal
//! standoff on its top face built by the Python plugin
//! `python/examples/standoff` (Tier 2, by kernel callbacks), and a core
//! fillet downstream that names one of the standoff's edges. Built by
//! commands alone through `LocalSession`; it writes `tests/docs/standoff/`,
//! the document `arrix eval --plugin python/examples/standoff` prints a
//! golden line for (`tests/eval.rs`).

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, FeatureId, Id, IdMinter, NameRoot, ParamId, PartId, PersistentName, QuantityKind,
    Ref, RegionKey, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, Evaluator, FeatureEdit, FeatureOutcome, FeatureRecord, FeatureTypeId, Param,
    Part, Registry, SlotView, expr::Expr,
};
use arrix_kernel::{KernelCall, KernelOp};
use arrix_plugin_host::register_tier2;
use arrix_sketch::{Constraint, Draft, Entity, Point, Sketch};
use common::{Client, scenario_directory, workspace};

const MM: f64 = 1e-3;

/// Every id the scenario's commands create, from the test's fixed seed.
struct Ids {
    author: AuthorId,
    w: ParamId,
    part: PartId,
    plate_sketch: FeatureId,
    plate: FeatureId,
    standoff: FeatureId,
    fillet: FeatureId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(202_610);
        Ids {
            author: AuthorId(m.next_id()),
            w: m.mint(),
            part: m.mint(),
            plate_sketch: m.mint(),
            plate: m.mint(),
            standoff: m.mint(),
            fillet: m.mint(),
        }
    }

    /// The plate's top face.
    fn plate_top(&self) -> PersistentName {
        sweep(TopoKind::Face, self.plate, SweepPartName::EndCap)
    }

    /// The top edge of the standoff's hexagon side `key`: named by the
    /// standoff's own sweep, rooted at the curve key its plugin gave.
    fn standoff_edge(&self, key: u64) -> PersistentName {
        sweep(
            TopoKind::Edge,
            self.standoff,
            SweepPartName::EndEdge(CurveKey(Id(key))),
        )
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

fn standoff_dir() -> std::path::PathBuf {
    workspace().join("python/examples/standoff")
}

/// The core types and the standoff plugin, started as a child process.
fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier2(&mut r, &standoff_dir()).expect("the standoff plugin starts");
    r
}

fn slot(feature: FeatureId, name: &str) -> Ref {
    Ref::Slot {
        feature,
        slot: SlotName::new(name).unwrap(),
    }
}

fn record(
    id: FeatureId,
    ty: &str,
    name: &str,
    params: &[(&str, &str)],
    inputs: &[(&str, Ref)],
) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(ty).unwrap(),
        type_version: 1,
        name: name.into(),
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
            .collect(),
        choices: BTreeMap::new(),
        inputs: inputs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone().into()))
            .collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

/// The plate's sketch: `w` × 30 mm from the origin, the corner fixed.
fn plate_sketch() -> Sketch {
    let mut d = Draft::seeded(7);
    let corners = [(0.0, 0.0), (50.0, 0.0), (50.0, 30.0), (0.0, 30.0)];
    let p = corners.map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)));
    for i in 0..4 {
        let line = d.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
        d.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
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
    d.sketch
}

fn region_of(c: &Client, sketch: FeatureId) -> RegionKey {
    match c.outcome(sketch) {
        FeatureOutcome::Ok { slots, .. } => match slots.values().next() {
            Some(SlotView::Sketch(v)) => {
                assert_eq!(v.regions.len(), 1);
                v.regions[0].key.clone()
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

fn add(c: &mut Client, i: &Ids, record: FeatureRecord) {
    let at = c.replica.document().parts()[&i.part].history.len();
    c.submit(Command::AddFeature {
        part: i.part,
        at,
        record,
    });
}

/// The plate, the standoff on its top face, the fillet on its top ring.
fn build(c: &mut Client, i: &Ids) {
    let param = Param {
        name: "w".into(),
        kind: QuantityKind::Length,
        expr: Expr::parse("50 mm").unwrap(),
    };
    c.submit(Command::AddParam {
        param: i.w,
        record: param,
    });
    let part = Part {
        id: i.part,
        name: "Plate with a standoff".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
    };
    c.submit(Command::AddPart { part });

    let mut sketch = record(i.plate_sketch, "core.sketch", "Plate sketch", &[], &[]);
    sketch.sketch = Some(Box::new(plate_sketch()));
    add(c, i, sketch);
    let key = region_of(c, i.plate_sketch);
    let region = Ref::Region {
        feature: i.plate_sketch,
        key,
    };
    let mut plate = record(
        i.plate,
        "core.extrude",
        "Plate",
        &[("distance", "5 mm")],
        &[("region", region)],
    );
    plate.choices.insert("mode".into(), "new".into());
    add(c, i, plate);

    // The standoff stands on the plate's top face, by its name.
    add(
        c,
        i,
        record(
            i.standoff,
            "standoff.standoff",
            "Standoff",
            &[("x", "25 mm"), ("y", "15 mm")],
            &[("plane", Ref::Topo(i.plate_top()))],
        ),
    );

    // A 0.3 mm fillet on the top ring of the hexagon, six edges named by the
    // keys the plugin gave its sides. (One edge alone fails inside Arris,
    // `kernel/tests/recipes/hexagon-single-edge-fillet`.)
    add(
        c,
        i,
        record(
            i.fillet,
            "core.fillet",
            "Fillet",
            &[("radius", "0.3 mm")],
            &[("target", slot(i.standoff, "body"))],
        ),
    );
    c.submit(Command::EditFeature {
        feature: i.fillet,
        edit: FeatureEdit {
            inputs: [(
                "edges".into(),
                Some(
                    (1..=6)
                        .map(|k| Ref::Topo(i.standoff_edge(k)))
                        .collect::<Vec<_>>()
                        .into(),
                ),
            )]
            .into(),
            ..FeatureEdit::default()
        },
    });
}

/// Evaluated from fresh state: the outcomes and every kernel call.
fn run(c: &Client) -> (arrix_doc::Evaluation, Vec<KernelCall>) {
    let mut ev = Evaluator::new(registry());
    let e = ev.evaluate(c.replica.document());
    (e, ev.kernel_calls().to_vec())
}

#[test]
fn the_standoff_scenario_by_command() {
    let i = Ids::new();
    let mut c = Client::with_registry(i.author, registry());
    build(&mut c, &i);
    for f in [i.plate_sketch, i.plate, i.standoff, i.fillet] {
        assert!(
            matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
            "{f}: {:?}",
            c.outcome(f)
        );
    }

    // The standoff is a hexagonal prism, 8 mm across flats and 10 mm tall,
    // with a 3 mm hole and a rounded bottom ring: a little under the prism.
    let FeatureOutcome::Ok { slots, .. } = c.outcome(i.standoff) else {
        unreachable!()
    };
    let Some(SlotView::Body(m)) = slots.get(&SlotName::new("body").unwrap()) else {
        panic!("{slots:?}")
    };
    let prism = 3f64.sqrt() / 2.0 * 8.0 * 8.0 * 10.0 * MM * MM * MM;
    let hole = std::f64::consts::PI * 1.5 * 1.5 * 10.0 * MM * MM * MM;
    assert!(
        m.volume < prism - hole && m.volume > 0.99 * (prism - hole),
        "{m:?}"
    );

    // One snapshot evaluated twice, each time by a new plugin process: the
    // same outcomes, names and kernel calls, to the bit.
    let (first, calls) = run(&c);
    assert_eq!((first.clone(), calls.clone()), run(&c));
    assert!(calls.iter().any(|c| matches!(c.op, KernelOp::Cut { .. })));

    scenario_directory("standoff", &c.saves[c.replica.generation().0 as usize]);
}
