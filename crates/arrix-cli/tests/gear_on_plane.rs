//! M1's first acceptance scenario (docs/ROADMAP.md §M1, the plugin
//! feature in the history): a document built by commands alone, through
//! `LocalSession` with the compiled-in `gears` registered, headless. It
//! also writes `tests/docs/gear-on-plane/`, the document `arrix eval`
//! prints a golden line for: with `UPDATE_SNAPSHOTS=1` it rewrites the
//! directory, otherwise the directory must equal what the commands save.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arrix_core::{
    CurveKey, DVec3, FeatureId, Id, IdMinter, NameRoot, ParamId, PartId, PersistentName,
    QuantityKind, Ref, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Document, Evaluator, EventStream, FeatureOutcome,
    FeatureRecord, FeatureTypeId, Generation, LocalSession, MemorySource, Outcome, Param, Part,
    Registry, Replica, Request, Session, SessionEvent, SlotView, expr::Expr, open, save,
};
use arrix_plugin_host::register_tier0;

const MM: f64 = 1e-3;

fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    r
}

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
        inputs: plane.into_iter().map(|r| ("plane".into(), r)).collect(),
        suppressed: false,
        sketch: None,
    }
}

/// A client of the session: its replica, every event it heard, the
/// requests it made, and the saved files at each generation.
struct Client {
    session: LocalSession,
    events: EventStream,
    replica: Replica,
    author: AuthorId,
    heard: Vec<SessionEvent>,
    requests: Vec<Request>,
    saves: Vec<MemorySource>,
}

impl Client {
    fn new(author: AuthorId) -> Self {
        let session = LocalSession::open(Document::default(), registry()).unwrap();
        let events = session.subscribe();
        let mut c = Client {
            session,
            events,
            replica: Replica::default(),
            author,
            heard: Vec::new(),
            requests: Vec::new(),
            saves: Vec::new(),
        };
        c.hear_until(|r| r.finished().is_some());
        c.saves.push(save(c.replica.document()));
        c
    }

    fn hear_until(&mut self, done: impl Fn(&Replica) -> bool) {
        while !done(&self.replica) {
            let e = self
                .events
                .recv_timeout(Duration::from_secs(60))
                .expect("the session publishes within the timeout");
            self.replica.receive(&e).unwrap();
            self.heard.push(e);
        }
    }

    /// Makes `request`, then follows the stream until its generation is
    /// evaluated.
    fn request(&mut self, request: Request) -> Generation {
        self.requests.push(request.clone());
        let Ok(Outcome::Applied(change)) = self.session.request(request) else {
            panic!("the request applies")
        };
        let g = change.generation;
        self.hear_until(|r| r.finished().is_some_and(|(f, _)| f >= g));
        g
    }

    fn submit(&mut self, command: Command) -> Generation {
        let g = self.request(Request::Submit(CommandEnvelope {
            author: self.author,
            base: self.replica.generation(),
            command,
        }));
        self.saves.push(save(self.replica.document()));
        assert_eq!(self.saves.len() as u64, g.0 + 1);
        g
    }

    fn set(&mut self, param: ParamId, expr: &str) -> Generation {
        let expr = Expr::parse(expr).unwrap();
        self.submit(Command::SetParam { param, expr })
    }

    fn outcome(&self, feature: FeatureId) -> &FeatureOutcome {
        let (g, o) = self.replica.outcome(feature).expect("evaluated");
        assert_eq!(g, self.replica.generation(), "of the latest generation");
        o
    }

    fn cached(&self, feature: FeatureId) -> bool {
        match self.outcome(feature) {
            FeatureOutcome::Ok { cached, .. } => *cached,
            other => panic!("feature {feature}: {other:?}"),
        }
    }

    fn plane(&self, feature: FeatureId) -> arrix_core::Frame {
        match self.outcome(feature) {
            FeatureOutcome::Ok { slots, .. } => match slots.values().next() {
                Some(SlotView::Plane(f)) => *f,
                other => panic!("feature {feature}: {other:?}"),
            },
            other => panic!("feature {feature}: {other:?}"),
        }
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

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir`, by `/`-separated path relative to it.
fn read_tree(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type().unwrap().is_dir() {
            read_tree(&entry.path(), &format!("{name}/"), out);
        } else {
            out.insert(name, std::fs::read(entry.path()).unwrap());
        }
    }
}

/// `tests/docs/gear-on-plane/` holds exactly `files`, or is rewritten to.
fn scenario_directory(files: &MemorySource) {
    let dir = workspace().join("tests/docs/gear-on-plane");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        let _ = std::fs::remove_dir_all(&dir);
        for (path, bytes) in &files.0 {
            let to = dir.join(path);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::write(to, bytes).unwrap();
        }
    }
    let mut on_disk = BTreeMap::new();
    read_tree(&dir, "", &mut on_disk);
    assert!(
        on_disk == files.0,
        "tests/docs/gear-on-plane differs from what the commands save; \
         rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
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

    scenario_directory(&scenario);
}
