//! **When a cached surface mesh is still the answer a fresh build would give** — plus the hand-off
//! altitude derived from a body's own raster resolution.
//!
//! This module WAS the tangent ground cap (docs/43 Phase 5): a square patch projected onto the sphere,
//! cross-faded against a separate coarse globe. Both are gone (docs/63). `terra::segment` draws one
//! surface at whatever extent is visible, so the cap builder, the cross-fade, the depth-fight lift and
//! the "may I skip the globe" test all went with the second mesh they existed to mediate.
//!
//! What survives is the part that was never about having two meshes: the cache rule, and the hand-off.

use crate::mesher::Vertex;
use glam::{DVec3, Vec3};

/// **The close-range hand-off altitude (m), DERIVED from the raster's own resolution** — never a
/// declared constant. It is the altitude at which one texel of the surface raster subtends exactly
/// the docs/49 angular budget: above it the planetary raster still fills the view honestly; below
/// it the renderer would be stretching texels across more than a budget unit each, so the
/// close-range treatment (this cap, sampling the raster at the camera's own angular density, plus
/// the material relief) must take over. It IS `site::view_resolution_distance` asked about one
/// texel — "at what distance does an extent this size stop filling the view" is one question with
/// one answer, whether the asker is the site materialization threshold or the render (Law II).
pub fn handoff_alt_m(texel_arc_m: f64, angular_resolution_rad: f64) -> f64 {
    crate::site::view_resolution_distance(texel_arc_m, angular_resolution_rad)
}

/// The finest ground arc (m) any of a body's shipped rasters resolves — the LAST data to run out
/// on a descent, so the one the hand-off keys on (the coarser rasters are already stretched by
/// then; showing their texels at their true size is the honest floor where no finer tier exists).
/// `None` when no raster is loaded: nothing finer exists, so there is nothing to hand off to.
pub fn finest_texel_arc_m(
    rasters: &[Option<&crate::terra::raster::Raster>],
    radius_m: f64,
) -> Option<f64> {
    rasters
        .iter()
        .flatten()
        .map(|r| r.texel_arc_m(radius_m))
        .min_by(f64::total_cmp)
}

/// The cap reaches this factor PAST the horizon, so its far edge sits below the horizon /
/// occluded and no visible boundary is drawn where it ends.
pub const CAP_MARGIN: f64 = 1.3;
/// The clamp on the cap's angular parameter: the tangent-frame parameterization (`center +
/// east·du + north·dv`, normalized) reaches arc `atan(du)`, so large parameters buy less and
/// less arc; past this the patch geometry degrades faster than it covers.
pub const CAP_MAX_ANGLE: f64 = 0.6;

/// **What a ground-cap tier was built WITH** — everything a later frame needs in order to ask whether
/// the cached mesh still says what a fresh build would say. See [`tier_is_current`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapTierBuild {
    /// Unit surface direction the disc is centred on (the sub-camera point at build time).
    pub center: DVec3,
    /// The world point (display units) every vertex is emitted relative to. The draw carries the
    /// remainder, `anchor - eye`, in the model matrix — which is what lets the eye move without
    /// touching a vertex.
    pub anchor: DVec3,
    /// Angular radius the disc spans from `center`, [`CAP_MARGIN`] included.
    pub cap_angle: f64,
    /// One cell's size on the ground (m) — the mesh's own resolution.
    pub cell_m: f64,
}

/// **Is a cached tier still the answer a fresh build would give?**
///
/// A tier is a CACHE OF THE VIEW. Anchored, its vertices are world-fixed, so re-deriving them about
/// the same centre reproduces them exactly — the eye moving is carried by the model matrix and
/// changes nothing. So the question is not "how old is this mesh" (a refresh interval is a dial with
/// no physics in it) but **"would rebuilding it change anything the observer can resolve?"** — the
/// same question `atmosphere::air_reaches` asks of the air, and the same angular budget
/// (docs/49) that sizes every other resolution decision here.
///
/// Three things can differ between the cached build and a fresh one, and each is asked that way:
///
/// 1. **Coverage.** The cached disc must still contain the one a fresh build needs. `fresh.cap_angle`
///    carries [`CAP_MARGIN`] past the horizon, so the BARE requirement is `fresh.cap_angle /
///    CAP_MARGIN` and the margin is exactly the slack the eye may drift into. Nothing new is declared
///    here: the tier is allowed to go stale by precisely the amount it was over-built by. This is
///    also the condition that catches ASCENT, where a fresh build needs a WIDER disc than the cache.
/// 2. **Resolution.** A fresh build lower down uses finer cells. The unit of "meaningfully finer" is
///    the OCTAVE — the same one [`cap_fade`] spans, where a doubling is the point at which the
///    stretching becomes unambiguously visible. Under an octave, a rebuild buys less than one rung of
///    the ladder this tier sits on. Only the too-coarse direction matters: a cache FINER than needed
///    costs nothing to look at, and if it is also too small, coverage has already said so.
/// A THIRD condition used to live here — the depth-fight lift baked into the vertices, which had to
/// track altitude within an octave too. It went with the second mesh: a lift exists only to hold two
/// copies of one surface apart, and there is one (docs/63).
///
/// **An absolute angular test was tried here first and is wrong** — `|Δcell| / range ≤ θ`, the
/// budget that sizes everything else in this module. It collapses at low altitude: a horizon-sized
/// cap has ~52 m cells at 2 m altitude, so *any* descent changes them by more than the eye resolves
/// and the answer is "rebuild" forever (measured: ~13,000 rebuilds per halving). It is not lying —
/// those cells really are visibly coarse — but the cure for a mesh too coarse to express the ground
/// is another TIER, not another rebuild of the same one. The octave measures the rebuild against
/// what a rebuild can actually deliver.
pub fn tier_is_current(built: &CapTierBuild, fresh: &CapTierBuild) -> bool {
    const OCTAVE: f64 = 2.0;
    let drift = built.center.angle_between(fresh.center);
    let covers = drift + fresh.cap_angle / CAP_MARGIN <= built.cap_angle;
    let resolves = built.cell_m <= OCTAVE * fresh.cell_m;
    covers && resolves
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The hand-off altitude is DERIVED from the raster's own resolution, never declared.** It is
    /// the altitude where ONE TEXEL of the finest shipped raster subtends exactly the angular
    /// budget — the same budget the site materialization threshold uses — so it is the same
    /// `view_resolution_distance` law asked about one texel, not a second answer (Law II).
    #[test]
    fn the_handoff_altitude_derives_from_the_rasters_own_resolution() {
        let theta = crate::resolution::ResolutionController::default().angular_resolution;
        let r_m = 6.371e6;
        let raster =
            crate::terra::raster::Raster::new(2048, 1024, 1, vec![0; 2048 * 1024]).unwrap();
        let texel = raster.texel_arc_m(r_m);
        let start = handoff_alt_m(texel, theta);
        // The definition: one texel per budget unit — texel / θ, ~19,500 km for the shipped Earth
        // rasters at the 1 mrad budget. (Yes, that high: a 2048-wide equirectangular Earth is
        // stretched well above LEO at this budget; the issue's "a few thousand km" was the eyeball
        // estimate the derivation replaces.)
        assert!(
            (start - texel / theta).abs() < 1e-6,
            "hand-off = texel / angular budget"
        );
        assert!(
            (1.9e7..2.0e7).contains(&start),
            "shipped-raster hand-off ~19,500 km, got {start}"
        );
        // It IS the materialization threshold's own law (one primitive, two askers).
        assert_eq!(start, crate::site::view_resolution_distance(texel, theta));
        // Finer data hands off proportionally lower: better rasters push the corridor down.
        assert!((handoff_alt_m(texel / 4.0, theta) - start / 4.0).abs() < 1e-6);
        // No raster → no texel → no hand-off (nothing finer exists to hand off to).
        assert_eq!(handoff_alt_m(0.0, theta), 0.0);
    }

    /// The hand-off keys on the FINEST raster a body ships — the last data to run out on the way
    /// down; absent rasters contribute nothing.
    #[test]
    fn the_handoff_keys_on_the_finest_shipped_raster() {
        let r_m = 6.371e6;
        let coarse = crate::terra::raster::Raster::new(512, 256, 1, vec![0; 512 * 256]).unwrap();
        let fine = crate::terra::raster::Raster::new(2048, 1024, 1, vec![0; 2048 * 1024]).unwrap();
        let t =
            finest_texel_arc_m(&[Some(&coarse), None, Some(&fine)], r_m).expect("rasters present");
        assert_eq!(
            t,
            fine.texel_arc_m(r_m),
            "the finest raster is the one that matters"
        );
        assert_eq!(
            finest_texel_arc_m(&[None, None], r_m),
            None,
            "no rasters, no texel"
        );
    }

    /// A tangent frame and a sampler with real relief, shared by the anchoring tests below.
    fn frame(center: DVec3) -> (DVec3, DVec3) {
        let east = center.cross(DVec3::Y).normalize();
        (east, east.cross(center).normalize())
    }

    /// A cached tier stands while a rebuild would change nothing the eye can resolve, and the
    /// slack it is allowed to drift into is exactly the margin it was over-built by ([`CAP_MARGIN`]) —
    /// not a declared refresh distance.
    #[test]
    fn a_tier_stands_while_rebuilding_it_would_change_nothing_visible() {
        let r_m: f64 = 6.371e6;
        let alt: f64 = 5_000.0;
        let horizon_ang = ((alt / r_m) * (alt / r_m + 2.0)).sqrt();
        let center = DVec3::X;
        let built = CapTierBuild {
            center,
            anchor: center,
            cap_angle: CAP_MARGIN * horizon_ang,
            cell_m: horizon_ang * r_m * 2.0 / 192.0,
        };
        // Rebuilt on the spot, nothing has moved: identical, so the cache is trivially current.
        assert!(tier_is_current(&built, &built));

        // Drift the sub-point sideways. It stays current out to the margin and not past it.
        let inside = {
            let a = 0.25 * horizon_ang; // inside the 0.3·h of slack CAP_MARGIN buys
            let mut f = built;
            f.center = (center + DVec3::Y * a.tan()).normalize();
            f
        };
        assert!(
            tier_is_current(&built, &inside),
            "still covers the view after drifting inside its own margin"
        );
        let outside = {
            let a = 0.35 * horizon_ang;
            let mut f = built;
            f.center = (center + DVec3::Y * a.tan()).normalize();
            f
        };
        assert!(
            !tier_is_current(&built, &outside),
            "past the margin the cached disc no longer reaches the horizon"
        );
    }

    /// Descending is what a cached tier must NOT survive: a fresh build has finer cells, and the
    /// tier is stale the moment that difference becomes resolvable. This is the condition that makes
    /// detail rise on the way down instead of freezing at the altitude the mesh was born at.
    #[test]
    fn a_tier_goes_stale_when_a_rebuild_would_be_visibly_finer() {
        let r_m: f64 = 6.371e6;
        let tier_at = |alt: f64| {
            let h = ((alt / r_m) * (alt / r_m + 2.0)).sqrt();
            CapTierBuild {
                center: DVec3::X,
                anchor: DVec3::X,
                cap_angle: CAP_MARGIN * h,
                cell_m: h * r_m * 2.0 / 192.0,
            }
        };
        let built = tier_at(500_000.0);
        // A 100 m descent out of 500 km buys nothing a rebuild could deliver.
        assert!(
            tier_is_current(&built, &tier_at(499_900.0)),
            "a 100 m descent out of 500 km is not worth a rebuild"
        );
        // **★ The boundary moved, and that is a real change worth stating.** It used to be a halving of
        // altitude, because the depth-fight lift is LINEAR in altitude and tripped its octave first. The
        // lift went with the second mesh it existed to separate (docs/63), so the CELL condition alone
        // governs — and the horizon, hence the cell, goes as sqrt(h(h+2R)), so the cache now lasts about
        // four times the altitude drop. For a reason, not by tuning.
        assert!(
            tier_is_current(&built, &tier_at(250_000.0)),
            "half the altitude is now well inside the cache's life"
        );
        // The boundary is where the CELL halves, and that is not exactly a quartering of altitude: the
        // horizon goes as sqrt(h(h+2R)), so from 500 km the cell halves at **128,607 m**, not 125,000.
        // Solved rather than eyeballed — at 125 km the ratio is already 2.0289 and the cache is stale.
        assert!(
            tier_is_current(&built, &tier_at(128_700.0)),
            "just inside the octave it still stands"
        );
        assert!(
            !tier_is_current(&built, &tier_at(128_500.0)),
            "just past it, a rebuild is owed"
        );
        assert!(
            (built.cell_m / tier_at(128_607.0).cell_m - 2.0).abs() < 1e-4,
            "the solved boundary really is one octave of cell: ratio {}",
            built.cell_m / tier_at(128_607.0).cell_m
        );

        // The rebuild ladder this implies is LOGARITHMIC, not per-frame: find the altitude each
        // cached tier survives down to, and check the whole descent costs tens of rebuilds rather
        // than one per frame (the 45 ms/frame this replaces).
        let mut alt = 500_000.0;
        let mut rebuilds = 0;
        while alt > 2.0 {
            let b = tier_at(alt);
            let mut next = alt;
            while next > 2.0 && tier_is_current(&b, &tier_at(next)) {
                next *= 0.999;
            }
            alt = next;
            rebuilds += 1;
            assert!(rebuilds < 500, "rebuild ladder must converge");
        }
        // ★ NINE rebuilds for the whole 500 km descent — half what it was when the depth-fight lift
        // still had a vote (18). Removing the second mesh did not just delete code, it halved the work
        // the cache has to do, because the lift's LINEAR octave tripped twice as often as the cell's.
        assert!(
            (5..40).contains(&rebuilds),
            "a 500 km descent should cost a handful of rebuilds, got {rebuilds}"
        );
    }

    /// A ladder of `n` tiers at `alt`, each covering a quarter of the span of the one outside it —
    /// Terra's `TERRA_TIER_RATIO`, reproduced here so the policy is tested against the real shape.
    fn ladder(alt: f64, n: usize) -> Vec<CapTierBuild> {
        let r_m: f64 = 6.371e6;
        let h = ((alt / r_m) * (alt / r_m + 2.0)).sqrt();
        (0..n)
            .map(|t| {
                let ang = CAP_MARGIN * h / 4f64.powi(t as i32);
                CapTierBuild {
                    center: DVec3::X,
                    anchor: DVec3::X,
                    cap_angle: ang,
                    cell_m: ang * r_m * 2.0 / 192.0,
                }
            })
            .collect()
    }
}
