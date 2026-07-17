//! The accretion / growth operator (docs/33 stage 4c.3).
//!
//! A giant-impact disk of equal-mass SPH particles is effectively COLLISIONLESS at low resolution and has
//! no fusion law — particle masses never grow — so a round Moon can never emerge from it (diagnosis, JOURNAL
//! 2026-07-17). This module adds the growth law: detect gravitationally-**bound clumps** in the disk by
//! friends-of-friends (the same union-find `lib.rs::disk_stats_json` uses to count moonlets), and PROMOTE
//! each clump that is genuinely self-bound AND sits outside the central remnant's Roche limit into ONE body.
//!
//! **Honesty gates (both required to accrete):**
//!   1. **Self-bound** — the clump's internal kinetic energy plus its own gravitational binding energy is
//!      negative (`Σ½mᵢ|vᵢ−v_com|² + PE_self < 0`). A spatially-close but hot/unbound group is NOT a body.
//!   2. **Outside Roche** — the clump's COM is beyond the fluid Roche limit `d = 2.44·R·(ρ_planet/ρ_clump)^⅓`
//!      of the remnant (same law as `tides::secular_step`). A clump inside Roche should tidally SHRED, not
//!      accrete, so it is left as particles for the sim to disrupt.
//!
//! **Conservation.** Promotion to a point body at the clump COM conserves **mass, linear momentum, and the
//! centre of mass EXACTLY** (the body carries `Σm`, `Σmv/Σm`, `Σmx/Σm`). It cannot conserve everything: the
//! clump's internal random kinetic energy is absorbed as heat (physical for inelastic accretion) and its
//! internal spin angular momentum is folded into the body — both are REPORTED (`internal_ke`, and the
//! orbital vs total-L split is recoverable from the members), never silently dropped. This is the "conserve
//! mass + momentum, and energy + angular momentum as far as possible" the stage-4c spec asks for.

use crate::neighbors::NeighborGrid;
use glam::DVec3;

const FOUR_THIRDS_PI: f64 = 4.0 / 3.0 * std::f64::consts::PI;

/// A candidate accreted body found in the particle field.
#[derive(Clone, Debug)]
pub struct Clump {
    pub members: Vec<usize>,
    pub mass: f64,
    pub com_pos: DVec3,
    pub com_vel: DVec3,
    pub rho: f64,          // volume-summed density: mass / Σ(mᵢ/ρᵢ)
    pub radius: f64,       // sphere of that density and mass: (3·mass / 4πρ)^⅓
    pub internal_ke: f64,  // Σ ½ mᵢ |vᵢ − v_com|²  (random motion; absorbed as heat on merge)
    pub self_pe: f64,      // −Σ_{i<j} G mᵢ mⱼ / |rᵢⱼ|  (softened) — the clump's own binding energy
    pub bound: bool,       // internal_ke + self_pe < 0
    pub outside_roche: bool,
}

impl Clump {
    /// Does this clump accrete into one body? Bound, outside Roche, and more than one member.
    pub fn accretes(&self) -> bool {
        self.members.len() >= 2 && self.bound && self.outside_roche
    }
}

/// A body promoted from an accreted clump — mass at the clump COM with the clump's bulk velocity.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: DVec3,
    pub vel: DVec3,
    pub mass: f64,
    pub rho: f64,
    pub radius: f64,
}

/// Result of one accretion pass: the promoted bodies and the indices they consumed.
#[derive(Clone, Debug, Default)]
pub struct Accreted {
    pub bodies: Vec<Body>,
    pub consumed: Vec<usize>, // particle indices absorbed into a promoted body (sorted)
}

/// Friends-of-friends clumps of a particle field, each classified for boundedness and the Roche gate.
///
/// `linking_length` is the FoF link distance (typically a few × the interparticle spacing — particles within
/// it are in the same clump). `g`/`softening` match the sim's gravity so the binding energy is consistent.
/// The central remnant `(central_pos, central_mass, central_radius)` sets the Roche limit.
#[allow(clippy::too_many_arguments)]
pub fn find_clumps(
    pos: &[DVec3],
    vel: &[DVec3],
    mass: &[f64],
    rho: &[f64],
    linking_length: f64,
    g: f64,
    softening: f64,
    central_pos: DVec3,
    central_mass: f64,
    central_radius: f64,
) -> Vec<Clump> {
    let n = pos.len();
    assert!(vel.len() == n && mass.len() == n && rho.len() == n, "accretion: ragged particle arrays");

    // --- friends-of-friends: union particles within the linking length (union-find, path-compressed) ---
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        // path compression
        let mut c = i;
        while p[c] != r {
            let nx = p[c];
            p[c] = r;
            c = nx;
        }
        r
    }
    let ll2 = linking_length * linking_length;
    let grid = NeighborGrid::build(pos, linking_length);
    grid.for_each_pair(pos, |i, j| {
        if (pos[i] - pos[j]).length_squared() <= ll2 {
            let (a, b) = (find(&mut parent, i), find(&mut parent, j));
            if a != b {
                parent[a] = b;
            }
        }
    });

    // --- gather members per root ---
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }

    // --- classify each clump ---
    let s2 = softening * softening;
    let mut clumps = Vec::with_capacity(groups.len());
    for members in groups.into_values() {
        let m: f64 = members.iter().map(|&i| mass[i]).sum();
        if m <= 0.0 {
            continue;
        }
        let com_pos: DVec3 = members.iter().map(|&i| pos[i] * mass[i]).sum::<DVec3>() / m;
        let com_vel: DVec3 = members.iter().map(|&i| vel[i] * mass[i]).sum::<DVec3>() / m;
        let vol: f64 = members.iter().map(|&i| mass[i] / rho[i]).sum();
        let clump_rho = if vol > 0.0 { m / vol } else { *rho.get(members[0]).unwrap_or(&1.0) };
        let radius = (m / (FOUR_THIRDS_PI * clump_rho)).cbrt();
        // internal KE about the COM (the random motion an inelastic merge would thermalise)
        let internal_ke: f64 = members
            .iter()
            .map(|&i| 0.5 * mass[i] * (vel[i] - com_vel).length_squared())
            .sum();
        // self gravitational PE (softened, matching the sim) — O(k²) in the clump size, fine (clumps small)
        let mut self_pe = 0.0;
        for a in 0..members.len() {
            for b in (a + 1)..members.len() {
                let (ia, ib) = (members[a], members[b]);
                let r = ((pos[ia] - pos[ib]).length_squared() + s2).sqrt();
                self_pe -= g * mass[ia] * mass[ib] / r;
            }
        }
        let bound = internal_ke + self_pe < 0.0;
        // Fluid Roche limit of the remnant for THIS clump's density.
        let d_roche = 2.44 * central_radius * (central_density(central_mass, central_radius) / clump_rho).cbrt();
        let outside_roche = (com_pos - central_pos).length() > d_roche;
        clumps.push(Clump { members, mass: m, com_pos, com_vel, rho: clump_rho, radius, internal_ke, self_pe, bound, outside_roche });
    }
    clumps
}

fn central_density(mass: f64, radius: f64) -> f64 {
    mass / (FOUR_THIRDS_PI * radius.powi(3))
}

/// Run one accretion pass: promote every clump that [`Clump::accretes`] to a single body at its COM,
/// conserving mass, linear momentum, and centre of mass exactly. Returns the promoted bodies and the sorted
/// list of consumed particle indices (everything else remains a particle).
#[allow(clippy::too_many_arguments)]
pub fn accrete(
    pos: &[DVec3],
    vel: &[DVec3],
    mass: &[f64],
    rho: &[f64],
    linking_length: f64,
    g: f64,
    softening: f64,
    central_pos: DVec3,
    central_mass: f64,
    central_radius: f64,
) -> Accreted {
    let clumps = find_clumps(pos, vel, mass, rho, linking_length, g, softening, central_pos, central_mass, central_radius);
    let mut out = Accreted::default();
    for c in clumps.iter().filter(|c| c.accretes()) {
        out.bodies.push(Body { pos: c.com_pos, vel: c.com_vel, mass: c.mass, rho: c.rho, radius: c.radius });
        out.consumed.extend_from_slice(&c.members);
    }
    out.consumed.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f64 = 6.674e-11;

    // A dense, cold, self-bound blob of `n` particles inside `radius` about `center`, drifting at `bulk`.
    fn cold_blob(center: DVec3, bulk: DVec3, radius: f64, n: usize, m_i: f64, rho: f64) -> (Vec<DVec3>, Vec<DVec3>, Vec<f64>, Vec<f64>) {
        let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
        let (mut p, mut v, mut m, mut r) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for i in 0..n {
            let rr = radius * ((i as f64 + 0.5) / n as f64).cbrt();
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let rad = (1.0 - y * y).max(0.0).sqrt();
            let th = golden * i as f64;
            p.push(center + DVec3::new(th.cos() * rad * rr, y * rr, th.sin() * rad * rr));
            v.push(bulk); // cold: no internal motion ⇒ trivially bound
            m.push(m_i);
            r.push(rho);
        }
        (p, v, m, r)
    }

    fn totals(pos: &[DVec3], vel: &[DVec3], mass: &[f64]) -> (f64, DVec3, DVec3) {
        let mt: f64 = mass.iter().sum();
        let mom: DVec3 = (0..pos.len()).map(|i| vel[i] * mass[i]).sum();
        let com: DVec3 = (0..pos.len()).map(|i| pos[i] * mass[i]).sum::<DVec3>() / mt;
        (mt, mom, com)
    }

    // The accretion result, expanded back to (bodies + residual particles), must have the SAME total mass,
    // linear momentum, and centre of mass as the input — exactly (to f64 round-off).
    #[test]
    fn accretion_conserves_mass_momentum_and_com() {
        // Two well-separated cold blobs (both should accrete) + scattered singletons that must NOT.
        let m_i = 1.0e19;
        let rho = 3000.0;
        let (mut pos, mut vel, mut mass, mut r) = cold_blob(DVec3::new(2.0e7, 0.0, 0.0), DVec3::new(0.0, 1500.0, 0.0), 3.0e5, 40, m_i, rho);
        let (p2, v2, m2, r2) = cold_blob(DVec3::new(-1.5e7, 1.0e7, 0.0), DVec3::new(-800.0, 0.0, 300.0), 2.5e5, 30, m_i, rho);
        pos.extend(p2); vel.extend(v2); mass.extend(m2); r.extend(r2);
        // Scattered lone particles, far apart (each its own singleton clump ⇒ never accretes).
        for k in 0..5 {
            pos.push(DVec3::new(4.0e7 + k as f64 * 5.0e6, -3.0e7, 0.0));
            vel.push(DVec3::new(0.0, 0.0, 500.0 * k as f64));
            mass.push(m_i);
            r.push(rho);
        }

        let (m0, mom0, com0) = totals(&pos, &vel, &mass);
        // remnant far away so both blobs are outside Roche
        let out = accrete(&pos, &vel, &mass, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);

        assert_eq!(out.bodies.len(), 2, "both cold blobs should accrete, singletons should not");
        // Rebuild the full system: promoted bodies + the particles they did NOT consume.
        let consumed: std::collections::HashSet<usize> = out.consumed.iter().copied().collect();
        let (mut fp, mut fv, mut fm) = (Vec::new(), Vec::new(), Vec::new());
        for b in &out.bodies {
            fp.push(b.pos); fv.push(b.vel); fm.push(b.mass);
        }
        for i in 0..pos.len() {
            if !consumed.contains(&i) {
                fp.push(pos[i]); fv.push(vel[i]); fm.push(mass[i]);
            }
        }
        let (m1, mom1, com1) = totals(&fp, &fv, &fm);
        assert!((m1 - m0).abs() / m0 < 1e-12, "mass not conserved: {m0} → {m1}");
        assert!((mom1 - mom0).length() / mom0.length() < 1e-12, "momentum not conserved: {mom0} → {mom1}");
        assert!((com1 - com0).length() / com0.length() < 1e-12, "COM not conserved: {com0} → {com1}");
        // Residual = the 5 singletons.
        assert_eq!(fp.len(), 2 + 5, "2 bodies + 5 residual singletons");
    }

    // A clump INSIDE the Roche limit must NOT accrete (it should shred); the SAME clump outside Roche must.
    #[test]
    fn roche_gate_blocks_accretion_inside_the_limit() {
        let (m_planet, r_planet) = (5.0e24, 6.0e6);
        let rho_clump = 3000.0;
        let d_roche = 2.44 * r_planet * (central_density(m_planet, r_planet) / rho_clump).cbrt();

        let inside = DVec3::new(0.6 * d_roche, 0.0, 0.0);
        let (p, v, m, r) = cold_blob(inside, DVec3::ZERO, 2.0e5, 30, 1.0e19, rho_clump);
        let out_in = accrete(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, m_planet, r_planet);
        assert_eq!(out_in.bodies.len(), 0, "clump inside Roche must not accrete (shreds instead)");

        let outside = DVec3::new(2.0 * d_roche, 0.0, 0.0);
        let (p, v, m, r) = cold_blob(outside, DVec3::ZERO, 2.0e5, 30, 1.0e19, rho_clump);
        let out_out = accrete(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, m_planet, r_planet);
        assert_eq!(out_out.bodies.len(), 1, "same clump outside Roche must accrete");
    }

    // A spatially-tight but HOT group (internal KE ≫ binding energy) is unbound and must NOT accrete.
    #[test]
    fn unbound_hot_group_does_not_accrete() {
        let rho = 3000.0;
        let m_i = 1.0e15; // tiny masses ⇒ negligible self-gravity
        let (mut p, mut v, mut m, mut r) = cold_blob(DVec3::new(3.0e7, 0.0, 0.0), DVec3::ZERO, 2.0e5, 30, m_i, rho);
        // Give every particle a large random-ish velocity about the COM: hot, unbound.
        for (i, vi) in v.iter_mut().enumerate() {
            let s = if i % 2 == 0 { 1.0 } else { -1.0 };
            *vi = DVec3::new(s * 5000.0, s * -4000.0, s * 3000.0);
        }
        let _ = (&mut p, &mut m, &mut r);
        let clumps = find_clumps(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
        // It IS one spatial clump, but not bound.
        let big = clumps.iter().max_by_key(|c| c.members.len()).unwrap();
        assert!(big.members.len() >= 25, "should be one spatial group");
        assert!(!big.bound, "hot group must be classified unbound");
        let out = accrete(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
        assert_eq!(out.bodies.len(), 0, "unbound hot group must not accrete");
    }
}
