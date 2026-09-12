//! Short-range springs between facing edge midpoints, with an alignment torque.
//! The same kernel is implemented in WGSL. Contacts, not attraction, prevent overlap.
use super::Body;

pub(super) const RANGE: f32 = 0.75;
fn normals(theta: f32) -> [(f32, f32); 4] {
    let (s, c) = theta.sin_cos();
    [(c, s), (-s, c), (-c, -s), (s, -c)]
}
fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}
fn cross(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.1 - a.1 * b.0
}
/// Force at an edge midpoint and the resulting angular acceleration.
fn pull(normal: (f32, f32), opposite: (f32, f32), delta: (f32, f32), strength: f32) -> [f32; 3] {
    let distance = delta.0.hypot(delta.1);
    if distance >= RANGE
        || dot(normal, opposite) > -0.5
        || dot(normal, delta) < 0.0
        || dot(opposite, delta) > 0.0
    {
        return [0.0; 3];
    }
    let weight = (1.0 - distance / RANGE).powi(2);
    let force = (strength * weight * delta.0, strength * weight * delta.1);
    // Unit square inertia is 1/6. The extra equal/opposite couple favors flush faces.
    let torque =
        6.0 * (0.5 * cross(normal, force) - strength * 0.15 * weight * cross(normal, opposite));
    [force.0, force.1, torque]
}
pub(super) fn pair(a: &Body, b: &Body, strength: f32) -> [f32; 3] {
    let mut result = [0.0; 3];
    if strength == 0.0 || (a.x - b.x).powi(2) + (a.y - b.y).powi(2) > 4.7 {
        return result;
    }
    for na in normals(a.theta) {
        for nb in normals(b.theta) {
            let delta = (
                b.x + 0.5 * nb.0 - a.x - 0.5 * na.0,
                b.y + 0.5 * nb.1 - a.y - 0.5 * na.1,
            );
            let f = pull(na, nb, delta, strength);
            for k in 0..3 {
                result[k] += f[k];
            }
        }
    }
    result
}
/// Force/torque on the square, plus the equal/opposite force on a movable band.
pub(super) fn walls(b: &Body, side: f32, strength: f32) -> [f32; 4] {
    let mut result = [0.0; 4];
    if strength == 0.0 {
        return result;
    }
    let (s, c) = b.theta.sin_cos();
    let h = 0.5 * (s.abs() + c.abs());
    // Wall inward normals; attraction is disabled at a wall already penetrated.
    let walls = [
        ((1.0, 0.0), b.x - h),
        ((0.0, 1.0), b.y - h),
        ((-1.0, 0.0), side - b.x - h),
        ((0.0, -1.0), side - b.y - h),
    ];
    for na in normals(b.theta) {
        let anchor = (b.x + 0.5 * na.0, b.y + 0.5 * na.1);
        let points = [
            (0.0, anchor.1.clamp(0.0, side)),
            (anchor.0.clamp(0.0, side), 0.0),
            (side, anchor.1.clamp(0.0, side)),
            (anchor.0.clamp(0.0, side), side),
        ];
        for (j, (normal, gap)) in walls.iter().enumerate() {
            if *gap < 0.0 {
                continue;
            }
            let f = pull(
                na,
                *normal,
                (points[j].0 - anchor.0, points[j].1 - anchor.1),
                strength,
            );
            for k in 0..3 {
                result[k] += f[k];
            }
            if j >= 2 {
                result[3] -= f[0] + f[1];
            }
        }
    }
    result
}
