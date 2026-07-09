//! A single rigid sphere integrated under a gravitational field, with simple ground contact.
//!
//! Phase 2 has exactly one dynamic body (the dropped probe), so we integrate its motion directly
//! (`F = ma`, semi-implicit Euler) rather than pulling in a full rigid-body engine. Rapier earns
//! its place later, once we need many bodies, arbitrary shapes, and rich contacts (see `docs`).
//!
//! Note the equivalence principle: gravitational acceleration is independent of the body's mass, so
//! the probe falls at the field's `g` whatever its mass — the "5 kg" is a label, not a lever on the
//! fall. Mass matters for momentum/collision, not for how fast it drops.

use glam::Vec3;

/// Speed below which a grounded body is snapped to rest (avoids endless micro-bouncing).
const REST_SPEED: f32 = 1.0e-4;

pub struct Sphere {
    pub pos: Vec3,
    pub vel: Vec3,
    /// kg. Not read yet (free-fall is mass-independent); kept for momentum/collision in later phases.
    #[allow(dead_code)]
    pub mass: f32,
    pub radius: f32,
    pub restitution: f32,
    /// Fraction of tangential speed removed per ground contact (0 = frictionless, 1 = instant stop).
    pub friction: f32,
    pub resting: bool,
}

impl Sphere {
    pub fn new(pos: Vec3, mass: f32, radius: f32) -> Self {
        Sphere {
            pos,
            vel: Vec3::ZERO,
            mass,
            radius,
            restitution: 0.2,
            friction: 0.4,
            resting: false,
        }
    }

    /// Advance one step of `dt` seconds under gravitational acceleration `accel`, resting on a flat
    /// ground whose surface is at world-Y = `ground_y` directly beneath the sphere.
    pub fn step(&mut self, accel: Vec3, dt: f32, ground_y: f32) {
        // Semi-implicit (symplectic) Euler.
        self.vel += accel * dt;
        self.pos += self.vel * dt;

        let floor = ground_y + self.radius;
        if self.pos.y <= floor {
            self.pos.y = floor;
            if self.vel.y < 0.0 {
                self.vel.y = -self.vel.y * self.restitution;
            }
            // Tangential friction.
            self.vel.x *= 1.0 - self.friction;
            self.vel.z *= 1.0 - self.friction;
            // Rest when the post-bounce speed is small relative to one gravity step. Scaling by
            // `accel·dt` makes this work at both Earth-g and asteroid micro-g (a fixed absolute
            // threshold cannot, since steady-state bounce speed ∝ restitution·|accel|·dt).
            let rest_threshold = (2.0 * accel.length() * dt).max(REST_SPEED);
            if self.vel.length() < rest_threshold {
                self.vel = Vec3::ZERO;
                self.resting = true;
            }
        } else {
            self.resting = false;
        }
    }

    /// Height of the sphere's lowest point above the ground surface at `ground_y`.
    pub fn altitude(&self, ground_y: f32) -> f32 {
        (self.pos.y - self.radius) - ground_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_fall_matches_kinematics() {
        // Under constant downward accel g, semi-implicit Euler gives v = -g·t exactly.
        let g = 9.81;
        let dt = 0.001;
        let steps = 1000; // t = 1.0 s
        let mut s = Sphere::new(Vec3::new(0.0, 1000.0, 0.0), 5.0, 0.05);
        for _ in 0..steps {
            s.step(Vec3::new(0.0, -g, 0.0), dt, -1.0e30); // ground far below
        }
        let t = dt * steps as f32;
        assert!((s.vel.y - (-g * t)).abs() < 1e-3, "v = -g t");
        // Position within a percent of the analytic ½·g·t² drop.
        let drop = 1000.0 - s.pos.y;
        let expected = 0.5 * g * t * t;
        assert!(
            (drop - expected).abs() / expected < 0.01,
            "drop ~= 1/2 g t^2"
        );
    }

    #[test]
    fn falls_and_rests_on_ground() {
        let g = 9.81;
        let dt = 0.01;
        let ground_y = 0.0;
        let mut s = Sphere::new(Vec3::new(0.0, 10.0, 0.0), 5.0, 0.5);
        for _ in 0..100_000 {
            s.step(Vec3::new(0.0, -g, 0.0), dt, ground_y);
            if s.resting {
                break;
            }
        }
        assert!(s.resting, "sphere should come to rest");
        assert!(
            (s.pos.y - (ground_y + s.radius)).abs() < 1e-3,
            "rests on the surface"
        );
        assert!(s.vel.length() < 1e-3, "velocity ~0 at rest");
    }
}
