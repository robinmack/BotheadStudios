//! docs/43 Phase 5 — the fine ground cap: a high-resolution local patch of the surface under the camera, sampling
//! REAL elevation and curving to a true horizon. It is built CAMERA-RELATIVE — every position is `surface - eye`
//! in display units, computed in f64 and cast to f32 only at the end — so ground-level detail survives the
//! radius-1 globe (raw f32 at Earth's radius has ~0.6 m ULP). The caller draws it with a rotation-only view (the
//! eye sitting at the origin) and cross-fades it against the coarse globe by altitude. Pure geometry (native+wasm).
//!
//! This is the plan's low-risk first LOD step (a single tangent cap). The screen-space-error quadtree with
//! geomorphing + edge skirts, and sub-raster fbm micro-detail, are the follow-on refinements.

use crate::mesher::Vertex;
use glam::{DVec3, Vec3};

/// Fill `out` with the ground-cap vertices (cleared first). The index topology is fixed for a given `res` — get
/// it once from [`cap_indices`] — so this is called every frame to rewrite only the vertex buffer.
/// - `center`,`east`,`north`: the unit surface direction under the camera and its tangent frame.
/// - `eye`: camera position in display units (the patch is emitted relative to this).
/// - `r_disp`: planet radius in display units.
/// - `cap_angle`: angular radius (radians) the patch spans from `center` — size it to ~the horizon.
/// - `res`: grid points per side.
/// - `sample(dir) -> (albedo, radius_offset_display, material_index)`: Terra reads the rasters (real elevation × the declared
///   exaggeration, biome albedo) for a surface direction.
#[allow(clippy::too_many_arguments)]
pub fn fill_ground_cap(
    out: &mut Vec<Vertex>,
    center: DVec3,
    east: DVec3,
    north: DVec3,
    eye: DVec3,
    r_disp: f64,
    cap_angle: f64,
    res: usize,
    sample: impl Fn(DVec3) -> ([f32; 3], f64, u32),
) {
    assert!(res >= 2);
    out.clear();
    out.reserve(res * res);

    let mut rel = vec![Vec3::ZERO; res * res]; // positions relative to the eye (display units)
    let mut cols = vec![[0f32; 3]; res * res];
    // The material each cap vertex is made OF — the shader needs it to pick the right relief layer.
    let mut mats = vec![0u32; res * res];
    for j in 0..res {
        for i in 0..res {
            // Angular offsets east/north in [-cap_angle, cap_angle]; a denser-toward-centre curve (u³) keeps
            // resolution high near the camera while still reaching the horizon at the edges.
            let su = -1.0 + 2.0 * i as f64 / (res - 1) as f64;
            let sv = -1.0 + 2.0 * j as f64 / (res - 1) as f64;
            let du = su * su.abs() * cap_angle; // signed-square: |curve| toward centre
            let dv = sv * sv.abs() * cap_angle;
            let dir = (center + east * du + north * dv).normalize();
            let (col, off, mat) = sample(dir);
            let p = dir * (r_disp + off);
            let r = p - eye;
            rel[j * res + i] = Vec3::new(r.x as f32, r.y as f32, r.z as f32);
            cols[j * res + i] = col;
            mats[j * res + i] = mat;
        }
    }

    for j in 0..res {
        for i in 0..res {
            // Normals from central differences of the displaced patch (translation-invariant, so eye-relative is
            // fine); edges fall back to the outward (radial) direction.
            let outward = {
                let p = rel[j * res + i].as_dvec3() + eye;
                let o = p.normalize_or_zero();
                Vec3::new(o.x as f32, o.y as f32, o.z as f32)
            };
            let nrm = if i > 0 && i < res - 1 && j > 0 && j < res - 1 {
                let dU = rel[j * res + i + 1] - rel[j * res + i - 1];
                let dV = rel[(j + 1) * res + i] - rel[(j - 1) * res + i];
                let mut nn = dU.cross(dV).normalize_or_zero();
                if nn == Vec3::ZERO {
                    nn = outward;
                }
                if nn.dot(outward) < 0.0 {
                    nn = -nn;
                }
                nn
            } else {
                outward
            };
            out.push(Vertex { pos: rel[j * res + i].to_array(), nrm: nrm.to_array(), col: cols[j * res + i], mat: mats[j * res + i] });
        }
    }
}

/// The index buffer for a cap of a given `res` (topology is fixed, so the caller builds it once and only rewrites
/// vertices each frame).
pub fn cap_indices(res: usize) -> Vec<u32> {
    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let a = (j * res + i) as u32;
            let b = (j * res + i + 1) as u32;
            let c = ((j + 1) * res + i + 1) as u32;
            let d = ((j + 1) * res + i) as u32;
            indices.extend_from_slice(&[a, c, b, a, d, c]);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_counts_and_indices_are_well_formed() {
        let res = 16;
        let center = DVec3::X;
        let (east, north) = (DVec3::new(0.0, 0.0, -1.0), DVec3::Y);
        let eye = center * 1.001; // 0.001 display units up
        let mut v = Vec::new();
        fill_ground_cap(&mut v, center, east, north, eye, 1.0, 0.02, res, |_| ([0.3, 0.4, 0.5], 0.0, 0));
        assert_eq!(v.len(), res * res);
        let idx = cap_indices(res);
        assert_eq!(idx.len(), (res - 1) * (res - 1) * 6);
        assert!(idx.iter().all(|&i| (i as usize) < v.len()));
    }

    #[test]
    fn centre_vertex_sits_directly_below_the_eye_at_the_camera_height() {
        // Flat sampler, eye 0.004 above the centre point → the centre vertex is the eye-relative surface point,
        // i.e. length ≈ the eye height, pointing down (−center).
        let res = 9; // odd so the centre grid point is exactly at (mid, mid)
        let center = DVec3::new(0.3, 0.5, 0.2).normalize();
        let up = center;
        let east = up.cross(DVec3::Y).normalize();
        let north = east.cross(up).normalize();
        let h = 0.004;
        let eye = center * (1.0 + h);
        let mut v = Vec::new();
        fill_ground_cap(&mut v, center, east, north, eye, 1.0, 0.02, res, |_| ([0.0, 0.0, 0.0], 0.0, 0));
        let mid = (res / 2) * res + res / 2;
        let c = v[mid].pos;
        let cv = Vec3::from_array(c);
        assert!((cv.length() - h as f32).abs() < 1e-5, "centre vertex height {} != {}", cv.length(), h);
        let down = Vec3::new(-center.x as f32, -center.y as f32, -center.z as f32);
        assert!(cv.normalize().dot(down) > 0.999, "centre vertex points straight down from the eye");
    }
}
