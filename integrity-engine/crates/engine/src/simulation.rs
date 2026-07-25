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

use crate::aggregate::Aggregate;
use crate::gravity::MassField;
use crate::materials::Material;
use crate::matter::MatterSim;
use crate::resolution::{Effect, ResolutionField};
use crate::terra::world_def::{GroundBody, GroundDef, GroundEvent, World as WorldDef};
use glam::{DVec3, Vec3};

/// Bond stiffness ceiling (N/m) for real-time explicit integration. The honest bond constant is the
/// material's own k = E·L (iron: ~2e11 N/m), but that needs thousands of substeps per frame to stay
/// stable explicitly - true stiffness arrives with implicit integration. FLAGGED stand-in (Law V):
/// still ~1000x the old hand-tuned value, so the solid reads as rigid.
const BODY_STIFFNESS_CAP: f64 = 5.0e9;

/// Fractional bond stretch at which the solid fractures. Iron is nearly inextensible: it breaks at a
/// small strain rather than stretching like rubber. FLAGGED stand-in (Law V) for a catalogued
/// elongation-at-fracture datum the material DB does not carry yet - small enough to shatter under a
/// meteor, large enough to survive its own landing.
const BODY_BREAK_STRAIN: f64 = 0.06;

/// Per-substep position-projection cap (m) for a body particle resolving against the terrain.
/// Mirrors `particle_step.wgsl::MAX_SURFACE_CORRECTION` - the bound that makes the projection
/// stack-safe and keeps it from doing work (the grains' settling-storm fix). A body's bonds are far
/// stiffer than a grain contact, so an unbounded snap here would pump them.
const BODY_MAX_SURFACE_CORRECTION: f64 = 0.01;

/// A solid body as COHESIVE MATTER (docs/23 step 1): a lattice of real particles bonded by the
/// material's own elastic force, resting on the terrain through the same contact law every grain
/// uses. This is what retires the rigid-sphere probe: impacts, gravity, contact and damage all act on
/// its particles with no special case.
pub struct CohesiveBody {
    pub agg: Aggregate,
    /// Velocity-Verlet acceleration buffer, seeded once and carried across steps.
    acc: Vec<DVec3>,
    /// A particle's collision half-extent (m) - half the lattice spacing, so neighbours touch at rest.
    pub part_half: f64,
}

impl CohesiveBody {
    /// **The one cohesive-matter law, from parcels already laid out.** Bond stiffness from the
    /// material's own elastic modulus (k = E·L, capped for explicit stability), the declared
    /// break strain, damping derived from the material's restitution through the SAME
    /// `zeta_for_restitution` the granular contact law uses, specific heat from the catalogue,
    /// and gravity from the named planet. `build_cohesive_body` lattices a declared sphere and
    /// comes here; the docs/59 site hands its released split children straight in - one law,
    /// two consumers, so the site's ball and the ground scene's ball cannot answer "what holds
    /// iron together" differently.
    pub fn from_parcels(
        particles: Vec<crate::orbit::Body>,
        mat_idx: usize,
        spacing_m: f64,
        materials: &[Material],
        surface_g: f32,
    ) -> Result<CohesiveBody, String> {
        let mat = &materials[mat_idx];
        if mat.youngs_modulus <= 0.0 {
            return Err(format!(
                "material {:?} has no elastic modulus; a cohesive solid's stiffness derives from it",
                mat.id
            ));
        }
        if particles.is_empty() {
            return Err(format!("no parcels to build a cohesive {:?} body from", mat.id));
        }
        let l = spacing_m.max(1.0e-3);
        // Rigidity comes from the material's OWN elastic force (docs/23): a lattice bond of
        // spacing L has spring constant k = E·A/L = E·L (A = L² tributary area) - capped for
        // explicit stability (see BODY_STIFFNESS_CAP). Bond cutoff 1.75·L reaches
        // face/edge/corner neighbours.
        let stiffness = (mat.youngs_modulus as f64 * l).min(BODY_STIFFNESS_CAP);
        let mut agg = Aggregate::cohesive(
            particles,
            mat_idx,
            0.5 * l,
            1.75 * l,
            stiffness,
            0.0,
            BODY_BREAK_STRAIN,
        );
        // Damping DERIVED from the material's own coefficient of restitution - ζ = −ln(e)/√(π²+ln²e),
        // the SAME `zeta_for_restitution` the granular contact law uses, so a bond and a grain
        // contact agree on what "iron is this bouncy" means.
        agg.damping =
            agg.critically_damped(crate::granular::zeta_for_restitution(mat.restitution as f64));
        if let Some(c) = mat.specific_heat() {
            agg = agg.with_specific_heat(c as f64);
        }
        // Surface gravity is the field of the WHOLE planet below, ~uniform over this small patch -
        // computed from the named planet (g = GM/R²), never a constant.
        let mut agg = agg.with_gravity(DVec3::new(0.0, -(surface_g as f64), 0.0));
        let acc = agg.accelerations();
        Ok(CohesiveBody { agg, acc, part_half: 0.5 * l })
    }

    /// One velocity-Verlet substep with the body's carried acceleration buffer - the same call
    /// `Simulation::step_cohesive_bodies` makes, exposed so the docs/59 site can step its own
    /// cohesive matter through the identical integrator.
    pub fn substep(&mut self, dt: f64) {
        self.agg.step(&mut self.acc, dt);
    }

    /// The body's structural verdict in one word, read from the same bond state the physics runs
    /// on: "intact" while every forged bond still holds, "dented" once some fraction has fractured
    /// but the majority still binds the lattice, "shattered" once fewer than half survive. The
    /// half-way boundary is the one the fracture tests already assert (a sufficient meteor breaks
    /// more than half the bonds; an insufficient one leaves at least nine in ten). No new physics -
    /// this only NAMES the bond count so a reader is not left to interpret a raw number.
    pub fn verdict(&self) -> &'static str {
        let total = self.agg.bonds.len();
        let active = self.agg.active_bonds();
        if active == total {
            "intact"
        } else if 2 * active >= total {
            "dented"
        } else {
            "shattered"
        }
    }
}

/// Build a declared body as a cohesive aggregate. The lattice spacing is the world's own grain scale,
/// so the body is resolved at the same granularity as the matter around it (docs/47).
fn build_cohesive_body(
    def: &GroundBody,
    materials: &[Material],
    lattice_m: f64,
    surface_g: f32,
) -> Result<CohesiveBody, String> {
    let mat_idx = materials
        .iter()
        .position(|m| m.id == def.material)
        .ok_or_else(|| format!("body material {:?} is not in the material DB", def.material))?;
    let mat = &materials[mat_idx];
    let l = lattice_m.max(1.0e-3);
    let density = mat.density as f64;
    let at = DVec3::new(def.at_m[0] as f64, def.at_m[1] as f64, def.at_m[2] as f64);
    let ri = (def.radius_m / l).ceil() as i32;
    let mut particles = Vec::new();
    for z in -ri..=ri {
        for y in -ri..=ri {
            for x in -ri..=ri {
                let off = DVec3::new(x as f64, y as f64, z as f64) * l;
                if off.length() <= def.radius_m {
                    particles.push(crate::orbit::Body {
                        pos: at + off,
                        vel: DVec3::ZERO,
                        mass: density * l * l * l, // one lattice cell of the real material
                    });
                }
            }
        }
    }
    if particles.is_empty() {
        return Err(format!(
            "body radius {} m is smaller than the world's {} m grain - no matter to build",
            def.radius_m, l
        ));
    }
    CohesiveBody::from_parcels(particles, mat_idx, l, materials, surface_g)
}

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
    /// Grains ever created (impact excavation + effect materialisation).
    created_total: usize,
    /// Meteors in flight. The engine flies and lands them; the caller only throws.
    meteors: Vec<Meteor>,
    /// The declared solid bodies, as cohesive matter (docs/23 step 1).
    bodies: Vec<CohesiveBody>,
    /// How long the impact aftermath has been continuously quiet (docs/61) — the trigger for the
    /// batch downward rung that folds any remaining settled particles back into the world.
    settle: crate::recohere::SettleGauge,
    /// Voxels the batch rung has returned to the world since construction (matter accounting).
    recohered_voxels: usize,
    /// J of kinetic energy the batch rung's crossings have measured since construction. The voxel
    /// world has no thermal sink for it yet (docs/46 row 17), so this is a MEASURED loss the
    /// energy accounting can see, not a deposit.
    recohered_kinetic_j: f64,
    /// J of carried heat measured at the same crossings, same missing sink, same deferral.
    recohered_heat_j: f64,
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
        let mut ground = def
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
        // The COLUMN the patch digs through comes from the same body, at the declared site (docs/59):
        // a world says WHERE it sits; the skin and strata derive from the shared definition unless the
        // world declared an explicit sandbox column.
        ground.surface.resolve_strata(&planet, ground.lat, ground.lon);
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
        // The declared solid bodies, built as cohesive matter at the world's own grain scale.
        let bodies = ground
            .bodies
            .iter()
            .map(|b| build_cohesive_body(b, &materials, ground.grain_size_m as f64, surface_g))
            .collect::<Result<Vec<_>, String>>()?;
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
            created_total: 0,
            meteors: Vec::new(),
            bodies,
            settle: crate::recohere::SettleGauge::new(),
            recohered_voxels: 0,
            recohered_kinetic_j: 0.0,
            recohered_heat_j: 0.0,
        };
        sim.apply_events();
        Ok(sim)
    }

    /// Convenience: parse JSON and build.
    pub fn from_json(json: &str, materials: Vec<Material>) -> Result<Self, String> {
        let def = WorldDef::parse(json)?;
        Self::from_definition(&def, materials)
    }

    /// Apply the declared events. Impacts go through the one deposition door like every other impact
    /// event; ejecta become analytic effects for the resolution field to hand off when they enter view.
    fn apply_events(&mut self) {
        for ev in self.def.events.clone() {
            match ev {
                GroundEvent::Impact { at_m, direction, energy_j } => {
                    // A declared impact is pure energy with no named impactor, so it carries no
                    // momentum of its own - the deposit is heat and excavation only.
                    self.created_total += self.deposit_event(
                        Vec3::from_array(at_m),
                        Vec3::from_array(direction),
                        DVec3::ZERO,
                        energy_j as f64,
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
        self.step_cohesive_bodies(dt);
        self.recohere_when_settled(dt);
        resolved
    }

    /// **The impact site re-coheres into meshed ground** (docs/61): the production trigger for the
    /// batch downward rung. Once the aftermath is quiet at the rung's own physical criterion —
    /// nothing in flight and every remaining grain below the quiescent speed for one cell dynamical
    /// time — whatever the per-grain settle path left behind is offered back to the voxel world in
    /// one conserving pass, and `take_dirty()` drives the remesh that renders the result as ground.
    /// The per-grain path usually empties the field first on this CPU container; this is the
    /// REGION-level guarantee, and the trigger the particle-ball remnants of other containers wire
    /// into next (the flagged docs/61 IOU).
    fn recohere_when_settled(&mut self, dt: f32) {
        if self.matter.particles.is_empty() || !self.meteors.is_empty() {
            // Nothing left to demote, or more matter inbound: a fresh disturbance re-arms the gauge.
            self.settle.reset();
            return;
        }
        let peak = self
            .matter
            .particles
            .iter()
            .map(|p| p.vel.length())
            .fold(0.0f32, f32::max);
        self.settle.observe(peak, self.surface_g, dt);
        if !self.settle.settled(self.surface_g) {
            return;
        }
        if let Ok(r) =
            self.matter
                .recohere_settled(&mut self.world, &self.materials, self.surface_g, &self.settle, &[])
        {
            self.recohered_voxels += r.voxels;
            // The crossing's measured energy (docs/46 row 17): kinetic energy and carried heat the
            // binned mass took into a store with no thermal state. Accumulated so the loss is
            // visible in the accounting, not silent.
            self.recohered_kinetic_j += r.binned_kinetic_j;
            self.recohered_heat_j += r.binned_heat_j;
            // Re-arm: what remains (sub-quantum mass, law-refused columns) stays honest particles,
            // and any new excitement starts its own settle window.
            self.settle.reset();
        }
    }

    /// Advance every cohesive body: its bonds + gravity settle it to a ground state
    /// (`Aggregate::step`, substepped to the bond stiffness - rigidity is paid for with real
    /// substeps), and each particle rests on the terrain through the SAME non-injecting contact
    /// constraint the grains and the camera shell use (`granular::terrain_contact_resolve`). The
    /// bonds transmit the support up, so the ball rests as a ball; dig its ground away and its
    /// support is really gone.
    fn step_cohesive_bodies(&mut self, dt: f32) {
        let dt = dt as f64;
        // μ under a contact is the TERRAIN's own coefficient - ice is slippery because ice's datum
        // says so. Off the patch or over an empty column, the world's declared bottom stratum
        // continues (the bulk is the same declared matter, docs/54). FLAGGED: the surface material's
        // μ alone; a pair-combining rule between body and ground does not exist yet.
        let mu_fallback = self
            .def
            .surface
            .strata
            .last()
            .and_then(|s| self.materials.iter().find(|m| m.id == s.material))
            .map(|m| m.friction_coefficient as f64)
            .unwrap_or(0.0);
        let center = self.world.center();
        for body in &mut self.bodies {
            if body.agg.particles.is_empty() {
                continue;
            }
            let sub = body.agg.stable_substeps(dt).clamp(1, 256);
            let pdt = dt / sub as f64;
            for _ in 0..sub {
                body.agg.step(&mut body.acc, pdt);
                for p in &mut body.agg.particles {
                    let sample = Vec3::new(p.pos.x as f32, p.pos.y as f32, p.pos.z as f32);
                    let (h, dhdx, dhdz) = self.world.surface_bilinear_grad(sample);
                    let xi = (sample.x + center.x).floor() as i32;
                    let zi = (sample.z + center.z).floor() as i32;
                    let mu = self
                        .world
                        .surface_top_voxel(xi, zi)
                        .and_then(|t| self.world.material_at(xi, t, zi))
                        .map(|m| self.materials[m].friction_coefficient as f64)
                        .unwrap_or(mu_fallback);
                    let hit = crate::granular::terrain_contact_resolve(
                        p.pos,
                        p.vel,
                        h as f64,
                        dhdx as f64,
                        dhdz as f64,
                        body.part_half,
                        mu,
                        BODY_MAX_SURFACE_CORRECTION,
                        f64::INFINITY, // open sky: nothing rests on the body yet
                    );
                    if hit.hit {
                        p.vel = hit.vel;
                        p.pos += hit.dpos;
                    }
                }
            }
        }
    }

    /// The world's declared name (for the HUD).
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The declared surface (skin) material id — what you are standing on.
    pub fn surface_material(&self) -> &str {
        self.def.surface.strata.first().map(|s| s.material.as_str()).unwrap_or("?")
    }
    /// The RESOLVED material column, top-down - inherited from the placed body at the declared site
    /// unless the world declared a sandbox column. Exposed so "the ground is the shared Earth" is
    /// measurable, not asserted.
    pub fn strata(&self) -> &[crate::terra::world_def::Stratum] {
        &self.def.surface.strata
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
    /// Wherever the arrival is detected - a solid body forecast on the swept segment, or the bulk
    /// terrain - the delivery is the SAME event through the same door (`deposit_event`); detection
    /// picks the site, never the recipients. Returns grains created this step.
    fn fly_meteors(&mut self, dt: f32) -> usize {
        let g = Vec3::new(0.0, -self.surface_g, 0.0);
        let mut landed: Vec<(Vec3, Vec3, DVec3, f64)> = Vec::new(); // (site, dir, momentum, energy)
        let mut still: Vec<Meteor> = Vec::with_capacity(self.meteors.len());
        let mut flying = std::mem::take(&mut self.meteors);
        for mut m in flying.drain(..) {
            let before = m;
            m.vel += g * dt;
            m.pos += m.vel * dt;
            // A meteor meeting a cohesive body is forecast by the ONE collision owner
            // (`interaction::detect_swept`) on the swept segment. The forecast yields the event -
            // site, momentum, reduced-mass energy - and nothing else; what that event reaches is
            // the deposition door's business alone.
            if let Some((site, momentum, energy_j)) = self.forecast_body_contact(&before, &m, dt) {
                let dir = m.vel.normalize_or(Vec3::new(0.0, -1.0, 0.0));
                landed.push((site, dir, momentum, energy_j));
                log::info!(
                    "impact event on a body: {:.0} kg, {:.2e} J through the one door",
                    m.mass_kg, energy_j
                );
                continue;
            }
            // THE shared ground height (`World::ground_height`). This asked `surface_top_voxel` — an
            // integer voxel top — while the camera's collision shell used the bilinear surface, up to a
            // metre apart on a slope. A meteor's contact height disagreed with the surface it landed on.
            let ground = self.world.ground_height(m.pos.x, m.pos.z);
            if m.pos.y <= ground {
                // The site is where the TRAJECTORY crosses the surface, not wherever the post-step
                // sample happens to be: a fast meteor moves metres per step, so the sample can be
                // metres underground, and a buried site couples to the wrong matter (the swept body
                // path already has this honesty; the ground gets the same). Bisect the segment
                // against the shared ground height.
                let (mut lo, mut hi) = (0.0f32, 1.0f32);
                for _ in 0..16 {
                    let mid = 0.5 * (lo + hi);
                    let p = before.pos + (m.pos - before.pos) * mid;
                    if p.y > self.world.ground_height(p.x, p.z) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let site = before.pos + (m.pos - before.pos) * hi;
                // Energy is ½mv² of the matter that actually arrived. Not a parameter.
                let speed = m.vel.length();
                let energy_j = 0.5 * m.mass_kg as f64 * (speed as f64) * (speed as f64);
                let dir = m.vel.normalize_or(Vec3::new(0.0, -1.0, 0.0));
                let momentum =
                    DVec3::new(m.vel.x as f64, m.vel.y as f64, m.vel.z as f64) * m.mass_kg as f64;
                landed.push((site, dir, momentum, energy_j));
                log::info!(
                    "impact event on the ground: {:.0} kg at {:.0} m/s = {:.2e} J",
                    m.mass_kg, speed, energy_j
                );
            } else {
                still.push(m);
            }
        }
        self.meteors = still;
        let mut created = 0;
        for (site, dir, momentum, energy_j) in landed {
            created += self.deposit_event(site, dir, momentum, energy_j);
        }
        self.created_total += created;
        created
    }

    /// **The engine forecasting a meteor into a solid body.** The engine already holds both - the
    /// meteor's mass, material and swept path; the body's matter - so it builds the `BodyState`s and
    /// asks `interaction::detect_swept`, the same owner that forecasts Theia into proto-Earth. On a
    /// contact that resolves matter it returns the EVENT: the site on the struck body's own skin, the
    /// striker's momentum, and the door's reduced-mass energy. It deposits nothing itself - delivery
    /// belongs to `deposit_event`, the same door a ground landing goes through.
    /// (Like the ground impact, the impactor's own matter joining the wreck is a flagged IOU.)
    fn forecast_body_contact(
        &mut self,
        before: &Meteor,
        after: &Meteor,
        dt: f32,
    ) -> Option<(Vec3, DVec3, f64)> {
        if self.bodies.is_empty() {
            return None;
        }
        let m_strength = self
            .materials
            .get(before.material)
            .map(|mm| mm.fracture_strength as f64)
            .unwrap_or(0.0);
        for body in &self.bodies {
            if body.agg.particles.is_empty() {
                continue;
            }
            let com = body.agg.com();
            let mass = body.agg.total_mass();
            let vel = body.agg.particles.iter().map(|p| p.vel * p.mass).sum::<DVec3>() / mass;
            // The body's contact radius is its real extent: the farthest particle plus its half-width.
            let radius = body
                .agg
                .particles
                .iter()
                .map(|p| (p.pos - com).length())
                .fold(0.0, f64::max)
                + body.part_half;
            let strength = self
                .materials
                .get(body.agg.material)
                .map(|mm| mm.fracture_strength as f64)
                .unwrap_or(0.0);
            let states = [
                crate::interaction::BodyState {
                    pos: com,
                    vel,
                    mass_kg: mass,
                    radius_m: radius,
                    strength_pa: strength,
                },
                crate::interaction::BodyState {
                    pos: DVec3::new(before.pos.x as f64, before.pos.y as f64, before.pos.z as f64),
                    vel: DVec3::new(before.vel.x as f64, before.vel.y as f64, before.vel.z as f64),
                    mass_kg: before.mass_kg as f64,
                    radius_m: before.radius_m as f64,
                    strength_pa: m_strength,
                },
            ];
            let after_pos = [
                com + vel * dt as f64,
                DVec3::new(after.pos.x as f64, after.pos.y as f64, after.pos.z as f64),
            ];
            for h in crate::interaction::detect_swept(&states, &after_pos, &[true, true]) {
                if let crate::interaction::Response::ResolveMatter { .. } = h.response {
                    // The door reports the contact velocity as striker-relative-to-struck; the
                    // momentum the METEOR delivers flips sign if it is the more massive one.
                    let v_meteor_rel = if h.striker == 1 {
                        h.contact_velocity
                    } else {
                        -h.contact_velocity
                    };
                    let momentum = v_meteor_rel * before.mass_kg as f64;
                    // The door's site is the STRIKER's centre at contact - a striker-radius off the
                    // ball's skin. Report the point on the ball's own outermost matter instead, so
                    // the deposition couples to the matter actually struck rather than empty space
                    // beside it.
                    let n = (h.site - after_pos[0]).normalize_or_zero();
                    let site = after_pos[0] + n * (radius - body.part_half);
                    return Some((
                        Vec3::new(site.x as f32, site.y as f32, site.z as f32),
                        momentum,
                        h.energy_j,
                    ));
                }
            }
        }
        None
    }

    /// **The one deposition door for the awake set** (docs/23 step 2, docs/16, docs/60). An impact
    /// event - a meteor arriving, a declared impact, wherever detection found it - deposits its
    /// energy and momentum into ALL matter in range in ONE walk: terrain voxels, every cohesive
    /// body's parcels, and every debris grain. No recipient is named; the split is geometry and
    /// coupling alone:
    ///
    ///   * the coupling length λ is the crater radius this much energy opens in the matter AT the
    ///     site (E/σ - `damage::crater_volume`, the same accounting every impact in the engine
    ///     uses), clamped between the grain scale and the materialisation LOD cap. A site of
    ///     cohesionless matter (σ = 0) arrests nothing, so the event reaches the full cap;
    ///   * each parcel of matter couples through the same shell it sits on: w = V · exp(−d/λ) / d²,
    ///     spherical spreading of the front attenuated over the crater scale, with d floored at the
    ///     grain scale (coupling below the resolution is not resolvable).
    ///
    /// Each recipient's SHARE of the event is w/Σw, delivered through the operator that already owns
    /// its container - `MatterSim::impact` excavates the terrain's share (its momentum share is
    /// transmitted into the planet the patch is attached to), `Aggregate::deposit_impact` couples a
    /// body's share into its parcels, `deposit_impulse` + `deposit_shock_heat` drive the debris
    /// share. What any share DOES - crater, dent, shatter, glow - stays each material's own verdict.
    ///
    /// FLAGGED IOU: the kernel is isotropic. Real shock transport is shadowed and impedance-matched -
    /// a dense body between the site and the far wall absorbs what would have reached the wall, and
    /// energy crosses a material boundary by the impedance ratio. That needs shock transport through
    /// the actual contact network, which does not exist yet; the isotropic kernel is the honest
    /// geometric answer until it does.
    fn deposit_event(
        &mut self,
        site: Vec3,
        direction: Vec3,
        momentum: DVec3,
        energy_j: f64,
    ) -> usize {
        if energy_j <= 0.0 {
            return 0;
        }
        let cap = crate::matter::IMPACT_LOD_R as f64;
        let grain = self.def.grain_size_m as f64;
        let sigma = self.strength_at(site);
        let lambda = if sigma > 0.0 {
            crate::damage::crater_radius(crate::damage::crater_volume(energy_j, sigma))
        } else {
            cap
        }
        .clamp(grain, cap);
        let kernel = |d: f64| {
            let d = d.max(grain);
            (-d / lambda).exp() / (d * d)
        };
        let sited = DVec3::new(site.x as f64, site.y as f64, site.z as f64);

        // Terrain: every solid voxel in range couples with its own cubic metre of matter.
        let mut w_terrain = 0.0f64;
        {
            let center = self.world.center();
            let sv = site + center;
            let (cx, cy, cz) = (sv.x.floor() as i32, sv.y.floor() as i32, sv.z.floor() as i32);
            let ri = crate::matter::IMPACT_LOD_R;
            for dz in -ri..=ri {
                for dy in -ri..=ri {
                    for dx in -ri..=ri {
                        let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                        let vc = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                        let d = (vc - sv).length() as f64;
                        if d > cap || self.world.material_at(x, y, z).is_none() {
                            continue;
                        }
                        w_terrain += kernel(d);
                    }
                }
            }
        }

        // Debris grains in range - matter already flying or resting as particles.
        let mut w_grains = 0.0f64;
        let mut m_grains = 0.0f64;
        for p in &self.matter.particles {
            let d = (p.pos - site).length() as f64;
            if d > cap {
                continue;
            }
            let rho = self
                .materials
                .get(p.material)
                .map(|mm| mm.density as f64)
                .unwrap_or(1.0)
                .max(1.0);
            w_grains += (p.mass as f64 / rho) * kernel(d);
            m_grains += p.mass as f64;
        }

        // Every cohesive body's parcels in range.
        let mut w_bodies = vec![0.0f64; self.bodies.len()];
        for (bi, b) in self.bodies.iter().enumerate() {
            let rho = self
                .materials
                .get(b.agg.material)
                .map(|mm| mm.density as f64)
                .unwrap_or(1.0)
                .max(1.0);
            for p in &b.agg.particles {
                let d = (p.pos - sited).length();
                if d > cap {
                    continue;
                }
                w_bodies[bi] += (p.mass / rho) * kernel(d);
            }
        }

        let w_total = w_terrain + w_grains + w_bodies.iter().sum::<f64>();
        if w_total <= 0.0 {
            return 0; // an event in empty space couples to nothing
        }

        // Debris first, so the share reaches the grains the walk actually saw - the terrain's
        // excavation appends NEW ejecta afterwards, and those carry their own energy already.
        if w_grains > 0.0 && m_grains > 0.0 {
            let share = w_grains / w_total;
            let p_g = momentum * share;
            self.matter.deposit_impulse(
                0,
                site,
                Vec3::new(p_g.x as f32, p_g.y as f32, p_g.z as f32),
                cap as f32,
            );
            // The heat is what the impulse did not turn into bulk motion, same as every deposit.
            let heat = (energy_j * share - p_g.length_squared() / (2.0 * m_grains)).max(0.0);
            self.matter.deposit_shock_heat(0, site, heat as f32, &self.materials);
        }

        for (bi, b) in self.bodies.iter_mut().enumerate() {
            if w_bodies[bi] <= 0.0 {
                continue;
            }
            let share = w_bodies[bi] / w_total;
            b.agg.deposit_impact(&self.materials, sited, momentum * share, energy_j * share);
        }

        if w_terrain > 0.0 {
            let share = w_terrain / w_total;
            return self.matter.impact(
                &mut self.world,
                &self.materials,
                site,
                direction,
                (energy_j * share) as f32,
            );
        }
        0
    }

    /// Yield strength of the matter AT a site - what resists the event there, and therefore what
    /// sets its coupling length. A body whose parcels contain the site answers with its material;
    /// otherwise the voxel there, then the column's surface skin, then the declared bottom stratum
    /// (off the patch the bulk is the same declared matter, docs/54).
    fn strength_at(&self, site: Vec3) -> f64 {
        let sited = DVec3::new(site.x as f64, site.y as f64, site.z as f64);
        for b in &self.bodies {
            if b.agg
                .particles
                .iter()
                .any(|p| (p.pos - sited).length() <= 2.0 * b.part_half)
            {
                return self
                    .materials
                    .get(b.agg.material)
                    .map(|m| m.fracture_strength as f64)
                    .unwrap_or(0.0);
            }
        }
        let c = self.world.center();
        let (x, y, z) = (
            (site.x + c.x).floor() as i32,
            (site.y + c.y).floor() as i32,
            (site.z + c.z).floor() as i32,
        );
        if let Some(m) = self.world.material_at(x, y, z) {
            return self.materials[m].fracture_strength as f64;
        }
        if let Some(t) = self.world.surface_top_voxel(x, z) {
            if let Some(m) = self.world.material_at(x, t, z) {
                return self.materials[m].fracture_strength as f64;
            }
        }
        self.def
            .surface
            .strata
            .last()
            .and_then(|s| self.materials.iter().find(|m| m.id == s.material))
            .map(|m| m.fracture_strength as f64)
            .unwrap_or(0.0)
    }

    /// The declared solid bodies, live - cohesive matter for the renderer and the tests.
    pub fn cohesive_bodies(&self) -> &[CohesiveBody] {
        &self.bodies
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

    /// Voxels the batch downward rung (docs/61) has returned to the world — the matter-accounting
    /// counterpart of `created_total`, so "the grains became ground" is measurable, not assumed.
    pub fn recohered_voxels(&self) -> usize {
        self.recohered_voxels
    }

    /// J of kinetic energy measured at the batch rung's grain-to-voxel crossings since
    /// construction. Settling is dissipation, so this energy should become heat on the receiving
    /// side; the voxel world carries no thermal state yet (docs/46 row 17), so the engine measures
    /// the loss instead of pretending it did not happen.
    pub fn recohered_kinetic_j(&self) -> f64 {
        self.recohered_kinetic_j
    }

    /// J of heat above ambient measured at the same crossings: the binned mass's carried
    /// temperature, which the voxel store cannot hold yet (docs/46 row 17). Remainder grains keep
    /// their own heat; only what actually crossed is booked here.
    pub fn recohered_heat_j(&self) -> f64 {
        self.recohered_heat_j
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// **Ledger row 16, the ground half** (docs/59 order-of-work 1): a ground world declares WHERE on
    /// the shared Earth it sits, and its gravity AND its material column both derive from the one body
    /// definition at that site. No strata list exists in the world file; deleting the private copy
    /// broke nothing because nothing reads one.
    #[test]
    fn the_ground_column_and_gravity_derive_from_the_shared_earth_at_the_declared_site() {
        let earth = crate::planet::earth();
        // A LAND site: biosphere skin, then Earth's own layers, top-down.
        let sim = Simulation::from_json(
            r#"{"name":"g","type":"ground","ground":{"lat":45.0,"lon":-100.0}}"#, mats())
            .expect("builds");
        let names: Vec<&str> = sim.strata().iter().map(|s| s.material.as_str()).collect();
        let mut from_layers: Vec<String> =
            earth.layers.iter().rev().map(|l| l.material.clone()).collect();
        from_layers.dedup();
        assert_eq!(
            names[1..].to_vec(),
            from_layers.iter().map(String::as_str).collect::<Vec<_>>(),
            "the column under the skin IS the definition's layer stack"
        );
        assert_eq!(names[0], "grass", "a land site wears the biosphere skin");
        // An OCEAN site from the same body: the crust is the seabed, no skin.
        let ocean = Simulation::from_json(
            r#"{"name":"o","type":"ground","ground":{"lat":0.0,"lon":-150.0}}"#, mats())
            .expect("builds");
        assert_eq!(ocean.strata()[0].material, "basalt", "sea floor is the body's own crust");
        // Gravity and the planet's bulk parameters are the definition's, to the digit.
        assert_eq!(sim.planet_radius_m(), earth.radius());
        assert_eq!(sim.planet_mass_kg(), earth.total_mass());
        assert_eq!(sim.gravity_ms2(), earth.gravity_at(earth.radius()) as f32);
        // And the SHIPPED ground world carries no private column: it inherits this same derivation.
        let shipped = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"), "/../../web/public/worlds/ground/world.json"))
            .expect("shipped ground world");
        assert!(!shipped.contains("\"strata\""),
            "worlds/ground/world.json must not carry a private strata list - the body answers");
        let s = Simulation::from_json(&shipped, mats()).expect("shipped world builds");
        assert!(!s.strata().is_empty(), "the shipped world's column is inherited, not absent");
    }

    /// **The ground-zero demo is ONE world definition** (docs/23, the demo the engine is aimed at;
    /// docs/59). `worlds/ground-zero/world.json` declares the whole cast in one file: the Sun, the
    /// shared Earth, one Luna instance on her real orbit, and the iron ball at a declared ground-zero
    /// site ON that same Earth. This test proves the file EXECUTES through both paths that exist
    /// today - the system half parses through the schema the space band loads, and the ground half
    /// builds and runs through the definition-driven `Simulation` - so the definition cannot rot into
    /// wishful data. The zoom milestones (docs/59 order of work 2-4) consume the ground half; until
    /// they land, nothing renders the ball at orbital scale, and that gap is documented in the file
    /// itself rather than papered over.
    #[test]
    fn the_ground_zero_world_executes_through_the_definition_paths_that_exist() {
        let shipped = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"), "/../../web/public/worlds/ground-zero/world.json"))
            .expect("shipped ground-zero world");

        // THE SYSTEM HALF: the space band's schema. Sun + Earth + one Luna instance, every body an
        // instance of a defined profile (declared, never re-sized in the scene - the laws scan holds
        // that line for every shipped world).
        let def = crate::terra::world_def::World::parse(&shipped).expect("ground-zero world parses");
        assert_eq!(def.kind, "system", "the space band routes by type");
        let bodies = def.bodies.as_ref().expect("a system world declares its cast");
        let roles: Vec<&str> = bodies.iter().map(|b| b.role.as_str()).collect();
        assert_eq!(roles, ["star", "planet", "moon"], "Sun, the shared Earth, one Luna");
        let earth_def = &bodies[1];
        let luna = &bodies[2];
        assert_eq!(earth_def.profile.as_deref(), Some("earth"));
        assert_eq!(luna.profile.as_deref(), Some("moon"), "Luna is an instance of the defined Moon");

        // Luna is on her REAL orbit: the declared state vector is the mean distance, and the orbit it
        // implies around the shared Earth is bound (specific orbital energy < 0).
        let earth = crate::planet::earth();
        let moon = crate::planet::moon();
        let rel_pos = DVec3::from_array(luna.pos_m) - DVec3::from_array(earth_def.pos_m);
        let rel_vel = DVec3::from_array(luna.vel_ms) - DVec3::from_array(earth_def.vel_ms);
        let r = rel_pos.length();
        assert!(
            (r - 384_400_000.0).abs() < 1.0e6,
            "Luna starts at the mean Earth-Moon distance; got {r:.3e} m"
        );
        let mu = crate::orbit::G * (earth.total_mass() + moon.total_mass());
        let energy = 0.5 * rel_vel.length_squared() - mu / r;
        assert!(energy < 0.0, "the declared state is a bound orbit (E = {energy:.3e} J/kg)");

        // THE GROUND HALF: the SAME file builds through the definition-driven path. The patch derives
        // from the shared Earth at the declared site and the ball exists as cohesive iron matter.
        let sim = Simulation::from_json(&shipped, mats()).expect("the ground half builds");
        assert_eq!(sim.cohesive_bodies().len(), 1, "the declared iron ball is built");
        assert!(sim.cohesive_bodies()[0].agg.active_bonds() > 0, "the ball is a bonded solid");
        assert_eq!(
            sim.strata().first().map(|s| s.material.as_str()),
            Some("grass"),
            "ground zero is a land site, so the column wears the shared Earth's biosphere skin"
        );
        assert_eq!(sim.planet_radius_m(), earth.radius(), "the patch belongs to the one Earth");
    }

    /// **Ground zero round-trips to the shared Earth's surface.** The ground-zero world places its
    /// ground patch by latitude and longitude; the point those degrees name on the ORBITAL Earth -
    /// the body the system half flies - is `dir_from_lat_lon x the definition's radius`, and reading
    /// it back returns the declared site exactly. This is the anchor the zoom's materialization
    /// trigger descends to (docs/59): one conversion, already owned by `geo`, no second copy.
    #[test]
    fn the_ground_zero_site_round_trips_to_the_orbital_earths_surface() {
        let shipped = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"), "/../../web/public/worlds/ground-zero/world.json"))
            .expect("shipped ground-zero world");
        let def = crate::terra::world_def::World::parse(&shipped).expect("parses");
        let g = def.ground.as_ref().expect("the demo declares its ground-zero placement");

        // The placement rides the SAME body the orbital cast places - one Earth, by id.
        assert_eq!(g.planet, "earth", "ground zero is on the shared Earth");
        let placed = def.bodies.as_ref().unwrap().iter().any(|b| {
            b.role == "planet" && b.profile.as_deref() == Some(g.planet.as_str())
        });
        assert!(placed, "the ground's host body is the planet the system half flies");

        // lat/lon -> a point on that body's surface, in the body's own frame.
        let earth = crate::planet::earth();
        let site = crate::geo::dir_from_lat_lon(g.lat, g.lon) * earth.radius();
        assert!(
            (site.length() - earth.radius()).abs() < 1.0e-6,
            "the site sits ON the surface the definition declares"
        );
        // ... and back: the round trip is the identity, so ground zero is one place, whoever asks.
        let (lat, lon) = crate::geo::lat_lon_from_dir(site);
        assert!(
            (lat - g.lat).abs() < 1.0e-9 && (lon - g.lon).abs() < 1.0e-9,
            "({lat}, {lon}) must be the declared ({}, {})",
            g.lat,
            g.lon
        );
        // The site is land there, which is why the ball has ground to rest on.
        assert!(crate::planet::is_land(g.lat, g.lon), "ground zero is a land site");
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

    /// **The impact site re-coheres into meshed ground** (docs/61). After a thrown meteor's
    /// aftermath settles — quiet at the rung's own physical criterion, not merely "the test waited
    /// a while" — NO bare particles remain: every grain either deposited through the per-grain
    /// settle path or was folded back by the batch rung, so the remnant is ground the mesher can
    /// stand a camera on, not a frozen particle field. The world must hold the returned matter as
    /// real voxels (excavated minus returned = only what left the patch, which the accounting
    /// counters expose separately).
    #[test]
    fn a_settled_impact_aftermath_leaves_ground_not_bare_particles() {
        let mut sim = Simulation::from_json(
            r#"{"name":"g","type":"ground","ground":{"camera_m":[0,30,0],"view_radius_m":120}}"#,
            mats(),
        )
        .expect("builds");
        let c = sim.world.center();
        let ground = sim.world.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 60.0, 0.0),
            vel: Vec3::new(0.0, -80.0, 0.0),
            mass_kg: 1500.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.5,
        });
        // A minute of simulated time: land, excavate, loft, settle, re-cohere.
        for _ in 0..3600 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() && sim.created_total() > 0 && sim.particle_count() == 0 {
                break;
            }
        }
        assert!(sim.created_total() > 0, "the meteor must excavate real matter");
        assert_eq!(
            sim.particle_count(),
            0,
            "a settled impact site must be GROUND again — {} grains left as a bare particle field",
            sim.particle_count()
        );
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

    /// **The ball is matter** (docs/23 step 1). A ground world declares a solid body - material,
    /// radius, position, nothing else - and the engine builds it as COHESIVE MATTER: a bonded lattice
    /// of real particles. Declared above the terrain it FALLS under the planet's own gravity, RESTS
    /// on the surface through the same contact law every grain uses, and STAYS there. No rigid-sphere
    /// special case, no scripted placement.
    #[test]
    fn a_declared_cohesive_ball_falls_rests_on_the_terrain_and_stays() {
        let json = r#"{
          "name":"ball","type":"ground",
          "ground":{ "camera_m":[0,50,0], "view_radius_m":200,
            "surface":{ "amplitude_m":0.0 },
            "bodies":[{"material":"iron","radius_m":1.5,"at_m":[0.0,50.0,0.0]}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("a world can declare a solid body");
        assert_eq!(sim.cohesive_bodies().len(), 1);
        let ground = sim.world.ground_height(0.0, 0.0) as f64;
        let ball = &sim.cohesive_bodies()[0];
        let bonds0 = ball.agg.active_bonds();
        assert!(bonds0 > 0, "a cohesive solid is bonded");
        assert!(
            ball.agg.com().y > ground + 3.0,
            "it starts in the air (com {:.1} vs ground {ground:.1})",
            ball.agg.com().y
        );

        // FALLS: after one second its centre of mass is measurably lower.
        let y0 = sim.cohesive_bodies()[0].agg.com().y;
        for _ in 0..60 {
            sim.step(1.0 / 60.0);
        }
        let y1 = sim.cohesive_bodies()[0].agg.com().y;
        assert!(y1 < y0 - 1.0, "gravity pulls it down without help ({y0:.2} -> {y1:.2})");

        // RESTS: give it time to land and ring down, then its lowest particle sits ON the surface.
        for _ in 0..840 {
            sim.step(1.0 / 60.0);
        }
        let ball = &sim.cohesive_bodies()[0];
        let bottom = ball
            .agg
            .particles
            .iter()
            .map(|p| p.pos.y)
            .fold(f64::INFINITY, f64::min)
            - ball.part_half;
        assert!(
            (bottom - ground).abs() < 0.6,
            "the ball rests ON the terrain: bottom {bottom:.2} vs ground {ground:.2}"
        );

        // STAYS: two more seconds move the centre of mass almost nowhere.
        let com1 = sim.cohesive_bodies()[0].agg.com();
        for _ in 0..120 {
            sim.step(1.0 / 60.0);
        }
        let ball = &sim.cohesive_bodies()[0];
        assert!(
            (ball.agg.com() - com1).length() < 0.2,
            "it has settled to a ground state (moved {:.3} m)",
            (ball.agg.com() - com1).length()
        );
        // Its own landing dents nothing: iron survives a few metres of fall.
        assert_eq!(ball.agg.active_bonds(), bonds0, "landing does not shatter it");
    }

    /// **The impact energy reaches the ball through the one door** (`interaction::detect_swept` /
    /// `respond`) - no ball-specific collision branch. A meteor thrown at the resting ball is
    /// forecast by the engine's own collision owner; the door's reduced-mass energy and the
    /// striker's momentum are deposited into the ball's PARCELS (`Aggregate::deposit_impact`,
    /// the same coupling the terrain uses), so its matter measurably heats and recoils. What that
    /// energy then DOES to iron (dent / shatter / vaporize) is `damage`'s call, asserted when the
    /// destruction step lands.
    #[test]
    fn a_meteors_energy_reaches_the_ball_through_the_shared_door() {
        let json = r#"{
          "name":"door","type":"ground",
          "ground":{ "camera_m":[0,50,0], "view_radius_m":200,
            "surface":{ "amplitude_m":0.0 },
            "bodies":[{"material":"iron","radius_m":1.5,"at_m":[0.0,45.2,0.0]}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        // Let it settle onto the ground first, so the hit is on a RESTING ball.
        for _ in 0..300 {
            sim.step(1.0 / 60.0);
        }
        let before_temp = sim.cohesive_bodies()[0]
            .agg
            .temps
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);
        let com_v0 = {
            let b = &sim.cohesive_bodies()[0];
            b.agg.particles.iter().map(|p| p.vel * p.mass).sum::<glam::DVec3>()
                / b.agg.total_mass()
        };
        let ground = sim.world.ground_height(0.0, 0.0);
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 80.0, 0.0),
            vel: Vec3::new(0.0, -250.0, 0.0),
            mass_kg: 1200.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.33,
        });
        for _ in 0..600 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() {
                break;
            }
        }
        assert!(sim.meteors().is_empty(), "the meteor arrived");
        let ball = &sim.cohesive_bodies()[0];
        let after_temp = ball.agg.temps.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            after_temp > before_temp + 0.5,
            "the door's deposit must HEAT the parcels: {before_temp:.1} K -> {after_temp:.1} K"
        );
        let com_v1 = ball.agg.particles.iter().map(|p| p.vel * p.mass).sum::<glam::DVec3>()
            / ball.agg.total_mass();
        assert!(
            (com_v1 - com_v0).length() > 0.05,
            "and the momentum really arrives (com velocity changed {:.3} m/s)",
            (com_v1 - com_v0).length()
        );
    }

    /// **One event, every recipient** (docs/23 step 2). A meteor landing BESIDE the resting ball
    /// must reach the terrain (a crater) AND the ball (heat and momentum through the same door) in
    /// the one deposition walk - no per-object branch decides who is hit. And the ball's fate stays
    /// its material's own verdict: a nearby landing deposits less than iron's fracture strength, so
    /// every bond survives.
    #[test]
    fn one_impact_event_reaches_the_terrain_and_the_ball_through_one_door() {
        let json = r#"{
          "name":"door-all","type":"ground",
          "ground":{ "camera_m":[0,50,0], "view_radius_m":200,
            "surface":{ "amplitude_m":0.0 },
            "bodies":[{"material":"iron","radius_m":1.5,"at_m":[5.0,45.2,0.0]}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        for _ in 0..300 {
            sim.step(1.0 / 60.0); // let the ball settle 5 m from ground zero
        }
        let bonds0 = sim.cohesive_bodies()[0].agg.active_bonds();
        let temp0 = sim.cohesive_bodies()[0].agg.temps.iter().cloned().fold(0.0f32, f32::max);
        let ground = sim.world.ground_height(0.0, 0.0);
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 80.0, 0.0),
            vel: Vec3::new(0.0, -500.0, 0.0),
            mass_kg: 2000.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.39,
        });
        for _ in 0..600 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() {
                break;
            }
        }
        assert!(sim.meteors().is_empty(), "the meteor arrived");
        assert!(sim.created_total() > 0, "the terrain's share excavated a crater");
        let ball = &sim.cohesive_bodies()[0];
        let temp1 = ball.agg.temps.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            temp1 > temp0 + 0.05,
            "the ball 5 m away is heated by the SAME event: {temp0:.2} K -> {temp1:.2} K"
        );
        assert_eq!(
            ball.agg.active_bonds(),
            bonds0,
            "a nearby landing deposits under iron's strength, so the ball keeps every bond"
        );
    }

    /// **The door reaches debris grains too.** Grains already in flight when an impact lands nearby
    /// receive their share - heat a contact could never give them - through the same walk.
    #[test]
    fn an_impact_event_heats_debris_grains_already_in_flight() {
        let json = r#"{
          "name":"door-debris","type":"ground",
          "ground":{ "camera_m":[0,50,0], "view_radius_m":200,
            "surface":{ "amplitude_m":0.0 },
            "events":[{"kind":"ejecta","at_m":[6.0,50.0,0.0],"velocity_ms":[0,0,0],
                       "radius_m":2,"grain_radius_m":0.5}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        sim.step(1.0 / 60.0); // the effect is in view: it materialises into real grains
        let n0 = sim.particle_count();
        assert!(n0 > 0, "debris exists before the meteor arrives");
        assert!(
            sim.particles().iter().all(|p| p.temp_k <= crate::matter::REF_TEMP_K + 0.01),
            "and it is cold"
        );
        let ground = sim.world.ground_height(0.0, 0.0);
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 60.0, 0.0),
            vel: Vec3::new(0.0, -400.0, 0.0),
            mass_kg: 1200.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.33,
        });
        for _ in 0..60 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() {
                break;
            }
        }
        assert!(sim.meteors().is_empty(), "the meteor arrived while the grains were still airborne");
        let heated = sim
            .particles()
            .iter()
            .take(n0)
            .filter(|p| p.temp_k > crate::matter::REF_TEMP_K + 0.05)
            .count();
        assert!(
            heated > 0,
            "pre-existing grains must receive the event's heat through the one door \
             (0 of {n0} warmed)"
        );
    }

    /// **The docs/23 sentence, as a test.** A meteor of sufficient energy dropped on the ball
    /// destroys it - bonds fracture, parcels scatter, the hottest parcels glow through the shared
    /// emission curve - with no line of code that says destroy. The deposited energy density simply
    /// exceeds what iron can survive (`damage::classify` against `data/materials.json`), and the
    /// same event craters the ground beneath, because the door reaches everything.
    #[test]
    fn a_sufficient_meteor_shatters_the_ball_and_its_hottest_parcels_glow() {
        let json = r#"{
          "name":"shatter","type":"ground",
          "ground":{ "camera_m":[0,50,0], "view_radius_m":200,
            "surface":{ "amplitude_m":0.0 },
            "bodies":[{"material":"iron","radius_m":1.5,"at_m":[0.0,45.2,0.0]}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        for _ in 0..300 {
            sim.step(1.0 / 60.0);
        }
        let bonds0 = sim.cohesive_bodies()[0].agg.active_bonds();
        let spread0 = sim.cohesive_bodies()[0].agg.rms_radius();
        assert!(bonds0 > 0, "the resting ball is a bonded solid");
        let ground = sim.world.ground_height(0.0, 0.0);
        // 1,200 kg of iron at 17 km/s - a typical asteroid arrival speed, ~1.7e11 J. Real matter on
        // a real trajectory; the outcome is not asked for anywhere below.
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 80.0, 0.0),
            vel: Vec3::new(0.0, -17_000.0, 0.0),
            mass_kg: 1200.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.33,
        });
        for _ in 0..600 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() {
                break;
            }
        }
        assert!(sim.meteors().is_empty(), "the meteor arrived");
        for _ in 0..120 {
            sim.step(1.0 / 60.0); // two seconds: the over-strained bonds break, the parcels fly
        }
        let ball = &sim.cohesive_bodies()[0];
        let bonds1 = ball.agg.active_bonds();
        assert!(
            bonds1 < bonds0 / 2,
            "iron's thresholds are exceeded: the structure fractures ({bonds0} -> {bonds1} bonds)"
        );
        assert_eq!(ball.verdict(), "shattered", "the one-word verdict reports the same bond state");
        let spread1 = ball.agg.rms_radius();
        assert!(
            spread1 > 2.0 * spread0,
            "the parcels scatter - it is no longer a ball ({spread0:.2} m -> {spread1:.2} m rms)"
        );
        let tmax = ball.agg.temps.iter().cloned().fold(0.0f32, f32::max);
        let glow = crate::emission::incandescence(tmax);
        assert!(
            glow[0] > 0.0,
            "the hottest parcels glow through the SHARED emission curve (peak {tmax:.0} K)"
        );
        assert!(
            sim.created_total() > 0,
            "and the same event cratered the ground beneath - the door reached everything"
        );
    }

    /// The other half of the docs/23 sentence: an INSUFFICIENT meteor dents or merely displaces the
    /// ball, and it survives - same code path, no branch, just less energy than iron's thresholds.
    #[test]
    fn an_insufficient_meteor_displaces_the_ball_and_it_survives() {
        let json = r#"{
          "name":"survive","type":"ground",
          "ground":{ "camera_m":[0,50,0], "view_radius_m":200,
            "surface":{ "amplitude_m":0.0 },
            "bodies":[{"material":"iron","radius_m":1.5,"at_m":[0.0,45.2,0.0]}] }
        }"#;
        let mut sim = Simulation::from_json(json, mats()).expect("builds");
        for _ in 0..300 {
            sim.step(1.0 / 60.0);
        }
        let bonds0 = sim.cohesive_bodies()[0].agg.active_bonds();
        assert_eq!(
            sim.cohesive_bodies()[0].verdict(),
            "intact",
            "settling onto the ground breaks nothing, and the verdict says so"
        );
        let spread0 = sim.cohesive_bodies()[0].agg.rms_radius();
        let com_v0 = {
            let b = &sim.cohesive_bodies()[0];
            b.agg.particles.iter().map(|p| p.vel * p.mass).sum::<DVec3>() / b.agg.total_mass()
        };
        let ground = sim.world.ground_height(0.0, 0.0);
        // 300 kg falling at 60 m/s - a boulder, not an asteroid. Under a thousandth of the energy
        // density iron fractures at.
        sim.throw_meteor(Meteor {
            pos: Vec3::new(0.0, ground + 80.0, 0.0),
            vel: Vec3::new(0.0, -60.0, 0.0),
            mass_kg: 300.0,
            material: crate::materials::index_of(&mats(), "iron"),
            radius_m: 0.21,
        });
        for _ in 0..600 {
            sim.step(1.0 / 60.0);
            if sim.meteors().is_empty() {
                break;
            }
        }
        assert!(sim.meteors().is_empty(), "the boulder arrived");
        let struck = {
            let b = &sim.cohesive_bodies()[0];
            let v = b.agg.particles.iter().map(|p| p.vel * p.mass).sum::<DVec3>()
                / b.agg.total_mass();
            (v - com_v0).length()
        };
        assert!(struck > 0.02, "the ball was really hit - its momentum changed ({struck:.3} m/s)");
        for _ in 0..120 {
            sim.step(1.0 / 60.0);
        }
        let ball = &sim.cohesive_bodies()[0];
        let bonds1 = ball.agg.active_bonds();
        assert!(
            bonds1 * 10 >= bonds0 * 9,
            "below iron's thresholds the structure holds ({bonds0} -> {bonds1} bonds)"
        );
        assert_ne!(ball.verdict(), "shattered", "a surviving ball never reads as shattered");
        let spread1 = ball.agg.rms_radius();
        assert!(
            spread1 < 1.3 * spread0,
            "it is still a ball ({spread0:.2} m -> {spread1:.2} m rms)"
        );
        let tmax = ball.agg.temps.iter().cloned().fold(0.0f32, f32::max);
        assert_eq!(
            crate::emission::incandescence(tmax),
            [0.0, 0.0, 0.0],
            "nothing glows - the deposit could not heat iron to incandescence ({tmax:.1} K)"
        );
    }

    /// A body of a material the DB does not know is REFUSED at build - never a silent default solid.
    #[test]
    fn a_body_of_unknown_material_is_refused() {
        let err = match Simulation::from_json(
            r#"{"name":"x","type":"ground",
                "ground":{"bodies":[{"material":"unobtainium","radius_m":1.0,"at_m":[0,50,0]}]}}"#,
            mats(),
        ) {
            Err(e) => e,
            Ok(_) => panic!("an uncatalogued material must not build"),
        };
        assert!(err.contains("unobtainium"), "the error names it: {err}");
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

