//! Reusable spatial neighbour index — stage 1 of the accelerated particle-compute module (docs/30).
//!
//! Every short-range force loop in the engine (granular contact, SPH vapor pressure/density, and — as the
//! module grows — clouds, smoke, fluids) is currently O(N²): each particle tests every other. Almost all
//! of those tests return nothing, because these forces are SHORT-RANGE. A uniform-cell spatial HASH fixes
//! that: bucket particles by integer cell `⌊pos / cell_size⌋`, and a particle's neighbours within
//! `cell_size` are then only in its own cell plus the 26 adjacent ones → **O(N) pair-finding**.
//!
//! It is deliberately GENERIC over `&[DVec3]` (not tied to `Aggregate`) so any particle system reuses it,
//! and it is EXACT: [`NeighborGrid::for_each_pair`] yields precisely the pairs a brute-force O(N²) sweep
//! would (every pair within `cell`, each once, plus some just-outside candidates the caller filters by the
//! true radius). That exactness is what lets the accelerated force loops conserve energy/momentum
//! identically to the brute-force ones — the invariant the whole perf effort rests on (verified in tests).

use glam::DVec3;
use std::collections::HashMap;

/// A uniform spatial hash over world positions. Build once per force evaluation (positions change every
/// step); cell size = the interaction radius of the force being accelerated.
pub struct NeighborGrid {
    cell: f64,
    cells: HashMap<(i32, i32, i32), Vec<usize>>,
}

#[inline]
fn key(p: DVec3, cell: f64) -> (i32, i32, i32) {
    ((p.x / cell).floor() as i32, (p.y / cell).floor() as i32, (p.z / cell).floor() as i32)
}

impl NeighborGrid {
    /// Bucket the `pos` into cells of side `cell` (clamped > 0). O(N).
    pub fn build(pos: &[DVec3], cell: f64) -> Self {
        let cell = cell.max(1.0e-9);
        let mut cells: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (i, p) in pos.iter().enumerate() {
            cells.entry(key(*p, cell)).or_default().push(i);
        }
        Self { cell, cells }
    }

    /// Invoke `f(i, j)` once for every unique pair `i < j` whose centres lie within one cell of each other
    /// (plus a few just-outside corner candidates — filter by the true radius in `f`). Each pair is emitted
    /// EXACTLY once: iterating from particle `i`, a neighbour `j` is only reported when `j > i`, so the
    /// mirror encounter from `j`'s neighbourhood is skipped. O(N · ⟨neighbours⟩).
    pub fn for_each_pair(&self, pos: &[DVec3], mut f: impl FnMut(usize, usize)) {
        for (i, p) in pos.iter().enumerate() {
            let (cx, cy, cz) = key(*p, self.cell);
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if let Some(bucket) = self.cells.get(&(cx + dx, cy + dy, cz + dz)) {
                            for &j in bucket {
                                if j > i {
                                    f(i, j);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A cheap deterministic PRNG (the engine forbids Math.random-style nondeterminism; this is a test-only
    // splitmix64 so positions are reproducible without external crates).
    fn splitmix(state: &mut u64) -> f64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64 // in [0,1)
    }

    #[test]
    fn grid_finds_exactly_the_brute_force_pairs() {
        // The load-bearing invariant: the grid must yield PRECISELY the pairs within `cell` that an O(N²)
        // sweep finds — no misses (which would silently drop forces and break conservation), no spurious
        // extras beyond what a radius filter removes. Random cloud, several cell sizes.
        let mut s = 0x1234_5678u64;
        let n = 400;
        let pos: Vec<DVec3> = (0..n)
            .map(|_| {
                DVec3::new(
                    splitmix(&mut s) * 100.0,
                    splitmix(&mut s) * 100.0,
                    splitmix(&mut s) * 100.0,
                )
            })
            .collect();
        for &cell in &[3.0, 7.5, 20.0, 50.0] {
            // Brute force: every pair genuinely within `cell`.
            let mut brute = std::collections::HashSet::new();
            for i in 0..n {
                for j in (i + 1)..n {
                    if (pos[i] - pos[j]).length() < cell {
                        brute.insert((i, j));
                    }
                }
            }
            // Grid: collect candidates, then apply the SAME radius filter the real force loops apply.
            let grid = NeighborGrid::build(&pos, cell);
            let mut found = std::collections::HashSet::new();
            let mut seen_pairs = 0usize;
            grid.for_each_pair(&pos, |i, j| {
                seen_pairs += 1;
                assert!(i < j, "pairs must be ordered and unique");
                if (pos[i] - pos[j]).length() < cell {
                    assert!(found.insert((i, j)), "grid emitted a duplicate pair ({i},{j})");
                }
            });
            assert_eq!(found, brute, "grid pair set (cell={cell}) must equal brute force");
            // And it must be doing far less work than O(N²) for a small cell (else it is pointless).
            if cell <= 7.5 {
                assert!(seen_pairs < n * n / 4, "grid should cull most of the N² candidates");
            }
        }
    }
}
