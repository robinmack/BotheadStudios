//! **The water rests on the ground** (docs/46 row 93).
//!
//! Robin, 2026-11-02, from a shore on the live site: *"it appears water rises above terrain in places,
//! which of course physics forbids."* And the rule that decides the fix: *"the render should come from
//! the results of physics applied to the assemblies."*
//!
//! A body's ocean is one of its components resting on another (docs/67): water on rock. Water at rest
//! has a level surface — an equipotential — and it covers every point of the ground that lies below
//! that surface. That sentence is the whole of the physics, and it is what this module states. It
//! replaces three answers the engine used to give to "is there water here", none of which was that
//! sentence:
//!
//! - the **ocean** was wherever a 1:110 m land mask said so (19.5 km per texel in the shipped raster),
//!   drawn flat at sea level — while the ground under a mask-"land" point was clamped up to sea level
//!   even where a streamed tile measured 20 m of water over it (the Ireland gun site, measured
//!   2026-11-02: mask land, raster +36 m, tile −20 m, drawn as a grass plain at exactly 0 m);
//! - **inland water** (a water-dominant land-cover class on mask-land) took the elevation raster's
//!   bilinear height at each vertex with no relief, while the land beside it carried signed relief —
//!   so the lake stood up to the relief amplitude above its own shore wherever the land dipped
//!   (measured: 0.97 m at Black Head from 3 m, 3.20 m near Lough Corrib from 50 m, 22.6 m from 500 m);
//! - **land below sea level** did not exist: `e.max(0.0)` in two places, so the Dead Sea's shore, Death
//!   Valley and a sea floor were all one flat plain at the datum.
//!
//! What this module decides, and what it leaves declared:
//!
//! - [`local_level`]: the level of the water that would rest at a place — the body's sea level, or a
//!   declared enclosed basin's own level, or none where a basin below sea level is dry.
//! - [`surface`]: given the ground and that level, where the surface the eye sees is, and whether it
//!   is water. Water never stands above the ground beside it, by construction: any ground below the
//!   level is under the water.
//! - [`ENCLOSED_BASINS`] is a **declared IOU** (Law V): a basin below sea level that the ocean does not
//!   reach holds water at its own level, or none. The honest computation is a flood fill of the measured
//!   elevation from the open ocean, and lake levels from measured data (ETOPO's surface carries them
//!   where a lake is resolved); until then the handful of basins that matter are listed with their
//!   sources, and anything below sea level outside them is sea.
//! - **Inland water's level is NOT decided here.** The shipped land cover is a derived approximation
//!   (`web/public/bodies/earth/SOURCES.txt`) that classes coastal hills as water (Black Head, measured
//!   at +15.6 m by its tile, is class 0 "water"); a rule that gave such a texel a level would flood the
//!   Burren. Lakes wait on measured land cover and a level datum — docs/46 row 93 names the debt and
//!   the measurement.

/// A basin below the body's sea level that the ocean does not fill (docs/46 row 93, a declared IOU).
///
/// `level_m` is the water's surface where the basin holds a lake, `None` where it is dry. Bounds are
/// generous boxes, not shorelines: inside the box the rule is "water at `level_m` wherever the ground
/// is below it", so a box larger than the lake costs nothing where the ground is above the level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basin {
    pub name: &'static str,
    /// [south, north] in degrees.
    pub lat: [f64; 2],
    /// [west, east] in degrees.
    pub lon: [f64; 2],
    /// The lake's surface, metres above the body's datum sphere; `None` for a dry basin.
    pub level_m: Option<f64>,
}

/// The enclosed basins below sea level that hold or do not hold water. Levels are the published
/// figures, rounded; they move by metres over years (the Dead Sea falls ~1 m/yr), which this table
/// does not follow — a level datum is the honest replacement.
///
/// Sources: Caspian Sea −28 m (CSDB, 2020s mean); Dead Sea −435 m (Israel Hydrological Service,
/// 2023); Sea of Galilee −213 m (Israel Water Authority, long-run mean); Salton Sea −72 m (USGS, 2023);
/// Lake Assal −155 m (Djibouti, SRTM); Death Valley floor −86 m, Qattara −133 m, Turpan −154 m,
/// Danakil −125 m, Lake Eyre −15 m — dry or ephemeral, no standing level (USGS/SRTM); the Dutch
/// polders −7 m, dry by dikes the engine does not model.
pub const ENCLOSED_BASINS: &[Basin] = &[
    Basin {
        name: "Caspian Sea",
        lat: [36.0, 47.5],
        lon: [46.0, 55.5],
        level_m: Some(-28.0),
    },
    Basin {
        name: "Dead Sea",
        lat: [30.9, 31.8],
        lon: [35.3, 35.7],
        level_m: Some(-435.0),
    },
    Basin {
        name: "Sea of Galilee",
        lat: [32.7, 32.95],
        lon: [35.5, 35.68],
        level_m: Some(-213.0),
    },
    Basin {
        name: "Salton Sea",
        lat: [33.0, 33.6],
        lon: [-116.2, -115.5],
        level_m: Some(-72.0),
    },
    Basin {
        name: "Lake Assal",
        lat: [11.6, 11.72],
        lon: [42.3, 42.5],
        level_m: Some(-155.0),
    },
    Basin {
        name: "Death Valley",
        lat: [35.7, 36.9],
        lon: [-117.4, -116.3],
        level_m: None,
    },
    Basin {
        name: "Qattara Depression",
        lat: [29.2, 30.6],
        lon: [26.1, 28.7],
        level_m: None,
    },
    Basin {
        name: "Turpan Depression",
        lat: [42.4, 43.3],
        lon: [88.4, 90.1],
        level_m: None,
    },
    Basin {
        name: "Danakil Depression",
        lat: [13.3, 14.6],
        lon: [39.9, 41.1],
        level_m: None,
    },
    Basin {
        name: "Lake Eyre",
        lat: [-29.6, -27.8],
        lon: [136.7, 138.1],
        level_m: None,
    },
    Basin {
        name: "Dutch polders",
        lat: [51.3, 53.5],
        lon: [3.3, 7.2],
        level_m: None,
    },
];

/// The declared basin a point lies in, if any.
pub fn enclosed_basin(lat: f64, lon: f64) -> Option<&'static Basin> {
    ENCLOSED_BASINS
        .iter()
        .find(|b| lat >= b.lat[0] && lat <= b.lat[1] && lon >= b.lon[0] && lon <= b.lon[1])
}

/// **The level of the water that rests at this place**, if any water does: the body's sea level
/// everywhere the ocean reaches, a declared basin's own level inside one, `None` in a dry basin.
/// Whether the ground here is actually under that water is [`surface`]'s question.
pub fn local_level(lat: f64, lon: f64, sea_level_m: f64) -> Option<f64> {
    match enclosed_basin(lat, lon) {
        Some(b) => b.level_m,
        None => Some(sea_level_m),
    }
}

/// **Where the surface is, and whether it is water** — the one rule every surface in the engine
/// applies (docs/46 row 93).
///
/// `elev_m` is the measured ground (raster, tile, or their blend — never clamped), `level` is
/// [`local_level`], and `relief` yields the ground's generated sub-datum relief, asked for only when
/// the measured ground stands above the water: relief is the LAND's statistic, and a sea floor's
/// roughness under 20 m of water decides nothing the eye can see. A land point whose relief dips
/// below the level is under water too — that is the shore's own waterline, and it is why no water
/// vertex can ever be higher than the land vertex beside it.
///
/// Returns `(height_m, is_water)`: the water's level and `true`, or the ground's height and `false`.
pub fn surface(elev_m: f64, level: Option<f64>, relief: impl FnOnce() -> f64) -> (f64, bool) {
    match level {
        Some(l) if elev_m <= l => (l, true),
        Some(l) => {
            let ground = elev_m + relief();
            if ground <= l {
                (l, true)
            } else {
                (ground, false)
            }
        }
        None => (elev_m + relief(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sea_floor_is_under_the_sea_and_a_hill_is_not() {
        // Galway Bay's floor, as the tile measured it under the gun site.
        assert_eq!(
            surface(-20.0, local_level(53.10, -9.45, 0.0), || 3.0),
            (0.0, true)
        );
        // Black Head, as its tile measured it.
        let (h, water) = surface(15.6, local_level(53.153, -9.266, 0.0), || -0.3);
        assert!(!water && (h - 15.3).abs() < 1e-9, "{h} {water}");
        // The waterline itself is water.
        assert_eq!(surface(0.0, Some(0.0), || 1.0), (0.0, true));
    }

    #[test]
    fn land_whose_relief_dips_below_the_level_is_under_the_water() {
        // A shore point 1 m up whose generated relief dips 2 m: the water fills the dip.
        assert_eq!(surface(1.0, Some(0.0), || -2.0), (0.0, true));
        // The same point with the relief rising: land, 4 m up.
        assert_eq!(surface(1.0, Some(0.0), || 3.0), (4.0, false));
    }

    #[test]
    fn the_water_is_never_above_the_ground_beside_it() {
        // Any pair of neighbouring points: a water surface height can never exceed a land height,
        // because any ground at or below the level is itself water.
        let level = Some(0.0);
        for e in [-50.0, -1.0, -0.001, 0.0, 0.001, 0.5, 2.0, 40.0] {
            for r in [-3.0, -0.5, 0.0, 0.5, 3.0] {
                let (h, water) = surface(e, level, || r);
                if water {
                    assert_eq!(h, 0.0);
                } else {
                    assert!(
                        h > 0.0,
                        "land at {e} + {r} drawn at {h}, below the water beside it"
                    );
                }
            }
        }
    }

    #[test]
    fn an_enclosed_basin_holds_its_own_lake_or_none() {
        // The Caspian floor is under the Caspian, at its own level, not the ocean's.
        assert_eq!(
            surface(-100.0, local_level(42.0, 50.0, 0.0), || 0.0),
            (-28.0, true)
        );
        // The Caspian shore at −10 m is dry land 18 m above the lake.
        assert_eq!(
            surface(-10.0, local_level(42.0, 50.0, 0.0), || 0.0),
            (-10.0, false)
        );
        // Death Valley's floor is dry land 86 m below sea level.
        assert_eq!(local_level(36.2, -116.8, 0.0), None);
        assert_eq!(surface(-86.0, None, || 0.5), (-85.5, false));
        // Outside every basin, below sea level is sea.
        assert_eq!(enclosed_basin(53.1, -9.45), None);
        assert_eq!(
            surface(-0.5, local_level(53.1, -9.45, 0.0), || 2.0),
            (0.0, true)
        );
    }

    #[test]
    fn the_basins_are_distinct_boxes_with_sane_bounds() {
        for b in ENCLOSED_BASINS {
            assert!(b.lat[0] < b.lat[1] && b.lon[0] < b.lon[1], "{}", b.name);
            assert!(
                b.level_m.is_none_or(|l| l < 0.0),
                "{} is below sea level",
                b.name
            );
        }
        for (i, a) in ENCLOSED_BASINS.iter().enumerate() {
            for b in &ENCLOSED_BASINS[i + 1..] {
                let overlap = a.lat[0] < b.lat[1]
                    && b.lat[0] < a.lat[1]
                    && a.lon[0] < b.lon[1]
                    && b.lon[0] < a.lon[1];
                assert!(!overlap, "{} overlaps {}", a.name, b.name);
            }
        }
    }
}
