//! B-Spline curve evaluations and basis function derivatives (Piegl & Tiller, "The NURBS Book").

/// Generates a standard clamped (open uniform) knot vector for `num_points` control points and `degree`.
pub fn default_clamped_knots(num_points: usize, degree: usize) -> Vec<f64> {
    if num_points == 0 {
        return Vec::new();
    }
    let p = degree.min(num_points.saturating_sub(1));
    let n = num_points - 1;
    let m = n + p + 1;
    let mut knots = Vec::with_capacity(m + 1);

    knots.extend(std::iter::repeat_n(0.0, p + 1));
    let num_interior = n.saturating_sub(p);
    for j in 1..=num_interior {
        knots.push(j as f64 / (num_interior + 1) as f64);
    }
    knots.extend(std::iter::repeat_n(1.0, p + 1));
    knots
}

/// Evaluates B-spline basis functions $N_{i, p}(u)$ for all control points $i \in 0 \dots n$.
pub fn evaluate_bspline_basis(degree: usize, knots: &[f64], u: f64) -> Vec<f64> {
    let (n0, _, _) = evaluate_bspline_basis_derivs(degree, knots, u);
    n0
}

/// Evaluates B-spline basis functions and their 1st and 2nd derivatives: $(N_{i, p}(u), N'_{i, p}(u), N''_{i, p}(u))$.
pub fn evaluate_bspline_basis_derivs(
    degree: usize,
    knots: &[f64],
    u: f64,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    if knots.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let m = knots.len() - 1;
    let p = degree;
    if m < p + 1 {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let num_basis = m - p; // number of control points = m - p
    let u_clamped = u.clamp(knots[0], knots[m]);

    // Table of basis functions for orders 0 to p: n_table[k][i] = N_{i, k}(u)
    let mut n_table = vec![vec![0.0; m]; p + 1];

    for i in 0..m {
        let span_match = if u_clamped >= knots[m] {
            // At the right boundary, the last non-empty knot span contains u
            u_clamped >= knots[i] && u_clamped <= knots[i + 1] && knots[i] < knots[i + 1]
        } else {
            u_clamped >= knots[i] && u_clamped < knots[i + 1]
        };
        if span_match {
            n_table[0][i] = 1.0;
        }
    }

    for k in 1..=p {
        for i in 0..(m - k) {
            let mut left = 0.0;
            let d1 = knots[i + k] - knots[i];
            if d1 > 1e-15 {
                left = ((u_clamped - knots[i]) / d1) * n_table[k - 1][i];
            }

            let mut right = 0.0;
            let d2 = knots[i + k + 1] - knots[i + 1];
            if d2 > 1e-15 {
                right = ((knots[i + k + 1] - u_clamped) / d2) * n_table[k - 1][i + 1];
            }

            n_table[k][i] = left + right;
        }
    }

    let mut n0 = vec![0.0; num_basis];
    n0[..num_basis].copy_from_slice(&n_table[p][..num_basis]);

    // 1st derivatives: N'_{i, p} = p * (N_{i, p-1}/(u_{i+p} - u_i) - N_{i+1, p-1}/(u_{i+p+1} - u_{i+1}))
    let mut n1 = vec![0.0; num_basis];
    if p >= 1 {
        for i in 0..num_basis {
            let mut term1 = 0.0;
            let d1 = knots[i + p] - knots[i];
            if d1 > 1e-15 {
                term1 = (p as f64 / d1) * n_table[p - 1][i];
            }

            let mut term2 = 0.0;
            let d2 = knots[i + p + 1] - knots[i + 1];
            if d2 > 1e-15 {
                term2 = (p as f64 / d2) * n_table[p - 1][i + 1];
            }

            n1[i] = term1 - term2;
        }
    }

    // 2nd derivatives
    let mut n2 = vec![0.0; num_basis];
    if p >= 2 {
        for i in 0..num_basis {
            // Derivative of term1 and term2
            let mut d_term1 = 0.0;
            let d1 = knots[i + p] - knots[i];
            if d1 > 1e-15 {
                let mut sub1 = 0.0;
                let sd1 = knots[i + p - 1] - knots[i];
                if sd1 > 1e-15 {
                    sub1 = ((p - 1) as f64 / sd1) * n_table[p - 2][i];
                }
                let mut sub2 = 0.0;
                let sd2 = knots[i + p] - knots[i + 1];
                if sd2 > 1e-15 {
                    sub2 = ((p - 1) as f64 / sd2) * n_table[p - 2][i + 1];
                }
                d_term1 = (p as f64 / d1) * (sub1 - sub2);
            }

            let mut d_term2 = 0.0;
            let d2 = knots[i + p + 1] - knots[i + 1];
            if d2 > 1e-15 {
                let mut sub1 = 0.0;
                let sd1 = knots[i + p] - knots[i + 1];
                if sd1 > 1e-15 {
                    sub1 = ((p - 1) as f64 / sd1) * n_table[p - 2][i + 1];
                }
                let mut sub2 = 0.0;
                let sd2 = knots[i + p + 1] - knots[i + 2];
                if sd2 > 1e-15 {
                    sub2 = ((p - 1) as f64 / sd2) * n_table[p - 2][i + 2];
                }
                d_term2 = (p as f64 / d2) * (sub1 - sub2);
            }

            n2[i] = d_term1 - d_term2;
        }
    }

    (n0, n1, n2)
}

/// Evaluates a 2D point $C(u)$ on a B-spline curve.
pub fn evaluate_bspline_point(
    control_points: &[[f64; 2]],
    degree: usize,
    knots: &[f64],
    u: f64,
) -> [f64; 2] {
    let basis = evaluate_bspline_basis(degree, knots, u);
    let mut pt = [0.0, 0.0];
    for (i, cp) in control_points.iter().enumerate() {
        if i < basis.len() {
            pt[0] += basis[i] * cp[0];
            pt[1] += basis[i] * cp[1];
        }
    }
    pt
}

/// Evaluates curve point $C(u)$, 1st derivative $C'(u)$, and 2nd derivative $C''(u)$.
pub fn evaluate_bspline_derivatives(
    control_points: &[[f64; 2]],
    degree: usize,
    knots: &[f64],
    u: f64,
) -> ([f64; 2], [f64; 2], [f64; 2]) {
    let (n0, n1, n2) = evaluate_bspline_basis_derivs(degree, knots, u);
    let mut pt = [0.0, 0.0];
    let mut d1 = [0.0, 0.0];
    let mut d2 = [0.0, 0.0];

    for (i, cp) in control_points.iter().enumerate() {
        if i < n0.len() {
            pt[0] += n0[i] * cp[0];
            pt[1] += n0[i] * cp[1];
            d1[0] += n1[i] * cp[0];
            d1[1] += n1[i] * cp[1];
            d2[0] += n2[i] * cp[0];
            d2[1] += n2[i] * cp[1];
        }
    }
    (pt, d1, d2)
}

/// Computes the signed curvature $\kappa(u) = \frac{x' y'' - y' x''}{(x'^2 + y'^2)^{3/2}}$.
pub fn evaluate_bspline_curvature(
    control_points: &[[f64; 2]],
    degree: usize,
    knots: &[f64],
    u: f64,
) -> f64 {
    let (_, d1, d2) = evaluate_bspline_derivatives(control_points, degree, knots, u);
    let speed2 = d1[0] * d1[0] + d1[1] * d1[1];
    let speed = speed2.sqrt();
    if speed < 1e-12 {
        return 0.0;
    }
    let cross = d1[0] * d2[1] - d1[1] * d2[0];
    cross / (speed * speed2)
}

/// Samples the B-spline curve at `num_samples` uniform intervals in $u \in [0, 1]$.
pub fn sample_bspline(
    control_points: &[[f64; 2]],
    degree: usize,
    knots: &[f64],
    num_samples: usize,
) -> Vec<[f64; 2]> {
    let samples = num_samples.max(2);
    let mut pts = Vec::with_capacity(samples);
    let u_min = knots.first().copied().unwrap_or(0.0);
    let u_max = knots.last().copied().unwrap_or(1.0);
    for i in 0..samples {
        let t = i as f64 / (samples - 1) as f64;
        let u = u_min + t * (u_max - u_min);
        pts.push(evaluate_bspline_point(control_points, degree, knots, u));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bspline_partition_of_unity() {
        let knots = default_clamped_knots(4, 3);
        for i in 0..=10 {
            let u = i as f64 / 10.0;
            let basis = evaluate_bspline_basis(3, &knots, u);
            let sum: f64 = basis.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-12,
                "Partition of unity failed at u={u}: sum={sum}"
            );
        }
    }

    #[test]
    fn test_bspline_endpoint_interpolation() {
        let cps = [[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]];
        let knots = default_clamped_knots(cps.len(), 3);
        let p_start = evaluate_bspline_point(&cps, 3, &knots, 0.0);
        let p_end = evaluate_bspline_point(&cps, 3, &knots, 1.0);
        assert!((p_start[0] - cps[0][0]).abs() < 1e-12);
        assert!((p_start[1] - cps[0][1]).abs() < 1e-12);
        assert!((p_end[0] - cps[3][0]).abs() < 1e-12);
        assert!((p_end[1] - cps[3][1]).abs() < 1e-12);
    }
}
