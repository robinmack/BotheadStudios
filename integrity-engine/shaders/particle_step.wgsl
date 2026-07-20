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
    c_cohesion   : f32,       // attractive adhesion (1/s²) between touching grains — cohesion (docs/24)
    air_rho      : f32,       // AIR DENSITY (kg/m3) at this patch — 0 = vacuum. Replaces the old `drag`
                              // multiplier, which was a per-step velocity scale (a fudge: it bled speed
                              // from a particle in vacuum). Drag is now a FORCE from a real medium.
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
    // --- thermodynamic state (docs/35 increment 4b, docs/38) ---
    specific_heat : f32,      // J/(kg·K): the grain carries u = c·T, so temp = u/specific_heat (matches hydrostatic.rs)
    drag_cd : f32,            // drag coefficient (declared shape factor; IOU = resolved flow, docs/46)
    _hp1 : f32, _hp2 : f32,   // pad the uniform tail to a full 16-byte row
};

// θ-method blend for the directional implicit contact solve (docs/24 Stage 0+1). θ=0.5 is the
// trapezoidal (implicit-midpoint) rule — energy-CONSERVING, so restitution is real, but it RINGS at high
// coordination (no damping of the stiff contact modes → grains buzz and tunnel). θ=1 is backward-Euler —
// unconditionally dissipative (stable) but kills restitution to zero. A blend just above 0.5 keeps most
// of the rebound while adding just enough numerical dissipation to kill the ringing. The two θ-tensors
// are proportional, so the scheme is (I + S)·v_new = (I − ρ·S)·v_old + dt·a with S = θ²·dt²·k + θ·dt·c
// accumulated once, and ρ = (1−θ)/θ a scalar. High-frequency modes decay to amplitude ρ, so ρ (hence θ)
// is exactly the ringing-vs-restitution knob. This is a numerical-scheme parameter, NOT physics (the
// material's restitution lives in c); it's the minimum dissipation needed for a stable stiff solve.
const THETA     : f32 = 0.70;
const THETA_RHO : f32 = (1.0 - THETA) / THETA;

struct Particle {
    offset   : vec3<f32>,
    u        : f32,       // specific internal energy (J/kg) — the thermodynamic state (docs/38). temp = u/c is
                          // DERIVED (grain_temp): the physically-correct variable across phase change (T plateaus
                          // at melt/vapor while u keeps rising) and the same state the SPH Particle carries.
    vel      : vec3<f32>,
    resting  : f32,
    color    : vec3<f32>,
    material : f32,
    emission : vec3<f32>,
    rho      : f32,       // density (kg/m³) — the other Tillotson input. Placeholder ρ₀ until 4b.2 computes it.
    radius   : f32,       // THIS grain's contact radius (m) — docs/47 §1. Not a global constant: granularity
                          // follows the interaction (metre grains for ejecta, ~1 cm for a tyre patch), so the
                          // size must travel WITH the particle rather than in the per-dispatch uniform.
    _p0 : f32, _p1 : f32, _p2 : f32,  // pad to a 5th 16-byte row; reserved (a cached grid level goes here)
};

// Per-grain contact accumulation for the directional implicit velocity solve: the total contact force,
// the contact stiffness/damping TENSOR S = Σ g·(n⊗n) (its unique components), AND the momentum-coupling
// vector sv_nbr = Σ S_contact·v_neighbor. The tensor stabilizes packed grains along contact normals; the
// null space (no contact) is untouched so free-flight/ejection velocity survives (docs/24 Stage 0). The
// sv_nbr term is what makes the solve MOMENTUM-CONSERVING for MOVING contacts: without it the per-grain
// solve damps each grain's ABSOLUTE velocity, bleeding the shared COM motion (a 20 m/s head-on collision
// lost ~74% of its momentum — caught by gpu-verify F5). With it, the pair's COM velocity is preserved
// exactly (static terrain has v_neighbor = 0, so its momentum is correctly absorbed). 64 bytes (4 rows).
struct Accum {
    force  : vec3<f32>, // Σ contact force (grains + terrain)
    headroom : f32,     // free gap (m) to the nearest grain resting above — caps the terrain projection
    s_diag : vec3<f32>, // Sxx, Syy, Szz
    _p1    : f32,
    s_off  : vec3<f32>, // Sxy, Sxz, Syz
    _p2    : f32,
    sv_nbr : vec3<f32>, // Σ S_contact · v_neighbor  (momentum-conserving coupling; 0 for static terrain)
    _p3    : f32,
};

@group(0) @binding(0) var<uniform> P : Params;
@group(0) @binding(1) var<storage, read_write> particles : array<Particle>;
@group(0) @binding(2) var<storage, read> heightfield : array<i32>;
@group(0) @binding(3) var<storage, read_write> grid_count : array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> grid_bucket : array<u32>;
@group(0) @binding(5) var<storage, read_write> forces : array<Accum>;
@group(0) @binding(6) var<storage, read_write> render_out : array<Particle>; // 8× render sub-cubes

// Temperature (K) derived from the grain's specific internal energy: u = c·T ⇒ T = u/c (matches the SPH
// path, hydrostatic.rs:82). NOTE (4b): linear for now — the latent-heat plateaus at melt/vaporization
// (T ~constant while u climbs through e_iv..e_cv) come with the EOS tier (4b.3), where they matter.
fn grain_temp(u : f32) -> f32 {
    return u / max(P.specific_heat, 1.0);
}

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
fn zero_accum() -> Accum {
    return Accum(vec3<f32>(0.0), 1.0e30, vec3<f32>(0.0), 0.0, vec3<f32>(0.0), 0.0, vec3<f32>(0.0), 0.0);
}
// One contact's contribution to the implicit stabilization matrix along normal n. The coefficient g is
// the full backward-Euler Jacobian of a spring-DAMPER contact projected onto n: g = dt²·k + dt·c. The
// dt²·k term makes the SPRING implicit (stable at any stiffness); the dt·c term makes the DAMPER implicit
// too — without it the damping is explicit, and in a dense pack the summed damping coefficient (Z·c for
// coordination Z) times dt exceeds the explicit stability limit (2), so the damper flips sign and INJECTS
// energy (the directional-implicit explosion). Both belong in M so contacts are unconditionally stable.
fn accum_tensor(acc : ptr<function, Accum>, n : vec3<f32>, g : f32) {
    (*acc).s_diag = (*acc).s_diag + g * n * n;
    (*acc).s_off = (*acc).s_off + g * vec3<f32>(n.x * n.y, n.x * n.z, n.y * n.z);
}

// Short-range adhesion tail (m) beyond touch: real cohesion (van der Waals / capillary) is a near-contact
// attraction that fades over a small gap. Sets how far a bonded pair can be pulled before the bond lets go.
const COH_RANGE : f32 = 0.15;

fn contact_accel(pi : vec3<f32>, vi : vec3<f32>, ri : f32, pj : vec3<f32>, vj : vec3<f32>, rj : f32) -> Accum {
    var acc = zero_accum();
    let d = pi - pj;
    let dist = length(d);
    let touch = ri + rj; // NOT 2*radius: the pair may differ in size (mirrors granular::contact_force)
    // Cohesion EXTENDS the interaction range: grains attract within COH_RANGE of touching, not only when
    // overlapping. Beyond that the bond has let go (separated).
    if (dist >= touch + COH_RANGE || dist < 1.0e-9) { return acc; }
    let n = d / dist;
    let overlap = touch - dist; // > 0 overlapping (compression); < 0 separated but within cohesion range
    let v_rel = vi - vj;
    let v_n = dot(v_rel, n);
    // Normal force = repulsive spring (compression only) MINUS cohesive adhesion (an attraction that is
    // full in contact and tapers to 0 at COH_RANGE). c_cohesion=0 ⇒ pure repulsion (dry, cohesionless
    // sand — which correctly grazes frictionlessly). c_cohesion>0 ⇒ touching grains BOND, so a resting or
    // grazing pair has a real normal load and therefore friction (closing the zero-overlap knife-edge),
    // and a wet/fine/soil pile holds a slope its dry counterpart can't. The DAMPING is implicit (tensor).
    let f_rep = P.c_stiffness * max(overlap, 0.0);
    let sep = max(-overlap, 0.0); // separation beyond touch (0 while overlapping)
    let f_coh = P.c_cohesion * clamp(1.0 - sep / COH_RANGE, 0.0, 1.0); // adhesion, tapered over the range
    let a_n = n * (f_rep - f_coh); // net: repulsive − attractive (can pull grains together)
    // Friction opposes slip, capped at μ·N where N is the real contact LOAD pressing the surfaces — BOTH
    // the repulsion and the adhesion hold them together, so cohesion raises the friction (apparent
    // cohesion in shear). This is what gives a touching pair friction even at zero compression.
    let normal_load = f_rep + f_coh;
    let v_t = v_rel - n * v_n;
    let vt_mag = length(v_t);
    var a_t = vec3<f32>(0.0);
    if (vt_mag > 1.0e-9) {
        // Clamped at vt_mag/dt so friction can only HALT the slip, never reverse it (see the note above).
        let mag = min(min(P.c_tangent_damp * vt_mag, P.c_friction * normal_load), vt_mag / P.dt);
        a_t = -(v_t / vt_mag) * mag;
    }
    acc.force = a_n + a_t;
    // Only the repulsive SPRING is stiff enough to need implicit stabilization (the adhesion is a bounded,
    // near-constant force). Gate the tensor + momentum coupling on compression.
    if (overlap > 0.0) {
        let g = THETA * THETA * P.dt * P.dt * P.c_stiffness + THETA * P.dt * P.c_normal_damp;
        accum_tensor(&acc, n, g);
        // Momentum coupling: S_contact·v_j = g·(n·v_j)·n — preserves the pair's COM velocity (gpu-verify F5).
        acc.sv_nbr = g * dot(n, vj) * n;
    }
    return acc;
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

// --- pass 2b: put each bucket in a CANONICAL order -------------------------------------------------
//
// DETERMINISM. `cs_grid_insert` takes its slot from `atomicAdd`, so which particle lands at which slot
// is decided by whichever thread wins the race — the bucket holds the same SET every run but in a
// different ORDER. `cs_forces` then sums contact forces in that order, and float addition is not
// associative, so identical input produced different output run to run. Measured before this pass
// existed: 7 of 174 grains diverged by up to 8.3e-5 m after 40 frames, which amplified to ~6% spread in
// scene E and left the FUDGE DETECTOR's tolerance wider than its own reproducibility.
//
// Sorting each bucket by particle index makes the accumulation sequence a function of the data alone.
// The 27-cell walk in `cs_forces` is already a fixed nesting, so with sorted buckets the whole reduction
// is canonical. Insertion sort: `bucket_k` is 16, buckets are usually far shorter, and it is branch-cheap
// and stable on the near-sorted input the previous frame tends to leave.
//
// NOT fixed here, and detectable rather than assumed: if a cell OVERFLOWS `bucket_k`, which particles
// won slots is still race-decided, so the SET itself varies. `grid_count` keeps the true (unclamped)
// count, so a caller can check for it — `gpu-verify` does.
@compute @workgroup_size(64)
fn cs_grid_sort(@builtin(global_invocation_id) gid : vec3<u32>) {
    let cell = gid.x;
    if (cell > P.table_mask) { return; }
    let n = min(atomicLoad(&grid_count[cell]), P.bucket_k);
    if (n < 2u) { return; }
    let base = cell * P.bucket_k;
    for (var a = 1u; a < n; a = a + 1u) {
        let key = grid_bucket[base + a];
        var b = a;
        loop {
            if (b == 0u) { break; }
            let prev = grid_bucket[base + b - 1u];
            if (prev <= key) { break; }
            grid_bucket[base + b] = prev;
            b = b - 1u;
        }
        grid_bucket[base + b] = key;
    }
}

// --- pass 3: accumulate contact forces from the 27 neighbouring cells -----------------------------
@compute @workgroup_size(64)
fn cs_forces(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= P.count) { return; }
    let pi = particles[i].offset;
    let vi = particles[i].vel;
    let r_i = particles[i].radius;
    let base = cell_of(pi);
    var acc = zero_accum();
    // HEADROOM: the free gap (m) to the nearest grain resting ABOVE this one, measured along the terrain
    // surface normal. The terrain position projection (cs_integrate) is capped by this so it can push a
    // buried grain up only into EMPTY space, never THROUGH the grain resting on it — the honest fix for
    // the stack-ram. Without it, projecting a grain the surface rose under drives it into its stack and
    // the (energy-conserving) grain-grain contact launches the pile. A grain with something resting on it
    // (headroom ≈ 0) instead stays put — transiently a little embedded — until it de-resolves or the pile
    // above shifts: the ground can't teleport a rigid stack upward. 1e30 ⇒ open sky above.
    let tn = normalize(vec3<f32>(-terrain_surface(pi).y, 1.0, -terrain_surface(pi).z));
    var headroom = 1.0e30;
    for (var dz = -1; dz <= 1; dz = dz + 1) {
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            for (var dx = -1; dx <= 1; dx = dx + 1) {
                let h = hash_cell(base + vec3<i32>(dx, dy, dz));
                let n = min(atomicLoad(&grid_count[h]), P.bucket_k);
                for (var s = 0u; s < n; s = s + 1u) {
                    let j = grid_bucket[h * P.bucket_k + s];
                    if (j == i) { continue; }
                    let pj = particles[j].offset;
                    let c = contact_accel(pi, vi, r_i, pj, particles[j].vel, particles[j].radius);
                    acc.force = acc.force + c.force;
                    acc.s_diag = acc.s_diag + c.s_diag;
                    acc.s_off = acc.s_off + c.s_off;
                    acc.sv_nbr = acc.sv_nbr + c.sv_nbr;
                    // Is j resting above i (ahead along the outward normal)? If so, the free gap before
                    // pushing i into it is (distance − diameter), clamped ≥ 0. Conservative (uses the full
                    // centre distance), so it never lets the projection open a new overlap.
                    let dj = pj - pi;
                    if (dot(dj, tn) > 0.0) {
                        headroom = min(headroom, max(length(dj) - (r_i + particles[j].radius), 0.0));
                    }
                }
            }
        }
    }
    // Terrain contact is NOT a force here. It is resolved as a non-injecting constraint (velocity clamp
    // + velocity-decoupled position projection) AFTER the grain-grain velocity solve, in cs_integrate —
    // see `terrain_resolve`. A penalty spring in the force sum was the settling-storm fudge (it stored
    // penetration and released it as launch KE). So this accumulation is grain-grain only.
    acc.headroom = headroom;
    forces[i] = acc;
}

// A heightfield column top in centered coords, clamped to the grid edge (so the terrain extends flat
// past the world border — no void that would inject huge PE). The mesh iso-surface sits 0.5 below the
// air voxel, hence the −0.5.
fn terrain_top(cx : i32, cz : i32) -> f32 {
    let x = clamp(cx, 0, i32(P.world_w) - 1);
    let z = clamp(cz, 0, i32(P.world_d) - 1);
    return f32(heightfield[u32(z) * P.world_w + u32(x)]) - P.center.y - 0.5;
}

// The smooth (bilinear) terrain surface height at a position — the SAME surface `terrain_resolve` collides
// against, factored out so the integrate step can tell whether a grain is grounded (for the settle
// counter). Clamped to the patch edge exactly like `terrain_top`.
fn terrain_h(pos : vec3<f32>) -> f32 {
    let vx = pos.x + P.center.x;
    let vz = pos.z + P.center.z;
    let cx = i32(floor(vx));
    let cz = i32(floor(vz));
    let h00 = terrain_top(cx, cz);
    let h10 = terrain_top(cx + 1, cz);
    let h01 = terrain_top(cx, cz + 1);
    let h11 = terrain_top(cx + 1, cz + 1);
    let fx = vx - f32(cx);
    let fz = vz - f32(cz);
    return mix(mix(h00, h10, fx), mix(h01, h11, fx), fz);
}

// The bilinear terrain surface height AND its horizontal gradient at a position. `xyz` unused; returns
// vec3(h, ∂h/∂x, ∂h/∂z). The surface is the four surrounding column tops bilinearly interpolated (the
// SAME field `terrain_h` samples) — a continuous height, so the outward normal (−∂h/∂x, 1, −∂h/∂z) never
// flips at voxel edges.
fn terrain_surface(pos : vec3<f32>) -> vec3<f32> {
    let vx = pos.x + P.center.x;
    let vz = pos.z + P.center.z;
    let cx = i32(floor(vx));
    let cz = i32(floor(vz));
    let h00 = terrain_top(cx, cz);
    let h10 = terrain_top(cx + 1, cz);
    let h01 = terrain_top(cx, cz + 1);
    let h11 = terrain_top(cx + 1, cz + 1);
    let fx = vx - f32(cx);
    let fz = vz - f32(cz);
    let h = mix(mix(h00, h10, fx), mix(h01, h11, fx), fz);
    let dhdx = mix(h10 - h00, h11 - h01, fz);
    let dhdz = mix(h01 - h00, h11 - h10, fx);
    return vec3<f32>(h, dhdx, dhdz);
}

// Per-substep bound on the GEOMETRIC position projection out of the terrain (metres). This is a solver
// RELAXATION rate, NOT a physical dial: the rest state (penetration → 0) is independent of it; it only
// caps how fast residual penetration is walked out. It exists so a surface that jumps a whole voxel under
// a STACK (a de-resolution deposit, or a grain buried by a wall) relaxes over several substeps instead of
// being teleported out in one — a one-shot teleport would open a metre-scale overlap with the grains
// resting ABOVE the buried one and re-launch them through the (stiff) grain-grain contact. Chosen so the
// induced per-substep grain-grain overlap stays well inside the settling regime (verified insensitive
// across [0.002, 0.05] m in gpu-verify's SURFACE-STEP sweep — a wide stable basin, not a tuned edge).
const MAX_SURFACE_CORRECTION : f32 = 0.01;

// Terrain contact as a NON-INJECTING constraint (the honest fix for the settling storm). The terrain is
// solid matter summarised as a per-column heightfield. The OLD law was a one-sided stiff penalty spring
// F = k·penetration: fine for a grain pressing DOWN under its own weight, but whenever penetration
// appeared from a change in the SURFACE (a de-resolution deposit stepping the column up under a resting
// neighbour, or a grain shoved against a steep wall) it released ½k·pen² as launch KE ≈ √k·pen ≈ 707·pen
// m/s — the deposit-/wall-kick that drove the km-scale settling storm. A spring STORES penetration and
// releases it; that is the fudge.
//
// Here contact is resolved at the CONSTRAINT level so it can NEVER increase kinetic energy:
//   1. VELOCITY — remove only the component of velocity going INTO the surface (vn<0 ⇒ set vn:=0). This
//      is the exact normal-constraint impulse Jn = max(0,−vn) ≥ 0; it can only REMOVE kinetic energy
//      (→ heat), never add it, whatever the penetration depth. It also SUPPORTS a resting grain: the
//      per-substep gravity increment (−g·dt into the surface) is zeroed every step, so the grain rests
//      on the surface and on slopes rather than sinking through.
//   2. FRICTION — Coulomb kinetic friction opposing tangential slip, bounded by μ·Jn (the normal impulse
//      just applied). A grain pressed harder — by a faster fall, or by the weight of the pile above it
//      transmitted as downward velocity — gets proportionally more friction: the honest μ·N law, purely
//      dissipative, no tuned coefficient.
//   3. POSITION — reconcile the residual penetration by a GEOMETRIC projection along the surface normal
//      that writes NO velocity. Moving a grain to the surface without touching its velocity injects zero
//      KE regardless of how far the surface moved (it raises gravitational PE by exactly the work the
//      ground did lifting it — real support, not a launch). Bounded by MAX_SURFACE_CORRECTION so it stays
//      stack-safe. Returns the corrected velocity in .xyz-of-vel and the position delta.
struct TerrainHit {
    dvel : vec3<f32>, // velocity AFTER the constraint (into-surface removed + friction)
    dpos : vec3<f32>, // position correction along the surface normal (bounded, no velocity written)
    hit  : f32,       // 1.0 if in contact, else 0.0
};
fn terrain_resolve(pos : vec3<f32>, vel : vec3<f32>, r_self : f32, headroom : f32) -> TerrainHit {
    let s = terrain_surface(pos);
    let penetration = s.x - (pos.y - r_self);
    if (penetration <= 0.0) {
        return TerrainHit(vel, vec3<f32>(0.0), 0.0);
    }
    let n = normalize(vec3<f32>(-s.y, 1.0, -s.z)); // outward surface normal (continuous, never flips)
    var v = vel;
    // 1. Normal: remove into-surface velocity. Jn = the impulse magnitude applied (≥ 0).
    let vn = dot(v, n);
    var jn = 0.0;
    if (vn < 0.0) {
        jn = -vn;
        v = v + jn * n; // clamp the into-surface component to 0 — dissipative, never a rebound
    }
    // 2. Friction: oppose tangential slip, bounded by μ·Jn (kinetic Coulomb). Can only halt slip.
    let v_t = v - dot(v, n) * n;
    let vt_mag = length(v_t);
    if (vt_mag > 1.0e-9) {
        let dv = min(P.c_friction * jn, vt_mag);
        v = v - (v_t / vt_mag) * dv;
    }
    // 3. Position projection out of the surface — velocity-decoupled, bounded per substep AND by the
    // headroom to the grain resting above (so a buried grain is never rammed up into its stack; the
    // energy-conserving grain-grain contact would otherwise launch the pile). Velocity-decoupled ⇒ zero
    // KE injected regardless of how far the surface moved.
    let dpos = min(min(penetration, MAX_SURFACE_CORRECTION), headroom) * n;
    return TerrainHit(v, dpos, 1.0);
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

    // Cool toward ambient (Newton's law of cooling), in energy: u relaxes toward u_ambient = c·300 K. EVERY
    // particle steps every frame — a settled grain keeps cooling AND re-checks its support / neighbours.
    let u_amb = P.specific_heat * 300.0;
    pt.u = u_amb + (pt.u - u_amb) * exp(-P.cool_rate * P.dt);

    // DIRECTIONAL TRAPEZOIDAL contact solve (docs/24 Stage 0 + Stage 1): solve (I + S)·v_new = (I − S)·v
    // + dt·a, where S = Σ [(dt²/4)·k + (dt/2)·c]·(n⊗n) is the per-grain contact Jacobian along the contact
    // normals ONLY. This is the implicit-midpoint (trapezoidal) rule — A-stable, so high coordination is
    // safe, but ENERGY-CONSERVING (Cayley transform, spectral radius ≤ 1), unlike backward-Euler which
    // dissipated the rebound to zero restitution. A grain with NO contacts gets S = 0 ⇒ M = I ⇒ pure
    // explicit, keeping its full free-flight/ejection velocity. Now a compressed contact RETURNS its
    // stored energy — restitution is real and set by the material's damping c (docs/24 Stage 1).
    let acc = forces[i];
    // AERODYNAMIC DRAG — a force from a real medium, not a velocity multiply.
    //   F = 1/2 rho_air v^2 C_d A ,  A = s^2 ,  m = rho_grain s^3  =>  a = 1/2 rho_air v^2 C_d / (rho_grain s)
    // so the grain's OWN density (docs/38's `rho`) and size set how much the air can push it: a dense iron
    // grain and a snow grain of the same size are slowed very differently, which is the physical answer.
    // rho_air is one value for the whole patch: over 96 m the barometric profile varies 1.1%, so resolving
    // it here would buy nothing (docs/44 — resolution by necessity, applied to the air itself). Altitude
    // dependence belongs where altitude actually varies: re-entry and the orbit band.
    var a = P.gravity + acc.force;
    let sp = length(pt.vel);
    if (P.air_rho > 0.0 && sp > 1.0e-9) {
        let s_grain = max(2.0 * pt.radius, 1.0e-6);
        let a_drag = 0.5 * P.air_rho * sp * sp * P.drag_cd / (max(pt.rho, 1.0e-6) * s_grain);
        a = a - (a_drag / sp) * pt.vel;   // opposes motion; can only remove energy
    }
    // RHS = (I − S)·v + dt·a. The −S·v term (symmetric tensor · old velocity) is the trapezoidal half
    // that makes the scheme conservative rather than dissipative.
    let sv = vec3<f32>(
        acc.s_diag.x * pt.vel.x + acc.s_off.x * pt.vel.y + acc.s_off.y * pt.vel.z,
        acc.s_off.x * pt.vel.x + acc.s_diag.y * pt.vel.y + acc.s_off.z * pt.vel.z,
        acc.s_off.y * pt.vel.x + acc.s_off.z * pt.vel.y + acc.s_diag.z * pt.vel.z,
    );
    // rhs = v − ρ·S·v + (1/θ)·Σ(S_contact·v_neighbor) + dt·a. The neighbor-coupling term is what makes
    // the pair's COM velocity survive the solve (momentum conservation, gpu-verify F5). ρ = (1−θ)/θ,
    // and 1+ρ = 1/θ.
    let rhs = pt.vel - THETA_RHO * sv + (1.0 / THETA) * acc.sv_nbr + a * P.dt;
    // M = I + S (symmetric positive-definite; S already carries the dt², dt factors).
    let m00 = 1.0 + acc.s_diag.x;
    let m11 = 1.0 + acc.s_diag.y;
    let m22 = 1.0 + acc.s_diag.z;
    let m01 = acc.s_off.x;
    let m02 = acc.s_off.y;
    let m12 = acc.s_off.z;
    // Solve via the symmetric 3×3 inverse (det > 0 since M is PD).
    let c00 = m11 * m22 - m12 * m12;
    let c01 = m02 * m12 - m01 * m22;
    let c02 = m01 * m12 - m02 * m11;
    let c11 = m00 * m22 - m02 * m02;
    let c12 = m01 * m02 - m00 * m12;
    let c22 = m00 * m11 - m01 * m01;
    let inv_det = 1.0 / (m00 * c00 + m01 * c01 + m02 * c02);
    var vel = vec3<f32>(
        (c00 * rhs.x + c01 * rhs.y + c02 * rhs.z) * inv_det,
        (c01 * rhs.x + c11 * rhs.y + c12 * rhs.z) * inv_det,
        (c02 * rhs.x + c12 * rhs.y + c22 * rhs.z) * inv_det,
    );
    // Trapezoidal position update: pos += (dt/2)(v_old + v_new) — consistent with the midpoint velocity
    // solve above (a symplectic-Euler pos += v_new·dt would break the energy conservation the solve buys).
    var pos = pt.offset + (pt.vel + vel) * (0.5 * P.dt);

    // TERRAIN CONTACT (non-injecting constraint — the settling-storm fix). Grain-grain contact is the
    // momentum-conserving implicit solve above; the terrain is an infinite-mass boundary resolved AFTER
    // it, by a velocity clamp (removes only into-surface velocity ⇒ never adds KE, and supports a resting
    // grain) plus a velocity-decoupled geometric position projection (reconciles penetration without
    // writing velocity ⇒ still no KE, even when the SURFACE jumped under the grain). No penalty spring,
    // so no store-and-release launch. See `terrain_resolve`.
    let th = terrain_resolve(pos, vel, pt.radius, acc.headroom);
    if (th.hit > 0.5) {
        vel = th.dvel;
        pos = pos + th.dpos;
    }

    // NOTE on energy: the contact damping + friction here REMOVE kinetic energy. Physically that energy
    // is not destroyed — it becomes HEAT in the grains (→ temp_k) and radiates to space. We drop it for
    // now (flagged); routing it into temperature matters once we do phase change (steam/boiling). The
    // enforceable invariant today: this step never CREATES mechanical energy (docs/23).

    pt.offset = pos;
    pt.vel = vel;
    // Count consecutive GROUNDED substeps into `resting`. This is the GPU port of the CPU
    // `matter::step` SETTLE_FRAMES fallback: a grain sitting on the terrain surface but still creeping
    // (soft contact leaves a small residual horizontal speed above SETTLE_SPEED) would otherwise never
    // read as "at rest" — so the CPU deposits any grain grounded for enough frames regardless of speed.
    // The de-resolution readback (lib.rs) deposits a grounded grain once this counter crosses its
    // threshold OR its horizontal speed is below SETTLE_SPEED — the SAME dual criterion as the CPU.
    if (pos.y - pt.radius <= terrain_h(pos) + 0.1) {
        pt.resting = pt.resting + 1.0;
    } else {
        pt.resting = 0.0;
    }
    pt.emission = incandescence(grain_temp(pt.u));
    particles[i] = pt;
}
