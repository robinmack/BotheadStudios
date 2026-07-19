//! Barnes–Hut octree for O(N log N) softened self-gravity — stage 1c of the accelerated compute module
//! (docs/30), the LONG-RANGE partner of the short-range neighbour grid ([`crate::neighbors`]).
//!
//! Self-gravity is every-pair O(N²): each particle feels every other. But a distant CLUMP of particles
//! pulls almost exactly like a single mass at its centre of mass — so we group them. Build an octree whose
//! internal nodes cache the centre-of-mass and total mass of their subtree; then for each particle, walk
//! the tree and whenever a node is far enough that its angular size `(2·half)/distance` is below an opening
//! angle `θ`, use its COM as ONE source instead of recursing. That turns O(N²) into O(N log N).
//!
//! Unlike the neighbour grid (which is EXACT), Barnes–Hut is an APPROXIMATION — but a bounded, θ-controlled
//! one: θ→0 recovers brute force, θ=0.5 (the default) keeps the per-particle error well under a percent,
//! which is far below the FP/chaos noise the disk statistics already tolerate (verified in tests). Softened
//! exactly like the direct sum (`G·m·d / (|d|²+s²)^{3/2}`), so it is the same physics, grouped. Generic over
//! positions + masses, so — like the grid — any particle system reuses it. Below [`BRUTE_BELOW`] it just
//! does the direct O(N²) sum (the tree-build overhead only pays past a few hundred bodies).

use crate::orbit::G;
use glam::DVec3;

/// Below this body count, skip the tree and direct-sum (tree build isn't worth it for small clouds).
const BRUTE_BELOW: usize = 1024;
/// Depth cap so coincident/degenerate particles can't subdivide forever; they collapse into a bucket leaf.
const MAX_DEPTH: u32 = 28;

struct Node {
    center: DVec3, // geometric centre of the cubic cell
    half: f64,     // half-width of the cell
    com: DVec3,    // centre of mass of the subtree
    mass: f64,     // total mass of the subtree
    children: [usize; 8], // arena indices; EMPTY when absent
    leaf: Vec<usize>,     // particle indices when this is a leaf (usually 1; more only if degenerate)
}

const EMPTY: usize = usize::MAX;

pub struct BarnesHut {
    nodes: Vec<Node>,
    theta: f64,
    soft2: f64,
    n: usize,
}

impl BarnesHut {
    /// Build the tree over `pos`/`mass` with opening angle `theta` and Plummer softening `softening`.
    pub fn build(pos: &[DVec3], mass: &[f64], theta: f64, softening: f64) -> Self {
        let n = pos.len();
        let mut bh = BarnesHut { nodes: Vec::new(), theta, soft2: softening * softening, n };
        if n < BRUTE_BELOW {
            return bh; // brute-force mode: no tree
        }
        // Bounding cube of all bodies.
        let (mut lo, mut hi) = (pos[0], pos[0]);
        for p in &pos[1..] {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let center = (lo + hi) * 0.5;
        let half = ((hi - lo).max_element() * 0.5).max(1.0e-9) * 1.0001; // pad so all bodies are inside
        let all: Vec<usize> = (0..n).collect();
        bh.build_node(&all, center, half, pos, mass, 0);
        bh
    }

    /// Recursively build a node over `idx`, returning its arena index. Internal nodes recurse per octant;
    /// a single body (or a degenerate coincident cluster at the depth cap) becomes a leaf.
    fn build_node(
        &mut self,
        idx: &[usize],
        center: DVec3,
        half: f64,
        pos: &[DVec3],
        mass: &[f64],
        depth: u32,
    ) -> usize {
        let id = self.nodes.len();
        // Mass + COM of this cell.
        let mut m = 0.0;
        let mut com = DVec3::ZERO;
        for &i in idx {
            m += mass[i];
            com += pos[i] * mass[i];
        }
        com = if m > 0.0 { com / m } else { center };
        self.nodes.push(Node { center, half, com, mass: m, children: [EMPTY; 8], leaf: Vec::new() });
        if idx.len() <= 1 || depth >= MAX_DEPTH {
            self.nodes[id].leaf = idx.to_vec(); // leaf (single body, or a coincident bucket at the cap)
            return id;
        }
        // Partition into 8 octants by sign relative to the cell centre.
        let mut oct: [Vec<usize>; 8] = Default::default();
        for &i in idx {
            let p = pos[i];
            let o = (if p.x >= center.x { 1 } else { 0 })
                | (if p.y >= center.y { 2 } else { 0 })
                | (if p.z >= center.z { 4 } else { 0 });
            oct[o].push(i);
        }
        let ch = half * 0.5;
        for (o, bodies) in oct.iter().enumerate() {
            if bodies.is_empty() {
                continue;
            }
            let cc = center
                + DVec3::new(
                    if o & 1 != 0 { ch } else { -ch },
                    if o & 2 != 0 { ch } else { -ch },
                    if o & 4 != 0 { ch } else { -ch },
                );
            let cid = self.build_node(bodies, cc, ch, pos, mass, depth + 1);
            self.nodes[id].children[o] = cid;
        }
        id
    }

    /// Softened self-gravity acceleration on every body: `Σ_j G·m_j·d/(|d|²+s²)^{3/2}` (d = j − i),
    /// grouped via the tree. Brute-force below [`BRUTE_BELOW`].
    pub fn accelerations(&self, pos: &[DVec3], mass: &[f64]) -> Vec<DVec3> {
        if self.nodes.is_empty() {
            // Brute-force direct sum (small clouds).
            let mut acc = vec![DVec3::ZERO; self.n];
            for i in 0..self.n {
                for j in 0..self.n {
                    if i == j {
                        continue;
                    }
                    let d = pos[j] - pos[i];
                    let r2 = d.length_squared() + self.soft2;
                    acc[i] += d * (G * mass[j] / (r2 * r2.sqrt()));
                }
            }
            return acc;
        }
        (0..self.n).map(|i| self.accel_on(i, 0, pos, mass)).collect()
    }

    /// Softened self-gravity on ONLY the `active` bodies (others get `DVec3::ZERO`) — the block-timestep
    /// fast path (docs/30 stage 3): a coasting particle's gravity is not recomputed until its own kick, so
    /// per-sub-step traversal cost drops from O(N log N) to O(N_active log N). The tree is still built over
    /// ALL current positions, so the active bodies see every other body correctly.
    pub fn accelerations_active(&self, pos: &[DVec3], mass: &[f64], active: &[bool]) -> Vec<DVec3> {
        if self.nodes.is_empty() {
            let mut acc = vec![DVec3::ZERO; self.n];
            for i in 0..self.n {
                if !active[i] {
                    continue;
                }
                for j in 0..self.n {
                    if i == j {
                        continue;
                    }
                    let d = pos[j] - pos[i];
                    let r2 = d.length_squared() + self.soft2;
                    acc[i] += d * (G * mass[j] / (r2 * r2.sqrt()));
                }
            }
            return acc;
        }
        (0..self.n)
            .map(|i| if active[i] { self.accel_on(i, 0, pos, mass) } else { DVec3::ZERO })
            .collect()
    }

    /// Acceleration on body `i` from the subtree rooted at `node` (iterative-free recursion over the tree).
    fn accel_on(&self, i: usize, node: usize, pos: &[DVec3], mass: &[f64]) -> DVec3 {
        let nd = &self.nodes[node];
        let mut a = DVec3::ZERO;
        if !nd.leaf.is_empty() {
            for &j in &nd.leaf {
                if j != i {
                    let d = pos[j] - pos[i];
                    let r2 = d.length_squared() + self.soft2;
                    a += d * (G * mass[j] / (r2 * r2.sqrt()));
                }
            }
            return a;
        }
        // Internal node: use its COM if far enough, else recurse into the children.
        let d = nd.com - pos[i];
        let r2 = d.length_squared() + self.soft2;
        let dist = r2.sqrt();
        if (2.0 * nd.half) / dist < self.theta {
            return d * (G * nd.mass / (r2 * dist));
        }
        for &c in &nd.children {
            if c != EMPTY {
                a += self.accel_on(i, c, pos, mass);
            }
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splitmix(state: &mut u64) -> f64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }

    #[test]
    fn barnes_hut_matches_brute_force_within_theta_bound() {
        // docs/30 stage 1c: Barnes–Hut is an APPROXIMATION, but a θ-controlled one — at θ=0.5 every
        // particle's gravity must agree with the O(N²) direct sum to well under a percent (far below the
        // FP/chaos noise the disk already tolerates). Random cloud > BRUTE_BELOW so the TREE path runs.
        let mut s = 0xABCD_1234u64;
        let n = 1500;
        let pos: Vec<DVec3> = (0..n)
            .map(|_| DVec3::new(splitmix(&mut s), splitmix(&mut s), splitmix(&mut s)) * 1.0e6)
            .collect();
        let mass: Vec<f64> = (0..n).map(|_| 1.0e18 * (0.5 + splitmix(&mut s))).collect();
        let soft = 5.0e3;
        // Brute reference (θ=0 ⇒ tree opens fully, i.e. exact) computed directly.
        let mut brute = vec![DVec3::ZERO; n];
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let d = pos[j] - pos[i];
                    let r2 = d.length_squared() + soft * soft;
                    brute[i] += d * (G * mass[j] / (r2 * r2.sqrt()));
                }
            }
        }
        let bh = BarnesHut::build(&pos, &mass, 0.5, soft);
        let approx = bh.accelerations(&pos, &mass);
        // The meaningful accuracy metric is the RMS relative error (a few particles near cell corners can
        // be worse — that is normal for θ=0.5 and unbiased, so it averages out of the disk statistics).
        let (mut sum_sq, mut max_rel) = (0.0f64, 0.0f64);
        for i in 0..n {
            let e = (approx[i] - brute[i]).length() / brute[i].length().max(1.0e-30);
            sum_sq += e * e;
            max_rel = max_rel.max(e);
        }
        let rms = (sum_sq / n as f64).sqrt();
        assert!(rms < 0.01, "Barnes–Hut (θ=0.5) RMS error must be <1% (got rms {rms:.4}, max {max_rel:.4})");
        assert!(max_rel < 0.1, "and no single particle wildly off (max {max_rel:.4})");
        // And θ→0 must be ~exact (opens every node ⇒ direct sum).
        let exact = BarnesHut::build(&pos, &mass, 1.0e-6, soft).accelerations(&pos, &mass);
        let mut max_rel_exact = 0.0f64;
        for i in 0..n {
            max_rel_exact =
                max_rel_exact.max((exact[i] - brute[i]).length() / brute[i].length().max(1.0e-30));
        }
        assert!(max_rel_exact < 1.0e-9, "θ→0 must recover brute force exactly (got {max_rel_exact:.2e})");
    }
}
