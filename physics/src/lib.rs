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
    pub gravity: bool,
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
struct State {
    bodies: Vec<Body>,
    contact_forces: Vec<[f32; 2]>,
    side: f64,
    params: Params,
    mouse: Mouse,
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
        assert!((1..=shared::MAX_N).contains(&n));
        assert!(side.is_finite() && (1.0..=1000.0).contains(&side));
        let physics = Self {
            state: Rc::new(RefCell::new(State {
                bodies: vec![Body::default(); n as usize],
                contact_forces: vec![[0.0; 2]; n as usize],
                side,
                params: Params {
                    gravity: false,
                    attraction: false,
                    edge_attraction: 0.0,
                    damping: 3.0,
                    stiffness: 900.0,
                    band_tension: 0.0,
                    target_side: side,
                },
                mouse: Mouse::default(),
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
                if s.disposed || s.initializing || s.gpu.is_some() {
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
                if let Some(gpu) = s.gpu.clone().filter(|g| g.alive()) {
                    s.stepping = true;
                    Some((gpu, s.bodies.clone(), s.params, s.side, s.mouse, s.revision))
                } else {
                    s.gpu = None;
                    None
                }
            };
            if let Some((gpu, initial, params, side, mouse, revision)) = snapshot {
                let _guard = BusyGuard {
                    state: Rc::downgrade(&self.state),
                    initializing: false,
                };
                let result = gpu.step(&initial, params, side, mouse, steps).await;
                let mut s = self.state.borrow_mut();
                if s.disposed {
                    return;
                }
                if let Some((result, forces)) = result {
                    s.merge_readback(&initial, &result, revision);
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
    }
    pub fn bodies(&self) -> Vec<Body> {
        self.state.borrow().bodies.clone()
    }
    pub fn side(&self) -> f64 {
        self.state.borrow().side
    }
    pub fn set_side(&self, side: f64) {
        if !side.is_finite() || !(1.0..=1000.0).contains(&side) {
            return;
        }
        let mut s = self.state.borrow_mut();
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
        if p.band_tension != s.params.band_tension {
            s.band_velocity = 0.0;
        }
        s.params = Params {
            damping: p.damping.clamp(0.0, 12.0),
            edge_attraction: p.edge_attraction.clamp(0.0, 40.0),
            stiffness: p.stiffness.clamp(300.0, 1600.0),
            band_tension: p.band_tension.clamp(0.0, 100.0),
            target_side: p.target_side.clamp(1.0, 1000.0),
            ..p
        };
    }
    pub fn paused(&self) -> bool {
        self.state.borrow().paused
    }
    pub fn set_paused(&self, paused: bool) {
        self.state.borrow_mut().paused = paused;
    }
    /// Net collision and edge forces from the last substep, excluding gravity,
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
        let gain = 100.0 * (0.4 / dx.hypot(dy).max(0.0001)).min(1.0);
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
        s.mouse = Mouse {
            x,
            y,
            index: index.filter(|i| *i < s.bodies.len()),
            down,
        };
    }
    pub fn set_pose(&self, i: usize, x: f32, y: f32, theta: f32) {
        if ![x, y, theta].iter().all(|v| v.is_finite()) {
            return;
        }
        let mut s = self.state.borrow_mut();
        if let Some(b) = s.bodies.get_mut(i) {
            b.x = x;
            b.y = y;
            b.theta = theta;
            s.contact_forces.fill([0.0; 2]);
            s.revision += 1;
        }
    }
    pub fn rotate(&self, i: usize, dtheta: f32) {
        if !dtheta.is_finite() {
            return;
        }
        let mut s = self.state.borrow_mut();
        if let Some(b) = s.bodies.get_mut(i) {
            b.theta += dtheta;
            s.contact_forces.fill([0.0; 2]);
            s.revision += 1;
        }
    }
    pub fn nudge(&self, i: usize, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        let mut s = self.state.borrow_mut();
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
        let cols = (s.bodies.len() as f32).sqrt().ceil() as usize;
        let spacing = s.side as f32 / cols as f32;
        for (i, b) in s.bodies.iter_mut().enumerate() {
            *b = Body {
                x: (i % cols) as f32 * spacing + spacing * 0.5,
                y: (i / cols) as f32 * spacing + spacing * 0.5,
                ..Body::default()
            };
        }
        s.band_velocity = 0.0;
        s.mouse.down = false;
        s.contact_forces.fill([0.0; 2]);
        s.revision += 1;
    }
    pub fn arrangement(&self) -> shared::Arrangement {
        let s = self.state.borrow();
        shared::Arrangement {
            n: s.bodies.len() as u32,
            side: s.side,
            squares: s
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
    /// Loads a finite scene, including overlaps that the play solver can repair.
    pub fn load(&self, a: &shared::Arrangement) {
        let mut s = self.state.borrow_mut();
        if a.n as usize != s.bodies.len()
            || a.squares.len() != s.bodies.len()
            || !a.side.is_finite()
            || !(1.0..=1000.0).contains(&a.side)
            || a.squares.iter().any(|p| {
                !p.theta.is_finite()
                    || ![p.cx, p.cy]
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 1000.0)
            })
        {
            return;
        }
        s.side = a.side;
        s.params.target_side = a.side;
        s.band_velocity = 0.0;
        s.mouse.down = false;
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
        s.disposed = true;
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
