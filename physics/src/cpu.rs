use super::{contacts, edges, glue, State, FIXED_STEP};
fn radius(t: f32, x: f32, y: f32) -> f32 {
    let (sin, cos) = t.sin_cos();
    0.5 * ((cos * x + sin * y).abs() + (-sin * x + cos * y).abs())
}
impl State {
    pub(super) fn cpu_step(&mut self) {
        let dt = FIXED_STEP as f32;
        let p = self.params;
        let (glue_forces, _) = glue::forces(
            &self.glues,
            &self.bodies,
            self.side as f32,
            self.band_velocity,
            p.stiffness,
        );
        let mut next = self.bodies.clone();
        for (i, b) in self.bodies.iter().enumerate() {
            let (mut fx, mut fy, mut torque) = (0.0, 0.0, 0.0);
            let mut contact = [0.0; 2];
            for (j, other) in self.bodies.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (dx, dy) = (b.x - other.x, b.y - other.y);
                let d2 = dx * dx + dy * dy + 0.15;
                if p.attraction {
                    fx -= 1.8 * dx / (d2 * d2.sqrt());
                    fy -= 1.8 * dy / (d2 * d2.sqrt());
                }
                let (mut depth, mut nx, mut ny) = (f32::INFINITY, 1.0, 0.0);
                for t in [
                    b.theta,
                    b.theta + std::f32::consts::FRAC_PI_2,
                    other.theta,
                    other.theta + std::f32::consts::FRAC_PI_2,
                ] {
                    let (ay, ax) = t.sin_cos();
                    let dot = dx * ax + dy * ay;
                    let overlap = radius(b.theta, ax, ay) + radius(other.theta, ax, ay) - dot.abs();
                    if overlap < depth {
                        depth = overlap;
                        let sign = if dot >= 0.0 { 1.0 } else { -1.0 };
                        nx = ax * sign;
                        ny = ay * sign;
                    }
                }
                if depth > 0.0 {
                    let (lever, other_lever, width) = contacts::levers(b, other, (nx, ny), depth);
                    let speed = (b.vx - other.vx) * nx + (b.vy - other.vy) * ny - b.omega * lever
                        + other.omega * other_lever;
                    let force = (p.stiffness * depth - 12.0 * speed).max(0.0);
                    fx += nx * force;
                    fy += ny * force;
                    contact[0] += nx * force;
                    contact[1] += ny * force;
                    torque -= lever * force * 6.0;
                    // A finite face patch also distributes pressure and damping
                    // across its width; a point contact has no such couple.
                    torque -= width * width / 12.0
                        * 6.0
                        * (p.stiffness * (4.0 * (b.theta - other.theta)).sin() / 4.0
                            + 12.0 * (b.omega - other.omega));
                } else if p.edge_attraction > 0.0 {
                    let f = edges::pair(b, other, p.edge_attraction);
                    fx += f[0];
                    fy += f[1];
                    torque += f[2];
                    contact[0] += f[0];
                    contact[1] += f[1];
                }
            }
            let h = radius(b.theta, 1.0, 0.0);
            let side = self.side as f32;
            for (normal, depth) in [
                ((1.0, 0.0), h - b.x),
                ((0.0, 1.0), h - b.y),
                ((-1.0, 0.0), b.x - side + h),
                ((0.0, -1.0), b.y - side + h),
            ] {
                let f = contacts::wall(b, normal, depth, p.stiffness);
                fx += f[0];
                fy += f[1];
                torque += f[2];
                contact[0] += f[0];
                contact[1] += f[1];
            }
            let f = edges::walls(b, side, p.edge_attraction);
            fx += f[0];
            fy += f[1];
            torque += f[2];
            contact[0] += f[0];
            contact[1] += f[1];
            let f = glue_forces[i];
            fx += f[0];
            fy += f[1];
            torque += f[2];
            contact[0] += f[0];
            contact[1] += f[1];
            self.contact_forces[i] = contact;
            if self.mouse.down && self.mouse.index == Some(i) {
                let (dx, dy) = (self.mouse.x - b.x, self.mouse.y - b.y);
                let gain = 100.0 * (0.4 / dx.hypot(dy).max(0.0001)).min(1.0);
                fx += dx * gain - b.vx * 14.0;
                fy += dy * gain - b.vy * 14.0;
            }
            if self.rotation.index == Some(i) && self.rotation.remaining > 0.0 {
                let error = self.rotation.target - b.theta;
                let error = error.sin().atan2(error.cos());
                torque += (60.0 * error - 8.0 * b.omega).clamp(-30.0, 30.0);
            }
            let n = &mut next[i];
            n.vx = ((b.vx + fx * dt) * (-p.damping * dt).exp()).clamp(-15.0, 15.0);
            n.vy = ((b.vy + fy * dt) * (-p.damping * dt).exp()).clamp(-15.0, 15.0);
            n.omega = ((b.omega + torque * dt) * (-(p.damping + 3.0) * dt).exp()).clamp(-8.0, 8.0);
            n.x = b.x + n.vx * dt;
            n.y = b.y + n.vy * dt;
            n.theta = b.theta + n.omega * dt;
        }
        self.rotation.remaining = (self.rotation.remaining - dt).max(0.0);
        self.bodies = next;
        self.step_band();
    }
    pub(super) fn step_band(&mut self) {
        if self.params.band_tension == 0.0 {
            self.band_velocity = 0.0;
            return;
        }
        let reaction: f64 = self
            .bodies
            .iter()
            .map(|b| {
                let h = radius(b.theta, 1.0, 0.0) as f64;
                self.params.stiffness as f64
                    * ((b.x as f64 + h - self.side).max(0.0)
                        + (b.y as f64 + h - self.side).max(0.0))
                    + edges::walls(b, self.side as f32, self.params.edge_attraction)[3] as f64
            })
            .sum();
        let n = self.bodies.len() as f64;
        let (_, glue_reaction) = glue::forces(
            &self.glues,
            &self.bodies,
            self.side as f32,
            self.band_velocity,
            self.params.stiffness,
        );
        let force = reaction + glue_reaction as f64
            - n * self.params.band_tension as f64 * (self.side - self.params.target_side);
        self.band_velocity = ((self.band_velocity as f64 + force * FIXED_STEP / (2.0 * n))
            * (-8.0 * FIXED_STEP).exp())
        .clamp(-1.0, 1.0) as f32;
        self.side = (self.side + self.band_velocity as f64 * FIXED_STEP).clamp(n.sqrt(), 1000.0);
    }
}
