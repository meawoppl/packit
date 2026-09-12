//! Local packing refinement and polynomial contact reconstruction.
//!
//! Contact branches are selected numerically, then represented as polynomials
//! in (x_i,y_i,c_i,s_i,L), with c_i²+s_i²=1. Solving one branch is not a proof
//! of global optimality. Reports retain residuals and never claim certification.
use serde::{Deserialize, Serialize};
use shared::geometry::{tighten, validate};
pub use shared::{Arrangement, Placement};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Term {
    pub coefficient: f64,
    pub variables: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Equation {
    pub label: String,
    pub terms: Vec<Term>,
}
impl Equation {
    pub fn evaluate(&self, x: &[f64]) -> f64 {
        self.terms
            .iter()
            .map(|t| t.coefficient * t.variables.iter().map(|&i| x[i]).product::<f64>())
            .sum()
    }
    fn gradient(&self, x: &[f64]) -> Vec<f64> {
        let mut g = vec![0.0; x.len()];
        for t in &self.terms {
            for (k, &v) in t.variables.iter().enumerate() {
                g[v] += t.coefficient
                    * t.variables
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != k)
                        .map(|(_, &i)| x[i])
                        .product::<f64>();
            }
        }
        g
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactSystem {
    pub variables: Vec<String>,
    pub values: Vec<f64>,
    /// Distance in the contact graph from a wall. None means a floating component.
    pub layers: Vec<Option<usize>>,
    pub equations: Vec<Equation>,
    pub max_residual: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveReport {
    pub arrangement: Arrangement,
    pub valid: bool,
    pub max_violation: f64,
    pub iterations: usize,
    pub contacts: ContactSystem,
    /// Recognized expression, only when a feasible side matches a simple value.
    /// This is numerical recognition, not an exact certificate.
    pub candidate_expression: Option<String>,
    pub status: String,
}
fn radius(p: &Placement, n: (f64, f64)) -> f64 {
    let (s, c) = p.theta.sin_cos();
    0.5 * ((c * n.0 + s * n.1).abs() + (-s * n.0 + c * n.1).abs())
}
fn separation(a: &Placement, b: &Placement) -> (f64, (f64, f64), usize) {
    let mut best = (f64::NEG_INFINITY, (1.0, 0.0), 0);
    for (k, t) in [
        a.theta,
        a.theta + std::f64::consts::FRAC_PI_2,
        b.theta,
        b.theta + std::f64::consts::FRAC_PI_2,
    ]
    .into_iter()
    .enumerate()
    {
        let (s, c) = t.sin_cos();
        let d = (b.cx - a.cx) * c + (b.cy - a.cy) * s;
        let sign = if d >= 0.0 { 1.0 } else { -1.0 };
        let gap = d.abs() - radius(a, (c, s)) - radius(b, (c, s));
        if gap > best.0 {
            best = (gap, (c * sign, s * sign), k);
        }
    }
    best
}
pub fn max_violation(a: &Arrangement) -> f64 {
    let mut v: f64 = 0.0;
    for (i, p) in a.squares.iter().enumerate() {
        v = v.max(shared::geometry::protrusion(p, a.side));
        for q in a.squares.iter().skip(i + 1) {
            v = v.max(-separation(p, q).0);
        }
    }
    v
}
fn term(coefficient: f64, variables: Vec<usize>) -> Term {
    Term {
        coefficient,
        variables,
    }
}
// Symbolic corner: x + u*c/2 - v*s/2; y + u*s/2 + v*c/2.
fn corner(i: usize, u: f64, v: f64, dim: usize) -> Vec<Term> {
    if dim == 0 {
        vec![
            term(1.0, vec![4 * i]),
            term(u * 0.5, vec![4 * i + 2]),
            term(-v * 0.5, vec![4 * i + 3]),
        ]
    } else {
        vec![
            term(1.0, vec![4 * i + 1]),
            term(v * 0.5, vec![4 * i + 2]),
            term(u * 0.5, vec![4 * i + 3]),
        ]
    }
}
fn support(p: &Placement, n: (f64, f64)) -> (f64, f64) {
    let (s, c) = p.theta.sin_cos();
    (
        if c * n.0 + s * n.1 >= 0.0 { 1.0 } else { -1.0 },
        if -s * n.0 + c * n.1 >= 0.0 { 1.0 } else { -1.0 },
    )
}
pub fn contact_system(a: &Arrangement, tolerance: f64) -> ContactSystem {
    let count = a.squares.len();
    let side = 4 * count;
    let mut equations = Vec::new();
    let mut layers = vec![None; count];
    let mut edges = Vec::new();
    let mut variables = Vec::new();
    let mut values = Vec::new();
    for (i, p) in a.squares.iter().enumerate() {
        variables.extend([
            format!("x{i}"),
            format!("y{i}"),
            format!("c{i}"),
            format!("s{i}"),
        ]);
        values.extend([p.cx, p.cy, p.theta.cos(), p.theta.sin()]);
        for (dim, dir) in [(0, -1.0), (1, -1.0), (0, 1.0), (1, 1.0)] {
            let n = if dim == 0 { (dir, 0.0) } else { (0.0, dir) };
            let (u, v) = support(p, n);
            let mut ts = corner(i, u, v, dim);
            if dir > 0.0 {
                ts.push(term(-1.0, vec![side]));
            }
            let q = p.corners();
            let wall = if dir < 0.0 {
                q.iter()
                    .map(|p| if dim == 0 { p.0 } else { p.1 })
                    .fold(f64::INFINITY, f64::min)
            } else {
                a.side
                    - q.iter()
                        .map(|p| if dim == 0 { p.0 } else { p.1 })
                        .fold(f64::NEG_INFINITY, f64::max)
            };
            if wall.abs() < tolerance {
                layers[i] = Some(0);
                equations.push(Equation {
                    label: format!("wall {i} {dim} {dir}"),
                    terms: ts,
                });
            }
        }
    }
    variables.push("L".into());
    values.push(a.side);
    for i in 0..count {
        for j in i + 1..count {
            let (gap, n, k) = separation(&a.squares[i], &a.squares[j]);
            if gap.abs() > tolerance {
                continue;
            }
            edges.push((i, j));
            let owner = if k < 2 { i } else { j };
            let perpendicular = k % 2 == 1;
            let (u, v) = support(&a.squares[i], n);
            let (r, s) = support(&a.squares[j], (-n.0, -n.1));
            let mut terms = Vec::new();
            for dim in 0..2 {
                let axis = if perpendicular {
                    if dim == 0 {
                        (4 * owner + 3, -1.0)
                    } else {
                        (4 * owner + 2, 1.0)
                    }
                } else if dim == 0 {
                    (4 * owner + 2, 1.0)
                } else {
                    (4 * owner + 3, 1.0)
                };
                for (sign, ts) in [(1.0, corner(j, r, s, dim)), (-1.0, corner(i, u, v, dim))] {
                    for mut t in ts {
                        t.coefficient *= sign * axis.1;
                        t.variables.push(axis.0);
                        terms.push(t);
                    }
                }
            }
            equations.push(Equation {
                label: format!("contact {i} {j}"),
                terms,
            });
        }
    }
    for depth in 0..count {
        for &(i, j) in &edges {
            if layers[i] == Some(depth) && layers[j].is_none() {
                layers[j] = Some(depth + 1);
            }
            if layers[j] == Some(depth) && layers[i].is_none() {
                layers[i] = Some(depth + 1);
            }
        }
    }
    for i in 0..count {
        equations.push(Equation {
            label: format!("rotation {i}"),
            terms: vec![
                term(1.0, vec![4 * i + 2, 4 * i + 2]),
                term(1.0, vec![4 * i + 3, 4 * i + 3]),
                term(-1.0, vec![]),
            ],
        });
    }
    let max_residual = equations
        .iter()
        .map(|e| e.evaluate(&values).abs())
        .fold(0.0, f64::max);
    ContactSystem {
        variables,
        values,
        layers,
        equations,
        max_residual,
    }
}
/// Project the polynomial contact equations, ordered wall-first, to a local root.
/// The root is accepted only after checking *all* inequalities independently.
fn polish(a: &Arrangement) -> Option<Arrangement> {
    let system = contact_system(a, 1e-5);
    let mut x = system.values;
    for _ in 0..120 {
        let mut max: f64 = 0.0;
        for eq in &system.equations {
            let r = eq.evaluate(&x);
            max = max.max(r.abs());
            let g = eq.gradient(&x);
            let norm = g.iter().map(|v| v * v).sum::<f64>();
            if norm > 1e-20 {
                for (v, d) in x.iter_mut().zip(g) {
                    *v -= 0.7 * r * d / norm;
                }
            }
        }
        if max < 1e-12 {
            break;
        }
    }
    let squares = (0..a.squares.len())
        .map(|i| Placement {
            cx: x[4 * i],
            cy: x[4 * i + 1],
            theta: x[4 * i + 3].atan2(x[4 * i + 2]),
        })
        .collect();
    let candidate = Arrangement {
        n: a.n,
        side: x[x.len() - 1],
        squares,
    };
    if candidate.side.is_finite() && candidate.side > 0.0 && validate(&candidate, 1e-10).is_ok() {
        Some(candidate)
    } else {
        None
    }
}
pub fn refine(input: &Arrangement) -> Result<SolveReport, String> {
    if input.n == 0 || input.n > shared::MAX_N || input.squares.len() != input.n as usize {
        return Err("Invalid square count".into());
    }
    if !input.side.is_finite()
        || input.side <= 0.0
        || input.side > 1000.0
        || input.squares.iter().any(|p| {
            !p.cx.is_finite()
                || !p.cy.is_finite()
                || !p.theta.is_finite()
                || p.cx.abs() > 1000.0
                || p.cy.abs() > 1000.0
        })
    {
        return Err("Coordinates must be finite and within 1000 units".into());
    }
    let mut a = input.clone();
    let mut iterations = 0;
    // Resolve slight bounce penetration from outer edges inward. Keep orientations
    // so the player's contact topology is retained, rather than replacing their layout.
    let layers = contact_system(&a, 0.08).layers;
    let mut order: Vec<usize> = (0..a.squares.len()).collect();
    order.sort_by_key(|&i| layers[i].unwrap_or(usize::MAX));
    for iteration in 0..1500 {
        iterations = iteration + 1;
        for &i in &order {
            let h = radius(&a.squares[i], (1.0, 0.0));
            if a.side < 2.0 * h {
                a.side = 2.0 * h;
            }
            a.squares[i].cx = a.squares[i].cx.clamp(h, a.side - h);
            a.squares[i].cy = a.squares[i].cy.clamp(h, a.side - h);
            for j in i + 1..a.squares.len() {
                let (gap, n, _) = separation(&a.squares[i], &a.squares[j]);
                if gap < 0.0 {
                    let d = (-gap + 1e-12) * 0.5;
                    a.squares[i].cx -= n.0 * d;
                    a.squares[i].cy -= n.1 * d;
                    a.squares[j].cx += n.0 * d;
                    a.squares[j].cy += n.1 * d;
                }
            }
        }
        let violation = max_violation(&a);
        if violation < 1e-11 {
            break;
        }
        // If jammed, minimally grow the container to obtain an honest feasible score.
        if iteration > 0 && iteration % 150 == 0 {
            let factor = 1.0 + (violation / a.side).min(0.002);
            a.side *= factor;
            for p in &mut a.squares {
                p.cx *= factor;
                p.cy *= factor;
            }
        }
    }
    a = tighten(&a.squares);
    if let Some(p) = polish(&a) {
        if p.side <= a.side + 1e-8 {
            a = p;
        }
    }
    // Roundoff-safe tiny expansion separates contacts without changing unit size.
    let gap = max_violation(&a);
    if gap < 1e-7 {
        let factor = 1.0 + 2e-10;
        for p in &mut a.squares {
            p.cx *= factor;
            p.cy *= factor;
        }
        a.side *= factor;
    }
    let valid = validate(&a, shared::VALIDATION_TOL).is_ok();
    let side = a.side;
    let candidate_expression = if valid && (side - side.round()).abs() < 1e-8 {
        Some(format!("{} (numerically recognized)", side.round()))
    } else if valid && (side - (side.floor() + std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-8 {
        Some(format!(
            "{} + 1/sqrt(2) (numerically recognized)",
            side.floor()
        ))
    } else {
        None
    };
    let contacts = contact_system(&a, 1e-6);
    Ok(SolveReport{max_violation:max_violation(&a),arrangement:a,valid,iterations,contacts,candidate_expression,status:if valid{"Numerically feasible local packing; polynomial contacts are not a proof of optimality."}else{"Refinement did not reach a feasible packing; increase the container or rearrange squares."}.into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn grid() -> Arrangement {
        Arrangement {
            n: 4,
            side: 2.0,
            squares: vec![
                Placement {
                    cx: 0.5,
                    cy: 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: 1.5,
                    cy: 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: 0.5,
                    cy: 1.5,
                    theta: 0.0,
                },
                Placement {
                    cx: 1.5,
                    cy: 1.5,
                    theta: 0.0,
                },
            ],
        }
    }
    #[test]
    fn grid_has_polynomial_root() {
        let s = contact_system(&grid(), 1e-6);
        assert!(s.max_residual < 1e-12);
        assert_eq!(s.layers, vec![Some(0); 4]);
        assert!(s.equations.iter().any(|e| e.label.starts_with("contact")));
    }
    #[test]
    fn refinement_preserves_optimal_grid() {
        let r = refine(&grid()).unwrap();
        assert!(r.valid);
        assert!((r.arrangement.side - 2.0).abs() < 1e-8);
    }
    #[test]
    fn repairs_bounce_overlap() {
        let mut a = grid();
        a.squares[1].cx -= 0.001;
        let r = refine(&a).unwrap();
        assert!(r.valid, "{}", r.max_violation);
        assert!(r.arrangement.side < 2.01);
    }
    #[test]
    fn floating_component_has_no_wall_layer() {
        let mut a = grid();
        a.side = 8.0;
        for p in &mut a.squares {
            p.cx += 2.0;
            p.cy += 2.0;
        }
        assert!(contact_system(&a, 1e-6).layers.iter().all(Option::is_none));
    }
    #[test]
    fn rejects_nonfinite() {
        let mut a = grid();
        a.side = f64::NAN;
        assert!(refine(&a).is_err());
    }
    #[test]
    fn five_square_polynomials_vanish() {
        let s = 2.0 + std::f64::consts::FRAC_1_SQRT_2;
        let a = Arrangement {
            n: 5,
            side: s,
            squares: vec![
                Placement {
                    cx: 0.5,
                    cy: 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: s - 0.5,
                    cy: 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: 0.5,
                    cy: s - 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: s - 0.5,
                    cy: s - 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: s / 2.0,
                    cy: s / 2.0,
                    theta: std::f64::consts::FRAC_PI_4,
                },
            ],
        };
        let r = refine(&a).unwrap();
        assert!(r.valid);
        assert!(r.contacts.max_residual < 1e-8);
        assert!(r.candidate_expression.is_some());
    }
}
