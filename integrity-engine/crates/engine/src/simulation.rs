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
    name: String,
    planet_mass: f64,
    planet_radius: f64,
    surface_g: f32,
    /// Surface air pressure (Pa) — EMERGENT from the planet's declared atmosphere mass
    /// (`LayeredBody::surface_pressure`), the same value the sky's Rayleigh optics use. Feeds the drag a
    /// meteor feels flying through this air (docs/48). Zero for an airless world ⇒ flight in real vacuum.
    surface_pressure: f64,
    /// Grains ever created (impact excavation + effect materialisation).
    created_total: usize,
    /// Meteors in flight. The engine flies and lands them; the caller only throws.
    meteors: Vec<Meteor>,
}

/// A meteor: real matter with a mass, a material, a place and a velocity.
#[derive(Debug, Clone, Copy)]
pub struct Meteor {
    pub pos: Vec3,
    pub vel: Vec3,
    pub mass_kg: f32,
    pub material: usize,
    /// Rendered radius (m), from its mass and its material's density: r = (3m/4πρ)^(1/3).
    pub radius_m: f32,
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
        // The ground is a surface patch OF a real planet. Its mass, radius and the gravity the patch
        // feels all emerge from that body — there is no magic 9.81 anywhere in this path.
        let planet = match ground.planet.as_str() {
            "earth" | "" => crate::planet::earth(),
            other => return Err(format!("unknown planet {other:?} (known: \"earth\")")),
        };
        let planet_radius = planet.radius();
        let planet_mass = planet.total_mass();
        let surface_g = planet.gravity_at(planet_radius) as f32;
        let world = crate::world::generate_from(&ground.surface, &materials);
        // **The patch belongs to the planet.** Without this the field is the patch's own self-gravity —
        // measured at 0.000214 m/s² against this planet's 9.8808, one forty-six-thousandth of Earth — and
        // the grains fall in microgravity while the analytic effects a few lines below use the correct
        // `surface_g`. Two answers to "what is down", and the grains had the wrong one.
        //
        // The surface sits at the patch's own ground height in centred coordinates, so the host's centre
        // is a planet-radius below that and "down" is a direction rather than an assumption.
        let surface_y = world.bulk_height(0.0, 0.0);
        let field = MassField::build(&world, &materials, 8)
            .on_host(planet_mass, planet_radius, surface_y);
        let mut sim = Simulation {
            world,
            matter: MatterSim::new(60_000),
            resolution: ResolutionField::new(Default::default()),
            field,
            def: ground,
            materials,
            resolved_total: 0,
            name: def.name.clone(),
            planet_mass,
            planet_radius,
            surface_g,
            surface_pressure: planet.surface_pressure(),
            created_total: 0,
            meteors: Vec::new(),
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
        let gravity = Vec3::new(0.0, -self.surface_g, 0.0);
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
        // Count what the RESOLUTION hand-off materialised, before flying meteors — `fly_meteors` counts
        // its own excavation, and measuring the particle delta across both double-counted every impact
        // (the HUD read 45,380 created for 22,690 grains, which is how it was spotted).
        self.created_total += self.matter.particle_count().saturating_sub(before);
        self.fly_meteors(dt);
        self.matter.step(&mut self.world, &self.field, &[], dt);
        resolved
    }

    /// The world's declared name (for the HUD).
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The declared surface (skin) material id — what you are standing on.
    pub fn surface_material(&self) -> &str {
        self.def.surface.strata.first().map(|s| s.material.as_str()).unwrap_or("?")
    }
    /// Did matter change the world since the last call (a crater dug, grains de-resolved)? Drives remesh.
    pub fn take_dirty(&mut self) -> bool {
        self.matter.take_dirty()
    }

    /// Declared camera altitude (m) above the surface.
    pub fn eye_height_m(&self) -> f32 {
        self.def.eye_height_m
    }
    /// Declared grain size (m) a resolved region breaks into.
    pub fn grain_size_m(&self) -> f32 {
        self.def.grain_size_m
    }
    /// Surface gravity, EMERGENT from the planet this ground is a patch of: `g = GM/R²` over that
    /// body's real layered mass. Never a declared constant.
    /// The acceleration the MASS FIELD reports at a point — what a grain actually falls under
    /// (`matter::step` uses exactly this). Exposed so the discrepancy against the planet's own surface
    /// gravity can be measured rather than argued about.
    pub fn probe_field_acceleration(&self, at: glam::Vec3) -> glam::Vec3 {
        self.field.acceleration_point_approx(at, 6.0)
    }

    pub fn gravity_ms2(&self) -> f32 {
        self.surface_g
    }
    /// The planet's total mass (kg) — real matter, not a scene parameter.
    pub fn planet_mass_kg(&self) -> f64 {
        self.planet_mass
    }
    /// The planet's radius (m). The ground curves to a horizon at this radius.
    pub fn planet_radius_m(&self) -> f64 {
        self.planet_radius
    }
    /// The materials this world was built from.
    pub fn materials(&self) -> &[Material] {
        &self.materials
    }
    /// **Throw a meteor. The engine does the rest.**
    ///
    /// You give it MATTER — a mass, a material, a position and a velocity — not an abstract "energy"
    /// with a hand-computed impact site. It flies under the planet's own gravity, and when it reaches
    /// the ground the engine excavates, throws the ejecta, and settles it. The caller's whole job is
    /// creating the rock and letting go of it.
    ///
    /// The impact energy is ½mv² at the moment of contact — a consequence of the matter and its flight,
    /// never a dial. A caller cannot ask for "a big crater"; it can only throw a bigger or faster rock.
    pub fn throw_meteor(&mut self, m: Meteor) {
        self.meteors.push(m);
    }

    /// Meteors currently in flight (for the renderer, and so a HUD can say one is incoming).
    pub fn meteors(&self) -> &[Meteor] {
        &self.meteors
    }

    /// Advance every meteor in flight under the planet's gravity and impact the ones that arrive.
    /// Returns grains created this step.
    fn fly_meteors(&mut self, dt: f32) -> usize {
        let g = Vec3::new(0.0, -self.surface_g, 0.0);
        // The air this meteor flies through (docs/48). The density is the SAME emergent exponential
        // atmosphere the sky is drawn from, so the picture and the physics cannot disagree about the air.
        let air = self.materials.iter().find(|mm| mm.id == "air");
        // c_d for a sphere. FLAGGED (Law V): a DECLARED shape factor, not a tuned dial — 0.47 is the
        // measured incompressible-sphere drag coefficient. Its resolved counterpart is the pressure field
        // of `AirField` parcels flowing around the body (the IOU `drag_accel`'s own doc names), and its
        // near-term refinement is c_d(Mach): a meteor at these speeds is supersonic, where a sphere's c_d
        // rises toward ~1, so this UNDER-drags the hypersonic phase. Same primitive, finer resolution.
        const SPHERE_DRAG_CD: f64 = 0.47;
        let mut landed: Vec<Meteor> = Vec::new();
        let mut still: Vec<Meteor> = Vec::with_capacity(self.meteors.len());
        for mut m in self.meteors.drain(..) {
            // Atmospheric drag, opposing the meteor's motion. The kinetic energy it removes is dissipated
            // to the air and the meteor (real re-entry heating; the meteor's own glow/ablation is the
            // unbuilt visible follow-on) — and because the impact energy is ½mv² of the speed that ACTUALLY
            // arrives, a meteor braked by air correctly delivers a smaller crater. Nothing is lost: the KE
            // becomes atmospheric heat instead of excavation.
            if let Some(air) = air {
                let altitude = (m.pos.y - self.world.ground_height(m.pos.x, m.pos.z)).max(0.0) as f64;
                let rho = crate::atmosphere::air_density_at(
                    self.surface_pressure, air, 288.0, self.surface_g as f64, altitude,
                );
                let area = std::f64::consts::PI * (m.radius_m as f64).powi(2);
                let a_drag = crate::atmosphere::drag_accel(
                    rho, m.vel.as_dvec3(), area, m.mass_kg as f64, SPHERE_DRAG_CD,
                );
                m.vel += a_drag.as_vec3() * dt;
            }
            m.vel += g * dt;
            m.pos += m.vel * dt;
            // THE shared ground height (`World::ground_height`). This asked `surface_top_voxel` — an
            // integer voxel top — while the camera's collision shell used the bilinear surface, up to a
            // metre apart on a slope. A meteor's contact height disagreed with the surface it landed on.
            let ground = self.world.ground_height(m.pos.x, m.pos.z);
            if m.pos.y <= ground {
                landed.push(m);
            } else {
                still.push(m);
            }
        }
        self.meteors = still;
        let mut created = 0;
        for m in landed {
            // Energy is ½mv² of the matter that actually arrived. Not a parameter.
            let speed = m.vel.length();
            let energy_j = 0.5 * m.mass_kg * speed * speed;
            let dir = m.vel.normalize_or(Vec3::new(0.0, -1.0, 0.0));
            let n = self.matter.impact(&mut self.world, &self.materials, m.pos, dir, energy_j);
            log::info!(
                "impact: {:.0} kg at {:.0} m/s = {:.2e} J -> {n} grains",
                m.mass_kg, speed, energy_j
            );
            created += n;
        }
        self.created_total += created;
        created
    }
    /// Live particles, for the renderer.
    pub fn particles(&self) -> &[crate::matter::Particle] {
        &self.matter.particles
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
        // A real BALLISTIC arc under the planet's own gravity — this test used to declare zero gravity,
        // which is not a thing on a planet. The ejecta is launched at 30 m altitude closing at 50 m/s and
        // arcs into a 40 m view radius around t≈1.5 s, still inside the 96 m patch (`matter::step` culls
        // anything that leaves the world, so an effect resolving outside it spawns grains that vanish).
        let json = r#"{
          "name":"ejecta","type":"ground",
          "ground":{ "camera_m":[0,20,0], "view_radius_m":40, "planet":"earth",
            "events":[{"kind":"ejecta","at_m":[90,30,0],"velocity_ms":[-50,0,0],
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
          "ground":{ "camera_m":[0,0,0], "view_radius_m":10, "planet":"earth",
            "events":[{"kind":"ejecta","at_m":[5000,900,0],"velocity_ms":[200,0,0],
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

    /// **A meteor is MATTER, and its energy EMERGES.** The caller throws a rock; it must not be able
    /// to ask for an outcome. A heavier or faster rock must dig more because ½mv² is larger — not
    /// because a "power" parameter was turned up.
    ///
    /// This exists because the first version of this scene took `drop_meteor(energy_j)`: an abstract
    /// number the host chose, at a site the host computed. That is a dial wearing a physics coat.
    #[test]
    fn a_thrown_meteor_digs_by_its_own_kinetic_energy() {
        let world = r#"{"name":"g","type":"ground","ground":{"camera_m":[0,30,0],"view_radius_m":80}}"#;
        let iron = crate::materials::index_of(&mats(), "iron");
        let dig = |mass_kg: f32, speed: f32| -> usize {
            let mut sim = Simulation::from_json(world, mats()).expect("builds");
            let c = sim.world.center();
            let ground = sim.world.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
            sim.throw_meteor(Meteor {
                pos: Vec3::new(0.0, ground + 60.0, 0.0),
                vel: Vec3::new(0.0, -speed, 0.0),
                mass_kg,
                material: iron,
                radius_m: 0.5,
            });
            // The ENGINE flies it and lands it; the caller never computes an impact site.
            for _ in 0..600 {
                sim.step(1.0 / 60.0);
                if sim.meteors().is_empty() && sim.created_total() > 0 {
                    break;
                }
            }
            sim.created_total()
        };

        let small = dig(500.0, 40.0);
        let heavy = dig(4_000.0, 40.0);
        let fast = dig(500.0, 160.0);
        assert!(small > 0, "a thrown meteor must actually excavate; got {small}");
        assert!(heavy > small, "8x the MASS must dig more: {heavy} vs {small}");
        assert!(fast > small, "4x the SPEED must dig more (v is squared): {fast} vs {small}");
    }

    /// A meteor flies through AIR, not vacuum (docs/48). Before this the flight loop was pure ballistics —
    /// a silent Law-V gap (the atmosphere the sky is derived from did not act on anything falling through
    /// it). Thrown HORIZONTALLY, gravity leaves the horizontal speed untouched, so any drop in it is drag
    /// alone — which a vacuum forbids. This is `atmosphere::drag_accel` fed by the already-emergent air
    /// density, and it is the analytic rung of the same primitive `AirField` resolves (the c_d IOU).
    #[test]
    fn a_meteor_feels_atmospheric_drag_as_it_flies() {
        let world = r#"{"name":"g","type":"ground","ground":{"camera_m":[0,30,0],"view_radius_m":2000}}"#;
        let iron = crate::materials::index_of(&mats(), "iron");
        // Horizontal flight, high enough that it does not land during the few steps we watch.
        let fly = |radius_m: f32| -> f32 {
            let mut sim = Simulation::from_json(world, mats()).expect("builds");
            let c = sim.world.center();
            let ground = sim.world.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
            sim.throw_meteor(Meteor {
                pos: Vec3::new(-40.0, ground + 40.0, 0.0),
                vel: Vec3::new(900.0, 0.0, 0.0), // supersonic, purely horizontal
                mass_kg: 1000.0,
                material: iron,
                radius_m,
            });
            let v0 = sim.meteors()[0].vel.x;
            for _ in 0..4 {
                sim.step(1.0 / 60.0);
                if sim.meteors().is_empty() { break; }
            }
            // Horizontal speed lost over the flight — gravity cannot touch it, so this is pure drag.
            v0 - sim.meteors().first().map_or(v0, |m| m.vel.x)
        };

        let compact = fly(0.3); // small cross-section
        let broad = fly(1.0); // ~11x the frontal area, same mass ⇒ much higher drag
        assert!(compact > 0.0, "a meteor in atmosphere must lose horizontal speed to drag; lost {compact}");
        assert!(
            broad > compact * 3.0,
            "drag scales with frontal area: the broad meteor ({broad} m/s) must shed far more than the \
             compact one ({compact} m/s)"
        );
    }

    /// The engine flies the meteor. A caller that throws one and steps must see it in flight, then gone.
    #[test]
    fn the_engine_flies_the_meteor_the_caller_only_throws_it() {
        let mut sim = Simulation::from_json(
            r#"{"name":"g","type":"ground","ground":{"camera_m":[0,30,0]}}"#, mats()).expect("builds");
        let c = sim.world.center();
        let ground = sim.world.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 80.0, 0.0),
            vel: Vec3::new(0.0, -20.0, 0.0),
            mass_kg: 800.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.5,
        });
        assert_eq!(sim.meteors().len(), 1, "it is in flight");
        let start_y = sim.meteors()[0].pos.y;
        sim.step(1.0 / 60.0);
        assert!(sim.meteors()[0].pos.y < start_y, "gravity must pull it down without the caller helping");
        for _ in 0..600 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() { break; }
        }
        assert!(sim.meteors().is_empty(), "it must land on its own");
        assert!(sim.created_total() > 0, "landing must excavate real matter");
    }

    /// Every grain is counted ONCE. A meteor's excavation was being counted both by `fly_meteors` and
    /// by the generic particle-count delta, so `created_total` read double — and a matter-accounting
    /// number that lies is worse than none, because the whole point of it is catching lost matter.
    #[test]
    fn created_total_counts_each_grain_exactly_once() {
        let mut sim = Simulation::from_json(
            r#"{"name":"g","type":"ground","ground":{"camera_m":[0,30,0]}}"#, mats()).expect("builds");
        let c = sim.world.center();
        let ground = sim.world.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 60.0, 0.0),
            vel: Vec3::new(0.0, -50.0, 0.0),
            mass_kg: 800.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.5,
        });
        // Step to the frame the impact lands on, and check the count against the grains that exist.
        let mut peak = 0usize;
        for _ in 0..600 {
            sim.step(1.0 / 60.0);
            peak = peak.max(sim.particle_count());
            if sim.meteors().is_empty() && peak > 0 { break; }
        }
        assert!(peak > 0, "the meteor must excavate");
        assert_eq!(
            sim.created_total(), peak,
            "created_total ({}) must equal the grains actually created ({peak}) — a double count here \
             makes the lost-matter figure meaningless",
            sim.created_total()
        );
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

#[cfg(test)]
mod gravity_audit_tests {
    /// **What acceleration does a grain in the Ground scene actually feel?**
    ///
    /// The grains are stepped under `field.acceleration_point_approx` (matter.rs:1031) — the self-gravity
    /// of the loaded surface PATCH. A patch is a small box of voxels; a planet is not. If the patch is all
    /// a grain feels, it is falling in microgravity toward the middle of the box rather than toward the
    /// planet, and every settling time, ejecta arc and crater profile in the scene is wrong by orders of
    /// magnitude.
    ///
    /// This test does not assert a fix; it MEASURES the discrepancy so the burn-down has a number.
    #[test]
    fn measure_what_gravity_a_ground_grain_actually_feels() {
        let mats = crate::materials::load();
        let sim = super::Simulation::from_json(
            r#"{"name":"probe","type":"ground","ground":{"surface":{"amplitude_m":0.0}}}"#,
            mats,
        )
        .expect("the probe world parses");

        let g_planet = sim.gravity_ms2();
        // A point a couple of metres above the patch, where a grain would sit.
        let probe = glam::Vec3::new(0.0, 30.0, 0.0);
        let a = sim.probe_field_acceleration(probe);

        println!("planet surface gravity : {g_planet:.4} m/s²");
        println!("patch field at the surface: {:?} (|a| = {:.6} m/s²)", a, a.length());
        println!("ratio                  : {:.3e}", a.length() as f64 / g_planet as f64);

        assert!(g_planet > 9.0, "the planet's own gravity is Earth-like: {g_planet}");

        // **Grains fall under the PLANET.** This test was written the other way round: it asserted the
        // defect, because the Ground scene stepped its grains under the self-gravity of the loaded patch
        // — a box of voxels tens of metres across — which measured 0.000214 m/s² against the planet's
        // 9.8808. Microgravity, at one forty-six-thousandth of Earth, so every settling time, ejecta arc,
        // crater profile and angle of repose was wrong by four orders of magnitude and a grain took ~215×
        // too long to fall.
        //
        // Now the field knows which body its patch belongs to, and answers with the planet's own gravity
        // plus the local terrain as the perturbation it actually is.
        let ratio = a.length() as f64 / g_planet as f64;
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "a grain must fall under the PLANET, not under the patch: got {:.6} m/s² against the \
             planet's {g_planet:.4} (ratio {ratio:.3e})",
            a.length()
        );
        // And down is a DIRECTION, computed toward the host's centre, not an assumed −Y: on a patch this
        // small against Earth the two agree to a part in millions, which is exactly why it must be
        // derived rather than typed.
        assert!(a.y < 0.0 && a.x.abs() < 1e-3 * a.length() && a.z.abs() < 1e-3 * a.length(),
            "down points at the planet's centre: {a:?}");
    }
}
