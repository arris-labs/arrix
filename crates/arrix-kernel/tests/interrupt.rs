//! The kernel's interrupt (plans/c1-m4 step 2): every Arris call polls the
//! `Kernel`'s flag, so one raised from another thread stops a long call
//! inside the kernel, as `kernel.interrupted`, with the model as it was
//! (Arris's `Control`, A3).

use std::collections::BTreeSet;
use std::thread;
use std::time::Duration;

use web_time::Instant;

use arrix_core::{
    CurveKey, DVec2, FeatureId, Frame, Id, MeshDetail, NameRoot, PersistentName, Profile,
    ProfileLoop, ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelError};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const ROUND: FeatureId = FeatureId(Id(2));
/// Holes a side: 256 hole edges, whose fillet takes about 0.4 s in the
/// workspace's dev profile and grows faster than the count, so a raise
/// at 50 ms lands inside it.
const GRID: u32 = 16;

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

/// A plate `GRID` × 10 mm a side with a 6 mm hole in each 10 mm cell,
/// the holes keyed from 10.
fn perforated() -> Profile {
    let side = 10.0 * f64::from(GRID);
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |k, to| ProfileSegment::Line { key: key(k), to };
    let outer = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments: vec![
            line(1, p(side, 0.0)),
            line(2, p(side, side)),
            line(3, p(0.0, side)),
            line(4, p(0.0, 0.0)),
        ],
    };
    let cell = |i: u32| 5.0 + 10.0 * f64::from(i);
    let holes = (0..GRID * GRID)
        .map(|i| ProfileLoop::Circle {
            key: key(10 + u64::from(i)),
            center: p(cell(i % GRID), cell(i / GRID)),
            radius: 3.0 * MM,
        })
        .collect();
    Profile::new(Frame::WORLD_XY, outer, holes).unwrap()
}

/// Every hole's top edge.
fn hole_edges() -> Vec<PersistentName> {
    (0..GRID * GRID)
        .map(|i| PersistentName {
            kind: TopoKind::Edge,
            root: NameRoot::Sweep {
                feature: PLATE,
                index: 0,
                part: SweepPartName::EndEdge(key(10 + u64::from(i))),
            },
            chain: vec![],
        })
        .collect()
}

fn plate(k: &mut Kernel) -> KernelBody {
    k.extrude(PLATE, 0, &perforated(), 5.0 * MM).unwrap()
}

fn round(k: &mut Kernel, body: KernelBody) -> Result<KernelBody, KernelError> {
    k.fillet(ROUND, 0, body, &hole_edges(), 1.0 * MM)
}

#[test]
fn a_flag_raised_from_another_thread_stops_a_fillet_inside_the_kernel() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let interrupt = k.interrupt();
    let raiser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        interrupt.raise();
        Instant::now()
    });
    let started = Instant::now();
    let stopped = round(&mut k, body).unwrap_err();
    let returned = Instant::now();
    let raised = raiser.join().unwrap();
    assert_eq!(stopped.code().as_str(), "kernel.interrupted", "{stopped}");
    eprintln!(
        "interrupt latency: {:?} from raise to return, the call stopped {:?} in",
        returned.saturating_duration_since(raised),
        returned - started,
    );
    // The failure is recorded like any other.
    let call = stopped.call().expect("a recorded call");
    assert_eq!(call.0 as usize, k.records().len() - 1);

    // A raised flag stops every later call too, whatever its kind.
    let mesh = k.tessellate(body, MeshDetail::Coarse).unwrap_err();
    assert_eq!(mesh.code().as_str(), "kernel.interrupted", "{mesh}");

    // Lowered, the kernel goes on: the stopped call left nothing behind
    // in the model, and the same fillet now succeeds.
    k.interrupt().lower();
    assert_eq!(k.retain(&BTreeSet::from([body])), 0, "nothing to free");
    let rounded = round(&mut k, body).unwrap();
    let before = k.mass_properties(body).unwrap().volume;
    let after = k.mass_properties(rounded).unwrap().volume;
    assert!(after < before, "{after} ≥ {before}");
}

#[test]
fn an_unraised_flag_changes_nothing() {
    let mut a = Kernel::new();
    let mut b = Kernel::new();
    let profile = perforated();
    let x = a.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap();
    let y = b.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap();
    b.interrupt().raise();
    b.interrupt().lower();
    assert_eq!(a.to_bytes(x).unwrap(), b.to_bytes(y).unwrap());
}
