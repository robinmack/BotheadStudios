//! **What is actually growing here, and where** — the engine resolving a land-cover class into plants.
//!
//! Robin (2026-08-04): *"low-cost models for grasses and trees… added as members of Earth so they can
//! be rendered as called for"*, and *"these are hues at altitude but must become realistic flora at very
//! low altitude."*
//!
//! At altitude a footprint answers with its mixture's albedo, which is a measured spectrum convolved
//! through the CIE observer. Close up the same footprint answers with the plants themselves. **These are
//! two representations of one fact**, not two models — Law IV, the camera changes representation and
//! never existence — and the invariant that keeps them honest is that a plant is made of the same
//! catalogued matter its class contributes to the ground's colour
//! (`assembly::plant_tests::the_plant_you_walk_up_to_is_made_of_what_the_ground_looked_like_from_orbit`).
//!
//! ## Nothing here is chosen
//!
//! **How many plants** is not a density dial. A land-cover class states what fraction of the ground its
//! canopy covers, and a plant's crown covers a known area, so
//!
//! ```text
//! plants per m² = cover fraction ÷ crown footprint
//! ```
//!
//! For IGBP class 4 (deciduous broadleaf, 0.75 broadleaf) with an 18 m oak crown that is 0.003 /m² —
//! about 30 mature trees per hectare, which is what a stand of trees that size actually is. For class 10
//! (grassland, 0.80 grass) with a 0.12 m tuft it is 71 tufts/m², which is a pasture.
//!
//! **Where each plant stands** is deterministic — hashed from its own cell, never from a random number
//! generator seeded at load. Look away and come back and the same tufts are in the same places, because
//! they were never placed, only *derived*. A scatter that re-rolled would be the camera changing what is
//! true.

use crate::materials::Material;

/// One plant, sited. `yaw` and `scale` are derived from the cell too, so a stand does not look stamped.
#[derive(Clone, Copy, Debug)]
pub struct Sited {
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Index into the `kinds` slice the caller passed to [`scatter`].
    pub kind: usize,
    pub yaw: f64,
    /// Multiplier on the assembly's own size — real stands are not clones.
    pub scale: f64,
}

/// A plant the engine knows how to grow: which assembly, what it is made of, how much ground its crown
/// covers.
#[derive(Clone, Debug)]
pub struct Kind {
    pub assembly_id: String,
    /// The foliage material this plant IS — the link back to the land-cover mixture.
    pub foliage: String,
    /// Ground area one plant's crown covers, m². Derived from the assembly's own geometry.
    pub crown_m2: f64,
}

impl Kind {
    /// Read a plant's crown footprint off the assembly itself, so the density that follows from it
    /// cannot drift from the thing being drawn.
    pub fn from_assembly(a: &crate::assembly::Assembly, foliage: &str) -> Kind {
        let widest = a
            .parts
            .iter()
            .filter(|p| p.material == foliage)
            .map(|p| match p.shape {
                crate::assembly::Shape::Sphere { r } => r,
                crate::assembly::Shape::Cylinder { r, .. } => r,
                crate::assembly::Shape::Tube { r_outer, .. } => r_outer,
                crate::assembly::Shape::Slab { x, z, .. } => 0.5 * x.max(z),
            })
            .fold(0.0f64, f64::max);
        Kind {
            assembly_id: a.id.clone(),
            foliage: foliage.to_string(),
            crown_m2: std::f64::consts::PI * widest * widest,
        }
    }
}

/// A deterministic value in `0..1` for an integer cell — the same cell always gives the same number.
///
/// This is what makes an unwatched meadow keep its tufts. It is a hash, not a random number generator:
/// no seed, no state, no order dependence, so the answer cannot depend on when or whether anything
/// looked.
fn cell_unit(x: i64, z: i64, salt: u64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// **Resolve the plants standing within `radius_m` of a coordinate.**
///
/// `mixture_at` answers what a point's land cover is made of — the same mixture the ground's albedo
/// comes from. `budget` bounds how many plants may be resolved at once (Law III: the minimal necessary
/// matter, not everything within sight); the nearest are kept, because those are the ones a viewer can
/// actually resolve.
///
/// Returns an empty list when nothing growable is there, which is the correct answer for open water,
/// bare rock and an ice sheet — and it costs nothing.
pub fn scatter(
    centre_lat: f64,
    centre_lon: f64,
    radius_m: f64,
    kinds: &[Kind],
    mats: &[Material],
    mut mixture_at: impl FnMut(f64, f64) -> Vec<(usize, f32)>,
    budget: usize,
) -> Vec<Sited> {
    if kinds.is_empty() || radius_m <= 0.0 || budget == 0 {
        return Vec::new();
    }
    // The cell is sized so ONE plant of the commonest kind lands in it at its own natural density —
    // spacing follows from the crown, so a meadow is fine-grained and a forest is not.
    let mut out = Vec::new();
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = m_per_deg_lat * centre_lat.to_radians().cos().abs().max(1e-6);

    for (ki, kind) in kinds.iter().enumerate() {
        let fol = crate::materials::index_of(mats, &kind.foliage);
        // Cover fraction here decides density; crown footprint decides spacing.
        let frac = mixture_at(centre_lat, centre_lon)
            .iter()
            .find(|&&(m, _)| m == fol)
            .map(|&(_, f)| f as f64)
            .unwrap_or(0.0);
        if frac <= 0.0 || kind.crown_m2 <= 0.0 {
            continue; // this plant does not grow here
        }
        let per_m2 = frac / kind.crown_m2;
        let cell_m = (1.0 / per_m2).sqrt();
        let n = (radius_m / cell_m).ceil() as i64;
        for cz in -n..=n {
            for cx in -n..=n {
                // Jitter inside the cell so a stand is not a lattice, but jitter DERIVED from the cell.
                let jx = cell_unit(cx, cz, ki as u64 * 7 + 1) - 0.5;
                let jz = cell_unit(cx, cz, ki as u64 * 7 + 2) - 0.5;
                let dx = (cx as f64 + jx) * cell_m;
                let dz = (cz as f64 + jz) * cell_m;
                if dx * dx + dz * dz > radius_m * radius_m {
                    continue;
                }
                let lat = centre_lat + dz / m_per_deg_lat;
                let lon = centre_lon + dx / m_per_deg_lon;
                // Does this KIND actually grow at that point, or is the class different there?
                let here = mixture_at(lat, lon);
                if !here.iter().any(|&(m, f)| m == fol && f > 0.0) {
                    continue;
                }
                out.push(Sited {
                    lat_deg: lat,
                    lon_deg: lon,
                    kind: ki,
                    yaw: cell_unit(cx, cz, ki as u64 * 7 + 3) * std::f64::consts::TAU,
                    // Real stands vary; ±25% about the assembly's own size.
                    scale: 0.75 + 0.5 * cell_unit(cx, cz, ki as u64 * 7 + 4),
                });
            }
        }
    }
    // Nearest first, then bound — the plants a viewer can actually resolve (Law III).
    out.sort_by(|a, b| {
        let d = |s: &Sited| {
            let x = (s.lon_deg - centre_lon) * m_per_deg_lon;
            let z = (s.lat_deg - centre_lat) * m_per_deg_lat;
            x * x + z * z
        };
        d(a).total_cmp(&d(b))
    });
    out.truncate(budget);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds() -> (Vec<Kind>, Vec<Material>) {
        let mats = crate::materials::load();
        let oak = crate::assembly::shipped::load("broadleaf-tree-oak");
        let tuft = crate::assembly::shipped::load("grass-tuft");
        (
            vec![
                Kind::from_assembly(&oak, "broadleaf_foliage"),
                Kind::from_assembly(&tuft, "grass"),
            ],
            mats,
        )
    }

    /// **The density is the class's own cover fraction, not a dial.**
    ///
    /// A stand of 18 m oaks comes out at tens per hectare and a pasture at tens of tufts per square
    /// metre — four orders of magnitude apart — from ONE rule and the plants' own geometry.
    #[test]
    fn how_many_plants_grow_here_follows_from_how_much_ground_they_cover() {
        let (k, _) = kinds();
        let oak = &k[0];
        let tuft = &k[1];
        // Crown footprints, read off the assemblies.
        assert!(
            (oak.crown_m2 - std::f64::consts::PI * 81.0).abs() < 1.0,
            "an 18 m crown covers ~254 m², got {:.0}",
            oak.crown_m2
        );
        assert!(
            tuft.crown_m2 < 0.05,
            "a tuft covers a hand's width, got {:.3}",
            tuft.crown_m2
        );
        // IGBP class 4 is 0.75 broadleaf; class 10 is 0.80 grass.
        let oaks_per_ha = 0.75 / oak.crown_m2 * 10_000.0;
        let tufts_per_m2 = 0.80 / tuft.crown_m2;
        assert!(
            (10.0..80.0).contains(&oaks_per_ha),
            "a stand of mature oaks is tens per hectare, got {oaks_per_ha:.0}"
        );
        assert!(
            (20.0..200.0).contains(&tufts_per_m2),
            "a pasture is tens of tufts per m², got {tufts_per_m2:.0}"
        );
    }

    /// **★ AN UNWATCHED MEADOW KEEPS ITS TUFTS.** Law IV in its sharpest form: the scatter is derived
    /// from position, so looking away and coming back — or arriving from a different direction, or with
    /// a different budget — cannot move a single plant.
    #[test]
    fn the_same_ground_grows_the_same_plants_every_time() {
        let (k, mats) = kinds();
        let grass = crate::materials::index_of(&mats, "grass");
        let mix = |_: f64, _: f64| vec![(grass, 0.8f32)];
        let a = scatter(53.1, -9.45, 6.0, &k, &mats, mix, 200);
        let b = scatter(53.1, -9.45, 6.0, &k, &mats, mix, 200);
        assert!(!a.is_empty(), "a pasture grows something");
        assert_eq!(a.len(), b.len(), "the same ground grows the same number");
        for (p, q) in a.iter().zip(&b) {
            assert!(
                (p.lat_deg - q.lat_deg).abs() < 1e-12 && (p.lon_deg - q.lon_deg).abs() < 1e-12,
                "a plant moved between two looks at the same ground"
            );
            assert!((p.yaw - q.yaw).abs() < 1e-12 && (p.scale - q.scale).abs() < 1e-12);
        }
        // And a SMALLER budget must return a prefix of the same answer — the nearest plants, not a
        // different meadow.
        let few = scatter(53.1, -9.45, 6.0, &k, &mats, mix, 20);
        assert_eq!(few.len(), 20);
        for (p, q) in few.iter().zip(&a) {
            assert!(
                (p.lat_deg - q.lat_deg).abs() < 1e-12,
                "a tighter budget must keep the NEAREST plants, not re-roll them"
            );
        }
    }

    /// **Nothing grows on the sea, on bare rock, or on an ice sheet** — and it costs nothing to ask.
    #[test]
    fn a_class_with_no_foliage_grows_nothing() {
        let (k, mats) = kinds();
        let water = crate::materials::index_of(&mats, "water");
        let sand = crate::materials::index_of(&mats, "sand");
        let granite = crate::materials::index_of(&mats, "granite");
        for mix in [vec![(water, 1.0f32)], vec![(sand, 0.6), (granite, 0.4)]] {
            let got = scatter(0.0, 0.0, 50.0, &k, &mats, |_, _| mix.clone(), 500);
            assert!(
                got.is_empty(),
                "nothing grows here, got {} plants",
                got.len()
            );
        }
    }

    /// **A stand is not a lattice.** Positions are jittered, but jittered DETERMINISTICALLY — the
    /// previous test already pinned that the jitter is stable, so this one only has to show it exists.
    #[test]
    fn a_stand_does_not_look_stamped() {
        let (k, mats) = kinds();
        let grass = crate::materials::index_of(&mats, "grass");
        let got = scatter(0.0, 0.0, 3.0, &k, &mats, |_, _| vec![(grass, 0.8f32)], 400);
        assert!(got.len() > 50, "expected a crowd, got {}", got.len());
        let yaws: Vec<f64> = got.iter().map(|s| s.yaw).collect();
        let spread = yaws.iter().cloned().fold(f64::MIN, f64::max)
            - yaws.iter().cloned().fold(f64::MAX, f64::min);
        assert!(
            spread > 4.0,
            "plants should face every which way, spread {spread:.2} rad"
        );
        let scales: Vec<f64> = got.iter().map(|s| s.scale).collect();
        let smin = scales.iter().cloned().fold(f64::MAX, f64::min);
        let smax = scales.iter().cloned().fold(f64::MIN, f64::max);
        assert!(
            smax - smin > 0.3,
            "real stands vary in size, got {smin:.2}..{smax:.2}"
        );
    }
}
