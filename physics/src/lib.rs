//! Browser physics implementation. The WebGPU kernel and CPU fallback share the
//! same oriented-square SAT model; the Rust frontend loads the browser module.
pub const MAX_SQUARES: usize = 128;
pub const FIXED_STEP: f64 = 1.0 / 120.0;
pub const GPU_KERNEL: &str = include_str!("../kernel.wgsl");
