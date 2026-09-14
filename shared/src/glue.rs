//! Glue constraints between features of the packing: square edges, corners,
//! edge midpoints, and the container walls.

pub const MAX_GLUES: usize = 4096;

/// Edge/midpoint indices are local +u, +v, -u, -v. Corner bit 0 selects
/// +u (otherwise -u), bit 1 selects +v (otherwise -v), not cyclic order.
/// Walls are left, bottom, right, top. Square indices follow body order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    Edge { square: usize, edge: u8 },
    Corner { square: usize, corner: u8 },
    Midpoint { square: usize, edge: u8 },
    Wall(u8),
}

impl Feature {
    pub fn square(self) -> Option<usize> {
        match self {
            Self::Edge { square, .. }
            | Self::Corner { square, .. }
            | Self::Midpoint { square, .. } => Some(square),
            Self::Wall(_) => None,
        }
    }

    fn valid(self, n: usize, sides: u8, walls: u8) -> bool {
        match self {
            Self::Edge { square, edge } | Self::Midpoint { square, edge } => {
                square < n && edge < sides
            }
            Self::Corner { square, corner } => square < n && corner < sides,
            Self::Wall(w) => w < walls,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glue {
    pub a: Feature,
    pub b: Feature,
}

/// Rules every glue set for `n` squares must meet: at most [`MAX_GLUES`],
/// valid features on different objects, and no pair glued twice in either
/// order.
pub fn check(glues: &[Glue], n: usize) -> Result<(), String> {
    check_for(glues, n, crate::Shape::Square)
}
pub fn check_for(glues: &[Glue], n: usize, shape: crate::Shape) -> Result<(), String> {
    check_in(glues, n, shape, crate::Shape::Square)
}
pub fn check_in(
    glues: &[Glue],
    n: usize,
    shape: crate::Shape,
    container: crate::Shape,
) -> Result<(), String> {
    if glues.len() > MAX_GLUES {
        return Err("Too many glue constraints".into());
    }
    for (i, g) in glues.iter().enumerate() {
        if !g.a.valid(n, shape.sides() as u8, container.sides() as u8)
            || !g.b.valid(n, shape.sides() as u8, container.sides() as u8)
            || g.a.square() == g.b.square()
        {
            return Err("Glue requires valid features on different objects".into());
        }
        if glues[..i]
            .iter()
            .any(|h| h == g || (h.a == g.b && h.b == g.a))
        {
            return Err("Duplicate glue constraint".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVALID: &str = "Glue requires valid features on different objects";

    #[test]
    fn accepts_every_feature_kind_between_objects() {
        let glues = [
            Glue {
                a: Feature::Edge { square: 0, edge: 3 },
                b: Feature::Corner {
                    square: 1,
                    corner: 3,
                },
            },
            Glue {
                a: Feature::Midpoint { square: 1, edge: 0 },
                b: Feature::Wall(3),
            },
        ];
        assert_eq!(check(&glues, 2), Ok(()));
        assert_eq!(check(&[], 0), Ok(()));
    }

    #[test]
    fn rejects_invalid_features_and_self_glue() {
        let a = Feature::Midpoint { square: 0, edge: 0 };
        for bad in [
            Glue { a, b: a },
            Glue {
                a,
                b: Feature::Edge { square: 0, edge: 1 },
            },
            Glue {
                a: Feature::Wall(0),
                b: Feature::Wall(1),
            },
            Glue {
                a: Feature::Wall(4),
                b: a,
            },
            Glue {
                a,
                b: Feature::Edge { square: 2, edge: 0 },
            },
            Glue {
                a,
                b: Feature::Corner {
                    square: 1,
                    corner: 4,
                },
            },
            Glue {
                a,
                b: Feature::Midpoint { square: 1, edge: 4 },
            },
        ] {
            assert_eq!(check(&[bad], 2), Err(INVALID.into()), "{bad:?}");
        }
    }

    #[test]
    fn rejects_duplicates_in_either_order_and_too_many() {
        let g = Glue {
            a: Feature::Midpoint { square: 0, edge: 0 },
            b: Feature::Wall(2),
        };
        let reversed = Glue { a: g.b, b: g.a };
        for glues in [[g, g], [g, reversed]] {
            assert_eq!(check(&glues, 1), Err("Duplicate glue constraint".into()));
        }
        let many: Vec<Glue> = (0..=MAX_GLUES)
            .map(|i| Glue {
                a: Feature::Edge {
                    square: i % 100,
                    edge: 0,
                },
                b: Feature::Wall(0),
            })
            .collect();
        assert_eq!(check(&many, 100), Err("Too many glue constraints".into()));
    }
}
