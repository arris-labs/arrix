//! The wire spike (docs/plans/boundary-m1.md step 2, ADR-0022): what the
//! Tier 2 wire and `arrix serve` would cost in JSON-RPC, against a binary
//! encoding. Not run by `cargo test` (`test = false`); asked for by name:
//!
//! ```sh
//! cargo test --release -p arrix-cli --test wire_spike -- --nocapture
//! ```
//!
//! Two measurements, each a median printed:
//!
//! 1. **The bracket's meshes**, every `meshed` event of its first
//!    evaluation, coarse and refined, in three encodings: JSON as the
//!    session's serde types give it (numbers as text), JSON with the bulk
//!    arrays as base64 of their little-endian elements, and CBOR. Size,
//!    encode and decode in Rust, and `json.loads` (plus the base64 decode)
//!    in a Python child, since a Python agent reads `serve`'s events.
//! 2. **Evaluate round trips** over a real pipe to a Python child, as a
//!    Tier 2 feature makes them: a `feature.evaluate` request, ten
//!    `kernel.extrude` callbacks from the child, each answered with a
//!    handle, then the result. 1,000 of them.
//!
//! The numbers are recorded in ADR-0022; the kernel's own times are
//! `--test c1_benchmark`'s.

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use web_time::Instant;

use arrix_core::{DVec3, MeshDetail, MeshPoint, MeshRange, RenderMesh};
use arrix_doc::{Generation, LocalSession, MemorySource, Session, SessionEvent, open};
use common::{read_tree, workspace};

const RUNS: usize = 20;
const ROUND_TRIPS: usize = 1000;
const CALLBACKS: usize = 10;

/// The child: answers `spike.mesh` with how long its line took to parse
/// (and, in the base64 form, to decode), and `feature.evaluate` with
/// `CALLBACKS` kernel calls back to the parent before its result.
const CHILD: &str = r#"
import base64, json, sys, time
from array import array

out = sys.stdout
def send(obj):
    out.write(json.dumps(obj, separators=(",", ":")))
    out.write("\n")
    out.flush()

PROFILE = {"plane": {"origin": {"x": 0, "y": 0, "z": 0}, "x-axis": {"x": 1, "y": 0, "z": 0},
           "z-axis": {"x": 0, "y": 0, "z": 1}},
           "outer": {"path": {"start": {"x": 0, "y": 0}, "segments": [
               {"line": {"key": k, "to": {"x": 0.001 * k, "y": 0.002 * k}}} for k in range(12)]}},
           "holes": []}

for line in sys.stdin:
    t = time.perf_counter()
    msg = json.loads(line)
    method = msg.get("method")
    if method == "spike.mesh":
        mesh = msg["params"]["mesh"]
        if isinstance(mesh.get("positions"), str):
            for key in ("positions", "normals", "edge_points"):
                a = array("f"); a.frombytes(base64.b64decode(mesh[key]))
            a = array("I"); a.frombytes(base64.b64decode(mesh["triangles"]))
        send({"jsonrpc": "2.0", "id": msg["id"], "result": time.perf_counter() - t})
    elif method == "feature.evaluate":
        handles = []
        for k in range(CALLBACKS):
            send({"jsonrpc": "2.0", "id": "k%d" % k, "method": "kernel.extrude",
                  "params": {"p": PROFILE, "distance": 0.01}})
            handles.append(json.loads(sys.stdin.readline())["result"])
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {"slots": [
            {"name": "body", "value": {"body": handles[-1]}}], "tool": None}})
    elif method == "exit":
        break
"#;

struct Child2 {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    line: String,
}

impl Child2 {
    fn spawn() -> Self {
        let script = CHILD.replace("CALLBACKS", &CALLBACKS.to_string());
        let mut child = Command::new("python3")
            .args(["-I", "-c", &script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("python3 runs (the gate's lints need it too)");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Child2 {
            child,
            stdin,
            stdout,
            line: String::new(),
        }
    }

    fn send(&mut self, line: &str) {
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    fn recv(&mut self) -> Value {
        self.line.clear();
        self.stdout.read_line(&mut self.line).unwrap();
        serde_json::from_str(&self.line).unwrap()
    }
}

impl Drop for Child2 {
    fn drop(&mut self) {
        self.send(r#"{"jsonrpc":"2.0","method":"exit"}"#);
        let _ = self.child.wait();
    }
}

/// A mesh with its bulk arrays as base64 of their little-endian elements.
#[derive(Serialize, Deserialize)]
struct BulkMesh {
    origin: DVec3,
    chord: f64,
    positions: String,
    normals: String,
    triangles: String,
    faces: Vec<MeshRange>,
    edge_points: String,
    edges: Vec<MeshRange>,
    vertices: Vec<MeshPoint>,
}

fn f32s(v: &[[f32; 3]]) -> String {
    B64.encode(
        v.iter()
            .flatten()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<_>>(),
    )
}

fn u32s(v: &[[u32; 3]]) -> String {
    B64.encode(
        v.iter()
            .flatten()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<_>>(),
    )
}

fn from_f32s(s: &str) -> Vec<[f32; 3]> {
    let b = B64.decode(s).unwrap();
    b.chunks_exact(12)
        .map(|c| {
            let f = |i: usize| f32::from_le_bytes(c[i..i + 4].try_into().unwrap());
            [f(0), f(4), f(8)]
        })
        .collect()
}

fn from_u32s(s: &str) -> Vec<[u32; 3]> {
    let b = B64.decode(s).unwrap();
    b.chunks_exact(12)
        .map(|c| {
            let u = |i: usize| u32::from_le_bytes(c[i..i + 4].try_into().unwrap());
            [u(0), u(4), u(8)]
        })
        .collect()
}

impl BulkMesh {
    fn of(m: &RenderMesh) -> Self {
        BulkMesh {
            origin: m.origin,
            chord: m.chord,
            positions: f32s(&m.positions),
            normals: f32s(&m.normals),
            triangles: u32s(&m.triangles),
            faces: m.faces.clone(),
            edge_points: f32s(&m.edge_points),
            edges: m.edges.clone(),
            vertices: m.vertices.clone(),
        }
    }

    fn mesh(self) -> RenderMesh {
        RenderMesh {
            origin: self.origin,
            chord: self.chord,
            positions: from_f32s(&self.positions),
            normals: from_f32s(&self.normals),
            triangles: from_u32s(&self.triangles),
            faces: self.faces,
            edge_points: from_f32s(&self.edge_points),
            edges: self.edges,
            vertices: self.vertices,
        }
    }
}

/// Every mesh of the bracket's first evaluation, coarse and refined.
fn bracket_meshes() -> Vec<(MeshDetail, RenderMesh)> {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/bracket"), "", &mut files);
    let doc = open(&MemorySource(files)).unwrap();
    let session = LocalSession::open(doc, common::registry()).unwrap();
    let events = session.subscribe();
    let (mut meshes, mut coarse, mut refined) = (Vec::new(), 0, 0);
    loop {
        let e = events.recv_timeout(Duration::from_secs(60)).unwrap();
        if let SessionEvent::Meshed {
            generation: Generation(0),
            detail,
            mesh,
            ..
        } = e
        {
            match detail {
                MeshDetail::Coarse => coarse += 1,
                MeshDetail::Refined => refined += 1,
            }
            meshes.push((detail, mesh.expect("the bracket meshes")));
            if refined == coarse {
                return meshes;
            }
        }
    }
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn time<T>(mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut runs = Vec::new();
    let mut last = None;
    for _ in 0..RUNS {
        let t = Instant::now();
        let v = f();
        runs.push(t.elapsed());
        last = Some(v);
    }
    (median(runs), last.unwrap())
}

/// The child's median parse time of `line`, a `spike.mesh` request.
fn python_parse(child: &mut Child2, line: &str) -> Duration {
    let runs = (0..RUNS)
        .map(|_| {
            child.send(line);
            Duration::from_secs_f64(child.recv()["result"].as_f64().unwrap())
        })
        .collect();
    median(runs)
}

fn meshes(child: &mut Child2) {
    let all = bracket_meshes();
    let tris: usize = all.iter().map(|(_, m)| m.triangles.len()).sum();
    let points: usize = all.iter().map(|(_, m)| m.positions.len()).sum();
    println!(
        "bracket meshes: {} (coarse and refined), {tris} triangles, {points} positions",
        all.len()
    );
    let request = |id: usize, mesh: Value| json!({"jsonrpc": "2.0", "id": id, "method": "spike.mesh", "params": {"mesh": mesh}});
    for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
        let set: Vec<&RenderMesh> = all
            .iter()
            .filter(|(d, _)| *d == detail)
            .map(|(_, m)| m)
            .collect();
        println!("-- {detail:?}, {} meshes", set.len());

        let (enc, text) = time(|| {
            set.iter()
                .map(|m| serde_json::to_string(m).unwrap())
                .collect::<Vec<_>>()
        });
        let (dec, _) = time(|| {
            text.iter()
                .map(|t| serde_json::from_str::<RenderMesh>(t).unwrap())
                .collect::<Vec<_>>()
        });
        let lines: Vec<String> = set
            .iter()
            .enumerate()
            .map(|(i, m)| request(i, serde_json::to_value(m).unwrap()).to_string())
            .collect();
        let py: Duration = lines.iter().map(|l| python_parse(child, l)).sum();
        let size: usize = text.iter().map(String::len).sum();
        println!(
            "json:        {:>9} bytes, encode {:.3} ms, decode {:.3} ms, python {:.3} ms",
            size,
            ms(enc),
            ms(dec),
            ms(py)
        );

        let (enc, text) = time(|| {
            set.iter()
                .map(|m| serde_json::to_string(&BulkMesh::of(m)).unwrap())
                .collect::<Vec<_>>()
        });
        let (dec, back) = time(|| {
            text.iter()
                .map(|t| serde_json::from_str::<BulkMesh>(t).unwrap().mesh())
                .collect::<Vec<_>>()
        });
        assert!(back.iter().zip(&set).all(|(a, b)| a == *b));
        let lines: Vec<String> = set
            .iter()
            .enumerate()
            .map(|(i, m)| request(i, serde_json::to_value(BulkMesh::of(m)).unwrap()).to_string())
            .collect();
        let py: Duration = lines.iter().map(|l| python_parse(child, l)).sum();
        let size: usize = text.iter().map(String::len).sum();
        println!(
            "json+base64: {:>9} bytes, encode {:.3} ms, decode {:.3} ms, python {:.3} ms",
            size,
            ms(enc),
            ms(dec),
            ms(py)
        );

        let (enc, bytes) = time(|| {
            set.iter()
                .map(|m| {
                    let mut b = Vec::new();
                    ciborium::into_writer(m, &mut b).unwrap();
                    b
                })
                .collect::<Vec<_>>()
        });
        let (dec, back) = time(|| {
            bytes
                .iter()
                .map(|b| ciborium::from_reader::<RenderMesh, _>(&b[..]).unwrap())
                .collect::<Vec<_>>()
        });
        assert!(back.iter().zip(&set).all(|(a, b)| a == *b));
        let size: usize = bytes.iter().map(Vec::len).sum();
        println!(
            "cbor:        {:>9} bytes, encode {:.3} ms, decode {:.3} ms, python (no stdlib cbor)",
            size,
            ms(enc),
            ms(dec)
        );
    }
}

fn round_trips(child: &mut Child2) {
    let evaluate = |id: usize| {
        json!({"jsonrpc": "2.0", "id": id, "method": "feature.evaluate", "params": {
            "type-id": "spike.boss",
            "params": [{"name": "height", "value": {"kind": "length", "si": 0.01}}],
            "choices": [],
            "inputs": [{"name": "on", "value": {"plane": {
                "origin": {"x": 0, "y": 0, "z": 0.005},
                "x-axis": {"x": 1, "y": 0, "z": 0},
                "z-axis": {"x": 0, "y": 0, "z": 1}}}}],
        }})
        .to_string()
    };
    let mut runs = Vec::with_capacity(ROUND_TRIPS);
    let mut handle = 0u32;
    for id in 0..ROUND_TRIPS {
        let t = Instant::now();
        child.send(&evaluate(id));
        let mut calls = 0;
        loop {
            let msg = child.recv();
            if msg.get("method").is_some() {
                handle += 1;
                calls += 1;
                child.send(
                    &json!({"jsonrpc": "2.0", "id": msg["id"], "result": handle}).to_string(),
                );
            } else {
                assert_eq!(msg["id"], id);
                break;
            }
        }
        assert_eq!(calls, CALLBACKS);
        runs.push(t.elapsed());
    }
    let total: Duration = runs.iter().sum();
    let m = median(runs);
    println!(
        "evaluate with {CALLBACKS} callbacks, {ROUND_TRIPS} runs: median {:.3} ms, \
         {:.3} ms a message pair, total {:.1} ms",
        ms(m),
        ms(m) / (CALLBACKS + 1) as f64,
        ms(total)
    );
}

#[test]
fn wire_spike() {
    let mut child = Child2::spawn();
    meshes(&mut child);
    round_trips(&mut child);
}
