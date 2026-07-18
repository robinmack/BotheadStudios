//! In-browser GPU SPH stepper (docs/33 stage 4c.4) — runs `shaders/sph_step.wgsl` on the engine's shared
//! WebGPU device so the deformable-Earth giant impact (verified offline in `tools/impact-run`, stages
//! 4a–4c.2) can run in the birth scene (`OrbitDemo`). Same kernels as the offline driver; the differences are
//! all WebGPU-shaped:
//!   • **Fixed dt.** Adaptive Courant dt needs a blocking read-back of the per-particle signal-speed min each
//!     step, and WebGPU forbids blocking (`Maintain::Wait` is a no-op in the browser). In-browser we run a
//!     fixed, conservative dt (computed once on the CPU from the initial state) — stable and visible, which is
//!     what the scene needs; the converged offline number stays the job of `tools/impact-run`.
//!   • **Earth-relative f32 frame.** Planetary coordinates (~10⁶–10⁸ m) lose precision in f32, so positions
//!     are kept relative to the proto-Earth centre; the scene re-adds Earth's world position at render time.
//!   • **No per-step read-back.** A whole batch of KDK/relax substeps is encoded into ONE command buffer and
//!     submitted; the particle buffer doubles as the render vertex buffer (instanced), so the stepped
//!     positions are drawn with no CPU round-trip.
//!
//! The kernels, layouts, and physics are IDENTICAL to `tools/impact-run` (which is verified against the CPU
//! on the RTX 2070); this module is the WebGPU host for them, nothing more.


// Spatial-hash grid sizing for the browser (smaller than the offline 2^16/256 to keep buffers modest).
// grid_bucket = TABLE · BUCKET_K · 4 B = 32768 · 128 · 4 ≈ 16 MB. The cell-membership guard in the shader
// keeps the grid EXACT regardless of bucket depth (a full cell just drops far duplicates, never neighbours).
const SPH_TABLE_SIZE: u32 = 1 << 15;
const SPH_BUCKET_K: u32 = 128;

/// Mirrors the `Particle` struct in `sph_step.wgsl` (std430, 48 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SphParticle {
    pub pos: [f32; 3],
    pub h: f32,
    pub vel: [f32; 3],
    pub u: f32,
    pub mass: f32,
    pub mat: u32,
    pub rho: f32,
    pub prov: u32, // 0 = Earth, 1 = Theia
}

/// Mirrors the `Eos` struct in `sph_step.wgsl` (48 bytes). Cited Tillotson params (see `eos.rs`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SphEos {
    pub rho0: f32,
    pub a: f32,
    pub b: f32,
    pub cap_a: f32,
    pub cap_b: f32,
    pub e0: f32,
    pub e_iv: f32,
    pub e_cv: f32,
    pub alpha: f32,
    pub beta: f32,
    pub _p0: f32,
    pub _p1: f32,
}

/// Mirrors the `Params` uniform in `sph_step.wgsl` (48 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SphParams {
    pub n: u32,
    pub softening: f32,
    pub av_alpha: f32,
    pub av_beta: f32,
    pub cell_size: f32,
    pub table_mask: u32,
    pub bucket_k: u32,
    pub dt: f32,
    pub damp: f32,
    pub _p0: f32,
    pub _p1: f32,
    pub _p2: f32,
}

impl SphEos {
    pub fn basalt() -> Self {
        SphEos { rho0: 2700.0, a: 0.5, b: 1.5, cap_a: 2.67e10, cap_b: 2.67e10, e0: 4.87e8, e_iv: 4.72e6, e_cv: 1.82e7, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
    }
    pub fn iron() -> Self {
        SphEos { rho0: 7850.0, a: 0.5, b: 1.28, cap_a: 1.28e11, cap_b: 1.815e11, e0: 1.425e7, e_iv: 2.4e6, e_cv: 8.67e6, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
    }
}
pub const MAT_BASALT: u32 = 0;
pub const MAT_IRON: u32 = 1;

/// Camera uniform for `sph_render.wgsl` (96 bytes): the view-projection matrix + the Earth display origin +
/// (DISPLAY_SCALE, billboard half-size) so the instanced particle shader does the Earth-relative→clip map.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SphCam {
    pub view_proj: [[f32; 4]; 4],
    pub origin: [f32; 4],
    pub params: [f32; 4],
}

/// Build the deformable-Earth giant impact as an SPH particle set in an EARTH-RELATIVE frame (Earth COM at
/// the origin), ready to upload to [`GpuSph`]. Two differentiated bodies (iron core + basalt mantle) are
/// built and RELAXED on the CPU (the cheap one-time setup — `HydroBody`, the verified physics), then placed
/// on the canonical oblique giant-impact geometry (Theia inbound at 1.15·v_esc, impact parameter b≈R_e). The
/// per-frame dynamics then run on the GPU. Returns (particles, [basalt, iron], softening, a conservative
/// fixed dt). `n_earth`/`n_theia` are particle-count targets; `relax_steps` trades setup time for equilibrium
/// (fewer = a snappier trigger but a slightly hotter start — the offline `tools/impact-run` is the faithful
/// converged run; this is the in-browser visualization).
pub fn build_deformable_impact(n_earth: usize, n_theia: usize, relax_steps: usize) -> (Vec<SphParticle>, [SphEos; 2], f32, f32) {
    let (mut earth, mut theia) = build_impact_bodies(n_earth, n_theia);
    relax_chunk(&mut earth, relax_steps);
    relax_chunk(&mut theia, relax_steps);
    assemble_impact(&earth, &theia, true)
}

/// Build the two differentiated proto-bodies UNRELAXED (Earth 5000 km, Theia 2700 km ~1/7 mass — sub-Earth,
/// tractable, same as `tools/impact-run`). The caller relaxes them (`relax_chunk`, split across frames so the
/// wasm main thread never blocks) then places them on the collision geometry with [`assemble_impact`].
pub fn build_impact_bodies(n_earth: usize, n_theia: usize) -> (crate::hydrostatic::HydroBody, crate::hydrostatic::HydroBody) {
    use crate::hydrostatic::HydroBody;
    let (core, mantle) = (crate::eos::Tillotson::iron(), crate::eos::Tillotson::basalt());
    let earth = HydroBody::new_differentiated(core, mantle, 0.5 * 5.0e6, 5.0e6, 1.0e6, n_earth);
    let theia = HydroBody::new_differentiated(core, mantle, 0.5 * 2.7e6, 2.7e6, 1.0e6, n_theia);
    (earth, theia)
}

/// Advance a body `steps` damped-relaxation steps toward hydrostatic equilibrium (in place). Called in small
/// chunks per frame so the relax never blocks the browser main thread.
pub fn relax_chunk(b: &mut crate::hydrostatic::HydroBody, steps: usize) {
    relax_body(b, steps);
}

/// Place the two (relaxed) bodies on the oblique giant-impact geometry and return the SPH particle set (Earth
/// COM at the origin at rest; Theia offset by 1.6·contact with impact parameter b≈R_e). `infall` sets Theia's
/// inbound velocity (1.15·v_esc) — pass `false` while relaxing (bodies shown at rest, settling) and `true` to
/// launch the collision. Does NOT mutate the bodies (positions are offset only in the emitted particles).
pub fn assemble_impact(earth: &crate::hydrostatic::HydroBody, theia: &crate::hydrostatic::HydroBody, infall: bool) -> (Vec<SphParticle>, [SphEos; 2], f32, f32) {
    let m_earth: f64 = earth.mass.iter().sum();
    let m_theia: f64 = theia.mass.iter().sum();
    let (r_e, r_t) = (body_radius(earth), body_radius(theia));
    let contact = r_e + r_t;
    let v_esc = 1.15 * (2.0 * crate::orbit::G * (m_earth + m_theia) / contact).sqrt();
    let (d0, b_param) = (1.6 * contact, r_e);
    let ec = com(earth);
    let tc = com(theia);
    let mut out = Vec::with_capacity(earth.pos.len() + theia.pos.len());
    push_body(&mut out, earth, 0, -ec, glam::DVec3::ZERO);
    let theia_off = -tc + glam::DVec3::new(d0, b_param, 0.0);
    let theia_vel = if infall { glam::DVec3::new(-v_esc, 0.0, 0.0) } else { glam::DVec3::ZERO };
    push_body(&mut out, theia, 1, theia_off, theia_vel);
    let softening = earth.softening.min(theia.softening) as f32;
    let min_h = out.iter().map(|p| p.h).fold(f32::INFINITY, f32::min);
    // Conservative FIXED dt: resolve the sound speed (~5 km/s) AND the inbound impactor (WebGPU forbids the
    // adaptive read-back, so it stays fixed and small enough to hold through the shock).
    let dt = (0.05 * min_h as f64 / (5000.0 + v_esc)) as f32;
    (out, [SphEos::basalt(), SphEos::iron()], softening, dt)
}

/// Measure the orbiting disk of a read-back SPH particle set (docs/33 stage 5, mirrors
/// `tools/impact-run::measure_disk`): remnant = the 85%-mass inner body; a particle is DISK if bound with
/// perigee above the remnant surface. Split by provenance (Earth prov 0 vs Theia prov 1), and report the
/// largest self-bound clump (the Moon candidate) via the verified `accretion` operator. Returns HUD JSON.
pub fn disk_stats_json(particles: &[SphParticle]) -> String {
    use glam::DVec3;
    const M_MOON: f64 = 7.342e22;
    let n = particles.len();
    if n == 0 {
        return String::from("null");
    }
    let m_total: f64 = particles.iter().map(|p| p.mass as f64).sum();
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    let vel = |p: &SphParticle| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
    let com: DVec3 = particles.iter().map(|p| pos(p) * p.mass as f64).sum::<DVec3>() / m_total;
    let v_com: DVec3 = particles.iter().map(|p| vel(p) * p.mass as f64).sum::<DVec3>() / m_total;
    // Remnant radius = smallest radius about the COM enclosing 85% of the mass.
    let mut radii: Vec<(f64, f64)> = particles.iter().map(|p| ((pos(p) - com).length(), p.mass as f64)).collect();
    radii.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut cum, mut r_remnant, mut m_remnant) = (0.0, radii.last().map_or(0.0, |x| x.0), m_total);
    for &(r, m) in &radii {
        cum += m;
        if cum >= 0.85 * m_total {
            r_remnant = r;
            m_remnant = cum;
            break;
        }
    }
    let mu = crate::orbit::G * m_remnant;
    let (mut e_disk, mut t_disk, mut esc) = (0.0f64, 0.0f64, 0.0f64);
    for p in particles {
        let m = p.mass as f64;
        match crate::orbit::perigee(pos(p) - com, vel(p) - v_com, mu) {
            None => esc += m,
            Some(pg) if pg > r_remnant => {
                if p.prov == 0 { e_disk += m } else { t_disk += m }
            }
            Some(_) => {}
        }
    }
    let disk = e_disk + t_disk;
    let earth_pct = if disk > 0.0 { 100.0 * e_disk / disk } else { 0.0 };
    // Moon candidate: the largest self-bound clump in the disk (the verified accretion operator).
    let (dp, dv, dm, dr): (Vec<DVec3>, Vec<DVec3>, Vec<f64>, Vec<f64>) = {
        let (mut p, mut v, mut m, mut r) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for pt in particles {
            let peri = crate::orbit::perigee(pos(pt) - com, vel(pt) - v_com, mu);
            if matches!(peri, Some(pg) if pg > r_remnant) {
                p.push(pos(pt));
                v.push(vel(pt));
                m.push(pt.mass as f64);
                r.push(pt.rho.max(1.0) as f64);
            }
        }
        (p, v, m, r)
    };
    let mut biggest = 0.0f64;
    if dp.len() >= 2 {
        let mean_h: f64 = particles.iter().map(|p| p.h as f64).sum::<f64>() / n as f64;
        let clumps = crate::accretion::find_clumps(&dp, &dv, &dm, &dr, 2.0 * mean_h, crate::orbit::G, 1.0e4, com, m_remnant, r_remnant);
        biggest = clumps.iter().filter(|c| c.accretes()).map(|c| c.mass).fold(0.0, f64::max);
    }
    format!(
        "{{\"disk\":{:.3},\"earth_pct\":{:.0},\"remnant_km\":{:.0},\"escaped\":{:.3},\"moon\":{:.3}}}",
        disk / M_MOON,
        earth_pct,
        r_remnant / 1e3,
        esc / M_MOON,
        biggest / M_MOON,
    )
}

fn relax_body(b: &mut crate::hydrostatic::HydroBody, steps: usize) {
    let dt = b.relax_dt(0.2);
    for _ in 0..steps {
        b.relax_step(dt, 0.94);
    }
}
fn com(b: &crate::hydrostatic::HydroBody) -> glam::DVec3 {
    let m: f64 = b.mass.iter().sum();
    let mut c = glam::DVec3::ZERO;
    for i in 0..b.pos.len() {
        c += b.pos[i] * b.mass[i];
    }
    c / m
}
fn body_radius(b: &crate::hydrostatic::HydroBody) -> f64 {
    let c = com(b);
    b.pos.iter().map(|p| (*p - c).length()).fold(0.0, f64::max)
}
// Emit `b`'s particles translated by `offset` with a uniform bulk `vel` (the body's own relax velocities are
// internal settling motion, not shown — the collision IC is Earth-at-rest / Theia-infall).
fn push_body(out: &mut Vec<SphParticle>, b: &crate::hydrostatic::HydroBody, prov: u32, offset: glam::DVec3, vel: glam::DVec3) {
    for i in 0..b.pos.len() {
        let mat = if b.eos[i].rho0() > 5000.0 { MAT_IRON } else { MAT_BASALT };
        let p = b.pos[i] + offset;
        out.push(SphParticle {
            pos: [p.x as f32, p.y as f32, p.z as f32],
            h: b.h[i] as f32,
            vel: [vel.x as f32, vel.y as f32, vel.z as f32],
            u: b.u[i] as f32,
            mass: b.mass[i] as f32,
            mat,
            rho: b.rho.get(i).copied().unwrap_or(b.eos[i].rho0()) as f32,
            prov,
        });
    }
}

/// GPU-resident SPH particle system + the `sph_step.wgsl` pipelines. Owns the physics buffer (which is ALSO
/// the render vertex buffer — zero-copy instanced draw) and the grid/force scratch.
pub struct GpuSph {
    particles: wgpu::Buffer, // STORAGE | VERTEX | COPY_DST | COPY_SRC — pos at byte 0 is the render instance
    params_buf: wgpu::Buffer,
    eos_buf: wgpu::Buffer,
    acc: wgpu::Buffer,
    dudt: wgpu::Buffer,
    signal: wgpu::Buffer,
    grid_count: wgpu::Buffer,
    grid_bucket: wgpu::Buffer,
    bind: wgpu::BindGroup,
    clear: wgpu::ComputePipeline,
    insert: wgpu::ComputePipeline,
    density: wgpu::ComputePipeline,
    forces: wgpu::ComputePipeline,
    kick_drift: wgpu::ComputePipeline,
    kick: wgpu::ComputePipeline,
    relax_k: wgpu::ComputePipeline,
    capacity: u32,
    count: u32,
    params: SphParams,
    // Two-phase async read-back (WebGPU forbids blocking on a map, so a copy+map_async is started one frame
    // and its result collected the next — mirrors `GpuParticles::begin_readback`/`take_readback`).
    readback_staging: Option<wgpu::Buffer>,
    readback_count: u32,
    readback_ready: std::rc::Rc<std::cell::Cell<bool>>,
}

impl GpuSph {
    pub fn new(device: &wgpu::Device, capacity: u32) -> Self {
        let cap = capacity.max(1);
        let particles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sph-particles"),
            size: (cap as u64) * std::mem::size_of::<SphParticle>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sph-params"),
            size: std::mem::size_of::<SphParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let eos_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sph-eos"),
            size: (2 * std::mem::size_of::<SphEos>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let acc = device.create_buffer(&wgpu::BufferDescriptor { label: Some("sph-acc"), size: (cap as u64) * 16, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let dudt = device.create_buffer(&wgpu::BufferDescriptor { label: Some("sph-dudt"), size: (cap as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let signal = device.create_buffer(&wgpu::BufferDescriptor { label: Some("sph-signal"), size: (cap as u64) * 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let grid_count = device.create_buffer(&wgpu::BufferDescriptor { label: Some("sph-grid-count"), size: (SPH_TABLE_SIZE as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let grid_bucket = device.create_buffer(&wgpu::BufferDescriptor { label: Some("sph-grid-bucket"), size: (SPH_TABLE_SIZE as u64) * (SPH_BUCKET_K as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sph-step"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/sph_step.wgsl").into()),
        });
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sph-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                storage(1, false), // particles
                storage(2, true),  // eos
                storage(3, false), // acc
                storage(4, false), // dudt
                storage(5, false), // grid_count
                storage(6, false), // grid_bucket
                storage(7, false), // signal
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("sph-pipeline-layout"), bind_group_layouts: &[&layout], push_constant_ranges: &[] });
        let mk = |entry: &str| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(entry), layout: Some(&pipeline_layout), module: &shader, entry_point: Some(entry), compilation_options: Default::default(), cache: None });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sph-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: particles.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: eos_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: acc.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: dudt.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: grid_count.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: grid_bucket.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: signal.as_entire_binding() },
            ],
        });
        GpuSph {
            clear: mk("cs_grid_clear"), insert: mk("cs_grid_insert"), density: mk("cs_density"), forces: mk("cs_forces"),
            kick_drift: mk("cs_kick_drift"), kick: mk("cs_kick"), relax_k: mk("cs_relax"),
            particles, params_buf, eos_buf, acc, dudt, signal, grid_count, grid_bucket, bind, capacity: cap, count: 0,
            params: SphParams { n: 0, softening: 0.0, av_alpha: 1.0, av_beta: 2.0, cell_size: 1.0, table_mask: SPH_TABLE_SIZE - 1, bucket_k: SPH_BUCKET_K, dt: 0.0, damp: 1.0, _p0: 0.0, _p1: 0.0, _p2: 0.0 },
            readback_staging: None,
            readback_count: 0,
            readback_ready: std::rc::Rc::new(std::cell::Cell::new(false)),
        }
    }

    /// Upload a particle set (≤ capacity) + the two EOS materials, and set the physics params. `cell_size` is
    /// the max smoothing length (set here from the particles so the 27-cell grid scan stays exact).
    pub fn upload(&mut self, queue: &wgpu::Queue, particles: &[SphParticle], eos: &[SphEos; 2], softening: f32) {
        let n = particles.len().min(self.capacity as usize);
        self.count = n as u32;
        let cell_size = particles.iter().map(|p| p.h).fold(1.0f32, f32::max);
        self.params.n = n as u32;
        self.params.softening = softening;
        self.params.cell_size = cell_size;
        queue.write_buffer(&self.particles, 0, bytemuck::cast_slice(&particles[..n]));
        queue.write_buffer(&self.eos_buf, 0, bytemuck::cast_slice(eos));
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    /// Set the integration timestep (and damping — 1.0 for dynamics, <1 for relaxation) and push the uniform.
    pub fn set_dt(&mut self, queue: &wgpu::Queue, dt: f32, damp: f32) {
        self.params.dt = dt;
        self.params.damp = damp;
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    pub fn count(&self) -> u32 {
        self.count
    }
    /// The particle buffer — bind as an instance vertex buffer (pos = vec3 at byte offset 0) to draw the
    /// stepped particles with no read-back.
    pub fn particle_buffer(&self) -> &wgpu::Buffer {
        &self.particles
    }

    fn pass(&self, enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, threads: u32) {
        let mut p = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        p.set_pipeline(pipe);
        p.set_bind_group(0, &self.bind, &[]);
        p.dispatch_workgroups(threads.div_ceil(64), 1, 1);
    }
    /// clear → insert → density → forces (one full force evaluation).
    fn force_eval(&self, enc: &mut wgpu::CommandEncoder) {
        self.pass(enc, &self.clear, SPH_TABLE_SIZE);
        self.pass(enc, &self.insert, self.count);
        self.pass(enc, &self.density, self.count);
        self.pass(enc, &self.forces, self.count);
    }

    /// Encode `steps` damped relaxation steps (each = one force eval + `cs_relax`). Uses the current dt/damp.
    pub fn encode_relax(&self, enc: &mut wgpu::CommandEncoder, steps: u32) {
        for _ in 0..steps {
            self.force_eval(enc);
            self.pass(enc, &self.relax_k, self.count);
        }
    }

    /// Encode `substeps` KDK leapfrog dynamical steps (each = eval → half-kick+drift → eval → half-kick).
    pub fn encode_kdk(&self, enc: &mut wgpu::CommandEncoder, substeps: u32) {
        for _ in 0..substeps {
            self.force_eval(enc);
            self.pass(enc, &self.kick_drift, self.count);
            self.force_eval(enc);
            self.pass(enc, &self.kick, self.count);
        }
    }

    /// Phase 1 of read-back: copy the live particles into a MAP_READ staging buffer and start the async map.
    /// No-op if empty or a read-back is already in flight. WebGPU maps are non-blocking, so the result is
    /// collected a later frame via [`take_readback`](Self::take_readback).
    pub fn begin_readback(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.count == 0 || self.readback_staging.is_some() {
            return;
        }
        let size = self.count as u64 * std::mem::size_of::<SphParticle>() as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sph-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&self.particles, 0, &staging, 0, size);
        queue.submit(std::iter::once(enc.finish()));
        self.readback_ready.set(false);
        let flag = self.readback_ready.clone();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if res.is_ok() {
                flag.set(true);
            }
        });
        self.readback_count = self.count;
        self.readback_staging = Some(staging);
    }

    /// Phase 2: if the in-flight read-back completed, return the snapshotted particles and clear the state.
    /// `None` while pending or when nothing is in flight.
    pub fn take_readback(&mut self) -> Option<Vec<SphParticle>> {
        if !self.readback_ready.get() {
            return None;
        }
        let staging = self.readback_staging.take()?;
        let data = staging.slice(..).get_mapped_range();
        let out = bytemuck::cast_slice::<u8, SphParticle>(&data).to_vec();
        drop(data);
        staging.unmap();
        self.readback_ready.set(false);
        Some(out)
    }
}
