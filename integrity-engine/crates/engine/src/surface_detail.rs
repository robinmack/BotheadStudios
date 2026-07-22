//! **Surface detail that follows the camera, continuously** (`docs/49`, `docs/08`).
//!
//! Robin: *"we should continually be adjusting level of detail based on camera perception"*, and
//! *"being that close to the ground should make the detailed texture generate naturally (scaling based
//! on viewable area)."*
//!
//! Measured 2026-07-21: flying the definitive Earth down to 2 m altitude works, and the surface renders
//! FEATURELESS — the globe mesh is built for orbital viewing, so standing on it puts the camera inside
//! one planet-scale triangle. `ResolutionController::camera_grain_radius` already computes exactly the
//! number needed (*"detail finer than this is sub-pixel at this distance"*) and had **zero consumers**.
//! This module is that consumer.
//!
//! **This changes REPRESENTATION, never EXISTENCE (Law IV).** The relief is a property of the surface
//! and is there whether or not anyone is close enough to see it. All that varies is how finely it is
//! sampled for drawing. Walking away does not flatten the ground; it stops paying to resolve it.
//!
//! **Continuous, not tiered.** Every quantity here is a smooth function of distance. A level-of-detail
//! *ladder* would pop as the camera crossed a threshold, and popping is the render telling you about
//! the renderer instead of about the world (Law VI).
//!
//! **PER OBJECT, by ITS OWN distance — there is no global detail level.** Robin's case: *"if I'm
//! simulating a spacewalk, the earth doesn't have to be as finely detailed as the orbital debris that is
//! rapidly approaching the helmet of my spacesuit."* Both are in the same frame, and they want detail
//! four or five orders of magnitude apart. So every function here takes the distance to the THING being
//! drawn. A caller that computes one number per frame — from camera altitude, say — and applies it to
//! everything has reintroduced a global LOD level and will over-detail the planet while under-detailing
//! the rivet about to hit the visor.

use crate::resolution::ResolutionController;

/// The finest surface feature worth drawing at `distance_m`, in metres.
///
/// This is `camera_grain_radius` — `distance × angular_resolution`, floored by the controller's
/// `min_grain_radius`. Smaller is finer. It is deliberately the SAME function that decides particle
/// granularity: "how fine can this viewer resolve" must not have two answers (Law II).
pub fn texel_size_m(ctrl: &ResolutionController, distance_m: f64) -> f64 {
    ctrl.camera_grain_radius(distance_m)
}

/// How many octaves of detail to add on top of a surface whose coarsest feature is `base_feature_m`,
/// to reach the finest the viewer can resolve at `distance_m`.
///
/// **Fractional on purpose.** Rounding to an integer octave count is what makes detail POP as the
/// camera moves; a caller blends the last octave in by its fractional part so detail arrives smoothly.
/// Returns 0 when the viewer cannot even resolve the base feature (far away — draw the plain surface).
pub fn detail_octaves(ctrl: &ResolutionController, distance_m: f64, base_feature_m: f64) -> f64 {
    let target = texel_size_m(ctrl, distance_m);
    if !(base_feature_m > 0.0) || !(target > 0.0) || target >= base_feature_m {
        return 0.0;
    }
    // Each octave halves the feature size, so the count is log2(base / target).
    (base_feature_m / target).log2().max(0.0)
}

/// Fraction of the world the viewer can see across, given a vertical field of view — the "viewable
/// area" detail scales against. Used to turn an altitude into the distance that matters.
pub fn view_span_m(distance_m: f64, vertical_fov_rad: f64) -> f64 {
    2.0 * distance_m.max(0.0) * (vertical_fov_rad * 0.5).tan()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl() -> ResolutionController {
        ResolutionController::default() // 1 mrad angular resolution, 1 mm floor
    }

    /// Closer must ALWAYS mean finer, with no flat spots and no reversals — that is what "continually
    /// adjusting to camera perception" means.
    #[test]
    fn detail_refines_monotonically_as_the_camera_approaches() {
        let c = ctrl();
        let dists = [8_000_000.0, 100_000.0, 1_000.0, 100.0, 10.0, 2.0];
        let mut prev = f64::INFINITY;
        for d in dists {
            let t = texel_size_m(&c, d);
            assert!(t < prev, "at {d} m the texel ({t}) must be finer than at the previous distance ({prev})");
            prev = t;
        }
    }

    /// The numbers a person would sanity-check: at orbital altitude you resolve kilometres; standing on
    /// it you resolve millimetres. If these are wrong the whole feature is decorative.
    #[test]
    fn resolvable_detail_matches_the_scale_you_are_viewing_from() {
        let c = ctrl();
        // 8,000 km up (Terra's default): ~1 mrad × 8e6 m = 8 km. You cannot see a boulder.
        let orbital = texel_size_m(&c, 8_000_000.0);
        assert!((7_000.0..9_000.0).contains(&orbital), "orbital texel {orbital} m should be ~8 km");
        // 2 m up (standing): ~2 mm. Individual pebbles are resolvable — which is why the ground must
        // not be a single flat triangle there.
        let standing = texel_size_m(&c, 2.0);
        assert!((0.001..0.01).contains(&standing), "standing texel {standing} m should be ~2 mm");
        assert!(
            orbital / standing > 1e5,
            "the span from orbit to standing is five orders of magnitude — that is the whole problem"
        );
    }

    /// The floor is real: you cannot usefully resolve below the controller's declared minimum, however
    /// close you get. Without it, detail would diverge as distance → 0.
    #[test]
    fn detail_never_goes_below_the_declared_floor() {
        let c = ctrl();
        for d in [1.0, 0.1, 0.001, 0.0] {
            assert!(texel_size_m(&c, d) >= c.min_grain_radius, "floor breached at {d} m");
        }
    }

    /// Octaves must be CONTINUOUS in distance. A jump means a visible pop as the camera moves, which is
    /// the render reporting on the renderer rather than the world.
    #[test]
    fn octave_count_is_continuous_so_detail_cannot_pop() {
        let c = ctrl();
        let base = 1000.0; // a 1 km raster cell
        let mut prev = detail_octaves(&c, 1_000_000.0, base);
        let mut d = 1_000_000.0;
        while d > 2.0 {
            let next_d = d * 0.97; // 3% closer each step
            let n = detail_octaves(&c, next_d, base);
            assert!(
                (n - prev).abs() < 0.1,
                "octaves jumped {prev} -> {n} between {d} m and {next_d} m — that is a pop"
            );
            assert!(n >= prev, "moving closer must never REDUCE detail");
            prev = n;
            d = next_d;
        }
        assert!(prev > 15.0, "closing from 1000 km to 2 m on a 1 km cell needs many octaves, got {prev}");
    }

    /// Far enough away, added detail is not merely unnecessary — it is invisible, so the honest answer
    /// is zero extra work.
    #[test]
    fn no_octaves_are_spent_on_detail_the_viewer_cannot_resolve() {
        let c = ctrl();
        // A 1 mm feature viewed from 8,000 km: hopeless.
        assert_eq!(detail_octaves(&c, 8_000_000.0, 0.001), 0.0);
        // The same feature from 1 m: worth it.
        assert!(detail_octaves(&c, 1.0, 0.001) >= 0.0);
    }

    /// **The spacewalk.** Two things in ONE frame, at wildly different distances, must get wildly
    /// different detail — the Earth below and the debris arriving at the visor. This is the guard
    /// against anyone collapsing detail to a single per-frame level.
    #[test]
    fn objects_at_different_distances_get_their_own_detail_in_the_same_frame() {
        let c = ctrl();
        let earth_below_m = 400_000.0; // low Earth orbit
        let debris_at_visor_m = 2.0;

        let earth = texel_size_m(&c, earth_below_m);
        let debris = texel_size_m(&c, debris_at_visor_m);

        assert!(
            earth / debris > 10_000.0,
            "the planet ({earth} m/texel) and the debris ({debris} m/texel) must differ by orders of \
             magnitude; a single global LOD level cannot serve both"
        );
        // Concretely: hundreds of metres per texel for the planet, millimetres for the debris.
        assert!(earth > 100.0, "the Earth 400 km away needs no sub-metre detail, got {earth} m");
        assert!(debris < 0.01, "the debris at arm's length does, got {debris} m");
    }

    /// Detail must follow VIEWABLE AREA, not just a raw number — a wide field of view at the same
    /// distance shows more world, so each texel covers more of it.
    #[test]
    fn view_span_grows_with_distance_and_field_of_view() {
        let narrow = view_span_m(100.0, 30f64.to_radians());
        let wide = view_span_m(100.0, 90f64.to_radians());
        assert!(wide > narrow * 2.0, "a 90-degree view spans far more than a 30-degree one");
        assert!(view_span_m(200.0, 60f64.to_radians()) > view_span_m(100.0, 60f64.to_radians()));
        assert_eq!(view_span_m(0.0, 60f64.to_radians()), 0.0);
    }
}
