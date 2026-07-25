//! **The Laws, made checkable** (`docs/00`).
//!
//! The Laws are the engine's compass, and they are *available* — `CLAUDE.md` carries them, memory loads
//! them, `docs/00` states them in full. On 2026-07-21 a scene shipped that broke four of them anyway:
//! a declared `gravity_ms2: 9.81`, a second grain-interaction path, the whole patch resolved regardless
//! of necessity, and a camera clamp — all while the Laws sat in a file that had been edited that day.
//!
//! Availability is evidently not enough. This module is the part of Law-abidance a machine can hold:
//! it FAILS THE BUILD when a world file declares a quantity that must emerge from matter. Judgement
//! still belongs to the author (see the pre-flight checklist in `CLAUDE.md`), but the specific mistakes
//! already made are now caught rather than remembered.
//!
//! Test-only: it guards bytes, it does not ship any.

/// A quantity that must EMERGE from matter, and the law that says so. Declaring one in a world file is
/// Law V — a number that did not come from physics — and usually Law II as well, since the emergent
/// value already exists elsewhere and the two will drift.
pub(crate) const MUST_EMERGE: &[(&str, &str)] = &[
    ("gravity_ms2", "g = GM/R² from the body's real layered mass (planet::LayeredBody::gravity_at)"),
    ("surface_gravity", "g = GM/R² from the body's real layered mass"),
    ("gravity", "g = GM/R² from the body's real layered mass"),
    ("surface_pressure_pa", "P = M_atm·g/(4πR²) — the weight of the declared air column"),
    ("surface_pressure", "P = M_atm·g/(4πR²) — the weight of the declared air column"),
    ("escape_velocity", "v_esc = sqrt(2GM/R) from mass and radius"),
    ("escape_velocity_ms", "v_esc = sqrt(2GM/R) from mass and radius"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every world definition that ships, scanned. A world may declare INITIAL CONDITIONS (a mass, a
    /// radius, a velocity, a material) — those are facts about the matter. It may not declare a
    /// CONSEQUENCE of them.
    ///
    /// This is the guard that would have caught `"gravity_ms2": 9.81` in `worlds/ground/world.json`
    /// before it reached a browser and a deploy.
    #[test]
    fn no_world_file_declares_a_quantity_that_must_emerge() {
        let roots = ["../../definitions", "../../web/public/worlds"];
        let mut files = Vec::new();
        for root in roots {
            collect_json(std::path::Path::new(root), &mut files);
        }
        assert!(
            !files.is_empty(),
            "found no world files to check — a guard that scans nothing passes vacuously"
        );

        let mut sins = Vec::new();
        for f in &files {
            let text = std::fs::read_to_string(f).expect("readable world file");
            for (key, emerges_from) in MUST_EMERGE {
                // Match the JSON key, not a substring of prose in a "_note".
                if text.contains(&format!("\"{key}\"")) {
                    sins.push(format!(
                        "{}: declares \"{key}\" — Law V: it must EMERGE ({emerges_from})",
                        f.display()
                    ));
                }
            }
        }
        assert!(sins.is_empty(), "world files declare emergent quantities:\n  {}", sins.join("\n  "));
    }

    /// The guard must be able to fail, or it is decoration that reports safety it never checked.
    #[test]
    fn the_law_guard_detects_a_declared_constant() {
        let offending = r#"{"name":"bad","type":"ground","ground":{"gravity_ms2":9.81}}"#;
        let caught = MUST_EMERGE
            .iter()
            .any(|(k, _)| offending.contains(&format!("\"{k}\"")));
        assert!(caught, "the guard failed to see a declared gravity — it would pass a Law V violation");
        let clean = r#"{"name":"ok","type":"ground","ground":{"planet":"earth"}}"#;
        assert!(
            !MUST_EMERGE.iter().any(|(k, _)| clean.contains(&format!("\"{k}\""))),
            "naming the planet is how you get gravity honestly; it must not be flagged"
        );
    }

    fn collect_json(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_json(&p, out);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }
}

/// A physical quantity that must have exactly ONE home in the source. Each entry is
/// `(literal, what it is, the module that owns it)`.
///
/// Law II says one question must never get two answers, and the way that law actually breaks is not by
/// argument — it is by someone typing a number that already exists somewhere else. Every case found so
/// far looked harmless at the keyboard:
///
///   * `22.0` — the display exposure — sat in `atmosphere`, in `ground_scene`, and again inside
///     `globe.wgsl`. Three copies of one camera setting.
///   * a missing specific heat was filled in as `840.0` in `impact.rs`, `1000.0` in `aggregate.rs` and
///     `1000.0` again in `matter.rs` — one unknown, three different invented answers.
///   * `6.96e8`, the Sun's radius, was written beside a definition file that already declared it.
///
/// None of those were caught by reading the Laws. They are caught by counting.
pub(crate) const SINGLE_SOURCE: &[(&str, &str, &str)] = &[
    ("6.371e6", "Earth's radius — assets/bodies/earth.json declares it", "planet"),
    ("6.96e8", "the Sun's radius — assets/bodies/sun.json declares it", "planet"),
    ("5.972e24", "Earth's mass — it emerges from the declared layers", "planet"),
    // The exemplar this checker was written for, and which the first version of it did not catch: the
    // display exposure lived in `atmosphere`, in `ground_scene` and again inside `globe.wgsl`.
    ("22.0", "the display exposure — atmosphere::SUN_GAIN owns it", "atmosphere"),
];

/// Shaders count too. A constant duplicated from Rust into WGSL is the same defect and harder to see,
/// because the two files never appear in the same diff — `22.0` sat in `space.wgsl` while three Rust
/// modules were being deduplicated.
pub(crate) const SHADER_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../shaders");

#[cfg(test)]
mod single_source_tests {
    /// **Law II, made countable.** A physical constant that appears in more than one place is two answers
    /// to one question waiting to drift apart, and that is exactly how every Law II violation in this
    /// engine has actually happened — not by argument, but by someone typing a number that already
    /// existed. Reading the Laws did not catch a single one of them. Counting does.
    ///
    /// Comments are stripped before counting: describing a number is how the reasoning gets recorded, and
    /// the point is to stop it being *computed* from two places, not to stop it being explained.
    /// Remove comments and `#[cfg(test)]` modules. The first version simply TRUNCATED at the first
    /// `#[cfg(test)]`, which in a file with an early test module discarded almost everything after it —
    /// `lib.rs` was 98% invisible to its own conformance check. Prose may name a number freely; a test
    /// asserting a value against a published reference is the opposite of a hidden duplicate.
    fn strip(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut skipping = false;
        let mut depth = 0i32;
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if !skipping && (t.starts_with("#[cfg(test)]") || t.starts_with("#![cfg(test)]")) {
                skipping = true;
                depth = 0;
                continue;
            }
            if skipping {
                depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if depth <= 0 && line.contains('}') {
                    skipping = false;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// Does `code` use `literal` AS A NUMBER, rather than as a fragment of a longer one?
    ///
    /// A plain substring search reported the Moon's orbital speed, 1022.0 m/s, as a copy of the display
    /// exposure 22.0 — and a checker that cries wolf gets switched off, which would cost more than the
    /// duplicates it finds. So the match must not begin mid-number or continue into more digits.
    fn contains_number(code: &str, literal: &str) -> bool {
        let bytes = code.as_bytes();
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(literal) {
            let at = from + rel;
            let before_ok = at == 0 || !matches!(bytes[at - 1], b'0'..=b'9' | b'.' | b'_');
            let end = at + literal.len();
            let after_ok = end >= bytes.len() || !matches!(bytes[end], b'0'..=b'9' | b'_');
            if before_ok && after_ok {
                return true;
            }
            from = at + 1;
        }
        false
    }

    #[test]
    fn a_physical_constant_lives_in_exactly_one_place() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut sources: Vec<(String, String)> = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(p) = stack.pop() {
            for e in std::fs::read_dir(&p).expect("engine sources are readable").flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x == "rs")
                    && !path.ends_with("laws.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    sources.push((path.display().to_string(), strip(&text)));
                }
            }
        }
        // Shaders as well: a constant copied from Rust into WGSL is the same defect and harder to spot,
        // because the two files never show up in the same diff.
        for e in std::fs::read_dir(super::SHADER_DIR).expect("shaders are readable").flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "wgsl") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                sources.push((path.display().to_string(), strip(&text)));
            }
        }
        assert!(sources.len() > 20, "expected the engine's sources AND its shaders, got {}", sources.len());

        // The matcher itself must not cry wolf: a checker that reports the Moon's 1022.0 m/s as a copy
        // of the exposure 22.0 gets switched off, which costs more than the duplicates it catches.
        assert!(!contains_number("const MOON_SPEED: f64 = 1022.0;", "22.0"), "1022.0 is not 22.0");
        assert!(contains_number("let g = 22.0;", "22.0"), "but 22.0 is");
        assert!(!contains_number("6.3712e6", "6.371e6"), "6.3712e6 is not 6.371e6");

        for &(literal, what, owner) in super::SINGLE_SOURCE {
            let hits: Vec<&str> = sources
                .iter()
                .filter(|(_, code)| contains_number(code, literal))
                .map(|(path, _)| path.rsplit('/').next().unwrap_or(path))
                .collect();
            assert!(
                hits.len() <= 1,
                "{literal} ({what}) appears in {} files: {hits:?} — it belongs to `{owner}` alone. \
                 Two copies of one number is Law II breaking quietly; ask the definition for it.",
                hits.len()
            );
        }
    }

    /// **A scene carries NO copy of a body parameter - not one, not a pinned one** (docs/59 one Earth,
    /// docs/58 name-freeness). The scene modules used to hold `EARTH_RADIUS_M`/`EARTH_MASS`/`MOON_*`
    /// constants that every render and fallback read; they now READ the shared definitions, so removing
    /// the constants broke nothing, and this scan is the grep made permanent: zero hits, forever.
    /// (Non-scene modules are covered by the ≤1-copy rule above; test fixtures pinning published values
    /// are stripped before counting, as everywhere in this file.)
    #[test]
    fn a_scene_module_carries_no_copy_of_a_body_parameter() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for &scene in super::SCENE_MODULES {
            let text = std::fs::read_to_string(format!("{dir}/{scene}"))
                .unwrap_or_else(|_| panic!("{scene} must exist"));
            let code = strip(&text);
            for &(literal, what) in super::DEFINITION_OWNED {
                assert!(
                    !contains_number(&code, literal),
                    "{scene} carries {literal} ({what}) - a scene reads the definition \
                     (planet::body / the cached shared params), it never copies the number"
                );
            }
        }
    }
}

/// Body parameters the DEFINITIONS own outright: a scene module may not carry even ONE copy of these,
/// pinned or otherwise. Each is `(literal, what it is)`. This replaces the old pinned
/// `EARTH_RADIUS_M` test - the constant it pinned is gone; scenes now READ the definition (cached
/// once), so there is no second copy left to drift, and this scan is what keeps it that way.
pub(crate) const DEFINITION_OWNED: &[(&str, &str)] = &[
    ("6.371e6", "Earth's radius - assets/bodies/earth.json"),
    ("5.972e24", "Earth's mass - it emerges from earth.json's layers"),
    ("1.737e6", "the Moon's radius - assets/bodies/moon.json"),
    ("7.342e22", "the Moon's mass - it emerges from moon.json's layers"),
];

/// The low-level collision primitives. A SCENE must never call these — detecting a collision is the
/// engine's job (`interaction::detect_swept`), and a scene that forecasts contact or recovers a contact
/// state by hand is a scene dictating its own physics.
pub(crate) const COLLISION_PRIMITIVES: &[&str] = &["swept_first_contact", "contact_velocity"];

/// The scene-facing modules: they own a canvas, a camera and a set of declared bodies, and nothing else.
/// A scene describes objects, trajectories and user controls; the engine does the physics.
pub(crate) const SCENE_MODULES: &[&str] = &["lib.rs", "ground_scene.rs"];

#[cfg(test)]
mod scene_purity_tests {
    /// **A scene describes; the engine simulates.**
    ///
    /// Robin: "we should be able to inject user controls (camera, etc) but not drive any physics from the
    /// scene itself... ensuring we don't try to dictate our own collision physics." This is that,
    /// mechanically: the collision-DETECTION primitives (forecast the contact, recover the true contact
    /// state) may be CALLED only by the engine's one collision owner, `interaction`. A scene reaches
    /// collisions through `interaction::detect_swept` and reads back what the engine found — it never runs
    /// its own swept-CCD loop, which is what `OrbitDemo` used to do, twice.
    ///
    /// The test scans the scene modules' source and asserts the primitives appear only as FIELD READS of
    /// a `DetectedCollision` (`c.contact_velocity`), never as function CALLS (`contact_velocity(`).
    #[test]
    fn a_scene_never_calls_the_collision_primitives_itself() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for &scene in super::SCENE_MODULES {
            let path = format!("{dir}/{scene}");
            let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{scene} must exist"));
            // Strip line comments — prose may name a primitive while explaining that the scene no longer
            // calls it, which is exactly what the migration comments do.
            let code: String = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            for &prim in super::COLLISION_PRIMITIVES {
                let call = format!("{prim}(");
                assert!(
                    !code.contains(&call),
                    "{scene} calls `{call}` — collision detection belongs to the engine \
                     (`interaction::detect_swept`), not a scene. A scene declares which bodies exist and \
                     where; it does not forecast their contacts."
                );
            }
        }

        // And the owner really does own it — the primitives ARE called there, or the invariant is vacuous.
        let owner = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/interaction.rs"))
            .expect("interaction.rs exists");
        for &prim in super::COLLISION_PRIMITIVES {
            assert!(
                owner.contains(&format!("{prim}(")),
                "the collision owner `interaction` must actually call `{prim}` — otherwise this test \
                 guards nothing"
            );
        }
    }
}

/// A scene body that NAMES a defined body (Luna, Terra, the Sun) may declare only its IDENTITY and
/// INITIAL CONDITIONS. These keys are the body's PHYSICS and belong to the definition — a scene that sets
/// them is overriding what Luna weighs or how big Terra is, which is the thing the engine must never let a
/// scene do.
pub(crate) const SCENE_BODY_OVERRIDE_KEYS: &[&str] = &["mass_kg", "radius_m", "tint"];

/// The body ids that HAVE a definition in `assets/bodies`. A scene body whose `profile`/`body` is one of
/// these is an instance of that definition.
pub(crate) const DEFINED_BODY_IDS: &[&str] = &["sun", "earth", "moon", "theia", "proto-earth"];

#[cfg(test)]
mod scene_declares_not_overrides_tests {
    /// **A scene declares objects and trajectories; it never overrides the engine's physics.**
    ///
    /// Robin: "the scene should be set up as: Sun in position, Earth in position/rotation/velocity/mass,
    /// Moon: position/velocity/mass... NOTHING about how to collide, particles, etc.", "each moon should
    /// be an instance of pre-defined object Luna", and "add a test to ensure scenes don't get run with
    /// engine overrides ever again... the scene test should be a parse of the scene's definition."
    ///
    /// So this parses every world file and asserts: a body that NAMES a defined body (an instance of Luna
    /// or Terra) carries only its identity and initial conditions — position, velocity, spin — and NOT
    /// mass, radius or tint, which are the definition's. A scene may still place a bare point mass (a body
    /// with no `profile`) and give it a mass; what it may not do is redefine Luna.
    #[test]
    fn no_scene_body_overrides_the_physics_of_the_body_it_names() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/worlds");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).expect("worlds directory exists").flatten() {
            let world = entry.path().join("world.json");
            let Ok(text) = std::fs::read_to_string(&world) else { continue };
            let json: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{world:?} is malformed: {e}"));
            let scene = entry.file_name().to_string_lossy().to_string();

            // The `planet` block of a "planet" world is a body instance too (docs/59 one Earth): a
            // world that names a defined body places it and may not size or weigh it.
            if let Some(planet) = json.get("planet") {
                let named = json
                    .get("body")
                    .or_else(|| planet.get("profile"))
                    .and_then(|p| p.as_str());
                if named.is_some_and(|p| super::DEFINED_BODY_IDS.contains(&p)) {
                    for &key in super::SCENE_BODY_OVERRIDE_KEYS {
                        assert!(
                            planet.get(key).is_none(),
                            "{scene}: the planet block names the defined body {:?} yet declares \
                             `{key}` - that is the definition's physics, not the scene's.",
                            named.unwrap(),
                        );
                        checked += 1;
                    }
                }
            }

            let Some(bodies) = json.get("bodies").and_then(|b| b.as_array()) else { continue };
            for b in bodies {
                let profile = b
                    .get("profile")
                    .or_else(|| b.get("body"))
                    .and_then(|p| p.as_str());
                // Only bodies that NAME a definition are instances; a bare point mass is free to declare
                // its own mass.
                if !profile.is_some_and(|p| super::DEFINED_BODY_IDS.contains(&p)) {
                    continue;
                }
                for &key in super::SCENE_BODY_OVERRIDE_KEYS {
                    assert!(
                        b.get(key).is_none(),
                        "{scene}: body {:?} names the defined body {:?} yet declares `{key}` — that is \
                         the definition's physics, not the scene's. A scene says WHICH body and WHERE; \
                         mass, radius and composition come from assets/bodies/{}.json.",
                        b.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                        profile.unwrap(),
                        profile.unwrap(),
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "expected to check some defined-body instances across the worlds");
    }
}
