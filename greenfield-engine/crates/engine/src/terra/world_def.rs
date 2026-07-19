//! docs/43 — the "world" schema: a scene defined as DATA (JSON) that the engine loads and renders. This is the
//! reusable contract (docs/43 "initial conditions + a few dials"). v1 (terrain) consumes `planet`, `surface`,
//! `atmosphere`, `camera`; the rest is parsed-and-carried so the schema generalizes to other worlds later.
//! Optional fields default so a minimal world (`{name, planet:{radius_m}}`) still loads.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct World {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    pub planet: Planet,
    #[serde(default)]
    pub surface: Option<Surface>,
    #[serde(default)]
    pub atmosphere: Option<Atmosphere>,
    #[serde(default)]
    pub camera: Option<CameraDef>,
    #[serde(default)]
    pub time: Option<TimeDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Planet {
    pub radius_m: f64,
    #[serde(default)]
    pub mass_kg: Option<f64>,
    #[serde(default)]
    pub rotation_period_s: Option<f64>,
    /// A named layered profile (e.g. "earth") → `planet::earth()` defaults, so Earth stays declared, not fudged.
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Surface {
    #[serde(default)]
    pub landmask_url: Option<String>,
    #[serde(default)]
    pub elevation_url: Option<String>,
    /// [min, max] metres the elevation raster decodes to (incl. bathymetry, e.g. [-11000, 9000]).
    #[serde(default)]
    pub elevation_range_m: Option<[f64; 2]>,
    #[serde(default)]
    pub landcover_url: Option<String>,
    #[serde(default)]
    pub sea_level_m: f64,
    /// biome index (as a string key) → material id in `data/materials.json`.
    #[serde(default)]
    pub biomes: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Atmosphere {
    #[serde(default)]
    pub profile: Option<String>, // "rayleigh"
    #[serde(default)]
    pub surface_pressure_pa: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CameraDef {
    #[serde(default)]
    pub mode: Option<String>, // "fly" (v1) | "orbit"
    #[serde(default)]
    pub lat: f64,
    #[serde(default)]
    pub lon: f64,
    #[serde(default)]
    pub alt_m: f64,
    #[serde(default)]
    pub look: Option<Look>,
    #[serde(default)]
    pub min_alt_m: Option<f64>,
    #[serde(default)]
    pub max_alt_m: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Look {
    #[serde(default)]
    pub yaw: f64,
    #[serde(default)]
    pub pitch: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimeDef {
    #[serde(default)]
    pub rotation: bool,
    #[serde(default = "one")]
    pub scale: f64,
}
fn one() -> f64 {
    1.0
}

impl World {
    /// Parse a world JSON string; a clear error string on failure (surfaced to the JS host).
    pub fn parse(json: &str) -> Result<World, String> {
        serde_json::from_str(json).map_err(|e| format!("world JSON parse error: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_and_a_full_earth_world() {
        // Minimal: just a named planet with a radius.
        let w = World::parse(r#"{"name":"Bare","planet":{"radius_m":6371000}}"#).unwrap();
        assert_eq!(w.name, "Bare");
        assert_eq!(w.planet.radius_m, 6_371_000.0);
        assert!(w.surface.is_none());

        // Full-ish Earth world (the reference).
        let json = r#"{
            "name":"Earth","type":"planet",
            "planet":{"radius_m":6371000,"mass_kg":5.972e24,"profile":"earth"},
            "surface":{"landmask_url":"landmask.png","elevation_url":"elevation.png",
                "elevation_range_m":[-11000,9000],"landcover_url":"landcover.png","sea_level_m":0,
                "biomes":{"0":"water","1":"grass","2":"sand"}},
            "atmosphere":{"profile":"rayleigh","surface_pressure_pa":101325},
            "camera":{"mode":"fly","lat":20,"lon":0,"alt_m":8000000,"look":{"yaw":0,"pitch":-1.2},
                "min_alt_m":2,"max_alt_m":40000000},
            "time":{"rotation":false,"scale":1}
        }"#;
        let w = World::parse(json).unwrap();
        assert_eq!(w.name, "Earth");
        assert_eq!(w.planet.profile.as_deref(), Some("earth"));
        let s = w.surface.unwrap();
        assert_eq!(s.elevation_range_m, Some([-11000.0, 9000.0]));
        assert_eq!(s.biomes.get("1").map(String::as_str), Some("grass"));
        assert_eq!(w.camera.unwrap().mode.as_deref(), Some("fly"));
    }
}
