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
    (
        "gravity_ms2",
        "g = GM/R² from the body's real layered mass (planet::LayeredBody::gravity_at)",
    ),
    (
        "surface_gravity",
        "g = GM/R² from the body's real layered mass",
    ),
    ("gravity", "g = GM/R² from the body's real layered mass"),
    (
        "surface_pressure_pa",
        "P = M_atm·g/(4πR²) — the weight of the declared air column",
    ),
    (
        "surface_pressure",
        "P = M_atm·g/(4πR²) — the weight of the declared air column",
    ),
    (
        "escape_velocity",
        "v_esc = sqrt(2GM/R) from mass and radius",
    ),
    (
        "escape_velocity_ms",
        "v_esc = sqrt(2GM/R) from mass and radius",
    ),
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
        assert!(
            sins.is_empty(),
            "world files declare emergent quantities:\n  {}",
            sins.join("\n  ")
        );
    }

    /// The guard must be able to fail, or it is decoration that reports safety it never checked.
    #[test]
    fn the_law_guard_detects_a_declared_constant() {
        let offending = r#"{"name":"bad","type":"ground","ground":{"gravity_ms2":9.81}}"#;
        let caught = MUST_EMERGE
            .iter()
            .any(|(k, _)| offending.contains(&format!("\"{k}\"")));
        assert!(
            caught,
            "the guard failed to see a declared gravity — it would pass a Law V violation"
        );
        let clean = r#"{"name":"ok","type":"ground","ground":{"planet":"earth"}}"#;
        assert!(
            !MUST_EMERGE
                .iter()
                .any(|(k, _)| clean.contains(&format!("\"{k}\""))),
            "naming the planet is how you get gravity honestly; it must not be flagged"
        );
    }

    pub(super) fn collect_json(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
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
    (
        "6.371e6",
        "Earth's radius — assets/bodies/earth.json declares it",
        "planet",
    ),
    (
        "6.96e8",
        "the Sun's radius — assets/bodies/sun.json declares it",
        "planet",
    ),
    (
        "5.972e24",
        "Earth's mass — it emerges from the declared layers",
        "planet",
    ),
    // The exemplar this checker was written for, and which the first version of it did not catch: the
    // display exposure lived in `atmosphere`, in `ground_scene` and again inside `globe.wgsl`.
    (
        "22.0",
        "the display exposure — atmosphere::SUN_GAIN owns it",
        "atmosphere",
    ),
    // Universal constants are the easiest of all to retype, because everyone knows them. Found
    // 2026-07-24: G written out in FIVE modules (`gravity`, `planet`, `damage`, `accretion`, `orbit`),
    // and Stefan–Boltzmann in three — one of those as a truncated `5.670e-8`, so a cooling moonlet and an
    // ablating meteor were literally radiating by different constants. Nothing had drifted far enough to
    // fail a test, which is precisely why counting is the check and reading is not.
    // G is exempt from the count because two SHADERS legitimately need it as a compile-time `const` and
    // WGSL cannot read a Rust constant. Those copies are PINNED instead — see
    // `pinned_constant_tests::the_shaders_gravitational_constant_matches_the_engines`, the same treatment
    // `EARTH_RADIUS_M` gets: a second copy is honest only if something fails when the two disagree.
    // ("6.674e-11", "Newton's constant G — orbit::G owns it", "orbit"),
    (
        "5.670_374_419e-8",
        "the Stefan–Boltzmann constant σ — blackbody::SIGMA owns it",
        "blackbody",
    ),
    (
        "5.670e-8",
        "σ again, truncated — ask blackbody::SIGMA, do not retype it shorter",
        "blackbody",
    ),
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
    pub(super) fn strip(text: &str) -> String {
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
            for e in std::fs::read_dir(&p)
                .expect("engine sources are readable")
                .flatten()
            {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x == "rs") && !path.ends_with("laws.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    sources.push((path.display().to_string(), strip(&text)));
                }
            }
        }
        // Shaders as well: a constant copied from Rust into WGSL is the same defect and harder to spot,
        // because the two files never show up in the same diff.
        for e in std::fs::read_dir(super::SHADER_DIR)
            .expect("shaders are readable")
            .flatten()
        {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "wgsl") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                sources.push((path.display().to_string(), strip(&text)));
            }
        }
        assert!(
            sources.len() > 20,
            "expected the engine's sources AND its shaders, got {}",
            sources.len()
        );

        // The matcher itself must not cry wolf: a checker that reports the Moon's 1022.0 m/s as a copy
        // of the exposure 22.0 gets switched off, which costs more than the duplicates it catches.
        assert!(
            !contains_number("const MOON_SPEED: f64 = 1022.0;", "22.0"),
            "1022.0 is not 22.0"
        );
        assert!(contains_number("let g = 22.0;", "22.0"), "but 22.0 is");
        assert!(
            !contains_number("6.3712e6", "6.371e6"),
            "6.3712e6 is not 6.371e6"
        );

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

#[cfg(test)]
mod fov_tests {
    /// **A frame's field of view must come from a NAMED source, never a literal at the projection.**
    ///
    /// Robin's condition for letting scenes switch between camera-handling systems (2026-07-25): *"As long as
    /// we can unify FOV within the engine for rendering, we should be good."* Several camera systems producing
    /// poses is fine — but every one of them ends at a projection, and if the FOV is written inline there,
    /// then switching systems can silently change how wide the world is.
    ///
    /// It was already broken when this was written, in the quietest possible way: the space band wrote `0.9`
    /// TWICE — once building its projection and once in `meters_per_pixel`, the HUD's scale bar. Change the
    /// projection and the bar goes on measuring a frustum the scene is no longer drawing, reporting confident
    /// wrong metres with nothing failing. The ground scene had a bare `60f32.to_radians()` at its projection.
    ///
    /// So the rule is mechanical and checkable: the first argument to `perspective_rh` must not begin with a
    /// digit. A named constant or a parameter can be traced, shared and tested; a literal cannot.
    #[test]
    fn no_projection_is_built_from_a_bare_field_of_view_literal() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut sins = Vec::new();
        let mut checked = 0usize;
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(p) = stack.pop() {
            for e in std::fs::read_dir(&p)
                .expect("engine sources are readable")
                .flatten()
            {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for (i, line) in text.lines().enumerate() {
                    let t = line.trim_start();
                    if t.starts_with("//") || t.starts_with("///") {
                        continue; // prose may name a number freely
                    }
                    let Some(at) = line.find("perspective_rh(") else {
                        continue;
                    };
                    checked += 1;
                    let arg = line[at + "perspective_rh(".len()..].trim_start();
                    if arg.starts_with(|c: char| c.is_ascii_digit()) {
                        sins.push(format!(
                            "{}:{}: builds a projection from a literal field of view ({}…) — name it, so a \
                             scale bar and a frustum cannot disagree",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            i + 1,
                            arg.chars().take(12).collect::<String>()
                        ));
                    }
                }
            }
        }
        assert!(
            checked >= 3,
            "expected to find the engine's projections, found {checked}"
        );
        assert!(
            sins.is_empty(),
            "field of view written inline at a projection:\n  {}",
            sins.join("\n  ")
        );
    }
}

#[cfg(test)]
mod pinned_constant_tests {
    /// `EARTH_RADIUS_M` has to be a `const` — `DISPLAY_SCALE` is derived from it in a const context — so
    /// it cannot simply ask `planet::body("earth")` at runtime. That makes it the one legitimate second
    /// copy of a number the definitions already own, and the only honest way to keep a second copy is to
    /// pin it: if `earth.json` ever changes, this fails rather than the two drifting apart in silence.
    /// **The GPU must fall at the same rate as the CPU.** `sph_step.wgsl` and `bh_gravity.wgsl` each
    /// carry their own `const G` because WGSL cannot read a Rust constant — a legitimate second copy, and
    /// therefore one that has to be pinned. If someone corrects G in `orbit.rs` and not in the shaders,
    /// the particles would gravitate by one constant while every CPU check used another, and the two
    /// would agree on nothing except that the tests passed.
    #[test]
    fn the_shaders_gravitational_constant_matches_the_engines() {
        let mut found = 0;
        for name in ["sph_step.wgsl", "bh_gravity.wgsl"] {
            let path = std::path::Path::new(super::SHADER_DIR).join(name);
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            let line = text
                .lines()
                .find(|l| l.trim_start().starts_with("const G:"))
                .unwrap_or_else(|| panic!("{name} declares no `const G` — did it move?"));
            let value: f64 = line
                .split('=')
                .nth(1)
                .and_then(|rhs| rhs.trim().trim_end_matches(';').parse().ok())
                .unwrap_or_else(|| panic!("{name}: cannot read G from {line:?}"));
            assert!(
                (value - crate::orbit::G).abs() <= f64::EPSILON * crate::orbit::G,
                "{name} gravitates by G = {value:e}, the engine by {:e}. Change `orbit::G`, then \
                 the shaders — never one alone.",
                crate::orbit::G
            );
            found += 1;
        }
        assert_eq!(found, 2, "both gravity shaders must be pinned");
    }
}

/// Body parameters the DEFINITIONS own outright: a scene module may not carry even ONE copy of these,
/// pinned or otherwise. Each is `(literal, what it is)`. This replaces the old pinned
/// `EARTH_RADIUS_M` test - the constant it pinned is gone; scenes now READ the definition (cached
/// once), so there is no second copy left to drift, and this scan is what keeps it that way.
pub(crate) const DEFINITION_OWNED: &[(&str, &str)] = &[
    ("6.371e6", "Earth's radius - assets/bodies/earth.json"),
    (
        "5.972e24",
        "Earth's mass - it emerges from earth.json's layers",
    ),
    ("1.737e6", "the Moon's radius - assets/bodies/moon.json"),
    (
        "7.342e22",
        "the Moon's mass - it emerges from moon.json's layers",
    ),
];

/// The low-level collision primitives. A SCENE must never call these — detecting a collision is the
/// engine's job (`interaction::detect_swept`), and a scene that forecasts contact or recovers a contact
/// state by hand is a scene dictating its own physics.
pub(crate) const COLLISION_PRIMITIVES: &[&str] = &["swept_first_contact", "contact_velocity"];

/// **Every engine physics primitive a SCENE must never call, paired with the module that owns it.**
///
/// Robin, having stated the rule repeatedly and then watched it broken anyway (2026-08-03): *"scenes
/// specify assemblies present, their positions, and starting velocities. They must NEVER introduce
/// physics"*, and — decisively — ***"laws without enforcement are vanity projects."***
///
/// ★ The boundary is not "a scene may not mention physics". A scene legitimately asks the engine to
/// STEP (`flight.step(&env, ..)`, `sim.step(dt)`) and reads back what happened. What it may never do is
/// compute a force, an acceleration, a contact or an atmosphere ITSELF — because then the answer to a
/// physical question depends on which scene asked it.
///
/// Every entry is a failure that actually happened here:
///   * `swept_first_contact`, `contact_velocity` — `OrbitDemo` ran its own swept-CCD loop, twice.
///   * `drag_accel` — a cannon's trajectory integrator took a drag COEFFICIENT as an argument, putting
///     the caller in charge of how hard the air pushes back. Deleted in favour of `flight::Flight`,
///     which already flies meteors through the same air and integrates quadratic drag in closed form.
///   * `AirShell::new` — building a world's atmosphere is deciding what its air IS. `PlanetAir` did
///     exactly that from inside `mod app` until it was moved into `flight`, where it belongs.
///   * `atmospheric_step`, `contact_accel`, `contact_force` — drag/heating/ablation, and the granular
///     contact law.
///
/// The paired module is checked too: if the owner does not call it either, the entry guards nothing and
/// the test says so rather than passing quietly.
pub(crate) const SCENE_FORBIDDEN_PHYSICS: &[(&str, &str)] = &[
    ("swept_first_contact", "interaction.rs"),
    ("contact_velocity", "interaction.rs"),
    ("drag_accel", "atmosphere.rs"),
    ("atmospheric_step", "flight.rs"),
    ("AirShell::new", "flight.rs"),
    ("contact_accel", "granular.rs"),
    ("contact_force", "granular.rs"),
    ("surface_crossing", "flight.rs"),
];

/// The scene-facing modules: they own a canvas, a camera and a set of declared bodies, and nothing else.
/// A scene describes objects, trajectories and user controls; the engine does the physics.
pub(crate) const SCENE_MODULES: &[&str] = &["lib.rs"];

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
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{scene} must exist"));
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
        let owner =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/interaction.rs"))
                .expect("interaction.rs exists");
        for &prim in super::COLLISION_PRIMITIVES {
            assert!(
                owner.contains(&format!("{prim}(")),
                "the collision owner `interaction` must actually call `{prim}` — otherwise this test \
                 guards nothing"
            );
        }
    }

    /// **A SCENE MUST NEVER INTRODUCE PHYSICS** — the general form, and the one with teeth.
    ///
    /// Robin: *"scenes specify assemblies present, their positions, and starting velocities. They must
    /// NEVER introduce physics"*, and *"laws without enforcement are vanity projects."*
    ///
    /// The sibling test above guards collision detection specifically. This guards the rest: drag, the
    /// atmospheric response, the contact law, the surface crossing. Both halves matter, because the way
    /// this rule actually breaks is never by someone re-implementing collisions — it is by a new module
    /// reaching for whichever primitive its own job happens to need.
    ///
    /// ★ Verified by making it fail, in both directions: inserting `drag_accel(` or `AirShell::new(`
    /// into a scene module turns it red, and an entry whose named owner does not call it is reported as
    /// guarding nothing.
    #[test]
    fn a_scene_never_introduces_physics() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let strip = |text: &str| -> String {
            text.lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        for &scene in super::SCENE_MODULES {
            let path = format!("{dir}/{scene}");
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{scene} must exist"));
            let code = strip(&text);
            for &(prim, owner) in super::SCENE_FORBIDDEN_PHYSICS {
                assert!(
                    !code.contains(&format!("{prim}(")),
                    "{scene} calls `{prim}(` — that is PHYSICS, and it belongs to `{owner}`.\n\
                     A scene specifies which assemblies are present, where they are and how fast they \
                     are going. It asks the engine to step them; it never computes a force, an \
                     acceleration, a contact or an atmosphere itself."
                );
            }
        }
        // Each entry must be REAL: if the named owner does not call it either, the line guards nothing.
        for &(prim, owner) in super::SCENE_FORBIDDEN_PHYSICS {
            let path = format!("{dir}/{owner}");
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{owner} must exist"));
            assert!(
                text.contains(&format!("{prim}(")),
                "`{prim}` is declared as owned by `{owner}`, which does not call it — so this entry \
                 forbids something nobody does, and guards nothing."
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
        for entry in std::fs::read_dir(dir)
            .expect("worlds directory exists")
            .flatten()
        {
            let world = entry.path().join("world.json");
            let Ok(text) = std::fs::read_to_string(&world) else {
                continue;
            };
            let json: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{world:?} is malformed: {e}"));
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

            let Some(bodies) = json.get("bodies").and_then(|b| b.as_array()) else {
                continue;
            };
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
        assert!(
            checked > 0,
            "expected to check some defined-body instances across the worlds"
        );
    }
}

/// **Numbering is a shared namespace, and shared namespaces collide when two people append to them
/// independently.** These are not style checks; they are the mechanical half of a coordination problem.
///
/// Three collisions happened in three consecutive integration steps of one contributor's work — two
/// documents claiming `docs/60`, two claiming `docs/59`, and two pairs of `docs/46` ledger rows numbered
/// 17 and 18. None was anyone's mistake: both sides appended to the next free number, and both were right
/// about what "next free" meant on their own branch. Prose in `CLAUDE.md` cannot fix that, because a
/// collaborator's copy of it is whatever they last merged — which is by definition older than the work
/// they are about to send. A test can, because CI runs on THEIR pull request, before it ever reaches us.
#[cfg(test)]
mod numbering_tests {
    const DOCS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs");

    /// A document's number is its identity: code cites `docs/59` and a reader has to land on one file.
    /// Two files sharing a number makes every citation to it ambiguous, and the ambiguity is invisible
    /// until someone follows the reference.
    #[test]
    fn no_two_documents_claim_the_same_number() {
        let mut seen: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(DOCS_DIR)
            .expect("docs directory exists")
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") {
                continue;
            }
            let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
            let Ok(n) = digits.parse::<u32>() else {
                continue;
            };
            if let Some(other) = seen.insert(n, name.clone()) {
                panic!(
                    "docs/{n} is claimed by BOTH {other:?} and {name:?}. A document's number is how code \
                     cites it, so two files cannot share one. Take the next free number and update the \
                     references in the module that cites it."
                );
            }
        }
        assert!(
            seen.len() > 20,
            "the scan found only {} numbered docs — it is not reading the directory",
            seen.len()
        );
    }

    /// The docs/46 conformance ledger is append-only and inherited: a row number is how a violation is
    /// referred to in commit messages, code comments and other rows. Two rows sharing a number silently
    /// makes one of them unreachable — and the way that goes wrong in a merge is that a whole run of rows
    /// gets replaced by a shorter run carrying the same numbers.
    #[test]
    fn the_conformance_ledger_numbers_each_row_once() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/46-one-physics-charter.md"
        );
        let text = std::fs::read_to_string(path).expect("the ledger exists");
        let mut rows: Vec<u32> = Vec::new();
        for line in text.lines() {
            // A ledger row starts `| <n> |`; the header and separator rows do not.
            let Some(rest) = line.strip_prefix("| ") else {
                continue;
            };
            let Some((num, _)) = rest.split_once(" |") else {
                continue;
            };
            if let Ok(n) = num.parse::<u32>() {
                rows.push(n);
            }
        }
        assert!(
            rows.len() > 10,
            "found only {} ledger rows — the parser is not matching",
            rows.len()
        );
        let mut sorted = rows.clone();
        sorted.sort_unstable();
        let mut dedup = sorted.clone();
        dedup.dedup();
        assert_eq!(
            sorted, dedup,
            "the docs/46 ledger has duplicate row numbers ({rows:?}). Rows are cited by number, so each \
             one must appear once — when a merge brings in rows numbered against a shorter ledger, \
             renumber the incoming ones onto the end rather than letting them land on existing rows."
        );
        // Contiguity from 1 is what makes "the next free number" unambiguous for the next contributor.
        let expected: Vec<u32> = (1..=rows.len() as u32).collect();
        assert_eq!(
            sorted, expected,
            "the ledger's row numbers are not 1..={} with no gaps. A gap makes 'the next free number' \
             ambiguous, which is how two people pick the same one.",
            rows.len()
        );
    }
}

/// **Physical quantities the material catalogue CARRIES that no code reads yet**, each with what it is
/// for. Every entry is a declared IOU (Law V): the data is sourced and real, the physics that would
/// consume it is not built, and saying so out loud is what keeps the gap from being invisible.
///
/// ★ **This list exists because a catalogued-but-unread property is the quietest failure in the
/// project.** `data/materials.json` gives `oak` and `pine` full orthotropic strength — tensile
/// **90 MPa along the grain against 5.5 MPa across it**, under an explicit `assumed_condition` of
/// *"strongly anisotropic"* — and **that 16x ratio is the whole reason wood splinters rather than
/// cratering.** Nothing read any of it, so oak failed like a weak stone, and the only way anyone found
/// out was by tracing a cannonball through a hull by hand (docs/46 row 30).
///
/// Read the other way, this is an inventory of physics the engine has DATA for and does not yet do —
/// a roadmap derived from measurement rather than opinion. Nearly half the catalogue is on it.
pub(crate) const UNWIRED_MATERIAL_PROPERTIES: &[(&str, &str)] = &[
    // ★ Straw's, added 2026-08-10 with the compaction channel (docs/70). Both are real numbers with no
    // reader YET, and the gate is right to insist they be named rather than left to look wired.
    (
        "ignition_point",
        "dry straw burns at ~550 K — `oxidation::burn` exists and no assembly is wired to it (docs/70 §4)",
    ),
    (
        "tensile_strength_stem",
        "a single straw in tension (25 MPa) against a bale's ~8 kPa as a mass — the same blade-vs-turf \
         distinction `grass` records, and it is what a RESOLVED pile of blades would spend instead of \
         the bale-scale figure (docs/46 row 59)",
    ),
    // ★★★ The grass LEAF BLADE's own flexure, added 2026-08-17 for docs/46 row 64. Unwired because
    // the engine has no elastic branch at all yet — docs/18's one deformation process is built for
    // FAILURE (crater, crush, fragment) and nothing lets a body store elastic strain and give it
    // back. These are the composable parts a slender-body flexure would read. There is deliberately
    // NO catalogued `flexural_rigidity`: EI depends on width, on thickness CUBED and on cross-section
    // shape, so a stored EI is wrong for every blade but the one it was measured on.
    // ★★★ THE TWO AGGREGATE COMPLIANCES, deliberately unread (2026-08-25, docs/46 row 67).
    // Both used to sit in `youngs_modulus`, where `granular::contact_from_material` — "the ONE place
    // where what the matter IS becomes how it collides" — handed them to every individual blade and
    // stem. They are not material properties: they are what an ASSEMBLY of those members does, so
    // Law III says the engine COMPUTES them. They stay catalogued as the measured targets that
    // emergent answer has to reproduce, which is the opposite of an input.
    (
        "youngs_modulus_sward_aggregate",
        "the measured compliance of a TURF MAT (5.0 MPa) — blades plus roots plus moist topsoil. Held \
         `youngs_modulus` until 2026-08-25, making every grass blade in the engine 212x too soft. Kept \
         as the target a vegetated-soil assembly must reproduce, never as a member's stiffness",
    ),
    (
        "youngs_modulus_loose_aggregate",
        "the measured compliance of a LOOSE HAY MASS (0.15 MPa). Held `youngs_modulus` until \
         2026-08-25 — 37,800x below the stem it describes, the worst instance in the catalogue, and \
         CIRCULAR besides: a haystack's bulk compliance is precisely what `pile::settle` exists to \
         produce from members stacking and bridging, so feeding it back as those members' own contact \
         stiffness made the emergent quantity an input to itself. Kept as what `pile::settle` must \
         REPRODUCE rather than be told",
    ),
    (
        "youngs_modulus_blade",
        "the leaf blade's EFFECTIVE FLAT-SLAB BENDING modulus (1.06 GPa) — derived from the only \
         directly measured Poaceae blade EI (Wu 2024, wheat) over its flat-slab I. It was NULL until \
         now, and the culm's 5.55 GPa stood in for it, making the engine 5.23x too STIFF",
    ),
    (
        "youngs_modulus_blade_tensile",
        "the blade in TENSION (0.552 GPa, Vincent 1982 via Inoue 1992, Lolium perenne) — a different \
         quantity from the bending modulus above by the sandwich ratio 2.59, and not interchangeable \
         with it",
    ),
    (
        "transverse_modulus_blade",
        "the blade ACROSS its fibres (13.9 MPa against 552 MPa along) — a 40:1 anisotropy that makes \
         a blade an oriented fibrous composite rather than an isotropic slab, and the leaf-scale \
         sibling of oak's `youngs_modulus_perp` (docs/46 row 30)",
    ),
    (
        "youngs_modulus_sclerenchyma_fibre",
        "the load-bearing phase inside the blade (22.6 GPa) — ~2-4% of cross-section carrying 90-95% \
         of the stiffness, which is what a composite flexure model would resolve rather than smear",
    ),
    (
        "density_fresh_blade",
        "a FRESH leaf's density (710 kg/m3) against `density` 1400, which is the DRY CELL WALL — the \
         same fibre-vs-arrangement split `straw` records. The modelled blade was 1.97x too heavy \
         (measured: n~991 Lolium blades, 258 mm / 149 mg)",
    ),
    // ★★★ The velocity a restitution was MEASURED AT, added 2026-08-17 for docs/46 row 63. Unwired
    // on purpose and it should stay that way until the contact can USE it: the engine's contact is
    // linear, so it returns one restitution at every impact speed, and there is nothing for a
    // reference velocity to mean until a nonlinear (Hertz-Kuwabara-Kono) contact makes `e` depend on
    // speed the way real viscoelastic contacts do. Recorded now because it is part of the
    // measurement — a bare `e` with no velocity is not a number — and because sourcing it was what
    // revealed that the restitutions themselves had never been cited at all.
    (
        "restitution_at_ms",
        "the impact velocity a catalogued `restitution` was measured at (m/s). Meaningless to a \
         LINEAR contact, which has no velocity dependence to anchor; the deferred computation is the \
         nonlinear contact of docs/46 rows 62-63",
    ),
    // ★★ The FIBRE's own stiffness, added 2026-08-15 for docs/46 row 60. Same blade-vs-bale split as
    // `tensile_strength_stem` above, and it had to be made because the gap is four to five orders of
    // magnitude: `youngs_modulus` on `straw` is 150 kPa (a BALE being squashed) and a single straw in
    // bending is 5.67 GPa. A pile of blades that could BEND would spend these; the reason nothing
    // reads them yet is that the pile's rods are rigid AND cannot rotate AND its contact injects
    // energy, so bending is not yet the measurable term (row 60).
    (
        "youngs_modulus_stem",
        "a single straw in BENDING (5.67 GPa) against the 150 kPa bale figure — O'Dogherty et al. \
         1995, four-point loading transverse to the stem; the EI a flexible blade would nest with \
         (docs/46 row 60)",
    ),
    (
        "rigidity_modulus_stem",
        "a single straw in TORSION (407 MPa, same source) — E/G = 13.9 is the axial anisotropy a \
         fibre composite must show, and it is what a twisting blade would spend (docs/46 row 60)",
    ),
    (
        "youngs_modulus_culm",
        "a living grass culm (5.55 GPa) against the 5 MPa soil-mat figure the same entry records — \
         the stiffness a standing stem bends and waves with. Its sibling `youngs_modulus_blade` is \
         deliberately NULL, because a leaf blade is not a culm and none was sourced (docs/46 row 60)",
    ),
    // Anisotropic failure — docs/46 row 30. The set that makes wood splinter along its grain, and
    // rolled steel and composite layup tear along theirs.
    // Bulk elasticity and strength the contact law does not yet ask for.
    (
        "bulk_modulus",
        "volumetric stiffness; the EOS covers this where a Tillotson block exists",
    ),
    (
        "poisson_ratio",
        "transverse strain under load — needed by any real stress solver",
    ),
    (
        "ductility",
        "how far matter yields before it breaks, i.e. dents rather than shatters",
    ),
    // Granular and fluid behaviour.
    (
        "friction_angle",
        "the repose angle as a MEASURED property, beside the derived mu",
    ),
    (
        "dynamic_viscosity",
        "real fluid flow; SPH currently carries its own viscosity",
    ),
    (
        "surface_tension",
        "droplets, menisci, and why a raindrop is a sphere",
    ),
    // Hardness — scratch, indent and impact resistance, none of it consulted.
    ("hardness_mohs", "scratch hardness"),
    ("hardness_janka_n", "indentation hardness of wood"),
    ("hardness_shore_a", "indentation hardness of elastomers"),
    // Optics.
    (
        "translucency",
        "light carried THROUGH matter — every material declares it, nothing reads it",
    ),
];

#[cfg(test)]
mod material_property_tests {
    /// **Every NUMBER in the material catalogue is either read by the engine or declared unwired.**
    ///
    /// Prose is not a gate (AGENTS.md §2), and neither is a memory note. The failure this catches is
    /// specific and has already happened: someone sources a real property, puts it in the catalogue,
    /// and nothing consumes it — so the engine keeps answering with the isotropic approximation while
    /// the honest number sits in the file. It looks like progress and changes nothing.
    ///
    /// The check runs BOTH ways, which is what keeps the list from rotting:
    ///   * a numeric property nothing reads and nobody declared -> FAIL (data added with no consumer)
    ///   * a declared property that IS now read -> FAIL (wire it, then delete the entry)
    ///
    /// `reaction` is scanned too, added when the first energetic substances were catalogued — a new
    /// block of sourced numbers is exactly the case this guard exists for, and leaving it unscanned
    /// would have made the oxidation data invisible to the very check written to catch invisible data.
    ///
    /// Only NUMBERS are enforced. `grain`, `notes`, `source`, `equation` and `conductivity_note` are prose — they
    /// document the data rather than being physical quantities, so nothing is expected to consume them.
    /// The `tillotson` block is excluded too: it is read wholesale as a struct and its fields are named
    /// `A`, `B`, `a`, `b`, `alpha`, so scanning source for them would match everything and prove nothing.
    #[test]
    fn every_catalogued_material_number_is_read_or_declared_unwired() {
        let json: serde_json::Value = serde_json::from_str(crate::materials::MATERIALS_JSON)
            .expect("data/materials.json parses");
        let mut props: std::collections::BTreeMap<String, usize> = Default::default();
        for m in json["materials"].as_array().expect("a materials array") {
            for block in ["mechanical", "optical", "thermal", "reaction"] {
                let Some(map) = m.get(block).and_then(|b| b.as_object()) else {
                    continue;
                };
                for (k, v) in map {
                    let numeric = v.is_number()
                        || v.as_array()
                            .is_some_and(|a| a.first().is_some_and(|x| x.is_number()));
                    if numeric {
                        *props.entry(k.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        assert!(
            props.len() > 20,
            "expected a rich catalogue; found only {} numeric properties",
            props.len()
        );

        // Every line of engine source, as one blob to scan.
        let mut blob = String::new();
        collect_rs(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut blob,
        );
        assert!(blob.len() > 100_000, "the source scan found almost nothing");

        let declared: std::collections::BTreeMap<&str, &str> =
            super::UNWIRED_MATERIAL_PROPERTIES.iter().copied().collect();
        let mut undeclared = Vec::new();
        let mut now_wired = Vec::new();
        for (prop, on) in &props {
            let read = reads_identifier(&blob, prop);
            match (read, declared.contains_key(prop.as_str())) {
                (false, false) => undeclared.push(format!("  {prop} (on {on} materials)")),
                (true, true) => now_wired.push(format!("  {prop}")),
                _ => {}
            }
        }
        assert!(
            undeclared.is_empty(),
            "these catalogued NUMBERS are read by nothing and are not declared unwired.\n\
             A sourced property with no consumer is an invisible gap — the engine keeps using its\n\
             approximation while the honest number sits in the file. Either wire it, or add it to\n\
             `laws::UNWIRED_MATERIAL_PROPERTIES` with what it is for:\n{}",
            undeclared.join("\n")
        );
        assert!(
            now_wired.is_empty(),
            "these are declared UNWIRED but the engine now reads them — delete the entries, the debt\n\
             is paid:\n{}",
            now_wired.join("\n")
        );
    }

    /// Word-boundary search, so `cohesion` does not match `cohesion_ceiling` and `a` does not match
    /// every word in the tree.
    ///
    /// ★ **Comments do NOT count as readers, and the first version of this guard had that backwards.**
    /// It counted them deliberately, on the reasoning that a property named in a comment is at least
    /// VISIBLE. The flaw showed up the moment a test explained WHY `ductility` is unread: naming it in
    /// prose marked it as read and the guard reported the debt paid. **A guard that a comment about the
    /// gap can satisfy is a guard about prose, not about code.** Source is stripped of comments and of
    /// `#[cfg(test)]` blocks (`single_source_tests::strip`, reused rather than rewritten) before
    /// scanning, so only production code counts — which is the correct bar anyway: a property read only
    /// by its own test is not consumed by the engine.
    fn reads_identifier(blob: &str, ident: &str) -> bool {
        let bytes = blob.as_bytes();
        let mut from = 0;
        while let Some(i) = blob[from..].find(ident) {
            let s = from + i;
            let e = s + ident.len();
            let before_ok = s == 0 || !is_word(bytes[s - 1]);
            let after_ok = e >= bytes.len() || !is_word(bytes[e]);
            if before_ok && after_ok {
                return true;
            }
            from = s + 1;
        }
        false
    }

    fn is_word(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }

    pub(super) fn collect_rs(dir: &std::path::Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_rs(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                // ★ Skip THIS file. The declaration list below names every unwired property, so
                // including it makes each declaration its own reader and the check reports that all
                // the debt is paid. A registry is not a consumer.
                if p.file_name().is_some_and(|n| n == "laws.rs") {
                    continue;
                }
                if let Ok(t) = std::fs::read_to_string(&p) {
                    out.push_str(&super::single_source_tests::strip(&t));
                    out.push('\n');
                }
            }
        }
    }
}

/// **What a body's surface SHOWS is the outside of the thing standing there, not its structure.**
///
/// Robin (2026-08-03), on a picture of the Irish coast that came back the colour of a plank:
/// *"Pine Timber is always the wrong choice for flora though, we should look for 'pine needles' or
/// 'pine leaves', same with other biomes."*
///
/// A land-cover class says *"this footprint is forest"*. What a forest presents to anything looking at
/// it is the CANOPY — foliage. Timber is the trunk: barely visible from any distance, and a different
/// substance with a different colour, density and thermal response. `assets/bodies/earth.json` mapped
/// class 3 straight to `pine`, the catalogue's pine TIMBER (albedo [0.68, 0.48, 0.21], a brown), so
/// every forest on Earth — the Amazon, the Congo, Ireland — was drawn as cut lumber, and a bronze
/// cannon standing on it nearly vanished into ground the same colour as itself.
///
/// ★ **The criterion is physical, not a list of approved names.** Living vegetation is green because
/// chlorophyll absorbs the red and the blue and leaves the green: a material standing for a vegetated
/// surface must have its green channel above both others. A name-based rule would have to be extended
/// by hand for every new material; this one is a property of the matter and extends itself.
#[cfg(test)]
mod biome_material_tests {
    /// Every body definition on disk, since a rule that only checks Earth is a rule about Earth.
    fn body_definitions() -> Vec<(String, crate::terra::world_def::World)> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/bodies");
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir)
            .expect("assets/bodies exists")
            .flatten()
        {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
                let body: crate::terra::world_def::World = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("{name} parses as a body definition: {e}"));
                out.push((name, body));
            }
        }
        assert!(
            !out.is_empty(),
            "no body definitions found — this guards nothing"
        );
        out
    }

    #[test]
    fn a_biome_never_paints_the_ground_with_the_inside_of_a_plant() {
        let mats = crate::materials::load();
        let json: serde_json::Value = serde_json::from_str(crate::materials::MATERIALS_JSON)
            .expect("data/materials.json parses");
        let organic: std::collections::BTreeSet<&str> = json["materials"]
            .as_array()
            .expect("a materials array")
            .iter()
            .filter(|m| m.get("category").and_then(|c| c.as_str()) == Some("organic"))
            .filter_map(|m| m.get("id").and_then(|i| i.as_str()))
            .collect();
        assert!(
            organic.contains("pine") && organic.contains("oak"),
            "the woods must be catalogued as organic or this check cannot see the failure it exists for"
        );

        let mut checked = 0;
        for (file, body) in body_definitions() {
            let Some(surface) = body.surface else {
                continue;
            };
            // A class is a MIXTURE, so every living constituent of it is checked. That matters: a
            // savanna is mostly grass with some tree, and slipping timber in as the minority
            // constituent would be exactly as wrong and much harder to see.
            for (class, mix) in &surface.biomes {
                for (mat_id, frac) in mix {
                    if !organic.contains(mat_id.as_str()) {
                        continue; // water, sand, snow, rock — not living, not this rule's business
                    }
                    checked += 1;
                    let m = &mats[crate::materials::index_of(&mats, mat_id)];
                    let [r, g, b] = m.albedo;
                    assert!(
                        g > r && g > b,
                        "{file}: land-cover class {class} contains `{mat_id}` at {frac:.2}, whose \
                         albedo is [{r:.3}, {g:.3}, {b:.3}] — that is not green, so it is not a \
                         living surface.\n\
                         A land-cover class shows the OUTSIDE of what grows there: foliage, not the \
                         timber inside the trunk. Reach for a foliage material (`conifer_foliage`, \
                         `broadleaf_foliage`, `grass`), not the structural tissue sharing its name."
                    );
                }
            }
        }
        assert!(
            checked >= 2,
            "no body maps a land-cover class to an organic material — this guard is vacuous"
        );
    }

    /// **And the guard must be able to fail**, which is only knowable by checking that the material
    /// it was written to reject is still in the catalogue and still fails the criterion.
    ///
    /// Verified by mutation on 2026-08-03: putting `pine` back on class 3 turns
    /// `a_biome_never_paints_the_ground_with_the_inside_of_a_plant` red with the albedo printed. This
    /// test is what keeps that true after somebody edits the wood entry — a guard whose trigger has
    /// quietly stopped triggering passes forever and teaches you to trust it.
    #[test]
    fn the_material_this_guard_rejects_is_still_rejectable() {
        let mats = crate::materials::load();
        for wood in ["pine", "oak"] {
            let [r, g, b] = mats[crate::materials::index_of(&mats, wood)].albedo;
            assert!(
                !(g > r && g > b),
                "`{wood}` now reads green ([{r:.3}, {g:.3}, {b:.3}]), so the biome guard would ACCEPT \
                 timber as a vegetated surface and the rule it enforces has silently switched off"
            );
        }
    }
}

/// **The complete set of engine calls a SCENE is allowed to make.**
///
/// Robin (2026-08-03): *"Setting a scene should never involve changes to the engine… I'm tired of
/// scenes adding/accessing custom engine routes."* `docs/65` states the model: the scene sets the
/// characters and the setting, the assemblies are the actors, the engine is the director and the stage.
///
/// Four verbs and nothing else:
///   * **place**  — which assemblies are present, where, how fast: `load_world`
///   * **observe** — where the watcher is: `set_camera_pose`, `clear_camera_pose`, `camera_state`, `resize`
///   * **step**   — let time pass, draw what is: `advance`, `render`
///   * **signal** — tell the universe something happened at a point: ★ DOES NOT EXIST YET
///
/// `add_tile`/`tiles_wanted` are here as the pattern worth copying: the ENGINE decides what data it
/// needs and the host merely performs the I/O. The decision never leaves the engine.
pub(crate) const SCENE_API_ALLOWED: &[&str] = &[
    "add_tile",
    "advance",
    "camera_follow",
    "camera_is_following",
    "camera_state",
    "clear_camera_pose",
    "load_world",
    "place_camera",
    "render",
    "resize",
    "set_camera_pose",
    "tiles_wanted",
];

/// **Every engine route a scene calls that it should not — declared, so the list can only shrink.**
///
/// Measured 2026-08-03: 79 distinct engine methods are called from `web/src/*.ts`, of which nine are
/// legitimate. The rest are here. They fall into four kinds, and naming the kind is how each one gets
/// paid off:
///
///   1. **An assembly's name in the engine's API** — `fire_cannon`, `emplace_cannon`, `brake_moon`,
///      `drop_moon`, `reset_moon`, `throw_meteor`, `launch_swarm`, `moon_perigee_km`. These are five
///      different spellings of ONE missing verb, `signal`. Robin's decomposition of the gun is the
///      template (`docs/65` §2): the scene says *apply heat, here*; the engine asks the GUN assembly
///      where its charge sits; combustion, pressure and launch follow as consequences nobody named.
///   2. **A second answer to a question the engine already answers** — `set_fly`, `set_orbit`,
///      `pan_view`, `drag_look`, `walk`, `move_tangent`, `pan_tangent`, `zoom_alt`, `aim_screen`,
///      `set_alt_bounds`. Three scene structs, three camera models. `set_camera_pose` is the general
///      one and is already allowed; the rest collapse into it.
///   3. **Reads** — `altitude_m`, `latitude`, `longitude`, `world_name`, `particle_count`,
///      `sun_elevation_deg`, `surface_material`, and the counters. These change nothing and decide
///      nothing, so they do not break the model; but forty accessors is still forty pieces of API where
///      one `state` query would do.
///   4. **Scene-specific loading** — `load_earth_surface`, `load_impact_world`, `load_site_world`.
///      `load_world` is the general form and is allowed.
///
/// ★ The entry to fix FIRST is the missing `signal`, because kind 1 cannot be paid off without it.
pub(crate) const SCENE_API_DEBT: &[&str] = &[
    "altitude_m",
    "arc_available",
    "arc_label",
    "arc_press",
    "arc_stop",
    "brake_moon",
    "contact_distance_km",
    "debris_extent_km",
    "disk_stats_json",
    "drag_look",
    "drawn_count",
    "drop_moon",
    "drop_window_impact_s",
    "drop_window_s",
    "earth_binding_energy_j",
    "earth_day_hours",
    "emplace_cannon",
    "enter_geologic_time",
    "fire_cannon",
    "flight_count",
    "focus_earth",
    "focus_label",
    "focus_moon",
    "gpu_disk_stats_json",
    "ground_biome",
    "has_impacted",
    "impact_countdown_s",
    "impact_energy_j",
    "latitude",
    "launch_swarm",
    "load_earth_surface",
    "load_impact_world",
    "load_site_world",
    "load_star_catalog",
    "longitude",
    "meters_per_pixel",
    "moon_binding_energy_j",
    "moon_distance_km",
    "moon_perigee_km",
    "moon_speed_kms",
    "move_tangent",
    "nudge_aftermath_rate",
    "pan_tangent",
    "pan_view",
    "reset_moon",
    "set_alt_bounds",
    "set_orbit",
    "set_time_scale",
    "sim_since_impact_s",
    "site_status",
    "start_gpu_impact",
    "sun_elevation_deg",
    "tile_count",
    "time_scale_value",
    "trail_mass_kg",
    "world_name",
    "zoom_alt",
];

#[cfg(test)]
mod scene_api_tests {
    /// Every call a scene makes on the engine handle, with the file it was made from.
    fn scene_calls() -> Vec<(String, String)> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/src");
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).expect("web/src exists").flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "ts") {
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let Ok(src) = std::fs::read_to_string(&p) else {
                continue;
            };
            // The handles a scene host holds the engine object by. Comments are stripped first so a
            // method NAMED in prose does not count as a call — the same mistake `laws` already made
            // once, when a comment mentioning a property counted as a consumer of it.
            let code = super::single_source_tests::strip(&src);
            for handle in ["terra", "demo", "g", "engine"] {
                let needle = format!("{handle}.");
                let mut rest = code.as_str();
                while let Some(i) = rest.find(&needle) {
                    let after = &rest[i + needle.len()..];
                    let m: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                        .collect();
                    // Only a CALL counts; `terra.foo` as a property read is not an API call.
                    if !m.is_empty() && after[m.len()..].starts_with('(') {
                        out.push((name.clone(), m));
                    }
                    rest = &rest[i + needle.len()..];
                }
            }
        }
        assert!(
            !out.is_empty(),
            "no scene calls found — this guard scans nothing"
        );
        out
    }

    /// **A scene may not grow a new engine route.** The ratchet, in both directions.
    ///
    /// Robin (2026-08-03): *"Somehow we need to codify this with tests, etc to ensure this vision is
    /// preserved always."* So:
    ///   * a call that is neither ALLOWED nor declared debt -> FAIL — somebody added a custom route
    ///   * a declared entry nothing calls any more -> FAIL — delete it; the list must stay TRUE
    ///
    /// The second half is what stops this becoming a stale list that quietly forgives everything. It is
    /// the same shape as `UNWIRED_MATERIAL_PROPERTIES`, which works for the same reason.
    #[test]
    fn a_scene_calls_only_the_general_engine_api() {
        let calls = scene_calls();
        let allowed: std::collections::BTreeSet<&str> =
            super::SCENE_API_ALLOWED.iter().copied().collect();
        let debt: std::collections::BTreeSet<&str> =
            super::SCENE_API_DEBT.iter().copied().collect();

        let mut fresh: std::collections::BTreeMap<String, String> = Default::default();
        for (file, m) in &calls {
            if !allowed.contains(m.as_str()) && !debt.contains(m.as_str()) {
                fresh.insert(m.clone(), file.clone());
            }
        }
        assert!(
            fresh.is_empty(),
            "a scene grew {} NEW engine route(s):\n{}\n\n\
             docs/65: a scene names which assemblies are present, where they are, how fast they are \
             going, and where the watcher stands. It does not get a method of its own.\n\
             If the engine genuinely lacks a capability, say so and build it GENERAL — the way \
             `oxidation::apply_heat` replaced `fire_gun` — rather than adding a route shaped like this \
             one scene. If you are certain it belongs, add it to SCENE_API_ALLOWED and defend that in \
             review; adding it to SCENE_API_DEBT is an admission, not a fix.",
            fresh.len(),
            fresh
                .iter()
                .map(|(m, f)| format!("  {m}  (called from {f})"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // The other direction: debt that is no longer owed must be struck off, or the list stops
        // describing the code and starts excusing it.
        let called: std::collections::BTreeSet<&str> =
            calls.iter().map(|(_, m)| m.as_str()).collect();
        let paid: Vec<&&str> = super::SCENE_API_DEBT
            .iter()
            .filter(|d| !called.contains(**d))
            .collect();
        assert!(
            paid.is_empty(),
            "SCENE_API_DEBT lists {} route(s) no scene calls any more: {:?}\n\
             Delete them from the list. A debt register that is not true is not a register.",
            paid.len(),
            paid
        );
    }

    /// **The allowed list must stay small, and it must stay general.**
    ///
    /// A whitelist defends nothing if the way to pass is to widen it. This pins the size, and pins that
    /// no permitted call names a specific thing in the universe — the failure the whole document is
    /// about is an assembly's name appearing in the engine's public surface.
    #[test]
    fn the_permitted_engine_api_names_no_particular_thing() {
        assert!(
            super::SCENE_API_ALLOWED.len() <= 12,
            "the permitted scene API has grown to {} calls. docs/65 says four verbs: place, observe, \
             step, signal. Widening the whitelist is how a whitelist stops being one.",
            super::SCENE_API_ALLOWED.len()
        );
        // Nouns from the universe. An engine that knows what a cannon is has stopped being an engine.
        const THINGS: &[&str] = &[
            "cannon", "moon", "earth", "meteor", "swarm", "gun", "shot", "ship", "tree", "impact",
        ];
        for call in super::SCENE_API_ALLOWED {
            for thing in THINGS {
                assert!(
                    !call.contains(thing),
                    "`{call}` is in the permitted scene API and names `{thing}`. The engine knows \
                     about matter, heat, contact, time and light; it must not know what a {thing} is."
                );
            }
        }
    }
}

/// **Worlds that name a real place and then INVENT its ground** — declared, so the list can only shrink.
///
/// Robin, on finding the Ground scene still shipping (2026-08-03): *"It should be destroyed with fire.
/// It is so wrong… a cube of terrain with no planet to support it, anathema to the engine."* And then
/// the question that produced this guard: *"Since the scene defined by 'Ground' does not qualify at all
/// under the new model where we add assemblies to the engine, is there something that we can do to
/// guard against such merges in future?"*
///
/// ★★ **The honest answer is that the guard must be on the PROPERTY, not the file or the merge.** The
/// history shows why: the terrain `Engine` was deleted (docs/50), and then the same idea was REBUILT
/// under a different name as `ground_scene.rs` (PR #53). A tombstone naming `Engine` would have caught
/// nothing, because nothing named `Engine` came back. And a merge-specific check would catch nothing
/// either — `git log --diff-filter=A` shows the scene was added once and never resurrected.
///
/// So the sin is stated as a property instead, and it is exactly docs/63 item 1: **a world that names a
/// real body and a real coordinate, and then declares its own surface relief.** That is a real place
/// with imaginary ground. It is checkable, it fires on a fork's own CI before the work ever reaches
/// this repo (AGENTS.md §2), and it catches a rebuild under any name.
///
/// Each entry is `(file, why it is still here)`. **The list may shrink and must never grow.**
pub(crate) const WORLDS_THAT_INVENT_THEIR_GROUND: &[(&str, &str)] = &[
    (
        "ground-zero/world.json",
        "the Ground Zero scene, still shipping — found BY this guard on the day it was written, which \
         is the argument for property guards over tombstones. Same sin as the deleted Ground scene: \
         it names earth and a lat/lon, then declares size_voxels/amplitude/octaves. Its surface must \
         come from the body's measured elevation (terra::tiles), as Terra's does.",
    ),
    (
        "ground-patch.json",
        "NOT SHIPPED — an engine test fixture (assets/worlds/), kept when the Ground scene was deleted \
         because three native tests use it to prove a ground patch still BUILDS from a definition. It \
         is the specimen this guard exists to reject, retained deliberately so the capability outlives \
         the diorama. If it ever moves under web/public it is a scene again and must be fixed first.",
    ),
];

#[cfg(test)]
mod world_surface_tests {
    /// **A world may say WHERE it is. It may not then invent what is there.**
    ///
    /// Naming a body and a coordinate is a scene doing its job (docs/65: characters and setting).
    /// Declaring the relief at that coordinate is the scene answering a question the body already
    /// answers — one question, two answers, and the second one is fiction.
    ///
    /// Verified by making it fail: adding an `octaves` block to a world that names a planet turns this
    /// red and prints the file.
    #[test]
    fn a_world_that_names_a_real_place_does_not_invent_its_ground() {
        // What a world INVENTS: procedural relief dials, which describe a surface rather than locate one.
        const INVENTS: &[&str] = &[
            "\"octaves\"",
            "\"amplitude_m\"",
            "\"size_voxels\"",
            "\"base_top_m\"",
        ];
        // What a world may honestly declare: where on which body it sits.
        const LOCATES: &[&str] = &["\"planet\"", "\"body\"", "\"lat\""];

        let mut files = Vec::new();
        for root in ["../../web/public/worlds", "../../assets/worlds"] {
            super::tests::collect_json(std::path::Path::new(root), &mut files);
        }
        assert!(
            !files.is_empty(),
            "no world files found — this guard scans nothing"
        );

        let declared: std::collections::BTreeSet<&str> = super::WORLDS_THAT_INVENT_THEIR_GROUND
            .iter()
            .map(|(f, _)| *f)
            .collect();

        let mut fresh = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for f in &files {
            let text = std::fs::read_to_string(f).expect("readable world file");
            let locates = LOCATES.iter().any(|k| text.contains(k));
            let invents: Vec<&str> = INVENTS
                .iter()
                .copied()
                .filter(|k| text.contains(k))
                .collect();
            if !locates || invents.is_empty() {
                continue;
            }
            // Match a declared entry by suffix, so the list does not encode a directory layout.
            let path = f.to_string_lossy().replace('\\', "/");
            match declared.iter().find(|d| path.ends_with(**d)) {
                Some(d) => {
                    seen.insert(*d);
                }
                None => fresh.push(format!("  {path}  declares {invents:?}")),
            }
        }
        assert!(
            fresh.is_empty(),
            "a world names a real place and then invents the ground there:\n{}\n\n\
             docs/63 item 1: that is a real place with IMAGINARY ground, and it is the exact shape of \
             the Ground scene Robin had destroyed — a cube of relief on a coordinate that has measured \
             elevation available. A world says WHERE; the body says WHAT IS THERE.",
            fresh.join("\n")
        );
        // The other direction: an entry that no longer describes anything must be struck off, or the
        // register stops being true and starts being an excuse.
        let stale: Vec<&str> = declared
            .iter()
            .copied()
            .filter(|d| !seen.contains(d))
            .collect();
        assert!(
            stale.is_empty(),
            "WORLDS_THAT_INVENT_THEIR_GROUND lists {} file(s) that no longer do: {stale:?}\n\
             Delete them from the list — a register that is not true is not a register.",
            stale.len()
        );
    }
}

/// **Everything the engine exports to the browser, and what each one IS.**
///
/// A new entry appearing means the engine grew a new surface for content to attach to — which docs/65
/// forbids: *"Setting a scene should never involve changes to the engine."* This is the census that
/// would have caught `ground_scene.rs` on the day it landed, not because of its NAME (the deleted
/// terrain scene was called `Engine`; nothing named `Engine` ever came back) but because it was a third
/// exported struct owning a canvas and a render loop.
///
/// `GpuProbe` is here and is NOT a scene — it owns no canvas and draws nothing, it is a compute-only
/// diagnostic. It is listed because the check is "what does the engine export", which is the question
/// that has teeth; calling it a scene to make a test pass would be the test lying about the code.
pub(crate) const WASM_EXPORTED_STRUCTS: &[(&str, &str)] = &[
    (
        "GpuProbe",
        "compute-only diagnostic, no canvas — not a scene",
    ),
    (
        "OrbitDemo",
        "SCENE: the space band (docs/27 giant impact); owns gpu_sph",
    ),
    (
        "Terra",
        "SCENE: worlds-as-data planet (docs/43); owns terra::",
    ),
];

#[cfg(test)]
mod scene_struct_tests {
    /// **No new scene struct.** Adding one is an engine edit to add content (docs/46 row 14).
    #[test]
    fn adding_a_scene_does_not_mean_editing_the_engine() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut found = std::collections::BTreeSet::new();
        let mut src = String::new();
        super::material_property_tests::collect_rs(std::path::Path::new(dir), &mut src);
        for text in [src] {
            // A scene struct is one exported to the browser that owns a canvas surface.
            for (i, line) in text.lines().enumerate() {
                if !line.contains("pub struct ") {
                    continue;
                }
                let before = text
                    .lines()
                    .skip(i.saturating_sub(3))
                    .take(3)
                    .collect::<String>();
                if !before.contains("#[wasm_bindgen]") {
                    continue;
                }
                if let Some(name) = line
                    .split("pub struct ")
                    .nth(1)
                    .and_then(|r| r.split(|c: char| !c.is_alphanumeric() && c != '_').next())
                {
                    if !name.is_empty() {
                        found.insert(name.to_string());
                    }
                }
            }
        }
        let allowed: std::collections::BTreeSet<&str> = super::WASM_EXPORTED_STRUCTS
            .iter()
            .map(|(n, _)| *n)
            .collect();
        let fresh: Vec<&String> = found
            .iter()
            .filter(|f| !allowed.contains(f.as_str()))
            .collect();
        assert!(
            fresh.is_empty(),
            "the engine exports {fresh:?} to the browser, which the register does not know about.\n\
             docs/65: a scene names which assemblies are present, where they are and where the watcher \
             stands. It does not get a struct of its own inside the engine — that is docs/46 row 14, \
             and it is how the Ground scene came to exist after its predecessor was deleted."
        );
        let stale: Vec<&str> = super::WASM_EXPORTED_STRUCTS
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| !found.contains(*n))
            .collect();
        assert!(
            stale.is_empty(),
            "the register lists {stale:?}, which the engine no longer exports — strike them off. \
             (`Ground` was struck off here when its scene was deleted.)"
        );
    }
}

#[cfg(test)]
mod one_earth_tests {
    /// **Every scene that draws Earth draws the SAME Earth.**
    ///
    /// Robin, stating it as a requirement rather than a hope (2026-08-03): *"Because the scene just
    /// calls out which assemblies to include, we should be able to get enhanced renders of earth in ALL
    /// scenes from this work today. If not, we have a serious flaw in how we implement
    /// scene/assembly/engine."* And, when it was called a prediction: *"Not a prediction, a confident
    /// assertion of the rules I've decreed (and a way to ensure they are being met)."*
    ///
    /// This is the way. Earth's SURFACE — its rasters, its elevation range, its relief exaggeration and
    /// its biome map — belongs to `assets/bodies/earth.json` and to nothing else. A world file that
    /// grows its own `surface` for a body the engine already defines is a SECOND Earth, free to drift
    /// from the first, which is what docs/63 exists to end.
    #[test]
    fn a_worlds_body_is_the_only_place_its_surface_is_described() {
        let bodies = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/bodies");
        let defined: std::collections::BTreeSet<String> = std::fs::read_dir(bodies)
            .expect("assets/bodies exists")
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension()? == "json").then(|| p.file_stem()?.to_str().map(str::to_string))?
            })
            .collect();
        assert!(defined.contains("earth"), "earth must be a defined body");

        let mut files = Vec::new();
        super::tests::collect_json(
            std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../web/public/worlds"
            )),
            &mut files,
        );
        assert!(
            !files.is_empty(),
            "no world files — this guard scans nothing"
        );

        for f in &files {
            let text = std::fs::read_to_string(f).expect("readable world");
            let w: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(body) = w.get("body").and_then(|b| b.as_str()) else {
                continue;
            };
            if !defined.contains(body) {
                continue; // a body the engine has no definition for may still describe itself
            }
            assert!(
                w.get("surface").is_none(),
                "{}: names body `{body}`, which the engine defines, AND carries its own `surface`.\n\
                 That is a second {body}, free to drift from the first. A world says WHICH body and \
                 WHERE on it; assets/bodies/{body}.json says what its surface IS.",
                f.display()
            );
        }
    }

    /// **The biome map is applied by ONE piece of code.**
    ///
    /// It was written twice — identically — inside `Terra::load_world` and
    /// `OrbitDemo::load_earth_surface`. Nothing had diverged, and that is what made it dangerous: the
    /// foliage change landed in the DATA, so both copies picked it up and the duplication stayed
    /// invisible. Phenology is the change that would not be so kind, because it makes a biome's
    /// material depend on the date — two copies, and one Earth turns while the other does not.
    ///
    /// So the mapping lives in `Surface::biome_mixtures` and this counts the implementations rather
    /// than trusting that nobody re-types eight obvious lines.
    #[test]
    fn one_piece_of_code_turns_a_land_cover_class_into_a_material() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut src = String::new();
        super::material_property_tests::collect_rs(std::path::Path::new(dir), &mut src);
        // ★ Count the MAPPING itself, not a material name. The first version of this test counted
        // `index_of(mats, "granite")` — the fallback — and went red the moment a texture test looked
        // granite up for unrelated reasons. A guard that fires on a coincidence teaches people to
        // widen it, which is how a guard dies.
        let defs = src.matches("pub fn biome_mixtures(").count();
        assert_eq!(
            defs, 1,
            "`biome_mixtures` is defined {defs} times; a land-cover class must become materials in \
             exactly ONE place. Two scenes each doing it their own way is two Earths waiting to \
             happen (Law II) — it was written twice before, identically, and nothing noticed."
        );
        let calls = src.matches(".biome_mixtures(").count();
        assert_eq!(
            calls, 2,
            "expected exactly one call from each scene that draws a surface, found {calls}. Fewer \
             means a scene builds its biome map some other way; more means somewhere is asking twice."
        );
    }
}

/// ★★ **A SHADER NOBODY COMPILES IS NOT A FEATURE — it is a claim** (docs/46 row 41, docs/66).
///
/// `shaders/sky.wgsl` sat in this repo for weeks describing an honest Rayleigh sky, and it was in no
/// `include_str!` at all: the scene it was written for was deleted in July, its successor in August,
/// and Terra never had one. Every test about the atmosphere passed the whole time, because the tests
/// asked whether the OPTICS were right and nothing asked whether anything ran them. Meanwhile Robin was
/// looking at a daylight frame of lit grass under a black starfield and asking why the ground seemed to
/// be rendered "without taking available light into account".
///
/// This is that question, asked by a machine, every build. It is the docs/48 pattern — *the law is
/// built and proven, then wired into one place or none* — turned into a gate.
#[cfg(test)]
mod compiled_shader_tests {
    /// Every `shaders/*.wgsl` is named by at least one `include_str!` in the crate.
    ///
    /// ★ VERIFIED BY MAKING IT FAIL, which here meant simply running it: on its first run it caught
    /// TWO orphans, not one — `rayleigh.wgsl`, which had become one minutes earlier when the ground
    /// started marching the real integral, and `particles.wgsl`, superseded by `matter.wgsl` and left
    /// behind since **July**. Both are deleted. Deleting the last consumer of a shader is now a
    /// decision someone has to make out loud rather than a silence that survives for weeks.
    ///
    /// (The reverse direction — an `include_str!` naming a file that is gone — is checked below for
    /// completeness, but rustc gets there first: removing a shader that is still included fails the
    /// BUILD. Confirmed by moving `atmos.wgsl` aside and watching the compile go red.)
    #[test]
    fn every_shader_is_compiled_by_something() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut src = String::new();
        super::material_property_tests::collect_rs(&root.join("crates/engine/src"), &mut src);

        let mut orphans = Vec::new();
        let mut shaders = Vec::new();
        for entry in std::fs::read_dir(root.join("shaders"))
            .expect("shaders/ exists")
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "wgsl") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            shaders.push(name.clone());
            if !src.contains(&format!("shaders/{name}")) {
                orphans.push(name);
            }
        }
        assert!(
            !shaders.is_empty(),
            "no shaders found — this gate is pointing at the wrong directory, which is worse than \
             having no gate (verify a gate by making it fail)"
        );
        assert!(
            orphans.is_empty(),
            "these shaders are compiled by NOTHING: {orphans:?}. A shader in no `include_str!` is a \
             feature that does not exist while reading as though it does — `sky.wgsl` was one for \
             weeks (docs/46 row 41). Wire it up or delete it."
        );

        // The other direction: nothing may `include_str!` a shader that is gone.
        for (i, _) in src.match_indices("shaders/") {
            let rest = &src[i..];
            let end = rest.find(".wgsl").map(|e| e + 5).unwrap_or(0);
            if end == 0 {
                continue;
            }
            let named = rest[..end].trim_start_matches("shaders/").to_string();
            assert!(
                shaders.contains(&named),
                "an `include_str!` names shaders/{named}, which does not exist"
            );
        }
    }
}

/// **THE SEPARATION SWEEP — matter, assembly, engine, viewer** (docs/69).
///
/// Robin, 2026-08-09: *"I think we need to do a rigor sweep based on the assembly/matter/engine/viewer
/// paradigm and make sure they are all separated out correctly and tested properly."* This module is
/// the machine-checkable part of that sweep. The rest — the audit, with its evidence — is docs/69, and
/// the open violations are docs/46 rows 52-56.
///
/// The boundary each test defends is stated on the test, because a gate whose reason is elsewhere is a
/// gate the next person deletes.
#[cfg(test)]
mod separation_tests {
    /// ★★★ **THE MODEL MUST NOT INVENT A VIEWER.**
    ///
    /// docs/68, in Robin's words: *"the viewport decides RESOLUTION, and resolution is a request the
    /// renderer MAKES of the model, not a decision it makes FOR it."* A model module that calls
    /// `ResolutionController::default()` has done the opposite — it has conjured a viewer with a
    /// declared 1 mrad eye and answered a question nobody asked it, which is Law IV inverted: the
    /// resolution of an imaginary camera decides what the world resolves into.
    ///
    /// It is also how `FLORA_ALT_M = 300` happened — one declared cutoff for every plant, wrong by a
    /// factor of forty against the real viewport (docs/46 row 49). `render::Fidelity` is the shape of
    /// the fix: derived from the viewport that frame was actually projected with.
    ///
    /// ★ This is a RATCHET, not a ban. Every site below is DEBT, listed with its count. Adding one
    /// fails the build; removing one also fails, so the list cannot rot into a lie. Fix a site and
    /// delete its line.
    #[test]
    fn the_model_does_not_invent_a_viewer() {
        // (file, how many `ResolutionController::default()` it still contains)
        const DEBT: &[(&str, usize)] = &[
            // Ballistic arc and impact-site machinery, each sizing its own detail against an
            // imagined eye rather than being told what is looking.
            ("arc.rs", 2),
            ("site.rs", 1),
            // The generated-relief octave count — the same number docs/46 row 53 shows the mesh and
            // the model already disagreeing about.
            ("surface_detail.rs", 1),
            ("terra/ground_cap.rs", 1),
            // The scenes. These are the ones with a real viewport in hand, so they have the least
            // excuse; `render::Fidelity` already exists for exactly this.
            ("lib.rs", 3),
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found: Vec<(String, usize)> = Vec::new();
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, usize)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, root, out);
                } else if p.extension().is_some_and(|x| x == "rs")
                    && !p.ends_with("resolution.rs")
                    && !p.ends_with("laws.rs")
                {
                    let Ok(t) = std::fs::read_to_string(&p) else {
                        continue;
                    };
                    let n = t.matches("ResolutionController::default()").count();
                    if n > 0 {
                        let rel = p
                            .strip_prefix(root)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/");
                        out.push((rel, n));
                    }
                }
            }
        }
        walk(&root, &root, &mut found);
        found.sort();
        let mut want: Vec<(String, usize)> =
            DEBT.iter().map(|(f, n)| (f.to_string(), *n)).collect();
        want.sort();
        assert!(
            !found.is_empty(),
            "no sites found at all — this gate is pointing at the wrong directory, which is worse \
             than having no gate. Verify a gate by making it fail."
        );
        assert_eq!(
            found, want,
            "\nThe model invents a viewer in a place this list does not admit to.\n\
             If you ADDED one: don't. Ask the renderer what is looking — `render::Fidelity::of_view` \
             derives angular resolution from the real viewport (docs/68 step 2).\n\
             If you REMOVED one: thank you — delete its line from DEBT so the ratchet tightens.\n"
        );
    }

    /// ★★ **A RENDER SHADER MAY SHAPE LIGHT; IT MAY NOT SHAPE MATTER.**
    ///
    /// Robin, 2026-08-09: *"The engine understands craters — radius, energy, impact, velocity. The
    /// renderer should be blissfully free of these calculations and just render the world as best it
    /// can, represent what the assemblies/models are telling it."* And the governing form (docs/68
    /// §1b): the renderer may approximate HOW it shows what the engine says, never WHAT it says.
    ///
    /// The line this draws is not "no maths in shaders" — `sph_step.wgsl`, `particle_step.wgsl` and
    /// `bh_gravity.wgsl` are the ENGINE running on the GPU, which is a processor and not a renderer,
    /// and `atmos.wgsl` is light transport, which is the renderer's own realm. The line is that a
    /// VERTEX or FRAGMENT shader must not compute where matter IS. `globe.wgsl` does: `crater_sink`
    /// deforms the surface by an excavation profile the model has no record of, so the ground you can
    /// see inside a bowl and the ground the model reports differ by the whole crater depth.
    ///
    /// ★ RATCHET, same rules as above: the debt is named and cannot grow.
    #[test]
    fn a_render_shader_does_not_move_matter() {
        // ★★★ **A LIST OF WHAT IS ALLOWED CANNOT CATCH WHAT IS ADDED.** The first version of this gate
        // compared a DEBT list against the tree, which worked while the list had an entry in it — and
        // the moment `crater_sink` was removed and the list went empty, re-adding a matter-moving
        // function PASSED, because empty equalled empty. Caught by re-running the mutation after
        // closing the debt, which is the only reason it is not still there.
        //
        // So the gate scans for the CONCEPT instead: a render shader must not define a function whose
        // name says it moves, deforms or excavates matter. Names are a proxy, and an honest one — the
        // failure mode this guards is somebody writing the physics where it is convenient, and they
        // will call it what it is.
        const MATTER_VERBS: &[&str] = &[
            "crater", "sink", "displace", "excavate", "deform", "erode", "subside", "bulge",
            "settle", "collapse",
        ];
        // Declared exceptions, with why. EMPTY as of 2026-08-12: `globe.wgsl::crater_sink` was the last
        // one and the excavation now comes from `terra::globe_mesh::SurfaceSampler` (docs/46 row 54).
        const ALLOWED: &[(&str, &str)] = &[];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shaders");
        let mut offenders: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        for e in std::fs::read_dir(&root).expect("shaders/ exists").flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "wgsl") {
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            // Compute shaders are the ENGINE running on the GPU — a processor, not a renderer — so
            // they are out of scope. `atmos.wgsl` is light transport, which IS the renderer's realm.
            if text.contains("@compute") {
                continue;
            }
            scanned += 1;
            for line in text.lines() {
                let t = line.trim_start();
                // Only DEFINITIONS, so a comment explaining what used to be here does not trip it.
                let Some(rest) = t.strip_prefix("fn ") else {
                    continue;
                };
                let fname = rest.split('(').next().unwrap_or("").trim().to_lowercase();
                if MATTER_VERBS.iter().any(|v| fname.contains(v))
                    && !ALLOWED.iter().any(|(s, f)| *s == name && *f == fname)
                {
                    offenders.push(format!("{name}::{fname}"));
                }
            }
        }
        assert!(
            scanned > 0,
            "no render shaders scanned — this gate is pointing at the wrong directory, which is worse \
             than having no gate. Verify a gate by making it fail."
        );
        assert!(
            offenders.is_empty(),
            "\nThese RENDER shaders define functions that move matter: {offenders:?}\n\
             The engine states where matter is; the renderer draws it (docs/68 §1b, docs/46 row 54). \
             `globe.wgsl::crater_sink` was the last of these — the excavation now comes from \
             `terra::globe_mesh::SurfaceSampler`, so the surface arrives already dug and the vertex \
             shader states nothing about what happened to it.\n\
             If this is genuinely light and not matter, rename it so; if it is matter, it belongs in \
             the model.\n"
        );
    }
}

/// ★★★ **THE RULES, APPLIED TO EVERY MATERIAL** (Robin, 2026-08-21: *"all of these rules should be
/// applied to all materials in the engine"*).
///
/// Each of these started as a defect found by hand in one or two materials while building something
/// else. A defect found by hand is found once; a defect found by a gate is found forever, and in the
/// 897 numeric properties nobody has looked at yet. They are stated as laws over the WHOLE catalogue
/// rather than fixed where they were noticed.
#[cfg(test)]
mod catalogue_tests {
    use serde_json::Value;

    fn catalogue() -> Vec<Value> {
        let json: Value = serde_json::from_str(crate::materials::MATERIALS_JSON).expect("parses");
        json["materials"].as_array().expect("array").clone()
    }
    fn num(m: &Value, block: &str, key: &str) -> Option<f64> {
        m.get(block)?.get(key)?.as_f64()
    }

    /// ★★★ **A MATERIAL ID MUST NAME ONE MATERIAL — THE MEMBER OR THE BULK, NEVER BOTH.**
    ///
    /// `granular::contact_from_material` calls itself *"the ONE place where 'what the matter IS'
    /// becomes 'how it collides'"*, and it reads the plain `youngs_modulus`. So whatever that field
    /// holds is what a single grain, blade or stem is made of, everywhere in the engine.
    ///
    /// Two entries put the **aggregate's** compliance there instead of the **member's**, and both say
    /// so in their own notes while the field goes on lying to every caller:
    ///
    /// - `grass`: `youngs_modulus` 5.0e6 Pa is the soil MAT; `youngs_modulus_blade` is 1.06e9 — **212×**.
    ///   Its notes read *"Two stiffnesses coexist: soft soil mat (~MPa) vs stiff strong blades."*
    /// - `straw`: `youngs_modulus` 1.5e5 Pa is the loose HAY MASS; `youngs_modulus_stem` is 5.67e9 —
    ///   **37,800×**.
    ///
    /// ★★ **The straw case is also circular.** A haystack's bulk compliance is what `pile::settle`
    /// exists to PRODUCE — members stacking, bridging and settling under gravity. Feeding it back in as
    /// the members' own stiffness makes the emergent quantity an input to itself (Law V: every number
    /// traces to physics or is a flagged IOU; Law III: the aggregate is computed, not declared).
    ///
    /// The ratio bound is deliberately loose — **30×** — because it is not a tuned threshold but a
    /// statement that a member and its bulk are not the same substance. Real anisotropy and real
    /// tissue variation live well inside one decade; three or four decades is a different material
    /// wearing the same id.
    #[test]
    fn a_material_id_names_one_material_not_a_member_and_its_bulk() {
        // The member-scale moduli the catalogue already records, in the order a member would claim them.
        const MEMBER_MODULI: [&str; 3] = [
            "youngs_modulus_blade",
            "youngs_modulus_stem",
            "youngs_modulus_culm",
        ];
        const MAX_RATIO: f64 = 30.0;

        let mut offenders = Vec::new();
        for m in catalogue() {
            let id = m["id"].as_str().unwrap_or("?").to_string();
            let Some(generic) = num(&m, "mechanical", "youngs_modulus") else {
                continue;
            };
            if generic <= 0.0 {
                continue;
            }
            for key in MEMBER_MODULI {
                let Some(member) = num(&m, "mechanical", key) else {
                    continue;
                };
                let ratio = (member / generic).max(generic / member);
                println!(
                    "  {id:10} youngs_modulus {generic:.3e} vs {key} {member:.3e} — {ratio:.0}x"
                );
                if ratio > MAX_RATIO {
                    offenders.push(format!(
                        "`{id}`: youngs_modulus {generic:.3e} Pa vs {key} {member:.3e} Pa ({ratio:.0}x). \
                         `contact_from_material` gives every member of this material the FORMER."
                    ));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a material id must name one material — the member or the bulk, never both:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// ★★ **A VALUE MUST LIE INSIDE ITS OWN DECLARED RANGE.**
    ///
    /// Where an entry records both `mechanical.x` and `ranges.x`, the value has to be in the range it
    /// claims for itself. Catches unit slips and transcription errors — the class that produced a
    /// 38,000x error elsewhere in this catalogue — with no judgement and no threshold.
    #[test]
    fn every_value_lies_inside_the_range_its_own_entry_declares() {
        let mats = catalogue();
        let mut checked = 0;
        let mut bad = Vec::new();
        for m in &mats {
            let Some(ranges) = m.get("ranges").and_then(|r| r.as_object()) else {
                continue;
            };
            for (k, v) in ranges {
                let Some(arr) = v.as_array() else { continue };
                if arr.len() != 2 {
                    continue;
                }
                let (Some(a), Some(b)) = (arr[0].as_f64(), arr[1].as_f64()) else {
                    continue;
                };
                // `ranges` keys sometimes carry a unit suffix the property itself does not.
                let mut base = k.as_str();
                for suf in ["_pa", "_kg_m3", "_m3", "_ms"] {
                    base = base.strip_suffix(suf).unwrap_or(base);
                }
                let Some(val) = ["mechanical", "thermal", "optical"]
                    .iter()
                    .find_map(|blk| num(m, blk, base))
                else {
                    continue;
                };
                checked += 1;
                let (lo, hi) = (a.min(b), a.max(b));
                if val < lo || val > hi {
                    bad.push(format!(
                        "  {} :: {base} = {val:.6e} outside [{lo:.6e}, {hi:.6e}]",
                        m["id"].as_str().unwrap_or("?")
                    ));
                }
            }
        }
        assert!(
            checked > 50,
            "expected a rich catalogue, checked only {checked}"
        );
        assert!(
            bad.is_empty(),
            "a value must lie inside the range its own entry declares:\n{}",
            bad.join("\n")
        );
    }

    /// ★★★ **BEAM PROPERTIES ONLY ON THINGS THAT CAN BE A BEAM.**
    ///
    /// ★★ **CORRECTED 2026-08-22: the outcome was right and the stated reason was FALSE.** This gate
    /// used to justify itself with *"a granular bed has no bending strength at all — a heap of sand
    /// cannot be a cantilever."* An adversarial audit of the exclusions killed that: **soil-cement has
    /// an ASTM standard for exactly this** (D1635, flexural strength of a simple soil-cement beam under
    /// third-point loading), and snow-slab mechanics measures a snow slab's bending and tensile
    /// strength. A cemented granular medium genuinely IS a beam.
    ///
    /// The criterion that actually decides comes from the Wood Handbook's own definition:
    ///
    /// > *"Modulus of rupture … is not a true stress because the formula by which it is computed is
    /// > valid only to the elastic limit."*
    ///
    /// MoR is `σ = M·c/I` evaluated at the COLLAPSE moment. It is a fiction, and only a SMALL fiction
    /// for a material that **stays nearly elastic to failure**. That is the real test, and it excludes
    /// a ductile metal (which yields extensively first, making `Mc/I` a large fiction) for a quite
    /// different reason than it excludes a fluid (which supports no static shear, so there is no
    /// bending moment and no outer fibre at all).
    ///
    /// Phase remains the right PROXY for this catalogue, because every granular entry here is LOOSE —
    /// sand, gravel, clay, dirt, snow. It would be the wrong proxy the moment a cemented one is added,
    /// and whoever adds it should widen this gate rather than delete it.
    #[test]
    fn only_things_that_can_be_a_beam_carry_beam_strengths() {
        let mut bad = Vec::new();
        for m in catalogue() {
            let phase = m["phase"].as_str().unwrap_or("");
            if !matches!(phase, "gas" | "liquid" | "granular") {
                continue;
            }
            for k in ["modulus_of_rupture", "yield_strength"] {
                if num(&m, "mechanical", k).is_some_and(|v| v > 0.0) {
                    bad.push(format!(
                        "  {} (phase {phase}) carries {k}",
                        m["id"].as_str().unwrap_or("?")
                    ));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "a gas, a liquid and a granular bed cannot be a beam:\n{}",
            bad.join("\n")
        );
    }

    /// ★★★ **A YIELD STRENGTH MUST BE JUSTIFIED BY ITS OWN ENTRY.**
    ///
    /// `yield_strength` was PROMOTED, not sourced (docs/46 row 65): the catalogue's
    /// `compressive_strength` carries the yield for a ductile metal and a genuine crushing strength for
    /// a brittle one, and nothing in the field told a reader which. Three entries said so in their own
    /// notes and got a `yield_strength`; `cast_iron` and `nickel` did not and did not.
    ///
    /// This keeps that honest: a material may carry a yield only if its notes say why. It is the rule
    /// that stopped `nickel` acquiring one because its number merely LOOKED like a published yield —
    /// which is the pattern-match this catalogue exists to prevent.
    #[test]
    fn a_yield_strength_is_only_carried_where_the_entry_says_why() {
        let mut bad = Vec::new();
        for m in catalogue() {
            if num(&m, "mechanical", "yield_strength").is_none() {
                continue;
            }
            let notes = m["notes"].as_str().unwrap_or("").to_lowercase();
            if !notes.contains("yield") {
                bad.push(format!(
                    "  {} carries a yield_strength and never says why",
                    m["id"].as_str().unwrap_or("?")
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "a promoted number must carry its justification:\n{}",
            bad.join("\n")
        );
    }
}

/// ★★★ **THE EARTH IS NOT TILTED, AND NOTHING COULD TELL** (docs/46 row 39).
///
/// These tests are written BEFORE the fix, and they FAIL. That is deliberate. Row 39 has been open
/// since 2026-08-04 with the seasonal tests passing the whole time, and it records exactly why they
/// could not catch it: *"the seasonal tests in `solar` all pass BECAUSE they read the orbit-side
/// value — they would pass identically with the body upright."*
///
/// The body IS upright. `lib.rs` builds the spin as `DVec3::new(0.0, 0.0, 1.0)` and says so in its own
/// comment: *"spin axis ⊥ the orbital (x-y) plane."* So tilting it now would be unverifiable in either
/// direction, because no test asks. Fix the instrument first.
#[cfg(test)]
mod obliquity_tests {
    use super::material_property_tests::collect_rs;

    fn engine_src() -> String {
        let mut blob = String::new();
        collect_rs(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut blob,
        );
        assert!(blob.len() > 100_000, "the source scan found almost nothing");
        blob
    }

    /// ★★★ **ONE OBLIQUITY, ONE OWNER.**
    ///
    /// Earth's axial tilt is the reason there are seasons, and the engine currently states it in FOUR
    /// places that cannot disagree loudly enough to be noticed:
    ///
    /// - `orbit.rs` — `23.439` with the secular term, inside a local `let`
    /// - `solar.rs` — `23.44`, twice, one of them under a comment claiming it *"keeps ONE source for
    ///   the obliquity"*, which is the opposite of what it does
    /// - `lib.rs` — `0.0`, implicitly, by building the spin axis perpendicular to the orbital plane
    ///
    /// A number stated four times is four numbers. This scans for the literal rather than for a name,
    /// because the whole problem is that it has no name — there is no `obliquity_rad()` anywhere to
    /// grep for, which is why nothing reads it and nothing can contradict it.
    #[test]
    fn the_obliquity_is_stated_in_exactly_one_place() {
        let blob = engine_src();
        // Count source lines that state Earth's tilt as a bare literal. Test fixtures legitimately
        // quote the solstice declination, so only non-test statements of the constant count.
        let sites: Vec<&str> = blob
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.starts_with("//")
                    && !t.starts_with("///")
                    && (t.contains("23.439") || t.contains("23.44"))
                    // a fixture asserting the solstice declination is data, not a statement of tilt
                    && !t.contains("solstice")
            })
            .collect();
        assert_eq!(
            sites.len(),
            1,
            "Earth's obliquity is stated in {} places; a number stated more than once is more than \
             one number. Give it a single public owner and have every site read it.\n{}",
            sites.len(),
            sites
                .iter()
                .map(|l| format!("  {}", l.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// ★★★ **THE BODY IS TILTED BY IT — not just the ephemeris.**
    ///
    /// Written first as a source scan, because there was nothing to assert against: the tilt had no
    /// NAME, which is exactly why nothing read it and nothing could contradict it.
    /// `orbit::obliquity_rad` now owns it and `orbit::spin_axis_for_obliquity` builds the axis from
    /// it, so this is a physics assertion rather than a grep.
    #[test]
    fn the_earth_body_is_tilted_and_not_merely_the_ephemeris() {
        let eps = crate::orbit::obliquity_rad(946_728_000.0); // J2000
        let axis = crate::orbit::spin_axis_for_obliquity(eps);
        let orbital_normal = glam::DVec3::Z; // the orbital plane is x-y

        let tilt_deg = axis.angle_between(orbital_normal).to_degrees();
        assert!(
            (tilt_deg - 23.439).abs() < 1.0e-6,
            "the spin axis must lean from the orbital normal by the obliquity: {tilt_deg:.6}"
        );
        assert!(
            (axis.length() - 1.0).abs() < 1.0e-12,
            "a direction is a unit vector"
        );

        // ★ THE NEGATIVE CONTROL, and it is the whole point: at zero obliquity the axis IS the
        // orbital normal — exactly the `DVec3::new(0.0, 0.0, 1.0)` the scene used to hardcode. The
        // untilted Earth was not a different model, it was THIS model with the tilt left out, which
        // is precisely why no test could see the difference.
        let upright = crate::orbit::spin_axis_for_obliquity(0.0);
        assert!(
            (upright - orbital_normal).length() < 1.0e-15,
            "zero tilt must reproduce the old hardcoded axis exactly, or this is a different model"
        );

        // ★★ IT DRIFTS, which is why the owner takes a time. 0.47 arcsec/yr is small, real, and
        // impossible to express with the constant this replaced.
        let millennium = 946_728_000.0 + 1000.0 * 365.25 * 86_400.0;
        let then = crate::orbit::obliquity_rad(millennium).to_degrees();
        let now = eps.to_degrees();
        println!("obliquity: {now:.5} deg at J2000 -> {then:.5} deg a millennium later");
        assert!(
            then < now && (now - then) > 0.1,
            "the obliquity must decrease measurably over a millennium: {now:.5} -> {then:.5}"
        );
    }

    /// ★★★ **THE SEASONS MUST VANISH WHEN THE TILT DOES** — the claim row 39 has wanted since
    /// 2026-08-04 and that could not be WRITTEN until now.
    ///
    /// This is the real physics statement, and it is a counterfactual: an Earth with no obliquity has
    /// no seasons anywhere, at any latitude, on any date. Until `orbit` and `solar` could be handed a
    /// hypothetical tilt, there was nowhere for that counterfactual to enter — every seasonal function
    /// looked the tilt up for itself, so no test could ask "and what if it were zero?" That is exactly
    /// how an upright Earth survived twenty days of a passing suite.
    #[test]
    fn the_seasons_vanish_when_the_tilt_does() {
        // Four dates across a year, and latitudes from equator to well inside the Arctic.
        const JAN: f64 = 1_704_067_200.0; // 2024-01-01
        const APR: f64 = 1_711_929_600.0;
        const JUL: f64 = 1_719_792_000.0;
        const OCT: f64 = 1_727_740_800.0;

        for lat in [0.0, 23.5, 45.0, 60.0, 70.0] {
            for t in [JAN, APR, JUL, OCT] {
                // ★ AN UPRIGHT EARTH: the sun sits on the equator all year, every day is 12 hours
                // everywhere, and there is no season to be part-way through.
                let (dec, _) = crate::orbit::solar_declination_ra_at_obliquity(t, 0.0);
                assert!(
                    dec.abs() < 1.0e-12,
                    "with no tilt the sun must never leave the equator: dec {dec:.3e} rad at \
                     lat {lat}, t {t}"
                );
                let flat = crate::solar::senescence_fraction_at_obliquity(lat, t, 0.0);
                assert!(
                    flat.abs() < 1.0e-9,
                    "an untilted Earth has no season anywhere: senescence {flat:.6} at lat {lat}"
                );
            }
        }

        // ★★ AND THE REAL EARTH DOES HAVE ONE — otherwise the assertion above would be satisfied by a
        // function that always returns zero, which is the failure mode this whole row is about.
        let eps = crate::orbit::obliquity_rad(JUL);
        let north_summer = crate::solar::senescence_fraction_at_obliquity(60.0, JUL, eps);
        let north_winter = crate::solar::senescence_fraction_at_obliquity(60.0, JAN, eps);
        println!(
            "lat 60: senescence {north_summer:.3} in July vs {north_winter:.3} in January \
             (tilt {:.3} deg)",
            eps.to_degrees()
        );
        assert!(
            north_winter - north_summer > 0.5,
            "a tilted Earth must show a real season at 60 deg: {north_summer:.3} -> {north_winter:.3}"
        );

        // ★ The tropics barely senesce even WITH the tilt, which the model predicts for free and is
        // why the counterfactual above is a meaningful control rather than a tautology.
        let tropics = crate::solar::senescence_fraction_at_obliquity(0.0, JAN, eps);
        assert!(
            tropics.abs() < 1.0e-9,
            "the equator has no season either way: {tropics:.6}"
        );
    }
}
