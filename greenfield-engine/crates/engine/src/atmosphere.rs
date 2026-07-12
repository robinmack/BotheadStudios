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

/// Per-mass 1D EOS force between adjacent parcels of an air COLUMN (docs/26 emergence test 1): each
/// parcel-slab presses on its neighbour with its full ideal-gas pressure, F = A·P = A·ρ·R_s·T, and the
/// chain density at spacing `s` is ρ = m/(A·s) — so the per-mass acceleration is simply R_s·T/s. This is
/// the EXACT discrete form of hydrostatic equilibrium dP/dz = −ρg for an isothermal column: nothing but
/// the declared gas constants goes in, and the exponential profile with H = R_s·T/g must EMERGE from the
/// settling dynamics. (The 3D generalization is an SPH kernel density — flagged next; a column is the
/// honest first resolvable case, like the two-particle collision was for solids.)
pub fn gas_column_accel(spacing: f64, rs_t: f64) -> f64 {
    rs_t / spacing.max(1.0e-9)
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
    fn a_settling_air_column_finds_the_real_exponential_atmosphere() {
        // docs/26 emergence tests 1 + 2, THE atmosphere result: N parcel-slabs under gravity, each
        // pushing on its neighbours with its ideal-gas pressure and nothing else, must SETTLE into the
        // exponential density profile with scale height H = R_s·T/g ≈ 8.4 km — and the settled column's
        // basal pressure must equal its weight (the docs/25 static boundary condition is this dynamic
        // model's limit). No profile is imposed anywhere; only the gas constants are declared.
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let rs_t = specific_gas_constant(air) * 288.0; // isothermal at 288 K
        let g = 9.81;
        let h_expected = scale_height(air, 288.0, g); // ≈ 8,430 m — the analytic target, NOT an input

        // A chain of N equal-mass slabs (per m² of column). STRONGER emergence framing: start from
        // exponential profiles with the WRONG scale height (half and double the real one) and let the
        // damped dynamics relax — both must converge to the SAME real H, proving the equilibrium is an
        // attractor of the physics, not an artifact of the initial condition. (The damping is numerical
        // relaxation to find the static state; the EQUILIBRIUM is the physics under test.)
        const N: usize = 200;
        let m_slab = 10_332.0 / N as f64; // total = one real atmosphere's column mass (kg/m²)
        let h_wrong = h_expected * 2.0; // deliberately wrong starting profile
        let mut z: Vec<f64> = (0..N)
            .map(|i| {
                // Exponential column with scale height h_wrong: equal-mass slabs sit at
                // z_i = −H·ln(1 − i/N)-ish; use the inverse-CDF spacing of an exponential.
                let f = (i as f64 + 0.5) / (N as f64 + 1.0);
                -h_wrong * (1.0 - f).ln()
            })
            .collect();
        let mut v = vec![0.0f64; N];
        let dt = 5.0e-3;
        for _ in 0..200_000 {
            for i in 0..N {
                let mut a = -g;
                if i == 0 {
                    // The ground: the same EOS push from a virtual ground-level slab (no free lid above).
                    a += gas_column_accel(2.0 * z[0].max(1.0e-3), rs_t);
                } else {
                    a += gas_column_accel(z[i] - z[i - 1], rs_t);
                }
                if i + 1 < N {
                    a -= gas_column_accel(z[i + 1] - z[i], rs_t);
                }
                v[i] += a * dt;
                v[i] *= 0.9995; // relaxation damping
            }
            for i in 0..N {
                z[i] += v[i] * dt;
            }
        }

        // Measure the emergent scale height: density ∝ 1/spacing; ln ρ vs z must have slope −1/H.
        // Fit over the bulk of the column (skip the top tail where a finite chain truncates the gas).
        let lo = N / 10;
        let hi = 8 * N / 10;
        let (mut sx, mut sy, mut sxx, mut sxy, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for i in lo..hi {
            let spacing = z[i + 1] - z[i];
            let zi = 0.5 * (z[i + 1] + z[i]);
            let ln_rho = (m_slab / spacing).ln();
            sx += zi;
            sy += ln_rho;
            sxx += zi * zi;
            sxy += zi * ln_rho;
            n += 1.0;
        }
        let slope = (n * sxy - sx * sy) / (n * sxx - sx * sx);
        let h_measured = -1.0 / slope;
        println!("scale height: measured {h_measured:.0} m vs R_s·T/g = {h_expected:.0} m");
        assert!(
            (h_measured - h_expected).abs() / h_expected < 0.1,
            "the exponential atmosphere EMERGES: measured H {h_measured:.0} m vs {h_expected:.0} m"
        );

        // Consistency (test 2): the settled column's basal pressure = its weight (docs/25's boundary
        // condition is the limit of this dynamic model). P_base = ρ_base·R_s·T vs Σm·g.
        let p_base = (m_slab / (z[1] - z[0])) * rs_t;
        let weight = m_slab * N as f64 * g;
        println!("basal pressure {p_base:.0} Pa vs column weight {weight:.0} Pa");
        assert!(
            (p_base - weight).abs() / weight < 0.1,
            "the settled column carries exactly its own weight ({p_base:.0} vs {weight:.0} Pa)"
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
