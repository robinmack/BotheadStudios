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
    temp     : f32,
    vel      : vec3<f32>,
    resting  : f32,
    color    : vec3<f32>,
    material : f32,
    emission : vec3<f32>,
    _pad     : f32,
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
    _p0    : f32,
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
    return Accum(vec3<f32>(0.0), 0.0, vec3<f32>(0.0), 0.0, vec3<f32>(0.0), 0.0, vec3<f32>(0.0), 0.0);
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

fn contact_accel(pi : vec3<f32>, vi : vec3<f32>, pj : vec3<f32>, vj : vec3<f32>) -> Accum {
    var acc = zero_accum();
    let d = pi - pj;
    let dist = length(d);
    let touch = 2.0 * P.c_radius;
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

// --- pass 3: accumulate contact forces from the 27 neighbouring cells -----------------------------
@compute @workgroup_size(64)
fn cs_forces(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= P.count) { return; }
    let pi = particles[i].offset;
    let vi = particles[i].vel;
    let base = cell_of(pi);
    var acc = zero_accum();
    for (var dz = -1; dz <= 1; dz = dz + 1) {
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            for (var dx = -1; dx <= 1; dx = dx + 1) {
                let h = hash_cell(base + vec3<i32>(dx, dy, dz));
                let n = min(atomicLoad(&grid_count[h]), P.bucket_k);
                for (var s = 0u; s < n; s = s + 1u) {
                    let j = grid_bucket[h * P.bucket_k + s];
                    if (j == i) { continue; }
                    let c = contact_accel(pi, vi, particles[j].offset, particles[j].vel);
                    acc.force = acc.force + c.force;
                    acc.s_diag = acc.s_diag + c.s_diag;
                    acc.s_off = acc.s_off + c.s_off;
                    acc.sv_nbr = acc.sv_nbr + c.sv_nbr;
                }
            }
        }
    }
    let t = terrain_accel(pi, vi); // the terrain is matter too — same contact law
    acc.force = acc.force + t.force;
    acc.s_diag = acc.s_diag + t.s_diag;
    acc.s_off = acc.s_off + t.s_off;
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

// The smooth (bilinear) terrain surface height at a position — the SAME surface `terrain_accel` collides
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

// Terrain contact as a CONSERVATIVE penalty (docs/24 Path B — the honest, Newtonian way). The terrain is
// solid matter summarised as a per-column heightfield; a grain that sinks below the surface feels a
// repulsive force. The OLD min-translation normal FLIPPED between up and sideways at voxel edges — a
// discontinuous force that is NOT the gradient of any potential, so it pumped energy (the crater "free
// energy", and it was masked by the `drag` fudge). Here we build a SMOOTH surface instead: bilinear-
// interpolate the four surrounding column tops into a continuous height h(x,z), and make the penalty the
// EXACT gradient of U = ½·k·penetration²  ⇒  F = k·penetration·(−∂h/∂x, 1, −∂h/∂z). That is −∇U by
// construction, so it CONSERVES energy (no flip, no injection). Normal damping is implicit (the tensor);
// Coulomb friction is explicit, clamped so it can only halt slip. Returns an Accum.
fn terrain_accel(pos : vec3<f32>, vel : vec3<f32>) -> Accum {
    var acc = zero_accum();
    let vx = pos.x + P.center.x;
    let vz = pos.z + P.center.z;
    let cx = i32(floor(vx));
    let cz = i32(floor(vz));
    // Bilinear surface over the cell (cx,cz)–(cx+1,cz+1) from the four corner column tops.
    let h00 = terrain_top(cx, cz);
    let h10 = terrain_top(cx + 1, cz);
    let h01 = terrain_top(cx, cz + 1);
    let h11 = terrain_top(cx + 1, cz + 1);
    let fx = vx - f32(cx);
    let fz = vz - f32(cz);
    let h = mix(mix(h00, h10, fx), mix(h01, h11, fx), fz); // smooth surface height
    let dhdx = mix(h10 - h00, h11 - h01, fz);              // ∂h/∂x
    let dhdz = mix(h01 - h00, h11 - h10, fx);              // ∂h/∂z
    let bottom = pos.y - P.part_half;
    let penetration = h - bottom;
    if (penetration <= 0.0) { return acc; } // above the smooth surface — no contact

    // F = −∇U = k·penetration·grad, grad = (−∂h/∂x, 1, −∂h/∂z). Conservative by construction.
    let grad = vec3<f32>(-dhdx, 1.0, -dhdz);
    let n = normalize(grad);
    let f_mag = P.c_stiffness * penetration;      // scalar spring magnitude along +y-of-U
    let normal_load = f_mag * length(grad);       // |F|, the real normal load for Coulomb friction
    let vn = dot(vel, n);
    let v_t = vel - n * vn;
    let vt_mag = length(v_t);
    var a_t = vec3<f32>(0.0);
    if (vt_mag > 1.0e-9) {
        // Clamped at vt_mag/dt so friction can only halt the slip, never reverse it (see contact_accel).
        let fmag = min(min(P.c_tangent_damp * vt_mag, P.c_friction * normal_load), vt_mag / P.dt);
        a_t = -(v_t / vt_mag) * fmag;
    }
    acc.force = f_mag * grad + a_t; // spring is the exact −∇U; friction is tangential
    accum_tensor(&acc, n, THETA * THETA * P.dt * P.dt * P.c_stiffness + THETA * P.dt * P.c_normal_damp);
    return acc;
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

    // DIRECTIONAL TRAPEZOIDAL contact solve (docs/24 Stage 0 + Stage 1): solve (I + S)·v_new = (I − S)·v
    // + dt·a, where S = Σ [(dt²/4)·k + (dt/2)·c]·(n⊗n) is the per-grain contact Jacobian along the contact
    // normals ONLY. This is the implicit-midpoint (trapezoidal) rule — A-stable, so high coordination is
    // safe, but ENERGY-CONSERVING (Cayley transform, spectral radius ≤ 1), unlike backward-Euler which
    // dissipated the rebound to zero restitution. A grain with NO contacts gets S = 0 ⇒ M = I ⇒ pure
    // explicit, keeping its full free-flight/ejection velocity. Now a compressed contact RETURNS its
    // stored energy — restitution is real and set by the material's damping c (docs/24 Stage 1).
    let acc = forces[i];
    let a = P.gravity + acc.force;
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
    let vel = vec3<f32>(
        (c00 * rhs.x + c01 * rhs.y + c02 * rhs.z) * inv_det,
        (c01 * rhs.x + c11 * rhs.y + c12 * rhs.z) * inv_det,
        (c02 * rhs.x + c12 * rhs.y + c22 * rhs.z) * inv_det,
    ) * P.drag;
    // Trapezoidal position update: pos += (dt/2)(v_old + v_new) — consistent with the midpoint velocity
    // solve above (a symplectic-Euler pos += v_new·dt would break the energy conservation the solve buys).
    let pos = pt.offset + (pt.vel + vel) * (0.5 * P.dt);

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
    if (pos.y - P.part_half <= terrain_h(pos) + 0.1) {
        pt.resting = pt.resting + 1.0;
    } else {
        pt.resting = 0.0;
    }
    pt.emission = incandescence(pt.temp);
    particles[i] = pt;
}
