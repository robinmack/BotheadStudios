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
/// **Stefan–Boltzmann constant** σ (W·m⁻²·K⁻⁴), CODATA 2018 — exact, since it is fixed by the SI
/// definitions of k_B, h and c. THE one value: a body radiating σT⁴ is the same law whether it is a
/// cooling moonlet, an ablating meteor or a shed vapour parcel, and this module is where radiation lives.
/// (It was written out three times, once as a truncated `5.670e-8` — one question, two answers.)
pub const SIGMA: f64 = 5.670_374_419e-8;

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

/// CIE XYZ → linear sRGB (IEC 61966-2-1 primaries, D65). Written once, because a colour space is
/// exactly the kind of thing that gets typed out twice with one digit different.
pub fn xyz_to_linear_srgb(x: f64, y: f64, z: f64) -> [f64; 3] {
    [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
}

/// **The colour of a SURFACE, from what a spectrometer measured it reflect.**
///
/// `blackbody_srgb` answers "what colour is light of this temperature". This answers the other half:
/// given a surface that returns `reflectance[i]` of the light at `lo_nm + i*step_nm`, and an
/// illuminant that is a blackbody at `illuminant_k`, what fraction does each of the render's three
/// channels get back? It is the same CIE observer and the same primaries — one question, one answer.
///
/// Per channel: `∫R(λ)·S(λ)·c̄(λ)dλ ÷ ∫S(λ)·c̄(λ)dλ`. The denominator is the illuminant seen by the
/// same observer, so it is a REFLECTANCE and not a radiance: a surface that returns everything comes
/// back `[1,1,1]` whatever the illuminant, and one that returns half comes back `[0.5,0.5,0.5]`.
/// Those two are the tests.
///
/// ★ Why this exists: `Material::albedo`'s own doc calls itself *"a stand-in for the full spectral …
/// optics … a placeholder to be grounded later"*. A material carrying a measured spectrum is that
/// grounding — its three numbers stop being chosen and become a convolution of a measurement. It is
/// also what phenology needs, since a leaf turning in autumn is a change of SPECTRUM (chlorophyll
/// degrading, carotenoids unmasked) and only derivatively a change of colour.
///
/// Outside the sampled range the surface is treated as reflecting nothing, so the range must cover
/// the visible band; the observer is ~0 beyond 380–780 nm, which is what the samples should span.
pub fn reflectance_srgb(
    reflectance: &[f64],
    lo_nm: f64,
    step_nm: f64,
    illuminant_k: f64,
) -> [f32; 3] {
    if reflectance.is_empty() || step_nm <= 0.0 || illuminant_k <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let (mut rx, mut ry, mut rz) = (0.0, 0.0, 0.0); // reflected
    let (mut wx, mut wy, mut wz) = (0.0, 0.0, 0.0); // the illuminant itself (the white point)
    for (i, &r) in reflectance.iter().enumerate() {
        let nm = lo_nm + step_nm * i as f64;
        let s = planck(nm * 1e-9, illuminant_k);
        let (cx, cy, cz) = cie_observer(nm);
        rx += r * s * cx;
        ry += r * s * cy;
        rz += r * s * cz;
        wx += s * cx;
        wy += s * cy;
        wz += s * cz;
    }
    let refl = xyz_to_linear_srgb(rx, ry, rz);
    let white = xyz_to_linear_srgb(wx, wy, wz);
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        // A negative white channel would mean the illuminant is outside the sRGB gamut, which no
        // blackbody in the visible is; guard anyway rather than divide by it.
        out[c] = if white[c] > 0.0 {
            (refl[c] / white[c]).max(0.0) as f32
        } else {
            0.0
        };
    }
    out
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
    let rgb = xyz_to_linear_srgb(x, y, z);
    let mut rgb = [rgb[0].max(0.0), rgb[1].max(0.0), rgb[2].max(0.0)];
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
        assert!(
            (t - 5772.0).abs() < 60.0,
            "B−V 0.65 must give ~5772 K, got {t:.0} K"
        );
        // Real stars, real colour indices, real temperatures (within the two-band method's accuracy).
        let vega = temperature_from_bv(0.00); // A0V, ~9,600 K
        assert!(
            (8_500.0..11_500.0).contains(&vega),
            "Vega (B−V 0) ≈ 9,600 K, got {vega:.0}"
        );
        let betelgeuse = temperature_from_bv(1.85); // M1-2Ia, ~3,600 K
        assert!(
            (3_000.0..4_200.0).contains(&betelgeuse),
            "Betelgeuse (B−V 1.85) ≈ 3,600 K, got {betelgeuse:.0}"
        );
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
        let spread =
            sun.iter().cloned().fold(0.0f32, f32::max) - sun.iter().cloned().fold(1.0f32, f32::min);
        assert!(
            spread < 0.30,
            "the Sun should be near-white, got {sun:?} (spread {spread:.2})"
        );

        // A cool red giant is red-dominant; a hot blue giant is blue-dominant.
        let cool = blackbody_srgb(3000.0);
        assert!(
            cool[0] > cool[2] * 1.5,
            "3,000 K must be red-dominant, got {cool:?}"
        );
        let hot = blackbody_srgb(20000.0);
        assert!(
            hot[2] > hot[0] * 1.1,
            "20,000 K must be blue-dominant, got {hot:?}"
        );

        // Colour must shift monotonically from red toward blue as temperature climbs — no wobbles.
        let ratio = |t: f64| {
            let c = blackbody_srgb(t);
            c[2] / c[0].max(1e-6)
        };
        let mut prev = 0.0;
        for t in [
            2000.0, 3000.0, 4000.0, 5000.0, 6500.0, 8000.0, 12000.0, 20000.0, 30000.0,
        ] {
            let r = ratio(t);
            assert!(
                r > prev,
                "blue/red must rise with temperature (at {t} K: {r:.3} vs {prev:.3})"
            );
            prev = r;
        }
        // Every channel stays in range, and the brightest is exactly 1 (chromaticity, not brightness).
        for t in [1500.0, 5772.0, 40000.0] {
            let c = blackbody_srgb(t);
            assert!(
                c.iter().all(|v| (0.0..=1.0).contains(v)),
                "in gamut at {t} K: {c:?}"
            );
            assert!(
                (c.iter().cloned().fold(0.0f32, f32::max) - 1.0).abs() < 1e-6,
                "normalised at {t} K"
            );
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
                if v > best.1 {
                    best = (nm, v);
                }
                nm += 0.5;
            }
            let expected_nm = 2.897_771_955e-3 / t * 1e9;
            assert!(
                (best.0 - expected_nm).abs() < expected_nm * 0.01,
                "Wien peak at {t} K: got {:.1} nm, expected {expected_nm:.1} nm",
                best.0
            );
        }
    }

    /// **A reflectance is a FRACTION, so the illuminant must cancel out of it.**
    ///
    /// The two cases that pin `reflectance_srgb` are the ones with an answer known in advance: a
    /// surface returning everything is `[1,1,1]`, a surface returning half is `[0.5,0.5,0.5]`, and
    /// both must hold under ANY illuminant. If the white-point division were wrong — or missing —
    /// a white surface would come back the colour of the lamp, which is exactly the bug that makes a
    /// renderer's "albedo" secretly a radiance.
    #[test]
    fn a_perfect_reflector_is_white_under_any_sun() {
        for t in [3000.0, 5772.0, 20000.0] {
            let white = reflectance_srgb(&[1.0; 81], 380.0, 5.0, t);
            for (c, &v) in white.iter().enumerate() {
                assert!(
                    (v - 1.0).abs() < 1e-3,
                    "a perfect reflector under {t} K: channel {c} = {v}, want 1"
                );
            }
            let grey = reflectance_srgb(&[0.5; 81], 380.0, 5.0, t);
            for (c, &v) in grey.iter().enumerate() {
                assert!(
                    (v - 0.5).abs() < 1e-3,
                    "a half reflector under {t} K: channel {c} = {v}, want 0.5"
                );
            }
        }
        // And linearity in the reflectance, which is what makes it a fraction rather than a curve.
        let a = reflectance_srgb(&[0.2; 81], 380.0, 5.0, 5772.0);
        let b = reflectance_srgb(&[0.4; 81], 380.0, 5.0, 5772.0);
        for c in 0..3 {
            assert!((b[c] - 2.0 * a[c]).abs() < 1e-4, "channel {c} is linear");
        }
    }

    /// **A green surface must come back green, and the greenness must be the SPECTRUM's doing.**
    ///
    /// A band that reflects only 500–600 nm has to land with G the largest channel. This is the
    /// property the foliage materials depend on: nothing anywhere chooses that leaves are green —
    /// chlorophyll absorbs the red and the blue, and the observer reports what is left.
    #[test]
    fn the_channel_that_wins_is_the_one_the_spectrum_favours() {
        let band = |lo: f64, hi: f64| -> [f32; 3] {
            let s: Vec<f64> = (0..81)
                .map(|i| {
                    let nm = 380.0 + 5.0 * i as f64;
                    if nm >= lo && nm <= hi {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
            reflectance_srgb(&s, 380.0, 5.0, 5772.0)
        };
        let green = band(500.0, 600.0);
        assert!(
            green[1] > green[0] && green[1] > green[2],
            "500-600 nm should be green-dominant, got {green:?}"
        );
        let red = band(600.0, 700.0);
        assert!(
            red[0] > red[1] && red[0] > red[2],
            "600-700 nm should be red-dominant, got {red:?}"
        );
        let blue = band(400.0, 490.0);
        assert!(
            blue[2] > blue[0] && blue[2] > blue[1],
            "400-490 nm should be blue-dominant, got {blue:?}"
        );
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
        assert_eq!(
            thermal_glow_gain(288.0),
            0.0,
            "modern Earth's surface does not glow"
        );
        assert_eq!(thermal_glow_gain(800.0), 0.0, "the visible-glow floor");

        // Proto-Earth's declared magma ocean, against the exposure's own reference.
        let magma = thermal_glow_gain(1900.0);
        assert!(
            (500.0..600.0).contains(&magma),
            "1,900 K glows ~547× a sunlit white surface, got {magma:.0}"
        );

        // T⁴, exactly: double the temperature, sixteen times the glow.
        let a = thermal_glow_gain(1500.0);
        let b = thermal_glow_gain(3000.0);
        assert!(
            (b / a - 16.0).abs() < 0.01,
            "Stefan–Boltzmann is T⁴ (got {:.2}×)",
            b / a
        );

        // And the colour comes from the same temperature, through Planck.
        let c = blackbody_srgb(1900.0);
        assert!(
            c[0] > c[1] && c[1] > c[2],
            "1,900 K is orange: red > green > blue, got {c:?}"
        );
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
        assert!(
            magma[0] > 0.9 && magma[1] < 0.4 && magma[2] < 0.05,
            "1,900 K is deep orange: {magma:?}"
        );

        // At the radiance a magma ocean really emits, against the scene's sunlit exposure.
        let gain = thermal_glow_gain(1900.0) * 22.0;
        let radiance = magma.map(|v| v * gain);

        // The old per-channel form, for the record: green saturates and the hue is gone.
        let per_channel = radiance.map(|v| v / (1.0 + v));
        assert!(
            per_channel[1] > 0.99,
            "per channel, green saturates too ({:.3})",
            per_channel[1]
        );

        // The shared law keeps the ratio the object actually has.
        let out = tonemap(radiance);
        assert!(out[1] < 0.75, "green must NOT saturate: {out:?}");
        assert!(
            out[0] > out[1] && out[1] > out[2],
            "still orange after tone-mapping: {out:?}"
        );
        // Chromaticity preserved where the gamut allows: G/R must survive the compression.
        let before = magma[1] / magma[0];
        let after = out[1] / out[0];
        assert!(
            (after - before).abs() < 0.4,
            "hue roughly preserved ({before:.2} -> {after:.2})"
        );

        // Dim things are essentially untouched — this is not a look, it is a limit.
        let dim = [0.02, 0.01, 0.005];
        let t = tonemap(dim);
        for i in 0..3 {
            assert!(
                (t[i] - dim[i]).abs() < 0.02 * dim[i].max(1e-6) + 1e-3,
                "dim values pass through"
            );
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
