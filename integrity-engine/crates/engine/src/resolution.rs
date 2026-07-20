//! **Resolution by necessity** (`docs/44`) — the admission test that decides WHERE matter must be
//! resolved into particles, sized from the interaction's own physics rather than a chosen radius.
//!
//! The rule (docs/44 §3): resolve only where the imposed stress reaches the material's own yield, and
//! nowhere else — because everywhere else the cheap model (rigid heightfield / bulk field) is not an
//! approximation, it is the *correct* answer, since the material provably cannot move. The test's main
//! job is to say **no**: a car on basalt never approaches yield, so its honest footprint is zero
//! particles.
//!
//! This module is the QUASI-STATIC regime (docs/44 §4b) — a resting/rolling load, the case that unlocks
//! a vehicle and which "does not exist" per the docs/44 §8 status table. The impulsive regime (impact,
//! blast) already lives in `damage.rs` (`V = E/σ`); the two are the same principle for different loads.
//!
//! **Honesty flags carried from docs/44 §4b:** the Boussinesq field is an elastic-half-space result and
//! granular media are not elastic half-spaces. It is used ONLY as a conservative *sizing envelope* for
//! how much to resolve, never as a force law — the forces stay `granular::contact_accel` +
//! `terrain_contact_resolve`. Its permitted error mode is over-estimation (docs/44 §5): under-resolving
//! silently loses physics, over-resolving only costs frame time, so every threshold biases toward
//! resolving.

/// **Boussinesq axial stress** under the centre of a circular contact patch (`docs/44` §4b).
///
/// For a uniform pressure `p` over a patch of radius `a`, the vertical stress on the centreline at depth
/// `z` is `σ_z(z) = p · [1 − (1 + (a/z)²)^(−3/2)]`: full `p` at the surface, decaying with depth. Returns
/// stress in the same units as `p`.
pub fn boussinesq_axial_stress(p: f64, a: f64, z: f64) -> f64 {
    if z <= 0.0 {
        return p; // at/above the surface the full contact pressure acts
    }
    let ratio = a / z;
    p * (1.0 - (1.0 + ratio * ratio).powf(-1.5))
}

/// **The resolved depth `z*`** (metres): how deep the contact stress still reaches the material's yield.
///
/// `z*` is the root of `σ_z(z*) = σ_yield`. Above it the material can respond (resolve); below it the
/// stress is sub-yield and the material provably cannot move (leave it as bulk). Solved in closed form —
/// `σ_z` is monotone in `z`, so inverting the Boussinesq expression is exact, no iteration:
///
/// ```text
/// σ_z/p = 1 − (1 + (a/z)²)^(−3/2) = r   ⇒   (a/z)² = (1 − r)^(−2/3) − 1
/// ```
///
/// Returns **0 when the surface pressure itself is below yield** (`p ≤ σ_yield`): the load cannot move
/// this material at any depth, which is the rejection test — a car on basalt gets zero. `p`, `a` in SI;
/// `yield_stress` in Pa.
pub fn resolved_depth(p: f64, a: f64, yield_stress: f64) -> f64 {
    if p <= yield_stress || a <= 0.0 || yield_stress <= 0.0 {
        return 0.0; // sub-yield everywhere (max stress is p, at the surface) ⇒ nothing to resolve
    }
    let r = yield_stress / p;
    let inner = (1.0 - r).powf(-2.0 / 3.0) - 1.0;
    if inner <= 0.0 {
        return 0.0;
    }
    a / inner.sqrt()
}

/// **The quasi-static admission depth** — `resolved_depth` biased toward inclusion (`docs/44` §5).
///
/// Under-resolving loses physics silently; over-resolving only costs frame time. So the honest footprint
/// is expanded by a margin before it becomes a compute decision. The margin is one contact-patch radius
/// `a` (a correlation length of the loaded region), added to `z*` — a documented, conservative expansion,
/// not a tuned dial: shrinking it risks the silent failure, growing it only wastes compute. Returns 0
/// exactly when `resolved_depth` does, so the null case stays exactly free.
pub fn admission_depth(p: f64, a: f64, yield_stress: f64) -> f64 {
    let z = resolved_depth(p, a, yield_stress);
    if z <= 0.0 {
        0.0
    } else {
        z + a
    }
}

/// Contact pressure `p = P/A` (Pa) and patch radius `a = √(A/π)` (m) for a normal load `P` (N) spread
/// over patch area `A` (m²) — the inputs the admission test needs, from a vehicle's real weight
/// distribution and real contact patch. `(pressure, radius)`.
pub fn contact_patch(load_n: f64, area_m2: f64) -> (f64, f64) {
    if area_m2 <= 0.0 {
        return (0.0, 0.0);
    }
    (load_n / area_m2, (area_m2 / std::f64::consts::PI).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// docs/44 §4b's worked table, which is the specification. A ~1500 kg car, one wheel: P = 3679 N over
    /// A = 0.02 m². The three surfaces must land where the doc says — and basalt must be EXACTLY zero, not
    /// nearly, because the null case is the cheap half of the whole idea (docs/44 §7).
    #[test]
    fn the_worked_car_table_matches_docs44() {
        let (p, a) = contact_patch(1500.0 * 9.81 / 4.0, 0.02);
        assert!((p - 183_937.5).abs() < 1.0, "contact pressure {p:.0} Pa (doc ~184 kPa)");
        assert!((a - 0.0798).abs() < 1.0e-3, "patch radius {a:.4} m (doc ~0.080 m)");

        // Basalt: competent rock, p far below yield ⇒ resolve NOTHING, exactly.
        assert_eq!(resolved_depth(p, a, 10.0e6), 0.0, "a car on basalt must resolve zero particles");

        // Packed regolith (~100 kPa) and loose sand (~10 kPa): the doc's depths, to the millimetre.
        assert!((resolved_depth(p, a, 100.0e3) - 0.096).abs() < 2.0e-3, "regolith z* ≈ 0.096 m");
        assert!((resolved_depth(p, a, 10.0e3) - 0.409).abs() < 5.0e-3, "sand z* ≈ 0.409 m");
    }

    /// The monotone shape docs/44 §4b calls out: weaker material ⇒ deeper resolution, and the derivation
    /// is self-consistent — the depth `resolved_depth` returns is exactly where the stress equals yield.
    #[test]
    fn weaker_material_resolves_deeper_and_the_root_is_exact() {
        let (p, a) = contact_patch(3679.0, 0.02);
        let (soft, hard) = (resolved_depth(p, a, 10.0e3), resolved_depth(p, a, 150.0e3)); // both yields < p = 184 kPa
        assert!(soft > hard && hard > 0.0, "lower yield must resolve deeper: soft {soft}, hard {hard}");
        // At z*, the Boussinesq stress equals the yield it was solved for (the root is real).
        for y in [10.0e3, 50.0e3, 150.0e3] {
            let z = resolved_depth(p, a, y);
            assert!((boussinesq_axial_stress(p, a, z) - y).abs() < 1.0, "σ_z(z*) must equal yield {y}");
        }
    }

    /// The rejection test, stated as its own property (docs/44 §3: "the test is mostly a rejection
    /// test"). Any load whose surface pressure is at or below yield resolves nothing at all — there is no
    /// depth at which sub-yield stress can move the material.
    #[test]
    fn a_load_below_yield_resolves_nothing() {
        let (p, a) = contact_patch(500.0, 0.05); // a light load, p = 10 kPa
        assert_eq!(resolved_depth(p, a, p), 0.0, "at exactly yield ⇒ zero");
        assert_eq!(resolved_depth(p, a, p * 2.0), 0.0, "below yield ⇒ zero");
        assert_eq!(admission_depth(p, a, p * 2.0), 0.0, "and the margin does not resurrect it");
    }

    /// The inclusion margin (docs/44 §5) expands a real footprint but never a null one. Over-resolving
    /// is safe (only frame time); under-resolving is the silent failure the whole engine avoids.
    #[test]
    fn the_admission_margin_biases_toward_resolving_but_keeps_the_null_case_free() {
        let (p, a) = contact_patch(3679.0, 0.02);
        let bare = resolved_depth(p, a, 100.0e3);
        let admitted = admission_depth(p, a, 100.0e3);
        assert!(admitted > bare, "the margin must expand a real footprint (bias toward inclusion)");
        assert!((admitted - (bare + a)).abs() < 1.0e-9, "the margin is exactly one patch radius");
        assert_eq!(admission_depth(p, a, 10.0e6), 0.0, "but basalt is still exactly zero");
    }
}
