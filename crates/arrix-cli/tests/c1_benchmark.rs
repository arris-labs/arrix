//! C1's benchmark (docs/ROADMAP.md §C1, accept item 8): the budgets of
//! docs/CONCURRENCY-WASM.md §Budgets measured on the bracket, the
//! acceptance part `c1_accept.rs` saves as `tests/docs/bracket`. Each is a
//! median over runs, printed with `--nocapture`; a budget the reference
//! machine meets with room is asserted, so a regression past it fails.
//!
//! One test, so its sections never run beside each other; the other test
//! here writes no timing.

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use arrix_kernel::{CallIndex, FixtureRecipe, KernelOp};
use web_time::Instant;

use arrix_core::{FeatureId, Id, MeshDetail, ParamId};
use arrix_doc::{
    AuthorId, Authority, Command, CommandEnvelope, DocHash, Document, Evaluator, EventStream,
    Generation, LocalSession, MemorySource, Outcome, Replica, Request, Session, SessionEvent,
    expr::Expr, open, open_zip, save, save_zip,
};
use common::{read_tree, workspace};

const ME: AuthorId = AuthorId(Id(1));

fn bracket() -> Document {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/bracket"), "", &mut files);
    open(&MemorySource(files)).unwrap()
}

fn width(doc: &Document) -> ParamId {
    doc.param_by_name("w").expect("the bracket's width").0
}

fn feature(doc: &Document, name: &str) -> FeatureId {
    doc.parts()
        .values()
        .flat_map(|p| p.features.values())
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("the bracket's {name}"))
        .id
}

/// The width at run `i`: never the one before it, never 60 mm, and inside
/// the range the 50-point sweep evaluates clean.
fn set_width(w: ParamId, i: usize) -> Command {
    let mm = 60.0 + 0.1 * (i % 20 + 1) as f64;
    Command::SetParam {
        param: w,
        expr: Expr::parse(&format!("{mm} mm")).unwrap(),
    }
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Prints the median of `runs` and returns it.
fn report(what: &str, runs: Vec<Duration>) -> Duration {
    let n = runs.len();
    let (lo, hi) = (*runs.iter().min().unwrap(), *runs.iter().max().unwrap());
    let m = median(runs);
    println!(
        "{what}: median {:.3} ms (min {:.3}, max {:.3}, {n} runs)",
        ms(m),
        ms(lo),
        ms(hi)
    );
    m
}

/// A session on the bracket, at rest: generation 0 evaluated and every
/// body meshed refined. Returns its stream and its number of final bodies.
fn session_at_rest() -> (LocalSession, EventStream, usize) {
    let session = LocalSession::open(bracket(), common::registry()).unwrap();
    let events = session.subscribe();
    let bodies = rest(&events, Generation(0), None);
    (session, events, bodies)
}

/// Follows `events` until every final body of `g` is meshed refined, and
/// returns how many there were. With `bodies` given, that many.
fn rest(events: &EventStream, g: Generation, bodies: Option<usize>) -> usize {
    let (mut coarse, mut refined) = (0, 0);
    loop {
        let e = events.recv_timeout(Duration::from_secs(60)).unwrap();
        if let SessionEvent::Meshed {
            generation, detail, ..
        } = e
            && generation == g
        {
            match detail {
                MeshDetail::Coarse => coarse += 1,
                MeshDetail::Refined => refined += 1,
            }
            if refined == bodies.unwrap_or(coarse) {
                return refined;
            }
        }
    }
}

fn submit(session: &LocalSession, base: Generation, command: Command) -> Generation {
    let envelope = CommandEnvelope {
        author: ME,
        base,
        command,
    };
    match session.request(Request::Submit(envelope)) {
        Ok(Outcome::Applied(change)) => change.generation,
        other => panic!("the edit applies: {other:?}"),
    }
}

/// A width edit applied by the authority, and by a replica from the
/// change it hears (the document's hash checked, in a debug build, as
/// every replica checks it).
fn command_apply(runs: usize) -> (Duration, Duration) {
    let doc = bracket();
    let w = width(&doc);
    let mut authority = Authority::new(doc.clone());
    let mut replica = Replica::default();
    let files = save(&doc)
        .0
        .into_iter()
        .map(|(p, b)| (p, String::from_utf8(b).unwrap()))
        .collect();
    let opened = SessionEvent::Opened {
        generation: Generation(0),
        files,
        blobs: BTreeMap::new(),
        types: Vec::new(),
    };
    replica.receive(&opened).unwrap();
    let (mut a, mut r) = (Vec::new(), Vec::new());
    for i in 0..runs {
        let envelope = CommandEnvelope {
            author: ME,
            base: authority.generation(),
            command: set_width(w, i),
        };
        let t = Instant::now();
        let Ok(Outcome::Applied(change)) = authority.submit(&envelope) else {
            panic!("the edit applies")
        };
        let hash = cfg!(debug_assertions).then(|| DocHash::of(authority.document()));
        a.push(t.elapsed());
        let applied = SessionEvent::Applied {
            change: *change,
            hash,
        };
        let t = Instant::now();
        replica.receive(&applied).unwrap();
        r.push(t.elapsed());
    }
    assert_eq!(replica.document(), authority.document());
    (
        report("command apply, authority", a),
        report("command apply, replica", r),
    )
}

/// From a width edit's submit to the first coarse mesh of its generation,
/// the session at rest before each; the evaluation's share (to its
/// `Finished`) reported beside it, and each feature's in the slowest run.
fn first_coarse_mesh(runs: usize) -> Duration {
    let (session, events, bodies) = session_at_rest();
    let doc = bracket();
    let w = width(&doc);
    let (mut times, mut evaluated) = (Vec::new(), Vec::new());
    let mut features = Vec::new();
    for i in 0..runs {
        let t = Instant::now();
        let g = submit(&session, Generation(i as u64), set_width(w, i));
        let mut heard = Vec::new();
        loop {
            match events.recv_timeout(Duration::from_secs(60)).unwrap() {
                SessionEvent::Evaluated { generation, event } if generation == g => {
                    heard.push((event.feature, t.elapsed()));
                }
                SessionEvent::Finished { generation, .. } if generation == g => {
                    evaluated.push(t.elapsed());
                }
                SessionEvent::Meshed {
                    generation,
                    detail: MeshDetail::Coarse,
                    mesh,
                    ..
                } if generation == g => {
                    times.push(t.elapsed());
                    mesh.expect("the bracket meshes");
                    break;
                }
                _ => {}
            }
        }
        if times.last() == times.iter().max() {
            features = heard;
        }
        rest(&events, g, Some(bodies));
    }
    let m = report("first coarse mesh after a width edit", times);
    report("  of which the evaluation", evaluated);
    let mut since = Duration::ZERO;
    for (f, at) in features {
        let (_, _, record) = doc.feature(f).unwrap();
        println!("    {}: {:.1} ms", record.name, ms(at - since));
        since = at;
    }
    m
}

/// Saving the bracket as the session saves it (the gear's frozen result
/// included) to a directory and to an `.arrx`, and opening each again.
fn save_and_load(runs: usize) -> (Duration, Duration) {
    let (session, _events, _) = session_at_rest();
    let doc = open(&session.save()).unwrap();
    let dir = workspace().join("target/c1-benchmark");
    let (mut saves, mut loads, mut zip_saves, mut zip_loads) = (vec![], vec![], vec![], vec![]);
    for _ in 0..runs {
        let _ = std::fs::remove_dir_all(&dir);
        let t = Instant::now();
        for (path, bytes) in &save(&doc).0 {
            let to = dir.join(path);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::write(to, bytes).unwrap();
        }
        saves.push(t.elapsed());

        let t = Instant::now();
        let mut files = BTreeMap::new();
        read_tree(&dir, "", &mut files);
        let loaded = open(&MemorySource(files)).unwrap();
        loads.push(t.elapsed());
        assert_eq!(loaded, doc);

        let arrx = dir.with_extension("arrx");
        let t = Instant::now();
        std::fs::write(&arrx, save_zip(&doc).unwrap()).unwrap();
        zip_saves.push(t.elapsed());

        let t = Instant::now();
        let loaded = open_zip(&std::fs::read(&arrx).unwrap()).unwrap();
        zip_loads.push(t.elapsed());
        assert_eq!(loaded, doc);
    }
    let save = report("save to a directory", saves);
    let load = report("load from a directory", loads);
    let zip_save = report("save to an .arrx", zip_saves);
    let zip_load = report("load from an .arrx", zip_loads);
    (save.max(zip_save), load.max(zip_load))
}

/// From a newer width edit's submit, made while the evaluation of the one
/// before is inside the gear's join, to the first event of the newer
/// generation: the stop inside the kernel call, the restart, and the
/// evaluation of the first feature (the plate's sketch).
fn cancellation(runs: usize) -> Duration {
    let (session, events, bodies) = session_at_rest();
    let doc = bracket();
    let w = width(&doc);
    let before = feature(&doc, "Spur gear");
    let mut times = Vec::new();
    let mut base = Generation(0);
    for i in 0..runs {
        let g = submit(&session, base, set_width(w, 2 * i));
        loop {
            match events.recv_timeout(Duration::from_secs(60)).unwrap() {
                SessionEvent::Evaluated { generation, event }
                    if generation == g && event.feature == before =>
                {
                    break;
                }
                SessionEvent::Finished { generation, .. } if generation == g => {
                    panic!("generation {g:?} finished before it was superseded")
                }
                _ => {}
            }
        }
        let t = Instant::now();
        let newer = submit(&session, g, set_width(w, 2 * i + 1));
        loop {
            match events.recv_timeout(Duration::from_secs(60)).unwrap() {
                SessionEvent::Evaluated { generation, .. } if generation == newer => {
                    times.push(t.elapsed());
                    break;
                }
                SessionEvent::Finished { generation, .. } if generation == g => {
                    panic!("the superseded generation finished")
                }
                _ => {}
            }
        }
        rest(&events, newer, Some(bodies));
        base = newer;
    }
    report("cancellation, a newer edit inside the join", times)
}

/// `arrix eval tests/docs` on one thread and on four: the wall time of
/// the process, and documents per second.
fn eval_throughput(runs: usize) {
    for jobs in ["1", "4"] {
        let mut times = Vec::new();
        let mut docs = 0;
        for _ in 0..runs {
            let t = Instant::now();
            let out = std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
                .current_dir(workspace())
                .args(["eval", "--jobs", jobs, "tests/docs"])
                .output()
                .expect("run arrix");
            times.push(t.elapsed());
            assert_eq!(out.status.code(), Some(1), "the oversize fillet fails");
            docs = out.stdout.iter().filter(|&&b| b == b'\n').count();
        }
        let m = report(
            &format!("arrix eval --jobs {jobs} tests/docs ({docs} documents)"),
            times,
        );
        println!(
            "  {:.1} documents per second",
            docs as f64 / m.as_secs_f64()
        );
    }
}

/// The recipe of the call that makes most of a width edit's time: the
/// mass properties of the bracket's last body, which carries the gear's
/// 240 faces, about 90 ms in Arris alone (`docs/BACKLOG.md`). Written to
/// `crates/arrix-kernel/tests/recipes/` beside the failures' recipes, for
/// Arris to measure; rewritten under `UPDATE_SNAPSHOTS=1`.
#[test]
fn the_slow_measurement_is_a_recipe() {
    let mut evaluator = Evaluator::new(common::registry());
    evaluator.evaluate(&bracket());
    let records = evaluator.kernel_calls();
    let last = records
        .iter()
        .rposition(|r| matches!(r.op, KernelOp::MassProperties))
        .expect("the bracket is measured");
    let recipe = FixtureRecipe::measured(
        records,
        CallIndex(last as u32),
        "measured slow: about 90 ms for the mass properties of the bracket's last \
         body, three times its fuse with the gear (plans/c1-m4 step 16)",
    )
    .unwrap()
    .to_json();
    let path =
        workspace().join("crates/arrix-kernel/tests/recipes/bracket-mass-properties/fixture.json");
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

#[test]
fn c1_benchmark() {
    let profile = if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    };
    println!("C1 benchmark on the bracket, {profile} profile");
    let (authority, replica) = command_apply(200);
    let coarse = first_coarse_mesh(15);
    let (save, load) = save_and_load(30);
    let cancel = cancellation(15);
    eval_throughput(3);

    // The budgets met, each by five times or more on the reference
    // machine, asserted at the budget itself.
    let ms = Duration::from_millis;
    assert!(
        authority <= ms(1) && replica <= ms(1),
        "command apply ≤ 1 ms"
    );
    assert!(
        save <= ms(100) && load <= ms(100),
        "save and load ≤ 100 ms each"
    );
    assert!(cancel <= ms(50), "cancellation ≤ 50 ms");
    // Missed (docs/BACKLOG.md): Arris's mass properties of the bodies that
    // carry the gear take most of it. Reported, not asserted.
    let _ = coarse;
}
