use std::f64::consts::PI;

use super::*;

const MM: f64 = 1e-3;

fn spur(teeth: u32, module: f64) -> Spur {
    Spur {
        teeth,
        module,
        pressure_angle: 20.0_f64.to_radians(),
    }
}

fn segments(l: &ProfileLoop) -> (DVec2, &[ProfileSegment]) {
    match l {
        ProfileLoop::Path { start, segments } => (*start, segments),
        ProfileLoop::Circle { .. } => panic!("a gear outline is a path"),
    }
}

/// The true involute of tooth 1's clockwise-facing flank, written
/// independently of the code under test: the base circle unwound from
/// angle `theta0`, which puts the flank at half the tooth's angular
/// thickness, `π / 2z`, on the pitch circle.
fn true_involute(g: &Spur, t: f64) -> DVec2 {
    let rb = g.base_radius();
    let a = g.pressure_angle;
    let theta0 = -PI / (2.0 * f64::from(g.teeth)) - (a.tan() - a);
    let (s, c) = (theta0 + t).sin_cos();
    DVec2::new(rb * (c + t * s), rb * (s - t * c))
}

/// Tooth 1's rising flank arcs as (from, via, to), in walking order.
fn rising_arcs(g: &Spur) -> Vec<(DVec2, DVec2, DVec2)> {
    let outline = g.outline().unwrap();
    let (start, segs) = segments(&outline);
    let mut at = start;
    let mut out = Vec::new();
    for s in segs {
        if let ProfileSegment::Arc { key, to, via } = s
            && (1100..1200).contains(&key.0.0)
        {
            out.push((at, *via, *to));
        }
        at = s.end();
    }
    out
}

#[test]
fn every_flank_arc_is_within_the_tolerance_of_the_true_involute() {
    for g in [
        spur(20, MM),
        spur(18, MM),
        spur(24, 2.5 * MM),
        spur(60, MM),
        spur(8, MM),
    ] {
        let arcs = rising_arcs(&g);
        assert!(!arcs.is_empty() && arcs.len() <= 64, "{} arcs", arcs.len());
        let tol = g.tolerance();
        let rb = g.base_radius();
        let t_of = |p: DVec2| ((p.length() / rb).powi(2) - 1.0).max(0.0).sqrt();
        let mut worst = 0.0_f64;
        for (from, via, to) in &arcs {
            for p in [from, via, to] {
                let t = t_of(*p);
                assert!(
                    (true_involute(&g, t) - *p).length() < 1e-12,
                    "every arc's defining points are on the involute"
                );
            }
            let (center, radius) = circle(*from, *via, *to).unwrap();
            let (t0, t1) = (t_of(*from), t_of(*to));
            for i in 0..=200 {
                let t = t0 + (t1 - t0) * f64::from(i) / 200.0;
                let e = ((true_involute(&g, t) - center).length() - radius).abs();
                worst = worst.max(e);
            }
        }
        assert!(worst <= tol, "z = {}: {worst} > {tol}", g.teeth);
        // The flank runs from the root (or the base circle) to the tip.
        let first = arcs[0].0.length();
        let last = arcs.last().unwrap().2.length();
        assert!((first - g.root_radius().max(rb)).abs() < 1e-12);
        assert!((last - g.tip_radius()).abs() < 1e-12);
    }
}

#[test]
fn the_outline_closes_and_every_tooth_is_symmetric() {
    let g = spur(20, MM);
    let outline = g.outline().unwrap();
    let (start, segs) = segments(&outline);
    assert_eq!(segs.last().unwrap().end(), start, "closed exactly");
    // Tooth 1 is centred on +X: its falling side mirrors its rising side.
    let rising = rising_arcs(&g);
    let falling: Vec<DVec2> = segs
        .iter()
        .filter(|s| (1200..1300).contains(&s.key().0.0))
        .map(|s| s.end())
        .collect();
    assert_eq!(falling.len(), rising.len());
    for ((from, _, _), end) in rising.iter().zip(falling.iter().rev()) {
        assert!((DVec2::new(from.x, -from.y) - *end).length() < 1e-15);
    }
    // Every point is between the root and tip circles.
    for s in segs {
        let r = s.end().length();
        assert!(r >= g.root_radius() - 1e-12 && r <= g.tip_radius() + 1e-12);
    }
}

#[test]
fn keys_name_the_tooth_and_the_curve_and_are_stable() {
    let keys = |g: &Spur| -> Vec<u64> {
        segments(&g.outline().unwrap())
            .1
            .iter()
            .map(|s| s.key().0.0)
            .collect()
    };
    let twenty = keys(&spur(20, MM));
    assert_eq!(twenty, keys(&spur(20, MM)), "the same gear, the same keys");
    assert_eq!(twenty, keys(&spur(20, 3.0 * MM)), "scale keeps the keys");
    let mut unique = twenty.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), twenty.len(), "no key twice");
    assert!(twenty.contains(&20_100) && twenty.contains(&20_003));
    let eighteen = keys(&spur(18, MM));
    assert!(!eighteen.iter().any(|k| (19_000..21_000).contains(k)));
    assert!(eighteen.contains(&18_003));
    // Below 42 teeth the root is inside the base circle: radial lines.
    assert!(twenty.contains(&1_001) && twenty.contains(&1_002));
    assert!(!keys(&spur(60, MM)).contains(&1_001));
}

#[test]
fn a_gear_that_cannot_be_cut_is_refused() {
    let pointed = Spur {
        teeth: 6,
        module: MM,
        pressure_angle: 40.0_f64.to_radians(),
    };
    assert_eq!(pointed.outline(), Err(OutlineError::Pointed));
}
