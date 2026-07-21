//! **The engine driven by a definition** (`docs/53`) — no scene struct, no canvas, no `wasm_bindgen`.
//!
//! ## Why this exists
//!
//! Deleting the terrain scene (docs/50) left three built-and-verified systems with **zero** production
//! consumers — `matter::MatterSim` (the shared matter path), `resolution::ResolutionField` (docs/49's
//! camera-driven resolution) and the voxel `world::World` — while every test kept passing (docs/46
//! ledger row 15). That is docs/48's wiring pattern at its sharpest: physics wired into one place, and
//! then that place deleted.
//!
//! Robin's diagnosis: *"this is why we make the engine standalone, with external definitions."* The
//! failure was structural — capability was reachable only THROUGH a scene, so a scene's deletion took it
//! down. Here capability is reached from a `World` definition the engine loads, which is a file. Nothing
//! is orphaned by deleting a scene because no scene owns it.
//!
//! ## What it is not
//!
//! Not a renderer and not a scene. It builds the world, applies declared events through the SHARED
//! primitives, and steps. Anything that wants pixels supplies its own host — the browser today, a native
//! window later (docs/52). Keeping this headless is what makes it natively testable, which is the
//! property the scene structs never had.

use crate::gravity::MassField;
use crate::materials::Material;
use crate::matter::MatterSim;
use crate::resolution::{Effect, ResolutionField};
use crate::terra::world_def::{GroundDef, GroundEvent, World as WorldDef};
use glam::Vec3;

/// A running ground simulation built from a definition.
pub struct Simulation {
    pub world: crate::world::World,
    pub matter: MatterSim,
    pub resolution: ResolutionField,
    field: MassField,
    def: GroundDef,
    materials: Vec<Material>,
    /// Effects materialised so far — the docs/49 Analytic→Resolved hand-off, counted.
    resolved_total: usize,
    /// Grains ever created (impact excavation + effect materialisation).
    created_total: usize,
}

impl Simulation {
    /// Build from a parsed `"ground"` world. The voxel world is the procedural surface patch; the
    /// definition declares the observer, the gravity the analytic effects fall under, and the events.
    pub fn from_definition(def: &WorldDef, materials: Vec<Material>) -> Result<Self, String> {
        let ground = def
            .ground
            .clone()
            .ok_or_else(|| "not a ground world: no `ground` block".to_string())?;
        // The SURFACE comes from the definition too (docs/54) — size, relief, sea level and strata.
        // Omitted ⇒ declared defaults, which are voxel-identical to the old hardcoded patch.
        let world = crate::world::generate_from(&ground.surface, &materials);
        let field = MassField::build(&world, &materials, 8);
        let mut sim = Simulation {
            world,
            matter: MatterSim::new(60_000),
            resolution: ResolutionField::new(Default::default()),
            field,
            def: ground,
            materials,
            resolved_total: 0,
            created_total: 0,
        };
        sim.apply_events();
        Ok(sim)
    }

    /// Convenience: parse JSON and build.
    pub fn from_json(json: &str, materials: Vec<Material>) -> Result<Self, String> {
        let def = WorldDef::parse(json)?;
        Self::from_definition(&def, materials)
    }

    /// Apply the declared events. Impacts go straight through the shared `MatterSim::impact`; ejecta
    /// become analytic effects for the resolution field to hand off when they enter view.
    fn apply_events(&mut self) {
        for ev in self.def.events.clone() {
            match ev {
                GroundEvent::Impact { at_m, direction, energy_j } => {
                    self.created_total += self.matter.impact(
                        &mut self.world,
                        &self.materials,
                        Vec3::from_array(at_m),
                        Vec3::from_array(direction),
                        energy_j,
                    );
                }
                GroundEvent::Ejecta { at_m, velocity_ms, radius_m, grain_radius_m, material } => {
                    self.resolution.add(Effect {
                        center: Vec3::from_array(at_m),
                        velocity: Vec3::from_array(velocity_ms),
                        radius: radius_m,
                        grain_radius: grain_radius_m,
                        material,
                    });
                }
            }
        }
    }

    /// One step: the docs/49 hand-off (analytic effects propagate by math and materialise when seen),
    /// then the shared matter step. Returns how many effects resolved this step.
    pub fn step(&mut self, dt: f32) -> usize {
        let camera = Vec3::from_array(self.def.camera_m);
        let view_r = self.def.view_radius_m;
        let gravity = Vec3::new(0.0, -self.def.gravity_ms2, 0.0);
        let before = self.matter.particle_count();
        let resolved = self.resolution.update(
            &mut self.matter,
            &self.materials,
            camera,
            gravity,
            dt,
            |c, _r| (c - camera).length() < view_r,
        );
        self.resolved_total += resolved;
        // Effects materialise grains inside `update`; count the increase so the accounting is complete.
        self.created_total += self.matter.particle_count().saturating_sub(before);
        self.matter.step(&mut self.world, &self.field, &[], dt);
        resolved
    }

    /// Live particle count in the shared matter sim.
    pub fn particle_count(&self) -> usize {
        self.matter.particle_count()
    }
    /// Effects still propagating analytically (off-camera physics that is happening but not simulated).
    pub fn analytic_count(&self) -> usize {
        self.resolution.analytic_count()
    }
    /// Every grain this simulation has ever created — excavated by an impact or materialised from an
    /// effect. Needed to tell "the grains went back into the world" from "the grains were culled off the
    /// patch", which the live particle count alone cannot distinguish.
    pub fn created_total(&self) -> usize {
        self.created_total
    }

    /// Effects handed off from analytic to resolved since construction.
    pub fn resolved_total(&self) -> usize {
        self.resolved_total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// **Ledger row 15, paid.** An impact declared in DATA must reach `MatterSim` and make real
    /// particles. Before this, `MatterSim` had zero production callers: verified physics that nothing
    /// ran. The definition is the consumer now, and it is a file rather than a scene struct.
    #[test]
    fn a_declared_impact_drives_the_shared_matter_path() {
        let json = r#"{
          "name":"ground test","type":"ground",
          "ground":{ "camera_m":[0,20,0], "view_radius_m":2000,
            "events":[{"kind":"impact","at_m":[0,0,0],"energy_j":3.0e7}] }
        }"#;
        let sim = Simulation::from_json(json, mats()).expect("ground world builds");
        assert!(
            sim.particle_count() > 0,
            "a declared impact must materialise matter through MatterSim; got {} particles",
            sim.particle_count()
        );
    }

    /// **docs/49 end to end, from data.** An effect OUT of view is tracked and propagated by cheap math
    /// with no particles — existence is not gated by the camera — and materialises the moment it enters
    /// view. This is the whole Analytic→Resolved hand-off, driven by a definition.
    #[test]
    fn an_off_camera_effect_stays_analytic_then_resolves_when_it_enters_view() {
        // Geometry kept INSIDE the voxel patch (96 m wide, centred ⇒ |x| ≲ 48). `matter::step` culls
        // particles that drift off the world, so an effect resolving outside the footprint spawns grains
        // that are removed in the same step — matter that appears and vanishes. The first version of this
        // test resolved at x≈150 and looked green because it only asserted that the effect RESOLVED.
        // Camera at the origin, view radius 20 m; the ejecta starts at x=40 closing at 10 m/s, so it
        // crosses at t = 2.0 s (step ~20), well inside the 40-step window and well inside the patch.
        let json = r#"{
          "name":"ejecta","type":"ground",
          "ground":{ "camera_m":[0,0,0], "view_radius_m":20, "gravity_ms2":0,
            "events":[{"kind":"ejecta","at_m":[40,0,0],"velocity_ms":[-10,0,0],
                       "radius_m":3,"grain_radius_m":0.5}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        assert_eq!(sim.analytic_count(), 1, "the effect is TRACKED before it is ever seen");
        assert_eq!(sim.particle_count(), 0, "and costs no particles while out of view");

        let mut resolved_at = None;
        for i in 0..40 {
            if sim.step(0.1) > 0 {
                resolved_at = Some(i);
                break;
            }
        }
        let i = resolved_at.expect("the effect must resolve once it enters view");
        assert!(i > 0, "it must NOT resolve on the first step — it starts far outside the view radius");
        assert_eq!(sim.analytic_count(), 0, "resolved effects leave analytic tracking");
        assert_eq!(sim.resolved_total(), 1);
        // THE ASSERTION THIS TEST WAS MISSING. "It resolved" is a state change; the point is that matter
        // exists afterwards. Without this the hand-off can spawn grains that are culled in the same step
        // and the test still passes — the hollow-green failure this whole module exists to prevent.
        assert!(
            sim.particle_count() > 0,
            "materialising an effect must PRODUCE MATTER; got {} particles",
            sim.particle_count()
        );
    }

    /// The camera changes REPRESENTATION, never EXISTENCE (docs/49 / Law 4). An effect that never enters
    /// view must still be tracked and propagated — it must not silently vanish because nobody looked.
    #[test]
    fn an_effect_that_is_never_seen_is_still_tracked_and_never_materialised() {
        let json = r#"{
          "name":"unseen","type":"ground",
          "ground":{ "camera_m":[0,0,0], "view_radius_m":10, "gravity_ms2":0,
            "events":[{"kind":"ejecta","at_m":[5000,0,0],"velocity_ms":[200,0,0],
                       "radius_m":3,"grain_radius_m":0.5}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        for _ in 0..50 {
            sim.step(0.1);
        }
        assert_eq!(sim.resolved_total(), 0, "it never entered view, so it was never simulated");
        assert_eq!(sim.analytic_count(), 1, "but it is STILL TRACKED — looking away changes nothing");
        assert_eq!(sim.particle_count(), 0);
    }

    /// The SURFACE is declared too (docs/54): a definition that asks for a different ground must get
    /// one. Without this the terrain block could be ignored and every test would still pass.
    #[test]
    fn the_definition_shapes_the_ground_it_runs_on() {
        let flat = Simulation::from_json(r#"{
          "name":"flat","type":"ground",
          "ground":{ "surface":{ "amplitude_m":0.0, "sea_level_m":0.0 } }
        }"#, mats()).expect("builds");
        let tops: Vec<i32> = (0..flat.world.w as i32)
            .map(|x| flat.world.surface_top_voxel(x, 0).unwrap_or(-1))
            .collect();
        assert!(tops.windows(2).all(|p| p[0] == p[1]), "a declared flat world must be flat");

        let rolling = Simulation::from_json(
            r#"{"name":"rolling","type":"ground","ground":{}}"#, mats()).expect("builds");
        let tops: Vec<i32> = (0..rolling.world.w as i32)
            .map(|x| rolling.world.surface_top_voxel(x, 0).unwrap_or(-1))
            .collect();
        assert!(tops.windows(2).any(|p| p[0] != p[1]), "the default world has real relief");
    }

    /// A definition with no events must do nothing. Guards against the engine quietly supplying a
    /// default scene — the failure mode where "it works" without the data driving anything.
    #[test]
    fn an_empty_ground_definition_does_nothing() {
        let sim = Simulation::from_json(
            r#"{"name":"empty","type":"ground","ground":{}}"#, mats()).expect("builds");
        assert_eq!(sim.particle_count(), 0);
        assert_eq!(sim.analytic_count(), 0);
    }

    /// A world that is not a ground world must be REFUSED, not silently treated as an empty one.
    #[test]
    fn a_non_ground_world_is_refused() {
        let err = match Simulation::from_json(r#"{"name":"x","type":"impact","impact":{}}"#, mats()) {
            Err(e) => e,
            Ok(_) => panic!("must not build a ground sim from an impact world"),
        };
        assert!(err.contains("ground"), "the error should say what was wrong: {err}");
    }
}
