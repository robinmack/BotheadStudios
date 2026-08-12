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
    /// ★ **Its identity, derived from the cell it grows in** — the same hash that decided it was there.
    /// This is what lets a container hold an EXCEPTION for one plant (`containment::Contents`): an
    /// exception has to be able to name what it is an exception to, and the rule keeps no list.
    /// Stable across queries from any centre, which is the property row 47 was about.
    pub id: crate::instance::InstanceId,
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Index into the `kinds` slice the caller passed to [`scatter`].
    pub kind: usize,
    pub yaw: f64,
    /// Multiplier on the assembly's own size — real stands are not clones.
    pub scale: f64,
    /// Horizontal distance from the eye's ground point (m), and this plant's own height — the two
    /// numbers the budget spends. Kept here because they are free at generation and awkward after.
    pub dist_m: f64,
    pub height_m: f64,
}

/// **How big this plant looks from an eye `eye_m` above the ground** — its own extent over the slant
/// distance to it. The angle, which is what decides whether resolving it could change the picture.
fn subtended(s: &Sited, eye_m: f64) -> f64 {
    let slant = (s.dist_m * s.dist_m + eye_m * eye_m).sqrt().max(1e-6);
    s.height_m * s.scale / slant
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
    /// ★ **How tall it stands**, m — the assembly's own extent (`Assembly::reach_m`), which is what
    /// decides how big it looks from anywhere. A grass tuft is 0.35 m and an oak is 15; the budget
    /// below spends that difference instead of ignoring it.
    pub height_m: f64,
}

impl Kind {
    /// Read a plant's crown footprint off the assembly itself, so the density that follows from it
    /// cannot drift from the thing being drawn.
    /// ★ The crown is the assembly's own footprint (`Assembly::crown_m2`) — the outermost boundary of
    /// its outermost foliage part, taken horizontally, with each part standing where and pointing how
    /// it says. What stood here read each part's widest DIMENSION and ignored both its orientation and
    /// its offset, which is fine for a crown that is one sphere on an axis and wrong the moment an
    /// assembly resolves into components: a grass clump's 0.35 m blade would have claimed a 0.35 m
    /// crown and thinned the pasture thirty-fold through `plants per m² = cover ÷ crown`.
    pub fn from_assembly(a: &crate::assembly::Assembly, foliage: &str) -> Kind {
        Kind {
            assembly_id: a.id.clone(),
            foliage: foliage.to_string(),
            crown_m2: a.crown_m2(foliage),
            height_m: a.reach_m(),
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
    // How high the eye is above the ground here (m) — what turns a distance into an ANGLE.
    eye_m: f64,
    budget: usize,
) -> Vec<Sited> {
    if kinds.is_empty() || radius_m <= 0.0 || budget == 0 {
        return Vec::new();
    }
    // The cell is sized so ONE plant of the commonest kind lands in it at its own natural density —
    // spacing follows from the crown, so a meadow is fine-grained and a forest is not.
    let mut out = Vec::new();
    let m_per_deg_lat = 111_320.0;

    for (ki, kind) in kinds.iter().enumerate() {
        let fol = crate::materials::index_of(mats, &kind.foliage);
        if kind.crown_m2 <= 0.0 {
            continue;
        }
        // The cover here, used ONLY to size the scan (below). Occupancy is still decided per cell at
        // each cell's own position, so density follows the ground rather than this one sample.
        let frac_here = mixture_at(centre_lat, centre_lon)
            .iter()
            .find(|&&(m, _)| m == fol)
            .map(|&(_, f)| f as f64)
            .unwrap_or(0.0);
        if frac_here <= 0.0 {
            continue; // this plant does not grow here at all
        }
        // ★★ **THE LATTICE IS A FACT ABOUT THE GROUND, NOT ABOUT THE OBSERVER** (docs/46 row 47).
        //
        // Spacing comes from the KIND's own crown and nothing else: at full cover one plant occupies
        // one crown, so plants sit sqrt(crown) apart. What it must NOT come from is the local cover
        // fraction, which is what it used to — cells were sized from `frac` sampled at the QUERY
        // CENTRE and indexed `-n..=n` about it, so the whole lattice was pinned to the camera and
        // every plant in the stand moved when you walked. Robin asked for the opposite in as many
        // words: *"so trees don't move to different positions when one turns one's head away and turns
        // back."*
        //
        // Cover still decides how MANY there are — by deciding whether each cell is occupied (below),
        // which is what a cover fraction means. That change also makes density follow the cover at
        // each PLANT's own position instead of the one sampled at the centre, so a stand thins out
        // across a boundary rather than being uniform to its edge.
        //
        // ★ **THE COST THIS CHANGES, MEASURED.** Spacing no longer widens where cover is thin, so a
        // sparse meadow now walks the same number of cells as a dense one — for grass (crown 0.011 m²,
        // so 0.106 m apart) that is ~π(r/0.106)² cells, about 220,000 over a 25 m disc. The old
        // lattice at 60% cover was ~1.7× cheaper and WRONG, and the whole test module went from 0.05 s
        // to 3.5 s when these tests started asking about realistic radii. Two honest ways down if it
        // matters: skip whole cells with a coarse cover test before sampling the mixture, or let
        // spacing come from a QUANTISED cover (stable within a band, so plants only shift when the
        // band does). Neither is worth doing before a rig measures a rebuild in a real scene.
        let cell_m = kind.crown_m2.sqrt();
        let d_lat = cell_m / m_per_deg_lat;
        // ★★★ **EACH KIND REACHES AS FAR AS IT IS WORTH REACHING** — and one radius for every kind is
        // why there were no trees at eye level.
        //
        // The caller used to hand in a single `(alt * 8).clamp(6, 120)`, which at 1.7 m eye height is
        // **13.6 m**, while oaks sit `sqrt(crown)` = 16 m apart. A query disc smaller than a lattice's
        // spacing cannot contain a single one of its cells, so **no oak could ever appear from standing
        // height, at any budget**. That number was a renderer's fidelity-versus-cost decision (docs/68)
        // wearing the model's clothes, and it was not derived from anything.
        //
        // Derived instead, from two things already known here: a kind's own density (`frac/crown`) and
        // the budget. The radius at which THIS kind alone would fill the budget is
        // `sqrt(budget / (π · density))` — beyond that its plants cannot win a slot however big they
        // look, so scanning further is provably wasted. Dense things get a small disc and sparse things
        // a large one, which is both correct and *cheaper*: grass reaches ~3.5 m (3.4k cells) and oaks
        // ~460 m (2.6k cells), against 285k cells for one 32 m disc that still would not have held the
        // oaks. The angular sort then decides which of them actually get drawn.
        let density = (frac_here / kind.crown_m2).max(1e-12);
        let reach_m = (budget as f64 / (std::f64::consts::PI * density))
            .sqrt()
            .min(radius_m);
        let lat_span = reach_m / m_per_deg_lat;
        // ★ Each KIND gets its own lattice, and they are meant to overlap — grass grows under an oak,
        // and their spacings differ by two orders of magnitude. What must never overlap is IDENTITY,
        // so the kind goes into the salt of every hash below (Robin: *"different flora types have
        // different spacing, so lattices will overlap"*).
        let salt = |n: u64| ki as u64 * 16 + n;
        let cz0 = ((centre_lat - lat_span) / d_lat).floor() as i64;
        let cz1 = ((centre_lat + lat_span) / d_lat).ceil() as i64;
        for cz in cz0..=cz1 {
            // The row's own latitude — from the ROW INDEX, never from the camera, or the east-west
            // spacing would drift with the observer exactly the way the whole lattice used to.
            let lat_row = cz as f64 * d_lat;
            let m_per_deg_lon = m_per_deg_lat * lat_row.to_radians().cos().abs().max(1e-6);
            let d_lon = cell_m / m_per_deg_lon;
            let lon_span = reach_m / m_per_deg_lon;
            let cx0 = ((centre_lon - lon_span) / d_lon).floor() as i64;
            let cx1 = ((centre_lon + lon_span) / d_lon).ceil() as i64;
            for cx in cx0..=cx1 {
                // Jitter inside the cell so a stand is not a lattice — derived from the ABSOLUTE cell,
                // so a plant's offset is as fixed as the cell it sits in.
                let jx = cell_unit(cx, cz, salt(1)) - 0.5;
                let jz = cell_unit(cx, cz, salt(2)) - 0.5;
                let lat = (cz as f64 + 0.5 + jz) * d_lat;
                let lon = (cx as f64 + 0.5 + jx) * d_lon;
                let dz = (lat - centre_lat) * m_per_deg_lat;
                let dx = (lon - centre_lon) * m_per_deg_lon;
                if dx * dx + dz * dz > reach_m * reach_m {
                    continue;
                }
                // **Cover decides OCCUPANCY.** A cover fraction is the share of ground this plant
                // holds, so it is exactly the probability that a cell of its own size has one in it —
                // and the draw is the cell's own deterministic number, so the answer is a property of
                // that patch of ground forever.
                let frac = mixture_at(lat, lon)
                    .iter()
                    .find(|&&(m, _)| m == fol)
                    .map(|&(_, f)| f as f64)
                    .unwrap_or(0.0);
                if frac <= 0.0 || cell_unit(cx, cz, salt(5)) >= frac {
                    continue;
                }
                out.push(Sited {
                    id: crate::instance::InstanceId::derived(salt(0), (cx, cz), 0),
                    // Kept from where the scales are already in hand, so the budget below does not have
                    // to re-derive a metre-per-degree at every comparison.
                    dist_m: (dx * dx + dz * dz).sqrt(),
                    height_m: kind.height_m,
                    lat_deg: lat,
                    lon_deg: lon,
                    kind: ki,
                    yaw: cell_unit(cx, cz, salt(3)) * std::f64::consts::TAU,
                    // Real stands vary; ±25% about the assembly's own size.
                    scale: 0.75 + 0.5 * cell_unit(cx, cz, salt(4)),
                });
            }
        }
    }
    // ★★★ **THE BUDGET IS SPENT ON WHAT IS BIG IN THE VIEW, NOT ON WHAT IS UNDERFOOT.**
    //
    // This sorted by DISTANCE and truncated, which is not a rule — it is a way to bound a count, and it
    // silently decided that the 1,201st grass tuft at 3.5 m mattered more than every oak in the county.
    // Grass at a 0.35 cover fraction is ~31 plants per m², so a 1,200 budget is exhausted inside
    // `sqrt(1200/(31π))` ≈ 3.5 m: **a disc of columns centred under the camera**, which is exactly what
    // Robin reported seeing — *"the columns were clumped in one tiny area in center of viewport,
    // suspiciously crater like."* A radius-bounded budget drawn on the ground IS a crater.
    //
    // What decides whether something is worth resolving is how big it LOOKS: its own extent over its
    // distance, the angle it subtends. That is the same criterion docs/44 already uses to decide
    // whether to resolve anything at all, and the same one that turns a plant back into its albedo
    // above 300 m — so nothing new is being invented, and no per-kind quota has to be written. Grass
    // wins underfoot because it is underfoot; a 15 m oak wins at 100 m because it is 15 m tall.
    //
    // The slant includes the EYE HEIGHT, or a tuft directly below the camera would subtend infinity.
    out.sort_by(|a, b| subtended(b, eye_m).total_cmp(&subtended(a, eye_m)));
    out.truncate(budget);
    out
}

/// **What the flora mesh was built FROM** — the renderer's cache key (docs/68, docs/46 row 52).
///
/// The mesh is a CACHE of an answer the model can give at any instant: *which plants stand within
/// reach of this point.* A cache goes stale when its INPUTS change — and this one was keyed on the
/// camera's latitude and longitude alone. So it survived two things that change the answer: the
/// descent, which changes which plants are worth resolving, and the elevation tiles, which arrive
/// seconds later and move the ground.
///
/// ★★ MEASURED at Galway, 2026-08-09. The anchor froze the pre-tile ground (426 m); the tiles
/// revised it to 390 m; the plants stayed where the frozen number put them, **36.1 m in the air**,
/// for the whole descent — `|anchor − eye|` came back as `|36.1 − alt|` at every altitude the rig
/// visited. From 1.5 m with the eye pitched down, 36 m overhead is BEHIND the camera: all eight
/// corners of the mesh's bounds projected with negative `w`. Meanwhile `flora_count` reported
/// 1,200 and the draw reported 43,200 triangles the entire way down, which is why nine
/// deploy-and-photograph cycles bisected toward the GPU and found nothing wrong there.
///
/// The lesson is the one docs/68 is built on: a renderer that BAKES a model answer owns a copy
/// that the model cannot correct. Ask again, or be told when to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Built {
    pub lat: f64,
    pub lon: f64,
    /// Ground elevation under the anchor (m above sea level) — the number the streamed tiles revise.
    pub ground_m: f64,
    /// The eye height the angular budget was spent at (m). The budget buys SUBTENDED ANGLE, so
    /// altitude decides WHICH plants win a slot, not merely how many.
    pub alt_m: f64,
    /// How many plants this mesh holds.
    pub n: usize,
}

impl Built {
    /// Is a mesh built for `self` still the answer at `fresh`? Tolerances rather than equality:
    /// rebuilding on every float wobble would walk a quarter of a million lattice cells per frame.
    pub fn still_current(&self, fresh: &Built) -> bool {
        let moved_m =
            ((self.lat - fresh.lat).powi(2) + (self.lon - fresh.lon).powi(2)).sqrt() * 111_320.0;
        // GROUND: a quarter of a metre is well under the shortest plant the engine draws (a
        // 0.35 m tuft), so any revision large enough to hide one forces a rebuild.
        let ground_moved = (self.ground_m - fresh.ground_m).abs();
        // ALTITUDE: the budget spends on angle, which goes as 1/slant, so equal RATIOS are equal
        // changes — a rule that behaves the same at 4 km and at 1.5 m, which no absolute
        // threshold does.
        let alt_ratio = fresh.alt_m.max(1e-3) / self.alt_m.max(1e-3);
        moved_m < 2.0 && ground_moved < 0.25 && (0.8..1.25).contains(&alt_ratio)
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn at(lat: f64, lon: f64, ground_m: f64, alt_m: f64) -> Built {
        Built {
            lat,
            lon,
            ground_m,
            alt_m,
            n: 1200,
        }
    }

    /// ★★ **THE BUG THIS EXISTS FOR** (2026-08-09). The elevation tiles stream in seconds after the
    /// scene loads and REVISE the ground under the camera — at Galway, by 36 m. The flora mesh was
    /// keyed on latitude and longitude alone, so nothing about that revision invalidated it: every
    /// plant stayed at the height the pre-tile guess put it, hanging in the air, for the rest of the
    /// session. From standing height with the eye pitched down they were *behind the camera*, and the
    /// scene reported 1,200 plants and 43,200 triangles the whole time.
    ///
    /// Ground is checked at a quarter of a metre because the shortest plant the engine draws is a
    /// 0.35 m tuft: any revision big enough to bury one has to force a rebuild.
    #[test]
    fn a_revised_ground_makes_the_mesh_stale() {
        let before = at(53.10, -9.45, 426.6, 4000.0);
        let after_tiles = at(53.10, -9.45, 390.5, 4000.0);
        assert!(
            !before.still_current(&after_tiles),
            "a 36 m correction to the ground must invalidate the mesh standing on it"
        );
        // A revision far below the shortest plant is not worth a quarter-million-cell rebuild.
        assert!(before.still_current(&at(53.10, -9.45, 426.7, 4000.0)));
    }

    /// The other half of the same bug: the budget buys SUBTENDED ANGLE, so the eye's height decides
    /// which plants win a slot — not merely how many. Keyed on position alone, a descent from 4 km to
    /// standing height reused the selection made at 4 km, which is the wrong 1,200 plants.
    #[test]
    fn descending_makes_the_mesh_stale() {
        let high = at(53.10, -9.45, 390.0, 4000.0);
        assert!(!high.still_current(&at(53.10, -9.45, 390.0, 1.5)));
        // Ratios, not absolutes: the SAME rule has to behave at 4 km and at 1.5 m.
        assert!(high.still_current(&at(53.10, -9.45, 390.0, 4200.0)));
        let low = at(53.10, -9.45, 390.0, 1.5);
        assert!(low.still_current(&at(53.10, -9.45, 390.0, 1.6)));
        assert!(!low.still_current(&at(53.10, -9.45, 390.0, 3.0)));
    }

    /// Walking still invalidates it, which is the rule that was already there and still holds.
    #[test]
    fn walking_two_metres_makes_the_mesh_stale() {
        let here = at(53.10, -9.45, 390.0, 1.5);
        let step = 2.5 / 111_320.0;
        assert!(!here.still_current(&at(53.10 + step, -9.45, 390.0, 1.5)));
        assert!(here.still_current(&at(53.10 + step * 0.5, -9.45, 390.0, 1.5)));
    }
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
        let a = scatter(53.1, -9.45, 6.0, &k, &mats, mix, 1.7, 200);
        let b = scatter(53.1, -9.45, 6.0, &k, &mats, mix, 1.7, 200);
        assert!(!a.is_empty(), "a pasture grows something");
        assert_eq!(a.len(), b.len(), "the same ground grows the same number");
        for (p, q) in a.iter().zip(&b) {
            assert!(
                (p.lat_deg - q.lat_deg).abs() < 1e-12 && (p.lon_deg - q.lon_deg).abs() < 1e-12,
                "a plant moved between two looks at the same ground"
            );
            assert!((p.yaw - q.yaw).abs() < 1e-12 && (p.scale - q.scale).abs() < 1e-12);
        }
        // ★ AMENDED 2026-08-05. This demanded that a smaller budget return a PREFIX BY DISTANCE, which
        // was true when the budget was spent nearest-first and is not now: it is spent on what SUBTENDS
        // the most, and `scale` varies ±25%, so a slightly larger tuft can outrank a slightly nearer
        // one. That is the rule working, not failing. What the test was really guarding is that a
        // tighter budget SELECTS from the same meadow rather than re-rolling it — which is the property
        // determinism actually promises, and it still holds.
        let few = scatter(53.1, -9.45, 6.0, &k, &mats, mix, 1.7, 20);
        // Nearly the budget, not exactly it — see the note in
        // `the_budget_buys_what_can_be_seen_not_what_is_underfoot`. A sub-millimetre change in a
        // clump's crown moves the scan's last cell, and demanding an exact count asserts a precision
        // grass does not have. The property under test is BELOW: the same meadow, selected from.
        assert!(
            (18..=20).contains(&few.len()),
            "a tight budget should be nearly spent, got {}",
            few.len()
        );
        for p in &few {
            let same = a
                .iter()
                .find(|q| q.id == p.id)
                .expect("a tighter budget must SELECT from the same meadow, not grow a new one");
            assert!(
                (p.lat_deg - same.lat_deg).abs() < 1e-12
                    && (p.lon_deg - same.lon_deg).abs() < 1e-12
                    && (p.scale - same.scale).abs() < 1e-12,
                "and the plant it kept is the same plant"
            );
        }
    }

    /// ★★★ **THE BUDGET IS NOT A CRATER.** Robin, on an earlier render (2026-08-05): *"when I scrolled
    /// in far enough trees should be visible, the columns were clumped in one tiny area in center of
    /// viewport, suspiciously crater like."*
    ///
    /// That is what a NEAREST-FIRST budget draws. Grass at a 0.35 cover fraction is ~31 plants per m²,
    /// so 1,200 of them are exhausted inside `sqrt(1200/(31π))` ≈ 3.5 m — a disc of columns centred
    /// under the camera, with every oak in the county starved out. The rule was never a design; it was
    /// a way to bound a count.
    ///
    /// Spending the budget on what SUBTENDS THE MOST fixes it without a per-kind quota: the oaks are
    /// present because they are 15 m tall, and the stand reaches far beyond the grass.
    #[test]
    fn the_budget_buys_what_can_be_seen_not_what_is_underfoot() {
        let (kinds, mats) = kinds();
        let leaf = crate::materials::index_of(&mats, "broadleaf_foliage");
        let grass = crate::materials::index_of(&mats, "grass");
        // Maine's mixed forest: 45% broadleaf, 35% grass — the mixture that produced the crater.
        let mix = move |_: f64, _: f64| vec![(leaf, 0.45f32), (grass, 0.35f32)];
        let got = scatter(45.3, -69.0, 60.0, &kinds, &mats, mix, 1.7, 1200);
        // ★ NOT an exact 1200. Robin, 2026-08-12, when a 0.07 mm change in the clump's crown moved
        // this to 1198: *"remember there is variance in nature; I think 0.07 mm on a blade of grass may
        // not be enough to get uptight about."* Right — and `scatter` itself varies every clump's
        // `scale`, because real stands are not clones. A budget that must land on its exact ceiling is
        // asserting a precision the thing being measured does not have. What matters is that the budget
        // is SPENT, not that it is spent to the last plant.
        assert!(
            got.len() > 1150 && got.len() <= 1200,
            "the budget should be spent, got {} of 1200",
            got.len()
        );

        let trees = got.iter().filter(|s| s.kind == 0).count();
        assert!(
            trees > 20,
            "the oaks are in the picture, not starved out by grass: {trees} of 1200"
        );
        // ★ AND THE STAND IS NOT A DISC. Under the old rule everything sat within ~3.5 m; the tallest
        // things now reach out to where they still subtend something.
        let far = got.iter().map(|s| s.dist_m).fold(0.0f64, f64::max);
        assert!(
            far > 25.0,
            "the stand reaches out to {far:.1} m, not a crater at the camera's feet"
        );
        // The grass is still there, and still underfoot — it wins where it genuinely dominates the view.
        let near_grass = got.iter().filter(|s| s.kind == 1 && s.dist_m < 3.0).count();
        assert!(
            near_grass > 100,
            "grass underfoot is still resolved: {near_grass}"
        );
    }

    /// ★★★ **THE TREES STAY PUT WHEN YOU WALK** (docs/46 row 47).
    ///
    /// Robin, 2026-08-04: *"so trees don't move to different positions when one turns one's head away
    /// and turns back."* The old lattice could not do this and read as though it could: cells ran
    /// `-n..=n` about the QUERY CENTRE and their size came from the cover fraction sampled there, so
    /// the whole stand regenerated somewhere else the moment the camera moved far enough to trigger a
    /// rebuild. The hash was stateless; its inputs were not absolute.
    ///
    /// This asks about the same patch of ground from three different places and requires the plants in
    /// the overlap to be the SAME plants — same position, same identity, same size, same facing. It is
    /// also the precondition for `containment::Contents`: an exception naming a damaged tree attaches
    /// to nothing if the rule renames it when you walk away.
    #[test]
    fn the_same_ground_grows_the_same_plants_from_wherever_you_ask() {
        let (kinds, mats) = kinds();
        let leaf = crate::materials::index_of(&mats, "broadleaf_foliage");
        let grass = crate::materials::index_of(&mats, "grass");
        let all = move |_: f64, _: f64| vec![(leaf, 1.0f32), (grass, 0.6f32)];
        let here = |lat: f64, lon: f64| scatter(lat, lon, 15.0, &kinds, &mats, all, 1.7, 100_000);

        let a = here(10.0, 20.0);
        // Three centres, each offset by more than Terra's 2 m rebuild threshold.
        // Offsets of 4-9 m: well past Terra's 2 m rebuild threshold, well inside the disc.
        for (dlat, dlon) in [(0.00004, 0.0), (0.0, 0.00008), (-0.00005, 0.00004)] {
            let b = here(10.0 + dlat, 20.0 + dlon);
            let mut matched = 0;
            for p in &a {
                let Some(q) = b.iter().find(|q| q.id == p.id) else {
                    continue; // outside the other query's disc — not a disagreement
                };
                matched += 1;
                assert_eq!(p.lat_deg, q.lat_deg, "a plant moved north/south");
                assert_eq!(p.lon_deg, q.lon_deg, "a plant moved east/west");
                assert_eq!(p.yaw, q.yaw, "a plant turned around");
                assert_eq!(p.scale, q.scale, "a plant changed size");
                assert_eq!(p.kind, q.kind, "a plant changed species");
            }
            assert!(
                matched > a.len() / 3,
                "the two views overlap substantially: {matched} of {}",
                a.len()
            );
        }
    }

    /// ★ **Two species share the ground and never share an identity.** Robin: *"Different flora types
    /// have different spacing, so lattices will overlap."* They must — grass grows under an oak, and
    /// their spacings differ by two orders of magnitude — so the lattices are deliberately independent.
    /// What must never overlap is the ID, or damaging a tuft would damage a tree.
    #[test]
    fn overlapping_lattices_never_share_an_identity() {
        let (kinds, mats) = kinds();
        let leaf = crate::materials::index_of(&mats, "broadleaf_foliage");
        let grass = crate::materials::index_of(&mats, "grass");
        let all = move |_: f64, _: f64| vec![(leaf, 1.0f32), (grass, 1.0f32)];
        // 25 m: wide enough that the OAK lattice (16 m spacing, from its 9 m crown) has cells in it at
        // all, which an 8 m disc does not — the two lattices differ by two orders of magnitude.
        let out = scatter(10.0, 20.0, 25.0, &kinds, &mats, all, 1.7, 100_000);
        let by_kind: Vec<usize> = (0..kinds.len())
            .map(|k| out.iter().filter(|s| s.kind == k).count())
            .collect();
        assert!(
            by_kind.iter().all(|&n| n > 0),
            "both species grow on this ground: {by_kind:?}"
        );
        let mut ids: Vec<_> = out.iter().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            n,
            "every plant has its own identity across both lattices"
        );
    }

    /// **Nothing grows on the sea, on bare rock, or on an ice sheet** — and it costs nothing to ask.
    #[test]
    fn a_class_with_no_foliage_grows_nothing() {
        let (k, mats) = kinds();
        let water = crate::materials::index_of(&mats, "water");
        let sand = crate::materials::index_of(&mats, "sand");
        let granite = crate::materials::index_of(&mats, "granite");
        for mix in [vec![(water, 1.0f32)], vec![(sand, 0.6), (granite, 0.4)]] {
            let got = scatter(0.0, 0.0, 50.0, &k, &mats, |_, _| mix.clone(), 1.7, 500);
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
        let got = scatter(
            0.0,
            0.0,
            3.0,
            &k,
            &mats,
            |_, _| vec![(grass, 0.8f32)],
            1.7,
            400,
        );
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
