//! Picking glue targets on the play screen: square corners, edge midpoints,
//! edges, and the container walls.

use physics::{Body, Feature};

/// Where a feature sits in world coordinates, for picking and highlighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Point((f64, f64)),
    Segment((f64, f64), (f64, f64)),
}

impl Anchor {
    fn distance(self, p: (f64, f64)) -> f64 {
        match self {
            Anchor::Point(q) => (p.0 - q.0).hypot(p.1 - q.1),
            Anchor::Segment(a, b) => {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let t =
                    (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
            }
        }
    }
}

/// Every feature of `n` squares plus the four walls.
pub fn features(n: usize) -> impl Iterator<Item = Feature> {
    (0..n)
        .flat_map(|square| {
            (0..4).flat_map(move |k| {
                [
                    Feature::Corner { square, corner: k },
                    Feature::Midpoint { square, edge: k },
                    Feature::Edge { square, edge: k },
                ]
            })
        })
        .chain((0..4).map(Feature::Wall))
}

/// Two features can be glued unless they share a square (or are both walls).
pub fn compatible(a: Feature, b: Feature) -> bool {
    a.square() != b.square()
}

/// World position of `f`. Edge and midpoint `k` are the local +x, +y, -x,
/// -y faces; corner bit 0 is +x and bit 1 is +y; walls are left, bottom,
/// right, top.
pub fn anchor(bodies: &[Body], side: f64, f: Feature) -> Option<Anchor> {
    let local = |square: usize, u: f64, v: f64| {
        bodies.get(square).map(|b| {
            let (s, c) = (b.theta as f64).sin_cos();
            (b.x as f64 + u * c - v * s, b.y as f64 + u * s + v * c)
        })
    };
    let face = |edge: u8| match edge {
        0 => (0.5, 0.0),
        1 => (0.0, 0.5),
        2 => (-0.5, 0.0),
        _ => (0.0, -0.5),
    };
    Some(match f {
        Feature::Corner { square, corner } => {
            let sign = |bit| if corner & bit != 0 { 0.5 } else { -0.5 };
            Anchor::Point(local(square, sign(1), sign(2))?)
        }
        Feature::Midpoint { square, edge } => {
            let (u, v) = face(edge);
            Anchor::Point(local(square, u, v)?)
        }
        Feature::Edge { square, edge } => {
            let (u, v) = face(edge);
            // The face runs perpendicular to its outward normal.
            Anchor::Segment(local(square, u - v, v - u)?, local(square, u + v, v + u)?)
        }
        Feature::Wall(k) => {
            let (lo, hi) = match k {
                0 => ((0.0, 0.0), (0.0, side)),
                1 => ((0.0, 0.0), (side, 0.0)),
                2 => ((side, 0.0), (side, side)),
                _ => ((0.0, side), (side, side)),
            };
            Anchor::Segment(lo, hi)
        }
    })
}

/// The feature under `p` within `reach`, among those `allow` accepts.
/// Corners and midpoints win over the edges they lie on, and square edges
/// over walls, so every target stays reachable.
pub fn pick(
    bodies: &[Body],
    side: f64,
    p: (f64, f64),
    reach: f64,
    allow: impl Fn(Feature) -> bool,
) -> Option<Feature> {
    let rank = |f: Feature| match f {
        Feature::Corner { .. } | Feature::Midpoint { .. } => 0,
        Feature::Edge { .. } => 1,
        Feature::Wall(_) => 2,
    };
    features(bodies.len())
        .filter(|f| allow(*f))
        .filter_map(|f| Some((f, anchor(bodies, side, f)?.distance(p))))
        .filter(|(_, d)| *d <= reach)
        .min_by(|(fa, da), (fb, db)| rank(*fa).cmp(&rank(*fb)).then(da.total_cmp(db)))
        .map(|(f, _)| f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: f32, y: f32, theta: f32) -> Body {
        Body {
            x,
            y,
            theta,
            ..Body::default()
        }
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).hypot(a.1 - b.1) < 1e-6
    }

    #[test]
    fn anchors_follow_the_documented_numbering() {
        let bodies = [body(1.0, 1.0, 0.0)];
        let point = |f| match anchor(&bodies, 4.0, f) {
            Some(Anchor::Point(p)) => p,
            other => panic!("{other:?}"),
        };
        assert!(close(
            point(Feature::Corner {
                square: 0,
                corner: 0
            }),
            (0.5, 0.5)
        ));
        assert!(close(
            point(Feature::Corner {
                square: 0,
                corner: 1
            }),
            (1.5, 0.5)
        ));
        assert!(close(
            point(Feature::Corner {
                square: 0,
                corner: 2
            }),
            (0.5, 1.5)
        ));
        assert!(close(
            point(Feature::Midpoint { square: 0, edge: 1 }),
            (1.0, 1.5)
        ));
        let Some(Anchor::Segment(a, b)) =
            anchor(&bodies, 4.0, Feature::Edge { square: 0, edge: 0 })
        else {
            panic!("edge is a segment");
        };
        assert!(close(a, (1.5, 0.5)) && close(b, (1.5, 1.5)), "{a:?} {b:?}");
        assert_eq!(
            anchor(&bodies, 4.0, Feature::Wall(3)),
            Some(Anchor::Segment((0.0, 4.0), (4.0, 4.0)))
        );
        assert_eq!(
            anchor(
                &bodies,
                4.0,
                Feature::Corner {
                    square: 1,
                    corner: 0
                }
            ),
            None
        );
    }

    #[test]
    fn points_beat_edges_and_edges_beat_walls() {
        let bodies = [body(1.0, 1.0, 0.0)];
        let at = |p| pick(&bodies, 4.0, p, 0.12, |_| true);
        assert_eq!(
            at((1.55, 1.52)),
            Some(Feature::Corner {
                square: 0,
                corner: 3
            })
        );
        assert_eq!(
            at((1.52, 1.02)),
            Some(Feature::Midpoint { square: 0, edge: 0 })
        );
        assert_eq!(at((1.52, 1.25)), Some(Feature::Edge { square: 0, edge: 0 }));
        assert_eq!(at((0.05, 3.0)), Some(Feature::Wall(0)));
        assert_eq!(at((2.5, 2.5)), None);
    }

    #[test]
    fn rotation_moves_the_features() {
        let bodies = [body(1.0, 1.0, std::f32::consts::FRAC_PI_2)];
        // Local +x now faces world +y.
        assert_eq!(
            pick(&bodies, 4.0, (1.0, 1.5), 0.1, |_| true),
            Some(Feature::Midpoint { square: 0, edge: 0 })
        );
    }

    #[test]
    fn the_second_pick_skips_the_first_square() {
        let bodies = [body(0.5, 0.5, 0.0), body(1.5, 0.5, 0.0)];
        let first = Feature::Edge { square: 0, edge: 0 };
        let second = pick(&bodies, 2.0, (1.0, 0.25), 0.1, |f| compatible(first, f));
        assert_eq!(second, Some(Feature::Edge { square: 1, edge: 2 }));
        assert!(!compatible(Feature::Wall(0), Feature::Wall(2)));
        assert!(compatible(Feature::Wall(0), first));
    }
}
