//! Self-gravitating particle aggregates — a body as a **cloud of particles held together by its own
//! gravity** (a "rubble pile"), rather than a point mass or a rigid sphere (`docs/21`).
//!
//! This is what makes celestial destruction a *simulation, not a mock*: the aggregate's cohesion and
//! (roughly spherical) shape **emerge** from mutual gravity (the representation invariant, `docs/15` —
//! roundness is emergent), and it **disrupts when given more energy than its gravitational binding
//! energy** — the particles simply exceed escape velocity and disperse. Nothing is scripted; a
//! shattered moon is the same N-body gravity that made it round, run past its binding energy.
//!
//! The particles reuse `orbit::Body` (pos, vel, mass). Gravity is **softened** (unlike the clean
//! two-body `orbit.rs`) because a dense cloud has close pairs whose bare 1/r² would explode.
//!
//! It also models a **cohesive solid** (`Aggregate::cohesive`, `docs/23`): particles held by material
//! **bonds** (Hookean spring + damper) rather than gravity — the honest way to make a metal ball *real
//! matter*. The damper dissipates energy, so a struck solid **settles to a ground state** instead of
//! ringing forever (a deterministic model reaches equilibrium); bonds **fracture** past a break strain,
//! so it **shatters emergently** under a hard impact — no scripted destroy. (The same contact-bond
//! mechanics, applied *between* surfaces, is where static-vs-kinetic friction would emerge from first
//! principles instead of two tabulated constants — a future subsystem.)

#![allow(dead_code)] // consumed by the space-band integration (staged) and native tests

use crate::materials::Material;
use crate::matter::REF_TEMP_K;
use crate::orbit::{Body, G};
use glam::DVec3;

/// A **material bond** between two particles — how a *solid* holds itself together (cohesion), as
/// opposed to a rubble pile held by gravity. A Hookean spring at its rest length; it **fractures**
/// (goes inactive) when stretched past the material's break strain. This is what makes the metal ball
/// real matter: it keeps its shape under load and *shatters emergently* under a hard enough impact —
/// no scripted "destroy" (`docs/23`).
#[derive(Clone, Copy)]
pub struct Bond {
    pub a: usize,
    pub b: usize,
    pub rest: f64,    // rest length (m)
    pub active: bool, // false once fractured
}

pub struct Aggregate {
    pub particles: Vec<Body>,
    /// Kelvin, per particle — heated by impacts (`deposit_impact`); drives the incandescent glow of
    /// molten/vaporized debris ([`crate::emission::incandescence`]).
    pub temps: Vec<f32>,
    /// Material index (uniform for now — a basalt Moon; per-particle composition is a later slice).
    pub material: usize,
    /// Softening length (m): removes the 1/r² singularity between close particles. ~half the mean
    /// spacing keeps the cloud stable without erasing its self-gravity.
    pub softening: f64,
    /// Material cohesion bonds (empty for a pure gravitational rubble pile; populated for a solid).
    pub bonds: Vec<Bond>,
    /// Bond spring constant (N/m).
    pub stiffness: f64,
    /// Bond damping (N·s/m) — internal friction that dissipates energy, so the solid **settles to a
    /// ground state** rather than ringing forever (Robin's point: a deterministic model reaches
    /// equilibrium). This is why "vibrate forever" was a bug: we were missing dissipation.
    pub damping: f64,
    /// Fractional stretch at which a bond fractures.
    pub break_strain: f64,
    /// Uniform external gravity (m/s²) applied to every particle — e.g. a planet's surface field for a
    /// ball resting on the ground. Zero for a free rubble pile (which makes its own gravity).
    pub gravity: DVec3,
}

impl Aggregate {
    pub fn new(particles: Vec<Body>, softening: f64) -> Self {
        let n = particles.len();
        Aggregate {
            particles,
            temps: vec![REF_TEMP_K; n],
            material: 0,
            softening,
            bonds: Vec::new(),
            stiffness: 0.0,
            damping: 0.0,
            break_strain: f64::INFINITY,
            gravity: DVec3::ZERO,
        }
    }

    /// Set a uniform external gravity (e.g. a planet's surface field).
    pub fn with_gravity(mut self, gravity: DVec3) -> Self {
        self.gravity = gravity;
        self
    }

    /// A **cohesive solid**: bond every pair of particles within `cutoff` at their current separation,
    /// so material strength (not gravity) holds it together. `stiffness` is the bond spring constant;
    /// `break_strain` is the fractional stretch at which a bond fractures.
    #[allow(clippy::too_many_arguments)]
    pub fn cohesive(
        particles: Vec<Body>,
        material: usize,
        softening: f64,
        cutoff: f64,
        stiffness: f64,
        damping: f64,
        break_strain: f64,
    ) -> Self {
        let mut bonds = Vec::new();
        for i in 0..particles.len() {
            for j in (i + 1)..particles.len() {
                let rest = (particles[j].pos - particles[i].pos).length();
                if rest <= cutoff {
                    bonds.push(Bond {
                        a: i,
                        b: j,
                        rest,
                        active: true,
                    });
                }
            }
        }
        let n = particles.len();
        Aggregate {
            particles,
            temps: vec![REF_TEMP_K; n],
            material,
            softening,
            bonds,
            stiffness,
            damping,
            break_strain,
            gravity: DVec3::ZERO,
        }
    }

    /// Number of intact (unfractured) bonds — a measure of structural integrity.
    pub fn active_bonds(&self) -> usize {
        self.bonds.iter().filter(|b| b.active).count()
    }

    /// Fracture any bond stretched past `break_strain` (called each step after the drift).
    fn break_overstrained_bonds(&mut self) {
        let bs = self.break_strain;
        for bond in &mut self.bonds {
            if !bond.active {
                continue;
            }
            let dist = (self.particles[bond.b].pos - self.particles[bond.a].pos).length();
            if (dist - bond.rest) / bond.rest > bs {
                bond.active = false; // fractured
            }
        }
    }

    /// Set the aggregate's material (its constituent stuff — e.g. basalt for the Moon).
    pub fn with_material(mut self, material: usize) -> Self {
        self.material = material;
        self
    }

    /// Deposit impact `energy` (J) at `site` travelling along `dir` — the same physics as
    /// `matter::impact`, on a self-gravitating cloud instead of a voxel grid (`docs/21`). Energy density
    /// peaks at the contact and falls off, so each particle **heats** (temperature from `e/(ρc)`) and is
    /// **kicked** outward + along the impact; vaporized parcels (`damage::classify`) expand faster.
    /// Whether the aggregate then **survives or shatters is emergent** — it falls out of the kick vs the
    /// self-gravity that binds it (run `step` and watch `rms_radius`). Energy-conserving deposit
    /// (`Σ eᵢ·Vᵢ = energy`).
    pub fn deposit_impact(&mut self, materials: &[Material], site: DVec3, dir: DVec3, energy: f64) {
        if self.particles.is_empty() {
            return;
        }
        let dir = dir.normalize_or_zero();
        let mat = &materials[self.material];
        let density = (mat.density as f64).max(1.0);
        let c = mat
            .thermal
            .as_ref()
            .map_or(1000.0, |t| t.specific_heat as f64);
        let vapor = crate::damage::vapor_energy_density(mat);

        // Coupling length ~ half the cloud's spread, so the energy concentrates near the contact.
        let lambda = (self.rms_radius() * 0.5).max(1.0);
        // Energy-conserving peak density: Σ eᵢ·Vᵢ = energy, with eᵢ = e0·exp(−dᵢ/λ), Vᵢ = mᵢ/ρ.
        let wsum: f64 = self
            .particles
            .iter()
            .map(|p| (-(p.pos - site).length() / lambda).exp() * (p.mass / density))
            .sum();
        if wsum <= 0.0 {
            return;
        }
        let e0 = energy / wsum;

        for (p, temp) in self.particles.iter_mut().zip(self.temps.iter_mut()) {
            let off = p.pos - site;
            let d = off.length();
            let e_i = e0 * (-d / lambda).exp(); // J/m³ deposited in this particle
            *temp += (e_i / (density * c)) as f32; // temperature rise

            // Velocity kick from the deposited energy density (shock): v ~ √(2·frac·e/ρ), outward from
            // the contact and along the impactor. Vaporized parcels are gas/plasma → they expand faster.
            let mut speed = (2.0 * 0.3 * e_i / density).sqrt();
            if let Some(ev) = vapor {
                if e_i >= ev {
                    speed *= 3.0;
                }
            }
            let kick = (off.normalize_or_zero() * 0.6 + dir * 0.4).normalize_or_zero();
            p.vel += kick * speed;
        }
    }

    /// Softened mutual-gravity acceleration on every particle (N-body).
    pub fn accelerations(&self) -> Vec<DVec3> {
        let s2 = self.softening * self.softening;
        let p = &self.particles;
        let mut acc = vec![self.gravity; p.len()]; // uniform external gravity (0 for a rubble pile)
        for i in 0..p.len() {
            for j in 0..p.len() {
                if i == j {
                    continue;
                }
                let d = p[j].pos - p[i].pos;
                let r2 = d.length_squared() + s2;
                acc[i] += d * (G * p[j].mass * r2.powf(-1.5));
            }
        }
        // Material cohesion: each intact bond is a Hookean spring toward its rest length, plus a damper
        // that dissipates along-bond motion — so a struck solid settles to a ground state (docs/23).
        for bond in &self.bonds {
            if !bond.active {
                continue;
            }
            let (pa, pb) = (&p[bond.a], &p[bond.b]);
            let d = pb.pos - pa.pos;
            let dist = d.length();
            if dist < 1e-9 {
                continue;
            }
            let n = d / dist;
            // Spring along the bond toward rest length; damper on the FULL relative velocity (internal
            // friction resists all relative motion — longitudinal AND shear — so every mode settles).
            let f = n * (self.stiffness * (dist - bond.rest)) + (pb.vel - pa.vel) * self.damping;
            acc[bond.a] += f / pa.mass;
            acc[bond.b] -= f / pb.mass;
        }
        acc
    }

    /// One velocity-Verlet step (symplectic; conserves energy over many dynamical times). Pass the
    /// same `acc` buffer each step, seeded with `accelerations()`.
    /// How many explicit substeps a step of `dt` needs to integrate the bonds **stably**. A stiff
    /// spring oscillates at ω = √(k_eff/m); velocity-Verlet is stable only for dt·ω ≲ 2, so a stiffer
    /// solid needs a finer timestep (`docs/23`). k_eff per particle ≈ (mean bonds/particle)·stiffness.
    /// Returns 1 for a bondless (purely gravitational) aggregate — gravity is soft and needs no
    /// subdivision. This is why rigidity is honest: we pay for real stiffness with real substeps rather
    /// than faking it.
    pub fn stable_substeps(&self, dt: f64) -> usize {
        if self.stiffness <= 0.0 || self.bonds.is_empty() || self.particles.is_empty() {
            return 1;
        }
        let mean_mass =
            self.particles.iter().map(|b| b.mass).sum::<f64>() / self.particles.len() as f64;
        let coordination = (2 * self.bonds.len()) as f64 / self.particles.len() as f64;
        let omega = (coordination.max(1.0) * self.stiffness / mean_mass.max(1e-9)).sqrt();
        // Target dt·ω ≤ 1 (a factor of 2 inside the stability limit for margin against the damper).
        ((dt * omega).ceil() as usize).max(1)
    }

    pub fn step(&mut self, acc: &mut Vec<DVec3>, dt: f64) {
        for (b, a) in self.particles.iter_mut().zip(acc.iter()) {
            b.vel += *a * (0.5 * dt);
            b.pos += b.vel * dt;
        }
        self.break_overstrained_bonds(); // fracture: bonds stretched past break_strain fail
        let new_acc = self.accelerations();
        for (b, a) in self.particles.iter_mut().zip(new_acc.iter()) {
            b.vel += *a * (0.5 * dt);
        }
        *acc = new_acc;
    }

    pub fn total_mass(&self) -> f64 {
        self.particles.iter().map(|b| b.mass).sum()
    }

    /// Center of mass.
    pub fn com(&self) -> DVec3 {
        let m = self.total_mass();
        if m <= 0.0 {
            return DVec3::ZERO;
        }
        self.particles
            .iter()
            .fold(DVec3::ZERO, |s, b| s + b.pos * b.mass)
            / m
    }

    /// Mass-weighted RMS radius about the COM — the cloud's *spread*. Bounded while the aggregate
    /// holds together; grows without limit once it disperses.
    pub fn rms_radius(&self) -> f64 {
        let m = self.total_mass();
        if m <= 0.0 {
            return 0.0;
        }
        let c = self.com();
        let s: f64 = self
            .particles
            .iter()
            .map(|b| b.mass * (b.pos - c).length_squared())
            .sum();
        (s / m).sqrt()
    }

    /// Gravitational **binding energy** (J, positive): `Σ_{i<j} G·m_i·m_j / r_ij` (softened) — the
    /// energy needed to disperse the aggregate to infinity. Give it more than this and it comes apart.
    pub fn binding_energy(&self) -> f64 {
        let s2 = self.softening * self.softening;
        let p = &self.particles;
        let mut e = 0.0;
        for i in 0..p.len() {
            for j in (i + 1)..p.len() {
                let r = ((p[j].pos - p[i].pos).length_squared() + s2).sqrt();
                e += G * p[i].mass * p[j].mass / r;
            }
        }
        e
    }

    /// Kinetic energy in the centre-of-mass frame (J) — the "disordered" energy that competes with
    /// binding. `kinetic_com > binding` ⇒ the aggregate flies apart.
    pub fn kinetic_energy_com(&self) -> f64 {
        let vcom = {
            let m = self.total_mass();
            if m <= 0.0 {
                DVec3::ZERO
            } else {
                self.particles
                    .iter()
                    .fold(DVec3::ZERO, |s, b| s + b.vel * b.mass)
                    / m
            }
        };
        self.particles
            .iter()
            .map(|b| 0.5 * b.mass * (b.vel - vcom).length_squared())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small cubic cloud of equal-mass particles, at rest.
    fn cloud(side: i32, spacing: f64, mass: f64) -> Vec<Body> {
        let mut v = Vec::new();
        for x in 0..side {
            for y in 0..side {
                for z in 0..side {
                    v.push(Body {
                        pos: DVec3::new(x as f64, y as f64, z as f64) * spacing,
                        vel: DVec3::ZERO,
                        mass,
                    });
                }
            }
        }
        v
    }

    #[test]
    fn a_stiffer_solid_needs_more_substeps_to_stay_stable() {
        // Rigidity is paid for honestly: a stiffer bond oscillates faster, so it needs a finer
        // timestep. The substep count rises with √stiffness, and a bondless (gravitational) pile
        // needs none.
        let soft = Aggregate::cohesive(cloud(3, 1.0, 7870.0), 0, 0.5, 1.75, 5.0e6, 1.0e4, 0.2);
        let stiff = Aggregate::cohesive(cloud(3, 1.0, 7870.0), 0, 0.5, 1.75, 5.0e9, 1.0e5, 0.2);
        let dt = 1.0 / 60.0;
        assert!(
            stiff.stable_substeps(dt) > soft.stable_substeps(dt),
            "1000× stiffness needs more substeps (stiff {} vs soft {})",
            stiff.stable_substeps(dt),
            soft.stable_substeps(dt)
        );
        // √1000 ≈ 32× the substeps.
        assert!(stiff.stable_substeps(dt) >= 20 * soft.stable_substeps(dt).max(1) / 10);
        // A purely gravitational pile has no bonds ⇒ no stiff subdivision needed.
        let pile = Aggregate::new(cloud(3, 100.0, 1.0e13), 50.0);
        assert_eq!(pile.stable_substeps(dt), 1);
    }

    #[test]
    fn a_self_gravitating_cloud_holds_together() {
        // A cold cloud bound by its own gravity does not fly apart — its spread stays bounded (it
        // collapses/virialises inward, it does not disperse). Cohesion is emergent, not glued.
        let mut agg = Aggregate::new(cloud(3, 100.0, 1.0e13), 50.0);
        let r0 = agg.rms_radius();
        let mut acc = agg.accelerations();
        for _ in 0..400 {
            agg.step(&mut acc, 2.0);
        }
        assert!(
            agg.rms_radius() < 3.0 * r0,
            "self-gravity keeps it bound (rms {:.1} vs r0 {:.1})",
            agg.rms_radius(),
            r0
        );
    }

    #[test]
    fn energy_above_binding_disrupts_it() {
        // Give the same cloud outward kinetic energy exceeding its binding energy and it comes
        // apart — emergent disruption, the identity behind a shattered moon (no scripted explosion).
        let mut agg = Aggregate::new(cloud(3, 100.0, 1.0e13), 50.0);
        let r0 = agg.rms_radius();
        let bind = agg.binding_energy();
        let com = agg.com();
        for b in &mut agg.particles {
            b.vel = (b.pos - com).normalize_or_zero() * 40.0; // outward kick
        }
        assert!(
            agg.kinetic_energy_com() > bind,
            "the kick exceeds binding (KE {:.2e} > bind {:.2e})",
            agg.kinetic_energy_com(),
            bind
        );

        let mut acc = agg.accelerations();
        for _ in 0..400 {
            agg.step(&mut acc, 2.0);
        }
        assert!(
            agg.rms_radius() > 10.0 * r0,
            "it disperses (rms {:.1} vs r0 {:.1})",
            agg.rms_radius(),
            r0
        );
    }

    #[test]
    fn an_impact_heats_the_core_and_shatters_the_aggregate() {
        // Deposit an impact into a self-gravitating basalt cloud: the particles heat (a radial gradient
        // — core hotter than rim) AND, with enough energy, the aggregate flies apart. The shatter is
        // emergent (kick vs self-gravity), not scripted — the whole point of docs/21.
        let mats = crate::materials::load();
        let basalt = crate::materials::index_of(&mats, "basalt");
        let mut agg = Aggregate::new(cloud(3, 100.0, 1.0e13), 50.0).with_material(basalt);
        let r0 = agg.rms_radius();
        let bind = agg.binding_energy();
        let site = agg.com(); // strike the centre

        agg.deposit_impact(&mats, site, DVec3::NEG_Y, 100.0 * bind);

        let hottest = agg.temps.iter().cloned().fold(0.0f32, f32::max);
        let coldest = agg.temps.iter().cloned().fold(f32::MAX, f32::min);
        assert!(hottest > REF_TEMP_K, "the impact deposits heat");
        assert!(
            hottest > coldest,
            "heating has a radial gradient (core {hottest} K hotter than rim {coldest} K)"
        );
        assert!(
            agg.kinetic_energy_com() > bind,
            "the deposit exceeds binding — it will unbind"
        );

        let mut acc = agg.accelerations();
        for _ in 0..400 {
            agg.step(&mut acc, 2.0);
        }
        assert!(
            agg.rms_radius() > 5.0 * r0,
            "it shatters and disperses (rms {:.1} vs r0 {:.1})",
            agg.rms_radius(),
            r0
        );
    }

    #[test]
    fn a_cohesive_solid_settles_to_a_ground_state_but_shatters_under_a_hard_impact() {
        // Robin's point: a deterministic model with real dissipation reaches a GROUND STATE. A struck
        // solid rings, then the bond damping bleeds the vibration away and it settles. A hard enough
        // blow instead fractures the bonds and it shatters — both emergent, no scripted settle/destroy.
        let mk = || Aggregate::cohesive(cloud(3, 1.0, 1.0), 0, 0.5, 1.5, 1.0e4, 1.0e2, 0.1);

        // Gentle strike: nudge one particle; the internal vibration damps to ~0 (ground state), and no
        // bond is over-stretched, so the solid stays whole.
        let mut solid = mk();
        let bonds0 = solid.active_bonds();
        assert!(bonds0 > 0, "the solid is bonded");
        solid.particles[0].vel = DVec3::new(2.0, 0.0, 0.0);
        let ke0 = solid.kinetic_energy_com();
        let mut acc = solid.accelerations();
        for _ in 0..3000 {
            solid.step(&mut acc, 5.0e-4);
        }
        assert!(
            solid.kinetic_energy_com() < 0.02 * ke0,
            "it settles to a ground state (internal KE {:.3e} → ~0 from {:.3e})",
            solid.kinetic_energy_com(),
            ke0
        );
        assert_eq!(
            solid.active_bonds(),
            bonds0,
            "a gentle strike breaks no bonds"
        );

        // Hard strike: a violent outward kick over-strains the bonds → they fracture → it shatters.
        let mut hit = mk();
        let r0 = hit.rms_radius();
        let com = hit.com();
        for p in &mut hit.particles {
            p.vel = (p.pos - com).normalize_or_zero() * 500.0;
        }
        let mut acc2 = hit.accelerations();
        for _ in 0..500 {
            hit.step(&mut acc2, 5.0e-4);
        }
        assert!(
            hit.active_bonds() < bonds0 / 2,
            "the impact fractures most bonds"
        );
        assert!(hit.rms_radius() > 3.0 * r0, "it shatters and disperses");
    }
}
