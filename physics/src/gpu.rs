//! WebGPU owns body integration; only the scalar band coordinate stays on CPU.
use super::{Body, Mouse, Params, FIXED_STEP, GPU_KERNEL};
use std::{
    borrow::Cow,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub(super) struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    buffers: [wgpu::Buffer; 2],
    uniform: wgpu::Buffer,
    readback: wgpu::Buffer,
    groups: [wgpu::BindGroup; 2],
    lost: Arc<AtomicBool>,
}
impl Drop for Gpu {
    fn drop(&mut self) {
        self.device.destroy();
    }
}
impl Gpu {
    pub(super) async fn new(n: usize) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .ok()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .ok()?;
        let lost = Arc::new(AtomicBool::new(false));
        let flag = lost.clone();
        device.set_device_lost_callback(move |_, _| flag.store(true, Ordering::Relaxed));
        let flag = lost.clone();
        device.on_uncaptured_error(Arc::new(move |_| flag.store(true, Ordering::Relaxed)));
        let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("packing contacts"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(GPU_KERNEL)),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("packing step"),
            layout: None,
            module: &shader,
            entry_point: Some("step"),
            compilation_options: Default::default(),
            cache: None,
        });
        let size = (n * 32) as u64;
        let buffers = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("packing bodies"),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("packing parameters"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("packing readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = pipeline.get_bind_group_layout(0);
        let groups = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffers[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffers[1 - i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        });
        // Own the device before awaiting scopes, so cancellation destroys it too.
        let gpu = Self {
            device,
            queue,
            pipeline,
            buffers,
            uniform,
            readback,
            groups,
            lost,
        };
        // Pop all scopes even if the first reports an error.
        let invalid = validation.pop().await.is_some();
        let invalid = internal.pop().await.is_some() || invalid;
        let invalid = oom.pop().await.is_some() || invalid;
        if invalid || !gpu.alive() {
            return None;
        }
        Some(gpu)
    }

    pub(super) fn alive(&self) -> bool {
        !self.lost.load(Ordering::Relaxed)
    }
    pub(super) fn destroy(&self) {
        self.lost.store(true, Ordering::Relaxed);
        self.device.destroy();
    }
    pub(super) async fn step(
        &self,
        initial: &[Body],
        p: Params,
        side: f64,
        mouse: Mouse,
        steps: u32,
    ) -> Option<Vec<Body>> {
        if !self.alive() {
            return None;
        }
        let data: Vec<f32> = initial
            .iter()
            .flat_map(|b| [b.x, b.y, b.theta, 0.0, b.vx, b.vy, b.omega, 0.0])
            .collect();
        let parameters = [
            initial.len() as f32,
            FIXED_STEP as f32,
            side as f32,
            u32::from(p.gravity) as f32,
            u32::from(p.attraction) as f32 * 1.8,
            p.damping,
            p.stiffness,
            0.0,
            mouse.x,
            mouse.y,
            mouse.index.unwrap_or(0) as f32,
            if mouse.down && mouse.index.is_some() {
                1.0
            } else {
                0.0
            },
        ];
        self.queue
            .write_buffer(&self.buffers[0], 0, bytemuck::cast_slice(&data));
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&parameters));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for s in 0..steps {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.groups[s as usize % 2], &[]);
            pass.dispatch_workgroups((initial.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.buffers[steps as usize % 2],
            0,
            &self.readback,
            0,
            (data.len() * 4) as u64,
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = futures_channel::oneshot::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        // Browsers drive completion; unlike native wgpu, no blocking device.poll.
        let _mapping = ReadbackGuard(&self.readback);
        if rx.await.ok()?.is_err() {
            return None;
        }
        let mapped = self.readback.slice(..).get_mapped_range().ok()?;
        let result = bytemuck::cast_slice::<u8, f32>(&mapped)
            .chunks_exact(8)
            .map(|v| Body {
                x: v[0],
                y: v[1],
                theta: v[2],
                vx: v[4],
                vy: v[5],
                omega: v[6],
            })
            .collect();
        drop(mapped);
        Some(result)
    }
}

struct ReadbackGuard<'a>(&'a wgpu::Buffer);
impl Drop for ReadbackGuard<'_> {
    fn drop(&mut self) {
        self.0.unmap();
    }
}
