//! **Elevation streamed by necessity** (docs/44 applied to DATA rather than to matter, docs/46 row 27).
//!
//! The shipped global raster is 19.5 km per texel, so below ~20 km altitude the frame sits inside a single
//! texel and every visible bump has to be invented. Measured, that invention is scaled to ~0.003 of what
//! the material could hold and extrapolated with the wrong exponent, and the result is a flat green fill
//! that does not change from 94 m altitude down to 10 cm.
//!
//! No global dataset fixes that by shipping: Copernicus GLO-30 is ~1.8 TB. What fixes it is fetching the
//! metres-per-pixel data **only where the camera is**, which is the engine's own resolution law pointed at
//! a different resource — necessity decides what is fetched, exactly as it decides what is particalized.
//!
//! **The seam.** The engine decides WHICH tiles it needs (that is a resolution decision, and it belongs to
//! the universe); the host fetches and decodes them (that is I/O, and the browser already decodes every
//! other raster this scene uses). The same split `load_world` already uses.
//!
//! **The source** is AWS Terrain Tiles — `terrarium` PNGs, global, unauthenticated, `Access-Control-Allow-
//! Origin: *`, derived from SRTM/other national DEMs. Elevation is packed into RGB, which is the same trick
//! the shipped raster uses, so nothing new had to be invented to read it.

use std::collections::HashMap;

/// The deepest zoom the AWS terrarium set publishes globally. Beyond it tiles simply 404, and a tile that
/// does not arrive is not an error — it is the resolution ladder running out of rungs, which is the honest
/// floor for measured data and exactly where generated relief has to take over.
pub const MAX_ZOOM: u32 = 15;

/// Web Mercator's equatorial ground resolution at zoom 0 for a 256-pixel tile (m/px) — the circumference
/// divided by 256. Every other resolution is this halved per zoom and scaled by `cos(latitude)`.
pub const Z0_PIXEL_M: f64 = 156_543.033_928_040_9;

/// **Terrarium's elevation encoding**: `(R·256 + G + B/256) − 32768` metres. Verified against ground truth
/// when this was wired up — Mt Whitney reads 4,417 m against a surveyed 4,421 m, the Dead Sea shore −412 m
/// against −430 m, and Everest 8,732 m against 8,849 m (a ~30 m pixel averages the summit down, which is
/// the data being honest about its own resolution rather than an error).
pub fn terrarium_elevation_m(r: u8, g: u8, b: u8) -> f64 {
    (r as f64) * 256.0 + (g as f64) + (b as f64) / 256.0 - 32768.0
}

/// One tile in the standard slippy-map scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

/// The ground size of one tile PIXEL (m) at this zoom and latitude. Web Mercator's scale is latitude
/// dependent — a tile at 60°N covers half the ground a tile at the equator does — so a caller asking "how
/// fine is my data here" must ask with a latitude.
pub fn pixel_ground_m(z: u32, lat_deg: f64, tile_px: u32) -> f64 {
    let scale = Z0_PIXEL_M * 256.0 / tile_px.max(1) as f64;
    scale * lat_deg.to_radians().cos().abs() / (1u64 << z) as f64
}

/// **The zoom whose pixels are about the size the observer can resolve** — the same angular budget that
/// sizes particle granularity and the raster hand-off, asked about a tile pyramid instead (Law II: "how
/// fine can this viewer resolve" has one answer). Clamped to what the set actually publishes.
pub fn zoom_for_ground_size(target_m: f64, lat_deg: f64, tile_px: u32) -> u32 {
    if !(target_m > 0.0) {
        return MAX_ZOOM;
    }
    let scale = Z0_PIXEL_M * 256.0 / tile_px.max(1) as f64 * lat_deg.to_radians().cos().abs();
    // scale / 2^z ≈ target  ⇒  z ≈ log2(scale / target)
    let z = (scale / target_m).log2().ceil();
    z.clamp(0.0, MAX_ZOOM as f64) as u32
}

/// Fractional tile coordinates of a point — the integer part names the tile, the fraction locates the
/// pixel inside it.
pub fn tile_coords(lat_deg: f64, lon_deg: f64, z: u32) -> (f64, f64) {
    let n = (1u64 << z) as f64;
    let x = (lon_deg + 180.0).rem_euclid(360.0) / 360.0 * n;
    // Web Mercator's y is unbounded at the poles, so the projection is only defined to ±85.05°.
    let lat = lat_deg.clamp(-85.051_128, 85.051_128).to_radians();
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    (x, y)
}

/// The tiles covering a disc of `radius` TILES around the point, at zoom `z`. `radius = 1` is the 3×3
/// patch under the camera. Longitude wraps; latitude is clamped, so a polar camera simply gets fewer
/// distinct tiles rather than an invalid one.
pub fn tiles_around(lat_deg: f64, lon_deg: f64, z: u32, radius: i32) -> Vec<TileId> {
    let n = 1i64 << z;
    let (fx, fy) = tile_coords(lat_deg, lon_deg, z);
    let (cx, cy) = (fx.floor() as i64, fy.floor() as i64);
    let mut out = Vec::new();
    for dy in -radius as i64..=radius as i64 {
        for dx in -radius as i64..=radius as i64 {
            let y = cy + dy;
            if y < 0 || y >= n {
                continue; // past the pole: no tile exists, and inventing one would be a lie
            }
            let x = (cx + dx).rem_euclid(n);
            out.push(TileId {
                z,
                x: x as u32,
                y: y as u32,
            });
        }
    }
    // Nearest first, so a bandwidth-limited host fetches what the camera is looking at before the edges.
    out.sort_by(|a, b| {
        let d = |t: &TileId| {
            let ddx = (t.x as f64 + 0.5) - fx;
            let ddy = (t.y as f64 + 0.5) - fy;
            ddx * ddx + ddy * ddy
        };
        d(a).total_cmp(&d(b))
    });
    out
}

/// A decoded tile: `tile_px` square, RGB(A) interleaved, terrarium-encoded.
pub struct Tile {
    pub id: TileId,
    pub px: u32,
    pub chans: usize,
    pub data: Vec<u8>,
}

impl Tile {
    /// Bilinear elevation (m) at a fractional pixel, clamped at the tile's own edges. Interpolating the
    /// DECODED metres rather than the packed bytes, for the same reason `Raster::elevation_m_at` does:
    /// the packing is not linear, so lerping the bytes would blend two different powers of 256.
    fn elevation_at_px(&self, px: f64, py: f64) -> f64 {
        let n = self.px as i64;
        let at = |ix: i64, iy: i64| {
            let ix = ix.clamp(0, n - 1) as usize;
            let iy = iy.clamp(0, n - 1) as usize;
            let o = (iy * self.px as usize + ix) * self.chans;
            terrarium_elevation_m(self.data[o], self.data[o + 1], self.data[o + 2])
        };
        let (x0, y0) = (px.floor() as i64, py.floor() as i64);
        let (tx, ty) = (px - x0 as f64, py - y0 as f64);
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
        let bot = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
        top * (1.0 - ty) + bot * ty
    }
}

/// The tiles currently held, and the patch they are being held for.
#[derive(Default)]
pub struct TileStore {
    tiles: HashMap<TileId, Tile>,
    /// The patch the engine last asked for — centre and zoom. Sampling blends OUT to the patch edge, so
    /// this is what says where the edge is.
    want: Option<(f64, f64, u32, i32)>, // lat, lon, zoom, radius
}

impl TileStore {
    pub fn len(&self) -> usize {
        self.tiles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Declare the patch the observer needs. Tiles outside it are dropped, which is what bounds the store
    /// without an eviction policy needing to be invented: the camera moving IS the eviction.
    pub fn want_patch(&mut self, lat: f64, lon: f64, z: u32, radius: i32) {
        self.want = Some((lat, lon, z, radius));
        let keep: std::collections::HashSet<TileId> =
            tiles_around(lat, lon, z, radius).into_iter().collect();
        self.tiles.retain(|id, _| keep.contains(id));
    }

    /// The tiles the current patch needs that are not held yet, nearest first.
    pub fn missing(&self) -> Vec<TileId> {
        match self.want {
            None => Vec::new(),
            Some((lat, lon, z, r)) => tiles_around(lat, lon, z, r)
                .into_iter()
                .filter(|id| !self.tiles.contains_key(id))
                .collect(),
        }
    }

    pub fn insert(&mut self, tile: Tile) {
        // Only keep what the current patch wants — a tile that arrives after the camera moved is stale.
        if let Some((lat, lon, z, r)) = self.want {
            if tile.id.z != z || !tiles_around(lat, lon, z, r).contains(&tile.id) {
                return;
            }
        }
        self.tiles.insert(tile.id, tile);
    }

    /// The ground size of one held pixel at this latitude — what the generated relief must key off where
    /// tiles cover, in place of the global raster's 19.5 km.
    pub fn pixel_ground_m(&self, lat: f64) -> Option<f64> {
        let (_, _, z, _) = self.want?;
        let px = self.tiles.values().next()?.px;
        Some(pixel_ground_m(z, lat, px))
    }

    /// **Measured elevation here, and how much to trust it** — `(metres, weight)`, weight in `[0,1]`.
    ///
    /// `None` when no held tile covers the point. The WEIGHT is what keeps the patch from having a visible
    /// edge: it falls to zero across the outermost tile, and a caller blends
    /// `raster + weight·(tile − raster)`. That is not a fudge and not a new idea — it is the same
    /// "detail fades in rather than popping" rule the octave count and the cap cross-fade already use, and
    /// at weight 0 the surface is exactly the raster it always was, so the two never disagree at the seam.
    pub fn elevation_m_at(&self, lat: f64, lon: f64) -> Option<(f64, f64)> {
        let (clat, clon, z, radius) = self.want?;
        let (fx, fy) = tile_coords(lat, lon, z);
        let tile = self.tiles.get(&TileId {
            z,
            x: (fx.floor() as i64).rem_euclid(1i64 << z) as u32,
            y: fy.floor() as u32,
        })?;
        let px = tile.px as f64;
        let e = tile.elevation_at_px((fx.fract()) * px - 0.5, (fy.fract()) * px - 0.5);

        // Distance from the patch centre in TILES, against the radius the patch was built with. The last
        // tile of the patch is the fade band, so the blend width is a property of the patch, not a dial.
        let (cfx, cfy) = tile_coords(clat, clon, z);
        let d = ((fx - cfx).abs()).max((fy - cfy).abs());
        let outer = (radius as f64 + 0.5).max(0.5);
        let inner = (outer - 1.0).max(0.0);
        let w = if d <= inner {
            1.0
        } else {
            let t = ((outer - d) / (outer - inner)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t) // smoothstep, so the seam has no visible crease
        };
        Some((e, w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The encoding, against the ground truth it was checked on when this was wired up. If this drifts,
    /// every elevation the engine streams is wrong by a constant or a factor and nothing else would say so.
    #[test]
    fn terrarium_decodes_to_real_metres() {
        assert_eq!(terrarium_elevation_m(128, 0, 0), 0.0); // 128·256 = 32768 = the zero offset
        assert_eq!(terrarium_elevation_m(128, 1, 0), 1.0);
        assert_eq!(terrarium_elevation_m(127, 255, 0), -1.0);
        assert!((terrarium_elevation_m(128, 0, 128) - 0.5).abs() < 1e-12); // the fractional byte
                                                                           // The real fixtures: Whitney's tile pixel and the Dead Sea's, as fetched.
        let whitney = terrarium_elevation_m(145, 65, 51);
        assert!(
            (whitney - 4417.0).abs() < 2.0,
            "Mt Whitney should decode near 4,417 m, got {whitney}"
        );
    }

    /// Slippy-map coordinates must land where the world says. Checked against the corners and against a
    /// worked example, because an off-by-one in the y projection puts the camera on a different continent.
    #[test]
    fn tile_coordinates_follow_the_slippy_map_convention() {
        // Zoom 0 is one tile holding the world; its centre is (0,0) lat/lon at (0.5, 0.5).
        let (x, y) = tile_coords(0.0, 0.0, 0);
        assert!((x - 0.5).abs() < 1e-12 && (y - 0.5).abs() < 1e-12);
        // The anti-meridian is the left edge; the north edge of the projection is y = 0.
        assert!((tile_coords(0.0, -180.0, 0).0).abs() < 1e-12);
        assert!(tile_coords(85.05, 0.0, 0).1 < 1e-3);
        assert!(tile_coords(-85.05, 0.0, 0).1 > 1.0 - 1e-3);
        // At zoom 12 there are 4096 tiles across, and Everest sits in a tile that was actually FETCHED and
        // decoded to check this: `terrarium/12/3037/1716.png`, whose pixel (3,20) reads rgb(162,28,0) =
        // 8,732 m, and whose maximum is 8,753 m. ★ The first version of this assertion carried (3054,1808)
        // — numbers I typed rather than computed, and the code was right and the fixture wrong. A fixture
        // is a measurement too.
        let (fx, fy) = tile_coords(27.9881, 86.9250, 12);
        assert_eq!((fx.floor() as u32, fy.floor() as u32), (3037, 1716));
        assert!(
            (terrarium_elevation_m(162, 28, 0) - 8732.0).abs() < 0.5,
            "the pixel that tile really holds"
        );
        // Latitude beyond Mercator's limit clamps rather than exploding.
        assert!(tile_coords(89.9, 0.0, 5).1.is_finite());
    }

    /// The zoom is chosen by the SAME question that sizes every other resolution decision: how fine can
    /// the viewer resolve here. Finer targets ask for deeper zooms, and the ladder stops where the data
    /// stops rather than pretending to go further.
    #[test]
    fn zoom_follows_the_resolvable_size_and_stops_where_the_data_does() {
        // Equator, 256 px tiles: zoom 0 is ~611 m/px, and each zoom halves it.
        assert!((pixel_ground_m(0, 0.0, 256) - Z0_PIXEL_M).abs() < 1e-6);
        assert!((pixel_ground_m(1, 0.0, 256) - Z0_PIXEL_M / 2.0).abs() < 1e-6);
        // Latitude compresses Mercator: 60° is exactly half the ground per pixel.
        assert!((pixel_ground_m(5, 60.0, 256) / pixel_ground_m(5, 0.0, 256) - 0.5).abs() < 1e-3);
        // Asking for a metre of ground gets the deepest published zoom, not an imaginary one.
        assert_eq!(zoom_for_ground_size(1.0, 0.0, 256), MAX_ZOOM);
        assert_eq!(zoom_for_ground_size(1e-6, 45.0, 256), MAX_ZOOM);
        // And a coarse target gets a shallow one; the chosen zoom's pixel is at least as fine as asked.
        for target in [10.0, 100.0, 1_000.0, 10_000.0] {
            let z = zoom_for_ground_size(target, 39.0, 256);
            assert!(
                pixel_ground_m(z, 39.0, 256) <= target * 1.0001,
                "z={z} for target {target} m gives {} m/px",
                pixel_ground_m(z, 39.0, 256)
            );
            assert!(z <= MAX_ZOOM);
        }
        // A nonsense target must not panic or produce a wild zoom.
        assert!(zoom_for_ground_size(0.0, 0.0, 256) <= MAX_ZOOM);
    }

    /// The patch is bounded and centred, wraps at the anti-meridian, and refuses to invent tiles past the
    /// pole. Bounded is the point: this is what keeps "stream the data" from meaning "download the planet".
    #[test]
    fn the_patch_is_bounded_wraps_and_stops_at_the_pole() {
        let t = tiles_around(39.0, -106.0, 12, 1);
        assert_eq!(t.len(), 9, "radius 1 is a 3x3 patch");
        // Nearest first: the first tile is the one the camera is actually over.
        let (fx, fy) = tile_coords(39.0, -106.0, 12);
        assert_eq!(t[0].x, fx.floor() as u32);
        assert_eq!(t[0].y, fy.floor() as u32);
        // Across the anti-meridian, x wraps instead of going negative or off the end.
        let w = tiles_around(0.0, -179.99, 3, 1);
        assert_eq!(w.len(), 9);
        assert!(w.iter().any(|t| t.x == 7), "wrapped to the far edge");
        assert!(w.iter().all(|t| t.x < 8 && t.y < 8));
        // At the pole the patch is clipped, not wrapped: there is no tile north of the north edge.
        let p = tiles_around(85.0, 0.0, 3, 1);
        assert!(
            p.len() < 9 && !p.is_empty(),
            "clipped at the pole, got {}",
            p.len()
        );
    }

    fn flat_tile(id: TileId, metres: f64) -> Tile {
        // Encode `metres` into every pixel of a small tile.
        let v = metres + 32768.0;
        let r = (v / 256.0).floor() as u8;
        let g = (v - (r as f64) * 256.0).floor() as u8;
        let px = 8u32;
        let mut data = Vec::with_capacity((px * px) as usize * 3);
        for _ in 0..px * px {
            data.extend_from_slice(&[r, g, 0]);
        }
        Tile {
            id,
            px,
            chans: 3,
            data,
        }
    }

    /// **The seam must not be visible, and at the patch edge the surface must be EXACTLY the raster it
    /// always was.** The weight is what buys that: 1 in the middle, smoothly 0 at the rim, so a caller
    /// blending `raster + w·(tile − raster)` lands back on the raster with no step to see.
    #[test]
    fn the_patch_fades_to_nothing_at_its_rim() {
        let mut store = TileStore::default();
        let (lat, lon, z, r) = (39.0, -106.0, 10, 1);
        store.want_patch(lat, lon, z, r);
        assert_eq!(store.missing().len(), 9);
        for id in tiles_around(lat, lon, z, r) {
            store.insert(flat_tile(id, 2500.0));
        }
        assert!(store.missing().is_empty(), "the patch is complete");
        assert_eq!(store.len(), 9);

        // Dead centre: full weight and the tile's own elevation.
        let (e, w) = store.elevation_m_at(lat, lon).expect("covered");
        assert!((e - 2500.0).abs() < 1.0, "elevation {e}");
        assert!((w - 1.0).abs() < 1e-9, "full trust at the centre, got {w}");

        // Walking out to the rim, the weight must fall monotonically to zero and never rise.
        let px_m = pixel_ground_m(z, lat, 8);
        let tile_m = px_m * 8.0;
        let mut last = 1.0;
        let mut reached_zero = false;
        for step in 0..40 {
            let dlon = (step as f64 * tile_m * 0.05) / (111_320.0 * lat.to_radians().cos());
            match store.elevation_m_at(lat, lon + dlon) {
                Some((_, w)) => {
                    assert!(
                        w <= last + 1e-9,
                        "weight rose at step {step}: {last} -> {w}"
                    );
                    last = w;
                    if w == 0.0 {
                        reached_zero = true;
                    }
                }
                None => {
                    reached_zero = true;
                    break;
                }
            }
        }
        assert!(
            reached_zero,
            "the patch must fade to nothing, ended at {last}"
        );

        // Outside the patch entirely: no answer at all, so the caller keeps the raster.
        assert!(store.elevation_m_at(lat, lon + 40.0).is_none());
    }

    /// Moving the camera evicts what it left behind, so the store cannot grow without bound — and a tile
    /// that arrives after the camera moved on is dropped rather than stored against the wrong patch.
    #[test]
    fn the_camera_moving_is_the_eviction_policy() {
        let mut store = TileStore::default();
        store.want_patch(39.0, -106.0, 10, 1);
        let old = tiles_around(39.0, -106.0, 10, 1);
        for id in &old {
            store.insert(flat_tile(*id, 2500.0));
        }
        assert_eq!(store.len(), 9);
        // A long way away: nothing from the old patch survives.
        store.want_patch(-33.0, 18.0, 10, 1);
        assert_eq!(store.len(), 0, "the old patch is gone");
        // A late arrival from the old patch is refused rather than stored.
        store.insert(flat_tile(old[0], 2500.0));
        assert_eq!(store.len(), 0, "a stale tile must not be kept");
        // A zoom change also invalidates: the same ground at a different rung is a different tile.
        store.want_patch(-33.0, 18.0, 11, 1);
        assert!(store.missing().iter().all(|t| t.z == 11));
    }
}
