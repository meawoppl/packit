//! Controls shared by both simulation backends. All timing uses simulated time.
use crate::{Params, Physics, State, ViolationReport};

pub const SETTLE_DEPTH: f64 = 1e-5;
pub const SETTLE_LIMIT: f64 = 12.0;
pub const SETTLE_GLUE_ERROR: f64 = 1e-3;
const CALM_WINDOW: f64 = 0.5;
const MOTION_PER_SQUARE: f32 = 0.002;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettlePhase {
    #[default]
    Idle,
    Running,
    Settled,
    Blocked,
    TimedOut,
}

/// `Settled` is a simulation stopping condition, not score certification.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SettleStatus {
    pub phase: SettlePhase,
    pub elapsed: f64,
    pub max_depth: f64,
    pub motion: f32,
    pub max_glue_error: f64,
}

#[derive(Default)]
pub(crate) struct Interaction {
    pub drag_expansion: bool,
    pub frame_shift: f64,
    pressure_time: f64,
    calm_time: f64,
    status: SettleStatus,
    saved_params: Option<Params>,
}

impl Physics {
    /// Opt in to centered, pressure-driven growth. The renderer must place
    /// local `side/2` at its fixed screen center and invert that transform for
    /// pointer coordinates. Legacy anchored rendering can leave this disabled.
    pub fn set_drag_expansion(&self, enabled: bool) {
        let mut s = self.state.borrow_mut();
        s.interaction.drag_expansion = enabled;
        s.interaction.pressure_time = 0.0;
    }

    /// Cumulative local-coordinate translation on each axis due to symmetric
    /// growth. It never resets during this Physics instance's lifetime.
    /// Rotation input stores only an angle, so needs no frame translation.
    pub fn frame_shift(&self) -> f64 {
        self.state.borrow().interaction.frame_shift
    }

    /// Release compression/attraction and resolve contacts with damping. Deep
    /// intersections left after a brief relaxation slowly open all four walls.
    /// No poses are changed by this call, and glue constraints remain intact.
    pub fn begin_settle(&self) {
        let mut s = self.state.borrow_mut();
        if s.disposed {
            return;
        }
        s.cancel_settle();
        s.interaction.saved_params = Some(s.params);
        s.interaction.status.phase = SettlePhase::Running;
        s.mouse.down = false;
        s.rotation = Default::default();
        s.params.attraction = false;
        s.params.edge_attraction = 0.0;
        s.params.damping = 6.0;
        s.params.band_tension = 0.0;
        s.params.target_side = s.side;
        s.band_velocity = 0.0;
        s.paused = false;
    }

    /// Cancel the controller and release the band without moving any bodies.
    /// The caller decides whether to pause or continue direct interaction.
    pub fn cancel_settle(&self) {
        self.state.borrow_mut().cancel_settle();
    }

    pub fn settle_status(&self) -> SettleStatus {
        self.state.borrow().interaction.status
    }
}

impl State {
    pub(crate) fn cancel_settle_for_drag(&mut self) {
        if self.interaction.status.phase != SettlePhase::Idle {
            self.cancel_settle();
        }
    }

    pub(crate) fn cancel_settle(&mut self) {
        if let Some(params) = self.interaction.saved_params.take() {
            self.params = Params {
                band_tension: 0.0,
                target_side: self.side,
                ..params
            };
            self.band_velocity = 0.0;
        }
        self.interaction.status = SettleStatus::default();
        self.interaction.calm_time = 0.0;
        self.interaction.pressure_time = 0.0;
    }

    /// This changes reference frame, not velocity or relative body positions.
    /// Called only after a completed physics batch; no outstanding GPU result
    /// can overwrite the translation. Do not bump the direct-edit revision.
    fn grow_frame(&mut self, increase: f64) {
        let room = self.bodies.iter().fold(1000.0 - self.side, |room, b| {
            room.min(2.0 * (1000.0 - b.x.max(b.y) as f64))
        });
        let increase = increase.min(room).max(0.0);
        let shift = increase * 0.5;
        if shift == 0.0 {
            return;
        }
        for b in &mut self.bodies {
            b.x += shift as f32;
            b.y += shift as f32;
        }
        self.mouse.x += shift as f32;
        self.mouse.y += shift as f32;
        self.side += increase;
        self.params.target_side = self.side;
        self.band_velocity = 0.0;
        self.interaction.frame_shift += shift;
        // These forces describe the previous boundary locations.
        self.contact_forces.fill([0.0; 2]);
    }

    pub(crate) fn step_interaction(&mut self, dt: f64) {
        if self.paused || self.disposed {
            return;
        }
        if self.interaction.status.phase == SettlePhase::Running {
            let report = self.violation_report();
            let motion = self.total_motion();
            let status = &mut self.interaction.status;
            status.elapsed += dt;
            status.max_depth = report.max_depth;
            status.motion = motion;
            status.max_glue_error = report.max_glue_error;
            let calm = report.max_depth <= SETTLE_DEPTH
                && report.max_glue_error <= SETTLE_GLUE_ERROR
                && motion <= MOTION_PER_SQUARE * self.bodies.len() as f32;
            self.interaction.calm_time = if calm {
                self.interaction.calm_time + dt
            } else {
                0.0
            };
            if self.interaction.calm_time >= CALM_WINDOW {
                self.finish_settle(SettlePhase::Settled);
            } else if status.elapsed >= SETTLE_LIMIT {
                self.finish_settle(
                    if report.max_depth > SETTLE_DEPTH || report.max_glue_error > SETTLE_GLUE_ERROR
                    {
                        SettlePhase::Blocked
                    } else {
                        SettlePhase::TimedOut
                    },
                );
            } else if status.elapsed >= CALM_WINDOW && report.max_depth > SETTLE_DEPTH {
                self.grow_frame(0.08 * dt);
            }
            return;
        }
        if !self.interaction.drag_expansion {
            return;
        }
        let Some(i) = self.mouse.index.filter(|_| self.mouse.down) else {
            self.interaction.pressure_time = 0.0;
            return;
        };
        let b = self.bodies[i];
        let (dx, dy) = (self.mouse.x - b.x, self.mouse.y - b.y);
        let distance = dx.hypot(dy);
        let contact = self.contact_forces[i];
        let resistance = -(contact[0] * dx + contact[1] * dy) / distance.max(1e-6);
        if distance <= 0.18 || resistance <= 8.0 {
            self.interaction.pressure_time = 0.0;
            return;
        }
        let report = ViolationReport::measure(&self.arrangement());
        // Require a chain of actual contacts to a wall, not merely a moving
        // square's inertia or a collision elsewhere in a spacious container.
        let mut connected = vec![false; self.bodies.len()];
        for wall in &report.wall_contacts {
            if wall.depth > 1e-3 {
                connected[wall.square] = true;
            }
        }
        for _ in 0..self.bodies.len() {
            let mut changed = false;
            for pair in &report.pairs {
                if connected[pair.a] != connected[pair.b] {
                    connected[pair.a] = true;
                    connected[pair.b] = true;
                    changed = true;
                }
            }
            if !changed || connected[i] {
                break;
            }
        }
        if !connected[i] {
            self.interaction.pressure_time = 0.0;
            return;
        }
        self.interaction.pressure_time += dt;
        if self.interaction.pressure_time >= 0.15 {
            let rate = ((resistance as f64 - 8.0) / 32.0 * 0.5).clamp(0.05, 0.5);
            self.params.band_tension = 0.0;
            self.grow_frame(rate * dt);
        }
    }

    fn finish_settle(&mut self, phase: SettlePhase) {
        let mut status = self.interaction.status;
        status.phase = phase;
        self.cancel_settle();
        self.interaction.status = status;
        self.paused = true;
    }

    fn total_motion(&self) -> f32 {
        self.bodies
            .iter()
            .map(|b| b.vx.abs() + b.vy.abs() + b.omega.abs())
            .sum()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::{Feature, Glue};
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    fn step_count(p: &Physics, count: u32) {
        let mut future = std::pin::pin!(p.step(count));
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(())
        ));
    }
    fn step(p: &Physics) {
        step_count(p, 6);
    }
    fn run(p: &Physics, batches: usize) {
        for _ in 0..batches {
            step(p);
        }
    }

    #[test]
    fn frame_growth_preserves_centered_poses_mouse_lead_and_velocity() {
        let p = Physics::new(2, 3.0);
        p.set_mouse(0.3, 1.7, Some(0), true);
        p.turn(0, 0.2);
        let old = p.bodies();
        let revision = p.state.borrow().revision;
        let force = p.mouse_force();
        let turn = p.state.borrow().rotation.target;
        p.state.borrow_mut().grow_frame(0.25);
        assert_eq!(p.side(), 3.25);
        assert_eq!(p.frame_shift(), 0.125);
        assert_eq!(p.state.borrow().revision, revision);
        assert_eq!(p.state.borrow().rotation.target, turn);
        let next_force = p.mouse_force().unwrap().1;
        for (a, b) in force.unwrap().1.into_iter().zip(next_force) {
            assert!((a - b).abs() < 1e-5);
        }
        for (a, b) in old.iter().zip(p.bodies()) {
            assert!(((a.x as f64 - 1.5) - (b.x as f64 - p.side() / 2.0)).abs() < 1e-6);
            assert!(((a.y as f64 - 1.5) - (b.y as f64 - p.side() / 2.0)).abs() < 1e-6);
            assert_eq!(
                (a.vx, a.vy, a.omega, a.theta),
                (b.vx, b.vy, b.omega, b.theta)
            );
        }
    }

    #[test]
    fn resisted_drag_grows_each_wall_and_release_stops_growth() {
        for (x, y) in [(-2.0, 1.0), (1.0, -2.0), (4.0, 1.0), (1.0, 4.0)] {
            let p = Physics::new(1, 2.0);
            p.set_drag_expansion(true);
            let before = p.bodies();
            p.set_mouse(x, y, Some(0), true);
            assert_eq!(p.side(), 2.0, "input must not jump the box");
            assert_eq!(p.bodies(), before);
            let revision = p.state.borrow().revision;
            for _ in 0..100 {
                // Re-derive the same screen/world target in the new local frame.
                let shift = p.frame_shift() as f32;
                p.set_mouse(x + shift, y + shift, Some(0), true);
                step(&p);
            }
            assert!(p.side() > 2.1, "wall ({x},{y}) did not yield: {}", p.side());
            assert!(p.side() < 4.6, "growth must be rate-limited");
            assert_eq!(
                p.state.borrow().revision,
                revision,
                "spring input must not discard readbacks"
            );
            p.set_mouse(0.0, 0.0, None, false);
            let side = p.side();
            run(&p, 20);
            assert_eq!(p.side(), side);
        }
    }

    #[test]
    fn free_drag_and_brief_contact_do_not_expand_box() {
        let p = Physics::new(1, 20.0);
        p.set_drag_expansion(true);
        p.set_mouse(15.0, 10.0, Some(0), true);
        run(&p, 80);
        assert_eq!(p.side(), 20.0);
        let p = Physics::new(1, 2.0);
        p.set_drag_expansion(true);
        p.set_pose(0, 1.52, 1.0, 0.0);
        p.set_mouse(5.0, 1.0, Some(0), true);
        step(&p);
        step(&p);
        p.set_mouse(0.0, 0.0, None, false);
        run(&p, 40);
        assert_eq!(p.side(), 2.0, "a brief bump must not grow the box");
    }

    #[test]
    fn touching_grid_settles_without_a_pose_change() {
        let p = Physics::new(4, 2.0);
        let initial = p.arrangement();
        p.begin_settle();
        assert_eq!(p.arrangement(), initial);
        run(&p, 30);
        assert_eq!(p.settle_status().phase, SettlePhase::Settled);
        assert!(p.paused());
        assert_eq!(p.arrangement(), initial);
        assert_eq!(p.params().band_tension, 0.0);
    }

    #[test]
    fn cramped_scene_resolves_by_forces_and_gradual_growth() {
        let p = Physics::new(2, 1.8);
        p.set_pose(0, 0.5, 0.9, 0.0);
        p.set_pose(1, 1.3, 0.9, 0.0);
        let before = p.arrangement();
        p.begin_settle();
        assert_eq!(p.arrangement(), before);
        let mut previous = p.bodies();
        let mut previous_shift = p.frame_shift();
        for _ in 0..1560 {
            step_count(&p, 1);
            let shift = p.frame_shift() - previous_shift;
            let next = p.bodies();
            for (a, b) in previous.iter().zip(&next) {
                // Every displacement is the actual semi-implicit physics
                // velocity integral plus the symmetric coordinate shift.
                assert!(
                    ((b.x - a.x) as f64 - shift - b.vx as f64 * crate::FIXED_STEP).abs() < 1e-6
                );
                assert!(
                    ((b.y - a.y) as f64 - shift - b.vy as f64 * crate::FIXED_STEP).abs() < 1e-6
                );
            }
            previous = next;
            previous_shift = p.frame_shift();
            if p.settle_status().phase != SettlePhase::Running {
                break;
            }
        }
        assert_eq!(
            p.settle_status().phase,
            SettlePhase::Settled,
            "{:?}",
            p.settle_status()
        );
        assert!(p.violations().max_depth <= SETTLE_DEPTH);
        assert!(p.side() >= 2.0 - SETTLE_DEPTH * 3.0);
    }

    #[test]
    fn contradictory_glue_is_blocked_and_never_claims_settled() {
        let p = Physics::new(2, 2.0);
        let links = [0, 2].map(|edge| Glue {
            a: Feature::Midpoint { square: 0, edge },
            b: Feature::Midpoint { square: 1, edge },
        });
        p.set_glues(&links).unwrap();
        p.begin_settle();
        run(&p, 260);
        assert_eq!(
            p.settle_status().phase,
            SettlePhase::Blocked,
            "{:?}",
            p.settle_status()
        );
        assert!(p.paused());
        assert_eq!(p.glues(), links);
        assert_eq!(p.params().band_tension, 0.0);
        assert!(p.violations().max_depth > SETTLE_DEPTH);
    }

    #[test]
    fn unsatisfied_wall_glues_are_blocked_even_without_overlap() {
        let p = Physics::new(1, 2.0);
        p.set_glues(&[
            Glue {
                a: Feature::Midpoint { square: 0, edge: 0 },
                b: Feature::Wall(0),
            },
            Glue {
                a: Feature::Midpoint { square: 0, edge: 2 },
                b: Feature::Wall(2),
            },
        ])
        .unwrap();
        assert_eq!(p.violations().max_depth, 0.0);
        assert!(p.violations().max_glue_error > 1.0);
        assert!(p.violations().bodies[0] > 1.0);
        p.begin_settle();
        run(&p, 260);
        assert_eq!(p.settle_status().phase, SettlePhase::Blocked);
        assert_eq!(p.side(), 2.0, "don't expand for glue error alone");
        assert_eq!(p.violations().max_depth, 0.0);
    }

    #[test]
    fn persistent_motion_times_out_instead_of_claiming_completion() {
        let p = Physics::new(1, 3.0);
        p.begin_settle();
        for _ in 0..260 {
            if p.settle_status().phase != SettlePhase::Running {
                break;
            }
            // An ongoing disturbance with no geometric overlap.
            p.state.borrow_mut().bodies[0].vx = 0.05;
            step(&p);
        }
        assert_eq!(p.settle_status().phase, SettlePhase::TimedOut);
        assert_eq!(p.violations().max_depth, 0.0);
        assert!(p.paused());
    }

    #[test]
    fn direct_controls_cancel_a_run_and_release_compression() {
        let p = Physics::new(1, 2.0);
        p.set_params(Params {
            band_tension: 40.0,
            target_side: 1.0,
            ..p.params()
        });
        for action in 0..5 {
            p.begin_settle();
            assert_eq!(p.settle_status().phase, SettlePhase::Running);
            match action {
                0 => p.set_mouse(1.0, 1.0, Some(0), true),
                1 => p.set_paused(true),
                2 => p.reset(),
                3 => p.load(&p.arrangement()),
                _ => p.cancel_settle(),
            }
            assert_eq!(p.settle_status().phase, SettlePhase::Idle);
            assert_eq!(p.params().band_tension, 0.0);
        }
        p.begin_settle();
        p.dispose();
        assert_eq!(p.settle_status().phase, SettlePhase::Idle);
    }
}
