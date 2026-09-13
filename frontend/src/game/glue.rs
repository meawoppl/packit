//! Picking glue targets on the play screen: square corners, edge midpoints,
//! edges, and the container walls.

use physics::{Body, Feature, Glue};

/// Where a feature sits in world coordinates, for picking and highlighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Point((f64, f64)),
    Segment((f64, f64), (f64, f64)),
}

impl Anchor {
    /// The point of the anchor nearest `p`.
    fn nearest(self, p: (f64, f64)) -> (f64, f64) {
        match self {
            Anchor::Point(q) => q,
            Anchor::Segment(a, b) => {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let t =
                    (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (a.0 + t * dx, a.1 + t * dy)
            }
        }
    }

    fn distance(self, p: (f64, f64)) -> f64 {
        let q = self.nearest(p);
        (p.0 - q.0).hypot(p.1 - q.1)
    }

    fn center(self) -> (f64, f64) {
        match self {
            Anchor::Point(q) => q,
            Anchor::Segment(a, b) => ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
        }
    }
}

/// Every feature of `n` squares plus the four walls.
pub fn features(shape: shared::Shape, n: usize) -> impl Iterator<Item = Feature> {
    (0..n)
        .flat_map(move |square| {
            (0..shape.sides() as u8).flat_map(move |k| {
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
pub fn anchor(shape: shared::Shape, bodies: &[Body], side: f64, f: Feature) -> Option<Anchor> {
    if !shape.is_square() {
        if let Some(i) = f.square() {
            let b = bodies.get(i)?;
            let pts = shape.vertices(&shared::Placement {
                cx: b.x as f64,
                cy: b.y as f64,
                theta: b.theta as f64,
            });
            let index = match f {
                Feature::Corner { corner, .. } => corner,
                Feature::Edge { edge, .. } | Feature::Midpoint { edge, .. } => edge,
                _ => unreachable!(),
            } as usize;
            let a = *pts.get(index)?;
            let z = pts[(index + 1) % pts.len()];
            return Some(match f {
                Feature::Corner { .. } => Anchor::Point(a),
                Feature::Midpoint { .. } => Anchor::Point(((a.0 + z.0) / 2.0, (a.1 + z.1) / 2.0)),
                _ => Anchor::Segment(a, z),
            });
        }
    }
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

/// Largest head start corners and midpoints get over the edges they lie on,
/// in world units. Every point of an edge is within a quarter unit of a
/// corner or midpoint, so this must stay well below 0.25 for the middle of
/// each edge-half to remain an edge at any reach.
const POINT_SNAP: f64 = 0.1;
/// Largest handicap for walls, just enough to lose ties with a square edge
/// lying along them.
const WALL_YIELD: f64 = 0.05;

/// The feature under `p` within `reach`, among those `allow` accepts.
/// Nearest wins, except that corners and midpoints win within a bounded
/// snap distance and walls yield to a square edge lying along them.
pub fn pick(
    shape: shared::Shape,
    bodies: &[Body],
    side: f64,
    p: (f64, f64),
    reach: f64,
    allow: impl Fn(Feature) -> bool,
) -> Option<Feature> {
    let bias = |f: Feature| match f {
        Feature::Corner { .. } | Feature::Midpoint { .. } => (reach / 2.0).min(POINT_SNAP),
        Feature::Edge { .. } => 0.0,
        Feature::Wall(_) => -(reach / 4.0).min(WALL_YIELD),
    };
    features(shape, bodies.len())
        .filter(|f| allow(*f))
        .filter_map(|f| Some((f, anchor(shape, bodies, side, f)?.distance(p))))
        .filter(|(_, d)| *d <= reach)
        .min_by(|(fa, da), (fb, db)| (da - bias(*fa)).total_cmp(&(db - bias(*fb))))
        .map(|(f, _)| f)
}

/// The two ends of a glue's drawn link: each feature's center, except that
/// a wall end sits on the wall where it is nearest the other end.
pub fn link(
    shape: shared::Shape,
    bodies: &[Body],
    side: f64,
    g: Glue,
) -> Option<((f64, f64), (f64, f64))> {
    let (a, b) = (
        anchor(shape, bodies, side, g.a)?,
        anchor(shape, bodies, side, g.b)?,
    );
    Some(match (g.a, g.b) {
        (Feature::Wall(_), _) => (a.nearest(b.center()), b.center()),
        (_, Feature::Wall(_)) => (a.center(), b.nearest(a.center())),
        _ => (a.center(), b.center()),
    })
}

/// Index of the glue whose link midpoint is nearest `p`, within `reach`.
pub fn glue_at(
    shape: shared::Shape,
    bodies: &[Body],
    side: f64,
    glues: &[Glue],
    p: (f64, f64),
    reach: f64,
) -> Option<usize> {
    glues
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            let (a, b) = link(shape, bodies, side, *g)?;
            let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            Some((i, (p.0 - mid.0).hypot(p.1 - mid.1)))
        })
        .filter(|(_, d)| *d <= reach)
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
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
        let point = |f| match anchor(shared::Shape::Square, &bodies, 4.0, f) {
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
        let Some(Anchor::Segment(a, b)) = anchor(
            shared::Shape::Square,
            &bodies,
            4.0,
            Feature::Edge { square: 0, edge: 0 },
        ) else {
            panic!("edge is a segment");
        };
        assert!(close(a, (1.5, 0.5)) && close(b, (1.5, 1.5)), "{a:?} {b:?}");
        assert_eq!(
            anchor(shared::Shape::Square, &bodies, 4.0, Feature::Wall(3)),
            Some(Anchor::Segment((0.0, 4.0), (4.0, 4.0)))
        );
        assert_eq!(
            anchor(
                shared::Shape::Square,
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
        let at = |p| pick(shared::Shape::Square, &bodies, 4.0, p, 0.12, |_| true);
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
    fn edges_stay_pickable_at_a_wide_reach() {
        // A fingertip on a small board: reach is over a quarter unit, so
        // every edge point is within reach of a corner or midpoint. At 0.6
        // (18 px on a 30 px square) even half the reach spans the whole gap
        // between a corner and a midpoint.
        let bodies = [body(0.5, 1.5, 0.0)];
        for reach in [0.3, 0.6] {
            let at = |p| pick(shared::Shape::Square, &bodies, 4.0, p, reach, |_| true);
            assert_eq!(
                at((1.02, 1.25)),
                Some(Feature::Edge { square: 0, edge: 0 }),
                "reach {reach}"
            );
            assert_eq!(
                at((1.02, 1.55)),
                Some(Feature::Midpoint { square: 0, edge: 0 }),
                "reach {reach}"
            );
            assert_eq!(
                at((1.03, 1.97)),
                Some(Feature::Corner {
                    square: 0,
                    corner: 3
                }),
                "reach {reach}"
            );
            // The square's left edge lies along the left wall; the edge wins
            // there, and the wall beyond the square.
            assert_eq!(
                at((0.02, 1.25)),
                Some(Feature::Edge { square: 0, edge: 2 }),
                "reach {reach}"
            );
            assert_eq!(at((0.02, 3.5)), Some(Feature::Wall(0)), "reach {reach}");
        }
    }

    #[test]
    fn rotation_moves_the_features() {
        let bodies = [body(1.0, 1.0, std::f32::consts::FRAC_PI_2)];
        // Local +x now faces world +y.
        assert_eq!(
            pick(shared::Shape::Square, &bodies, 4.0, (1.0, 1.5), 0.1, |_| {
                true
            }),
            Some(Feature::Midpoint { square: 0, edge: 0 })
        );
    }

    #[test]
    fn the_second_pick_skips_the_first_square() {
        let bodies = [body(0.5, 0.5, 0.0), body(1.5, 0.5, 0.0)];
        let first = Feature::Edge { square: 0, edge: 0 };
        let second = pick(shared::Shape::Square, &bodies, 2.0, (1.0, 0.25), 0.1, |f| {
            compatible(first, f)
        });
        assert_eq!(second, Some(Feature::Edge { square: 1, edge: 2 }));
        assert!(!compatible(Feature::Wall(0), Feature::Wall(2)));
        assert!(compatible(Feature::Wall(0), first));
    }

    #[test]
    fn links_join_feature_centers_and_meet_walls_squarely() {
        let bodies = [body(0.5, 0.5, 0.0), body(1.5, 0.5, 0.0)];
        let edges = Glue {
            a: Feature::Edge { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        };
        assert_eq!(
            link(shared::Shape::Square, &bodies, 2.0, edges),
            Some(((1.0, 0.5), (1.0, 0.5)))
        );
        let wall = Glue {
            a: Feature::Wall(3),
            b: Feature::Corner {
                square: 1,
                corner: 3,
            },
        };
        assert_eq!(
            link(shared::Shape::Square, &bodies, 2.0, wall),
            Some(((2.0, 2.0), (2.0, 1.0)))
        );

        let glues = [edges, wall];
        assert_eq!(
            glue_at(
                shared::Shape::Square,
                &bodies,
                2.0,
                &glues,
                (1.0, 0.55),
                0.1
            ),
            Some(0)
        );
        assert_eq!(
            glue_at(shared::Shape::Square, &bodies, 2.0, &glues, (2.0, 1.5), 0.1),
            Some(1)
        );
        assert_eq!(
            glue_at(shared::Shape::Square, &bodies, 2.0, &glues, (0.2, 1.8), 0.1),
            None
        );
    }
}
