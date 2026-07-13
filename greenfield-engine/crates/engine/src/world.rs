//! The voxel matter store and the Phase 1 layered-world generator.
//!
//! Each voxel holds a material index (0 = empty/air, else `material_index + 1`). This is the
//! authoritative "matter store" — later phases attach per-voxel density = material.density (so
//! summed mass drives gravity) and activate voxels into MPM particles under stress. The generator
//! lays a surface patch of the REAL layered Earth — grass skin, basalt crust, peridotite mantle, iron
//! core — as a declared vertical LOD (real materials/order, compressed depths; docs/25/28).

use crate::materials::{index_of, Material};
use glam::{IVec3, Vec3};
use std::collections::VecDeque;

/// Width (X), height (Y, up), depth (Z) of the world in voxels. 1 voxel = 1 metre.
pub const W: usize = 96;
pub const H: usize = 96;
pub const D: usize = 96;

const GRASS_THICKNESS: usize = 1; // thin fragile biosphere skin over the crust

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

    /// The offset used to center the world on the origin (shared by the mesher, gravity, and
    /// physics so geometry and forces live in the same coordinate frame).
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            self.w as f32 * 0.5,
            self.max_top as f32 * 0.5,
            self.d as f32 * 0.5,
        )
    }

    /// The Y (in voxel units) where air begins above column `(x, z)` — i.e. the surface top.
    /// `None` if the column is empty or out of bounds.
    pub fn surface_top_voxel(&self, x: i32, z: i32) -> Option<i32> {
        if x < 0 || z < 0 || x as usize >= self.w || z as usize >= self.d {
            return None;
        }
        for y in (0..self.h as i32).rev() {
            if self.is_solid(x, y, z) {
                return Some(y + 1);
            }
        }
        None
    }

    /// Is a camera `eye` (in CENTERED coordinates, the frame `center()` maps to voxel space) in free
    /// air, at least `clearance` metres above the ground beneath it? "Free" means the voxel the eye sits
    /// in is not solid AND, where a ground column exists under the eye, the eye is `clearance` above that
    /// column's surface top. Off the terrain footprint (out-of-bounds column) there is no ground beneath,
    /// so the eye is free — the patch is a finite chunk of matter in vacuum, not walled in.
    pub fn eye_is_free(&self, eye: Vec3, clearance: f32) -> bool {
        let p = eye + self.center();
        let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if self.is_solid(xi, yi, zi) {
            return false;
        }
        match self.surface_top_voxel(xi, zi) {
            Some(top) => p.y >= top as f32 + clearance,
            None => true, // no ground column here (beside/beyond the patch)
        }
    }

    /// Clamp a third-person orbit camera eye (CENTERED coords) so it never sits inside solid matter or
    /// below the terrain surface. If the requested eye is already free air a `clearance` above the
    /// ground, it is returned UNCHANGED (normal orbit/zoom stays smooth). Otherwise the eye is pulled
    /// back along its own radial direction (a third-person "boom", preserving yaw/pitch) in half-voxel
    /// steps until it reaches free air — the honest fix for zoom-into-the-ground. For a degenerate
    /// near-straight-down aim (little horizontal spread, or a radial pull-back that can only burrow
    /// deeper), it falls back to lifting the eye straight up until it clears the surface.
    pub fn clamp_eye_outside(&self, eye: Vec3, clearance: f32) -> Vec3 {
        if self.eye_is_free(eye, clearance) {
            return eye;
        }
        let dir = eye.normalize_or_zero();
        let horiz = (dir.x * dir.x + dir.z * dir.z).sqrt();
        // World bounding-sphere radius (centered): any point past it is guaranteed outside the matter.
        let bound = ((self.w * self.w + self.h * self.h + self.d * self.d) as f32).sqrt()
            + clearance
            + 4.0;

        // Radial boom pull-back: only meaningful when the ray has real horizontal spread AND is not
        // aimed down into the ground (a down-pointing ray only burrows deeper as it lengthens).
        if dir != Vec3::ZERO && horiz >= 0.1 && dir.y >= -0.05 {
            let mut d = eye.length();
            while d <= bound {
                let e = dir * d;
                if self.eye_is_free(e, clearance) {
                    return e;
                }
                d += 0.5;
            }
        }

        // Vertical-lift fallback: raise the eye straight up until it clears the surface above it.
        let mut e = eye;
        let ceiling = self.h as f32 - self.center().y + clearance + 1.0;
        while e.y <= ceiling {
            if self.eye_is_free(e, clearance) {
                return e;
            }
            e.y += 0.5;
        }
        e
    }

    /// Clamp a terrain camera eye (CENTERED coords) so it can NEVER go below the real Earth surface —
    /// anywhere, not just over the resolved 96 m voxel patch. The bulk planet is a sphere of `radius`
    /// centred at `earth_center` (in the same centered frame; ≈ a full Earth radius straight down under
    /// the uniform surface gravity). Two constraints, both enforced:
    ///   1. The eye stays at least `clearance` ABOVE the Earth sphere: its distance from `earth_center`
    ///      is ≥ `radius + clearance`. If it dips inside, it is pushed radially back out from the centre
    ///      (so off the patch, over the summarized bulk surface, the horizon still walls the eye in —
    ///      closing the old "off-footprint is free" hole that only the fake flat plane made safe).
    ///   2. The eye stays out of the resolved voxel hills (the existing local clamp), which can rise
    ///      ABOVE the smooth sphere where the patch relief pokes up.
    /// Computed in f64: `radius` is ~6.4e6 m, so the metre-scale curvature drop is below f32 precision.
    pub fn clamp_eye_above_earth(
        &self,
        eye: Vec3,
        earth_center: Vec3,
        radius: f32,
        clearance: f32,
    ) -> Vec3 {
        let c = earth_center.as_dvec3();
        let rel = eye.as_dvec3() - c;
        let d = rel.length();
        let min_d = radius as f64 + clearance as f64;
        let lifted = if d > 1e-9 && d < min_d {
            (c + rel * (min_d / d)).as_vec3()
        } else {
            eye
        };
        // Then keep it clear of the resolved voxel patch (lifting only ever increases the distance from
        // the Earth centre, so the sphere constraint from above still holds).
        self.clamp_eye_outside(lifted, clearance)
    }

    /// Set a voxel's material (`None` = air). Out-of-bounds writes are ignored.
    pub fn set_voxel(&mut self, x: i32, y: i32, z: i32, material: Option<usize>) {
        if x < 0
            || y < 0
            || z < 0
            || x as usize >= self.w
            || y as usize >= self.h
            || z as usize >= self.d
        {
            return;
        }
        let i = self.idx(x as usize, y as usize, z as usize);
        self.voxels[i] = material.map(|m| m as u16 + 1).unwrap_or(0);
    }

    /// Total number of solid voxels — used for matter-conservation checks (tests).
    #[allow(dead_code)]
    pub fn solid_count(&self) -> usize {
        self.voxels.iter().filter(|&&v| v != 0).count()
    }

    /// March a ray (given in centered coordinates) through the grid; return the first solid voxel it
    /// hits and the centered hit position. Amanatides–Woo DDA — used for click-to-dig picking.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(i32, i32, i32, Vec3)> {
        let d = dir.normalize_or_zero();
        if d == Vec3::ZERO {
            return None;
        }
        let o = origin + self.center(); // ray origin in voxel space

        let mut v = IVec3::new(o.x.floor() as i32, o.y.floor() as i32, o.z.floor() as i32);
        let step = IVec3::new(sign(d.x), sign(d.y), sign(d.z));

        // Parametric distance to the first voxel boundary on each axis, and per-voxel increments.
        let t_max = |oc: f32, dc: f32, s: i32| -> f32 {
            if dc == 0.0 {
                f32::INFINITY
            } else if s > 0 {
                (oc.floor() + 1.0 - oc) / dc
            } else {
                (oc.floor() - oc) / dc
            }
        };
        let mut tmx = t_max(o.x, d.x, step.x);
        let mut tmy = t_max(o.y, d.y, step.y);
        let mut tmz = t_max(o.z, d.z, step.z);
        let tdx = if d.x != 0.0 {
            (1.0 / d.x).abs()
        } else {
            f32::INFINITY
        };
        let tdy = if d.y != 0.0 {
            (1.0 / d.y).abs()
        } else {
            f32::INFINITY
        };
        let tdz = if d.z != 0.0 {
            (1.0 / d.z).abs()
        } else {
            f32::INFINITY
        };

        let mut t = 0.0f32;
        for _ in 0..8192 {
            if self.is_solid(v.x, v.y, v.z) {
                return Some((v.x, v.y, v.z, origin + d * t));
            }
            if tmx <= tmy && tmx <= tmz {
                v.x += step.x;
                t = tmx;
                tmx += tdx;
            } else if tmy <= tmz {
                v.y += step.y;
                t = tmy;
                tmy += tdy;
            } else {
                v.z += step.z;
                t = tmz;
                tmz += tdz;
            }
            if t > max_dist {
                break;
            }
        }
        None
    }

    /// Solid voxels **not** connected (6-connectivity, through solid) to the anchored base (the
    /// `y = 0` layer). These are unsupported and should collapse. A flood-fill from the base marks
    /// everything supported; the rest is returned. O(number of voxels).
    pub fn find_unsupported(&self) -> Vec<(i32, i32, i32)> {
        const NEIGHBORS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        let mut supported = vec![false; self.w * self.h * self.d];
        let mut stack: Vec<usize> = Vec::new();

        // Seed with every solid voxel in the base layer.
        for z in 0..self.d {
            for x in 0..self.w {
                if self.is_solid(x as i32, 0, z as i32) {
                    let i = self.idx(x, 0, z);
                    if !supported[i] {
                        supported[i] = true;
                        stack.push(i);
                    }
                }
            }
        }

        // Flood-fill through connected solid voxels.
        while let Some(i) = stack.pop() {
            let x = i % self.w;
            let rem = i / self.w;
            let z = rem % self.d;
            let y = rem / self.d;
            for (dx, dy, dz) in NEIGHBORS {
                let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                if self.is_solid(nx, ny, nz) {
                    let j = self.idx(nx as usize, ny as usize, nz as usize);
                    if !supported[j] {
                        supported[j] = true;
                        stack.push(j);
                    }
                }
            }
        }

        // Collect solid voxels the fill never reached.
        let mut out = Vec::new();
        for y in 0..self.h {
            for z in 0..self.d {
                for x in 0..self.w {
                    if self.is_solid(x as i32, y as i32, z as i32) && !supported[self.idx(x, y, z)]
                    {
                        out.push((x as i32, y as i32, z as i32));
                    }
                }
            }
        }
        out
    }

    /// Solid voxels that gravity cannot hold up — the honest structural-support model (`docs/28`).
    ///
    /// The old [`Self::find_unsupported`] called a voxel "supported" if ANY 6-connected path of solid
    /// reached the base, so an overhanging crater lip attached SIDEWAYS to the rim counted as supported
    /// even with nothing beneath it. That is unphysical: matter is held against gravity by support FROM
    /// BELOW. Here support propagates from the base UPWARD, with a material-strength-limited cantilever:
    ///
    /// 1. **DIRECTLY SUPPORTED** — a column to the base: `(x, y, z)` is directly supported iff it is
    ///    solid AND (`y == 0` OR the voxel directly below `(x, y-1, z)` is directly supported). A single
    ///    bottom-up sweep resolves the whole column.
    /// 2. **BRACED** (cantilever) — a solid voxel that is NOT directly supported is still held if it lies
    ///    within its material's CANTILEVER REACH, laterally at its own `y`-level (through solid), of a
    ///    directly-supported voxel. The reach is DERIVED from the material, not tuned: a ~1-voxel-thick
    ///    cantilever beam of length `L` carrying its own weight develops a root bending stress that grows
    ///    as `σ ≈ ρ·g·L² / t` (Euler–Bernoulli beam under a uniform self-weight load). It fails when that
    ///    reaches the material's tensile strength `σ_t`, giving
    ///        `L_max ≈ sqrt(σ_t · t / (ρ · g))`     (`t` = voxel size = 1 m)
    ///    FLAGGED as a first-order STRUCTURAL APPROXIMATION — a declared model, derived from real material
    ///    properties (an order-1 geometric constant is dropped). With the real DB values it gives basalt
    ///    σ_t≈1.45e7 → L≈22 m (competent rock holds a real crater lip) and grass/soil σ_t≈1.5e4 → L≈1 m
    ///    (loose soil barely overhangs) — physically right. Lateral graph distance (unit steps through
    ///    solid) stands in for the beam's arc length.
    /// 3. **UNSUPPORTED** — solid AND neither directly supported nor braced. These collapse (see
    ///    [`crate::matter::MatterSim::collapse`]): an undercut overhang past its material's reach falls.
    ///
    /// `g` is the terrain's surface gravity (m/s²; the emergent ~9.88, the Engine's `surface_g`). This
    /// also subsumes pure disconnection — a floating chunk has no directly-supported voxel to brace from,
    /// so it is returned too. O(voxels).
    pub fn find_structurally_unsupported(&self, materials: &[Material], g: f32) -> Vec<(i32, i32, i32)> {
        let n = self.w * self.h * self.d;
        // Cantilever reach per material, in voxels (≈ metres): L_max = sqrt(σ_t · t / (ρ · g)), t = 1 m.
        let reach: Vec<f32> = materials
            .iter()
            .map(|m| {
                let rho = m.density.max(1.0);
                let gg = g.max(1.0e-6);
                (m.fracture_strength / (rho * gg)).max(0.0).sqrt()
            })
            .collect();

        // 1. Directly supported: bottom-up per-column sweep (a voxel stands on a directly-supported one).
        let mut directly = vec![false; n];
        for z in 0..self.d {
            for x in 0..self.w {
                for y in 0..self.h {
                    if !self.is_solid(x as i32, y as i32, z as i32) {
                        continue;
                    }
                    let i = self.idx(x, y, z);
                    directly[i] = y == 0 || directly[self.idx(x, y - 1, z)];
                }
            }
        }

        // 2. Braced: per y-level multi-source BFS from the directly-supported voxels, flooding laterally
        //    (4-connectivity in x,z) through solid. Each solid voxel's shortest lateral distance to a
        //    directly-supported voxel is its cantilever length; it is braced iff that ≤ its material reach.
        let mut dist = vec![-1i32; self.w * self.d]; // reused per level
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        let mut out = Vec::new();
        for y in 0..self.h {
            dist.iter_mut().for_each(|d| *d = -1);
            queue.clear();
            for z in 0..self.d {
                for x in 0..self.w {
                    if directly[self.idx(x, y, z)] {
                        dist[z * self.w + x] = 0;
                        queue.push_back((x, z));
                    }
                }
            }
            while let Some((x, z)) = queue.pop_front() {
                let d0 = dist[z * self.w + x];
                for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                    if nx < 0 || nz < 0 || nx as usize >= self.w || nz as usize >= self.d {
                        continue;
                    }
                    let (nx, nz) = (nx as usize, nz as usize);
                    if dist[nz * self.w + nx] >= 0 {
                        continue; // already reached at an equal-or-shorter distance
                    }
                    if self.is_solid(nx as i32, y as i32, nz as i32) {
                        dist[nz * self.w + nx] = d0 + 1;
                        queue.push_back((nx, nz));
                    }
                }
            }
            for z in 0..self.d {
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    if directly[i] {
                        continue; // supported from below already
                    }
                    let Some(mat) = self.material_at(x as i32, y as i32, z as i32) else {
                        continue; // air
                    };
                    let d = dist[z * self.w + x];
                    let braced = d >= 0 && (d as f32) <= reach[mat];
                    if !braced {
                        out.push((x as i32, y as i32, z as i32));
                    }
                }
            }
        }
        out
    }
}

fn sign(x: f32) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// Generate the world as a surface patch of the REAL layered Earth (planet::earth()): a grass skin over
/// basalt crust, peridotite mantle, iron core — Earth's true radial column as a declared VERTICAL LOD
/// (material order real; layer thicknesses compressed into the patch so the strata are visible when a
/// dig or impact excavates). A deterministic multi-octave value-noise heightfield gives the grassy
/// surface real rolling relief — hills and valleys, not a flat plateau.
pub fn generate(materials: &[Material]) -> World {
    // Real Earth column (planet::earth(), docs/25/28): a biosphere skin over basalt CRUST, peridotite
    // MANTLE, iron CORE. This is a DECLARED VERTICAL LOD: the material order is Earth's real radial
    // structure, but the layer THICKNESSES are rebalanced into the ~88-voxel patch (real crust is 0.4%
    // of the radius — invisible at true scale), so a dig or a giant impact exposes honest strata from
    // this surface frame (Robin: "see Theia impact from this perspective"). Depths are compressed —
    // flagged; 1 voxel = 1 m holds only for the near-surface probe/dig physics.
    let grass = index_of(materials, "grass") as u16 + 1;
    let crust = index_of(materials, "basalt") as u16 + 1;
    let mantle = index_of(materials, "peridotite") as u16 + 1;
    let core = index_of(materials, "iron") as u16 + 1;

    let mut voxels = vec![0u16; W * H * D];
    let base_top = H as i32 - 8; // highest possible surface; leaves headroom above the terrain
    let amplitude = 34.0f32; // peak-to-valley relief in voxels (≈ m): real rolling hills, not a plateau
    let valley_floor = base_top - amplitude as i32; // the LOWEST any surface top can reach

    // Flat strata boundaries (real geology is horizontal), anchored BENEATH the deepest valley so every
    // column — hilltop or valley bottom — carries the full grass → crust → mantle → core column. The
    // grass skin follows the undulating terrain top; the crust/mantle/core boundaries are level planes,
    // so a dig anywhere hits the same deep layer at the same absolute depth.
    const CRUST_VOX: i32 = 12; // basalt crust band (LOD-inflated from ~25 km)
    const MANTLE_VOX: i32 = 22; // peridotite mantle band
    let crust_bottom = valley_floor - CRUST_VOX;
    let mantle_bottom = crust_bottom - MANTLE_VOX;

    let mut max_top = 0usize;
    for z in 0..D {
        for x in 0..W {
            let n = fbm(x as f32, z as f32); // 0..1
            let top = (base_top as f32 - amplitude * (1.0 - n)).round() as i32;
            let top = top.clamp(GRASS_THICKNESS as i32 + 1, H as i32 - 1);
            let grass_start = top - GRASS_THICKNESS as i32;
            for y in 0..top {
                let v = if y >= grass_start {
                    grass
                } else if y >= crust_bottom {
                    crust
                } else if y >= mantle_bottom {
                    mantle
                } else {
                    core
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

/// Three-octave fractal noise in 0..1 (deterministic; no RNG, stable across runs/clients).
///
/// A broad low-frequency octave carries the large rolling hills and valleys across the map, a mid band
/// shapes individual slopes, and a fine octave adds surface texture. The weights sum to 1.0 so the
/// result stays in 0..1; the low-frequency term is weighted heaviest to give genuine map-wide relief
/// (not a flat plateau) rather than only local bumps.
fn fbm(x: f32, z: f32) -> f32 {
    let n = 0.55 * value_noise(x, z, 0.026)
        + 0.30 * value_noise(x, z, 0.062)
        + 0.15 * value_noise(x, z, 0.13);
    n.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;

    /// The declared cantilever reach for a material at gravity `g` (voxels ≈ m): the SAME derivation the
    /// support model uses, so the tests assert against the real physics, not a copied literal.
    fn reach_of(m: &Material, g: f32) -> f32 {
        (m.fracture_strength / (m.density.max(1.0) * g)).sqrt()
    }

    /// Build a bare world with a vertical support wall (column to the base at x=0, z=`z0`) and a
    /// horizontal cantilever beam of `mat`, `len` voxels long, jutting in +x at height `y0` over air.
    fn overhang_world(mat: usize, len: i32, y0: i32) -> World {
        let (w, h, d) = (48usize, 24usize, 8usize);
        let mut world = World {
            w,
            h,
            d,
            voxels: vec![0u16; w * h * d],
            max_top: y0 as usize + 1,
        };
        let z0 = (d / 2) as i32;
        // Support wall: a full column to the base at x=0, so its top voxel at y0 is DIRECTLY supported.
        for y in 0..=y0 {
            world.set_voxel(0, y, z0, Some(mat));
        }
        // Cantilever beam at y0, x = 1..=len, with nothing beneath it (air below) — a pure overhang.
        for x in 1..=len {
            world.set_voxel(x, y0, z0, Some(mat));
        }
        world
    }

    #[test]
    fn overhang_longer_than_material_reach_collapses() {
        // docs/28 (a): a soil/grass overhang LONGER than its cantilever reach must fail. Grass σ_t≈1.5e4,
        // ρ≈1400 → reach ≈ 1 m at surface g, so only the first voxel off the wall can hold; everything
        // past ~1 voxel is unsupported and returned to collapse.
        let mats = materials::load();
        let g = 9.88; // emergent surface gravity (Engine::surface_g); pass g, don't hardcode a reach
        let grass = materials::index_of(&mats, "grass");
        let reach = reach_of(&mats[grass], g);
        assert!(reach < 1.5, "grass barely overhangs (reach {reach:.2} m)");

        let len = 6;
        let y0 = 15;
        let w = overhang_world(grass, len, y0);
        let unsup: std::collections::HashSet<(i32, i32, i32)> =
            w.find_structurally_unsupported(&mats, g).into_iter().collect();
        let z0 = (w.d / 2) as i32;
        // Beam voxels at lateral distance d (= x) from the wall hold iff d ≤ reach; the rest fall.
        for x in 1..=len {
            let far = (x as f32) > reach;
            assert_eq!(
                unsup.contains(&(x, y0, z0)),
                far,
                "beam voxel x={x} (dist {x} vs reach {reach:.2}) support classification"
            );
        }
        // The wall's own column (support to base) is never returned.
        assert!(!unsup.contains(&(0, y0, z0)), "the support wall must hold");
        assert!(!unsup.contains(&(0, 0, z0)), "the base voxel must hold");
    }

    #[test]
    fn overhang_shorter_than_material_reach_holds() {
        // docs/28 (b): competent rock keeps a small lip. Basalt σ_t≈1.45e7, ρ≈2900 → reach ≈ 22 m, so a
        // 6-voxel basalt overhang is well within reach and NONE of it collapses (a real crater rim holds).
        let mats = materials::load();
        let g = 9.88;
        let basalt = materials::index_of(&mats, "basalt");
        let reach = reach_of(&mats[basalt], g);
        assert!(reach > 10.0, "basalt holds a real lip (reach {reach:.1} m)");

        let len = 6;
        assert!((len as f32) < reach, "the test overhang is shorter than the reach");
        let y0 = 15;
        let w = overhang_world(basalt, len, y0);
        assert!(
            w.find_structurally_unsupported(&mats, g).is_empty(),
            "a rock overhang shorter than its cantilever reach holds — nothing collapses"
        );
    }

    #[test]
    fn a_full_base_supported_column_never_collapses() {
        // docs/28 (c): matter in a solid column to the base is directly supported and never returned.
        let mats = materials::load();
        let g = 9.88;
        let w = generate(&mats);
        assert!(
            w.find_structurally_unsupported(&mats, g).is_empty(),
            "intact terrain — every column full to y=0 — is fully supported"
        );
    }

    #[test]
    fn a_disconnected_floating_chunk_still_collapses() {
        // docs/28 (d): don't regress pure disconnection. A chunk with no directly-supported voxel (no
        // column to the base) has nothing to brace from, so every voxel of it is returned to collapse.
        let mats = materials::load();
        let g = 9.88;
        let rock = materials::index_of(&mats, "basalt");
        let mut w = generate(&mats);
        // A small floating 2×2×2 block high above the terrain, disconnected from everything.
        let fy = w.max_top as i32 + 4;
        let mut expected = Vec::new();
        for dy in 0..2 {
            for dz in 0..2 {
                for dx in 0..2 {
                    let (x, y, z) = (4 + dx, fy + dy, 4 + dz);
                    w.set_voxel(x, y, z, Some(rock));
                    expected.push((x, y, z));
                }
            }
        }
        let unsup: std::collections::HashSet<(i32, i32, i32)> =
            w.find_structurally_unsupported(&mats, g).into_iter().collect();
        for e in expected {
            assert!(unsup.contains(&e), "floating chunk voxel {e:?} must collapse");
        }
    }

    #[test]
    fn column_is_earths_real_layers_top_to_bottom() {
        // docs/28 A: the terrain is a surface patch of the REAL layered Earth (planet::earth()) as a
        // declared vertical LOD — grass skin, basalt CRUST, peridotite MANTLE, iron CORE, in that order
        // down a column. Asserts the strata (not game grass/dirt/granite) so a dig/impact exposes honest
        // composition. Depths are LOD-compressed; the ORDER and MATERIALS are Earth's.
        let mats = materials::load();
        let w = generate(&mats);
        let id = |name| materials::index_of(&mats, name);
        let (cx, cz) = (W as i32 / 2, D as i32 / 2);
        let top = w.surface_top_voxel(cx, cz).expect("solid column at centre");

        // Surface skin is grass; the first solid below it is basalt crust.
        assert_eq!(w.material_at(cx, top - 1, cz), Some(id("grass")), "surface skin");
        // Walk down and record the sequence of DISTINCT materials encountered.
        let mut seq: Vec<usize> = Vec::new();
        for y in (0..top).rev() {
            if let Some(m) = w.material_at(cx, y, cz) {
                if seq.last() != Some(&m) {
                    seq.push(m);
                }
            }
        }
        assert_eq!(
            seq,
            vec![id("grass"), id("basalt"), id("peridotite"), id("iron")],
            "column must be Earth's real radial order: grass → crust → mantle → core"
        );
    }

    #[test]
    fn terrain_has_varied_elevation_and_is_solid_all_the_way_down() {
        // The surface must read as REAL rolling terrain — hills and valleys — not a near-flat plateau,
        // and the relief must be genuine matter: every column solid from its surface top down to the
        // base (matter all the way down, no holes). We measure the map-wide surface-top distribution.
        let mats = materials::load();
        let w = generate(&mats);

        let mut tops: Vec<f64> = Vec::with_capacity(W * D);
        for z in 0..D as i32 {
            for x in 0..W as i32 {
                let top = w
                    .surface_top_voxel(x, z)
                    .expect("every column must be solid (matter all the way down)");
                // No holes: solid from the base up to the surface top.
                for y in 0..top {
                    assert!(
                        w.is_solid(x, y, z),
                        "hole at ({x},{y},{z}) beneath surface top {top}"
                    );
                }
                tops.push(top as f64);
            }
        }

        let n = tops.len() as f64;
        let mean = tops.iter().sum::<f64>() / n;
        let std = (tops.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n).sqrt();
        let (min, max) = tops.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &t| {
            (lo.min(t), hi.max(t))
        });
        let range = max - min;

        // Threshold justification: 1 voxel ≈ 1 m. The old amplitude-6 heightfield was a near-flat
        // plateau (surface-top std under ~1 voxel, peak-to-valley range only a few voxels). Real
        // rolling terrain over this 96×96 patch must show many metres of relief, so we require
        // surface-top std ≥ 4 voxels (≈4 m of undulation about the mean) AND a peak-to-valley range
        // ≥ 15 voxels. The current heightfield measures std ≈ 4.6 and range ≈ 27 — comfortably above
        // these floors, and far above a slab. (Deterministic/seedless, so these values are stable.)
        assert!(
            std >= 4.0,
            "surface-top std must show real relief, not a plateau (got {std:.2} voxels)"
        );
        assert!(
            range >= 15.0,
            "peak-to-valley range must be substantial (got {range:.0} voxels)"
        );
    }

    #[test]
    fn ground_is_matter_all_the_way_through() {
        // Robin: "visible ground is matter all the way through — eliminating the lies of other game
        // engines." Every column must be SOLID contiguously from its surface top down to the base
        // (y = 0): no hollow shell, no air pockets. And a downward raycast from high above must strike
        // solid matter (the surface is not a paper-thin skin over a void).
        let mats = materials::load();
        let w = generate(&mats);

        for z in 0..D as i32 {
            for x in 0..W as i32 {
                let top = w
                    .surface_top_voxel(x, z)
                    .expect("every column must be solid (matter all the way down)");
                // Contiguous solid from the very base up to the surface top — no gaps anywhere.
                for y in 0..top {
                    assert!(
                        w.is_solid(x, y, z),
                        "air pocket / hollow at ({x},{y},{z}) below surface top {top}"
                    );
                }
                // The base voxel itself is matter (solid all the way THROUGH to y = 0).
                assert!(w.is_solid(x, 0, z), "hollow base at ({x},0,{z})");
            }
        }

        // A downward raycast from well above the centre column hits solid matter (not a void skin).
        let c = w.center();
        let start = Vec3::new(0.0, H as f32 - c.y + 10.0, 0.0); // centered coords, above the terrain
        let hit = w
            .raycast(start, Vec3::new(0.0, -1.0, 0.0), 4000.0)
            .expect("downward ray must strike solid ground");
        let (hx, hy, hz, _) = hit;
        assert!(
            w.is_solid(hx, hy, hz),
            "raycast reported a non-solid hit voxel"
        );
    }

    #[test]
    fn orbit_camera_never_penetrates_terrain() {
        // Robin: "camera should never penetrate matter." The terrain orbit camera builds its eye as
        //   eye = dir * (base_distance * zoom),  dir = (cos(pitch)sin(yaw), sin(pitch), cos(pitch)cos(yaw))
        // (see the wasm-gated `Engine::view_proj` / `set_orbit` in lib.rs, which is unreachable from
        // native tests — so we replicate the EXACT construction here). Zoomed in / tilted down, the raw
        // eye ends up buried in the terrain; `World::clamp_eye_outside` must push it back to free air.
        let mats = materials::load();
        let w = generate(&mats);

        let max_dim = w.w.max(w.h).max(w.d) as f32;
        let base_distance = max_dim * 1.6; // exactly Engine::create's construction
        let clearance = 2.0;

        let eye_for = |yaw: f32, pitch: f32, zoom: f32| -> Vec3 {
            let cp = pitch.cos();
            let dir = Vec3::new(cp * yaw.sin(), pitch.sin(), cp * yaw.cos());
            dir * (base_distance * zoom)
        };
        // A centered eye penetrates iff its voxel is solid, or it sits below the surface of its column.
        let penetrates = |eye: Vec3| -> bool {
            let p = eye + w.center();
            let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            if w.is_solid(xi, yi, zi) {
                return true;
            }
            match w.surface_top_voxel(xi, zi) {
                Some(top) => p.y < top as f32,
                None => false,
            }
        };

        // GUARD against a vacuous pass: a config that provably buries the raw eye in the terrain.
        let buried = eye_for(0.7, -1.4, 0.2);
        assert!(
            penetrates(buried),
            "test setup is vacuous: the raw eye must actually penetrate the terrain, got {buried:?}"
        );

        // Sweep the full yaw/pitch/zoom envelope (matching set_orbit's clamps: pitch ∈ [-1.5, 1.5],
        // zoom ∈ [0.2, 6.0]).
        let mut saw_penetration = false;
        for &yaw in &[0.0f32, 0.7, 1.5, 2.4, 3.14, 4.7, 6.0] {
            for &pitch in &[-1.5f32, -1.0, -0.5, -0.16, 0.0, 0.16, 0.6, 1.2, 1.5] {
                for &zoom in &[0.2f32, 0.4, 0.7, 1.0, 2.0, 4.0, 6.0] {
                    let raw = eye_for(yaw, pitch, zoom);
                    if penetrates(raw) {
                        saw_penetration = true;
                    }
                    let clamped = w.clamp_eye_outside(raw, clearance);
                    // The clamped eye must be free air, never inside/below matter.
                    assert!(
                        !penetrates(clamped),
                        "clamped eye still penetrates at yaw={yaw} pitch={pitch} zoom={zoom}: {clamped:?}"
                    );
                    assert!(
                        w.eye_is_free(clamped, clearance),
                        "clamped eye not clearance-above ground at yaw={yaw} pitch={pitch} zoom={zoom}"
                    );
                    // An already-free eye is returned UNCHANGED (smooth normal orbit).
                    if w.eye_is_free(raw, clearance) {
                        assert_eq!(
                            clamped, raw,
                            "free eye must be returned unchanged at yaw={yaw} pitch={pitch} zoom={zoom}"
                        );
                    }
                }
            }
        }
        assert!(
            saw_penetration,
            "sweep must exercise penetrating configs (else the clamp is untested)"
        );
    }

    #[test]
    fn eye_never_goes_below_the_earth_sphere_anywhere() {
        // Robin rejected the flat decorative ground: off the 96 m patch the OLD clamp treated the eye as
        // "free" (no ground column) — a hole that was only safe because a fake infinite plane hid it.
        // With the real curved Earth surface, the eye must be walled in by the planet EVERYWHERE — over
        // the patch AND far off it — never below the sphere at radius R centred a full Earth radius down.
        use crate::planet;
        let mats = materials::load();
        let w = generate(&mats);

        let radius = planet::earth().radius() as f32; // ≈6.371e6 m — the SAME body the space band draws
        // The patch centre's surface height in centered coords; the Earth centre is `radius` below it.
        let c = w.center();
        let surf_y = w
            .surface_top_voxel(c.x as i32, c.z as i32)
            .map(|t| t as f32 - c.y)
            .expect("solid centre column");
        let earth_center = Vec3::new(0.0, surf_y - radius, 0.0);
        let clearance = 2.0f32;

        // True below-the-surface test (f64: the metre-scale drop is below f32 precision at R≈6.4e6).
        let below_sphere = |eye: Vec3| -> bool {
            (eye.as_dvec3() - earth_center.as_dvec3()).length() < radius as f64
        };

        // GUARD (non-vacuous): an eye placed 5 km OFF the patch and 40 m below the local surface height.
        // The cap there has dropped only ~2 m, so this eye is well below the real sphere — exactly the
        // off-footprint case the old clamp let through.
        let off_patch_buried = Vec3::new(5000.0, surf_y - 40.0, 0.0);
        assert!(
            below_sphere(off_patch_buried),
            "test setup vacuous: the off-patch eye must actually start below the Earth sphere"
        );
        let fixed = w.clamp_eye_above_earth(off_patch_buried, earth_center, radius, clearance);
        assert!(
            !below_sphere(fixed),
            "clamp failed to lift an off-patch eye above the Earth sphere: {fixed:?}"
        );

        // The terrain orbit eye (exact `Engine::view_proj` construction, wasm-gated so replicated here).
        let max_dim = w.w.max(w.h).max(w.d) as f32;
        let base_distance = max_dim * 1.6;
        let eye_for = |yaw: f32, pitch: f32, zoom: f32| -> Vec3 {
            let cp = pitch.cos();
            let dir = Vec3::new(cp * yaw.sin(), pitch.sin(), cp * yaw.cos());
            dir * (base_distance * zoom)
        };

        // Sweep the full envelope; a steep downward pitch drives the raw eye below the sphere.
        let mut saw_below = false;
        let extra_lateral = [
            Vec3::new(9000.0, surf_y - 10.0, 0.0),
            Vec3::new(-12000.0, surf_y - 30.0, 6000.0),
            Vec3::new(0.0, surf_y - 200.0, 15000.0),
        ];
        for &yaw in &[0.0f32, 0.7, 1.5, 2.4, 3.14, 4.7, 6.0] {
            for &pitch in &[-1.5f32, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                for &zoom in &[0.2f32, 0.7, 1.0, 2.0, 4.0, 6.0] {
                    let raw = eye_for(yaw, pitch, zoom);
                    if below_sphere(raw) {
                        saw_below = true;
                    }
                    let clamped = w.clamp_eye_above_earth(raw, earth_center, radius, clearance);
                    assert!(
                        !below_sphere(clamped),
                        "clamped eye still below the Earth sphere at yaw={yaw} pitch={pitch} zoom={zoom}: {clamped:?}"
                    );
                }
            }
        }
        for &e in &extra_lateral {
            assert!(below_sphere(e), "lateral guard {e:?} must start below the sphere");
            let clamped = w.clamp_eye_above_earth(e, earth_center, radius, clearance);
            assert!(
                !below_sphere(clamped),
                "clamped lateral eye still below the Earth sphere: {clamped:?}"
            );
        }
        assert!(
            saw_below,
            "sweep must exercise below-sphere configs (else the sphere clamp is untested)"
        );
    }
}
