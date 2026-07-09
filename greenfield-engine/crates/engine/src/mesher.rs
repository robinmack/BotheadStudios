//! Voxel meshers.
//!
//! `build_surface_nets` (Phase 6) produces the **smooth** terrain the renderer uses — the same voxel
//! occupancy field meshed as a rounded surface with smooth normals. `build` is a simple blocky
//! face-culling mesher kept as a reference/fallback. Also here: `build_cube` (debris) and
//! `build_uv_sphere` (probe). All emit the same `Vertex` (position, normal, color, material id), so
//! they share one pipeline and the triplanar texturing.

use crate::materials::Material;
use crate::world::World;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub col: [f32; 3],
    /// Material index — the layer to sample in the procedural texture array (Phase 4).
    pub mat: u32,
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// One cube face: neighbor offset to test for exposure, outward normal, and 4 corner offsets.
type Face = ([i32; 3], [f32; 3], [[f32; 3]; 4]);

/// The six face directions. Corners are unit-cube offsets added to the voxel's minimum corner.
const FACES: [Face; 6] = [
    // +X
    (
        [1, 0, 0],
        [1.0, 0.0, 0.0],
        [
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
    ),
    // -X
    (
        [-1, 0, 0],
        [-1.0, 0.0, 0.0],
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
    ),
    // +Y (top)
    (
        [0, 1, 0],
        [0.0, 1.0, 0.0],
        [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
    ),
    // -Y (bottom)
    (
        [0, -1, 0],
        [0.0, -1.0, 0.0],
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
    ),
    // +Z
    (
        [0, 0, 1],
        [0.0, 0.0, 1.0],
        [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
    ),
    // -Z
    (
        [0, 0, -1],
        [0.0, 0.0, -1.0],
        [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
    ),
];

/// Blocky face-culling mesher — kept as a simple, robust reference/fallback. The renderer now uses
/// `build_surface_nets` for smooth terrain (Phase 6).
#[allow(dead_code)]
pub fn build(world: &World, materials: &[Material]) -> Mesh {
    // Center the mesh on the origin so the orbit camera looks at the terrain's middle.
    let c = world.center();
    let (cx, cy, cz) = (c.x, c.y, c.z);

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for y in 0..world.h {
        for z in 0..world.d {
            for x in 0..world.w {
                let mat = match world.material_at(x as i32, y as i32, z as i32) {
                    Some(m) => m,
                    None => continue,
                };
                let color = shade(materials[mat].albedo, x, y, z);

                for (offset, normal, corners) in FACES.iter() {
                    let nx = x as i32 + offset[0];
                    let ny = y as i32 + offset[1];
                    let nz = z as i32 + offset[2];
                    if world.is_solid(nx, ny, nz) {
                        continue; // face is buried
                    }
                    let base = vertices.len() as u32;
                    for c in corners.iter() {
                        vertices.push(Vertex {
                            pos: [
                                x as f32 + c[0] - cx,
                                y as f32 + c[1] - cy,
                                z as f32 + c[2] - cz,
                            ],
                            nrm: *normal,
                            col: color,
                            mat: mat as u32,
                        });
                    }
                    indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
        }
    }

    Mesh { vertices, indices }
}

/// Build a unit-normal UV sphere mesh of the given radius and color, centered at its local origin.
/// Placed in the world via a model matrix at draw time. Used for the dropped probe (Phase 2).
pub fn build_uv_sphere(
    radius: f32,
    mat: u32,
    color: [f32; 3],
    rings: usize,
    sectors: usize,
) -> Mesh {
    use std::f32::consts::{PI, TAU};
    let mut vertices: Vec<Vertex> = Vec::new();
    for i in 0..=rings {
        let phi = (i as f32 / rings as f32) * PI; // 0..PI (pole to pole)
        for j in 0..=sectors {
            let theta = (j as f32 / sectors as f32) * TAU; // 0..2PI (around)
            let n = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            vertices.push(Vertex {
                pos: [n[0] * radius, n[1] * radius, n[2] * radius],
                nrm: n,
                col: color,
                mat,
            });
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    let stride = (sectors + 1) as u32;
    for i in 0..rings as u32 {
        for j in 0..sectors as u32 {
            let a = i * stride + j;
            let b = a + stride;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh { vertices, indices }
}

/// Build a small cube mesh centered on its local origin (half-extent `half`), colored `color`.
/// Used as the instanced base mesh for debris particles (Phase 3); the per-instance offset places
/// each copy, so `color` here is just a fallback.
pub fn build_cube(half: f32, color: [f32; 3]) -> Mesh {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for (_, normal, corners) in FACES.iter() {
        let base = vertices.len() as u32;
        for c in corners.iter() {
            vertices.push(Vertex {
                pos: [
                    (c[0] * 2.0 - 1.0) * half,
                    (c[1] * 2.0 - 1.0) * half,
                    (c[2] * 2.0 - 1.0) * half,
                ],
                nrm: *normal,
                col: color,
                mat: 0,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh { vertices, indices }
}

/// Smooth terrain mesh via **Surface Nets** (Phase 6): the same voxel occupancy field is meshed as
/// a smooth surface with smooth normals, instead of stair-stepped cubes. Physics is unchanged — this
/// is purely the visual representation of the matter store. Each vertex is tagged with the nearest
/// solid voxel's material so triplanar texturing (Phase 4) still applies.
pub fn build_surface_nets(world: &World, materials: &[Material]) -> Mesh {
    use fast_surface_nets::{surface_nets, SurfaceNetsBuffer};
    use ndshape::{ConstShape, ConstShape3u32};

    // Padded by one cell on every side so the outer boundary (solid voxel ↔ outside air) is meshed.
    const PX: u32 = crate::world::W as u32 + 2;
    const PY: u32 = crate::world::H as u32 + 2;
    const PZ: u32 = crate::world::D as u32 + 2;
    type Shape = ConstShape3u32<PX, PY, PZ>;

    // Signed field: negative inside solid, positive in air (surface at 0). Padding stays air.
    let mut sdf = vec![1.0f32; (PX * PY * PZ) as usize];
    for y in 0..world.h {
        for z in 0..world.d {
            for x in 0..world.w {
                if world.is_solid(x as i32, y as i32, z as i32) {
                    let i = Shape::linearize([x as u32 + 1, y as u32 + 1, z as u32 + 1]) as usize;
                    sdf[i] = -1.0;
                }
            }
        }
    }

    let mut buffer = SurfaceNetsBuffer::default();
    surface_nets(
        &sdf,
        &Shape {},
        [0; 3],
        [PX - 1, PY - 1, PZ - 1],
        &mut buffer,
    );

    // Recompute smooth normals from the meshed geometry (the binary field's own gradient is blocky),
    // oriented to agree with the surface-nets outward normal so lighting isn't inverted.
    use glam::Vec3;
    let mut accum = vec![Vec3::ZERO; buffer.positions.len()];
    for tri in buffer.indices.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let a = Vec3::from(buffer.positions[ia]);
        let b = Vec3::from(buffer.positions[ib]);
        let c = Vec3::from(buffer.positions[ic]);
        let face = (b - a).cross(c - a); // area-weighted, not normalized
        accum[ia] += face;
        accum[ib] += face;
        accum[ic] += face;
    }

    let center = world.center();
    let mut vertices = Vec::with_capacity(buffer.positions.len());
    for (i, p) in buffer.positions.iter().enumerate() {
        // Padded coords → voxel coords → centered world coords (matching the other meshes).
        let (wx, wy, wz) = (p[0] - 1.0, p[1] - 1.0, p[2] - 1.0);
        let mat = nearest_material(world, wx, wy, wz);
        let smooth = accum[i].normalize_or_zero();
        let reference = Vec3::from(buffer.normals[i]);
        let n = if smooth.dot(reference) < 0.0 {
            -smooth
        } else {
            smooth
        };
        vertices.push(Vertex {
            pos: [wx - center.x, wy - center.y, wz - center.z],
            nrm: [n.x, n.y, n.z],
            col: materials[mat].albedo,
            mat: mat as u32,
        });
    }
    Mesh {
        vertices,
        indices: buffer.indices,
    }
}

/// Material of the solid voxel nearest to a (boundary) point, for coloring a surface-nets vertex.
fn nearest_material(world: &World, wx: f32, wy: f32, wz: f32) -> usize {
    let (bx, by, bz) = (wx.round() as i32, wy.round() as i32, wz.round() as i32);
    let mut best = 0usize;
    let mut best_d = f32::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if let Some(m) = world.material_at(x, y, z) {
                    let d = (x as f32 + 0.5 - wx).powi(2)
                        + (y as f32 + 0.5 - wy).powi(2)
                        + (z as f32 + 0.5 - wz).powi(2);
                    if d < best_d {
                        best_d = d;
                        best = m;
                    }
                }
            }
        }
    }
    best
}

/// A little deterministic per-voxel brightness jitter so large flat material regions get subtle
/// variation instead of reading as a single poster color — a first hint of "grain" before real
/// procedural texturing (docs/06).
fn shade(albedo: [f32; 3], x: usize, y: usize, z: usize) -> [f32; 3] {
    let mut h = (x as u32)
        .wrapping_mul(2_654_435_761)
        .wrapping_add((y as u32).wrapping_mul(40_503))
        .wrapping_add((z as u32).wrapping_mul(668_265_263));
    h ^= h >> 15;
    let jitter = 0.90 + 0.20 * ((h & 0xffff) as f32 / 65535.0); // 0.90..1.10
    [
        (albedo[0] * jitter).clamp(0.0, 1.0),
        (albedo[1] * jitter).clamp(0.0, 1.0),
        (albedo[2] * jitter).clamp(0.0, 1.0),
    ]
}
