//! Integrity engine core.
//!
//! Phase 2: real Newtonian **self-gravity** from the world's aggregate voxel mass, and a rigid
//! sphere that falls under it (`F = ma`) and rests on the terrain. The layered voxel world and its
//! renderer come from Phase 1; densities in `data/materials.json` are now physically active — summed
//! voxel mass produces the gravitational field the sphere obeys.
//!
//! ## Scale & time
//! The Phase-1 test world is ~96 m across, so its real surface gravity is asteroid-scale micro-g
//! (~1e-5 m/s²) — correct physics, but far too slow to watch. `G` stays real; instead a **time
//! scale** fast-forwards the simulation for viewing (time-lapse, not fake gravity).
//!
//! ## Structure & testing
//! The pure simulation logic (materials, voxel store, mesher, gravity, body) compiles and unit-tests
//! **natively** (`cargo test`). Only the rendering/host layer is gated to the wasm target. TDD is
//! canonical for this project.

// On native builds the sim modules' only non-test consumer (the wasm renderer) is compiled out, so
// their API reads as "unused" there. The wasm build still enforces dead-code detection, and tests
// exercise them. (A future `matter-core` crate split, per docs, removes the need for this.)
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod accretion;
mod aggregate;
mod atmosphere;
mod axle; // docs/47 §3 — the revolute joint: holds a wheel's hub, frees ONE spin axis
mod bhtree;
mod blackbody; // Planck + the CIE observer: colour from temperature, for stars and (a follow-up) hot ejecta
mod body;
/// The physics clock: real elapsed time into whole fixed steps, so a slow machine simulates the same
/// world as a fast one. Not scene code — every scene needs it, and there is one right answer.
pub mod clock;
mod damage;
mod emission;
mod eos;
mod geo; // THE lat/lon <-> direction conversion (one place, so the world cannot be mirrored in six)
/// GPU direct-sum self-gravity: the exact per-particle N-body force on the GPU, dispatched to when a
/// foreseen collision materialises particles above the CPU/GPU knee.
mod gpu_gravity;
/// docs/52 — acquiring a GPU with no browser and no canvas: the standalone engine's device entry point.
/// Native only; in the browser the device comes from the canvas context.
#[cfg(not(target_arch = "wasm32"))]
pub mod gpu_host;
mod gpu_layout; // docs/47 — GPU repr(C) layouts, pinned to the shader by test
/// docs/33 — THE GPU particle container (granular). Lifted out of `#[cfg(wasm32)] mod app`, where a
/// scene-agnostic container looked like the terrain scene's private machinery and no native build
/// compiled it. Sibling of `gpu_sph`: the two containers converge, their solvers stay specialized.
mod gpu_particles;
/// docs/50 — THE one GPU particle container: allocation, capacity/count, and the two-phase async
/// read-back, shared by the granular and SPH pipelines. Their SOLVERS stay specialized (docs/46 §1).
mod gpu_store;
mod granular;
mod grid; // docs/47 §1 — the hierarchical spatial hash: no global cell size
          // WebGPU host for sph_step.wgsl. Compiled on EVERY target, deliberately: it used to be wasm-only, but the
          // only thing that actually required wasm was one `Rc<Cell<bool>>` in a `map_async` callback (see
          // `gpu_sph::GpuSph::readback_ready`). That accident hid ~700 lines of shipping GPU host code from native
          // `cargo check`/`cargo test` — the very trap CLAUDE.md rule 3 flags ("no in-crate tests") and that once
          // shipped a non-compiling commit. Building it natively costs nothing (wgpu's types exist without a
          // backend) and puts its shader-facing layouts under the suite. Running still needs a browser.
mod gpu_sph;
pub mod gravity;
mod hydrostatic;
mod impact;
mod planet;
/// docs/33 — scene-agnostic render scaffolding (`GpuMesh`, `UniformSlot`, `Camera`, the uniform PODs and
/// their helpers). Lifted out of `#[cfg(wasm32)] mod app`: all three scenes use these identically, so
/// they were never scene code, and living there kept them out of every native build.
mod render;
pub mod solar; // the engine's time signal: what the sky is doing at a place, from tilt + orbit
/// What the engine is holding, as it must be drawn — the one physics→picture mapping (docs/50).
pub use render::Drawn;
pub mod arc; // the out-and-back demo arc: one continuous camera path, surface <-> celestial, pacing derived
pub mod assembly; // docs/64 - matter with a shape, in a place
pub mod ballistics; // docs/46 row 33 - a confined gas doing work on a moving boundary
/// docs/53 — the engine driven by a DEFINITION: builds the world, applies declared matter events through
/// the shared primitives, and steps. No scene struct, no canvas. This is what re-consumes the systems
/// deleting terrain orphaned (docs/46 ledger row 15).
pub mod flight;
/// ONE entry point for "two things met — what does the engine do?". Delegates to the laws that already
/// own each half, so a new scene finds them instead of writing a third path.
pub mod interaction;
mod intercept; // the launch-window solve: release time chosen so the site rotates under the impact
mod isotropy;
#[cfg(test)]
/// docs/00 — the Laws, made checkable: fails the build when a world file declares a quantity that must
/// emerge from matter. Availability of the Laws proved insufficient on its own (2026-07-21).
mod laws;
pub mod materials;
pub mod matter;
mod mesher;
mod neighbors;
mod orbit;
pub mod oxidation; // docs/46 row 31 - rapid oxidation: fires, charges, one reaction
pub mod recohere; // docs/61 — the batch downward rung: a settled particle field re-coheres to ground
pub mod refine; // docs/62, the upward rung: the celestial field initializes the local patch, conserved
pub mod resolution; // docs/44 — resolution by necessity: the quasi-static admission test
pub mod simulation;
pub mod site; // docs/62, the camera-driven materialization trigger and its site (wires refine.rs)
/// docs/49 — surface detail that follows the camera CONTINUOUSLY. The consumer
/// `ResolutionController::camera_grain_radius` never had.
mod sky;
mod surface_detail;
pub mod terra;
mod tides; // docs/43 — worlds-as-data: the world schema (+ later raster/mesh/camera). The wasm `Terra` scene
           // struct lives in `mod app` below to reuse its render helpers.
mod texture;
/// Test-only: the ONE WGSL↔Rust layout checker, shared by every module with a `#[repr(C)]` shader mirror.
#[cfg(test)]
mod wgsl_layout;
pub mod world;

#[cfg(target_arch = "wasm32")]
pub use app::OrbitDemo;

/// World metres spanned by ONE screen pixel at the focal plane (distance `dist_m` from the eye),
/// for a perspective camera with vertical field of view `fov_y` (radians) rendered into a viewport
/// `viewport_h` pixels tall. Pure frustum geometry: the visible slice at `dist_m` is
/// `2·dist_m·tan(fov_y/2)` metres tall, spread over `viewport_h` pixels. Both the terrain scene
/// (world units already metres) and the space scene (convert display units → metres first) feed the
/// HUD scale bar through this one function, so "scale" means the same thing on every screen.
pub(crate) fn metres_per_pixel_at(dist_m: f64, fov_y: f64, viewport_h: f64) -> f64 {
    if viewport_h <= 0.0 {
        return 0.0;
    }
    2.0 * dist_m * (fov_y * 0.5).tan() / viewport_h
}

/// The rendering + browser-host layer. wasm/`wgpu`-only; excluded from native builds and tests.
/// The rasters a body's surface needs, so a SCENE never hardcodes Earth's continents. The engine holds
/// the definitive body; the host just fetches what it names. Returns `[]` for a body with no surface.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn body_surface_urls(id: &str) -> String {
    let urls = crate::planet::body(id)
        .surface
        .map(|s| {
            [s.landmask_url, s.elevation_url, s.landcover_url]
                .into_iter()
                .map(|u| u.unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::to_string(&urls).unwrap_or_else(|_| "[]".into())
}

/// **How a scene body maps to a DEFINED body** — the "instance of Luna / Terra" rule, and where the
/// engine's refusal to override a scene lives. A scene declares WHICH body (its `profile`) and WHERE and
/// how fast; the body's mass, radius and composition come from `assets/bodies/<id>.json`. These are free
/// (not inside the wasm-only `mod app`) so they can be TESTED natively — a rule nobody can run is a rule
/// nobody keeps.

#[cfg(test)]
mod body_spec_tests {
    use crate::terra::world_def::BodyDef;

    /// **An instance of Luna weighs what Luna weighs — even if a scene tried to say otherwise.** The parse
    /// test forbids the override in the data; this proves the ENGINE would ignore one anyway, so the two
    /// guards agree: a defined body's mass and radius come from its definition, full stop.
    #[test]
    fn a_defined_body_takes_its_definition_not_a_declared_override() {
        // A "moon" body carrying a bogus mass/radius — the engine must ignore both.
        let mut d = BodyDef::default();
        d.profile = Some("moon".into());
        d.mass_kg = Some(1.0); // absurd override
        d.radius_m = Some(1.0);
        let m = super::declared_body_mass(&d);
        let r = super::declared_body_radius(&d);
        assert!(
            (m - crate::planet::body("moon").total_mass()).abs() < 1.0,
            "mass is Luna's, not 1 kg"
        );
        assert!(
            (r - 1.737e6).abs() < 1.0e4,
            "radius is Luna's, not 1 m (got {r})"
        );

        // Earth likewise.
        let mut e = BodyDef::default();
        e.profile = Some("earth".into());
        assert!(
            (super::declared_body_radius(&e) - 6.371e6).abs() < 1e3,
            "Terra's radius from the definition"
        );

        // A BARE point mass (no profile) keeps its declared mass — a scene may still place one.
        let mut point = BodyDef::default();
        point.mass_kg = Some(5.0e20);
        assert_eq!(
            super::declared_body_mass(&point),
            5.0e20,
            "an undefined body keeps its declared mass"
        );
    }
}

/// The defined body a scene body refers to, or `None` for a bare point mass with no definition.
fn body_definition(profile: Option<&str>) -> Option<crate::planet::LayeredBody> {
    match profile {
        Some("sun") | Some("earth") | Some("moon") | Some("theia") | Some("proto-earth") => {
            Some(crate::planet::body(profile.unwrap()))
        }
        _ => None,
    }
}

/// A declared body's mass (kg). An instance of a defined body takes the DEFINITION's mass; an explicit
/// `mass_kg` is honoured only for a body with no definition — so a scene can place a generic point mass,
/// but can never OVERRIDE what Luna weighs.
fn declared_body_mass(d: &crate::terra::world_def::BodyDef) -> f64 {
    if let Some(def) = body_definition(d.profile.as_deref()) {
        return def.total_mass();
    }
    d.mass_kg.unwrap_or(0.0)
}

/// A declared body's radius (m), from its definition, or an explicit `radius_m` for an undefined body.
fn declared_body_radius(d: &crate::terra::world_def::BodyDef) -> f64 {
    if let Some(def) = body_definition(d.profile.as_deref()) {
        return def.radius();
    }
    d.radius_m.unwrap_or(0.0)
}

/// The radius (m) a `"planet"` world's body renders and computes at - the Terra half of the
/// "instance of a defined body" rule. A world that names a defined body (the world-level `body`, or
/// the planet's `profile`) takes the DEFINITION's radius, and any `radius_m` it declared is ignored;
/// an undefined body keeps its declared radius. `None` only when neither source names one.
fn declared_planet_radius(w: &crate::terra::world_def::World) -> Option<f64> {
    let named = w
        .body
        .as_deref()
        .or_else(|| w.planet.as_ref().and_then(|p| p.profile.as_deref()));
    if let Some(def) = body_definition(named) {
        return Some(def.radius());
    }
    w.planet.as_ref().and_then(|p| p.radius_m)
}

#[cfg(test)]
mod one_earth_tests {
    /// **One Earth serves the orbit, the globe and the ground** (docs/59 order-of-work 1; docs/46
    /// row 16). The three shipped paths - the space band's body instance, Terra's placed body, and the
    /// ground patch's host planet - must all read `assets/bodies/earth.json`, to the digit. Not
    /// "close": IDENTICAL, because they are supposed to be reads of one value, and the shipped world
    /// files must carry no private copy for the paths to drift toward.
    #[test]
    fn the_three_scenes_read_one_earth() {
        let def = crate::planet::earth();

        // The SPACE BAND: an Earth instance in a system world resolves to the definition.
        let mut d = crate::terra::world_def::BodyDef::default();
        d.profile = Some("earth".into());
        assert_eq!(
            super::declared_body_radius(&d),
            def.radius(),
            "orbit radius is the definition's"
        );
        assert_eq!(
            super::declared_body_mass(&d),
            def.total_mass(),
            "orbit mass is the definition's"
        );

        // TERRA: the shipped Earth world places the body and inherits its radius - the file itself
        // declares none.
        let json = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/public/worlds/earth/world.json"
        ))
        .expect("shipped Earth world");
        let w = crate::terra::world_def::World::parse(&json).expect("parses");
        assert_eq!(
            super::declared_planet_radius(&w),
            Some(def.radius()),
            "Terra radius is the definition's"
        );
        let p = w.planet.as_ref().expect("planet block");
        assert!(
            p.radius_m.is_none(),
            "the Earth world file must not carry a private radius"
        );
        assert!(p.mass_kg.is_none(), "nor a private mass");

        // The GROUND: the shipped ground world's host planet is the same body.
        let gjson = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/worlds/ground-patch.json"
        ))
        .expect("shipped ground world");
        let sim = crate::simulation::Simulation::from_json(&gjson, crate::materials::load())
            .expect("ground world builds");
        assert_eq!(
            sim.planet_radius_m(),
            def.radius(),
            "ground radius is the definition's"
        );
        assert_eq!(
            sim.planet_mass_kg(),
            def.total_mass(),
            "ground mass is the definition's"
        );
        assert_eq!(
            sim.gravity_ms2(),
            def.gravity_at(def.radius()) as f32,
            "ground gravity emerges from the same body"
        );
    }

    /// A world that names a defined body cannot smuggle a different radius past the engine - the same
    /// refusal `declared_body_radius` makes for system bodies, on the Terra path.
    #[test]
    fn a_planet_world_cannot_override_its_named_bodys_radius() {
        let w = crate::terra::world_def::World::parse(
            r#"{"name":"w","type":"planet","body":"earth","planet":{"radius_m":1.0,"profile":"earth"}}"#,
        )
        .expect("parses");
        assert_eq!(
            super::declared_planet_radius(&w),
            Some(crate::planet::earth().radius()),
            "the definition answers, not the file"
        );
        // A body with NO definition keeps its declared radius - a sandbox planet is still expressible.
        let bare = crate::terra::world_def::World::parse(
            r#"{"name":"bare","type":"planet","planet":{"radius_m":1.0e6}}"#,
        )
        .expect("parses");
        assert_eq!(super::declared_planet_radius(&bare), Some(1.0e6));
    }
}

/// **The live resolution-crossing check** (the wiring the SPH assembly primitive exists for): the first
/// eligible body whose separation from the planet is inside `accretion::resolution_distance` (the point
/// where tidal stress makes "two point masses" a lie), plus its body-centric (offset, relative velocity)
/// in f64 SI. That pair is exactly what the impactor's `gpu_sph::BodyPlacement` carries into
/// `assemble_from_relaxed_n`: TARGET-relative, never heliocentric (f32 collapses at 1.5e11 m).
///
/// `planet` is the index of the body the scene declared as its planet, found by ROLE by the caller
/// (docs/58): no `bodies[1]=Earth` layout assumption, so a scene that ordered its bodies differently is
/// not wrong here. Every other body is checked, the star included -- a star never gets near the
/// threshold, and finding nothing is more honest than special-casing it out. `eligible[i] == false`
/// skips a body (already materialized, or already handed to the SPH).
///
/// Pure and kept OUT of the wasm-only `mod app` so it is natively tested, like the body-spec rules above.
/// (The scene no longer calls it: a drop routes through the SPH engine at release and the Approaching
/// phase hands off at `resolution_distance.max(contact)`. It stays as the natively-tested statement of
/// the crossing law the site trigger mirrors, docs/59.)
#[allow(dead_code)]
fn live_resolution_crossing(
    bodies: &[crate::orbit::Body],
    planet: usize,
    planet_radius_m: f64,
    eligible: &[bool],
    tidal_fraction: f64,
) -> Option<(usize, glam::DVec3, glam::DVec3)> {
    let p = bodies.get(planet)?;
    for i in 0..bodies.len() {
        if i == planet || !eligible.get(i).copied().unwrap_or(true) {
            continue;
        }
        let resolve_at = crate::accretion::resolution_distance(
            p.mass,
            planet_radius_m,
            bodies[i].mass,
            tidal_fraction,
        );
        let offset = bodies[i].pos - p.pos;
        if offset.length() <= resolve_at {
            return Some((i, offset, bodies[i].vel - p.vel));
        }
    }
    None
}

#[cfg(target_arch = "wasm32")]
mod app {
    use crate::mesher::{self, Mesh, Vertex};
    use crate::{materials, matter, texture};
    use glam::{Mat4, Vec3};
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    // Probe / simulation parameters.
    const SPAWN_HEIGHT: f32 = 12.0; // metres of clearance above the surface at spawn
    const SPHERE_RADIUS: f32 = 3.0; // rendered/collision radius — enlarged for visibility (a real
                                    // 5 kg iron ball is ~5 cm; free-fall is size- and mass-independent, so this doesn't affect the
                                    // measured acceleration).
    const SPHERE_MASS: f32 = 5.0; // kg
    const GRAVITY_SOFTENING: f32 = 6.0; // ~ mass-aggregation block size
                                        // The terrain slab is a patch of a planet, so it feels the planet's ~uniform surface gravity
                                        // (down), not the slab's own micro-g self-gravity (docs/22). Self-gravity is demonstrated at
                                        // planetary scale in the space band; here it is negligible vs the planet below. That surface
                                        // gravity is now COMPUTED from planet::earth() (g = GM/R²) at create() — no hardcoded constant.
    const GRAVITY_BLOCK: usize = 8; // voxel aggregation for the mass field (coarser = cheaper queries)
    /// Debris substeps per frame. Higher = densely-packed grains settle cleanly (less residual energy
    /// leak from the explicit integrator) at a proportional GPU cost (docs/23). The probe substeps
    /// itself, sized to its bond stiffness (`Aggregate::stable_substeps`).
    const DEBRIS_SUBSTEPS: u32 = 16;
    const DEFAULT_TIME_SCALE: f32 = 1.0; // real-time: Earth-like surface gravity needs no fast-forward
    /// How far the real Earth-surface cap extends (m). It curves down to a horizon at a finite distance
    /// (√(2·R·h) ≈ 16 km for the default ~20 m eye height), well inside this radius and the render far
    /// plane, so the horizon you see is the planet's true curvature — not a cap edge, not infinity.
    const EARTH_CAP_RADIUS: f32 = 26_000.0;
    /// Render far plane (m) — pushed out from 6 km so the curved cap's horizon is in view. The distant
    /// cap is smooth, so the mild depth imprecision far out is acceptable; the near patch is fine.
    const CAMERA_FAR: f32 = 30_000.0;
    const CAMERA_NEAR: f32 = 0.5;
    // SPACE-BAND scene resolution — DECOUPLED from impact.rs's test-facing DEBRIS_N/CAP_N so the on-screen
    // disk can run at the high N the fluid disk actually needs (the grid + Barnes–Hut of docs/30 made this
    // affordable) WITHOUT dragging the native test suite up to high N. The scene's time-LOD keeps it
    // interactive if a step gets heavy (observable time dilates rather than the frame stalling). Trade
    // on-screen disk richness ↔ browser step-rate by bumping these; keep CAP:DEBRIS ≈ 2:1 (docs/28 item 4).
    /// ~1.5 h of sim time — long enough to cover the collision and its excavation. Past it the event is
    /// over: the dt coarsens for the slow disk aftermath (docs/42) AND de-resolution is allowed to run
    /// (docs/44 §6 — demote on quiescence). ONE definition of "the shock is finished" serves both.
    const SPH_SHOCK_WINDOW_S: f64 = 5400.0;
    const SCENE_DEBRIS_N: usize = 512;
    const SCENE_CAP_N: usize = 1024;
    const SCENE_IMPACT_N: usize = SCENE_DEBRIS_N + SCENE_CAP_N;
    // Render pool for moonlet spheres in the space band (the SPH disk's accreting clumps, docs/42
    // Phase 4, and the geologic-time bodies). Sized to the pool the retired CPU debris cloud used,
    // so the draw budget is unchanged; the physics never reads this.
    /// **1536 is the same NUMBER as `SCENE_IMPACT_N` and a different QUESTION — do not couple them.**
    ///
    /// It looks like a duplicate (512 + 1024 = 1536), and on merging this the first instinct was to write
    /// `= SCENE_IMPACT_N` on Law V grounds. That would have been wrong, and Robin caught it: our side moved
    /// the disk to **GPU** SPH, so `SCENE_IMPACT_N` is a physics particle count, while this is a **render**
    /// pool of uniform slots for drawing moonlet spheres, which Sean sized to match the **CPU** debris pool
    /// his commit retires. Two different questions that agree only because the CPU-era pool happened to
    /// equal the GPU particle count we now run.
    ///
    /// Deriving one from the other would assert a dependency that does not exist and would silently resize
    /// the draw pool whenever anyone tuned GPU resolution. A coincidence documented beats both a bare
    /// literal and a false relationship.
    const MOONLET_UNIS_N: usize = 1536;
    /// Cohesive-bond geometry + stability for the steel probe (`docs/23`). The bond stiffness is the
    /// material's REAL elastic modulus (k = E·L for a lattice of spacing L) — rigidity is cohesive
    /// force, not a fudge. But true iron (E ≈ 2.05e11 → k ≈ 2e11 N/m) would need thousands of explicit
    /// substeps/frame to stay stable; we cap k here and reach true steel only with implicit integration
    /// (flagged). The cap is still ~1000× the old hand-tuned 5e6, so the ball reads as rigid.
    const PROBE_LATTICE: f64 = 1.0; // particle spacing (m)
    const PROBE_STIFFNESS_CAP: f64 = 5.0e9; // N/m — real-time explicit-stability ceiling (flagged)

    /// Granular debris contact (`docs/23`) — the DEM model in `granular.rs`, run on the GPU and TUNED +
    /// verified on real hardware by `tools/gpu-verify`. Grains push apart, stack, settle, and flow to a
    /// slope. The PHYSICS is one grain per 1 m voxel (radius 0.5 ⇒ neighbours touch at rest); the finer
    /// look is a render-only 8× subdivision (`cs_expand`). Values chosen for explicit stability at the
    /// debris substep with coordination z≈6: soft contacts + a normal-force cap + sub-critical damping.
    const CONTACT_RADIUS: f32 = 0.5; // = ½ the 1 m grain spacing ⇒ grains just touch at rest
    const DEBRIS_PART_HALF: f32 = 0.5; // a debris grain's collision half-extent (rests on the ground)
                                       // Stiff (real-ish) contact — kept stable by IMPLICIT integration (1/(1+dt²K) in the shader), not by
                                       // a force cap or a freeze (both removed as fudges). Verified energy-conserving on the 2070
                                       // (tools/gpu-verify scene I: total mechanical energy only ever decreases). A real angle of repose
                                       // emerges from the friction (docs/23).
    const CONTACT_STIFFNESS: f32 = 5.0e5; // normal repulsion (1/s²) per metre of overlap
                                          // Normal damping is no longer a constant — it's DERIVED per-material from restitution (docs/24
                                          // Stage 1), see `granular::damping_for_restitution` in `gpu_step_params`.
    const CONTACT_TANGENT_DAMP: f32 = 100.0; // friction ramp with slip speed
    /// Air temperature (K) for the surface band's density. ISA sea level; the isothermal assumption is
    /// the same one `scale_height` and the settling-column emergence test make (docs/26).
    const AIR_TEMP_K: f64 = 288.0;
    /// Drag coefficient for a voxel grain — a cube, tumbling. DECLARED shape factor (docs/46 §1); the
    /// resolved computation it stands in for is the pressure field of `AirField` parcels flowing around
    /// the grain, so it is deletable when that flow is resolved. ~1.05 is the standard cube value.
    const DRAG_CD_CUBE: f32 = 1.05;

    /// Per-substep position-projection cap for a BODY resolving against the terrain constraint. Mirrors
    /// `particle_step.wgsl::MAX_SURFACE_CORRECTION` (0.01 m) — the bound that makes the projection
    /// stack-safe and stops it doing work, which is what fixed the grains' settling storm
    /// (JOURNAL 2026-07-19). A body's bonds are stiffer than a grain's contacts, so this bound matters
    /// more here, not less: an unbounded snap is exactly what used to pump the probe apart.
    const PROBE_MAX_SURFACE_CORRECTION: f64 = 0.01;
    /// μ used when the column under a contact has no material (empty column / off the voxel footprint).
    /// Basalt's coefficient — this world's actual crust (docs/28), the same representative choice
    /// `gpu_step_params` makes for debris, so a body off the patch grips like the ground it is drawn on.
    const PROBE_GROUND_MU_FALLBACK: f64 = 0.7;
    // Specific heat (J/(kg·K)) for the grain's temp↔u conversion (u = c·T). Generic rock default, matching
    // aggregate/hydrostatic; per-material c is a flagged refinement (like the global contact params). docs/38.
    const GRAIN_SPECIFIC_HEAT: f32 = 1000.0;

    // How often the GPU debris is de-resolved back into voxels (docs/22): a grain that has come to REST
    // on the terrain returns to the voxel grid, matter-conserving, so the debris count falls to ~0 once
    // the excitement passes (no more "rubble hovering forever"). The readback STALLS the pipeline, so we
    // amortise it — every N frames, not per frame. ~4×/s at 60 fps is imperceptible next to the ~30 s
    // settle window and keeps the sky clearing smoothly.
    const SETTLE_READBACK_INTERVAL: u64 = 15;
    // A grounded grain whose vertical velocity is only snap-contact jitter still sits a hair BELOW the
    // heightfield surface under the penalty spring; count it grounded if its base is within this margin
    // of the terrain top (the shader's own bilinear surface uses the same −0.5 mesh offset).
    const SETTLE_GROUND_MARGIN: f32 = 0.1;
    // Consecutive GROUNDED substeps (the shader's `resting` counter) after which a grain deposits even if
    // it is still creeping above SETTLE_SPEED — the GPU port of the CPU `matter::step` SETTLE_FRAMES=10
    // fallback. cs_integrate runs once per substep (~960/s at ×1), so ~150 substeps ≈ 0.16 s of grounded
    // contact, matching the CPU's 10 frames at 60 fps. Without this, soft-contact grains creep forever.
    const SETTLE_REST_SUBSTEPS: f32 = 150.0;

    // Phase 3 dig/fracture. (MAX_PARTICLES moved to `crate::gpu_particles` — it is the container's
    // capacity, and the grid-table sizing invariant is tested against it there.)
    const PARTICLE_CUBE_HALF: f32 = 0.21; // half of the old 0.42 — finer debris, now GPU can afford it

    /// Each physics particle (one per 1 m³ voxel) is DRAWN as 8 half-size sub-cubes at the octant
    /// centres of its cell — 8× the cubes at ½ the size (2³, cubed in volume). Purely a rendering
    /// subdivision: the physics model stays one particle per voxel (mass/conservation unchanged); this
    /// just resolves the debris more finely now that the sim runs on the GPU.
    const SUB_Q: f32 = 0.25;
    const SUB8: [[f32; 3]; 8] = [
        [-SUB_Q, -SUB_Q, -SUB_Q],
        [SUB_Q, -SUB_Q, -SUB_Q],
        [-SUB_Q, SUB_Q, -SUB_Q],
        [SUB_Q, SUB_Q, -SUB_Q],
        [-SUB_Q, -SUB_Q, SUB_Q],
        [SUB_Q, -SUB_Q, SUB_Q],
        [-SUB_Q, SUB_Q, SUB_Q],
        [SUB_Q, SUB_Q, SUB_Q],
    ];
    const DIG_RADIUS: f32 = 3.0;
    const DIG_POWER: f32 = 1.5e6; // breaks soil/grass, not granite
    const BLAST_POWER: f32 = 3.0e7; // breaks granite too
                                    // A meteor is a real nickel-iron body, not an abstract energy: its impact energy is ½·m·v²
                                    // (docs/23). ~91% iron / ~8% nickel; it vaporizes on impact into its own matter.
    const METEOR_MASS: f32 = 1_000.0; // kg (~0.3 m Fe-Ni body)
    const METEOR_SPEED: f32 = 17_000.0; // m/s (typical hypervelocity impact speed)

    // Render scaffolding (Camera/GpuMesh/UniformSlot/Uniforms/... + the generic helpers) moved
    // to `crate::render` (docs/33).
    use crate::render::*;

    // GPU-compute debris particles moved to `crate::gpu_particles` (docs/33) — see that module.
    use crate::gpu_layout::{GpuParticle, GpuStepParams};
    use crate::gpu_particles::{GpuParticles, GRID_BUCKET_K, GRID_TABLE_SIZE, MAX_PARTICLES};

    /// A compute-only GPU probe for **cross-device** verification (JOURNAL 2026-07-19).
    ///
    /// WHY. Two blind spots meet here. (1) `Engine::create` acquires its adapter with
    /// `request_adapter(HighPerformance)` and never reports what it got, so a browser run is silent
    /// about which GPU produced it — the same ambiguity `pick_adapter` fixes natively in
    /// `tools/gpu-verify`. (2) `GpuParticles::dispatch` splits its four stages into four separate
    /// compute passes precisely because fusing them "happened to work on desktop Vulkan (the 2070) but
    /// can RACE on other backends (e.g. Metal / the M4)" — and that mitigation has never been exercised
    /// ON Metal. This probe answers both on any device with a browser: which adapter, how fast, and
    /// whether energy stays bounded (a race injects energy).
    ///
    /// It drives the REAL `GpuParticles`, hence the real `shaders/particle_step.wgsl` — not a
    /// reimplementation — so a result here is a statement about shipping code. Compute only: no canvas,
    /// no surface. Material properties are read from the material DB, not invented (see `probe_params`).
    ///
    /// ASYNC SHAPE. Browser buffer mapping cannot block (`Maintain::Wait` is a no-op there), so this
    /// uses the same two-phase pattern as `begin_readback`/`take_readback`: `start_run` records and
    /// submits, returning immediately; JS polls `poll()` until it flips true, then reads
    /// `result_json()`. JS brackets that with `performance.now()`. Run enough frames that the poll
    /// granularity is a small fraction of the total — a single frame is not measurable this way.
    #[wasm_bindgen]
    pub struct GpuProbe {
        device: wgpu::Device,
        queue: wgpu::Queue,
        info: wgpu::AdapterInfo,
        max_buffer_size: u64,
        max_wg_per_dim: u32,
        parts: Option<GpuParticles>,
        snapshot: Vec<GpuParticle>,
        n: u32,
        frames: u32,
        gravity: f32,
    }

    #[wasm_bindgen]
    impl GpuProbe {
        /// Acquire a compute-only device. `compatible_surface: None` — nothing is drawn.
        pub async fn create() -> Result<GpuProbe, JsValue> {
            console_error_panic_hook::set_once();
            let _ = console_log::init_with_level(log::Level::Info);
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..Default::default()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .ok_or_else(|| JsValue::from_str("no GPU adapter (is WebGPU enabled?)"))?;
            let info = adapter.get_info();
            let limits = adapter.limits();
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("gpu-probe"),
                        required_features: wgpu::Features::empty(),
                        required_limits: limits.clone(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;
            Ok(GpuProbe {
                device,
                queue,
                info,
                max_buffer_size: limits.max_buffer_size,
                max_wg_per_dim: limits.max_compute_workgroups_per_dimension,
                parts: None,
                snapshot: Vec::new(),
                n: 0,
                frames: 0,
                gravity: 9.81,
            })
        }

        /// Adapter provenance. On iPadOS this is what proves the backend is Metal; everywhere it stops a
        /// result from being ambiguous about the hardware that produced it.
        pub fn gpu_adapter_json(&self) -> String {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "{{\"name\":\"{}\",\"backend\":\"{:?}\",\"device_type\":\"{:?}\",\"driver\":\"{}\",\"driver_info\":\"{}\",\"vendor\":{},\"device\":{},\"max_buffer_size\":{},\"max_workgroups_per_dim\":{}}}",
                esc(&self.info.name),
                self.info.backend,
                self.info.device_type,
                esc(&self.info.driver),
                esc(&self.info.driver_info),
                self.info.vendor,
                self.info.device,
                self.max_buffer_size,
                self.max_wg_per_dim,
            )
        }

        /// Phase 1: seed `n` grains and submit `frames × DEBRIS_SUBSTEPS` substeps, then start a
        /// readback that fences the whole batch. Returns as soon as the work is queued.
        pub fn start_run(&mut self, n: u32, frames: u32) {
            let n = n.clamp(1, MAX_PARTICLES as u32);
            self.n = n;
            self.frames = frames.max(1);
            self.snapshot.clear();

            let mut parts = GpuParticles::new(&self.device, n, PROBE_W * PROBE_W);
            // Flat floor at voxel 0 — the probe measures the granular step, not terrain shape.
            parts.upload_heightfield(&self.queue, &vec![0i32; (PROBE_W * PROBE_W) as usize]);

            // ρ₀ from the REAL material (basalt), matching `probe_params` and the spawn path — the
            // grain carries density as Tillotson input (docs/38), so it must not be invented.
            let rho0 = {
                let mats = materials::load();
                mats[materials::index_of(&mats, "basalt")].density
            };

            // A cube of grains on the 1 m lattice, jittered for the same reason gpu-verify jitters: a
            // perfect lattice is metastable and will not flow, so an unjittered pile is not a
            // representative contact workload.
            let side = (n as f64).cbrt().ceil() as u32;
            let mut grains = Vec::with_capacity(n as usize);
            for i in 0..n {
                let (x, y, z) = (i % side, (i / side) % side, i / (side * side));
                let j = |salt: u32| {
                    let h = (i.wrapping_add(salt).wrapping_mul(2654435761)) ^ 0x9e37_79b9;
                    (((h >> 8) & 0xffff) as f32 / 32768.0 - 1.0) * 0.1
                };
                grains.push(GpuParticle {
                    offset: [x as f32 + j(1), 8.0 + y as f32 + j(2), z as f32 + j(3)],
                    // docs/38: the grain's thermodynamic state is specific internal energy, not
                    // temperature — temp = u/c is derived. 300 K ambient, same as the spawn path.
                    u: GRAIN_SPECIFIC_HEAT * 300.0,
                    vel: [0.0; 3],
                    resting: 0.0,
                    color: [0.5, 0.5, 0.5],
                    material: 0.0,
                    emission: [0.0; 3],
                    rho: rho0,
                    // docs/47 §1: size travels WITH the grain. Uniform today (every debris grain is
                    // the same 1 m ejecta scale); the hierarchical grid is what makes mixed sizes correct.
                    radius: CONTACT_RADIUS,
                    _p0: 0.0,
                    _p1: 0.0,
                    _p2: 0.0, // ρ₀ at spawn, from the real material (docs/38 4b.2 will compute it)
                });
            }
            parts.append(&self.queue, &grains);
            parts.set_params(&self.queue, &self.probe_params());

            // ONE encoder for every substep of every frame — mirrors `Engine::step_physics`, which
            // records all DEBRIS_SUBSTEPS into one encoder and submits once. Timing a
            // submit-per-substep shape would measure driver launch overhead instead of the shader
            // (JOURNAL 2026-07-19: that mistake made a 2.5× hardware gap look like 17%).
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu-probe"),
                });
            for _ in 0..self.frames {
                for _ in 0..DEBRIS_SUBSTEPS {
                    parts.dispatch(&mut enc);
                }
            }
            self.queue.submit(std::iter::once(enc.finish()));
            // Fences the batch: the map callback cannot fire until the GPU has drained the queue.
            parts.begin_readback(&self.device, &self.queue);
            self.parts = Some(parts);
        }

        /// Phase 2: true once the GPU has finished and the grains are read back. Poll from JS.
        pub fn poll(&mut self) -> bool {
            let Some(parts) = self.parts.as_mut() else {
                return false;
            };
            match parts.take_readback() {
                Some(snap) => {
                    self.snapshot = snap;
                    true
                }
                None => false,
            }
        }

        /// Energy + motion summary of the settled grains. `"null"` before the first completed run.
        ///
        /// UNIT GRAIN MASS: the shader carries no per-grain mass, so these are per-unit-mass figures.
        /// That is deliberate — the check is the INVARIANT (`tot` must never rise between runs of
        /// increasing `frames`), not an absolute energy claim. A backend race shows up here as rising
        /// total energy, which is exactly how gpu-verify's scene I detects fudges natively.
        pub fn result_json(&self) -> String {
            if self.snapshot.is_empty() {
                return String::from("null");
            }
            let (mut ke, mut pe, mut vmax) = (0.0f64, 0.0f64, 0.0f64);
            for p in &self.snapshot {
                let v2 = (p.vel[0] * p.vel[0] + p.vel[1] * p.vel[1] + p.vel[2] * p.vel[2]) as f64;
                ke += 0.5 * v2;
                pe += (self.gravity * p.offset[1]) as f64;
                vmax = vmax.max(v2.sqrt());
            }
            format!(
                "{{\"n\":{},\"frames\":{},\"substeps\":{},\"grains\":{},\"ke\":{:.6e},\"pe\":{:.6e},\"tot\":{:.6e},\"vmax\":{:.4}}}",
                self.n,
                self.frames,
                DEBRIS_SUBSTEPS,
                self.snapshot.len(),
                ke,
                pe,
                ke + pe,
                vmax,
            )
        }
    }

    /// Probe world footprint in cells. Only needs to comfortably contain the seeded cube (the largest,
    /// at MAX_PARTICLES, is ~40 cells on a side).
    const PROBE_W: u32 = 256;

    impl GpuProbe {
        /// Step params for the probe. Friction, restitution-derived normal damping and cohesion are read
        /// from REAL basalt in the material DB, mirroring `Engine::gpu_step_params` — a probe that
        /// invented these would be exercising a shader configuration the engine never actually runs, and
        /// its timings would not transfer. (docs/24; same representative-material approximation, flagged
        /// there.)
        fn probe_params(&self) -> GpuStepParams {
            let mats = materials::load();
            let bulk = &mats[materials::index_of(&mats, "basalt")];
            let normal_damp = crate::granular::damping_for_restitution(
                bulk.restitution as f64,
                CONTACT_STIFFNESS as f64,
            ) as f32;
            let grain_area = std::f32::consts::PI * CONTACT_RADIUS * CONTACT_RADIUS;
            const GRANULAR_COHESION_CEIL: f32 = 5.0e4; // Pa — loose-debris adhesion ceiling (docs/24)
            let c_cohesion =
                bulk.cohesion.min(GRANULAR_COHESION_CEIL) * grain_area / bulk.density.max(1.0);
            GpuStepParams {
                gravity: [0.0, -self.gravity, 0.0],
                dt: (1.0 / 60.0) / DEBRIS_SUBSTEPS as f32,
                center: [0.0, 0.0, 0.0], // grains are already in voxel coords ⇒ ground sits at y = 0
                c_cohesion,
                // AIR: density derived from the planet's own declared atmosphere mass (docs/48). One
                // value for the patch — the barometric profile varies 1.1% over 96 m, so resolving it
                // here buys nothing (docs/44). `matter::DRAG` is gone: it was a velocity multiply.
                // Same air the engine runs in — the probe exercises SHIPPING code, so it must not
                // measure a shader configuration the engine never uses. `mats` and `self.gravity` are
                // this fn's own; the Engine's `self.mats`/`self.surface_g` do not exist on GpuProbe.
                air_rho: crate::atmosphere::air_density_at(
                    crate::planet::earth().surface_pressure(),
                    &mats[materials::index_of(&mats, "air")],
                    AIR_TEMP_K,
                    self.gravity as f64,
                    0.0,
                ) as f32,
                contact_damp: matter::CONTACT_DAMP,
                settle_speed: 0.0,
                part_half: DEBRIS_PART_HALF,
                cool_rate: 0.4,
                count: self.n,
                world_w: PROBE_W,
                world_d: PROBE_W,
                cell_size: 2.0 * CONTACT_RADIUS,
                table_mask: GRID_TABLE_SIZE - 1,
                bucket_k: GRID_BUCKET_K,
                c_radius: CONTACT_RADIUS,
                c_stiffness: CONTACT_STIFFNESS,
                c_normal_damp: normal_damp,
                c_friction: bulk.friction_coefficient,
                c_tangent_damp: CONTACT_TANGENT_DAMP,
                // docs/38: the grain carries u = c·T, so the shader needs c to derive temperature.
                // Same constant the production path passes (`gpu_step_params`) — the probe must not
                // run a different thermodynamic conversion than the engine it is measuring.
                specific_heat: GRAIN_SPECIFIC_HEAT,
                drag_cd: DRAG_CD_CUBE,
                // docs/47 §1: level-0 cell = today's single cell size, and max_level 0 because every
                // grain is currently the same size — so the hierarchical walk collapses to the old
                // ±1 scan bit-identically. Mixed sizes raise max_level.
                base_cell: 2.0 * CONTACT_RADIUS,
                max_level: 0,
            }
        }
    }

    // ============================================================================================
    // Space band (scale-relative "orbit-to-ground", Step A): render the Earth + Moon as two lit
    // spheres whose positions come from the *validated* N-body physics (orbit.rs), so what you watch
    // is the same law the `moon_orbits_earth` test proves. Physics runs in real SI units (f64); we
    // map metres to display units (Earth radius -> 1) only for drawing. This is the coarse "celestial
    // band" of docs/13 — the first rung of the scale ladder.
    // ============================================================================================

    // The default bodies this band places (its `create()` path predates worlds-as-data). Their
    // parameters are READS of the shared definitions in `assets/bodies/*.json` - the private
    // `earth_mass()`/`earth_radius_m()`/`MOON_*` constants that used to sit here are retired (docs/58,
    // docs/59 one Earth): mass EMERGES from the declared layers, radius is the outer layer, and a
    // `laws` scan keeps the literals from creeping back into a scene. Cached once; a definition is a
    // file, not a per-frame parse.
    static EARTH_PARAMS: std::sync::LazyLock<(f64, f64)> = std::sync::LazyLock::new(|| {
        let e = crate::planet::earth();
        (e.total_mass(), e.radius())
    });
    static MOON_PARAMS: std::sync::LazyLock<(f64, f64)> = std::sync::LazyLock::new(|| {
        let m = crate::planet::moon();
        (m.total_mass(), m.radius())
    });
    fn earth_mass() -> f64 {
        EARTH_PARAMS.0
    }
    fn earth_radius_m() -> f64 {
        EARTH_PARAMS.1
    }
    fn moon_mass() -> f64 {
        MOON_PARAMS.0
    }
    fn moon_radius_m() -> f64 {
        MOON_PARAMS.1
    }

    /// What kind of body this is, for the generic render (a star lights, a planet is the globe, a moon is
    /// a sphere). Declared by the scene's `role`; the engine decides everything downstream.
    #[derive(Clone, Copy, PartialEq)]
    enum BodyRole {
        Star,
        Planet,
        Moon,
    }

    /// Per-body metadata the engine renders and collides from. Indexed identically to `OrbitDemo::bodies`.
    #[derive(Clone)]
    struct BodyMeta {
        radius_m: f64,
        tint: [f32; 4],
        role: BodyRole,
        /// **This body's matter** — its layered material composition (`docs/58`). The engine keeps each
        /// body's own matter so mass, radius, moment of inertia and particalization all DERIVE from it,
        /// with no named `planet::earth()` lookup and no `earth_radius_m()`: a body is a body. `None` for a
        /// bare point mass with no defined composition.
        matter: Option<crate::planet::LayeredBody>,
        /// The engine has resolved this body into the debris field — its matter is particles now, so it
        /// is no longer drawn as an intact body. Set by collision detection, never by the scene.
        materialized: bool,
    }
    const MOON_DIST_M: f64 = 3.844e8; // m (semi-major axis)
    const MOON_SPEED: f64 = 1022.0; // m/s (mean orbital speed)
    const SUN_MASS: f64 = 1.989e30; // kg — holds and lights the system
    const AU_M: f64 = 1.496e11; // m (Earth–Sun distance)
    const EARTH_HELIO_SPEED: f64 = 29_780.0; // m/s (Earth's mean heliocentric speed = sqrt(G·M_sun/AU))
    /// Metres -> display units: Earth's radius becomes 1.0, so the Moon sits ~60 units out. Derived
    /// from the shared definition like everything else about the body.
    ///
    /// (`display_scale()`, a second scale that magnified a 5,000 km stand-in Earth 4.5x, was retired when
    /// the impact started running the REAL Earth and Theia from their definitions - one scale, one
    /// place to disagree fewer.)
    fn display_scale() -> f64 {
        1.0 / EARTH_PARAMS.1
    }
    // Fast-forward so a full ~27.3-day orbit plays in ~20 s. Symplectic Verlet stays stable with many
    // substeps per frame (dt ~= 125 s at this scale => thousands of steps per orbit).
    const ORBIT_TIME_SCALE: f64 = 118_000.0; // sim-seconds per real-second
    const ORBIT_SUBSTEPS: u32 = 16;

    /// A clump promoted out of the particle set (docs/58's generic body: matter + pos + vel + ang_mom).
    /// The matter is SAMPLED from the particles that formed it, so the layering it carries is the
    /// differentiation the simulation actually produced (`accretion::sample_layers`).
    #[derive(Clone, Debug)]
    struct PromotedBody {
        body: crate::accretion::Body,
        matter: crate::planet::LayeredBody,
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct SpaceUniforms {
        view_proj: [[f32; 4]; 4],
        model: [[f32; 4]; 4],
        light_dir: [f32; 4], // xyz = direction to the "sun"
        tint: [f32; 4],      // body color
        emissive: [f32; 4],  // rgb = incandescent glow, w = intensity (self-lit hot ejecta)
        atm: [f32; 4], // xyz = Rayleigh optical depth per band, w = sun gain (the SAME air the ground sky uses)
        /// rgb = the Planck colour of this body's own surface temperature, w = how brightly it glows as a
        /// multiple of a sunlit white surface (`blackbody::thermal_glow_gain`). A cold planet sends zeros
        /// and pays nothing; a magma ocean sends ~547 and lights itself.
        glow: [f32; 4],
        /// THE CRATER, measured (docs/39 surface hook, docs/46 row 18). xyz = the bowl axis as a unit
        /// vector in the globe's MODEL space, w = its angular radius (rad). Zero ⇒ no crater, so every
        /// other consumer of this uniform pays nothing.
        crater: [f32; 4],
        /// x = bowl depth as a fraction of the surface radius. Derived from the EXCAVATED MASS actually
        /// lifted above the pristine surface, never authored — see `gpu_crater_depth_frac`.
        crater2: [f32; 4],
    }

    /// Byte offset of `SpaceUniforms::crater` — 2 mat4 (128) + 5 vec4 (80). The globe patches just these
    /// 32 bytes after `write_space_uniform`, so the crater does not have to be threaded through all 14
    /// call sites of a uniform that only one draw uses.
    const CRATER_UNIFORM_OFFSET: u64 = 208;

    /// How far (wall-clock seconds) the RENDER runs behind the PHYSICS (docs/13). Humans don't
    /// resolve detail below ~1/10 s, so the physics keeps a 100 ms head start: every collision in the
    /// next 100 ms is already fully resolved before the frame that shows it is drawn — the simulation
    /// drives the visualization and can never be caught mid-lie by a frame boundary.
    const RENDER_LAG_S: f64 = 0.10;

    /// A snapshot of the observable physics state at one physics-clock instant. The renderer
    /// interpolates between snapshots at (now − RENDER_LAG_S); it never reads live physics state.
    /// (The CPU debris cloud's per-fragment fields are gone with the Aggregate path, docs/58 #7:
    /// resolved impact matter lives in the GPU SPH buffer and is drawn straight from it.)
    struct FrameSnap {
        t: f64, // physics wall-clock (s) when taken
        bodies: Vec<glam::DVec3>, // positions of [Sun, Earth, Moon(s)] — the only thing the render lags.
                                  // (The live GPU SPH particle field is drawn straight from `sph_snapshot`, not lagged through here;
                                  // a struck moon hides on its own `materialized` flag, so no per-snapshot debris state is needed.)
    }

    /// Resolution-on-demand plan for a small-impactor collision (docs/39): the target stays an abstract
    /// BULK and only a CAP of it is resolved, alongside the whole impactor(s). Held while the impactor(s)
    /// approach as solid bodies; the `Assembling` phase reads it to particalize the cap at the LIVE impact
    /// site(s) and configure the GPU bulk. `None` ⇒ a whole-body collision (birth), which resolves everything.
    struct CapPlan {
        planet: usize,         // self.bodies index of the target (the bulk)
        impactors: Vec<usize>, // self.bodies indices of the impactor(s), whole-resolved (prov 1.. in the relax)
        r_core: f64, // the bulk floor radius (the cap's inner edge) — the cap relaxes seated here
        bulk_mass: f64, // the un-resolved remainder (planet − cap): the Gauss gravity source
    }

    /// Setup phase of the GPU SPH impact (docs/35): relax the two bodies on the GPU (placed far apart so each
    /// settles under its own gravity), read them back, assemble the collision, then step the dynamics.
    #[derive(Clone, Copy)]
    enum SphPhase {
        Relaxing(u32), // GPU `cs_relax` steps completed so far
        /// Both bodies are SOLID and approaching. They integrate as bodies, they are drawn as bodies, and
        /// no particle exists yet — because nothing has happened that requires one.
        ///
        /// The scene used to open by building a particle field for both bodies and relaxing it in view,
        /// so Theia arrived as a visible bundle of specks that nothing had disrupted. A body is a body
        /// until an interaction cannot be represented any other way; `accretion::resolution_distance`
        /// says when that is, from the tides, and the relax happens here where nobody is looking.
        Approaching,
        Assembling, // relax done; awaiting the async read-back to compute the collision geometry
        Dynamics,   // colliding — KDK substeps + read-back
    }

    /// The orbital ("space band") demo handle exposed to JavaScript.
    #[wasm_bindgen]
    pub struct OrbitDemo {
        /// The giant impact's DECLARED initial conditions (docs/51). Defaults to the values that were
        /// Rust constants, so the scene is unchanged until a world file says otherwise.
        impact_def: crate::terra::world_def::ImpactDef,
        /// **When this scene is set**, Unix seconds — `None` is now. A world may declare it; the
        /// birth-of-the-Moon scene is proto-Earth while Terra is this afternoon, and both draw the
        /// same Earth (docs/65: time is part of the setting, not part of the body).
        scene_epoch: Option<f64>,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        depth_view: wgpu::TextureView,
        pipeline: wgpu::RenderPipeline,
        sphere_gpu: GpuMesh,
        moon_unis: Vec<UniformSlot>, // one per moon (the two-moon scene has two)
        bodies: Vec<crate::orbit::Body>, // [Sun, Earth, Moon, (Moon2)…]
        acc: Vec<glam::DVec3>,
        time_scale: f64,
        camera: Camera,
        /// The body the view is centred on — the viewport's physical frame of reference (docs/17).
        /// 1 = Earth (default), 2.. = moons.
        focus: usize,
        // Body colours are the *aggregate albedo of a real composition* (materials.json), not painted
        // tints — see `materials::aggregate_albedo` / docs/17. Reflectance only; the shader supplies
        // brightness (illumination × reflectance), so a dark-but-lit body still reads bright.
        earth_tint: [f32; 4],
        moon_tint: [f32; 4],
        /// **Per-body render/collision metadata, one entry per `self.bodies`.** A scene declares bodies
        /// (position, velocity, mass, radius); this holds the rest the ENGINE needs to render and collide
        /// each one — its radius, its tint, and whether it has been resolved into the debris field.
        ///
        /// This replaces the single-value `impactor_radius` / `moon_tint` / `impactor_mass`, which were
        /// OVERWRITTEN on each moon in the load loop — so with two moons they held only the LAST moon's
        /// values, and both moons shared one radius, one tint, one mass. That is why one moon fragmented
        /// and the other did not, and why the tints were wrong.
        body_meta: Vec<BodyMeta>,
        /// Snapshot of the initial [Sun, Earth, Moon] state, for the "reset" control.
        initial_bodies: Vec<crate::orbit::Body>,
        /// Snapshot of the initial spin angular momentum, restored on Reset alongside `initial_bodies`.
        /// Without this a Reset kept the impact-induced spin — a world reset that conjured angular
        /// momentum out of the previous run (a render-truth bug, docs/28).
        initial_spin_l: glam::DVec3,
        /// True once any moon has struck the Earth (contact resolution fired) — for the HUD.
        impacted: bool,
        /// Per-moon "has already hit" flags, so each moon's impact energy is counted exactly once
        /// (the two-moon scene sums both).
        moon_hit: Vec<bool>,
        /// Kinetic energy (J) the impact(s) dissipated — the energy that would become damage. Reported,
        /// not yet turned into actual fragmentation (docs/17 honesty: measure it, don't hide it).
        impact_energy_j: f64,
        mats: Vec<materials::Material>,
        /// The inbound impactor's physical radius/mass — the Moon by default; Theia in the
        /// birth-of-the-Moon scenario (docs/27). Drives CCD contact distance and shell rendering.
        impactor_radius: f64,
        impactor_mass: f64,
        /// SIM seconds elapsed since the impact — the honest answer to "what timeframe are we watching
        /// this over?" (geologic time runs under time-LOD, so real seconds ≠ sim seconds).
        sim_since_impact: f64,
        /// Earth's SPIN angular momentum (docs/27): set by the modern day length in the orbital scenes;
        /// ZERO for proto-Earth in the birth scene (its primordial spin is unknown — flagged) so the
        /// post-impact day length EMERGES from the collision geometry. Fed by the boundary-shear mirror,
        /// demoted matter's orbital L, and drained by tidal torque on the moonlets.
        spin_l: glam::DVec3,
        /// **The definitive Earth.** The same globe mesh Terra renders, built by the same shared builder
        /// from the same body definition — because Earth is one object, not one per scene. `None` until
        /// the host hands over the body's surface rasters; the scene then shows the real planet instead of
        /// the 512-grain shell that used to stand in for it.
        /// The real sky (HYG). `None` until the host hands over the catalogue — a scene without it simply
        /// has no stars, rather than inventing any.
        stars: Option<StarField>,
        /// The IMPACTOR drawn as the body it is. Theia arrived as a visible bundle of specks that nothing
        /// had disrupted yet — particles by default, with a surface bolted onto the target only. A body
        /// is a body until an event takes it apart, and that rule is not about which body the scene
        /// happens to care about.
        impactor_uni: UniformSlot,
        globe_pipeline: wgpu::RenderPipeline,
        globe_mesh: Option<GpuMesh>,
        globe_uni: UniformSlot,
        /// **The descent corridor's fine ground cap** — the SAME close-range treatment Terra
        /// renders (terra::ground_cap + the shared SurfaceSampler + the same alpha-blend globe
        /// shader), picked up by this scene once the arc descends below the derived hand-off
        /// altitude, where one texel of the planetary rasters exceeds the angular budget and the
        /// coarse globe would otherwise stretch them across the view.
        cap_pipeline: wgpu::RenderPipeline,
        cap_gpu: GpuMesh,
        cap_uni: UniformSlot,
        cap_verts: Vec<Vertex>,
        earth_surface: Option<EarthSurface>,
        /// GEOLOGIC time-LOD (docs/27): once the aftermath is quiet, each settled clump IS one body
        /// (orbital elements), evolved by the validated secular tidal law — millennia per real second.
        geologic: bool,
        geo_moonlets: Vec<crate::tides::Moonlet>,
        geo_rate_yr_s: f64,
        /// Accumulated rotation angle (rad) about the spin axis — the VISIBLE rotation of the shell
        /// (and its landmask) at the real rate implied by spin_l.
        spin_angle: f64,
        shell_unis: Vec<UniformSlot>,
        /// The bulk interior sphere (the un-materialized deep Earth): visible only through the crater —
        /// the top of the outer core at cap depth, glowing at its REAL temperature ("hollow earth" fix).
        interior_uni: UniformSlot,
        sun_uni: UniformSlot,
        atm_tau: [f64; 3],
        atm_twilight: f64,
        interior_tint: [f32; 4],
        interior_glow: [f32; 4],
        // Physics/render decoupling (docs/13): physics advances on its own fixed timestep driven by
        // wall-clock time; the renderer samples snapshots RENDER_LAG_S behind. See `advance`.
        snaps: std::collections::VecDeque<FrameSnap>,
        phys_clock: f64,
        real_accum: f64,
        /// A pool of sphere-render slots for moonlet spheres (one draw each, like `moon_unis`):
        /// the SPH disk's accreting clumps and the geologic-time bodies.
        debris_unis: Vec<UniformSlot>,
        // --- GPU SPH deformable-Earth impact in the browser (docs/33 stage 4c.4) ---
        /// The GPU SPH particle system (built + relaxed on the CPU at `start_gpu_impact`, then stepped on the
        /// GPU each frame via the verified `sph_step.wgsl` kernels). `None` until triggered.
        gpu_sph: Option<crate::gpu_sph::GpuSph>,
        sph_pipeline: wgpu::RenderPipeline, // instanced billboard particles (sph_render.wgsl)
        sph_cam: UniformSlot, // view-proj + Earth display origin + scale for the particle shader
        sph_active: bool,
        sph_dt: f32, // fixed integration timestep (chosen at build; WebGPU forbids the adaptive read-back)
        sph_soft: f64, // gravitational softening (for the energy diagnostic's PE term)
        /// The material EOS TABLE the current SPH collision indexes (`SphParticle.mat` → `sph_eos[mat]`),
        /// built once from the bodies' matter (docs/58 #7). Was a fixed `[basalt, iron]` pair; now N materials.
        sph_eos: Vec<crate::gpu_sph::SphEos>,
        /// SPH source index (`prov`) → `self.bodies` index. The `Assembling` phase reads each source body's
        /// LIVE pos/vel/spin from the body it maps to — the ONE geometry source for EVERY collision (docs/58,
        /// Robin): `prov 0` is the planet. Birth places fresh bodies on a designed approach and the moon-drop
        /// keeps its orbiting ones, but both resolve identically from these live states — no flag, no branch.
        sph_prov_to_body: Vec<usize>,
        /// Live de-resolution budget (docs/08/44) — while the particle count exceeds it, redundant pairs
        /// merge in the shader. Driven by MEASURED frame time, not a chosen constant; 0 = no coarsening.
        sph_merge_budget: u32,
        /// PROMOTED bodies (docs/58): a settled, self-bound clump that has left the particle set and become
        /// a body carrying its own `LayeredBody` matter, spin and heat. They keep acting on the remaining
        /// particles through the shader's `ext_mass` channel — which is what makes that channel
        /// load-bearing — and are integrated here, since the shader no longer moves them.
        sph_promoted: Vec<PromotedBody>,
        /// Frames since the last promotion attempt; it is a change of representation, not a force.
        sph_promote_tick: u32,
        /// A collision the ballistic `step_substep` DETECTED but must not resolve itself (docs/58 — the ONE
        /// collision engine). Holds the colliding set (`[planet, impactors…]`, planet first = prov 0);
        /// `advance` executes it via `route_bodies_to_sph` after the substep loop unwinds, because
        /// `begin_sph_relax` rebuilds the whole scene. `None` when no contact is pending.
        pending_sph_route: Option<Vec<usize>>,
        /// A live resolution-on-demand CAP collision (docs/39): the target is a bulk, only a cap is resolved.
        /// `None` ⇒ whole-body (birth). Set by `route_bodies_to_sph`, consumed by the `Assembling` phase.
        sph_cap: Option<CapPlan>,
        /// docs/42 browser-parity — SCHEDULED shock-dt: WebGPU forbids the per-step adaptive read-back, so the
        /// dt is stepped by SIM TIME instead — the small shock dt (`sph_dt`) resolves the collision, then after
        /// `SPH_SHOCK_WINDOW_S` we switch to the larger `sph_dt_aftermath` for the slow disk evolution (restores
        /// playback). `sph_sim_t` is the physical time integrated since the collision started.
        sph_sim_t: f64,
        sph_dt_aftermath: f32,
        /// docs/42 — ADAPTIVE GPU load: substeps (relax steps) encoded per frame, scaled to a wall-clock frame
        /// budget so the sim never monopolizes the GPU / freezes the tab or OS. Grows when there's headroom,
        /// shrinks (down to 1) when frames run long. The direct-sum O(N²) step is heavy, so this self-limits.
        sph_substeps: u32,
        /// Latest async read-back of the GPU SPH particles (one frame behind) — for the HUD/disk-stats and
        /// (later) the momentum mirror. Empty until the first read-back completes.
        sph_snapshot: Vec<crate::gpu_sph::SphParticle>,
        /// The GPU impact's setup/step phase (relax on GPU → assemble collision → dynamics). See `advance`.
        sph_phase: SphPhase,
        /// A drop ARMED for the launch window (`crate::intercept`): on a world that declares a
        /// site, the Drop control solves for the release time at which the site rotates under
        /// the fall's impact point and arms this instead of releasing. The times count down in
        /// SIM seconds inside `step_substep`; the release fires itself at the window. `None` =
        /// nothing armed (and a world without a site never arms - the drop stays instant).
        armed_drop: Option<crate::intercept::DropWindow>,
        /// docs/42 render-layer blend: 0 = the PRETTY render (sphere/atmosphere), 1 = the raw PHYSICS particles.
        /// Cross-fades by size (grains × (1−blend), billboards × blend), so no alpha-sort. Only meaningful while
        /// `sph_active`. Default 0 (pretty first — the slider reveals the physics).
        render_blend: f64,
        /// docs/42 Phase 2: the giant-impact crater on the pretty sphere. The impact site (an EARTH-RELATIVE
        /// unit direction, captured from the GPU field at first Theia contact) and how open the bowl is (0→1,
        /// grows as the shock excavates). `None` until contact. Persists after (bake-back — Robin's call).
        gpu_impact_site: Option<glam::DVec3>,
        gpu_crater_frac: f64,
        /// Bowl depth as a fraction of the surface radius, MEASURED from the target material actually
        /// lifted above its pristine surface (docs/46 row 18). 0 until something is excavated.
        gpu_crater_depth_frac: f64,
        /// Bowl RADIUS as a fraction of the surface radius, from the same measured excavated volume as the
        /// depth (docs/46 row 18) — not the inherited 0.72 dial that made the crater a flat saucer.
        gpu_crater_r_frac: f64,
        /// Last reported bowl depth (quantised) so the diagnostic logs on CHANGE, not every frame.
        gpu_crater_logged: i32,
        // --- docs/59: the declared site and its camera-driven materialization trigger. ---
        /// The declared ground-zero site (from the ground world definition), armed once the host
        /// loads it. `None` = no site declared; the trigger never fires.
        site_spec: Option<crate::site::SiteSpec>,
        /// The bidirectional trigger - the camera's mirror of the moon-drop's
        /// resolution-distance crossing (one materialization pattern, docs/59).
        site_trigger: crate::site::SiteTrigger,
        site: Option<crate::site::MaterializedSite>,
        /// The docs/61 settle gauge gating the downward (fold) crossing.
        site_gauge: crate::recohere::SettleGauge,
        /// The honest one-line site state for the HUD: the audit when materialized, the refusal
        /// and its reason when refused, the fold report when folded.
        site_status: String,
        /// A refine-level refusal at the current descent is remembered so the standing demand
        /// does not rebuild (and re-refuse) every frame; cleared when the camera re-arms by
        /// ascending back past the threshold.
        site_refused: bool,
        /// Instance buffer for the site's particles (billboards through `sph_render.wgsl`),
        /// rewritten per frame with Earth-relative positions (the site co-rotates with the
        /// crust, exactly like the shell grains and the crater mask).
        site_buf: Option<wgpu::Buffer>,
        site_cam: UniformSlot,
        /// Live trigger inputs, kept for the HUD: the camera's distance to the site and the
        /// derived view-necessity threshold (both metres, updated every `advance`).
        site_dist_m: f64,
        site_resolve_at_m: f64,
        // --- docs/59 "The hand-down, made concrete": the mid-event boundary hand-down. ---
        /// Counts coarse-field readbacks, so the guard band re-samples once per COARSE step
        /// (each new snapshot), never redundantly per render frame.
        sph_snapshot_gen: u64,
        /// The snapshot generation the guard band last re-sampled.
        site_sampled_gen: u64,
        /// The event window the audit ledger books: boundary state at open, latest, and peak,
        /// so what arrived at the site is a measured delta on the HUD.
        site_window: crate::site::EventWindow,
        /// The RELEASED site's dynamics (docs/59): the fine parcels as cohesive matter, stepped
        /// each frame, driven by the guard band's boundary deltas during an event. `None` while
        /// no site is materialized or the release gate refused dynamics.
        site_dyn: Option<crate::site::SiteDynamics>,
        /// The pi-scaling prediction, frozen from the MEASURED contact state (rim metres,
        /// contact speed m/s) the moment the impact direction freezes. `None` until contact.
        pi_prediction: Option<(f64, f64)>,
        /// The latest pi-gate readout (measured rim vs prediction, or the stated refusal).
        pi_line: String,
        // --- The out-and-back demo arc (crate::arc): a CAMERA/TIME driver, never physics. ---
        /// The world's declared arc pacing (s per octave of camera distance). `None` = this world
        /// declares no arc; the control never appears.
        arc_octave_s: Option<f64>,
        /// The world's DECLARED celestial time scale, the pacing law's top anchor. `time_scale`
        /// itself mutates (⏪/⏩, the arc), so the declaration is kept separately.
        arc_declared_scale: f64,
        /// The running arc, when one is active. While `Some`, the arc owns the camera pose and the
        /// observable time rate; manual camera/time input is ignored until `arc_stop`.
        arc: Option<ArcDrive>,
        /// The time scale to hand back on `arc_stop` (captured at start).
        arc_saved_scale: f64,
    }

    /// Where the running demo arc is in its choreography. Glides advance on real time; holds wait
    /// for the operator's next press (the arc is a camera/time driver, dropping Luna, watching the
    /// impact, and when to come home stay the operator's calls).
    #[derive(Clone, Copy, PartialEq)]
    enum ArcPhase {
        GlideDown,
        HoldLow,
        GlideUp,
        HoldHigh,
    }

    /// The running arc: the derived span plus the current scalar state. The POSE is stateless,
    /// a pure function of (`d_m`, the leg, the live crust orientation) via `crate::arc`, so the
    /// path is continuous by construction, in both directions.
    struct ArcDrive {
        span: crate::arc::ArcSpan,
        phase: ArcPhase,
        d_m: f64,
        leg: crate::arc::Leg,
        /// Where the manual camera was aiming when the arc took over (Earth-relative, m), faded
        /// into the arc's own look target over the first octave of travel so taking over is a
        /// glide, not a cut.
        aim_from_rel: glam::DVec3,
        octaves: f64,
    }

    /// Earth rendered as a shell of particles (the honest low-res look, docs/15): a smooth sphere is a
    /// representation LIE once matter can be excavated — it hides the damage. The shell is the
    /// VISUALIZATION of the un-materialized bulk summary (whose physics is the boundary + gravity
    /// source); shell points inside the materialized impact region are hidden so the real crater shows.
    const SHELL_N: usize = 512;
    /// The intact Moon renders as a grain shell too — every solid object in the universe is composed of
    /// matter (Robin); a smooth sphere is the same representation lie we removed from Earth.
    const MOON_SHELL_N: usize = 128;
    /// Impactor/target mass ratio below which a collision resolves ON-DEMAND — the impactor(s) whole + a
    /// CAP of the target, the rest of the target an abstract bulk (docs/39). Above it, the impact is a giant
    /// impact and resolves every body whole (birth's regime). The Moon is 1.2% of Earth ⇒ a cap; a Theia-
    /// class body (~12%) ⇒ whole-body. A flagged first-cut threshold (energy-sizing, docs/44, is the refinement).
    const CAP_MASS_RATIO: f64 = 0.1;

    #[wasm_bindgen]
    impl OrbitDemo {
        /// Initialize the space band: acquire the GPU, build a unit sphere, seed the Earth + `num_moons`
        /// moons. `num_moons == 1` is the standard scene; `2` places moons on opposite sides of the same
        /// orbit (the de-orbit-both stress test).
        pub async fn create(
            canvas: HtmlCanvasElement,
            num_moons: u32,
        ) -> Result<OrbitDemo, JsValue> {
            console_error_panic_hook::set_once();
            let _ = console_log::init_with_level(log::Level::Info);

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);

            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..Default::default()
            });
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
                .map_err(|e| JsValue::from_str(&format!("create_surface failed: {e}")))?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .ok_or_else(|| JsValue::from_str("no suitable GPU adapter found"))?;
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("greenfield-orbit"),
                        required_features: wgpu::Features::empty(),
                        required_limits: adapter.limits(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;

            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            let depth_view = create_depth_view(&device, width, height);

            // One white unit sphere, tinted per-body in the shader.
            let sphere_gpu = upload_mesh(
                &device,
                "orbit-sphere",
                &mesher::build_uv_sphere(1.0, 0, [1.0, 1.0, 1.0], 48, 64),
            );

            // Materials first: the surface layout and its textures are built FROM them.
            let mats = materials::load();
            // **One surface bind layout for every scene.** There is nothing special about the orbit
            // view: it is a camera position looking at the same rendered world, so it carries the same
            // material albedo + NORMAL arrays. Giving the space band a uniform-only layout was what made
            // "Earth in orbit" a differently-rendered object from "Earth underfoot".
            let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            };
            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("surface-bind-layout"),
                entries: &[
                    uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT),
                    tex_entry(1),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    tex_entry(4),
                ],
            });
            let (tex_view, normal_view, sampler) = upload_material_textures(&device, &queue, &mats);
            let num_moons = num_moons.clamp(1, 2) as usize;
            let debris_unis: Vec<UniformSlot> = (0..MOONLET_UNIS_N)
                .map(|_| {
                    make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler)
                })
                .collect();
            let shell_unis: Vec<UniformSlot> = (0..SHELL_N)
                .map(|_| {
                    make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler)
                })
                .collect();
            let interior_uni =
                make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler);
            let impactor_uni =
                make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler);
            let sun_uni =
                make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler);
            // Rayleigh optical depths from the EMERGENT surface pressure (planet::earth's declared
            // atmosphere mass) — the blue marble is derived from the air, never painted (docs/26).
            let atm_tau = crate::atmosphere::rayleigh_tau(
                crate::planet::earth().surface_pressure() / 101_325.0,
            );
            let atm_twilight = {
                let e = crate::planet::earth();
                let h = mats
                    .iter()
                    .find(|m| m.id == "air")
                    .map(|air| {
                        crate::atmosphere::scale_height(air, 288.0, e.gravity_at(e.radius()))
                    })
                    .unwrap_or(0.0);
                crate::atmosphere::twilight_half_angle(h, e.radius())
            };
            let moon_unis: Vec<UniformSlot> = (0..num_moons * MOON_SHELL_N)
                .map(|_| {
                    make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler)
                })
                .collect();
            let pipeline = build_space_pipeline(&device, &bind_layout, config.format);
            let globe_pipeline = build_globe_pipeline(
                &device,
                &bind_layout,
                config.format,
                wgpu::BlendState::REPLACE,
            );
            let globe_uni =
                make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler);
            // The descent corridor's ground cap: Terra's exact recipe — the same shader, alpha-blended
            // for the cross-fade, a writable vertex buffer rebuilt per frame over a fixed topology.
            let cap_pipeline = build_globe_pipeline(
                &device,
                &bind_layout,
                config.format,
                wgpu::BlendState::ALPHA_BLENDING,
            );
            let cap_gpu = make_dynamic_mesh(
                &device,
                "corridor-cap",
                crate::terra::segment::SegmentRes::new(SEG_RINGS, SEG_SPOKES).vertex_count(),
                &crate::terra::segment::segment_indices(crate::terra::segment::SegmentRes::new(
                    SEG_RINGS, SEG_SPOKES,
                )),
            );
            let cap_uni =
                make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler);
            // GPU SPH deformable-Earth impact (stage 4c.4): its instanced-particle pipeline + a camera
            // uniform (reuses the uniform-only `bind_layout`; the buffer is sized for `SphCam`).
            // The SPH particle render is NOT a textured surface — it draws particles from the physics
            // buffer and needs only a camera uniform. It gets its own layout rather than carrying the
            // surface layout's material arrays. (Universality is about every SURFACE being the same
            // rendered world; it is not a reason to hand texture bindings to a particle shader.)
            let particle_bind_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("particle-bind-layout"),
                    entries: &[uniform_entry(
                        0,
                        wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    )],
                });
            let sph_pipeline = build_sph_pipeline(&device, &particle_bind_layout, config.format);
            let make_particle_cam = |label: &str| {
                let buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: std::mem::size_of::<crate::gpu_sph::SphCam>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout: &particle_bind_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buf.as_entire_binding(),
                    }],
                });
                UniformSlot { buf, bind }
            };
            let sph_cam = make_particle_cam("sph-cam");
            // The declared site's particles render through the SAME billboard pipeline (one
            // particle look, not two), with their own camera slot.
            let site_cam = make_particle_cam("site-cam");

            // The real three-body system in SI units: [Sun, Earth, Moon] (orbit.rs). The Earth carries
            // its true heliocentric velocity and the Moon co-moves with it plus its own orbital speed,
            // so the whole nesting is emergent — the Moon stays bound to the Earth while the Earth
            // orbits the Sun (verified by `orbit::sun_earth_moon_system_is_bound`), not hand-placed.
            // The Sun both holds the system (gravity) and lights it. At this zoom it sits ~23,000
            // display units off-frame, so it is the *light source*, not a drawn disk — the scale-
            // adaptive choice (docs/17): render what matters at this scale.
            let mut bodies = vec![
                crate::orbit::Body {
                    pos: glam::DVec3::ZERO,
                    vel: glam::DVec3::ZERO,
                    // The Sun's mass EMERGES from its declared composition (planet::sun), like Earth's
                    // from PREM — the constant is retired from the source of truth.
                    mass: crate::planet::sun().total_mass(),
                },
                crate::orbit::Body {
                    pos: glam::DVec3::new(AU_M, 0.0, 0.0),
                    vel: glam::DVec3::new(0.0, EARTH_HELIO_SPEED, 0.0),
                    mass: earth_mass(),
                },
            ];
            // Moons on the same circular orbit. For two, place them on OPPOSITE sides and give the
            // second the opposite tangential velocity, so both orbit the Earth the same way and stay
            // diametrically opposed — the symmetric "de-orbit both at once" stress test.
            for i in 0..num_moons {
                let side = if i == 0 { 1.0 } else { -1.0 };
                bodies.push(crate::orbit::Body {
                    pos: glam::DVec3::new(AU_M + side * MOON_DIST_M, 0.0, 0.0),
                    vel: glam::DVec3::new(0.0, EARTH_HELIO_SPEED + side * MOON_SPEED, 0.0),
                    mass: moon_mass(),
                });
            }
            let acc = crate::orbit::accelerations(&bodies);
            let initial_bodies = bodies.clone();
            // Modern Earth: the measured sidereal day, spin axis ⊥ the orbital (x-y) plane.
            let spin_l = glam::DVec3::new(0.0, 0.0, 1.0)
                * (crate::tides::moment_of_inertia(earth_mass(), earth_radius_m())
                    * (2.0 * std::f64::consts::PI / 86_164.0));

            // Body colours derived from a real composition, aggregated (docs/17) — NOT hand-picked.
            // Earth: ~71% ocean water, ~24% continental (granitic) rock, ~5% polar ice. This EXCLUDES
            // the atmosphere, so there is no Rayleigh-scattered "blue marble" blue — that blue is an
            // atmospheric effect we don't yet model, and faking it here would be a fudge. Moon: maria
            // basalt; the brighter highland anorthosite isn't in the DB yet, so the Moon renders darker
            // than reality until it's added (a flagged data gap, not a paint job).
            // The interior sphere's material/temperature: the layer at the depth the crater exposes
            // (the cap bottom) — for a Moon-scale impact that is the top of the molten outer core.
            // The bulk just under the crust: OPAQUE DARK ROCK. It sits right beneath the shell grains
            // so nothing shines through the gaps between them — the old white-hot sphere (meant as the
            // crater floor, 3,500 km down) bled through the gaps and made Earth look lit from WITHIN,
            // reading as anti-sun lighting (Robin's "anti-raycasting"). Depth-glow belongs to the
            // CRATER alone, whose wall grains carry the real layer temperatures.
            let int_mat = &mats[materials::index_of(&mats, "basalt")];
            let interior_tint = [int_mat.albedo[0], int_mat.albedo[1], int_mat.albedo[2], 1.0];
            let interior_glow = [0.0f32; 4];
            let earth_comp = [
                (materials::index_of(&mats, "water"), 0.71),
                (materials::index_of(&mats, "granite"), 0.24),
                (materials::index_of(&mats, "ice"), 0.05),
            ];
            let moon_comp = [(materials::index_of(&mats, "basalt"), 1.0)];
            let rgba = |c: &materials::Composition| {
                let a = materials::aggregate_albedo(c, &mats);
                [a[0], a[1], a[2], 1.0]
            };
            let earth_tint = rgba(&earth_comp);
            let moon_tint = rgba(&moon_comp);

            let camera = Camera {
                yaw: 0.6,
                pitch: 0.5,
                zoom: 1.0,
                base_distance: (MOON_DIST_M * display_scale() * crate::arc::WHOLE_ORBIT_MARGIN)
                    as f32,
                pan: Vec3::ZERO,
            };

            log::info!(
                "orbit demo ready: Sun+Earth+{num_moons} moon(s), sun-lit, {ORBIT_TIME_SCALE:.0}x time"
            );
            Ok(OrbitDemo {
                scene_epoch: None,
                impact_def: Default::default(),
                surface,
                device,
                queue,
                config,
                depth_view,
                pipeline,
                sphere_gpu,
                moon_unis,
                bodies,
                acc,
                time_scale: ORBIT_TIME_SCALE,
                camera,
                focus: 1, // start on the planet
                earth_tint,
                moon_tint,
                body_meta: Vec::new(),
                initial_bodies,
                impacted: false,
                moon_hit: vec![false; num_moons],
                impact_energy_j: 0.0,
                mats,
                impactor_radius: moon_radius_m(),
                impactor_mass: moon_mass(),
                sim_since_impact: 0.0,
                spin_l,
                initial_spin_l: spin_l,
                spin_angle: 0.0,
                geologic: false,
                geo_moonlets: Vec::new(),
                geo_rate_yr_s: 1_000.0,
                shell_unis,
                interior_uni,
                sun_uni,
                atm_tau,
                atm_twilight,
                stars: None,
                impactor_uni,
                globe_pipeline,
                globe_mesh: None,
                globe_uni,
                cap_pipeline,
                cap_gpu,
                cap_uni,
                cap_verts: Vec::new(),
                earth_surface: None,
                interior_tint,
                interior_glow,
                snaps: std::collections::VecDeque::new(),
                phys_clock: 0.0,
                real_accum: 0.0,
                debris_unis,
                gpu_sph: None,
                sph_pipeline,
                sph_cam,
                sph_active: false,
                sph_dt: 0.0,
                sph_soft: 1.0,
                sph_sim_t: 0.0,
                sph_dt_aftermath: 0.0,
                sph_substeps: 6,
                sph_snapshot: Vec::new(),
                sph_eos: Vec::new(),
                sph_merge_budget: 0,
                sph_promoted: Vec::new(),
                sph_promote_tick: 0,
                sph_prov_to_body: Vec::new(),
                pending_sph_route: None,
                sph_cap: None,
                sph_phase: SphPhase::Dynamics,
                armed_drop: None,
                render_blend: 0.0, // pretty by default (docs/42)
                gpu_impact_site: None,
                gpu_crater_frac: 0.0,
                gpu_crater_depth_frac: 0.0,
                gpu_crater_r_frac: 0.0,
                gpu_crater_logged: -1,
                site_spec: None,
                site_trigger: crate::site::SiteTrigger::new(),
                site: None,
                site_gauge: crate::recohere::SettleGauge::new(),
                site_status: String::new(),
                site_refused: false,
                site_buf: None,
                site_cam,
                site_dist_m: 0.0,
                site_resolve_at_m: 0.0,
                sph_snapshot_gen: 0,
                site_sampled_gen: 0,
                site_window: crate::site::EventWindow::default(),
                site_dyn: None,
                pi_prediction: None,
                pi_line: String::new(),
                arc_octave_s: None,
                arc_declared_scale: ORBIT_TIME_SCALE,
                arc: None,
                arc_saved_scale: ORBIT_TIME_SCALE,
            })
        }

        /// docs/42: set the pretty⇄physics render blend (0 = pretty sphere, 1 = raw physics particles).
        /// RETIRED as a control. Kept only so an old page cannot break; it does nothing.
        ///
        /// This was a "Pretty ⇄ Physics" slider cross-fading the resolved surface against the particle
        /// field — two representations of the SAME matter, blended by hand. That is why the surface and
        /// the particles were seen racing each other, the surface being swallowed by the disk and
        /// reappearing: nothing decided where the matter actually was, so both answers were drawn at once
        /// and a dial chose how much of each. Representation is now derived from the matter itself
        /// (`body_coherence`).
        pub fn set_render_blend(&mut self, _blend: f32) {}

        /// docs/43 — load a "system" world (Sun/Earth/Moon initial conditions) from JSON, replacing the built-in
        /// constants with declared DATA. `create(canvas, num_moons)` must have been called with the world's moon
        /// count first (the GPU per-moon uniforms are sized there); this sets the physical initial conditions
        /// (positions/velocities/masses), the planet's spin, the composition-derived tints, the time scale, the
        /// frame-of-reference focus, and the orbit-camera framing. The deorbit stays a user control
        /// (`brake_moon`/`drop_moon`) — no scripted outcome. (The planet's render radius still uses the
        /// `earth_radius_m()` constant in v1; per-body render radii from data is a flagged follow-up.)
        /// Hand the scene the real star catalogue (`sky/stars.bin`). The engine derives each star's
        /// temperature and colour from its measured colour index; nothing about the sky is authored.
        pub fn load_star_catalog(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            let stars = crate::sky::parse_catalog(bytes).map_err(|e| JsValue::from_str(&e))?;
            log::info!("sky: {} catalogued stars", stars.len());
            self.stars = Some(StarField::new(&self.device, self.config.format, &stars));
            Ok(())
        }

        /// This scene's air — Earth's, from Earth's own definition, at the shared exposure. Identical to
        /// `Terra::air()` by construction: one body, one atmosphere.
        fn air(&self) -> Air {
            [
                self.atm_tau[0] as f32,
                self.atm_tau[1] as f32,
                self.atm_tau[2] as f32,
                crate::atmosphere::SUN_GAIN,
                self.atm_twilight as f32,
            ]
        }

        /// Hand the scene the DEFINITIVE Earth's surface rasters (the host fetches whatever
        /// `body_surface_urls("earth")` names). Builds the same globe mesh Terra builds, from the same
        /// shared builder — so this scene stops standing in a grain shell for the planet.
        #[allow(clippy::too_many_arguments)]
        pub fn load_earth_surface(
            &mut self,
            landmask: &[u8],
            lm_w: usize,
            lm_h: usize,
            elevation: &[u8],
            ev_w: usize,
            ev_h: usize,
            landcover: &[u8],
            lc_w: usize,
            lc_h: usize,
        ) -> Result<(), JsValue> {
            let body = crate::planet::body("earth");
            let def = body
                .surface
                .ok_or_else(|| JsValue::from_str("earth.json declares no surface"))?;
            // RGBA from the host's canvas decode; a missing/!=RGBA raster is treated as absent rather
            // than fatal, exactly as Terra treats it.
            let mk = |d: &[u8], w: usize, h: usize| {
                (!d.is_empty() && w > 0 && h > 0)
                    .then(|| crate::terra::raster::Raster::new(w, h, 4, d.to_vec()).ok())
                    .flatten()
            };
            let biome_mix = def.biome_mixtures(&self.mats);
            let surf = EarthSurface {
                landmask: mk(landmask, lm_w, lm_h),
                elevation: mk(elevation, ev_w, ev_h),
                landcover: mk(landcover, lc_w, lc_h),
                biome_mix,
                elev_range: def.elevation_range_m.unwrap_or([-11_000.0, 9_000.0]),
                relief_exag: def.relief_exaggeration.unwrap_or(1.0),
            };
            // Unit radius: the draw scales it to whichever radius this scene renders Earth at (real, or
            // the sub-scale SPH body during the impact), so one mesh serves both.
            let mesh = crate::terra::globe_mesh::build_body_globe(
                192,
                1.0,
                1.0 / earth_radius_m(),
                surf.relief_exag,
                &self.mats,
                &surf.biome_mix,
                surf.landmask.as_ref(),
                surf.elevation.as_ref(),
                surf.landcover.as_ref(),
                surf.elev_range,
            );
            log::info!(
                "space: definitive Earth built — {} triangles",
                mesh.indices.len() / 3
            );
            self.globe_mesh = Some(upload_mesh(&self.device, "earth-globe", &mesh));
            self.earth_surface = Some(surf);
            Ok(())
        }

        pub fn load_world(&mut self, world_json: &str) -> Result<(), JsValue> {
            use crate::terra::world_def::{BodyDef, World};
            let w = World::parse(world_json).map_err(|e| JsValue::from_str(&e))?;
            // **When this scene is set** — a world may name its epoch (docs/65: time is part of the
            // setting). The birth-of-the-Moon scene is proto-Earth; Terra is this afternoon. Same
            // Earth, different dates.
            self.scene_epoch = w.time.as_ref().and_then(|t| t.epoch);
            let defs = w
                .bodies
                .as_ref()
                .ok_or_else(|| JsValue::from_str("system world is missing a `bodies` array"))?;

            // Mass/radius resolve from an explicit field or a named profile (declared, not fudged). The Sun's mass
            // EMERGES from its composition (`planet::sun`), like the current hardcoded path.
            let body_mass = |d: &BodyDef| -> f64 { crate::declared_body_mass(d) };
            let body_radius = |d: &BodyDef| -> f64 { crate::declared_body_radius(d) };

            let mut bodies = Vec::with_capacity(defs.len());
            let mut planet_idx = 1usize;
            let mut moon_count = 0usize;
            self.body_meta.clear();
            for (i, d) in defs.iter().enumerate() {
                bodies.push(crate::orbit::Body {
                    pos: glam::DVec3::from_array(d.pos_m),
                    vel: glam::DVec3::from_array(d.vel_ms),
                    mass: body_mass(d),
                });
                // Tint: explicit override, else aggregated from the profile's real composition (docs/17) — the
                // borrow of `self.mats` is confined to this block, released before we mutate the tint fields.
                let tint = |profile: Option<&str>, mats: &[materials::Material]| -> [f32; 4] {
                    if let Some(t) = d.tint {
                        return [t[0], t[1], t[2], 1.0];
                    }
                    let comp: Vec<(usize, f32)> = match profile {
                        Some("earth") => vec![
                            (materials::index_of(mats, "water"), 0.71),
                            (materials::index_of(mats, "granite"), 0.24),
                            (materials::index_of(mats, "ice"), 0.05),
                        ],
                        Some("moon") => vec![(materials::index_of(mats, "basalt"), 1.0)],
                        _ => vec![(materials::index_of(mats, "granite"), 1.0)],
                    };
                    let a = materials::aggregate_albedo(&comp, mats);
                    [a[0], a[1], a[2], 1.0]
                };
                let this_tint = tint(d.profile.as_deref(), &self.mats);
                let role = match d.role.as_str() {
                    "star" => BodyRole::Star,
                    "planet" => BodyRole::Planet,
                    _ => BodyRole::Moon,
                };
                // ONE metadata entry per declared body — no overwriting. This is where the single-value
                // collapse is fixed: each moon keeps its OWN radius and tint.
                self.body_meta.push(BodyMeta {
                    radius_m: body_radius(d),
                    tint: this_tint,
                    role,
                    matter: crate::body_definition(d.profile.as_deref()),
                    materialized: false,
                });
                match d.role.as_str() {
                    "planet" => {
                        planet_idx = i;
                        self.earth_tint = this_tint;
                        if let Some(p) = d.spin_period_s {
                            // L = I·ω with the body's EMERGENT inertia (docs/58), so the declared day length
                            // reproduces when the rotation reads the same emergent I back (spin_inertia()).
                            let inertia = crate::body_definition(d.profile.as_deref())
                                .map(|b| b.moment_of_inertia())
                                .unwrap_or_else(|| {
                                    crate::tides::moment_of_inertia(body_mass(d), body_radius(d))
                                });
                            self.spin_l = glam::DVec3::new(0.0, 0.0, 1.0)
                                * (inertia * (2.0 * std::f64::consts::PI / p));
                            self.initial_spin_l = self.spin_l;
                        }
                    }
                    "moon" => {
                        moon_count += 1;
                        // Kept for the birth path and back-compat; the orbital render/collision now reads
                        // `body_meta`. With multiple moons these hold the last one, which is exactly the
                        // bug body_meta replaces — so nothing new should come to depend on them.
                        self.moon_tint = this_tint;
                        self.impactor_radius = body_radius(d);
                        self.impactor_mass = body_mass(d);
                    }
                    _ => {}
                }
            }
            // `moon_unis` is a fixed render pool (drawn per moon body); guard only that we don't exceed it.
            if moon_count > self.moon_unis.len() {
                return Err(JsValue::from_str(&format!(
                    "world declares {moon_count} moon(s), exceeding the render pool of {}",
                    self.moon_unis.len()
                )));
            }

            self.bodies = bodies;
            self.acc = crate::orbit::accelerations(&self.bodies);
            self.initial_bodies = self.bodies.clone();
            // Per-moon impact-hit flags sized to this world (the physics state; `moon_unis` is just the pool).
            self.moon_hit = vec![false; moon_count];

            if let Some(t) = w.time.as_ref() {
                self.time_scale = t.scale.clamp(1.0, 2_000_000.0);
            }
            // The pacing law's anchors: the DECLARED celestial scale (kept apart from the mutable
            // time_scale) and the world's declared arc pacing, if it declares an arc at all.
            self.arc_declared_scale = self.time_scale;
            self.arc_octave_s = w.arc.as_ref().map(|a| a.octave_s);
            self.arc = None;

            // Orbit camera: frame-of-reference focus body + framing.
            self.focus = planet_idx;
            if let Some(c) = w.camera.as_ref() {
                if let Some(f) = c.focus.as_deref() {
                    if let Some(idx) = defs.iter().position(|d| d.name == f) {
                        self.focus = idx;
                    }
                }
                if let Some(y) = c.yaw {
                    self.camera.yaw = y as f32;
                }
                if let Some(p) = c.pitch {
                    self.camera.pitch = p as f32;
                }
                if let Some(z) = c.zoom {
                    self.camera.zoom = z as f32;
                }
            }
            // Frame the view on the planet→moon separation (fall back to the current base distance).
            if let Some(moon) = self.bodies.get(planet_idx + 1) {
                let sep = (moon.pos - self.bodies[planet_idx].pos).length();
                if sep > 0.0 {
                    self.camera.base_distance =
                        (sep * display_scale() * crate::arc::WHOLE_ORBIT_MARGIN) as f32;
                }
            }

            log::info!(
                "orbit demo: loaded system world '{}' — {} bodies, {moon_count} moon(s), {:.0}x time",
                w.name,
                self.bodies.len(),
                self.time_scale,
            );
            Ok(())
        }

        // --- Orbital-decay controls: brake the Moon and watch its orbit tighten into a crash. ---

        /// Halve **every** moon's velocity relative to the Earth — the orbital-decay control (all moons
        /// at once, so the two-moon scene de-orbits symmetrically). Each tap tightens the orbit (watch
        /// `moon_perigee_km` fall); a few taps drop the perigee below the surface and they crash. (A
        /// single halving still misses — real orbital mechanics, not a trick.)
        pub fn brake_moon(&mut self) {
            let earth_vel = self.bodies[1].vel;
            for i in 2..self.bodies.len() {
                self.bodies[i].vel = earth_vel + (self.bodies[i].vel - earth_vel) * 0.5;
            }
        }

        /// Route a set of live bodies (by `self.bodies` index) through the ONE SPH engine (docs/58) — the
        /// SOLE collision path, shared by the explicit Drop and by a collision the orbital phase detected.
        /// `bodies[0]` is the planet (becomes `prov 0`); the rest are the impactor(s). Each is particalised
        /// from its OWN matter, relaxed far apart, then resolved on the GPU at the LIVE geometry — the same
        /// machine birth uses, N bodies at once (a two-moon world resolves all three in one collision). A
        /// body with no matter (a bare point mass) is skipped; fewer than two with matter ⇒ nothing to do.
        fn route_bodies_to_sph(&mut self, bodies: &[usize]) {
            if self.sph_active {
                return; // a collision is already resolving
            }
            // Clone the matter off `self` first so we can then borrow `&mut self`; map each SPH source
            // (`prov`) to its `self.bodies` index (`prov 0` = the planet, `bodies[0]`). The Approaching
            // phase then integrates the fall and hands off at the tidal/contact distance.
            let mut mats: Vec<crate::planet::LayeredBody> = Vec::new();
            let mut prov_to_body: Vec<usize> = Vec::new();
            for &idx in bodies {
                if let Some(m) = self.body_meta.get(idx).and_then(|b| b.matter.clone()) {
                    mats.push(m);
                    prov_to_body.push(idx);
                }
            }
            if mats.len() < 2 {
                return; // need a planet + at least one impactor with matter
            }
            // The impactors ARE particles from here on: mark them materialised so their point-mass shells
            // stop being drawn (during the resolve `sph_active` hides everything; after a geologic hand-off
            // this keeps them retired). `prov_to_body[0]` is the planet — it stays a body.
            for &idx in &prov_to_body[1..] {
                if let Some(m) = self.body_meta.get_mut(idx) {
                    m.materialized = true;
                }
            }
            let planet = prov_to_body[0];
            let planet_m = self.bodies[planet].mass.max(1.0);
            let max_ratio = prov_to_body[1..]
                .iter()
                .map(|&i| self.bodies[i].mass / planet_m)
                .fold(0.0, f64::max);
            // RESOLUTION-ON-DEMAND (docs/39): a SMALL impactor shocks only a CAP of the target — its deep
            // interior stays an abstract BULK (configured at Assembling). Resolve the impactor(s) whole + the
            // cap, not the whole planet (Law III). A COMPARABLE-mass impactor is a giant impact and still
            // resolves everything (the whole-body branch below — birth's regime, unchanged).
            if max_ratio < CAP_MASS_RATIO && mats.len() >= 2 {
                // Particalize a CAP of the target at each impact site NOW — the site is the impactor's current
                // direction, and a de-orbited moon falls radially, so it doesn't change. One dome per site,
                // unioned. The cap relaxes SEATED on the bulk (below) so it holds hydrostatic before the shock.
                let planet_r = mats[0].radius();
                let planet_mass = mats[0].total_mass();
                let mut cap_body: Option<crate::hydrostatic::HydroBody> = None;
                let mut r_core = planet_r;
                let mut cap_mass = 0.0f64;
                for &bi in &prov_to_body[1..] {
                    let site_dir =
                        (self.bodies[bi].pos - self.bodies[planet].pos).normalize_or_zero();
                    let imp_r = self
                        .body_meta
                        .get(bi)
                        .map_or(self.impactor_radius, |m| m.radius_m);
                    let cap_radius = (2.0 * imp_r).min(0.55 * planet_r);
                    let c = crate::hydrostatic::HydroBody::particalize_cap(
                        &mats[0], site_dir, cap_radius, 2000,
                    );
                    r_core = r_core.min(c.r_core);
                    cap_mass += c.body.mass.iter().sum::<f64>();
                    match &mut cap_body {
                        Some(cb) => cb.append(c.body),
                        None => cap_body = Some(c.body),
                    }
                }
                let cap_body = cap_body.expect("≥1 impactor with matter");
                let bulk_mass = (planet_mass - cap_mass).max(0.0);
                // Relax the impactor(s) as free bodies (whole) alongside the cap, all seated on the bulk.
                let m_i = mats[1].total_mass() / 1500.0;
                let list: Vec<(&crate::planet::LayeredBody, usize)> = mats[1..]
                    .iter()
                    .map(|m| (m, (m.total_mass() / m_i).round().max(200.0) as usize))
                    .collect();
                self.begin_cap_relax(&cap_body, &list, 40.0, r_core, bulk_mass);
                self.sph_prov_to_body = prov_to_body[1..].to_vec(); // prov 1.. → impactor body (prov 0 = the cap)
                self.sph_cap = Some(CapPlan {
                    planet,
                    impactors: prov_to_body[1..].to_vec(),
                    r_core,
                    bulk_mass,
                });
                self.focus = planet;
                self.camera.zoom = 0.4;
                return;
            }
            // WHOLE-BODY (comparable masses — a giant impact): equal particle mass across every body (the
            // planet, prov 0, sets it), so none biases the dynamics.
            let m_i = mats[0].total_mass() / 2400.0;
            let list: Vec<(&crate::planet::LayeredBody, usize)> = mats
                .iter()
                .map(|m| (m, (m.total_mass() / m_i).round().max(50.0) as usize))
                .collect();
            self.begin_sph_relax(&list, 40.0);
            self.sph_prov_to_body = prov_to_body;
            self.sph_cap = None;
            self.focus = bodies[0];
            self.camera.zoom = 0.4;
        }

        /// The Drop control. On a world that declares a ground site, this ARMS for the launch
        /// window instead of releasing: the intercept solve (`crate::intercept`) integrates the
        /// same fall the scene will run and picks the release time at which the site rotates
        /// under the impact point - the ball never moves, the trajectory is never bent, only the
        /// release time is chosen, which is what any real mission does. The release then fires
        /// itself in `step_substep` when the countdown reaches the window. A world without a
        /// site (or one whose solve cannot find a window) keeps the instant drop.
        pub fn drop_moon(&mut self) {
            if let Some(w) = self.solve_site_drop_window() {
                log::info!(
                    "drop armed: window in {:.0} sim s, contact {:.2} sim days out (residual {:.4} deg, plane offset {:.1} deg)",
                    w.release_in_s,
                    w.impact_in_s / 86_400.0,
                    w.residual_rad.to_degrees(),
                    w.plane_offset_rad.to_degrees(),
                );
                self.armed_drop = Some(w);
                return;
            }
            self.release_drop();
        }

        /// Cancel every moon's velocity relative to the planet — they drop straight in and crash — then
        /// route the planet + moon(s) through the ONE SPH engine (docs/58). The dramatic version (every
        /// moon at once): a two-moon world resolves all three bodies in a single N-body SPH collision.
        fn release_drop(&mut self) {
            let planet = self.planet_idx();
            let planet_vel = self.bodies[planet].vel;
            for i in 2..self.bodies.len() {
                if i != planet {
                    self.bodies[i].vel = planet_vel; // de-orbit: fall straight in
                }
            }
            let set: Vec<usize> = std::iter::once(planet)
                .chain((2..self.bodies.len()).filter(|&i| i != planet))
                .collect();
            self.route_bodies_to_sph(&set);
        }

        /// Solve the launch window for the declared site from the CURRENT deterministic state:
        /// the N-body positions, the planet's declared spin (day length and accumulated angle),
        /// and the site's lat/lon. `None` when no site is declared, the planet does not spin,
        /// or no moon is left to drop - the callers then keep the instant behaviour.
        fn solve_site_drop_window(&self) -> Option<crate::intercept::DropWindow> {
            let spec = self.site_spec.as_ref()?;
            let p = self.planet_idx();
            let drop = self
                .body_meta
                .iter()
                .position(|m| matches!(m.role, BodyRole::Moon) && !m.materialized)?;
            let inertia = self.spin_inertia();
            let omega = if inertia > 0.0 {
                self.spin_l.length() / inertia
            } else {
                0.0
            };
            let spin = crate::intercept::Spin {
                axis: self.spin_l.try_normalize().unwrap_or(glam::DVec3::Z),
                omega_rad_s: omega,
                angle_rad: self.spin_angle,
            };
            let r_contact = self
                .body_meta
                .get(p)
                .map_or(earth_radius_m(), |m| m.radius_m)
                + self
                    .body_meta
                    .get(drop)
                    .map_or(self.impactor_radius, |m| m.radius_m);
            // The solver integrates the scene's own law; its step only needs to resolve the
            // fall, so bound it: at the top fast-forward the scene substep is ~2,000 s, far too
            // coarse a statement of the trajectory to time a window against, and at 1x it is
            // milliseconds, which would spend minutes of compute on a days-long forecast. The
            // release itself still fires on the LIVE substep grid, whatever its size.
            let dt = (self.time_scale / 960.0).clamp(30.0, 120.0);
            crate::intercept::solve_drop_window(
                &self.bodies,
                p,
                drop,
                r_contact,
                &spin,
                spec.lat_deg,
                spec.lon_deg,
                dt,
            )
        }

        /// SIM seconds until an armed drop releases (-1 when nothing is armed) - the HUD's
        /// "window in T−…" countdown.
        pub fn drop_window_s(&self) -> f64 {
            self.armed_drop.map_or(-1.0, |w| w.release_in_s)
        }

        /// SIM seconds from now to the armed drop's forecast contact (−1 when nothing is armed).
        pub fn drop_window_impact_s(&self) -> f64 {
            self.armed_drop.map_or(-1.0, |w| w.impact_in_s)
        }

        /// Restore the original Sun–Earth–Moon(s) state (undo braking / the crash).
        pub fn reset_moon(&mut self) {
            self.bodies = self.initial_bodies.clone();
            self.acc = crate::orbit::accelerations(&self.bodies);
            self.armed_drop = None; // an armed window belongs to the state this just discarded
            self.impacted = false;
            self.impact_energy_j = 0.0;
            for hit in &mut self.moon_hit {
                *hit = false;
            }
            // Restore an intact world: un-materialise every moon (a resolved one becomes a body again on
            // Reset), stop any live GPU resolve, and clear the aftermath clock.
            for m in self.body_meta.iter_mut() {
                m.materialized = false;
            }
            self.sph_active = false;
            self.gpu_sph = None;
            self.sph_snapshot.clear();
            self.sph_prov_to_body.clear();
            self.pending_sph_route = None;
            self.sph_cap = None;
            self.sim_since_impact = 0.0;
            self.geologic = false;
            self.geo_moonlets.clear();
            // Restore the pristine spin: without this the impact-induced spin_l survived a world reset,
            // conjuring angular momentum from the previous run (render-truth bug, docs/28).
            self.spin_l = self.initial_spin_l;
            self.spin_angle = 0.0;
            // Drop the snapshot history — the renderer must not interpolate across the reset.
            self.snaps.clear();
            self.real_accum = 0.0;
        }

        /// Predicted perigee (closest approach) of the Moon's orbit about the Earth, in km — or a
        /// negative value if the orbit is unbound. Drops below Earth's radius (~6,371 km) before a crash.
        pub fn moon_perigee_km(&self) -> f64 {
            // During the impact the orbital element belongs to the PARTICLE bodies (see `sph_body_state`);
            // reading `bodies[2]` there quoted the default Moon's untouched 380,661 km for the whole run.
            let (rel_pos, rel_vel, mu) = match self.sph_body_state() {
                Some([(me, ec, ev), (mt, tc, tv)]) => {
                    (tc - ec, tv - ev, crate::orbit::G * (me + mt))
                }
                None => (
                    self.bodies[2].pos - self.bodies[1].pos,
                    self.bodies[2].vel - self.bodies[1].vel,
                    crate::orbit::G * (self.bodies[1].mass + self.bodies[2].mass),
                ),
            };
            crate::orbit::perigee(rel_pos, rel_vel, mu).map_or(-1.0, |p| p / 1000.0)
        }

        /// The separation at which the two bodies' SURFACES meet, km — from their definitions, so the
        /// HUD's "brake below this to crash" line is right for whichever bodies the scene placed. It was
        /// hardcoded to Earth + Moon (8,108 km); with Theia as the impactor the real figure is 9,761.
        pub fn contact_distance_km(&self) -> f64 {
            (self.impact_def.target.radius_m() + self.impact_def.impactor.radius_m()) / 1000.0
        }

        /// The Moon's speed relative to the Earth, km/s (HUD). On a true drop this *climbs* all the way
        /// to impact (~11 km/s) — there is no drag or terminal velocity in vacuum. An eccentric orbit
        /// (a partial brake) instead slows at apogee and speeds at perigee (Kepler), which can look
        /// like "flattening" but is the opposite of drag.
        pub fn moon_speed_kms(&self) -> f64 {
            // Same correction as `moon_distance_km`: during the impact the closing speed is a property of
            // the particle field, not of an N-body slot that nothing is updating.
            if let Some([(_, _, ev), (_, _, tv)]) = self.sph_body_state() {
                return (tv - ev).length() / 1000.0;
            }
            (self.bodies[2].vel - self.bodies[1].vel).length() / 1000.0
        }

        /// Whether the Moon has struck the planet (HUD).
        pub fn has_impacted(&self) -> bool {
            self.impacted
        }

        /// Number of resolved matter particles (0 until a collision resolves bodies into matter): a
        /// HUD diagnostic. Resolved impact matter lives in the GPU SPH machine, so this counts its
        /// latest read-back; the CPU Aggregate cloud this used to count is retired (docs/58 #7).
        pub fn debris_count(&self) -> u32 {
            if self.sph_active {
                self.sph_snapshot.len() as u32
            } else {
                0
            }
        }

        /// Energy (J) the impact released — what would become heat, fracture, and ejecta.
        pub fn impact_energy_j(&self) -> f64 {
            self.impact_energy_j
        }

        /// The Moon's gravitational binding energy (J), for comparison: impact ≫ binding ⇒ it shatters.
        pub fn moon_binding_energy_j(&self) -> f64 {
            crate::orbit::binding_energy(moon_mass(), moon_radius_m())
        }

        /// The Earth's gravitational binding energy (J). The Moon impact is a small fraction of this,
        /// so the Earth is not destroyed — it takes a planet-scale crater (docs/19 LOD bridge).
        pub fn earth_binding_energy_j(&self) -> f64 {
            crate::orbit::binding_energy(earth_mass(), earth_radius_m())
        }

        /// Current time multiplier (sim-seconds per real-second), for the HUD.
        pub fn time_scale_value(&self) -> f64 {
            self.time_scale
        }

        /// Cycle the view's frame of reference through the Earth and each moon. The focused body becomes
        /// the origin; everything else moves honestly around it (docs/17).
        pub fn cycle_focus(&mut self) {
            let last = self.bodies.len() - 1; // last moon
            self.focus = if self.focus >= last {
                1
            } else {
                self.focus + 1
            };
            // Same promise as the focus buttons: choosing a body frames that body, so any pan
            // offset snaps back to zero.
            self.camera.pan = Vec3::ZERO;
        }

        /// Put the camera's frame of reference on Earth (origin re-centres on the planet).
        /// Choosing a focus also snaps any pan offset back to zero: the button's promise is
        /// "frame THIS body", and a leftover offset would frame something else.
        /// This scene's clock: its declared epoch, or now.
        fn scene_epoch_s(&self) -> f64 {
            self.scene_epoch
                .unwrap_or_else(crate::orbit::unix_now_seconds)
        }

        pub fn focus_earth(&mut self) {
            self.focus = 1;
            self.camera.pan = Vec3::ZERO;
        }

        /// Put the camera's frame of reference on the Moon (or, once it has shattered, the impact site,
        /// since the shattered body's point mass stays parked there — so this frames the debris/crater).
        /// Like `focus_earth`, this snaps any pan offset back to zero.
        pub fn focus_moon(&mut self) {
            if self.bodies.len() > 2 {
                self.focus = 2;
                self.camera.pan = Vec3::ZERO;
            }
        }

        /// Pan the view: translate the look target off the focused body by a pointer delta in
        /// DEVICE pixels (the render surface's own pixel grid). The offset is held in the frame
        /// that rides the focused body (see `render::Camera::pan`), so the framing follows the
        /// body through its orbit rather than smearing against inertial space; the focus buttons
        /// snap it back to zero. Representation only — no matter moves.
        pub fn pan_view(&mut self, dx_px: f32, dy_px: f32) {
            if self.arc.is_some() {
                return; // the demo arc is the camera driver until arc_stop hands back
            }
            self.camera
                .pan_by_pixels(dx_px, dy_px, 0.9, self.config.height.max(1) as f32);
        }

        /// Name of the currently-focused body (for the HUD / focus button).
        pub fn focus_label(&self) -> String {
            if self.focus == 1 {
                return "Earth".to_string();
            }
            // Two-moon scene → "Moon A" / "Moon B"; single moon → just "Moon".
            if self.bodies.len() > 3 {
                format!("Moon {}", (b'A' + (self.focus - 2) as u8) as char)
            } else {
                "Moon".to_string()
            }
        }

        pub fn set_orbit(&mut self, yaw: f32, pitch: f32, zoom: f32) {
            if self.arc.is_some() {
                return; // the demo arc is the camera driver until arc_stop hands back
            }
            self.camera.yaw = yaw;
            self.camera.pitch = pitch.clamp(-1.5, 1.5);
            // Floor low enough for the descent-follow camera (25% of lunar distance ≈ zoom 0.147).
            self.camera.zoom = zoom.clamp(0.05, 6.0);
        }

        pub fn resize(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 {
                return;
            }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = create_depth_view(&self.device, width, height);
        }

        /// Double/halve the aftermath speed (the ⏩/⏪ controls after an impact). With resolved impact
        /// matter living on the SPH machine (which paces itself by the frame budget, docs/42), the
        /// only aftermath rate left to nudge is GEOLOGIC time. Returns the current rate.
        /// (The CPU birth scenario `start_birth` and its Aggregate aftermath multiplier are retired,
        /// docs/58 #7; the birth scene runs `start_gpu_impact`.)
        pub fn nudge_aftermath_rate(&mut self, faster: bool) -> f64 {
            if self.geologic {
                self.geo_rate_yr_s =
                    (self.geo_rate_yr_s * if faster { 2.0 } else { 0.5 }).clamp(100.0, 1.0e6);
            }
            self.geo_rate_yr_s
        }

        /// Live disk statistics, the HUD's answer to "did we achieve orbit?". In GEOLOGIC time the
        /// promoted moonlets ARE the state, reported as JSON
        /// {"bound":M,"escaped":0,"biggest":M,"clumps":N} in lunar masses. While a particle field is
        /// live, the ONE measurement of it is the SPH read-back (`gpu_disk_stats_json`,
        /// `gpu_sph::disk_stats_json`); the CPU `Aggregate` twin of that measurement is retired
        /// (docs/58 #7, docs/46 rows 1/3), so this delegates rather than answering a second way.
        pub fn disk_stats_json(&self) -> String {
            let m_moon = moon_mass(); // the HUD's "lunar masses" unit - the definition's, not a literal
            if self.geologic {
                let bound: f64 = self.geo_moonlets.iter().map(|m| m.mass).sum();
                let biggest = self.geo_moonlets.iter().map(|m| m.mass).fold(0.0, f64::max);
                return format!(
                    "{{\"bound\":{:.3},\"escaped\":0,\"biggest\":{:.3},\"clumps\":{}}}",
                    bound / m_moon,
                    biggest / m_moon,
                    self.geo_moonlets.len()
                );
            }
            self.gpu_disk_stats_json()
        }

        /// Enter GEOLOGIC time (docs/27, docs/35 stage 5): promote the live SPH disk's bound clumps
        /// to moonlets around the real Earth, retire the particle sim, and hand evolution to the
        /// validated secular law. This is the ONE hand-off; the CPU `Aggregate` twin it once had
        /// is retired (docs/58 #7, docs/46 rows 1/3).
        pub fn enter_geologic_time(&mut self) {
            if !self.sph_active {
                return; // no live particle field, nothing to promote
            }
            let moonlets = crate::gpu_sph::disk_moonlets(&self.sph_snapshot, earth_radius_m());
            if moonlets.is_empty() {
                return; // no orbiting disk yet — keep the impact running rather than blanking the scene
            }
            self.geo_moonlets = moonlets;
            self.sph_active = false;
            self.gpu_sph = None;
            self.sph_phase = SphPhase::Dynamics;
            self.camera.zoom = 1.0; // back out from the impact framing to the Earth–Moon geologic view
            self.geologic = true;
        }

        /// Earth's day length (hours) from its live spin state — ∞ (0.0 returned as -1) if not spinning.
        pub fn earth_day_hours(&self) -> f64 {
            // Measured from the particle field while the impact is live — the post-impact day length is
            // the quantity this scene exists to let EMERGE, and the HUD was printing the modern 23.9 h
            // from an N-body slot the impact never touches.
            if let Some(t) = self.sph_target_spin_period_s() {
                return t / 3600.0;
            }
            let t = crate::tides::spin_period_from_inertia(self.spin_l, self.spin_inertia());
            if t.is_finite() {
                t / 3600.0
            } else {
                -1.0
            }
        }

        /// SIM seconds since the impact (−1 before it), for the HUD's T+ aftermath clock. Only
        /// geologic time accumulates it now; the live SPH aftermath reports through its own
        /// stats (`gpu_disk_stats_json`), exactly as it did before the CPU cloud retired.
        pub fn sim_since_impact_s(&self) -> f64 {
            if self.geologic {
                self.sim_since_impact
            } else {
                -1.0
            }
        }

        /// Real seconds until the forecast impact (−1 once it has happened / no closing approach).
        /// The countdown IS the simulation's own forecast — distance and closing speed from the live
        /// N-body state, divided by the observable time rate.
        pub fn impact_countdown_s(&self) -> f64 {
            if self.impacted || self.bodies.len() < 3 {
                return -1.0;
            }
            let rel = self.bodies[2].pos - self.bodies[1].pos;
            let relv = self.bodies[2].vel - self.bodies[1].vel;
            let dist = rel.length() - (earth_radius_m() + self.impactor_radius);
            let closing = -rel.dot(relv) / rel.length().max(1.0);
            if closing <= 0.0 {
                return -1.0;
            }
            (dist / closing) / self.time_scale
        }

        /// Farthest geologic moonlet's orbital radius (km): the camera rides the system outward as
        /// it evolves. 0 outside geologic time: the SPH impact frames itself (the scene zooms at the
        /// hand-off), so the retired CPU cloud's extent has no SPH replacement to report here.
        pub fn debris_extent_km(&self) -> f64 {
            if self.geologic {
                return self.geo_moonlets.iter().map(|m| m.a).fold(0.0, f64::max) / 1000.0;
            }
            // The retired CPU debris cloud reported its farthest bound fragment here to ride the camera
            // outward; the live GPU disk is framed by `focus`/zoom directly, so there is no CPU extent.
            0.0
        }

        pub fn set_time_scale(&mut self, scale: f32) {
            if self.arc.is_some() {
                return; // while the arc runs, the pacing law owns the observable clock
            }
            self.time_scale = (scale as f64).clamp(1.0, 2_000_000.0);
        }

        /// Live Earth–Moon separation in km (for the HUD). Should hover near 384,400 km.
        pub fn moon_distance_km(&self) -> f64 {
            // While the impact is live, the third body is NOT in the N-body array — the SPH field is the
            // matter. Reporting `bodies[2]` there printed the default Moon's untouched orbit: a frozen
            // "Earth–Theia 384,400 km · v 1.02 km/s" that never moved for the whole scene, describing a
            // body that was not on screen. Measure the real separation from the particles instead.
            if let Some([(_, ec, _), (_, tc, _)]) = self.sph_body_state() {
                return (tc - ec).length() / 1000.0;
            }
            (self.bodies[2].pos - self.bodies[1].pos).length() / 1000.0
        }

        /// **How much of a body is still a body**: the fraction of its mass lying within 1.2× its own
        /// radius of its centre of mass. 1.0 is an intact planet; it falls as the body is torn apart and
        /// climbs again as debris re-accretes into a clump.
        ///
        /// This replaces the "Pretty ⇄ Physics" slider. Whether matter is drawn as a resolved surface or
        /// as particles is not a preference — it is a question about the matter, and this is the measured
        /// answer. An intact body has a surface; a disrupted one does not, and pretending otherwise is
        /// what made the surface appear to be swallowed by the disk and peek out of it.
        ///
        /// FLAGGED: 1.2× is a coherence radius, not a physical boundary — the honest refinement is a
        /// self-bound clump test (`accretion`), which already exists and costs more per frame.

        /// This body's particles, if the engine has resolved any. SELECTION ONLY — every decision about
        /// what they mean belongs to `accretion`, so that a planet and a raindrop are judged by one rule.
        fn body_particles(&self, prov: u32) -> (Vec<glam::DVec3>, Vec<f64>) {
            self.sph_snapshot
                .iter()
                .filter(|p| p.prov == prov)
                .map(|p| {
                    (
                        glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64),
                        p.mass as f64,
                    )
                })
                .unzip()
        }

        /// Mass, centre of mass and mean velocity of each impact body, straight from the live particle
        /// field: (target, impactor). `None` when the impact is not running.
        ///
        /// **This exists because the HUD was reading `bodies[2]` throughout the impact** — an N-body slot
        /// holding the default Moon, which nothing updates while the SPH field IS the matter. Distance,
        /// speed, perigee and day length were all quoting a body that was not in the scene: a frozen
        /// "384,400 km · 1.02 km/s" for the entire run. Anything the HUD says about the impact must be
        /// measured from the particles.
        fn sph_body_state(&self) -> Option<[(f64, glam::DVec3, glam::DVec3); 2]> {
            if !self.sph_active || self.sph_snapshot.is_empty() {
                return None;
            }
            let mut acc = [(0.0f64, glam::DVec3::ZERO, glam::DVec3::ZERO); 2];
            for p in &self.sph_snapshot {
                let i = (p.prov != 0) as usize;
                let m = p.mass as f64;
                acc[i].0 += m;
                acc[i].1 += glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) * m;
                acc[i].2 += glam::DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64) * m;
            }
            (acc[0].0 > 0.0 && acc[1].0 > 0.0)
                .then(|| [0, 1].map(|i| (acc[i].0, acc[i].1 / acc[i].0, acc[i].2 / acc[i].0)))
        }

        /// The TARGET's spin period (s) measured from the particle field: its own angular momentum about
        /// its own centre, over its measured moment of inertia. This is the honest "Earth day" during and
        /// after the impact — the quantity the scene exists to let EMERGE, rather than the modern 23.9 h
        /// the HUD was printing from an untouched N-body slot.
        /// The index of the body the scene declared as its PLANET, found by ROLE — not the hardcoded
        /// `bodies[1]=Earth` (docs/58 brick 3). The engine reads the roles the scene declared; a scene that
        /// ordered its bodies differently, or has several planets, is no longer wrong. Falls back to 1 for
        /// the default/birth setup whose `body_meta` is not yet populated (Sun at 0, the planet at 1).
        fn planet_idx(&self) -> usize {
            self.body_meta
                .iter()
                .position(|m| matches!(m.role, BodyRole::Planet))
                .unwrap_or(1)
        }

        /// The planet's moment of inertia (kg·m²), EMERGENT from its matter (docs/58) — the actual layered
        /// mass distribution, not the uniform-sphere ⅖mr² with a hardcoded Earth radius. Falls back to the
        /// uniform form only where a scene has no per-body matter yet (the default/birth setup, whose spin
        /// is zero), so a declared day length is read back with the SAME inertia it was set with.
        fn spin_inertia(&self) -> f64 {
            let p = self.planet_idx();
            self.body_meta
                .get(p)
                .and_then(|m| m.matter.as_ref())
                .map(|b| b.moment_of_inertia())
                .unwrap_or_else(|| {
                    crate::tides::moment_of_inertia(self.bodies[p].mass, earth_radius_m())
                })
        }

        fn sph_target_spin_period_s(&self) -> Option<f64> {
            let [(m_t, c, v_c), _] = self.sph_body_state()?;
            let _ = m_t;
            let (mut l, mut inertia) = (glam::DVec3::ZERO, 0.0f64);
            for p in self.sph_snapshot.iter().filter(|p| p.prov == 0) {
                let m = p.mass as f64;
                let r = glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) - c;
                let v = glam::DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64) - v_c;
                l += r.cross(v) * m;
            }
            let axis = l.try_normalize()?;
            // Moment of inertia about the MEASURED spin axis, not an assumed sphere.
            for p in self.sph_snapshot.iter().filter(|p| p.prov == 0) {
                let r = glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) - c;
                inertia += p.mass as f64 * (r - axis * r.dot(axis)).length_squared();
            }
            let omega = l.length() / inertia.max(1e-9);
            (omega > 1e-12).then(|| std::f64::consts::TAU / omega)
        }

        /// Start the GPU deformable-Earth giant impact (docs/33 stage 4c.4): build + relax two differentiated
        /// EOS bodies on the CPU, place them on the oblique giant-impact geometry, and hand the per-frame
        /// dynamics to the GPU SPH stepper (the verified `sph_step.wgsl` kernels — same physics as the offline
        /// `tools/impact-run`). The scene then renders the live particle field instead of the rigid-Earth
        /// debris model. Call from JS on the `OrbitDemo` handle, like `drop_moon()`.
        /// Declare the giant impact's initial conditions from a world file (`docs/51`). Call BEFORE
        /// `start_gpu_impact`. Without it the engine uses `ImpactDef::default()`, which reproduces the
        /// constants this replaced exactly — so an un-migrated caller is unchanged.
        pub fn load_impact_world(&mut self, world_json: &str) -> Result<(), JsValue> {
            let w = crate::terra::world_def::World::parse(world_json)
                .map_err(|e| JsValue::from_str(&e))?;
            self.impact_def = w.impact.unwrap_or_default();
            Ok(())
        }

        /// docs/59 - arm the declared site: load a `"ground"` world (the SAME file the ground
        /// scene runs) and derive the trigger's spec from it and the one shared body. Until this
        /// is called no site exists and the trigger never fires.
        ///
        /// A DECLARED site then PRE-RESOLVES right here, at load, before any event exists
        /// (docs/59 "The hand-down, made concrete", decision 1): no code hands off state
        /// mid-shock, so refinement happens ahead of where the shock will arrive, and a site
        /// that wants to witness an event exists before it. The descent trigger stays armed as
        /// the general path - and the only path when this pre-resolve refuses (a mid-event
        /// load refuses with the measured speeds, exactly like a mid-event descent).
        pub fn load_site_world(&mut self, world_json: &str) -> Result<(), JsValue> {
            let w = crate::terra::world_def::World::parse(world_json)
                .map_err(|e| JsValue::from_str(&e))?;
            let spec =
                crate::site::SiteSpec::from_ground_world(&w).map_err(|e| JsValue::from_str(&e))?;
            self.site_trigger = crate::site::SiteTrigger::new();
            self.site = None;
            self.site_dyn = None;
            self.site_buf = None;
            self.site_refused = false;
            self.site_window.reset();
            self.site_sampled_gen = self.sph_snapshot_gen;
            self.pi_prediction = None;
            self.pi_line.clear();
            // The hand-down at load: the definition answers when no live field exists; a live
            // field is sampled through the one law (quiescent samples, mid-event refuses).
            let live = self.sph_active
                && matches!(self.sph_phase, SphPhase::Dynamics)
                && !self.sph_snapshot.is_empty();
            let hand = if live {
                let site_dir = crate::geo::dir_from_lat_lon(spec.lat_deg, spec.lon_deg);
                let r_p = spec.body_radius_m;
                match crate::site::sample_hand_down(&self.sph_snapshot, site_dir * r_p, spec.g_ms2)
                {
                    Ok(h) => Some(h),
                    Err(r) => {
                        self.site_status = format!(
                            "site armed at ({:.0}, {:.0}); pre-resolve refused: {r}",
                            spec.lat_deg, spec.lon_deg
                        );
                        self.site_spec = Some(spec);
                        return Ok(());
                    }
                }
            } else {
                Some(crate::site::HandDown::Declared)
            };
            match crate::site::materialize_site(&spec, &hand.expect("set above"), &self.mats) {
                Ok(site) => {
                    self.site_trigger
                        .confirm(crate::site::SiteCrossing::Materialize);
                    self.site_gauge.reset();
                    self.site_status = format!(
                        "{} · pre-resolved at load, before any event",
                        site_audit_line(&site)
                    );
                    // The released site ENTERS DYNAMICS (docs/59); the release gate refusing
                    // keeps the patch static, standing and stated.
                    self.site_dyn = match crate::site::SiteDynamics::new(&site, &spec, &self.mats) {
                        Ok(d) => Some(d),
                        Err(r) => {
                            self.site_status
                                .push_str(&format!(" · dynamics gated: {r}"));
                            None
                        }
                    };
                    self.site = Some(site);
                }
                Err(r) => {
                    self.site_status = format!(
                        "site armed at ({:.0}, {:.0}); pre-resolve refused: {r}",
                        spec.lat_deg, spec.lon_deg
                    );
                }
            }
            self.site_spec = Some(spec);
            Ok(())
        }

        /// The site's honest HUD line: the standing state (armed / materialized with its audit /
        /// refused with the reason / folded with the fold audit) plus the live camera-vs-threshold
        /// numbers the trigger is watching. Empty when no site is armed.
        pub fn site_status(&self) -> String {
            if self.site_spec.is_none() {
                return String::new();
            }
            let (d, at) = (self.site_dist_m, self.site_resolve_at_m);
            let mut s = format!(
                "{} · camera {:.0} km / threshold {:.0} km",
                self.site_status,
                d / 1.0e3,
                at / 1.0e3
            );
            // The event window's book and the pi-gate readout ride the same honest line.
            if self.site_window.is_open() {
                s.push_str(" · ");
                s.push_str(&self.site_window.hud_line());
            }
            // The dynamics readout: the ball's verdict word, the classify fate mix, the
            // boundary's delivered energy and the site clock.
            if let Some(d) = &self.site_dyn {
                s.push_str(" · ");
                s.push_str(&d.hud_line(&self.mats));
            }
            if !self.pi_line.is_empty() {
                s.push_str(" · ");
                s.push_str(&self.pi_line);
            }
            s
        }

        // ----------------------------------------------------------------------------------
        // The out-and-back demo arc (crate::arc): one continuous camera path from the manual
        // celestial rig down to standing over the declared site and back, with sim-time
        // compression tied to camera distance. A CAMERA/TIME driver only, it moves the eye
        // and the observable clock, never any matter; the site trigger fires along the way
        // exactly as it does under a manual camera, in both directions.
        // ----------------------------------------------------------------------------------

        /// Whether this world declares the arc (its pacing lives in the world file) AND arms a
        /// site for it to open on. No declaration, no control.
        pub fn arc_available(&self) -> bool {
            self.arc_octave_s.is_some() && self.site_spec.is_some()
        }

        pub fn arc_active(&self) -> bool {
            self.arc.is_some()
        }

        /// The arc control's label, the ENGINE names the phase, so the button cannot disagree
        /// with what the camera is actually doing.
        pub fn arc_label(&self) -> String {
            match self.arc.as_ref().map(|a| a.phase) {
                None if self.arc_available() => "▶ glide to the ball".to_string(),
                None => String::new(),
                Some(ArcPhase::GlideDown) => "descending · sim time easing to real".to_string(),
                Some(ArcPhase::HoldLow) => "▶ pull out (sim time will compress)".to_string(),
                Some(ArcPhase::GlideUp) => "pulling out · sim time compressing".to_string(),
                Some(ArcPhase::HoldHigh) => "▶ descend to the site".to_string(),
            }
        }

        /// The arc camera's current distance to the site (m); 0 while inactive. For the HUD and
        /// the verification rig.
        pub fn arc_distance_m(&self) -> f64 {
            self.arc.as_ref().map_or(0.0, |a| a.d_m)
        }

        /// The manual rig's pose, `[yaw, pitch, zoom]`, the host resyncs its camera state from
        /// this after `arc_stop`, so releasing the arc does not fight a stale local copy.
        pub fn camera_state(&self) -> Vec<f32> {
            vec![self.camera.yaw, self.camera.pitch, self.camera.zoom]
        }

        /// One press advances the choreography: from idle, take over the camera and glide down
        /// to the site (opening framing); from the low hold, pull out; from the high hold,
        /// descend home. Presses during a glide do nothing, the glide finishes first.
        pub fn arc_press(&mut self) {
            if self.arc.is_none() {
                let Some(octave_s) = self.arc_octave_s else {
                    return;
                };
                let Some(spec) = self.site_spec.as_ref() else {
                    return;
                };
                let Some(q) = spec.finest_child_extent_m(&self.mats) else {
                    return;
                };
                let theta = crate::resolution::ResolutionController::default().angular_resolution;
                let threshold =
                    crate::site::view_resolution_distance(spec.declared_coarse_extent_m(), theta);
                let span = crate::arc::ArcSpan::derive(
                    q,
                    theta,
                    self.camera.base_distance as f64 / display_scale(),
                    threshold,
                    self.arc_declared_scale,
                    octave_s,
                );
                // Take over exactly where the manual camera stands: the leg's start direction
                // and distance ARE the current eye, and the manual aim fades into the arc's
                // over the first octave, a glide, never a cut.
                let earth_c = self.bodies[self.planet_idx()].pos;
                let r_anchor = self.site_anchor_radius_m();
                let eye_w = self.manual_eye_world();
                let v0 = (eye_w - earth_c).normalize_or(glam::DVec3::X);
                let d0 = ((eye_w - earth_c).length() - r_anchor).max(span.d_floor_m);
                let focus_w = self.bodies.get(self.focus).map_or(earth_c, |b| b.pos);
                let aim_from_rel = focus_w + self.camera.pan.as_dvec3() / display_scale() - earth_c;
                self.arc_saved_scale = self.time_scale;
                self.arc = Some(ArcDrive {
                    span,
                    phase: ArcPhase::GlideDown,
                    d_m: d0,
                    leg: crate::arc::Leg {
                        from_dir: v0,
                        d_start_m: d0,
                    },
                    aim_from_rel,
                    octaves: 0.0,
                });
                return;
            }
            // A hold advances to the next leg. The new leg sets out from the CURRENT pose
            // direction, so the pose is continuous across the press by construction.
            let earth_c = self.bodies[self.planet_idx()].pos;
            let Some((eye_w, _, _, _)) = self.arc_pose_world(earth_c) else {
                return;
            };
            let v_now = (eye_w - earth_c).normalize_or(glam::DVec3::X);
            let Some(a) = self.arc.as_mut() else { return };
            match a.phase {
                ArcPhase::HoldLow => {
                    a.leg = crate::arc::Leg {
                        from_dir: v_now,
                        d_start_m: a.d_m,
                    };
                    a.phase = ArcPhase::GlideUp;
                }
                ArcPhase::HoldHigh => {
                    a.leg = crate::arc::Leg {
                        from_dir: v_now,
                        d_start_m: a.d_m,
                    };
                    a.phase = ArcPhase::GlideDown;
                }
                ArcPhase::GlideDown | ArcPhase::GlideUp => {}
            }
        }

        /// Release the camera to the manual rig at the nearest pose it can represent: same eye
        /// direction, distance clamped into its envelope. The manual rig has no surface regime
        /// (docs/59's open descent-camera remainder), so releasing near the floor steps out to
        /// its zoom floor, an explicit exit from the choreography, stated, not a cut inside it.
        /// The observable clock returns to the rate the arc found the scene at.
        pub fn arc_stop(&mut self) {
            if self.arc.is_none() {
                return;
            }
            let p = self.planet_idx();
            let earth_c = self.bodies[p].pos;
            if let Some((eye_w, _t, _u, _d)) = self.arc_pose_world(earth_c) {
                let dir = (eye_w - earth_c).normalize_or(glam::DVec3::X).as_vec3();
                self.camera.yaw = dir.x.atan2(dir.z);
                self.camera.pitch = dir.y.clamp(-1.0, 1.0).asin().clamp(-1.5, 1.5);
                let dist_disp = ((eye_w - earth_c).length() * display_scale()) as f32;
                self.camera.zoom = (dist_disp / self.camera.base_distance).clamp(0.05, 6.0);
                self.camera.pan = Vec3::ZERO;
                self.focus = p;
            }
            self.arc = None;
            self.time_scale = self.arc_saved_scale;
        }

        /// The manual rig's eye in WORLD metres, the one construction `view_proj`, the site
        /// trigger and the arc's takeover all share, so they cannot disagree about where the
        /// camera stands.
        fn manual_eye_world(&self) -> glam::DVec3 {
            let focus_w = self
                .bodies
                .get(self.focus)
                .map_or(self.bodies[self.planet_idx()].pos, |b| b.pos);
            let eye_disp = self.camera.eye_dir().as_dvec3()
                * (self.camera.base_distance * self.camera.zoom) as f64
                + self.camera.pan.as_dvec3();
            focus_w + eye_disp / display_scale()
        }

        /// The site's local elevation on the DRAWN planet (m): the raster elevation at the
        /// declared lat/lon under the render's own exaggeration; 0 before the rasters arrive.
        fn site_elev_m(&self) -> f64 {
            let Some(spec) = self.site_spec.as_ref() else {
                return 0.0;
            };
            self.earth_surface
                .as_ref()
                .and_then(|s| {
                    s.elevation.as_ref().map(|r| {
                        r.elevation_m_at(
                            spec.lat_deg,
                            spec.lon_deg,
                            s.elev_range[0],
                            s.elev_range[1],
                        )
                        .max(0.0)
                            * s.relief_exag
                    })
                })
                .unwrap_or(0.0)
        }

        /// The site's radial anchor on the drawn planet: body radius plus the local drawn
        /// elevation, the same anchor the site's particles render on, so the arc's floor
        /// hovers over the site the viewer actually sees.
        fn site_anchor_radius_m(&self) -> f64 {
            let p = self.planet_idx();
            self.body_meta
                .get(p)
                .map_or(earth_radius_m(), |m| m.radius_m)
                + self.site_elev_m()
        }

        /// The arc's camera pose in WORLD metres about the given planet centre (live for the
        /// trigger, render-lagged for the frame, the caller picks the frame it composes in).
        /// STATELESS: a pure function of the arc scalars and the crust's current orientation
        /// through `crate::arc`, which is what makes the path cut-free in both directions.
        /// Returns `(eye, look target, view up, distance-to-site)`.
        fn arc_pose_world(
            &self,
            earth_c: glam::DVec3,
        ) -> Option<(glam::DVec3, glam::DVec3, glam::DVec3, f64)> {
            let a = self.arc.as_ref()?;
            let spec = self.site_spec.as_ref()?;
            let spin_axis = self.spin_l.try_normalize().unwrap_or(glam::DVec3::Z);
            let spin_rot = glam::DQuat::from_axis_angle(
                spin_axis,
                self.spin_angle % (2.0 * std::f64::consts::PI),
            );
            let (up_b, north_b, _east) = crate::geo::tangent_frame(spec.lat_deg, spec.lon_deg);
            let (u_crust, north) = (spin_rot * up_b, spin_rot * north_b);
            let r_anchor = self.site_anchor_radius_m();
            let spin_rate = self.spin_l.length() / self.spin_inertia();
            let v = match a.phase {
                ArcPhase::GlideDown | ArcPhase::HoldLow => crate::arc::descend_dir(
                    &a.span, &a.leg, a.d_m, u_crust, spin_axis, spin_rate, r_anchor,
                ),
                ArcPhase::GlideUp | ArcPhase::HoldHigh => {
                    crate::arc::ascend_dir(&a.span, &a.leg, a.d_m, u_crust)
                }
            };
            let eye_w = earth_c + crate::arc::eye(v, r_anchor, a.d_m);
            let mut target_rel = crate::arc::look_target(&a.span, a.d_m, u_crust * r_anchor);
            if a.octaves < 1.0 {
                // The takeover blend (one octave): manual aim → the arc's own target.
                target_rel = a.aim_from_rel.lerp(target_rel, a.octaves.clamp(0.0, 1.0));
            }
            let up = crate::arc::view_up(&a.span, a.d_m, north, glam::DVec3::Y);
            Some((eye_w, earth_c + target_rel, up, a.d_m))
        }

        /// Advance the arc by real time: the glide moves the one scalar, the pacing law sets
        /// the observable clock (every frame while active, the arc owns time until released),
        /// and a finished glide parks in the next hold. Camera/time state only.
        fn arc_tick(&mut self, real_dt: f64) {
            let Some(a) = self.arc.as_mut() else { return };
            match a.phase {
                ArcPhase::GlideDown => {
                    a.d_m = a.span.glide(a.d_m, real_dt, true);
                    a.octaves += real_dt / a.span.octave_s;
                    if a.d_m <= a.span.d_floor_m {
                        a.phase = ArcPhase::HoldLow;
                    }
                }
                ArcPhase::GlideUp => {
                    a.d_m = a.span.glide(a.d_m, real_dt, false);
                    a.octaves += real_dt / a.span.octave_s;
                    if a.d_m >= a.span.d_top_m {
                        a.phase = ArcPhase::HoldHigh;
                    }
                }
                ArcPhase::HoldLow | ArcPhase::HoldHigh => {}
            }
            self.time_scale = a.span.time_scale(a.d_m);
        }

        /// docs/59 - the per-frame site check: the camera's mirror of the moon-drop's
        /// resolution-distance crossing (`live_resolution_crossing`), so the engine has ONE
        /// materialization pattern. Derives the view-necessity threshold from the coarse quantum
        /// (measured from the live field when one exists, the declared celestial statement
        /// otherwise) and the docs/49 angular budget; executes whatever crossing the trigger
        /// demands; keeps the refusals and the audit on the HUD.
        fn update_site(&mut self, dt: f64) {
            let Some(spec) = self.site_spec.as_ref() else {
                return;
            };
            let p = self.planet_idx();
            let r_p = self
                .body_meta
                .get(p)
                .map_or(earth_radius_m(), |m| m.radius_m);
            // The site rides the rotating crust, exactly like the shell grains and the crater.
            let spin_axis = self.spin_l.try_normalize().unwrap_or(glam::DVec3::Z);
            let spin_rot = glam::DQuat::from_axis_angle(
                spin_axis,
                self.spin_angle % (2.0 * std::f64::consts::PI),
            );
            let site_dir = spin_rot * crate::geo::dir_from_lat_lon(spec.lat_deg, spec.lon_deg);
            let site_w = self.bodies[p].pos + site_dir * r_p;
            // The camera eye in world metres: the arc's pose while the arc drives, the manual
            // rig's otherwise (ONE construction each, shared with the render path).
            let eye_w = self
                .arc_pose_world(self.bodies[p].pos)
                .map(|(e, _, _, _)| e)
                .unwrap_or_else(|| self.manual_eye_world());
            let dist = (eye_w - site_w).length();
            // The coarse quantum this site is currently represented at.
            let live = self.sph_active
                && matches!(self.sph_phase, SphPhase::Dynamics)
                && !self.sph_snapshot.is_empty();
            let extent = if live {
                crate::site::measured_coarse_extent_m(&self.sph_snapshot)
            } else {
                spec.declared_coarse_extent_m()
            };
            let theta = crate::resolution::ResolutionController::default().angular_resolution;
            let resolve_at = crate::site::view_resolution_distance(extent, theta);
            self.site_dist_m = dist;
            self.site_resolve_at_m = resolve_at;

            if let Some(site) = self.site.as_mut() {
                // docs/59 decision 2 - the mid-event boundary hand-down: while a live event
                // runs, the guard band re-samples the coarse field once per COARSE step (each
                // new readback), so the guards ARE the coarse field at the boundary and the
                // impact's energy arrives at the site as real boundary state, booked by the
                // event window. Ownership stays single: guards mirror state, never matter.
                // (The tangent frame co-rotates with the crust while the coarse field does
                // not spin; the omega-cross-r difference, under ~0.5 km/s, is sub-resolution
                // against the coarse quantum's own quiescent speed of several km/s.)
                if live && self.site_sampled_gen != self.sph_snapshot_gen {
                    self.site_sampled_gen = self.sph_snapshot_gen;
                    let (up, north, east) = crate::geo::tangent_frame(spec.lat_deg, spec.lon_deg);
                    let frame = crate::site::SiteFrame {
                        origin_rel_m: site_dir * r_p,
                        east: spin_rot * east,
                        up: spin_rot * up,
                        north: spin_rot * north,
                    };
                    // The window's previous book is the baseline this coarse step's ARRIVAL is
                    // measured against; the first book of an event is the baseline itself.
                    let prev = self
                        .site_window
                        .is_open()
                        .then(|| self.site_window.last_state());
                    let state = crate::site::resample_guards(site, &self.sph_snapshot, &frame);
                    self.site_window.book(state);
                    // The arrival drives the site's dynamics through the one door (docs/59):
                    // the guard band's step-to-step delta, delivered per aggregate by
                    // Aggregate::deposit_impact - the ground scene's own operator.
                    if let (Some(prev), Some(d)) = (prev, self.site_dyn.as_mut()) {
                        d.deliver_boundary(&prev, &state, &self.mats);
                    }
                    // The pi-scaling cross-check (docs/59): once contact froze the impact
                    // direction and the measured-state prediction, read the crater off the
                    // coarse field at the field's own quantum and gate it - or carry the
                    // stated refusal when the quantum cannot hold a verdict.
                    if let (Some(dir), Some((rim_pred, speed))) =
                        (self.gpu_impact_site, self.pi_prediction)
                    {
                        self.pi_line = match crate::refine::measure_crater_rim(
                            &self.sph_snapshot,
                            dir,
                            r_p,
                            extent,
                        ) {
                            Ok(m) => format!(
                                "pi gate ({}): rim {:.0} km measured at the {:.0} km quantum \
                                 vs {:.0} km predicted from the {:.1} km/s contact: {}",
                                crate::refine::HARD_ROCK.name,
                                m.rim_radius_m / 1.0e3,
                                extent / 1.0e3,
                                rim_pred / 1.0e3,
                                speed / 1.0e3,
                                crate::refine::pi_scaling_gate(m.rim_radius_m, rim_pred, r_p)
                            ),
                            Err(r) => {
                                format!("pi gate ({}): {r}", crate::refine::HARD_ROCK.name)
                            }
                        };
                    }
                }
                // The site STEPS: its parcels evolve under the existing laws each frame, on the
                // site's own real-second clock (a compute statement the HUD quotes; verdicts
                // are deposit-driven and land at the coarse step regardless).
                if let (Some(site), Some(d)) = (self.site.as_mut(), self.site_dyn.as_mut()) {
                    d.step(site, dt, &self.mats);
                }
                let site = self.site.as_ref().expect("checked above");
                // Feed the docs/61 gauge the site's own peak speed (the boundary now carries
                // the field's speeds mid-event, so a hot site honestly refuses to fold; the
                // gauge is the law, not a shortcut past it).
                self.site_gauge.observe(
                    crate::site::site_peak_speed(site),
                    spec.g_ms2 as f32,
                    dt as f32,
                );
                // The standing contamination check while a live celestial field exists: a coarse
                // particle penetrating the fine site invalidates it, and the screen says so.
                if live {
                    let fine_mass = site.particles[site.fine_start..]
                        .iter()
                        .map(|q| q.mass)
                        .fold(f32::INFINITY, f32::min);
                    let region = crate::refine::Region {
                        center: (site_dir * r_p).as_vec3(),
                        radius: site.extent_m as f32,
                    };
                    if let Err(r) =
                        crate::refine::contamination_check(&self.sph_snapshot, &region, fine_mass)
                    {
                        self.site_status = format!("SITE INVALID: {r}");
                        return;
                    }
                }
            }

            match self.site_trigger.observe(dist, resolve_at) {
                Some(crate::site::SiteCrossing::Materialize) => {
                    if self.site_refused {
                        return; // the standing refusal is on the HUD; re-arms on ascent
                    }
                    // The smallest honest hand-down: sample a quiescent live field, refuse a
                    // mid-event one (a cheap check, retried every frame so the demand stands),
                    // or take the definition when no field exists.
                    let hand = if live {
                        match crate::site::sample_hand_down(
                            &self.sph_snapshot,
                            site_dir * r_p,
                            spec.g_ms2,
                        ) {
                            Ok(h) => h,
                            Err(r) => {
                                self.site_status = format!("{r}");
                                return;
                            }
                        }
                    } else {
                        crate::site::HandDown::Declared
                    };
                    match crate::site::materialize_site(spec, &hand, &self.mats) {
                        Ok(site) => {
                            self.site_trigger
                                .confirm(crate::site::SiteCrossing::Materialize);
                            self.site_gauge.reset();
                            self.site_status = site_audit_line(&site);
                            // Released -> the site enters dynamics; gated -> static, stated.
                            self.site_dyn =
                                match crate::site::SiteDynamics::new(&site, spec, &self.mats) {
                                    Ok(d) => Some(d),
                                    Err(r) => {
                                        self.site_status
                                            .push_str(&format!(" · dynamics gated: {r}"));
                                        None
                                    }
                                };
                            self.site = Some(site);
                            self.site_buf = None; // sized on first draw
                        }
                        Err(r) => {
                            // A build-level refusal is deterministic at this descent: latch it
                            // (the demand stands, stated) instead of re-refusing every frame.
                            self.site_refused = true;
                            self.site_status = format!("{r}");
                        }
                    }
                }
                Some(crate::site::SiteCrossing::Deresolve) => {
                    let Some(site) = self.site.as_ref() else {
                        return;
                    };
                    match crate::site::fold_site(site, &self.site_gauge, spec.g_ms2 as f32) {
                        Ok(rep) => {
                            self.site_trigger
                                .confirm(crate::site::SiteCrossing::Deresolve);
                            self.site = None;
                            self.site_dyn = None;
                            self.site_buf = None;
                            self.site_status = format!(
                                "site folded to the summary: {} particles, {:.4e} kg returned \
                                 (drift {:+.1e} kg)",
                                rep.folded, rep.audit.mass, rep.mass_drift_kg
                            );
                        }
                        Err(r) => {
                            // Not settled: it honestly stays resolved; the demand stands.
                            self.site_status = format!("{r}");
                        }
                    }
                }
                None => {
                    // Ascended past the threshold un-materialized: the next descent retries.
                    if !self.site_trigger.is_resolved() {
                        self.site_refused = false;
                    }
                }
            }
        }

        /// Set up the GPU SPH relaxation for a collision of the given bodies (docs/58 #7 — the ONE engine).
        /// Each is particalized from its MATTER (real mass, per-material EOS from the catalogue) and relaxed
        /// PROMOTE a settled clump out of the particle set into a layered body (docs/58, docs/44).
        ///
        /// This is the tier above the shader's pairwise merge: merging coarsens redundant particles, and a
        /// blob that has fully coalesced and gone quiet stops being particles at all. Robin: *"a planet is a
        /// promoted particle with more properties/analysis"* — so the promoted record is docs/58's generic
        /// body, `{matter, pos, vel, ang_mom}`, and its matter is SAMPLED from the very particles that made
        /// it, never declared.
        ///
        /// Gated on the same quiescence the merge uses: promoting mid-shock would freeze matter that is
        /// still being excavated. Only clumps that `accretes()` (self-bound AND outside Roche) qualify, so
        /// the central remnant and any tidally-shredding debris are both excluded by construction.
        ///
        /// Returns `true` if the particle field was rewritten — the caller must not step that frame, since
        /// `upload` replaces the field from a read-back one frame old.
        fn promote_settled_bodies(&mut self) -> bool {
            use glam::DVec3;
            if self.sph_snapshot.len() < 8 || self.sph_sim_t <= SPH_SHOCK_WINDOW_S {
                return false;
            }
            if self.sph_promoted.len() >= crate::gpu_sph::MAX_EXT_BODIES {
                return false;
            }
            let snap = std::mem::take(&mut self.sph_snapshot);
            let Some(view) = crate::gpu_sph::DiskView::of(&snap) else {
                self.sph_snapshot = snap;
                return false;
            };
            // Two members is the minimum that can express a radial profile at all. It is NOT the real
            // gate: a COUNT gate is self-defeating here, because merging systematically reduces the member
            // count, so it becomes unsatisfiable exactly when coalescence succeeds (Robin caught this —
            // the same shape of mistake as writing the binding energy as an O(k²) sum that explodes when
            // clumps unite). The real gate is MASS, below.
            let clumps = view.moonlets(2);
            let names = crate::gpu_sph::eos_material_names(&self.sph_eos, &self.mats);
            // A clump is promoted when its own gravity has overcome its material strength — the physical
            // boundary between a rock and a BODY (`accretion::rounding_mass`). Below it the thing has no
            // hydrostatic interior to describe with layers and the particle tier is its right home.
            let strength_of = |c: &crate::accretion::Clump| -> f64 {
                let mut wsum = 0.0f64;
                let mut msum = 0.0f64;
                for &k in &c.members {
                    let p = &snap[view.idx[k]];
                    let m = p.mass as f64;
                    let sig = names
                        .get(p.mat as usize)
                        .and_then(|n| self.mats.iter().find(|mm| &mm.id == n))
                        .map(|mm| mm.fracture_strength as f64)
                        .unwrap_or(1.0e8);
                    wsum += m * sig;
                    msum += m;
                }
                if msum > 0.0 {
                    wsum / msum
                } else {
                    1.0e8
                }
            };
            let Some(c) = clumps
                .into_iter()
                .filter(|c| c.mass >= crate::accretion::rounding_mass(strength_of(c), c.rho))
                .max_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap())
            else {
                self.sph_snapshot = snap;
                return false;
            };

            // Sample the clump's own matter — the layering the sim produced, read rather than declared.
            let idx: Vec<usize> = c.members.iter().map(|&k| view.idx[k]).collect();
            let (mut pos, mut mass, mut rho, mut mat, mut temp) =
                (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for &i in &idx {
                let p = &snap[i];
                pos.push(DVec3::new(
                    p.pos[0] as f64,
                    p.pos[1] as f64,
                    p.pos[2] as f64,
                ));
                mass.push(p.mass as f64);
                rho.push(p.rho.max(1.0) as f64);
                mat.push(p.mat as usize);
                // T from the particle's OWN specific internal energy and its material's heat capacity.
                let sh = names
                    .get(p.mat as usize)
                    .and_then(|n| self.mats.iter().find(|m| &m.id == n))
                    .and_then(|m| m.specific_heat())
                    .unwrap_or(840.0);
                temp.push((p.u as f64 / sh.max(1.0)).max(0.0));
            }
            let layers =
                crate::accretion::sample_layers(&pos, &mass, &rho, &mat, &names, &temp, 12);
            if layers.is_empty() {
                self.sph_snapshot = snap;
                return false;
            }
            let matter = crate::planet::LayeredBody::from_layers(layers);
            let body = crate::accretion::Body {
                pos: c.com_pos,
                vel: c.com_vel,
                mass: c.mass,
                rho: c.rho,
                radius: c.radius,
                ang_mom: c.ang_mom,
                thermal_j: c.thermal_ke,
            };
            log::info!(
                "promote: {} particles -> a body of {:.3e} kg in {} layer(s) [{}]",
                idx.len(),
                body.mass,
                matter.layers.len(),
                matter
                    .layers
                    .iter()
                    .map(|l| l.material.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            );
            self.sph_promoted.push(PromotedBody { body, matter });

            // The particles it was made of leave the field; the body carries their mass, momentum and spin.
            let consumed: std::collections::HashSet<usize> = idx.into_iter().collect();
            let kept: Vec<crate::gpu_sph::SphParticle> = snap
                .iter()
                .enumerate()
                .filter(|(i, _)| !consumed.contains(i))
                .map(|(_, p)| *p)
                .collect();
            if kept.is_empty() {
                self.sph_snapshot = snap;
                self.sph_promoted.pop();
                return false;
            }
            let ext: Vec<(DVec3, f64)> = self
                .sph_promoted
                .iter()
                .map(|p| (p.body.pos, p.body.mass))
                .collect();
            let soft = self.sph_soft as f32;
            let dt = self.sph_dt;
            let budget = self.sph_merge_budget;
            if let Some(sph) = self.gpu_sph.as_mut() {
                sph.upload(&self.queue, &kept, &self.sph_eos, soft);
                sph.set_dt(&self.queue, dt, 1.0);
                sph.set_av(&self.queue, 1.0, 2.0);
                sph.set_merge_budget(&self.queue, budget);
                // THIS is what makes `ext_mass` load-bearing: without it the promoted body's mass would
                // simply stop acting on everything left behind, and a change of representation would have
                // changed what is true (Law IV).
                sph.set_external_masses(&self.queue, &ext);
                sph.begin_readback(&self.device, &self.queue);
            }
            true
        }

        /// Advance the PROMOTED bodies. The shader no longer integrates them — they left the particle set —
        /// but they are still matter in the same field, so they must feel the particles and each other, or a
        /// promoted moon would hang frozen while the disk moved around it.
        ///
        /// FLAGGED (Law V): kick-then-drift rather than the KDK the particles use, since the force is
        /// evaluated once per frame here. It is symplectic, so it does not secularly gain energy, and it
        /// converges to the particles' KDK as dt falls; the resolved form is simply evaluating the force
        /// twice per step, as `encode_kdk` does.
        fn step_promoted_bodies(&mut self, dt: f64) {
            use glam::DVec3;
            if self.sph_promoted.is_empty() || dt <= 0.0 {
                return;
            }
            let g = crate::orbit::G;
            let s = self.sph_soft.max(1.0);
            let s2 = s * s;
            let n = self.sph_promoted.len();
            let mut acc = vec![DVec3::ZERO; n];
            for i in 0..n {
                let b = self.sph_promoted[i].body;
                let mut a = DVec3::ZERO;
                for pt in &self.sph_snapshot {
                    let d =
                        DVec3::new(pt.pos[0] as f64, pt.pos[1] as f64, pt.pos[2] as f64) - b.pos;
                    let r2 = d.length_squared() + s2;
                    a += d * (g * pt.mass as f64 / (r2 * r2.sqrt()));
                }
                for (j, o) in self.sph_promoted.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let d = o.body.pos - b.pos;
                    let r2 = d.length_squared() + s2;
                    a += d * (g * o.body.mass / (r2 * r2.sqrt()));
                }
                acc[i] = a;
            }
            for (p, a) in self.sph_promoted.iter_mut().zip(acc) {
                p.body.vel += a * dt; // kick
                p.body.pos += p.body.vel * dt; // drift
            }
            // Keep the shader's copy of where they are in step with where they actually are.
            let ext: Vec<(DVec3, f64)> = self
                .sph_promoted
                .iter()
                .map(|p| (p.body.pos, p.body.mass))
                .collect();
            if let Some(sph) = self.gpu_sph.as_mut() {
                sph.set_external_masses(&self.queue, &ext);
            }
        }

        /// far apart; the material EOS table is kept (`sph_eos`) for the assemble + dynamics uploads. `live`
        /// selects the assembly geometry when the relax completes — the LIVE N-body trajectory (a moon-drop)
        /// or the declared canonical giant impact (birth). Shared by `start_gpu_impact` and `drop_moon`.
        fn begin_sph_relax(
            &mut self,
            bodies: &[(&crate::planet::LayeredBody, usize)],
            separation: f64,
        ) {
            let (particles, eos, softening, relax_dt) =
                crate::gpu_sph::build_far_apart_n(bodies, separation);
            self.sph_eos = eos;
            self.sph_soft = softening as f64;
            let cap = particles.len() as u32;
            let mut sph = crate::gpu_sph::GpuSph::new(&self.device, cap);
            sph.upload(&self.queue, &particles, &self.sph_eos, softening);
            sph.set_dt(&self.queue, relax_dt, 0.94); // damped relaxation toward hydrostatic equilibrium
            sph.set_av(&self.queue, 0.0, 0.0); // no artificial viscosity during relax (matches the CPU relax)
            self.gpu_sph = Some(sph);
            self.sph_dt = relax_dt;
            self.sph_active = true;
            self.sph_snapshot.clear();
            self.sph_phase = SphPhase::Relaxing(0);
            self.gpu_impact_site = None; // no crater until contact (docs/42 Phase 2)
            self.gpu_crater_frac = 0.0;
            self.gpu_crater_depth_frac = 0.0;
            self.gpu_crater_r_frac = 0.0;
            self.site_window.reset(); // a new event opens its own boundary window (docs/59)
            self.site_sampled_gen = self.sph_snapshot_gen;
            self.pi_prediction = None;
            self.pi_line.clear();
            self.sph_substeps = 6; // start conservative; the frame-budget controller adapts up (docs/42)
        }

        /// Begin a resolution-on-demand CAP relaxation (docs/39): the target's `cap` + the impactor(s), relaxed
        /// SEATED on the bulk (Gauss gravity + non-injecting floor at `r_core`). The cap settles to hydrostatic
        /// on the floor BEFORE the impact — an un-relaxed cap over-ejects (the 3a lesson). Mirrors
        /// `begin_sph_relax` but with the bulk configured and the pre-built cap-relax field.
        fn begin_cap_relax(
            &mut self,
            cap: &crate::hydrostatic::HydroBody,
            impactors: &[(&crate::planet::LayeredBody, usize)],
            separation: f64,
            r_core: f64,
            bulk_mass: f64,
        ) {
            let (particles, eos, softening, relax_dt) =
                crate::gpu_sph::build_cap_relax(cap, impactors, separation);
            self.sph_eos = eos;
            self.sph_soft = softening as f64;
            let n = particles.len() as u32;
            let mut sph = crate::gpu_sph::GpuSph::new(&self.device, n);
            sph.upload(&self.queue, &particles, &self.sph_eos, softening);
            sph.set_dt(&self.queue, relax_dt, 0.94);
            sph.set_av(&self.queue, 0.0, 0.0);
            // The un-resolved planet bulk the cap relaxes on (docs/39 39b — a bare floor beats a boundary shell).
            sph.set_bulk(
                &self.queue,
                glam::DVec3::ZERO,
                r_core,
                glam::DVec3::ZERO,
                bulk_mass,
            );
            self.gpu_sph = Some(sph);
            self.sph_dt = relax_dt;
            self.sph_active = true;
            self.sph_snapshot.clear();
            self.sph_phase = SphPhase::Relaxing(0);
            self.gpu_impact_site = None;
            self.gpu_crater_frac = 0.0;
            self.gpu_crater_depth_frac = 0.0;
            self.gpu_crater_r_frac = 0.0;
            self.site_window.reset(); // a new event opens its own boundary window (docs/59)
            self.site_sampled_gen = self.sph_snapshot_gen;
            self.pi_prediction = None;
            self.pi_line.clear();
            self.sph_substeps = 6;
        }

        pub fn start_gpu_impact(&mut self) {
            // Particalize the two declared bodies (proto-Earth + Theia) and relax them; equal particle mass
            // across both (the target sets it), so neither body's resolution biases the shared dynamics.
            let t_def = self.impact_def.target.definition();
            let i_def = self.impact_def.impactor.definition();
            let n_target = 2400usize;
            let m_i = t_def.total_mass() / n_target as f64;
            let n_impactor = (i_def.total_mass() / m_i).round().max(50.0) as usize;
            // The proto-Earth's DECLARED spin becomes the target's live spin (spin_l = ω·I about +z), so the
            // ONE assembly path — which reads spin from self.spin_l — flings the rotationally-sustained disk
            // (docs/41 spin IOU) with no birth-only branch.
            self.spin_l = glam::DVec3::new(
                0.0,
                0.0,
                self.impact_def.target_spin_rad_s * self.spin_inertia(),
            );
            self.begin_sph_relax(
                &[(&t_def, n_target), (&i_def, n_impactor)],
                self.impact_def.relax_separation,
            );
            self.sph_prov_to_body = vec![1, 2]; // planet at bodies[1], impactor at bodies[2] (placed just below)
            self.sph_cap = None; // birth is a WHOLE-BODY giant impact — resolve both bodies, no bulk
                                 // **Put the two bodies on their approach, AS BODIES.** The scene declares what they are and
                                 // how they meet (which bodies, the approach speed as a multiple of mutual escape, the impact
                                 // parameter); the engine turns that into a trajectory and integrates it. No particle exists
                                 // yet and none should: nothing has happened to either body.
                                 //
                                 // The approach starts well outside `accretion::resolution_distance` so the bodies are visibly
                                 // whole and closing before matter is resolved — that distance is where tides begin to matter,
                                 // and it is the engine's call, not the scene's.
            {
                let t_def = self.impact_def.target.definition();
                let i_def = self.impact_def.impactor.definition();
                let (m_t, r_t) = (t_def.total_mass(), t_def.radius());
                let (m_i, r_i) = (i_def.total_mass(), i_def.radius());
                let contact = r_t + r_i;
                let resolve_at = crate::accretion::resolution_distance(
                    m_t,
                    r_t,
                    m_i,
                    crate::accretion::RESOLVE_TIDAL_FRACTION,
                );
                let d0 = 3.0 * resolve_at; // room to watch two solid worlds converge
                let v_esc = (2.0 * crate::orbit::G * (m_t + m_i) / contact).sqrt();
                // Speed at d0 for a trajectory whose speed at contact is the declared multiple of escape:
                // energy conservation, v² = v_c² − 2GM(1/contact − 1/d0).
                let v_c = self.impact_def.v_esc_multiple * v_esc;
                let mu = crate::orbit::G * (m_t + m_i);
                let v0 = (v_c * v_c - 2.0 * mu * (1.0 / contact - 1.0 / d0))
                    .max(0.0)
                    .sqrt();
                let b = self.impact_def.impact_parameter * r_t;
                let earth = self.bodies[1];
                self.bodies.truncate(2);
                self.bodies.push(crate::orbit::Body {
                    pos: earth.pos + glam::DVec3::new(d0, b, 0.0),
                    vel: earth.vel + glam::DVec3::new(-v0, 0.0, 0.0),
                    mass: m_i,
                });
                self.impactor_radius = r_i;
                self.impactor_mass = m_i;
                self.acc = crate::orbit::accelerations(&self.bodies);
            }
            self.sph_phase = SphPhase::Relaxing(0);
            self.gpu_impact_site = None; // no crater until Theia makes contact (docs/42 Phase 2)
            self.gpu_crater_frac = 0.0;
            self.gpu_crater_depth_frac = 0.0;
            self.gpu_crater_r_frac = 0.0;
            self.site_window.reset(); // a new event opens its own boundary window (docs/59)
            self.site_sampled_gen = self.sph_snapshot_gen;
            self.pi_prediction = None;
            self.pi_line.clear();
            self.sph_substeps = 6; // start conservative; the frame-budget controller adapts up (docs/42)
            self.focus = 1; // centre on Earth (the particle system sits at the display origin)
            self.camera.zoom = 0.4; // frame the impact (the Earth–Moon default zoom shows it as a speck)
        }

        /// Disk-provenance stats of the live GPU SPH impact (docs/33 stage 5), computed from the latest
        /// read-back: orbiting-disk mass (M☾), its Earth %, remnant radius, escaped mass, and the largest
        /// self-bound clump (Moon candidate). `"null"` before the first read-back. JS reads this for the HUD.
        pub fn gpu_disk_stats_json(&self) -> String {
            if !self.sph_active {
                return String::from("null");
            }
            crate::gpu_sph::disk_stats_json(&self.sph_snapshot)
        }

        /// docs/42 escape-check: the largest proto-Moon clump's orbit about the remnant — distance (km), speed
        /// (km/s), whether it is BOUND (specific orbital energy < 0), and semi-major axis (km). Tracks whether
        /// the accreted Moon is receding / unbinding. `"null"` if there's no clump yet.
        pub fn gpu_moon_track_json(&self) -> String {
            if !self.sph_active {
                return String::from("null");
            }
            match crate::gpu_sph::largest_moonlet_orbit(&self.sph_snapshot) {
                Some((r, v, e, a, mass, ecc, theta)) => format!(
                    "{{\"dist_km\":{:.0},\"v_kms\":{:.3},\"bound\":{},\"a_km\":{},\"ecc\":{:.3},\"theta_deg\":{:.0},\"mass_moon\":{:.3}}}",
                    r / 1e3, v / 1e3, e < 0.0,
                    if a.is_finite() { format!("{:.0}", a / 1e3) } else { "\"unbound\"".to_string() },
                    ecc, theta.to_degrees(), mass / moon_mass(),
                ),
                None => String::from("null"),
            }
        }

        /// Energy diagnostic of the live GPU impact (docs/35): kinetic / internal / gravitational-PE / total
        /// (J), from the latest read-back. A steadily rising total = the integrator is injecting energy (the
        /// remnant then puffs apart instead of orbiting). `"null"` before the first read-back.
        pub fn gpu_energy_json(&self) -> String {
            if !self.sph_active || self.sph_snapshot.is_empty() {
                return String::from("null");
            }
            let (ke, ie, pe) = crate::gpu_sph::total_energy(&self.sph_snapshot, self.sph_soft);
            format!(
                "{{\"ke\":{:.4e},\"ie\":{:.4e},\"pe\":{:.4e},\"tot\":{:.4e}}}",
                ke,
                ie,
                pe,
                ke + ie + pe
            )
        }

        /// Advance the PHYSICS by `real_dt` wall-clock seconds. Fixed sim-timestep substeps whose
        /// COUNT (not size) varies with the wall clock — so the physics rate is independent of the
        /// display frame rate (a 30 fps client and a 120 fps client simulate the same world), and the
        /// physics NEVER depends on rendering: the render only samples what this produced, RENDER_LAG_S
        /// later. Under overload the observable clock dilates (we drop backlog) rather than corrupting
        /// the physics with an oversized step — time slows before truth breaks.
        pub fn advance(&mut self, real_dt: f64) {
            let real_dt = real_dt.clamp(0.0, 0.25); // tab-sleep / hiccup guard
                                                    // The demo arc drives camera distance and the observable clock first (a pure
                                                    // camera/time driver), so the site trigger below sees the arc's eye this frame.
            self.arc_tick(real_dt);
            // docs/59 - the declared site's camera trigger runs every frame, whichever machine
            // owns the physics below (the SPH phases early-return out of this function).
            self.update_site(real_dt);
            // docs/42 — ADAPTIVE GPU-load control: keep each frame's encoded work inside a wall-clock budget so
            // the sim can never monopolize the GPU and freeze the tab / OS. `real_dt` is the previous frame's
            // total time; a slow frame shrinks the substep count (multiplicative, down to 1), headroom grows it
            // by one (additive, capped). The heavy direct-sum O(N²) step is exactly what this throttles.
            if self.sph_active {
                if real_dt > 0.060 {
                    self.sph_substeps = (self.sph_substeps * 3 / 4).max(1);
                } else if real_dt < 0.028 {
                    self.sph_substeps = (self.sph_substeps + 1).min(24);
                }
                // DE-RESOLUTION BUDGET (docs/08/44). The substep throttle above is the FIRST response to a
                // frame we cannot afford. Once it has bottomed out at 1 and the frame is STILL over budget,
                // the cost is the particle COUNT, not the step count — and that is the honest trigger for
                // coarsening. So necessity here is MEASURED (frame time), never a declared number: while the
                // sim is comfortable no merging happens at all, however redundant the particles look.
                // QUIESCENCE first (docs/44 §6: "demote on quiescence... not when motion stops"). Coarsening
                // during the shock is not a saving, it is a CORRUPTION: the excavation is actively separating
                // material, and merging fights it. Measured — with the gate on frame time alone, birth's disk
                // went 0.34/0.31/0.25 M☾ -> 0.00, because the throttle bottoms out exactly during the impact
                // and the merge then swept disk material back into the remnant. The shock window is the
                // boundary the scheduled-dt coarsening already uses, so one definition of "the event is over"
                // serves both.
                let quiescent = self.sph_sim_t > SPH_SHOCK_WINDOW_S;
                let live = self.gpu_sph.as_ref().map_or(0, |s| s.count());
                let budget = if quiescent && real_dt > 0.060 && self.sph_substeps <= 1 {
                    // Shed a tenth of the field per coarsening round: enough to converge over a few frames,
                    // gentle enough that one bad frame cannot collapse the sim.
                    (live * 9 / 10).max(64)
                } else if real_dt < 0.028 {
                    0 // comfortable: stop coarsening entirely
                } else {
                    self.sph_merge_budget // hold steady in the band between
                };
                if budget != self.sph_merge_budget {
                    self.sph_merge_budget = budget;
                    if let Some(sph) = self.gpu_sph.as_mut() {
                        sph.set_merge_budget(&self.queue, budget);
                    }
                }
            }
            // GPU SPH deformable-Earth impact owns the frame while active (docs/33 stage 4c.4): encode a batch
            // of KDK substeps on the GPU and skip the CPU orbital physics. Fixed dt (WebGPU forbids the
            // adaptive read-back); ~8 substeps/frame plays the ~10 h aftermath out over a few seconds.
            if self.sph_active {
                match self.sph_phase {
                    // RELAX (on the GPU): the two bodies sit far apart and settle under their own gravity via
                    // `cs_relax`. Fast enough to run many steps/frame; on completion, kick off the read-back.
                    SphPhase::Relaxing(steps) => {
                        // Relax steps/frame. This used to ride the DYNAMICS budget — `clamp(2·substeps, 2, 48)`
                        // — which is self-defeating: the adaptive substep count collapses to its floor under GPU
                        // load, so the chunk became 2, and 2,400 steps took 1,200 frames. At the ~10 fps this
                        // scene runs during setup that is TWO MINUTES of watching two blobs drift toward each
                        // other before anything happens. Almost certainly what Robin saw as "Theia moving away
                        // from earth at beginning".
                        //
                        // Relaxation is not the dynamics and should not be paced like it: it is a damped settle
                        // with artificial viscosity switched off, over ~2,800 particles — cheap enough that the
                        // floor was the only thing making it slow. A far higher floor finishes the settle in a
                        // second or two; the ceiling still bounds any single frame.
                        let chunk: u32 = (8 * self.sph_substeps).clamp(64, 512);
                        const TARGET: u32 = 2400; // AV-free relax is stable at the normal Courant dt ⇒ few steps
                        if let Some(sph) = self.gpu_sph.as_mut() {
                            let mut enc = self.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("sph-relax"),
                                },
                            );
                            sph.encode_relax(&mut enc, chunk);
                            self.queue.submit(std::iter::once(enc.finish()));
                            let done = steps + chunk >= TARGET;
                            if done {
                                sph.begin_readback(&self.device, &self.queue);
                                self.sph_phase = SphPhase::Approaching;
                            } else {
                                self.sph_phase = SphPhase::Relaxing(steps + chunk);
                            }
                        }
                        return;
                    }
                    // APPROACHING: the relaxed field is ready and waiting, but the bodies are still whole
                    // and far apart, so they stay solid and keep flying as bodies. Matter is resolved the
                    // moment tidal stress across the target reaches a percent of its own surface gravity
                    // — a distance the physics gives (17,700 km here, 1.86x contact), not a cue.
                    SphPhase::Approaching => {
                        // **The two SOLID bodies actually converge.** This phase used only to CHECK the
                        // separation and return — it never moved the bodies, so the approach never closed:
                        // they sat at arm's length while the analytical "IMPACT IN T−N" countdown promised
                        // a collision that could not arrive. They are whole bodies on an inbound
                        // trajectory; integrate them under gravity at the scene's fast-forward rate.
                        // resolution_distance from the LIVE bodies the engine holds (docs/58 — ONE geometry
                        // source): the planet is `prov 0`, the impactor the CLOSEST of the other source bodies.
                        // For a CAP collision the target is the BULK (no planet prov) — its index comes from the
                        // cap plan and EVERY prov is an impactor; for a whole-body impact prov 0 is the planet
                        // and prov 1.. are the impactors.
                        let (planet_i, imp_candidates): (usize, Vec<usize>) = match &self.sph_cap {
                            Some(c) => (c.planet, c.impactors.clone()),
                            None => (
                                self.sph_prov_to_body.first().copied().unwrap_or(1),
                                self.sph_prov_to_body
                                    .get(1..)
                                    .map(<[usize]>::to_vec)
                                    .unwrap_or_default(),
                            ),
                        };
                        let planet_pos = self.bodies[planet_i].pos;
                        let imp_i = imp_candidates
                            .iter()
                            .copied()
                            .min_by(|&x, &y| {
                                (self.bodies[x].pos - planet_pos)
                                    .length()
                                    .total_cmp(&(self.bodies[y].pos - planet_pos).length())
                            })
                            .unwrap_or(2);
                        let r_t = self
                            .body_meta
                            .get(planet_i)
                            .map_or(earth_radius_m(), |m| m.radius_m);
                        let r_imp = self
                            .body_meta
                            .get(imp_i)
                            .map_or(self.impactor_radius, |m| m.radius_m);
                        // Matter resolves when EITHER tides start to dominate OR the surfaces meet —
                        // whichever comes first. For a heavy impactor (Theia) the tidal distance is well
                        // outside contact, so it resolves early; for a LIGHT one (the Moon, ~9× lighter
                        // than Earth) the 1%-tidal distance falls INSIDE contact (8,600 km < 9,551 km), so
                        // contact is the honest trigger. Without the `.max`, a light body reaches contact
                        // while still "two point masses", the CPU swept detector trips, and the collision
                        // resolves twice. `.max(contact)` guarantees the ONE SPH engine takes over first.
                        let contact = r_t + r_imp;
                        let resolve_at = crate::accretion::resolution_distance(
                            self.bodies[planet_i].mass,
                            r_t,
                            self.bodies[imp_i].mass,
                            crate::accretion::RESOLVE_TIDAL_FRACTION,
                        )
                        .max(contact);
                        self.real_accum += real_dt;
                        let real_per_sub = 1.0 / 960.0;
                        let dt_sub = self.time_scale / 960.0;
                        let mut steps = 0u32;
                        while self.real_accum >= real_per_sub && steps < 96 {
                            // Hand off to the resolved SPH the moment tides make "two point masses" a lie
                            // — which is above contact, so `step_substep` never detects an N-body collision
                            // here and never materialises the wrong kind of debris.
                            let sep = (self.bodies[imp_i].pos - self.bodies[planet_i].pos).length();
                            if sep <= resolve_at {
                                self.sph_phase = SphPhase::Assembling;
                                break;
                            }
                            self.real_accum -= real_per_sub;
                            self.step_substep(dt_sub);
                            steps += 1;
                        }
                        self.push_snapshot();
                        return;
                    }
                    // ASSEMBLE: once the relaxed bodies are read back, compute the collision geometry from the
                    // ACTUAL relaxed radii, place them on the impact, and switch to the shock-safe dynamics dt.
                    SphPhase::Assembling => {
                        let relaxed = self.gpu_sph.as_mut().and_then(|s| s.take_readback());
                        if let Some(relaxed) = relaxed {
                            if let Some(cap) = self.sph_cap.as_ref() {
                                // CAP impact (docs/39): the target is the abstract BULK. The cap (prov 0) was
                                // already relaxed SEATED on the bulk, so it STAYS where it settled (offset = its
                                // own relaxed COM ⇒ `local + offset = pos`, not recentred to the origin); each
                                // impactor (prov k≥1) goes to its LIVE contact geometry. The bulk (Gauss gravity
                                // + floor) persists from the relax.
                                let (planet, impactors, r_core, bulk_mass) =
                                    (cap.planet, cap.impactors.clone(), cap.r_core, cap.bulk_mass);
                                let planet_b = self.bodies[planet];
                                let cap_com = crate::gpu_sph::body_bulk(&relaxed, 0).0;
                                let mut placements = vec![crate::gpu_sph::BodyPlacement {
                                    offset: cap_com,
                                    vel: glam::DVec3::ZERO,
                                    spin: glam::DVec3::ZERO,
                                }];
                                for &bi in &impactors {
                                    placements.push(crate::gpu_sph::BodyPlacement {
                                        offset: self.bodies[bi].pos - planet_b.pos,
                                        vel: self.bodies[bi].vel - planet_b.vel,
                                        spin: glam::DVec3::ZERO,
                                    });
                                }
                                let (particles, softening, dt) =
                                    crate::gpu_sph::assemble_from_relaxed_n(&relaxed, &placements);
                                self.sph_soft = softening as f64;
                                self.sph_dt = dt;
                                self.sph_dt_aftermath = dt * 5.0;
                                self.sph_sim_t = 0.0;
                                self.sph_snapshot.clear();
                                if let Some(sph) = self.gpu_sph.as_mut() {
                                    sph.upload(&self.queue, &particles, &self.sph_eos, softening);
                                    sph.set_dt(&self.queue, dt, 1.0);
                                    sph.set_av(&self.queue, 1.0, 2.0);
                                    sph.set_bulk(
                                        &self.queue,
                                        glam::DVec3::ZERO,
                                        r_core,
                                        glam::DVec3::ZERO,
                                        bulk_mass,
                                    );
                                }
                                self.sph_phase = SphPhase::Dynamics;
                                return;
                            }
                            // ONE geometry source (docs/58): each SPH source (`prov k`) is
                            // self.bodies[sph_prov_to_body[k]] — place it at its LIVE offset/velocity relative
                            // to the planet (`prov 0`), which spins at its own live rate (ω = spin_l / I, a
                            // VECTOR). Birth's live bodies ARE its designed approach; the moon-drop's are its
                            // real orbit — same code, no branch, no flag.
                            let planet_b =
                                self.bodies[self.sph_prov_to_body.first().copied().unwrap_or(1)];
                            let inertia = self.spin_inertia();
                            let omega = if inertia > 0.0 {
                                self.spin_l / inertia
                            } else {
                                glam::DVec3::ZERO
                            };
                            let placements: Vec<crate::gpu_sph::BodyPlacement> = self
                                .sph_prov_to_body
                                .iter()
                                .enumerate()
                                .map(|(k, &bi)| crate::gpu_sph::BodyPlacement {
                                    offset: self.bodies[bi].pos - planet_b.pos,
                                    vel: self.bodies[bi].vel - planet_b.vel,
                                    spin: if k == 0 { omega } else { glam::DVec3::ZERO },
                                })
                                .collect();
                            let (particles, softening, dt) =
                                crate::gpu_sph::assemble_from_relaxed_n(&relaxed, &placements);
                            self.sph_soft = softening as f64;
                            self.sph_dt = dt; // the SMALL shock dt (resolves the collision)
                            self.sph_dt_aftermath = dt * 5.0; // switch to this once the shock has passed
                            self.sph_sim_t = 0.0;
                            self.sph_snapshot.clear();
                            if let Some(sph) = self.gpu_sph.as_mut() {
                                sph.upload(&self.queue, &particles, &self.sph_eos, softening);
                                sph.set_dt(&self.queue, dt, 1.0);
                                sph.set_av(&self.queue, 1.0, 2.0); // restore shock-capture AV for the impact
                            }
                            self.sph_phase = SphPhase::Dynamics;
                        }
                        return;
                    }
                    // DYNAMICS: KDK substeps on the GPU + async read-back for the HUD/disk-stats/energy. The dt
                    // is the shock-safe FIXED value from `assemble_from_relaxed` — MEASURED to conserve total
                    // energy to ~0.01 % (KE→IE shock heating), so the well-relaxed bodies form a bound remnant +
                    // disk rather than dispersing (docs/35). An in-kernel per-substep adaptive dt (to trim the
                    // residual escape) is the next refinement.
                    SphPhase::Dynamics => {
                        let substeps = self.sph_substeps;
                        let merge_budget = self.sph_merge_budget;
                        // PROMOTION (docs/58): every so often, a settled self-bound clump leaves the
                        // particle set and becomes a layered body. It is a change of REPRESENTATION, so it
                        // runs on a cadence rather than every frame — and when it fires it rewrites the
                        // field, so that frame must not also step (the read-back it works from is a frame
                        // old, and stepping too would rewind the sim by that step).
                        const PROMOTE_EVERY: u32 = 120;
                        self.sph_promote_tick += 1;
                        if self.sph_promote_tick >= PROMOTE_EVERY {
                            self.sph_promote_tick = 0;
                            if self.promote_settled_bodies() {
                                return;
                            }
                        } // adaptive (frame-budget controlled) — never a fixed 100
                        if let Some(sph) = self.gpu_sph.as_mut() {
                            let mut enc = self.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("sph-step"),
                                },
                            );
                            sph.encode_kdk(&mut enc, substeps);
                            // Coarsen only while over budget (docs/08/44). `encode_merge` rebuilds the grid
                            // itself, so it is safe after the KDK batch; at budget 0 the kernels early-out.
                            if merge_budget > 0 {
                                sph.encode_merge(&mut enc);
                            }
                            self.queue.submit(std::iter::once(enc.finish()));
                            if let Some(snap) = sph.take_readback() {
                                self.sph_snapshot = snap;
                                // One coarse step the CPU can see: the guard-band resample
                                // and the pi-gate read key off this (docs/59).
                                self.sph_snapshot_gen += 1;
                            }
                            sph.begin_readback(&self.device, &self.queue);
                        }
                        // Scheduled dt (docs/42): once the shock window has passed, coarsen the dt for the slow
                        // disk aftermath (WebGPU can't read back the adaptive Courant dt, so we schedule by time).
                        // The promoted bodies are no longer in the shader, so advance them here — over the
                        // SAME interval the particles just took, or they would drift out of step with the
                        // field they gravitate on.
                        self.step_promoted_bodies(substeps as f64 * self.sph_dt as f64);
                        self.sph_sim_t += substeps as f64 * self.sph_dt as f64;
                        if self.sph_dt < self.sph_dt_aftermath
                            && self.sph_sim_t > SPH_SHOCK_WINDOW_S
                        {
                            self.sph_dt = self.sph_dt_aftermath;
                            if let Some(sph) = self.gpu_sph.as_mut() {
                                sph.set_dt(&self.queue, self.sph_dt, 1.0);
                            }
                        }
                        return;
                    }
                }
            }
            self.phys_clock += real_dt;
            if self.geologic {
                // Millennia per second: the validated secular law in 50-year strides (exactly
                // L-conserving; see tides::secular_step). N-body/cloud machinery is retired — at this
                // LOD the orbit-averaged equations ARE the physics.
                let years = self.geo_rate_yr_s * real_dt;
                let year_s = 3.156e7;
                let mut left = years;
                while left > 0.0 {
                    let step = left.min(50.0);
                    let (_merged, shed) = crate::tides::secular_step(
                        &mut self.geo_moonlets,
                        &mut self.spin_l,
                        self.bodies[1].mass,
                        earth_radius_m(),
                        crate::tides::EARTH_K2_OVER_Q,
                        step * year_s,
                    );
                    // A moonlet that decayed inside the Roche limit was shredded: its mass rains onto Earth
                    // (angular momentum already added to the spin in secular_step). Mass is conserved.
                    self.bodies[1].mass += shed;
                    left -= step;
                }
                self.sim_since_impact += years * year_s;
                self.push_snapshot();
                return;
            }
            self.real_accum += real_dt;
            // Cheap orbital phase: fast-forward the N-body system in fixed sub-steps whose COUNT tracks the
            // wall clock, so the physics rate is display-independent (a 30 fps and a 120 fps client simulate
            // the same world). A single substep may DETECT an imminent collision; the loop then stops and
            // the colliding set is routed through the ONE SPH engine below (docs/58) — there is no CPU
            // debris path to fall into. Under overload the observable clock dilates, keeping the frame
            // interactive without corrupting the physics with an oversized step.
            const MAX_SUBSTEPS: u32 = 128;
            let (dt_sub, real_per_sub) = (self.time_scale / 960.0, 1.0 / 960.0);
            let mut steps = 0u32;
            while self.real_accum >= real_per_sub && steps < MAX_SUBSTEPS {
                self.real_accum -= real_per_sub;
                steps += 1;
                self.step_substep(dt_sub);
                if self.pending_sph_route.is_some() {
                    break; // a contact was detected — stop integrating and hand it to the SPH engine
                }
            }
            if steps >= MAX_SUBSTEPS {
                self.real_accum = 0.0;
            }
            // Execute a detected collision AFTER the loop: `begin_sph_relax` rebuilds the whole scene, so it
            // cannot run mid-substep. This is the sole path from a detected orbital collision to resolution.
            if let Some(set) = self.pending_sph_route.take() {
                self.route_bodies_to_sph(&set);
            }
            self.push_snapshot();
        }

        /// One physics substep of the cheap orbital phase: N-body verlet + swept collision DETECTION.
        /// Pure ballistics — no collision RESPONSE lives here. **Detection is the ENGINE's job** (Robin:
        /// "it must be the sole owner of collisions"): when the swept test finds an imminent contact, the
        /// engine routes the whole colliding set through the ONE SPH engine (docs/58) and the GPU resolves
        /// the touch. The legacy CPU Aggregate that used to materialise debris here is retired.
        fn step_substep(&mut self, dt: f64) {
            // An ARMED drop counts down in sim time and fires at the substep boundary nearest
            // the solved window (±dt/2 - the wiring's honest quantization, measured in the
            // intercept tests). The solver chose the time; nothing else about the drop changes.
            // The release de-orbits and routes through the ONE SPH engine (`route_bodies_to_sph`,
            // the same call the un-armed Drop makes); the relax it starts touches only the SPH
            // state, so the remaining substeps of this frame stay pure ballistics (`sph_active`
            // returns early below) until the machine owns the next `advance`.
            if let Some(w) = self.armed_drop.as_mut() {
                if w.release_in_s <= 0.5 * dt {
                    self.armed_drop = None;
                    self.release_drop();
                } else {
                    w.release_in_s -= dt;
                    w.impact_in_s -= dt;
                }
            }
            let strength =
                self.mats[materials::index_of(&self.mats, "basalt")].fracture_strength as f64;
            // Each body's radius comes from its OWN metadata — no shared `impactor_radius`, so two moons
            // of different sizes would collide at their own contact distances.
            let radii: Vec<f64> = self.body_meta.iter().map(|m| m.radius_m).collect();
            let body_state = |i: usize| {
                let r = radii.get(i).copied().unwrap_or(earth_radius_m());
                crate::interaction::BodyState {
                    pos: self.bodies[i].pos,
                    vel: self.bodies[i].vel,
                    mass_kg: self.bodies[i].mass,
                    radius_m: r,
                    strength_pa: strength,
                    // The FLUID branch is not wired in this scene yet (docs/59 Stage A wires it in Terra,
                    // which is where the swarm flies). It is not free to leave off: proto-Earth carries a
                    // real ~100-bar atmosphere, so Theia's approach genuinely passes through air. MEASURED
                    // for this scene's numbers — ρ₀≈23 kg/m³, H≈44 km, Theia at ~9 km/s — the drag comes to
                    // ~0.02 m/s² over a ~5 s crossing, ≈0.1 m/s off a 9 km/s approach: 1e-5 of the speed,
                    // far below anything the impact outcome resolves. Declaring it `None` here is therefore
                    // a statement about magnitude, not a scene opting out of physics.
                    air: None,
                }
            };
            let before: Vec<crate::interaction::BodyState> =
                (0..self.bodies.len()).map(body_state).collect();

            crate::orbit::verlet_step(&mut self.bodies, &mut self.acc, dt);
            // The planet visibly ROTATES at the rate its spin angular momentum implies.
            self.spin_angle += dt * self.spin_l.length() / self.spin_inertia();

            // ONE collision engine (docs/58): while a live SPH resolve owns the frame — the Approaching
            // phase integrates the infall through this very function — the GPU SPH engine is the sole
            // owner of contact. The legacy CPU swept-detection + Aggregate materialization below must NOT
            // run, or a collision resolves twice (a light impactor reaching contact trips both). During a
            // resolve, `step_substep` is pure ballistics; the SPH engine handles the touch.
            if self.sph_active {
                return;
            }

            // The integrated endpoints, and which bodies may still collide (a moon already materialised
            // into Earth is no longer a distinct body to detect). The Sun (index 0) never reaches anything
            // — it is checked and correctly finds nothing, which is more honest than special-casing it out.
            // A body handed to the SPH machine is excluded too: its collision belongs to the particle
            // physics now, and CPU-materialising it here would resolve the same matter twice.
            let after_pos: Vec<glam::DVec3> = self.bodies.iter().map(|b| b.pos).collect();
            let active: Vec<bool> = (0..self.bodies.len())
                .map(|i| self.body_meta.get(i).map_or(true, |m| !m.materialized))
                .collect();
            let collisions = crate::interaction::detect_swept(&before, &after_pos, &active);

            // Route the colliding cluster through the ONE SPH engine (docs/58 — the single collision
            // engine): the most-massive struck body is the planet (prov 0); every impactor that struck
            // joins it. `begin_sph_relax` rebuilds the whole scene, so record the set here and let
            // `advance` execute the route after the substep loop unwinds. The legacy CPU Aggregate that
            // used to materialise debris here is RETIRED — there is no second contact path.
            let planet = self.planet_idx();
            let mut set: Vec<usize> = vec![planet];
            for c in &collisions {
                self.impacted = true;
                self.impact_energy_j += c.energy_j;
                for idx in [c.struck, c.striker] {
                    if idx >= 2 && !set.contains(&idx) {
                        set.push(idx);
                    }
                }
            }
            if set.len() > 1 {
                self.pending_sph_route = Some(set);
            }
        }

        /// Record the observable state at the current physics clock (the renderer's source of truth).
        fn push_snapshot(&mut self) {
            self.snaps.push_back(FrameSnap {
                t: self.phys_clock,
                bodies: self.bodies.iter().map(|b| b.pos).collect(),
            });
            // Keep a little more history than the lag needs; drop the rest.
            let horizon = self.phys_clock - (RENDER_LAG_S + 0.5);
            while self.snaps.len() > 2 && self.snaps.front().is_some_and(|f| f.t < horizon) {
                self.snaps.pop_front();
            }
        }

        /// The state the RENDERER sees: body positions interpolated at (now − RENDER_LAG_S). Falls
        /// back to the live state before the first snapshot exists. (The SPH particle field draws
        /// straight from its GPU buffer and needs no snapshot lag: a whole KDK batch is resolved
        /// before its command buffer presents.)
        fn sampled_state(&self) -> Vec<glam::DVec3> {
            if self.snaps.is_empty() {
                return self.bodies.iter().map(|b| b.pos).collect();
            }
            let target = self.phys_clock - RENDER_LAG_S;
            // Bracket the target time (snaps are time-ordered).
            let mut s0 = self.snaps.front().unwrap();
            let mut s1 = s0;
            for f in self.snaps.iter() {
                s1 = f;
                if f.t > target {
                    break;
                }
                s0 = f;
            }
            let f = if s1.t > s0.t {
                ((target - s0.t) / (s1.t - s0.t)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            s0.bodies
                .iter()
                .zip(s1.bodies.iter())
                .map(|(a, b)| *a + (*b - *a) * f)
                .collect()
        }

        pub fn render(&mut self) -> Result<(), JsValue> {
            // NO physics here (docs/13): the renderer samples the physics snapshots RENDER_LAG_S behind
            // the live state — every event it draws is already fully resolved. The physics is advanced
            // by `advance(real_dt)`, on wall-clock time, independent of this function's call rate.
            let r_bodies = self.sampled_state();

            // Render in the focused body's frame of reference (docs/17): its position is the origin,
            // everything else is drawn relative to it. Switching focus re-centres the whole view.
            let focus = r_bodies[self.focus];
            let sun = r_bodies[0];

            // The demo arc's pose, composed in this same lagged frame, replaces the manual rig's
            // view while it drives. Built in f64 and cast once, with a distance-scaled near plane
            // (the fly camera's discipline): at the arc floor the ground is ~1.4 km away, far
            // inside the manual rig's fixed 0.05-display-unit near plane; near never EXCEEDS the
            // manual value and far stays put, so the top of the arc is exactly the manual frustum.
            let arc_pose = self.arc_pose_world(r_bodies[self.planet_idx()]);
            let view_proj = match arc_pose {
                Some((eye_w, target_w, up, d)) => {
                    let ds = display_scale();
                    let aspect = self.config.width as f64 / self.config.height.max(1) as f64;
                    let near = (0.03 * d * ds).min(0.05);
                    // SPACE_FOV_Y, not a bare 0.9. This is the ARC's projection, and the arc is one of
                    // two camera systems this scene can switch between — which is exactly the case
                    // Robin's condition covers: *"as long as we can unify FOV within the engine for
                    // rendering"*. If the arc rendered at its own literal, switching producers would
                    // silently change how wide the world is.
                    let proj = glam::DMat4::perspective_rh(
                        SPACE_FOV_Y as f64,
                        aspect.max(1.0e-3),
                        near,
                        100_000.0,
                    );
                    let view =
                        glam::DMat4::look_at_rh((eye_w - focus) * ds, (target_w - focus) * ds, up);
                    (proj * view).as_mat4()
                }
                None => self.view_proj(),
            };

            // GPU SPH impact (docs/33 stage 4c.4): push the particle-shader camera uniform. The particle
            // system lives in an Earth-relative f32 frame, so its display origin is Earth's position in the
            // focused frame; the shader maps each Earth-relative position through display_scale() and view_proj.
            if self.sph_active {
                let origin = ((r_bodies[self.planet_idx()] - focus) * display_scale()).as_vec3();
                let cam = crate::gpu_sph::SphCam {
                    view_proj: view_proj.to_cols_array_2d(),
                    origin: [origin.x, origin.y, origin.z, 0.0],
                    // billboard half-size fades with the render blend (docs/42): 0 at the pretty end, full at the
                    // physics end. z/w (Phase 3): matter beyond ~6.5e6 m (just past the sub-scale remnant) is
                    // EJECTA — it keeps a glowing mote size (0.006) even at the pretty end, so the sphere wears a
                    // real ejecta plume.
                    // Particles fade IN exactly as the surface fades out — one matter, one budget.
                    params: [
                        display_scale() as f32,
                        0.013 * self.render_blend as f32,
                        6.5e6,
                        0.006,
                    ],
                };
                self.queue
                    .write_buffer(&self.sph_cam.buf, 0, bytemuck::bytes_of(&cam));
            }

            // Light direction = TO the real Sun from each body (per-body; the Sun is the illuminant,
            // not a hardcoded direction). So the lit hemisphere and the phases come from the geometry.
            let earth_light = (sun - r_bodies[self.planet_idx()]).as_vec3().normalize();
            // EARTH AS PARTICLES (docs/15): the planet renders as a shell of coarse grains — the honest
            // low-res visualization of the un-materialized bulk (whose PHYSICS is the boundary + gravity
            // source). A smooth sphere would hide excavation; grains can be missing. Shell points inside
            // the materialized impact region are hidden — the real (moving, glowing) cap particles are
            // the matter there now, and the void they leave IS the crater.
            let earth_center = r_bodies[self.planet_idx()];
            // The impact now runs the REAL Earth and the REAL Theia (their own definitions), so there is no
            // sub-scale body and no second display scale: both branches draw at display_scale() and differ
            // only in which radius they ask for. The `render_blend` cross-fade between the resolved surface
            // and the particle field remains, and is the next thing to go — see the note at its use.
            // Earth's RADIUS comes from Earth's definition — the scene does not get to say how big Earth
            // is. (During the GPU impact the body is deliberately rendered at the sub-scale radius the SPH
            // field actually occupies, which is a declared visualization scale, not a second Earth.)
            // **The target, as it currently is.** Measured every frame: where it sits, how big it still
            // is, and whether it is still one thing. A struck planet SHRINKS to its remnant rather than
            // fading out — drawing it at its original radius and dimming it as mass left was what made it
            // flicker and vanish while a perfectly good remnant sat there.
            // Same call, same rule, for the target.
            let target = {
                let (pos, mass) = self.body_particles(0);
                let r_dec = self.impact_def.target.radius_m();
                let resolved = (!pos.is_empty()).then_some((&pos[..], &mass[..]));
                match crate::accretion::representation(resolved, earth_center, r_dec, 0.75) {
                    crate::accretion::Representation::Surface {
                        centre,
                        radius,
                        coherence,
                    } => Some((centre, radius, coherence)),
                    crate::accretion::Representation::Particles => None,
                }
            };
            // A CAP impact (docs/39 resolution-on-demand) leaves the target a WHOLE solid globe — only a cap
            // of it became particles — so draw it at its FULL radius and centre, not the tiny cap centroid
            // that `body_particles(0)` (now the cap material) would give. The SPH cap + impactor still draw as
            // particles at the impact site, overlaid on the intact planet.
            let target = if let Some(c) = self.sph_cap.as_ref() {
                let r = self
                    .body_meta
                    .get(c.planet)
                    .map_or_else(|| crate::planet::body("earth").radius(), |m| m.radius_m);
                Some((earth_center, r, 1.0))
            } else {
                target
            };
            let (pretty_scale, pretty_r_surf) = match (&target, self.sph_active) {
                (Some((_, r, _)), _) => (display_scale(), *r),
                (None, true) => (display_scale(), self.impact_def.target.radius_m()),
                (None, false) => (display_scale(), crate::planet::body("earth").radius()),
            };
            // Coherence decides how it is drawn — no dial. Intact ⇒ the resolved surface; genuinely torn
            // apart ⇒ the particles that ARE the matter; re-accreted ⇒ a surface again. Smoothstepped so
            // the handover is continuous rather than a pop.
            let coherence = target.map_or(1.0, |(_, _, c)| c);
            let pretty_fade = crate::accretion::surface_weight(coherence, 0.55);
            self.render_blend = 1.0 - pretty_fade as f64; // particles take over as the surface loses meaning

            // docs/59 - the materialized site: rebuild its instances in the lagged render frame
            // (site-local particles mapped through the rotating tangent frame onto Earth-relative
            // metres - the site rides the crust like the shell grains and the crater) and push
            // its camera slot; drawn later in the pass through the ONE billboard pipeline. The
            // radial anchor is the DRAWN surface (body radius plus the raster's local elevation),
            // so the site sits on the planet the viewer actually sees; procedural-patch relief vs
            // raster elevation converging into one surface is docs/54's noted seam.
            let site_count = {
                let Self {
                    site,
                    site_spec,
                    site_buf,
                    site_cam,
                    device,
                    queue,
                    earth_surface,
                    spin_l,
                    spin_angle,
                    ..
                } = &mut *self;
                if let (Some(site), Some(spec)) = (site.as_ref(), site_spec.as_ref()) {
                    let spin_axis = spin_l.try_normalize().unwrap_or(glam::DVec3::Z);
                    let spin_rot = glam::DQuat::from_axis_angle(
                        spin_axis,
                        *spin_angle % (2.0 * std::f64::consts::PI),
                    );
                    let (up, north, east) = crate::geo::tangent_frame(spec.lat_deg, spec.lon_deg);
                    let (up, north, east) = (spin_rot * up, spin_rot * north, spin_rot * east);
                    let elev = earth_surface
                        .as_ref()
                        .and_then(|s| {
                            s.elevation.as_ref().map(|r| {
                                r.elevation_m_at(
                                    spec.lat_deg,
                                    spec.lon_deg,
                                    s.elev_range[0],
                                    s.elev_range[1],
                                )
                                .max(0.0)
                                    * s.relief_exag
                            })
                        })
                        .unwrap_or(0.0);
                    let base = up * (pretty_r_surf + elev);
                    let mut inst = Vec::with_capacity(site.particles.len());
                    for q in &site.particles {
                        let w = base
                            + east * q.pos[0] as f64
                            + up * q.pos[1] as f64
                            + north * q.pos[2] as f64;
                        let mut qq = *q;
                        qq.pos = w.as_vec3().to_array();
                        inst.push(qq);
                    }
                    let bytes: Vec<u8> = bytemuck::cast_slice(&inst).to_vec();
                    let need = bytes.len() as u64;
                    if site_buf.as_ref().map_or(true, |b| b.size() < need) {
                        *site_buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("site-particles"),
                            size: need,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        }));
                    }
                    queue.write_buffer(site_buf.as_ref().unwrap(), 0, &bytes);
                    let origin = ((earth_center - focus) * display_scale()).as_vec3();
                    let cam = crate::gpu_sph::SphCam {
                        view_proj: view_proj.to_cols_array_2d(),
                        origin: [origin.x, origin.y, origin.z, 0.0],
                        // The same mote half-size the SPH ejecta use (one particle look); the
                        // ejecta-radius param is pushed out so that ramp never fires for ground.
                        params: [display_scale() as f32, 0.006, 1.0e12, 0.0],
                    };
                    queue.write_buffer(&site_cam.buf, 0, bytemuck::bytes_of(&cam));
                    inst.len() as u32
                } else {
                    0
                }
            };
            // **The impactor, by the same rule.** Measured position, measured radius, drawn as a solid
            // body for as long as it IS one — and it is one, all the way in, because nothing has touched
            // it yet. Its own declared geotherm decides whether it glows: Theia's mantle is 1,200 K, hot
            // from accretion, so it does.
            // Where the impactor is, and how big — from its PARTICLES once they exist, and from the body
            // itself before that. The first cut asked only the particles, so during the approach (when
            // there are none, by design) it drew nothing at all and Theia was invisible until it came
            // apart. A body is drawn as a body from the moment the scene places it.
            // THE shared representation rule (accretion::representation) — the same call a droplet
            // striking a petal would make. The scene supplies this body's particles (if any) and its
            // declared place and size; the engine decides whether that is a surface or a debris field.
            let impactor = {
                let (pos, mass) = self.body_particles(1);
                let declared = self
                    .bodies
                    .get(2)
                    .map(|b| b.pos)
                    .unwrap_or(glam::DVec3::ZERO);
                let r_dec = self.impact_def.impactor.definition().radius();
                let resolved = (!pos.is_empty()).then_some((&pos[..], &mass[..]));
                match crate::accretion::representation(resolved, declared, r_dec, 0.75) {
                    crate::accretion::Representation::Surface {
                        centre,
                        radius,
                        coherence,
                    } => Some((centre, radius, coherence)),
                    crate::accretion::Representation::Particles => None,
                }
            };
            // Its OWN coherence, independent of the target's — the two bodies are disrupted at different
            // times by different amounts, and each is drawn as what it currently is.
            let impactor_fade =
                impactor.map_or(0.0, |(_, _, c)| crate::accretion::surface_weight(c, 0.55));
            if let Some((ic, ir, _)) = impactor {
                let ipos = ((ic - focus) * pretty_scale).as_vec3();
                let idef = self.impact_def.impactor.definition();
                let imat = idef
                    .layers
                    .last()
                    .map(|l| l.material.clone())
                    .unwrap_or_else(|| "peridotite".into());
                let ialb = self.mats[materials::index_of(&self.mats, &imat)].albedo;
                write_space_uniform(
                    &self.queue,
                    &self.impactor_uni,
                    view_proj,
                    Mat4::from_translation(ipos)
                        * Mat4::from_scale(Vec3::splat((ir * pretty_scale) as f32)),
                    earth_light,
                    [ialb[0], ialb[1], ialb[2], impactor_fade],
                    {
                        let g = glow_of(&idef);
                        [g[0], g[1], g[2], g[3]]
                    },
                    AIRLESS,
                    glow_of(&idef),
                );
            }
            // docs/42 Phase 3 — atmosphere MIST: the giant impact vaporizes rock into a thick, shocked vapor
            // atmosphere, so the Rayleigh veil is boosted while the impact is live → a hazy, glowing limb.
            let atm_tau_eff = if self.sph_active {
                [
                    self.atm_tau[0] * 2.6,
                    self.atm_tau[1] * 2.6,
                    self.atm_tau[2] * 2.6,
                ]
            } else {
                self.atm_tau
            };
            let shell_spacing =
                pretty_r_surf * (4.0 * std::f64::consts::PI / SHELL_N as f64).sqrt();
            // Grains overlap MORE while the GPU impact is live (0.90 vs 0.62 of the spacing) so the crust reads
            // as opaque — the glowing interior then shows ONLY through the actual crater hole, not every crevice.
            let grain_overlap = if self.sph_active { 0.90 } else { 0.62 };
            let shell_grain_r =
                ((grain_overlap * shell_spacing) * pretty_scale) as f32 * pretty_fade;
            // docs/42 Phase 2 — capture the giant-impact crater site from the GPU field: at first Theia (prov 1)
            // contact with Earth (prov 0) freeze the impact DIRECTION (Earth-relative), then open the bowl over
            // ~1 s. Persists after (bake-back). The bowl radius grows with `gpu_crater_frac` (set in the crater
            // block below). `earth_center + dir·pretty_r_surf` lands it on the sub-scale surface, same frame as
            // the shell grains, so the `hidden` test carves the crust exactly where Theia struck.
            if self.sph_active && !self.sph_snapshot.is_empty() {
                let (mut ec, mut me, mut tc, mut mt) =
                    (glam::DVec3::ZERO, 0.0f64, glam::DVec3::ZERO, 0.0f64);
                let (mut ev, mut tv) = (glam::DVec3::ZERO, glam::DVec3::ZERO);
                for p in &self.sph_snapshot {
                    let pos = glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
                    let vel = glam::DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
                    let m = p.mass as f64;
                    if p.prov == 0 {
                        ec += pos * m;
                        me += m;
                        ev += vel * m;
                    } else {
                        tc += pos * m;
                        mt += m;
                        tv += vel * m;
                    }
                }
                if me > 0.0 && mt > 0.0 {
                    let (ec, tc) = (ec / me, tc / mt);
                    if self.gpu_impact_site.is_none() && (tc - ec).length() < 1.3e7 {
                        self.gpu_impact_site = (tc - ec).try_normalize(); // contact ≈ r_e + r_t (sub-scale)
                                                                          // Freeze the pi-scaling prediction from the MEASURED contact state
                                                                          // (docs/59): the barycentric closing speed the field actually carries,
                                                                          // the impactor's measured particle mass over its declared radius, and
                                                                          // the site body's own outer-layer density and surface gravity. Only a
                                                                          // world with a declared site runs the cross-check (the gate reads the
                                                                          // site HUD).
                        if let Some(spec) = self.site_spec.as_ref() {
                            let speed = (tv / mt - ev / me).length();
                            let a = self.impactor_radius;
                            if speed > 0.0 && a > 0.0 {
                                let s = crate::refine::ImpactSpec {
                                    impactor_radius_m: a,
                                    impactor_density: mt
                                        / (4.0 / 3.0 * std::f64::consts::PI * a.powi(3)),
                                    speed_ms: speed,
                                    target_density: spec.coarse_density,
                                    gravity: spec.g_ms2,
                                };
                                self.pi_prediction = Some((
                                    crate::refine::rim_radius_gravity_m(
                                        &s,
                                        &crate::refine::HARD_ROCK,
                                    ),
                                    speed,
                                ));
                            }
                        }
                    }
                    if self.gpu_impact_site.is_some() {
                        self.gpu_crater_frac = (self.gpu_crater_frac + 0.03).min(1.0);
                        // DEPTH FROM EXCAVATED MASS (docs/46 row 18). The target material now lifted above
                        // its own pristine surface is, by conservation, the material missing from the bowl.
                        // Its volume at the cap's real density gives the hole; a simple crater's bowl is a
                        // PARABOLOID, V = ½·π·R²·d, so d = 2V/(πR²). Nothing here is authored — an impact
                        // that excavates nothing leaves no crater, and one that excavates more digs deeper.
                        let r_surf = self
                            .body_meta
                            .get(self.planet_idx())
                            .map_or(earth_radius_m(), |m| m.radius_m);
                        let (mut m_exc, mut vol) = (0.0f64, 0.0f64);
                        for p in &self.sph_snapshot {
                            if p.prov != 0 {
                                continue;
                            }
                            let pos =
                                glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
                            if pos.length() > r_surf {
                                let m = p.mass as f64;
                                m_exc += m;
                                vol += m / (p.rho.max(1.0) as f64);
                            }
                        }
                        // BOWL GEOMETRY FROM THE MEASURED VOLUME (docs/46 row 18).
                        //
                        // The radius used to be `gpu_crater_frac * 0.72 * r_surf` — an inherited dial, and
                        // the rig proved it was the bug: it produced a 4,600 km bowl only 282 km deep, a
                        // depth:radius of 0.06 where real simple craters run ~0.4. The crater was reaching
                        // the shader correctly and rendering as an imperceptible saucer, which is why it
                        // "never showed" however right the depth was.
                        //
                        // Both numbers now come from the SAME measured excavated volume plus ONE sourced
                        // shape fact: a simple crater's depth is about a fifth of its diameter (Melosh,
                        // *Impact Cratering*), i.e. d = 0.4 R. For a paraboloid V = ½πR²d, so
                        // V = 0.2πR³  ⇒  R = (V / 0.2π)^⅓,  and d follows. Excavate nothing and there is
                        // no crater; excavate more and it grows in BOTH dimensions, keeping its shape.
                        let (r_bowl, d) = crate::accretion::crater_bowl(vol);
                        if r_bowl > 0.0 {
                            // A bowl cannot be deeper than the cap that was resolved to make it; past that
                            // the excavation is reaching matter this resolution never promoted.
                            self.gpu_crater_depth_frac = (d / r_surf).min(0.25);
                            self.gpu_crater_r_frac = (r_bowl / r_surf).min(0.9);
                        }
                        let _ = m_exc;
                    }
                }
            }
            // Camera eye in display coordinates (relative to the focus body) — the same construction
            // as view_proj (the arc's eye while it drives, the orbit distance around the panned look
            // target otherwise), needed for the per-grain Rayleigh view path.
            let eye_disp = match arc_pose {
                Some((eye_w, _, _, _)) => (eye_w - focus) * display_scale(),
                None => {
                    self.camera.eye_dir().as_dvec3()
                        * (self.camera.base_distance * self.camera.zoom) as f64
                        + self.camera.pan.as_dvec3()
                }
            };
            let sun_dir_earth = (sun - earth_center).normalize_or_zero();
            let spin_axis = self.spin_l.try_normalize().unwrap_or(glam::DVec3::Z);
            let spin_rot = glam::DQuat::from_axis_angle(
                spin_axis,
                self.spin_angle % (2.0 * std::f64::consts::PI),
            );
            // The giant-impact crater (docs/42 Phase 2): the frozen site on the sub-scale surface;
            // the bowl opens with the shock. This is the ONE crater source now; the CPU shatter's
            // co-rotating `impact_site_rel` bowl went with the Aggregate debris path (docs/58 #7).
            let (crater_site, crater_r) = if self.sph_active {
                match self.gpu_impact_site {
                    Some(dir) => (
                        Some(earth_center + dir * pretty_r_surf),
                        self.gpu_crater_frac * 0.72 * pretty_r_surf,
                    ),
                    None => (None, 0.0),
                }
            } else {
                // No CPU crater: the retired Aggregate used to carve one here from `impact_site_rel`.
                (None, 0.0)
            };
            // OBLATE figure: the spin flattens the planet (Radau–Darwin) — equator bulges (+f/3),
            // poles sink (−2f/3), volume-preserving to first order. At today's day it's 1/298
            // (imperceptible); at the post-impact 3.8-h day it's ~13% — a visibly squashed world.
            let spin_omega_r = self.spin_l.length() / self.spin_inertia();
            let flat = crate::tides::flattening_from_spin(
                spin_omega_r,
                self.bodies[self.planet_idx()].mass,
                earth_radius_m(),
            );
            // **The definitive Earth's transform.** One draw: spin the CRUST (so continents co-rotate,
            // exactly as they must), flatten it by the spin's own oblateness, and scale to the display.
            // The Rayleigh veil is not applied here — globe.wgsl scatters the declared air itself, using
            // the one shared model, so this scene and Terra cannot disagree about Earth's atmosphere.
            if self.globe_mesh.is_some() {
                // Draw it where the matter actually is. During the impact that is the measured remnant
                // centre, which drifts as mass is lost; outside it, the body's own position.
                let centre = target.map_or(earth_center, |(c, _, _)| c);
                let spos = ((centre - focus) * pretty_scale).as_vec3();
                let f = flat as f32;
                // Volume-preserving to first order: +f/3 at the equator, −2f/3 at the poles, about the
                // spin axis. At today's day this is 1/298 (invisible); at the post-impact 3.8-h day it is
                // a visibly squashed world — and it is the SAME flattening the shell used.
                let r = (pretty_r_surf * pretty_scale) as f32;
                let oblate = Mat4::from_scale(Vec3::new(
                    r * (1.0 + f / 3.0),
                    r * (1.0 - 2.0 * f / 3.0),
                    r * (1.0 + f / 3.0),
                ));
                let spin_m = Mat4::from_quat(glam::Quat::from_xyzw(
                    spin_rot.x as f32,
                    spin_rot.y as f32,
                    spin_rot.z as f32,
                    spin_rot.w as f32,
                ));
                write_space_uniform(
                    &self.queue,
                    &self.globe_uni,
                    view_proj,
                    Mat4::from_translation(spos) * spin_m * oblate,
                    earth_light,
                    [1.0, 1.0, 1.0, pretty_fade],
                    [eye_disp.x as f32, eye_disp.y as f32, eye_disp.z as f32, 0.0],
                    self.air(),
                    // The heat of whichever body this scene actually PLACED. Only the WHOLE-BODY birth impact
                    // goes back in time: it targets proto-Earth, a magma ocean, which glows rather than being
                    // lit. A CAP impact (docs/39) resolves modern Earth — cool rock whose bulk stays a solid
                    // globe — so it must NOT inherit the proto-Earth glow (that is what set the present-day
                    // Earth on fire); only the impact-site cap particles are hot, and they draw themselves.
                    if self.sph_active && self.sph_cap.is_none() {
                        glow_of(&self.impact_def.target.definition())
                    } else {
                        glow_of(&crate::planet::body("earth"))
                    },
                );

                // Patch the measured crater into the globe's uniform (docs/46 row 18). The axis must be in
                // MODEL space, and the model matrix spins with the crust — so the bowl is un-rotated by the
                // same spin, exactly as `crater_site` is rotated INTO world space for the shell grains. The
                // crater and the matter it is cut from must share one frame.
                let r_surf_now = self
                    .body_meta
                    .get(self.planet_idx())
                    .map_or(earth_radius_m(), |m| m.radius_m);
                if self.gpu_crater_r_frac > 0.0 && self.gpu_crater_depth_frac > 0.0 {
                    if let Some(dir) = self.gpu_impact_site {
                        let axis = spin_rot.inverse() * dir;
                        // Angular radius of the bowl on the sphere: asin(R_bowl / R_surface).
                        let theta = self.gpu_crater_r_frac.clamp(0.0, 0.95).asin();
                        let c: [f32; 4] =
                            [axis.x as f32, axis.y as f32, axis.z as f32, theta as f32];
                        let c2: [f32; 4] = [self.gpu_crater_depth_frac as f32, 0.0, 0.0, 0.0];
                        // DIAGNOSTIC: is the bowl actually reaching the shader, and how big is it? A render
                        // change cannot be trusted from a screenshot alone when the impact site may be on
                        // the night side — this reports the numbers behind the picture.
                        // Report the bowl only when it MEASURABLY changes, not every frame: a render change
                        // has to be checkable by number as well as by eye, because the impact site may be on
                        // the night side where no screenshot can settle it.
                        let stamp = (self.gpu_crater_depth_frac * 200.0).round() as i32;
                        if stamp != self.gpu_crater_logged {
                            self.gpu_crater_logged = stamp;
                            log::info!(
                                "crater: depth={:.0} km radius={:.0} km (d/r={:.2}) theta={:.3}rad axis=({:.2},{:.2},{:.2})",
                                self.gpu_crater_depth_frac * r_surf_now / 1e3,
                                self.gpu_crater_r_frac * r_surf_now / 1e3,
                                if self.gpu_crater_r_frac > 0.0 { self.gpu_crater_depth_frac / self.gpu_crater_r_frac } else { 0.0 },
                                theta, axis.x, axis.y, axis.z
                            );
                        }
                        self.queue.write_buffer(
                            &self.globe_uni.buf,
                            CRATER_UNIFORM_OFFSET,
                            bytemuck::cast_slice(&[c, c2]),
                        );
                    }
                }
            }
            // **The descent corridor picks up Terra's close-range treatment.** Below the DERIVED
            // hand-off altitude the planetary rasters no longer fill the view — one texel exceeds
            // the docs/49 angular budget, the same budget the site materialization threshold uses —
            // so this scene builds the SAME fine ground cap Terra builds (`terra::ground_cap`), from
            // the SAME sampler the globe mesh is built from, cross-faded in by the SAME derived
            // fade; the coarse globe is skipped only once the cap covers the view past the horizon.
            // Where even the finest raster is exhausted (the known missing finer LOD tier), the
            // cap's raster texels at their true size plus globe.wgsl's material relief mottle are
            // the honest floor — stretching would be blur pretending to be data.
            //
            // The cap's vertices are built in the BODY (crust) frame around the sub-camera point
            // and subtracted from the eye in f64; the draw then goes through the GLOBE'S OWN
            // conventions (the same view_proj, the spin as the model rotation, the eye re-added as
            // an f64-built translation), so the cap and the globe cannot disagree about where or
            // how the same surface is drawn, and the absolute-f32 residual stays inside the
            // sub-pixel bound the arc floor itself is licensed by (crate::arc's floor test). The
            // manual rig's zoom floor sits above the hand-off, so the arc's descent is what crosses
            // it; during the GPU impact the planet is drawn at the SPH field's sub-scale radius and
            // the particle field is the matter, so no cap.
            let mut corridor_cap = false;
            let mut corridor_skip_globe = false;
            if let (Some((eye_w, aim_w, _, d_m)), Some(surf), false) =
                (arc_pose, self.earth_surface.as_ref(), self.sph_active)
            {
                let theta = crate::resolution::ResolutionController::default().angular_resolution;
                let cap_start_alt = crate::terra::ground_cap::handoff_alt_m(
                    crate::terra::ground_cap::finest_texel_arc_m(
                        &[
                            surf.landmask.as_ref(),
                            surf.elevation.as_ref(),
                            surf.landcover.as_ref(),
                        ],
                        earth_radius_m(),
                    )
                    .unwrap_or(0.0),
                    theta,
                );
                // Below the derived hand-off, the corridor's own surface takes over. This was
                // `cap_fade(d_m, ..) > 0.0`, which meant exactly this and then spent the fade blending
                // two meshes; with one mesh there is nothing to blend, only a threshold to cross.
                if d_m < cap_start_alt && self.globe_mesh.is_some() {
                    let centre = target.map_or(earth_center, |(c, _, _)| c);
                    let ds = pretty_scale;
                    let r_draw = pretty_r_surf * ds;
                    let q_inv = spin_rot.conjugate();
                    let eye_body = (q_inv * (eye_w - centre)) * ds; // body frame, display units
                                                                    // Centred on what the camera LOOKS at (`segment::look_centre`), not its own nadir:
                                                                    // the descent looks AHEAD along its path, so centring under the eye would spend the
                                                                    // fine rings on ground behind it. Measured as jagged biome edges at the corridor's
                                                                    // mid-stations before this.
                    let fwd_body = (q_inv * ((aim_w - eye_w) * ds))
                        .normalize_or(-eye_body.normalize_or(glam::DVec3::Y));
                    let dir_body = crate::terra::segment::look_centre(eye_body, fwd_body, r_draw);
                    let (lat, lon) = crate::geo::lat_lon_from_dir(dir_body);
                    let (up_b, north_b, east_b) = crate::geo::tangent_frame(lat, lon);
                    let h = (eye_body.length() - r_draw).max(1.0e-9);
                    let horizon = (h * (h + 2.0 * r_draw)).sqrt();
                    // **ONE surface** (docs/63): the extent is what is VISIBLE from here, and the globe
                    // is always skipped because there is no second mesh to blend with. No fade, and no
                    // depth-fight lift — both existed only to hold two copies of one surface apart.
                    let _ = horizon;
                    let angle = crate::terra::segment::visible_angle(
                        h / ds,
                        r_draw / ds,
                        crate::terra::ground_cap::CAP_MARGIN,
                    );
                    corridor_skip_globe = true;
                    let lift = 0.0;
                    let mut verts = std::mem::take(&mut self.cap_verts);
                    {
                        let sampler = crate::terra::globe_mesh::SurfaceSampler::new(
                            &self.mats,
                            &surf.biome_mix,
                            surf.landmask.as_ref(),
                            surf.elevation.as_ref(),
                            surf.landcover.as_ref(),
                            surf.elev_range,
                            ds,
                            surf.relief_exag,
                        )
                        // THIS SCENE'S clock — its declared epoch if the world names one, else now.
                        // A scene showing proto-Earth and a scene showing this afternoon are ONE Earth
                        // at two times, which is a scene's own business (docs/65). What "one Earth"
                        // forbids is two answers to *what Earth is made of*, not two dates.
                        .at_epoch(self.scene_epoch_s());
                        // The spin's oblate figure as a radial factor — first-order identical to
                        // the globe draw's affine scale about the spin axis (the mesh's y), so the
                        // cap sits on the same flattened surface the globe draws.
                        // NOTE (docs/63, and it is a real gap): this segment does NOT yet run the
                        // appearance integral Terra's does, so it carries `rough = 0` and shades as
                        // plain Lambert. Two scenes drawing one Earth differently is the very thing
                        // docs/63 exists to end — but the corridor's descent is not frame-reproducible,
                        // so a change here cannot be A/B'd until the fixed-pose rig (docs/63 item 1c)
                        // exists. Flagged in docs/46 rather than shipped unverified.
                        let sample = |dir: glam::DVec3| {
                            let p = sampler.sample(dir);
                            crate::terra::globe_mesh::SurfaceSample {
                                offset: p.offset
                                    + lift
                                    + r_draw * flat * (1.0 / 3.0 - dir.y * dir.y),
                                ..p
                            }
                        };
                        crate::terra::segment::fill_segment(
                            &mut verts,
                            up_b,
                            east_b,
                            north_b,
                            eye_body,
                            r_draw,
                            angle,
                            crate::terra::segment::SegmentRes::new(SEG_RINGS, SEG_SPOKES),
                            sample,
                        );
                    }
                    self.queue.write_buffer(
                        &self.cap_gpu.vertex_buf,
                        0,
                        bytemuck::cast_slice(&verts),
                    );
                    self.cap_verts = verts;
                    // Drawn through the GLOBE'S OWN conventions — the same focus-relative
                    // view_proj, the same spin rotation in the model, the same world-frame light
                    // and anchor — so the cap differs from the globe by sampling density alone
                    // and the cross-fade cannot shade one point two ways. The vertices are
                    // eye-relative in the body frame (subtracted in f64); the model adds the eye
                    // back as a translation built in f64 and cast once, so the absolute-f32
                    // residual stays inside the sub-pixel bound the arc floor itself is licensed
                    // by (see crate::arc's floor test).
                    let spin_m = Mat4::from_quat(glam::Quat::from_xyzw(
                        spin_rot.x as f32,
                        spin_rot.y as f32,
                        spin_rot.z as f32,
                        spin_rot.w as f32,
                    ));
                    let eye_spos = ((eye_w - focus) * ds).as_vec3();
                    write_space_uniform(
                        &self.queue,
                        &self.cap_uni,
                        view_proj,
                        Mat4::from_translation(eye_spos) * spin_m,
                        earth_light,
                        // Opaque: the cross-fade alpha went with the mesh it was fading against.
                        // `pretty_fade` stays — that is the docs/42 pretty-render blend, a different
                        // thing entirely from the globe/cap hand-off.
                        [1.0, 1.0, 1.0, pretty_fade],
                        [eye_disp.x as f32, eye_disp.y as f32, eye_disp.z as f32, 0.0],
                        self.air(),
                        glow_of(&crate::planet::body("earth")),
                    );
                    corridor_cap = true;
                }
            }
            for (i, uni) in self.shell_unis.iter().enumerate() {
                let body_dir = crate::impact::fib_dir(i, SHELL_N); // this grain's fixed BODY direction
                let dir = spin_rot * body_dir; // its current WORLD direction (rotated by the spin)
                let u = dir.dot(spin_axis);
                let r_oblate =
                    (pretty_r_surf - 0.62 * shell_spacing) * (1.0 + flat * (1.0 / 3.0 - u * u)); // +f/3 equator, −2f/3 poles
                let pos_w = earth_center + dir * r_oblate;
                let hidden = crater_site.map_or(false, |s| (pos_w - s).length() < crater_r);
                let scale = if hidden { 0.0 } else { shell_grain_r }; // zero-scale ⇒ not drawn
                let spos = ((pos_w - focus) * pretty_scale).as_vec3();
                // Continents & oceans (docs/25): each grain samples the landmask at its fixed BODY direction
                // — so a continent is a property of the CRUST and CO-ROTATES with the planet (and with the
                // crater), rather than being painted world-fixed while the grains slide underneath. "Average
                // area particles": the grain is the mean of its ~10°×10° patch, nothing painted.
                let surf = crate::planet::earth_surface_material(body_dir);
                let m = &self.mats[materials::index_of(&self.mats, surf)];
                // RAYLEIGH (docs/26): the declared air scatters sunlight over this patch — a blue
                // veil (into the emissive channel: it IS added light) whose ground shows through
                // slightly reddened (two-way transmittance). All from the emergent pressure; an
                // airless world renders colorless by the same code.
                let v_dir = (eye_disp - (pos_w - focus) * display_scale()).normalize_or_zero();
                let mu_v = dir.dot(v_dir);
                let mu_s = dir.dot(sun_dir_earth);
                let cos_th = v_dir.dot(sun_dir_earth);
                let veil = crate::atmosphere::rayleigh_veil(
                    mu_v,
                    mu_s,
                    cos_th,
                    atm_tau_eff,
                    crate::atmosphere::SUN_GAIN as f64,
                    self.atm_twilight,
                );
                let tr = crate::atmosphere::rayleigh_transmit(mu_v, mu_s, atm_tau_eff);
                let tint = [
                    m.albedo[0] * tr[0],
                    m.albedo[1] * tr[1],
                    m.albedo[2] * tr[2],
                    1.0,
                ];
                write_space_uniform(
                    &self.queue,
                    uni,
                    view_proj,
                    Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(scale)),
                    earth_light,
                    tint,
                    [veil[0], veil[1], veil[2], 1.0], // the sky, added over the ground,
                    AIRLESS,
                    NO_GLOW,
                );
            }
            // THE SUN: real matter (planet::sun), rendered where it actually is — a ~0.5° disk of
            // photosphere-temperature plasma (5,772 K → white, via the same incandescence law as hot
            // rock). It enters frame whenever the camera looks sunward — opposition geometry included —
            // because it is drawn at its position, not painted on a skybox.
            {
                let spos = ((r_bodies[0] - focus) * display_scale()).as_vec3();
                // Radius and photosphere temperature from the SUN'S OWN DEFINITION (assets/bodies/sun.json),
                // not repeated here. The ~0.53° disk seen from Earth is then emergent — it is a real sphere
                // of a real size at a real distance, so it grows on approach and shrinks from Mars without
                // anyone writing an angle down.
                let sun = crate::planet::body("sun");
                let sun_r_disp = (sun.radius() * display_scale()) as f32;
                // Colour from the declared photosphere temperature through the SAME incandescence law that
                // makes hot rock glow — a star is not a special case, it is matter at a temperature. At the
                // Sun's 5,772 K that lands on white, which is what the Sun actually looks like from space
                // (its blackbody peak is green; the integral is white — the yellow sun is an atmospheric
                // effect, seen from under the air). A cooler star now renders red WITHOUT new code.
                let photosphere = sun.layers.last().map(|l| l.t_outer).unwrap_or(5772.0);
                let glow = incandescence(photosphere as f32);
                write_space_uniform(
                    &self.queue,
                    &self.sun_uni,
                    view_proj,
                    Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(sun_r_disp)),
                    earth_light,
                    [0.0, 0.0, 0.0, 1.0], // no reflectance — it is the illuminant
                    // The photosphere's radiance against the same reference every other glowing surface
                    // uses. This was the literal 4.6e4, worked out by hand from ~2e7 vs ~430 W/m²/sr —
                    // and `thermal_glow_gain(5772)` returns 46,530, so the law reproduces the number that
                    // was measured. One Stefan–Boltzmann for the Sun and for a magma ocean; any exposure
                    // set for sunlit surfaces saturates on both, which is what looking at them is like.
                    [
                        glow[0],
                        glow[1],
                        glow[2],
                        crate::blackbody::thermal_glow_gain(photosphere) as f32,
                    ],
                    AIRLESS,
                    NO_GLOW,
                );
            }
            // The BULK INTERIOR (the un-materialized deep Earth): an opaque sphere at the depth the
            // crater exposes — the top of the outer core — glowing at its real temperature (docs/25).
            // The planet is not hollow; through the crater you see molten interior, not far-side crust.
            {
                let ipos = ((earth_center - focus) * pretty_scale).as_vec3();
                // The interior must wear the SAME oblate figure as the shell, else at the post-impact
                // ~13% flattening the poles sink below a perfect 0.985 R sphere and the interior pokes
                // OUT through the crust at both poles (a render-truth bug). Ellipsoid: equator +f/3,
                // poles −2f/3 about the spin axis — one non-uniform scale, oriented to the spin axis.
                // docs/42: sized to the sub-scale body + faded with the blend while the GPU impact is live.
                let ir = (pretty_r_surf * 0.985) * pretty_scale * pretty_fade as f64;
                let ir_eq = (ir * (1.0 + flat / 3.0)) as f32;
                let ir_pol = (ir * (1.0 - 2.0 * flat / 3.0)) as f32;
                let align = glam::DQuat::from_rotation_arc(glam::DVec3::Z, spin_axis);
                // During a WHOLE-BODY giant impact the exposed interior is a MAGMA ocean (docs/42 Phase 2): a
                // hot self-lit orange ramping up as the crater opens, so the crater reads as a molten
                // post-impact Earth. A CAP impact (docs/39) leaves modern Earth's bulk intact and un-exposed —
                // its cool declared interior, not a global magma ocean (only the cap particles are hot).
                let (itint, iglow) = if self.sph_active && self.sph_cap.is_none() {
                    let g = 0.6 + 2.4 * self.gpu_crater_frac as f32; // brighter as the shock excavates
                    ([0.20, 0.09, 0.05, 1.0], [1.0, 0.42, 0.12, g])
                } else {
                    (self.interior_tint, self.interior_glow)
                };
                write_space_uniform(
                    &self.queue,
                    &self.interior_uni,
                    view_proj,
                    Mat4::from_translation(ipos)
                        * Mat4::from_quat(align.as_quat())
                        * Mat4::from_scale(Vec3::new(ir_eq, ir_eq, ir_pol)),
                    earth_light,
                    itint,
                    iglow, // outer-core iron: self-lit at its real temperature (magma while impacting),
                    AIRLESS,
                    NO_GLOW,
                );
            }
            // docs/42 Phase 4 — accreting MOONLET spheres: self-bound disk clumps resolve out of the ejecta into
            // growing rock spheres (borrowing the debris uni pool, unused while the GPU impact runs). Warm-tinted
            // — freshly accreted, still cooling. They grow as the clump gathers mass; the largest is the Moon.
            let n_moonlets =
                if self.sph_active && pretty_fade > 0.0 && !self.sph_snapshot.is_empty() {
                    let bodies = crate::gpu_sph::moonlet_bodies(&self.sph_snapshot);
                    let n = bodies.len().min(self.debris_unis.len());
                    for (uni, &(com_pos, radius, _mass)) in
                        self.debris_unis.iter().zip(bodies.iter()).take(n)
                    {
                        let spos = ((earth_center + com_pos - focus) * pretty_scale).as_vec3();
                        let r_disp = (radius * pretty_scale * 1.6) as f32 * pretty_fade;
                        write_space_uniform(
                            &self.queue,
                            uni,
                            view_proj,
                            Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(r_disp)),
                            earth_light,
                            [0.45, 0.34, 0.28, 1.0], // cooling rock
                            [1.0, 0.55, 0.25, 0.5],  // a faint warm glow — recently molten,
                            AIRLESS,
                            NO_GLOW,
                        );
                    }
                    n
                } else {
                    0
                };
            // **MOONS ARE SOLID BODIES.** Each intact moon is ONE sphere at its position, and it stops
            // being drawn the moment it has struck Earth, because its matter is the planet's (or the
            // SPH field's) then, not a shell. What stood here drew each moon as 128 grain billboards, so a
            // moon "dropped as particles, not a sphere"; and the shatter-hide only ever caught moon 0,
            // so a second moon left an empty shell hanging on Earth while only
            // the first appeared to fragment. One rule, every moon.
            // Each moon is drawn from ITS OWN metadata — its own radius, its own tint, its own
            // materialised state. One sphere per moon (the first pool slot); the rest of the pool is
            // zeroed. A moon that has struck is not drawn at all: its matter is the debris field now.
            for (idx, uni) in self.moon_unis.iter().enumerate() {
                let k = idx / MOON_SHELL_N;
                let bi = 2 + k; // body index of this moon
                let meta = self.body_meta.get(bi);
                let visible = idx % MOON_SHELL_N == 0
                    && meta.map_or(false, |m| m.role == BodyRole::Moon && !m.materialized);
                if !visible {
                    write_space_uniform(
                        &self.queue,
                        uni,
                        view_proj,
                        Mat4::from_scale(Vec3::ZERO),
                        earth_light,
                        [0.0; 4],
                        [0.0; 4],
                        AIRLESS,
                        NO_GLOW,
                    );
                    continue;
                }
                let m = meta.unwrap();
                let mpos = ((r_bodies[bi] - focus) * display_scale()).as_vec3();
                let mr = (m.radius_m * display_scale()) as f32;
                let mlight = (sun - r_bodies[bi]).as_vec3().normalize();
                write_space_uniform(
                    &self.queue,
                    uni,
                    view_proj,
                    Mat4::from_translation(mpos) * Mat4::from_scale(Vec3::splat(mr)),
                    mlight,
                    m.tint,   // this moon's own reflectance
                    [0.0; 4], // intact body: reflected light only (its hot core is buried)
                    AIRLESS,
                    NO_GLOW,
                );
            }
            // GEOLOGIC moonlets: one grain ball per body at its true orbital radius. Orbital PHASE is
            // unresolvable at millennia-per-second (a moonlet completes ~10⁶ orbits per frame), so the
            // drawn angle is a slow golden-spaced drift — a liveliness cue, honestly not a phase.
            let mut debris_count = 0usize;
            if self.geologic {
                let rho = 2_900.0f64; // basalt bulk — the moonlets' crusts have long frozen (docs/27)
                for (i, m) in self.geo_moonlets.iter().enumerate() {
                    if i >= self.debris_unis.len() {
                        break;
                    }
                    let ang = 2.399963 * i as f64 + self.phys_clock * 0.15;
                    let dir = glam::DVec3::new(ang.cos(), ang.sin(), 0.0);
                    let pos_w = earth_center + dir * m.a;
                    let r_disp = ((3.0 * m.mass / (4.0 * std::f64::consts::PI * rho)).cbrt()
                        * display_scale()) as f32;
                    let fpos = ((pos_w - focus) * display_scale()).as_vec3();
                    let flight = (sun - pos_w).as_vec3().normalize();
                    write_space_uniform(
                        &self.queue,
                        &self.debris_unis[i],
                        view_proj,
                        Mat4::from_translation(fpos) * Mat4::from_scale(Vec3::splat(r_disp)),
                        flight,
                        self.moon_tint,
                        [0.0; 4], // crusted over: reflected light only (interior heat is sub-surface),
                        AIRLESS,
                        NO_GLOW,
                    );
                    debris_count += 1;
                }
            }

            let output = self
                .surface
                .get_current_texture()
                .map_err(|e| JsValue::from_str(&format!("get_current_texture failed: {e}")))?;
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("orbit-frame"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("orbit-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.01,
                                g: 0.01,
                                b: 0.03,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                // THE SKY FIRST. Real stars at real positions; everything else paints over them. This
                // band's world frame is inertial, so the catalogue's own frame needs no rotation.
                if let Some(stars) = self.stars.as_ref() {
                    // The eye's REAL position relative to Sol, in parsecs — this is what makes the sky
                    // honest rather than a shell. Across the whole solar system it is ~1e-4 pc, so the
                    // parallax is correctly invisible; the machinery is the same one that would show a
                    // different sky from another star.
                    const PC_M: f64 = 3.085_677_581e16;
                    let eye_from_sol = (focus + eye_disp / display_scale()) - r_bodies[0];
                    let cam_pc = (eye_from_sol / PC_M).as_vec3();
                    stars.draw(
                        &self.queue,
                        &mut pass,
                        view_proj,
                        Mat4::IDENTITY,
                        eye_disp.as_vec3(),
                        cam_pc,
                        50_000.0,
                        self.config.width as f32,
                        self.config.height as f32,
                        80.0,
                    );
                }
                pass.set_pipeline(&self.pipeline);
                draw(&mut pass, &self.sun_uni, &self.sphere_gpu); // the Sun, where it really is
                                                                  // The rigid-Earth + moon spheres draw only when the GPU SPH impact is NOT running
                                                                  // (docs/33 stage 4c.4): with the deformable impact active, the particle field IS the planet.
                if !self.sph_active {
                    // Every moon's sphere slot; the write pass above zero-scaled the ones that have
                    // struck (invisible) and the unused pool entries, so no per-moon special case here.
                    for uni in self.moon_unis.iter() {
                        draw(&mut pass, uni, &self.sphere_gpu);
                    }
                    // Geologic moonlets (the only debris_unis writer outside the SPH moonlet pass).
                    for uni in self.debris_unis.iter().take(debris_count) {
                        draw(&mut pass, uni, &self.sphere_gpu);
                    }
                }
                // The pretty Earth shell (docs/42): the CPU scene always; the GPU-impact scene whenever the blend
                // isn't fully at the physics end (its grains were sized to the SPH body + faded by 1−blend above,
                // so they overlay the particle field and cross-fade to it).
                if !self.sph_active || self.render_blend < 1.0 {
                    // the glowing deep interior first (shows through the crater), then the crust shell over it
                    draw(&mut pass, &self.interior_uni, &self.sphere_gpu);
                    // **EARTH.** The definitive body — the same globe mesh, from the same shared builder,
                    // that Terra renders. What stood here was a 512-grain shell: a stand-in that made the
                    // planet look warty and, being its own render path, quietly gave this scene a different
                    // Earth from the one next door. Excavation is not this mesh's job — the impact resolves
                    // real particles at the impact site, and THEY are the matter (and the crater) there.
                    if let Some(globe) = self.globe_mesh.as_ref() {
                        // Skipped once the corridor cap is fully faded in and covers the view out
                        // past the horizon (ground_cap::cap_covers_view — the same rule Terra
                        // skips its globe by): the depth buffer cannot separate two copies of one
                        // surface in the final metres.
                        if !corridor_skip_globe {
                            pass.set_pipeline(&self.globe_pipeline);
                            draw(&mut pass, &self.globe_uni, globe);
                        }
                        pass.set_pipeline(&self.pipeline);
                        // The impactor, as the body it still is. Zero-alpha when it has come apart, at
                        // which point its particles are the matter and are already being drawn.
                        if impactor_fade > 0.0 {
                            draw(&mut pass, &self.impactor_uni, &self.sphere_gpu);
                        }
                    } else {
                        for uni in self.shell_unis.iter() {
                            draw(&mut pass, uni, &self.sphere_gpu); // no surface handed over yet
                        }
                    }
                    // accreting moonlet spheres (docs/42 Phase 4), from the disk's self-bound clumps
                    for uni in self.debris_unis.iter().take(n_moonlets) {
                        draw(&mut pass, uni, &self.sphere_gpu);
                    }
                }
                // The descent corridor's ground cap (built above): alpha-blended over the globe
                // through the fade band, the whole foreground below it. Drawn before the site's
                // billboards so the fine matter at the site depth-tests against the real ground.
                if corridor_cap {
                    pass.set_pipeline(&self.cap_pipeline);
                    draw(&mut pass, &self.cap_uni, &self.cap_gpu);
                }
                // GPU SPH particles: instanced billboards straight from the physics buffer (zero-copy).
                // Particles are drawn only once they ARE the matter. During the relax and the approach the
                // field exists — settled and waiting — but neither body has been touched, so drawing it
                // put a scatter of specks over two perfectly whole worlds.
                if self.sph_active && matches!(self.sph_phase, SphPhase::Dynamics) {
                    if let Some(sph) = self.gpu_sph.as_ref() {
                        if sph.count() > 0 {
                            pass.set_pipeline(&self.sph_pipeline);
                            pass.set_bind_group(0, &self.sph_cam.bind, &[]);
                            pass.set_vertex_buffer(0, sph.particle_buffer().slice(..));
                            pass.draw(0..6, 0..sph.count());
                        }
                    }
                }
                // docs/59 - the materialized site's fine matter (the declared ball and its
                // ground patch), instanced through the SAME billboard pipeline: one particle
                // representation, wherever fine matter comes from.
                if site_count > 0 {
                    if let Some(buf) = self.site_buf.as_ref() {
                        pass.set_pipeline(&self.sph_pipeline);
                        pass.set_bind_group(0, &self.site_cam.bind, &[]);
                        pass.set_vertex_buffer(0, buf.slice(..));
                        pass.draw(0..6, 0..site_count);
                    }
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        /// World metres spanned by one screen pixel at the focus body (the look target sits at the
        /// display origin, so the focal distance is exactly `base_distance·zoom` display units).
        /// Display units are metres·display_scale(), so divide back out to report a true metres/pixel -
        /// which the HUD renders as a km/AU scale bar. Honest live read of camera state; feeds the
        /// same scale bar as the terrain scene through `metres_per_pixel_at`.
        pub fn meters_per_pixel(&self) -> f64 {
            // While the arc drives, the focal distance is its camera-to-site distance; the HUD
            // scale bar then reads honestly all the way down to the surface framing. (Sean, upstream-8.)
            let dist_m = match self.arc.as_ref() {
                Some(a) => a.d_m,
                None => (self.camera.base_distance * self.camera.zoom) as f64 / display_scale(),
            };
            // SPACE_FOV_Y, not a bare 0.9 — the scale bar and the projection must read ONE field of view
            // (PR #88). His branch predates that fix, so the literal comes back on every merge; it is the
            // half `laws::fov_tests` cannot see, because this is not a `perspective_rh` call.
            crate::metres_per_pixel_at(dist_m, SPACE_FOV_Y as f64, self.config.height.max(1) as f64)
        }

        fn view_proj(&self) -> Mat4 {
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            let proj = Mat4::perspective_rh(SPACE_FOV_Y, aspect, 0.05, 100_000.0);
            // The look target is the focus body (display origin) plus the user's pan offset; the
            // eye keeps its orbit distance and angles around that target, so rotate/zoom and pan
            // compose without either changing the other's meaning.
            let target = self.camera.pan;
            let eye =
                self.camera.eye_dir() * (self.camera.base_distance * self.camera.zoom) + target;
            let view = Mat4::look_at_rh(eye, target, Vec3::Y);
            proj * view
        }
    }

    /// Generate + upload the material albedo and NORMAL arrays, returning views and a shared sampler.
    /// One function, so every surface in the engine samples the same texture set (Law II).
    fn upload_material_textures(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mats: &[materials::Material],
    ) -> (wgpu::TextureView, wgpu::TextureView, wgpu::Sampler) {
        let textures = texture::generate_all(mats);
        let (n_layers, mip_count) = (textures.len() as u32, textures[0].mips.len() as u32);
        let mk = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: texture::TEX_SIZE as u32,
                    height: texture::TEX_SIZE as u32,
                    depth_or_array_layers: n_layers,
                },
                mip_level_count: mip_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let (albedo_tex, normal_tex) = (mk("material-textures"), mk("material-normals"));
        for (layer, t) in textures.iter().enumerate() {
            for (which, mips) in [(&albedo_tex, &t.mips), (&normal_tex, &t.normal_mips)] {
                for (mip, data) in mips.iter().enumerate() {
                    let msize = (texture::TEX_SIZE >> mip) as u32;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: which,
                            mip_level: mip as u32,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: layer as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * msize),
                            rows_per_image: Some(msize),
                        },
                        wgpu::Extent3d {
                            width: msize,
                            height: msize,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
        let view = |t: &wgpu::Texture| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        (view(&albedo_tex), view(&normal_tex), sampler)
    }

    /// `surfaces` is `None` for scenes whose layout is the uniform alone (the space band draws bodies,
    /// not textured surfaces) and `Some` where the shader samples material relief. One function rather
    /// than two near-identical ones that would drift.
    /// A uniform slot on THE surface bind layout — the same one every scene uses.
    fn make_space_uniform(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        tex: &wgpu::TextureView,
        normal: &wgpu::TextureView,
        samp: &wgpu::Sampler,
    ) -> UniformSlot {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("space-uniform"),
            size: std::mem::size_of::<SpaceUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("space-bind"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(tex),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(samp),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(normal),
                },
            ],
        });
        UniformSlot { buf, bind }
    }

    /// A body's AIR, as the shaders want it: `[tau_r, tau_g, tau_b, exposure, twilight_half_angle]`.
    /// One value carries the whole atmosphere so that adding a property does not mean a new argument at
    /// every draw call — `write_space_uniform` unpacks it into the two uniform slots that hold it.
    type Air = [f32; 5];

    /// A body with NO declared atmosphere: zero optical depth and zero twilight. The shared Rayleigh
    /// model then returns exactly black with a knife-edge terminator — the airless Moon needs no special
    /// case, it just has no air.
    /// A body that does not glow — anything below visible incandescence.
    const NO_GLOW: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

    const AIRLESS: Air = [0.0, 0.0, 0.0, crate::atmosphere::SUN_GAIN, 0.0];

    /// What `camera_follow` is riding. An assembly the engine can point at — today that is anything in
    /// flight; when assemblies carry identity (docs/64) this is where a gun or a ship joins the list.
    #[derive(Clone, Copy, PartialEq)]
    enum RideSubject {
        /// Whichever body in flight is heaviest right now — survives a fragmenting subject.
        Heaviest,
        /// One body, by the engine's own id.
        Body(u64),
    }

    /// A seat on a moving actor: which actor, where you sit in ITS frame, and where you look.
    #[derive(Clone, Copy)]
    struct Ride {
        subject: RideSubject,
        back_m: f64,
        up_m: f64,
        side_m: f64,
        yaw: f64,
        pitch: f64,
    }

    impl Terra {
        /// This world's air, as the shared Rayleigh model wants it: the optical depth derived from the
        /// DECLARED atmosphere's mass (a world never declares τ, and never declares surface pressure —
        /// both fall out of the air's own weight), at the one canonical exposure. A world with no
        /// atmosphere yields zeros here and renders with a hard terminator, correctly.
        fn air(&self) -> Air {
            [
                self.atm_tau[0] as f32,
                self.atm_tau[1] as f32,
                self.atm_tau[2] as f32,
                crate::atmosphere::SUN_GAIN,
                self.atm_twilight as f32,
            ]
        }
    }

    /// The twilight half-angle for a body: sqrt(2H/R), with H the scale height of the DECLARED air at
    /// this body's own surface gravity. No air ⇒ no twilight ⇒ a knife-edge terminator.
    /// A body's own thermal glow, ready for the uniform: the Planck colour of its declared surface
    /// temperature and the Stefan-Boltzmann radiance that goes with it. This is the whole of what the
    /// renderer needs to draw a magma ocean — the heat, not a picture of the heat.
    fn glow_of(body: &crate::planet::LayeredBody) -> [f32; 4] {
        let t = body.layers.last().map_or(0.0, |l| l.t_outer);
        let gain = crate::blackbody::thermal_glow_gain(t);
        if gain <= 0.0 {
            return NO_GLOW;
        }
        let c = crate::blackbody::blackbody_srgb(t);
        [c[0], c[1], c[2], gain as f32]
    }

    fn twilight_of(radius_m: f64, g: f64, mats: &[materials::Material], tau: [f64; 3]) -> f64 {
        if tau[2] <= 0.0 {
            return 0.0;
        }
        let h = mats
            .iter()
            .find(|m| m.id == "air")
            .map(|air| crate::atmosphere::scale_height(air, 288.0, g))
            .unwrap_or(0.0);
        crate::atmosphere::twilight_half_angle(h, radius_m)
    }

    /// Earth's DEFINITIVE surface, handed over by the host once and reused by every draw. The scene
    /// does not own these continents — the body definition does; this is just the decoded copy.
    /// The one-line conservation audit of a materialized site for the HUD (docs/59: the ledger
    /// is surfaced, not implied): mass in and out, the release state or its stated residual,
    /// the angular-momentum drift against the relax's own bound, and where the thermal state
    /// came from.
    fn site_audit_line(site: &crate::site::MaterializedSite) -> String {
        let l = &site.ledger;
        let release = match site.release {
            crate::site::SiteRelease::Released(r) => format!(
                "released {:.1e} (bound {:.1e}{}) in {} iters",
                r.released_max_density_error,
                r.release_bound,
                if r.release_bound > crate::refine::RELEASE_DENSITY_ERROR {
                    ", the field's own scale mismatch at the stall"
                } else {
                    ""
                },
                r.iterations
            ),
            crate::site::SiteRelease::Unreleased {
                achieved,
                bound,
                iterations,
            } => format!(
                "UNRELEASED: exact split, density residual {achieved:.1e} over the {bound:.0e} \
                 bound after {iterations} iters (static site; the release gates dynamics)"
            ),
        };
        let hand = match site.hand_down {
            crate::site::HandDown::Declared => String::from("state from the definition"),
            crate::site::HandDown::Sampled {
                u_j_kg,
                peak_speed_ms,
                quiescent_speed_ms,
            } => {
                format!(
                    "u sampled from the quiet field: {u_j_kg:.2e} J/kg (peak {peak_speed_ms:.0} \
                     < v_q {quiescent_speed_ms:.0} m/s)"
                )
            }
        };
        let n_fine = site.particles.len() - site.fine_start;
        let am_drift = (l.after_relax.angular_momentum - l.before.angular_momentum).length();
        format!(
            "SITE MATERIALIZED: ball {} + patch {} fine, {} coarse guards · mass {:.4e} kg in, \
             {:.4e} kg out · |dL| {:.1e} (bound {:.1e}) · {} · {}",
            site.ball_children,
            n_fine - site.ball_children,
            site.fine_start,
            l.before.mass,
            l.after_relax.mass,
            am_drift,
            l.relax_am_bound,
            release,
            hand
        )
    }

    struct EarthSurface {
        landmask: Option<crate::terra::raster::Raster>,
        elevation: Option<crate::terra::raster::Raster>,
        landcover: Option<crate::terra::raster::Raster>,
        biome_mix: Vec<Vec<(usize, f32)>>,
        elev_range: [f64; 2],
        relief_exag: f64,
    }

    fn write_space_uniform(
        queue: &wgpu::Queue,
        slot: &UniformSlot,
        view_proj: Mat4,
        model: Mat4,
        light: Vec3,
        tint: [f32; 4],
        emissive: [f32; 4],
        air: Air,
        glow: [f32; 4],
    ) {
        let u = SpaceUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            // .w = the twilight half-angle: how far past the geometric terminator the air is still lit.
            light_dir: [light.x, light.y, light.z, air[4]],
            tint,
            emissive,
            atm: [air[0], air[1], air[2], air[3]],
            glow,
            crater: [0.0; 4],
            crater2: [0.0; 4],
        };
        queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&u));
    }

    /// Blackbody-ish incandescence for a material at temperature `temp` (K): a self-emissive glow colour
    /// (rgb) and intensity (w), ramping dark→red→orange→yellow→white as rock heats past ~800 K. This is
    /// the visual "for free" from the thermal state — the render just reads the fragment's real temperature.
    fn incandescence(temp: f32) -> [f32; 4] {
        const GLOW_START: f32 = 800.0; // K — below this, rock shows no visible self-glow
        const WHITE_HOT: f32 = 3200.0; // K — ramp saturates to white here
        if temp <= GLOW_START {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let x = ((temp - GLOW_START) / (WHITE_HOT - GLOW_START)).clamp(0.0, 1.0);
        // Red saturates first, then green (→orange/yellow), then blue (→white) — a coarse Planckian locus.
        let r = (x * 2.5).clamp(0.0, 1.0);
        let g = ((x - 0.25) * 2.0).clamp(0.0, 1.0);
        let b = ((x - 0.55) * 2.2).clamp(0.0, 1.0);
        // Intensity grows with temperature so the hottest fragments read brightest (Stefan–Boltzmann-ish).
        let intensity = (0.4 + 1.6 * x) * (x.max(0.05));
        [r, g, b, intensity]
    }

    fn build_space_pipeline(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("space-shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../../../shaders/tonemap.wgsl"),
                    include_str!("../../../shaders/space.wgsl")
                )
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("space-pipeline-layout"),
            bind_group_layouts: &[bind_layout],
            push_constant_ranges: &[],
        });
        // Same vertex layout as the world mesh; the space shader only reads position + normal.
        const ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Uint32, 4 => Float32];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("space-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        })
    }

    /// docs/43 Phase 3 — the Terra globe pipeline: the same vertex layout + bind layout as the space pipeline,
    /// but `globe.wgsl` (per-vertex biome colour + a cheap atmospheric limb) instead of the flat-tint shader.
    /// `blend` is REPLACE for the opaque globe and alpha-blending for the ground cap's cross-fade (Phase 5).
    fn build_globe_pipeline(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        blend: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("globe-shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../../../shaders/tonemap.wgsl"),
                    include_str!("../../../shaders/rayleigh.wgsl"),
                    include_str!("../../../shaders/surface_normal.wgsl"),
                    include_str!("../../../shaders/globe.wgsl")
                )
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("globe-pipeline-layout"),
            bind_group_layouts: &[bind_layout],
            push_constant_ranges: &[],
        });
        const ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Uint32, 4 => Float32];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("globe-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // docs/43: NO culling — the fly camera looking down saw the front-facing globe triangles
                // culled (a growing black VOID at nadir on descent, the ~250 km bug). Convex globe → depth
                // alone occludes correctly; robust regardless of winding, extra fragments are cheap.
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        })
    }

    /// The instanced particle pipeline for the GPU SPH impact (docs/33 stage 4c.4). One camera-facing
    /// billboard quad per particle, generated in the vertex shader; the instance buffer is the `sph_step.wgsl`
    /// particle buffer itself (48-byte stride, pos at offset 0, provenance u32 at offset 44). No mesh, no
    /// per-vertex buffer — the quad corners come from the vertex index.
    fn build_sph_pipeline(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sph-render-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/sph_render.wgsl").into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sph-render-pipeline-layout"),
            bind_group_layouts: &[bind_layout],
            push_constant_ranges: &[],
        });
        // Instance-step layout over the SPH particle buffer: pos (vec3 @ 0) + provenance (u32 @ 44).
        const ATTRS: [wgpu::VertexAttribute; 2] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 44,
                shader_location: 1,
            },
        ];
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::gpu_sph::SphParticle>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sph-render-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[instance_layout],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // billboards always face the camera
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        })
    }

    // ---------------------------------------------------------------------------------------------
    // docs/43 — Terra: a planet/terrain scene built from a DATA "world" (the first worlds-as-data scene).
    // Phase 1 renders the Earth as the reused grain shell, recolored by the loaded world + its declared
    // atmosphere; later phases add the raster surface sampler, the displaced globe mesh, and the fly camera.
    // ---------------------------------------------------------------------------------------------
    /// Default relief exaggeration if a world doesn't declare one (`surface.relief_exaggeration`). 1.0 = true
    /// scale. The globe mesh, ground cap, and camera floor all read the world's value so they stay one surface.
    const TERRA_RELIEF_EXAG: f64 = 1.0;
    /// Ground-cap grid resolution per side (Phase 5). The vertex buffer is rebuilt each frame; the index buffer
    /// (fixed topology) is built once.
    /// **The space band's vertical field of view** (radians) — ONE definition, because it was written out
    /// twice: once building the projection and once in `meters_per_pixel`, the HUD's scale bar. Change one
    /// and the bar silently measures against a frustum the scene is not drawing, which is the quietest
    /// possible way for a readout to lie.
    ///
    /// Deliberately NOT shared with `fly_camera::DEFAULT_FOV_Y`, which is also 0.9. Two scenes asking "what
    /// field of view do I render at" are two questions that happen to have the same answer today; coupling
    /// them would assert that one scene's framing must follow another's. (Same trap as `MOONLET_UNIS_N` —
    /// see `merge-reports/2026-07-25-sean-reid.md`.)
    const SPACE_FOV_Y: f32 = 0.9;

    /// Terra's segment resolution is the SHARED one (`SEG_RINGS`/`SEG_SPOKES`) — two scenes drawing the
    /// same planet at two densities would be two Earths again, in the one place that is easiest to miss.
    const TERRA_SEG_RINGS: usize = SEG_RINGS;
    const TERRA_SEG_SPOKES: usize = SEG_SPOKES;
    /// How far past the horizon the segment reaches, so its rim is never a visible edge. Same job, and
    /// same value, as the cap's own margin — the geometry changed, the reason did not.
    const TERRA_SEG_MARGIN: f64 = 1.3;
    /// **How many octaves of generated relief a vertex may sum.** Set generously, because MEASUREMENT says
    /// it is nearly free — the octaves were not the cost, and assuming they were is the mistake this number
    /// records. Priced at 2 km over the Himalaya, paced to ~60 fps (`web/rig/terra_lod_cost.mjs`):
    ///
    /// ```text
    /// tiers  octaves   p50 frame
    ///   1      0        45.2 ms   <- the ladder as it shipped, before any of this
    ///   1      2        51.2 ms
    ///   1      6        50.1 ms
    ///   1     15        51.6 ms   <- fifteen octaves cost 14% over zero
    ///   2      4       126.5 ms
    ///   3      4       159.2 ms
    ///   4      4       642.5 ms
    /// ```
    ///
    /// So generated relief is cheap and TIERS are expensive: what costs frame time is rebuilding and
    /// re-uploading a 192² camera-relative mesh, which the engine was already doing once per frame for
    /// **45 ms** before this work — a pre-existing cost nobody had measured. The mesh, not the maths, is the
    /// budget.
    const TERRA_OCTAVE_BUDGET: f64 = 16.0;
    /// (fixed topology) is built once. **One resolution for every scene's SEGMENT** — Terra's descent and
    /// the space band's corridor build the same surface, by the same builder, at the same density
    /// (docs/63). Rings out from under the eye, spokes around it.
    const SEG_RINGS: usize = 96;
    const SEG_SPOKES: usize = 192;
    /// The finest REAL feature the elevation raster carries (m). Detail below this is not in the data,
    /// so it must come from the material — see the micro-relief in the cap sample.
    const RASTER_FEATURE_M: f64 = 20_000.0;

    #[wasm_bindgen]
    pub struct Terra {
        /// The ONE resolution controller (docs/49). `alt_m` is the distance to the SURFACE, so it is the
        /// right distance for the ground cap specifically — anything else drawn in this scene must ask
        /// with its OWN distance, never reuse this one (the spacewalk rule in `surface_detail`).
        detail: crate::resolution::ResolutionController,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        depth_view: wgpu::TextureView,
        pipeline: wgpu::RenderPipeline,
        sphere_gpu: GpuMesh,
        shell_unis: Vec<UniformSlot>,
        shell_count: usize,
        // docs/43 Phase 3 — the displaced cube-sphere globe. Once a world with surface rasters loads, this
        // smooth mesh (land lifted by real elevation + biome-coloured, ocean cells at sea level with the water
        // material) replaces the grain shell for the scene. `None` until then (falls back to the grain shell).
        globe_pipeline: wgpu::RenderPipeline,
        /// WHICH defined body this scene placed. A scene positions a body and never defines one, but it
        /// does have to remember which one it put there — the render asks the definition for the body's
        /// own heat, and a magma-ocean world must not borrow modern Earth's daylit look.
        body_id: String,
        /// The real sky. Terra's world frame is Earth-FIXED, so the catalogue is rotated by Greenwich
        /// sidereal time each frame — which is what makes the stars wheel overhead, once per sidereal day.
        stars: Option<StarField>,
        // docs/43 Phase 5 — the fine, camera-relative ground cap (rebuilt each frame under the camera) + its
        // alpha-blend pipeline, and a reused CPU vertex scratch buffer. Cross-faded with the globe by altitude.
        /// The ground tiers, OUTERMOST first — one mesh and one uniform slot each, all built by the same
        /// `build_cap` from the same `sample`, because a second cap builder would be a second answer to
        /// "what does the ground look like here".
        /// **THE one surface geometry** (docs/63, `terra::segment`): a sphere segment whose angular radius
        /// follows the camera — a chord underfoot, a hemisphere from orbit. `None` until built. While
        /// `surface_mode` is 0 this is unused and the scene draws globe + cap exactly as before, so the two
        /// can be compared on the same frame before either is deleted.
        segment_gpu: Option<GpuMesh>,
        segment_uni: Option<UniformSlot>,
        /// **The cannon, as geometry derived from its assembly** — built once from
        /// `Assembly::mesh`, so the picture and the mass come from the same statement of what is there.
        /// `None` until the gun is emplaced.
        cannon_gpu: Option<GpuMesh>,
        cannon_uni: Option<UniformSlot>,
        /// Where the gun stands: (lat, lon, bearing). The scene's whole contribution — placement.
        cannon_at: Option<(f64, f64, f64)>,
        /// **The plants standing near the eye, as one mesh.** Rebuilt only when the ground under the
        /// camera has actually moved, the same rule the segment uses: re-derive when re-deriving would
        /// change something.
        flora_gpu: Option<GpuMesh>,
        flora_uni: Option<UniformSlot>,
        flora_kinds: Vec<crate::terra::flora::Kind>,
        /// Where the current flora mesh was built for, and how many plants are in it.
        flora_at: Option<(f64, f64, usize)>,
        /// Where the flora mesh's vertices are measured FROM (display units).
        flora_anchor: glam::DVec3,
        segment_verts: Vec<Vertex>,
        /// What the segment's mesh currently HOLDS — the same cache-of-the-view rule the ground tiers use
        /// (`ground_cap::tier_is_current`), on the one mesh instead of a ladder of them. The LIFT term is
        /// 0 here and always will be: a depth-fight allowance exists only to separate two meshes drawn
        /// over each other, and there is one.
        segment_built: Option<crate::terra::ground_cap::CapTierBuild>,
        /// Whether a world with real surface rasters has loaded — the signal that used to be carried by
        /// `globe_mesh.is_some()`, back when a globe mesh existed to ask about. Below it, the Phase-2
        /// grain shell stands in for a body with no surface data.
        surface_loaded: bool,
        /// **Measured elevation streamed by necessity** (`terra::tiles`, docs/46 row 27). The shipped
        /// global raster is 19.5 km per texel, which is why the ground goes flat below ~20 km altitude;
        /// this holds the metres-per-pixel tiles for the patch the camera is actually over. Empty until a
        /// host feeds it, and the scene renders exactly as before when it is empty.
        tiles: crate::terra::tiles::TileStore,
        /// The instant the SKY is drawn for, when something has pinned it — see `celestial_epoch_s`.
        /// `None` means the wall clock, which is the shipping behaviour.
        epoch_s: Option<f64>,
        relief_exag: f64,
        mats: Vec<materials::Material>,
        fly: crate::terra::fly_camera::FlyCamera,
        planet_radius: f64,
        atm_tau: [f64; 3],
        atm_twilight: f64,
        world_name: String,
        // docs/43 Phase 2 — the baked surface rasters (land mask, elevation+bathymetry, land-cover biome) and
        // the biome-index → material-index map. `None` until a world with surface rasters is loaded.
        landmask: Option<crate::terra::raster::Raster>,
        elevation: Option<crate::terra::raster::Raster>,
        landcover: Option<crate::terra::raster::Raster>,
        elev_range: [f64; 2],
        biome_mix: Vec<Vec<(usize, f32)>>, // land-cover class → material mixture
        // **Matter in flight — the ENGINE's operation, not a Terra feature** (docs/59). Terra's whole
        // contribution is the button that declares initial conditions and the draw that presents the
        // result; everything between is `flight::Flight` running the same code the ground patch runs.
        flight: crate::flight::Flight,
        /// Shots fired from this scene's cannon — a counter for the HUD and the rig, not physics.
        cannon_shots: u32,
        /// Wall-clock stamp of the last frame, so the flight advances in real seconds.
        last_frame_s: f64,
        /// The scene-agnostic renderer for whatever the engine is holding (`render::MatterField`).
        matter: MatterField,
        /// **A camera pose supplied from OUTSIDE**, if anything is driving one (Robin: feed the engine
        /// coordinates and FOV; let something else — another thread — decide position and framing). When
        /// set it is the authority for this frame: `fly` is synced from it so the HUD, the ground cap and
        /// the LOD blend all still read one altitude, and `None` hands control back to the fly camera.
        cam_pose: Option<([f64; 3], [f64; 3], [f64; 3], f64, f64)>, // eye, forward, up, fov_y, alt_m
        /// Where the eye actually WAS last frame, metres from the planet centre — the start of
        /// the shell's swept resolve, so a fast camera cannot tunnel through the surface skin.
        last_eye_m: Option<glam::DVec3>,
        /// What the camera is riding, if anything (`camera_follow`).
        ride: Option<Ride>,
        /// Rig knobs for PRICING the ground ladder: how many tiers to build, and how many octaves of
        /// generated relief each vertex may sum. Both cost frame time and both buy detail, and the only way
        /// to know the exchange rate is to move one at a time (gpu-perf §5).
        cap_octave_budget: f64,
        /// **How many sub-samples the appearance integral may take per mesh cell, on THIS machine**
        /// (docs/63, `resolution::WorkBudget`). Robin's rule: *"budget for textels/etc in engine should
        /// scale based on compute/GPU capability … naturally degrading on slower systems, built-in
        /// future-proofing for future platforms."* So it is measured from the time a rebuild actually
        /// took rather than read off a table of device names, and it stops growing on its own once the
        /// grid is as fine as the elevation data underneath it — at which point the integral is complete
        /// and more samples re-read the same numbers.
        appearance_budget: crate::resolution::WorkBudget,
        /// Rig override for the above: non-zero pins the grid side so the stage can be priced.
        appearance_probes_pinned: usize,
        /// Diagnostic: skip UPLOADING and DRAWING the engine's matter while still simulating it. The
        /// physics and the render path both scale with the same number, so measuring either one alone is
        /// impossible without a knob that moves one and not the other (the gpu-perf rule: price a stage,
        /// do not delete it and re-time the whole). A rig switch, not a feature.
        draw_matter: u32, // 0 = neither, 1 = upload only, 2 = upload + draw
        /// Reused per-frame scratch for the engine's matter: a render loop that allocates and frees these
        /// every frame churns megabytes a second for nothing.
        drawn_buf: Vec<Drawn>,
        inst_buf: Vec<GpuParticle>,
        /// The placed body's own matter and air, resolved ONCE. `planet::body()` deserializes its JSON on
        /// every call, and the flight step was calling it every frame — rebuilding the planet, in full,
        /// to ask what gravity is.
        flight_env: crate::flight::PlanetAir,
    }

    #[wasm_bindgen]
    impl Terra {
        /// Hand Terra the real star catalogue (`sky/stars.bin`) — the same asset the space band loads.
        /// EXPORTED here (a `#[wasm_bindgen]` impl) so JS can actually call it: it previously sat in a
        /// plain `impl Terra`, so `terra.load_star_catalog` was not a function and Terra's sky was empty.
        pub fn load_star_catalog(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            let stars = crate::sky::parse_catalog(bytes).map_err(|e| JsValue::from_str(&e))?;
            log::info!("sky: {} catalogued stars", stars.len());
            self.stars = Some(StarField::new(&self.device, self.config.format, &stars));
            Ok(())
        }

        pub async fn create(canvas: HtmlCanvasElement) -> Result<Terra, JsValue> {
            console_error_panic_hook::set_once();
            let _ = console_log::init_with_level(log::Level::Info);
            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..Default::default()
            });
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
                .map_err(|e| JsValue::from_str(&format!("create_surface failed: {e}")))?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .ok_or_else(|| JsValue::from_str("no suitable GPU adapter found"))?;
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("greenfield-terra"),
                        required_features: wgpu::Features::empty(),
                        required_limits: adapter.limits(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            let depth_view = create_depth_view(&device, width, height);
            // A LOW-poly grain sphere: with a fine shell (thousands of grains) each grain is tiny, so a coarse
            // sphere keeps the triangle + draw budget sane (the smooth displaced globe mesh arrives in Phase 3).
            let sphere_gpu = upload_mesh(
                &device,
                "terra-grain",
                &mesher::build_uv_sphere(1.0, 0, [1.0, 1.0, 1.0], 10, 14),
            );
            // Materials first: the surface layout and its textures are built FROM them.
            let mats = materials::load();
            // **One surface bind layout for every scene.** There is nothing special about the orbit
            // view: it is a camera position looking at the same rendered world, so it carries the same
            // material albedo + NORMAL arrays. Giving the space band a uniform-only layout was what made
            // "Earth in orbit" a differently-rendered object from "Earth underfoot".
            let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            };
            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("surface-bind-layout"),
                entries: &[
                    uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT),
                    tex_entry(1),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    tex_entry(4),
                ],
            });
            let (tex_view, normal_view, sampler) = upload_material_textures(&device, &queue, &mats);
            let pipeline = build_space_pipeline(&device, &bind_layout, config.format);
            let globe_pipeline = build_globe_pipeline(
                &device,
                &bind_layout,
                config.format,
                wgpu::BlendState::REPLACE,
            );
            let globe_uni =
                make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler);
            // The ground cap: same shader, alpha-blended for the cross-fade; a writable vertex buffer rebuilt each
            // frame, fixed index topology.
            // Same shader as the globe, alpha-blended for the cross-fade. (The merge had this calling
            // build_globe_pipeline with make_dynamic_mesh's arguments — the pipeline takes a bind layout
            // and a blend state; the label/count/indices belong to the mesh built just below.)
            // THE one surface (docs/63). Built here rather than lazily because this is where the bind
            // layout and the material texture views are in scope, which is the same reason the cap's are.
            let seg_res = crate::terra::segment::SegmentRes::new(TERRA_SEG_RINGS, TERRA_SEG_SPOKES);
            let segment_gpu = Some(make_dynamic_mesh(
                &device,
                "terra-segment",
                seg_res.vertex_count(),
                &crate::terra::segment::segment_indices(seg_res),
            ));
            let segment_uni = Some(make_space_uniform(
                &device,
                &bind_layout,
                &tex_view,
                &normal_view,
                &sampler,
            ));
            // **The cannon's geometry, DERIVED from its assembly** (docs/64). There is no cannon model
            // in this repo: the barrel is a tube because `naval-24pdr-gun.json` says it is a tube, and
            // each part wears its own material's colour and texture layer. Built here for the same
            // reason the segment is — this is where the bind layout and the material views are in
            // scope. Robin: the picture *"should be a product of the assembly and the engine"*.
            let (cannon_gpu, cannon_uni) = {
                let a =
                    crate::assembly::compiled::parse(crate::assembly::compiled::NAVAL_24PDR_GUN);
                let m = a.mesh(&mats, 20);
                let gpu = make_dynamic_mesh(&device, "terra-cannon", m.vertices.len(), &m.indices);
                queue.write_buffer(&gpu.vertex_buf, 0, bytemuck::cast_slice(&m.vertices));
                (
                    Some(gpu),
                    Some(make_space_uniform(
                        &device,
                        &bind_layout,
                        &tex_view,
                        &normal_view,
                        &sampler,
                    )),
                )
            };
            // **The plants the engine knows how to grow.** Their crown footprints are read off the
            // assemblies themselves, so the density that follows cannot drift from what is drawn.
            let flora_kinds = {
                let mk = |txt: &str, foliage: &str| {
                    let a = crate::assembly::compiled::parse(txt);
                    crate::terra::flora::Kind::from_assembly(&a, foliage)
                };
                vec![
                    mk(
                        crate::assembly::compiled::BROADLEAF_TREE_OAK,
                        "broadleaf_foliage",
                    ),
                    mk(
                        crate::assembly::compiled::CONIFER_TREE_SPRUCE,
                        "conifer_foliage",
                    ),
                    mk(crate::assembly::compiled::GRASS_TUFT, "grass"),
                ]
            };
            let flora_uni = Some(make_space_uniform(
                &device,
                &bind_layout,
                &tex_view,
                &normal_view,
                &sampler,
            ));

            let shell_count = 4096; // ~2.8° grain spacing — resolves continents/biomes (Phase 2, grain shell)
            let shell_unis: Vec<UniformSlot> = (0..shell_count)
                .map(|_| {
                    make_space_uniform(&device, &bind_layout, &tex_view, &normal_view, &sampler)
                })
                .collect();
            let earth = crate::planet::earth();
            let atm_tau = crate::atmosphere::rayleigh_tau(earth.surface_pressure() / 101_325.0);
            let atm_twilight = twilight_of(
                earth.radius(),
                earth.gravity_at(earth.radius()),
                &mats,
                atm_tau,
            );
            // Default fly camera: orbital over the equator (a world file overrides this in `load_world`).
            let fly = crate::terra::fly_camera::FlyCamera::new(
                20.0,
                0.0,
                12_000_000.0,
                0.0,
                -1.2,
                2.0,
                40_000_000.0,
            );
            let matter = MatterField::new(&device, config.format, 200_000);
            let flight_env = crate::flight::PlanetAir::of(&mats, "earth", earth_radius_m());
            Ok(Terra {
                detail: Default::default(),
                flight: crate::flight::Flight::default(),
                cannon_shots: 0,
                last_frame_s: 0.0,
                matter,
                flight_env,
                cap_octave_budget: TERRA_OCTAVE_BUDGET,
                // Start where a mid-range machine plausibly lands and let the FIRST rebuild correct
                // it — the starting value is a seed for the loop, not a claim about the hardware. One
                // rebuild may cost ~1.5 vsync frames: rebuilds are rare (nine in a 500 km descent) and
                // already the largest single cost in the frame they land on.
                // ★ The target is what a MESH REBUILD may cost, because that is what is measured —
                // the integral is not separable from the build it runs inside without timing half a
                // million probes individually. So the baseline mesh cost is the floor: on a machine
                // where building the mesh ALONE exceeds this, the integral correctly collapses to its
                // minimum, which is the graceful degradation that was asked for. Measured on a
                // 5060 Ti: baseline rebuild 61-79 ms, so ~110 ms leaves real headroom to spend and
                // still bounds the hitch. Lowering the BASELINE is separate open work (JOURNAL
                // 2026-07-31: "one tier's rebuild, the floor for scheduling WHOLE tiers").
                appearance_budget: crate::resolution::WorkBudget::new(4, 110.0),
                appearance_probes_pinned: 0,
                cam_pose: None,
                last_eye_m: None,
                ride: None,
                draw_matter: 2,
                drawn_buf: Vec::new(),
                inst_buf: Vec::new(),
                surface,
                device,
                queue,
                config,
                depth_view,
                pipeline,
                sphere_gpu,
                shell_unis,
                shell_count,
                globe_pipeline,
                body_id: "earth".into(),
                stars: None,
                segment_gpu,
                cannon_gpu,
                cannon_uni,
                flora_kinds,
                flora_uni,
                cannon_at: None,
                flora_gpu: None,
                flora_at: None,
                flora_anchor: glam::DVec3::ZERO,
                segment_uni,
                segment_built: None,
                surface_loaded: false,
                segment_verts: Vec::new(),
                tiles: Default::default(),
                epoch_s: None,
                relief_exag: TERRA_RELIEF_EXAG,
                mats,
                fly,
                planet_radius: earth_radius_m(),
                atm_twilight,
                atm_tau,
                world_name: String::new(),
                landmask: None,
                elevation: None,
                landcover: None,
                elev_range: [-11000.0, 9000.0],
                biome_mix: Vec::new(),
            })
        }

        /// docs/43: load a world from JSON + its decoded surface rasters. The JS host decodes each PNG to raw
        /// RGBA (4 channels) via ImageBitmap and passes the bytes + dims here. Any raster may be empty (`len 0`)
        /// → treated as absent (falls back to the built-in ASCII landmask / no displacement).
        #[allow(clippy::too_many_arguments)]
        pub fn load_world(
            &mut self,
            world_json: &str,
            landmask: &[u8],
            lm_w: u32,
            lm_h: u32,
            elevation: &[u8],
            ev_w: u32,
            ev_h: u32,
            landcover: &[u8],
            lc_w: u32,
            lc_h: u32,
        ) -> Result<(), JsValue> {
            let w = crate::terra::world_def::World::parse(world_json)
                .map_err(|e| JsValue::from_str(&e))?;
            if w.planet.is_none() {
                return Err(JsValue::from_str(
                    "Terra world is missing a `planet` section",
                ));
            }
            // The radius comes from the DEFINITION the world names (docs/59 one Earth) - the world
            // file places a body, it does not size one. Only an undefined sandbox body may declare
            // its own radius.
            self.planet_radius = crate::declared_planet_radius(&w).ok_or_else(|| {
                JsValue::from_str("Terra world names no body and declares no radius")
            })?;
            // ONE SOURCE for surface pressure: the declared atmosphere MASS, weighed. Reading a declared
            // `surface_pressure_pa` here was a docs/46 violation with a measured cost — Earth's world file
            // said 101,325 Pa while the emergent value is 99,049 Pa, so Terra's sky was a 2.2%-different
            // atmosphere from the one the terrain and orbit scenes render. Same planet, two airs.
            let g_surface = crate::planet::earth().gravity_at(self.planet_radius);
            let p_ratio = w
                .atmosphere
                .as_ref()
                .and_then(|a| a.surface_pressure(self.planet_radius, g_surface))
                .unwrap_or_else(|| crate::planet::earth().surface_pressure())
                / 101_325.0;
            self.atm_tau = crate::atmosphere::rayleigh_tau(p_ratio);
            // The flight environment is this body's own matter and air — re-resolve it when the world
            // changes, and never again per frame. (Sean's branch predates this cache and dropped it;
            // without it the flight step deserializes the planet every frame.)
            self.flight_env =
                crate::flight::PlanetAir::of(&self.mats, &self.body_id, self.planet_radius);
            self.atm_twilight =
                twilight_of(self.planet_radius, g_surface, &self.mats, self.atm_tau);
            self.world_name = w.name.clone();

            // docs/43 Phase 4 — seed the fly camera from the world's declared camera (default: orbital over 20°N).
            if let Some(c) = w.camera.as_ref() {
                let look = c.look.clone().unwrap_or_default();
                self.fly = crate::terra::fly_camera::FlyCamera::new(
                    c.lat,
                    c.lon,
                    if c.alt_m > 0.0 { c.alt_m } else { 12_000_000.0 },
                    look.yaw,
                    look.pitch,
                    c.min_alt_m.unwrap_or(2.0),
                    c.max_alt_m.unwrap_or(40_000_000.0),
                );
            }

            use crate::terra::raster::Raster;
            let mk = |bytes: &[u8], rw: u32, rh: u32| -> Option<Raster> {
                if bytes.is_empty() {
                    return None;
                }
                Raster::new(rw as usize, rh as usize, 4, bytes.to_vec()).ok()
            };
            self.landmask = mk(landmask, lm_w, lm_h);
            self.elevation = mk(elevation, ev_w, ev_h);
            self.landcover = mk(landcover, lc_w, lc_h);

            // Biome index → material index. `biomes` maps a string index → material id in data/materials.json.
            self.biome_mix.clear();
            self.elev_range = [-11000.0, 9000.0];
            self.relief_exag = TERRA_RELIEF_EXAG;
            // Earth's surface belongs to Earth, not to this world file. Prefer the body definition; a
            // world's own `surface` is only honoured for a body the engine has no definition for yet.
            if let Some(id) = w.body.as_deref() {
                self.body_id = id.to_string();
            }
            let body_surface = w
                .body
                .as_deref()
                .map(crate::planet::body)
                .and_then(|b| b.surface);
            if let Some(s) = body_surface.as_ref().or(w.surface.as_ref()) {
                if let Some(r) = s.elevation_range_m {
                    self.elev_range = r;
                }
                if let Some(x) = s.relief_exaggeration {
                    self.relief_exag = x.max(0.0);
                }
                self.biome_mix = s.biome_mixtures(&self.mats);
            }
            // **When this scene is set** (docs/65: time is part of the setting). A world that names an
            // epoch gets it; one that does not runs on the wall clock. Robin: *"One earth assembly.
            // Each scene can show different times, geological epochs, etc."*
            if let Some(t) = w.time.as_ref().and_then(|t| t.epoch) {
                self.epoch_s = Some(t);
            }

            // A new world is a different surface, so the cached segment is about the old one. The cache
            // is keyed on the CAMERA (`tier_is_current`), which cannot see that the planet underneath it
            // changed — so anything that moves the surface has to say so here.
            self.segment_built = None;
            // docs/43 Phase 3 — build the smooth displaced globe from the loaded rasters (retires the grain
            // shell for this scene). Built once here; the fly-camera LOD refinement comes in Phase 5.
            // docs/63: no globe mesh is built. The segment IS the surface, at whatever extent the camera
            // asks for, so a second planetary mesh has nothing left to do.
            self.surface_loaded = true;
            self.segment_built = None;

            let land_frac = self.landmask.as_ref().map(|r| r.land_fraction());
            log::info!("Terra: surface loaded — drawn as ONE sphere segment (docs/63)");
            log::info!(
                "Terra: loaded '{}' — radius {:.0} km, rasters land={} elev={} cover={}, land fraction {:?}",
                w.name,
                self.planet_radius / 1e3,
                self.landmask.is_some(),
                self.elevation.is_some(),
                self.landcover.is_some(),
                land_frac,
            );
            Ok(())
        }

        /// **Launch a meteor swarm at this world.** The whole of the scene's contribution: a mass, a
        /// material, a place and a velocity — INITIAL CONDITIONS, which is what a scene is for. Everything
        /// after this (falling, meeting the air, ablating, trailing, arriving) is
        /// `flight::Flight`, the same engine operation the ground patch runs (docs/59).
        ///
        /// The ICs, and why each is a declared fact rather than a chosen outcome:
        /// - **A disintegrated asteroid.** `damage::disrupt` divides the parent by Dohnanyi's measured
        ///   mass law and separates the pieces at the parent's own escape speed; the swarm's spread is
        ///   how long ago it came apart. Nobody picks a fragment size or a scatter width.
        /// - **17 km/s**, within the real 11–30 km/s range for an Earth-crossing body and comfortably
        ///   above escape (11.2 km/s) — a declared approach speed, not a consequence.
        /// - **Released 500 km up**, which is not a round number chosen to look right: it is above the
        ///   altitude the engine itself derives for where the air can still change the answer
        ///   (`atmosphere::air_reaches` puts that at ~296–354 km for bodies like these). Starting there
        ///   means the swarm arrives through the whole atmosphere, with nothing skipped.
        /// - **Aimed where the camera is looking**, exactly as the ground scene's crosshair IS the impact
        ///   point. That is the USER aiming, not the camera deciding physics — the fragments fall, ablate
        ///   and arrive whether or not anyone watches (Law IV).
        pub fn launch_swarm(&mut self) {
            self.launch_swarm_n(1_200);
        }

        /// The same, with the fragment COUNT given — the resolution the disruption is divided at (docs/44),
        /// which is a declaration a caller is allowed to make. Exposed so a rig can vary the workload and
        /// measure what actually costs frame time instead of guessing.
        /// **Emplace the gun at the point below the camera.**
        ///
        /// Placement only — WHICH assembly, WHERE, and WHICH WAY. The geometry was built from the
        /// assembly in `new()` (where the bind layout and material textures are in scope, the same
        /// reason the segment's are), so this sets a coordinate and nothing else.
        pub fn emplace_cannon(&mut self, bearing_deg: f64) {
            self.cannon_at = Some((self.fly.lat, self.fly.lon, bearing_deg));
            log::info!(
                "cannon emplaced at lat {:.3} lon {:.3}, bearing {bearing_deg:.0}",
                self.fly.lat,
                self.fly.lon
            );
        }

        /// The compass bearing the camera is looking along — so a gun points where you are looking.
        /// Reads the fly camera's yaw; it computes nothing.
        pub fn camera_bearing(&self) -> f64 {
            self.fly.yaw.to_degrees().rem_euclid(360.0)
        }

        /// **Fire a 24-pounder from the point below the camera, out along the given bearing.**
        ///
        /// ★ This is the whole of the scene's contribution, and it is deliberately nothing but
        /// placement: WHICH assemblies are present (the three compiled ones), WHERE the gun stands (the
        /// surface point under the camera), and WHICH WAY it points. `ballistics::fire_gun` derives the
        /// burn, the chamber pressure, the containment check and the muzzle velocity from those
        /// assemblies and the material catalogue; `flight::Flight` then carries the shot through the
        /// air exactly as it carries a meteor. **Nothing here computes a force, and
        /// `laws::scene_purity_tests` fails the build if it ever does.**
        ///
        /// Returns the muzzle velocity in m/s, or 0 if the gun did not fire (a burst or a squib, which
        /// the engine decides and the scene merely reports).
        pub fn fire_cannon(&mut self, bearing_deg: f64, elevation_deg: f64) -> f64 {
            use crate::assembly::compiled;
            let gun = compiled::parse(compiled::NAVAL_24PDR_GUN);
            let charge = compiled::parse(compiled::CHARGE_24PDR_SERVICE);
            let shot = compiled::parse(compiled::ROUND_SHOT_24PDR);
            if self.cannon_at.is_none() {
                self.emplace_cannon(bearing_deg);
            }
            // ★ A gun fires along ITS OWN bearing, not the camera's. The rig caught this: emplaced
            // facing 240, the shot left on 208, because the button passed whichever way the camera
            // happened to be looking. Where a gun points is a property of the gun.
            let (glat, glon, bearing_deg) =
                self.cannon_at
                    .unwrap_or((self.fly.lat, self.fly.lon, bearing_deg));
            let e = crate::ballistics::Emplacement {
                lat_deg: glat,
                lon_deg: glon,
                // The gun's BASE is on the ground; how high its muzzle sits above that is the
                // assembly's business and `fire_gun` derives it.
                height_m: 0.0,
                bearing_deg,
                elevation_deg,
            };
            match crate::ballistics::fire_gun(
                &gun,
                &charge,
                &shot,
                &e,
                self.planet_radius,
                &self.mats,
            ) {
                Ok((fired, Some(body), ejecta)) => {
                    // ★ **The muzzle's products go into the air, and the engine decided all of it.**
                    // Robin: *"smoke and flash should emerge naturally from the detonation/shape of
                    // barrel/velocity/amount of material, not the scene."* `fire_gun` returned WHERE
                    // the barrel ends, HOW FAST the gas leaves, HOW MUCH there is and HOW HOT it is;
                    // this hands that to the same door an ablating meteor's vapour goes through. The
                    // flash needs no separate effect — the products leave far above the temperature at
                    // which `emission::incandescence` makes matter glow.
                    if let Some(x) = ejecta {
                        // A muzzle blast is a CLOUD. Same mass, held more finely — a resolution
                        // choice, not a physical one (Law IV). The cone is the gas expanding as it
                        // leaves the bore's confinement.
                        self.flight
                            .shed_cloud(x.mass_kg, x.material, x.pos, x.vel, x.temp_k, 0.55, 160);
                    }
                    self.flight.introduce(body);
                    self.cannon_shots += 1;
                    log::info!(
                        "cannon: {:?} at {:.0} m/s, peak {:.0} MPa, recoil {:.2} m/s, ejecting {:.2} kg gas + {:.2} kg smoke at {:.0} K, from lat {:.2} lon {:.2} bearing {bearing_deg:.0}",
                        fired.outcome, fired.muzzle_ms, fired.peak_pressure_pa / 1.0e6,
                        fired.recoil_ms, fired.gas_kg, fired.residue_kg, fired.flame_k,
                        self.fly.lat, self.fly.lon
                    );
                    fired.muzzle_ms
                }
                Ok((fired, None, _)) => {
                    log::warn!(
                        "cannon: {:?} — peak {:.0} MPa against a wall good for {:.0} MPa",
                        fired.outcome,
                        fired.peak_pressure_pa / 1.0e6,
                        fired.peak_hoop_pa / 1.0e6
                    );
                    0.0
                }
                Err(e) => {
                    log::error!("cannon: {e}");
                    0.0
                }
            }
        }

        /// How many shots this gun has fired — for the HUD and for a rig to assert against.
        pub fn cannon_shots(&self) -> u32 {
            self.cannon_shots
        }

        pub fn launch_swarm_n(&mut self, count: usize) {
            let iron = materials::index_of(&self.mats, "iron");
            // Where the swarm is headed: the point on the surface under the camera. THE shared conversion
            // (`crate::geo`) — this was hand-rolled here with the opposite sign on z, so the swarm aimed at
            // a MIRRORED longitude and arrived nowhere near where the camera was pointed. CLAUDE.md warns
            // about exactly this: the tangent frame was once six hand-written copies, and the one sign they
            // all shared was wrong.
            let target =
                crate::geo::dir_from_lat_lon(self.fly.lat, self.fly.lon) * self.planet_radius;
            // Released 500 km above, offset so the path is a slanting entry rather than straight down —
            // a real Earth-crosser almost never arrives on the local vertical.
            let up = target.normalize();
            let east = glam::DVec3::Y.cross(up).normalize_or(glam::DVec3::X);
            let start = target + up * 500_000.0 - east * 900_000.0;
            let approach = (target - start).normalize() * 17_000.0;
            // A 3 m iron asteroid — ~890 tonnes — that came apart a day ago, resolved into 1,200 pieces
            // from ~11 cm to ~1.85 m. Both numbers are declared, and they are different KINDS of
            // declaration:
            //
            // - The parent SIZE is an initial condition: a disintegrated asteroid, which is what Robin
            //   asked for. It USED to be a half-metre bolide, chosen small because `atmospheric_step`
            //   heated a body's whole mass at once and nothing metre-scale could reach incandescence
            //   (docs/46 row 21). That is fixed — the heat now soaks in at the material's own diffusivity
            //   and only the skin warms — so the scene no longer has to shrink the event to fit the model.
            //   MEASURED after the fix: iron glows at its 3134 K boiling point at every size up to 3 m.
            // - The fragment COUNT is a RESOLUTION choice, not physics (docs/44): the same mass, divided
            //   more finely. 1,200 spans the size range where the air's share falls from ~15% to under 1%,
            //   so small pieces are consumed and large ones reach the ground — what a real fall does.
            let parent_r = 3.0_f64;
            let parent_m = self.mats[iron].density as f64
                * (4.0 / 3.0)
                * std::f64::consts::PI
                * parent_r.powi(3);
            // The trail resolves up to a fraction of the instance budget; past that the shed mass is
            // booked into the air (same mass, coarser). Law IV: representation, not existence.
            self.flight.set_trail_budget(120_000);
            self.flight.introduce_swarm(
                start,
                approach,
                parent_m,
                parent_r,
                iron,
                count.max(1),
                86_400.0,
                250.0,
            );
            log::info!(
                "swarm: {count} fragments of a {:.0} kg asteroid, entering at {:.1} km/s toward lat {:.1} lon {:.1}",
                parent_m, approach.length() / 1000.0, self.fly.lat, self.fly.lon
            );
        }

        /// **Drive the camera from outside.** `eye` is in metres, planet-centred (the engine's own frame);
        /// `forward`/`up` are directions; `fov_y` is the vertical field of view in radians. The engine
        /// renders from exactly this and derives everything else — latitude, longitude, altitude, and
        /// therefore the terrain LOD — from it, so a caller does not have to keep a second idea of where
        /// the camera is. Call `clear_camera_pose` to hand control back to the built-in fly camera.
        ///
        /// This exists so camera placement can be computed by whatever is best placed to compute it,
        /// including off the render thread: nothing here reads input, and nothing here decides framing.
        #[allow(clippy::too_many_arguments)]
        pub fn set_camera_pose(
            &mut self,
            eye_x: f64,
            eye_y: f64,
            eye_z: f64,
            fwd_x: f64,
            fwd_y: f64,
            fwd_z: f64,
            up_x: f64,
            up_y: f64,
            up_z: f64,
            fov_y: f64,
        ) {
            let eye = glam::DVec3::new(eye_x, eye_y, eye_z);
            let r = eye.length();
            // Altitude above the LOCAL ground, so the LOD machinery sees the same altitude it would if the
            // fly camera had been driven here.
            let (lat, lon) = crate::geo::lat_lon_from_dir(eye.normalize_or(glam::DVec3::Y));
            let ground_m = self.ground_disp_at(lat, lon) / display_scale();
            let alt_m = (r - self.planet_radius - ground_m).max(0.0);
            // Keep the fly camera in step: it is what the HUD, the cap and the blend read, and a caller
            // that later releases the pose should not find the camera somewhere else.
            self.fly.lat = lat;
            self.fly.lon = lon;
            self.fly.alt_m = alt_m.clamp(self.fly.min_alt, self.fly.max_alt);
            self.cam_pose = Some((
                (eye * display_scale()).to_array(),
                [fwd_x, fwd_y, fwd_z],
                [up_x, up_y, up_z],
                fov_y,
                self.fly.alt_m,
            ));
        }

        /// Hand the camera back to the built-in fly camera, where the pose left it.
        pub fn clear_camera_pose(&mut self) {
            self.cam_pose = None;
        }

        /// The heaviest body still in flight, as `[id, x, y, z, vx, vy, vz, radius_m, temp_k]` in metres —
        /// or an empty array if nothing is in flight. This is what a follower needs and no more: the engine
        /// says where its matter IS, and something else decides where to put a camera because of it
        /// (docs/59 Stage B). The heaviest is the one the air takes least of, so it is the one that will
        /// still be there at the ground.
        pub fn heaviest_fragment(&self) -> Vec<f64> {
            match self.flight.heaviest() {
                Some(b) => vec![
                    b.id as f64,
                    b.pos.x,
                    b.pos.y,
                    b.pos.z,
                    b.vel.x,
                    b.vel.y,
                    b.vel.z,
                    b.radius_m,
                    b.temp_k,
                ],
                None => Vec::new(),
            }
        }

        /// The body with this id, in the same layout — empty once it has arrived or been consumed, which is
        /// how a follower learns its fragment is gone rather than silently tracking a different one.
        pub fn fragment(&self, id: f64) -> Vec<f64> {
            match self.flight.body(id as u64) {
                Some(b) => vec![
                    b.id as f64,
                    b.pos.x,
                    b.pos.y,
                    b.pos.z,
                    b.vel.x,
                    b.vel.y,
                    b.vel.z,
                    b.radius_m,
                    b.temp_k,
                ],
                None => Vec::new(),
            }
        }

        /// **How many octaves of generated relief a surface sample may sum.** A rig knob for pricing
        /// detail against cost; physics is unaffected. (Was `set_cap_ladder(tiers, octaves)` — the tier
        /// count went with the tier ladder, docs/63.)
        pub fn set_octave_budget(&mut self, octave_budget: f64) {
            self.cap_octave_budget = octave_budget.max(0.0);
            self.segment_built = None;
        }

        /// **Pin the appearance integral's sample grid, for PRICING it** (docs/63, docs/46 row 29).
        ///
        /// Normally `resolution::WorkBudget` sets this from measured time. A rig needs it PINNED
        /// instead, because the way to find out what a stage costs is to move one thing and re-time —
        /// not to delete the stage and re-time the whole build, which prices the deletion rather than
        /// the stage (the gpu-perf rule). `0` hands control back to the budget.
        ///
        /// Cost should go as the grid AREA. If it does not, the cost is not in the probes and the
        /// optimisation aimed at the probes would be aimed at the wrong thing.
        pub fn set_appearance_probes(&mut self, side: u32) {
            self.appearance_probes_pinned = side as usize;
            self.segment_built = None;
        }

        /// **How much of the air column actually lies between the eye and the ground** — the factor the
        /// surface's in-scattered veil must be scaled by.
        ///
        /// `rayleigh_veil` computes the in-scatter for the FULL vertical column, which is right when the
        /// observer is above the atmosphere and wrong everywhere else: applied unscaled it puts a whole
        /// sky's worth of haze between a camera standing on the grass and the grass 1 m in front of it.
        /// Measured by ablation — with the veil disabled the same ground goes from a pale cyan wash
        /// (rgb ~150,230,190) to real grass (84,195,65) with its material grain visible. It was not the
        /// texture that was missing; it was drowned.
        ///
        /// The column above altitude `h` is `ρ₀·H·e^(−h/H)`, so the fraction lying BELOW the eye — the part
        /// its downward view actually looks through — is `1 − e^(−h/H)`, using the same barometric scale
        /// height `atmosphere::AirShell` derives from the air's own molar mass and temperature. Nothing is
        /// declared here: at 0.3 m altitude it is 3.5e-5 and the ground is its own colour, at one scale
        /// height 0.63, and from orbit it is 1 and the planet looks exactly as it did.
        ///
        /// **FLAGGED IOU (Law V).** This is the VERTICAL column difference, so it omits the horizontal path
        /// term: from ground level, distant terrain will now be too crisp, because the air along a long
        /// near-horizontal path is real and this does not count it. The computation it stands in for is the
        /// segment integral ∫ρ dl from eye to surface point, which for an exponential atmosphere over a
        /// linearly-varying altitude is `ρ₀·L·(e^(−h₁/H) − e^(−h₂/H))·H/(h₂−h₁)` — cheap, but it needs the
        /// eye's world position in the shader, which this uniform layout does not carry yet.
        fn veil_column_fraction(&self) -> f32 {
            let h = self.fly.alt_m.max(0.0);
            // The SAME scale height the flight integrates through — asked of the environment that owns
            // this world's air, not re-derived here (Law II).
            let scale_h = {
                use crate::flight::FlightEnvironment;
                self.flight_env.air_scale_height_m()
            };
            if !(scale_h > 0.0) {
                return 1.0; // an airless world has no veil to scale, and none is added
            }
            (1.0 - (-h / scale_h).exp()) as f32
        }

        /// **The instant this scene's SKY is drawn for** (Unix seconds) — the one answer to "where are the
        /// Sun and the stars", used by both the terminator and the star field so they cannot disagree.
        ///
        /// Free-running wall clock unless [`Terra::set_epoch`] pins it.
        ///
        /// **Deliberately NOT the simulation's clock.** The flight advances on elapsed wall time, because
        /// that is a DURATION ("how much time passed since the last frame") while this is an INSTANT
        /// ("what is the sky at"). They are different questions and pinning one must not stop the other:
        /// a rig that froze the sky and the physics together could not film anything moving. Over the
        /// seconds a rig runs, the Sun moves ~0.02°, so nothing observable is inconsistent.
        fn celestial_epoch_s(&self) -> f64 {
            self.epoch_s.unwrap_or_else(crate::orbit::unix_now_seconds)
        }

        /// **Pin the sky to an instant**, so a visual test is reproducible.
        ///
        /// Robin, after a rig run came back black and looked like a renderer collapse when it was simply
        /// the night side: *"this being a test rig, you should be able to rotate earth as you see fit to
        /// run a test?"* Exactly — a rig should command the clock rather than wait for the sun.
        ///
        /// This is also what makes screenshot comparison honest. Two runs of IDENTICAL code differ by a
        /// mean of 2.5–4.8/255 purely because the Sun and the star field moved between them, which is
        /// large enough to swamp a real change: proving the anchored-tier work had not altered the picture
        /// needed a same-code control run to measure that drift first. With the epoch pinned there is no
        /// drift to control for.
        pub fn set_epoch(&mut self, unix_seconds: f64) {
            self.epoch_s = Some(unix_seconds);
        }

        /// Hand the sky back to the wall clock.
        pub fn clear_epoch(&mut self) {
            self.epoch_s = None;
        }

        /// **Put the daylight over this longitude**, by solving the engine's own solar law for the instant
        /// that does it (`orbit::epoch_for_sub_solar_lon`) and pinning the sky there. Returns the epoch.
        ///
        /// The solver lives in `orbit` rather than here, or in a rig, because "where is the Sun" must have
        /// one answer (Law II) — and the rule a harness would otherwise write for itself, subsolar
        /// longitude ≈ 180° − 15°·UTC_hours, is wrong by degrees: it has no equation of time and no
        /// sidereal/solar day distinction.
        pub fn set_epoch_sun_over_lon(&mut self, lon_deg: f64) -> f64 {
            let t =
                // ★ Solve near the epoch ALREADY PINNED, if there is one — not always near now.
                // Otherwise this silently threw away a date: a rig asking for "October, sun overhead"
                // got "today, sun overhead", and a seasons run reported the same season on four
                // different dates because the second call overwrote the first. Pinning a date and then
                // pinning the daylight are not in conflict; they are latitude and longitude of the
                // same instant.
                crate::orbit::epoch_for_sub_solar_lon(lon_deg, self.celestial_epoch_s());
            self.epoch_s = Some(t);
            t
        }

        /// Where the Sun is standing overhead right now, `[lat, lon]` in degrees — the engine answering
        /// from the law it draws with, so a caller never needs its own solar model.
        pub fn sub_solar(&self) -> Vec<f64> {
            let d = crate::orbit::solar_direction_earth_fixed(self.celestial_epoch_s());
            let (lat, lon) = crate::geo::lat_lon_from_dir(d);
            vec![lat, lon]
        }

        /// **How high the Sun stands above the horizon at this coordinate, in degrees** — negative
        /// when it is below, i.e. when it is night there.
        ///
        /// Robin (2026-08-03), looking at the gun on the Irish coast after dark: *"It's dark in
        /// Galway… let's switch back to Chile. Probably need a scene button where you can flip to
        /// wherever it's daylight of the two."* A scene needs to know which of its sites is lit, and
        /// the scene must not work that out for itself — solar geometry is the engine's, and a page
        /// doing its own spherical trig is a second answer to a question already answered here.
        ///
        /// It is `asin(up · sun)`: the local up from [`geo::tangent_frame`] against the solar
        /// direction the sky is already drawn with. No new physics — two existing primitives, dotted.
        pub fn sun_elevation_deg(&self, lat_deg: f64, lon_deg: f64) -> f64 {
            let sun = crate::orbit::solar_direction_earth_fixed(self.celestial_epoch_s());
            let (up, _, _) = crate::geo::tangent_frame(lat_deg, lon_deg);
            up.dot(sun).clamp(-1.0, 1.0).asin().to_degrees()
        }

        /// **How far through its autumn this latitude is**, 0..1 — a read, for rigs and HUDs, of the
        /// same `solar::senescence_fraction` the surface itself spends.
        pub fn senescence_at(&self, lat_deg: f64) -> f64 {
            crate::solar::senescence_fraction(lat_deg, self.celestial_epoch_s())
        }

        /// **Which land-cover class and which material the surface has at this coordinate** — the
        /// engine's own answer, as `"<class>:<material id>"`.
        ///
        /// A read, not a control: it changes nothing and decides nothing. It exists so a rig can check
        /// the PICTURE against the DATA instead of inferring one from the other. That distinction has
        /// already cost real time here — a frame the colour of cut lumber was read as a lighting fault
        /// for a whole session, and the thing that settled it was asking the engine what material it
        /// thought it was standing on (`docs/46` row 28).
        pub fn surface_material_at(&self, lat_deg: f64, lon_deg: f64) -> String {
            let class = self
                .landcover
                .as_ref()
                .map_or(1, |r| r.biome_at(lat_deg, lon_deg) as usize);
            // The whole mixture, so a rig sees what a land-cover class actually IS — "8:broadleaf_
            // foliage 0.45 + grass 0.35 + dirt 0.20" rather than a single name that hides two thirds
            // of the ground.
            let Some(mix) = self.biome_mix.get(class) else {
                return format!("{class}:<unmapped>");
            };
            let parts: Vec<String> = mix
                .iter()
                .map(|&(m, f)| format!("{} {:.2}", self.mats[m].id, f))
                .collect();
            format!("{class}:{}", parts.join(" + "))
        }

        /// **What measured elevation this view needs and does not have** — a JSON `[[z,x,y],…]`, nearest
        /// first, for a host to fetch and hand back through [`Terra::add_tile`].
        ///
        /// The ENGINE chooses, because which data to resolve is a resolution decision and those belong to
        /// the universe (docs/44); the host only performs the I/O, exactly as it already does for the
        /// world's own rasters. The zoom comes from `tiles::zoom_for_ground_size` asked with the observer's
        /// own resolvable size, so it is the same angular budget that sizes particle granularity and the
        /// raster hand-off — not a second opinion about what is worth seeing.
        ///
        /// Bounded by construction: a 3×3 patch that follows the camera, so this cannot ask for the planet.
        pub fn tiles_wanted(&self) -> String {
            let mut out = String::from("[");
            for (i, t) in self.tiles.missing().iter().take(16).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("[{},{},{}]", t.z, t.x, t.y));
            }
            out.push(']');
            out
        }

        /// Hand back a decoded tile — `data` is interleaved RGB(A), `px` square, terrarium-encoded, which
        /// is what the browser produces from the PNG by the same route it decodes the world's rasters.
        /// A tile the camera has already moved past is dropped rather than stored.
        pub fn add_tile(&mut self, z: u32, x: u32, y: u32, data: &[u8], px: u32) {
            let chans = if px > 0 {
                (data.len() / (px as usize * px as usize)).max(1)
            } else {
                return;
            };
            if chans < 3 || data.len() < (px * px) as usize * chans {
                return;
            }
            let before = self.tiles.len();
            self.tiles.insert(crate::terra::tiles::Tile {
                id: crate::terra::tiles::TileId { z, x, y },
                px,
                chans,
                data: data.to_vec(),
            });
            // New measured ground is exactly the kind of change the tier cache CANNOT see: it is keyed on
            // where the camera is, and the camera did not move. Without this the tiles would arrive and
            // the mesh would keep drawing the surface it was built from — the same failure `set_cap_ladder`
            // has to guard against, and it would look like the streaming did nothing.
            if self.tiles.len() != before {
                self.segment_built = None;
            }
        }

        /// How many streamed tiles are held — for a HUD, and for a rig to know the patch is complete
        /// before it believes a screenshot.
        pub fn tile_count(&self) -> usize {
            self.tiles.len()
        }

        /// **The altitude band the observer may occupy** (m). A world declares this in its camera block
        /// (`min_alt_m` / `max_alt_m`); Earth's says 2 m to 40,000 km, which is a statement about a person
        /// standing on a planet, not about what the engine can render.
        ///
        /// It is exposed because the scale-ladder rig has to ask a question the declared band cannot
        /// express: *does one continuous representation carry the view from interplanetary distance down to
        /// standing height* (docs/13, docs/23). `set_camera_pose` places the eye anywhere, but it CLAMPS the
        /// altitude the LOD machinery reads to this band, so outside it the cap, the blend and the near
        /// plane are all being told a different altitude from the one the eye is at — and a scale ladder
        /// that walked past the clamp would be measuring the clamp.
        ///
        /// Not a physics knob: nothing here changes what exists or how it moves, only how far the observer
        /// may stand. `FlyCamera::new`'s own floor of 0.1 m still applies and is not raised.
        pub fn set_alt_bounds(&mut self, min_alt_m: f64, max_alt_m: f64) {
            self.fly.min_alt = min_alt_m.max(0.1);
            self.fly.max_alt = max_alt_m.max(self.fly.min_alt);
            self.fly.alt_m = self.fly.alt_m.clamp(self.fly.min_alt, self.fly.max_alt);
        }

        /// Diagnostic knob — see `draw_matter`. Simulation is unaffected.
        pub fn set_draw_matter(&mut self, mode: u32) {
            self.draw_matter = mode;
        }

        /// How many pieces of matter the engine is holding in flight — for a HUD, and so a rig can assert
        /// that something is actually there rather than trusting a picture.
        pub fn flight_count(&self) -> usize {
            self.flight.bodies().len()
        }

        /// How many marks of matter were drawn last frame (bodies + shed vapour).
        pub fn drawn_count(&self) -> u32 {
            self.matter.drawn_count()
        }

        /// Mass the entry has ablated away so far (kg) — the trail, on the books.
        pub fn trail_mass_kg(&self) -> f64 {
            self.flight.trail().mass()
        }

        /// The lowest altitude (km) anything in flight has reached, and the fastest speed (km/s) — so a
        /// rig can tell "descending" from "stuck" without trusting a picture, and a HUD can say how far
        /// the entry has got.
        pub fn swarm_min_alt_km(&self) -> f64 {
            self.flight
                .bodies()
                .iter()
                .map(|b| (b.pos.length() - self.planet_radius) / 1000.0)
                .fold(f64::INFINITY, f64::min)
        }

        pub fn swarm_speed_kms(&self) -> f64 {
            self.flight
                .bodies()
                .iter()
                .map(|b| b.vel.length() / 1000.0)
                .fold(0.0, f64::max)
        }

        /// How the trail is FADING: parcels still resolved, the hottest and mass-mean temperature of
        /// what is left, and the mass that has finished cooling into the air. A trail dissipating is
        /// exactly these numbers moving — so they are readable rather than inferred from a picture.
        pub fn trail_parcels(&self) -> usize {
            self.flight.trail().parcels().len()
        }
        pub fn trail_hot_k(&self) -> f64 {
            self.flight.trail().temperature_range_k().0
        }
        pub fn trail_mean_k(&self) -> f64 {
            self.flight.trail().temperature_range_k().1
        }
        pub fn trail_merged_kg(&self) -> f64 {
            self.flight.trail().merged_kg()
        }

        pub fn world_name(&self) -> String {
            self.world_name.clone()
        }

        // docs/43 Phase 4 — the continuous fly-camera API (WASD + zoom(=altitude) + mouse-look). The JS host
        // maps input to these; the camera itself blends orbit⇄ground by altitude (see `terra::fly_camera`).

        /// Set the camera outright (lat/lon degrees, altitude metres, look yaw/pitch radians).
        /// **PLACE THE CAMERA** — `<position, heading>`, the first of the two verbs a scene gets.
        ///
        /// Robin (2026-08-03): *"place camera `<position, heading>` and camera-follow `<assembly>,
        /// `<relative position>`, `<heading>`."* This is a statement about where the observer STANDS,
        /// in the body's own frame, which is the frame a scene naturally knows things in ("the gun is
        /// at Galway; stand behind it"). Nothing about a camera MODEL crosses the boundary.
        ///
        /// It replaces `set_fly`, whose name was itself the problem: "fly" is a camera model, and a
        /// scene that names a camera model is a scene that has one.
        ///
        /// ★ **This is a request, not a placement.** The camera is matter (docs/46 row 36), so the eye
        /// ends up where the contact law allows — ask for a seat inside a mountain and you get one on
        /// its slope. Read [`Terra::altitude_m`] afterwards for where it actually is.
        pub fn place_camera(&mut self, lat: f64, lon: f64, alt_m: f64, yaw: f64, pitch: f64) {
            self.ride = None; // a scene that names a place has stopped following something
            self.fly.lat = lat;
            self.fly.lon = lon;
            self.fly.alt_m = alt_m.clamp(self.fly.min_alt, self.fly.max_alt);
            self.fly.yaw = yaw;
            self.fly.pitch = pitch;
        }

        /// **CAMERA-FOLLOW** — `<assembly>, <relative position>, <heading>`, the second verb.
        ///
        /// `subject` names what to ride: `"heaviest"` for the largest thing in flight, or a body's id
        /// as a decimal string. `back_m`/`up_m`/`side_m` are an offset **in the subject's own frame**
        /// (behind where it is going, above it, beside it); `yaw`/`pitch` are the heading **relative to
        /// that frame**, so `0,0` looks where the subject is going and `yaw = π` looks back down the
        /// trajectory. Pass an empty subject to let go.
        ///
        /// ★★ **This deletes the scene's own chase camera**, which was 43 lines of vector maths run
        /// every frame — normalising a velocity, crossing it with the local up to build a basis, and
        /// scaling a standoff by the subject's radius. The engine knows where its own matter is; a
        /// scene reading that back to compute a camera position is the wrong side of docs/65. It also
        /// had the bug that follows from the standoff heuristic: riding a 7 cm cannonball put the eye
        /// a metre away and filled the frame with sky.
        pub fn camera_follow(
            &mut self,
            subject: &str,
            back_m: f64,
            up_m: f64,
            side_m: f64,
            yaw: f64,
            pitch: f64,
        ) -> bool {
            let subject = subject.trim();
            if subject.is_empty() || subject.eq_ignore_ascii_case("none") {
                self.ride = None;
                return false;
            }
            let which = if subject.eq_ignore_ascii_case("heaviest") {
                RideSubject::Heaviest
            } else {
                match subject.parse::<u64>() {
                    Ok(id) => RideSubject::Body(id),
                    Err(_) => {
                        log::warn!("camera_follow: no subject named '{subject}'");
                        return false;
                    }
                }
            };
            // Refuse to ride something that is not there, rather than silently pointing at nothing.
            let exists = match which {
                RideSubject::Heaviest => self.flight.heaviest().is_some(),
                RideSubject::Body(id) => self.flight.body(id).is_some(),
            };
            if !exists {
                self.ride = None;
                return false;
            }
            self.ride = Some(Ride {
                subject: which,
                back_m,
                up_m,
                side_m,
                yaw,
                pitch,
            });
            true
        }

        /// Is the camera riding something? Goes false on its own when the subject lands or is culled,
        /// so a scene does not have to watch for that.
        pub fn camera_is_following(&self) -> bool {
            self.ride.is_some()
        }

        /// WASD: move across the surface. `forward`/`right` are −1/0/+1 intents; the step scales with altitude
        /// (fast from orbit, metres-per-frame on the ground) so a keypress feels the same at every scale.
        pub fn move_tangent(&mut self, forward: f64, right: f64) {
            // Step ≈ a small fraction of the current altitude per frame, floored so ground movement still works.
            let step = (self.fly.alt_m * 0.02).max(2.0);
            self.fly
                .move_tangent(forward * step, right * step, self.planet_radius);
        }

        /// Zoom = altitude change. `notches` is the wheel delta (or +/−1); positive climbs, negative descends.
        pub fn zoom_alt(&mut self, notches: f64) {
            self.fly.zoom_alt((notches * 0.12).exp());
        }

        /// A pointer drag (pixel deltas): orbit high up, free-look near the ground (altitude-blended).
        pub fn drag_look(&mut self, dx: f64, dy: f64) {
            self.fly.drag(dx, dy);
        }

        /// Pan: slide across the surface by a pointer delta in DEVICE pixels, the same gesture the
        /// orbit band honours, expressed in this camera's rig. The SAME mover as the strafe keys
        /// (`FlyCamera::move_tangent`), fed metres instead of key intents: one pixel of the
        /// camera's frustum spans `2·alt·tan(fov_y/2)/h` metres of ground under a downward view, so
        /// the globe tracks the pointer one-for-one, map-style (dragging right carries the
        /// viewpoint west; screen y grows downward, and dragging down carries it north). The FOV is
        /// read from the same constant the projection is built from, never a second copy.
        pub fn pan_tangent(&mut self, dx_px: f64, dy_px: f64) {
            let m_per_px =
                2.0 * self.fly.alt_m * (0.5 * crate::terra::fly_camera::DEFAULT_FOV_Y).tan()
                    / self.config.height.max(1) as f64;
            self.fly
                .move_tangent(dy_px * m_per_px, -dx_px * m_per_px, self.planet_radius);
        }

        pub fn altitude_m(&self) -> f64 {
            self.fly.alt_m
        }
        pub fn latitude(&self) -> f64 {
            self.fly.lat
        }
        pub fn longitude(&self) -> f64 {
            self.fly.lon
        }

        /// docs/43 Phase 6 — the surface type directly under the camera (for the HUD): the biome material id on
        /// land ("grass", "sand", "snow", …) or "ocean" over water.
        pub fn ground_biome(&self) -> String {
            let (lat, lon) = (self.fly.lat, self.fly.lon);
            let is_land = self
                .landmask
                .as_ref()
                .map(|r| r.land_at(lat, lon))
                .unwrap_or(false);
            if !is_land {
                return "ocean".to_string();
            }
            let biome = self
                .landcover
                .as_ref()
                .map_or(1, |r| r.biome_at(lat, lon) as usize);
            // The class's dominant constituent stands for it where ONE material is wanted.
            let mi = self
                .biome_mix
                .get(biome)
                .and_then(|m| m.iter().max_by(|a, b| a.1.total_cmp(&b.1)))
                .map(|&(m, _)| m)
                .unwrap_or(0);
            self.mats.get(mi).map(|m| m.id.clone()).unwrap_or_default()
        }

        pub fn resize(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 {
                return;
            }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = create_depth_view(&self.device, width, height);
        }

        pub fn render(&mut self) -> Result<(), JsValue> {
            let r_disp = self.planet_radius * display_scale(); // = 1.0 for Earth
                                                               // docs/43 Phase 4/5 — the fly camera builds the frame (absolute + camera-relative
                                                               // view·projection, the f64 eye, and the tangent frame). The terrain height under the camera
                                                               // keeps "altitude" above the local ground (not sea level).
            let aspect = self.config.width as f64 / self.config.height.max(1) as f64;

            // ★★ **THE CAMERA IS MATTER, and this is where that becomes true for Terra.**
            //
            // The eye the fly camera WANTS is resolved against the surface by the same contact law a
            // grain obeys, and where it ends up is where it is. Nothing below gets a say: the resolved
            // position is fed back into the camera itself, so the HUD's altitude, the LOD's idea of how
            // close the ground is, and the picture all read one position.
            //
            // What this replaces: `alt_m.clamp(min_alt, ..)` stacked on a ground height that was the
            // MAX over a 22 km neighbourhood. Two fudges, and neither could slide — a clamp only ever
            // pushes the eye straight UP, so a camera driven into a steep face popped through it.
            // **Riding an actor** (`camera_follow`): the engine knows where its own matter is, so it
            // seats the observer itself rather than handing coordinates out for a scene to do maths on.
            // The ride ENDS ITSELF when its subject lands or is culled — a scene should not have to
            // watch for that, and the old scene-side version had to.
            if let Some(ride) = self.ride {
                let body = match ride.subject {
                    RideSubject::Heaviest => self.flight.heaviest(),
                    RideSubject::Body(id) => self.flight.body(id),
                };
                match body {
                    Some(b) => {
                        let ds = display_scale();
                        let (eye_m, look, up) = crate::terra::fly_camera::ride_pose(
                            b.pos,
                            b.vel,
                            ride.back_m,
                            ride.up_m,
                            ride.side_m,
                            ride.yaw,
                            ride.pitch,
                        );
                        // The camera is matter wherever it is — a seat behind a shot is still a seat
                        // that cannot be inside a hillside.
                        let eye = self.camera_shell_resolve(eye_m * ds);
                        self.last_eye_m = Some(eye / ds);
                        let (la, lo) =
                            crate::geo::lat_lon_from_dir((eye / ds).normalize_or(glam::DVec3::Y));
                        self.fly.lat = la;
                        self.fly.lon = lo;
                        self.fly.alt_m = ((eye / ds).length()
                            - self.planet_radius
                            - self.ground_disp_at(la, lo) / ds)
                            .max(0.0);
                        self.cam_pose = Some((
                            eye.to_array(),
                            look.to_array(),
                            up.to_array(),
                            0.9,
                            self.fly.alt_m,
                        ));
                    }
                    None => {
                        self.ride = None;
                        self.cam_pose = None;
                    }
                }
            }

            if self.cam_pose.is_none() {
                let ds = display_scale();
                let ground0 = self.ground_disp_at(self.fly.lat, self.fly.lon);
                let (desired, _, _) = self.fly.view_basis(r_disp, ds, ground0);
                let resolved = self.camera_shell_resolve(desired);
                let m = resolved / ds;
                if (resolved - desired).length() > 0.0 {
                    let (la, lo) = crate::geo::lat_lon_from_dir(m.normalize_or(glam::DVec3::Y));
                    self.fly.lat = la;
                    self.fly.lon = lo;
                    self.fly.alt_m =
                        (m.length() - self.planet_radius - self.ground_disp_at(la, lo) / ds)
                            .max(0.0);
                }
                self.last_eye_m = Some(m);
            }

            let ground_disp = self.ground_disp_at(self.fly.lat, self.fly.lon);
            self.build_flora();
            // An externally supplied pose is the authority when there is one; otherwise the fly camera.
            // (docs/59 observer/universe, PR #81 — Sean's branch predates it and dropped the whole arm.)
            let view = match self.cam_pose {
                Some((eye, fwd, up, fov_y, alt_m)) => {
                    // How close is the engine's OWN matter? The camera may be riding a metre-wide fragment
                    // from eighty metres away while two hundred kilometres up, and a near plane derived
                    // from altitude would clip it (measured: it did — a black screen with a working HUD).
                    let eye_m = glam::DVec3::from_array(eye) / display_scale();
                    let nearest_m = self
                        .flight
                        .bodies()
                        .iter()
                        .map(|b| (b.pos - eye_m).length() - b.radius_m)
                        .fold(f64::INFINITY, f64::min)
                        .max(0.01);
                    crate::terra::fly_camera::FlyCamera::view_from_pose(
                        glam::DVec3::from_array(eye),
                        glam::DVec3::from_array(fwd),
                        glam::DVec3::from_array(up),
                        fov_y,
                        r_disp,
                        display_scale(),
                        aspect,
                        alt_m,
                        nearest_m,
                    )
                }
                None => self.fly.view(r_disp, display_scale(), aspect, ground_disp),
            };
            // Captured here because `view` is shadowed by a texture view further down; the FOV still has
            // exactly one source (`View::fov_y`), which is the point.
            let cam_fov_y = view.fov_y;
            // THE CAMERA-RELATIVE-EYE CONVENTION (terra::fly_camera module doc): every draw in this
            // scene uses `vp_rel` (eye at the origin). The eye leaves f64 only as a model translation
            // of −eye (static meshes) or already subtracted per-vertex (the cap); never as an
            // absolute f32 position, which cannot hold the final metres at planet radius.
            let view_proj = view.vp_rel;
            let eye = view.eye;
            // Static meshes are world-absolute; this f64-built translation makes their draw
            // camera-relative. Residual: ~2 f32 ULPs at planet radius, sub-pixel at the ≥15 km
            // distances where the coarse globe is visible (see the fly_camera precision tests).
            let rel_model = glam::DMat4::from_translation(-eye).as_mat4();
            // Triplanar texture anchor: the relief textures must stay glued to the SURFACE while
            // positions are camera-relative, so the shader re-adds the eye folded modulo the texture
            // tile period (8 m; globe.wgsl GLOBE_TEX_SCALE). Folded in f64, it is tiny in f32; the
            // full eye would just re-lose the precision the subtraction bought.
            let tile_p = 8.0 * display_scale();
            let anchor = glam::DVec3::new(
                eye.x.rem_euclid(tile_p),
                eye.y.rem_euclid(tile_p),
                eye.z.rem_euclid(tile_p),
            )
            .as_vec3();
            // The REAL direction to the Sun for right now (orbit::solar_direction_earth_fixed). What stood
            // here was `DVec3::new(1.0, 0.45, 0.6)` — a fixed vector whose comment called it "a pleasant ¾
            // lighting" while claiming the terminator was emergent. It was not: the globe was already
            // oriented to real time, so a decorative sun put noon in the wrong ocean and the day/night line
            // wherever it happened to land. Now the terminator is where the Sun actually puts it, and the
            // seasons come from the same declination that makes them real.
            let sun_dir = crate::orbit::solar_direction_earth_fixed(self.celestial_epoch_s());
            let sun_light = Vec3::new(sun_dir.x as f32, sun_dir.y as f32, sun_dir.z as f32);

            // docs/43 Phase 5 — build the fine ground cap under the camera and cross-fade it in as we
            // descend. The fade/lift rules live in `terra::ground_cap` (natively tested). The hand-off
            // altitude is DERIVED from the rasters' own resolution against the docs/49 angular budget
            // (`ground_cap::handoff_alt_m`) — below it the planetary raster is being stretched, so the
            // cap (which resamples the same raster at the camera's own angular density) takes over.
            // The globe is skipped only once the cap is fully faded in AND genuinely covers the view
            // out past the horizon; that removal is what keeps the final metres free of the
            // cap-vs-globe depth fight.
            let alt_m = self.fly.alt_m;
            let cap_start_alt = crate::terra::ground_cap::handoff_alt_m(
                crate::terra::ground_cap::finest_texel_arc_m(
                    &[
                        self.landmask.as_ref(),
                        self.elevation.as_ref(),
                        self.landcover.as_ref(),
                    ],
                    self.planet_radius,
                )
                .unwrap_or(0.0),
                self.detail.angular_resolution,
            );
            let _ = cap_start_alt;
            // **Ask for the measured elevation this view needs** (docs/46 row 27). The zoom is the docs/49
            // angular budget asked of a tile pyramid — the same "how fine can this viewer resolve" that
            // sizes everything else — and the patch is a bounded 3x3 that follows the camera, so a descent
            // walks the ladder down instead of downloading a planet. Costs nothing until a host answers.
            {
                let want_m = self.detail.camera_grain_radius(alt_m.max(1e-3));
                let z = crate::terra::tiles::zoom_for_ground_size(want_m, self.fly.lat, 256);
                self.tiles.want_patch(self.fly.lat, self.fly.lon, z, 1);
            }

            // **THE one surface** (docs/63): a segment whose extent is simply what is visible from here.
            if self.surface_loaded {
                self.build_segment(&view, sun_light, anchor);
            }

            // **The engine advances its own matter.** Terra's part is a wall-clock dt and the environment
            // this world presents; the flight law itself is `flight::Flight::step` — the same code the
            // ground patch runs, and the reason the swarm needs no Terra-specific physics (docs/59).
            {
                let now = crate::orbit::unix_now_seconds();
                // First frame, or a tab that was backgrounded: take one frame's worth rather than a gap.
                // Clamped to a THIRTIETH of a second, not a quarter. A long gap (a stalled frame, a
                // backgrounded tab) must not turn into a large catch-up step: the work that step implies
                // is what stalls the next frame, and the sim falling briefly behind wall-clock is a far
                // smaller lie than the page ceasing to respond.
                let dt = if self.last_frame_s <= 0.0 {
                    1.0 / 60.0
                } else {
                    (now - self.last_frame_s).clamp(0.0, 1.0 / 30.0)
                };
                self.last_frame_s = now;
                if !self.flight.bodies().is_empty() || !self.flight.trail().parcels().is_empty() {
                    // ONE call: the engine sizes its own substeps from the air's scale height. This used
                    // to substep HERE, from the frame time, which was a feedback loop — a slow frame asked
                    // for more substeps, which made the next frame slower, until the page stopped
                    // answering the mouse. (Robin: "we seem to lose camera controls when the engine is
                    // working.")
                    let env = &self.flight_env;
                    for a in self.flight.step(env, &self.mats, dt) {
                        log::info!(
                            "arrival: {:.1} kg at {:.1} km/s, {:.0} K = {:.2e} J",
                            a.body.mass_kg,
                            a.body.vel.length() / 1000.0,
                            a.body.temp_k,
                            a.energy_j
                        );
                    }
                    // What the engine is holding, as it must be drawn — mapped once, by the engine's rule,
                    // into buffers that persist across frames.
                    if self.draw_matter >= 1 {
                        let (drawn, inst) = (&mut self.drawn_buf, &mut self.inst_buf);
                        // **CAMERA-RELATIVE, like every other draw in this scene.** The eye is subtracted
                        // HERE, per particle, in f64 — the convention's "already subtracted per-vertex"
                        // case (the ground cap is the other one). `MatterField::draw` is handed `vp_rel`,
                        // which puts the eye at the ORIGIN, so an absolute position would be rendered a
                        // whole eye-vector away: at the surface that is ~1.0 display unit, an entire
                        // planet radius off screen.
                        //
                        // That is exactly what happened. Adopting the camera-relative convention
                        // (upstream-7) switched `view_proj` from `vp_abs` to `vp_rel` and gave the static
                        // meshes a −eye model matrix, but this upload still emitted absolute positions —
                        // his branch predates the MatterField, so there was nothing here to convert and
                        // nothing flagged it. MEASURED: a followed fragment at 1553 K with 1,778 items
                        // drawn rendered a completely black frame.
                        //
                        // Subtracting in f64 before the f32 cast also buys back precision the absolute
                        // form never had: raw f32 at Earth's radius has ~0.6 m ULP, which is larger than
                        // the fragments this camera exists to sit behind.
                        let eye_disp = eye;
                        self.flight.drawn_into(drawn, &self.flight_env, |p| {
                            (p * display_scale() - eye_disp).as_vec3()
                        });
                        inst.clear();
                        inst.extend(drawn.iter().map(|d| GpuParticle::of_matter(d, &self.mats)));
                        self.matter.upload(&self.queue, inst);
                    } else {
                        self.matter.upload(&self.queue, &[]);
                    }
                } else {
                    self.matter.upload(&self.queue, &[]);
                }
            }

            let air = self.air();
            if !self.surface_loaded {
                // Fallback: the Phase-2 grain shell (used until a world's surface rasters build the globe mesh).
                let shell_spacing = self.planet_radius
                    * (4.0 * std::f64::consts::PI / self.shell_count as f64).sqrt();
                let grain_r = ((0.62 * shell_spacing) * display_scale()) as f32;
                const EXAG: f64 = TERRA_RELIEF_EXAG;
                let water_idx = materials::index_of(&self.mats, "water");
                for (i, uni) in self.shell_unis.iter().enumerate() {
                    let dir = crate::impact::fib_dir(i, self.shell_count);
                    let (lat, lon) = crate::geo::lat_lon_from_dir(dir);
                    // Land/ocean from the real Natural Earth mask (fallback: the built-in ASCII mask).
                    let is_land = self
                        .landmask
                        .as_ref()
                        .map(|r| r.land_at(lat, lon))
                        .unwrap_or_else(|| crate::planet::earth_surface_material(dir) == "granite");
                    // Land: biome material (land-cover) + real elevation displacement. Ocean: water at sea level.
                    let (mat_idx, elev_m) = if is_land {
                        let biome = self
                            .landcover
                            .as_ref()
                            .map_or(1, |r| r.biome_at(lat, lon) as usize);
                        let mi = self
                            .biome_mix
                            .get(biome)
                            .and_then(|m| m.iter().max_by(|a, b| a.1.total_cmp(&b.1)))
                            .map(|&(m, _)| m)
                            .unwrap_or(water_idx);
                        let e = self
                            .elevation
                            .as_ref()
                            .map_or(0.0, |r| {
                                r.elevation_m_at(lat, lon, self.elev_range[0], self.elev_range[1])
                            })
                            .max(0.0);
                        (mi, e)
                    } else {
                        (water_idx, 0.0)
                    };
                    let m = &self.mats[mat_idx];
                    let pos = dir * (r_disp + elev_m * display_scale() * EXAG);
                    // Camera-relative translation (the convention above): subtracted in f64, cast small.
                    let spos = (pos - eye).as_vec3();
                    // Rayleigh atmosphere (docs/26): blue veil (added light) + two-way transmittance on the ground.
                    let v_dir = (eye - pos).normalize_or_zero();
                    let mu_v = dir.dot(v_dir);
                    let mu_s = dir.dot(sun_dir);
                    let cos_th = v_dir.dot(sun_dir);
                    let veil = crate::atmosphere::rayleigh_veil(
                        mu_v,
                        mu_s,
                        cos_th,
                        self.atm_tau,
                        crate::atmosphere::SUN_GAIN as f64,
                        self.atm_twilight,
                    );
                    let tr = crate::atmosphere::rayleigh_transmit(mu_v, mu_s, self.atm_tau);
                    let tint = [
                        m.albedo[0] * tr[0],
                        m.albedo[1] * tr[1],
                        m.albedo[2] * tr[2],
                        1.0,
                    ];
                    write_space_uniform(
                        &self.queue,
                        uni,
                        view_proj,
                        Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(grain_r)),
                        sun_light,
                        tint,
                        [veil[0], veil[1], veil[2], 1.0],
                        air,
                        NO_GLOW,
                    );
                }
            }
            let output = self
                .surface
                .get_current_texture()
                .map_err(|e| JsValue::from_str(&format!("get_current_texture failed: {e}")))?;
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("terra-frame"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("terra-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.01,
                                g: 0.01,
                                b: 0.03,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                // THE SKY FIRST. Terra's world frame is Earth-FIXED, so the inertial catalogue is turned
                // by Greenwich sidereal time: the stars wheel overhead once per SIDEREAL day, four minutes
                // shy of a solar one, which is why they rise earlier each night. Nothing is animated —
                // the same clock that puts the Sun in the sky puts the stars in it.
                if let Some(stars) = self.stars.as_ref() {
                    let gmst = crate::sky::gmst_rad(self.celestial_epoch_s());
                    stars.draw(
                        &self.queue,
                        &mut pass,
                        view_proj,
                        Mat4::from_rotation_y(-gmst as f32),
                        // Camera-relative: `view_proj` has the eye at the origin, so the billboards
                        // hang around the origin too; same sky, none of the absolute-eye rounding.
                        Vec3::ZERO,
                        // Terra does not carry a heliocentric position, so the observer is taken as Sol.
                        // The error is Earth's 1 AU baseline: at most 1 arcsecond of parallax for the
                        // nearest star, against a ~4 arcminute pixel — a thousandth of a pixel. FLAGGED,
                        // and it becomes exact the moment this scene knows where Earth is in its orbit.
                        glam::Vec3::ZERO,
                        // Any distance works (the shader pins depth); this keeps the billboards clear of
                        // the globe so the near plane never cuts into them.
                        1_000.0,
                        self.config.width as f32,
                        self.config.height as f32,
                        80.0,
                    );
                }
                if self.surface_loaded {
                    // **ONE surface** (docs/63): the segment, whose extent is simply what is visible from
                    // here. There is no cross-fade, no depth-fight lift and no "does the cap cover enough
                    // to skip the globe" decision, because all of that existed to mediate between two
                    // meshes drawing the same planet, and there is one.
                    if let (Some(gpu), Some(uni)) =
                        (self.segment_gpu.as_ref(), self.segment_uni.as_ref())
                    {
                        pass.set_pipeline(&self.globe_pipeline);
                        draw(&mut pass, uni, gpu);
                    }
                    // **The cannon**, standing where it was emplaced. Its geometry came from its
                    // assembly and its uniform was written in `build_segment` beside the surface it
                    // stands on; this only draws it.
                    if let (Some(gpu), Some(uni)) =
                        (self.cannon_gpu.as_ref(), self.cannon_uni.as_ref())
                    {
                        if self.cannon_at.is_some() {
                            pass.set_pipeline(&self.globe_pipeline);
                            draw(&mut pass, uni, gpu);
                        }
                    }
                    // **The plants standing near the eye.** Same surface pipeline the ground and the
                    // gun use — a tree is matter with a shape, not a special kind of thing.
                    if let (Some(gpu), Some(uni)) =
                        (self.flora_gpu.as_ref(), self.flora_uni.as_ref())
                    {
                        if self.flora_at.is_some_and(|(_, _, n)| n > 0) {
                            pass.set_pipeline(&self.globe_pipeline);
                            draw(&mut pass, uni, gpu);
                        }
                    }
                } else {
                    pass.set_pipeline(&self.pipeline);
                    for uni in self.shell_unis.iter() {
                        draw(&mut pass, uni, &self.sphere_gpu);
                    }
                }
                // The engine's matter, last: it is emissive and additive, so it brightens whatever it is
                // in front of. The projection scales come from the field of view THIS FRAME was built with
                // (`View::fov_y`) — not a second copy of it, which is how a "one pixel" floor stops being
                // one pixel the moment anyone changes the FOV.
                let proj_y = 1.0 / (cam_fov_y * 0.5).tan() as f32;
                let proj_x = proj_y / aspect.max(1e-3) as f32;
                if self.draw_matter >= 2 {
                    self.matter.draw(
                        &mut pass,
                        &self.queue,
                        view_proj,
                        display_scale() as f32,
                        (proj_x, proj_y),
                        self.config.height as f32,
                    );
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        /// docs/43 Phase 3 — build the displaced cube-sphere globe surface from the loaded rasters. The ocean is
        /// integrated into the same mesh (ocean cells sit at exactly sea level with the water material), so there
        /// is no separate ocean shell and no coast z-fighting. `EXAG` exaggerates relief so it reads on a radius-1
        /// globe (Everest is only ~0.05% of Earth's radius); the true ratio returns with the ground LOD (Phase 5).
        /// **THE ground height at a coordinate** (metres above sea level), and the size of the finest
        /// datum that answered — measured elevation where a streamed tile covers, the shipped raster
        /// otherwise, blended exactly as the mesh blends them.
        ///
        /// ★★ This is an ASSOCIATED function taking its inputs rather than a method, so the segment
        /// builder — which already holds `&self` pieces split across a mutable borrow — calls the SAME
        /// code the camera and the cannon call. That is the whole point of it existing.
        ///
        /// **It replaces three different answers to one question** (Law II), found 2026-08-03:
        ///   1. a closure inside `build_segment`, raster blended with streamed tiles — what the mesh
        ///      draws, i.e. the surface you can actually see;
        ///   2. `ground_disp_at`, raster only with NO tiles and a 3×3 **max over ±0.2° (~22 km)** — a
        ///      "clearance envelope" that held the eye above the highest peak within 22 kilometres;
        ///   3. and (2) again, used to stand the cannon, so **the gun floated above its own ground**
        ///      whenever a hill stood anywhere within 22 km. That was a real bug and it shipped.
        ///
        /// The envelope existed to stop the camera flying into a mountain. That is a CONTACT problem
        /// and it now has a contact answer (`camera_shell_resolve`): the camera is matter, it rests
        /// against the surface and slides along it. Inflating the ground to fake a collision is exactly
        /// the clamp fudge Law V forbids — and it could only ever push the eye straight up.
        fn ground_elev_m(
            elevation: Option<&crate::terra::raster::Raster>,
            tiles: &crate::terra::tiles::TileStore,
            elev_lo: f64,
            elev_hi: f64,
            raster_step_m: f64,
            lat: f64,
            lon: f64,
        ) -> (f64, f64) {
            let e_raster = elevation.map_or(0.0, |r| r.elevation_m_at(lat, lon, elev_lo, elev_hi));
            match tiles.elevation_m_at(lat, lon) {
                Some((e_tile, w)) => {
                    let base = match tiles.pixel_ground_m(lat) {
                        Some(px_m) if px_m > 0.0 && raster_step_m > 0.0 => {
                            raster_step_m * (px_m / raster_step_m).powf(w)
                        }
                        _ => raster_step_m,
                    };
                    ((e_raster + w * (e_tile - e_raster)).max(0.0), base)
                }
                None => (e_raster.max(0.0), raster_step_m),
            }
        }

        /// The pixel size of the shipped elevation raster on the ground, metres — the coarsest datum.
        fn raster_step_m(&self) -> f64 {
            self.elevation.as_ref().map_or(90.0, |r| {
                2.0 * std::f64::consts::PI * self.planet_radius / r.w.max(1) as f64
            })
        }

        /// [`ground_elev_m`] for callers that hold a plain `&self`, in DISPLAY units above the
        /// sea-level sphere (so relief exaggeration is applied, as the mesh applies it).
        fn ground_disp_at(&self, lat: f64, lon: f64) -> f64 {
            let is_land = self
                .landmask
                .as_ref()
                .map(|r| r.land_at(lat, lon))
                .unwrap_or(false);
            if !is_land {
                return 0.0; // the sea surface is at sea level, and it is flat
            }
            let (e, _) = Self::ground_elev_m(
                self.elevation.as_ref(),
                &self.tiles,
                self.elev_range[0],
                self.elev_range[1],
                self.raster_step_m(),
                lat,
                lon,
            );
            e * display_scale() * self.relief_exag
        }

        /// **The camera is MATTER** — a tiny transparent shell obeying the SAME contact law as a grain
        /// (`granular::sweep_shell_resolve`), not a geometric clamp.
        ///
        /// Robin, canonical: *"If the camera isn't material, it can subvert our rules. Let's place a
        /// tiny cube of matter around the camera (transparent) so the camera can't pierce through our
        /// skin."* And, deciding it belongs here rather than in a scene (2026-08-03): *"Camera must
        /// exist in the engine, but can be directed by the scene"*, because **"the engine does a lot of
        /// calculation based on what can be seen, so it must know everything about the camera all the
        /// time."** A camera the scene owns is a camera the engine's own resolution decisions trail by
        /// a frame.
        ///
        /// The law was already built and had exactly ONE consumer — the Ground scene, which is being
        /// deleted. Terra used `alt_m.clamp(min_alt, ..)` plus a 22 km max-filtered ground: two fudges
        /// stacked, and neither can slide. This makes Terra a second consumer of the one law, on the
        /// sphere.
        ///
        /// ★ **A tangent plane is exact at shell scale.** The shell is 0.35 m; over one metre of
        /// tangent offset a 6371 km sphere departs from its own tangent plane by `x²/2R` ≈ **8e-8 m**,
        /// which is eleven million times smaller than the shell. So presenting the sphere to the
        /// heightfield primitive as a local y-up plane is a coordinate change, not an approximation
        /// anybody has to defend.
        ///
        /// CONTACT, not excavation: nudging the eye into a hillside must not blast a crater. (Ram it in
        /// at real speed and the same energy gate a meteor obeys would honestly dig — that is the rule
        /// being universal, not a bug.)
        fn camera_shell_resolve(&self, desired_disp: glam::DVec3) -> glam::DVec3 {
            let ds = display_scale();
            if ds <= 0.0 {
                return desired_disp;
            }
            // The physics is in METRES — the shell is 0.35 m, and a display unit is an Earth radius.
            let to_m = desired_disp / ds;
            let resolved = crate::granular::sweep_shell_on_sphere(
                self.last_eye_m.unwrap_or(to_m),
                to_m,
                self.planet_radius,
                |lat, lon| self.ground_disp_at(lat, lon) / ds,
            );
            resolved * ds
        }

        /// **Resolve the plants standing near the eye into geometry** — the near half of Law IV.
        ///
        /// Robin (2026-08-04): *"These are hues at altitude but must become realistic flora at very low
        /// altitude."* Above `FLORA_ALT_M` a footprint answers with its mixture's albedo and nothing is
        /// instantiated; below it the same footprint answers with the plants themselves. **The plants
        /// were always there** — necessity decides what is RESOLVED, the camera only decides what is
        /// drawn (Law III/IV), and the scatter is derived from position so looking away cannot move a
        /// single one.
        ///
        /// The scene says nothing about any of this. It named Earth; the land cover says what grows.
        fn build_flora(&mut self) {
            /// Above this the plants are smaller than a pixel and the ground's albedo IS the answer.
            /// Not a style choice: at 300 m a 0.35 m tuft subtends about a thousandth of a radian, well
            /// under one pixel of a 60-degree frame, so resolving it could not change the picture.
            const FLORA_ALT_M: f64 = 300.0;
            /// How much matter may stand at once (Law III: the minimal necessary, not all in sight).
            const FLORA_BUDGET: usize = 1200;

            let alt = self.fly.alt_m;
            if alt > FLORA_ALT_M || self.landcover.is_none() || self.flora_kinds.is_empty() {
                self.flora_at = None;
                return;
            }
            // Rebuild only when the ground under the eye has actually moved — the segment's own rule.
            let (lat, lon) = (self.fly.lat, self.fly.lon);
            if let Some((blat, blon, _)) = self.flora_at {
                let moved_m = ((lat - blat).powi(2) + (lon - blon).powi(2)).sqrt() * 111_320.0;
                if moved_m < 2.0 {
                    return;
                }
            }
            // What a viewer at this altitude can actually resolve: the horizon is far, but a tuft is
            // sub-pixel long before that. Scale the radius with altitude and cap it.
            let radius_m = (alt * 8.0).clamp(6.0, 120.0);
            let mats = &self.mats;
            let biome_mix = &self.biome_mix;
            let landcover = self.landcover.as_ref();
            let sited = crate::terra::flora::scatter(
                lat,
                lon,
                radius_m,
                &self.flora_kinds,
                mats,
                |la, lo| {
                    let class = landcover.map_or(0, |r| r.biome_at(la, lo) as usize);
                    biome_mix.get(class).cloned().unwrap_or_default()
                },
                FLORA_BUDGET,
            );
            if sited.is_empty() {
                self.flora_at = Some((lat, lon, 0));
                return;
            }
            // One mesh for the lot: each plant's own assembly mesh, moved to where it stands. The
            // vertices are CAMERA-RELATIVE like every other draw in this scene.
            let ds = display_scale();
            let r_disp = self.planet_radius * ds;
            // ★ Vertices are built about a LOCAL ANCHOR — the surface point under the camera — not
            // about the eye, and not in absolute space. Absolute f32 at Earth's radius has ~0.6 m ULP,
            // which is larger than the plants; and building about the EYE bakes in whichever eye
            // happened to be current, so the patch sits wherever the camera was when it was last
            // rebuilt. The anchor is stored and the per-frame model matrix is `translate(anchor - eye)`,
            // exactly as the segment does it.
            let anchor_m = crate::geo::dir_from_lat_lon(lat, lon)
                * (self.planet_radius + self.ground_disp_at(lat, lon) / ds);
            let eye = anchor_m * ds;
            let mut combined = crate::mesher::Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
            };
            let meshes: Vec<crate::mesher::Mesh> = self
                .flora_kinds
                .iter()
                .map(|k| {
                    let txt = match k.assembly_id.as_str() {
                        "broadleaf-tree-oak" => crate::assembly::compiled::BROADLEAF_TREE_OAK,
                        "conifer-tree-spruce" => crate::assembly::compiled::CONIFER_TREE_SPRUCE,
                        _ => crate::assembly::compiled::GRASS_TUFT,
                    };
                    crate::assembly::compiled::parse(txt).mesh(mats, 6)
                })
                .collect();
            for s in &sited {
                let m = &meshes[s.kind];
                let ground = r_disp + self.ground_disp_at(s.lat_deg, s.lon_deg);
                let model = crate::assembly::place_on_surface(
                    s.lat_deg,
                    s.lon_deg,
                    s.yaw.to_degrees(),
                    ground,
                    ds * s.scale,
                    eye,
                )
                .as_mat4();
                let base = combined.vertices.len() as u32;
                for v in &m.vertices {
                    let p = model.transform_point3(glam::Vec3::from(v.pos));
                    let n = model
                        .transform_vector3(glam::Vec3::from(v.nrm))
                        .normalize_or_zero();
                    let mut nv = *v;
                    nv.pos = p.into();
                    nv.nrm = n.into();
                    combined.vertices.push(nv);
                }
                combined.indices.extend(m.indices.iter().map(|i| i + base));
            }
            let gpu = make_dynamic_mesh(
                &self.device,
                "terra-flora",
                combined.vertices.len(),
                &combined.indices,
            );
            self.queue
                .write_buffer(&gpu.vertex_buf, 0, bytemuck::cast_slice(&combined.vertices));
            log::info!(
                "flora: {} plants resolved within {:.0} m at {:.0} m altitude ({} tris)",
                sited.len(),
                radius_m,
                alt,
                combined.indices.len() / 3
            );
            self.flora_gpu = Some(gpu);
            self.flora_anchor = anchor_m * ds;
            self.flora_at = Some((lat, lon, sited.len()));
        }

        /// How many plants are standing right now — a read, for rigs and the HUD.
        pub fn flora_count(&self) -> u32 {
            self.flora_at.map_or(0, |(_, _, n)| n as u32)
        }

        /// **Build the ONE surface segment** (docs/63) — the collapse of globe + cap into a single mesh.
        ///
        /// Its angular radius is `segment::visible_angle`: literally the surface that is not over the
        /// horizon, times the same margin the cap used so the rim is never an edge. That is the whole
        /// extent rule. There is no cross-fade, no depth-fight lift and no "is the cap covering enough to
        /// skip the globe" decision, because those existed only to mediate between two meshes.
        fn build_segment(
            &mut self,
            view: &crate::terra::fly_camera::View,
            sun_light: Vec3,
            anchor: Vec3,
        ) {
            let res = crate::terra::segment::SegmentRes::new(TERRA_SEG_RINGS, TERRA_SEG_SPOKES);
            let r_disp = self.planet_radius * display_scale();
            let ds = display_scale();
            let exag = self.relief_exag;
            let angle = crate::terra::segment::visible_angle(
                self.fly.alt_m,
                self.planet_radius,
                TERRA_SEG_MARGIN,
            );
            let water_idx = materials::index_of(&self.mats, "water");
            let elev_lo = self.elev_range[0];
            let elev_hi = self.elev_range[1];
            let tiles = &self.tiles;
            let elevation = self.elevation.as_ref();
            let mats = &self.mats;
            let detail = &self.detail;
            let planet_radius = self.planet_radius;
            let octave_budget = self.cap_octave_budget;
            let raster_step_m = self.raster_step_m();
            // How steep this ground measures, as a fraction of what the material can hold — from the
            // TILES where they cover (a metres-long baseline, so the ratio means what its name says) and
            // from the raster otherwise. Same rule as the cap's, sampled once for the segment.
            let tier_slope = {
                let (lat, lon) = (self.fly.lat, self.fly.lon);
                let from_tiles = self.tiles.pixel_ground_m(lat).and_then(|px_m| {
                    let run = (2.0 * px_m).max(1.0);
                    let dlat = run / 2.0 / 111_320.0;
                    let dlon = dlat / lat.to_radians().cos().abs().max(1e-6);
                    let e = |la: f64, lo: f64| self.tiles.elevation_m_at(la, lo).map(|(e, _)| e);
                    let (n, s_, e_, w_) = (
                        e(lat + dlat, lon)?,
                        e(lat - dlat, lon)?,
                        e(lat, lon + dlon)?,
                        e(lat, lon - dlon)?,
                    );
                    Some((((n - s_) / run).powi(2) + (((e_ - w_) / run).powi(2))).sqrt())
                });
                from_tiles.unwrap_or_else(|| {
                    let d = 360.0 / self.elevation.as_ref().map_or(2048, |r| r.w.max(1)) as f64;
                    let e_at = |la: f64, lo: f64| {
                        self.elevation
                            .as_ref()
                            .map_or(0.0, |r| r.elevation_m_at(la, lo, elev_lo, elev_hi))
                    };
                    let run = (2.0 * raster_step_m).max(1.0);
                    let dn = (e_at(lat + d, lon) - e_at(lat - d, lon)) / run;
                    let de = (e_at(lat, lon + d) - e_at(lat, lon - d)) / run;
                    (dn * dn + de * de).sqrt()
                })
            };
            let sampler = crate::terra::globe_mesh::SurfaceSampler::new(
                &self.mats,
                &self.biome_mix,
                self.landmask.as_ref(),
                self.elevation.as_ref(),
                self.landcover.as_ref(),
                self.elev_range,
                ds,
                exag,
            )
            // The clock the sky is drawn with, so the leaves and the light agree about the date.
            .at_epoch(self.celestial_epoch_s());
            // A cache of the view, the same rule the tiers use: re-derive only when re-deriving would
            // change something. Anchored to the surface point under the camera, so the eye moving is
            // carried by the model matrix and touches no vertex.
            // Centred on what the camera LOOKS at, not on its own nadir: the rings concentrate at the
            // centre, so an oblique view would otherwise spend its fine samples on ground behind the eye.
            let look = crate::terra::segment::look_centre(view.eye, view.forward, r_disp);
            let fresh = crate::terra::ground_cap::CapTierBuild {
                center: look,
                anchor: look * r_disp,
                cap_angle: angle,
                cell_m: (angle * self.planet_radius / res.rings as f64).max(1e-6),
            };
            let built = match self.segment_built {
                Some(b) if crate::terra::ground_cap::tier_is_current(&b, &fresh) => b,
                _ => fresh,
            };
            let rebuild = self.segment_built != Some(built);
            let mut verts = std::mem::take(&mut self.segment_verts);
            if rebuild {
                let rings_f = res.rings as f64;
                // **ONE answer to "how high is the ground here, and how fine is the data that says
                // so"** — used by the vertex's own displacement AND by the appearance integral's
                // probe below. Two copies would be two answers to one question (Law II), and the
                // probe's copy is exactly where the tile's measured relief would silently go missing:
                // a probe that read only the raster would report a smooth bilinear ramp and the
                // integral would conclude, wrongly, that the ground is flat.
                let ground_m = |lat: f64, lon: f64| -> (f64, f64) {
                    Self::ground_elev_m(elevation, tiles, elev_lo, elev_hi, raster_step_m, lat, lon)
                };
                let mut scratch = crate::terra::appearance::Moments::new();
                // What this machine can afford this rebuild — measured, not declared. See
                // `resolution::WorkBudget`: it settles lower on a slow device and higher on a fast
                // one, and stops growing once the grid reaches the data's own resolution.
                let probe_side = if self.appearance_probes_pinned > 0 {
                    self.appearance_probes_pinned
                } else {
                    self.appearance_budget.side()
                };
                // The finest grid the DATA under this segment could support, over the whole rebuild —
                // the budget's convergence ceiling (see `WorkBudget::observe`).
                let max_want = std::cell::Cell::new(1usize);
                let t_appearance = crate::clock::now_seconds();
                let sample = |dir: glam::DVec3| -> crate::terra::globe_mesh::SurfaceSample {
                    let point = sampler.sample(dir);
                    let mut off = point.offset;
                    let mi = point.material as usize;
                    // Measured beats generated, exactly as the cap does it — one surface, one rule.
                    let (lat, lon) = crate::geo::lat_lon_from_dir(dir);
                    let (elev_m, base_feature_m) = ground_m(lat, lon);
                    if mi != water_idx {
                        off = elev_m * ds * exag;
                    }
                    // **The mesh's own cell here** — and on a polar segment that VARIES, finer toward the
                    // centre where the rings bunch. Differentiating the squared ring spacing gives
                    // `2·sqrt(a·cap)/rings` radians per ring at angular distance `a`, so the generated
                    // relief is bounded by what the mesh can actually carry AT THIS POINT rather than by
                    // one number for the whole patch, which is what a uniform grid forced.
                    let a_ang = dir.dot(built.center).clamp(-1.0, 1.0).acos();
                    // ★ The floor is the INNERMOST RING's own spacing (`cap/rings²`), not an epsilon.
                    // Squared ring spacing sends the local step to zero at the centre, and a cell size of
                    // zero lets the octave count run away: `log2(base/cell)` grew past 11, the finest
                    // generated wavelength fell under a millimetre, and `value_noise`'s lattice
                    // coordinate (position × frequency, ~1e7 × 1e3) overflowed i32 and panicked. The
                    // centre does not have an infinitely fine cell — it has the first ring's.
                    let inner_m = built.cap_angle / (rings_f * rings_f) * planet_radius;
                    let cell_m = (2.0 * (a_ang * built.cap_angle).sqrt() / rings_f * planet_radius)
                        .max(inner_m)
                        .max(1e-3);
                    // Ocean keeps its own displacement rule (sea level, flat); land gets the generated
                    // sub-raster relief. Both then have their APPEARANCE integrated, because a coastal
                    // cell really is part land and part sea, and point-sampling it as one or the other
                    // is the jagged-coastline bug the integral exists to end.
                    if mi != water_idx {
                        let m = &mats[mi];
                        let mu = m.friction_coefficient as f64;
                        let h_crit = crate::granular::critical_bank_height(
                            m.fracture_strength,
                            m.density,
                            9.81,
                        ) as f64;
                        let octaves =
                            crate::surface_detail::detail_octaves(detail, cell_m, base_feature_m)
                                .min((base_feature_m / cell_m).max(1.0).log2())
                                .min(octave_budget);
                        if octaves > 0.0 {
                            let frac = if mu > 0.0 {
                                (tier_slope / mu).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            let px = lon.to_radians() * planet_radius * lat.to_radians().cos();
                            let pz = lat.to_radians() * planet_radius;
                            let relief = crate::surface_detail::micro_relief_m(
                                px,
                                pz,
                                base_feature_m,
                                octaves,
                                mu,
                                h_crit,
                                frac,
                            );
                            off += relief * ds * exag;
                        }
                    }
                    // **THE APPEARANCE INTEGRAL** (docs/63). The mesh carries this cell's mean shape;
                    // everything finer than the cell — the material MIXTURE and the slope SPREAD — is
                    // integrated here and carried as colour and roughness. Without it those sixteen
                    // thousand measured tile pixels per cell reach nothing.
                    max_want.set(
                        max_want
                            .get()
                            .max((cell_m / base_feature_m.max(1e-9)).floor().max(1.0) as usize),
                    );
                    let a = crate::terra::appearance::integrate_on_sphere(
                        &mut scratch,
                        dir,
                        planet_radius,
                        cell_m,
                        base_feature_m,
                        probe_side,
                        mats,
                        |la, lo| (ground_m(la, lo).0, sampler.material_at(la, lo) as usize),
                    );
                    // An empty mixture means the footprint was finer than the data under it, so there
                    // was nothing to integrate and the point sample IS the answer — with `rough = 0`,
                    // which shades as exactly the Lambert it always did. Not a fallback for failure:
                    // it is the correct result, arrived at without doing the work.
                    if a.mix.is_empty() {
                        crate::terra::globe_mesh::SurfaceSample {
                            offset: off,
                            ..point
                        }
                    } else {
                        crate::terra::globe_mesh::SurfaceSample {
                            albedo: a.albedo,
                            offset: off,
                            material: a.material as u32,
                            rough: a.sigma_rad() as f32,
                        }
                    }
                };
                crate::terra::segment::fill_segment(
                    &mut verts,
                    built.center,
                    view.east,
                    view.north,
                    built.anchor,
                    r_disp,
                    built.cap_angle,
                    res,
                    sample,
                );
                // One rebuild is one unit of work: fold its real cost back into the budget. The side
                // REALLY used is the smaller of what we allowed and what the data supports — passing
                // the allowance alone would let a cheap, data-bound rebuild argue for a bigger budget.
                if self.appearance_probes_pinned == 0 {
                    self.appearance_budget.observe(
                        probe_side.min(max_want.get()),
                        (crate::clock::now_seconds() - t_appearance) * 1000.0,
                    );
                }
                if let Some(gpu) = self.segment_gpu.as_ref() {
                    self.queue
                        .write_buffer(&gpu.vertex_buf, 0, bytemuck::cast_slice(&verts));
                }
                self.segment_built = Some(built);
            }
            // **Stand the cannon on the surface.** Placement only — the assembly supplied the shape
            // and `assembly::place_on_surface` supplies the transform. The scene names a coordinate and
            // a bearing; it does not build a basis, which is how it built a MIRRORED one.
            if let (Some(uni), Some((glat, glon, bearing))) =
                (self.cannon_uni.as_ref(), self.cannon_at)
            {
                let model = crate::assembly::place_on_surface(
                    glat,
                    glon,
                    bearing,
                    r_disp + self.ground_disp_at(glat, glon),
                    ds,
                    view.eye,
                )
                .as_mat4();
                write_space_uniform(
                    &self.queue,
                    uni,
                    view.vp_rel,
                    model,
                    sun_light,
                    [1.0, 1.0, 1.0, 1.0],
                    [anchor.x, anchor.y, anchor.z, self.veil_column_fraction()],
                    self.air(),
                    [0.0, 0.0, 0.0, 0.0],
                );
            }
            // **The plants.** Their vertices are measured from a local anchor, so the per-frame model
            // matrix carries them to camera-relative space — the same thing the segment does, and for
            // the same two reasons: f32 has ~0.6 m ULP at Earth's radius, and a mesh built about one
            // eye is wrong for every other.
            if let (Some(uni), Some((_, _, n))) = (self.flora_uni.as_ref(), self.flora_at) {
                if n > 0 {
                    let model =
                        glam::DMat4::from_translation(self.flora_anchor - view.eye).as_mat4();
                    write_space_uniform(
                        &self.queue,
                        uni,
                        view.vp_rel,
                        model,
                        sun_light,
                        [1.0, 1.0, 1.0, 1.0],
                        [anchor.x, anchor.y, anchor.z, self.veil_column_fraction()],
                        self.air(),
                        [0.0, 0.0, 0.0, 0.0],
                    );
                }
            }
            if let Some(uni) = self.segment_uni.as_ref() {
                let model = glam::DMat4::from_translation(built.anchor - view.eye).as_mat4();
                write_space_uniform(
                    &self.queue,
                    uni,
                    view.vp_rel,
                    model,
                    sun_light,
                    [1.0, 1.0, 1.0, 1.0],
                    [anchor.x, anchor.y, anchor.z, self.veil_column_fraction()],
                    self.air(),
                    glow_of(&crate::planet::body(&self.body_id)),
                );
            }
            self.segment_verts = verts;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{body, gravity, materials, mesher, world};

    /// **A live body crossing its resolution distance is the engine's cue to resolve it as matter**
    /// (the cue to resolve it as matter): the check must report the FIRST eligible body inside
    /// `accretion::resolution_distance` of the planet, together with the exact body-centric
    /// (offset, relative velocity) pair: the same f64 SI numbers the impactor's `BodyPlacement`
    /// hands `assemble_from_relaxed_n`. Body-centric, never heliocentric (f32 collapses at 1.5e11 m). The planet is passed
    /// by the index its declared ROLE resolves to (docs/58), never assumed to sit at index 1, and a
    /// permuted body order must report the same crossing with the same geometry.
    #[test]
    fn a_body_crossing_its_resolution_distance_is_reported_with_its_live_geometry() {
        use crate::orbit::Body;
        use glam::DVec3;
        const M_EARTH: f64 = 5.972e24;
        const R_EARTH: f64 = 6.371e6;
        const M_MOON: f64 = 7.342e22;
        let f = crate::accretion::RESOLVE_TIDAL_FRACTION;
        let resolve_at = crate::accretion::resolution_distance(M_EARTH, R_EARTH, M_MOON, f);
        let sun = Body {
            pos: DVec3::ZERO,
            vel: DVec3::ZERO,
            mass: 1.989e30,
        };
        let earth = Body {
            pos: DVec3::new(1.496e11, 0.0, 0.0),
            vel: DVec3::new(0.0, 29_780.0, 0.0),
            mass: M_EARTH,
        };
        // One moon just OUTSIDE the threshold, one just INSIDE: the crossing is at index 3 only.
        let outside = Body {
            pos: earth.pos + DVec3::new(0.0, 1.05 * resolve_at, 0.0),
            vel: earth.vel + DVec3::new(-1022.0, 0.0, 0.0),
            mass: M_MOON,
        };
        let inside = Body {
            pos: earth.pos + DVec3::new(0.95 * resolve_at, 0.0, 0.0),
            vel: earth.vel + DVec3::new(0.0, -3000.0, 0.0),
            mass: M_MOON,
        };
        let bodies = [sun, earth, outside, inside];

        let (i, offset, rel_vel) =
            crate::live_resolution_crossing(&bodies, 1, R_EARTH, &[true; 4], f)
                .expect("a body inside the resolution distance must be reported");
        assert_eq!(
            i, 3,
            "the body OUTSIDE the threshold must not fire; the one INSIDE must"
        );
        // EXACT equality: the pair is handed straight to the SPH assembly, so nothing may drift here.
        assert_eq!(
            offset,
            bodies[3].pos - bodies[1].pos,
            "body-centric offset, bit-exact"
        );
        assert_eq!(
            rel_vel,
            bodies[3].vel - bodies[1].vel,
            "body-centric velocity, bit-exact"
        );

        // A PERMUTED body order reports the same crossing with the same geometry (docs/58: bodies
        // are addressed by declared role, so the planet's slot in the list must not matter).
        let permuted = [outside, sun, inside, earth];
        let (pi, poffset, prel_vel) =
            crate::live_resolution_crossing(&permuted, 3, R_EARTH, &[true; 4], f)
                .expect("the same crossing must be found wherever the planet sits");
        assert_eq!(pi, 2, "the inside body, at its permuted index");
        assert_eq!(poffset, offset, "identical body-centric offset, bit-exact");
        assert_eq!(
            prel_vel, rel_vel,
            "identical body-centric velocity, bit-exact"
        );

        // Nobody inside ⇒ no crossing (the point-mass representation is still honest).
        assert!(
            crate::live_resolution_crossing(&bodies[..3], 1, R_EARTH, &[true; 3], f).is_none(),
            "a body above the threshold is still a body"
        );
        // An ineligible body (already materialized, or already handed to the SPH) is skipped.
        let mut eligible = [true; 4];
        eligible[3] = false;
        assert!(
            crate::live_resolution_crossing(&bodies, 1, R_EARTH, &eligible, f).is_none(),
            "an ineligible body must not be reported twice"
        );
    }

    #[test]
    fn metres_per_pixel_matches_frustum_geometry() {
        // The visible slice of the world at the focal plane is 2·d·tan(fov/2) metres tall; one pixel
        // is that divided by the viewport height. Check the closed form and its scaling behaviour —
        // this is the pure math behind the HUD scale bar (same on terrain and in space).
        let fov = 0.9_f64;
        let vh = 1000.0_f64;
        let d = 100.0_f64;
        let mpp = crate::metres_per_pixel_at(d, fov, vh);
        let expected = 2.0 * d * (fov * 0.5).tan() / vh;
        assert!(
            (mpp - expected).abs() < 1e-12,
            "closed form: {mpp} vs {expected}"
        );
        // Linear in distance: twice as far away ⇒ twice the metres per pixel (zooming out coarsens).
        assert!(
            (crate::metres_per_pixel_at(2.0 * d, fov, vh) - 2.0 * mpp).abs() < 1e-12,
            "scale must be linear in focal distance"
        );
        // Inverse in viewport height: a taller viewport packs the same slice into more pixels.
        assert!(
            (crate::metres_per_pixel_at(d, fov, 2.0 * vh) - 0.5 * mpp).abs() < 1e-12,
            "scale must be inverse in viewport height"
        );
        // Degenerate viewport is guarded (no divide-by-zero into the HUD).
        assert_eq!(crate::metres_per_pixel_at(d, fov, 0.0), 0.0);
    }

    #[test]
    fn material_database_loads() {
        let mats = materials::load();
        // A FLOOR, not a fixed count. This asserted exactly 24 and went red the moment the atmospheric
        // gases were catalogued — pinning the size of a catalogue that is meant to grow turns standing
        // procedure ("source and catalogue any new substance") into a chore that breaks a test each time.
        // What matters is that the database loads and that the materials the engine names are present.
        assert!(
            mats.len() >= 24,
            "the catalogue should not SHRINK (got {})",
            mats.len()
        );
        for id in [
            "granite",
            "basalt",
            "peridotite",
            "iron",
            "water",
            "air",
            "carbon_dioxide",
        ] {
            assert!(
                mats.iter().any(|m| m.id == id),
                "{id} must be catalogued — the engine names it"
            );
        }
        // `rubber` — the tyre compound; the go-kart's grip, damping and hysteresis live in this datum.
        // It used to carry NO thermal block at all, on the reasoning that rubber does not melt so
        // melt_point had no honest value. The reasoning was right and the encoding was not: "does not
        // melt" is a different claim from "nothing is known", and lumping them together threw away a heat
        // capacity rubber certainly has — which is how three call sites ended up inventing one (840 in
        // impact.rs, 1000 in aggregate.rs, 1000 in matter.rs). It now says the true thing: a specific
        // heat, a pyrolysis temperature, and no melting point.
        let rubber = &mats[materials::index_of(&mats, "rubber")];
        assert!(
            rubber.specific_heat().is_some(),
            "rubber has a heat capacity like everything else"
        );
        assert_eq!(
            rubber.melt_point(),
            None,
            "but no melting point — it pyrolyses"
        );
        assert!(
            rubber.decomposition_point().is_some(),
            "and it says where it breaks down"
        );
        for id in ["granite", "dirt", "grass", "iron", "nickel", "rubber"] {
            let i = materials::index_of(&mats, id);
            assert!(mats[i].density > 0.0, "{id} must have positive density");
        }
        // Metals carry a real elastic modulus — the probe's cohesive-bond stiffness derives from it.
        let iron = materials::index_of(&mats, "iron");
        assert!(
            mats[iron].youngs_modulus > 1.0e11,
            "iron's Young's modulus must be ~200 GPa (got {})",
            mats[iron].youngs_modulus
        );
        let g = mats[materials::index_of(&mats, "granite")].density;
        let d = mats[materials::index_of(&mats, "dirt")].density;
        assert!(g > d, "granite ({g}) should be denser than dirt ({d})");
    }

    #[test]
    fn world_column_is_density_sorted_light_skin_over_heavy_depths() {
        // The surface patch is gravitationally sorted like the real Earth: a light organic skin on top,
        // then progressively DENSER matter with depth, down to the iron core. (This supersedes the old
        // granite/dirt/grass game world, which the engine no longer generates; the precise material
        // ORDER — grass → basalt → peridotite → iron — is asserted by world::tests::
        // column_is_earths_real_layers_top_to_bottom. Here we assert the distinct honest property:
        // scanning DOWN a column, density never decreases, and several distinct layers are stacked.)
        let mats = materials::load();
        let w = world::generate(&mats);

        let (x, z) = (w.w as i32 / 2, w.d as i32 / 2);
        assert!(w.is_solid(x, 0, z), "world must be solid at the bottom");
        let top = w.surface_top_voxel(x, z).expect("solid column at centre");

        let mut prev_density = 0.0f32;
        let mut layers = 0usize;
        let mut last_mat: Option<usize> = None;
        for y in (0..top).rev() {
            let m = w
                .material_at(x, y, z)
                .expect("solid below the surface top (no holes)");
            let d = mats[m].density;
            assert!(
                d >= prev_density - 1e-3,
                "denser matter must sit deeper: {} (ρ={d}) sits below ρ={prev_density}",
                mats[m].id
            );
            prev_density = d;
            if last_mat != Some(m) {
                layers += 1;
                last_mat = Some(m);
            }
        }
        assert!(
            layers >= 3,
            "the column must show multiple stacked layers, not one slab (got {layers})"
        );
    }

    #[test]
    fn mesher_produces_valid_surface() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = mesher::build(&w, &mats);
        assert!(!mesh.vertices.is_empty(), "mesh must have vertices");
        assert_eq!(mesh.vertices.len() % 4, 0, "vertices come in quads of 4");
        assert_eq!(
            mesh.indices.len() % 6,
            0,
            "indices come in 2 triangles (6) per quad"
        );
        let vmax = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < vmax), "indices in range");
    }

    #[test]
    fn sphere_mesh_is_valid() {
        let (rings, sectors) = (16, 24);
        let mesh = mesher::build_uv_sphere(3.0, 0, [0.5, 0.5, 0.5], rings, sectors);
        assert_eq!(mesh.vertices.len(), (rings + 1) * (sectors + 1));
        assert_eq!(mesh.indices.len(), rings * sectors * 6);
        let vmax = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < vmax));
        // Every vertex sits on the sphere of the requested radius.
        for v in &mesh.vertices {
            let r = (v.pos[0].powi(2) + v.pos[1].powi(2) + v.pos[2].powi(2)).sqrt();
            assert!((r - 3.0).abs() < 1e-3, "vertex on sphere surface");
        }
    }

    #[test]
    fn sphere_falls_toward_world_and_rests() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let field = gravity::MassField::build(&w, &mats, 4);
        let c = w.center();
        let radius = 1.0;
        let surf = w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        let spawn = glam::Vec3::new(0.0, surf + radius + 8.0, 0.0);
        let mut s = body::Sphere::new(spawn, 5.0, radius);
        let start_y = s.pos.y;

        // Fast-forward: the accel is tiny and smooth, so large steps integrate fine.
        let dt = 5.0;
        for _ in 0..8000 {
            let accel = field.acceleration_at(s.pos, 6.0);
            s.integrate(accel, dt);
            s.collide(&w, accel, dt);
            if s.resting {
                break;
            }
        }
        assert!(s.pos.y < start_y, "sphere should fall downward");
        assert!(s.resting, "sphere should come to rest on the surface");
        assert!(
            (s.pos.y - (surf + radius)).abs() < 1.0,
            "rests on the surface"
        );
    }

    #[test]
    fn raycast_hits_terrain_from_above() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let c = w.center();
        let origin = glam::Vec3::new(0.0, c.y + 50.0, 0.0);
        let hit = w.raycast(origin, glam::Vec3::NEG_Y, 1000.0);
        assert!(hit.is_some(), "a downward ray should hit the terrain");
        let (_x, _y, _z, p) = hit.unwrap();
        let surf = w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        assert!((p.y - surf).abs() < 2.0, "hit near the surface height");
    }

    #[test]
    fn surface_nets_is_smooth_and_valid() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = mesher::build_surface_nets(&w, &mats);
        assert!(
            !mesh.vertices.is_empty(),
            "surface nets should produce geometry"
        );
        assert_eq!(mesh.indices.len() % 3, 0);
        let vmax = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < vmax), "indices in range");
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.pos.iter().chain(v.nrm.iter()).all(|c| c.is_finite())),
            "no NaN/inf in positions or normals"
        );
        // Smooth: unlike the cube mesher, many normals are NOT axis-aligned.
        let non_axis = mesh
            .vertices
            .iter()
            .filter(|v| {
                let n = v.nrm;
                !(n[0].abs() > 0.99 || n[1].abs() > 0.99 || n[2].abs() > 0.99)
            })
            .count();
        assert!(non_axis > 0, "surface nets should yield smooth normals");
    }

    #[test]
    fn surface_nets_mesh_is_closed() {
        // "Hollow from two sides" would mean an open surface. A closed (watertight) mesh shares every
        // undirected edge an even number of times; a boundary edge (odd count) is a hole.
        use std::collections::HashMap;
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = mesher::build_surface_nets(&w, &mats);
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for tri in mesh.indices.chunks_exact(3) {
            for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_insert(0) += 1;
            }
        }
        let boundary = edges.values().filter(|&&c| c % 2 != 0).count();
        assert_eq!(
            boundary, 0,
            "mesh must be closed (watertight); found {boundary} boundary edges"
        );
    }
}
