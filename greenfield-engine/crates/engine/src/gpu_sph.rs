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
    use crate::hydrostatic::HydroBody;
    let (core, mantle) = (crate::eos::Tillotson::iron(), crate::eos::Tillotson::basalt());
    // Sub-Earth proto-bodies (tractable, same as tools/impact-run): Earth 5000 km, Theia 2700 km (~1/7 mass).
    let mut earth = HydroBody::new_differentiated(core, mantle, 0.5 * 5.0e6, 5.0e6, 1.0e6, n_earth);
    let mut theia = HydroBody::new_differentiated(core, mantle, 0.5 * 2.7e6, 2.7e6, 1.0e6, n_theia);
    relax_body(&mut earth, relax_steps);
    relax_body(&mut theia, relax_steps.min(relax_steps));

    let m_earth: f64 = earth.mass.iter().sum();
    let m_theia: f64 = theia.mass.iter().sum();
    let r_e = body_radius(&earth);
    let r_t = body_radius(&theia);
    let contact = r_e + r_t;
    let v_esc = 1.15 * (2.0 * crate::orbit::G * (m_earth + m_theia) / contact).sqrt();
    let d0 = 1.6 * contact;
    let b_param = 1.0 * r_e;

    // Centre Earth at the origin, at rest; Theia offset + inbound (−x) with the impact parameter in +y.
    let ec = com(&earth);
    for i in 0..earth.pos.len() {
        earth.pos[i] -= ec;
        earth.vel[i] = glam::DVec3::ZERO;
    }
    let tc = com(&theia);
    for i in 0..theia.pos.len() {
        theia.pos[i] = theia.pos[i] - tc + glam::DVec3::new(d0, b_param, 0.0);
        theia.vel[i] = glam::DVec3::new(-v_esc, 0.0, 0.0);
    }

    let mut out = Vec::with_capacity(earth.pos.len() + theia.pos.len());
    push_body(&mut out, &earth, 0);
    push_body(&mut out, &theia, 1);

    let softening = earth.softening.min(theia.softening) as f32;
    let min_h = out.iter().map(|p| p.h).fold(f32::INFINITY, f32::min);
    // Conservative FIXED dt: resolve the sound speed (~5 km/s) AND the inbound impactor. Small enough to stay
    // stable through the shock without the adaptive read-back WebGPU forbids.
    let dt = (0.05 * min_h as f64 / (5000.0 + v_esc)) as f32;
    (out, [SphEos::basalt(), SphEos::iron()], softening, dt)
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
fn push_body(out: &mut Vec<SphParticle>, b: &crate::hydrostatic::HydroBody, prov: u32) {
    for i in 0..b.pos.len() {
        let mat = if b.eos[i].rho0 > 5000.0 { MAT_IRON } else { MAT_BASALT };
        out.push(SphParticle {
            pos: [b.pos[i].x as f32, b.pos[i].y as f32, b.pos[i].z as f32],
            h: b.h[i] as f32,
            vel: [b.vel[i].x as f32, b.vel[i].y as f32, b.vel[i].z as f32],
            u: b.u[i] as f32,
            mass: b.mass[i] as f32,
            mat,
            rho: b.rho.get(i).copied().unwrap_or(b.eos[i].rho0) as f32,
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
}
