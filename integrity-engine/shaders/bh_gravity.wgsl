// GPU Barnes-Hut (LBVH) self-gravity — the O(N log N) replacement for the direct O(N²) gravity loop in
// sph_step.wgsl (`cs_forces`, docs/36). Built + verified standalone in tools/gpu-bh-verify before it is
// wired into the SPH step. The physics is IDENTICAL to bhtree.rs / the sph_step direct sum — softened
// Newtonian self-gravity a_i = Σ_j G·m_j·d/(|d|²+ε²)^{3/2} (d = pos_j − pos_i) — but distant subtrees are
// approximated by their centre of mass when their angular size (size/dist) drops below the opening angle θ.
//
// Pipeline (each kernel VERIFIED before the next, docs/36): bbox → morton → sort → Karras tree → bottom-up
// COM → θ-traversal. This file grows kernel-by-kernel; `cs_gravity_direct` is the reference the tree is
// checked against (both f32, so their difference is purely the θ multipole error, not precision).

const G: f32 = 6.674e-11;

struct Params {
  n: u32,          // particle count
  theta: f32,      // Barnes-Hut opening angle (0.5 default; →0 recovers direct sum)
  soft2: f32,      // Plummer softening squared (ε²)
  n_leaves: u32,   // number of tree LEAVES = ceil(n / bucket_k) (with bucket_k=1 this equals n)
  bucket_k: u32,   // particles per leaf bucket (1 = one particle per leaf, the classic LBVH)
  _p0: u32, _p1: u32, _p2: u32,
}

// A binary-radix-tree node (Karras). Node indexing over a 2·L−1 arena (L = n_leaves): internal nodes at
// [0, L−1) (root=0), leaves at [L−1, 2L−1). Leaf node (L−1)+c owns the sorted-particle bucket
// [c·K, min(c·K+K, n)) (K = bucket_k) via order[]. com/bmin/bmax are filled by the bottom-up COM pass;
// cs_tree fills left/right/parent only.
struct Node {
  left: u32, right: u32, parent: u32, flags: u32, // flags bit0 = COM ready (set once, by the child pass)
  com: vec4<f32>,   // subtree centre of mass (xyz) + total mass (w)
  bmin: vec4<f32>,  // subtree AABB min (xyz)
  bmax: vec4<f32>,  // subtree AABB max (xyz)
}
const NO_PARENT: u32 = 0xffffffffu;

@group(0) @binding(0) var<uniform> P: Params;
// A "body" is (pos.xyz, mass) packed into a vec4 — the only inputs gravity needs.
@group(0) @binding(1) var<storage, read> bodies: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> acc: array<vec4<f32>>; // output accel (xyz; w unused)
// bbox: [minx,miny,minz, maxx,maxy,maxz] as ORDER-PRESERVING u32 keys (WGSL atomics are integer-only, so
// floats are encoded to a monotonic u32 before atomicMin/Max — decode with bbox_decode).
@group(0) @binding(3) var<storage, read_write> bbox: array<atomic<u32>, 6>;
@group(0) @binding(4) var<storage, read_write> codes: array<u32>; // 30-bit Morton code per particle (SORTED for the tree)
@group(0) @binding(5) var<storage, read_write> order: array<u32>; // particle index (identity, then sorted)
@group(0) @binding(6) var<storage, read_write> nodes: array<Node>; // 2N−1 tree nodes
@group(0) @binding(7) var<storage, read_write> ready: array<atomic<u32>>; // per-internal-node arrival counter (COM climb)
@group(0) @binding(8) var<storage, read> sbodies: array<vec4<f32>>; // bodies permuted into sorted (Morton) order — leaf buckets are contiguous ⇒ coalesced reads

// Map an IEEE-754 f32 to a u32 whose unsigned order matches the float's signed order (so integer atomicMin/
// Max reduce to the true float min/max): flip the sign bit for positives, invert all bits for negatives.
fn bbox_key(f: f32) -> u32 {
  let k = bitcast<u32>(f);
  if ((k >> 31u) == 1u) { return ~k; }      // negative → invert all bits
  return k ^ 0x80000000u;                    // positive/zero → flip sign bit
}
fn bbox_decode(k: u32) -> f32 {
  if ((k >> 31u) == 1u) { return bitcast<f32>(k ^ 0x80000000u); } // top bit set → was positive
  return bitcast<f32>(~k);                                        // top bit clear → was negative
}

// --- Stage 1: adaptive bounding box (min/max of all positions) ---
@compute @workgroup_size(6)
fn cs_bbox_reset(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= 6u) { return; }
  // min slots start at +inf-key (0xffffffff), max slots at -inf-key (0) so the first atomic wins.
  if (i < 3u) { atomicStore(&bbox[i], 0xffffffffu); } else { atomicStore(&bbox[i], 0u); }
}
@compute @workgroup_size(64)
fn cs_bbox(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let p = bodies[i].xyz;
  atomicMin(&bbox[0], bbox_key(p.x));
  atomicMin(&bbox[1], bbox_key(p.y));
  atomicMin(&bbox[2], bbox_key(p.z));
  atomicMax(&bbox[3], bbox_key(p.x));
  atomicMax(&bbox[4], bbox_key(p.y));
  atomicMax(&bbox[5], bbox_key(p.z));
}

// --- Stage 2: Morton codes (spatial sort key) ---
// Spread a 10-bit integer to 30 bits by inserting two 0s between each bit (the standard bit trick).
fn expand_bits(v0: u32) -> u32 {
  var v = v0 & 0x000003ffu;
  v = (v * 0x00010001u) & 0xff0000ffu;
  v = (v * 0x00000101u) & 0x0f00f00fu;
  v = (v * 0x00000011u) & 0xc30c30c3u;
  v = (v * 0x00000005u) & 0x49249249u;
  return v;
}
// 30-bit Morton code from a position, using the adaptive bbox to map to the unit cube.
fn morton_code(p: vec3<f32>, lo: vec3<f32>, ext: vec3<f32>) -> u32 {
  let u = clamp((p - lo) / ext, vec3<f32>(0.0), vec3<f32>(1.0));
  let q = clamp(floor(u * 1024.0), vec3<f32>(0.0), vec3<f32>(1023.0));
  let xx = expand_bits(u32(q.x));
  let yy = expand_bits(u32(q.y));
  let zz = expand_bits(u32(q.z));
  return xx * 4u + yy * 2u + zz;
}
@compute @workgroup_size(64)
fn cs_morton(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let lo = vec3<f32>(bbox_decode(atomicLoad(&bbox[0])), bbox_decode(atomicLoad(&bbox[1])), bbox_decode(atomicLoad(&bbox[2])));
  let hi = vec3<f32>(bbox_decode(atomicLoad(&bbox[3])), bbox_decode(atomicLoad(&bbox[4])), bbox_decode(atomicLoad(&bbox[5])));
  let ext = max(hi - lo, vec3<f32>(1.0e-30)); // guard a flat axis (all coords equal) against divide-by-zero
  codes[i] = morton_code(bodies[i].xyz, lo, ext);
  order[i] = i;
}

// --- Stage 4: Karras binary radix tree over the SORTED Morton codes ---
// δ(i,j): length of the longest common prefix of codes[i] and codes[j] (−1 if j is out of range). Equal
// codes are disambiguated by extending the key with the leaf index (Karras 2012), so duplicates still build
// a valid tree.
fn delta(i: i32, j: i32) -> i32 {
  if (j < 0 || j >= i32(P.n_leaves)) { return -1; }
  let ci = codes[u32(i)];
  let cj = codes[u32(j)];
  if (ci == cj) { return 32 + i32(countLeadingZeros(u32(i) ^ u32(j))); }
  return i32(countLeadingZeros(ci ^ cj));
}
// Reset the whole node arena (2N−1 nodes): clear pointers, parent = sentinel, flags = 0.
@compute @workgroup_size(64)
fn cs_tree_reset(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= 2u * P.n_leaves - 1u) { return; }
  nodes[i].left = 0u;
  nodes[i].right = 0u;
  nodes[i].parent = NO_PARENT;
  nodes[i].flags = 0u;
  if (i < P.n_leaves - 1u) { atomicStore(&ready[i], 0u); } // clear the COM-climb arrival counters
}
// Build the N−1 internal nodes: each covers a contiguous run of sorted leaves; findSplit places its two
// children. Leaf children get node index (N−1)+pos, internal children get index = pos.
@compute @workgroup_size(64)
fn cs_tree(@builtin(global_invocation_id) gid: vec3<u32>) {
  let ii = gid.x;
  if (ii >= P.n_leaves - 1u) { return; } // L−1 internal nodes
  let i = i32(ii);

  // determineRange: find the [first,last] leaf span this internal node owns.
  let d = select(-1, 1, delta(i, i + 1) > delta(i, i - 1));
  let delta_min = delta(i, i - d);
  var l_max = 2;
  while (delta(i, i + l_max * d) > delta_min) { l_max = l_max * 2; }
  var l = 0;
  var t = l_max / 2;
  while (t >= 1) {
    if (delta(i, i + (l + t) * d) > delta_min) { l = l + t; }
    t = t / 2;
  }
  let j = i + l * d;
  let first = min(i, j);
  let last = max(i, j);

  // findSplit: the position where the top differing Morton bit flips (binary search on common prefix).
  var split = first;
  let first_code = codes[u32(first)];
  let last_code = codes[u32(last)];
  if (first_code == last_code) {
    split = (first + last) >> 1u;
  } else {
    let cpfx = i32(countLeadingZeros(first_code ^ last_code));
    var step = last - first;
    loop {
      step = (step + 1) >> 1u;
      let new_split = split + step;
      if (new_split < last) {
        let sc = codes[u32(new_split)];
        if (i32(countLeadingZeros(first_code ^ sc)) > cpfx) { split = new_split; }
      }
      if (step <= 1) { break; }
    }
  }

  // Children: a child that spans a single leaf is a LEAF node (index (N−1)+pos), else an INTERNAL node (pos).
  let left_leaf = (split == first);
  let right_leaf = (split + 1 == last);
  let left_idx = select(u32(split), (P.n_leaves - 1u) + u32(split), left_leaf);
  let right_idx = select(u32(split + 1), (P.n_leaves - 1u) + u32(split + 1), right_leaf);
  nodes[ii].left = left_idx;
  nodes[ii].right = right_idx;
  nodes[left_idx].parent = ii;
  nodes[right_idx].parent = ii;
}

// --- Stage 5: bottom-up centre of mass + subtree AABB ---
// One thread per leaf. It sets its own leaf node (COM = the particle, AABB = the point) then climbs toward
// the root: at each ancestor it atomically registers arrival; the FIRST child to arrive stops (its sibling's
// subtree isn't done), the SECOND merges both children (COM mass-weighted, AABB unioned) and keeps climbing.
// So every internal node is computed exactly once, by whichever child finishes last. No float atomics needed.
@compute @workgroup_size(64)
fn cs_com(@builtin(global_invocation_id) gid: vec3<u32>) {
  let c = gid.x;                  // cluster / leaf index
  if (c >= P.n_leaves) { return; }
  let li = (P.n_leaves - 1u) + c; // this leaf's node index
  // Accumulate the bucket: sorted particles [c·K, min(c·K+K, n)).
  let start = c * P.bucket_k;
  let end = min(start + P.bucket_k, P.n);
  var m: f32 = 0.0;
  var mx = vec3<f32>(0.0);
  var lo = vec3<f32>(1.0e30);
  var hi = vec3<f32>(-1.0e30);
  for (var s: u32 = start; s < end; s++) {
    let b = sbodies[s]; // contiguous (sorted-order) read
    m += b.w;
    mx += b.xyz * b.w;
    lo = min(lo, b.xyz);
    hi = max(hi, b.xyz);
  }
  let com = select(vec3<f32>(0.0), mx / m, m > 0.0);
  nodes[li].com = vec4<f32>(com, m);
  nodes[li].bmin = vec4<f32>(lo, 0.0);
  nodes[li].bmax = vec4<f32>(hi, 0.0);

  var node = nodes[li].parent;
  loop {
    if (node == NO_PARENT) { break; }
    let arrived = atomicAdd(&ready[node], 1u);
    if (arrived == 0u) { break; }  // first child here — the sibling subtree will finish this node
    // Second arrival: both children are complete. Merge them.
    let l = nodes[node].left;
    let r = nodes[node].right;
    let cl = nodes[l].com;
    let cr = nodes[r].com;
    let m = cl.w + cr.w;
    let com = select(vec3<f32>(0.0), (cl.xyz * cl.w + cr.xyz * cr.w) / m, m > 0.0);
    nodes[node].com = vec4<f32>(com, m);
    nodes[node].bmin = vec4<f32>(min(nodes[l].bmin.xyz, nodes[r].bmin.xyz), 0.0);
    nodes[node].bmax = vec4<f32>(max(nodes[l].bmax.xyz, nodes[r].bmax.xyz), 0.0);
    node = nodes[node].parent;
  }
}

// --- direct O(N²) gravity: the reference the Barnes-Hut tree is verified against ---
// Extracted verbatim (same formula) from sph_step.wgsl:161-167.
@compute @workgroup_size(64)
fn cs_gravity_direct(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= P.n) { return; }
  let pi = bodies[i].xyz;
  let s2 = P.soft2;
  var a = vec3<f32>(0.0);
  for (var j: u32 = 0u; j < P.n; j++) {
    if (j == i) { continue; }
    let bj = bodies[j];
    let d = bj.xyz - pi;
    let r2 = dot(d, d);
    a += d * (G * bj.w / pow(r2 + s2, 1.5));
  }
  acc[i] = vec4<f32>(a, 0.0);
}

// --- Stage 6: Barnes-Hut θ-traversal gravity (the O(N log N) payoff) ---
// Per particle, an explicit stack walk from the root: use a node's monopole (its COM) when it is far enough
// that its angular size (box side / distance) is below θ, else descend into its two children. Same softened
// formula and the SAME opening rule as bhtree.rs (`(2·half)/dist < θ`, with dist including softening); the
// LBVH node's "2·half" is the largest side of its subtree AABB. θ→0 opens every node down to the leaves and
// recovers the exact direct sum — the strong structural check.
const MAX_STACK: u32 = 64u; // correctness-safe bound: 30 Morton bits + up to 32 index-tiebreak bits (duplicate chains)
@compute @workgroup_size(64)
fn cs_gravity_bh(@builtin(global_invocation_id) gid: vec3<u32>) {
  let t = gid.x;
  if (t >= P.n) { return; }
  // Work entirely in MORTON (sorted) space: thread t owns sorted position t. Adjacent threads are spatial
  // neighbours (coherent tree paths + hot upper nodes) AND their particle reads are contiguous in sbodies —
  // both the self read and the leaf-bucket sums are coalesced. Only the final write scatters to acc[order[t]].
  let pi = sbodies[t].xyz;
  let s2 = P.soft2;
  let leaf0 = P.n_leaves - 1u; // node indices ≥ this are leaves
  var a = vec3<f32>(0.0);

  var stack: array<u32, 64>;
  var sp: u32 = 0u;
  stack[sp] = 0u; sp = sp + 1u; // push root
  loop {
    if (sp == 0u) { break; }
    sp = sp - 1u;
    let ni = stack[sp];
    if (ni >= leaf0) {
      // leaf → exact direct sum over its bucket of ≤K particles (coalesced, cheap)
      let c = ni - leaf0;
      let start = c * P.bucket_k;
      let end = min(start + P.bucket_k, P.n);
      for (var s: u32 = start; s < end; s++) {
        if (s != t) {
          let bj = sbodies[s]; // contiguous (sorted-order) read
          let d = bj.xyz - pi;
          let r2 = dot(d, d) + s2;
          a += d * (G * bj.w / (r2 * sqrt(r2)));
        }
      }
      continue;
    }
    let nd = nodes[ni];
    let d = nd.com.xyz - pi;
    let r2 = dot(d, d) + s2;
    let dist = sqrt(r2);
    let size = length(nd.bmax.xyz - nd.bmin.xyz); // AABB diagonal = rigorous bound on the node's angular extent
    // Robust (Salmon-Warren / Barnes 1994) MAC: with a TIGHT AABB the COM can sit far from the box centre,
    // so a plain size/dist<θ under-opens and a few particles get large errors. Adding the centre↔COM offset
    // δ to the required distance (accept only when dist ≥ size/θ + δ) guarantees the particle is far from ALL
    // of the node's mass, not just its centroid — keeps the tight box AND caps the worst-case error.
    let centre = 0.5 * (nd.bmin.xyz + nd.bmax.xyz);
    let delta = length(centre - nd.com.xyz);
    if (dist >= size / P.theta + delta) {
      // far enough: use the node's centre of mass as a single source
      a += d * (G * nd.com.w / (r2 * dist));
      continue;
    }
    // too close: descend (guard against stack overflow — height is bounded but be safe)
    if (sp + 2u <= MAX_STACK) {
      stack[sp] = nd.left; sp = sp + 1u;
      stack[sp] = nd.right; sp = sp + 1u;
    }
  }
  acc[order[t]] = vec4<f32>(a, 0.0); // scatter back to the particle's original index
}
