//! The solver drivers: DogLeg and Levenberg-Marquardt over one subsystem,
//! the multi-priority driver that sequences them, and the public entry
//! points `solve`/`solve_with_options`/`solve_with_drag`.

use crate::constraint::ConstraintPriority;
use crate::ids::PointId;
use crate::sketch::Sketch;

use super::drag::DragSession;

use super::jacobian::*;
use super::linalg::*;
use super::residuals::*;
use super::system::*;
use super::*;

/// Solve `(A + λI) δ = b` for symmetric positive-definite `A = JᵀJ` via
/// Gaussian elimination with partial pivoting. `a` is n×n row-major flat.
pub(crate) fn solve_normal(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
    for col in 0..n {
        let mut pivot_row = col;
        let mut pivot_val = a[col * n + col].abs();
        for row in (col + 1)..n {
            let v = a[row * n + col].abs();
            if v > pivot_val {
                pivot_val = v;
                pivot_row = row;
            }
        }
        if pivot_val < 1e-18 {
            return false;
        }
        if pivot_row != col {
            for k in 0..n {
                a.swap(col * n + k, pivot_row * n + k);
            }
            b.swap(col, pivot_row);
        }
        let pivot = a[col * n + col];
        for row in (col + 1)..n {
            let factor = a[row * n + col] / pivot;
            for k in col..n {
                a[row * n + k] -= factor * a[col * n + k];
            }
            b[row] -= factor * b[col];
        }
    }
    for i in (0..n).rev() {
        let mut sum = b[i];
        for k in (i + 1)..n {
            sum -= a[i * n + k] * b[k];
        }
        let diag = a[i * n + i];
        if diag.abs() < 1e-18 {
            return false;
        }
        b[i] = sum / diag;
    }
    true
}

pub(crate) fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(&x, &y)| x * y).sum()
}

pub(crate) fn primary_residual_norm(r: &[f64], priorities: &[ConstraintPriority]) -> f64 {
    let mut sum_sq = 0.0;
    let mut has_primary = false;
    for (i, &val) in r.iter().enumerate() {
        let p = priorities
            .get(i)
            .copied()
            .unwrap_or(ConstraintPriority::Primary);
        if p == ConstraintPriority::Primary {
            sum_sq += val * val;
            has_primary = true;
        }
    }
    if has_primary {
        sum_sq.sqrt()
    } else {
        residual_norm(r)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubsystemSolveResult {
    pub(crate) converged: bool,
    pub(crate) iterations: usize,
    pub(crate) residual_norm: f64,
    pub(crate) rank: usize,
    pub(crate) dof: i32,
}

/// With [`SolverOptions::sketch_scaled_trust`], the first DogLeg step may
/// move the variables by this fraction of the sketch's size
/// ([`trust_scale`]); the region doubles from there.
const INITIAL_TRUST_FRACTION: f64 = 0.1;

/// Below this (1 mm) a sketch counts as 1 mm across for [`trust_scale`], so
/// a sketch of coincident points still gets a usable first step.
const MIN_TRUST_SCALE: f64 = 1e-3;

/// The sketch's size, meters: the diagonal of the box around its points and
/// its circles. The trust region starts from it instead of from 1 m, so a
/// millimetre sketch does not take a first step a thousand times its size.
pub(crate) fn trust_scale(sketch: &Sketch) -> f64 {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let mut grow = |x: f64, y: f64, r: f64| {
        lo = [lo[0].min(x - r), lo[1].min(y - r)];
        hi = [hi[0].max(x + r), hi[1].max(y + r)];
    };
    for p in sketch.points.values() {
        grow(p.x, p.y, 0.0);
    }
    for e in sketch.entities.values() {
        if let Entity::Circle { center, radius } = e
            && let Some(c) = sketch.points.get(center)
        {
            grow(c.x, c.y, *radius);
        }
    }
    let diag = (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
    if diag.is_finite() {
        diag.max(MIN_TRUST_SCALE)
    } else {
        MIN_TRUST_SCALE
    }
}

/// The weight on each variable's pull back to where the solve started: what
/// keeps an under-constrained system from wandering.
const ANCHOR_WEIGHT: f64 = 1e-6;
const DRAG_WEIGHT: f64 = 1.0;

/// The residual vector DogLeg minimises: the weighted constraint rows, then
/// one anchor row per variable.
fn anchored_residuals(r_raw: &[f64], weights: &[f64], x: &[f64], x0: &[f64]) -> Vec<f64> {
    let mut r = Vec::with_capacity(r_raw.len() + x.len());
    for (i, &ri) in r_raw.iter().enumerate() {
        let w = weights.get(i).copied().unwrap_or(1.0);
        r.push(w * ri);
    }
    for i in 0..x.len() {
        r.push(ANCHOR_WEIGHT * (x[i] - x0[i]));
    }
    r
}

/// `JᵀJ` (dense, row-major) and the gradient `Jᵀr` of the anchored system.
fn normal_equations(
    j_rows: &JacobianRows,
    weights: &[f64],
    r: &[f64],
    x: &[f64],
    x0: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let n = x.len();
    let mut a = vec![0.0; n * n];
    let mut g = vec![0.0; n];
    for (i, row) in j_rows.rows.iter().enumerate() {
        let w = weights.get(i).copied().unwrap_or(1.0);
        let w2 = w * w;
        for &(c1, v1) in row {
            g[c1] += v1 * (w * r[i]);
            for &(c2, v2) in row {
                a[c1 * n + c2] += w2 * v1 * v2;
            }
        }
    }
    const W2: f64 = ANCHOR_WEIGHT * ANCHOR_WEIGHT;
    for i in 0..n {
        a[i * n + i] += W2;
        g[i] += W2 * (x[i] - x0[i]);
    }
    (a, g)
}

/// `A·v` for a dense row-major `n × n` matrix.
fn mat_vec(a: &[f64], v: &[f64], n: usize) -> Vec<f64> {
    (0..n)
        .map(|row| {
            let mut sum = 0.0;
            for col in 0..n {
                sum += a[row * n + col] * v[col];
            }
            sum
        })
        .collect()
}

/// The Gauss–Newton step `A h = −g`, damped once if `A` is singular, and a
/// small gradient step if even that fails.
fn gauss_newton_step(a: &[f64], g: &[f64], n: usize) -> Vec<f64> {
    let mut h_gn: Vec<f64> = g.iter().map(|v| -*v).collect();
    let mut a_gn = a.to_vec();
    if !solve_normal(&mut a_gn, &mut h_gn, n) {
        for i in 0..n {
            a_gn[i * n + i] += 1e-6 * a[i * n + i].abs().max(1.0);
        }
        h_gn = g.iter().map(|v| -*v).collect();
        if !solve_normal(&mut a_gn, &mut h_gn, n) {
            h_gn = g.iter().map(|v| -1e-4 * v).collect();
        }
    }
    h_gn
}

/// The Cauchy point: the steepest-descent step to the minimum along `−g`.
fn steepest_descent_step(a: &[f64], g: &[f64], n: usize) -> Vec<f64> {
    let ag = mat_vec(a, g, n);
    let g_ag = dot(g, &ag);
    let g_sq = dot(g, g);
    let alpha = if g_ag > 1e-18 { g_sq / g_ag } else { 1e-3 };
    g.iter().map(|&gi| -alpha * gi).collect()
}

/// Powell's dog leg inside a trust region of radius `delta`: the
/// Gauss–Newton step if it fits, the clipped descent step if even that does
/// not, else the point where the leg between them meets the boundary.
fn dogleg_step(h_gn: &[f64], h_sd: &[f64], delta: f64) -> Vec<f64> {
    let h_gn_norm = residual_norm(h_gn);
    let h_sd_norm = residual_norm(h_sd);
    if h_gn_norm <= delta {
        return h_gn.to_vec();
    }
    if h_sd_norm >= delta {
        let scale = delta / h_sd_norm.max(1e-18);
        return h_sd.iter().map(|&v| v * scale).collect();
    }
    let b: Vec<f64> = h_gn.iter().zip(h_sd).map(|(gn, sd)| gn - sd).collect();
    let b_norm_sq = dot(&b, &b);
    let sd_b = dot(h_sd, &b);
    let c = (delta + h_sd_norm) * (delta - h_sd_norm);
    let beta = if b_norm_sq > 1e-18 {
        if sd_b > 0.0 {
            c / (sd_b + (sd_b * sd_b + c * b_norm_sq).max(0.0).sqrt())
        } else {
            (-sd_b + (sd_b * sd_b + c * b_norm_sq).max(0.0).sqrt()) / b_norm_sq
        }
    } else {
        0.0
    };
    let beta = beta.clamp(0.0, 1.0);
    h_sd.iter().zip(&b).map(|(sd, bi)| sd + beta * bi).collect()
}

/// The ratio of actual to predicted reduction that grows or shrinks the
/// trust region.
fn gain_ratio(actual: f64, predicted: f64) -> f64 {
    if predicted > 1e-18 {
        actual / predicted
    } else if actual > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Leaves `x` in the sketch and reports on it: the primary residual, and
/// the rank and DoF of the Jacobian there.
fn finish(
    sys: &System,
    sketch: &mut Sketch,
    x: &[f64],
    iterations: usize,
    converged_below: f64,
) -> SubsystemSolveResult {
    sys.unpack_into(x, sketch);
    let r = sys.residuals_of(sketch);
    let norm = primary_residual_norm(&r, &sys.residual_priorities);
    let rank = matrix_rank(&jacobian(sys, sketch, x).to_dense());
    SubsystemSolveResult {
        converged: norm < converged_below,
        iterations,
        residual_norm: norm,
        rank,
        dof: sys.n_vars() as i32 - rank as i32,
    }
}

pub(crate) fn solve_system_dogleg(
    sys: &System,
    sketch: &mut Sketch,
    max_iterations: usize,
    residual_tol: f64,
    step_tol: f64,
    sketch_scaled_trust: bool,
) -> SubsystemSolveResult {
    let n = sys.n_vars();
    if n == 0 {
        let norm = residual_norm(&sys.residuals_of(sketch));
        return SubsystemSolveResult {
            converged: norm < residual_tol * 10.0,
            iterations: 0,
            residual_norm: norm,
            rank: 0,
            dof: 0,
        };
    }

    let mut x = sys.pack(sketch);
    let x0 = x.clone();
    let weights: Vec<f64> = sys
        .residual_priorities
        .iter()
        .map(|&p| match p {
            ConstraintPriority::Primary => 1.0,
            ConstraintPriority::Drag => DRAG_WEIGHT,
        })
        .collect();

    sys.unpack_into(&x, sketch);
    if residual_norm(&sys.residuals_of(sketch)) < residual_tol {
        let mut done = finish(sys, sketch, &x, 0, f64::INFINITY);
        done.converged = true;
        return done;
    }

    let mut delta: f64 = if sketch_scaled_trust {
        INITIAL_TRUST_FRACTION * trust_scale(sketch)
    } else {
        1.0
    };
    let delta_max: f64 = 1e6;
    let mut iterations = 0;

    while iterations < max_iterations {
        sys.unpack_into(&x, sketch);
        let r_raw = sys.residuals_of(sketch);
        if residual_norm(&r_raw) < residual_tol {
            break;
        }
        let r = anchored_residuals(&r_raw, &weights, &x, &x0);
        let f_current = 0.5 * dot(&r, &r);
        let (a, g) = normal_equations(&jacobian(sys, sketch, &x), &weights, &r, &x, &x0);

        let primary_norm = primary_residual_norm(&r_raw, &sys.residual_priorities);
        if residual_norm(&g) < 1e-14 && primary_norm < residual_tol * 100.0 {
            break;
        }

        let h_gn = gauss_newton_step(&a, &g, n);
        let h_sd = steepest_descent_step(&a, &g, n);
        let h_dl = dogleg_step(&h_gn, &h_sd, delta);

        let step_len = residual_norm(&h_dl);
        let x_trial: Vec<f64> = x.iter().zip(&h_dl).map(|(xi, hi)| xi + hi).collect();
        sys.unpack_into(&x_trial, sketch);
        let r_trial_raw = sys.residuals_of(sketch);
        let r_trial = anchored_residuals(&r_trial_raw, &weights, &x_trial, &x0);

        let f_trial = 0.5 * dot(&r_trial, &r_trial);
        let actual_reduction = f_current - f_trial;
        let pred_reduction = -dot(&g, &h_dl) - 0.5 * dot(&h_dl, &mat_vec(&a, &h_dl, n));
        let rho = gain_ratio(actual_reduction, pred_reduction);

        if rho > 0.75 {
            if step_len > 0.8 * delta {
                delta = (2.0 * delta).min(delta_max);
            }
        } else if rho < 0.25 {
            delta *= 0.5;
        }

        iterations += 1;

        if rho > 0.0 {
            x = x_trial;
            let trial_total = residual_norm(&r_trial_raw);
            if actual_reduction < residual_tol * residual_tol * 1e-4 && trial_total < residual_tol {
                break;
            }
        }

        if step_len < step_tol && primary_norm < residual_tol * 100.0 {
            break;
        }
        if delta < 1e-15 {
            break;
        }
    }

    finish(sys, sketch, &x, iterations, residual_tol * 100.0)
}

pub(crate) fn solve_system_lm(
    sys: &System,
    sketch: &mut Sketch,
    max_iterations: usize,
    residual_tol: f64,
    step_tol: f64,
) -> SubsystemSolveResult {
    let n = sys.n_vars();
    let m = sys.n_residuals();

    if n == 0 {
        let r = sys.residuals_of(sketch);
        let norm = residual_norm(&r);
        return SubsystemSolveResult {
            converged: norm < residual_tol * 10.0,
            iterations: 0,
            residual_norm: norm,
            rank: 0,
            dof: 0,
        };
    }

    let mut x = sys.pack(sketch);
    let mut lambda = LAMBDA0;
    let mut iterations = 0;

    let x0 = x.clone();
    const ANCHOR_WEIGHT: f64 = 1e-6;
    const DRAG_WEIGHT: f64 = 1.0;

    let weights: Vec<f64> = sys
        .residual_priorities
        .iter()
        .map(|&p| match p {
            ConstraintPriority::Primary => 1.0,
            ConstraintPriority::Drag => DRAG_WEIGHT,
        })
        .collect();

    loop {
        sys.unpack_into(&x, sketch);
        let r_raw = sys.residuals_of(sketch);
        let total_norm = residual_norm(&r_raw);
        if total_norm < residual_tol {
            break;
        }
        if iterations >= max_iterations {
            break;
        }

        let mut r = Vec::with_capacity(m + n);
        for (i, &ri) in r_raw.iter().enumerate() {
            let w = weights.get(i).copied().unwrap_or(1.0);
            r.push(w * ri);
        }
        for i in 0..n {
            r.push(ANCHOR_WEIGHT * (x[i] - x0[i]));
        }
        let residual_norm_val = residual_norm(&r);

        let j = jacobian(sys, sketch, &x);

        let mut a = vec![0.0; n * n];
        let mut g = vec![0.0; n];
        for (i, row) in j.rows.iter().enumerate() {
            let w = weights.get(i).copied().unwrap_or(1.0);
            let w2 = w * w;
            for &(c1, v1) in row {
                g[c1] += v1 * (w * r[i]);
                for &(c2, v2) in row {
                    a[c1 * n + c2] += w2 * v1 * v2;
                }
            }
        }
        const W2: f64 = ANCHOR_WEIGHT * ANCHOR_WEIGHT;
        for i in 0..n {
            a[i * n + i] += W2;
            g[i] += W2 * (x[i] - x0[i]);
        }

        let mut accepted = false;
        let mut next_x = x.clone();
        let mut next_norm = residual_norm_val;
        for _ in 0..8 {
            let mut a_damped = a.clone();
            for i in 0..n {
                let diag = a[i * n + i].abs().max(1e-12);
                a_damped[i * n + i] += lambda * diag;
            }
            let mut delta = g.clone();
            for v in &mut delta {
                *v = -*v;
            }
            if !solve_normal(&mut a_damped, &mut delta, n) {
                lambda *= 10.0;
                continue;
            }
            let step = residual_norm(&delta);
            if step < step_tol {
                accepted = true;
                next_x = x.clone();
                break;
            }
            let mut trial = x.clone();
            for i in 0..n {
                trial[i] += delta[i];
            }
            sys.unpack_into(&trial, sketch);
            let r_trial_raw = sys.residuals_of(sketch);
            let mut r_trial = Vec::with_capacity(m + n);
            for (i, &ri) in r_trial_raw.iter().enumerate() {
                let w = weights.get(i).copied().unwrap_or(1.0);
                r_trial.push(w * ri);
            }
            for i in 0..n {
                r_trial.push(ANCHOR_WEIGHT * (trial[i] - x0[i]));
            }
            let trial_norm = residual_norm(&r_trial);
            if trial_norm < residual_norm_val {
                next_x = trial;
                next_norm = trial_norm;
                accepted = true;
                lambda = (lambda / 3.0).max(1e-12);
                break;
            }
            lambda *= 10.0;
            if lambda > 1e12 {
                break;
            }
        }

        iterations += 1;
        if !accepted {
            break;
        }
        let improvement = residual_norm_val - next_norm;
        x = next_x;
        if improvement < residual_tol * residual_tol {
            break;
        }
    }

    sys.unpack_into(&x, sketch);
    let r_final_raw = sys.residuals_of(sketch);
    let final_primary_norm = primary_residual_norm(&r_final_raw, &sys.residual_priorities);

    let j_rows = jacobian(sys, sketch, &x);
    let j = j_rows.to_dense();
    let rank = matrix_rank(&j);
    let dof = n as i32 - rank as i32;

    SubsystemSolveResult {
        converged: final_primary_norm < residual_tol * 100.0,
        iterations,
        residual_norm: final_primary_norm,
        rank,
        dof,
    }
}

pub(crate) fn solve_subsystem(
    sys: &System,
    sketch: &mut Sketch,
    options: &SolverOptions,
) -> SubsystemSolveResult {
    let has_drag = sys.residual_priorities.contains(&ConstraintPriority::Drag);
    if has_drag {
        // A subsystem the primary constraints already fix completely has
        // nothing for a drag-priority row to move: solve it without them.
        let primary_sys = sys.primary_only(sketch);
        let x = primary_sys.pack(sketch);
        let j = jacobian(&primary_sys, sketch, &x).to_dense();
        let primary_dof = primary_sys.n_vars() as i32 - matrix_rank(&j) as i32;
        if primary_dof <= 0 {
            return solve_system(&primary_sys, sketch, options);
        }
    }

    solve_system(sys, sketch, options)
}

/// One subsystem through the configured algorithm. DogLeg that fails to
/// converge falls back to LM, started from where DogLeg stopped.
pub(crate) fn solve_system(
    sys: &System,
    sketch: &mut Sketch,
    options: &SolverOptions,
) -> SubsystemSolveResult {
    let lm = |sketch: &mut Sketch| {
        solve_system_lm(
            sys,
            sketch,
            options.max_iterations,
            options.residual_tol,
            options.step_tol,
        )
    };
    match options.algorithm {
        SolverAlgorithm::DogLeg => {
            let res = solve_system_dogleg(
                sys,
                sketch,
                options.max_iterations,
                options.residual_tol,
                options.step_tol,
                options.sketch_scaled_trust,
            );
            if res.converged {
                res
            } else {
                keep_better_of_lm(sys, sketch, res, lm)
            }
        }
        SolverAlgorithm::LevenbergMarquardt => lm(sketch),
    }
}

/// Runs the LM fallback after an unconverged DogLeg and keeps whichever
/// did better. LM writes its `x` into the sketch as it goes, so when its
/// result is the one discarded, DogLeg's `x` is put back: the returned
/// residual, rank and DoF always describe the geometry left in the sketch.
fn keep_better_of_lm(
    sys: &System,
    sketch: &mut Sketch,
    dogleg: SubsystemSolveResult,
    lm: impl FnOnce(&mut Sketch) -> SubsystemSolveResult,
) -> SubsystemSolveResult {
    let dogleg_x = sys.pack(sketch);
    let lm_res = lm(sketch);
    if lm_res.residual_norm < dogleg.residual_norm || lm_res.converged {
        return lm_res;
    }
    sys.unpack_into(&dogleg_x, sketch);
    dogleg
}

/// Solves the sketch constraint system using subsystem graph decomposition and
/// the specified solver options.
pub fn solve_with_options(sketch: &mut Sketch, options: &SolverOptions) -> SolveResult {
    crate::instrument::note_solve();
    let full_sys = System::build(sketch);
    solve_partitioned(&full_sys, sketch, options, solve_subsystem)
}

/// `full_sys` split into its independent subsystems, each through
/// `solve_sub`, and the results summed into one [`SolveResult`].
pub(crate) fn solve_partitioned(
    full_sys: &System,
    sketch: &mut Sketch,
    options: &SolverOptions,
    mut solve_sub: impl FnMut(&System, &mut Sketch, &SolverOptions) -> SubsystemSolveResult,
) -> SolveResult {
    let n_total = full_sys.n_vars();
    let m_total = full_sys.n_residuals();

    if n_total == 0 {
        let r = full_sys.residuals_of(sketch);
        let residual_norm_val = residual_norm(&r);
        return SolveResult {
            converged: residual_norm_val < options.residual_tol * 10.0,
            iterations: 0,
            residual_norm: residual_norm_val,
            dof: 0,
            rank: 0,
            n_vars: 0,
            n_residuals: m_total,
        };
    }

    let subsystems = full_sys.partition(sketch);
    let mut total_converged = true;
    let mut max_iterations = 0;
    let mut total_rank = 0;
    let mut total_dof = 0;
    let mut sum_sq_residuals = 0.0;

    for sub in &subsystems {
        let sub_res = solve_sub(sub, sketch, options);
        if !sub_res.converged {
            total_converged = false;
        }
        max_iterations = max_iterations.max(sub_res.iterations);
        total_rank += sub_res.rank;
        total_dof += sub_res.dof;
        sum_sq_residuals += sub_res.residual_norm * sub_res.residual_norm;
    }

    let overall_residual_norm = sum_sq_residuals.sqrt();

    SolveResult {
        converged: total_converged && (overall_residual_norm < options.residual_tol * 100.0),
        iterations: max_iterations,
        residual_norm: overall_residual_norm,
        dof: total_dof,
        rank: total_rank,
        n_vars: n_total,
        n_residuals: m_total,
    }
}

/// Runs the nonlinear constraint solver on `sketch`, mutating point positions and circle radii in place.
pub fn solve(sketch: &mut Sketch) -> SolveResult {
    solve_with_options(sketch, &SolverOptions::default())
}

/// One drag frame: `point` pulled toward `(x, y)` by a [`DragSession`]
/// that lives for this frame only. The app keeps one session per drag
/// instead (docs/CONCURRENCY-WASM.md: the solver runs on the UI thread
/// during a drag). A frame that fails leaves the sketch as it was, and the
/// result describes the solve that was rolled back.
pub fn solve_with_drag(sketch: &mut Sketch, point: PointId, x: f64, y: f64) -> SolveResult {
    DragSession::new(sketch, point).frame(sketch, x, y).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Draft;

    fn result(converged: bool, residual_norm: f64) -> SubsystemSolveResult {
        SubsystemSolveResult {
            converged,
            iterations: 1,
            residual_norm,
            rank: 1,
            dof: 3,
        }
    }

    /// A line 30 mm long dimensioned to 50 mm: a system with variables for
    /// a stand-in LM to move. What the solvers would do with it is beside
    /// the point — the fallback's bookkeeping is what's under test.
    fn stretched_line() -> (Draft, System) {
        let mut sketch = Draft::seeded(1);
        let (a, b, _) = sketch.add_line(0.0, 0.0, 0.03, 0.0);
        sketch.add_constraint(Constraint::Distance { a, b, value: 0.05 });
        let sys = System::build(&sketch);
        (sketch, sys)
    }

    /// LM moves the geometry as it iterates. When it ends worse than the
    /// unconverged DogLeg it started from, DogLeg's result is returned — and
    /// DogLeg's geometry must be what the sketch holds, or the reported
    /// residual and rank describe points that are no longer there.
    #[test]
    fn a_discarded_lm_fallback_leaves_dogleg_geometry_behind() {
        let (mut sketch, sys) = stretched_line();
        let dogleg_x = sys.pack(&sketch);

        let kept = keep_better_of_lm(&sys, &mut sketch, result(false, 0.02), |sketch| {
            let worse: Vec<f64> = sys.pack(sketch).iter().map(|v| v + 1.0).collect();
            sys.unpack_into(&worse, sketch);
            result(false, 0.9)
        });

        assert_eq!(kept.residual_norm, 0.02, "DogLeg's result is kept");
        assert_eq!(sys.pack(&sketch), dogleg_x, "and so is its geometry");
    }

    /// How far one DogLeg iteration moves the stretched line's variables.
    fn first_step_length(sketch_scaled_trust: bool) -> f64 {
        let (mut sketch, sys) = stretched_line();
        let x0 = sys.pack(&sketch);
        solve_system_dogleg(&sys, &mut sketch, 1, 1e-9, 1e-12, sketch_scaled_trust);
        let x1 = sys.pack(&sketch);
        let d: Vec<f64> = x0.iter().zip(&x1).map(|(a, b)| b - a).collect();
        residual_norm(&d)
    }

    /// A 30 mm line asked to be 50 mm: from 1 m, the whole Gauss–Newton
    /// step (about 14 mm) is taken at once; from the sketch's size, the
    /// first step stays inside a tenth of its 30 mm.
    #[test]
    fn a_millimetre_sketch_limits_the_first_step() {
        let (sketch, _) = stretched_line();
        assert!((trust_scale(&sketch) - 0.03).abs() < 1e-12);
        let scaled = first_step_length(true);
        assert!(scaled <= 0.003 + 1e-12, "scaled first step {scaled}");
        let unscaled = first_step_length(false);
        assert!(unscaled > 0.01, "unscaled first step {unscaled}");
    }

    #[test]
    fn a_sketch_of_one_point_still_has_a_size() {
        let mut sketch = Draft::seeded(1);
        sketch.add_point(crate::entity::Point::new(0.0, 0.0));
        assert_eq!(trust_scale(&sketch), MIN_TRUST_SCALE);
        assert_eq!(trust_scale(&Sketch::default()), MIN_TRUST_SCALE);
        sketch.add_circle(0.0, 0.0, 0.5);
        assert!((trust_scale(&sketch) - 2f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn a_better_lm_fallback_keeps_its_own_geometry() {
        let (mut sketch, sys) = stretched_line();
        let mut lm_x = Vec::new();

        let kept = keep_better_of_lm(&sys, &mut sketch, result(false, 0.02), |sketch| {
            lm_x = sys.pack(sketch).iter().map(|v| v + 0.01).collect();
            sys.unpack_into(&lm_x, sketch);
            result(true, 1e-12)
        });

        assert!(kept.converged);
        assert_eq!(sys.pack(&sketch), lm_x);
    }
}
