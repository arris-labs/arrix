// `tests/` is never built for wasm (the wasm build compiles the library
// targets only), so the std clock is fine here
// (docs/CONCURRENCY-WASM.md §The browser build).
#[allow(clippy::disallowed_types, reason = "native-only benchmark clock")]
use std::time::Instant;

use arrix_sketch::{
    Constraint, Draft, Point, System, analyze_system, jacobian, jacobian_finite_difference,
};

/// Creates a 10x6 grid of coincident-linked rectangles with horizontal, vertical,
/// and dimension constraints to form a realistic medium-large sketch constraint system.
#[allow(clippy::needless_range_loop)]
fn create_grid_sketch() -> Draft {
    let mut sk = Draft::seeded(1);
    let rows = 10;
    let cols = 6;

    // Create a 2D grid of shared points (11 x 7 = 77 points => 154 variables)
    let mut grid_pts = Vec::new();
    for r in 0..=rows {
        let mut row_pts = Vec::new();
        for c in 0..=cols {
            let p = sk.add_point(Point::new(c as f64 * 10.0 + 0.1, r as f64 * 10.0 + 0.1));
            row_pts.push(p);
        }
        grid_pts.push(row_pts);
    }

    // Fix origin point to ground the system
    sk.add_constraint(Constraint::Fix {
        point: grid_pts[0][0],
        x: 0.0,
        y: 0.0,
    });

    // Horizontal lines & constraints
    for r in 0..=rows {
        for c in 0..cols {
            let p1 = grid_pts[r][c];
            let p2 = grid_pts[r][c + 1];
            let line = sk.add_entity(arrix_sketch::Entity::Line { start: p1, end: p2 });
            sk.add_constraint(Constraint::Horizontal { line });
            sk.add_constraint(Constraint::Distance {
                a: p1,
                b: p2,
                value: 10.0,
            });
        }
    }

    // Vertical lines & constraints
    for r in 0..rows {
        for c in 0..=cols {
            let p1 = grid_pts[r][c];
            let p2 = grid_pts[r + 1][c];
            let line = sk.add_entity(arrix_sketch::Entity::Line { start: p1, end: p2 });
            sk.add_constraint(Constraint::Vertical { line });
            sk.add_constraint(Constraint::Distance {
                a: p1,
                b: p2,
                value: 10.0,
            });
        }
    }

    sk
}

#[test]
fn test_jacobian_sparsity_gate() {
    let sk = create_grid_sketch();
    let sys = System::build(&sk);
    let x = sys.pack(&sk);
    let n = sys.n_vars();
    let m = sys.n_residuals();

    let j_rows = jacobian(&sys, &sk, &x);

    let mut nnz = 0;
    for row in &j_rows.rows {
        nnz += row.len();
    }

    let dense_size = m * n;
    println!(
        "Grid benchmark system: vars n = {n}, residuals m = {m}, nonzeros nnz = {nnz} (dense capacity = {dense_size})"
    );

    // Deterministic machine-independent gate:
    // Sparsity check proving sparse row assembly avoids dense allocation:
    // Nonzero entries must be strictly less than dense_size / 4.
    assert!(
        nnz < dense_size / 4,
        "Jacobian sparsity regression: nnz ({nnz}) >= dense_size / 4 ({})",
        dense_size / 4
    );
}

// NOTE: `Instant::now()` would panic on wasm32-unknown-unknown; see the
// allow on the import above.
#[test]
fn test_analytical_vs_fd_speedup_benchmark() {
    let mut sk = create_grid_sketch();
    let sys = System::build(&sk);
    let x = sys.pack(&sk);

    // 3 untimed warm-up evaluations
    for _ in 0..3 {
        let _ = jacobian(&sys, &sk, &x);
        let _ = jacobian_finite_difference(&sys, &mut sk, &x);
    }

    // Min of 7 repetitions to be robust against scheduler preemption
    let reps = 7;
    let iters_per_rep = 20;

    let mut min_analytic_ns = u128::MAX;
    for _ in 0..reps {
        let start = Instant::now();
        for _ in 0..iters_per_rep {
            let _ = jacobian(&sys, &sk, &x);
        }
        let elapsed = start.elapsed().as_nanos();
        min_analytic_ns = min_analytic_ns.min(elapsed);
    }

    let mut min_fd_ns = u128::MAX;
    for _ in 0..reps {
        let start = Instant::now();
        for _ in 0..iters_per_rep {
            let _ = jacobian_finite_difference(&sys, &mut sk, &x);
        }
        let elapsed = start.elapsed().as_nanos();
        min_fd_ns = min_fd_ns.min(elapsed);
    }

    let analytic_per_eval_us = (min_analytic_ns as f64) / (iters_per_rep as f64 * 1000.0);
    let fd_per_eval_us = (min_fd_ns as f64) / (iters_per_rep as f64 * 1000.0);
    let ratio = fd_per_eval_us / analytic_per_eval_us;

    println!(
        "Benchmark: Analytic Jacobian = {analytic_per_eval_us:.2} µs/eval vs FD = {fd_per_eval_us:.2} µs/eval => {ratio:.1}x speedup"
    );

    // Assert speedup ratio with 20x safety margin (expected ~100x on n=154, gate at 5.0x)
    assert!(
        ratio >= 5.0,
        "Analytic Jacobian speedup ratio ({ratio:.2}x) fell below safety threshold 5.0x"
    );
}

#[test]
fn test_analyze_system_benchmark() {
    let sk = create_grid_sketch();

    // 3 untimed warm-up evaluations
    for _ in 0..3 {
        let _ = analyze_system(&sk);
    }

    let reps = 7;
    let iters_per_rep = 20;

    let mut min_analysis_ns = u128::MAX;
    for _ in 0..reps {
        let start = Instant::now();
        for _ in 0..iters_per_rep {
            let _ = analyze_system(&sk);
        }
        let elapsed = start.elapsed().as_nanos();
        min_analysis_ns = min_analysis_ns.min(elapsed);
    }

    let analysis_per_eval_us = (min_analysis_ns as f64) / (iters_per_rep as f64 * 1000.0);
    println!(
        "Benchmark: analyze_system on 10x6 grid (154 vars, 274 constraints) = {analysis_per_eval_us:.2} µs/eval"
    );

    // Reported, not gated: a wall-clock ceiling becomes a gate only once it
    // is measured on the reference machine (docs/CONCURRENCY-WASM.md
    // §Budgets), and a debug build sharing the machine with the rest of the
    // suite is not that.
}

/// A grid of `rows × cols` cells, 10 mm square, every line horizontal or
/// vertical and the corner at the origin fixed; only the bottom row and the
/// left column are dimensioned, so the far rows and columns stretch when
/// the far corner is dragged. Returns the draft and that corner.
fn stretchy_grid(rows: usize, cols: usize) -> (Draft, arrix_sketch::PointId) {
    const PITCH: f64 = 0.01;
    let mut sk = Draft::seeded(1);
    let pts: Vec<Vec<_>> = (0..=rows)
        .map(|r| {
            (0..=cols)
                .map(|c| sk.add_point(Point::new(c as f64 * PITCH, r as f64 * PITCH)))
                .collect()
        })
        .collect();
    sk.point_mut(pts[0][0]).unwrap().fixed = true;
    for r in 0..=rows {
        for c in 0..=cols {
            if c < cols {
                let (a, b) = (pts[r][c], pts[r][c + 1]);
                let line = sk.add_entity(arrix_sketch::Entity::Line { start: a, end: b });
                sk.add_constraint(Constraint::Horizontal { line });
                if r == 0 && c + 1 < cols {
                    sk.add_constraint(Constraint::Distance { a, b, value: PITCH });
                }
            }
            if r < rows {
                let (a, b) = (pts[r][c], pts[r + 1][c]);
                let line = sk.add_entity(arrix_sketch::Entity::Line { start: a, end: b });
                sk.add_constraint(Constraint::Vertical { line });
                if c == 0 && r + 1 < rows {
                    sk.add_constraint(Constraint::Distance { a, b, value: PITCH });
                }
            }
        }
    }
    (sk, pts[rows][cols])
}

/// One drag frame on a 200-entity sketch against the 4 ms budget
/// (docs/CONCURRENCY-WASM.md §Budgets): measured and reported, not gated.
/// Run it with `--release --nocapture` for the number the budget means.
#[test]
fn a_drag_frame_on_a_200_entity_sketch_is_measured() {
    let (mut sk, corner) = stretchy_grid(9, 10);
    let entities = sk.entities().len();
    assert!(entities >= 199, "{entities} entities");
    let mut session = arrix_sketch::DragSession::new(&sk, corner);
    let start = sk.point(corner).unwrap().pos();
    let mut frames_ns = Vec::new();
    for i in 1..=60 {
        let t = f64::from(i) * 0.0005;
        let clock = Instant::now();
        let frame = session.step(&mut sk, start[0] + t, start[1] + 0.5 * t);
        frames_ns.push(clock.elapsed().as_nanos());
        assert!(frame.converged, "frame {i}: {frame:?}");
    }
    let moved = sk.point(corner).unwrap().pos();
    assert!((moved[0] - start[0] - 0.03).abs() < 1e-6, "{moved:?}");
    frames_ns.sort_unstable();
    let ms = |ns: u128| ns as f64 / 1e6;
    println!(
        "Benchmark: drag frame on {entities} entities, {} points: median {:.3} ms, worst {:.3} ms",
        sk.points().len(),
        ms(frames_ns[frames_ns.len() / 2]),
        ms(frames_ns[frames_ns.len() - 1]),
    );
}
