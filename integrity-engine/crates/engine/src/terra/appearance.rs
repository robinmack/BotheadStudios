//! **The appearance integral — what a footprint of surface looks like, as a statistic of the matter
//! that is in it** (docs/63).
//!
//! Robin: *"Mathematically it is matter, but we only materialize that matter **visually** when we need
//! to, and only the amount we need to."*
//!
//! That sentence is Law IV said exactly, and this module is its render-side half. The ground is matter
//! at every point, always; a camera never decides whether it is there, only how much of it we bother to
//! MATERIALIZE. So the texture drawn over a patch is not a stand-in FOR the matter — it is a
//! **statistical description of the matter that is there, integrated over the footprint being drawn**.
//! That is what makes it honest rather than a fudge, and it is the same argument
//! [`surface_normal.wgsl`](../../../shaders/surface_normal.wgsl) already makes for material grain,
//! generalised from one scale to all of them.
//!
//! ## Why this has to exist — the measured gap
//!
//! The segment mesh carries geometry down to its own cell. On a descent that cell is enormous compared
//! with the data underneath it: at 94 m altitude Terra's cell is ~469 m while a streamed elevation tile
//! pixel is ~3.7 m, so roughly **sixteen thousand measured elevation samples sit inside one mesh cell**
//! and not one of them reaches the screen. The clamp in `Terra::build_segment` says so outright —
//! `octaves ≤ log2(base_feature / cell)` is `log2(3.71/469).max(1) = 0`, so generated relief switches
//! off entirely exactly where the descent was supposed to start revealing ground.
//!
//! Adding vertices is not the cure; that was measured too, and it is what docs/63 retired. One tier
//! against four differed by **at most four pixel values at 100 m altitude**, because geometry was never
//! the right carrier for sub-cell detail. What the mesh cannot hold as SHAPE it must hold as
//! STATISTICS, and there are exactly two moments that matter:
//!
//! - **Mean albedo** — the area-weighted MIXTURE of the materials in the footprint. Today a vertex
//!   point-samples one land-cover texel and wears that one colour, which is why biome edges come back
//!   jagged and why a whole frame can be a single flat green.
//! - **Variance of the normal** — sub-footprint geometry does not stop existing when it stops being
//!   resolved, it becomes ROUGHNESS. Averaging normals alone throws that away and the surface goes
//!   glassy, which is most of why our ground reads as painted plastic from altitude.
//!
//! ## The one rule that keeps it from double-counting
//!
//! **The mesh already carries the MEAN slope — its own normal. The appearance carries only the
//! variance ABOUT that mean.** Integrate the total slope instead and every hill gets counted twice:
//! once as the shape the mesh is already displaying, and again as roughness smeared over it. So
//! [`Moments`] tracks the mean separately and reports variance about it, and
//! [`Appearance::mean_slope`] is exactly what the mesh normal should be showing.
//!
//! ## Why it is refinement-invariant, and why that is the test docs/63 asked for
//!
//! > *"Resolve a patch to matter, integrate its appearance over the footprint that was being drawn, and
//! > the result must equal the texture that was already being drawn there. If they differ, one of them
//! > is lying."*
//!
//! Combining sub-footprints is the **law of total variance**, and getting it wrong is the whole failure
//! mode this module exists to prevent:
//!
//! ```text
//! Var(total) = E[Var(within)]  +  Var(E[within])
//!              ^^^^^^^^^^^^^^     ^^^^^^^^^^^^^^
//!              roughness the      roughness that lives in how the sub-footprints
//!              children saw       DIFFER from each other — the term a naive
//!                                 average of children silently drops
//! ```
//!
//! Drop the second term and a coarse footprint reports less roughness than the sum of its parts, so the
//! answer CHANGES as you refine — which is precisely "one of them is lying". [`Appearance::combine`]
//! carries both terms, and `refining_the_footprint_does_not_change_the_answer` fails if either goes
//! missing.
//!
//! ## What the shader does with it (Law VIII, and the convergence clause)
//!
//! Lambert is roughness-blind: `max(dot(n, l), 0)` gives the same answer however the sub-pixel surface
//! is arranged, so variance computed here would have nowhere to go. What replaces it is not a new
//! empirical BRDF but **the same Lambert law integrated over the slope distribution we just measured** —
//! which is what Oren-Nayar is. The honest bound is stated where it is used
//! (`surface_normal.wgsl::rough_diffuse`): the closed form is Oren & Nayar's own qualitative
//! approximation to that integral, flagged as a declared model naming the real computation it defers.
//!
//! It converges the right way, which is the clause Law VIII actually requires: as the mesh resolves the
//! terrain the residual variance goes to zero, `sigma → 0`, and the term returns to **exactly** Lambert.
//! Nothing is added far away that a finer budget would not remove.

use crate::materials::Material;

/// **The appearance of a patch of surface, as the two moments a renderer can act on.**
///
/// `area_weight` is what the patch is worth when several are combined — the solid angle or area it
/// stands for. It is carried rather than assumed equal because a footprint's sub-patches are not the
/// same size on a sphere.
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    /// Area fractions of the materials present, `(material index, fraction)`, summing to 1.
    /// This is the MIXTURE — the thing a point sample cannot represent.
    pub mix: Vec<(usize, f32)>,
    /// Fraction-weighted mean albedo of that mixture, via [`crate::materials::aggregate_albedo`] —
    /// the engine's one scale-relative colour summary, not a second copy of it (Law II).
    pub albedo: [f32; 3],
    /// The material holding the largest share. The shader samples ONE texture layer, so it needs a
    /// single answer for which; the mixture above is what the colour actually comes from.
    pub material: usize,
    /// Mean surface gradient over the footprint, `(east, north)`, as rise over run. **This is what the
    /// mesh normal carries**; it is reported so the two can be checked against each other.
    pub mean_slope: [f64; 2],
    /// Variance of the gradient ABOUT that mean, summed over both axes — the roughness the mesh is
    /// not showing. Dimensionless (tangent-squared).
    pub slope_variance: f64,
    /// The area this patch stands for, in whatever unit the caller is consistent about.
    pub area_weight: f64,
}

impl Appearance {
    /// The RMS slope ANGLE (radians) — what a microfacet model's `sigma` means.
    ///
    /// The measurement is a variance of GRADIENT (a tangent); a facet distribution is over ANGLE. They
    /// agree to first order and diverge for steep ground, so the conversion is stated rather than
    /// assumed: `atan` of the RMS gradient. Monotone, exact at zero, and saturating at π/2 as the
    /// ground goes vertical — which is the correct limit, not a clamp.
    pub fn sigma_rad(&self) -> f64 {
        self.slope_variance.max(0.0).sqrt().atan()
    }

    /// **Combine sub-patches into the patch that contains them — the law of total variance.**
    ///
    /// The mean is the area-weighted mean of the means. The variance is the area-weighted mean of the
    /// children's variances PLUS the variance of the children's means: roughness that exists purely
    /// because the children differ from one another is still roughness, and it is exactly the term a
    /// naive average loses. See the module note; this is the identity the convergence invariant tests.
    ///
    /// An empty input, or one with no area, returns a null appearance rather than a NaN.
    pub fn combine(parts: &[Appearance], materials: &[Material]) -> Appearance {
        let total: f64 = parts.iter().map(|p| p.area_weight.max(0.0)).sum();
        if parts.is_empty() || total <= 0.0 {
            return Appearance::null();
        }
        // Area-weighted mean gradient.
        let mut mean = [0.0f64; 2];
        for p in parts {
            let w = p.area_weight.max(0.0) / total;
            mean[0] += p.mean_slope[0] * w;
            mean[1] += p.mean_slope[1] * w;
        }
        // E[Var(within)] + Var(E[within]), the second term measured against the mean just formed.
        let mut var = 0.0f64;
        let mut mix: Vec<(usize, f32)> = Vec::new();
        for p in parts {
            let w = p.area_weight.max(0.0) / total;
            let d0 = p.mean_slope[0] - mean[0];
            let d1 = p.mean_slope[1] - mean[1];
            var += w * (p.slope_variance.max(0.0) + d0 * d0 + d1 * d1);
            for &(mi, f) in &p.mix {
                let f = f * w as f32;
                match mix.iter_mut().find(|(m, _)| *m == mi) {
                    Some((_, acc)) => *acc += f,
                    None => mix.push((mi, f)),
                }
            }
        }
        Appearance::finish(mix, mean, var, total, materials)
    }

    /// A patch with nothing in it: black, flat, weightless. Never a NaN.
    pub fn null() -> Appearance {
        Appearance {
            mix: Vec::new(),
            albedo: [0.0; 3],
            material: 0,
            mean_slope: [0.0; 2],
            slope_variance: 0.0,
            area_weight: 0.0,
        }
    }

    /// Normalise a mixture and derive the colour and the dominant layer from it. One place, so the
    /// integrator and the combiner cannot disagree about what a mixture means.
    fn finish(
        mut mix: Vec<(usize, f32)>,
        mean_slope: [f64; 2],
        slope_variance: f64,
        area_weight: f64,
        materials: &[Material],
    ) -> Appearance {
        let sum: f32 = mix.iter().map(|&(_, f)| f.max(0.0)).sum();
        if sum > 0.0 {
            for e in mix.iter_mut() {
                e.1 = e.1.max(0.0) / sum;
            }
        }
        // Largest share first — the shader takes `mix[0]` as its texture layer, and a stable order
        // keeps a vertex from flickering between two materials that are nearly tied.
        mix.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let albedo = crate::materials::aggregate_albedo(&mix, materials);
        let material = mix.first().map_or(0, |&(m, _)| m);
        Appearance {
            mix,
            albedo,
            material,
            mean_slope,
            slope_variance,
            area_weight,
        }
    }
}

/// **A streaming accumulator for one footprint.** Kept separate from [`Appearance`] because it holds
/// the running sums, and because reusing one across thousands of vertices is what keeps a per-vertex
/// integral from allocating per vertex.
#[derive(Default)]
pub struct Moments {
    n: f64,
    sum: [f64; 2],
    sum_sq: [f64; 2],
    mix: Vec<(usize, f32)>,
}

impl Moments {
    pub fn new() -> Moments {
        Moments::default()
    }

    /// Forget everything but the allocation — the reason this type exists.
    pub fn clear(&mut self) {
        self.n = 0.0;
        self.sum = [0.0; 2];
        self.sum_sq = [0.0; 2];
        self.mix.clear();
    }

    /// Add one sub-sample: the gradient there, and the material occupying it.
    pub fn add(&mut self, grad: [f64; 2], material: usize) {
        self.n += 1.0;
        for k in 0..2 {
            self.sum[k] += grad[k];
            self.sum_sq[k] += grad[k] * grad[k];
        }
        match self.mix.iter_mut().find(|(m, _)| *m == material) {
            Some((_, f)) => *f += 1.0,
            None => self.mix.push((material, 1.0)),
        }
    }

    /// Close the footprint. `area_weight` is what this footprint is worth to a later [`Appearance::combine`].
    ///
    /// The variance is the population variance about the sample mean, floored at zero — with one
    /// sample it is exactly zero, which is the honest answer (a single probe has seen no variation),
    /// not an undefined one.
    pub fn finish(&self, area_weight: f64, materials: &[Material]) -> Appearance {
        if self.n <= 0.0 {
            return Appearance::null();
        }
        let mean = [self.sum[0] / self.n, self.sum[1] / self.n];
        let var = (0..2)
            .map(|k| (self.sum_sq[k] / self.n - mean[k] * mean[k]).max(0.0))
            .sum::<f64>();
        Appearance::finish(self.mix.clone(), mean, var, area_weight, materials)
    }
}

/// **Integrate the appearance of a square footprint of surface.**
///
/// `probe(u, v)` answers what the surface IS at an offset `(u, v)` metres from the footprint's centre,
/// in the footprint's own tangent frame: `(height in metres, material index)`. Everything physical
/// comes from the caller's probe — this function only takes moments of it.
///
/// `size_m` is the footprint's side; `step_m` the spacing at which it is sampled, which should be **the
/// resolution of the DATA underneath, never finer**. Sampling below the data's own resolution only
/// interpolates it, and interpolation is smooth, so it would report a surface flatter than the one
/// actually measured — the exact error this module exists to correct, reintroduced from the other side.
/// `max_side` caps the probe count per footprint; when it binds the grid is stretched to still span the
/// whole footprint, so the estimate stays unbiased (it sub-samples, it does not shrink the window).
///
/// Gradients come from forward differences on the probe grid, so an `n × n` gradient field costs
/// `(n+1)²` probes.
pub fn integrate_footprint(
    size_m: f64,
    step_m: f64,
    max_side: usize,
    materials: &[Material],
    scratch: &mut Moments,
    probe: impl Fn(f64, f64) -> (f64, usize),
) -> Appearance {
    scratch.clear();
    if !(size_m > 0.0) || !(step_m > 0.0) || max_side == 0 {
        return Appearance::null();
    }
    // How many gradient cells the data supports across this footprint, capped by the budget. At least
    // one: a footprint smaller than one data step still has a defined appearance (that of its single
    // sample), it simply has no measurable variance.
    let want = (size_m / step_m).floor() as usize;
    let n = want.clamp(1, max_side);
    // The realised spacing. When the budget binds this is COARSER than the data, which sub-samples the
    // surface rather than cropping it; when the data is coarser than the footprint it is `step_m`.
    let d = size_m / n as f64;
    let half = size_m * 0.5;
    // Probe grid: (n+1)² heights, so every cell has a forward neighbour on both axes.
    let mut heights = vec![0.0f64; (n + 1) * (n + 1)];
    let mut mats = vec![0usize; (n + 1) * (n + 1)];
    for j in 0..=n {
        for i in 0..=n {
            let u = -half + i as f64 * d;
            let v = -half + j as f64 * d;
            let (h, m) = probe(u, v);
            heights[j * (n + 1) + i] = h;
            mats[j * (n + 1) + i] = m;
        }
    }
    for j in 0..n {
        for i in 0..n {
            let h00 = heights[j * (n + 1) + i];
            let h10 = heights[j * (n + 1) + i + 1];
            let h01 = heights[(j + 1) * (n + 1) + i];
            scratch.add([(h10 - h00) / d, (h01 - h00) / d], mats[j * (n + 1) + i]);
        }
    }
    scratch.finish(size_m * size_m, materials)
}

/// **The appearance of the patch of SPHERE a vertex stands for.**
///
/// [`integrate_footprint`] works in a flat tangent frame; this is the binding that puts that frame on a
/// body. `probe(lat, lon)` answers with `(height in metres, material index)` — the caller's own ground
/// function, so the integral and the vertex displacement cannot disagree about where the ground is.
///
/// The offset from the centre is a FIRST-ORDER tangent step, `dir + east·(u/R) + north·(v/R)`
/// renormalised. Its error is second order in `u/R`: the largest cell the segment ever builds is about
/// a hundred kilometres against a 6,371 km radius, so `u/R ≲ 0.016` and the position error is under two
/// parts in ten thousand of the cell — far below the data's own resolution, which is what the integral
/// is sampling. Exact spherical offsets would cost two trig calls per probe to move the sample less
/// than the data can distinguish.
#[allow(clippy::too_many_arguments)]
pub fn integrate_on_sphere(
    scratch: &mut Moments,
    dir: glam::DVec3,
    radius_m: f64,
    cell_m: f64,
    step_m: f64,
    max_side: usize,
    materials: &[Material],
    probe: impl Fn(f64, f64) -> (f64, usize),
) -> Appearance {
    if !(radius_m > 0.0) {
        return Appearance::null();
    }
    let up = dir.normalize_or(glam::DVec3::Y);
    // A tangent frame. Any frame will do — the moments are rotation-invariant in the tangent plane —
    // so this only has to be well defined, including over the poles where `Y` is degenerate.
    let east = up
        .cross(glam::DVec3::Y)
        .try_normalize()
        .unwrap_or_else(|| up.cross(glam::DVec3::X).normalize());
    let north = east.cross(up).normalize();
    integrate_footprint(cell_m, step_m, max_side, materials, scratch, |u, v| {
        let d = (up + east * (u / radius_m) + north * (v / radius_m)).normalize();
        let (lat, lon) = crate::geo::lat_lon_from_dir(d);
        probe(lat, lon)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// **THE NEGATIVE RESULT, pinned: at mesh-cell scale the shipped rasters have nothing to
    /// integrate.** (docs/46 row 28.)
    ///
    /// The machinery in this module is correct and tested, and at 100 m altitude over the Colorado
    /// Rockies it changes the picture by almost nothing. This test says why, so nobody re-derives it
    /// from a screenshot:
    ///
    /// - The land-cover raster is **19.5 km per texel**. Terra's mesh cell at 100 m altitude is ~469 m.
    ///   A cell is therefore INSIDE one texel, the mixture has exactly one constituent, and a
    ///   fraction-weighted mean of one material is that material. There is no mixture to find.
    /// - The elevation raster is the same resolution, so across 469 m it is a smooth bilinear ramp —
    ///   a plane — and a plane has no slope VARIANCE (that is `a_plane_is_not_rough_however_steep_it_is`
    ///   restated on real data). Only the streamed tiles carry metre-scale relief, and their bounded
    ///   3x3 patch covers a few kilometres of a segment tens of kilometres wide.
    ///
    /// So the appearance integral is not what is missing from the ground; **the DATA is**, exactly as
    /// Robin said on seeing it. This test FAILS when better land cover lands — which is the point: it
    /// is the reason the expansion is worth doing, stated as an assertion rather than a memory.
    #[test]
    fn the_shipped_rasters_cannot_express_a_mixture_at_mesh_cell_scale() {
        let mats = mats();
        let (lc, biomes) = crate::terra::raster::shipped::earth_landcover();
        let (elev, range) = crate::terra::raster::shipped::earth_elevation();
        // The land cover a 469 m cell can distinguish. One texel spans 19.5 km, so the answer is one.
        let texel_km = 40_075.0 / lc.w as f64;
        assert!(
            texel_km > 15.0,
            "this test describes a COARSE raster; it is now {texel_km:.1} km/texel"
        );
        let biome_mats: Vec<usize> = (0..=5)
            .map(|i| {
                crate::materials::index_of(
                    &mats,
                    biomes
                        .get(&i.to_string())
                        .map(String::as_str)
                        .unwrap_or("granite"),
                )
            })
            .collect();
        let (lat, lon) = (39.0, -106.0); // the scale ladder's own site
        let cell_m = 469.0;
        let r = 6.371e6;
        let mut scratch = Moments::new();
        let a = integrate_on_sphere(
            &mut scratch,
            crate::geo::dir_from_lat_lon(lat, lon),
            r,
            cell_m,
            30.0, // sample far FINER than the raster, to prove the raster is what is flat
            64,
            &mats,
            |la, lo| {
                let b = lc.biome_at(la, lo) as usize;
                (
                    elev.elevation_m_at(la, lo, range[0], range[1]),
                    biome_mats.get(b).copied().unwrap_or(0),
                )
            },
        );
        assert_eq!(
            a.mix.len(),
            1,
            "a 469 m cell sits inside one 19.5 km texel, so the mixture has ONE constituent: {:?}",
            a.mix
        );
        // And the relief it can see over that cell is a bilinear ramp — a plane, hence no variance.
        // (Not exactly zero: the cell straddles a texel boundary where the ramp's slope changes.)
        assert!(
            a.sigma_rad() < 1.0e-3,
            "the coarse raster is a plane at this scale; sigma {} rad is too large to be one",
            a.sigma_rad()
        );
        // The mean slope, by contrast, is real and is what the MESH already shows.
        let mean = (a.mean_slope[0].powi(2) + a.mean_slope[1].powi(2)).sqrt();
        assert!(
            mean > 1.0e-4,
            "the regional gradient IS there and the mesh carries it: {mean}"
        );
    }

    /// **THE CONVERGENCE INVARIANT** (docs/63): *"Resolve a patch to matter, integrate its appearance
    /// over the footprint that was being drawn, and the result must equal the texture that was already
    /// being drawn there. If they differ, one of them is lying."*
    ///
    /// Stated operationally: integrating one footprint must equal combining the integrals of the four
    /// quadrants it is made of. That is not automatic — it is true only if [`Appearance::combine`]
    /// carries BOTH terms of the law of total variance. The between-quadrant term is the one a naive
    /// average drops, and on a surface that tilts across the footprint it is most of the answer.
    ///
    /// This is the test that separates an appearance model from a look-right texture: a model that
    /// changes its answer when you refine it is reporting the sampling, not the ground.
    #[test]
    fn refining_the_footprint_does_not_change_the_answer() {
        let mats = mats();
        let granite = crate::materials::index_of(&mats, "granite");
        let sand = crate::materials::index_of(&mats, "sand");
        // A surface with structure at several scales AND a material boundary through it, so both
        // moments have something to disagree about. Heights are metres; u,v are metres.
        //
        // ★ The CURVATURE terms are load-bearing and were missing from the first version of this test.
        // Without them the surface was symmetric in `u`, every quadrant had the same mean gradient, and
        // `Var(E[within])` was ~0 — so this test PASSED with `combine`'s between-parts term deliberately
        // deleted. A convergence test whose fixture does not exercise the term it protects is not a
        // gate. Hence the curvature, and hence the precondition assertion below.
        let surface = |u: f64, v: f64| -> (f64, usize) {
            let h = 0.30 * u                       // a regional tilt: pure MEAN, no variance
                + 0.0015 * u * u                   // curvature: makes the quadrant MEANS differ
                + 0.0010 * v * v                   // ... on both axes
                + 12.0 * (u / 90.0).sin()          // a hill: resolved by a coarse grid
                + 0.8 * (u / 3.0).sin() * (v / 3.0).cos(); // roughness: only a fine grid sees it
            let m = if v + 4.0 * (u / 50.0).sin() > 0.0 {
                granite
            } else {
                sand
            };
            (h, m)
        };
        let size = 480.0; // one mesh cell at ~94 m altitude
        let step = 3.71; // one streamed tile pixel
        let mut scratch = Moments::new();

        // Coarse: one integral over the whole footprint.
        let whole = integrate_footprint(size, step, 128, &mats, &mut scratch, surface);

        // Refined: the same footprint as four quadrants, each integrated on its own and combined.
        let half = size * 0.5;
        let mut parts = Vec::new();
        for (cu, cv) in [
            (-half * 0.5, -half * 0.5),
            (half * 0.5, -half * 0.5),
            (-half * 0.5, half * 0.5),
            (half * 0.5, half * 0.5),
        ] {
            parts.push(integrate_footprint(
                half,
                step,
                64,
                &mats,
                &mut scratch,
                |u, v| surface(cu + u, cv + v),
            ));
        }
        let refined = Appearance::combine(&parts, &mats);

        // ★ **THE PRECONDITION — this test is worthless without it.** The invariant is only being
        // exercised if the quadrants genuinely DIFFER from one another, because the term that gets
        // dropped by a naive combine is exactly `Var(E[within])`. Measure it here rather than trusting
        // the fixture to be interesting: the first version of this test used a surface symmetric in `u`,
        // so this quantity was ~0 and the whole test passed with `combine` broken on purpose.
        let mean_of_means = {
            let mut m = [0.0f64; 2];
            for p in &parts {
                m[0] += p.mean_slope[0] / parts.len() as f64;
                m[1] += p.mean_slope[1] / parts.len() as f64;
            }
            m
        };
        let between: f64 = parts
            .iter()
            .map(|p| {
                let d0 = p.mean_slope[0] - mean_of_means[0];
                let d1 = p.mean_slope[1] - mean_of_means[1];
                (d0 * d0 + d1 * d1) / parts.len() as f64
            })
            .sum();
        assert!(
            between > 0.05 * whole.slope_variance,
            "fixture does not exercise the between-parts term ({between} against a total variance of \
             {}); a symmetric surface makes this test pass with `combine` deliberately broken",
            whole.slope_variance
        );

        // The mean gradient must agree — it is the thing the mesh normal shows.
        for k in 0..2 {
            assert!(
                (whole.mean_slope[k] - refined.mean_slope[k]).abs() < 2.0e-3,
                "mean slope axis {k} moved on refinement: {} vs {}",
                whole.mean_slope[k],
                refined.mean_slope[k]
            );
        }
        // And so must the variance. This is the assertion that fails if `combine` drops Var(E[within]).
        let rel =
            (whole.slope_variance - refined.slope_variance).abs() / whole.slope_variance.max(1e-12);
        assert!(
            rel < 0.05,
            "slope variance moved {:.1}% on refinement: {} vs {}",
            rel * 100.0,
            whole.slope_variance,
            refined.slope_variance
        );
        // The mixture is an area fraction, so it must survive subdivision essentially exactly.
        for &(mi, f) in &whole.mix {
            let g = refined
                .mix
                .iter()
                .find(|(m, _)| *m == mi)
                .map_or(0.0, |&(_, f)| f);
            assert!(
                (f - g).abs() < 0.02,
                "material {mi} share moved on refinement: {f} vs {g}"
            );
        }
        // And the colour that follows from it.
        for k in 0..3 {
            assert!(
                (whole.albedo[k] - refined.albedo[k]).abs() < 0.01,
                "albedo channel {k} moved on refinement"
            );
        }
    }

    /// **The failure this module exists to prevent, pinned from the other side.** If `combine` averaged
    /// the children's variances and forgot how much the children DIFFER, a footprint made of four
    /// perfectly smooth but differently-tilted quadrants would report zero roughness. It has plenty:
    /// the tilts themselves are the sub-footprint structure.
    ///
    /// Computed, not typed: quadrant mean gradients are ±0.1 on each axis about a zero mean, so
    /// `Var(E) = mean(d0² + d1²) = 0.01 + 0.01 = 0.02` exactly, and `E[Var] = 0`.
    #[test]
    fn roughness_that_lives_between_the_parts_is_not_lost() {
        let mats = mats();
        let flat_but_tilted = |gx: f64, gy: f64| Appearance {
            mix: vec![(0, 1.0)],
            albedo: mats[0].albedo,
            material: 0,
            mean_slope: [gx, gy],
            slope_variance: 0.0, // each quadrant is perfectly smooth on its own
            area_weight: 1.0,
        };
        let parts = [
            flat_but_tilted(0.1, 0.1),
            flat_but_tilted(-0.1, 0.1),
            flat_but_tilted(0.1, -0.1),
            flat_but_tilted(-0.1, -0.1),
        ];
        let c = Appearance::combine(&parts, &mats);
        assert!(
            (c.mean_slope[0]).abs() < 1e-12 && (c.mean_slope[1]).abs() < 1e-12,
            "the tilts cancel, so the mean is flat: {:?}",
            c.mean_slope
        );
        assert!(
            (c.slope_variance - 0.02).abs() < 1e-12,
            "variance between the parts must survive: got {}, want 0.02",
            c.slope_variance
        );
        // And it must reach the shader as a real angle, not a zero.
        assert!(
            (c.sigma_rad() - 0.02f64.sqrt().atan()).abs() < 1e-12,
            "sigma follows the variance"
        );
    }

    /// **A pure tilt is the mesh's job, not the appearance's.** A perfectly planar slope has a mean
    /// gradient and NO variance about it — if this reported roughness, every hillside would be shaded
    /// as though it were rubble, and the hill's own shape would be counted twice.
    #[test]
    fn a_plane_is_not_rough_however_steep_it_is() {
        let mats = mats();
        let mut scratch = Moments::new();
        for slope in [0.0, 0.05, 0.5, 2.0] {
            let a =
                integrate_footprint(400.0, 4.0, 64, &mats, &mut scratch, |u, _v| (slope * u, 3));
            assert!(
                a.slope_variance < 1e-18,
                "a plane of slope {slope} reported roughness {}",
                a.slope_variance
            );
            assert!(
                (a.mean_slope[0] - slope).abs() < 1e-9,
                "and its mean gradient is the slope itself"
            );
            assert!(
                a.sigma_rad() < 1e-9,
                "so sigma is zero and shading is Lambert"
            );
        }
    }

    /// **The mixture is what a point sample cannot see.** Half granite, half sand must come back as a
    /// 50/50 mixture whose colour is the mean of the two — not as whichever one the centre landed on.
    /// This is the jagged-biome-edge failure stated as an assertion.
    #[test]
    fn a_footprint_straddling_two_materials_reports_both() {
        let mats = mats();
        let granite = crate::materials::index_of(&mats, "granite");
        let sand = crate::materials::index_of(&mats, "sand");
        let mut scratch = Moments::new();
        let a = integrate_footprint(400.0, 4.0, 100, &mats, &mut scratch, |u, _v| {
            (0.0, if u < 0.0 { granite } else { sand })
        });
        let share = |m: usize| a.mix.iter().find(|(k, _)| *k == m).map_or(0.0, |&(_, f)| f);
        assert!(
            (share(granite) - 0.5).abs() < 0.02 && (share(sand) - 0.5).abs() < 0.02,
            "expected a 50/50 mixture, got {:?}",
            a.mix
        );
        for k in 0..3 {
            let want = 0.5 * (mats[granite].albedo[k] + mats[sand].albedo[k]);
            assert!(
                (a.albedo[k] - want).abs() < 0.02,
                "channel {k}: mixture colour {} should be the mean {want}",
                a.albedo[k]
            );
        }
        // A point sample at the centre would have answered with exactly one of them, and been wrong
        // about the other half of the ground.
        assert_eq!(a.mix.len(), 2, "both materials present: {:?}", a.mix);
    }

    /// A single material must pass through untouched — the integral may not tint anything on its way.
    /// (`aggregate_albedo` is pinned to this too; the point here is that the INTEGRATOR preserves it.)
    #[test]
    fn one_material_is_its_own_colour() {
        let mats = mats();
        let g = crate::materials::index_of(&mats, "granite");
        let mut scratch = Moments::new();
        let a = integrate_footprint(100.0, 5.0, 64, &mats, &mut scratch, |_u, _v| (0.0, g));
        assert_eq!(a.material, g);
        assert_eq!(a.albedo, mats[g].albedo);
        assert_eq!(a.mix.len(), 1);
    }

    /// **The sampling budget sub-samples the footprint; it must never shrink it.** A capped grid still
    /// spans the whole cell, so the mixture stays right and the variance stays close — the estimate
    /// gets noisier, not biased. A version that kept `step_m` and covered only part of the footprint
    /// would report the appearance of a patch nobody is drawing.
    #[test]
    fn a_capped_budget_still_spans_the_whole_footprint() {
        let mats = mats();
        let a_idx = crate::materials::index_of(&mats, "granite");
        let b_idx = crate::materials::index_of(&mats, "sand");
        let surface = |u: f64, v: f64| -> (f64, usize) {
            (
                4.0 * (u / 25.0).sin() + 4.0 * (v / 25.0).cos(),
                if u < 0.0 { a_idx } else { b_idx },
            )
        };
        let mut scratch = Moments::new();
        let full = integrate_footprint(500.0, 2.0, 250, &mats, &mut scratch, surface);
        let capped = integrate_footprint(500.0, 2.0, 25, &mats, &mut scratch, surface);
        // The material split is a property of the window, not of how finely it is sampled.
        let share =
            |a: &Appearance, m: usize| a.mix.iter().find(|(k, _)| *k == m).map_or(0.0, |&(_, f)| f);
        assert!(
            (share(&full, a_idx) - share(&capped, a_idx)).abs() < 0.03,
            "the capped grid must still cover the whole footprint: {:?} vs {:?}",
            full.mix,
            capped.mix
        );
        // Both must see the ground as rough; a budget is not allowed to flatten it.
        assert!(
            capped.slope_variance > 0.2 * full.slope_variance,
            "capped {} vs full {}",
            capped.slope_variance,
            full.slope_variance
        );
    }

    /// Nonsense in, null out — never a NaN reaching a vertex buffer.
    #[test]
    fn a_degenerate_footprint_is_null_not_nan() {
        let mats = mats();
        let mut scratch = Moments::new();
        for (size, step, cap) in [(0.0, 1.0, 8), (10.0, 0.0, 8), (10.0, 1.0, 0)] {
            let a = integrate_footprint(size, step, cap, &mats, &mut scratch, |_, _| (0.0, 0));
            assert_eq!(a, Appearance::null(), "size {size} step {step} cap {cap}");
        }
        let empty = Appearance::combine(&[], &mats);
        assert_eq!(empty, Appearance::null());
        assert!(empty.sigma_rad().is_finite());
        // Zero-area parts must not divide by zero either.
        let z = Appearance::combine(&[Appearance::null(), Appearance::null()], &mats);
        assert_eq!(z, Appearance::null());
    }
}
