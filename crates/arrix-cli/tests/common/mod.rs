//! What the scenario tests share: the registry with `gears` compiled in, a
//! client of `LocalSession` that records what it heard and saved, and the
//! check of a scenario directory in `tests/docs/`.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arrix_core::FeatureId;
use arrix_core::ParamId;
use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Document, EventStream, FeatureOutcome, Generation,
    LocalSession, MemorySource, Outcome, Registry, Replica, Request, Session, SessionEvent,
    SlotView, expr::Expr, save,
};
use arrix_plugin_host::register_tier0;

const MM: f64 = 1e-3;

pub fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    r
}

/// A client of the session: its replica, every event it heard, the
/// requests it made, and the saved files at each generation.
pub struct Client {
    pub session: LocalSession,
    events: EventStream,
    pub replica: Replica,
    pub author: AuthorId,
    pub heard: Vec<SessionEvent>,
    pub requests: Vec<Request>,
    pub saves: Vec<MemorySource>,
}

impl Client {
    pub fn new(author: AuthorId) -> Self {
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

    pub fn hear_until(&mut self, done: impl Fn(&Replica) -> bool) {
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
    pub fn request(&mut self, request: Request) -> Generation {
        self.requests.push(request.clone());
        let outcome = self.session.request(request.clone());
        let Ok(Outcome::Applied(change)) = outcome else {
            panic!("the request applies: {request:?}: {outcome:?}")
        };
        let g = change.generation;
        self.hear_until(|r| r.finished().is_some_and(|(f, _)| f >= g));
        g
    }

    pub fn submit(&mut self, command: Command) -> Generation {
        let g = self.request(Request::Submit(CommandEnvelope {
            author: self.author,
            base: self.replica.generation(),
            command,
        }));
        self.saves.push(save(self.replica.document()));
        assert_eq!(self.saves.len() as u64, g.0 + 1);
        g
    }

    pub fn set(&mut self, param: ParamId, expr: &str) -> Generation {
        let expr = Expr::parse(expr).unwrap();
        self.submit(Command::SetParam { param, expr })
    }

    pub fn outcome(&self, feature: FeatureId) -> &FeatureOutcome {
        let (g, o) = self.replica.outcome(feature).expect("evaluated");
        assert_eq!(g, self.replica.generation(), "of the latest generation");
        o
    }

    pub fn cached(&self, feature: FeatureId) -> bool {
        match self.outcome(feature) {
            FeatureOutcome::Ok { cached, .. } => *cached,
            other => panic!("feature {feature}: {other:?}"),
        }
    }

    pub fn plane(&self, feature: FeatureId) -> arrix_core::Frame {
        match self.outcome(feature) {
            FeatureOutcome::Ok { slots, .. } => match slots.values().next() {
                Some(SlotView::Plane(f)) => *f,
                other => panic!("feature {feature}: {other:?}"),
            },
            other => panic!("feature {feature}: {other:?}"),
        }
    }
}

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir`, by `/`-separated path relative to it.
pub fn read_tree(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
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

/// `tests/docs/<name>/` holds exactly `files`, or is rewritten to.
pub fn scenario_directory(name: &str, files: &MemorySource) {
    let dir = workspace().join("tests/docs").join(name);
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
        "tests/docs/{name} differs from what the commands save; \
         rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
}
