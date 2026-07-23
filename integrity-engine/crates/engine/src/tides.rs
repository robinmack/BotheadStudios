//! Earth spin + tidal coupling (docs/27 roadmap #1).
//!
//! A planet's SPIN is real angular momentum that must be bookkept like everything else:
//! - the giant impact DELIVERS it (the impactor's off-centre momentum → the canonical ~5-hour
//!   post-impact day — never declared, it emerges from the encounter geometry);
//! - settled matter returns its orbital angular momentum to the planet when it demotes;
//! - the boundary's shear reaction torques the planet (measured, Newton's-third-law-exact, the same
//!   trick as the linear momentum mirror);
//! - and the spinning planet's TIDAL BULGE torques every moonlet: ahead of an orbiting body when the
//!   planet spins faster than the orbit, dragging it OUTWARD — the mechanism that carried the real
//!   Moon from ~3 R⊕ to 60 R⊕ over 4.5 Gyr (validated here against the measured 3.8 cm/yr recession).
//!
//! Declared parameters (constitutive summaries, cited): the tidal Love number k₂ and dissipation
//! factor Q — the same epistemic status as friction coefficients (real sub-resolution physics we
//! cannot derive yet, so we declare the measured value and flag it).

#![allow(dead_code)] // consumed by the wasm space band + native tests

use crate::orbit::{Body, G};
use glam::DVec3;

/// Earth's measured moment-of-inertia factor I/(M·R²) — 0.3307 (declared, like PREM; a uniform sphere
/// would be 0.4, the deficit is the dense core).
pub const EARTH_MOI_FACTOR: f64 = 0.3307;

/// Earth's tidal Love number k₂ (measured ≈ 0.298) over its effective dissipation factor Q. The
/// PRESENT-DAY effective k₂/Q ≈ 0.025 is dominated by ocean dissipation and reproduces the measured
/// lunar recession (3.8 cm/yr). HONESTY FLAG: a post-giant-impact magma-ocean Earth has a different
/// (debated) Q; we declare the modern measured value and note the uncertainty rather than invent one.
pub const EARTH_K2_OVER_Q: f64 = 0.025;

/// Moment of inertia of a planet about its spin axis.
pub fn moment_of_inertia(mass: f64, radius: f64) -> f64 {
    EARTH_MOI_FACTOR * mass * radius * radius
}

/// Spin period (seconds) from spin angular momentum and a GIVEN moment of inertia — the generic form
/// (docs/58), so a body's EMERGENT `LayeredBody::moment_of_inertia()` sets its day length rather than the
/// uniform-sphere `⅖mr²`. INFINITY for a non-spinning body.
pub fn spin_period_from_inertia(spin_l: DVec3, inertia: f64) -> f64 {
    let l = spin_l.length();
    if l <= 0.0 {
        return f64::INFINITY;
    }
    2.0 * std::f64::consts::PI * inertia / l
}

/// Spin period (seconds) from spin angular momentum. The HUD's "length of day".
pub fn spin_period_s(spin_l: DVec3, mass: f64, radius: f64) -> f64 {
    spin_period_from_inertia(spin_l, moment_of_inertia(mass, radius))
}

/// Total angular momentum of a particle cloud about `center` moving at `v_center`.
pub fn cloud_angular_momentum(particles: &[Body], center: DVec3, v_center: DVec3) -> DVec3 {
    particles
        .iter()
        .map(|p| (p.pos - center).cross((p.vel - v_center) * p.mass))
        .sum()
}

/// The Sun's torque on a cloud about the planet's centre, × dt = its angular impulse. Subtracted from
/// the cloud's measured ΔL so only the PLANET-cloud interactions (boundary shear, which must mirror
/// into the planet's spin) are attributed to the planet — the Sun's share belongs to the Sun.
pub fn sun_angular_impulse(
    particles: &[Body],
    center: DVec3,
    sun_pos: DVec3,
    sun_mass: f64,
    dt: f64,
) -> DVec3 {
    particles
        .iter()
        .map(|p| {
            let d = sun_pos - p.pos;
            let r2 = d.length_squared().max(1.0);
            let f = d * (G * sun_mass * p.mass * (1.0 / (r2 * r2.sqrt())));
            (p.pos - center).cross(f * dt)
        })
        .sum()
}

/// SECULAR tidal migration rate da/dt (m/s) for a circular-orbit satellite: the standard
/// `da/dt = 3·(k₂/Q)·(m/M)·(R/a)⁵·n·a`, signed by whether the planet's spin leads (outward) or trails
/// (inward) the orbit. This is the orbit-averaged law — the honest summary at time-LOD, exactly as the
/// conservation laws are at contact (docs/13: what we simulate must not depend on how coarsely we look).
pub fn tidal_da_dt(
    k2_over_q: f64,
    m_sat: f64,
    m_planet: f64,
    r_planet: f64,
    a: f64,
    spin_omega: f64,
) -> f64 {
    let n = (G * (m_planet + m_sat) / (a * a * a)).sqrt(); // orbital mean motion
    let rate = 3.0 * k2_over_q * (m_sat / m_planet) * (r_planet / a).powi(5) * n * a;
    rate * (spin_omega - n).signum()
}

/// The tidal TORQUE'S per-substep effect as a tangential acceleration on the satellite (and the
/// equal-and-opposite change to the planet's spin L, returned): from da/dt via the vis-viva relation
/// `dv/dt = (n/2)·da/dt` along the orbital direction. Angular momentum moves between spin and orbit;
/// none is created (the energy difference is dissipated as heat in the planet — not yet tracked
/// against a planetary temperature state, flagged).
pub fn tidal_kick(
    k2_over_q: f64,
    sat: &Body,
    planet_pos: DVec3,
    planet_vel: DVec3,
    m_planet: f64,
    r_planet: f64,
    spin_l: DVec3,
    dt: f64,
) -> (DVec3, DVec3) {
    let rel = sat.pos - planet_pos;
    let vel = sat.vel - planet_vel;
    let a = rel.length();
    if a <= r_planet {
        return (DVec3::ZERO, DVec3::ZERO);
    }
    let spin_omega = spin_l.length() / moment_of_inertia(m_planet, r_planet);
    // Spin direction defines "leading": torque only meaningfully defined for orbits with angular
    // momentum; a radial plunge has no tangential direction to push along.
    let h = rel.cross(vel);
    if h.length_squared() < 1.0e-9 {
        return (DVec3::ZERO, DVec3::ZERO);
    }
    // Prograde/retrograde relative to the spin decides the sign coupling; magnitude from the secular law.
    let n = (G * (m_planet + sat.mass) / (a * a * a)).sqrt();
    let da_dt = tidal_da_dt(k2_over_q, sat.mass, m_planet, r_planet, a, spin_omega)
        * h.normalize().dot(spin_l.normalize_or_zero()).signum();
    let dv = 0.5 * n * da_dt * dt; // vis-viva: dv = (n/2)·da along the orbit
    let t_hat = h.cross(rel).normalize_or_zero(); // h×r̂ IS the direction of motion (test-caught)
    let kick = t_hat * dv;
    // The satellite's ΔL_orbit about the planet = r × (m·Δv); the spin loses exactly that.
    let d_l_orbit = rel.cross(kick * sat.mass);
    (kick, -d_l_orbit)
}

/// Rotational FLATTENING from spin (Radau–Darwin): a spinning body bulges until the equipotential
/// balances centrifugal vs gravity. f = (5q/2) / (1 + (25/4)·(1 − (3/2)·C)²), with
/// q = ω²R³/(GM) (centrifugal/gravity ratio at the equator) and C the measured moment-of-inertia
/// factor — nothing new declared. ANCHORS: today's day ⇒ f ≈ 1/298 (the real flattening); the
/// emergent 3.8-h post-impact day ⇒ f ≈ 0.13, a visibly squashed proto-Earth (as models predict).
pub fn flattening_from_spin(spin_omega: f64, mass: f64, radius: f64) -> f64 {
    // DOMAIN clamp: Radau–Darwin is a small-flattening theory. Past q ≈ 0.3 the body approaches
    // rotational breakup (mass shedding — not yet modelled, flagged); extrapolating the formula gave
    // f ≈ 2.4 and a NEGATIVE polar radius on screen. Clamp, honestly labelled.
    let q = (spin_omega * spin_omega * radius.powi(3) / (G * mass)).min(0.3);
    let eta = 1.0 - 1.5 * EARTH_MOI_FACTOR;
    (2.5 * q) / (1.0 + 6.25 * eta * eta)
}

/// The oblate figure's J₂ gravity coefficient, first order: J₂ = (2/3)·(f − q/2).
/// ANCHOR: today's Earth ⇒ 1.08e-3 (measured: 1.0826e-3).
pub fn j2_from_spin(spin_omega: f64, mass: f64, radius: f64) -> f64 {
    let q = (spin_omega * spin_omega * radius.powi(3) / (G * mass)).min(0.3); // domain clamp (see above)
    let f = flattening_from_spin(spin_omega, mass, radius);
    (2.0 / 3.0) * (f - 0.5 * q)
}

/// The J₂ (oblateness) gravitational perturbation on a satellite at `rel` from the planet centre,
/// spin axis `s_hat`: a = −(3/2)·J₂·μ·R²/r⁴ · [(1 − 5u²)·r̂ + 2u·ŝ], u = r̂·ŝ. This is what makes
/// close orbits around an oblate world precess — the gravity profile Robin asked about.
pub fn j2_accel(rel: DVec3, mu: f64, r_planet: f64, j2: f64, s_hat: DVec3) -> DVec3 {
    let r = rel.length();
    if r < 1.0e-9 || j2 == 0.0 {
        return DVec3::ZERO;
    }
    let r_hat = rel / r;
    let u = r_hat.dot(s_hat);
    let k = -1.5 * j2 * mu * r_planet * r_planet / (r * r * r * r);
    (r_hat * (1.0 - 5.0 * u * u) + s_hat * (2.0 * u)) * k
}

/// A moonlet at GEOLOGIC time-LOD: a settled rubble pile on a stable orbit is ONE BODY described by
/// its orbital elements (the scale-relative promotion in reverse — docs/13: when nothing eventful
/// happens for many orbits, the orbit-averaged secular equations ARE the honest physics, exactly as
/// the conservation laws are at contact).
#[derive(Clone, Copy, Debug)]
pub struct Moonlet {
    pub a: f64,    // semi-major axis (m)
    pub mass: f64, // kg
}

/// One SECULAR step of `dt` (seconds — typically years to millennia): every moonlet migrates by the
/// validated tidal law, the planet's spin pays for it (angular momentum moves, never appears), and
/// moonlets whose orbits close within 3.5 mutual Hill radii MERGE (the standard planetesimal
/// stability criterion, Gladman 1993) — conserving mass and orbital angular momentum exactly
/// (a_new from L₁+L₂ = (m₁+m₂)·√(G·M·a_new)). Returns the number of mergers.
///
/// A moonlet whose orbit decays INSIDE the Roche limit is tidally SHREDDED (a rubble pile cannot hold
/// itself together against the tidal field there) — it is removed and its mass + orbital angular momentum
/// rain onto the planet. This is why a sub-synchronous moonlet does not "roll on the surface": it disrupts.
/// Returns `(mergers, mass shed onto the planet)`.
pub fn secular_step(
    moonlets: &mut Vec<Moonlet>,
    spin_l: &mut DVec3,
    m_planet: f64,
    r_planet: f64,
    k2_over_q: f64,
    dt: f64,
) -> (usize, f64) {
    let spin_omega = spin_l.length() / moment_of_inertia(m_planet, r_planet);
    let s_hat = spin_l.normalize_or_zero();
    // Fluid Roche limit d = 2.44·R·(ρ_planet/ρ_moon)^⅓ — inside it a self-gravitating rubble moon is torn
    // apart (the real Moon formed just OUTSIDE it and migrated out). ρ_moon = basalt rubble (docs/27).
    let rho_planet = m_planet / (4.0 / 3.0 * std::f64::consts::PI * r_planet.powi(3));
    let d_roche = 2.44 * r_planet * (rho_planet / 2_900.0).cbrt();
    for m in moonlets.iter_mut() {
        // Per-step da capped at 5% of a: the secular law is a rate, not a leap (early migration at
        // 3 R⊕ is ferocious; an uncapped step overshoots the nonlinear (R/a)⁵ falloff).
        let da = (tidal_da_dt(k2_over_q, m.mass, m_planet, r_planet, m.a, spin_omega) * dt)
            .clamp(-0.05 * m.a, 0.05 * m.a);
        // No floor: a sub-synchronous orbit decays freely toward Roche, where it disrupts (below).
        let a_new = (m.a + da).max(1.0);
        // The spin pays the EXACT orbital-L change (L = m·√(G·M·a) is an identity, not a Taylor
        // expansion — first-order payment leaked 1.7% over an early-fast migration, measured).
        let dl = m.mass * (G * m_planet).sqrt() * (a_new.sqrt() - m.a.sqrt());
        m.a = a_new;
        *spin_l -= s_hat * dl;
    }
    // ROCHE DISRUPTION: shred moonlets that have decayed inside the Roche limit; their mass and orbital
    // angular momentum go to the planet (it swallows the infalling rubble and spins up). Total mass and
    // angular momentum are conserved (mass returned to the caller; L added to the spin here).
    let mut shed = 0.0;
    moonlets.retain(|m| {
        if m.a < d_roche {
            shed += m.mass;
            *spin_l += s_hat * (m.mass * (G * m_planet * m.a).sqrt());
            false
        } else {
            true
        }
    });
    // Hill-criterion mergers, innermost outward.
    moonlets.sort_by(|x, y| x.a.partial_cmp(&y.a).unwrap());
    let mut merged = 0;
    let mut i = 0;
    while i + 1 < moonlets.len() {
        let (m1, m2) = (moonlets[i], moonlets[i + 1]);
        let r_hill = 0.5 * (m1.a + m2.a) * ((m1.mass + m2.mass) / (3.0 * m_planet)).powf(1.0 / 3.0);
        if (m2.a - m1.a) < 3.5 * r_hill {
            let mass = m1.mass + m2.mass;
            let l = m1.mass * (G * m_planet * m1.a).sqrt() + m2.mass * (G * m_planet * m2.a).sqrt();
            let a_new = (l / (mass * (G * m_planet).sqrt())).powi(2);
            moonlets[i] = Moonlet { a: a_new, mass };
            moonlets.remove(i + 1);
            merged += 1;
        } else {
            i += 1;
        }
    }
    (merged, shed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M_E: f64 = 5.972e24;
    const R_E: f64 = 6.371e6;
    const M_MOON: f64 = 7.342e22;

    #[test]
    fn todays_spin_yields_the_real_flattening_and_j2() {
        // Radau–Darwin + the declared MOI factor, nothing else: the measured day length must give the
        // measured figure of the Earth. f_real = 1/298.25 = 3.353e-3; J2_real = 1.0826e-3.
        let omega = 2.0 * std::f64::consts::PI / 86_164.0;
        let f = flattening_from_spin(omega, M_E, R_E);
        let j2 = j2_from_spin(omega, M_E, R_E);
        println!("flattening 1/{:.0} (real 1/298) · J2 {j2:.4e} (real 1.0826e-3)", 1.0 / f);
        assert!((f - 3.353e-3).abs() / 3.353e-3 < 0.05, "real flattening emerges (got {f:.3e})");
        assert!((j2 - 1.0826e-3).abs() / 1.0826e-3 < 0.08, "real J2 emerges (got {j2:.3e})");
        // And the post-impact 3.8-h day squashes the planet visibly.
        let f_fast = flattening_from_spin(2.0 * std::f64::consts::PI / (3.8 * 3600.0), M_E, R_E);
        assert!(f_fast > 0.08, "a 3.8-h day is dramatically oblate (got {f_fast:.3})");
    }

    #[test]
    fn the_declared_k2_q_reproduces_the_measured_lunar_recession() {
        // THE real-world anchor: laser ranging measures the Moon receding at 3.8 cm/yr. With today's
        // Earth spin (24 h) and the declared k₂/Q, the secular law must land on it.
        let a = 3.844e8;
        let spin_omega = 2.0 * std::f64::consts::PI / 86_164.0; // sidereal day
        let da_dt = tidal_da_dt(EARTH_K2_OVER_Q, M_MOON, M_E, R_E, a, spin_omega);
        let cm_per_year = da_dt * 3.156e7 * 100.0;
        println!("lunar recession: {cm_per_year:.2} cm/yr (measured: 3.8)");
        assert!(
            (2.8..5.0).contains(&cm_per_year),
            "declared k₂/Q reproduces the measured recession (got {cm_per_year:.2} cm/yr)"
        );
        assert!(da_dt > 0.0, "Earth spins faster than the Moon orbits ⇒ OUTWARD migration");
    }

    #[test]
    fn tides_pull_inward_when_the_planet_spins_slower_than_the_orbit() {
        // The sign law (Phobos' fate): a satellite orbiting FASTER than the planet spins is dragged
        // inward — the bulge trails it.
        let a = 2.0 * R_E; // close orbit: n large
        let slow_spin = 2.0 * std::f64::consts::PI / (30.0 * 86_400.0); // 30-day "day"
        let da_dt = tidal_da_dt(EARTH_K2_OVER_Q, M_MOON, M_E, R_E, a, slow_spin);
        assert!(da_dt < 0.0, "slow spin ⇒ trailing bulge ⇒ inward migration (got {da_dt:.3e})");
    }

    #[test]
    fn a_close_moonlet_of_a_fast_spinning_earth_migrates_outward_fast() {
        // Post-giant-impact configuration: 5-hour day, moonlet at 3 R⊕ — migration must be many
        // orders of magnitude faster than today's (the (R/a)⁵ leverage), or the Moon could never have
        // reached 60 R⊕ in the age of the Earth.
        let a = 3.0 * R_E;
        let spin_omega = 2.0 * std::f64::consts::PI / (5.0 * 3_600.0);
        let da_dt = tidal_da_dt(EARTH_K2_OVER_Q, 0.5 * M_MOON, M_E, R_E, a, spin_omega);
        let m_per_year = da_dt * 3.156e7;
        println!("post-impact moonlet: {m_per_year:.1} m/yr outward");
        assert!(
            m_per_year > 100.0,
            "close-in migration is fast (got {m_per_year:.1} m/yr)"
        );
    }

    #[test]
    fn geologic_time_merges_the_moonlets_and_migrates_the_moon_outward() {
        // The endgame (docs/27): three post-impact moonlets under secular tides must (a) MERGE into
        // one Moon (Hill instability), (b) migrate OUTWARD, (c) conserve total angular momentum —
        // the day lengthens as the orbit grows, exactly the real Earth–Moon history.
        let mut moonlets = vec![
            Moonlet { a: 2.6 * R_E, mass: 0.3 * M_MOON },
            Moonlet { a: 3.0 * R_E, mass: 0.4 * M_MOON },
            Moonlet { a: 3.6 * R_E, mass: 0.3 * M_MOON },
        ];
        // Post-impact 4-h day.
        let mut spin = DVec3::new(0.0, 0.0, 1.0)
            * (moment_of_inertia(M_E, R_E) * 2.0 * std::f64::consts::PI / (4.0 * 3600.0));
        let l_orbit0: f64 =
            moonlets.iter().map(|m| m.mass * (G * M_E * m.a).sqrt()).sum();
        let l_total0 = spin.length() + l_orbit0;
        let day0 = spin_period_s(spin, M_E, R_E);

        let year = 3.156e7;
        let mut years = 0.0f64;
        while moonlets.len() > 1 && years < 2.0e6 {
            secular_step(&mut moonlets, &mut spin, M_E, R_E, EARTH_K2_OVER_Q, 50.0 * year);
            years += 50.0;
        }
        assert_eq!(moonlets.len(), 1, "the moonlets merge into ONE Moon (after {years:.0} y)");
        let a_at_merge = moonlets[0].a;
        for _ in 0..40_000 {
            secular_step(&mut moonlets, &mut spin, M_E, R_E, EARTH_K2_OVER_Q, 500.0 * year);
        }
        let m = moonlets[0];
        let l_total1 =
            spin.length() + m.mass * (G * M_E * m.a).sqrt();
        let day1 = spin_period_s(spin, M_E, R_E);
        println!(
            "one Moon: merged at {:.1} R⊕ · after 20 Myr more: a = {:.1} R⊕ · day {:.1} h → {:.1} h · L drift {:.2e}",
            a_at_merge / R_E, m.a / R_E, day0 / 3600.0, day1 / 3600.0,
            (l_total1 - l_total0).abs() / l_total0
        );
        assert!(m.a > a_at_merge * 1.5, "the Moon migrates outward (got {:.1} R⊕)", m.a / R_E);
        assert!(day1 > day0 * 1.2, "the day lengthens as the orbit grows");
        assert!(
            (l_total1 - l_total0).abs() / l_total0 < 1.0e-6,
            "angular momentum conserved through mergers and migration"
        );
    }

    #[test]
    fn a_sub_synchronous_moonlet_disrupts_at_roche_not_on_the_surface() {
        // The reported "giant ball rolling on Earth's surface" bug: a moonlet whose orbit decays inside the
        // Roche limit was CLAMPED at 1.2 R⊕ and drawn as an intact ball overlapping Earth. Honest physics:
        // it is tidally SHREDDED at the Roche limit, its mass + angular momentum raining onto the planet.
        // Start a moonlet just outside Roche with a SLOW (24 h) day so it is sub-synchronous → migrates
        // inward → must disrupt, with mass and total angular momentum conserved.
        let mut moonlets = vec![Moonlet { a: 3.2 * R_E, mass: 0.3 * M_MOON }];
        let mut spin = DVec3::new(0.0, 0.0, 1.0)
            * (moment_of_inertia(M_E, R_E) * 2.0 * std::f64::consts::PI / (24.0 * 3600.0));
        let l0 = spin.length() + moonlets[0].mass * (G * M_E * moonlets[0].a).sqrt();
        let m0 = moonlets[0].mass;
        let year = 3.156e7;
        let mut shed_total = 0.0;
        for _ in 0..200_000 {
            let (_m, shed) = secular_step(&mut moonlets, &mut spin, M_E, R_E, EARTH_K2_OVER_Q, 500.0 * year);
            shed_total += shed;
            if moonlets.is_empty() {
                break;
            }
        }
        let d_roche = 2.44 * R_E * (M_E / (4.0 / 3.0 * std::f64::consts::PI * R_E.powi(3)) / 2_900.0).cbrt();
        println!("Roche limit {:.2} R⊕; sub-synchronous moonlet disrupted, shed {:.3} M☾", d_roche / R_E, shed_total / M_MOON);
        assert!(moonlets.is_empty(), "the sub-synchronous moonlet must DISRUPT, not survive on the surface");
        assert!((shed_total - m0).abs() / m0 < 1.0e-9, "its full mass rains onto the planet (mass conserved)");
        let l1 = spin.length();
        assert!((l1 - l0).abs() / l0 < 1.0e-5, "total angular momentum conserved through disruption ({l0:.3e} → {l1:.3e})");
    }

    #[test]
    fn the_tidal_kick_conserves_angular_momentum_between_orbit_and_spin() {
        let sat = Body {
            pos: DVec3::new(3.0 * R_E, 0.0, 0.0),
            vel: DVec3::new(0.0, (G * M_E / (3.0 * R_E)).sqrt(), 0.0),
            mass: M_MOON,
        };
        let spin_l = DVec3::new(0.0, 0.0, 1.0) * 3.0e34; // fast prograde spin (post-impact scale)
        let (kick, d_spin) = tidal_kick(
            EARTH_K2_OVER_Q, &sat, DVec3::ZERO, DVec3::ZERO, M_E, R_E, spin_l, 100.0,
        );
        let d_l_orbit = sat.pos.cross(kick * sat.mass);
        assert!(
            (d_l_orbit + d_spin).length() < 1.0e-6 * d_l_orbit.length().max(1.0),
            "orbit gains exactly what the spin loses"
        );
        assert!(kick.dot(sat.vel) > 0.0, "prograde fast spin accelerates the orbit (outward)");
    }

    /// **A DECLARED day length survives the emergent-inertia round-trip (docs/58).** A scene declares a
    /// day; the engine sets `L = I·ω` and reads it back with `spin_period_from_inertia`. As long as the
    /// SAME inertia is used at both ends — which the scene's `spin_inertia()` guarantees — the declared
    /// period reproduces, for ANY body's inertia (no uniform-sphere ⅖mr², no "Earth"). This is the
    /// consistency the per-body-spin migration must preserve.
    #[test]
    fn a_declared_day_length_survives_the_emergent_inertia_round_trip() {
        let period = 86_164.0; // a declared sidereal day
        for body in [crate::planet::earth(), crate::planet::moon()] {
            let i = body.moment_of_inertia();
            let l = DVec3::new(0.0, 0.0, 1.0) * (i * 2.0 * std::f64::consts::PI / period);
            let read_back = spin_period_from_inertia(l, i);
            assert!((read_back - period).abs() < 1e-3, "{}: declared day {period} reproduced (got {read_back})", body.name);
        }
        // A non-spinning body has no day (guards the zero-L branch).
        assert!(spin_period_from_inertia(DVec3::ZERO, 1.0e37).is_infinite(), "no spin ⇒ no day");
    }
}
