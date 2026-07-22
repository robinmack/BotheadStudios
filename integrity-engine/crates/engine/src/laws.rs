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
