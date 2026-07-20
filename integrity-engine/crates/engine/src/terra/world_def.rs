//! docs/43 — the "world" schema: a scene defined as DATA (JSON) that the engine loads and renders. This is the
//! reusable contract (docs/43 "initial conditions + a few dials"). Two `type`s exist so far: a `"planet"` world
//! (terrain — `planet`, `surface`, `atmosphere`, a fly `camera`; consumed by `Terra`) and a `"system"` world
//! (an N-body space scene — a `bodies[]` array with orbital initial conditions, an orbit `camera`; consumed by
//! `OrbitDemo`). Optional fields default so a minimal world (`{name, planet:{radius_m}}` or `{name, bodies:[…]}`)
//! still loads. The renderer picks physics/laws by type; the file only declares initial conditions + a few dials.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct World {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    /// The single planet (a `"planet"`/terrain world). Absent for a `"system"` world.
    #[serde(default)]
    pub planet: Option<Planet>,
    /// The N-body cast (a `"system"`/space world): star + planet + moon(s), each with orbital initial conditions.
    #[serde(default)]
    pub bodies: Option<Vec<BodyDef>>,
    #[serde(default)]
    pub surface: Option<Surface>,
    #[serde(default)]
    pub atmosphere: Option<Atmosphere>,
    #[serde(default)]
    pub camera: Option<CameraDef>,
    #[serde(default)]
    pub time: Option<TimeDef>,
}

/// One body in a `"system"` world — the declared initial conditions the N-body integrator (`orbit`) evolves.
/// Mass/radius/tint may come from a named `profile` ("sun"/"earth"/"moon" → `planet::` + composition) so the
/// bodies stay *declared, not fudged*; explicit `mass_kg`/`radius_m`/`tint` override.
#[derive(Debug, Clone, Deserialize)]
pub struct BodyDef {
    pub name: String,
    /// "star" (holds + lights the system, not drawn) | "planet" (the focus / impact target) | "moon" (deorbits).
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub mass_kg: Option<f64>,
    #[serde(default)]
    pub radius_m: Option<f64>,
    #[serde(default)]
    pub profile: Option<String>,
    /// Position (metres) in the shared inertial frame.
    #[serde(default)]
    pub pos_m: [f64; 3],
    /// Velocity (metres/second) in the same frame.
    #[serde(default)]
    pub vel_ms: [f64; 3],
    /// Rotation period (s) about +Z → the body's spin angular momentum (the planet's day). None = no spin.
    #[serde(default)]
    pub spin_period_s: Option<f64>,
    /// Optional linear-RGB tint override (else derived from the profile's composition).
    #[serde(default)]
    pub tint: Option<[f32; 3]>,
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
    /// Declared relief exaggeration (a visualization dial, honest — not a physics fudge). 1.0 = true scale. The
    /// globe mesh, ground cap, and camera floor all use it consistently so they stay one surface.
    #[serde(default)]
    pub relief_exaggeration: Option<f64>,
    #[serde(default)]
    pub landcover_url: Option<String>,
    #[serde(default)]
    pub sea_level_m: f64,
    /// biome index (as a string key) → material id in `data/materials.json`.
    #[serde(default)]
    pub biomes: HashMap<String, String>,
}

/// A world's atmosphere, DECLARED AS MATTER — its mass and what it is made of.
///
/// **Surface pressure is deliberately not a field here.** `planet::LayeredBody::atmosphere_mass` states
/// the invariant: *"The surface pressure is never declared: it EMERGES as the weight of this column,
/// P = M·g/(4πR²)."* This schema previously carried `surface_pressure_pa`, and Earth's world file
/// declared `101325` while the emergent value is `99,049 Pa` — so Terra rendered a 2.2%-different
/// atmosphere from the terrain and orbit scenes, which read the emergent one. One physical quantity,
/// two answers, differing per scene (docs/46). Declaring MASS instead makes that impossible: there is
/// one source and pressure is computed from it.
///
/// This is also what makes other worlds data rather than code — Mars is CO₂ at a smaller mass, the Moon
/// is `mass_kg: 0.0` and provably airless (its zero-drag case is already tested).
#[derive(Debug, Clone, Deserialize)]
pub struct Atmosphere {
    #[serde(default)]
    pub profile: Option<String>, // "rayleigh"
    /// Total atmosphere mass (kg) — the DECLARED quantity. Earth: 5.15e18 (measured). `0.0` = airless.
    #[serde(default)]
    pub mass_kg: Option<f64>,
    /// What the air is made of: material ids from the DB with mass fractions, e.g.
    /// `[["air", 1.0]]` for Earth. Absent ⇒ Earth air. Mars would be `[["co2", 0.95], …]` once those
    /// materials exist — the specific gas constant, and hence the scale height, then follow from the
    /// composition rather than from a constant.
    #[serde(default)]
    pub composition: Option<Vec<(String, f64)>>,
}

impl Atmosphere {
    /// Surface pressure (Pa) DERIVED from the declared mass: the weight of the column over the planet's
    /// area, `P = M·g/(4πR²)`. Returns `None` when no mass is declared, so the caller falls back to the
    /// planet profile's own atmosphere rather than inventing a number.
    pub fn surface_pressure(&self, radius_m: f64, g: f64) -> Option<f64> {
        let m = self.mass_kg?;
        if radius_m <= 0.0 {
            return Some(0.0);
        }
        Some(m * g / (4.0 * std::f64::consts::PI * radius_m * radius_m))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CameraDef {
    #[serde(default)]
    pub mode: Option<String>, // "fly" (terrain) | "orbit" (space)
    // --- fly camera (terrain) ---
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
    // --- orbit camera (space): yaw/pitch/zoom around a focus body (frame of reference) ---
    #[serde(default)]
    pub yaw: Option<f64>,
    #[serde(default)]
    pub pitch: Option<f64>,
    #[serde(default)]
    pub zoom: Option<f64>,
    /// Name of the body the orbit camera centres on (its frame of reference). Defaults to the "planet" body.
    #[serde(default)]
    pub focus: Option<String>,
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
mod atmosphere_source_tests {
    use super::*;

    /// ONE EARTH, ONE ATMOSPHERE. The shipped Earth world must derive the SAME surface pressure the
    /// planet profile computes — because both now weigh the same declared air mass, rather than one
    /// reading a literal. Before this, `world.json` declared 101,325 Pa against an emergent 99,049 Pa,
    /// so Terra's sky was a 2.2%-different atmosphere from the terrain and orbit scenes (docs/46).
    #[test]
    fn the_world_file_and_the_planet_profile_agree_on_earths_air() {
        let json = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/worlds/earth/world.json"),
        )
        .expect("shipped Earth world");
        let w = World::parse(&json).expect("Earth world parses");
        let planet = w.planet.as_ref().expect("Earth world has a planet");
        let atm = w.atmosphere.as_ref().expect("Earth world declares an atmosphere");

        let earth = crate::planet::earth();
        let g = earth.gravity_at(planet.radius_m);
        let from_world = atm.surface_pressure(planet.radius_m, g).expect("mass is declared");
        let from_profile = earth.surface_pressure();

        let rel = (from_world - from_profile).abs() / from_profile;
        assert!(
            rel < 0.02,
            "one Earth must have one atmosphere: world file says {from_world:.0} Pa, \
             planet profile says {from_profile:.0} Pa ({:.1}% apart)",
            rel * 100.0
        );
    }

    /// Pressure is DERIVED, never declared: the schema must not carry a surface-pressure field, or the
    /// two-source bug can walk straight back in. A compile-time guarantee would be better; this is the
    /// next best thing, and it names the invariant so a future edit trips over it.
    #[test]
    fn the_schema_does_not_let_a_world_declare_its_surface_pressure() {
        // Scope the search to the struct BODY. Searching the whole file matches this test's own source
        // (include_str! includes it), which is a false positive, not a violation.
        let src = include_str!("world_def.rs");
        let start = src.find("pub struct Atmosphere {").expect("Atmosphere struct");
        let body = &src[start..start + src[start..].find("\n}").expect("struct end")];
        let banned = concat!("surface_", "pressure_pa"); // split so this line is not itself a match
        assert!(
            !body.contains(banned),
            "the Atmosphere schema must not carry a declared surface pressure — declare mass instead, \
             so there is ONE source and pressure is derived from it"
        );
    }

    /// An AIRLESS world is expressible and gives exactly zero pressure — the Moon, and every vacuum body
    /// a Solar System Cup would add, are data rather than a code path.
    #[test]
    fn an_airless_world_is_expressible_and_gives_zero_pressure() {
        let w = World::parse(
            r#"{"name":"luna","planet":{"radius_m":1737400.0},"atmosphere":{"mass_kg":0.0}}"#,
        )
        .expect("airless world parses");
        let atm = w.atmosphere.as_ref().unwrap();
        assert_eq!(atm.surface_pressure(1_737_400.0, 1.62), Some(0.0), "no air ⇒ no pressure");
    }

    /// Composition is declarable, so the specific gas constant (hence scale height) can follow from what
    /// the air IS rather than from a constant — the hook a CO₂ world needs.
    #[test]
    fn composition_is_declarable() {
        let w = World::parse(
            r#"{"name":"m","planet":{"radius_m":3389500.0},
                "atmosphere":{"mass_kg":2.5e16,"composition":[["air",1.0]]}}"#,
        )
        .expect("world with composition parses");
        let c = w.atmosphere.as_ref().unwrap().composition.as_ref().expect("composition present");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, "air");
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
        assert_eq!(w.planet.as_ref().unwrap().radius_m, 6_371_000.0);
        assert!(w.surface.is_none());

        // Full-ish Earth world (the reference).
        let json = r#"{
            "name":"Earth","type":"planet",
            "planet":{"radius_m":6371000,"mass_kg":5.972e24,"profile":"earth"},
            "surface":{"landmask_url":"landmask.png","elevation_url":"elevation.png",
                "elevation_range_m":[-11000,9000],"landcover_url":"landcover.png","sea_level_m":0,
                "biomes":{"0":"water","1":"grass","2":"sand"}},
            "atmosphere":{"profile":"rayleigh","mass_kg":5.15e18,"composition":[["air",1.0]]},
            "camera":{"mode":"fly","lat":20,"lon":0,"alt_m":8000000,"look":{"yaw":0,"pitch":-1.2},
                "min_alt_m":2,"max_alt_m":40000000},
            "time":{"rotation":false,"scale":1}
        }"#;
        let w = World::parse(json).unwrap();
        assert_eq!(w.name, "Earth");
        assert_eq!(w.planet.as_ref().unwrap().profile.as_deref(), Some("earth"));
        let s = w.surface.unwrap();
        assert_eq!(s.elevation_range_m, Some([-11000.0, 9000.0]));
        assert_eq!(s.biomes.get("1").map(String::as_str), Some("grass"));
        assert_eq!(w.camera.unwrap().mode.as_deref(), Some("fly"));
    }

    #[test]
    fn parses_a_system_world_with_bodies() {
        // A "system" world: Sun + Earth + Moon with orbital initial conditions and an orbit camera.
        let json = r#"{
            "name":"Earth–Moon","type":"system",
            "bodies":[
                {"name":"Sun","role":"star","profile":"sun","pos_m":[0,0,0],"vel_ms":[0,0,0]},
                {"name":"Earth","role":"planet","profile":"earth","mass_kg":5.972e24,"radius_m":6371000,
                    "pos_m":[1.496e11,0,0],"vel_ms":[0,29780,0],"spin_period_s":86164},
                {"name":"Moon","role":"moon","profile":"moon","mass_kg":7.342e22,"radius_m":1737000,
                    "pos_m":[1.499844e11,0,0],"vel_ms":[0,30802,0]}
            ],
            "camera":{"mode":"orbit","yaw":0.6,"pitch":0.5,"zoom":1.0,"focus":"Earth"},
            "time":{"scale":118000}
        }"#;
        let w = World::parse(json).unwrap();
        assert_eq!(w.kind, "system");
        assert!(w.planet.is_none(), "a system world has no single planet");
        let bodies = w.bodies.as_ref().unwrap();
        assert_eq!(bodies.len(), 3);
        assert_eq!(bodies[0].role, "star");
        assert_eq!(bodies[1].name, "Earth");
        assert_eq!(bodies[1].pos_m, [1.496e11, 0.0, 0.0]);
        assert_eq!(bodies[1].spin_period_s, Some(86164.0));
        assert_eq!(bodies[2].role, "moon");
        let cam = w.camera.unwrap();
        assert_eq!(cam.mode.as_deref(), Some("orbit"));
        assert_eq!(cam.focus.as_deref(), Some("Earth"));
        assert_eq!(w.time.unwrap().scale, 118000.0);
    }
}
