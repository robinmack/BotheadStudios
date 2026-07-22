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
];

#[cfg(test)]
mod single_source_tests {
    /// **Law II, made countable.** A physical constant that appears in more than one place is two answers
    /// to one question waiting to drift apart, and that is exactly how every Law II violation in this
    /// engine has actually happened — not by argument, but by someone typing a number that already
    /// existed. Reading the Laws did not catch a single one of them. Counting does.
    ///
    /// Comments are stripped before counting: describing a number is how the reasoning gets recorded, and
    /// the point is to stop it being *computed* from two places, not to stop it being explained.
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
                } else if path.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    // Strip line comments and test modules: prose may name a number freely, and a test
                    // asserting a value against a reference is the opposite of a hidden duplicate.
                    let code: String = text
                        .lines()
                        .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("///"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let code = code.split("#[cfg(test)]").next().unwrap_or("").to_string();
                    sources.push((path.display().to_string(), code));
                }
            }
        }
        assert!(sources.len() > 10, "expected to find the engine's sources");

        for &(literal, what, owner) in super::SINGLE_SOURCE {
            let hits: Vec<&str> = sources
                .iter()
                .filter(|(_, code)| code.contains(literal))
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
}
