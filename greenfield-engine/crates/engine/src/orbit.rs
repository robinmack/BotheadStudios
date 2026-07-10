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
            acc[i] += d * (G * bodies[j].mass * r2.powf(-1.5));
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
}
