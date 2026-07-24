// GPU SPH-EOS-gravity force kernel (docs/33 stage 4). The space-band self-gravitating condensed-matter step
// (`hydrostatic.rs`) as a WGSL compute shader, so a giant impact can run at N~10^5 — the resolution the
// isotopic-crisis number (and accretion) need. SAME physics as the CPU `HydroBody::forces_and_dudt`, in f32:
//   • SPH density   ρ_i = Σ_j m_j W(r_ij, h_ij)                        (cubic spline, per-pair h_ij=½(h_i+h_j))
//   • Tillotson EOS  P_i = P(ρ_i, u_i)                                  (per-material; matches eos.rs)
//   • pressure force a_i = −Σ_j m_j (P_i/ρ_i² + P_j/ρ_j² + Π_ij) ∇W    (Monaghan artificial viscosity Π)
//   • self-gravity   a_i += Σ_j G m_j d/(|d|²+ε²)^{3/2}
//   • energy         du_i/dt = ½ Σ_j m_j (…) (v_i−v_j)·∇W
//
// The SHORT-RANGE SPH (density/pressure/AV) uses a SPATIAL HASH GRID (stage 4b) — each particle scans only
// the 27 neighbouring cells, so it is O(N) not O(N²). The grid is EXACT (cell_size = the max smoothing
// length ⇒ every pair within h_ij lands in the 27-cell neighbourhood; bucket_k ≫ particles-per-cell ⇒ none
// dropped) — like the CPU `neighbors.rs`, verified by tools/sph-verify (the gridded output still matches the
// O(N²) CPU physics to f32 precision). LONG-RANGE self-gravity stays direct O(N²) here — GPU-tiled direct
// summation is tractable at these N; a Barnes–Hut tree (CPU has one in bhtree.rs) is the further optimization
// if profiling at 10^5 demands it. The KDK integration loop (cs_kick_drift/cs_kick, stage 4c.1) is BELOW;
// adaptive Courant dt + scene wiring are stage 4c.2+/5. VERIFIED on the RTX 2070 (tools/sph-verify) to f32
// precision — the force kernel per-eval AND the integrator over 50 steps vs the CPU HydroBody::step leapfrog.

const PI: f32 = 3.14159265359;
const G: f32 = 6.674e-11;

struct Params {
  n: u32,
  softening: f32,
  av_alpha: f32,
  av_beta: f32,
  cell_size: f32,   // = the max smoothing length (so the 27-cell scan is exact)
  table_mask: u32,  // hash table size − 1 (power of two)
  bucket_k: u32,    // max particles stored per cell
  dt: f32,          // integration timestep for cs_kick_drift/cs_kick/cs_relax (KDK leapfrog, stage 4c.1)
  damp: f32,        // velocity damping for cs_relax (settle to hydrostatic equilibrium, stage 4c.2)
  omega: f32,       // cs_relax ONLY: rigid-rotation rate (rad/s) about +z for a ROTATING-frame relaxation
                    // (adds centrifugal ω²·(x,y,0) so the body settles to its oblate equilibrium). 0 elsewhere.
  n_ext: u32,       // number of DE-RESOLVED bodies in `ext_mass` (docs/44). 0 ⇒ the channel is inert.
  merge_budget: u32, // docs/08/44 coarsening: merge redundant pairs while `n` exceeds this. 0 ⇒ never.
  // The un-resolved BULK planet a particalized CAP rests on (docs/39 — resolution-on-demand). A small
  // impactor resolves only a cap of the planet; its 10²⁴ kg interior stays this abstract sphere — a Gauss
  // gravity source + a non-injecting floor. `bulk_cr.w` (R_core) <= 0 DISABLES the bulk (a whole-body
  // impact, e.g. birth, resolves every body and needs none). Centre/vel are in the SPH's working frame.
  bulk_cr: vec4<f32>,  // xyz = centre, w = core radius R_core
  bulk_vm: vec4<f32>,  // xyz = velocity, w = mass
}

struct Particle {
  pos: vec3<f32>, h: f32,
  vel: vec3<f32>, u: f32,
  mass: f32, mat: u32, rho: f32, prov: u32, // prov: provenance tag (0=Earth, 1=Theia) — survives the round-trip
}
struct Eos {
  rho0: f32, a: f32, b: f32, cap_a: f32,
  cap_b: f32, e0: f32, e_iv: f32, e_cv: f32,
  alpha: f32, beta: f32, _p0: f32, _p1: f32,
}

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(2) var<storage, read> eos: array<Eos>;
@group(0) @binding(3) var<storage, read_write> acc: array<vec3<f32>>;
@group(0) @binding(4) var<storage, read_write> dudt: array<f32>;
@group(0) @binding(5) var<storage, read_write> grid_count: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read_write> grid_bucket: array<u32>;
@group(0) @binding(7) var<storage, read_write> signal: array<f32>; // Courant signal h/(c+|v|); CPU min→dt (4c.2)
@group(0) @binding(8) var<storage, read> ext_mass: array<vec4<f32>>; // de-resolved bodies: xyz = pos, w = mass
@group(0) @binding(9) var<storage, read_write> merge_target: array<u32>; // i → its absorber, or NO_MERGE

fn cell_of(pos: vec3<f32>) -> vec3<i32> { return vec3<i32>(floor(pos / P.cell_size)); }
fn hash_cell(c: vec3<i32>) -> u32 {
  let h = (u32(c.x) * 73856093u) ^ (u32(c.y) * 19349663u) ^ (u32(c.z) * 83492791u);
  return h & P.table_mask;
}

fn sph_w(r: f32, h: f32) -> f32 {
  let q = r / h;
  let sig = 8.0 / (PI * h * h * h);
  if (q < 0.5) { return sig * (1.0 - 6.0 * q * q + 6.0 * q * q * q); }
  if (q < 1.0) { let t = 1.0 - q; return sig * 2.0 * t * t * t; }
  return 0.0;
}
fn sph_dw(r: f32, h: f32) -> f32 {
  let q = r / h;
  let sig = 8.0 / (PI * h * h * h);
  if (q < 0.5) { return sig * (-12.0 * q + 18.0 * q * q) / h; }
  if (q < 1.0) { let t = 1.0 - q; return sig * (-6.0 * t * t) / h; }
  return 0.0;
}
fn pressure(e: Eos, rho: f32, u: f32) -> f32 {
  let r = max(rho, 1.0e-9);
  let eta = r / e.rho0;
  let mu = eta - 1.0;
  let omega = u / (e.e0 * eta * eta) + 1.0;
  let p_c = (e.a + e.b / omega) * r * u + e.cap_a * mu + e.cap_b * mu * mu;
  if (eta >= 1.0 || u <= e.e_iv) { return p_c; }
  let z = e.rho0 / r - 1.0;
  let p_e = e.a * r * u + (e.b * r * u / omega + e.cap_a * mu * exp(-e.beta * z)) * exp(-e.alpha * z * z);
  if (u >= e.e_cv) { return p_e; }
  return ((u - e.e_iv) * p_e + (e.e_cv - u) * p_c) / (e.e_cv - e.e_iv);
}
fn dfdu(e: Eos, rho: f32, u: f32) -> f32 {
  let du = abs(u) * 1.0e-3 + 1.0;
  return (pressure(e, rho, u + du) - pressure(e, rho, u - du)) / (2.0 * du);
}
fn sound_speed(e: Eos, rho: f32, u: f32) -> f32 {
  let r = max(rho, 1.0e-9);
  let dr = r * 1.0e-3;
  let dp = (pressure(e, r + dr, u) - pressure(e, r - dr, u)) / (2.0 * dr);
  let p = pressure(e, r, u);
  return sqrt(max(dp + p / (r * r) * dfdu(e, r, u), 0.0));
}

// Gravity of the un-resolved bulk planet (docs/39): monopole G·M/r² outside R_core, Gauss-LINEAR interior
// (∝r, →0 at the centre). A raw 1/r² singularity-sucks any particle that penetrates R_core (39b lesson 1),
// so the interior branch is mandatory. `R_core <= 0` ⇒ no bulk (returns 0), the whole-body impact.
// Bulk modes (docs/39), selected by `bulk_cr.w` so ONE shader serves every scale (Law II): w == 0 ⇒ no bulk;
// w > 0 ⇒ SPHERICAL (a planet cap, this fn) with centre `bulk_cr.xyz`, R_core `bulk_cr.w`, mass `bulk_vm.w`;
// w < 0 ⇒ PLANAR (a flat terrestrial ground, meter scale) — a plane through `bulk_cr.xyz` with up-normal
// `bulk_vm.xyz` and uniform gravity `bulk_vm.w` (a big planet is locally flat, and a patch-local frame keeps
// f32 precision where the 1 m grains are — the alternative, a huge-R sphere, forms ~6.4e6 f32 coords and
// loses 0.5 m ULP; docs/39 open-decision #4).
fn bulk_gravity(pos: vec3<f32>) -> vec3<f32> {
  let R = P.bulk_cr.w;
  if (R == 0.0) { return vec3<f32>(0.0); }
  if (R < 0.0) {
    // PLANAR: uniform gravity, straight down (−up), magnitude `bulk_vm.w`.
    return -normalize(P.bulk_vm.xyz) * P.bulk_vm.w;
  }
  let d = pos - P.bulk_cr.xyz;   // centre → particle
  let r = length(d);
  if (r < 1.0) { return vec3<f32>(0.0); }
  let M = P.bulk_vm.w;
  let gg = select(G * M * r / (R * R * R), G * M / (r * r), r >= R);
  return -(d / r) * gg;          // toward the centre
}

// The bulk's hard surface (docs/39 39b): a particle cannot sink into the rigid bulk. A penetrator is
// projected to R_core and its INWARD velocity RELATIVE to the bulk removed — a non-injecting position
// constraint, NOT a penalty spring (a spring releases ½k·pen² as launch KE: the terrain-contact lesson).
// Rigid bulk (no recoil yet — negligible for a Moon on Earth; the bulk-recoil is docs/39 39c, flagged).
fn apply_bulk_floor(i: u32) {
  let R = P.bulk_cr.w;
  if (R == 0.0) { return; }
  if (R < 0.0) {
    // PLANAR floor: the ground plane through `bulk_cr.xyz` with up-normal `bulk_vm.xyz`. A particle that has
    // sunk below it is lifted back to the plane and its DOWNWARD (into-floor) velocity removed — the same
    // non-injecting constraint as the sphere, no bounce. The static ground has no velocity to credit.
    let up = normalize(P.bulk_vm.xyz);
    let depth = dot(particles[i].pos - P.bulk_cr.xyz, up);
    if (depth < 0.0) {
      particles[i].pos = particles[i].pos - up * depth; // project up onto the plane
      let vn = dot(particles[i].vel, up);
      if (vn < 0.0) { particles[i].vel = particles[i].vel - up * vn; }
    }
    return;
  }
  let d = particles[i].pos - P.bulk_cr.xyz;
  let r = length(d);
  if (r >= R || r < 1.0) { return; }
  let n = d / r;
  particles[i].pos = P.bulk_cr.xyz + n * R;
  let vn = dot(particles[i].vel - P.bulk_vm.xyz, n);
  if (vn < 0.0) { particles[i].vel = particles[i].vel - n * vn; }
}

// --- grid build ---
@compute @workgroup_size(64)
fn cs_grid_clear(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i > P.table_mask) { return; }
  atomicStore(&grid_count[i], 0u);
}
@compute @workgroup_size(64)
fn cs_grid_insert(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let h = hash_cell(cell_of(particles[i].pos));
  let slot = atomicAdd(&grid_count[h], 1u);
  if (slot < P.bucket_k) { grid_bucket[h * P.bucket_k + slot] = i; }
}

// PASS: SPH density over the 27 neighbouring cells (exact). O(N).
@compute @workgroup_size(64)
fn cs_density(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let pi = particles[i];
  var rho = pi.mass * sph_w(0.0, pi.h);
  let ci = cell_of(pi.pos);
  for (var dx: i32 = -1; dx <= 1; dx++) {
    for (var dy: i32 = -1; dy <= 1; dy++) {
      for (var dz: i32 = -1; dz <= 1; dz++) {
        let hh = hash_cell(ci + vec3<i32>(dx, dy, dz));
        let cnt = min(atomicLoad(&grid_count[hh]), P.bucket_k);
        let cell = ci + vec3<i32>(dx, dy, dz);
        for (var s: u32 = 0u; s < cnt; s++) {
          let j = grid_bucket[hh * P.bucket_k + s];
          if (j == i) { continue; }
          let pj = particles[j];
          // Cell-membership guard: a bucket may hold particles from a DIFFERENT cell (hash collision).
          // Only count j when scanning ITS cell — so each neighbour is counted exactly ONCE (no
          // double-count) and collided far particles are skipped. Makes the grid EXACT.
          let cj = cell_of(pj.pos);
          if (cj.x != cell.x || cj.y != cell.y || cj.z != cell.z) { continue; }
          let r = length(pi.pos - pj.pos);
          let hij = 0.5 * (pi.h + pj.h);
          if (r < hij) { rho += pj.mass * sph_w(r, hij); }
        }
      }
    }
  }
  particles[i].rho = rho;
}

// PASS: forces — direct-sum gravity (all N) + grid-neighbour SPH pressure/AV + du/dt.
@compute @workgroup_size(64)
fn cs_forces(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let pi = particles[i];
  let ei = eos[pi.mat];
  let p_i = pressure(ei, pi.rho, pi.u);
  let c_i = sound_speed(ei, pi.rho, pi.u);
  let s2 = P.softening * P.softening;
  var a = vec3<f32>(0.0);
  var de: f32 = 0.0;
  // long-range gravity: direct O(N²)
  for (var j: u32 = 0u; j < P.n; j++) {
    if (j == i) { continue; }
    let d = particles[j].pos - pi.pos;
    let r2 = dot(d, d);
    a += d * (G * particles[j].mass / pow(r2 + s2, 1.5));
  }
  // + the un-resolved bulk planet's gravity (docs/39 — no-op when no bulk is set)
  a += bulk_gravity(pi.pos);
  // + DE-RESOLVED bodies (docs/44): a clump whose members have united leaves the particle set and becomes
  // one orbit body, but its MASS must go on acting on the survivors — collapsing a body's internal
  // structure is a change of representation, and a representation change must never change what is true
  // (Law IV). Identical G, softening and 1/r² form as the direct sum above, so a survivor feels the same
  // force whether the clump is still resolved or has been collapsed. `n_ext` = 0 ⇒ no-op.
  for (var k: u32 = 0u; k < P.n_ext; k++) {
    let e = ext_mass[k];
    let d = e.xyz - pi.pos;
    let r2 = dot(d, d);
    a += d * (G * e.w / pow(r2 + s2, 1.5));
  }
  // short-range SPH pressure + AV: the 27 neighbouring cells (exact)
  let ci = cell_of(pi.pos);
  for (var dx: i32 = -1; dx <= 1; dx++) {
    for (var dy: i32 = -1; dy <= 1; dy++) {
      for (var dz: i32 = -1; dz <= 1; dz++) {
        let cell = ci + vec3<i32>(dx, dy, dz);
        let hh = hash_cell(cell);
        let cnt = min(atomicLoad(&grid_count[hh]), P.bucket_k);
        for (var s: u32 = 0u; s < cnt; s++) {
          let j = grid_bucket[hh * P.bucket_k + s];
          if (j == i) { continue; }
          let pj = particles[j];
          let cj = cell_of(pj.pos); // cell-membership guard (see cs_density): count each neighbour once
          if (cj.x != cell.x || cj.y != cell.y || cj.z != cell.z) { continue; }
          let dpos = pi.pos - pj.pos;
          let r = length(dpos);
          let hij = 0.5 * (pi.h + pj.h);
          if (r < hij && r > 1.0e-9) {
            let ej = eos[pj.mat];
            let p_j = pressure(ej, pj.rho, pj.u);
            let c_j = sound_speed(ej, pj.rho, pj.u);
            let dvel = pi.vel - pj.vel;
            let vr = dot(dvel, dpos);
            var pi_ij: f32 = 0.0;
            if (vr < 0.0) {
              let mu = hij * vr / (r * r + 0.01 * hij * hij);
              let c_bar = 0.5 * (c_i + c_j);
              let rho_bar = 0.5 * (pi.rho + pj.rho);
              pi_ij = (-P.av_alpha * c_bar * mu + P.av_beta * mu * mu) / rho_bar;
            }
            let coeff = p_i / (pi.rho * pi.rho) + p_j / (pj.rho * pj.rho) + pi_ij;
            let grad = (dpos / r) * sph_dw(r, hij);
            a += grad * (-coeff * pj.mass);
            de += 0.5 * pj.mass * coeff * dot(dvel, grad);
          }
        }
      }
    }
  }
  acc[i] = a;
  dudt[i] = de;
}

// --- KDK leapfrog integration (stage 4c.1) ---
// One dynamical step = TWO force evals with a half-kick+drift between and a half-kick after, matching the CPU
// `HydroBody::step` EXACTLY (energy-conserving; no damping). Internal energy is integrated alongside velocity
// (its rate du/dt is the pressure/AV work) and clamped u = max(u, 0) as the CPU does. Per step the host
// dispatches: clear→insert→density→forces → cs_kick_drift → clear→insert→density→forces → cs_kick.

// First half-kick (v, u) then DRIFT position. Reads acc/dudt from the FIRST force eval of the step.
@compute @workgroup_size(64)
fn cs_kick_drift(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let v = particles[i].vel + acc[i] * (0.5 * P.dt);
  particles[i].vel = v;
  particles[i].u = max(particles[i].u + dudt[i] * (0.5 * P.dt), 0.0);
  particles[i].pos = particles[i].pos + v * P.dt;
  apply_bulk_floor(i); // rest the cap on the un-resolved bulk (no-op when no bulk is set)
}

// Final half-kick (v, u). Reads acc/dudt from the SECOND force eval of the step.
@compute @workgroup_size(64)
fn cs_kick(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  particles[i].vel = particles[i].vel + acc[i] * (0.5 * P.dt);
  particles[i].u = max(particles[i].u + dudt[i] * (0.5 * P.dt), 0.0);
}

// --- Damped relaxation (stage 4c.2): settle a body to hydrostatic equilibrium before colliding it (an
// UNRELAXED body dumps startup non-equilibrium into the shock, tripling the energy — the 3a lesson). Matches
// the CPU `HydroBody::relax_step`: v = (v + a·dt)·damp; x += v·dt. Damping is numerical; the equilibrium
// (dP/dr = −ρg) is the physics. Internal energy is held fixed here (relaxation is mechanical). One relax step
// = ONE force eval (clear→insert→density→forces) then this kernel. Reads acc from that eval.
@compute @workgroup_size(64)
fn cs_relax(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  // Rotating-frame relaxation: add the centrifugal acceleration ω²·(x,y,0) so a spinning body settles to its
  // OBLATE equilibrium (Coriolis −2ω×v_frame → 0 as the damped frame velocity → 0, so it's omitted). ω=0
  // (the default for every non-spin caller) recovers the exact hydrostatic relaxation. The body is centred at
  // the origin during relaxation, so (x,y) is measured from its own spin axis.
  let p = particles[i].pos;
  let a_cf = P.omega * P.omega * vec3<f32>(p.x, p.y, 0.0);
  let v = (particles[i].vel + (acc[i] + a_cf) * P.dt) * P.damp;
  particles[i].vel = v;
  particles[i].pos = p + v * P.dt;
  apply_bulk_floor(i); // a cap relaxes seated on the bulk (no-op when no bulk is set)
}

// Per-particle Courant signal speed h_i/(c_i+|v_i|); the CPU reduces min·cfl → the adaptive dt (stage 4c.2).
// During a shock the material compresses and c_i rises steeply (Tillotson), so dt shrinks to stay stable —
// the fixed-dt version injected energy exactly because it didn't. Needs density (cs_density ran).
@compute @workgroup_size(64)
fn cs_signal(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let pi = particles[i];
  let c = sound_speed(eos[pi.mat], pi.rho, pi.u);
  signal[i] = pi.h / max(c + length(pi.vel), 1.0);
}

// ---------------------------------------------------------------------------------------------------
// DE-RESOLUTION BY PAIRWISE MERGE (Robin, 2026-07-23; docs/08 clump tier, docs/44 resolve-by-necessity)
//
// "If two sticky particles collide and now share a common position/vector, we merge them... the conjoined
// particles are on their way. When others collide they are added into the composition of the largest blob."
//
// This is the INVERSE of particalization done as a CONTACT LAW rather than a global search: local,
// incremental, and free of any read-back. A merged blob stays a particle, so it keeps gravitating and
// pressure-interacting with no special case — nothing has to be fed back in from outside.
//
// TWO gates, and BOTH are required:
//
//   1. REDUNDANT — the pair carries no more information than one particle would. `r < h_ij/2` is closer
//      than one interparticle spacing (`smoothing_for` sets h = 2·(m/ρ)^⅓, so h/2 IS the spacing): they
//      occupy the same lattice site and SPH cannot resolve structure between them. `|Δv| < c_s` is
//      subsonic relative motion: the fluid has already equilibrated across that gap. Both are the
//      material's own numbers, not dials.
//   2. NECESSARY — `P.n > P.merge_budget`. Similarity alone is NOT sufficient and this is the trap:
//      every pair inside a settled planet is within a smoothing length and mutually subsonic, so a pure
//      redundancy test would collapse the whole interior on the first frame. Merging is a response to
//      BUDGET PRESSURE (docs/08: "under budget pressure, clumps merge"), so detail is only ever spent
//      when it is not affordable — never merely because two particles look alike.
//
// RACE-FREEDOM by three passes with DISJOINT write sets. `pick` writes only `merge_target`; `apply` runs
// on ROOTS (targetless particles) and writes only roots; `retire` writes only non-roots. Since an absorbed
// particle has exactly one target and roots are by definition untargeted, no two invocations ever write
// the same particle, and no invocation reads a particle another is writing. Chains (i→j→k) simply resolve
// over successive frames instead of racing.
//
// CONSERVATION is exact: mass, momentum and centre of mass by construction, and the relative kinetic
// energy the inelastic merge destroys is added to `u` — the specific internal energy SPH already carries.
// It becomes heat, which is what an inelastic merge physically does, so total energy is unchanged.
//
// MATERIALS DO NOT BLEND: only same-material pairs merge (see `cs_merge_pick`), so the merged particle's
// EOS is exactly its constituents' and no mixture EOS is needed. That also means a coalescing body keeps
// its COMPOSITION — it converges to one particle per material rather than one homogeneous lump — which is
// what lets it still be promoted to a LAYERED body afterwards.
//
// FLAGGED (Law V): a merge discards the pair's angular momentum about their common centre — bounded by the
// redundancy gate (they are within a spacing and mutually subsonic), but not zero.
// ---------------------------------------------------------------------------------------------------

const NO_MERGE: u32 = 0xffffffffu;

fn merge_enabled() -> bool {
  return P.merge_budget > 0u && P.n > P.merge_budget;
}

// PASS: pick — each particle chooses the neighbour that will ABSORB it (heavier; ties by lower index).
@compute @workgroup_size(64)
fn cs_merge_pick(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  merge_target[i] = NO_MERGE;
  if (!merge_enabled()) { return; }
  let pi = particles[i];
  if (pi.mass <= 0.0) { return; }
  let ei = eos[pi.mat];
  let c_i = sound_speed(ei, pi.rho, pi.u);
  var best = NO_MERGE;
  var best_mass = pi.mass;
  let ci = cell_of(pi.pos);
  for (var dx: i32 = -1; dx <= 1; dx++) {
    for (var dy: i32 = -1; dy <= 1; dy++) {
      for (var dz: i32 = -1; dz <= 1; dz++) {
        let cell = ci + vec3<i32>(dx, dy, dz);
        let hh = hash_cell(cell);
        let cnt = min(atomicLoad(&grid_count[hh]), P.bucket_k);
        for (var s: u32 = 0u; s < cnt; s++) {
          let j = grid_bucket[hh * P.bucket_k + s];
          if (j == i) { continue; }
          let pj = particles[j];
          if (pj.mass <= 0.0) { continue; }
          let cj = cell_of(pj.pos);
          if (cj.x != cell.x || cj.y != cell.y || cj.z != cell.z) { continue; }
          // SAME MATERIAL ONLY. Two different materials in contact do not become one homogeneous
          // material — they stay distinct phases, and iron absorbed into basalt would be a physical
          // fiction. Refusing the pair removes the material-blending IOU this kernel used to carry rather
          // than deferring it, and it PRESERVES COMPOSITION for free: a differentiated blob converges to
          // one particle PER MATERIAL, each still at its own radius (the iron sank), so the layering
          // survives coalescence instead of being homogenised away.
          if (pj.mat != pi.mat) { continue; }
          // The absorber must be strictly "greater" — heavier, or equal mass with the lower index. That
          // total order is what stops i and j each claiming the other.
          let greater = (pj.mass > pi.mass) || (pj.mass == pi.mass && j < i);
          if (!greater) { continue; }
          let hij = 0.5 * (pi.h + pj.h);
          let r = length(pi.pos - pj.pos);
          if (r >= 0.5 * hij) { continue; }              // closer than one interparticle spacing
          if (length(pi.vel - pj.vel) >= c_i) { continue; } // subsonic relative motion
          // Prefer the LARGEST eligible absorber — Robin's "added into the composition of the largest blob".
          if (pj.mass > best_mass || best == NO_MERGE) {
            best = j;
            best_mass = pj.mass;
          }
        }
      }
    }
  }
  merge_target[i] = best;
}

// PASS: apply — a ROOT absorbs every neighbour that picked it. Writes only roots.
@compute @workgroup_size(64)
fn cs_merge_apply(@builtin(global_invocation_id) gid: vec3<u32>) {
  let j = gid.x;
  if (j >= P.n) { return; }
  if (!merge_enabled()) { return; }
  if (merge_target[j] != NO_MERGE) { return; } // not a root: it is itself being absorbed this pass
  let pj = particles[j];
  if (pj.mass <= 0.0) { return; }
  var m_new = pj.mass;
  var mom = pj.vel * pj.mass;
  var com = pj.pos * pj.mass;
  var mu_acc = pj.u * pj.mass;
  var ke_before = 0.5 * pj.mass * dot(pj.vel, pj.vel);
  var absorbed = 0u;
  let cj = cell_of(pj.pos);
  for (var dx: i32 = -1; dx <= 1; dx++) {
    for (var dy: i32 = -1; dy <= 1; dy++) {
      for (var dz: i32 = -1; dz <= 1; dz++) {
        let cell = cj + vec3<i32>(dx, dy, dz);
        let hh = hash_cell(cell);
        let cnt = min(atomicLoad(&grid_count[hh]), P.bucket_k);
        for (var s: u32 = 0u; s < cnt; s++) {
          let i = grid_bucket[hh * P.bucket_k + s];
          if (i == j) { continue; }
          if (merge_target[i] != j) { continue; }
          let pi = particles[i];
          let ci = cell_of(pi.pos);
          if (ci.x != cell.x || ci.y != cell.y || ci.z != cell.z) { continue; }
          m_new += pi.mass;
          mom += pi.vel * pi.mass;
          com += pi.pos * pi.mass;
          mu_acc += pi.u * pi.mass;
          ke_before += 0.5 * pi.mass * dot(pi.vel, pi.vel);
          absorbed += 1u;
        }
      }
    }
  }
  if (absorbed == 0u) { return; }
  let v_new = mom / m_new;
  let x_new = com / m_new;
  // The kinetic energy the merge destroys is exactly what the single particle can no longer represent;
  // it becomes heat, so kinetic + internal is unchanged.
  let ke_after = 0.5 * m_new * dot(v_new, v_new);
  let u_new = (mu_acc + max(ke_before - ke_after, 0.0)) / m_new;
  particles[j].pos = x_new;
  particles[j].vel = v_new;
  particles[j].mass = m_new;
  particles[j].u = u_new;
  // h tracks mass at fixed density: h ∝ (m/ρ)^⅓, so the blob's kernel grows as it eats.
  particles[j].h = pj.h * pow(m_new / pj.mass, 1.0 / 3.0);
}

// PASS: retire — an absorbed particle goes inert (mass 0 contributes nothing to gravity, density or
// pressure). Writes only non-roots. Slots are reclaimed by the host when it next uploads.
@compute @workgroup_size(64)
fn cs_merge_retire(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  if (!merge_enabled()) { return; }
  let t = merge_target[i];
  if (t == NO_MERGE) { return; }
  if (merge_target[t] != NO_MERGE) { return; } // its absorber was not a root — try again next frame
  particles[i].mass = 0.0;
  particles[i].vel = vec3<f32>(0.0);
}
