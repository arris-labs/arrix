//! Dense linear algebra over the Jacobian: rank, row-space basis, and the
//! single-pass redundancy/conflict analysis built on them.

use crate::ids::ConstraintId;
use crate::sketch::Sketch;

use super::jacobian::*;
use super::system::*;
use super::*;

/// Rank of `J` via column-pivoted QR on `JᵀJ` (or on `J` itself for fat
/// systems). Absolute threshold relative to the largest pivot.
pub fn matrix_rank(j: &[Vec<f64>]) -> usize {
    let m = j.len();
    if m == 0 {
        return 0;
    }
    let n = j[0].len();
    if n == 0 {
        return 0;
    }

    // Work on a copy of J (row-major) with column pivoting Gram-Schmidt.
    let mut cols: Vec<Vec<f64>> = (0..n).map(|c| (0..m).map(|r| j[r][c]).collect()).collect();
    let mut rank = 0usize;
    let mut max_norm = 0.0_f64;
    for col in cols.iter().take(n) {
        let nrm = col_norm(col);
        max_norm = max_norm.max(nrm);
    }
    let tol = max_norm * 1e-8 + 1e-15;

    for c in 0..n {
        // Pivot: largest remaining column.
        let mut best = c;
        let mut best_n = col_norm(&cols[c]);
        for (k, col) in cols.iter().enumerate().take(n).skip(c + 1) {
            let nrm = col_norm(col);
            if nrm > best_n {
                best_n = nrm;
                best = k;
            }
        }
        cols.swap(c, best);
        if best_n < tol {
            break;
        }
        // Normalize and subtract projections from later columns.
        let scale = 1.0 / best_n;
        for v in &mut cols[c] {
            *v *= scale;
        }
        let (basis_cols, remaining_cols) = cols.split_at_mut(c + 1);
        let basis = &basis_cols[c];
        for col in remaining_cols.iter_mut().take(n - c - 1) {
            let dot: f64 = basis.iter().zip(col.iter()).map(|(a, b)| a * b).sum();
            for (target, basis_value) in col.iter_mut().zip(basis.iter()).take(m) {
                *target -= dot * basis_value;
            }
        }
        rank += 1;
    }
    rank
}

/// Orthonormal basis for the row space of `J` (size r × n).
pub fn row_space_basis(j: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let m = j.len();
    if m == 0 {
        return Vec::new();
    }
    let n = j[0].len();
    if n == 0 {
        return Vec::new();
    }

    let mut flat = vec![0.0_f64; m * n];
    let mut norms_sq = vec![0.0_f64; m];
    for (r, row) in j.iter().enumerate() {
        let offset = r * n;
        let mut sum_sq = 0.0;
        for (c, &val) in row.iter().enumerate() {
            flat[offset + c] = val;
            sum_sq += val * val;
        }
        norms_sq[r] = sum_sq;
    }

    let max_norm_sq = norms_sq.iter().copied().fold(0.0_f64, f64::max);
    let max_norm = max_norm_sq.sqrt();
    let tol = max_norm * 1e-8 + 1e-15;
    let tol_sq = tol * tol;

    let mut active = vec![true; m];
    let mut basis = Vec::new();

    for _ in 0..n.min(m) {
        let mut best_idx = None;
        let mut best_sq = 0.0_f64;
        for r in 0..m {
            if active[r] && norms_sq[r] > best_sq {
                best_sq = norms_sq[r];
                best_idx = Some(r);
            }
        }
        if best_sq < tol_sq || best_idx.is_none() {
            break;
        }
        let best_r = best_idx.unwrap();
        active[best_r] = false;

        let best_norm = best_sq.sqrt();
        let scale = 1.0 / best_norm;
        let best_offset = best_r * n;
        let q: Vec<f64> = flat[best_offset..best_offset + n]
            .iter()
            .map(|&v| v * scale)
            .collect();

        for r in 0..m {
            if active[r] {
                let offset = r * n;
                let row_slice = &mut flat[offset..offset + n];
                let mut dot = 0.0;
                for (a, &b) in row_slice.iter().zip(q.iter()) {
                    dot += *a * b;
                }
                if dot != 0.0 {
                    let mut sum_sq = 0.0;
                    for (a, &b) in row_slice.iter_mut().zip(q.iter()) {
                        *a -= dot * b;
                        sum_sq += *a * *a;
                    }
                    // Recomputed, never downdated as `norms_sq[r] - dot²`: for a
                    // row that is nearly in the span already the two terms
                    // cancel catastrophically, and a numerically-zero row then
                    // still looks like a valid pivot — overestimating the rank
                    // and under-reporting DoF. The recomputation rides along in
                    // the loop that was already touching all `n` entries, so it
                    // costs nothing.
                    norms_sq[r] = sum_sq;
                }
            }
        }
        basis.push(q);
    }
    basis
}

pub(crate) fn col_norm(col: &[f64]) -> f64 {
    col.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// Detailed linear algebra analysis of the constraint system.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverSystemAnalysis {
    pub vars: Vec<Var>,
    pub residual_owners: Vec<ResidualOwner>,
    pub rank: usize,
    /// For each variable in `vars`, whether it is locally rigid (0 DoF).
    pub var_constrained: Vec<bool>,
    /// Constraints whose equations are satisfied and linearly dependent on other constraints.
    pub redundant_constraints: Vec<ConstraintId>,
    /// Constraints whose equations are *unsatisfied* and linearly dependent
    /// on other constraints: the solver cannot reduce their residual
    /// without growing another's, so the set conflicts. An unsatisfied
    /// constraint that is independent is one the solver merely has not
    /// reached (`SketchStatus::DidNotConverge`).
    pub dependent_unsatisfied: Vec<ConstraintId>,
}

pub(crate) fn column_orthonormal_basis(m: usize, u: &[f64], k: usize) -> (Vec<f64>, Vec<f64>) {
    let mut flat = vec![0.0_f64; k * m];
    let mut norms_sq = vec![0.0_f64; k];
    for r in 0..m {
        let u_offset = r * k;
        for c in 0..k {
            flat[c * m + r] = u[u_offset + c];
        }
    }
    for c in 0..k {
        let col_slice = &flat[c * m..(c + 1) * m];
        norms_sq[c] = col_slice.iter().map(|&v| v * v).sum();
    }

    let max_norm_sq = norms_sq.iter().copied().fold(0.0_f64, f64::max);
    let max_norm = max_norm_sq.sqrt();
    let tol = max_norm * 1e-8 + 1e-15;
    let tol_sq = tol * tol;

    let mut active = vec![true; k];
    let mut y_row_major = vec![0.0_f64; m * k];

    let mut rank_count = 0;
    for _ in 0..k {
        let mut best_idx = None;
        let mut best_sq = 0.0_f64;
        for c in 0..k {
            if active[c] && norms_sq[c] > best_sq {
                best_sq = norms_sq[c];
                best_idx = Some(c);
            }
        }
        if best_sq < tol_sq || best_idx.is_none() {
            break;
        }
        let best_c = best_idx.unwrap();
        active[best_c] = false;

        let best_norm = best_sq.sqrt();
        let scale = 1.0 / best_norm;
        let best_offset = best_c * m;

        for r in 0..m {
            let y_val = flat[best_offset + r] * scale;
            y_row_major[r * k + rank_count] = y_val;
        }

        let y_rank = rank_count;
        for c in 0..k {
            if active[c] {
                let offset = c * m;
                let col_slice = &mut flat[offset..offset + m];
                let mut dot = 0.0;
                for (r, a) in col_slice.iter().enumerate() {
                    dot += *a * y_row_major[r * k + y_rank];
                }
                if dot != 0.0 {
                    // Recomputed rather than downdated — see `row_space_basis`.
                    let mut sum_sq = 0.0;
                    for (r, a) in col_slice.iter_mut().enumerate() {
                        *a -= dot * y_row_major[r * k + y_rank];
                        sum_sq += *a * *a;
                    }
                    norms_sq[c] = sum_sq;
                }
            }
        }
        rank_count += 1;
    }

    let mut h_diag = vec![0.0_f64; m];
    for (r, h_val) in h_diag.iter_mut().enumerate().take(m) {
        let r_offset = r * k;
        let mut sum = 0.0;
        for c in 0..rank_count {
            let v = y_row_major[r_offset + c];
            sum += v * v;
        }
        *h_val = sum;
    }

    (y_row_major, h_diag)
}

#[allow(clippy::needless_range_loop)]
pub(crate) fn is_row_set_redundant(y: &[f64], h_diag: &[f64], k: usize, rows: &[usize]) -> bool {
    let p = rows.len();
    if p == 0 {
        return false;
    }
    const TOL: f64 = 1e-6;
    if p == 1 {
        let r = rows[0];
        return (1.0 - h_diag[r]) > TOL;
    }
    if p == 2 {
        let r1 = rows[0];
        let r2 = rows[1];
        let h11 = h_diag[r1];
        let h22 = h_diag[r2];
        let mut h12 = 0.0;
        let off1 = r1 * k;
        let off2 = r2 * k;
        for c in 0..k {
            h12 += y[off1 + c] * y[off2 + c];
        }
        let m11 = 1.0 - h11;
        let m22 = 1.0 - h22;
        let m12 = -h12;
        let tr = m11 + m22;
        let det = m11 * m22 - m12 * m12;
        let discr = (tr * tr - 4.0 * det).max(0.0);
        let lambda_min = 0.5 * (tr - discr.sqrt());
        return lambda_min > TOL;
    }

    // General p > 2 (e.g. Block constraint)
    let mut m_mat = vec![vec![0.0; p]; p];
    for i in 0..p {
        for j in 0..p {
            let ri = rows[i];
            let rj = rows[j];
            let dot = if i == j {
                h_diag[ri]
            } else {
                let off_i = ri * k;
                let off_j = rj * k;
                let mut sum = 0.0;
                for c in 0..k {
                    sum += y[off_i + c] * y[off_j + c];
                }
                sum
            };
            m_mat[i][j] = if i == j { 1.0 - dot - TOL } else { -dot };
        }
    }
    // Test if m_mat is positive definite via Cholesky decomposition
    for i in 0..p {
        for j in 0..=i {
            let mut sum = m_mat[i][j];
            for l in 0..j {
                sum -= m_mat[i][l] * m_mat[j][l];
            }
            if i == j {
                if sum <= 0.0 {
                    return false;
                }
                m_mat[i][i] = sum.sqrt();
            } else {
                let div = m_mat[j][j];
                if div.abs() < 1e-15 {
                    return false;
                }
                m_mat[i][j] = sum / div;
            }
        }
    }
    true
}

/// The active driving constraints no subsystem in `carried` holds that are
/// left unsatisfied. Nothing they touch can move, so each conflicts with
/// whatever fixed its geometry.
fn unsatisfied_immovable(
    sketch: &Sketch,
    residuals: &std::collections::BTreeMap<ConstraintId, f64>,
    carried: &std::collections::BTreeSet<ConstraintId>,
) -> Vec<ConstraintId> {
    sketch
        .constraints
        .iter()
        .filter(|(cid, r)| {
            r.is_active
                && r.is_driving
                && !carried.contains(cid)
                && residuals.get(cid).copied().unwrap_or(0.0) > RESIDUAL_TOL * 100.0
        })
        .map(|(cid, _)| *cid)
        .collect()
}

/// Performs linear algebra diagnostics (Jacobian rank, null space projections,
/// redundant constraint detection) on the current sketch state by analyzing
/// each independent subsystem component.
pub fn analyze_system(sketch: &Sketch) -> SolverSystemAnalysis {
    let sys = System::build(sketch);
    let n = sys.n_vars();
    if n == 0 {
        let res_map = constraint_residuals(sketch).into_iter().collect();
        return SolverSystemAnalysis {
            vars: Vec::new(),
            residual_owners: sys.residual_owners,
            rank: 0,
            var_constrained: Vec::new(),
            redundant_constraints: Vec::new(),
            dependent_unsatisfied: unsatisfied_immovable(sketch, &res_map, &Default::default()),
        };
    }

    let subsystems = sys.partition(sketch);
    let mut total_rank = 0;
    let mut global_var_constrained = vec![false; n];
    let mut redundant_constraints = Vec::new();
    let mut dependent_unsatisfied = Vec::new();

    let mut var_to_global_idx = std::collections::BTreeMap::new();
    for (idx, &v) in sys.vars.iter().enumerate() {
        var_to_global_idx.insert(v, idx);
    }

    let residuals = constraint_residuals(sketch);
    let res_map: std::collections::BTreeMap<ConstraintId, f64> = residuals.into_iter().collect();

    for sub in &subsystems {
        let sub_n = sub.n_vars();
        let sub_m = sub.n_residuals();
        if sub_n == 0 || sub_m == 0 {
            continue;
        }

        let sub_x = sub.pack(sketch);
        let sub_j_rows = jacobian(sub, sketch, &sub_x);
        let sub_j = sub_j_rows.to_dense();

        let sub_q_basis = row_space_basis(&sub_j);
        let sub_rank = sub_q_basis.len();
        total_rank += sub_rank;

        for local_i in 0..sub_n {
            let proj_sq: f64 = sub_q_basis.iter().map(|q| q[local_i] * q[local_i]).sum();
            if (1.0 - proj_sq).abs() < 1e-3 {
                let v = sub.vars[local_i];
                if let Some(&g_idx) = var_to_global_idx.get(&v) {
                    global_var_constrained[g_idx] = true;
                }
            }
        }

        if sub_rank > 0 && sub_m > 0 {
            let mut u = vec![0.0_f64; sub_m * sub_rank];
            for (r, row_entries) in sub_j_rows.rows.iter().enumerate() {
                let u_offset = r * sub_rank;
                for (l, q) in sub_q_basis.iter().enumerate() {
                    let mut sum = 0.0;
                    for &(col, val) in row_entries {
                        sum += val * q[col];
                    }
                    u[u_offset + l] = sum;
                }
            }

            let (y, h_diag) = column_orthonormal_basis(sub_m, &u, sub_rank);

            let mut constraint_rows: std::collections::BTreeMap<ConstraintId, Vec<usize>> =
                std::collections::BTreeMap::new();
            for (row_idx, owner) in sub.residual_owners.iter().enumerate() {
                if let Some(cid) = owner.constraint_id() {
                    constraint_rows.entry(cid).or_default().push(row_idx);
                }
            }

            for &cid in &sub.constraint_ids {
                if let Some(record) = sketch.constraints.get(&cid) {
                    if !record.is_active || !record.is_driving {
                        continue;
                    }
                    let res_mag = res_map.get(&cid).copied().unwrap_or(0.0);
                    let dependent = constraint_rows
                        .get(&cid)
                        .is_some_and(|rows| is_row_set_redundant(&y, &h_diag, sub_rank, rows));
                    if !dependent {
                        continue;
                    }
                    if res_mag > RESIDUAL_TOL * 100.0 {
                        dependent_unsatisfied.push(cid);
                    } else {
                        redundant_constraints.push(cid);
                    }
                }
            }
        }
    }

    // `partition` gathers the constraints with no free variable under them
    // into one subsystem of their own: it has nothing to move, so it
    // carries nothing.
    let carried: std::collections::BTreeSet<ConstraintId> = subsystems
        .iter()
        .filter(|sub| sub.n_vars() > 0)
        .flat_map(|sub| sub.constraint_ids.iter().copied())
        .collect();
    dependent_unsatisfied.extend(unsatisfied_immovable(sketch, &res_map, &carried));

    SolverSystemAnalysis {
        vars: sys.vars,
        residual_owners: sys.residual_owners,
        rank: total_rank,
        var_constrained: global_var_constrained,
        redundant_constraints,
        dependent_unsatisfied,
    }
}
