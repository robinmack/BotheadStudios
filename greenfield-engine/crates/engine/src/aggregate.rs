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
    /// PER-GRAIN contact law (docs/23: iron collides as iron, not bulk basalt). When non-empty, a solid
    /// pair `(i, j)` collides via `contact[i].mix(&contact[j])` instead of the single bulk `contact` — each
    /// grain brings its own material's stiffness/restitution/friction (from its `mat_ids` entry). Empty ⇒
    /// the single-material path (unchanged). Same-material pairs are byte-identical (`mix` is idempotent),
    /// so only genuinely cross-material contacts differ. Built at ref mass `contact_ref_mass` like `contact`.
    pub per_grain_contact: Vec<crate::granular::Contact>,
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
    /// Sim-seconds since radiative cooling was last applied: per-substep decrements (~1e-4 K) underflow
    /// f32 at thousands of kelvin, so cooling is applied in batched, resolvable intervals.
    pub cool_elapsed: f64,
    /// The NET boundary force (N) and its torque about the boundary centre (N·m) from the latest
    /// `accelerations()` pass — measured DIRECTLY at the point of application, so the planet's
    /// reaction (linear and SPIN) is Newton's-third-law-exact without differencing cloud momenta
    /// about a moving reference (which FABRICATES angular momentum — measured: a 0.9-h day from a
    /// 4.6e34 impactor, 4× more L than exists).
    pub boundary_force_sum: DVec3,
    pub boundary_torque_sum: DVec3,
    /// Per-particle VAPOR flag (docs/26+27): matter hotter than its boiling point is GAS — it
    /// interacts through `contact_gas` (EOS pressure, no cohesion, shock closure) instead of the solid
    /// law. This is the vapor phase of the proto-lunar disk: pressure support spreads material outward
    /// past the Roche limit, where the Moon can accrete. Condenses back on radiative cooling.
    pub vapor: Vec<bool>,
    /// The gas-phase pair law (see `atmosphere::gas_contact_from_material`).
    pub contact_gas: Option<crate::granular::Contact>,
    /// Boil threshold (K) for the phase flip — the bulk material's boiling point (1-atm value;
    /// the local-vapor-pressure criterion is the refinement, flagged).
    pub boil_k: f64,
    /// SPH VAPOR PRESSURE (docs/26/27, replacing the docs/28-item-5 overlap hack): the specific gas
    /// constant R_s and kernel smoothing length h for a REAL continuum pressure field among vaporized
    /// parcels — P = ρ·R_s·T with each parcel's OWN temperature. When both > 0, vapor↔vapor pairs interact
    /// by a momentum-conserving SPH pressure that does PdV expansion work (launching the plume) and, in
    /// `step`, cools as it expands (energy-conserving). 0 ⇒ fall back to the old `contact_gas` overlap law.
    pub vapor_rs: f64,
    pub vapor_h: f64,
    /// LATENT-HEAT offset L_v/c (K): the energy absorbed by the phase change itself, which is stored in
    /// the parcel's tracked `temps` (it must reach boil + L_v/c to flag vapor) but is NOT thermal motion —
    /// so the pressure-driving temperature is `temps − vapor_latent_k` (docs/28: latent heat has a
    /// reservoir, not fake sensible T). Without this the vapor pressure over-reads by ~L_v/c ≈ 7,000 K.
    pub vapor_latent_k: f64,
    /// Cached SPH density at each vaporized parcel (from `accelerations`), reused by `step`'s PdV cooling.
    pub vapor_rho: Vec<f64>,
    /// Per-particle PROVENANCE (docs/28): which body this matter came from — [`SOURCE_IMPACTOR`] (Theia)
    /// or [`SOURCE_TARGET`] (Earth). A physical attribute, not an index convention: it rides `swap_remove`
    /// and lets the disk's composition be measured and tinted by origin (the real Moon is Earth-like, so
    /// "the disk is 100% impactor" is a bug the tag makes visible).
    pub source: Vec<u8>,
}

/// Provenance tags for [`Aggregate::source`]. The impactor is the default (0) so a bare aggregate with
/// no impact history reads as one uniform body.
pub const SOURCE_IMPACTOR: u8 = 0;
pub const SOURCE_TARGET: u8 = 1;

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
            per_grain_contact: Vec::new(),
            specific_heat: 1000.0, // generic rock-ish default; set from the material via with_specific_heat
            boundary: None,
            boundary_vel: DVec3::ZERO,
            boundary_hole: None,
            cool_elapsed: 0.0,
            boundary_force_sum: DVec3::ZERO,
            boundary_torque_sum: DVec3::ZERO,
            vapor: vec![false; n],
            contact_gas: None,
            boil_k: f64::INFINITY,
            vapor_rs: 0.0,
            vapor_h: 0.0,
            vapor_latent_k: 0.0,
            vapor_rho: Vec::new(),
            source: vec![SOURCE_IMPACTOR; n],
        }
    }

    /// Give the aggregate the canonical particle–particle contact law (built from a material). This is
    /// the one collision law the whole engine uses — see `granular::contact_from_material`.
    pub fn with_contact(mut self, contact: crate::granular::Contact, ref_mass: f64) -> Self {
        self.contact = Some(contact);
        self.contact_ref_mass = ref_mass.max(1.0e-30);
        self
    }

    /// Enable the VAPOR phase: pairs with a vaporized member use the gas law; particles flip phase at
    /// `boil_k` (hotter ⇒ gas, cooler ⇒ condensed back to the solid law).
    pub fn with_vapor_phase(mut self, gas: crate::granular::Contact, boil_k: f64) -> Self {
        self.contact_gas = Some(gas);
        self.boil_k = boil_k;
        self
    }

    /// Enable the REAL SPH vapor pressure field (docs/26/27): `rs` = the vapor's specific gas constant
    /// (`atmosphere::specific_gas_constant`), `h` = the kernel smoothing length (≈ a few vapor-parcel
    /// spacings). Vapor↔vapor pairs then use a continuum P = ρ·R_s·T pressure instead of the overlap hack.
    pub fn with_vapor_sph(mut self, rs: f64, h: f64, latent_k: f64) -> Self {
        self.vapor_rs = rs;
        self.vapor_h = h;
        self.vapor_latent_k = latent_k.max(0.0);
        self
    }

    /// Cell size for the short-range neighbour grid (docs/30): the LONGEST reach of any short-range force —
    /// the SPH kernel `h` and the widest contact interaction (touch + cohesion range) across all grains —
    /// so the grid misses no interacting pair (a missed pair would silently drop a force and break
    /// conservation). Long-range self-gravity is not short-range and uses its own scheme (Barnes–Hut).
    fn short_range_cell(&self) -> f64 {
        let mut reach = self.vapor_h;
        if let Some(c) = self.contact {
            reach = reach.max(2.0 * c.radius + c.coh_range);
        }
        for pc in &self.per_grain_contact {
            reach = reach.max(2.0 * pc.radius + pc.coh_range);
        }
        reach.max(1.0e-9)
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
            per_grain_contact: Vec::new(),
            specific_heat: 1000.0, // generic rock-ish default; set from the material via with_specific_heat
            boundary: None,
            boundary_vel: DVec3::ZERO,
            boundary_hole: None,
            cool_elapsed: 0.0,
            boundary_force_sum: DVec3::ZERO,
            boundary_torque_sum: DVec3::ZERO,
            vapor: vec![false; n],
            contact_gas: None,
            boil_k: f64::INFINITY,
            vapor_rs: 0.0,
            vapor_h: 0.0,
            vapor_latent_k: 0.0,
            vapor_rho: Vec::new(),
            source: vec![SOURCE_IMPACTOR; n],
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
    pub fn accelerations(&mut self) -> Vec<DVec3> {
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
        // Positions + masses, materialised ONCE and shared by BOTH accelerated schemes: Barnes–Hut for the
        // long-range self-gravity here, and the neighbour grid for the short-range contact/SPH below
        // (docs/30). Positions are fixed within one accelerations pass.
        let sr_pos: Vec<DVec3> = p.iter().map(|b| b.pos).collect();
        let masses: Vec<f64> = p.iter().map(|b| b.mass).collect();
        // N-body SELF-GRAVITY via Barnes–Hut → O(N log N) (docs/30 stage 1c): a distant clump pulls like a
        // single mass at its centre of mass. θ=0.5 (RMS error <1%, unbiased — below the FP/chaos noise the
        // disk tolerates), softened exactly like the direct sum; brute-force below ~1k bodies. Only for a
        // self-gravitating pile — a cohesive solid's own gravity is ~0 and skips it.
        if self.self_gravity {
            let bh = crate::bhtree::BarnesHut::build(&sr_pos, &masses, 0.5, self.softening);
            for (a, g) in acc.iter_mut().zip(bh.accelerations(&sr_pos, &masses)) {
                *a += g;
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
        // SHORT-RANGE neighbour grid (docs/30 stage 1b): O(N) pair-finding for contact + SPH, replacing the
        // O(N²) double loops. Built ONCE at the widest reach (reusing `sr_pos` from above) and reused by the
        // SPH block below. It yields exactly the pairs a brute sweep would (plus a few just-outside
        // candidates the force laws zero out), so the forces — and conservation — are identical
        // (verified: contact_grid_matches_brute_force).
        let sr_grid = crate::neighbors::NeighborGrid::build(&sr_pos, self.short_range_cell());
        if let Some(c) = self.contact {
            let m_ref = self.contact_ref_mass;
            let sph_vapor = self.vapor_rs > 0.0 && self.vapor_h > 0.0;
            sr_grid.for_each_pair(&sr_pos, |i, j| {
                // Phase-appropriate law per PAIR. With the real SPH vapor field on, a vapor↔vapor pair is
                // handled by the continuum pressure below — SKIP it here (no double force). A vapor↔solid
                // pair still contacts (the plume pushes the condensed debris at the interface). Without SPH,
                // fall back to the legacy overlap gas law. Otherwise a solid pair collides via each grain's
                // own material mixed (iron-as-iron, docs/23).
                let (vi, vj) = (self.vapor.get(i) == Some(&true), self.vapor.get(j) == Some(&true));
                if sph_vapor && vi && vj {
                    return;
                }
                let mixed_law;
                let law = if !sph_vapor && self.contact_gas.is_some() && (vi || vj) {
                    self.contact_gas.as_ref().unwrap()
                } else if !self.per_grain_contact.is_empty() {
                    mixed_law = self.per_grain_contact[i].mix(&self.per_grain_contact[j]);
                    &mixed_law
                } else {
                    &c
                };
                // The law's per-mass output × the reference mass = the pair FORCE; each particle divides by
                // its own mass — equal & opposite FORCES ⇒ momentum conserved for ANY mass ratio.
                let f = crate::granular::contact_accel(p[i].pos, p[i].vel, p[j].pos, p[j].vel, law) * m_ref;
                acc[i] += f / p[i].mass.max(1.0e-30);
                acc[j] -= f / p[j].mass.max(1.0e-30);
            });
        }
        // Conservative boundary sphere (the un-materialised bulk planet): an outward penalty spring for any
        // particle that has pushed inside it. A FORCE (−∇U of ½k·penetration²), not a velocity reset — so
        // debris rests on it and rains back without the "cancel the inward component" fudge.
        self.boundary_force_sum = DVec3::ZERO;
        self.boundary_torque_sum = DVec3::ZERO;
        if let Some((center, radius, stiffness)) = self.boundary {
            let mut f_sum = DVec3::ZERO;
            let mut tq_sum = DVec3::ZERO;
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
                let mut f_particle = DVec3::ZERO; // per-mass accel from the boundary, this particle
                let v_rel = body.vel - self.boundary_vel;
                let v_n = v_rel.dot(n_hat); // >0 exiting the solid
                let (c_damp, mu, tan_d, sh, cr) = self.contact.map_or((0.0, 0.0, 0.0, 0.0, 1.0), |c| {
                    (c.normal_damp, c.friction, c.tangent_damp, c.shock, c.radius)
                });
                let c_eff = c_damp + sh * v_n.abs() / (4.0 * cr.max(1.0e-9));
                let f_n = (stiffness * pen - c_eff * v_n).max(0.0);
                acc[i] += n_hat * f_n;
                f_particle += n_hat * f_n;
                // Coulomb shear against the moving ground: opposes slip, capped at μ·N.
                let v_t = v_rel - n_hat * v_n;
                let vt_mag = v_t.length();
                if vt_mag > 1.0e-9 && mu > 0.0 {
                    let f_t = (tan_d * vt_mag).min(mu * f_n);
                    acc[i] -= (v_t / vt_mag) * f_t;
                    f_particle -= (v_t / vt_mag) * f_t;
                }
                let force = f_particle * body.mass; // per-mass → newtons
                f_sum += force;
                tq_sum += (body.pos - center).cross(force);
            }
            self.boundary_force_sum = f_sum;
            self.boundary_torque_sum = tq_sum;
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
        // SPH VAPOR PRESSURE (docs/26/27): a real continuum pressure among vaporized parcels — P = ρ·R_s·T
        // with each parcel's OWN temperature — replacing the docs/28-item-5 overlap hack. Symmetric ⇒
        // momentum-conserving; this is the force that does the PdV expansion work launching the plume (and
        // cools it, in `step`). Density is a kernel estimate; the cached ρ feeds `step`'s energy update.
        if self.vapor_rs > 0.0 && self.vapor_h > 0.0 {
            let h = self.vapor_h;
            let n = self.particles.len();
            let mut rho = vec![0.0f64; n];
            // Density: self-contribution, then symmetric neighbour sums over the grid (vapor↔vapor within h).
            for i in 0..n {
                if self.vapor.get(i) == Some(&true) {
                    rho[i] = (self.particles[i].mass * crate::atmosphere::sph_w(0.0, h)).max(1.0e-30);
                }
            }
            sr_grid.for_each_pair(&sr_pos, |i, j| {
                if self.vapor.get(i) != Some(&true) || self.vapor.get(j) != Some(&true) {
                    return;
                }
                let r = (self.particles[i].pos - self.particles[j].pos).length();
                if r < h {
                    let w = crate::atmosphere::sph_w(r, h);
                    rho[i] += self.particles[j].mass * w;
                    rho[j] += self.particles[i].mass * w;
                }
            });
            // Symmetric pressure force over the same grid pairs.
            sr_grid.for_each_pair(&sr_pos, |i, j| {
                if self.vapor.get(i) != Some(&true) || self.vapor.get(j) != Some(&true) {
                    return;
                }
                let dv = self.particles[i].pos - self.particles[j].pos;
                let r = dv.length();
                if r >= h || r < 1.0e-9 {
                    return;
                }
                // Pressure-driving temperature EXCLUDES the latent heat (docs/28): P = ρ·R_s·(T − L_v/c).
                let ti = (self.temps[i] as f64 - self.vapor_latent_k).max(1.0);
                let tj = (self.temps[j] as f64 - self.vapor_latent_k).max(1.0);
                let pi = rho[i] * self.vapor_rs * ti;
                let pj = rho[j] * self.vapor_rs * tj;
                let term = pi / (rho[i] * rho[i]) + pj / (rho[j] * rho[j]);
                let grad = (dv / r) * crate::atmosphere::sph_dw(r, h); // dW<0 ⇒ repulsive
                acc[i] += grad * (-term * self.particles[j].mass);
                acc[j] += grad * (term * self.particles[i].mass);
            });
            self.vapor_rho = rho;
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

    /// PER-PARTICLE timestep (docs/30 stage 3, the foundation of block/individual timesteps): each
    /// particle's own stable step from its DYNAMICAL time at the current acceleration `acc[i]` — the
    /// free-fall time √(ε/|a|) (ε = the gravity softening, the shortest resolved length), tightened by the
    /// turnaround time |v|/|a| where a particle is being violently accelerated (a fresh contact). `eta` is
    /// the accuracy factor (~0.03–0.05). The violent shocked core comes out with a TINY dt (must be updated
    /// often); the quiescent disk with a LARGE dt (can coast) — the split a block scheduler buckets by, so
    /// per-step cost drops from O(N) to O(N_active). The scheduler that consumes these (a hierarchical,
    /// conservation-checked leapfrog with prediction) is the next, larger piece — this criterion is its
    /// small, testable foundation, and it changes NOTHING until that scheduler is wired in.
    pub fn particle_timesteps(&self, acc: &[DVec3], eta: f64) -> Vec<f64> {
        let eps = self.softening.max(1.0e-9);
        self.particles
            .iter()
            .zip(acc)
            .map(|(p, a)| {
                let amag = a.length();
                if amag <= 1.0e-30 {
                    return f64::INFINITY; // unaccelerated ⇒ no constraint (a lone drifting body)
                }
                let t_ff = (eps / amag).sqrt(); // free-fall / dynamical time
                let t_coll = p.vel.length() / amag; // turnaround time (velocity / accel)
                eta * if t_coll > 0.0 { t_ff.min(t_coll) } else { t_ff }
            })
            .collect()
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
    ) -> (usize, f64, DVec3) {
        let mut drained_mass = 0.0;
        let mut drained_l = DVec3::ZERO; // angular momentum about the planet — it becomes SPIN
        let mut i = 0;
        let mut n = 0;
        while i < self.particles.len() {
            let p = self.particles[i];
            let settled = (p.pos - center).length() < surface_r + r_tol
                && (p.vel - v_ref).length() < speed_tol;
            if settled {
                drained_mass += p.mass;
                drained_l += (p.pos - center).cross((p.vel - v_ref) * p.mass);
                self.particles.swap_remove(i);
                self.temps.swap_remove(i);
                self.mat_ids.swap_remove(i);
                if !self.vapor.is_empty() {
                    self.vapor.swap_remove(i);
                }
                if !self.source.is_empty() {
                    self.source.swap_remove(i);
                }
                n += 1;
            } else {
                i += 1;
            }
        }
        (n, drained_mass, drained_l)
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
        // One short-range neighbour grid for step()'s vapor PdV + contact-dissipation passes (docs/30):
        // O(N) not O(N²), built at the post-step positions and shared by both.
        let step_pos: Vec<DVec3> = self.particles.iter().map(|b| b.pos).collect();
        let step_grid = crate::neighbors::NeighborGrid::build(&step_pos, self.short_range_cell());
        // PdV EXPANSION WORK (docs/26/27): as the vapor plume expands, its pressure does work on the flow —
        // internal heat converts to bulk kinetic energy (the launch) and the gas COOLS. The SPH energy
        // equation du_i/dt = (P_i/ρ_i²)·Σ_j m_j (v_i−v_j)·∇_i W is the exact conservative partner of the
        // pressure force in `accelerations`: the KE it injects is paid for out of temperature, so total
        // energy is conserved and the 80,000 K trapped heat becomes expansion instead of just sitting.
        if self.vapor_rs > 0.0 && self.vapor_h > 0.0 && !self.vapor_rho.is_empty() {
            let h = self.vapor_h;
            let mut du = vec![0.0f64; self.particles.len()];
            step_grid.for_each_pair(&step_pos, |i, j| {
                if self.vapor.get(i) != Some(&true) || self.vapor.get(j) != Some(&true) {
                    return;
                }
                let dv = self.particles[i].pos - self.particles[j].pos;
                let r = dv.length();
                if r >= h || r < 1.0e-9 {
                    return;
                }
                let (ri, rj) = (self.vapor_rho[i], self.vapor_rho[j]);
                if ri <= 0.0 || rj <= 0.0 {
                    return;
                }
                let ti = (self.temps[i] as f64 - self.vapor_latent_k).max(1.0); // latent-excluded (docs/28)
                let tj = (self.temps[j] as f64 - self.vapor_latent_k).max(1.0);
                let pi = ri * self.vapor_rs * ti;
                let pj = rj * self.vapor_rs * tj;
                let grad = (dv / r) * crate::atmosphere::sph_dw(r, h);
                let dwv = (self.particles[i].vel - self.particles[j].vel).dot(grad); // (v_i−v_j)·∇_iW
                du[i] += (pi / (ri * ri)) * self.particles[j].mass * dwv;
                du[j] += (pj / (rj * rj)) * self.particles[i].mass * dwv;
            });
            let c = self.specific_heat.max(1.0);
            for i in 0..self.particles.len() {
                if self.vapor.get(i) == Some(&true) {
                    self.temps[i] = ((self.temps[i] as f64) + du[i] * dt / c).max(2.7) as f32;
                }
            }
        }
        // RADIATIVE COOLING (Stefan–Boltzmann): hot matter in vacuum sheds σ·ε·A·T⁴ — a white-hot
        // fragment visibly fades over hours-days (Robin: "the white-hot fragments never seem to cool" —
        // they had no radiative channel at all). Grain surface area from the contact radius (the
        // mass-agnostic geometry, flagged); rock emissivity ~0.9; space is ~0 K.
        if let Some(c) = self.contact {
            // Batched so the decrement is resolvable in f32 (a per-substep ~1e-4 K rounds away at
            // thousands of kelvin — measured: ΔT stayed exactly 0.00).
            self.cool_elapsed += dt;
            if self.cool_elapsed >= 600.0 {
                const SIGMA: f64 = 5.670e-8;
                let area = 4.0 * std::f64::consts::PI * c.radius * c.radius * 0.9;
                for (t, p) in self.temps.iter_mut().zip(self.particles.iter()) {
                    let tk = *t as f64;
                    if tk > 3.0 {
                        let d_t = SIGMA * area * tk.powi(4) * self.cool_elapsed
                            / (p.mass.max(1.0e-30) * self.specific_heat);
                        *t = (tk - d_t).max(2.7) as f32;
                    }
                }
                self.cool_elapsed = 0.0;
            }
        }
        // PHASE (docs/26): hotter than the boil point ⇒ vapor; cooled below ⇒ condensed. The flip is
        // a state change, not a force — the pair law above reads it.
        if self.contact_gas.is_some() || self.vapor_rs > 0.0 {
            for (i, t) in self.temps.iter().enumerate() {
                self.vapor[i] = (*t as f64) > self.boil_k;
            }
        }
        // Energy conservation (docs/20): the mechanical energy the contact damping/friction removed this
        // step becomes HEAT, split evenly across each dissipating pair — the source of the emergent
        // incandescence (a hard impact glows because the matter genuinely got hot).
        if let Some(c) = self.contact {
            let heat_per_k = self.specific_heat; // J/(kg·K)
            let m_ref = self.contact_ref_mass;
            let mut d_temp = vec![0.0f64; self.particles.len()];
            let sph_vapor = self.vapor_rs > 0.0 && self.vapor_h > 0.0;
            step_grid.for_each_pair(&step_pos, |i, j| {
                // A vapor↔vapor pair interacts by CONSERVATIVE SPH pressure (no contact force above), so it
                // dissipates NOTHING — skip it, exactly as the force loop does, or we would create heat from
                // a force that no longer acts (spurious energy, over-heating the plume).
                if sph_vapor && self.vapor.get(i) == Some(&true) && self.vapor.get(j) == Some(&true) {
                    return;
                }
                // Specific power × ref mass = the pair's actual dissipated watts; split half-half, each
                // side's temperature rise divides by ITS OWN heat capacity (m·c). Same mixed law as the
                // force, so the heat accounting matches the force that produced it (energy conserved).
                let mixed_law;
                let law: &crate::granular::Contact = if !self.per_grain_contact.is_empty() {
                    mixed_law = self.per_grain_contact[i].mix(&self.per_grain_contact[j]);
                    &mixed_law
                } else {
                    &c
                };
                let pw = crate::granular::contact_dissipation(
                    self.particles[i].pos,
                    self.particles[i].vel,
                    self.particles[j].pos,
                    self.particles[j].vel,
                    law,
                ) * m_ref;
                if pw > 0.0 {
                    let e_half = 0.5 * pw * dt;
                    d_temp[i] += e_half / (self.particles[i].mass.max(1.0e-30) * heat_per_k);
                    d_temp[j] += e_half / (self.particles[j].mass.max(1.0e-30) * heat_per_k);
                }
            });
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

    #[test]
    fn particle_timesteps_shrink_with_acceleration() {
        // docs/30 stage 3 criterion: a violently-accelerated particle gets a SMALLER dt than a quiescent
        // one (the whole point — it must be updated more often), positive/finite for real accel, and an
        // unaccelerated body is unconstrained (∞). Free-fall scaling dt ∝ √(ε/|a|) with velocity 0.
        let ps = vec![
            Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: 1.0 },
            Body { pos: DVec3::X, vel: DVec3::ZERO, mass: 1.0 },
            Body { pos: DVec3::X * 2.0, vel: DVec3::ZERO, mass: 1.0 },
        ];
        let mut agg = Aggregate::new(ps, 1.0); // softening ε = 1
        agg.self_gravity = false;
        let acc = vec![DVec3::X * 100.0, DVec3::X * 1.0, DVec3::ZERO]; // violent, gentle, none
        let dt = agg.particle_timesteps(&acc, 0.05);
        assert!(dt[0] < dt[1], "violent particle needs a smaller dt: {} !< {}", dt[0], dt[1]);
        assert!(dt[0] > 0.0 && dt[0].is_finite(), "positive finite dt for real accel");
        assert!(dt[2].is_infinite(), "unaccelerated body is unconstrained");
        assert!((dt[1] / dt[0] - 10.0).abs() < 1e-6, "dt ∝ 1/√|a|: 100× accel ⇒ dt/10");
    }

    #[test]
    fn contact_grid_matches_brute_force() {
        // docs/30 stage 1b invariant: the grid-accelerated contact force must EQUAL the O(N²) brute force.
        // Summation order differs, so it is identical to floating point (a tight tolerance), not bit-exact —
        // which is all conservation needs. Random overlapping cloud, contact only (no gravity/boundary/vapor).
        let mats = crate::materials::load();
        let basalt = crate::materials::index_of(&mats, "basalt");
        let (m, r) = (1.0e6, 0.5);
        let contact = crate::granular::contact_from_material(&mats[basalt], r, m);
        let mut s = 0x51ED_2701_ABCD_1234u64;
        let mut rng = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 40) as f64 / (1u64 << 24) as f64 - 0.5 // in [-0.5, 0.5)
        };
        let spacing = 0.7 * 2.0 * r; // < touch ⇒ many overlapping pairs
        let side = 9; // 729 grains > BRUTE_BELOW ⇒ exercises the grid's CELL path, not the brute fallback
        let mut ps = Vec::new();
        for x in 0..side {
            for y in 0..side {
                for z in 0..side {
                    let base = DVec3::new(x as f64, y as f64, z as f64) * spacing;
                    let jitter = DVec3::new(rng(), rng(), rng()) * (0.3 * r);
                    ps.push(Body {
                        pos: base + jitter,
                        vel: DVec3::new(rng(), rng(), rng()) * 10.0,
                        mass: m,
                    });
                }
            }
        }
        let n = ps.len();
        let mut agg = Aggregate::new(ps.clone(), 0.0).with_contact(contact, m);
        agg.self_gravity = false; // isolate the contact force
        let grid_acc = agg.accelerations();
        // Brute-force reference: the same law, O(N²).
        let mut brute = vec![DVec3::ZERO; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let f = crate::granular::contact_accel(ps[i].pos, ps[i].vel, ps[j].pos, ps[j].vel, &contact)
                    * m;
                brute[i] += f / ps[i].mass;
                brute[j] -= f / ps[j].mass;
            }
        }
        let mut max_rel = 0.0f64;
        for i in 0..n {
            let d = (grid_acc[i] - brute[i]).length();
            max_rel = max_rel.max(d / brute[i].length().max(1.0e-30).max(1.0));
        }
        assert!(max_rel < 1.0e-9, "grid contact accel must match brute force (max rel err {max_rel:.2e})");
        // Sanity: the cloud actually has overlaps (else the test proves nothing).
        assert!(brute.iter().any(|a| a.length() > 1.0), "test cloud must have real contact forces");
    }

    #[test]
    fn vapor_sph_expands_and_cools_conserving_energy() {
        // docs/26/27 real vapor pressure: a hot, packed blob with NO gravity/contact must EXPAND under its
        // own SPH pressure while the PdV energy equation COOLS it — internal heat → bulk kinetic, TOTAL
        // energy conserved, and (started at rest) momentum stays ~0. This is the mechanism that launches
        // the proto-lunar plume instead of trapping the impact's heat as a dead 80,000 K.
        let spacing = 1.0;
        let ps = cloud(3, spacing, 1.0); // 27 equal-mass parcels
        let (c, rs, h, t0) = (840.0f64, 100.0f64, 2.5 * spacing, 6000.0f32);
        let mut agg = Aggregate::new(ps, 0.0).with_vapor_sph(rs, h, 0.0).with_specific_heat(c);
        agg.self_gravity = false; // isolate the vapor pressure from gravity
        agg.boil_k = 0.0; // keep parcels flagged vapor as they cool (temp stays > 0)
        agg.temps = vec![t0; agg.particles.len()];
        agg.vapor = vec![true; agg.particles.len()];
        let com = |a: &Aggregate| {
            a.particles.iter().map(|p| p.pos).sum::<DVec3>() / a.particles.len() as f64
        };
        let rms = |a: &Aggregate, cm: DVec3| {
            (a.particles.iter().map(|p| (p.pos - cm).length_squared()).sum::<f64>()
                / a.particles.len() as f64)
                .sqrt()
        };
        let energy = |a: &Aggregate| {
            let ke: f64 = a.particles.iter().map(|p| 0.5 * p.mass * p.vel.length_squared()).sum();
            let u: f64 =
                a.particles.iter().zip(a.temps.iter()).map(|(p, &t)| p.mass * c * t as f64).sum();
            ke + u
        };
        let cm0 = com(&agg);
        let (r0, e0) = (rms(&agg, cm0), energy(&agg));
        let mut acc = agg.accelerations();
        for _ in 0..80 {
            agg.step(&mut acc, 1.0e-4);
        }
        let cm1 = com(&agg);
        let (r1, e1) = (rms(&agg, cm1), energy(&agg));
        let tmean = agg.temps.iter().map(|&t| t as f64).sum::<f64>() / agg.temps.len() as f64;
        assert!(r1 > 1.05 * r0, "vapor must expand: rms {r0:.3} → {r1:.3}");
        assert!(tmean < t0 as f64, "vapor must cool as it expands: {t0} → {tmean:.0} K");
        assert!(
            (e1 - e0).abs() < 0.03 * e0,
            "total energy (KE + internal) conserved: {e0:.4e} → {e1:.4e}"
        );
        let p1: DVec3 = agg.particles.iter().map(|p| p.vel * p.mass).sum();
        assert!(p1.length() < 1.0e-6 * agg.particles.len() as f64, "momentum stays ~0 (started at rest)");
    }

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
    fn white_hot_fragments_radiate_and_cool() {
        // Stefan–Boltzmann: a 4,600 K Theia-scale fragment must visibly fade within sim-hours.
        let mats = crate::materials::load();
        let basalt = crate::materials::index_of(&mats, "basalt");
        let m = 5.0e21;
        let r = (3.0 * m / (4.0 * std::f64::consts::PI * 2900.0)).cbrt();
        let contact = crate::granular::contact_from_material(&mats[basalt], r, m);
        let mut agg = Aggregate::new(
            vec![Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: m }],
            1.0,
        )
        .with_contact(contact, m)
        .with_specific_heat(840.0);
        agg.self_gravity = false;
        agg.temps[0] = 4_600.0;
        let mut acc = agg.accelerations();
        for _ in 0..7_200 {
            agg.step(&mut acc, 2.0); // 4 sim-hours
        }
        // The HONEST rate, not a cinematic one: a 750-km fragment's thermal mass is enormous per unit
        // surface, so σT⁴ sheds only ~0.5 K in 4 h — bulk-molten moonlets genuinely glow for MONTHS
        // (real magma bodies: millennia). Analytic: ΔT = σ·ε·A·T⁴·t/(m·c) ≈ 0.54 K here. What reality
        // adds that we can't resolve yet is the thin cooled CRUST (fast surface fade over days, molten
        // interior) — a surface/interior temperature split, flagged for the roadmap.
        let expect = 5.670e-8 * 0.9 * (4.0 * std::f64::consts::PI * r * r) * 4600.0f64.powi(4)
            * 14_400.0
            / (m * 840.0);
        let cooled = 4_600.0 - agg.temps[0] as f64;
        assert!(
            cooled > 0.0 && (cooled - expect).abs() / expect < 0.05,
            "cools at the Stefan–Boltzmann rate (ΔT {cooled:.2} K vs analytic {expect:.2} K)"
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
        let (n, m, _l) = agg.drain_settled(DVec3::ZERO, r0, DVec3::ZERO, 30.0, 1.0e6);
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
