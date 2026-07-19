//! Headless GPU verification of `shaders/bh_gravity.wgsl` (docs/36) on the box's RTX 2070 via native Vulkan
//! wgpu — the engine's own wgpu is webgpu-only, so this lives in a standalone crate (same reason as
//! sph-verify / impact-run). It builds a self-gravity Barnes-Hut (LBVH) tree entirely on the GPU and checks
//! it against (a) the GPU direct O(N²) sum — the difference is then PURELY the θ multipole error, both f32 —
//! and (b) the CPU f64 direct sum. Exit code 0 = every stage matches within its bound.
//!
//! Built kernel-by-kernel (docs/36): direct-sum baseline → bbox → morton → sort → Karras tree → COM →
//! θ-traversal. Each stage prints PASS/FAIL against a CPU reference before the next is trusted.

const SHADER: &str = include_str!("../../../shaders/bh_gravity.wgsl");

const G: f64 = 6.674e-11;

// ---- GPU context (created once; kernels added as stages land) ----
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    module: wgpu::ShaderModule,
}
impl Gpu {
    fn new() -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("no Vulkan adapter (RTX 2070 expected)");
        println!("adapter: {}", adapter.get_info().name);
        // Request the adapter's full native limits (many storage buffers per stage — the tree pipeline binds
        // more than the WebGPU baseline of 8; that binding-budget question is a wiring-time concern, docs/36).
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("gpu-bh-verify"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("request_device");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bh_gravity"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        Gpu { device, queue, module }
    }

    fn storage(&self, contents: &[u8], copy_src: bool) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        let usage = wgpu::BufferUsages::STORAGE
            | if copy_src { wgpu::BufferUsages::COPY_SRC } else { wgpu::BufferUsages::empty() };
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents, usage })
    }
    fn storage_zeroed(&self, size: u64, copy_src: bool) -> wgpu::Buffer {
        let usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | if copy_src { wgpu::BufferUsages::COPY_SRC } else { wgpu::BufferUsages::empty() };
        self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: false })
    }
    fn uniform(&self, contents: &[u8]) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }
    fn write(&self, buf: &wgpu::Buffer, data: &[u8]) {
        self.queue.write_buffer(buf, 0, data);
        // write_buffer is queued; a trivial submit flushes it before the next compute submission.
        self.queue.submit(std::iter::empty());
    }
    fn read(&self, buf: &wgpu::Buffer, size: u64) -> Vec<u8> {
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, size);
        self.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let v = slice.get_mapped_range().to_vec();
        v
    }
}

// std430 layouts (byte-match the WGSL) ------------------------------------------------------------
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    theta: f32,
    soft2: f32,
    n_leaves: u32,
    bucket_k: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
}
impl Params {
    fn new(n: usize, theta: f32, soft: f64, bucket_k: u32) -> Self {
        let n_leaves = (n as u32).div_ceil(bucket_k);
        Params { n: n as u32, theta, soft2: (soft * soft) as f32, n_leaves, bucket_k, _p0: 0, _p1: 0, _p2: 0 }
    }
}

// ---- test configuration: a random cloud (matches the bhtree.rs unit-test config so numbers cross-check) --
fn splitmix(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}
/// Random cloud in a 1e6 m box, masses ~1e18 kg — the same generator as bhtree.rs's verified test.
fn build_cloud(n: usize, seed: u64) -> (Vec<[f32; 4]>, Vec<[f64; 3]>, Vec<f64>) {
    let mut s = seed;
    let mut bodies = Vec::with_capacity(n);
    let mut pos = Vec::with_capacity(n);
    let mut mass = Vec::with_capacity(n);
    for _ in 0..n {
        let p = [splitmix(&mut s) * 1.0e6, splitmix(&mut s) * 1.0e6, splitmix(&mut s) * 1.0e6];
        let m = 1.0e18 * (0.5 + splitmix(&mut s));
        bodies.push([p[0] as f32, p[1] as f32, p[2] as f32, m as f32]);
        pos.push(p);
        mass.push(m);
    }
    (bodies, pos, mass)
}

/// CPU f64 direct softened self-gravity — the ground truth (same formula as bhtree.rs `accelerations`).
fn cpu_direct(pos: &[[f64; 3]], mass: &[f64], soft: f64) -> Vec<[f64; 3]> {
    let n = pos.len();
    let s2 = soft * soft;
    let mut acc = vec![[0.0f64; 3]; n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let d = [pos[j][0] - pos[i][0], pos[j][1] - pos[i][1], pos[j][2] - pos[i][2]];
            let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] + s2;
            let g = G * mass[j] / (r2 * r2.sqrt());
            for k in 0..3 {
                acc[i][k] += d[k] * g;
            }
        }
    }
    acc
}

fn to_f64(acc: &[[f32; 4]]) -> Vec<[f64; 3]> {
    acc.iter().map(|a| [a[0] as f64, a[1] as f64, a[2] as f64]).collect()
}

/// RMS-relative error between a GPU acceleration field and a CPU reference (‖Σ err‖ / ‖Σ ref‖), plus the
/// worst single-particle relative error.
fn compare(gpu: &[[f32; 4]], cpu: &[[f64; 3]]) -> (f64, f64) {
    let n = cpu.len();
    let (mut sum_sq, mut ref_sq, mut max_rel) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let (mut e, mut a2) = (0.0, 0.0);
        for k in 0..3 {
            let de = gpu[i][k] as f64 - cpu[i][k];
            e += de * de;
            a2 += cpu[i][k] * cpu[i][k];
        }
        sum_sq += e;
        ref_sq += a2;
        max_rel = max_rel.max(e.sqrt() / a2.sqrt().max(1e-30));
    }
    ((sum_sq / ref_sq.max(1e-300)).sqrt(), max_rel)
}

/// The Barnes-Hut GPU pipeline context: owns every buffer + one bind group spanning all bindings, and
/// dispatches kernels by entry-point name. Grows binding-by-binding as stages land (docs/36). `n_nodes`
/// = 2N−1 (N leaves at [N−1, 2N−1), N−1 internal at [0, N−1), root = node 0).
struct Bh<'a> {
    gpu: &'a Gpu,
    n: u32,
    params: Params,
    layout: wgpu::BindGroupLayout,
    bind: wgpu::BindGroup,
    ubuf: wgpu::Buffer,
    bbuf: wgpu::Buffer,
    abuf: wgpu::Buffer,
    bboxbuf: wgpu::Buffer,
    codebuf: wgpu::Buffer,
    orderbuf: wgpu::Buffer,
    nodebuf: wgpu::Buffer,
    readybuf: wgpu::Buffer,
    sbodybuf: wgpu::Buffer,
    bodies_cpu: Vec<[f32; 4]>,
}

// std430 tree node (byte-matches the WGSL Node: 4×u32 + 3×vec4<f32> = 64 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
struct Node {
    left: u32,
    right: u32,
    parent: u32,
    flags: u32,
    com: [f32; 4],
    bmin: [f32; 4],
    bmax: [f32; 4],
}
const NO_PARENT: u32 = 0xffff_ffff;
impl<'a> Bh<'a> {
    fn new(gpu: &'a Gpu, bodies: &[[f32; 4]], params: Params) -> Self {
        let n = bodies.len() as u32;
        let ubuf = gpu.uniform(bytemuck::bytes_of(&params));
        let bbuf = gpu.storage(bytemuck::cast_slice(bodies), false);
        let abuf = gpu.storage_zeroed((n as u64) * 16, true);
        let bboxbuf = gpu.storage_zeroed(6 * 4, true);
        let codebuf = gpu.storage_zeroed((n as u64) * 4, true);
        let orderbuf = gpu.storage_zeroed((n as u64) * 4, true);
        let l = params.n_leaves as u64; // tree leaves (= n when bucket_k = 1)
        let nodebuf = gpu.storage_zeroed((2 * l - 1) * 64, true);
        let readybuf = gpu.storage_zeroed((l - 1).max(1) * 4, false);
        let sbodybuf = gpu.storage_zeroed((n as u64) * 16, false);

        let sb = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: ro },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bh"),
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
                sb(1, true),  // bodies
                sb(2, false), // acc
                sb(3, false), // bbox (atomics)
                sb(4, false), // codes
                sb(5, false), // order
                sb(6, false), // nodes
                sb(7, false), // ready (atomics)
                sb(8, true),  // sbodies (sorted-order bodies)
            ],
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: bbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: abuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: bboxbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: codebuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: orderbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: nodebuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: readybuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: sbodybuf.as_entire_binding() },
            ],
        });
        let bodies_cpu = bodies.to_vec();
        Bh { gpu, n, params, layout, bind, ubuf, bbuf, abuf, bboxbuf, codebuf, orderbuf, nodebuf, readybuf, sbodybuf, bodies_cpu }
    }

    fn pipe(&self, entry: &str) -> wgpu::ComputePipeline {
        let pl = self.gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&self.layout],
            push_constant_ranges: &[],
        });
        self.gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&pl),
            module: &self.gpu.module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    }

    /// Run a sequence of (entry_point, thread_count) kernels — one compute pass each (pass boundaries are
    /// memory barriers), all in a single submission.
    fn run(&self, kernels: &[(&str, u32)]) {
        let pipes: Vec<_> = kernels.iter().map(|(e, _)| self.pipe(e)).collect();
        let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for (pipe, (_, threads)) in pipes.iter().zip(kernels) {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.dispatch_workgroups(threads.div_ceil(64).max(1), 1, 1);
        }
        self.gpu.queue.submit(Some(enc.finish()));
    }

    fn set_theta(&mut self, theta: f32) {
        self.params.theta = theta;
        self.gpu.write(&self.ubuf, bytemuck::bytes_of(&self.params));
    }

    /// Full tree build: bbox → morton → (CPU sort, interim) → tree → COM. Leaves a valid tree in nodebuf.
    fn build_tree(&self) {
        let l = self.params.n_leaves;
        self.run(&[("cs_bbox_reset", 6), ("cs_bbox", self.n), ("cs_morton", self.n)]);
        self.cpu_sort_upload();
        self.run(&[("cs_tree_reset", 2 * l - 1), ("cs_tree", l - 1), ("cs_com", l)]);
    }

    /// Time one kernel over `iters` back-to-back dispatches (pipeline built once; a warmup submit primes the
    /// driver). Returns ms/eval. Coarse GPU wall time — enough to show O(N log N) vs O(N²) scaling.
    fn time_kernel(&self, entry: &str, iters: u32) -> f64 {
        let pipe = self.pipe(entry);
        let record = |iters: u32| {
            let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            for _ in 0..iters {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
                pass.set_pipeline(&pipe);
                pass.set_bind_group(0, &self.bind, &[]);
                pass.dispatch_workgroups(self.n.div_ceil(64), 1, 1);
                drop(pass);
            }
            self.gpu.queue.submit(Some(enc.finish()));
            self.gpu.device.poll(wgpu::Maintain::Wait);
        };
        record(2); // warmup
        let t0 = std::time::Instant::now();
        record(iters);
        t0.elapsed().as_secs_f64() * 1000.0 / iters as f64
    }

    fn read_acc(&self) -> Vec<[f32; 4]> {
        bytemuck::cast_slice::<u8, [f32; 4]>(&self.gpu.read(&self.abuf, (self.n as u64) * 16)).to_vec()
    }
    /// Read the 6 bbox atomics and decode to [min_xyz, max_xyz] floats.
    fn read_bbox(&self) -> ([f32; 3], [f32; 3]) {
        let k = bytemuck::cast_slice::<u8, u32>(&self.gpu.read(&self.bboxbuf, 6 * 4)).to_vec();
        let dec = |x: u32| -> f32 {
            if (x >> 31) == 1 {
                f32::from_bits(x ^ 0x8000_0000)
            } else {
                f32::from_bits(!x)
            }
        };
        ([dec(k[0]), dec(k[1]), dec(k[2])], [dec(k[3]), dec(k[4]), dec(k[5])])
    }
    fn read_u32(&self, buf: &wgpu::Buffer) -> Vec<u32> {
        bytemuck::cast_slice::<u8, u32>(&self.gpu.read(buf, (self.n as u64) * 4)).to_vec()
    }
    fn read_nodes(&self) -> Vec<Node> {
        let l = self.params.n_leaves as u64;
        bytemuck::cast_slice::<u8, Node>(&self.gpu.read(&self.nodebuf, (2 * l - 1) * 64)).to_vec()
    }
    /// CPU-sort the (code, particle-index) pairs by (code, index), then write the full sorted ORDER (n) and
    /// the per-leaf CLUSTER codes (n_leaves; the tree indexes leaves, not particles) back to the GPU. Interim
    /// sort that unblocks the tree/COM/traversal stages before the GPU radix sort (docs/36 stage 3).
    fn cpu_sort_upload(&self) {
        let codes = self.read_u32(&self.codebuf);
        let order = self.read_u32(&self.orderbuf);
        let mut pairs: Vec<(u32, u32)> = codes.iter().zip(&order).map(|(&c, &o)| (c, o)).collect();
        pairs.sort_by_key(|&(c, o)| (c, o)); // ties broken by particle index (Karras duplicate handling)
        let sorted_codes: Vec<u32> = pairs.iter().map(|p| p.0).collect();
        let sorted_order: Vec<u32> = pairs.iter().map(|p| p.1).collect();
        let k = self.params.bucket_k as usize;
        // Cluster representative = the Morton code of the first (lowest-code) particle in each bucket; the
        // buckets are contiguous runs of the sorted array, so cluster codes are non-decreasing (valid tree).
        let cluster_codes: Vec<u32> =
            (0..self.params.n_leaves as usize).map(|c| sorted_codes[c * k]).collect();
        // Permute the bodies into sorted order so leaf buckets are contiguous (coalesced reads). Interim on
        // CPU; the GPU pipeline would gather with a kernel (sbodies[t] = bodies[order[t]]).
        let sbodies: Vec<[f32; 4]> = sorted_order.iter().map(|&o| self.bodies_cpu[o as usize]).collect();
        self.gpu.write(&self.codebuf, bytemuck::cast_slice(&cluster_codes));
        self.gpu.write(&self.orderbuf, bytemuck::cast_slice(&sorted_order));
        self.gpu.write(&self.sbodybuf, bytemuck::cast_slice(&sbodies));
    }
}

// CPU Morton reference (bit-identical to the WGSL: same expand + same bbox, which matches exactly).
fn expand_bits(v0: u32) -> u32 {
    let mut v = v0 & 0x0000_03ff;
    v = v.wrapping_mul(0x0001_0001) & 0xff00_00ff;
    v = v.wrapping_mul(0x0000_0101) & 0x0f00_f00f;
    v = v.wrapping_mul(0x0000_0011) & 0xc30c_30c3;
    v = v.wrapping_mul(0x0000_0005) & 0x4924_9249;
    v
}
fn cpu_morton(bodies: &[[f32; 4]], lo: [f32; 3], hi: [f32; 3]) -> Vec<u32> {
    bodies
        .iter()
        .map(|b| {
            let mut q = [0u32; 3];
            for k in 0..3 {
                let ext = (hi[k] - lo[k]).max(1.0e-30);
                let u = ((b[k] - lo[k]) / ext).clamp(0.0, 1.0);
                q[k] = (u * 1024.0).floor().clamp(0.0, 1023.0) as u32;
            }
            expand_bits(q[0]) * 4 + expand_bits(q[1]) * 2 + expand_bits(q[2])
        })
        .collect()
}

/// Structural validation of the Karras tree: internal nodes are [0,N−1), leaves [N−1,2N−1), root = node 0.
/// Every node must be reachable from the root exactly once, and child.parent must point back to the parent.
fn verify_tree(nodes: &[Node], n: usize) -> bool {
    let n_internal = n - 1;
    let n_nodes = 2 * n - 1;
    if nodes[0].parent != NO_PARENT {
        eprintln!("  tree: root parent is {} (expected sentinel)", nodes[0].parent);
        return false;
    }
    let mut visited = vec![0u32; n_nodes];
    let mut stack = vec![0usize]; // root
    let mut leaves_seen = 0usize;
    let mut internals_seen = 0usize;
    while let Some(idx) = stack.pop() {
        if idx >= n_nodes {
            eprintln!("  tree: child index {idx} out of range");
            return false;
        }
        visited[idx] += 1;
        if visited[idx] > 1 {
            eprintln!("  tree: node {idx} reached more than once (cycle / shared child)");
            return false;
        }
        if idx >= n_internal {
            leaves_seen += 1; // leaf node
        } else {
            internals_seen += 1;
            let (l, r) = (nodes[idx].left as usize, nodes[idx].right as usize);
            if nodes[l].parent as usize != idx || nodes[r].parent as usize != idx {
                eprintln!("  tree: node {idx} children parent-pointers inconsistent (l={l} r={r})");
                return false;
            }
            stack.push(l);
            stack.push(r);
        }
    }
    if leaves_seen != n || internals_seen != n_internal {
        eprintln!("  tree: reached {leaves_seen}/{n} leaves, {internals_seen}/{n_internal} internal nodes");
        return false;
    }
    true
}

fn main() {
    let gpu = Gpu::new();
    let n = 1500usize;
    let soft = 5.0e3f64;
    let (bodies, pos, mass) = build_cloud(n, 0xABCD_1234);
    let params = Params::new(n, 0.5, soft, 1); // staged per-stage checks use 1 particle/leaf (classic LBVH)
    let mut bh = Bh::new(&gpu, &bodies, params);
    let mut all_ok = true;

    // Stage 0: GPU direct-sum vs CPU f64 direct-sum — establishes the harness with a known-correct kernel.
    let cpu = cpu_direct(&pos, &mass, soft);
    bh.run(&[("cs_gravity_direct", n as u32)]);
    let gpu_acc = bh.read_acc();
    let (rms, max_rel) = compare(&gpu_acc, &cpu);
    let finite = gpu_acc.iter().all(|a| a.iter().all(|c| c.is_finite()));
    println!("N={n}  direct-sum GPU(f32) vs CPU(f64): RMS rel {rms:.2e}, max per-particle {max_rel:.2e}");
    let ok = finite && rms < 1.0e-2;
    println!("  {}", if ok { "PASS (direct) — GPU direct-sum matches CPU" } else { "FAIL (direct)" });
    all_ok &= ok;

    // Stage 1: adaptive bounding box — GPU float-radix atomicMin/Max vs CPU min/max.
    bh.run(&[("cs_bbox_reset", 6), ("cs_bbox", n as u32)]);
    let (gmin, gmax) = bh.read_bbox();
    let mut cmin = [f32::INFINITY; 3];
    let mut cmax = [f32::NEG_INFINITY; 3];
    for b in &bodies {
        for k in 0..3 {
            cmin[k] = cmin[k].min(b[k]);
            cmax[k] = cmax[k].max(b[k]);
        }
    }
    // Encoding is lossless (bit-exact), so the GPU min/max must EXACTLY equal the CPU reduction.
    let bbox_ok = (0..3).all(|k| gmin[k] == cmin[k] && gmax[k] == cmax[k]);
    println!("bbox GPU min {gmin:?} max {gmax:?}");
    println!("     CPU min {cmin:?} max {cmax:?}");
    println!("  {}", if bbox_ok { "PASS (bbox) — GPU adaptive bbox matches CPU exactly" } else { "FAIL (bbox)" });
    all_ok &= bbox_ok;

    // Stage 2: Morton codes — GPU vs CPU (bit-exact, and order[] is the identity before sorting).
    bh.run(&[("cs_morton", n as u32)]);
    let gcodes = bh.read_u32(&bh.codebuf);
    let gorder = bh.read_u32(&bh.orderbuf);
    let ccodes = cpu_morton(&bodies, gmin, gmax);
    let codes_match = gcodes == ccodes;
    let order_identity = gorder.iter().enumerate().all(|(i, &v)| v as usize == i);
    // Coincident points must share a code: inject a duplicate and check (structural guarantee for the tree).
    let dup_ok = {
        let mut b2 = bodies.clone();
        b2[7] = b2[3]; // force a coincident pair
        let c2 = cpu_morton(&b2, gmin, gmax);
        c2[7] == c2[3]
    };
    println!("morton: {} distinct codes over N={n}", gcodes.iter().collect::<std::collections::HashSet<_>>().len());
    let morton_ok = codes_match && order_identity && dup_ok;
    println!("  {}", if morton_ok { "PASS (morton) — GPU codes match CPU; order=identity; coincident→equal" } else { "FAIL (morton)" });
    all_ok &= morton_ok;

    // Stage 4: Karras tree. CPU-sort the (code,index) pairs (interim; GPU radix sort is stage 3), then build
    // the 2N−1-node tree on the GPU and check its structure: reachability + pointer consistency.
    bh.cpu_sort_upload();
    bh.run(&[("cs_tree_reset", 2 * n as u32 - 1), ("cs_tree", n as u32 - 1)]);
    let nodes = bh.read_nodes();
    let tree_ok = verify_tree(&nodes, n);
    println!("  {}", if tree_ok { "PASS (tree) — Karras tree: every leaf reachable once, pointers consistent" } else { "FAIL (tree)" });
    all_ok &= tree_ok;

    // Stage 5: bottom-up COM — root node must hold Σm, the mass-weighted centroid, and the global AABB. This
    // also validates the atomic-climb's cross-invocation coherence (a stale read would corrupt the root).
    bh.run(&[("cs_com", n as u32)]);
    let nodes = bh.read_nodes();
    let root = nodes[0];
    let total_m: f64 = mass.iter().sum();
    let mut com = [0.0f64; 3];
    for i in 0..n {
        for k in 0..3 {
            com[k] += pos[i][k] * mass[i];
        }
    }
    for k in 0..3 {
        com[k] /= total_m;
    }
    let m_rel = (root.com[3] as f64 - total_m).abs() / total_m;
    let com_err = (0..3).map(|k| (root.com[k] as f64 - com[k]).powi(2)).sum::<f64>().sqrt();
    let com_scale = (0..3).map(|k| com[k].powi(2)).sum::<f64>().sqrt().max(1.0);
    let com_rel = com_err / com_scale;
    let aabb_ok = (0..3).all(|k| root.bmin[k] == gmin[k] && root.bmax[k] == gmax[k]);
    println!("com: root mass rel err {m_rel:.2e}, root COM rel err {com_rel:.2e}, root AABB matches bbox={aabb_ok}");
    let com_ok = m_rel < 1.0e-4 && com_rel < 1.0e-4 && aabb_ok;
    println!("  {}", if com_ok { "PASS (com) — root COM/mass/AABB correct (atomic climb coherent)" } else { "FAIL (com)" });
    all_ok &= com_ok;

    // Stage 6a: θ-traversal accuracy vs the CPU f64 direct sum (ground truth). At θ=0.5 the RMS must be < 1%;
    // as θ→0 the tree opens fully and must recover the direct sum to f32 precision (the strong structural
    // check that every particle is reached exactly once). The tree in the buffers is already built.
    println!("--- Stage 6a: θ-traversal accuracy (N={n}, vs CPU f64 direct) ---");
    let mut sweep_ok = true;
    for &theta in &[0.5f32, 0.25, 0.1, 1.0e-4] {
        bh.set_theta(theta);
        bh.run(&[("cs_gravity_bh", n as u32)]);
        let bh_acc = bh.read_acc();
        let (rms, max_rel) = compare(&bh_acc, &cpu);
        let finite = bh_acc.iter().all(|a| a.iter().all(|c| c.is_finite()));
        let bound = if theta < 1.0e-3 { 1.0e-4 } else { 1.0e-2 };
        let ok = finite && rms < bound;
        sweep_ok &= ok;
        println!("  θ={theta:<7} RMS rel {rms:.3e}  max {max_rel:.3e}  [{}]", if ok { "ok" } else { "FAIL" });
    }
    println!("  {}", if sweep_ok { "PASS (θ-accuracy) — <1% at θ=0.5, recovers direct as θ→0" } else { "FAIL (θ-accuracy)" });
    all_ok &= sweep_ok;

    // Stage 6b: scaling + leaf-bucketing sweep. Build the full GPU tree at growing N for several bucket
    // sizes K (particles/leaf), compare BH vs GPU direct-sum (both f32 → difference is purely θ error), and
    // time the TRAVERSAL kernel. Reported truthfully: GPU direct-sum is coalesced/compute-bound and wins at
    // small N; bucketing collapses tree depth + coalesces leaf sums, pushing the crossover down. The question
    // that decides in-browser wiring: does any K put the crossover below the browser's N (~10–20k)?
    println!("--- Stage 6b: scaling + leaf bucketing (θ=0.5, traversal only; tree BUILD cost separate) ---");
    let ns = [2_000usize, 8_000, 32_000, 128_000];
    let mut scale_acc_ok = true;
    // Direct-sum baseline per N (independent of K).
    let mut t_dir_v = vec![0.0f64; ns.len()];
    let mut best_crossover: Option<usize> = None;
    let mut best_k = 1u32;
    for &k in &[1u32, 8, 16, 32] {
        println!("  bucket_k={k:<3}   N       RMS(BH−direct)   BH ms/eval   direct ms/eval   BH speedup");
        let mut t_bh_v = Vec::new();
        for (idx, &nn) in ns.iter().enumerate() {
            let (bodies_n, _, _) = build_cloud(nn, 0x51A1_7EED ^ nn as u64);
            let mut b = Bh::new(&gpu, &bodies_n, Params::new(nn, 0.5, soft, k));
            b.build_tree();
            b.run(&[("cs_gravity_bh", nn as u32)]);
            let bh_acc = b.read_acc();
            b.run(&[("cs_gravity_direct", nn as u32)]);
            let direct_acc = b.read_acc();
            let (rms, _) = compare(&bh_acc, &to_f64(&direct_acc));
            let t_bh = b.time_kernel("cs_gravity_bh", 5);
            let t_dir = b.time_kernel("cs_gravity_direct", if nn > 40_000 { 2 } else { 5 });
            t_bh_v.push(t_bh);
            t_dir_v[idx] = t_dir; // same across K; last write wins (all equal)
            let acc_ok = rms < 1.0e-2 && bh_acc.iter().all(|a| a.iter().all(|c| c.is_finite()));
            scale_acc_ok &= acc_ok;
            println!(
                "           {nn:>7}   {rms:>10.3e}     {t_bh:>8.3}     {t_dir:>10.3}      {:>6.2}×  [{}]",
                t_dir / t_bh.max(1e-9),
                if acc_ok { "acc ok" } else { "ACC FAIL" }
            );
        }
        // Crossover for this K: smallest measured N where BH beats direct.
        let crossover = ns.iter().zip(t_bh_v.iter().zip(&t_dir_v)).find(|(_, (bh, d))| bh < d).map(|(n, _)| *n);
        match crossover {
            Some(n) => println!("           → crossover N≈{n}"),
            None => println!("           → no crossover within N≤{}", ns.last().unwrap()),
        }
        if let Some(n) = crossover {
            if best_crossover.map_or(true, |b| n < b) {
                best_crossover = Some(n);
                best_k = k;
            }
        }
    }
    // Asymptotic exponent (t ∝ N^p over the top decade) for the classic K=1 tree — small-N points are
    // launch-overhead-bound so understate the order; the top decade shows direct → O(N²), BH → O(N log N).
    let (a, z) = (ns.len() - 2, ns.len() - 1);
    let logslope = |t: &[f64]| (t[z] / t[a]).ln() / (ns[z] as f64 / ns[a] as f64).ln();
    let p_dir = logslope(&t_dir_v);
    println!("  direct-sum asymptotic exponent ({}k→{}k): p≈{p_dir:.2} (→ O(N²))", ns[a] / 1000, ns[z] / 1000);
    match best_crossover {
        Some(n) => println!("  BEST crossover: N≈{n} at bucket_k={best_k}"),
        None => println!("  BEST crossover: none within N≤{} (BH never beats direct-sum in range)", ns.last().unwrap()),
    }
    // Verify bar: CORRECTNESS (accurate at every N/K) + direct → quadratic. Speed vs direct is a reported
    // fact feeding the wiring decision, not a pass/fail on the tree.
    let scale_ok = scale_acc_ok && p_dir > 1.5;
    println!("  {}", if scale_ok { "PASS (scaling) — accurate at all N and K; direct-sum → O(N²)" } else { "FAIL (scaling)" });
    all_ok &= scale_ok;

    println!("{}", if all_ok { "PASS — GPU Barnes-Hut matches direct within the θ bound at all N" } else { "FAIL" });
    std::process::exit(if all_ok { 0 } else { 1 });
}
