//! **One entry point for "two things met — what does the engine do?"**
//!
//! The premise of this engine is that Theia striking proto-Earth and a raindrop striking a petal are the
//! same mechanic, differing in energy and in whether matter must be resolved — never in the rules or the
//! code path. The laws for both already existed and were both correct:
//!
//!   * how MUCH matter an interaction makes real — `damage::crater_volume`, E/σ against the struck
//!     material's own strength — which the ground scene used;
//!   * WHEN two bodies can no longer be treated as points — `accretion::resolution_distance`, from the
//!     tides — which the impact scene used.
//!
//! Neither scene knew about the other's, so a third scene would have found neither and written a third
//! path. That is how "the same mechanic implemented twice" happens: not by argument, but by a new author
//! reasonably not finding what already exists. This module is the one door, and it delegates — it does
//! not reimplement.
//!
//! **Two branches, one door (docs/58/59).** A trajectory can meet two kinds of thing, and both are this
//! module's business:
//!
//!   * **hard matter** — the SOLID branch, [`detect_swept`]: surfaces meet at an instant, and the
//!     response excavates matter or resolves the bodies;
//!   * **an atmosphere** — the FLUID branch, [`detect_atmospheric`]: a body carrying declared air is
//!     something to collide WITH, and the response is `atmosphere::atmospheric_step` applied along the
//!     path (drag, aeroheating, ablation).
//!
//! They are reported differently because they ARE different — an impact is an event, flight through air
//! is a state — and they compose the way the physics does: a body slows and ablates through the air, and
//! whatever survives arrives at the surface as a solid-branch collision. Neither belongs to a scene:
//! "there is a meteor entering the atmosphere" is a fact about the bodies the engine already holds, so
//! any scene that hands the engine its bodies gets entry physics without writing any.
//!
//! **And the scenes do not walk through the door.** A scene declares which bodies exist and where; it
//! never reaches into collision, never assembles an interaction, never asks whether two things hit. The
//! ENGINE holds the bodies — their mass, radius, velocity, spin — so the engine is the one that knows a
//! collision is coming, and it is the one that prepares for it. `detect` is that owner: hand it the
//! bodies and a step, and it forecasts every imminent contact on the continuous trajectory and returns
//! the response for each. A scene that could construct an `Interaction` by hand is a scene reaching into
//! the engine's job; the engine constructs them, from what it already holds.

use glam::DVec3;

/// A body as the engine holds it — everything the collision owner needs to forecast and size a contact.
/// The scene supplies these (which bodies, where, how fast); the engine reads them.
#[derive(Debug, Clone, Copy)]
pub struct BodyState {
    pub pos: DVec3,
    pub vel: DVec3,
    pub mass_kg: f64,
    pub radius_m: f64,
    /// Yield strength of this body's surface material (Pa) — what resists being excavated when something
    /// strikes it. From the body's own material, never declared by a scene.
    pub strength_pa: f64,
    /// This body's own atmosphere, if it has one — what another body's trajectory collides with on the
    /// FLUID branch. `None` for an airless body (and for a body whose air we have not characterised,
    /// which is vacuum honestly rather than a guess). Emerges from the body's declared air mass.
    pub air: Option<crate::atmosphere::AirShell>,
}

/// A contact the engine detected on its own — everything a response needs, computed by the engine from
/// the bodies it holds. A scene reads these; it does not compute them.
#[derive(Debug, Clone, Copy)]
pub struct DetectedCollision {
    /// Indices into the body slice: the struck body (the more massive) and the striking one.
    pub struck: usize,
    pub striker: usize,
    /// Fraction of the step at which contact first occurs (0 = already touching).
    pub toi: f64,
    /// The contact point, in world coordinates.
    pub site: DVec3,
    /// The TRUE relative velocity at the moment of contact — recovered from the conservation laws
    /// (vis-viva + angular momentum), NOT the raw post-step sample, which fast-forward renders garbage.
    pub contact_velocity: DVec3,
    /// Reduced-mass impact energy at contact (J): ½·μ·v_contact².
    pub energy_j: f64,
    pub response: Response,
}

/// Two things meeting, described physically.
#[derive(Debug, Clone, Copy)]
pub struct Interaction {
    /// Kinetic energy available to the interaction (J).
    pub energy_j: f64,
    /// Yield strength of the struck material (Pa) — what resists being excavated.
    pub strength_pa: f64,
    /// Current separation of the two bodies' centres (m).
    pub separation_m: f64,
    /// (mass kg, radius m) for the struck body and the striking one, in that order.
    pub bodies: [(f64, f64); 2],
    /// Where it happens, for the caller's convenience.
    pub at: DVec3,
}

/// What the engine should do about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Response {
    /// Far apart and nothing is happening: they stay whole bodies, and cost nothing.
    Untouched,
    /// Close enough that tides make "two point masses" a lie: resolve the BODIES into matter.
    ResolveBodies,
    /// Contact: this much of the struck material becomes real matter, over this radius.
    ResolveMatter { volume_m3: f64, radius_m: f64 },
}

impl Interaction {
    /// Are the bodies touching?
    pub fn in_contact(&self) -> bool {
        self.separation_m <= self.bodies[0].1 + self.bodies[1].1
    }
}

/// **The decision.** Contact excavates matter; approach within the tidal distance resolves the bodies;
/// anything else leaves them alone.
///
/// Every branch delegates to the law that already owned it, so there is one implementation of each and
/// one place to find them.
pub fn respond(i: &Interaction) -> Response {
    if i.in_contact() && i.energy_j > 0.0 {
        let volume_m3 = crate::damage::crater_volume(i.energy_j, i.strength_pa);
        return Response::ResolveMatter {
            volume_m3,
            radius_m: crate::damage::crater_radius(volume_m3),
        };
    }
    let (m_struck, r_struck) = i.bodies[0];
    let (m_striker, _) = i.bodies[1];
    let resolve_at = crate::accretion::resolution_distance(
        m_struck,
        r_struck,
        m_striker,
        crate::accretion::RESOLVE_TIDAL_FRACTION,
    );
    if i.separation_m <= resolve_at {
        Response::ResolveBodies
    } else {
        Response::Untouched
    }
}

/// **The engine detecting its own collisions.** Sweep every ordered pair of bodies, forecast contact on
/// the continuous path over the coming step (so a fast body cannot tunnel through a slow one between
/// samples), and for each imminent contact BUILD the interaction and decide the response — all from the
/// bodies the engine already holds. No scene is consulted, and none can be: the inputs are the engine's
/// own state.
///
/// `struck` is whichever body is more massive (the smaller one is the impactor); the interaction's energy
/// is ½·μ·v_rel² with the reduced mass μ, which is the energy actually available at the contact frame,
/// not either body's kinetic energy in an arbitrary frame.
pub fn detect(bodies: &[BodyState], dt: f64) -> Vec<DetectedCollision> {
    // Linear projection of where each body will be — the convenience entry for a caller holding only the
    // current state. The scene path uses `detect_swept` with its real integrated endpoints.
    let after: Vec<DVec3> = bodies.iter().map(|b| b.pos + b.vel * dt).collect();
    let active = vec![true; bodies.len()];
    detect_swept(bodies, &after, &active)
}

/// **The detection core.** `before` is every body's state at the START of the step; `after_pos` is where
/// the integrator ACTUALLY put each one (so gravity's curvature within the step is respected, not
/// linearised away); `active[i]` is false for a body already resolved this event, so it is not detected
/// twice.
///
/// Sweeps every ordered pair, forecasts contact on the continuous segment (a fast body cannot tunnel
/// through a slow one between samples), and for each hit recovers the TRUE contact state from the
/// conservation laws — the vis-viva speed at the surface and the angular-momentum tangent — rather than
/// trusting a post-step sample. The reduced-mass energy uses that contact speed. Everything here is the
/// engine reading its own state; nothing is handed in by a scene.
pub fn detect_swept(
    before: &[BodyState],
    after_pos: &[DVec3],
    active: &[bool],
) -> Vec<DetectedCollision> {
    let mut out = Vec::new();
    for a in 0..before.len() {
        for b in (a + 1)..before.len() {
            if !active[a] || !active[b] {
                continue;
            }
            let (ba, bb) = (before[a], before[b]);
            let r_sum = ba.radius_m + bb.radius_m;
            let rel_old = bb.pos - ba.pos;
            let rel_new = after_pos[b] - after_pos[a];
            let Some(toi) = crate::orbit::swept_first_contact(rel_old, rel_new, r_sum) else {
                continue;
            };
            // The more massive body is struck; the lighter is the impactor.
            let (struck, striker) = if ba.mass_kg >= bb.mass_kg { (a, b) } else { (b, a) };
            let (sbody, kbody) = (before[struck], before[striker]);
            // Relative kinematics in the struck body's frame — the frame `contact_velocity` works in.
            let rel_old_s = kbody.pos - sbody.pos;
            let vel_old_s = kbody.vel - sbody.vel;
            let rel_contact = rel_old_s + ((after_pos[striker] - after_pos[struck]) - rel_old_s) * toi;
            let n_hat = rel_contact.normalize_or_zero();
            let mu_grav = crate::orbit::G * (sbody.mass_kg + kbody.mass_kg);
            let contact_velocity =
                crate::orbit::contact_velocity(rel_old_s, vel_old_s, n_hat, r_sum, mu_grav);
            let m_red = sbody.mass_kg * kbody.mass_kg / (sbody.mass_kg + kbody.mass_kg).max(1e-30);
            let energy_j = 0.5 * m_red * contact_velocity.length_squared();
            let site = after_pos[struck] + rel_contact;
            // A forecast time-of-impact IS contact, so the interaction's separation is exactly the contact
            // radius — never a subtracted float that lands a hair outside it and makes `respond` deny a
            // collision the engine just forecast.
            let response = respond(&Interaction {
                energy_j,
                strength_pa: sbody.strength_pa,
                separation_m: r_sum,
                bodies: [(sbody.mass_kg, sbody.radius_m), (kbody.mass_kg, kbody.radius_m)],
                at: site,
            });
            out.push(DetectedCollision {
                struck,
                striker,
                toi,
                site,
                contact_velocity,
                energy_j,
                response,
            });
        }
    }
    out
}

/// **A body flying through another body's air** — the FLUID branch of "two things met" (docs/58/59).
///
/// The solid branch ([`DetectedCollision`]) is a point EVENT: two surfaces meet at an instant, at a site,
/// with an energy. The fluid branch is not — an atmosphere has no surface to meet, and the interaction is
/// spread continuously along the path. So it is reported as a STATE ("this body is in that body's air,
/// this thickly") rather than as an event, and the caller applies [`crate::atmosphere::atmospheric_step`]
/// with it each step. The two compose exactly as the physics does: a body ablates and slows through the
/// air, and whatever mass survives arrives at the hard surface as a solid-branch collision.
#[derive(Debug, Clone, Copy)]
pub struct AtmosphericContact {
    /// Index of the body whose atmosphere this is.
    pub host: usize,
    /// Index of the body flying through it.
    pub body: usize,
    /// Altitude above the host's surface at the START of the step (m) — where `rho` was read.
    pub altitude_m: f64,
    /// Air density at that altitude (kg/m³): the value the step integrates with.
    pub rho: f64,
    /// The LOWEST altitude the swept path reaches this step (m). Equal to `altitude_m` for a body moving
    /// slowly relative to the scale height; far below it for one plunging in, which is the caller's signal
    /// that this step spans a large density change and wants substepping.
    pub min_altitude_m: f64,
    /// Velocity RELATIVE TO THE AIR (m/s) — the host's velocity subtracted, because drag and heating are
    /// about motion through the gas, not motion in whatever frame the scene happens to use. (The air is
    /// taken to move with the body's centre; a co-ROTATING atmosphere, which adds up to ~465 m/s at
    /// Earth's equator, is the flagged refinement.)
    pub rel_vel: DVec3,
}

/// **The engine detecting a body in the air** — the fluid-branch counterpart of [`detect_swept`], and the
/// thing that makes atmospheric entry a CAPABILITY rather than a scene's feature (docs/59): a scene says
/// which bodies exist and which of them carry air, and the engine works out who is flying through what.
///
/// Sweeps every (host-with-air, body) pair over the step, so a body fast enough to cross the air between
/// two samples is still caught ([`crate::orbit::swept_min_distance`]). Whether the air REACHES a body at
/// all is [`crate::atmosphere::air_reaches`] — a derived bound, not an altitude anyone picked.
///
/// The density reported is the one at the step's start: this is an explicit integration, exactly like
/// every other force the engine applies, and its error vanishes with `dt` like the rest. `min_altitude_m`
/// is what tells a caller its step was too coarse to resolve the profile.
pub fn detect_atmospheric(
    before: &[BodyState],
    after_pos: &[DVec3],
    active: &[bool],
    dt: f64,
) -> Vec<AtmosphericContact> {
    let mut out = Vec::new();
    for host in 0..before.len() {
        let Some(air) = before[host].air else { continue };
        if !air.exists() || !active[host] {
            continue;
        }
        for body in 0..before.len() {
            if body == host || !active[body] {
                continue;
            }
            let (h, b) = (before[host], before[body]);
            let rel_old = b.pos - h.pos;
            let rel_new = after_pos[body] - after_pos[host];
            let altitude_m = (rel_old.length() - h.radius_m).max(0.0);
            let min_altitude_m = (crate::orbit::swept_min_distance(rel_old, rel_new) - h.radius_m).max(0.0);
            // Relative to the AIR, which moves with its host.
            let rel_vel = b.vel - h.vel;
            // The air reaches this body if it reaches it ANYWHERE on the path — judged at the deepest
            // point, so a body that dips into the atmosphere mid-step is not missed. The density it is
            // then stepped with is the one where it actually is.
            if !crate::atmosphere::air_reaches(
                air.density_at(min_altitude_m), rel_vel, b.mass_kg, b.radius_m, dt,
            ) {
                continue;
            }
            out.push(AtmosphericContact {
                host,
                body,
                altitude_m,
                rho: air.density_at(altitude_m),
                min_altitude_m,
                rel_vel,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A giant impact and a raindrop, through the same door.**
    ///
    /// This is the engine's premise stated as a test: one function, eleven orders of magnitude apart,
    /// giving each the answer its own physics demands. If these two ever need different code, that is the
    /// bug — not the scale.
    #[test]
    fn one_entry_point_serves_a_giant_impact_and_a_raindrop() {
        // Theia into proto-Earth. Basalt-ish yield.
        let giant = Interaction {
            energy_j: 7.0e30,
            strength_pa: 1.0e8,
            separation_m: 9.0e6, // inside contact
            bodies: [(5.435e24, 6.161e6), (6.477e23, 3.39e6)],
            at: DVec3::ZERO,
        };
        // A 3 mm raindrop onto a petal, at terminal velocity (~8 m/s, ~14 µJ). Petal tissue is weak.
        let drop = Interaction {
            energy_j: 1.4e-5,
            strength_pa: 1.0e5,
            separation_m: 1.6e-3, // touching
            bodies: [(1.0e-4, 1.5e-3), (1.4e-5, 1.5e-3)],
            at: DVec3::ZERO,
        };

        for (what, i) in [("the giant impact", giant), ("the raindrop", drop)] {
            match respond(&i) {
                Response::ResolveMatter { volume_m3, radius_m } => {
                    assert!(volume_m3 > 0.0 && radius_m > 0.0, "{what} excavates something");
                    // E/σ, exactly — the same law, not a scaled copy of it.
                    assert!(
                        (volume_m3 - i.energy_j / i.strength_pa).abs() < 1e-9 * volume_m3.max(1e-12),
                        "{what} is sized by E/σ"
                    );
                }
                other => panic!("{what} is in contact and must resolve matter, got {other:?}"),
            }
        }

        // The resolved volumes differ by the ratio the ENERGIES differ by — the law is scale-free, and
        // that is what lets one engine do both.
        let vol = |i: &Interaction| match respond(i) {
            Response::ResolveMatter { volume_m3, .. } => volume_m3,
            _ => unreachable!(),
        };
        let ratio = vol(&giant) / vol(&drop);
        let expected = (giant.energy_j / giant.strength_pa) / (drop.energy_j / drop.strength_pa);
        assert!((ratio / expected - 1.0).abs() < 1e-9, "the ratio is the physics, not a special case");
        assert!(ratio > 1e30, "and they really are worlds apart ({ratio:.1e})");
    }

    /// Approach, contact, and the quiet in between — one function decides all three.
    #[test]
    fn the_same_pair_moves_through_untouched_then_resolve_then_contact() {
        let mk = |sep: f64, energy: f64| Interaction {
            energy_j: energy,
            strength_pa: 1.0e8,
            separation_m: sep,
            bodies: [(5.435e24, 6.161e6), (6.477e23, 3.39e6)],
            at: DVec3::ZERO,
        };
        // Far out: two bodies, nothing to do, nothing to pay for.
        assert_eq!(respond(&mk(4.0e8, 0.0)), Response::Untouched, "far apart ⇒ whole bodies");
        // Inside the tidal distance (~17,700 km): the point-mass description has stopped being true.
        assert_eq!(respond(&mk(1.5e7, 0.0)), Response::ResolveBodies, "tides ⇒ resolve the bodies");
        // Touching, with energy: matter.
        assert!(matches!(respond(&mk(9.0e6, 7.0e30)), Response::ResolveMatter { .. }), "contact ⇒ matter");

        // A grazing touch with NO energy excavates nothing — the response follows the physics, not the
        // geometry alone.
        assert_eq!(respond(&mk(9.0e6, 0.0)), Response::ResolveBodies, "contact without energy ⇒ no crater");
    }

    /// **The engine finds the collision itself.** No `Interaction` is constructed here — the test hands
    /// `detect` a set of bodies, exactly what the engine already holds, and the engine forecasts the
    /// contact, sizes it, and decides. A scene's only contribution is having placed the bodies.
    #[test]
    fn the_engine_detects_and_prepares_a_collision_from_bodies_alone() {
        // A small fast body aimed at a large slow one.
        let bodies = [
            BodyState { pos: DVec3::ZERO, vel: DVec3::ZERO, mass_kg: 5.972e24, radius_m: 6.371e6, strength_pa: 1.0e8, air: None },
            BodyState {
                pos: DVec3::new(2.0e7, 0.0, 0.0),
                vel: DVec3::new(-1.1e4, 0.0, 0.0),
                mass_kg: 7.342e22,
                radius_m: 1.737e6,
                strength_pa: 1.0e8,
                air: None,
            },
        ];
        // One step long enough that the impactor crosses the gap — the engine must forecast the contact.
        let hits = detect(&bodies, 2000.0);
        assert_eq!(hits.len(), 1, "the engine finds exactly the one collision");
        let h = hits[0];
        assert_eq!(h.struck, 0, "the more massive body is the one struck");
        assert_eq!(h.striker, 1, "the lighter one is the impactor");
        assert!((0.0..=1.0).contains(&h.toi), "contact is forecast within the step ({})", h.toi);
        assert!(matches!(h.response, Response::ResolveMatter { .. }), "a real hit resolves matter");

        // Bodies flying apart are not a collision, however close they pass.
        let apart = [
            bodies[0],
            BodyState { vel: DVec3::new(1.1e4, 0.0, 0.0), ..bodies[1] },
        ];
        assert!(detect(&apart, 2000.0).is_empty(), "receding bodies do not collide");
    }

    /// Earth's air, built the way a scene would get it: from the body's own declared matter.
    fn earth_air() -> crate::atmosphere::AirShell {
        let mats = crate::materials::load();
        let air = &mats[crate::materials::index_of(&mats, "air")];
        let earth = crate::planet::earth();
        crate::atmosphere::AirShell::new(
            earth.surface_pressure(),
            air,
            earth.temperature_at(earth.radius()),
            earth.gravity_at(earth.radius()),
        )
    }

    fn planet_with_air(air: Option<crate::atmosphere::AirShell>) -> BodyState {
        BodyState {
            pos: DVec3::ZERO,
            vel: DVec3::ZERO,
            mass_kg: 5.972e24,
            radius_m: 6.371e6,
            strength_pa: 1.0e8,
            air,
        }
    }

    /// A metre-class rock on a real Earth-approach trajectory, `alt` metres up, falling straight down.
    fn incoming(alt: f64, speed: f64) -> BodyState {
        BodyState {
            pos: DVec3::new(0.0, 6.371e6 + alt, 0.0),
            vel: DVec3::new(0.0, -speed, 0.0),
            mass_kg: 3.3e4, // a 1 m iron sphere
            radius_m: 1.0,
            strength_pa: 1.0e8,
            air: None,
        }
    }

    /// **The engine finds a body in the air by itself.** Nothing here says "this is a meteor" or "entry
    /// starts at 100 km": one body carries declared air, another flies at it, and the fluid branch reports
    /// the contact — the same way `detect_swept` reports a solid one.
    #[test]
    fn the_engine_detects_a_body_flying_through_another_bodys_air() {
        let bodies = [planet_with_air(Some(earth_air())), incoming(80_000.0, 2.0e4)];
        let dt = 0.5;
        let after: Vec<DVec3> = bodies.iter().map(|b| b.pos + b.vel * dt).collect();
        let hits = detect_atmospheric(&bodies, &after, &[true, true], dt);
        assert_eq!(hits.len(), 1, "the rock is in the planet's air");
        let h = hits[0];
        assert_eq!((h.host, h.body), (0, 1), "the air belongs to the planet, the flight to the rock");
        assert!((h.altitude_m - 80_000.0).abs() < 1.0, "altitude is above the SURFACE, got {}", h.altitude_m);
        assert!(h.rho > 0.0 && h.rho < 1.0e-3, "80 km air is real but thin ({:.2e} kg/m³)", h.rho);
        assert!(h.min_altitude_m < h.altitude_m, "the swept path goes deeper than where it started");

        // An airless planet is not something to fly through. Same geometry, same speeds.
        let airless = [planet_with_air(None), incoming(80_000.0, 2.0e4)];
        assert!(
            detect_atmospheric(&airless, &after, &[true, true], dt).is_empty(),
            "an airless body has no fluid branch — vacuum, not thin air"
        );
    }

    /// **The air's edge is derived, and it belongs to the BODY.** Two bodies at the SAME altitude, same
    /// speed, differing only in how much air they present per unit mass: the fluffier one is still in the
    /// atmosphere where the dense one has left it. No altitude decides this — [`air_reaches`] does, and it
    /// is a statement about whether the drag can still change the answer.
    #[test]
    fn where_the_atmosphere_ends_depends_on_the_body_not_on_a_declared_altitude() {
        let air = earth_air();
        let dt = 1.0;
        // MEASURED (bisecting `air_reaches` on Earth's own emergent air, ρ₀=1.207 kg/m³, H=8367 m, for a
        // 1 s step at 20 km/s): the 1 m iron sphere's air ends at 296 km, the 1000×-lighter one's at
        // 354 km. Both are far above the Kármán line, at densities of order 1e-16 kg/m³ — which is the
        // point: the derived bound is nowhere near any altitude convention, and it is not the same
        // altitude for two different bodies. 320 km sits between the two.
        let alt = 320_000.0;
        let dense = BodyState { mass_kg: 3.3e4, radius_m: 1.0, ..incoming(alt, 2.0e4) };
        // Same size, a thousandth of the mass: a thousand times more drag per kilogram.
        let fluffy = BodyState { mass_kg: 33.0, ..dense };

        let reach = |b: BodyState| {
            let bodies = [planet_with_air(Some(air)), b];
            let after: Vec<DVec3> = bodies.iter().map(|x| x.pos + x.vel * dt).collect();
            !detect_atmospheric(&bodies, &after, &[true, true], dt).is_empty()
        };
        assert!(reach(fluffy), "the light body is still flying through air at {alt} m");
        assert!(!reach(dense), "the dense one, in the same place at the same speed, is not");

        // And the bound really is "the arithmetic cannot see it": at the altitude where the dense body
        // drops out, adding a step of drag does not change its speed at all.
        let rho = air.density_at(alt);
        let step = crate::atmosphere::atmospheric_step(
            rho, dense.vel, dense.mass_kg, dense.radius_m, 300.0, air.ambient_temp_k,
            &crate::materials::load()[crate::materials::index_of(&crate::materials::load(), "iron")], dt,
        );
        let v0 = dense.vel;
        assert_eq!((v0 + step.drag_accel * dt).length(), v0.length(), "the air it left changes nothing");
    }

    /// **A body cannot skim the atmosphere between two frames and be recorded as never having entered.**
    /// The endpoints of the step are both in vacuum; the path between them passes through the air. This is
    /// the fluid-branch twin of the tunnelling test below, and the reason detection is swept.
    #[test]
    fn a_body_that_grazes_the_air_between_samples_is_still_caught() {
        let air = earth_air();
        let planet = planet_with_air(Some(air));
        // A grazing pass: comes in high on one side, dips to 60 km at closest approach, leaves high. The
        // step is deliberately coarse (400 s) — at 20 km/s that puts both endpoints ~1200 km up, far
        // outside the ~350 km the air reaches even over a step this long.
        let dt = 400.0;
        let speed = 2.0e4;
        let peri = 6.371e6 + 60_000.0;
        let half = speed * dt * 0.5;
        let grazer = BodyState {
            pos: DVec3::new(-half, peri, 0.0),
            vel: DVec3::new(speed, 0.0, 0.0),
            mass_kg: 3.3e4,
            radius_m: 1.0,
            strength_pa: 1.0e8,
            air: None,
        };
        let bodies = [planet, grazer];
        let after: Vec<DVec3> = bodies.iter().map(|b| b.pos + b.vel * dt).collect();

        // Both endpoints are far too high for this body to feel anything — a sampler sees empty sky.
        let alt_of = |p: DVec3| (p - bodies[0].pos).length() - bodies[0].radius_m;
        for p in [bodies[1].pos, after[1]] {
            assert!(
                !crate::atmosphere::air_reaches(
                    air.density_at(alt_of(p)), grazer.vel, grazer.mass_kg, grazer.radius_m, dt
                ),
                "endpoint at {:.0} km is out of the air", alt_of(p) / 1000.0
            );
        }
        let hits = detect_atmospheric(&bodies, &after, &[true, true], dt);
        assert_eq!(hits.len(), 1, "but the swept path went through it");
        assert!(
            (hits[0].min_altitude_m - 60_000.0).abs() < 1.0,
            "and the caller is told how deep it got ({:.0} m)", hits[0].min_altitude_m
        );
    }

    /// **Forecasting, not sampling.** A body moving fast enough to jump the target between one step and
    /// the next must still be caught — this is the whole reason detection is the engine's job and not a
    /// per-frame `pos == pos` check a scene could fumble.
    #[test]
    fn a_body_that_would_tunnel_through_in_one_step_is_still_caught() {
        let target = BodyState { pos: DVec3::ZERO, vel: DVec3::ZERO, mass_kg: 6.0e24, radius_m: 6.4e6, strength_pa: 1.0e8, air: None };
        // Starts one side, ends the other side, in a single step — never sampled inside.
        let bullet = BodyState {
            pos: DVec3::new(-5.0e7, 0.0, 0.0),
            vel: DVec3::new(2.0e6, 0.0, 0.0), // 50 s later it is at +5e7, straight through
            mass_kg: 1.0e20,
            radius_m: 1.0e5,
            strength_pa: 1.0e8,
            air: None,
        };
        // A naive check at the endpoints sees no overlap; the swept forecast sees the crossing.
        let start_overlap = (bullet.pos - target.pos).length() < target.radius_m + bullet.radius_m;
        let end = bullet.pos + bullet.vel * 50.0;
        let end_overlap = (end - target.pos).length() < target.radius_m + bullet.radius_m;
        assert!(!start_overlap && !end_overlap, "neither endpoint overlaps — sampling would miss it");
        assert_eq!(detect(&[target, bullet], 50.0).len(), 1, "but the engine forecasts the tunneling hit");
    }
}
