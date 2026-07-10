//! Loads the cited material database (`data/materials.json`) that ships with the engine.
//!
//! Phase 1 only needs each material's **density** (the physical source of truth) and **albedo**
//! (for rendering). We deserialize just those fields; serde ignores the rest. Later phases will
//! read the full mechanical/optical property set (see `docs/04-materials-model.md`).

use serde::Deserialize;

/// The material database, embedded at compile time so the WASM is self-contained.
const MATERIALS_JSON: &str = include_str!("../../../data/materials.json");

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
}

#[derive(Deserialize)]
struct RawThermal {
    specific_heat: f32,       // J/(kg·K)
    melt_point: f32,          // K
    latent_fusion: f32,       // J/kg
    boil_point: f32,          // K
    latent_vaporization: f32, // J/kg
}

#[derive(Deserialize)]
struct RawMechanical {
    /// kg/m^3. Present for every material in the seed database.
    density: f32,
    /// Pa. Resistance to being pulled apart; null for liquids. Drives fracture (Phase 3).
    #[serde(default)]
    tensile_strength: Option<f32>,
    /// Pa. Fallback bonding strength where tensile isn't given.
    #[serde(default)]
    cohesion: Option<f32>,
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
}

/// Thermal properties — enough to compute the energy to melt or vaporize the material (`docs/20`).
/// Optional: only materials we've cited thermal data for carry it; without it, an impact can fracture
/// the material but we don't claim to know its melt/boil behaviour (honesty).
#[derive(Clone, Debug)]
pub struct Thermal {
    pub specific_heat: f32,       // J/(kg·K)
    pub melt_point: f32,          // K
    pub latent_fusion: f32,       // J/kg (solid → liquid)
    pub boil_point: f32,          // K
    pub latent_vaporization: f32, // J/kg (liquid → gas)
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
    /// 0 (mirror) .. 1 (matte). Drives specular highlight width (Phase 4).
    pub roughness: f32,
    /// 0 (dielectric) .. 1 (metal). Metals get a tinted, tighter highlight (sparkle).
    pub metallic: f32,
    /// 0 (uniform) .. 1 (high per-grain spread). Drives procedural texture contrast (Phase 4).
    pub color_variance: f32,
    /// Thermal properties for melt/vaporization (`docs/20`), when we have cited data for the material.
    pub thermal: Option<Thermal>,
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
                roughness: m.optical.roughness,
                metallic: m.optical.metallic,
                color_variance: m.optical.color_variance,
                thermal: m.thermal.map(|t| Thermal {
                    specific_heat: t.specific_heat,
                    melt_point: t.melt_point,
                    latent_fusion: t.latent_fusion,
                    boil_point: t.boil_point,
                    latent_vaporization: t.latent_vaporization,
                }),
            }
        })
        .collect()
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
