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

// -----------------------------------------------------------------------------------------------------
// THE CORE RESOLUTION CONTROLLER (docs/13, docs/44 section 1) — camera-driven resolution as a default
// engine feature, not a per-scene frill. Every scene holds one and queries it per region; the controller
// alone decides whether matter is resolved into particles and how FINE those particles are.
//
// The two axes, and the ONE rule that keeps them honest (docs/44 section 1, docs/30):
//   - CAMERA drives GRANULARITY. How finely to resolve is a screen-space question: a grain finer than one
//     projecting to the angular-resolution threshold at the camera's distance is sub-pixel and buys no
//     visible fidelity (docs/13: cost scales with what is observable). Closer camera => finer grains.
//   - NECESSITY drives EXISTENCE. WHETHER a physical response happens is a physics question, decided by
//     the admission test above — an unwatched wheel still sinks; an off-camera crater still forms
//     (docs/30: the simulate-trigger is "a physical error bound, never a visual one").
//   - They COMPOSE, and the camera may only REFINE, never gate. resolve = necessity OR camera-close; the
//     granularity is the finer of what each axis needs. Letting the camera gate existence — so looking
//     away changes what is true — is the charter violation this controller exists to prevent.
// -----------------------------------------------------------------------------------------------------

/// A region's inputs to the resolution decision. All SI.
#[derive(Clone, Copy, Debug)]
pub struct RegionQuery {
    /// Distance from the camera to the region (m). Sets the camera-granularity term.
    pub distance_to_camera: f64,
    /// Depth the resolution is physically NECESSARY to (m), from [`admission_depth`] (quasi-static) or
    /// the impact footprint (`damage.rs`). `> 0` => physics demands resolution here, watched or not.
    pub necessity_depth: f64,
    /// The grain radius the active interaction NEEDS (m), from [`crate::granular::grain_radius_for`] — a
    /// tyre patch ~1 cm, ejecta ~metres. `None` when no interaction constrains granularity (then it is
    /// purely camera-driven).
    pub interaction_grain: Option<f64>,
}

/// What the controller decided for a region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolutionDecision {
    /// Resolve this region into particles at all?
    pub resolve: bool,
    /// If `resolve`, the grain RADIUS to resolve at (m); `0.0` when not resolving.
    pub grain_radius: f64,
}

/// The engine-level resolution controller. Default-constructed by every scene; the fidelity knob
/// (`angular_resolution`) is a declared setting, like render resolution — NOT a physics fudge.
#[derive(Clone, Copy, Debug)]
pub struct ResolutionController {
    /// Angular size (rad) below which detail is imperceptible — the screen-space threshold. A grain
    /// subtending less than this at the camera is sub-pixel. This is the one legitimate fidelity/cost
    /// dial (coarser => cheaper, blurrier); it declares a viewing tolerance, not a physical quantity.
    pub angular_resolution: f64,
    /// The grain radius the BULK model (voxel heightfield / T0 field) effectively represents (m). Detail
    /// finer than this is where resolving into particles begins to add visible or physical fidelity;
    /// coarser than this, the bulk model already is the answer. For the 1 m voxel terrain, ~0.5.
    pub bulk_grain_radius: f64,
    /// The finest grain the engine will resolve to (m) — a hard floor so a camera at the surface cannot
    /// demand infinitely fine grains. A resolution IOU: the true floor is the material's own structure.
    pub min_grain_radius: f64,
}

impl Default for ResolutionController {
    fn default() -> Self {
        // ~1 milliradian: about one pixel across a 60-degree field at ~1000 px, a deliberately
        // conservative (fine) default so nothing visibly under-resolves; scenes may coarsen it to trade
        // fidelity for cost. bulk = the 1 m terrain voxel (radius 0.5). floor = 1 mm.
        ResolutionController {
            angular_resolution: 1.0e-3,
            bulk_grain_radius: 0.5,
            min_grain_radius: 1.0e-3,
        }
    }
}

impl ResolutionController {
    /// The coarsest grain radius (m) that still looks right at `distance` — the screen-space bound. A
    /// grain of this radius subtends `angular_resolution` at the camera; anything finer is sub-pixel.
    /// Linear in distance (docs/13): twice as far => twice as coarse is acceptable. Floored at
    /// `min_grain_radius`.
    pub fn camera_grain_radius(&self, distance: f64) -> f64 {
        (distance.max(0.0) * self.angular_resolution).max(self.min_grain_radius)
    }

    /// **The decision.** Composes the two axes with the camera-may-only-refine rule.
    pub fn decide(&self, q: RegionQuery) -> ResolutionDecision {
        let must_exist = q.necessity_depth > 0.0; // physics demands it — watched or not
        let cam_grain = self.camera_grain_radius(q.distance_to_camera);
        // The camera adds VISUAL resolution only where the bulk model is too coarse to look right from
        // here — i.e. where a camera-acceptable grain is finer than the bulk already represents.
        let visible_detail = cam_grain < self.bulk_grain_radius;

        if !must_exist && !visible_detail {
            // Far enough that bulk looks right, and no physical need: the cheap model IS the answer.
            return ResolutionDecision { resolve: false, grain_radius: 0.0 };
        }

        // Resolve. Granularity = the FINER of what each axis needs (satisfying both), never finer than the
        // floor nor coarser than the bulk. Necessity pins granularity to the PHYSICS need even when the
        // camera is far (an unwatched interaction resolves at the scale the physics requires), so
        // `interaction_grain` is honoured regardless of `visible_detail`; the camera term only makes it
        // FINER when close.
        let mut grain = self.bulk_grain_radius;
        if let Some(g) = q.interaction_grain {
            grain = grain.min(g);
        }
        if visible_detail {
            grain = grain.min(cam_grain);
        }
        grain = grain.clamp(self.min_grain_radius, self.bulk_grain_radius);
        ResolutionDecision { resolve: true, grain_radius: grain }
    }
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

    // ---- the core resolution controller ----

    /// **Camera drives granularity — the screen-space bound (docs/13).** A grain finer than one
    /// subtending the angular threshold is sub-pixel; the acceptable grain grows linearly with distance.
    #[test]
    fn camera_granularity_is_the_screen_space_bound() {
        let c = ResolutionController::default();
        let near = c.camera_grain_radius(2.0);
        let far = c.camera_grain_radius(200.0);
        assert!(far > near, "twice as far must tolerate a coarser grain");
        assert!((far / near - 100.0).abs() < 1.0e-6, "grain is LINEAR in distance (100x farther, 100x)");
        assert!((near - 2.0 * c.angular_resolution).abs() < 1.0e-12, "grain = distance * angular_res");
        // The floor holds at the surface: a camera at zero distance cannot demand infinitely fine grains.
        assert_eq!(c.camera_grain_radius(0.0), c.min_grain_radius);
    }

    /// **THE invariant: the camera may REFINE but never GATE existence (docs/44 section 1, docs/30).**
    /// An unwatched region where physics demands resolution STILL resolves — looking away must not change
    /// what is true. This is the charter rule the whole controller exists to enforce.
    #[test]
    fn necessity_resolves_even_when_the_camera_is_infinitely_far() {
        let c = ResolutionController::default();
        // A wheel is sinking (necessity_depth > 0) but the camera is 100 km away and the region is far
        // below the visible-detail threshold — bulk would look fine from here.
        let d = c.decide(RegionQuery {
            distance_to_camera: 100_000.0,
            necessity_depth: 0.1, // physics demands resolution
            interaction_grain: Some(0.01), // a ~1 cm contact patch
        });
        assert!(d.resolve, "an unwatched sinking wheel MUST still resolve — the camera cannot gate physics");
        // And it resolves at the PHYSICS granularity (1 cm), not the coarse camera grain, because
        // necessity pins granularity to what the interaction needs regardless of viewpoint.
        assert!((d.grain_radius - 0.01).abs() < 1.0e-9, "unwatched necessity resolves at the physics scale");
    }

    /// The null case (docs/44 section 7), now for the controller: far away, no physical need, bulk looks
    /// right — resolve NOTHING. The cheap half of the whole idea, and it must be exactly free.
    #[test]
    fn far_and_unnecessary_resolves_nothing() {
        let c = ResolutionController::default();
        let d = c.decide(RegionQuery {
            distance_to_camera: 10_000.0, // camera grain here (10 m) >> bulk (0.5 m): bulk looks fine
            necessity_depth: 0.0,
            interaction_grain: None,
        });
        assert!(!d.resolve, "no necessity and bulk-looks-right ⇒ the bulk model IS the answer");
        assert_eq!(d.grain_radius, 0.0);
    }

    /// Camera-only resolution: no physics need, but the camera is close enough that the bulk model is
    /// visibly too coarse. Resolve for VISUAL fidelity, at the camera granularity — and finer as it nears.
    #[test]
    fn a_close_camera_resolves_for_visual_fidelity_and_refines_with_proximity() {
        let c = ResolutionController::default();
        let q = |dist: f64| c.decide(RegionQuery {
            distance_to_camera: dist,
            necessity_depth: 0.0,
            interaction_grain: None,
        });
        // At 100 m the camera grain (0.1 m) is finer than bulk (0.5 m) ⇒ resolve for visual detail.
        let mid = q(100.0);
        assert!(mid.resolve && (mid.grain_radius - 0.1).abs() < 1.0e-9, "visual resolve at camera grain");
        // Closer ⇒ finer.
        let close = q(10.0);
        assert!(close.grain_radius < mid.grain_radius, "moving closer must refine the grain");
        // Right at the surface, the floor caps it — not infinitely fine.
        assert!(q(0.0).grain_radius >= c.min_grain_radius);
    }

    /// **Composition = the FINER of the two axes.** When both a physics interaction and a close camera
    /// constrain granularity, the result must satisfy BOTH — i.e. be as fine as the stricter one.
    #[test]
    fn granularity_is_the_finer_of_camera_and_physics() {
        let c = ResolutionController::default();
        // Physics wants 1 cm; camera at 100 m tolerates 10 cm. The finer (1 cm) must win.
        let physics_stricter = c.decide(RegionQuery {
            distance_to_camera: 100.0,
            necessity_depth: 0.05,
            interaction_grain: Some(0.01),
        });
        assert!((physics_stricter.grain_radius - 0.01).abs() < 1.0e-9, "the 1 cm physics need wins");
        // Physics wants 20 cm; camera at 5 m tolerates 5 mm. Now the CAMERA is stricter and wins.
        let camera_stricter = c.decide(RegionQuery {
            distance_to_camera: 5.0,
            necessity_depth: 0.05,
            interaction_grain: Some(0.20),
        });
        assert!(camera_stricter.grain_radius < 0.20, "a close camera refines below the physics need");
        assert!((camera_stricter.grain_radius - c.camera_grain_radius(5.0)).abs() < 1.0e-9);
    }

    /// Grain is never coarser than the bulk (resolving coarser than the bulk buys nothing) and never
    /// finer than the floor — the two clamps that keep the decision bounded.
    #[test]
    fn granularity_stays_between_the_floor_and_the_bulk_scale() {
        let c = ResolutionController::default();
        // A huge interaction grain must not produce a grain coarser than the bulk.
        let coarse = c.decide(RegionQuery {
            distance_to_camera: 200.0,
            necessity_depth: 0.05,
            interaction_grain: Some(100.0),
        });
        assert!(coarse.resolve && coarse.grain_radius <= c.bulk_grain_radius + 1.0e-12);
        // A camera pressed to the surface cannot go below the floor.
        let fine = c.decide(RegionQuery {
            distance_to_camera: 0.0,
            necessity_depth: 0.05,
            interaction_grain: Some(1.0e-9),
        });
        assert!(fine.grain_radius >= c.min_grain_radius - 1.0e-15);
    }
}
