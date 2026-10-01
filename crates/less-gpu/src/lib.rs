//! GPU compute kernels (wgpu).
//!
//! LessDB offloads scan-side arithmetic — filtered sums and dot products,
//! the hot loops behind `WHERE` + `SUM` and join costing — to the GPU when
//! one is available (Metal on macOS, Vulkan on Linux), with transparent CPU
//! fallback. The kernels use f32 storage; widening to f64 happens behind a
//! device feature check on supported hardware.
//!
//! This crate is optional (`--features gpu`) so the core engine builds
//! everywhere, including headless CI.

use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use less_common::{LessError, Result};
use wgpu::util::DeviceExt;

/// A GPU device with the LessDB kernel pipeline.
pub struct GpuDevice {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline_sum: wgpu::ComputePipeline,
    pipeline_dot: wgpu::ComputePipeline,
    name: String,
}

const WORKGROUP_SIZE: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    n: u32,
}

impl GpuDevice {
    /// Initialize the default adapter (Metal/Vulkan).
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| LessError::Gpu(e.to_string()))?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| LessError::Gpu(e.to_string()))?;
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders.wgsl"));
        let pipeline_sum = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("lessdb.filtered_sum"),
            layout: None,
            module: &shader,
            entry_point: Some("filtered_sum"),
            compilation_options: Default::default(),
            cache: None,
        });
        let pipeline_dot = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("lessdb.dot"),
            layout: None,
            module: &shader,
            entry_point: Some("dot"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            pipeline_sum,
            pipeline_dot,
            name: format!("{} ({:?})", info.name, info.backend),
        })
    }

    /// Human-readable device description.
    pub fn info(&self) -> &str {
        &self.name
    }

    fn run_kernel(
        &self,
        pipeline: &wgpu::ComputePipeline,
        n: usize,
        a: &[f32],
        b: &[u32],
    ) -> Result<f32> {
        let n_u32 = n as u32;
        let n_workgroups = n_u32.div_ceil(WORKGROUP_SIZE).clamp(
            1,
            wgpu::Limits::default().max_compute_workgroups_per_dimension,
        );

        let a_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("a"),
                contents: bytemuck::cast_slice(a),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let b_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("b"),
                contents: bytemuck::cast_slice(b),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let partials = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("partials"),
                contents: bytemuck::cast_slice(&vec![0.0f32; n_workgroups as usize]),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            });
        let params = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("params"),
                contents: bytemuck::bytes_of(&Params { n: n_u32 }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kernel_bind"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: a_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: b_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: partials.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lessdb"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("lessdb.kernel"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(n_workgroups, 1, 1);
        }
        self.queue.submit(Some(encoder.finish()));

        // Read partials back and finish the reduction on the CPU.
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: partials.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy"),
            });
        encoder.copy_buffer_to_buffer(&partials, 0, &staging, 0, partials.size());
        self.queue.submit(Some(encoder.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::Wait);
        rx.recv()
            .map_err(|e| LessError::Gpu(e.to_string()))?
            .map_err(|e| LessError::Gpu(e.to_string()))?;
        let data = slice.get_mapped_range();
        let partials: &[f32] = bytemuck::cast_slice(&data);
        Ok(partials.iter().sum())
    }

    /// `sum(values[i])` where `mask[i]` is true.
    pub fn filtered_sum_f32(&self, values: &[f32], mask: &[bool]) -> Result<f32> {
        if values.is_empty() {
            return Ok(0.0);
        }
        let n = values.len().min(mask.len());
        let mask_u32: Vec<u32> = mask[..n].iter().map(|b| *b as u32).collect();
        self.run_kernel(&self.pipeline_sum, n, &values[..n], &mask_u32)
    }

    /// Dot product of two f32 vectors.
    pub fn dot_f32(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.is_empty() {
            return Ok(0.0);
        }
        let n = a.len().min(b.len());
        let b_u32: Vec<u32> = b[..n].iter().map(|v| v.to_bits()).collect();
        self.run_kernel(&self.pipeline_dot, n, &a[..n], &b_u32)
    }
}

/// CPU reference implementations (for comparison and fallback).
pub mod cpu {
    pub fn filtered_sum_f32(values: &[f32], mask: &[bool]) -> f32 {
        values
            .iter()
            .zip(mask.iter())
            .filter(|(_, m)| **m)
            .map(|(v, _)| v)
            .sum()
    }

    pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
}

/// Built-in GPU vs CPU benchmark over `rows` elements.
pub async fn benchmark(rows: usize) -> Result<()> {
    let mut rng = Rng::new(0x0DB_0DB);
    let values: Vec<f32> = (0..rows).map(|_| rng.f() * 2.0 - 1.0).collect();
    let mask: Vec<bool> = (0..rows).map(|_| !rng.next().is_multiple_of(4)).collect();
    let b: Vec<f32> = (0..rows).map(|_| rng.f()).collect();

    let t = std::time::Instant::now();
    let cpu_sum = cpu::filtered_sum_f32(&values, &mask);
    println!(
        "cpu:  filtered_sum {rows} rows in {:?} = {cpu_sum:.3}",
        t.elapsed()
    );

    let device = GpuDevice::new().await?;
    println!("gpu:  {}", device.info());
    let t = std::time::Instant::now();
    let gpu_sum = device.filtered_sum_f32(&values, &mask)?;
    println!(
        "gpu:  filtered_sum {rows} rows in {:?} = {gpu_sum:.3}",
        t.elapsed()
    );

    let t = std::time::Instant::now();
    let cpu_dot = cpu::dot_f32(&values, &b);
    println!("cpu:  dot {rows} rows in {:?} = {cpu_dot:.3}", t.elapsed());
    let t = std::time::Instant::now();
    let gpu_dot = device.dot_f32(&values, &b)?;
    println!("gpu:  dot {rows} rows in {:?} = {gpu_dot:.3}", t.elapsed());
    Ok(())
}

/// Tiny xorshift RNG.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}
