//! Granular contact — how loose grains push apart, stack, and flow (`docs/23`).
//!
//! Debris particles were resting on the terrain heightfield but ignoring *each other*: they could not
//! stack (everything piled into one layer — the "moiré") and could not slide down a crater wall to a
//! natural slope (a ring stranded on the rim). Both are the same missing physics: **particle-particle
//! contact**. This is the standard discrete-element (DEM) linear contact model — a soft repulsive
//! spring along the contact normal, damping that removes bounce, and **Coulomb friction** on the
//! tangential slip. Friction is the important part: it is what gives a pile its **angle of repose**,
//! and it is the same mechanism behind emergent static-vs-kinetic friction (`docs/23`) — grains at
//! rest settle and resist shear; grains sliding never settle. Nothing here is tuned per-object.
//!
//! The force is returned as an **acceleration** (the 1/m folded into the stiffness/damping constants):
//! the GPU step has no per-particle mass, so we model all debris as equal-mass grains — a documented
//! approximation, honest as long as it is flagged. Per-material mass is a later refinement.
//!
//! This module is the *physics of record* — the continuous contact **force law** (spring + damper +
//! Coulomb friction), verified natively. `shaders/particle_step.wgsl` integrates this *same* force law
//! on the GPU, but reorganizes it for stable integration (`docs/24` Stage 0): the normal DAMPING and the
//! spring are moved into a directional **implicit** solve — a per-grain stiffness/damping tensor
//! `S = Σ(dt²k + dt·c)(n⊗n)` — so stiff contacts at high coordination can't inject energy (explicit
//! damping overshoots and pumps energy once `Z·c·dt` nears 2). The GPU also clamps the friction impulse
//! at `|v_t|/dt` so friction can only halt a slip, never reverse it (another discrete anti-injection
//! guard). Both are integrator-level and agree with this force law as `dt → 0`; they are NOT new physics.
//! Net result, verified on real hardware (`tools/gpu-verify` scene I-flat): grain-grain contact
//! **conserves energy** (mechanical energy only falls). The one remaining energy injector is the lossy
//! heightfield terrain contact (min-translation normal flips at voxel edges) — the motivation for
//! terrain-as-matter (`docs/24`), not a granular-contact bug.

use crate::materials::Material;
use glam::DVec3;

/// Build the canonical contact parameters for a grain of the given `material`, `radius`, and mass — the
/// ONE place where "what the matter IS" becomes "how it collides". Stiffness from the real Young's
/// modulus, normal damping from the coefficient of restitution, friction + cohesion straight from the
/// material. The SAME `Contact`/`contact_accel` law then governs grains, debris, and planets — only the
/// material and the scale change, never the law (docs/23, docs/24; "get the small stuff right, apply
/// everywhere"). Per-mass form (the mass-agnostic model), so pass the grain's actual mass.
pub fn contact_from_material(mat: &Material, radius: f64, particle_mass: f64) -> Contact {
    let m = particle_mass.max(1.0e-30);
    // Linear soft-sphere stiffness from the elastic modulus: force k_f = E·r (N/m); as a per-mass
    // acceleration, k_f/m. Stiffer material or heavier grain ⇒ firmer contact — no tuned constant.
    let stiffness = (mat.youngs_modulus as f64 * radius) / m;
    let normal_damp = damping_for_restitution(mat.restitution as f64, stiffness);
    // Cohesion as per-mass adhesion σ·A/m (A = grain cross-section), capped for already-fractured debris.
    const GRANULAR_COHESION_CEIL: f64 = 5.0e4; // Pa — clay-level; loose-debris adhesion ceiling
    let area = std::f64::consts::PI * radius * radius;
    let cohesion = (mat.cohesion as f64).min(GRANULAR_COHESION_CEIL) * area / m;
    Contact {
        radius,
        stiffness,
        normal_damp,
        friction: mat.friction_coefficient as f64,
        tangent_damp: normal_damp, // regularise friction at the same scale as normal damping
        cohesion,
        coh_range: 0.15 * radius, // adhesion reaches a small fraction of a grain beyond touch
        shock: 0.0,               // solids: restitution-calibrated damping, no shock closure
    }
}

/// Contact parameters for equal-radius grains. All "stiffness/damping" are per-mass (accelerations),
/// so the model is mass-agnostic (see the module note).
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    /// Grain radius (m): two grains touch when their centres are closer than `2·radius`.
    pub radius: f64,
    /// Normal repulsion (1/s²): the penalty acceleration per metre of overlap. Stiffer ⇒ less
    /// interpenetration but a smaller stable timestep (ω = √stiffness).
    pub stiffness: f64,
    /// Normal damping (1/s): removes bounce by resisting the *approach* velocity. Contacts only ever
    /// push (never pull), so damping cannot turn into an attractive force.
    pub normal_damp: f64,
    /// Coulomb friction coefficient μ: the tangential force is capped at `μ · normal`. This cap is
    /// what produces the angle of repose (grains stop sliding once the slope shallows past ~atan μ).
    pub friction: f64,
    /// Tangential regularization (1/s): how sharply the friction force ramps with slip speed before it
    /// saturates at the Coulomb cap. Avoids a discontinuity at zero slip.
    pub tangent_damp: f64,
    /// Attractive ADHESION (1/s², per-mass) between touching grains — cohesion (`docs/24`). Full in
    /// contact, tapering to 0 over `coh_range` beyond touch. It lets a pile hold a slope a cohesionless
    /// pile can't, and gives a touching pair a real normal load (so friction — closing the zero-overlap
    /// graze). 0 ⇒ cohesionless (dry sand). Derived from `Material::cohesion` (capped for loose debris).
    pub cohesion: f64,
    /// Range (m) beyond `touch` over which cohesion acts before the bond lets go.
    pub coh_range: f64,
    /// SUB-PARCEL SHOCK closure (0..1; gases 1, solids 0): real shocks are far thinner than a particle,
    /// so at parcel LOD the ordered relative motion must thermalize within ONE crossing — which fixes
    /// the damping GEOMETRICALLY at c = |v_n|/(4·radius) (no tunable constant: c·τ_crossing ≈ ½ ⇒ the
    /// relative KE is dissipated in the pass). Same epistemic status as friction: a constitutive summary
    /// of sub-resolution physics. Side effect, also physical: the damping force becomes ∝ v² — the real
    /// ram-drag law of the hypersonic regime.
    pub shock: f64,
}

/// Momentum-conserving PLOUGHING LOFT (docs/24 + docs/28 step 3) — a primitive of the shared particle
/// physics, not an impact-scene special case (so a terrain meteor and a giant impact loft their excavated
/// matter through the SAME law). When a fast body ploughs into slower target matter, the excavation drags
/// that matter DOWNRANGE along the track — the mechanism that gives target material the near-orbital
/// tangential velocity to join a bound disk (why the Moon is Earth-derived). At feasible particle counts
/// the excavation shock is finer than a grain (docs/24 problem #1), so we declare its result HONESTLY —
/// as a *conserved momentum transfer*, never a scripted velocity: the TANGENTIAL (along-surface) momentum
/// is shared inelastically toward the common centre-of-mass velocity (the physical maximum drag,
/// co-motion — no free dial), and whatever the target gains, the impactor loses, so Σ(m·v) is EXACTLY
/// conserved. Only the along-track component is touched; the radial shock-rebound and gravity keep theirs.
///
/// `impactor[k]` marks which particles are the ploughing body (the rest are the excavated target). `n` is
/// the outward surface normal at the impact site; `v_contact` is the impactor's velocity relative to the
/// (co-moving) target — its along-surface part is the plough direction. A vertical strike (no tangential
/// component) is a no-op. Requires the target mass to be PHYSICAL (ρ·V): if the target is over-massed the
/// COM velocity collapses and nothing lofts — the reason this pairs with the docs/28 item-4 mass fix.
pub fn plough_loft(particles: &mut [crate::orbit::Body], impactor: &[bool], n: DVec3, v_contact: DVec3) {
    let tang = v_contact - n * v_contact.dot(n);
    let t = match tang.try_normalize() {
        Some(t) => t,
        None => return, // vertical incidence: no downrange plough
    };
    let (mut m_imp, mut p_imp, mut m_cap, mut p_cap) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (b, &imp) in particles.iter().zip(impactor) {
        let pt = b.mass * b.vel.dot(t);
        if imp {
            m_imp += b.mass;
            p_imp += pt;
        } else {
            m_cap += b.mass;
            p_cap += pt;
        }
    }
    if m_imp <= 0.0 || m_cap <= 0.0 {
        return;
    }
    // Common COM tangential velocity — the mass-weighted mean, so re-setting EVERY particle's tangential
    // component to it leaves Σ(m·v_t) unchanged (exact conservation), while dragging the (lighter) target
    // up toward the impactor's speed and slowing the impactor only slightly.
    let v_com = (p_imp + p_cap) / (m_imp + m_cap);
    for b in particles.iter_mut() {
        let vt = b.vel.dot(t);
        b.vel += t * (v_com - vt);
    }
}

impl Contact {
    /// The contact law for a pair of grains of DIFFERENT materials — "iron collides as iron, basalt as
    /// basalt" (docs/23: everything is matter; a Theia iron-core grain must not collide as bulk basalt).
    /// Each grain brings its OWN [`contact_from_material`] law; this mixes the two per pair, by construction
    /// reducing EXACTLY to a single material when both sides are identical (so same-material pairs — the
    /// whole terrain/debris path — are byte-unchanged, and only cross-material pairs differ):
    ///  • radius — arithmetic mean, so `touch = 2·radius = r_a + r_b` (the real sum of the two grain radii);
    ///  • stiffness — HARMONIC mean (two contacts in series; the softer material dominates the compliance);
    ///  • normal / tangential damping and friction — GEOMETRIC mean (the standard DEM cross-property mix);
    ///  • cohesion — the MINIMUM (a bond is only as strong as its weaker partner);
    ///  • cohesion range — arithmetic mean; shock closure — the max (a gas member makes the pass gaseous).
    /// All are symmetric and idempotent on equal inputs. (Restitution enters through `normal_damp`, which is
    /// already derived from each material's restitution in [`contact_from_material`].)
    pub fn mix(&self, o: &Contact) -> Contact {
        let hmean = |a: f64, b: f64| if a + b > 0.0 { 2.0 * a * b / (a + b) } else { 0.0 };
        let gmean = |a: f64, b: f64| (a * b).max(0.0).sqrt();
        Contact {
            radius: 0.5 * (self.radius + o.radius),
            stiffness: hmean(self.stiffness, o.stiffness),
            normal_damp: gmean(self.normal_damp, o.normal_damp),
            friction: gmean(self.friction, o.friction),
            tangent_damp: gmean(self.tangent_damp, o.tangent_damp),
            cohesion: self.cohesion.min(o.cohesion),
            coh_range: 0.5 * (self.coh_range + o.coh_range),
            shock: self.shock.max(o.shock),
        }
    }
}

/// Acceleration on grain *i* due to contact with grain *j* (equal radii). Zero unless they overlap.
/// Symmetric: grain *j* receives the negation from its own evaluation, so momentum is conserved.
#[inline]
pub fn contact_accel(pi: DVec3, vi: DVec3, pj: DVec3, vj: DVec3, c: &Contact) -> DVec3 {
    let d = pi - pj;
    let dist = d.length();
    let touch = 2.0 * c.radius;
    // Cohesion extends the interaction range beyond touch; beyond that the bond has let go.
    if dist >= touch + c.coh_range || dist < 1.0e-9 {
        return DVec3::ZERO; // not in contact (or coincident — no defined normal)
    }
    let n = d / dist; // unit normal, from j toward i
    let overlap = touch - dist; // >0 overlapping (compression); <0 separated but within cohesion range
    let v_rel = vi - vj;
    let v_n = v_rel.dot(n); // >0 separating, <0 approaching

    // Normal = repulsive spring (soft-sphere DEM, compression only) minus attractive ADHESION. NO force
    // cap — a cap is a fudge; a stiff contact is kept stable by implicit integration, not by clamping.
    // Cohesion lets the net force PULL (attractive) — bonding touching grains — until it breaks past
    // `coh_range`. c.cohesion = 0 recovers the old push-only contact.
    // Damping = the calibrated constant + the sub-parcel shock closure (see `Contact::shock`).
    let c_damp = c.normal_damp + c.shock * v_n.abs() / (4.0 * c.radius);
    let f_rep = if overlap > 0.0 {
        (c.stiffness * overlap - c_damp * v_n).max(0.0)
    } else {
        0.0
    };
    let sep = (-overlap).max(0.0); // separation beyond touch (0 while overlapping)
    // Adhesion, tapered over the range. Guard coh_range>0 so a cohesionless contact (coh_range=0) can't
    // divide 0/0 → NaN while overlapping.
    let f_coh = if c.cohesion > 0.0 && c.coh_range > 0.0 {
        c.cohesion * (1.0 - sep / c.coh_range).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let a_n = n * (f_rep - f_coh); // net: repulsion − attraction

    // Tangential (Coulomb friction): oppose the slip, never exceed μ·N where N is the real contact LOAD —
    // BOTH the repulsion and the adhesion press the surfaces, so cohesion raises the friction (apparent
    // cohesion), giving a touching pair friction even at zero compression. Regularized so it ramps smoothly.
    let normal_load = f_rep + f_coh;
    let v_t = v_rel - n * v_n;
    let vt_mag = v_t.length();
    let a_t = if vt_mag > 1.0e-9 {
        let mag = (c.tangent_damp * vt_mag).min(c.friction * normal_load);
        -(v_t / vt_mag) * mag
    } else {
        DVec3::ZERO
    };

    a_n + a_t
}

/// Specific DISSIPATED POWER (W/kg) of a contact pair — the mechanical energy the damping and friction
/// terms of `contact_accel` remove per second, per unit particle mass. Energy is conserved, not
/// destroyed (docs/20): what contact dissipation removes from motion MUST reappear as heat, and this is
/// the accounting for it — route it into the particles' temperatures (→ emergent incandescence: matter
/// struck hard glows because the impact heated it, not because anyone painted it orange).
/// Mirrors `contact_accel` exactly: normal term = (spring − realised force)·v_n (covers both the damper
/// and the push-only clamp, each ≥ 0), tangential term = |friction accel|·|slip|.
pub fn contact_dissipation(pi: DVec3, vi: DVec3, pj: DVec3, vj: DVec3, c: &Contact) -> f64 {
    let d = pi - pj;
    let dist = d.length();
    let touch = 2.0 * c.radius;
    if dist >= touch || dist < 1.0e-9 {
        return 0.0; // dissipation only in compression contact (cohesion is conservative)
    }
    let n = d / dist;
    let overlap = touch - dist;
    let v_rel = vi - vj;
    let v_n = v_rel.dot(n);
    let c_damp = c.normal_damp + c.shock * v_n.abs() / (4.0 * c.radius); // incl. shock closure
    let f_rep = (c.stiffness * overlap - c_damp * v_n).max(0.0);
    // Normal: the damping (or the push-only clamp) removes (k·overlap − f_rep)·v_n ≥ 0.
    let p_n = ((c.stiffness * overlap - f_rep) * v_n).max(0.0);
    // Tangential: Coulomb friction opposes the slip exactly — same load (repulsion + adhesion) as the
    // force law.
    let f_coh = if c.cohesion > 0.0 && c.coh_range > 0.0 { c.cohesion } else { 0.0 };
    let v_t = v_rel - n * v_n;
    let vt_mag = v_t.length();
    let p_t = if vt_mag > 1.0e-9 {
        (c.tangent_damp * vt_mag).min(c.friction * (f_rep + f_coh)) * vt_mag
    } else {
        0.0
    };
    p_n + p_t
}

/// Normal damping (1/s, per unit mass) that yields coefficient of restitution `e` for a linear
/// spring–dashpot contact of stiffness `k` (`docs/24` Stage 1). Invert the textbook relation
/// `e = exp(−ζπ/√(1−ζ²))` to `ζ = −ln e / √(π² + ln²e)` (ζ = fraction of critical damping), then
/// `c = 2ζ√k` (critical damping is `2√(km)`, m = 1 in the mass-agnostic model). So `e = 1` → `c = 0`
/// (perfectly elastic), and less-bouncy matter gets more damping. This makes how bouncy a contact is a
/// **material property**, not a dial — the source of truth is `Material::restitution`. NOTE: the stable
/// θ-solver in the shader adds a little numerical dissipation on top, so the realized restitution is
/// somewhat below `e` (a documented approximation, verified by the bounce test in `tools/gpu-verify`).
pub fn damping_for_restitution(e: f64, stiffness: f64) -> f64 {
    2.0 * zeta_for_restitution(e) * stiffness.sqrt()
}

/// The DAMPING RATIO ζ a material's coefficient of restitution implies, for a linear spring-dashpot
/// contact: `ζ = −ln(e) / √(π² + ln²e)`. This is the standard inversion of `e = exp(−ζπ/√(1−ζ²))`.
///
/// Factored out of [`damping_for_restitution`] (which now calls it) because ζ is needed on its own by
/// callers whose damping coefficient carries different units — e.g. `Aggregate::critically_damped`
/// works in force units and applies its own `√(k·m)` and coordination correction, whereas
/// `damping_for_restitution` returns a per-unit-mass coefficient (`granular`'s stiffness is an
/// acceleration per metre of overlap). Same physics, one implementation, so the two cannot drift.
pub fn zeta_for_restitution(e: f64) -> f64 {
    let e = e.clamp(1.0e-3, 0.999);
    let l = -e.ln();
    l / (std::f64::consts::PI.powi(2) + l * l).sqrt()
}

/// Result of resolving one grain against the terrain: the corrected velocity and the position delta.
#[derive(Clone, Copy, Debug)]
pub struct TerrainContact {
    /// Velocity after the constraint (into-surface component removed + Coulomb friction). Contact can
    /// only ever REMOVE kinetic energy from this — it never adds.
    pub vel: DVec3,
    /// Position correction along the surface normal (velocity-decoupled — writes no velocity, so it
    /// injects no kinetic energy however far the surface moved). Zero if not in contact.
    pub dpos: DVec3,
    /// True iff the grain was penetrating the surface (in contact).
    pub hit: bool,
}

/// NON-INJECTING terrain contact — the native **physics of record** for `particle_step.wgsl`'s
/// `terrain_resolve` (kept in sync by construction). The terrain is a per-column heightfield summarising
/// solid matter; a grain below the bilinear surface is in contact. Contact is resolved as a CONSTRAINT,
/// never a penalty spring, so it can NEVER increase a grain's kinetic energy — the fix for the settling
/// storm (a stiff penalty spring `F = k·pen` stored ½k·pen² and RELEASED it as launch KE ≈ √k·pen
/// whenever penetration appeared from a SURFACE change, e.g. a de-resolution deposit stepping a column up
/// under a resting neighbour). Given the grain state, the surface height `h` and its horizontal gradient
/// `(dhdx, dhdz)` at the grain, the grain half-extent, μ, the per-substep projection cap `max_corr`, and
/// the `headroom` to the nearest grain resting above (∞ for open sky), it returns:
///   1. NORMAL — the into-surface velocity is removed (clamped to ≥ 0), impulse `jn = max(0, −v·n)`.
///      Dissipative only; it also SUPPORTS a resting grain (the per-substep gravity increment is zeroed).
///   2. FRICTION — Coulomb, bounded by `μ·jn` (a harder-pressed grain gets more friction). Dissipative.
///   3. POSITION — a velocity-decoupled projection out of the surface, bounded by `max_corr` AND by the
///      `headroom` above (so a buried grain is never rammed up through the grains resting on it). No KE.
pub fn terrain_contact_resolve(
    pos: DVec3,
    vel: DVec3,
    h: f64,
    dhdx: f64,
    dhdz: f64,
    part_half: f64,
    mu: f64,
    max_corr: f64,
    headroom: f64,
) -> TerrainContact {
    let penetration = h - (pos.y - part_half);
    if penetration <= 0.0 {
        return TerrainContact { vel, dpos: DVec3::ZERO, hit: false };
    }
    let n = DVec3::new(-dhdx, 1.0, -dhdz).normalize(); // outward surface normal (continuous, never flips)
    let mut v = vel;
    // 1. Normal: remove into-surface velocity.
    let vn = v.dot(n);
    let mut jn = 0.0;
    if vn < 0.0 {
        jn = -vn;
        v += jn * n; // clamp into-surface component to 0 — dissipative, never a rebound
    }
    // 2. Friction: oppose tangential slip, bounded by μ·jn (kinetic Coulomb). Can only halt slip.
    let v_t = v - v.dot(n) * n;
    let vt_mag = v_t.length();
    if vt_mag > 1.0e-9 {
        let dv = (mu * jn).min(vt_mag);
        v -= (v_t / vt_mag) * dv;
    }
    // 3. Position projection out of the surface — velocity-decoupled, bounded (stack-safe).
    let dpos = penetration.min(max_corr).min(headroom) * n;
    TerrainContact { vel: v, dpos, hit: true }
}

/// **The heightfield's slope quantum** (metres). An integer voxel heightfield can represent a column
/// top only to whole voxels, so the shallowest NON-FLAT slope it can express over one 1 m cell is a
/// 1 m step — 45°. Every soil in the material DB reposes BELOW that (gravel 40°, sand 34°, dirt 29°),
/// so enforcing repose at a one-cell baseline with no allowance would force cohesionless terrain
/// perfectly FLAT — deleting the landscape rather than relaxing it.
///
/// This is that allowance, and it is a **resolution IOU** (`docs/24`), not a tuned dial: it is exactly
/// one voxel, the field's own quantisation, and the continuous sub-voxel surface (deferred part (A) of
/// the terrain-contact work) is what retires it. Note what it costs and what it does not: over a
/// baseline of `r` cells the tolerated excess is `1/r`, so the enforced angle is `atan(μ + 1/r)` and
/// converges on the true `atan μ` as a slope lengthens. Short faces are over-permitted by up to 45°;
/// SUSTAINED slopes — the ones that carry a landslide — are held to repose within `1/r`.
pub const SLOPE_QUANTUM_M: f32 = 1.0;

/// **Mohr–Coulomb slope stability** (`docs/45`) — how far a terrain face may stand above a neighbour
/// `r` metres away before it fails. This is the **φ term terrain never had**: shear strength is
/// `τ = c + σ·tan φ`, and terrain stability implemented only `c` (`h_crit = fracture_strength/(ρg)`),
/// hidden behind a constant `steep_drop = 3` that silently tolerated a 72° face. The grains have
/// carried φ all along — the Coulomb cap in [`Contact::friction`] IS what gives a pile its angle of
/// repose — so this makes ground and grain answer the slope question with ONE law.
///
/// Either term alone holds a face:
///
/// ```text
/// stable  ⇔  drop ≤ μ·r + QUANT      (friction: the slope is at or below repose, tan φ = μ)
///         ∨  drop ≤ c/(ρg)           (cohesion: a bank or a rock wall stands vertically)
/// ```
///
/// so the allowance is the MAX of the two — which is why cohesionless gravel (`h_crit = 0`) cannot
/// stand a vertical face at any height yet is perfectly stable as a 40° slope, while basalt
/// (`h_crit ≈ 510 m`) keeps its cliffs unchanged. Both are the physical answer, neither is tuned.
///
/// `mu` is the material's own DB datum (`Material::friction_coefficient`) and `r` the horizontal
/// baseline in metres. Returns the permitted drop, in metres.
pub fn repose_allowance(mu: f32, r: f32) -> f32 {
    mu * r + SLOPE_QUANTUM_M
}

/// **Is this face held up?** (`docs/45` §3) — the full Mohr–Coulomb test, and the reason the two terms
/// are an OR over *different measurements* rather than a max over one.
///
/// - **Friction** acts on the SLOPE, which is a property of the surface: `drop` over baseline `r`,
///   compared against [`repose_allowance`]. A veneer on a steep hillside slides if the hillside is
///   steeper than the veneer's repose angle, however thin the veneer is.
/// - **Cohesion** acts on the BANK this material must hold up by itself: `run_height`, the contiguous
///   vertical run of the *same* material in the exposed face — NOT the full drop to the neighbour.
///
/// Conflating those two heights is subtly wrong in exactly the case a layered world is made of. A 1 m
/// grass skin over basalt, on ground that steps down 2 m, is not a 2 m grass bank: grass holds its own
/// 1 m (`h_crit ≈ 1.09 m`) and the basalt below holds the rest (`h_crit ≈ 510 m`). Judging the grass
/// against the whole 2 m drop condemns a veneer that is in fact supported, and strips undisturbed
/// hillsides — measured, 470 grains shed from a world nothing had touched.
pub fn face_stable(drop: f32, r: f32, run_height: f32, mu: f32, h_crit: f32) -> bool {
    drop <= repose_allowance(mu, r) || run_height <= h_crit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repose_allowance_carries_the_friction_term() {
        // docs/45 §3. Gravel is cohesionless (h_crit = 0) with φ = 40°, so friction is the ONLY thing
        // holding it — the term terrain stability did not have. The allowance therefore grows with the
        // baseline at exactly tan φ, and its intercept is the field's quantum and nothing else.
        let mu = 0.84_f32; // the DB datum for gravel: tan 40°
        assert!((repose_allowance(mu, 1.0) - (mu + SLOPE_QUANTUM_M)).abs() < 1.0e-6);
        assert!((repose_allowance(mu, 4.0) - (4.0 * mu + SLOPE_QUANTUM_M)).abs() < 1.0e-6);

        // What the quantum costs, stated as a bound rather than hidden: the slope it permits starts
        // over-steep and converges on the material's real repose angle as the face lengthens. This is
        // the claim `SLOPE_BASELINE_CELLS` is chosen against.
        let phi = mu.atan().to_degrees();
        let angle_at = |r: f32| (repose_allowance(mu, r) / r).atan().to_degrees();
        assert!(angle_at(1.0) > 55.0, "one cell cannot resolve repose at all");
        assert!(
            angle_at(8.0) - phi < 4.0,
            "an 8-cell baseline holds gravel within 4° of its 40° repose, got {:.1}°",
            angle_at(8.0)
        );
        assert!(
            angle_at(8.0) > phi,
            "the allowance must never fall BELOW repose — that would slump stable ground"
        );
        assert!(angle_at(64.0) - phi < 0.6, "and it keeps converging on φ, not on some floor");
    }

    #[test]
    fn either_mohr_coulomb_term_alone_holds_a_face() {
        // Cohesion is the other half, and it is what keeps rock cliffs standing: basalt's h_crit ≈ 510 m
        // holds a 50 m vertical face with no help from friction whatsoever.
        assert!(face_stable(50.0, 1.0, 50.0, 0.7, 510.0), "a basalt cliff stands on cohesion alone");
        // Cohesionless gravel gets nothing from that term at any height — friction is all it has...
        assert!(!face_stable(4.0, 1.0, 4.0, 0.84, 0.0), "gravel cannot stand a 4 m vertical face");
        // ...but the SAME gravel is perfectly stable once the slope is at repose. Both are docs/45 §3.
        assert!(face_stable(6.0, 8.0, 6.0, 0.84, 0.0), "and is stable as a slope below its repose angle");
    }

    #[test]
    fn cohesion_judges_the_material_bank_not_the_whole_drop() {
        // The layered-world case, and the bug that made this a separate function. A 1 m grass skin over
        // basalt on ground that steps down 2 m: the grass must hold only ITS OWN 1 m run (h_crit ≈ 1.09 m),
        // not the 2 m drop — the basalt beneath holds that. Measuring cohesion against the full drop
        // strips veneers off hillsides nothing has disturbed.
        let (mu_grass, h_grass) = (0.7_f32, 1.09_f32);
        assert!(
            face_stable(2.0, 1.0, 1.0, mu_grass, h_grass),
            "a 1 m grass veneer on a 2 m step is supported by the rock under it"
        );
        // A real 2 m grass BANK is a different object and it fails, because now the run IS the drop.
        assert!(
            !face_stable(2.0, 1.0, 2.0, mu_grass, h_grass),
            "a free-standing 2 m grass bank exceeds grass's own critical height and slumps"
        );
    }

    #[test]
    fn contact_mix_is_idempotent_and_bounded() {
        // docs/23 per-grain material contact. The critical no-regression guarantee: mixing a material with
        // ITSELF returns exactly that material's law — so same-material pairs (the entire terrain/debris
        // path) are byte-unchanged and only cross-material contacts differ. And a mixed pair's parameters
        // must lie BETWEEN the two materials (no contact stiffer/bouncier than either constituent).
        let mats = crate::materials::load();
        let idx = |n| crate::materials::index_of(&mats, n);
        let (iron, basalt) = (&mats[idx("iron")], &mats[idx("basalt")]);
        let ci = contact_from_material(iron, 0.5, 1000.0);
        let cb = contact_from_material(basalt, 0.5, 1000.0);
        // Idempotent: mix(c, c) == c, field by field.
        let self_mix = ci.mix(&ci);
        assert!((self_mix.stiffness - ci.stiffness).abs() < 1e-6 * ci.stiffness, "stiffness self-mix");
        assert!((self_mix.normal_damp - ci.normal_damp).abs() < 1e-6 * ci.normal_damp.max(1.0), "damp self-mix");
        assert!((self_mix.friction - ci.friction).abs() < 1e-9, "friction self-mix");
        assert!((self_mix.radius - ci.radius).abs() < 1e-12, "radius self-mix");
        assert!((self_mix.cohesion - ci.cohesion).abs() < 1e-6 * ci.cohesion.max(1.0), "cohesion self-mix");
        // Symmetric: mix(a, b) == mix(b, a).
        let ab = ci.mix(&cb);
        let ba = cb.mix(&ci);
        assert!((ab.stiffness - ba.stiffness).abs() < 1e-9, "mix symmetric in stiffness");
        assert!((ab.friction - ba.friction).abs() < 1e-9, "mix symmetric in friction");
        // Bounded between the constituents (harmonic mean of stiffness lies between the two).
        let (lo, hi) = (ci.stiffness.min(cb.stiffness), ci.stiffness.max(cb.stiffness));
        assert!(lo <= ab.stiffness && ab.stiffness <= hi, "mixed stiffness between iron & basalt");
        let (flo, fhi) = (ci.friction.min(cb.friction), ci.friction.max(cb.friction));
        assert!(flo <= ab.friction && ab.friction <= fhi, "mixed friction between iron & basalt");
    }

    #[test]
    fn plough_loft_conserves_momentum_and_lofts_the_lighter_target() {
        use crate::orbit::Body;
        // docs/28 step 3: the momentum-conserving loft. A heavy impactor moving downrange (+x) at 6 km/s
        // ploughs a LIGHTER cap at rest. Total tangential momentum must be UNCHANGED (a transfer), the
        // impactor must slow, and the cap must be dragged toward the impactor's speed — the near-orbital
        // loft. With a light cap (physical ρ·V), the shared COM speed is CLOSE to the impactor's, not a
        // third of it (the over-massed-cap failure this fixes).
        let n = DVec3::Y; // surface normal
        let v_contact = DVec3::new(6_000.0, -6_000.0, 0.0); // 45° oblique; downrange = +x, v_t = 6 km/s
        let t = DVec3::X;
        // 2 impactor grains (mass 3) + 2 cap grains (mass 1): m_cap/m_imp = 1/3, like the physical cap.
        let mut ps = vec![
            Body { pos: DVec3::ZERO, vel: v_contact, mass: 3.0 },
            Body { pos: DVec3::new(0.0, 1.0, 0.0), vel: v_contact, mass: 3.0 },
            Body { pos: DVec3::new(1.0, -1.0, 0.0), vel: DVec3::ZERO, mass: 1.0 },
            Body { pos: DVec3::new(2.0, -1.0, 0.0), vel: DVec3::ZERO, mass: 1.0 },
        ];
        let imp = vec![true, true, false, false];
        let p_before: f64 = ps.iter().map(|b| b.mass * b.vel.dot(t)).sum();
        let imp_t0 = ps[0].vel.dot(t);
        plough_loft(&mut ps, &imp, n, v_contact);
        let p_after: f64 = ps.iter().map(|b| b.mass * b.vel.dot(t)).sum();
        assert!((p_after - p_before).abs() < 1e-6 * p_before.abs(), "tangential momentum conserved");
        // COM tangential speed = (6·6000 + 2·0)/8 = 4500. Both populations meet there.
        let v_com = (6.0 * 6_000.0) / 8.0;
        assert!((ps[0].vel.dot(t) - v_com).abs() < 1e-6, "impactor → v_com");
        assert!((ps[2].vel.dot(t) - v_com).abs() < 1e-6, "cap dragged to v_com");
        assert!(ps[0].vel.dot(t) < imp_t0, "impactor slows");
        assert!(ps[2].vel.dot(t) > 0.0, "cap lofted downrange");
        // Radial (y) untouched — only the along-track component couples.
        assert!((ps[0].vel.y - v_contact.y).abs() < 1e-9, "impactor radial untouched");
        assert!(ps[2].vel.y.abs() < 1e-9, "cap radial untouched");
        // Vertical strike ⇒ no-op.
        let mut vert = ps.clone();
        let b0 = vert.clone();
        plough_loft(&mut vert, &imp, n, DVec3::new(0.0, -6_000.0, 0.0));
        for (a, b) in vert.iter().zip(b0.iter()) {
            assert!((a.vel - b.vel).length() < 1e-9, "vertical strike: no plough");
        }
    }

    #[test]
    fn mixed_material_contact_conserves_momentum() {
        // A cross-material pair (iron grain overrunning a basalt grain) must exert equal-and-opposite
        // forces — the per-grain path uses the SAME symmetric contact_accel on the mixed law, so Σp is
        // conserved regardless of the materials. Overlap them and check the pair force sums to zero.
        let mats = crate::materials::load();
        let idx = |n| crate::materials::index_of(&mats, n);
        let ci = contact_from_material(&mats[idx("iron")], 0.5, 1000.0);
        let cb = contact_from_material(&mats[idx("basalt")], 0.5, 1000.0);
        let law = ci.mix(&cb);
        // Two grains overlapping (centres 0.9·touch apart), approaching.
        let (pi, pj) = (DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.9 * 2.0 * law.radius, 0.0, 0.0));
        let (vi, vj) = (DVec3::new(1.0, 0.0, 0.0), DVec3::new(-1.0, 0.0, 0.0));
        let fi = contact_accel(pi, vi, pj, vj, &law);
        let fj = contact_accel(pj, vj, pi, vi, &law);
        assert!((fi + fj).length() < 1e-9, "pair forces must be equal & opposite (got {fi:?} + {fj:?})");
        assert!(fi.x < 0.0, "the overtaken grain-i should be pushed back (−x), got {fi:?}");
    }

    #[test]
    fn restitution_damping_is_monotone_and_matches_the_calibration() {
        let k = 5.0e5;
        // Bouncier material ⇒ less damping. Perfectly elastic ⇒ zero damping.
        let c_bouncy = damping_for_restitution(0.8, k);
        let c_dead = damping_for_restitution(0.2, k);
        assert!(c_dead > c_bouncy, "less-bouncy matter is damped harder");
        assert!(damping_for_restitution(0.999, k) < 5.0, "≈elastic ⇒ ≈no damping");
        // Granite (e=0.80) reproduces the hand-calibrated c≈100 the contact was using — a sanity anchor.
        assert!(
            (c_bouncy - 100.0).abs() < 6.0,
            "granite e=0.80 ⇒ c≈100 (got {c_bouncy:.1})"
        );
    }

    fn params() -> Contact {
        #[allow(clippy::needless_update)]
        Contact {
            radius: 0.5,
            stiffness: 2.0e4,
            normal_damp: 140.0,
            friction: 0.6,
            tangent_damp: 200.0,
            cohesion: 0.0, // cohesionless by default (dry) — existing tests are the push-only contact
            coh_range: 0.15,
            shock: 0.0,
        }
    }

    #[test]
    fn cohesion_bonds_touching_grains_and_raises_friction() {
        // docs/24: cohesion is an ATTRACTIVE force between touching grains (0 for dry sand). It bonds a
        // just-touching pair (net force pulls them together) and adds to the friction normal load.
        let mut c = params();
        c.cohesion = 500.0;
        // Two grains EXACTLY touching (overlap 0): dry ⇒ no force; cohesive ⇒ net attraction (pulls in).
        let touching = |cohesion: f64| {
            let mut cc = params();
            cc.cohesion = cohesion;
            contact_accel(
                DVec3::new(1.0, 0.0, 0.0), // 1.0 apart = touch (radius 0.5)
                DVec3::ZERO,
                DVec3::ZERO,
                DVec3::ZERO,
                &cc,
            )
        };
        assert_eq!(touching(0.0), DVec3::ZERO, "dry: no force at zero overlap");
        assert!(touching(500.0).x < 0.0, "cohesive: net force pulls the pair together (−x)");

        // Cohesion raises the friction load: a grain sliding tangentially on a just-touching cohesive
        // contact feels friction even with zero compression (closing the frictionless graze).
        let sliding = |cohesion: f64| {
            let mut cc = params();
            cc.cohesion = cohesion;
            contact_accel(
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(0.0, 0.0, 2.0), // sliding in +z, no compression
                DVec3::ZERO,
                DVec3::ZERO,
                &cc,
            )
            .z
        };
        assert_eq!(sliding(0.0), 0.0, "dry graze at zero overlap is frictionless");
        assert!(sliding(500.0) < 0.0, "cohesive graze has friction opposing the slip");
    }

    #[test]
    fn separated_grains_do_not_interact() {
        let c = params();
        // 2.0 apart, touch distance is 1.0 → no contact.
        let a = contact_accel(
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        assert_eq!(a, DVec3::ZERO);
    }

    #[test]
    fn overlapping_grains_push_apart_along_the_normal() {
        let c = params();
        // Centres 0.8 apart (touch 1.0) → overlap 0.2, along +x. At rest ⇒ pure repulsion.
        let a = contact_accel(
            DVec3::new(0.8, 0.0, 0.0),
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        assert!(
            a.x > 0.0 && a.y.abs() < 1e-9 && a.z.abs() < 1e-9,
            "along +x"
        );
        assert!(
            (a.x - c.stiffness * 0.2).abs() < 1e-6,
            "repulsion = stiffness·overlap (got {})",
            a.x
        );
    }

    #[test]
    fn the_normal_force_is_a_linear_spring_no_cap() {
        // The repulsion is k·overlap — a real linear spring, NOT clamped. A cap was a fudge (it made a
        // deep contact under-push); stability is the integrator's job, not the force law's (docs/23).
        let c = params(); // stiffness 2e4
        let a = contact_accel(
            DVec3::new(0.2, 0.0, 0.0), // overlap 0.8
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        assert!(
            (a.x - 2.0e4 * 0.8).abs() < 1e-6,
            "force = stiffness·overlap, uncapped (got {})",
            a.x
        );
    }

    #[test]
    fn contacts_only_push_never_pull() {
        let c = params();
        // Overlapping but separating fast: damping term is large, but the force must not go negative
        // (a contact cannot pull two grains together).
        let a = contact_accel(
            DVec3::new(0.99, 0.0, 0.0),
            DVec3::new(100.0, 0.0, 0.0), // flying apart
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        assert!(a.x >= 0.0, "never attractive (got {})", a.x);
    }

    #[test]
    fn approaching_grains_feel_extra_damping() {
        let c = params();
        let at_rest = contact_accel(
            DVec3::new(0.8, 0.0, 0.0),
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        let approaching = contact_accel(
            DVec3::new(0.8, 0.0, 0.0),
            DVec3::new(-1.0, 0.0, 0.0), // moving toward j
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        assert!(
            approaching.x > at_rest.x,
            "approach adds damping resistance ({} vs {})",
            approaching.x,
            at_rest.x
        );
    }

    #[test]
    fn friction_opposes_slip_and_is_capped_by_the_normal_force() {
        let c = params();
        // Overlap along x; slip along z. Friction should be along −z and never exceed μ·normal.
        let a = contact_accel(
            DVec3::new(0.8, 0.0, 0.0),
            DVec3::new(0.0, 0.0, 5.0), // sliding in +z
            DVec3::ZERO,
            DVec3::ZERO,
            &c,
        );
        assert!(a.z < 0.0, "friction opposes the slip");
        let normal = c.stiffness * 0.2;
        assert!(
            a.z.abs() <= c.friction * normal + 1e-6,
            "friction capped at μ·normal ({} vs {})",
            a.z.abs(),
            c.friction * normal
        );
    }

    /// Integration check: a grain dropped onto a grain already resting on the floor comes to rest
    /// *stacked above it* — contact both stops interpenetration and holds the weight. This is the
    /// behaviour that was missing (debris could not stack).
    #[test]
    fn a_dropped_grain_settles_stacked_on_another() {
        let c = params();
        let g = -9.81;
        let floor = 0.0; // grains rest with centre at floor + radius
        let rest_y = floor + c.radius;
        // j sits on the floor; i starts a bit above, will fall and stack.
        let mut pi = DVec3::new(0.0, rest_y + 3.0, 0.0);
        let mut vi = DVec3::ZERO;
        let mut pj = DVec3::new(0.0, rest_y, 0.0);
        let mut vj = DVec3::ZERO;
        let dt = 1.0 / 3000.0; // fine step for the stiff contact
        for _ in 0..30_000 {
            let ai = DVec3::new(0.0, g, 0.0) + contact_accel(pi, vi, pj, vj, &c);
            let aj = DVec3::new(0.0, g, 0.0) + contact_accel(pj, vj, pi, vi, &c);
            vi += ai * dt;
            vj += aj * dt;
            pi += vi * dt;
            pj += vj * dt;
            // Floor: neither grain passes through it.
            for (p, v) in [(&mut pi, &mut vi), (&mut pj, &mut vj)] {
                if p.y < rest_y {
                    p.y = rest_y;
                    if v.y < 0.0 {
                        v.y = 0.0;
                    }
                }
            }
        }
        // i ends up resting ~one diameter above the floor, on top of j — not interpenetrating, not
        // passed through.
        let gap = pi.y - pj.y;
        assert!(
            gap > 1.5 * c.radius && gap < 2.1 * c.radius,
            "i is stacked ~one diameter above j (gap {gap:.3}, diameter {:.3})",
            2.0 * c.radius
        );
        assert!(pj.y < rest_y + 0.05, "j stays on the floor (y {:.3})", pj.y);
        assert!(
            vi.length() < 0.1 && vj.length() < 0.1,
            "both settle to rest"
        );
    }

    // ── Non-injecting terrain contact (the settling-storm fix) — native mirror of the GPU
    // `terrain_resolve`, verified on hardware in `tools/gpu-verify` (scenes K/L/N/O/I).

    #[test]
    fn terrain_surface_step_injects_no_kinetic_energy() {
        // A grain RESTING on flat terrain (surface height h0) has the surface step UP by Δ beneath it —
        // exactly what a de-resolution deposit does to a neighbour's bilinear surface. The OLD penalty
        // spring released ½k·Δ² as launch KE (≈√k·Δ ≈ 707·Δ m/s for k=5e5). The constraint must add ZERO
        // kinetic energy: the grain is reconciled to the risen surface by a velocity-decoupled projection.
        let part_half = 0.5;
        for &d in &[0.1f64, 0.25, 0.5, 1.0, 2.5] {
            // Grain at rest with its base exactly on the old surface (h0 = 0), velocity ~0.
            let pos = DVec3::new(0.0, part_half, 0.0);
            let vel = DVec3::ZERO;
            // Surface steps up to h = Δ under the (stationary) grain — flat, so zero gradient.
            let r = terrain_contact_resolve(pos, vel, d, 0.0, 0.0, part_half, 0.6, 0.01, f64::INFINITY);
            assert!(r.hit, "penetrating after the surface stepped up");
            let ke = 0.5 * r.vel.length_squared();
            assert!(
                ke < 1.0e-9,
                "surface step Δ={d} injected KE {ke:.3e} (a spring would give ½·(707·Δ)² ≈ {:.0})",
                0.5 * (707.0 * d as f64).powi(2)
            );
            // And the projection only ever pushes OUTWARD (never deeper), and never writes velocity.
            assert!(r.dpos.y > 0.0, "projection is outward (up)");
        }
    }

    #[test]
    fn terrain_supports_a_resting_grain_without_launch_or_sink() {
        // A grain pressed into the surface by the per-substep gravity increment: the constraint zeroes the
        // into-surface velocity (support — it does not sink) and does not launch it (no rebound).
        let part_half = 0.5;
        let dt = 1.0 / 960.0;
        let g = 9.81;
        // Grain sitting a hair below the surface (as a resting grain does — gravity pulls it down each
        // substep) with the downward velocity gravity added this substep. h=0.
        let pos = DVec3::new(0.0, part_half - g * dt * dt, 0.0);
        let vel = DVec3::new(0.0, -g * dt, 0.0);
        let r = terrain_contact_resolve(pos, vel, 0.0, 0.0, 0.0, part_half, 0.6, 0.01, f64::INFINITY);
        assert!(r.hit, "a resting grain is in contact (slightly penetrating)");
        assert!(r.vel.y >= 0.0, "into-surface velocity removed (supported, y-vel {:.4})", r.vel.y);
        assert!(r.vel.y < 1.0e-9, "not launched upward (y-vel {:.4})", r.vel.y);
        assert!(r.dpos.y > 0.0, "projected back up to the surface (does not sink)");
    }

    #[test]
    fn terrain_contact_energy_is_monotone_non_increasing_on_a_drop() {
        // Integrate a grain FALLING onto flat terrain over many substeps (gravity + the constraint) and
        // assert total mechanical energy (KE + g·y) only ever DECREASES — the constraint never manufactures
        // energy. This is the native analogue of gpu-verify scene I (the fudge detector).
        let part_half = 0.5;
        let dt = 1.0 / 960.0;
        let g = 9.81;
        let mut pos = DVec3::new(0.0, 8.0, 0.0); // released from 8 m up
        let mut vel = DVec3::ZERO;
        let energy = |p: DVec3, v: DVec3| g * p.y + 0.5 * v.length_squared();
        let mut e_prev = energy(pos, vel);
        for _ in 0..4000 {
            // gravity
            vel.y -= g * dt;
            pos += vel * dt;
            // terrain constraint (flat surface at y=0)
            let r = terrain_contact_resolve(pos, vel, 0.0, 0.0, 0.0, part_half, 0.6, 0.01, f64::INFINITY);
            if r.hit {
                vel = r.vel;
                pos += r.dpos;
            }
            let e = energy(pos, vel);
            // Non-increase, with a tiny slack for the projection's PE bookkeeping (the projection lifts the
            // grain to the surface it fell to — never above where it fell from, so no net gain) and f64 noise.
            assert!(
                e <= e_prev + 1.0e-6,
                "terrain contact injected energy: {e_prev:.6} → {e:.6}"
            );
            e_prev = e;
        }
        // It came to rest supported at the surface (base ~0 ⇒ centre ~part_half), not sunk, not launched.
        assert!((pos.y - part_half).abs() < 0.05, "rests at the surface (y {:.4})", pos.y);
        assert!(vel.length() < 0.05, "settled (speed {:.4})", vel.length());
    }
}
