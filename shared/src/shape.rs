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
    pub fn play_path(self, container: Shape, n: u32) -> String {
        if !container.is_square() {
            format!("/play/{self}/{container}/{n}")
        } else if self.is_square() {
            format!("/play/{n}")
        } else {
            format!("/play/{self}/{n}")
        }
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
    /// Regular container of edge length `side`, centered at `(side/2, side/2)`.
    /// Square coordinates and its historical wall order remain unchanged.
    pub fn container_vertices(self, side: f64) -> Vec<(f64, f64)> {
        self.vertices(&Placement {
            cx: 0.0,
            cy: 0.0,
            theta: 0.0,
        })
        .into_iter()
        .map(|(x, y)| (side * (x + 0.5), side * (y + 0.5)))
        .collect()
    }
    pub fn apothem(self) -> f64 {
        0.5 / (std::f64::consts::PI / self.sides() as f64).tan()
    }
    pub fn extent(self) -> f64 {
        if self.is_square() {
            1.0
        } else {
            2.0 * self.radius()
        }
    }
    pub fn area_bound(self, piece: Shape, n: u32) -> f64 {
        (n as f64 * piece.area() / self.area()).sqrt()
    }
    /// Wall endpoints in the glue order: left/bottom/right/top for squares,
    /// otherwise consecutive counterclockwise vertices starting at the top.
    pub fn walls(self, side: f64) -> Vec<Wall> {
        let v = self.container_vertices(side);
        let order: Vec<_> = if self.is_square() {
            vec![3, 0, 1, 2]
        } else {
            (0..v.len()).collect()
        };
        order
            .into_iter()
            .map(|i| {
                let a = v[i];
                let b = v[(i + 1) % v.len()];
                let len = (b.0 - a.0).hypot(b.1 - a.1);
                let normal = (-(b.1 - a.1) / len, (b.0 - a.0) / len);
                Wall {
                    a,
                    b,
                    normal,
                    limit: a.0 * normal.0 + a.1 * normal.1,
                }
            })
            .collect()
    }
    pub fn protrusion(self, piece: Shape, p: &Placement, side: f64) -> f64 {
        let vertices = piece.vertices(p);
        self.walls(side)
            .iter()
            .flat_map(|wall| vertices.iter().map(move |&p| wall.depth(p)))
            .fold(0.0, f64::max)
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
            container: crate::Shape::Square,
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
            container: crate::Shape::Square,
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

/// An inward unit normal and its halfplane `dot(normal, point) >= limit`.
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub normal: (f64, f64),
    pub limit: f64,
}
impl Wall {
    pub fn depth(&self, p: (f64, f64)) -> f64 {
        self.limit - self.normal.0 * p.0 - self.normal.1 * p.1
    }
}

#[cfg(test)]
mod container_tests {
    use super::*;
    #[test]
    fn every_container_has_unit_edges_and_correct_halfplanes() {
        for shape in Shape::ALL {
            let walls = shape.walls(3.7);
            assert_eq!(walls.len(), shape.sides());
            for wall in walls {
                assert!(((wall.b.0 - wall.a.0).hypot(wall.b.1 - wall.a.1) - 3.7).abs() < 1e-12);
                assert!(wall.depth((1.85, 1.85)) < 0.0);
                assert!(wall.depth(wall.a).abs() < 1e-12);
                let outside = (
                    wall.a.0 - wall.normal.0 * 0.01,
                    wall.a.1 - wall.normal.1 * 0.01,
                );
                assert!((wall.depth(outside) - 0.01).abs() < 1e-12);
            }
        }
    }
    #[test]
    fn same_shape_fits_exactly_and_each_wall_detects_escape() {
        for container in Shape::ALL {
            let a = crate::Arrangement {
                container,
                shape: container,
                n: 1,
                side: 1.0,
                squares: vec![Placement {
                    cx: 0.5,
                    cy: 0.5,
                    theta: 0.0,
                }],
            };
            assert!(crate::geometry::validate(&a, crate::VALIDATION_TOL).is_ok());
            for wall in container.walls(1.0) {
                let mut outside = a.clone();
                outside.squares[0].cx -= wall.normal.0 * 0.01;
                outside.squares[0].cy -= wall.normal.1 * 0.01;
                assert!(crate::geometry::validate(&outside, crate::VALIDATION_TOL).is_err());
            }
        }
    }
    #[test]
    fn all_sixteen_games_round_trip_with_their_last_wall_and_feature() {
        for container in Shape::ALL {
            for shape in Shape::ALL {
                let a = crate::Arrangement {
                    container,
                    shape,
                    n: 1,
                    side: 5.0,
                    squares: vec![Placement {
                        cx: 2.5,
                        cy: 2.5,
                        theta: 0.3,
                    }],
                };
                assert!(crate::geometry::validate(&a, crate::VALIDATION_TOL).is_ok());
                let glue = crate::glue::Glue {
                    a: crate::glue::Feature::Corner {
                        square: 0,
                        corner: shape.sides() as u8 - 1,
                    },
                    b: crate::glue::Feature::Wall(container.sides() as u8 - 1),
                };
                let encoded = crate::board::encode(&a, &[glue]);
                let b = crate::board::decode(&encoded, 1).unwrap();
                assert_eq!(a, b.arrangement);
                assert_eq!(b.glues, vec![glue]);
                let invalid = crate::glue::Glue {
                    b: crate::glue::Feature::Wall(container.sides() as u8),
                    ..glue
                };
                assert!(crate::board::decode(&crate::board::encode(&a, &[invalid]), 1).is_err());
            }
        }
    }
}
