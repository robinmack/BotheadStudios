//! docs/43 Phase 5 — the fine ground cap: a high-resolution local patch of the surface under the camera, sampling
//! REAL elevation and curving to a true horizon. It is built CAMERA-RELATIVE — every position is `surface - eye`
//! in display units, computed in f64 and cast to f32 only at the end — so ground-level detail survives the
//! radius-1 globe (raw f32 at Earth's radius has ~0.6 m ULP). The caller draws it with a rotation-only view (the
//! eye sitting at the origin) and cross-fades it against the coarse globe by altitude. Pure geometry (native+wasm).
//!
//! This is the plan's low-risk first LOD step (a single tangent cap). The screen-space-error quadtree with
//! geomorphing + edge skirts, and sub-raster fbm micro-detail, are the follow-on refinements.

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

/// The cap↔globe cross-fade: 0 at/above the derived hand-off (`start_alt_m`,
/// [`handoff_alt_m`]), 1 at/below half of it, smoothstepped over that first OCTAVE of raster
/// deficit (at half the hand-off a texel subtends two budget units — the stretching is
/// unambiguously visible, so the cap must be fully in charge by then). A `start_alt_m` of 0 (no
/// rasters) never fades the cap in.
pub fn cap_fade(alt_m: f64, start_alt_m: f64) -> f64 {
    if !(start_alt_m > 0.0) {
        return 0.0;
    }
    let t = (start_alt_m / alt_m.max(1e-9)).log2().clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The cap reaches this factor PAST the horizon, so its far edge sits below the horizon /
/// occluded and no visible boundary is drawn where it ends.
pub const CAP_MARGIN: f64 = 1.3;
/// The clamp on the cap's angular parameter: the tangent-frame parameterization (`center +
/// east·du + north·dv`, normalized) reaches arc `atan(du)`, so large parameters buy less and
/// less arc; past this the patch geometry degrades faster than it covers.
pub const CAP_MAX_ANGLE: f64 = 0.6;

/// The cap's angular parameter for a camera whose horizon distance (display units) is
/// `horizon_disp` on a sphere of radius `r_disp`. `horizon/r = tan` of the true horizon arc,
/// which is exactly this gnomonic-style parameter's own measure, so the margin covers the
/// horizon at any altitude the clamp allows.
pub fn cap_angle(horizon_disp: f64, r_disp: f64) -> f64 {
    (CAP_MARGIN * horizon_disp / r_disp).clamp(1e-4, CAP_MAX_ANGLE)
}

/// Whether the cap covers every visible point of the surface — its margin past the horizon fits
/// inside the clamp. Only then may a renderer skip the coarse globe underneath (which is what
/// removes the cap-vs-globe depth fight in the final metres, where the tight near plane leaves
/// the f32 depth buffer only ~tens of metres of resolution at the horizon); skipping any higher
/// would cut the planet's limb out of the frame.
pub fn cap_covers_view(horizon_disp: f64, r_disp: f64) -> bool {
    r_disp > 0.0 && CAP_MARGIN * horizon_disp / r_disp <= CAP_MAX_ANGLE
}

/// The cap's radial lift (METRES) that wins the depth fight against the coarse globe while BOTH
/// are drawn (the fade band). It is 0.2% of the altitude — proportional at every scale, because the f32
/// depth buffer's own resolution at the nadir also scales with the viewing distance (≈ z²/(near·2²⁴)
/// with a near plane that is a few percent of the altitude, i.e. a few 1e-4 % of z), so a fixed lift
/// that wins at one altitude loses at another. The old 20 m ceiling belonged to the declared 15–40 km
/// fade band and lost the fight once the band's top became the DERIVED hand-off (thousands of km).
/// Proportionality is also what keeps the lift from ever reaching the eye: 0.2% of the altitude sits
/// far below a camera at 100% of it, down to and past standing height.
///
/// It is stated in metres because a CACHED tier has to compare the lift it baked against the one the
/// current altitude asks for ([`tier_is_current`]), and that comparison is against a distance.
pub fn cap_lift_m(alt_m: f64) -> f64 {
    alt_m * 0.002
}

/// [`cap_lift_m`] in display units. `ds` is the display scale (metres → display units).
pub fn cap_lift_disp(alt_m: f64, ds: f64) -> f64 {
    cap_lift_m(alt_m) * ds
}

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
    /// The radial lift (m, [`cap_lift_m`]) baked into the vertices.
    pub lift_m: f64,
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
/// 3. **Lift.** The depth-fight allowance baked into the vertices is altitude-proportional, so a
///    cached one is a stale allowance and must track altitude within the same octave — in BOTH
///    directions, since too small loses the depth fight against the globe just as surely as too large
///    floats above it. This is what keeps a stale lift from ever reaching the eye: held within an
///    octave, a baked lift is at most 0.4% of the current altitude against a camera at 100% of it.
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
    let lifted = built.lift_m <= OCTAVE * fresh.lift_m && fresh.lift_m <= OCTAVE * built.lift_m;
    covers && resolves && lifted
}

/// Fill `out` with the ground-cap vertices (cleared first). The index topology is fixed for a given `res` — get
/// it once from [`cap_indices`] — so this is called every frame to rewrite only the vertex buffer.
/// - `center`,`east`,`north`: the unit surface direction under the camera and its tangent frame.
/// - `origin`: the world point (display units) the patch is emitted relative to. Pass the EYE for a
///   throwaway patch; pass a fixed [`CapTierBuild::anchor`] to cache the mesh across frames, and put
///   `anchor - eye` in the model matrix so the draw stays camera-relative either way. The two are the
///   same picture — the anchored one simply stops rewriting every vertex when only the eye moved.
/// - `r_disp`: planet radius in display units.
/// - `cap_angle`: angular radius (radians) the patch spans from `center` — size it to ~the horizon.
/// - `res`: grid points per side.
/// - `sample(dir) -> (albedo, radius_offset_display, material_index)`: Terra reads the rasters (real elevation × the declared
///   exaggeration, biome albedo) for a surface direction.
#[allow(clippy::too_many_arguments)]
pub fn fill_ground_cap(
    out: &mut Vec<Vertex>,
    center: DVec3,
    east: DVec3,
    north: DVec3,
    origin: DVec3,
    r_disp: f64,
    cap_angle: f64,
    res: usize,
    sample: impl Fn(DVec3) -> ([f32; 3], f64, u32),
) {
    assert!(res >= 2);
    out.clear();
    out.reserve(res * res);

    let mut rel = vec![Vec3::ZERO; res * res]; // positions relative to `origin` (display units)
    let mut cols = vec![[0f32; 3]; res * res];
    // The material each cap vertex is made OF — the shader needs it to pick the right relief layer.
    let mut mats = vec![0u32; res * res];
    for j in 0..res {
        for i in 0..res {
            // Angular offsets east/north in [-cap_angle, cap_angle]; a denser-toward-centre curve (u³) keeps
            // resolution high near the camera while still reaching the horizon at the edges.
            let su = -1.0 + 2.0 * i as f64 / (res - 1) as f64;
            let sv = -1.0 + 2.0 * j as f64 / (res - 1) as f64;
            let du = su * su.abs() * cap_angle; // signed-square: |curve| toward centre
            let dv = sv * sv.abs() * cap_angle;
            let dir = (center + east * du + north * dv).normalize();
            let (col, off, mat) = sample(dir);
            let p = dir * (r_disp + off);
            let r = p - origin;
            rel[j * res + i] = Vec3::new(r.x as f32, r.y as f32, r.z as f32);
            cols[j * res + i] = col;
            mats[j * res + i] = mat;
        }
    }

    for j in 0..res {
        for i in 0..res {
            // Normals from central differences of the displaced patch (translation-invariant, so relative
            // positions are fine whatever the origin); edges fall back to the outward (radial) direction.
            let outward = {
                let p = rel[j * res + i].as_dvec3() + origin;
                let o = p.normalize_or_zero();
                Vec3::new(o.x as f32, o.y as f32, o.z as f32)
            };
            let nrm = if i > 0 && i < res - 1 && j > 0 && j < res - 1 {
                let dU = rel[j * res + i + 1] - rel[j * res + i - 1];
                let dV = rel[(j + 1) * res + i] - rel[(j - 1) * res + i];
                let mut nn = dU.cross(dV).normalize_or_zero();
                if nn == Vec3::ZERO {
                    nn = outward;
                }
                if nn.dot(outward) < 0.0 {
                    nn = -nn;
                }
                nn
            } else {
                outward
            };
            out.push(Vertex {
                pos: rel[j * res + i].to_array(),
                nrm: nrm.to_array(),
                col: cols[j * res + i],
                mat: mats[j * res + i],
            });
        }
    }
}

/// The index buffer for a cap of a given `res` (topology is fixed, so the caller builds it once and only rewrites
/// vertices each frame).
pub fn cap_indices(res: usize) -> Vec<u32> {
    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let a = (j * res + i) as u32;
            let b = (j * res + i + 1) as u32;
            let c = ((j + 1) * res + i + 1) as u32;
            let d = ((j + 1) * res + i) as u32;
            indices.extend_from_slice(&[a, c, b, a, d, c]);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_counts_and_indices_are_well_formed() {
        let res = 16;
        let center = DVec3::X;
        let (east, north) = (DVec3::new(0.0, 0.0, -1.0), DVec3::Y);
        let eye = center * 1.001; // 0.001 display units up
        let mut v = Vec::new();
        fill_ground_cap(&mut v, center, east, north, eye, 1.0, 0.02, res, |_| {
            ([0.3, 0.4, 0.5], 0.0, 0)
        });
        assert_eq!(v.len(), res * res);
        let idx = cap_indices(res);
        assert_eq!(idx.len(), (res - 1) * (res - 1) * 6);
        assert!(idx.iter().all(|&i| (i as usize) < v.len()));
    }

    #[test]
    fn cap_lift_scales_with_altitude_and_stays_below_the_eye() {
        let ds = 1.0 / 6_371_000.0;
        for alt in [0.5, 2.0, 100.0, 15_000.0, 5.0e6, 2.0e7] {
            let lift = cap_lift_disp(alt, ds);
            // The lift can never reach the camera: 0.2% of the altitude, so the eye (at 100%) always
            // sits far above the lifted surface, down to and below the 2 m standing height.
            assert!(
                lift < alt * ds * 0.01,
                "lift must stay well under the eye at {alt} m"
            );
            // And it always wins the depth fight: the f32 depth buffer resolves ≈ z²/(near·2²⁴) at
            // the nadir with a near plane ≥ 3% of the altitude — a few 1e-4 % of z — so a lift
            // proportional at 0.2% clears it by orders of magnitude at EVERY altitude (the fixed
            // 20 m ceiling this replaces lost the fight once the fade band topped out at the
            // derived hand-off instead of a declared 40 km).
            let depth_ulp = alt * alt / ((0.03 * alt) * 2.0f64.powi(24)) * ds;
            assert!(
                lift > 10.0 * depth_ulp,
                "lift must clear the depth resolution at {alt} m"
            );
        }
        // Proportional, not tiered: one law across the whole corridor.
        assert!((cap_lift_disp(2.0e7, ds) / cap_lift_disp(2.0, ds) - 1.0e7).abs() < 1.0);
    }

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

    /// The cross-fade spans the FIRST OCTAVE of raster deficit: 0 at/above the derived hand-off,
    /// 1 at/below half of it (a texel now subtends two budget units), smooth and monotonic
    /// between — so the close-range treatment arrives as a glide, never a pop.
    #[test]
    fn the_cross_fade_spans_the_first_octave_below_the_handoff() {
        let start = 1.95e7;
        assert_eq!(
            cap_fade(start, start),
            0.0,
            "no fade at the hand-off itself"
        );
        assert_eq!(cap_fade(3.0 * start, start), 0.0, "none above it");
        assert_eq!(
            cap_fade(start / 2.0, start),
            1.0,
            "fully the cap one octave down"
        );
        assert_eq!(
            cap_fade(2.0, start),
            1.0,
            "and all the way to standing height"
        );
        let mut prev = 0.0f64;
        for i in 1..20 {
            let alt = start * 0.5f64.powf(i as f64 / 20.0);
            let f = cap_fade(alt, start);
            assert!(
                f > prev && f < 1.0,
                "fade blends monotonically inside the octave at {alt} m"
            );
            prev = f;
        }
        // A world with no derived hand-off (no rasters) never fades the cap in.
        assert_eq!(cap_fade(1_000.0, 0.0), 0.0);
    }

    /// The coarse globe may be skipped only when the cap genuinely covers every visible point of
    /// the surface (its margin past the horizon fits inside the parameterization clamp). Skipping
    /// any higher would cut the planet's limb out of the frame.
    #[test]
    fn the_globe_is_skipped_only_where_the_cap_covers_past_the_horizon() {
        let r_disp = 1.0;
        let horizon = |alt_m: f64| {
            let h = alt_m / 6.371e6;
            (h * (h + 2.0)).sqrt()
        };
        for alt in [2.0, 15_000.0, 100_000.0, 400_000.0] {
            assert!(cap_covers_view(horizon(alt), r_disp), "covered at {alt} m");
        }
        for alt in [2.0e6, 5.0e6, 1.95e7] {
            assert!(
                !cap_covers_view(horizon(alt), r_disp),
                "the limb is visible beyond the cap at {alt} m"
            );
        }
        // The angle the cap is actually built with honours the same margin, clamped.
        for alt in [2.0, 15_000.0, 400_000.0, 5.0e6] {
            let a = cap_angle(horizon(alt), r_disp);
            assert!(a <= CAP_MAX_ANGLE + 1e-12);
            if cap_covers_view(horizon(alt), r_disp) {
                assert!(
                    (a - CAP_MARGIN * horizon(alt) / r_disp).abs() < 1e-12,
                    "unclamped where it covers"
                );
            }
        }
    }

    /// A tangent frame and a sampler with real relief, shared by the anchoring tests below.
    fn frame(center: DVec3) -> (DVec3, DVec3) {
        let east = center.cross(DVec3::Y).normalize();
        (east, east.cross(center).normalize())
    }

    /// **THE pin on anchoring: it must not move the picture.** The anchored mesh plus its model
    /// translation is the eye-relative mesh, vertex for vertex. This is the whole safety argument for
    /// caching a tier — the cached draw is not an approximation of the per-frame one, it is the same
    /// arithmetic with the eye's part factored out into the model matrix (Law VI: the render may be
    /// reorganised, the surface may not move).
    #[test]
    fn an_anchored_cap_draws_exactly_where_an_eye_relative_one_did() {
        let res = 33;
        let center = DVec3::new(0.3, 0.5, 0.2).normalize();
        let (east, north) = frame(center);
        let eye = center * 1.0004; // ~2.5 km up on a radius-1 Earth
        let anchor = center; // the surface point under the build-time camera
                             // Relief with real structure, so an error cannot hide in a flat patch.
        let sample = |d: DVec3| ([0.2, 0.4, 0.1], 1e-6 * (d.x * 900.0).sin(), 3u32);

        let mut eye_rel = Vec::new();
        fill_ground_cap(
            &mut eye_rel,
            center,
            east,
            north,
            eye,
            1.0,
            0.02,
            res,
            sample,
        );
        let mut anchored = Vec::new();
        fill_ground_cap(
            &mut anchored,
            center,
            east,
            north,
            anchor,
            1.0,
            0.02,
            res,
            sample,
        );

        // What the shader adds: `model` translates by `anchor - eye`, so world = pos + (anchor - eye).
        let model = (anchor - eye).as_vec3();
        assert_eq!(eye_rel.len(), anchored.len());
        for (a, b) in eye_rel.iter().zip(anchored.iter()) {
            let want = Vec3::from_array(a.pos);
            let got = Vec3::from_array(b.pos) + model;
            // f32 at ~0.02 rad from a radius-1 planet: a few ULPs of the 0.02-magnitude offsets.
            assert!(
                (want - got).length() < 1e-6,
                "anchored vertex {got:?} != eye-relative {want:?}"
            );
            // Normals and materials are translation-invariant. The edge normals route through
            // `rel + origin`, so they differ in the last f32 ULP with a different origin — the
            // DIRECTION is what must not move.
            let (na, nb) = (Vec3::from_array(a.nrm), Vec3::from_array(b.nrm));
            assert!(
                na.dot(nb) > 1.0 - 1e-6,
                "anchoring must not rotate a normal: {na:?} vs {nb:?}"
            );
            assert_eq!(a.mat, b.mat);
        }
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
            lift_m: cap_lift_m(alt),
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
                lift_m: cap_lift_m(alt),
            }
        };
        let built = tier_at(500_000.0);
        // A 100 m descent out of 500 km buys nothing a rebuild could deliver.
        assert!(
            tier_is_current(&built, &tier_at(499_900.0)),
            "a 100 m descent out of 500 km is not worth a rebuild"
        );
        // **Halving the altitude is exactly the boundary** — the lift is linear in altitude, so one
        // halving is precisely one octave of it, and the rule holds to the last metre of that octave.
        // Stating it as the boundary rather than as "half is stale" is the honest form: it is where
        // the octave lands, not a number chosen to make a test read well.
        assert!(
            tier_is_current(&built, &tier_at(250_000.0)),
            "the cache survives to the octave and no further"
        );
        assert!(
            !tier_is_current(&built, &tier_at(249_000.0)),
            "one metre past the octave the lift it baked is stale"
        );
        // A quarter of the altitude is past the octave on the CELL condition too, independently of
        // the lift: the horizon shrinks as √alt, so cells go a full doubling coarser over 4×.
        let quarter = tier_at(125_000.0);
        assert!(
            built.cell_m > 2.0 * quarter.cell_m,
            "cells are more than an octave coarse a quarter of the way down"
        );
        assert!(!tier_is_current(&built, &quarter));

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
        assert!(
            (10..200).contains(&rebuilds),
            "a 500 km descent should cost tens of rebuilds, got {rebuilds}"
        );
    }

    /// The baked lift is altitude-proportional, so a CACHED one is a stale allowance. It can never
    /// outlive its own altitude: the resolution/lift conditions go red after a fraction of a descent,
    /// hundreds of times before 0.2%-of-a-past-altitude could climb to the eye — which is the bug
    /// (ground above the camera) that the proportional lift was introduced to fix.
    #[test]
    fn a_cached_lift_can_never_climb_to_the_eye() {
        let r_m: f64 = 6.371e6;
        for built_alt in [10.0, 500.0, 20_000.0, 500_000.0] {
            let h = |a: f64| ((a / r_m) * (a / r_m + 2.0)).sqrt();
            let t = |a: f64| CapTierBuild {
                center: DVec3::X,
                anchor: DVec3::X,
                cap_angle: CAP_MARGIN * h(a),
                cell_m: h(a) * r_m * 2.0 / 192.0,
                lift_m: cap_lift_m(a),
            };
            let built = t(built_alt);
            // Descend until the cache is declared stale; the lift it baked must still sit far below
            // the eye at every altitude it was allowed to survive to.
            let mut alt = built_alt;
            while alt > 1e-3 && tier_is_current(&built, &t(alt)) {
                assert!(
                    built.lift_m < 0.01 * alt,
                    "a lift of {} m is not far below a {alt} m eye",
                    built.lift_m
                );
                alt *= 0.99;
            }
            assert!(alt < built_alt, "the cache must go stale on the way down");
        }
    }

    #[test]
    fn centre_vertex_sits_directly_below_the_eye_at_the_camera_height() {
        // Flat sampler, eye 0.004 above the centre point → the centre vertex is the eye-relative surface point,
        // i.e. length ≈ the eye height, pointing down (−center).
        let res = 9; // odd so the centre grid point is exactly at (mid, mid)
        let center = DVec3::new(0.3, 0.5, 0.2).normalize();
        let up = center;
        let east = up.cross(DVec3::Y).normalize();
        let north = east.cross(up).normalize();
        let h = 0.004;
        let eye = center * (1.0 + h);
        let mut v = Vec::new();
        fill_ground_cap(&mut v, center, east, north, eye, 1.0, 0.02, res, |_| {
            ([0.0, 0.0, 0.0], 0.0, 0)
        });
        let mid = (res / 2) * res + res / 2;
        let c = v[mid].pos;
        let cv = Vec3::from_array(c);
        assert!(
            (cv.length() - h as f32).abs() < 1e-5,
            "centre vertex height {} != {}",
            cv.length(),
            h
        );
        let down = Vec3::new(-center.x as f32, -center.y as f32, -center.z as f32);
        assert!(
            cv.normalize().dot(down) > 0.999,
            "centre vertex points straight down from the eye"
        );
    }
}
