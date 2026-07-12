//! Atmosphere as matter (docs/26): air parcels as particles, governed by the SAME canonical contact
//! machinery as everything else — with one honest difference: a gas's resistance to compression is its
//! EQUATION OF STATE (ideal gas: isentropic bulk modulus K = γ·P), never an elastic modulus. Matter
//! declares what it is; the law reads the right property for its phase.

use crate::granular::Contact;
use crate::materials::Material;

/// Universal gas constant (J/(mol·K)).
const R_U: f64 = 8.314;
/// Heat-capacity ratio γ for diatomic gases (N₂/O₂ air). A composition-derived value is the refinement.
const GAMMA_DIATOMIC: f64 = 1.4;

/// Canonical contact parameters for a GAS parcel of the given material at a reference pressure —
/// the gas-phase sibling of `granular::contact_from_material` (docs/26). Stiffness comes from the
/// isentropic bulk modulus K = γ·P_ref (v0: isothermal reference state, flagged), not Young's modulus;
/// zero cohesion (gases don't bond), zero Coulomb friction (viscosity is the later refinement, flagged).
/// `radius`/`parcel_mass` follow the mass-agnostic model like every other particle.
pub fn gas_contact_from_material(mat: &Material, radius: f64, parcel_mass: f64, p_ref: f64) -> Contact {
    let m = parcel_mass.max(1.0e-30);
    let k_bulk = GAMMA_DIATOMIC * p_ref.max(1.0); // Pa — the gas's real resistance to compression
    // Same per-mass linear form as the solid law (force k_bulk·r per metre of overlap, over mass).
    let stiffness = (k_bulk * radius) / m;
    Contact {
        radius,
        stiffness,
        normal_damp: 0.0, // an ideal-gas parcel collision is elastic (dissipation enters via viscosity later)
        friction: 0.0,
        tangent_damp: 0.0,
        cohesion: 0.0,
        coh_range: 0.0,
    }
}

/// Specific gas constant R_s = R_u/M (J/(kg·K)) from the material's declared molar mass.
pub fn specific_gas_constant(mat: &Material) -> f64 {
    let m = mat.thermal.as_ref().map_or(0.0, |t| t.molar_mass as f64);
    if m > 0.0 {
        R_U / m
    } else {
        0.0
    }
}

/// The scale height H = R_s·T/g (m) — the e-folding height a settled isothermal atmosphere MUST show
/// (docs/26 emergence test 1). For air at 288 K under 9.81 m/s² this is ≈ 8.4 km; nothing but the
/// declared gas constants goes in.
pub fn scale_height(mat: &Material, temp_k: f64, g: f64) -> f64 {
    specific_gas_constant(mat) * temp_k / g.max(1.0e-9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::Aggregate;
    use crate::materials;
    use crate::orbit::Body;
    use glam::DVec3;

    #[test]
    fn airs_declared_constants_give_the_real_gas_constant_and_scale_height() {
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let rs = specific_gas_constant(air);
        assert!((rs - 287.0).abs() < 2.0, "R_s = R_u/M ≈ 287 J/(kg·K) (got {rs:.1})");
        let h = scale_height(air, 288.0, 9.81);
        assert!(
            (8.2e3..8.6e3).contains(&h),
            "scale height ≈ 8.4 km from the declared constants alone (got {h:.0} m)"
        );
    }

    #[test]
    fn air_parcels_released_in_vacuum_expand_freely_and_never_clump() {
        // docs/26 emergence test 3: no cohesion, no fake containment — gas fills whatever it's given.
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let (radius, mass) = (1.0, 1.0);
        let contact = gas_contact_from_material(air, radius, mass, 101_325.0);
        assert!(contact.cohesion == 0.0 && contact.stiffness > 0.0);
        // A small overlapping cluster at rest in vacuum: pressure (contact) must push it apart.
        let mut parcels = Vec::new();
        for i in 0..8 {
            parcels.push(Body {
                pos: crate::impact::fib_dir(i, 8) * (0.8 * radius),
                vel: DVec3::ZERO,
                mass,
            });
        }
        let mut agg = Aggregate::new(parcels, 0.1).with_contact(contact);
        agg.self_gravity = false; // a lab box of air, not a self-gravitating cloud
        let r0 = agg.rms_radius();
        let mut acc = agg.accelerations();
        for _ in 0..800 {
            agg.step(&mut acc, 1.0e-3);
        }
        assert!(
            agg.rms_radius() > 2.0 * r0,
            "the cluster expands (gas fills space; got {:.2}× the initial radius)",
            agg.rms_radius() / r0
        );
    }
}
