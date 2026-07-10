// GPU particle step (docs/22, docs/23) — the debris sim, one compute invocation per particle.
//
// Now with PARTICLE-PARTICLE CONTACT (granular): grains push apart, stack, and flow to a natural
// slope, instead of resting only on the terrain heightfield (which left them stranded on ledges and
// unable to pile — the "moiré"). Contact needs neighbours, so each substep runs four passes:
//
//   1. cs_grid_clear   — zero the spatial-hash cell counts
//   2. cs_grid_insert  — bucket every particle into its grid cell (atomic)
//   3. cs_forces       — sum contact accelerations from the 27 neighbouring cells → forces[]
//   4. cs_integrate    — gravity + contact + drag + terrain collision + cooling, then move
//
// Splitting force-accumulation (pass 3, positions read-only) from integration (pass 4) avoids a
// read/write race on the particle buffer. The contact law is a WGSL mirror of `granular::contact_accel`
// (verified natively; kept in sync by construction).

struct Params {
    gravity      : vec3<f32>,
    dt           : f32,
    center       : vec3<f32>, // world.center() offset (centered→voxel coords)
    c_max_accel  : f32,       // cap on normal contact accel (prevents deep-overlap launches — docs/23)
    drag         : f32,
    contact_damp : f32,       // ground-collision velocity damping
    settle_speed : f32,
    part_half    : f32,
    cool_rate    : f32,
    count        : u32,
    world_w      : u32,
    world_d      : u32,
    // --- granular spatial hash + contact (docs/23) ---
    cell_size    : f32,       // grid cell edge (m); ≥ contact diameter so contacts stay within ±1 cell
    table_mask   : u32,       // hash table size − 1 (table size is a power of two)
    bucket_k     : u32,       // max particles recorded per hash bucket (overflow is dropped)
    c_radius     : f32,       // grain radius (m)
    c_stiffness  : f32,       // normal repulsion (1/s²) per metre of overlap
    c_normal_damp: f32,       // normal damping (1/s)
    c_friction   : f32,       // Coulomb μ
    c_tangent_damp: f32,      // tangential regularization (1/s)
};

struct Particle {
    offset   : vec3<f32>,
    temp     : f32,
    vel      : vec3<f32>,
    resting  : f32,
    color    : vec3<f32>,
    material : f32,
    emission : vec3<f32>,
    _pad     : f32,
};

@group(0) @binding(0) var<uniform> P : Params;
@group(0) @binding(1) var<storage, read_write> particles : array<Particle>;
@group(0) @binding(2) var<storage, read> heightfield : array<i32>;
@group(0) @binding(3) var<storage, read_write> grid_count : array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> grid_bucket : array<u32>;
@group(0) @binding(5) var<storage, read_write> forces : array<vec4<f32>>; // xyz=force, w=stiffness K
@group(0) @binding(6) var<storage, read_write> render_out : array<Particle>; // 8× render sub-cubes

fn incandescence(t : f32) -> vec3<f32> {
    if (t <= 800.0) { return vec3<f32>(0.0); }
    let x = t - 800.0;
    let intensity = clamp(x / 2200.0, 0.0, 4.0);
    let gg = clamp(x / 2200.0, 0.0, 1.0);
    let bb = clamp((t - 2600.0) / 2400.0, 0.0, 1.0);
    return vec3<f32>(intensity, gg * intensity, bb * intensity);
}

// --- spatial hash ---------------------------------------------------------------------------------
fn cell_of(pos : vec3<f32>) -> vec3<i32> {
    return vec3<i32>(floor(pos / P.cell_size));
}
fn hash_cell(c : vec3<i32>) -> u32 {
    let h = (u32(c.x) * 73856093u) ^ (u32(c.y) * 19349663u) ^ (u32(c.z) * 83492791u);
    return h & P.table_mask;
}

// Contact between grain i and grain j. Returns vec4: xyz = acceleration on i, w = the contact's normal
// STIFFNESS (0 if not touching) — the caller sums w to form the per-grain K for the implicit velocity
// update. EXACT mirror of granular::contact_accel (docs/23). NOTE: the damping/friction here REMOVE
// kinetic energy; physically that becomes HEAT in the grains (→ temp_k → radiated). We don't route it
// to temperature yet (flagged) — matters for phase change (steam/boiling) later. No force cap (a cap
// is a fudge); stability comes from the implicit integrator, not a clamp.
fn contact_accel(pi : vec3<f32>, vi : vec3<f32>, pj : vec3<f32>, vj : vec3<f32>) -> vec4<f32> {
    let d = pi - pj;
    let dist = length(d);
    let touch = 2.0 * P.c_radius;
    if (dist >= touch || dist < 1.0e-9) { return vec4<f32>(0.0); }
    let n = d / dist;
    let overlap = touch - dist;
    let v_rel = vi - vj;
    let v_n = dot(v_rel, n);
    let a_n_mag = max(P.c_stiffness * overlap - P.c_normal_damp * v_n, 0.0);
    let a_n = n * a_n_mag;
    let v_t = v_rel - n * v_n;
    let vt_mag = length(v_t);
    var a_t = vec3<f32>(0.0);
    if (vt_mag > 1.0e-9) {
        let mag = min(P.c_tangent_damp * vt_mag, P.c_friction * a_n_mag);
        a_t = -(v_t / vt_mag) * mag;
    }
    return vec4<f32>(a_n + a_t, P.c_stiffness);
}

// --- pass 1: clear cell counts --------------------------------------------------------------------
@compute @workgroup_size(64)
fn cs_grid_clear(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i > P.table_mask) { return; } // table size = table_mask + 1
    atomicStore(&grid_count[i], 0u);
}

// --- pass 2: bucket each particle into its cell ---------------------------------------------------
@compute @workgroup_size(64)
fn cs_grid_insert(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= P.count) { return; }
    let h = hash_cell(cell_of(particles[i].offset));
    let slot = atomicAdd(&grid_count[h], 1u);
    if (slot < P.bucket_k) {
        grid_bucket[h * P.bucket_k + slot] = i;
    }
}

// --- pass 3: accumulate contact forces from the 27 neighbouring cells -----------------------------
@compute @workgroup_size(64)
fn cs_forces(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= P.count) { return; }
    let pi = particles[i].offset;
    let vi = particles[i].vel;
    let base = cell_of(pi);
    var acc = vec4<f32>(0.0); // xyz = force, w = summed contact stiffness (for the implicit step)
    for (var dz = -1; dz <= 1; dz = dz + 1) {
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            for (var dx = -1; dx <= 1; dx = dx + 1) {
                let h = hash_cell(base + vec3<i32>(dx, dy, dz));
                let n = min(atomicLoad(&grid_count[h]), P.bucket_k);
                for (var s = 0u; s < n; s = s + 1u) {
                    let j = grid_bucket[h * P.bucket_k + s];
                    if (j == i) { continue; }
                    acc = acc + contact_accel(pi, vi, particles[j].offset, particles[j].vel);
                }
            }
        }
    }
    acc = acc + terrain_accel(pi, vi); // the terrain is matter too — same contact law
    forces[i] = acc;
}

// Terrain contact as a PENALTY FORCE — the honest, Newtonian way (docs/23). The terrain is solid
// matter; a grain that penetrates it feels a repulsive contact force along the surface normal, exactly
// like grain-grain contact — NO position teleport (teleporting a grain up injects potential energy, the
// crater "free energy"; teleporting sideways is just as unphysical). The normal is the shortest way out
// of the solid; the force is a spring + damper + Coulomb friction — the SAME contact law as between
// grains. Returns vec4: xyz = acceleration, w = the contact's stiffness (0 if not touching), for the
// implicit velocity update.
fn terrain_accel(pos : vec3<f32>, vel : vec3<f32>) -> vec4<f32> {
    let vx = pos.x + P.center.x;
    let vz = pos.z + P.center.z;
    let cx = i32(floor(vx));
    let cz = i32(floor(vz));
    if (cx < 0 || cz < 0 || cx >= i32(P.world_w) || cz >= i32(P.world_d)) { return vec4<f32>(0.0); }
    let top = f32(heightfield[u32(cz) * P.world_w + u32(cx)]) - P.center.y - 0.5;
    let bottom = pos.y - P.part_half;
    if (bottom >= top) { return vec4<f32>(0.0); } // not penetrating the column
    // Shortest way out (min-translation) gives the contact NORMAL + penetration depth. A sideways exit
    // is valid only into a neighbour low enough to admit the grain (else it still penetrates there).
    let room = bottom + 1.0e-4;
    var depth = top - bottom;
    var normal = vec3<f32>(0.0, 1.0, 0.0); // up
    if (cx > 0) {
        let tn = f32(heightfield[u32(cz) * P.world_w + u32(cx - 1)]) - P.center.y - 0.5;
        let d = vx - f32(cx);
        if (tn <= room && d < depth) { depth = d; normal = vec3<f32>(-1.0, 0.0, 0.0); }
    }
    if (cx + 1 < i32(P.world_w)) {
        let tn = f32(heightfield[u32(cz) * P.world_w + u32(cx + 1)]) - P.center.y - 0.5;
        let d = f32(cx + 1) - vx;
        if (tn <= room && d < depth) { depth = d; normal = vec3<f32>(1.0, 0.0, 0.0); }
    }
    if (cz > 0) {
        let tn = f32(heightfield[u32(cz - 1) * P.world_w + u32(cx)]) - P.center.y - 0.5;
        let d = vz - f32(cz);
        if (tn <= room && d < depth) { depth = d; normal = vec3<f32>(0.0, 0.0, -1.0); }
    }
    if (cz + 1 < i32(P.world_d)) {
        let tn = f32(heightfield[u32(cz + 1) * P.world_w + u32(cx)]) - P.center.y - 0.5;
        let d = f32(cz + 1) - vz;
        if (tn <= room && d < depth) { depth = d; normal = vec3<f32>(0.0, 0.0, 1.0); }
    }
    // Penalty: repulsive spring along the normal (capped so a deep overlap can't launch) minus damping
    // of the inward velocity, PLUS Coulomb friction on the tangential slip — the SAME contact law as
    // between grains (docs/23). The friction is what stops grains sliding freely across the ground; the
    // angle of repose emerges from it. Never negative (contacts push).
    let vn = dot(vel, normal);
    let normal_mag = max(P.c_stiffness * depth - P.c_normal_damp * vn, 0.0);
    let v_t = vel - normal * vn;
    let vt_mag = length(v_t);
    var a_t = vec3<f32>(0.0);
    if (vt_mag > 1.0e-9) {
        let fmag = min(P.c_tangent_damp * vt_mag, P.c_friction * normal_mag);
        a_t = -(v_t / vt_mag) * fmag;
    }
    return vec4<f32>(normal * normal_mag + a_t, P.c_stiffness);
}

// --- render expansion: 1 physics grain → 8 sub-cubes (docs/23) ------------------------------------
// The sim steps ONE particle per 1 m voxel (8× cheaper, lower packing density), but we DRAW 8 half-size
// sub-cubes at the octant centres for a finer look — a render-only subdivision, run once per frame into
// a separate instance buffer, never stepped.
@compute @workgroup_size(64)
fn cs_expand(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= P.count) { return; }
    let src = particles[i];
    let q = 0.25;
    for (var k = 0u; k < 8u; k = k + 1u) {
        var o = src;
        let ox = select(-q, q, (k & 1u) != 0u);
        let oy = select(-q, q, (k & 2u) != 0u);
        let oz = select(-q, q, (k & 4u) != 0u);
        o.offset = src.offset + vec3<f32>(ox, oy, oz);
        render_out[i * 8u + k] = o;
    }
}

// --- pass 4: integrate (gravity + contact + drag + terrain + cooling) -----------------------------
@compute @workgroup_size(64)
fn cs_integrate(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= P.count) { return; }
    var pt = particles[i];

    // Cool toward ambient (Newton's law of cooling). EVERY particle steps every frame — a settled
    // grain is not skipped, so it keeps cooling AND re-checks its support / neighbours.
    pt.temp = 300.0 + (pt.temp - 300.0) * exp(-P.cool_rate * P.dt);

    // Implicit contact stabilization. The 2070 shows removing it entirely makes explicit EXPLODE (energy
    // rises) — so it earns its keep. BUT it is ISOTROPIC (damps ALL directions), which also kills a
    // grain's FREE flight (its ejection velocity) → impacts fracture but nothing flies. The right fix is
    // a DIRECTIONAL implicit (damp only along contact normals) — a per-grain stiffness tensor — so
    // packed grains are stabilized while free flight is untouched (docs/23). Isotropic for now (flagged).
    let f = forces[i];
    let a = P.gravity + f.xyz;
    let vel = ((pt.vel + a * P.dt) / (1.0 + P.dt * P.dt * f.w)) * P.drag;
    let pos = pt.offset + vel * P.dt;

    // NOTE on energy: the contact damping + friction here REMOVE kinetic energy. Physically that energy
    // is not destroyed — it becomes HEAT in the grains (→ temp_k) and radiates to space. We drop it for
    // now (flagged); routing it into temperature matters once we do phase change (steam/boiling). The
    // enforceable invariant today: this step never CREATES mechanical energy (docs/23).

    pt.offset = pos;
    pt.vel = vel;
    pt.resting = 0.0;
    pt.emission = incandescence(pt.temp);
    particles[i] = pt;
}
