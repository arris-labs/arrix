//! The Tier 1 probe (plans/c3-m1 step 2): `gears`, unchanged but for its
//! export line, built for wasm32 with wit-bindgen's export glue (the guest
//! in `tests/probe-tier1/`, a workspace of its own), made a component and
//! run in wasmtime with the world's `kernel` served by a stub.
//!
//! It establishes, for ADR-0035:
//!
//! - which target gives a component whose imports are the world's alone:
//!   `wasm32-unknown-unknown`, componentised by the host (`wit-component`);
//! - where the `unsafe` of the export glue can live under the workspace's
//!   `forbid`: nowhere needs an exception. The SDK (`tests/probe-tier1/sdk`)
//!   and the plugin (`tests/probe-tier1/gears`) both forbid it and build,
//!   because the glue is expanded only in the plugin's crate and from the
//!   SDK's macro, and the lint does not report another crate's macro;
//! - that the world, the `body` resource included, carries `describe`, an
//!   `evaluate` error path, and a spur through the kernel's `extrude` and
//!   back as an owned body;
//! - whether the profile `gears` builds in wasm is the one it builds
//!   natively, to the bit;
//! - the build, componentising, compile and call times, the sizes, and the
//!   fuel a spur takes.
//!
//! Run `cargo test -p arrix-plugin-host --test probe_tier1 -- --nocapture`
//! for the numbers.

use std::path::{Path, PathBuf};
use std::process::Command;
use web_time::Instant;

use arrix_plugin_api::{
    Body, DVec2, DVec3, Diagnostic, Feature, Frame, InputValue, Kernel, ParamValue, Profile,
    ProfileLoop, ProfileSegment, Quantity, QuantityKind, ResolvedInput,
};
use wasmtime::component::{Component, Linker, Resource, ResourceTable};
use wasmtime::{Config, Engine, Store};

mod host {
    wasmtime::component::bindgen!({
        path: "../arrix-plugin-api/wit",
        world: "plugin",
        with: { "arrix:plugin/kernel.body": super::BodyRep },
    });
}

use host::arrix::plugin::kernel as hk;
use host::arrix::plugin::types as ht;
use host::exports::arrix::plugin::feature as hf;

/// A body the stub kernel made: the index of its call.
pub struct BodyRep(usize);

/// The stub kernel: `extrude` records its profile and distance and returns
/// a body; everything else is refused.
#[derive(Default)]
struct Stub {
    table: ResourceTable,
    extruded: Vec<(ht::Profile, f64)>,
}

fn refused() -> ht::Diagnostic {
    ht::Diagnostic {
        severity: ht::Severity::Error,
        code: "probe.stub".into(),
        message: "the stub kernel extrudes only".into(),
        refs: vec![],
        candidates: vec![],
    }
}

impl ht::Host for Stub {}

impl hk::HostBody for Stub {
    fn drop(&mut self, rep: Resource<BodyRep>) -> wasmtime::Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

type Made = Result<Resource<BodyRep>, ht::Diagnostic>;

impl hk::Host for Stub {
    fn extrude(&mut self, p: ht::Profile, distance: f64) -> Made {
        self.extruded.push((p, distance));
        Ok(self.table.push(BodyRep(self.extruded.len())).unwrap())
    }
    fn revolve(&mut self, _: ht::Profile, _: ht::Axis, _: f64) -> Made {
        Err(refused())
    }
    fn names(&mut self, _: Resource<BodyRep>) -> Result<Vec<ht::PersistentRef>, ht::Diagnostic> {
        Err(refused())
    }
    fn face_frame(
        &mut self,
        _: Resource<BodyRep>,
        _: ht::PersistentRef,
    ) -> Result<ht::Frame, ht::Diagnostic> {
        Err(refused())
    }
    fn measure(&mut self, _: Resource<BodyRep>) -> Result<ht::MassProperties, ht::Diagnostic> {
        Err(refused())
    }
    fn fuse(&mut self, _: Resource<BodyRep>, _: Resource<BodyRep>) -> Made {
        Err(refused())
    }
    fn cut(&mut self, _: Resource<BodyRep>, _: Resource<BodyRep>) -> Made {
        Err(refused())
    }
    fn common(&mut self, _: Resource<BodyRep>, _: Resource<BodyRep>) -> Made {
        Err(refused())
    }
    fn fillet(&mut self, _: Resource<BodyRep>, _: Vec<ht::PersistentRef>, _: f64) -> Made {
        Err(refused())
    }
    fn chamfer(&mut self, _: Resource<BodyRep>, _: Vec<ht::PersistentRef>, _: f64) -> Made {
        Err(refused())
    }
    fn copy(&mut self, _: u32, _: Resource<BodyRep>, _: ht::Motion) -> Made {
        Err(refused())
    }
    fn mirror(&mut self, _: Resource<BodyRep>, _: ht::Vec3, _: ht::Vec3) -> Made {
        Err(refused())
    }
}

/// The native side: `gears` at Tier 0 with a kernel that records the
/// profile it is asked to extrude.
#[derive(Default)]
struct Recorder {
    extruded: Vec<(Profile, f64)>,
}

fn unused<T>() -> Result<T, Diagnostic> {
    unreachable!("a spur extrudes only")
}

impl Kernel for Recorder {
    fn extrude(&mut self, p: &Profile, distance: f64) -> Result<Body, Diagnostic> {
        self.extruded.push((p.clone(), distance));
        Ok(Body::from_handle(0))
    }
    fn revolve(
        &mut self,
        _: &Profile,
        _: &arrix_plugin_api::Axis,
        _: f64,
    ) -> Result<Body, Diagnostic> {
        unused()
    }
    fn names(&mut self, _: &Body) -> Result<Vec<arrix_plugin_api::PersistentName>, Diagnostic> {
        unused()
    }
    fn face_frame(
        &mut self,
        _: &Body,
        _: &arrix_plugin_api::PersistentName,
    ) -> Result<Frame, Diagnostic> {
        unused()
    }
    fn measure(&mut self, _: &Body) -> Result<arrix_plugin_api::MassProperties, Diagnostic> {
        unused()
    }
    fn fuse(&mut self, _: &Body, _: &Body) -> Result<Body, Diagnostic> {
        unused()
    }
    fn cut(&mut self, _: &Body, _: &Body) -> Result<Body, Diagnostic> {
        unused()
    }
    fn common(&mut self, _: &Body, _: &Body) -> Result<Body, Diagnostic> {
        unused()
    }
    fn fillet(
        &mut self,
        _: &Body,
        _: &[arrix_plugin_api::PersistentName],
        _: f64,
    ) -> Result<Body, Diagnostic> {
        unused()
    }
    fn chamfer(
        &mut self,
        _: &Body,
        _: &[arrix_plugin_api::PersistentName],
        _: f64,
    ) -> Result<Body, Diagnostic> {
        unused()
    }
    fn copy(&mut self, _: u32, _: &Body, _: &arrix_plugin_api::Motion) -> Result<Body, Diagnostic> {
        unused()
    }
    fn mirror(&mut self, _: &Body, _: DVec3, _: DVec3) -> Result<Body, Diagnostic> {
        unused()
    }
}

/// Every number of a profile, in one order for both sides.
fn flat_native(p: &Profile) -> Vec<f64> {
    let mut out = Vec::new();
    let f = p.plane();
    for v in [f.origin(), f.x_axis(), f.z_axis()] {
        out.extend([v.x, v.y, v.z]);
    }
    let v2 = |out: &mut Vec<f64>, v: DVec2| out.extend([v.x, v.y]);
    for l in std::iter::once(p.outer()).chain(p.holes()) {
        match l {
            ProfileLoop::Circle { center, radius, .. } => {
                v2(&mut out, *center);
                out.push(*radius);
            }
            ProfileLoop::Path { start, segments } => {
                v2(&mut out, *start);
                for s in segments {
                    match *s {
                        ProfileSegment::Line { to, .. } => v2(&mut out, to),
                        ProfileSegment::Arc { to, via, .. } => {
                            v2(&mut out, to);
                            v2(&mut out, via);
                        }
                    }
                }
            }
        }
    }
    out
}

fn flat_wit(p: &ht::Profile) -> Vec<f64> {
    let mut out = Vec::new();
    for v in [&p.plane.origin, &p.plane.x_axis, &p.plane.z_axis] {
        out.extend([v.x, v.y, v.z]);
    }
    let v2 = |out: &mut Vec<f64>, v: &ht::Vec2| out.extend([v.x, v.y]);
    for l in std::iter::once(&p.outer).chain(&p.holes) {
        match l {
            ht::ProfileLoop::Circle(c) => {
                v2(&mut out, &c.center);
                out.push(c.radius);
            }
            ht::ProfileLoop::Path(path) => {
                v2(&mut out, &path.start);
                for s in &path.segments {
                    match s {
                        ht::ProfileSegment::Line(l) => v2(&mut out, &l.to),
                        ht::ProfileSegment::Arc(a) => {
                            v2(&mut out, &a.to);
                            v2(&mut out, &a.via);
                        }
                    }
                }
            }
        }
    }
    out
}

fn ulps(a: f64, b: f64) -> u64 {
    (a.to_bits() as i64).abs_diff(b.to_bits() as i64)
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Builds the guest and returns its core module.
fn build_guest() -> Vec<u8> {
    let target_dir = root().join("target/tier1-probe");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/probe-tier1"))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "the guest builds");
    std::fs::read(target_dir.join("wasm32-unknown-unknown/release/probe_gears.wasm")).unwrap()
}

/// The core module's imports, as `module/name`.
fn core_imports(module: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(module) {
        if let wasmparser::Payload::ImportSection(r) = payload.unwrap() {
            for imp in r.into_imports() {
                let imp = imp.unwrap();
                out.push(format!("{}/{}", imp.module, imp.name));
            }
        }
    }
    out
}

fn spur_params() -> Vec<hf::ParamValue> {
    let q = |name: &str, kind, si| hf::ParamValue {
        name: name.into(),
        value: ht::Quantity { kind, si },
    };
    vec![
        q("teeth", ht::QuantityKind::Count, 20.0),
        q("module", ht::QuantityKind::Length, 0.001),
        q("width", ht::QuantityKind::Length, 0.008),
        q(
            "pressure_angle",
            ht::QuantityKind::Angle,
            20f64.to_radians(),
        ),
    ]
}

fn xy() -> ht::Frame {
    let v = |x, y, z| ht::Vec3 { x, y, z };
    ht::Frame {
        origin: v(0.0, 0.0, 0.0),
        x_axis: v(1.0, 0.0, 0.0),
        z_axis: v(0.0, 0.0, 1.0),
    }
}

#[test]
fn gears_runs_as_a_component() {
    let t = Instant::now();
    let module = build_guest();
    let build = t.elapsed();

    let imports = core_imports(&module);
    for imp in &imports {
        assert!(
            imp.starts_with("arrix:plugin/kernel@0.6.0/")
                || imp.starts_with("[export]arrix:plugin/feature@0.6.0/"),
            "a core import outside the world: {imp}"
        );
    }

    let t = Instant::now();
    let component = wit_component::ComponentEncoder::default()
        .module(&module)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap();
    let encode = t.elapsed();

    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.cranelift_nan_canonicalization(true);
    let engine = Engine::new(&config).unwrap();
    let t = Instant::now();
    let component = Component::new(&engine, &component).unwrap();
    let compile = t.elapsed();

    let world: Vec<String> = component
        .component_type()
        .imports(&engine)
        .map(|(name, _)| name.to_owned())
        .collect();
    assert_eq!(
        world,
        ["arrix:plugin/types@0.6.0", "arrix:plugin/kernel@0.6.0"],
        "the component imports the world alone"
    );

    let mut linker = Linker::<Stub>::new(&engine);
    host::Plugin::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |s| s).unwrap();
    let pre = linker.instantiate_pre(&component).unwrap();

    let t = Instant::now();
    let mut store = Store::new(&engine, Stub::default());
    store.set_fuel(u64::MAX).unwrap();
    let plugin = host::PluginPre::new(pre)
        .unwrap()
        .instantiate(&mut store)
        .unwrap();
    let instantiate = t.elapsed();
    let feature = plugin.arrix_plugin_feature();

    // describe: the native spec, field by field where it matters.
    let specs = feature.call_describe(&mut store).unwrap();
    let native = arrix_gears::Gears.describe();
    assert_eq!(specs.len(), native.len());
    assert_eq!(specs[0].id, native[0].id);
    assert_eq!(specs[0].version, native[0].version);
    let names: Vec<_> = specs[0]
        .params
        .iter()
        .map(|p| (&p.name, &p.default))
        .collect();
    let native_names: Vec<_> = native[0]
        .params
        .iter()
        .map(|p| (&p.name, &p.default))
        .collect();
    assert_eq!(names, native_names);

    // evaluate's error path, kernel-free.
    let err = feature
        .call_evaluate(&mut store, "gears.nope", &[], &[], &[])
        .unwrap()
        .unwrap_err();
    assert_eq!(err.code, "gears.unknown-type");

    // evaluate a spur: through `extrude` and back as an owned body.
    let fuel_before = store.get_fuel().unwrap();
    let t = Instant::now();
    let out = feature
        .call_evaluate(
            &mut store,
            "gears.spur",
            &spur_params(),
            &[],
            &[hf::ResolvedInput {
                name: "plane".into(),
                value: hf::InputValue::Plane(xy()),
            }],
        )
        .unwrap()
        .expect("the spur evaluates");
    let call = t.elapsed();
    let fuel = fuel_before - store.get_fuel().unwrap();
    assert_eq!(out.slots.len(), 1);
    let hf::OutputValue::Body(body) = &out.slots[0].value else {
        panic!("a body slot");
    };
    assert_eq!(
        store.data().table.get(body).unwrap().0,
        1,
        "the extruded body"
    );

    // The same spur natively.
    let mut recorder = Recorder::default();
    let q = |name: &str, kind, si| ParamValue {
        name: name.into(),
        value: Quantity::si(kind, si),
    };
    arrix_gears::Gears
        .evaluate(
            &mut recorder,
            "gears.spur",
            &[
                q("teeth", QuantityKind::Count, 20.0),
                q("module", QuantityKind::Length, 0.001),
                q("width", QuantityKind::Length, 0.008),
                q("pressure_angle", QuantityKind::Angle, 20f64.to_radians()),
            ],
            &[],
            &[ResolvedInput {
                name: "plane".into(),
                value: InputValue::Plane(Frame::WORLD_XY),
            }],
        )
        .unwrap();
    let (wasm_profile, wasm_distance) = &store.data().extruded[0];
    let (native_profile, native_distance) = &recorder.extruded[0];
    assert_eq!(wasm_distance, native_distance);
    let (w, n) = (flat_wit(wasm_profile), flat_native(native_profile));
    assert_eq!(w.len(), n.len(), "the same outline");
    let differ = w
        .iter()
        .zip(&n)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    let worst = w.iter().zip(&n).map(|(a, b)| ulps(*a, *b)).max().unwrap();
    let metres = w
        .iter()
        .zip(&n)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);

    println!(
        "probe: build {build:?}, module {} B, componentise {encode:?}, component compile \
         {compile:?}, instantiate {instantiate:?}, spur evaluate {call:?} on {fuel} fuel",
        module.len()
    );
    println!("probe: core imports {imports:?}");
    println!(
        "probe: profile {} numbers, {differ} differ from native, worst {worst} ulp, {metres:e} m",
        n.len()
    );
}
