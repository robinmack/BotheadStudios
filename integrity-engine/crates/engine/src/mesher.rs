//! Voxel meshers.
//!
//! `build_surface_nets` (Phase 6) produces the **smooth** terrain the renderer uses — the same voxel
//! occupancy field meshed as a rounded surface with smooth normals. `build` is a simple blocky
//! face-culling mesher kept as a reference/fallback. Also here: `build_cube` (debris) and
//! `build_uv_sphere` (probe). All emit the same `Vertex` (position, normal, color, material id), so
//! they share one pipeline and the triplanar texturing.

use crate::materials::{index_of, Material};
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
                    if world.material_at(nx, ny, nz).is_some() {
                        continue; // face is buried behind matter (solid or water)
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
            // CCW seen from OUTSIDE (the space pipeline culls back faces): the old CW winding made
            // every sphere render inside-out — the near hemisphere was culled and you saw the lit
            // INNER far wall, so bodies appeared reverse-lit (Earth "brilliantly lit" when backlit).
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    Mesh { vertices, indices }
}

/// Build the REAL bulk-Earth surface as a curved CAP that FOLLOWS the same terrain relief as the voxel
/// patch (NOT a flat decorative plane, and NOT a smooth shelf pinned at one height above the valleys):
/// a tessellated polar disk that samples the SHARED [`crate::world::terrain_height`] at every vertex AND
/// curves DOWN to a finite (~km) horizon — the honest replacement for a flat ground skirt whose horizon
/// would be at infinity, and the fix for the flat-cap-above-a-valley step that made resting rubble read
/// as hovering.
///
/// Geometry (centered coords; `center` is the world's `center()`): "down" (−Y under the uniform surface
/// gravity) is toward the Earth's centre. A cap vertex at centered `(cx, cz)` — horizontal distance `d`
/// from the patch centre — sits at
///   `y = terrain_height(cx + center.x, cz + center.z) − center.y  −  d²/(2·radius)`
/// i.e. the SAME continuous heightfield the patch fills to (converted centered→voxel frame), MINUS the
/// sphere's parabolic curvature drop `d²/(2R)` (accurate to <1 m over the visible ~km, cheaper than
/// trig). Near the patch this equals the patch surface exactly — continuous, no step. Far out it is the
/// same field sampled coarsely by the geometric ring spacing (OPTIMIZATION, not a fudge — the model is
/// authoritative, only the far sampling is coarse). `earth_c` is a full radius below the patch-centre
/// surface height, so the surface-gravity "down" and the cap's fall-away agree.
///
/// The per-vertex normal is the TRUE normal of the surface it follows — `normalize(−∂h/∂x, 1, −∂h/∂z)`
/// of the shared height `h = cap_y` (relief PLUS the sphere's curvature) by central difference — so the
/// distant terrain shades as real rolling hills (not a flat sheet) and joins the voxel patch's shading
/// without a tonal seam, while the planetary curvature (`∂/∂d = d/R`) still tilts the far ground away
/// from the sun. Tessellation is dense near the patch (where the eye is) and coarser toward the horizon.
///
/// BULK EVERYWHERE by default (`hole = None`): the cap is a FULL polar disk that samples the shared
/// `terrain_height` from its very centre outward, so the DEFAULT terrain is one continuous smooth bulk
/// surface over the whole footprint and beyond — no finite voxel block, nothing special about any square
/// (Robin: "dissolve the fixed cube"). Near the centre the rings are fine (≈1.5 m radial, ≈few-m arc), so
/// the bulk cap resolves the rolling relief where the eye is; far out they grow geometrically to the
/// horizon (that coarse far sampling is OPTIMIZATION of the one authoritative heightfield, not a fudge).
///
/// ON-DEMAND HOLE (`hole = Some(half_extents)`): when a region has been RESOLVED into voxels (an active
/// impact/dig), the cap leaves a square HOLE of those half-extents (centred on the origin) so the resolved
/// voxels — the crater bowl, its walls, exposed strata — render there instead of being hidden under a cap
/// lid, while the rest of the world stays bulk. The seam ring sits on the hole boundary at the shared
/// `terrain_height`, so it joins the resolved patch edge continuously (no step, no gap → one surface).
pub fn build_earth_cap(
    materials: &[Material],
    center: glam::Vec3,
    radius: f32,
    r_max: f32,
    hole: Option<glam::Vec2>,
    field: Option<&crate::world::World>,
) -> Mesh {
    use crate::world::terrain_height;
    use std::f32::consts::TAU;
    let grass = index_of(materials, "grass"); // the SAME surface skin the voxel patch wears
    let col = materials[grass].albedo;
    // Surface height (centered coords) at cap point (cx, cz): shared relief PLUS the persistent T0
    // displacement, minus the sphere drop.
    //
    // The displacement term is what makes a de-resolved crater VISIBLE. Without it the cap redraws
    // pristine procedural relief, so a column demoted to T0 is physically correct under the probe and
    // the grains — both read `ground_top_voxel` — while rendering as untouched ground. That is the
    // render disagreeing with the physics, which is the one direction this engine never allows
    // (docs/46: physics drives the render). `None` keeps the pure-procedural cap for callers with no
    // world to sample, and an untouched world's displacement is all zeros, so nothing moves until
    // something is actually baked back.
    let cap_y = |cx: f32, cz: f32| -> f32 {
        let d = (cx * cx + cz * cz).sqrt();
        let disp = field.map_or(0.0, |w| w.displacement_at(cx + center.x, cz + center.z));
        terrain_height(cx + center.x, cz + center.z) + disp - center.y - d * d / (2.0 * radius)
    };
    // True surface normal of y = cap_y(x, z): central difference of the shared heightfield (relief +
    // curvature), so the cap shades as real rolling hills that join the patch without a seam.
    let cap_nrm = |cx: f32, cz: f32| -> glam::Vec3 {
        const E: f32 = 1.0; // 1 m finite-difference step
        let dh_dx = (cap_y(cx + E, cz) - cap_y(cx - E, cz)) / (2.0 * E);
        let dh_dz = (cap_y(cx, cz + E) - cap_y(cx, cz - E)) / (2.0 * E);
        glam::Vec3::new(-dh_dx, 1.0, -dh_dz).normalize()
    };

    const SEG: usize = 160; // angular segments — smooth horizon AND fine bulk sampling near the centre
    let angle = |s: usize| s as f32 / SEG as f32 * TAU;
    let vert = |x: f32, z: f32| -> Vertex {
        let pos = glam::Vec3::new(x, cap_y(x, z), z);
        Vertex {
            pos: pos.into(),
            nrm: cap_nrm(x, z).into(),
            col,
            mat: grass as u32,
        }
    };

    // The list of ring radii, and (for the hole case) an inner SQUARE seam ring on the hole boundary.
    // A ray at angle `a` meets a square of half-extents (hx, hz) at `min(hx/|cos a|, hz/|sin a|)`.
    let square_r = |hx: f32, hz: f32, a: f32| -> f32 {
        let (ca, sa) = (a.cos().abs(), a.sin().abs());
        let tx = if ca > 1e-6 { hx / ca } else { f32::INFINITY };
        let tz = if sa > 1e-6 { hz / sa } else { f32::INFINITY };
        tx.min(tz)
    };

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // `rings[k]` gives the per-segment radius of ring k; consecutive rings are quad-stripped together.
    let mut rings: Vec<Vec<f32>> = Vec::new();
    let mut has_center = false;

    match hole {
        Some(h) => {
            // Ring 0 = the SQUARE seam on the hole boundary (per-angle radius). Circular rings begin just
            // outside the square's far corner so no ring ever dips into the hole, then grow to the horizon.
            rings.push((0..SEG).map(|s| square_r(h.x, h.y, angle(s))).collect());
            let mut r = (h.x * h.x + h.y * h.y).sqrt() + 1.0;
            let mut step = 8.0f32;
            while r < r_max {
                rings.push(vec![r; SEG]);
                step *= 1.12;
                r += step;
            }
            if rings.last().map(|ring| ring[0]).unwrap_or(0.0) < r_max {
                rings.push(vec![r_max; SEG]);
            }
        }
        None => {
            // FULL DISK: a centre vertex, then fine rings out to the footprint half-diagonal, growing
            // geometrically to the horizon. The bulk terrain covers everything — no hole, no cube.
            has_center = true;
            let fp = (center.x * center.x + center.z * center.z).sqrt(); // footprint half-diagonal
            let mut r = 0.0f32;
            let mut step = 1.5f32;
            loop {
                r += step;
                if r >= r_max {
                    rings.push(vec![r_max; SEG]);
                    break;
                }
                rings.push(vec![r; SEG]);
                if r > fp {
                    step *= 1.12; // coarsen only past the footprint (the near field stays fine)
                }
            }
        }
    }

    // Emit vertices: optional centre, then each ring's SEG points.
    if has_center {
        vertices.push(vert(0.0, 0.0)); // index 0
        // Triangle fan from the centre to ring 0.
        for s in 0..SEG {
            let s1 = (s + 1) % SEG;
            indices.extend_from_slice(&[0, 1 + s as u32, 1 + s1 as u32]);
        }
    }
    let base0 = vertices.len() as u32; // first ring's base index
    for ring in &rings {
        for (s, &rr) in ring.iter().enumerate() {
            let a = angle(s);
            vertices.push(vert(rr * a.cos(), rr * a.sin()));
        }
    }
    // Quad strips between successive rings. All rings are SEG-aligned by angle.
    for k in 0..rings.len().saturating_sub(1) {
        let base_in = base0 + (k * SEG) as u32;
        let base_out = base0 + ((k + 1) * SEG) as u32;
        for s in 0..SEG {
            let s1 = ((s + 1) % SEG) as u32;
            let s = s as u32;
            let i0 = base_in + s;
            let i1 = base_in + s1;
            let o0 = base_out + s;
            let o1 = base_out + s1;
            indices.extend_from_slice(&[i0, o0, o1, i0, o1, i1]);
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

    // Padded by TWO cells on every side. The boundary walls must sit at least one cell inside the
    // grid, or Surface Nets can't form the closing quads there and the mesh has holes ("hollow").
    const PAD: u32 = 2;
    const PX: u32 = crate::world::W as u32 + 2 * PAD;
    const PY: u32 = crate::world::H as u32 + 2 * PAD;
    const PZ: u32 = crate::world::D as u32 + 2 * PAD;
    type Shape = ConstShape3u32<PX, PY, PZ>;

    // Occupancy field (1 = solid, 0 = air) in the padded grid, then smoothed so Surface Nets places
    // the surface along a real gradient. A binary ±1 field only "erodes" cubes; a smoothed field
    // genuinely rounds the geometry and gives consistently-outward normals.
    let n = (PX * PY * PZ) as usize;
    let mut occ = vec![0.0f32; n];
    for y in 0..world.h {
        for z in 0..world.d {
            for x in 0..world.w {
                // SOLID ground only — the smooth land surface. The sea is meshed separately as a flat
                // surface at the waterline (see [`append_sea_surface`]), so its mirror-flat top reads as
                // water and catches the specular sun-glint instead of being smoothed into the hills.
                if world.is_solid(x as i32, y as i32, z as i32) {
                    occ[Shape::linearize([x as u32 + PAD, y as u32 + PAD, z as u32 + PAD])
                        as usize] = 1.0;
                }
            }
        }
    }
    smooth_field(&mut occ, PX, PY, PZ, 2);
    // Signed field: inside (occ > 0.5) negative, outside positive; the surface is the occ = 0.5 iso.
    let sdf: Vec<f32> = occ.iter().map(|o| 0.5 - o).collect();

    let mut buffer = SurfaceNetsBuffer::default();
    surface_nets(
        &sdf,
        &Shape {},
        [0; 3],
        [PX - 1, PY - 1, PZ - 1],
        &mut buffer,
    );

    // The smoothed field gives Surface Nets good, consistently-outward normals — use them directly.
    let center = world.center();
    let mut vertices = Vec::with_capacity(buffer.positions.len());
    for (p, nrm) in buffer.positions.iter().zip(buffer.normals.iter()) {
        // Padded coords → voxel coords → centered world coords (matching the other meshes).
        let pad = PAD as f32;
        let (wx, wy, wz) = (p[0] - pad, p[1] - pad, p[2] - pad);
        let mat = surface_material(world, wx, wy, wz);
        vertices.push(Vertex {
            pos: [wx - center.x, wy - center.y, wz - center.z],
            nrm: *nrm,
            col: materials[mat].albedo,
            mat: mat as u32,
        });
    }
    Mesh {
        vertices,
        indices: buffer.indices,
    }
}

/// Build the SEA as its own mesh: the flat top of the real filled water matter, at the waterline datum
/// `SEA_LEVEL_Y`, over every submerged column, plus vertical shore walls where the water meets dry land
/// or the patch edge. This is NOT a decorative disconnected plane — it is the literal top surface of the
/// `water` voxels the world store filled below the datum (their sides/bottom rest against the seabed and
/// are hidden). Kept SEPARATE from [`build_surface_nets`] so the solid land stays a watertight manifold
/// (a water surface is legitimately an open shell). Flat `+Y` normals so the calm water reads as a mirror
/// and catches the specular sun-glint (unlike the smoothed land), tagged with the DB `water` material so
/// the shader gives it water optics. FLAGGED refinements (need more shader/physics work, or the deferred
/// dynamic step): volumetric scattering/absorption to the seabed, refraction, and waves/flow — the sea
/// is STATIC.
pub fn build_sea(world: &World, materials: &[Material]) -> Mesh {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let center = world.center();
    let Some(water) = world.water_mat else {
        return Mesh { vertices, indices }; // no sea in this world
    };
    let col = materials[water].albedo;
    let sea = crate::world::SEA_LEVEL_Y; // waterline (voxel-y); the top water voxel's top face sits here
    let (w, d) = (world.w as i32, world.d as i32);

    // Solid land top of a column (the seabed / shore height); None off the patch.
    let land_top = |x: i32, z: i32| -> Option<i32> { world.surface_top_voxel(x, z) };
    // Surface the sea only where it is at least MIN_RENDER_DEPTH voxels deep. The water MATTER (and its
    // hydrostatic pressure) still fills every voxel below the datum — this is purely a RENDER guard: at
    // the paper-thin (~1 voxel) shoreline fringe the flat waterline quad sits within the smoothed LAND
    // surface's ~1-voxel rounding and z-fights it into a speckle band, so we don't draw the surface there
    // (the shore just reads as wet-edged grass). Flagged: the thin shallows aren't surfaced yet.
    const MIN_RENDER_DEPTH: f32 = 2.0;
    let submerged = |x: i32, z: i32| -> bool {
        land_top(x, z).map_or(false, |t| sea - (t as f32) >= MIN_RENDER_DEPTH)
    };

    let push_quad = |vertices: &mut Vec<Vertex>,
                     indices: &mut Vec<u32>,
                     c: [glam::Vec3; 4],
                     nrm: glam::Vec3| {
        let base = vertices.len() as u32;
        for pos in c {
            vertices.push(Vertex {
                pos: (pos - center).into(),
                nrm: nrm.into(),
                col,
                mat: water as u32,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    for z in 0..d {
        for x in 0..w {
            if !submerged(x, z) {
                continue;
            }
            let (xf, zf) = (x as f32, z as f32);
            // TOP face: a unit quad at the waterline, wound CCW seen from above → outward normal +Y.
            push_quad(
                &mut vertices,
                &mut indices,
                [
                    glam::Vec3::new(xf, sea, zf),
                    glam::Vec3::new(xf, sea, zf + 1.0),
                    glam::Vec3::new(xf + 1.0, sea, zf + 1.0),
                    glam::Vec3::new(xf + 1.0, sea, zf),
                ],
                glam::Vec3::Y,
            );

            // SHORE walls: on each edge where the neighbour is NOT submerged (dry land or off-patch),
            // drop a vertical face from the waterline down to this column's seabed so the water body is
            // closed at the shoreline (a real edge, not a floating sheet).
            let bed = land_top(x, z).unwrap_or(sea as i32) as f32;
            let hi = sea;
            for (dx, dz, n) in [
                (1i32, 0i32, glam::Vec3::X),
                (-1, 0, glam::Vec3::NEG_X),
                (0, 1, glam::Vec3::Z),
                (0, -1, glam::Vec3::NEG_Z),
            ] {
                if submerged(x + dx, z + dz) {
                    continue; // interior water/water edge — no wall
                }
                // The four corners of the shared edge between (x,z) and its neighbour, at the two heights.
                let (a, b) = match (dx, dz) {
                    (1, 0) => (glam::Vec3::new(xf + 1.0, 0.0, zf), glam::Vec3::new(xf + 1.0, 0.0, zf + 1.0)),
                    (-1, 0) => (glam::Vec3::new(xf, 0.0, zf + 1.0), glam::Vec3::new(xf, 0.0, zf)),
                    (0, 1) => (glam::Vec3::new(xf + 1.0, 0.0, zf + 1.0), glam::Vec3::new(xf, 0.0, zf + 1.0)),
                    _ => (glam::Vec3::new(xf, 0.0, zf), glam::Vec3::new(xf + 1.0, 0.0, zf)),
                };
                let a_hi = glam::Vec3::new(a.x, hi, a.z);
                let b_hi = glam::Vec3::new(b.x, hi, b.z);
                let a_lo = glam::Vec3::new(a.x, bed, a.z);
                let b_lo = glam::Vec3::new(b.x, bed, b.z);
                push_quad(&mut vertices, &mut indices, [a_hi, a_lo, b_lo, b_hi], n);
            }
        }
    }
    Mesh { vertices, indices }
}

/// Separable 3-tap box blur applied `passes` times (border-clamped). Smooths a 0/1 occupancy field
/// so the iso-surface rounds instead of looking like eroded cubes.
fn smooth_field(field: &mut [f32], px: u32, py: u32, pz: u32, passes: u32) {
    let (px, py, pz) = (px as usize, py as usize, pz as usize);
    let idx = |x: usize, y: usize, z: usize| x + px * (y + py * z);
    let mut tmp = vec![0.0f32; field.len()];
    for _ in 0..passes {
        for z in 0..pz {
            for y in 0..py {
                for x in 0..px {
                    let c = field[idx(x, y, z)];
                    let l = if x > 0 { field[idx(x - 1, y, z)] } else { c };
                    let r = if x + 1 < px {
                        field[idx(x + 1, y, z)]
                    } else {
                        c
                    };
                    tmp[idx(x, y, z)] = (l + c + r) / 3.0;
                }
            }
        }
        for z in 0..pz {
            for y in 0..py {
                for x in 0..px {
                    let c = tmp[idx(x, y, z)];
                    let d = if y > 0 { tmp[idx(x, y - 1, z)] } else { c };
                    let u = if y + 1 < py { tmp[idx(x, y + 1, z)] } else { c };
                    field[idx(x, y, z)] = (d + c + u) / 3.0;
                }
            }
        }
        for z in 0..pz {
            for y in 0..py {
                for x in 0..px {
                    let c = field[idx(x, y, z)];
                    let b = if z > 0 { field[idx(x, y, z - 1)] } else { c };
                    let f = if z + 1 < pz {
                        field[idx(x, y, z + 1)]
                    } else {
                        c
                    };
                    tmp[idx(x, y, z)] = (b + c + f) / 3.0;
                }
            }
        }
        field.copy_from_slice(&tmp);
    }
}

/// Material a surface-nets vertex should wear — the HONEST surface tag.
///
/// The rule mirrors real ground: a TOP surface wears the column's surface skin — the material of its
/// topmost solid voxel: grass on undisturbed ground, or the deeper stratum a dig/crater has EXPOSED as
/// the new top. An exposed SIDE/wall below the surface (a cliff, a crater wall, the patch's cut edge)
/// wears the real stratum at that depth.
///
/// This fixes the dark-basalt-tiles-on-grass-slopes bug: the biosphere skin is only one voxel thick, so
/// a plain nearest-solid sample ([`nearest_material`]) on a slope grabbed the basalt crust one voxel
/// beneath the skin and painted the hillside dark rock. Keying the TOP to the column's own topmost solid
/// voxel is independent of slope steepness (we compare against the top of the column directly under the
/// vertex), so undisturbed slopes read uniform grass while a genuine dig still exposes the real basalt.
fn surface_material(world: &World, wx: f32, wy: f32, wz: f32) -> usize {
    let (bx, bz) = (wx.round() as i32, wz.round() as i32);
    // How far below its column's top a vertex may sit and still count as the TOP cap (not an exposed
    // wall): the smoothed iso-surface offset (~1 voxel above the topmost solid centre) plus the small
    // drop of a natural slope across the rounding cell. A near-vertical cliff/crater wall sits FAR below
    // its column top, so it correctly falls through to the exposed-strata branch.
    const TOP_BAND: f32 = 2.0;
    if let Some(top_air) = world.surface_top_voxel(bx, bz) {
        let top_solid = top_air - 1; // the surface skin voxel (grass, unless a dig exposed a stratum)
        if wy >= top_solid as f32 - TOP_BAND {
            if let Some(m) = world.material_at(bx, top_solid, bz) {
                return m;
            }
        }
    }
    // Exposed side/wall (a cliff, crater wall, or the patch's cut edge): the real stratum at this depth.
    nearest_material(world, wx, wy, wz)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;
    use crate::planet;

    #[test]
    fn the_sea_surface_is_meshed_flat_at_the_waterline() {
        // The world mesh must render the sea: water-tagged vertices whose TOP sits exactly at the
        // waterline datum (SEA_LEVEL_Y), spanning the low basins — the visible flat sea, meshed from the
        // real filled water matter (not a decorative plane). The flat top gives the +Y normals that catch
        // the Fresnel sky-reflection in the shader.
        use crate::world;
        let mats = materials::load();
        let w = world::generate(&mats);
        let water = materials::index_of(&mats, "water");
        let mesh = build_sea(&w, &mats);
        let c = w.center();
        let wv: Vec<&Vertex> = mesh.vertices.iter().filter(|v| v.mat as usize == water).collect();
        assert_eq!(wv.len(), mesh.vertices.len(), "the sea mesh is all water-tagged");
        assert!(wv.len() > 100, "the sea must be meshed as a visible body (got {} verts)", wv.len());
        // The highest water vertices are the flat top faces — they sit AT the waterline datum.
        let top_y = wv.iter().map(|v| v.pos[1] + c.y).fold(f32::MIN, f32::max);
        assert!(
            (top_y - world::SEA_LEVEL_Y).abs() < 1e-3,
            "sea surface top {top_y} must sit at the waterline SEA_LEVEL_Y {}",
            world::SEA_LEVEL_Y
        );
        // Some water vertex has a flat, upward normal (the mirror surface the Fresnel water shading needs).
        assert!(
            wv.iter().any(|v| v.nrm[1] > 0.99),
            "the sea surface must have flat +Y (mirror) faces at the waterline"
        );
    }

    #[test]
    fn earth_cap_follows_the_shared_terrain_and_curves_to_a_horizon() {
        // The distant ground is the REAL Earth surface, sampled from the SAME heightfield as the voxel
        // patch (crate::world::terrain_height) — so it (a) FOLLOWS the rolling relief (NOT a flat shelf
        // pinned at one height, the bug that made valley rubble read as hovering) and (b) still CURVES
        // DOWN to a finite horizon by the sphere drop d²/(2R) (NOT a flat plane whose horizon is at
        // infinity, the fudge this scene exists to kill).
        use crate::world::{self, terrain_height};
        let mats = materials::load();
        let radius = planet::earth().radius() as f32; // ≈6.371e6 m, the same body the space band draws
        let w = world::generate(&mats);
        let center = w.center();
        let r_max = 26_000.0f32;
        // Default BULK cap (no hole): the full disk over the whole footprint and beyond.
        let mesh = build_earth_cap(&mats, center, radius, r_max, None, None);

        // (a) Every vertex sits EXACTLY on the shared heightfield minus the sphere drop — proving the cap
        //     is the same terrain_height surface, not an independent flat plane.
        let mut relief_vals: Vec<f32> = Vec::with_capacity(mesh.vertices.len());
        let mut max_d = 0.0f32;
        let mut drop_at_far = 0.0f32;
        for v in &mesh.vertices {
            let [x, y, z] = v.pos;
            let d = (x * x + z * z).sqrt();
            let curvature = d * d / (2.0 * radius);
            let expected = terrain_height(x + center.x, z + center.z) - center.y - curvature;
            assert!(
                (y - expected).abs() < 0.01,
                "cap vertex off the shared heightfield at d={d:.0}: y={y:.3} expected {expected:.3}"
            );
            relief_vals.push(y + curvature); // remove curvature → the terrain relief component
            if d > max_d {
                max_d = d;
                drop_at_far = curvature;
            }
        }

        // Curvature: the far edge drops many metres (a flat plane would drop 0), matching d²/2R.
        assert!(
            drop_at_far > 20.0,
            "far cap must drop many metres (curved), got {drop_at_far:.2} m — a flat plane would be 0"
        );

        // FOLLOWS relief (guard against a flat cap pinned at one height): with curvature removed, the
        // per-vertex terrain component must show real spread — the patch amplitude is 34 m, so a flat
        // shelf (constant relief, std≈0) fails here.
        let n = relief_vals.len() as f32;
        let mean = relief_vals.iter().sum::<f32>() / n;
        let std = (relief_vals.iter().map(|r| (r - mean).powi(2)).sum::<f32>() / n).sqrt();
        assert!(
            std > 3.0,
            "cap must FOLLOW the rolling relief, not sit flat above the valleys (relief std {std:.2} m)"
        );

        // Per-vertex normals must be the TRUE normal of the surface the cap follows — `(−∂h/∂x, 1,
        // −∂h/∂z)` of the shared height (relief + curvature) — so the distant ground shades as rolling
        // hills (a flat plane's normals are all EXACTLY +Y). We verify each stored normal against the
        // finite-difference normal of the SAME cap_y the mesher uses, and that the normals genuinely TILT
        // (real hillslopes), not stay vertical.
        let cap_y = |cx: f32, cz: f32| -> f32 {
            let d = (cx * cx + cz * cz).sqrt();
            terrain_height(cx + center.x, cz + center.z) - center.y - d * d / (2.0 * radius)
        };
        let mut max_lean = 0.0f32;
        for v in &mesh.vertices {
            let (cx, cz) = (v.pos[0], v.pos[2]);
            let e = 1.0f32;
            let dh_dx = (cap_y(cx + e, cz) - cap_y(cx - e, cz)) / (2.0 * e);
            let dh_dz = (cap_y(cx, cz + e) - cap_y(cx, cz - e)) / (2.0 * e);
            let expected = glam::Vec3::new(-dh_dx, 1.0, -dh_dz).normalize();
            let n = glam::Vec3::from(v.nrm);
            assert!(n.y > 0.0, "cap normal must point up (got {n:?})");
            assert!(
                n.dot(expected) > 0.999,
                "cap normal must follow the surface slope at ({cx:.0},{cz:.0}): {n:?} vs {expected:?}"
            );
            max_lean = max_lean.max((n.x * n.x + n.z * n.z).sqrt());
        }
        // Genuinely NOT a flat plane: some normals lean well off vertical (real hillslopes tilt the light).
        assert!(
            max_lean > 0.1,
            "cap normals must tilt with the rolling relief, not stay flat +Y (max lean {max_lean:.3})"
        );
    }

    #[test]
    fn earth_cap_full_disk_covers_the_footprint_as_one_bulk_surface() {
        // BULK EVERYWHERE (Robin: "dissolve the fixed cube"): with NO hole the cap is a FULL disk that
        // samples the shared terrain_height from its very centre outward, so the default terrain is one
        // continuous smooth bulk surface over the whole footprint — nothing special about any square. The
        // per-vertex height must still be exactly the shared heightfield (minus the sphere drop) so the
        // near bulk cap is the SAME surface the resolved voxels would be, and the interior must be COVERED
        // (vertices actually fall inside the footprint — no hole where a floating cube used to sit).
        use crate::world::{self, terrain_height, D, W};
        let mats = materials::load();
        let radius = planet::earth().radius() as f32;
        let w = world::generate(&mats);
        let center = w.center();
        let mesh = build_earth_cap(&mats, center, radius, 26_000.0, None, None);

        let mut inside_count = 0usize;
        for v in &mesh.vertices {
            let [x, y, z] = v.pos;
            let d = (x * x + z * z).sqrt();
            let expected = terrain_height(x + center.x, z + center.z) - center.y - d * d / (2.0 * radius);
            assert!(
                (y - expected).abs() < 0.01,
                "bulk cap vertex off the shared heightfield at d={d:.0}: y={y:.3} expected {expected:.3}"
            );
            let (vx, vz) = (x + center.x, z + center.z); // centered → voxel frame
            if vx > 1.0 && vx < W as f32 - 1.0 && vz > 1.0 && vz < D as f32 - 1.0 {
                inside_count += 1;
            }
        }
        assert!(
            inside_count > 200,
            "the full disk must COVER the footprint interior (bulk, no cube-hole), got {inside_count} verts inside"
        );
    }

    #[test]
    fn earth_cap_with_a_hole_leaves_the_resolved_region_open_and_joins_it() {
        // ON-DEMAND HOLE: when a region is RESOLVED into voxels, the cap must (1) leave a HOLE exactly over
        // that region (here the whole footprint) so a crater/dig excavated below the surface stays visible
        // (no cap lid over it), and (2) meet the resolved patch edge CONTINUOUSLY at the boundary.
        use crate::world::{self, D, W};
        let mats = materials::load();
        let radius = planet::earth().radius() as f32;
        let w = world::generate(&mats);
        let center = w.center();
        // Resolve the whole footprint: hole half-extents = the patch's centre offset (its half-extents).
        let mesh = build_earth_cap(&mats, center, radius, 26_000.0, Some(glam::Vec2::new(center.x, center.z)), None);

        // (1) HOLE: NO cap vertex may lie strictly inside the patch footprint (voxel (x,z) in 0..W, 0..D).
        //     A cap vertex there would be a lid drawn over the patch, hiding craters/digs beneath it.
        for v in &mesh.vertices {
            let (vx, vz) = (v.pos[0] + center.x, v.pos[2] + center.z); // centered → voxel frame
            let inside = vx > 0.5 && vx < W as f32 - 0.5 && vz > 0.5 && vz < D as f32 - 0.5;
            assert!(
                !inside,
                "cap vertex inside the patch footprint at voxel ({vx:.1},{vz:.1}) — it would hide craters"
            );
        }

        // (2) CONTINUOUS: the seam ring (the FIRST SEG vertices) lies ON the footprint boundary; each
        //     must match the patch's edge surface there. Tolerance covers the patch's integer rounding
        //     (≤0.5 m), the ≤1-voxel offset from the boundary to the nearest resolved column, and the
        //     (sub-mm) sphere drop across the 96 m patch — a genuine no-step bound against 34 m of relief.
        const SEG: usize = 160;
        assert!(mesh.vertices.len() >= SEG, "cap has no seam ring");
        let mut checked = 0;
        for v in &mesh.vertices[..SEG] {
            let y = v.pos[1];
            let (vx, vz) = (v.pos[0] + center.x, v.pos[2] + center.z);
            // Sanity: a seam vertex sits on the footprint boundary (one axis at 0 or W, the other within).
            let on_boundary = (vx.abs() < 0.5 || (vx - W as f32).abs() < 0.5 || vz.abs() < 0.5
                || (vz - D as f32).abs() < 0.5)
                && (-0.5..=W as f32 + 0.5).contains(&vx)
                && (-0.5..=D as f32 + 0.5).contains(&vz);
            assert!(on_boundary, "seam vertex off the footprint boundary at ({vx:.1},{vz:.1})");
            let xi = (vx.round() as i32).clamp(0, W as i32 - 1);
            let zi = (vz.round() as i32).clamp(0, D as i32 - 1);
            let patch = w.surface_top_voxel(xi, zi).expect("edge column solid") as f32 - center.y;
            assert!(
                (y - patch).abs() < 2.0,
                "cap seam steps off the patch at edge voxel ({xi},{zi}): cap {y:.2} vs patch {patch:.2}"
            );
            checked += 1;
        }
        assert_eq!(checked, SEG, "the whole seam ring must be continuity-checked");
    }

    /// **The cap must render the ground the physics reports.** A column demoted to T0 keeps its surface
    /// in `displacement`; if the cap ignores that field it redraws pristine relief, so a de-resolved
    /// crater is solid under the probe and the grains (both read `ground_top_voxel`) while LOOKING
    /// untouched. Physics drives the render — never the reverse (docs/46) — so this pins the agreement.
    #[test]
    fn the_bulk_cap_renders_a_de_resolved_crater() {
        let mats = materials::load();
        let mut w = crate::world::generate(&mats);
        let center = w.center();
        let radius = 6.371e6_f32;
        // Dig a pit at the patch centre, then demote those columns to the field.
        let (px, pz) = (center.x as i32, center.z as i32);
        const DEPTH: i32 = 6;
        for dz in -3..=3 {
            for dx in -3..=3 {
                // Dig from each column's OWN top: a fixed y-range across columns of differing height
                // leaves voxels stranded above the cut, which is a void, which correctly refuses to bake.
                let top = w.surface_top_voxel(px + dx, pz + dz).unwrap();
                for y in (top - DEPTH)..top {
                    w.set_voxel(px + dx, y, pz + dz, None);
                }
                w.demote_column_to_field(px + dx, pz + dz).expect("bakeable");
            }
        }
        let sample = |m: &Mesh| -> f32 {
            // Lowest vertex near the patch centre — the crater floor if the cap is reading the field.
            m.vertices
                .iter()
                .filter(|v| v.pos[0].abs() < 2.0 && v.pos[2].abs() < 2.0)
                .fold(f32::MAX, |acc, v| acc.min(v.pos[1]))
        };
        let blind = build_earth_cap(&mats, center, radius, 26_000.0, None, None);
        let seeing = build_earth_cap(&mats, center, radius, 26_000.0, None, Some(&w));
        let (yb, ys) = (sample(&blind), sample(&seeing));
        assert!(yb.is_finite() && ys.is_finite(), "the cap must have vertices over the patch centre");
        assert!(
            ys < yb - (DEPTH as f32 * 0.5),
            "the cap did not follow the baked crater down: field-blind {yb:.2} m vs field-aware \
             {ys:.2} m, for a {DEPTH} m pit — a de-resolved crater would render as untouched ground"
        );
    }

    #[test]
    fn undisturbed_slope_surface_wears_grass_not_the_basalt_beneath() {
        // BUG 1: the terrain surface-nets mesh tagged slope vertices with the basalt CRUST one voxel
        // under the 1-voxel grass skin, so undisturbed green hills rendered as dark rock tiles. The honest
        // surface must wear its TOP material (grass) uniformly on every undisturbed slope — deeper strata
        // only show where actually EXPOSED (a dig/crater), tested separately below.
        use crate::world::{self, D, W};
        let mats = materials::load();
        let w = world::generate(&mats);
        let grass = materials::index_of(&mats, "grass");
        let basalt = materials::index_of(&mats, "basalt");
        let mesh = build_surface_nets(&w, &mats);
        let center = w.center();

        // TOP-facing surface vertices over the INTERIOR (away from the patch's cut edges, which legitimately
        // expose strata) must be grass — never the basalt crust beneath the skin.
        let mut tops = 0usize;
        for v in &mesh.vertices {
            // Surface Nets emits raw (un-normalized) gradient normals; the shader normalizes them. Do the
            // same here, then keep only clearly upward-facing top surface (skip walls / patch-edge cuts).
            let n = glam::Vec3::from(v.nrm).normalize_or_zero();
            if n.y < 0.5 {
                continue;
            }
            let (vx, vz) = (v.pos[0] + center.x, v.pos[2] + center.z); // centered → voxel frame
            if vx < 4.0 || vx > W as f32 - 4.0 || vz < 4.0 || vz > D as f32 - 4.0 {
                continue; // stay off the patch boundary walls
            }
            tops += 1;
            assert_ne!(
                v.mat as usize, basalt,
                "undisturbed top vertex at voxel ({vx:.1},{vz:.1}) tagged BASALT — the slope-tiles bug"
            );
            assert_eq!(
                v.mat as usize, grass,
                "undisturbed top vertex at voxel ({vx:.1},{vz:.1}) must wear grass, got {}",
                mats[v.mat as usize].id
            );
        }
        assert!(tops > 200, "expected many interior top vertices to check, got {tops}");
    }

    #[test]
    fn a_dig_through_the_grass_skin_exposes_the_real_basalt_beneath() {
        // The counterpart to the slope test: where the grass skin is actually CUT AWAY (a dig/crater), the
        // surface must honestly wear the EXPOSED stratum (basalt), not be blanket-recoloured green. This is
        // the invariant that keeps the fix honest — surface = skin, but a real excavation shows real rock.
        use crate::world::{self, D, W};
        let mats = materials::load();
        let mut w = world::generate(&mats);
        let grass = materials::index_of(&mats, "grass");
        let basalt = materials::index_of(&mats, "basalt");
        let (cx, cz) = (W as i32 / 2, D as i32 / 2);
        let top = w.surface_top_voxel(cx, cz).expect("solid column at centre");

        // Excavate a pit at the centre: strip the grass skin AND several crust voxels over a 7×7 area, so
        // the new column top is exposed basalt.
        for y in (top - 6)..top {
            for dz in -3..=3 {
                for dx in -3..=3 {
                    let (x, z) = (cx + dx, cz + dz);
                    if x >= 0 && x < W as i32 && z >= 0 && z < D as i32 && y >= 0 {
                        let i = w.idx(x as usize, y as usize, z as usize);
                        w.voxels[i] = 0; // dig to air
                    }
                }
            }
        }
        let new_top = w.surface_top_voxel(cx, cz).expect("pit floor still solid");
        assert_eq!(
            w.material_at(cx, new_top - 1, cz),
            Some(basalt),
            "the dig must expose real basalt crust as the new column top"
        );

        // The meshed pit floor (upward-facing vertices over the dug column) must be tagged basalt — the
        // real exposed stratum — and NOT grass.
        let mesh = build_surface_nets(&w, &mats);
        let center = w.center();
        let mut floor: Option<&Vertex> = None;
        for v in &mesh.vertices {
            let n = glam::Vec3::from(v.nrm).normalize_or_zero();
            if n.y < 0.5 {
                continue;
            }
            let (vx, vz) = (v.pos[0] + center.x, v.pos[2] + center.z);
            if (vx - cx as f32).abs() <= 1.5 && (vz - cz as f32).abs() <= 1.5 {
                // the lowest such vertex is the pit floor
                if floor.map_or(true, |f| v.pos[1] < f.pos[1]) {
                    floor = Some(v);
                }
            }
        }
        let floor = floor.expect("a pit-floor surface vertex over the dug column");
        assert_eq!(
            floor.mat as usize, basalt,
            "the dug pit floor must expose real basalt, got {}",
            mats[floor.mat as usize].id
        );
    }

    #[test]
    fn the_land_surface_mesh_is_a_closed_watertight_manifold() {
        // BUG 2: Robin saw white lines on ridge crests "like peeking THROUGH the terrain". If real, that is
        // an open crack — a boundary edge belonging to only ONE triangle — through which the background
        // shows. The solid land mesh must be a CLOSED manifold: every edge shared by exactly two triangles,
        // so the ground is genuinely opaque matter from every angle (the sea is meshed separately as a
        // legitimately-open shell, so this checks only the solid land).
        use crate::world;
        use std::collections::HashMap;
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = build_surface_nets(&w, &mats);
        assert!(mesh.indices.len() >= 3, "empty land mesh");
        assert_eq!(mesh.indices.len() % 3, 0, "indices are not whole triangles");

        let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
        for tri in mesh.indices.chunks(3) {
            for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_insert(0) += 1;
            }
        }
        let open = edges.iter().filter(|(_, &c)| c != 2).count();
        assert_eq!(
            open, 0,
            "{open} open / non-manifold edges — the ground has cracks the sky can show through"
        );
    }
}

