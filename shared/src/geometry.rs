//! Exact-ish geometry for unit squares inside a square container.
//!
//! Conventions shared by every crate: a square is a *unit* square described by
//! its center `(cx, cy)` and rotation `theta` in radians. The container is the
//! axis-aligned square `[0, side] x [0, side]` with the origin at bottom-left.

use crate::{Arrangement, Placement};
use serde::{Deserialize, Serialize};

/// Why an arrangement failed validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Violation {
    /// `arrangement.n` does not match the number of squares supplied.
    CountMismatch { expected: u32, actual: usize },
    /// A coordinate or the side length is NaN or infinite.
    NonFinite { index: Option<usize> },
    /// The container side is zero or negative.
    NonPositiveSide { side: f64 },
    /// The tolerance passed to [`validate`] is negative or not finite.
    InvalidTolerance { tol: f64 },
    /// A square pokes outside the container by `depth`.
    OutOfBounds { index: usize, depth: f64 },
    /// Two squares overlap with penetration `depth` along the best separating axis.
    Overlap { a: usize, b: usize, depth: f64 },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::CountMismatch { expected, actual } => {
                write!(f, "expected {expected} squares, got {actual}")
            }
            Violation::NonFinite { index: Some(i) } => write!(f, "square {i} is not finite"),
            Violation::NonFinite { index: None } => write!(f, "side length is not finite"),
            Violation::NonPositiveSide { side } => write!(f, "side length {side} is not positive"),
            Violation::InvalidTolerance { tol } => write!(f, "tolerance {tol} is invalid"),
            Violation::OutOfBounds { index, depth } => {
                write!(f, "square {index} is outside the container by {depth:.3e}")
            }
            Violation::Overlap { a, b, depth } => {
                write!(f, "squares {a} and {b} overlap by {depth:.3e}")
            }
        }
    }
}

impl std::error::Error for Violation {}

impl Placement {
    /// The four corners, counter-clockwise.
    pub fn corners(&self) -> [(f64, f64); 4] {
        let (s, c) = self.theta.sin_cos();
        let (ux, uy) = (0.5 * c, 0.5 * s);
        let (vx, vy) = (-0.5 * s, 0.5 * c);
        [
            (self.cx - ux - vx, self.cy - uy - vy),
            (self.cx + ux - vx, self.cy + uy - vy),
            (self.cx + ux + vx, self.cy + uy + vy),
            (self.cx - ux + vx, self.cy - uy + vy),
        ]
    }

    /// The two edge normals (unit vectors).
    fn axes(&self) -> [(f64, f64); 2] {
        let (s, c) = self.theta.sin_cos();
        [(c, s), (-s, c)]
    }

    fn is_finite(&self) -> bool {
        self.cx.is_finite() && self.cy.is_finite() && self.theta.is_finite()
    }
}

/// Penetration depth of two unit squares along their best separating axis.
///
/// Returns a value `<= 0` when the squares are disjoint or merely touching.
pub fn penetration(a: &Placement, b: &Placement) -> f64 {
    separation(a, b).0
}

/// Penetration depth of two unit squares and the unit axis it's measured
/// along, pointing from `a` toward `b`.
fn separation(a: &Placement, b: &Placement) -> (f64, (f64, f64)) {
    let ca = a.corners();
    let cb = b.corners();
    let mut best = (f64::INFINITY, (1.0, 0.0));
    for (ax, ay) in a.axes().into_iter().chain(b.axes()) {
        let project = |pts: &[(f64, f64); 4]| {
            pts.iter()
                .map(|(x, y)| x * ax + y * ay)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p), hi.max(p))
                })
        };
        let (alo, ahi) = project(&ca);
        let (blo, bhi) = project(&cb);
        let depth = ahi.min(bhi) - alo.max(blo);
        if depth < best.0 {
            best = (depth, (ax, ay));
        }
    }
    let (depth, (ax, ay)) = best;
    if (b.cx - a.cx) * ax + (b.cy - a.cy) * ay < 0.0 {
        (depth, (-ax, -ay))
    } else {
        (depth, (ax, ay))
    }
}

/// How far a square extends outside `[0, side]^2`; `<= 0` when contained.
pub fn protrusion(p: &Placement, side: f64) -> f64 {
    p.corners()
        .iter()
        .flat_map(|&(x, y)| [-x, -y, x - side, y - side])
        .fold(f64::NEG_INFINITY, f64::max)
}

/// The worst violation in `arr`: its largest protrusion from the container
/// or pairwise penetration, or 0 when every square is inside and disjoint.
pub fn worst_violation(arr: &Arrangement) -> f64 {
    let squares = &arr.squares;
    let protrusions = squares.iter().map(|p| protrusion(p, arr.side));
    let overlaps = squares
        .iter()
        .enumerate()
        .flat_map(|(i, a)| squares[i + 1..].iter().map(move |b| penetration(a, b)));
    protrusions.chain(overlaps).fold(0.0, f64::max)
}

/// Check that every square lies in the container and no two squares overlap,
/// allowing violations up to `tol`.
pub fn validate(arr: &Arrangement, tol: f64) -> Result<(), Violation> {
    if !(tol.is_finite() && tol >= 0.0) {
        return Err(Violation::InvalidTolerance { tol });
    }
    if arr.squares.len() != arr.n as usize {
        return Err(Violation::CountMismatch {
            expected: arr.n,
            actual: arr.squares.len(),
        });
    }
    if !arr.side.is_finite() {
        return Err(Violation::NonFinite { index: None });
    }
    if arr.side <= 0.0 {
        return Err(Violation::NonPositiveSide { side: arr.side });
    }
    for (i, p) in arr.squares.iter().enumerate() {
        if !p.is_finite() {
            return Err(Violation::NonFinite { index: Some(i) });
        }
        let depth = protrusion(p, arr.side);
        if depth > tol {
            return Err(Violation::OutOfBounds { index: i, depth });
        }
    }
    for (i, a) in arr.squares.iter().enumerate() {
        for (j, b) in arr.squares.iter().enumerate().skip(i + 1) {
            let depth = penetration(a, b);
            if depth > tol {
                return Err(Violation::Overlap { a: i, b: j, depth });
            }
        }
    }
    Ok(())
}

/// Side of the smallest axis-aligned square containing all placements.
pub fn bounding_side(squares: &[Placement]) -> f64 {
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in squares.iter().flat_map(Placement::corners) {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x1 - x0).max(y1 - y0).max(0.0)
}

/// Translate placements so their bounding box starts at the origin and return
/// the resulting tight arrangement.
pub fn tighten(squares: &[Placement]) -> Arrangement {
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    for (x, y) in squares.iter().flat_map(Placement::corners) {
        x0 = x0.min(x);
        y0 = y0.min(y);
    }
    let squares: Vec<Placement> = squares
        .iter()
        .map(|p| Placement {
            cx: p.cx - x0,
            cy: p.cy - y0,
            theta: p.theta,
        })
        .collect();
    Arrangement {
        n: squares.len() as u32,
        side: bounding_side(&squares),
        squares,
    }
}

/// Certify a settled `arr` where it stands, in its own box: it passes
/// `validate` at `tol`, or does once squares still overlapping or past a
/// wall are nudged apart, no square moving farther than `bound`. Returns the
/// certified arrangement and the farthest any center moved (0 when valid as
/// it stood), or `None` past the bound. The side and rotations never change.
pub fn certify(arr: &Arrangement, tol: f64, bound: f64) -> Option<(Arrangement, f64)> {
    let mut nudged = arr.clone();
    for _ in 0..64 {
        let moved = farthest_move(arr, &nudged);
        if moved > bound {
            return None;
        }
        if validate(&nudged, tol).is_ok() {
            return Some((nudged, moved));
        }
        separate(&mut nudged);
    }
    None
}

/// Largest distance any center moves between two arrangements of the same
/// squares.
fn farthest_move(a: &Arrangement, b: &Arrangement) -> f64 {
    a.squares
        .iter()
        .zip(&b.squares)
        .map(|(p, q)| (p.cx - q.cx).hypot(p.cy - q.cy))
        .fold(0.0, f64::max)
}

/// One pass pushing each square back inside the walls, then each
/// overlapping pair apart along its separating axis, a hair past touching.
fn separate(arr: &mut Arrangement) {
    const HAIR: f64 = 1e-12;
    let side = arr.side;
    for p in &mut arr.squares {
        let corners = p.corners();
        let low =
            |axis: fn(&(f64, f64)) -> f64| corners.iter().map(axis).fold(f64::INFINITY, f64::min);
        let high = |axis: fn(&(f64, f64)) -> f64| {
            corners.iter().map(axis).fold(f64::NEG_INFINITY, f64::max)
        };
        let shift = |lo: f64, hi: f64| {
            if lo < 0.0 {
                HAIR - lo
            } else if hi > side {
                side - hi - HAIR
            } else {
                0.0
            }
        };
        let dx = shift(low(|c| c.0), high(|c| c.0));
        let dy = shift(low(|c| c.1), high(|c| c.1));
        p.cx += dx;
        p.cy += dy;
    }
    for i in 0..arr.squares.len() {
        for j in i + 1..arr.squares.len() {
            let (depth, (ax, ay)) = separation(&arr.squares[i], &arr.squares[j]);
            if depth > 0.0 {
                let push = depth / 2.0 + HAIR;
                arr.squares[i].cx -= ax * push;
                arr.squares[i].cy -= ay * push;
                arr.squares[j].cx += ax * push;
                arr.squares[j].cy += ay * push;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_4;

    fn sq(cx: f64, cy: f64, theta: f64) -> Placement {
        Placement { cx, cy, theta }
    }

    fn pair(gap: f64) -> Arrangement {
        Arrangement {
            n: 2,
            side: 2.0,
            squares: vec![sq(0.5, 0.5, 0.0), sq(1.5 + gap, 0.5, 0.0)],
        }
    }

    /// Certify `arr` and check the result is valid, in the same box and
    /// rotations, with every center within the reported move of where it was.
    fn certified(arr: &Arrangement) -> (Arrangement, f64) {
        let (out, moved) = certify(arr, 1e-9, 1e-4).expect("certified");
        validate(&out, 1e-9).unwrap();
        assert_eq!(out.side, arr.side, "the box never changes");
        assert!(moved <= 1e-4, "{moved}");
        for (a, b) in arr.squares.iter().zip(&out.squares) {
            assert_eq!(a.theta, b.theta, "rotations never change");
            assert!((a.cx - b.cx).hypot(a.cy - b.cy) <= moved);
        }
        (out, moved)
    }

    #[test]
    fn certify_keeps_valid_packings_exactly() {
        let loose = Arrangement {
            n: 2,
            side: 2.5,
            squares: vec![sq(0.75, 0.75, 0.0), sq(1.75, 0.75, 0.0)],
        };
        let huge = Arrangement {
            n: 2,
            side: 1000.0,
            squares: vec![sq(0.5, 0.5, 0.0), sq(500.0, 500.0, 0.3)],
        };
        for arr in [pair(0.0), loose, huge] {
            assert_eq!(certify(&arr, 1e-9, 1e-4), Some((arr, 0.0)));
        }
    }

    #[test]
    fn certify_nudges_a_hair_of_overlap_apart() {
        for gap in [-1e-5, -6e-5] {
            let (_, moved) = certified(&pair(gap));
            assert!(moved > 0.0 && moved < -gap, "{moved}");
        }
    }

    #[test]
    fn certify_pushes_a_square_back_inside_the_wall() {
        let mut arr = pair(0.0);
        arr.squares[1].cx -= 0.5;
        arr.squares[1].cy = 1.5 + 1e-5;
        certified(&arr);
    }

    #[test]
    fn certify_separates_a_rotated_corner_contact() {
        let arr = Arrangement {
            n: 2,
            side: 3.0,
            squares: vec![
                sq(0.5, 0.5, 0.0),
                // Its left corner pokes 1e-5 into the first square's right side.
                sq(1.0 + FRAC_PI_4.sin() - 1e-5, 0.9, FRAC_PI_4),
            ],
        };
        certified(&arr);
    }

    #[test]
    fn certify_refuses_more_than_the_bound() {
        let overlapping = pair(-0.01);
        let before = overlapping.clone();
        assert_eq!(certify(&overlapping, 1e-9, 1e-4), None);
        assert_eq!(
            overlapping, before,
            "a failed certification changes nothing"
        );
        // Wedged between the walls: no nudge within the box can clear it.
        let wedged = Arrangement {
            n: 2,
            side: 2.0 - 1e-5,
            squares: vec![sq(0.5, 0.5, 0.0), sq(1.5 - 1e-5, 0.5, 0.0)],
        };
        assert_eq!(certify(&wedged, 1e-9, 1e-4), None);
    }

    fn grid(k: u32) -> Arrangement {
        let squares = (0..k * k)
            .map(|i| sq((i % k) as f64 + 0.5, (i / k) as f64 + 0.5, 0.0))
            .collect();
        Arrangement {
            n: k * k,
            side: k as f64,
            squares,
        }
    }

    #[test]
    fn perfect_grids_are_valid() {
        for k in 1..=5 {
            assert_eq!(validate(&grid(k), 1e-9), Ok(()));
        }
    }

    #[test]
    fn touching_squares_do_not_overlap() {
        assert!(penetration(&sq(0.5, 0.5, 0.0), &sq(1.5, 0.5, 0.0)).abs() < 1e-12);
    }

    #[test]
    fn overlap_depth_is_measured() {
        let d = penetration(&sq(0.5, 0.5, 0.0), &sq(1.25, 0.5, 0.0));
        assert!((d - 0.25).abs() < 1e-12);
    }

    #[test]
    fn rotated_square_corner_intrusion() {
        // Diamond's corner reaches 1 + sqrt(2)/2 - 0.5 past its center in x.
        let a = sq(0.5, 0.5, 0.0);
        let b = sq(1.0 + std::f64::consts::FRAC_1_SQRT_2 - 0.1, 0.5, FRAC_PI_4);
        let d = penetration(&a, &b);
        assert!((d - 0.1).abs() < 1e-12, "{d}");
    }

    #[test]
    fn detects_overlap_violation() {
        let mut arr = grid(2);
        arr.squares[1].cx -= 0.01;
        match validate(&arr, 1e-9) {
            Err(Violation::Overlap { a: 0, b: 1, depth }) => assert!((depth - 0.01).abs() < 1e-12),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn detects_out_of_bounds() {
        let mut arr = grid(2);
        arr.squares[3].theta = 0.3;
        assert!(matches!(
            validate(&arr, 1e-9),
            Err(Violation::OutOfBounds { index: 3, .. })
        ));
    }

    #[test]
    fn detects_count_and_nan() {
        let mut arr = grid(2);
        arr.n = 5;
        assert!(matches!(
            validate(&arr, 1e-9),
            Err(Violation::CountMismatch { .. })
        ));
        let mut arr = grid(2);
        arr.squares[0].cx = f64::NAN;
        assert!(matches!(
            validate(&arr, 1e-9),
            Err(Violation::NonFinite { index: Some(0) })
        ));
    }

    #[test]
    fn rejects_bad_side_and_tolerance() {
        let empty = Arrangement {
            n: 0,
            side: 0.0,
            squares: vec![],
        };
        assert!(matches!(
            validate(&empty, 1e-9),
            Err(Violation::NonPositiveSide { .. })
        ));
        for tol in [-1e-9, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                validate(&grid(2), tol),
                Err(Violation::InvalidTolerance { .. })
            ));
        }
    }

    #[test]
    fn five_squares_known_packing() {
        // s = 2 + 1/sqrt(2): four corner squares plus a 45-degree center square.
        let s = 2.0 + std::f64::consts::FRAC_1_SQRT_2;
        let arr = Arrangement {
            n: 5,
            side: s,
            squares: vec![
                sq(0.5, 0.5, 0.0),
                sq(s - 0.5, 0.5, 0.0),
                sq(0.5, s - 0.5, 0.0),
                sq(s - 0.5, s - 0.5, 0.0),
                sq(s / 2.0, s / 2.0, FRAC_PI_4),
            ],
        };
        assert_eq!(validate(&arr, 1e-9), Ok(()));
        assert!((bounding_side(&arr.squares) - s).abs() < 1e-12);
    }

    #[test]
    fn worst_violation_covers_overlap_and_walls() {
        let arr = |side, squares| Arrangement {
            n: 2,
            side,
            squares,
        };
        assert_eq!(
            worst_violation(&arr(2.0, vec![sq(0.5, 0.5, 0.0), sq(1.5, 0.5, 0.0)])),
            0.0
        );
        let overlap = worst_violation(&arr(2.0, vec![sq(0.5, 0.5, 0.0), sq(1.3, 0.5, 0.0)]));
        assert!((overlap - 0.2).abs() < 1e-12, "{overlap}");
        let outside = worst_violation(&arr(1.8, vec![sq(0.5, 0.5, 0.0), sq(1.5, 0.5, 0.0)]));
        assert!((outside - 0.2).abs() < 1e-12, "{outside}");
    }

    #[test]
    fn tighten_moves_to_origin() {
        let arr = tighten(&[sq(10.5, -3.5, 0.0), sq(11.5, -3.5, 0.0)]);
        assert_eq!(arr.n, 2);
        assert!((arr.side - 2.0).abs() < 1e-12);
        assert!((arr.squares[0].cx - 0.5).abs() < 1e-12);
        assert!((arr.squares[0].cy - 0.5).abs() < 1e-12);
    }
}
