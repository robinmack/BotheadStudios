//! Condensed-matter equation of state — the **Tillotson EOS** (docs/33 stage 1). The missing physics the
//! architecture map (docs/32 §5) called out: today solids resist compression only via a linear-elastic
//! contact penalty (`granular::contact_from_material`, stiffness E·r/m) and planet layer densities are
//! DECLARED constants — so shock-compressed rock has no way to develop pressure from its density. A giant
//! impact needs exactly that: the shock is a pressure wave, and the disk/vapor come from the material's
//! pressure response as it is compressed and then released. Tillotson gives `P(ρ, u)` — pressure as a
//! function of density and specific internal energy — across ALL the states one impact traverses: cold
//! solid, shock-compressed, decompressed/expanded, and vapor. One closure, the giant-impact standard
//! (Tillotson 1962; Melosh, *Impact Cratering*, 1989, App. II; Benz, Cameron & Melosh 1989).
//!
//! This is the SINGLE pressure law the realignment (docs/33) unifies onto — it replaces the split between
//! the ideal-gas vapor EOS (`atmosphere`/`aggregate` `P=ρR_sT`), the linear-elastic contact penalty
//! (solids), and the declared PREM densities (`planet.rs`). The SPH pressure-force machinery
//! (`aggregate.rs` / `atmosphere.rs`, `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W`) is UNCHANGED — only the `P(ρ,u)` call
//! it evaluates changes, which is why a self-gravitating condensed-matter planet is a merge, not new
//! machinery (docs/33 stage 2).
//!
//! The form (SI throughout: ρ kg/m³, u J/kg, P Pa). With η=ρ/ρ₀, μ=η−1, ω=u/(E₀η²)+1:
//! - **Compressed / cold** (η≥1, or u≤E_iv): `P = (a + b/ω)·ρu + A·μ + B·μ²`. A is the ρ₀ bulk modulus, so
//!   cold compression (u→0) gives `P≈A·μ` — a real bulk modulus, not a contact-spring surrogate.
//! - **Expanded & hot** (η<1 and u≥E_cv): the compressed term decays with expansion toward the ideal-gas
//!   limit `a·ρu`, via `exp(−α z²)` and `exp(−β z)` with z=1/η−1.
//! - **Partial vaporization** (η<1, E_iv<u<E_cv): energy-linear blend of the two, so `P(ρ,u)` is continuous.

/// Tillotson parameters for one material. All SI. See the named constructors for cited values.
#[derive(Clone, Copy, Debug)]
pub struct Tillotson {
    /// Reference (zero-pressure, cold) density ρ₀ (kg/m³).
    pub rho0: f64,
    /// Nondimensional Tillotson `a` (the ρu coefficient's constant part; the u→∞ ideal-gas Grüneisen-like term).
    pub a: f64,
    /// Nondimensional Tillotson `b` (the ρu coefficient's ω-weighted part).
    pub b: f64,
    /// Bulk modulus at ρ₀ (Pa) — the linear compression stiffness `A`.
    pub cap_a: f64,
    /// Second-order compression modulus (Pa) — `B`.
    pub cap_b: f64,
    /// Reference specific internal energy E₀ (J/kg).
    pub e0: f64,
    /// Incipient-vaporization specific energy E_iv (J/kg): below this the expanded state stays on the cold branch.
    pub e_iv: f64,
    /// Complete-vaporization specific energy E_cv (J/kg): above this the expanded state is fully on the hot branch.
    pub e_cv: f64,
    /// Expansion decay exponents (nondimensional): α (in z²) and β (in z), z = ρ₀/ρ − 1.
    pub alpha: f64,
    pub beta: f64,
}

impl Tillotson {
    /// Pressure `P(ρ, u)` in Pa. Piecewise over the compressed, expanded-hot, and partial-vapor regions,
    /// continuous across the E_iv / E_cv boundaries by construction.
    pub fn pressure(&self, rho: f64, u: f64) -> f64 {
        let rho = rho.max(1.0e-9);
        let eta = rho / self.rho0;
        let mu = eta - 1.0;
        let p_compressed = self.p_compressed(rho, u, eta, mu);
        // Compressed (η≥1) or cold enough that no vaporization has begun: the cold/compressed branch.
        if eta >= 1.0 || u <= self.e_iv {
            return p_compressed;
        }
        let p_expanded = self.p_expanded(rho, u, eta, mu);
        // Fully vaporized expanded state: the hot branch.
        if u >= self.e_cv {
            return p_expanded;
        }
        // Partial vaporization: energy-linear blend (→ p_compressed at E_iv, → p_expanded at E_cv).
        ((u - self.e_iv) * p_expanded + (self.e_cv - u) * p_compressed) / (self.e_cv - self.e_iv)
    }

    /// Cold / compressed branch: `P = (a + b/ω)·ρu + A·μ + B·μ²`, ω = u/(E₀η²)+1.
    fn p_compressed(&self, rho: f64, u: f64, eta: f64, mu: f64) -> f64 {
        let omega = u / (self.e0 * eta * eta) + 1.0;
        (self.a + self.b / omega) * rho * u + self.cap_a * mu + self.cap_b * mu * mu
    }

    /// Expanded / hot branch: the compressed term decays with expansion toward the ideal-gas limit a·ρu.
    fn p_expanded(&self, rho: f64, u: f64, eta: f64, mu: f64) -> f64 {
        let omega = u / (self.e0 * eta * eta) + 1.0;
        let z = self.rho0 / rho - 1.0; // ≥ 0 in expansion
        self.a * rho * u
            + (self.b * rho * u / omega + self.cap_a * mu * (-self.beta * z).exp())
                * (-self.alpha * z * z).exp()
    }

    /// Adiabatic sound speed squared `c² = ∂P/∂ρ|_u + (P/ρ²)·∂P/∂u|_ρ` (m²/s²), by central differences —
    /// robust and formula-error-proof. Used for the CFL timestep and to read off the bulk modulus. Clamped
    /// ≥0 (the tensile branch can give a locally negative slope; a negative c² is not physical for a wave).
    pub fn sound_speed_sq(&self, rho: f64, u: f64) -> f64 {
        let rho = rho.max(1.0e-9);
        let dr = rho * 1.0e-4;
        let du = u.abs() * 1.0e-4 + 1.0;
        let dp_drho = (self.pressure(rho + dr, u) - self.pressure(rho - dr, u)) / (2.0 * dr);
        let dp_du = (self.pressure(rho, u + du) - self.pressure(rho, u - du)) / (2.0 * du);
        let p = self.pressure(rho, u);
        (dp_drho + p / (rho * rho) * dp_du).max(0.0)
    }

    // ---- Parameter sets. PROVENANCE / HONESTY (docs/33): ----
    // BASALT is VERIFIED against Benz & Asphaug 1999 (Table 2): ρ₀=2700, A=B=26.7 GPa, E₀=487 MJ/kg,
    // E_iv=4.72, E_cv=18.2 MJ/kg, α=β=5 (A = the bulk modulus; B = A). This is the material stage 2a
    // settled with, and it matches exactly.
    // GRANITE, DUNITE, IRON below are the standard Melosh-1989-family values as commonly transcribed in the
    // giant-impact literature — but I have NOT confirmed them against the primary table (Melosh 1989 p.234;
    // Benz, Cameron & Melosh 1989): the source PDFs were not text-extractable online. Treat them as
    // PROVISIONAL — verify against the primary source before making a physics claim that depends on them.
    // (Known correction applied: dunite ρ₀ = 3320, per Chau et al. 2018.) A prior differentiated-body
    // experiment (stage 2b) puffed up, which may reflect a bad transcribed parameter here AND/OR the
    // equal-volume SPH init — both flagged, both to be resolved before the layered planet is trusted.
    // FLAGGED follow-up: migrate the verified sets into data/materials.json (a `tillotson` block alongside
    // `thermal`, docs/04); ANEOS/M-ANEOS are the better-vapor-curve upgrade (the closure is swappable).

    /// Granite (Melosh 1989 — PROVISIONAL, unverified against the primary table). Continental-crust analog.
    pub fn granite() -> Self {
        Tillotson {
            rho0: 2680.0,
            a: 0.5,
            b: 1.3,
            cap_a: 1.8e10,
            cap_b: 1.8e10,
            e0: 1.6e7,
            e_iv: 3.5e6,
            e_cv: 1.8e7,
            alpha: 5.0,
            beta: 5.0,
        }
    }

    /// Basalt (Melosh 1989). The oceanic-crust layer of the engine's layered Earth.
    pub fn basalt() -> Self {
        Tillotson {
            rho0: 2700.0,
            a: 0.5,
            b: 1.5,
            cap_a: 2.67e10,
            cap_b: 2.67e10,
            e0: 4.87e8,
            e_iv: 4.72e6,
            e_cv: 1.82e7,
            alpha: 5.0,
            beta: 5.0,
        }
    }

    /// Dunite / olivine — the engine's PERIDOTITE mantle analog (peridotite is olivine+pyroxene; dunite is
    /// olivine, the standard giant-impact mantle material). PROVISIONAL: ρ₀=3320 is confirmed (Chau et al.
    /// 2018); the moduli/energies are transcribed and UNVERIFIED against the primary table — `cap_b` in
    /// particular is suspect (a differentiated body using it puffed up). Verify before relying on it.
    pub fn peridotite() -> Self {
        Tillotson {
            rho0: 3320.0,
            a: 0.5,
            b: 1.4,
            cap_a: 1.31e11,
            cap_b: 4.9e11,
            e0: 5.5e7,
            e_iv: 4.5e6,
            e_cv: 1.4e7,
            alpha: 5.0,
            beta: 5.0,
        }
    }

    /// Iron core material. COMPRESSED BRANCH verified/open: ρ₀, A, B, a, b, E₀ are the Wissing & Hobbs
    /// (2020, A&A 635 A21, Table 2) Tillotson refit to modern shock data (Brown et al. 2000) — this is all a
    /// STATIC planet needs. VAPOR BRANCH (e_iv/e_cv/α/β) is still the provisional Melosh-1989-family value —
    /// it only bites at impact energies (stage 3); verify against the primary table before then.
    pub fn iron() -> Self {
        Tillotson {
            rho0: 7850.0,   // Wissing & Hobbs 2020
            a: 0.5,         // Wissing & Hobbs 2020
            b: 1.28,        // Wissing & Hobbs 2020
            cap_a: 1.28e11, // A = 128 GPa (Wissing & Hobbs 2020)
            cap_b: 1.815e11, // B = C = 181.5 GPa (Wissing & Hobbs 2020)
            e0: 1.425e7,    // E₀ = 14.25 MJ/kg (Wissing & Hobbs 2020)
            e_iv: 2.4e6,    // PROVISIONAL (Melosh-family; vapor branch, stage-3 concern)
            e_cv: 8.67e6,   // PROVISIONAL
            alpha: 5.0,
            beta: 5.0,
        }
    }

    /// Look up the cited parameter set by the engine's material name (as in `data/materials.json`). Returns
    /// `None` for materials without a characterized condensed-matter EOS (e.g. water/ice/soil — flagged
    /// follow-up), so callers can fall back to the contact-penalty stiffness.
    pub fn for_material(name: &str) -> Option<Self> {
        match name {
            "granite" => Some(Self::granite()),
            "basalt" => Some(Self::basalt()),
            "peridotite" => Some(Self::peridotite()),
            "iron" => Some(Self::iron()),
            _ => None,
        }
    }
}

/// A pluggable equation of state for the shared SPH machinery (docs/33 stage 5 — unify the containers). The
/// SPH density and symmetric pressure-force loops (`a = −Σ m (P_i/ρ_i² + P_j/ρ_j²) ∇W`) are IDENTICAL whether
/// the parcel is air or rock — only `P(ρ, u)` differs. Carrying an `Eos` on each particle is the seam that
/// lets ONE SPH container (`hydrostatic::HydroBody`) serve both an ideal-gas atmosphere and a condensed-matter
/// planet, folding the duplicated `atmosphere::AirField` / `aggregate` vapor loops onto the same code path.
#[derive(Clone, Copy, Debug)]
pub enum Eos {
    /// Condensed matter (rock, iron): the Tillotson closure `P(ρ, u)`.
    Tillotson(Tillotson),
    /// Isothermal ideal gas `P = ρ·R_s·T` (air). `rs_t` = R_s·T (J/kg); `u` is ignored (isothermal), matching
    /// `atmosphere::AirField`'s `rho·rs_t`.
    IdealGas { rs_t: f64 },
}

impl Eos {
    /// Pressure `P(ρ, u)` in Pa — the only thing the SPH force loop needs from the EOS.
    pub fn pressure(&self, rho: f64, u: f64) -> f64 {
        match self {
            Eos::Tillotson(t) => t.pressure(rho, u),
            Eos::IdealGas { rs_t } => rho.max(1.0e-9) * rs_t,
        }
    }
    /// Sound speed squared (m²/s²) — for the CFL/Courant timestep. Isothermal ideal gas: `c² = ∂P/∂ρ = R_s·T`.
    pub fn sound_speed_sq(&self, rho: f64, u: f64) -> f64 {
        match self {
            Eos::Tillotson(t) => t.sound_speed_sq(rho, u),
            Eos::IdealGas { rs_t } => *rs_t,
        }
    }
    /// Reference (cold, zero-pressure) density ρ₀ (kg/m³) — for material identification / init. An ideal gas
    /// has no reference density, so returns 0.
    pub fn rho0(&self) -> f64 {
        match self {
            Eos::Tillotson(t) => t.rho0,
            Eos::IdealGas { .. } => 0.0,
        }
    }
}

impl From<Tillotson> for Eos {
    fn from(t: Tillotson) -> Self {
        Eos::Tillotson(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATS: &[(&str, fn() -> Tillotson)] = &[
        ("granite", Tillotson::granite),
        ("basalt", Tillotson::basalt),
        ("peridotite", Tillotson::peridotite),
        ("iron", Tillotson::iron),
    ];

    #[test]
    fn cold_reference_state_has_zero_pressure() {
        // At ρ=ρ₀, u=0: μ=0 so the A·μ + B·μ² terms vanish, and the ρu thermal term is 0 → P=0. The
        // zero-pressure cold reference state is the definition of ρ₀; a nonzero P here is a sign error.
        for (name, mk) in MATS {
            let t = mk();
            let p = t.pressure(t.rho0, 0.0);
            assert!(p.abs() < 1.0, "{name}: cold reference P must be ~0, got {p:.3e} Pa");
        }
    }

    #[test]
    fn cold_compression_gives_the_bulk_modulus() {
        // The whole point of an EOS over a contact spring: cold compression develops a REAL bulk modulus.
        // K = ρ·dP/dρ at ρ₀ (u≈0) must equal the material's A within a few percent (higher-order B·μ² and
        // the tiny thermal term are the residual). This is the physical anchor of the parameter set.
        for (name, mk) in MATS {
            let t = mk();
            let u = 1.0; // ~cold
            let dr = t.rho0 * 1.0e-4;
            let dp_drho = (t.pressure(t.rho0 + dr, u) - t.pressure(t.rho0 - dr, u)) / (2.0 * dr);
            let k = t.rho0 * dp_drho;
            let rel = (k - t.cap_a).abs() / t.cap_a;
            assert!(rel < 0.02, "{name}: cold bulk modulus {k:.3e} must match A={:.3e} (rel {rel:.3})", t.cap_a);
        }
    }

    #[test]
    fn compression_monotonically_raises_pressure() {
        // Compressing cold matter past ρ₀ must raise P monotonically (a stiffening solid), and to hundreds
        // of GPa at strong compression — the regime a giant impact reaches.
        for (name, mk) in MATS {
            let t = mk();
            let u = 1.0;
            let p10 = t.pressure(t.rho0 * 1.10, u); // 10% compression
            let p30 = t.pressure(t.rho0 * 1.30, u); // 30% compression
            assert!(p10 > 0.0 && p30 > p10, "{name}: P must rise with compression ({p10:.2e} → {p30:.2e})");
            // 30% compression reaches GPa scale (≈ A·μ + B·μ²) — the giant-impact regime. Loose absolute
            // bound: the exact value scales with each material's A (granite ~7 GPa, iron/peridotite ~100+).
            assert!(p30 > 1.0e9, "{name}: 30% compression must reach >1 GPa, got {p30:.2e}");
        }
    }

    #[test]
    fn hot_expansion_relaxes_toward_vanishing_pressure() {
        // A parcel decompressed well below ρ₀ AND fully vaporized (u > E_cv) is on the hot branch, where the
        // condensed terms have decayed away: P → a·ρu (small, gas-like), and far below the cold-compressed
        // pressure at the same density. This is the vapor the disk expands into.
        for (name, mk) in MATS {
            let t = mk();
            let rho = t.rho0 * 0.3; // strongly expanded
            let u_hot = t.e_cv * 3.0;
            let p_hot = t.pressure(rho, u_hot);
            let p_ideal = t.a * rho * u_hot;
            // On the fully-expanded hot branch the exp() terms are ~0, so P ≈ a·ρu.
            assert!(
                (p_hot - p_ideal).abs() < 0.05 * p_ideal.abs().max(1.0),
                "{name}: hot expanded P {p_hot:.2e} must approach the ideal-gas limit {p_ideal:.2e}"
            );
        }
    }

    #[test]
    fn pressure_is_continuous_across_the_vaporization_boundaries() {
        // The piecewise closure must be continuous in u at E_iv and E_cv for an expanded parcel (η<1) — a
        // jump would be a spurious pressure discontinuity (numerical shock). Continuity holds by
        // construction (the blend → p_compressed at E_iv, → p_expanded at E_cv); this guards the wiring.
        for (name, mk) in MATS {
            let t = mk();
            let rho = t.rho0 * 0.7;
            // Tiny straddle so the finite slope contributes negligibly (the function is continuous, so the
            // jump → 0 as δ → 0). The scale floor is tied to the bulk modulus A so it does NOT collapse where
            // P crosses zero (iron's tension branch near E_iv) and give a spuriously tight relative tolerance.
            let eps = 1.0e-5;
            for &e_bound in &[t.e_iv, t.e_cv] {
                let below = t.pressure(rho, e_bound * (1.0 - eps));
                let above = t.pressure(rho, e_bound * (1.0 + eps));
                let scale = below.abs().max(above.abs()).max(1.0e-2 * t.cap_a);
                assert!(
                    (below - above).abs() < 1.0e-2 * scale,
                    "{name}: P discontinuous across u={e_bound:.2e} ({below:.3e} vs {above:.3e})"
                );
            }
        }
    }

    #[test]
    fn sound_speed_is_real_and_of_the_expected_order() {
        // At the cold reference state c² ≈ A/ρ₀ (the bulk-modulus sound speed), giving km/s — the right
        // order for rock/iron (basalt ~3 km/s bar, iron ~4 km/s). Confirms sound_speed_sq is wired to P.
        for (name, mk) in MATS {
            let t = mk();
            let c = t.sound_speed_sq(t.rho0, 1.0).sqrt();
            let c_bulk = (t.cap_a / t.rho0).sqrt();
            assert!(c > 1.0e3 && c < 1.5e4, "{name}: bar sound speed {c:.0} m/s out of range");
            let rel = (c - c_bulk).abs() / c_bulk;
            assert!(rel < 0.1, "{name}: c {c:.0} must match √(A/ρ₀) {c_bulk:.0} (rel {rel:.2})");
        }
    }

    #[test]
    fn eos_enum_dispatches_ideal_gas_and_delegates_tillotson() {
        // Ideal gas: P = ρ·rs_t, independent of u (isothermal), and c² = rs_t.
        let rs_t = 287.0 * 288.0; // dry-air R_s · 288 K ≈ 82.7 kJ/kg
        let air = Eos::IdealGas { rs_t };
        for &rho in &[0.5, 1.2, 5.0] {
            for &u in &[0.0, 1.0e5, 1.0e7] {
                assert!((air.pressure(rho, u) - rho * rs_t).abs() < 1e-6, "ideal-gas P = ρ·rs_t");
            }
        }
        assert_eq!(air.sound_speed_sq(1.2, 0.0), rs_t);
        assert_eq!(air.rho0(), 0.0);
        // Tillotson wrapped in the enum must be byte-identical to calling the material directly.
        let t = Tillotson::basalt();
        let e: Eos = t.into();
        for &(rho, u) in &[(2700.0, 1.0e6), (3200.0, 4.0e6), (2000.0, 2.0e7)] {
            assert_eq!(e.pressure(rho, u), t.pressure(rho, u));
            assert_eq!(e.sound_speed_sq(rho, u), t.sound_speed_sq(rho, u));
        }
        assert_eq!(e.rho0(), t.rho0);
    }
}
