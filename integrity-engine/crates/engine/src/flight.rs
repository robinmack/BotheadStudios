//! **Matter in flight — the engine's operation, not a scene's** (docs/59).
//!
//! A scene's whole contribution to a meteor is an INITIAL CONDITION: here is a mass, of this material, at
//! this place, moving this way. Everything after that — falling under the world's gravity, meeting the
//! air, slowing, heating, ablating, shedding a trail, and finally arriving at hard matter — is physics,
//! and physics is the engine's. Robin (2026-07-24): the swarm must not be *wired into* a scene; it must be
//! *"a natural operation of the engine receiving these materializations and rendering them naturally"*,
//! and the only legitimate scene-side part is the button that introduces the mass and its trajectory.
//!
//! This module is that operation. It was extracted from `Simulation::fly_meteors`, where the logic was
//! already correct and already generic in everything but its ADDRESS: it lived inside a 96 m voxel ground
//! patch, in `f32`, so nothing at planetary scale could reach it. Nothing about drag, aeroheating or
//! ablation was ever ground-specific — only the geometry of *where the air is* and *where the ground is*.
//!
//! **One flight law, any world.** That geometry is the whole of [`FlightEnvironment`]: what gravity is
//! here, what air is here, and whether the path has met hard matter. A flat ground patch answers those
//! from a heightfield; a planet answers them from its own layered mass and its [`AirShell`]. The physics
//! in between is byte-identical, which is the docs/46 distinction between legitimate specialization (the
//! *geometry* differs) and a violation (the same question answered twice).
//!
//! [`AirShell`]: crate::atmosphere::AirShell

use crate::atmosphere::Trail;
use crate::materials::Material;
use glam::DVec3;

/// A body in flight: real matter, with a place and a velocity. `f64` because the same record has to serve
/// a pebble over a field and a fragment on a hyperbolic approach from outside a planet's atmosphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlyingBody {
    /// A stable handle for this body, for as long as it is in flight.
    ///
    /// Needed because an INDEX is not one: bodies leave the list when they arrive or are consumed, so the
    /// fragment at slot 7 is not the fragment that was at slot 7 a second ago. Anything that wants to keep
    /// hold of one particular body across time — a camera following a fragment down (docs/59 Stage B) —
    /// needs to name it, not point at where it happens to sit.
    pub id: u64,
    pub pos: DVec3,
    pub vel: DVec3,
    pub mass_kg: f64,
    /// Index into the material catalogue — what it is made of. Drag, heating and ablation all read this
    /// body's own material; nothing about flight is written for "a meteor".
    pub material: usize,
    /// Radius (m), from its mass at its material's density. SHRINKS as it ablates.
    pub radius_m: f64,
    /// Surface temperature (K). Rises from aeroheating in flight, and is what it glows AT. It is the
    /// SKIN's temperature: for anything bigger than `skin_m` the interior is cooler and unmodelled.
    pub temp_k: f64,
    /// How deep the aeroheating has soaked into this body (m) — 0 for a body that has not been heated.
    /// Only the mass within this depth warms, which is why a metre-class body can glow at all
    /// (`atmosphere::heated_mass`, docs/46 row 21). It saturates at the radius, recovering bulk heating.
    pub skin_m: f64,
}

impl FlyingBody {
    /// A body that has not been heated yet — the state anything is introduced in. `id` is stamped by
    /// [`Flight::introduce`]; a body that has not been introduced yet has none, which is why this is 0.
    pub fn fresh(
        pos: DVec3,
        vel: DVec3,
        mass_kg: f64,
        material: usize,
        radius_m: f64,
        temp_k: f64,
    ) -> Self {
        FlyingBody {
            id: 0,
            pos,
            vel,
            mass_kg,
            material,
            radius_m,
            temp_k,
            skin_m: 0.0,
        }
    }
}

/// **The world a body is flying through**, reduced to the three things flight actually needs to ask of it.
///
/// This is the seam between the one flight law and the many geometries it runs in. A ground patch, a
/// planet seen from orbit, and a moon with no air at all differ only in how they answer these; none of
/// them re-implements entry.
pub trait FlightEnvironment {
    /// Gravitational acceleration at a point (m/s²), as a vector — so a flat patch can answer "down" and
    /// a planet can answer "toward the centre" without either being a special case.
    fn gravity_at(&self, pos: DVec3) -> DVec3;

    /// Air density (kg/m³) and ambient temperature (K) at a point, or `None` for vacuum. An airless world
    /// returns `None` and its bodies fly ballistically — honestly, rather than through a thin fudge.
    fn air_at(&self, pos: DVec3) -> Option<(f64, f64)>;

    /// **Has the path from `from` to `to` met hard matter, and what does that meeting deliver?** This is
    /// the hand-off from the FLUID branch to the SOLID one (docs/58 "it's all impact"): whatever mass
    /// survives the air arrives here, and the caller excavates.
    ///
    /// The ENVIRONMENT answers because the environment is the only thing that knows where its own surface
    /// is and what is standing on it — a heightfield, a planet's radius, a cohesive body resting on the
    /// ground. That is also why the site it returns must be the point where the TRAJECTORY CROSSED the
    /// surface, not the post-step sample: at 17 km/s a 1/60 s step is ~280 m long, so returning `to`
    /// couples the impact to matter hundreds of metres underground. Use [`surface_crossing`] to find it
    /// and [`delivered`] to price it, so every environment answers with one law.
    ///
    /// `dt` is the SUBSTEP this segment spans (not the caller's frame), so an environment can advance
    /// whatever else is moving over the same interval before forecasting contact against it.
    fn arrival(&self, body: &FlyingBody, from: DVec3, to: DVec3, dt: f64) -> Option<Met>;

    /// The e-folding height of this world's air (m), or 0 for vacuum — the length over which the density
    /// a body is flying through changes appreciably.
    ///
    /// It is here so [`Flight::step`] can size its OWN substeps: a hypervelocity body must not cross a
    /// large density change in one step, and the world is the only thing that knows how long that distance
    /// is. Asking the caller to substep put that judgement in a scene, and a scene deriving it from its
    /// frame time produced a feedback loop — a slow frame asked for more substeps, which made the next
    /// frame slower.
    fn air_scale_height_m(&self) -> f64;
}

/// **What a flight path met, as the environment sees it.** The site plus what the meeting is worth —
/// returned by [`FlightEnvironment::arrival`], which is the only thing that knows its own surface.
#[derive(Clone, Copy, Debug)]
pub struct Met {
    /// Where the TRAJECTORY crossed into hard matter — see [`surface_crossing`].
    pub at: DVec3,
    /// Energy available at the contact frame (J) — see [`delivered`].
    pub energy_j: f64,
    /// Momentum delivered (kg·m/s) — see [`delivered`].
    pub momentum: DVec3,
}

/// **Where a straight segment crosses into hard matter.** ONE bisection, used by every environment, so
/// a ground patch and a planet locate an impact site the same way.
///
/// `inside` is the environment's own "is this point within hard matter" test — a heightfield comparison,
/// a radius comparison, whatever its geometry is. The returned point is the first one known to be inside,
/// so an excavation couples to matter that is really there.
///
/// **Resolve it to the surface field's own precision — NOT to the resolution of the matter.** This was
/// got wrong once, and the way it failed is worth keeping. The first version stopped bisecting once the
/// bracket was below the GRAIN size, reasoning that `deposit_event` floors its coupling distance at the
/// grain scale so a finer site changes nothing. That reasoning is false, because the site is not only a
/// distance — it is also **where the material is sampled**. A site a fraction of a voxel too deep reads
/// the material *under* the surface, which changes the yield strength, which changes the crater radius,
/// which changes the coupling length λ, which is inside an exponential.
///
/// MEASURED, on Sean's own `an_impact_event_heats_debris_grains_already_in_flight`: a grain-sized
/// tolerance left the bracket 0.83 m wide, put the site **0.795 m under** the surface, and cut the
/// grains' coupling weight by **284×** (0.2416 → 0.00085) — the debris took essentially none of the
/// event. Nothing about that is visible at the call site; the test is what caught it.
///
/// So the bracket halves until it is below what the surface field itself can distinguish. The heights
/// come from an `f32` field (~1e-7 relative), so 30 halvings put the bracket about an order of magnitude
/// beneath the noise floor; more would be resolving float noise, and fewer is the bug above.
pub fn surface_crossing(from: DVec3, to: DVec3, inside: impl Fn(DVec3) -> bool) -> DVec3 {
    let span = (to - from).length();
    // Already inside at the start: the segment did not cross, it began buried. The honest site is where
    // the body already was, not somewhere further in.
    if inside(from) {
        return from;
    }
    if span <= 0.0 {
        return to;
    }
    // 30 halvings: span/2³⁰ ≈ 1e-9 of the segment, an order of magnitude below the f32 surface field's
    // ~1e-7 relative precision. Derived from what the field can represent, not tuned. It costs 30 height
    // samples on the rare step where something actually arrives.
    const STEPS: u32 = 30;
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..STEPS {
        let mid = 0.5 * (lo + hi);
        if inside(from + (to - from) * mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    from + (to - from) * hi
}

/// **What an arriving body delivers to the matter it struck.** ONE formula for every arrival.
///
/// The energy is ½·μ·|Δv|² with the REDUCED mass μ — "the energy actually available at the contact
/// frame, not either body's kinetic energy in an arbitrary frame", which is the law
/// [`crate::interaction::detect_swept`] already states and applies to every body-body collision in the
/// engine. Momentum is the arriving body's own mass times that same relative velocity.
///
/// `target_mass_kg = None` means matter too massive to move — a planet's bulk, a ground patch attached to
/// one. Then μ → m and Δv → v, so this reduces EXACTLY to the ½mv² and mv the terrain path used to write
/// out separately. **Terrain is not a special case of impact; it is the immovable limit of the general
/// one**, and `an_immovable_target_is_the_reduced_mass_limit` pins the two against each other.
pub fn delivered(
    body: &FlyingBody,
    target_vel: DVec3,
    target_mass_kg: Option<f64>,
) -> (f64, DVec3) {
    let rel = body.vel - target_vel;
    let mu = match target_mass_kg {
        Some(mt) if mt > 0.0 => body.mass_kg * mt / (body.mass_kg + mt),
        // Immovable (or massless-target nonsense): the reduced mass IS the arriving body's own.
        _ => body.mass_kg,
    };
    (0.5 * mu * rel.length_squared(), rel * body.mass_kg)
}

/// A body that has reached hard matter — everything an excavation needs, computed from the matter that
/// actually arrived rather than from what was launched.
#[derive(Clone, Copy, Debug)]
pub struct Arrival {
    pub body: FlyingBody,
    /// Where it struck — the crossing point on the surface, not the post-step sample.
    pub at: DVec3,
    /// Energy delivered (J), from [`delivered`]. Never a parameter — a caller cannot ask for a bigger
    /// crater, only throw a bigger or faster rock.
    pub energy_j: f64,
    /// Momentum delivered (kg·m/s), from [`delivered`]. An excavation needs a direction and a magnitude,
    /// not just a scalar budget.
    pub momentum: DVec3,
}

/// **Everything in flight, and what it has shed.** One call introduces matter; one call advances it.
#[derive(Clone, Debug, Default)]
pub struct Flight {
    bodies: Vec<FlyingBody>,
    trail: Trail,
    burned_up: usize,
    /// Monotonic, so an id is never reused and a stale handle reads as "gone" rather than as some other
    /// body that happens to have taken the number.
    next_id: u64,
}

impl Flight {
    /// **Introduce matter on a trajectory.** This is the one door, and the only thing a scene does: it
    /// hands the engine a mass, a material, a place and a velocity. It cannot ask for an outcome.
    pub fn introduce(&mut self, body: FlyingBody) -> u64 {
        self.next_id += 1;
        self.bodies.push(FlyingBody {
            id: self.next_id,
            ..body
        });
        self.next_id
    }

    /// Introduce a disrupted body's fragments (`damage::disrupt`) as a SWARM, placed relative to `origin`
    /// and travelling at `approach` — the docs/59 meteor swarm, as a plain composition of two operations
    /// the engine already has. `origin`/`approach` are declared initial conditions; every fragment's mass,
    /// size, position and velocity come from the disruption physics.
    pub fn introduce_swarm(
        &mut self,
        origin: DVec3,
        approach: DVec3,
        parent_mass_kg: f64,
        parent_radius_m: f64,
        material: usize,
        count: usize,
        since_s: f64,
        temp_k: f64,
    ) {
        for f in crate::damage::disrupt(parent_mass_kg, parent_radius_m, count, since_s) {
            self.introduce(FlyingBody::fresh(
                origin + f.rel_pos,
                approach + f.rel_vel,
                f.mass_kg,
                material,
                f.radius_m,
                temp_k,
            ));
        }
    }

    pub fn bodies(&self) -> &[FlyingBody] {
        &self.bodies
    }

    /// The body with this id, if it is still in flight. `None` once it has arrived or been consumed — which
    /// is the answer a follower needs, and the reason ids are never reused.
    pub fn body(&self, id: u64) -> Option<&FlyingBody> {
        self.bodies.iter().find(|b| b.id == id)
    }

    /// The most massive body still in flight. The natural one to FOLLOW (docs/59 Stage B): it is the one
    /// the air takes least of, so it is the one that will still be there at the ground — and picking it
    /// needs no dial, only a comparison.
    pub fn heaviest(&self) -> Option<&FlyingBody> {
        self.bodies
            .iter()
            .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))
    }

    /// Resolve at most `n` trail parcels at once; beyond that the shed mass is booked into the air. The
    /// mass is identical either way — this is how finely it is held, not whether it exists.
    pub fn set_trail_budget(&mut self, n: usize) {
        self.trail.set_resolve_budget(n);
    }

    /// What the bodies have ablated away — still on the books (docs/59).
    /// **Shed mass into the world at a point** — matter leaving something, entering the air.
    ///
    /// An ablating meteor is one source; a gun's muzzle is another. Both put hot matter into the
    /// atmosphere at a place with a velocity and a temperature, and both are then carried, cooled and
    /// mixed by the same trail. Nothing here is written for either case.
    pub fn shed_at(&mut self, mass_kg: f64, material: usize, pos: DVec3, vel: DVec3, temp_k: f64) {
        self.trail.shed(mass_kg, material, pos, vel, temp_k);
    }

    /// **Shed a CLOUD** — the same mass, spread over many parcels and a spray of directions.
    ///
    /// A muzzle blast is not a point. `shed_at` puts all of it in one place, which is honest about mass
    /// and useless as a picture; this divides it, which is a RESOLUTION choice and not a physical one
    /// (Law IV, docs/44): the same matter, held more finely. `introduce_swarm` makes exactly this
    /// distinction for a disintegrating bolide.
    ///
    /// The spread is derived from the jet itself — parcels leave within `cone` radians of the jet
    /// direction at speeds from a third of it to the whole, which is what an expanding gas leaving a
    /// tube does. Deterministic (a golden-angle spiral, not a random draw), so two runs of the same
    /// shot produce the same cloud and a rig can compare frames.
    pub fn shed_cloud(
        &mut self,
        mass_kg: f64,
        material: usize,
        pos: DVec3,
        vel: DVec3,
        temp_k: f64,
        cone: f64,
        parcels: usize,
    ) {
        let n = parcels.max(1);
        let each = mass_kg / n as f64;
        let dir = vel.normalize_or(DVec3::Y);
        // A frame about the jet, so the spray is around IT rather than around a world axis.
        let a = if dir.x.abs() < 0.9 {
            DVec3::X
        } else {
            DVec3::Y
        };
        let u = dir.cross(a).normalize_or(DVec3::X);
        let w = dir.cross(u);
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            // Angle from the axis grows with sqrt(t) so parcels fill the cone evenly by AREA rather
            // than bunching at the rim.
            let (sa, ca) = (cone * t.sqrt()).sin_cos();
            let phi = i as f64 * golden;
            let d = (dir * ca + (u * phi.cos() + w * phi.sin()) * sa).normalize();
            // Slower at the edges, fastest on the axis — a jet, not a shell.
            let speed = vel.length() * (0.33 + 0.67 * (1.0 - t));
            self.trail
                .shed(each, material, pos + d * 0.5, d * speed, temp_k);
        }
    }

    pub fn trail(&self) -> &Trail {
        &self.trail
    }

    /// How many bodies ablated to nothing before arriving — the real fate of most meteors, and a number
    /// worth being able to state rather than inferring from a body quietly disappearing.
    pub fn burned_up(&self) -> usize {
        self.burned_up
    }

    /// **One step of everything in flight.** Each body meets the air (drag, aeroheating, ablation — the
    /// generic `atmosphere::atmospheric_step`), sheds what it vaporises into the trail, falls under the
    /// world's gravity, and is returned as an [`Arrival`] if its path met hard matter. The trail ages
    /// alongside: parcels drift, are slowed, and radiate into the air.
    ///
    /// A body that ablates to nothing never arrives, and leaves no crater, because no matter got there.
    pub fn step(
        &mut self,
        env: &impl FlightEnvironment,
        mats: &[Material],
        dt: f64,
    ) -> Vec<Arrival> {
        // **The engine sizes its own substeps.** A body at 17 km/s must not cross a large slice of the
        // atmosphere in one step, so the count follows the distance actually travelled against the air's
        // scale height — never the caller's frame time. The cap is a COMPUTE bound, and reaching it means
        // the entry is being integrated more coarsely than it deserves, not that more work is spawned.
        let h = env.air_scale_height_m();
        let fastest = self
            .bodies
            .iter()
            .map(|b| b.vel.length())
            .fold(0.0_f64, f64::max);
        let n = if h > 0.0 && fastest > 0.0 {
            (((fastest * dt) / (0.1 * h)).ceil() as usize).clamp(1, 32)
        } else {
            1
        };
        let sub = dt / n as f64;
        let mut arrivals = Vec::new();
        for _ in 0..n {
            arrivals.extend(self.step_bodies(env, mats, sub));
        }
        // The trail ages ONCE per call, not once per substep. Shed vapour is dragged to rest in
        // milliseconds and never travels far, so it needs none of the resolution a hypervelocity body
        // does — and stepping tens of thousands of parcels per substep was pure cost.
        self.trail
            .step(mats, dt, |at| env.air_at(at).unwrap_or((0.0, 0.0)));
        arrivals
    }

    /// One substep of the bodies alone (the trail is aged by [`Flight::step`], which owns the substepping).
    fn step_bodies(
        &mut self,
        env: &impl FlightEnvironment,
        mats: &[Material],
        dt: f64,
    ) -> Vec<Arrival> {
        let mut arrivals = Vec::new();
        let (trail, burned) = (&mut self.trail, &mut self.burned_up);
        // In place: this drained the body list into a fresh `Vec` every substep, which at a thousand
        // bodies and a few hundred frames a second is megabytes of allocation churn per second for no
        // reason. `retain_mut` mutates and removes without building a second list.
        self.bodies.retain_mut(|b| {
            // THE FLUID BRANCH. Nothing here is meteor-specific: it is the body's own material against
            // whatever air this world has at this point.
            if let (Some((rho, ambient)), Some(mat)) = (env.air_at(b.pos), mats.get(b.material)) {
                let s = crate::atmosphere::atmospheric_step(
                    rho, b.vel, b.mass_kg, b.radius_m, b.temp_k, b.skin_m, ambient, mat, dt,
                );
                // The vaporised mass LEAVES the body; it does not leave the simulation.
                if s.ablated_mass > 0.0 {
                    trail.shed(s.ablated_mass, b.material, b.pos, b.vel, s.temp_k);
                }
                // Integrate the drag EXACTLY over the substep instead of sampling it. An entry
                // decelerates at hundreds of g, and `v += a·dt` at that stiffness overshoots — the same
                // failure the vapour parcels hit, where a step long enough to stop a parcel reversed it.
                // Quadratic drag has a closed form: dv/dt = −k|v|v ⇒ |v| = |v₀|/(1 + k|v₀|·t), and k|v₀|
                // is just |a|/|v₀|, which the operator has already told us. Direction is unchanged (drag
                // is along −v̂), so scaling the vector is the whole update. Exact for constant density;
                // the remaining error is ρ varying along the step, which vanishes with dt like every
                // other explicit term.
                let speed = b.vel.length();
                let decel = s.drag_accel.length();
                if speed > 0.0 && decel > 0.0 {
                    b.vel /= 1.0 + (decel / speed) * dt;
                } else {
                    b.vel += s.drag_accel * dt;
                }
                b.temp_k = s.temp_k;
                b.skin_m = s.skin_m;
                b.mass_kg = (b.mass_kg - s.ablated_mass).max(0.0);
                b.radius_m = s.radius_m;
            }
            // Burned up: a small body can ablate to nothing before it ever reaches the ground. It leaves
            // no crater because no matter arrives — the fate of most meteors, not an early-out.
            //
            // The test is "no mass left", not "less than a gram left": the ground scene retired a meteor
            // below `1.0e-3` kg, a threshold that traced to nothing (Law V). It is not needed — ablation
            // takes `min(net/L_v·dt, mass)` per step, so a body that is being consumed reaches exactly
            // zero on its own — and a gram of iron at 15 km/s still carries ~110 kJ, which is not nothing.
            if b.mass_kg <= 0.0 || b.radius_m <= 0.0 {
                *burned += 1;
                return false;
            }
            let from = b.pos;
            b.vel += env.gravity_at(b.pos) * dt;
            b.pos += b.vel * dt;
            // THE SOLID BRANCH: whatever survived the air arrives, carrying the energy it actually has.
            // The environment prices the meeting, because only it knows WHAT was met — bare terrain, or a
            // body standing on it that is free to move and so takes a reduced-mass share.
            if let Some(met) = env.arrival(b, from, b.pos, dt) {
                arrivals.push(Arrival {
                    body: *b,
                    at: met.at,
                    energy_j: met.energy_j,
                    momentum: met.momentum,
                });
                return false;
            }
            true
        });
        arrivals
    }

    /// **Everything in flight, as it must be drawn** (`crate::Drawn`): the bodies and the vapour they have
    /// shed, in one list of physical facts. A scene draws this and never learns what a meteor is.
    ///
    /// Bodies come before trail parcels for the reason `Simulation::drawn` orders matter in flight first:
    /// a caller with a finite instance budget should lose the least informative matter.
    pub fn drawn(
        &self,
        env: &impl FlightEnvironment,
        to_scene: impl Fn(DVec3) -> glam::Vec3,
    ) -> Vec<crate::Drawn> {
        let mut out = Vec::new();
        self.drawn_into(&mut out, env, to_scene);
        out
    }

    /// The same, into a buffer the caller owns and REUSES. A render loop calling [`Flight::drawn`] every
    /// frame allocates and frees the whole list every frame — tens of thousands of items at a few hundred
    /// frames a second — and that churn is worth not doing.
    pub fn drawn_into(
        &self,
        out: &mut Vec<crate::Drawn>,
        env: &impl FlightEnvironment,
        to_scene: impl Fn(DVec3) -> glam::Vec3,
    ) {
        out.clear();
        out.extend(self.bodies.iter().map(|b| crate::Drawn {
            pos: to_scene(b.pos),
            vel: b.vel.as_vec3(),
            radius_m: b.radius_m as f32,
            material: b.material,
            temp_k: b.temp_k as f32,
            resting: false,
        }));
        out.extend(self.trail.parcels().iter().map(|p| crate::Drawn {
            pos: to_scene(p.pos),
            vel: p.vel.as_vec3(),
            // A parcel's size is its expansion into the air where it actually is — so a puff shed high,
            // into thin air, is genuinely bigger. The environment is what knows the local density.
            radius_m: p.radius_in(env.air_at(p.pos).map_or(0.0, |(rho, _)| rho)) as f32,
            material: p.material,
            temp_k: p.temp_k as f32,
            resting: false,
        }));
    }
}

/// **A planet as a world to fly through**: gravity is the body's own layered mass, the air is its
/// emergent hydrostatic column, and hard matter is its surface. A flat ground patch answers the same
/// three questions from a heightfield (`simulation::GroundAir`), and the flight physics between them
/// is the same code.
///
/// ★★ **This lived inside `mod app` — inside a SCENE — until 2026-08-03, and that was the defect.**
/// Robin: *"SCENES must never apply physics."* Answering *what is gravity here, what is the air here,
/// where is the surface* IS physics, so a scene that owns those answers is applying them; and being
/// wasm-only, it could not be tested natively or reused by anything that was not a browser. It is the
/// shape docs/46 row 15 records — capability reachable only THROUGH a scene. Now the scene merely
/// names a body and the engine answers.
pub struct PlanetAir {
    matter: crate::planet::LayeredBody,
    air: crate::atmosphere::AirShell,
    radius_m: f64,
}

impl PlanetAir {
    /// Resolve a placed body's matter and its emergent air once, so the flight step does not have to
    /// rebuild the planet to ask what gravity is.
    pub fn of(mats: &[crate::materials::Material], body_id: &str, radius_m: f64) -> Self {
        let matter = crate::planet::body(body_id);
        let air = match mats.iter().find(|m| m.id == "air") {
            Some(a) => crate::atmosphere::AirShell::new(
                matter.surface_pressure(),
                a,
                288.0,
                matter.gravity_at(matter.radius()),
            ),
            None => crate::atmosphere::AirShell {
                rho_surface: 0.0,
                scale_height_m: 0.0,
                ambient_temp_k: 288.0,
            },
        };
        PlanetAir {
            matter,
            air,
            radius_m,
        }
    }
}

impl FlightEnvironment for PlanetAir {
    fn gravity_at(&self, pos: glam::DVec3) -> glam::DVec3 {
        // Gauss's law over the body's REAL differentiated mass profile — not a declared surface g.
        self.matter.acceleration_at(pos, glam::DVec3::ZERO)
    }
    fn air_at(&self, pos: glam::DVec3) -> Option<(f64, f64)> {
        if !self.air.exists() {
            return None; // an airless body: real vacuum, and its bodies fly ballistically
        }
        Some((
            self.air.density_at(pos.length() - self.radius_m),
            self.air.ambient_temp_k,
        ))
    }
    fn arrival(
        &self,
        body: &FlyingBody,
        from: glam::DVec3,
        to: glam::DVec3,
        _dt: f64,
    ) -> Option<Met> {
        if to.length() > self.radius_m {
            return None;
        }
        // **The site is where the trajectory CROSSED the surface.** This used to return `to`, the
        // post-step sample, which at orbital entry speed is kilometres inside the planet — the same
        // defect the ground patch had, and worse here because the speeds are higher.
        //
        // The sphere is analytic, so the crossing can be resolved as finely as the loop allows; the
        // tolerance that would matter is the elevation raster's, and this coarse shell does not carry
        // one. (docs/46: the raster is 19.55 km/pixel, so a metre of site precision is already far
        // below the surface this radius stands in for.)
        let at = surface_crossing(from, to, |p| p.length() <= self.radius_m);
        // A planet is immovable: the reduced mass is the arriving body's own.
        let (energy_j, momentum) = delivered(body, glam::DVec3::ZERO, None);
        Some(Met {
            at,
            energy_j,
            momentum,
        })
    }
    fn air_scale_height_m(&self) -> f64 {
        self.air.scale_height_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with a flat floor at y=0, uniform gravity, and an exponential atmosphere — a ground patch.
    struct FlatGround {
        g: f64,
        air: crate::atmosphere::AirShell,
    }
    impl FlightEnvironment for FlatGround {
        fn gravity_at(&self, _pos: DVec3) -> DVec3 {
            DVec3::new(0.0, -self.g, 0.0)
        }
        fn air_at(&self, pos: DVec3) -> Option<(f64, f64)> {
            Some((self.air.density_at(pos.y.max(0.0)), self.air.ambient_temp_k))
        }
        fn arrival(&self, body: &FlyingBody, from: DVec3, to: DVec3, _dt: f64) -> Option<Met> {
            if to.y > 0.0 {
                return None;
            }
            // A flat floor is defined everywhere, so the crossing can be resolved to the loop's limit.
            let at = surface_crossing(from, to, |p| p.y <= 0.0);
            let (energy_j, momentum) = delivered(body, DVec3::ZERO, None);
            Some(Met {
                at,
                energy_j,
                momentum,
            })
        }
        fn air_scale_height_m(&self) -> f64 {
            self.air.scale_height_m
        }
    }

    /// A whole planet: radial gravity from its real layered mass, its own air, and a spherical surface.
    struct Planet {
        matter: crate::planet::LayeredBody,
        air: crate::atmosphere::AirShell,
    }
    impl FlightEnvironment for Planet {
        fn gravity_at(&self, pos: DVec3) -> DVec3 {
            self.matter.acceleration_at(pos, DVec3::ZERO)
        }
        fn air_at(&self, pos: DVec3) -> Option<(f64, f64)> {
            let alt = pos.length() - self.matter.radius();
            Some((self.air.density_at(alt), self.air.ambient_temp_k))
        }
        fn arrival(&self, body: &FlyingBody, from: DVec3, to: DVec3, _dt: f64) -> Option<Met> {
            let r = self.matter.radius();
            if to.length() > r {
                return None;
            }
            let at = surface_crossing(from, to, |p| p.length() <= r);
            let (energy_j, momentum) = delivered(body, DVec3::ZERO, None);
            Some(Met {
                at,
                energy_j,
                momentum,
            })
        }
        fn air_scale_height_m(&self) -> f64 {
            self.air.scale_height_m
        }
    }

    fn earth_planet() -> (Planet, Vec<Material>) {
        let mats = crate::materials::load();
        let air_mat = &mats[crate::materials::index_of(&mats, "air")];
        let earth = crate::planet::earth();
        let air = crate::atmosphere::AirShell::new(
            earth.surface_pressure(),
            air_mat,
            288.0,
            earth.gravity_at(earth.radius()),
        );
        (Planet { matter: earth, air }, mats)
    }

    /// **The same flight law, in a ground patch and around a planet.** Nothing in `Flight` knows which it
    /// is in: one implements "down" and a floor, the other radial gravity and a sphere, and the entry
    /// physics between them is the same code. If these ever needed different code, that would be the bug —
    /// not the eleven orders of magnitude between them.
    /// **The immovable target is the reduced-mass limit, not a second formula.**
    ///
    /// `interaction::detect_swept` prices every body-body collision at ½·μ·|Δv|² — "the energy actually
    /// available at the contact frame". The terrain path used to write out ½·m·v² separately. They are the
    /// same law: as the struck mass grows without bound, μ → m and Δv → v. This pins that, so the two can
    /// never drift into being two answers to one question.
    #[test]
    fn an_immovable_target_is_the_reduced_mass_limit() {
        let b = FlyingBody::fresh(
            DVec3::ZERO,
            DVec3::new(0.0, -17_000.0, 0.0),
            1200.0,
            0,
            0.33,
            288.0,
        );
        let (e_immovable, p_immovable) = delivered(&b, DVec3::ZERO, None);

        // The closed form the terrain path used to compute inline.
        let speed = b.vel.length();
        assert!((e_immovable - 0.5 * b.mass_kg * speed * speed).abs() < 1e-6 * e_immovable);
        assert!((p_immovable - b.vel * b.mass_kg).length() < 1e-9);

        // And it is the LIMIT of the finite-mass case, approached from below as the target gets heavier.
        let mut last_err = f64::INFINITY;
        for exp in [3, 6, 9, 12, 15] {
            let m_target = b.mass_kg * 10f64.powi(exp);
            let (e, p) = delivered(&b, DVec3::ZERO, Some(m_target));
            assert!(
                e < e_immovable,
                "a target that can recoil takes LESS energy at the contact frame"
            );
            let err = (e_immovable - e) / e_immovable;
            assert!(
                err < last_err,
                "and it converges as the target gets heavier ({err:e})"
            );
            last_err = err;
            // Momentum is the arriving body's own mv either way — the target's mass does not change it.
            assert!((p - p_immovable).length() < 1e-9);
        }
        assert!(
            last_err < 1e-14,
            "at 1e15x the striker's mass it is the immovable case ({last_err:e})"
        );

        // A moving target is priced on the RELATIVE velocity, so a body it cannot catch delivers nothing.
        let (e_chasing, _) = delivered(&b, b.vel, None);
        assert_eq!(e_chasing, 0.0, "matter moving with you does not strike you");
    }

    /// **The impact site is where the TRAJECTORY crossed the surface, not the post-step sample.**
    ///
    /// `arrival` used to return `to`. At entry speed one step is hundreds of metres long, so the site
    /// landed that far underground and the excavation coupled to matter that was nowhere near the
    /// surface. Both environments had it; this pins the fix at ground scale and planetary scale at once.
    #[test]
    fn an_arrival_sites_the_impact_on_the_surface_not_where_the_step_ended() {
        let (planet, mats) = earth_planet();
        let g = FlatGround {
            g: 9.81,
            air: crate::atmosphere::AirShell {
                rho_surface: 0.0,
                scale_height_m: 0.0,
                ambient_temp_k: 288.0,
            },
        };

        // 17 km/s over a 1/60 s step is a ~283 m segment straddling the floor.
        let from = DVec3::new(0.0, 40.0, 0.0);
        let to = DVec3::new(0.0, 40.0 - 283.0, 0.0);
        let b = FlyingBody::fresh(
            to,
            (to - from).normalize() * 17_000.0,
            1200.0,
            0,
            0.33,
            288.0,
        );
        let met = g
            .arrival(&b, from, to, 1.0 / 60.0)
            .expect("the path met the floor");
        assert!(
            met.at.y.abs() < 1e-6,
            "the site is ON the floor, not {:.1} m under it",
            -met.at.y
        );
        assert!(
            met.at.y > to.y + 1.0,
            "and emphatically not the post-step sample"
        );

        // The same law at planetary scale, where the step is longer still.
        let r = planet.matter.radius();
        let from_p = DVec3::new(0.0, r + 5_000.0, 0.0);
        let to_p = DVec3::new(0.0, r - 5_000.0, 0.0);
        let bp = FlyingBody::fresh(
            to_p,
            DVec3::new(0.0, -17_000.0, 0.0),
            1200.0,
            0,
            0.33,
            288.0,
        );
        let met_p = planet
            .arrival(&bp, from_p, to_p, 1.0 / 60.0)
            .expect("the path met the ground");
        assert!(
            (met_p.at.length() - r).abs() < 1e-3,
            "the site sits on the planet's surface, {:.1} m off",
            met_p.at.length() - r
        );
        let _ = mats;
    }

    #[test]
    fn one_flight_law_serves_a_ground_patch_and_a_planet() {
        let (planet, mats) = earth_planet();
        let iron = crate::materials::index_of(&mats, "iron");
        let rho_iron = mats[iron].density as f64;
        let grain = |r: f64| rho_iron * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);

        // Ground patch: a rock dropped from 200 m arrives at the floor.
        let ground = FlatGround {
            g: 9.81,
            air: crate::atmosphere::AirShell::new(
                101_325.0,
                &mats[crate::materials::index_of(&mats, "air")],
                288.0,
                9.81,
            ),
        };
        let mut flight = Flight::default();
        flight.introduce(FlyingBody::fresh(
            DVec3::new(0.0, 200.0, 0.0),
            DVec3::ZERO,
            grain(0.1),
            iron,
            0.1,
            288.0,
        ));
        let mut hit = None;
        for _ in 0..2000 {
            if let Some(a) = flight.step(&ground, &mats, 0.005).first() {
                hit = Some(*a);
                break;
            }
        }
        let a = hit.expect("the rock reaches the floor");
        assert!(
            a.energy_j > 0.0,
            "it arrives with the energy its own fall gave it"
        );
        assert!(flight.bodies().is_empty(), "and is no longer in flight");

        // Planet: the SAME code, the same struct, from 300 km up at orbital-entry speed, falling in.
        let mut orbital = Flight::default();
        let r0 = planet.matter.radius() + 300_000.0;
        orbital.introduce(FlyingBody::fresh(
            DVec3::new(0.0, r0, 0.0),
            DVec3::new(0.0, -15_000.0, 0.0),
            grain(0.5),
            iron,
            0.5,
            288.0,
        ));
        let mut arrived = None;
        for _ in 0..40_000 {
            if let Some(x) = orbital.step(&planet, &mats, 0.002).first() {
                arrived = Some(*x);
                break;
            }
            if orbital.bodies().is_empty() {
                break; // burned up
            }
        }
        let x = arrived.expect("a half-metre iron body survives entry and reaches the surface");
        assert!(
            (x.at.length() - planet.matter.radius()).abs() < 1000.0,
            "it arrives AT the surface, not somewhere near it"
        );
        // It was slowed and heated by real air on the way down.
        assert!(
            x.body.vel.length() < 15_000.0,
            "the air braked it ({:.0} m/s)",
            x.body.vel.length()
        );
        assert!(
            x.body.temp_k > 288.0,
            "and heated it ({:.0} K)",
            x.body.temp_k
        );
    }

    /// **A swarm is two operations composed, not a feature.** `introduce_swarm` is `damage::disrupt` fed
    /// into `introduce`; the engine has nothing called "meteor".
    ///
    /// And what the swarm DOES is not written anywhere either. Ablation is a surface-to-volume effect —
    /// heating enters through the frontal area, and the mass behind it grows as the cube — so the smaller
    /// a fragment is, the larger the fraction of it the air takes. MEASURED on Earth's own emergent
    /// atmosphere, eight iron fragments entering at 15 km/s from 200 km, as the parent grows:
    ///
    /// ```text
    /// parent r   ablated   peak surface T
    ///   1 cm      94.7%     3134 K  (iron's boiling point)
    ///   5 cm      31.1%     3134 K
    ///  10 cm      14.6%     3134 K
    ///  30 cm       4.3%     3134 K
    ///   1 m        0.75%    3134 K
    ///   3 m        0.03%    3134 K
    ///  10 m        0%       2346 K  — too blunt to reach boiling
    /// ```
    ///
    /// That is why shooting stars are small and iron meteorites reach the ground, and nothing in the
    /// engine states it: it falls out of one heating law meeting one mass distribution.
    ///
    /// The 10 m body is not a limitation either — it is Sutton–Graves being right. Stagnation flux goes as
    /// `√(ρ/R_n)`, so a BLUNTER body is heated less, which is the whole reason re-entry capsules are blunt.
    #[test]
    fn a_swarm_enters_and_the_air_takes_more_of_the_small_fragments() {
        let (planet, mats) = earth_planet();
        let iron = crate::materials::index_of(&mats, "iron");
        let r_surface = planet.matter.radius();

        // Fly one swarm and report what fraction of the parent the air took.
        let ablated_fraction = |parent_r: f64| -> f64 {
            let parent_m =
                mats[iron].density as f64 * (4.0 / 3.0) * std::f64::consts::PI * parent_r.powi(3);
            let mut flight = Flight::default();
            flight.introduce_swarm(
                DVec3::new(0.0, r_surface + 200_000.0, 0.0),
                DVec3::new(0.0, -15_000.0, 0.0),
                parent_m,
                parent_r,
                iron,
                8,
                86_400.0,
                288.0,
            );
            assert_eq!(
                flight.bodies().len(),
                8,
                "eight fragments, from one disrupted body"
            );
            let launched: f64 = flight.bodies().iter().map(|b| b.mass_kg).sum();
            assert!(
                (launched / parent_m - 1.0).abs() < 1e-9,
                "the swarm IS the parent's mass"
            );

            let mut arrived_mass = 0.0;
            for _ in 0..120_000 {
                for a in flight.step(&planet, &mats, 0.002) {
                    arrived_mass += a.body.mass_kg;
                }
                if flight.bodies().is_empty() {
                    break;
                }
            }
            // CONSERVATION, whatever became of them: still flying + turned to trail + landed = launched.
            let aloft: f64 = flight.bodies().iter().map(|b| b.mass_kg).sum();
            let booked = aloft + flight.trail().mass() + arrived_mass;
            assert!(
                (booked / parent_m - 1.0).abs() < 1e-9,
                "parent r={parent_r}: {booked:.6e} accounted for vs {parent_m:.6e} launched"
            );
            flight.trail().mass() / parent_m
        };

        let f = [0.01_f64, 0.05, 0.1, 0.3, 1.0].map(ablated_fraction);
        for w in f.windows(2) {
            assert!(
                w[0] > w[1],
                "the air takes a larger share of a smaller fragment: {w:?}"
            );
        }
        assert!(
            f[0] > 0.9,
            "millimetre pieces are almost entirely consumed ({:.1}%)",
            f[0] * 100.0
        );
        // A METRE-CLASS body still arrives with essentially all of itself — iron meteorites do — but it is
        // no longer untouched: it ablates a real, small fraction, which before the thermal-skin model was
        // exactly zero because the whole body was heated at once (docs/46 row 21).
        assert!(
            (1.0e-4..0.05).contains(&f[4]),
            "a metre iron body loses a little of itself, not none and not much ({:.3}%)",
            f[4] * 100.0
        );
    }

    /// **Row 21, closed: a metre-class body glows because only its SKIN heats.**
    ///
    /// `atmospheric_step` used to raise the temperature of a body's WHOLE mass at its bulk heat capacity.
    /// Thermal response therefore scaled with volume, and a half-metre iron body flew a perfectly correct
    /// 20 km/s entry and barely warmed — while real iron meteorites arrive with a molten fusion crust over
    /// a core cold enough to frost. Ablation is a SURFACE process.
    ///
    /// Now the heat front advances at the material's own diffusivity `α = k/(ρc)` and only the mass it has
    /// reached takes part. This asserts the fix directly, by flying the SAME body both ways: with a fresh
    /// skin (the real case) and with the skin pre-set to the radius (the old bulk case, which the model
    /// still contains as its limit).
    #[test]
    fn a_metre_class_body_glows_because_only_its_skin_heats() {
        let (planet, mats) = earth_planet();
        let iron = crate::materials::index_of(&mats, "iron");
        let r = 1.0_f64;
        let m = mats[iron].density as f64 * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);
        let t_boil = mats[iron].boil_point().expect("iron boils");

        // `skin_m` is the only difference between the two bodies.
        let fly = |skin_m: f64| -> (f64, f64, f64) {
            let mut flight = Flight::default();
            let mut b = FlyingBody::fresh(
                DVec3::new(0.0, planet.matter.radius() + 200_000.0, 0.0),
                DVec3::new(0.0, -15_000.0, 0.0),
                m,
                iron,
                r,
                288.0,
            );
            b.skin_m = skin_m;
            flight.introduce(b);
            let (mut peak_t, mut peak_skin) = (0.0_f64, 0.0_f64);
            for _ in 0..120_000 {
                flight.step(&planet, &mats, 0.002);
                for x in flight.bodies() {
                    peak_t = peak_t.max(x.temp_k);
                    peak_skin = peak_skin.max(x.skin_m);
                }
                if flight.bodies().is_empty() {
                    break;
                }
            }
            (peak_t, flight.trail().mass(), peak_skin)
        };

        let (skin_t, skin_ablated, peak_skin) = fly(0.0);
        let (bulk_t, bulk_ablated, _) = fly(r); // heated through from the start: the old behaviour

        assert!(
            (skin_t - t_boil).abs() < 1.0,
            "with a real skin, a 1 m iron body reaches its BOILING point ({skin_t:.0} K of {t_boil:.0} K)"
        );
        assert!(
            skin_ablated > 0.0,
            "and therefore ablates ({skin_ablated:.2} kg)"
        );
        assert!(
            bulk_t < 1500.0,
            "heated in bulk the same body barely warms ({bulk_t:.0} K) — that was row 21"
        );
        assert_eq!(bulk_ablated, 0.0, "and ablates nothing at all");
        assert!(
            skin_t > bulk_t * 2.0,
            "the surface runs far hotter than the bulk average: {skin_t:.0} K vs {bulk_t:.0} K"
        );

        // The skin does not simply grow: ablation strips it as fast as conduction deepens it, so it
        // SETTLES — at δ = α/2v for a surface receding at v, the classical thermal boundary layer of an
        // ablating body. Nothing declares that thickness; it is where the two rates balance. MEASURED at
        // ~1.4 cm for iron, and it must stay a small fraction of a metre-wide body or "skin" means nothing.
        assert!(
            (0.002..0.05).contains(&peak_skin),
            "the heated layer settles thin ({:.4} m on a {r} m body)",
            peak_skin
        );
    }

    /// **Nothing is left aloft forever** (Robin, 2026-07-24: *"we should be certain the particles
    /// eventually reach the ground/merge so they can safely be 'forgotten'"*).
    ///
    /// An entry that never finishes is a leak: bodies hovering at a hundred metres, parcels creeping
    /// toward ambient, both drawn every frame for the rest of the session. So this runs a whole swarm to
    /// completion and asserts the books close and then EMPTY — every fragment either arrived or was
    /// consumed by the air, every parcel became air, and the mass adds up at the end exactly as it did at
    /// the start.
    ///
    /// The measured version of this in the browser is the reason it exists: 3,734 parcels sat at 288.00 K
    /// indefinitely, because `merged_into_air` asked whether a radiatively-cooling parcel had REACHED
    /// ambient — which it never can.
    #[test]
    fn an_entry_finishes_and_nothing_is_left_aloft() {
        let (planet, mats) = earth_planet();
        let iron = crate::materials::index_of(&mats, "iron");
        let parent_r = 0.5_f64;
        let parent_m =
            mats[iron].density as f64 * (4.0 / 3.0) * std::f64::consts::PI * parent_r.powi(3);

        let mut flight = Flight::default();
        flight.introduce_swarm(
            glam::DVec3::new(0.0, planet.matter.radius() + 500_000.0, 0.0),
            glam::DVec3::new(0.0, -17_000.0, 0.0),
            parent_m,
            parent_r,
            iron,
            40,
            86_400.0,
            250.0,
        );
        let launched = flight.bodies().len();
        let launched_mass: f64 = flight.bodies().iter().map(|b| b.mass_kg).sum();

        let (mut arrived, mut arrived_mass) = (0usize, 0.0);
        let dt = 0.02;
        let mut steps = 0usize;
        // Dark flight is genuinely slow — a centimetre fragment falls the last tens of km at ~80 m/s — so
        // the budget is generous. What matters is that it TERMINATES, not that it terminates quickly.
        while steps < 200_000 {
            for a in flight.step(&planet, &mats, dt) {
                arrived += 1;
                arrived_mass += a.body.mass_kg;
            }
            steps += 1;
            if flight.bodies().is_empty() && flight.trail().parcels().is_empty() {
                break;
            }
        }

        println!(
            "entry finished after {:.1} s ({steps} steps): {arrived} arrived ({arrived_mass:.1} kg), {} burned up, trail {:.2} kg",
            steps as f64 * dt, flight.burned_up(), flight.trail().mass()
        );
        // Guards against a VACUOUS pass: the entry has to have actually taken place. An empty sim also
        // satisfies "nothing is left aloft".
        assert!(arrived > 0, "some fragments must reach the ground");
        assert!(
            flight.trail().mass() > 0.0,
            "and the air must have taken some of them"
        );
        assert!(
            steps > 1_000,
            "the entry must have taken real time, not exited immediately ({steps} steps)"
        );
        assert!(
            flight.bodies().is_empty(),
            "every fragment must arrive or be consumed — {} still aloft after {:.0} s",
            flight.bodies().len(),
            steps as f64 * dt
        );
        assert!(
            flight.trail().parcels().is_empty(),
            "every parcel must become air — {} still resolved after {:.0} s",
            flight.trail().parcels().len(),
            steps as f64 * dt
        );
        assert_eq!(
            arrived + flight.burned_up(),
            launched,
            "{arrived} arrived + {} burned up must account for all {launched}",
            flight.burned_up()
        );
        // And the mass is the mass: what landed, plus what the air took, is what was launched.
        let booked = arrived_mass + flight.trail().mass();
        assert!(
            (booked / launched_mass - 1.0).abs() < 1e-9,
            "mass closes at the end too: {booked:.6e} vs {launched_mass:.6e} launched"
        );
        // Everything the air took has finished cooling — no resolved remainder hiding in the total.
        assert!(
            (flight.trail().merged_kg() - flight.trail().mass()).abs() < 1e-12,
            "all trail mass has become air"
        );
    }

    /// An airless world flies its bodies ballistically — no drag, no heating, no trail. Vacuum honestly,
    /// rather than a thin atmosphere nobody declared.
    #[test]
    fn a_world_with_no_air_flies_its_bodies_through_real_vacuum() {
        struct Airless;
        impl FlightEnvironment for Airless {
            fn gravity_at(&self, _p: DVec3) -> DVec3 {
                DVec3::new(0.0, -1.62, 0.0) // lunar surface gravity
            }
            fn air_at(&self, _p: DVec3) -> Option<(f64, f64)> {
                None
            }
            fn arrival(&self, body: &FlyingBody, f: DVec3, t: DVec3, _dt: f64) -> Option<Met> {
                if t.y > 0.0 {
                    return None;
                }
                let at = surface_crossing(f, t, |p| p.y <= 0.0);
                let (energy_j, momentum) = delivered(body, DVec3::ZERO, None);
                Some(Met {
                    at,
                    energy_j,
                    momentum,
                })
            }
            fn air_scale_height_m(&self) -> f64 {
                0.0 // airless
            }
        }
        let mats = crate::materials::load();
        let iron = crate::materials::index_of(&mats, "iron");
        let (h0, v0, mass) = (1000.0, 100.0, 10.0);
        let mut flight = Flight::default();
        flight.introduce(FlyingBody::fresh(
            DVec3::new(0.0, h0, 0.0),
            DVec3::new(v0, 0.0, 0.0),
            mass,
            iron,
            0.1,
            100.0,
        ));
        // 1,200 steps = 60 s. The drop needs t = sqrt(2h/g) = 35.1 s at lunar gravity; the ORIGINAL loop
        // ran 400 steps = 20 s and so never landed at all, which is exactly why the arrival below was
        // worth asserting.
        let mut arrivals = Vec::new();
        for _ in 0..1200 {
            arrivals = flight.step(&Airless, &mats, 0.05);
            if !arrivals.is_empty() {
                break;
            }
        }
        assert_eq!(flight.trail().mass(), 0.0, "no air, nothing ablates");
        assert_eq!(flight.burned_up(), 0, "and nothing burns up");

        // **A vacuous world still takes impacts.** The test above breaks out of its loop on the first
        // arrival but never asserted that one HAPPENED, so every claim in it also held for a body that
        // simply never landed. On an airless world the arrival is the whole point: no air means nothing
        // slows the body down, so the ground gets ALL of it.
        let a = *arrivals
            .first()
            .expect("an airless world still receives the impact");
        assert!(
            a.at.y.abs() < 1e-6,
            "and it is sited on the surface ({:.3e} m off)",
            a.at.y
        );
        assert_eq!(
            a.body.mass_kg, mass,
            "nothing was ablated away on the way down"
        );

        // Energy is the launch KE plus the work gravity did over the drop — the closed form, because in
        // vacuum there is nothing else to spend it on. On an atmosphere-bearing world this is exactly the
        // quantity drag eats into, which is why the same drop delivers less through air.
        let expect = 0.5 * mass * v0 * v0 + mass * 1.62 * h0;
        assert!(
            (a.energy_j - expect).abs() < 0.01 * expect,
            "vacuum arrival carries the full ½mv₀² + mgh = {expect:.0} J, got {:.0} J",
            a.energy_j
        );
        assert!(
            flight.bodies().is_empty(),
            "and the body is no longer in flight — it arrived"
        );
    }
}
