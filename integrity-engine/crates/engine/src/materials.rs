//! Loads the cited material database (`data/materials.json`) that ships with the engine.
//!
//! Phase 1 only needs each material's **density** (the physical source of truth) and **albedo**
//! (for rendering). We deserialize just those fields; serde ignores the rest. Later phases will
//! read the full mechanical/optical property set (see `docs/04-materials-model.md`).

use serde::Deserialize;

/// The material database, embedded at compile time so the WASM is self-contained.
pub(crate) const MATERIALS_JSON: &str = include_str!("../../../data/materials.json");

#[derive(Deserialize)]
struct RawFile {
    materials: Vec<RawMaterial>,
}

#[derive(Deserialize)]
struct RawMaterial {
    id: String,
    #[serde(default)]
    phase: String, // "solid" | "granular" | "liquid" | …
    mechanical: RawMechanical,
    optical: RawOptical,
    #[serde(default)]
    thermal: Option<RawThermal>,
    #[serde(default)]
    reaction: Option<Reaction>,
    #[serde(default)]
    tillotson: Option<TillotsonBlock>,
}

#[derive(Deserialize)]
struct RawThermal {
    specific_heat: f32,       // J/(kg·K)
    melt_point: f32,          // K
    latent_fusion: f32,       // J/kg
    boil_point: f32,          // K
    latent_vaporization: f32, // J/kg
    #[serde(default)]
    simon_a: f32, // Pa — Simon–Glatzel melting-curve pressure scale (0 = curve not characterized)
    #[serde(default)]
    simon_c: f32, // dimensionless Simon–Glatzel exponent
    #[serde(default)]
    molar_mass: f32, // kg/mol — for the Clausius–Clapeyron boiling curve (0 = not characterized)
    #[serde(default)]
    decomposes_k: f32, // K — irreversible breakdown instead of melting (0 = does not / not characterized)
    #[serde(default)]
    decomposition_suppressed_pa: f32, // Pa — above this confining pressure the breakdown cannot proceed
    /// W/(m·K) — how fast heat travels THROUGH the material. 0 = not characterized.
    #[serde(default)]
    thermal_conductivity: f32,
}

#[derive(Deserialize)]
struct RawMechanical {
    /// kg/m^3. Present for every material in the seed database.
    density: f32,
    /// Pa. Elastic (Young's) modulus — resistance to stretch/compress. Drives cohesive-bond stiffness
    /// (a solid is rigid because its bonds are stiff, docs/23). null where not characterized.
    #[serde(default)]
    youngs_modulus: Option<f32>,
    /// Pa. Resistance to being pulled apart; null for liquids. Drives fracture (Phase 3).
    #[serde(default)]
    tensile_strength: Option<f32>,
    /// Pa. Fallback bonding strength where tensile isn't given.
    #[serde(default)]
    cohesion: Option<f32>,
    /// Coulomb friction coefficient μ (dimensionless). For granular debris this drives the contact
    /// friction, from which the angle of repose emerges (`docs/23`).
    #[serde(default)]
    friction_coefficient: Option<f32>,
    /// Coefficient of restitution e (0 = perfectly inelastic, 1 = perfectly elastic). Drives the
    /// contact normal damping — how much of a collision's energy rebounds vs. dissipates (`docs/24`).
    #[serde(default)]
    restitution: Option<f32>,
}

#[derive(Deserialize)]
struct RawOptical {
    /// Linear RGB, each 0..1.
    albedo: [f32; 3],
    #[serde(default)]
    roughness: f32,
    #[serde(default)]
    metallic: f32,
    #[serde(default)]
    color_variance: f32,
    #[serde(default)]
    spectrum: Option<Spectrum>,
    #[serde(default)]
    senescent_spectrum: Option<Spectrum>,
}

/// **What a spectrometer measured this material reflect, wavelength by wavelength.**
///
/// `albedo` is three numbers; this is what those three numbers are a summary OF. Where a material
/// carries one, its albedo is not chosen — it is this convolved against the CIE 1931 observer under
/// the sun (`blackbody::reflectance_srgb`), and `albedo_derives_from_the_measured_spectrum` re-derives
/// it on every run so the stored triple can only ever be a CACHE of the measurement.
///
/// This is also the representation seasonal change needs. A leaf turning in autumn is chlorophyll
/// degrading and carotenoids being unmasked — a different SPECTRUM, not a tint applied to this one —
/// so phenology will add states here rather than a colour multiplier anywhere.
#[derive(Clone, Debug, Deserialize)]
pub struct Spectrum {
    /// Wavelength of the first sample, nm.
    pub lo_nm: f64,
    /// Spacing between samples, nm.
    pub step_nm: f64,
    /// Reflectance 0..1 at `lo_nm + i*step_nm`.
    pub reflectance: Vec<f64>,
}

/// **The chemistry of burning, stored as its SOURCED PRIMARIES rather than as per-kilogram results.**
///
/// Robin (2026-08-02): *"Rapid oxidation will be an important principle in the engine (fires, etc)."*
/// A campfire, a burning ship, a powder charge and a rusting hull are ONE reaction at different rates
/// and different oxidiser availability, so this describes the chemistry once and lets the rate and the
/// oxygen supply belong to the situation rather than to the substance.
///
/// A heat-of-combustion table gives one blended number per fuel. Standard formation enthalpies plus a
/// balanced equation give the same number AND show their working — so the per-kg energy and the oxygen
/// demand are DERIVED below instead of typed, and a reader can see which part is thermodynamics and
/// which part is a particular sample of a messy natural material.
#[derive(Clone, Debug, Deserialize)]
pub struct Reaction {
    /// `"fuel"` (releases energy, consumes oxygen) or `"oxidiser"` (supplies it).
    pub role: String,
    /// kg/mol of the REACTING species — for charcoal this is carbon's, not the lump's.
    pub reactant_molar_mass: f64,
    /// Moles of O2 consumed per mole of reactant. 0 for an oxidiser.
    pub moles_o2_per_mole: f64,
    /// J/mol, the standard enthalpy of FORMATION of the product (negative when heat is released).
    /// 0 for an oxidiser, which releases no combustion energy of its own.
    pub product_formation_enthalpy: f64,
    /// Moles of O2-equivalent the molecule CONTAINS, per mole. **What the molecule holds, not what a
    /// reaction liberates** — how much is actually available depends on the products, which is the
    /// reaction's business rather than the substance's.
    pub oxygen_content: f64,
    /// Moles of PERMANENT GAS this reactant contributes per mole of itself. Condensed products (the
    /// potassium salts, soot) do not count — they carry mass but exert no pressure, and for a gun the
    /// difference between the two is most of the answer.
    pub moles_gas_per_mole: f64,
    /// **The PRODUCTS' properties** — of the reaction this `equation` describes, not of any one
    /// constituent, which is why they sit on the OXIDISER: the oxidiser is what decides which reaction
    /// is happening, since the same fuels burning in air give entirely different products.
    ///
    /// Zero where uncharacterised. A gun needs all four; a campfire needs none of them.
    #[serde(default)]
    pub flame_temperature: f64,
    /// Specific-heat ratio of the products.
    #[serde(default)]
    pub products_gamma: f64,
    /// Noble-Abel covolume of the products, m³/kg — the volume the gas cannot be compressed below.
    #[serde(default)]
    pub products_covolume: f64,
    /// Mean molar mass of the permanent gas, kg/mol.
    #[serde(default)]
    pub products_molar_mass: f64,
    /// The balanced equation these numbers describe, so the stoichiometry is auditable by eye.
    pub equation: String,
}

impl Reaction {
    /// J/kg of reactant released on complete combustion — `-dHf(product) / M`. Carbon comes out at
    /// 32.8 MJ/kg and sulfur at 9.26, from formation enthalpies alone, no combustion table consulted.
    pub fn energy_per_kg(&self) -> f64 {
        if self.reactant_molar_mass <= 0.0 {
            return 0.0;
        }
        -self.product_formation_enthalpy / self.reactant_molar_mass
    }

    /// kg of O2 needed per kg of this fuel, from the stoichiometry.
    pub fn oxygen_demand(&self) -> f64 {
        if self.reactant_molar_mass <= 0.0 {
            return 0.0;
        }
        self.moles_o2_per_mole * O2_MOLAR_MASS / self.reactant_molar_mass
    }

    /// Moles of permanent gas produced per kg of this reactant.
    pub fn gas_moles_per_kg(&self) -> f64 {
        if self.reactant_molar_mass <= 0.0 {
            return 0.0;
        }
        self.moles_gas_per_mole / self.reactant_molar_mass
    }

    /// kg of O2-equivalent this oxidiser CARRIES per kg of itself — the quantity that decides whether a
    /// reaction needs air at all. Black powder works in a sealed bore because this is non-zero for KNO3.
    pub fn oxygen_carried(&self) -> f64 {
        if self.reactant_molar_mass <= 0.0 {
            return 0.0;
        }
        self.oxygen_content * O2_MOLAR_MASS / self.reactant_molar_mass
    }
}

/// kg/mol of O2 — the one reagent every oxidation shares, so its molar mass has ONE home rather than
/// being retyped beside each fuel (Law II).
pub const O2_MOLAR_MASS: f64 = 0.0319988;

/// Thermal properties — enough to compute the energy to melt or vaporize the material (`docs/20`).
/// Optional: only materials we've cited thermal data for carry it; without it, an impact can fracture
/// the material but we don't claim to know its melt/boil behaviour (honesty).
#[derive(Clone, Debug)]
pub struct Thermal {
    pub specific_heat: f32,       // J/(kg·K)
    pub melt_point: f32,          // K (at 1 atm)
    pub latent_fusion: f32,       // J/kg (solid → liquid)
    pub boil_point: f32,          // K
    pub latent_vaporization: f32, // J/kg (liquid → gas)
    /// Simon–Glatzel melting-curve coefficients: T_m(P) = melt_point·(1 + P/simon_a)^(1/simon_c).
    /// Pressure RAISES most materials' melting points — this is why Earth's inner core is SOLID even
    /// though it is hotter than the molten outer core (the emergence test in `planet.rs`). simon_a in
    /// Pa; 0 ⇒ curve not characterized ⇒ melt_point is used flat (honest fallback, flagged).
    pub simon_a: f32,
    pub simon_c: f32,
    /// kg/mol — the vapor's molar mass, for the Clausius–Clapeyron boiling curve. 0 ⇒ not characterized
    /// ⇒ boil_point is used flat (honest fallback, flagged).
    pub molar_mass: f32,
    /// K — the temperature at which this material breaks down IRREVERSIBLY instead of melting, and the
    /// reason `melt_point` is 0 for several entries.
    ///
    /// Wood pyrolyses, limestone calcines (CaCO₃ → CaO + CO₂ above 825 °C), rubber and concrete break
    /// down. None of them has a melting point, and filling one in to close a gap in the table would have
    /// been inventing physics rather than sourcing it. A decomposed material does not come back when it
    /// cools, which is the difference that matters: melting is reversible and this is not.
    ///
    /// 0 ⇒ does not decompose (or not characterized).
    pub decomposes_k: f32,
    /// **Thermal conductivity k** (W/(m·K)) — how fast heat travels through this material, and therefore
    /// how much of a body actually participates when its surface is heated.
    ///
    /// This is what separates a body that heats through from one that grows a hot SKIN over a cold
    /// interior: with `specific_heat` and `density` it gives the thermal diffusivity α = k/(ρc), whose
    /// square root sets the depth a heat front reaches in a given time ([`Material::thermal_diffusivity`]).
    /// `docs/04` reserved this field and it sat unused; entering the atmosphere is what needed it, because
    /// heating a metre-wide body's whole mass at once made it impossible for one to glow (docs/46 row 21).
    ///
    /// 0 ⇒ not characterized ⇒ callers must not claim to know how the heat spreads (only `hh_plasma`).
    pub thermal_conductivity: f32,
    /// Pa — the confining pressure above which decomposition CANNOT proceed, so the material melts
    /// instead.
    ///
    /// Melting and decomposition are not properties a material has one of; they are a RACE, and pressure
    /// decides it. Calcite calcines at 1,098 K at one atmosphere only because the CO₂ can escape: squeeze
    /// it and Le Chatelier pushes the reaction back, the breakdown temperature climbs past the melting
    /// curve, and it melts (~1,612 K near a kilobar) — which is exactly the regime inside an impact.
    /// Concrete behaves the same way, and concrete melts are observed in real fires and accidents.
    ///
    /// 0 ⇒ nothing suppresses it (or not characterized).
    pub decomposition_suppressed_pa: f32,
}

impl Thermal {
    /// Melting point (K) at pressure `p` (Pa) — Simon–Glatzel, or the flat 1-atm value when the curve
    /// isn't characterized.
    pub fn melt_point_at(&self, p: f64) -> f64 {
        let t0 = self.melt_point as f64;
        if self.simon_a > 0.0 && self.simon_c > 0.0 {
            t0 * (1.0 + p / self.simon_a as f64).powf(1.0 / self.simon_c as f64)
        } else {
            t0
        }
    }

    /// Boiling point (K) at ambient pressure `p` (Pa) — Clausius–Clapeyron from the 1-atm boil point,
    /// the latent heat of vaporization, and the vapor's molar mass:
    /// 1/T_b(P) = 1/T_b0 − (R_u/(M·L))·ln(P/P_atm). Lower pressure ⇒ lower boiling point; as P → 0
    /// (vacuum) the boiling point → 0 K, i.e. a liquid exposed to space boils at ANY temperature — the
    /// physical reason open water cannot exist without an atmosphere (planet.rs surface-phase test).
    /// Flat 1-atm fallback when the molar mass isn't characterized (flagged). Approximation: constant L
    /// (real L varies ~10% with T — e.g. water's triple point comes out ~268.5 K vs the real 273.16).
    pub fn boil_point_at(&self, p: f64) -> f64 {
        const R_U: f64 = 8.314; // J/(mol·K)
        const P_ATM: f64 = 101_325.0;
        let t0 = self.boil_point as f64;
        if self.molar_mass <= 0.0 || self.latent_vaporization <= 0.0 {
            return t0;
        }
        if p <= 0.0 {
            return 0.0; // vacuum: boils at any temperature
        }
        let k = R_U / (self.molar_mass as f64 * self.latent_vaporization as f64);
        let inv_t = 1.0 / t0 - k * (p / P_ATM).ln();
        if inv_t <= 0.0 {
            f64::INFINITY // enormous pressure: boiling suppressed entirely (supercritical caveats flagged)
        } else {
            1.0 / inv_t
        }
    }
}

/// The Tillotson equation-of-state parameters for a condensed-matter material (`docs/33`, consumed by
/// `eos::Tillotson`). SI throughout. **This is the source of truth for the EOS**: the parameters used to
/// live as constants in `eos.rs`; they now live here so a world is a world is a world — one place to
/// improve a material improves every scene that uses it.
///
/// `status` records provenance honestly, because a physics parameter that quietly lies is worse than one
/// openly flagged (Law VII): `"verified"` (checked against the primary table), `"partial"` (some params
/// verified, others provisional), or `"provisional"` (transcribed, not yet confirmed). `source` carries
/// the citation. Deserialized straight from `data/materials.json`'s `tillotson` block; the literature
/// symbols A, B, E0, E_iv, E_cv are the JSON keys.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TillotsonBlock {
    /// Reference (zero-pressure, cold) density ρ₀ (kg/m³).
    pub rho0: f64,
    /// Nondimensional Tillotson `a`.
    pub a: f64,
    /// Nondimensional Tillotson `b`.
    pub b: f64,
    /// Bulk modulus at ρ₀ — the Tillotson `A` (Pa).
    #[serde(rename = "A")]
    pub cap_a: f64,
    /// Second (nonlinear) compression modulus — the Tillotson `B` (Pa).
    #[serde(rename = "B")]
    pub cap_b: f64,
    /// Reference specific internal energy E₀ (J/kg).
    #[serde(rename = "E0")]
    pub e0: f64,
    /// Incipient-vaporization specific energy E_iv (J/kg).
    #[serde(rename = "E_iv")]
    pub e_iv: f64,
    /// Complete-vaporization specific energy E_cv (J/kg).
    #[serde(rename = "E_cv")]
    pub e_cv: f64,
    /// Expansion decay exponent α (nondimensional).
    pub alpha: f64,
    /// Expansion decay exponent β (nondimensional).
    pub beta: f64,
    /// Provenance: `"verified"` | `"partial"` | `"provisional"`.
    #[serde(default)]
    pub status: String,
    /// Cited source(s) for the parameter set.
    #[serde(default)]
    pub source: String,
    /// Optional per-material caveats.
    #[serde(default)]
    pub notes: String,
}

/// A material as the engine consumes it.
#[derive(Clone, Debug)]
pub struct Material {
    pub id: String,
    /// State of matter: "solid" | "granular" | "liquid" | … . Governs the deformation response
    /// (docs/18): solids fracture at their strength, granular media crater and flow, liquids yield at
    /// ~no strength and flow. Data-driven, so a bullet-in-rock and a pebble-in-a-pond are the *same*
    /// operator with different material.
    pub phase: String,
    /// kg/m^3. Authoritative per-material mass; drives self-gravity (voxel mass = density * volume).
    pub density: f32,
    /// Linear-RGB **diffuse reflectance** (0..1) — the fraction of light scattered back, per channel.
    /// HONESTY NOTE: this is a *summary* property, a stand-in for the full spectral, microstructure-
    /// dependent optics (BRDF, specular, subsurface) we don't yet derive from first principles. It is
    /// the source of truth for colour *today*, and coarse-scale appearance is aggregated from it
    /// ([`aggregate_albedo`], `docs/17`) — but it is a placeholder to be grounded later, not an
    /// irreducible fact. Reflectance is not brightness: a low albedo under a bright sun still looks
    /// bright (basalt), so brightness belongs to the lighting, never baked into this number.
    pub albedo: [f32; 3],
    /// Pa. How hard it is to fracture/detach a chunk (Phase 3): rock is high (barely chips), soil and
    /// grass are ~1000× lower (detach easily). Falls back to cohesion, then to "effectively unbreakable".
    pub fracture_strength: f32,
    /// Pa. Young's (elastic) modulus — how stiffly the material resists deformation. A solid is rigid
    /// because its bonds are stiff; the cohesive-aggregate bond stiffness derives from this (`docs/23`).
    /// 0 where not characterized (falls back to a soft default at the call site).
    pub youngs_modulus: f32,
    /// Coulomb friction coefficient μ (dimensionless). HONESTY NOTE: like [`albedo`], this is a
    /// *summary* placeholder, not an irreducible fact. Real friction lives in sub-parcel molecular
    /// roughness/asperities — below voxel resolution (a voxel is ~1e9 molecules), so it can't be
    /// resolved at this LOD and must be a constitutive summary of that unresolved physics. It is the
    /// source of truth for debris friction *today* (the angle of repose emerges from it, `docs/23`),
    /// but the goal is to DERIVE it from contact-bond mechanics at finer scale (`docs/23`'s emergent
    /// static-vs-kinetic friction), never to tabulate or tune it. 0.6 default where not characterized.
    pub friction_coefficient: f32,
    /// Coefficient of restitution e (0..1): the fraction of collision *speed* returned on rebound (so
    /// energy returns as e²). The granular contact derives its normal damping from this, so how bouncy
    /// debris is — and how strongly an impact rebounds into ejecta — is a material property, not a dial
    /// (`docs/24` Stage 1). Like [`friction_coefficient`], a constitutive summary of sub-parcel physics.
    /// 0.5 default where not characterized.
    pub restitution: f32,
    /// Pa. Cohesion — the ATTRACTIVE bond strength between touching grains of this matter (`docs/24`).
    /// This is what lets a pile hold a slope (soil, wet sand) that a cohesionless pile (dry sand) can't,
    /// and it closes the zero-overlap "frictionless graze". NOTE: this is the INTACT cohesion; loose
    /// debris (already fractured) retains only a fraction, so the granular contact caps it at a granular
    /// ceiling (a flagged approximation). 0 where not characterized (cohesionless).
    pub cohesion: f32,
    /// 0 (mirror) .. 1 (matte). Drives specular highlight width (Phase 4).
    pub roughness: f32,
    /// 0 (dielectric) .. 1 (metal). Metals get a tinted, tighter highlight (sparkle).
    pub metallic: f32,
    /// 0 (uniform) .. 1 (high per-grain spread). Drives procedural texture contrast (Phase 4).
    pub color_variance: f32,
    /// **The measured reflectance spectrum this material's [`albedo`](Material::albedo) summarises**,
    /// where one has been sourced. `None` for the materials whose colour is still three chosen
    /// numbers — which is most of them, and is exactly what the `albedo` field's honesty note is
    /// about. A material that has one is a material whose colour is a measurement.
    pub spectrum: Option<Spectrum>,
    /// **What this material reflects when its chlorophyll is spent** — the other end of a season.
    ///
    /// `None` for everything that does not senesce, which is almost everything, and notably for
    /// `conifer_foliage`: an evergreen having no senescent state is the model's own testable
    /// prediction, not an omission.
    pub senescent_spectrum: Option<Spectrum>,
    /// Thermal properties for melt/vaporization (`docs/20`), when we have cited data for the material.
    /// `None` for the 11 of 24 materials whose thermal data has not been sourced — an honest gap marker,
    /// NOT a licence to invent one. Ask through [`Material::specific_heat`] and friends rather than
    /// `map_or`-ing a number in at the call site (see those methods for what went wrong).
    pub thermal: Option<Thermal>,
    /// **What this material does in an OXIDATION reaction** — `None` for matter that neither burns nor
    /// supplies oxygen, which is most of the catalogue.
    ///
    /// Robin (2026-08-02): *"Rapid oxidation will be an important principle in the engine (fires, etc)."*
    /// A campfire, a burning ship, a powder charge and a rusting hull are ONE reaction at different
    /// rates and different oxidiser availability, so this describes the chemistry once and lets the
    /// rate and the oxygen supply belong to the situation.
    pub reaction: Option<Reaction>,
    /// Condensed-matter equation of state (Tillotson) — `None` for materials with no characterized EOS
    /// (gases use the ideal-gas closure; wood/soils fall back to the contact-penalty stiffness). Read
    /// through [`tillotson_block`] / `eos::Tillotson`, which treat this as the source of truth.
    pub tillotson: Option<TillotsonBlock>,
}

impl Material {
    /// Specific heat capacity (J/kg/K), or `None` when this material has no sourced thermal data.
    ///
    /// **This exists because the same missing number was being invented three different ways**: 840 in
    /// `impact.rs`, 1000 in `aggregate.rs`, 1000 in `matter.rs` — one question with three answers, each a
    /// stand-in for data nobody had. A quantity that is unknown must stay unknown at the boundary; the
    /// caller then decides visibly whether it can proceed, instead of a plausible constant flowing into a
    /// heat budget and out the other side as a temperature.
    pub fn specific_heat(&self) -> Option<f64> {
        self.thermal.as_ref().map(|t| t.specific_heat as f64)
    }

    /// Boiling point (K), or `None` when unsourced. Defaulting this to infinity — as `impact.rs` did —
    /// silently makes a material unvaporizable, so shock-heated debris of unknown composition could never
    /// turn to gas no matter how much energy it absorbed.
    pub fn boil_point(&self) -> Option<f64> {
        self.thermal
            .as_ref()
            .map(|t| t.boil_point as f64)
            .filter(|v| *v > 0.0)
    }

    /// Latent heat of vaporization (J/kg, liquid → gas), or `None` when unsourced — the energy per unit
    /// mass an ablating body sheds once its surface reaches the boiling point.
    /// Thermal conductivity k (W/(m·K)), or `None` if this material's heat transport is uncharacterised.
    pub fn thermal_conductivity(&self) -> Option<f64> {
        let k = self
            .thermal
            .as_ref()
            .map_or(0.0, |t| t.thermal_conductivity) as f64;
        (k > 0.0).then_some(k)
    }

    /// **Thermal diffusivity α = k/(ρ·c)** (m²/s) — the rate a temperature front travels through this
    /// material, and the only thing needed to answer "how deep has the heat got?": a front reaches
    /// √(α·t) in time t.
    ///
    /// It is a ratio of three MEASURED quantities, so the spread between materials is real and large:
    /// iron comes out at 1.5e-5 m²/s and basalt at 7.0e-7 — a factor of 21 — which is why an iron
    /// meteorite conducts heat inward and warms through while a stony one grows a millimetre-thin fusion
    /// crust over a cold core. `None` if any of the three is uncharacterised.
    pub fn thermal_diffusivity(&self) -> Option<f64> {
        let k = self.thermal_conductivity()?;
        let c = self.specific_heat()?;
        let rho = self.density as f64;
        (c > 0.0 && rho > 0.0).then_some(k / (rho * c))
    }

    pub fn latent_vaporization(&self) -> Option<f64> {
        self.thermal
            .as_ref()
            .map(|t| t.latent_vaporization as f64)
            .filter(|v| *v > 0.0)
    }

    /// Melting point (K), or `None` when unsourced.
    pub fn melt_point(&self) -> Option<f64> {
        self.thermal
            .as_ref()
            .map(|t| t.melt_point as f64)
            .filter(|v| *v > 0.0)
    }

    /// The temperature at which this material breaks down irreversibly instead of melting, if it does.
    pub fn decomposition_point(&self) -> Option<f64> {
        self.thermal
            .as_ref()
            .map(|t| t.decomposes_k as f64)
            .filter(|v| *v > 0.0)
    }

    /// Does this material break down at `pressure_pa`, or melt? Decomposition that releases a gas is
    /// suppressed by confining pressure, so a rock that calcines on a kiln floor MELTS inside an impact.
    pub fn decomposes_at(&self, pressure_pa: f64) -> bool {
        match (self.decomposition_point(), self.thermal.as_ref()) {
            (Some(_), Some(t)) => {
                let limit = t.decomposition_suppressed_pa as f64;
                limit <= 0.0 || pressure_pa < limit
            }
            _ => false,
        }
    }
}

/// Parse the embedded database. Panics with a clear message if the bundled JSON is malformed
/// (that would be a build-time data error, surfaced immediately in the console).
pub fn load() -> Vec<Material> {
    let file: RawFile =
        serde_json::from_str(MATERIALS_JSON).expect("bundled data/materials.json is invalid JSON");
    file.materials
        .into_iter()
        .map(|m| {
            // A liquid has ~no tensile/shear strength: it yields and flows, it does not hold together.
            // The old `unwrap_or(1e12)` fallback made a fluid *stronger than granite* — a fudge that
            // blocked "pebble in a pond". Liquids yield at ~0; other matter uses its real strength.
            let fracture_strength = if m.phase == "liquid" {
                0.0
            } else {
                m.mechanical
                    .tensile_strength
                    .or(m.mechanical.cohesion)
                    .unwrap_or(1.0e12)
            };
            Material {
                id: m.id,
                phase: m.phase,
                density: m.mechanical.density,
                albedo: m.optical.albedo,
                fracture_strength,
                youngs_modulus: m.mechanical.youngs_modulus.unwrap_or(0.0),
                friction_coefficient: m.mechanical.friction_coefficient.unwrap_or(0.6),
                restitution: m.mechanical.restitution.unwrap_or(0.5),
                cohesion: m.mechanical.cohesion.unwrap_or(0.0),
                roughness: m.optical.roughness,
                metallic: m.optical.metallic,
                color_variance: m.optical.color_variance,
                spectrum: m.optical.spectrum,
                senescent_spectrum: m.optical.senescent_spectrum,
                thermal: m.thermal.map(|t| Thermal {
                    specific_heat: t.specific_heat,
                    melt_point: t.melt_point,
                    latent_fusion: t.latent_fusion,
                    boil_point: t.boil_point,
                    latent_vaporization: t.latent_vaporization,
                    simon_a: t.simon_a,
                    simon_c: t.simon_c,
                    molar_mass: t.molar_mass,
                    decomposes_k: t.decomposes_k,
                    thermal_conductivity: t.thermal_conductivity,
                    decomposition_suppressed_pa: t.decomposition_suppressed_pa,
                }),
                reaction: m.reaction,
                tillotson: m.tillotson,
            }
        })
        .collect()
}

/// The parsed catalogue, cached (the bundled JSON is parsed once). Prefer this over [`load`] for
/// repeated lookups — the EOS constructors call it, and re-parsing 29 materials per call would be waste.
pub fn catalogue() -> &'static [Material] {
    static CACHE: std::sync::OnceLock<Vec<Material>> = std::sync::OnceLock::new();
    CACHE.get_or_init(load).as_slice()
}

/// The Tillotson EOS parameters for a material id, or `None` when it has no characterized condensed-matter
/// EOS. This is the door `eos::Tillotson` reads through, making `data/materials.json` the single source of
/// truth for the parameters (previously constants in `eos.rs`).
pub fn tillotson_block(id: &str) -> Option<&'static TillotsonBlock> {
    catalogue()
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.tillotson.as_ref())
}

/// Find the index of a material by id. Panics if a required material is missing (Phase 1 relies
/// on `granite`, `dirt`, and `grass` existing in the seed set).
pub fn index_of(materials: &[Material], id: &str) -> usize {
    materials
        .iter()
        .position(|m| m.id == id)
        .unwrap_or_else(|| panic!("material '{id}' not found in materials.json"))
}

/// A composition: constituent materials with relative amounts (mass/area/volume fractions — need not
/// sum to 1, they are normalized). This is how an object states *what it is made of*.
pub type Composition = [(usize, f32)];

/// The scale-relative **summary** operator for colour: the fraction-weighted mean albedo of a
/// composition. Zooming out must summarize, but honestly — the summary is *computed from everything
/// we know about the object's constituents*, never hand-picked (`docs/17`). The SAME reduction serves
/// any object at any scale: a shovel of mixed dirt, or a planet's ocean+rock+ice surface. Returns
/// black for an empty/zero-weight composition.
///
/// (Colour first; density and the other summaries reduce the same way. And albedo itself is a
/// placeholder for real optics — see the note on [`Material::albedo`].)
pub fn aggregate_albedo(composition: &Composition, materials: &[Material]) -> [f32; 3] {
    let total: f32 = composition.iter().map(|&(_, f)| f.max(0.0)).sum();
    if total <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let mut acc = [0.0f32; 3];
    for &(mi, f) in composition {
        let w = f.max(0.0) / total;
        let a = materials[mi].albedo;
        acc[0] += a[0] * w;
        acc[1] += a[1] * w;
        acc[2] += a[2] * w;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_albedo_summarizes_real_constituents() {
        let mats = load();
        let water = index_of(&mats, "water");
        let granite = index_of(&mats, "granite");

        // A single-material composition is exactly that material's albedo — no distortion.
        assert_eq!(
            aggregate_albedo(&[(granite, 1.0)], &mats),
            mats[granite].albedo
        );

        // A 50/50 mix is the component-wise mean.
        let mix = aggregate_albedo(&[(water, 1.0), (granite, 1.0)], &mats);
        for (k, &got) in mix.iter().enumerate() {
            let expect = 0.5 * (mats[water].albedo[k] + mats[granite].albedo[k]);
            assert!((got - expect).abs() < 1e-6, "channel {k}");
        }

        // Weights are ratios, not required to sum to 1: 3:1 water:granite.
        let w = aggregate_albedo(&[(water, 3.0), (granite, 1.0)], &mats);
        for (k, &got) in w.iter().enumerate() {
            let expect = (3.0 * mats[water].albedo[k] + mats[granite].albedo[k]) / 4.0;
            assert!((got - expect).abs() < 1e-6, "channel {k}");
        }

        // Nothing known → black (no invented colour).
        assert_eq!(aggregate_albedo(&[], &mats), [0.0, 0.0, 0.0]);
    }

    /// **A material carrying a measured spectrum does not get to choose its colour.**
    ///
    /// `Material::albedo`'s own doc has always called itself *"a summary property, a stand-in for the
    /// full spectral … optics … a placeholder to be grounded later, not an irreducible fact."* For the
    /// materials with a `spectrum` block, this is that grounding: the three numbers in
    /// `data/materials.json` are a CACHE of the convolution, and this re-derives it every run. If
    /// somebody nudges a leaf greener because it looks better, the build goes red and prints the
    /// number physics actually gives.
    ///
    /// The illuminant is the Sun as a 5772 K blackbody — the same `planck` the sky and the stars use,
    /// so a leaf, a star and a hot ejecta parcel are lit and coloured by one law.
    #[test]
    fn albedo_derives_from_the_measured_spectrum() {
        const SUN_K: f64 = 5772.0;
        let mats = load();
        let mut checked = 0;
        // Collect ALL of them before failing: a test that stops at the first stale entry makes you
        // run it once per material to find out what the answers are.
        let mut stale = Vec::new();
        for m in &mats {
            let Some(s) = &m.spectrum else { continue };
            checked += 1;
            let want =
                crate::blackbody::reflectance_srgb(&s.reflectance, s.lo_nm, s.step_nm, SUN_K);
            if (0..3).any(|c| (m.albedo[c] - want[c]).abs() >= 5e-4) {
                stale.push(format!(
                    "  {}\n    stored  {:?}\n    derived [{:.6}, {:.6}, {:.6}]",
                    m.id, m.albedo, want[0], want[1], want[2]
                ));
            }
            // A leaf must be green, and it must be green because the SPECTRUM says so.
            if m.id.contains("foliage") {
                assert!(
                    want[1] > want[0] && want[1] > want[2],
                    "{} is foliage and must come out green, got {want:?}",
                    m.id
                );
            }
        }
        assert!(
            stale.is_empty(),
            "albedo is a CACHE of the measured spectrum and these are stale:\n{}\n\
             Put the derived triples in data/materials.json — do not adjust a spectrum to match a \
             colour somebody liked.",
            stale.join("\n")
        );
        assert!(
            checked >= 2,
            "no material carries a measured spectrum — this test is guarding nothing"
        );
    }

    /// **Foliage is not timber, and the catalogue must be able to tell them apart.**
    ///
    /// Robin (2026-08-03): *"Pine Timber is always the wrong choice for flora though, we should look
    /// for 'pine needles' or 'pine leaves', same with other biomes."* The surface of a forest seen
    /// from anywhere is the CANOPY; timber is the trunk, which is barely visible and is a different
    /// substance with a different colour, a different density and a different thermal response.
    ///
    /// This pins the difference numerically so the two can never be confused again by a map that
    /// reaches for the first "pine"-shaped id it finds.
    #[test]
    fn foliage_is_a_different_substance_from_the_wood_it_grows_on() {
        let mats = load();
        let get = |id: &str| &mats[index_of(&mats, id)];
        let foliage = get("conifer_foliage");
        let timber = get("pine");

        // Timber is BROWN: red is its strongest channel. Foliage is GREEN.
        assert!(
            timber.albedo[0] > timber.albedo[1],
            "pine timber is a brown, {:?}",
            timber.albedo
        );
        assert!(
            foliage.albedo[1] > foliage.albedo[0] && foliage.albedo[1] > foliage.albedo[2],
            "conifer foliage is green, {:?}",
            foliage.albedo
        );
        // And far darker: cut lumber returns several times what a needle does.
        let lum = |a: [f32; 3]| 0.2126 * a[0] + 0.7152 * a[1] + 0.0722 * a[2];
        assert!(
            lum(timber.albedo) > 2.0 * lum(foliage.albedo),
            "timber {:.3} should be much brighter than foliage {:.3} — that difference IS why a \
             forest mapped to timber reads as orange ground",
            lum(timber.albedo),
            lum(foliage.albedo)
        );
        // Different matter, not a shade of the same matter.
        assert!(
            foliage.density > timber.density,
            "live needles at ~100% moisture are denser than dry pine timber"
        );
    }

    #[test]
    fn a_liquid_yields_where_a_solid_resists() {
        // The seed of the unified deformation model (docs/18): the SAME deposited stress yields a
        // fluid but not a solid — the response comes from material data, not per-object code.
        let mats = load();
        let water = &mats[index_of(&mats, "water")];
        let granite = &mats[index_of(&mats, "granite")];

        assert_eq!(water.phase, "liquid");
        // A fluid must not be "unbreakable" — that fudge made water stronger than rock.
        assert!(
            water.fracture_strength < 1.0,
            "water yields trivially (it flows)"
        );
        assert!(granite.fracture_strength > 1.0e6, "granite resists");

        // A gentle poke (1 kPa) displaces the pond but doesn't crack the rock — bullet-in-rock vs
        // pebble-in-pond falls out of the material, not a special case.
        let poke = 1.0e3;
        assert!(poke >= water.fracture_strength, "the poke displaces water");
        assert!(
            poke < granite.fracture_strength,
            "the same poke leaves granite intact"
        );
    }
}

#[cfg(test)]
mod thermal_data_tests {
    /// **An unknown number must stay unknown — and a material that cannot melt must not be given a
    /// melting point.**
    ///
    /// 11 of 24 materials had no thermal data at all, and three call sites were quietly filling the gap
    /// with three different constants (specific heat 840 in `impact.rs`, 1000 in `aggregate.rs`, 1000 in
    /// `matter.rs`), while a fourth defaulted the boiling point to INFINITY — making a material
    /// unvaporizable however much energy it absorbed. The data is now sourced.
    ///
    /// Filling it in surfaced a distinction the table could not previously express: several of those
    /// materials do not melt at all. Wood pyrolyses, limestone calcines, rubber and concrete break down.
    /// Writing a plausible melt point into those rows would have been inventing physics to close a gap.
    #[test]
    fn every_material_reports_what_is_true_of_it_and_nothing_more() {
        let mats = super::load();
        let get = |id: &str| &mats[super::index_of(&mats, id)];

        // Specific heat is measurable for everything, so everything has one.
        for m in &mats {
            assert!(
                m.specific_heat().is_some(),
                "{} must declare a specific heat",
                m.id
            );
            assert!(
                m.specific_heat().unwrap() > 0.0,
                "{} has a positive specific heat",
                m.id
            );
        }

        // MELTERS carry real, citable numbers.
        // (values are stored as f32, so compare within a tolerance rather than bit-for-bit)
        let near = |got: Option<f64>, want: f64, what: &str| {
            let g = got.unwrap_or_else(|| panic!("{what} must be declared"));
            assert!((g - want).abs() < 0.01, "{what}: got {g}, want {want}");
        };
        near(get("copper").melt_point(), 1357.77, "copper melts (CRC)");
        near(
            get("aluminium").melt_point(),
            933.47,
            "aluminium melts (CRC)",
        );
        near(get("ice").melt_point(), 273.15, "ice melts");
        near(get("ice").boil_point(), 373.15, "water boils");

        // DECOMPOSERS declare where they break down.
        for id in ["oak", "pine", "rubber", "limestone", "concrete"] {
            assert!(
                get(id).decomposition_point().is_some(),
                "{id} must declare where it breaks down"
            );
        }
        // The organics have no melting point at all, at any pressure.
        for id in ["oak", "pine", "rubber"] {
            assert_eq!(
                get(id).melt_point(),
                None,
                "{id} does not melt — it pyrolyses"
            );
        }
        // Limestone calcines above 825 °C — CaCO₃ → CaO + CO₂, verified against the calcium-oxide data.
        near(
            get("limestone").decomposition_point(),
            1098.0,
            "limestone calcines",
        );
        // Wood pyrolyses far below any rock's melting point; that ordering must survive.
        assert!(
            get("oak").decomposition_point().unwrap() < get("copper").melt_point().unwrap(),
            "wood breaks down long before metal melts"
        );

        // A material CAN do both, and pressure decides which — Robin's correction, and the physics is
        // the point: calcite calcines at 1,098 K on a kiln floor only because the CO₂ escapes. Confine it
        // and the reaction is pushed back, the breakdown temperature climbs past the melting curve, and
        // the same rock melts near 1,612 K. That is the regime inside any impact.
        let lime = get("limestone");
        assert!(
            lime.decomposition_point().is_some() && lime.melt_point().is_some(),
            "limestone does both"
        );
        assert!(
            lime.decomposes_at(super::super::damage::ONE_ATM_PA),
            "at 1 atm it calcines"
        );
        assert!(
            !lime.decomposes_at(1.0e9),
            "under a kilobar it melts instead"
        );
        // Concrete likewise — concrete melts are observed in real fires and accidents.
        assert!(
            !get("concrete").decomposes_at(1.0e9),
            "concrete melts under pressure"
        );

        // But a material that decomposes with NO suppression pressure does so at any pressure: wood
        // chars however hard you squeeze it, because pyrolysis is not a pressure-reversible reaction the
        // way calcination is.
        for id in ["oak", "pine", "rubber"] {
            assert!(
                get(id).decomposes_at(1.0e11),
                "{id} pyrolyses at any pressure"
            );
            assert_eq!(
                get(id).melt_point(),
                None,
                "{id} has no melting point at all"
            );
        }

        // Crude oil is a MIXTURE: it fractionates across a range, so it has no single boiling point and
        // none was invented for it.
        assert_eq!(
            get("crude_oil").boil_point(),
            None,
            "a mixture has no one boiling point"
        );
        assert!(
            get("crude_oil").specific_heat().is_some(),
            "but its heat capacity is still known"
        );
    }
}

#[cfg(test)]
mod atmospheric_gas_tests {
    /// **Gases are materials.** Standing procedure: when a new substance enters the engine — solid,
    /// liquid or gas — its properties get sourced and catalogued rather than assumed at the point of use.
    /// These five went in because a magma-ocean atmosphere is steam, CO₂ and SO₂, and because Mars is CO₂
    /// and cannot be honest without it.
    ///
    /// The molar masses are the load-bearing numbers: the engine derives a specific gas constant from
    /// them, and a scale height from that. A CO₂ atmosphere is genuinely more compact than an air one at
    /// the same temperature and gravity, and it is this table that makes that true.
    #[test]
    fn the_atmospheric_gases_carry_sourced_properties() {
        let mats = super::load();
        let get = |id: &str| &mats[super::index_of(&mats, id)];

        for id in [
            "carbon_dioxide",
            "sulfur_dioxide",
            "nitrogen",
            "methane",
            "hydrogen",
        ] {
            let m = get(id);
            // (the catalogue records phase as data; here we only need the physical numbers)
            assert!(
                m.specific_heat().is_some(),
                "{id} must carry sourced thermal data"
            );
            assert!(m.density > 0.0, "{id} must have a density");
        }

        // Molar masses, against the values everyone can check.
        let molar = |id: &str| get(id).thermal.as_ref().unwrap().molar_mass as f64;
        assert!(
            (molar("carbon_dioxide") - 0.044).abs() < 0.001,
            "CO₂ is 44 g/mol"
        );
        assert!((molar("nitrogen") - 0.028).abs() < 0.001, "N₂ is 28 g/mol");
        assert!((molar("hydrogen") - 0.002).abs() < 0.0005, "H₂ is 2 g/mol");

        // The consequence that matters: scale height goes as 1/molar mass, so at one temperature and one
        // gravity a CO₂ atmosphere hugs the ground and a hydrogen one puffs out. Same law, different gas.
        let h = |id: &str| crate::atmosphere::scale_height(get(id), 288.0, 9.81);
        assert!(
            h("carbon_dioxide") < h("air"),
            "CO₂ is heavier than air, so its atmosphere is shallower"
        );
        assert!(
            h("hydrogen") > 10.0 * h("air"),
            "hydrogen puffs out — 14× air's scale height"
        );
        assert!(
            h("nitrogen") > h("air"),
            "N₂ alone is slightly lighter than air (which carries O₂ and Ar)"
        );

        // CO₂ has NO liquid phase at one atmosphere — it sublimes. That is why Mars grows frost, not rain.
        let co2 = get("carbon_dioxide");
        assert!(co2.boil_point().unwrap() < co2.melt_point().unwrap(),
            "CO₂ sublimes at 1 atm: its sublimation point (194.7 K) is BELOW the 216.6 K triple-point melt");
    }
}

#[cfg(test)]
mod mixture_tests {
    /// **Black powder is a MIXTURE, not a substance — and this test is what that buys** (docs/64).
    ///
    /// The first attempt at the cannon tried to catalogue `black_powder` as one material and quietly
    /// carried `specific_heat: 1000.0`, a number invented at the keyboard — the exact defect
    /// [`Material::specific_heat`] exists to prevent. It was backed out. Its three constituents are
    /// catalogued instead, each with sourced properties, and the bulk figures DERIVE.
    ///
    /// Two derivations, and **they take different weightings, which is the part worth pinning**:
    ///
    /// * **Specific heat is MASS-weighted.** It is already per-kilogram, so a kilogram of mixture is
    ///   just its constituents' kilograms: `sum(w_i * c_i)`.
    /// * **True density is VOLUME-weighted, i.e. the HARMONIC mean over mass fractions.** A kilogram of
    ///   mixture occupies the sum of its constituents' volumes: `rho = 1 / sum(w_i / rho_i)`. Taking an
    ///   arithmetic mean of densities here would be simply wrong, and wrong in a direction that looks
    ///   plausible.
    ///
    /// ★★ **And the leftover is physics, not error.** The derived TRUE density comes out near 2000
    /// kg/m^3 while poured black powder measures about 1000 — because corned powder is roughly half
    /// void. **That gap IS the porosity**, and porosity is a property of an ARRANGEMENT of matter, not
    /// of the matter itself. Which is the substance-versus-assembly distinction from docs/64 showing up
    /// as a number: the catalogue describes the substance, the packing belongs to whatever holds it.
    #[test]
    fn black_powders_bulk_properties_derive_from_its_constituents() {
        let mats = super::load();
        let get = |id: &str| &mats[super::index_of(&mats, id)];
        // The classic 75/15/10 by MASS (see the `black_powder` discussion in docs/64).
        let mix = [
            (get("potassium_nitrate"), 0.75f64),
            (get("charcoal"), 0.15),
            (get("sulfur"), 0.10),
        ];
        assert!(
            (mix.iter().map(|(_, w)| w).sum::<f64>() - 1.0).abs() < 1e-12,
            "mass fractions must close"
        );

        // Every constituent must carry a SOURCED specific heat — that is the whole reason the mixture
        // can be derived rather than typed.
        for (m, _) in &mix {
            assert!(
                m.specific_heat().is_some_and(|c| c > 0.0),
                "{} needs a sourced specific heat for the mixture to derive",
                m.id
            );
        }

        // Mass-weighted: a kilogram of mixture is its constituents' kilograms.
        let c_mix: f64 = mix
            .iter()
            .map(|(m, w)| w * m.specific_heat().unwrap())
            .sum();
        assert!(
            (700.0..1000.0).contains(&c_mix),
            "a derived specific heat between its constituents' extremes, got {c_mix}"
        );
        // It must lie strictly BETWEEN the extremes of what went in — a mixture cannot be hotter to
        // heat than all of its parts, and a derivation that escaped that range would be a bug.
        let (lo, hi) = mix.iter().fold((f64::MAX, f64::MIN), |(lo, hi), (m, _)| {
            let c = m.specific_heat().unwrap();
            (lo.min(c), hi.max(c))
        });
        assert!(
            c_mix > lo && c_mix < hi,
            "the mixture's {c_mix} must sit inside its constituents' [{lo}, {hi}]"
        );

        // Volume-weighted: a kilogram of mixture occupies the sum of its constituents' volumes.
        let rho_true: f64 = 1.0 / mix.iter().map(|(m, w)| w / m.density as f64).sum::<f64>();
        assert!(
            (1800.0..2100.0).contains(&rho_true),
            "the SOLID mixture's true density, got {rho_true}"
        );
        // The arithmetic mean is the wrong operator here, and this pins that it differs enough to
        // matter — so nobody later "simplifies" the harmonic mean into an average.
        let rho_arith: f64 = mix.iter().map(|(m, w)| w * m.density as f64).sum();
        assert!(
            (rho_arith - rho_true).abs() > 20.0,
            "arithmetic {rho_arith} and volume-weighted {rho_true} must differ enough that using the \
             wrong one is a real error, not a rounding one"
        );

        // ★ The gap to poured black powder (~1000 kg/m^3, the bulk figure quoted for corned powder) is
        // POROSITY — an arrangement, not a substance. Roughly half void.
        let packing = 1000.0 / rho_true;
        assert!(
            (0.4..0.62).contains(&packing),
            "corned powder should be roughly half void; derived packing fraction {packing}"
        );
    }
}

#[cfg(test)]
mod reaction_tests {
    /// **The chemistry of black powder, DERIVED from formation enthalpies — nothing typed.**
    ///
    /// Robin: *"Rapid oxidation will be an important principle in the engine (fires, etc) so this won't
    /// be wasted."* So the first thing built is not a propellant but the general reaction data, and
    /// this pins that the derivation reproduces the textbook figures without any combustion table.
    ///
    /// It also pins the property that makes gunpowder gunpowder: **it carries its own oxygen.** A fire
    /// is air-limited; black powder is not, which is exactly why it works in a sealed bore. That is one
    /// comparison between two numbers, and it is the whole difference.
    #[test]
    fn black_powders_chemistry_derives_from_formation_enthalpies() {
        let mats = super::load();
        let get = |id: &str| &mats[super::index_of(&mats, id)];
        let (kno3, charcoal, sulfur) = (get("potassium_nitrate"), get("charcoal"), get("sulfur"));
        let rx = |m: &super::Material| m.reaction.clone().expect("carries reaction data");
        let (r_k, r_c, r_s) = (rx(kno3), rx(charcoal), rx(sulfur));

        assert_eq!(r_c.role, "fuel");
        assert_eq!(r_s.role, "fuel");
        assert_eq!(r_k.role, "oxidiser");

        // Carbon: -(-393.51 kJ/mol) / 0.0120107 kg/mol = 32.8 MJ/kg. The textbook figure, from the
        // formation enthalpy of CO2 alone.
        let e_c = r_c.energy_per_kg();
        assert!(
            (32.6e6..33.0e6).contains(&e_c),
            "carbon should release ~32.8 MJ/kg, got {e_c:e}"
        );
        // Sulfur: ~9.26 MJ/kg, about a third of carbon's — which is why it is the minority fuel and is
        // there to lower the ignition temperature rather than to carry the energy.
        let e_s = r_s.energy_per_kg();
        assert!(
            (9.1e6..9.4e6).contains(&e_s),
            "sulfur should release ~9.26 MJ/kg, got {e_s:e}"
        );
        assert!(e_c > 3.0 * e_s, "carbon carries the energy, not sulfur");

        // Stoichiometry: one mole of O2 per mole of either fuel, so the mass ratio is just the molar
        // ratio. Carbon is light, so it is thirsty per kilogram: 2.66 kg of O2 for every kg burnt.
        let d_c = r_c.oxygen_demand();
        assert!(
            (2.6..2.7).contains(&d_c),
            "carbon needs ~2.664 kg O2 per kg, got {d_c}"
        );
        assert!(
            (0.99..1.01).contains(&r_s.oxygen_demand()),
            "sulfur is nearly 1:1 by mass with O2 — its molar mass is almost O2's"
        );

        // An OXIDISER releases no combustion energy of its own and demands no oxygen.
        assert_eq!(r_k.energy_per_kg(), 0.0);
        assert_eq!(r_k.oxygen_demand(), 0.0);
        // What it does is CARRY oxygen: 1.5 mol O2-equivalent per mole KNO3 = 0.475 kg per kg.
        let carried = r_k.oxygen_carried();
        assert!(
            (0.46..0.49).contains(&carried),
            "KNO3 carries ~0.475 kg O2 per kg, got {carried}"
        );
        // And the fuels carry none — a fuel that supplied its own oxygen would be a monopropellant.
        assert_eq!(r_c.oxygen_carried(), 0.0);
        assert_eq!(r_s.oxygen_carried(), 0.0);

        // ★★ THE PROPERTY THAT MAKES IT GUNPOWDER. At the classic 75/15/10 by mass, does the KNO3
        // carry enough oxygen for the charcoal and sulfur to burn with NO AIR? Compute both sides.
        let (w_k, w_c, w_s) = (0.75, 0.15, 0.10);
        let supplied = w_k * carried;
        let demanded = w_c * d_c + w_s * r_s.oxygen_demand();
        assert!(
            supplied > 0.5 * demanded,
            "the oxidiser must supply a large share of the demand, or this is just kindling: \
             supplied {supplied:.3} kg/kg vs demanded {demanded:.3} kg/kg"
        );
        // It is deliberately OXYGEN-LEAN — real black powder does not burn its carbon all the way to
        // CO2, which is why it smokes heavily and why its gas yield is far below a smokeless
        // propellant's. Asserting a stoichiometric balance here would be asserting a chemistry the
        // substance does not have.
        assert!(
            supplied < demanded,
            "75/15/10 is oxygen-lean, not balanced: supplied {supplied:.3} vs demanded {demanded:.3}"
        );
    }
}

#[cfg(test)]
mod gun_metal_tests {
    /// **Why bronze guns bulge and iron guns shatter — as two catalogued numbers, not as a story.**
    ///
    /// Cannon were cast in bronze for centuries despite costing far more than iron, for one reason:
    /// bronze is DUCTILE. An overloaded bronze gun swells and splits; an overloaded cast-iron one comes
    /// apart without warning. Nothing in the engine is told that. It follows from `ductility` and from
    /// grey iron's tension/compression asymmetry, and this test pins that the catalogue actually carries
    /// the difference rather than merely naming two metals.
    ///
    /// ★ **The asymmetry is the whole mechanism.** A pressurised barrel's hoop stress is TENSILE, and
    /// tension is precisely the direction grey cast iron is weakest in — roughly a third of its
    /// compressive strength. A material chosen for cannon on the strength of how well it takes a
    /// compressive load is being loaded the other way round.
    #[test]
    fn cast_iron_is_weak_exactly_where_a_barrel_is_loaded() {
        let mats = super::load();
        let get = |id: &str| &mats[super::index_of(&mats, id)];
        let (iron, bronze) = (get("cast_iron"), get("gunmetal"));

        // Both are plausible gun metals: comparable tensile strength.
        assert!(
            (0.7..1.4).contains(&(bronze.fracture_strength / iron.fracture_strength)),
            "bronze {} and cast iron {} are in the same class in TENSION",
            bronze.fracture_strength,
            iron.fracture_strength
        );

        // ★★ **THE OTHER HALF OF THIS TEST CANNOT BE WRITTEN YET, and that is docs/46 row 30 made
        // concrete.** What actually separates these two metals is `ductility` (bronze 0.2, cast iron
        // 0.01) and grey iron's tension/compression asymmetry (295 MPa against ~930). Both are
        // CATALOGUED in `data/materials.json` and neither reaches the engine: `RawMechanical` does not
        // deserialize them, so `Material` cannot carry them and nothing can read them.
        //
        // Reaching around the engine into the raw JSON to assert it here would be measuring ALONGSIDE
        // the code instead of THROUGH it — the mistake that once had a script disagreeing with the
        // engine about Everest by a factor of four. So this test asserts what the engine can actually
        // answer, and the rest is a debt with a row number rather than a clever workaround.
        assert!(
            iron.fracture_strength > 0.0 && bronze.fracture_strength > 0.0,
            "both are real gun metals in the engine's eyes; what it cannot yet see is HOW they fail"
        );
    }

    /// **A hot gun is a different gun** — the thermal side of the same catalogue entries.
    ///
    /// Reloading a hot barrel could set the charge off in the loader's hands, which is why crews
    /// sponged between shots. Whether that happens is a comparison between the barrel's temperature and
    /// the charge's ignition threshold, so the barrel's THERMAL MASS decides how fast a gun may be
    /// fired — a rate of fire that EMERGES from specific heat and conductivity rather than being
    /// declared as a number of rounds per minute.
    ///
    /// Bronze and iron differ in both, so the two gun types heat and shed heat differently, and this
    /// pins that the catalogue carries enough to tell them apart.
    #[test]
    fn a_barrels_thermal_mass_is_catalogued_well_enough_to_limit_a_rate_of_fire() {
        let mats = super::load();
        let get = |id: &str| &mats[super::index_of(&mats, id)];
        for id in ["gunmetal", "cast_iron"] {
            let m = get(id);
            let t = m.thermal.as_ref().expect("a gun metal needs thermal data");
            assert!(
                t.specific_heat > 0.0,
                "{id} needs a specific heat to warm up"
            );
            assert!(
                t.thermal_conductivity > 0.0,
                "{id} needs a conductivity to shed it again"
            );
        }
        // Heat capacity per unit VOLUME is what a barrel actually has, and it is density x specific
        // heat — so the comparison must go through both, not through specific heat alone.
        let cap = |id: &str| {
            let m = get(id);
            m.density as f64 * m.thermal.as_ref().unwrap().specific_heat as f64
        };
        assert!(
            cap("cast_iron") > cap("gunmetal"),
            "an iron barrel stores more heat per unit volume ({:e}) than a bronze one ({:e}), while \
             bronze sheds it faster — different guns, different limits, from the data alone",
            cap("cast_iron"),
            cap("gunmetal")
        );
        assert!(
            get("gunmetal")
                .thermal
                .as_ref()
                .unwrap()
                .thermal_conductivity
                > get("cast_iron")
                    .thermal
                    .as_ref()
                    .unwrap()
                    .thermal_conductivity,
            "bronze conducts heat away faster than grey iron"
        );
    }
}

impl Material {
    /// **What this material looks like when `turned` of it has gone over to its senescent state.**
    ///
    /// Robin: *"wire it up so Ireland actually turns."* This is the wiring: `turned` is the fraction of
    /// the footprint whose chlorophyll is spent (`solar::senescence_fraction`), the two measured spectra
    /// are mixed by it, and the CIE observer turns the mixture into a colour — the SAME derivation that
    /// produced the summer albedo, so a leaf in October and a leaf in June are one law apart.
    ///
    /// ★ Mixing REFLECTANCE, not colour. A footprint part-way through autumn genuinely contains both
    /// kinds of leaf, so a linear spectral mixture is what is physically there — and blending the two
    /// RGB triples instead would give a different, wronger answer, because the observer is not linear
    /// in the way a naive colour lerp assumes.
    ///
    /// ★★ **It is a DECLARED stand-in for pigment chemistry** (docs/46 row 38). The resolved version
    /// carries the pigment complement and sums what each absorbs; Robin's note is why that complement
    /// must be a variable rather than a constant — chlorophyll *a*/*b* give out around 660–680 nm while
    /// *d* and *f* absorb well past it. Two measured endmembers cannot express a pigment nobody
    /// sampled. What this DOES buy is that both ends are measurements rather than choices.
    ///
    /// Materials with no senescent spectrum return their ordinary albedo whatever the season, which is
    /// how an evergreen, a rock and the sea all behave correctly for free.
    pub fn albedo_when_turned(&self, turned: f64) -> [f32; 3] {
        const SUN_K: f64 = 5772.0;
        let (Some(green), Some(dead)) = (&self.spectrum, &self.senescent_spectrum) else {
            return self.albedo;
        };
        let t = turned.clamp(0.0, 1.0);
        if t <= 0.0 {
            return self.albedo;
        }
        // The two states must be sampled the same way, or "mixing" them is comparing two grids.
        if green.lo_nm != dead.lo_nm
            || green.step_nm != dead.step_nm
            || green.reflectance.len() != dead.reflectance.len()
        {
            return self.albedo;
        }
        let mixed: Vec<f64> = green
            .reflectance
            .iter()
            .zip(&dead.reflectance)
            .map(|(g, d)| g * (1.0 - t) + d * t)
            .collect();
        crate::blackbody::reflectance_srgb(&mixed, green.lo_nm, green.step_nm, SUN_K)
    }
}

#[cfg(test)]
mod senescence_tests {
    use super::*;

    /// **A turning leaf gets brighter and yellower, and green stops winning.**
    ///
    /// Not a colour anybody picked: it follows from chlorophyll leaving. The measured endmembers differ
    /// by 5.7x in chlorophyll (1.96 vs 11.10 mg/g), and as it falls the leaf reflects MORE in both the
    /// green and the red — which is what the eye reads as gold.
    #[test]
    fn a_leaf_that_loses_its_chlorophyll_turns() {
        let mats = load();
        let leaf = &mats[index_of(&mats, "broadleaf_foliage")];
        let summer = leaf.albedo_when_turned(0.0);
        let autumn = leaf.albedo_when_turned(1.0);
        assert_eq!(
            summer, leaf.albedo,
            "an unturned leaf is exactly its summer self"
        );
        let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        assert!(
            lum(autumn) > lum(summer),
            "autumn foliage is brighter: {:?} vs {:?}",
            autumn,
            summer
        );
        // Yellower: red climbs relative to green, because the red trough fills in as chlorophyll goes.
        assert!(
            autumn[0] / autumn[1] > summer[0] / summer[1],
            "the red:green ratio must rise, {:.3} -> {:.3}",
            summer[0] / summer[1],
            autumn[0] / autumn[1]
        );
        // And it is monotone, so a half-turned wood sits between the two.
        let half = leaf.albedo_when_turned(0.5);
        assert!(
            lum(half) > lum(summer) && lum(half) < lum(autumn),
            "half-turned must sit between the ends"
        );
    }

    /// **★ An evergreen does not turn, and nothing had to say so.** `conifer_foliage` carries no
    /// senescent spectrum, so it answers with the same colour in January and July — the model's own
    /// prediction, and the reason a Maine hillside reads as mixed forest rather than uniformly gold.
    #[test]
    fn a_conifer_looks_the_same_in_january() {
        let mats = load();
        let fir = &mats[index_of(&mats, "conifer_foliage")];
        assert!(
            fir.senescent_spectrum.is_none(),
            "an evergreen must carry no senescent state"
        );
        for t in [0.0, 0.5, 1.0] {
            assert_eq!(
                fir.albedo_when_turned(t),
                fir.albedo,
                "a conifer is unchanged at turned={t}"
            );
        }
        // Rock and sea likewise — the mechanism must cost nothing for matter that does not live.
        for id in ["granite", "water", "sand"] {
            let m = &mats[index_of(&mats, id)];
            assert_eq!(m.albedo_when_turned(1.0), m.albedo, "{id} has no season");
        }
    }
}

/// **The albedo of a mixture that is part-way through its autumn.**
///
/// [`aggregate_albedo`] for matter with a season: each constituent answers
/// [`Material::albedo_when_turned`] for itself, and the fraction-weighted mean of those answers is the
/// footprint's colour. So a woody savanna's trees can turn while the grass beneath them does something
/// different, and a mixed forest goes partly gold and partly evergreen — which is what a mixed forest
/// does, and what a single material per class could never express.
///
/// ★ The weighting is the constituent FRACTION, which for a land-cover class comes from the class's own
/// definition (IGBP states its classes as canopy-cover thresholds). Nothing here is chosen.
pub fn aggregate_albedo_turned(
    composition: &Composition,
    materials: &[Material],
    turned: f64,
) -> [f32; 3] {
    let total: f32 = composition.iter().map(|&(_, f)| f.max(0.0)).sum();
    if total <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let mut acc = [0.0f32; 3];
    for &(mi, f) in composition {
        let w = f.max(0.0) / total;
        let a = materials[mi].albedo_when_turned(turned);
        for c in 0..3 {
            acc[c] += a[c] * w;
        }
    }
    acc
}

#[cfg(test)]
mod mixture_season_tests {
    use super::*;

    /// **A mixed forest turns PARTLY**, because only part of it is deciduous — and that is the whole
    /// argument for a class being a mixture rather than one material.
    #[test]
    fn a_mixed_forest_turns_less_than_a_deciduous_one() {
        let mats = load();
        let (broad, conif, dirt) = (
            index_of(&mats, "broadleaf_foliage"),
            index_of(&mats, "conifer_foliage"),
            index_of(&mats, "dirt"),
        );
        // IGBP class 4 (deciduous broadleaf) against class 5 (mixed forest), as earth.json states them.
        let deciduous = [(broad, 0.75), (dirt, 0.25)];
        let mixed = [(conif, 0.35), (broad, 0.35), (dirt, 0.30)];
        let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let d_change = lum(aggregate_albedo_turned(&deciduous, &mats, 1.0))
            - lum(aggregate_albedo_turned(&deciduous, &mats, 0.0));
        let m_change = lum(aggregate_albedo_turned(&mixed, &mats, 1.0))
            - lum(aggregate_albedo_turned(&mixed, &mats, 0.0));
        assert!(d_change > 0.0, "a deciduous wood brightens in autumn");
        assert!(
            m_change > 0.0 && m_change < d_change,
            "a mixed forest must turn LESS than a pure deciduous one: {m_change:.4} vs {d_change:.4}"
        );
    }

    /// **Matter with no season is untouched**, whatever fraction is passed — so barren, ice and open
    /// water cost nothing and cannot drift.
    #[test]
    fn a_mixture_of_seasonless_matter_never_changes() {
        let mats = load();
        let barren = [
            (index_of(&mats, "sand"), 0.6),
            (index_of(&mats, "granite"), 0.4),
        ];
        assert_eq!(
            aggregate_albedo_turned(&barren, &mats, 1.0),
            aggregate_albedo(&barren, &mats),
            "the Sahara has no autumn"
        );
    }

    /// At `turned = 0` the seasonal path must reduce EXACTLY to the plain one, or every summer frame
    /// quietly differs from what the non-seasonal code drew.
    #[test]
    fn summer_is_bit_identical_to_the_seasonless_answer() {
        let mats = load();
        let savanna = [
            (index_of(&mats, "grass"), 0.6),
            (index_of(&mats, "broadleaf_foliage"), 0.2),
            (index_of(&mats, "dirt"), 0.2),
        ];
        assert_eq!(
            aggregate_albedo_turned(&savanna, &mats, 0.0),
            aggregate_albedo(&savanna, &mats)
        );
    }
}
