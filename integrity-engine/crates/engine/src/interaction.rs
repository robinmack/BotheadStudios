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
}

/// A contact the engine detected on its own, and what to do about it.
#[derive(Debug, Clone, Copy)]
pub struct DetectedCollision {
    /// Indices into the body slice: the struck body and the striking one.
    pub struck: usize,
    pub striker: usize,
    /// Fraction of the step at which contact first occurs (0 = already touching).
    pub toi: f64,
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
    let mut out = Vec::new();
    for a in 0..bodies.len() {
        for b in (a + 1)..bodies.len() {
            let (ba, bb) = (bodies[a], bodies[b]);
            let r_sum = ba.radius_m + bb.radius_m;
            let rel_old = bb.pos - ba.pos;
            let rel_new = (bb.pos + bb.vel * dt) - (ba.pos + ba.vel * dt);
            // Forecast, don't sample: the same continuous-trajectory test the N-body loop uses.
            let Some(toi) = crate::orbit::swept_first_contact(rel_old, rel_new, r_sum) else {
                continue;
            };
            // The more massive body is the one struck; the lighter is the impactor.
            let (struck, striker) = if ba.mass_kg >= bb.mass_kg { (a, b) } else { (b, a) };
            let (s, k) = (bodies[struck], bodies[striker]);
            let v_rel = (k.vel - s.vel).length();
            let mu = s.mass_kg * k.mass_kg / (s.mass_kg + k.mass_kg).max(1e-30);
            let energy_j = 0.5 * mu * v_rel * v_rel;
            // A forecast time-of-impact in [0,1] IS contact by definition, so the interaction's separation
            // is exactly the contact radius — not a subtracted float that can land a hair outside it and
            // make `respond` report "not touching" about a collision the engine just forecast.
            let interaction = Interaction {
                energy_j,
                strength_pa: s.strength_pa,
                separation_m: r_sum,
                bodies: [(s.mass_kg, s.radius_m), (k.mass_kg, k.radius_m)],
                at: s.pos + (k.pos - s.pos) * (s.radius_m / r_sum),
            };
            out.push(DetectedCollision { struck, striker, toi, response: respond(&interaction) });
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
            BodyState { pos: DVec3::ZERO, vel: DVec3::ZERO, mass_kg: 5.972e24, radius_m: 6.371e6, strength_pa: 1.0e8 },
            BodyState {
                pos: DVec3::new(2.0e7, 0.0, 0.0),
                vel: DVec3::new(-1.1e4, 0.0, 0.0),
                mass_kg: 7.342e22,
                radius_m: 1.737e6,
                strength_pa: 1.0e8,
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

    /// **Forecasting, not sampling.** A body moving fast enough to jump the target between one step and
    /// the next must still be caught — this is the whole reason detection is the engine's job and not a
    /// per-frame `pos == pos` check a scene could fumble.
    #[test]
    fn a_body_that_would_tunnel_through_in_one_step_is_still_caught() {
        let target = BodyState { pos: DVec3::ZERO, vel: DVec3::ZERO, mass_kg: 6.0e24, radius_m: 6.4e6, strength_pa: 1.0e8 };
        // Starts one side, ends the other side, in a single step — never sampled inside.
        let bullet = BodyState {
            pos: DVec3::new(-5.0e7, 0.0, 0.0),
            vel: DVec3::new(2.0e6, 0.0, 0.0), // 50 s later it is at +5e7, straight through
            mass_kg: 1.0e20,
            radius_m: 1.0e5,
            strength_pa: 1.0e8,
        };
        // A naive check at the endpoints sees no overlap; the swept forecast sees the crossing.
        let start_overlap = (bullet.pos - target.pos).length() < target.radius_m + bullet.radius_m;
        let end = bullet.pos + bullet.vel * 50.0;
        let end_overlap = (end - target.pos).length() < target.radius_m + bullet.radius_m;
        assert!(!start_overlap && !end_overlap, "neither endpoint overlaps — sampling would miss it");
        assert_eq!(detect(&[target, bullet], 50.0).len(), 1, "but the engine forecasts the tunneling hit");
    }
}
