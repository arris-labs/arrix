//! The grown world's probe (plans/c3-m2 step 1): a scratch world
//! (`tests/plugins/probe-world/wit`) with a `document` import and an
//! `exchange` export beside `feature`, and a guest built as each of its
//! three worlds. It establishes, for ADR-0037:
//!
//! - which interfaces each component imports and exports, so whether a
//!   guest that never calls an import of its world still imports it;
//! - whether one host, bound to the whole world, loads a component that
//!   exports only part of it, and how it tells which parts are there;
//! - what a 200 000-triangle mesh costs through the `document` import and
//!   back out as a binary STL from `exchange`, in time and fuel;
//! - what the same mesh and file cost on the Tier 2 wire, as base64 runs
//!   in JSON (ADR-0022 §5), in Rust and in CPython.
//!
//! Run `cargo test -p arrix-plugin-host --test probe_world -- --nocapture`
//! for the numbers.

#[path = "support/tier1.rs"]
mod tier1_build;

use std::io::Write as _;
use std::process::{Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use web_time::Instant;

mod host {
    wasmtime::component::bindgen!({
        path: "tests/plugins/probe-world/wit",
        world: "full",
    });
}

use host::arrix::probe::document as hd;
use host::exports::arrix::probe::{exchange as hx, feature as hf};

/// The host's `document`: one part, its mesh a grid of `side` × `side`
/// vertices in the XY plane, two triangles per cell.
struct Doc {
    mesh: hd::Triangles,
}

fn grid(side: u32) -> hd::Triangles {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    for j in 0..side {
        for i in 0..side {
            positions.extend([i as f32 * 1e-3, j as f32 * 1e-3, 0.0]);
            normals.extend([0.0, 0.0, 1.0]);
        }
    }
    let mut triangles = Vec::new();
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let a = j * side + i;
            triangles.extend([a, a + 1, a + side + 1, a, a + side + 1, a + side]);
        }
    }
    hd::Triangles {
        positions,
        normals,
        triangles,
    }
}

impl hd::Host for Doc {
    fn parts(&mut self) -> Vec<u64> {
        vec![7]
    }
    fn mesh(&mut self, part: u64) -> hd::Triangles {
        assert_eq!(part, 7);
        self.mesh.clone()
    }
}

/// The STL the exporter should write, built natively.
fn native_stl(m: &hd::Triangles) -> Vec<u8> {
    let mut bytes = vec![0u8; 80];
    bytes.extend(((m.triangles.len() / 3) as u32).to_le_bytes());
    for t in m.triangles.as_chunks::<3>().0 {
        let n = t[0] as usize * 3;
        for c in &m.normals[n..n + 3] {
            bytes.extend(c.to_le_bytes());
        }
        for &v in t {
            let p = v as usize * 3;
            for c in &m.positions[p..p + 3] {
                bytes.extend(c.to_le_bytes());
            }
        }
        bytes.extend([0, 0]);
    }
    bytes
}

fn module(world: &str) -> Vec<u8> {
    let manifest =
        tier1_build::root().join("crates/arrix-plugin-host/tests/plugins/probe-world/Cargo.toml");
    tier1_build::build_with(
        &["--manifest-path", manifest.to_str().unwrap()],
        "probe-world",
        world,
    )
}

fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.cranelift_nan_canonicalization(true);
    Engine::new(&config).unwrap()
}

fn component(engine: &Engine, module: &[u8]) -> Component {
    let bytes = wit_component::ComponentEncoder::default()
        .module(module)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap();
    Component::new(engine, &bytes).unwrap()
}

/// The component's imports and exports, by name.
fn surface(engine: &Engine, c: &Component) -> (Vec<String>, Vec<String>) {
    let ty = c.component_type();
    let names = |it: &mut dyn Iterator<Item = (&str, _)>| it.map(|(n, _)| n.to_owned()).collect();
    (
        names(&mut ty.imports(engine)),
        names(&mut ty.exports(engine)),
    )
}

/// What one host bound to the whole world finds in a component: whether
/// the world's bindings load, and which exported interfaces do.
fn load(engine: &Engine, c: &Component) -> (bool, bool, bool) {
    let mut linker = Linker::<Doc>::new(engine);
    host::Full::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |s| s).unwrap();
    let pre = linker
        .instantiate_pre(c)
        .expect("a linker with more imports than used instantiates");
    let mut store = Store::new(engine, Doc { mesh: grid(2) });
    store.set_fuel(u64::MAX).unwrap();
    let instance = pre.instantiate(&mut store).unwrap();
    let whole = host::Full::new(&mut store, &instance).is_ok();
    let feature = hf::GuestIndices::new(&pre).is_ok();
    let exchange = hx::GuestIndices::new(&pre).is_ok();
    (whole, feature, exchange)
}

#[test]
fn a_component_carries_the_part_of_the_world_it_uses() {
    let engine = engine();
    let full = component(&engine, &module("full"));
    let exporter = component(&engine, &module("exporter"));
    let features = component(&engine, &module("features"));

    let (imports, exports) = surface(&engine, &full);
    println!("probe: full imports {imports:?}, exports {exports:?}");
    assert!(
        imports.is_empty(),
        "an import the guest never calls is not in the component"
    );
    assert_eq!(
        exports,
        ["arrix:probe/feature@0.1.0", "arrix:probe/exchange@0.1.0"]
    );

    let (imports, exports) = surface(&engine, &exporter);
    println!("probe: exporter imports {imports:?}, exports {exports:?}");
    assert_eq!(imports, ["arrix:probe/document@0.1.0"]);
    assert_eq!(exports, ["arrix:probe/exchange@0.1.0"]);

    let (imports, exports) = surface(&engine, &features);
    println!("probe: features imports {imports:?}, exports {exports:?}");
    assert!(imports.is_empty());
    assert_eq!(exports, ["arrix:probe/feature@0.1.0"]);

    // One host, bound to the whole world, loads each by interface.
    assert_eq!(load(&engine, &full), (true, true, true));
    assert_eq!(
        load(&engine, &exporter),
        (false, false, true),
        "the world's bindings refuse a component lacking an export; its interfaces load"
    );
    assert_eq!(load(&engine, &features), (false, true, false));
}

/// 200 000 triangles: a 317 × 317 grid gives 2 × 316² = 199 712.
const SIDE: u32 = 317;

#[test]
fn a_large_mesh_crosses_the_component_boundary() {
    let engine = engine();
    let t = Instant::now();
    let module = module("exporter");
    let build = t.elapsed();
    let c = component(&engine, &module);

    let mesh = grid(SIDE);
    let triangles = mesh.triangles.len() / 3;
    let expected = native_stl(&mesh);

    let mut linker = Linker::<Doc>::new(&engine);
    host::Full::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |s| s).unwrap();
    let pre = linker.instantiate_pre(&c).unwrap();
    let indices = hx::GuestIndices::new(&pre).unwrap();
    let mut store = Store::new(&engine, Doc { mesh });
    store.set_fuel(u64::MAX).unwrap();
    let instance = pre.instantiate(&mut store).unwrap();
    let exchange = indices.load(&mut store, &instance).unwrap();

    let before = store.get_fuel().unwrap();
    let t = Instant::now();
    let files = exchange
        .call_export(&mut store, "probe.stl")
        .unwrap()
        .expect("the exporter writes");
    let call = t.elapsed();
    let fuel = before - store.get_fuel().unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "meshes/7.stl");
    assert!(
        files[0].bytes == expected,
        "the component's STL is the native one"
    );
    println!(
        "probe: build {build:?}; {triangles} triangles in, {} B STL out: export {call:?} on \
         {fuel} fuel ({:.2} of the 2^30 default)",
        expected.len(),
        fuel as f64 / (1u64 << 30) as f64
    );
}

fn b64<T: Copy>(xs: &[T], to_le: impl Fn(T) -> Vec<u8>) -> String {
    let bytes: Vec<u8> = xs.iter().flat_map(|&x| to_le(x)).collect();
    STANDARD.encode(bytes)
}

#[test]
fn a_large_mesh_crosses_the_wire() {
    let mesh = grid(SIDE);
    let stl = native_stl(&mesh);

    // The host's side: the mesh out, the file in.
    let t = Instant::now();
    let line = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "positions": b64(&mesh.positions, |x: f32| x.to_le_bytes().to_vec()),
            "normals": b64(&mesh.normals, |x: f32| x.to_le_bytes().to_vec()),
            "triangles": b64(&mesh.triangles, |x: u32| x.to_le_bytes().to_vec()),
        }
    })
    .to_string();
    let encode = t.elapsed();
    let file_line = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "result": {"ok": [{"name": "meshes/7.stl", "bytes": STANDARD.encode(&stl)}]}
    })
    .to_string();
    let t = Instant::now();
    let parsed: serde_json::Value = serde_json::from_str(&file_line).unwrap();
    let back = STANDARD
        .decode(parsed["result"]["ok"][0]["bytes"].as_str().unwrap())
        .unwrap();
    let decode = t.elapsed();
    assert!(back == stl);
    println!(
        "probe: wire, Rust: mesh line {} B encoded in {encode:?}; STL line {} B decoded in \
         {decode:?}",
        line.len(),
        file_line.len()
    );

    // CPython's side: the mesh in, the file out.
    let script = r#"
import array, base64, json, sys, time
line = sys.stdin.readline()
t = time.perf_counter()
r = json.loads(line)["result"]
p = array.array("f"); p.frombytes(base64.b64decode(r["positions"]))
n = array.array("f"); n.frombytes(base64.b64decode(r["normals"]))
i = array.array("I"); i.frombytes(base64.b64decode(r["triangles"]))
read = time.perf_counter() - t
stl = bytes(80) + (len(i) // 3).to_bytes(4, "little") + bytes(50 * (len(i) // 3))
t = time.perf_counter()
out = json.dumps({"jsonrpc": "2.0", "id": 2, "result": {"ok": [{"name": "m.stl", "bytes": base64.b64encode(stl).decode()}]}})
write = time.perf_counter() - t
print(f"{len(i) // 3} {read * 1e3:.1f} {write * 1e3:.1f} {len(out)}")
"#;
    let Ok(mut child) = Command::new("python3")
        .args(["-I", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    else {
        println!("probe: SKIPPED the CPython side: no python3");
        return;
    };
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(line.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let out = String::from_utf8(out.stdout).unwrap();
    let fields: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(
        fields[0].parse::<usize>().unwrap(),
        mesh.triangles.len() / 3
    );
    println!(
        "probe: wire, CPython: mesh read in {} ms; STL line {} B written in {} ms",
        fields[1], fields[3], fields[2]
    );
}
