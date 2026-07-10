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
}
