//! **Re-coherence — the batch downward rung from a settled particle field to standable ground**
//! (docs/61).
//!
//! docs/44 §6 names demotion-on-quiescence as the cheap half of resolution-by-necessity, and the
//! docs/46 ledger (row 6) records that the de-resolution ladder's downward rungs sat without a batch
//! trigger: once an impact's excitement passes, whatever remains a particle stays a particle forever
//! — a bare ball of frozen matter where the ground should be. This module is the trigger and the
//! rung: a SETTLED region of any particle field bins back into the voxel [`World`], conserving mass
//! and material identity, and the existing surface-nets mesher then draws the result as walkable
//! ground. The mesh is only the render of the demotion; the physics here is the criterion and the
//! conservation.
//!
//! **The criterion is physical, not a frame count** (the `SETTLE_FRAMES` lesson, burned down in
//! docs/57 #4: a frame count makes the moment matter stops being matter depend on the host's step
//! rate). Both halves derive from the local gravity `g` and the field's own cell size:
//!
//! - *Quiescent speed* `v_q = sqrt(2 g Δ)`: below it a particle's kinetic energy cannot buy a
//!   one-cell rise (`½v² < gΔ`), so its remaining motion is sub-resolution — the static field and
//!   the live particle agree to within the representation's own quantum.
//! - *Sustained interval* `t_q = sqrt(2 Δ / g)`: one cell free-fall time, the region's own dynamical
//!   time at the binning resolution. Quiet for one `t_q` means nothing could have crossed a cell in
//!   the window the demotion takes effect, which is docs/44 §6's bound stated concretely.
//!
//! Neither number is a dial: change the gravity or the cell and both move as the physics says.
//!
//! **Conservation is the contract.** Mass in = voxels out × the material's own voxel quantum
//! (`ρ · Δ³`) + a remainder that STAYS particles. Matter is never deleted to lower a count, and a
//! grain's material identity survives the rung — gravel comes back as gravel voxels, never as
//! "terrain". Deposition itself goes through the ONE grain→voxel law ([`crate::matter::deposit_grain`])
//! so this rung and the per-grain settle path cannot disagree about where matter may return.
//! **The crossing's energy is measured, not zeroed** (docs/46 row 17). The voxel store carries no
//! thermal state, so a binned grain's remaining kinetic energy (settling IS dissipation) and its
//! carried heat have nowhere physical to land yet; [`Recohered::binned_kinetic_j`] and
//! [`Recohered::binned_heat_j`] book both at the crossing so the loss is visible in the audit.
//! Remainder grains keep their own motion and temperature. The deferred computation is the
//! voxel-side thermal sink itself; `Aggregate::drain_settled` shares the same IOU.

use crate::body::Sphere;
use crate::materials::Material;
use crate::world::World;
use glam::Vec3;

/// The voxel [`World`]'s cell edge (m). Voxel indices ARE metres by construction throughout the
/// world/mesher/matter path (one detached voxel is `ρ · 1 m³` of its material — see
/// `MatterSim::dig`), so the binning resolution this rung demotes to is fixed by the store itself,
/// not chosen here.
const CELL_M: f32 = 1.0;

/// Speed (m/s) below which motion is sub-resolution at binning scale `cell_m` under gravity `g`:
/// `½v² < g·Δ` — not enough kinetic energy to rise one cell, so the static field can represent the
/// particle without lying by more than the field's own quantum.
pub fn quiescent_speed(g: f32, cell_m: f32) -> f32 {
    (2.0 * g.max(0.0) * cell_m).sqrt()
}

/// The sustained-quiet interval (s): one cell free-fall time `sqrt(2Δ/g)`, the dynamical time of the
/// binning resolution. In free space (`g ≤ 0`) nothing ever "settles onto ground", so the interval
/// is infinite and the rung refuses — honestly — rather than binning floating matter.
pub fn quiescent_interval_s(g: f32, cell_m: f32) -> f32 {
    if g <= 0.0 {
        return f32::INFINITY;
    }
    (2.0 * cell_m / g).sqrt()
}

/// Tracks how long a region has been continuously quiet, in SECONDS of simulated time. The caller
/// feeds it the region's peak particle speed each physics step; one observation above the quiescent
/// speed resets the clock, because "sustained" means continuous — a region that jolts mid-window has
/// not settled, however quiet the average.
#[derive(Clone, Copy, Debug, Default)]
pub struct SettleGauge {
    quiet_s: f32,
}

impl SettleGauge {
    pub fn new() -> Self {
        SettleGauge { quiet_s: 0.0 }
    }

    /// Feed one physics step: the region's peak particle speed over the `dt` seconds just simulated.
    pub fn observe(&mut self, peak_speed: f32, g: f32, dt: f32) {
        if peak_speed < quiescent_speed(g, CELL_M) {
            self.quiet_s += dt;
        } else {
            self.quiet_s = 0.0;
        }
    }

    /// Has the region been quiet for at least its own cell dynamical time?
    pub fn settled(&self, g: f32) -> bool {
        self.quiet_s > 0.0 && self.quiet_s >= quiescent_interval_s(g, CELL_M)
    }

    /// Re-arm after a demotion or a fresh disturbance.
    pub fn reset(&mut self) {
        self.quiet_s = 0.0;
    }

    /// Continuous quiet accumulated so far (s) — exposed for refusal reporting and HUDs.
    pub fn quiet_seconds(&self) -> f32 {
        self.quiet_s
    }
}

/// One particle of a field, in the [`World`]'s centered coordinates — the scene-agnostic shape any
/// container can adapt to (the CPU `matter::Particle` today; an SPH readback or an `Aggregate` wreck
/// are the flagged next consumers, docs/61).
#[derive(Clone, Copy, Debug)]
pub struct FieldGrain {
    pub pos: Vec3,
    pub vel: Vec3,
    /// kg — real mass, not assumed voxel-quantized; the rung bins whatever it is given.
    pub mass: f32,
    /// Index into the material DB. Identity survives the rung.
    pub material: usize,
    /// Kelvin. Kept on remainder grains; the binned mass's heat above ambient is measured into
    /// [`Recohered::binned_heat_j`] at the crossing (no voxel-side thermal sink yet, see module doc).
    pub temp_k: f32,
}

/// Why the rung said no. Refusal is the criterion doing its job (docs/44: the test is mostly a
/// rejection test) — a still-moving region binned into a static field would delete real motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    /// The region has not been continuously quiet for one cell dynamical time.
    NotSustained { quiet_s: f32, needed_s: f32 },
    /// A particle still carries enough kinetic energy to cross a cell.
    StillMoving { speed: f32, limit: f32 },
}

/// What the rung did: how much matter became field again, and what honestly could not.
#[derive(Clone, Debug, Default)]
pub struct Recohered {
    /// Voxels written back into the world.
    pub voxels: usize,
    /// Mass (kg) those voxels claim — exactly `Σ voxels × ρ_mat · Δ³` by construction.
    pub deposited_mass: f64,
    /// Matter the field could not take as whole voxels: sub-quantum column remainders, and columns
    /// the one deposit law refused (full to the sky, inside a dynamic body, sea with no room to
    /// displace). These STAY particles — the rung never deletes matter to lower a count.
    pub remainder: Vec<FieldGrain>,
    /// J. Kinetic energy that crossed the rung and found no sink: energy carried IN by a column's
    /// grains minus energy carried back OUT by its remainder. Settling is dissipation, so this
    /// should become heat on the receiving side, and the voxel [`World`] carries no thermal state
    /// at all (no temperature per voxel, no heat raster). Until that sink exists (the deferred
    /// computation, docs/46 row 17) the rung MEASURES the loss here instead of zeroing it
    /// silently. Bounded per grain by `½ m v_q² = m g Δ`, the quiescence criterion's own quantum.
    pub binned_kinetic_j: f64,
    /// J. Heat above ambient (`m · c · (T − REF_TEMP_K)`) that crossed the rung and found no sink,
    /// measured the same way: carried in minus carried out on the remainder. Counted only for
    /// materials with a SOURCED specific heat ([`Material::specific_heat`]); an unknown `c` means
    /// the carried heat has no honest value in joules, and an unknown stays unknown (Law VII)
    /// rather than being invented at the call site. Same missing voxel-side thermal sink, same
    /// deferral as `binned_kinetic_j` (docs/46 row 17).
    pub binned_heat_j: f64,
}

/// **The rung.** Bin a settled particle field into the voxel world, then let the caller re-mesh.
///
/// Refuses the whole region unless the [`SettleGauge`] shows one sustained cell-dynamical-time of
/// quiet AND every grain is individually below the quiescent speed right now — a single still-moving
/// grain means the excitement has not passed. On success, grains are grouped per column and
/// material, their mass accumulated (f64, so the error is the inputs' f32 quantization and not the
/// summation), and each whole voxel quantum `ρ · Δ³` is deposited through
/// [`crate::matter::deposit_grain`] — the same law the per-grain settle path uses, so water
/// displacement and body refusal have one answer. Sub-quantum remainders ride out on the column's
/// last grain (position, velocity, temperature kept; mass shrunk to exactly the unbinned amount).
pub fn recohere_settled(
    world: &mut World,
    materials: &[Material],
    grains: &[FieldGrain],
    g: f32,
    gauge: &SettleGauge,
    bodies: &[Sphere],
) -> Result<Recohered, Refusal> {
    // The criterion, both halves. Refusal must precede ANY write: a half-binned region would be a
    // world that disagrees with its own particle field.
    if !gauge.settled(g) {
        return Err(Refusal::NotSustained {
            quiet_s: gauge.quiet_seconds(),
            needed_s: quiescent_interval_s(g, CELL_M),
        });
    }
    let limit = quiescent_speed(g, CELL_M);
    for gr in grains {
        let speed = gr.vel.length();
        if speed >= limit {
            return Err(Refusal::StillMoving { speed, limit });
        }
    }

    // Bin per (column, material): mass accumulates in f64 so the only error left is the inputs' own
    // f32 quantization, not the summation order. Keys are sorted so the rung is deterministic —
    // the same field always produces the same world.
    use std::collections::BTreeMap;
    let center = world.center();
    let mut bins: BTreeMap<(i32, i32, usize), Vec<usize>> = BTreeMap::new();
    for (i, gr) in grains.iter().enumerate() {
        let xi = (gr.pos.x + center.x).floor() as i32;
        let zi = (gr.pos.z + center.z).floor() as i32;
        bins.entry((xi, zi, gr.material)).or_default().push(i);
    }

    let mut out = Recohered::default();
    let cell_volume = (CELL_M as f64).powi(3);
    for ((_, _, mat), idxs) in &bins {
        let Some(m) = materials.get(*mat) else {
            // An unknown material cannot claim a density, so its mass cannot become field — it
            // stays particles rather than being guessed at (Law VII: an unknown stays unknown).
            for &i in idxs {
                out.remainder.push(grains[i]);
            }
            continue;
        };
        let voxel_mass = m.density as f64 * cell_volume;
        let total: f64 = idxs.iter().map(|&i| grains[i].mass as f64).sum();
        // The crossing's energy books, per column: what the grains carry IN. What the remainder
        // carries back OUT is subtracted below; the difference is the energy that crossed into the
        // voxel store and found no thermal sink, measured into the audit rather than zeroed
        // (docs/46 row 17 names the sink this defers). Heat is counted only where the material's
        // specific heat is sourced; an unknown c stays unknown (Law VII), never a number invented
        // here.
        let c_heat = m.specific_heat();
        let t_ref = crate::matter::REF_TEMP_K as f64;
        let kin_in: f64 = idxs
            .iter()
            .map(|&i| 0.5 * grains[i].mass as f64 * grains[i].vel.length_squared() as f64)
            .sum();
        let heat_in: f64 = c_heat.map_or(0.0, |c| {
            idxs.iter()
                .map(|&i| grains[i].mass as f64 * c * (grains[i].temp_k as f64 - t_ref))
                .sum()
        });
        // Whole voxel quanta this column's mass can claim. The epsilon forgives the inputs' f32
        // quantization only (a column of exactly-voxel-mass grains must not lose a voxel to the
        // last ulp); it is ~1e-9 of one quantum, far below any physical mass here.
        let want = ((total / voxel_mass) + 1e-9).floor() as usize;
        // Deposit through the ONE law. It may refuse (full column, body in the way, sea with no
        // room) — refused quanta stay mass in the remainder, never a forced write.
        let mut placed = 0usize;
        let site = grains[idxs[0]].pos;
        for _ in 0..want {
            if crate::matter::deposit_grain(world, site, *mat, bodies) {
                placed += 1;
            } else {
                break;
            }
        }
        out.voxels += placed;
        let deposited = placed as f64 * voxel_mass;
        out.deposited_mass += deposited;
        let leftover = total - deposited;
        let mut kin_out = 0.0f64;
        let mut heat_out = 0.0f64;
        if leftover > voxel_mass * 1e-9 {
            // The unbinned mass rides out on the column's last grain: position, velocity, material
            // and temperature kept; only the mass shrinks to exactly what was not binned.
            let mut gr = grains[*idxs.last().expect("bins are never empty")];
            gr.mass = leftover as f32;
            kin_out = 0.5 * leftover * gr.vel.length_squared() as f64;
            heat_out = c_heat.map_or(0.0, |c| leftover * c * (gr.temp_k as f64 - t_ref));
            out.remainder.push(gr);
        }
        // Energy in minus energy still on particles: the measured loss at the crossing. This also
        // books the remainder's re-representation (a column's leftover rides out at the LAST
        // grain's velocity and temperature), so the ledger identity "in = remainder + audit" holds
        // exactly rather than only for the binned share.
        out.binned_kinetic_j += kin_in - kin_out;
        out.binned_heat_j += heat_in - heat_out;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;

    fn mats() -> Vec<Material> {
        materials::load()
    }

    /// A 16³ world with a flat dirt floor 4 voxels deep and open air above.
    fn floor_world(mats: &[Material]) -> World {
        let dirt = materials::index_of(mats, "dirt") as u16;
        let n = 16usize;
        let mut voxels = vec![0u16; n * n * n];
        for y in 0..4 {
            for z in 0..n {
                for x in 0..n {
                    voxels[(y * n + z) * n + x] = dirt + 1;
                }
            }
        }
        World::from_voxels(n, n, n, voxels, 4, None)
    }

    /// Centered coords for the centre of voxel (x, y, z).
    fn at(world: &World, x: i32, y: i32, z: i32) -> Vec3 {
        Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - world.center()
    }

    fn quiet_gauge(g: f32) -> SettleGauge {
        let mut gauge = SettleGauge::new();
        // Feed far more sustained quiet than one cell dynamical time (~0.45 s at Earth g).
        for _ in 0..200 {
            gauge.observe(0.0, g, 0.01);
        }
        gauge
    }

    /// **The conservation contract.** A settled mound of gravel grains — some exactly one voxel
    /// quantum, one deliberately sub-quantum — re-coheres: whole quanta become gravel voxels
    /// stacked on the dirt floor, the sub-quantum remainder SURVIVES as a particle, and
    /// mass-in equals voxel mass out plus remainder mass to within the inputs' f32 error.
    #[test]
    fn a_settled_mound_recoheres_conserving_mass_and_material() {
        let mats = mats();
        let gravel = materials::index_of(&mats, "gravel");
        let mut w = floor_world(&mats);
        let g = 9.81;
        let rho = mats[gravel].density;

        // Three columns of resting gravel: 3 + 2 + 1 whole-quantum grains, drifting at millimetres
        // per second (real settled jitter), plus one 0.6-quantum grain that cannot fill a voxel.
        let mut grains = Vec::new();
        for (dx, dz, k) in [(0i32, 0i32, 3usize), (1, 0, 2), (0, 1, 1)] {
            for i in 0..k {
                grains.push(FieldGrain {
                    pos: at(&w, 8 + dx, 4 + i as i32, 8 + dz),
                    vel: Vec3::new(0.003, 0.0, -0.002),
                    mass: rho,
                    material: gravel,
                    temp_k: 300.0,
                });
            }
        }
        grains.push(FieldGrain {
            pos: at(&w, 8, 7, 8),
            vel: Vec3::ZERO,
            mass: 0.6 * rho,
            material: gravel,
            temp_k: 340.0,
        });
        let mass_in: f64 = grains.iter().map(|gr| gr.mass as f64).sum();
        let solids_before = w.solid_count();

        let r = recohere_settled(&mut w, &mats, &grains, g, &quiet_gauge(g), &[])
            .expect("a settled mound must re-cohere");

        // Six whole quanta became six voxels; the world holds them as SOLID ground.
        assert_eq!(r.voxels, 6, "six whole voxel quanta of gravel were offered");
        assert_eq!(w.solid_count() - solids_before, 6, "the world gained exactly those voxels");

        // Material identity: the new ground IS gravel, stacked from each column's old air-start.
        for (dx, dz, k) in [(0, 0, 3), (1, 0, 2), (0, 1, 1)] {
            for i in 0..k {
                assert_eq!(
                    w.material_at(8 + dx, 4 + i, 8 + dz),
                    Some(gravel),
                    "column (+{dx},+{dz}) layer {i} must come back as gravel, not generic terrain"
                );
            }
        }

        // Mass conservation within f32 accumulation error: binned + remainder == offered.
        let remainder_mass: f64 = r.remainder.iter().map(|gr| gr.mass as f64).sum();
        let mass_out = r.deposited_mass + remainder_mass;
        assert!(
            (mass_out - mass_in).abs() <= mass_in * 1e-5,
            "mass must be conserved: in {mass_in:.3} kg, out {mass_out:.3} kg"
        );

        // The sub-quantum grain is the remainder: same material, its own temperature kept.
        assert_eq!(r.remainder.len(), 1, "only the sub-quantum mass stays a particle");
        assert_eq!(r.remainder[0].material, gravel);
        assert!(
            (r.remainder[0].mass - 0.6 * rho).abs() <= 1e-3 * rho,
            "the remainder is exactly the unbinned 0.6 quantum, got {} of {}",
            r.remainder[0].mass,
            rho
        );
    }

    /// **Energy is measured at the crossing, never silently zeroed.** The voxel world has no
    /// thermal state to receive it (the deferred sink, docs/46 row 17), so the honest contract is
    /// a ledger: energy in = energy out on the remainder + energy measured as `binned_kinetic_j`
    /// and `binned_heat_j`, to within f32 accumulation. Both parts are checked against
    /// independently computed expectations, not against the rung's own arithmetic.
    #[test]
    fn the_crossing_measures_the_binned_kinetic_energy_and_carried_heat() {
        let mats = mats();
        let gravel = materials::index_of(&mats, "gravel");
        let mut w = floor_world(&mats);
        let g = 9.81;
        let rho = mats[gravel].density;
        let c = mats[gravel].specific_heat().expect("gravel thermal data is sourced");
        let t_ref = crate::matter::REF_TEMP_K as f64;

        // Column A: two whole-quantum grains drifting at 2 m/s (below v_q ≈ 4.4 m/s), 50 K above
        // ambient. Both bin entirely: ALL their kinetic energy and heat cross with no remainder.
        // Column B: one 1.6-quantum grain at 1 m/s, 40 K above ambient. One quantum bins; the
        // 0.6-quantum remainder keeps its velocity and temperature, so only the binned 1.0·ρ of
        // mass loses its share.
        let grains = vec![
            FieldGrain {
                pos: at(&w, 8, 4, 8),
                vel: Vec3::new(2.0, 0.0, 0.0),
                mass: rho,
                material: gravel,
                temp_k: 350.0,
            },
            FieldGrain {
                pos: at(&w, 8, 5, 8),
                vel: Vec3::new(2.0, 0.0, 0.0),
                mass: rho,
                material: gravel,
                temp_k: 350.0,
            },
            FieldGrain {
                pos: at(&w, 9, 4, 8),
                vel: Vec3::new(0.0, 0.0, 1.0),
                mass: 1.6 * rho,
                material: gravel,
                temp_k: 340.0,
            },
        ];
        let energy_of = |gr: &FieldGrain| {
            let kin = 0.5 * gr.mass as f64 * gr.vel.length_squared() as f64;
            let heat = gr.mass as f64 * c * (gr.temp_k as f64 - t_ref);
            (kin, heat)
        };
        let (kin_in, heat_in) = grains
            .iter()
            .map(|gr| energy_of(gr))
            .fold((0.0, 0.0), |(k, h), (dk, dh)| (k + dk, h + dh));

        let r = recohere_settled(&mut w, &mats, &grains, g, &quiet_gauge(g), &[])
            .expect("a settled field must re-cohere");
        assert_eq!(r.voxels, 3, "two quanta from column A, one from column B");

        // Independent expectations. Column A: 2 · ½ρ·2² kinetic, 2 · ρ·c·50 heat. Column B binned
        // exactly one quantum of its 1.6: ½·1.0ρ·1² kinetic, 1.0ρ·c·40 heat.
        let kin_expected = 2.0 * 0.5 * rho as f64 * 4.0 + 0.5 * rho as f64 * 1.0;
        let heat_expected = 2.0 * rho as f64 * c * 50.0 + rho as f64 * c * 40.0;
        assert!(
            (r.binned_kinetic_j - kin_expected).abs() <= kin_expected * 1e-5,
            "the binned kinetic energy is measured: expected {kin_expected:.3e} J, audit says {:.3e} J",
            r.binned_kinetic_j
        );
        assert!(
            (r.binned_heat_j - heat_expected).abs() <= heat_expected * 1e-5,
            "the binned heat is measured: expected {heat_expected:.3e} J, audit says {:.3e} J",
            r.binned_heat_j
        );

        // The ledger identity: energy before the crossing = energy still on particles + energy
        // measured at the crossing, within f32 accumulation. Nothing vanishes unmeasured.
        let (kin_rem, heat_rem) = r
            .remainder
            .iter()
            .map(|gr| energy_of(gr))
            .fold((0.0, 0.0), |(k, h), (dk, dh)| (k + dk, h + dh));
        let kin_out = kin_rem + r.binned_kinetic_j;
        let heat_out = heat_rem + r.binned_heat_j;
        assert!(
            (kin_out - kin_in).abs() <= kin_in * 1e-5,
            "kinetic energy is conserved across the crossing: in {kin_in:.3e} J, out {kin_out:.3e} J"
        );
        assert!(
            (heat_out - heat_in).abs() <= heat_in * 1e-5,
            "heat is conserved across the crossing: in {heat_in:.3e} J, out {heat_out:.3e} J"
        );
        // And the remainder still owns its share as a live particle: temperature kept, not audited.
        assert_eq!(r.remainder.len(), 1);
        assert!((r.remainder[0].temp_k - 340.0).abs() < 1e-3, "the remainder keeps its own heat");
    }

    /// **A still-moving region is refused, and refusal changes nothing.** One grain above the
    /// quiescent speed poisons the whole region — binning around it would freeze real motion into
    /// the field.
    #[test]
    fn a_still_moving_region_is_refused_and_the_world_is_untouched() {
        let mats = mats();
        let gravel = materials::index_of(&mats, "gravel");
        let mut w = floor_world(&mats);
        let g = 9.81;
        let rho = mats[gravel].density;
        let limit = quiescent_speed(g, 1.0);

        let grains = vec![
            FieldGrain {
                pos: at(&w, 8, 4, 8),
                vel: Vec3::ZERO,
                mass: rho,
                material: gravel,
                temp_k: 300.0,
            },
            FieldGrain {
                pos: at(&w, 9, 4, 8),
                vel: Vec3::new(1.5 * limit, 0.0, 0.0), // still in flight
                mass: rho,
                material: gravel,
                temp_k: 300.0,
            },
        ];
        let solids_before = w.solid_count();
        match recohere_settled(&mut w, &mats, &grains, g, &quiet_gauge(g), &[]) {
            Err(Refusal::StillMoving { speed, limit: l }) => {
                assert!(speed > l, "the refusal must name the offending speed");
            }
            other => panic!("a moving region must be refused, got {other:?}"),
        }
        assert_eq!(w.solid_count(), solids_before, "refusal must not touch the world");

        // And a quiet field behind a NOT-yet-sustained gauge is refused too: quiet-right-now is
        // not settled; the interval is the criterion's other half.
        let calm = vec![grains[0]];
        let mut fresh = SettleGauge::new();
        fresh.observe(0.0, g, 0.05); // one brief quiet moment, far under sqrt(2Δ/g)
        match recohere_settled(&mut w, &mats, &calm, g, &fresh, &[]) {
            Err(Refusal::NotSustained { quiet_s, needed_s }) => {
                assert!(quiet_s < needed_s);
            }
            other => panic!("an unsustained region must be refused, got {other:?}"),
        }
    }

    /// **The criterion is physical time, not a frame count.** The same simulated quiet in coarse or
    /// fine steps settles identically (the docs/57 #4 lesson), the threshold scales with g as
    /// sqrt(2gΔ) says, and one loud observation resets the clock because sustained means continuous.
    #[test]
    fn the_settle_criterion_is_seconds_of_quiet_not_frames() {
        let g = 9.81;
        let t_q = quiescent_interval_s(g, 1.0);
        assert!((t_q - (2.0f32 / 9.81).sqrt()).abs() < 1e-6, "one cell free-fall time at Earth g");

        // Same 0.5 s of quiet, 5 coarse steps vs 500 fine ones: both settled.
        let mut coarse = SettleGauge::new();
        for _ in 0..5 {
            coarse.observe(0.0, g, 0.1);
        }
        let mut fine = SettleGauge::new();
        for _ in 0..500 {
            fine.observe(0.0, g, 0.001);
        }
        assert!(coarse.settled(g) && fine.settled(g), "the step size must not decide settling");

        // 0.4 s < t_q ≈ 0.45 s: not yet, in either stepping.
        let mut short = SettleGauge::new();
        for _ in 0..400 {
            short.observe(0.0, g, 0.001);
        }
        assert!(!short.settled(g), "quiet shorter than the cell dynamical time is not settled");

        // A jolt mid-window resets the clock: sustained means CONTINUOUS.
        let mut jolted = SettleGauge::new();
        for _ in 0..4 {
            jolted.observe(0.0, g, 0.1);
        }
        jolted.observe(2.0 * quiescent_speed(g, 1.0), g, 0.1);
        jolted.observe(0.0, g, 0.1);
        assert!(!jolted.settled(g), "one loud observation must restart the sustained window");

        // Free space: nothing settles onto ground where there is no down.
        assert!(quiescent_interval_s(0.0, 1.0).is_infinite());
    }
}
