//! docs/43 Phase 4 — the continuous fly camera (orbit ⇄ ground). ONE camera, altitude-blended: high up it looks
//! down at the planet and a drag orbits the globe; near the ground it looks along the horizon and a drag turns the
//! view. The transition is a smoothstep on altitude, so there is no mode switch — you fly in from space to a
//! standing-on-the-ground horizon continuously (the Google-Earth feel).
//!
//! Precision: THE CAMERA-RELATIVE-EYE CONVENTION (this is where it lives; docs/59 order-of-work item 2, the
//! descent camera that holds f32 precision to 2 m). Positions are kept in f64 and the
//! whole view+projection is built in f64, cast to f32 only at the very end. The renderer is handed ONE matrix,
//! `vp_rel`, whose eye sits at the ORIGIN: everything Terra draws is expressed relative to the eye before any
//! f32 sees it, because raw f32 at Earth's radius (radius-1 display units) has ~0.4 m ULP and cannot hold the
//! final metres of a descent. Concretely:
//! - per-frame geometry (the ground cap) subtracts the eye per-VERTEX in f64 and uploads the small remainder
//!   (`ground_cap::fill_ground_cap`); round-trip error is sub-millimetre at planet radius (tested below);
//! - static geometry (the globe mesh, the grain shell) is drawn with a MODEL translation of −eye built in f64
//!   and cast once, so the residual per-vertex f32 error is a couple of ULPs at planet radius (~1 m), which
//!   subtends under a pixel at the ≥15 km distances where that coarse geometry is still visible (tested below);
//! - there is deliberately NO absolute view·projection in `View`; an absolute-eye matrix is the precision bug,
//!   so the type does not offer one.
//!
//! Conventions: `lat`/`lon` in degrees (the world-file convention; `dir.y = sin(lat)`, `lon` measured from +X
//! toward +Z — the same mapping the raster sampler uses). `yaw`/`pitch` in radians (heading 0 = north, +east;
//! pitch 0 = horizon, + up, − down). Distances that enter the matrices are in DISPLAY units (`metres × ds`).

use glam::{DMat4, DVec3, Mat4};

/// One frame's camera outputs (see `FlyCamera::view`).
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub vp_rel: Mat4,
    pub eye: DVec3,
    pub up: DVec3,
    pub north: DVec3,
    pub east: DVec3,
    pub alt_disp: f64,
    pub horizon: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct FlyCamera {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub min_alt: f64,
    pub max_alt: f64,
}

/// Altitudes (metres) bounding the orbit⇄ground blend: fully ground at/below `GROUND_ALT`, fully orbital
/// at/above `ORBIT_ALT`, smoothstepped between.
const GROUND_ALT: f64 = 3_000.0;
const ORBIT_ALT: f64 = 400_000.0;

/// Vertical field of view (radians). One constant, used by the projection AND by the pan gesture's
/// pixel-to-metres scale, so the two can never disagree about how much world a pixel spans.
pub const FOV_Y: f64 = 0.9;

impl FlyCamera {
    #[allow(clippy::too_many_arguments)]
    pub fn new(lat: f64, lon: f64, alt_m: f64, yaw: f64, pitch: f64, min_alt: f64, max_alt: f64) -> Self {
        let mut c = FlyCamera { lat, lon, alt_m, yaw, pitch, min_alt: min_alt.max(0.1), max_alt };
        c.alt_m = c.alt_m.clamp(c.min_alt, c.max_alt);
        c
    }

    /// The local tangent frame (unit `up`, `north`, `east`) at the current lat/lon.
    pub fn frame(&self) -> (DVec3, DVec3, DVec3) {
        // THE shared conversion (crate::geo) — this frame was one of six hand-written copies, and the
        // one sign they all shared put east on the left of the screen.
        crate::geo::tangent_frame(self.lat, self.lon)
    }

    /// Fraction of "ground mode" (1 at/below `GROUND_ALT`, 0 at/above `ORBIT_ALT`), smoothstepped.
    pub fn ground_blend(&self) -> f64 {
        let t = ((self.alt_m - GROUND_ALT) / (ORBIT_ALT - GROUND_ALT)).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }

    /// Eye position, forward direction, and view-up (all in display units / unit vectors), altitude-blended.
    /// `ground_disp` is the terrain height (display units, above the sea-level sphere) directly under the camera,
    /// so `alt_m` is height above the *local ground* — otherwise low altitude would put the eye inside the
    /// (exaggerated) terrain. The caller (Terra) samples it from the elevation raster.
    pub fn view_basis(&self, r_disp: f64, ds: f64, ground_disp: f64) -> (DVec3, DVec3, DVec3) {
        let (up, north, east) = self.frame();
        let eye = up * (r_disp + ground_disp + self.alt_m * ds);
        let g = self.ground_blend();
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // Ground: look along the horizon per yaw/pitch. Orbital: look straight down at the planet centre.
        let f_ground = (north * cy + east * sy) * cp + up * sp;
        let f_orbit = -up;
        let fwd = f_orbit.lerp(f_ground, g).normalize_or_zero();
        // View-up: north when looking down (orbital), local up when looking at the horizon (ground).
        let up_view = north.lerp(up, g).normalize_or_zero();
        (eye, fwd, up_view)
    }

    /// Everything the renderer needs for one frame. `vp_rel` is THE view·projection, with the eye at the
    /// ORIGIN (the camera-relative-eye convention, module doc): subtract the f64 `eye` from positions in f64
    /// (per vertex, or via a model translation of −eye built in f64), then this maps them. `eye` is the f64 eye
    /// (display units); `up/north/east` is the tangent frame under the camera; `horizon` is the display-unit
    /// distance to the horizon (for sizing the cap). Near/far scale with altitude.
    pub fn view(&self, r_disp: f64, ds: f64, aspect: f64, ground_disp: f64) -> View {
        let (eye, fwd, up_view) = self.view_basis(r_disp, ds, ground_disp);
        let (up, north, east) = self.frame();
        let alt_disp = (self.alt_m * ds).max(1e-9);
        let h = (ground_disp + alt_disp).max(1e-9);
        let horizon = (h * (h + 2.0 * r_disp)).sqrt();
        // FAR reaches just past the horizon (the farthest visible surface); not inflated beyond that (the old
        // `.max(near*1000)` was), so the depth range stays tight.
        let far = horizon * 1.4;
        // NEAR: tuned for depth precision. At HIGH altitude the nearest visible surface is the point directly
        // below the camera (~the altitude itself), so `near` can be a large fraction of it — a tight near/far
        // range keeps the globe's far hemisphere cleanly depth-occluded (the globe is drawn without back-face
        // culling; see the globe pipeline). Near the GROUND the cap can have terrain very close (down to
        // `min_alt`), so `near` must stay tiny there. Blend by altitude (metres): ground regime below ~10 km,
        // globe regime above ~60 km.
        let t = ((self.alt_m - 10_000.0) / 50_000.0).clamp(0.0, 1.0);
        let globe_regime = t * t * (3.0 - 2.0 * t);
        let near_frac = 0.03 + 0.45 * globe_regime;
        // Floor: a few centimetres at Earth scale (5e-9 display units ≈ 3 cm), so standing at min-alt
        // never near-clips the ground underfoot. The old 1e-6 floor was ~6.4 m; at 2 m altitude it cut
        // a hole in the terrain below the camera.
        let near = (alt_disp * near_frac).clamp(5e-9, 0.5).min(far * 0.5);
        let proj = DMat4::perspective_rh(FOV_Y, aspect.max(1e-3), near, far);
        // Built in f64 with the eye at the ORIGIN and cast once; the camera-relative-eye convention
        // (module doc). No absolute-eye matrix is produced anywhere.
        let vp_rel = (proj * DMat4::look_at_rh(DVec3::ZERO, fwd, up_view)).as_mat4();
        View { vp_rel, eye, up, north, east, alt_disp, horizon }
    }

    // --- input deltas ---

    /// Multiply altitude (zoom): `factor < 1` descends, `> 1` climbs; clamped to [min, max]. `alt_m` is height
    /// above the LOCAL ground (Terra offsets the eye by the terrain height), and `min_alt` is a hard clearance, so
    /// the eye can never sink into the solid ground beneath it — the camera obeys real physics (no clipping
    /// through rock/soil; gas/liquid is pass-through). Flank collision along the path is a TODO for Phase 5.
    pub fn zoom_alt(&mut self, factor: f64) {
        self.alt_m = (self.alt_m * factor).clamp(self.min_alt, self.max_alt);
    }

    /// Move across the surface by metres in the local north/east directions (WASD). Step size is the caller's
    /// concern (it scales with altitude); this just maps metres → lat/lon on the sphere.
    pub fn move_tangent(&mut self, dnorth_m: f64, deast_m: f64, planet_radius_m: f64) {
        let dlat = (dnorth_m / planet_radius_m).to_degrees();
        let cos_lat = self.lat.to_radians().cos().abs().max(1e-3);
        let dlon = (deast_m / (planet_radius_m * cos_lat)).to_degrees();
        self.lat = (self.lat + dlat).clamp(-89.9, 89.9);
        self.lon = wrap_lon(self.lon + dlon);
    }

    /// A pointer drag (pixel deltas). Blended by altitude: orbital → pan the globe under the camera; ground →
    /// free-look (turn the head). Mid-altitude does a little of both, so the feel is continuous.
    pub fn drag(&mut self, dx: f64, dy: f64) {
        let g = self.ground_blend();
        // Orbit part (weight 1−g): drag moves the camera's lat/lon so the globe rolls under a downward view.
        let orbit_k = 0.25 * (1.0 - g);
        let cos_lat = self.lat.to_radians().cos().abs().max(0.2);
        self.lat = (self.lat + dy * orbit_k).clamp(-89.9, 89.9);
        self.lon = wrap_lon(self.lon - dx * orbit_k / cos_lat);
        // Look part (weight g): drag turns yaw/pitch.
        let look_k = 0.005 * g;
        self.yaw += dx * look_k;
        self.pitch = (self.pitch - dy * look_k).clamp(-1.5, 1.5);
    }
}

/// Wrap a longitude to (−180, 180].
fn wrap_lon(lon: f64) -> f64 {
    let mut l = (lon + 180.0) % 360.0;
    if l < 0.0 {
        l += 360.0;
    }
    l - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> FlyCamera {
        FlyCamera::new(20.0, 0.0, 8_000_000.0, 0.0, -1.2, 2.0, 40_000_000.0)
    }

    #[test]
    fn tangent_frame_is_orthonormal_and_up_matches_latlon() {
        for &(lat, lon) in &[(0.0, 0.0), (45.0, 90.0), (-30.0, -120.0), (89.0, 200.0)] {
            let c = FlyCamera::new(lat, lon, 1000.0, 0.0, 0.0, 1.0, 1e9);
            let (up, north, east) = c.frame();
            for v in [up, north, east] {
                assert!((v.length() - 1.0).abs() < 1e-9, "unit vectors");
            }
            assert!(up.dot(north).abs() < 1e-9 && up.dot(east).abs() < 1e-9 && north.dot(east).abs() < 1e-9);
            // `up` must equal the surface direction the raster sampler decodes back to this lat/lon.
            assert!((up.y.asin().to_degrees() - lat).abs() < 1e-6, "up recovers latitude");
        }
    }

    #[test]
    fn ground_blend_is_1_low_0_high_and_monotonic() {
        let mut c = cam();
        c.alt_m = 100.0;
        assert!((c.ground_blend() - 1.0).abs() < 1e-9, "ground at low alt");
        c.alt_m = 20_000_000.0;
        assert!(c.ground_blend().abs() < 1e-9, "orbital at high alt");
        // Monotonically decreasing with altitude across the transition band.
        let mut prev = 2.0;
        for &a in &[1_000.0, 10_000.0, 50_000.0, 200_000.0, 500_000.0] {
            c.alt_m = a;
            let g = c.ground_blend();
            assert!(g <= prev + 1e-12, "blend must not increase with altitude");
            prev = g;
        }
    }

    #[test]
    fn eye_sits_at_the_expected_radius_and_orbital_view_looks_down() {
        let c = cam();
        let (eye, fwd, _up) = c.view_basis(1.0, 1.0 / 6_371_000.0, 0.0);
        let expect_r = 1.0 + 8_000_000.0 / 6_371_000.0;
        assert!((eye.length() - expect_r).abs() < 1e-9, "eye radius = r_disp + alt");
        // At 8000 km the camera is orbital → forward points inward (toward the planet centre).
        assert!(fwd.dot(-eye.normalize()) > 0.99, "orbital forward looks down");
    }

    #[test]
    fn ground_view_looks_at_the_horizon() {
        let mut c = cam();
        c.alt_m = 500.0; // ground mode
        c.pitch = 0.0;
        let (eye, fwd, up_view) = c.view_basis(1.0, 1.0 / 6_371_000.0, 0.0);
        let up = eye.normalize();
        // pitch 0 → forward is tangent to the sphere (perpendicular to local up), and sky (up_view) points up.
        assert!(fwd.dot(up).abs() < 1e-6, "horizon forward is tangent");
        assert!(up_view.dot(up) > 0.999, "ground view-up is local up");
    }

    #[test]
    fn camera_relative_eye_round_trip_is_submillimetre_at_planet_radius() {
        // The convention's core claim (module doc): subtract the eye in f64, cast only the small
        // remainder to f32, and the final metres of a descent survive Earth's radius. Bound: under
        // 1 mm of round-trip error for ground-cap geometry around a camera standing 2 m up.
        // Contrast: pushing the ABSOLUTE position through f32; the naive scheme this replaces ;
        // loses centimetres-to-decimetres at the very same points; that loss, re-rolled every frame
        // as the eye moves, is exactly the ground-level jitter the convention exists to kill.
        let ds = 1.0 / 6_371_000.0; // display scale: Earth radius → 1.0
        let c = FlyCamera::new(47.3, 8.5, 2.0, 0.3, -0.2, 2.0, 4e7);
        let (eye, _fwd, _upv) = c.view_basis(1.0, ds, 0.0);
        let (up, north, east) = c.frame();
        let (mut worst_rel_m, mut worst_naive_m) = (0.0f64, 0.0f64);
        for k in 0..8 {
            let a = k as f64 * std::f64::consts::TAU / 8.0;
            // A ring of surface points ~2 m around the point under the camera (the cap's near field).
            let p = (up + (east * a.cos() + north * a.sin()) * (2.0 * ds)).normalize();
            // The renderer's path: f64 subtract, f32 upload, viewed from the origin.
            let round_trip = (p - eye).as_vec3().as_dvec3() + eye;
            worst_rel_m = worst_rel_m.max((round_trip - p).length() / ds);
            // The naive path: the absolute position quantized by f32.
            worst_naive_m = worst_naive_m.max((p.as_vec3().as_dvec3() - p).length() / ds);
        }
        assert!(worst_rel_m < 1e-3, "camera-relative round trip must hold 1 mm at planet radius, got {worst_rel_m} m");
        // And it is far inside a pixel: at 2 m viewing distance against a ~0.9 rad / ~900 px frustum.
        assert!(worst_rel_m / 2.0 < 1e-5, "relative-eye error must be well under a pixel, got {} rad", worst_rel_m / 2.0);
        assert!(worst_naive_m > 0.01, "absolute f32 at planet radius should lose centimetres (measured {worst_naive_m} m); the failure this scheme replaces");
    }

    #[test]
    fn triplanar_anchor_restores_surface_fixed_texture_phase() {
        // Positions reach the shader camera-relative, but the relief texture must stay glued to the
        // SURFACE, not slide with the camera. The renderer re-adds the eye folded modulo the 8 m
        // texture tile (small, so f32-safe); this checks the identity behind it: rel + anchor equals
        // the absolute position minus a WHOLE number of tiles per axis; the same texture phase ;
        // to well under a millimetre at 2 m standing height.
        let ds = 1.0 / 6_371_000.0;
        let tile = 8.0 * ds;
        let c = FlyCamera::new(23.0, 10.0, 2.0, 0.0, -0.7, 2.0, 4e7);
        let (eye, _fwd, _upv) = c.view_basis(1.0, ds, 0.0);
        let (up, north, east) = c.frame();
        let anchor =
            DVec3::new(eye.x.rem_euclid(tile), eye.y.rem_euclid(tile), eye.z.rem_euclid(tile)).as_vec3();
        for k in 0..8 {
            let a = k as f64 * std::f64::consts::TAU / 8.0;
            let p = (up + (east * a.cos() + north * a.sin()) * (3.0 * ds)).normalize();
            let coord = ((p - eye).as_vec3() + anchor).as_dvec3(); // the shader's f32 arithmetic
            for c in ((coord - p) / tile).to_array() {
                let frac_m = (c - c.round()).abs() * 8.0; // fractional-tile error, in metres
                assert!(frac_m < 1e-4, "texture phase must be surface-fixed, off by {frac_m} m");
            }
        }
    }

    #[test]
    fn globe_model_translation_stays_subpixel_where_the_coarse_globe_is_drawn() {
        // Static meshes (the globe) cannot re-subtract the eye per vertex each frame, so their share of
        // the convention is a MODEL translation of −eye built in f64 and cast once; the GPU then adds
        // f32 vertex to f32 translation. The residual is a couple of f32 ULPs at planet radius (~1 m,
        // absolute). That is fine because the coarse globe is only ever on screen alongside the cap at
        // camera altitudes ≥ 15 km (below that the cap alone covers the view out past the horizon), so
        // the bound that matters is ANGULAR: error over ≥ 15 km must stay under a tenth of a pixel
        // (a 0.9 rad frustum over ~900 px is ~1e-3 rad per pixel).
        let ds = 1.0 / 6_371_000.0;
        let c = FlyCamera::new(-12.7, 131.9, 15_000.0, 1.1, -0.4, 2.0, 4e7);
        let (eye, _fwd, _upv) = c.view_basis(1.0, ds, 0.0);
        let model = DMat4::from_translation(-eye).as_mat4(); // the renderer's exact construction
        let mut worst_m = 0.0f64;
        for k in 0..32 {
            let a = k as f64 * 0.11;
            // Vertices spread over the globe, carrying terrain-scale radial offsets, stored as the
            // mesh stores them: absolute f32.
            let dir = DVec3::new(a.cos() * 0.6, 0.4 + 0.01 * a, a.sin() * 0.6).normalize();
            let p64 = dir * (1.0 + 2_000.0 * ds * (0.5 + 0.5 * a.sin()));
            let rel32 = model.transform_point3(p64.as_vec3()); // f32 mesh vertex, f32 GPU arithmetic
            worst_m = worst_m.max((rel32.as_dvec3() - (p64 - eye)).length() / ds);
        }
        assert!(worst_m < 1.5, "model-relative residual must stay within ~2 ULPs at planet radius, got {worst_m} m");
        let pixel = 0.9 / 900.0;
        let angular = worst_m / 15_000.0;
        assert!(angular < 0.1 * pixel, "coarse-globe error must subtend under a tenth of a pixel, got {angular} rad");
    }

    #[test]
    fn zoom_and_move_clamp_and_wrap() {
        let mut c = cam();
        c.zoom_alt(0.0); // would go to 0 → clamps to min
        assert_eq!(c.alt_m, c.min_alt);
        c.zoom_alt(1e12); // clamps to max
        assert_eq!(c.alt_m, c.max_alt);
        // A quarter-circumference east step moves ~90° of longitude at the equator.
        let mut e = FlyCamera::new(0.0, 0.0, 1000.0, 0.0, 0.0, 1.0, 1e9);
        let quarter = std::f64::consts::FRAC_PI_2 * 6_371_000.0;
        e.move_tangent(0.0, quarter, 6_371_000.0);
        assert!((e.lon - 90.0).abs() < 1e-6, "quarter-circumference east = 90° lon, got {}", e.lon);
        // Longitude wraps.
        e.move_tangent(0.0, 3.0 * quarter, 6_371_000.0);
        assert!(e.lon > -180.0 && e.lon <= 180.0, "lon stays wrapped, got {}", e.lon);
    }
}
