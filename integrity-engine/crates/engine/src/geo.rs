//! **The one conversion between geography and direction.**
//!
//! Earth's continents rendered MIRRORED east-for-west, and the reason was not the rasters or the camera —
//! both were self-consistent. It was this conversion, written out by hand in six places with the sign that
//! makes longitude increase to the LEFT on screen.
//!
//! The geometry: the render camera is right-handed (`look_at_rh`) with +Y up. Viewing the globe from
//! outside at longitude 0 — eye on +X, looking back at the origin — the camera's right vector is
//! `cross(forward, up) = cross(-X, +Y) = -Z`. So whatever direction we assign to *east* must be −Z there,
//! or east appears on the left and the map is a mirror of the world.
//!
//! The old form `(cos φ cos λ, sin φ, cos φ sin λ)` put east at +Z — the left of screen. Negating the Z
//! term fixes it, and because the same pair of functions is now used for the mesh sampler, the fly
//! camera's tangent frame, the coarse landmask and the solar subsolar point, they cannot disagree: a
//! latitude/longitude means one place, whoever is asking.
//!
//! This was invisible while Earth rendered as a symmetric shell of grains sampling a coarse mask. Giving
//! the planet its real coastlines is what made it obvious — the render got honest enough to show a bug.

use glam::DVec3;

/// Unit direction from the body centre to (`lat_deg`, `lon_deg`). +Y is the spin axis (north); longitude 0
/// is +X; longitude increases EASTWARD, which is −Z, so east appears to the right from outside.
#[inline]
pub fn dir_from_lat_lon(lat_deg: f64, lon_deg: f64) -> DVec3 {
    let (sla, cla) = lat_deg.to_radians().sin_cos();
    let (slo, clo) = lon_deg.to_radians().sin_cos();
    DVec3::new(cla * clo, sla, -cla * slo)
}

/// (latitude°, longitude°) for a direction from the body centre — the exact inverse of
/// [`dir_from_lat_lon`], so a round trip is the identity.
#[inline]
pub fn lat_lon_from_dir(dir: DVec3) -> (f64, f64) {
    let d = dir.normalize_or_zero();
    (d.y.clamp(-1.0, 1.0).asin().to_degrees(), (-d.z).atan2(d.x).to_degrees())
}

/// The local tangent frame at (`lat_deg`, `lon_deg`): unit `up`, `north`, `east`. Derived from
/// [`dir_from_lat_lon`] by differentiation, so the frame and the position can never drift apart.
#[inline]
pub fn tangent_frame(lat_deg: f64, lon_deg: f64) -> (DVec3, DVec3, DVec3) {
    let (sla, cla) = lat_deg.to_radians().sin_cos();
    let (slo, clo) = lon_deg.to_radians().sin_cos();
    let up = DVec3::new(cla * clo, sla, -cla * slo);
    let north = DVec3::new(-sla * clo, cla, sla * slo); // ∂up/∂lat
    let east = DVec3::new(-slo, 0.0, -clo); // ∂up/∂lon, normalised
    (up, north, east)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A latitude/longitude must survive the round trip, and must land where the world says it does.
    #[test]
    fn geography_round_trips_and_east_is_east() {
        for &(lat, lon) in &[(0.0, 0.0), (51.5, -0.13), (-33.9, 151.2), (35.7, 139.7), (89.0, 45.0)] {
            let (a, b) = lat_lon_from_dir(dir_from_lat_lon(lat, lon));
            assert!((a - lat).abs() < 1e-9 && (b - lon).abs() < 1e-9, "round trip {lat},{lon} → {a},{b}");
        }
        // Longitude 0 is +X; the north pole is +Y.
        assert!((dir_from_lat_lon(0.0, 0.0) - DVec3::X).length() < 1e-12);
        assert!((dir_from_lat_lon(90.0, 0.0) - DVec3::Y).length() < 1e-12);

        // **The mirror test.** Stand off the globe at longitude 0 (eye on +X) with north up. The
        // right-handed camera's right vector is cross(forward, up) = cross(-X, +Y) = -Z. A point to the
        // EAST must project onto that side — this is exactly the check the old convention failed, and it
        // is why the continents came out backwards.
        let forward = -DVec3::X;
        let screen_right = forward.cross(DVec3::Y);
        let east_of_us = dir_from_lat_lon(0.0, 10.0);
        assert!(
            east_of_us.dot(screen_right) > 0.0,
            "10°E must appear on screen-right (got {east_of_us:?} vs right {screen_right:?})"
        );
        // ...and west on the other side.
        assert!(dir_from_lat_lon(0.0, -10.0).dot(screen_right) < 0.0, "10°W must appear on screen-left");
    }

    /// The tangent frame must be orthonormal and actually point north/east at the place it claims.
    #[test]
    fn the_tangent_frame_is_orthonormal_and_points_the_right_way() {
        for &(lat, lon) in &[(0.0, 0.0), (45.0, 30.0), (-20.0, -120.0)] {
            let (up, north, east) = tangent_frame(lat, lon);
            for v in [up, north, east] {
                assert!((v.length() - 1.0).abs() < 1e-12, "unit vectors at {lat},{lon}");
            }
            assert!(up.dot(north).abs() < 1e-12 && up.dot(east).abs() < 1e-12 && north.dot(east).abs() < 1e-12);
            assert!((up - dir_from_lat_lon(lat, lon)).length() < 1e-12, "up IS the position direction");
            // Stepping east must increase longitude.
            let (_, lon2) = lat_lon_from_dir((up + east * 1e-4).normalize());
            assert!(lon2 > lon, "east increases longitude at {lat},{lon} ({lon2} vs {lon})");
            // Stepping north must increase latitude.
            let (lat2, _) = lat_lon_from_dir((up + north * 1e-4).normalize());
            assert!(lat2 > lat, "north increases latitude at {lat},{lon}");
        }
    }
}
