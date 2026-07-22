//! **The real sky.** Parses `sky/stars.bin` (the HYG naked-eye subset) into render-ready stars.
//!
//! The asset carries only what was MEASURED — right ascension, declination, apparent magnitude, colour
//! index. Temperature, colour and brightness are derived here, because they are physics and physics lives
//! in the engine. That is also why the file has no RGB in it: a sky with baked colours is a picture.
//!
//! **No sky sphere.** Stars carry their real CARTESIAN positions, in parsecs, with Sol at the origin —
//! not directions on a shell. A shell is a Sol-centric universe: it asserts every star is equidistant and
//! that the observer never moves, so the constellations would drag along with the camera and interstellar
//! travel could never work. Here the renderer computes direction from (star − observer) and brightness
//! from the inverse-square law over the true distance, every frame. From here that reproduces the sky we
//! see, because it IS the sky we see; from Sirius it would show Sirius's sky, with Sol among the stars,
//! for free.
//!
//! **Frames.** Positions are in the inertial equatorial frame (ICRS), placed through the one shared `geo`
//! convention so the sky and the continents agree about which way is east. A scene whose world frame is
//! Earth-FIXED (Terra) also hands over Earth's rotation angle, which is what makes the stars wheel
//! overhead once per sidereal day without anything being animated.

/// One star, ready to draw: a real place, a real colour, a real luminosity.
#[derive(Debug, Clone, Copy)]
pub struct Star {
    /// Position in PARSECS, Sol at the origin, inertial equatorial axes.
    pub pos_pc: [f32; 3],
    /// Linear sRGB from the star's own temperature — chromaticity only, brightest channel 1.
    pub color: [f32; 3],
    /// Luminosity as the flux it would show at 10 pc (i.e. 10^(−0.4·M)). Apparent brightness is
    /// `luminosity · (10/d)²` for an observer at distance `d` parsecs — computed per frame, because it
    /// depends on where you are standing.
    pub luminosity: f32,
}

/// Relative flux for a magnitude, normalised to 1.0 at magnitude 0 (Pogson's ratio: five magnitudes is a
/// factor of one hundred).
#[inline]
pub fn flux_from_magnitude(mag: f64) -> f64 {
    10f64.powf(-0.4 * mag)
}

/// Apparent flux of a star of luminosity `lum` (flux at 10 pc) seen from `dist_pc` parsecs — the
/// inverse-square law, which is the whole reason positions are stored instead of directions.
#[inline]
pub fn apparent_flux(lum: f64, dist_pc: f64) -> f64 {
    if dist_pc <= 0.0 { return 0.0; }
    lum * (10.0 / dist_pc).powi(2)
}

/// Parse the catalogue: 20-byte little-endian records of (x, y, z parsecs, absolute magnitude, B−V).
///
/// Returns an error rather than a partial sky if the file is the wrong shape — a truncated star
/// catalogue silently missing its faint half is the kind of thing nobody notices.
pub fn parse_catalog(bytes: &[u8]) -> Result<Vec<Star>, String> {
    const REC: usize = 20;
    if bytes.is_empty() || bytes.len() % REC != 0 {
        return Err(format!("star catalogue is {} bytes, not a multiple of {REC}", bytes.len()));
    }
    let mut stars = Vec::with_capacity(bytes.len() / REC);
    for rec in bytes.chunks_exact(REC) {
        let f = |i: usize| f32::from_le_bytes([rec[i], rec[i + 1], rec[i + 2], rec[i + 3]]);
        let (x, y, z, absmag, bv) = (f(0), f(4), f(8), f(12), f(16));
        let color = crate::blackbody::blackbody_srgb(crate::blackbody::temperature_from_bv(bv as f64));
        stars.push(Star {
            pos_pc: [x, y, z],
            color,
            luminosity: flux_from_magnitude(absmag as f64) as f32,
        });
    }
    Ok(stars)
}

/// **How long the sky can be treated as fixed**, in seconds: the time before the nearest star drifts by
/// `angular_resolution_rad` for an observer moving at `speed_ms`. Pure geometry — a star's apparent
/// motion is `v/d`, so the answer falls out of the nearest distance and nothing else.
///
/// This is the engine's version of Robin's rule: *"factor in the velocity relative to distance... stars
/// are close enough to immobile even at solar escape velocity to only need position calculations once in
/// a long interval."* Quantified, that is generous beyond belief — at 42 km/s (solar escape at 1 AU) the
/// nearest naked-eye star moves ONE PIXEL IN 35 YEARS.
///
/// The renderer nonetheless recomputes every star every frame, and deliberately so: an ablation measured
/// the star pass at 80 fps with and 80 fps without, i.e. free. Caching would mean a CPU pass plus a buffer
/// upload on each refresh — strictly MORE work than the per-vertex arithmetic it replaces — in exchange
/// for state, a staleness policy, and a correctness cliff at high speed. The stateless form is exact at
/// any velocity and costs nothing measurable, so there is nothing to buy.
///
/// It becomes worth revisiting when the catalogue grows by orders of magnitude (the full HYG is 119,613
/// stars; a galaxy is more), and the function is here so that decision is made against a NUMBER. Note the
/// separate limit: near c, stellar ABERRATION shifts apparent positions far faster than parallax does,
/// and that is a different correction, not a cache-invalidation interval.
pub fn sky_fixed_for_seconds(nearest_pc: f64, speed_ms: f64, angular_resolution_rad: f64) -> f64 {
    const PC_M: f64 = 3.085_677_581e16;
    if speed_ms <= 0.0 {
        return f64::INFINITY; // a stationary observer's sky never shifts by parallax
    }
    angular_resolution_rad * nearest_pc * PC_M / speed_ms
}

/// Greenwich Mean Sidereal Time (radians) — how far Earth has turned under the stars. Shared with the
/// solar position so the Sun and the sky cannot disagree about what time it is.
pub fn gmst_rad(unix_seconds: f64) -> f64 {
    let n = (unix_seconds - 946_728_000.0) / 86_400.0; // days since J2000.0
    let hours = 18.697_374_558 + 24.065_709_824_419_08 * n;
    (hours * std::f64::consts::PI / 12.0).rem_euclid(std::f64::consts::TAU)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalogue must parse to the sky we actually have, and the brightest thing in it must be
    /// Sirius — at the real distance, with the real luminosity. If the record layout or the axis mapping
    /// drifts, the sky silently becomes noise, and this is the check that says so.
    #[test]
    fn the_catalogue_parses_to_the_real_sky() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/sky/stars.bin"))
            .expect("web/public/sky/stars.bin must ship with the engine");
        let stars = parse_catalog(&bytes).expect("catalogue must parse");
        assert_eq!(stars.len(), 8_715, "the naked-eye sky with usable distances");

        // Sirius: 2.64 pc away, absolute magnitude 1.45, B−V ≈ 0 (blue-white), at RA 6h45m Dec −16°43′.
        let s = stars[0];
        let d = (s.pos_pc[0].powi(2) + s.pos_pc[1].powi(2) + s.pos_pc[2].powi(2)).sqrt() as f64;
        assert!((d - 2.64).abs() < 0.05, "Sirius is 2.64 pc away, got {d:.3}");
        let dir = crate::geo::dir_from_lat_lon(-16.716, 101.287);
        let dot = (s.pos_pc[0] as f64 * dir.x + s.pos_pc[1] as f64 * dir.y + s.pos_pc[2] as f64 * dir.z) / d;
        assert!(dot > 0.9999, "Sirius must lie in Sirius's direction (cos {dot:.5})");
        assert!(s.color[2] >= s.color[0], "Sirius is blue-white, got {:?}", s.color);

        // **The apparent magnitude is DERIVED, not stored** — luminosity and true distance must give back
        // the −1.44 we measure from here. This is the whole claim of storing positions.
        let flux = apparent_flux(s.luminosity as f64, d);
        let mag = -2.5 * flux.log10();
        assert!((mag + 1.44).abs() < 0.05, "Sirius must come out at mag −1.44 from Sol, got {mag:.2}");

        // Both hemispheres populated — a sign error in one axis would pile the sky into half the sphere.
        let (mut north, mut south) = (0, 0);
        for st in &stars {
            if st.pos_pc[1] > 0.0 { north += 1 } else { south += 1 }
        }
        assert!(north > 3000 && south > 3000, "both hemispheres populated (N {north}, S {south})");
    }

    /// **The reason there is no sky sphere.** Move the observer and the sky must actually change: a star
    /// you have travelled toward gets brighter and its direction swings. A shell painted at infinity can
    /// do neither — it would drag along with the camera for ever.
    #[test]
    fn the_sky_changes_when_the_observer_moves() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/sky/stars.bin")).unwrap();
        let stars = parse_catalog(&bytes).unwrap();
        let sirius = stars[0];
        let p = [sirius.pos_pc[0] as f64, sirius.pos_pc[1] as f64, sirius.pos_pc[2] as f64];
        let d_sol = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();

        // Fly 90% of the way to Sirius. It must brighten by ~100× (inverse square over 1/10 the distance).
        let obs = [p[0] * 0.9, p[1] * 0.9, p[2] * 0.9];
        let d_near = ((p[0] - obs[0]).powi(2) + (p[1] - obs[1]).powi(2) + (p[2] - obs[2]).powi(2)).sqrt();
        let ratio = apparent_flux(sirius.luminosity as f64, d_near)
            / apparent_flux(sirius.luminosity as f64, d_sol);
        assert!((ratio - 100.0).abs() < 1.0, "10× closer ⇒ 100× brighter, got {ratio:.1}×");

        // And from out there, some OTHER star is the brightest in the sky — the constellations are not
        // ours any more. (Sol itself would be one of them; it is not in this file by design.)
        let brightest_from_there = stars
            .iter()
            .map(|s| {
                let dd = ((s.pos_pc[0] as f64 - obs[0]).powi(2)
                    + (s.pos_pc[1] as f64 - obs[1]).powi(2)
                    + (s.pos_pc[2] as f64 - obs[2]).powi(2))
                .sqrt();
                apparent_flux(s.luminosity as f64, dd)
            })
            .fold(0.0f64, f64::max);
        let sirius_from_there = apparent_flux(sirius.luminosity as f64, d_near);
        assert!(
            (brightest_from_there - sirius_from_there).abs() < 1e-9,
            "standing next to Sirius, Sirius should dominate the sky"
        );

        // Direction changes too: a star at right angles swings visibly once you have moved parsecs.
        let far = stars.iter().find(|s| {
            let dd = (s.pos_pc[0] as f64 * p[0] + s.pos_pc[1] as f64 * p[1] + s.pos_pc[2] as f64 * p[2]) / d_sol;
            dd.abs() < 0.1
        }).expect("some star lies near a right angle to Sirius");
        let unit = |v: [f64; 3]| { let n = (v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt(); [v[0]/n, v[1]/n, v[2]/n] };
        let from_sol = unit([far.pos_pc[0] as f64, far.pos_pc[1] as f64, far.pos_pc[2] as f64]);
        let from_there = unit([far.pos_pc[0] as f64 - obs[0], far.pos_pc[1] as f64 - obs[1], far.pos_pc[2] as f64 - obs[2]]);
        let cos = from_sol[0]*from_there[0] + from_sol[1]*from_there[1] + from_sol[2]*from_there[2];
        assert!(cos < 0.99999, "the sky must not be rigid: that star's direction moved (cos {cos:.6})");
    }

    /// A malformed catalogue must fail loudly rather than render half a sky.
    #[test]
    fn a_truncated_catalogue_is_an_error_not_a_partial_sky() {
        assert!(parse_catalog(&[0u8; 21]).is_err(), "21 bytes is not whole 20-byte records");
        assert!(parse_catalog(&[]).is_err(), "an empty catalogue is not a sky");
        assert!(parse_catalog(&[0u8; 40]).is_ok(), "two whole records parse");
    }

    /// Quantify the "stars are effectively immobile" claim, because an engine should hold the RULE and
    /// not a hardcoded interval. The numbers here are what justify recomputing the sky every frame
    /// without a cache — and what would justify a cache if the catalogue ever grew enough to matter.
    #[test]
    fn the_sky_holds_still_for_decades_at_any_speed_we_can_reach() {
        const NEAREST_PC: f64 = 1.34; // Alpha Centauri, the nearest naked-eye star
        const ONE_PIXEL: f64 = 0.9 / 800.0; // the engine's FOV over a typical viewport
        let years = |v: f64| sky_fixed_for_seconds(NEAREST_PC, v, ONE_PIXEL) / 3.15576e7;

        // At solar escape velocity the nearest star drifts one pixel in ~35 YEARS.
        let escape = years(4.2e4);
        assert!((30.0..40.0).contains(&escape), "≈35 years at solar escape, got {escape:.1}");
        // Even at 1% of light speed it is months, not frames.
        assert!(years(3.0e6) > 0.4, "≈6 months at 0.01c, got {:.2} years", years(3.0e6));
        // Slower observer ⇒ the sky holds still longer; a stationary one, for ever.
        assert!(years(1.7e4) > escape, "Voyager's pace holds longer than escape velocity");
        assert!(sky_fixed_for_seconds(NEAREST_PC, 0.0, ONE_PIXEL).is_infinite(), "standing still ⇒ no parallax");
        // A more distant star holds still proportionally longer — the rule is v/d, nothing else.
        assert!(
            (sky_fixed_for_seconds(13.4, 4.2e4, ONE_PIXEL) / sky_fixed_for_seconds(1.34, 4.2e4, ONE_PIXEL) - 10.0).abs() < 1e-9,
            "ten times farther ⇒ ten times longer"
        );
    }

    /// Sidereal time must advance one full turn per SIDEREAL day (23h56m04s), not per solar day — that
    /// four-minute difference is why the stars rise earlier each night.
    #[test]
    fn sidereal_time_turns_once_per_sidereal_day() {
        const SIDEREAL_DAY_S: f64 = 86_164.0905;
        let t0 = 1_718_884_800.0;
        let a = gmst_rad(t0);
        let b = gmst_rad(t0 + SIDEREAL_DAY_S);
        let drift = (b - a).rem_euclid(std::f64::consts::TAU);
        let err = drift.min(std::f64::consts::TAU - drift);
        assert!(err < 1e-4, "one sidereal day is one full turn (off by {err:.2e} rad)");
        // ...and a SOLAR day leaves it ~1° short of a turn (360/365.25).
        let c = gmst_rad(t0 + 86_400.0);
        let solar = (c - a).rem_euclid(std::f64::consts::TAU).to_degrees();
        assert!((solar - 0.9856).abs() < 0.01, "a solar day over-turns by ~0.986°, got {solar:.4}°");
    }
}
