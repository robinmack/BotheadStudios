//! **What an assembly contains, when it contains more than it could ever list** (docs/67 §3 item 3).
//!
//! A planet contains something like 10¹² trees. `Vec<Instance>` is not an option, and neither is any
//! other container that holds one entry per thing. So containment here is two halves:
//!
//! > **a RULE that answers "what is in this region", plus a set of EXCEPTIONS the container remembers.**
//!
//! And the reason that is affordable rather than merely clever is Robin's scalability law (docs/67 §3
//! item 2): *"Oaks can be handled as identical to their construction until they are damaged, at which
//! point they become unique."* A pristine oak has nothing worth storing — the rule regenerates its
//! placement and the TYPE answers everything else — so **a container stores divergences, not
//! individuals**. Ten thousand undamaged trees cost zero bytes. Ten damaged ones cost ten.
//!
//! ## The crux: derived identity
//!
//! An exception has to be able to NAME the thing it is an exception to, and the rule cannot remember
//! what it generated — remembering is the thing being avoided. So **the rule derives each instance's id
//! from where it is** ([`InstanceId::derived`]), by the same hash that decided it was there at all.
//! Query the region again and the same tree comes back with the same id, so the exception finds it.
//!
//! That is what makes this work, and it is also the constraint on any rule: it must be a pure function
//! of position. `terra::flora::scatter` already is one, deliberately — *"no seed, no state, no order
//! dependence, so the answer cannot depend on when or whether anything looked"* — which is Law IV
//! written as code, and the reason an unwatched meadow keeps its tufts.
//!
//! ## What this is not
//!
//! It is not a spatial index. Nothing here accelerates a search over stored objects, because there are
//! no stored objects to search. The rule is the index.

use crate::instance::{Instance, InstanceId};
use glam::DVec3;
use std::collections::BTreeMap;

/// A ball in the container's own frame — what a caller is asking about.
///
/// Deliberately the crudest shape that works: a container's rule decides what "in here" means for its
/// own geometry (a planet's rule takes a ball and answers with a patch of its surface), and giving this
/// type opinions about discs, frusta or view volumes would put the caller's business inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub centre_m: DVec3,
    pub radius_m: f64,
    /// ★★ **What can be SEEN from the centre**, as a direction and the cosine of a half-angle — or
    /// `None` for the whole ball.
    ///
    /// Robin: *"let's try to scope to what is visible… if trees overlap the region behind so the region
    /// behind is invisible to the viewport/camera, then there's no need to spend compute on them."*
    /// Right, and it lands on the exact line **Law IV** draws: the camera changes REPRESENTATION, never
    /// EXISTENCE. A cone is therefore only ever legitimate for a query about what to DESCRIBE. The
    /// hidden tree still exists, still stands in the way of a bus, and still gets crushed — it simply is
    /// not detailed for a picture.
    ///
    /// So this field is the difference between the two kinds of question, made explicit in the type:
    /// **a renderer asks a cone, physics asks the ball.** A query that narrows what is COMPUTED by what
    /// is looked at would be Law IV inverted, and the way to catch that is for the narrowing to be
    /// visible at every call site rather than buried in a rule.
    pub facing: Option<(DVec3, f64)>,
}

impl Region {
    /// Everything within `radius_m` — the honest question, and the one physics must ask.
    pub fn ball(centre_m: DVec3, radius_m: f64) -> Region {
        Region {
            centre_m,
            radius_m,
            facing: None,
        }
    }

    /// Only what lies within `half_angle` of `dir` — a question about what to DESCRIBE. See `facing`.
    pub fn seen(centre_m: DVec3, radius_m: f64, dir: DVec3, half_angle: f64) -> Region {
        Region {
            centre_m,
            radius_m,
            facing: Some((dir.normalize_or_zero(), half_angle.cos())),
        }
    }

    pub fn contains(&self, p: DVec3) -> bool {
        let d = p - self.centre_m;
        if d.length_squared() > self.radius_m * self.radius_m {
            return false;
        }
        match self.facing {
            None => true,
            // Anything at the centre is in view whichever way it faces.
            Some((dir, cos_half)) => {
                let len = d.length();
                len < 1e-12 || d.dot(dir) / len >= cos_half
            }
        }
    }
}

impl InstanceId {
    /// **An identity derived from WHERE something is**, so a rule can regenerate it forever and an
    /// exception can name it without anyone keeping a list.
    ///
    /// A hash, not a counter: a counter would depend on the order things were generated in, which
    /// depends on which region was asked about, which depends on where the camera went — and then
    /// looking at a meadow from the other side would rename every tree in it.
    ///
    /// `salt` separates rules that share a container, so a planet's trees and its boulders cannot
    /// collide. The mixing is the same one `terra::flora::cell_unit` uses.
    pub fn derived(salt: u64, cell: (i64, i64), index: u64) -> InstanceId {
        // ★★ MIXED SEQUENTIALLY, NOT XOR-ED TOGETHER. The first version of this combined the inputs as
        // `(x·A) ^ (z·B) ^ (salt·C) ^ (index·D)` and then finalised — which is not a hash, it is four
        // numbers laid on top of each other, and it COLLIDED on the first realistic test: the trees at
        // cells (−3, 1) and (3, −1) came back with one identity. Two trees sharing an id means damaging
        // one damages the other, and a bus crushing a tree in front of you splinters one behind you.
        // Caught by `looking_narrows_what_is_described_and_changes_nothing_about_it`, which compared
        // two views of one wood and found the same id in two places.
        let mut h = 0xCBF2_9CE4_8422_2325u64; // FNV offset basis, as a starting state
        for x in [salt, cell.0 as u64, cell.1 as u64, index] {
            h ^= x;
            h = h.wrapping_mul(0x0100_0000_01B3); // FNV prime: every input reaches every later bit
            h ^= h >> 29;
        }
        // Final avalanche (splitmix64), so neighbouring cells are not neighbouring ids.
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        InstanceId(h ^ (h >> 31))
    }
}

/// **A rule that generates what stands in a region.** Pure in its region: the same region must always
/// produce the same instances with the same ids, or the exception set stops finding what it names.
pub trait Populates {
    /// Append everything this rule says is in `region`. Ids must come from [`InstanceId::derived`].
    fn generate(&mut self, region: &Region, out: &mut Vec<Instance>);
}

/// **What a container remembers** — and it remembers as little as it can.
///
/// The invariant this type exists to hold: **only divergences are stored.** Handing it a pristine
/// instance stores nothing, because a pristine instance is exactly what the rule already produces.
/// That is Robin's law made structural rather than remembered.
#[derive(Clone, Debug, Default)]
pub struct Contents {
    /// `Some` = this individual diverged and here is how. `None` = the rule says it is here and it is
    /// not — destroyed, or carried off by something.
    remembered: BTreeMap<InstanceId, Option<Instance>>,
}

impl Contents {
    pub fn new() -> Contents {
        Contents::default()
    }

    /// How many individuals this container is paying for. The number that must stay small while the
    /// population does not.
    pub fn remembered(&self) -> usize {
        self.remembered.len()
    }

    /// **Record what happened to one of them.** The first time anything happens to an instance it stops
    /// being derivable and starts costing bytes; this is the only door to that.
    ///
    /// ★ Handing back a PRISTINE instance forgets it instead of storing it — it has converged with what
    /// the rule would produce, so keeping it would be paying for a copy of the rule's own answer. That
    /// is not an optimisation bolted on; it is the invariant.
    pub fn diverge(&mut self, inst: Instance) {
        if inst.damage.is_pristine() && inst.thermal_j == 0.0 && inst.motion == Default::default() {
            self.remembered.remove(&inst.id);
        } else {
            self.remembered.insert(inst.id, Some(inst));
        }
    }

    /// It is gone: destroyed past the point of being anything, or removed to somewhere else. The
    /// container still has to remember the ABSENCE, because its rule will keep generating it.
    pub fn forget(&mut self, id: InstanceId) {
        self.remembered.insert(id, None);
    }

    /// Something that no rule would generate — a bus that drove here, a tree that fell from over there.
    /// Its id must not be one a rule derives; use a counter or another salt.
    pub fn place(&mut self, inst: Instance) {
        self.remembered.insert(inst.id, Some(inst));
    }

    /// **Everything actually in this region**: the rule's answer, with what the container remembers
    /// applied over it, plus anything placed here that no rule would produce.
    pub fn resolve(&self, rule: &mut impl Populates, region: &Region, out: &mut Vec<Instance>) {
        out.clear();
        rule.generate(region, out);
        // Apply divergences in place, and drop what is remembered as gone.
        let mut generated: Vec<InstanceId> = Vec::with_capacity(out.len());
        out.retain_mut(|inst| {
            generated.push(inst.id);
            match self.remembered.get(&inst.id) {
                None => true,        // pristine: the rule's answer stands
                Some(None) => false, // remembered as absent
                Some(Some(d)) => {
                    inst.clone_from(d);
                    true
                }
            }
        });
        generated.sort_unstable();
        // Then anything remembered that the rule did NOT produce but which stands here anyway.
        for (id, slot) in &self.remembered {
            let Some(inst) = slot else { continue };
            if generated.binary_search(id).is_err() && region.contains(inst.placement.at_m) {
                out.push(inst.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;
}

/// The fake rule both test modules drive. A stand-in for `terra::flora::scatter`: one tree per metre of
/// grid, generated from position and nothing else. The point is not the trees — it is that the rule
/// keeps no list.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use crate::instance::{Instance, Placement};

    pub struct Grid {
        salt: u64,
        calls: usize,
    }

    impl Populates for Grid {
        fn generate(&mut self, region: &Region, out: &mut Vec<Instance>) {
            self.calls += 1;
            let r = region.radius_m.ceil() as i64;
            let (cx, cz) = (
                region.centre_m.x.round() as i64,
                region.centre_m.z.round() as i64,
            );
            for x in (cx - r)..=(cx + r) {
                for z in (cz - r)..=(cz + r) {
                    let at = DVec3::new(x as f64, 0.0, z as f64);
                    if !region.contains(at) {
                        continue;
                    }
                    let id = InstanceId::derived(self.salt, (x, z), 0);
                    let mut inst =
                        Instance::of_type(id, "broadleaf-tree-oak", Placement::inside(None));
                    inst.placement.at_m = at;
                    out.push(inst);
                }
            }
        }
    }

    pub fn grid() -> Grid {
        Grid { salt: 7, calls: 0 }
    }
    pub fn region(r: f64) -> Region {
        Region::ball(DVec3::ZERO, r)
    }

    /// ★★★ **A CONTAINER FULL OF PRISTINE THINGS STORES NOTHING.** The whole scalability argument, as
    /// an assertion: the rule produced thousands of trees and the planet is paying for none of them.
    ///
    /// If this ever goes red, something is materialising instances in order to READ them, and 10¹²
    /// trees have become 10¹² allocations.
    #[test]
    fn an_undamaged_forest_costs_the_container_nothing() {
        let contents = Contents::new();
        let mut rule = grid();
        let mut out = Vec::new();
        contents.resolve(&mut rule, &region(30.0), &mut out);
        assert!(
            out.len() > 2500,
            "the rule produced a forest: {}",
            out.len()
        );
        assert_eq!(
            contents.remembered(),
            0,
            "and the container remembers none of it"
        );
    }

    /// **An unwatched meadow keeps its tufts.** The same region asked twice gives the same trees with
    /// the same ids — which is Law IV as code, and also the thing the exception set depends on: an
    /// exception that named a tree the rule renamed would attach to nothing.
    #[test]
    fn the_same_region_answers_the_same_way_however_often_it_is_asked() {
        let contents = Contents::new();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        contents.resolve(&mut grid(), &region(12.0), &mut a);
        contents.resolve(&mut grid(), &region(12.0), &mut b);
        assert_eq!(a, b, "identical trees, identical ids");
        // And looking from somewhere else does not rename what overlaps — the ids come from POSITION,
        // never from the order of generation.
        let mut shifted = Vec::new();
        contents.resolve(
            &mut grid(),
            &Region::ball(DVec3::new(4.0, 0.0, 0.0), 12.0),
            &mut shifted,
        );
        let here = a.iter().find(|i| i.placement.at_m == DVec3::ZERO).unwrap();
        let there = shifted
            .iter()
            .find(|i| i.placement.at_m == DVec3::ZERO)
            .unwrap();
        assert_eq!(
            here.id, there.id,
            "one tree, one identity, either way you look"
        );
    }

    /// ★★ **DAMAGE PERSISTS, AND IT IS LOCAL.** The bus crushes one tree; that tree stays crushed when
    /// the region is asked again, its neighbours are untouched, and the container is paying for exactly
    /// one individual.
    #[test]
    fn a_crushed_tree_stays_crushed_and_its_neighbours_do_not_notice() {
        let mut contents = Contents::new();
        let mut out = Vec::new();
        contents.resolve(&mut grid(), &region(8.0), &mut out);
        let before = out.clone();

        // Something happens to the tree at the origin.
        let mut hit = before
            .iter()
            .find(|i| i.placement.at_m == DVec3::ZERO)
            .unwrap()
            .clone();
        hit.damage.part_integrity = vec![1.0, 0.0]; // crown gone
        let hit_id = hit.id;
        contents.diverge(hit);
        assert_eq!(contents.remembered(), 1, "one individual, one entry");

        contents.resolve(&mut grid(), &region(8.0), &mut out);
        assert_eq!(out.len(), before.len(), "still the same forest");
        let now = out.iter().find(|i| i.id == hit_id).unwrap();
        assert!(!now.damage.is_pristine(), "it is still crushed");
        for (a, b) in before.iter().zip(out.iter()) {
            if a.id != hit_id {
                assert_eq!(a, b, "a neighbour changed, and nothing touched it");
            }
        }
    }

    /// ★★ **STORAGE SCALES WITH DAMAGE, NOT WITH POPULATION.** Ten times the forest, the same bytes.
    /// This is the claim the whole design rests on, so it is measured rather than argued.
    #[test]
    fn the_bill_is_for_what_happened_not_for_what_exists() {
        let mut small = Contents::new();
        let mut big = Contents::new();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        small.resolve(&mut grid(), &region(10.0), &mut a);
        big.resolve(&mut grid(), &region(40.0), &mut b);
        assert!(
            b.len() > 10 * a.len(),
            "one forest is far larger: {} vs {}",
            b.len(),
            a.len()
        );

        // Damage the same NUMBER of trees in each.
        for (c, src) in [(&mut small, &a), (&mut big, &b)] {
            for t in src.iter().take(10) {
                let mut d = t.clone();
                d.damage.part_integrity = vec![0.2];
                c.diverge(d);
            }
        }
        assert_eq!(small.remembered(), 10);
        assert_eq!(
            big.remembered(),
            10,
            "the larger forest costs no more to remember"
        );
    }

    /// **Convergence forgets.** Handing back an instance that is pristine again removes it, because it
    /// is once more exactly what the rule produces. The invariant is that only divergences are stored,
    /// and it is enforced at the door rather than swept up later.
    #[test]
    fn a_pristine_instance_handed_back_is_forgotten_not_stored() {
        let mut contents = Contents::new();
        let mut out = Vec::new();
        contents.resolve(&mut grid(), &region(4.0), &mut out);
        let mut t = out[0].clone();
        t.damage.part_integrity = vec![0.5];
        contents.diverge(t.clone());
        assert_eq!(contents.remembered(), 1);

        t.damage = Default::default();
        contents.diverge(t);
        assert_eq!(
            contents.remembered(),
            0,
            "back to what the rule says, so there is nothing left to remember"
        );
    }

    /// **Gone means gone, and placed means placed.** A container has to remember an ABSENCE, because
    /// its rule will keep generating what is no longer there — and it has to be able to hold something
    /// no rule would ever produce, because that is what a bus is.
    #[test]
    fn a_container_remembers_what_is_missing_and_what_arrived() {
        let mut contents = Contents::new();
        let mut out = Vec::new();
        contents.resolve(&mut grid(), &region(6.0), &mut out);
        let n = out.len();
        let felled = out[0].id;
        contents.forget(felled);

        contents.resolve(&mut grid(), &region(6.0), &mut out);
        assert_eq!(out.len(), n - 1, "the felled tree is not regenerated");
        assert!(out.iter().all(|i| i.id != felled));

        // A bus, which no rule generates.
        let mut bus = Instance::of_type(InstanceId(1), "naval-24pdr-gun", Placement::inside(None));
        bus.placement.at_m = DVec3::new(2.0, 0.0, 1.0);
        contents.place(bus);
        contents.resolve(&mut grid(), &region(6.0), &mut out);
        assert_eq!(out.len(), n, "the felled tree gone, the arrival present");
        assert!(out.iter().any(|i| i.id == InstanceId(1)));

        // ...and it is only there when you are looking somewhere that contains it.
        contents.resolve(
            &mut grid(),
            &Region::ball(DVec3::new(500.0, 0.0, 500.0), 6.0),
            &mut out,
        );
        assert!(out.iter().all(|i| i.id != InstanceId(1)));
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    /// ★★ **NO TWO CELLS MAY SHARE AN IDENTITY.** A collision means damaging one tree damages another
    /// somewhere else, and the first version of `derived` had one — cells (−3, 1) and (3, −1), found by
    /// a test that was looking for something else entirely.
    ///
    /// This walks a realistic patch and every salt a handful of overlapping rules would use, because
    /// species share ground deliberately (Robin: *"different flora types have different spacing, so
    /// lattices will overlap"*) and only the identities must stay apart.
    #[test]
    fn a_wood_of_cells_never_repeats_an_identity() {
        let mut ids = Vec::with_capacity(64 * 400 * 400);
        for salt in 0..4u64 {
            for x in -200..200i64 {
                for z in -200..200i64 {
                    ids.push(InstanceId::derived(salt, (x, z), 0));
                }
            }
        }
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            n,
            "{} collisions across {n} cells — a collision is two trees with one fate",
            n - ids.len()
        );
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::tests_support::*;
    use super::*;

    /// ★★ **A CONE ASKS FOR FEWER THINGS, NEVER FOR DIFFERENT ONES** — which is Law IV as an assertion.
    ///
    /// Robin: *"let's try to scope to what is visible."* The saving is real and so is the constraint:
    /// what a narrowed query returns must be a SUBSET of what the ball returns, identical in identity
    /// and position. If looking somewhere changed what was there, the camera would be deciding
    /// existence.
    #[test]
    fn looking_narrows_what_is_described_and_changes_nothing_about_it() {
        let contents = Contents::new();
        let (mut all, mut seen) = (Vec::new(), Vec::new());
        contents.resolve(&mut grid(), &Region::ball(DVec3::ZERO, 20.0), &mut all);
        // A 40-degree half-angle looking down +X — roughly a viewport.
        contents.resolve(
            &mut grid(),
            &Region::seen(DVec3::ZERO, 20.0, DVec3::X, 40f64.to_radians()),
            &mut seen,
        );

        assert!(!seen.is_empty(), "something is in view");
        assert!(
            seen.len() * 3 < all.len(),
            "a 40-degree cone is a small share of the ball: {} of {}",
            seen.len(),
            all.len()
        );
        // ★ Every visible thing is EXACTLY the thing the ball had — same id, same place.
        for v in &seen {
            let same = all
                .iter()
                .find(|a| a.id == v.id)
                .expect("in view but not in the world");
            assert_eq!(same, v, "looking at a tree must not change the tree");
        }
    }
}
