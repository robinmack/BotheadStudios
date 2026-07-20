//! **The hierarchical spatial hash** (`docs/47` §1) — neighbour finding that does not assume one grain
//! size, so a tyre contact patch and a crater can share a scene.
//!
//! The flat hash this replaces uses a single `cell_size = 2·CONTACT_RADIUS`, with the invariant that a
//! cell is at least a contact diameter so every contact lies within ±1 cell. Admitting mixed sizes by
//! growing that cell to the LARGEST grain is the obvious move and it is wrong: a 1 m cell packed with
//! 1 cm grains holds ~10⁶ of them, so a small grain's ±1-cell scan degenerates to O(N). That does not
//! slow the acceleration structure, it defeats it. It is the same mistake as a global particle size,
//! one level up — **there is no global cell size either.**
//!
//! Instead: one grid per size class, `cell_size(level) = base·2^level`, each item inserted at the level
//! whose cell is at least its own contact diameter. The cost follows the number of NON-EMPTY levels,
//! with no cap — see docs/47 on why that number must be measured rather than assumed, and why what
//! really bounds it is the representation ladder (a 1 cm grain never contact-tests a 10⁶ m body; at that
//! scale separation the large thing is a field or an orbital body, not particles).
//!
//! **Each pair is enumerated exactly once, and none is missed.** Both properties are structural:
//! - *Once* — an item scans its OWN level and every COARSER one, never finer. For `Lᵢ < Lⱼ` only the
//!   finer item finds the pair; for `Lᵢ = Lⱼ` the tie is broken on index.
//! - *None missed* — at level `Lⱼ ≥ Lᵢ` the cell is at least `2rⱼ ≥ rᵢ + rⱼ`, and a ±1-cell scan reaches
//!   at least one full cell, so any `j` in contact range is in the scanned block.
//!
//! `pairs_within` is the reference implementation, pinned to brute force by test. The WGSL mirror is to
//! be written against it, not alongside it.

use glam::{DVec3, IVec3};

/// Grid level for an item of contact radius `r`: the smallest `L` with `base·2^L ≥ 2r`.
///
/// Level 0 holds everything at or below the base size, so `base` should be the finest granularity the
/// scene actually resolves. Items larger than the base climb one level per doubling.
pub fn level_for(radius: f64, base: f64) -> u32 {
    let d = 2.0 * radius;
    if !(d > 0.0) || !(base > 0.0) || d <= base {
        return 0;
    }
    (d / base).log2().ceil().max(0.0) as u32
}

/// Cell edge (m) at `level`.
pub fn cell_size_at(base: f64, level: u32) -> f64 {
    base * (1u64 << level.min(62)) as f64
}

/// Cell coordinate of `pos` at `level`.
pub fn cell_of(pos: DVec3, base: f64, level: u32) -> IVec3 {
    let s = cell_size_at(base, level);
    IVec3::new(
        (pos.x / s).floor() as i32,
        (pos.y / s).floor() as i32,
        (pos.z / s).floor() as i32,
    )
}

/// Hash a (level, cell) key into a table of `mask + 1` buckets. The level is folded INTO the key so one
/// table serves every level — the GPU keeps a single buffer and its existing clear/insert passes.
pub fn hash_cell(level: u32, c: IVec3, mask: u32) -> u32 {
    let mut h = (c.x as u32).wrapping_mul(0x8da6_b343);
    h ^= (c.y as u32).wrapping_mul(0xd8163841);
    h ^= (c.z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= level.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h.wrapping_mul(0x2545_f491) & mask
}

/// Every pair of `items` (position, contact radius) whose surfaces are within `slack` of touching —
/// each pair exactly once, found through the hierarchy rather than by testing all pairs.
///
/// `base` is the finest cell edge. `slack` extends the test beyond `rᵢ + rⱼ` for interaction ranges that
/// reach past contact (cohesion). Returns `(i, j)` with no ordering guarantee between pairs.
pub fn pairs_within(items: &[(DVec3, f64)], base: f64, slack: f64) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    if items.is_empty() || !(base > 0.0) {
        return Vec::new();
    }
    let levels: Vec<u32> = items.iter().map(|&(_, r)| level_for(r, base)).collect();
    let max_level = levels.iter().copied().max().unwrap_or(0);
    // One bucket map per level. An absent level costs nothing to skip, which is the O(1) empty-level
    // skip docs/47 requires — the hierarchy must never pay for levels a scene does not use.
    let mut buckets: HashMap<(u32, i32, i32, i32), Vec<usize>> = HashMap::new();
    for (i, &(p, _)) in items.iter().enumerate() {
        let c = cell_of(p, base, levels[i]);
        buckets.entry((levels[i], c.x, c.y, c.z)).or_default().push(i);
    }
    let mut out = Vec::new();
    for (i, &(pi, ri)) in items.iter().enumerate() {
        // Own level and every COARSER one — never finer, which is what makes each pair unique.
        for l in levels[i]..=max_level {
            let c = cell_of(pi, base, l);
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let key = (l, c.x + dx, c.y + dy, c.z + dz);
                        let Some(list) = buckets.get(&key) else {
                            continue;
                        };
                        for &j in list {
                            // Same level ⇒ both scan each other; break the tie on index. Different
                            // level ⇒ only the finer item ever looks, so no tie to break.
                            if l == levels[i] && j <= i {
                                continue;
                            }
                            let (pj, rj) = items[j];
                            if (pi - pj).length() <= ri + rj + slack {
                                out.push((i, j));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(items: &[(DVec3, f64)], slack: f64) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for i in 0..items.len() {
            for j in (i + 1)..items.len() {
                let (pi, ri) = items[i];
                let (pj, rj) = items[j];
                if (pi - pj).length() <= ri + rj + slack {
                    v.push((i, j));
                }
            }
        }
        v
    }

    fn norm(mut v: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
        for p in v.iter_mut() {
            if p.0 > p.1 {
                *p = (p.1, p.0);
            }
        }
        v.sort_unstable();
        v
    }

    /// A grain sits at the level whose cell can hold its contact diameter — that is the invariant the
    /// whole structure rests on, because a ±1-cell scan only reaches a neighbour if the cell is at
    /// least a contact diameter across.
    #[test]
    fn a_grain_lands_in_a_cell_that_can_hold_it() {
        let base = 0.02; // 2 cm — a tyre-patch grain
        for &r in &[0.005, 0.01, 0.05, 0.25, 0.5, 7.0] {
            let l = level_for(r, base);
            assert!(
                cell_size_at(base, l) >= 2.0 * r - 1.0e-12,
                "r={r} landed at level {l}, cell {} < diameter {}",
                cell_size_at(base, l),
                2.0 * r
            );
            if l > 0 {
                assert!(cell_size_at(base, l - 1) < 2.0 * r, "r={r} could have fitted a finer level");
            }
        }
        // The kart's two scales differ by ~7 levels and coexist without either forcing the other's cell.
        assert!(level_for(0.5, 0.0075) > level_for(0.0075, 0.0075));
    }

    /// **THE test (CLAUDE.md rule 3): the hierarchy is pinned to brute force.** Across a 100× size
    /// range — the ratio that defeats a single-cell grid — it must find EXACTLY the same pairs, each
    /// exactly once. Speed must never change the answer.
    #[test]
    fn the_hierarchy_finds_exactly_the_brute_force_pairs() {
        // Deterministic pseudo-random cloud spanning 5 mm to 0.5 m radii.
        let mut items = Vec::new();
        let mut s: u64 = 0x1234_5678;
        let mut next = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 33) as f64) / (u32::MAX as f64)
        };
        for k in 0..400 {
            let p = DVec3::new(next() * 4.0, next() * 2.0, next() * 4.0);
            // A few big grains among many small ones — the mixed-granularity case, not a uniform cloud.
            let r = if k % 17 == 0 { 0.2 + next() * 0.3 } else { 0.005 + next() * 0.02 };
            items.push((p, r));
        }
        let got = norm(pairs_within(&items, 0.01, 0.0));
        let want = norm(brute(&items, 0.0));
        assert!(!want.is_empty(), "the fixture must actually produce contacts");
        assert_eq!(got.len(), want.len(), "pair COUNT differs — a pair was missed or double-counted");
        assert_eq!(got, want, "the hierarchy and brute force disagree on which pairs are in contact");
        // No duplicates, stated separately so a double-count cannot hide behind a compensating miss.
        let mut d = got.clone();
        d.dedup();
        assert_eq!(d.len(), got.len(), "a pair was enumerated more than once");
    }

    /// The extreme ratio, isolated: one boulder among many pebbles. This is the configuration where
    /// "just grow the cell" collapses, so it must be exactly right here.
    #[test]
    fn a_boulder_among_pebbles_is_found_from_the_pebble_side() {
        let mut items = vec![(DVec3::new(0.0, 0.0, 0.0), 0.5)]; // the boulder, index 0
        for k in 0..64 {
            let a = k as f64 / 64.0 * std::f64::consts::TAU;
            // A ring of pebbles just touching the boulder's surface, and a second ring clearly clear of it.
            items.push((DVec3::new(a.cos(), 0.0, a.sin()) * 0.505, 0.01));
            items.push((DVec3::new(a.cos(), 0.0, a.sin()) * 0.9, 0.01));
        }
        let got = norm(pairs_within(&items, 0.02, 0.0));
        let want = norm(brute(&items, 0.0));
        assert_eq!(got, want, "mixed 50x size ratio disagrees with brute force");
        let touching = got.iter().filter(|&&(i, j)| i == 0 || j == 0).count();
        assert_eq!(touching, 64, "every pebble on the inner ring must contact the boulder, once each");
    }

    /// Cohesion and other laws reach beyond touch, so the query must honour a slack — and still agree
    /// with brute force at the widened range.
    #[test]
    fn the_interaction_range_can_reach_past_touch() {
        let items = vec![
            (DVec3::new(0.0, 0.0, 0.0), 0.05),
            (DVec3::new(0.13, 0.0, 0.0), 0.05), // 3 cm clear of touch
        ];
        assert!(pairs_within(&items, 0.02, 0.0).is_empty(), "clear of touch ⇒ no pair");
        assert_eq!(pairs_within(&items, 0.02, 0.05).len(), 1, "within slack ⇒ found");
        assert_eq!(norm(pairs_within(&items, 0.02, 0.05)), norm(brute(&items, 0.05)));
    }
}
