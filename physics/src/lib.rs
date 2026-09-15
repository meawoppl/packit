//! Shared browser physics state: unit squares, bottom-left origin, radians.
use std::{cell::RefCell, rc::Rc};

pub const MAX_SQUARES: usize = shared::MAX_N as usize;
pub const FIXED_STEP: f64 = 1.0 / 120.0;
pub const GPU_KERNEL: &str = include_str!("../kernel.wgsl");
#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;
mod contacts;
mod cpu;
mod edges;
mod glue;
mod interaction;
mod polygon;
pub use interaction::{SettlePhase, SettleStatus, SETTLE_DEPTH, SETTLE_GLUE_ERROR, SETTLE_LIMIT};
mod violations;
pub use glue::{Feature, Glue, MAX_GLUES};
pub use violations::{GlueViolation, PairViolation, ViolationReport, WallViolation};
#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub vx: f32,
    pub vy: f32,
    pub omega: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Gpu,
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub attraction: bool,
    pub edge_attraction: f32,
    pub damping: f32,
    pub stiffness: f32,
    pub band_tension: f32,
    pub target_side: f64,
}
#[derive(Clone, Copy, Default)]
struct Mouse {
    x: f32,
    y: f32,
    index: Option<usize>,
    down: bool,
}
#[derive(Clone, Copy, Default)]
struct Rotation {
    index: Option<usize>,
    target: f32,
    remaining: f32,
}
struct State {
    shape: shared::Shape,
    container: shared::Shape,
    interaction: interaction::Interaction,
    bodies: Vec<Body>,
    glues: Vec<Glue>,
    contact_forces: Vec<[f32; 2]>,
    side: f64,
    params: Params,
    mouse: Mouse,
    grabs: Vec<Mouse>,
    rotation: Rotation,
    band_velocity: f32,
    paused: bool,
    disposed: bool,
    revision: u64,
    #[cfg(target_arch = "wasm32")]
    gpu: Option<Rc<gpu::Gpu>>,
    #[cfg(target_arch = "wasm32")]
    initializing: bool,
    #[cfg(target_arch = "wasm32")]
    stepping: bool,
}
/// Clones share one simulation. Call `dispose` when the owning UI unmounts.
#[derive(Clone)]
pub struct Physics {
    state: Rc<RefCell<State>>,
}
impl Physics {
    pub fn new(n: u32, side: f64) -> Self {
        Self::new_for(shared::Shape::Square, n, side)
    }
    pub fn shape(&self) -> shared::Shape {
        self.state.borrow().shape
    }
    pub fn new_for(shape: shared::Shape, n: u32, side: f64) -> Self {
        Self::new_in(shape, shared::Shape::Square, n, side)
    }
    pub fn container(&self) -> shared::Shape {
        self.state.borrow().container
    }
    pub fn new_in(shape: shared::Shape, container: shared::Shape, n: u32, side: f64) -> Self {
        assert!((1..=shared::MAX_N).contains(&n));
        assert!(side.is_finite() && (container.area_bound(shape, 1)..=1000.0).contains(&side));
        let physics = Self {
            state: Rc::new(RefCell::new(State {
                shape,
                container,
                interaction: Default::default(),
                glues: Vec::new(),
                bodies: vec![Body::default(); n as usize],
                contact_forces: vec![[0.0; 2]; n as usize],
                side,
                params: Params {
                    attraction: false,
                    edge_attraction: 0.0,
                    damping: 3.0,
                    stiffness: 900.0,
                    band_tension: 0.0,
                    target_side: side,
                },
                mouse: Mouse::default(),
                grabs: Vec::new(),
                rotation: Rotation::default(),
                band_velocity: 0.0,
                paused: false,
                disposed: false,
                revision: 0,
                #[cfg(target_arch = "wasm32")]
                gpu: None,
                #[cfg(target_arch = "wasm32")]
                initializing: false,
                #[cfg(target_arch = "wasm32")]
                stepping: false,
            })),
        };
        physics.reset();
        physics
    }
    /// Attempts WebGPU initialization; CPU stays available throughout.
    pub async fn init_gpu(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let n = {
                let mut s = self.state.borrow_mut();
                if !s.shape.is_square()
                    || !s.container.is_square()
                    || s.disposed
                    || s.initializing
                    || s.gpu.is_some()
                {
                    return;
                }
                s.initializing = true;
                s.bodies.len()
            };
            let _guard = BusyGuard {
                state: Rc::downgrade(&self.state),
                initializing: true,
            };
            let gpu = gpu::Gpu::new(n).await;
            let mut s = self.state.borrow_mut();
            if !s.disposed {
                s.gpu = gpu.map(Rc::new);
            }
        }
    }
    pub fn mode(&self) -> Backend {
        if !self.state.borrow().grabs.is_empty() {
            return Backend::Cpu;
        }
        #[cfg(target_arch = "wasm32")]
        if self
            .state
            .borrow()
            .gpu
            .as_ref()
            .is_some_and(|gpu| gpu.alive())
        {
            return Backend::Gpu;
        }
        Backend::Cpu
    }
    pub async fn step(&self, steps: u32) {
        let steps = steps.min(6);
        if steps == 0 {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let snapshot = {
                let mut s = self.state.borrow_mut();
                if s.paused || s.disposed || s.stepping {
                    return;
                }
                if !s.grabs.is_empty() {
                    None
                } else if let Some(gpu) = s.gpu.clone().filter(|g| g.alive()) {
                    s.stepping = true;
                    Some((
                        gpu,
                        s.bodies.clone(),
                        s.params,
                        s.side,
                        s.mouse,
                        s.rotation,
                        s.revision,
                        s.glues.clone(),
                        s.band_velocity,
                    ))
                } else {
                    s.gpu = None;
                    None
                }
            };
            if let Some((
                gpu,
                initial,
                params,
                side,
                mouse,
                rotation,
                revision,
                glues,
                band_velocity,
            )) = snapshot
            {
                let _guard = BusyGuard {
                    state: Rc::downgrade(&self.state),
                    initializing: false,
                };
                let result = gpu
                    .step(
                        &initial,
                        params,
                        side,
                        (mouse, rotation, &glues, band_velocity),
                        steps,
                    )
                    .await;
                let mut s = self.state.borrow_mut();
                if s.disposed {
                    return;
                }
                if let Some((result, forces)) = result {
                    s.merge_readback(&initial, &result, revision);
                    s.rotation.remaining =
                        (s.rotation.remaining - steps as f32 * FIXED_STEP as f32).max(0.0);
                    // Telemetry is output only; never merge it into edited poses.
                    if s.revision == revision {
                        s.contact_forces = forces;
                    } else {
                        s.contact_forces.fill([0.0; 2]);
                    }
                    if s.revision == revision {
                        for _ in 0..steps {
                            s.step_band();
                        }
                    }
                } else {
                    s.gpu = None;
                    for _ in 0..steps {
                        if !s.paused {
                            s.cpu_step();
                        }
                    }
                }
                if s.revision == revision {
                    s.step_interaction(steps as f64 * FIXED_STEP);
                }
                return;
            }
        }
        let mut s = self.state.borrow_mut();
        if s.paused || s.disposed {
            return;
        }
        for _ in 0..steps {
            s.cpu_step();
        }
        s.step_interaction(steps as f64 * FIXED_STEP);
    }
    pub fn bodies(&self) -> Vec<Body> {
        self.state.borrow().bodies.clone()
    }
    pub fn side(&self) -> f64 {
        self.state.borrow().side
    }
    pub fn set_side(&self, side: f64) {
        if !side.is_finite()
            || !(self.container().area_bound(self.shape(), 1)..=1000.0).contains(&side)
        {
            return;
        }
        let mut s = self.state.borrow_mut();
        s.cancel_settle();
        s.side = side;
        s.band_velocity = 0.0;
        s.contact_forces.fill([0.0; 2]);
        s.revision += 1;
    }
    pub fn params(&self) -> Params {
        self.state.borrow().params
    }
    pub fn set_params(&self, p: Params) {
        if ![p.damping, p.stiffness, p.band_tension, p.edge_attraction]
            .iter()
            .all(|v| v.is_finite())
            || !p.target_side.is_finite()
        {
            return;
        }
        let mut s = self.state.borrow_mut();
        s.cancel_settle();
        if p.band_tension != s.params.band_tension {
            s.band_velocity = 0.0;
        }
        s.params = Params {
            damping: p.damping.clamp(0.0, 12.0),
            edge_attraction: p.edge_attraction.clamp(0.0, 40.0),
            stiffness: p.stiffness.clamp(300.0, 1600.0),
            band_tension: p.band_tension.clamp(0.0, 100.0),
            target_side: p
                .target_side
                .clamp(s.container.area_bound(s.shape, 1), 1000.0),
            ..p
        };
    }
    pub fn paused(&self) -> bool {
        self.state.borrow().paused
    }
    pub fn set_paused(&self, paused: bool) {
        let mut s = self.state.borrow_mut();
        s.cancel_settle();
        s.paused = paused;
        if paused {
            s.rotation = Rotation::default();
        }
    }
    /// Net collision and edge forces from the last substep, excluding
    /// center attraction, and the mouse spring. Output-only visualization data.
    pub fn contact_forces(&self) -> Vec<[f32; 2]> {
        self.state.borrow().contact_forces.clone()
    }
    /// The current capped mouse spring, including its velocity damping.
    pub fn mouse_force(&self) -> Option<(usize, [f32; 2])> {
        let s = self.state.borrow();
        let i = s.mouse.index.filter(|_| s.mouse.down)?;
        let b = s.bodies.get(i)?;
        let (dx, dy) = (s.mouse.x - b.x, s.mouse.y - b.y);
        let gain = 100.0 * (0.5 / dx.hypot(dy).max(0.0001)).min(1.0);
        Some((i, [dx * gain - b.vx * 14.0, dy * gain - b.vy * 14.0]))
    }
    pub fn band_velocity(&self) -> f32 {
        self.state.borrow().band_velocity
    }
    pub fn motion(&self) -> f32 {
        self.state
            .borrow()
            .bodies
            .iter()
            .map(|b| b.vx.abs() + b.vy.abs() + b.omega.abs())
            .sum()
    }
    pub fn set_mouse(&self, x: f32, y: f32, index: Option<usize>, down: bool) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let mut s = self.state.borrow_mut();
        if down && index.is_some_and(|i| i < s.bodies.len()) {
            s.cancel_settle_for_drag();
        }
        s.mouse = Mouse {
            x,
            y,
            index: index.filter(|i| *i < s.bodies.len()),
            down,
        };
    }
    /// Independent touch springs, one per body. Multitouch uses the CPU
    /// integrator while held; the GPU resumes from that live pose on release.
    pub fn set_grabs(&self, targets: &[(usize, f32, f32)]) {
        let mut s = self.state.borrow_mut();
        let mut grabs = Vec::new();
        for &(i, x, y) in targets.iter().take(s.bodies.len()) {
            if i < s.bodies.len()
                && x.is_finite()
                && y.is_finite()
                && !grabs.iter().any(|m: &Mouse| m.index == Some(i))
            {
                grabs.push(Mouse {
                    index: Some(i),
                    x,
                    y,
                    down: true,
                });
            }
        }
        if !grabs.is_empty() {
            s.cancel_settle_for_drag();
        }
        s.grabs = grabs;
    }
    pub fn set_pose(&self, i: usize, x: f32, y: f32, theta: f32) {
        if ![x, y, theta].iter().all(|v| v.is_finite()) {
            return;
        }
        let mut s = self.state.borrow_mut();
        if i < s.bodies.len() {
            s.cancel_settle();
        }
        if let Some(b) = s.bodies.get_mut(i) {
            b.x = x;
            b.y = y;
            b.theta = theta;
            s.contact_forces.fill([0.0; 2]);
            s.revision += 1;
        }
    }
    /// Request a short, bounded angular spring instead of editing the pose.
    /// Repeated inputs retain at most 0.35 radians of target lead, so a blocked
    /// square cannot accumulate a large hidden turn or discard GPU readbacks.
    pub fn turn(&self, i: usize, delta: f32) {
        if !delta.is_finite() {
            return;
        }
        let mut s = self.state.borrow_mut();
        if s.disposed {
            return;
        }
        if i < s.bodies.len() {
            s.cancel_settle();
        }
        let Some(b) = s.bodies.get(i) else {
            return;
        };
        let lead = if s.rotation.index == Some(i) && s.rotation.remaining > 0.0 {
            let error = s.rotation.target - b.theta;
            error.sin().atan2(error.cos())
        } else {
            0.0
        };
        s.rotation = Rotation {
            index: Some(i),
            target: b.theta + (lead + delta).clamp(-0.35, 0.35),
            remaining: 0.5,
        };
    }
    pub fn nudge(&self, i: usize, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        let mut s = self.state.borrow_mut();
        if i < s.bodies.len() {
            s.cancel_settle();
        }
        if let Some(b) = s.bodies.get_mut(i) {
            b.x += dx;
            b.y += dy;
            s.contact_forces.fill([0.0; 2]);
            s.revision += 1;
        }
    }
    pub fn shake(&self, seed: u64) {
        self.shake_scaled(seed, 1.0);
    }
    /// A bounded kick; amplitudes below one support cooling/annealing.
    pub fn shake_scaled(&self, mut seed: u64, strength: f32) {
        if !strength.is_finite() {
            return;
        }
        let strength = strength.clamp(0.0, 1.0);
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) as f32 / u32::MAX as f32) - 0.5
        };
        let mut s = self.state.borrow_mut();
        s.cancel_settle();
        for b in &mut s.bodies {
            b.vx = random() * 7.0 * strength;
            b.vy = random() * 7.0 * strength;
            b.omega = random() * 3.0 * strength;
        }
        s.contact_forces.fill([0.0; 2]);
        s.revision += 1;
    }
    pub fn reset(&self) {
        let mut s = self.state.borrow_mut();
        s.cancel_settle();
        let cols = (s.bodies.len() as f32).sqrt().ceil() as usize;
        let width = if s.container.is_square() {
            s.side
        } else {
            s.side * s.container.apothem() * 2.0f64.sqrt()
        };
        let offset = (s.side - width) as f32 / 2.0;
        let spacing = width as f32 / cols as f32;
        for (i, b) in s.bodies.iter_mut().enumerate() {
            *b = Body {
                x: offset + (i % cols) as f32 * spacing + spacing * 0.5,
                y: offset + (i / cols) as f32 * spacing + spacing * 0.5,
                ..Body::default()
            };
        }
        s.band_velocity = 0.0;
        s.mouse.down = false;
        s.grabs.clear();
        s.rotation = Rotation::default();
        s.glues.clear();
        s.contact_forces.fill([0.0; 2]);
        s.revision += 1;
    }
    pub fn arrangement(&self) -> shared::Arrangement {
        self.state.borrow().arrangement()
    }
    /// Loads a finite scene, including overlaps that the play solver can repair.
    pub fn load(&self, a: &shared::Arrangement) {
        let mut s = self.state.borrow_mut();
        if a.container != s.container
            || a.shape != s.shape
            || a.n as usize != s.bodies.len()
            || a.squares.len() != s.bodies.len()
            || !a.side.is_finite()
            || !(s.container.area_bound(s.shape, 1)..=1000.0).contains(&a.side)
            || a.squares.iter().any(|p| {
                !p.theta.is_finite()
                    || ![p.cx, p.cy]
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 1000.0)
            })
        {
            return;
        }
        s.cancel_settle();
        s.side = a.side;
        s.params.target_side = a.side;
        s.band_velocity = 0.0;
        s.mouse.down = false;
        s.grabs.clear();
        s.rotation = Rotation::default();
        s.glues.clear();
        for (b, p) in s.bodies.iter_mut().zip(&a.squares) {
            *b = Body {
                x: p.cx as f32,
                y: p.cy as f32,
                theta: p.theta.sin().atan2(p.theta.cos()) as f32,
                ..Body::default()
            };
        }
        s.contact_forces.fill([0.0; 2]);
        s.revision += 1;
    }
    pub fn dispose(&self) {
        let mut s = self.state.borrow_mut();
        s.cancel_settle();
        s.disposed = true;
        s.glues.clear();
        s.rotation = Rotation::default();
        #[cfg(target_arch = "wasm32")]
        if let Some(gpu) = s.gpu.take() {
            gpu.destroy();
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct BusyGuard {
    state: std::rc::Weak<RefCell<State>>,
    initializing: bool,
}
#[cfg(target_arch = "wasm32")]
impl Drop for BusyGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut s = state.borrow_mut();
            if self.initializing {
                s.initializing = false;
            } else {
                s.stepping = false;
            }
        }
    }
}
impl State {
    fn arrangement(&self) -> shared::Arrangement {
        shared::Arrangement {
            container: self.container,
            shape: self.shape,
            n: self.bodies.len() as u32,
            side: self.side,
            squares: self
                .bodies
                .iter()
                .map(|b| shared::Placement {
                    cx: b.x as f64,
                    cy: b.y as f64,
                    theta: b.theta as f64,
                })
                .collect(),
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn merge_readback(&mut self, initial: &[Body], computed: &[Body], revision: u64) {
        if self.revision == revision {
            self.bodies.copy_from_slice(computed);
            return;
        }
        for ((current, old), new) in self.bodies.iter_mut().zip(initial).zip(computed) {
            macro_rules! merge {($($field:ident),*)=>{$(if current.$field==old.$field {current.$field=new.$field;})*};}
            merge!(x, y, theta, vx, vy, omega);
        }
    }
}
