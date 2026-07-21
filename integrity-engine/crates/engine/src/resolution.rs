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

use glam::Vec3;
use crate::materials::Material;

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
    /// Distance from the camera to the region (m). Sets the camera-granularity term when Resolved.
    pub distance_to_camera: f64,
    /// Is this region within the camera's view? The scene computes it (frustum + occlusion). This is
    /// what chooses MATH vs SIMULATION for active physics — NOT whether the physics happens. An effect
    /// that was Analytic off-view flips to Resolved the frame it enters view (docs/49): "render the
    /// effects as/when they come into view".
    pub in_view: bool,
    /// Depth the resolution is physically NECESSARY to (m), from [`admission_depth`] (quasi-static) or
    /// the impact footprint (`damage.rs`), OR the marker of an analytically-propagated effect present
    /// here. `> 0` => ACTIVE PHYSICS here, watched or not — existence, camera-independent.
    pub necessity_depth: f64,
    /// The grain radius the active interaction NEEDS (m), from [`crate::granular::grain_radius_for`] — a
    /// tyre patch ~1 cm, ejecta ~metres. `None` when no interaction constrains granularity (then it is
    /// purely camera-driven).
    pub interaction_grain: Option<f64>,
}

/// How a region is computed and shown (docs/49). Three regimes, chosen by ACTIVE-PHYSICS × IN-VIEW:
/// existence is decided by the physics (necessity), the camera only chooses math vs simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResolutionMode {
    /// No active physics: the cheap bulk model IS the answer (rendered at whatever LOD the camera wants).
    Bulk,
    /// Active physics that is NOT in view: compute it with MATH (analytic/declared propagation — e.g.
    /// `docs/28` giant-impact ejection), no particles, no render. The Moon slamming the far side of the
    /// planet is here: energy known, effects propagated cheaply until they reach the camera's view.
    Analytic,
    /// Active physics IN VIEW: particle SIMULATION + render, at the camera-appropriate granularity.
    Resolved { grain_radius: f64 },
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

    /// **The decision** (docs/49). Existence is the physics'; the camera only chooses HOW to compute it.
    ///   - no active physics                     => `Bulk`
    ///   - active physics, NOT in view           => `Analytic` (cheap math; camera does NOT gate existence)
    ///   - active physics, in view               => `Resolved` at the camera-appropriate granularity
    pub fn decide(&self, q: RegionQuery) -> ResolutionMode {
        let active = q.necessity_depth > 0.0; // existence — watched or not
        if !active {
            return ResolutionMode::Bulk;
        }
        if !q.in_view {
            // The far-side impact: real, but nobody is looking. Compute it with math and propagate its
            // effects; simulate none of it. This is the win — math is far cheaper than particles, and the
            // camera legitimately chooses it WITHOUT gating whether the physics happens.
            return ResolutionMode::Analytic;
        }
        // In view AND active: simulate. Granularity is the FINER of what the interaction needs and what
        // the camera can resolve at this distance — satisfying both — clamped to [floor, bulk].
        let mut grain = self.bulk_grain_radius;
        if let Some(g) = q.interaction_grain {
            grain = grain.min(g);
        }
        grain = grain.min(self.camera_grain_radius(q.distance_to_camera));
        grain = grain.clamp(self.min_grain_radius, self.bulk_grain_radius);
        ResolutionMode::Resolved { grain_radius: grain }
    }
}

// -----------------------------------------------------------------------------------------------------
// THE CENTRAL RESOLUTION FIELD (docs/49) — the ONE system, for every scene and every scale, that makes the
// Analytic -> Resolved hand-off an inherent property of the engine.
//
// A scene registers the active physics it knows about as ANALYTIC EFFECTS — a region carrying a material
// and a granularity, propagated by cheap math while off-camera (the Moon's ejecta arcing over the far
// horizon, a shock front, a distant landslide). Once per step the scene calls `update()`; the field
// advances every effect analytically, asks the ONE `ResolutionController` per effect, and — the frame an
// effect enters view — materialises it through the ONE shared materialisation, `MatterSim::
// materialize_region`, the exact call the meteor already uses.
//
// THERE IS NO PER-BACKEND ADAPTER, deliberately. The engine's promise is one particle system, one contact
// law, one materialisation, differing only in scale (docs/23, docs/46) — the forked particle containers
// (docs/32 §4) are the violation to be unified onto this path, NOT a fact to design around. So the field
// resolves straight into `MatterSim`; a scene on a different container is a scene that has not yet
// converged (docs/33), not a reason to add a second resolution path here.
// -----------------------------------------------------------------------------------------------------

/// A physical process tracked ANALYTICALLY until it must be resolved — the cheap representation of active
/// physics that is real but not (yet) visible (docs/49). Scale-free: `radius`/`grain_radius` are whatever
/// the interaction is, from ejecta metres to a tyre patch's centimetres.
#[derive(Clone, Copy, Debug)]
pub struct Effect {
    /// Region centre (centred world coords).
    pub center: Vec3,
    /// Analytic propagation velocity (e.g. ballistic). Advanced by the per-step acceleration.
    pub velocity: Vec3,
    /// Region radius (m) — how much matter this effect will materialise.
    pub radius: f32,
    /// Granularity to resolve at when it enters view (m) — the interaction's own scale. The controller
    /// refines it finer if the camera is closer. (Carried through to the particle store as that store
    /// gains per-particle radius — a docs/33 unification step; `materialize_region` is voxel-scale today.)
    pub grain_radius: f32,
    /// What matter this is (index into the material DB). The field materialises the WORLD matter in the
    /// region, so this is used only when the region is off the voxel footprint / for record.
    pub material: usize,
}

/// The one central system a scene holds. Owns the single [`ResolutionController`] and the live analytic
/// effects; turns "active physics off-camera" into "particles the instant it is seen", through the one
/// shared materialisation.
#[derive(Clone, Debug, Default)]
pub struct ResolutionField {
    pub controller: ResolutionController,
    effects: Vec<Effect>,
}

impl ResolutionField {
    pub fn new(controller: ResolutionController) -> Self {
        ResolutionField { controller, effects: Vec::new() }
    }

    /// Register an analytic effect (e.g. the off-camera ejecta a far-side impact just launched).
    pub fn add(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    /// Effects still tracked analytically (not yet resolved). For tests/instrumentation.
    pub fn analytic_count(&self) -> usize {
        self.effects.len()
    }

    /// **The one call every scene makes.** Advance every analytic effect by `dt` under `accel` (cheap
    /// math, no particles), then ask the controller whether each is now visible; for each effect that has
    /// entered view, materialise it through the shared `MatterSim::materialize_region` and drop it from
    /// analytic tracking (the particle sim owns it thereafter). Returns the number of effects resolved
    /// this step. `in_view(center, radius)` is the scene's frustum test — the single scene-specific input,
    /// and it decides only math-vs-simulation, never existence.
    pub fn update(
        &mut self,
        matter: &mut crate::matter::MatterSim,
        materials: &[Material],
        camera: Vec3,
        accel: Vec3,
        dt: f32,
        in_view: impl Fn(Vec3, f32) -> bool,
    ) -> usize {
        let ctrl = self.controller;
        let mut resolved = 0usize;
        // Two-phase (borrow the field immutably to decide, then mutate matter) — `retain` cannot hold a
        // mutable `matter` borrow across iterations cleanly, so collect the survivors.
        let mut survivors = Vec::with_capacity(self.effects.len());
        for mut e in self.effects.drain(..) {
            e.velocity += accel * dt;
            e.center += e.velocity * dt;
            let mode = ctrl.decide(RegionQuery {
                distance_to_camera: (camera - e.center).length() as f64,
                in_view: in_view(e.center, e.radius),
                necessity_depth: 1.0, // an effect IS active physics by definition (existence, camera-free)
                interaction_grain: Some(e.grain_radius as f64),
            });
            match mode {
                ResolutionMode::Resolved { .. } => {
                    // The ONE shared deposit of carried matter — grains created at the effect's kinematic
                    // state (position + the velocity its analytic flight reached). Voxel-scale grains
                    // until the CPU particle store carries per-particle radius (docs/33).
                    matter.spawn_region(materials, e.center, e.radius, e.material, e.velocity);
                    resolved += 1;
                }
                _ => survivors.push(e), // Analytic (off-camera) or Bulk: keep propagating cheaply
            }
        }
        self.effects = survivors;
        resolved
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

    // ---- the core resolution controller (docs/49, three modes) ----

    fn q(dist: f64, in_view: bool, necessity: f64, grain: Option<f64>) -> RegionQuery {
        RegionQuery { distance_to_camera: dist, in_view, necessity_depth: necessity, interaction_grain: grain }
    }

    /// **Camera drives granularity — the screen-space bound (docs/13).** Linear in distance; floored.
    #[test]
    fn camera_granularity_is_the_screen_space_bound() {
        let c = ResolutionController::default();
        let (near, far) = (c.camera_grain_radius(2.0), c.camera_grain_radius(200.0));
        assert!((far / near - 100.0).abs() < 1.0e-6, "grain is LINEAR in distance");
        assert!((near - 2.0 * c.angular_resolution).abs() < 1.0e-12, "grain = distance * angular_res");
        assert_eq!(c.camera_grain_radius(0.0), c.min_grain_radius, "floored at the surface");
    }

    /// No active physics => Bulk, at ANY distance. Static undisturbed ground is not simulated just
    /// because the camera is close — it is rendered from the bulk model. (The flaw the three-mode model
    /// corrected: camera-closeness alone must NOT trigger simulation.)
    #[test]
    fn no_active_physics_is_always_bulk() {
        let c = ResolutionController::default();
        assert_eq!(c.decide(q(10_000.0, false, 0.0, None)), ResolutionMode::Bulk, "far, nothing happening");
        assert_eq!(c.decide(q(0.5, true, 0.0, None)), ResolutionMode::Bulk, "camera at the surface, but static");
    }

    /// **The invariant: the camera does not gate EXISTENCE, only representation (docs/44 §1, docs/30).**
    /// Active physics off-camera is never Bulk — it is computed by MATH (Analytic). The physics happens;
    /// the camera chose the cheaper representation.
    #[test]
    fn active_physics_off_camera_is_analytic_not_bulk() {
        let c = ResolutionController::default();
        let m = c.decide(q(100_000.0, false, 0.1, Some(0.01)));
        assert_eq!(m, ResolutionMode::Analytic, "an unwatched sinking wheel is COMPUTED, not ignored");
        assert_ne!(m, ResolutionMode::Bulk, "camera absence must never turn active physics into nothing");
    }

    /// **The Moon example (Robin, 2026-07-20).** The Moon slams the FAR side of the planet — real impact,
    /// known energy, but off-camera => Analytic (math, no particles). Its ejecta arcs over the horizon;
    /// the region it enters is active AND in view => Resolved (simulate + render). "Render the effects
    /// as/when they come into view."
    #[test]
    fn a_far_side_impact_is_analytic_and_its_ejecta_resolves_as_it_enters_view() {
        let c = ResolutionController::default();
        // Far side of the impact: active, not in view.
        assert_eq!(c.decide(q(6_000_000.0, false, 500.0, None)), ResolutionMode::Analytic);
        // The ejecta blanket, now arriving in the camera's view a few km away: active AND in view.
        match c.decide(q(3_000.0, true, 500.0, Some(0.5))) {
            ResolutionMode::Resolved { grain_radius } => {
                assert!(grain_radius > 0.0 && grain_radius <= c.bulk_grain_radius);
            }
            other => panic!("ejecta entering view must Resolve (simulate + render), got {other:?}"),
        }
    }

    /// Active + in view => Resolved at the FINER of camera and physics granularity (satisfying both).
    #[test]
    fn in_view_active_resolves_at_the_finer_granularity() {
        let c = ResolutionController::default();
        // Physics wants 1 cm; camera at 100 m tolerates 10 cm. The finer (1 cm) wins.
        match c.decide(q(100.0, true, 0.05, Some(0.01))) {
            ResolutionMode::Resolved { grain_radius } => {
                assert!((grain_radius - 0.01).abs() < 1.0e-9, "the 1 cm physics need wins")
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
        // Physics wants 20 cm; camera at 5 m tolerates 5 mm. The camera is stricter and wins.
        match c.decide(q(5.0, true, 0.05, Some(0.20))) {
            ResolutionMode::Resolved { grain_radius } => {
                assert!((grain_radius - c.camera_grain_radius(5.0)).abs() < 1.0e-9, "a close camera refines")
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    /// Resolved granularity stays within [floor, bulk]: never coarser than the bulk (buys nothing), never
    /// finer than the floor (a resolution IOU).
    #[test]
    fn resolved_granularity_stays_between_floor_and_bulk() {
        let c = ResolutionController::default();
        match c.decide(q(200.0, true, 0.05, Some(100.0))) {
            ResolutionMode::Resolved { grain_radius } => assert!(grain_radius <= c.bulk_grain_radius + 1e-12),
            other => panic!("{other:?}"),
        }
        match c.decide(q(0.0, true, 0.05, Some(1.0e-9))) {
            ResolutionMode::Resolved { grain_radius } => assert!(grain_radius >= c.min_grain_radius - 1e-15),
            other => panic!("{other:?}"),
        }
    }

    // ---- the central resolution field: the Analytic -> Resolved hand-off ----

    use crate::materials::{self, Material};
    use crate::world::World;

    fn earth_world() -> (World, Vec<Material>) {
        let mats = materials::load();
        let w = crate::world::generate(&mats);
        (w, mats)
    }

    /// **The Moon example, end to end, through the ONE shared materialisation (docs/49).** An analytic
    /// effect (ejecta) starts off-camera, propagates by cheap math with NO particles created, and the
    /// instant it enters the camera's view it is materialised into grains via `MatterSim::
    /// materialize_region` — the same call the meteor uses. No per-backend adapter; one path.
    #[test]
    fn an_off_camera_effect_materialises_the_frame_it_enters_view() {
        let (_w, mats) = earth_world();
        let mut matter = crate::matter::MatterSim::new(50_000);
        let mut field = ResolutionField::new(ResolutionController::default());

        // Camera at the origin looking at a small view box around it. The effect starts 50 m away moving
        // toward the view, in the air (no terrain contact needed for this test).
        let cam = Vec3::new(0.0, 40.0, 0.0);
        let view_half = 6.0f32;
        let in_view = |c: Vec3, _r: f32| (c - cam).length() < view_half;
        field.add(Effect {
            center: Vec3::new(0.0, 40.0, 50.0),
            velocity: Vec3::new(0.0, 0.0, -12.0), // heading toward the camera
            radius: 3.0,
            grain_radius: 0.5,
            material: materials::index_of(&mats, "basalt"),
        });

        let mut materialised_at = None;
        for step in 0..400 {
            let before = matter.particle_count();
            let n = field.update(&mut matter, &mats, cam, Vec3::ZERO, 1.0 / 60.0, in_view);
            if n > 0 {
                materialised_at = Some(step);
                assert!(matter.particle_count() > before, "resolve must create grains via the shared path");
                break;
            }
            assert_eq!(matter.particle_count(), before, "OFF-camera: cheap math only, ZERO particles");
        }
        let step = materialised_at.expect("the effect should have entered view and resolved");
        assert!(step > 0, "it must have propagated analytically for a while before entering view");
        assert_eq!(field.analytic_count(), 0, "resolved ⇒ handed to the sim, no longer tracked analytically");
    }

    /// An effect that never enters view is NEVER materialised — it stays analytic, costing only math. The
    /// camera does not gate existence (it is still tracked and propagated), but it is not simulated.
    #[test]
    fn an_effect_that_stays_off_camera_is_never_materialised() {
        let (_w, mats) = earth_world();
        let mut matter = crate::matter::MatterSim::new(50_000);
        let mut field = ResolutionField::new(ResolutionController::default());
        let cam = Vec3::new(0.0, 40.0, 0.0);
        let in_view = |_c: Vec3, _r: f32| false; // nothing is ever in view
        field.add(Effect {
            center: Vec3::new(0.0, 40.0, 50.0),
            velocity: Vec3::new(0.0, 0.0, 5.0), // moving AWAY
            radius: 3.0,
            grain_radius: 0.5,
            material: materials::index_of(&mats, "basalt"),
        });
        for _ in 0..300 {
            let n = field.update(&mut matter, &mats, cam, Vec3::ZERO, 1.0 / 60.0, in_view);
            assert_eq!(n, 0);
        }
        assert_eq!(matter.particle_count(), 0, "never seen ⇒ never simulated (only cheap math ran)");
        assert_eq!(field.analytic_count(), 1, "but it is still TRACKED — existence is not gated by the camera");
    }

    /// Analytic propagation is real (ballistic under gravity), so an effect's arrival is physically timed,
    /// not scripted. Under downward gravity a horizontally-drifting effect falls, and where it falls
    /// decides when it crosses the view — the hand-off timing is emergent.
    #[test]
    fn analytic_propagation_is_ballistic() {
        let (_w, mats) = earth_world();
        let mut matter = crate::matter::MatterSim::new(1000);
        let mut field = ResolutionField::new(ResolutionController::default());
        field.add(Effect {
            center: Vec3::new(0.0, 100.0, 0.0),
            velocity: Vec3::ZERO,
            radius: 1.0,
            grain_radius: 0.5,
            material: materials::index_of(&mats, "basalt"),
        });
        let cam = Vec3::new(1000.0, 0.0, 0.0); // far away, nothing in view
        let dt = 1.0 / 60.0;
        // One second of free fall ⇒ ~½·g·t² = ~4.9 m drop, v ≈ 9.8 m/s down.
        for _ in 0..60 {
            field.update(&mut matter, &mats, cam, Vec3::new(0.0, -9.81, 0.0), dt, |_, _| false);
        }
        // The effect is private; assert via re-adding logic is impossible, so check the observable: it
        // fell (materialise it now by forcing view and confirming it's below its start).
        let n = field.update(&mut matter, &mats, cam, Vec3::ZERO, dt, |_, _| true);
        assert_eq!(n, 1, "forcing view resolves it");
        // It materialised near y ≈ 100 − 4.9 ≈ 95 (grains created around there). Confirm grains are below start.
        let ys: Vec<f32> = matter.particles.iter().map(|p| p.pos.y).collect();
        assert!(!ys.is_empty(), "grains were created");
        let max_y = ys.iter().cloned().fold(f32::MIN, f32::max);
        assert!(max_y < 100.0, "the effect fell under gravity before resolving (max grain y {max_y:.1} < 100)");
    }
}
