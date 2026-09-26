//! Rank-revealing decomposition stability.
//!
//! `row_space_basis` and `column_orthonormal_basis` back every rank-derived
//! diagnostic — DoF, `var_constrained`, and the redundant-constraint set — so a
//! rank that is off by one silently mislabels a sketch as under- or
//! over-constrained. Both run pivoted modified Gram-Schmidt, whose classic trap
//! is *downdating* the pivot norms (`norm² − dot²`) instead of recomputing
//! them: for a row that is already nearly in the span the two terms cancel
//! catastrophically, and a numerically-zero row still looks like a valid pivot.
//!
//! The oracle here is a deliberately naive implementation that recomputes every
//! norm exactly on every pass — the same role `jacobian_finite_difference`
//! plays for `gradient_validation.rs`.

use arrix_sketch::{Draft, matrix_rank, row_space_basis};

/// Oracle: pivoted modified Gram-Schmidt over rows, recomputing every norm
/// exactly on every pass. Quadratically slower than the real implementation and
/// deliberately so.
fn reference_rank(j: &[Vec<f64>]) -> usize {
    let m = j.len();
    if m == 0 {
        return 0;
    }
    let n = j[0].len();
    if n == 0 {
        return 0;
    }
    let mut rows = j.to_vec();
    let norm = |r: &Vec<f64>| r.iter().map(|v| v * v).sum::<f64>().sqrt();
    let max_norm = rows.iter().map(norm).fold(0.0_f64, f64::max);
    let tol = max_norm * 1e-8 + 1e-15;
    let mut rank = 0;
    let mut basis: Vec<Vec<f64>> = Vec::new();
    for _ in 0..n.min(m) {
        let mut best = None;
        let mut best_n = 0.0_f64;
        for (i, r) in rows.iter().enumerate() {
            let nr = norm(r);
            if nr > best_n {
                best_n = nr;
                best = Some(i);
            }
        }
        if best_n < tol {
            break;
        }
        let bi = best.unwrap();
        let q: Vec<f64> = rows[bi].iter().map(|v| v / best_n).collect();
        rows[bi] = vec![0.0; n];
        for r in rows.iter_mut() {
            let d: f64 = r.iter().zip(&q).map(|(a, b)| a * b).sum();
            for (a, b) in r.iter_mut().zip(&q) {
                *a -= d * b;
            }
        }
        basis.push(q);
        rank += 1;
    }
    rank
}

struct R(u64);
impl R {
    fn f(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        ((x >> 11) as f64 / ((1u64 << 53) as f64)) * 2.0 - 1.0
    }
}

/// Rank-deficient matrices whose rows span ~8 orders of magnitude — the regime
/// where norm downdating loses the deficient direction in the noise.
#[test]
fn rank_matches_exact_norm_oracle_on_scaled_deficient_systems() {
    let mut rng = R(0x9e37_79b9_7f4a_7c15);
    let mut worst = 0i64;
    for trial in 0..400 {
        let m = 4 + (trial % 14);
        let n = 3 + (trial % 11);
        let true_rank = 1 + (trial % n.min(m));
        // Build a rank-`true_rank` matrix: random basis, random combinations,
        // then rows scaled over ~8 orders of magnitude to stress downdating.
        let basis: Vec<Vec<f64>> = (0..true_rank)
            .map(|_| (0..n).map(|_| rng.f()).collect())
            .collect();
        let mut j: Vec<Vec<f64>> = (0..m)
            .map(|_| {
                let mut row = vec![0.0; n];
                for b in &basis {
                    let c = rng.f();
                    for (t, s) in row.iter_mut().zip(b) {
                        *t += c * s;
                    }
                }
                row
            })
            .collect();
        let scale = 10f64.powi(((trial % 9) as i32) - 4);
        for (i, row) in j.iter_mut().enumerate() {
            if i % 3 == 0 {
                for v in row.iter_mut() {
                    *v *= scale;
                }
            }
        }
        let got = row_space_basis(&j).len();
        let want = reference_rank(&j);
        worst = worst.max((got as i64 - want as i64).abs());
        assert_eq!(
            got, want,
            "trial {trial}: row_space_basis rank {got} != recomputed-norm reference {want} \
             (m={m}, n={n}, constructed rank={true_rank}, scale={scale})"
        );
        assert_eq!(
            matrix_rank(&j),
            want,
            "trial {trial}: matrix_rank disagrees"
        );
    }
    println!("max rank deviation: {worst}");
}

/// The leverage-score path (`column_orthonormal_basis`) is private, so it is
/// reachable only through `analyze_system`. This drives it on a sketch whose
/// rows differ by four orders of magnitude — a millimetre-scale hole beside a
/// metre-scale plate — with one exactly-duplicated constraint that must be
/// reported redundant, and one independent constraint that must not be.
#[test]
fn redundancy_survives_mixed_scale_geometry() {
    use arrix_sketch::{Constraint, Point, analyze_system};

    let mut sk = Draft::seeded(1);

    // Metre-scale plate edge.
    let (a, b, plate) = sk.add_line(0.0, 0.0, 1.0, 0.0);
    sk.point_mut(a).unwrap().fixed = true;
    sk.add_constraint(Constraint::Horizontal { line: plate });
    // Exactly the same equation, said a second way.
    let duplicate = sk.add_constraint(Constraint::HorizontalPoints { a, b });
    let sizing = sk.add_constraint(Constraint::Distance { a, b, value: 1.0 });

    // Millimetre-scale hole hanging off it — four orders of magnitude smaller.
    let c = sk.add_point(Point::new(0.5, 0.0002));
    let d = sk.add_point(Point::new(0.5002, 0.0002));
    let hole = sk.add_entity(arrix_sketch::Entity::Line { start: c, end: d });
    sk.add_constraint(Constraint::Horizontal { line: hole });
    let hole_sizing = sk.add_constraint(Constraint::Distance {
        a: c,
        b: d,
        value: 0.0002,
    });

    let analysis = analyze_system(&sk);
    assert!(
        analysis.redundant_constraints.contains(&duplicate),
        "the duplicated horizontal must be flagged redundant, got {:?}",
        analysis.redundant_constraints
    );
    for independent in [sizing, hole_sizing] {
        assert!(
            !analysis.redundant_constraints.contains(&independent),
            "an independent dimension was flagged redundant: {independent:?}"
        );
    }
}
