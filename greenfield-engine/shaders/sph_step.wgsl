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
  dt: f32,          // fixed integration timestep for cs_kick_drift/cs_kick (KDK leapfrog, stage 4c.1)
}

struct Particle {
  pos: vec3<f32>, h: f32,
  vel: vec3<f32>, u: f32,
  mass: f32, mat: u32, rho: f32, _pad: f32,
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
}

// Final half-kick (v, u). Reads acc/dudt from the SECOND force eval of the step.
@compute @workgroup_size(64)
fn cs_kick(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  particles[i].vel = particles[i].vel + acc[i] * (0.5 * P.dt);
  particles[i].u = max(particles[i].u + dudt[i] * (0.5 * P.dt), 0.0);
}
