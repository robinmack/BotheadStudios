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
/// Measured incompressible-sphere drag coefficient. DECLARED, with the same resolved counterpart as every
/// other shape factor here: [`AirField`] parcels flowing around the body, which produce the force without
/// anyone naming a coefficient. Refinement short of that: `c_d(Mach)`.
const SPHERE_DRAG_CD: f64 = 0.47;
/// Measured emissivity of molten iron (0.42–0.45) — what an incandescent body radiates AS, for both the
/// body and the vapour it sheds (one surface, one number). DECLARED, and a candidate for the optical
/// catalogue alongside albedo, which is where a per-material emissivity belongs.
const MOLTEN_EMISSIVITY: f64 = 0.45;

/// Canonical contact parameters for a GAS parcel of the given material at a reference pressure —
/// the gas-phase sibling of `granular::contact_from_material` (docs/26). Stiffness comes from the
/// isentropic bulk modulus K = γ·P_ref (v0: isothermal reference state, flagged), not Young's modulus;
/// zero cohesion (gases don't bond), zero Coulomb friction (viscosity is the later refinement, flagged).
/// `radius`/`parcel_mass` follow the mass-agnostic model like every other particle.
pub fn gas_contact_from_material(
    mat: &Material,
    radius: f64,
    parcel_mass: f64,
    p_ref: f64,
) -> Contact {
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
        shock: 1.0, // gas: the sub-parcel shock closure ON (see granular::Contact::shock)
    }
}

/// Specific gas constant R_s = R_u/M (J/(kg·K)) from the material's declared molar mass.
/// Cubic spline SPH kernel W(r, h), 3D-normalized (σ = 8/(π h³)), support 0..h. The ONE kernel used by
/// both the air field ([`AirField`]) and the impact vapor (`aggregate`'s SPH pressure) — docs/23: one law.
pub fn sph_w(r: f64, h: f64) -> f64 {
    let q = r / h;
    let sigma = 8.0 / (std::f64::consts::PI * h.powi(3));
    if q < 0.5 {
        sigma * (6.0 * (q * q * q - q * q) + 1.0)
    } else if q < 1.0 {
        sigma * 2.0 * (1.0 - q).powi(3)
    } else {
        0.0
    }
}

/// dW/dr — the cubic-spline kernel gradient magnitude (negative on 0..h ⇒ pressure is repulsive).
pub fn sph_dw(r: f64, h: f64) -> f64 {
    let q = r / h;
    let sigma = 8.0 / (std::f64::consts::PI * h.powi(4));
    if q < 0.5 {
        sigma * (18.0 * q * q - 12.0 * q)
    } else if q < 1.0 {
        sigma * -6.0 * (1.0 - q) * (1.0 - q)
    } else {
        0.0
    }
}

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

/// **A body's atmosphere, as the engine holds it** — the two numbers that describe an isothermal
/// hydrostatic air column, both EMERGENT from the body's own matter, plus the temperature the air sits at.
///
/// This is the FLUID half of "two things met" (docs/58/59): a body carrying one of these has something
/// for another body to collide WITH. It exists as a type so the barometric profile has exactly one
/// implementation and so a scene can hand the engine "this planet's air" without restating the gas laws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirShell {
    /// Density at the surface (kg/m³): `ρ₀ = P₀/(R_s·T)` over the EMERGENT surface pressure — the weight
    /// of the body's declared air column, never a declared pressure.
    pub rho_surface: f64,
    /// The e-folding height (m): `H = R_s·T/g`, over the body's own emergent surface gravity.
    pub scale_height_m: f64,
    /// The air's temperature (K) — the floor a body in it radiates against, and what set `ρ₀` and `H`.
    pub ambient_temp_k: f64,
}

impl AirShell {
    /// The air of a body with `surface_pressure` (Pa, emergent) made of gas `air` at `temp_k` under
    /// surface gravity `g`. An airless body — no declared atmosphere mass, or a gas with no characterised
    /// molar mass — yields a shell of zero density: vacuum, honestly, not a small fudge of air.
    pub fn new(surface_pressure: f64, air: &Material, temp_k: f64, g: f64) -> Self {
        let rs = specific_gas_constant(air);
        if rs <= 0.0 || temp_k <= 0.0 || surface_pressure <= 0.0 {
            return AirShell {
                rho_surface: 0.0,
                scale_height_m: 0.0,
                ambient_temp_k: temp_k.max(0.0),
            };
        }
        AirShell {
            rho_surface: surface_pressure / (rs * temp_k),
            scale_height_m: scale_height(air, temp_k, g),
            ambient_temp_k: temp_k,
        }
    }

    /// Density (kg/m³) at altitude `h` (m) above the surface — `ρ(h) = ρ₀·e^(−h/H)`. Below the surface
    /// (`h < 0`) it reports the surface value rather than extrapolating a column that isn't there.
    pub fn density_at(&self, h: f64) -> f64 {
        if self.rho_surface <= 0.0 {
            return 0.0;
        }
        if self.scale_height_m <= 0.0 {
            return self.rho_surface;
        }
        self.rho_surface * (-h.max(0.0) / self.scale_height_m).exp()
    }

    /// Is there any air here at all?
    pub fn exists(&self) -> bool {
        self.rho_surface > 0.0
    }
}

/// **Where an atmosphere ends — DERIVED, never declared.**
///
/// Physically it doesn't end: `ρ(h) = ρ₀·e^(−h/H)` is positive at every altitude, so no surface is "the
/// top of the air". The Kármán line (100 km) is an aeronautical convention about the altitude at which a
/// wing would need orbital speed to fly — a statement about aircraft, not about gas — and writing it in
/// here as the edge of the atmosphere would be exactly the declared number Law V forbids.
///
/// What IS real is the altitude at which *including* the air stops being able to change the answer. Over
/// a step `dt` the air changes the body's speed by `|Δv| = a_drag·dt`; once that falls below the smallest
/// difference f64 can hold against the speed itself (`ε·|v|`), the air is not being neglected — it is
/// being added and having no effect. That is the honest bound, and this reports whether the air still
/// reaches the body by it.
///
/// The boundary it defines belongs to the BODY, not to the planet: a light, wide body still feels air a
/// dense compact one has left behind, and a fast body feels it further out than a slow one (drag goes as
/// v²). It also tightens by itself as the arithmetic improves — it describes what the computation can
/// represent, not where the sky stops.
pub fn air_reaches(rho: f64, vel: glam::DVec3, mass_kg: f64, radius_m: f64, dt: f64) -> bool {
    let speed = vel.length();
    if rho <= 0.0 || mass_kg <= 0.0 || radius_m <= 0.0 || speed <= 0.0 || dt <= 0.0 {
        return false;
    }
    let frontal = std::f64::consts::PI * radius_m * radius_m;
    let dv = drag_accel(rho, vel, frontal, mass_kg, SPHERE_DRAG_CD).length() * dt;
    dv > f64::EPSILON * speed
}

/// **Air density at altitude `h` above the surface** (kg/m³) — the barometric profile, derived.
///
/// `ρ(h) = ρ₀·exp(−h/H)` with `ρ₀ = P₀/(R_s·T)` and `H = R_s·T/g`. Nothing here is chosen:
/// - `P₀` is [`crate::planet::Planet::surface_pressure`], which is **emergent** — the declared
///   atmosphere's own mass times gravity, over the planet's area (`5.15e18 kg` for Earth, its real value).
/// - `R_s` comes from air's real molar mass in the material DB.
/// - `H` is [`scale_height`], the same function the settling-column test proves the dynamics converge to.
///
/// This is the DECLARED (docs/46 §1) static form of the atmosphere: isothermal, hydrostatic, no weather.
/// **The resolved computation it stands in for is [`AirField`]** — SPH gas parcels whose density is
/// whatever the flow makes it — so it is deletable exactly when parcels are affordable in the region
/// concerned. Until then it gives a body something real to move through, rather than the vacuum that
/// followed deleting the old `DRAG` fudge.
pub fn air_density_at(surface_pressure: f64, air: &Material, temp_k: f64, g: f64, h: f64) -> f64 {
    AirShell::new(surface_pressure, air, temp_k, g).density_at(h)
}

/// **Drag acceleration** (m/s², opposing motion) on a body of cross-section `area` and mass `mass`
/// moving at `vel` through air of density `rho`: the quadratic law `F = ½·ρ·v²·C_d·A`.
///
/// Quadratic (not linear) because everything this engine throws around — ejecta, meteors, a kart — is
/// firmly in the high-Reynolds regime where inertial drag dominates viscous. Returns an ACCELERATION so
/// it composes with the per-mass force convention the granular law already uses.
///
/// `c_d` is a DECLARED shape factor with its IOU: the resolved computation is the pressure field of
/// [`AirField`] parcels flowing around the body, which yields the same force without anyone naming a
/// coefficient. Deletable when the flow around that body is resolved.
pub fn drag_accel(rho: f64, vel: glam::DVec3, area: f64, mass: f64, c_d: f64) -> glam::DVec3 {
    let speed = vel.length();
    if speed <= 1.0e-12 || rho <= 0.0 || mass <= 0.0 {
        return glam::DVec3::ZERO;
    }
    let f = 0.5 * rho * speed * speed * c_d * area; // N, along −v̂
    -(f / mass) * (vel / speed)
}

/// **Stagnation-point convective heat flux** (W/m²) onto a blunt body of nose radius `nose_r` (m) moving
/// at `speed` (m/s) through air of density `rho` (kg/m³) — the Sutton–Graves correlation:
///
/// ```text
/// q = k · √(ρ / R_n) · v³ ,   k = 1.7415e-4  (Earth air)
/// ```
///
/// This is the CONTINUUM aeroheating a meteor feels low in the atmosphere, and it is the RIGHT law for the
/// regime a ground-scene meteor lives in — near sea level, dense air. The classical meteor `Λ·½ρv³`
/// free-molecular law is a DIFFERENT regime (high altitude, hypervelocity, thin air); using it near the
/// surface overpredicts the heat flux by ~10³ and would vaporise a mere supersonic pebble. Both are the
/// same physics at different Knudsen number; `AirField` resolving the flow is the counterpart of both.
///
/// **Source + unit check.** Sutton, K. & Graves, R. A. (1971), *A General Stagnation-Point Convective-
/// Heating Equation for Arbitrary Gas Mixtures*, NASA TR R-376. `k = 1.7415e-4` gives `q` in **W/m²** with
/// SI inputs — verified against the Stardust sample-return capsule (the fastest crewed-era Earth entry,
/// v≈12.6 km/s, R_n=0.229 m): at the ρ≈2e-4 kg/m³ of peak heating this returns ≈1.0e7 W/m² ≈ 1030 W/cm²,
/// matching the ~1200 W/cm² Stardust peak. As W/cm² the same constant would be 10⁴× high — hence the check.
///
/// Zero below the local speed of sound: with no bow shock there is no stagnation heating (a subsonic body
/// is cooled convectively, not heated — a smaller, opposite effect not modelled here).
pub fn stagnation_heat_flux(rho: f64, speed: f64, nose_r: f64) -> f64 {
    const K_SUTTON_GRAVES: f64 = 1.7415e-4; // Earth air, W/m² with SI inputs (NASA TR R-376)
    const SOUND_SPEED_MS: f64 = 340.0; // sea-level; below Mach 1 there is no shock to heat the body
    if rho <= 0.0 || speed <= SOUND_SPEED_MS || nose_r <= 0.0 {
        return 0.0;
    }
    K_SUTTON_GRAVES * (rho / nose_r).sqrt() * speed * speed * speed
}

/// How one step of atmospheric flight acts on a body (docs/58 — ONE law for EVERY body, not a meteor
/// special-case): the drag acceleration to apply, its new surface temperature, and any mass it ablated.
///
/// This is the generic "body ⊕ atmosphere" interaction. The same operator serves a meteor, a lump of
/// ejecta, orbital debris re-entering, a spacecraft — anything moving through any declared atmosphere. A
/// scene never re-implements entry physics; it hands the engine a body and the local air and applies the
/// result. The engine deciding WHICH bodies are in atmosphere (a detected interaction, like body↔body
/// collision) is the docs/58 generalization this operator is built to be driven by.
#[derive(Clone, Copy, Debug)]
pub struct AtmosphericStep {
    /// Drag acceleration (m/s², opposing motion) — the caller integrates it like any other acceleration.
    pub drag_accel: glam::DVec3,
    /// Updated surface temperature (K) after this step's aeroheating and radiation.
    pub temp_k: f64,
    /// Mass vaporised this step (kg, ≥ 0) — the caller removes it and shrinks the body.
    pub ablated_mass: f64,
    /// Updated radius (m) after ablation (r ∝ m^⅓ at the material's own density).
    pub radius_m: f64,
    /// **How deep the heat has soaked** (m) — the thickness of the body's heated skin after this step.
    /// The caller carries it forward; a fresh body starts at 0. See [`heated_mass`].
    pub skin_m: f64,
}

/// **How much of a body is actually hot** (kg) — the mass within `skin_m` of the surface of a sphere of
/// radius `r` and density `rho`: `ρ·(4/3)π[r³ − (r−δ)³]`, saturating at the whole body once the heat has
/// soaked all the way through.
///
/// This is the correction that lets a metre-class body glow (docs/46 row 21). Heating a body's WHOLE mass
/// at its bulk heat capacity makes thermal response scale with volume, so a half-metre iron body flew a
/// perfectly correct entry at 20 km/s and barely warmed — while real iron meteorites arrive with a molten
/// fusion crust over a core still cold enough to frost. Ablation is a SURFACE process; the mass that
/// participates is the mass the heat has reached.
pub fn heated_mass(mass_kg: f64, radius_m: f64, skin_m: f64) -> f64 {
    if radius_m <= 0.0 || mass_kg <= 0.0 {
        return 0.0;
    }
    if skin_m >= radius_m {
        return mass_kg; // soaked through — the bulk limit, and the old behaviour exactly
    }
    let inner = radius_m - skin_m.max(0.0);
    mass_kg * (1.0 - (inner / radius_m).powi(3))
}

/// **How far a heat front has travelled into a material** after `dt` more of heating, given it had already
/// reached `skin_m`: from `δ = √(α·t)` it follows that `δ² = α·t`, so one step is simply
/// `δ' = √(δ² + α·dt)` — exact, and cheaper than integrating `dδ/dt = α/2δ`.
///
/// `α` is [`crate::materials::Material::thermal_diffusivity`], a ratio of three measured quantities. A
/// material whose heat transport is uncharacterised returns `None`, and a caller must then not claim to
/// know how the heat spread — the honest fallback is to treat the body as soaked through, which
/// under-predicts ablation rather than inventing it.
pub fn soak_depth(skin_m: f64, alpha: f64, dt: f64) -> f64 {
    if alpha <= 0.0 || dt <= 0.0 {
        return skin_m.max(0.0);
    }
    (skin_m.max(0.0).powi(2) + alpha * dt).sqrt()
}

/// One step of a spherical body's flight through air of density `rho`, made of `mat`, at `temp_k`, moving
/// at `vel` (docs/48/58). Drag (`drag_accel`) + Sutton–Graves aeroheating (`stagnation_heat_flux`) +
/// ablation at the boiling point — every material quantity read from the catalogue, nothing meteor-specific.
///
/// Declared shape/surface factors, both flagged with the same resolved counterpart (`AirField` resolving
/// the flow around the body): `c_d = 0.47` (measured incompressible-sphere drag coefficient; refinement
/// c_d(Mach)) and `emissivity = 0.45` (measured molten-iron emissivity; belongs in the optical catalogue).
#[allow(clippy::too_many_arguments)]
pub fn atmospheric_step(
    rho: f64,
    vel: glam::DVec3,
    mass_kg: f64,
    radius_m: f64,
    temp_k: f64,
    skin_m: f64,
    ambient_temp_k: f64,
    mat: &Material,
    dt: f64,
) -> AtmosphericStep {
    let mut out = AtmosphericStep {
        drag_accel: glam::DVec3::ZERO,
        temp_k,
        ablated_mass: 0.0,
        radius_m,
        skin_m,
    };
    if rho <= 0.0 || mass_kg <= 0.0 || radius_m <= 0.0 {
        return out; // no air, or nothing left of the body — vacuum flight
    }
    let r = radius_m;
    let frontal = std::f64::consts::PI * r * r;
    let speed = vel.length();

    // DRAG (momentum). The KE it removes heats the air and the body; a braked body delivers less on impact.
    out.drag_accel = drag_accel(rho, vel, frontal, mass_kg, SPHERE_DRAG_CD);

    // AEROHEATING + ABLATION (energy). The catalogue must characterise the body's thermal response; a
    // material with no boiling point or latent heat cannot ablate honestly, so it only heats.
    let (Some(c), Some(t_boil), Some(l_v)) = (
        mat.specific_heat(),
        mat.boil_point(),
        mat.latent_vaporization(),
    ) else {
        return out;
    };
    // Windward-hemisphere heat: the stagnation flux averaged (~½) over the front (~2πr²) ⇒ q·πr².
    let p_in = stagnation_heat_flux(rho, speed, r) * frontal;
    // Radiated from the whole surface (4πr²) as σεT⁴ — this is the glow we see.
    let p_rad = MOLTEN_EMISSIVITY
        * crate::blackbody::SIGMA
        * (temp_k.powi(4) - ambient_temp_k.powi(4)).max(0.0)
        * (4.0 * std::f64::consts::PI * r * r);
    let net = p_in - p_rad; // W into the body; negative ⇒ it cools by radiating

    // **HOW MUCH OF THE BODY IS HOT.** The heat front advances by conduction at the material's own
    // diffusivity; only the mass it has reached takes part in the temperature change. Without this,
    // thermal response scales with VOLUME and nothing metre-sized can ever glow (docs/46 row 21) —
    // measured: a 0.5 m iron body at 20 km/s barely warmed over an entire descent.
    // A material with no characterised heat transport falls back to the whole body, which under-predicts
    // ablation rather than inventing it (flagged).
    let alpha = mat.thermal_diffusivity();
    let skin = match alpha {
        Some(a) => soak_depth(skin_m, a, dt).min(r),
        None => r,
    };
    out.skin_m = skin;
    let hot_mass = heated_mass(mass_kg, r, skin).max(1.0e-30);

    if temp_k >= t_boil && net > 0.0 {
        // At the boiling point with heat to spare: the excess vaporises mass at the latent cost `l_v`; the
        // body shrinks (r ∝ m^⅓). A body only ablates once its aeroheating beats its own radiative loss —
        // which for iron (boil 3134 K) needs true meteor speeds, not merely supersonic ones.
        out.temp_k = t_boil;
        out.ablated_mass = (net / l_v * dt).min(mass_kg);
        let new_mass = mass_kg - out.ablated_mass;
        let rho_mat = mat.density.max(1.0) as f64;
        out.radius_m = (3.0 * new_mass / (4.0 * std::f64::consts::PI * rho_mat)).cbrt();
        // The ablated shell was the HOTTEST material, and it has gone: the surface has receded into the
        // body, so the heated layer left behind is thinner by exactly what was removed. It then regrows by
        // conduction, and the balance between the two is not declared anywhere — it settles at δ = α/2v
        // for a surface receding at v, which is the classical thermal boundary layer of an ablating body.
        out.skin_m = (skin - (r - out.radius_m)).max(0.0);
    } else {
        // Otherwise the net heat changes the temperature (up if heating dominates, down if radiation does).
        // Clamped to [ambient, boiling]: it cannot radiate below the air it sits in, nor exceed boiling
        // without ablating (the branch above).
        out.temp_k = (temp_k + net / (hot_mass * c) * dt).clamp(ambient_temp_k, t_boil);
    }
    out
}

/// **A parcel of the body that is no longer the body** — mass [`atmospheric_step`] ablated away, at the
/// place and moment it was shed, still on the books.
///
/// This is the ENTRY TRAIL (docs/59 feature 2), and it exists because the alternative is a conservation
/// hole: `atmospheric_step` reports `ablated_mass`, callers subtract it from the body, and the vapour
/// simply ceased to exist. What you see behind a meteor is precisely that vapour — the body, glowing at
/// the temperature its own ablation put it at — so the thing that closes the books and the thing that
/// makes the picture are the same thing.
#[derive(Clone, Copy, Debug)]
pub struct VaporParcel {
    pub mass_kg: f64,
    /// What it is vapour OF (index into the material catalogue). Vapour keeps the identity of the body it
    /// left: iron vapour cools like iron, glows like iron, and is drawn in iron's own albedo.
    pub material: usize,
    pub pos: glam::DVec3,
    /// Shed at the body's velocity — it leaves with the momentum it had.
    pub vel: glam::DVec3,
    /// Its temperature (K). The glow is [`crate::blackbody::blackbody_srgb`] of THIS, not a colour chosen
    /// for a trail: physics drives the render.
    pub temp_k: f64,
    /// The temperature it was SHED at — the heat it started with, kept so that "has it finished cooling?"
    /// can be asked about this parcel's own history instead of against an absolute number.
    pub shed_temp_k: f64,
}

impl VaporParcel {
    /// The radius (m) this parcel occupies, EMERGENT: vapour shed into air expands until it is no denser
    /// than what it expands into, so `r = (3m/4πρ_air)^⅓` at the local air density. Nothing declares a
    /// puff size — a parcel shed high, into thin air, is correspondingly larger.
    ///
    /// *Flagged:* it reaches that density instantly here. The resolved computation is [`AirField`] — SPH
    /// gas parcels whose density is whatever the flow makes it — which is also what turns this trail from
    /// a column of independent parcels into one that mixes, shears and disperses.
    pub fn radius_in(&self, rho_air: f64) -> f64 {
        if rho_air <= 0.0 || self.mass_kg <= 0.0 {
            return 0.0;
        }
        (3.0 * self.mass_kg / (4.0 * std::f64::consts::PI * rho_air)).cbrt()
    }

    /// Has it finished being a trail? A parcel that has cooled to the air around it IS the air around it:
    /// its mass has joined the atmosphere, which is where ablated mass really goes. Retiring it is
    /// bookkeeping reaching its end, not mass being dropped.
    ///
    /// **This asked `temp_k <= ambient`, which is a test that can never pass.** Radiative cooling is
    /// asymptotic — `p_rad ∝ T⁴ − T_amb⁴` vanishes as the parcel approaches ambient — so a parcel creeps
    /// toward the air temperature and never arrives. MEASURED before the fix: 3,734 parcels of one entry
    /// sat at 288.00 K indefinitely, holding 9.1 kg permanently "aloft" and costing a draw every frame.
    /// Found by asking whether the trail dissipates (Robin, 2026-07-24) — the fade was real and the
    /// retirement was not.
    ///
    /// So the question is asked about the parcel's OWN history: it is air once it has radiated all but a
    /// hundredth of the heat it was shed with. That is an e-folding statement, not an absolute
    /// temperature, so it scales with whatever the parcel started at and with any world's ambient.
    ///
    /// Robin (2026-07-24) on why this is not a fudge: *"calculus teaches us that some limits may never be
    /// reached, but we can get 'close enough' as to dismiss the difference without fear of fudge."* The
    /// distinction that matters is that the tolerance is RELATIVE to the quantity being resolved, so it
    /// converges with it and states its own error — unlike a dial, which states nothing.
    /// *Flagged:* the honest criterion is MIXING — a real trail disperses turbulently, it does not sit
    /// still radiating — and the resolved computation for that is [`AirField`].
    pub fn merged_into_air(&self, ambient_temp_k: f64) -> bool {
        const RADIATED_AWAY: f64 = 0.01; // the last hundredth of the excess is no longer a trail
        let excess = self.temp_k - ambient_temp_k;
        if excess <= 0.0 {
            return true;
        }
        excess <= RADIATED_AWAY * (self.shed_temp_k - ambient_temp_k).max(0.0)
    }
}

/// One step of a shed parcel's life: it drifts, it is slowed by the air it is moving through, and it
/// radiates its heat away.
///
/// The cooling is the same Stefan–Boltzmann loss [`atmospheric_step`] already applies to the body, over
/// the area the parcel actually presents at the local air density ([`VaporParcel::radius_in`]) — one
/// radiation law, not a second one written for trails. Drag likewise reuses [`drag_accel`]. A parcel that
/// reaches ambient has become air; the caller retires it then.
pub fn vapor_step(
    p: VaporParcel,
    rho_air: f64,
    ambient_temp_k: f64,
    mat: &Material,
    dt: f64,
) -> VaporParcel {
    let mut out = p;
    out.pos += p.vel * dt;
    if p.mass_kg <= 0.0 || dt <= 0.0 {
        return out;
    }
    let r = p.radius_in(rho_air);
    if r > 0.0 {
        // Shed vapour is moving at the body's speed through air that is not, and a parcel as thin as the
        // air it is in has an enormous area per kilogram: it is stopped in milliseconds. Quadratic drag
        // that strong is STIFF, and an explicit `v += a·dt` overshoots — a step long enough to stop the
        // parcel reverses it and then accelerates it away, which is how this first read as a parcel
        // speeding UP. So the step is taken exactly: dv/dt = −k·|v|·v integrates in closed form to
        // |v|(t) = |v₀|/(1 + k·|v₀|·t), which decays monotonically for any dt and cannot overshoot.
        // (Same law as `drag_accel`, k = ½ρ·C_d·A/m — solved rather than sampled.)
        let speed = p.vel.length();
        if speed > 0.0 {
            let k = 0.5 * rho_air * SPHERE_DRAG_CD * std::f64::consts::PI * r * r / p.mass_kg;
            out.vel = p.vel / (1.0 + k * speed * dt);
        }
    }
    let Some(c) = mat.specific_heat() else {
        return out; // uncharacterised: we do not claim to know how fast it cools
    };
    let area = 4.0 * std::f64::consts::PI * r * r;
    let p_rad = MOLTEN_EMISSIVITY
        * crate::blackbody::SIGMA
        * (p.temp_k.powi(4) - ambient_temp_k.powi(4)).max(0.0)
        * area;
    out.temp_k = (p.temp_k - p_rad / (p.mass_kg * c) * dt).max(ambient_temp_k);
    out
}

/// **Everything a body has shed, still on the books** — the entry trail, at whichever resolution the view
/// needs (Robin, 2026-07-24: *"rendering/tracking it should be decided based on the scale it is being
/// viewed at"*).
///
/// Two representations of the same mass, which is the docs/44 ladder rather than two models:
///
/// - **resolved** — [`VaporParcel`]s, each drifting, slowing and cooling. What a camera near the trail
///   needs, because from there you can see individual puffs shear and fade.
/// - **booked** — `merged_kg`, a running total that has become part of the atmosphere. From orbit that
///   is all a trail IS: mass returned to the air it was ablated into. A parcel is retired here once it
///   reaches ambient temperature, and a caller watching from far enough away may book mass directly
///   without ever resolving a parcel.
///
/// Either way [`Trail::mass`] is the same number, which is the point: the representation changes with the
/// camera, the mass does not (Law IV).
///
/// **Flagged, and named rather than lost:** mass booked into the air stays there. In reality it condenses
/// to micrometeoritic dust and settles out — some 40,000 tonnes a year on Earth — so `merged_kg` is a
/// staging post, not a final resting place. The resolved computation it defers to is [`AirField`] gas
/// parcels that condense and fall under the same gravity as everything else.
#[derive(Clone, Debug, Default)]
pub struct Trail {
    parcels: Vec<VaporParcel>,
    merged_kg: f64,
    /// How many parcels are worth RESOLVING at once. Past this, shed mass is booked instead — the same
    /// mass, the coarse representation (Robin: tracking follows the scale it is viewed at). Zero means
    /// "no limit". The number is not a guess about physics: a caller sets it from its instance budget,
    /// which is a real hardware bound, and the choice is which representation to spend it on.
    resolve_budget: usize,
}

impl Trail {
    /// Resolve at most `n` parcels at a time; shed mass beyond that is booked into the air. Zero = no
    /// limit. Nothing about EXISTENCE changes with this (Law IV) — only how finely the same mass is held.
    pub fn set_resolve_budget(&mut self, n: usize) {
        self.resolve_budget = n;
    }

    /// The body shed this much mass, here, at this velocity and temperature.
    pub fn shed(
        &mut self,
        mass_kg: f64,
        material: usize,
        pos: glam::DVec3,
        vel: glam::DVec3,
        temp_k: f64,
    ) {
        if mass_kg <= 0.0 {
            return;
        }
        if self.resolve_budget > 0 && self.parcels.len() >= self.resolve_budget {
            self.book(mass_kg); // over budget: same mass, coarser representation
            return;
        }
        self.parcels.push(VaporParcel {
            mass_kg,
            material,
            pos,
            vel,
            temp_k,
            shed_temp_k: temp_k,
        });
    }

    /// Shed mass without resolving it — the coarse representation, for a trail nothing is close enough to
    /// see. The mass is accounted for identically; only the detail differs.
    pub fn book(&mut self, mass_kg: f64) {
        if mass_kg > 0.0 {
            self.merged_kg += mass_kg;
        }
    }

    /// Total mass the trail holds — resolved parcels plus what has become air. The invariant a caller
    /// checks: body mass + `trail.mass()` is what entered.
    pub fn mass(&self) -> f64 {
        self.parcels.iter().map(|p| p.mass_kg).sum::<f64>() + self.merged_kg
    }

    /// Mass that has cooled into the atmosphere.
    pub fn merged_kg(&self) -> f64 {
        self.merged_kg
    }

    /// The hottest parcel still resolved (K), and the mass-weighted mean temperature — how the trail is
    /// FADING, in the only terms it can honestly be described in. Zero when nothing is left aloft.
    pub fn temperature_range_k(&self) -> (f64, f64) {
        let mass: f64 = self.parcels.iter().map(|p| p.mass_kg).sum();
        if mass <= 0.0 {
            return (0.0, 0.0);
        }
        let hottest = self.parcels.iter().map(|p| p.temp_k).fold(0.0, f64::max);
        let mean = self
            .parcels
            .iter()
            .map(|p| p.temp_k * p.mass_kg)
            .sum::<f64>()
            / mass;
        (hottest, mean)
    }

    /// The parcels still hot enough to be worth resolving — what a renderer draws, glowing at their own
    /// temperatures via [`crate::blackbody::blackbody_srgb`].
    pub fn parcels(&self) -> &[VaporParcel] {
        &self.parcels
    }

    /// Age every resolved parcel one step: it drifts, is slowed, and radiates. `ambient` reports the air
    /// density and temperature at a parcel's position, which only the scene knows the geometry for.
    /// Parcels that reach ambient are retired into `merged_kg` — mass moves between representations, never
    /// out of the books.
    /// `mats` is the catalogue, not one material: several bodies of different materials can be ablating
    /// at once, and each parcel cools as what it actually is. (It took a `&Material` first, which quietly
    /// cooled everything as whatever the first body in flight happened to be made of.)
    pub fn step(
        &mut self,
        mats: &[Material],
        dt: f64,
        ambient: impl Fn(glam::DVec3) -> (f64, f64),
    ) {
        let mut merged = 0.0;
        self.parcels.retain_mut(|p| {
            let (rho, t_amb) = ambient(p.pos);
            let Some(mat) = mats.get(p.material) else {
                return true;
            };
            *p = vapor_step(*p, rho, t_amb, mat, dt);
            if p.merged_into_air(t_amb) {
                merged += p.mass_kg;
                false
            } else {
                true
            }
        });
        self.merged_kg += merged;
    }
}

/// The 3D generalization of the column (docs/26): an SPH air FIELD. Density is estimated by a cubic
/// spline kernel over neighbours, pressure is the ideal gas P = ρ·R_s·T (isothermal v0, flagged), and
/// the symmetric pressure force  a_i = −Σ_j m_j (P_i/ρ_i² + P_j/ρ_j²) ∇W  conserves momentum exactly by
/// construction. Nothing but the declared gas constants enters. O(n²) neighbour search — the neighbour
/// grid is the same scaling refinement flagged for aggregates.
pub struct AirField {
    pub pos: Vec<glam::DVec3>,
    pub vel: Vec<glam::DVec3>,
    pub mass: f64, // per parcel (equal-mass model)
    pub h: f64,    // kernel smoothing length (m)
    pub rs_t: f64, // R_s·T (isothermal v0)
    pub rho: Vec<f64>,
    /// GHOST-PARTICLE boundaries: parcels within `h` of a face see their own and their neighbours'
    /// mirror images across it, completing the kernel support — without this, boundary densities are
    /// ~2× deficient and the basal pressure halves (observed: a settling column collapsed onto the
    /// floor). The ghosts' reaction is carried by the boundary — the floor honestly supports the
    /// column's weight. SIDE WALLS carry the mirror symmetry of a representative column inside a WIDE
    /// atmosphere (its lateral neighbours are identical columns) — without them the gas simply flows
    /// sideways into vacuum and no column can ever hold hydrostatic pressure (observed). No lid: the
    /// top is a real free surface to space. (Corner double-mirror ghosts are neglected — a small,
    /// flagged kernel deficiency exactly at edges.)
    pub floor_y: Option<f64>,
    pub walls_x: Option<(f64, f64)>,
    pub walls_z: Option<(f64, f64)>,
}

impl AirField {
    pub fn new(pos: Vec<glam::DVec3>, mass: f64, h: f64, rs_t: f64) -> Self {
        let n = pos.len();
        AirField {
            pos,
            vel: vec![glam::DVec3::ZERO; n],
            mass,
            h,
            rs_t,
            rho: vec![0.0; n],
            floor_y: None,
            walls_x: None,
            walls_z: None,
        }
    }

    /// Add a ghost-particle floor at height `y`.
    pub fn with_floor(mut self, y: f64) -> Self {
        self.floor_y = Some(y);
        self
    }

    /// Add ghost-particle side walls (the mirror symmetry of a representative column in a wide field).
    pub fn with_walls(mut self, x: (f64, f64), z: (f64, f64)) -> Self {
        self.walls_x = Some(x);
        self.walls_z = Some(z);
        self
    }

    /// All mirror ghosts of parcel `j` within kernel range of any active boundary face —
    /// COMPOSITIONAL: mirrors across every subset of nearby faces (single faces, edges via double
    /// mirrors, corners via triples), so kernels are complete even where floor meets wall. In a small
    /// box most parcels are boundary-adjacent; neglecting the corner mirrors left quarter-kernels
    /// missing along every edge (observed as a systematically low basal pressure).
    fn ghosts_of(&self, j: usize) -> Vec<glam::DVec3> {
        let p = self.pos[j];
        let mut pts = vec![p]; // seed: the real parcel; mirrors of ALL accumulated points per face
        let mut reflect = |pts: &mut Vec<glam::DVec3>, axis: usize, at: f64, near: bool| {
            if !near {
                return;
            }
            let cur = pts.len();
            for k in 0..cur {
                let mut g = pts[k];
                match axis {
                    0 => g.x = 2.0 * at - g.x,
                    1 => g.y = 2.0 * at - g.y,
                    _ => g.z = 2.0 * at - g.z,
                }
                pts.push(g);
            }
        };
        if let Some(fy) = self.floor_y {
            reflect(&mut pts, 1, fy, p.y - fy < self.h);
        }
        if let Some((x0, x1)) = self.walls_x {
            reflect(&mut pts, 0, x0, p.x - x0 < self.h);
            reflect(&mut pts, 0, x1, x1 - p.x < self.h);
        }
        if let Some((z0, z1)) = self.walls_z {
            reflect(&mut pts, 2, z0, p.z - z0 < self.h);
            reflect(&mut pts, 2, z1, z1 - p.z < self.h);
        }
        pts.remove(0); // the seed is the real parcel, not a ghost
        pts
    }

    /// Cubic spline kernel W(r, h), 3D-normalized (σ = 8/(π h³)), support 0..h.
    fn w(&self, r: f64) -> f64 {
        sph_w(r, self.h)
    }

    /// dW/dr — the kernel gradient magnitude.
    fn dw(&self, r: f64) -> f64 {
        sph_dw(r, self.h)
    }

    /// Kernel density estimate at every parcel (includes self-contribution and floor ghosts).
    pub fn compute_density(&mut self) {
        let n = self.pos.len();
        // Mirror ghosts built ONCE per pass (not per pair — that was an accidental O(n²) allocation).
        let ghosts: Vec<glam::DVec3> = (0..n).flat_map(|j| self.ghosts_of(j)).collect();
        for i in 0..n {
            let mut rho = self.mass * self.w(0.0);
            for j in 0..n {
                if j != i {
                    let r = (self.pos[i] - self.pos[j]).length();
                    if r < self.h {
                        rho += self.mass * self.w(r);
                    }
                }
            }
            for ghost in &ghosts {
                let r = (self.pos[i] - *ghost).length();
                if r < self.h && r > 1.0e-9 {
                    rho += self.mass * self.w(r);
                }
            }
            self.rho[i] = rho;
        }
    }

    /// Symmetric SPH pressure accelerations (momentum-conserving by construction) + any external accel.
    pub fn accelerations(&self, external: glam::DVec3) -> Vec<glam::DVec3> {
        let n = self.pos.len();
        let mut acc = vec![external; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let d = self.pos[i] - self.pos[j];
                let r = d.length();
                if r >= self.h || r < 1.0e-9 {
                    continue;
                }
                let (pi, pj) = (self.rho[i] * self.rs_t, self.rho[j] * self.rs_t);
                let term = pi / (self.rho[i] * self.rho[i]) + pj / (self.rho[j] * self.rho[j]);
                let a = -(d / r) * (self.mass * term * self.dw(r)); // dw < 0 ⇒ repulsive
                acc[i] += a;
                acc[j] -= a; // equal-mass parcels: equal/opposite accelerations = forces
            }
        }
        // Ghost forces: every real parcel j near a boundary (including i itself) has mirrors whose
        // pressure pushes i away from the face. The reaction goes to the BOUNDARY (not to j) — the
        // floor genuinely carries the column's weight, the walls carry the neighbouring columns' push;
        // air momentum alone is not conserved against a wall, and must not be. Ghost list built once.
        let ghost_list: Vec<(glam::DVec3, usize)> = (0..n)
            .flat_map(|j| self.ghosts_of(j).into_iter().map(move |g| (g, j)))
            .collect();
        for i in 0..n {
            for (ghost, j) in &ghost_list {
                let d = self.pos[i] - *ghost;
                let r = d.length();
                if r >= self.h || r < 1.0e-9 {
                    continue;
                }
                let (pi, pj) = (self.rho[i] * self.rs_t, self.rho[*j] * self.rs_t);
                let term = pi / (self.rho[i] * self.rho[i]) + pj / (self.rho[*j] * self.rho[*j]);
                acc[i] += -(d / r) * (self.mass * term * self.dw(r));
            }
        }
        acc
    }

    /// Damped relaxation step (settling to hydrostatic equilibrium; damping is numerical, the
    /// EQUILIBRIUM is the physics).
    pub fn relax_step(&mut self, external: glam::DVec3, dt: f64, damp: f64) {
        self.compute_density();
        let acc = self.accelerations(external);
        for i in 0..self.pos.len() {
            self.vel[i] = (self.vel[i] + acc[i] * dt) * damp;
            let dv = self.vel[i] * dt;
            self.pos[i] += dv;
            // Hard clamps as a numerical backstop; the ghost pressure is the real boundary force.
            if let Some(fy) = self.floor_y {
                if self.pos[i].y < fy {
                    self.pos[i].y = fy;
                    self.vel[i].y = self.vel[i].y.max(0.0);
                }
            }
            if let Some((x0, x1)) = self.walls_x {
                self.pos[i].x = self.pos[i].x.clamp(x0, x1);
            }
            if let Some((z0, z1)) = self.walls_z {
                self.pos[i].z = self.pos[i].z.clamp(z0, z1);
            }
        }
    }
}

/// Sea-level Rayleigh optical depths for the R/G/B bands (650/550/450 nm), scaled by the EMERGENT
/// surface-pressure ratio (an airless world scatters nothing — the Moon stays colorless for free).
/// τ(λ) = 0.0088·(P/P₀)·λ^−4.05 (λ in µm) — the standard empirical fit for Earth air (Hansen &
/// Travis 1974); the λ⁻⁴ is molecular (Rayleigh) physics, the coefficient is our declared N₂/O₂
/// column doing the scattering. THE BLUE MARBLE IS DERIVED, NEVER PAINTED: remove the atmosphere and
/// the blue leaves with it.
/// The exposure the Rayleigh veil is displayed at — the sun's radiance in the same arbitrary units as
/// albedo, before the Reinhard tonemap. It is a DISPLAY constant (a camera exposure), not a physical
/// dial: it scales what the eye sees, never what the air does. It lives here because every view of the
/// same air must use the SAME exposure — the ground sky and the globe seen from orbit are one
/// atmosphere, and two gains would make one planet look like two.
pub const SUN_GAIN: f32 = 22.0;

pub fn rayleigh_tau(pressure_ratio: f64) -> [f64; 3] {
    let t = |um: f64| 0.0088 * pressure_ratio * um.powf(-4.05);
    [t(0.650), t(0.550), t(0.450)]
}

/// **Why the day/night line is soft.** When the Sun sets at a point on the ground, the AIR ABOVE that
/// point is still in sunlight — and stays lit until the shadow of the planet climbs past the top of the
/// dense atmosphere. That is twilight, and the angle it spans is not a taste parameter: for a shell of
/// scale height `H` on a planet of radius `R`, the terminator is geometrically blurred by
///
///   w ≈ sqrt(2·H / R)
///
/// which for Earth's ~8.4 km scale height on a 6371 km radius is 0.051 rad ≈ 2.9°. A body with no air
/// gets w = 0 and a knife-edge terminator, which is exactly what the airless Moon shows.
///
/// FLAGGED: this is the geometric height of the scattering shell, so it reproduces the bright part of
/// twilight. The long red tail (civil/nautical twilight out to ~12°) needs multiple scattering, which
/// this single-scatter model does not carry.
pub fn twilight_half_angle(scale_height_m: f64, radius_m: f64) -> f64 {
    if scale_height_m <= 0.0 || radius_m <= 0.0 {
        return 0.0; // airless: the terminator is a hard edge, as on the Moon
    }
    (2.0 * scale_height_m / radius_m).sqrt()
}

/// Single-scatter Rayleigh VEIL toward a viewer: the added radiance (pre-tonemap, in the same units
/// as albedo·SUN_GAIN) for a surface patch with view cosine `mu_v`, sun cosine `mu_s`, and
/// sun-to-view angle cosine `cos_theta`. In-scatter = phase·(1 − e^−τ/μᵥ)·(sunlight attenuated on the
/// way in). Flat-slab slant path (Chapman function is the refinement, flagged); single scatter only
/// (multiple scattering + ozone are the refinement). Night side → 0, honestly.
pub fn rayleigh_veil(
    mu_v: f64,
    mu_s: f64,
    cos_theta: f64,
    tau: [f64; 3],
    sun_gain: f64,
    twilight: f64,
) -> [f32; 3] {
    // TWILIGHT: the ground here may have turned away from the Sun, but the AIR ABOVE IT has not. The
    // shell stays lit for `twilight` radians past the geometric terminator (see `twilight_half_angle`),
    // so the day/night line is a gradient the width of the atmosphere itself rather than a knife edge.
    // Day side (mu_s > 0) is untouched: `lit` clamps to 1 there. An airless body passes twilight = 0 and
    // gets the hard terminator it should have.
    let lit = if twilight > 0.0 {
        ((mu_s + twilight) / twilight).clamp(0.0, 1.0)
    } else {
        (mu_s > 0.0) as u8 as f64
    };
    if lit <= 0.0 {
        return [0.0; 3];
    }
    // FIRST-ORDER slab scattering (Chandrasekhar): the reflected single-scatter radiance of an
    // optically thin layer is L = F·P(Θ)/(4(μᵥ+μₛ))·μₛ·(1 − e^{−τ(1/μᵥ+1/μₛ)}), with the Rayleigh
    // phase P(Θ) = ¾(1+cos²Θ). Textbook, no tunable weight — the earlier ad-hoc form under-lit the
    // veil ~3×. Grazing cosines capped in lieu of the true Chapman function (flagged); multiple
    // scattering and ozone remain the refinement.
    let mu_v = mu_v.max(0.08);
    let mu_s_c = mu_s.max(0.08);
    let phase = 0.75 * (1.0 + cos_theta * cos_theta);
    let geom = phase / (4.0 * (mu_v + mu_s_c)) * mu_s_c;
    let path = 1.0 / mu_v + 1.0 / mu_s_c;
    let mut out = [0.0f32; 3];
    for (i, t) in tau.iter().enumerate() {
        out[i] = ((sun_gain * lit) * geom * (1.0 - (-t * path).exp())) as f32;
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// THE SKY, AS A MEDIUM (docs/66)
//
// `rayleigh_veil` above is a CLOSED FORM, and it is the closed form of one specific geometry: a slab
// of air seen from OUTSIDE it, looking down. Along such a ray the sun path and the view path shorten
// together, which is what lets the integral collapse to `1 − e^{−τ(1/μᵥ+1/μₛ)}`. Stand on the ground
// and look UP and that pairing inverts — the view path grows with height while the sun path shrinks —
// so the closed form is not merely approximate there, it is the wrong integral. (The retired
// `shaders/sky.wgsl` used it anyway, with `μᵥ = ray.y`, which is also why it could only ever be a sky
// for a flat world.)
//
// So the law below is the INTEGRAL, marched, and `rayleigh_veil` is its analytic special case — kept,
// and used as the reference the march is pinned to (`the_march_reproduces_the_closed_form_from_above`).
// One law, and the geometry decides which face of it you see:
//
//   * from orbit  — the blue marble and a limb that glows beyond the silhouette;
//   * from the ground looking up — blue overhead, pale at the horizon, red at sunset;
//   * from the ground looking ACROSS — aerial perspective over exactly the air in between;
//   * after sunset — the low air is in the planet's shadow while the air above it is still lit, so
//     TWILIGHT falls out of the shadow test rather than being a declared half-angle ramp.
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// **The air along a ray, as the one scattering integral wants it.** Every field is emergent: `tau`
/// from the declared air's own mass via [`rayleigh_tau`], `scale_height` from its molar mass and
/// temperature under the body's own gravity ([`AirShell`]), `radius` from the body. A world that
/// declares no atmosphere carries `tau = 0` and gets exactly nothing from every function below — the
/// airless case needs no branch anywhere.
///
/// **Units are free but must agree**: metres in the physics, display units in the renderer. The
/// integral is scale-invariant because the volume scattering coefficient is `τ/H`, which carries the
/// length unit back out again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirColumn {
    /// Vertical optical depth of the WHOLE column at the surface, per band.
    pub tau: [f64; 3],
    /// Barometric e-folding height.
    pub scale_height: f64,
    /// The surface the column stands on.
    pub radius: f64,
}

impl AirColumn {
    /// ★ **A body's own air, asked of the body.** Robin's rule for this whole feature (2026-08-04):
    /// *"Sky must be a component of Earth assembly."* So the sky is not something a scene owns and
    /// configures — it is what THIS body's declared mass of air does to light, and there is one place
    /// that turns the one into the other. A body that declares no atmosphere returns a column that
    /// scatters nothing, which is how the Moon gets a black sky without a single branch.
    ///
    /// Nothing here is chosen: the optical depth comes from the EMERGENT surface pressure (the weight
    /// of the declared air over the declared radius), and the scale height from the air's own molar
    /// mass at the body's own surface gravity.
    pub fn of_body(body: &crate::planet::LayeredBody, mats: &[Material], temp_k: f64) -> Self {
        let radius = body.radius();
        let air = mats.iter().find(|m| m.id == "air");
        AirColumn {
            tau: rayleigh_tau(body.surface_pressure() / 101_325.0),
            scale_height: air.map_or(0.0, |a| {
                scale_height(a, temp_k, body.gravity_at(radius.max(1.0)))
            }),
            radius,
        }
    }

    /// The same column in DISPLAY units — every length divided by the same scale. The integral is
    /// scale-free (`β = τ/H` carries the unit back out), so this is a change of units and nothing else;
    /// it exists because the renderer's positions are scaled so a planet radius is 1.
    pub fn scaled(&self, per_metre: f64) -> Self {
        AirColumn {
            tau: self.tau,
            scale_height: self.scale_height * per_metre,
            radius: self.radius * per_metre,
        }
    }

    /// **How far out THIS COMPONENT reaches** — the air's own contribution to the boundary of the
    /// assembly it belongs to, measured from the body's centre.
    ///
    /// ★★ It is deliberately NOT called "where the body ends", and that correction is Robin's
    /// (2026-08-05): *"'A body ends where its AIR ends' is not accurate, as bodies are assemblies. An
    /// assembly ends at the outermost boundary of the assembly."* The rule is general —
    /// **an assembly ends at the outermost boundary of its outermost component** — and air is merely
    /// the outermost component Earth happens to have today. Name the rule after the air and the engine
    /// learns a special case; the same question is asked by a tree's canopy, a ship's mast and a
    /// cannon's muzzle, and it must have one answer.
    ///
    /// The context it was corrected in: a rig locating the planet's edge by scanning pixel columns.
    /// *"This should be done in the engine as a boundary between the assembly (containing the
    /// atmosphere as a component of Earth) and space, which should be a collection of assemblies of
    /// type 'star' at coordinates."* The boundary is a fact the assembly holds; nothing downstream —
    /// a renderer, a rig, a visibility test — should be inferring it from a picture.
    ///
    /// An airless body's air reaches exactly its surface, with no branch, so a body whose outermost
    /// component is its rock reports its rock.
    pub fn outer_reach(&self) -> f64 {
        self.radius + if self.exists() { self.top() } else { 0.0 }
    }

    /// The angular radius this component subtends from `altitude` above the surface:
    /// `asin(outer_reach / (radius + altitude))`, or π/2 from inside it. For a body whose air is its
    /// outermost component this IS "how much of the frame is Earth" — and it is the assembly's own
    /// answer rather than the observer's guess at it.
    pub fn angular_reach_from(&self, altitude: f64) -> f64 {
        let r_eye = self.radius + altitude;
        let e = self.outer_reach();
        if r_eye <= e {
            return std::f64::consts::FRAC_PI_2;
        }
        (e / r_eye).asin()
    }

    /// Is there any air to scatter? (Blue band, because it is the last to vanish.)
    pub fn exists(&self) -> bool {
        self.tau[2] > 0.0 && self.scale_height > 0.0 && self.radius > 0.0
    }

    /// **Where the column stops being worth integrating** — the height above which the remaining air is
    /// [`COLUMN_TAIL`] of the whole. `∫ρ` above `h` is `ρ₀H·e^{−h/H}`, so that height is `−H·ln(tail)`:
    /// for Earth's 8.4 km scale height, 97 km. Nothing declares an "edge of the atmosphere" — this is a
    /// stated truncation tolerance, the same argument [`air_reaches`] makes for drag.
    pub fn top(&self) -> f64 {
        -self.scale_height * COLUMN_TAIL.ln()
    }
}

/// The fraction of the air column allowed to fall outside the integral (see [`AirColumn::top`]).
const COLUMN_TAIL: f64 = 1.0e-5;

/// **The resolution the sky is drawn at**, mirrored by `shaders/atmos.wgsl`. These are not taste: they
/// are read off the convergence measurement in `the_integral_converges_with_sample_count`, which walks
/// the WORST ray in the scene (near-horizontal view, near-horizontal sun) down against a 512×128
/// reference. At these counts that ray is within 2%; halving them costs 6.6%, which is why they are
/// what they are. A resolution is a resolution — raise them and the answer improves, which is the
/// property a fudge does not have.
pub const SKY_VIEW_STEPS: usize = 32;
/// Sun-path samples per view sample — see [`SKY_VIEW_STEPS`].
pub const SKY_SUN_STEPS: usize = 8;

/// What the air did to a ray: what it ADDED (scattered sunlight) and what it PASSED (everything behind).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scattered {
    /// In-scattered radiance, per band, at the same exposure as every other view of this world.
    pub inscatter: [f32; 3],
    /// Transmittance of the ray's own endpoint through the air in front of it — the ground's reddening
    /// under its own veil, and the reason a star low on the horizon is extinguished.
    pub transmit: [f32; 3],
}

impl Scattered {
    /// Vacuum: nothing added, everything passed.
    pub const CLEAR: Scattered = Scattered {
        inscatter: [0.0; 3],
        transmit: [1.0; 3],
    };
}

/// Radius of a point on the ray, as a RATIO of the eye's own radius. `k` is path length in units of
/// that radius, `mu_v` the ray's cosine from the eye's zenith: `|r̂ + k·d̂| = √(1 + 2k·μᵥ + k²)`.
#[inline]
fn ray_radius_ratio(k: f64, mu_v: f64) -> f64 {
    (1.0 + k * (2.0 * mu_v + k)).max(0.0).sqrt()
}

/// **Altitude of a point on the ray**, written so it survives f32 (the shader mirrors this line for
/// line). The naive `|eye + t·d| − R` subtracts two numbers that agree to seven digits at ground level
/// and keeps the noise; this multiplies by the conjugate instead, so every term is `O(h·R)` or `O(t·R)`
/// and nothing cancels: `alt = (r² − R²)/(r + R)` with `r0² − R² = h₀(2R + h₀)`.
#[inline]
fn ray_altitude(k: f64, mu_v: f64, h0: f64, radius: f64) -> f64 {
    let r0 = radius + h0;
    let num = h0 * (2.0 * radius + h0) + r0 * r0 * k * (2.0 * mu_v + k);
    // One refinement of the denominator: alt ≪ R, so `2R` is already within a fraction of a percent,
    // and this removes even that.
    let approx = num / (2.0 * radius);
    num / (2.0 * radius + approx.max(0.0))
}

/// Path length (in units of the eye's radius `r0`) at which the ray from altitude `h0` reaches
/// altitude `target_alt`, or `None` if it never does. Solves `k² + 2k·μᵥ − q = 0` for the first root
/// ahead of the eye.
///
/// The altitudes are passed rather than the radii, and that is not tidiness: `target − r0` for the
/// GROUND is `−h0`, and a camera 2 m up is 3·10⁻⁷ of Earth's radius — in the f32 the shader mirroring
/// this runs in, subtracting the two radii returns noise where the answer should be. Passing the
/// difference keeps every term small and exact.
fn ray_reaches(mu_v: f64, radius: f64, h0: f64, target_alt: f64, want_far: bool) -> Option<f64> {
    let r0 = radius + h0;
    let q = (target_alt - h0) * (2.0 * radius + h0 + target_alt) / (r0 * r0);
    let disc = mu_v * mu_v + q;
    if disc < 0.0 {
        return None; // the ray misses that radius entirely
    }
    let s = disc.sqrt();
    let (near, far) = (-mu_v - s, -mu_v + s);
    let pick = if want_far { far } else { near };
    // A root behind the eye is not on this ray; fall through to the far one if it is ahead.
    if pick > 0.0 {
        Some(pick)
    } else if far > 0.0 {
        Some(far)
    } else {
        None
    }
}

/// **Where along a ray to put the `i`-th of `n` samples, and how wide its step is.**
///
/// Uniform steps spend most of their samples on air that is not there: an exponential column varies by
/// e-foldings along a near-horizontal ray, and the in-scatter is dominated by the densest stretch. So
/// the samples are packed quadratically toward the ray's CLOSEST APPROACH to the surface — its densest
/// point, wherever that falls — which is one rule covering a ray leaving the ground (perigee behind
/// the eye, so samples bunch at the near end), a ray grazing the limb from orbit (perigee in the
/// middle) and a ray straight up. Measured: it is what takes the worst ray in the scene from 9% error
/// at 24 samples to under 1%, which is the difference between a sky the GPU can afford and one it
/// cannot (`the_integral_converges_with_sample_count`).
///
/// Returns `(k, dk)` with `Σ dk = k1 − k0` exactly, so the quadrature stays a partition of the path.
#[inline]
fn ray_sample(i: usize, n: usize, k0: f64, k1: f64, mu_v: f64) -> (f64, f64) {
    let span = k1 - k0;
    let k_min = (-mu_v).clamp(k0, k1); // parameter of closest approach: dk/dr = 0 at k = −µ
    let lo = (k_min - k0) / span;
    let u = (i as f64 + 0.5) / n as f64;
    let (k, s) = if u < lo {
        let s = (lo - u) / lo; // 1 at the near end, 0 at the perigee
        (k_min - (k_min - k0) * s * s, s)
    } else {
        let s = (u - lo) / (1.0 - lo).max(1.0e-12);
        (k_min + (k1 - k_min) * s * s, s)
    };
    (k, 2.0 * span * s / n as f64)
}

/// **Optical depth from a point out to space along one direction**, per band — the sun path, which is
/// also the whole of "does light reach here". Returns `None` when the planet itself is in the way:
/// that is the shadow, and it is a geometric fact rather than a lighting term.
///
/// `h` is the point's altitude, `mu` the cosine of the direction from ITS local zenith.
fn column_to_space(air: &AirColumn, h: f64, mu: f64, steps: usize) -> Option<[f64; 3]> {
    // Does this ray graze into the ground? Perpendicular distance from the centre is `r·√(1−μ²)`; the
    // ray misses iff that exceeds R. Expanded in `x = h/R` so the big numbers cancel algebraically
    // instead of numerically: `(2x + x²)(1 − μ²) > μ²`.
    if mu < 0.0 {
        let x = h / air.radius;
        let m2 = mu * mu;
        if (2.0 * x + x * x) * (1.0 - m2) <= m2 {
            return None; // shadowed by the body — no direct sunlight reaches this parcel
        }
    }
    // Already above the column: lit, and with nothing left in front of the Sun. (Reached only by a
    // sample that lands a hair outside the top; without it that sample would read as shadowed and
    // punch a black speck through the limb.)
    if h >= air.top() {
        return Some([0.0; 3]);
    }
    let r0 = air.radius + h;
    let k_top = ray_reaches(mu, air.radius, h, air.top(), true)?;
    let mut depth = 0.0f64;
    for i in 0..steps {
        let (k, dk) = ray_sample(i, steps, 0.0, k_top, mu);
        let alt = ray_altitude(k, mu, h, air.radius);
        depth += (-alt / air.scale_height).exp() * dk;
    }
    let ds = depth * r0 / air.scale_height; // β = τ/H, and ds = r0·dk
    Some([air.tau[0] * ds, air.tau[1] * ds, air.tau[2] * ds])
}

/// ★★ **THE atmosphere: light single-scattered into a view ray by the air it passes through.**
///
/// This is the whole sky, the whole limb, the whole aerial perspective and the whole terminator, in one
/// integral — `L = F·(P(Θ)/4)·∫ β(h)·e^{−τ_sun(s)}·e^{−τ_view(s)} ds` along the ray, with the Rayleigh
/// phase `P(Θ) = ¾(1+cos²Θ)` and `β(h) = (τ/H)·e^{−h/H}` from the DECLARED air. The `/4` rather than
/// `/4π` is the engine's standing radiance convention (the surface term is likewise `albedo·μ·F`, not
/// `albedo·μ·F/π`), so the sky and the ground it hangs over share one exposure.
///
/// All geometry arrives as cosines about the EYE's local zenith, which is what makes it identical in
/// f64 here and in f32 on the GPU:
/// * `h0` — the eye's altitude above the surface,
/// * `mu_v` — cosine of the view ray from the eye's zenith (`+1` straight up),
/// * `mu_s` — cosine of the sun from the eye's zenith (`<0` after sunset),
/// * `cos_theta` — cosine between view ray and sun (the phase angle; constant along the ray, because
///   the Sun subtends nothing at this distance — Robin: *"Sun is close to a point source"*),
/// * `t_end` — where the ray STOPS: the distance to the ground fragment, or infinity for open sky.
///
/// SINGLE scatter, no Mie/aerosol, no ozone, and a point Sun. All four are flagged, none is a dial.
pub fn air_inscatter(
    air: &AirColumn,
    h0: f64,
    mu_v: f64,
    mu_s: f64,
    cos_theta: f64,
    t_end: f64,
    sun_gain: f64,
    view_steps: usize,
    sun_steps: usize,
) -> Scattered {
    if !air.exists() || view_steps == 0 || sun_steps == 0 {
        return Scattered::CLEAR;
    }
    let r0 = air.radius + h0;
    let top = air.top();
    // Where the ray is inside the air at all. An eye ABOVE the column enters it at the near root; an eye
    // inside it starts immediately and leaves at the far one.
    let (mut k0, mut k1) = if h0 >= top {
        match ray_reaches(mu_v, air.radius, h0, top, false) {
            Some(k_in) => (
                k_in,
                ray_reaches(mu_v, air.radius, h0, top, true).unwrap_or(k_in),
            ),
            None => return Scattered::CLEAR, // looking past the planet's air entirely
        }
    } else {
        (
            0.0,
            ray_reaches(mu_v, air.radius, h0, top, true).unwrap_or(0.0),
        )
    };
    // The body itself stops the ray, whatever the caller said.
    if let Some(k_hit) = ray_reaches(mu_v, air.radius, h0, 0.0, false) {
        k1 = k1.min(k_hit);
    }
    k1 = k1.min(t_end / r0);
    k0 = k0.max(0.0);
    if !(k1 > k0) {
        return Scattered::CLEAR;
    }

    let phase = 0.75 * (1.0 + cos_theta * cos_theta);
    let mut tau_view = [0.0f64; 3];
    let mut acc = [0.0f64; 3];
    for i in 0..view_steps {
        let (k, dk) = ray_sample(i, view_steps, k0, k1, mu_v);
        let ds = dk * r0;
        let alt = ray_altitude(k, mu_v, h0, air.radius);
        let rho = (-alt / air.scale_height).exp();
        // Optical depth this step contributes, per band: β·ds with β = (τ/H)·ρ.
        let d_tau = [
            air.tau[0] / air.scale_height * rho * ds,
            air.tau[1] / air.scale_height * rho * ds,
            air.tau[2] / air.scale_height * rho * ds,
        ];
        // The sample's own zenith, and the sun's cosine from IT — the two things that turn a flat-slab
        // sky into a round one. `r̂ₚ = (r̂ + k·d̂)/|r̂ + k·d̂|`, so both cosines just divide by that length.
        let rr = ray_radius_ratio(k, mu_v).max(1.0e-12);
        let mu_s_p = (mu_s + k * cos_theta) / rr;
        // Half a step of the current cell's depth puts the sample at its own midpoint rather than its
        // near face (the difference is second order, and it is free).
        let half = [
            tau_view[0] + 0.5 * d_tau[0],
            tau_view[1] + 0.5 * d_tau[1],
            tau_view[2] + 0.5 * d_tau[2],
        ];
        if let Some(tau_sun) = column_to_space(air, alt, mu_s_p, sun_steps) {
            for b in 0..3 {
                acc[b] += (-(tau_sun[b] + half[b])).exp() * d_tau[b];
            }
        }
        for b in 0..3 {
            tau_view[b] += d_tau[b];
        }
    }
    let gain = sun_gain * phase * 0.25;
    Scattered {
        inscatter: [
            (gain * acc[0]) as f32,
            (gain * acc[1]) as f32,
            (gain * acc[2]) as f32,
        ],
        transmit: [
            (-tau_view[0]).exp() as f32,
            (-tau_view[1]).exp() as f32,
            (-tau_view[2]).exp() as f32,
        ],
    }
}

/// Two-way transmittance of the surface's reflected light through the air (in on the sun path, out on
/// the view path) — the slight reddening of the ground under its blue veil.
pub fn rayleigh_transmit(mu_v: f64, mu_s: f64, tau: [f64; 3]) -> [f32; 3] {
    let path = 1.0 / mu_v.max(0.08) + 1.0 / mu_s.max(0.08);
    [
        (-tau[0] * path).exp() as f32,
        (-tau[1] * path).exp() as f32,
        (-tau[2] * path).exp() as f32,
    ]
}

/// ★★★ **THE SKY'S OWN LIGHT, FALLING ON A SURFACE** — the fourth consumer of the one integral
/// (docs/46 row 56, docs/66).
///
/// Robin, looking at grass that rendered black in full daylight (2026-08-09): *"one would assume the
/// light scatter from the atmosphere would make the grass green, no?"* It would, and it did not,
/// because every surface in this engine received DIRECT SUNLIGHT and nothing else. The atmosphere's
/// radiance was computed three times over — by the sky pass, by the ground's aerial perspective, by
/// the star field's extinction — and never came back as irradiance on anything.
///
/// A horizontal ground hid it: at Galway with the sun 52° up, ground faces the sun and the direct
/// term is most of the answer. The first VERTICAL surface at eye level exposed it — a 3 mm blade
/// standing up presents almost no area to a high sun, so direct light gives it nearly zero, while in
/// reality it is lit almost entirely by a hemisphere of blue.
///
/// This is the honest quantity:
///
/// ```text
/// E(n) = ∫_hemisphere L_sky(ω) · max(n·ω, 0) dω
/// ```
///
/// `L_sky` is [`air_inscatter`] looking along `ω` with no far limit — the same function, at the same
/// exposure, that draws the pixel of sky in that direction. So a surface cannot be lit by a different
/// sky from the one above it, which is the Law II failure this would otherwise invite.
///
/// **Quadrature, not a constant.** The hemisphere is sampled on a `rings × spokes` grid in
/// cos-elevation (equal solid angle per ring, so no band is over-weighted) and the result CONVERGES as
/// the grid refines — pinned by `sky_irradiance_converges`. That is what makes it a declared
/// computation rather than an ambient dial: **do not replace this with a constant**, which is the
/// thing docs/46 row 56 explicitly forbids.
///
/// Returned at the same exposure as everything else, per band.
pub fn sky_irradiance(
    air: &AirColumn,
    altitude_m: f64,
    normal_dot_up: f64,
    sun_dot_up: f64,
    normal_dot_sun: f64,
    sun_gain: f64,
    rings: usize,
    spokes: usize,
) -> [f64; 3] {
    if !air.exists() || rings == 0 || spokes == 0 {
        return [0.0; 3];
    }
    // A local frame in which the geometry is expressible with the three cosines the caller has: `up`
    // is +Y, the sun lies in the +X half-plane, and the normal is placed to satisfy both of its own
    // dot products. Everything below is then plain vector algebra, and the caller never has to hand
    // in a basis it does not have.
    let up = glam::DVec3::Y;
    let s_y = sun_dot_up.clamp(-1.0, 1.0);
    let sun = glam::DVec3::new((1.0 - s_y * s_y).max(0.0).sqrt(), s_y, 0.0);
    let n_y = normal_dot_up.clamp(-1.0, 1.0);
    let n_perp = (1.0 - n_y * n_y).max(0.0).sqrt();
    // n·sun = n_x·sun_x + n_y·sun_y  ⇒  n_x follows, and n_z takes up the slack.
    let n_x = if sun.x.abs() > 1e-9 {
        ((normal_dot_sun - n_y * sun.y) / sun.x).clamp(-n_perp, n_perp)
    } else {
        0.0
    };
    let n_z = (n_perp * n_perp - n_x * n_x).max(0.0).sqrt();
    let n = glam::DVec3::new(n_x, n_y, n_z);

    let mut acc = [0.0f64; 3];
    for r in 0..rings {
        // Equal solid angle per ring: uniform in cos(elevation) from the horizon to the zenith.
        let cos_e = (r as f64 + 0.5) / rings as f64;
        let sin_e = (1.0 - cos_e * cos_e).max(0.0).sqrt();
        for s in 0..spokes {
            let az = (s as f64 + 0.5) / spokes as f64 * std::f64::consts::TAU;
            let dir = glam::DVec3::new(sin_e * az.cos(), cos_e, sin_e * az.sin());
            let cos_i = n.dot(dir);
            if cos_i <= 0.0 {
                continue; // below this surface's own horizon; it receives nothing from there
            }
            let sky = air_inscatter(
                air,
                altitude_m,
                dir.dot(up),
                sun.dot(up),
                dir.dot(sun),
                f64::INFINITY,
                sun_gain,
                SKY_VIEW_STEPS,
                SKY_SUN_STEPS,
            );
            for b in 0..3 {
                acc[b] += sky.inscatter[b] as f64 * cos_i;
            }
        }
    }
    // ★ **THE SOLID ANGLE PER SAMPLE, NOT PER ACCEPTED SAMPLE.** Sampling uniformly in cos-elevation
    // gives every sample the same `dω = 2π/N` over the hemisphere, so the Riemann sum is
    // `E = dω · Σ L·cosθ` and `N` is the WHOLE grid — including the samples a tilted surface rejects,
    // because those really do contribute nothing.
    //
    // Normalising by the accumulated cosine instead — which is what stood here — divides by a
    // different number for every orientation, and MEASURED it made a vertical surface receive **1.6×**
    // what an upward-facing one did. `a_vertical_surface_at_noon_is_lit_by_the_sky` caught it: an
    // upright face sees half the sky and must get about half the light, and a formula that says
    // otherwise is describing its own sampling. Isotropic check: uniform `L` gives `πL` facing up and
    // `πL/2` upright, which is the analytic answer.
    let k = std::f64::consts::TAU / (rings * spokes) as f64;
    [acc[0] * k, acc[1] * k, acc[2] * k]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::Aggregate;
    use crate::materials;
    use crate::orbit::Body;

    /// Sutton–Graves stagnation heating, pinned to a REAL entry so the constant AND its units are right —
    /// the PDF sources would not render, and as W/cm² the same constant is 10⁴× too large. The Stardust
    /// capsule (v≈12.6 km/s, R_n=0.229 m) peaked near 1200 W/cm² ≈ 1.2e7 W/m²; at the ρ≈2e-4 kg/m³ of peak
    /// heating this correlation must land in that ballpark, not orders off.
    #[test]
    fn stagnation_heating_matches_a_real_reentry_and_scales_right() {
        let q_stardust = stagnation_heat_flux(2.0e-4, 12_600.0, 0.229);
        assert!(
            (5.0e6..2.0e7).contains(&q_stardust),
            "Sutton-Graves must reproduce the Stardust peak (~1.2e7 W/m²); got {q_stardust:.2e} W/m² — a \
             value 10^4 off would be the W/cm² unit error"
        );
        // v³ scaling: doubling the speed is 8× the flux.
        let q1 = stagnation_heat_flux(1.0, 1000.0, 0.5);
        let q2 = stagnation_heat_flux(1.0, 2000.0, 0.5);
        assert!(
            (q2 / q1 - 8.0).abs() < 1.0e-6,
            "heating must go as v³: {q1} -> {q2}"
        );
        // √ρ scaling, 1/√R scaling.
        assert!(
            (stagnation_heat_flux(4.0, 1000.0, 0.5) / q1 - 2.0).abs() < 1.0e-6,
            "√ρ"
        );
        assert!(
            (stagnation_heat_flux(1.0, 1000.0, 2.0) / q1 - 0.5).abs() < 1.0e-6,
            "1/√R"
        );
        // No bow shock below Mach 1 ⇒ no stagnation heating.
        assert_eq!(
            stagnation_heat_flux(1.2, 200.0, 0.5),
            0.0,
            "subsonic: no shock, no stagnation heat"
        );
    }

    /// The generic body⊕atmosphere operator (docs/58): drag slows any body, aeroheating warms it, and at
    /// the boiling point it ablates — all from the material, nothing meteor-specific. Iron is the fixture.
    ///
    /// Every call here passes a skin already equal to the radius — i.e. a body heated all the way through —
    /// so these assertions still test exactly what they were written to test. That the bulk case is
    /// reachable as `skin = r` is the point: the thermal-skin model does not replace bulk heating, it
    /// contains it as the limit.
    #[test]
    fn atmospheric_step_slows_heats_and_ablates_any_body() {
        let mats = materials::load();
        let iron = &mats[materials::index_of(&mats, "iron")];
        let v = glam::DVec3::new(3000.0, 0.0, 0.0); // supersonic
        let step = |rho: f64, temp: f64| {
            atmospheric_step(rho, v, 1000.0, 0.3, temp, 0.3, 288.0, iron, 1.0 / 60.0)
        };

        // DRAG opposes motion.
        let s = step(1.2, 288.0);
        assert!(
            s.drag_accel.x < 0.0,
            "drag must oppose motion, got {:?}",
            s.drag_accel
        );

        // HEATING raises the temperature of a cold fast body.
        assert!(
            s.temp_k > 288.0,
            "a fast body in air must heat above ambient, got {}",
            s.temp_k
        );

        // ABLATION needs true METEOR speed, not merely supersonic: at 3000 m/s iron's radiative loss at its
        // boiling point EXCEEDS the aeroheating (net < 0), so it does NOT ablate — the equilibrium temp is
        // below boiling. Only at ~12 km/s does the heat flux overwhelm radiation and vaporise mass. That the
        // slow case refuses to ablate is the physics being honest, not a bug.
        let t_boil = iron.boil_point().unwrap();
        let slow_at_boil = atmospheric_step(
            1.2,
            glam::DVec3::new(3000.0, 0.0, 0.0),
            1000.0,
            0.3,
            t_boil,
            0.3,
            288.0,
            iron,
            1.0 / 60.0,
        );
        assert_eq!(
            slow_at_boil.ablated_mass, 0.0,
            "a merely-supersonic body cannot sustain iron at boiling"
        );

        // Consistent mass/radius: 1000 kg of iron is r0, and the operator recomputes radius from the
        // ablated mass at iron's own density, so ablation must return a radius below r0.
        let r0 = (3.0 * 1000.0 / (4.0 * std::f64::consts::PI * iron.density as f64)).cbrt();
        let hyper = atmospheric_step(
            1.2,
            glam::DVec3::new(12_000.0, 0.0, 0.0),
            1000.0,
            r0,
            t_boil,
            r0,
            288.0,
            iron,
            1.0 / 60.0,
        );
        assert!(
            hyper.ablated_mass > 0.0,
            "at meteor speed, excess heat must vaporise mass"
        );
        assert!(
            hyper.radius_m < r0,
            "ablation must shrink the body: {} vs {r0}",
            hyper.radius_m
        );
        assert!(
            (hyper.temp_k - t_boil).abs() < 1.0,
            "temperature pins at the boiling point while ablating"
        );

        // VACUUM / SUBSONIC: no air ⇒ nothing happens; below Mach 1 ⇒ drag only, no heating.
        let vac = atmospheric_step(0.0, v, 1000.0, 0.3, 288.0, 0.3, 288.0, iron, 1.0 / 60.0);
        assert_eq!(vac.drag_accel, glam::DVec3::ZERO, "no air, no drag");
        assert_eq!(vac.temp_k, 288.0, "no air, no heating");
        let slow = atmospheric_step(
            1.2,
            glam::DVec3::new(100.0, 0.0, 0.0),
            1000.0,
            0.3,
            288.0,
            0.3,
            288.0,
            iron,
            1.0 / 60.0,
        );
        assert!(
            (slow.temp_k - 288.0).abs() < 1.0,
            "subsonic: no shock heating"
        );
    }
    /// **A real entry, flown, with the books kept.** An iron grain comes in at 20 km/s and ablates its
    /// way down through Earth's own emergent air. The thing being tested is that the mass it loses does
    /// not cease to exist: at every step, body + trail is EXACTLY the mass that entered. Before this,
    /// `ablated_mass` was subtracted from the body and dropped.
    ///
    /// A GRAIN, not a boulder, and for a physical reason: `atmospheric_step` heats the body's whole mass
    /// at its bulk heat capacity, which is only true while the body is thinner than its own thermal skin
    /// depth (√(αt) ≈ 1.5 cm for iron over ten seconds). A half-metre iron body run through this model
    /// barely warms — and, as it happens, that is also what really becomes of one: small meteoroids burn
    /// up, iron meteorites land. The model gets the right answer here for the right reason, but the
    /// isothermal-body assumption is the flagged limit (real ablation is a SURFACE process, and resolving
    /// it needs a temperature profile through the body).
    #[test]
    fn what_a_body_ablates_becomes_a_trail_and_the_mass_is_never_lost() {
        let mats = materials::load();
        let iron = &mats[materials::index_of(&mats, "iron")];
        let air_mat = &mats[materials::index_of(&mats, "air")];
        let earth = crate::planet::earth();
        let g = earth.gravity_at(earth.radius());
        let air = AirShell::new(earth.surface_pressure(), air_mat, 288.0, g);

        // A 1 cm iron grain entering at 20 km/s, straight down from 120 km.
        let rho_iron = iron.density as f64;
        let mut radius: f64 = 0.005;
        let mut mass = rho_iron * (4.0 / 3.0) * std::f64::consts::PI * radius.powi(3);
        let start_mass = mass;
        let mut temp = air.ambient_temp_k;
        let mut skin = 0.0_f64;
        let mut alt: f64 = 120_000.0;
        let mut vel = DVec3::new(0.0, -20_000.0, 0.0);
        let mut trail = Trail::default();
        let dt = 0.001;

        let mut shed_hot = 0;
        for _ in 0..20_000 {
            if alt <= 0.0 || mass <= 0.0 {
                break;
            }
            let rho = air.density_at(alt);
            let s = atmospheric_step(
                rho,
                vel,
                mass,
                radius,
                temp,
                skin,
                air.ambient_temp_k,
                iron,
                dt,
            );
            skin = s.skin_m;
            if s.ablated_mass > 0.0 {
                // The vapour leaves the body carrying the body's own velocity and temperature.
                trail.shed(
                    s.ablated_mass,
                    materials::index_of(&mats, "iron"),
                    DVec3::new(0.0, alt, 0.0),
                    vel,
                    s.temp_k,
                );
                if s.temp_k > air.ambient_temp_k {
                    shed_hot += 1;
                }
            }
            mass -= s.ablated_mass;
            radius = s.radius_m;
            temp = s.temp_k;
            vel += (s.drag_accel + DVec3::new(0.0, -g, 0.0)) * dt;
            alt += vel.y * dt;
            trail.step(&mats, dt, |at| {
                (air.density_at(at.y.max(0.0)), air.ambient_temp_k)
            });

            // THE INVARIANT, every single step: nothing has left the books.
            let booked = mass + trail.mass();
            assert!(
                (booked / start_mass - 1.0).abs() < 1e-12,
                "mass conserved: {booked:.9e} vs {start_mass:.9e}"
            );
        }

        // The entry has to have actually HAPPENED for the invariant to mean anything.
        assert!(
            mass < start_mass,
            "the grain ablated ({mass:.3e} kg of {start_mass:.3e})"
        );
        assert!(
            shed_hot > 0,
            "and the vapour left hot — that is the trail you see"
        );
        assert!(trail.mass() > 0.0, "the shed mass is somewhere");
        assert!(
            trail.merged_kg() > 0.0,
            "and some of it has finished cooling into the air ({:.3e} kg)",
            trail.merged_kg()
        );
    }

    /// **The camera changes representation, never existence** (Law IV). The same shed mass, tracked as
    /// resolved parcels or simply booked into the atmosphere, is the same mass — which is what makes it
    /// legitimate to watch a trail from orbit without resolving a single puff.
    #[test]
    fn a_trail_holds_the_same_mass_resolved_or_booked() {
        let mut resolved = Trail::default();
        let mut booked = Trail::default();
        for i in 0..10 {
            let m = 0.5 * (i + 1) as f64;
            resolved.shed(
                m,
                0,
                DVec3::new(0.0, 60_000.0, 0.0),
                DVec3::new(0.0, -1.0e4, 0.0),
                3134.0,
            );
            booked.book(m);
        }
        assert_eq!(
            resolved.mass(),
            booked.mass(),
            "one mass, two representations"
        );
        assert_eq!(
            resolved.parcels().len(),
            10,
            "resolved: individual puffs to draw"
        );
        assert!(
            booked.parcels().is_empty(),
            "booked: nothing to draw, the mass is in the air"
        );
    }

    /// A shed parcel expands to the air around it, and cools by the SAME radiation law the body uses —
    /// so its size and its glow are both consequences, not settings. High, thin air ⇒ a bigger, more
    /// rapidly cooling puff than the same mass shed low down.
    #[test]
    fn shed_vapor_expands_to_the_local_air_and_radiates_its_heat_away() {
        let mats = materials::load();
        let iron = &mats[materials::index_of(&mats, "iron")];
        let p = VaporParcel {
            mass_kg: 1.0,
            material: materials::index_of(&mats, "iron"),
            pos: DVec3::ZERO,
            vel: DVec3::new(0.0, -1.0e4, 0.0),
            temp_k: 3134.0, // iron's boiling point: the temperature ablation sheds it at
            shed_temp_k: 3134.0,
        };
        // Size is set by the air it expands into: 1 kg at sea level vs at 80 km.
        let (rho_low, rho_high) = (1.2, 1.0e-5);
        assert!(
            p.radius_in(rho_high) > 10.0 * p.radius_in(rho_low),
            "thin air ⇒ a far bigger puff"
        );

        // It cools toward ambient and never below it.
        let mut hot = p;
        for _ in 0..200 {
            hot = vapor_step(hot, rho_low, 288.0, iron, 0.01);
        }
        assert!(
            hot.temp_k < p.temp_k,
            "a hot parcel radiates its heat away ({:.0} K)",
            hot.temp_k
        );
        assert!(
            hot.temp_k >= 288.0,
            "and never cools below the air it is in"
        );
        // **And it must FINISH.** Radiative cooling is asymptotic, so "has it reached ambient?" is a test
        // that never passes — a trail of parcels crept to 288.00 K and sat there permanently. A parcel is
        // air once it has radiated all but a hundredth of the heat it was shed with.
        let mut fading = p;
        let mut steps = 0;
        while !fading.merged_into_air(288.0) && steps < 100_000 {
            fading = vapor_step(fading, rho_low, 288.0, iron, 0.01);
            steps += 1;
        }
        assert!(
            fading.merged_into_air(288.0),
            "a shed parcel must eventually BE air, not approach it forever ({:.4} K after {steps} steps)",
            fading.temp_k
        );

        // And it is slowed by the air, like anything else moving through a fluid.
        assert!(
            hot.vel.length() < p.vel.length(),
            "the parcel is dragged to a stop, not left at 10 km/s"
        );

        // A parcel already at ambient is simply air, and says so.
        let cold = VaporParcel { temp_k: 288.0, ..p };
        assert!(
            cold.merged_into_air(288.0),
            "cooled to ambient ⇒ it has joined the atmosphere"
        );
    }

    use glam::DVec3;

    #[test]
    fn the_blue_marble_is_derived_from_the_air_not_painted() {
        // λ⁻⁴: blue scatters ~4.4× more than red — the sky is blue because molecules are small.
        let tau = rayleigh_tau(1.0);
        assert!(
            (tau[2] / tau[0] - (650.0f64 / 450.0).powf(4.05)).abs() < 0.1,
            "the λ⁻⁴ law (got ratio {:.2})",
            tau[2] / tau[0]
        );
        // The day-side veil is BLUE-dominant…
        let v = rayleigh_veil(0.8, 0.8, 0.5, tau, 22.0, 0.0);
        assert!(
            v[2] > v[1] && v[1] > v[0],
            "blue > green > red veil (got {v:?})"
        );
        // …brighter at the limb (long slant path)…
        let limb = rayleigh_veil(0.1, 0.8, 0.5, tau, 22.0, 0.0);
        assert!(
            limb[2] > v[2],
            "limb glow exceeds nadir (got {} vs {})",
            limb[2],
            v[2]
        );
        // …zero on the night side, and zero on an airless world. No atmosphere, no blue. Honest.
        assert_eq!(
            rayleigh_veil(0.8, -0.1, 0.5, tau, 22.0, 0.0),
            [0.0; 3],
            "night is dark"
        );
        let vacuum = rayleigh_tau(0.0);
        assert_eq!(
            rayleigh_veil(0.8, 0.8, 0.5, vacuum, 22.0, 0.0),
            [0.0; 3],
            "the Moon stays colorless"
        );
        // And the ground under the air reddens slightly (blue is scattered OUT of the beam).
        let t = rayleigh_transmit(0.8, 0.8, tau);
        assert!(t[0] > t[2], "transmittance favors red (got {t:?})");
    }

    #[test]
    fn the_rayleigh_sky_is_blue_overhead_and_pale_at_the_horizon() {
        // The terrain scene's sky (shaders/sky.wgsl) is this SAME single-scatter law evaluated along the
        // view ray: mu_v = the ray's cosine from the zenith (1 overhead, →0 at the horizon), mu_s the
        // sun's elevation cosine, cos_theta = ray·sun. This test locks the two properties the shader
        // renders, so the derived-physics claim can't silently regress into a hand-painted gradient.
        let tau = rayleigh_tau(1.0); // Earth's 1-atm air — the same τ the space band's blue marble uses
        let sun_y = 0.9f64; // a sun most of the way up
                            // Look straight up (short air path) vs out at the horizon (long slant path). cos_theta uses the
                            // ray·sun geometry for each; the horizon sample looks away from the sun (its dimmest azimuth).
        let zenith = rayleigh_veil(1.0, sun_y, sun_y, tau, 22.0, 0.0); // ray=up ⇒ cosθ = sun.y
        let horizon = rayleigh_veil(0.02, sun_y, -0.2, tau, 22.0, 0.0); // ray near-horizontal, anti-sun

        // (1) OVERHEAD IS BLUE: short path ⇒ (1−e^{−τ·path}) ≈ τ·path, so radiance ∝ τ ∝ λ⁻⁴ — blue
        //     dominates. The blue/red ratio at the zenith is far above 1.
        let zen_ratio = zenith[2] / zenith[0];
        assert!(
            zenith[2] > zenith[1] && zenith[1] > zenith[0],
            "the zenith is blue: blue > green > red (got {zenith:?})"
        );
        // (2) THE HORIZON PALES: long path saturates every band toward 1, so the colour whitens — its
        //     blue/red ratio collapses toward unity, and it is BRIGHTER overall (more air scatters more
        //     light). This is the pale/warm horizon band, and it FALLS OUT of the path length alone.
        let hor_ratio = horizon[2] / horizon[0];
        assert!(
            zen_ratio > hor_ratio + 1.0,
            "the zenith is far bluer than the horizon (zenith B/R {zen_ratio:.2} vs horizon {hor_ratio:.2})"
        );
        assert!(
            horizon[2] > zenith[2],
            "the horizon is brighter than the zenith (longer air path; got {} vs {})",
            horizon[2],
            zenith[2]
        );
        // (3) NO AIR, NO SKY: strip the declared atmosphere and the whole gradient goes black — derived,
        //     never painted, exactly like the space band's airless Moon.
        let vacuum = rayleigh_tau(0.0);
        assert_eq!(
            rayleigh_veil(1.0, sun_y, sun_y, vacuum, 22.0, 0.0),
            [0.0; 3],
            "airless ⇒ black sky"
        );
    }

    #[test]
    /// The derived barometric profile must reproduce the REAL atmosphere, at altitudes anyone can check.
    /// Nothing here is fitted: surface pressure is the declared air mass's own weight (planet.rs), R_s is
    /// air's real molar mass, and H is the same scale_height the settling-column test converges to. If
    /// this matches the standard atmosphere to within the isothermal approximation's honest error, the
    /// engine has a real medium rather than a number.
    #[test]
    fn the_derived_air_density_profile_matches_the_real_atmosphere() {
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let earth = crate::planet::earth();
        let p0 = earth.surface_pressure();
        let g = earth.gravity_at(earth.radius());
        let t = 288.0; // ISA sea-level temperature

        // Sea level: the real value is 1.225 kg/m^3.
        let rho0 = air_density_at(p0, air, t, g, 0.0);
        assert!(
            (rho0 - 1.225).abs() < 0.06,
            "sea-level air density {rho0:.3} should be ~1.225 kg/m^3 (from {p0:.0} Pa emergent surface pressure)"
        );

        // One scale height up, density must fall by exactly 1/e — that IS the barometric law.
        let h = scale_height(air, t, g);
        assert!(
            (7500.0..9500.0).contains(&h),
            "Earth scale height {h:.0} m should be ~8.4 km"
        );
        let rho_h = air_density_at(p0, air, t, g, h);
        assert!(
            (rho_h / rho0 - std::f64::consts::E.recip()).abs() < 1.0e-9,
            "one scale height must be exactly 1/e"
        );

        // Monotone decreasing, and effectively vacuum by the Karman line.
        let mut prev = rho0;
        for km in 1..=20 {
            let r = air_density_at(p0, air, t, g, km as f64 * 1000.0);
            assert!(r < prev, "density must fall monotonically with altitude");
            prev = r;
        }
        assert!(
            air_density_at(p0, air, t, g, 100_000.0) < 1.0e-4,
            "≈vacuum at the Karman line"
        );
    }

    /// An AIRLESS body gives exactly zero density and therefore exactly zero drag — the Moon
    /// (`atmosphere_mass: 0.0`) must not acquire an atmosphere by accident.
    #[test]
    fn an_airless_body_has_no_air_and_no_drag() {
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let moon = crate::planet::moon();
        let rho = air_density_at(
            moon.surface_pressure(),
            air,
            288.0,
            moon.gravity_at(moon.radius()),
            0.0,
        );
        assert_eq!(rho, 0.0, "an airless body must have zero air density");
        let a = drag_accel(rho, glam::DVec3::new(1000.0, 0.0, 0.0), 1.0, 10.0, 1.0);
        assert_eq!(a, glam::DVec3::ZERO, "no air ⇒ no drag, exactly");
    }

    /// Drag opposes motion, scales as v², and never adds energy — the property the deleted `DRAG` fudge
    /// could not guarantee (it was a blanket per-step multiply).
    #[test]
    fn drag_opposes_motion_scales_quadratically_and_only_removes_energy() {
        let v = glam::DVec3::new(30.0, -10.0, 5.0);
        let a1 = drag_accel(1.225, v, 1.0, 100.0, 1.0);
        assert!(a1.dot(v) < 0.0, "drag must oppose motion");
        // Double the speed ⇒ four times the force.
        let a2 = drag_accel(1.225, v * 2.0, 1.0, 100.0, 1.0);
        assert!(
            (a2.length() / a1.length() - 4.0).abs() < 1.0e-9,
            "quadratic: {}x for 2x speed",
            a2.length() / a1.length()
        );
        // Applied over any positive dt, speed strictly decreases — never a rebound, never a gain.
        let after = v + a1 * 0.01;
        assert!(
            after.length() < v.length(),
            "drag must only remove kinetic energy"
        );
    }

    fn airs_declared_constants_give_the_real_gas_constant_and_scale_height() {
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let rs = specific_gas_constant(air);
        assert!(
            (rs - 287.0).abs() < 2.0,
            "R_s = R_u/M ≈ 287 J/(kg·K) (got {rs:.1})"
        );
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
    fn a_dense_body_ploughing_through_air_feels_drag_and_momentum_is_conserved() {
        // docs/26 emergence test 4: DRAG is not a coefficient — it is a dense solid exchanging momentum
        // with the air parcels it sweeps, through the same contact machinery as everything else (the
        // unequal-mass F/m form: equal-and-opposite FORCES, each particle divided by its own mass).
        // Assertions: the body slows, the air gains exactly what the body loses (momentum conserved to
        // float precision), and no energy is created. HONESTY FLAG: v0 parcels are isothermal-elastic,
        // so the swept air gains bulk motion but not yet temperature — entry GLOW (test 5) needs the gas
        // energy equation (compression work → internal energy), the next rung.
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let r = 1.0_f64; // parcel radius (m)
        let parcel_m = 1.225 * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3); // real air mass
        let body_m = 2900.0 * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3); // real basalt mass
        let contact = gas_contact_from_material(air, r, parcel_m, 101_325.0);

        // A corridor of resting parcels; the body flies down its axis.
        let mut particles = vec![Body {
            pos: DVec3::new(0.0, 0.0, -4.0),
            vel: DVec3::new(0.0, 0.0, 60.0),
            mass: body_m,
        }];
        for ix in -2i32..2 {
            for iy in -2i32..2 {
                for iz in 0..12 {
                    particles.push(Body {
                        pos: DVec3::new(
                            (ix as f64 + 0.5) * 2.0 * r,
                            (iy as f64 + 0.5) * 2.0 * r,
                            iz as f64 * 2.0 * r,
                        ),
                        vel: DVec3::ZERO,
                        mass: parcel_m,
                    });
                }
            }
        }
        let mut agg = Aggregate::new(particles, 0.1).with_contact(contact, parcel_m);
        agg.self_gravity = false;

        let p0: DVec3 = agg.particles.iter().map(|b| b.vel * b.mass).sum();
        let ke0: f64 = agg
            .particles
            .iter()
            .map(|b| 0.5 * b.mass * b.vel.length_squared())
            .sum();
        let v0 = agg.particles[0].vel.z;
        let mut acc = agg.accelerations();
        for _ in 0..800 {
            agg.step(&mut acc, 1.0e-3);
        }
        let p1: DVec3 = agg.particles.iter().map(|b| b.vel * b.mass).sum();
        let ke1: f64 = agg
            .particles
            .iter()
            .map(|b| 0.5 * b.mass * b.vel.length_squared())
            .sum();
        let v1 = agg.particles[0].vel.z;
        let air_pz: f64 = agg.particles[1..].iter().map(|b| b.mass * b.vel.z).sum();
        println!(
            "drag: body {v0:.1} → {v1:.2} m/s · air gained {air_pz:.0} kg·m/s · ΔP {:.2e} · KE {ke0:.0} → {ke1:.0} J",
            (p1 - p0).length()
        );

        assert!(
            v1 < v0 * 0.999,
            "the body decelerates — drag EMERGES from swept air (v {v0} → {v1})"
        );
        assert!(
            air_pz > 0.0,
            "the air is swept forward — it gained the body's momentum"
        );
        assert!(
            (p1 - p0).length() < 1.0e-6 * p0.length(),
            "momentum conserved across the phase boundary (drift {:.3e})",
            (p1 - p0).length()
        );
        assert!(
            ke1 <= ke0 * 1.001,
            "no energy created (KE {ke0:.0} → {ke1:.0} J)"
        );
    }

    #[test]
    fn hypersonic_entry_heats_the_swept_air_to_incandescence() {
        // docs/26 emergence test 5: the FIREBALL is mostly air. At entry speed, a swept parcel's
        // ordered relative KE thermalizes through the shock — the strong-shock limit (restitution → 0;
        // a Mach-dependent restitution is the flagged refinement) — and the dissipation→temperature
        // machinery routes it into the parcel's heat. The emergent scale is the STAGNATION temperature
        // T ≈ T₀ + v²/(2·c_p): at 8 km/s that is ~32,000 K — glowing plasma, from nothing but the
        // declared c_p and the one contact law. Momentum stays conserved through it all.
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let r = 1.0_f64;
        let parcel_m = 1.225 * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);
        let body_m = 2900.0 * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);
        let v_entry = 8_000.0;
        let mut contact = gas_contact_from_material(air, r, parcel_m, 101_325.0);
        // Strong-shock limit: the collision is fully thermalizing (e ≈ 0), not elastic.
        contact.normal_damp = crate::granular::damping_for_restitution(0.05, contact.stiffness);

        let mut particles = vec![Body {
            pos: DVec3::new(0.0, 0.0, -4.0),
            vel: DVec3::new(0.0, 0.0, v_entry),
            mass: body_m,
        }];
        for ix in -2i32..2 {
            for iy in -2i32..2 {
                for iz in 0..12 {
                    particles.push(Body {
                        pos: DVec3::new(
                            (ix as f64 + 0.5) * 2.0 * r,
                            (iy as f64 + 0.5) * 2.0 * r,
                            iz as f64 * 2.0 * r,
                        ),
                        vel: DVec3::ZERO,
                        mass: parcel_m,
                    });
                }
            }
        }
        let cp = air.thermal.as_ref().unwrap().specific_heat as f64; // 1005 J/(kg·K)
        let mut agg = Aggregate::new(particles, 0.1)
            .with_contact(contact, parcel_m)
            .with_specific_heat(cp);
        agg.self_gravity = false;

        let p0: DVec3 = agg.particles.iter().map(|b| b.vel * b.mass).sum();
        let mut acc = agg.accelerations();
        for _ in 0..600 {
            agg.step(&mut acc, 1.0e-5);
        }
        let p1: DVec3 = agg.particles.iter().map(|b| b.vel * b.mass).sum();
        let hottest = agg.temps.iter().cloned().fold(0.0f32, f32::max) as f64;
        let t_stag = 288.0 + v_entry * v_entry / (2.0 * cp);
        println!(
            "entry: hottest parcel {hottest:.0} K · stagnation scale {t_stag:.0} K · ΔP {:.2e}",
            (p1 - p0).length()
        );

        // The sub-parcel shock closure (`Contact::shock` — geometric, no tunable constant) thermalizes
        // the relative motion within one parcel crossing: the swept air passes visible incandescence,
        // the docs/26 test-5 bar. HONESTY FLAG: the quantitative post-shock value (Rankine–Hugoniot,
        // ~12,000 K at Mach 23) needs resolved shock layers / finer parcels — this coarse corridor is
        // mostly grazing hits; matching that number is the refinement, the GLOW is the emergence.
        assert!(
            hottest > 800.0,
            "the shocked air GLOWS — entry plasma emerges (hottest {hottest:.0} K from 288)"
        );
        assert!(
            hottest < 3.0 * t_stag,
            "and stays at the physical (stagnation) scale, not runaway (hottest {hottest:.0} vs {t_stag:.0} K)"
        );
        assert!(
            (p1 - p0).length() < 1.0e-6 * p0.length(),
            "momentum conserved through the shock heating"
        );
    }

    #[test]
    fn the_sph_air_field_is_normalized_symmetric_and_finds_hydrostatic_balance() {
        // docs/26, the 3D generalization of the column. Three checks on the SPH field:
        // (1) NORMALIZATION: on a uniform lattice the kernel density estimate equals m/spacing³;
        // (2) SYMMETRY: pressure forces conserve momentum exactly by construction;
        // (3) HYDROSTATIC BALANCE in 3D: a settled column of parcels under gravity carries its own
        //     weight — basal ρ·R_s·T ≈ Σm·g/A (the 1D exponential result, now from the 3D field).
        let mats = materials::load();
        let air = &mats[materials::index_of(&mats, "air")];
        let rs_t = specific_gas_constant(air) * 288.0;
        let g = 9.81;

        // (1) Normalization on a 6³ lattice, checked at the interior points.
        let spacing = 1_000.0;
        let mut pts = Vec::new();
        for x in 0..6 {
            for y in 0..6 {
                for z in 0..6 {
                    pts.push(glam::DVec3::new(
                        x as f64 * spacing,
                        y as f64 * spacing,
                        z as f64 * spacing,
                    ));
                }
            }
        }
        let m = 1.0e6; // kg per parcel → expected ρ = m/spacing³ = 1e-3 kg/m³ (arbitrary; scale-free)
        let mut f = AirField::new(pts, m, 2.0 * spacing, rs_t);
        f.compute_density();
        let center = 3 * 36 + 3 * 6 + 3; // an interior lattice point
        let expected = m / spacing.powi(3);
        assert!(
            (f.rho[center] - expected).abs() / expected < 0.05,
            "kernel density on a lattice ≈ m/spacing³ (got {:.3e} vs {expected:.3e})",
            f.rho[center]
        );

        // (2) Momentum symmetry on a random-ish (fibonacci) cloud.
        let cloud: Vec<glam::DVec3> = (0..64)
            .map(|i| crate::impact::fib_dir(i, 64) * (spacing * (0.4 + 0.02 * i as f64)))
            .collect();
        let mut fc = AirField::new(cloud, m, 2.0 * spacing, rs_t);
        fc.compute_density();
        let total: glam::DVec3 = fc.accelerations(glam::DVec3::ZERO).iter().copied().sum();
        assert!(
            total.length() < 1.0e-9,
            "SPH pressure forces sum to zero — momentum conserved by construction"
        );

        // (3) 3D HYDROSTATIC BALANCE. Root cause of the earlier collapse/under-convergence: the
        //     column had NO lateral confinement, so the gas flowed sideways into vacuum and no base
        //     pressure could ever build. The honest boundary for a representative column inside a WIDE
        //     atmosphere is mirror symmetry — ghost side walls (its lateral neighbours are identical
        //     columns) — plus the ghost floor. The exponential ATTRACTOR is already proven in 1D at
        //     0.2%; here the claim is BALANCE: the settled 3D field must satisfy hydrostatics
        //     pointwise — kernel-density pressure at a height = weight of everything above it — at an
        //     interior height AND near the base (~1 atm, since one real column mass is declared).
        //     (Measurements are taken OFF the wall: kernel estimates in the first half-spacing of a
        //     mirror boundary self-inflate — a known SPH artifact, flagged.)
        let dz = 800.0;
        let h_init = rs_t / g; // start at the 1D-proven profile; relaxation removes lattice noise
        let n_side = 3usize;
        let n_up = 18usize;
        let mut col = Vec::new();
        for x in 0..n_side {
            for zz in 0..n_side {
                for y in 0..n_up {
                    let f = (y as f64 + 0.5) / (n_up as f64 + 1.0);
                    col.push(glam::DVec3::new(
                        x as f64 * dz,
                        -h_init * (1.0 - f).ln(),
                        zz as f64 * dz,
                    ));
                }
            }
        }
        let n_col = col.len() as f64;
        let area = (n_side as f64 * dz) * (n_side as f64 * dz);
        let m_parcel = 10_332.0 * area / n_col; // one real atmosphere per column area
        let mut field = AirField::new(col, m_parcel, 2.0 * dz, rs_t)
            .with_floor(0.0)
            .with_walls(
                (-0.5 * dz, (n_side as f64 - 0.5) * dz),
                (-0.5 * dz, (n_side as f64 - 0.5) * dz),
            );
        // Relaxation at a CFL-appropriate step (c_s = √(R_s·T) ≈ 287 m/s, h = 1.6 km ⇒ dt ≲ 1.4 s;
        // the old dt = 0.02 s crept 70× slower than sound and never transported mass). Two phases:
        // light damping to move mass, then heavier damping to ring down.
        let g_vec = glam::DVec3::new(0.0, -g, 0.0);
        let (s1, s2) = if cfg!(debug_assertions) {
            (3_000, 1_000)
        } else {
            (8_000, 2_000)
        };
        for _ in 0..s1 {
            field.relax_step(g_vec, 0.4, 0.999);
        }
        for _ in 0..s2 {
            field.relax_step(g_vec, 0.4, 0.99);
        }
        field.compute_density();
        // Pointwise hydrostatics at two heights (mass quantiles 1/8 and 1/2, both off the wall):
        // kernel pressure P(y) = ρ(y)·R_s·T must equal the weight per area of everything above y.
        let mut ys: Vec<f64> = field.pos.iter().map(|p| p.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let band = |y0: f64| -> f64 {
            let (mut r, mut n) = (0.0, 0.0f64);
            for i in 0..field.pos.len() {
                if (field.pos[i].y - y0).abs() < 0.25 * field.h {
                    r += field.rho[i];
                    n += 1.0;
                }
            }
            r / n.max(1.0)
        };
        let check = |label: &str, y0: f64| -> (f64, f64) {
            let p_meas = band(y0) * rs_t;
            let above = field.pos.iter().filter(|p| p.y > y0).count() as f64;
            let p_expect = above * m_parcel * g / area;
            println!("3D hydrostatics {label}: P {p_meas:.0} vs weight-above {p_expect:.0} Pa");
            (p_meas, p_expect)
        };
        let (p1, e1) = check("near-base", ys[n_col as usize / 8].max(0.3 * field.h));
        let (p2, e2) = check("mid-column", ys[n_col as usize / 2]);
        // The field must be genuinely SETTLED — self-supported, not falling: if the kernel pressure
        // were truly deficient the column would still be accelerating downward. It is static.
        let v_max = field.vel.iter().map(|v| v.length()).fold(0.0f64, f64::max);
        assert!(
            v_max < 5.0,
            "the field is static — self-supported equilibrium (max |v| {v_max:.2} m/s)"
        );
        // Continuum bookkeeping matches within the OPERATOR'S truncation error at this resolution
        // (N=162, h/H ≈ 0.19 ⇒ ~20–35% observed; documented, resolution-convergent — the standard SPH
        // claim, and the neighbour-grid refinement will let us verify convergence at larger N). This is
        // a quantified discretization error, not a physics gap: the 1D column proves the physics at
        // 0.2%; this test proves the 3D machinery (normalization, symmetry, boundaries, self-support).
        assert!(
            (p1 - e1).abs() / e1 < 0.35,
            "near-base hydrostatic balance within operator error ({p1:.0} vs {e1:.0} Pa)"
        );
        assert!(
            (p2 - e2).abs() / e2 < 0.35,
            "mid-column hydrostatic balance within operator error ({p2:.0} vs {e2:.0} Pa)"
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
        let mut agg = Aggregate::new(parcels, 0.1).with_contact(contact, mass);
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

#[cfg(test)]
mod sky_tests {
    use super::*;

    /// The day/night line must be a GRADIENT the width of the atmosphere, not a knife edge — the thing
    /// you actually see from orbit. Before this, the veil returned black the instant the Sun dropped
    /// below a point's own horizon, so Earth from space had a hard-edged terminator no real photograph
    /// shows.
    #[test]
    fn the_terminator_is_soft_and_its_width_is_the_atmospheres_own_geometry() {
        let tau = rayleigh_tau(1.0);
        let mats = crate::materials::load();
        let air = &mats[crate::materials::index_of(&mats, "air")];
        let e = crate::planet::earth();
        let g = e.gravity_at(e.radius());
        let h = scale_height(air, 288.0, g);
        let w = twilight_half_angle(h, e.radius());

        // Earth's twilight wedge is a few degrees — set by sqrt(2H/R), nothing else.
        let deg = w.to_degrees();
        assert!(
            (2.0..4.0).contains(&deg),
            "Earth's twilight wedge ≈ 3°, got {deg:.2}°"
        );

        // Just PAST the geometric terminator the sky is still lit (the air above is in sunlight)...
        let past = rayleigh_veil(0.5, -0.3 * w, 0.5, tau, SUN_GAIN as f64, w);
        assert!(
            past[2] > 0.0,
            "just past the terminator must still scatter, got {past:?}"
        );
        // ...and it fades MONOTONICALLY to black by the far edge of the wedge.
        let deeper = rayleigh_veil(0.5, -0.8 * w, 0.5, tau, SUN_GAIN as f64, w);
        assert!(
            deeper[2] < past[2],
            "deeper into night must be dimmer ({deeper:?} vs {past:?})"
        );
        let night = rayleigh_veil(0.5, -1.5 * w, 0.5, tau, SUN_GAIN as f64, w);
        assert_eq!(
            night, [0.0; 3],
            "well past the wedge is honestly black, got {night:?}"
        );

        // The DAY side is untouched by any of this — the same value with or without twilight.
        let day_hard = rayleigh_veil(0.7, 0.6, 0.5, tau, SUN_GAIN as f64, 0.0);
        let day_soft = rayleigh_veil(0.7, 0.6, 0.5, tau, SUN_GAIN as f64, w);
        assert_eq!(
            day_hard, day_soft,
            "twilight must not change the lit hemisphere"
        );

        // AIRLESS: no atmosphere, no twilight, hard edge — the Moon, with no special case in the code.
        assert_eq!(
            twilight_half_angle(0.0, e.radius()),
            0.0,
            "no air ⇒ no twilight"
        );
        let moon_night = rayleigh_veil(0.5, -0.001, 0.5, rayleigh_tau(0.0), SUN_GAIN as f64, 0.0);
        assert_eq!(
            moon_night, [0.0; 3],
            "airless body keeps its knife-edge terminator"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────────────────────────
    // THE MARCHED INTEGRAL (docs/66) — the one law the closed form above is a special case of.
    // ─────────────────────────────────────────────────────────────────────────────────────────────

    /// Earth's real air, as the integral wants it: optical depth from the emergent surface pressure,
    /// scale height from the air's own molar mass under Earth's own gravity. Nothing here is chosen.
    fn earths_air() -> AirColumn {
        let mats = crate::materials::load();
        let air = &mats[crate::materials::index_of(&mats, "air")];
        let e = crate::planet::earth();
        let g = e.gravity_at(e.radius());
        AirColumn {
            tau: rayleigh_tau(e.surface_pressure() / 101_325.0),
            scale_height: scale_height(air, 288.0, g),
            radius: e.radius(),
        }
    }

    /// ★★ **The march IS `rayleigh_veil`, where `rayleigh_veil` is right.** The closed form is the
    /// analytic solution for one geometry — a slab seen from outside, looking down — and in that
    /// geometry the sun path and the view path shorten together, which is the only reason the integral
    /// collapses. So the march must reproduce it there to the discretisation error and no worse.
    ///
    /// This is the pin that makes them ONE law rather than two (Law II). It is run in the flat limit
    /// (R ≫ H) because that is the closed form's own assumption; the next test measures what the
    /// sphericity is worth on the real Earth, rather than leaving it unquantified.
    #[test]
    fn the_march_reproduces_the_closed_form_from_above() {
        let h = 8_400.0;
        let flat = AirColumn {
            tau: rayleigh_tau(1.0),
            scale_height: h,
            radius: 1.0e6 * h, // R/H = 1e6: plane-parallel to well beyond the tolerance below
        };
        // Eye above the whole column, looking DOWN at the ground — the closed form's geometry.
        let eye = flat.top() * 1.5;
        for &(mu_v, mu_s) in &[(1.0, 1.0), (0.9, 0.7), (0.7, 0.9), (0.5, 0.5), (0.35, 0.8)] {
            let cos_theta = 0.5;
            let marched = air_inscatter(
                &flat,
                eye,
                -mu_v, // the ray travels downward; the closed form's µ is the upward view
                mu_s,
                cos_theta,
                f64::INFINITY,
                SUN_GAIN as f64,
                256,
                64,
            );
            let closed = rayleigh_veil(mu_v, mu_s, cos_theta, flat.tau, SUN_GAIN as f64, 0.0);
            for b in 0..3 {
                let rel = (marched.inscatter[b] - closed[b]).abs() / closed[b].max(1e-9);
                assert!(
                    rel < 0.01,
                    "band {b} at µv={mu_v} µs={mu_s}: marched {} vs closed form {} ({:.2}% apart)",
                    marched.inscatter[b],
                    closed[b],
                    rel * 100.0
                );
            }
        }
    }

    /// What the closed form's flat-slab assumption is WORTH on the real Earth — measured, so the
    /// number is inherited instead of re-argued. It is small looking down (the planet is nearly flat
    /// over one scale height) and it is the whole story at grazing angles, which is where the
    /// closed form has to cap `µ` at 0.08 to avoid dividing by zero and the march simply does not.
    #[test]
    fn sphericity_is_small_overhead_and_decisive_at_the_limb() {
        let air = earths_air();
        let eye = air.top() * 2.0;
        let overhead = air_inscatter(
            &air,
            eye,
            -0.9,
            0.9,
            0.5,
            f64::INFINITY,
            SUN_GAIN as f64,
            256,
            64,
        );
        let closed = rayleigh_veil(0.9, 0.9, 0.5, air.tau, SUN_GAIN as f64, 0.0);
        let rel = ((overhead.inscatter[2] - closed[2]) / closed[2]).abs();
        assert!(
            rel < 0.05,
            "looking straight down the round Earth agrees with the flat one ({:.1}%)",
            rel * 100.0
        );

        // A ray that MISSES the surface but crosses the air — the limb. The closed form has no such
        // ray at all: it is a slab, so every ray ends on the ground. This is the blue rim that stands
        // OUTSIDE the planet's silhouette, and it cannot be drawn by any function of surface cosines.
        // Closest approach of a ray leaving radius `r_eye` at zenith cosine µ is `r_eye·sin(θ)`, so
        // aiming its perigee half a scale height above the ground fixes µ exactly.
        let r_eye = air.radius + eye;
        let perigee = (air.radius + 0.5 * air.scale_height) / r_eye;
        let mu_limb = -(1.0 - perigee * perigee).sqrt();
        let limb = air_inscatter(
            &air,
            eye,
            mu_limb,
            0.9,
            0.5,
            f64::INFINITY,
            SUN_GAIN as f64,
            256,
            64,
        );
        assert!(
            limb.inscatter[2] > 0.0,
            "a ray that misses the ground still passes through lit air, got {limb:?}"
        );
    }

    /// **Blue overhead, pale at the horizon — and neither is painted.** Overhead the path is one scale
    /// height and only the λ⁻⁴ band scatters appreciably, so blue wins by a wide margin. Along the
    /// horizon the path is hundreds of kilometres, every band saturates at `1 − e^{−τ}` → 1, and the
    /// colour washes out. The ratio between them is the whole of "why the sky is paler near the ground".
    #[test]
    fn the_sky_is_blue_overhead_and_pale_at_the_horizon() {
        let air = earths_air();
        let up = air_inscatter(
            &air,
            2.0,
            1.0,
            0.9,
            0.9,
            f64::INFINITY,
            SUN_GAIN as f64,
            64,
            16,
        );
        let horizon = air_inscatter(
            &air,
            2.0,
            0.02,
            0.9,
            0.1,
            f64::INFINITY,
            SUN_GAIN as f64,
            64,
            16,
        );
        let blueness = |s: &Scattered| s.inscatter[2] / s.inscatter[0].max(1e-9);
        assert!(
            blueness(&up) > 3.0,
            "the zenith is strongly blue, got B/R {:.2} ({:?})",
            blueness(&up),
            up.inscatter
        );
        assert!(
            blueness(&horizon) < blueness(&up),
            "the horizon washes out relative to the zenith ({:.2} vs {:.2})",
            blueness(&horizon),
            blueness(&up)
        );
        assert!(
            horizon.inscatter[0] > up.inscatter[0],
            "the long horizon path scatters MORE red than the short zenith one ({:?} vs {:?})",
            horizon.inscatter,
            up.inscatter
        );
    }

    /// **Sunset is the blue being removed, not the red being added.** With the Sun on the horizon its
    /// light crosses the whole atmosphere before it reaches the air above the observer, so the blue is
    /// gone before it can scatter and what is left to scatter is red. Nothing in the code knows the
    /// word "sunset": the same integral with a different `µs` produces it.
    #[test]
    fn a_setting_sun_reddens_the_sky_it_lights() {
        let air = earths_air();
        let toward = 0.15; // looking just above the horizon
        let noon = air_inscatter(
            &air,
            2.0,
            toward,
            0.95,
            0.3,
            f64::INFINITY,
            SUN_GAIN as f64,
            64,
            16,
        );
        let setting = air_inscatter(
            &air,
            2.0,
            toward,
            0.02,
            0.99,
            f64::INFINITY,
            SUN_GAIN as f64,
            64,
            16,
        );
        let redness = |s: &Scattered| s.inscatter[0] / s.inscatter[2].max(1e-9);
        assert!(
            redness(&setting) > 3.0 * redness(&noon),
            "the setting sky is far redder than the noon one (R/B {:.3} vs {:.3})",
            redness(&setting),
            redness(&noon)
        );
    }

    /// ★★ **Twilight EMERGES, and this retires a declared number.** `twilight_half_angle` is an
    /// openly-flagged stand-in: a `sqrt(2H/R)` ramp applied to the closed form because a flat slab has
    /// no geometry that could produce twilight. The march has that geometry — the low air is inside
    /// the planet's shadow while the air above it is still in sunlight — so the gradient falls out of
    /// the shadow test and nothing declares its width.
    ///
    /// The assertions are geometric facts, not remembered values: a point at altitude `h` leaves the
    /// shadow when the Sun's depression is `θ ≈ sqrt(2h/R)`, so the sky must be dark once the depression
    /// exceeds that angle for the TOP of the column, and lit below it.
    #[test]
    fn twilight_emerges_from_the_planets_own_shadow() {
        let air = earths_air();
        // The depression at which the shadow swallows the whole column — pure geometry, no constant.
        let full_dark = (2.0 * air.top() / air.radius).sqrt();
        let sky = |depression: f64| {
            air_inscatter(
                &air,
                2.0,
                0.4,
                -depression.sin(),
                0.6,
                f64::INFINITY,
                SUN_GAIN as f64,
                96,
                24,
            )
            .inscatter[2]
        };
        let just_set = sky(0.2 * full_dark);
        let deeper = sky(0.6 * full_dark);
        assert!(
            just_set > 0.0,
            "the air above a set Sun is still lit, got {just_set}"
        );
        assert!(
            deeper < just_set,
            "twilight fades monotonically into night ({deeper} then {just_set})"
        );
        assert_eq!(
            sky(1.4 * full_dark),
            0.0,
            "once the shadow clears the column the sky is honestly black"
        );
        // And the emergent width is the same scale the declared ramp was standing in for — which is
        // why the ramp was a good stand-in and why it is no longer needed.
        let declared = twilight_half_angle(air.scale_height, air.radius);
        assert!(
            sky(0.5 * declared) > sky(2.0 * declared),
            "the emergent gradient spans the sqrt(2H/R) scale the declared ramp used"
        );
    }

    /// **Air that is not there scatters nothing** — no branch, no epsilon, no faintly-blue vacuum. The
    /// Moon gets a black sky by declaring no atmosphere, which is the only reason it should.
    #[test]
    fn a_world_with_no_air_has_no_sky() {
        let vacuum = AirColumn {
            tau: rayleigh_tau(0.0),
            scale_height: 0.0,
            radius: 1_737_400.0,
        };
        let s = air_inscatter(
            &vacuum,
            2.0,
            0.5,
            0.5,
            0.5,
            f64::INFINITY,
            SUN_GAIN as f64,
            64,
            16,
        );
        assert_eq!(s, Scattered::CLEAR, "vacuum adds nothing and hides nothing");
    }

    /// **The integral converges, and this is what fixes the sample counts the shader ships with.** A
    /// number of steps is a resolution, not a dial: the test measures where the answer stops moving and
    /// the renderer spends that, so "cheap enough to draw" is a measured claim.
    #[test]
    fn the_integral_converges_with_sample_count() {
        let air = earths_air();
        // The hardest case for the march: a near-horizontal ray at low sun, where the path is longest
        // and the density varies most along it.
        let at = |v: usize, s: usize| {
            air_inscatter(
                &air,
                2.0,
                0.05,
                0.05,
                0.7,
                f64::INFINITY,
                SUN_GAIN as f64,
                v,
                s,
            )
            .inscatter
        };
        let reference = at(512, 128);
        let err = |v: usize, s: usize| {
            let got = at(v, s);
            let worst = (0..3)
                .map(|b| (got[b] - reference[b]).abs() / reference[b].max(1e-9))
                .fold(0.0f32, f32::max);
            println!(
                "air_inscatter {v}x{s}: {got:?} vs {reference:?} — {:.2}%",
                worst * 100.0
            );
            worst
        };
        // What ships, and why it is that and not less.
        let shipped = err(SKY_VIEW_STEPS, SKY_SUN_STEPS);
        assert!(
            shipped < 0.02,
            "the shipped {SKY_VIEW_STEPS}x{SKY_SUN_STEPS} is within 2% on the worst ray ({:.2}%)",
            shipped * 100.0
        );
        let halved = err(SKY_VIEW_STEPS / 2, SKY_SUN_STEPS / 2);
        assert!(
            halved > 2.0 * shipped,
            "halving the resolution must visibly cost accuracy, or the shipped count is wasted \
             ({:.2}% vs {:.2}%)",
            halved * 100.0,
            shipped * 100.0
        );
        // MONOTONE in resolution — the property that makes this a resolution and not a fudge.
        assert!(
            err(64, 16) < shipped,
            "spending more samples must improve the answer"
        );
    }

    /// ★ **The air between the eye and the grass in front of it is the air between them** — which
    /// sounds like nothing and replaces a flagged stand-in. `veil_column_fraction` scaled the FULL
    /// column's in-scatter by `1 − e^{−h/H}` because the closed form had no way to stop early; it was
    /// right vertically and wrong along the ground, so distant terrain got no aerial perspective at
    /// all. Ending the integral at the surface point is the computation it was standing in for.
    #[test]
    fn a_ray_that_stops_early_only_crosses_the_air_it_crossed() {
        let air = earths_air();
        let sky = |t_end: f64| {
            air_inscatter(&air, 1.7, 0.02, 0.7, 0.5, t_end, SUN_GAIN as f64, 64, 16).inscatter[2]
        };
        let open = sky(f64::INFINITY);
        let near = sky(2.0); // grass two metres away
        let far = sky(30_000.0); // a mountain thirty kilometres off
        assert!(
            near / open < 1.0e-3,
            "two metres of air is not a sky ({near} vs {open})"
        );
        assert!(
            far > 100.0 * near && far < open,
            "thirty kilometres of it is real haze, short of the whole sky ({near} / {far} / {open})"
        );
    }
}

#[cfg(test)]
mod assembly_boundary_tests {
    use super::*;

    /// ★★ **AN ASSEMBLY ENDS AT ITS OUTERMOST COMPONENT** — which for Earth today is its air (docs/66
    /// §10). Robin's wording, correcting mine: *"'A body ends where its AIR ends' is not accurate, as
    /// bodies are assemblies. An assembly ends at the outermost boundary of the assembly."*
    ///
    /// The difference is not decorative: it is the ~97 km of air a limb ray crosses, the reason the
    /// terminator is a gradient rather than an edge, and the reason anything scanning for "the edge of
    /// the planet" finds it in the wrong place if it looks for the rock.
    #[test]
    fn an_assembly_reaches_past_its_core_by_its_outermost_component() {
        let mats = crate::materials::load();
        let e = crate::planet::earth();
        let air = AirColumn::of_body(&e, &mats, 288.0);

        assert!(
            air.outer_reach() > air.radius,
            "Earth's assembly reaches past its surface — air is a component of it"
        );
        let km = (air.outer_reach() - air.radius) / 1000.0;
        assert!(
            (80.0..120.0).contains(&km),
            "the air adds ~97 km of assembly (11.5 scale heights), got {km:.0} km"
        );

        // ★ AND THE MOON DOES NOT. Same code, same call, no branch: Luna is its own assembly, it
        // declares no atmosphere, so its boundary IS its surface and its terminator is a knife edge.
        let moon = crate::planet::body("moon");
        let vacuum = AirColumn::of_body(&moon, &mats, 288.0);
        assert_eq!(
            vacuum.outer_reach(),
            vacuum.radius,
            "an assembly whose outermost component is its rock reports its rock — no special case"
        );

        // From orbit the assembly subtends more than the rock does — which is the limb.
        let alt = 400_000.0;
        let with_air = air.angular_reach_from(alt);
        let rock_only = (air.radius / (air.radius + alt)).asin();
        assert!(
            with_air > rock_only,
            "the air stands outside the silhouette ({:.4} rad vs {:.4})",
            with_air,
            rock_only
        );
        // Standing inside it, the assembly fills the sky.
        assert_eq!(
            air.angular_reach_from(1.7),
            std::f64::consts::FRAC_PI_2,
            "an observer inside the air is inside the assembly"
        );
    }
}

#[cfg(test)]
mod sky_irradiance_tests {
    use super::*;

    fn earth_air() -> AirColumn {
        let mats = crate::materials::load();
        AirColumn::of_body(&crate::planet::earth(), &mats, 288.0)
    }

    /// ★★ **IT IS A QUADRATURE, SO IT MUST CONVERGE** — the property that separates a computed
    /// integral from an ambient constant (Law V). Refining the hemisphere grid must stop changing the
    /// answer; if it did not, the number would be a property of the sampling rather than of the sky.
    #[test]
    fn sky_irradiance_converges_as_the_hemisphere_is_refined() {
        let air = earth_air();
        // Standing at Galway's latitude with the sun 52° up, looking at a VERTICAL surface — the grass
        // blade that started this (docs/46 row 56).
        let sun_up = (52.4f64).to_radians().sin();
        let coarse = sky_irradiance(&air, 1.0, 0.0, sun_up, 0.5, SUN_GAIN as f64, 4, 8);
        let fine = sky_irradiance(&air, 1.0, 0.0, sun_up, 0.5, SUN_GAIN as f64, 8, 16);
        let finer = sky_irradiance(&air, 1.0, 0.0, sun_up, 0.5, SUN_GAIN as f64, 16, 32);
        let rel = |a: [f64; 3], b: [f64; 3]| {
            (0..3)
                .map(|i| (a[i] - b[i]).abs() / b[i].max(1e-12))
                .fold(0.0f64, f64::max)
        };
        let step1 = rel(coarse, fine);
        let step2 = rel(fine, finer);
        println!("sky irradiance: 4x8 {coarse:?}\n                8x16 {fine:?}\n                16x32 {finer:?}\n  steps {step1:.4} then {step2:.4}");
        assert!(
            step2 < step1,
            "refining must CONVERGE: first step {step1:.4}, second {step2:.4}"
        );
        assert!(
            step2 < 0.05,
            "the shipped grid must already be within 5% of a refined one, got {step2:.4}"
        );
    }

    /// ★★★ **THE DEFECT THIS EXISTS FOR.** A vertical surface at noon receives almost no DIRECT sun —
    /// `max(n·l, 0)` is near zero for a blade standing up under a high sun — and yet a lawn is not
    /// black at midday. The sky is what lights it. So the irradiance on a vertical surface must be a
    /// substantial fraction of what the ground beneath it receives, and it must be BLUE, because that
    /// is what Rayleigh scattering makes of sunlight.
    #[test]
    fn a_vertical_surface_at_noon_is_lit_by_the_sky_and_the_light_is_blue() {
        let air = earth_air();
        let sun_up = (52.4f64).to_radians().sin(); // Galway, local noon
        let up_facing = sky_irradiance(&air, 1.0, 1.0, sun_up, sun_up, SUN_GAIN as f64, 8, 16);
        let vertical = sky_irradiance(&air, 1.0, 0.0, sun_up, 0.5, SUN_GAIN as f64, 8, 16);

        assert!(up_facing[2] > 0.0, "the sky lights an upward face at all");
        // A vertical face sees half the hemisphere, so it gets a real share — not a rounding error.
        let share = vertical[2] / up_facing[2];
        println!("vertical/up-facing sky irradiance = {share:.3} (blue band)");
        assert!(
            (0.25..0.85).contains(&share),
            "a vertical surface sees about half the sky, got {share:.3} of the upward face"
        );
        // And it is BLUE: Rayleigh goes as 1/λ⁴, so the blue band dominates the red.
        assert!(
            vertical[2] > vertical[0] * 1.5,
            "skylight is blue — got r={:.4} b={:.4}",
            vertical[0],
            vertical[2]
        );
    }

    /// A body with no air lights nothing, with no branch anywhere — the same negative control the sky
    /// itself is held to (`worlds/earth-airless`). If this ever returns light, an ambient constant has
    /// crept in.
    #[test]
    fn no_air_means_no_skylight() {
        let airless = AirColumn {
            tau: [0.0; 3],
            scale_height: 0.0,
            radius: 6_371_000.0,
        };
        let e = sky_irradiance(&airless, 1.0, 1.0, 0.8, 0.8, SUN_GAIN as f64, 8, 16);
        assert_eq!(e, [0.0; 3], "vacuum scatters nothing onto anything");
    }
}
