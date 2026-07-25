//! **GPU direct-sum self-gravity — the engine's GPU backend for a per-particle N-body force.**
//!
//! Self-gravity on a debris cloud is embarrassingly parallel — every particle sums the pull of every
//! other — yet the orbital scene was walking a Barnes-Hut tree on a SINGLE CPU THREAD (34 ms/pass at
//! N=3000, measured; 64% of the frame). A GPU does the same sum in microseconds, in parallel, and
//! EXACTLY: there is no opening-angle multipole error, so it is higher fidelity as well as far faster.
//!
//! This wraps the already-verified kernels in `shaders/bh_gravity.wgsl` (checked stage by stage in
//! `tools/gpu-bh-verify`): the exact `cs_gravity_direct` sum, and above [`TREE_KNEE`] the LBVH
//! Barnes-Hut tree pipeline (`cs_tree_reset`/`cs_tree`/`cs_com`/`cs_gravity_bh`), because the quadratic
//! sum eventually loses to the O(N log N) tree even on a GPU (docs/37 measured the crossover shape;
//! `gpu_tree_speedup` measures it on this box). It is the piece a foreseen collision dispatches
//! to: the engine forecasts the impact (`interaction::detect`), materialises the particles, and, at or
//! above the measured [`DIRECT_SUM_KNEE`], steps their self-gravity here instead of on the CPU
//! ([`crate::aggregate::Aggregate`] carries the [`GravityField`] and makes the shunt per pass). Birth
//! already runs its SPH gravity on the GPU this way; this brings the orbital debris onto the same footing.
//!
//! **Precision.** The kernels are f32 (the GPU's native width); the CPU reference is f64. For
//! self-gravity that difference is far below the chaos/theta noise the disk already tolerates; the
//! correctness tests pin the agreement. The tree path additionally carries the theta multipole error,
//! the SAME bounded approximation (theta = 0.5, RMS < 1%) the CPU tree it replaces already made; the
//! tests pin that bound too, and theta -> 0 recovering the exact sum pins the tree's structure.

use glam::DVec3;
use wgpu::util::DeviceExt;

/// **The measured CPU/GPU knee for the direct-sum dispatch** (particle count). Below it the CPU is
/// genuinely faster and the aggregate stays on its brute/tree path; at or above it the GPU direct sum is
/// dispatched. Read off `gpu_gravity_speedup`, not guessed: on an Apple M4 Max (Metal) the GPU per-call
/// round trip is ~1.3 ms and nearly flat, while the CPU cost per pass crosses it between N=400 (0.5x,
/// CPU wins) and N=750 (2.0x, GPU wins); equal-cost interpolation lands at N of roughly 550. The
/// discrete-card numbers already recorded in this file's bench history (RTX 5060 Ti: ~2.5 ms floor
/// against a 48 ms single-thread brute sum at N=3000) put their crossover in the same few-hundred
/// range, so 600 sits on the CPU-favoured side of every measured box. Above the knee the dispatch is
/// never slower than the tree on any measured hardware, and it is EXACT (no theta multipole error):
/// higher fidelity and higher speed together.
pub const DIRECT_SUM_KNEE: usize = 600;

/// **The measured direct-vs-tree knee on the GPU** (particle count). At or above it the dispatch runs
/// the LBVH Barnes-Hut tree (`cs_gravity_bh`) instead of the exact direct sum: the direct sum is O(N^2)
/// and eventually loses to the O(N log N) tree even though the tree pays a CPU Morton sort and a
/// divergent traversal per call. Read off `gpu_tree_speedup`, not guessed: on an Apple M4 Max (Metal),
/// over three runs, the two are a dead heat at N=12000 (0.98x/1.02x/1.19x), the tree wins at every
/// N=24000 measurement (5.5-6.6 ms direct vs 4.8-5.3 ms tree, 1.14x-1.25x) and pulls away as theory
/// says it must (~2x at 48k, ~5x at 96k, 9-12x at 192k, direct ~400 ms vs tree 35-46 ms). The knee
/// sits at the first DECIDED N, not the dead heat, because below it the direct sum is exact as well as
/// even: where the measurements tie, fidelity breaks the tie. docs/37 measured the same crossover shape
/// on an RTX 2070 (traversal-only crossover N~128k); the knee is per-box, and this constant carries the
/// number for the box that measured it.
pub const TREE_KNEE: usize = 24_000;

/// Barnes-Hut opening angle for the GPU tree: the SAME theta as the CPU tree in `Aggregate` (0.5), the
/// value `tools/gpu-bh-verify` verified to RMS < 1% against the f64 direct sum. One question, one answer:
/// the GPU tree must not be a second opinion on how coarse a multipole is acceptable.
const THETA: f32 = 0.5;

/// Particles per LBVH leaf bucket. K=1 is the classic one-particle-per-leaf tree; docs/37 measured that
/// bucketing raises traversal cost more than it saves on this workload, and the Metal sweep in
/// `gpu_tree_speedup` agrees (K=1 fastest at every N in every run, e.g. 15 ms vs 49-159 ms at N=96000),
/// so the classic tree ships.
const BUCKET_K: u32 = 1;

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

/// A reusable GPU self-gravity dispatch: the exact direct-sum pipeline plus the LBVH Barnes-Hut tree
/// pipelines from the same verified shader. Build once; dispatch per step ([`GravityField`] routes
/// between the two by [`TREE_KNEE`]).
pub struct GpuGravity {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    // The tree path: cs_tree -> cs_com_sweep xR -> cs_com_resolve -> cs_gravity_bh (the verifier's
    // stage order, with the racy single-pass cs_com climb replaced by the race-free ping-pong sweep;
    // the Morton sort between morton and tree happens on the CPU, see `dispatch_tree_to_staging`).
    tree_layout: wgpu::BindGroupLayout,
    tree_build: wgpu::ComputePipeline,
    tree_sweep: wgpu::ComputePipeline,
    tree_resolve: wgpu::ComputePipeline,
    tree_walk: wgpu::ComputePipeline,
}

impl GpuGravity {
    /// Build the pipelines for the `cs_gravity_direct` and tree entry points of `bh_gravity.wgsl`.
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
        // The tree kernels touch bindings 0 (Params), 2 (acc), 4 (codes), 5 (order), 6 (nodes),
        // 8 (sbodies) and 9/10 (the moment ping-pong) - never 1 (bodies) or 3 (bbox), which belong to
        // the GPU bbox/morton stages the CPU sort replaces here, and never 7 (ready), which belongs to
        // the climb form of cs_com this dispatch does not run. Seven storage buffers, inside the WebGPU
        // baseline of eight.
        let tree_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu-gravity-tree-layout"),
            entries: &[
                entry(0, true, true),
                entry(2, false, false),  // acc
                entry(4, false, false),  // codes (per-leaf cluster codes, sorted)
                entry(5, false, false),  // order (sorted particle index -> original index)
                entry(6, false, false),  // nodes (2L-1 Karras arena)
                entry(8, true, false),   // sbodies (bodies permuted into Morton order)
                entry(9, true, false),   // mom_src (previous sweep's moments)
                entry(10, false, false), // mom_dst (this sweep's moments)
            ],
        });
        let tree_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-gravity-tree-pipeline-layout"),
            bind_group_layouts: &[&tree_layout],
            push_constant_ranges: &[],
        });
        let tree_pipe = |entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&tree_pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            pipeline,
            layout,
            tree_layout,
            tree_build: tree_pipe("cs_tree"),
            tree_sweep: tree_pipe("cs_com_sweep"),
            tree_resolve: tree_pipe("cs_com_resolve"),
            tree_walk: tree_pipe("cs_gravity_bh"),
        }
    }

    /// Softened self-gravity acceleration on every body, computed on the GPU. `softening` is the Plummer
    /// length (m). Synchronous: uploads the bodies, dispatches, and reads the result back with a blocking
    /// poll, which is correct for a native step and for measurement. Native only, because the blocking
    /// poll is exactly what a browser forbids; [`GravityField`] wraps the same dispatch in the engine's
    /// two-phase read-back there.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn accelerations(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: &[DVec3],
        mass: &[f64],
        softening: f64,
    ) -> Vec<DVec3> {
        if pos.is_empty() {
            return Vec::new();
        }
        let staging = self.dispatch_to_staging(device, queue, pos, mass, softening);
        block_read(device, &staging)
    }

    /// Softened self-gravity via the LBVH Barnes-Hut tree ([`THETA`], [`BUCKET_K`]), synchronous, native
    /// only (same contract as [`GpuGravity::accelerations`]). This is the path [`GravityField`] takes at
    /// or above [`TREE_KNEE`]; exposed so the correctness tests and the crossover bench can force it at
    /// any N.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn accelerations_tree(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: &[DVec3],
        mass: &[f64],
        softening: f64,
    ) -> Vec<DVec3> {
        self.accelerations_tree_with(device, queue, pos, mass, softening, THETA, BUCKET_K)
    }

    /// [`GpuGravity::accelerations_tree`] with theta and the leaf bucket size exposed: the theta-to-zero
    /// structural test and the bucket sweep in the bench need them; the live path never varies them.
    #[cfg(not(target_arch = "wasm32"))]
    fn accelerations_tree_with(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: &[DVec3],
        mass: &[f64],
        softening: f64,
        theta: f32,
        bucket_k: u32,
    ) -> Vec<DVec3> {
        if pos.is_empty() {
            return Vec::new();
        }
        let staging = self.dispatch_tree_to_staging(device, queue, pos, mass, softening, theta, bucket_k);
        block_read(device, &staging)
    }

    /// Upload the bodies, dispatch `cs_gravity_direct`, and submit a copy of the acceleration field into
    /// a fresh MAP_READ staging buffer. The one dispatch both platforms share: the native blocking path
    /// polls the returned buffer immediately; the browser path maps it asynchronously and collects it a
    /// later pass ([`GravityField`]).
    fn dispatch_to_staging(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: &[DVec3],
        mass: &[f64],
        softening: f64,
    ) -> wgpu::Buffer {
        let n = pos.len();
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
        staging
    }

    /// Upload a Morton-sorted cloud, dispatch the LBVH tree pipeline (`cs_tree` -> `cs_com_sweep` xR ->
    /// `cs_com_resolve` -> `cs_gravity_bh`, the stage order `tools/gpu-bh-verify` verified kernel by
    /// kernel, with the single-pass COM climb replaced by the race-free sweep because the climb is
    /// measurably incoherent on Metal, see the shader), and submit a copy of the acceleration field into
    /// a fresh MAP_READ staging buffer, exactly like the direct path.
    ///
    /// The bbox, the Morton codes and the sort run on the CPU here, deliberately: the GPU radix sort was
    /// never built (docs/36 stage 3; the verifier reads the codes back and sorts them on the CPU too), so
    /// the codes must transit the CPU either way, and computing them there, bit-identically to
    /// `cs_bbox`/`cs_morton` (the verifier proved this arithmetic exactly equal to the kernels), spares a
    /// blocking mid-pipeline read-back that the browser could not legally make. The GPU keeps the parts
    /// that are actually parallel: the Karras tree build, the bottom-up COM climb, and the theta
    /// traversal. A GPU sort replacing the CPU leg is the standing refinement, not a behaviour change.
    fn dispatch_tree_to_staging(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: &[DVec3],
        mass: &[f64],
        softening: f64,
        theta: f32,
        bucket_k: u32,
    ) -> wgpu::Buffer {
        let n = pos.len();
        let bodies: Vec<[f32; 4]> = pos
            .iter()
            .zip(mass)
            .map(|(p, &m)| [p.x as f32, p.y as f32, p.z as f32, m as f32])
            .collect();
        // f32 bounding box: the same reduction cs_bbox performs (its u32 float-radix encoding is
        // lossless, so the GPU result IS the f32 min/max computed here).
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for b in &bodies {
            for k in 0..3 {
                lo[k] = lo[k].min(b[k]);
                hi[k] = hi[k].max(b[k]);
            }
        }
        // 30-bit Morton codes, bit-identical to cs_morton (same expand + same clamp arithmetic, in f32).
        let mut pairs: Vec<(u32, u32)> = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let mut q = [0u32; 3];
                for k in 0..3 {
                    let ext = (hi[k] - lo[k]).max(1.0e-30);
                    let u = ((b[k] - lo[k]) / ext).clamp(0.0, 1.0);
                    q[k] = (u * 1024.0).floor().clamp(0.0, 1023.0) as u32;
                }
                (expand_bits(q[0]) * 4 + expand_bits(q[1]) * 2 + expand_bits(q[2]), i as u32)
            })
            .collect();
        // Sort by (code, index): the index tiebreak is Karras's duplicate handling, so coincident
        // particles still build a valid tree.
        pairs.sort_unstable();
        let k = bucket_k as usize;
        let n_leaves = n.div_ceil(k);
        let order: Vec<u32> = pairs.iter().map(|p| p.1).collect();
        // Leaf cluster code = the code of the first (lowest) particle in each bucket; buckets are
        // contiguous runs of the sorted array, so cluster codes are non-decreasing (a valid tree).
        let cluster_codes: Vec<u32> = (0..n_leaves).map(|c| pairs[c * k].0).collect();
        // Bodies permuted into Morton order: leaf buckets are contiguous, traversal reads coalesce.
        let sbodies: Vec<[f32; 4]> = order.iter().map(|&o| bodies[o as usize]).collect();

        let params = Params {
            n: n as u32,
            theta,
            soft2: (softening * softening) as f32,
            n_leaves: n_leaves as u32,
            bucket_k,
            _pad: [0; 3],
        };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gravity-tree-params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let storage_init = |label: &str, contents: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let sbody_buf = storage_init("gravity-tree-sbodies", bytemuck::cast_slice(&sbodies));
        let code_buf = storage_init("gravity-tree-codes", bytemuck::cast_slice(&cluster_codes));
        let order_buf = storage_init("gravity-tree-order", bytemuck::cast_slice(&order));
        let n_nodes = 2 * n_leaves - 1;
        // No cs_tree_reset pass: every buffer here is created fresh and zero-initialised per call, and
        // the reset exists to clear REUSED arenas (its ready counters belong to the climb form of
        // cs_com, which this dispatch replaces with the sweep). Nothing below reads parent or flags.
        let node_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gravity-tree-nodes"),
            size: (n_nodes * 64) as u64, // 2L-1 nodes, 64 bytes each (the WGSL Node)
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let moments = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (n_nodes * 48) as u64, // 3 vec4 per node (the WGSL Moment)
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        let mom_a = moments("gravity-tree-moments-a");
        let mom_b = moments("gravity-tree-moments-b");
        let acc_size = (n * 16) as u64; // vec4<f32>
        let acc_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gravity-tree-acc"),
            size: acc_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        // Two bind groups differing only in the moment ping-pong direction: a-reads-write-b and the
        // swap. Every other resource is shared.
        let bind_with = |src: &wgpu::Buffer, dst: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gravity-tree-bind"),
                layout: &self.tree_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: acc_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: code_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: order_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: node_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 8, resource: sbody_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 9, resource: src.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 10, resource: dst.as_entire_binding() },
                ],
            })
        };
        let bind_ab = bind_with(&mom_a, &mom_b);
        let bind_ba = bind_with(&mom_b, &mom_a);
        // Sweep count = root height + 1 (a node at height h converges at sweep h+1). Over DISTINCT
        // sorted 30-bit codes each level strictly lengthens the common prefix, so the distinct-code
        // levels are at most 30; a run of R equal codes splits on the Karras index tiebreak, at most
        // ceil(log2(R)) further levels. One more sweep as margin costs microseconds.
        let max_run = {
            let (mut longest, mut run) = (1usize, 1usize);
            for w in cluster_codes.windows(2) {
                run = if w[0] == w[1] { run + 1 } else { 1 };
                longest = longest.max(run);
            }
            longest
        };
        let dup_levels = (usize::BITS - (max_run - 1).leading_zeros()) as usize;
        let sweeps = 32 + dup_levels;
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        // One pass per kernel: pass boundaries are the memory barriers between stages, the ONLY
        // cross-invocation ordering WGSL guarantees (which is exactly why the sweep exists). The Karras
        // build covers the L-1 internal nodes, each sweep the whole 2L-1 arena, the traversal one
        // thread per body.
        let l = n_leaves as u32;
        let mut run_pass = |pipe: &wgpu::ComputePipeline, bind: &wgpu::BindGroup, threads: u32| {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gravity-tree-dispatch"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(threads.div_ceil(64).max(1), 1, 1);
        };
        run_pass(&self.tree_build, &bind_ab, l.saturating_sub(1));
        for s in 0..sweeps {
            // Even sweeps write B (src A), odd write A: strict alternation, never read-your-own-pass.
            let bind = if s % 2 == 0 { &bind_ab } else { &bind_ba };
            run_pass(&self.tree_sweep, bind, n_nodes as u32);
        }
        // Resolve reads mom_src, so bind the buffer the LAST sweep wrote as the source: with an even
        // sweep count the final (odd-indexed) sweep wrote A, whose source-side bind group is bind_ab.
        run_pass(&self.tree_resolve, if sweeps % 2 == 0 { &bind_ab } else { &bind_ba }, n_nodes as u32);
        run_pass(&self.tree_walk, &bind_ab, n as u32);
        drop(run_pass);
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gravity-staging"),
            size: acc_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(&acc_buf, 0, &staging, 0, acc_size);
        queue.submit(Some(enc.finish()));
        staging
    }
}

/// Spread a 10-bit integer to 30 bits, two zeros between each bit: the exact `expand_bits` in
/// `bh_gravity.wgsl` (integer arithmetic, so CPU and GPU agree bit for bit; `tools/gpu-bh-verify`
/// pinned that equality).
fn expand_bits(v0: u32) -> u32 {
    let mut v = v0 & 0x0000_03ff;
    v = v.wrapping_mul(0x0001_0001) & 0xff00_00ff;
    v = v.wrapping_mul(0x0000_0101) & 0x0f00_f00f;
    v = v.wrapping_mul(0x0000_0011) & 0xc30c_30c3;
    v = v.wrapping_mul(0x0000_0005) & 0x4924_9249;
    v
}

/// Block until `staging` is mapped and decode the acceleration field. Native only: `Maintain::Wait` is
/// the blocking poll the browser forbids.
#[cfg(not(target_arch = "wasm32"))]
fn block_read(device: &wgpu::Device, staging: &wgpu::Buffer) -> Vec<DVec3> {
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let raw: Vec<[f32; 4]> = bytemuck::cast_slice(&slice.get_mapped_range()).to_vec();
    raw.iter().map(|a| DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64)).collect()
}

/// **The live dispatch**: [`GpuGravity`] bound to the device and queue it runs on, attachable to a
/// particle system ([`crate::aggregate::Aggregate::gpu_gravity`]). The device is the SHARED one: in the
/// browser it is the same device the renderer and `gpu_sph` already run on (a scene hands it over when
/// the debris materialises), natively it comes from [`crate::gpu_host::GpuHost`]. wgpu devices and
/// queues are reference-counted handles, so cloning binds, it does not duplicate the GPU.
///
/// Two execution modes, one per platform, because WebGPU forbids blocking (`Maintain::Wait` is a no-op
/// in the browser, the `gpu_sph` lesson):
///  * **native**, synchronous: submit this pass's positions and read the field back within the pass.
///    Exact at the pass's own positions; this is what the correctness test and the bench measure.
///  * **wasm**, two-phase (the `gpu_store` read-back pattern): a pass harvests the field submitted by
///    the PREVIOUS pass if the map has completed, and submits the current positions for a later one.
///    The harvested field is one submission old, the same class of deferral the block-timestep
///    scheduler already accepts when a coasting particle keeps the acceleration from its last kick.
///    While nothing has landed yet, or the particle count changed mid-flight (an absorb/drain), the
///    caller falls back to the CPU tree for that pass, so no pass ever blocks and none goes without
///    gravity.
pub struct GravityField {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: GpuGravity,
    /// In-flight staging buffer and its completion flag. `Arc<AtomicBool>`, not `Rc<Cell<bool>>`: wgpu
    /// bounds the `map_async` callback by `WasmNotSend` (plain `Send` off-wasm), so the `Rc` form
    /// compiles only for the browser (the defect `gpu_store` had to fix twice).
    staging: Option<wgpu::Buffer>,
    ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl GravityField {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        GravityField {
            pipeline: GpuGravity::new(device),
            device: device.clone(),
            queue: queue.clone(),
            staging: None,
            ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// The self-gravity field for these positions, or `None` when the caller must take the CPU path for
    /// this pass (browser only: first pass, map still in flight, or a mid-flight count change).
    pub fn accelerations(&mut self, pos: &[DVec3], mass: &[f64], softening: f64) -> Option<Vec<DVec3>> {
        if pos.is_empty() {
            return Some(Vec::new());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Native blocks legally: submit, wait, read. The field is exact, never deferred.
            self.staging = None; // drop any stale in-flight buffer (defensive; native drains every pass)
            self.submit(pos, mass, softening);
            self.device.poll(wgpu::Maintain::Wait);
            self.take()
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Harvest the previous pass's field if its map completed; reject it if the cloud changed
            // size mid-flight (stale shape would misassign forces). Then keep exactly one job in flight.
            let harvested = self.take().filter(|g| g.len() == pos.len());
            if self.staging.is_none() {
                self.submit(pos, mass, softening);
            }
            harvested
        }
    }

    /// Phase 1: dispatch the field for these positions and start the async map of the result. The
    /// second measured knee routes WITHIN the GPU: below [`TREE_KNEE`] the exact direct sum (O(N^2) but
    /// the ideal GPU workload), at or above it the LBVH Barnes-Hut tree (O(N log N), theta = the CPU
    /// tree's 0.5), because past the knee the quadratic sum loses to the tree on wall time (measured in
    /// `gpu_tree_speedup`).
    fn submit(&mut self, pos: &[DVec3], mass: &[f64], softening: f64) {
        let staging = if pos.len() >= TREE_KNEE {
            self.pipeline.dispatch_tree_to_staging(
                &self.device,
                &self.queue,
                pos,
                mass,
                softening,
                THETA,
                BUCKET_K,
            )
        } else {
            self.pipeline.dispatch_to_staging(&self.device, &self.queue, pos, mass, softening)
        };
        self.ready.store(false, std::sync::atomic::Ordering::Release);
        let flag = self.ready.clone();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if res.is_ok() {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        });
        self.staging = Some(staging);
    }

    /// Phase 2: the mapped field if the map has completed, else `None` while pending or idle.
    fn take(&mut self) -> Option<Vec<DVec3>> {
        if !self.ready.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        let staging = self.staging.take()?;
        let out = {
            let data = staging.slice(..).get_mapped_range();
            bytemuck::cast_slice::<u8, [f32; 4]>(&data)
                .iter()
                .map(|a| DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64))
                .collect()
        };
        staging.unmap();
        self.ready.store(false, std::sync::atomic::Ordering::Release);
        Some(out)
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

    /// A debris-like self-gravitating cloud (the moon-drop configuration, gravity isolated) at count `n`.
    fn debris_cloud(n: usize, seed: u64) -> Vec<crate::orbit::Body> {
        let mut s = seed;
        let mut rng = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 40) as f64 / (1u64 << 24) as f64 - 0.5
        };
        (0..n)
            .map(|_| crate::orbit::Body {
                pos: DVec3::new(rng(), rng(), rng()).normalize_or_zero()
                    * (1.7e6 * (0.2 + rng().abs())),
                vel: DVec3::ZERO,
                mass: 5.0e19,
            })
            .collect()
    }

    /// **The live dispatch, exercised through the aggregate it is wired into.** Above the knee, an
    /// aggregate carrying a [`GravityField`] must produce the same self-gravity the CPU tree would,
    /// within the tree's own θ bound (RMS relative < 1e-2, the bound `tools/gpu-bh-verify` enforces
    /// between the tree and the direct sum). The GPU sum is EXACT, so the difference measured here IS
    /// the tree's multipole error; past the bound, the dispatch is computing different physics. Skips
    /// cleanly with no GPU.
    #[test]
    fn the_aggregate_dispatch_matches_the_cpu_tree_within_the_theta_bound() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU device - skipping aggregate dispatch test");
            return;
        };
        let n = 1500; // above DIRECT_SUM_KNEE, the moon-drop cloud's scale
        assert!(n >= DIRECT_SUM_KNEE);
        let soft = 2.5e4;
        let bodies = debris_cloud(n, 0xA66E_D15Bu64);
        let mut cpu_agg = crate::aggregate::Aggregate::new(bodies.clone(), soft);
        let mut gpu_agg = crate::aggregate::Aggregate::new(bodies, soft);
        gpu_agg.gpu_gravity = Some(GravityField::new(&host.device, &host.queue));

        let a_cpu = cpu_agg.accelerations();
        let a_gpu = gpu_agg.accelerations();
        assert!(
            a_cpu.iter().zip(&a_gpu).any(|(a, b)| a != b),
            "identical fields mean the GPU path was never taken; the knee gate is not dispatching"
        );
        let (mut err_sq, mut ref_sq) = (0.0f64, 0.0f64);
        for (g, c) in a_gpu.iter().zip(&a_cpu) {
            err_sq += (*g - *c).length_squared();
            ref_sq += c.length_squared();
        }
        let rms = (err_sq / ref_sq.max(1e-300)).sqrt();
        assert!(
            rms < 1e-2,
            "the dispatched GPU field must agree with the CPU tree within its θ bound; RMS rel {rms:.2e}"
        );
    }

    /// **Below the knee the CPU path stands**, field attached or not: the measured crossover says the
    /// CPU is faster there, so the gate must not dispatch. Pinned by bitwise identity (the same CPU
    /// code runs in both aggregates). Skips cleanly with no GPU.
    #[test]
    fn below_the_knee_the_cpu_path_stands_even_with_a_field_attached() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU device - skipping knee gate test");
            return;
        };
        let n = 300; // below DIRECT_SUM_KNEE
        assert!(n < DIRECT_SUM_KNEE);
        let bodies = debris_cloud(n, 0x0BE1_0B0Bu64);
        let mut cpu_agg = crate::aggregate::Aggregate::new(bodies.clone(), 2.5e4);
        let mut gpu_agg = crate::aggregate::Aggregate::new(bodies, 2.5e4);
        gpu_agg.gpu_gravity = Some(GravityField::new(&host.device, &host.queue));
        assert_eq!(
            cpu_agg.accelerations(),
            gpu_agg.accelerations(),
            "below the knee both aggregates must take the identical CPU path"
        );
    }

    /// RMS relative error between two acceleration fields (norm of the error over norm of the
    /// reference), the metric every tree bound in this file and in `tools/gpu-bh-verify` uses.
    fn rms_rel(test: &[DVec3], reference: &[DVec3]) -> f64 {
        let (mut err_sq, mut ref_sq) = (0.0f64, 0.0f64);
        for (t, r) in test.iter().zip(reference) {
            err_sq += (*t - *r).length_squared();
            ref_sq += r.length_squared();
        }
        (err_sq / ref_sq.max(1e-300)).sqrt()
    }

    /// **The GPU tree must agree with the CPU tree within the theta multipole bound.** Both walk the
    /// same physics with the same opening angle (0.5) against the same cloud, so each sits within RMS
    /// 1e-2 of the exact sum (`tools/gpu-bh-verify` bound; `barnes_hut_matches_brute_force_within_theta_bound`
    /// for the CPU side) and they must sit within that bound of EACH OTHER. N is above the CPU tree's
    /// brute-force cutoff so a real octree is the reference, and mid-scale so the exact f64 reference
    /// stays cheap; the theta error on the debris geometry SHRINKS toward large N (measured: RMS 5.8e-3
    /// here, 3.4e-3 at 24k, 1.1e-2 down at N=1500, a size the tree is never dispatched at), and the
    /// tree-knee test below pins the bound at the N where the live path actually switches. Skips
    /// cleanly with no GPU.
    #[test]
    fn the_gpu_tree_matches_the_cpu_tree_within_the_theta_bound() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU device - skipping gpu tree correctness test");
            return;
        };
        let n = 6000; // > bhtree's BRUTE_BELOW, so the CPU reference is the actual tree
        let soft = 2.5e4;
        let bodies = debris_cloud(n, 0x7EE5_2707u64);
        let pos: Vec<DVec3> = bodies.iter().map(|b| b.pos).collect();
        let mass: Vec<f64> = bodies.iter().map(|b| b.mass).collect();

        let g = GpuGravity::new(&host.device);
        let gpu = g.accelerations_tree(&host.device, &host.queue, &pos, &mass, soft);
        let cpu = crate::bhtree::BarnesHut::build(&pos, &mass, 0.5, soft).accelerations(&pos, &mass);
        assert!(gpu.iter().all(|a| a.x.is_finite() && a.y.is_finite() && a.z.is_finite()));
        let rms = rms_rel(&gpu, &cpu);
        assert!(
            rms < 1e-2,
            "GPU LBVH tree must match the CPU Barnes-Hut within the theta bound; RMS rel {rms:.2e}"
        );
        // And against the exact f64 direct sum, the honest ground truth for the same bound.
        let exact = cpu_direct(&pos, &mass, soft);
        let rms_exact = rms_rel(&gpu, &exact);
        assert!(
            rms_exact < 1e-2,
            "GPU LBVH tree must sit within the theta bound of the exact sum; RMS rel {rms_exact:.2e}"
        );
        // Bitwise repeatability: the single-pass COM climb was measurably NONDETERMINISTIC on Metal
        // (relaxed atomics do not order the sibling moment reads), which is why the dispatch builds
        // moments with the ping-pong sweep. A rerun differing bit for bit means that race is back.
        let again = g.accelerations_tree(&host.device, &host.queue, &pos, &mass, soft);
        assert_eq!(gpu, again, "the tree dispatch must be deterministic; the COM race is back");
    }

    /// **Theta to zero recovers the direct sum**, the strong structural check from `tools/gpu-bh-verify`
    /// stage 6a: a fully-opened traversal visits every particle exactly once, so any tree defect
    /// (unreachable leaf, double count, wrong COM plumbing) surfaces as a hard failure, not a slightly
    /// worse approximation. Bound 1e-4 (f32 rounding), the verifier's. Skips cleanly with no GPU.
    #[test]
    fn the_gpu_tree_opened_fully_recovers_the_direct_sum() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU device - skipping gpu tree structural test");
            return;
        };
        let n = 1500;
        let soft = 2.5e4;
        let bodies = debris_cloud(n, 0x09E4_74EEu64);
        let pos: Vec<DVec3> = bodies.iter().map(|b| b.pos).collect();
        let mass: Vec<f64> = bodies.iter().map(|b| b.mass).collect();

        let g = GpuGravity::new(&host.device);
        let opened =
            g.accelerations_tree_with(&host.device, &host.queue, &pos, &mass, soft, 1.0e-4, BUCKET_K);
        let exact = cpu_direct(&pos, &mass, soft);
        let rms = rms_rel(&opened, &exact);
        assert!(
            rms < 1e-4,
            "theta -> 0 must recover the exact direct sum to f32 precision; RMS rel {rms:.2e}"
        );
    }

    /// **At the tree knee the field dispatches the TREE, and the tree is right.** The routing inside
    /// [`GravityField::submit`] is pinned by physics, not by inspection: at N = [`TREE_KNEE`] the field's
    /// output must carry the tree's (bounded, nonzero) multipole signature against the exact GPU direct
    /// sum, and must agree with the CPU Barnes-Hut within the same theta bound. Skips cleanly with no GPU.
    #[test]
    fn at_the_tree_knee_the_field_dispatches_the_tree() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU device - skipping tree knee routing test");
            return;
        };
        let n = TREE_KNEE;
        let soft = 2.5e4;
        let bodies = debris_cloud(n, 0x74EE_C4EEu64);
        let pos: Vec<DVec3> = bodies.iter().map(|b| b.pos).collect();
        let mass: Vec<f64> = bodies.iter().map(|b| b.mass).collect();

        let mut field = GravityField::new(&host.device, &host.queue);
        let dispatched =
            field.accelerations(&pos, &mass, soft).expect("native dispatch is synchronous");
        let direct = field.pipeline.accelerations(&host.device, &host.queue, &pos, &mass, soft);
        assert!(
            dispatched.iter().zip(&direct).any(|(a, b)| a != b),
            "identical fields mean the direct sum ran; the tree knee is not routing"
        );
        let cpu = crate::bhtree::BarnesHut::build(&pos, &mass, 0.5, soft).accelerations(&pos, &mass);
        let rms = rms_rel(&dispatched, &cpu);
        assert!(
            rms < 1e-2,
            "the dispatched tree field must match the CPU Barnes-Hut within its theta bound; RMS rel {rms:.2e}"
        );
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    /// **GPU vs CPU Barnes-Hut at production N**, so the dispatch win is measured, not asserted. Reports
    /// the GPU per-call time INCLUDING the upload+dispatch+readback round trip (the honest cost of the
    /// synchronous backend) against the CPU tree at the SAME per-pass cost the live path pays
    /// (`Aggregate::accelerations_masked` rebuilds the tree every pass, so build + eval, not eval alone).
    /// The N sweep brackets the crossover so [`DIRECT_SUM_KNEE`] is read off this table, not guessed.
    ///
    /// Run: `cargo test --lib gpu_gravity_speedup -- --ignored --nocapture`
    /// (multi-GPU boxes: set `INTEGRITY_ADAPTER`; the harness refuses to guess).
    #[test]
    #[ignore = "perf bench — needs a GPU; run with --ignored --nocapture"]
    fn gpu_gravity_speedup() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU — skipping");
            return;
        };
        println!("  adapter: {} ({:?})", host.info.name, host.info.backend);
        let mut s = 0xBEEF_1234u64;
        let mut rng = || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s >> 40) as f64 / (1u64 << 24) as f64 - 0.5 };
        let g = GpuGravity::new(&host.device);
        for &n in &[200usize, 400, 750, 1000, 1500, 2000, 3000, 6000] {
            let pos: Vec<DVec3> = (0..n).map(|_| DVec3::new(rng(), rng(), rng()).normalize_or_zero() * 1.7e6).collect();
            let mass: Vec<f64> = (0..n).map(|_| 5.0e19).collect();
            // Warm up (shader/pipeline, first alloc).
            let _ = g.accelerations(&host.device, &host.queue, &pos, &mass, 2.5e4);
            let t = Instant::now();
            for _ in 0..10 { let _ = g.accelerations(&host.device, &host.queue, &pos, &mass, 2.5e4); }
            let gpu_ms = t.elapsed().as_secs_f64() * 1e3 / 10.0;
            // CPU Barnes-Hut at the live per-pass cost: build the tree AND evaluate it, like the
            // aggregate's acceleration pass does (positions move every pass, so the tree cannot be reused).
            let t = Instant::now();
            for _ in 0..3 {
                let bh = crate::bhtree::BarnesHut::build(&pos, &mass, 0.5, 2.5e4);
                let _ = bh.accelerations(&pos, &mass);
            }
            let cpu_ms = t.elapsed().as_secs_f64() * 1e3 / 3.0;
            println!("  N={n:<5} GPU {gpu_ms:6.2} ms (incl round trip)   CPU-BH build+eval {cpu_ms:6.2} ms   -> {:.1}x", cpu_ms / gpu_ms.max(1e-6));
        }
    }

    /// **GPU direct sum vs GPU LBVH tree at growing N**, so [`TREE_KNEE`] is read off a table, not
    /// guessed. Both columns are honest per-call costs: the direct sum pays upload+dispatch+readback,
    /// the tree ADDITIONALLY pays its per-call CPU Morton sort and the larger upload (its build cost;
    /// positions move every pass, so the tree is rebuilt every call, exactly like the live path). The
    /// leaf-bucket sweep (K=1/8/32) justifies the shipped [`BUCKET_K`] the same way; the CPU Barnes-Hut
    /// column places both against the processor they replace.
    ///
    /// Run: `cargo test --lib gpu_tree_speedup -- --ignored --nocapture`
    /// (multi-GPU boxes: set `INTEGRITY_ADAPTER`; the harness refuses to guess).
    #[test]
    #[ignore = "perf bench - needs a GPU; run with --ignored --nocapture"]
    fn gpu_tree_speedup() {
        let Ok(host) = crate::gpu_host::GpuHost::headless(None) else {
            eprintln!("no GPU - skipping");
            return;
        };
        println!("  adapter: {} ({:?})", host.info.name, host.info.backend);
        let mut s = 0x7EE5_EED5u64;
        let mut rng = || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s >> 40) as f64 / (1u64 << 24) as f64 - 0.5 };
        let g = GpuGravity::new(&host.device);
        let soft = 2.5e4;
        println!("  {:>7}  {:>10}  {:>10}  {:>10}  {:>10}  {:>11}  {:>8}", "N", "direct ms", "tree K=1", "tree K=8", "tree K=32", "CPU-BH ms", "dir/tree");
        for &n in &[3000usize, 6000, 12000, 24000, 48000, 96000, 192000] {
            // The debris-cloud volume distribution (not a shell): the tree's depth and the traversal's
            // divergence depend on the spatial distribution, so measure the one the live path sees.
            let pos: Vec<DVec3> = (0..n)
                .map(|_| DVec3::new(rng(), rng(), rng()).normalize_or_zero() * (1.7e6 * (0.2 + rng().abs())))
                .collect();
            let mass: Vec<f64> = vec![5.0e19; n];
            let iters = if n >= 96_000 { 3 } else { 10 };
            let mut time_ms = |f: &mut dyn FnMut()| {
                f(); // warm up (pipeline, first alloc)
                let t = Instant::now();
                for _ in 0..iters { f(); }
                t.elapsed().as_secs_f64() * 1e3 / iters as f64
            };
            let t_dir = time_ms(&mut || { let _ = g.accelerations(&host.device, &host.queue, &pos, &mass, soft); });
            let t_k1 = time_ms(&mut || { let _ = g.accelerations_tree_with(&host.device, &host.queue, &pos, &mass, soft, THETA, 1); });
            let t_k8 = time_ms(&mut || { let _ = g.accelerations_tree_with(&host.device, &host.queue, &pos, &mass, soft, THETA, 8); });
            let t_k32 = time_ms(&mut || { let _ = g.accelerations_tree_with(&host.device, &host.queue, &pos, &mass, soft, THETA, 32); });
            // CPU Barnes-Hut at the live per-pass cost (build + eval), single iteration at large N.
            let cpu_iters = if n >= 48_000 { 1 } else { 3 };
            let t = Instant::now();
            for _ in 0..cpu_iters {
                let bh = crate::bhtree::BarnesHut::build(&pos, &mass, 0.5, soft);
                let _ = bh.accelerations(&pos, &mass);
            }
            let t_cpu = t.elapsed().as_secs_f64() * 1e3 / cpu_iters as f64;
            let best_tree = t_k1.min(t_k8).min(t_k32);
            println!("  {n:>7}  {t_dir:>10.2}  {t_k1:>10.2}  {t_k8:>10.2}  {t_k32:>10.2}  {t_cpu:>11.2}  {:>7.2}x", t_dir / best_tree.max(1e-9));
        }
    }
}
