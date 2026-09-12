//! Shared browser physics state: unit squares, bottom-left origin, radians.
use std::{cell::RefCell, rc::Rc};

pub const MAX_SQUARES: usize = shared::MAX_N as usize;
pub const FIXED_STEP: f64 = 1.0 / 120.0;
pub const GPU_KERNEL: &str = include_str!("../kernel.wgsl");
mod cpu;
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
    side: f64,
    params: Params,
    mouse: Mouse,
    band_velocity: f32,
    paused: bool,
    disposed: bool,
    revision: u64,
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
                side,
                params: Params {
                    gravity: false,
                    attraction: false,
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
            })),
        };
        physics.reset();
        physics
    }
    // The GPU implementation lands next; CPU is immediately usable.
    pub async fn init_gpu(&self) {}
    pub fn mode(&self) -> Backend {
        Backend::Cpu
    }
    pub async fn step(&self, steps: u32) {
        let mut s = self.state.borrow_mut();
        if s.paused || s.disposed {
            return;
        }
        for _ in 0..steps.min(6) {
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
        s.revision += 1;
    }
    pub fn params(&self) -> Params {
        self.state.borrow().params
    }
    pub fn set_params(&self, p: Params) {
        if ![p.damping, p.stiffness, p.band_tension]
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
            s.revision += 1;
        }
    }
    pub fn shake(&self, mut seed: u64) {
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) as f32 / u32::MAX as f32) - 0.5
        };
        let mut s = self.state.borrow_mut();
        for b in &mut s.bodies {
            b.vx = random() * 7.0;
            b.vy = random() * 7.0;
            b.omega = random() * 3.0;
        }
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
                ![p.cx, p.cy, p.theta]
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
                theta: p.theta as f32,
                ..Body::default()
            };
        }
        s.revision += 1;
    }
    pub fn dispose(&self) {
        self.state.borrow_mut().disposed = true;
    }
}
