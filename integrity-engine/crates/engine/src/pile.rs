//! **Drop a heap of things and measure what forms** — the settled pile, and its summary (docs/71 §3b).
//!
//! Robin, 2026-08-10, on a haystack whose packing was a number computed from a cylinder somebody chose:
//!
//! > *"As you pile them a stack should naturally form if you drop them all in the same location. Which
//! > is cool, but very slow to simulate. I wonder if we could simulate it and then map the pile as a
//! > derived assembly?"*
//!
//! So this settles the members under the engine's own contact law and MEASURES the heap. The result is
//! a measurement with provenance, not a declaration — the same epistemic status as a catalogued
//! material property, and falsifiable in the same way: a settled heap's density can be compared
//! against a real one's.
//!
//! ## It is the engine's own contact law, applied where the bodies actually touch
//!
//! `granular::contact_accel` takes two POINTS and their velocities. A grain of sand is a sphere and its
//! centre is where it touches; a blade of grass is 0.35 m long and 3 mm wide, and where it touches
//! depends on which way it lies. So each member is carried as a CAPSULE — a segment with a radius — and
//! the same law is evaluated at the two segments' closest approach. **No new physics**: the same
//! stiffness, the same damping, the same Coulomb cap that produce an angle of repose for sand.
//!
//! ★ **Why this matters more than it sounds.** Settling blades as SPHERES would pack them at random
//! close packing, ~0.6, and a straw bale is **0.071** — eight times looser. The elongation IS the
//! physics here; a sphere model would return a number that looked measured and was wrong by that
//! factor. The capsule is volume-exact (`π r² L` equals the member's own matter volume), so nothing is
//! gained or lost in the substitution.
//!
//! ## What is honest about it, and what is not
//!
//! - The heap's ENVELOPE is measured by occupancy on a grid whose cell size is stated with the result,
//!   because "the volume a heap occupies" has no meaning without one. Refining the cell converges.
//! - ★ FLAGGED: a capsule cannot BEND. Real straw is flexible, and flexibility lets a heap settle
//!   denser than rigid rods of the same shape — so this measurement is a LOWER bound on packing, in a
//!   direction that is stated rather than discovered.
//! - ★ FLAGGED: this settles under gravity alone. A BALE is compressed by a machine, so a free heap is
//!   loose hay rather than a bale, and the difference between the two numbers is the baling.
//!
//! ## ★★★ WHAT WAS ACTUALLY WRONG, AND WHAT I GOT WRONG ABOUT IT (2026-08-15, docs/46 row 60)
//!
//! The heap used to run a flat 4.0 simulated seconds and report whatever it had. Asked to PROVE it
//! had settled (`recohere::SettleGauge`, [`Settled::quiet`]) it never did — 400 blades ran a 20 s cap
//! without one `t_q` of quiet — and hunting that turned up a real defect and a wrong explanation.
//!
//! ### The defect: the floor could not hold anything up
//!
//! See [`floor_contact`]. FIXED. It was worse than "a missing force": a **12.245 m/s² downward magnet**
//! (1.248× gravity) with zero support, reaching ~82% of the population, plus μ·cohesion = 9.80 m/s² of
//! horizontal Coulomb brake. An unsupported near-vertical rod sank its whole 0.175 m half-length —
//! **325 contact radii** — through the floor in 0.126 s with nothing under it. That is a strong
//! candidate for the 0.850 → 0.270 m collapse and for the heap never coming to rest.
//!
//! ### ★★ What I claimed and had refuted — do not re-derive it
//!
//! I attributed the energy rise to the `centre.y = radius` position clamp "doing positive work every
//! step". **That is wrong and three independent audits killed it.** Over one step the clamp's
//! `+mgA·dt²` of position work is EXACTLY the potential energy gravity removed in that same step, and
//! its velocity zeroing removes EXACTLY the kinetic energy the same step's velocity update added: the
//! net is identically zero for any acceleration `A`. In this regime it is in fact a net SINK — the
//! clamp-only term `m·V·(g·dt − V/2)` is positive only below `V = 2g·dt` = 4.61 mm/s, and a rod under
//! `g + cohesion` arrives at 5.18 mm/s. The magnitude never reached either: the absolute ceiling with
//! every rod clamped every step at the optimum is ~1.0e-3 J/s against a required gross of ~9.6e-3.
//!
//! I also said the floor "can never push up". **Wrong at the bit level, and the truth is uglier.**
//! `f_rep` is gated on `overlap > 0.0` but is dominated by its DAMPING term, not the spring. `overlap`
//! is zero only in exact arithmetic; in f64 it lands ±1..30 ulp and comes out positive on 5–8% of
//! in-zone steps, and the damper then fires at up to **+1320 m/s²**, ~100× the cohesion. So the floor
//! was a constant downward pull with a bit-randomised, one-sided, enormous damper on top of it. Still
//! dissipative (the `.max(0.0)` kills it on ascent), so it is a worse bug rather than a second pump.
//!
//! ### ★★ And the meter itself was lying
//!
//! [`Sample::energy_j`] is KE + `m·g·y` and nothing else. The image cohesion was an **unaccounted
//! external potential well**, up to 9.54e-4 J deep per rod against a mean per-rod traced energy of
//! 1.3e-4 J, so a rod merely SINKING into it raised the trace with no energy created anywhere. A
//! single ISOLATED rod — no neighbours at all — showed a 10.2% rise from this alone. **A rise in this
//! trace is therefore evidence, not proof.**
//!
//! ★ The blind spot did not go away with the fix, it changed sign: `terrain_contact_resolve`'s
//! position projection lifts a rod without any matching term in the meter, so it too can raise `mgy`
//! for free. Post-fix the measured rise is 0.0%, but that is a measurement, not a guarantee.
//!
//! ### ★★★ STILL OPEN — the late rise is NOT explained
//!
//! Over t = 12 → 20 s the heap's height ROSE, 0.270 → 0.306 m. Sinking into the cohesion well raises
//! the trace and climbing out of it LOWERS the trace, so the floor bug has the wrong sign to explain
//! that window. Something was really lifting the top of the heap. The remaining suspect is the
//! **rod-rod `granular::contact_accel` penalty spring**, which is the one contact here still not
//! unified onto the non-injecting `granular::terrain_contact_resolve`. The post-fix heap settles in
//! 0.80 s, which is too short for a slow pump to show, so this is UNTESTED rather than resolved.
//!
//! ### ★★ Rods cannot rotate
//!
//! [`Rod::axis`] is set at construction and never updated: no angular velocity, no torque, no moment
//! of inertia, and a contact force found at an off-centre closest approach is applied as pure
//! translation to the centre. A dropped straw rotates to lie flat, which is the principal way a rod
//! heap densifies, so the release's uniform-on-the-sphere orientation is also the final one. Whether
//! this is also an ENERGY problem is open; that it is a PACKING problem is not in doubt.
//!
//! Both outrank the bending flag above, and bending cannot be measured through either of them.

use crate::assembly::Assembly;
use crate::materials::Material;
use glam::DVec3;

/// A member of a pile, as the settler carries it: a segment with a radius, volume-exact against the
/// assembly it stands for.
#[derive(Clone, Copy, Debug)]
pub struct Rod {
    pub centre: DVec3,
    /// Unit direction of the long axis.
    pub axis: DVec3,
    pub half_length_m: f64,
    pub radius_m: f64,
    pub vel: DVec3,
}

impl Rod {
    /// The two ends of the segment.
    pub fn ends(&self) -> (DVec3, DVec3) {
        (
            self.centre - self.axis * self.half_length_m,
            self.centre + self.axis * self.half_length_m,
        )
    }

    pub fn volume_m3(&self) -> f64 {
        std::f64::consts::PI * self.radius_m * self.radius_m * 2.0 * self.half_length_m
    }
}

/// **The member of a pile, as a capsule** — length from its longest extent, radius chosen so the
/// capsule holds exactly the matter the assembly does.
pub fn rod_for(member: &Assembly) -> Option<(f64, f64)> {
    let matter = member.matter_volume_m3();
    if matter <= 0.0 {
        return None;
    }
    // The longest dimension any of its parts has — a blade's length, a log's length.
    let length = member
        .parts
        .iter()
        .map(|p| {
            let h = p.shape.half_extents_m();
            2.0 * h.x.max(h.y).max(h.z)
        })
        .fold(0.0f64, f64::max);
    if length <= 0.0 {
        return None;
    }
    // π r² L = matter ⇒ the capsule carries exactly what the member does.
    let radius = (matter / (std::f64::consts::PI * length)).sqrt();
    Some((length, radius))
}

/// The closest points on two segments, and the distance between them. Standard segment–segment
/// closest approach; the degenerate parallel case falls back to clamping, which is what it should do.
fn closest_points(a0: DVec3, a1: DVec3, b0: DVec3, b1: DVec3) -> (DVec3, DVec3) {
    let (u, v, w) = (a1 - a0, b1 - b0, a0 - b0);
    let (a, b, c) = (u.dot(u), u.dot(v), v.dot(v));
    let (d, e) = (u.dot(w), v.dot(w));
    let denom = a * c - b * b;
    // Solve the unconstrained problem, then clamp — and RE-SOLVE the other parameter after each clamp.
    // ★ Skipping the re-solve is the classic bug in this routine and my own test caught it: two
    // COLLINEAR segments lying end to end returned the far end of the first one, because the
    // degenerate branch pinned `s = 0` and never asked what `s` should be once `t` was clamped.
    let mut s = if denom.abs() < 1e-12 {
        0.0
    } else {
        ((b * e - c * d) / denom).clamp(0.0, 1.0)
    };
    let mut t = if c > 1e-12 { (b * s + e) / c } else { 0.0 };
    if t < 0.0 {
        t = 0.0;
        s = if a > 1e-12 {
            (-d / a).clamp(0.0, 1.0)
        } else {
            0.0
        };
    } else if t > 1.0 {
        t = 1.0;
        s = if a > 1e-12 {
            ((b - d) / a).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    (a0 + u * s, b0 + v * t)
}

/// ★★★ **THE FLOOR, AND IT IS THE ENGINE'S OWN NON-INJECTING CONSTRAINT — NOT A SECOND ONE.**
///
/// `granular::terrain_contact_resolve` is the engine's existing answer to "a body has gone below a
/// surface, what happens", and its own documentation names the history this module repeated: a stiff
/// penalty spring stores ½k·pen² and releases it as launch kinetic energy, which is the *settling
/// storm*. It resolves contact as a CONSTRAINT — the into-surface velocity is removed (never
/// reversed), Coulomb friction can only halt slip, and the position projection is velocity-decoupled,
/// so it writes no kinetic energy however far the surface moved. It can only ever REMOVE energy.
///
/// ## What this replaces, and why it was worse than a penalty spring
///
/// The floor used to be an image particle at `p - ŷ·2r` handed to `granular::contact_accel`. That is
/// a FIXED OFFSET, not a reflection, so `|p - ghost|` was exactly `2r` — exactly `touch` — no matter
/// how deep the rod had sunk. `overlap = touch - dist` was therefore **always zero**, the repulsive
/// spring was gated on `overlap > 0.0` and never fired once, and the floor could not push up at all.
/// It did not merely do nothing: `coh_range = 0.15·radius > 0` kept the early-out from triggering, so
/// the ADHESION term ran at full strength and pulled the rod DOWN — measured at **−12.245 m/s²**
/// against a correctly reflected ghost's **+85.197 m/s²**. That is 1.248× gravity, and it collapses
/// exactly to `σ/(ρ·L)` = 6000/(1400 × 0.35), independent of the blade's cross-section. The same call
/// also returned `normal_load = f_rep + f_coh` = cohesion at zero compression, so it applied
/// `μ·cohesion` = 9.80 m/s² of horizontal Coulomb brake as well: a downward magnet WITH a strong
/// sideways drag. The heap was held off the ground entirely by a `centre.y = radius` position clamp.
///
/// ★ Two corrections to the first telling of this, both from adversarial audit, both worth keeping:
///
/// - **The clamp was not a pump.** Its `+mgA·dt²` of position work is exactly the potential energy
///   gravity removed in the same step and its velocity zeroing removes exactly that step's added
///   kinetic energy — net zero identically, and a net SINK at the arrival speeds in play. The energy
///   argument for this fix was wrong; the fix is right for a simpler reason, which is that a floor
///   that cannot push up is not a floor.
/// - **"Never pushes up" was wrong at the bit level.** `overlap` is zero only in exact arithmetic; in
///   f64 it lands ±1..30 ulp and is positive on 5–8% of in-zone steps, and `f_rep`'s DAMPING term —
///   `−c_damp·v_n` with `c_damp` = 587.5 s⁻¹ — then fires at up to **+1320 m/s²**, ~100× the cohesion.
///   The floor was a constant pull with a bit-randomised one-sided damper on top: worse, not better.
///
/// ★★ The mismatch that made it maximally destructive: the force triggered on the rod's lowest END
/// while the clamp acted on its CENTRE. For an unsupported near-vertical rod the clamp never fires
/// until the rod has sunk its entire 0.175 m half-length — **325 contact radii** — through the floor,
/// which takes 0.126 s at `g + cohesion`.
///
/// A flat floor is `h = 0` with zero gradient. `part_half` is the capsule's own radius, because the
/// point handed in is the rod's lowest END on the centre-line and its surface is `radius` below that.
/// `max_corr` is the radius rather than a chosen length — a body is never projected further than its
/// own size in one substep — and `headroom` is `INFINITY`, matching every other caller, which is the
/// one thing here worth revisiting if a buried rod is ever seen being rammed up through its
/// neighbours.
fn floor_contact(rod: &Rod, radius: f64, friction: f64) -> crate::granular::TerrainContact {
    let (e0, e1) = rod.ends();
    // ★ The WHOLE lower end, not its height bolted onto the centre's x and z. The old floor block
    // built `(centre.x, lowest, centre.z)` — a point that is on neither the rod nor the segment — and
    // got away with it only because its ghost differed in Y alone. It would stop getting away with it
    // the moment this floor is given a real heightfield, because `h` and its gradients are sampled at
    // (x, z). Passing the actual contact point costs nothing and removes the trap.
    let lower = if e0.y <= e1.y { e0 } else { e1 };
    crate::granular::terrain_contact_resolve(
        lower,
        rod.vel,
        0.0,
        0.0,
        0.0,
        radius,
        friction,
        radius,
        f64::INFINITY,
    )
}

/// What a settled heap turned out to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settled {
    pub members: usize,
    /// Substance in the heap, m³ — exactly `members × the member's own`.
    pub matter_m3: f64,
    /// The space the heap occupies, m³, by occupancy on `cell_m`.
    pub envelope_m3: f64,
    /// What the heap came to: matter over envelope. **Measured, not authored.**
    pub packing: f64,
    /// How tall it stands, m.
    pub height_m: f64,
    /// The grid cell the envelope was measured on — the number without which the envelope is
    /// meaningless.
    pub cell_m: f64,
    /// ★ **Did it actually come to rest**, by `recohere::SettleGauge` asked at the CONTACT radius —
    /// this simulation's own resolution, not the coarser cell the envelope is reported on. The module
    /// doc has always said "if a heap is still moving at the end its packing is not a settled
    /// packing" — this is the field that makes that checkable instead of merely stated.
    pub quiet: bool,
    /// Simulated seconds the heap ran for: the moment it went quiet, or the cap if it never did.
    pub elapsed_s: f64,
    /// The fastest member at the end (m/s), against `recohere::quiescent_speed` at the CONTACT
    /// RADIUS — the scale the gauge is asked at, which is not `cell_m`.
    pub peak_speed_ms: f64,
}

/// A deterministic value in `0..1` — the same seed and index always give the same number, so a heap is
/// reproducible and a test is not at the mercy of an RNG.
pub fn derived_unit(seed: u64, i: u64, k: u64) -> f64 {
    unit(seed, i, k)
}

fn unit(seed: u64, i: u64, k: u64) -> f64 {
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ i.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ k.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// ★★★ **DROP THEM ALL IN THE SAME PLACE AND SEE WHAT FORMS.**
///
/// `count` members are released above a floor with derived positions and orientations, fall under
/// `gravity`, and are settled by the engine's contact law at their capsule closest approach. The heap
/// that results is then measured.
///
/// This is a BUILD-TIME cost, not a frame cost — identical until damaged means every bale in a world
/// is the same settled heap until something happens to one, so the simulation runs once per TYPE.
pub fn settle(
    member: &Assembly,
    mats: &[Material],
    count: usize,
    gravity_ms2: f64,
    seed: u64,
) -> Option<Settled> {
    settle_traced(member, mats, count, gravity_ms2, seed, 0.0).map(|(s, _)| s)
}

/// One observation of a heap on its way down — what `settle` was doing while it ran.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub t_s: f64,
    /// The fastest member at this instant (m/s).
    pub peak_speed_ms: f64,
    /// Mean member speed (m/s) — with the peak, this separates "the whole heap is still moving" from
    /// "one member is ringing", which are different problems with different fixes.
    pub mean_speed_ms: f64,
    /// How tall it stands at this instant (m).
    pub height_m: f64,
    /// Continuous quiet the gauge has accumulated so far (s).
    pub quiet_s: f64,
    /// ★ **Total mechanical energy, J** — `Σ ½mv²` plus `Σ mgh`, and NOTHING ELSE. If gravity is the
    /// only thing doing work and every contact is dissipative, this is monotone non-increasing, so a
    /// rise is a signal worth chasing.
    ///
    /// ★★ **It is a signal and not a proof, and knowing which is the whole lesson of docs/46 row 60.**
    /// This sum accounts for exactly two potentials, so ANY other force with a potential is invisible
    /// to it and shows up as spontaneous energy:
    ///
    /// - The old image-particle floor applied a constant attraction inside `lowest < radius`. Because
    ///   `axis` never changes, `lowest` is affine in `centre.y`, so that was exactly a piecewise-linear
    ///   potential well — up to 9.54e-4 J deep per rod against a mean per-rod traced energy of
    ///   1.3e-4 J. A rod merely SINKING into it raised this number with nothing created anywhere, and
    ///   a single isolated rod with no neighbours showed a 10.2% rise from that alone.
    /// - The replacement has the blind spot with the opposite sign: `terrain_contact_resolve`'s
    ///   position projection lifts a rod out of the surface without any matching term here, so it can
    ///   raise `mgh` for free too.
    ///
    /// So: read a rise as "go and find out why", never as "the integrator violates conservation". The
    /// honest confirmation is a mechanism you can point at, or an isolated single-body control.
    pub energy_j: f64,
}

/// **`settle`, and it will tell you how it got there.** Same one implementation — `settle` is this
/// with the trace discarded, so there is no second settling law to disagree with the first.
///
/// A heap that reports `quiet: false` after twenty seconds says only that it did not settle, which is
/// not enough to act on: a heap still collapsing and a settled heap with one member ringing against a
/// PEAK criterion look identical in the summary and want opposite fixes. `trace_every_s` samples the
/// run so the difference is visible. Zero means no trace.
pub fn settle_traced(
    member: &Assembly,
    mats: &[Material],
    count: usize,
    gravity_ms2: f64,
    seed: u64,
    trace_every_s: f64,
) -> Option<(Settled, Vec<Sample>)> {
    let (length, radius) = rod_for(member)?;
    let material = member.dominant_material()?;
    let m = mats.iter().find(|m| m.id == material)?;
    if count == 0 {
        return None;
    }
    // The engine's own contact, for THIS material — the same call a sand grain gets. The member's own
    // mass is what sets the contact stiffness per unit mass, so a blade and a boulder of the same
    // substance are as stiff as their masses make them.
    let member_mass = member.mass_kg(mats).ok()?.max(1e-12);
    let contact = crate::granular::contact_from_material(m, radius, member_mass);

    // Released over a disc a few lengths across, stacked upward so they fall rather than start merged.
    // ★★ **DROPPED IN ONE PLACE**, which is Robin's own wording and is not a detail: a heap's packing
    // is a property of straw only if the heap forms by its OWN repose. MEASURED the other way first —
    // released over a disc wider than a blade is long, 400 blades settled at 0.0005, which is a
    // measurement of how thinly they were scattered rather than of how they pack. A point source lets
    // the pile spread to the angle the contact law gives it.
    let spread = length * 0.1;
    let mut rods: Vec<Rod> = (0..count)
        .map(|i| {
            let i = i as u64;
            let (u1, u2, u3) = (unit(seed, i, 1), unit(seed, i, 2), unit(seed, i, 3));
            let r = spread * u1.sqrt();
            let a = u2 * std::f64::consts::TAU;
            // Orientation on the sphere, uniformly — a dropped blade has no preferred direction.
            let z = 2.0 * u3 - 1.0;
            let phi = unit(seed, i, 4) * std::f64::consts::TAU;
            let s = (1.0 - z * z).max(0.0).sqrt();
            Rod {
                // ★ Released just above the floor, not in a tower. MEASURED the other way first: a
                // column `count`-blades tall is seven metres, and at this contact's stable timestep
                // 4,000 steps is 0.94 s — less than the fall takes — so the "heap" was still a cloud
                // at 0.0001 packing. A release height of a couple of blade lengths falls in ~0.4 s.
                centre: DVec3::new(
                    r * a.cos(),
                    radius + unit(seed, i, 5) * length * 2.0,
                    r * a.sin(),
                ),
                axis: DVec3::new(s * phi.cos(), z, s * phi.sin()).normalize(),
                half_length_m: length * 0.5,
                radius_m: radius,
                vel: DVec3::ZERO,
            }
        })
        .collect();

    // A timestep the contact can hold: ω = √stiffness, and a tenth of that period is stable.
    let dt = (0.1 / contact.stiffness.max(1.0).sqrt()).min(1e-3);

    // ★★★ **IT RUNS UNTIL IT IS QUIET, AND THE ENGINE ALREADY OWNS WHAT QUIET MEANS.**
    //
    // This used to run for a flat 4.0 simulated seconds and call whatever it had a settled heap. The
    // module doc claimed better — *"if a heap is still moving at the end its packing is not a settled
    // packing"* — and `Settled` carried no field that could show it, so the claim was unbacked and a
    // heap that was still flying would have been measured and reported exactly like one at rest.
    //
    // `recohere::SettleGauge` is **the ONE settling gauge** (docs/61): sustained quiet for one cell
    // dynamical time `t_q = √(2Δ/g)`, with "quiet" meaning every member below `v_q = √(2gΔ)` — the
    // speed at which motion cannot buy a one-cell rise. Both halves are physics, neither is a dial.
    //
    // ★★★ **IT IS ASKED AT THE SCALE THIS SIMULATION RESOLVES, WHICH IS THE CONTACT RADIUS.**
    //
    // Not the voxel world's metre: at Δ = 1 m a member counts as quiet below 4.4 m/s, and a 0.35 m
    // blade at 4.4 m/s crosses its own length in 80 ms.
    //
    // And — MEASURED, and it took two tries — not the cell the ENVELOPE is reported on either. That
    // cell is 0.044 m, giving `v_q` = 0.93 m/s, which sits INSIDE the range of speeds a heap has while
    // it is still falling. A 200-blade heap was therefore declared settled at 0.34 s with its top
    // still descending (0.850 → 0.640 m) and its MEAN speed still rising (0.023 → 0.469 m/s): the peak
    // dipped under the bar for one `t_q` in the middle of the collapse. Arming the gauge (below) fixes
    // a population released at rest; it cannot fix a threshold the process transiently satisfies.
    //
    // The contact radius is the right scale because it is this simulation's own quantum: `settle`
    // resolves capsules of radius `r` touching, so `r` is the finest length it represents, and a
    // member too slow to rise by one contact radius is moving below what the arrangement can record.
    // That is exactly the argument `recohere` makes for its voxel, applied to the resolution actually
    // in play here — one criterion, and the caller states the scale it is resolving at. For this blade
    // it is 9× stricter than the envelope cell in speed, well clear of falling speeds.
    //
    // The envelope keeps its own coarser cell, because "how much space is the heap in" and "what can
    // this simulation resolve" are different questions and collapsing them is what went wrong.
    let cell = (length * 0.125).max(radius * 4.0);
    let mut gauge = crate::recohere::SettleGauge::for_cell(radius as f32);
    // A cap so a heap that never settles REPORTS that instead of running forever. Reaching it is a
    // result, not a failure — `Settled::quiet` is false and the packing is not a settled packing.
    const CAP_S: f64 = 20.0;
    let mut elapsed_s = 0.0f64;
    let mut peak_speed = 0.0f64;

    // ★★★ **THE GAUGE IS NOT ARMED UNTIL THE HEAP HAS ACTUALLY BEEN DISTURBED, AND SKIPPING THAT
    // MADE IT REPORT A CLOUD AS A SETTLED HEAP.**
    //
    // The gauge answers "has this region STOPPED moving". A population released from rest has not
    // stopped moving; it has not started. Those two states are identical to a speed threshold, and
    // the consequence is not a near miss — it is an exact dead heat:
    //
    //   free fall from rest reaches  v_q = √(2gΔ)  at  t = v_q/g = √(2Δ/g) = t_q
    //
    // The quiescent SPEED and the quiescent INTERVAL are the same two numbers, so a gauge armed at
    // release always comes due at the very instant the fall first becomes visible to it, and which
    // side wins is decided by rounding. MEASURED, 2026-08-15: heaps of 200, 400 and 800 blades all
    // reported "settled" at 0.09 s against a required 0.0947 s — the earliest arithmetically possible
    // moment — and the 400-blade heap's packing came out at 0.0016 against the four-second run's
    // 0.0024, because what was being measured was the release cloud. A 100-blade heap took 0.41 s
    // only because one unimpeded rod happened to cross the line first.
    //
    // So: wait for motion, THEN wait for quiet. This is a pile-side arming and deliberately NOT a
    // change to the gauge, whose other caller (`site::fold_site`) is right to fold a region that has
    // genuinely never moved.
    let mut disturbed = false;
    let mut trace: Vec<Sample> = Vec::new();
    let mut next_sample = 0.0f64;
    while elapsed_s < CAP_S {
        let snapshot = rods.clone();
        for i in 0..rods.len() {
            let mut acc = DVec3::new(0.0, -gravity_ms2, 0.0);
            let (a0, a1) = snapshot[i].ends();
            for (j, other) in snapshot.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (b0, b1) = other.ends();
                // Cheap reject before the closest-point solve.
                if (other.centre - snapshot[i].centre).length_squared()
                    > (length + 4.0 * radius).powi(2)
                {
                    continue;
                }
                let (pa, pb) = closest_points(a0, a1, b0, b1);
                acc += crate::granular::contact_accel(pa, snapshot[i].vel, pb, other.vel, &contact);
            }
            // ★ The floor is NOT a force here. It is resolved as a constraint after the step, by
            // `floor_contact` — see its doc for the image-particle bug this replaces.
            let r = &mut rods[i];
            r.vel += acc * dt;
            // ★ NUMERICAL damping only, and a RATE rather than a per-step factor — the physics of
            // dissipation is the contact's own `normal_damp` and Coulomb friction. MEASURED the wrong
            // way first: `vel *= 0.98` every step is 85 per second at this timestep, which held the
            // blades in the air and produced a "heap" seven metres tall. 2/s is gentle enough to let
            // them fall and firm enough to stop the explicit integrator ringing.
            r.vel *= 1.0 - (2.0 * dt).min(0.5);
            r.centre += r.vel * dt;

            // ★★★ THE FLOOR, AS A CONSTRAINT. The `r.centre.y = radius; r.vel.y = max(0)` clamp that
            // used to live here is DELETED — not because it injected energy (audited: its net work is
            // identically zero per step, and a net SINK at the speeds in play) but because it was
            // standing in for a floor that could not push up, and it was standing in badly: the clamp
            // tested the rod's CENTRE while the force tested its lowest END. For an unsupported
            // near-vertical blade the centre sits ~0.175 m above the end, so nothing stopped it until
            // it had sunk its whole half-length through the ground.
            let hit = floor_contact(r, radius, contact.friction);
            if hit.hit {
                r.vel = hit.vel;
                r.centre += hit.dpos;
            }
        }
        elapsed_s += dt;
        // PEAK, not mean — the gauge's contract, and the right one: an average hides the single
        // member still bouncing, and one member crossing cells is a heap that has not settled.
        peak_speed = rods.iter().map(|r| r.vel.length()).fold(0.0f64, f64::max);
        if peak_speed >= gauge.moving_above(gravity_ms2 as f32) as f64 {
            disturbed = true;
        }
        if disturbed {
            gauge.observe(peak_speed as f32, gravity_ms2 as f32, dt as f32);
        }
        if trace_every_s > 0.0 && elapsed_s >= next_sample {
            next_sample = elapsed_s + trace_every_s;
            let mean = rods.iter().map(|r| r.vel.length()).sum::<f64>() / rods.len().max(1) as f64;
            let top = rods
                .iter()
                .map(|r| {
                    let (a, b) = r.ends();
                    a.y.max(b.y)
                })
                .fold(0.0f64, f64::max);
            let energy: f64 = rods
                .iter()
                .map(|r| {
                    0.5 * member_mass * r.vel.length_squared()
                        + member_mass * gravity_ms2 * r.centre.y
                })
                .sum();
            trace.push(Sample {
                t_s: elapsed_s,
                peak_speed_ms: peak_speed,
                mean_speed_ms: mean,
                height_m: top,
                quiet_s: gauge.quiet_seconds() as f64,
                energy_j: energy,
            });
        }
        if disturbed && gauge.settled(gravity_ms2 as f32) {
            break;
        }
    }
    // Never disturbed at all is not a settled heap either — it is a heap that never fell, which for a
    // release above a floor means something is wrong with the release, not that the answer is ready.
    let quiet = disturbed && gauge.settled(gravity_ms2 as f32);

    // ★ MEASURE THE HEAP. Occupancy on a grid whose cell is the member's own radius scale, so the
    // envelope means "space the heap is in" rather than "box that contains it".
    // The cell must resolve the HEAP, not the blade: too coarse and the envelope is mostly the
    // measurement's own air. An eighth of a member's length, floored at its thickness. (`cell` is
    // computed above, because the settle gauge is asked at the same scale the answer is expressed at.)
    let mut cells = std::collections::BTreeSet::new();
    let mut height: f64 = 0.0;
    for r in &rods {
        let (e0, e1) = r.ends();
        height = height.max(e0.y.max(e1.y));
        // Walk the segment at cell resolution so a long rod occupies every cell it crosses.
        let n = ((e1 - e0).length() / cell).ceil().max(1.0) as usize;
        for k in 0..=n {
            let p = e0.lerp(e1, k as f64 / n as f64);
            cells.insert((
                (p.x / cell).floor() as i64,
                (p.y / cell).floor() as i64,
                (p.z / cell).floor() as i64,
            ));
        }
    }
    let envelope = cells.len() as f64 * cell * cell * cell;
    let matter = member.matter_volume_m3() * count as f64;
    let settled = Settled {
        members: count,
        matter_m3: matter,
        envelope_m3: envelope,
        packing: if envelope > 0.0 {
            (matter / envelope).clamp(0.0, 1.0)
        } else {
            0.0
        },
        height_m: height,
        cell_m: cell,
        quiet,
        elapsed_s,
        peak_speed_ms: peak_speed,
    };
    Some((settled, trace))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ **DROP THE BLADES AND SEE WHAT FORMS** (docs/71 §3b) — Robin's own idea, measured.
    ///
    /// The question this answers is not "does it run" but **what density does a free heap of dry grass
    /// blades actually come to**, and how that compares with the real numbers:
    ///
    /// | | bulk density | packing at 1400 kg/m³ |
    /// |---|---|---|
    /// | loose hay | 40 kg/m³ | 0.029 |
    /// | field bale | 100 kg/m³ | 0.071 |
    /// | high-density bale | 200 kg/m³ | 0.143 |
    ///
    /// ★★ **THE TARGET IS THE HAYSTACK, NOT THE BALE**, and Robin had to say so twice before I built
    /// the right object: *"a hay bale is tighter packed than a loose pile of straw (a haystack) in real
    /// life… in a hay bale, compressing bands are employed… in a hay stack it's all gravity"* — and
    /// then, when I split them, *"which is why I suggested modelling the haystack."* A heap settling
    /// under gravity alone IS a haystack; a bale is that plus a machine and twine. Comparing this
    /// simulation to a bale was comparing gravity against gravity-plus-baling.
    ///
    /// `#[ignore]`: a few hundred rods over four thousand steps is seconds, not milliseconds.
    #[test]
    #[ignore]
    fn a_heap_of_dry_blades_does_not_come_to_rest_yet() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        println!(
            "blade as a capsule: {length:.3} m long, {:.3} mm across",
            radius * 2000.0
        );

        let settled = settle(&blade, &mats, 400, 9.81, 20260810).expect("a heap forms");
        println!(
            "settled heap: {} blades · {:.3e} m³ of straw in {:.3e} m³ · packing {:.4} \
             ({:.0} kg/m³) · {:.2} m tall · measured on {:.3} m cells",
            settled.members,
            settled.matter_m3,
            settled.envelope_m3,
            settled.packing,
            settled.packing * 1400.0,
            settled.height_m,
            settled.cell_m
        );
        let (_, radius) = rod_for(&blade).expect("a blade is a rod");
        println!(
            "  came to rest: {} after {:.2} s · peak member {:.4} m/s against a quiescent \
             {:.4} m/s at the {:.5} m contact radius",
            settled.quiet,
            settled.elapsed_s,
            settled.peak_speed_ms,
            crate::recohere::quiescent_speed(9.81, radius as f32),
            radius,
        );
        assert_it_actually_fell(&settled, length, radius, 9.81);

        // ★★★ **AND IT DOES NOT COME TO REST — MEASURED 2026-08-15, AND IT RETIRES THE OLD HEADLINE.**
        //
        // This test used to be called `..._settles_at_the_density_of_loose_hay` and pinned 0.0024.
        // That number was a snapshot taken at a flat four seconds, of a heap that was still moving.
        // Asked properly — `recohere::SettleGauge` at the contact radius, armed after the release —
        // the heap runs the full twenty-second cap and never accumulates one `t_q` of quiet.
        //
        // `what_does_a_heap_do_while_it_settles` shows what it does instead, and the shape is not
        // "slowly converging": height falls to 0.270 m by t ≈ 12 s and then CLIMBS BACK to 0.306 m,
        // mean speed stops decaying and plateaus around 0.06–0.10 m/s, and TOTAL MECHANICAL ENERGY
        // falls to 2.60e-2 J and then RISES 9.1% to 2.83e-2 J. Gravity is the only thing doing work
        // and every contact dissipates, so that rise is manufactured by the solver.
        //
        // So the packing below is NOT a bulk density and must not be compared with hay's 0.029. It is
        // the balance point between energy injection and damping, which is a fact about the
        // integrator. Pinned so the number cannot drift unnoticed, and it should FAIL UPWARD the
        // moment the contact stops injecting — that failure is the goal, not a regression.
        assert!(
            !settled.quiet,
            "★ THE HEAP NOW SETTLES — that is the fix landing, not a break. Re-derive the packing \
             against loose hay's 0.029, invert this assertion, and update docs/46 row 60."
        );
        assert!(
            (0.0015..0.0025).contains(&settled.packing),
            "the un-settled heap has moved off its recorded 0.0019: {:.4}",
            settled.packing
        );

        assert!(settled.matter_m3 > 0.0 && settled.envelope_m3 > 0.0);
        // It must form a HEAP — something with height, not a single layer on the floor.
        assert!(
            settled.height_m > length * 0.5,
            "it should stack, not lie flat: {:.3} m for {length:.3} m blades",
            settled.height_m
        );

        // ★★ **THE CAUSE LIST HAS CHANGED, AND THE MEASUREMENT IS WHAT CHANGED IT** (docs/46 row 60).
        // The row named three reasons for the gap to real loose hay and asserted the first was
        // dominant. That assertion was mine and was never measured. Measuring it found two omissions
        // that were not on the list at all and that both outrank it:
        //
        //   A. **THE SOLVER INJECTS ENERGY.** Total mechanical energy rises 9.1% off its minimum
        //      while the heap re-expands. Nothing measured downstream of that is a bulk density.
        //   B. **RODS CANNOT ROTATE.** `Rod::axis` is written once at construction and never again —
        //      there is no angular velocity, no torque, no moment of inertia, and contact forces
        //      found at off-centre closest-approach points are applied as pure translation. A dropped
        //      straw ROTATES TO LIE FLAT, which is the main way a rod heap densifies; here the
        //      initial uniform-on-the-sphere orientation is permanent, so the near-vertical third of
        //      the population props the heap open forever.
        //
        // The original three stand behind those: rods cannot BEND (so a stem props instead of
        // nesting), cannot TANGLE (capsules slide where straw hooks), and 400 members is mostly free
        // surface. But bending cannot be measured through a heap that is being inflated by its own
        // integrator and cannot re-orient, so it is not the next move.
        assert!(
            settled.packing < 0.30,
            "elongated blades cannot pack like spheres — got {:.3}",
            settled.packing
        );
    }

    /// ★★★ **IS THE HEAP BIG ENOUGH TO HAVE A BULK DENSITY AT ALL?** (docs/46 row 60, cause 3.)
    ///
    /// Row 60 names three reasons a settled heap of 400 straw rods comes out at 0.0024 where loose hay
    /// is 0.029, and asserts that the first — a capsule cannot bend — is dominant. **That assertion was
    /// never measured, and it has to be, because the third reason contaminates any test of the first.**
    /// Bulk density is a BULK property; a heap that is mostly free surface does not have one, and
    /// adding bending to a heap that is mostly free surface would be measured against a baseline that
    /// was never a bulk measurement.
    ///
    /// The tell is in the baseline's own printout: 400 blades of 0.350 m settle into a heap **0.41 m
    /// tall**, which is 1.2 blade-lengths. That is a mat one member thick, where essentially every
    /// member touches air.
    ///
    /// So: run the ladder and see whether packing rises and turns over. If it converges, the asymptote
    /// is the number to compare against hay and 400 was simply too few. If it is flat, the finite size
    /// is innocent and bending owns the whole factor of twelve. Either way the next step is aimed by a
    /// measurement instead of by my assessment.
    ///
    /// **A heap cannot have settled before its highest member has landed.** The release is up to
    /// `radius + 2·length` above the floor, so free fall alone takes `√(2h/g)` — a physical lower
    /// bound on the run, with no dial in it.
    ///
    /// ★ This is the negative control for the arming trap (see `settle`): an un-armed gauge fed a
    /// population released AT REST reports "settled" at exactly `t_q`, during the fall, because free
    /// fall from rest reaches `v_q` in exactly `t_q`. Three of four rungs of the first ladder did
    /// precisely that, and the packing they reported was the release cloud's.
    fn assert_it_actually_fell(s: &Settled, length: f64, radius: f64, g: f64) {
        let release_h = radius + 2.0 * length;
        let fall_s = (2.0 * release_h / g).sqrt();
        assert!(
            s.elapsed_s > fall_s,
            "settled in {:.3} s but the highest member needs {:.3} s just to FALL {:.3} m — the \
             gauge fired during the release, so this is a cloud and not a heap",
            s.elapsed_s,
            fall_s,
            release_h
        );
    }

    /// ★★ **WHAT IS THE HEAP DOING WHILE IT FAILS TO SETTLE?**
    ///
    /// A 200-blade heap ran the full twenty-second cap without the gauge ever coming due. The summary
    /// cannot say why, and the two candidate reasons want opposite fixes:
    ///
    /// - the heap is **still collapsing** — then the cap is simply too short, and the answer is more
    ///   simulated time (or a release that does not have so far to fall);
    /// - the heap is **at rest with one member ringing** — then the settling is done and the criterion
    ///   is the problem, because PEAK over a large population is unforgiving of a single contact
    ///   oscillating against an explicit integrator.
    ///
    /// Peak and mean together separate them: both decaying is a collapse, peak stuck high while mean
    /// falls to nothing is a ringer. Height decaying tells the same story from the geometry side.
    ///
    /// This prints; it asserts only that the run is interpretable. It is a measurement, not a gate.
    #[test]
    #[ignore]
    fn what_does_a_heap_do_while_it_settles() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let g = 9.81;
        // ★ 400, not 200, and the count is the point. Post-floor-fix a 100-rod heap comes to rest in
        // 1.44 s and a 200-rod heap in 0.80 s, but 400 still runs the full cap. The residual scales
        // with MEMBER COUNT, which is the signature of a rod-ROD problem rather than a floor one — so
        // the case worth tracing is the one that still misbehaves.
        let (s, trace) =
            settle_traced(&blade, &mats, 400, g, 20260810, 0.25).expect("a heap forms");

        let v_q = crate::recohere::quiescent_speed(g as f32, radius as f32) as f64;
        println!(
            "\n400 blades of {length:.3} m (capsule {:.2} mm across); the gauge is asked at the \
             CONTACT radius {:.5} m, so quiescent speed is {v_q:.4} m/s (at the {:.3} m envelope \
             cell it would be {:.3} m/s, which a falling heap passes through)\n",
            radius * 2000.0,
            radius,
            s.cell_m,
            crate::recohere::quiescent_speed(g as f32, s.cell_m as f32),
        );
        println!("   t_s    peak m/s    mean m/s   height m   quiet_s      energy J");
        let e0 = trace.first().map(|k| k.energy_j).unwrap_or(0.0);
        let mut e_min = f64::INFINITY;
        for k in &trace {
            e_min = e_min.min(k.energy_j);
            println!(
                "{:6.2}   {:8.4}   {:9.5}   {:8.3}   {:7.4}   {:11.4e}{}",
                k.t_s,
                k.peak_speed_ms,
                k.mean_speed_ms,
                k.height_m,
                k.quiet_s,
                k.energy_j,
                if k.peak_speed_ms < v_q {
                    "  <- quiet"
                } else {
                    ""
                }
            );
        }
        let e_end = trace.last().map(|k| k.energy_j).unwrap_or(0.0);
        // ★★ THE WORST DRAWUP, not "the rise from the minimum". MEASURED THE WEAK WAY FIRST: if the
        // run happens to END at its lowest point, "end minus minimum" is 0.0% BY CONSTRUCTION however
        // much the curve climbed in between — which is exactly what the 400-rod run did, reading 0.0%
        // while energy went 2.7154e-1 -> 2.7202e-1 and back. The honest statistic is the largest rise
        // from any running minimum to any LATER sample, because that is what "did it ever go up"
        // means for a quantity that is supposed to be monotone non-increasing.
        let (mut running_min, mut worst_rise, mut worst_at) = (f64::INFINITY, 0.0f64, 0.0f64);
        for k in &trace {
            running_min = running_min.min(k.energy_j);
            let rise = (k.energy_j - running_min) / running_min.abs().max(1e-30);
            if rise > worst_rise {
                worst_rise = rise;
                worst_at = k.t_s;
            }
        }
        println!(
            "\n★ ENERGY: start {e0:.4e} J · minimum {e_min:.4e} J · end {e_end:.4e} J\n  \
             end-vs-minimum {:+.2}% · ★ WORST DRAWUP {:+.3}% (peaking at t = {worst_at:.2} s)",
            (e_end - e_min) / e_min.abs().max(1e-30) * 100.0,
            worst_rise * 100.0,
        );
        println!(
            "  This sum is KE + mgh and NOTHING else, so a rise means some other potential is being \
             converted into one it can see —\n  the old floor's cohesion well, or the rod-rod \
             contact's stored overlap — and not necessarily that energy was created."
        );
        println!(
            "\nended: quiet={} after {:.2} s · packing {:.5} · {:.2} m tall",
            s.quiet, s.elapsed_s, s.packing, s.height_m
        );
        // The diagnosis, stated by the numbers rather than by me.
        if let (Some(first), Some(last)) = (trace.first(), trace.last()) {
            println!(
                "peak {:.4} -> {:.4} ({:.1}x) · mean {:.5} -> {:.5} ({:.1}x) · height {:.3} -> {:.3} m",
                first.peak_speed_ms,
                last.peak_speed_ms,
                first.peak_speed_ms / last.peak_speed_ms.max(1e-12),
                first.mean_speed_ms,
                last.mean_speed_ms,
                first.mean_speed_ms / last.mean_speed_ms.max(1e-12),
                first.height_m,
                last.height_m,
            );
        }
        assert!(!trace.is_empty(), "a traced run must produce samples");
    }

    /// `#[ignore]`: this is minutes, and it is a measurement rather than a gate.
    #[test]
    #[ignore]
    fn does_a_heap_have_a_bulk_density_yet() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");

        println!("\n  n     packing   kg/m³   height   h/blade   rest?   t_rest   peak m/s   cell");
        // ★ THE RUNG THAT IS NOT RUN, STATED RATHER THAN DROPPED SILENTLY. 800 members was in this
        // ladder. Since no heap settles, every rung now runs to the full 20 s cap, and the cost is
        // O(n²) in members: 800 alone is ~37 minutes against ~12 for the other three together. It buys
        // nothing while the answer is "none of these are bulk densities". Put it back — and 1600 with
        // it — the moment the heaps come to rest, because that is when the finite-size question
        // becomes answerable and the large end is exactly where it is answered.
        let mut ladder = Vec::new();
        for n in [100usize, 200, 400] {
            let s = settle(&blade, &mats, n, 9.81, 20260810).expect("a heap forms");
            println!(
                "{:5}   {:.5}   {:5.1}   {:.3} m   {:5.2}     {:5}   {:5.2} s   {:.4}    {:.3} m",
                n,
                s.packing,
                s.packing * 1400.0,
                s.height_m,
                s.height_m / length,
                s.quiet,
                s.elapsed_s,
                s.peak_speed_ms,
                s.cell_m,
            );
            ladder.push((n, s));
        }

        // ★★ **THIS LADDER CANNOT ANSWER ITS OWN QUESTION YET, AND SAYS SO RATHER THAN PRETENDING.**
        // Convergence of a BULK property is only meaningful between heaps that have come to rest. As
        // of 2026-08-15 none of them do: the solver injects energy (see
        // `a_heap_of_dry_blades_does_not_come_to_rest_yet`), so each rung reports the balance point
        // between injection and damping at its own member count, which is a fact about the integrator
        // rather than about straw. The rungs are printed because the trend is still worth seeing; the
        // finite-size question stays OPEN until the heaps settle.
        let unsettled: Vec<usize> = ladder
            .iter()
            .filter(|(_, s)| !s.quiet)
            .map(|(n, _)| *n)
            .collect();
        if !unsettled.is_empty() {
            println!(
                "\n★ NOT A CONVERGENCE MEASUREMENT: {:?} of {:?} never came to rest, so these \
                 packings are not bulk densities. Fix the energy injection first (docs/46 row 60).",
                unsettled,
                ladder.iter().map(|(n, _)| *n).collect::<Vec<_>>()
            );
        }
        for (_, s) in &ladder {
            assert_it_actually_fell(s, length, radius, 9.81);
        }

        // Report the trend rather than assert a target: this test exists to FIND the shape, and a
        // threshold here would be a number I chose. What it does assert is that the run is
        // interpretable — the heaps settled, and packing stayed physical.
        let first = ladder.first().expect("a ladder").1.packing;
        let last = ladder.last().expect("a ladder").1.packing;
        println!(
            "\n  {:5.2}× from {} to {} members; loose hay is 0.029 ({:.1}× the largest heap here)",
            last / first,
            ladder[0].0,
            ladder[ladder.len() - 1].0,
            0.029 / last,
        );
        for (_, s) in &ladder {
            assert!(
                s.packing > 0.0 && s.packing < 0.30,
                "elongated members cannot pack like spheres: {:.4}",
                s.packing
            );
        }
    }

    /// ★★★ **A FLOOR MUST PUSH UP.** The one-line question nobody asked of the settler's floor.
    ///
    /// `settle` models the ground by handing `granular::contact_accel` a ghost particle built as
    /// `mirrored = p - ŷ·2r`. That is a FIXED OFFSET, not a reflection through the floor plane, so
    /// `|p - mirrored|` is exactly `2r` — exactly `touch` — **however deep the rod has sunk**. The
    /// repulsive spring is gated on `overlap > 0.0` and `overlap = touch - dist = 0`, so it never
    /// fires. The contact cannot push up at all.
    ///
    /// It does not merely do nothing, either. `coh_range = 0.15·radius > 0`, so the early-out at
    /// `dist >= touch + coh_range` does not trigger, `sep = 0`, and the adhesion term is at FULL
    /// strength — a constant attraction toward the ghost, which is DOWNWARD.
    ///
    /// A reflection is what the image-particle trick actually means: a body of radius `r` whose
    /// nearest point sits at height `h` above a plane has its image at `-h`, so the centres are `2h`
    /// apart and the overlap `2r - 2h` GROWS as it sinks. That is the comparison this test makes.
    #[test]
    fn the_floor_the_settler_builds_can_only_pull_down() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (_length, radius) = rod_for(&blade).expect("a blade is a rod");
        let material = blade
            .dominant_material()
            .expect("a blade is made of something");
        let m = mats
            .iter()
            .find(|m| m.id == material)
            .expect("straw is catalogued");
        let member_mass = blade.mass_kg(&mats).expect("a blade has mass").max(1e-12);
        let contact = crate::granular::contact_from_material(m, radius, member_mass);

        // A rod that has sunk halfway into the floor. Any honest floor pushes back, hard.
        let lowest = radius * 0.5;
        let p = DVec3::new(0.0, lowest, 0.0);

        let ghost_as_built = p - DVec3::Y * (2.0 * radius);
        let as_built =
            crate::granular::contact_accel(p, DVec3::ZERO, ghost_as_built, DVec3::ZERO, &contact);

        // The same trick done as a REFLECTION through the plane y = 0.
        let ghost_reflected = DVec3::new(p.x, -p.y, p.z);
        let reflected =
            crate::granular::contact_accel(p, DVec3::ZERO, ghost_reflected, DVec3::ZERO, &contact);

        println!(
            "rod sunk to {lowest:.6} m (radius {radius:.6} m), gravity is -9.81 m/s²:\n  \
             ghost at a fixed 2r offset  -> a = {:+.3} m/s²  (separation {:.6} m, overlap {:.3e} m)\n  \
             ghost REFLECTED through y=0 -> a = {:+.3} m/s²  (separation {:.6} m, overlap {:.3e} m)",
            as_built.y,
            (p - ghost_as_built).length(),
            2.0 * radius - (p - ghost_as_built).length(),
            reflected.y,
            (p - ghost_reflected).length(),
            2.0 * radius - (p - ghost_reflected).length(),
        );

        // The reflection is the control: it proves `contact_accel` itself is willing to push up, so
        // the fault belonged to how the ghost was built and never to the contact law.
        assert!(
            reflected.y > 0.0,
            "the CONTROL failed: a correctly reflected ghost must push up, got {:+.3} m/s². If this \
             fails the fault is in contact_accel, not in the ghost construction.",
            reflected.y
        );
        // ★ PINNED: the construction that shipped could only PULL. Kept as an executable record of
        // why, so nobody reintroduces the offset-ghost trick believing it is the image method.
        assert!(
            as_built.y < 0.0,
            "the offset ghost now pushes up ({:+.3} m/s²) — if `contact_accel`'s cohesion or range \
             handling changed, re-derive this whole argument rather than deleting the test",
            as_built.y
        );
        // ★ The overlap is zero to within a few ulp, NOT exactly zero — and the difference matters.
        // On the steps where rounding puts it marginally POSITIVE, `f_rep`'s gate opens and its
        // DAMPING term (not its spring, which is ~1e-12 m/s² at this overlap) fires at up to
        // ~1300 m/s². The bug was never "no upward force"; it was "no upward force that depends on
        // how deep you are, plus a bit-randomised one-sided damper".
        assert!(
            (2.0 * radius - (p - ghost_as_built).length()).abs() < 1e-15,
            "the offset ghost's whole problem is that its overlap is zero to within rounding"
        );

        // ★★★ AND NOW THE FLOOR THE SETTLER ACTUALLY USES. This is the gate: it must support a rod
        // that has sunk into it, and it must never hand energy back.
        let sunk = Rod {
            centre: DVec3::new(0.0, lowest + 0.0, 0.0),
            axis: DVec3::X, // lying flat, so centre height IS the lowest end's height
            half_length_m: 0.175,
            radius_m: radius,
            vel: DVec3::new(0.0, -1.0, 0.0), // driving downward into the floor
        };
        let hit = floor_contact(&sunk, radius, contact.friction);
        println!(
            "  the settler's floor now: hit={} · vel {:+.4} -> {:+.4} m/s · dpos {:+.3e} m",
            hit.hit, sunk.vel.y, hit.vel.y, hit.dpos.y
        );
        assert!(
            hit.hit,
            "a rod sunk to half its radius is in contact with the floor"
        );
        assert!(
            hit.dpos.y > 0.0,
            "the projection must push OUT of the surface, got {:+.3e}",
            hit.dpos.y
        );
        assert!(
            hit.vel.y >= 0.0,
            "the into-surface velocity must be removed, not reversed: {:+.4} m/s",
            hit.vel.y
        );
        // ★ The whole point of a constraint over a spring: it cannot give energy back.
        assert!(
            hit.vel.length_squared() <= sunk.vel.length_squared() + 1e-15,
            "the floor ADDED kinetic energy: {:.6e} -> {:.6e} m²/s²",
            sunk.vel.length_squared(),
            hit.vel.length_squared()
        );
    }

    /// The capsule stands for exactly the matter the assembly holds — the substitution that makes this
    /// a measurement of the member rather than of a rod somebody sized.
    #[test]
    fn the_capsule_holds_exactly_what_the_blade_holds() {
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let rod = Rod {
            centre: DVec3::ZERO,
            axis: DVec3::Y,
            half_length_m: length * 0.5,
            radius_m: radius,
            vel: DVec3::ZERO,
        };
        let want = blade.matter_volume_m3();
        assert!(
            (rod.volume_m3() - want).abs() <= want * 1e-9,
            "capsule {:.6e} m³ vs blade {want:.6e} m³",
            rod.volume_m3()
        );
    }

    /// Two segments' closest approach, checked on cases with known answers — the one piece of new
    /// geometry here, so it does not get to be taken on trust.
    #[test]
    fn closest_approach_of_two_segments() {
        // Crossed at right angles, one metre apart in y.
        let (a, b) = closest_points(
            DVec3::new(-1.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, 1.0, -1.0),
            DVec3::new(0.0, 1.0, 1.0),
        );
        assert!((a - DVec3::ZERO).length() < 1e-9);
        assert!((b - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
        // End to end, collinear and apart: the near ENDS are the answer.
        let (a, b) = closest_points(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(4.0, 0.0, 0.0),
        );
        assert!((a - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
        assert!((b - DVec3::new(3.0, 0.0, 0.0)).length() < 1e-9);
    }
}
