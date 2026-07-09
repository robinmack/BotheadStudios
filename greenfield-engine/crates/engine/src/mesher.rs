//! Face-culling voxel mesher.
//!
//! For every solid voxel we emit a quad on each face that borders air (or the world edge). Interior
//! faces are culled, so the vertex count tracks surface area, not volume. Each face is colored by
//! its voxel's material albedo, so the rock/dirt/grass layers are directly visible on the exposed
//! side walls. This is deliberately simple and robust for Phase 1; a smooth surface-nets mesher is
//! a planned upgrade (see `docs/07`/`docs/08`).

use crate::materials::Material;
use crate::world::World;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub col: [f32; 3],
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

pub fn build(world: &World, materials: &[Material]) -> Mesh {
    // Center the mesh on the origin so the orbit camera looks at the terrain's middle.
    let cx = world.w as f32 * 0.5;
    let cy = world.max_top as f32 * 0.5;
    let cz = world.d as f32 * 0.5;

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
