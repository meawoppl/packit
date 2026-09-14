//! Controls shared by both simulation backends. All timing uses simulated time.
use crate::{Body, Params, Physics, State, ViolationReport};

pub const SETTLE_DEPTH: f64 = 1e-5;
pub const SETTLE_LIMIT: f64 = 12.0;
pub const SETTLE_GLUE_ERROR: f64 = 1e-3;
const CALM_WINDOW: f64 = 0.5;
const COMPRESSION_MIN_BUDGET: f64 = 20.0;
const GENTLE_TENSION: f32 = 15.0;
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
    progress_depth: f64,
    stall_time: f64,
    opening: bool,
    compressing: bool,
    compression_elapsed: f64,
    compression_limit: f64,
    compression_timed_out: bool,
    compression_pose: Vec<Body>,
    compression_side: f64,
    compression_window: f64,
    status: SettleStatus,
    saved_params: Option<Params>,
}

impl Physics {
    /// Opt in to centered, pressure-driven growth. The renderer must place
    /// local `side/2` at its fixed screen center and invert that transform for
    /// pointer coordinates. Legacy anchored rendering can leave this disabled.
    /// Sustained drag growth releases band tension and holds the new size.
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

    /// Release compression/attraction and resolve contacts with damping. The
    /// box slowly opens all four walls only once deep overlap stops improving.
    /// No poses are changed by this call, and glue constraints remain intact.
    pub fn begin_settle(&self) {
        let mut s = self.state.borrow_mut();
        if s.disposed {
            return;
        }
        s.cancel_settle();
        s.interaction.saved_params = Some(s.params);
        s.interaction.status.phase = SettlePhase::Running;
        s.interaction.progress_depth = s.violation_report().max_depth;
        s.mouse.down = false;
        s.grabs.clear();
        s.rotation = Default::default();
        s.params.attraction = false;
        s.params.edge_attraction = 0.0;
        s.params.damping = 6.0;
        s.params.band_tension = 0.0;
        s.params.target_side = s.side;
        s.band_velocity = 0.0;
        s.paused = false;
    }

    /// First seek a damped local equilibrium under inward band pressure,
    /// then release it and resolve residual contacts before reporting Settled.
    /// Already-overlapping or unsatisfied-glue scenes resolve first instead.
    /// This bounded local relaxation is not a global packing optimizer.
    pub fn begin_settle_with_pressure(&self) {
        let original = self.params();
        self.begin_settle();
        let mut s = self.state.borrow_mut();
        if s.disposed {
            return;
        }
        let report = s.violation_report();
        if report.max_depth > SETTLE_DEPTH || report.max_glue_error > SETTLE_GLUE_ERROR {
            return;
        }
        let floor = s.container.area_bound(s.shape, s.bodies.len() as u32);
        // Don't grow a cramped box in the compression phase.
        if s.side <= floor {
            return;
        }
        s.interaction.compressing = true;
        s.interaction.compression_pose = s.bodies.clone();
        s.interaction.compression_side = s.side;
        // The wall speed is capped at one unit/second. Allow travel plus
        // damping time even for imported boards much larger than the pieces.
        s.interaction.compression_limit = COMPRESSION_MIN_BUDGET + 2.0 * (s.side - floor);
        s.params.band_tension = if original.band_tension > 0.0 {
            original.band_tension
        } else {
            GENTLE_TENSION
        };
        s.params.target_side = if original.band_tension > 0.0 && original.target_side < s.side {
            original.target_side.max(floor)
        } else {
            floor
        };
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
        self.interaction.stall_time = 0.0;
        self.interaction.opening = false;
        self.interaction.compressing = false;
        self.interaction.compression_elapsed = 0.0;
        self.interaction.compression_limit = 0.0;
        self.interaction.compression_timed_out = false;
        self.interaction.compression_pose.clear();
        self.interaction.compression_window = 0.0;
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
        for m in &mut self.grabs {
            m.x += shift as f32;
            m.y += shift as f32;
        }
        self.side += increase;
        self.params.target_side = self.side;
        self.band_velocity = 0.0;
        self.interaction.frame_shift += shift;
        // Preserve the completed batch's force telemetry through this frame
        // translation; the next batch refreshes it against the expanded walls.
    }

    pub(crate) fn step_interaction(&mut self, dt: f64) {
        if self.paused || self.disposed {
            return;
        }
        if self.interaction.status.phase == SettlePhase::Running {
            let report = self.violation_report();
            let motion = self.total_motion();
            if self.interaction.compressing {
                self.interaction.status = SettleStatus {
                    phase: SettlePhase::Running,
                    elapsed: self.interaction.status.elapsed + dt,
                    max_depth: report.max_depth,
                    motion,
                    max_glue_error: report.max_glue_error,
                };
                // Stiff contacts can have alternating substep velocities even
                // at a stationary pose. Judge compression equilibrium over a
                // real time window, then require low velocity after releasing.
                self.interaction.compression_window += dt;
                if self.interaction.compression_window >= CALM_WINDOW {
                    let displacement = self
                        .bodies
                        .iter()
                        .zip(&self.interaction.compression_pose)
                        .map(|(a, b)| {
                            let angle = f64::from(a.theta - b.theta);
                            f64::from(a.x - b.x)
                                .hypot(f64::from(a.y - b.y))
                                .max(angle.sin().atan2(angle.cos()).abs())
                        })
                        .fold(0.0, f64::max);
                    let calm = displacement <= 0.002
                        && (self.side - self.interaction.compression_side).abs() <= 0.002;
                    self.interaction.calm_time = if calm {
                        self.interaction.calm_time + self.interaction.compression_window
                    } else {
                        0.0
                    };
                    self.interaction.compression_window = 0.0;
                    self.interaction.compression_pose.clone_from(&self.bodies);
                    self.interaction.compression_side = self.side;
                }
                if (self.interaction.status.elapsed >= 1.0
                    && self.interaction.calm_time >= 2.0 * CALM_WINDOW)
                    || self.interaction.status.elapsed >= self.interaction.compression_limit
                {
                    self.interaction.compression_timed_out =
                        self.interaction.calm_time < 2.0 * CALM_WINDOW;
                    self.interaction.compression_elapsed = self.interaction.status.elapsed;
                    self.interaction.compressing = false;
                    self.params.band_tension = 0.0;
                    self.params.target_side = self.side;
                    self.band_velocity = 0.0;
                    self.interaction.calm_time = 0.0;
                    self.interaction.progress_depth = report.max_depth;
                    self.interaction.stall_time = 0.0;
                }
                return;
            }
            // Give contacts time to resolve at the existing size. Restart the
            // observation window whenever penetration falls meaningfully.
            let progress = self.interaction.progress_depth - report.max_depth;
            if progress > (self.interaction.progress_depth * 0.1).max(1e-6) {
                self.interaction.progress_depth = report.max_depth;
                self.interaction.stall_time = 0.0;
            } else {
                self.interaction.stall_time += dt;
            }
            if report.max_depth <= SETTLE_DEPTH {
                self.interaction.opening = false;
            } else if self.interaction.stall_time >= CALM_WINDOW {
                // Once jammed, keep opening until clear; resetting the stall
                // window on growth-induced progress would throttle resolution.
                self.interaction.opening = true;
            }
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
                self.finish_settle(if self.interaction.compression_timed_out {
                    SettlePhase::TimedOut
                } else {
                    SettlePhase::Settled
                });
            } else if status.elapsed >= SETTLE_LIMIT + self.interaction.compression_elapsed {
                self.finish_settle(
                    if report.max_depth > SETTLE_DEPTH || report.max_glue_error > SETTLE_GLUE_ERROR
                    {
                        SettlePhase::Blocked
                    } else {
                        SettlePhase::TimedOut
                    },
                );
            } else if self.interaction.opening {
                self.grow_frame(0.08 * dt);
            }
            return;
        }
        if !self.interaction.drag_expansion {
            return;
        }
        if !std::iter::once(&self.mouse)
            .chain(&self.grabs)
            .any(|m| m.down && m.index.is_some())
        {
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
            if !changed {
                break;
            }
        }
        let mouse = std::iter::once(&self.mouse)
            .chain(&self.grabs)
            .filter(|m| m.down && m.index.is_some_and(|i| connected[i]))
            .max_by(|a, b| {
                let pressure = |m: &super::Mouse| {
                    let i = m.index.unwrap();
                    let b = self.bodies[i];
                    let (dx, dy) = (m.x - b.x, m.y - b.y);
                    let c = self.contact_forces[i];
                    if dx.hypot(dy) <= 0.18 {
                        0.0
                    } else {
                        -(c[0] * dx + c[1] * dy) / dx.hypot(dy)
                    }
                };
                pressure(a).total_cmp(&pressure(b))
            })
            .copied();
        let Some(mouse) = mouse else {
            self.interaction.pressure_time = 0.0;
            return;
        };
        let i = mouse.index.unwrap();
        let b = self.bodies[i];
        let (dx, dy) = (mouse.x - b.x, mouse.y - b.y);
        let distance = dx.hypot(dy);
        let contact = self.contact_forces[i];
        let resistance = -(contact[0] * dx + contact[1] * dy) / distance.max(1e-6);
        if distance <= 0.18 || resistance <= 8.0 {
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
        p.state.borrow_mut().contact_forces[0] = [12.0, -7.0];
        p.state.borrow_mut().grow_frame(0.25);
        assert_eq!(p.contact_forces()[0], [12.0, -7.0]);
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
    fn slack_settle_applies_pressure_then_finishes_clear_and_releases_band() {
        for shape in shared::Shape::ALL {
            for container in shared::Shape::ALL {
                let p = Physics::new_in(shape, container, 1, 4.0);
                p.set_pose(0, 2.0, 2.0, 0.0);
                let before = p.arrangement();
                p.begin_settle_with_pressure();
                assert_eq!(p.arrangement(), before, "no input teleport");
                assert!(p.params().band_tension > 0.0);
                assert!(p.params().target_side < 4.0);
                run(&p, 900);
                assert_eq!(
                    p.settle_status().phase,
                    SettlePhase::Settled,
                    "{shape}/{container}: {:?}",
                    p.settle_status()
                );
                assert!(p.side() < 3.9, "{shape}/{container} did not tighten");
                assert!(p.violations().max_depth <= SETTLE_DEPTH);
                assert_eq!(p.params().band_tension, 0.0);
                assert!(p.paused());
            }
        }
    }

    #[test]
    fn roomy_boards_reach_equilibrium_and_polygon_sides_can_shrink_below_one() {
        for (shape, container, side, expected) in [
            (shared::Shape::Square, shared::Shape::Square, 20.0, 1.01),
            (shared::Shape::Triangle, shared::Shape::Hexagon, 0.9, 0.85),
        ] {
            let p = Physics::new_in(shape, container, 1, side);
            p.set_pose(0, (side / 2.0) as f32, (side / 2.0) as f32, 0.0);
            p.begin_settle_with_pressure();
            assert!(p.params().band_tension > 0.0);
            run(&p, 1500);
            assert_eq!(
                p.settle_status().phase,
                SettlePhase::Settled,
                "{shape}/{container}: {:?}",
                p.settle_status()
            );
            assert!(p.side() < expected, "{shape}/{container}: {}", p.side());
            assert!(p.violations().max_depth <= SETTLE_DEPTH);
        }
    }

    #[test]
    fn pressure_settle_cancels_cleanly_and_resolves_cramped_scenes_first() {
        let p = Physics::new(2, 4.0);
        p.begin_settle_with_pressure();
        assert!(p.params().band_tension > 0.0);
        run(&p, 10);
        p.set_mouse(1.0, 1.0, Some(0), true);
        assert_eq!(p.settle_status().phase, SettlePhase::Idle);
        assert_eq!(p.params().band_tension, 0.0);
        let p = Physics::new(2, 1.8);
        p.begin_settle_with_pressure();
        assert_eq!(
            p.params().band_tension,
            0.0,
            "don't squeeze existing overlaps harder"
        );
        run(&p, 900);
        assert_eq!(p.settle_status().phase, SettlePhase::Settled);
        assert!(p.violations().max_depth <= SETTLE_DEPTH);
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
    fn resolvable_overlap_preserves_wall_to_wall_glue_and_size() {
        for offset in [0.005, 0.01, 0.02, 0.05] {
            let p = Physics::new(4, 2.0);
            p.set_pose(2, 0.5, 1.5 - offset, 0.0);
            p.set_pose(3, 1.5, 1.5 - offset, 0.0);
            let point = |square, edge| Feature::Midpoint { square, edge };
            p.set_glues(&[
                Glue {
                    a: Feature::Wall(0),
                    b: point(0, 2),
                },
                Glue {
                    a: point(0, 0),
                    b: point(1, 2),
                },
                Glue {
                    a: point(1, 0),
                    b: Feature::Wall(2),
                },
            ])
            .unwrap();
            p.begin_settle();
            run(&p, 260);
            assert_eq!(
                p.settle_status().phase,
                SettlePhase::Settled,
                "offset {offset}: {:?}",
                p.settle_status()
            );
            assert_eq!(p.side(), 2.0);
        }
    }

    #[test]
    fn disturbance_requires_a_fresh_continuous_calm_window() {
        let p = Physics::new(1, 3.0);
        p.begin_settle();
        run(&p, 7);
        p.state.borrow_mut().bodies[0].vx = 0.05;
        step(&p);
        p.state.borrow_mut().bodies[0].vx = 0.0;
        run(&p, 7);
        assert_eq!(p.settle_status().phase, SettlePhase::Running);
        run(&p, 4);
        assert_eq!(p.settle_status().phase, SettlePhase::Settled);
    }

    #[test]
    fn interior_resistance_without_wall_chain_does_not_grow() {
        let p = Physics::new(2, 20.0);
        p.set_drag_expansion(true);
        p.set_pose(0, 9.5, 10.0, 0.0);
        p.set_pose(1, 10.45, 10.0, 0.0);
        p.set_mouse(13.0, 10.0, Some(0), true);
        // Isolate the controller gate: sustained contact resistance, with no
        // contacted wall, must never be mistaken for pressure on the container.
        for _ in 0..40 {
            let mut s = p.state.borrow_mut();
            s.contact_forces[0] = [-40.0, 0.0];
            s.step_interaction(0.05);
        }
        assert_eq!(p.side(), 20.0);
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

#[cfg(test)]
mod touch_tests {
    use super::*;

    #[test]
    fn interior_grab_cannot_mask_another_fingers_wall_pressure() {
        let p = Physics::new(3, 8.0);
        p.set_pose(0, 3.0, 4.0, 0.0);
        p.set_pose(1, 0.49, 1.0, 0.0);
        p.set_pose(2, 3.95, 4.0, 0.0);
        p.set_drag_expansion(true);
        p.set_grabs(&[(0, 2.0, 4.0), (1, -0.5, 1.0)]);
        let mut s = p.state.borrow_mut();
        s.contact_forces[0] = [60.0, 0.0];
        s.contact_forces[1] = [20.0, 0.0];
        s.step_interaction(0.2);
        assert!(s.side > 8.0, "the wall-connected grab must grow the box");
    }

    #[test]
    fn grabs_reject_invalid_targets_and_shift_with_the_frame() {
        let p = Physics::new(2, 4.0);
        p.set_grabs(&[(0, 1.0, 1.0), (0, 3.0, 3.0)]);
        assert_eq!(p.state.borrow().grabs.len(), 1);
        p.state.borrow_mut().grow_frame(0.2);
        let s = p.state.borrow();
        assert!((s.grabs[0].x - 1.1).abs() < 1e-6);
        assert!((s.grabs[0].y - 1.1).abs() < 1e-6);
        drop(s);
        p.set_grabs(&[(9, 1.0, 1.0), (0, f32::NAN, 0.0)]);
        assert!(p.state.borrow().grabs.is_empty());
        p.set_grabs(&[(0, 1.0, 1.0)]);
        let a = p.arrangement();
        p.load(&a);
        assert!(p.state.borrow().grabs.is_empty());
    }
}
