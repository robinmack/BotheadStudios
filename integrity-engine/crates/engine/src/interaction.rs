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

use glam::DVec3;

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
}
