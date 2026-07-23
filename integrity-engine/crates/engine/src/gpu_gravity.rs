//! **GPU direct-sum self-gravity — the engine's GPU backend for a per-particle N-body force.**
//!
//! Self-gravity on a debris cloud is embarrassingly parallel — every particle sums the pull of every
//! other — yet the orbital scene was walking a Barnes-Hut tree on a SINGLE CPU THREAD (34 ms/pass at
//! N=3000, measured; 64% of the frame). A GPU does the same sum in microseconds, in parallel, and
//! EXACTLY: there is no opening-angle multipole error, so it is higher fidelity as well as far faster.
//!
//! This wraps the already-verified `cs_gravity_direct` kernel in `shaders/bh_gravity.wgsl` (checked
//! against the CPU direct sum in `tools/gpu-bh-verify`). It is the piece a foreseen collision dispatches
//! to: the engine forecasts the impact (`interaction::detect`), materialises the particles, and — above
//! the CPU/GPU knee (~2,000 bodies) — steps their gravity here instead of on the CPU. Birth already runs
//! its SPH gravity on the GPU this way; this brings the orbital debris onto the same footing.
//!
//! **Precision.** The kernel is f32 (the GPU's native width); the CPU reference is f64. For self-gravity
//! that difference is far below the chaos/theta noise the disk already tolerates — the correctness test
//! pins the agreement.

use glam::DVec3;
use wgpu::util::DeviceExt;

/// Matches `Params` in `bh_gravity.wgsl`. Only `n` and `soft2` matter for the direct sum; the rest belong
/// to the tree kernels in the same file and are left zero here.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    theta: f32,
    soft2: f32,
    n_leaves: u32,
    bucket_k: u32,
    _pad: [u32; 3],
}

/// A reusable GPU direct-sum gravity pipeline. Build once; call [`GpuGravity::accelerations`] per step.
pub struct GpuGravity {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

impl GpuGravity {
    /// Build the pipeline for the `cs_gravity_direct` entry point of `bh_gravity.wgsl`.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu-gravity"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/bh_gravity.wgsl").into()),
        });
        // cs_gravity_direct touches only bindings 0 (Params), 1 (bodies), 2 (acc).
        let entry = |binding: u32, read_only: bool, uniform: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: if uniform {
                    wgpu::BufferBindingType::Uniform
                } else {
                    wgpu::BufferBindingType::Storage { read_only }
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu-gravity-layout"),
            entries: &[entry(0, true, true), entry(1, true, false), entry(2, false, false)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-gravity-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gpu-gravity"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_gravity_direct"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self { pipeline, layout }
    }

    /// Softened self-gravity acceleration on every body, computed on the GPU. `softening` is the Plummer
    /// length (m). Synchronous: uploads the bodies, dispatches, and reads the result back with a blocking
    /// poll — correct for a native step and for measurement. The LIVE browser frame keeps the field
    /// resident on the GPU instead (no per-call readback); this is the backend that field is stepped by.
    pub fn accelerations(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: &[DVec3],
        mass: &[f64],
        softening: f64,
    ) -> Vec<DVec3> {
        let n = pos.len();
        if n == 0 {
            return Vec::new();
        }
        let bodies: Vec<[f32; 4]> = pos
            .iter()
            .zip(mass)
            .map(|(p, &m)| [p.x as f32, p.y as f32, p.z as f32, m as f32])
            .collect();
        let params = Params {
            n: n as u32,
            theta: 0.0,
            soft2: (softening * softening) as f32,
            n_leaves: n as u32,
            bucket_k: 1,
            _pad: [0; 3],
        };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gravity-params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bodies_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gravity-bodies"),
            contents: bytemuck::cast_slice(&bodies),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let acc_size = (n * 16) as u64; // vec4<f32>
        let acc_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gravity-acc"),
            size: acc_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gravity-bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: bodies_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: acc_buf.as_entire_binding() },
            ],
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gravity-dispatch"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((n as u32).div_ceil(64), 1, 1);
        }
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gravity-staging"),
            size: acc_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(&acc_buf, 0, &staging, 0, acc_size);
        queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let raw: Vec<[f32; 4]> = bytemuck::cast_slice(&slice.get_mapped_range()).to_vec();
        raw.iter().map(|a| DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_direct(pos: &[DVec3], mass: &[f64], soft: f64) -> Vec<DVec3> {
        let s2 = soft * soft;
        (0..pos.len())
            .map(|i| {
                let mut a = DVec3::ZERO;
                for j in 0..pos.len() {
                    if i == j {
                        continue;
                    }
                    let d = pos[j] - pos[i];
                    let r2 = d.length_squared() + s2;
                    a += d * (crate::orbit::G * mass[j] / (r2 * r2.sqrt()));
                }
                a
            })
            .collect()
    }

    /// **The GPU gravity must equal the CPU brute-force sum.** Same softened Newtonian force, so the only
    /// difference is f32-vs-f64 rounding — far below the noise the disk already lives with. If this
    /// drifts, the backend is computing different physics, and no speedup is worth that. Skips cleanly
    /// when no GPU is present.
    #[test]
    fn gpu_gravity_matches_the_cpu_direct_sum() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU device - skipping gpu_gravity correctness test");
            return;
        };
        let mut s = 0x51ED_2707u64;
        let mut rng = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 40) as f64 / (1u64 << 24) as f64 - 0.5
        };
        let n = 1500;
        let pos: Vec<DVec3> = (0..n)
            .map(|_| DVec3::new(rng(), rng(), rng()).normalize_or_zero() * (1.7e6 * (0.2 + rng().abs())))
            .collect();
        let mass: Vec<f64> = (0..n).map(|_| 5.0e19).collect();
        let soft = 2.5e4;

        let g = GpuGravity::new(&host.device);
        let gpu = g.accelerations(&host.device, &host.queue, &pos, &mass, soft);
        let cpu = cpu_direct(&pos, &mass, soft);

        let mut worst = 0.0f64;
        for (a, b) in gpu.iter().zip(&cpu) {
            let denom = b.length().max(1e-30);
            worst = worst.max((*a - *b).length() / denom);
        }
        assert!(
            worst < 1e-3,
            "GPU direct-sum gravity must match the CPU sum to f32 precision; worst relative error {worst:.2e}"
        );
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    /// **GPU vs CPU Barnes-Hut at production N**, so the dispatch win is measured, not asserted. Reports
    /// the GPU per-call time INCLUDING the upload+dispatch+readback round trip — the honest cost of the
    /// synchronous backend — against the 34 ms CPU tree eval from the baseline.
    ///
    /// Run: `INTEGRITY_ADAPTER=5060 cargo test --lib gpu_gravity_speedup -- --ignored --nocapture`
    #[test]
    #[ignore = "perf bench — needs a GPU; run with --ignored --nocapture"]
    fn gpu_gravity_speedup() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU — skipping");
            return;
        };
        let mut s = 0xBEEF_1234u64;
        let mut rng = || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s >> 40) as f64 / (1u64 << 24) as f64 - 0.5 };
        let g = GpuGravity::new(&host.device);
        for &n in &[1500usize, 3000usize, 6000usize] {
            let pos: Vec<DVec3> = (0..n).map(|_| DVec3::new(rng(), rng(), rng()).normalize_or_zero() * 1.7e6).collect();
            let mass: Vec<f64> = (0..n).map(|_| 5.0e19).collect();
            // Warm up (shader/pipeline, first alloc).
            let _ = g.accelerations(&host.device, &host.queue, &pos, &mass, 2.5e4);
            let t = Instant::now();
            for _ in 0..10 { let _ = g.accelerations(&host.device, &host.queue, &pos, &mass, 2.5e4); }
            let gpu_ms = t.elapsed().as_secs_f64() * 1e3 / 10.0;
            // CPU Barnes-Hut, the current path.
            let bh = crate::bhtree::BarnesHut::build(&pos, &mass, 0.5, 2.5e4);
            let t = Instant::now();
            for _ in 0..3 { let _ = bh.accelerations(&pos, &mass); }
            let cpu_ms = t.elapsed().as_secs_f64() * 1e3 / 3.0;
            println!("  N={n:<5} GPU {gpu_ms:6.2} ms (incl round trip)   CPU-BH {cpu_ms:6.2} ms   -> {:.1}x", cpu_ms / gpu_ms.max(1e-6));
        }
    }
}
