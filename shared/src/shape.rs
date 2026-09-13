//! Regular polygons with unit edge length, positioned by their centroid.
use crate::Placement;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Triangle,
    #[default]
    Square,
    Pentagon,
    Hexagon,
}
impl Shape {
    pub const ALL: [Self; 4] = [Self::Triangle, Self::Square, Self::Pentagon, Self::Hexagon];
    pub fn is_square(&self) -> bool {
        *self == Self::Square
    }
    pub fn sides(self) -> usize {
        match self {
            Self::Triangle => 3,
            Self::Square => 4,
            Self::Pentagon => 5,
            Self::Hexagon => 6,
        }
    }
    pub fn from_sides(n: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.sides() == n as usize)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Triangle => "triangle",
            Self::Square => "square",
            Self::Pentagon => "pentagon",
            Self::Hexagon => "hexagon",
        }
    }
    pub fn plural(self) -> &'static str {
        match self {
            Self::Triangle => "triangles",
            Self::Square => "squares",
            Self::Pentagon => "pentagons",
            Self::Hexagon => "hexagons",
        }
    }
    pub fn area(self) -> f64 {
        if self.is_square() {
            return 1.0;
        }
        let n = self.sides() as f64;
        n / (4.0 * (std::f64::consts::PI / n).tan())
    }
    pub fn radius(self) -> f64 {
        0.5 / (std::f64::consts::PI / self.sides() as f64).sin()
    }
    pub fn min_side(self) -> f64 {
        self.area().sqrt()
    }
    pub fn vertices(self, p: &Placement) -> Vec<(f64, f64)> {
        if self.is_square() {
            return p.corners().to_vec();
        }
        let r = self.radius();
        let (sin, cos) = p.theta.sin_cos();
        (0..self.sides())
            .map(|i| {
                let a = std::f64::consts::FRAC_PI_2
                    + std::f64::consts::TAU * i as f64 / self.sides() as f64;
                (
                    p.cx + r * (a.cos() * cos - a.sin() * sin),
                    p.cy + r * (a.cos() * sin + a.sin() * cos),
                )
            })
            .collect()
    }
}
impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
impl std::str::FromStr for Shape {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|x| x.name() == s || x.plural() == s)
            .ok_or_else(|| "Unknown shape".into())
    }
}

/// Minimum translation depth and its direction from A toward B. This uses
/// directional interval distances, including when one projection contains another.
pub fn separation(shape: Shape, a: &Placement, b: &Placement) -> (f64, (f64, f64)) {
    let va = shape.vertices(a);
    let vb = shape.vertices(b);
    let mut best = (f64::INFINITY, (1.0, 0.0));
    for pts in [&va, &vb] {
        for i in 0..pts.len() {
            let p = pts[i];
            let q = pts[(i + 1) % pts.len()];
            let len = (q.0 - p.0).hypot(q.1 - p.1);
            let axis = (-(q.1 - p.1) / len, (q.0 - p.0) / len);
            let project = |v: &Vec<(f64, f64)>| {
                v.iter()
                    .map(|p| p.0 * axis.0 + p.1 * axis.1)
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                        (lo.min(p), hi.max(p))
                    })
            };
            let (al, ah) = project(&va);
            let (bl, bh) = project(&vb);
            let (d, sign) = if ah - bl <= bh - al {
                (ah - bl, 1.0)
            } else {
                (bh - al, -1.0)
            };
            if d < best.0 {
                best = (d, (axis.0 * sign, axis.1 * sign));
            }
        }
    }
    best
}
pub fn protrusion(shape: Shape, p: &Placement, side: f64) -> f64 {
    shape.vertices(p).into_iter().fold(0.0, |d, (x, y)| {
        d.max(-x).max(-y).max(x - side).max(y - side)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unit_edges_and_area_survive_huge_rotations() {
        for shape in Shape::ALL {
            for theta in [0.0, 0.37, 1e300, -1e300] {
                let p = Placement {
                    cx: 0.0,
                    cy: 0.0,
                    theta,
                };
                let v = shape.vertices(&p);
                let mut area = 0.0;
                for i in 0..v.len() {
                    let a = v[i];
                    let b = v[(i + 1) % v.len()];
                    assert!(((b.0 - a.0).hypot(b.1 - a.1) - 1.0).abs() < 1e-12);
                    area += a.0 * b.1 - a.1 * b.0;
                }
                assert!((area.abs() / 2.0 - shape.area()).abs() < 1e-12);
            }
        }
        let impossible = crate::Arrangement {
            shape: Shape::Triangle,
            n: 1,
            side: 0.7,
            squares: vec![Placement {
                cx: 0.682199326626663,
                cy: 0.8222043421254457,
                theta: 1e300,
            }],
        };
        assert!(crate::geometry::validate(&impossible, crate::VALIDATION_TOL).is_err());
    }
    #[test]
    fn a_unit_triangle_fits_below_one() {
        let shape = Shape::Triangle;
        let p = Placement {
            cx: 0.0,
            cy: 0.0,
            theta: std::f64::consts::PI / 12.0,
        };
        let vertices = shape.vertices(&p);
        let lo = vertices
            .iter()
            .fold((f64::INFINITY, f64::INFINITY), |a, p| {
                (a.0.min(p.0), a.1.min(p.1))
            });
        let side = (2.0f64.sqrt() + 6.0f64.sqrt()) / 4.0;
        let a = crate::Arrangement {
            shape,
            n: 1,
            side,
            squares: vec![Placement {
                cx: -lo.0,
                cy: -lo.1,
                ..p
            }],
        };
        assert!(crate::geometry::validate(&a, crate::VALIDATION_TOL).is_ok());
        let code = crate::board::encode(&a, &[]);
        assert_eq!(crate::board::decode(&code, 1).unwrap().arrangement, a);
    }
}
