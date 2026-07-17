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
//! Usage: `cargo run --release -- [earth_n] [steps]`  (defaults 1800 / 4000 — the CPU 3c config, for a
//! cross-check; pass larger earth_n to converge). Verification of the kernel itself lives in tools/sph-verify.

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
    _p0: f32,
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
    fn relax(&self, particles: &[Particle], eos: &[Eos], soft: f64, cfl: f64, damp: f64, steps: usize) -> Vec<Particle> {
        let cell_size = particles.iter().map(|p| p.h).fold(0.0f32, f32::max);
        let mut params = Params { n: particles.len() as u32, softening: soft as f32, av_alpha: AV_ALPHA, av_beta: AV_BETA, cell_size, table_mask: TABLE_SIZE - 1, bucket_k: BUCKET_K, dt: 0.0, damp: damp as f32, _p0: 0.0, _p1: 0.0, _p2: 0.0 };
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

    // KDK impact with adaptive Courant dt (per-step signal read-back). Returns final particles and the total
    // physical time integrated. `probe` (optional) receives (step, dt, t) callbacks for progress.
    fn impact(&self, particles: &[Particle], eos: &[Eos], soft: f64, cfl: f64, steps: usize) -> (Vec<Particle>, f64) {
        let cell_size = particles.iter().map(|p| p.h).fold(0.0f32, f32::max);
        let mut params = Params { n: particles.len() as u32, softening: soft as f32, av_alpha: AV_ALPHA, av_beta: AV_BETA, cell_size, table_mask: TABLE_SIZE - 1, bucket_k: BUCKET_K, dt: 0.0, damp: 1.0, _p0: 0.0, _p1: 0.0, _p2: 0.0 };
        let b = self.make_buffers(particles, eos, &params);
        let mut t = 0.0f64;
        for s in 0..steps {
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
            if s % 500 == 0 || s == steps - 1 {
                self.device.poll(wgpu::Maintain::Wait);
                println!("  impact step {:>5}/{}  dt={:.3}s  t={:.0}s ({:.2} h)", s, steps, dt, t, t / 3600.0);
            }
        }
        self.device.poll(wgpu::Maintain::Wait);
        (self.read_particles(&b), t)
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let earth_n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1800);
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4000);
    let theia_n = (earth_n / 6).max(50); // ~mass ratio 6:1 ⇒ equal particle mass (as the CPU 3c test)

    let gpu = Gpu::new();
    let eos = [eos_basalt(), eos_iron()];
    // Sub-Earth scale (tractable): proto-Earth 5000 km, Theia 2700 km (~1/7 mass), both differentiated.
    let (mut earth, m_i_e) = build_differentiated(0.5 * 5.0e6, 5.0e6, 1.0e6, earth_n, 0);
    let (mut theia, m_i_t) = build_differentiated(0.5 * 2.7e6, 2.7e6, 1.0e6, theia_n, 1);
    let soft = 0.5 * (m_i_e.min(m_i_t) / 7850.0).cbrt(); // finest (iron) spacing, like hydrostatic.rs
    println!("build: Earth {} particles (m_i={:.2e} kg), Theia {} particles (m_i={:.2e} kg), soft={:.0} m", earth.len(), m_i_e, theia.len(), m_i_t, soft);

    // ---- relax both bodies to hydrostatic equilibrium (damped, on the GPU) ----
    // physical relax time ~ several sound-crossing times R/c (c≈4 km/s); step count from dt below.
    println!("relaxing Earth ({} particles)...", earth.len());
    earth = gpu.relax(&earth, &eos, soft, 0.2, 0.94, (earth_n / 3 + 1500).min(6000));
    println!("relaxing Theia ({} particles)...", theia.len());
    theia = gpu.relax(&theia, &eos, soft, 0.2, 0.94, (theia_n / 3 + 1500).min(6000));
    println!("post-relax radii: R_earth={:.0} km, R_theia={:.0} km", body_radius(&earth) / 1e3, body_radius(&theia) / 1e3);

    // ---- collision IC (oblique, ~mutual escape speed, impact parameter b≈R_e) ----
    let (m_earth, m_theia): (f64, f64) = (earth.iter().map(|p| p.mass as f64).sum(), theia.iter().map(|p| p.mass as f64).sum());
    let (r_e, r_t) = (body_radius(&earth), body_radius(&theia));
    let n_earth = earth.len();
    let contact = r_e + r_t;
    let v_esc = 1.15 * (2.0 * G * (m_earth + m_theia) / contact).sqrt();
    let d0 = 1.6 * contact;
    let b_param = 1.0 * r_e;
    let ec = com(&earth);
    for p in earth.iter_mut() {
        for k in 0..3 { p.pos[k] -= ec[k] as f32; }
        p.vel = [0.0; 3];
    }
    let tc = com(&theia);
    let offset = [d0, b_param, 0.0];
    for p in theia.iter_mut() {
        for k in 0..3 { p.pos[k] = p.pos[k] - tc[k] as f32 + offset[k] as f32; }
        p.vel = [-v_esc as f32, 0.0, 0.0];
    }
    let mut body = earth;
    body.extend(theia);
    println!("collision: M_e={:.3e} kg, M_t={:.3e} kg, v_esc={:.0} m/s, b={:.0} km, N={}", m_earth, m_theia, v_esc, b_param / 1e3, body.len());

    let with_pe = body.len() <= 40000; // O(N²) CPU PE ~seconds to ~40k; above that report IE trend only
    let (ke0, ie0, pe0) = total_energy(&body, soft, with_pe);
    println!("energy before: KE={:.3e} IE={:.3e}{}", ke0, ie0, if with_pe { format!(" PE={:.3e} TOT={:.3e}", pe0, ke0 + ie0 + pe0) } else { " (PE skipped, N>40000)".into() });

    // ---- integrate the impact ----
    let (body, t_total) = gpu.impact(&body, &eos, soft, 0.1, steps);
    println!("integrated {:.0}s ({:.2} h) of aftermath in {} steps", t_total, t_total / 3600.0, steps);
    let (ke1, ie1, pe1) = total_energy(&body, soft, with_pe);
    if with_pe {
        let (e0, e1) = (ke0 + ie0 + pe0, ke1 + ie1 + pe1);
        println!("energy after:  KE={:.3e} IE={:.3e} PE={:.3e} TOT={:.3e}  (ΔTOT/|TOT0| = {:.1}%)", ke1, ie1, pe1, e1, 100.0 * (e1 - e0).abs() / e0.abs());
    } else {
        println!("energy after:  KE={:.3e} IE={:.3e}  (IE/IE0 = {:.2}× shock heating)", ke1, ie1, ie1 / ie0);
    }

    // ---- measure the disk (perigee > remnant surface), split by provenance ----
    measure_disk(&body, n_earth, m_earth, m_theia, v_esc);
}

fn measure_disk(body: &[Particle], n_earth: usize, m_earth: f64, m_theia: f64, v_esc: f64) {
    let m_total: f64 = body.iter().map(|p| p.mass as f64).sum();
    let com = com(body);
    let mut v_com = [0.0f64; 3];
    for p in body {
        for k in 0..3 { v_com[k] += p.vel[k] as f64 * p.mass as f64; }
    }
    for k in 0..3 { v_com[k] /= m_total; }
    // Remnant = smallest radius about the COM enclosing 85% of the mass.
    let mut radii: Vec<(f64, f64)> = body.iter().map(|p| { let d = [p.pos[0] as f64 - com[0], p.pos[1] as f64 - com[1], p.pos[2] as f64 - com[2]]; ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(), p.mass as f64) }).collect();
    radii.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut cum, mut r_remnant) = (0.0, radii.last().map_or(0.0, |x| x.0));
    let mut m_remnant = m_total;
    for &(r, m) in &radii {
        cum += m;
        if cum >= 0.85 * m_total { r_remnant = r; m_remnant = cum; break; }
    }
    let mu = G * m_remnant;
    let (mut e_disk, mut t_disk, mut e_esc, mut t_esc, mut e_rem, mut t_rem) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, p) in body.iter().enumerate() {
        let rel_p = [p.pos[0] as f64 - com[0], p.pos[1] as f64 - com[1], p.pos[2] as f64 - com[2]];
        let rel_v = [p.vel[0] as f64 - v_com[0], p.vel[1] as f64 - v_com[1], p.vel[2] as f64 - v_com[2]];
        let is_earth = i < n_earth; // provenance also in p.prov (0=Earth); index and tag agree
        let m = p.mass as f64;
        match perigee(rel_p, rel_v, mu) {
            None => { if is_earth { e_esc += m } else { t_esc += m } }
            Some(pg) if pg > r_remnant => { if is_earth { e_disk += m } else { t_disk += m } }
            Some(_) => { if is_earth { e_rem += m } else { t_rem += m } }
        }
    }
    let disk = e_disk + t_disk;
    let earth_frac = if disk > 0.0 { 100.0 * e_disk / disk } else { 0.0 };
    let m_moon = 7.342e22;
    println!("\n=== DEFORMABLE-EARTH IMPACT (M_e={:.2e}, M_t={:.2e}, v={:.0} m/s, R_remnant={:.0} km) ===", m_earth, m_theia, v_esc, r_remnant / 1e3);
    println!("  ORBITING DISK (perigee > remnant): Earth {:.3e} | Theia {:.3e} kg = {:.3} M_moon  → {:.0}% EARTH", e_disk, t_disk, disk / m_moon, earth_frac);
    println!("  remnant: Earth {:.3e} | Theia {:.3e} kg · escaped: Earth {:.3e} | Theia {:.3e} kg", e_rem, t_rem, e_esc, t_esc);
    println!("  → Earth material {} reach orbit", if e_disk > 0.0 { "DID" } else { "did NOT" });
}
