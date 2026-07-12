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
    let e = e.clamp(1.0e-3, 0.999);
    let l = -e.ln();
    let zeta = l / (std::f64::consts::PI.powi(2) + l * l).sqrt();
    2.0 * zeta * stiffness.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
