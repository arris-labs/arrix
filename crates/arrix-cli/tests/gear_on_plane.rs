//! M1's first acceptance scenario (docs/ROADMAP.md §M1, the plugin
//! feature in the history): a document built by commands alone, through
//! `LocalSession` with the compiled-in `gears` registered, headless. It
//! also writes `tests/docs/gear-on-plane/`, the document `arrix eval`
//! prints a golden line for: with `UPDATE_SNAPSHOTS=1` it rewrites the
//! directory, otherwise the directory must equal what the commands save.

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, DVec3, FeatureId, Id, IdMinter, NameRoot, ParamId, PartId, PersistentName,
    QuantityKind, Ref, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, Evaluator, FeatureOutcome, FeatureRecord, FeatureTypeId, Param, Part,
    Request, SessionEvent, expr::Expr, open, save,
};
mod common;
use common::{Client, registry, scenario_directory};

const MM: f64 = 1e-3;

/// Every id the scenario's commands create, from the test's fixed seed.
struct Ids {
    author: AuthorId,
    teeth: ParamId,
    m: ParamId,
    w: ParamId,
    part: PartId,
    base: FeatureId,
    gear: FeatureId,
    top: FeatureId,
    flank: FeatureId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(2026);
        Ids {
            author: AuthorId(m.next_id()),
            teeth: m.mint(),
            m: m.mint(),
            w: m.mint(),
            part: m.mint(),
            base: m.mint(),
            gear: m.mint(),
            top: m.mint(),
            flank: m.mint(),
        }
    }

    fn gear_face(&self, part: SweepPartName) -> PersistentName {
        PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Sweep {
                index: 0,
                feature: self.gear,
                part,
            },
            chain: vec![],
        }
    }

    /// Tooth 20's rising radial flank: planar, below the base circle.
    fn tooth_20_flank(&self) -> PersistentName {
        self.gear_face(SweepPartName::Side(CurveKey(Id(20_001))))
    }
}

fn record(
    id: FeatureId,
    name: &str,
    ty: &str,
    params: &[(&str, &str)],
    plane: Option<Ref>,
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
        inputs: plane
            .into_iter()
            .map(|r| ("plane".into(), r.into()))
            .collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

/// Criterion 1: parameters, a datum plane, the gear on it, and a datum
/// plane on the gear's top face by persistent name.
fn build(c: &mut Client, i: &Ids) {
    let params = [
        (i.teeth, "teeth", QuantityKind::Count, "20"),
        (i.m, "m", QuantityKind::Length, "1 mm"),
        (i.w, "w", QuantityKind::Length, "8 mm"),
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
        name: "Gear on a plane".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
    };
    c.submit(Command::AddPart { part });
    let on_base = Ref::Slot {
        feature: i.base,
        slot: SlotName::new("plane").unwrap(),
    };
    let top = Ref::Topo(i.gear_face(SweepPartName::EndCap));
    let features = [
        record(
            i.base,
            "Base plane",
            "core.datum-plane",
            &[("offset", "10 mm")],
            None,
        ),
        record(
            i.gear,
            "Spur gear",
            "gears.spur",
            &[("teeth", "teeth"), ("module", "m"), ("width", "w")],
            Some(on_base),
        ),
        record(
            i.top,
            "Top plane",
            "core.datum-plane",
            &[("offset", "2 mm")],
            Some(top),
        ),
    ];
    for (at, record) in features.into_iter().enumerate() {
        c.submit(Command::AddFeature {
            part: i.part,
            at,
            record,
        });
    }
}

#[test]
fn gear_on_plane() {
    let i = Ids::new();
    let mut c = Client::new(i.author);

    // 1. Built by commands alone.
    build(&mut c, &i);
    let built = c.replica.generation();
    let scenario = c.saves[built.0 as usize].clone();
    let top = c.plane(i.top);
    assert!((top.origin().z - 20.0 * MM).abs() < 1e-12, "{top:?}");
    assert!((top.z_axis() - DVec3::Z).length() < 1e-12);

    // 2. Edits re-evaluate what depends on them; names hold or are lost.
    c.set(i.w, "12 mm");
    assert!(
        c.cached(i.base),
        "the lower plane reads nothing that changed"
    );
    assert!(!c.cached(i.gear) && !c.cached(i.top));
    let moved = c.plane(i.top).origin() - top.origin();
    assert!((moved - DVec3::new(0.0, 0.0, 4.0 * MM)).length() < 1e-12);
    c.set(i.teeth, "24");
    assert!(!c.cached(i.gear));
    assert!((c.plane(i.top).origin().z - 24.0 * MM).abs() < 1e-12);
    c.submit(Command::AddFeature {
        part: i.part,
        at: 3,
        record: record(
            i.flank,
            "Flank plane",
            "core.datum-plane",
            &[("offset", "1 mm")],
            Some(Ref::Topo(i.tooth_20_flank())),
        ),
    });
    c.plane(i.flank);
    c.set(i.teeth, "18");
    let FeatureOutcome::Failed { diagnostic, .. } = c.outcome(i.flank) else {
        panic!("{:?}", c.outcome(i.flank))
    };
    assert_eq!(diagnostic.code.as_str(), "ref.lost");
    assert_eq!(diagnostic.refs, [Ref::Topo(i.tooth_20_flank())]);
    assert!(!diagnostic.candidates.is_empty());
    for f in [i.base, i.gear, i.top] {
        assert!(matches!(c.outcome(f), FeatureOutcome::Ok { .. }));
    }

    // 3. One snapshot, evaluated twice from fresh state: the same names
    // (the lost reference's candidates, the frames resolved by name, the
    // records' curve keys) and the same volumes, to the bit.
    let run = || {
        let mut ev = Evaluator::new(registry());
        let e = ev.evaluate(c.replica.document());
        (e, ev.kernel_calls().to_vec())
    };
    let (first, calls) = run();
    assert_eq!((first.clone(), calls.clone()), run());
    assert!(!calls.is_empty());
    let line = first.line("gear-on-plane", c.replica.document());
    assert_eq!(line.bodies.len(), 1);

    // 4. Saves are deterministic and round-trip; undo and redo walk the
    // saved bytes back and forth.
    let last = c.replica.generation();
    assert_eq!(save(c.replica.document()).0, c.saves[last.0 as usize].0);
    let reopened = open(&c.saves[last.0 as usize]).unwrap();
    assert_eq!(save(&reopened).0, c.saves[last.0 as usize].0);
    for back in (0..last.0).rev() {
        c.request(Request::Undo { author: i.author });
        let now = save(c.replica.document());
        assert!(
            now.0 == c.saves[back as usize].0,
            "undone to generation {back}"
        );
    }
    for forward in 1..=last.0 {
        c.request(Request::Redo { author: i.author });
        let now = save(c.replica.document());
        assert!(
            now.0 == c.saves[forward as usize].0,
            "redone to generation {forward}"
        );
    }

    // 5. Every request and event crosses as JSON and back.
    for r in &c.requests {
        let json = serde_json::to_string(r).unwrap();
        assert_eq!(
            &serde_json::from_str::<Request>(&json).unwrap(),
            r,
            "{json}"
        );
    }
    for e in &c.heard {
        let json = serde_json::to_string(e).unwrap();
        assert_eq!(
            &serde_json::from_str::<SessionEvent>(&json).unwrap(),
            e,
            "{json}"
        );
    }
    let applied = c
        .heard
        .iter()
        .filter(|e| matches!(e, SessionEvent::Applied { .. }))
        .count() as u64;
    assert_eq!(applied, 3 * last.0, "built, undone and redone");

    scenario_directory("gear-on-plane", &scenario);
}

/// The body native builds for the gear's extrude, by its hash; wasm32
/// builds `87b72b8d…` from the same call (plans/c4 step 2).
const NATIVE_GEAR_BODY: &str = "9e7347d226c4ef05f9c6b4fa8cf61f18d4adb0820fcbb5a40ad4c619a7f8fc05";

/// The first kernel call whose result differs on wasm32 (plans/c4 step 2,
/// `arrix-plugin-host`'s `tests/wasm_determinism.rs`): the extrude of the
/// gear's profile, whose arcs are three-point arcs. Its inputs are the
/// same bits on both targets, since `gears` computes them with
/// `arrix_plugin_api::math` (ADR-0036); the bodies it makes are not.
/// Written to `crates/arrix-kernel/tests/recipes/` for Arris, rewritten
/// under `UPDATE_SNAPSHOTS=1`.
#[test]
fn the_gear_extrude_wasm32_builds_differently_is_a_recipe() {
    use arrix_kernel::{CallIndex, FixtureRecipe, KernelOp};
    let mut files = BTreeMap::new();
    common::read_tree(
        &common::workspace().join("tests/docs/gear-on-plane"),
        "",
        &mut files,
    );
    let mut evaluator = Evaluator::new(registry());
    evaluator.evaluate(&open(&arrix_doc::MemorySource(files)).unwrap());
    let records = evaluator.kernel_calls();
    assert!(
        matches!(records[0].op, KernelOp::Extrude { .. }),
        "{:?}",
        records[0].op
    );
    assert_eq!(
        records[1].operands[0].as_str(),
        NATIVE_GEAR_BODY,
        "the body native builds changed: rerun the wasm32 probe"
    );
    let recipe = FixtureRecipe::measured(
        records,
        CallIndex(0),
        "kept because its body is not the same on wasm32 as natively: the \
         gear's extrude in tests/docs/gear-on-plane, a profile of lines and \
         three-point arcs given the same bits on both targets, hashes (BLAKE3 \
         of its encoded bytes) to 9e7347d2… on x86_64 Linux and 87b72b8d… on \
         wasm32-unknown-unknown under Node, and its volume is \
         2.452232294324476e-6 natively and 2.4522322943244763e-6 on wasm32 \
         (plans/c4 step 2)",
    )
    .unwrap()
    .to_json();
    let path = common::workspace()
        .join("crates/arrix-kernel/tests/recipes/gear-extrude-on-wasm32/fixture.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &recipe).unwrap();
    }
    assert!(
        std::fs::read_to_string(&path).is_ok_and(|on_disk| on_disk == recipe),
        "{} differs; rerun with UPDATE_SNAPSHOTS=1 and read the diff",
        path.display()
    );
}
