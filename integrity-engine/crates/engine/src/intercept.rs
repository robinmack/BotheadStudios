//! **The launch-window intercept: release time chosen so the site rotates under the impact.**
//!
//! A from-rest fall from lunar distance takes days of sim time while the planet turns under it,
//! so ground zero is a moving target: pressing Drop at a whim almost never lands the contact on
//! the declared site. That is correct physics and a bad control. The honest fix is the one any
//! real mission uses: never move the ball, never bend the trajectory - CHOOSE THE RELEASE TIME.
//!
//! Everything the solve needs is deterministic data the engine already holds: the N-body state,
//! the planet's declared spin (its day length and current spin angle), and the declared site
//! (lat/lon). The impact point of a fall released at time `t` is COMPUTED BY INTEGRATING THE
//! SAME LAW THE SCENE RUNS - `orbit::verlet_step` plus the swept first-contact forecast - not by
//! an analytic stand-in (the Sun's tide and the planet's own recoil bend a "radial" fall by real
//! degrees over a multi-day plunge; an unpinned formula would quietly disagree with the scene).
//!
//! What release timing can and cannot fix, stated: timing moves the site AROUND the spin axis,
//! so the solve matches the impact's AZIMUTH about that axis (the longitude of the moment). The
//! polar offset between the fall's impact ring and the site's rotation ring is set by the orbit
//! geometry alone and is reported ([`DropWindow::plane_offset_rad`]), never bent away.

use crate::orbit::{self, Body};
use glam::DVec3;

/// The planet's declared rotation, as the engine holds it: the axis and rate implied by its spin
/// angular momentum, plus the accumulated spin angle "now" (the same angle the render and the
/// site placement use, so the solver and the scene cannot disagree about where the site is).
#[derive(Clone, Copy, Debug)]
pub struct Spin {
    /// Unit spin axis.
    pub axis: DVec3,
    /// Rotation rate (rad/s): 2π over the sidereal day.
    pub omega_rad_s: f64,
    /// The accumulated spin angle at t = 0 (rad).
    pub angle_rad: f64,
}

/// A solved release window, all in SIM seconds measured from the state handed to the solver.
#[derive(Clone, Copy, Debug)]
pub struct DropWindow {
    /// Time until the release fires. Always ≥ 0: a site on the far side yields a LATER window,
    /// never a bent trajectory.
    pub release_in_s: f64,
    /// Time until the forecast first contact (release + fall).
    pub impact_in_s: f64,
    /// Duration of the from-rest fall itself.
    pub fall_s: f64,
    /// The solver's own residual at the solution: |impact azimuth − site azimuth| (rad).
    pub residual_rad: f64,
    /// The polar-angle offset between the impact point and the site's rotation ring (rad) -
    /// the part of the miss release timing cannot change, reported rather than hidden.
    pub plane_offset_rad: f64,
}

/// Azimuth of `d` about `axis` (rad), measured in a fixed basis perpendicular to the axis. The
/// basis is deterministic, and only azimuth DIFFERENCES matter to the solve, so its zero point
/// cancels. A right-handed rotation of `d` about `axis` by θ adds exactly θ to this azimuth.
pub fn azimuth_about(axis: DVec3, d: DVec3) -> f64 {
    let a = axis.normalize_or_zero();
    let seed = if a.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let e1 = (seed - a * seed.dot(a)).normalize();
    let e2 = a.cross(e1);
    d.dot(e2).atan2(d.dot(e1))
}

/// Wrap an angle to (−π, π].
pub fn wrap_pi(a: f64) -> f64 {
    let t = a.rem_euclid(std::f64::consts::TAU);
    if t > std::f64::consts::PI { t - std::f64::consts::TAU } else { t }
}

/// Integrate a from-rest fall released `release_in_s` from now, under THE scene's law: coast the
/// whole N-body system forward with `verlet_step`, cancel the dropped body's velocity relative
/// to the planet at the release, then integrate the fall until `swept_first_contact` forecasts
/// the hit at `r_contact`. Returns the contact direction (planet-relative, unit) and the total
/// sim seconds from now to contact - or `None` if nothing contacts within `horizon_s`.
pub fn from_rest_fall_contact(
    bodies: &[Body],
    planet: usize,
    drop: usize,
    r_contact: f64,
    dt: f64,
    release_in_s: f64,
    horizon_s: f64,
) -> Option<(DVec3, f64)> {
    let mut b = bodies.to_vec();
    let mut acc = orbit::accelerations(&b);
    // Coast: the body stays on its orbit until the window. Whole steps of dt, then one partial
    // step so the release lands at the asked-for time, not the nearest grid point.
    let mut t = 0.0;
    let mut remaining = release_in_s.max(0.0);
    while remaining > 0.0 {
        let step = remaining.min(dt);
        orbit::verlet_step(&mut b, &mut acc, step);
        t += step;
        remaining -= step;
    }
    // The release: from rest relative to the planet - the drop control's own definition.
    b[drop].vel = b[planet].vel;
    // The fall, with the swept forecast so a coarse step cannot tunnel past the surface.
    while t < release_in_s + horizon_s {
        let rel_old = b[drop].pos - b[planet].pos;
        orbit::verlet_step(&mut b, &mut acc, dt);
        let rel_new = b[drop].pos - b[planet].pos;
        if let Some(f) = orbit::swept_first_contact(rel_old, rel_new, r_contact) {
            let contact = rel_old + (rel_new - rel_old) * f;
            return Some((contact.normalize(), t + f * dt));
        }
        t += dt;
    }
    None
}

/// Solve for the NEXT release time at which the declared site rotates under the fall's impact
/// point. Returns the window and time-to-window, or `None` when no fall from this state reaches
/// the planet (nothing to time) or the planet does not spin (no window will ever come - the
/// caller should drop at once).
pub fn solve_drop_window(
    bodies: &[Body],
    planet: usize,
    drop: usize,
    r_contact: f64,
    spin: &Spin,
    site_lat_deg: f64,
    site_lon_deg: f64,
    dt: f64,
) -> Option<DropWindow> {
    if spin.omega_rad_s <= 0.0 {
        return None;
    }
    let period = std::f64::consts::TAU / spin.omega_rad_s;
    // A fall from rest takes days; thirty gives the forecast room without letting a non-falling
    // configuration spin forever.
    let horizon = 30.0 * 86_400.0;
    // The site's body-fixed direction, exactly as the engine places it (geo convention), and its
    // azimuth about the spin axis. At sim time T the site's world azimuth is this plus the spin
    // angle then: az₀ + angle_now + ω·T.
    let site_dir = crate::geo::dir_from_lat_lon(site_lat_deg, site_lon_deg);
    let site_az0 = azimuth_about(spin.axis, site_dir) + spin.angle_rad;
    // err(t): how far the site trails the impact point if we release at t. It decreases at
    // ~ −ω per second of delay (the site sweeps a full turn per day while the impact azimuth
    // drifts at the Moon's slow orbital rate), so t ← t + err/ω is a contraction.
    let mut eval = |t: f64| -> Option<(f64, DVec3, f64)> {
        let (dir, t_impact) = from_rest_fall_contact(bodies, planet, drop, r_contact, dt, t, horizon)?;
        let err = wrap_pi(
            azimuth_about(spin.axis, dir) - (site_az0 + spin.omega_rad_s * t_impact),
        );
        Some((err, dir, t_impact))
    };
    let (err0, mut dir, mut t_impact) = eval(0.0)?;
    // First guess: the wait for the site to close the gap, taken the positive way around.
    let mut t = err0.rem_euclid(std::f64::consts::TAU) / spin.omega_rad_s;
    let mut err = err0;
    for _ in 0..24 {
        let (e, d, ti) = eval(t)?;
        err = e;
        dir = d;
        t_impact = ti;
        if err.abs() < 1.0e-5 {
            break;
        }
        t += err / spin.omega_rad_s;
        if t < 0.0 {
            t += period; // never a negative window: the next one, not a bent path
        }
    }
    if err.abs() > 1.0e-3 {
        return None; // did not converge - refuse rather than hand back a wrong window
    }
    let cos_polar_imp = dir.dot(spin.axis).clamp(-1.0, 1.0);
    let cos_polar_site = site_dir.normalize().dot(spin.axis).clamp(-1.0, 1.0);
    Some(DropWindow {
        release_in_s: t,
        impact_in_s: t_impact,
        fall_s: t_impact - t,
        residual_rad: err.abs(),
        plane_offset_rad: (cos_polar_imp.acos() - cos_polar_site.acos()).abs(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::Body;
    use glam::DVec3;

    /// The Ground Zero world's own cast: Sun at rest, Earth on its heliocentric orbit with the
    /// declared sidereal day, Luna co-orbiting - the exact state the scene integrates.
    fn ground_zero_bodies() -> (Vec<Body>, usize, usize, f64) {
        let bodies = vec![
            Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: 1.989e30 },
            Body {
                pos: DVec3::new(1.496e11, 0.0, 0.0),
                vel: DVec3::new(0.0, 29_780.0, 0.0),
                mass: 5.972e24,
            },
            Body {
                pos: DVec3::new(1.496e11 + 3.844e8, 0.0, 0.0),
                vel: DVec3::new(0.0, 29_780.0 + 1022.0, 0.0),
                mass: 7.342e22,
            },
        ];
        let r_contact = 6.371e6 + 1.737e6;
        (bodies, 1, 2, r_contact)
    }

    fn earth_spin(angle_rad: f64) -> Spin {
        Spin {
            axis: DVec3::Z, // the scene's spin_l axis: ⊥ the orbital (x-y) plane
            omega_rad_s: std::f64::consts::TAU / 86_164.0,
            angle_rad,
        }
    }

    /// The scene's own substep at the Ground Zero world's declared 118,000× fast-forward.
    const SCENE_DT: f64 = 118_000.0 / 960.0;

    /// Replay of the WIRED release: count the armed window down in scene substeps, fire at the
    /// nearest substep boundary (the same ±dt/2 quantization `step_substep` applies), integrate
    /// the fall with the scene's own law, and measure where the contact lands relative to the
    /// site. Returns the azimuth miss (rad) about the spin axis.
    fn simulate_release_and_measure_miss(
        bodies: &[Body],
        planet: usize,
        drop: usize,
        r_contact: f64,
        spin: &Spin,
        site_lat: f64,
        site_lon: f64,
        release_in_s: f64,
    ) -> f64 {
        let mut b = bodies.to_vec();
        let mut acc = crate::orbit::accelerations(&b);
        let dt = SCENE_DT;
        let mut t = 0.0;
        let mut remaining = release_in_s;
        // The wired countdown: whole substeps until the window is closer than half a step.
        while remaining > 0.5 * dt {
            crate::orbit::verlet_step(&mut b, &mut acc, dt);
            t += dt;
            remaining -= dt;
        }
        b[drop].vel = b[planet].vel; // the release fires
        loop {
            let rel_old = b[drop].pos - b[planet].pos;
            crate::orbit::verlet_step(&mut b, &mut acc, dt);
            let rel_new = b[drop].pos - b[planet].pos;
            if let Some(f) = crate::orbit::swept_first_contact(rel_old, rel_new, r_contact) {
                let contact_t = t + f * dt;
                let contact_dir = (rel_old + (rel_new - rel_old) * f).normalize();
                let site_az = azimuth_about(spin.axis, crate::geo::dir_from_lat_lon(site_lat, site_lon))
                    + spin.angle_rad
                    + spin.omega_rad_s * contact_t;
                return wrap_pi(azimuth_about(spin.axis, contact_dir) - site_az);
            }
            t += dt;
            assert!(t < 40.0 * 86_400.0, "the released fall never contacted");
        }
    }

    /// **Solving then simulating lands the contact on the site.** The solver picks the window
    /// from the deterministic state; the scene-law replay (same integrator, same substep, same
    /// ±dt/2 release quantization the wiring applies) must then land the contact within a stated
    /// angular tolerance of the site - 1° of azimuth about the spin axis, which at Earth's
    /// equator is ~111 km on a target reached after a multi-day fall.
    #[test]
    fn solved_release_lands_the_contact_on_the_site() {
        let (bodies, planet, drop, r_contact) = ground_zero_bodies();
        let spin = earth_spin(1.234); // an arbitrary, nonzero phase: the solve must use it
        // The Ground Zero world's OWN declared site, read from the shipped file so the solver
        // test and the world cannot drift apart.
        let shipped = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/public/worlds/ground-zero/world.json"
        ))
        .expect("shipped ground-zero world");
        let def = crate::terra::world_def::World::parse(&shipped).expect("parses");
        let g = def.ground.as_ref().expect("declares its site");
        let (site_lat, site_lon) = (g.lat, g.lon);

        let w = solve_drop_window(
            &bodies, planet, drop, r_contact, &spin, site_lat, site_lon, 60.0,
        )
        .expect("a dropped moon always reaches the planet: the window must exist");

        // The next window, and honest bookkeeping around it.
        assert!(w.release_in_s >= 0.0, "time-to-window is never negative");
        assert!(
            w.release_in_s < 86_164.0,
            "the site comes around once per day: the NEXT window is inside one ({:.0} s)",
            w.release_in_s
        );
        assert!(
            w.fall_s > 3.0 * 86_400.0 && w.fall_s < 7.0 * 86_400.0,
            "a from-rest fall from lunar distance takes days ({:.2} d)",
            w.fall_s / 86_400.0
        );
        assert!((w.impact_in_s - w.release_in_s - w.fall_s).abs() < 1.0e-6);

        let miss = simulate_release_and_measure_miss(
            &bodies, planet, drop, r_contact, &spin, site_lat, site_lon, w.release_in_s,
        );
        println!(
            "window in {:.0} s, fall {:.3} d, solver residual {:.4}°, simulated miss {:.4}° \
             (plane offset {:.1}°, set by geometry, not timing)",
            w.release_in_s,
            w.fall_s / 86_400.0,
            w.residual_rad.to_degrees(),
            miss.to_degrees(),
            w.plane_offset_rad.to_degrees(),
        );
        assert!(
            miss.abs().to_degrees() < 1.0,
            "the simulated contact lands within 1° of the site's azimuth (got {:.3}°)",
            miss.to_degrees()
        );
        // The declared site sits ON the fall's impact ring (the world file's derivation): the
        // whole cast is in the orbital plane, so the reachable ring is the plane's great circle,
        // and the site was placed on it - the polar offset timing can never fix reads ~0 here.
        // A site off the ring (the old 45N 100W) measured 44 degrees of offset and could never
        // be hit; azimuth AND plane must both close for the ball to stand at ground zero.
        assert!(
            w.plane_offset_rad.to_degrees() < 1.0,
            "the declared site is on the impact ring (plane offset {:.2}°)",
            w.plane_offset_rad.to_degrees()
        );
    }

    /// **A site on the far side yields a LATER window, never a bent trajectory.** "Far side" to
    /// a rotation means half a turn AROUND THE SPIN AXIS - the point the planet's own spin
    /// brings under the impact half a sidereal day later. The trajectory is the same physics
    /// either way: the two solves must find essentially the SAME inertial impact point and fall
    /// time, about half a spin period apart in release.
    #[test]
    fn a_far_side_site_waits_half_a_day_with_the_same_trajectory() {
        let (bodies, planet, drop, r_contact) = ground_zero_bodies();
        let omega = std::f64::consts::TAU / 86_164.0;

        // Phase the spin so the NEAR site's window is early in the day (deterministically):
        // find where an immediate release lands, then set the spin angle so the site trails
        // that impact azimuth by 15° at contact time - its window is then ~1 h out, and the
        // far-side twin must wait about half a day on top.
        let (dir0, t0) =
            from_rest_fall_contact(&bodies, planet, drop, r_contact, 60.0, 0.0, 30.0 * 86_400.0)
                .expect("an immediate drop contacts");
        let (near_lat, near_lon) = (45.0, 0.0); // the declared Ground Zero site
        let d_near = crate::geo::dir_from_lat_lon(near_lat, near_lon);
        let delta = 15.0_f64.to_radians();
        let spin = earth_spin(wrap_pi(
            azimuth_about(DVec3::Z, dir0) - azimuth_about(DVec3::Z, d_near) - omega * t0 - delta,
        ));
        // The far-side twin: the same site carried half a turn about the spin axis.
        let d_far = glam::DQuat::from_axis_angle(DVec3::Z, std::f64::consts::PI) * d_near;
        let (far_lat, far_lon) = crate::geo::lat_lon_from_dir(d_far);

        let near = solve_drop_window(
            &bodies, planet, drop, r_contact, &spin, near_lat, near_lon, 60.0,
        )
        .expect("near-side window");
        let far = solve_drop_window(
            &bodies, planet, drop, r_contact, &spin, far_lat, far_lon, 60.0,
        )
        .expect("far-side window");

        println!(
            "near site ({near_lat:.0}°, {near_lon:.0}°): window {:.0} s · far side \
             ({far_lat:.1}°, {far_lon:.1}°): window {:.0} s · fall {:.3} vs {:.3} d",
            near.release_in_s, far.release_in_s, near.fall_s / 86_400.0, far.fall_s / 86_400.0
        );
        // The far side WAITS - roughly half a sidereal day longer, and strictly later.
        assert!(
            far.release_in_s > near.release_in_s,
            "the far-side site's window comes later ({:.0} vs {:.0} s)",
            far.release_in_s,
            near.release_in_s
        );
        let half = 86_164.0 / 2.0;
        assert!(
            (far.release_in_s - near.release_in_s - half).abs() < 0.1 * half,
            "the wait is about half a day ({:.0} s vs {half:.0})",
            far.release_in_s - near.release_in_s
        );
        // NEVER a bent trajectory: the same from-rest fall both times. Fall durations agree to
        // minutes over days, and the inertial impact azimuth moved only by the Moon's own slow
        // orbital drift over the half-day wait (≪ the 180° a bent path would need).
        assert!(
            (far.fall_s - near.fall_s).abs() < 600.0,
            "fall time is the trajectory's own ({:.0} vs {:.0} s)",
            far.fall_s,
            near.fall_s
        );
        let drift = {
            let (d_near, _) = from_rest_fall_contact(
                &bodies, planet, drop, r_contact, 60.0, near.release_in_s, 30.0 * 86_400.0,
            )
            .unwrap();
            let (d_far, _) = from_rest_fall_contact(
                &bodies, planet, drop, r_contact, 60.0, far.release_in_s, 30.0 * 86_400.0,
            )
            .unwrap();
            wrap_pi(azimuth_about(spin.axis, d_far) - azimuth_about(spin.axis, d_near)).abs()
        };
        println!("inertial impact-azimuth drift over the wait: {:.2}°", drift.to_degrees());
        assert!(
            drift.to_degrees() < 15.0,
            "the impact point barely moves in the inertial frame ({:.1}°) - the trajectory was \
             not bent toward the far side",
            drift.to_degrees()
        );
    }

    /// The azimuth convention the whole solve leans on: a right-handed rotation about the axis
    /// adds exactly its angle to the azimuth, for any axis.
    #[test]
    fn rotation_about_the_axis_adds_to_azimuth() {
        for axis in [DVec3::Z, DVec3::Y, DVec3::new(0.3, -0.5, 0.81).normalize()] {
            let d = DVec3::new(0.7, 0.2, -0.4).normalize();
            for theta in [0.3, 1.7, -2.4] {
                let rotated = glam::DQuat::from_axis_angle(axis, theta) * d;
                let got = wrap_pi(azimuth_about(axis, rotated) - azimuth_about(axis, d));
                assert!(
                    (got - wrap_pi(theta)).abs() < 1.0e-9,
                    "axis {axis:?}, θ {theta}: az delta {got}"
                );
            }
        }
    }
}
