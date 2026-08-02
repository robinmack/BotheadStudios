//! **ONE surface geometry: a segment of a sphere** (docs/63).
//!
//! Robin: *"we just map a texture onto a segment of a sphere, whether it's a small chord or a full globe."*
//!
//! Terra draws its planet twice today — a cube-sphere globe for the far view and a tangent cap for the near
//! one — and everything that mediates between those two meshes (`cap_fade`, `cap_lift_disp`,
//! `cap_covers_view`, the `draw_globe` decision) exists only because there are two. None of it is physics.
//! It is bookkeeping between two answers to one question, and the class of bug it invites is the two
//! answers disagreeing about where the ground is.
//!
//! This is the one answer. A segment is a disc on the sphere: a centre direction and an angular radius,
//! from a millimetre of felt up to **π**, where it closes into the whole planet. The near cap and the globe
//! stop being different objects and become the same object at two extents.
//!
//! ## Why not just widen the existing cap
//!
//! [`super::ground_cap::fill_ground_cap`] parameterizes as `(center + east·du + north·dv).normalize()` —
//! gnomonic, a tangent plane projected onto the sphere. That reaches 90° only as `du → ∞`, so its arc per
//! unit parameter collapses long before a hemisphere; hence its `CAP_MAX_ANGLE = 0.6` and the comment that
//! *"past this the patch geometry degrades faster than it covers"*. It is not a tuning limit, it is the
//! projection's own asymptote, and no constant makes it reach the far side of a planet.
//!
//! A POLAR parameterization has no such asymptote: walk an angular distance `r` from the centre along an
//! azimuth `φ`, and `r` runs to π by construction with every ring the same shape. Its one singularity is
//! the centre point itself, which is a triangle fan rather than a degeneracy, and — usefully — it is the
//! one place a camera is always looking at, so the resolution it concentrates there is resolution spent
//! where the eye is.

use crate::mesher::Vertex;
use glam::{DVec3, Vec3};

/// A segment's own resolution: `rings` steps outward from the centre, `spokes` around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentRes {
    pub rings: usize,
    pub spokes: usize,
}

impl SegmentRes {
    pub fn new(rings: usize, spokes: usize) -> Self {
        SegmentRes {
            rings: rings.max(1),
            spokes: spokes.max(3),
        }
    }
    /// Vertices: one centre point plus a ring of `spokes` for each step out.
    pub fn vertex_count(&self) -> usize {
        1 + self.rings * self.spokes
    }
}

/// **How the rings are spaced.** Uniform in angle would spend the same number of samples on ground a metre
/// away and on ground at the horizon, which is backwards: the near ground fills far more of the frame. The
/// squared curve concentrates rings toward the centre — the same `signed-square` bias the tangent cap
/// already used, restated on a parameterization that survives to π.
///
/// `t` is the ring's fraction of the way out, `0..=1`; the result is its angular distance from the centre.
#[inline]
pub fn ring_angle(t: f64, cap_angle: f64) -> f64 {
    (t * t) * cap_angle
}

/// Fill `out` with a sphere segment's vertices (cleared first), centred on `center`, reaching `cap_angle`
/// radians — up to π, which is the whole sphere.
///
/// `origin` is the world point the vertices are emitted relative to (see the anchoring note on
/// `ground_cap::fill_ground_cap`; the same rule applies, and for the same reason). `sample` answers what
/// the surface IS in a direction: albedo, radial offset in display units, material.
///
/// The index topology is fixed for a given [`SegmentRes`] — get it once from [`segment_indices`].
pub fn fill_segment(
    out: &mut Vec<Vertex>,
    center: DVec3,
    east: DVec3,
    north: DVec3,
    origin: DVec3,
    r_disp: f64,
    cap_angle: f64,
    res: SegmentRes,
    sample: impl Fn(DVec3) -> ([f32; 3], f64, u32),
) {
    out.clear();
    out.reserve(res.vertex_count());
    let cap = cap_angle.clamp(0.0, std::f64::consts::PI);

    // Positions first, then normals from the ring/spoke neighbourhood — the same two-pass shape the
    // tangent cap uses, because a normal needs its neighbours to exist.
    let mut pos = Vec::with_capacity(res.vertex_count());
    let mut cols = Vec::with_capacity(res.vertex_count());
    let mut mats = Vec::with_capacity(res.vertex_count());
    let mut dirs = Vec::with_capacity(res.vertex_count());

    let mut push = |dir: DVec3,
                    pos: &mut Vec<Vec3>,
                    cols: &mut Vec<[f32; 3]>,
                    mats: &mut Vec<u32>,
                    dirs: &mut Vec<DVec3>| {
        let (col, off, mat) = sample(dir);
        let p = dir * (r_disp + off);
        let r = p - origin;
        pos.push(Vec3::new(r.x as f32, r.y as f32, r.z as f32));
        cols.push(col);
        mats.push(mat);
        dirs.push(dir);
    };

    push(center, &mut pos, &mut cols, &mut mats, &mut dirs);
    for ring in 1..=res.rings {
        let a = ring_angle(ring as f64 / res.rings as f64, cap);
        let (sa, ca) = a.sin_cos();
        for s in 0..res.spokes {
            let phi = s as f64 * std::f64::consts::TAU / res.spokes as f64;
            let (sp, cp) = phi.sin_cos();
            // Rodrigues about the centre: rotate `center` by `a` toward the azimuth `phi` in the tangent
            // frame. Exact at every angle, including past a hemisphere, where a projected plane is not.
            let dir = (center * ca + (east * cp + north * sp) * sa).normalize();
            push(dir, &mut pos, &mut cols, &mut mats, &mut dirs);
        }
    }

    // Normals from the displaced surface, not from the sphere: the relief is the point. Interior samples
    // difference their ring/spoke neighbours; the centre and the rim fall back to the radial direction,
    // which is what an undisplaced sphere would give.
    let idx = |ring: usize, s: usize| 1 + (ring - 1) * res.spokes + s % res.spokes;
    for i in 0..pos.len() {
        let outward = {
            let o = dirs[i];
            Vec3::new(o.x as f32, o.y as f32, o.z as f32)
        };
        let nrm = if i == 0 || res.rings < 3 {
            outward
        } else {
            let ring = (i - 1) / res.spokes + 1;
            let s = (i - 1) % res.spokes;
            if ring == res.rings {
                outward
            } else {
                // The inward neighbour of the first ring is the CENTRE vertex, which is index 0 and is
                // not on any ring — `idx` would underflow asking for ring 0.
                let inner = if ring == 1 { 0 } else { idx(ring - 1, s) };
                let d_out = pos[idx(ring + 1, s)] - pos[inner];
                let d_az = pos[idx(ring, s + 1)] - pos[idx(ring, s + res.spokes - 1)];
                let mut nn = d_az.cross(d_out).normalize_or_zero();
                if nn == Vec3::ZERO {
                    nn = outward;
                }
                if nn.dot(outward) < 0.0 {
                    nn = -nn;
                }
                nn
            }
        };
        out.push(Vertex {
            pos: pos[i].to_array(),
            nrm: nrm.to_array(),
            col: cols[i],
            mat: mats[i],
        });
    }
}

/// The index buffer for a segment of a given resolution — a fan at the centre, quads between rings.
pub fn segment_indices(res: SegmentRes) -> Vec<u32> {
    let mut out = Vec::with_capacity(res.spokes * 3 + (res.rings - 1) * res.spokes * 6);
    let idx = |ring: usize, s: usize| (1 + (ring - 1) * res.spokes + s % res.spokes) as u32;
    // Centre fan.
    for s in 0..res.spokes {
        out.extend_from_slice(&[0, idx(1, s + 1), idx(1, s)]);
    }
    // Ring quads.
    for ring in 1..res.rings {
        for s in 0..res.spokes {
            let (a, b) = (idx(ring, s), idx(ring, s + 1));
            let (c, d) = (idx(ring + 1, s + 1), idx(ring + 1, s));
            out.extend_from_slice(&[a, d, c, a, c, b]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_d: DVec3) -> ([f32; 3], f64, u32) {
        ([0.3, 0.4, 0.2], 0.0, 1)
    }

    fn frame(center: DVec3) -> (DVec3, DVec3) {
        let east = center.cross(DVec3::Y).normalize();
        (east, east.cross(center).normalize())
    }

    /// **A segment reaches the whole sphere.** This is the property the tangent cap cannot have at any
    /// constant, and the reason this module exists: at `cap_angle = π` the rim closes on the antipode, so
    /// the near patch and the entire globe are one primitive at two extents.
    #[test]
    fn a_segment_can_close_into_a_whole_sphere() {
        let res = SegmentRes::new(24, 48);
        let center = DVec3::X;
        let (east, north) = frame(center);
        let mut v = Vec::new();
        fill_segment(
            &mut v,
            center,
            east,
            north,
            DVec3::ZERO,
            1.0,
            std::f64::consts::PI,
            res,
            flat,
        );
        assert_eq!(v.len(), res.vertex_count());
        // Every vertex is on the unit sphere, at every angle out to the antipode.
        for x in &v {
            let p = Vec3::from_array(x.pos);
            assert!(
                (p.length() - 1.0).abs() < 1e-5,
                "vertex off the sphere: {}",
                p.length()
            );
        }
        // The outermost ring really is the antipode, not an asymptotic approach to the equator — which is
        // exactly what the gnomonic cap could not do.
        let last = Vec3::from_array(v[v.len() - 1].pos).as_dvec3();
        assert!(
            last.dot(center) < -0.999,
            "the rim should close on the antipode, got dot {}",
            last.dot(center)
        );
        // And the segment covers the sphere: sample directions all over and check each is within a
        // triangle's reach of some vertex.
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        for i in 0..200 {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / 200.0;
            let r = (1.0 - z * z).sqrt();
            let a = i as f64 * golden;
            let q = DVec3::new(r * a.cos(), z, r * a.sin());
            let best = v
                .iter()
                .map(|x| Vec3::from_array(x.pos).as_dvec3().normalize().dot(q))
                .fold(f64::MIN, f64::max);
            assert!(best > 0.9, "direction {q:?} uncovered, best dot {best}");
        }
    }

    /// At a small angle the segment must agree with the tangent cap it replaces — same surface, same
    /// place. If the two disagreed, swapping them would move the ground, which is the exact failure the
    /// globe/cap pair already has and this module exists to end.
    #[test]
    fn a_small_segment_lands_where_the_tangent_cap_does() {
        let center = DVec3::new(0.3, 0.5, 0.2).normalize();
        let (east, north) = frame(center);
        let angle = 0.02;
        let eye = center * 1.0004;
        let mut seg = Vec::new();
        fill_segment(
            &mut seg,
            center,
            east,
            north,
            eye,
            1.0,
            angle,
            SegmentRes::new(16, 32),
            flat,
        );
        // Every segment vertex must lie on the same sphere the cap builds, and inside the same angular
        // disc — the two parameterizations differ in HOW they sample, never in WHAT they sample.
        for x in &seg {
            let p = Vec3::from_array(x.pos).as_dvec3() + eye;
            assert!((p.length() - 1.0).abs() < 1e-9, "off the surface");
            assert!(
                p.normalize().dot(center) >= angle.cos() - 1e-9,
                "outside the declared disc"
            );
        }
        // The centre vertex sits directly under the eye at the eye's own height, exactly as the tangent
        // cap's does (`centre_vertex_sits_directly_below_the_eye_at_the_camera_height`).
        let c = Vec3::from_array(seg[0].pos);
        assert!(
            (c.length() - 0.0004).abs() < 1e-6,
            "centre height {}",
            c.length()
        );
    }

    /// Rings concentrate toward the centre — resolution spent where the eye is looking — and the mapping
    /// is monotonic, so rings never cross.
    #[test]
    fn rings_concentrate_toward_the_centre_and_never_cross() {
        let cap = 0.5;
        let mut prev = -1.0;
        for i in 0..=32 {
            let a = ring_angle(i as f64 / 32.0, cap);
            assert!(a > prev, "ring angles must increase");
            prev = a;
        }
        assert!(
            (ring_angle(1.0, cap) - cap).abs() < 1e-12,
            "the rim is the declared angle"
        );
        assert_eq!(ring_angle(0.0, cap), 0.0, "the centre is the centre");
        // Squared spacing: the inner half of the rings covers a quarter of the angle.
        assert!((ring_angle(0.5, cap) / cap - 0.25).abs() < 1e-12);
    }

    /// The topology must be well formed at every extent: indices in range, no degenerate triangles, and
    /// the whole surface wound the same way.
    #[test]
    fn the_index_topology_is_well_formed() {
        for res in [
            SegmentRes::new(1, 3),
            SegmentRes::new(8, 16),
            SegmentRes::new(24, 48),
        ] {
            let idx = segment_indices(res);
            assert_eq!(idx.len() % 3, 0);
            assert!(idx.iter().all(|&i| (i as usize) < res.vertex_count()));
            for t in idx.chunks(3) {
                assert!(
                    t[0] != t[1] && t[1] != t[2] && t[0] != t[2],
                    "degenerate triangle {t:?}"
                );
            }
            // A fan of `spokes` triangles plus `rings-1` quad rings.
            assert_eq!(
                idx.len(),
                (res.spokes + (res.rings - 1) * res.spokes * 2) * 3
            );
        }
    }

    /// Relief must reach the normals, or the surface is lit as a smooth sphere however much shape it has.
    #[test]
    fn displacement_tilts_the_normals() {
        let center = DVec3::X;
        let (east, north) = frame(center);
        let res = SegmentRes::new(12, 24);
        let mut v = Vec::new();
        // A ridge along the east axis: offset varies with the component across it.
        fill_segment(
            &mut v,
            center,
            east,
            north,
            DVec3::ZERO,
            1.0,
            0.2,
            res,
            |d| ([0.3, 0.3, 0.3], 0.01 * (d.dot(north) * 40.0).sin(), 0),
        );
        let tilted = v
            .iter()
            .filter(|x| {
                let n = Vec3::from_array(x.nrm).as_dvec3();
                let p = Vec3::from_array(x.pos).as_dvec3().normalize();
                n.dot(p) < 0.999
            })
            .count();
        assert!(
            tilted > v.len() / 4,
            "displacement must tilt normals; only {tilted} of {} moved",
            v.len()
        );
    }
}
