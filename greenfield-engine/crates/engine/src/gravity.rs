//! Self-gravity from the world's aggregate matter.
//!
//! Every solid voxel has mass = its material's density × voxel volume (1 m³ ⇒ mass in kg equals
//! density). The gravitational acceleration at a point is the Newtonian sum over all that mass:
//!
//! `g(p) = Σ_i  G · m_i · (c_i − p) / |c_i − p|³`
//!
//! Summing every voxel per query would be wasteful, so we aggregate voxels into coarse blocks
//! (one mass point per block at its center of mass) — a fixed one-level approximation of the
//! Barnes–Hut idea (`docs/02`, `docs/08`). This is exact in the far field and cheap enough to
//! query many times per frame. Real `G` is used: for a small (asteroid-scale) world the field is
//! genuinely tiny — that is correct physics, not a bug.

use crate::materials::Material;
use crate::world::World;
use glam::{DVec3, Vec3};

/// Newton's gravitational constant (m³·kg⁻¹·s⁻²).
pub const G: f32 = 6.674e-11;

/// One aggregated lump of matter: its center of mass (in the world's centered coordinates) and mass.
#[derive(Clone, Copy)]
pub struct MassPoint {
    pub center: Vec3,
    pub mass: f32,
}

/// A gravitational source: aggregated mass points plus the world total.
pub struct MassField {
    pub points: Vec<MassPoint>,
    pub total_mass: f32,
    /// Center of mass (centered coords). Used by tests/tooling; not read by the renderer yet.
    #[allow(dead_code)]
    pub com: Vec3,
}

impl MassField {
    /// Build the field by aggregating voxels into `block`³ lumps. Positions are in the world's
    /// **centered** coordinates (matching the rendered mesh), so gravity and geometry share a frame.
    pub fn build(world: &World, materials: &[Material], block: usize) -> MassField {
        let block = block.max(1);
        let nbx = world.w.div_ceil(block);
        let nbz = world.d.div_ceil(block);
        let nby = world.h.div_ceil(block);
        let nblocks = nbx * nby * nbz;

        // Accumulate in f64: the world's total mass is ~1e9 kg over hundreds of thousands of
        // voxels, which would lose precision in f32.
        let mut mass = vec![0.0f64; nblocks];
        let mut wpos = vec![DVec3::ZERO; nblocks]; // mass-weighted position sum (voxel coords)

        let bidx = |bx: usize, by: usize, bz: usize| (by * nbz + bz) * nbx + bx;

        let mut total_mass = 0.0f64;
        let mut total_wpos = DVec3::ZERO;

        for y in 0..world.h {
            for z in 0..world.d {
                for x in 0..world.w {
                    let m = match world.material_at(x as i32, y as i32, z as i32) {
                        Some(mat) => materials[mat].density as f64,
                        None => continue,
                    };
                    // Voxel center in voxel coordinates.
                    let vc = DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
                    let b = bidx(x / block, y / block, z / block);
                    mass[b] += m;
                    wpos[b] += vc * m;
                    total_mass += m;
                    total_wpos += vc * m;
                }
            }
        }

        let center = world.center();
        let mut points = Vec::new();
        for b in 0..nblocks {
            if mass[b] > 0.0 {
                let centroid = (wpos[b] / mass[b]).as_vec3() - center;
                points.push(MassPoint {
                    center: centroid,
                    mass: mass[b] as f32,
                });
            }
        }

        let com = if total_mass > 0.0 {
            (total_wpos / total_mass).as_vec3() - center
        } else {
            Vec3::ZERO
        };

        MassField {
            points,
            total_mass: total_mass as f32,
            com,
        }
    }

    /// Gravitational acceleration at `p`. `softening` (metres) removes the singularity when very
    /// close to a mass point; keep it ~the block size. Acceleration is mass-independent (the
    /// equivalence principle): the same `g` acts on a 5 kg or a 5 t probe.
    pub fn acceleration_at(&self, p: Vec3, softening: f32) -> Vec3 {
        let s2 = softening * softening;
        let mut a = Vec3::ZERO;
        for mp in &self.points {
            let d = mp.center - p;
            let r2 = d.length_squared() + s2;
            // 1 / r³ (softened): r2^{-1.5}
            let inv_r3 = r2.powf(-1.5);
            a += d * (G * mp.mass * inv_r3);
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_mass_matches_analytic() {
        // A single 1e9 kg lump 10 m along +x from the origin: g at origin is G·M/r² toward +x.
        let m = 1.0e9;
        let field = MassField {
            points: vec![MassPoint {
                center: Vec3::new(10.0, 0.0, 0.0),
                mass: m,
            }],
            total_mass: m,
            com: Vec3::new(10.0, 0.0, 0.0),
        };
        let a = field.acceleration_at(Vec3::ZERO, 0.0);
        let expected = G * m / (10.0 * 10.0);
        assert!((a.length() - expected).abs() / expected < 1e-4, "magnitude");
        // Points toward the mass (+x), negligible off-axis.
        assert!(
            a.x > 0.0 && a.y.abs() < 1e-6 && a.z.abs() < 1e-6,
            "direction"
        );
    }

    #[test]
    fn far_field_matches_total_point_mass() {
        // Far from the world, the aggregate should look like a point mass at the COM.
        let mats = crate::materials::load();
        let w = crate::world::generate(&mats);
        let field = MassField::build(&w, &mats, 4);
        assert!(field.total_mass > 0.0);

        // Aggregation conserves mass.
        let summed: f32 = field.points.iter().map(|p| p.mass).sum();
        assert!(
            (summed - field.total_mass).abs() / field.total_mass < 1e-3,
            "mass conserved"
        );

        // A test point far above the COM.
        let p = field.com + Vec3::new(0.0, 100_000.0, 0.0);
        let a = field.acceleration_at(p, 4.0);
        let r = (p - field.com).length();
        let expected = G * field.total_mass / (r * r);
        assert!(
            (a.length() - expected).abs() / expected < 0.01,
            "far-field within 1% of G*M/r^2 (got {}, expected {})",
            a.length(),
            expected
        );
        // Pulls back down toward the world.
        assert!(a.y < 0.0, "far-field points toward the mass");
    }
}
