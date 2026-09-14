//! Explicit feature contacts. Tangential motion is free while segments overlap.
use crate::{Body, Physics};
pub use shared::glue::{Feature, Glue, MAX_GLUES};
impl Physics {
    /// Replace all constraints atomically. This changes input, never body poses.
    pub fn set_glues(&self, glues: &[Glue]) -> Result<(), String> {
        let mut s = self.state.borrow_mut();
        if s.disposed {
            return Err("Simulation disposed".into());
        }
        shared::glue::check_in(glues, s.bodies.len(), s.shape, s.container)?;
        s.cancel_settle();
        s.glues = glues.to_vec();
        Ok(())
    }
    pub fn glues(&self) -> Vec<Glue> {
        self.state.borrow().glues.clone()
    }
}
type V = [f32; 2];
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: V, b: f32) -> V {
    [a[0] * b, a[1] * b]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: V, b: V) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}
fn tangent(n: V) -> V {
    [-n[1], n[0]]
}
#[derive(Clone, Copy)]
struct World {
    p: V,
    n: V,
    half: f32,
    owner: Option<usize>,
}
fn world(f: Feature, bodies: &[Body], side: f32) -> World {
    if let Feature::Wall(w) = f {
        return World {
            p: [
                [0., side / 2.],
                [side / 2., 0.],
                [side, side / 2.],
                [side / 2., side],
            ][w as usize],
            n: [[1., 0.], [0., 1.], [-1., 0.], [0., -1.]][w as usize],
            half: side / 2.,
            owner: None,
        };
    }
    let owner = f.square();
    let b = bodies[owner.unwrap()];
    let (s, c) = b.theta.sin_cos();
    let normals = [[c, s], [-s, c], [-c, -s], [s, -c]];
    let (offset, n, half) = match f {
        Feature::Edge { edge, .. } => (
            mul(normals[edge as usize], 0.5),
            normals[edge as usize],
            0.5,
        ),
        Feature::Midpoint { edge, .. } => (mul(normals[edge as usize], 0.5), [0.; 2], 0.),
        Feature::Corner { corner, .. } => (
            add(
                mul(normals[0], if corner & 1 == 0 { -0.5 } else { 0.5 }),
                mul(normals[1], if corner & 2 == 0 { -0.5 } else { 0.5 }),
            ),
            [0.; 2],
            0.,
        ),
        Feature::Wall(_) => unreachable!(),
    };
    World {
        p: add([b.x, b.y], offset),
        n,
        half,
        owner,
    }
}
fn world_for(
    shape: shared::Shape,
    container: shared::Shape,
    f: Feature,
    bodies: &[Body],
    side: f32,
) -> World {
    if let Feature::Wall(w) = f {
        let wall = container.walls(side as f64)[w as usize];
        return World {
            p: [
                (wall.a.0 + wall.b.0) as f32 / 2.,
                (wall.a.1 + wall.b.1) as f32 / 2.,
            ],
            n: [wall.normal.0 as f32, wall.normal.1 as f32],
            half: side / 2.,
            owner: None,
        };
    }
    if shape.is_square() {
        return world(f, bodies, side);
    }
    let i = f.square().unwrap();
    let b = bodies[i];
    let pts = shape.vertices(&shared::Placement {
        cx: b.x as f64,
        cy: b.y as f64,
        theta: b.theta as f64,
    });
    let idx = match f {
        Feature::Edge { edge, .. } | Feature::Midpoint { edge, .. } => edge,
        Feature::Corner { corner, .. } => corner,
        _ => unreachable!(),
    } as usize;
    let a = pts[idx];
    let z = pts[(idx + 1) % pts.len()];
    let corner = matches!(f, Feature::Corner { .. });
    let line = matches!(f, Feature::Edge { .. });
    World {
        p: if corner {
            [a.0 as f32, a.1 as f32]
        } else {
            [(a.0 + z.0) as f32 / 2., (a.1 + z.1) as f32 / 2.]
        },
        n: if line {
            [(z.1 - a.1) as f32, -(z.0 - a.0) as f32]
        } else {
            [0.; 2]
        },
        half: if line { 0.5 } else { 0. },
        owner: Some(i),
    }
}
fn wall_derivative(container: shared::Shape, p: V, side: f32) -> V {
    if !container.is_square() {
        return [p[0] / side - 0.5, p[1] / side - 0.5];
    }
    [
        if p[0] >= side - 1e-6 { 1. } else { 0. },
        if p[1] >= side - 1e-6 { 1. } else { 0. },
    ]
}
fn velocity(container: shared::Shape, w: World, p: V, bodies: &[Body], side: f32, band: f32) -> V {
    if let Some(i) = w.owner {
        let b = bodies[i];
        add([b.vx, b.vy], mul(tangent(sub(p, [b.x, b.y])), b.omega))
    } else {
        mul(wall_derivative(container, p, side), band)
    }
}
fn omega(w: World, bodies: &[Body]) -> f32 {
    w.owner.map_or(0., |i| bodies[i].omega)
}
fn point_at(w: World, t: V, q: f32) -> V {
    let d = dot(tangent(w.n), t);
    let offset = if d.abs() > 0.1 {
        ((q - dot(w.p, t)) / d).clamp(-w.half, w.half)
    } else {
        0.
    };
    add(w.p, mul(tangent(w.n), offset))
}
fn contact_geometry(mut a: World, mut b: World) -> (World, World, V, V, V, bool, f32) {
    if a.half > 0. && b.half == 0. {
        std::mem::swap(&mut a, &mut b);
    }
    let (mut pa, mut pb) = (a.p, b.p);
    let mut normal = [0.; 2];
    let mut sliding = false;
    let mut angle = 0.;
    if a.half > 0. && b.half > 0. {
        normal = sub(a.n, b.n);
        let len = dot(normal, normal).sqrt();
        normal = if len > 0.001 {
            mul(normal, 1. / len)
        } else {
            a.n
        };
        let t = tangent(normal);
        let ha = a.half * dot(tangent(a.n), t).abs();
        let hb = b.half * dot(tangent(b.n), t).abs();
        let lo = (dot(a.p, t) - ha).max(dot(b.p, t) - hb);
        let hi = (dot(a.p, t) + ha).min(dot(b.p, t) + hb);
        let q = (lo + hi) * 0.5;
        pa = point_at(a, t, q);
        pb = point_at(b, t, q);
        sliding = lo <= hi;
        let sine = cross(a.n, mul(b.n, -1.));
        let cosine = dot(a.n, mul(b.n, -1.));
        angle = if sine.abs() < 1e-6 && cosine < 0. {
            std::f32::consts::PI
        } else {
            sine.atan2(cosine)
        };
    } else if b.half > 0. {
        normal = b.n;
        let t = tangent(b.n);
        let q = dot(sub(a.p, b.p), t);
        pb = add(b.p, mul(t, q.clamp(-b.half, b.half)));
        sliding = q.abs() <= b.half;
    }
    (a, b, pa, pb, normal, sliding, angle)
}

pub(super) fn error_for(
    shape: shared::Shape,
    container: shared::Shape,
    g: Glue,
    bodies: &[Body],
    side: f32,
) -> (f64, f64) {
    let (_, _, pa, pb, normal, sliding, angle) = contact_geometry(
        world_for(shape, container, g.a, bodies, side),
        world_for(shape, container, g.b, bodies, side),
    );
    let delta = sub(pb, pa);
    let distance = if sliding {
        dot(delta, normal).abs()
    } else {
        dot(delta, delta).sqrt()
    };
    (distance as f64, angle.abs() as f64)
}

/// Sum force and angular acceleration per body, plus generalized band reaction.
pub(super) fn forces(
    glues: &[Glue],
    bodies: &[Body],
    side: f32,
    band: f32,
    k: f32,
) -> (Vec<[f32; 3]>, f32) {
    forces_for(
        shared::Shape::Square,
        shared::Shape::Square,
        glues,
        bodies,
        side,
        band,
        k,
    )
}
pub(super) fn forces_for(
    shape: shared::Shape,
    container: shared::Shape,
    glues: &[Glue],
    bodies: &[Body],
    side: f32,
    band: f32,
    k: f32,
) -> (Vec<[f32; 3]>, f32) {
    let mut out = vec![[0.; 3]; bodies.len()];
    let mut reaction = 0.;
    let mut degrees = vec![0u32; bodies.len()];
    for g in glues {
        for f in [g.a, g.b] {
            if let Some(i) = f.square() {
                degrees[i] += 1;
            }
        }
    }
    for g in glues {
        // Bound the total explicit stiffness/damping on a body, including
        // redundant point unions. Apply one symmetric weight to both ends.
        let degree =
            g.a.square()
                .map_or(0, |i| degrees[i])
                .max(g.b.square().map_or(0, |i| degrees[i]));
        let weight = 1.0 / degree.max(1) as f32;
        let (a, b, pa, pb, normal, sliding, angle) = contact_geometry(
            world_for(shape, container, g.a, bodies, side),
            world_for(shape, container, g.b, bodies, side),
        );
        let mut couple = if a.half > 0. && b.half > 0. {
            (k / 24. * angle + 4. * (omega(b, bodies) - omega(a, bodies))).clamp(-12., 12.)
        } else {
            0.
        };
        let delta = sub(pb, pa);
        let speed = sub(
            velocity(container, b, pb, bodies, side, band),
            velocity(container, a, pa, bodies, side, band),
        );
        let mut force = add(mul(delta, k), mul(speed, 18.));
        if sliding {
            force = mul(normal, dot(force, normal));
        }
        let len = dot(force, force).sqrt();
        force = mul(force, (80. / len.max(0.0001)).min(1.) * weight);
        couple *= weight;
        for (w, p, f, c) in [(a, pa, force, couple), (b, pb, mul(force, -1.), -couple)] {
            if let Some(i) = w.owner {
                out[i][0] += f[0];
                out[i][1] += f[1];
                out[i][2] += 6. * (cross(sub(p, [bodies[i].x, bodies[i].y]), f) + c);
            } else {
                reaction += dot(f, wall_derivative(container, p, side));
            }
        }
    }
    (out, reaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_is_atomic_and_input_does_not_edit_revision() {
        let p = Physics::new(2, 4.);
        let g = Glue {
            a: Feature::Midpoint { square: 0, edge: 0 },
            b: Feature::Corner {
                square: 1,
                corner: 0,
            },
        };
        let revision = p.state.borrow().revision;
        p.set_glues(&[g]).unwrap();
        assert_eq!(p.state.borrow().revision, revision);
        for bad in [
            Glue { a: g.a, b: g.a },
            Glue {
                a: Feature::Wall(0),
                b: Feature::Wall(1),
            },
            Glue {
                a: Feature::Wall(4),
                b: g.a,
            },
        ] {
            assert!(p.set_glues(&[bad]).is_err());
            assert_eq!(p.glues(), vec![g]);
        }
        assert!(p.set_glues(&[g, Glue { a: g.b, b: g.a }]).is_err());
        p.set_paused(true);
        assert_eq!(p.glues(), vec![g]);
        p.reset();
        assert!(p.glues().is_empty());
        p.set_glues(&[g]).unwrap();
        p.load(&p.arrangement());
        assert!(p.glues().is_empty());
        p.set_glues(&[g]).unwrap();
        p.dispose();
        assert!(p.glues().is_empty());
    }
    #[test]
    fn edges_slide_and_point_pairs_pull_in_both_dimensions() {
        let bodies = [
            Body {
                x: 1.,
                y: 1.,
                ..Body::default()
            },
            Body {
                x: 2.2,
                y: 1.2,
                ..Body::default()
            },
        ];
        let a = Feature::Edge { square: 0, edge: 0 };
        let b = Feature::Edge { square: 1, edge: 2 };
        let (f, r) = forces(&[Glue { a, b }], &bodies, 4., 0., 900.);
        assert!(f[0][0] > 0.);
        assert_eq!(f[0][1], 0.);
        assert_eq!(f[0][0], -f[1][0]);
        assert_eq!(r, 0.);
        let (f, _) = forces(
            &[Glue {
                a: Feature::Midpoint { square: 0, edge: 0 },
                b: Feature::Midpoint { square: 1, edge: 2 },
            }],
            &bodies,
            4.,
            0.,
            900.,
        );
        assert!(f[0][0] > 0. && f[0][1] > 0.);
    }
    #[test]
    fn glued_edges_close_and_point_line_retains_sliding() {
        let p = Physics::new(2, 5.0);
        p.set_pose(0, 1.5, 2.0, 0.0);
        p.set_pose(1, 3.0, 2.2, 0.0);
        p.set_glues(&[Glue {
            a: Feature::Edge { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        }])
        .unwrap();
        for _ in 0..960 {
            p.state.borrow_mut().cpu_step();
        }
        let b = p.bodies();
        let n = [b[0].theta.cos(), b[0].theta.sin()];
        let d = [b[1].x - b[0].x, b[1].y - b[0].y];
        assert!((dot(d, n) - 1.0).abs() < 0.002, "{b:?}");
        assert!((b[0].theta - b[1].theta).abs() < 0.002);
        assert!(
            dot(d, tangent(n)).abs() > 0.05,
            "glue must not weld midpoints together"
        );
        assert!(p.motion() < 0.004, "{}", p.motion());
        let bodies = [
            Body {
                x: 1.5,
                y: 1.2,
                ..Body::default()
            },
            Body {
                x: 2.7,
                y: 1.4,
                ..Body::default()
            },
        ];
        let g = Glue {
            a: Feature::Midpoint { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        };
        let (f, _) = forces(&[g], &bodies, 5., 0., 900.);
        assert_eq!(f[0][1], 0.0);
        let mut outside = bodies;
        outside[0].y = 3.0;
        let (f, _) = forces(&[g], &outside, 5., 0., 900.);
        assert!(f[0][1] < 0., "outside segment pulls toward endpoint");
    }
    #[test]
    fn wall_glue_has_opposite_band_reaction() {
        let bodies = [Body {
            x: 2.3,
            y: 1.,
            ..Body::default()
        }];
        let (f, r) = forces(
            &[Glue {
                a: Feature::Edge { square: 0, edge: 0 },
                b: Feature::Wall(2),
            }],
            &bodies,
            3.,
            0.,
            900.,
        );
        assert!(f[0][0] > 0.);
        assert_eq!(r, -f[0][0]);
    }
}
