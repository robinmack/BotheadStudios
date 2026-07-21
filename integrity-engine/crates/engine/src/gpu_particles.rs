//! **The GPU particle container** (`docs/22`, `docs/33`) — a storage buffer of grains stepped by
//! `shaders/particle_step.wgsl` and rendered from the SAME buffer (zero-copy sim↔render).
//!
//! **Why this is a module and not scene code.** It lived inside `#[cfg(target_arch = "wasm32")] mod app`
//! next to the terrain `Engine`, which made a general container look like one scene's private machinery
//! and kept it out of every native build. It is neither scene-specific nor wasm-specific: `wgpu`'s types
//! exist without a backend, so the host code type-checks natively (running it still needs a browser).
//! `gpu_sph` proved the pattern — its wasm-only gate turned out to be a single `Rc<Cell<bool>>`, and the
//! same one line was here.
//!
//! This is the **container** half of the realignment's convergence step (`docs/33`): `gpu_particles`
//! (granular) and `gpu_sph` (SPH) are now sibling modules in the same build, which is the precondition
//! for hosting both pipelines on one allocator/render path. Their SOLVERS stay specialized — stiff
//! granular contacts and smoothed-particle hydrodynamics are genuinely different physics, which
//! `docs/46` §1 sanctions; it is the duplicated CONTAINER that is the violation.

/// Spatial-hash configuration for the granular grid. Shared with the consumers that build a
/// `GpuStepParams` for this container (the terrain scene and `GpuProbe`), so it lives with the container
/// rather than in any one scene.
pub(crate) const GRID_TABLE_SIZE: u32 = 1 << 18; // spatial-hash cells (≥ ~2× particle capacity → few collisions)
pub(crate) const GRID_BUCKET_K: u32 = 16; // max particles recorded per cell (overflow is dropped — flagged)

/// Threads per workgroup, mirroring `@workgroup_size(N)` on every compute entry point in
/// `particle_step.wgsl`. The host turns a thread count into a workgroup count with this, so it is a
/// shader mirror like any `#[repr(C)]` struct — and it was an unguarded literal `64` in two dispatch
/// sites. If the shader's size were raised, the host would under-dispatch and a tail of grains would
/// silently never step (physics that quietly stops for some particles, no error); if lowered, it would
/// over-dispatch. `the_workgroup_size_matches_every_shader_entry_point` pins it.
pub(crate) const WORKGROUP: u32 = 64;

/// Particle capacity the scenes build this container with. It bounds the grid's load factor, so it
/// lives beside `GRID_TABLE_SIZE` and is checked against it by
/// `the_grid_table_is_large_enough_for_the_particle_capacity`.
pub(crate) const MAX_PARTICLES: usize = 60_000;
// ============================================================================================
// GPU-compute debris particles (docs/22). Particles live in a storage buffer stepped by a compute
// shader (one thread each) and rendered from the SAME buffer (zero-copy sim↔render). This is the
// engine's north-star architecture and the fix for the single-digit FPS after a big impact.
// ============================================================================================

use crate::gpu_layout::{GpuParticle, GpuStepParams};

/// GPU-resident debris: a storage+vertex buffer of `GpuParticle`, a compute pipeline that steps it,
/// and a heightfield the step collides against. The CPU only appends new particles (on fracture)
/// and updates the per-frame params; the physics runs entirely on the GPU.
pub(crate) struct GpuParticles {
    buf: wgpu::Buffer,         // STORAGE — the PHYSICS grains (1 per voxel), stepped
    pub(crate) render_buf: wgpu::Buffer, // STORAGE | VERTEX — 8× render sub-cubes (cs_expand fills it), drawn
    params: wgpu::Buffer,     // UNIFORM | COPY_DST
    heightfield: wgpu::Buffer, // STORAGE | COPY_DST
    grid_count: wgpu::Buffer, // STORAGE — atomic per-cell particle count (spatial hash)
    grid_bucket: wgpu::Buffer, // STORAGE — per-cell particle indices
    forces: wgpu::Buffer,     // STORAGE — accumulated contact acceleration per particle
    clear: wgpu::ComputePipeline,
    insert: wgpu::ComputePipeline,
    sort: wgpu::ComputePipeline,
    force_pass: wgpu::ComputePipeline,
    integrate: wgpu::ComputePipeline,
    expand: wgpu::ComputePipeline, // 1 grain → 8 render sub-cubes
    bind: wgpu::BindGroup,
    capacity: u32,
    pub(crate) count: u32,
    // Non-blocking readback (docs/22 de-resolution). On WebGPU buffer mapping is genuinely async —
    // we cannot block (`Maintain::Wait` is a no-op in the browser), so the readback is two-phase:
    // `begin_readback` copies the grains into `readback_staging` and calls `map_async`, whose
    // callback flips `readback_ready`; a later frame `take_readback` reads the mapped bytes.
    // `readback_count` snapshots `count` at copy time so `take_readback` can detect an intervening
    // append (a fresh meteor) and discard the now-misaligned snapshot rather than deposit stale data.
    readback_staging: Option<wgpu::Buffer>,
    pub(crate) readback_count: u32,
    /// See `gpu_sph::GpuSph::readback_ready`: wgpu bounds the `map_async` callback by `WasmNotSend`,
    /// which is a no-op on wasm but plain `Send` everywhere else — so the `Rc<Cell<bool>>` this used to
    /// be compiled ONLY for wasm, and was the single line pinning this container inside `mod app`.
    /// Release/Acquire, because the callback publishes a completed mapping that `take_readback` then
    /// reads through `get_mapped_range`.
    readback_ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl GpuParticles {
    pub(crate) fn new(device: &wgpu::Device, capacity: u32, world_cells: u32) -> Self {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-particles-physics"),
            size: (capacity as usize * std::mem::size_of::<GpuParticle>()) as u64,
            // COPY_SRC so the de-resolution readback (docs/22) can copy the live grains to a MAP_READ
            // staging buffer — without it copy_buffer_to_buffer is a (silent, async) WebGPU validation
            // error and the readback reads all zeros.
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        // 8× render sub-cubes, filled each frame by cs_expand and drawn as instances.
        let render_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-particles-render"),
            size: (capacity as usize * 8 * std::mem::size_of::<GpuParticle>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-particles-params"),
            size: std::mem::size_of::<GpuStepParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let heightfield = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-particles-heightfield"),
            size: (world_cells as usize * std::mem::size_of::<i32>()).max(4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Spatial-hash grid + per-particle contact-force scratch (docs/23).
        let grid_count = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-grid-count"),
            size: (GRID_TABLE_SIZE as u64) * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let grid_bucket = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-grid-bucket"),
            size: (GRID_TABLE_SIZE as u64)
                * (GRID_BUCKET_K as u64)
                * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let forces = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-particle-forces"),
            // `Accum` (particle_step.wgsl): contact force + the 6-component stiffness/damping tensor
            // S = Σ g·(n⊗n) + the momentum-coupling vector Σ S·v_neighbor for the DIRECTIONAL implicit
            // solve — 64 bytes (four 16-byte rows).
            size: (capacity as u64) * 64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle-step"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/particle_step.wgsl").into(),
            ),
        });

        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu-particles-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1, false), // particles (physics)
                storage(2, true),  // heightfield
                storage(3, false), // grid_count
                storage(4, false), // grid_bucket
                storage(5, false), // forces
                storage(6, false), // render_out (8× render sub-cubes)
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-particles-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let mk = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let clear = mk("cs_grid_clear");
        let insert = mk("cs_grid_insert");
        let sort = mk("cs_grid_sort");
        let force_pass = mk("cs_forces");
        let integrate = mk("cs_integrate");
        let expand = mk("cs_expand");
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu-particles-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: heightfield.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: grid_count.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: grid_bucket.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: forces.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: render_buf.as_entire_binding(),
                },
            ],
        });

        GpuParticles {
            buf,
            render_buf,
            params,
            heightfield,
            grid_count,
            grid_bucket,
            forces,
            clear,
            insert,
            sort,
            force_pass,
            integrate,
            expand,
            bind,
            capacity,
            count: 0,
            readback_staging: None,
            readback_count: 0,
            readback_ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Upload the terrain heightfield (per-column air-start Y) the step collides against.
    pub(crate) fn upload_heightfield(&self, queue: &wgpu::Queue, tops: &[i32]) {
        queue.write_buffer(&self.heightfield, 0, bytemuck::cast_slice(tops));
    }

    /// Append newly-spawned particles (from a fracture) to the GPU buffer. Silently caps at
    /// capacity for now (no recycling yet — docs/22).
    pub(crate) fn append(&mut self, queue: &wgpu::Queue, new: &[GpuParticle]) {
        let room = (self.capacity - self.count) as usize;
        let take = new.len().min(room);
        if take == 0 {
            return;
        }
        let offset = self.count as u64 * std::mem::size_of::<GpuParticle>() as u64;
        queue.write_buffer(&self.buf, offset, bytemuck::cast_slice(&new[..take]));
        self.count += take as u32;
    }

    /// Record one substep into `encoder`: rebuild the spatial hash, accumulate granular contact
    /// SORT the buckets into a canonical order, accumulate granular contact forces, then integrate
    /// (gravity + contact + terrain). Five passes so force-accumulation (positions read-only) never
    /// races integration (docs/23). Params already written this frame.
    pub(crate) fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.count == 0 {
            return;
        }
        // Each stage is its OWN compute pass. The stages have strict data dependencies (insert
        // writes the grid that forces reads; forces writes the accelerations that integrate reads),
        // and a memory barrier between dependent dispatches is only GUARANTEED at pass boundaries.
        // Four dispatches in one pass happened to work on desktop Vulkan (the 2070) but can RACE on
        // other backends (e.g. Metal / the M4) — reading a half-built grid or stale forces injects
        // energy (a "matter fountain"). Separate passes force the barrier on every backend (docs/23).
        let stages: [(&wgpu::ComputePipeline, u32); 5] = [
            (&self.clear, GRID_TABLE_SIZE),
            (&self.insert, self.count),
            // DETERMINISM (docs/47): `insert` takes its slot from `atomicAdd`, so bucket ORDER is
            // whichever thread won the race. `force_pass` then sums contacts in that order and float
            // addition is not associative, so identical input gave different output run to run —
            // measured at ~6% on gpu-verify scene E, which left the FUDGE DETECTOR's tolerance wider
            // than its own reproducibility. Sorting the buckets makes the reduction a function of the
            // data alone. It is also its own pass for the same barrier reason as the rest.
            (&self.sort, GRID_TABLE_SIZE),
            (&self.force_pass, self.count),
            (&self.integrate, self.count),
        ];
        for (pipeline, threads) in stages {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("particle-stage"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.dispatch_workgroups(threads.div_ceil(WORKGROUP), 1, 1);
        }
    }

    /// Fill `render_buf` with 8 sub-cubes per physics grain. Run ONCE per frame after the substeps
    /// (the sub-cubes only need the settled positions) — a render-only subdivision.
    pub(crate) fn expand(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.count == 0 {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("particle-expand"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.expand);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.dispatch_workgroups(self.count.div_ceil(WORKGROUP), 1, 1);
    }

    pub(crate) fn set_params(&self, queue: &wgpu::Queue, params: &GpuStepParams) {
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(params));
    }

    /// Phase 1 of the non-blocking de-resolution readback (`docs/22`): copy the live PHYSICS grains
    /// into a transient `MAP_READ` staging buffer and kick off `map_async`. Its callback flips the
    /// shared `readback_ready` flag when the GPU has finished (on WebGPU that lands during the JS
    /// event loop between frames — we can NOT block for it, unlike native `tools/gpu-verify`). A
    /// no-op if the buffer is empty or a readback is already in flight.
    pub(crate) fn begin_readback(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.count == 0 || self.readback_staging.is_some() {
            return;
        }
        let size = self.count as u64 * std::mem::size_of::<GpuParticle>() as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-particles-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&self.buf, 0, &staging, 0, size);
        queue.submit(std::iter::once(enc.finish()));
        self.readback_ready.store(false, std::sync::atomic::Ordering::Release);
        let flag = self.readback_ready.clone();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if res.is_ok() {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        });
        self.readback_count = self.count;
        self.readback_staging = Some(staging);
    }

    /// Phase 2: if the in-flight readback has completed, return the snapshotted grains (a `Vec` of the
    /// `readback_count` grains as they were at copy time) and clear the in-flight state. Returns
    /// `None` while still pending or when nothing is in flight. If the live buffer was appended to
    /// since the copy (a new meteor), the snapshot no longer aligns with the buffer, so the caller
    /// must NOT compact against it — `take_readback` reports the snapshot count via `readback_count`
    /// so the caller can detect the mismatch.
    pub(crate) fn take_readback(&mut self) -> Option<Vec<GpuParticle>> {
        if !self.readback_ready.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        let staging = self.readback_staging.take()?;
        let data = staging.slice(..).get_mapped_range();
        let out = bytemuck::cast_slice::<u8, GpuParticle>(&data).to_vec();
        drop(data);
        staging.unmap();
        self.readback_ready.store(false, std::sync::atomic::Ordering::Release);
        Some(out)
    }

    /// Replace the buffer contents with `survivors` and set `count` to their number — the compaction
    /// half of de-resolution. Grains that settled back into voxels (CPU-side) are simply not in
    /// `survivors`, so `count` drops; the tail past the new count is left as-is (never stepped/drawn
    /// because every pass bounds itself by `count`). Matter is NOT destroyed here — the caller has
    /// already turned each removed grain into a voxel; this only shrinks the live GPU set.
    pub(crate) fn replace(&mut self, queue: &wgpu::Queue, survivors: &[GpuParticle]) {
        let take = survivors.len().min(self.capacity as usize);
        if take > 0 {
            queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&survivors[..take]));
        }
        self.count = take as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = include_str!("../../../shaders/particle_step.wgsl");

    /// `WORKGROUP` is a shader mirror with no type to protect it: the host converts a thread count into
    /// a workgroup count, so if the two disagree the GPU silently steps the wrong number of grains. Too
    /// few workgroups is the dangerous direction — a tail of particles never runs, which reads as
    /// physics quietly stopping for some matter rather than as an error.
    ///
    /// None of this was checkable while the container sat in `#[cfg(wasm32)] mod app`.
    #[test]
    fn the_workgroup_size_matches_every_shader_entry_point() {
        let sizes: Vec<u32> = SHADER
            .match_indices("@workgroup_size(")
            .map(|(i, m)| {
                let rest = &SHADER[i + m.len()..];
                rest[..rest.find(')').expect("unterminated @workgroup_size")]
                    .trim()
                    .parse()
                    .expect("non-numeric workgroup size")
            })
            .collect();

        assert!(!sizes.is_empty(), "parsed no @workgroup_size — the guard would pass vacuously");
        assert_eq!(
            sizes.len(),
            SHADER.matches("@compute").count(),
            "every @compute entry point must declare a workgroup size for this check to cover it"
        );
        for (i, s) in sizes.iter().enumerate() {
            assert_eq!(
                *s, WORKGROUP,
                "entry point {i} declares @workgroup_size({s}) but the host dispatches in units of \
                 {WORKGROUP}; grains would be skipped or over-dispatched"
            );
        }
    }

    /// The spatial hash degrades as it fills: `GRID_BUCKET_K` slots per cell, and overflow is DROPPED
    /// (a dropped entry is a contact that never happens). The table must stay comfortably larger than
    /// the particle capacity for that to remain rare. Pinned as a ratio so raising `MAX_PARTICLES`
    /// without resizing the table fails here rather than as missing contacts on a busy frame.
    #[test]
    fn the_grid_table_is_large_enough_for_the_particle_capacity() {
        const SCENE_CAPACITY: u32 = MAX_PARTICLES as u32;
        assert!(
            GRID_TABLE_SIZE >= 2 * SCENE_CAPACITY,
            "grid table {GRID_TABLE_SIZE} is under 2x the {SCENE_CAPACITY} particle capacity; cells \
             fill and `GRID_BUCKET_K` overflow silently drops contacts"
        );
        assert!(GRID_TABLE_SIZE.is_power_of_two(), "the shader masks with `table_mask` = size - 1");
    }
}
