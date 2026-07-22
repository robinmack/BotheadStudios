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
    /// `cs_relax` ONLY: rigid-rotation rate (rad/s) about +z for a ROTATING-frame relaxation — the shader
    /// adds centrifugal ω²·(x,y,0) (`sph_step.wgsl:253`) so a body settles to its OBLATE equilibrium
    /// instead of a sphere. Was named `_p0` here while the shader called it `omega`: same bytes, so every
    /// size and offset check passed, but the host could not name the parameter and hardcoded 0.0 — the
    /// rotating-frame relaxation the shader implements was unreachable, and anyone reusing "padding" as
    /// scratch would have silently spun the body. 0.0 = the non-rotating relaxation used today.
    pub omega: f32,
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
/// Build the two differentiated proto-bodies UNRELAXED (Earth 5000 km, Theia 2700 km ~1/7 mass — sub-Earth,
/// tractable, same as `tools/impact-run`). [`build_far_apart`] places them far apart for GPU relaxation.
/// Back-compat: the far-apart relax set on the default (declared) initial conditions.
pub fn build_far_apart(n_earth: usize, n_theia: usize) -> (Vec<SphParticle>, f32, f32) {
    build_far_apart_from(&crate::terra::world_def::ImpactDef::default(), n_earth, n_theia)
}

/// Back-compat: assembly on the default (declared) initial conditions.
pub fn assemble_from_relaxed(particles: &[SphParticle]) -> (Vec<SphParticle>, [SphEos; 2], f32, f32) {
    assemble_from_relaxed_with(&crate::terra::world_def::ImpactDef::default(), particles)
}

/// Build the two bodies from DECLARED initial conditions (`docs/51`). The radii, softening and
/// core-resolution factor come from the world file; the LAWS (Tillotson EOS, the SPH construction) stay
/// in the engine. `ImpactDef::default()` reproduces the constants this replaced exactly.
pub fn build_impact_bodies_from(
    def: &crate::terra::world_def::ImpactDef,
    n_earth: usize,
) -> (crate::hydrostatic::HydroBody, crate::hydrostatic::HydroBody) {
    use crate::hydrostatic::HydroBody;
    const FTP: f64 = 4.0 / 3.0 * std::f64::consts::PI;
    let (core, mantle) = (crate::eos::Tillotson::iron(), crate::eos::Tillotson::basalt());
    let (t, im) = (&def.target, &def.impactor);
    // Radii and core boundaries come from the BODY DEFINITIONS, never from the scene. Resolution (the
    // particle count) is the only compute knob left — a coarse simulation of the real Earth is honest;
    // a fine simulation of a half-sized one is not.
    let (t_r, t_core) = (t.radius_m(), t.core_radius_m());
    let (im_r, im_core) = (im.radius_m(), im.core_radius_m());
    // docs/42 browser-parity: the target is variable-resolution (coarse iron core + FINE basalt mantle) —
    // the fine mantle sheds a real disk (uniform seeding did not). Solve m_fine so the count ≈ n_earth.
    let m_mantle = mantle.rho0 * FTP * (t_r.powi(3) - t_core.powi(3));
    let m_core = core.rho0 * FTP * t_core.powi(3);
    // **Resolution is the ENGINE's call, not the scene's.** Detail is spent where the physics needs it:
    // the TARGET's mantle is what shears off and forms the disk, so it is finely seeded, while its core
    // (which mostly just sits there being massive) is seeded coarse. That allocation is a statement about
    // compute, not about the world, so it lives here.
    const TARGET_CORE_LOD: f64 = 8.0; // core particles this much heavier than mantle ones
    let m_fine = (m_mantle + m_core / TARGET_CORE_LOD) / n_earth as f64;
    // Softening EMERGES from the resolution: half the fine-particle spacing, (m/ρ)^⅓. Declaring it (the
    // world file said 1.0e6 m) let a number that must track particle size sit still while the size moved.
    let softening = 0.5 * (m_fine / mantle.rho0).cbrt();
    let earth = HydroBody::new_lod(core, mantle, t_core, t_r, softening, m_fine, TARGET_CORE_LOD);
    // The impactor: uniform-differentiated at the SAME fine particle mass (equal-mass across the system).
    let m_theia = core.rho0 * FTP * im_core.powi(3)
        + mantle.rho0 * FTP * (im_r.powi(3) - im_core.powi(3));
    let theia_n = (m_theia / m_fine).round().max(50.0) as usize;
    // The impactor is seeded uniformly at the SAME fine particle mass — equal-mass particles across
    // the system, so neither body's resolution biases the shared dynamics.
    let theia = HydroBody::new_differentiated(core, mantle, im_core, im_r, softening, theia_n);
    (earth, theia)
}

/// Back-compat shim: the default (declared) initial conditions.
pub fn build_impact_bodies(n_earth: usize, _n_theia: usize) -> (crate::hydrostatic::HydroBody, crate::hydrostatic::HydroBody) {
    build_impact_bodies_from(&crate::terra::world_def::ImpactDef::default(), n_earth)
}


/// Build the two UNRELAXED bodies as one SPH particle set for GPU relaxation: Earth at the origin, Theia far
/// away (`RELAX_SEPARATION`× the contact radius), both at rest. The caller relaxes this on the GPU (`cs_relax`,
/// milliseconds — no CPU chunking), reads it back, then [`assemble_from_relaxed`] positions the collision.
/// Returns (particles, softening, relax_dt).
pub fn build_far_apart_from(def: &crate::terra::world_def::ImpactDef, n_earth: usize, n_theia: usize) -> (Vec<SphParticle>, f32, f32) {
    let (earth, theia) = build_impact_bodies_from(def, n_earth);
    let far = def.relax_separation * (def.target.radius_m() + def.impactor.radius_m());
    let ec = com(&earth);
    let tc = com(&theia);
    let mut out = Vec::with_capacity(earth.pos.len() + theia.pos.len());
    push_body(&mut out, &earth, 0, -ec, glam::DVec3::ZERO);
    push_body(&mut out, &theia, 1, -tc + glam::DVec3::new(far, 0.0, 0.0), glam::DVec3::ZERO);
    let softening = earth.softening.min(theia.softening) as f32;
    // Relaxation Courant dt (cfl·min h / max c, as the working CPU relax used). Stable at cfl 0.2 PROVIDED the
    // caller zeroes the artificial viscosity during relax (`set_av(0,0)`) — AV stiffens the transient and would
    // otherwise force a ~4× smaller dt (and 4× more steps).
    let relax_dt = earth.relax_dt(0.2).min(theia.relax_dt(0.2)) as f32;
    (out, softening, relax_dt)
}

/// After GPU relaxation of the far-apart bodies (from [`build_far_apart`]), read back the particles and place
/// them on the oblique giant-impact geometry: Earth (prov 0) recentred at the origin at rest; Theia (prov 1)
/// recentred then offset by 1.6·contact with impact parameter b≈R_e and the inbound velocity 1.15·v_esc — the
/// contact radius and v_esc computed from the ACTUAL relaxed radii. Returns (particles, [basalt, iron],
/// softening, the shock-safe impact dt).
pub fn assemble_from_relaxed_with(def: &crate::terra::world_def::ImpactDef, particles: &[SphParticle]) -> (Vec<SphParticle>, [SphEos; 2], f32, f32) {
    use glam::DVec3;
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    let subset = |prov: u32| -> (Vec<&SphParticle>, DVec3, f64, f64) {
        let ps: Vec<&SphParticle> = particles.iter().filter(|p| p.prov == prov).collect();
        let m: f64 = ps.iter().map(|p| p.mass as f64).sum();
        let c: DVec3 = ps.iter().map(|p| pos(p) * p.mass as f64).sum::<DVec3>() / m.max(1.0);
        let r = ps.iter().map(|p| (pos(p) - c).length()).fold(0.0, f64::max);
        (ps, c, m, r)
    };
    let (earth, ec, m_earth, r_e) = subset(0);
    let (theia, tc, m_theia, r_t) = subset(1);
    let contact = r_e + r_t;
    let v_esc = def.v_esc_multiple * (2.0 * crate::orbit::G * (m_earth + m_theia) / contact).sqrt();
    let (d0, b_param) = (def.start_separation * contact, def.impact_parameter * r_e);
    let emit = |out: &mut Vec<SphParticle>, ps: &[&SphParticle], off: DVec3, vel: DVec3| {
        for p in ps {
            let q = pos(p) + off;
            out.push(SphParticle { pos: [q.x as f32, q.y as f32, q.z as f32], vel: [vel.x as f32, vel.y as f32, vel.z as f32], ..**p });
        }
    };
    // Proto-Earth SPIN about +z (docs/41 spin IOU): a spinning target flings its OWN mantle into a
    // rotationally-SUSTAINED disk (it plateaus instead of re-accreting) and recovers the Earth-rich ~58% disk
    // that a non-spinning impact never reaches. Applied here at assembly — after the (spherical) relax, prompt
    // impact — so no rotational-equilibrium relaxation is needed (ω is near breakup only over a long settle).
    // v = ω ẑ × r, with r measured from Earth's recentred origin. ω≈7e-4 rad/s ≈ a 2.5 h primordial day.
    let emit_spun = |out: &mut Vec<SphParticle>, ps: &[&SphParticle], off: DVec3, omega: f64| {
        for p in ps {
            let q = pos(p) + off;
            let v = DVec3::new(-omega * q.y, omega * q.x, 0.0);
            out.push(SphParticle { pos: [q.x as f32, q.y as f32, q.z as f32], vel: [v.x as f32, v.y as f32, v.z as f32], ..**p });
        }
    };
    let mut out = Vec::with_capacity(particles.len());
    emit_spun(&mut out, &earth, -ec, def.target_spin_rad_s);
    emit(&mut out, &theia, -tc + DVec3::new(d0, b_param, 0.0), DVec3::new(-v_esc, 0.0, 0.0));
    // softening = the finest (iron) spacing = min_h/4 (h = 2·(m/ρ)^⅓, softening = ½·(m/ρ)^⅓); dt is shock-safe.
    let min_h = out.iter().map(|p| p.h).fold(f32::INFINITY, f32::min);
    let softening = 0.25 * min_h;
    // Shock-safe dt. The fixed-dt browser path (WebGPU forbids the adaptive read-back) must be small enough to
    // resolve the impact shock, or Theia interpenetrates Earth and hit-and-runs (docs/41 browser debug): a 5×
    // reduction from the original lets Earth shed into a disk. Paired with more substeps/frame to hold playback.
    let dt = (0.01 * min_h as f64 / (20_000.0 + v_esc)) as f32;
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
        let (rel_p, rel_v) = (pos(p) - com, vel(p) - v_com);
        // BOUND first, then geometry. Asking `perigee` for boundedness was safe only while it refused to
        // describe hyperbolic orbits; once it could, everything leaving counted as disk.
        if !crate::orbit::is_bound(rel_p, rel_v, mu) {
            esc += m;
            continue;
        }
        if matches!(crate::orbit::perigee(rel_p, rel_v, mu), Some(pg) if pg > r_remnant) {
            if p.prov == 0 { e_disk += m } else { t_disk += m }
        }
    }
    let disk = e_disk + t_disk;
    let earth_pct = if disk > 0.0 { 100.0 * e_disk / disk } else { 0.0 };
    // Moon candidate: the largest self-bound clump in the disk (the verified accretion operator).
    let (dp, dv, dm, dr): (Vec<DVec3>, Vec<DVec3>, Vec<f64>, Vec<f64>) = {
        let (mut p, mut v, mut m, mut r) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for pt in particles {
            let (rel_p, rel_v) = (pos(pt) - com, vel(pt) - v_com);
            let peri = crate::orbit::perigee(rel_p, rel_v, mu);
            if crate::orbit::is_bound(rel_p, rel_v, mu) && matches!(peri, Some(pg) if pg > r_remnant) {
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

/// Promote the GPU impact's orbiting disk to geologic-time moonlets (docs/35 stage 5, 2c): find the disk's
/// self-bound clumps (the `accretion` operator) and turn each into a `tides::Moonlet` orbiting the REAL Earth
/// just outside the Roche limit (~3.8 R⊕, the real Moon's formation distance), carrying the clump's actual
/// mass. The secular tidal law then migrates/merges them. This is the GPU-path replacement for the
/// `Aggregate`-based `enter_geologic_time` hand-off. Returns an empty vec if there's no bound disk yet.
/// docs/42 Phase 4: the accreting disk clumps as (Earth-relative COM position, sphere radius, mass) — so the
/// pretty render can draw growing moonlet SPHERES that resolve out of the ejecta. Same disk + friends-of-friends
/// clump detection as [`disk_moonlets`], but keeps the geometry instead of collapsing to a Moonlet {a, mass}.
pub fn moonlet_bodies(particles: &[SphParticle]) -> Vec<(glam::DVec3, f64, f64)> {
    use glam::DVec3;
    if particles.len() < 2 {
        return Vec::new();
    }
    let m_total: f64 = particles.iter().map(|p| p.mass as f64).sum();
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    let vel = |p: &SphParticle| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
    let com: DVec3 = particles.iter().map(|p| pos(p) * p.mass as f64).sum::<DVec3>() / m_total;
    let v_com: DVec3 = particles.iter().map(|p| vel(p) * p.mass as f64).sum::<DVec3>() / m_total;
    let mut radii: Vec<(f64, f64)> = particles.iter().map(|p| ((pos(p) - com).length(), p.mass as f64)).collect();
    radii.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut cum, mut r_remnant, mut m_remnant) = (0.0, radii.last().map_or(0.0, |x| x.0), m_total);
    for &(r, m) in &radii {
        cum += m;
        if cum >= 0.85 * m_total { r_remnant = r; m_remnant = cum; break; }
    }
    let mu = crate::orbit::G * m_remnant;
    let (mut dp, mut dv, mut dm, mut dr) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for pt in particles {
        if matches!(crate::orbit::perigee(pos(pt) - com, vel(pt) - v_com, mu), Some(pg) if pg > r_remnant) {
            dp.push(pos(pt)); dv.push(vel(pt)); dm.push(pt.mass as f64); dr.push(pt.rho.max(1.0) as f64);
        }
    }
    if dp.len() < 2 {
        return Vec::new();
    }
    let mean_h: f64 = particles.iter().map(|p| p.h as f64).sum::<f64>() / particles.len() as f64;
    let clumps = crate::accretion::find_clumps(&dp, &dv, &dm, &dr, 2.0 * mean_h, crate::orbit::G, 1.0e4, com, m_remnant, r_remnant);
    // Self-bound clumps of ≥3 members: the moonlets forming in the disk (COM position kept Earth-relative).
    // Only bound clumps OUTSIDE Roche are real moonlets (an inside-Roche clump is tidal debris that escapes —
    // it must render as ejecta, not a moon sphere). `accretes()` = bound + outside-Roche + ≥2 members.
    clumps
        .iter()
        .filter(|c| c.accretes() && c.members.len() >= 3)
        .map(|c| (c.com_pos, c.radius, c.mass))
        .collect()
}

/// docs/42 escape-check: the LARGEST self-bound disk clump's orbit about the remnant — distance, speed, specific
/// orbital energy (bound iff < 0), semi-major axis, mass. Returns None if there is no clump. This tracks the
/// actual proto-Moon's trajectory (is it receding / unbinding?) rather than aggregate disk mass.
pub fn largest_moonlet_orbit(particles: &[SphParticle]) -> Option<(f64, f64, f64, f64, f64, f64, f64)> {
    use glam::DVec3;
    if particles.len() < 2 {
        return None;
    }
    let m_total: f64 = particles.iter().map(|p| p.mass as f64).sum();
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    let vel = |p: &SphParticle| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
    let com: DVec3 = particles.iter().map(|p| pos(p) * p.mass as f64).sum::<DVec3>() / m_total;
    let v_com: DVec3 = particles.iter().map(|p| vel(p) * p.mass as f64).sum::<DVec3>() / m_total;
    let mut radii: Vec<(f64, f64)> = particles.iter().map(|p| ((pos(p) - com).length(), p.mass as f64)).collect();
    radii.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut cum, mut r_remnant, mut m_remnant) = (0.0, radii.last().map_or(0.0, |x| x.0), m_total);
    for &(r, m) in &radii {
        cum += m;
        if cum >= 0.85 * m_total { r_remnant = r; m_remnant = cum; break; }
    }
    let mu = crate::orbit::G * m_remnant;
    let (mut dp, mut dv, mut dm, mut dr) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for pt in particles {
        if matches!(crate::orbit::perigee(pos(pt) - com, vel(pt) - v_com, mu), Some(pg) if pg > r_remnant) {
            dp.push(pos(pt)); dv.push(vel(pt)); dm.push(pt.mass as f64); dr.push(pt.rho.max(1.0) as f64);
        }
    }
    if dp.len() < 2 {
        return None;
    }
    let mean_h: f64 = particles.iter().map(|p| p.h as f64).sum::<f64>() / particles.len() as f64;
    let clumps = crate::accretion::find_clumps(&dp, &dv, &dm, &dr, 2.0 * mean_h, crate::orbit::G, 1.0e4, com, m_remnant, r_remnant);
    // Only a bound clump OUTSIDE the Roche limit is a real Moon — an inside-Roche clump is tidal debris (it
    // forms skimming the surface and escapes; it must not be counted/rendered as the Moon).
    let biggest = clumps.iter().filter(|c| c.accretes() && c.members.len() >= 3).max_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap())?;
    // Its orbit about the remnant (COM-relative).
    let rel_p = biggest.com_pos - com;
    let rel_v = biggest.com_vel - v_com;
    let r = rel_p.length();
    let v = rel_v.length();
    let energy = 0.5 * v * v - mu / r.max(1.0); // specific orbital energy: bound iff < 0
    let a = if energy < 0.0 { -mu / (2.0 * energy) } else { f64::INFINITY };
    // Angular momentum & eccentricity — does it actually ORBIT, or plunge/escape near-radially? e→1 = radial
    // (goes straight out, barely returns); e well below 1 = a real ellipse. h = |rel_p × rel_v|.
    let h = rel_p.cross(rel_v).length();
    let ecc = (1.0 + 2.0 * energy * h * h / (mu * mu)).max(0.0).sqrt();
    let theta = rel_p.y.atan2(rel_p.x); // angle in the orbital plane (radians) — sweeps if it orbits
    Some((r, v, energy, a, biggest.mass, ecc, theta))
}

pub fn disk_moonlets(particles: &[SphParticle], earth_radius: f64) -> Vec<crate::tides::Moonlet> {
    use glam::DVec3;
    if particles.len() < 2 {
        return Vec::new();
    }
    let m_total: f64 = particles.iter().map(|p| p.mass as f64).sum();
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    let vel = |p: &SphParticle| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
    let com: DVec3 = particles.iter().map(|p| pos(p) * p.mass as f64).sum::<DVec3>() / m_total;
    let v_com: DVec3 = particles.iter().map(|p| vel(p) * p.mass as f64).sum::<DVec3>() / m_total;
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
    // Disk particles (bound, perigee above the remnant).
    let (mut dp, mut dv, mut dm, mut dr) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for pt in particles {
        if matches!(crate::orbit::perigee(pos(pt) - com, vel(pt) - v_com, mu), Some(pg) if pg > r_remnant) {
            dp.push(pos(pt));
            dv.push(vel(pt));
            dm.push(pt.mass as f64);
            dr.push(pt.rho.max(1.0) as f64);
        }
    }
    if dp.len() < 2 {
        return Vec::new();
    }
    let mean_h: f64 = particles.iter().map(|p| p.h as f64).sum::<f64>() / particles.len() as f64;
    let clumps = crate::accretion::find_clumps(&dp, &dv, &dm, &dr, 2.0 * mean_h, crate::orbit::G, 1.0e4, com, m_remnant, r_remnant);
    // Each self-bound clump → a moonlet just outside the real-Earth Roche limit (~3.8 R⊕).
    let moonlets: Vec<crate::tides::Moonlet> = clumps
        .iter()
        .filter(|c| c.accretes())
        .map(|c| crate::tides::Moonlet { a: 3.8 * earth_radius, mass: c.mass })
        .collect();
    if !moonlets.is_empty() {
        return moonlets;
    }
    // No tight self-bound clump yet, but there IS bound orbiting disk: in geologic time it accretes a Moon, so
    // promote the whole bound-disk mass to one moonlet (the secular tidal law then migrates/circularises it).
    let disk_mass: f64 = dm.iter().sum();
    if disk_mass > 0.0 {
        vec![crate::tides::Moonlet { a: 3.8 * earth_radius, mass: disk_mass }]
    } else {
        Vec::new()
    }
}

/// Total energy of a read-back particle set: (kinetic, internal, gravitational-PE) in J. The direct signal of
/// energy conservation — a giant impact should hold it to a few % (offline `impact-run`: 0.3–0.5 %); a
/// steadily rising total means the integrator is injecting energy (too-large dt at the shock) and the debris
/// will puff apart instead of orbiting. PE is O(N²) (softened), fine at the browser's N.
pub fn total_energy(particles: &[SphParticle], softening: f64) -> (f64, f64, f64) {
    let (mut ke, mut ie) = (0.0f64, 0.0f64);
    for p in particles {
        let v2 = (p.vel[0] * p.vel[0] + p.vel[1] * p.vel[1] + p.vel[2] * p.vel[2]) as f64;
        ke += 0.5 * p.mass as f64 * v2;
        ie += p.mass as f64 * p.u as f64;
    }
    let s2 = softening * softening;
    let mut pe = 0.0f64;
    for i in 0..particles.len() {
        let (pi, mi) = (particles[i].pos, particles[i].mass as f64);
        for j in (i + 1)..particles.len() {
            let pj = particles[j].pos;
            let dx = (pi[0] - pj[0]) as f64;
            let dy = (pi[1] - pj[1]) as f64;
            let dz = (pi[2] - pj[2]) as f64;
            let r = (dx * dx + dy * dy + dz * dz + s2).sqrt();
            pe -= crate::orbit::G * mi * particles[j].mass as f64 / r;
        }
    }
    (ke, ie, pe)
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
    /// THE shared container (`docs/50`): the particle buffer, capacity/count, and the two-phase async
    /// read-back. Was a private buffer plus a byte-for-byte copy of `GpuParticles`' read-back — one
    /// answer written down twice, which is how the same `Rc<Cell<bool>>` defect had to be fixed in both.
    /// Everything BELOW this line is the SPH solver, which stays specialized (docs/46 §1).
    store: crate::gpu_store::ParticleStore<SphParticle>,
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
    params: SphParams,
}

impl GpuSph {
    pub fn new(device: &wgpu::Device, capacity: u32) -> Self {
        let cap = capacity.max(1);
        let store = crate::gpu_store::ParticleStore::<SphParticle>::new(
            device, cap, wgpu::BufferUsages::VERTEX, "sph-particles");
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
                wgpu::BindGroupEntry { binding: 1, resource: store.buffer().as_entire_binding() },
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
            store, params_buf, eos_buf, acc, dudt, signal, grid_count, grid_bucket, bind,
            params: SphParams { n: 0, softening: 0.0, av_alpha: 1.0, av_beta: 2.0, cell_size: 1.0, table_mask: SPH_TABLE_SIZE - 1, bucket_k: SPH_BUCKET_K, dt: 0.0, damp: 1.0, omega: 0.0, _p1: 0.0, _p2: 0.0 },
        }
    }

    /// Upload a particle set (≤ capacity) + the two EOS materials, and set the physics params. `cell_size` is
    /// the max smoothing length (set here from the particles so the 27-cell grid scan stays exact).
    pub fn upload(&mut self, queue: &wgpu::Queue, particles: &[SphParticle], eos: &[SphEos; 2], softening: f32) {
        // Clamp-to-capacity + write-at-0 + set-count IS the shared container's `replace` (docs/50).
        self.store.replace(queue, particles);
        let n = self.store.count() as usize;
        let cell_size = particles[..n].iter().map(|p| p.h).fold(1.0f32, f32::max);
        self.params.n = n as u32;
        self.params.softening = softening;
        self.params.cell_size = cell_size;
        queue.write_buffer(&self.eos_buf, 0, bytemuck::cast_slice(eos));
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    /// Set the integration timestep (and damping — 1.0 for dynamics, <1 for relaxation) and push the uniform.
    pub fn set_dt(&mut self, queue: &wgpu::Queue, dt: f32, damp: f32) {
        self.params.dt = dt;
        self.params.damp = damp;
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    /// Set the Monaghan artificial-viscosity coefficients. Zero them during RELAXATION so the force kernel is
    /// gravity + SPH pressure only (matching the CPU relax, which has no AV) — AV stiffens the settling
    /// transient and forces a much smaller stable dt; without it the relax is stable at the normal Courant dt
    /// (≈4× fewer steps). Restore the shock-capture values (1, 2) for the dynamics.
    pub fn set_av(&mut self, queue: &wgpu::Queue, alpha: f32, beta: f32) {
        self.params.av_alpha = alpha;
        self.params.av_beta = beta;
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    pub fn count(&self) -> u32 {
        self.store.count()
    }
    /// The particle buffer — bind as an instance vertex buffer (pos = vec3 at byte offset 0) to draw the
    /// stepped particles with no read-back.
    pub fn particle_buffer(&self) -> &wgpu::Buffer {
        self.store.buffer()
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
        self.pass(enc, &self.insert, self.store.count());
        self.pass(enc, &self.density, self.store.count());
        self.pass(enc, &self.forces, self.store.count());
    }

    /// Encode `steps` damped relaxation steps (each = one force eval + `cs_relax`). Uses the current dt/damp.
    pub fn encode_relax(&self, enc: &mut wgpu::CommandEncoder, steps: u32) {
        for _ in 0..steps {
            self.force_eval(enc);
            self.pass(enc, &self.relax_k, self.store.count());
        }
    }

    /// Encode `substeps` KDK leapfrog dynamical steps (each = eval → half-kick+drift → eval → half-kick).
    pub fn encode_kdk(&self, enc: &mut wgpu::CommandEncoder, substeps: u32) {
        for _ in 0..substeps {
            self.force_eval(enc);
            self.pass(enc, &self.kick_drift, self.store.count());
            self.force_eval(enc);
            self.pass(enc, &self.kick, self.store.count());
        }
    }

    /// Phase 1 of read-back: copy the live particles into a MAP_READ staging buffer and start the async map.
    /// No-op if empty or a read-back is already in flight. WebGPU maps are non-blocking, so the result is
    /// collected a later frame via [`take_readback`](Self::take_readback).
    /// Phase 1 of the non-blocking read-back — see [`crate::gpu_store::ParticleStore::begin_readback`].
    pub fn begin_readback(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.store.begin_readback(device, queue);
    }

    /// Phase 2 — see [`crate::gpu_store::ParticleStore::take_readback`].
    pub fn take_readback(&mut self) -> Option<Vec<SphParticle>> {
        self.store.take_readback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgsl_layout::{offsets, wgsl_offsets, wgsl_typed};

    const SHADER: &str = include_str!("../../../shaders/sph_step.wgsl");

    /// These three mirrors ship, and until this module compiled natively NOTHING could check them:
    /// `gpu_sph` was `#[cfg(target_arch = "wasm32")]`, so `cargo test` never built the structs, and
    /// CLAUDE.md rule 3 records the consequence ("`gpu_sph.rs` has **no in-crate tests**"). The
    /// out-of-process `tools/sph-verify` guards the physics; it does not guard THIS layout, because it
    /// carries its own replica. A drift here is silent: the GPU reinterprets the bytes without error.
    #[test]
    fn sph_particle_matches_the_shader_field_for_field() {
        let rust = offsets!(SphParticle, pos, h, vel, u, mass, mat, rho, prov);
        assert_eq!(
            rust,
            wgsl_offsets(&wgsl_typed(SHADER, "Particle")),
            "SphParticle has drifted from sph_step.wgsl's Particle"
        );
        assert_eq!(
            std::mem::size_of::<SphParticle>(),
            48,
            "the particle stride is the array element size; every particle after the first would read \
             shifted memory"
        );
    }

    #[test]
    fn sph_eos_matches_the_shader_field_for_field() {
        let rust = offsets!(SphEos, rho0, a, b, cap_a, cap_b, e0, e_iv, e_cv, alpha, beta, _p0, _p1);
        assert_eq!(
            rust,
            wgsl_offsets(&wgsl_typed(SHADER, "Eos")),
            "SphEos has drifted from sph_step.wgsl's Eos — these are the cited Tillotson coefficients, \
             so a swap silently evaluates the EOS with the wrong material constants"
        );
    }

    /// `Params` is the uniform the whole step is driven by — and where `particle_step.wgsl`'s equivalent
    /// drift actually happened (`drag_cd` arriving as 0.0, making drag a quiet no-op).
    #[test]
    fn sph_params_matches_the_shader_field_for_field() {
        let rust = offsets!(
            SphParams, n, softening, av_alpha, av_beta, cell_size, table_mask, bucket_k, dt, damp,
            omega, _p1, _p2,
        );
        assert_eq!(
            rust,
            wgsl_offsets(&wgsl_typed(SHADER, "Params")),
            "SphParams has drifted from sph_step.wgsl's Params"
        );
        assert_eq!(
            std::mem::size_of::<SphParams>() % 16,
            0,
            "a uniform buffer's size must stay 16-byte aligned; the `_p` tail exists for this"
        );
    }

    /// The parser must survive THIS shader's real formatting, not just tidy input — otherwise a green
    /// test means "found nothing to compare" rather than "they agree". `sph_step.wgsl` packs several
    /// fields per line and wraps a comment across two lines inside `Params`, which is exactly the shape
    /// that defeats a naive line-based reader.
    #[test]
    fn the_wgsl_parser_actually_reads_the_sph_structs() {
        assert_eq!(wgsl_typed(SHADER, "Particle").len(), 8);
        assert_eq!(wgsl_typed(SHADER, "Eos").len(), 12);
        let p = wgsl_typed(SHADER, "Params");
        assert_eq!(p.len(), 12);
        // The two-line-comment case: `omega` is followed by a wrapped comment, then a shared line.
        let tail: Vec<&str> = p[p.len() - 3..].iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(tail, ["omega", "_p1", "_p2"], "the wrapped-comment tail must all be seen");
    }
}
