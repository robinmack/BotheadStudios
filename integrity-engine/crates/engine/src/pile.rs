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

    /// **The velocity of the material at a point offset `r` from the centre** — `v + ω × r`. The
    /// quantity every contact wanted and none was given (docs/46 row 72).
    pub fn velocity_at(&self, r: DVec3) -> DVec3 {
        self.vel + self.ang_vel.cross(r)
    }

    /// Apply an impulse at a point offset `r`: it both pushes and turns.
    pub fn apply_impulse_at(&mut self, mass_kg: f64, r: DVec3, impulse: DVec3) {
        self.vel += impulse / mass_kg.max(1e-30);
        self.ang_vel += self.inv_inertia_world(mass_kg) * r.cross(impulse);
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

    rod.ang_vel += rod.ang_vel_from_impulse(mass_kg, extra_torque * dt);
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
        rod.ang_vel += rod.ang_vel_from_impulse(mass_kg, world * dt);
    }

    rod.centre += rod.vel * dt;
    rod.spin(dt);

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
        rod.centre += hit.dpos;
        // The constraint says what the FOOT should now be doing. Distributing that between travelling
        // and turning needs the effective mass at the foot: `Δv = K·J`, so `J = K⁻¹·Δv`. A point far
        // out on a light blade is easier to move than the blade's mass suggests, because the body can
        // rotate out of the way — using `m` here instead would over-brake it.
        let k = rod.effective_mass_at(mass_kg, arm);
        let impulse = k.inverse() * (hit.vel - v_foot);
        rod.apply_impulse_at(mass_kg, arm, impulse);
    }
}

/// ★ **THE RELEASE — one owner.** `settle_traced` drops members with these positions and
/// orientations, and a test that wants to know what a member was given must ask HERE rather than
/// rebuild the seeded draw for itself. It was briefly rebuilt in a test, which is a second
/// implementation of the release and would have drifted the first time either changed.
pub fn release_rods(member: &Assembly, count: usize, seed: u64) -> Option<Vec<Rod>> {
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
        };
        // Only against the CURRENT forkful: earlier ones are already on the heap and out of the way.
        let (c0, c1) = cand.ends();
        let clash = rods[forkful_start..].iter().any(|o| {
            let (o0, o1) = o.ends();
            let (pa, pb) = closest_points(c0, c1, o0, o1);
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
    // Air density at the pile, kg/m³ — **0.0 is a vacuum**. The Earth assembly supplies this; the Moon
    // supplies zero. A parameter and not a constant because whether there is air is a property of
    // where the pile IS, not of piles.
    air_density_kgm3: f64,
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
    // Not bound for use here — `release_rods` reads the cross-section itself. Called for its REFUSAL:
    // a member with no cross-section is not a rod, and `settle` should decline rather than build a
    // pile of degenerate ribbons that present no area to the air.
    cross_section_for(member)?;
    let member_mass = member.mass_kg(mats).ok()?.max(1e-12);
    let contact = crate::granular::contact_from_material(m, radius, member_mass);

    // Released over a disc a few lengths across, stacked upward so they fall rather than start merged.
    // ★★ **DROPPED IN ONE PLACE**, which is Robin's own wording and is not a detail: a heap's packing
    // is a property of straw only if the heap forms by its OWN repose. MEASURED the other way first —
    // released over a disc wider than a blade is long, 400 blades settled at 0.0005, which is a
    // measurement of how thinly they were scattered rather than of how they pack. A point source lets
    // the pile spread to the angle the contact law gives it.
    let rods_v = release_rods(member, count, seed)?;
    let mut rods: Vec<Rod> = rods_v;

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
            // ★ Not thrown on yet: a member waiting for its forkful is not in the world, so it neither
            // falls nor collides. Skipping it is what makes the release a SEQUENCE rather than a cloud.
            if rods[i].release_t_s > elapsed_s {
                continue;
            }
            let mut acc = DVec3::ZERO;
            let mut torque = DVec3::ZERO;
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
                let (pa, pb) = closest_points(a0, a1, b0, b1);
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
                let a_n = crate::granular::contact_accel(pa, va, pb, vb, &contact);
                acc += a_n;
                // ★ THE NEIGHBOUR'S MOMENT ARM. The contact happens at `pa`, which is somewhere along
                // this rod, not at its centre — so it both pushes and TURNS. Discarding the arm is what
                // made a landing blade slide instead of topple onto the heap.
                torque += arm_a.cross(a_n * member_mass);
            }
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
            );
        }
        elapsed_s += dt;
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
                peak_ang_speed_rads: rods
                    .iter()
                    .map(|r| r.ang_vel.length())
                    .fold(0.0f64, f64::max),
                peak_surface_speed_ms: rods
                    .iter()
                    .map(|r| r.max_surface_speed_ms())
                    .fold(0.0f64, f64::max),
                contacting_fraction: {
                    let touch = 2.0 * contact.radius + contact.coh_range;
                    let mut n_touch = 0usize;
                    for (i, r) in rods.iter().enumerate() {
                        let (a0, a1) = r.ends();
                        let floor = a0.y.min(a1.y) <= contact.radius;
                        let neigh = rods.iter().enumerate().any(|(j, o)| {
                            if i == j {
                                return false;
                            }
                            let (b0, b1) = o.ends();
                            let (pa, pb) = closest_points(a0, a1, b0, b1);
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

    /// ★★★ **THE QUANTITY THAT MUST FALL IS ENERGY, NOT ANGULAR SPEED** (docs/46 row 73, corrected).
    ///
    /// A body with anisotropic inertia can spin FASTER while losing energy, because `E = ½Iω²` and the
    /// axial moment of a blade is ~10⁴ below the others: the same joules parked in the soft mode buy
    /// ~100× the angular speed. Measured — a blade whose `|ω|` ran 20 → 290 rad/s had **27% of its
    /// starting energy left**, and the sweep showed it converging as `dt` fell 128×, so it was never an
    /// instability at all. Asserting on `|ω|` called correct physics a blow-up and sent me to revert it.
    fn mechanical_energy_j(rod: &Rod, mass_kg: f64, g: f64) -> f64 {
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

    /// Air at the bottom of Earth's atmosphere, from the catalogue's own `air` and the standard
    /// sea-level state — NOT a typed 1.225. The Earth assembly is what supplies air to anything
    /// standing on it, and a haystack is standing on it.
    fn sea_level_air(mats: &[Material]) -> f64 {
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
                    println!(
                        "  {n:3} blades -> packing {:.5} · quiet {} after {:.3} s · peak centre {:.6} m/s",
                        s.packing, s.quiet, s.elapsed_s, s.peak_speed_ms
                    );
                    // ★ The gauge asks about the fastest POINT, so report what it is actually seeing:
                    // a heap can be translationally dead and still be turning.
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
