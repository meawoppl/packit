//! Contact points on the facing support features, including angular contact speed.
use super::Body;
fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}
fn feature(b: &Body, d: (f32, f32), t: (f32, f32), skin: f32) -> (f32, f32) {
    let (s, c) = b.theta.sin_cos();
    let u = (c, s);
    let v = (-s, c);
    let select = |v: f32| if v.abs() <= skin { 0.0 } else { v.signum() };
    let a = select(dot(u, d));
    let z = select(dot(v, d));
    let r = (0.5 * (a * u.0 + z * v.0), 0.5 * (a * u.1 + z * v.1));
    let center = dot((b.x + r.0, b.y + r.1), t);
    let half =
        0.5 * (if dot(u, d).abs() <= skin {
            dot(u, t).abs()
        } else {
            0.0
        }) + 0.5
            * (if dot(v, d).abs() <= skin {
                dot(v, t).abs()
            } else {
                0.0
            });
    (center - half, center + half)
}
/// Tangential lever arms at the midpoint of the overlapping support intervals.
pub(super) fn levers(a: &Body, b: &Body, n: (f32, f32), depth: f32) -> (f32, f32, f32) {
    let t = (-n.1, n.0);
    let fa = feature(a, (-n.0, -n.1), t, depth.max(1e-5));
    let fb = feature(b, n, t, depth.max(1e-5));
    let contact = 0.5 * (fa.0.max(fb.0) + fa.1.min(fb.1));
    (
        contact - dot((a.x, a.y), t),
        contact - dot((b.x, b.y), t),
        (fa.1.min(fb.1) - fa.0.max(fb.0)).max(0.0),
    )
}
pub(super) fn wall(b: &Body, n: (f32, f32), depth: f32, stiffness: f32) -> [f32; 3] {
    if depth <= 0.0 {
        return [0.0; 3];
    }
    let (s, c) = b.theta.sin_cos();
    let u = (c, s);
    let v = (-s, c);
    let h = 0.5 * (dot(u, n).abs() + dot(v, n).abs());
    let mut total_penetration = 0.0;
    for a in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            let r = (0.5 * (a * u.0 + z * v.0), 0.5 * (a * u.1 + z * v.1));
            total_penetration += (depth - dot(r, n) - h).max(0.0);
        }
    }
    let weight = depth / total_penetration.max(1e-12);
    let mut result = [0.0; 3];
    // Integrate the two penetrating face corners instead of switching a single
    // support vertex at tiny angles; this gives resting faces a stable torque.
    for a in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            let r = (0.5 * (a * u.0 + z * v.0), 0.5 * (a * u.1 + z * v.1));
            let penetration = depth - dot(r, n) - h;
            if penetration <= 0.0 {
                continue;
            }
            let spin = r.0 * n.1 - r.1 * n.0;
            let speed = dot((b.vx, b.vy), n) + b.omega * spin;
            let force = weight * (stiffness * penetration - 12.0 * speed).max(0.0);
            result[0] += n.0 * force;
            result[1] += n.1 * force;
            result[2] += spin * force * 6.0;
        }
    }
    result
}
