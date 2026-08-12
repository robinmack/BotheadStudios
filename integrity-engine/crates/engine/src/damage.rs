//! Impact damage across scales — the **LOD bridge** (`docs/19`).
//!
//! The same impact energy a celestial collision reports (`orbit.rs`) determines the ground-scale
//! consequence. Crucially, the crater *volume* here uses the **same `σ·V` accounting** as the voxel
//! impact operator (`matter::impact`): the energy fractures a volume `V ≈ E/σ` of target material. So
//! a coarse-scale **summary** (this module) and a zoomed-in **voxel crater** (matter.rs) describe the
//! *same event* and agree — that is what makes damage consistent across level of detail.
//!
//! Honesty (`docs/19`): this is the **strength regime**, valid while the crater is small relative to
//! the body. Big impacts enter the **gravity regime** (you must lift ejecta out of the gravity well)
//! and, past the body's **binding energy**, **disruption** (the body comes apart — the giant-impact
//! regime that shattered-and-reformed the real Moon). We model the strength crater and the disruption
//! threshold; the gravity regime between them is flagged, not faked.

#![allow(dead_code)] // used by the wasm HUD and native tests; the native lib sees only tests

use crate::materials::Material;

/// Reference (pre-impact) temperature the melt/vaporization budgets start from (K) — surface-ish.
const REF_TEMP_K: f64 = 300.0;

/// What a parcel of material becomes at a given deposited **energy density** (J/m³, = Pa). The
/// thresholds are its own material data (`docs/20`): fracture strength, then the energy to melt, then
/// to vaporize. This is the SAME "energy density vs material threshold" idea as fracture — melt and
/// vaporization are just higher thresholds — so fragmentation, melting, and vaporization are one
/// data-driven response, and a single impact produces all three at different radii (near-field
/// vaporizes, mid melts, far fractures): a test of scale-of-detail as much as of thermodynamics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseChange {
    Intact,
    Fractured,
    /// Broken down irreversibly — charred, calcined, pyrolysed. The fate of a material that has no
    /// melting point: wood, rubber, limestone, concrete. Unlike melting, it does not reverse on cooling.
    Decomposed,
    Melted,
    Vaporized,
}

/// Energy density (J/m³) to melt the material from `REF_TEMP_K`: `ρ·(c·ΔT_to_melt + L_fusion)`.
/// `None` if we have no thermal data (then we only claim fracture, not melt — honesty).
pub fn melt_energy_density(m: &Material) -> Option<f64> {
    // Ask through the accessors: a material that DOES NOT MELT has no melting point, and reading the raw
    // field gave 0 K — an energy of c·(0 − 293) + 0, i.e. NEGATIVE, which classified oak as molten at any
    // energy whatsoever. Wood chars; it does not melt.
    let (c, melt, fusion) = (
        m.specific_heat()?,
        m.melt_point()?,
        m.thermal.as_ref()?.latent_fusion as f64,
    );
    Some((c * (melt - REF_TEMP_K) + fusion) * m.density as f64)
}

/// Energy density (J/m³) to break a material down irreversibly, for those that decompose instead of
/// melting: heat it from the reference temperature to its decomposition point. `None` if it melts (or if
/// the breakdown temperature is unsourced).
///
/// FLAGGED: this counts only the sensible heat to reach breakdown, not the enthalpy of the reaction
/// itself (calcination is strongly endothermic, pyrolysis mildly so), so it is a floor, not the full cost.
pub fn decomposition_energy_density(m: &Material) -> Option<f64> {
    let (c, t_d) = (m.specific_heat()?, m.decomposition_point()?);
    Some(c * (t_d - REF_TEMP_K) * m.density as f64)
}

/// Energy density (J/m³) to fully vaporize the material: heat to melt + heat to boil + latent heats.
/// A first model — it uses the solid specific heat throughout and ignores pressure (`docs/20`).
pub fn vapor_energy_density(m: &Material) -> Option<f64> {
    // Requires BOTH a melting and a boiling point: something that decomposes on the way up never gets to
    // be a vapour of itself.
    let (c, melt, boil) = (m.specific_heat()?, m.melt_point()?, m.boil_point()?);
    let t = m.thermal.as_ref()?;
    let per_kg = c * (melt - REF_TEMP_K)
        + t.latent_fusion as f64
        + c * (boil - melt)
        + t.latent_vaporization as f64;
    Some(per_kg * m.density as f64)
}

/// Classify a parcel's fate from the deposited energy density (J/m³) and its material.
pub fn classify(energy_density: f64, m: &Material, pressure_pa: f64) -> PhaseChange {
    if energy_density < m.fracture_strength as f64 {
        return PhaseChange::Intact;
    }
    // **Pressure picks the fate.** Melting and irreversible breakdown are a race, not a label: calcite
    // calcines at 1,098 K on a kiln floor only because the CO₂ can leave. Under confinement the reaction
    // is pushed back, the breakdown temperature climbs past the melting curve, and the same rock MELTS —
    // which is the regime inside any impact worth simulating. Concrete does this too.
    if m.decomposes_at(pressure_pa) {
        if let Some(ed) = decomposition_energy_density(m) {
            if energy_density >= ed {
                return PhaseChange::Decomposed;
            }
        }
        return PhaseChange::Fractured; // it will char before it can ever melt, so do not consider melting
    }
    if let Some(ev) = vapor_energy_density(m) {
        if energy_density >= ev {
            return PhaseChange::Vaporized;
        }
    }
    if let Some(em) = melt_energy_density(m) {
        if energy_density >= em {
            return PhaseChange::Melted;
        }
    }
    PhaseChange::Fractured
}

/// Standard sea-level pressure (Pa) — the ambient a bench-top experiment happens at, and the default for
/// callers that genuinely have no confining pressure to offer.
pub const ONE_ATM_PA: f64 = 101_325.0;

/// Excavated crater volume (m³) for `energy` (J) into a material of yield `strength` (Pa), strength
/// regime: `E ≈ σ·V`. A fluid (`strength ≈ 0`) holds no crater — it flows back — so this returns 0.
/// This is the SAME σ·V as `matter::impact`, so summary and voxel materialisation match.
pub fn crater_volume(energy: f64, strength: f64) -> f64 {
    if strength <= 0.0 {
        return 0.0;
    }
    energy / strength
}

/// **What it takes to crush an ARRANGEMENT, from how densely it is packed** (docs/70).
///
/// Robin, on being shown a haystack built as one cylinder with a packing fraction (2026-08-10):
/// *"Haystack should be created as an assembly of grass blades (dry)… is it?"* It is not — a 136 kg
/// bale is **95,239 straws**, 1.14 million triangles, so the blades are a DECLARED summary today
/// (`Part::packing`) rather than resolved geometry. This is the part of that summary which must not
/// also be declared.
///
/// A catalogued `compressive_strength` for straw is measured on a BALE, so it describes an
/// arrangement and not the substance. Used directly it says a loose haystack and a high-density bale
/// resist identically, which is false and is exactly the "sand versus sandstone" conflation Robin
/// named: the same grains at different packing behave differently.
///
/// So the stress SCALES with relative density, by the cellular-solids law for an open-cell structure
/// (Gibson & Ashby): plastic collapse goes as `(ρ*/ρs)^1.5`. Anchored on the measurement rather than
/// derived from it, because the measurement is the trustworthy end:
///
/// ```text
/// σ(p) = σ_ref · (p / p_ref)^1.5
/// ```
///
/// ★ FLAGGED, with the check that shows the size of the gap: Gibson & Ashby's own prefactor form,
/// `σ ≈ 0.3·σ_yield·(ρ*/ρs)^1.5`, gives **143 kPa** for a standard bale against a measured 30–75 kPa.
/// Same order, two to four times high — which is why the measurement anchors this and the theory only
/// supplies the exponent. Tuning the prefactor to match would be a dial; naming the discrepancy is not.
///
/// The reference pair is the bale the catalogue's number was measured on: `p_ref = 0.0714`
/// (100 kg/m³ bulk over straw's 1400 kg/m³ cell wall).
pub fn crush_stress_pa(reference_pa: f64, packing: f64, reference_packing: f64) -> f64 {
    if reference_pa <= 0.0 || packing <= 0.0 || reference_packing <= 0.0 {
        return 0.0;
    }
    // A solid has no arrangement left to collapse; it is simply the substance, which fracture answers.
    let p = packing.min(1.0);
    reference_pa * (p / reference_packing).powf(1.5)
}

/// The packing the catalogue's bale-scale `compressive_strength` figures were measured at — a standard
/// field bale, 100 kg/m³ of straw whose own cell wall is 1400. Named here so the one number that ties
/// the measurement to the scaling law is not a literal in the middle of an expression.
pub const BALE_REFERENCE_PACKING: f64 = 100.0 / 1400.0;

/// Radius (m) of a hemispherical crater of `volume` m³: `V = (2/3)π R³`.
pub fn crater_radius(volume: f64) -> f64 {
    (volume * 3.0 / (2.0 * std::f64::consts::PI)).cbrt()
}

/// The ground-scale verdict for an impact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GroundEffect {
    /// A crater of this radius (m) in the target's surface material (strength regime).
    Crater { radius_m: f64 },
    /// The impact energy meets or exceeds the body's gravitational binding energy: it is torn apart.
    Disruption,
}

/// Honest verdict: disruption if `energy` reaches the body's `binding` energy, else a strength-regime
/// crater in the surface material of yield `strength`. (A crater computed larger than the body means
/// we've left the strength regime — see the module note.)
pub fn ground_effect(energy: f64, surface_strength: f64, binding: f64) -> GroundEffect {
    if energy >= binding {
        GroundEffect::Disruption
    } else {
        GroundEffect::Crater {
            radius_m: crater_radius(crater_volume(energy, surface_strength)),
        }
    }
}

/// One piece of a body that came apart, described relative to the parent's centre of mass at the moment
/// of disruption. Mass and radius are matter; position and velocity are where the disruption put it.
#[derive(Clone, Copy, Debug)]
pub struct Fragment {
    pub mass_kg: f64,
    /// Radius (m) at the parent's own density — `r = (3m/4πρ)^⅓`, not a size anyone assigned.
    pub radius_m: f64,
    /// Position relative to the parent's centre at the time asked for (m).
    pub rel_pos: glam::DVec3,
    /// Velocity relative to the parent's centre of mass (m/s).
    pub rel_vel: glam::DVec3,
}

/// `n` directions spread as evenly over the sphere as `n` directions can be — the golden-angle (Fibonacci)
/// spiral: `z` stepped uniformly through [−1, 1] and longitude advanced by the golden angle each time.
///
/// A disruption throws fragments every way at once, and what that needs is ISOTROPY, not randomness. A
/// random draw of a dozen directions clumps visibly and differs run to run; this is uniform by
/// construction, needs no seed, and is identical every run — which is what lets a scene fly back to the
/// same crater it watched form (docs/59). The golden angle is the one that leaves no gaps: any other
/// rotation per step eventually repeats and stripes the sphere.
pub fn isotropic_directions(n: usize) -> Vec<glam::DVec3> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![glam::DVec3::Z];
    }
    // π(3 − √5): the golden angle, the irrational turn that never lines up with itself.
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..n)
        .map(|i| {
            // Midpoint sampling of z so the first and last points are not both stuck at the poles.
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let theta = golden * i as f64;
            glam::DVec3::new(r * theta.cos(), r * theta.sin(), z)
        })
        .collect()
}

/// **A body that came apart** — the inverse of accretion, and what a meteor SWARM is (docs/59).
///
/// [`ground_effect`] already decides WHETHER a body is disrupted (its impact energy reaching its own
/// gravitational binding energy). This says what disruption LEAVES, and every number in it is either the
/// parent's own matter or a sourced measurement:
///
/// - **How the mass divides.** A collisional fragment population follows a power law. Dohnanyi (1969,
///   *JGR* 74, 2531) gives the steady-state differential mass distribution `n(m) ∝ m^(−11/6)`, so the
///   cumulative is `N(>m) ∝ m^(−5/6)` and the i-th largest fragment carries `m_i ∝ i^(−6/5)`. The
///   exponent is that ratio, not a fitted decimal. Masses are then normalised to sum EXACTLY to the
///   parent's — conservation is what removes the remaining freedom, so no fragment size is chosen.
///   *Flagged:* Dohnanyi's slope describes a collisional cascade in steady state; a SINGLE disruption's
///   slope depends on how far past the threshold it was (`Q/Q*_D`). The resolved computation this stands
///   in for is simulating the fragmentation itself, which this engine can already do in principle — a
///   particalized body torn apart by a real impact. Until a scene needs that, this is the declared form.
///
/// - **How fast they separate.** At the disruption threshold — the condition [`ground_effect`] tests,
///   energy just reaching binding energy — the pieces are just barely unbound, so the scale of the
///   separation is the parent's own escape speed `√(2GM/R)`. That is the SAME escape-speed criterion
///   `accretion::Body::absorbs` uses to decide whether a straggler stays; one law, read in both
///   directions. *Flagged:* a super-catastrophic disruption imparts more than the threshold.
///
/// - **Momentum.** Disruption is an INTERNAL process: it cannot move the parent's centre of mass, so
///   `Σ m·v = 0` exactly, and the swarm as a whole still flies the trajectory the parent was on. That
///   constraint is not decoration — isotropic directions with UNEQUAL masses do not satisfy it on their
///   own (the heaviest fragment would drag the centre of mass off course), so the velocities are taken in
///   the centre-of-momentum frame: `v_i = v_esc·(d̂_i − d̄)` with `d̄` the mass-weighted mean direction.
///   The resulting spread in speed is therefore a CONSEQUENCE of conservation, not an assumed law — the
///   heavier pieces come out slower, which is also what disruption experiments find. *Flagged:* the
///   measured size–speed relation from laboratory catastrophic disruption is the refinement that would
///   replace conservation-alone as the source of that spread.
///
/// - **How far apart they are.** `v·t` over `since_s`, the time since the body came apart. The swarm's
///   spread is therefore a CONSEQUENCE of when it broke up, never a declared width.
///
/// `since_s` and the parent's matter are the initial conditions; everything else here follows from them.
pub fn disrupt(mass_kg: f64, radius_m: f64, n: usize, since_s: f64) -> Vec<Fragment> {
    if n == 0 || mass_kg <= 0.0 || radius_m <= 0.0 {
        return Vec::new();
    }
    const G: f64 = crate::orbit::G;
    // Dohnanyi: cumulative N(>m) ∝ m^(−5/6) ⇒ the i-th largest goes as i^(−6/5).
    const RANK_EXPONENT: f64 = -6.0 / 5.0;
    let weights: Vec<f64> = (1..=n).map(|i| (i as f64).powf(RANK_EXPONENT)).collect();
    let total_w: f64 = weights.iter().sum();
    // The parent's own density — what the pieces are made of, so their radii follow from their masses.
    let density = mass_kg / ((4.0 / 3.0) * std::f64::consts::PI * radius_m.powi(3));
    // Just-unbound: the escape speed of the body that came apart.
    let v_esc = (2.0 * G * mass_kg / radius_m).sqrt();
    let dirs = isotropic_directions(n);
    let masses: Vec<f64> = weights.iter().map(|w| mass_kg * w / total_w).collect();
    // The mass-weighted mean direction. Subtracting it puts the velocities in the parent's
    // centre-of-momentum frame, so `Σ m·v = 0` exactly and the disruption moves nothing but the pieces.
    let mean_dir: glam::DVec3 = masses
        .iter()
        .zip(&dirs)
        .map(|(m, d)| *d * (*m / mass_kg))
        .sum();
    masses
        .iter()
        .zip(&dirs)
        .map(|(&m, &dir)| {
            let rel_vel = (dir - mean_dir) * v_esc;
            Fragment {
                mass_kg: m,
                radius_m: (3.0 * m / (4.0 * std::f64::consts::PI * density)).cbrt(),
                // From where it sat on the parent's surface, drifting since.
                rel_pos: dir * radius_m + rel_vel * since_s.max(0.0),
                rel_vel,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A disrupted body is still the same body.** Whatever the power law does to the shares, the
    /// pieces add back up to what came apart — that conservation is what leaves no room to choose a
    /// fragment size.
    #[test]
    fn disruption_conserves_the_parents_mass_and_leaves_it_moving_nowhere() {
        // A 300 m stony asteroid at basalt-ish density.
        let (r, rho) = (300.0_f64, 2900.0_f64);
        let m = rho * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);
        for n in [2usize, 7, 12, 60] {
            let frags = disrupt(m, r, n, 6.0 * 3600.0);
            assert_eq!(frags.len(), n);
            let sum: f64 = frags.iter().map(|f| f.mass_kg).sum();
            assert!(
                (sum / m - 1.0).abs() < 1e-12,
                "n={n}: mass conserved ({sum:.6e} vs {m:.6e})"
            );

            // Disruption is INTERNAL: it cannot move the centre of mass, so the momentum closes to zero
            // exactly — which is why the swarm as a whole still flies the parent's declared trajectory and
            // the approach velocity stays an honest IC. Isotropic directions alone do NOT give this when
            // the masses differ; the centre-of-momentum construction is what does.
            let p: glam::DVec3 = frags.iter().map(|f| f.rel_vel * f.mass_kg).sum();
            let scale = m * frags.iter().map(|f| f.rel_vel.length()).fold(0.0, f64::max);
            assert!(
                p.length() / scale < 1e-14,
                "n={n}: momentum closes ({:.3e} relative)",
                p.length() / scale
            );

            // Every piece is real matter at the parent's density.
            for f in &frags {
                let implied = f.mass_kg / ((4.0 / 3.0) * std::f64::consts::PI * f.radius_m.powi(3));
                assert!(
                    (implied / rho - 1.0).abs() < 1e-9,
                    "fragment keeps the parent's density"
                );
            }
        }
    }

    /// **The size spread is the sourced power law, and the swarm's spread is the clock.** Neither is a
    /// width someone dialled in: one comes from Dohnanyi's exponent, the other from how long ago the body
    /// came apart.
    #[test]
    fn fragments_follow_the_collisional_power_law_and_spread_with_time() {
        let (r, rho) = (300.0_f64, 2900.0_f64);
        let m = rho * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);
        let frags = disrupt(m, r, 12, 0.0);

        // Ranked heaviest-first, and the ratio between ranks is i^(-6/5) — checked against the law, not
        // against a recorded number.
        for i in 1..frags.len() {
            assert!(
                frags[i].mass_kg < frags[i - 1].mass_kg,
                "fragment {i} is smaller than {}",
                i - 1
            );
            let expect = ((i + 1) as f64 / i as f64).powf(-6.0 / 5.0);
            let got = frags[i].mass_kg / frags[i - 1].mass_kg;
            assert!(
                (got / expect - 1.0).abs() < 1e-9,
                "rank {i}: {got:.6} vs {expect:.6}"
            );
        }
        // One dominant remnant with a tail of smaller pieces — what the exponent implies, ~39% at n=12.
        let biggest = frags[0].mass_kg / m;
        assert!(
            (0.3..0.5).contains(&biggest),
            "largest remnant is {:.0}% of the parent",
            biggest * 100.0
        );

        // The separation SCALE is the parent's escape speed (0.38 m/s for this asteroid). Individual
        // speeds spread around it because momentum must close, and they spread the way conservation says:
        // the heaviest piece is the slowest.
        let v_esc = (2.0 * crate::orbit::G * m / r).sqrt();
        for f in &frags {
            let ratio = f.rel_vel.length() / v_esc;
            assert!(
                (0.3..2.0).contains(&ratio),
                "speeds are of order the escape speed (got {ratio:.2})"
            );
        }
        assert!(
            frags[0].rel_vel.length() < frags[frags.len() - 1].rel_vel.length(),
            "the heaviest fragment leaves slowest — momentum conservation, not an assumed size-speed law"
        );
        let extent = |t: f64| {
            let f = disrupt(m, r, 12, t);
            f.iter().map(|x| x.rel_pos.length()).fold(0.0, f64::max)
        };
        let (a_day, a_week) = (extent(86_400.0), extent(7.0 * 86_400.0));
        assert!(
            a_day > 30_000.0,
            "a day out the swarm is tens of km across, got {a_day:.0} m"
        );
        assert!(
            (a_week / a_day - 7.0).abs() < 0.1,
            "and it spreads linearly in time"
        );
    }

    /// The directions really do cover the sphere: no hemisphere is empty, and no axis is preferred.
    /// (Grid bias is a mistake this repo has made before — see `isotropy.rs`.)
    #[test]
    fn the_separation_directions_are_isotropic() {
        let dirs = isotropic_directions(200);
        for d in &dirs {
            assert!((d.length() - 1.0).abs() < 1e-12, "unit vectors");
        }
        let mean: glam::DVec3 = dirs.iter().sum::<glam::DVec3>() / dirs.len() as f64;
        assert!(mean.length() < 0.02, "no net direction ({mean:?})");
        // Second moments: an isotropic set has ⟨x²⟩=⟨y²⟩=⟨z²⟩=⅓, so no axis is special.
        for (axis, v) in [
            ("x", glam::DVec3::X),
            ("y", glam::DVec3::Y),
            ("z", glam::DVec3::Z),
        ] {
            let m2: f64 = dirs.iter().map(|d| d.dot(v).powi(2)).sum::<f64>() / dirs.len() as f64;
            assert!(
                (m2 - 1.0 / 3.0).abs() < 0.02,
                "{axis}: ⟨{axis}²⟩ = {m2:.4}, expected ⅓"
            );
        }
    }

    #[test]
    fn crater_scales_with_energy_and_inversely_with_strength() {
        // Volume is E/σ: 10× the energy → 10× the volume; 10× the strength → 1/10 the volume.
        let base = crater_volume(1.0e9, 1.0e6);
        assert!((base - 1.0e3).abs() < 1e-6, "V = E/σ");
        assert!((crater_volume(1.0e10, 1.0e6) - 10.0 * base).abs() / (10.0 * base) < 1e-9);
        assert!((crater_volume(1.0e9, 1.0e7) - base / 10.0).abs() / (base / 10.0) < 1e-9);

        // A fluid holds no crater.
        assert_eq!(crater_volume(1.0e9, 0.0), 0.0);

        // Radius is the hemisphere inverse: V = (2/3)π R³.
        let r = crater_radius(base);
        assert!((2.0 / 3.0 * std::f64::consts::PI * r * r * r - base).abs() / base < 1e-9);
    }

    #[test]
    fn moon_shatters_but_earth_only_craters() {
        // The honest regimes, with real numbers. G, masses, radii.
        let g = crate::orbit::G;
        let (m_earth, r_earth) = (5.972e24, 6.371e6);
        let (m_moon, r_moon) = (7.342e22, 1.737e6);
        let bind = |m: f64, r: f64| 0.6 * g * m * m / r;
        let earth_binding = bind(m_earth, r_earth); // ~2.2e32 J
        let moon_binding = bind(m_moon, r_moon); // ~1.2e29 J
        let impact = 4.5e30; // J — the Moon dropped onto the Earth

        // The impact dwarfs the Moon's binding energy → the Moon is disrupted…
        assert_eq!(
            ground_effect(impact, 1.0e7, moon_binding),
            GroundEffect::Disruption
        );
        // …but it's a small fraction of the Earth's binding energy → the Earth survives (cratered).
        assert!(
            impact < 0.1 * earth_binding,
            "Earth is not disrupted by the Moon"
        );
        assert!(matches!(
            ground_effect(impact, 1.0e7, earth_binding),
            GroundEffect::Crater { .. }
        ));
    }

    #[test]
    fn impact_fractures_then_melts_then_vaporizes_by_energy_density() {
        let mats = crate::materials::load();
        let basalt = &mats[crate::materials::index_of(&mats, "basalt")];
        let sigma = basalt.fracture_strength as f64;
        let em = melt_energy_density(basalt).unwrap();
        let ev = vapor_energy_density(basalt).unwrap();

        // Ordered thresholds: fracture < melt < vaporize (all higher energy densities).
        assert!(
            sigma < em && em < ev,
            "σ {sigma:.2e} < melt {em:.2e} < vapor {ev:.2e}"
        );

        // A single impact produces ALL of these at once — near-field vaporizes, mid melts, far
        // fractures — because the deposited energy density falls with distance. (Also a scale-of-detail
        // test: one event, several material fates.)
        assert_eq!(
            classify(sigma * 0.5, basalt, ONE_ATM_PA),
            PhaseChange::Intact
        );
        assert_eq!(
            classify((sigma + em) * 0.5, basalt, ONE_ATM_PA),
            PhaseChange::Fractured
        );
        assert_eq!(
            classify((em + ev) * 0.5, basalt, ONE_ATM_PA),
            PhaseChange::Melted
        );
        assert_eq!(
            classify(ev * 2.0, basalt, ONE_ATM_PA),
            PhaseChange::Vaporized
        );

        // Planetary-scale sanity: a giant impact vaporizes rock (real giant impacts do — magma ocean +
        // rock-vapour atmosphere).
        assert_eq!(classify(1.0e12, basalt, ONE_ATM_PA), PhaseChange::Vaporized);

        // Wood has no melting point at ANY pressure — it pyrolyses. However much energy arrives, it can
        // never be classified molten or vaporised; it chars. (This used to read `oak.thermal.is_none()`,
        // back when the honesty came from having no data at all. The data is now sourced, and the honesty
        // comes from the data saying what is actually true of wood.)
        let oak = &mats[crate::materials::index_of(&mats, "oak")];
        assert!(oak.melt_point().is_none(), "wood does not melt");
        assert_eq!(classify(1.0e12, oak, ONE_ATM_PA), PhaseChange::Decomposed);
        assert_eq!(
            classify(1.0e12, oak, 1.0e11),
            PhaseChange::Decomposed,
            "not even under pressure"
        );

        // LIMESTONE, though, is decided by pressure — Robin's correction. On a kiln floor it calcines;
        // inside an impact the CO₂ cannot escape, the reaction is suppressed, and the same rock melts.
        let lime = &mats[crate::materials::index_of(&mats, "limestone")];
        assert_eq!(
            classify(1.0e12, lime, ONE_ATM_PA),
            PhaseChange::Decomposed,
            "at 1 atm it calcines"
        );
        assert_eq!(
            classify(1.0e12, lime, 1.0e9),
            PhaseChange::Melted,
            "under a kilobar it melts"
        );
    }
}
