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
    mechanical: RawMechanical,
    optical: RawOptical,
}

#[derive(Deserialize)]
struct RawMechanical {
    /// kg/m^3. Present for every material in the seed database.
    density: f32,
}

#[derive(Deserialize)]
struct RawOptical {
    /// Linear RGB, each 0..1.
    albedo: [f32; 3],
}

/// A material as the engine consumes it in Phase 1.
#[derive(Clone, Debug)]
pub struct Material {
    pub id: String,
    /// kg/m^3. Stored now as the authoritative per-material mass; Phase 2 uses it for self-gravity
    /// (voxel mass = density * volume). Not read yet in Phase 1's renderer.
    #[allow(dead_code)]
    pub density: f32,
    pub albedo: [f32; 3],
}

/// Parse the embedded database. Panics with a clear message if the bundled JSON is malformed
/// (that would be a build-time data error, surfaced immediately in the console).
pub fn load() -> Vec<Material> {
    let file: RawFile =
        serde_json::from_str(MATERIALS_JSON).expect("bundled data/materials.json is invalid JSON");
    file.materials
        .into_iter()
        .map(|m| Material {
            id: m.id,
            density: m.mechanical.density,
            albedo: m.optical.albedo,
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
