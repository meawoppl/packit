//! Convex-polygon CPU contacts. Square games retain their existing CPU/GPU path.
use crate::{glue, Body, State, FIXED_STEP};
fn placement(b: &Body) -> shared::Placement {
    shared::Placement {
        cx: b.x as f64,
        cy: b.y as f64,
        theta: b.theta as f64,
    }
}
fn support(pts: &[(f64, f64)], axis: (f64, f64), max: bool) -> (f64, f64) {
    let sign = if max { 1.0 } else { -1.0 };
    let best = pts
        .iter()
        .map(|p| sign * (p.0 * axis.0 + p.1 * axis.1))
        .fold(f64::NEG_INFINITY, f64::max);
    let mut out = (0.0, 0.0);
    let mut n = 0.0;
    for p in pts {
        if (sign * (p.0 * axis.0 + p.1 * axis.1) - best).abs() < 1e-5 {
            out.0 += p.0;
            out.1 += p.1;
            n += 1.0;
        }
    }
    (out.0 / n, out.1 / n)
}
impl State {
    pub(super) fn polygon_step(&mut self) {
        let dt = FIXED_STEP as f32;
        let p = self.params;
        let shape = self.shape;
        let side = self.side;
        let placements: Vec<_> = self.bodies.iter().map(placement).collect();
        let vertices: Vec<_> = placements.iter().map(|p| shape.vertices(p)).collect();
        let inv_i =
            (6.0 / (shape.radius().powi(2)
                * (2.0 + (std::f64::consts::TAU / shape.sides() as f64).cos()))) as f32;
        let (glues, glue_reaction) = glue::forces_for(
            shape,
            &self.glues,
            &self.bodies,
            side as f32,
            self.band_velocity,
            p.stiffness,
        );
        let mut next = self.bodies.clone();
        let mut reaction = glue_reaction as f64;
        for (i, b) in self.bodies.iter().enumerate() {
            let mut f = [0.0f32; 3];
            for (j, other) in self.bodies.iter().enumerate() {
                if i == j {
                    continue;
                }
                // A circumscribed-circle rejection keeps sparse large boards cheap.
                let distance = (placements[i].cx - placements[j].cx)
                    .hypot(placements[i].cy - placements[j].cy);
                let (depth, axis) = if distance > 2.0 * shape.radius() + 1e-10 {
                    (-1.0, (1.0, 0.0))
                } else {
                    shared::shape::separation(shape, &placements[i.min(j)], &placements[i.max(j)])
                };
                let normal = if i < j { axis } else { (-axis.0, -axis.1) };
                if depth > 0.0 {
                    let a = support(&vertices[i], normal, true);
                    let z = support(&vertices[j], normal, false);
                    let point = ((a.0 + z.0) / 2.0, (a.1 + z.1) / 2.0);
                    let ra = ((point.0 - b.x as f64) as f32, (point.1 - b.y as f64) as f32);
                    let rb = (
                        (point.0 - other.x as f64) as f32,
                        (point.1 - other.y as f64) as f32,
                    );
                    let nx = normal.0 as f32;
                    let ny = normal.1 as f32;
                    let va = (b.vx - b.omega * ra.1, b.vy + b.omega * ra.0);
                    let vb = (other.vx - other.omega * rb.1, other.vy + other.omega * rb.0);
                    let speed = (vb.0 - va.0) * nx + (vb.1 - va.1) * ny;
                    let force = (p.stiffness * depth as f32 - 12.0 * speed).max(0.0);
                    f[0] -= nx * force;
                    f[1] -= ny * force;
                    f[2] -= (ra.0 * ny - ra.1 * nx) * force * inv_i;
                } else if p.attraction {
                    let dx = b.x - other.x;
                    let dy = b.y - other.y;
                    let d2 = dx * dx + dy * dy + 0.15;
                    f[0] -= 1.8 * dx / (d2 * d2.sqrt());
                    f[1] -= 1.8 * dy / (d2 * d2.sqrt());
                }
            }
            for (normal, limit) in [
                ((1.0, 0.0), 0.0),
                ((0.0, 1.0), 0.0),
                ((-1.0, 0.0), -side),
                ((0.0, -1.0), -side),
            ] {
                let at = support(&vertices[i], normal, false);
                let depth = limit - at.0 * normal.0 - at.1 * normal.1;
                if depth > 0.0 {
                    let r = ((at.0 - b.x as f64) as f32, (at.1 - b.y as f64) as f32);
                    let nx = normal.0 as f32;
                    let ny = normal.1 as f32;
                    let speed = (b.vx - b.omega * r.1) * nx + (b.vy + b.omega * r.0) * ny;
                    let force = (p.stiffness * depth as f32 - 12.0 * speed).max(0.0);
                    f[0] += nx * force;
                    f[1] += ny * force;
                    f[2] += (r.0 * ny - r.1 * nx) * force * inv_i;
                    if normal.0 < 0.0 || normal.1 < 0.0 {
                        reaction += p.stiffness as f64 * depth;
                    }
                }
            }
            f[0] += glues[i][0];
            f[1] += glues[i][1];
            f[2] += glues[i][2] * inv_i / 6.0;
            self.contact_forces[i] = [f[0], f[1]];
            if self.mouse.down && self.mouse.index == Some(i) {
                let dx = self.mouse.x - b.x;
                let dy = self.mouse.y - b.y;
                let gain = 100.0 * (0.4 / dx.hypot(dy).max(0.0001)).min(1.0);
                f[0] += dx * gain - b.vx * 14.0;
                f[1] += dy * gain - b.vy * 14.0;
            }
            if self.rotation.index == Some(i) && self.rotation.remaining > 0.0 {
                let e = self.rotation.target - b.theta;
                f[2] += (60.0 * e.sin().atan2(e.cos()) - 8.0 * b.omega).clamp(-30.0, 30.0);
            }
            let z = &mut next[i];
            z.vx = ((b.vx + f[0] * dt) * (-p.damping * dt).exp()).clamp(-15.0, 15.0);
            z.vy = ((b.vy + f[1] * dt) * (-p.damping * dt).exp()).clamp(-15.0, 15.0);
            z.omega = ((b.omega + f[2] * dt) * (-(p.damping + 3.0) * dt).exp()).clamp(-8.0, 8.0);
            z.x += z.vx * dt;
            z.y += z.vy * dt;
            z.theta += z.omega * dt;
        }
        self.bodies = next;
        self.rotation.remaining = (self.rotation.remaining - dt).max(0.0);
        if p.band_tension == 0.0 {
            self.band_velocity = 0.0;
        } else {
            let n = self.bodies.len() as f64;
            let force = reaction - n * p.band_tension as f64 * (self.side - p.target_side);
            self.band_velocity = ((self.band_velocity as f64 + force * FIXED_STEP / (2.0 * n))
                * (-8.0 * FIXED_STEP).exp())
            .clamp(-1.0, 1.0) as f32;
            self.side = (self.side + self.band_velocity as f64 * FIXED_STEP)
                .clamp((n * shape.area()).sqrt(), 1000.0);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn coincident_polygons_separate_with_opposite_forces() {
        for shape in [
            shared::Shape::Triangle,
            shared::Shape::Pentagon,
            shared::Shape::Hexagon,
        ] {
            let physics = crate::Physics::new_for(shape, 2, 10.0);
            let a = shared::Arrangement {
                shape,
                n: 2,
                side: 10.0,
                squares: vec![
                    shared::Placement {
                        cx: 5.0,
                        cy: 5.0,
                        theta: 0.0
                    };
                    2
                ],
            };
            physics.load(&a);
            {
                let mut state = physics.state.borrow_mut();
                state.polygon_step();
                assert!((state.contact_forces[0][0] + state.contact_forces[1][0]).abs() < 1e-4);
                assert!((state.contact_forces[0][1] + state.contact_forces[1][1]).abs() < 1e-4);
                for _ in 0..2000 {
                    state.polygon_step();
                }
            }
            let out = physics.arrangement();
            assert!(
                shared::shape::separation(shape, &out.squares[0], &out.squares[1]).0 < 1e-4,
                "{shape}: {out:?}"
            );
        }
    }
    #[test]
    fn last_polygon_corner_can_glue_to_a_wall() {
        for shape in [
            shared::Shape::Triangle,
            shared::Shape::Pentagon,
            shared::Shape::Hexagon,
        ] {
            let p = crate::Physics::new_for(shape, 1, 6.0);
            let mut piece = shared::Placement {
                cx: 3.0,
                cy: 3.0,
                theta: 0.0,
            };
            let corner = shape.sides() - 1;
            piece.cx -= shape.vertices(&piece)[corner].0;
            p.load(&shared::Arrangement {
                shape,
                n: 1,
                side: 6.0,
                squares: vec![piece],
            });
            p.set_glues(&[crate::Glue {
                a: crate::Feature::Corner {
                    square: 0,
                    corner: corner as u8,
                },
                b: crate::Feature::Wall(0),
            }])
            .unwrap();
            assert!(p.violations().max_glue_error < 1e-6);
        }
    }
}
