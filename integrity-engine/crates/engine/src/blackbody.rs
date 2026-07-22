//! **Colour from temperature — Planck's law through the human eye.**
//!
//! A star's colour is not a look-up table: it is a blackbody spectrum integrated against the CIE 1931
//! colour-matching functions and converted to sRGB primaries. The catalogue gives us what was MEASURED
//! (the colour index B−V); everything visible follows from physics here, which is why `stars.bin` ships no
//! RGB. Baking colours into the asset would make the sky a picture instead of a consequence.
//!
//! This also gives the engine a real blackbody colour for the first time. The existing `incandescence`
//! ramp (a linear fade to white above 3,200 K) was built for glowing rock and cannot describe a star:
//! every star from a 3,000 K red dwarf to a 30,000 K blue giant would come out white. Retiring that ramp
//! in favour of this is a flagged follow-up — hot ejecta should glow by the same law as a star.

/// Planck spectral radiance (W·m⁻³·sr⁻¹) for wavelength `lambda_m` at temperature `t_k`.
pub fn planck(lambda_m: f64, t_k: f64) -> f64 {
    const H: f64 = 6.626_070_15e-34; // Planck constant, J·s (SI exact)
    const C: f64 = 2.997_924_58e8; // speed of light, m/s (SI exact)
    const KB: f64 = 1.380_649e-23; // Boltzmann constant, J/K (SI exact)
    let l5 = lambda_m.powi(5);
    let exponent = H * C / (lambda_m * KB * t_k);
    // exp() overflows for short wavelengths at low temperature; that tail is physically ~0 anyway.
    if exponent > 700.0 {
        return 0.0;
    }
    (2.0 * H * C * C) / (l5 * (exponent.exp() - 1.0))
}

/// A piecewise-Gaussian lobe: σ differs either side of the peak. The shape Wyman et al. fit the CIE
/// observer with.
fn lobe(x: f64, mu: f64, s1: f64, s2: f64) -> f64 {
    let s = if x < mu { s1 } else { s2 };
    let t = (x - mu) / s;
    (-0.5 * t * t).exp()
}

/// The CIE 1931 2° standard observer (x̄, ȳ, z̄) at wavelength `nm`, from the multi-lobe analytic fit of
/// Wyman, Sloan & Shirley (2013), "Simple Analytic Approximations to the CIE XYZ Color Matching
/// Functions" (JCGT 2:2). Accurate to a fraction of a percent — far below anything a viewer resolves, and
/// it avoids shipping a 471-row table.
pub fn cie_observer(nm: f64) -> (f64, f64, f64) {
    let x = 1.056 * lobe(nm, 599.8, 37.9, 31.0) + 0.362 * lobe(nm, 442.0, 16.0, 26.7)
        - 0.065 * lobe(nm, 501.1, 20.4, 26.2);
    let y = 0.821 * lobe(nm, 568.8, 46.9, 40.5) + 0.286 * lobe(nm, 530.9, 16.3, 31.1);
    let z = 1.217 * lobe(nm, 437.0, 11.8, 36.0) + 0.681 * lobe(nm, 459.0, 26.0, 13.8);
    (x, y, z)
}

/// The colour of a blackbody at `t_k`, as LINEAR sRGB normalised so the strongest channel is 1.
///
/// Normalised because a star's brightness comes from its magnitude, not its temperature — this answers
/// "what colour", and the renderer answers "how bright". Out-of-gamut negatives (very hot or very cold
/// bodies fall outside the sRGB triangle) are clipped to the gamut edge, which is a display limit, not a
/// physical claim.
pub fn blackbody_srgb(t_k: f64) -> [f32; 3] {
    if t_k <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    // 5 nm steps across the visible band — the CMFs are ~0 outside it.
    let mut nm = 360.0;
    while nm <= 830.0 {
        let radiance = planck(nm * 1e-9, t_k);
        let (bx, by, bz) = cie_observer(nm);
        x += radiance * bx;
        y += radiance * by;
        z += radiance * bz;
        nm += 5.0;
    }
    let sum = x + y + z;
    if sum <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    // Chromaticity only — discard the absolute scale, which is the magnitude's job.
    let (x, y, z) = (x / sum, y / sum, z / sum);
    // CIE XYZ -> linear sRGB (IEC 61966-2-1 primaries, D65).
    let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let b = 0.0557 * x - 0.2040 * y + 1.0570 * z;
    let mut rgb = [r.max(0.0), g.max(0.0), b.max(0.0)];
    let peak = rgb[0].max(rgb[1]).max(rgb[2]);
    if peak > 0.0 {
        for c in &mut rgb {
            *c /= peak;
        }
    }
    [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32]
}

/// Effective temperature (K) from the colour index B−V, by Ballesteros (2012), EPL 97, 34008 — derived by
/// treating the star as a blackbody seen through the B and V passbands, so it is the same physics as
/// [`blackbody_srgb`] read backwards. Valid across the main sequence; it is a two-band estimate, not a
/// spectral fit (FLAGGED — a spectral-type table is the refinement for peculiar stars).
pub fn temperature_from_bv(bv: f64) -> f64 {
    4600.0 * (1.0 / (0.92 * bv + 1.70) + 1.0 / (0.92 * bv + 0.62))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The B−V → temperature law has one check everybody can verify: our own star. The Sun's measured
    /// B−V is 0.65 and its photosphere is 5,772 K — the same number `assets/bodies/sun.json` declares.
    #[test]
    fn the_suns_colour_index_recovers_the_suns_temperature() {
        let t = temperature_from_bv(0.65);
        assert!((t - 5772.0).abs() < 60.0, "B−V 0.65 must give ~5772 K, got {t:.0} K");
        // Real stars, real colour indices, real temperatures (within the two-band method's accuracy).
        let vega = temperature_from_bv(0.00); // A0V, ~9,600 K
        assert!((8_500.0..11_500.0).contains(&vega), "Vega (B−V 0) ≈ 9,600 K, got {vega:.0}");
        let betelgeuse = temperature_from_bv(1.85); // M1-2Ia, ~3,600 K
        assert!((3_000.0..4_200.0).contains(&betelgeuse), "Betelgeuse (B−V 1.85) ≈ 3,600 K, got {betelgeuse:.0}");
        // Bluer is always hotter — the relation must be monotonic or the sky's colours scramble.
        let mut prev = f64::INFINITY;
        for i in 0..=40 {
            let t = temperature_from_bv(-0.3 + i as f64 * 0.05);
            assert!(t < prev, "temperature must fall as B−V rises");
            prev = t;
        }
    }

    /// Planck + the CIE observer must reproduce the colours the sky actually has.
    #[test]
    fn blackbody_colour_matches_the_stars_we_can_see() {
        // The Sun is WHITE. Its spectrum peaks in the green and the integral lands near the white point —
        // the yellow sun is what our atmosphere does to it, from underneath.
        let sun = blackbody_srgb(5772.0);
        let spread = sun.iter().cloned().fold(0.0f32, f32::max) - sun.iter().cloned().fold(1.0f32, f32::min);
        assert!(spread < 0.30, "the Sun should be near-white, got {sun:?} (spread {spread:.2})");

        // A cool red giant is red-dominant; a hot blue giant is blue-dominant.
        let cool = blackbody_srgb(3000.0);
        assert!(cool[0] > cool[2] * 1.5, "3,000 K must be red-dominant, got {cool:?}");
        let hot = blackbody_srgb(20000.0);
        assert!(hot[2] > hot[0] * 1.1, "20,000 K must be blue-dominant, got {hot:?}");

        // Colour must shift monotonically from red toward blue as temperature climbs — no wobbles.
        let ratio = |t: f64| { let c = blackbody_srgb(t); c[2] / c[0].max(1e-6) };
        let mut prev = 0.0;
        for t in [2000.0, 3000.0, 4000.0, 5000.0, 6500.0, 8000.0, 12000.0, 20000.0, 30000.0] {
            let r = ratio(t);
            assert!(r > prev, "blue/red must rise with temperature (at {t} K: {r:.3} vs {prev:.3})");
            prev = r;
        }
        // Every channel stays in range, and the brightest is exactly 1 (chromaticity, not brightness).
        for t in [1500.0, 5772.0, 40000.0] {
            let c = blackbody_srgb(t);
            assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "in gamut at {t} K: {c:?}");
            assert!((c.iter().cloned().fold(0.0f32, f32::max) - 1.0).abs() < 1e-6, "normalised at {t} K");
        }
    }

    /// Planck's law itself: Wien's displacement is the check that needs no reference data.
    #[test]
    fn planck_obeys_wiens_displacement_law() {
        for t in [3000.0, 5772.0, 12000.0] {
            // Find the peak by scanning; it must sit at b/T with b = 2.898e-3 m·K.
            let mut best = (0.0, 0.0);
            let mut nm = 50.0;
            while nm < 4000.0 {
                let v = planck(nm * 1e-9, t);
                if v > best.1 { best = (nm, v); }
                nm += 0.5;
            }
            let expected_nm = 2.897_771_955e-3 / t * 1e9;
            assert!(
                (best.0 - expected_nm).abs() < expected_nm * 0.01,
                "Wien peak at {t} K: got {:.1} nm, expected {expected_nm:.1} nm", best.0
            );
        }
    }
}

/// **How brightly a surface glows because of its own heat**, as a multiple of the radiance of a sunlit
/// white surface at 1 AU (~430 W·m⁻²·sr⁻¹ — the reference the scenes' exposure is built on).
///
/// Stefan–Boltzmann, divided by π for radiance and by that reference: nothing chosen. It is what lets the
/// engine render a magma ocean from the ONE thing it needs to know — the temperature — instead of being
/// handed a picture of one. Below the visible-glow floor it returns 0, so a cold planet costs nothing.
///
/// The numbers are worth knowing before looking at the result: proto-Earth's declared 1,900 K surface
/// emits ~547× a sunlit white surface, and about 4,000× what its own sunlit rock reflects. A magma ocean
/// outshines its own daylight, so it has no day/night terminator at all — it glows all over.
pub fn thermal_glow_gain(t_k: f64) -> f64 {
    const SIGMA: f64 = 5.670_374_419e-8; // Stefan–Boltzmann, W·m⁻²·K⁻⁴
    const SUNLIT_WHITE_RADIANCE: f64 = 430.0; // W·m⁻²·sr⁻¹ at 1 AU — the exposure's reference point
    if t_k <= 800.0 {
        return 0.0; // below visible incandescence
    }
    (SIGMA * t_k.powi(4) / std::f64::consts::PI) / SUNLIT_WHITE_RADIANCE
}

#[cfg(test)]
mod glow_tests {
    use super::*;

    /// A body glows because it is hot, and how much is not a choice. These are the numbers that decide
    /// whether a magma ocean reads as a magma ocean.
    #[test]
    fn thermal_glow_follows_stefan_boltzmann() {
        assert_eq!(thermal_glow_gain(288.0), 0.0, "modern Earth's surface does not glow");
        assert_eq!(thermal_glow_gain(800.0), 0.0, "the visible-glow floor");

        // Proto-Earth's declared magma ocean, against the exposure's own reference.
        let magma = thermal_glow_gain(1900.0);
        assert!((500.0..600.0).contains(&magma), "1,900 K glows ~547× a sunlit white surface, got {magma:.0}");

        // T⁴, exactly: double the temperature, sixteen times the glow.
        let a = thermal_glow_gain(1500.0);
        let b = thermal_glow_gain(3000.0);
        assert!((b / a - 16.0).abs() < 0.01, "Stefan–Boltzmann is T⁴ (got {:.2}×)", b / a);

        // And the colour comes from the same temperature, through Planck.
        let c = blackbody_srgb(1900.0);
        assert!(c[0] > c[1] && c[1] > c[2], "1,900 K is orange: red > green > blue, got {c:?}");
    }
}

/// **The display law, in Rust — the reference for `shaders/tonemap.wgsl`.**
///
/// Compress LUMINANCE and carry chromaticity through unchanged. The per-channel form every shader used
/// (`radiance / (1 + radiance)`) walks each channel toward 1 independently, so a bright coloured surface
/// loses its hue — and the brighter and more saturated it is, the more wrong it gets.
///
/// WGSL cannot call this, so the two are kept in step by hand and this side carries the test. Any change
/// to one is a change to both.
pub fn tonemap(radiance: [f64; 3]) -> [f64; 3] {
    let l = 0.2126 * radiance[0] + 0.7152 * radiance[1] + 0.0722 * radiance[2];
    if l <= 0.0 {
        return [0.0; 3];
    }
    let compressed = l / (1.0 + l);
    let k = compressed / l;
    [
        (radiance[0] * k).min(1.0),
        (radiance[1] * k).min(1.0),
        (radiance[2] * k).min(1.0),
    ]
}

#[cfg(test)]
mod tonemap_tests {
    use super::*;

    /// **A hot surface must keep its colour.** proto-Earth's magma ocean is the case that exposed this:
    /// Planck gives 1,900 K as linear sRGB (1.000, 0.243, 0.000) — a deep orange — and at the radiance it
    /// actually emits the per-channel Reinhard returned (1.000, 1.000, 0.000), which is YELLOW. Green
    /// saturating alongside red invented a colour the object does not have.
    #[test]
    fn brightness_is_compressed_but_hue_is_not_invented() {
        let magma = blackbody_srgb(1900.0).map(|v| v as f64);
        assert!(magma[0] > 0.9 && magma[1] < 0.4 && magma[2] < 0.05, "1,900 K is deep orange: {magma:?}");

        // At the radiance a magma ocean really emits, against the scene's sunlit exposure.
        let gain = thermal_glow_gain(1900.0) * 22.0;
        let radiance = magma.map(|v| v * gain);

        // The old per-channel form, for the record: green saturates and the hue is gone.
        let per_channel = radiance.map(|v| v / (1.0 + v));
        assert!(per_channel[1] > 0.99, "per channel, green saturates too ({:.3})", per_channel[1]);

        // The shared law keeps the ratio the object actually has.
        let out = tonemap(radiance);
        assert!(out[1] < 0.75, "green must NOT saturate: {out:?}");
        assert!(out[0] > out[1] && out[1] > out[2], "still orange after tone-mapping: {out:?}");
        // Chromaticity preserved where the gamut allows: G/R must survive the compression.
        let before = magma[1] / magma[0];
        let after = out[1] / out[0];
        assert!((after - before).abs() < 0.4, "hue roughly preserved ({before:.2} -> {after:.2})");

        // Dim things are essentially untouched — this is not a look, it is a limit.
        let dim = [0.02, 0.01, 0.005];
        let t = tonemap(dim);
        for i in 0..3 {
            assert!((t[i] - dim[i]).abs() < 0.02 * dim[i].max(1e-6) + 1e-3, "dim values pass through");
        }
        // Monotonic in brightness, and black stays black.
        assert_eq!(tonemap([0.0; 3]), [0.0; 3]);
        let a = tonemap([1.0, 0.5, 0.2]);
        let b = tonemap([2.0, 1.0, 0.4]);
        assert!(b[0] > a[0], "brighter input, brighter output");
        // Everything stays inside the display range.
        for v in tonemap([1e6, 1e5, 1e4]) {
            assert!((0.0..=1.0).contains(&v), "in range");
        }
    }
}
