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
    Melted,
    Vaporized,
}

/// Energy density (J/m³) to melt the material from `REF_TEMP_K`: `ρ·(c·ΔT_to_melt + L_fusion)`.
/// `None` if we have no thermal data (then we only claim fracture, not melt — honesty).
pub fn melt_energy_density(m: &Material) -> Option<f64> {
    m.thermal.as_ref().map(|t| {
        let per_kg =
            t.specific_heat as f64 * (t.melt_point as f64 - REF_TEMP_K) + t.latent_fusion as f64;
        per_kg * m.density as f64
    })
}

/// Energy density (J/m³) to fully vaporize the material: heat to melt + heat to boil + latent heats.
/// A first model — it uses the solid specific heat throughout and ignores pressure (`docs/20`).
pub fn vapor_energy_density(m: &Material) -> Option<f64> {
    m.thermal.as_ref().map(|t| {
        let per_kg = t.specific_heat as f64 * (t.melt_point as f64 - REF_TEMP_K)
            + t.latent_fusion as f64
            + t.specific_heat as f64 * (t.boil_point as f64 - t.melt_point as f64)
            + t.latent_vaporization as f64;
        per_kg * m.density as f64
    })
}

/// Classify a parcel's fate from the deposited energy density (J/m³) and its material.
pub fn classify(energy_density: f64, m: &Material) -> PhaseChange {
    if energy_density < m.fracture_strength as f64 {
        return PhaseChange::Intact;
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

/// Excavated crater volume (m³) for `energy` (J) into a material of yield `strength` (Pa), strength
/// regime: `E ≈ σ·V`. A fluid (`strength ≈ 0`) holds no crater — it flows back — so this returns 0.
/// This is the SAME σ·V as `matter::impact`, so summary and voxel materialisation match.
pub fn crater_volume(energy: f64, strength: f64) -> f64 {
    if strength <= 0.0 {
        return 0.0;
    }
    energy / strength
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
        let g = 6.674e-11;
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
        assert_eq!(classify(sigma * 0.5, basalt), PhaseChange::Intact);
        assert_eq!(classify((sigma + em) * 0.5, basalt), PhaseChange::Fractured);
        assert_eq!(classify((em + ev) * 0.5, basalt), PhaseChange::Melted);
        assert_eq!(classify(ev * 2.0, basalt), PhaseChange::Vaporized);

        // Planetary-scale sanity: a giant impact vaporizes rock (real giant impacts do — magma ocean +
        // rock-vapour atmosphere).
        assert_eq!(classify(1.0e12, basalt), PhaseChange::Vaporized);

        // Honesty: with no thermal data we only claim fracture, never melt/vaporize.
        let dirt = &mats[crate::materials::index_of(&mats, "dirt")];
        assert!(dirt.thermal.is_none());
        assert_eq!(classify(1.0e12, dirt), PhaseChange::Fractured);
    }
}
