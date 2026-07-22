//! N-body orbital mechanics — the gravity law at astronomical scale.
//!
//! The voxel self-gravity (`gravity.rs`) proves gravity *emerges from aggregate mass*; this module
//! proves the same Newtonian law reproduces real celestial motion. Point masses attract each other
//! (`a_i = Σ_{j≠i} G·m_j·(p_j − p_i)/|p_j − p_i|³`) and are advanced with **velocity-Verlet**, a
//! symplectic integrator that conserves energy and angular momentum over many orbits.
//!
//! The test drops in the real Earth and Moon (masses, separation, and the Moon's ~1.022 km/s speed)
//! and checks the Moon completes a bound orbit — the "if the Moon orbits the planet, our simulator
//! is good" validation. f64 throughout (astronomical magnitudes need the precision).

// Currently exercised only by the validation test; keep it compiled on all targets, warning-free.
#![allow(dead_code)]

use glam::DVec3;

/// Newton's gravitational constant (m³·kg⁻¹·s⁻²).
pub const G: f64 = 6.674e-11;

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: DVec3,
    pub vel: DVec3,
    pub mass: f64,
}

/// Gravitational acceleration on each body from every other body.
pub fn accelerations(bodies: &[Body]) -> Vec<DVec3> {
    let mut acc = vec![DVec3::ZERO; bodies.len()];
    for i in 0..bodies.len() {
        for j in 0..bodies.len() {
            if i == j {
                continue;
            }
            let d = bodies[j].pos - bodies[i].pos;
            let r2 = d.length_squared();
            acc[i] += d * (G * bodies[j].mass * (1.0 / (r2 * r2.sqrt())));
        }
    }
    acc
}

/// One velocity-Verlet step. `acc` holds the accelerations at the current positions and is updated
/// to the new ones — pass the same buffer each step (start with `accelerations(bodies)`).
pub fn verlet_step(bodies: &mut [Body], acc: &mut Vec<DVec3>, dt: f64) {
    for (b, a) in bodies.iter_mut().zip(acc.iter()) {
        b.vel += *a * (0.5 * dt); // half-kick
        b.pos += b.vel * dt; // drift
    }
    let new_acc = accelerations(bodies);
    for (b, a) in bodies.iter_mut().zip(new_acc.iter()) {
        b.vel += *a * (0.5 * dt); // half-kick
    }
    *acc = new_acc;
}

/// Total mechanical energy (kinetic + gravitational potential). Conserved by the integrator.
pub fn total_energy(bodies: &[Body]) -> f64 {
    let mut ke = 0.0;
    for b in bodies {
        ke += 0.5 * b.mass * b.vel.length_squared();
    }
    let mut pe = 0.0;
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            pe -= G * bodies[i].mass * bodies[j].mass / (bodies[j].pos - bodies[i].pos).length();
        }
    }
    ke + pe
}

/// Total angular momentum about the origin. Conserved by the integrator.
pub fn angular_momentum(bodies: &[Body]) -> DVec3 {
    bodies
        .iter()
        .fold(DVec3::ZERO, |l, b| l + b.mass * b.pos.cross(b.vel))
}

/// Perigee (closest approach) of the relative two-body orbit, in metres — or `None` if the orbit is
/// unbound (it would escape, not come back). `mu = G·(m1 + m2)`. Lets the HUD tell, live, whether a
/// slowed Moon will merely graze, plunge deep, or crash into the planet. Standard orbital-elements
/// relations (specific energy + angular momentum → semi-major axis + eccentricity → perigee).
pub fn perigee(rel_pos: DVec3, rel_vel: DVec3, mu: f64) -> Option<f64> {
    let r = rel_pos.length();
    if r == 0.0 {
        return Some(0.0);
    }
    let energy = 0.5 * rel_vel.length_squared() - mu / r;
    if energy >= 0.0 {
        return None; // unbound (parabolic/hyperbolic) — no perigee it returns to
    }
    let a = -mu / (2.0 * energy);
    let h = rel_pos.cross(rel_vel).length();
    let e = (1.0 + 2.0 * energy * h * h / (mu * mu)).max(0.0).sqrt();
    Some(a * (1.0 - e))
}

/// Perfectly-inelastic **contact resolution** for two solid bodies that have interpenetrated: separate
/// them until their surfaces just touch (`r_sum` apart) and, when they are approaching, **coalesce them
/// to their common (centre-of-mass) velocity** — removing *all* relative motion, not just the inward
/// normal component. So a crashing body **splats and merges** rather than sliding frictionlessly around
/// the surface (which looked like it "orbited at the planet's radius"). Momentum-conserving. Returns
/// `true` if they were in contact. The celestial-scale echo of the voxel contacts (`docs/16`): solid
/// things collide at their surfaces — a point mass tunnelling into a 1/r² singularity would be a fudge
/// (and a numerical explosion), not a collision. (Grazing shear / partial merge is a future refinement.)
pub fn resolve_contact(a: &mut Body, b: &mut Body, r_sum: f64) -> bool {
    let d = b.pos - a.pos;
    let dist = d.length();
    if dist >= r_sum || dist == 0.0 {
        return false;
    }
    let n = d / dist; // contact normal, a → b
    let inv_a = 1.0 / a.mass;
    let inv_b = 1.0 / b.mass;
    let inv_sum = inv_a + inv_b;

    // Separate to just-touching, split by inverse mass (the heavier body barely moves).
    let pen = r_sum - dist;
    a.pos -= n * (pen * inv_a / inv_sum);
    b.pos += n * (pen * inv_b / inv_sum);

    // Perfectly inelastic *stick*: when approaching, bring BOTH bodies to the shared centre-of-mass
    // velocity (all relative motion removed — normal AND tangential). `inv_a/inv_sum == m_b/(m_a+m_b)`,
    // so this is exactly `v → v_com` for each. Momentum is conserved (equal, opposite impulses).
    let rel_v = b.vel - a.vel;
    if rel_v.dot(n) < 0.0 {
        a.vel += rel_v * (inv_a / inv_sum);
        b.vel -= rel_v * (inv_b / inv_sum);
    }
    true
}

/// **Swept (continuous) first-contact** — the honest fix for a FAST body TUNNELLING through the planet
/// in one big step (fast-forward), which the discrete `resolve_contact` (are-surfaces-overlapping-*this*-
/// -sample?) misses entirely: the body jumps from outside to outside and the collision is never seen.
/// Given a body's position RELATIVE to the planet centre before (`rel_old`) and after (`rel_new`) a step,
/// this returns the fraction `t ∈ [0,1]` of the step at which its straight path FIRST enters the contact
/// sphere of radius `r_sum` — i.e. *when* it hits — or `None` if the path never reaches the planet. The
/// simulation FORECASTS the collision on the continuous trajectory; what we simulate must not depend on
/// how coarsely we sample or render it (Robin; docs/13's "render ≠ simulate", docs/24 hard-problem #1).
pub fn swept_first_contact(rel_old: DVec3, rel_new: DVec3, r_sum: f64) -> Option<f64> {
    // Solve |rel_old + t·(rel_new − rel_old)|² = r_sum² for the first t ∈ [0,1].
    let delta = rel_new - rel_old;
    let a = delta.length_squared();
    let c = rel_old.length_squared() - r_sum * r_sum;
    if c <= 0.0 {
        return Some(0.0); // already inside the planet at the start of the step
    }
    if a < 1.0e-6 {
        return None; // not moving and not already inside ⇒ no contact
    }
    let b = 2.0 * rel_old.dot(delta);
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None; // the straight path never reaches the sphere
    }
    let t = (-b - disc.sqrt()) / (2.0 * a); // smaller root = first entry
    if (0.0..=1.0).contains(&t) {
        Some(t)
    } else {
        None // the path's closest approach is outside this step's [0,1] window
    }
}

/// The relative velocity of a body at FIRST CONTACT, recovered from the two-body conservation laws —
/// specific orbital energy (`v² = v₀² + 2μ(1/r_c − 1/r₀)`, vis-viva) and angular momentum
/// (`v_t = |r₀×v₀|/r_c`, direction `L̂×n̂`) — using the PRE-step state. This is dt-INDEPENDENT: in
/// fast-forward, the integrator's post-step velocity near the 1/r² singularity is garbage (the body has
/// been stepped far past the surface), and depositing an impact from it inflates the energy several-fold.
/// The conservation laws know the true state at the surface no matter how coarsely we stepped — the
/// simulation FORECASTS the collision, it doesn't sample it (docs/13). `n_hat` = outward surface normal
/// at the contact point; the returned velocity has its radial part inward (it is arriving).
pub fn contact_velocity(rel_old: DVec3, vel_old: DVec3, n_hat: DVec3, r_contact: f64, mu: f64) -> DVec3 {
    let r0 = rel_old.length().max(1.0e-9);
    // Energy conservation: speed² at the contact radius.
    let v2 = (vel_old.length_squared() + 2.0 * mu * (1.0 / r_contact - 1.0 / r0)).max(0.0);
    // Angular-momentum conservation: the tangential component at contact, in the orbit plane.
    let l = rel_old.cross(vel_old);
    let vt = (l.length() / r_contact).min(v2.sqrt()); // cannot exceed the total speed
    let t_dir = l.cross(n_hat);
    let t_hat = if t_dir.length_squared() > 1.0e-18 {
        t_dir / t_dir.length()
    } else {
        DVec3::ZERO // pure radial plunge — no tangential direction (and vt ≈ 0)
    };
    let vr = (v2 - vt * vt).max(0.0).sqrt();
    t_hat * vt - n_hat * vr
}

/// Kinetic energy (J) a perfectly-inelastic collision between two bodies would dissipate: ½·μ·|Δv|²
/// with reduced mass μ = m_a·m_b/(m_a+m_b). This is the energy that *must* go somewhere real — heat,
/// fracture, melt, ejecta. Our contact resolution currently removes it without modelling where it
/// goes; surfacing this number keeps us honest that a "click to rest" is a placeholder, not the whole
/// truth of an impact.
pub fn inelastic_dissipation(a: &Body, b: &Body) -> f64 {
    let reduced = a.mass * b.mass / (a.mass + b.mass);
    0.5 * reduced * (b.vel - a.vel).length_squared()
}

/// Gravitational **binding energy** (J) of a uniform sphere: (3/5)·G·M²/R — roughly the energy needed
/// to disperse the body. Comparing an impact's energy to this tells us, honestly, whether the impact
/// would shatter the body (impact ≫ binding) rather than merely dent it.
pub fn binding_energy(mass: f64, radius: f64) -> f64 {
    0.6 * G * mass * mass / radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moon_orbits_earth() {
        // Real values: masses (kg), separation (m), the Moon's mean orbital speed (m/s).
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let d = 3.844e8; // 384,400 km
        let v_moon = 1022.0; // ~1.022 km/s

        // Barycentric frame with zero net momentum: Earth recoils oppositely to the Moon.
        let v_earth = v_moon * m_moon / m_earth;
        let mut bodies = vec![
            Body {
                pos: DVec3::new(0.0, 0.0, 0.0),
                vel: DVec3::new(0.0, -v_earth, 0.0),
                mass: m_earth,
            },
            Body {
                pos: DVec3::new(d, 0.0, 0.0),
                vel: DVec3::new(0.0, v_moon, 0.0),
                mass: m_moon,
            },
        ];

        let e0 = total_energy(&bodies);
        let l0 = angular_momentum(&bodies);

        let dt = 60.0; // 1-minute steps
        let steps = (60.0 * 86_400.0 / dt) as usize; // 60 days ≈ 2+ orbits
        let mut acc = accelerations(&bodies);

        let mut min_r = f64::MAX;
        let mut max_r = 0.0f64;
        let mut swept = 0.0f64; // accumulated orbital angle
        let mut prev = {
            let rel = bodies[1].pos - bodies[0].pos;
            rel.y.atan2(rel.x)
        };

        for _ in 0..steps {
            verlet_step(&mut bodies, &mut acc, dt);
            let rel = bodies[1].pos - bodies[0].pos;
            let r = rel.length();
            min_r = min_r.min(r);
            max_r = max_r.max(r);
            // Accumulate swept angle (unwrapped).
            let ang = rel.y.atan2(rel.x);
            let mut da = ang - prev;
            if da > std::f64::consts::PI {
                da -= std::f64::consts::TAU;
            }
            if da < -std::f64::consts::PI {
                da += std::f64::consts::TAU;
            }
            swept += da;
            prev = ang;
        }

        // 1. Bound orbit: the Moon neither escapes nor spirals in — distance stays near d.
        assert!(
            min_r > 0.85 * d && max_r < 1.15 * d,
            "orbit should stay bound near {d:.3e} m (min {min_r:.3e}, max {max_r:.3e})"
        );
        // 2. It actually goes *around* — at least one full revolution.
        assert!(
            swept.abs() > std::f64::consts::TAU,
            "the Moon should complete at least one full orbit (swept {swept:.2} rad)"
        );
        // 3. Symplectic integrator conserves energy and angular momentum.
        assert!(
            (total_energy(&bodies) - e0).abs() / e0.abs() < 0.01,
            "energy conserved to <1%"
        );
        assert!(
            (angular_momentum(&bodies) - l0).length() / l0.length() < 0.01,
            "angular momentum conserved to <1%"
        );
    }

    #[test]
    fn sun_earth_moon_system_is_bound() {
        // The honest three-body system: a real Sun lights and holds the Earth, which in turn holds the
        // Moon. Proves (a) a Sun at the true mass/distance, and (b) the Earth given its *appropriate*
        // heliocentric velocity, produce a Moon that stays bound to the Earth while the Earth orbits
        // the Sun — the beautiful, correct nesting, not a hand-placed tableau.
        let m_sun = 1.989e30; // kg
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let au = 1.496e11; // m (Earth–Sun distance)
        let d = 3.844e8; // m (Earth–Moon distance)
        let v_earth = 29_780.0; // m/s (Earth's mean heliocentric speed = sqrt(G·M_sun/AU))
        let v_moon = 1022.0; // m/s (Moon's speed relative to Earth)

        // Heliocentric frame, Sun at rest at the origin. Earth carries its orbital velocity; the Moon
        // carries the Earth's velocity PLUS its own orbital velocity about the Earth (so it co-moves).
        let mut bodies = vec![
            Body {
                pos: DVec3::ZERO,
                vel: DVec3::ZERO,
                mass: m_sun,
            },
            Body {
                pos: DVec3::new(au, 0.0, 0.0),
                vel: DVec3::new(0.0, v_earth, 0.0),
                mass: m_earth,
            },
            Body {
                pos: DVec3::new(au + d, 0.0, 0.0),
                vel: DVec3::new(0.0, v_earth + v_moon, 0.0),
                mass: m_moon,
            },
        ];

        let e0 = total_energy(&bodies);
        let dt = 600.0; // 10-minute steps resolve the ~27.3-day lunar orbit finely
        let steps = (60.0 * 86_400.0 / dt) as usize; // 60 days

        let mut acc = accelerations(&bodies);
        let (mut min_es, mut max_es) = (f64::MAX, 0.0f64); // Earth–Sun distance range
        let (mut min_me, mut max_me) = (f64::MAX, 0.0f64); // Moon–Earth distance range

        for _ in 0..steps {
            verlet_step(&mut bodies, &mut acc, dt);
            let es = (bodies[1].pos - bodies[0].pos).length();
            let me = (bodies[2].pos - bodies[1].pos).length();
            min_es = min_es.min(es);
            max_es = max_es.max(es);
            min_me = min_me.min(me);
            max_me = max_me.max(me);
        }

        // Earth stays on its ~1 AU heliocentric orbit (near-circular).
        assert!(
            min_es > 0.95 * au && max_es < 1.05 * au,
            "Earth should hold a ~1 AU orbit (min {min_es:.3e}, max {max_es:.3e})"
        );
        // The Moon stays bound to the *moving* Earth — neither flung off nor dragged into the Sun.
        assert!(
            min_me > 0.80 * d && max_me < 1.20 * d,
            "Moon should stay bound to Earth near {d:.3e} m (min {min_me:.3e}, max {max_me:.3e})"
        );
        // The whole system conserves energy (symplectic integrator).
        assert!(
            (total_energy(&bodies) - e0).abs() / e0.abs() < 0.01,
            "3-body energy conserved to <1%"
        );
    }

    #[test]
    fn perigee_tracks_how_hard_the_moon_is_braked() {
        let mu = G * (5.972e24 + 7.342e22); // Earth+Moon
        let r = 3.844e8;
        let vc = (mu / r).sqrt(); // circular speed at this radius

        // A circular orbit's perigee is (essentially) its radius.
        let rp = perigee(DVec3::new(r, 0.0, 0.0), DVec3::new(0.0, vc, 0.0), mu).unwrap();
        assert!((rp - r).abs() / r < 1e-3, "circular perigee ≈ radius");

        // Halving the speed drops perigee deep inside (analytic: r·f²/(2−f²), f=0.5 → 0.1429 r) — still
        // well above Earth's radius, so a single halving does NOT crash the Moon.
        let rp_half = perigee(DVec3::new(r, 0.0, 0.0), DVec3::new(0.0, 0.5 * vc, 0.0), mu).unwrap();
        assert!((rp_half - 0.1429 * r).abs() / (0.1429 * r) < 0.02);
        assert!(rp_half > 6.371e6, "halving alone misses the planet");

        // Cancelling the velocity entirely → radial plunge → perigee 0 → a guaranteed impact.
        let rp_drop = perigee(DVec3::new(r, 0.0, 0.0), DVec3::ZERO, mu).unwrap();
        assert!(
            rp_drop < 1.0,
            "a dropped Moon falls straight through the centre (perigee ≈ 0)"
        );
    }

    #[test]
    fn a_dropped_moon_crashes_into_the_planet_and_stops_at_the_surface() {
        // Cancel the Moon's orbital velocity and let it fall: it must reach the Earth's surface and be
        // caught by contact resolution (not tunnel through the point-mass singularity).
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let r_earth = 6.371e6;
        let r_moon = 1.737e6;
        let r_sum = r_earth + r_moon;
        let d = 3.844e8;

        let mut bodies = vec![
            Body {
                pos: DVec3::ZERO,
                vel: DVec3::ZERO,
                mass: m_earth,
            },
            Body {
                pos: DVec3::new(d, 0.0, 0.0),
                vel: DVec3::ZERO, // dropped from rest → radial plunge
                mass: m_moon,
            },
        ];

        let dt = 30.0;
        let mut acc = accelerations(&bodies);
        let mut impacted = false;
        // ~5 days is plenty for the fall from 384,400 km.
        for _ in 0..(5 * 86_400 / 30) {
            verlet_step(&mut bodies, &mut acc, dt);
            let (left, right) = bodies.split_at_mut(1);
            if resolve_contact(&mut left[0], &mut right[0], r_sum) {
                impacted = true;
                break;
            }
        }

        assert!(impacted, "the dropped Moon should reach the planet");
        let sep = (bodies[1].pos - bodies[0].pos).length();
        assert!(
            sep >= r_sum - 1.0 && sep < r_sum + 5.0e5,
            "it rests at the surface, not inside the planet (sep {sep:.3e}, r_sum {r_sum:.3e})"
        );
    }

    #[test]
    fn swept_ccd_catches_a_dropped_moon_in_browser_fastforward_but_a_braked_one_ricochets() {
        // Reproduces the browser EXACTLY: a huge fast-forward step (the Moon jumps many Earth-radii per
        // substep, so the discrete test would tunnel), driven through the same substep+swept-CCD loop the
        // renderer uses. Two scenarios, one physics: a DROP must register a hit; a single ½× BRAKE must
        // NOT (its perigee is ~55,000 km — a gravitational slingshot / "ricochet", the correct outcome).
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let r_sum = 6.371e6 + 1.737e6;
        let d = 3.844e8;
        let mu = G * (m_earth + m_moon);
        let vc = (mu / d).sqrt(); // circular speed at the Moon's distance

        // Browser step: time_scale 2e6 → sim_dt = 2e6/60 ≈ 33 333 s, over ORBIT_SUBSTEPS = 16 → ~2083 s.
        let dt = 2.0e6 / 60.0 / 16.0;

        let ran = |moon_vel: DVec3| -> bool {
            let mut bodies = vec![
                Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: m_earth },
                Body { pos: DVec3::new(d, 0.0, 0.0), vel: moon_vel, mass: m_moon },
            ];
            let mut acc = accelerations(&bodies);
            for _ in 0..4000 {
                let earth_before = bodies[0].pos;
                let rel_old = bodies[1].pos - earth_before;
                verlet_step(&mut bodies, &mut acc, dt);
                let rel_new = bodies[1].pos - bodies[0].pos;
                if swept_first_contact(rel_old, rel_new, r_sum).is_some() {
                    return true;
                }
            }
            false
        };

        // Dropped (velocity cancelled) → radial plunge → the swept CCD forecasts the hit despite tunnelling.
        assert!(ran(DVec3::ZERO), "a DROPPED moon is caught by the swept CCD in fast-forward");
        // Braked to half circular speed → perigee ~0.143 d ≈ 55,000 km → never reaches the surface.
        assert!(
            !ran(DVec3::new(0.0, 0.5 * vc, 0.0)),
            "a single ½× BRAKE ricochets (perigee above the surface) — correctly NO impact"
        );
    }

    #[test]
    fn dropped_moon_in_the_full_three_body_system_registers_an_impact() {
        // Faithful replay of OrbitDemo::render for a DROPPED moon in the real Sun–Earth–Moon system
        // (this is what the browser runs). Robin reports "no visible impact" — so reproduce it natively.
        let (sun_m, earth_m, moon_m) = (1.989e30, 5.972e24, 7.342e22);
        let (au, moon_dist) = (1.496e11, 3.844e8);
        let (earth_helio, moon_speed) = (29_780.0, 1022.0);
        let contact = 6.371e6 + 1.737e6;
        let substeps = 16u32;

        for &time_scale in &[118_000.0_f64, 2_000_000.0] {
            let mut bodies = vec![
                Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: sun_m },
                Body { pos: DVec3::new(au, 0.0, 0.0), vel: DVec3::new(0.0, earth_helio, 0.0), mass: earth_m },
                Body {
                    pos: DVec3::new(au + moon_dist, 0.0, 0.0),
                    vel: DVec3::new(0.0, earth_helio + moon_speed, 0.0),
                    mass: moon_m,
                },
            ];
            bodies[2].vel = bodies[1].vel; // drop_moon: cancel velocity RELATIVE to Earth

            let mut acc = accelerations(&bodies);
            let mut impacted = false;
            let mut min_sep = f64::MAX;
            let sim_dt = time_scale / 60.0;
            let dt = sim_dt / substeps as f64;
            let frames = (12.0 * 86_400.0 / sim_dt) as usize; // ~12 days of fall
            'outer: for _ in 0..frames.max(1) {
                for _ in 0..substeps {
                    let earth_before = bodies[1].pos;
                    let rel_old = bodies[2].pos - earth_before;
                    verlet_step(&mut bodies, &mut acc, dt);
                    let rel_new = bodies[2].pos - bodies[1].pos;
                    min_sep = min_sep.min(rel_new.length());
                    if swept_first_contact(rel_old, rel_new, contact).is_some() {
                        impacted = true;
                        break 'outer;
                    }
                    let (h, t) = bodies.split_at_mut(2);
                    resolve_contact(&mut h[1], &mut t[0], contact);
                }
            }
            println!(
                "time_scale {time_scale}: impacted={impacted}, min_sep={:.4e} m = {:.3}× contact (frames={frames}, dt={dt:.0}s)",
                min_sep,
                min_sep / contact
            );
            assert!(impacted, "dropped moon must impact at time_scale {time_scale}; closest was {:.3}× contact", min_sep / contact);
        }
    }

    #[test]
    fn contact_velocity_recovers_the_true_impact_speed_regardless_of_step_size() {
        // A dropped Moon's true contact speed follows from energy conservation alone:
        // v² = 2μ(1/r_c − 1/d). In browser fast-forward (dt ≈ 2083 s) the integrator steps the point
        // mass far past the surface, so its post-step velocity is WRONG — depositing an impact from it
        // inflates the energy (the "large percentage of debris escapes" bug). `contact_velocity` must
        // recover the true speed from the conservation laws, no matter the step size.
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let contact = 6.371e6 + 1.737e6;
        let d = 3.844e8;
        let mu = G * (m_earth + m_moon);
        let v_true = (2.0 * mu * (1.0 / contact - 1.0 / d)).sqrt();

        let mut bodies = vec![
            Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: m_earth },
            Body { pos: DVec3::new(d, 0.0, 0.0), vel: DVec3::ZERO, mass: m_moon },
        ];
        let mut acc = accelerations(&bodies);
        let dt = 2.0e6 / 60.0 / 16.0; // the browser's max fast-forward substep
        for _ in 0..4000 {
            let rel_old = bodies[1].pos - bodies[0].pos;
            let vel_old = bodies[1].vel - bodies[0].vel;
            verlet_step(&mut bodies, &mut acc, dt);
            let rel_new = bodies[1].pos - bodies[0].pos;
            if let Some(t) = swept_first_contact(rel_old, rel_new, contact) {
                let n_hat = (rel_old + (rel_new - rel_old) * t).normalize();
                let recovered = contact_velocity(rel_old, vel_old, n_hat, contact, mu);
                let sampled = (bodies[1].vel - bodies[0].vel).length(); // the garbage post-step speed
                println!(
                    "true {v_true:.0} m/s · recovered {:.0} m/s · post-step sample {sampled:.0} m/s",
                    recovered.length()
                );
                // 2% tolerance: the residual is the coarse integrator's drift in the PRE-step state
                // (verlet at dt≈2083 s accumulates ~1% energy error over the fall), not the recovery —
                // vs the ~120% error of the post-step sample this replaces.
                assert!(
                    (recovered.length() - v_true).abs() / v_true < 0.02,
                    "conservation-law recovery matches the analytic contact speed \
                     (got {:.0} vs {v_true:.0} m/s)",
                    recovered.length()
                );
                assert!(
                    recovered.dot(n_hat) < 0.0,
                    "the recovered velocity points INTO the surface (it is arriving)"
                );
                return;
            }
        }
        panic!("the dropped moon never contacted");
    }

    #[test]
    fn a_crashing_body_sticks_it_does_not_slide_around_the_surface() {
        // A body arriving with BOTH inward and sideways (tangential) velocity must end up moving *with*
        // the planet — all relative motion gone — not skating frictionlessly around the surface (the
        // "the moon just orbits super fast at the planet's radius" bug).
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let r_sum = 6.371e6 + 1.737e6;
        let mut earth = Body {
            pos: DVec3::ZERO,
            vel: DVec3::ZERO,
            mass: m_earth,
        };
        let mut moon = Body {
            pos: DVec3::new(r_sum * 0.99, 0.0, 0.0), // just overlapping
            vel: DVec3::new(-8000.0, 5000.0, 0.0),   // inward (−x) AND sideways (+y)
            mass: m_moon,
        };
        let p_before = earth.mass * earth.vel + moon.mass * moon.vel;

        assert!(resolve_contact(&mut earth, &mut moon, r_sum));

        let rel = (moon.vel - earth.vel).length();
        assert!(
            rel < 1.0,
            "the crashing body sticks (relative speed {rel:.3} m/s ≈ 0), it doesn't orbit the surface"
        );
        let p_after = earth.mass * earth.vel + moon.mass * moon.vel;
        assert!(
            (p_after - p_before).length() / p_before.length() < 1e-9,
            "momentum conserved through the perfectly-inelastic stick"
        );
    }

    #[test]
    fn impact_energy_would_shatter_both_bodies() {
        // A Moon arriving at ~11 km/s releases far more energy than holds it together — so a real
        // impact is catastrophic disruption, not a gentle stop. We measure it even though the
        // fragmentation itself is not simulated yet (honesty: report the damage, don't hide it).
        let m_earth = 5.972e24;
        let m_moon = 7.342e22;
        let r_earth = 6.371e6;
        let r_moon = 1.737e6;
        let earth = Body {
            pos: DVec3::ZERO,
            vel: DVec3::ZERO,
            mass: m_earth,
        };
        let moon = Body {
            pos: DVec3::new(r_earth + r_moon, 0.0, 0.0),
            vel: DVec3::new(-11_090.0, 0.0, 0.0), // ~free-fall speed from lunar distance
            mass: m_moon,
        };

        let ke = inelastic_dissipation(&earth, &moon);
        let bind = binding_energy(m_moon, r_moon);
        assert!(
            ke > 1.0e30 && ke < 1.0e31,
            "impact energy ~4.5e30 J (got {ke:.3e})"
        );
        assert!(
            ke > 10.0 * bind,
            "impact energy dwarfs the Moon's binding energy (ke {ke:.3e}, bind {bind:.3e})"
        );
    }

    #[test]
    fn swept_contact_catches_a_body_that_tunnels_through_the_planet() {
        // The fast-forward bug: a body leaps from one side of the planet to the other in ONE step. The
        // discrete overlap test sees it outside at both samples and MISSES the collision; the swept test
        // finds the crossing on the continuous path — forecast, not sample.
        let r_sum = 1.0;
        // Straight through the centre, −5 → +5: both endpoints are outside (dist 5), yet it clearly hits.
        let t = swept_first_contact(DVec3::new(-5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0), r_sum)
            .expect("a body tunnelling through the planet must be caught");
        assert!((t - 0.4).abs() < 1.0e-6, "first contact at the near surface (x=−1 ⇒ t=0.4), got {t}");
        // A genuine miss: passes to the side (y = 3, never within r_sum = 1) ⇒ None.
        assert!(
            swept_first_contact(DVec3::new(-5.0, 3.0, 0.0), DVec3::new(5.0, 3.0, 0.0), r_sum).is_none(),
            "a path that clears the planet is not a collision"
        );
        // Already inside at the start ⇒ t = 0.
        assert_eq!(
            swept_first_contact(DVec3::new(0.5, 0.0, 0.0), DVec3::new(0.6, 0.0, 0.0), r_sum),
            Some(0.0)
        );
    }
}

/// **Where the Sun actually is, for an Earth-fixed frame.**
///
/// This replaces a hardcoded `DVec3::new(1.0, 0.45, 0.6)` whose own comment admitted it was chosen for
/// "a pleasant ¾ lighting" — a direction picked because it looked good, which is exactly the thing the
/// Laws forbid, and which put the terminator wherever it happened to fall rather than where the Sun puts
/// it. With the globe already oriented to real time, a decorative sun made noon land in the wrong ocean.
///
/// The returned unit vector points TO the Sun in the same frame `FlyCamera::frame` uses: +Y is the spin
/// axis (north), longitude 0 is +X, longitude 90°E is +Z. In an Earth-FIXED frame the Sun's declination
/// swings ±ε over the year and its longitude sweeps a full turn each day — so the axial tilt enters here,
/// as declination, rather than by tilting the mesh.
///
/// Low-precision solar position from the Astronomical Almanac (~0.01° over 1950–2050): good to a few
/// kilometres of subsolar point, far below anything a viewer can see. FLAGGED: it ignores nutation and
/// aberration, and uses mean rather than apparent sidereal time.
pub fn solar_direction_earth_fixed(unix_seconds: f64) -> glam::DVec3 {
    // Days since the J2000.0 epoch (2000-01-01 12:00 UTC = Unix 946_728_000).
    let n = (unix_seconds - 946_728_000.0) / 86_400.0;
    let mean_longitude = (280.460 + 0.985_647_4 * n).to_radians();
    let mean_anomaly = (357.528 + 0.985_600_3 * n).to_radians();
    // Ecliptic longitude: the mean longitude plus the equation of centre (Earth's orbit is an ellipse,
    // so the Sun runs ahead of / behind the mean by up to ~2°).
    let ecliptic = mean_longitude + (1.915_f64.to_radians()) * mean_anomaly.sin()
        + (0.020_f64.to_radians()) * (2.0 * mean_anomaly).sin();
    let obliquity = (23.439 - 4.0e-7 * n).to_radians(); // Earth's axial tilt — the reason there are seasons
    let declination = (obliquity.sin() * ecliptic.sin()).asin();
    let right_ascension = (obliquity.cos() * ecliptic.sin()).atan2(ecliptic.cos());
    // Greenwich mean sidereal time — how far Earth has turned under the stars. The subsolar longitude is
    // the Sun's right ascension measured from the Greenwich meridian.
    let gmst_hours = 18.697_374_558 + 24.065_709_824_419_08 * n;
    let gmst = (gmst_hours * std::f64::consts::PI / 12.0).rem_euclid(std::f64::consts::TAU);
    let subsolar_longitude = right_ascension - gmst;
    let (sin_dec, cos_dec) = declination.sin_cos();
    let (sin_lon, cos_lon) = subsolar_longitude.sin_cos();
    glam::DVec3::new(cos_dec * cos_lon, sin_dec, cos_dec * sin_lon)
}

/// Wall-clock seconds since the Unix epoch, on either target.
pub fn unix_now_seconds() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() / 1000.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod solar_position_tests {
    use super::solar_direction_earth_fixed;

    /// The Sun's declination IS the seasons. Checked against the real solstices and equinoxes, which
    /// anyone can look up — if this drifts, the terminator is lying about the time of year.
    #[test]
    fn declination_tracks_the_real_seasons() {
        // The REAL instants of the 2024 equinoxes and solstices (UTC), not the nearest round noon — the
        // first cut of this test used 2024-09-19 for the September equinox and failed by 1.18°, which is
        // precisely three days of the Sun's own 0.39°/day march. The code was right and the fixture was
        // wrong; these are the published times.
        let cases = [
            (1_710_903_960.0, 0.0, 0.2),    // 2024-03-20 03:06 equinox: Sun over the equator
            (1_718_916_660.0, 23.44, 0.2),  // 2024-06-20 20:51 solstice: over the Tropic of Cancer
            (1_727_009_040.0, 0.0, 0.2),    // 2024-09-22 12:44 equinox
            (1_734_772_800.0, -23.44, 0.2), // 2024-12-21 09:20 solstice: over the Tropic of Capricorn
        ];
        for (t, want, tol) in cases {
            let d = solar_direction_earth_fixed(t);
            let dec = d.y.asin().to_degrees();
            assert!((dec - want).abs() < tol, "declination at {t}: got {dec:.2}°, want {want}±{tol}");
            assert!((d.length() - 1.0).abs() < 1e-12, "must be a unit vector");
        }
    }

    /// Local noon must be under the Sun. At 12:00 UTC the subsolar point sits near the Greenwich
    /// meridian, and it marches west at 15°/hour — this is what puts the terminator in the right ocean.
    #[test]
    fn the_subsolar_point_marches_west_at_fifteen_degrees_an_hour() {
        let noon = 1_718_884_800.0; // 2024-06-20 12:00 UTC
        let lon_at = |t: f64| {
            let d = solar_direction_earth_fixed(t);
            d.z.atan2(d.x).to_degrees()
        };
        let at_noon = lon_at(noon);
        assert!(at_noon.abs() < 4.0, "subsolar longitude at 12:00 UTC ≈ Greenwich, got {at_noon:.2}°");
        // Six hours later it must be ~90° west (allowing the equation of time).
        let later = lon_at(noon + 6.0 * 3600.0);
        let moved = (at_noon - later).rem_euclid(360.0);
        assert!((moved - 90.0).abs() < 2.0, "6 h ⇒ ~90° west, got {moved:.2}°");
    }
}
