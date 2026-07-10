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

// DEBT (physical honesty): `DRAG` is a mild per-step velocity damping. There is **no atmosphere**
// modelled, so in vacuum this is not real drag — it's a numerical stabilizer standing in for one.
// Keep it only until proper contact damping / an actual fluid (pressure + drag) exists; don't let it
// masquerade as physics. See docs/16 (no-fakery) and docs/15 (honesty invariant).
pub const DRAG: f32 = 0.9995;
pub const CONTACT_DAMP: f32 = 0.35; // energy kept after touching ground
pub const SETTLE_SPEED: f32 = 0.02; // below this, a grounded particle deposits into the grid
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
            let ke_each = 0.3 * energy / ejecta.len() as f32;
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

    /// Structural collapse: detach every voxel no longer connected to the anchored base into a
    /// falling particle (starting from rest). Run after an edit that may have undercut or isolated
    /// matter (a dig). One pass suffices — `find_unsupported` returns the complete disconnected set,
    /// so the remainder is fully supported. Returns the number collapsed.
    pub fn collapse(&mut self, world: &mut World, materials: &[Material]) -> usize {
        let center = world.center();
        let mut n = 0;
        for (x, y, z) in world.find_unsupported() {
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
            }
        }
        if n > 0 {
            self.dirty = true;
        }
        n
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
                if p.vel.length() < SETTLE_SPEED || p.resting_frames > SETTLE_FRAMES {
                    // Deposit into the column's air-start voxel (stacks / refills the crater) — unless
                    // a dynamic body occupies that cell, in which case the debris stays a particle and
                    // rests on the body (coupling resolves the contact); we never inject matter inside
                    // a solid object. (If the column is full it also stays, rather than being deleted —
                    // matter is conserved.)
                    let mut settled = false;
                    if let Some(ty) = world.surface_top_voxel(xi, zi) {
                        if (ty as usize) < world.h {
                            let cell = Vec3::new(xi as f32 + 0.5, ty as f32 + 0.5, zi as f32 + 0.5)
                                - center;
                            let inside_body = bodies
                                .iter()
                                .any(|b| (cell - b.pos).length() < b.radius + PARTICLE_HALF);
                            if !inside_body {
                                world.set_voxel(xi, ty, zi, Some(p.material));
                                self.dirty = true;
                                settled = true;
                            }
                        }
                    }
                    if settled {
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

        // The same tool deep in granite (1.2e7 Pa) removes nothing.
        let mut w2 = world::generate(&mats);
        let mut hard = MatterSim::new(50_000);
        let n_rock = hard.dig(&mut w2, &mats, Vec3::new(0.0, surf - 30.0, 0.0), 3.0, 1.0e6);
        assert_eq!(
            n_rock, 0,
            "granite resists a tool weaker than its fracture strength"
        );

        // A stronger blast (2e7 Pa) *does* break the rock.
        let mut w3 = world::generate(&mats);
        let mut blast = MatterSim::new(50_000);
        let n_blast = blast.dig(&mut w3, &mats, Vec3::new(0.0, surf - 30.0, 0.0), 3.0, 2.0e7);
        assert!(n_blast > 0, "a strong enough blast breaks granite");
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
        let n = sim.collapse(&mut w, &mats);
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
