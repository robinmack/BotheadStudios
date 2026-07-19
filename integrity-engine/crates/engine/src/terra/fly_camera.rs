//! docs/43 Phase 4 — the continuous fly camera (orbit ⇄ ground). ONE camera, altitude-blended: high up it looks
//! down at the planet and a drag orbits the globe; near the ground it looks along the horizon and a drag turns the
//! view. The transition is a smoothstep on altitude, so there is no mode switch — you fly in from space to a
//! standing-on-the-ground horizon continuously (the Google-Earth feel).
//!
//! Precision: positions are kept in f64 and the whole view+projection is built in f64, cast to f32 only at the
//! very end, so ground-level framing survives the radius-1 globe (raw f32 at Earth's radius has ~0.6 m ULP). The
//! per-tile local-origin rebasing that the ground LOD needs (Phase 5) layers on top of this.
//!
//! Conventions: `lat`/`lon` in degrees (the world-file convention; `dir.y = sin(lat)`, `lon` measured from +X
//! toward +Z — the same mapping the raster sampler uses). `yaw`/`pitch` in radians (heading 0 = north, +east;
//! pitch 0 = horizon, + up, − down). Distances that enter the matrices are in DISPLAY units (`metres × ds`).

use glam::{DMat4, DVec3, Mat4};

/// One frame's camera outputs (see `FlyCamera::view`).
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub vp_abs: Mat4,
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

impl FlyCamera {
    #[allow(clippy::too_many_arguments)]
    pub fn new(lat: f64, lon: f64, alt_m: f64, yaw: f64, pitch: f64, min_alt: f64, max_alt: f64) -> Self {
        let mut c = FlyCamera { lat, lon, alt_m, yaw, pitch, min_alt: min_alt.max(0.1), max_alt };
        c.alt_m = c.alt_m.clamp(c.min_alt, c.max_alt);
        c
    }

    /// The local tangent frame (unit `up`, `north`, `east`) at the current lat/lon.
    pub fn frame(&self) -> (DVec3, DVec3, DVec3) {
        let (sla, cla) = self.lat.to_radians().sin_cos();
        let (slo, clo) = self.lon.to_radians().sin_cos();
        let up = DVec3::new(cla * clo, sla, cla * slo);
        let north = DVec3::new(-sla * clo, cla, -sla * slo);
        let east = DVec3::new(-slo, 0.0, clo);
        (up, north, east)
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

    /// Everything the renderer needs for one frame. `vp_abs` is the view·projection with the eye in world space
    /// (for the globe); `vp_rel` is the same projection with the eye at the ORIGIN (for camera-relative geometry
    /// like the ground cap — subtract the eye from positions in f64, then this maps them). `eye` is the f64 eye
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
        let near = (alt_disp * near_frac).clamp(1e-6, 0.5).min(far * 0.5);
        let proj = DMat4::perspective_rh(0.9, aspect.max(1e-3), near, far);
        let vp_abs = (proj * DMat4::look_at_rh(eye, eye + fwd, up_view)).as_mat4();
        let vp_rel = (proj * DMat4::look_at_rh(DVec3::ZERO, fwd, up_view)).as_mat4();
        View { vp_abs, vp_rel, eye, up, north, east, alt_disp, horizon }
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
