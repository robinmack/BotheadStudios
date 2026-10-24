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
    /// ★ **Every pixel is the void sentinel** — the tile exists on the server and measures nothing.
    /// The terrarium set answers a request past its local coverage with a valid PNG of `0,0,0`
    /// (−32,768 m), not a 404: over the Alps at 46.5 N 10 E, six of the nine zoom-14 tiles and six of
    /// the nine zoom-15 tiles are this (measured 2026-10-24), and the engine read them as ground at
    /// sea level — a 2.3 km pit under every vertex they covered (docs/46 row 89). A void tile is held,
    /// so the host is not asked for it again, and it answers `None` like ground no tile covers.
    pub void: bool,
    /// ★ **Pixels the provider's mosaic seam ran through, repaired from their lateral neighbours** —
    /// a DECLARED stand-in (Law V), counted so it is never silent. The real Alps tiles beside the
    /// void ones carry a one-pixel-wide column, the full height of the tile, of values 2,000–7,000 m
    /// below BOTH its neighbours while those neighbours agree with each other to within metres
    /// (zoom 14 column 26: 2,396 m · −4,156 m · 2,391 m; zoom 15 column 53, the same seam). No
    /// ground is a canyon one pixel wide and three kilometres deep; that is where two source DEMs
    /// were stitched. Read as ground it drew a trench with a 2.9 km step, and fed the slope law a
    /// gradient of 435 that became a 583 m generated spike beside it (docs/46 row 89).
    pub seams_repaired: u32,
}

/// The terrarium byte triple that means "no measurement": `0,0,0` decodes to −32,768 m, a depth no
/// ground on Earth reaches (the Challenger Deep is −10,935 m), so it cannot be confused with bathymetry.
#[inline]
pub fn is_void_rgb(r: u8, g: u8, b: u8) -> bool {
    r == 0 && g == 0 && b == 0
}

/// How far below BOTH neighbours a one-pixel line has to sit, along at least nine tenths of a tile's
/// width or height, to be a mosaic seam rather than ground. The deepest slot canyons on Earth are a few
/// hundred metres deep and tens of metres wide; nothing is one pixel wide and this deep with its two
/// walls at the same height.
pub const SEAM_NOTCH_M: f64 = 200.0;

/// Metres → the terrarium byte triple, the inverse of [`terrarium_elevation_m`].
fn terrarium_bytes(metres: f64) -> (u8, u8, u8) {
    let v = (metres + 32768.0).clamp(0.0, 65535.996);
    let r = (v / 256.0).floor();
    let g = (v - r * 256.0).floor();
    let b = ((v - r * 256.0 - g) * 256.0).floor();
    (r as u8, g as u8, b as u8)
}

impl Tile {
    /// Decoded bytes → a tile, with its void-ness measured once rather than guessed later, and the
    /// provider's seam lines repaired and counted.
    pub fn new(id: TileId, px: u32, chans: usize, mut data: Vec<u8>) -> Tile {
        let n = (px as usize) * (px as usize);
        let complete = chans >= 3 && data.len() >= n * chans;
        let void = complete
            && (0..n).all(|i| {
                let o = i * chans;
                is_void_rgb(data[o], data[o + 1], data[o + 2])
            });
        let mut seams_repaired = 0u32;
        if complete && !void && px >= 3 {
            let w = px as usize;
            let e_at = |d: &[u8], x: usize, y: usize| -> Option<f64> {
                let o = (y * w + x) * chans;
                let (r, g, b) = (d[o], d[o + 1], d[o + 2]);
                if is_void_rgb(r, g, b) {
                    None
                } else {
                    Some(terrarium_elevation_m(r, g, b))
                }
            };
            // A notch: the middle sits more than SEAM_NOTCH_M below both sides, and the two sides agree
            // with each other — a real cliff has sides that do not.
            let notch = |a: Option<f64>, m: Option<f64>, b: Option<f64>| -> bool {
                match (a, m, b) {
                    (Some(a), Some(m), Some(b)) => {
                        m < a.min(b) - SEAM_NOTCH_M
                            && (a - b).abs() < 0.2 * a.abs().max(b.abs()).max(1.0)
                    }
                    _ => false,
                }
            };
            let seam_cols: Vec<usize> = (1..w - 1)
                .filter(|&x| {
                    let notched = (0..w)
                        .filter(|&y| {
                            notch(
                                e_at(&data, x - 1, y),
                                e_at(&data, x, y),
                                e_at(&data, x + 1, y),
                            )
                        })
                        .count();
                    notched * 10 >= w * 9
                })
                .collect();
            let seam_rows: Vec<usize> = (1..w - 1)
                .filter(|&y| {
                    let notched = (0..w)
                        .filter(|&x| {
                            notch(
                                e_at(&data, x, y - 1),
                                e_at(&data, x, y),
                                e_at(&data, x, y + 1),
                            )
                        })
                        .count();
                    notched * 10 >= w * 9
                })
                .collect();
            for &x in &seam_cols {
                for y in 0..w {
                    if let (Some(a), Some(b)) = (e_at(&data, x - 1, y), e_at(&data, x + 1, y)) {
                        let (r, g, bb) = terrarium_bytes(0.5 * (a + b));
                        let o = (y * w + x) * chans;
                        data[o] = r;
                        data[o + 1] = g;
                        data[o + 2] = bb;
                        seams_repaired += 1;
                    }
                }
            }
            for &y in &seam_rows {
                for x in 0..w {
                    if let (Some(a), Some(b)) = (e_at(&data, x, y - 1), e_at(&data, x, y + 1)) {
                        let (r, g, bb) = terrarium_bytes(0.5 * (a + b));
                        let o = (y * w + x) * chans;
                        data[o] = r;
                        data[o + 1] = g;
                        data[o + 2] = bb;
                        seams_repaired += 1;
                    }
                }
            }
            if seams_repaired > 0 {
                log::info!(
                    "tile {}/{}/{}: repaired {} seam pixel(s) across {} column(s) and {} row(s)",
                    id.z,
                    id.x,
                    id.y,
                    seams_repaired,
                    seam_cols.len(),
                    seam_rows.len()
                );
            }
        }
        Tile {
            id,
            px,
            chans,
            data,
            void,
            seams_repaired,
        }
    }

    /// Bilinear elevation (m) at a fractional pixel, clamped at the tile's own edges. Interpolating the
    /// DECODED metres rather than the packed bytes, for the same reason `Raster::elevation_m_at` does:
    /// the packing is not linear, so lerping the bytes would blend two different powers of 256.
    ///
    /// `None` when any of the four pixels is the void sentinel: a measurement interpolated with "no
    /// measurement" is not a measurement. The provider's mosaic seam — a one-pixel line of garbage that
    /// is NOT the sentinel — is repaired at decode (`Tile::new`, `seams_repaired`), so by the time a
    /// sample is asked for here it is gone.
    fn elevation_at_px(&self, px: f64, py: f64) -> Option<f64> {
        let n = self.px as i64;
        let at = |ix: i64, iy: i64| -> Option<f64> {
            let ix = ix.clamp(0, n - 1) as usize;
            let iy = iy.clamp(0, n - 1) as usize;
            let o = (iy * self.px as usize + ix) * self.chans;
            let (r, g, b) = (self.data[o], self.data[o + 1], self.data[o + 2]);
            if is_void_rgb(r, g, b) {
                None
            } else {
                Some(terrarium_elevation_m(r, g, b))
            }
        };
        let (x0, y0) = (px.floor() as i64, py.floor() as i64);
        let (tx, ty) = (px - x0 as f64, py - y0 as f64);
        let top = at(x0, y0)? * (1.0 - tx) + at(x0 + 1, y0)? * tx;
        let bot = at(x0, y0 + 1)? * (1.0 - tx) + at(x0 + 1, y0 + 1)? * tx;
        Some(top * (1.0 - ty) + bot * ty)
    }
}

/// The tiles currently held, and the patch they are being held for.
#[derive(Default)]
pub struct TileStore {
    tiles: HashMap<TileId, Tile>,
    /// The patch the engine last asked for — centre and zoom. Sampling blends OUT to the patch edge, so
    /// this is what says where the edge is.
    want: Option<(f64, f64, u32, i32)>, // lat, lon, zoom, radius
    /// Tiles that arrived entirely void — where the data stops, remembered so the zoom stops there too.
    known_void: std::collections::HashSet<TileId>,
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
        if tile.void {
            // Remembered past eviction: this is where the DATA stops, and the zoom must not ask past
            // it again next frame (`deepest_measured_zoom`).
            self.known_void.insert(tile.id);
        }
        self.tiles.insert(tile.id, tile);
    }

    /// ★ **The deepest zoom at which the data has measured THIS ground**, no deeper than `z`. The set
    /// publishes to zoom 15 globally, but its coverage at the deep zooms is local — over the Alps the
    /// zoom-14 and zoom-15 tiles are void where the zoom-13 ones are real. `zoom_for_ground_size`
    /// says what the viewer could resolve; this says what the data can answer, and the patch is asked
    /// for at the shallower of the two. A void is learned by fetching it once (`insert`), so a descent
    /// pays one empty tile per step past the data, not an empty surface.
    pub fn deepest_measured_zoom(&self, lat: f64, lon: f64, z: u32) -> u32 {
        let mut z = z;
        while z > 0 {
            let (fx, fy) = tile_coords(lat, lon, z);
            let id = TileId {
                z,
                x: (fx.floor() as i64).rem_euclid(1i64 << z) as u32,
                y: fy.floor() as u32,
            };
            if !self.known_void.contains(&id) {
                break;
            }
            z -= 1;
        }
        z
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
        let (_clat, _clon, z, _radius) = self.want?;
        let n = 1i64 << z;
        let (fx, fy) = tile_coords(lat, lon, z);
        let (tx, ty) = (fx.floor() as i64, fy.floor() as i64);
        let tile = self.tiles.get(&TileId {
            z,
            x: tx.rem_euclid(n) as u32,
            y: fy.floor() as u32,
        })?;
        if tile.void {
            return None; // held so it is not fetched again; answers like ground no tile covers
        }
        let px = tile.px as f64;
        let e = tile.elevation_at_px((fx.fract()) * px - 0.5, (fy.fract()) * px - 0.5)?;

        // ★★ **THE FADE IS A PROPERTY OF THE TILES HELD, NOT OF THE PATCH THAT WAS ASKED FOR**
        // (docs/46 row 89). It used to run from the camera POINT out to `radius + 0.5` tiles, while the
        // tiles actually held are the 3×3 around the camera's TILE — two different squares. Up to half
        // a tile of the fade band fell outside the held set, where the lookup above returns `None` and
        // the surface drops to the raster with the weight still at ~0.35: a hard seam, hundreds of
        // metres tall in the Alps, with every tile present. And a tile that had not ARRIVED yet left
        // its neighbour's edge at weight 1 — the state a descending camera is in most of the way down.
        // Measured 2026-10-24 on the segment's own edges: slopes of 20–38 with all nine tiles held,
        // 273 with one, against 0.1 on the raster alone.
        //
        // So the weight falls to zero toward any edge of THIS tile whose neighbour across it is not
        // held, over the tile's own width, and is 1 everywhere else. A tile surrounded by held tiles is
        // trusted throughout; the outermost held tiles fade to the raster at their outer edges; a tile
        // that arrived alone is a smooth bump of measured detail that widens as its neighbours land.
        // Every seam is continuous by construction, in every state the stream passes through — the
        // same "detail fades in rather than popping" rule, now applied to what is actually there.
        // "Held" means held AND measured: a void tile is kept so the host is not asked for it again,
        // but it is unmeasured ground, and the fade treats it exactly like a tile that never came.
        let held = |dx: i64, dy: i64| -> bool {
            let y = ty + dy;
            y >= 0
                && y < n
                && self
                    .tiles
                    .get(&TileId {
                        z,
                        x: (tx + dx).rem_euclid(n) as u32,
                        y: y as u32,
                    })
                    .is_some_and(|t| !t.void)
        };
        // Where inside this tile, 0..1 each way.
        let (ux, uy) = (fx.fract(), fy.fract());
        // Distance, in tiles, to the nearest ground that is NOT measured: a missing side neighbour is a
        // half-plane (distance to the edge), a missing corner neighbour a quadrant (distance to the
        // corner). 1.0 means nothing unmeasured within a tile's width.
        let mut reach = 1.0f64;
        if !held(-1, 0) {
            reach = reach.min(ux);
        }
        if !held(1, 0) {
            reach = reach.min(1.0 - ux);
        }
        if !held(0, -1) {
            reach = reach.min(uy);
        }
        if !held(0, 1) {
            reach = reach.min(1.0 - uy);
        }
        if !held(-1, -1) {
            reach = reach.min(ux.hypot(uy));
        }
        if !held(1, -1) {
            reach = reach.min((1.0 - ux).hypot(uy));
        }
        if !held(-1, 1) {
            reach = reach.min(ux.hypot(1.0 - uy));
        }
        if !held(1, 1) {
            reach = reach.min((1.0 - ux).hypot(1.0 - uy));
        }
        let t = reach.clamp(0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t); // smoothstep, so the seam has no visible crease
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

    /// ★ **A ONE-PIXEL TRENCH THROUGH A WHOLE TILE IS A MOSAIC SEAM, NOT A CANYON** (docs/46 row 89).
    /// The real Alps tiles beside the void ones carry exactly this, full height, 2–7 km below both
    /// neighbours; read as ground it was a 2.9 km trench and a 583 m generated spike. A real cliff —
    /// whose two sides disagree — must be left exactly as measured.
    #[test]
    fn a_seam_column_is_repaired_and_a_cliff_is_not() {
        let px = 8usize;
        let enc = |m: f64| -> [u8; 3] {
            let (r, g, b) = terrarium_bytes(m);
            [r, g, b]
        };
        // A 2,500 m plateau with column 3 reading −563 m in every row.
        let mut data = Vec::with_capacity(px * px * 3);
        for _y in 0..px {
            for x in 0..px {
                data.extend_from_slice(&enc(if x == 3 { -563.0 } else { 2500.0 }));
            }
        }
        let id = TileId {
            z: 15,
            x: 17294,
            y: 11591,
        };
        let t = Tile::new(id, px as u32, 3, data);
        assert!(!t.void);
        assert_eq!(
            t.seams_repaired, px as u32,
            "every pixel of the seam column"
        );
        let e = t.elevation_at_px(3.0, 4.0).expect("measured");
        assert!(
            (e - 2500.0).abs() < 0.01,
            "the seam reads as its neighbours: {e}"
        );
        // A real cliff: 2,500 m on the left, 1,000 m on the right, step between columns 3 and 4.
        let mut cliff = Vec::with_capacity(px * px * 3);
        for _y in 0..px {
            for x in 0..px {
                cliff.extend_from_slice(&enc(if x < 4 { 2500.0 } else { 1000.0 }));
            }
        }
        let c = Tile::new(id, px as u32, 3, cliff);
        assert_eq!(
            c.seams_repaired, 0,
            "a cliff is ground and stays as measured"
        );
        let lo = c.elevation_at_px(5.0, 2.0).expect("measured");
        assert!((lo - 1000.0).abs() < 0.01, "{lo}");
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
        Tile::new(id, px, 3, data)
    }

    /// ★ **A VOID TILE IS NOT GROUND AT SEA LEVEL** (docs/46 row 89). The set answers past its local
    /// coverage with a valid PNG of `0,0,0` — −32,768 m — and six of the nine Alps tiles at zoom 14 and
    /// again at zoom 15 were exactly that; the engine clamped them to 0 m and dug a 2.3 km pit under
    /// every vertex they covered, with the real ground standing around it as pillars.
    #[test]
    fn a_void_tile_measures_nothing_and_its_neighbour_fades_toward_it() {
        let mut store = TileStore::default();
        let (lat, lon, z, r) = (46.5, 10.0, 14, 1);
        store.want_patch(lat, lon, z, r);
        let ids = tiles_around(lat, lon, z, r);
        let centre = ids[0];
        let east = TileId {
            z,
            x: centre.x + 1,
            y: centre.y,
        };
        for id in &ids {
            if *id == east {
                let px = 8u32;
                store.insert(Tile::new(*id, px, 3, vec![0u8; (px * px * 3) as usize]));
            } else {
                store.insert(flat_tile(*id, 2500.0));
            }
        }
        assert!(
            store.missing().is_empty(),
            "a void tile is held, so the host is not asked for it again"
        );
        let n = (1u64 << z) as f64;
        let lon_of = |fx: f64| fx / n * 360.0 - 180.0;
        // Inside the void tile: no measurement, exactly as if no tile covered it.
        let in_void = lon_of(east.x as f64 + 0.5);
        assert!(store.elevation_m_at(lat, in_void).is_none());
        // In the real tile beside it, the weight falls to zero toward the edge that faces the void…
        let at = |fx: f64| store.elevation_m_at(lat, lon_of(fx)).expect("covered").1;
        let (w_mid, w_near, w_edge) = (
            at(centre.x as f64 + 0.5),
            at(centre.x as f64 + 0.95),
            at(centre.x as f64 + 0.999),
        );
        assert!(
            w_mid > w_near && w_near > w_edge && w_edge < 0.01,
            "toward the void: mid {w_mid} near {w_near} edge {w_edge}"
        );
        // …and stays trusted on the side that faces measured ground.
        let w_west = at(centre.x as f64 + 0.05);
        assert!(w_west > 0.99, "west edge faces a real tile: {w_west}");
        // The zoom ladder stops above the void, and nowhere else.
        assert_eq!(store.deepest_measured_zoom(lat, in_void, z), z - 1);
        assert_eq!(store.deepest_measured_zoom(lat, lon, z), z);
        // A single void pixel inside a real tile is not a measurement either, and does not make the
        // tile void.
        let mut t = flat_tile(centre, 2500.0);
        t.data[0] = 0;
        t.data[1] = 0;
        t.data[2] = 0;
        assert!(!t.void);
        assert!(t.elevation_at_px(0.0, 0.0).is_none());
        assert!(t.elevation_at_px(4.0, 4.0).is_some());
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
