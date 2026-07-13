//! Phase 3 matter solver: digging, material-driven fracture, and granular settling.
//!
//! This is the **CPU, natively-tested** foundation for destructible matter. It captures the target
//! behaviors — dig a hole, break chunks off, materials respond differently *by their own strength*,
//! debris falls under real gravity and settles back into the world (matter-conserving) — without the
//! full continuum machinery. It is deliberately structured to grow toward true **MLS-MPM** (add a
//! deformation gradient + constitutive stress, then move the hot loops to WGSL); see `docs/06`/`08`.
//!
//! Fracture is emergent from data: a voxel detaches only if the tool's stress exceeds its material's
//! `fracture_strength` (from `data/materials.json`). Granite (~1.2e7 Pa) shrugs off a tool that
//! shreds soil/grass (~5e3–1.5e4 Pa) — no per-material special-casing, just the numbers.
//!
//! Scale note (same as Phase 2): the test world is asteroid-sized, so its escape velocity is a few
//! cm/s. Ejection speeds are kept sub-escape so debris stays bound and re-settles; that is correct
//! micro-gravity physics, viewed via the time-scale.

use crate::body::Sphere;
use crate::gravity::MassField;
use crate::materials::Material;
use crate::world::World;
use glam::Vec3;

pub const PARTICLE_HALF: f32 = 0.45; // rendered/collision half-extent (voxel-ish)

// `DRAG` = 1.0: NO per-step velocity damping. This was a fudge — a per-step multiply that bled 62%/s of
// a vacuum particle's speed (exposed by `gpu-verify` foundational test F, docs/24). It was masking the
// non-conservative HEIGHTFIELD terrain contact (min-translation normal flip / vertical walls). With that
// resolved — the momentum-conserving contact solve, the conservative bilinear terrain penalty, and
// steep-terrain materialization (`materialize_steep_terrain`) — the core model no longer needs it, and a
// particle in vacuum correctly keeps its momentum (no atmosphere is modelled; when one is, drag emerges
// from real gas dynamics, not a constant). See docs/16 (no-fakery), docs/15 (honesty invariant).
pub const DRAG: f32 = 1.0;
pub const CONTACT_DAMP: f32 = 0.15; // fraction of velocity kept after touching ground. Loose rock rubble
//                                    is highly inelastic (coefficient of restitution ~0.1–0.2); 0.35 was
//                                    too bouncy and, combined with Earth g, left grains jittering forever.
pub const SETTLE_SPEED: f32 = 0.02; // below this HORIZONTAL speed, a grounded particle deposits into the
//                                    grid. NB the check is on horizontal speed only: a grounded grain's
//                                    vertical velocity is the explicit snap-contact's numerical jitter
//                                    (~g·dt residual), NOT real motion, so it must not block deposition.
const SETTLE_FRAMES: u32 = 10; // ...or after this many consecutive grounded steps
const MAX_EJECT: f32 = 0.045; // cap ejection speed below the world's ~7 cm/s escape velocity
const VAPOR_EXPANSION: f32 = 3.0; // vaporized ejecta expand away faster (gas/plasma) — docs/20

/// Ambient/reference temperature (K) — cold matter; impact ejecta heat above this (`docs/20`).
pub const REF_TEMP_K: f32 = 300.0;

/// A detached lump of matter in flight (one former voxel).
#[derive(Clone, Copy)]
pub struct Particle {
    pub pos: Vec3, // centered world coords
    pub vel: Vec3,
    pub material: usize,
    /// kg. Not read yet (gravity is mass-independent); kept for momentum/collision later.
    #[allow(dead_code)]
    pub mass: f32,
    /// Kelvin. Impact ejecta carry the heat deposited in them (`docs/20`); drives the incandescent
    /// glow of molten debris ([`crate::emission::incandescence`]). Cold matter sits at `REF_TEMP_K`.
    pub temp_k: f32,
    resting_frames: u32,
}

pub struct MatterSim {
    pub particles: Vec<Particle>,
    max_particles: usize,
    dirty: bool, // a voxel changed → terrain needs re-meshing
}

impl MatterSim {
    pub fn new(max_particles: usize) -> Self {
        MatterSim {
            particles: Vec::new(),
            max_particles,
            dirty: false,
        }
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Consume the "terrain changed" flag (caller re-meshes when true).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Dig a spherical region at `hit` (centered coords) with a tool of `strength` (Pa). Voxels whose
    /// material `fracture_strength` is at or below the tool strength detach into particles with a
    /// small outward+upward ejection; stronger materials are untouched. Returns the count detached.
    pub fn dig(
        &mut self,
        world: &mut World,
        materials: &[Material],
        hit: Vec3,
        radius: f32,
        strength: f32,
    ) -> usize {
        let center = world.center();
        let hv = hit + center; // voxel-space
        let ri = radius.ceil() as i32;
        let (cx, cy, cz) = (
            hv.x.floor() as i32,
            hv.y.floor() as i32,
            hv.z.floor() as i32,
        );
        let mut spawned = 0;
        for dz in -ri..=ri {
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    if self.particles.len() >= self.max_particles {
                        break;
                    }
                    let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                    let vc = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                    if (vc - hv).length() > radius {
                        continue;
                    }
                    let Some(mat) = world.material_at(x, y, z) else {
                        continue;
                    };
                    if strength < materials[mat].fracture_strength {
                        continue; // too strong to break with this tool
                    }
                    world.set_voxel(x, y, z, None);
                    let pos = vc - center;
                    let outward = (pos - hit).normalize_or_zero();
                    let dir = (outward * 0.7 + Vec3::Y * 0.6).normalize_or_zero();
                    let excess = (strength / materials[mat].fracture_strength).clamp(1.0, 6.0);
                    let speed = (0.02 * excess).min(MAX_EJECT);
                    self.particles.push(Particle {
                        pos,
                        vel: dir * speed,
                        material: mat,
                        mass: materials[mat].density,
                        temp_k: REF_TEMP_K,
                        resting_frames: 0,
                    });
                    spawned += 1;
                }
            }
        }
        if spawned > 0 {
            self.dirty = true;
        }
        spawned
    }

    /// A projectile striking the world — the generalized, **energy-driven impact** that a bullet, a
    /// pebble, or a falling moon all use (`docs/18`). It deposits kinetic `energy` (J) at `site`
    /// (centered coords) travelling along `direction`; spending the energy nearest-first, each voxel
    /// whose material fracture strength the budget can pay for (σ·V joules) detaches into ejecta, and
    /// too-strong voxels are left intact. So bigger energy → bigger crater, stronger material → smaller
    /// crater, and a liquid (σ≈0) yields everywhere it reaches (a splash). Ejecta fly along the impact
    /// + outward, sharing the leftover energy as kinetic. Returns the number of voxels ejected.
    ///
    /// **Scale-invariant:** a 10 g bullet at ~300 m/s (~450 J) and the Moon at ~11 km/s (~4.5e30 J)
    /// are the *same call* — only the numbers differ. The one non-physical knob is a hard search-radius
    /// cap, standing in for LOD: a truly huge impact should be *summarized* at coarse scale, not
    /// materialised voxel-by-voxel (`docs/18`).
    pub fn impact(
        &mut self,
        world: &mut World,
        materials: &[Material],
        site: Vec3,
        direction: Vec3,
        energy: f32,
    ) -> usize {
        const MAX_R: i32 = 24; // LOD guard on the materialised crater
        let center = world.center();
        let sv = site + center; // voxel space
        let dir = direction.normalize_or_zero();
        let (cx, cy, cz) = (
            sv.x.floor() as i32,
            sv.y.floor() as i32,
            sv.z.floor() as i32,
        );

        // Solid voxels in range, nearest first (a stand-in for "most stressed first").
        let mut candidates: Vec<(f32, i32, i32, i32, usize)> = Vec::new();
        for dz in -MAX_R..=MAX_R {
            for dy in -MAX_R..=MAX_R {
                for dx in -MAX_R..=MAX_R {
                    let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                    let vc = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                    let d = (vc - sv).length();
                    if d > MAX_R as f32 {
                        continue;
                    }
                    if let Some(mat) = world.material_at(x, y, z) {
                        candidates.push((d, x, y, z, mat));
                    }
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));

        // Spend the energy budget fracturing what it can afford; a voxel costs σ·V (Pa·m³ = J) to
        // detach. Too-strong voxels are skipped (left intact), so weak matter craters while strong
        // matter resists — bullet-in-rock vs pebble-in-pond falls out of the material, not a branch.
        let mut budget = energy;
        let mut ejecta: Vec<(usize, f32)> = Vec::new(); // (particle index, distance from the impact)
        for (d, x, y, z, mat) in candidates {
            if self.particles.len() >= self.max_particles {
                break;
            }
            let work = materials[mat].fracture_strength; // σ · 1 m³
            if budget < work {
                continue; // can't afford this one — leave it intact, try the rest
            }
            budget -= work;
            world.set_voxel(x, y, z, None);
            let pos = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - center;
            let outward = (pos - site).normalize_or_zero();
            let ev = (dir * 0.5 + outward * 0.5).normalize_or_zero();
            ejecta.push((self.particles.len(), d));
            self.particles.push(Particle {
                pos,
                vel: ev, // unit direction now; speed assigned below from the energy share
                material: mat,
                mass: materials[mat].density,
                temp_k: REF_TEMP_K, // set below from the deposited heat
                resting_frames: 0,
            });
        }

        if !ejecta.is_empty() {
            // Kinetic: share ~30% of the impact energy among the ejecta; v = √(2·KE/m).
            // ~5% of the impact energy goes to ejecta kinetic (most goes to fracture + heat). Under
            // real planetary gravity this keeps ejecta arcing within the scene rather than off the top.
            let ke_each = 0.05 * energy / ejecta.len() as f32;
            // Shock heat: deposited energy density peaks at the contact and falls to zero at the crater
            // rim, so the centre melts/vaporizes (glows) while the rim stays cold rubble — the honest
            // radial gradient (docs/20). e_peak concentrates ~30% of the energy into a small core.
            const V_CORE: f32 = 8.0; // m³, central concentration volume
            let r_max = ejecta.iter().map(|&(_, d)| d).fold(1.0f32, f32::max);
            let e_peak = 0.3 * energy / V_CORE; // J/m³ at the centre
            for &(i, d) in &ejecta {
                let m = self.particles[i].mass.max(1.0e-6);
                let mut speed = (2.0 * ke_each / m).sqrt();

                let falloff = (1.0 - d / r_max).clamp(0.0, 1.0).powi(2);
                let e_shock = e_peak * falloff; // J/m³ deposited here
                let mat = &materials[self.particles[i].material];
                let c = mat.thermal.as_ref().map_or(1000.0, |t| t.specific_heat);
                self.particles[i].temp_k = REF_TEMP_K + e_shock / (mat.density.max(1.0) * c);

                // Phase class (docs/20) from the thermodynamic thresholds: a carved voxel is at least
                // Fractured, and the hot core Melts / Vaporizes. The class drives behaviour — vaporized
                // matter is gas/plasma, so it expands away fast (a vapour flash).
                let e_class = e_shock.max(mat.fracture_strength);
                if crate::damage::classify(e_class as f64, mat)
                    == crate::damage::PhaseChange::Vaporized
                {
                    speed *= VAPOR_EXPANSION;
                }
                self.particles[i].vel *= speed;
            }
            self.dirty = true;
        }
        ejecta.len()
    }

    /// **Terrain becomes matter** (`docs/24` Stage 3, the `docs/19` LOD-materialization bridge made
    /// real). Every SOLID voxel within `radius` of `site` (centered coords) is removed from the world
    /// and re-created as a grain **at rest** at that voxel's centre: same position (so gravitational
    /// potential energy is conserved — nothing teleports), same material and temperature, **zero
    /// velocity** (so no kinetic energy is injected). This is not destruction and not scripted ejecta —
    /// it is a change of *representation*, from the lossy heightfield summary into the real grains the
    /// granular contact law acts on. The impact region is then honest matter: compression, rebound, and
    /// the crater all EMERGE from contact (verified conservative in `tools/gpu-verify` scene I-flat),
    /// instead of the non-conservative heightfield-edge penalty that injected the crater "free energy".
    /// A driver (the meteor's momentum, [`Self::deposit_impulse`]) is applied separately. Returns the
    /// count materialized; the new grains are `self.particles[start..]` where `start` was the prior len.
    pub fn materialize_region(
        &mut self,
        world: &mut World,
        materials: &[Material],
        site: Vec3,
        radius: f32,
    ) -> usize {
        let center = world.center();
        let sv = site + center; // voxel space
        let ri = radius.ceil() as i32;
        let (cx, cy, cz) = (sv.x.floor() as i32, sv.y.floor() as i32, sv.z.floor() as i32);
        let start = self.particles.len();
        for dz in -ri..=ri {
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    if self.particles.len() >= self.max_particles {
                        break;
                    }
                    let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                    let vc = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                    if (vc - sv).length() > radius {
                        continue;
                    }
                    let Some(mat) = world.material_at(x, y, z) else {
                        continue;
                    };
                    world.set_voxel(x, y, z, None);
                    self.particles.push(Particle {
                        pos: vc - center, // same place the voxel was → PE conserved
                        vel: Vec3::ZERO,  // AT REST → no KE injected
                        material: mat,
                        mass: materials[mat].density, // kg (1 m³ voxel)
                        temp_k: REF_TEMP_K,
                        resting_frames: 0,
                    });
                }
            }
        }
        if self.particles.len() > start {
            self.dirty = true;
        }
        self.particles.len() - start
    }

    /// **Excavate a FURROW into the terrain via the SHARED impact law** (`docs/28`). Every solid voxel
    /// inside the [`crate::impact::Furrow`] bowl is removed from the world and re-created as a grain
    /// carrying the furrow's DECLARED shock-ejection velocity — the SAME excavation the space-band Theia
    /// strike runs (`impact::furrow_target_grains` fills the same [`Furrow`] with fresh grains; here we
    /// convert the terrain's REAL voxels). So a meteor craters terrain by the one canonical law, at ANY
    /// impact angle: an oblique strike carves a downrange-elongated furrow, a vertical one a symmetric
    /// bowl (the obliquity dependence lives in `Furrow::new`). Matter-conserving like
    /// [`Self::materialize_region`] — voxels removed == grains made, each grain kept at its own voxel
    /// centre (PE conserved) with its OWN material/mass/temperature (the layered strata the meteor
    /// actually struck), NOT one bulk proxy. The furrow's frame is in CENTERED world coords (matching the
    /// meteor `hit`); `ground_vel` is the surface's bulk motion (0 for static terrain). `impactor_mass`
    /// (kg) is the meteor's mass — with the furrow's impact speed it sets the impact energy `½·m·v²` that
    /// CAPS the total ejecta KE ([`crate::impact::ejecta_energy_scale`]): for a SMALL meteor the declared
    /// H-H law would hand the excavated grains more KE than the impact delivered (the debris storm), and
    /// exact energy conservation scales the ejection back so `Σ½m|v_ej|² ≤ ½·m·v²`. Returns the count
    /// materialized; the new grains are `self.particles[start..]`.
    pub fn materialize_furrow(
        &mut self,
        world: &mut World,
        materials: &[Material],
        furrow: &crate::impact::Furrow,
        ground_vel: Vec3,
        impactor_mass: f64,
    ) -> usize {
        let center = world.center();
        let c64 = glam::DVec3::new(center.x as f64, center.y as f64, center.z as f64);
        let up = furrow.n; // outward surface normal (flat terrain: +y under uniform gravity)
        // Scan the voxel bounding box the bowl can reach: it spans ±(l_along) about a centre `downrange`
        // along-track, ±l_lat across, and l_depth into the surface — bound by the largest of these.
        let reach = (furrow.l_along + furrow.downrange.abs())
            .max(furrow.l_lat)
            .max(furrow.l_depth);
        let sv = Vec3::new(furrow.site.x as f32, furrow.site.y as f32, furrow.site.z as f32) + center;
        let ri = (reach.ceil() as i32).max(1);
        let (cx, cy, cz) = (sv.x.floor() as i32, sv.y.floor() as i32, sv.z.floor() as i32);
        let start = self.particles.len();
        for dz in -ri..=ri {
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    if self.particles.len() >= self.max_particles {
                        break;
                    }
                    let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                    // Voxel centre in CENTERED world coords (f64, the frame the furrow lives in).
                    let vc = glam::DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5) - c64;
                    let below = -(vc - furrow.site).dot(up); // depth beneath the surface plane
                    if !furrow.contains_point(vc, below) {
                        continue;
                    }
                    let Some(mat) = world.material_at(x, y, z) else {
                        continue;
                    };
                    world.set_voxel(x, y, z, None);
                    let ej = furrow.ejection(vc, up, below);
                    self.particles.push(Particle {
                        pos: Vec3::new(vc.x as f32, vc.y as f32, vc.z as f32), // its own voxel centre (PE conserved)
                        vel: ground_vel + Vec3::new(ej.x as f32, ej.y as f32, ej.z as f32),
                        material: mat,
                        mass: materials[mat].density, // kg (1 m³ voxel)
                        temp_k: REF_TEMP_K,
                        resting_frames: 0,
                    });
                }
            }
        }
        if self.particles.len() > start {
            self.dirty = true;
            // EXACT energy conservation (docs/28): the declared H-H ejection can hand a SMALL meteor's
            // excavated grains more KE than the impact carried — cap the total ejecta KE at the impact
            // energy ½·m·v² (the SAME cap the space band uses). For a small impactor the ejection is
            // scaled by √(E_i/KE); a giant one is within budget (factor 1, unchanged). The KE is measured
            // RELATIVE to the co-moving ground (`ground_vel`) — only the ejection component is scaled.
            let e_impact = 0.5 * impactor_mass * furrow.v_mag * furrow.v_mag;
            let scale = crate::impact::ejecta_energy_scale(
                self.particles[start..].iter().map(|p| {
                    let ej = p.vel - ground_vel;
                    (
                        p.mass as f64,
                        glam::DVec3::new(ej.x as f64, ej.y as f64, ej.z as f64),
                    )
                }),
                e_impact,
            );
            if scale < 1.0 {
                let s = scale as f32;
                for p in &mut self.particles[start..] {
                    p.vel = ground_vel + (p.vel - ground_vel) * s;
                }
            }
        }
        self.particles.len() - start
    }

    /// **Materialize STEEP terrain into grains** (`docs/24` Path B). A heightfield represents gentle
    /// slopes conservatively (a smooth bilinear surface → an exact −∇U penalty), but NOT vertical walls:
    /// a cliff smoothed over one voxel becomes a ~N:1 gradient, a huge non-conservative force that
    /// explodes energetic grains (and was the last thing the `drag` fudge masked). The honest fix is to
    /// make steep terrain what it physically is — loose matter (talus/scree). Any column within `radius`
    /// of `site` whose highest solid voxel stands `steep_drop`+ above its LOWEST neighbour is a cliff
    /// face; its exposed voxels (down to that neighbour) become grains at rest, and the heightfield
    /// settles to a gentle slope the contact can handle. Same conservation as [`Self::materialize_region`]
    /// (mass + potential energy; zero injected kinetic energy). Returns the count materialized.
    pub fn materialize_steep_terrain(
        &mut self,
        world: &mut World,
        materials: &[Material],
        site: Vec3,
        radius: f32,
        steep_drop: i32,
    ) -> usize {
        let center = world.center();
        let sv = site + center;
        let ri = radius.ceil() as i32;
        let (cx, cz) = (sv.x.floor() as i32, sv.z.floor() as i32);
        // 1. Find the exposed cliff-face voxels (scan the heightfield read-only, then mutate).
        let mut faces: Vec<(i32, i32, i32)> = Vec::new();
        for dz in -ri..=ri {
            for dx in -ri..=ri {
                let (x, z) = (cx + dx, cz + dz);
                if (((dx * dx + dz * dz) as f32).sqrt()) > radius {
                    continue;
                }
                let Some(top) = world.surface_top_voxel(x, z) else {
                    continue;
                };
                let solid_top = top - 1; // highest solid voxel in this column
                let mut min_nbr = solid_top; // lowest neighbouring solid top
                for (nx, nz) in [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)] {
                    if let Some(nt) = world.surface_top_voxel(nx, nz) {
                        min_nbr = min_nbr.min(nt - 1);
                    }
                }
                let face_height = (solid_top - min_nbr) as f32;
                if solid_top - min_nbr >= steep_drop {
                    for y in (min_nbr + 1)..=solid_top {
                        // A HARD material HOLDS a steep face — a real granite cliff, which we see standing
                        // in nature (Robin). Only material too WEAK to support its own cliff slumps to
                        // talus. Critical vertical-cliff height ≈ strength / (ρ·g); above it the face can't
                        // hold. Granite (~1.2e7 Pa) holds ~450 m; dirt (~5e3 Pa) holds <0.4 m. So a granite
                        // cliff stays rigid terrain; a dirt/sand bank becomes grains — emergent from
                        // strength, not a rule. (A granite cliff that a heightfield still can't contact
                        // conservatively is the case for COHESIVE-aggregate materialization — flagged next.)
                        if let Some(mat) = world.material_at(x, y, z) {
                            let m = &materials[mat];
                            let h_crit = m.fracture_strength / (m.density.max(1.0) * 9.81);
                            if face_height > h_crit {
                                faces.push((x, y, z)); // too weak to hold this cliff ⇒ slumps
                            }
                        }
                    }
                }
            }
        }
        // 2. Materialize the faces (voxel → grain at rest at its own centre — mass + PE conserved).
        let start = self.particles.len();
        for (x, y, z) in faces {
            if self.particles.len() >= self.max_particles {
                break;
            }
            let Some(mat) = world.material_at(x, y, z) else {
                continue;
            };
            world.set_voxel(x, y, z, None);
            self.particles.push(Particle {
                pos: Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - center,
                vel: Vec3::ZERO,
                material: mat,
                mass: materials[mat].density,
                temp_k: REF_TEMP_K,
                resting_frames: 0,
            });
        }
        if self.particles.len() > start {
            self.dirty = true;
        }
        self.particles.len() - start
    }

    /// **The honest impact driver** (`docs/24` Stage 2): deposit the impactor's real `momentum`
    /// (kg·m/s, a vector) into the grains materialized this event (`self.particles[since..]`) that lie
    /// within `core_radius` of `site` — the coupling core the impactor actually shoves. Every core
    /// grain gets the SAME velocity change `Δv = momentum / Σmᵢ`, so `Σ mᵢ·Δvᵢ = momentum` EXACTLY:
    /// momentum is conserved, not invented. Ejection is NOT assigned here — it emerges as the driven
    /// core compresses the material ahead of it and the contacts rebound. Because a small fast impactor
    /// carries huge momentum but that momentum spread over the core's large mass yields only a modest
    /// Δv, only a few percent of ½mv² ends up as bulk motion — exactly the "~5% to ejecta" the old
    /// scripted code hard-coded, here falling out of momentum-vs-energy instead of a magic constant. The
    /// remaining energy is shock heat; deposit it with [`Self::deposit_shock_heat`]. Returns the core
    /// grain count (0 if none in range).
    pub fn deposit_impulse(
        &mut self,
        since: usize,
        site: Vec3,
        momentum: Vec3,
        core_radius: f32,
    ) -> usize {
        let core: Vec<usize> = (since..self.particles.len())
            .filter(|&i| (self.particles[i].pos - site).length() <= core_radius)
            .collect();
        if core.is_empty() {
            return 0;
        }
        let m_total: f32 = core.iter().map(|&i| self.particles[i].mass.max(1.0e-6)).sum();
        let dv = momentum / m_total; // uniform Δv ⇒ Σ mᵢ·Δv = momentum (conserved)
        for &i in &core {
            self.particles[i].vel += dv;
        }
        core.len()
    }

    /// Deposit shock **heat** (`docs/20`) into the grains materialized this event
    /// (`self.particles[since..]`): `heat_energy` J spread with a radial gradient — densest at `site`
    /// (the contact melts/vaporizes and glows) and falling to zero by `r_max` (the rim stays cold
    /// rubble). This is the energy the momentum impulse ([`Self::deposit_impulse`]) did NOT turn into
    /// motion — the bulk of a fast impactor's ½mv². It is not destroyed: it raises each grain's `temp_k`
    /// through its material specific heat (→ incandescent `emission`, radiated later). Honest gradient,
    /// not a uniform fireball.
    pub fn deposit_shock_heat(
        &mut self,
        since: usize,
        site: Vec3,
        heat_energy: f32,
        materials: &[Material],
    ) {
        if since >= self.particles.len() {
            return;
        }
        // FILL the isobaric core from the contact OUTWARD (not a smeared gradient — that diluted the
        // energy below the vaporization threshold everywhere, the "448 K over 14 m" bug). A hypervelocity
        // impactor is sub-grain-sized, so its energy concentrates into a small PLASMA CORE: fill each
        // grain, nearest first, up to `SUPERHEAT ×` its own vaporization energy, spilling to the next
        // grain when full, until the budget is spent. The core reaches a few× vaporization (→ vapor
        // expansion extracts the excess as ejecta KE); grains past the core stay cold rubble. SUPERHEAT
        // is the core superheat ratio — a physical modeling choice (real plasma cores reach ≫ this);
        // it also sets the fraction of impact energy that becomes ejection KE: `1 − 1/SUPERHEAT`.
        const SUPERHEAT: f32 = 3.0;
        let mut order: Vec<usize> = (since..self.particles.len()).collect();
        order.sort_by(|&a, &b| {
            (self.particles[a].pos - site)
                .length()
                .total_cmp(&(self.particles[b].pos - site).length())
        });
        let mut budget = heat_energy;
        for &i in &order {
            if budget <= 0.0 {
                break;
            }
            let mat = &materials[self.particles[i].material];
            let c = mat.thermal.as_ref().map_or(1000.0, |t| t.specific_heat);
            let rho = mat.density.max(1.0);
            let e_vap = crate::damage::vapor_energy_density(mat).unwrap_or(2.0e10) as f32;
            let e_here = budget.min(SUPERHEAT * e_vap); // J into this 1 m³ grain (fill to the cap)
            budget -= e_here;
            self.particles[i].temp_k += e_here / (rho * c);
        }
    }

    /// **Vapor-driven ejection** (`docs/24`, Robin's model) — the real engine of a hypervelocity crater.
    /// At ~17 km/s the shock deposits FAR more energy than it takes to vaporize the target near the
    /// contact (½v² ≈ 30–50× granite's vaporization energy), so that matter flashes directly to gas (no
    /// atmosphere needed). The vapor is a superheated high-pressure bubble; it EXPANDS, doing PdV work on
    /// the surrounding matter — and THAT throws the ejecta curtain and excavates the bowl, not elastic
    /// rebound. This routes the energy we already deposited as shock heat (`deposit_shock_heat`) through
    /// the phase transition it should drive, honestly and conservatively:
    ///   • For each grain heated PAST full vaporization (`damage::vapor_energy_density`), the EXCESS
    ///     (superheat) thermal energy is the vapor's available expansion energy `E_expand`.
    ///   • That energy is removed from the grain's `temp_k` (the gas cools as it expands — adiabatic) and
    ///     converted to RADIAL outward kinetic energy shared over the ejecta from `site`, with
    ///     `Σ ½mᵢvᵢ² = E_expand` (energy conserved: thermal → kinetic, nothing invented — the honest
    ///     replacement for the deleted scripted ejecta speed).
    /// The GPU then flies the trajectories (ballistic + contact + fallback). A uniform radial speed is a
    /// documented first model for the (unresolvable, sub-µs) expansion velocity profile. Returns
    /// `E_expand` (J), the energy the vapor delivered to ejection.
    pub fn deposit_vapor_expansion(
        &mut self,
        since: usize,
        site: Vec3,
        materials: &[Material],
    ) -> f32 {
        // 1. Sum the superheat (energy above full vaporization) and cool those grains back toward the
        //    boil point — that energy is leaving as expansion, not staying as heat.
        let mut e_expand = 0.0f32;
        for i in since..self.particles.len() {
            let mat = &materials[self.particles[i].material];
            let Some(e_vap) = crate::damage::vapor_energy_density(mat) else {
                continue; // no thermal data ⇒ we don't claim to know its vaporization (honesty)
            };
            let c = mat.thermal.as_ref().map_or(1000.0, |t| t.specific_heat);
            let rho = mat.density.max(1.0);
            let e_thermal = rho * c * (self.particles[i].temp_k - REF_TEMP_K); // J in this 1 m³ grain
            let excess = e_thermal - e_vap as f32;
            if excess > 0.0 {
                e_expand += excess;
                self.particles[i].temp_k -= excess / (rho * c); // adiabatic cooling of the vapor
            }
        }
        if e_expand <= 0.0 {
            return 0.0;
        }
        // 2. The vapor is a hot gas bubble at the core; as it expands it launches a shock FRONT into the
        //    surrounding grains. FAITHFULLY that front is a thin, ~km/s wave — UNRESOLVABLE at our step
        //    (it would tunnel, >1 m/substep). So we spread E_expand over just enough of the NEAREST grains
        //    that the front speed stays resolvable (≤ V_MAX), and no further — the shock as concentrated
        //    as the hardware allows (emulate to the extent computers are capable). We push those grains
        //    radially outward; the crater bowl, the up-and-out curtain, and the downward compaction then
        //    ALL EMERGE from the granular contact (the free surface lets the top fly; the buried sides
        //    compress). We do NOT impose the asymmetry, a direction, or a crater size — only the honest
        //    outward push, capped at what we can resolve.
        const V_MAX: f32 = 200.0; // ≈0.2 m/substep — within the implicit contact's deep-overlap range
        let m_needed = 2.0 * e_expand / (V_MAX * V_MAX); // mass over which E_expand gives exactly V_MAX
        // Gather the nearest grains (from the contact outward) until we've collected `m_needed`.
        let mut idx: Vec<usize> = (since..self.particles.len())
            .filter(|&i| (self.particles[i].pos - site).length() > 0.5)
            .collect();
        idx.sort_by(|&a, &b| {
            (self.particles[a].pos - site)
                .length()
                .total_cmp(&(self.particles[b].pos - site).length())
        });
        let mut m_front = 0.0f32;
        let mut front = Vec::new();
        for &i in &idx {
            if m_front >= m_needed {
                break;
            }
            m_front += self.particles[i].mass.max(1.0e-6);
            front.push(i);
        }
        if m_front <= 0.0 {
            return 0.0;
        }
        let v0 = (2.0 * e_expand / m_front).sqrt(); // ≤ V_MAX (exactly V_MAX once m_needed is reached)
        for &i in &front {
            let radial = self.particles[i].pos - site;
            let r = radial.length().max(1.0e-6);
            self.particles[i].vel += (radial / r) * v0; // push the front out; the rest emerges
        }
        e_expand
    }

    /// Structural collapse: detach every voxel gravity cannot hold into a falling particle (starting
    /// from rest). Run after an edit that may have undercut or isolated matter (a dig, a meteor crater).
    /// Uses the honest support model [`World::find_structurally_unsupported`] — support propagates from
    /// the base UPWARD with a material-strength-limited cantilever — so an undercut crater lip that hangs
    /// off the rim SIDEWAYS (which the old 6-connectivity `find_unsupported` wrongly kept) now falls.
    ///
    /// `g` is the terrain's surface gravity (the emergent ~9.88, `Engine::surface_g`), which sets each
    /// material's cantilever reach. We iterate to a FIXPOINT: removing an overhang's undercut tip can
    /// leave the next voxels beyond reach of the (now shorter) braced span, so we re-evaluate until the
    /// standing matter is self-consistently supported. Each pass only removes voxels, so it terminates;
    /// for uniform material one pass already converges. Matter-conserving (one particle per voxel).
    /// Returns the total number collapsed.
    pub fn collapse(&mut self, world: &mut World, materials: &[Material], g: f32) -> usize {
        let center = world.center();
        let mut n = 0;
        loop {
            let unsupported = world.find_structurally_unsupported(materials, g);
            if unsupported.is_empty() {
                break;
            }
            let mut removed = 0;
            for (x, y, z) in unsupported {
                if self.particles.len() >= self.max_particles {
                    break;
                }
                if let Some(mat) = world.material_at(x, y, z) {
                    world.set_voxel(x, y, z, None);
                    let pos = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - center;
                    self.particles.push(Particle {
                        pos,
                        vel: Vec3::ZERO,
                        material: mat,
                        mass: materials[mat].density,
                        temp_k: REF_TEMP_K,
                        resting_frames: 0,
                    });
                    n += 1;
                    removed += 1;
                }
            }
            if removed == 0 {
                break; // hit the particle cap with nothing left to remove — stop cleanly
            }
        }
        if n > 0 {
            self.dirty = true;
        }
        n
    }

    /// **Shared de-resolution primitive** (`docs/22`/`docs/23`): a single grain that has come to REST at
    /// `pos` (centered coords), of material `material`, returns to the voxel grid — matter-conserving
    /// (one grain → exactly one voxel). This is the on-demand-resolution principle in reverse: once the
    /// excitement passes, resolved matter goes back to bulk. It is the SINGLE source of truth for
    /// depositing a resting grain, used by BOTH the CPU [`Self::step`] settling and the GPU debris
    /// readback (`lib.rs::settle_gpu_debris`) — "improving one improves all".
    ///
    /// Deposits into the column's air-start voxel (stacks / refills the crater). Returns `true` iff the
    /// grain was deposited (the caller then removes it). Returns `false` — grain STAYS a grain, matter
    /// still conserved — when the target cell would be INSIDE a dynamic `body` (the probe: debris piles
    /// ON it, never through it) or the column is already full. NEVER deletes matter to lower a count.
    pub fn deposit_resting_grain(
        &mut self,
        world: &mut World,
        pos: Vec3,
        material: usize,
        bodies: &[Sphere],
    ) -> bool {
        let center = world.center();
        let xi = (pos.x + center.x).floor() as i32;
        let zi = (pos.z + center.z).floor() as i32;
        if let Some(ty) = world.surface_top_voxel(xi, zi) {
            if (ty as usize) < world.h {
                let cell = Vec3::new(xi as f32 + 0.5, ty as f32 + 0.5, zi as f32 + 0.5) - center;
                let inside_body = bodies
                    .iter()
                    .any(|b| (cell - b.pos).length() < b.radius + PARTICLE_HALF);
                if !inside_body {
                    world.set_voxel(xi, ty, zi, Some(material));
                    self.dirty = true;
                    return true;
                }
            }
        }
        false
    }

    /// Advance all particles by `dt`: gravity from the field, terrain collision, and — when a
    /// particle comes to rest — deposit it back into the voxel grid (piling; matter-conserving).
    /// `bodies` are dynamic solids (the probe) the settling matter must not deposit *inside* — debris
    /// piles on a body, never through it.
    pub fn step(&mut self, world: &mut World, field: &MassField, bodies: &[Sphere], dt: f32) {
        let center = world.center();
        let bound = world.w.max(world.h).max(world.d) as f32;

        // Perf: use the O(1) centre-of-mass gravity approximation for debris, not the full ~1000-point
        // field per particle. A big impact throws thousands of ejecta; the full field (per particle,
        // per substep) is ~10⁸ ops/frame on one wasm thread → single-digit FPS. The COM approximation is
        // ~1000× cheaper; the cost is a slight inward drift of off-centre debris (docs/08). The real fix
        // is moving this whole loop to a GPU compute shader (docs/08 / docs/22) — then we can afford the
        // full field again, massively parallel.
        let mut i = 0;
        while i < self.particles.len() {
            let mut p = self.particles[i];
            let accel = field.acceleration_point_approx(p.pos, 6.0);
            p.vel += accel * dt;
            p.vel *= DRAG;
            p.pos += p.vel * dt;

            // Drifted off the world entirely → lost (rare; ejection is sub-escape).
            if p.pos.y < -center.y - 20.0
                || p.pos.y > center.y + bound
                || p.pos.x.abs() > bound
                || p.pos.z.abs() > bound
            {
                self.particles.swap_remove(i);
                continue;
            }

            let xi = (p.pos.x + center.x).floor() as i32;
            let zi = (p.pos.z + center.z).floor() as i32;
            let ground_y = world
                .surface_top_voxel(xi, zi)
                .map(|t| t as f32 - center.y)
                .unwrap_or(-center.y - 1.0);

            if p.pos.y - PARTICLE_HALF <= ground_y {
                p.pos.y = ground_y + PARTICLE_HALF;
                p.vel *= CONTACT_DAMP;
                p.resting_frames += 1;
                // "At rest" = stopped sliding HORIZONTALLY. The remaining vertical velocity of a grounded
                // grain is the snap-contact's per-step jitter (gravity pulls it a hair below the surface,
                // the contact snaps it back), a numerical artifact under Earth g — checking full speed
                // left grains bouncing above SETTLE_SPEED forever and never depositing.
                let horiz = (p.vel.x * p.vel.x + p.vel.z * p.vel.z).sqrt();
                if horiz < SETTLE_SPEED || p.resting_frames > SETTLE_FRAMES {
                    // Deposit via the SHARED de-resolution primitive (same law the GPU debris readback
                    // uses): into the column's air-start voxel — unless a dynamic body occupies that cell
                    // or the column is full, in which case the grain stays a particle (matter conserved).
                    if self.deposit_resting_grain(world, p.pos, p.material, bodies) {
                        self.particles.swap_remove(i);
                        continue;
                    }
                }
            } else {
                p.resting_frames = 0;
            }

            self.particles[i] = p;
            i += 1;
        }
    }

    /// Body↔particle contacts — the other half of the unified awake-set dynamics. Any debris particle
    /// overlapping `body` exchanges momentum with it (mass-weighted impulse, lightly inelastic) and
    /// both wake. So a thrown clod actually shoves the probe, and the probe scatters debris it plows
    /// into — the interaction is *real*, read from mass and velocity, not a per-object script (see
    /// `docs/16`; honesty invariant in `docs/15`). O(particles) for the handful of bodies we have; a
    /// spatial index takes over when bodies/particles grow (`docs/08`). Momentum is conserved: the
    /// impulse on the body and the particle are equal and opposite.
    pub fn couple_body(&mut self, body: &mut Sphere, _dt: f32) {
        let sum_r = body.radius + PARTICLE_HALF;
        let inv_b = 1.0 / body.mass;
        for p in &mut self.particles {
            let d = p.pos - body.pos;
            let dist = d.length();
            if dist >= sum_r {
                continue;
            }
            let n = if dist > 1e-5 { d / dist } else { Vec3::Y }; // contact normal, body → particle
            let inv_p = 1.0 / p.mass;
            let inv_sum = inv_b + inv_p;

            // Separate the overlap, split by inverse mass — the heavy body barely moves, the light
            // particle does most of the moving.
            let pen = sum_r - dist;
            body.pos -= n * (pen * inv_b / inv_sum);
            p.pos += n * (pen * inv_p / inv_sum);

            // Exchange momentum only if they are approaching along the contact normal.
            let rel = (p.vel - body.vel).dot(n);
            if rel < 0.0 {
                let e = body.restitution.min(0.3);
                let j = -(1.0 + e) * rel / inv_sum;
                body.vel -= n * (j * inv_b);
                p.vel += n * (j * inv_p);
            }

            body.resting = false; // contact wakes the body
            p.resting_frames = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gravity, materials, world};

    fn center_surface(w: &World) -> f32 {
        let c = w.center();
        w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y
    }

    #[test]
    fn dig_detaches_soft_but_not_hard() {
        let mats = materials::load();

        // Soft layers (soil/grass, ~5e3–1.5e4 Pa) detach under a 1e6 Pa tool.
        let mut w1 = world::generate(&mats);
        let surf = center_surface(&w1);
        let mut soft = MatterSim::new(50_000);
        let n_soft = soft.dig(&mut w1, &mats, Vec3::new(0.0, surf - 1.5, 0.0), 3.0, 1.0e6);
        assert!(n_soft > 0, "soil/grass should detach under a 1e6 Pa tool");

        // The same tool deep in the rock strata (basalt crust ~1.45e7 Pa / peridotite mantle ~1.0e7 Pa)
        // removes nothing — the real layered column, not a granite proxy (docs/28).
        let mut w2 = world::generate(&mats);
        let mut hard = MatterSim::new(50_000);
        let n_rock = hard.dig(&mut w2, &mats, Vec3::new(0.0, surf - 30.0, 0.0), 3.0, 1.0e6);
        assert_eq!(
            n_rock, 0,
            "the rock strata resist a tool weaker than their fracture strength"
        );

        // A stronger blast (2e7 Pa) *does* break the rock (above basalt's ~1.45e7 Pa).
        let mut w3 = world::generate(&mats);
        let mut blast = MatterSim::new(50_000);
        let n_blast = blast.dig(&mut w3, &mats, Vec3::new(0.0, surf - 30.0, 0.0), 3.0, 2.0e7);
        assert!(n_blast > 0, "a strong enough blast breaks the crust rock");
    }

    /// The dig/fracture THRESHOLD is each voxel's OWN material strength — the terrain is real layered
    /// strata (grass → basalt → peridotite → iron; docs/28), NOT one bulk-rock proxy. A tool set
    /// strictly BETWEEN grass's and iron's real DB fracture strengths breaks the SOFT voxel but leaves
    /// the HARD one intact: the threshold genuinely tracks the matter actually there. This refutes a
    /// single-material proxy — the old hardcoded granite (1.2e7 Pa) would break NEITHER (the tool, the
    /// geometric mean ~2.57e6 Pa, is below granite), so `dig_one(grass) > 0` would fail under the fudge.
    #[test]
    fn dig_threshold_tracks_each_voxels_real_material() {
        let mats = materials::load();
        let grass = materials::index_of(&mats, "grass"); // ~1.5e4 Pa
        let iron = materials::index_of(&mats, "iron"); //   ~4.4e8 Pa
        let soft_sigma = mats[grass].fracture_strength;
        let hard_sigma = mats[iron].fracture_strength;
        assert!(soft_sigma < hard_sigma, "grass is far weaker than iron (real DB)");
        // A tool strictly between the two real strengths (geometric mean).
        let tool = (soft_sigma * hard_sigma).sqrt();
        assert!(
            soft_sigma < tool && tool < hard_sigma,
            "the tool sits between the two materials' real fracture strengths"
        );

        // Dig a single voxel of `mat` (an 8³ air world with one solid cell) and count detached grains.
        let dig_one = |mat: usize| -> usize {
            let mut w = World {
                w: 8,
                h: 8,
                d: 8,
                voxels: vec![0; 8 * 8 * 8],
                max_top: 4,
            };
            w.set_voxel(4, 3, 4, Some(mat));
            let hit = Vec3::new(4.5, 3.5, 4.5) - w.center(); // centered coords of that voxel
            let mut sim = MatterSim::new(64);
            sim.dig(&mut w, &mats, hit, 1.5, tool)
        };
        assert!(dig_one(grass) > 0, "the soft grass voxel fractures under the tool");
        assert_eq!(dig_one(iron), 0, "the hard iron voxel resists the same tool");
    }

    /// The SHARED de-resolution primitive [`MatterSim::deposit_resting_grain`] — the single law used by
    /// BOTH the CPU settling and the GPU debris readback (`lib.rs`). A resting grain returns to the voxel
    /// grid matter-conservingly: exactly one grain → one voxel, deposited into the column's air-start
    /// cell, and NEVER inside a dynamic body (there it stays a grain). This is the on-demand-resolution
    /// principle in reverse — once the excitement passes, resolved matter goes back to bulk.
    #[test]
    fn deposit_resting_grain_conserves_matter_and_respects_bodies() {
        let mats = materials::load();
        let basalt = materials::index_of(&mats, "basalt");
        // 8³ world, one solid basalt voxel at (4,3,4): the column's air-start is y=4.
        let mut w = World {
            w: 8,
            h: 8,
            d: 8,
            voxels: vec![0; 8 * 8 * 8],
            max_top: 4,
        };
        w.set_voxel(4, 3, 4, Some(basalt));
        let before = w.solid_count();
        let c = w.center();
        assert_eq!(w.surface_top_voxel(4, 4), Some(4), "air-start above the one solid voxel");

        // A grain resting on that column (centered coords of the air-start cell).
        let grain = Vec3::new(4.5, 4.5, 4.5) - c;
        let mut sim = MatterSim::new(64);

        // 1. A body (the probe) sitting on that cell BLOCKS the deposit — the grain must stay a grain,
        //    matter conserved, no voxel conjured inside the solid object.
        let body = Sphere::new(Vec3::new(4.5, 4.5, 4.5) - c, 1.0, 1.0);
        assert!(
            !sim.deposit_resting_grain(&mut w, grain, basalt, std::slice::from_ref(&body)),
            "a grain must NOT deposit inside a dynamic body — it stays a grain"
        );
        assert_eq!(w.solid_count(), before, "blocked deposit conjures no matter");

        // 2. With no body it deposits into the air-start voxel — one grain becomes one voxel.
        assert!(
            sim.deposit_resting_grain(&mut w, grain, basalt, &[]),
            "an unobstructed resting grain deposits into its column"
        );
        assert_eq!(w.solid_count(), before + 1, "exactly one voxel gained (matter conserved)");
        assert_eq!(
            w.material_at(4, 4, 4),
            Some(basalt),
            "the deposited voxel carries the grain's material, in the air-start cell"
        );
        assert!(sim.take_dirty(), "a deposit marks the world dirty (it must remesh)");
        // The column grew by one — a second grain would stack on top, never overwrite.
        assert_eq!(w.surface_top_voxel(4, 4), Some(5), "air-start rose after the deposit");
    }

    #[test]
    fn matter_conserved_through_dig_and_settle() {
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let field = gravity::MassField::build(&w, &mats, 8);
        let before = w.solid_count();
        let surf = center_surface(&w);

        let mut sim = MatterSim::new(50_000);
        let n = sim.dig(&mut w, &mats, Vec3::new(0.0, surf - 1.5, 0.0), 3.0, 1.0e6);
        assert!(n > 0);
        // Right after digging: removed n voxels, spawned n particles.
        assert_eq!(w.solid_count() + sim.particle_count(), before);

        // Settle. The invariant (voxels + airborne particles == original) must hold every step.
        let mut settled = false;
        for _ in 0..40_000 {
            sim.step(&mut w, &field, &[], 5.0);
            assert_eq!(
                w.solid_count() + sim.particle_count(),
                before,
                "matter conserved each step"
            );
            if sim.particle_count() == 0 {
                settled = true;
                break;
            }
        }
        assert!(settled, "all debris should eventually settle");
        assert_eq!(
            w.solid_count(),
            before,
            "matter fully conserved after settling"
        );
    }

    #[test]
    fn materializing_terrain_conserves_matter_and_injects_no_energy() {
        // docs/24 Stage 3: turning a patch of terrain into grains must conserve MASS (voxels removed ==
        // grains made) and inject NO energy — grains are at rest, at the exact voxel centres (so both
        // kinetic AND gravitational potential energy are unchanged by the representation change).
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let before = w.solid_count();
        let surf = center_surface(&w);

        let mut sim = MatterSim::new(50_000);
        let n = sim.materialize_region(&mut w, &mats, Vec3::new(0.0, surf - 3.0, 0.0), 4.0);
        assert!(n > 0, "solid terrain in range should materialize");
        // Mass conserved: the world lost exactly n voxels, now held as n grains.
        assert_eq!(n, sim.particle_count());
        assert_eq!(w.solid_count() + sim.particle_count(), before);
        // No kinetic energy injected: every grain is at rest.
        assert!(
            sim.particles.iter().all(|p| p.vel == Vec3::ZERO),
            "materialized grains start at rest (no injected KE)"
        );
        // No potential energy injected: each grain sits at an integer+0.5 voxel centre (where its voxel
        // was), and the world no longer contains a solid voxel there.
        let center = w.center();
        for p in &sim.particles {
            let v = p.pos + center;
            assert!(
                (v.x - (v.x.floor() + 0.5)).abs() < 1e-4 && (v.y - (v.y.floor() + 0.5)).abs() < 1e-4,
                "grain sits at its former voxel centre"
            );
            assert!(
                w.material_at(v.x.floor() as i32, v.y.floor() as i32, v.z.floor() as i32).is_none(),
                "the voxel it came from is now air"
            );
        }
    }

    #[test]
    fn materialize_furrow_excavates_via_the_shared_law_conserving_matter() {
        // docs/28: the terrain meteor craters through the SAME impact.rs excavation the space band uses —
        // voxels inside the shared `Furrow` become grains carrying its shock-ejection velocity. Asserts:
        // matter conserved (voxels removed == grains made), grains keep their OWN material, sit BELOW the
        // surface, and an OBLIQUE strike carves a downrange-elongated furrow with lofted ejecta.
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let before = w.solid_count();
        let c = w.center();
        let (px, pz) = (c.x as i32, c.z as i32);
        let surf = w.surface_top_voxel(px, pz).unwrap() as f32 - c.y; // centered surface height
        // Impact just under the surface at the patch centre; oblique 45° in x–y → downrange +x; local up
        // +y (the surface normal under uniform surface gravity).
        let site = glam::DVec3::new(0.0, surf as f64 - 0.5, 0.0);
        let v_impact = glam::DVec3::new(1.0, -1.0, 0.0).normalize() * 17_000.0;
        let furrow = crate::impact::Furrow::new(site, glam::DVec3::Y, v_impact, 0.3, 8.0);

        let mut sim = MatterSim::new(200_000);
        // A 1000 kg Fe-Ni meteor → the impact-energy cap (docs/28); its ½·m·v² bounds the ejecta KE.
        let impactor_mass = 1_000.0;
        let n = sim.materialize_furrow(&mut w, &mats, &furrow, Vec3::ZERO, impactor_mass);
        assert!(n > 0, "solid terrain inside the furrow materializes into grains");
        assert_eq!(n, sim.particle_count());
        // Matter conserved: the world lost exactly n voxels, now held as n grains.
        assert_eq!(w.solid_count() + sim.particle_count(), before);

        // Every grain sits at its own former voxel centre and at/below the local surface (below the plane).
        for p in &sim.particles {
            assert!(p.pos.y <= surf + 1e-3, "grain above the surface: y={}", p.pos.y);
        }
        // OBLIQUE: elongated downrange (+x, site.x = 0), centroid pushed downrange of contact.
        let along: Vec<f32> = sim.particles.iter().map(|p| p.pos.x).collect();
        let across: Vec<f32> = sim.particles.iter().map(|p| p.pos.z).collect();
        let span = |v: &[f32]| {
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(
            span(&along) > span(&across),
            "oblique furrow elongated downrange (along {:.1} vs across {:.1})",
            span(&along),
            span(&across)
        );
        let cx = along.iter().sum::<f32>() / along.len() as f32;
        assert!(cx > 0.0, "furrow centroid downrange of contact, got {cx:.2}");
        // Ejecta lofted: some grains carry upward velocity from the shared ejection (not all at rest).
        let max_up = sim.particles.iter().map(|p| p.vel.y).fold(f32::MIN, f32::max);
        assert!(max_up > 0.0, "some grains are lofted upward (shared shock ejection), got {max_up:.3}");
        // EXACT ENERGY CONSERVATION (docs/28): a SMALL meteor's raw H-H ejecta KE exceeds the impact
        // energy ½·m·v² (the debris storm) — the cap scales it back so the total ejecta KE ≤ E_i. With
        // ground_vel = 0 the ejecta KE equals the absolute grain KE.
        let e_impact = 0.5 * impactor_mass as f64 * furrow.v_mag * furrow.v_mag;
        let ke: f64 = sim
            .particles
            .iter()
            .map(|p| 0.5 * p.mass as f64 * p.vel.length_squared() as f64)
            .sum();
        assert!(
            ke <= e_impact * (1.0 + 1e-6),
            "terrain ejecta KE {ke:.3e} J must not exceed the impact energy {e_impact:.3e} J"
        );
        // The excavated column spans the real layered strata, so grains carry more than one material
        // (grass cap over rock) — each keeps its OWN, not a bulk proxy.
        let distinct: std::collections::HashSet<usize> =
            sim.particles.iter().map(|p| p.material).collect();
        assert!(distinct.len() >= 2, "furrow cuts through layered strata (got {} materials)", distinct.len());
    }

    #[test]
    fn materialize_furrow_is_symmetric_for_a_vertical_strike() {
        // A VERTICAL meteor has no downrange direction, so the shared law excavates a SYMMETRIC bowl
        // (obliquity is what elongates a furrow — `Furrow::new`). Same matter conservation.
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let before = w.solid_count();
        let c = w.center();
        let (px, pz) = (c.x as i32, c.z as i32);
        let surf = w.surface_top_voxel(px, pz).unwrap() as f32 - c.y;
        let site = glam::DVec3::new(0.0, surf as f64 - 0.5, 0.0);
        let furrow = crate::impact::Furrow::new(site, glam::DVec3::Y, -glam::DVec3::Y * 17_000.0, 0.3, 8.0);

        let mut sim = MatterSim::new(200_000);
        let n = sim.materialize_furrow(&mut w, &mats, &furrow, Vec3::ZERO, 1_000.0);
        assert!(n > 0);
        assert_eq!(w.solid_count() + sim.particle_count(), before);
        let span = |sel: &dyn Fn(&Particle) -> f32| {
            let vals: Vec<f32> = sim.particles.iter().map(sel).collect();
            vals.iter().cloned().fold(f32::MIN, f32::max) - vals.iter().cloned().fold(f32::MAX, f32::min)
        };
        let (sx, sz) = (span(&|p| p.pos.x), span(&|p| p.pos.z));
        assert!(
            (sx / sz - 1.0).abs() < 0.3 && (sz / sx - 1.0).abs() < 0.3,
            "vertical strike excavates a symmetric bowl (x-span {sx:.1} ≈ z-span {sz:.1})"
        );
    }

    #[test]
    fn materialize_furrow_caps_terrain_ejecta_at_the_impact_energy() {
        // docs/28: `materialize_furrow` caps the total ejecta KE at the impact energy ½·m·v² (the SAME
        // exact-conservation cap the space band uses). This test drives the terrain furrow twice — once
        // UNCAPPED (impactor_mass = ∞, the raw declared H-H ejection) and once with a LIGHT impactor
        // whose delivered energy the raw ejection would exceed — and asserts the cap corrects it EXACTLY.
        //
        // HONEST FINDING (measured, docs/28): for the REAL 1000 kg terrain meteor the raw ejecta KE is
        // ~2.1e10 J — only ~0.15× its impact energy (1.445e11 J) — so at the f=1 bound the cap does NOT
        // bind and the current terrain scene is UNCHANGED by it (the observed debris storm is a velocity-
        // DISTRIBUTION tail, not an aggregate energy-conservation violation — see the module note / report).
        // We therefore use a LIGHTER impactor (whose ½·m·v² is below the raw ejecta KE) to exercise and
        // prove the binding path, and separately confirm the 1000 kg meteor is within budget.
        let mats = materials::load();
        let v_impact = glam::DVec3::new(1.0, -1.0, 0.0).normalize() * 17_000.0;

        // RAW (uncapped) ejecta KE of the real furrow geometry.
        let raw_ke = {
            let mut w = world::generate(&mats);
            let c = w.center();
            let surf = w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
            let site = glam::DVec3::new(0.0, surf as f64 - 0.5, 0.0);
            let furrow = crate::impact::Furrow::new(site, glam::DVec3::Y, v_impact, 0.31, 12.0);
            let mut sim = MatterSim::new(200_000);
            sim.materialize_furrow(&mut w, &mats, &furrow, Vec3::ZERO, f64::INFINITY);
            sim.particles
                .iter()
                .map(|p| 0.5 * p.mass as f64 * p.vel.length_squared() as f64)
                .sum::<f64>()
        };
        assert!(raw_ke > 0.0);

        // A LIGHT impactor whose impact energy is HALF the raw ejecta KE → the cap must bind.
        let e_impact = 0.5 * raw_ke;
        let impactor_mass = e_impact / (0.5 * v_impact.length_squared()); // ½·m·v² == e_impact
        let mut w = world::generate(&mats);
        let c = w.center();
        let surf = w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        let site = glam::DVec3::new(0.0, surf as f64 - 0.5, 0.0);
        let furrow = crate::impact::Furrow::new(site, glam::DVec3::Y, v_impact, 0.31, 12.0);
        let mut sim = MatterSim::new(200_000);
        let start = sim.particle_count();
        let n = sim.materialize_furrow(&mut w, &mats, &furrow, Vec3::ZERO, impactor_mass);
        assert!(n > 0, "the furrow excavates terrain");
        let ke: f64 = sim.particles[start..]
            .iter()
            .map(|p| 0.5 * p.mass as f64 * p.vel.length_squared() as f64)
            .sum();
        // Energy conserved exactly: the over-budget ejection is scaled down to the impact energy.
        assert!(
            (ke - e_impact).abs() / e_impact < 1e-3,
            "capped ejecta KE {ke:.3e} J == impact energy {e_impact:.3e} J (exact conservation)"
        );
        assert!(ke < raw_ke, "the cap actually reduced the ejecta KE ({ke:.3e} < raw {raw_ke:.3e})");

        // And the REAL 1000 kg meteor is within budget → the cap leaves it untouched (the honest finding).
        let e_1000kg = 0.5 * 1000.0 * v_impact.length_squared();
        assert!(
            raw_ke < e_1000kg,
            "the 1000 kg meteor's raw ejecta KE {raw_ke:.3e} J is within its impact energy {e_1000kg:.3e} J \
             (ratio {:.3}) — the f=1 cap does not bind for it",
            raw_ke / e_1000kg
        );
    }

    #[test]
    fn materialize_steep_terrain_turns_cliffs_into_grains_conserving_mass() {
        // docs/24 Path B: a vertical cliff a heightfield can't represent conservatively becomes loose
        // grains (talus) — mass conserved, grains at rest, and the terrain left behind is gentler.
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let c = w.center();
        let (px, pz) = (c.x as i32, c.z as i32);
        let surf = w.surface_top_voxel(px, pz).unwrap();
        // Carve a deep narrow pit → steep walls around it (a cliff the bilinear penalty would explode on).
        for y in (surf - 6)..surf {
            for (x, z) in [(px, pz), (px + 1, pz), (px, pz + 1), (px + 1, pz + 1)] {
                w.set_voxel(x, y, z, None);
            }
        }
        let after_dig = w.solid_count();

        let mut sim = MatterSim::new(50_000);
        let site = Vec3::new(0.0, surf as f32 - c.y, 0.0);
        let n = sim.materialize_steep_terrain(&mut w, &mats, site, 6.0, 3);
        assert!(n > 0, "the steep pit walls materialize into grains");
        assert_eq!(n, sim.particle_count());
        // Mass conserved by the materialize step: solid lost == grains gained.
        assert_eq!(after_dig - w.solid_count(), sim.particle_count());
        assert!(
            sim.particles.iter().all(|p| p.vel == Vec3::ZERO),
            "materialized cliff grains start at rest (no injected KE)"
        );
    }

    #[test]
    fn a_granite_cliff_holds_while_the_dirt_above_it_slumps() {
        // Robin's antithesis: granite is strong enough to STAND as a cliff (we see them in nature). A pit
        // dug through the dirt cap INTO the granite bulk makes a wall that is weak dirt on top, granite
        // below. The dirt slumps to talus; the GRANITE HOLDS — no granite grains are shed. Emergent from
        // strength (critical cliff height ≈ σ/ρg): dirt ~0.4 m, granite ~450 m.
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let c = w.center();
        let (px, pz) = (c.x as i32, c.z as i32);
        let surf = w.surface_top_voxel(px, pz).unwrap();
        for y in (surf - 16)..surf {
            for (x, z) in [(px, pz), (px + 1, pz), (px, pz + 1), (px + 1, pz + 1)] {
                w.set_voxel(x, y, z, None); // pit through the ~10 m dirt into the granite
            }
        }
        let mut sim = MatterSim::new(50_000);
        let site = Vec3::new(0.0, surf as f32 - c.y, 0.0);
        let n = sim.materialize_steep_terrain(&mut w, &mats, site, 6.0, 3);
        assert!(n > 0, "the weak dirt above the cliff slumps to grains");
        let granite = materials::index_of(&mats, "granite");
        assert!(
            sim.particles.iter().all(|p| p.material != granite),
            "the granite cliff HOLDS — no granite grains slump (only the dirt above does)"
        );
    }

    #[test]
    fn the_impulse_deposits_exactly_the_impactor_momentum() {
        // docs/24 Stage 2: the driver conserves momentum — Σ mᵢ·vᵢ over the core grains equals the
        // deposited momentum vector, exactly. No scripted ejecta speed; the meteor's real momentum.
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let surf = center_surface(&w);
        let mut sim = MatterSim::new(50_000);
        let site = Vec3::new(0.0, surf - 3.0, 0.0);
        let start = sim.particle_count();
        sim.materialize_region(&mut w, &mats, site, 4.0);

        let momentum = Vec3::new(0.0, -1.7e7, 0.0); // a downward meteor impulse (kg·m/s)
        let core = sim.deposit_impulse(start, site, momentum, 3.0);
        assert!(core > 0, "grains in the coupling core receive the impulse");

        // Total momentum of the affected grains == the deposited momentum (to f32 tolerance).
        let total: Vec3 = sim.particles[start..]
            .iter()
            .map(|p| p.vel * p.mass)
            .fold(Vec3::ZERO, |a, b| a + b);
        assert!(
            (total - momentum).length() / momentum.length() < 1e-4,
            "momentum conserved: got {total:?} vs {momentum:?}"
        );
        // And only a modest fraction of a fast impactor's kinetic energy becomes bulk motion (the rest
        // is heat): with the core mass ≫ impactor mass, ½·p²/M_core ≪ the meteor's ½mv².
        let ke_bulk: f32 = sim.particles[start..]
            .iter()
            .map(|p| 0.5 * p.mass * p.vel.length_squared())
            .sum();
        let meteor_ke = 0.5 * 1000.0 * 17000.0 * 17000.0; // the p above is 1000 kg × 17 km/s
        assert!(
            ke_bulk < 0.20 * meteor_ke,
            "most impact energy is heat, not ejecta motion (bulk {ke_bulk:.2e} vs {meteor_ke:.2e})"
        );
    }

    #[test]
    fn shock_heat_is_hottest_at_the_impact_and_conserves_the_energy() {
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let surf = center_surface(&w);
        let mut sim = MatterSim::new(50_000);
        let site = Vec3::new(0.0, surf - 3.0, 0.0);
        let start = sim.particle_count();
        sim.materialize_region(&mut w, &mats, site, 5.0);

        let heat = 1.0e11f32;
        sim.deposit_shock_heat(start, site, heat, &mats);

        // Energy conserved: Σ (ΔT · ρ · c · V) over grains ≈ the deposited heat (V = 1 m³).
        let deposited: f32 = sim.particles[start..]
            .iter()
            .map(|p| {
                let m = &mats[p.material];
                let c = m.thermal.as_ref().map_or(1000.0, |t| t.specific_heat);
                (p.temp_k - REF_TEMP_K) * m.density.max(1.0) * c
            })
            .sum();
        assert!(
            (deposited - heat).abs() / heat < 1e-3,
            "shock heat conserved: {deposited:.3e} vs {heat:.3e}"
        );
        // Hottest grain is near the impact, coolest near the rim (radial gradient, not uniform).
        let nearest = sim.particles[start..]
            .iter()
            .min_by(|a, b| {
                (a.pos - site).length().total_cmp(&(b.pos - site).length())
            })
            .unwrap();
        let farthest = sim.particles[start..]
            .iter()
            .max_by(|a, b| {
                (a.pos - site).length().total_cmp(&(b.pos - site).length())
            })
            .unwrap();
        assert!(
            nearest.temp_k > farthest.temp_k,
            "the core is hotter than the rim ({} vs {})",
            nearest.temp_k,
            farthest.temp_k
        );
    }

    #[test]
    fn vapor_expansion_converts_superheat_to_radial_motion_conserving_energy() {
        // docs/24 (Robin's model): superheat past vaporization becomes RADIAL ejecta KE — the honest,
        // conservative engine of crater ejection (thermal → kinetic, nothing invented).
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let surf = center_surface(&w);
        let mut sim = MatterSim::new(50_000);
        // Below the 10 m dirt cap, in the granite bulk (dirt/grass carry no thermal data, so they can't
        // vaporize — an honest data gap, not a bug; the rock does the vaporizing).
        let site = Vec3::new(0.0, surf - 14.0, 0.0);
        let start = sim.particle_count();
        sim.materialize_region(&mut w, &mats, site, 4.0);
        // Deposit enough heat to drive the core WELL past vaporization (superheat).
        sim.deposit_shock_heat(start, site, 1.0e13, &mats);

        let thermal = |sim: &MatterSim| -> f32 {
            sim.particles[start..]
                .iter()
                .map(|p| {
                    let m = &mats[p.material];
                    let c = m.thermal.as_ref().map_or(1000.0, |t| t.specific_heat);
                    (p.temp_k - REF_TEMP_K) * m.density.max(1.0) * c
                })
                .sum()
        };
        let ke = |sim: &MatterSim| -> f32 {
            sim.particles[start..].iter().map(|p| 0.5 * p.mass * p.vel.length_squared()).sum()
        };
        let (th0, ke0) = (thermal(&sim), ke(&sim));

        let e_expand = sim.deposit_vapor_expansion(start, site, &mats);
        assert!(e_expand > 0.0, "superheated matter drives an expansion");

        // Energy conserved: the thermal energy removed equals the kinetic energy added equals E_expand.
        let thermal_lost = th0 - thermal(&sim);
        let ke_gained = ke(&sim) - ke0;
        assert!(
            (thermal_lost - e_expand).abs() / e_expand < 1.0e-3,
            "thermal removed == E_expand ({thermal_lost:.3e} vs {e_expand:.3e})"
        );
        assert!(
            (ke_gained - e_expand).abs() / e_expand < 1.0e-2,
            "kinetic added == E_expand — energy conserved ({ke_gained:.3e} vs {e_expand:.3e})"
        );
        // The vapor pushes only its BUBBLE WALL outward (radially — pure geometry, no assigned
        // direction). Most grains stay put at t=0; the crater bowl + up-and-out curtain EMERGE from
        // contact over time on the GPU (we don't impose them here).
        let pushed: Vec<_> = sim.particles[start..].iter().filter(|p| p.vel.length() > 1.0).collect();
        assert!(!pushed.is_empty(), "the vapor pushes its bubble wall");
        assert!(
            pushed
                .iter()
                .all(|p| p.vel.dot((p.pos - site).normalize_or_zero()) > 0.0),
            "the pushed wall moves radially outward"
        );
    }

    /// SERVER-SIDE DIAGNOSTIC (not an assertion): run the exact operator sequence the meteor uses on a
    /// real generated world and report what the debris actually does — so we can see whether a crater
    /// should form, headlessly. Run with: `cargo test -p engine meteor_impact_diagnostic -- --nocapture`.
    #[test]
    fn meteor_impact_diagnostic() {
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let surf = center_surface(&w);
        let mut sim = MatterSim::new(200_000);

        // Mirror lib.rs::meteor exactly.
        let (mmass, mspeed) = (1000.0f32, 17000.0f32);
        let energy = 0.5 * mmass * mspeed * mspeed;
        let hit = Vec3::new(0.0, surf - 0.5, 0.0); // strike at the surface
        let hv = hit + w.center();
        let hit_mat = w.material_at(hv.x as i32, hv.y as i32, hv.z as i32);
        let strength = hit_mat.map_or(1.2e7, |m| mats[m].fracture_strength);
        let crater_r =
            crate::damage::crater_radius(crate::damage::crater_volume(energy as f64, strength as f64));
        let mat_r = (crater_r as f32).min(14.0);
        let _ = hit_mat;

        let start = sim.particle_count();
        let n = sim.materialize_region(&mut w, &mats, hit, mat_r);
        let momentum = Vec3::new(0.0, -1.0, 0.0) * (mmass * mspeed);
        let core_r = (mat_r * 0.35).max(2.0);
        sim.deposit_impulse(start, hit, momentum, core_r);
        let bulk_ke: f32 =
            sim.particles[start..].iter().map(|p| 0.5 * p.mass * p.vel.length_squared()).sum();
        sim.deposit_shock_heat(start, hit, (energy - bulk_ke).max(0.0), &mats);
        let e_expand = sim.deposit_vapor_expansion(start, hit, &mats);

        // Composition of what got materialized.
        let mut comp: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for p in &sim.particles[start..] {
            *comp.entry(mats[p.material].id.as_str()).or_default() += 1;
        }
        let has_thermal = sim.particles[start..]
            .iter()
            .filter(|p| mats[p.material].thermal.is_some())
            .count();
        let hottest = sim.particles[start..].iter().map(|p| p.temp_k).fold(0.0f32, f32::max);
        let vaporized = sim.particles[start..]
            .iter()
            .filter(|p| {
                crate::damage::vapor_energy_density(&mats[p.material])
                    .map_or(false, |ev| {
                        let m = &mats[p.material];
                        let c = m.thermal.as_ref().map_or(1000.0, |t| t.specific_heat);
                        (m.density * c * (p.temp_k - REF_TEMP_K)) as f64 >= ev
                    })
            })
            .count();
        let moving_up = sim.particles[start..].iter().filter(|p| p.vel.y > 1.0).count();
        let outward = sim.particles[start..]
            .iter()
            .filter(|p| {
                let h = Vec3::new(p.pos.x - hit.x, 0.0, p.pos.z - hit.z);
                h.length() > 0.5 && Vec3::new(p.vel.x, 0.0, p.vel.z).dot(h.normalize_or_zero()) > 1.0
            })
            .count();
        let max_speed = sim.particles[start..].iter().map(|p| p.vel.length()).fold(0.0f32, f32::max);

        println!("\n=== METEOR IMPACT DIAGNOSTIC (server-side) ===");
        println!("energy {energy:.2e} J, crater_r {crater_r:.1} m (capped to mat_r {mat_r:.1} m)");
        println!("surface strength used: {strength:.2e} Pa (material at hit)");
        println!("materialized {n} grains; composition: {comp:?}");
        println!("  with thermal data (can vaporize): {has_thermal}/{n}");
        println!("hottest grain: {hottest:.0} K; grains AT/PAST vaporization: {vaporized}");
        println!("VAPOR expansion energy E_expand: {e_expand:.2e} J");
        println!(
            "ejection: {moving_up} grains moving UP (>1 m/s), {outward} moving OUTWARD, max speed {max_speed:.1} m/s"
        );
        println!("=== NB: this is the t=0 SETUP. The vapor pushes only its bubble wall; the crater bowl");
        println!("    and up-and-out curtain EMERGE from contact over ~10 s on the GPU (forward sim). ===\n");

        // Regression guard. NOT an assumption that meteors vaporize — a CONSEQUENCE we observe: at
        // 17 km/s the impactor's specific energy (½v² ≈ 1.4e8 J/kg) far exceeds soil's vaporization energy
        // (~1e10 J/m³ ÷ ρ), so the deposited energy density crosses the material's own threshold and vapor
        // EMERGES. A weaker impact, or a refractory target, would cross no threshold and make no vapor
        // crater — correctly. So this checks the *material-property-driven consequence* holds for this
        // energetic case (which is how we know energy→heat→vaporization still couples), not that
        // vaporization is imposed. We assert NO grain count and NO crater size — those emerge (forward sim).
        assert!(
            vaporized > 0,
            "this 17 km/s impact's energy density exceeds the target's vaporization threshold, so vapor \
             emerges (a consequence, not an assumption)"
        );
        assert!(e_expand > 0.0, "the vaporized core has superheat to expand");
        assert!(outward > 0 && max_speed > 50.0, "the vapor push drives an excavation front");
    }

    #[test]
    fn supported_terrain_has_no_floating() {
        let mats = materials::load();
        let w = world::generate(&mats);
        assert!(
            w.find_unsupported().is_empty(),
            "intact terrain is fully supported (connected to the base)"
        );
    }

    #[test]
    fn collapse_drops_floating_and_conserves() {
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let field = gravity::MassField::build(&w, &mats, 8);
        let rock = materials::index_of(&mats, "granite");
        let before = w.solid_count();

        // An isolated voxel high in the air, disconnected from the base.
        let fy = w.max_top as i32 + 2;
        w.set_voxel(5, fy, 5, Some(rock));
        assert_eq!(w.find_unsupported(), vec![(5, fy, 5)]);

        let mut sim = MatterSim::new(50_000);
        let g = 9.88; // emergent surface gravity (Engine::surface_g)
        let n = sim.collapse(&mut w, &mats, g);
        assert_eq!(n, 1, "only the isolated voxel collapses");
        assert!(w.find_unsupported().is_empty(), "nothing floating remains");
        assert_eq!(w.solid_count() + sim.particle_count(), before + 1);

        // It falls and re-settles into the grid.
        for _ in 0..40_000 {
            sim.step(&mut w, &field, &[], 5.0);
            if sim.particle_count() == 0 {
                break;
            }
        }
        assert_eq!(sim.particle_count(), 0, "collapsed matter settles");
        assert_eq!(w.solid_count(), before + 1, "matter conserved");
    }

    #[test]
    fn collapse_drops_an_undercut_overhang_and_conserves_matter() {
        // docs/28 (e): an overhanging lip attached SIDEWAYS to a wall — nothing beneath it — past its
        // material's cantilever reach must collapse, and collapse conserves matter (voxels removed ==
        // particles spawned). This is the crater-lip bug: a soil overhang the old 6-connectivity model
        // called "supported" because it touched the rim. Grass reach ≈ 1 m, so a 6-voxel grass overhang
        // sheds everything past the first voxel.
        let mats = materials::load();
        let g = 9.88;
        let grass = materials::index_of(&mats, "grass");
        // A support wall (column to base at x=0) with a 6-voxel grass overhang jutting over air at y0.
        let (w_, h_, d_) = (48usize, 24usize, 8usize);
        let mut w = World {
            w: w_,
            h: h_,
            d: d_,
            voxels: vec![0u16; w_ * h_ * d_],
            max_top: 16,
        };
        let (y0, z0, len) = (15i32, 4i32, 6i32);
        for y in 0..=y0 {
            w.set_voxel(0, y, z0, Some(grass));
        }
        for x in 1..=len {
            w.set_voxel(x, y0, z0, Some(grass)); // overhang: nothing below it
        }
        let before = w.solid_count();

        let mut sim = MatterSim::new(50_000);
        let n = sim.collapse(&mut w, &mats, g);
        assert!(n > 0, "the undercut grass overhang collapses");
        // Matter conserved: exactly n voxels removed, n particles spawned.
        assert_eq!(n, sim.particle_count(), "one particle spawned per collapsed voxel");
        assert_eq!(w.solid_count() + sim.particle_count(), before, "matter conserved");
        // After collapse the standing matter is self-consistently supported (fixpoint reached).
        assert!(
            w.find_structurally_unsupported(&mats, g).is_empty(),
            "nothing unsupported remains after collapse"
        );
        // The support wall survives (it is a column to the base).
        assert!(w.is_solid(0, y0, z0), "the support wall holds");
    }

    #[test]
    fn particle_transfers_momentum_to_a_body() {
        // Unified dynamics: a flung clod of debris actually shoves the probe (they are the same kind
        // of matter), and total linear momentum is conserved through the contact.
        let mut sim = MatterSim::new(10);
        let mut probe = Sphere::new(Vec3::ZERO, 100.0, 2.0);
        // Light, fast particle already overlapping the probe, moving straight at it (−x).
        sim.particles.push(Particle {
            pos: Vec3::new(2.4, 0.0, 0.0),
            vel: Vec3::new(-1.0, 0.0, 0.0),
            material: 0,
            mass: 5.0,
            temp_k: REF_TEMP_K,
            resting_frames: 0,
        });
        let momentum_before = probe.mass * probe.vel + sim.particles[0].mass * sim.particles[0].vel;

        sim.couple_body(&mut probe, 0.016);

        assert!(probe.vel.x < 0.0, "the impact shoves the probe along −x");
        assert!(!probe.resting, "contact wakes the body");
        let momentum_after = probe.mass * probe.vel + sim.particles[0].mass * sim.particles[0].vel;
        assert!(
            (momentum_after - momentum_before).length() < 1e-3,
            "linear momentum conserved (before {momentum_before:?}, after {momentum_after:?})"
        );
    }

    #[test]
    fn debris_does_not_settle_inside_a_body() {
        // A particle settling in a column occupied by the probe must NOT deposit a voxel inside the
        // probe's volume — matter piles on the body, never through it. (This is the specific fakery
        // that made the probe appear to "rest on nothing": debris re-materialising under it.)
        let n = 16usize;
        let mut voxels = vec![0u16; n * n * n];
        for y in 0..2 {
            for z in 0..n {
                for x in 0..n {
                    voxels[(y * n + z) * n + x] = 1; // two solid ground layers
                }
            }
        }
        let mut w = World {
            w: n,
            h: n,
            d: n,
            voxels,
            max_top: 2,
        };
        let mats = materials::load();
        let field = gravity::MassField::build(&w, &mats, 4);

        // Probe hovering just above the ground, centred on the origin column.
        let probe = Sphere::new(Vec3::new(0.0, 1.0, 0.0), 100.0, 1.5);
        // Debris at rest in that same column, exactly where a deposit would land.
        let mut sim = MatterSim::new(10);
        sim.particles.push(Particle {
            pos: Vec3::new(0.5, 1.0, 0.5),
            vel: Vec3::ZERO,
            material: 0,
            mass: 1.0,
            temp_k: REF_TEMP_K,
            resting_frames: 0,
        });
        let solids_before = w.solid_count();

        sim.step(&mut w, &field, std::slice::from_ref(&probe), 0.5);

        assert_eq!(
            w.solid_count(),
            solids_before,
            "no voxel deposited inside the body"
        );
        assert_eq!(
            sim.particle_count(),
            1,
            "the blocked debris survives (rests on the body), it is not deleted"
        );
    }

    #[test]
    fn impact_is_material_and_scale_invariant() {
        // The unified impact operator (docs/18): one call, response from material + energy.
        let mats = materials::load();
        let surf = center_surface(&world::generate(&mats));

        // Material invariance: the SAME energy craters soft ground but not deep granite.
        let e = 5.0e6;
        let mut wd = world::generate(&mats);
        let mut sd = MatterSim::new(200_000);
        let n_soft = sd.impact(
            &mut wd,
            &mats,
            Vec3::new(0.0, surf - 1.5, 0.0),
            Vec3::NEG_Y,
            e,
        );
        assert!(n_soft > 0, "a modest impact craters soft ground");

        let mut wg = world::generate(&mats);
        let mut sg = MatterSim::new(200_000);
        let n_rock = sg.impact(
            &mut wg,
            &mats,
            Vec3::new(0.0, surf - 40.0, 0.0),
            Vec3::NEG_Y,
            e,
        );
        assert_eq!(
            n_rock, 0,
            "the same energy can't crack deep granite (material-invariant)"
        );

        // Scale invariance: on the same granite, more energy → a bigger crater (the same call).
        let mut w1 = world::generate(&mats);
        let mut s1 = MatterSim::new(200_000);
        let small = s1.impact(
            &mut w1,
            &mats,
            Vec3::new(0.0, surf - 40.0, 0.0),
            Vec3::NEG_Y,
            1.0e8,
        );
        let mut w2 = world::generate(&mats);
        let mut s2 = MatterSim::new(200_000);
        let big = s2.impact(
            &mut w2,
            &mats,
            Vec3::new(0.0, surf - 40.0, 0.0),
            Vec3::NEG_Y,
            1.0e9,
        );
        assert!(
            small > 0 && big > small,
            "the crater grows with energy (small {small}, big {big})"
        );

        // Liquid: a pond yields to even a gentle impact (pebble in a pond) — σ≈0, so it splashes.
        let water = materials::index_of(&mats, "water");
        let n = 12usize;
        let mut pond = World {
            w: n,
            h: n,
            d: n,
            voxels: vec![water as u16 + 1; n * n * n],
            max_top: n,
        };
        let mut sp = MatterSim::new(200_000);
        let splash = sp.impact(&mut pond, &mats, Vec3::ZERO, Vec3::NEG_Y, 50.0);
        assert!(
            splash > 0,
            "a gentle impact still displaces water (a splash)"
        );
    }

    #[test]
    fn voxel_crater_matches_the_coarse_damage_summary() {
        // The LOD bridge (docs/19): the number of voxels the impact operator excavates equals the
        // crater VOLUME the coarse-scale summary predicts from the same energy + material. So a
        // celestial summary and a zoomed-in voxel crater describe the same event — damage is conserved
        // across level of detail.
        let mats = materials::load();
        let gi = materials::index_of(&mats, "granite");
        let sigma = mats[gi].fracture_strength; // Pa
        let n = 40usize;
        // Uniform granite, so the energy budget (not geometry) sets the crater — a clean bridge test.
        let mut w = World {
            w: n,
            h: n,
            d: n,
            voxels: vec![gi as u16 + 1; n * n * n],
            max_top: n,
        };
        let energy = 200.0 * sigma; // enough to excavate ~200 voxels
        let mut sim = MatterSim::new(200_000);
        let carved = sim.impact(&mut w, &mats, Vec3::ZERO, Vec3::NEG_Y, energy);

        let predicted = crate::damage::crater_volume(energy as f64, sigma as f64); // = 200 m³
        assert!(
            (carved as f64 - predicted).abs() <= 2.0,
            "voxel crater {carved} ≈ summary volume {predicted} (same σ·V accounting)"
        );
    }

    #[test]
    fn a_big_impact_melts_the_centre_and_leaves_the_rim_cold() {
        // Impact ejecta carry a temperature that peaks at the contact and falls to cold at the rim
        // (docs/20): the centre glows molten, the rim is cold rubble — one event, a radial gradient.
        let mats = materials::load();
        let bi = materials::index_of(&mats, "basalt");
        let melt = mats[bi].thermal.as_ref().unwrap().melt_point;
        let n = 40usize;
        let mut w = World {
            w: n,
            h: n,
            d: n,
            voxels: vec![bi as u16 + 1; n * n * n],
            max_top: n,
        };
        let mut sim = MatterSim::new(500_000);
        // Enough energy that the concentrated core exceeds basalt's melting point.
        sim.impact(&mut w, &mats, Vec3::ZERO, Vec3::NEG_Y, 1.5e11);

        let hottest = sim
            .particles
            .iter()
            .map(|p| p.temp_k)
            .fold(0.0f32, f32::max);
        let coldest = sim
            .particles
            .iter()
            .map(|p| p.temp_k)
            .fold(f32::MAX, f32::min);
        assert!(
            hottest > melt,
            "the centre melts (hottest {hottest} K > melt {melt} K)"
        );
        assert!(
            (coldest - REF_TEMP_K).abs() < 1.0,
            "the rim stays cold (coldest {coldest} K ≈ {REF_TEMP_K} K)"
        );
    }

    #[test]
    fn a_colossal_impact_vaporizes_the_core() {
        // Enough energy that the concentrated core passes basalt's boiling point → the phase class is
        // Vaporized (docs/20), which the impact operator turns into fast, incandescent gas/plasma.
        let mats = materials::load();
        let bi = materials::index_of(&mats, "basalt");
        let boil = mats[bi].thermal.as_ref().unwrap().boil_point;
        let n = 40usize;
        let mut w = World {
            w: n,
            h: n,
            d: n,
            voxels: vec![bi as u16 + 1; n * n * n],
            max_top: n,
        };
        let mut sim = MatterSim::new(500_000);
        sim.impact(&mut w, &mats, Vec3::ZERO, Vec3::NEG_Y, 1.0e12);
        let hottest = sim
            .particles
            .iter()
            .map(|p| p.temp_k)
            .fold(0.0f32, f32::max);
        assert!(
            hottest > boil,
            "the core vaporizes (hottest {hottest} K > boil {boil} K)"
        );
    }
}
