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
    /// The un-resolved BULK planet a particalized CAP rests on (docs/39 — resolution-on-demand). Mirrors
    /// `sph_step.wgsl`'s two `vec4` tail fields: `bulk_cr` = xyz centre + w R_core (`<= 0` disables the
    /// bulk, the whole-body impact), `bulk_vm` = xyz velocity + w mass. Landed at offset 48/64 (both
    /// 16-aligned), size 80 (`% 16 == 0`). Set via `GpuSph::set_bulk`.
    pub bulk_cr: [f32; 4],
    pub bulk_vm: [f32; 4],
}

impl SphEos {
    /// The GPU (f32) mirror of an engine-side condensed-matter EOS (`eos::Tillotson`) — how a particalized
    /// particle's OWN material (docs/58 #4) reaches the shader. The padding tail stays zero.
    pub fn from_tillotson(t: &crate::eos::Tillotson) -> Self {
        SphEos {
            rho0: t.rho0 as f32,
            a: t.a as f32,
            b: t.b as f32,
            cap_a: t.cap_a as f32,
            cap_b: t.cap_b as f32,
            e0: t.e0 as f32,
            e_iv: t.e_iv as f32,
            e_cv: t.e_cv as f32,
            alpha: t.alpha as f32,
            beta: t.beta as f32,
            _p0: 0.0,
            _p1: 0.0,
        }
    }

    pub fn basalt() -> Self {
        SphEos { rho0: 2700.0, a: 0.5, b: 1.5, cap_a: 2.67e10, cap_b: 2.67e10, e0: 4.87e8, e_iv: 4.72e6, e_cv: 1.82e7, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
    }
    pub fn iron() -> Self {
        SphEos { rho0: 7850.0, a: 0.5, b: 1.28, cap_a: 1.28e11, cap_b: 1.815e11, e0: 1.425e7, e_iv: 2.4e6, e_cv: 8.67e6, alpha: 5.0, beta: 5.0, _p0: 0.0, _p1: 0.0 }
    }
}
pub const MAT_BASALT: u32 = 0;
pub const MAT_IRON: u32 = 1;
/// The GPU EOS table holds up to this many DISTINCT materials — the shader indexes `eos[particle.mat]`, and a
/// body's layers are rock/metal/ice, so a handful. The EOS array used to be a fixed pair (`[basalt, iron]`);
/// an N-body impact of differentiated worlds (docs/58) needs the table to grow. Generous headroom.
pub const MAX_MATERIALS: usize = 16;

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
/// **Build the relax-input SPH field for a collision of ANY NUMBER of bodies (docs/58 #7 — the ONE collision
/// engine).** Particalize each `(matter, resolution)` ([`crate::hydrostatic::HydroBody::particalize`]) and
/// place the bodies FAR APART on a line (adjacent spacing `separation`× the largest body diameter) so each
/// settles under its own gravity with the others' pull negligible (~1/FAR²). Returns (particles, the shared
/// material EOS table `SphAssembly` deduped, softening, the relaxation Courant dt). This is the generic
/// replacement for `build_far_apart_from` + `build_impact_bodies_from` (the Earth/Theia iron-basalt pair):
/// birth and the moon-drop both build through it, any bodies, any materials.
pub fn build_far_apart_n(
    bodies: &[(&crate::planet::LayeredBody, usize)],
    separation: f64,
) -> (Vec<SphParticle>, Vec<SphEos>, f32, f32) {
    let hbs: Vec<crate::hydrostatic::HydroBody> = bodies
        .iter()
        .map(|(m, res)| crate::hydrostatic::HydroBody::particalize(m, *res))
        .collect();
    let r_max = bodies.iter().map(|(m, _)| m.radius()).fold(0.0, f64::max);
    let spacing = separation * 2.0 * r_max; // adjacent bodies this far apart along +x
    let mut asm = SphAssembly::default();
    for (k, hb) in hbs.iter().enumerate() {
        asm.add_body(hb, k as u32, glam::DVec3::new(k as f64 * spacing, 0.0, 0.0), glam::DVec3::ZERO);
    }
    let softening = hbs.iter().map(|hb| hb.softening).fold(f64::INFINITY, f64::min) as f32;
    let relax_dt = hbs.iter().map(|hb| hb.relax_dt(0.2)).fold(f64::INFINITY, f64::min) as f32;
    (asm.particles, asm.eos, softening, relax_dt)
}

/// Build the relax-input SPH field for a CAP impact (docs/39 resolution-on-demand): the target's `cap`
/// (already positioned on the bulk at the impact site) as `prov 0`, plus each impactor placed FAR APART to
/// relax as a free body (`prov 1..`). The caller relaxes this WITH the bulk (`GpuSph::set_bulk`) so the cap
/// settles seated on the floor before the impact — an UN-relaxed cap dumps startup non-equilibrium into the
/// shock (the 3a lesson) and over-ejects. Returns (particles, EOS table, softening, relaxation Courant dt).
pub fn build_cap_relax(
    cap: &crate::hydrostatic::HydroBody,
    impactors: &[(&crate::planet::LayeredBody, usize)],
    separation: f64,
) -> (Vec<SphParticle>, Vec<SphEos>, f32, f32) {
    let mut asm = SphAssembly::default();
    asm.add_body(cap, 0, glam::DVec3::ZERO, glam::DVec3::ZERO); // cap at its site positions (prov 0, on the bulk)
    let hbs: Vec<crate::hydrostatic::HydroBody> = impactors
        .iter()
        .map(|(m, res)| crate::hydrostatic::HydroBody::particalize(m, *res))
        .collect();
    // Place each impactor far from the cap (which sits near the target surface) and from the previous one.
    let cap_r = cap.pos.iter().map(|p| p.length()).fold(0.0_f64, f64::max);
    let mut x = cap_r;
    for (k, hb) in hbs.iter().enumerate() {
        let r = hb.pos.iter().map(|p| p.length()).fold(0.0_f64, f64::max);
        x += separation * 2.0 * r + r;
        asm.add_body(hb, (k + 1) as u32, glam::DVec3::new(x, 0.0, 0.0), glam::DVec3::ZERO);
        x += r;
    }
    let softening = std::iter::once(cap.softening)
        .chain(hbs.iter().map(|hb| hb.softening))
        .fold(f64::INFINITY, f64::min) as f32;
    let relax_dt = std::iter::once(cap.relax_dt(0.2))
        .chain(hbs.iter().map(|hb| hb.relax_dt(0.2)))
        .fold(f64::INFINITY, f64::min) as f32;
    (asm.particles, asm.eos, softening, relax_dt)
}

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

/// Robustly find a relaxed body's bulk COM, mass and radius for provenance `prov`, surviving the few stray
/// particles a relax always leaves. This pair sets the whole collision geometry -- the radius feeds the
/// contact distance and the aim, the centre is what every radius is measured from -- so a straggler must not
/// drag it. The first cut took the FARTHEST particle (one stray ruins it); a mass-weighted MEAN is not robust
/// either (a single 0.7%-mass particle flung twenty radii out drags the centre a quarter of a radius). So
/// clip: find the bulk, re-centre on it, and measure from there. Live proof it mattered: with the naive
/// centre the scene opened at ~96,000 km and passed Earth at 39,338 km against a 9,551 km contact -- a clean
/// hit turned into a fourfold miss, and every correct downstream piece waited for an impact that never came.
pub fn body_bulk(particles: &[SphParticle], prov: u32) -> (glam::DVec3, f64, f64) {
    use glam::DVec3;
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    let ps: Vec<&SphParticle> = particles.iter().filter(|p| p.prov == prov).collect();
    let m: f64 = ps.iter().map(|p| p.mass as f64).sum();
    let com = |sel: &[&SphParticle]| -> DVec3 {
        let mm: f64 = sel.iter().map(|p| p.mass as f64).sum();
        if mm <= 0.0 { return DVec3::ZERO; }
        sel.iter().map(|p| pos(p) * p.mass as f64).sum::<DVec3>() / mm
    };
    // Radius enclosing 98% of the mass about `centre` (98 not 99: the impactor has few particles, so one
    // stray there is already ~1% of its mass -- a threshold a single particle can cross is not a threshold).
    let bulk_radius = |sel: &[&SphParticle], centre: DVec3| -> f64 {
        let mm: f64 = sel.iter().map(|p| p.mass as f64).sum();
        let mut rr: Vec<(f64, f64)> = sel.iter().map(|p| ((pos(p) - centre).length(), p.mass as f64)).collect();
        rr.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let (mut cum, mut r) = (0.0, 0.0);
        for &(rad, mass) in &rr {
            cum += mass;
            r = rad;
            if cum >= 0.98 * mm { break; }
        }
        r
    };
    // Two clipping passes: re-centre on what is inside 1.5x the bulk radius, then re-measure. Converges
    // immediately for a body with a few escapees, and is a no-op for a clean one.
    let mut c = com(&ps);
    let mut r = bulk_radius(&ps, c);
    for _ in 0..2 {
        let keep: Vec<&SphParticle> =
            ps.iter().copied().filter(|p| (pos(p) - c).length() <= 1.5 * r).collect();
        if keep.is_empty() { break; }
        c = com(&keep);
        r = bulk_radius(&keep, c);
    }
    (c, m, r)
}

/// One source body's placement in an N-body SPH collision (docs/58 #7): where its recentred matter goes, its
/// bulk velocity, and its angular velocity ω — a VECTOR (any axis, not +z). Indexed by `prov` (the source
/// body). The engine computes these from the live bodies it holds; a scene never declares them.
#[derive(Clone, Copy, Debug)]
pub struct BodyPlacement {
    pub offset: glam::DVec3,
    pub vel: glam::DVec3,
    pub spin: glam::DVec3, // angular velocity ω (rad/s); v_spin = ω × (r − com)
}

/// **Place N relaxed source bodies on a given collision geometry — the generic, N-body assembly (docs/58 #7;
/// Robin: only two is not robust).** `placements[k]` is the placement for source `prov == k`: recentre that
/// body and put it at `offset` moving at `vel` + ω×(r−com). Materials are carried by the particles' `mat`
/// (the shared table [`SphAssembly`] built once); this only repositions. Returns (particles, softening, the
/// shock-safe dt against the fastest approach). A particle whose `prov` has no placement is dropped.
pub fn assemble_from_relaxed_n(
    particles: &[SphParticle],
    placements: &[BodyPlacement],
) -> (Vec<SphParticle>, f32, f32) {
    use glam::DVec3;
    let pos = |p: &SphParticle| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
    // Each source body's robust centre (surviving the strays a relax always leaves).
    let coms: Vec<DVec3> = (0..placements.len()).map(|k| body_bulk(particles, k as u32).0).collect();
    let mut out = Vec::with_capacity(particles.len());
    for p in particles {
        let Some(pl) = placements.get(p.prov as usize) else { continue };
        let local = pos(p) - coms[p.prov as usize];
        let q = local + pl.offset;
        let v = pl.vel + pl.spin.cross(local); // ω × (r − com), full vector — any spin axis
        out.push(SphParticle {
            pos: [q.x as f32, q.y as f32, q.z as f32],
            vel: [v.x as f32, v.y as f32, v.z as f32],
            ..*p
        });
    }
    // softening = finest spacing (min_h/4). Shock-safe dt against the FASTEST approach among the bodies.
    let min_h = out.iter().map(|p| p.h).fold(f32::INFINITY, f32::min);
    let softening = 0.25 * min_h;
    let max_speed = placements.iter().map(|pl| pl.vel.length()).fold(0.0, f64::max);
    let dt = (0.01 * min_h as f64 / (20_000.0 + max_speed)) as f32;
    (out, softening, dt)
}

/// Assemble a **resolution-on-demand CAP impact** (docs/39): the relaxed IMPACTOR(s) placed on their live
/// approach + a freshly-particalized CAP of the target, sharing one EOS table. For a small impactor whose
/// target stays an abstract BULK (`GpuSph::set_bulk`) instead of being resolved whole — so a Moon on modern
/// Earth resolves a crater's cap, not all 10²⁴ kg (Law III). The impactor keeps its relaxed structure; the
/// cap is the target's OWN near-surface matter at the impact site (`HydroBody::particalize_cap`), already
/// positioned in the target-centred frame. Provenance: the cap is `prov 0` (target/Earth material), every
/// impactor particle `prov 1`, so the disk stats read the Earth-vs-impactor split. Returns (particles, the
/// MERGED EOS table, softening, shock-safe dt).
pub fn assemble_cap_impact(
    relaxed_impactors: &[SphParticle],
    placements: &[BodyPlacement],
    impactor_eos: &[SphEos],
    cap: &crate::hydrostatic::HydroBody,
) -> (Vec<SphParticle>, Vec<SphEos>, f32, f32) {
    // Place the relaxed impactor(s) on their live geometry — keeps each particle's `mat` (→ impactor_eos).
    let (mut imp, _soft, dt) = assemble_from_relaxed_n(relaxed_impactors, placements);
    // The cap: the target's own near-surface matter, already centred on the target frame (origin).
    let mut asm = SphAssembly::default();
    asm.add_body(cap, 0, glam::DVec3::ZERO, glam::DVec3::ZERO); // prov 0 = target/Earth material
    // Merge the tables: cap materials first, then the impactor's (shifted). Both were deduped within
    // themselves; a duplicate across the two is a harmless extra slot (well within MAX_MATERIALS).
    let cap_mats = asm.eos.len() as u32;
    let mut eos = asm.eos;
    for e in impactor_eos {
        if eos.len() < MAX_MATERIALS {
            eos.push(*e);
        }
    }
    for p in &mut imp {
        p.mat = (p.mat + cap_mats).min(MAX_MATERIALS as u32 - 1);
        p.prov = 1; // every impactor particle reads as the impactor, whatever source body it came from
    }
    let mut out = asm.particles;
    out.extend(imp);
    let min_h = out.iter().map(|p| p.h).fold(f32::INFINITY, f32::min);
    (out, eos, 0.25 * min_h, dt)
}

/// **Place the two relaxed bodies on a GIVEN collision geometry -- the engine's geometry-agnostic assembly
/// primitive.** In the target-relative frame: the target (prov 0) recentred at the origin and spun at
/// `target_spin` (rad/s about +z), the impactor (prov 1) recentred then placed at `impactor_offset` moving at
/// `impactor_vel` (relative to the target) and spun at `impactor_spin`. Returns (particles, [basalt, iron],
/// softening, shock-safe dt).
///
/// The caller decides WHERE and how fast, and there are exactly two honest sources -- both computed by the
/// ENGINE, never declared by a scene: the declared canonical giant impact ([`assemble_from_relaxed_with`],
/// the birth experiment, whose approach must be imposed because free-fall from rest gives the wrong one), or
/// the LIVE N-body trajectory of a body already in orbit (a de-orbiting moon, whose real approach must not be
/// overwritten by a synthesized v_esc, and whose spin is the planet's own, not proto-Earth's). A scene
/// supplies bodies and initial orbits; the engine forecasts the collision and computes this geometry from the
/// state it already holds.
pub fn assemble_from_relaxed_at(
    particles: &[SphParticle],
    target_spin: f64,
    impactor_offset: glam::DVec3,
    impactor_vel: glam::DVec3,
    impactor_spin: f64,
) -> (Vec<SphParticle>, [SphEos; 2], f32, f32) {
    use glam::DVec3;
    // The 2-body case of `assemble_from_relaxed_n`: the target (prov 0) at the frame origin spun about +z (the
    // docs/41 spin IOU — a spinning target flings its own mantle into a rotationally-sustained disk), the
    // impactor (prov 1) on the given geometry. A convenience wrapper for the scene's current 2-body callers.
    let placements = [
        BodyPlacement { offset: DVec3::ZERO, vel: DVec3::ZERO, spin: DVec3::new(0.0, 0.0, target_spin) },
        BodyPlacement { offset: impactor_offset, vel: impactor_vel, spin: DVec3::new(0.0, 0.0, impactor_spin) },
    ];
    let (out, softening, dt) = assemble_from_relaxed_n(particles, &placements);
    (out, [SphEos::basalt(), SphEos::iron()], softening, dt)
}

/// After GPU relaxation of the far-apart bodies (from [`build_far_apart`]), place them on the DECLARED
/// canonical oblique giant impact (the birth experiment): the contact radius and v_esc are computed from the
/// ACTUAL relaxed radii, then the impactor starts at `start_separation`*contact, aim `impact_parameter`*r_e,
/// inbound at `v_esc_multiple`*v_esc. This is the imposed-IC caller of [`assemble_from_relaxed_at`]; a body
/// already in orbit uses its LIVE trajectory through the same primitive instead. Returns (particles,
/// [basalt, iron], softening, the shock-safe impact dt).
pub fn assemble_from_relaxed_with(def: &crate::terra::world_def::ImpactDef, particles: &[SphParticle]) -> (Vec<SphParticle>, [SphEos; 2], f32, f32) {
    use glam::DVec3;
    let (_ec, m_earth, r_e) = body_bulk(particles, 0);
    let (_tc, m_theia, r_t) = body_bulk(particles, 1);
    let contact = r_e + r_t;
    let v_esc = def.v_esc_multiple * (2.0 * crate::orbit::G * (m_earth + m_theia) / contact).sqrt();
    let (d0, b_param) = (def.start_separation * contact, def.impact_parameter * r_e);
    assemble_from_relaxed_at(
        particles,
        def.target_spin_rad_s,
        DVec3::new(d0, b_param, 0.0),
        DVec3::new(-v_esc, 0.0, 0.0),
        0.0,
    )
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

/// **Builds the SPH particle set + the shared material EOS table for a collision of ANY NUMBER of bodies**
/// (docs/58 #4/#5/#7 — N-body, not just target+impactor; "only two is not particularly robust", Robin). Each
/// body is particalized ([`crate::hydrostatic::HydroBody::particalize`]) then appended with its own SOURCE
/// index as `prov`; materials are deduped ACROSS ALL bodies into one ≤[`MAX_MATERIALS`]-entry table the shader
/// indexes with `eos[particle.mat]`. So a three-moon impact and a two-body impact are the same path.
#[derive(Default)]
pub struct SphAssembly {
    pub particles: Vec<SphParticle>,
    pub eos: Vec<SphEos>,
}

impl SphAssembly {
    /// Append body `prov`'s particalized matter, translated by `offset` and moving at bulk `vel`. The EOS
    /// table grows only when this body introduces a material not already present.
    pub fn add_body(
        &mut self,
        body: &crate::hydrostatic::HydroBody,
        prov: u32,
        offset: glam::DVec3,
        vel: glam::DVec3,
    ) {
        for i in 0..body.pos.len() {
            let t = match &body.eos[i] {
                crate::eos::Eos::Tillotson(t) => *t,
                // A body is condensed matter; a stray gas parcel is unexpected here — fall back, don't panic.
                crate::eos::Eos::IdealGas { .. } => crate::eos::Tillotson::basalt(),
            };
            let mat = self.material_index(SphEos::from_tillotson(&t));
            let q = body.pos[i] + offset;
            self.particles.push(SphParticle {
                pos: [q.x as f32, q.y as f32, q.z as f32],
                h: body.h[i] as f32,
                vel: [vel.x as f32, vel.y as f32, vel.z as f32],
                u: body.u[i] as f32,
                mass: body.mass[i] as f32,
                mat,
                rho: body.rho.get(i).copied().unwrap_or(t.rho0) as f32,
                prov,
            });
        }
    }

    /// The index of a material in the shared table, inserting it if new. Deduped by reference density (a
    /// material's identity). Capped at `MAX_MATERIALS`; beyond that a new material folds onto the last slot —
    /// a flagged ceiling, never an overflow (a body has far fewer distinct materials than the cap).
    fn material_index(&mut self, se: SphEos) -> u32 {
        if let Some(k) = self.eos.iter().position(|e| e.rho0 == se.rho0) {
            return k as u32;
        }
        if self.eos.len() < MAX_MATERIALS {
            self.eos.push(se);
            (self.eos.len() - 1) as u32
        } else {
            (MAX_MATERIALS - 1) as u32
        }
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
            size: (MAX_MATERIALS * std::mem::size_of::<SphEos>()) as u64,
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
            params: SphParams { n: 0, softening: 0.0, av_alpha: 1.0, av_beta: 2.0, cell_size: 1.0, table_mask: SPH_TABLE_SIZE - 1, bucket_k: SPH_BUCKET_K, dt: 0.0, damp: 1.0, omega: 0.0, _p1: 0.0, _p2: 0.0, bulk_cr: [0.0; 4], bulk_vm: [0.0; 4] },
        }
    }

    /// Upload a particle set (≤ capacity) + the material EOS TABLE (≤ [`MAX_MATERIALS`]; each particle's `mat`
    /// indexes it), and set the physics params. `cell_size` is the max smoothing length (set here from the
    /// particles so the 27-cell grid scan stays exact). Was fixed at two materials — `[basalt, iron]`.
    pub fn upload(&mut self, queue: &wgpu::Queue, particles: &[SphParticle], eos: &[SphEos], softening: f32) {
        // Clamp-to-capacity + write-at-0 + set-count IS the shared container's `replace` (docs/50).
        self.store.replace(queue, particles);
        let n = self.store.count() as usize;
        let cell_size = particles[..n].iter().map(|p| p.h).fold(1.0f32, f32::max);
        self.params.n = n as u32;
        self.params.softening = softening;
        self.params.cell_size = cell_size;
        let eos = &eos[..eos.len().min(MAX_MATERIALS)]; // the table is buffer-capped; a `mat` never exceeds it
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

    /// Configure the abstract BULK a particalized cap rests on (docs/39 — resolution-on-demand): a Gauss
    /// gravity source (monopole outside `r_core`, linear interior) + a non-injecting spherical floor at
    /// `r_core`. `r_core <= 0` DISABLES it — a whole-body impact (birth) resolves every body and needs no
    /// bulk, which is the default (`bulk_cr = [0;4]`). `center`/`vel` are in the SPH's working frame; for
    /// the rigid first cut the bulk does not recoil (negligible for a Moon on Earth; docs/39 39c refinement).
    pub fn set_bulk(
        &mut self,
        queue: &wgpu::Queue,
        center: glam::DVec3,
        r_core: f64,
        vel: glam::DVec3,
        mass: f64,
    ) {
        self.params.bulk_cr = [center.x as f32, center.y as f32, center.z as f32, r_core as f32];
        self.params.bulk_vm = [vel.x as f32, vel.y as f32, vel.z as f32, mass as f32];
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    /// Configure a PLANAR bulk (docs/39 — the terrestrial, meter-scale ground): a flat floor plane through
    /// `plane_point` with up-normal `up`, and uniform gravity `g` straight down. This is the flat-ground
    /// analogue of `set_bulk`'s planet sphere — a huge planet is locally flat, and a patch-local frame keeps
    /// f32 precision where the 1 m grains are (a huge-R sphere would form ~R-magnitude f32 coords and lose
    /// ~0.5 m ULP; docs/39 open-decision #4). Encoded as `bulk_cr.w = -1` (the planar marker).
    pub fn set_bulk_planar(&mut self, queue: &wgpu::Queue, plane_point: glam::DVec3, up: glam::DVec3, g: f64) {
        let n = up.normalize_or(glam::DVec3::Y);
        self.params.bulk_cr = [plane_point.x as f32, plane_point.y as f32, plane_point.z as f32, -1.0];
        self.params.bulk_vm = [n.x as f32, n.y as f32, n.z as f32, g as f32];
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
            omega, _p1, _p2, bulk_cr, bulk_vm,
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
        assert_eq!(p.len(), 14);
        // Two wrapped multi-line comments precede fields here: `omega` (a shared line + a 2-line comment)
        // and `bulk_cr` (a 4-line comment). A comma-splitting parser must see every field past them.
        let names: Vec<&str> = p.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            &names[9..],
            ["omega", "_p1", "_p2", "bulk_cr", "bulk_vm"],
            "the wrapped-comment tail must all be seen"
        );
    }
}

#[cfg(test)]
mod impact_geometry_tests {
    use super::*;
    use glam::DVec3;

    /// **The assembled collision must actually collide — and must stay a collision when a few particles
    /// have wandered off.**
    ///
    /// The body radius used to be the distance to the FARTHEST particle, and that radius sets both the
    /// start distance (1.6·contact) and the aim (b = impact_parameter·r_target). One strayed particle
    /// therefore launched Theia too far out AND aimed it too wide. Live, the scene opened at ~96,000 km
    /// and passed Earth with a closest approach of 39,338 km against a true contact of 9,551 km: a clean
    /// oblique hit turned into a miss by four times, so no disk formed, no Moon accreted, and every
    /// correct piece of machinery downstream sat waiting for an impact that never came.
    ///
    /// The relaxed field always has a few loose particles, so this is not a hypothetical.
    #[test]
    fn the_assembled_geometry_hits_even_with_stray_particles() {
        let def = crate::terra::world_def::ImpactDef::default();
        let (earth, theia) = build_impact_bodies_from(&def, 800);
        let mut field: Vec<SphParticle> = Vec::new();
        push_body(&mut field, &earth, 0, DVec3::ZERO, DVec3::ZERO);
        push_body(&mut field, &theia, 1, DVec3::new(4.0e7, 0.0, 0.0), DVec3::ZERO);

        let closest_approach = |particles: &[SphParticle]| -> (f64, f64) {
            let (assembled, ..) = assemble_from_relaxed_with(&def, particles);
            let body = |prov: u32| {
                let ps: Vec<&SphParticle> = assembled.iter().filter(|p| p.prov == prov).collect();
                let m: f64 = ps.iter().map(|p| p.mass as f64).sum();
                let c: DVec3 = ps.iter()
                    .map(|p| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) * p.mass as f64)
                    .sum::<DVec3>() / m;
                let v: DVec3 = ps.iter()
                    .map(|p| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64) * p.mass as f64)
                    .sum::<DVec3>() / m;
                (c, v, m)
            };
            let (ec, ev, me) = body(0);
            let (tc, tv, mt) = body(1);
            let mu = crate::orbit::G * (me + mt);
            let q = crate::orbit::perigee(tc - ec, tv - ev, mu).unwrap_or(0.0);
            (q, (tc - ec).length())
        };

        // The true contact distance, from the two bodies' own definitions.
        let contact = def.target.radius_m() + def.impactor.radius_m();

        // Clean field: a hit.
        let (q_clean, d_clean) = closest_approach(&field);
        assert!(q_clean < contact, "the assembled approach must strike: q {q_clean:.3e} vs contact {contact:.3e}");
        assert!(d_clean < 6.0 * contact, "and start near the bodies, not far away (d {d_clean:.3e})");

        // Now strand ONE particle of each body twenty radii out — exactly what a relax leaves behind.
        let mut strayed = field.clone();
        for prov in [0u32, 1u32] {
            if let Some(p) = strayed.iter_mut().find(|p| p.prov == prov) {
                p.pos[0] += 20.0 * contact as f32;
            }
        }
        let (q_stray, d_stray) = closest_approach(&strayed);
        assert!(
            q_stray < contact,
            "two stray particles must not turn the impact into a miss: q {q_stray:.3e} vs contact {contact:.3e}"
        );
        // The geometry should barely notice them at all.
        assert!(
            (d_stray - d_clean).abs() < 0.1 * d_clean,
            "the start distance must be robust to strays ({d_stray:.3e} vs {d_clean:.3e})"
        );
    }

    /// **A de-orbiting Moon is a giant impact, and the SAME assembly builds it.**
    ///
    /// Robin's law (docs/23, docs/28): Theia striking proto-Earth and the Moon striking Earth are ONE
    /// mechanic at two scales — so the deformable-SPH assembly that builds the birth impact must build
    /// the moon-drop with nothing swapped but the two bodies. This pins that precondition for routing
    /// the moon-drop onto `gpu_sph` and retiring the CPU `Aggregate` (docs/46 ledger rows 1/3/10): an
    /// `ImpactDef` naming the REAL Earth and the REAL Moon (`assets/bodies/{earth,moon}.json`) builds
    /// two bodies of the right size and proportion, and the assembled geometry actually collides.
    ///
    /// The geometry MULTIPLES here are still the birth defaults (v_esc 1.15, b = R_target). The
    /// moon-drop's real approach speed and offset come from the live de-orbit trajectory the N-body
    /// integrator already holds — that is a wiring decision, not baked here. What this proves is that
    /// the assembly MACHINERY is body-agnostic.
    #[test]
    fn a_moon_drop_builds_and_strikes_through_the_same_assembly() {
        use crate::terra::world_def::{ImpactBody, ImpactDef};
        let def = ImpactDef {
            target: ImpactBody { body: "earth".into() },
            impactor: ImpactBody { body: "moon".into() },
            ..ImpactDef::default()
        };

        // The build reads the REAL bodies — the built body's outer particle tracks the definition
        // radius (a lattice spacing inside it, ~7% at this count), so this is the sharpest check that
        // "earth" and "moon" were loaded and not proto-Earth/Theia.
        let (earth, moon) = build_impact_bodies_from(&def, 1200);
        let (r_e, r_m) = (def.target.radius_m(), def.impactor.radius_m());
        assert!((0.85 * r_e..=1.02 * r_e).contains(&body_radius(&earth)), "target is Earth-sized (built {:.3e} vs def {:.3e})", body_radius(&earth), r_e);
        assert!((0.85 * r_m..=1.02 * r_m).contains(&body_radius(&moon)), "impactor is Moon-sized (built {:.3e} vs def {:.3e})", body_radius(&moon), r_m);

        // **Seeded mass is UNCOMPRESSED (EOS reference density), a flagged fidelity IOU.**
        // `build_impact_bodies_from` seeds from the Tillotson reference densities (basalt 2700, iron
        // 7850), NOT the compressed PREM densities in the body file (Earth's inner core is ~12,900).
        // Compression is meant to EMERGE during the GPU relax under the EOS pressure; the seeded state
        // is the low-density start. So the initial SPH Earth is ~3.8e24 kg ≈ 64% of the real 5.97e24.
        // This is masked in birth (proto-Earth is deliberately a coarse sub-Earth) but is real for a
        // full Earth+Moon drop, and it is pinned here so a change to the seeding trips this test rather
        // than silently shifting the planet's mass. Whether the relax recovers the compressed profile
        // at a full Earth's resolution is unverified — tracked as the fidelity question for the wiring.
        let m_earth: f64 = earth.mass.iter().sum();
        let m_moon: f64 = moon.mass.iter().sum();
        assert!((3.0e24..4.5e24).contains(&m_earth), "uncompressed seeded Earth mass (got {m_earth:.3e})");
        assert!((4.0e22..8.0e22).contains(&m_moon), "uncompressed seeded Moon mass (got {m_moon:.3e})");
        assert!((40.0..120.0).contains(&(m_earth / m_moon)), "and the real Earth:Moon proportion (got {:.0})", m_earth / m_moon);

        // Assemble as an (unrelaxed) field and confirm the geometry strikes — same helper the birth
        // geometry test uses, now driven by the Earth+Moon def.
        let mut field: Vec<SphParticle> = Vec::new();
        push_body(&mut field, &earth, 0, DVec3::ZERO, DVec3::ZERO);
        push_body(&mut field, &moon, 1, DVec3::new(4.0e7, 0.0, 0.0), DVec3::ZERO);
        let (assembled, ..) = assemble_from_relaxed_with(&def, &field);
        let body = |prov: u32| {
            let ps: Vec<&SphParticle> = assembled.iter().filter(|p| p.prov == prov).collect();
            let m: f64 = ps.iter().map(|p| p.mass as f64).sum();
            let c: DVec3 = ps.iter().map(|p| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) * p.mass as f64).sum::<DVec3>() / m;
            let v: DVec3 = ps.iter().map(|p| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64) * p.mass as f64).sum::<DVec3>() / m;
            (c, v, m)
        };
        let (ec, ev, me) = body(0);
        let (mc, mv, mm) = body(1);
        let contact = def.target.radius_m() + def.impactor.radius_m();
        let q = crate::orbit::perigee(mc - ec, mv - ev, crate::orbit::G * (me + mm)).unwrap_or(0.0);
        assert!(q < contact, "the moon-drop assembles into a real strike: perigee {q:.3e} vs contact {contact:.3e}");

        // **WHY this belongs on the deformable-SPH path and not surface cratering.** The reduced-mass
        // impact energy dwarfs the Moon's own gravitational binding, so the impactor cannot survive as
        // a body — it must resolve into matter (docs/46 §1, the ResolveBodies regime), exactly like
        // Theia. A crater-into-rigid-Earth treatment (`ResolveMatter`) would be the wrong physics at
        // this scale. (Uniform-sphere binding 3GM²/5R — an order-of-magnitude estimate, which is all
        // the argument needs.)
        let v_rel = (mv - ev).length();
        let e_impact = 0.5 * (me * mm / (me + mm)) * v_rel * v_rel;
        let e_bind = 0.6 * crate::orbit::G * mm * mm / body_radius(&moon);
        assert!(e_impact > 10.0 * e_bind, "a moon-drop is a giant impact, not a crater: E {e_impact:.2e} J vs Moon binding {e_bind:.2e} J");
    }

    /// **The engine can place a moon-drop on its LIVE trajectory — settling the geometry fork (Robin).** A
    /// body already in orbit hands the SPH its real (offset, relative velocity), computed by the ENGINE from
    /// the state it holds — never a scene-declared v_esc. `assemble_from_relaxed_at` must honour that geometry
    /// exactly: it is the seam the moon-drop uses instead of birth's imposed canonical approach.
    #[test]
    fn assemble_at_honours_a_given_live_geometry() {
        use crate::terra::world_def::{ImpactBody, ImpactDef};
        let def = ImpactDef {
            target: ImpactBody { body: "earth".into() },
            impactor: ImpactBody { body: "moon".into() },
            ..ImpactDef::default()
        };
        let (earth, moon) = build_impact_bodies_from(&def, 800);
        let mut field: Vec<SphParticle> = Vec::new();
        push_body(&mut field, &earth, 0, DVec3::ZERO, DVec3::ZERO);
        push_body(&mut field, &moon, 1, DVec3::new(4.0e7, 0.0, 0.0), DVec3::ZERO);

        // A live approach the N-body integrator might hand over — NOT the canonical b / d0 / v_esc.
        let offset = DVec3::new(1.5e7, 3.0e6, 0.0);
        let rel_vel = DVec3::new(-9000.0, 500.0, 0.0);
        let (out, ..) = assemble_from_relaxed_at(&field, 0.0, offset, rel_vel, 0.0);

        let com_vel = |prov: u32| -> (DVec3, DVec3) {
            let ps: Vec<&SphParticle> = out.iter().filter(|p| p.prov == prov).collect();
            let m: f64 = ps.iter().map(|p| p.mass as f64).sum();
            let c: DVec3 = ps.iter().map(|p| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) * p.mass as f64).sum::<DVec3>() / m;
            let v: DVec3 = ps.iter().map(|p| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64) * p.mass as f64).sum::<DVec3>() / m;
            (c, v)
        };
        let (ec, ev) = com_vel(0);
        let (mc, mv) = com_vel(1);
        let r_e = def.target.radius_m();
        // Target sits at the frame origin, at rest (spin 0 ⇒ zero COM velocity).
        assert!(ec.length() < 0.02 * r_e, "target recentred to the origin (got {:.3e})", ec.length());
        assert!(ev.length() < 1.0, "target at rest in the frame (got {:.3e} m/s)", ev.length());
        // Impactor carries EXACTLY the requested live offset and relative velocity — no synthesized approach.
        assert!((mc - ec - offset).length() < 0.02 * r_e, "impactor on the live offset ({:.3e} off)", (mc - ec - offset).length());
        assert!((mv - ev - rel_vel).length() < 1.0, "impactor carries the live relative velocity");
    }

    /// **An N-body, N-material assembly — one path for any number of bodies (docs/58 #4/#5/#7, Robin's
    /// N-body note).** Particalize Earth + Moon into ONE SPH field: the EOS table holds each distinct
    /// material once (deduped across both bodies), every particle's `mat` indexes it, and the two bodies keep
    /// distinct source provenances. Proves the field is no longer the fixed `[basalt, iron]` pair of two.
    #[test]
    fn sph_assembly_builds_an_n_material_field_across_bodies() {
        let earth = crate::hydrostatic::HydroBody::particalize(&crate::planet::earth(), 1500);
        let moon = crate::hydrostatic::HydroBody::particalize(&crate::planet::moon(), 300);
        let mut asm = SphAssembly::default();
        asm.add_body(&earth, 0, DVec3::ZERO, DVec3::ZERO);
        asm.add_body(&moon, 1, DVec3::new(4.0e7, 0.0, 0.0), DVec3::ZERO);

        // Distinct materials across BOTH bodies (iron / peridotite / basalt) → deduped to ≥3, > the old 2.
        assert!(
            (3..=MAX_MATERIALS).contains(&asm.eos.len()),
            "an N-material EOS table, deduped across bodies (got {})",
            asm.eos.len()
        );
        // Every particle's `mat` is a valid index into that table.
        assert!(asm.particles.iter().all(|p| (p.mat as usize) < asm.eos.len()), "every mat is a valid index");
        // Both source bodies are present with distinct provenance.
        assert!(
            asm.particles.iter().any(|p| p.prov == 0) && asm.particles.iter().any(|p| p.prov == 1),
            "both bodies present with distinct source provenance"
        );
        // The iron-core particles index the iron EOS entry (ρ₀ 7850), not a defaulted material.
        let iron = asm.eos.iter().position(|e| (e.rho0 - 7850.0).abs() < 50.0).expect("iron is in the table");
        assert!(asm.particles.iter().any(|p| p.mat as usize == iron), "core particles index the iron EOS");
        // Mass is conserved to the two real bodies.
        let m: f64 = asm.particles.iter().map(|p| p.mass as f64).sum();
        let real = crate::planet::earth().total_mass() + crate::planet::moon().total_mass();
        assert!((m - real).abs() / real < 0.03, "N-body mass conserved (got {m:.3e} vs {real:.3e})");
    }

    /// **`assemble_from_relaxed_n` places EVERY body at its own live geometry, and spins about ANY axis
    /// (docs/58 #7 — N-body + vector spin).** Three bodies each land at their placement with their own
    /// velocity, and an off-axis (x) spin gives the target a `vz` a +z-only path could never produce.
    #[test]
    fn assemble_n_places_every_body_and_spins_about_any_axis() {
        let earth = crate::hydrostatic::HydroBody::particalize(&crate::planet::earth(), 1200);
        let m = crate::hydrostatic::HydroBody::particalize(&crate::planet::moon(), 250);
        let mut asm = SphAssembly::default();
        asm.add_body(&earth, 0, DVec3::ZERO, DVec3::ZERO);
        asm.add_body(&m, 1, DVec3::new(4.0e7, 0.0, 0.0), DVec3::ZERO);
        asm.add_body(&m, 2, DVec3::new(-4.0e7, 0.0, 0.0), DVec3::ZERO);

        let placements = [
            // Target at the origin, spun about the X axis — an off-axis spin a +z-only path cannot express.
            BodyPlacement { offset: DVec3::ZERO, vel: DVec3::ZERO, spin: DVec3::new(3.0e-4, 0.0, 0.0) },
            BodyPlacement { offset: DVec3::new(1.5e7, 2.0e6, 0.0), vel: DVec3::new(-8000.0, 0.0, 0.0), spin: DVec3::ZERO },
            BodyPlacement { offset: DVec3::new(0.0, 1.6e7, 0.0), vel: DVec3::new(0.0, -7000.0, 0.0), spin: DVec3::ZERO },
        ];
        let (out, _, _) = assemble_from_relaxed_n(&asm.particles, &placements);

        let com_vel = |prov: u32| -> (DVec3, DVec3) {
            let ps: Vec<&SphParticle> = out.iter().filter(|p| p.prov == prov).collect();
            let mm: f64 = ps.iter().map(|p| p.mass as f64).sum();
            let c = ps.iter().map(|p| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) * p.mass as f64).sum::<DVec3>() / mm;
            let v = ps.iter().map(|p| DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64) * p.mass as f64).sum::<DVec3>() / mm;
            (c, v)
        };
        let (c0, v0) = com_vel(0);
        let (c1, v1) = com_vel(1);
        let (c2, v2) = com_vel(2);
        let r_e = crate::planet::earth().radius();
        assert!(c0.length() < 0.02 * r_e, "target recentred to the origin");
        assert!((c1 - c0 - placements[1].offset).length() < 0.02 * r_e, "moon 1 at its own offset");
        assert!((c2 - c0 - placements[2].offset).length() < 0.02 * r_e, "moon 2 at its own offset");
        assert!((v1 - v0 - placements[1].vel).length() < 1.0, "moon 1 carries its own velocity");
        assert!((v2 - v0 - placements[2].vel).length() < 1.0, "moon 2 carries its own velocity");
        // An X-axis spin gives the target particles a Z velocity — impossible for a +z-only spin.
        let max_vz = out.iter().filter(|p| p.prov == 0).map(|p| p.vel[2].abs()).fold(0.0f32, f32::max);
        assert!(max_vz > 1.0, "an off-axis spin produces vz — spin is a VECTOR, not +z (max vz {max_vz})");
        assert!(v0.length() < 1.0, "spin is internal — no net bulk velocity on the target");
    }

    /// **`build_far_apart_n` particalizes any bodies and separates them for relax (docs/58 #7).** Earth + Moon
    /// build to a shared N-material table, placed far enough apart that mutual gravity is negligible.
    #[test]
    fn build_far_apart_n_particalizes_and_separates_the_bodies() {
        let (earth, moon) = (crate::planet::earth(), crate::planet::moon());
        let (parts, eos, soft, dt) = build_far_apart_n(&[(&earth, 1500), (&moon, 300)], 40.0);
        assert!(eos.len() >= 3, "shared N-material EOS table (got {})", eos.len());
        assert!(soft > 0.0 && dt > 0.0, "positive softening ({soft:.2e}) and relax dt ({dt:.2e})");
        let com = |prov: u32| {
            let ps: Vec<&SphParticle> = parts.iter().filter(|p| p.prov == prov).collect();
            let m: f64 = ps.iter().map(|p| p.mass as f64).sum();
            ps.iter().map(|p| DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) * p.mass as f64).sum::<DVec3>() / m
        };
        let sep = (com(1) - com(0)).length();
        assert!(sep > 10.0 * earth.radius(), "bodies placed far apart for relax (sep {sep:.2e})");
    }
}
