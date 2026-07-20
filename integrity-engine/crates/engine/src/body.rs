//! A single rigid sphere: integrate under gravity, then collide with the solid voxel world so it
//! behaves like a solid object — it rests on surfaces and never penetrates walls or crater sides.
//!
//! Phase 2 has one dynamic body (the dropped probe), so we integrate directly (`F = ma`, semi-implicit
//! Euler) and resolve collisions against the voxel grid ourselves rather than pulling in a full
//! rigid-body engine. Rapier earns its place once we need many bodies and rich contacts.
//!
//! Equivalence principle: gravitational acceleration is mass-independent, so the probe falls at the
//! field's `g` whatever its mass — the "5 kg" is a label. Mass matters for momentum/collision.

use crate::world::World;
use glam::Vec3;

/// Speed below which a resting body is snapped to a stop (avoids endless micro-bouncing).
const REST_SPEED: f32 = 1.0e-4;
/// Max push-out iterations per step (resolves floor + wall + corner contacts).
const COLLISION_ITERS: usize = 8;

pub struct Sphere {
    pub pos: Vec3,
    pub vel: Vec3,
    /// kg. Not read yet (free-fall is mass-independent); kept for momentum in later phases.
    #[allow(dead_code)]
    pub mass: f32,
    pub radius: f32,
    pub restitution: f32,
    /// Fraction of tangential speed removed per contact (0 = frictionless, 1 = instant stop).
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

    /// Integrate one step of `dt` under gravitational acceleration `accel` (no collision yet).
    pub fn integrate(&mut self, accel: Vec3, dt: f32) {
        self.vel += accel * dt;
        self.pos += self.vel * dt;
    }

    /// Push the sphere out of any solid voxels it overlaps and respond to the contact, so it acts
    /// like a solid object. Iteratively resolves the deepest contact (floor, then walls, corners),
    /// reflecting the into-surface velocity (restitution) and damping the tangent (friction).
    /// `ref_accel` sets a scale-relative rest threshold so it works from Earth-g to micro-g.
    pub fn collide(&mut self, world: &World, ref_accel: Vec3, dt: f32) {
        let center = world.center();
        let r = self.radius;
        let mut contacted = false;

        for _ in 0..COLLISION_ITERS {
            let sp = self.pos + center; // sphere center in voxel coordinates
            let lo = sp - r;
            let hi = sp + r;

            // Find the solid voxel the sphere penetrates deepest (smallest surface distance).
            let mut best_dist = r;
            let mut best_n = Vec3::ZERO;
            let mut found = false;
            for vz in (lo.z.floor() as i32)..(hi.z.ceil() as i32) {
                for vy in (lo.y.floor() as i32)..(hi.y.ceil() as i32) {
                    for vx in (lo.x.floor() as i32)..(hi.x.ceil() as i32) {
                        if !world.is_solid(vx, vy, vz) {
                            continue;
                        }
                        let vmin = Vec3::new(vx as f32, vy as f32, vz as f32);
                        let closest = sp.clamp(vmin, vmin + Vec3::ONE);
                        let d = sp - closest;
                        let dist = d.length();
                        if dist < best_dist {
                            best_dist = dist;
                            best_n = if dist > 1e-5 { d / dist } else { Vec3::Y };
                            found = true;
                        }
                    }
                }
            }

            if !found {
                break;
            }
            // Push out along the contact normal, then respond in velocity.
            self.pos += best_n * (r - best_dist);
            contacted = true;
            let vn = self.vel.dot(best_n);
            if vn < 0.0 {
                self.vel -= best_n * (vn * (1.0 + self.restitution));
            }
            let tangent = self.vel - best_n * self.vel.dot(best_n);
            self.vel -= tangent * self.friction;
        }

        if contacted {
            let rest_threshold = (2.0 * ref_accel.length() * dt).max(REST_SPEED);
            self.resting = self.vel.length() < rest_threshold;
            if self.resting {
                self.vel = Vec3::ZERO;
            }
        } else {
            self.resting = false;
        }
    }

    /// Height of the sphere's lowest point above a reference ground Y (HUD readout).
    pub fn altitude(&self, ground_y: f32) -> f32 {
        (self.pos.y - self.radius) - ground_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    /// A world with a flat solid slab (y in 0..4) filling the footprint.
    fn slab_world() -> World {
        let (w, h, d) = (8usize, 8usize, 8usize);
        let mut voxels = vec![0u16; w * h * d];
        for y in 0..4 {
            for z in 0..d {
                for x in 0..w {
                    voxels[(y * d + z) * w + x] = 1;
                }
            }
        }
        World::from_voxels(w, h, d, voxels, 4, None)
    }

    /// A world that is solid for x < 4 (a wall / half-space), air elsewhere.
    fn wall_world() -> World {
        let (w, h, d) = (8usize, 8usize, 8usize);
        let mut voxels = vec![0u16; w * h * d];
        for y in 0..h {
            for z in 0..d {
                for x in 0..4 {
                    voxels[(y * d + z) * w + x] = 1;
                }
            }
        }
        World::from_voxels(w, h, d, voxels, 8, None)
    }

    /// Deepest penetration of the sphere into any solid voxel (0 = not overlapping).
    fn max_penetration(s: &Sphere, w: &World) -> f32 {
        let center = w.center();
        let sp = s.pos + center;
        let r = s.radius;
        let mut pen = 0.0f32;
        for vz in ((sp.z - r).floor() as i32)..((sp.z + r).ceil() as i32) {
            for vy in ((sp.y - r).floor() as i32)..((sp.y + r).ceil() as i32) {
                for vx in ((sp.x - r).floor() as i32)..((sp.x + r).ceil() as i32) {
                    if w.is_solid(vx, vy, vz) {
                        let vmin = Vec3::new(vx as f32, vy as f32, vz as f32);
                        let closest = sp.clamp(vmin, vmin + Vec3::ONE);
                        let dist = (sp - closest).length();
                        if dist < r {
                            pen = pen.max(r - dist);
                        }
                    }
                }
            }
        }
        pen
    }

    #[test]
    fn free_fall_matches_kinematics() {
        // Under constant downward accel g, semi-implicit Euler gives v = -g·t exactly.
        let g = 9.81;
        let dt = 0.001;
        let steps = 1000; // t = 1.0 s
        let mut s = Sphere::new(Vec3::new(0.0, 1000.0, 0.0), 5.0, 0.05);
        for _ in 0..steps {
            s.integrate(Vec3::new(0.0, -g, 0.0), dt);
        }
        let t = dt * steps as f32;
        assert!((s.vel.y - (-g * t)).abs() < 1e-3, "v = -g t");
        let drop = 1000.0 - s.pos.y;
        assert!(
            (drop - 0.5 * g * t * t).abs() / (0.5 * g * t * t) < 0.01,
            "drop ~= 1/2 g t^2"
        );
    }

    #[test]
    fn rests_on_voxel_floor_without_penetrating() {
        let w = slab_world(); // solid top surface at voxel y=4 → centered y = 4 - center.y = 2
        let mut s = Sphere::new(Vec3::new(0.0, 5.0, 0.0), 5.0, 1.0);
        let g = 9.81;
        let dt = 0.01;
        for _ in 0..100_000 {
            let a = Vec3::new(0.0, -g, 0.0);
            s.integrate(a, dt);
            s.collide(&w, a, dt);
            if s.resting {
                break;
            }
        }
        assert!(s.resting, "sphere should come to rest on the slab");
        assert!(
            max_penetration(&s, &w) < 0.05,
            "must not sink into the ground"
        );
        assert!(
            (s.pos.y - (2.0 + s.radius)).abs() < 0.4,
            "rests on the surface"
        );
    }

    #[test]
    fn wakes_and_falls_when_support_is_removed() {
        // Physical honesty: a body sleeps only while something holds it up. Remove its support and it
        // must wake and fall on the very next step — "leave it unsupported and it falls" is structural,
        // not scripted. (Falls through vacuum: no atmosphere is modelled, so no drag.)
        let mut w = slab_world();
        let mut s = Sphere::new(Vec3::new(0.0, 5.0, 0.0), 5.0, 1.0);
        let g = Vec3::new(0.0, -9.81, 0.0);
        let dt = 0.01;
        for _ in 0..100_000 {
            s.integrate(g, dt);
            s.collide(&w, g, dt);
            if s.resting {
                break;
            }
        }
        assert!(s.resting, "sphere first comes to rest on the slab");

        // Dig the slab out from under it.
        for y in 0..4 {
            for z in 0..w.d {
                for x in 0..w.w {
                    w.set_voxel(x as i32, y, z as i32, None);
                }
            }
        }
        s.integrate(g, dt);
        s.collide(&w, g, dt);
        assert!(!s.resting, "removing the support wakes the body");
        assert!(s.vel.y < 0.0, "it accelerates downward under gravity");
    }

    #[test]
    fn does_not_clip_into_a_wall() {
        // Sphere placed overlapping the wall face (at centered x=0) must be pushed out to the +x side.
        let w = wall_world();
        let mut s = Sphere::new(Vec3::new(0.4, 0.0, 0.0), 5.0, 1.0);
        assert!(
            max_penetration(&s, &w) > 0.0,
            "test starts overlapping the wall"
        );
        s.collide(&w, Vec3::ZERO, 0.016);
        assert!(
            max_penetration(&s, &w) < 0.05,
            "collision pushes the sphere out of the wall"
        );
        assert!(
            s.pos.x > 0.9,
            "sphere ends on the open (+x) side of the wall face"
        );
    }
}
