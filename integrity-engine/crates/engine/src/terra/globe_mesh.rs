//! docs/43 Phase 3 — a displaced cube-sphere globe mesh (the Google-Earth surface). Six cube faces, each a
//! res×res grid projected to the sphere, every vertex displaced radially by the sampled surface offset and
//! colored by its biome albedo. Normals are computed from the displaced grid so relief reads as shaded terrain.
//! Pure geometry (compiles native + wasm); emits the shared `mesher::Vertex` so it uses the space vertex layout.

use crate::mesher::{Mesh, Vertex};
use glam::{DVec3, Vec3};

/// (outward normal, right tangent, up tangent) for the 6 cube faces — one consistent basis per face.
const FACES: [([f64; 3], [f64; 3], [f64; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),  // +X
    ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),  // -X
    ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),  // +Y
    ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),  // -Y
    ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),   // +Z
    ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // -Z
];

/// Build the globe surface. `res` = grid points per cube-face edge; `r_disp` = the sphere radius in display
/// units; `sample(dir)` returns `(albedo, radius_offset_display, material_index)` for a unit surface direction — the caller
/// (Terra) reads the rasters there (land biome + elevation lift, or ocean-floor depth).
pub fn build_globe(res: usize, r_disp: f64, sample: impl Fn(DVec3) -> ([f32; 3], f64, u32)) -> Mesh {
    assert!(res >= 2);
    let mut vertices: Vec<Vertex> = Vec::with_capacity(6 * res * res);
    let mut indices: Vec<u32> = Vec::with_capacity(6 * (res - 1) * (res - 1) * 6);

    for (n, right, up) in FACES.iter() {
        let base = vertices.len() as u32;
        let (n, right, up) = (DVec3::from_array(*n), DVec3::from_array(*right), DVec3::from_array(*up));
        // Positions + directions for this face's grid.
        let mut pos = vec![Vec3::ZERO; res * res];
        let mut dirs = vec![DVec3::ZERO; res * res];
        let mut cols = vec![[0f32; 3]; res * res];
    // Which material each vertex is made OF — the shader picks its relief layer with it.
    let mut mats = vec![0u32; res * res];
        for j in 0..res {
            for i in 0..res {
                let u = -1.0 + 2.0 * i as f64 / (res - 1) as f64;
                let v = -1.0 + 2.0 * j as f64 / (res - 1) as f64;
                let dir = (n + right * u + up * v).normalize();
                let (col, off, mat) = sample(dir);
                let p = dir * (r_disp + off);
                dirs[j * res + i] = dir;
                pos[j * res + i] = Vec3::new(p.x as f32, p.y as f32, p.z as f32);
                cols[j * res + i] = col;
                mats[j * res + i] = mat;
            }
        }
        // Vertices — normals from central differences of the DISPLACED grid (relief shading); edges fall back
        // to the sphere normal. Flip to outward if the cross product points inward.
        for j in 0..res {
            for i in 0..res {
                let d = dirs[j * res + i];
                let sphere_n = Vec3::new(d.x as f32, d.y as f32, d.z as f32);
                let nrm = if i > 0 && i < res - 1 && j > 0 && j < res - 1 {
                    let du = pos[j * res + i + 1] - pos[j * res + i - 1];
                    let dv = pos[(j + 1) * res + i] - pos[(j - 1) * res + i];
                    let mut nn = du.cross(dv).normalize_or_zero();
                    if nn == Vec3::ZERO {
                        nn = sphere_n;
                    }
                    if nn.dot(sphere_n) < 0.0 {
                        nn = -nn;
                    }
                    nn
                } else {
                    sphere_n
                };
                vertices.push(Vertex {
                    pos: pos[j * res + i].to_array(),
                    nrm: nrm.to_array(),
                    col: cols[j * res + i],
                    mat: mats[j * res + i],
                });
            }
        }
        // Two triangles per grid quad, wound CCW as seen from outside (matches cull_mode Back).
        for j in 0..res - 1 {
            for i in 0..res - 1 {
                let a = base + (j * res + i) as u32;
                let b = base + (j * res + i + 1) as u32;
                let c = base + ((j + 1) * res + i + 1) as u32;
                let d = base + ((j + 1) * res + i) as u32;
                indices.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
    }
    Mesh { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globe_counts_and_indices_are_well_formed() {
        let res = 8;
        let m = build_globe(res, 1.0, |_| ([0.5, 0.5, 0.5], 0.0, 0));
        assert_eq!(m.vertices.len(), 6 * res * res, "6 faces × res² vertices");
        assert_eq!(m.indices.len(), 6 * (res - 1) * (res - 1) * 6, "2 tris × 3 idx per quad");
        let n = m.vertices.len() as u32;
        assert!(m.indices.iter().all(|&i| i < n), "every index references a real vertex");
    }

    #[test]
    fn undisplaced_globe_is_a_unit_sphere_with_outward_normals() {
        // A zero-offset sampler must yield a sphere of the given radius, with every normal pointing outward
        // (positive dot with the position) so the lit shader shades the day side, not the interior.
        let r = 2.5;
        let m = build_globe(6, r, |_| ([1.0, 0.0, 0.0], 0.0, 0));
        for v in &m.vertices {
            let p = Vec3::from_array(v.pos);
            assert!((p.length() - r as f32).abs() < 1e-4, "radius {} != {}", p.length(), r);
            let nrm = Vec3::from_array(v.nrm);
            assert!(nrm.dot(p.normalize()) > 0.5, "normal must point outward");
        }
    }

    #[test]
    fn displacement_pushes_vertices_outward_by_the_offset() {
        // A face-centre sample gets a positive offset; that vertex must sit at radius r + off along its dir.
        let m = build_globe(3, 1.0, |dir| {
            let off = if dir.x > 0.9 { 0.3 } else { 0.0 };
            ([0.0, 0.0, 0.0], off, 0)
        });
        let max_r = m.vertices.iter().map(|v| Vec3::from_array(v.pos).length()).fold(0.0f32, f32::max);
        assert!((max_r - 1.3).abs() < 1e-3, "displaced apex radius {max_r} != 1.3");
    }
}
