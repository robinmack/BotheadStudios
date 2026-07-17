//! Self-gravitating condensed-matter body in hydrostatic equilibrium (docs/33 stage 2). This is the proof
//! that a planet can be REAL MATTER — a cloud of particles that holds itself up under its own gravity via
//! its equation-of-state pressure — instead of the rigid analytic boundary the impact scene uses today
//! (docs/28 root cause #1). It is the "merge" the architecture map identified (docs/32 §3): it COMPOSES the
//! shared kernels rather than forking them —
//!   • pressure: [`crate::eos::Tillotson`] `P(ρ, u)` (docs/33 stage 1),
//!   • SPH: the ONE cubic-spline kernel [`crate::atmosphere::sph_w`]/[`sph_dw`],
//!   • self-gravity: [`crate::bhtree::BarnesHut`] (O(N log N)),
//! exactly the pieces `atmosphere::AirField` (verified 3D hydrostatic balance) and `aggregate::Aggregate`
//! already use. The only new physics is the condensed EOS closure; everything else is reused. At the
//! unification stage (docs/33 stage 5) this capability folds INTO `Aggregate` so a planet and its debris are
//! one particle system — here it is a focused, independently-verifiable module to get the physics right
//! first (the correctness-first discipline).
//!
//! **Method (Genda et al. 2012).** SPH particles are all **equal mass**, laid down at the number density
//! that recovers each material's reference density (`ρ₀`), with a **per-particle smoothing length**
//! `h_i ∝ (m/ρ₀)^⅓` so a dense core is finely sampled and a light mantle coarsely — the standard SPH cure
//! for the density errors that made an earlier equal-VOLUME/unequal-mass prototype puff up. A single
//! material fills a uniform sphere; a differentiated planet packs a dense core inside a lighter mantle.
//! Relax under self-gravity + the symmetric SPH-EOS pressure force with light velocity damping until it
//! settles; then VERIFY the settled field satisfies hydrostatic equilibrium pointwise: `dP/dr = −ρ(r)·g(r)`,
//! with `g(r) = G·M(<r)/r²` from the enclosed particle mass. Isothermal this stage (u fixed) — the adiabatic
//! energy equation under compression is the stage-3 refinement.

use crate::eos::Tillotson;
use glam::DVec3;

const FOUR_THIRDS_PI: f64 = 4.0 / 3.0 * std::f64::consts::PI;

/// A deterministic near-uniform direction on the unit sphere (Fibonacci sphere) — no RNG (which the sim
/// forbids), reproducible across runs. `offset` decorrelates two shells that share an index range.
fn fib_dir(i: usize, n: usize, offset: f64) -> DVec3 {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let theta = golden * i as f64 + offset;
    DVec3::new(theta.cos() * r, y, theta.sin() * r)
}

/// A self-gravitating condensed-matter body: equal-mass particles, each with its OWN material EOS and
/// smoothing length (so a layered planet is one body of mixed materials).
pub struct HydroBody {
    pub pos: Vec<DVec3>,
    pub vel: Vec<DVec3>,
    pub mass: Vec<f64>,
    /// Specific internal energy per particle (J/kg). Fixed in stage 2 (isothermal relaxation).
    pub u: Vec<f64>,
    /// The condensed-matter EOS for each particle.
    pub eos: Vec<Tillotson>,
    /// Per-particle SPH smoothing length (m) — `∝ (m/ρ₀)^⅓`, so denser material is sampled more finely.
    pub h: Vec<f64>,
    /// Gravitational softening (m) — at half the FINEST particle spacing so gravity is honest to touching.
    pub softening: f64,
    /// Cached SPH density from the last [`Self::compute_density`] (kg/m³).
    pub rho: Vec<f64>,
}

/// Smoothing length for a particle of mass `m` in material of reference density `rho0`: ≈ 2 mean spacings.
fn smoothing_for(m: f64, rho0: f64) -> f64 {
    2.0 * (m / rho0).cbrt()
}

impl HydroBody {
    /// Build a single-material sphere of `n` equal-mass particles totalling `total_mass`, filled at uniform
    /// number density (initial density ≈ ρ₀), each at temperature `temp_k` → `u = c·temp_k`.
    pub fn new_sphere(
        eos: Tillotson,
        total_mass: f64,
        temp_k: f64,
        specific_heat: f64,
        n: usize,
    ) -> Self {
        let r0 = (total_mass / (FOUR_THIRDS_PI * eos.rho0)).cbrt();
        let m_i = total_mass / n as f64;
        let h_i = smoothing_for(m_i, eos.rho0);
        let pos: Vec<DVec3> = (0..n)
            .map(|i| fib_dir(i, n, 0.0) * (r0 * ((i as f64 + 0.5) / n as f64).cbrt()))
            .collect();
        HydroBody {
            vel: vec![DVec3::ZERO; n],
            mass: vec![m_i; n],
            u: vec![specific_heat * temp_k; n],
            eos: vec![eos; n],
            h: vec![h_i; n],
            softening: 0.5 * (m_i / eos.rho0).cbrt(),
            rho: vec![eos.rho0; n],
            pos,
        }
    }

    /// Build a DIFFERENTIATED planet (Genda-style): a dense `core` material inside `core_radius`, a lighter
    /// `mantle` out to `total_radius`, with EQUAL-MASS particles (mass chosen so `≈target_n` total). Each
    /// region is filled at the number density that recovers its ρ₀, and each particle gets its material's
    /// smoothing length. Internal energy `u` is set uniformly (Genda uses 1×10⁶ J/kg). Self-gravity then
    /// compresses it; the test checks it SETTLES (compresses, does not puff up), stays STRATIFIED (dense core
    /// stays central), and holds hydrostatic balance.
    pub fn new_differentiated(
        core: Tillotson,
        mantle: Tillotson,
        core_radius: f64,
        total_radius: f64,
        u_specific: f64,
        target_n: usize,
    ) -> Self {
        let v_core = FOUR_THIRDS_PI * core_radius.powi(3);
        let v_mantle = FOUR_THIRDS_PI * (total_radius.powi(3) - core_radius.powi(3));
        let m_core = core.rho0 * v_core;
        let m_mantle = mantle.rho0 * v_mantle;
        // Equal particle mass, chosen so the total count is ≈ target_n.
        let m_i = (m_core + m_mantle) / target_n as f64;
        let n_core = (m_core / m_i).round().max(1.0) as usize;
        let n_mantle = (m_mantle / m_i).round().max(1.0) as usize;
        let (mut pos, mut eos, mut h) = (Vec::new(), Vec::new(), Vec::new());
        // Core: uniform in the core sphere.
        for i in 0..n_core {
            let rr = core_radius * ((i as f64 + 0.5) / n_core as f64).cbrt();
            pos.push(fib_dir(i, n_core, 0.0) * rr);
            eos.push(core);
            h.push(smoothing_for(m_i, core.rho0));
        }
        // Mantle: uniform in the shell [core_radius, total_radius] (equal-volume radii).
        let (rc3, rt3) = (core_radius.powi(3), total_radius.powi(3));
        for i in 0..n_mantle {
            let rr = (rc3 + (rt3 - rc3) * (i as f64 + 0.5) / n_mantle as f64).cbrt();
            pos.push(fib_dir(i, n_mantle, 1.7) * rr);
            eos.push(mantle);
            h.push(smoothing_for(m_i, mantle.rho0));
        }
        let n = pos.len();
        HydroBody {
            vel: vec![DVec3::ZERO; n],
            mass: vec![m_i; n],
            u: vec![u_specific; n],
            rho: (0..n).map(|i| eos[i].rho0).collect(),
            softening: 0.5 * (m_i / core.rho0).cbrt(), // finest (core) spacing
            eos,
            h,
            pos,
        }
    }

    /// SPH density ρ_i = Σ_j m_j W(r_ij, h_ij) + self, with a symmetric per-pair smoothing length
    /// h_ij = ½(h_i+h_j) (so variable-resolution regions couple momentum-conservingly). Cached in `self.rho`.
    pub fn compute_density(&mut self) {
        let n = self.pos.len();
        let cell = self.h.iter().cloned().fold(0.0, f64::max);
        let grid = crate::neighbors::NeighborGrid::build(&self.pos, cell);
        let mut rho: Vec<f64> = (0..n).map(|i| self.mass[i] * crate::atmosphere::sph_w(0.0, self.h[i])).collect();
        grid.for_each_pair(&self.pos, |i, j| {
            let r = (self.pos[i] - self.pos[j]).length();
            let hij = 0.5 * (self.h[i] + self.h[j]);
            if r < hij {
                let w = crate::atmosphere::sph_w(r, hij);
                rho[i] += self.mass[j] * w;
                rho[j] += self.mass[i] * w;
            }
        });
        self.rho = rho;
    }

    /// Per-particle acceleration: Barnes–Hut self-gravity + the symmetric SPH-EOS pressure force
    /// `a_i = −Σ_j m_j (P_i/ρ_i² + P_j/ρ_j²) ∇W(r,h_ij)`, `P = Tillotson(ρ, u)`. Assumes `compute_density` ran.
    pub fn accelerations(&self) -> Vec<DVec3> {
        let n = self.pos.len();
        let bh = crate::bhtree::BarnesHut::build(&self.pos, &self.mass, 0.5, self.softening);
        let mut acc = bh.accelerations(&self.pos, &self.mass);
        let p: Vec<f64> = (0..n).map(|i| self.eos[i].pressure(self.rho[i], self.u[i])).collect();
        let cell = self.h.iter().cloned().fold(0.0, f64::max);
        let grid = crate::neighbors::NeighborGrid::build(&self.pos, cell);
        grid.for_each_pair(&self.pos, |i, j| {
            let dv = self.pos[i] - self.pos[j];
            let r = dv.length();
            let hij = 0.5 * (self.h[i] + self.h[j]);
            if r >= hij || r < 1.0e-9 {
                return;
            }
            let term = p[i] / (self.rho[i] * self.rho[i]) + p[j] / (self.rho[j] * self.rho[j]);
            let grad = (dv / r) * crate::atmosphere::sph_dw(r, hij); // dW<0 ⇒ repulsive
            acc[i] += grad * (-term * self.mass[j]);
            acc[j] += grad * (term * self.mass[i]);
        });
        acc
    }

    /// One damped relaxation step (settle toward equilibrium): recompute density, then
    /// `v = (v + a·dt)·damp; x += v·dt`. Damping is numerical — the equilibrium (dP/dr=−ρg) is the physics,
    /// exactly as in `atmosphere::AirField::relax_step`.
    pub fn relax_step(&mut self, dt: f64, damp: f64) {
        self.compute_density();
        let acc = self.accelerations();
        for i in 0..self.pos.len() {
            self.vel[i] = (self.vel[i] + acc[i] * dt) * damp;
            self.pos[i] += self.vel[i] * dt;
        }
    }

    /// A CFL-safe relaxation timestep: dt = cfl·min(h)/max(c), the stiffest+finest constraint.
    pub fn relax_dt(&self, cfl: f64) -> f64 {
        let min_h = self.h.iter().cloned().fold(f64::INFINITY, f64::min);
        let u0 = self.u.first().copied().unwrap_or(1.0);
        let c_max = self
            .eos
            .iter()
            .map(|e| e.sound_speed_sq(e.rho0, u0).sqrt())
            .fold(1.0_f64, f64::max);
        cfl * min_h / c_max.max(1.0)
    }

    pub fn com(&self) -> DVec3 {
        let m: f64 = self.mass.iter().sum();
        self.pos.iter().zip(&self.mass).map(|(p, &mi)| *p * mi).sum::<DVec3>() / m.max(1e-30)
    }

    /// Mass-weighted RMS radius about the COM — the stability yardstick (a settled body holds it steady).
    pub fn rms_radius(&self) -> f64 {
        let c = self.com();
        let m: f64 = self.mass.iter().sum();
        let s: f64 = self.pos.iter().zip(&self.mass).map(|(p, &mi)| mi * (*p - c).length_squared()).sum();
        (s / m.max(1e-30)).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::G;

    fn enclosed_mass(b: &HydroBody, c: DVec3, r: f64) -> f64 {
        b.pos.iter().zip(&b.mass).filter(|(p, _)| (**p - c).length() <= r).map(|(_, &m)| m).sum()
    }

    /// Shell-mean pressure and density at radius `r` (mean over particles in [r−dr, r+dr]); returns count too.
    fn shell(b: &HydroBody, c: DVec3, r: f64, dr: f64) -> (f64, f64, usize) {
        let (mut sp, mut sr, mut cnt) = (0.0, 0.0, 0usize);
        for i in 0..b.pos.len() {
            if ((b.pos[i] - c).length() - r).abs() <= dr {
                sp += b.eos[i].pressure(b.rho[i], b.u[i]);
                sr += b.rho[i];
                cnt += 1;
            }
        }
        if cnt == 0 { (0.0, 0.0, 0) } else { (sp / cnt as f64, sr / cnt as f64, cnt) }
    }

    #[test]
    #[ignore = "self-gravitating relaxation (~thousands of steps) — run with --ignored"]
    fn a_self_gravitating_eos_body_settles_into_hydrostatic_balance() {
        // docs/33 stage 2a: a single-material rocky body must relax under self-gravity + Tillotson pressure
        // into a STABLE hydrostatic equilibrium whose settled field satisfies dP/dr = −ρ(r)·g(r) pointwise.
        let eos = Tillotson::basalt();
        let r0 = 1.5e6;
        let total_mass = FOUR_THIRDS_PI * r0 * r0 * r0 * eos.rho0;
        let mut b = HydroBody::new_sphere(eos, total_mass, 300.0, 840.0, 3000);
        let dt = b.relax_dt(0.2);
        let mut rms_hist = Vec::new();
        for step in 0..4000 {
            b.relax_step(dt, 0.96);
            if step % 200 == 0 {
                rms_hist.push(b.rms_radius());
            }
        }
        b.compute_density();
        let c = b.com();
        let last = &rms_hist[rms_hist.len().saturating_sub(4)..];
        let mean: f64 = last.iter().sum::<f64>() / last.len() as f64;
        let spread = last.iter().map(|r| (r - mean).abs()).fold(0.0, f64::max) / mean;
        println!("2a settled RMS radius {:.0} km (spread {:.1}%)", mean / 1e3, spread * 100.0);
        assert!(spread < 0.05, "body must settle to a steady RMS radius (spread {spread:.2})");
        assert!(mean > 0.3 * r0 && mean < 1.2 * r0, "RMS radius {mean:.3e} sane vs R₀={r0:.3e}");

        let dr = 0.12 * mean;
        let mut checked = 0;
        for frac in [0.35_f64, 0.55] {
            let r = frac * mean;
            let (p_lo, _, n_lo) = shell(&b, c, r - dr, dr);
            let (p_hi, _, n_hi) = shell(&b, c, r + dr, dr);
            let (_, rho_mid, n_mid) = shell(&b, c, r, dr);
            if n_lo < 20 || n_hi < 20 || n_mid < 20 {
                continue;
            }
            let dpdr = (p_hi - p_lo) / (2.0 * dr);
            let expect = -rho_mid * G * enclosed_mass(&b, c, r) / (r * r);
            let rel = (dpdr - expect).abs() / expect.abs().max(1.0);
            println!("2a hydrostatic @ r={:.0} km: dP/dr {:.3e} vs −ρg {:.3e} (rel {:.2})", r / 1e3, dpdr, expect, rel);
            assert!(dpdr < 0.0, "pressure must DECREASE outward at r={r:.3e}");
            assert!(rel < 0.5, "hydrostatic balance within operator error at r={r:.3e} (rel {rel:.2})");
            checked += 1;
        }
        assert!(checked >= 1, "at least one interior shell must be testable");
    }

    #[test]
    #[ignore = "self-gravitating relaxation of a two-material body — run with --ignored"]
    fn a_differentiated_iron_core_earth_settles_compresses_and_stratifies() {
        // docs/33 stage 2b: an EARTH-MASS differentiated planet — iron core + basalt mantle, per-particle EOS
        // and smoothing length, EQUAL-MASS particles (the Genda fix for the earlier puff-up). It must
        // (1) SETTLE and COMPRESS (RMS ≤ initial — the anti-puff-up guard), (2) stay STRATIFIED (iron core
        // stays central and ends DENSER than its ρ₀), (3) hold hydrostatic balance, and (4) reach a central
        // pressure of the ORDER of Earth's real 364 GPa (Wissing & Hobbs 2020) — coarse-N, so order not exact.
        let core = Tillotson::iron(); // compressed branch verified (Wissing & Hobbs 2020)
        let mantle = Tillotson::basalt(); // verified (Benz & Asphaug 1999)
        // Uncompressed radii giving ≈ Earth mass with an Earth-like core fraction (compresses under gravity).
        let total_r = 7.37e6;
        let core_r = 0.55 * total_r;
        let mut b = HydroBody::new_differentiated(core, mantle, core_r, total_r, 1.0e6, 3000);
        let is_core: Vec<bool> = b.eos.iter().map(|e| e.rho0 == core.rho0).collect();
        let m_total: f64 = b.mass.iter().sum();
        println!("differentiated: N={}, M={:.2e} kg (Earth 5.97e24), initial R≈{:.0} km", b.pos.len(), m_total, total_r / 1e3);

        let dt = b.relax_dt(0.2);
        let mut rms_hist = Vec::new();
        for step in 0..5000 {
            b.relax_step(dt, 0.95);
            if step % 250 == 0 {
                rms_hist.push(b.rms_radius());
            }
        }
        b.compute_density();
        let c = b.com();

        // (1) SETTLE + COMPRESS (not puff up). The initial mass-weighted RMS of a uniform sphere is √(3/5)·R.
        let last = &rms_hist[rms_hist.len().saturating_sub(4)..];
        let mean: f64 = last.iter().sum::<f64>() / last.len() as f64;
        let spread = last.iter().map(|r| (r - mean).abs()).fold(0.0, f64::max) / mean;
        let rms_init = (3.0f64 / 5.0).sqrt() * total_r;
        println!("settled RMS {:.0} km (init ≈{:.0} km, spread {:.1}%)", mean / 1e3, rms_init / 1e3, spread * 100.0);
        assert!(spread < 0.06, "body must settle (spread {spread:.2})");
        assert!(mean <= 1.05 * rms_init, "body must COMPRESS not puff up (settled {mean:.3e} vs init {rms_init:.3e})");

        // (2) STRATIFICATION + core compression.
        let mean_r = |sel: bool| {
            let (mut s, mut n) = (0.0, 0usize);
            for i in 0..b.pos.len() { if is_core[i] == sel { s += (b.pos[i] - c).length(); n += 1; } }
            s / n.max(1) as f64
        };
        let dens = |sel: bool| {
            let (mut s, mut n) = (0.0, 0usize);
            for i in 0..b.pos.len() { if is_core[i] == sel { s += b.rho[i]; n += 1; } }
            s / n.max(1) as f64
        };
        let (rc, rm) = (mean_r(true), mean_r(false));
        let (dc, dm) = (dens(true), dens(false));
        println!("mean radius: core {:.0} km, mantle {:.0} km · settled ρ: core {:.0}, mantle {:.0} kg/m³", rc / 1e3, rm / 1e3, dc, dm);
        assert!(rc < 0.7 * rm, "iron core must stay stratified inside the mantle");
        assert!(dc > 1.5 * dm, "iron core must be denser than the rock mantle");
        assert!(dc > core.rho0, "iron core must be COMPRESSED above its ρ₀ (got {dc:.0} vs {:.0})", core.rho0);

        // (3) hydrostatic balance at an interior shell.
        let dr = 0.14 * mean;
        let r = 0.5 * mean;
        let (p_lo, _, n_lo) = shell(&b, c, r - dr, dr);
        let (p_hi, _, n_hi) = shell(&b, c, r + dr, dr);
        let (_, rho_mid, n_mid) = shell(&b, c, r, dr);
        assert!(n_lo >= 20 && n_hi >= 20 && n_mid >= 20, "interior shell must be populated");
        let dpdr = (p_hi - p_lo) / (2.0 * dr);
        let expect = -rho_mid * G * enclosed_mass(&b, c, r) / (r * r);
        let rel = (dpdr - expect).abs() / expect.abs().max(1.0);
        println!("hydrostatic @ r={:.0} km: dP/dr {:.3e} vs −ρg {:.3e} (rel {:.2})", r / 1e3, dpdr, expect, rel);
        assert!(dpdr < 0.0 && rel < 0.6, "hydrostatic balance within operator error (rel {rel:.2})");

        // (4) central pressure of the ORDER of Earth's 364 GPa (coarse-N: order, not exact).
        let (p_center, _, _) = shell(&b, c, 0.12 * mean, 0.12 * mean);
        println!("central P {:.3e} Pa (Earth ≈ 3.64e11)", p_center);
        assert!(p_center > 5.0e10 && p_center < 2.0e12, "central pressure must be ~100s of GPa, got {p_center:.2e}");
    }
}
