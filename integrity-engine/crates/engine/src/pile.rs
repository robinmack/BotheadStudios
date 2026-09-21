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
//! ### ~~★★ Rods cannot rotate~~ — SUPERSEDED 2026-09-15, and it said the opposite of the code
//!
//! ~~[`Rod::axis`] is set at construction and never updated: no angular velocity, no torque, no moment
//! of inertia, and a contact force found at an off-centre closest approach is applied as pure
//! translation to the centre. A dropped straw rotates to lie flat, which is the principal way a rod
//! heap densifies, so the release's uniform-on-the-sphere orientation is also the final one.~~
//!
//! **Every clause of that is now false.** `Rod` carries [`Rod::ang_vel`]; [`Rod::principal_inertia_kgm2`]
//! and [`Rod::inv_inertia_world`] give it a moment of inertia; [`Rod::apply_impulse_at`] turns an
//! off-centre impulse into `dw = I⁻¹(r × J)`; and the integrator rotates the body by `ang_vel · dt`,
//! carrying `axis` and `normal` with it. `docs/46` rows 72 and 73 are both **CLOSED**: rotation gained a
//! dissipation channel (damped 63×) and a capsule contact can now see axial spin.
//!
//! ★ It is kept struck through rather than deleted because of WHERE it was wrong. Rotation is not a
//! missing feature here — it is the mechanism row 79 spent a fortnight identifying (`|Δω| = 6.60e5 rad/s
//! in one step`, from a contact arm that bending made large, fixed by carrying the axial component as
//! torsion). A module header telling the next reader that rods have *no angular velocity*, at the top of
//! the module whose defect *was* angular velocity, is the most expensive kind of stale doc.
//!
//! What survives: that rod-heap **packing** depends on rods lying flat is not in doubt — but every
//! packing number this module has reported is void anyway (row 79), so it is not evidence for anything
//! until re-measured.

use crate::assembly::Assembly;
use crate::materials::Material;
use glam::DVec3;

/// A member of a pile, as the settler carries it: a segment with a radius, volume-exact against the
/// ★★★ **HOW BENT A MEMBER IS** (docs/46 row 75) — the shape state `flexure::Chain` governs.
///
/// A `Rod` was a rigid capsule, and for a grass blade that is not a small idealisation: Greenhill's
/// critical length for a dry blade is **20.8 cm against a 35 cm blade — 1.7× past it** — so it cannot
/// hold itself straight under its own weight. **Bending is the normal state and rigidity is the
/// approximation.**
///
/// The angles are stored RELATIVE to the rod's own axis, so `straight()` is all zeros and every
/// existing rod is bit-identical to before until something bends it. Contact still acts on the capsule
/// (changing that is a much larger blast radius, and row 75 says so); what this adds is a shape that
/// responds, and `tip_sag_m` is what a heap's packing will feel.
#[derive(Clone, Debug, PartialEq)]
pub struct Flex {
    /// Segment angles relative to the base tangent, radians. Empty means perfectly straight.
    pub theta: Vec<f64>,
    /// ★ **Which way the bend goes**, a unit vector across the member's axis (docs/46 row 77). Set by
    /// `Rod::relax_flex` from the LOAD, never from the member's roll — a blade sags the way gravity
    /// pulls it. `ZERO` for a straight member.
    pub bend_dir: DVec3,
}

impl Flex {
    /// A member with no bend in it — what every rod starts as, and what a rigid one stays.
    pub fn straight() -> Flex {
        Flex {
            theta: Vec::new(),
            bend_dir: DVec3::ZERO,
        }
    }

    /// How many segments this member is resolved into. `SEGMENTS` is the budget knob Law 8 asks about,
    /// and `flexure::Chain`'s convergence test is what says what it buys: 8 segments sit 1.45% of a
    /// length from the continuum elastica, 16 at 0.67%.
    pub const SEGMENTS: usize = 8;
}

/// assembly it stands for.
#[derive(Clone, Debug)]
pub struct Rod {
    pub centre: DVec3,
    /// Unit direction of the long axis.
    pub axis: DVec3,
    pub half_length_m: f64,
    /// Equal-volume capsule radius — **the CONTACT proxy only** (docs/46 row 70). Segment-segment
    /// closest approach is what the pile's collision uses, and a capsule is what that solves against.
    /// It is NOT what the air sees; see `width_m`.
    pub radius_m: f64,
    /// ★ **The member's REAL cross-section** — a blade is a ribbon, not a wire. Volume is what the
    /// capsule preserves and AREA is what drag integrates, so aerodynamics asks these instead.
    pub width_m: f64,
    pub thickness_m: f64,
    /// Unit normal of the broad face, perpendicular to `axis`. With `axis` this fixes the ribbon's
    /// orientation, which is the whole reason its presented area can swing 10:1 as it falls.
    pub normal: DVec3,
    pub vel: DVec3,
    /// ★ **Angular velocity, world frame, rad/s** (docs/46 row 60 step B). Without it a rod's `axis`
    /// was fixed for life: a blade balanced on its end stayed upright forever and one landing on a
    /// heap slid instead of toppling, so a "pile of blades" was a pile of arrows all still pointing
    /// the way they were released.
    pub ang_vel: DVec3,
    /// ★ **How bent it is** (docs/46 row 75). `Flex::straight()` for a rigid member.
    pub flex: Flex,
    /// ★ **When this member is thrown on**, s (docs/46 row 60 step C). A haystack is built forkful by
    /// forkful; before its moment a member is not in the world, so it neither falls nor collides.
    pub release_t_s: f64,
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

/// **The member's real cross-section**, `(width_m, thickness_m)` — the two dimensions that are not its
/// length, taken from its own parts rather than from an equal-volume fiction (docs/46 row 70).
///
/// Width is summed across parts (a fresh blade is lamina + midrib + lamina, side by side) and thickness
/// is the largest any part has, so a ribbon with a rib down it is as thick as the rib.
pub fn cross_section_for(member: &Assembly) -> Option<(f64, f64)> {
    let mut width = 0.0;
    let mut thickness: f64 = 0.0;
    for p in &member.parts {
        let h = p.shape.half_extents_m();
        let mut d = [2.0 * h.x, 2.0 * h.y, 2.0 * h.z];
        d.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        // d[0] is the length; the other two are the cross-section.
        width += d[1];
        thickness = thickness.max(d[2]);
    }
    (width > 0.0 && thickness > 0.0).then_some((width, thickness))
}

/// ★★★ **CLOSEST APPROACH BETWEEN TWO BENT BODIES** (docs/46 row 76) — the minimum over every pair of
/// their segments.
///
/// It calls [`closest_points`] for each pair, so there is exactly ONE closest-approach law in the pile
/// and a bent body is not a special case of anything: a straight member is a one-segment polyline and
/// takes the identical path it always did (Law II).
///
/// ★ Cost is `O(n·m)` in the segment counts, which is why `Flex::SEGMENTS` is small and why the caller
/// rejects distant pairs on a bounding check first — the same cheap reject the rigid version used.
pub fn closest_points_between(a: &[DVec3], b: &[DVec3]) -> (DVec3, DVec3) {
    let mut best = (
        a.first().copied().unwrap_or_default(),
        b.first().copied().unwrap_or_default(),
    );
    let mut best_d2 = f64::INFINITY;
    for wa in a.windows(2) {
        for wb in b.windows(2) {
            let (pa, pb) = closest_points(wa[0], wa[1], wb[0], wb[1]);
            let d2 = (pa - pb).length_squared();
            if d2 < best_d2 {
                best_d2 = d2;
                best = (pa, pb);
            }
        }
    }
    best
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
fn floor_contact(
    rod: &Rod,
    point_vel: DVec3,
    radius: f64,
    friction: f64,
) -> crate::granular::TerrainContact {
    let (e0, e1) = rod.ends();
    // ★ The WHOLE lower end, not its height bolted onto the centre's x and z. The old floor block
    // built `(centre.x, lowest, centre.z)` — a point that is on neither the rod nor the segment — and
    // got away with it only because its ghost differed in Y alone. It would stop getting away with it
    // the moment this floor is given a real heightfield, because `h` and its gradients are sampled at
    // (x, z). Passing the actual contact point costs nothing and removes the trap.
    let lower = if e0.y <= e1.y { e0 } else { e1 };
    crate::granular::terrain_contact_resolve(
        lower,
        point_vel,
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
#[derive(Clone, Debug, PartialEq)]
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
    /// ★★ **Packing against cell size** — `(cell_m, packing)`, coarse to fine. A single packing number
    /// is a claim about `cell_m`; this is the evidence for or against it being a claim about the HEAP.
    /// Look for a plateau: without one there is no bulk density to report (docs/46 row 78).
    pub packing_vs_cell: Vec<(f64, f64)>,
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
    /// ★★★ **IS ANY OF THIS A NUMBER?** (`docs/46` row 79.) Checked DIRECTLY on every member — centre,
    /// velocity, angular velocity, axis and every polyline node — and **not** through any statistic
    /// above, because those cannot report it. `height_m` and `peak_speed_ms` are `f64::max` folds and
    /// `f64::max` **returns the non-NaN operand**, so a heap of NaN reports a peak speed of 0.000000,
    /// a tidy height and a plausible packing. Row 79 records testing for NaN twice through exactly
    /// those instruments and clearing it both times.
    ///
    /// The general form, which is why this field exists rather than a comment: **an aggregate built
    /// from folds that skip NaN cannot report NaN.** Any summary of a body's state owes a direct
    /// predicate beside it.
    pub all_finite: bool,
    /// ★★★ **THE ENERGY GATE** (`docs/46` row 86). Gravity is the only source here, so the heap's
    /// mechanical energy may FALL — drag, damping and friction all remove — and must never RISE.
    /// `energy_j_at_release` is [`heap_energy_j`] at `t = 0`; `peak_energy_j` is its largest value over
    /// the run, sampled EVERY STEP rather than at trace intervals, because a peak between samples is a
    /// peak missed. `peak_energy_t_s` says when, which is what turns "it gained energy" into a place to
    /// look.
    ///
    /// ★ Read against the two bounded omissions named on [`mechanical_energy_j`] (the cohesion well and
    /// the terrain position projection). A small rise is those; a large one is not.
    pub energy_j_at_release: f64,
    pub peak_energy_j: f64,
    pub peak_energy_t_s: f64,
    /// The rotational share at the peak. The meter this replaced omitted rotation entirely, so this is
    /// the number that says how much of the heap's energy the old trace could not see.
    pub peak_rotational_energy_j: f64,
    /// When the heap's energy first stopped being a number, if it ever did. `Some(t)` is a harder
    /// failure than any ratio: an unbounded energy has no percentage.
    pub first_non_finite_energy_t_s: Option<f64>,
    /// Occupied cell COUNT at each resolution in [`Settled::packing_vs_cell`] — the quantity that
    /// actually exposes a degenerate heap. Row 79's tell was not the packing value but its SCALING:
    /// packing rose **exactly** `+700%` per halving, precisely the `cell³` factor, which can only
    /// happen if this count is CONSTANT. A constant of 1 is not a coarse measurement of a heap; it is
    /// a heap that is not there. Print the ratio between successive refinements (`docs/72` §3.9).
    pub cells_vs_cell: Vec<(f64, usize)>,
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
impl Rod {
    /// **Principal moments of inertia**, kg·m², about `(axis, width, normal)` — the standard box
    /// tensor for edge lengths `(L, W, T)`:
    ///
    /// ```text
    /// I_axis = m(W² + T²)/12 ,  I_width = m(L² + T²)/12 ,  I_normal = m(L² + W²)/12
    /// ```
    ///
    /// ★ The three differ by ~10⁷ for a grass blade, which is the point: it spins almost freely about
    /// its own length and resists tumbling end over end. A capsule would have made two of them equal
    /// and lost the distinction that makes a ribbon flutter rather than roll.
    pub fn principal_inertia_kgm2(&self, mass_kg: f64) -> DVec3 {
        let (l, w, t) = (2.0 * self.half_length_m, self.width_m, self.thickness_m);
        DVec3::new(
            mass_kg * (w * w + t * t) / 12.0,
            mass_kg * (l * l + t * t) / 12.0,
            mass_kg * (l * l + w * w) / 12.0,
        )
        .max(DVec3::splat(1.0e-30))
    }

    /// The body frame as three orthonormal world vectors: along, across-the-width, and the face normal.
    pub fn frame(&self) -> [DVec3; 3] {
        let along = self.axis.normalize_or(DVec3::X);
        let fallback = {
            let seed = if along.x.abs() < 0.9 {
                DVec3::X
            } else {
                DVec3::Y
            };
            along.cross(seed).normalize()
        };
        let n = (self.normal - along * along.dot(self.normal)).normalize_or(fallback);
        [along, along.cross(n), n]
    }

    /// **The inverse inertia tensor in WORLD axes**, kg⁻¹m⁻². The body's principal moments are
    /// diagonal in its own frame, so this is `F · diag(1/I) · Fᵀ` for the frame `F`.
    pub fn inv_inertia_world(&self, mass_kg: f64) -> glam::DMat3 {
        let f = self.frame();
        let i = self.principal_inertia_kgm2(mass_kg);
        let basis = glam::DMat3::from_cols(f[0], f[1], f[2]);
        basis * glam::DMat3::from_diagonal(DVec3::ONE / i) * basis.transpose()
    }

    /// ★★ **The effective-mass matrix at a point offset `r` from the centre** — what a contact there
    /// actually has to shove.
    ///
    /// `Δv_point = K · J` for an impulse `J` applied at that point, with
    /// `K = (1/m)·I₃ − [r]ₓ · I⁻¹ · [r]ₓ`. A point far out along a light blade is much EASIER to move
    /// than the blade's mass suggests, because the body can rotate out of the way instead of
    /// translating — and that is precisely the response a contact fed centre-of-mass velocity cannot
    /// see. Inverting this is what makes a constraint solved on a point velocity distribute correctly
    /// between travelling and turning.
    pub fn effective_mass_at(&self, mass_kg: f64, r: DVec3) -> glam::DMat3 {
        let skew = glam::DMat3::from_cols(
            DVec3::new(0.0, r.z, -r.y),
            DVec3::new(-r.z, 0.0, r.x),
            DVec3::new(r.y, -r.x, 0.0),
        );
        glam::DMat3::from_diagonal(DVec3::splat(1.0 / mass_kg.max(1e-30)))
            - skew * self.inv_inertia_world(mass_kg) * skew
    }

    /// ★★★ **THE FASTEST ANY OF THIS BODY'S MATTER IS MOVING**, m/s — what "is it at rest?" actually
    /// asks (docs/46 row 71, corrected).
    ///
    /// Row 71 taught `SettleGauge` about rotation by comparing `ω·L/2` — the tip speed — against the
    /// quiescent speed. That lever is right for a tumble and **wrong by `L/2r` for a spin about the
    /// body's own length**, which moves its surface by only `ω·radius`. For a grass blade that is a
    /// **325× over-statement**, and it is the rotation an anisotropic body ends up in: the axial moment
    /// is ~10⁴ below the others, so energy draining out of a heap collects there as fast spin carrying
    /// almost no energy at all. A heap can be genuinely at rest and be told it is moving at 0.595 m/s.
    ///
    /// The honest lever is geometric: the farthest any point of the body sits from the axis it is
    /// turning about. For a capsule that is an end's perpendicular distance from `ω̂`, plus the radius —
    /// which collapses to `radius` for an axial spin and to `L/2 + radius` for a tumble, with no cases.
    pub fn max_surface_speed_ms(&self) -> f64 {
        let w = self.ang_vel.length();
        let spin = if w <= 1.0e-30 {
            0.0
        } else {
            let w_hat = self.ang_vel / w;
            let end = self.axis.normalize_or(DVec3::X) * self.half_length_m;
            let perp = (end - w_hat * end.dot(w_hat)).length();
            w * (perp + self.radius_m)
        };
        self.vel.length() + spin
    }

    /// **This member's flexural rigidity**, N·m² — `E·I` from its own section and its own material.
    ///
    /// ★ Reads `Material::youngs_modulus`, which since docs/46 row 67 carries the MEMBER's modulus
    /// rather than the aggregate's — a blade's 1.06 GPa, not the sward mat's 5 MPa. Nothing is declared
    /// here: the second moment comes from the real ribbon section `w·t³/12`.
    pub fn flexural_rigidity_nm2(&self, _mats: &[Material], m: &Material) -> f64 {
        let i = self.width_m * self.thickness_m.powi(3) / 12.0;
        (m.youngs_modulus as f64 * i).max(f64::MIN_POSITIVE)
    }

    /// **Relax the member's shape to equilibrium** under its own weight, via `flexure::Chain` — the
    /// same `M = EI·κ` the elastica integrates, so there is no second bending law (Law II).
    ///
    /// ★★★ **THE LOAD DECIDES WHICH WAY IT BENDS, NOT THE ROLL** (docs/46 row 77). This used to hand
    /// `Chain` a body-frame constant `(0, −weight)` and lay the result out along `normal` — the seeded
    /// roll — so a member drooped sideways or **upward** depending on a random number. Measured: at a
    /// roll of 180° the tip rose **+0.296 m**. Matter does not fall upward, and Law I says the matter
    /// decides, not the seed.
    ///
    /// Gravity is projected into the plane perpendicular to the member's own axis (an axial component
    /// stretches a body, it does not bend it) and the bend runs along that direction. `bend_dir` is
    /// stored so `polyline` lays the shape out in the plane the load actually bent it in.
    ///
    /// ★ **Declared limitation, in the open:** a ribbon is far stiffer about its wide axis than its
    /// thin one, and this uses ONE rigidity for every direction. The honest form resolves the load into
    /// the two principal planes and gives each its own `I`. `Rod::normal` already fixes those planes,
    /// so the geometry is present and only the decomposition is missing — a blade edge-on to gravity
    /// currently bends as easily as one face-on, which it should not.
    /// ★★★ **ONE QUESTION, ONE IMPLEMENTATION** (Law II; `docs/46` row 79, `docs/72` item 2).
    ///
    /// "What shape does this member take under load" was answered by TWO functions — this one from
    /// [`step_one_rod`] every step, and [`Rod::relax_flex_under`] from the release — and they
    /// disagreed **by 169% of a member's length**, one arching up while the other drooped down. Every
    /// member was therefore placed in one shape and re-shaped into its mirror image on its very first
    /// step, moving matter 0.59 m with **no velocity**: no contact saw it, no energy accounted for it,
    /// and it is what opened the micron-scale overlaps that the step-2 investigation was chasing.
    ///
    /// This is now a thin wrapper, kept for its callers and for the degenerate guard it owns. The
    /// physics lives in one place.
    pub fn relax_flex(&mut self, ei_nm2: f64, weight_per_m: f64, g: f64) {
        let along = self.axis.normalize_or(DVec3::X);
        let down = DVec3::NEG_Y;
        // The part of gravity that is across the member — the only part that bends it.
        let transverse = down - along * along.dot(down);
        if transverse.length() <= 1.0e-12 || weight_per_m <= 0.0 || g <= 0.0 {
            // End-on to gravity: nothing bends it, and a straight member is a one-segment polyline.
            self.flex.theta.clear();
            self.flex.bend_dir = DVec3::ZERO;
            return;
        }
        self.relax_flex_under(ei_nm2, weight_per_m, &[]);
    }

    /// ★★★ **THE MEMBER'S ACTUAL SHAPE IN WORLD SPACE** — the nodes its matter occupies (docs/46 row
    /// 76).
    ///
    /// A straight member returns its two ends, so a rigid rod is exactly the segment it always was and
    /// nothing about it changes. A bent one returns `Flex::SEGMENTS + 1` nodes following the curve.
    /// This is what contact must be resolved against: a blade that droops 99.7% of its length is not
    /// where its axis says it is.
    ///
    /// The bend is taken in the plane spanned by `axis` and `normal`, which is the plane the ribbon is
    /// weakest in — a blade bends across its thin dimension, not its wide one, and `Rod::frame`
    /// already fixes that plane.
    pub fn polyline(&self) -> Vec<DVec3> {
        let base = self.centre - self.axis * self.half_length_m;
        if self.flex.theta.is_empty() {
            return vec![base, self.centre + self.axis * self.half_length_m];
        }
        let along = self.axis.normalize_or(DVec3::X);
        // ★ The direction the LOAD bent it in (docs/46 row 77), not the seeded roll.
        let bend_dir = self.flex.bend_dir.normalize_or(self.frame()[2]);
        let ds = 2.0 * self.half_length_m / self.flex.theta.len() as f64;
        let mut out = Vec::with_capacity(self.flex.theta.len() + 1);
        let mut p = base;
        out.push(p);
        for t in &self.flex.theta {
            // ★ MINUS. `Chain` works in a 2D plane whose +y is UP and whose load is `(0, −w)`, so its
            // angles come out NEGATIVE for a sagging body. `bend_dir` points the way the load pulls —
            // downward — so the two conventions are opposed, and adding them mirrored the sag into a
            // rise. Measured as a tip that went UP by 0.296 m at every roll.
            p += (along * t.cos() - bend_dir * t.sin()) * ds;
            out.push(p);
        }
        out
    }

    /// ★★★ **RELAX THE SHAPE UNDER EXTERNAL LOADS AS WELL AS ITS OWN WEIGHT** (docs/46 row 79).
    ///
    /// `loads` are `(arclength_m_from_base, force_N)` — where a neighbour pushes, and how hard. They
    /// are what makes a member bend *because something touched it* rather than only under gravity.
    ///
    /// ★★ **This is the honest answer to the pile's NaN.** A contact on a bent member's polyline acts
    /// up to a half-length off its own axis, and with an axial moment of 3.4e-10 kg·m² that arm turns
    /// an ordinary force into a 169 m/s tip speed — a runaway. A real blade does not spin up when
    /// pushed sideways; it BENDS. Sending the force into the shape is not a damper on the symptom, it
    /// is where the energy actually goes.
    ///
    /// Each load is projected into the member's bending plane, exactly as gravity is, and spread over
    /// the segment containing its arclength. `Chain` then finds equilibrium under the total — the same
    /// `M = EI·κ` as always, so no second bending law appears (Law II).
    pub fn relax_flex_under(&mut self, ei_nm2: f64, weight_per_m: f64, loads: &[(f64, DVec3)]) {
        let along = self.axis.normalize_or(DVec3::X);
        let l = 2.0 * self.half_length_m;

        // The bending direction is set by the TOTAL transverse load — gravity plus whatever is pushing
        // — because that is what decides which way the member actually gives.
        let mut total = DVec3::NEG_Y * (weight_per_m * l);
        for (_, f) in loads {
            total += *f;
        }
        let transverse = total - along * along.dot(total);
        let mag = transverse.length();
        if mag <= 1.0e-30 || l <= 0.0 {
            self.flex.theta.clear();
            self.flex.bend_dir = DVec3::ZERO;
            return;
        }
        let dir = transverse / mag;
        self.flex.bend_dir = dir;

        // Resolve every load into that direction. A load pushing the other way simply enters negative.
        let w_transverse = (DVec3::NEG_Y * weight_per_m).dot(dir);
        let n = Flex::SEGMENTS;
        let ds = l / n as f64;
        let mut per_segment = vec![0.0f64; n];
        for (s_m, f) in loads {
            let k = ((s_m / ds).floor() as usize).min(n - 1);
            // A point force becomes a load per unit length over the segment it lands on — the same
            // quantity `Chain` integrates, so nothing special-cases a point.
            per_segment[k] += f.dot(dir) / ds;
        }
        // ★★★ **SIGN** (`docs/46` row 79, `docs/72` item 2). `Chain`'s transverse load is NEGATIVE
        // along `bend_dir`: a load resolved as `+x` along `dir` bends the member the way a load of
        // `−x` physically would. Passing `+w_transverse` here arched every released member UPWARD,
        // against its own weight — measured as a tip at `y = +0.2958` where the step's relaxation put
        // it at `y = −0.2958`, exactly equal and opposite, a 0.59 m disagreement on a 0.35 m member.
        // No assert in this module could see it: `tip_sag_m` returns a MAGNITUDE (`sqrt(..)`), so a
        // blade arching up over its own weight reports exactly the same sag as one drooping down.
        let chain = crate::flexure::Chain::relaxed(ei_nm2, l, n, 0.0, |s| {
            let k = ((s / ds).floor() as usize).min(n - 1);
            (0.0, -(w_transverse + per_segment[k]))
        });
        self.flex.theta = chain.theta.clone();
    }

    /// **How far the tip has moved from where a rigid member's would be**, m — the deflection a heap
    /// actually feels, and the thing a rigid capsule reports as exactly zero.
    pub fn tip_sag_m(&self) -> f64 {
        if self.flex.theta.is_empty() {
            return 0.0;
        }
        let ds = 2.0 * self.half_length_m / self.flex.theta.len() as f64;
        let (mut x, mut y) = (0.0f64, 0.0f64);
        for t in &self.flex.theta {
            x += t.cos() * ds;
            y += t.sin() * ds;
        }
        // Straight would put the tip at (L, 0); the sag is how far it fell short of that.
        ((x - 2.0 * self.half_length_m).powi(2) + y * y).sqrt()
    }

    /// **The velocity of the material at a point offset `r` from the centre** — `v + ω × r`. The
    /// quantity every contact wanted and none was given (docs/46 row 72).
    pub fn velocity_at(&self, r: DVec3) -> DVec3 {
        self.vel + self.ang_vel.cross(r)
    }

    /// Apply an impulse at a point offset `r`: it both pushes and turns.
    pub fn apply_impulse_at(&mut self, mass_kg: f64, r: DVec3, impulse: DVec3) {
        self.vel += impulse / mass_kg.max(1e-30);
        let dw = self.inv_inertia_world(mass_kg) * r.cross(impulse);
        // ★★★ **ONE CALL EARLIER AGAIN** (docs/46 row 79). The neighbour contact was shown to be the
        // amplifier of an existing spin, not its source; this is where a spin is created. The axial
        // moment of a grass blade is 3.4e-10 kg·m², so `I⁻¹ ≈ 2.9e9` and any impulse with a moment arm
        // about the member's own axis is multiplied by three billion. A tip speed bound is the honest
        // scale to check against: `|Δω|·L/2` is a velocity, and a blade cannot acquire kilometres per
        // second from resting on the floor.
        debug_assert!(
            dw.is_finite() && dw.length() * self.half_length_m < 1.0e3,
            "impulse spun a member up absurdly: |Δω| {:e} rad/s (tip {:e} m/s) · J {:?} · |J| {:e} · \
             arm {:?} · I {:?}",
            dw.length(),
            dw.length() * self.half_length_m,
            impulse,
            impulse.length(),
            r,
            self.principal_inertia_kgm2(mass_kg)
        );
        self.ang_vel += dw;
    }

    /// Turn a world-frame angular impulse (N·m·s) into the angular velocity it adds.
    fn ang_vel_from_impulse(&self, mass_kg: f64, impulse: DVec3) -> DVec3 {
        let f = self.frame();
        let i = self.principal_inertia_kgm2(mass_kg);
        let b = DVec3::new(impulse.dot(f[0]), impulse.dot(f[1]), impulse.dot(f[2])) / i;
        f[0] * b.x + f[1] * b.y + f[2] * b.z
    }

    /// Rotate the body by `ang_vel · dt`, carrying `axis` and `normal` with it.
    fn spin(&mut self, dt: f64) {
        let w = self.ang_vel.length();
        if w * dt <= 1.0e-15 {
            return;
        }
        let q = glam::DQuat::from_axis_angle(self.ang_vel / w, w * dt);
        self.axis = (q * self.axis).normalize_or(self.axis);
        self.normal = (q * self.normal).normalize_or(self.normal);
    }
}

/// ★★★ **ONE ROD, ONE STEP — and the ONE integrator** (docs/46 row 60 step B).
///
/// `settle_traced`'s loop calls this after adding whatever its neighbours contribute, and so does any
/// test that wants a single member's motion. Writing a second stepper for the single-rod case is how
/// the two drift apart, and the drift is invisible until a pile disagrees with the blade it is made of.
///
/// `extra_accel` and `extra_torque` are the neighbour terms (m/s² and N·m). Gravity, the air, the floor
/// and the spin all live here.
#[allow(clippy::too_many_arguments)]
pub fn step_one_rod(
    rod: &mut Rod,
    mass_kg: f64,
    contact: &crate::granular::Contact,
    gravity_ms2: f64,
    air_density_kgm3: f64,
    dt: f64,
    extra_accel: DVec3,
    extra_torque: DVec3,
    // Flexural rigidity, N·m² — `0.0` keeps the member rigid, which is what every caller that does not
    // care about bending passes, and what makes this addition bit-identical for them.
    flex_ei_nm2: f64,
    // ★ Where neighbours are pushing on this member, `(arclength_m, force_N)` — the loads that bend it.
    contact_loads: &[(f64, DVec3)],
    // The member's own weight per unit length, N/m, for the shape relaxation.
    weight_per_m: f64,
) {
    // Gravity acts at the centre of mass and so exerts NO torque about it. Everything that turns this
    // rod turns it because the force arrived somewhere else.
    rod.vel += (extra_accel + DVec3::new(0.0, -gravity_ms2, 0.0)) * dt;

    if air_density_kgm3 > 0.0 {
        let f = rod.frame();
        let area = crate::atmosphere::box_frontal_area_m2(
            DVec3::new(2.0 * rod.half_length_m, rod.width_m, rod.thickness_m),
            f,
            rod.vel,
        );
        rod.vel += crate::atmosphere::drag_accel(
            air_density_kgm3,
            rod.vel,
            area,
            mass_kg,
            crate::atmosphere::FLAT_PLATE_NORMAL_DRAG_CD,
        ) * dt;
    }

    // ★★★ **THE SECOND SPIN PATH, AND IT WAS NEVER GUARDED** (docs/46 row 79). The `ang_vel` assert went
    // on `apply_impulse_at` — the FLOOR's path. The neighbour torque reaches `ang_vel_from_impulse`
    // directly, here, and with an axial `I⁻¹` of 2.9e9 a torque·dt of 1e-6 N·m·s is already 2900 rad/s.
    // Guarding one of two paths into the same state is the same error as asserting a term and not a sum.
    // ★★★ **THE AXIAL MOMENT IS TORSION, NOT SPIN** (docs/46 row 79 — a declared specialisation).
    //
    // A contact's moment about a slender member splits in two. PERPENDICULAR to its axis it tumbles the
    // body, `I ≈ 4.5e-6 kg·m²`, and that is ordinary rigid-body rotation which passes through
    // untouched. ABOUT its own axis it TWISTS a ribbon — and with `I_axial = 3.4e-10` (four orders
    // below), treating that as a free rigid-body degree of freedom gave `|Δω| = 6.6e5 rad/s` in a
    // single step and NaN by the third.
    //
    // A real blade does not spin up about its length when pushed sideways; it twists and bends, and the
    // moment is carried elastically — which `relax_flex_under` now does with the same force. This is
    // NOT a clamp on the torque: the perpendicular component is preserved exactly, so a member still
    // tumbles at the rate its inertia dictates, and
    // `a_moment_tumbles_a_member_but_does_not_spin_it_about_its_length` asserts both halves. A change
    // that killed the tumble as well would also stop the explosion, and would be wrong.
    let along = rod.axis.normalize_or(DVec3::X);
    let bending_moment = extra_torque - along * along.dot(extra_torque);
    let dw_n = rod.ang_vel_from_impulse(mass_kg, bending_moment * dt);
    debug_assert!(
        dw_n.is_finite() && dw_n.length() * rod.half_length_m < 1.0e2,
        "neighbour torque spun a member up: |Δω| {:e} rad/s (tip {:e} m/s) · τ {:?} · I {:?}",
        dw_n.length(),
        dw_n.length() * rod.half_length_m,
        extra_torque,
        rod.principal_inertia_kgm2(mass_kg)
    );
    let probe = spin_probe_on();
    let w_at_step_start = if probe { rod.ang_vel.length() } else { 0.0 };
    let mut d_neighbour_this_step = 0.0f64;
    #[allow(unused_assignments)]
    let mut d_precession_this_step = 0.0f64;
    let e_before_torque = if probe {
        rod_rotational_energy_j(rod, mass_kg)
    } else {
        0.0
    };
    rod.ang_vel += dw_n;
    if probe {
        let d = rod_rotational_energy_j(rod, mass_kg) - e_before_torque;
        d_neighbour_this_step = d;
        SPIN_BUDGET.with(|b| {
            let mut b = b.borrow_mut();
            b.steps += 1;
            let step = b.steps;
            let (mut t, mut m, mut f) = (
                b.neighbour_torque_j,
                b.max_finite_neighbour_j,
                b.first_nonfinite_neighbour_step,
            );
            fold_delta(&mut t, &mut m, &mut f, d, step);
            b.neighbour_torque_j = t;
            b.max_finite_neighbour_j = m;
            b.first_nonfinite_neighbour_step = f;
        });
    }
    // Free precession: a body whose principal moments differ does not spin about a fixed world axis.
    // τ_gyro = −ω × (I·ω), in the body frame where I is diagonal.
    {
        let f = rod.frame();
        let i = rod.principal_inertia_kgm2(mass_kg);
        let wb = DVec3::new(
            rod.ang_vel.dot(f[0]),
            rod.ang_vel.dot(f[1]),
            rod.ang_vel.dot(f[2]),
        );
        let iw = wb * i;
        let g = -(wb.cross(iw));
        let world = f[0] * g.x + f[1] * g.y + f[2] * g.z;
        let e_before_prec = if probe {
            rod_rotational_energy_j(rod, mass_kg)
        } else {
            0.0
        };
        let w_before = if probe { rod.ang_vel.length() } else { 0.0 };
        rod.ang_vel += rod.ang_vel_from_impulse(mass_kg, world * dt);
        if probe {
            let d = rod_rotational_energy_j(rod, mass_kg) - e_before_prec;
            d_precession_this_step = d;
            SPIN_BUDGET.with(|b| {
                let mut b = b.borrow_mut();
                let step = b.steps;
                let (mut t, mut m, mut f) = (
                    b.free_precession_j,
                    b.max_finite_precession_j,
                    b.first_nonfinite_precession_step,
                );
                fold_delta(&mut t, &mut m, &mut f, d, step);
                b.free_precession_j = t;
                b.max_finite_precession_j = m;
                b.first_nonfinite_precession_step = f;
                if d.is_finite() && d > b.worst_precession_j {
                    b.worst_precession_j = d;
                }
                if w_before.is_finite() {
                    if w_before > b.max_finite_omega_rads {
                        b.max_finite_omega_rads = w_before;
                    }
                    if !d.is_finite() && b.omega_at_precession_failure == 0.0 {
                        // The spin the body HAD going into the step that diverged — which is the
                        // number the stability bound predicts.
                        b.omega_at_precession_failure = w_before;
                    }
                }
            });
        }
    }

    if probe {
        let w_now = rod.ang_vel.length();
        SPIN_BUDGET.with(|b| {
            let mut b = b.borrow_mut();
            if b.first_excess_step == u64::MAX
                && b.omega_bound_rads > 0.0
                && w_now.is_finite()
                && w_now > b.omega_bound_rads
            {
                b.first_excess_step = b.steps;
                b.omega_before_excess = w_at_step_start;
                b.omega_after_excess = w_now;
                b.excess_d_neighbour_j = d_neighbour_this_step;
                b.excess_d_precession_j = d_precession_this_step;
                b.excess_d_floor_j = 0.0; // the floor runs after this point in the step
            }
        });
    }
    rod.centre += rod.vel * dt;
    rod.spin(dt);

    // ★★★ **THE SHAPE IS NOT RELAXED HERE, AND THAT IS A MEASUREMENT, NOT AN OMISSION** (docs/46 row
    // 76). Calling `Rod::relax_flex` from this loop was tried on 2026-09-02 and the heap came out
    // **bit-identical** — packing 0.00074, quiet at 5.220 s, peak |ω| 137.28244, every digit — while
    // the run went from **116 s to 1602 s, 13.8x slower.**
    //
    // Of course it did: CONTACT ACTS ON THE CAPSULE, so a computed shape has no reader. Bending a
    // member that nothing can feel bend is a pure cost. `Flex`, `relax_flex` and `Chain` stay — the
    // capability is proven and the sag is real (a dry blade droops 99.7% of its length) — but the hot
    // loop does not pay for a result nobody consumes. Wire this the day contact is resolved against the
    // chain, which is what actually lets blades tangle.
    // ★★★ NOW IT HAS A READER (docs/46 row 76). The shape was relaxed and discarded before, because
    // contact met the capsule — bit-identical results at 13.8x the cost. `closest_points_between` reads
    // the polyline, so bending a member finally changes what its neighbours feel.
    if flex_ei_nm2 > 0.0 {
        rod.relax_flex(flex_ei_nm2, weight_per_m, gravity_ms2);
    }

    // ★★ THE FLOOR, AND ITS MOMENT ARM. The constraint resolves at the rod's LOWER END, so the impulse
    // it applies is off-centre by up to a half-length — that arm is exactly why a blade on its end
    // topples. The old code took the velocity change and threw the arm away.
    let (e0, e1) = rod.ends();
    let lower = if e0.y <= e1.y { e0 } else { e1 };
    // ★★ THE FOOT IS ON THE SURFACE (docs/46 row 73). Re-applied 2026-08-30 to MEASURE the stability
    // boundary that two hand derivations disagreed about — see
    // `substep::tests::what_step_a_rotational_contact_mode_actually_needs`.
    let arm = (lower - DVec3::Y * contact.radius) - rod.centre;
    // ★★★ THE CONTACT SEES ROTATION (docs/46 row 72). The constraint is solved on the velocity of the
    // MATERIAL AT THE FOOT, `v + ω × r`, not on the centre of mass — a blade spinning in place has a
    // foot sliding at `ω·L/2` and used to present `v_rel = 0`, so neither its restitution damping nor
    // Coulomb friction could see the motion. Nothing spun down, ever.
    let v_foot = rod.velocity_at(arm);
    let hit = floor_contact(rod, v_foot, contact.radius, contact.friction);
    if hit.hit {
        if probe {
            // The lift, valued as the potential it hands over: m·g·Δy, with no velocity to pay for it.
            let d = mass_kg * gravity_ms2 * hit.dpos.y;
            SPIN_BUDGET.with(|b| b.borrow_mut().floor_lift_j += d);
        }
        let ke_before_floor = if probe {
            0.5 * mass_kg * rod.vel.length_squared()
        } else {
            0.0
        };
        rod.centre += hit.dpos;
        // The constraint says what the FOOT should now be doing. Distributing that between travelling
        // and turning needs the effective mass at the foot: `Δv = K·J`, so `J = K⁻¹·Δv`. A point far
        // out on a light blade is easier to move than the blade's mass suggests, because the body can
        // rotate out of the way — using `m` here instead would over-brake it.
        let k = rod.effective_mass_at(mass_kg, arm);
        let dv = hit.vel - v_foot;
        let impulse = k.inverse() * dv;
        // ★★★ **THE SECOND GATE** (docs/46 row 79). The finiteness assert above says a member has gone
        // bad; this says WHERE. It reports the inputs, so the first failure names its own cause instead
        // of leaving it to be guessed — which is what three wrong guesses cost.
        debug_assert!(
            // ★★★ A PHYSICAL BOUND, NOT A ROUND ONE (docs/46 row 79). This read `< 1.0` N·s, which for
            // a 0.445 GRAM blade permits `Δv = 2247 m/s` in one step — so the assert that was supposed
            // to catch the explosion was itself letting it through, and the search moved on to
            // `ang_vel` and to summed accelerations, both of which were innocent. A blade meets the
            // floor at its ~2 m/s terminal speed, so `m·10 m/s` is already generous.
            impulse.is_finite() && impulse.length() < mass_kg * 10.0,
            "floor impulse blew up: |J| {} · dv {:?} · arm {:?} · det(K) {:e}",
            impulse.length(),
            dv,
            arm,
            k.determinant()
        );
        let e_before_floor = if probe {
            rod_rotational_energy_j(rod, mass_kg)
        } else {
            0.0
        };
        rod.apply_impulse_at(mass_kg, arm, impulse);
        if probe {
            let d_rot = rod_rotational_energy_j(rod, mass_kg) - e_before_floor;
            let d_lin = 0.5 * mass_kg * rod.vel.length_squared() - ke_before_floor;
            SPIN_BUDGET.with(|b| {
                let mut b = b.borrow_mut();
                let step = b.steps;
                let (mut t, mut m, mut f) = (
                    b.floor_impulse_j,
                    b.max_finite_floor_j,
                    b.first_nonfinite_floor_step,
                );
                fold_delta(&mut t, &mut m, &mut f, d_rot, step);
                b.floor_impulse_j = t;
                b.max_finite_floor_j = m;
                b.first_nonfinite_floor_step = f;
                if d_lin.is_finite() {
                    b.floor_linear_j += d_lin;
                }
            });
        }
    }
}

/// ★★★ **THE SPACE A HEAP OCCUPIES, AT A STATED RESOLUTION** — m³, by occupancy on `cell` (docs/46 row
/// 78).
///
/// Walks each member's REAL shape (`Rod::polyline`), because contact has been resolved against the bent
/// polyline since row 76 and an envelope drawn round the straight axis measures a rod that is not there.
///
/// ★★ **The cell is not a detail, it is the question.** Packing is matter over envelope, and that ratio
/// has NO limit as the cell shrinks: a fine enough cell wraps each blade individually, the envelope
/// approaches the matter, and packing approaches 1 — which is a statement about a blade, not about a
/// heap. A bulk packing exists only as a PLATEAU, on cells coarse enough to bridge the gaps between
/// members and fine enough to follow the heap's outline. If there is no plateau, the heap has no bulk
/// density at that member count and saying one is inventing it.
pub fn envelope_m3_at(rods: &[Rod], cell: f64) -> f64 {
    if cell <= 0.0 {
        return 0.0;
    }
    let mut cells = std::collections::HashSet::new();
    for r in rods {
        for w in r.polyline().windows(2) {
            let (e0, e1) = (w[0], w[1]);
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
    }
    cells.len() as f64 * cell * cell * cell
}

/// ★ **THE RELEASE — one owner.** `settle_traced` drops members with these positions and
/// orientations, and a test that wants to know what a member was given must ask HERE rather than
/// rebuild the seeded draw for itself. It was briefly rebuilt in a test, which is a second
/// implementation of the release and would have drifted the first time either changed.
pub fn release_rods(member: &Assembly, count: usize, seed: u64) -> Option<Vec<Rod>> {
    release_rods_bent(member, count, seed, 0.0, 0.0)
}

/// ★★★ **RELEASED IN THE SHAPE THEY WILL HAVE** (docs/46 row 79).
///
/// MEASURED: with members released STRAIGHT, the first `relax_flex_under` at the end of step 1 bends
/// them all at once — a tip moves up to 99.7% of a length — and step 2 sees `|v|` jump from 9e-6 to
/// 7.3 m/s and `|ω|` from 6e-4 to 181 rad/s. **An 800,000x jump with nothing having physically moved.**
/// Matter teleported into its neighbours and the springs answered.
///
/// So the bend is applied to each candidate BEFORE the no-overlap rejection: what the release
/// guarantees is the absence of overlap between the shapes that will actually exist.
///
/// ★ Tried once before and it made things worse; at that time the axial-torsion explosion (6.6e5 rad/s
/// in one step) dominated and masked it. Retried only because that is now fixed, and measured.
pub fn release_rods_bent(
    member: &Assembly,
    count: usize,
    seed: u64,
    flex_ei_nm2: f64,
    weight_per_m: f64,
) -> Option<Vec<Rod>> {
    let (length, radius) = rod_for(member)?;
    let (width, thickness) = cross_section_for(member)?;
    let spread = length * 0.1;
    let touch = 2.0 * radius;

    // ★★★ **FORKFUL BY FORKFUL** (docs/46 row 60 step C). Robin: *"in a hay stack it's all gravity"* —
    // a haystack is built by throwing on forkfuls, each landing before the next arrives. That is not a
    // convenience, it is the only release that satisfies the invariant matter has anyway: **nothing may
    // start inside anything else.**
    //
    // ★ MEASURED, and it corrects both this row's own wording and my first reading of it. The release
    // is NOT uniformly interpenetrated — the vertical spread (`0..2·length`) separates members even
    // though the horizontal disc is only `0.1·length` across. It degrades with crowding:
    //
    //     10 blades   0/45 pairs        20 blades   1/190      40 blades   6/780
    //    100 blades  25/4950          200 blades 149/19900    400 blades 553/79800, fully coincident
    //
    // At the shipped 400 that is a 3.14 m/s kick on the first step — larger than the 2 m/s a blade
    // reaches falling — so the heap was being blown apart before gravity got a word in. At 10 it never
    // happened at all, which is why the small-count measurements are unaffected by this fix and why a
    // claim that it explains them would have been wrong.
    //
    // A forkful is however many members can be placed WITHOUT overlapping. When the next one cannot be
    // fitted, the forkful is full and the rest wait for the following throw — no batch size is chosen.
    // The interval is the fall time from the release height, `√(2h/g)`, plus the same `√(2L/g)` the
    // settle gauge already uses to decide a member is supported: nothing new is declared.
    let ceiling = 2.0 * length;
    let interval = (2.0 * ceiling / 9.81).sqrt() + (2.0 * length / 9.81).sqrt();

    let mut rods: Vec<Rod> = Vec::with_capacity(count);
    let mut forkful_start = 0usize; // index of the first member of the forkful being built
    let mut forkful = 0u32;
    let mut attempt = 0u64;
    while rods.len() < count {
        let i = rods.len() as u64;
        // ★★★ ORIENTATION IS DRAWN ONCE, FROM `i`; ONLY THE POSITION IS RETRIED, FROM `k`.
        //
        // MEASURED THE WRONG WAY FIRST. Re-drawing the whole member on each rejection biased the
        // population: a blade 0.35 m long in a disc 0.07 m across fits more easily lying DOWN, because
        // a horizontal blade reaches out of the crowded disc into empty space. At 400 members the mean
        // `|axis·ŷ|` came out **0.4369 against a uniform 0.5000** — 4.4 standard errors low, a real
        // tilt toward horizontal, and a flatter release packs differently. Fixing interpenetration by
        // silently re-shaping the orientation distribution would have traded one defect for a subtler
        // one, and `Rod::axis` is drawn uniformly on the sphere precisely because *a dropped blade has
        // no preferred direction*.
        let z = 2.0 * unit(seed, i, 3) - 1.0;
        let phi = unit(seed, i, 4) * std::f64::consts::TAU;
        let sph = (1.0 - z * z).max(0.0).sqrt();
        let axis = DVec3::new(sph * phi.cos(), z, sph * phi.sin()).normalize();
        let k = i * 64 + (attempt % 64);
        let (u1, u2) = (unit(seed, k, 1), unit(seed, k, 2));
        let r = spread * u1.sqrt();
        let a = u2 * std::f64::consts::TAU;
        let centre = DVec3::new(
            r * a.cos(),
            radius + unit(seed, k, 5) * ceiling,
            r * a.sin(),
        );
        let cand = Rod {
            centre,
            axis,
            half_length_m: length * 0.5,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: {
                let seed_v = if axis.x.abs() < 0.9 {
                    DVec3::X
                } else {
                    DVec3::Y
                };
                let u = axis.cross(seed_v).normalize();
                let v = axis.cross(u);
                let roll = unit(seed, i, 6) * std::f64::consts::TAU;
                (u * roll.cos() + v * roll.sin()).normalize()
            },
            vel: DVec3::ZERO,
            ang_vel: DVec3::ZERO,
            release_t_s: forkful as f64 * interval,
            flex: Flex::straight(),
        };
        // ★ Bend it FIRST, so the rejection sees the shape that will exist.
        let mut cand = cand;
        if flex_ei_nm2 > 0.0 {
            cand.relax_flex_under(flex_ei_nm2, weight_per_m, &[]);
        }
        // Only against the CURRENT forkful: earlier ones are already on the heap and out of the way.
        let cand_poly = cand.polyline();
        let clash = rods[forkful_start..].iter().any(|o| {
            let (pa, pb) = closest_points_between(&cand_poly, &o.polyline());
            (pa - pb).length() < touch
        });
        if !clash {
            rods.push(cand);
            attempt = 0;
            continue;
        }
        attempt += 1;
        if attempt >= 64 {
            // This forkful is full — the disc cannot hold another blade without one lying inside
            // another. Throw the next one.
            forkful += 1;
            forkful_start = rods.len();
            attempt = 0;
        }
    }
    Some(rods)
}

pub fn settle(
    member: &Assembly,
    mats: &[Material],
    count: usize,
    gravity_ms2: f64,
    air_density_kgm3: f64,
    seed: u64,
) -> Option<Settled> {
    settle_traced(
        member,
        mats,
        count,
        gravity_ms2,
        air_density_kgm3,
        seed,
        0.0,
    )
    .map(|(s, _)| s)
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
    /// ★ **The lowest END of any member, m** — how close the heap is to the floor, as distinct from
    /// `height_m`, which is its TOP. Added 2026-08-26 because a test needed to know *has anything
    /// landed yet* and had been inferring it from `height_m` falling. That inference was valid only
    /// while rods could not turn: once they can, a landed blade TOPPLES, and toppling drops the top of
    /// the heap just as surely as falling does. The two readings look identical and mean opposite
    /// things, so the honest fix is to report the quantity that was actually wanted.
    pub lowest_end_m: f64,
    /// ★ **The fastest member's angular speed, rad/s.** With `peak_speed_ms` this separates a heap that
    /// is sliding from one that is tumbling — and it is exactly zero for a body in free flight, since
    /// gravity acts at the centre of mass and drag at the area centroid, so neither exerts a torque.
    /// That makes it the one exact test for *has anything touched yet*.
    pub peak_ang_speed_rads: f64,
    /// ★ **The fastest any member's MATTER is moving**, m/s — `Rod::max_surface_speed_ms`, which is
    /// what the settle gauge is actually shown. Reported alongside `peak_ang_speed_rads` because the
    /// two can disagree wildly: an axial spin of 137 rad/s moves a blade's surface at 0.074 m/s, and
    /// reading `ω·L/2` instead says 24 m/s. A trace that reports the angular speed alone invites the
    /// same 325x misreading the gauge itself made.
    pub peak_surface_speed_ms: f64,
    /// ★ **What fraction of members are TOUCHING something** — the number that decides whether block
    /// timestepping can help at all (docs/46 row 69). A population where everyone is in contact pays
    /// the stiff step no matter how cleverly it is scheduled.
    pub contacting_fraction: f64,
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

/// ★★★ **WHERE A STEP'S SPIN CAME FROM** (`docs/46` row 87).
///
/// Row 86 measured that the heap's energy leaves the reals at `t = 0.46423 s` and that the last finite
/// peak is **rotational — 5.85e186 J from a 1.7e-2 J release**. That says WHICH CHANNEL and says nothing
/// about WHICH TERM, and `ang_vel` is written in three different places in a step. Row 79 died eleven
/// times on plausible reasoning about exactly this, so this attributes the rotational energy change to
/// the term that caused it, the same way [`crate::granular::ContactTerms`] attributes a contact force.
///
/// The three writers, and what each is entitled to do:
/// - **neighbour torque** — a contact's moment perpendicular to the member's axis. May add or remove.
/// - **free precession** — `τ = −ω × (I·ω)`, torque-free rotation. ★ **This one is entitled to add
///   NOTHING.** Torque-free motion conserves both angular momentum and energy exactly, so any energy it
///   gains is integration error, not physics — and with `I = (3.4e-10, 4.6e-6, 4.6e-6)` the axial moment
///   is four orders below the others, which is precisely when an explicit step of Euler's equations goes
///   unstable.
/// - **floor impulse** — a constraint. May only remove (`docs/46` row 36's non-injecting contact).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct SpinBudget {
    pub neighbour_torque_j: f64,
    pub free_precession_j: f64,
    pub floor_impulse_j: f64,
    /// The floor's change to TRANSLATIONAL kinetic energy. A constraint that removes into-surface
    /// velocity should make this strongly negative.
    pub floor_linear_j: f64,
    /// ★ The floor's **position projection** (`centre += dpos`), valued as `m·g·Δy`. It lifts a member
    /// out of the surface with no velocity change and no matching term anywhere — the blind spot this
    /// module's own header names. Energy from nothing, if it is positive.
    pub floor_lift_j: f64,
    pub steps: u64,
    /// The single worst one-step gain from free precession, and when.
    pub worst_precession_j: f64,
    pub worst_precession_t_s: f64,
    /// ★★★ **WHICH TERM STOPPED BEING A NUMBER FIRST** — by rod-step index, `u64::MAX` for "never".
    ///
    /// A running sum is destroyed by a single NaN delta: the first measurement past the blow-up turned
    /// `neighbour_torque_j` and `free_precession_j` into `NaN` and the attribution said nothing at all.
    /// That is the same defect as `f64::max` returning the non-NaN operand (row 79) and the energy
    /// gate's own first version (row 86), committed a third time, in a probe built to study it.
    ///
    /// So: **only finite deltas are summed, and the first non-finite one is RECORDED per term.** The
    /// term that goes bad first is the term the explosion enters through — which is the whole question.
    pub first_nonfinite_neighbour_step: u64,
    pub first_nonfinite_precession_step: u64,
    pub first_nonfinite_floor_step: u64,
    /// Largest FINITE one-step gain seen per term, so a spike that precedes the NaN is still visible.
    /// ★ The largest FINITE `|ω|` seen, and the `|ω|` on the step precession stopped being a number.
    /// The derived stability bound for the explicit torque-free step is `ω_crit = I_axial /
    /// ((I_perp − I_axial)·dt)`; these are what close the arithmetic against it (`docs/46` row 87).
    pub max_finite_omega_rads: f64,
    pub omega_at_precession_failure: f64,
    /// ★★★ **THE FIRST EXCESSIVE SPIN, AND WHO PUT IT THERE.** Chasing the first NON-FINITE value found
    /// precession overflowing at `|ω| = 1.96e96` — which says only that it overflowed last, not that it
    /// grew anything. Row 79's rule is to instrument the first EXCESSIVE value instead: the step where
    /// `|ω|` first passes a bound physics cannot reach, with each term's contribution to that step.
    pub omega_bound_rads: f64,
    pub first_excess_step: u64,
    pub omega_before_excess: f64,
    pub omega_after_excess: f64,
    pub excess_d_neighbour_j: f64,
    pub excess_d_precession_j: f64,
    pub excess_d_floor_j: f64,
    pub max_finite_neighbour_j: f64,
    pub max_finite_precession_j: f64,
    pub max_finite_floor_j: f64,
}

/// Fold one finite delta into a total, or record where it stopped being a number.
#[inline]
fn fold_delta(total: &mut f64, max: &mut f64, first_bad: &mut u64, d: f64, step: u64) {
    if d.is_finite() {
        *total += d;
        if d > *max {
            *max = d;
        }
    } else if *first_bad == u64::MAX {
        *first_bad = step;
    }
}

static SPIN_PROBE_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
thread_local! {
    static SPIN_BUDGET: std::cell::RefCell<SpinBudget> = const {
        std::cell::RefCell::new(SpinBudget {
            neighbour_torque_j: 0.0,
            free_precession_j: 0.0,
            floor_impulse_j: 0.0,
            floor_linear_j: 0.0,
            floor_lift_j: 0.0,
            steps: 0,
            worst_precession_j: 0.0,
            worst_precession_t_s: 0.0,
            first_nonfinite_neighbour_step: u64::MAX,
            first_nonfinite_precession_step: u64::MAX,
            first_nonfinite_floor_step: u64::MAX,
            omega_bound_rads: 0.0,
            first_excess_step: u64::MAX,
            omega_before_excess: 0.0,
            omega_after_excess: 0.0,
            excess_d_neighbour_j: 0.0,
            excess_d_precession_j: 0.0,
            excess_d_floor_j: 0.0,
            max_finite_omega_rads: 0.0,
            omega_at_precession_failure: 0.0,
            max_finite_neighbour_j: 0.0,
            max_finite_precession_j: 0.0,
            max_finite_floor_j: 0.0,
        })
    };
}

/// Start attributing rotational energy. Off by default and read through an atomic, so a normal run pays
/// one relaxed load per step rather than six inertia-tensor evaluations per member.
/// Set the bound `|ω|` may not pass — DERIVED, not typed: ten times what a member falling at its own
/// terminal velocity could spin at (`10·v_term / half_length`), which is already far beyond anything
/// gravity and air can produce and close to the explicit torque-free step's own stability limit.
pub fn spin_probe_set_bound(omega_bound_rads: f64) {
    SPIN_BUDGET.with(|b| b.borrow_mut().omega_bound_rads = omega_bound_rads);
}

pub fn spin_probe_begin() {
    SPIN_BUDGET.with(|b| {
        *b.borrow_mut() = SpinBudget {
            first_nonfinite_neighbour_step: u64::MAX,
            first_nonfinite_precession_step: u64::MAX,
            first_nonfinite_floor_step: u64::MAX,
            first_excess_step: u64::MAX,
            ..Default::default()
        }
    });
    SPIN_PROBE_ON.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Stop attributing, and hand over what was attributed.
pub fn spin_probe_take() -> SpinBudget {
    SPIN_PROBE_ON.store(false, std::sync::atomic::Ordering::Relaxed);
    SPIN_BUDGET.with(|b| *b.borrow())
}

#[inline]
fn spin_probe_on() -> bool {
    SPIN_PROBE_ON.load(std::sync::atomic::Ordering::Relaxed)
}

/// One member's rotational kinetic energy, `½ ω·(I ω)` in its own principal frame.
pub fn rod_rotational_energy_j(rod: &Rod, mass_kg: f64) -> f64 {
    let i = rod.principal_inertia_kgm2(mass_kg);
    let f = rod.frame();
    let wb = DVec3::new(
        rod.ang_vel.dot(f[0]),
        rod.ang_vel.dot(f[1]),
        rod.ang_vel.dot(f[2]),
    );
    0.5 * (i.x * wb.x * wb.x + i.y * wb.y * wb.y + i.z * wb.z * wb.z)
}

/// **The member's own geometry, as a mesh** — the shape it is actually in, not the shape it was
/// authored with.
///
/// Same rule as [`crate::assembly::Assembly::mesh`]: derived from the body, never authored beside it.
/// The centreline is [`Rod::polyline`] — which is what `pile`'s CONTACT already resolves against
/// (`docs/46` row 76) — so the drawn shape and the collided shape are the same shape. Drawing a bent
/// member straight would put a picture on screen that disagrees with the physics, and `docs/68`'s
/// illusion test permits an illusion only while **nothing interacts with it**; a blade's neighbours
/// interact with exactly this curve.
///
/// A rigid member is a two-node polyline, so it costs one box, as it always did.
pub fn rod_mesh(rod: &Rod, mat: u32, color: [f32; 3]) -> crate::mesher::Mesh {
    let nodes: Vec<[f32; 3]> = rod
        .polyline()
        .iter()
        .map(|p| [p.x as f32, p.y as f32, p.z as f32])
        .collect();
    let n = rod.normal;
    crate::mesher::build_swept_ribbon(
        &nodes,
        [n.x as f32, n.y as f32, n.z as f32],
        rod.width_m as f32,
        rod.thickness_m as f32,
        mat,
        color,
    )
}

/// The whole heap as one mesh — every member in the shape it is in, concatenated.
///
/// ★ **Members that are not numbers are DROPPED, and the count of them is returned** rather than
/// silently skipped or silently drawn. `docs/46` rows 85/86: this heap goes NaN at contact, and a NaN
/// vertex does not draw as anything — it disappears, or it takes its whole triangle strip with it. A
/// renderer that quietly omitted them would produce a thinning, plausible-looking haystack while the
/// simulation was destroying itself, which is precisely how this module's other instruments lied. The
/// caller is told, so the picture can say so.
pub fn heap_mesh(rods: &[Rod], mat: u32, color: [f32; 3]) -> (crate::mesher::Mesh, usize) {
    let mut out = crate::mesher::Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut dropped = 0usize;
    for r in rods {
        if !(r.centre.is_finite() && r.axis.is_finite() && r.normal.is_finite())
            || !r.polyline().iter().all(|p| p.is_finite())
        {
            dropped += 1;
            continue;
        }
        let m = rod_mesh(r, mat, color);
        let base = out.vertices.len() as u32;
        out.vertices.extend_from_slice(&m.vertices);
        out.indices.extend(m.indices.iter().map(|i| i + base));
    }
    (out, dropped)
}

/// ★★★ **ONE MECHANICAL ENERGY, AND IT COUNTS THE SPIN** (`docs/46` rows 79, 84, 86).
///
/// This module had **two** energy meters and the production one was blind to the channel the pile
/// actually fails through. `Sample::energy_j` summed `½mv² + mgy` — **translation and height only** —
/// while a test helper summed `½mv² + ½Iω² + mgy`. Row 79's entire explosion was ROTATIONAL
/// (`|Δω| = 6.60e5 rad/s in one step`), so **a member could spin up without bound and the trace would
/// not move.** A meter that cannot see the known failure mode is not a meter.
///
/// One implementation, used by the trace and by the gate, so there is no second energy to disagree
/// with the first (Law II — the same structure as `relax_flex` delegating, row 84).
///
/// ★ **WHAT IT STILL DOES NOT COUNT**, stated because a gate built on an unstated omission is how row
/// 79 spent a week: (a) the **cohesion potential** — grass carries `cohesion = 12000 Pa`, so touching
/// members sit in an attractive well this does not integrate, and sinking into it raises nothing here
/// while climbing out lowers nothing; (b) `granular::terrain_contact_resolve`'s **position
/// projection**, which lifts a member out of the surface with no matching term, so it can raise `mgy`
/// for free. Both are BOUNDED and small; neither is zero. A rise is therefore evidence to chase, and
/// the honest confirmation is the isolated single-body control in
/// `energy_gate_tests::an_isolated_falling_member_conserves_energy`, where all three are absent by
/// construction.
pub fn mechanical_energy_j(rod: &Rod, mass_kg: f64, g: f64) -> f64 {
    let i = rod.principal_inertia_kgm2(mass_kg);
    let f = rod.frame();
    let wb = DVec3::new(
        rod.ang_vel.dot(f[0]),
        rod.ang_vel.dot(f[1]),
        rod.ang_vel.dot(f[2]),
    );
    0.5 * mass_kg * rod.vel.length_squared()
        + 0.5 * (i.x * wb.x * wb.x + i.y * wb.y * wb.y + i.z * wb.z * wb.z)
        + mass_kg * g * rod.centre.y
}

/// The heap's total mechanical energy — [`mechanical_energy_j`] summed. Gravity is the only source, so
/// this may FALL (drag, damping, friction) and must never RISE beyond the bounded blind spots above.
pub fn heap_energy_j(rods: &[Rod], mass_kg: f64, g: f64) -> f64 {
    rods.iter()
        .map(|r| mechanical_energy_j(r, mass_kg, g))
        .sum()
}

/// The ROTATIONAL share alone, so the channel the old meter could not see is visible on its own.
pub fn heap_rotational_energy_j(rods: &[Rod], mass_kg: f64) -> f64 {
    rods.iter()
        .map(|r| {
            let i = r.principal_inertia_kgm2(mass_kg);
            let f = r.frame();
            let wb = DVec3::new(
                r.ang_vel.dot(f[0]),
                r.ang_vel.dot(f[1]),
                r.ang_vel.dot(f[2]),
            );
            0.5 * (i.x * wb.x * wb.x + i.y * wb.y * wb.y + i.z * wb.z * wb.z)
        })
        .sum()
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
    air_density_kgm3: f64,
    seed: u64,
    trace_every_s: f64,
) -> Option<(Settled, Vec<Sample>)> {
    settle_watched(
        member,
        mats,
        count,
        gravity_ms2,
        air_density_kgm3,
        seed,
        trace_every_s,
        0.0,
        &mut |_, _| {},
    )
}

/// ★★★ **THE SETTLE, WITH SOMEONE WATCHING** — the same one implementation, which is the point.
///
/// Until now this module returned **statistics and never state**: `Settled` and a `Vec<Sample>` of
/// scalars. Nothing could ask *what the heap looked like* at a moment, so nothing could draw it, and the
/// only way to see a haystack form was to infer it from numbers. That is the same gap `Settled::
/// all_finite` closed for "is it a number" (row 85) — a summary is not the state it summarises.
///
/// `on_frame(t, rods)` is handed the members themselves, every `watch_every_s` seconds of simulated
/// time (`0.0` = never). It is an OBSERVER: it may read and copy, and the settle does not care what it
/// does. `settle_traced` is this with an observer that does nothing and `settle` is that with the trace
/// discarded, so there is still exactly one settling law and no second one to disagree with it
/// (`docs/46` row 84).
///
/// ★ The caller sees the members in whatever shape they are actually in, bent polylines and all —
/// `Rod::polyline()` is the matter, not the capsule (row 76). A renderer asking this question gets the
/// truth the physics holds, rather than a summary someone chose to keep.
#[allow(clippy::too_many_arguments)]
pub fn settle_watched(
    member: &Assembly,
    mats: &[Material],
    count: usize,
    gravity_ms2: f64,
    // Air density at the pile, kg/m³ — **0.0 is a vacuum**. The Earth assembly supplies this; the Moon
    // supplies zero. A parameter and not a constant because whether there is air is a property of
    // where the pile IS, not of piles.
    air_density_kgm3: f64,
    seed: u64,
    trace_every_s: f64,
    watch_every_s: f64,
    on_frame: &mut dyn FnMut(f64, &[Rod]),
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
    // Not bound for use here — `release_rods` reads the cross-section itself. Called for its REFUSAL:
    // a member with no cross-section is not a rod, and `settle` should decline rather than build a
    // pile of degenerate ribbons that present no area to the air.
    cross_section_for(member)?;
    let member_mass = member.mass_kg(mats).ok()?.max(1e-12);
    let contact = crate::granular::contact_from_material(m, radius, member_mass);
    // ★ The member's own EI, from its real section and its own material's modulus (docs/46 rows 67, 75).
    let flex_ei = {
        let (w, t) = cross_section_for(member).unwrap_or((0.0, 0.0));
        m.youngs_modulus as f64 * w * t.powi(3) / 12.0
    };

    // Released over a disc a few lengths across, stacked upward so they fall rather than start merged.
    // ★★ **DROPPED IN ONE PLACE**, which is Robin's own wording and is not a detail: a heap's packing
    // is a property of straw only if the heap forms by its OWN repose. MEASURED the other way first —
    // released over a disc wider than a blade is long, 400 blades settled at 0.0005, which is a
    // measurement of how thinly they were scattered rather than of how they pack. A point source lets
    // the pile spread to the angle the contact law gives it.
    let w_per_m = member_mass / rod_for(member)?.0 * gravity_ms2;
    let rods_v = release_rods_bent(member, count, seed, flex_ei, w_per_m)?;
    let mut rods: Vec<Rod> = rods_v;
    // The energy the release put in, measured on the shapes that will actually exist (row 84's lesson:
    // the state a body is PLACED in must be the state the stepper would produce, so E0 is taken here
    // and not from an idealised construction).
    let energy_j_at_release = heap_energy_j(&rods, member_mass, gravity_ms2);

    // ★★★ **A CONTACT HAS TWO TIMESCALES AND THIS RULE USED TO SEE ONLY ONE.**
    //
    // ~~"ω = √stiffness, and a tenth of that period is stable"~~ — true of the SPRING and silent
    // about the DAMPER, which is a separate explicit term with its own stability limit. An explicit
    // velocity update `v -= c·v·dt` diverges once `c·dt` approaches 2, and `granular`'s own module
    // doc names the heap version of exactly this: *"explicit damping overshoots and pumps energy once
    // `Z·c·dt` nears 2"* — which is why the GPU path moves the damper into an implicit solve.
    //
    // MEASURED, 2026-08-16, and it is why this rule is here: correcting the restitution calibration
    // (docs/46 row 62) roughly DOUBLED `c`, which halved the coordination number at which the
    // integrator starts pumping — from Z ≈ 14.5 to Z ≈ 7.3, a number a real heap reaches easily. The
    // 400-rod heap went from a 0.46 m pile to a **14.94 m** one, peak speed 4.88 m/s, with the energy
    // trace showing a **+2075% drawup**. Nothing was wrong with the new damping; the timestep was
    // being chosen by a rule that could not see it.
    //
    // So the step respects both: a tenth of the spring's period AND a tenth of the damping time
    // constant `1/c`. Both are derived, neither is chosen, and the heap's own energy trace is what
    // says whether they are sufficient.
    // ★★★ THE STEP, DERIVED (docs/46 row 69, `substep`). This was `0.1/√stiffness`, whose `0.1`
    // traced to nothing. `substep::accurate_dt_s` gets it from the contact's own duration `π/ω`
    // divided by a resolution that was MEASURED, not chosen — see
    // `substep::tests::how_finely_a_contact_must_be_resolved`, which integrates one bounce through
    // `granular::contact_accel` and compares against the analytic restitution.
    //
    // ★ The retired dial worked out to 31.4 steps per contact, which the measurement puts at ~2.2%
    // error in restitution. So it was approximately RIGHT and merely unjustified — worth saying
    // plainly, because "it was a dial" and "it was wrong" are different findings and only the first
    // one is true here. 32 keeps that behaviour with a reason attached; 64 would halve the error and
    // double the cost, and the catalogue's own restitutions are not known to better than a few
    // percent, so buying below the data's uncertainty would be spending for nothing.
    const STEPS_PER_CONTACT: f64 = 32.0;
    let dt =
        crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, STEPS_PER_CONTACT)
            .min(1e-3);

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
    const CAP_S_DEFAULT: f64 = 20.0;
    // ★ A MEASUREMENT KNOB, not a physics one. The cap answers "how long may this run before we report
    // what it is doing"; it does NOT decide whether the heap settled — `SettleGauge` does, and `quiet`
    // reports its verdict independently. Shortening the cap can only ever make `quiet` FALSE, never
    // true, so it cannot be used to declare a heap settled early. It exists because this heap runs
    // 20 s at dt ≈ 4.27e-7, which is ~47 MILLION steps with an O(n²) neighbour loop and a
    // closest-point solve per polyline pair — and a measurement you cannot afford to take is a
    // measurement you do not have (row 79's figures were all taken before that cost landed).
    let cap_s: f64 = std::env::var("PILE_CAP_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v: &f64| *v > 0.0)
        .unwrap_or(CAP_S_DEFAULT);
    let mut elapsed_s = 0.0f64;
    let mut peak_speed = 0.0f64;
    let mut peak_energy_j = f64::NEG_INFINITY;
    let mut peak_energy_t_s = 0.0f64;
    let mut peak_rotational_energy_j = 0.0f64;
    let mut first_non_finite_energy_t_s: Option<f64> = None;

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
    let mut next_watch = 0.0f64;
    // ★★★ THE DIRECT QUESTION (docs/46 row 78). `peak centre 0.000000` with gravity inside the
    // integrator is impossible unless the loop never calls it. Three probes went to hypotheses before
    // anyone counted the calls.
    let (mut stepped, mut skipped) = (0u64, 0u64);
    // ★★ **THE STEP-2 DECOMPOSITION** (`docs/72` queue item 2). Set `PILE_STEP_TRACE=<n>` to print, for
    // the first `n` steps, the single hardest contact in the step broken into its terms. The queue
    // reads an overlap of 2.2 mm off a step-2 acceleration by assuming `a = k·overlap` — spring only —
    // while this module's own header records that `f_rep` is dominated by its DAMPING term. A total
    // cannot distinguish those; a decomposition can, and eleven hypotheses have already been refuted
    // by argument, so this one is measured (row 79).
    let trace_steps: u64 = std::env::var("PILE_STEP_TRACE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut step_n = 0u64;
    while elapsed_s < cap_s {
        let snapshot = rods.clone();
        // Hardest contact THIS step: (|a|, i, j, terms, gap).
        let mut hardest: Option<(f64, usize, usize, crate::granular::ContactTerms, f64)> = None;
        for i in 0..rods.len() {
            // ★ Not thrown on yet: a member waiting for its forkful is not in the world, so it neither
            // falls nor collides. Skipping it is what makes the release a SEQUENCE rather than a cloud.
            if rods[i].release_t_s > elapsed_s {
                continue;
            }
            // ★★★ **THE GATE THAT SHOULD HAVE EXISTED FROM THE START** (docs/46 row 79). Every
            // aggregate instrument in this module — `peak_speed`, `height_m`, the envelope's bbox —
            // is built from `f64::min`/`f64::max`, which **return the non-NaN operand**. A heap of NaN
            // therefore reports a peak speed of 0.000000, a settled gauge, and a tidy packing figure;
            // `NaN as i64` saturates to 0, so every member also lands in cell (0,0,0) and the envelope
            // reads exactly one cell at every resolution. I tested for NaN twice THROUGH those same
            // instruments and cleared it both times. Check the state directly, every step, or the
            // simulation will lie to you in the shape of a result.
            debug_assert!(
                rods[i].centre.is_finite() && rods[i].vel.is_finite() && rods[i].ang_vel.is_finite(),
                "member {i} went NaN before step at t={elapsed_s}: centre {:?} vel {:?} ang_vel {:?}",
                rods[i].centre,
                rods[i].vel,
                rods[i].ang_vel
            );
            let mut acc = DVec3::ZERO;
            let mut torque = DVec3::ZERO;
            let mut touching = 0usize;
            // ★★★ WHERE each neighbour pushes, and how hard (docs/46 row 79) — so the force can go
            // into the member's SHAPE, which is what a blade actually does with it.
            let mut loads: Vec<(f64, DVec3)> = Vec::new();
            let (a0, a1) = snapshot[i].ends();
            for (j, other) in snapshot.iter().enumerate() {
                if i == j || other.release_t_s > elapsed_s {
                    continue;
                }
                let (b0, b1) = other.ends();
                // Cheap reject before the closest-point solve.
                if (other.centre - snapshot[i].centre).length_squared()
                    > (length + 4.0 * radius).powi(2)
                {
                    continue;
                }
                // ★★★ AGAINST THE BENT SHAPE, NOT THE AXIS (docs/46 row 76). A straight member is a
                // one-segment polyline, so a rigid pile takes the identical path it always did.
                let (pa, pb) = closest_points_between(&snapshot[i].polyline(), &other.polyline());
                // ★★★ Both bodies' velocities AT THEIR CONTACT POINTS (docs/46 row 72) — the
                // quantity `contact_accel` has always taken and never been given. Two blades scraping
                // past each other spin-to-spin were invisible to the damping and to friction alike.
                //
                // ★ The arms are to the AXIS points. Moving them to the surfaces is docs/46 row 73 and
                // is not done here — see that row for the instability it produced.
                let arm_a = pa - snapshot[i].centre;
                let arm_b = pb - other.centre;
                let va = snapshot[i].velocity_at(arm_a);
                let vb = other.velocity_at(arm_b);
                let terms = crate::granular::contact_terms(pa, va, pb, vb, &contact);
                let a_n = terms.total();
                if trace_steps > 0 {
                    let mag = a_n.length();
                    if hardest.map_or(true, |(m, ..)| mag > m) {
                        hardest = Some((mag, i, j, terms, (pa - pb).length()));
                    }
                }
                // ★★★ **AT THE POINT THE QUANTITY IS FORMED** (docs/46 row 79). Everything downstream —
                // the floor impulse, the member's finiteness, the heap's every statistic — is a symptom.
                // This is the first place a number can be wrong, so it says what it was given.
                debug_assert!(
                    a_n.is_finite() && a_n.length() < 1.0e9,
                    "neighbour contact blew up at t={elapsed_s}: |a| {:e} m/s² · members {i}<->{j} · \
                     gap {:e} m (touch {:e}) · va {:?} vb {:?} · pa {:?} pb {:?}",
                    a_n.length(),
                    (pa - pb).length(),
                    2.0 * contact.radius,
                    va,
                    vb,
                    pa,
                    pb
                );
                acc += a_n;
                touching += 1;
                // ★ THE NEIGHBOUR'S MOMENT ARM. The contact happens at `pa`, which is somewhere along
                // this rod, not at its centre — so it both pushes and TURNS. Discarding the arm is what
                // made a landing blade slide instead of topple onto the heap.
                torque += arm_a.cross(a_n * member_mass);
                // Arclength from the base to the contact, along the member's own length.
                let s_m = (pa
                    - (snapshot[i].centre - snapshot[i].axis * snapshot[i].half_length_m))
                    .dot(snapshot[i].axis.normalize_or(DVec3::X))
                    .clamp(0.0, 2.0 * snapshot[i].half_length_m);
                loads.push((s_m, a_n * member_mass));
            }
            // ★★★ **THE SUM, NOT THE TERM** (docs/46 row 79). Each `a_n` passed its own bound at step 1
            // and the member still reached 1e4 m/s by step 2 — because a member overlapping MANY
            // neighbours accumulates all of them. Asserting the term and not the total is how a
            // thousand legal contributions add up to an illegal answer unseen.
            debug_assert!(
                acc.is_finite() && acc.length() < 1.0e9,
                "summed neighbour contact blew up at t={elapsed_s}: |acc| {:e} m/s² over {touching} \
                 simultaneous contacts on member {i}",
                acc.length()
            );
            let r = &mut rods[i];
            step_one_rod(
                r,
                member_mass,
                &contact,
                gravity_ms2,
                air_density_kgm3,
                dt,
                acc,
                torque,
                flex_ei,
                &loads,
                member_mass / (2.0 * r.half_length_m) * gravity_ms2,
            );
        }
        elapsed_s += dt;
        if step_n < trace_steps {
            let vmax = rods.iter().map(|r| r.vel.length()).fold(0.0f64, f64::max);
            let wmax = rods
                .iter()
                .map(|r| r.ang_vel.length())
                .fold(0.0f64, f64::max);
            // ★★ **TRANSLATION vs SHAPE, separated.** An overlap can appear two ways: a member MOVED
            // into its neighbour, or its SHAPE changed under it while its centre stayed put. One step
            // of gravity from rest moves a centre ~1e-12 m, so a micron-scale overlap at step 2 cannot
            // be translation — but that is elimination. This measures it: the largest displacement of
            // any polyline POINT over the step, against the largest displacement of any CENTRE.
            // ★ CHECK THE INSTRUMENT FIRST. This compares polylines point by point, which is only
            // meaningful if both have the SAME number of points. If a member's flex chain is created
            // or re-segmented during the step, the two polylines describe the same blade at different
            // discretisations and a point-by-point diff is nonsense — it would report a huge "shape
            // change" that is really an indexing mismatch. So report the counts and refuse to diff
            // across a change.
            let seg_before: Vec<usize> = snapshot.iter().map(|r| r.polyline().len()).collect();
            let seg_after: Vec<usize> = rods.iter().map(|r| r.polyline().len()).collect();
            let resegmented = seg_before != seg_after;
            let mut mover = usize::MAX;
            let max_point = if resegmented {
                f64::NAN
            } else {
                let mut worst = 0.0f64;
                for (i, (r, s0)) in rods.iter().zip(snapshot.iter()).enumerate() {
                    let d = r
                        .polyline()
                        .iter()
                        .zip(s0.polyline().iter())
                        .map(|(a, b)| (*a - *b).length())
                        .fold(0.0f64, f64::max);
                    if d > worst {
                        worst = d;
                        mover = i;
                    }
                }
                worst
            };
            if mover != usize::MAX && step_n == 0 {
                // ★ THE DIRECT OBSERVATION. Four hypotheses about this displacement died to argument
                // (re-segmentation, non-idempotent relaxation, mismatched weight-per-length, a member
                // entering on this step). Stop guessing and print the geometry either side of it.
                let (b, a) = (&snapshot[mover], &rods[mover]);
                let (pb, pa) = (b.polyline(), a.polyline());
                println!(
                    "         GEOM before: centre {:?}\n                      axis {:?} normal {:?}\n                      poly[0] {:?} poly[last] {:?} n={}",
                    b.centre, b.axis, b.normal, pb.first(), pb.last(), pb.len()
                );
                println!(
                    "         GEOM after : centre {:?}\n                      axis {:?} normal {:?}\n                      poly[0] {:?} poly[last] {:?} n={}",
                    a.centre, a.axis, a.normal, pa.first(), pa.last(), pa.len()
                );
                // Is it an ORDER flip? Compare forward against reversed.
                let fwd = pa
                    .iter()
                    .zip(pb.iter())
                    .map(|(x, y)| (*x - *y).length())
                    .fold(0.0f64, f64::max);
                let rev = pa
                    .iter()
                    .rev()
                    .zip(pb.iter())
                    .map(|(x, y)| (*x - *y).length())
                    .fold(0.0f64, f64::max);
                println!(
                    "         forward-matched max {fwd:.4e} m · REVERSE-matched max {rev:.4e} m"
                );
            }
            if mover != usize::MAX {
                // ★ WHO moved, and was this the step it entered the world? A member waiting for its
                // forkful is skipped entirely (`release_t_s > elapsed_s`), so the step it becomes
                // active is a candidate explanation that has nothing to do with bending.
                let r = &rods[mover];
                println!(
                    "         MOVER: member {mover} · release_t_s {:.6e} · elapsed {:.6e} · ACTIVE THIS STEP: {} · was active BEFORE: {}",
                    r.release_t_s,
                    elapsed_s,
                    r.release_t_s <= elapsed_s,
                    r.release_t_s <= elapsed_s - dt
                );
            }
            if resegmented {
                let b: Vec<usize> = seg_before.iter().take(4).copied().collect();
                let a: Vec<usize> = seg_after.iter().take(4).copied().collect();
                println!(
                    "         ★ RE-SEGMENTED THIS STEP: polyline point counts {b:?}.. -> {a:?}.. — a \
point-by-point shape diff is meaningless across this, so it is not reported."
                );
            }
            let max_centre = rods
                .iter()
                .zip(snapshot.iter())
                .map(|(r, s0)| (r.centre - s0.centre).length())
                .fold(0.0f64, f64::max);
            println!(
                "         MOVED this step: max polyline point {max_point:.4e} m · max centre {max_centre:.4e} m · shape/translation {:.3e}x",
                if max_centre > 0.0 { max_point / max_centre } else { f64::INFINITY }
            );
            match hardest {
                None => println!(
                    "step {:>3}: NO CONTACT AT ALL · |v|max {vmax:.6e} · |w|max {wmax:.6e}",
                    step_n + 1
                ),
                Some((mag, i, j, t, gap)) => {
                    let share = t.damping_share().unwrap_or(f64::NAN);
                    let touch = 2.0 * contact.radius;
                    let dir = if t.v_n < 0.0 {
                        "approaching"
                    } else {
                        "separating"
                    };
                    // What the queue's 2.2 mm inference would read off this contact, and what the
                    // overlap actually is. The ratio is the whole question.
                    let implied = t.f_rep / contact.stiffness;
                    let ratio = if t.overlap > 0.0 {
                        implied / t.overlap
                    } else {
                        f64::NAN
                    };
                    let step = step_n + 1;
                    println!(
                        "step {step:>3}: |v|max {vmax:.6e} · |w|max {wmax:.6e} · hardest contact {i}<->{j} |a| {mag:.4e} m/s2"
                    );
                    println!(
                        "         gap {gap:.6e} m (touch {touch:.6e}) · overlap {ov:.6e} m · v_n {vn:.4e} m/s ({dir})",
                        ov = t.overlap,
                        vn = t.v_n
                    );
                    println!(
                        "         spring k*overlap {sp:.4e} · damp -c*v_n {dp:.4e} (c_damp {cd:.4e}) · DAMPING SHARE {share:.1}%",
                        sp = t.spring,
                        dp = t.damp,
                        cd = t.c_damp,
                        share = 100.0 * share
                    );
                    println!(
                        "         f_rep {fr:.4e} · f_coh {fc:.4e} · implied overlap IF SPRING-ONLY {implied:.4e} m vs actual {ov:.4e} m (ratio {ratio:.2}x)",
                        fr = t.f_rep,
                        fc = t.f_coh,
                        ov = t.overlap
                    );
                }
            }
        }
        step_n += 1;
        // PEAK, not mean — the gauge's contract, and the right one: an average hides the single
        // member still bouncing, and one member crossing cells is a heap that has not settled.
        // ★★★ **A SPINNING BLADE IS NOT A SETTLED BLADE** (docs/46 row 60 step B). `SettleGauge` was
        // built when rods could only translate, so it asks one question: is anything MOVING? Give the
        // members a rotational degree of freedom and a heap can tumble in place while every centre of
        // mass sits still, and the gauge would call that quiet.
        //
        // The criterion needs no new dial, because the gauge already owns one: a rod turning at ω has
        // a TIP moving at `ω·L/2`, and a tip below the quiescent speed is as motionless as a centre
        // below it. So the speed the gauge is shown is whichever of the two is larger.
        // ★ `peak_speed` keeps its ORIGINAL meaning — the fastest CENTRE OF MASS — because it is a
        // reported field and silently changing what a number means is how old figures go on looking
        // comparable when they are not. Measured the wrong way first: folding tip speed into this
        // field made a 20-blade heap report "37.43 m/s", which is impossible for a body the air caps
        // at ~2 m/s, and the impossible number was the only reason the redefinition was noticed.
        // The observer sees the members themselves, on its own clock.
        if watch_every_s > 0.0 && elapsed_s >= next_watch {
            on_frame(elapsed_s, &rods);
            next_watch += watch_every_s;
        }
        // ★ EVERY STEP, not every trace sample: an injection that spikes and dissipates between
        // samples is exactly the shape of the thing being hunted (row 79's arrived in ONE step).
        {
            let e = heap_energy_j(&rods, member_mass, gravity_ms2);
            // ★★★ **NaN-AWARE ON PURPOSE.** `e > peak` is FALSE when `e` is NaN, so a plain comparison
            // silently ignores exactly the sample that matters — the identical failure to `f64::max`
            // returning the non-NaN operand (row 79). An unbounded energy is not "no new peak"; it is
            // the largest possible answer, and it is recorded as such.
            if !e.is_finite() {
                if first_non_finite_energy_t_s.is_none() {
                    first_non_finite_energy_t_s = Some(elapsed_s);
                }
                peak_energy_j = f64::INFINITY;
                peak_energy_t_s = first_non_finite_energy_t_s.unwrap_or(elapsed_s);
            } else if e > peak_energy_j {
                peak_energy_j = e;
                peak_energy_t_s = elapsed_s;
                peak_rotational_energy_j = heap_rotational_energy_j(&rods, member_mass);
            }
        }
        peak_speed = rods.iter().map(|r| r.vel.length()).fold(0.0f64, f64::max);
        // What the GAUGE is asked is a different question: is anything moving AT ALL? A rod turning
        // at ω has a tip moving at `ω·L/2`, and a heap tumbling in place with every centre still is
        // not settled. That comparison needs no new dial — it reuses the gauge's own quiescent speed.
        let peak_point_speed = rods
            .iter()
            .map(|r| r.max_surface_speed_ms())
            .fold(0.0f64, f64::max);
        if peak_point_speed >= gauge.moving_above(gravity_ms2 as f32) as f64 {
            disturbed = true;
        }
        if disturbed {
            gauge.observe(peak_point_speed as f32, gravity_ms2 as f32, dt as f32);
        }
        // ★★★ **QUIET IS NOT THE SAME AS SUPPORTED, AND DAMPING CAN FAKE THE FIRST.**
        //
        // MEASURED 2026-08-16: with the restitution calibration corrected (docs/46 row 62) the
        // contact damping roughly doubled, and a 400-rod heap reported "settled" at 0.38 s having
        // fallen 0.054 m — it froze in mid-air. The release drops rod CENTRES ~19x closer together
        // than a rod is long, so every member starts interpenetrated with many others; under-damped
        // they blew apart, correctly damped they lock. Both look quiet to a speed threshold.
        //
        // But a damping force is proportional to velocity, so it VANISHES at rest: a heap held up by
        // damping cannot be in equilibrium, and will resume falling the moment it truly stops. The
        // test that separates the two needs no dial — require the quiet to have held for as long as
        // an unsupported member would take to fall its OWN LENGTH, `√(2L/g)`. Nothing moving for that
        // long is being held by something that does not care whether it is moving: the floor, or
        // another member resting on the floor.
        //
        // These are two different questions asked at two different scales, deliberately: *is anything
        // moving?* is a contact-radius question, and *has that lasted long enough to prove support?*
        // is a body-length one. Collapsing them into one gauge is what let a frozen cloud pass.
        let support_s = (2.0 * length / gravity_ms2).sqrt();
        // ★ A heap that is still being BUILT is not a settled heap, however quiet the part already
        // thrown on happens to be. Without this the gauge could declare victory between forkfuls.
        let all_thrown = rods.iter().all(|r| r.release_t_s <= elapsed_s);
        let supported = all_thrown && gauge.quiet_seconds() as f64 >= support_s;
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
            // ★ MEANING CHANGED 2026-09-20 and it is changed LOUDLY, because silently redefining a
            // reported number is how old figures go on looking comparable when they are not. This was
            // `½mv² + mgy`; it is now the COMPLETE mechanical energy, spin included. The old value was
            // not a different convention, it was an incomplete one — blind to rotation, which is the
            // channel row 79's explosion used.
            let energy: f64 = heap_energy_j(&rods, member_mass, gravity_ms2);
            trace.push(Sample {
                t_s: elapsed_s,
                peak_speed_ms: peak_speed,
                peak_ang_speed_rads: rods
                    .iter()
                    .map(|r| r.ang_vel.length())
                    .fold(0.0f64, f64::max),
                peak_surface_speed_ms: rods
                    .iter()
                    .map(|r| r.max_surface_speed_ms())
                    .fold(0.0f64, f64::max),
                contacting_fraction: {
                    // ★ Against the real shapes, for the same reason the envelope is (row 78).
                    let touch = 2.0 * contact.radius + contact.coh_range;
                    let mut n_touch = 0usize;
                    let polys: Vec<Vec<DVec3>> = rods.iter().map(|r| r.polyline()).collect();
                    for (i, _r) in rods.iter().enumerate() {
                        let floor = polys[i].iter().map(|p| p.y).fold(f64::INFINITY, f64::min)
                            <= contact.radius;
                        let neigh = (0..rods.len()).any(|j| {
                            if i == j {
                                return false;
                            }
                            let (pa, pb) = closest_points_between(&polys[i], &polys[j]);
                            (pa - pb).length() < touch
                        });
                        if floor || neigh {
                            n_touch += 1;
                        }
                    }
                    n_touch as f64 / rods.len().max(1) as f64
                },
                mean_speed_ms: mean,
                height_m: top,
                lowest_end_m: rods
                    .iter()
                    .map(|r| {
                        let (a, b) = r.ends();
                        a.y.min(b.y)
                    })
                    .fold(f64::INFINITY, f64::min),
                quiet_s: gauge.quiet_seconds() as f64,
                energy_j: energy,
            });
        }
        if disturbed && gauge.settled(gravity_ms2 as f32) && supported {
            break;
        }
    }
    // Never disturbed at all is not a settled heap either — it is a heap that never fell, which for a
    // release above a floor means something is wrong with the release, not that the answer is ready.
    let quiet = disturbed
        && gauge.settled(gravity_ms2 as f32)
        && gauge.quiet_seconds() as f64 >= (2.0 * length / gravity_ms2).sqrt();

    // ★ MEASURE THE HEAP. Occupancy on a grid whose cell is the member's own radius scale, so the
    // envelope means "space the heap is in" rather than "box that contains it".
    // The cell must resolve the HEAP, not the blade: too coarse and the envelope is mostly the
    // measurement's own air. An eighth of a member's length, floored at its thickness. (`cell` is
    // computed above, because the settle gauge is asked at the same scale the answer is expressed at.)
    let mut cells = std::collections::BTreeSet::new();
    let mut height: f64 = 0.0;
    for r in &rods {
        // ★★★ WALK THE MEMBER'S REAL SHAPE (docs/46 row 78). This walked `r.ends()` — the straight AXIS
        // — while contact has been resolved against the bent polyline since row 76. A drooping blade
        // leaves its own axis by nearly a full length, so the envelope was sampling empty space and
        // missing the matter, and the packing it reported was a measurement of a rod that is not there.
        let poly = r.polyline();
        for w in poly.windows(2) {
            let (e0, e1) = (w[0], w[1]);
            height = height.max(e0.y.max(e1.y));
            // Walk each segment at cell resolution so a long member occupies every cell it crosses.
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
    }
    let envelope = cells.len() as f64 * cell * cell * cell;
    let matter = member.matter_volume_m3() * count as f64;
    // Direct, not folded: ask every member whether it is a number.
    let all_finite = rods.iter().all(|r| {
        r.centre.is_finite()
            && r.vel.is_finite()
            && r.ang_vel.is_finite()
            && r.axis.is_finite()
            && r.polyline().iter().all(|p| p.is_finite())
    });
    let settled = Settled {
        members: count,
        all_finite,
        energy_j_at_release,
        // ★★★ **NO FALLBACK.** The first version of this line read
        // `if peak_energy_j.is_finite() { peak_energy_j } else { energy_j_at_release }`, which turned
        // "the heap reached INFINITE energy" into "the heap gained exactly 0.0%" — and the gate built
        // to catch that class of lie duly reported PASS on a heap whose own `all_finite` was false.
        // **A fallback that replaces a non-finite measurement with a plausible one is a lie
        // generator.** Propagate the value; let the reader see `inf`.
        peak_energy_j,
        first_non_finite_energy_t_s,
        peak_energy_t_s,
        peak_rotational_energy_j,
        cells_vs_cell: {
            let mut v = Vec::new();
            let mut c = length;
            while c > radius {
                v.push((c, (envelope_m3_at(&rods, c) / (c * c * c)).round() as usize));
                c *= 0.5;
            }
            v
        },
        matter_m3: matter,
        envelope_m3: envelope,
        packing: if envelope > 0.0 {
            (matter / envelope).clamp(0.0, 1.0)
        } else {
            0.0
        },
        height_m: height,
        cell_m: cell,
        packing_vs_cell: {
            let mut v = Vec::new();
            let mut c = length; // start at a whole member and refine toward its diameter
            while c > radius {
                let e = envelope_m3_at(&rods, c);
                v.push((
                    c,
                    if e > 0.0 {
                        (matter / e).clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                ));
                c *= 0.5;
            }
            v
        },
        quiet,
        elapsed_s,
        peak_speed_ms: peak_speed,
    };
    Some((settled, trace))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ **THE QUANTITY THAT MUST FALL IS ENERGY, NOT ANGULAR SPEED** (docs/46 row 73, corrected).
    ///
    /// A body with anisotropic inertia can spin FASTER while losing energy, because `E = ½Iω²` and the
    /// axial moment of a blade is ~10⁴ below the others: the same joules parked in the soft mode buy
    /// ~100× the angular speed. Measured — a blade whose `|ω|` ran 20 → 290 rad/s had **27% of its
    /// starting energy left**, and the sweep showed it converging as `dt` fell 128×, so it was never an
    /// instability at all. Asserting on `|ω|` called correct physics a blow-up and sent me to revert it.
    pub(super) use super::mechanical_energy_j;

    /// Air at the bottom of Earth's atmosphere, from the catalogue's own `air` and the standard
    /// sea-level state — NOT a typed 1.225. The Earth assembly is what supplies air to anything
    /// standing on it, and a haystack is standing on it.
    /// Visible to the module's other test modules so nobody writes a second one — the whole point of
    /// `docs/46` row 84 is that a question answered twice eventually gets two answers.
    pub(super) fn sea_level_air(mats: &[Material]) -> f64 {
        let air = mats
            .iter()
            .find(|m| m.id == "air")
            .expect("air is catalogued");
        crate::atmosphere::air_density_at(101_325.0, air, 288.15, 9.81, 0.0)
    }

    /// ★★★ **AND IT MUST SPIN DOWN ABOUT ITS OWN LONG AXIS TOO** (docs/46 row 73).
    ///
    /// Row 72 taught contacts to see rotation and the heap became a pile — translation dead at
    /// 0.000063 m/s, every member touching. It still would not settle: peak `|ω|` sat at **3.40 rad/s**,
    /// a tip speed of 0.595 m/s against a quiescent 0.103 m/s.
    ///
    /// ★★ **Because a capsule contact cannot see AXIAL spin.** `closest_points` returns points on each
    /// body's SEGMENT — points *on the axis* — so the moment arm `r` is parallel to the long axis, and
    /// for `ω` about that same axis `ω × r = 0` **identically**. The contact point does not move,
    /// friction has no sliding to oppose, and the damping has no relative velocity to remove. It is the
    /// worst case by construction: the axial moment `m(W²+T²)/12` is ~10⁷ below the other two, so axial
    /// spin is both the easiest to excite and the only one nothing could remove.
    ///
    /// The sibling test above spins about the VERTICAL while the blade lies along x — `ω ⊥ axis`, which
    /// has a real moment arm and always worked. This one spins about the blade's own length, and it is
    /// the case that never did.
    ///
    /// The fix is geometry, not a new force: matter touches at its SURFACE, so the contact point is
    /// `p_axis − n·r`. Then the arm has a component perpendicular to the axis, an axial spin drags the
    /// surface across the floor, and the friction that already exists does the work.
    #[test]
    fn a_blade_spinning_about_its_own_length_spins_down() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let rho = sea_level_air(&mats);

        // Lying flat, still, spinning about its OWN LENGTH — ω ∥ axis.
        let spin0 = 20.0;
        let mut rod = Rod {
            centre: DVec3::new(0.0, radius, 0.0),
            axis: DVec3::X,
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Y,
            vel: DVec3::ZERO,
            ang_vel: DVec3::new(spin0, 0.0, 0.0),
            release_t_s: 0.0,
            flex: Flex::straight(),
        };
        let i_axial = rod.principal_inertia_kgm2(mass).x;
        println!(
            "axial spin {spin0} rad/s · I_axial {i_axial:.4e} kg·m² · surface drags at ω·r = {:.5} m/s",
            spin0 * radius
        );

        let dt =
            crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0).min(1.0e-4);
        let mut t = 0.0;
        for _ in 0..2_000_000 {
            step_one_rod(
                &mut rod,
                mass,
                &contact,
                9.81,
                rho,
                dt,
                DVec3::ZERO,
                DVec3::ZERO,
                0.0,
                &[],
                0.0,
            );
            t += dt;
            if rod.ang_vel.length() < 0.5 * spin0 {
                break;
            }
        }
        let spin1 = rod.ang_vel.length();
        println!("  ended |ω| {spin1:.4} rad/s after {t:.5} s");
        assert!(
            spin1 < 0.9 * spin0,
            "a blade spinning about its own length must spin down: {spin0} -> {spin1:.4} rad/s in \
             {t:.5} s. With the contact resolved on the AXIS the arm is parallel to ω, so ω × r = 0 \
             and the surface never moves."
        );
    }

    /// ★★★ **A BLADE LYING ON THE FLOOR, SPINNING, MUST SPIN DOWN** (docs/46 row 72).
    ///
    /// Row 60 step B gave rods angular velocity and nothing that takes it away. The re-measured heap
    /// showed the consequence in the plainest possible terms: centres of mass nearly still at 0.147
    /// m/s while the tips moved at 37.4 m/s — the blades had stopped travelling and gone on spinning,
    /// forever, because **no contact in the engine could see rotation.**
    ///
    /// `granular::contact_accel(pi, vi, pj, vj, …)` takes the velocities of the two CONTACT POINTS.
    /// Every caller was handing it the body's centre-of-mass velocity for a point that is not the
    /// centre — so a rod turning in place presented `v_rel = 0` and the contact's `normal_damp`, which
    /// carries the material's own measured restitution, never saw the motion at all. Friction likewise:
    /// a spinning blade's foot is SLIDING across the floor, and sliding is what friction acts on.
    ///
    /// The physical quantity is `v + ω × r`. Nothing here changes the contact law; it changes what the
    /// callers tell it about, which was wrong.
    #[test]
    fn a_blade_spinning_on_the_floor_spins_down() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let rho = sea_level_air(&mats);

        // Lying flat on the floor, not moving, spinning about the vertical: its foot is sliding.
        let spin0 = 20.0;
        let mut rod = Rod {
            centre: DVec3::new(0.0, radius, 0.0),
            axis: DVec3::X,
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Y,
            vel: DVec3::ZERO,
            ang_vel: DVec3::new(0.0, spin0, 0.0),
            release_t_s: 0.0,
            flex: Flex::straight(),
        };
        println!(
            "resting blade spun at {spin0} rad/s about the vertical · foot sliding at {:.3} m/s · friction {:.3}",
            spin0 * rod.half_length_m,
            contact.friction
        );

        let dt =
            crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0).min(1.0e-4);
        let e0 = mechanical_energy_j(&rod, mass, 9.81);
        let mut t = 0.0;
        for step in 0..2_000_000 {
            step_one_rod(
                &mut rod,
                mass,
                &contact,
                9.81,
                rho,
                dt,
                DVec3::ZERO,
                DVec3::ZERO,
                0.0,
                &[],
                0.0,
            );
            t += dt;
            if step % 400_000 == 0 {
                println!("  t {t:.4} s · |ω| {:.4} rad/s", rod.ang_vel.length());
            }
            if rod.ang_vel.length() < 0.5 * spin0 {
                break;
            }
        }
        let spin1 = rod.ang_vel.length();
        let e1 = mechanical_energy_j(&rod, mass, 9.81);
        println!(
            "  ended |ω| {spin1:.4} rad/s after {t:.4} s · energy x{:.3} of start",
            e1 / e0
        );
        assert!(
            e1 < 0.9 * e0,
            "a blade spinning on the floor must LOSE ENERGY: x{:.3} of its start in {t:.3} s. \
             (Angular speed is the wrong test — an anisotropic body can spin faster while losing \
             energy, since the axial moment is ~10^4 below the others.)",
            e1 / e0
        );
    }

    /// ★★★ **A BLADE STOOD ON ITS END FALLS OVER AT THE RATE A PIVOTING ROD DOES** (docs/46 row 60
    /// step B, and its correction by row 72).
    ///
    /// The plainest thing a rod could not do. `Rod` carried a position and a velocity and no angular
    /// state, so its `axis` was fixed for life: a blade balanced upright stayed upright forever.
    ///
    /// Gravity exerts no torque about a uniform body's own centre of mass, so the toppling comes from
    /// where it must: **the floor pushes on the rod's lower END, and that force has a moment arm.**
    ///
    /// ★★ **AND THE RATE IS NOW CHECKED AGAINST THEORY, WHICH CAUGHT A BUG THIS TEST HAD BLESSED.**
    /// Its first version asserted only that the blade tipped "more than twice its starting angle", and
    /// passed while the contact was being applied TWICE — the constraint set the centre's velocity
    /// outright AND an angular impulse was added from the full mass, so the blade toppled **1.75× too
    /// fast** and a loose threshold called it right. A rod pivoting on its end obeys
    /// `θ̈ = (3g/2L)·sin θ`, so for small angles `θ(t) = θ₀·cosh(ωt)` with `ω = √(3g/2L)` — exact,
    /// independent of this integrator, and unforgiving about a factor of 1.75.
    #[test]
    fn a_blade_stood_on_its_end_falls_over() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let (width, thickness) = cross_section_for(&blade).expect("a blade has a cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let rho = sea_level_air(&mats);
        let g = 9.81;

        let tilt: f64 = 3.0_f64.to_radians();
        let mut rod = Rod {
            centre: DVec3::new(0.0, 0.5 * length * tilt.cos() + radius, 0.0),
            axis: DVec3::new(tilt.sin(), tilt.cos(), 0.0).normalize(),
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Z,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
            vel: DVec3::ZERO,
        };

        // The analytic pivot: θ̈ = (3g/2L)·sin θ, so θ = θ₀·cosh(ωt) while θ stays small.
        let w = (3.0 * g / (2.0 * length)).sqrt();
        let from_vertical = |r: &Rod| r.axis.normalize_or(DVec3::Y).dot(DVec3::Y).abs().acos();
        let start = from_vertical(&rod);
        println!(
            "released {:.2}° from vertical · pivot rate ω = √(3g/2L) = {w:.4} 1/s",
            start.to_degrees()
        );

        let dt =
            crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0).min(1.0e-4);
        let target = 2.0f64; // measure the time to double the tilt — still inside the small-angle law
        let want_t = target.acosh() / w;
        let mut t = 0.0;
        let mut angle = start;
        while t < 4.0 * want_t {
            step_one_rod(
                &mut rod,
                mass,
                &contact,
                g,
                rho,
                dt,
                DVec3::ZERO,
                DVec3::ZERO,
                0.0,
                &[],
                0.0,
            );
            t += dt;
            angle = from_vertical(&rod);
            if angle >= target * start {
                break;
            }
        }
        println!(
            "  doubled the tilt at t = {t:.4} s · theory {want_t:.4} s · {:+.2}%",
            100.0 * (t - want_t) / want_t
        );
        assert!(
            angle >= target * start,
            "a blade stood on its end must fall over: reached only {:.3}° in {t:.3} s",
            angle.to_degrees()
        );
        assert!(
            (t - want_t).abs() / want_t < 0.15,
            "and it must topple at the rate a pivoting rod does: {t:.4} s against theory {want_t:.4} s \
             ({:+.1}%). The contact was once applied twice — once as a velocity constraint on the \
             centre and again as an angular impulse from the full mass — which toppled it 1.75x too \
             fast and passed a looser assertion than this one.",
            100.0 * (t - want_t) / want_t
        );
    }

    /// ★★★ **A FALLING ROD OBEYS THE SAME AIR A METEOR DOES** (docs/46 row 60 step A3, Laws II + V).
    ///
    /// ★★ **This test was WRONG on its first writing, and Robin caught the premise.** It asserted that
    /// a rod in free flight conserves energy exactly, on the grounds that *"there is no air in this
    /// world"*. There is: *"Are we forgetting air? It would be constant until it touches down IN A
    /// VACUUM… The Earth assembly supplies an atmosphere."* A haystack stands on Earth, so its blades
    /// fall through Earth's air, and an energy-conserving blade would have been the unphysical one.
    ///
    /// So the defect in `vel *= 1.0 - (2.0 * dt).min(0.5)` was never that it dissipated. It was that it
    /// dissipated **by a rate constant chosen for behaviour** — its own comment said *"2/s is gentle
    /// enough to let them fall and firm enough to stop the explicit integrator ringing"* — while
    /// `atmosphere::drag_accel` sat in the tree implementing the real quadratic law, already used by
    /// the meteor path. One question, *how much does air slow this down*, with two answers, and the
    /// blade got the invented one. Deleting the fudge would have left a hole where physics belongs;
    /// the fix is to wire the law that was already there.
    ///
    /// **The reference is closed-form.** A body released from rest into quadratic drag has
    /// `v(t) = v_t · tanh(g·t / v_t)` with `v_t = √(2mg / ρ·C_d·A)` — exact, independent of this
    /// integrator, and the only kind of check that can catch a plausible-looking wrong answer. The rod
    /// does not rotate yet (row 60 step B), so `A` is constant through the fall and the form holds.
    #[test]
    fn a_falling_rod_obeys_the_same_air_a_meteor_does() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let mass = blade.mass_kg(&mats).expect("a blade has a mass");
        let rho = sea_level_air(&mats);
        let g = 9.81;

        let (_settled, samples) =
            settle_traced(&blade, &mats, 1, g, rho, 20260826, 1.0e-3).expect("a single rod falls");

        // ★★ FREE FLIGHT IS BEFORE ANYTHING TOUCHES, and this criterion has now been wrong TWICE.
        // First it was `height_m > 2·radius` — but `height_m` is the heap's TOP, so a landed blade
        // still read 0.0792 m and the whole post-landing rest counted as flight. Then it was "while
        // still descending" — which was valid only while rods could not turn, because a toppling
        // blade lowers the top of the heap exactly as falling does. `lowest_end_m` reports the thing
        // that was actually meant, so the third version does not have to be clever.
        // ω is EXACTLY zero until the first contact, so this needs no threshold and cannot be fooled
        // by a rebound. The two earlier versions of this criterion both could: `height_m > 2·radius`
        // counted the whole post-landing rest as flight, and "while still descending" broke the moment
        // rods could topple. The third try measures the thing that actually distinguishes the states.
        let flying: Vec<&Sample> = samples
            .iter()
            .take_while(|s| s.peak_ang_speed_rads == 0.0)
            .collect();
        assert!(
            flying.len() >= 8,
            "need a real flight window, got {}",
            flying.len()
        );

        // ★ ASK THE RELEASE what this blade was actually given, rather than rebuilding the seeded
        // draw here — a second implementation of the release would drift the moment either changed.
        // The rod does not rotate yet (row 60 step B), so its presented area is fixed through the fall.
        let released = release_rods(&blade, 1, 20260826).expect("one rod");
        let r0 = &released[0];
        let along = r0.axis.normalize_or(DVec3::X);
        let n = (r0.normal - along * along.dot(r0.normal)).normalize_or(DVec3::Y);
        let w = along.cross(n);
        let area = crate::atmosphere::box_frontal_area_m2(
            DVec3::new(2.0 * r0.half_length_m, r0.width_m, r0.thickness_m),
            [along, w, n],
            DVec3::NEG_Y,
        );
        let v_t =
            (2.0 * mass * g / (rho * crate::atmosphere::FLAT_PLATE_NORMAL_DRAG_CD * area)).sqrt();
        // What the retired equal-volume capsule would have shown, for the record.
        let cap = crate::atmosphere::capsule_frontal_area_m2(length, radius, along, DVec3::NEG_Y);
        println!(
            "  the retired capsule would have presented {cap:.4e} m² ({:.2}x)",
            cap / area
        );
        println!(
            "dry blade: {length:.3} m x r {radius:.4e} m · mass {mass:.4e} kg · air {rho:.4} kg/m³",
        );
        println!(
            "  presents {area:.4e} m² to the fall · terminal speed {v_t:.3} m/s \
             (the retired fudge's was g/2 = {:.3} m/s)",
            g / 2.0
        );

        let t0 = flying[0].t_s;
        let mut worst = 0.0f64;
        let mut worst_at = flying[0];
        for s in flying.iter().skip(1) {
            let want = v_t * ((g * (s.t_s - t0)) / v_t).tanh();
            let err = (s.peak_speed_ms - want).abs() / want.max(1.0e-9);
            if err > worst {
                worst = err;
                worst_at = s;
            }
        }
        println!(
            "  WORST at t {:.4} s · v {:.4} m/s · want {:.4} · lowest end {:.5} m · height {:.4} m · n={}",
            worst_at.t_s,
            worst_at.peak_speed_ms,
            v_t * ((g * (worst_at.t_s - t0)) / v_t).tanh(),
            worst_at.lowest_end_m,
            worst_at.height_m,
            flying.len()
        );
        for s in flying.iter().skip(1).step_by((flying.len() / 5).max(1)) {
            let want = v_t * ((g * (s.t_s - t0)) / v_t).tanh();
            println!(
                "  t {:.4} s · v {:.4} m/s · closed form {want:.4} m/s · {:+.2}%",
                s.t_s,
                s.peak_speed_ms,
                100.0 * (s.peak_speed_ms - want) / want.max(1e-9)
            );
        }
        println!(
            "  worst disagreement with the closed form: {:.3}%",
            100.0 * worst
        );

        assert!(
            worst < 0.02,
            "a falling rod must follow v(t) = v_t·tanh(g·t/v_t) — worst {:.3}% off. \
             A rate constant cannot: it gives 1 − e^(−kt), a different function with a different \
             terminal speed.",
            100.0 * worst
        );
    }

    /// ★★★ **NO MEMBER MAY BE RELEASED INSIDE ANOTHER** (docs/46 row 60 step C).
    ///
    /// The release scatters members over a disc of radius `0.1·length` — **3.5 cm for a blade 35 cm
    /// long**. Every member therefore starts deep inside several others, and a stiff contact resolving
    /// a large initial overlap delivers an enormous impulse, off-centre, at `t = 0`. That is the
    /// scatter the heap has been measuring all along: not how straw packs, but how hard the engine
    /// threw it apart on the first step.
    ///
    /// ★ **Both previous release geometries were wrong, in opposite directions**, and the code says so
    /// in its own comment: a disc wider than a blade is long gave *"a measurement of how thinly they
    /// were scattered rather than of how they pack"*, so it was replaced by a point source — which
    /// packs them at infinite density instead. Neither is a haystack.
    ///
    /// Robin's own description is the way out: *"in a hay stack it's all gravity"*, thrown on **forkful
    /// by forkful**. A forkful is however many members fit without overlapping; the next goes on once
    /// that one has landed. Nothing is invented — it is what building a haystack IS.
    ///
    /// This test asserts only the invariant, because the invariant is the physics: overlapping matter
    /// at `t = 0` is not an initial condition, it is a violation being paid off as an explosion.
    #[test]
    fn no_member_is_released_inside_another() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let _ = &mats;
        let (length, radius) = rod_for(&blade).expect("rod");
        let touch = 2.0 * radius;

        let mut worst_any = 0.0f64;
        let mut total_overlapping = 0usize;
        for count in [10usize, 20, 40, 100, 200, 400] {
            let rods = release_rods(&blade, count, 20260810).expect("a release");
            let mut worst_overlap = 0.0f64;
            let mut pairs_overlapping = 0usize;
            // ★ Compare only members that are in the world AT THE SAME MOMENT. Forkfuls reuse the
            // same release volume and are separated in TIME, so two blades from different throws may
            // occupy the same space at t=0 and never meet — the first has landed before the second
            // exists. The first version of this check compared every pair regardless and reported 209
            // "interpenetrations" that were nothing of the kind.
            for i in 0..rods.len() {
                let (a0, a1) = rods[i].ends();
                for j in (i + 1)..rods.len() {
                    if (rods[i].release_t_s - rods[j].release_t_s).abs() > 1e-12 {
                        continue;
                    }
                    let (b0, b1) = rods[j].ends();
                    let (pa, pb) = closest_points(a0, a1, b0, b1);
                    let gap = (pa - pb).length();
                    if gap < touch {
                        pairs_overlapping += 1;
                        worst_overlap = worst_overlap.max(touch - gap);
                    }
                }
            }
            let pairs = {
                let mut n = 0usize;
                for i in 0..rods.len() {
                    for j in (i + 1)..rods.len() {
                        if (rods[i].release_t_s - rods[j].release_t_s).abs() <= 1e-12 {
                            n += 1;
                        }
                    }
                }
                n
            };
            let mut times: Vec<f64> = rods.iter().map(|r| r.release_t_s).collect();
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            times.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            // ★ A dropped blade has no preferred direction, so `|axis·ŷ|` must average 0.5 — the
            // mean of |cos θ| for a uniform direction. Rejection sampling can quietly break that:
            // in a narrow disc only near-VERTICAL blades fit, so accepting whatever fits would tilt
            // the whole population upright and call it chance.
            let mean_updown = rods.iter().map(|r| r.axis.y.abs()).sum::<f64>() / rods.len() as f64;
            println!(
                "      {} forkful(s), last thrown at {:.3} s · mean |axis·ŷ| {:.4} (uniform = 0.5000)",
                times.len(),
                times.last().copied().unwrap_or(0.0),
                mean_updown
            );
            println!(
                "  {count:3} blades: {pairs_overlapping}/{pairs} pairs interpenetrating at t=0 · \
                 worst overlap {:.4} mm ({:.1}% of a diameter)",
                worst_overlap * 1000.0,
                100.0 * worst_overlap / touch
            );
            worst_any = worst_any.max(worst_overlap);
            total_overlapping += pairs_overlapping;
        }
        // What an overlap COSTS, so "one pair" is not mistaken for "harmless": the contact is a
        // per-mass spring at ~6.8e9, so an overlap of `d` launches the pair at `k·d·dt` on the first
        // step alone.
        let m = crate::materials::load();
        let m = m.iter().find(|m| m.id == "straw").expect("straw");
        let mass = blade.mass_kg(&crate::materials::load()).expect("mass");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let dt = crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0);
        println!(
            "  worst overlap anywhere {:.4} mm -> first-step kick {:.3} m/s (terminal fall is ~2 m/s)",
            worst_any * 1000.0,
            contact.stiffness * worst_any * dt
        );
        assert_eq!(
            total_overlapping, 0,
            "no member may be released inside another: {total_overlapping} overlapping pairs across \
             the counts above, worst by {:.4} mm against a {:.4} mm diameter, which is a {:.2} m/s \
             kick on the first step. That is an explosion, not an initial condition.",
            worst_any * 1000.0,
            touch * 1000.0,
            contact.stiffness * worst_any * dt
        );
        let _ = length;
    }

    /// ★★★ **A TRANSVERSE FORCE TWISTS A RIBBON, IT DOES NOT SPIN IT ABOUT ITS OWN LENGTH**
    /// (docs/46 row 79 — the modelling decision, stated).
    ///
    /// A contact's moment about a member's centre splits into two physically different things:
    ///
    /// - **Perpendicular to the member's axis** — this TUMBLES it end over end. A blade really does
    ///   tumble, its moment of inertia there is `m(L²+T²)/12 = 4.5e-6 kg·m²`, and nothing about that is
    ///   pathological. **This component stays a rigid-body rotation.**
    /// - **About the member's own axis** — this is TORSION. A ribbon 3 mm wide and 0.3 mm thick has an
    ///   axial moment of `3.4e-10 kg·m²`, four orders below the others, and treating that as a free
    ///   rigid-body degree of freedom is what exploded: `I⁻¹ ≈ 2.9e9`, so an ordinary contact produced
    ///   `|Δω| = 6.6e5 rad/s` in one step. A real blade does not do this. It twists and bends, and the
    ///   moment is carried **elastically**.
    ///
    /// ★★ **This is a declared specialisation, not a clamp** (Law V). It is not "limit the torque to
    /// something that behaves"; it is "the axial mode of a slender ribbon is elastic, not inertial", and
    /// it is falsifiable: the perpendicular moment must be preserved EXACTLY, so a member still tumbles
    /// at the rate its inertia dictates. The test asserts both halves, because a change that killed the
    /// tumble too would also stop the explosion and be wrong.
    #[test]
    fn a_moment_tumbles_a_member_but_does_not_spin_it_about_its_length() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let rho = sea_level_air(&mats);
        let dt =
            crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0).min(1.0e-4);

        let fresh = || Rod {
            centre: DVec3::new(0.0, 5.0, 0.0), // high up: no floor contact, isolate the torque
            axis: DVec3::X,
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Y,
            vel: DVec3::ZERO,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
        };
        let tau = 1.0e-5f64;

        // ★ PERPENDICULAR moment — a tumble. Must survive, at exactly the rate `I_perp` gives.
        let mut tumbling = fresh();
        step_one_rod(
            &mut tumbling,
            mass,
            &contact,
            9.81,
            rho,
            dt,
            DVec3::ZERO,
            DVec3::Z * tau,
            0.0,
            &[],
            0.0,
        );
        let i_perp = tumbling.principal_inertia_kgm2(mass).z;
        let want = tau * dt / i_perp;
        let got = tumbling.ang_vel.length();
        println!(
            "perpendicular moment {tau:.1e} N·m · I_perp {i_perp:.4e} -> Δω want {want:.6e}, got {got:.6e} rad/s"
        );
        assert!(
            // 1e-3, not 1e-9: free precession acts in the same step (measured 7.4e-5 relative), so
            // demanding exactness would be demanding the absence of other physics. It still separates
            // the two cases by four orders, which is what the test is for.
            (got - want).abs() / want < 1.0e-3,
            "a perpendicular moment must tumble the member at the rate its inertia gives: \
             {got:.6e} against {want:.6e} rad/s"
        );

        // ★★ AXIAL moment — torsion. Must NOT become a rigid spin.
        let mut twisting = fresh();
        step_one_rod(
            &mut twisting,
            mass,
            &contact,
            9.81,
            rho,
            dt,
            DVec3::ZERO,
            DVec3::X * tau,
            0.0,
            &[],
            0.0,
        );
        let i_ax = twisting.principal_inertia_kgm2(mass).x;
        let rigid = tau * dt / i_ax;
        println!(
            "axial moment       {tau:.1e} N·m · I_axial {i_ax:.4e} -> a rigid body would spin at \
             {rigid:.4e} rad/s (tip {:.1} m/s); got {:.6e}",
            rigid * 0.5 * length,
            twisting.ang_vel.length()
        );
        assert!(
            twisting.ang_vel.length() < 1.0e-12,
            "an axial moment must NOT spin a ribbon about its own length: got {:.6e} rad/s. \
             That mode is torsional and elastic, not inertial — treating it as a free rigid-body \
             degree of freedom is what produced 6.6e5 rad/s in a single step.",
            twisting.ang_vel.length()
        );
    }

    /// ★★★ **A CONTACT FORCE BENDS THE MEMBER IT PUSHES ON** (docs/46 row 79).
    ///
    /// The pile's NaN is not a wrong line: it is a body that **bends for drawing and is rigid for
    /// responding**. A contact at a bent member's polyline sits up to a half-length off its own axis,
    /// and with `I_axial = 3.4e-10 kg·m²` that arm turns an ordinary force into a 169 m/s tip speed,
    /// which feeds the damping, which feeds the spin. A real blade pushed sideways does not spin up —
    /// **it bends**, and the energy goes into shape.
    ///
    /// The reference is the analytic cantilever, `δ = P·L³/(3·EI)`, valid while the deflection stays
    /// small — so the load here is deliberately gentle. Small-deflection theory is the only regime
    /// where an independent closed form exists, which is exactly why the test lives there.
    #[test]
    fn a_point_load_bends_a_member_by_the_cantilever_deflection() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");

        let mut rod = Rod {
            centre: DVec3::ZERO,
            axis: DVec3::X,
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Y,
            vel: DVec3::ZERO,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
        };
        let ei = rod.flexural_rigidity_nm2(&mats, m);

        // A gentle tip load, sized so the deflection stays inside small-deflection theory.
        let p = 0.02 * ei / (length * length);
        let want = p * length.powi(3) / (3.0 * ei);
        println!(
            "EI {ei:.4e} N·m² · L {length:.3} m · tip load {p:.4e} N -> cantilever δ = {want:.6} m ({:.1}% of L)",
            100.0 * want / length
        );

        // No gravity: this isolates the contact's own contribution to the shape.
        rod.relax_flex_under(ei, 0.0, &[(length, DVec3::NEG_Y * p)]);
        let got = rod.tip_sag_m();
        let err = (got - want).abs() / want;
        println!(
            "  {} segments -> {got:.6} m · {:+.2}% of the analytic cantilever",
            Flex::SEGMENTS,
            100.0 * (got - want) / want
        );
        assert!(
            got > 0.5 * want,
            "a point load must BEND the member: {got:.6} m against a {want:.6} m cantilever              deflection. A rigid member reports 0 and puts the whole force into spin instead."
        );

        // ★★ AND IT MUST CONVERGE, which is the claim that matters (Law 8). Asserting a tolerance at
        // one resolution would be asserting the discretisation. MEASURED: 8.98% at 8 segments, then
        // 4.59 / 2.32 / 1.16 — halving per doubling, first order, exactly as `flexure::Chain`'s own
        // convergence test shows. The shape is right; 8 segments is simply coarse, and the budget knob
        // is `Flex::SEGMENTS`.
        let mut prev = f64::INFINITY;
        for n in [8usize, 16, 32, 64] {
            let chain = crate::flexure::Chain::relaxed(ei, length, n, 0.0, |s| {
                let ds = length / n as f64;
                let k = ((s / ds).floor() as usize).min(n - 1);
                (0.0, if k == n - 1 { -p / ds } else { 0.0 })
            });
            let (tx, ty) = chain.tip();
            let d = ((tx - length).powi(2) + ty * ty).sqrt();
            let e = (d - want).abs() / want;
            println!(
                "    {n:3} segments -> {:+.2}% off",
                100.0 * (d - want) / want
            );
            assert!(
                e <= prev * 1.05 + 1e-12,
                "refining must not take it further from the cantilever: {n} segments {:.2}% vs {:.2}%",
                100.0 * e,
                100.0 * prev
            );
            prev = e;
        }
        assert!(
            prev < 0.02,
            "and 64 segments must sit within 2% of the analytic cantilever: {:.2}%",
            100.0 * prev
        );
        let _ = err;
    }

    /// ★★★ **A BLADE SAGS DOWNWARD, WHATEVER WAY IT WAS ROLLED** (docs/46 row 77).
    ///
    /// `relax_flex` asked `flexure::Chain` for the shape under `(0, −weight)` **in the chain's own 2D
    /// plane**, and `polyline` laid that shape out along the member's `normal` — which is the seeded
    /// ROLL drawn at release. So the bend direction was whatever the random roll happened to be:
    /// sideways, or upward. **Gravity did not enter it at all**, which is Law I inverted — the matter
    /// should decide which way it sags, not a random number.
    ///
    /// The test is the one thing that cannot be argued with: sweep the roll all the way round, and the
    /// tip must never end up HIGHER than a straight member's would. Matter does not fall upward.
    #[test]
    fn a_blade_sags_downward_whatever_way_it_was_rolled() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let q = mass / length * 9.81;

        let mut worst_rise = f64::NEG_INFINITY;
        let mut worst_roll = 0.0;
        for k in 0..12 {
            let roll = k as f64 / 12.0 * std::f64::consts::TAU;
            // Horizontal member, rolled about its own length by `roll`.
            let mut rod = Rod {
                centre: DVec3::ZERO,
                axis: DVec3::X,
                half_length_m: 0.5 * length,
                radius_m: radius,
                width_m: width,
                thickness_m: thickness,
                normal: DVec3::new(0.0, roll.cos(), roll.sin()),
                vel: DVec3::ZERO,
                ang_vel: DVec3::ZERO,
                release_t_s: 0.0,
                flex: Flex::straight(),
            };
            let ei = rod.flexural_rigidity_nm2(&mats, m);
            rod.relax_flex(ei, q, 9.81);
            let tip = *rod.polyline().last().expect("a tip");
            let straight_tip = rod.centre + rod.axis * rod.half_length_m;
            let rise = tip.y - straight_tip.y;
            println!(
                "  roll {:5.0}° · normal ({:+.2}, {:+.2}, {:+.2}) · tip rises {:+.5} m",
                roll.to_degrees(),
                rod.normal.x,
                rod.normal.y,
                rod.normal.z,
                rise
            );
            if rise > worst_rise {
                worst_rise = rise;
                worst_roll = roll.to_degrees();
            }
        }
        println!("  worst rise {worst_rise:+.5} m at roll {worst_roll:.0}°");
        assert!(
            worst_rise <= 1.0e-9,
            "a blade must never sag UPWARD: at roll {worst_roll:.0}° its tip rose {worst_rise:.5} m. \
             The bend direction was taken from the seeded roll instead of from gravity."
        );
    }

    /// ★★★ **A BENT BLADE IS FELT WHERE ITS MATTER IS, NOT WHERE ITS AXIS WOULD BE** (docs/46 row 76).
    ///
    /// Row 76 measured the gap: `Rod` could bend, and bending it changed the heap by **nothing at all,
    /// bit-identically, at 13.8× the cost** — because `closest_points` solves segment-to-segment on
    /// `Rod::axis`, so a member's computed shape had no reader. A blade could droop 99.7% of its length
    /// and its neighbours still met a straight capsule.
    ///
    /// This is the invariant that closes it, and it needs no tolerance: **place a neighbour against the
    /// bent body's real tip, and the contact must find it.** A straight-axis test reports the gap to a
    /// rod that is not there.
    #[test]
    fn a_bent_blade_is_felt_where_its_matter_is() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");

        let mut bent = Rod {
            centre: DVec3::ZERO,
            axis: DVec3::X,
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Y,
            vel: DVec3::ZERO,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
        };
        let ei = bent.flexural_rigidity_nm2(&mats, m);
        let q = mass / length * 9.81;
        bent.relax_flex(ei, q, 9.81);

        let poly = bent.polyline();
        let straight_tip = bent.centre + bent.axis * bent.half_length_m;
        let real_tip = *poly.last().expect("a polyline has a tip");
        let moved = (real_tip - straight_tip).length();
        println!(
            "bent blade: tip is {moved:.5} m ({:.0}% of length) from where a straight rod would put it",
            100.0 * moved / length
        );
        assert!(
            moved > 0.1 * length,
            "the test needs a genuinely bent body: tip moved only {moved:.6} m"
        );

        // A STRAIGHT neighbour resting on the bent tip — touching the real matter, nowhere near the
        // axis. ★ It must be straight: the first version used `..bent.clone()`, which carried the flex
        // along, so the "neighbour" was itself bent and its matter was not where it had been placed.
        let touch = 2.0 * radius;
        let neighbour = Rod {
            centre: real_tip + DVec3::Z * (0.5 * touch),
            axis: DVec3::Z,
            flex: Flex::straight(),
            ..bent.clone()
        };
        let (pa, pb) = closest_points_between(&poly, &neighbour.polyline());
        let gap_bent = (pa - pb).length();
        let (sa, sb) = closest_points(
            bent.centre - bent.axis * bent.half_length_m,
            straight_tip,
            neighbour.centre - neighbour.axis * neighbour.half_length_m,
            neighbour.centre + neighbour.axis * neighbour.half_length_m,
        );
        let gap_straight = (sa - sb).length();
        println!("  polyline gap {gap_bent:.6} m vs straight-axis gap {gap_straight:.6} m · touch {touch:.6} m");
        assert!(
            gap_bent < touch,
            "the bent body must be IN CONTACT with a neighbour resting on its tip: gap {gap_bent:.6} m \
             against a {touch:.6} m touch distance"
        );
        assert!(
            gap_straight > touch,
            "and the straight-axis test must MISS it — otherwise this test proves nothing: {gap_straight:.6} m"
        );
    }

    /// ★★★ **A BLADE IN A HEAP IS BENT, AND A RIGID ONE CANNOT PACK LIKE HAY** (docs/46 row 75).
    ///
    /// `flexure::Chain` is built and proven and nothing in the pile uses it: `Rod` is one rigid
    /// capsule. That is not a small idealisation for this body. Greenhill's critical length for a dry
    /// blade is 20.8 cm and the blade is 35 cm — **1.7× past it** — so a grass blade cannot hold itself
    /// straight under its own weight, let alone under a neighbour's. Bending is its NORMAL state and
    /// rigidity is the approximation.
    ///
    /// This asserts the consequence rather than the mechanism: a member released horizontally and left
    /// alone must **sag**. A rigid capsule cannot, whatever else it does.
    #[test]
    fn a_blade_resting_across_a_gap_sags_under_its_own_weight() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let (width, thickness) = cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let rho = sea_level_air(&mats);

        // Horizontal, one end held, the rest unsupported: the plainest cantilever there is.
        let mut rod = Rod {
            centre: DVec3::new(0.5 * length, 0.2, 0.0),
            axis: DVec3::X,
            half_length_m: 0.5 * length,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Y,
            vel: DVec3::ZERO,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
        };
        let ei = rod.flexural_rigidity_nm2(&mats, m);
        let q = mass / length * 9.81;
        println!(
            "dry blade: EI {ei:.4e} N·m² · self weight {q:.4e} N/m · Greenhill L_crit {:.4} m vs L {length:.3} m",
            crate::flexure::greenhill_critical_length_m(ei, q)
        );

        let dt =
            crate::substep::accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0).min(1.0e-4);
        for _ in 0..200_000 {
            // Held at the base, free everywhere else: only the flex state may move.
            rod.relax_flex(ei, q, 9.81);
            rod.vel = DVec3::ZERO;
            rod.ang_vel = DVec3::ZERO;
            let _ = (dt, rho, &contact);
        }
        let sag = rod.tip_sag_m();
        println!(
            "  tip sags {:.5} m = {:.1}% of its length",
            sag,
            100.0 * sag / length
        );
        // ★ WHAT SHAPE IS IT, not just how far the tip moved. A body that CURLS occupies far less
        // space than one that droops, and packing is matter over envelope — so a curl would raise the
        // reported packing without anything tangling.
        let poly = rod.polyline();
        let span = (*poly.last().unwrap() - *poly.first().unwrap()).length();
        // ★ The angles are ABSOLUTE per segment, so their SUM is not a turn — a first version printed
        // "total turn -485deg" for a body whose span is 97% of its arclength, which is impossible and
        // was my instrument, not the shape. The tip angle is the honest summary.
        let tip_angle = rod.flex.theta.last().copied().unwrap_or(0.0);
        println!(
            "  end-to-end span {span:.5} m of a {length:.3} m arclength ({:.0}%) · tip at {:.0}deg",
            100.0 * span / length,
            tip_angle.to_degrees()
        );

        assert!(
            sag > 0.05 * length,
            "a grass blade 1.7x past its own critical length must sag under its weight: {sag:.5} m \
             is only {:.2}% of its length. A rigid capsule sags 0 and cannot tangle the way hay does.",
            100.0 * sag / length
        );
    }

    /// ★★★ **RE-MEASURE THE HEAP WITH THE NEW INSTRUMENT** (docs/46 rows 67, 70, 71, 60 step A3/B).
    ///
    /// Every packing figure this module has ever reported was taken through machinery that has since
    /// been replaced: an invented `vel *= 1 − 2·dt` drag, a member 212× too soft, a blade shaped like a
    /// wire instead of a ribbon, rods that could not turn, and a settle gauge blind to rotation. Those
    /// numbers are not necessarily wrong, but they were read off an instrument that no longer exists,
    /// so they are re-taken here rather than inherited.
    ///
    /// ★★ **At a member count that FINISHES, and that is the honest caveat.** The 400-blade run this
    /// module's headline test uses did not complete in 19 minutes of release-build wall clock and was
    /// abandoned — exactly what `docs/46` row 69 predicts, since row 67's honest stem stiffness cut the
    /// timestep 194× and the neighbour loop is O(n²) on top. Row 60 step 1 established that packing
    /// CONVERGES with member count, which is what makes a smaller count a real measurement instead of a
    /// smaller one; but the convergence was itself measured with the old instrument, so treat these as
    /// the new baseline rather than as comparable to the old figures.
    #[test]
    #[ignore]
    fn re_measure_the_heap_with_the_new_instrument() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("rod");
        let rho = sea_level_air(&mats);
        println!(
            "dry blade: {length:.3} m · capsule r {:.4} mm · air {rho:.4} kg/m³",
            radius * 1000.0
        );
        for n in [10usize] {
            match settle_traced(&blade, &mats, n, 9.81, rho, 20260810, 0.5) {
                Some((s, tr)) => {
                    let mean_touch = tr.iter().map(|x| x.contacting_fraction).sum::<f64>()
                        / tr.len().max(1) as f64;
                    let last_touch = tr.last().map(|x| x.contacting_fraction).unwrap_or(0.0);
                    let last_w = tr.last().map(|x| x.peak_ang_speed_rads).unwrap_or(0.0);
                    // ★ IS ANY OF THIS A NUMBER? `f64::max` returns the non-NaN operand, so a heap
                    // full of NaN positions reports a peak speed of 0.000000 and looks perfectly
                    // settled. Check before believing the statistics.
                    // ★★★ **ASK THE MEMBERS, NOT THE STATISTICS** (docs/46 rows 79, 84). The check
                    // that used to live here read `height_m` and `peak_speed_ms` out of the trace —
                    // both `f64::max` folds, and `f64::max` RETURNS THE NON-NaN OPERAND, so a heap of
                    // NaN reports a peak speed of 0.000000 and a tidy height. Row 79 records clearing
                    // NaN twice through exactly those instruments. `Settled::all_finite` is a direct
                    // predicate over every member's centre, velocity, spin, axis and polyline nodes.
                    let folded_says_finite = tr
                        .last()
                        .map(|x| x.height_m.is_finite() && x.peak_speed_ms.is_finite())
                        .unwrap_or(false);
                    println!(
                        "      finite: DIRECT {} · folded-statistics say {} {}",
                        s.all_finite,
                        folded_says_finite,
                        if s.all_finite == folded_says_finite {
                            "(agree)"
                        } else {
                            "★ THEY DISAGREE — believe the direct one"
                        }
                    );
                    // ★★★ **THE SCALING, NOT THE VALUE** (docs/72 §3.9). Row 79's tell was that packing
                    // rose EXACTLY +700% per halving — the `cell³` factor — which can only happen if
                    // the occupied cell COUNT is constant. A constant of 1 is a heap that is not there.
                    // Print the count and the ratio between successive refinements; it is nearly free.
                    println!(
                        "      occupied cells vs resolution (count must GROW as cells shrink):"
                    );
                    let mut prev: Option<(f64, usize)> = None;
                    for &(c, n_cells) in &s.cells_vs_cell {
                        let note = match prev {
                            None => String::new(),
                            Some((_, pn)) if pn == 0 => String::new(),
                            Some((_, pn)) => {
                                let r = n_cells as f64 / pn as f64;
                                format!(
                                    "  ratio {r:5.2}x{}",
                                    if r < 1.05 {
                                        "  ★ NOT GROWING — the heap is one cell at every resolution"
                                    } else {
                                        ""
                                    }
                                )
                            }
                        };
                        println!("        cell {c:.5} m -> {n_cells:6} cells{note}");
                        prev = Some((c, n_cells));
                    }
                    println!(
                        "  {n:3} blades -> packing {:.5} · quiet {} after {:.3} s · peak centre {:.6} m/s",
                        s.packing, s.quiet, s.elapsed_s, s.peak_speed_ms
                    );
                    // ★ The gauge asks about the fastest POINT, so report what it is actually seeing:
                    // a heap can be translationally dead and still be turning.
                    // ★★★ IS THERE A PLATEAU? A packing number without this sweep is a claim about
                    // `cell_m` (docs/46 row 78). Packing has no limit as the cell shrinks — it runs to
                    // 1 as the envelope wraps each blade — so a bulk density exists only where the
                    // curve flattens.
                    println!("      packing vs cell (coarse -> fine):");
                    let sweep = &s.packing_vs_cell;
                    for (i, (c, pk)) in sweep.iter().enumerate() {
                        let change = if i == 0 {
                            String::from("      -")
                        } else {
                            format!(
                                "{:+7.1}%",
                                100.0 * (pk - sweep[i - 1].1) / sweep[i - 1].1.max(1e-12)
                            )
                        };
                        println!(
                            "        cell {c:.5} m ({:5.1}x blade dia) -> packing {pk:.5} {change}",
                            c / (2.0 * radius)
                        );
                    }
                    let last_surf = tr.last().map(|x| x.peak_surface_speed_ms).unwrap_or(0.0);
                    println!(
                        "      peak |ω| {last_w:.5} rad/s -> surface {:.6} m/s · quiescent {:.5} m/s -> {}",
                        last_surf,
                        crate::recohere::quiescent_speed(9.81, radius as f32),
                        if last_surf > crate::recohere::quiescent_speed(9.81, radius as f32) as f64 {
                            "STILL TURNING"
                        } else {
                            "rotationally at rest"
                        }
                    );
                    println!(
                        "      contacting fraction: mean {:.3} over the run, {:.3} at the end \
                         -> block-stepping could save at most {:.2}x",
                        mean_touch,
                        last_touch,
                        1.0 / (mean_touch + (1.0 - mean_touch) / 32.0).max(1e-9)
                    );
                }
                None => println!("  {n:3} blades -> no heap"),
            }
        }
        for n in [] as [usize; 0] {
            match settle(&blade, &mats, n, 9.81, rho, 20260810) {
                Some(s) => println!(
                    "  {n:3} blades -> packing {:.5} ({:.1} kg/m³ at straw's 1400) · {:.4} m tall · \
                     quiet {} after {:.3} s · peak member {:.5} m/s",
                    s.packing,
                    s.packing * 1400.0,
                    s.height_m,
                    s.quiet,
                    s.elapsed_s,
                    s.peak_speed_ms
                ),
                None => println!("  {n:3} blades -> no heap"),
            }
        }
        println!("  (loose hay is 40 kg/m³ = 0.029 packing; a field bale 100 = 0.071)");
    }

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

        let settled =
            settle(&blade, &mats, 400, 9.81, sea_level_air(&mats), 20260810).expect("a heap forms");
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
        // ★★★ IT NOW REACHES EQUILIBRIUM — AND IT IS STILL NOT A SETTLED HEAP (2026-08-16).
        //
        // With the floor fixed (row 60), the restitution calibrated (row 62) and a damping-aware
        // timestep, the energy trace is monotone (worst drawup +0.000%) and the gauge fires. What it
        // fires on is NOT a haystack: the heap ends 0.78 m tall having fallen 0.077 m from a release
        // that was 0.86 m tall. It did not pile up, it SAGGED.
        //
        // The release drops rod CENTRES ~19x closer together than a rod is long, so all 400 members
        // start interpenetrated with many neighbours. Their repulsive springs balance internally and
        // the cloud equilibrates as a jammed elastic blob that never rests on the floor. The old
        // under-damped contact hid this by blowing the cloud apart, which looked like falling.
        //
        // So the packing below is STILL NOT a bulk density, for a new and better-understood reason,
        // and must not be compared with loose hay's 0.029. The fix is the release, and it is Robin's
        // own original framing (docs/71 §3b): a haystack is built forkful by forkful, so members must
        // be dropped SEQUENTIALLY onto a settling heap rather than conjured overlapping in one cloud.
        assert!(
            settled.height_m > 0.5 * length,
            "it should stack, not lie flat: {:.3} m",
            settled.height_m
        );
        assert!(
            (0.0015..0.0025).contains(&settled.packing),
            "the heap has moved off its recorded 0.0019: {:.4}. Still not a bulk density — the \
             release starts interpenetrated (docs/46 row 60).",
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
        let (s, trace) = settle_traced(&blade, &mats, 400, g, sea_level_air(&mats), 20260810, 0.25)
            .expect("a heap forms");

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
            let s = settle(&blade, &mats, n, 9.81, sea_level_air(&mats), 20260810)
                .expect("a heap forms");
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
        // ★★★ AND EVEN WHEN EVERY RUNG SETTLES, THESE ARE STILL NOT BULK DENSITIES. Said out loud
        // because the moment the line above stops printing, a reader takes the trend for a
        // convergence result. The release drops rod CENTRES ~19x closer together than a rod is LONG,
        // so every member starts interpenetrated with many others; the cloud equilibrates as a jammed
        // elastic blob that SAGS rather than a heap that FALLS. MEASURED: 400 rods end 0.774 m tall
        // from a 0.861 m release, having dropped 0.077 m. Until members are released SEQUENTIALLY
        // onto a settling heap — Robin's forkful-by-forkful framing, docs/71 §3b — the trend below is
        // a fact about the release, not about straw.
        let tallest = ladder
            .iter()
            .map(|(_, s)| s.height_m)
            .fold(0.0f64, f64::max);
        println!(
            "\n★ STILL NOT A BULK DENSITY, even where every rung came to rest: the release starts \
             INTERPENETRATED, so these equilibrate by sagging rather than by piling. Tallest rung \
             {tallest:.3} m against a ~0.861 m release. Fix the release (sequential drop) before \
             comparing any of this with loose hay's 0.029."
        );
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
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        println!("dry blade rod: {length:.4} m long, radius {radius:.5e} m");
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
            width_m: 2.0 * radius,
            thickness_m: 0.5 * radius,
            normal: DVec3::Y,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
            vel: DVec3::new(0.0, -1.0, 0.0), // driving downward into the floor
        };
        let hit = floor_contact(&sunk, sunk.vel, radius, contact.friction);
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
            width_m: 2.0 * radius,
            thickness_m: 0.5 * radius,
            normal: DVec3::Y,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
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

#[cfg(test)]
mod relaxation_idempotence_tests {
    //! ★★★ **IS THE SHAPE RELAXATION A FIXED POINT?** (`docs/72` queue item 2, `docs/46` row 79.)
    //!
    //! Measured in the 10-blade heap: at **step 1 a polyline point moves 0.59 m** — 1.7× the blade's
    //! own 0.35 m arclength — while the fastest CENTRE moves 5.4e-4 m. A shape/translation ratio of
    //! **1100×**, with the polyline point COUNT unchanged, so it is not an indexing artefact.
    //!
    //! That should be impossible here. `settle_traced` releases members already bent
    //! ([`release_rods_bent`] with a real `flex_ei`), and at step 1 there are **no contacts at all**
    //! (measured: `overlap 0`, `|a| 0`), so step 1 relaxes under gravity alone — the same load the
    //! release used. A member released at its gravity-relaxed shape and relaxed again under gravity
    //! should not move.
    //!
    //! So: does [`Rod::relax_flex_under`] return the same shape when applied twice to the same load?
    //! If not, the "bend before the no-overlap rejection" guarantee is void — the release rejects
    //! overlaps between shapes that the first step then replaces.

    use super::*;

    /// A horizontal member, so "which way does it sag" has an unambiguous right answer: DOWN.
    fn a_horizontal_blade(length: f64, radius: f64, width: f64, thickness: f64) -> Rod {
        Rod {
            centre: DVec3::ZERO,
            axis: DVec3::X,
            half_length_m: length * 0.5,
            radius_m: radius,
            width_m: width,
            thickness_m: thickness,
            normal: DVec3::Z,
            vel: DVec3::ZERO,
            ang_vel: DVec3::ZERO,
            release_t_s: 0.0,
            flex: Flex::straight(),
        }
    }

    /// ★ **THIS TEST IS NOW TRIVIALLY TRUE, AND THAT IS ITS POINT.** Since the fix, `relax_flex`
    /// delegates to `relax_flex_under`, so the two names are one function and cannot disagree. It is
    /// kept as a RATCHET: it goes red the day someone gives `relax_flex` its own body again. The test
    /// that carries the physics is `a_member_sags_DOWNWARD_under_its_own_weight` — a direction, not an
    /// agreement, because two implementations agreeing on a wrong answer is exactly what a magnitude-
    /// only assert already let through here.
    #[test]
    fn the_two_shape_relaxations_must_agree_they_answer_one_question() {
        // ★★★ **ONE QUESTION, TWO ANSWERS** (Law II, `docs/46` row 79, `docs/72` item 2).
        //
        // "What shape does this member take under its own weight" is asked in two places and answered
        // by two different functions: the RELEASE calls `relax_flex_under` (pile.rs, `release_rods_bent`)
        // and EVERY STEP calls `relax_flex` (`step_one_rod`). With no contact loads they are the same
        // physical question, so they must give the same shape.
        //
        // They do not. `relax_flex` passes `-weight_per_m * mag` to `Chain::relaxed`; `relax_flex_under`
        // passes `w_transverse = (NEG_Y · weight_per_m) · dir`, which has the OPPOSITE sign for the same
        // configuration. The member is therefore bent one way at release and bent back the other way on
        // its very first step.
        //
        // ★ MEASURED CONSEQUENCE in the 10-blade heap: at step 1 a polyline point moves **0.59 m** — 1.7x
        // the blade's own 0.35 m arclength — while the fastest CENTRE moves 5.4e-4 m. A shape/translation
        // ratio of 1100x, with the polyline point count unchanged and the relaxation itself verified
        // idempotent. That displacement is how a 6.4 um overlap appears between two members at step 2
        // when one step of gravity can only move them 1.8e-12 m.
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let (width, thickness) = cross_section_for(&blade).expect("a blade has a section");
        let member_mass = blade.mass_kg(&mats).expect("a mass").max(1e-12);
        let material = blade
            .parts
            .first()
            .map(|p| p.material.clone())
            .unwrap_or_default();
        let m = mats.iter().find(|m| m.id == material).expect("catalogued");
        let ei = m.youngs_modulus as f64 * width * thickness.powi(3) / 12.0;
        let w_per_m = member_mass / length * 9.81;

        let mut by_step = a_horizontal_blade(length, radius, width, thickness);
        by_step.relax_flex(ei, w_per_m, 9.81);
        let mut by_release = a_horizontal_blade(length, radius, width, thickness);
        by_release.relax_flex_under(ei, w_per_m, &[]);

        let tip_step = *by_step.polyline().last().expect("a tip");
        let tip_release = *by_release.polyline().last().expect("a tip");
        let gap = (tip_step - tip_release).length();

        // Gravity is -Y and the member is horizontal, so the correct tip is BELOW the centre.
        println!(
            "relax_flex tip y {:+.6} · relax_flex_under tip y {:+.6} · they differ by {gap:.4e} m              (member length {length:.3} m)",
            tip_step.y, tip_release.y
        );

        assert!(
            gap < 1.0e-6,
            "the release and the step answer ONE question — what shape does this member take under its              own weight — and they disagree by {gap:.4e} m, {:.0}% of the member's {length:.3} m length.              `relax_flex` puts the tip at y {:+.6}, `relax_flex_under` at y {:+.6}; gravity is -Y, so at              most one of them is sagging. A body re-shaped between placement and its first step has              MOVED ITS MATTER WITH NO VELOCITY — no contact sees it and no energy accounts for it.",
            100.0 * gap / length,
            tip_step.y,
            tip_release.y
        );
    }

    #[test]
    fn a_member_sags_DOWNWARD_under_its_own_weight() {
        // ★★ THE ASSERT THAT DID NOT EXIST, and whose absence let a sign error live in the release.
        // `tip_sag_m` returns a MAGNITUDE — `((x−L)² + y²).sqrt()` — so a member arching UP over its
        // own weight reports exactly the same sag as one drooping down, and the module's sag test
        // passed throughout. **A magnitude cannot catch a sign.** This asks the physical question
        // instead: gravity is −Y, so the tip of a horizontal member must end up BELOW its base.
        //
        // General, not grass: any body whose shape is DERIVED state owes this check, because a
        // magnitude-only assert on a derived quantity is blind to exactly the error that inverts it.
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let (width, thickness) = cross_section_for(&blade).expect("a section");
        let member_mass = blade.mass_kg(&mats).expect("a mass").max(1e-12);
        let material = blade
            .parts
            .first()
            .map(|p| p.material.clone())
            .unwrap_or_default();
        let m = mats.iter().find(|m| m.id == material).expect("catalogued");
        let ei = m.youngs_modulus as f64 * width * thickness.powi(3) / 12.0;
        let w_per_m = member_mass / length * 9.81;

        for (name, relax) in [("relax_flex", 0u8), ("relax_flex_under", 1u8)] {
            let mut rod = a_horizontal_blade(length, radius, width, thickness);
            if relax == 0 {
                rod.relax_flex(ei, w_per_m, 9.81);
            } else {
                rod.relax_flex_under(ei, w_per_m, &[]);
            }
            let poly = rod.polyline();
            let base = poly.first().expect("a base");
            let tip = poly.last().expect("a tip");
            assert!(
                tip.y < base.y,
                "{name}: a horizontal member must SAG under its own weight, but its tip is at \
                 y {:+.6} against a base at y {:+.6} — it is arching upward against gravity",
                tip.y,
                base.y
            );
        }
    }

    #[test]
    fn relaxing_twice_under_the_same_load_gives_the_same_shape() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, _radius) = rod_for(&blade).expect("a blade is a rod");
        // Exactly what `settle_traced` computes, not a re-derivation of it.
        let member_mass = blade.mass_kg(&mats).expect("a mass").max(1e-12);
        let material = blade
            .parts
            .first()
            .map(|p| p.material.clone())
            .unwrap_or_default();
        let m = mats
            .iter()
            .find(|m| m.id == material)
            .expect("the blade's material is catalogued");
        let ei = {
            let (w, t) = cross_section_for(&blade).unwrap_or((0.0, 0.0));
            m.youngs_modulus as f64 * w * t.powi(3) / 12.0
        };
        let w_per_m = member_mass / length * 9.81;

        let mut rods = release_rods_bent(&blade, 10, 20260810, ei, w_per_m).expect("a release");
        let mut worst = 0.0f64;
        let mut worst_i = 0usize;
        for (i, r) in rods.iter_mut().enumerate() {
            let before = r.polyline();
            // Exactly what step 1 does when nothing is touching: relax under gravity, no contact loads.
            r.relax_flex_under(ei, w_per_m, &[]);
            let after = r.polyline();
            assert_eq!(
                before.len(),
                after.len(),
                "member {i} was re-segmented by a relaxation; a point-by-point diff would be meaningless"
            );
            let d = before
                .iter()
                .zip(after.iter())
                .map(|(a, b)| (*a - *b).length())
                .fold(0.0f64, f64::max);
            if d > worst {
                worst = d;
                worst_i = i;
            }
        }
        // ★ THE TWO CALL SITES DO NOT AGREE ON WHAT "WEIGHT PER UNIT LENGTH" MEANS.
        // The release divides the member's weight by `rod_for(member).0` — the blade's ARCLENGTH.
        // `settle_traced`'s per-step call divides by `2.0 * rod.half_length_m` — the capsule's straight
        // length. Those are the same number only while the member is straight.
        let r0 = &rods[0];
        let straight = 2.0 * r0.half_length_m;
        println!(
            "arclength {length:.6} m · 2*half_length_m {straight:.6} m · ratio {:.6} · w_per_m release \
{w_per_m:.6} vs per-step {:.6}",
            straight / length,
            member_mass / straight * 9.81
        );

        assert!(
            worst < 1.0e-6,
            "the release bends each member under gravity, so relaxing again under the SAME gravity \
             must be a fixed point — but member {worst_i} moved a polyline point {worst:.4e} m, which \
             is {:.1}% of its own {length:.3} m length. Matter that moves without a velocity is seen \
             by no contact and no energy accounting (docs/46 row 79).",
            100.0 * worst / length
        );
    }
}

#[cfg(test)]
mod heap_validity_tests {
    //! ★★★ **THE HEAP MUST STILL BE MADE OF NUMBERS** (`docs/46` rows 79, 84, 85).
    //!
    //! Re-measured 2026-09-15 after the release sign fix (row 84). What the fix DID achieve is real and
    //! measured: members now fall correctly, reaching **2.030 m/s at 0.3 s** against the blade's 2.05 m/s
    //! terminal velocity, and the occupied set is no longer degenerate — its cell count grows by **≈2.0×
    //! per halving**, the signature of a curve, where row 79 measured a constant 1 cell at every
    //! resolution.
    //!
    //! What it did NOT fix: by **0.5 s** the peak centre speed is **22.4 m/s — 11× terminal**, and by
    //! **0.7 s the heap is NaN**. Energy is still entering at CONTACT, somewhere in `0.3 s … 0.7 s`.
    //!
    //! ★★ And the instruments still lie about it. At 0.7 s `Settled::all_finite` (a direct predicate over
    //! every member) reports **false** while the folded statistics report **true** — because `f64::max`
    //! returns the non-NaN operand. At 20 s the folded report is row 79's fingerprint verbatim: packing
    //! **0.03798**, rising **exactly +700% per halving** (the `cell³` factor), peak speed **0.000000**,
    //! `quiet true`. A heap of NaN, reported as a settled heap.
    //!
    //! **So every packing figure this module produces remains VOID**, and this test is the gate that
    //! says so out loud rather than leaving it to a comment.

    use super::tests::sea_level_air;
    use super::*;

    #[test]
    #[ignore = "row 85: FAILS BY DESIGN — the heap goes NaN at contact; ~13 min at the default cap"]
    fn a_settled_heap_is_made_of_numbers() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let rho = sea_level_air(&mats);
        let (s, _trace) =
            settle_traced(&blade, &mats, 10, 9.81, rho, 20260810, 0.5).expect("a heap forms");
        assert!(
            s.all_finite,
            "the heap is not made of numbers: packing {:.5}, height {:.4} m, quiet {} after {:.3} s, \
             peak centre {:.6} m/s — and EVERY ONE of those is a `f64::max` fold, which returns the \
             non-NaN operand, so they are what a NaN heap looks like rather than evidence against one. \
             Believe `all_finite`. (docs/46 rows 79, 85)",
            s.packing,
            s.height_m,
            s.quiet,
            s.elapsed_s,
            s.peak_speed_ms,
        );
    }

    #[test]
    fn a_falling_member_does_not_exceed_its_terminal_velocity() {
        // ★★ A PHYSICAL BOUND, not a round number (`docs/72` §3.3). Before any contact, a blade in air
        // falls to terminal velocity and stops accelerating. Measured after the row-84 fix: 2.030 m/s at
        // 0.3 s, against `atmosphere`'s 2.05 m/s for this blade — correct. By 0.5 s it is 22.4 m/s, so
        // something puts in 11x terminal, and it arrives with contact. This bounds the FREE-FALL phase
        // only, which is why it is cheap enough to run every time: it is the half that works, held down
        // so it cannot regress while the contact half is chased.
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let rho = sea_level_air(&mats);
        // ONE member: a falling blade has no neighbours, so this isolates gravity + drag from contact
        // entirely rather than hoping contact stays quiet. It is also what makes it affordable — the
        // ten-member version of this measurement costs 220 s, which is six times the whole suite.
        std::env::set_var("PILE_CAP_S", "0.3");
        let settled = settle_traced(&blade, &mats, 1, 9.81, rho, 20260810, 0.05);
        std::env::remove_var("PILE_CAP_S");
        let (s, _trace) = settled.expect("a heap forms");
        assert!(
            s.all_finite,
            "the member went NaN in free fall, which is a different bug entirely"
        );
        // Terminal velocity for this member, from the same air the sim uses — derived, not typed.
        assert!(
            s.peak_speed_ms < 3.0,
            "a blade falling in air must approach its ~2.05 m/s terminal velocity, but the fastest \
             centre is {:.3} m/s at t = {:.3} s. Gravity and drag cannot produce that; something else \
             is doing work.",
            s.peak_speed_ms,
            s.elapsed_s
        );
    }
}

#[cfg(test)]
mod energy_gate_tests {
    //! ★★★ **THE PILE'S ENERGY GATE** (`docs/46` row 86).
    //!
    //! `docs/46` row 83 records the shape of this defect in the other module: `gpu-verify`'s scene I is
    //! labelled the FUDGE DETECTOR and guards **its own configuration only**, so scene D ran its whole
    //! life with no energy check and reported a plausible small number while holding **+638% of E₀**.
    //! `pile` had the same hole and worse: **no energy check at all**, and a meter that could not have
    //! seen the failure anyway, because it omitted rotation — the very channel row 79's explosion used.
    //!
    //! Two tests, because one configuration is not a conservation law:
    //!
    //! 1. **The control** — one member, in vacuum, stopped before it touches anything. Every known
    //!    omission is absent by construction, so energy must be conserved. This is what makes the gate
    //!    below *readable*: it validates the meter and the integrator, so a rise in the heap is a
    //!    statement about the heap rather than about arithmetic.
    //! 2. **The gate** — the real heap. Expected to FAIL while row 85's contact injection is open.

    use super::tests::sea_level_air;
    use super::*;

    fn dry_blade() -> (Assembly, Vec<crate::materials::Material>) {
        (
            crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY),
            crate::materials::load(),
        )
    }

    #[test]
    fn a_heap_mesh_carries_every_member_and_reports_the_ones_it_cannot() {
        // ★ The drawn shape must be the SIMULATED shape, and what cannot be drawn must be COUNTED.
        let (blade, mats) = dry_blade();
        let (length, _r) = rod_for(&blade).expect("a rod");
        let member_mass = blade.mass_kg(&mats).expect("a mass").max(1e-12);
        let material = blade
            .parts
            .first()
            .map(|p| p.material.clone())
            .unwrap_or_default();
        let m = mats.iter().find(|m| m.id == material).expect("catalogued");
        let (w, t) = cross_section_for(&blade).expect("a section");
        let ei = m.youngs_modulus as f64 * w * t.powi(3) / 12.0;
        let mut rods = release_rods_bent(&blade, 6, 20260810, ei, member_mass / length * 9.81)
            .expect("release");

        let (mesh, dropped) = heap_mesh(&rods, 0, [0.4, 0.5, 0.2]);
        assert_eq!(dropped, 0, "a healthy release has no undrawable members");
        assert!(
            !mesh.vertices.is_empty() && mesh.indices.len() % 3 == 0,
            "six members must produce a closed triangle mesh, got {} verts / {} indices",
            mesh.vertices.len(),
            mesh.indices.len()
        );
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.pos.iter().all(|c| c.is_finite())),
            "a mesh built from finite members must be finite"
        );
        // The bent shape, not the straight one: a rigid capsule would give 2 nodes and 6 quads per
        // member; a bent member gives Flex::SEGMENTS + 1 nodes and many more.
        let straight_quads_per_member = 6;
        assert!(
            mesh.indices.len() / 6 > 6 * straight_quads_per_member,
            "the mesh must follow the members' BENT centreline, but it has only {} quads for 6              members — that is the straight-capsule count, so the picture would disagree with the              collision (docs/46 row 76)",
            mesh.indices.len() / 6
        );

        // ★ AND A MEMBER THAT IS NOT A NUMBER MUST BE REPORTED, NOT QUIETLY OMITTED (rows 85, 86).
        rods[2].centre.y = f64::NAN;
        let (mesh2, dropped2) = heap_mesh(&rods, 0, [0.4, 0.5, 0.2]);
        assert_eq!(
            dropped2, 1,
            "a NaN member must be counted as undrawable so the picture can say so"
        );
        assert!(
            mesh2
                .vertices
                .iter()
                .all(|v| v.pos.iter().all(|c| c.is_finite())),
            "no NaN may reach the vertex buffer — it does not draw as anything, it just vanishes"
        );
    }

    #[test]
    fn the_settle_can_be_watched_and_hands_over_real_members() {
        // ★ A summary is not the state it summarises. Before `settle_watched` this module returned
        // statistics only, so nothing could draw a haystack forming — it could only be inferred from
        // scalars. This asserts the observer is handed the MEMBERS, in the shape they are actually in.
        let (blade, mats) = dry_blade();
        let mut frames: Vec<(f64, usize, f64)> = Vec::new();
        std::env::set_var("PILE_CAP_S", "0.05");
        let out = settle_watched(
            &blade,
            &mats,
            4,
            9.81,
            0.0,
            20260810,
            0.05,
            0.01,
            &mut |t, rods| {
                // Copy only what the assertion needs; the observer must not hold the borrow.
                let top = rods
                    .iter()
                    .flat_map(|r| r.polyline())
                    .map(|p| p.y)
                    .fold(f64::NEG_INFINITY, f64::max);
                frames.push((t, rods.len(), top));
            },
        );
        std::env::remove_var("PILE_CAP_S");
        let (_s, _t) = out.expect("a heap");
        assert!(
            frames.len() >= 3,
            "watching every 0.01 s of a 0.05 s run should yield several frames, got {}",
            frames.len()
        );
        assert!(
            frames.iter().all(|f| f.1 == 4),
            "every frame must carry all four members, got {:?}",
            frames.iter().map(|f| f.1).collect::<Vec<_>>()
        );
        assert!(
            frames.iter().all(|f| f.2.is_finite()),
            "the members handed over must be real geometry, not folded statistics"
        );
        // Time must advance, and the observer must not be called twice for one instant.
        for w in frames.windows(2) {
            assert!(
                w[1].0 > w[0].0,
                "observer time went backwards: {:?}",
                frames
            );
        }
    }

    #[test]
    fn an_isolated_falling_member_conserves_energy() {
        // ★ THE CONTROL, and it is the negative control the rest of this module kept lacking. ONE
        // member so there is no neighbour contact and no cohesion well; **vacuum** so nothing
        // dissipates; stopped long before the ground so `terrain_contact_resolve` never projects a
        // position. The release already bends the member to its gravity equilibrium and that
        // relaxation is idempotent (row 84), so its SHAPE does not change either — which matters,
        // because `mechanical_energy_j` measures height at `centre.y` while the matter is distributed
        // along a bent polyline, so a shape that moved would move energy this meter cannot see.
        //
        // With all of that absent, gravity is conservative and E must not move at all.
        let (blade, mats) = dry_blade();
        std::env::set_var("PILE_CAP_S", "0.25");
        let settled = settle_traced(&blade, &mats, 1, 9.81, 0.0, 20260810, 0.05);
        std::env::remove_var("PILE_CAP_S");
        let (s, _t) = settled.expect("a single member");
        assert!(
            s.all_finite,
            "the control member went NaN, which is a different bug"
        );
        let drift = (s.peak_energy_j - s.energy_j_at_release) / s.energy_j_at_release.abs();
        println!(
            "control: E0 {:.6e} J -> peak {:.6e} J ({:+.4}%) at t={:.4} s · rotational share at peak {:.3e} J",
            s.energy_j_at_release, s.peak_energy_j, 100.0 * drift, s.peak_energy_t_s,
            s.peak_rotational_energy_j
        );
        assert!(
            drift < 1.0e-6,
            "an isolated member falling in vacuum has ONE force on it and that force is conservative, \
             so its mechanical energy must not rise at all — but it gained {:+.4}% by t={:.4} s. If \
             this fails, the meter or the integrator is wrong and NOTHING measured with them means \
             anything, including the heap gate below.",
            100.0 * drift,
            s.peak_energy_t_s,
        );
    }

    #[test]
    #[ignore = "row 86: FAILS BY DESIGN — the heap gains energy at contact (row 85); ~4 min"]
    fn the_heap_gains_no_energy() {
        // Gravity is the only source. Drag, damping and friction can only REMOVE. So the heap's
        // mechanical energy may fall and must never rise — beyond the two bounded omissions named on
        // `mechanical_energy_j` (the cohesion well, the terrain position projection), which are small.
        let (blade, mats) = dry_blade();
        let rho = sea_level_air(&mats);
        std::env::set_var("PILE_CAP_S", "0.7");
        let settled = settle_traced(&blade, &mats, 10, 9.81, rho, 20260810, 0.1);
        std::env::remove_var("PILE_CAP_S");
        let (s, _t) = settled.expect("a heap forms");
        let gain = (s.peak_energy_j - s.energy_j_at_release) / s.energy_j_at_release.abs();
        println!(
            "heap: E0 {:.6e} J -> peak {:.6e} J ({:+.1}%) at t={:.4} s · rotational at peak {:.4e} J · all_finite {} · energy left the reals at {:?}",
            s.energy_j_at_release,
            s.peak_energy_j,
            100.0 * gain,
            s.peak_energy_t_s,
            s.peak_rotational_energy_j,
            s.all_finite,
            s.first_non_finite_energy_t_s,
        );
        // ★ UNBOUNDED IS NOT A PERCENTAGE. Checked before the ratio, because `inf - x / x` is `inf`
        // and `NaN < 0.01` is FALSE — a ratio test would report either as a pass-shaped nothing.
        assert!(
            s.first_non_finite_energy_t_s.is_none(),
            "the heap's mechanical energy stopped being a number at t={:?} — there is no percentage for \
             that. E0 was {:.4e} J. (docs/46 rows 85, 86)",
            s.first_non_finite_energy_t_s,
            s.energy_j_at_release,
        );
        assert!(
            s.all_finite,
            "the heap's members are not finite, so every energy figure above describes a heap that is \
             no longer made of numbers (docs/46 row 85)"
        );
        assert!(
            gain < 0.01,
            "the heap gained {:+.1}% of its release energy by t={:.4} s (E0 {:.4e} -> {:.4e} J), and \
             gravity is the only source. The rotational share at the peak is {:.4e} J — the channel \
             the old meter omitted entirely. See docs/46 rows 85, 86.",
            100.0 * gain,
            s.peak_energy_t_s,
            s.energy_j_at_release,
            s.peak_energy_j,
            s.peak_rotational_energy_j,
        );
    }
}

#[cfg(test)]
mod spin_attribution_tests {
    //! ★★★ **WHICH TERM PUTS THE SPIN IN?** (`docs/46` row 87.)
    //!
    //! Row 86 measured that the heap's energy leaves the reals at `t = 0.46423 s` and that the last
    //! finite peak is **rotational, 5.85e186 J from a 1.7e-2 J release**. That names the CHANNEL and not
    //! the TERM — and `ang_vel` is written in three separate places in a step. Row 79 refuted eleven
    //! plausible causes by measurement before finding one, so this measures rather than argues.

    use super::tests::sea_level_air;
    use super::*;

    #[test]
    #[ignore = "row 87: a measurement, minutes long — cargo test -- --ignored"]
    fn which_term_puts_the_spin_in() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let rho = sea_level_air(&mats);
        let cap = std::env::var("SPIN_CAP_S").unwrap_or_else(|_| "0.10".into());
        let n: usize = std::env::var("SPIN_MEMBERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12);

        std::env::set_var("PILE_CAP_S", &cap);
        spin_probe_begin();
        // The bound, derived from the member: ten times the spin a blade falling at its own ~2.05 m/s
        // terminal velocity could carry on a 0.175 m half-length.
        let (length, _r) = rod_for(&blade).expect("a rod");
        spin_probe_set_bound(10.0 * 2.05 / (0.5 * length));
        let settled = settle_traced(&blade, &mats, n, 9.81, rho, 20260810, 0.05);
        let budget = spin_probe_take();
        std::env::remove_var("PILE_CAP_S");
        let (s, _t) = settled.expect("a heap");

        let total = budget.neighbour_torque_j + budget.free_precession_j + budget.floor_impulse_j;
        println!(
            "spin budget over {cap} s, {n} members ({} rod-steps):",
            budget.steps
        );
        println!(
            "  neighbour torque  {:+.4e} J   ({:+.1}% of the total change)",
            budget.neighbour_torque_j,
            100.0 * budget.neighbour_torque_j / total.abs().max(f64::MIN_POSITIVE)
        );
        println!(
            "  free precession   {:+.4e} J   ({:+.1}%)  ★ ENTITLED TO ZERO — torque-free motion \
             conserves energy exactly, so anything here is integration error",
            budget.free_precession_j,
            100.0 * budget.free_precession_j / total.abs().max(f64::MIN_POSITIVE)
        );
        println!(
            "  floor impulse     {:+.4e} J   ({:+.1}%)  ★ MAY ONLY REMOVE (non-injecting, row 36)",
            budget.floor_impulse_j,
            100.0 * budget.floor_impulse_j / total.abs().max(f64::MIN_POSITIVE)
        );
        println!(
            "  floor LINEAR      {:+.4e} J   (a constraint removing into-surface velocity should be \
             strongly negative)",
            budget.floor_linear_j
        );
        println!(
            "  floor LIFT (m·g·Δy) {:+.4e} J ★ the position projection — no velocity pays for this",
            budget.floor_lift_j
        );
        println!(
            "  floor NET         {:+.4e} J",
            budget.floor_impulse_j + budget.floor_linear_j + budget.floor_lift_j
        );
        // ★★★ WHICH TERM STOPPED BEING A NUMBER FIRST — the question the poisoned sums could not answer.
        let name = |v: u64| {
            if v == u64::MAX {
                "never".to_string()
            } else {
                format!("step {v}")
            }
        };
        println!(
            "  FIRST NON-FINITE: neighbour {} · precession {} · floor {}",
            name(budget.first_nonfinite_neighbour_step),
            name(budget.first_nonfinite_precession_step),
            name(budget.first_nonfinite_floor_step)
        );
        let nm = |v: u64| {
            if v == u64::MAX {
                "never".to_string()
            } else {
                format!("step {v}")
            }
        };
        println!(
            "  ★ FIRST EXCESSIVE |ω| (bound {:.1} rad/s): {} · |ω| {:.4e} -> {:.4e} rad/s",
            budget.omega_bound_rads,
            nm(budget.first_excess_step),
            budget.omega_before_excess,
            budget.omega_after_excess
        );
        println!(
            "    that step's rotational energy change: neighbour {:+.4e} J · precession {:+.4e} J",
            budget.excess_d_neighbour_j, budget.excess_d_precession_j
        );
        println!(
            "  |ω| largest finite {:.4e} rad/s · |ω| entering the step that diverged {:.4e} rad/s",
            budget.max_finite_omega_rads, budget.omega_at_precession_failure
        );
        println!(
            "  largest FINITE one-step gain: neighbour {:+.4e} · precession {:+.4e} · floor {:+.4e} J",
            budget.max_finite_neighbour_j,
            budget.max_finite_precession_j,
            budget.max_finite_floor_j
        );
        println!(
            "  worst single precession step {:+.4e} J · heap all_finite {} · E0 {:.4e} -> peak {:.4e}",
            budget.worst_precession_j, s.all_finite, s.energy_j_at_release, s.peak_energy_j
        );

        // ★★★ THE CLAIM THAT MATTERS, and it is not the one I set out to test. The floor's ROTATIONAL
        // gain is a legitimate TRANSFER — a blade landing on its end converts travelling into turning,
        // which is what makes it topple — and the floor's NET is strongly negative, so it is a sink.
        // What is not legitimate is the LIFT: `centre += dpos` hands a member potential energy with no
        // velocity paying for it, and the heap's whole measured gain equals it.
        let gain = s.peak_energy_j - s.energy_j_at_release;
        println!(
            "  ★ heap gain {:+.4e} J vs floor lift {:+.4e} J — ratio {:.3}",
            gain,
            budget.floor_lift_j,
            gain / budget.floor_lift_j
        );
        assert!(
            budget.floor_lift_j <= 1.0e-6 * s.energy_j_at_release.abs(),
            "the floor's POSITION PROJECTION created {:+.4e} J against a release energy of {:.4e} J, \
             and the heap's entire measured gain ({:+.4e} J) is that number. `centre += dpos` lifts a \
             member out of the surface with no velocity change and no matching term anywhere, so it is \
             potential energy from nothing. A constraint may REMOVE energy; it may not hand any over. \
             (docs/46 rows 36, 86, 87)",
            budget.floor_lift_j,
            s.energy_j_at_release,
            gain
        );
        assert!(
            budget.floor_impulse_j + budget.floor_linear_j + budget.floor_lift_j <= 0.0,
            "the floor is a net energy SOURCE, which a constraint may never be"
        );
        // A torque-free term cannot do work either.
        assert!(
            budget.free_precession_j <= 1.0e-9 * s.energy_j_at_release.abs(),
            "free precession put {:+.4e} J of rotational energy in, against a release energy of \
             {:.4e} J. τ = −ω × (I·ω) is TORQUE-FREE: it conserves angular momentum and kinetic \
             energy exactly, so every joule of that is integration error, not physics. With \
             I = (3.4e-10, 4.6e-6, 4.6e-6) the axial moment is four orders below the others, which is \
             exactly when an explicit step of Euler's equations goes unstable. (docs/46 rows 79, 86, 87)",
            budget.free_precession_j,
            s.energy_j_at_release
        );
    }
}
