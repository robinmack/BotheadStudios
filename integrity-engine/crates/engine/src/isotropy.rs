//! Grid-isotropy regression suite — the guardrail behind `docs/15`.
//!
//! The voxel grid is a **sampling lattice, not a unit of matter** (`docs/15`). A regular Cartesian
//! lattice has preferred directions (its axes and 45° diagonals); the danger is that a solver or a
//! geometric operation silently bakes that bias into the *physics* — cracks that run along axes,
//! gravity that pulls harder toward a face than a corner, a "sphere" that is really a box. These
//! tests assert that the operations which touch the grid stay direction-independent, so a future
//! change that reintroduces lattice bias is caught rather than shipped (cf. the adversarial suite in
//! `docs/10`).
//!
//! Everything here is pure and native (no wgpu/wasm), per the project's TDD principle.

use crate::gravity::MassField;
use crate::materials;
use crate::world::World;
use glam::Vec3;

/// A cube world (`n`³ voxels) with a solid ball of `radius` (voxel units) of `mat` at its centre.
/// `n` is even and the ball is centred on the integer lattice point `n/2`, so the discretised set is
/// symmetric under every axis reflection and permutation — a genuinely spherically-symmetric mass.
fn ball_world(n: usize, radius: f32, mat: usize) -> World {
    assert!(
        n.is_multiple_of(2),
        "use an even dimension so the ball centres on a lattice point"
    );
    let c = (n / 2) as f32; // ball centre, an integer voxel coordinate
    let mut voxels = vec![0u16; n * n * n];
    for y in 0..n {
        for z in 0..n {
            for x in 0..n {
                let d2 = (x as f32 + 0.5 - c).powi(2)
                    + (y as f32 + 0.5 - c).powi(2)
                    + (z as f32 + 0.5 - c).powi(2);
                if d2 <= radius * radius {
                    voxels[(y * n + z) * n + x] = mat as u16 + 1;
                }
            }
        }
    }
    // max_top = n makes center() = (n/2, n/2, n/2): a symmetric frame.
    World::from_voxels(n, n, n, voxels, n, None)
}

/// A fully solid cube world of `mat` (`n`³). `max_top = n` puts the centre at the cube's middle, so
/// a dig at centered-coords origin lands on the symmetric lattice point `n/2`.
fn solid_world(n: usize, mat: usize) -> World {
    World::from_voxels(n, n, n, vec![mat as u16 + 1; n * n * n], n, None)
}

/// Unit sample directions: the six face axes plus the edge- and corner-diagonals. If the field were
/// biased toward axes vs. diagonals, these would disagree.
fn probe_dirs() -> Vec<Vec3> {
    let s2 = 1.0 / 2.0f32.sqrt();
    let s3 = 1.0 / 3.0f32.sqrt();
    vec![
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::new(s2, s2, 0.0),
        Vec3::new(s2, 0.0, s2),
        Vec3::new(0.0, s2, s2),
        Vec3::new(s3, s3, s3),
        Vec3::new(-s3, s3, -s3),
    ]
}

#[test]
fn gravity_is_direction_independent_for_a_symmetric_ball() {
    let mats = materials::load();
    let mat = materials::index_of(&mats, "granite");
    let (n, radius) = (40usize, 12.0f32);
    let world = ball_world(n, radius, mat);

    // block = 1: one mass point per voxel, so the *only* asymmetry a failure could expose is in the
    // solver itself, not in a coarse aggregation grid.
    let field = MassField::build(&world, &mats, 1);
    assert!(field.total_mass > 0.0);

    // Sample the exterior field at a fixed distance from the centre of mass in many directions.
    // Shell theorem: a spherically-symmetric mass has a perfectly radial, magnitude-equal exterior
    // field. The discretised ball only breaks that at the (cubic-symmetric) hexadecapole and higher,
    // which is well under a percent this far out — so real lattice bias would stand far above noise.
    let dist = radius * 3.0;
    let mags: Vec<f32> = probe_dirs()
        .iter()
        .map(|&dir| field.acceleration_at(field.com + dir * dist, 1.0).length())
        .collect();

    let max = mags.iter().cloned().fold(f32::MIN, f32::max);
    let min = mags.iter().cloned().fold(f32::MAX, f32::min);
    let mean = mags.iter().sum::<f32>() / mags.len() as f32;
    let spread = (max - min) / mean;
    assert!(
        spread < 0.01,
        "|g| must not depend on direction: spread {:.4} across {:?}",
        spread,
        mags
    );

    // And the field must point back along the sample ray (radial), with negligible tangential bias.
    for &dir in &probe_dirs() {
        let a = field.acceleration_at(field.com + dir * dist, 1.0);
        let radial = a.dot(-dir); // component pointing home
        let tangential = (a - (-dir) * radial).length();
        assert!(radial > 0.0, "gravity points toward the mass along {dir:?}");
        assert!(
            tangential / a.length() < 0.01,
            "tangential (sideways) component {:.4} of |g| along {:?}",
            tangential / a.length(),
            dir
        );
    }
}

#[test]
fn dig_carves_a_true_sphere_not_a_grid_biased_region() {
    let mats = materials::load();
    let soft = materials::index_of(&mats, "dirt");
    let n = 40usize;
    let mut world = solid_world(n, soft);

    let mut sim = crate::matter::MatterSim::new(200_000);
    let radius = 10.0f32;
    // Overwhelming tool strength so removal is purely geometric (every in-range voxel detaches),
    // isolating the shape of the carve from any material threshold.
    let removed = sim.dig(&mut world, &mats, Vec3::ZERO, radius, 1.0e12);

    // Count matches the Euclidean sphere's volume (a cube/Chebyshev region would be ~8/((4/3)π) ≈ 1.9×
    // larger; a Manhattan/octahedron ~2× smaller). ±8% covers the discretisation of a sphere this size.
    let expected = 4.0 / 3.0 * std::f32::consts::PI * radius.powi(3);
    let err = (removed as f32 - expected).abs() / expected;
    assert!(
        err < 0.08,
        "dig should remove ~a sphere's worth ({expected:.0}); removed {removed} (err {err:.3})"
    );

    // The carve reaches equally far on every axis — no lattice direction is preferred. Offsets are in
    // centered coords; the dig was at the origin, i.e. the cube's centre.
    let offs: Vec<Vec3> = sim.particles.iter().map(|p| p.pos).collect();
    let reach = |f: fn(Vec3) -> f32| {
        offs.iter()
            .cloned()
            .map(f)
            .fold(0.0f32, |m, v| m.max(v.abs()))
    };
    let (rx, ry, rz) = (reach(|o| o.x), reach(|o| o.y), reach(|o| o.z));
    assert!(
        (rx - ry).abs() <= 1.0 && (ry - rz).abs() <= 1.0 && (rx - rz).abs() <= 1.0,
        "carve must reach equally on each axis: x={rx} y={ry} z={rz}"
    );

    // Dynamics isotropy: ejected debris has no *lateral* (X/Z) bias — matter isn't flung preferentially
    // down an axis. (A deliberate +Y bias is physical, so Y is exempt.)
    let vels: Vec<Vec3> = sim.particles.iter().map(|p| p.vel).collect();
    let mean: Vec3 = vels.iter().copied().sum::<Vec3>() / vels.len() as f32;
    let mean_speed = vels.iter().map(|v| v.length()).sum::<f32>() / vels.len() as f32;
    assert!(
        mean.x.abs() / mean_speed < 0.05 && mean.z.abs() / mean_speed < 0.05,
        "no lateral ejection bias: mean lateral vel ({:.4}, {:.4}) vs mean speed {:.4}",
        mean.x,
        mean.z,
        mean_speed
    );
    assert!(mean.y > 0.0, "ejection keeps its intended upward bias");
}
