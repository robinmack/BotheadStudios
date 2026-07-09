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
}
