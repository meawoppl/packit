//! Exact linear elimination on contact branches whose orientations are multiples
//! of pi/4. Arithmetic is in Q(sqrt(2)); no floating-point elimination is used.
use crate::{ContactSystem, Placement};
use num_rational::BigRational as Rational;
use num_traits::{ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Debug, PartialEq)]
struct Quadratic {
    a: Rational,
    b: Rational,
}
impl Quadratic {
    fn zero() -> Self {
        Self::rational(0)
    }
    fn rational(n: i64) -> Self {
        Self {
            a: Rational::from_integer(n.into()),
            b: Rational::zero(),
        }
    }
    fn is_zero(&self) -> bool {
        self.a.is_zero() && self.b.is_zero()
    }
    fn approximate(&self) -> Option<f64> {
        Some(self.a.to_f64()? + self.b.to_f64()? * 2.0_f64.sqrt())
    }
    fn expression(&self) -> String {
        if self.b.is_zero() {
            self.a.to_string()
        } else {
            format!("{} + ({})*sqrt(2)", self.a, self.b)
        }
    }
}
impl Add for Quadratic {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            a: self.a + rhs.a,
            b: self.b + rhs.b,
        }
    }
}
impl Neg for Quadratic {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            a: -self.a,
            b: -self.b,
        }
    }
}
impl Sub for Quadratic {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self + (-rhs)
    }
}
impl Mul for Quadratic {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            a: &self.a * &rhs.a + &self.b * &rhs.b * Rational::from_integer(2.into()),
            b: self.a * rhs.b + self.b * rhs.a,
        }
    }
}
impl Div for Quadratic {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let norm = &rhs.a * &rhs.a - &rhs.b * &rhs.b * Rational::from_integer(2.into());
        let product = self
            * Self {
                a: rhs.a,
                b: -rhs.b,
            };
        Self {
            a: product.a / &norm,
            b: product.b / norm,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgebraicCandidate {
    pub side_expression: String,
    /// Rational coefficients in descending degree order.
    pub side_polynomial: Vec<String>,
    pub side_approximation: f64,
    pub assumptions: Vec<String>,
    pub status: String,
}
fn eighth_turn(k: i64) -> (Quadratic, Quadratic) {
    let half_root = Quadratic {
        a: Rational::zero(),
        b: Rational::new(1.into(), 2.into()),
    };
    match k.rem_euclid(8) {
        0 => (Quadratic::rational(1), Quadratic::zero()),
        1 => (half_root.clone(), half_root),
        2 => (Quadratic::zero(), Quadratic::rational(1)),
        3 => (-half_root.clone(), half_root),
        4 => (Quadratic::rational(-1), Quadratic::zero()),
        5 => (-half_root.clone(), -half_root),
        6 => (Quadratic::zero(), Quadratic::rational(-1)),
        _ => (half_root.clone(), -half_root),
    }
}
/// Recover an exact side on a selected pi/4 orientation branch. Rotations are
/// explicit assumptions, never inferred to be mathematically exact from pixels.
pub fn recover(
    system: &ContactSystem,
    squares: &[Placement],
    side: f64,
) -> Option<AlgebraicCandidate> {
    if squares.len() > 25 {
        return None;
    } // Bound exact arithmetic work in the browser.
    let n = squares.len();
    let width = 2 * n + 1;
    let mut angles = Vec::new();
    let mut assumptions = Vec::new();
    for (i, p) in squares.iter().enumerate() {
        let turns = p.theta / std::f64::consts::FRAC_PI_4;
        let rounded = turns.round();
        if (turns - rounded).abs() > 1e-7 || rounded.abs() > 1e6 {
            return None;
        }
        angles.push(eighth_turn(rounded as i64));
        assumptions.push(format!("theta_{i} = {}*pi/4", rounded as i64));
    }
    let mut rows = Vec::new();
    for equation in &system.equations {
        let mut row = vec![Quadratic::zero(); width + 1];
        for term in &equation.terms {
            let mut value = Quadratic {
                a: Rational::from_float(term.coefficient)?,
                b: Rational::zero(),
            };
            let mut variable = None;
            for &v in &term.variables {
                if v == 4 * n {
                    if variable.replace(2 * n).is_some() {
                        return None;
                    }
                } else if v % 4 < 2 {
                    if variable.replace(2 * (v / 4) + v % 4).is_some() {
                        return None;
                    }
                } else {
                    value = value
                        * if v % 4 == 2 {
                            angles[v / 4].0.clone()
                        } else {
                            angles[v / 4].1.clone()
                        };
                }
            }
            let column = variable.unwrap_or(width);
            row[column] = row[column].clone() + value;
        }
        row[width] = -row[width].clone();
        if row.iter().any(|x| !x.is_zero()) {
            rows.push(row);
        }
    }
    // Reduced row echelon form handles floating squares: a unique side may still
    // exist even when some center coordinates are free variables.
    let mut pivot_row = 0;
    let mut side_row = None;
    for column in 0..width {
        let Some(found) = (pivot_row..rows.len()).find(|&r| !rows[r][column].is_zero()) else {
            continue;
        };
        rows.swap(found, pivot_row);
        let divisor = rows[pivot_row][column].clone();
        for x in &mut rows[pivot_row][column..] {
            *x = x.clone() / divisor.clone();
        }
        let pivot = rows[pivot_row].clone();
        for (r, row) in rows.iter_mut().enumerate() {
            if r == pivot_row {
                continue;
            }
            let factor = row[column].clone();
            if factor.is_zero() {
                continue;
            }
            for c in column..=width {
                row[c] = row[c].clone() - factor.clone() * pivot[c].clone();
            }
        }
        if column == width - 1 {
            side_row = Some(pivot_row);
        }
        pivot_row += 1;
    }
    if rows
        .iter()
        .any(|row| row[..width].iter().all(Quadratic::is_zero) && !row[width].is_zero())
    {
        return None;
    }
    let row = &rows[side_row?];
    if row[..width - 1].iter().any(|x| !x.is_zero()) {
        return None;
    }
    let result = &row[width];
    let approximation = result.approximate()?;
    if (approximation - side).abs() > 1e-6 {
        return None;
    }
    let polynomial = if result.b.is_zero() {
        vec!["1".into(), (-result.a.clone()).to_string()]
    } else {
        vec![
            "1".into(),
            (-&result.a * Rational::from_integer(2.into())).to_string(),
            (&result.a * &result.a - &result.b * &result.b * Rational::from_integer(2.into()))
                .to_string(),
        ]
    };
    Some(AlgebraicCandidate{side_expression:result.expression(),side_polynomial:polynomial,side_approximation:approximation,assumptions,status:"Exact elimination on the stated contact/orientation branch; feasibility and global optimality are not certified.".into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn field_division_is_exact() {
        let x = Quadratic {
            a: Rational::from_integer(2.into()),
            b: Rational::new(1.into(), 2.into()),
        };
        assert_eq!(x.clone() / x, Quadratic::rational(1));
    }
}
