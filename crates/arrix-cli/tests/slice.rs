//! M1's accept scenario, the live half (docs/ROADMAP.md §M1), headless and
//! by commands alone through `LocalSession` with `gears` compiled in: a
//! sketched plate extruded, a datum plane on its top face, a spur gear on
//! that plane fused to the plate, and a pocket sketched on the joined top
//! face (kept by its name, ADR-0010) and cut. It also writes `tests/docs/slice/`, the
//! document `arrix eval` prints a golden line for.

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    DVec3, FeatureId, IdMinter, NameRoot, ParamId, PartId, PersistentName, QuantityKind, Ref,
    RegionKey, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, Document, FeatureOutcome, FeatureRecord, FeatureTypeId, Param, Part,
    Registry, Request, Session, SketchView, SlotView, apply, expr::Expr, open, save,
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
                index: 0,
                feature: self.plate,
                part: SweepPartName::EndCap,
            },
            chain: vec![],
        }
    }

    fn joined_top(&self) -> PersistentName {
        format!("face:sweep.{}.end-cap", self.plate)
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
    r.inputs
        .extend(plane.map(|p| ("plane".to_string(), p.into())));
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
        &[("target", slot(i.plate, "body"))],
    );
    join.type_version = 2;
    join.inputs
        .insert("tools".into(), vec![slot(i.gear, "body")].into());
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
    pocket
        .inputs
        .insert("target".into(), slot(i.plate, "body").into());
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

/// The files of a save that are blobs, by path.
fn blobs(files: &arrix_doc::MemorySource) -> BTreeMap<&String, &Vec<u8>> {
    files
        .0
        .iter()
        .filter(|(p, _)| p.starts_with("blobs/"))
        .collect()
}

#[test]
fn a_saved_slice_carries_the_gears_frozen_result() {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build(&mut c, &i);
    let saved = c.session.save();

    // The document's own save has none: a frozen result is not authored.
    assert!(blobs(&c.saves[c.replica.generation().0 as usize]).is_empty());
    let doc: Document = open(&saved).unwrap();
    let (_, _, gear) = doc.feature(i.gear).unwrap();
    let frozen = gear.frozen.as_ref().expect("the plugin feature is frozen");
    assert_eq!(frozen.plugin_version.to_string(), "0.1.0");
    let body = &frozen.slots[&SlotName::new("body").unwrap()];
    assert!(body.as_str().ends_with(".arrisbody"));
    assert!(doc.blob(body).unwrap().starts_with(b"ARRISBDY"));
    // The name table is JSON of slot to names; the body's are all there.
    let table: BTreeMap<String, Vec<serde_json::Value>> =
        serde_json::from_slice(doc.blob(&frozen.names).unwrap()).unwrap();
    assert!(table["body"].len() > 100, "{}", table["body"].len());
    // Core features carry none, and the save holds those blobs and no others.
    for core in [i.plate_sketch, i.plate, i.base, i.join, i.pocket] {
        assert!(doc.feature(core).unwrap().2.frozen.is_none());
    }
    assert_eq!(blobs(&saved).len(), 2);

    // Saved twice, identical; and it round-trips through open and save.
    assert_eq!(saved.0, c.session.save().0);
    assert_eq!(save(&doc).0, saved.0);
}

#[test]
fn the_gear_evaluated_twice_from_fresh_state_gives_identical_blobs() {
    let i = Ids::new();
    let (mut first, mut second) = (Client::new(i.author), Client::new(i.author));
    build(&mut first, &i);
    build(&mut second, &i);
    let (a, b) = (first.session.save(), second.session.save());
    assert_eq!(blobs(&a).len(), 2);
    assert_eq!(a.0, b.0);
}

#[test]
fn undo_re_evaluated_and_saved_restores_each_earlier_save() {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build(&mut c, &i);
    c.set(i.teeth, "13");
    let last = c.replica.generation().0;
    assert_ne!(
        blobs(&c.frozen_saves[last as usize]),
        blobs(&c.frozen_saves[last as usize - 1]),
        "the tooth count changes the gear's blob"
    );
    for back in (0..last).rev() {
        c.request(Request::Undo { author: i.author });
        assert!(
            c.session.save().0 == c.frozen_saves[back as usize].0,
            "undone to generation {back}"
        );
    }
}

/// The saved slice holds the gear's result as a blob, a file that is not
/// JSON: a session over it still opens a subscription, whose `Opened` carries
/// the blob, and a replica rebuilt from it saves to the very same files.
#[test]
fn a_subscriber_to_a_saved_slice_is_given_its_blobs() {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build(&mut c, &i);
    let saved = c.session.save();
    assert_eq!(blobs(&saved).len(), 2);
    let session = arrix_doc::LocalSession::open(open(&saved).unwrap(), common::registry())
        .expect("a session starts");
    let first = session.subscribe().recv().expect("Opened comes first");
    let mut replica = arrix_doc::Replica::default();
    replica.receive(&first).expect("a replica follows Opened");
    assert_eq!(save(replica.document()).0, saved.0);
}

/// A body line's measures of `feature`'s body slot.
fn body_line(line: &arrix_doc::EvalLine, feature: FeatureId) -> (f64, usize) {
    let b = line
        .bodies
        .iter()
        .find(|b| b.feature == feature)
        .unwrap_or_else(|| panic!("no body line for {feature}"));
    (b.volume, b.faces)
}

fn status(line: &arrix_doc::EvalLine, feature: FeatureId) -> &str {
    &line
        .features
        .iter()
        .find(|f| f.id == feature)
        .unwrap()
        .status
}

#[test]
fn the_saved_slice_evaluates_without_the_plugin_to_the_same_volumes() {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build(&mut c, &i);
    let saved = c.session.save();
    let with = arrix_doc::eval("with", &saved, &common::registry()).unwrap();
    let core = Registry::with_core_types();
    let without = arrix_doc::eval("without", &saved, &core).unwrap();

    // Criterion 4: the gear frozen, missing; the pocket still cut.
    assert_eq!(status(&with, i.gear), "ok");
    assert_eq!(status(&without, i.gear), "frozen");
    let gear = without.features.iter().find(|f| f.id == i.gear).unwrap();
    assert_eq!(gear.category.as_deref(), Some("plugin.frozen.missing"));
    assert_eq!(without.status, arrix_doc::EvalStatus::Ok);
    for f in [i.gear, i.join, i.pocket] {
        assert_eq!(body_line(&without, f), body_line(&with, f), "{f}");
    }

    // Criterion 5: an input of the gear edited without the plugin, as a
    // file: the plate's thickness moves the plane the gear stands on (the
    // width does not, the plane being the same), so the gear is stale and
    // the pocket re-evaluated against its stored result.
    let doc: Document = open(&saved).unwrap();
    let command = Command::SetParam {
        param: i.t,
        expr: Expr::parse("6 mm").unwrap(),
    };
    let doc = apply(&doc, &command).unwrap().document;
    let edit = arrix_doc::Evaluator::new(core.clone())
        .evaluate(&doc)
        .line("edited", &doc);
    assert_eq!(status(&edit, i.gear), "frozen-stale");
    assert_eq!(status(&edit, i.pocket), "ok");
    assert!(body_line(&edit, i.pocket).0 > body_line(&without, i.pocket).0);
    // Criterion 6: with the plugin back, the gear is live again, frozen
    // anew, and nothing is stale.
    let mut evaluator = arrix_doc::Evaluator::new(common::registry());
    let back = evaluator.evaluate(&doc).line("back", &doc);
    assert_eq!(status(&back, i.gear), "ok");
    assert!(evaluator.frozen().contains_key(&i.gear));
    assert!(back.features.iter().all(|f| {
        f.diagnostics
            .iter()
            .all(|d| d.code.as_str() != "plugin.frozen.stale")
    }));
    assert!(
        edit.features
            .iter()
            .find(|f| f.id == i.pocket)
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "plugin.frozen.stale")
    );
}

#[test]
fn the_saved_slice_zips_deterministically_with_its_blobs() {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build(&mut c, &i);
    let doc: Document = open(&c.session.save()).unwrap();
    // Criterion 3, the zip form: saved twice, identical, and the directory
    // form is what unzipping gives.
    let zip = arrix_doc::save_zip(&doc).unwrap();
    assert_eq!(zip, arrix_doc::save_zip(&doc).unwrap());
    let reopened = arrix_doc::open_zip(&zip).unwrap();
    assert_eq!(arrix_doc::save_zip(&reopened).unwrap(), zip);
    assert_eq!(arrix_doc::from_zip(&zip).unwrap().0, save(&doc).0);
    assert_eq!(blobs(&save(&reopened)).len(), 2);
}

/// `arrix eval` of `files` written to a scratch directory, as `name`
/// relative to the scratch space (so the line's `doc` is machine-free),
/// with `args` before it. Returns the exit code and stdout.
fn arrix_eval(name: &str, files: &arrix_doc::MemorySource, args: &[&str]) -> (Option<i32>, String) {
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("slice-frozen");
    let dir = scratch.join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in &files.0 {
        let to = dir.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, bytes).unwrap();
    }
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(&scratch)
        .arg("eval")
        .args(args)
        .arg(name)
        .output()
        .expect("run arrix");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), String::from_utf8(out.stdout).unwrap())
}

/// `stdout` is the golden's, or the golden is rewritten to it. The frozen
/// bytes themselves are never committed (ADR-0001): a golden is the line
/// `arrix eval` prints for them.
fn golden(file: &str, stdout: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(file);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, stdout).unwrap();
    }
    let want = std::fs::read_to_string(&path).expect("read the golden");
    assert_eq!(
        stdout, want,
        "{file}: rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
}

#[test]
fn arrix_eval_of_the_frozen_slice_equals_its_goldens() {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build(&mut c, &i);
    let saved = c.session.save();

    let (code, live) = arrix_eval("slice-live", &saved, &[]);
    assert_eq!(code, Some(0));
    golden("eval_slice_frozen_live.jsonl", &live);

    // Criterion 4 through the binary.
    let (code, without) = arrix_eval("slice-without", &saved, &["--without-plugin", "gears"]);
    assert_eq!(code, Some(0));
    golden("eval_slice_without_gears.jsonl", &without);

    // Criterion 5 through the binary: the thickness edited in the file.
    let doc: Document = open(&saved).unwrap();
    let command = Command::SetParam {
        param: i.t,
        expr: Expr::parse("6 mm").unwrap(),
    };
    let edited = save(&apply(&doc, &command).unwrap().document);
    let (code, stale) = arrix_eval("slice-edited", &edited, &["--without-plugin", "gears"]);
    assert_eq!(code, Some(0));
    golden("eval_slice_edited_without_gears.jsonl", &stale);
    assert!(stale.contains(r#""status":"frozen-stale""#), "{stale}");
}
