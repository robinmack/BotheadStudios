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
    /// Bulk material index (drives the contact law parameters; per-pair material contact is a flagged
    /// later refinement).
    pub material: usize,
    /// Per-particle material index — real COMPOSITION (docs/25): a layered planet's excavated cloud is
    /// crust rock + mantle rock + core iron, each particle knowing what matter it is (tint, thermal).
    pub mat_ids: Vec<usize>,
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
    /// Whether to compute the O(n²) N-body self-gravity. TRUE for a self-gravitating body (a rubble
    /// pile that holds itself together by gravity, `docs/21`). FALSE for a cohesive SOLID resting in an
    /// external field (the probe): its own gravity is utterly negligible at a few metres, and the O(n²)
    /// `powf(-1.5)` per substep dominates the frame — skipping it is honest (a real but ~0 effect) and
    /// turns the per-substep cost O(n²)→O(bonds).
    pub self_gravity: bool,
    /// Optional external POINT-mass gravity source (position, mass, softening radius) — e.g. the planet a
    /// debris cloud is falling back onto. Unlike `gravity` (a uniform field), this pulls each particle
    /// toward the source's actual centre with a real 1/r² law, so fragments launched above escape velocity
    /// genuinely escape and slower ones arc back — the honest fall-back/escape balance. Softened at the
    /// source radius so a fragment crossing the surface doesn't see a singularity (contact handles it).
    pub gravity_source: Option<(DVec3, f64, f64)>,
    /// ALL the system's massive bodies (position, mass, radius) — Sun, planet, other moons — each
    /// pulling every particle by the SAME law (1/r² outside its radius, Gauss interior inside). No body
    /// is a special case or a scene modifier: the Sun is declared MATTER (planet::sun(), its mass
    /// emerging from composition), and without its pull a planet's debris is dynamically wrong within
    /// days (Earth accelerates ~6 mm/s² around it — an 8 km/s drift over 16 days — and curves away from
    /// sun-blind debris: the disk visibly "escaped"). Refreshed each substep from the live N-body state.
    pub gravity_bodies: Vec<(DVec3, f64, f64)>,
    /// Particle–particle CONTACT law (the canonical `granular::Contact`) — when set, non-coincident
    /// particles that overlap push apart via `granular::contact_accel`, the SAME force law the granular
    /// terrain/debris uses. This is what makes matter collide instead of interpenetrate; without it a
    /// cloud is just ballistic points. Derived from the material via `granular::contact_from_material`.
    pub contact: Option<crate::granular::Contact>,
    /// The particle mass the `contact` parameters were derived for (`contact_from_material`'s
    /// `particle_mass`). The pair law's per-mass output × this = the pair FORCE; each particle then
    /// divides by ITS OWN mass — so unequal-mass contacts (a dense solid through light air parcels)
    /// conserve momentum exactly. Equal-mass aggregates are unchanged (F/m ≡ the old acceleration).
    pub contact_ref_mass: f64,
    /// Specific heat (J/(kg·K)) used to convert contact-dissipated energy into temperature rise — energy
    /// is conserved, not destroyed (docs/20): friction/damping heat the matter (→ incandescence).
    pub specific_heat: f64,
    /// A rigid CONVERVATIVE boundary sphere (centre, radius, per-mass stiffness) — e.g. the un-materialised
    /// bulk of a planet the debris rests on / rains back onto. A particle inside it feels an outward
    /// penalty spring `stiffness·(radius−dist)` (a FORCE, −∇U — never a velocity reset). The far-field
    /// summary of matter we don't resolve, contacted honestly (docs/24).
    pub boundary: Option<(DVec3, f64, f64)>,
    /// The boundary body's velocity — the ground the debris shears against moves with its planet.
    /// (No spin yet — the surface velocity omits rotation, flagged; docs/27 roadmap.)
    pub boundary_vel: DVec3,
    /// A spherical HOLE carved out of the boundary (centre, radius) — the excavated crater bowl. The
    /// solid region is (inside boundary sphere) MINUS (inside hole ball): a particle in the bowl is in
    /// free space; a particle in the remaining solid is pushed out through the NEAREST free surface
    /// (radially to the planet surface, or into the bowl through its wall). Without this the boundary
    /// was dishonestly a whole-planet excavation: debris landing far from the crater sank to cap depth.
    pub boundary_hole: Option<(DVec3, f64)>,
}

impl Aggregate {
    pub fn new(particles: Vec<Body>, softening: f64) -> Self {
        let n = particles.len();
        Aggregate {
            particles,
            temps: vec![REF_TEMP_K; n],
            material: 0,
            mat_ids: vec![0; n],
            softening,
            bonds: Vec::new(),
            stiffness: 0.0,
            damping: 0.0,
            break_strain: f64::INFINITY,
            gravity: DVec3::ZERO,
            self_gravity: true, // a bare aggregate is a self-gravitating pile
            gravity_source: None,
            gravity_bodies: Vec::new(),
            contact: None,
            contact_ref_mass: 1.0,
            specific_heat: 1000.0, // generic rock-ish default; set from the material via with_specific_heat
            boundary: None,
            boundary_vel: DVec3::ZERO,
            boundary_hole: None,
        }
    }

    /// Give the aggregate the canonical particle–particle contact law (built from a material). This is
    /// the one collision law the whole engine uses — see `granular::contact_from_material`.
    pub fn with_contact(mut self, contact: crate::granular::Contact, ref_mass: f64) -> Self {
        self.contact = Some(contact);
        self.contact_ref_mass = ref_mass.max(1.0e-30);
        self
    }

    /// Set the material's specific heat (J/(kg·K)) — converts contact dissipation into temperature.
    pub fn with_specific_heat(mut self, c: f64) -> Self {
        self.specific_heat = c.max(1.0);
        self
    }

    /// Rest the aggregate on / let it rain back onto a rigid boundary sphere (conservative penalty).
    pub fn with_boundary(mut self, center: DVec3, radius: f64, stiffness: f64) -> Self {
        self.boundary = Some((center, radius, stiffness));
        self
    }

    /// Move the boundary sphere's centre (the planet orbits while its debris settles).
    pub fn set_boundary_center(&mut self, center: DVec3) {
        if let Some(b) = self.boundary.as_mut() {
            b.0 = center;
        }
    }

    /// Carve a spherical hole (the crater bowl) out of the boundary solid.
    pub fn with_boundary_hole(mut self, center: DVec3, radius: f64) -> Self {
        self.boundary_hole = Some((center, radius));
        self
    }

    /// Move the hole with its planet (the impact site orbits too).
    pub fn set_boundary_hole_center(&mut self, center: DVec3) {
        if let Some(h) = self.boundary_hole.as_mut() {
            h.0 = center;
        }
    }

    /// Shrink (heal) the hole as settled matter refills it; radius 0 removes it entirely.
    pub fn set_boundary_hole_radius(&mut self, radius: f64) {
        if radius <= 0.0 {
            self.boundary_hole = None;
        } else if let Some(h) = self.boundary_hole.as_mut() {
            h.1 = radius;
        }
    }

    /// Set a uniform external gravity (e.g. a planet's surface field).
    pub fn with_gravity(mut self, gravity: DVec3) -> Self {
        self.gravity = gravity;
        self
    }

    /// Set an external extended-body gravity source (position, mass, PHYSICAL radius). 1/r² outside the
    /// radius, Gauss's-law linear interior inside it (see `accelerations`). Use this instead of
    /// `with_gravity` when the field varies over the cloud (e.g. debris flung far from a planet), so the
    /// escape/fall-back split is real. Update the position each frame with `set_gravity_source_pos`.
    pub fn with_gravity_source(mut self, pos: DVec3, mass: f64, body_radius: f64) -> Self {
        self.gravity_source = Some((pos, mass, body_radius));
        self
    }

    /// Update the moving source's position (the planet orbits while the debris falls back).
    pub fn set_gravity_source_pos(&mut self, pos: DVec3) {
        if let Some(src) = self.gravity_source.as_mut() {
            src.0 = pos;
        }
    }

    /// Refresh the system's massive bodies (position, mass, radius) from the live N-body state.
    pub fn set_gravity_bodies(&mut self, bodies: Vec<(DVec3, f64, f64)>) {
        self.gravity_bodies = bodies;
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
            mat_ids: vec![material; n],
            softening,
            bonds,
            stiffness,
            damping,
            break_strain,
            gravity: DVec3::ZERO,
            // A cohesive solid is held by its BONDS; its self-gravity is negligible. Skip the O(n²)
            // N-body loop — it would otherwise dominate the frame (the probe's ~135 substeps × n²).
            self_gravity: false,
            gravity_source: None,
            gravity_bodies: Vec::new(),
            contact: None,
            contact_ref_mass: 1.0,
            specific_heat: 1000.0, // generic rock-ish default; set from the material via with_specific_heat
            boundary: None,
            boundary_vel: DVec3::ZERO,
            boundary_hole: None,
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
        self.mat_ids = vec![material; self.particles.len()];
        self
    }

    /// Deposit impact `energy` (J) at `site` travelling along `dir` — the same physics as
    /// `matter::impact`, on a self-gravitating cloud instead of a voxel grid (`docs/21`). Energy density
    /// peaks at the contact and falls off, so each particle **heats** (temperature from `e/(ρc)`) and is
    /// **kicked** outward + along the impact; vaporized parcels (`damage::classify`) expand faster.
    /// Whether the aggregate then **survives or shatters is emergent** — it falls out of the kick vs the
    /// self-gravity that binds it (run `step` and watch `rms_radius`). Energy-conserving deposit
    /// (`Σ eᵢ·Vᵢ = energy`).
    /// Couple an impact into the aggregate **honestly** — the SAME physics as the terrain (`docs/24`),
    /// no scripted ejecta kick. The impactor delivers `momentum` (kg·m/s) and `energy` (J) at `site`:
    ///   1. **Momentum impulse** on the coupling core (particles within λ of the contact): the real
    ///      momentum, spread so `Σ mᵢ·Δv = momentum`. Only the near particles are shoved, so the core
    ///      tears away from the rest — spall/fracture emerges from the differential motion over-straining
    ///      bonds, not a magic speed.
    ///   2. **Shock heat** (the energy the impulse didn't turn into motion) with a radial gradient — core
    ///      hot (glows), rim cold.
    ///   3. **Vapor expansion**: matter heated past full vaporization flashes to gas and expands, throwing
    ///      ejecta radially (thermal → kinetic, conserved). This is the dominant destroyer at
    ///      hypervelocity — and it does nothing when the energy can't vaporize (an 890-t ball vs a pebble
    ///      just recoils and scars), which is the honest outcome.
    /// Whether the aggregate dents, spalls, or shatters then falls out of its own bond strengths.
    pub fn deposit_impact(&mut self, materials: &[Material], site: DVec3, momentum: DVec3, energy: f64) {
        if self.particles.is_empty() {
            return;
        }
        let mat = &materials[self.material];
        let density = (mat.density as f64).max(1.0);
        let c = mat.thermal.as_ref().map_or(1000.0, |t| t.specific_heat as f64);
        let vapor = crate::damage::vapor_energy_density(mat);
        // Coupling length ~ half the cloud's spread, so the impact concentrates near the contact.
        let lambda = (self.rms_radius() * 0.5).max(1.0);

        // 1. MOMENTUM impulse on the coupling core — the mechanical shock, momentum-conserving.
        let core: Vec<usize> = (0..self.particles.len())
            .filter(|&i| (self.particles[i].pos - site).length() <= lambda)
            .collect();
        let mut bulk_ke = 0.0;
        if !core.is_empty() {
            let m_total: f64 = core.iter().map(|&i| self.particles[i].mass.max(1.0e-6)).sum();
            let dv = momentum / m_total; // Σ mᵢ·Δv = momentum
            for &i in &core {
                self.particles[i].vel += dv;
            }
            bulk_ke = 0.5 * m_total * dv.length_squared();
        }

        // 2. SHOCK HEAT — the rest of the energy, radial gradient (Σ eᵢ·Vᵢ = heat, eᵢ = e0·exp(−dᵢ/λ)).
        let heat = (energy - bulk_ke).max(0.0);
        let wsum: f64 = self
            .particles
            .iter()
            .map(|p| (-(p.pos - site).length() / lambda).exp() * (p.mass / density))
            .sum();
        if wsum > 0.0 {
            let e0 = heat / wsum;
            for (p, temp) in self.particles.iter_mut().zip(self.temps.iter_mut()) {
                let e_i = e0 * (-(p.pos - site).length() / lambda).exp();
                *temp += (e_i / (density * c)) as f32;
            }
        }

        // 3. VAPOR EXPANSION — superheat past vaporization → radial ejecta KE (thermal → kinetic).
        if let Some(ev) = vapor {
            let mut e_expand = 0.0;
            for (p, temp) in self.particles.iter_mut().zip(self.temps.iter_mut()) {
                let e_th = density * c * (*temp as f64 - REF_TEMP_K as f64); // J/m³
                let excess = e_th - ev;
                if excess > 0.0 {
                    e_expand += excess * (p.mass / density); // × volume ⇒ J
                    *temp -= (excess / (density * c)) as f32; // adiabatic cooling of the vapor
                }
            }
            if e_expand > 0.0 {
                let shell_r = lambda * 0.25;
                let m_shell: f64 = self
                    .particles
                    .iter()
                    .filter(|p| (p.pos - site).length() > shell_r)
                    .map(|p| p.mass.max(1.0e-6))
                    .sum();
                if m_shell > 0.0 {
                    let v0 = (2.0 * e_expand / m_shell).sqrt(); // Σ½mv₀² over shell = E_expand
                    for p in &mut self.particles {
                        let radial = p.pos - site;
                        let r = radial.length();
                        if r > shell_r {
                            p.vel += (radial / r) * v0;
                        }
                    }
                }
            }
        }
    }

    /// Softened mutual-gravity acceleration on every particle (N-body).
    pub fn accelerations(&self) -> Vec<DVec3> {
        let s2 = self.softening * self.softening;
        let p = &self.particles;
        let mut acc = vec![self.gravity; p.len()]; // uniform external gravity (0 for a rubble pile)
        // External extended-body gravity source (e.g. the planet the debris falls back to). OUTSIDE the
        // body: real 1/r² toward its centre — escape vs. fall-back emerges. INSIDE it: Gauss's law — only
        // the mass interior to r pulls, so for a (uniform-density) planet g(r) = G·M·r/R³, decreasing
        // LINEARLY to zero at the centre. A point-mass 1/r² inside is WRONG physics: it grows without
        // bound inward and turns the core into an attractor that swallows anything that ploughs beneath
        // the surface. `body_r` is the source's physical radius (the crossover), doubling as the scale
        // that keeps the force finite everywhere.
        if let Some((src_pos, src_mass, body_r)) = self.gravity_source {
            let r3 = body_r * body_r * body_r;
            for (i, body) in p.iter().enumerate() {
                let d = src_pos - body.pos;
                let dist = d.length();
                if dist < 1.0e-9 {
                    continue;
                }
                let g_mag = if dist >= body_r {
                    G * src_mass / (dist * dist) // exterior: the full mass, 1/r²
                } else {
                    G * src_mass * dist / r3 // interior: Gauss — only the enclosed mass pulls
                };
                acc[i] += (d / dist) * g_mag;
            }
        }
                                                   // O(n²) N-body self-gravity — only for a self-gravitating pile. A cohesive solid skips it (its
                                                   // own gravity is ~0 and this `powf(-1.5)` loop would dominate the frame; see `self_gravity`).
        if self.self_gravity {
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
        }
        // Every declared massive body, one law each (see `gravity_bodies`) — the Sun keeps the
        // debris travelling WITH its planet; nothing is a scene modifier.
        for &(bp, bm, br) in &self.gravity_bodies {
            let r3 = (br * br * br).max(1.0);
            for (i, body) in p.iter().enumerate() {
                let d = bp - body.pos;
                let dist = d.length();
                if dist < 1.0e-9 {
                    continue;
                }
                let g_mag = if dist >= br { G * bm / (dist * dist) } else { G * bm * dist / r3 };
                acc[i] += (d / dist) * g_mag;
            }
        }
        // Particle–particle CONTACT — the canonical granular law (`contact_accel`), the SAME force that
        // governs the terrain/debris grains, now applied to any aggregate of matter. This is what stops
        // particles passing through each other; sticking, ploughing and cratering all emerge from it.
        // O(n²) — fine for the coarse clouds here; a neighbour grid is the scaling refinement.
        if let Some(c) = self.contact {
            let m_ref = self.contact_ref_mass;
            for i in 0..p.len() {
                for j in (i + 1)..p.len() {
                    // The law's per-mass output × the reference mass = the pair FORCE; each particle
                    // divides by its own mass — equal & opposite FORCES ⇒ momentum conserved for ANY
                    // mass ratio (a dense solid through light air parcels included).
                    let f = crate::granular::contact_accel(p[i].pos, p[i].vel, p[j].pos, p[j].vel, &c)
                        * m_ref;
                    acc[i] += f / p[i].mass.max(1.0e-30);
                    acc[j] -= f / p[j].mass.max(1.0e-30);
                }
            }
        }
        // Conservative boundary sphere (the un-materialised bulk planet): an outward penalty spring for any
        // particle that has pushed inside it. A FORCE (−∇U of ½k·penetration²), not a velocity reset — so
        // debris rests on it and rains back without the "cancel the inward component" fudge.
        if let Some((center, radius, stiffness)) = self.boundary {
            for (i, body) in p.iter().enumerate() {
                let d = body.pos - center;
                let dist = d.length();
                if dist >= radius || dist <= 1.0e-9 {
                    continue; // outside the planet (or degenerate)
                }
                // Inside the boundary sphere. If a crater bowl is carved and the particle is IN it,
                // it's in free space — no force. Otherwise it's in solid: push it out through the
                // NEAREST free surface — radially to the planet surface, or through the bowl wall.
                let pen_sphere = radius - dist; // depth below the planet surface
                // The boundary is MATTER: it contacts with the one canonical law — normal spring +
                // damping (incl. the shock closure) + Coulomb FRICTION — not a frictionless radial
                // push (an oblique impactor was skating over the planet with zero shear, Robin's
                // "flows over the surface"). Normal direction and penetration pick the nearest free
                // surface (planet surface or crater-bowl wall).
                let (mut n_hat, mut pen) = (d / dist, pen_sphere);
                let mut in_solid = true;
                if let Some((hc, hr)) = self.boundary_hole {
                    let dh = body.pos - hc;
                    let dist_h = dh.length();
                    if dist_h < hr {
                        in_solid = false; // in the excavated bowl: free space
                    } else {
                        let pen_hole = dist_h - hr;
                        if pen_hole < pen_sphere && dist_h > 1.0e-9 {
                            n_hat = -(dh / dist_h); // out through the bowl wall
                            pen = pen_hole;
                        }
                    }
                }
                if !in_solid {
                    continue;
                }
                let v_rel = body.vel - self.boundary_vel;
                let v_n = v_rel.dot(n_hat); // >0 exiting the solid
                let (c_damp, mu, tan_d, sh, cr) = self.contact.map_or((0.0, 0.0, 0.0, 0.0, 1.0), |c| {
                    (c.normal_damp, c.friction, c.tangent_damp, c.shock, c.radius)
                });
                let c_eff = c_damp + sh * v_n.abs() / (4.0 * cr.max(1.0e-9));
                let f_n = (stiffness * pen - c_eff * v_n).max(0.0);
                acc[i] += n_hat * f_n;
                // Coulomb shear against the moving ground: opposes slip, capped at μ·N.
                let v_t = v_rel - n_hat * v_n;
                let vt_mag = v_t.length();
                if vt_mag > 1.0e-9 && mu > 0.0 {
                    let f_t = (tan_d * vt_mag).min(mu * f_n);
                    acc[i] -= (v_t / vt_mag) * f_t;
                }
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
        // Target dt·ω ≤ 0.5 (a factor of 4 inside the Verlet limit of 2). The extra margin keeps the
        // velocity DAMPING term stable too, not just the spring — an overdamped stiff bond blows up
        // under explicit integration otherwise (the probe-detonation bug, docs/23).
        ((2.0 * dt * omega).ceil() as usize).max(1)
    }

    /// A coordination-corrected, **sub-critical** bond damping (N·s/m) that settles the solid without
    /// the explicit integrator going unstable. A particle with `z` bonds sees effective damping `z·c`;
    /// its critical damping is `2√(K·m) = 2√(z·k·m)`, so per bond `c_crit = 2√(k·m)/√z`. We use
    /// `ζ·c_crit` with ζ < 1. The `/√z` is the fix for the detonation bug: using `√(k·m)` directly
    /// (ignoring z) over-damped every particle ~√z× past critical, and overdamping a stiff spring
    /// explodes explicitly (`docs/23`).
    pub fn critically_damped(&self, zeta: f64) -> f64 {
        if self.bonds.is_empty() || self.particles.is_empty() {
            return 0.0;
        }
        let mean_mass =
            self.particles.iter().map(|b| b.mass).sum::<f64>() / self.particles.len() as f64;
        let z = (2 * self.bonds.len()) as f64 / self.particles.len() as f64;
        zeta * 2.0 * (self.stiffness * mean_mass).sqrt() / z.max(1.0).sqrt()
    }

    /// DEMOTION (docs/27, docs/13): matter that has landed on the boundary body and come to REST is
    /// that body again — remove it from the particle set and return its total mass so the caller can
    /// add it to the planet (mass conserved; its heat is dropped — flagged). The inverse of
    /// materialization: we stop simulating what has stopped happening. Criteria: within `r_tol` of the
    /// boundary surface AND slower than `speed_tol` relative to `v_ref` — an orbiting body is far away,
    /// a falling one is fast; only the settled pile qualifies.
    pub fn drain_settled(
        &mut self,
        center: DVec3,
        surface_r: f64,
        v_ref: DVec3,
        speed_tol: f64,
        r_tol: f64,
    ) -> (usize, f64) {
        let mut drained_mass = 0.0;
        let mut i = 0;
        let mut n = 0;
        while i < self.particles.len() {
            let p = self.particles[i];
            let settled = (p.pos - center).length() < surface_r + r_tol
                && (p.vel - v_ref).length() < speed_tol;
            if settled {
                drained_mass += p.mass;
                self.particles.swap_remove(i);
                self.temps.swap_remove(i);
                self.mat_ids.swap_remove(i);
                n += 1;
            } else {
                i += 1;
            }
        }
        (n, drained_mass)
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
        // Energy conservation (docs/20): the mechanical energy the contact damping/friction removed this
        // step becomes HEAT, split evenly across each dissipating pair — the source of the emergent
        // incandescence (a hard impact glows because the matter genuinely got hot).
        if let Some(c) = self.contact {
            let heat_per_k = self.specific_heat; // J/(kg·K)
            let m_ref = self.contact_ref_mass;
            let p = &self.particles;
            let mut d_temp = vec![0.0f64; p.len()];
            for i in 0..p.len() {
                for j in (i + 1)..p.len() {
                    // Specific power × ref mass = the pair's actual dissipated watts; split half-half,
                    // each side's temperature rise divides by ITS OWN heat capacity (m·c).
                    let pw = crate::granular::contact_dissipation(
                        p[i].pos, p[i].vel, p[j].pos, p[j].vel, &c,
                    ) * m_ref;
                    if pw > 0.0 {
                        let e_half = 0.5 * pw * dt;
                        d_temp[i] += e_half / (p[i].mass.max(1.0e-30) * heat_per_k);
                        d_temp[j] += e_half / (p[j].mass.max(1.0e-30) * heat_per_k);
                    }
                }
            }
            for (t, d) in self.temps.iter_mut().zip(d_temp.iter()) {
                *t += *d as f32;
            }
        }
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

    /// A solid ball of lattice particles within `radius` (the shape of the probe).
    fn ball(radius: f64, mass: f64) -> Vec<Body> {
        let ri = radius.ceil() as i32;
        let mut v = Vec::new();
        for x in -ri..=ri {
            for y in -ri..=ri {
                for z in -ri..=ri {
                    let off = DVec3::new(x as f64, y as f64, z as f64);
                    if off.length() <= radius {
                        v.push(Body {
                            pos: off,
                            vel: DVec3::ZERO,
                            mass,
                        });
                    }
                }
            }
        }
        v
    }

    #[test]
    fn a_stiff_cohesive_ball_does_not_spontaneously_explode() {
        // Reproduces the probe: a bonded iron ball whose stiffness derives from Young's modulus,
        // damped + substepped by the app's own rules. At rest with no external force it must STAY
        // intact — an unstable integrator makes it fly apart ("spontaneously detonate, leaving only
        // the core"). Before the coordination-corrected damping fix, this exploded (docs/23).
        let density = 7870.0;
        let stiffness = (2.05e11_f64 * 1.0).min(5.0e9); // E·L capped, as build_probe does
        let mut agg = Aggregate::cohesive(ball(2.0, density), 0, 0.5, 1.75, stiffness, 0.0, 0.06);
        agg.damping = agg.critically_damped(0.4);

        let n0 = agg.particles.len();
        let r0 = agg.rms_radius();
        let bonds0 = agg.bonds.len();
        let mut acc = agg.accelerations();
        let frame = 1.0 / 60.0;
        for _ in 0..40 {
            // 2/3 s — an unstable integrator blows up exponentially within a few frames
            let sub = agg.stable_substeps(frame).clamp(1, 256);
            let pdt = frame / sub as f64;
            for _ in 0..sub {
                agg.step(&mut acc, pdt);
            }
        }
        assert_eq!(agg.particles.len(), n0, "no particles lost");
        assert!(
            agg.active_bonds() as f64 / bonds0 as f64 > 0.99,
            "no spurious bond fractures (active {}/{})",
            agg.active_bonds(),
            bonds0
        );
        assert!(
            agg.rms_radius() < 1.05 * r0,
            "stays compact — does not detonate (rms {:.3} vs r0 {:.3})",
            agg.rms_radius(),
            r0
        );
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

        // Momentum sized so the core's mechanical KE ≈ 10× binding (it unbinds) while most of the energy
        // still lands as heat (so the radial gradient shows). Honest coupling — the same momentum + heat
        // + vapor pipeline as the terrain; the shatter is emergent (core tears loose from self-gravity).
        let lambda = agg.rms_radius() * 0.5;
        let m_core: f64 = agg
            .particles
            .iter()
            .filter(|p| (p.pos - site).length() <= lambda)
            .map(|p| p.mass)
            .sum();
        let p_mag = (20.0 * m_core * bind).sqrt(); // ½·p²/m_core = 10·bind
        agg.deposit_impact(&mats, site, DVec3::NEG_Y * p_mag, 100.0 * bind);

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

    #[test]
    fn point_source_gravity_splits_escape_from_fallback() {
        // The escape/fall-back boundary is not a tuned parameter — it is DECLARED by the source mass and G.
        // A fragment launched above the real escape velocity must leave; below it, it must arc back. We
        // read the threshold straight from the physics the model already declares (faithfulness, not eye).
        let r0 = 6.371e6_f64; // Earth radius (m)
        let m = 5.972e24_f64; // Earth mass (kg)
        let v_esc = (2.0 * G * m / r0).sqrt(); // ≈ 11.2 km/s, from the declared M and G — nothing tuned

        // A single free fragment (no self-gravity, no bonds) launched radially outward from the surface.
        let max_radius = |v: f64| -> f64 {
            let mut agg = Aggregate::new(
                vec![Body {
                    pos: DVec3::new(r0, 0.0, 0.0),
                    vel: DVec3::new(v, 0.0, 0.0),
                    mass: 1.0,
                }],
                1.0,
            )
            .with_gravity_source(DVec3::ZERO, m, r0); // 1/r² outside the planet; Gauss interior inside
            agg.self_gravity = false;
            let mut acc = agg.accelerations();
            let mut rmax = r0;
            for _ in 0..40_000 {
                agg.step(&mut acc, 2.0); // ~22 h of flight — long enough to fall back or clearly escape
                // Surface contact — exactly as the render does it: a fragment can't sink below the surface,
                // so a bound one arcs back UP to apoapsis and returns to rest on the ground (it never falls
                // through the singular core). This is the faithful setup; the split is read from it.
                let p = &mut agg.particles[0];
                let r = p.pos.length();
                if r < r0 {
                    let n = p.pos / r;
                    p.pos = n * r0;
                    let vn = p.vel.dot(n);
                    if vn < 0.0 {
                        p.vel -= n * vn;
                    }
                }
                rmax = rmax.max(agg.particles[0].pos.length());
            }
            rmax
        };

        // 1.4× escape → gone (climbs past many Earth radii and keeps going).
        assert!(
            max_radius(1.4 * v_esc) > 10.0 * r0,
            "above escape velocity the fragment leaves for good"
        );
        // 0.6× escape → bound: analytic apoapsis r0/(1−f²) = r0/0.64 ≈ 1.56 r0 (softening shifts it a hair).
        let bound = max_radius(0.6 * v_esc);
        assert!(
            bound < 2.0 * r0,
            "below escape velocity it arcs back — apoapsis stays near the surface, got {bound:.3e} m"
        );
    }

    #[test]
    fn settled_matter_demotes_to_the_planet_but_orbiting_matter_does_not() {
        let r0 = 6.371e6;
        let mut agg = Aggregate::new(
            vec![
                // resting on the surface, co-moving: settled ⇒ drained
                Body { pos: DVec3::new(0.0, r0 + 1.0e3, 0.0), vel: DVec3::ZERO, mass: 5.0 },
                // aloft: stays
                Body { pos: DVec3::new(0.0, 3.0 * r0, 0.0), vel: DVec3::new(4.0e3, 0.0, 0.0), mass: 7.0 },
                // near the surface but FAST (falling through): stays
                Body { pos: DVec3::new(r0 + 1.0e3, 0.0, 0.0), vel: DVec3::new(-2.0e3, 0.0, 0.0), mass: 3.0 },
            ],
            1.0,
        );
        agg.self_gravity = false;
        let (n, m) = agg.drain_settled(DVec3::ZERO, r0, DVec3::ZERO, 30.0, 1.0e6);
        assert_eq!(n, 1, "only the settled particle drains");
        assert!((m - 5.0).abs() < 1e-9, "its mass returns to the planet");
        assert_eq!(agg.particles.len(), 2, "orbiting + falling matter keeps simulating");
    }

    #[test]
    fn interior_gravity_follows_gauss_law_not_a_point_singularity() {
        // Inside a planet only the enclosed mass pulls (Gauss): g(r) = GM·r/R³, linear to ZERO at the
        // centre. The point-mass 1/r² inside was wrong physics — it made the core an attractor that
        // swallowed any debris that ploughed beneath the surface ("the balls absorb into the centre").
        let (m, r0) = (5.972e24_f64, 6.371e6_f64);
        let g_at = |r: f64| -> f64 {
            let mut agg = Aggregate::new(
                vec![Body { pos: DVec3::new(r, 0.0, 0.0), vel: DVec3::ZERO, mass: 1.0 }],
                1.0,
            )
            .with_gravity_source(DVec3::ZERO, m, r0);
            agg.self_gravity = false;
            agg.accelerations()[0].length()
        };
        let g_surface = G * m / (r0 * r0); // ≈ 9.82 m/s²
        assert!((g_at(r0) - g_surface).abs() / g_surface < 1e-6, "surface: full 1/r²");
        assert!((g_at(0.5 * r0) - 0.5 * g_surface).abs() / g_surface < 1e-6, "half depth: HALF g, not 4×");
        assert!(g_at(1.0e3) < 1.0e-2, "the centre pulls ~nothing — no singular attractor");
        assert!((g_at(2.0 * r0) - 0.25 * g_surface).abs() / g_surface < 1e-6, "exterior unchanged: 1/r²");
    }

    #[test]
    fn the_boundary_is_the_planet_minus_the_crater_bowl_not_a_global_excavation() {
        // Robin saw matter exit the far side of the planet: the old boundary sat at cap depth GLOBALLY,
        // as if the whole planet were excavated. The solid is (sphere R) minus (crater ball): debris far
        // from the crater rests at the SURFACE; inside the bowl is free space; nothing crosses the planet.
        let r0 = 6.371e6_f64;
        let hole_r = 3.474e6_f64;
        let site = DVec3::new(0.0, r0, 0.0); // crater at the north pole
        let probe = |pos: DVec3| -> DVec3 {
            let mut agg = Aggregate::new(
                vec![Body { pos, vel: DVec3::ZERO, mass: 1.0 }],
                1.0,
            )
            .with_boundary(DVec3::ZERO, r0, 1.0)
            .with_boundary_hole(site, hole_r);
            agg.self_gravity = false;
            agg.accelerations()[0]
        };
        // Far side, just under the surface: pushed OUT radially (the planet is solid there).
        let a = probe(DVec3::new(0.0, -(r0 - 1.0e5), 0.0));
        assert!(a.y < -1.0e4, "buried on the far side ⇒ pushed back out through the surface");
        // Inside the crater bowl: free space, no boundary force.
        let a = probe(site - DVec3::new(0.0, 0.5 * hole_r, 0.0));
        assert!(a.length() < 1.0e-9, "inside the excavated bowl ⇒ no force");
        // In solid just beyond the bowl wall: pushed INTO the bowl (the nearest free surface).
        let below_wall = site - DVec3::new(0.0, hole_r + 2.0e5, 0.0);
        let a = probe(below_wall);
        assert!(a.y > 1.0e4, "just under the bowl floor ⇒ pushed up into the bowl");
    }

    #[test]
    fn aggregate_particles_collide_via_the_canonical_law_and_conserve_momentum() {
        // The whole point: an aggregate of matter now COLLIDES through the same `granular::contact_accel`
        // law as everything else. Two equal-mass particles thrown head-on must push apart (not pass
        // through) with momentum conserved — the missing physics behind the "exploding sphere in a vacuum".
        let r = 1.0;
        let contact = crate::granular::Contact {
            radius: r,
            stiffness: 1.0e3,
            normal_damp: 0.0, // elastic ⇒ momentum AND (mechanical) energy conserved
            friction: 0.0,
            tangent_damp: 0.0,
            cohesion: 0.0,
            coh_range: 0.1,
            shock: 0.0,
        };
        let mut agg = Aggregate::new(
            vec![
                Body { pos: DVec3::new(-1.5, 0.0, 0.0), vel: DVec3::new(2.0, 0.0, 0.0), mass: 1.0 },
                Body { pos: DVec3::new(1.5, 0.0, 0.0), vel: DVec3::new(-2.0, 0.0, 0.0), mass: 1.0 },
            ],
            0.1,
        )
        .with_contact(contact, 1.0);
        agg.self_gravity = false;

        let p0: DVec3 = agg.particles.iter().map(|b| b.vel * b.mass).sum();
        let mut acc = agg.accelerations();
        let mut min_sep = f64::MAX;
        for _ in 0..2000 {
            agg.step(&mut acc, 1.0e-3);
            min_sep = min_sep.min((agg.particles[0].pos - agg.particles[1].pos).length());
        }
        let p1: DVec3 = agg.particles.iter().map(|b| b.vel * b.mass).sum();

        let final_sep = (agg.particles[0].pos - agg.particles[1].pos).length();
        assert!(min_sep < 2.0 * r, "they actually made contact (min_sep {min_sep:.3})");
        assert!(min_sep > 0.5 * r, "they did NOT pass through each other (min_sep {min_sep:.3})");
        assert!((p1 - p0).length() < 1.0e-9, "momentum conserved through the collision");
        assert!(final_sep > 2.0 * r, "they rebounded and separated");
    }
}
