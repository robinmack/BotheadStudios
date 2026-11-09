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

/// ★★ **THE TILES ARE HELD AS A STACK OF RINGS, AND EACH FADES TO THE RING BENEATH IT** (docs/46
/// row 97). One 3×3 patch at the viewer's zoom faded straight to the shipped 19.5 km raster: over
/// Kamchatka at 7 km the raster's texel says 552 m where the tiles say 232 m, and that 320 m
/// difference was spread across the outermost tiles as a skirt of slope 0.34 — a cliff ring around
/// the patch, eight times steeper than any ground inside it, with a crease along each corner tile's
/// diagonal where the fade's distance-to-the-nearest-edge changed direction (the hourglass seen
/// 2026-11-07; measured 2026-11-08 with `ground_parts_at`). Now beneath the finest 3×3 sit 3×3
/// patches every [`RING_STEP`] zooms down to [`RING_FLOOR_ZOOM`]: each ring fades over its outer
/// tiles to the ring beneath it, and only the coarsest fades to the raster, where a 10 km pixel
/// meets a 19.5 km one. The coarse rings' tiles arrive first (`missing` lists them first), so a
/// descent sees the ground resolve from the large scale down — and a void tile in a fine ring
/// simply lets the ring beneath answer (the Alps case of row 89, without a rule for it).
pub const RING_STEP: u32 = 4;
/// The coarsest ring: a zoom-4 pixel is 9.8 km at the equator, the last finer than the raster.
pub const RING_FLOOR_ZOOM: u32 = 4;
/// The most rings a stack can hold: from the deepest zoom a provider serves (15) down to the
/// floor in steps of [`RING_STEP`], plus one.
pub const MAX_RINGS: usize = 4;

/// The zooms of the rings beneath a finest zoom `z`, coarsest LAST (fine to coarse).
pub fn rings_beneath(z: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut r = z;
    while r >= RING_STEP + RING_FLOOR_ZOOM {
        r -= RING_STEP;
        out.push(r);
    }
    out
}

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
    /// ★ **The vertical quantum of the tile's source, measured** ([`terrace_quantum_m`]) — 0 where
    /// the tile shows no terraces (docs/46 row 96).
    pub quantum_m: f64,
    /// The decoded heights, row-major, `NAN` where void — decoded once, for the smoothing's
    /// context (a neighbour reads them each time a tile lands beside it).
    pub heights: Vec<f32>,
    /// **The surface this tile answers with**: the decoded heights where `quantum_m` is 0, else
    /// the smoothest surface within ±½ quantum of them ([`smoothest_within_the_band`]), computed
    /// with the held neighbours as context by [`TileStore::smooth_terraces`]. `NAN` where void.
    pub surface: Vec<f32>,
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

/// ★★ **THE SOURCE'S VERTICAL QUANTUM, AND THE TERRACES IT DRAWS** (docs/46 row 96). The set's
/// deep-zoom tiles are resampled from sources that stored whole metres: over Kamchatka at zoom 15
/// the decoded heights run flat at exactly 240.00 m for thirty pixels, climb one metre over eight,
/// and run flat at 241.00 m — contour terraces, one metre tall, every `1 m / slope` along the ground.
/// Shaded at the data's own step (the appearance patch's finest texel, docs/46 row 95) they are a
/// staircase of chevron ridges across ground that is a plain 3 % slope (seen 2026-11-07 from 500 m;
/// measured 2026-11-08: the pixel-to-pixel slope in the tile's own pixels is bimodal, half the
/// pixels under 0.005 — treads — and half over 0.03 — risers).
///
/// A height stored as a whole metre is a measurement with a band of ±½ m around it, not a line.
/// The surface the engine draws and stands on is the SMOOTHEST surface inside that band: each
/// pixel's height is the data's mean over the box that spans one quantum of rise at the local
/// slope (`q / s` wide — exactly one terrace period, over which a riser and its tread average to the
/// slope they were cut from), clamped back to within half a quantum of the pixel's own datum. On a
/// slope steep enough that a pixel's rise exceeds the quantum the box is a pixel wide and nothing
/// changes; a cliff is clamped back to within half a metre of the data and stays a cliff; the box
/// is bounded at [`TERRACE_REACH_PX`] (half a tile: 357 m at zoom 15, so terraces up to 714 m
/// apart — ground above 0.14 % — are smoothed; flatter ground keeps a metre step every kilometre
/// or more) — a cost bound, declared. The quantum itself is MEASURED from
/// the tile ([`terrace_quantum_m`]), never assumed: a tile stored at the encoding's 1/256 m has no
/// terraces and is left exactly as decoded.
///
/// The box reaches across tile edges into the held neighbours ([`TileStore::smooth_terraces`]),
/// so the surface is one function of position and a tile's edge is not a seam of the smoothing;
/// when a neighbour lands, the band of a held tile within reach of it is smoothed again — one band
/// a frame ([`TileStore::advance_smoothing`]), so no frame pays for all eight.
pub const TERRACE_REACH_PX: i64 = 128;
/// Fewest pixels of a flat run that count as a tread when measuring the quantum.
pub const PLATEAU_PX: usize = 4;
/// Fewest treads a tile must show before a quantum is read off them.
pub const PLATEAU_COUNT_MIN: usize = 16;

/// **The vertical quantum of a tile's source, measured from the tile**, or `None` where the tile
/// shows no terraces. The treads (flat runs of at least [`PLATEAU_PX`] pixels within 1/128 m) are
/// found along every row; their heights are tested for a lattice: the concentration
/// `|mean(exp(2πi·h/q))|` of the tread heights about a lattice of pitch `q` is near 1 when every
/// tread sits on it and near 0 when they do not. The quantum is the COARSEST pitch the treads sit on
/// (concentration ≥ ½) whose double they do not (concentration < ½) — whole metres satisfy every
/// finer lattice too, so the finest passing pitch would always be the encoding's. A tile whose
/// treads span less than four quanta of height cannot show a terrace pattern and answers `None`.
pub fn terrace_quantum_m(heights: &[Option<f64>], w: usize) -> Option<f64> {
    let mut treads: Vec<f64> = Vec::new();
    for row in heights.chunks_exact(w) {
        let mut i = 0;
        while i < w {
            let Some(h0) = row[i] else {
                i += 1;
                continue;
            };
            let mut j = i;
            while j + 1 < w && row[j + 1].is_some_and(|h| (h - h0).abs() <= 1.0 / 128.0) {
                j += 1;
            }
            if j - i + 1 >= PLATEAU_PX {
                let sum: f64 = row[i..=j].iter().map(|h| h.unwrap()).sum();
                treads.push(sum / (j - i + 1) as f64);
            }
            i = j + 1;
        }
    }
    if treads.len() < PLATEAU_COUNT_MIN {
        return None;
    }
    let (lo, hi) = treads
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &h| {
            (a.min(h), b.max(h))
        });
    let concentration = |q: f64| -> f64 {
        let (mut c, mut s) = (0.0f64, 0.0f64);
        for &h in &treads {
            let ph = std::f64::consts::TAU * h / q;
            c += ph.cos();
            s += ph.sin();
        }
        c.hypot(s) / treads.len() as f64
    };
    let pitches = [8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125];
    for pair in pitches.windows(2) {
        let (double, q) = (pair[0], pair[1]);
        if hi - lo >= 4.0 * q && concentration(q) >= 0.5 && concentration(double) < 0.5 {
            return Some(q);
        }
    }
    None
}

/// **The smoothest surface within the measurement band**, for the `rw × rh` pixels at `(x0, y0)`
/// of a `fw × fh` field of decoded heights (`None` where nothing is measured: beyond the held
/// neighbours, or void). Returns those pixels' heights, row-major; a pixel the data does not measure
/// comes back `None` and stays unmeasured.
#[allow(clippy::too_many_arguments)]
pub fn smoothest_within_the_band(
    field: &[Option<f64>],
    fw: usize,
    fh: usize,
    x0: usize,
    y0: usize,
    rw: usize,
    rh: usize,
    quantum_m: f64,
) -> Vec<Option<f64>> {
    // Summed-area tables of the heights and of the count of measured pixels, so a box mean of any
    // size is four lookups.
    let n = fw + 1;
    let mut sum = vec![0.0f64; n * (fh + 1)];
    let mut cnt = vec![0.0f64; n * (fh + 1)];
    for y in 0..fh {
        let (mut rs, mut rc) = (0.0f64, 0.0f64);
        for x in 0..fw {
            if let Some(h) = field[y * fw + x] {
                rs += h;
                rc += 1.0;
            }
            let i = (y + 1) * n + (x + 1);
            sum[i] = sum[i - n] + rs;
            cnt[i] = cnt[i - n] + rc;
        }
    }
    let box_mean = |cx: i64, cy: i64, r: i64| -> Option<f64> {
        let xa = (cx - r).max(0) as usize;
        let ya = (cy - r).max(0) as usize;
        let xb = ((cx + r + 1).min(fw as i64)) as usize;
        let yb = ((cy + r + 1).min(fh as i64)) as usize;
        if xb <= xa || yb <= ya {
            return None;
        }
        let at = |t: &[f64], x: usize, y: usize| t[y * n + x];
        let c = at(&cnt, xb, yb) - at(&cnt, xa, yb) - at(&cnt, xb, ya) + at(&cnt, xa, ya);
        if c <= 0.0 {
            return None;
        }
        let s = at(&sum, xb, yb) - at(&sum, xa, yb) - at(&sum, xb, ya) + at(&sum, xa, ya);
        Some(s / c)
    };
    // The local slope, m/px: the central difference of box means a box apart. First over a
    // 33-px box, then again over the box that slope asks for, since on ground flatter than one
    // quantum per 33 px the first estimate is a tread's (zero) and the box it asks for is the
    // bound; the second reads the slope at that scale.
    let slope_at = |cx: i64, cy: i64, b: i64| -> f64 {
        match (
            box_mean(cx - b, cy, b),
            box_mean(cx + b, cy, b),
            box_mean(cx, cy - b, b),
            box_mean(cx, cy + b, b),
        ) {
            (Some(wm), Some(em), Some(sm), Some(nm)) => {
                ((em - wm) / (2.0 * b as f64)).hypot((nm - sm) / (2.0 * b as f64))
            }
            _ => f64::INFINITY, // at the edge of the data: no smoothing
        }
    };
    let reach_for = |slope: f64| -> i64 {
        if slope > 0.0 {
            (quantum_m / (2.0 * slope))
                .min(TERRACE_REACH_PX as f64)
                .floor() as i64
        } else {
            TERRACE_REACH_PX
        }
    };
    // The reach, once per 4×4 block: the slope it comes from is a box mean's difference over 32 px
    // or more, which does not change across four pixels (measured 2026-11-08: per pixel it was
    // 13 ms a tile at the browser's opt-level, the eight box means of the slope estimate being
    // eight of the nine lookups per pixel).
    const BLOCK: usize = 4;
    let (bw, bh) = (rw.div_ceil(BLOCK), rh.div_ceil(BLOCK));
    let mut reach_of = vec![0i64; bw * bh];
    for by in 0..bh {
        for bx in 0..bw {
            let cx = (x0 + (bx * BLOCK + BLOCK / 2).min(rw - 1)) as i64;
            let cy = (y0 + (by * BLOCK + BLOCK / 2).min(rh - 1)) as i64;
            let mut reach = reach_for(slope_at(cx, cy, 16));
            if reach > 16 {
                reach = reach_for(slope_at(cx, cy, reach));
            }
            reach_of[by * bw + bx] = reach;
        }
    }
    let mut out = vec![None; rw * rh];
    for y in 0..rh {
        for x in 0..rw {
            let (cx, cy) = ((x + x0) as i64, (y + y0) as i64);
            let Some(h) = field[cy as usize * fw + cx as usize] else {
                continue;
            };
            let reach = reach_of[(y / BLOCK) * bw + x / BLOCK];
            let smoothed = if reach >= 1 {
                box_mean(cx, cy, reach).unwrap_or(h)
            } else {
                h
            };
            out[y * rw + x] = Some(smoothed.clamp(h - 0.5 * quantum_m, h + 0.5 * quantum_m));
        }
    }
    out
}

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
        let mut tile = Tile {
            id,
            px,
            chans,
            data,
            void,
            seams_repaired,
            quantum_m: 0.0,
            heights: Vec::new(),
            surface: Vec::new(),
        };
        let heights = tile.decoded();
        if complete && !void {
            tile.quantum_m = terrace_quantum_m(&heights, px as usize).unwrap_or(0.0);
        }
        tile.heights = heights
            .iter()
            .map(|h| h.map_or(f32::NAN, |h| h as f32))
            .collect();
        tile.surface = tile.heights.clone();
        if tile.quantum_m > 0.0 {
            // Smoothed by the store on the frames after it lands (`TileStore::advance_smoothing`),
            // with its neighbours as context; until then it answers as measured.
            log::info!(
                "tile {}/{}/{}: source quantum {} m, terraces to be smoothed within the band",
                id.z,
                id.x,
                id.y,
                tile.quantum_m
            );
        }
        tile
    }

    /// The decoded heights, row-major, `None` where void.
    pub fn decoded(&self) -> Vec<Option<f64>> {
        let n = (self.px as usize) * (self.px as usize);
        (0..n)
            .map(|i| {
                let o = i * self.chans;
                if o + 2 >= self.data.len() {
                    return None;
                }
                let (r, g, b) = (self.data[o], self.data[o + 1], self.data[o + 2]);
                if is_void_rgb(r, g, b) {
                    None
                } else {
                    Some(terrarium_elevation_m(r, g, b))
                }
            })
            .collect()
    }

    /// Bilinear elevation (m) at a fractional pixel, clamped at the tile's own edges. Interpolating the
    /// DECODED metres rather than the packed bytes, for the same reason `Raster::elevation_m_at` does:
    /// the packing is not linear, so lerping the bytes would blend two different powers of 256.
    ///
    /// `None` when any of the four pixels is the void sentinel: a measurement interpolated with "no
    /// measurement" is not a measurement. The provider's mosaic seam — a one-pixel line of garbage that
    /// is NOT the sentinel — is repaired at decode (`Tile::new`, `seams_repaired`), so by the time a
    /// sample is asked for here it is gone.
    /// One pixel of the SURFACE, `None` where the measurement is void or the surface has none.
    pub fn surface_px(&self, ix: usize, iy: usize) -> Option<f64> {
        let i = iy * self.px as usize + ix;
        let o = i * self.chans;
        if is_void_rgb(self.data[o], self.data[o + 1], self.data[o + 2]) {
            return None;
        }
        let h = self.surface[i];
        (!h.is_nan()).then_some(h as f64)
    }

    fn elevation_at_px(&self, px: f64, py: f64) -> Option<f64> {
        self.bilinear(px, py, |i| {
            let h = self.surface[i];
            (!h.is_nan()).then_some(h as f64)
        })
    }

    /// [`elevation_at_px`](Self::elevation_at_px) on the decoded BYTES — the measurement itself,
    /// before the terraces were smoothed (docs/46 row 96).
    pub fn measured_at_px(&self, px: f64, py: f64) -> Option<f64> {
        self.bilinear(px, py, |i| {
            let o = i * self.chans;
            Some(terrarium_elevation_m(
                self.data[o],
                self.data[o + 1],
                self.data[o + 2],
            ))
        })
    }

    fn bilinear(&self, px: f64, py: f64, value: impl Fn(usize) -> Option<f64>) -> Option<f64> {
        let n = self.px as i64;
        // The SURFACE, not the bytes: the bytes are the measurement, the surface is the smoothest
        // ground within its band (docs/46 row 96) — identical where the source has no quantum.
        let at = |ix: i64, iy: i64| -> Option<f64> {
            let ix = ix.clamp(0, n - 1) as usize;
            let iy = iy.clamp(0, n - 1) as usize;
            let i = iy * self.px as usize + ix;
            let o = i * self.chans;
            if is_void_rgb(self.data[o], self.data[o + 1], self.data[o + 2]) {
                None
            } else {
                value(i)
            }
        };
        let (x0, y0) = (px.floor() as i64, py.floor() as i64);
        let (tx, ty) = (px - x0 as f64, py - y0 as f64);
        let top = at(x0, y0)? * (1.0 - tx) + at(x0 + 1, y0)? * tx;
        let bot = at(x0, y0 + 1)? * (1.0 - tx) + at(x0 + 1, y0 + 1)? * tx;
        Some(top * (1.0 - ty) + bot * ty)
    }
}

impl TileId {
    /// The ground under the `w × h` pixels at `(x, y)` of this tile of `px` pixels a side,
    /// `(lat_s, lat_n, lon_w, lon_e)` degrees.
    pub fn bounds_deg_of_px(
        &self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        px: u32,
    ) -> (f64, f64, f64, f64) {
        let n = (1u64 << self.z) as f64;
        let p = px.max(1) as f64;
        let lon = |fx: f64| fx / n * 360.0 - 180.0;
        let lat = |fy: f64| {
            (std::f64::consts::PI * (1.0 - 2.0 * fy / n))
                .sinh()
                .atan()
                .to_degrees()
        };
        let (x0, x1) = (
            self.x as f64 + x as f64 / p,
            self.x as f64 + (x + w) as f64 / p,
        );
        let (y0, y1) = (
            self.y as f64 + y as f64 / p,
            self.y as f64 + (y + h) as f64 / p,
        );
        (lat(y1), lat(y0), lon(x0), lon(x1))
    }

    /// This tile's ground, `(lat_s, lat_n, lon_w, lon_e)` degrees — the inverse of [`tile_coords`]
    /// at the tile's corners.
    pub fn bounds_deg(&self) -> (f64, f64, f64, f64) {
        let n = (1u64 << self.z) as f64;
        let lon = |x: f64| x / n * 360.0 - 180.0;
        let lat = |y: f64| {
            (std::f64::consts::PI * (1.0 - 2.0 * y / n))
                .sinh()
                .atan()
                .to_degrees()
        };
        (
            lat(self.y as f64 + 1.0),
            lat(self.y as f64),
            lon(self.x as f64),
            lon(self.x as f64 + 1.0),
        )
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
    /// **Counts every change to what this store holds or is asked for** — the key a cache built
    /// from the tiles (the appearance patch, docs/73) compares against, so that a tile arriving
    /// after the camera stopped moving still reaches the picture (the lesson of `flora::Built`,
    /// docs/46 row 52: a cache keyed on anything but its inputs lies).
    generation: u64,
    /// Tiles whose surface was smoothed again because a neighbour landed — for a consumer that
    /// cached their old surface (the appearance patch's rects) to redo them. Drained by
    /// [`take_resmoothed`](Self::take_resmoothed).
    resmoothed: Vec<(TileId, usize, usize, usize, usize)>,
    /// Bands of held tiles waiting to be smoothed again with a newly landed neighbour as context:
    /// `(tile, x, y, w, h)` in the tile's pixels. One a frame ([`advance_smoothing`](Self::advance_smoothing)).
    pending_smoothing: Vec<(TileId, usize, usize, usize, usize)>,
}

impl TileStore {
    pub fn len(&self) -> usize {
        self.tiles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The generation: changes whenever a tile lands, is dropped, or the patch asked for moves.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The patch last asked for: `(lat, lon, zoom, radius)`.
    pub fn want(&self) -> Option<(f64, f64, u32, i32)> {
        self.want
    }

    /// The ground the asked-for patch covers, `(lat_s, lat_n, lon_w, lon_e)` degrees — the union of
    /// its tiles' bounds, with the longitudes unwrapped about the patch's centre so a patch over the
    /// dateline is one span (`lon_e` may exceed 180).
    pub fn patch_bounds_deg(&self) -> Option<(f64, f64, f64, f64)> {
        let (lat, lon, z, r) = self.want?;
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for id in tiles_around(lat, lon, z, r) {
            let (s, n, mut w, mut e) = id.bounds_deg();
            // Unwrap about the centre: a tile's west edge more than half a turn from the centre is
            // the same tile a turn over.
            while w - lon > 180.0 {
                w -= 360.0;
                e -= 360.0;
            }
            while w - lon < -180.0 {
                w += 360.0;
                e += 360.0;
            }
            b = Some(match b {
                None => (s, n, w, e),
                Some((bs, bn, bw, be)) => (bs.min(s), bn.max(n), bw.min(w), be.max(e)),
            });
        }
        b
    }

    /// Declare the patch the observer needs. Tiles outside it are dropped, which is what bounds the store
    /// without an eviction policy needing to be invented: the camera moving IS the eviction.
    pub fn want_patch(&mut self, lat: f64, lon: f64, z: u32, radius: i32) {
        let before = self.tiles.len();
        self.want = Some((lat, lon, z, radius));
        // The generation follows the TILES: a centre moving within its tile changes nothing held,
        // and a consumer that cares which tiles the patch names reads them through `want()`.
        let keep: std::collections::HashSet<TileId> = self
            .ring_zooms()
            .into_iter()
            .flat_map(|rz| tiles_around(lat, lon, rz, radius))
            .collect();
        self.tiles.retain(|id, _| keep.contains(id));
        if self.tiles.len() != before {
            self.generation += 1;
        }
    }

    /// The zooms held, finest first: the patch asked for and the rings beneath it.
    pub fn ring_zooms(&self) -> Vec<u32> {
        match self.want {
            None => Vec::new(),
            Some((_, _, z, _)) => {
                let mut v = vec![z];
                v.extend(rings_beneath(z));
                v
            }
        }
    }

    /// The tiles held that measure ground (not void), the finest ring first and nearest the
    /// patch's centre first within a ring — the order the appearance patch integrates them in
    /// (docs/46 row 95). Every ring: the ground a coarser ring alone covers is drawn from it, so
    /// its appearance is integrated over it too (docs/46 row 97).
    pub fn held_nearest_first(&self) -> Vec<TileId> {
        let Some((lat, lon, _, r)) = self.want else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for z in self.ring_zooms() {
            out.extend(
                tiles_around(lat, lon, z, r)
                    .into_iter()
                    .filter(|id| self.tiles.get(id).is_some_and(|t| !t.void)),
            );
        }
        out
    }

    /// The ring one step finer than zoom `z`, if there is one.
    pub fn ring_above(&self, z: u32) -> Option<u32> {
        let zooms = self.ring_zooms();
        let i = zooms.iter().position(|&rz| rz == z)?;
        (i > 0).then(|| zooms[i - 1])
    }

    /// The ground a ring covers in full — `(lat_s, lat_n, lon_w, lon_e)` of its tiles' union —
    /// when EVERY tile of the ring is held and measured, so the ring beneath need not be
    /// integrated there; `None` while any is missing. (A union across the antimeridian is not
    /// one rectangle in longitude; such a ring reports `None` and the ring beneath builds under
    /// it, which is correct and merely more work.)
    pub fn ring_union_held(&self, z: u32) -> Option<(f64, f64, f64, f64)> {
        let (lat, lon, _, r) = self.want?;
        let ids = tiles_around(lat, lon, z, r);
        if ids.is_empty()
            || !ids
                .iter()
                .all(|id| self.tiles.get(id).is_some_and(|t| !t.void))
        {
            return None;
        }
        let mut u = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for id in &ids {
            let (s, n, w, e) = id.bounds_deg();
            if w > e {
                return None;
            }
            u = (u.0.min(s), u.1.max(n), u.2.min(w), u.3.max(e));
        }
        Some(u)
    }

    /// The tiles the current patch needs that are not held yet: the coarsest ring first (it
    /// covers the most ground per tile, and is what the finer rings fade into), nearest first
    /// within a ring.
    pub fn missing(&self) -> Vec<TileId> {
        let Some((lat, lon, _, r)) = self.want else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for rz in self.ring_zooms().into_iter().rev() {
            out.extend(
                tiles_around(lat, lon, rz, r)
                    .into_iter()
                    .filter(|id| !self.tiles.contains_key(id)),
            );
        }
        out
    }

    /// Returns whether the tile's own smoothing was queued (its surface is its raw heights until
    /// the next frame's [`advance_smoothing`](Self::advance_smoothing) reaches it).
    pub fn insert(&mut self, tile: Tile) -> bool {
        // Only keep what the current patch wants — a tile that arrives after the camera moved is stale.
        if let Some((lat, lon, _, r)) = self.want {
            if !self.ring_zooms().contains(&tile.id.z)
                || !tiles_around(lat, lon, tile.id.z, r).contains(&tile.id)
            {
                return false;
            }
        }
        if tile.void {
            // Remembered past eviction: this is where the DATA stops, and the zoom must not ask past
            // it again next frame (`deepest_measured_zoom`).
            self.known_void.insert(tile.id);
        }
        let id = tile.id;
        let measured = !tile.void;
        let terraced = measured && tile.quantum_m > 0.0;
        self.tiles.insert(id, tile);
        self.generation += 1;
        if measured {
            // Each held neighbour's band within reach of the new tile, queued; then the new tile
            // itself, queued LAST so it is smoothed first (the queue is a stack): a frame's
            // budget of smoothing, not a landing's worth in one frame.
            let n = 1i64 << id.z;
            let w = self.tiles[&id].px as usize;
            let pad = (TERRACE_REACH_PX as usize).min(w);
            for dy in -1..=1i64 {
                for dx in -1..=1i64 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let y = id.y as i64 + dy;
                    if y < 0 || y >= n {
                        continue;
                    }
                    let nid = TileId {
                        z: id.z,
                        x: (id.x as i64 + dx).rem_euclid(n) as u32,
                        y: y as u32,
                    };
                    let Some(nt) = self.tiles.get(&nid) else {
                        continue;
                    };
                    if nt.void || nt.quantum_m <= 0.0 || nt.px as usize != w {
                        continue;
                    }
                    // The neighbour's band facing the new tile: the new tile is at (dx, dy) from
                    // it, so the band is its east side when dx = +1, its west when dx = −1, …
                    let (bx, bw) = match dx {
                        1 => (w - pad, pad),
                        -1 => (0, pad),
                        _ => (0, w),
                    };
                    let (by, bh) = match dy {
                        1 => (w - pad, pad),
                        -1 => (0, pad),
                        _ => (0, w),
                    };
                    let band = (nid, bx, by, bw, bh);
                    if !self.pending_smoothing.contains(&band) {
                        self.pending_smoothing.push(band);
                    }
                }
            }
            if terraced {
                self.pending_smoothing.push((id, 0, 0, w, w));
            }
        }
        terraced
    }

    /// **Smooth queued bands until `budget_s` has passed** (at least one if any waits): returns
    /// `(did_work, bands_still_queued)`. A frame calls this once with its share, so a tile landing
    /// costs no frame more than that share.
    pub fn advance_smoothing(&mut self, budget_s: f64) -> (bool, usize) {
        let t0 = crate::clock::now_seconds();
        let mut did = false;
        while let Some((id, x, y, w, h)) = self.pending_smoothing.pop() {
            if self.smooth_terraces_rect(id, x, y, w, h) {
                self.resmoothed.push((id, x, y, w, h));
                did = true;
            }
            if crate::clock::now_seconds() - t0 >= budget_s {
                break;
            }
        }
        (did, self.pending_smoothing.len())
    }

    /// Smooth every queued band now — what a rig calls so it reads the finished surface.
    pub fn finish_smoothing(&mut self) {
        while self.advance_smoothing(f64::INFINITY).1 > 0 {}
    }

    /// The bands whose surface moved since the last call — `(tile, x, y, w, h)` in the tile's
    /// pixels — so the appearance built over exactly that ground, and no more, is built again.
    pub fn take_resmoothed(&mut self) -> Vec<(TileId, usize, usize, usize, usize)> {
        std::mem::take(&mut self.resmoothed)
    }

    /// **Smooth a held tile's terraces with its held neighbours as context** (docs/46 row 96):
    /// the box that spans one quantum of rise may reach [`TERRACE_REACH_PX`] past the tile's edge,
    /// and with the neighbours in the field the surface is continuous across it. Returns whether
    /// the tile has a quantum and was recomputed.
    pub fn smooth_terraces(&mut self, id: TileId) -> bool {
        let Some(w) = self.tiles.get(&id).map(|t| t.px as usize) else {
            return false;
        };
        self.smooth_terraces_rect(id, 0, 0, w, w)
    }

    /// [`smooth_terraces`](Self::smooth_terraces) for the `rw × rh` pixels at `(rx, ry)` of the
    /// tile, with the field around them reaching [`TERRACE_REACH_PX`] into the held neighbours.
    pub fn smooth_terraces_rect(
        &mut self,
        id: TileId,
        rx: usize,
        ry: usize,
        rw: usize,
        rh: usize,
    ) -> bool {
        let Some(t) = self.tiles.get(&id) else {
            return false;
        };
        if t.void || t.quantum_m <= 0.0 {
            return false;
        }
        let (w, q) = (t.px as usize, t.quantum_m);
        let pad = TERRACE_REACH_PX as usize;
        // The field, in the tile's pixel coordinates: from (rx − pad, ry − pad), fw × fh.
        let (fx0, fy0) = (rx as i64 - pad as i64, ry as i64 - pad as i64);
        let (fw, fh) = (rw + 2 * pad, rh + 2 * pad);
        let mut field: Vec<Option<f64>> = vec![None; fw * fh];
        let n = 1i64 << id.z;
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let y = id.y as i64 + dy;
                if y < 0 || y >= n {
                    continue;
                }
                let nid = TileId {
                    z: id.z,
                    x: (id.x as i64 + dx).rem_euclid(n) as u32,
                    y: y as u32,
                };
                let Some(nt) = self.tiles.get(&nid) else {
                    continue;
                };
                if nt.void || nt.px as usize != w {
                    continue;
                }
                // The neighbour's pixel (tx, ty) sits at (tx + dx·w, ty + dy·w) in the tile's
                // coordinates; copy the part that falls inside the field.
                let (ox, oy) = (dx * w as i64 - fx0, dy * w as i64 - fy0);
                let tx_lo = (-ox).clamp(0, w as i64) as usize;
                let tx_hi = (fw as i64 - ox).clamp(0, w as i64) as usize;
                let ty_lo = (-oy).clamp(0, w as i64) as usize;
                let ty_hi = (fh as i64 - oy).clamp(0, w as i64) as usize;
                for ty in ty_lo..ty_hi {
                    let fy = (ty as i64 + oy) as usize;
                    let row = &nt.heights[ty * w..(ty + 1) * w];
                    for tx in tx_lo..tx_hi {
                        let v = row[tx];
                        if !v.is_nan() {
                            field[fy * fw + (tx as i64 + ox) as usize] = Some(v as f64);
                        }
                    }
                }
            }
        }
        let smooth = smoothest_within_the_band(&field, fw, fh, pad, pad, rw, rh, q);
        // Report a change only where the surface MOVED. A tile whose ground is steeper than one
        // quantum per pixel everywhere (a coarse ring over hills) has terraces the box cannot
        // span and comes back identical; saying it changed sent the appearance over it to be
        // built again, and at a 9 km pixel "over it" is the whole window (2026-11-08).
        let t = self.tiles.get_mut(&id).expect("held");
        let mut changed = false;
        for y in 0..rh {
            for x in 0..rw {
                let v = smooth[y * rw + x].map_or(f32::NAN, |h| h as f32);
                let slot = &mut t.surface[(ry + y) * w + rx + x];
                if v.to_bits() != slot.to_bits() && !(v.is_nan() && slot.is_nan()) {
                    *slot = v;
                    changed = true;
                }
            }
        }
        changed
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

    /// **The finest datum covering a point, and how much to trust it** — `(metres, weight)` from
    /// the finest ring that measures the point, weight in `[0,1]`. `None` when no held tile covers
    /// it. For the surface itself use [`blend`](Self::blend), which carries the weight down the
    /// whole stack; this answers "what does the finest tile say here" for diagnostics and tests.
    pub fn elevation_m_at(&self, lat: f64, lon: f64) -> Option<(f64, f64)> {
        self.ring_zooms()
            .into_iter()
            .find_map(|z| self.elevation_at_zoom(z, lat, lon))
    }

    /// ★★ **The measured surface at a point, down the whole stack** (docs/46 row 97):
    /// `(height_m, trust, base_feature_m)`. Each ring, finest first, answers `(e, w)`; the height is
    /// `beneath + w·(e − beneath)` where `beneath` is the same question asked of the rings below it,
    /// and at the bottom the raster. `trust` is how far the answer is measured ground rather than
    /// the raster (1 − Π(1 − wᵢ)); `base_feature_m` is the datum's feature size blended the same
    /// way — the raster's step where nothing covers the point, a ring's pixel where it is trusted.
    /// A ring that is fully trusted here (w = 1) ends the walk: the rings beneath cannot change
    /// the answer, so inside a patch this costs one lookup.
    pub fn blend(&self, lat: f64, lon: f64, raster_m: f64, raster_step_m: f64) -> (f64, f64, f64) {
        let zooms = self.ring_zooms();
        self.blend_from(&zooms, lat, lon, raster_m, raster_step_m)
    }

    /// [`blend`](Self::blend) with the finest ring left out — what the finest ring fades INTO.
    pub fn blend_beneath_finest(
        &self,
        lat: f64,
        lon: f64,
        raster_m: f64,
        raster_step_m: f64,
    ) -> (f64, f64, f64) {
        let zooms = self.ring_zooms();
        self.blend_from(
            zooms.get(1..).unwrap_or(&[]),
            lat,
            lon,
            raster_m,
            raster_step_m,
        )
    }

    fn blend_from(
        &self,
        zooms: &[u32],
        lat: f64,
        lon: f64,
        raster_m: f64,
        raster_step_m: f64,
    ) -> (f64, f64, f64) {
        let Some((&z, rest)) = zooms.split_first() else {
            return (raster_m, 0.0, raster_step_m);
        };
        match self.elevation_at_zoom(z, lat, lon) {
            Some((e, w)) if w >= 1.0 => (e, 1.0, self.pixel_at_zoom(z, lat)),
            Some((e, w)) if w > 0.0 => {
                let (below, trust, base) = self.blend_from(rest, lat, lon, raster_m, raster_step_m);
                let px = self.pixel_at_zoom(z, lat);
                (
                    below + w * (e - below),
                    1.0 - (1.0 - w) * (1.0 - trust),
                    base * (px / base).powf(w),
                )
            }
            _ => self.blend_from(rest, lat, lon, raster_m, raster_step_m),
        }
    }

    /// **The measured datums under a point, finest first**: `(pixel_m, weight)` for every ring
    /// that covers it with any trust, stopping at a ring trusted in full (nothing beneath it can
    /// change the answer). The relief law's input (docs/46 rows 89, 97): each rung of generated
    /// relief is produced only as far as NO datum resolving it is believed, `Π(1 − wᵢ)`, so a
    /// ring's edge, where its weight falls to zero, hands each rung to the ring beneath without a
    /// step. A single sliding "feature size" cannot say this — the law's rungs are resolved or
    /// not by a threshold, and a threshold that moves with the fade is the step the Alps showed
    /// (slope 8.6 at a ring's edge, 2026-11-08).
    ///
    /// Returned in a fixed array with its count — this is asked per sample of the appearance
    /// integral, and a heap allocation there is a cost on every texel.
    pub fn datums(&self, lat: f64, lon: f64) -> ([(f64, f64); MAX_RINGS], usize) {
        let mut out = [(0.0, 0.0); MAX_RINGS];
        let mut n = 0usize;
        for z in self.ring_zooms() {
            if n == MAX_RINGS {
                break;
            }
            if let Some((_, w)) = self.elevation_at_zoom(z, lat, lon) {
                if w > 0.0 {
                    out[n] = (self.pixel_at_zoom(z, lat), w);
                    n += 1;
                    if w >= 1.0 {
                        break;
                    }
                }
            }
        }
        (out, n)
    }

    /// **Bilinear in a ring's surface ACROSS tile edges.** A tap that falls in a held, measured
    /// neighbour reads the neighbour's pixel; one that falls where nothing is held clamps to this
    /// tile's edge, which is the side the fade covers. Clamping on both sides of a seam between
    /// two HELD tiles left a half-pixel band on each side at its own tile's edge value — a step of
    /// one pixel's rise at every inner seam, invisible at a 5 m pixel and 20 m tall at the 9 km
    /// pixel of the coarsest ring (terra's start, 50 km, slope 5.6 on a 3.7 m edge; 2026-11-08).
    fn bilinear_at_zoom(
        &self,
        z: u32,
        tx: i64,
        ty: i64,
        home: &Tile,
        sx: f64,
        sy: f64,
    ) -> Option<f64> {
        let n = 1i64 << z;
        let w = home.px as i64;
        let clamped = |ix: i64, iy: i64| -> Option<f64> {
            home.surface_px(ix.clamp(0, w - 1) as usize, iy.clamp(0, w - 1) as usize)
        };
        let tap = |ix: i64, iy: i64| -> Option<f64> {
            let (dx, dy) = (ix.div_euclid(w), iy.div_euclid(w));
            if dx == 0 && dy == 0 {
                return home.surface_px(ix as usize, iy as usize);
            }
            let y = ty + dy;
            if y < 0 || y >= n {
                return clamped(ix, iy);
            }
            match self.tiles.get(&TileId {
                z,
                x: (tx + dx).rem_euclid(n) as u32,
                y: y as u32,
            }) {
                Some(t) if !t.void && t.px == home.px => t
                    .surface_px(ix.rem_euclid(w) as usize, iy.rem_euclid(w) as usize)
                    .or_else(|| clamped(ix, iy)),
                _ => clamped(ix, iy),
            }
        };
        let (x0, y0) = (sx.floor() as i64, sy.floor() as i64);
        let (u, v) = (sx - x0 as f64, sy - y0 as f64);
        let top = tap(x0, y0)? * (1.0 - u) + tap(x0 + 1, y0)? * u;
        let bot = tap(x0, y0 + 1)? * (1.0 - u) + tap(x0 + 1, y0 + 1)? * u;
        Some(top * (1.0 - v) + bot * v)
    }

    /// The measured vertical quantum of the zoom-`z` tile under a point (docs/46 row 96).
    pub fn quantum_at_zoom(&self, z: u32, lat: f64, lon: f64) -> Option<f64> {
        let n = 1i64 << z;
        let (fx, fy) = tile_coords(lat, lon, z);
        let id = TileId {
            z,
            x: (fx.floor() as i64).rem_euclid(n) as u32,
            y: fy.floor() as u32,
        };
        self.tiles.get(&id).map(|t| t.quantum_m)
    }

    /// The zoom-`z` tile's elevation under a point AS MEASURED — bilinear in the decoded bytes,
    /// before any smoothing — for the gate that holds the surface within the band.
    pub fn measured_at_zoom(&self, z: u32, lat: f64, lon: f64) -> Option<f64> {
        let n = 1i64 << z;
        let (fx, fy) = tile_coords(lat, lon, z);
        let tile = self.tiles.get(&TileId {
            z,
            x: (fx.floor() as i64).rem_euclid(n) as u32,
            y: fy.floor() as u32,
        })?;
        if tile.void {
            return None;
        }
        let px = tile.px as f64;
        tile.measured_at_px(fx.fract() * px - 0.5, fy.fract() * px - 0.5)
    }

    /// A held tile, by id.
    pub fn get(&self, id: &TileId) -> Option<&Tile> {
        self.tiles.get(id)
    }

    /// The ground size of a pixel of a ring's tiles at this latitude.
    fn pixel_at_zoom(&self, z: u32, lat: f64) -> f64 {
        let px = self
            .tiles
            .values()
            .find(|t| t.id.z == z)
            .map_or(256, |t| t.px);
        pixel_ground_m(z, lat, px)
    }

    /// **Measured elevation at one ring, and how much to trust it** — `(metres, weight)`, weight in
    /// `[0,1]`. `None` when no held tile of zoom `z` covers the point. The WEIGHT is what keeps the
    /// ring from having a visible edge: it falls to zero across the outermost tile, and the stack
    /// blends `beneath + weight·(tile − beneath)`. That is not a fudge and not a new idea — it is
    /// the same "detail fades in rather than popping" rule the octave count and the cap cross-fade
    /// already use, and at weight 0 the surface is exactly the ring beneath, so the two never
    /// disagree at the seam.
    pub fn elevation_at_zoom(&self, z: u32, lat: f64, lon: f64) -> Option<(f64, f64)> {
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
        let e = self.bilinear_at_zoom(
            z,
            tx,
            ty,
            tile,
            fx.fract() * px - 0.5,
            fy.fract() * px - 0.5,
        )?;

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
        // trusted throughout; the outermost held tiles fade to the ring beneath at their outer edges; a
        // tile that arrived alone is a smooth bump of measured detail that widens as its neighbours land.
        // Every seam is continuous by construction, in every state the stream passes through — the
        // same "detail fades in rather than popping" rule, now applied to what is actually there.
        // "Held" means held AND measured: a void tile is kept so the host is not asked for it again,
        // but it is unmeasured ground, and the fade treats it exactly like a tile that never came.
        //
        // ★ **A SMOOTH MINIMUM OF THE DISTANCES, NOT THE NEAREST ONE** (docs/46 row 97). The
        // weight was `smoothstep(min(distances))`: continuous, but its gradient turns through 90°
        // where the nearest unmeasured edge changes — along a corner tile's diagonal — and the
        // skirt that gradient shapes had a crease there (the drawn slope jumped by 0.49 across it
        // against 0.014 between neighbouring texels elsewhere, Kamchatka 7 km, 2026-11-08). The
        // distance is now the order-8 harmonic mean of the distances, `(Σ dᵢ⁻⁸)^(−1/8)`: equal to
        // the nearest where one is nearest, within 8 % of it where two tie, and with a continuous
        // gradient everywhere — the fade still reaches 1 a tile's width from unmeasured ground and
        // 0 at its edge.
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
        // Distance, in tiles, to each piece of ground that is NOT measured: a missing side neighbour
        // is a half-plane (distance to the edge), a missing corner neighbour a quadrant (distance to
        // the corner). 1.0 and more means nothing unmeasured within a tile's width.
        let mut inv8 = 0.0f64;
        let mut consider = |d: f64| {
            let d = d.clamp(0.0, 1.0);
            if d <= 0.0 {
                inv8 = f64::INFINITY;
            } else if d < 1.0 {
                inv8 += d.powi(-8);
            }
        };
        if !held(-1, 0) {
            consider(ux);
        }
        if !held(1, 0) {
            consider(1.0 - ux);
        }
        if !held(0, -1) {
            consider(uy);
        }
        if !held(0, 1) {
            consider(1.0 - uy);
        }
        if !held(-1, -1) {
            consider(ux.hypot(uy));
        }
        if !held(1, -1) {
            consider((1.0 - ux).hypot(uy));
        }
        if !held(-1, 1) {
            consider(ux.hypot(1.0 - uy));
        }
        if !held(1, 1) {
            consider((1.0 - ux).hypot(1.0 - uy));
        }
        let reach = if inv8 == 0.0 {
            1.0
        } else {
            inv8.powf(-1.0 / 8.0)
        };
        let t = reach.clamp(0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t); // smoothstep, so the seam has no visible crease
        Some((e, w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two held tiles of one ring, flat at different heights: walking across the seam between
    /// them, the surface ramps over one pixel and never steps (the inner-seam clamp, 2026-11-08).
    #[test]
    fn the_surface_ramps_across_an_inner_seam_and_does_not_step() {
        let z = 4u32;
        let px = 8u32;
        let n = (px * px) as usize;
        let flat = |m: f64| -> Vec<u8> {
            let v = ((m + 32768.0) * 256.0).round() as u32;
            let (r, g, b) = ((v >> 16) as u8, (v >> 8) as u8, v as u8);
            (0..n).flat_map(|_| [r, g, b]).collect()
        };
        let mut store = TileStore::default();
        let (lat, lon) = (20.0f64, 0.0f64);
        let (fx, fy) = tile_coords(lat, lon, z);
        let (cx, cy) = (fx.floor() as u32, fy.floor() as u32);
        store.want_patch(lat, lon, z, 1);
        let a = TileId { z, x: cx, y: cy };
        let b = TileId {
            z,
            x: cx + 1,
            y: cy,
        };
        store.insert(Tile::new(a, px, 3, flat(100.0)));
        store.insert(Tile::new(b, px, 3, flat(120.0)));
        store.finish_smoothing();
        // Walk east across the seam at the tiles' middle row, 1/200 px a step.
        let n_tiles = (1u32 << z) as f64;
        let lat_mid = {
            let y = (cy as f64 + 0.5) / n_tiles;
            (std::f64::consts::PI * (1.0 - 2.0 * y))
                .sinh()
                .atan()
                .to_degrees()
        };
        let lon_at = |tile_x: f64| tile_x / n_tiles * 360.0 - 180.0;
        let mut prev = None;
        let mut max_step = 0.0f64;
        let mut k = -100i32;
        while k <= 100 {
            let x = (cx + 1) as f64 + k as f64 / (200.0 * px as f64);
            let (h, _) = store
                .elevation_at_zoom(z, lat_mid, lon_at(x))
                .expect("covered");
            if let Some(p) = prev {
                max_step = max_step.max((h - p as f64).abs());
            }
            prev = Some(h);
            k += 1;
        }
        // One pixel's rise, 20 m, spread over one pixel: a 1/200 px step moves at most 0.1 m.
        assert!(
            max_step <= 0.1 * 1.01,
            "the surface stepped {max_step:.3} m across the seam"
        );
    }

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
            store.missing().iter().all(|t| t.z != z),
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
        assert_eq!(store.missing().iter().filter(|t| t.z == z).count(), 9);
        for id in tiles_around(lat, lon, z, r) {
            store.insert(flat_tile(id, 2500.0));
        }
        assert!(
            store.missing().iter().all(|t| t.z != z),
            "the patch is complete"
        );
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
        assert!(store.missing().iter().any(|t| t.z == 11));
        assert!(!store.missing().iter().any(|t| t.z == 10));
    }

    /// A plane of slope `s` m/px stored in whole metres and resampled by the provider: treads at
    /// each whole metre, risers `ramp` px wide between them — what the set's deep zooms hold.
    fn terraced_plane(px: usize, s: f64, ramp: usize, x0: f64) -> Vec<Option<f64>> {
        let mut out = Vec::with_capacity(px * px);
        for _y in 0..px {
            for x in 0..px {
                let h = 300.0 + s * (x as f64 + x0);
                let lo = h.floor();
                // Within `ramp` px of the next whole metre the provider's kernel climbs to it.
                let to_next = (lo + 1.0 - h) / s; // px until the plane reaches the next metre
                let v = if to_next < ramp as f64 {
                    lo + 1.0 - to_next / ramp as f64
                } else {
                    lo
                };
                out.push(Some(v));
            }
        }
        out
    }

    fn tile_from_heights(id: TileId, px: usize, h: &[Option<f64>]) -> Tile {
        let mut data = Vec::with_capacity(px * px * 3);
        for v in h {
            let (r, g, b) = match v {
                Some(m) => terrarium_bytes(*m),
                None => (0, 0, 0),
            };
            data.extend_from_slice(&[r, g, b]);
        }
        Tile::new(id, px as u32, 3, data)
    }

    /// ★ **The quantum is read off the tile, and a tile without terraces has none** (docs/46 row
    /// 96). A whole-metre plane answers 1 m; the same plane at the encoding's step, and a flat
    /// tile, answer nothing.
    #[test]
    fn the_quantum_is_measured_from_the_treads() {
        let px = 64;
        let terraced = terraced_plane(px, 0.08, 6, 0.0);
        assert_eq!(terrace_quantum_m(&terraced, px), Some(1.0));
        let smooth: Vec<Option<f64>> = (0..px * px)
            .map(|i| Some(300.0 + 0.03 * (i % px) as f64 + 0.011 * (i / px) as f64))
            .collect();
        assert_eq!(
            terrace_quantum_m(&smooth, px),
            None,
            "no treads, no quantum"
        );
        let flat: Vec<Option<f64>> = vec![Some(12.0); px * px];
        assert_eq!(
            terrace_quantum_m(&flat, px),
            None,
            "one tread is not a lattice"
        );
        // Half-metre data answers a half metre, not the metre its even treads also sit on.
        let half: Vec<Option<f64>> = terraced_plane(px, 0.08, 6, 0.0)
            .iter()
            .map(|h| h.map(|v| 300.0 + (v - 300.0) * 0.5))
            .collect();
        assert_eq!(terrace_quantum_m(&half, px), Some(0.5));
    }

    /// ★★ **The terraces smooth into the plane they were cut from, and nothing moves further than
    /// half a quantum** (docs/46 row 96). The risers were slopes of 1/6 and the treads of 0; the
    /// surface's pixel-to-pixel slope is the plane's, and every pixel sits within ±½ m of its datum.
    #[test]
    fn the_terraces_smooth_into_the_plane_within_half_a_quantum() {
        let px = 128;
        let s = 0.05;
        let data = terraced_plane(px, s, 6, 0.0);
        let id = TileId {
            z: 15,
            x: 100,
            y: 100,
        };
        let mut store = TileStore::default();
        let (lat_s, lat_n, lon_w, lon_e) = id.bounds_deg();
        store.want_patch(0.5 * (lat_s + lat_n), 0.5 * (lon_w + lon_e), 15, 1);
        store.insert(tile_from_heights(id, px, &data));
        store.finish_smoothing();
        let t = &store.tiles[&id];
        assert_eq!(t.quantum_m, 1.0);
        let mut worst_from_datum = 0.0f64;
        let mut worst_from_plane = 0.0f64;
        let mut slopes: Vec<f64> = Vec::new();
        for y in 0..px {
            for x in 0..px {
                let v = t.surface[y * px + x] as f64;
                worst_from_datum = worst_from_datum.max((v - data[y * px + x].unwrap()).abs());
                let plane = 300.0 + s * x as f64;
                worst_from_plane = worst_from_plane.max((v - plane).abs());
                if x > 0 && (40..px - 40).contains(&x) {
                    slopes.push(v - t.surface[y * px + x - 1] as f64);
                }
            }
        }
        let mean = slopes.iter().sum::<f64>() / slopes.len() as f64;
        let rms_dev =
            (slopes.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / slopes.len() as f64).sqrt();
        println!(
            "terraces: worst from datum {worst_from_datum:.3} m, from the plane {worst_from_plane:.3} m; \
             slope mean {mean:.4} (plane {s}), rms deviation {rms_dev:.4} (risers were {:.3})",
            1.0 / 6.0
        );
        assert!(
            worst_from_datum <= 0.5 + 1e-3,
            "moved {worst_from_datum} m, more than half a quantum"
        );
        assert!(
            (mean - s).abs() < 0.2 * s,
            "the surface's slope {mean} is not the plane's {s}"
        );
        assert!(
            rms_dev < 0.25 * (1.0 / 6.0),
            "the risers are still there: rms deviation {rms_dev}"
        );
        // The surface stays inside the measurement's band about the plane: the fixture's risers
        // climb early, so its data sit up to a quantum above the plane, and so may the surface.
        assert!(
            worst_from_plane <= 1.0 + 1e-3,
            "{worst_from_plane} m from the plane"
        );
    }

    /// A cliff is ground: clamped back to within half a quantum of its data, it stays a cliff.
    #[test]
    fn a_cliff_is_not_a_terrace() {
        let px = 64;
        // A terraced plane so the tile HAS a quantum, with a 50 m cliff across column 32.
        let data: Vec<Option<f64>> = terraced_plane(px, 0.08, 6, 0.0)
            .iter()
            .enumerate()
            .map(|(i, h)| h.map(|v| if i % px >= 32 { v - 50.0 } else { v }))
            .collect();
        let id = TileId {
            z: 15,
            x: 1000,
            y: 1000,
        };
        let mut store = TileStore::default();
        let (lat_s, lat_n, lon_w, lon_e) = id.bounds_deg();
        store.want_patch(0.5 * (lat_s + lat_n), 0.5 * (lon_w + lon_e), 15, 1);
        store.insert(tile_from_heights(id, px, &data));
        store.finish_smoothing();
        let t = &store.tiles[&id];
        assert_eq!(t.quantum_m, 1.0);
        let step = t.surface[10 * px + 31] - t.surface[10 * px + 32];
        assert!(step > 49.0, "the cliff was smoothed to a {step} m step");
    }

    /// ★ **A tile is smoothed again when its neighbour lands, and the surface crosses the edge
    /// without a seam.** The same terraced plane continues from tile A into tile B east of it;
    /// alone, A's east edge is smoothed with half its box missing; once B is held both are
    /// recomputed with each other as context and the step across the edge is the plane's.
    #[test]
    fn a_neighbour_landing_resmooths_the_tile_and_the_edge_has_no_seam() {
        let px = 64;
        let s = 0.08;
        let a_id = TileId {
            z: 15,
            x: 100,
            y: 100,
        };
        let b_id = TileId {
            z: 15,
            x: 101,
            y: 100,
        };
        let a = tile_from_heights(a_id, px, &terraced_plane(px, s, 6, 0.0));
        let b = tile_from_heights(b_id, px, &terraced_plane(px, s, 6, px as f64));
        let mut store = TileStore::default();
        // Centre the want on the pair so both are kept.
        let (lat_s, lat_n, lon_w, _) = a_id.bounds_deg();
        let (_, _, _, lon_e) = b_id.bounds_deg();
        store.want_patch(0.5 * (lat_s + lat_n), 0.5 * (lon_w + lon_e), 15, 1);
        assert!(store.insert(a), "a terraced tile queues its own smoothing");
        store.finish_smoothing();
        let ids = |v: Vec<(TileId, usize, usize, usize, usize)>| -> Vec<TileId> {
            let mut ids: Vec<TileId> = v.into_iter().map(|b| b.0).collect();
            ids.sort_by_key(|t| t.x);
            ids.dedup();
            ids
        };
        assert_eq!(
            ids(store.take_resmoothed()),
            vec![a_id],
            "A alone, smoothed once"
        );
        store.insert(b);
        assert!(
            store.take_resmoothed().is_empty(),
            "the work waits for a frame"
        );
        store.finish_smoothing();
        let again = ids(store.take_resmoothed());
        assert_eq!(
            again,
            vec![a_id, b_id],
            "B smoothed, and A again with B as context: {again:?}"
        );
        let ta = &store.tiles[&a_id];
        let tb = &store.tiles[&b_id];
        let mut worst = 0.0f64;
        for y in 0..px {
            let step = tb.surface[y * px] as f64 - ta.surface[y * px + px - 1] as f64;
            worst = worst.max((step - s).abs());
        }
        println!("edge step vs the plane's {s}: worst deviation {worst:.4}");
        assert!(worst < 0.1, "a seam of {worst} across the tile edge");
    }

    /// ★★ **A ring fades to the ring beneath it, not to the raster** (docs/46 row 97). The fine
    /// 3×3 says 232 m, the ring beneath says 300 m, the raster 552 m: inside the fine patch the
    /// surface is 232; at the fine patch's edge it is 300, the ring beneath; only past the coarse
    /// ring's edge is it the raster.
    #[test]
    fn a_ring_fades_to_the_ring_beneath_it() {
        let z = 15;
        let (lat, lon) = (54.34, 158.23);
        let mut store = TileStore::default();
        store.want_patch(lat, lon, z, 1);
        assert_eq!(store.ring_zooms(), vec![15, 11, 7]);
        for id in tiles_around(lat, lon, z, 1) {
            store.insert(flat_tile(id, 232.0));
        }
        for id in tiles_around(lat, lon, z - RING_STEP, 1) {
            store.insert(flat_tile(id, 300.0));
        }
        assert_eq!(store.len(), 18);
        let (h, trust, _) = store.blend(lat, lon, 552.0, 19_500.0);
        assert!(
            (h - 232.0).abs() < 1e-6 && (trust - 1.0).abs() < 1e-9,
            "centre: {h} {trust}"
        );
        // The fine patch's outer edge: its weight is 0 there, and the surface is the ring beneath.
        let (lat_s, _, _, _) = tiles_around(lat, lon, z, 1)
            .iter()
            .map(|t| t.bounds_deg())
            .fold((f64::INFINITY, 0.0, 0.0, 0.0), |a, b| {
                (a.0.min(b.0), 0.0, 0.0, 0.0)
            });
        let (h, trust, _) = store.blend(lat_s + 1e-7, lon, 552.0, 19_500.0);
        assert!(
            (h - 300.0).abs() < 0.01,
            "fine edge fades to the ring beneath: {h}"
        );
        assert!((trust - 1.0).abs() < 1e-6, "still measured ground: {trust}");
        // Past the coarse ring entirely: the raster.
        let (h, trust, base) = store.blend(lat + 3.0, lon, 552.0, 19_500.0);
        assert_eq!((h, trust, base), (552.0, 0.0, 19_500.0));
        // What the finest ring fades into, by name.
        let (b, _, _) = store.blend_beneath_finest(lat, lon, 552.0, 19_500.0);
        assert!((b - 300.0).abs() < 1e-6, "beneath the finest ring: {b}");
    }

    /// ★ **A void fine tile lets the ring beneath answer.** No rule names the case: the void tile
    /// answers nothing, its ring's weight is 0 there, and the stack's next ring is the surface.
    #[test]
    fn a_void_fine_tile_falls_through_to_the_ring_beneath() {
        let z = 14;
        let (lat, lon) = (46.5, 10.0);
        let mut store = TileStore::default();
        store.want_patch(lat, lon, z, 1);
        let centre = tiles_around(lat, lon, z, 1)[0];
        let px = 8u32;
        store.insert(Tile::new(centre, px, 3, vec![0u8; (px * px * 3) as usize]));
        for id in tiles_around(lat, lon, z - RING_STEP, 1) {
            store.insert(flat_tile(id, 1500.0));
        }
        let (h, trust, _) = store.blend(lat, lon, 0.0, 19_500.0);
        assert!(
            (h - 1500.0).abs() < 1e-6,
            "the ring beneath answers under the void: {h}"
        );
        assert!((trust - 1.0).abs() < 1e-9);
    }

    /// ★ **The fade's gradient has no crease.** One tile held alone: its weight falls to zero at
    /// every edge. Along a line that crosses the tile's diagonal the old `smoothstep(min(d))` turned
    /// its gradient through 90° in one step; the product of smooth steps turns it gradually. The
    /// largest change of the weight's gradient between neighbouring samples is a small fraction of
    /// the gradient itself.
    #[test]
    fn the_fade_turns_its_gradient_without_a_crease() {
        let id = TileId {
            z: 15,
            x: 1000,
            y: 1000,
        };
        let mut store = TileStore::default();
        let (lat_s, lat_n, lon_w, lon_e) = id.bounds_deg();
        store.want_patch(0.5 * (lat_s + lat_n), 0.5 * (lon_w + lon_e), 15, 1);
        store.insert(flat_tile(id, 100.0));
        let w_at = |u: f64, v: f64| {
            store
                .elevation_at_zoom(15, lat_n + u * (lat_s - lat_n), lon_w + v * (lon_e - lon_w))
                .expect("covered")
                .1
        };
        let n = 400;
        let d = 1.0 / n as f64;
        let eps = 1e-4;
        let mut grads: Vec<(f64, f64)> = Vec::new();
        // Along the anti-diagonal from (0.05, 0.95) to (0.95, 0.05), crossing the diagonal at the centre.
        for k in 0..=n {
            let t = 0.05 + 0.9 * k as f64 / n as f64;
            let (u, v) = (t, 1.0 - t);
            let gu = (w_at(u + eps, v) - w_at(u - eps, v)) / (2.0 * eps);
            let gv = (w_at(u, v + eps) - w_at(u, v - eps)) / (2.0 * eps);
            grads.push((gu, gv));
        }
        let gmax = grads.iter().map(|(a, b)| a.hypot(*b)).fold(0.0, f64::max);
        let turn_max = grads
            .windows(2)
            .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
            .fold(0.0, f64::max);
        println!("fade gradient: largest {gmax:.3} per tile, largest change between samples {d:.4} apart: {turn_max:.4}");
        assert!(gmax > 0.5, "the weight does fall across the tile: {gmax}");
        assert!(
            turn_max < 0.05 * gmax,
            "a crease: the gradient changes by {turn_max} in one step of {d} ({:.1}% of its size)",
            100.0 * turn_max / gmax
        );
        // And the weight is still 1 nowhere short of the interior of a surrounded tile, 0 at the edges.
        assert!(w_at(0.5, 0.5) > 0.0 && w_at(0.5, 0.5) < 1.0);
        assert!(w_at(1e-9, 0.5) < 1e-9 && w_at(0.5, 1.0 - 1e-6) < 1e-9);
    }

    /// The coarsest ring is asked for first, nearest first within each ring.
    #[test]
    fn the_coarse_rings_are_wanted_first() {
        let mut store = TileStore::default();
        store.want_patch(54.34, 158.23, 14, 1);
        let m = store.missing();
        assert_eq!(m.len(), 27);
        assert!(
            m[..9].iter().all(|t| t.z == 6),
            "coarsest first: {:?}",
            &m[..3]
        );
        assert!(m[9..18].iter().all(|t| t.z == 10));
        assert!(m[18..].iter().all(|t| t.z == 14));
        assert_eq!(
            m[0],
            tiles_around(54.34, 158.23, 6, 1)[0],
            "nearest first within a ring"
        );
    }
}
