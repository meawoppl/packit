//! Geometric overlap telemetry from one snapshot, independent of the backend.
use crate::Physics;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairViolation {
    pub a: usize,
    pub b: usize,
    pub depth: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallViolation {
    pub square: usize,
    /// 0 left, 1 bottom, 2 right, 3 top (the same order as glue walls).
    pub wall: u8,
    pub depth: f64,
}

/// Positive penetration depths only. Exact touching has depth zero.
/// This is display/settling telemetry, not a certificate for score submission.
#[derive(Clone, Debug, PartialEq)]
pub struct ViolationReport {
    pub bodies: Vec<f64>,
    pub walls: [f64; 4],
    pub pairs: Vec<PairViolation>,
    pub wall_contacts: Vec<WallViolation>,
    pub max_depth: f64,
}

impl ViolationReport {
    pub(crate) fn measure(arrangement: &shared::Arrangement) -> Self {
        let mut report = Self {
            bodies: vec![0.0; arrangement.squares.len()],
            walls: [0.0; 4],
            pairs: Vec::new(),
            wall_contacts: Vec::new(),
            max_depth: 0.0,
        };
        for (i, square) in arrangement.squares.iter().enumerate() {
            let mut depths = [0.0_f64; 4];
            for (x, y) in square.corners() {
                for (depth, value) in
                    depths
                        .iter_mut()
                        .zip([-x, -y, x - arrangement.side, y - arrangement.side])
                {
                    *depth = depth.max(value);
                }
            }
            for (wall, depth) in depths.into_iter().enumerate() {
                if depth > 0.0 {
                    report.wall_contacts.push(WallViolation {
                        square: i,
                        wall: wall as u8,
                        depth,
                    });
                    report.walls[wall] = report.walls[wall].max(depth);
                    report.bodies[i] = report.bodies[i].max(depth);
                    report.max_depth = report.max_depth.max(depth);
                }
            }
            for (j, other) in arrangement.squares.iter().enumerate().skip(i + 1) {
                let depth = shared::geometry::penetration(square, other);
                if depth > 0.0 {
                    report.pairs.push(PairViolation { a: i, b: j, depth });
                    report.bodies[i] = report.bodies[i].max(depth);
                    report.bodies[j] = report.bodies[j].max(depth);
                    report.max_depth = report.max_depth.max(depth);
                }
            }
        }
        report
    }
}

impl Physics {
    /// Measures the current readback on CPU; no extra GPU readback or force
    /// buffer is required. At 100 squares this checks 4,950 unordered pairs.
    pub fn violations(&self) -> ViolationReport {
        ViolationReport::measure(&self.arrangement())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{Arrangement, Placement};

    fn square(cx: f64, cy: f64, theta: f64) -> Placement {
        Placement { cx, cy, theta }
    }

    #[test]
    fn touching_grid_has_no_violations() {
        let physics = Physics::new(4, 2.0);
        let report = physics.violations();
        assert_eq!(report.bodies, [0.0; 4]);
        assert_eq!(report.walls, [0.0; 4]);
        assert!(report.pairs.is_empty());
        assert!(report.wall_contacts.is_empty());
        assert_eq!(report.max_depth, 0.0);
    }

    #[test]
    fn rotated_pair_and_each_wall_are_reported_without_double_counting() {
        let scene = Arrangement {
            n: 4,
            side: 2.0,
            squares: vec![
                square(0.2, 0.2, 0.0),
                square(1.8, 1.8, 0.0),
                square(1.0, 1.0, std::f64::consts::FRAC_PI_4),
                square(1.5, 1.0, 0.0),
            ],
        };
        let report = ViolationReport::measure(&scene);
        for wall in report.walls {
            assert!((wall - 0.3).abs() < 1e-12);
        }
        assert_eq!(report.wall_contacts.len(), 4);
        assert!(report.pairs.iter().all(|pair| pair.a < pair.b));
        let pair = report
            .pairs
            .iter()
            .find(|pair| pair.a == 2 && pair.b == 3)
            .unwrap();
        assert!((pair.depth - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert_eq!(report.bodies[2], pair.depth);
        assert_eq!(report.max_depth, shared::geometry::worst_violation(&scene));
    }

    #[test]
    fn reads_current_scene_without_changing_revision_or_inputs() {
        let physics = Physics::new(2, 2.0);
        physics.set_pose(1, 0.7, 0.5, 0.0);
        let revision = physics.state.borrow().revision;
        let before = physics.bodies();
        assert!(!physics.violations().pairs.is_empty());
        assert_eq!(physics.state.borrow().revision, revision);
        assert_eq!(physics.bodies(), before);
    }
}
