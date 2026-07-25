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
    pub pos: DVec3,
    pub vel: DVec3,
    pub mass_kg: f64,
    /// Index into the material catalogue — what it is made of. Drag, heating and ablation all read this
    /// body's own material; nothing about flight is written for "a meteor".
    pub material: usize,
    /// Radius (m), from its mass at its material's density. SHRINKS as it ablates.
    pub radius_m: f64,
    /// Surface temperature (K). Rises from aeroheating in flight, and is what it glows AT.
    pub temp_k: f64,
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

    /// Has the path from `from` to `to` met hard matter? Returns where. This is the hand-off from the
    /// FLUID branch to the SOLID one (docs/58 "it's all impact"): whatever mass survives the air arrives
    /// here, and the caller excavates.
    fn arrival(&self, from: DVec3, to: DVec3) -> Option<DVec3>;
}

/// A body that has reached hard matter — everything an excavation needs, computed from the matter that
/// actually arrived rather than from what was launched.
#[derive(Clone, Copy, Debug)]
pub struct Arrival {
    pub body: FlyingBody,
    /// Where it struck.
    pub at: DVec3,
    /// Kinetic energy delivered (J): ½mv² of the mass that survived the flight. Never a parameter — a
    /// caller cannot ask for a bigger crater, only throw a bigger or faster rock.
    pub energy_j: f64,
}

/// **Everything in flight, and what it has shed.** One call introduces matter; one call advances it.
#[derive(Clone, Debug, Default)]
pub struct Flight {
    bodies: Vec<FlyingBody>,
    trail: Trail,
    burned_up: usize,
}

impl Flight {
    /// **Introduce matter on a trajectory.** This is the one door, and the only thing a scene does: it
    /// hands the engine a mass, a material, a place and a velocity. It cannot ask for an outcome.
    pub fn introduce(&mut self, body: FlyingBody) {
        self.bodies.push(body);
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
            self.introduce(FlyingBody {
                pos: origin + f.rel_pos,
                vel: approach + f.rel_vel,
                mass_kg: f.mass_kg,
                material,
                radius_m: f.radius_m,
                temp_k,
            });
        }
    }

    pub fn bodies(&self) -> &[FlyingBody] {
        &self.bodies
    }

    /// Resolve at most `n` trail parcels at once; beyond that the shed mass is booked into the air. The
    /// mass is identical either way — this is how finely it is held, not whether it exists.
    pub fn set_trail_budget(&mut self, n: usize) {
        self.trail.set_resolve_budget(n);
    }

    /// What the bodies have ablated away — still on the books (docs/59).
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
        let mut arrivals = Vec::new();
        let mut still = Vec::with_capacity(self.bodies.len());
        for mut b in self.bodies.drain(..) {
            // THE FLUID BRANCH. Nothing here is meteor-specific: it is the body's own material against
            // whatever air this world has at this point.
            if let (Some((rho, ambient)), Some(mat)) = (env.air_at(b.pos), mats.get(b.material)) {
                let s = crate::atmosphere::atmospheric_step(
                    rho, b.vel, b.mass_kg, b.radius_m, b.temp_k, ambient, mat, dt,
                );
                // The vaporised mass LEAVES the body; it does not leave the simulation.
                if s.ablated_mass > 0.0 {
                    self.trail.shed(s.ablated_mass, b.material, b.pos, b.vel, s.temp_k);
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
                self.burned_up += 1;
                continue;
            }
            let from = b.pos;
            b.vel += env.gravity_at(b.pos) * dt;
            b.pos += b.vel * dt;
            // THE SOLID BRANCH: whatever survived the air arrives, carrying the energy it actually has.
            if let Some(at) = env.arrival(from, b.pos) {
                let speed = b.vel.length();
                arrivals.push(Arrival { body: b, at, energy_j: 0.5 * b.mass_kg * speed * speed });
            } else {
                still.push(b);
            }
        }
        self.bodies = still;
        self.trail.step(mats, dt, |at| env.air_at(at).unwrap_or((0.0, 0.0)));
        arrivals
    }

    /// **Everything in flight, as it must be drawn** (`crate::Drawn`): the bodies and the vapour they have
    /// shed, in one list of physical facts. A scene draws this and never learns what a meteor is.
    ///
    /// Bodies come before trail parcels for the reason `Simulation::drawn` orders matter in flight first:
    /// a caller with a finite instance budget should lose the least informative matter.
    pub fn drawn(&self, env: &impl FlightEnvironment, to_scene: impl Fn(DVec3) -> glam::Vec3) -> Vec<crate::Drawn> {
        let mut out: Vec<crate::Drawn> = self
            .bodies
            .iter()
            .map(|b| crate::Drawn {
                pos: to_scene(b.pos),
                vel: b.vel.as_vec3(),
                radius_m: b.radius_m as f32,
                material: b.material,
                temp_k: b.temp_k as f32,
                resting: false,
            })
            .collect();
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
        out
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
        fn arrival(&self, _from: DVec3, to: DVec3) -> Option<DVec3> {
            (to.y <= 0.0).then_some(to)
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
        fn arrival(&self, _from: DVec3, to: DVec3) -> Option<DVec3> {
            (to.length() <= self.matter.radius()).then_some(to)
        }
    }

    fn earth_planet() -> (Planet, Vec<Material>) {
        let mats = crate::materials::load();
        let air_mat = &mats[crate::materials::index_of(&mats, "air")];
        let earth = crate::planet::earth();
        let air = crate::atmosphere::AirShell::new(
            earth.surface_pressure(), air_mat, 288.0, earth.gravity_at(earth.radius()),
        );
        (Planet { matter: earth, air }, mats)
    }

    /// **The same flight law, in a ground patch and around a planet.** Nothing in `Flight` knows which it
    /// is in: one implements "down" and a floor, the other radial gravity and a sphere, and the entry
    /// physics between them is the same code. If these ever needed different code, that would be the bug —
    /// not the eleven orders of magnitude between them.
    #[test]
    fn one_flight_law_serves_a_ground_patch_and_a_planet() {
        let (planet, mats) = earth_planet();
        let iron = crate::materials::index_of(&mats, "iron");
        let rho_iron = mats[iron].density as f64;
        let grain = |r: f64| rho_iron * (4.0 / 3.0) * std::f64::consts::PI * r.powi(3);

        // Ground patch: a rock dropped from 200 m arrives at the floor.
        let ground = FlatGround {
            g: 9.81,
            air: crate::atmosphere::AirShell::new(101_325.0, &mats[crate::materials::index_of(&mats, "air")], 288.0, 9.81),
        };
        let mut flight = Flight::default();
        flight.introduce(FlyingBody {
            pos: DVec3::new(0.0, 200.0, 0.0),
            vel: DVec3::ZERO,
            mass_kg: grain(0.1),
            material: iron,
            radius_m: 0.1,
            temp_k: 288.0,
        });
        let mut hit = None;
        for _ in 0..2000 {
            if let Some(a) = flight.step(&ground, &mats, 0.005).first() {
                hit = Some(*a);
                break;
            }
        }
        let a = hit.expect("the rock reaches the floor");
        assert!(a.energy_j > 0.0, "it arrives with the energy its own fall gave it");
        assert!(flight.bodies().is_empty(), "and is no longer in flight");

        // Planet: the SAME code, the same struct, from 300 km up at orbital-entry speed, falling in.
        let mut orbital = Flight::default();
        let r0 = planet.matter.radius() + 300_000.0;
        orbital.introduce(FlyingBody {
            pos: DVec3::new(0.0, r0, 0.0),
            vel: DVec3::new(0.0, -15_000.0, 0.0),
            mass_kg: grain(0.5),
            material: iron,
            radius_m: 0.5,
            temp_k: 288.0,
        });
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
        assert!(x.body.vel.length() < 15_000.0, "the air braked it ({:.0} m/s)", x.body.vel.length());
        assert!(x.body.temp_k > 288.0, "and heated it ({:.0} K)", x.body.temp_k);
    }

    /// **A swarm is two operations composed, not a feature.** `introduce_swarm` is `damage::disrupt` fed
    /// into `introduce`; the engine has nothing called "meteor".
    ///
    /// And what the swarm DOES is not written anywhere either. Ablation is a surface-to-volume effect —
    /// heating scales with frontal area, thermal mass with volume — so the smaller a fragment is, the
    /// larger the fraction of it the air takes. MEASURED here on Earth's own emergent atmosphere, eight
    /// fragments of an iron parent entering at 15 km/s from 200 km, as the parent grows:
    ///
    /// ```text
    /// parent r   fragment r        ablated
    ///   1 cm     3.3–7.5 mm         94.7%
    ///   2 cm     6.6–15  mm         69.9%
    ///   5 cm     1.6–3.8 cm         27.5%
    ///  10 cm     3.3–7.5 cm          6.0%
    ///  30 cm     9.9–23  cm          0
    ///   3 m      0.98–2.3 m          0
    /// ```
    ///
    /// That is the reason shooting stars are small and iron meteorites reach the ground, and nothing in
    /// the engine states it: it falls out of one heating law meeting one mass distribution. (The zeroes
    /// are also where docs/46 row 21 bites — `atmospheric_step` heats a body's whole mass at bulk heat
    /// capacity, so it understates ablation for anything past its thermal skin depth. The TREND is the
    /// physics; the exact cut-off is the flagged limit.)
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
                parent_m, parent_r, iron, 8, 86_400.0, 288.0,
            );
            assert_eq!(flight.bodies().len(), 8, "eight fragments, from one disrupted body");
            let launched: f64 = flight.bodies().iter().map(|b| b.mass_kg).sum();
            assert!((launched / parent_m - 1.0).abs() < 1e-9, "the swarm IS the parent's mass");

            let mut arrived_mass = 0.0;
            for _ in 0..60_000 {
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

        let f = [0.01_f64, 0.02, 0.05, 0.1, 0.3].map(ablated_fraction);
        for w in f.windows(2) {
            assert!(w[0] > w[1], "the air takes a larger share of a smaller fragment: {w:?}");
        }
        assert!(f[0] > 0.9, "millimetre pieces are almost entirely consumed ({:.1}%)", f[0] * 100.0);
        assert_eq!(f[4], 0.0, "decimetre iron reaches the ground intact — iron meteorites do");
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
        let parent_m = mats[iron].density as f64 * (4.0 / 3.0) * std::f64::consts::PI * parent_r.powi(3);

        let mut flight = Flight::default();
        flight.introduce_swarm(
            glam::DVec3::new(0.0, planet.matter.radius() + 500_000.0, 0.0),
            glam::DVec3::new(0.0, -17_000.0, 0.0),
            parent_m, parent_r, iron, 40, 86_400.0, 250.0,
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
        assert!(flight.trail().mass() > 0.0, "and the air must have taken some of them");
        assert!(steps > 1_000, "the entry must have taken real time, not exited immediately ({steps} steps)");
        assert!(
            flight.bodies().is_empty(),
            "every fragment must arrive or be consumed — {} still aloft after {:.0} s",
            flight.bodies().len(), steps as f64 * dt
        );
        assert!(
            flight.trail().parcels().is_empty(),
            "every parcel must become air — {} still resolved after {:.0} s",
            flight.trail().parcels().len(), steps as f64 * dt
        );
        assert_eq!(
            arrived + flight.burned_up(), launched,
            "{arrived} arrived + {} burned up must account for all {launched}", flight.burned_up()
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
            fn arrival(&self, _f: DVec3, t: DVec3) -> Option<DVec3> {
                (t.y <= 0.0).then_some(t)
            }
        }
        let mats = crate::materials::load();
        let iron = crate::materials::index_of(&mats, "iron");
        let mut flight = Flight::default();
        flight.introduce(FlyingBody {
            pos: DVec3::new(0.0, 1000.0, 0.0),
            vel: DVec3::new(100.0, 0.0, 0.0),
            mass_kg: 10.0,
            material: iron,
            radius_m: 0.1,
            temp_k: 100.0,
        });
        for _ in 0..400 {
            if !flight.step(&Airless, &mats, 0.05).is_empty() {
                break;
            }
        }
        assert_eq!(flight.trail().mass(), 0.0, "no air, nothing ablates");
        assert_eq!(flight.burned_up(), 0, "and nothing burns up");
    }
}
