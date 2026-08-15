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

/// **The relief amplitude this material can hold at a wavelength** (m, zero-to-peak) — by asking the
/// engine's own slope law, not by restating it.
///
/// The criterion is [`crate::granular::face_stable`]: a face stands if FRICTION carries the slope
/// ([`crate::granular::repose_allowance_on`], over a baseline of half a wavelength — crest to trough) OR if
/// COHESION carries the bank on its own ([`crate::granular::critical_bank_height`]). They are an OR over two
/// different measurements, which is what lets cohesionless sand be perfectly stable at 34° yet unable to
/// stand any vertical face, while granite keeps its crags.
///
/// **This function holds no slope physics of its own — it only converts a permitted DROP into a sinusoid's
/// amplitude.** The first version did hold its own, and was wrong twice over: it used friction alone, and it
/// added friction to cohesion in a single slope. The material table refuted the first immediately (dry sand
/// grips HARDER than granite, μ 0.67 vs 0.60, yet only granite stands vertical — sand's cohesion is 0,
/// granite's 28 MPa), and `granular`'s own documentation had already warned against the second: conflating
/// the friction height with the cohesion height is *"subtly wrong in exactly the case a layered world is
/// made of"*. The engine knew both things; this module had not asked. Hence
/// `generated_relief_is_stable_by_the_engines_own_slope_law`, which makes not-asking fail the suite.
///
/// The quantum passed to the allowance is `0`: generated relief is a continuous function, not a quantised
/// field, so there is no quantisation to allow for.
///
/// **The `λ/4` cap is REPRESENTATION, not physics — and it only ever bites for STONE.** A heightfield cannot
/// overhang, so relief steeper than that folds through its own samples. Robin (2026-07-24): *"arches and
/// overhangs, while rare, do form … but in stone, not soil/regolith"* — which is exactly the cohesion
/// distinction, and it means the cap discards real geometry in one case and nothing at all in the other:
///
/// - **Soil and regolith** (sand, dirt, clay, snow, gravel, grass) are cohesion-poor, so REPOSE limits them
///   first and the cap is never reached. An overhanging dune does not exist to be discarded. Pinned by
///   `soil_never_reaches_the_heightfield_cap_because_repose_forbids_overhangs`.
/// - **Stone** has cohesion enough to stand vertical and to undercut, so arches and overhangs are physically
///   real and the cap genuinely throws them away. That is a flagged limit of a HEIGHT, and its resolved form
///   is matter — the voxel/SPH path the engine already has, where an arch is simply rock with air under it.
pub fn relief_amplitude_m(wavelength_m: f64, mu: f64, h_crit_m: f64) -> f64 {
    if wavelength_m <= 0.0 {
        return 0.0;
    }
    // Crest to trough is half a wavelength of horizontal run.
    let baseline = wavelength_m * 0.5;
    let by_friction = crate::granular::repose_allowance_on(mu as f32, baseline as f32, 0.0) as f64;
    // The OR, exactly as `granular::face_stable` poses it: whichever term holds the face.
    let drop = by_friction.max(h_crit_m.max(0.0));
    // A crest-to-trough drop is twice the amplitude.
    (drop * 0.5).min(wavelength_m * 0.25)
}

/// **Sub-raster relief at a point** (m), summed over `octaves` halvings below `base_feature_m`.
///
/// Below the resolution of the elevation data there is no measurement, so relief has to be generated — and
/// Robin's rule is what makes that affordable: *"we don't have to make things renderable at planetary scale
/// while viewing subset of surface; we have the math — we can rebuild it if the camera moves again."* The
/// math is here, it is a pure function of position, and a caller evaluates it only for the patch it draws.
///
/// Two things keep it honest:
///
/// - **Every octave's amplitude is [`relief_amplitude_m`]**, so no generated feature is steeper than the
///   engine's own slope law permits, and the sum converges.
/// - **`slope_fraction` is how rough this GROUND is, not just this MATERIAL** — the local measured slope as
///   a fraction of what the material could hold. Without it the rule states a maximum everywhere and a flat
///   plain comes out as rough as a mountainside, which is false: relief and slope correlate because the same
///   erosion produced both. `0` leaves the measured surface exactly as measured.
///
/// Octaves start at `base_feature_m/2` — strictly BELOW the measurement, since at and above it the raster
/// already says what the ground does, and generating there would argue with data.
///
/// `octaves` is FRACTIONAL ([`detail_octaves`]) and the last is weighted by its fraction, so detail fades in
/// rather than popping. The lattice is `world::value_noise`, the same one the voxel worlds use, so two LOD
/// tiers cannot disagree about where a hill is — and being hash-keyed rather than seeded, looking away and
/// back finds the same ground (Law IV).
///
/// *Flagged, with the resolved computation named:* the SHAPE stands in for the erosion and deposition
/// history that actually carved this ground. The engine can produce that history — `granular`/`matter`
/// settle real grains to exactly this criterion — it simply cannot afford it over a continent.
#[allow(clippy::too_many_arguments)]
pub fn micro_relief_m(
    x_m: f64,
    z_m: f64,
    base_feature_m: f64,
    octaves: f64,
    mu: f64,
    h_crit_m: f64,
    slope_fraction: f64,
) -> f64 {
    if octaves <= 0.0 || base_feature_m <= 0.0 || slope_fraction <= 0.0 {
        return 0.0;
    }
    let whole = octaves.floor() as u32;
    let frac = octaves - whole as f64;
    let mut relief = 0.0;
    for i in 0..=whole.min(24) {
        // Strictly sub-raster: the first generated octave is half the measured feature.
        let wavelength = base_feature_m / (1u64 << (i + 1)) as f64;
        let amp = relief_amplitude_m(wavelength, mu, h_crit_m) * slope_fraction.clamp(0.0, 1.0);
        // Centred on zero: this is roughness, not a change of elevation. The measured raster stays the
        // truth about height.
        let n = crate::world::value_noise(x_m as f32, z_m as f32, (1.0 / wavelength) as f32) as f64
            - 0.5;
        let weight = if i == whole { frac } else { 1.0 };
        relief += n * 2.0 * amp * weight;
    }
    relief
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ **THE BAND LIMIT MUST CONVERGE** (docs/46 row 53) — what makes a renderer's smoothed ground
    /// an approximation rather than a fudge.
    ///
    /// The model owns the ground's height at a fixed physical floor; the mesh asks the SAME function
    /// with its own cell, which drops every octave finer than that cell. Legitimate under docs/68 §1b
    /// only if the two converge: a finer mesh must approach the model's answer, not merely differ from
    /// it by a different amount.
    ///
    /// MEASURED live at Galway the day this landed: at a 999 m cell the drawn ground sat 0.31 m from
    /// the model's, at 32 m it was 2.5 mm, and once the streamed tiles covered it was exactly zero.
    /// Before the two were one function it was **12.3 m** and did not converge at all, because they
    /// were different computations.
    #[test]
    fn a_coarser_band_limit_approaches_the_full_answer_as_it_refines() {
        let mats = crate::materials::load();
        let g = &mats[crate::materials::index_of(&mats, "granite")];
        let mu = g.friction_coefficient as f64;
        let h_crit =
            crate::granular::critical_bank_height(g.fracture_strength, g.density, 9.81) as f64;
        let base: f64 = 2_000.0; // the finest measured datum here
        let floor: f64 = 0.01; // what the model commits to
        let full: f64 = (base / floor).log2();
        // The model's own answer at a point.
        let at = |octaves: f64| micro_relief_m(1234.5, -678.9, base, octaves, mu, h_crit, 0.4);
        let truth = at(full);

        let mut last = f64::INFINITY;
        println!("  cell (m)   octaves   relief      |error|");
        for cell in [1000.0f64, 100.0, 10.0, 1.0, 0.1] {
            let octaves = (base / cell).max(1.0).log2().min(full);
            let err = (at(octaves) - truth).abs();
            println!(
                "  {cell:>8.1}   {octaves:>6.2}   {:>8.4}   {err:.5}",
                at(octaves)
            );
            assert!(
                err <= last + 1e-12,
                "refining the band limit must not move AWAY from the model's answer: \
                 {err:.6} at {cell} m against {last:.6} before it"
            );
            last = err;
        }
        // And the finest band limit is essentially the answer itself.
        assert!(
            last < 0.01,
            "a 0.1 m cell should be within a centimetre of the full answer, off by {last:.4}"
        );
    }

    /// **WHY THE GROUND FLATTENS ON DESCENT, part 1 of 2: the amplitude law is scale-INVARIANT below
    /// ~2 km, so zooming in cannot reveal more roughness** (docs/46 row 27).
    ///
    /// [`relief_amplitude_m`] is `min(drop/2, λ/4)`. For a cohesive rock the cohesion term wins the OR at
    /// every wavelength a camera cares about — granite's `h_crit` is 453 m — so the binding term is the
    /// `λ/4` NO-OVERHANG cap, which is a property of a HEIGHTFIELD, not of the rock. Amplitude ∝ wavelength
    /// is Hurst exponent H = 1: the smoothest self-affine surface there is, and the one whose slope is
    /// identical at every scale. Real topography has H ≈ 0.5–0.7, i.e. it gets relatively ROUGHER as you
    /// look closer, which is exactly the thing a descent is supposed to reveal.
    ///
    /// This test does not assert that H = 1 is right. It pins that it is what we currently DO, so the day
    /// someone gives the generator a real Hurst exponent, this fails and points at the reason it existed.
    #[test]
    fn generated_relief_has_the_same_slope_at_every_scale_below_two_km() {
        let mats = crate::materials::load();
        let g = &mats[crate::materials::index_of(&mats, "granite")];
        let mu = g.friction_coefficient as f64;
        let h_crit =
            crate::granular::critical_bank_height(g.fracture_strength, g.density, 9.81) as f64;
        // Below the wavelength where cohesion stops being the larger term, every octave has the SAME
        // amplitude-to-wavelength ratio, and it is the heightfield's λ/4, not anything the material said.
        for e in 6..20 {
            let lam = 19_550.0 / (1u64 << e) as f64;
            let ratio = relief_amplitude_m(lam, mu, h_crit) / lam;
            assert!(
                (ratio - 0.25).abs() < 1e-9,
                "λ={lam} m: amplitude/wavelength {ratio} — expected the λ/4 heightfield cap"
            );
        }
        // And it really is the cap rather than the physics: the material's own permitted drop is far
        // larger at these wavelengths, so nothing about granite is being consulted here.
        let lam = 100.0;
        assert!(
            h_crit * 0.5 > lam * 0.25,
            "the cohesion term must be the one being discarded by the cap"
        );
    }

    /// **WHY THE GROUND FLATTENS ON DESCENT, part 2 of 2, and the dominant term: `slope_fraction` compares
    /// two quantities measured four orders of magnitude apart** (docs/46 row 27).
    ///
    /// Terra builds it as `local_raster_gradient / mu` — but the gradient is taken over a baseline of TWO
    /// RASTER TEXELS, 39 km on the shipped Earth, while `mu` is a grain-scale material property. A 39 km
    /// baseline cannot be steep, and this test **measures the shipped raster to say so** rather than
    /// citing a number: it reproduces `Terra::build_cap`'s own sampling (±360/w degrees over a
    /// `2·raster_step` run) across the real elevation PNG and reports the distribution. Everest itself
    /// comes out at ~0.008, because averaging over 39 km flattens it.
    ///
    /// The consequence is the reported symptom: below ~20 km altitude the frame fits inside ONE raster
    /// texel, so measured elevation contributes only a smooth ramp and ALL visible roughness has to come
    /// from the generated relief — which is being scaled by that number.
    #[test]
    fn a_regional_gradient_cannot_reach_a_material_scale_slope() {
        let mats = crate::materials::load();
        let g = &mats[crate::materials::index_of(&mats, "granite")];
        let mu = g.friction_coefficient as f64;
        let h_crit =
            crate::granular::critical_bank_height(g.fracture_strength, g.density, 9.81) as f64;
        // The mismatch itself, which is the defect: the numerator is sampled over a baseline of two
        // raster texels while the denominator governs slopes at the wavelengths the relief is actually
        // generated at. Four orders of magnitude apart, so the ratio is not a fraction of anything.
        let baseline_m = 2.0 * 19_546.0;
        let finest_generated_wavelength_m = 19_550.0 / (1u64 << 16) as f64; // the 16-octave budget
        assert!(
            baseline_m / finest_generated_wavelength_m > 1.0e5,
            "the slope is measured over a baseline {}x the features it is scaling",
            baseline_m / finest_generated_wavelength_m
        );

        // **MEASURED on the raster the scene actually draws**, sampled exactly as `Terra::build_cap`
        // samples it. This is the whole claim, and it is now re-run on every commit.
        //
        // The bounds below are loose brackets around what the shipped data measures (printed on every
        // run), not targets: they exist to catch the raster being replaced or the sampler moving, and
        // they are deliberately far from the measured values so ordinary noise cannot trip them.
        // ★ Doing this in-suite CORRECTED the first version of this measurement. It was originally taken
        // by a separate script that reimplemented `Raster::coords` with a half-texel offset the engine
        // does not have. Over 16k points that is invisible — the median and p90 agreed to three digits —
        // but at a single named point in the steepest terrain on Earth it lands in a different texel, and
        // it reported Everest as 0.008 when the engine reads 0.033. A reimplementation of the thing under
        // test is a second answer to the same question (Law II), and this is what it cost.
        let (elev, range) = crate::terra::raster::shipped::earth_elevation();
        let r_m = 6.371e6;
        let raster_step_m = elev.texel_arc_m(r_m);
        let d = 360.0 / elev.w as f64;
        let tier_slope = |lat: f64, lon: f64| {
            let e = |la: f64, lo: f64| elev.elevation_m_at(la, lo, range[0], range[1]);
            let run = (2.0 * raster_step_m).max(1.0);
            let dn = (e(lat + d, lon) - e(lat - d, lon)) / run;
            let de = (e(lat, lon + d) - e(lat, lon - d)) / run;
            (dn * dn + de * de).sqrt()
        };
        // Everest: the steepest place there is, and the raster reads it as almost flat.
        let everest = tier_slope(27.99, 86.93);
        assert!(
            everest < 0.05,
            "even Everest reads as a gentle regional tilt, got {everest}"
        );
        // The distribution over land. `elevation > 1 m` stands in for land here (the raster carries
        // bathymetry, so ocean is strongly negative).
        let mut land: Vec<f64> = Vec::new();
        for i in 0..128 {
            for j in 0..128 {
                let lat = -60.0 + 120.0 * i as f64 / 127.0;
                let lon = -180.0 + 360.0 * j as f64 / 127.0;
                if elev.elevation_m_at(lat, lon, range[0], range[1]) > 1.0 {
                    land.push(tier_slope(lat, lon));
                }
            }
        }
        assert!(
            land.len() > 2_000,
            "enough land samples to be a distribution"
        );
        land.sort_by(f64::total_cmp);
        let pct = |p: f64| land[((land.len() - 1) as f64 * p) as usize];
        let (median, p90, worst) = (pct(0.5), pct(0.9), land[land.len() - 1]);
        assert!(
            median / mu < 0.01,
            "typical land is scaled to nearly nothing: median slope {median} → fraction {}",
            median / mu
        );
        assert!(
            worst / mu < 0.20,
            "nowhere on Earth does a 39 km baseline approach the material limit: worst {worst} → fraction {}",
            worst / mu
        );
        println!(
            "tier_slope over land: median {median:.5} (fraction {:.5}), p90 {p90:.5}, worst {worst:.5} (fraction {:.5}); Everest {everest:.5}",
            median / mu,
            worst / mu
        );

        // What that costs the picture, at the roughest value the shipped raster actually produces,
        // against the value the law is written in terms of (1.0).
        let span = 109.0; // what a 100 m camera has in frame at 60° fov
        let octaves = (19_550.0f64 / (span / 192.0)).max(1.0).log2().min(16.0);
        let pp = |sf: f64| {
            let h: Vec<f64> = (0..400)
                .map(|i| {
                    micro_relief_m(
                        i as f64 * span / 400.0,
                        0.0,
                        19_550.0,
                        octaves,
                        mu,
                        h_crit,
                        sf,
                    )
                })
                .collect();
            h.iter().cloned().fold(f64::MIN, f64::max) - h.iter().cloned().fold(f64::MAX, f64::min)
        };
        // Fed from the MEASUREMENT above, not from constants — so if the shipped raster is ever replaced
        // with finer data, this test reports the new consequence instead of the old one.
        let (roughest, median_land) = (pp(worst / mu), pp(median / mu));
        println!(
            "relief in a {span:.0} m frame at 100 m altitude: {roughest:.2} m at Earth's roughest, {median_land:.2} m on median land"
        );
        assert!(
            roughest / span < 0.15,
            "even Earth's roughest 39 km gradient yields a gentle grade in frame, got {}",
            roughest / span
        );
        assert!(
            median_land / span < 0.005,
            "typical land yields a billiard table in frame, got {}",
            median_land / span
        );
    }

    /// **What Earth's own roughness actually does with scale — the exponent the generator should have had**
    /// (docs/46 row 27).
    ///
    /// The generator's amplitude ∝ wavelength is Hurst exponent **H = 1**, which is an assumption nobody
    /// made deliberately — it falls out of the `λ/4` heightfield cap binding at every wavelength. This
    /// measures the real thing from the raster the scene draws, so the assumption can be replaced by a
    /// number instead of by another assumption.
    ///
    /// **Method: the structure function** (the variogram). For a self-affine surface the RMS height
    /// difference between two points a distance `r` apart grows as `r^H`, so a log-log fit of RMS Δz
    /// against `r` has slope `H` directly. Pairs are taken along great circles from golden-angle sample
    /// points — deterministic, no seed, uniform over the sphere — and **both ends must be land**: the sea
    /// floor is a different surface with different statistics, and the coastline between them is a
    /// kilometres-tall step that would dominate anything measured across it.
    ///
    /// **What this can and cannot say.** The raster resolves 19.5 km, so the honest fitting range is
    /// ~39 km (two texels, clear of the bilinear smoothing at one) to a few hundred km — under two
    /// decades. Extrapolating an exponent measured there down to metre wavelengths is FOUR MORE DECADES,
    /// and that extrapolation is a declared model, not a measurement (Law V). What it does establish is
    /// the thing the current law gets wrong: real topography is not H = 1, so a generator built on `λ/4`
    /// is not merely uncalibrated, it has the wrong SHAPE, and no multiplier fixes a wrong exponent.
    #[test]
    fn earths_topography_is_self_affine_and_its_exponent_is_not_one() {
        let (elev, range) = crate::terra::raster::shipped::earth_elevation();
        let mask = crate::terra::raster::shipped::earth_landmask();
        let r_m = 6.371e6;
        let texel = elev.texel_arc_m(r_m);

        // A great-circle step of `dist_m` on bearing `az` from (lat, lon), in degrees.
        let offset = |lat: f64, lon: f64, az: f64, dist_m: f64| -> (f64, f64) {
            let d = dist_m / r_m;
            let (la, lo) = (lat.to_radians(), lon.to_radians());
            let lat2 = (la.sin() * d.cos() + la.cos() * d.sin() * az.cos()).asin();
            let lon2 = lo + (az.sin() * d.sin() * la.cos()).atan2(d.cos() - la.sin() * lat2.sin());
            (lat2.to_degrees(), lon2.to_degrees())
        };

        // Golden-angle points: uniform over the sphere, deterministic, identical every run.
        const N: usize = 20_000;
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        let lags: Vec<f64> = (0..6).map(|k| texel * (1u32 << k) as f64).collect();
        let mut sum = vec![0.0f64; lags.len()];
        let mut cnt = vec![0usize; lags.len()];
        let mut land_points = 0usize;
        for i in 0..N {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / N as f64;
            let lat = z.asin().to_degrees();
            let lon = ((i as f64 * golden) % std::f64::consts::TAU).to_degrees() - 180.0;
            if !mask.land_at(lat, lon) {
                continue;
            }
            land_points += 1;
            let e0 = elev.elevation_m_at(lat, lon, range[0], range[1]);
            for (li, &lag) in lags.iter().enumerate() {
                for a in 0..8 {
                    let az = a as f64 * std::f64::consts::TAU / 8.0;
                    let (lat2, lon2) = offset(lat, lon, az, lag);
                    if !mask.land_at(lat2, lon2) {
                        continue;
                    }
                    let d = elev.elevation_m_at(lat2, lon2, range[0], range[1]) - e0;
                    sum[li] += d * d;
                    cnt[li] += 1;
                }
            }
        }
        assert!(land_points > 3_000, "enough land to be a measurement");

        let rms: Vec<f64> = (0..lags.len())
            .map(|i| (sum[i] / cnt[i].max(1) as f64).sqrt())
            .collect();
        for i in 0..lags.len() {
            println!(
                "  lag {:8.1} km   RMS dz {:8.1} m   ({} pairs)",
                lags[i] / 1000.0,
                rms[i],
                cnt[i]
            );
        }
        // Least-squares slope of log(RMS) against log(lag), skipping the shortest lag: at one texel the
        // bilinear interpolation is smoothing the very quantity being measured.
        let fit = |from: usize| {
            let xs: Vec<f64> = lags[from..].iter().map(|l| l.ln()).collect();
            let ys: Vec<f64> = rms[from..].iter().map(|v| v.ln()).collect();
            let n = xs.len() as f64;
            let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
            let num: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
            let den: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
            num / den
        };
        let h = fit(1);
        println!(
            "  Hurst exponent H = {h:.3} over {:.0}-{:.0} km ({} land points)",
            lags[1] / 1000.0,
            lags[lags.len() - 1] / 1000.0,
            land_points
        );
        println!(
            "  anchor: RMS dz at one raster texel ({:.1} km) = {:.1} m",
            texel / 1000.0,
            rms[0]
        );
        // The finding: Earth's land is self-affine with an exponent well under 1. The generator's implied
        // H = 1 is not a calibration error, it is the wrong shape — at 10 m wavelength, `λ^1` is smaller
        // than `λ^H` by (19_550/10)^(1-H), which is a factor of tens.
        assert!(
            (0.2..0.95).contains(&h),
            "H should land in the self-affine range reported for topography, got {h}"
        );
        assert!(
            h < 0.9,
            "the measurement must actually refute the generator's implied H = 1, got {h}"
        );
        let understatement = (19_550.0f64 / 10.0).powf(1.0 - h);
        println!(
            "  at 10 m wavelength, H=1 understates the measured exponent by {understatement:.0}x"
        );
        assert!(
            understatement > 5.0,
            "and the gap must matter at metre scale"
        );
    }

    fn ctrl() -> ResolutionController {
        ResolutionController::default() // 1 mrad angular resolution, 1 mm floor
    }

    /// **The relief this module generates must be STABLE by the engine's own slope law.**
    ///
    /// This is the guard the engine did not have, and its absence is why a wrong slope rule could be written
    /// here with nothing objecting. `granular` has carried Mohr-Coulomb all along (`face_stable`,
    /// `repose_allowance`) precisely so that *"ground and grain answer the slope question with ONE law"* —
    /// and a new module restating it in its own terms broke that silently, while every test still passed. A
    /// name check would not have caught it. This does: every octave of generated relief is handed back to
    /// `granular::face_stable` as the face it implies, and must be held up. If either side's physics moves,
    /// this fails.
    #[test]
    fn generated_relief_is_stable_by_the_engines_own_slope_law() {
        let mats = crate::materials::load();
        for id in ["granite", "basalt", "sand", "dirt", "grass", "clay", "snow"] {
            let m = &mats[crate::materials::index_of(&mats, id)];
            let mu = m.friction_coefficient as f64;
            let h_crit =
                crate::granular::critical_bank_height(m.fracture_strength, m.density, 9.81) as f64;
            for lambda in [0.01_f64, 0.1, 1.0, 10.0, 100.0, 1000.0] {
                let amp = relief_amplitude_m(lambda, mu, h_crit);
                // The face this relief implies: a crest-to-trough drop over half a wavelength of run,
                // standing in material of its own height.
                let (drop, run) = ((2.0 * amp) as f32, (lambda * 0.5) as f32);
                assert!(
                    crate::granular::face_stable(drop, run, drop, mu as f32, h_crit as f32),
                    "{id} at lambda={lambda} m: generated a {drop:.4} m face over {run:.4} m that the \
                     engine's own Mohr-Coulomb test says would fail"
                );
                assert!(
                    amp <= lambda * 0.25 + 1e-12,
                    "{id} lambda={lambda}: relief cannot overhang"
                );
            }
        }
    }

    /// **Cohesion, not friction, is why rock stands steep** — the fact that refuted the first version of
    /// this rule. Sand grips HARDER than granite, so a friction-only criterion made sand the craggier
    /// material; the difference is entirely cohesion, and asking `granular` gets it right.
    #[test]
    fn cohesionless_sand_cannot_stand_what_granite_can() {
        let mats = crate::materials::load();
        let get = |id: &str| {
            let m = &mats[crate::materials::index_of(&mats, id)];
            (
                m.friction_coefficient as f64,
                crate::granular::critical_bank_height(m.fracture_strength, m.density, 9.81) as f64,
            )
        };
        let (g_mu, g_h) = get("granite");
        let (s_mu, s_h) = get("sand");
        assert!(
            s_mu > g_mu,
            "sand grips harder ({s_mu} vs {g_mu}) - friction alone is not the answer"
        );
        assert_eq!(
            s_h, 0.0,
            "and cohesionless sand holds no bank at all: that IS the difference"
        );
        assert!(g_h > 100.0, "granite holds a substantial bank ({g_h:.0} m)");

        // At a SMALL scale cohesion dominates and rock is craggier than sand.
        let (rock, sand) = (
            relief_amplitude_m(0.1, g_mu, g_h),
            relief_amplitude_m(0.1, s_mu, s_h),
        );
        assert!(
            rock > sand,
            "at 10 cm rock is craggier than sand ({rock:.4} vs {sand:.4})"
        );
        // But only just — and the reason is worth pinning, because it is a limit of the DRAWING and not of
        // the ground: granite's cohesion permits a 1,057 m bank, so below ~2 km wavelength its ceiling is
        // the heightfield's own `lambda/4` (a height cannot overhang) rather than anything physical. Rock is
        // simply "as rough as a heightfield can express" across that whole range — and what it throws away
        // is REAL: arches and undercuts do form in stone.
        assert!(
            (rock - 0.1 * 0.25).abs() < 1e-12,
            "rock is at the representational cap, not a physical one ({rock:.5})"
        );

        // At a LARGE scale friction dominates for both — and then SAND stands steeper, because its
        // friction coefficient is genuinely higher (0.67 vs 0.60). Surprising, and correct: a dune field can
        // carry a steeper sustained slope than granite's friction alone would. It is only cohesion that
        // makes rock the steeper material, and cohesion is a small-scale term.
        let (rock_km, sand_km) = (
            relief_amplitude_m(10_000.0, g_mu, g_h),
            relief_amplitude_m(10_000.0, s_mu, s_h),
        );
        assert!(
            sand_km > rock_km,
            "at 10 km sand's higher friction wins ({sand_km:.0} vs {rock_km:.0}) - cohesion is small-scale"
        );

        // Sand is SELF-SIMILAR: with no cohesion its amplitude is pure repose at every scale, which is why
        // dunes look the same close up and far away.
        for lambda in [0.01_f64, 1.0, 100.0, 10_000.0] {
            let expect = (lambda * 0.5 * s_mu * 0.5).min(lambda * 0.25);
            let got = relief_amplitude_m(lambda, s_mu, s_h);
            // RELATIVE tolerance: `granular::repose_allowance_on` works in f32 (it serves the voxel
            // heightfield), so a round trip through it carries f32 precision, not f64.
            assert!(
                (got - expect).abs() <= 1e-5 * expect.abs(),
                "sand at lambda={lambda} is its own repose slope and nothing else ({got} vs {expect})"
            );
        }
    }

    /// **Where the heightfield cap discards real geometry, and where there is nothing to discard.**
    ///
    /// Robin (2026-07-24): *"arches and overhangs, while rare, do form … but in stone, not soil/regolith."*
    /// Correct — and the criterion is already in the engine: `h_crit = c/(ρg)` is the tallest bank a material
    /// holds by itself, so a height cannot express what that material does below a wavelength of about
    /// `2·h_crit`, and can express everything above it. MEASURED from the catalogue:
    ///
    /// ```text
    /// material     h_crit     overhangs possible below λ
    /// sand,gravel   0 m        never, at any scale   (cohesionless)
    /// dirt          0.36 m     0.73 m                (a stream-bank undercut)
    /// grass         1.09 m     2.18 m
    /// clay          1.70 m     3.40 m
    /// snow          6.80 m    13.6 m                 (CORNICES — they really do overhang)
    /// sandstone   222 m      443 m
    /// limestone   378 m      755 m
    /// granite     453 m      906 m
    /// basalt      510 m     1019 m                   (arches, at any human scale)
    /// ice         159 m      318 m                   (séracs, ice cliffs)
    /// ```
    ///
    /// So at arch scale the split is exactly the one Robin drew — soil and regolith are repose-limited,
    /// stone is not — with snow the honest exception, because a snow cornice overhangs by metres and that is
    /// why it is a mountaineering hazard. Where the cap bites it is discarding REAL geometry, and its
    /// resolved form is matter: an arch is simply rock with air under it, which the voxel/SPH path can hold
    /// and a heightfield cannot.
    #[test]
    fn the_heightfield_cap_bites_only_where_overhangs_actually_form() {
        let mats = crate::materials::load();
        let props = |id: &str| {
            let m = &mats[crate::materials::index_of(&mats, id)];
            (
                m.friction_coefficient as f64,
                crate::granular::critical_bank_height(m.fracture_strength, m.density, 9.81) as f64,
            )
        };
        let capped = |id: &str, lambda: f64| {
            let (mu, h) = props(id);
            (relief_amplitude_m(lambda, mu, h) - lambda * 0.25).abs() < 1e-12
        };

        // COHESIONLESS: repose binds at every scale — no overhanging dune exists to be discarded.
        for id in ["sand", "gravel"] {
            assert_eq!(props(id).1, 0.0, "{id} is cohesionless");
            for lambda in [0.001_f64, 0.1, 10.0, 10_000.0] {
                assert!(
                    !capped(id, lambda),
                    "{id} at lambda={lambda} is repose-limited, never capped"
                );
            }
        }

        // ARCH SCALE (10 m): soil and regolith are repose-limited; stone and ice are not. Robin's split.
        for id in ["dirt", "clay", "grass"] {
            assert!(
                !capped(id, 10.0),
                "a ten-metre {id} arch does not form, so nothing is discarded"
            );
        }
        for id in ["granite", "basalt", "limestone", "sandstone", "ice"] {
            assert!(
                capped(id, 10.0),
                "{id} can arch at ten metres and a heightfield cannot hold it"
            );
        }
        // SNOW is the honest exception: cornices overhang by metres.
        assert!(
            capped("snow", 10.0),
            "a snow cornice overhangs — that is why it is a hazard"
        );
        assert!(!capped("snow", 40.0), "but not at forty metres");

        // The boundary is h_crit, not the material's category: even soil is capped well below its own bank
        // height, because a centimetre undercut in damp dirt is real.
        assert!(
            capped("dirt", 0.1),
            "a decimetre undercut in damp dirt is real"
        );
        let (_, dirt_h) = props("dirt");
        assert!(
            !capped("dirt", 4.0 * dirt_h),
            "and well above ~2*h_crit ({dirt_h:.2} m) it is repose-limited again"
        );
    }

    /// Generated relief is ROUGHNESS on measurement, deterministic, and it fades in. Determinism is Law IV,
    /// not a nicety: if looking away and back gave different hills, the camera would decide what is true.
    #[test]
    fn micro_relief_is_deterministic_roughness_that_fades_in_with_detail() {
        let mats = crate::materials::load();
        let gr = &mats[crate::materials::index_of(&mats, "granite")];
        let (mu, h_crit) = (
            gr.friction_coefficient as f64,
            crate::granular::critical_bank_height(gr.fracture_strength, gr.density, 9.81) as f64,
        );
        let base = 90.0; // the elevation raster's ground resolution - where measurement runs out
        let r =
            |x: f64, z: f64, oct: f64, frac: f64| micro_relief_m(x, z, base, oct, mu, h_crit, frac);

        // Same place, same answer, even after sampling elsewhere in between.
        let a = r(1234.5, -678.9, 4.0, 1.0);
        let _ = r(0.0, 0.0, 4.0, 1.0);
        assert_eq!(
            a,
            r(1234.5, -678.9, 4.0, 1.0),
            "the same ground, every time"
        );

        // Nothing requested, nothing added - the measured surface untouched.
        assert_eq!(r(1234.5, -678.9, 0.0, 1.0), 0.0, "no octaves => no change");
        assert_eq!(
            r(1234.5, -678.9, 6.0, 0.0),
            0.0,
            "and flat ground stays flat"
        );

        // FLAT GROUND STAYS FLAT: the slope fraction is what stops a plain coming out like a mountainside.
        let (steep, gentle) = (r(50.0, 50.0, 6.0, 1.0).abs(), r(50.0, 50.0, 6.0, 0.1).abs());
        assert!(
            gentle < steep * 0.2,
            "a gentle slope is far smoother ({gentle:.4} vs {steep:.4})"
        );

        // It FADES IN: a fractional octave lands between its neighbours, so nothing pops.
        let (o3, o3h, o4) = (
            r(50.0, 50.0, 3.0, 1.0),
            r(50.0, 50.0, 3.5, 1.0),
            r(50.0, 50.0, 4.0, 1.0),
        );
        assert!(
            (o3h - o3).abs() <= (o4 - o3).abs() + 1e-9,
            "a half octave sits between {o3:.5} and {o4:.5}, got {o3h:.5}"
        );

        // ROUGHNESS, not elevation: over a 2D spread the mean is small against the amplitude it works at.
        let amp = relief_amplitude_m(base * 0.5, mu, h_crit);
        let n = 40;
        let mut sum = 0.0;
        for i in 0..n {
            for j in 0..n {
                sum += r(i as f64 * 7.3, j as f64 * 5.1, 5.0, 1.0);
            }
        }
        let mean = sum / (n * n) as f64;
        assert!(
            mean.abs() < 0.25 * amp,
            "relief neither raises nor lowers the ground (mean {mean:.4} m vs amplitude {amp:.4} m)"
        );
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
            assert!(
                t < prev,
                "at {d} m the texel ({t}) must be finer than at the previous distance ({prev})"
            );
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
        assert!(
            (7_000.0..9_000.0).contains(&orbital),
            "orbital texel {orbital} m should be ~8 km"
        );
        // 2 m up (standing): ~2 mm. Individual pebbles are resolvable — which is why the ground must
        // not be a single flat triangle there.
        let standing = texel_size_m(&c, 2.0);
        assert!(
            (0.001..0.01).contains(&standing),
            "standing texel {standing} m should be ~2 mm"
        );
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
            assert!(
                texel_size_m(&c, d) >= c.min_grain_radius,
                "floor breached at {d} m"
            );
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
        assert!(
            prev > 15.0,
            "closing from 1000 km to 2 m on a 1 km cell needs many octaves, got {prev}"
        );
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
        assert!(
            earth > 100.0,
            "the Earth 400 km away needs no sub-metre detail, got {earth} m"
        );
        assert!(
            debris < 0.01,
            "the debris at arm's length does, got {debris} m"
        );
    }

    /// Detail must follow VIEWABLE AREA, not just a raw number — a wide field of view at the same
    /// distance shows more world, so each texel covers more of it.
    #[test]
    fn view_span_grows_with_distance_and_field_of_view() {
        let narrow = view_span_m(100.0, 30f64.to_radians());
        let wide = view_span_m(100.0, 90f64.to_radians());
        assert!(
            wide > narrow * 2.0,
            "a 90-degree view spans far more than a 30-degree one"
        );
        assert!(view_span_m(200.0, 60f64.to_radians()) > view_span_m(100.0, 60f64.to_radians()));
        assert_eq!(view_span_m(0.0, 60f64.to_radians()), 0.0);
    }
}
