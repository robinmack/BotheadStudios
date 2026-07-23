//! Headless GPU verification of `shaders/sph_step.wgsl` (docs/33 stage 4) on the box's RTX 2070 via native
//! Vulkan wgpu — the engine's own wgpu is webgpu-only, so this lives in a standalone crate. It builds a
//! particle configuration, computes the SPH-EOS-gravity forces BOTH on the GPU (the WGSL kernel) and on the
//! CPU (an independent f64 reimplementation of the SAME equations as `hydrostatic.rs`), and asserts they
//! agree — so the kernel we will run at N~10^5 is trustworthy. Exit code 0 = match.

const SHADER: &str = include_str!("../../../shaders/sph_step.wgsl");

// ---- Layouts (must byte-match the WGSL structs; std430) ----
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
    _pad: f32,
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
    // The docs/39 resolution-on-demand bulk (Gauss gravity + non-injecting floor). Zeroed here ⇒ R_core=0
    // ⇒ disabled, so this harness verifies the free self-gravitating SPH unchanged. Mirrors sph_step.wgsl.
    bulk_cr: [f32; 4],
    bulk_vm: [f32; 4],
}
const TABLE_SIZE: u32 = 1 << 15; // 32768 cells
const BUCKET_K: u32 = 64; // >> ~18 particles/cell (cell-membership guard makes the grid exact regardless)

const G: f64 = 6.674e-11;
const AV_ALPHA: f64 = 1.0;
const AV_BETA: f64 = 2.0;

// Cited EOS (must match eos.rs; basalt = Benz & Asphaug 1999, iron = Wissing & Hobbs 2020 compressed branch).
fn eos_basalt() -> Eos {
    Eos { rho0: 2700.0, a: 0.5, b: 1.5, cap_a: 2.67e10, cap_b: 2.67e10, e0: 4.87e8, e_iv: 4.72e6, e_cv: 1.82e7, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
}
fn eos_iron() -> Eos {
    Eos { rho0: 7850.0, a: 0.5, b: 1.28, cap_a: 1.28e11, cap_b: 1.815e11, e0: 1.425e7, e_iv: 2.4e6, e_cv: 8.67e6, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
}

// ---- CPU f64 reference: the SAME equations as hydrostatic.rs / sph_step.wgsl ----
fn sph_w(r: f64, h: f64) -> f64 {
    let q = r / h;
    let sig = 8.0 / (std::f64::consts::PI * h * h * h);
    if q < 0.5 { sig * (1.0 - 6.0 * q * q + 6.0 * q * q * q) } else if q < 1.0 { let t = 1.0 - q; sig * 2.0 * t * t * t } else { 0.0 }
}
fn sph_dw(r: f64, h: f64) -> f64 {
    let q = r / h;
    let sig = 8.0 / (std::f64::consts::PI * h * h * h);
    if q < 0.5 { sig * (-12.0 * q + 18.0 * q * q) / h } else if q < 1.0 { let t = 1.0 - q; sig * (-6.0 * t * t) / h } else { 0.0 }
}
fn e64(e: &Eos) -> [f64; 10] {
    [e.rho0 as f64, e.a as f64, e.b as f64, e.cap_a as f64, e.cap_b as f64, e.e0 as f64, e.e_iv as f64, e.e_cv as f64, e.alpha as f64, e.beta as f64]
}
fn pressure(e: &Eos, rho: f64, u: f64) -> f64 {
    let p = e64(e);
    let (rho0, a, b, cap_a, cap_b, e0, e_iv, e_cv, alpha, beta) = (p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9]);
    let r = rho.max(1.0e-9);
    let eta = r / rho0;
    let mu = eta - 1.0;
    let omega = u / (e0 * eta * eta) + 1.0;
    let p_c = (a + b / omega) * r * u + cap_a * mu + cap_b * mu * mu;
    if eta >= 1.0 || u <= e_iv { return p_c; }
    let z = rho0 / r - 1.0;
    let p_e = a * r * u + (b * r * u / omega + cap_a * mu * (-beta * z).exp()) * (-alpha * z * z).exp();
    if u >= e_cv { return p_e; }
    ((u - e_iv) * p_e + (e_cv - u) * p_c) / (e_cv - e_iv)
}
fn sound_speed(e: &Eos, rho: f64, u: f64) -> f64 {
    let r = rho.max(1.0e-9);
    let dr = r * 1.0e-3; // match the WGSL finite-diff step
    let dp = (pressure(e, r + dr, u) - pressure(e, r - dr, u)) / (2.0 * dr);
    let du = u.abs() * 1.0e-3 + 1.0;
    let dpu = (pressure(e, r, u + du) - pressure(e, r, u - du)) / (2.0 * du);
    let pp = pressure(e, r, u);
    (dp + pp / (r * r) * dpu).max(0.0).sqrt()
}
fn cpu_forces(ps: &mut [Particle], eos: &[Eos], soft: f64) -> (Vec<[f64; 3]>, Vec<f64>) {
    let n = ps.len();
    // density
    let mut rho = vec![0.0f64; n];
    for i in 0..n {
        rho[i] = ps[i].mass as f64 * sph_w(0.0, ps[i].h as f64);
        for j in 0..n {
            if i == j { continue; }
            let d = [ps[i].pos[0] as f64 - ps[j].pos[0] as f64, ps[i].pos[1] as f64 - ps[j].pos[1] as f64, ps[i].pos[2] as f64 - ps[j].pos[2] as f64];
            let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let hij = 0.5 * (ps[i].h as f64 + ps[j].h as f64);
            if r < hij { rho[i] += ps[j].mass as f64 * sph_w(r, hij); }
        }
    }
    for i in 0..n { ps[i].rho = rho[i] as f32; }
    // forces
    let s2 = soft * soft;
    let mut acc = vec![[0.0f64; 3]; n];
    let mut dudt = vec![0.0f64; n];
    for i in 0..n {
        let ei = &eos[ps[i].mat as usize];
        let p_i = pressure(ei, rho[i], ps[i].u as f64);
        let c_i = sound_speed(ei, rho[i], ps[i].u as f64);
        for j in 0..n {
            if i == j { continue; }
            let d = [ps[j].pos[0] as f64 - ps[i].pos[0] as f64, ps[j].pos[1] as f64 - ps[i].pos[1] as f64, ps[j].pos[2] as f64 - ps[i].pos[2] as f64];
            let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let g = G * ps[j].mass as f64 / (r2 + s2).powf(1.5);
            for k in 0..3 { acc[i][k] += d[k] * g; }
            let r = r2.sqrt();
            let hij = 0.5 * (ps[i].h as f64 + ps[j].h as f64);
            if r < hij && r > 1.0e-9 {
                let ej = &eos[ps[j].mat as usize];
                let p_j = pressure(ej, rho[j], ps[j].u as f64);
                let c_j = sound_speed(ej, rho[j], ps[j].u as f64);
                let dvel = [ps[i].vel[0] as f64 - ps[j].vel[0] as f64, ps[i].vel[1] as f64 - ps[j].vel[1] as f64, ps[i].vel[2] as f64 - ps[j].vel[2] as f64];
                let dpos = [-d[0], -d[1], -d[2]];
                let vr = dvel[0] * dpos[0] + dvel[1] * dpos[1] + dvel[2] * dpos[2];
                let pi_ij = if vr < 0.0 {
                    let mu = hij * vr / (r * r + 0.01 * hij * hij);
                    let c_bar = 0.5 * (c_i + c_j);
                    let rho_bar = 0.5 * (rho[i] + rho[j]);
                    (-AV_ALPHA * c_bar * mu + AV_BETA * mu * mu) / rho_bar
                } else { 0.0 };
                let coeff = p_i / (rho[i] * rho[i]) + p_j / (rho[j] * rho[j]) + pi_ij;
                let dwdr = sph_dw(r, hij);
                let grad = [dpos[0] / r * dwdr, dpos[1] / r * dwdr, dpos[2] / r * dwdr];
                for k in 0..3 { acc[i][k] += grad[k] * (-coeff * ps[j].mass as f64); }
                dudt[i] += 0.5 * ps[j].mass as f64 * coeff * (dvel[0] * grad[0] + dvel[1] * grad[1] + dvel[2] * grad[2]);
            }
        }
    }
    (acc, dudt)
}

// ---- f64 CPU KDK reference (matches HydroBody::step EXACTLY) for the multi-step integration verify (4c.1).
// State carried in f64 (no f32 round-trip between steps) so it is a genuine higher-precision reference the
// GPU's f32 stepper is compared against. Direct O(N²) density+forces — the exact physics, no grid.
struct CpuState {
    pos: Vec<[f64; 3]>,
    vel: Vec<[f64; 3]>,
    u: Vec<f64>,
    h: Vec<f64>,
    mass: Vec<f64>,
    mat: Vec<usize>,
}
impl CpuState {
    fn from_particles(ps: &[Particle]) -> Self {
        CpuState {
            pos: ps.iter().map(|p| [p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64]).collect(),
            vel: ps.iter().map(|p| [p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64]).collect(),
            u: ps.iter().map(|p| p.u as f64).collect(),
            h: ps.iter().map(|p| p.h as f64).collect(),
            mass: ps.iter().map(|p| p.mass as f64).collect(),
            mat: ps.iter().map(|p| p.mat as usize).collect(),
        }
    }
    // Density (SPH) + accelerations (gravity + SPH-EOS pressure + Monaghan AV) + du/dt — the SAME equations
    // as cpu_forces / sph_step.wgsl, on the f64 state.
    fn forces_and_dudt(&self, eos: &[Eos], soft: f64) -> (Vec<[f64; 3]>, Vec<f64>) {
        let n = self.pos.len();
        let mut rho = vec![0.0f64; n];
        for i in 0..n {
            rho[i] = self.mass[i] * sph_w(0.0, self.h[i]);
            for j in 0..n {
                if i == j { continue; }
                let d = [self.pos[i][0] - self.pos[j][0], self.pos[i][1] - self.pos[j][1], self.pos[i][2] - self.pos[j][2]];
                let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let hij = 0.5 * (self.h[i] + self.h[j]);
                if r < hij { rho[i] += self.mass[j] * sph_w(r, hij); }
            }
        }
        let s2 = soft * soft;
        let mut acc = vec![[0.0f64; 3]; n];
        let mut dudt = vec![0.0f64; n];
        for i in 0..n {
            let ei = &eos[self.mat[i]];
            let p_i = pressure(ei, rho[i], self.u[i]);
            let c_i = sound_speed(ei, rho[i], self.u[i]);
            for j in 0..n {
                if i == j { continue; }
                let d = [self.pos[j][0] - self.pos[i][0], self.pos[j][1] - self.pos[i][1], self.pos[j][2] - self.pos[i][2]];
                let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let g = G * self.mass[j] / (r2 + s2).powf(1.5);
                for k in 0..3 { acc[i][k] += d[k] * g; }
                let r = r2.sqrt();
                let hij = 0.5 * (self.h[i] + self.h[j]);
                if r < hij && r > 1.0e-9 {
                    let ej = &eos[self.mat[j]];
                    let p_j = pressure(ej, rho[j], self.u[j]);
                    let c_j = sound_speed(ej, rho[j], self.u[j]);
                    let dvel = [self.vel[i][0] - self.vel[j][0], self.vel[i][1] - self.vel[j][1], self.vel[i][2] - self.vel[j][2]];
                    let dpos = [-d[0], -d[1], -d[2]];
                    let vr = dvel[0] * dpos[0] + dvel[1] * dpos[1] + dvel[2] * dpos[2];
                    let pi_ij = if vr < 0.0 {
                        let mu = hij * vr / (r * r + 0.01 * hij * hij);
                        let c_bar = 0.5 * (c_i + c_j);
                        let rho_bar = 0.5 * (rho[i] + rho[j]);
                        (-AV_ALPHA * c_bar * mu + AV_BETA * mu * mu) / rho_bar
                    } else { 0.0 };
                    let coeff = p_i / (rho[i] * rho[i]) + p_j / (rho[j] * rho[j]) + pi_ij;
                    let dwdr = sph_dw(r, hij);
                    let grad = [dpos[0] / r * dwdr, dpos[1] / r * dwdr, dpos[2] / r * dwdr];
                    for k in 0..3 { acc[i][k] += grad[k] * (-coeff * self.mass[j]); }
                    dudt[i] += 0.5 * self.mass[j] * coeff * (dvel[0] * grad[0] + dvel[1] * grad[1] + dvel[2] * grad[2]);
                }
            }
        }
        (acc, dudt)
    }
    // One KDK leapfrog step — byte-for-byte the operator order of HydroBody::step and cs_kick_drift/cs_kick.
    fn step(&mut self, eos: &[Eos], soft: f64, dt: f64) {
        let n = self.pos.len();
        let (a1, du1) = self.forces_and_dudt(eos, soft);
        for i in 0..n {
            for k in 0..3 { self.vel[i][k] += a1[i][k] * 0.5 * dt; }
            self.u[i] = (self.u[i] + du1[i] * 0.5 * dt).max(0.0);
            for k in 0..3 { self.pos[i][k] += self.vel[i][k] * dt; }
        }
        let (a2, du2) = self.forces_and_dudt(eos, soft);
        for i in 0..n {
            for k in 0..3 { self.vel[i][k] += a2[i][k] * 0.5 * dt; }
            self.u[i] = (self.u[i] + du2[i] * 0.5 * dt).max(0.0);
        }
    }
}

// Deterministic pseudo-random in [-1,1) (no rand crate; reproducible).
fn rnd(i: usize, salt: u64) -> f64 {
    let mut x = (i as u64).wrapping_add(salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    ((x >> 40) & 0xffff) as f64 / 32768.0 - 1.0
}

fn build_config(n: usize) -> Vec<Particle> {
    // A ~100 km cluster filled by a Fibonacci sphere (some pairs within h) with mixed materials, real ρ·V
    // masses, internal energy, and pseudo-random velocities (to exercise the artificial viscosity).
    let r0 = 1.0e5;
    let vol_per = 4.0 / 3.0 * std::f64::consts::PI * r0 * r0 * r0 / n as f64;
    let spacing = vol_per.cbrt();
    let h = (2.6 * spacing) as f32;
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..n)
        .map(|i| {
            let rr = r0 * ((i as f64 + 0.5) / n as f64).cbrt();
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let rad = (1.0 - y * y).max(0.0).sqrt();
            let th = golden * i as f64;
            let pos = [(th.cos() * rad * rr) as f32, (y * rr) as f32, (th.sin() * rad * rr) as f32];
            let mat = if rr < 0.5 * r0 { 1u32 } else { 0u32 }; // iron core, basalt mantle
            let rho0 = if mat == 1 { 7850.0 } else { 2700.0 };
            Particle {
                pos,
                h,
                vel: [(200.0 * rnd(i, 1)) as f32, (200.0 * rnd(i, 2)) as f32, (200.0 * rnd(i, 3)) as f32],
                u: 1.0e6,
                mass: (rho0 * vol_per) as f32,
                mat,
                rho: rho0 as f32,
                _pad: 0.0,
            }
        })
        .collect()
}

fn main() {
    let n = 300usize;
    let soft = 1.0e4f64;
    let eos = [eos_basalt(), eos_iron()];
    let particles = build_config(n);

    // ---- CPU reference ----
    let mut cpu_particles = particles.clone();
    let (acc_cpu, dudt_cpu) = cpu_forces(&mut cpu_particles, &eos, soft);

    // ---- GPU ----
    let (acc_gpu, dudt_gpu, gpu_particles) = run_gpu(&particles, &eos, soft);

    // ---- density check (isolates the grid) ----
    let mut d_max = 0.0f64;
    for i in 0..n {
        let rel = (gpu_particles[i].rho as f64 - cpu_particles[i].rho as f64).abs() / (cpu_particles[i].rho as f64).max(1.0);
        d_max = d_max.max(rel);
    }
    println!("density (grid): max rel error GPU vs CPU {:.2e}", d_max);

    // ---- compare ----
    let mut sum_sq = 0.0;
    let mut ref_sq = 0.0;
    let mut max_rel = 0.0f64;
    for i in 0..n {
        let mut e = 0.0;
        let mut a2 = 0.0;
        for k in 0..3 {
            let de = acc_gpu[i][k] as f64 - acc_cpu[i][k];
            e += de * de;
            a2 += acc_cpu[i][k] * acc_cpu[i][k];
        }
        sum_sq += e;
        ref_sq += a2;
        let rel = e.sqrt() / a2.sqrt().max(1e-30);
        max_rel = max_rel.max(rel);
    }
    let rms_rel = (sum_sq / ref_sq.max(1e-300)).sqrt();
    // energy rate error
    let (mut de_sq, mut du_sq) = (0.0, 0.0);
    for i in 0..n {
        let d = dudt_gpu[i] as f64 - dudt_cpu[i];
        de_sq += d * d;
        du_sq += dudt_cpu[i] * dudt_cpu[i];
    }
    let dudt_rms = (de_sq / du_sq.max(1e-300)).sqrt();

    println!("N={n}  acceleration: RMS rel error {:.2e}, max per-particle {:.2e}", rms_rel, max_rel);
    println!("        energy rate du/dt: RMS rel error {:.2e}", dudt_rms);
    // f32 GPU vs f64 CPU: expect ~1e-3–1e-2 (f32 mantissa + the sound-speed finite-diff). 3% is the bound.
    let force_ok = rms_rel < 3.0e-2 && dudt_rms < 5.0e-2 && acc_gpu.iter().all(|a| a.iter().all(|c| c.is_finite()));
    println!("{}", if force_ok { "PASS (force) — GPU sph_step.wgsl matches the CPU physics" } else { "FAIL (force) — GPU/CPU mismatch" });

    // ---- 4c.1: multi-step KDK integration verify (GPU stepper vs f64 CPU HydroBody::step) ----
    let step_ok = verify_integration(&eos, soft);

    // ---- docs/39 resolution-on-demand: the un-resolved-bulk boundary (Gauss gravity + non-injecting floor) ----
    let boundary_ok = verify_boundary(&eos, soft);
    // ---- docs/39: the PLANAR bulk (terrestrial ground) — uniform g + a flat non-injecting floor ----
    let planar_ok = verify_planar_boundary(&eos, soft);

    let ok = force_ok && step_ok && boundary_ok && planar_ok;
    println!("{}", if ok { "PASS — GPU force kernel + KDK integrator + spherical & planar bulk all match the CPU physics" } else { "FAIL" });
    std::process::exit(if ok { 0 } else { 1 });
}

// docs/39: the PLANAR bulk mode — a flat terrestrial ground (uniform gravity + a flat non-injecting floor),
// the meter-scale analogue of the planet sphere. A grain released above the plane must FALL under uniform g,
// REST on the plane (y=0, no leak-through), and NOT launch (a spring floor would fling it). Verifies the
// `bulk_cr.w < 0` branch of `bulk_gravity`/`apply_bulk_floor` and that the patch-local frame is f32-clean.
fn verify_planar_boundary(eos: &[Eos], soft: f64) -> bool {
    let g = 9.8_f64;
    let start = 100.0_f64; // released 100 m above the ground plane (y=0), at rest — meter scale, local frame
    let p = Particle { pos: [0.0, start as f32, 0.0], h: 2.0, vel: [0.0; 3], u: 1.0e5, mass: 1.0e3, mat: 0, rho: 2700.0, _pad: 0.0 };
    // planar bulk: bulk_cr = (plane point = origin, w = -1 marker), bulk_vm = (up = +Y, w = g)
    let out = run_gpu_steps(&[p], &[eos[0]], soft, 0.01, 3000, [0.0, 0.0, 0.0, -1.0], [0.0, 1.0, 0.0, g as f32]);
    let f = out[0];
    let y = f.pos[1] as f64;
    let speed = ((f.vel[0] as f64).powi(2) + (f.vel[1] as f64).powi(2) + (f.vel[2] as f64).powi(2)).sqrt();
    let fell = y < start - 1.0;
    let rests = y.abs() < 0.5 && y >= -0.1; // on the plane, not sunk through
    let no_launch = speed < 1.0; // a per-step g·dt residual, not a bounce
    let ok = fell && rests && no_launch && y.is_finite();
    println!(
        "planar: {} → y={:.3} m (plane 0), speed {:.3} m/s [fell={} rests={} no_launch={}]",
        if ok { "PASS — falls, rests on the flat floor, no launch" } else { "FAIL" },
        y, speed, fell, rests, no_launch
    );
    ok
}

// docs/39 (resolution-on-demand): a particalized cap rests on an un-resolved BULK — a Gauss gravity source
// plus a non-injecting spherical floor. This checks the boundary in ISOLATION (one particle, no cap yet):
// released at rest above R_core, it must (1) FALL under the bulk's gravity (so the Gauss term has the right
// sign/magnitude), (2) REST at R_core — never sink through (the floor holds), and (3) NOT launch — a spring
// boundary would release ½k·pen² as KE, a non-injecting constraint releases none. All three failure modes of
// the port (wrong-sign gravity, leaky floor, KE-injecting spring) are caught here, before the cap is built.
fn verify_boundary(eos: &[Eos], soft: f64) -> bool {
    let r_core = 1.0e6_f64; // 1,000 km abstract-bulk core
    let bulk_m = 5.0e22_f64; // a modest bulk mass
    let start = 1.6 * r_core; // released here, at rest, on +x
    // A single grain; its OWN self-gravity is nil (n=1), so only the bulk acts — isolating the boundary.
    let p = Particle { pos: [start as f32, 0.0, 0.0], h: 4.0e4, vel: [0.0; 3], u: 1.0e5, mass: 1.0e17, mat: 0, rho: 2700.0, _pad: 0.0 };
    let g_surf = G * bulk_m / (r_core * r_core); // ~3.3 m/s² here
    let dt = 0.2 * (r_core / g_surf).sqrt() / 20.0; // comfortably sub-Courant for the fall
    let out = run_gpu_steps(&[p], &[eos[0]], soft, dt, 6000, [0.0, 0.0, 0.0, r_core as f32], [0.0, 0.0, 0.0, bulk_m as f32]);
    let f = out[0];
    let r = ((f.pos[0] as f64).powi(2) + (f.pos[1] as f64).powi(2) + (f.pos[2] as f64).powi(2)).sqrt();
    let speed = ((f.vel[0] as f64).powi(2) + (f.vel[1] as f64).powi(2) + (f.vel[2] as f64).powi(2)).sqrt();
    let escape = (2.0 * G * bulk_m / r_core).sqrt(); // ~2.6 km/s
    let fell = r < start - r_core * 0.1; // it moved inward toward the floor
    let rests = (r - r_core).abs() < 0.05 * r_core && r >= r_core - 1.0e3; // at R_core, not sunk through
    let no_launch = speed < 0.05 * escape; // seated, not flung (a spring would launch it)
    let ok = fell && rests && no_launch && r.is_finite();
    println!(
        "boundary: {} → r={:.0} km (R_core {:.0} km), speed {:.1} m/s (esc {:.0} m/s) [fell={} rests={} no_launch={}]",
        if ok { "PASS — falls, rests on the floor, no launch" } else { "FAIL" },
        r / 1e3, r_core / 1e3, speed, escape, fell, rests, no_launch
    );
    ok
}

// Run K fixed-dt KDK steps on the GPU and on the f64 CPU reference from the SAME initial config; compare the
// final pos/vel/u. Errors accumulate over steps (f32 GPU vs f64 CPU), so the bound is looser than the
// single-eval force check — but the state must TRACK, not diverge. Fixed dt on both sides (per docs/34).
fn verify_integration(eos: &[Eos], soft: f64) -> bool {
    let n = 300usize;
    let steps = 50usize;
    let dt = 0.5f64; // safely below the ~5 s Courant limit of this ~100 km cluster (c≈3 km/s, h≈63 km)
    let particles = build_config(n);

    let gpu = run_gpu_steps(&particles, eos, soft, dt, steps, [0.0; 4], [0.0; 4]);

    let mut cpu = CpuState::from_particles(&particles);
    for _ in 0..steps { cpu.step(eos, soft, dt); }

    // RMS relative error of position (displacement-scaled), velocity, and internal energy over all particles.
    let (mut pe, mut pr, mut ve, mut vr, mut ue, mut ur) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    // Position error is scaled by the RMS DISPLACEMENT from the initial config, not absolute position (the
    // cluster sits at ~10^5 m but only moves ~10^2 m in 25 s — absolute-scaling would hide all the dynamics).
    for i in 0..n {
        for k in 0..3 {
            let dx = gpu[i].pos[k] as f64 - cpu.pos[i][k];
            let disp = cpu.pos[i][k] - particles[i].pos[k] as f64;
            pe += dx * dx;
            pr += disp * disp;
            let dv = gpu[i].vel[k] as f64 - cpu.vel[i][k];
            ve += dv * dv;
            vr += cpu.vel[i][k] * cpu.vel[i][k];
        }
        let d = gpu[i].u as f64 - cpu.u[i];
        ue += d * d;
        ur += cpu.u[i] * cpu.u[i];
    }
    let pos_rms = (pe / pr.max(1e-300)).sqrt();
    let vel_rms = (ve / vr.max(1e-300)).sqrt();
    let u_rms = (ue / ur.max(1e-300)).sqrt();
    let finite = gpu.iter().all(|p| p.pos.iter().chain(p.vel.iter()).all(|c| c.is_finite()) && p.u.is_finite());

    println!("--- integration: {steps} KDK steps, dt={dt}s (GPU f32 vs CPU f64) ---");
    println!("  pos RMS rel (displacement-scaled) {:.2e}   vel RMS rel {:.2e}   u RMS rel {:.2e}", pos_rms, vel_rms, u_rms);
    // f32 accumulates over 50 steps: ~1e-3 is honest (docs/34). Must track, not diverge.
    let ok = finite && pos_rms < 5.0e-3 && vel_rms < 5.0e-3 && u_rms < 5.0e-3;
    println!("  {}", if ok { "PASS (integration) — GPU KDK stepper tracks the CPU leapfrog" } else { "FAIL (integration) — GPU/CPU state diverged" });
    ok
}

fn run_gpu(particles: &[Particle], eos: &[Eos], soft: f64) -> (Vec<[f32; 4]>, Vec<f32>, Vec<Particle>) {
    use wgpu::util::DeviceExt;
    let n = particles.len() as u32;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no Vulkan adapter (RTX 2070 expected)");
    println!("adapter: {}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor { label: Some("sph-verify"), required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::default(), memory_hints: wgpu::MemoryHints::Performance },
        None,
    ))
    .expect("request_device");

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sph_step"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let storage = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry {
        binding: b,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("l"),
        entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            storage(1, false),
            storage(2, true),
            storage(3, false),
            storage(4, false),
            storage(5, false),
            storage(6, false),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&layout], push_constant_ranges: &[] });
    let mk = |e: &str| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: Some(&pl), module: &module, entry_point: Some(e), compilation_options: Default::default(), cache: None });
    let (p_clear, p_insert, p_density, p_forces) = (mk("cs_grid_clear"), mk("cs_grid_insert"), mk("cs_density"), mk("cs_forces"));

    let cell_size = particles.iter().map(|p| p.h).fold(0.0f32, f32::max);
    let params = Params { n, softening: soft as f32, av_alpha: AV_ALPHA as f32, av_beta: AV_BETA as f32, cell_size, table_mask: TABLE_SIZE - 1, bucket_k: BUCKET_K, dt: 0.0, damp: 0.0, _p0: 0.0, _p1: 0.0, _p2: 0.0, bulk_cr: [0.0; 4], bulk_vm: [0.0; 4] };
    let pbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("particles"), contents: bytemuck::cast_slice(particles), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC });
    let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("params"), contents: bytemuck::bytes_of(&params), usage: wgpu::BufferUsages::UNIFORM });
    let ebuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("eos"), contents: bytemuck::cast_slice(eos), usage: wgpu::BufferUsages::STORAGE });
    let abuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("acc"), size: (n as u64) * 16, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let dbuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("dudt"), size: (n as u64) * 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let gcount = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grid_count"), size: (TABLE_SIZE as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    let gbucket = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grid_bucket"), size: (TABLE_SIZE as u64) * (BUCKET_K as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: pbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: ebuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: abuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: dbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: gcount.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: gbucket.as_entire_binding() },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    // One pass per stage (memory barrier at pass boundary): clear grid → insert → density → forces.
    for (pipe, threads) in [(&p_clear, TABLE_SIZE), (&p_insert, n), (&p_density, n), (&p_forces, n)] {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(threads.div_ceil(64), 1, 1);
    }
    queue.submit(Some(enc.finish()));

    let read = |buf: &wgpu::Buffer, size: u64| -> Vec<u8> {
        let staging = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, size);
        queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let v = slice.get_mapped_range().to_vec();
        v
    };
    let acc = bytemuck::cast_slice::<u8, [f32; 4]>(&read(&abuf, (n as u64) * 16)).to_vec();
    let dudt = bytemuck::cast_slice::<u8, f32>(&read(&dbuf, (n as u64) * 4)).to_vec();
    let out_particles = bytemuck::cast_slice::<u8, Particle>(&read(&pbuf, (n as u64) * std::mem::size_of::<Particle>() as u64)).to_vec();
    (acc, dudt, out_particles)
}

// Run `steps` KDK leapfrog steps at fixed `dt` on the GPU (stage 4c.1) and read back the final particles.
// Per step: clear→insert→density→forces → cs_kick_drift → clear→insert→density→forces → cs_kick. All passes
// go into ONE command buffer — consecutive compute passes in a submission are ordered & memory-synchronized,
// so the drift of step k is visible to the density of step k+1.
fn run_gpu_steps(particles: &[Particle], eos: &[Eos], soft: f64, dt: f64, steps: usize, bulk_cr: [f32; 4], bulk_vm: [f32; 4]) -> Vec<Particle> {
    use wgpu::util::DeviceExt;
    let n = particles.len() as u32;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no Vulkan adapter (RTX 2070 expected)");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor { label: Some("sph-verify-steps"), required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::default(), memory_hints: wgpu::MemoryHints::Performance },
        None,
    ))
    .expect("request_device");

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sph_step"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let storage = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry {
        binding: b,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("l"),
        entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            storage(1, false),
            storage(2, true),
            storage(3, false),
            storage(4, false),
            storage(5, false),
            storage(6, false),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&layout], push_constant_ranges: &[] });
    let mk = |e: &str| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: Some(&pl), module: &module, entry_point: Some(e), compilation_options: Default::default(), cache: None });
    let (p_clear, p_insert, p_density, p_forces, p_kd, p_k) =
        (mk("cs_grid_clear"), mk("cs_grid_insert"), mk("cs_density"), mk("cs_forces"), mk("cs_kick_drift"), mk("cs_kick"));

    let cell_size = particles.iter().map(|p| p.h).fold(0.0f32, f32::max);
    let params = Params { n, softening: soft as f32, av_alpha: AV_ALPHA as f32, av_beta: AV_BETA as f32, cell_size, table_mask: TABLE_SIZE - 1, bucket_k: BUCKET_K, dt: dt as f32, damp: 0.0, _p0: 0.0, _p1: 0.0, _p2: 0.0, bulk_cr, bulk_vm };
    let pbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("particles"), contents: bytemuck::cast_slice(particles), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC });
    let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("params"), contents: bytemuck::bytes_of(&params), usage: wgpu::BufferUsages::UNIFORM });
    let ebuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("eos"), contents: bytemuck::cast_slice(eos), usage: wgpu::BufferUsages::STORAGE });
    let abuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("acc"), size: (n as u64) * 16, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let dbuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("dudt"), size: (n as u64) * 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let gcount = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grid_count"), size: (TABLE_SIZE as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    let gbucket = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grid_bucket"), size: (TABLE_SIZE as u64) * (BUCKET_K as u64) * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: pbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: ebuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: abuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: dbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: gcount.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: gbucket.as_entire_binding() },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let force_eval: [(&wgpu::ComputePipeline, u32); 4] = [(&p_clear, TABLE_SIZE), (&p_insert, n), (&p_density, n), (&p_forces, n)];
    let pass = |enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, threads: u32| {
        let mut p = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        p.set_pipeline(pipe);
        p.set_bind_group(0, &bind, &[]);
        p.dispatch_workgroups(threads.div_ceil(64), 1, 1);
    };
    for _ in 0..steps {
        for (pipe, threads) in force_eval { pass(&mut enc, pipe, threads); } // eval 1
        pass(&mut enc, &p_kd, n); // half-kick + drift
        for (pipe, threads) in force_eval { pass(&mut enc, pipe, threads); } // eval 2
        pass(&mut enc, &p_k, n); // final half-kick
    }
    queue.submit(Some(enc.finish()));

    let size = (n as u64) * std::mem::size_of::<Particle>() as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(&pbuf, 0, &staging, 0, size);
    queue.submit(Some(enc.finish()));
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let out = bytemuck::cast_slice::<u8, Particle>(&slice.get_mapped_range()).to_vec();
    out
}
