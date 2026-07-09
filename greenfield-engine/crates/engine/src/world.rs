//! The voxel matter store and the Phase 1 layered-world generator.
//!
//! Each voxel holds a material index (0 = empty/air, else `material_index + 1`). This is the
//! authoritative "matter store" — later phases attach per-voxel density = material.density (so
//! summed mass drives gravity) and activate voxels into MPM particles under stress. For Phase 1 we
//! generate a layered plateau — rock, ~10 m of dirt, a skin of grass — and render it.

use crate::materials::{index_of, Material};

/// Width (X), height (Y, up), depth (Z) of the world in voxels. 1 voxel = 1 metre.
pub const W: usize = 96;
pub const H: usize = 56;
pub const D: usize = 96;

const DIRT_THICKNESS: usize = 10; // "10 m of dirt", per the project brief
const GRASS_THICKNESS: usize = 1; // thin fragile skin

pub struct World {
    pub w: usize,
    pub h: usize,
    pub d: usize,
    /// `voxels[idx] == 0` is air; otherwise the material index is `voxels[idx] - 1`.
    pub voxels: Vec<u16>,
    /// Tallest column, for centering the camera on the terrain.
    pub max_top: usize,
}

impl World {
    #[inline]
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (y * self.d + z) * self.w + x
    }

    /// Material index at a voxel, or `None` for air / out of bounds.
    #[inline]
    pub fn material_at(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if x < 0
            || y < 0
            || z < 0
            || x as usize >= self.w
            || y as usize >= self.h
            || z as usize >= self.d
        {
            return None;
        }
        let v = self.voxels[self.idx(x as usize, y as usize, z as usize)];
        if v == 0 {
            None
        } else {
            Some((v - 1) as usize)
        }
    }

    #[inline]
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.material_at(x, y, z).is_some()
    }
}

/// Generate the layered Phase 1 world using materials resolved from the seed database.
/// Rock forms the bulk; `dirt` sits on top; `grass` is the surface skin. A gentle value-noise
/// heightfield makes the surface undulate a few metres so it reads as terrain, not a slab — and
/// the layers follow the terrain, visible on the exposed side walls.
pub fn generate(materials: &[Material]) -> World {
    let rock = index_of(materials, "granite") as u16 + 1;
    let dirt = index_of(materials, "dirt") as u16 + 1;
    let grass = index_of(materials, "grass") as u16 + 1;

    let mut voxels = vec![0u16; W * H * D];
    let base_top = H as i32 - 8; // leave headroom above the terrain
    let amplitude = 6.0f32;

    let mut max_top = 0usize;
    for z in 0..D {
        for x in 0..W {
            let n = fbm(x as f32, z as f32); // 0..1
            let top = (base_top as f32 - amplitude * (1.0 - n)).round() as i32;
            let top = top.clamp(
                DIRT_THICKNESS as i32 + GRASS_THICKNESS as i32 + 1,
                H as i32 - 1,
            );
            let grass_start = top - GRASS_THICKNESS as i32;
            let dirt_start = grass_start - DIRT_THICKNESS as i32;
            for y in 0..top {
                let v = if y >= grass_start {
                    grass
                } else if y >= dirt_start {
                    dirt
                } else {
                    rock
                };
                let i = (y as usize * D + z) * W + x;
                voxels[i] = v;
            }
            max_top = max_top.max(top as usize);
        }
    }

    World {
        w: W,
        h: H,
        d: D,
        voxels,
        max_top,
    }
}

// --- deterministic value noise (no RNG; stable across runs/clients) ---

fn hash2(x: i32, z: i32) -> f32 {
    let mut h = (x.wrapping_mul(374_761_393)).wrapping_add(z.wrapping_mul(668_265_263)) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t) // smoothstep
}

/// Bilinearly-interpolated value noise at lattice frequency `freq`.
fn value_noise(x: f32, z: f32, freq: f32) -> f32 {
    let fx = x * freq;
    let fz = z * freq;
    let x0 = fx.floor() as i32;
    let z0 = fz.floor() as i32;
    let tx = smooth(fx - x0 as f32);
    let tz = smooth(fz - z0 as f32);
    let a = hash2(x0, z0);
    let b = hash2(x0 + 1, z0);
    let c = hash2(x0, z0 + 1);
    let d = hash2(x0 + 1, z0 + 1);
    let top = a + (b - a) * tx;
    let bot = c + (d - c) * tx;
    top + (bot - top) * tz
}

/// Two-octave fractal noise in 0..1.
fn fbm(x: f32, z: f32) -> f32 {
    let n = 0.65 * value_noise(x, z, 0.045) + 0.35 * value_noise(x, z, 0.11);
    n.clamp(0.0, 1.0)
}
