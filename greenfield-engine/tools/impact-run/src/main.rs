//! Offline high-N deformable-Earth giant impact on the GPU (docs/33 stage 4c.2). The CPU test
//! `hydrostatic::a_deformable_earth_impact_measures_the_disk_provenance` measured a 58%-Earth orbiting disk
//! at ~2100 particles — a coarse-N, sub-scale IOU. This harness runs the SAME experiment at N~10^4–10^5 on
//! the RTX 2070 using the verified `shaders/sph_step.wgsl` kernel + KDK integrator (stage 4c.1), so the
//! isotopic-crisis number can begin to converge.
//!
//! Pipeline (all on the GPU except cheap O(N) / O(N log N) setup & measurement):
//!   1. build two DIFFERENTIATED bodies (iron core + basalt mantle), equal particle mass  [CPU, cheap]
//!   2. RELAX each to hydrostatic equilibrium with the damped `cs_relax` kernel            [GPU]
//!      (an unrelaxed body dumps startup non-equilibrium into the shock — the 3a lesson)
//!   3. place them: proto-Earth at rest at the origin, Theia inbound at ~1.15·v_esc,
//!      impact parameter b≈R_e (the oblique ~45° geometry that lofts a disk)              [CPU, cheap]
//!   4. KDK-step the impact + aftermath with ADAPTIVE Courant dt (CPU reads back the
//!      per-particle signal-speed min each step; `cs_signal`)                             [GPU]
//!   5. classify each particle remnant / orbiting-disk / escaped by the perigee-above-remnant
//!      criterion and split the disk by provenance (Earth vs Theia)                       [CPU]
//!
//! Usage (docs/40 #3): `cargo run --release -- ensemble [earth_n] [t_hours] [K]` — K perturbed-IC variable-res
//! impacts each integrated to the SAME physical epoch `t_hours`, reporting the converged Earth-fraction ±stdev.
//! Or `cargo run --release -- [earth_n] [t_hours]` for one verbose run. The disk RE-ACCRETES, so the fraction
//! is epoch-dependent — comparing N at a fixed epoch is what makes it converge (docs/41). Kernel verification
//! lives in tools/sph-verify.

const SHADER: &str = include_str!("../../../shaders/sph_step.wgsl");

const G: f64 = 6.674e-11;
const AV_ALPHA: f32 = 1.0;
const AV_BETA: f32 = 2.0;

// ---- Layouts (byte-match the WGSL structs; std430) ----
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Particle {
    pos: [f32; 3],
    h: f32,
    vel: [f32; 3],
    u: f32,
    mass: f32,
    mat: u32,
    rho: f32,
    prov: u32, // 0 = Earth, 1 = Theia
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Eos {
    rho0: f32,
    a: f32,
    b: f32,
    cap_a: f32,
    cap_b: f32,
    e0: f32,
    e_iv: f32,
    e_cv: f32,
    alpha: f32,
    beta: f32,
    _p0: f32,
    _p1: f32,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    softening: f32,
    av_alpha: f32,
    av_beta: f32,
    cell_size: f32,
    table_mask: u32,
    bucket_k: u32,
    dt: f32,
    damp: f32,
    omega: f32, // cs_relax rotating-frame rate (rad/s); 0 for all non-spin-relax dispatches
    _p1: f32,
    _p2: f32,
}

const TABLE_SIZE: u32 = 1 << 16; // 65536 cells
const BUCKET_K: u32 = 256; // deep buckets: a violent impact locally over-packs cells (fixed h); cell-guard exact

// Cited EOS (match eos.rs / sph-verify; basalt = Benz & Asphaug 1999, iron = Wissing & Hobbs 2020 compressed).
fn eos_basalt() -> Eos {
    Eos { rho0: 2700.0, a: 0.5, b: 1.5, cap_a: 2.67e10, cap_b: 2.67e10, e0: 4.87e8, e_iv: 4.72e6, e_cv: 1.82e7, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
}
fn eos_iron() -> Eos {
    Eos { rho0: 7850.0, a: 0.5, b: 1.28, cap_a: 1.28e11, cap_b: 1.815e11, e0: 1.425e7, e_iv: 2.4e6, e_cv: 8.67e6, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
}
const MAT_BASALT: u32 = 0;
const MAT_IRON: u32 = 1;

// ---- CPU body construction (ports hydrostatic.rs new_differentiated / fib_dir / smoothing_for) ----
fn fib_dir(i: usize, n: usize, offset: f64) -> [f64; 3] {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let theta = golden * i as f64 + offset;
    [theta.cos() * r, y, theta.sin() * r]
}
fn smoothing_for(m: f64, rho0: f64) -> f64 {
    2.0 * (m / rho0).cbrt()
}

// A differentiated body: iron core (r < core_radius) + basalt mantle, equal particle mass ≈ M/target_n.
// prov tags every particle with `provenance`. Returns (particles, m_i) — m_i is the equal particle mass.
fn build_differentiated(core_radius: f64, total_radius: f64, u_specific: f64, target_n: usize, provenance: u32) -> (Vec<Particle>, f64) {
    let (iron, basalt) = (eos_iron(), eos_basalt());
    const FTP: f64 = 4.0 / 3.0 * std::f64::consts::PI;
    let v_core = FTP * core_radius.powi(3);
    let v_mantle = FTP * (total_radius.powi(3) - core_radius.powi(3));
    let m_core = iron.rho0 as f64 * v_core;
    let m_mantle = basalt.rho0 as f64 * v_mantle;
    let m_i = (m_core + m_mantle) / target_n as f64;
    let n_core = (m_core / m_i).round().max(1.0) as usize;
    let n_mantle = (m_mantle / m_i).round().max(1.0) as usize;
    let mut ps = Vec::with_capacity(n_core + n_mantle);
    let mk = |pos: [f64; 3], mat: u32, rho0: f64| Particle {
        pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32],
        h: smoothing_for(m_i, rho0) as f32,
        vel: [0.0; 3],
        u: u_specific as f32,
        mass: m_i as f32,
        mat,
        rho: rho0 as f32,
        prov: provenance,
    };
    for i in 0..n_core {
        let rr = core_radius * ((i as f64 + 0.5) / n_core as f64).cbrt();
        let d = fib_dir(i, n_core, 0.0);
        ps.push(mk([d[0] * rr, d[1] * rr, d[2] * rr], MAT_IRON, iron.rho0 as f64));
    }
    let (rc3, rt3) = (core_radius.powi(3), total_radius.powi(3));
    for i in 0..n_mantle {
        let rr = (rc3 + (rt3 - rc3) * (i as f64 + 0.5) / n_mantle as f64).cbrt();
        let d = fib_dir(i, n_mantle, 1.7);
        ps.push(mk([d[0] * rr, d[1] * rr, d[2] * rr], MAT_BASALT, basalt.rho0 as f64));
    }
    (ps, m_i)
}

// docs/40 #3 step 1 — VARIABLE-RESOLUTION ("LOD") differentiated body (ports hydrostatic.rs run_lod_impact):
// a COARSE iron core (particle mass `m_fine*coarse_factor`, larger `h`) + a FINE basalt mantle (mass `m_fine`).
// `sph_step.wgsl` handles mixed h/mass natively (per-pair h_ij=½(h_i+h_j); grid cell_size = max h), so this is
// just seeding — no kernel change. #1 found the deformable coarse core is the win (63% vs the rigid ~25%).
// Returns (particles, m_fine) — m_fine sets the finest spacing (softening).
fn build_lod(core_radius: f64, total_radius: f64, u_specific: f64, m_fine: f64, coarse_factor: f64, provenance: u32) -> (Vec<Particle>, f64) {
    let (iron, basalt) = (eos_iron(), eos_basalt());
    const FTP: f64 = 4.0 / 3.0 * std::f64::consts::PI;
    let m_coarse = m_fine * coarse_factor;
    let m_core = iron.rho0 as f64 * FTP * core_radius.powi(3);
    let m_mantle = basalt.rho0 as f64 * FTP * (total_radius.powi(3) - core_radius.powi(3));
    let n_core = (m_core / m_coarse).round().max(1.0) as usize;
    let n_mantle = (m_mantle / m_fine).round().max(1.0) as usize;
    let mut ps = Vec::with_capacity(n_core + n_mantle);
    let mk = |pos: [f64; 3], mat: u32, rho0: f64, m: f64| Particle {
        pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32],
        h: smoothing_for(m, rho0) as f32,
        vel: [0.0; 3],
        u: u_specific as f32,
        mass: m as f32,
        mat,
        rho: rho0 as f32,
        prov: provenance,
    };
    for i in 0..n_core {
        let rr = core_radius * ((i as f64 + 0.5) / n_core as f64).cbrt();
        let d = fib_dir(i, n_core, 0.0);
        ps.push(mk([d[0] * rr, d[1] * rr, d[2] * rr], MAT_IRON, iron.rho0 as f64, m_coarse));
    }
    let (rc3, rt3) = (core_radius.powi(3), total_radius.powi(3));
    for i in 0..n_mantle {
        let rr = (rc3 + (rt3 - rc3) * (i as f64 + 0.5) / n_mantle as f64).cbrt();
        let d = fib_dir(i, n_mantle, 1.7);
        ps.push(mk([d[0] * rr, d[1] * rr, d[2] * rr], MAT_BASALT, basalt.rho0 as f64, m_fine));
    }
    (ps, m_fine)
}

// docs/40 #3 step 3 — deterministic per-(run,particle,axis) jitter in [-1,1). A splitmix64 hash, NOT
// `Math.random`/rand: the ensemble must be reproducible (same K → same numbers), the perturbation only breaks
// the microscopic symmetry so each run is an independent chaotic realization of the SAME macroscopic impact.
fn hash_jitter(run: u64, i: u64, axis: u64) -> f64 {
    let mut x = run
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ i.wrapping_mul(0xD1B5_4A32_D192_ED03)
        ^ axis.wrapping_mul(0xCA5A_8263_9512_1157)
        ^ 0x2545_F491_4F6C_DD1D;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x as f64 / u64::MAX as f64) * 2.0 - 1.0
}

// docs/40 #3 step 2 — order-independent reduction. Sorting the terms by magnitude then Kahan-summing makes the
// sum invariant to the order the terms arrive in (GPU readback / classification order), so measuring the same
// snapshot twice gives a bit-identical fraction. (The SIM still scatters — chaos — but the MEASUREMENT is
// deterministic; that separation is the whole point of the ensemble.)
fn sum_oi(terms: &mut Vec<f64>) -> f64 {
    terms.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap());
    let (mut s, mut c) = (0.0f64, 0.0f64); // Kahan compensated summation
    for &t in terms.iter() {
        let y = t - c;
        let z = s + y;
        c = (z - s) - y;
        s = z;
    }
    s
}

fn com(ps: &[Particle]) -> [f64; 3] {
    let mut c = [0.0f64; 3];
    let mut m = 0.0;
    for p in ps {
        for k in 0..3 { c[k] += p.pos[k] as f64 * p.mass as f64; }
        m += p.mass as f64;
    }
    [c[0] / m, c[1] / m, c[2] / m]
}
fn body_radius(ps: &[Particle]) -> f64 {
    let c = com(ps);
    ps.iter().map(|p| { let d = [p.pos[0] as f64 - c[0], p.pos[1] as f64 - c[1], p.pos[2] as f64 - c[2]]; (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() }).fold(0.0, f64::max)
}
// Equatorial (x-y plane) and polar (|z|) extents about the COM — a rotating equilibrium is oblate (r_eq > r_pol).
fn earth_shape(ps: &[Particle]) -> (f64, f64) {
    let c = com(ps);
    let (mut r_eq, mut r_pol) = (0.0f64, 0.0f64);
    for p in ps {
        let (x, y, z) = (p.pos[0] as f64 - c[0], p.pos[1] as f64 - c[1], p.pos[2] as f64 - c[2]);
        r_eq = r_eq.max((x * x + y * y).sqrt());
        r_pol = r_pol.max(z.abs());
    }
    (r_eq, r_pol)
}
// perigee of a bound orbit (None if unbound) — ports orbit::perigee.
fn perigee(rel_p: [f64; 3], rel_v: [f64; 3], mu: f64) -> Option<f64> {
    let r = (rel_p[0] * rel_p[0] + rel_p[1] * rel_p[1] + rel_p[2] * rel_p[2]).sqrt();
    if r == 0.0 { return Some(0.0); }
    let v2 = rel_v[0] * rel_v[0] + rel_v[1] * rel_v[1] + rel_v[2] * rel_v[2];
    let energy = 0.5 * v2 - mu / r;
    if energy >= 0.0 { return None; }
    let a = -mu / (2.0 * energy);
    let h = [
        rel_p[1] * rel_v[2] - rel_p[2] * rel_v[1],
        rel_p[2] * rel_v[0] - rel_p[0] * rel_v[2],
        rel_p[0] * rel_v[1] - rel_p[1] * rel_v[0],
    ];
    let h2 = h[0] * h[0] + h[1] * h[1] + h[2] * h[2];
    let e = (1.0 + 2.0 * energy * h2 / (mu * mu)).max(0.0).sqrt();
    Some(a * (1.0 - e))
}

// ---- GPU context: the sph_step.wgsl pipelines + per-run buffers ----
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    p_clear: wgpu::ComputePipeline,
    p_insert: wgpu::ComputePipeline,
    p_density: wgpu::ComputePipeline,
    p_forces: wgpu::ComputePipeline,
    p_signal: wgpu::ComputePipeline,
    p_kd: wgpu::ComputePipeline,
    p_k: wgpu::ComputePipeline,
    p_relax: wgpu::ComputePipeline,
}
impl Gpu {
    fn new() -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, force_fallback_adapter: false })).expect("no Vulkan adapter (RTX 2070 expected)");
        println!("adapter: {}", adapter.get_info().name);
        // Request the adapter's actual limits (grid_bucket is ~64 MB at BUCKET_K=256, above the 128 MB default
        // storage-binding cap only at larger tables — asking for the adapter max is the safe superset).
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("impact-run"), required_features: wgpu::Features::empty(), required_limits: adapter.limits(), memory_hints: wgpu::MemoryHints::Performance }, None)).expect("request_device");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sph_step"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let storage = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry { binding: b, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("l"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                storage(1, false), storage(2, true), storage(3, false), storage(4, false), storage(5, false), storage(6, false), storage(7, false),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&layout], push_constant_ranges: &[] });
        let mk = |e: &str| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: Some(&pl), module: &module, entry_point: Some(e), compilation_options: Default::default(), cache: None });
        Gpu {
            p_clear: mk("cs_grid_clear"), p_insert: mk("cs_grid_insert"), p_density: mk("cs_density"),
            p_forces: mk("cs_forces"), p_signal: mk("cs_signal"), p_kd: mk("cs_kick_drift"),
            p_k: mk("cs_kick"), p_relax: mk("cs_relax"), layout, device, queue,
        }
    }

    // Allocate the per-run buffers + bind group for `particles`.
    fn make_buffers(&self, particles: &[Particle], eos: &[Eos], params: &Params) -> Buffers {
        use wgpu::util::DeviceExt;
        let n = particles.len() as u64;
        let d = &self.device;
        let pbuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("particles"), contents: bytemuck::cast_slice(particles), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC });
        let ubuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("params"), contents: bytemuck::bytes_of(params), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
        let ebuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("eos"), contents: bytemuck::cast_slice(eos), usage: wgpu::BufferUsages::STORAGE });
        let abuf = d.create_buffer(&wgpu::BufferDescriptor { label: Some("acc"), size: n * 16, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let dbuf = d.create_buffer(&wgpu::BufferDescriptor { label: Some("dudt"), size: n * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let sbuf = d.create_buffer(&wgpu::BufferDescriptor { label: Some("signal"), size: n * 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let gcount = d.create_buffer(&wgpu::BufferDescriptor { label: Some("grid_count"), size: (TABLE_SIZE as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let gbucket = d.create_buffer(&wgpu::BufferDescriptor { label: Some("grid_bucket"), size: (TABLE_SIZE as u64) * (BUCKET_K as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: pbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: ebuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: abuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: dbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: gcount.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: gbucket.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: sbuf.as_entire_binding() },
            ],
        });
        Buffers { pbuf, ubuf, abuf: _hold(abuf), dbuf: _hold(dbuf), sbuf, _grid: (gcount, gbucket), bind, n: n as u32 }
    }

    fn dispatch(&self, enc: &mut wgpu::CommandEncoder, b: &Buffers, pipe: &wgpu::ComputePipeline, threads: u32) {
        let mut p = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        p.set_pipeline(pipe);
        p.set_bind_group(0, &b.bind, &[]);
        p.dispatch_workgroups(threads.div_ceil(64), 1, 1);
    }
    // clear → insert → density → forces (one full force evaluation).
    fn force_eval(&self, enc: &mut wgpu::CommandEncoder, b: &Buffers) {
        self.dispatch(enc, b, &self.p_clear, TABLE_SIZE);
        self.dispatch(enc, b, &self.p_insert, b.n);
        self.dispatch(enc, b, &self.p_density, b.n);
        self.dispatch(enc, b, &self.p_forces, b.n);
    }
    fn read_particles(&self, b: &Buffers) -> Vec<Particle> {
        let size = (b.n as u64) * std::mem::size_of::<Particle>() as u64;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&b.pbuf, 0, &staging, 0, size);
        self.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let out = bytemuck::cast_slice::<u8, Particle>(&slice.get_mapped_range()).to_vec();
        out
    }
    // Read back the signal[] buffer and return cfl · min_i signal_i  = the adaptive Courant dt.
    fn read_dt(&self, b: &Buffers, cfl: f64) -> f64 {
        let size = (b.n as u64) * 4;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&b.sbuf, 0, &staging, 0, size);
        self.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let v = bytemuck::cast_slice::<u8, f32>(&slice.get_mapped_range()).to_vec();
        cfl * v.iter().cloned().fold(f64::INFINITY, |a, x| a.min(x as f64))
    }

    // Damped GPU relaxation at a fixed dt (chosen from the initial signal min) for `steps` steps.
    fn relax(&self, particles: &[Particle], eos: &[Eos], soft: f64, cfl: f64, damp: f64, steps: usize, omega: f64) -> Vec<Particle> {
        let cell_size = particles.iter().map(|p| p.h).fold(0.0f32, f32::max);
        // AV-FREE relaxation (docs/35): the CPU `HydroBody::relax_step` settles on gravity + SPH pressure only
        // (`accelerations()`, no Monaghan AV). AV is a velocity-dependent dissipation for APPROACHING particles;
        // leaving it on during the damped settle corrupts the equilibrium and the subsequent impact DISPERSES
        // (loses orbits) — the docs/35 GPU finding. AV is restored (α=1,β=2) for the impact, where the shock is.
        let mut params = Params { n: particles.len() as u32, softening: soft as f32, av_alpha: 0.0, av_beta: 0.0, cell_size, table_mask: TABLE_SIZE - 1, bucket_k: BUCKET_K, dt: 0.0, damp: damp as f32, omega: omega as f32, _p1: 0.0, _p2: 0.0 };
        let b = self.make_buffers(particles, eos, &params);
        // dt from the initial state (density → signal); fixed for the relaxation.
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.dispatch(&mut enc, &b, &self.p_clear, TABLE_SIZE);
        self.dispatch(&mut enc, &b, &self.p_insert, b.n);
        self.dispatch(&mut enc, &b, &self.p_density, b.n);
        self.dispatch(&mut enc, &b, &self.p_signal, b.n);
        self.queue.submit(Some(enc.finish()));
        let dt = self.read_dt(&b, cfl);
        params.dt = dt as f32;
        self.queue.write_buffer(&b.ubuf, 0, bytemuck::bytes_of(&params));
        // relaxation steps: each = force eval + damped kick.
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for _ in 0..steps {
            self.force_eval(&mut enc, &b);
            self.dispatch(&mut enc, &b, &self.p_relax, b.n);
        }
        self.queue.submit(Some(enc.finish()));
        self.device.poll(wgpu::Maintain::Wait);
        println!("  relax: dt={:.3}s × {} steps = {:.0}s physical", dt, steps, dt * steps as f64);
        self.read_particles(&b)
    }

    // KDK impact with adaptive Courant dt (per-step signal read-back). Integrates through the ascending
    // `checkpoints` (seconds), reading back a snapshot at each — so ONE run reveals the disk's time-evolution
    // (does it plateau = rotationally sustained, or decay = re-accrete?). A checkpoint read-back copies the
    // particle buffer without disturbing the sim, so integration continues unaffected. Returns one snapshot per
    // checkpoint (final state padded in if the max_steps safety cap is hit early) and the total physical time.
    fn impact(&self, particles: &[Particle], eos: &[Eos], soft: f64, cfl: f64, max_steps: usize, checkpoints: &[f64]) -> (Vec<Vec<Particle>>, f64) {
        let cell_size = particles.iter().map(|p| p.h).fold(0.0f32, f32::max);
        let mut params = Params { n: particles.len() as u32, softening: soft as f32, av_alpha: AV_ALPHA, av_beta: AV_BETA, cell_size, table_mask: TABLE_SIZE - 1, bucket_k: BUCKET_K, dt: 0.0, damp: 1.0, omega: 0.0, _p1: 0.0, _p2: 0.0 };
        let b = self.make_buffers(particles, eos, &params);
        let t_end = checkpoints.last().copied().unwrap_or(0.0);
        let mut snaps: Vec<Vec<Particle>> = Vec::with_capacity(checkpoints.len());
        let mut t = 0.0f64;
        let mut s = 0usize;
        while t < t_end && s < max_steps {
            // eval 1 (+ signal) → read adaptive dt → half-kick+drift
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            self.force_eval(&mut enc, &b);
            self.dispatch(&mut enc, &b, &self.p_signal, b.n);
            self.queue.submit(Some(enc.finish()));
            let dt = self.read_dt(&b, cfl);
            params.dt = dt as f32;
            self.queue.write_buffer(&b.ubuf, 0, bytemuck::bytes_of(&params));
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            self.dispatch(&mut enc, &b, &self.p_kd, b.n);
            // eval 2 → final half-kick
            self.force_eval(&mut enc, &b);
            self.dispatch(&mut enc, &b, &self.p_k, b.n);
            self.queue.submit(Some(enc.finish()));
            t += dt;
            s += 1;
            // read back a snapshot for every checkpoint the step just crossed
            while snaps.len() < checkpoints.len() && t >= checkpoints[snaps.len()] {
                self.device.poll(wgpu::Maintain::Wait);
                snaps.push(self.read_particles(&b));
            }
            if s % 500 == 0 {
                self.device.poll(wgpu::Maintain::Wait);
                println!("  impact step {:>5} (≤{})  dt={:.3}s  t={:.0}s ({:.2}/{:.2} h)", s, max_steps, dt, t, t / 3600.0, t_end / 3600.0);
            }
        }
        // pad any un-reached checkpoints with the final state (max_steps cap hit before t_end)
        self.device.poll(wgpu::Maintain::Wait);
        while snaps.len() < checkpoints.len() { snaps.push(self.read_particles(&b)); }
        println!("  impact done: {} steps → t={:.2} h ({} checkpoints)", s, t / 3600.0, checkpoints.len());
        (snaps, t)
    }
}

// Buffer bundle for one GPU run. `_grid`/`abuf`/`dbuf` are held so they outlive the bind group.
struct Buffers {
    pbuf: wgpu::Buffer,
    ubuf: wgpu::Buffer,
    abuf: Hold,
    dbuf: Hold,
    sbuf: wgpu::Buffer,
    _grid: (wgpu::Buffer, wgpu::Buffer),
    bind: wgpu::BindGroup,
    n: u32,
}
struct Hold(#[allow(dead_code)] wgpu::Buffer);
fn _hold(b: wgpu::Buffer) -> Hold { Hold(b) }

// Total energy of the system: KE + IE + gravitational PE. PE is O(N²) — computed only when `with_pe` (small N).
fn total_energy(ps: &[Particle], soft: f64, with_pe: bool) -> (f64, f64, f64) {
    let mut ke = 0.0;
    let mut ie = 0.0;
    for p in ps {
        let v2 = (p.vel[0] * p.vel[0] + p.vel[1] * p.vel[1] + p.vel[2] * p.vel[2]) as f64;
        ke += 0.5 * p.mass as f64 * v2;
        ie += p.mass as f64 * p.u as f64;
    }
    let mut pe = 0.0;
    if with_pe {
        let s2 = soft * soft;
        for i in 0..ps.len() {
            for j in (i + 1)..ps.len() {
                let d = [ps[i].pos[0] as f64 - ps[j].pos[0] as f64, ps[i].pos[1] as f64 - ps[j].pos[1] as f64, ps[i].pos[2] as f64 - ps[j].pos[2] as f64];
                let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2] + s2).sqrt();
                pe -= G * ps[i].mass as f64 * ps[j].mass as f64 / r;
            }
        }
    }
    (ke, ie, pe)
}

// Earth = variable-resolution (coarse iron core + fine basalt mantle); Theia = uniform differentiated at the
// fine mass. The coarse core carries `COARSE_FACTOR`× the fine particle mass (docs/40 #1: deformable-coarse win).
const COARSE_FACTOR: f64 = 8.0;
const R_SURF: f64 = 5.0e6; // sub-Earth scale (tractable direct-sum); r_core = R_SURF/2
const R_THEIA: f64 = 2.7e6; // ~1/7 Earth mass, differentiated (iron core essential — basalt sphere sheds ~0%)

// Build + relax (once) the LOD Earth and the fine Theia at the given fine mass. `earth_n` sets the TOTAL Earth
// particle count (m_fine is solved from it and COARSE_FACTOR). Returns (earth_relaxed, theia_relaxed, soft).
fn build_and_relax(gpu: &Gpu, eos: &[Eos], earth_n: usize, omega_relax: f64) -> (Vec<Particle>, Vec<Particle>, f64) {
    const FTP: f64 = 4.0 / 3.0 * std::f64::consts::PI;
    let r_ic = 0.5 * R_SURF;
    let (iron, basalt) = (eos_iron(), eos_basalt());
    let m_mantle = basalt.rho0 as f64 * FTP * (R_SURF.powi(3) - r_ic.powi(3));
    let m_core = iron.rho0 as f64 * FTP * r_ic.powi(3);
    // total Earth count = n_mantle + n_core = (m_mantle + m_core/cf)/m_fine  ⇒ solve m_fine from earth_n.
    let m_fine = (m_mantle + m_core / COARSE_FACTOR) / earth_n as f64;
    let (mut earth, _) = build_lod(r_ic, R_SURF, 1.0e6, m_fine, COARSE_FACTOR, 0);
    // Theia: uniform differentiated, equal particle mass ≈ m_fine (so exactly two mass classes system-wide).
    let m_theia_tot = iron.rho0 as f64 * FTP * (0.5 * R_THEIA).powi(3) + basalt.rho0 as f64 * FTP * (R_THEIA.powi(3) - (0.5 * R_THEIA).powi(3));
    let theia_n = (m_theia_tot / m_fine).round().max(50.0) as usize;
    let (mut theia, _) = build_differentiated(0.5 * R_THEIA, R_THEIA, 1.0e6, theia_n, 1);
    let soft = 0.5 * (m_fine / basalt.rho0 as f64).cbrt(); // finest (fine basalt) spacing, matches run_lod_impact
    let n_core = earth.iter().filter(|p| p.mass as f64 > 1.5 * m_fine).count();
    println!("build: Earth {} particles ({} coarse core @ {:.1}×m_fine + {} fine mantle), Theia {} particles, m_fine={:.2e} kg, soft={:.0} m",
        earth.len(), n_core, COARSE_FACTOR, earth.len() - n_core, theia.len(), m_fine, soft);

    if omega_relax != 0.0 {
        println!("relaxing Earth ({} particles) in the ROTATING frame at ω={:.2e} rad/s (oblate equilibrium)...", earth.len(), omega_relax);
    } else {
        println!("relaxing Earth ({} particles)...", earth.len());
    }
    earth = gpu.relax(&earth, eos, soft, 0.2, 0.94, (earth.len() / 3 + 1500).min(6000), omega_relax);
    println!("relaxing Theia ({} particles)...", theia.len());
    theia = gpu.relax(&theia, eos, soft, 0.2, 0.94, (theia.len() / 3 + 1500).min(6000), 0.0);
    // report the equatorial vs polar radius (oblateness) so a spun relaxation is visibly flattened
    let (r_eq, r_pol) = earth_shape(&earth);
    println!("post-relax radii: R_earth={:.0} km (eq {:.0} / pol {:.0} → flattening {:.3}), R_theia={:.0} km", body_radius(&earth) / 1e3, r_eq / 1e3, r_pol / 1e3, (r_eq - r_pol) / r_eq.max(1.0), body_radius(&theia) / 1e3);
    (earth, theia, soft)
}

// The collision initial condition. `b_over_re` is the impact parameter in units of R_earth (baseline 1.0;
// larger = more grazing = more ORBITAL angular momentum). `omega` is a pre-impact SPIN of proto-Earth about the
// orbit-normal (+z) axis in rad/s (adds SPIN angular momentum, coherent with the orbital L). The spin IOU
// (docs/41): with too little L the disk re-accretes; raising L (grazing and/or spin) should let it plateau.
#[derive(Clone, Copy)]
struct Ic {
    b_over_re: f64,
    omega: f64,
}
impl Default for Ic {
    fn default() -> Self { Ic { b_over_re: 1.0, omega: 0.0 } } // the docs/41 baseline (b = R_earth, no spin)
}

// Net angular momentum about +z (the orbital-plane normal): Σ mᵢ (xᵢ vyᵢ − yᵢ vxᵢ), about the system COM.
fn angular_momentum_z(body: &[Particle]) -> f64 {
    let c = com(body);
    let mut lz = Vec::with_capacity(body.len());
    for p in body {
        let (x, y) = (p.pos[0] as f64 - c[0], p.pos[1] as f64 - c[1]);
        lz.push(p.mass as f64 * (x * p.vel[1] as f64 - y * p.vel[0] as f64));
    }
    sum_oi(&mut lz)
}

// Place the relaxed bodies into the collision IC (`ic`), optionally apply a tiny deterministic jitter (an
// ensemble realization), integrate the impact on the GPU, and return the order-independent measurement AT EACH
// checkpoint epoch (seconds, ascending). `jitter_run = None` is the nominal impact; `verbose` prints diagnostics.
fn run_and_measure(gpu: &Gpu, eos: &[Eos], earth_relaxed: &[Particle], theia_relaxed: &[Particle], soft: f64, ic: Ic, checkpoints: &[f64], jitter_run: Option<u64>, verbose: bool) -> Vec<Measure> {
    const MAX_STEPS: usize = 60000; // safety cap; the physical-time checkpoints are the real stop
    let mut earth = earth_relaxed.to_vec();
    let mut theia = theia_relaxed.to_vec();
    let (m_earth, m_theia): (f64, f64) = (earth.iter().map(|p| p.mass as f64).sum(), theia.iter().map(|p| p.mass as f64).sum());
    let (r_e, r_t) = (body_radius(&earth), body_radius(&theia));
    let n_earth = earth.len();
    let contact = r_e + r_t;
    let v_esc = 1.15 * (2.0 * G * (m_earth + m_theia) / contact).sqrt();
    let (d0, b_param) = (1.6 * contact, ic.b_over_re * r_e);
    // Proto-Earth at the origin, spun as a rigid body about +z: v = ω ẑ × r = ω(−y, x, 0). (Applied to the
    // relaxed sphere and impacted promptly — not re-relaxed into a rotational equilibrium; ω is kept modest.)
    let ec = com(&earth);
    for p in earth.iter_mut() {
        for k in 0..3 { p.pos[k] -= ec[k] as f32; }
        let (x, y) = (p.pos[0] as f64, p.pos[1] as f64);
        p.vel = [(-ic.omega * y) as f32, (ic.omega * x) as f32, 0.0];
    }
    let tc = com(&theia);
    let offset = [d0, b_param, 0.0];
    for p in theia.iter_mut() {
        for k in 0..3 { p.pos[k] = p.pos[k] - tc[k] as f32 + offset[k] as f32; }
        p.vel = [-v_esc as f32, 0.0, 0.0];
    }
    let mut body = earth;
    body.extend(theia);
    // A tiny deterministic position jitter (0.1% of the fine inter-particle spacing) breaks the microscopic
    // symmetry so each ensemble run is an independent chaotic realization of the SAME macroscopic impact.
    if let Some(run) = jitter_run {
        let amp = 1.0e-3 * 2.0 * soft; // spacing = 2·soft (soft = ½·spacing)
        for (i, p) in body.iter_mut().enumerate() {
            for k in 0..3 { p.pos[k] += (amp * hash_jitter(run, i as u64, k as u64)) as f32; }
        }
    }

    let with_pe = verbose && body.len() <= 40000; // O(N²) CPU PE — only for the single-run diagnostic
    let (ke0, ie0, pe0) = total_energy(&body, soft, with_pe);
    let lz0 = angular_momentum_z(&body);
    if verbose {
        println!("collision: M_e={:.3e} kg, M_t={:.3e} kg, v_esc={:.0} m/s, b={:.2}·R_e, ω={:.2e} rad/s, L_z={:.3e} kg·m²/s, N={}", m_earth, m_theia, v_esc, ic.b_over_re, ic.omega, lz0, body.len());
        println!("energy before: KE={:.3e} IE={:.3e}{}", ke0, ie0, if with_pe { format!(" PE={:.3e} TOT={:.3e}", pe0, ke0 + ie0 + pe0) } else { " (PE skipped)".into() });
    }
    let (snaps, _t_total) = gpu.impact(&body, eos, soft, 0.1, MAX_STEPS, checkpoints);
    let measures: Vec<Measure> = snaps.iter().map(|s| measure(s, n_earth)).collect();
    if verbose {
        let last = snaps.last().unwrap();
        if with_pe {
            let (ke1, ie1, pe1) = total_energy(last, soft, true);
            let (e0, e1) = (ke0 + ie0 + pe0, ke1 + ie1 + pe1);
            println!("energy after:  KE={:.3e} IE={:.3e} PE={:.3e} TOT={:.3e}  (ΔTOT/|TOT0| = {:.1}%, L_z {:.3e}→{:.3e})", ke1, ie1, pe1, e1, 100.0 * (e1 - e0).abs() / e0.abs(), lz0, angular_momentum_z(last));
        }
        print_measure(measures.last().unwrap(), m_earth, m_theia, v_esc);
        // step-2 self-check: the reduction is order-independent — re-measure the SAME snapshot, assert identical.
        let m2 = measure(last, n_earth);
        assert_eq!(measures.last().unwrap().earth_frac.to_bits(), m2.earth_frac.to_bits(), "measurement not order-independent");
        println!("  [order-independent reduction verified: re-measure bit-identical]");
    }
    measures
}

fn mean_std(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    if n == 0.0 { return (0.0, 0.0); }
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    (mean, var.sqrt())
}

// docs/41 spin IOU — run K perturbed-IC impacts with the IC `ic`, measuring the disk at 4 epochs, so the disk's
// TIME-EVOLUTION shows PLATEAU (rotationally sustained) vs DECAY (re-accretes). `relax_omega` picks the proto-
// Earth relaxation: 0 = spherical hydrostatic + startup spin (the fast check); ω = a rotating-frame OBLATE
// equilibrium (the cross-check that the sustained disk is not a startup-non-equilibrium artifact).
fn spin_sweep(gpu: &Gpu, eos: &[Eos], earth_n: usize, ic: Ic, t_max_h: f64, k: usize, relax_omega: f64, label: &str) {
    let cps: Vec<f64> = (1..=4).map(|q| t_max_h * 3600.0 * q as f64 / 4.0).collect();
    println!("=== docs/41 {}: earth_n={}, ω={:.2e} rad/s, b={:.2}·R_e, K={}, relax_ω={:.2e}, epochs={:.1}/{:.1}/{:.1}/{:.1} h ===",
        label, earth_n, ic.omega, ic.b_over_re, k, relax_omega, cps[0] / 3600.0, cps[1] / 3600.0, cps[2] / 3600.0, cps[3] / 3600.0);
    let (earth, theia, soft) = build_and_relax(gpu, eos, earth_n, relax_omega);
    let (mut fr, mut dk): (Vec<Vec<f64>>, Vec<Vec<f64>>) = (vec![vec![]; 4], vec![vec![]; 4]);
    let mut moon: [usize; 4] = [0; 4];
    for run in 0..k {
        let ms = run_and_measure(gpu, eos, &earth, &theia, soft, ic, &cps, Some(run as u64), false);
        for (e, m) in ms.iter().enumerate() {
            if m.disk_kg > 0.0 { fr[e].push(m.earth_frac); }
            dk[e].push(m.disk_kg / M_MOON);
            if m.clump_kg > 0.0 { moon[e] += 1; }
        }
        println!("  run {:>2}: disk M☾ by epoch = [{}]  | Earth% = [{}]", run,
            ms.iter().map(|m| format!("{:.3}", m.disk_kg / M_MOON)).collect::<Vec<_>>().join(", "),
            ms.iter().map(|m| format!("{:.0}", m.earth_frac)).collect::<Vec<_>>().join(", "));
    }
    println!("\n=== TIME-EVOLUTION (ω={:.2e}, b={:.2}·R_e, relax_ω={:.2e}, K={}) — PLATEAU or DECAY? ===", ic.omega, ic.b_over_re, relax_omega, k);
    for e in 0..4 {
        let (fm, fs) = mean_std(&fr[e]);
        let (dm, ds) = mean_std(&dk[e]);
        println!("  {:>5.1} h:  disk {:.3} ± {:.3} M☾  | Earth {:.1}% ± {:.1}%  | Moon {}/{}", cps[e] / 3600.0, dm, ds, fm, fs, moon[e], k);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gpu = Gpu::new();
    let eos = [eos_basalt(), eos_iron()];

    match args.get(1).map(|s| s.as_str()) {
        Some("ensemble") => {
            // Usage: ensemble [earth_n] [t_hours] [K] — K perturbed-IC runs to the SAME epoch (docs/41).
            let earth_n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1800);
            let t_hours: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(11.0);
            let k: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(8);
            println!("=== docs/40 #3 ENSEMBLE: K={} perturbed-IC variable-res impacts, earth_n={}, epoch={:.1} h ===", k, earth_n, t_hours);
            let (earth, theia, soft) = build_and_relax(&gpu, &eos, earth_n, 0.0);
            let cps = [t_hours * 3600.0];
            let (mut fracs, mut disks, mut clumps): (Vec<f64>, Vec<f64>, Vec<f64>) = (vec![], vec![], vec![]);
            let mut n_with_moon = 0;
            for run in 0..k {
                let m = run_and_measure(&gpu, &eos, &earth, &theia, soft, Ic::default(), &cps, Some(run as u64), false)[0];
                if m.disk_kg > 0.0 { fracs.push(m.earth_frac); }
                disks.push(m.disk_kg / M_MOON);
                clumps.push(m.clump_kg / M_MOON);
                if m.clump_kg > 0.0 { n_with_moon += 1; }
                println!("  run {:>2}: {:>4.0}% Earth  | disk {:.3} M☾ | largest clump {:.3} M☾ ({} clumps, {} bound){}",
                    run, m.earth_frac, m.disk_kg / M_MOON, m.clump_kg / M_MOON, m.n_clumps, m.n_bound, if m.clump_kg > 0.0 { " ← Moon" } else { "" });
            }
            let (f_mean, f_std) = mean_std(&fracs);
            let (d_mean, d_std) = mean_std(&disks);
            let (c_mean, c_std) = mean_std(&clumps);
            println!("\n=== CONVERGED (K={} runs) ===", k);
            println!("  EARTH-FRACTION: {:.1}% ± {:.1}%  (n={} disk-forming runs; stdev = the chaos scatter)", f_mean, f_std, fracs.len());
            println!("  DISK MASS:      {:.3} ± {:.3} M☾", d_mean, d_std);
            println!("  LARGEST CLUMP:  {:.3} ± {:.3} M☾  · bound Moon-mass clump accreted in {}/{} runs", c_mean, c_std, n_with_moon, k);
        }
        Some("spin") => {
            // docs/41 spin IOU: Usage: spin [earth_n] [omega] [b_over_re] [t_max_h] [K]. Run K perturbed-IC
            // impacts with a pre-spin ω and grazing b, measuring the disk at 4 epochs (t_max·{¼,½,¾,1}) — so the
            // disk's TIME-EVOLUTION shows whether added angular momentum makes it PLATEAU (sustained) or DECAY
            // (re-accrete). Baseline (ω=0, b=1.0) re-accretes (docs/41 Finding A).
            let earth_n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2400);
            let omega: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let b_over_re: f64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            let t_max_h: f64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(24.0);
            let k: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(6);
            // spin: spherical relaxation + startup spin (relax_ω = 0)
            spin_sweep(&gpu, &eos, earth_n, Ic { b_over_re, omega }, t_max_h, k, 0.0, "SPIN IOU");
        }
        Some("spineq") => {
            // Cross-check: same as `spin` but proto-Earth is relaxed in the ROTATING frame first (oblate
            // equilibrium), so the sustained-disk result can't be a startup-non-equilibrium artifact.
            // Usage: spineq [earth_n] [omega] [b_over_re] [t_max_h] [K]
            let earth_n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2400);
            let omega: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(7.0e-4);
            let b_over_re: f64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            let t_max_h: f64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(24.0);
            let k: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(6);
            // spineq: rotating-frame oblate-equilibrium relaxation at the SAME ω as the impact
            spin_sweep(&gpu, &eos, earth_n, Ic { b_over_re, omega }, t_max_h, k, omega, "SPIN-EQ CROSS-CHECK");
        }
        _ => {
            // Single run: [earth_n] [t_hours] [omega] [b_over_re]
            let earth_n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1800);
            let t_hours: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(11.0);
            let omega: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let b_over_re: f64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            let (earth, theia, soft) = build_and_relax(&gpu, &eos, earth_n, 0.0);
            run_and_measure(&gpu, &eos, &earth, &theia, soft, Ic { b_over_re, omega }, &[t_hours * 3600.0], None, true);
        }
    }
}

// docs/40 #3 — the disk measurement as a pure, order-independent reduction returning a struct (so the ensemble
// can aggregate over K runs). All category masses are reduced with `sum_oi` (sorted Kahan): the same particle
// snapshot always yields a bit-identical fraction, regardless of readback/classification order.
const M_MOON: f64 = 7.342e22;
#[derive(Clone, Copy, Default)]
struct Measure {
    earth_frac: f64, // % Earth of the orbiting disk
    disk_kg: f64,
    e_disk: f64,
    t_disk: f64,
    e_rem: f64,
    t_rem: f64,
    e_esc: f64,
    t_esc: f64,
    r_remnant: f64,
    // Moon candidate (largest self-bound clump outside Roche)
    clump_kg: f64,
    clump_earth_frac: f64,
    clump_n: usize,
    n_clumps: usize,
    n_bound: usize,
}

fn measure(body: &[Particle], n_earth: usize) -> Measure {
    let m_total = sum_oi(&mut body.iter().map(|p| p.mass as f64).collect());
    let com = com(body);
    let mut vx: Vec<f64> = body.iter().map(|p| p.vel[0] as f64 * p.mass as f64).collect();
    let mut vy: Vec<f64> = body.iter().map(|p| p.vel[1] as f64 * p.mass as f64).collect();
    let mut vz: Vec<f64> = body.iter().map(|p| p.vel[2] as f64 * p.mass as f64).collect();
    let v_com = [sum_oi(&mut vx) / m_total, sum_oi(&mut vy) / m_total, sum_oi(&mut vz) / m_total];
    // Remnant = smallest radius about the COM enclosing 85% of the mass (radius-sorted → order-independent).
    let mut radii: Vec<(f64, f64)> = body.iter().map(|p| { let d = [p.pos[0] as f64 - com[0], p.pos[1] as f64 - com[1], p.pos[2] as f64 - com[2]]; ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(), p.mass as f64) }).collect();
    radii.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut cum, mut r_remnant) = (0.0, radii.last().map_or(0.0, |x| x.0));
    let mut m_remnant = m_total;
    for &(r, m) in &radii {
        cum += m;
        if cum >= 0.85 * m_total { r_remnant = r; m_remnant = cum; break; }
    }
    let mu = G * m_remnant;
    // Classify into per-category term lists, then reduce order-independently.
    let (mut e_disk, mut t_disk, mut e_esc, mut t_esc, mut e_rem, mut t_rem) = (vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut disk_idx: Vec<usize> = Vec::new(); // the orbiting-disk particles (Moon-candidate feedstock)
    for (i, p) in body.iter().enumerate() {
        let rel_p = [p.pos[0] as f64 - com[0], p.pos[1] as f64 - com[1], p.pos[2] as f64 - com[2]];
        let rel_v = [p.vel[0] as f64 - v_com[0], p.vel[1] as f64 - v_com[1], p.vel[2] as f64 - v_com[2]];
        let is_earth = i < n_earth; // provenance also in p.prov (0=Earth); index and tag agree
        let m = p.mass as f64;
        match perigee(rel_p, rel_v, mu) {
            None => { if is_earth { e_esc.push(m) } else { t_esc.push(m) } }
            Some(pg) if pg > r_remnant => { if is_earth { e_disk.push(m) } else { t_disk.push(m) } disk_idx.push(i); }
            Some(_) => { if is_earth { e_rem.push(m) } else { t_rem.push(m) } }
        }
    }
    let (e_disk, t_disk) = (sum_oi(&mut e_disk), sum_oi(&mut t_disk));
    let disk = e_disk + t_disk;
    let clump = moon_candidate(body, &disk_idx, n_earth, r_remnant, m_remnant, com);
    Measure {
        earth_frac: if disk > 0.0 { 100.0 * e_disk / disk } else { 0.0 },
        disk_kg: disk,
        e_disk,
        t_disk,
        e_rem: sum_oi(&mut e_rem),
        t_rem: sum_oi(&mut t_rem),
        e_esc: sum_oi(&mut e_esc),
        t_esc: sum_oi(&mut t_esc),
        r_remnant,
        clump_kg: clump.0,
        clump_earth_frac: clump.1,
        clump_n: clump.2,
        n_clumps: clump.3,
        n_bound: clump.4,
    }
}

fn print_measure(m: &Measure, m_earth: f64, m_theia: f64, v_esc: f64) {
    println!("\n=== DEFORMABLE-EARTH IMPACT (M_e={:.2e}, M_t={:.2e}, v={:.0} m/s, R_remnant={:.0} km) ===", m_earth, m_theia, v_esc, m.r_remnant / 1e3);
    println!("  ORBITING DISK (perigee > remnant): Earth {:.3e} | Theia {:.3e} kg = {:.3} M_moon  → {:.0}% EARTH", m.e_disk, m.t_disk, m.disk_kg / M_MOON, m.earth_frac);
    println!("  remnant: Earth {:.3e} | Theia {:.3e} kg · escaped: Earth {:.3e} | Theia {:.3e} kg", m.e_rem, m.t_rem, m.e_esc, m.t_esc);
    println!("  → Earth material {} reach orbit", if m.e_disk > 0.0 { "DID" } else { "did NOT" });
    println!("  ACCRETION: {} disk clumps, {} self-bound", m.n_clumps, m.n_bound);
    if m.clump_kg > 0.0 {
        println!("  MOON CANDIDATE: largest bound+outside-Roche clump = {:.3e} kg = {:.3} M_moon ({} particles) → {:.0}% EARTH", m.clump_kg, m.clump_kg / M_MOON, m.clump_n, m.clump_earth_frac);
    } else {
        println!("  MOON CANDIDATE: none yet (no bound clump outside Roche — disk still dispersed; needs more time / N)");
    }
}

// The accretion operator (stage 4c.3) applied to the orbiting disk: friends-of-friends over the disk
// particles, then report the largest SELF-BOUND clump outside Roche as the Moon candidate. Mirrors the
// engine's verified `accretion.rs` (FoF + internal-KE+self-PE<0 + Roche gate); reimplemented here because
// this GPU tool is standalone (same reason sph-verify reimplements the physics).
// Returns (clump_kg, clump_earth_frac_%, clump_n, n_clumps, n_bound). clump_kg=0 ⇒ no bound clump outside Roche.
fn moon_candidate(body: &[Particle], disk_idx: &[usize], n_earth: usize, r_remnant: f64, m_remnant: f64, com: [f64; 3]) -> (f64, f64, usize, usize, usize) {
    if disk_idx.len() < 2 {
        return (0.0, 0.0, 0, 0, 0);
    }
    // Linking length = 2× the mean disk smoothing length (particles within a smoothing length touch).
    let mean_h: f64 = disk_idx.iter().map(|&i| body[i].h as f64).sum::<f64>() / disk_idx.len() as f64;
    let link = 2.0 * mean_h;
    let link2 = link * link;
    // union-find over the disk subset (O(k²); k = disk particle count, a small fraction of N)
    let k = disk_idx.len();
    let mut parent: Vec<usize> = (0..k).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r { r = p[r]; }
        let mut c = i;
        while p[c] != r { let nx = p[c]; p[c] = r; c = nx; }
        r
    }
    for a in 0..k {
        for b in (a + 1)..k {
            let (pa, pb) = (body[disk_idx[a]].pos, body[disk_idx[b]].pos);
            let d2 = ((pa[0] - pb[0]) as f64).powi(2) + ((pa[1] - pb[1]) as f64).powi(2) + ((pa[2] - pb[2]) as f64).powi(2);
            if d2 <= link2 {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb { parent[ra] = rb; }
            }
        }
    }
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for a in 0..k { let r = find(&mut parent, a); groups.entry(r).or_default().push(disk_idx[a]); }
    // Roche limit of the remnant for a ~basalt-density clump.
    let rho_rem = m_remnant / (4.0 / 3.0 * std::f64::consts::PI * r_remnant.powi(3));
    // Largest self-bound clump.
    let (mut best_m, mut best_e, mut best_n, mut best_bound_outside) = (0.0f64, 0.0f64, 0usize, false);
    let mut n_bound = 0;
    for members in groups.values() {
        if members.len() < 2 { continue; }
        let m: f64 = members.iter().map(|&i| body[i].mass as f64).sum();
        let mut cv = [0.0f64; 3];
        let mut cp = [0.0f64; 3];
        let mut vol = 0.0;
        for &i in members {
            let p = &body[i];
            for x in 0..3 { cv[x] += p.vel[x] as f64 * p.mass as f64; cp[x] += p.pos[x] as f64 * p.mass as f64; }
            vol += p.mass as f64 / p.rho as f64;
        }
        for x in 0..3 { cv[x] /= m; cp[x] /= m; }
        let clump_rho = if vol > 0.0 { m / vol } else { 2700.0 };
        let ke: f64 = members.iter().map(|&i| { let p = &body[i]; 0.5 * p.mass as f64 * (((p.vel[0] as f64 - cv[0]).powi(2)) + ((p.vel[1] as f64 - cv[1]).powi(2)) + ((p.vel[2] as f64 - cv[2]).powi(2))) }).sum();
        let mut pe = 0.0;
        for a in 0..members.len() {
            for b in (a + 1)..members.len() {
                let (ia, ib) = (members[a], members[b]);
                let d = [(body[ia].pos[0] - body[ib].pos[0]) as f64, (body[ia].pos[1] - body[ib].pos[1]) as f64, (body[ia].pos[2] - body[ib].pos[2]) as f64];
                let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0);
                pe -= G * body[ia].mass as f64 * body[ib].mass as f64 / r;
            }
        }
        let bound = ke + pe < 0.0;
        let d_roche = 2.44 * r_remnant * (rho_rem / clump_rho).cbrt();
        let dist = ((cp[0] - com[0]).powi(2) + (cp[1] - com[1]).powi(2) + (cp[2] - com[2]).powi(2)).sqrt();
        let outside = dist > d_roche;
        if bound { n_bound += 1; }
        if bound && outside && m > best_m {
            best_m = m;
            best_e = members.iter().filter(|&&i| i < n_earth).map(|&i| body[i].mass as f64).sum();
            best_n = members.len();
            best_bound_outside = true;
        }
    }
    let n_clumps = groups.values().filter(|m| m.len() >= 2).count();
    if best_bound_outside {
        (best_m, 100.0 * best_e / best_m, best_n, n_clumps, n_bound)
    } else {
        (0.0, 0.0, 0, n_clumps, n_bound)
    }
}
