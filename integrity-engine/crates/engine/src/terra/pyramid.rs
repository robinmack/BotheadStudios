//! **The appearance pyramid — the ground's colour and roughness as a function of WHERE and of HOW
//! MUCH GROUND ONE PIXEL SEES, anchored to the world and not to any mesh** (docs/73 §2a; docs/46
//! row 91).
//!
//! Robin, from a 7 km descent: *"The entire system is expected to change scale not in abrupt tiers,
//! but in response to nearness and ability to render and perceive detail … Everything must change
//! based on POV of camera in the renderer, all fed from model/assembly properties."*
//!
//! What she saw was the appearance integral ([`super::appearance`], docs/63) evaluated per VERTEX of
//! a polar mesh centred under the eye: a starburst of colour wedges along the spokes, rings of it along
//! the rings, and the whole pattern re-centring on the camera every time the mesh did (measured by
//! `native_render_tests::the_picture_does_not_move_when_the_camera_does`, 2026-11-02: after a 528 m
//! move the slid-back frames differed by 2.10/255 per channel against a one-pixel difference of
//! 0.10). The integral was right; the footprint it was asked over — a mesh cell, kilometres long at
//! altitude and shaped by the mesh — was the wrong one, and docs/47 names a change of representation
//! that moves the world as the fudge seam.
//!
//! This module asks the same integral over footprints that do not move: the texels of an
//! equirectangular grid whose edges lie on fixed meridians and parallels. A **level** is that integral
//! over footprints of one size; the level above it is the integral over footprints twice the size,
//! which by docs/63's convergence rule must equal the combination of the four below it — and does,
//! because [`Appearance::combine`] carries both terms of the law of total variance
//! (`a_level_is_the_integral_of_the_level_below`). The renderer then samples the level whose texel
//! matches the ground one pixel actually sees, from the pixel's own derivatives, so a frame from 7 km
//! and a frame from 70 m read different levels of one pyramid and nothing changes in tiers.
//!
//! Two pyramids, one rule. The **globe** is the whole base grid — the shipped rasters' own 2048 × 1024
//! — built from one point per texel, because a base texel is the data's resolution and a footprint
//! smaller than the data has nothing to integrate (its colour is the texel's class mixture in season,
//! its roughness zero: the mesh already carries the raster's slope, and the raster has no sub-texel
//! variance to report). The **patch** is a window over the streamed elevation tiles, subdivided to the
//! tiles' resolution, each texel at the data's own step and every coarser level their exact
//! combination ([`Build`]); it has [`PATCH_LEVELS`] levels, and past its coarsest the shader reads
//! the globe. Both are built by the model's one surface rule (`Terra::surface_at`, docs/46 row 93) through the probe a caller supplies; this module
//! owns no datum and samples no raster itself.
//!
//! ★ The grid is anchored to the WORLD, not to the window. A window moving with the camera slides by
//! whole texels over a grid that does not move (`the_texel_grid_is_anchored_to_the_world`), so a texel
//! has one answer however the camera approaches it. That is the whole difference from the mesh.

use super::appearance::{integrate_rect_on_sphere, Kinds, Moments};
use crate::materials::Material;
use std::collections::VecDeque;

/// The base grid every window subdivides: an equirectangular `w × h` over 360° × 180°, texel (0, 0)
/// at the north-west corner (180° W, 90° N) — the shipped rasters' own addressing
/// (`raster::Raster::coords`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseGrid {
    pub w: u32,
    pub h: u32,
}

impl BaseGrid {
    /// One base texel on the ground at this latitude, `[east, north]` metres. The east–west side
    /// shrinks with the cosine of the latitude; the north–south side does not.
    pub fn texel_m(&self, radius_m: f64, lat_deg: f64) -> [f64; 2] {
        let north = std::f64::consts::PI * radius_m / self.h.max(1) as f64;
        let east = 2.0 * std::f64::consts::PI * radius_m / self.w.max(1) as f64
            * lat_deg.to_radians().cos().abs();
        [east, north]
    }
}

/// **A rectangle of texels on the base grid subdivided `sub` times per axis**, `sub` a power of two,
/// addressed in that finer grid's integer coordinates. `x0`/`y0` are the window's north-west texel;
/// `x0` may run past the base grid's width and is wrapped when a texel is located, so a window over
/// the dateline is one window.
///
/// A window's levels are the same ground at `sub`, `sub/2`, … 1: level `l` is this window with every
/// coordinate shifted right by `l`, which is why a window is kept aligned to multiples of `sub` — so
/// that every level has whole texels and the coarsest is a rectangle of the base grid itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub base: BaseGrid,
    pub sub: u32,
    pub x0: i64,
    pub y0: i64,
    pub w: u32,
    pub h: u32,
    /// How many levels this window's pyramid has. The globe's run all the way down to its base
    /// grid; a patch's stop at [`PATCH_LEVELS`], and the window is aligned to its coarsest texel
    /// rather than to the base grid — see `covering`.
    pub levels: u32,
}

/// **How many levels a patch window's pyramid has, at most** — so its coarsest texel is
/// `2^(PATCH_LEVELS-1)` of its finest, and the window is aligned to THAT, not to the base grid.
///
/// ★ Measured 2026-11-07 (the first run of `the_patch_arrives_in_slices_and_no_frame_pays_more_than_one`):
/// aligned to whole base texels, a 4.3 km tile patch over Kamchatka became a 19.5 km window — one
/// base texel is 19.5 km — and under the texel ceiling that window could afford 1024 × 1024 texels
/// of 19 m against a 5.6 m tile pixel: three and a half times coarser than the data it was built
/// from, with every texel a point sample (docs/46 row 95 at a different scale). Aligning to the
/// patch's own coarsest texel instead pads the patch by at most that texel on each side, so the
/// finest texel reaches the data. Past the coarsest level the shader reads the globe, as it did
/// before — the handoff footprint is now `2^(PATCH_LEVELS-1)` finest texels instead of a base texel.
pub const PATCH_LEVELS: u32 = 8;

impl Window {
    /// **The globe's window: a whole `w × h` equirectangular raster, with every coarser level down
    /// to the coarsest grid that still divides it.** The raster is taken as the finest level of a
    /// base grid `sub` times coarser, `sub` the largest power of two dividing both sides — for the
    /// shipped 2048 × 1024 that is a 2 × 1 base and eleven levels — so a view from orbit, where one
    /// pixel holds many raster texels, reads the level whose texels it can resolve.
    pub fn globe(w: u32, h: u32) -> Window {
        let sub = 1u32 << (w.max(1).trailing_zeros().min(h.max(1).trailing_zeros()));
        Window {
            base: BaseGrid {
                w: w / sub,
                h: h / sub,
            },
            sub,
            x0: 0,
            y0: 0,
            w,
            h,
            levels: sub.trailing_zeros() + 1,
        }
    }

    /// **The window covering a latitude/longitude box at texels no coarser than `target_m`**, with
    /// at most `max_texels` in it. The subdivision is the smallest power of two that brings the base
    /// texel's north–south side under the target; if the box then needs more texels than allowed the
    /// subdivision is halved until it fits, so a ceiling coarsens the window rather than cropping it.
    /// The window has at most [`PATCH_LEVELS`] levels and is aligned to its coarsest texel — a whole
    /// power of two of the base grid's own texels, so every level has whole texels and the grid stays
    /// anchored to the world. Longitudes may be given across the dateline (`lon_w > lon_e`).
    #[allow(clippy::too_many_arguments)]
    pub fn covering(
        base: BaseGrid,
        lat_s: f64,
        lat_n: f64,
        lon_w: f64,
        lon_e: f64,
        target_m: f64,
        radius_m: f64,
        max_texels: usize,
    ) -> Window {
        let base_north_m = base.texel_m(radius_m, 0.0)[1];
        let mut sub: u32 = if target_m > 0.0 && base_north_m > target_m {
            ((base_north_m / target_m).ceil() as u32).next_power_of_two()
        } else {
            1
        };
        let lon_e = if lon_e < lon_w { lon_e + 360.0 } else { lon_e };
        loop {
            let levels = (sub.trailing_zeros() + 1).min(PATCH_LEVELS);
            let grain = 1u32 << (levels - 1);
            let dpt = [
                360.0 / (base.w as f64 * sub as f64),
                180.0 / (base.h as f64 * sub as f64),
            ];
            let align = |v: f64, up: bool| -> i64 {
                let t = if up { v.ceil() } else { v.floor() } as i64;
                let s = grain as i64;
                if up {
                    (t + s - 1).div_euclid(s) * s
                } else {
                    t.div_euclid(s) * s
                }
            };
            let x0 = align((lon_w + 180.0) / dpt[0], false);
            let x1 = align((lon_e + 180.0) / dpt[0], true).max(x0 + grain as i64);
            let y0 = align((90.0 - lat_n) / dpt[1], false).max(0);
            let y1 = align((90.0 - lat_s) / dpt[1], true)
                .max(y0 + grain as i64)
                .min(base.h as i64 * sub as i64);
            let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
            if (w as usize) * (h as usize) <= max_texels.max(1) || sub == 1 {
                return Window {
                    base,
                    sub,
                    x0,
                    y0,
                    w,
                    h,
                    levels,
                };
            }
            sub /= 2;
        }
    }

    /// How many levels this window's pyramid has.
    pub fn levels(&self) -> u32 {
        self.levels
    }

    /// This window at level `l` — the same ground, texels `2^l` times larger.
    pub fn at_level(&self, l: u32) -> Window {
        debug_assert!(
            l < self.levels(),
            "level {l} of a {}-level window",
            self.levels()
        );
        Window {
            base: self.base,
            sub: self.sub >> l,
            x0: self.x0 >> l,
            y0: self.y0 >> l,
            w: self.w >> l,
            h: self.h >> l,
            levels: self.levels - l,
        }
    }

    /// Degrees per texel, `[lon, lat]`.
    pub fn deg_per_texel(&self) -> [f64; 2] {
        [
            360.0 / (self.base.w as f64 * self.sub as f64),
            180.0 / (self.base.h as f64 * self.sub as f64),
        ]
    }

    /// The window's western edge, degrees (unwrapped: may exceed 180 for a window over the dateline).
    pub fn west_deg(&self) -> f64 {
        self.x0 as f64 * self.deg_per_texel()[0] - 180.0
    }

    /// The window's northern edge, degrees.
    pub fn north_deg(&self) -> f64 {
        90.0 - self.y0 as f64 * self.deg_per_texel()[1]
    }

    /// The centre of texel `(x, y)` of this window, `(lat, lon)` degrees, longitude wrapped to
    /// `[-180, 180)`.
    pub fn centre(&self, x: u32, y: u32) -> (f64, f64) {
        let dpt = self.deg_per_texel();
        let mut lon = self.west_deg() + (x as f64 + 0.5) * dpt[0];
        lon = (lon + 180.0).rem_euclid(360.0) - 180.0;
        let lat = self.north_deg() - (y as f64 + 0.5) * dpt[1];
        (lat, lon)
    }

    /// One texel of row `y` on the ground, `[east, north]` metres, at the row's centre latitude.
    pub fn texel_m(&self, radius_m: f64, y: u32) -> [f64; 2] {
        let (lat, _) = self.centre(0, y);
        let b = self.base.texel_m(radius_m, lat);
        [b[0] / self.sub as f64, b[1] / self.sub as f64]
    }

    /// The centre of texel `(x, y)` of this window's grid, the indices unbounded — a texel beside
    /// the window (a neighbour the gradient needs) is on the same grid. Latitude is clamped to the
    /// poles; longitude wraps.
    pub fn centre_i(&self, x: i64, y: i64) -> (f64, f64) {
        let dpt = self.deg_per_texel();
        let mut lon = self.west_deg() + (x as f64 + 0.5) * dpt[0];
        lon = (lon + 180.0).rem_euclid(360.0) - 180.0;
        let lat = (self.north_deg() - (y as f64 + 0.5) * dpt[1]).clamp(-90.0, 90.0);
        (lat, lon)
    }

    /// The level-0 texel under a point, clamped into the window — `(x, y)`.
    pub fn texel_at_level0(&self, lat: f64, lon: f64) -> (u32, u32) {
        let dpt = self.deg_per_texel();
        let x = ((lon - self.west_deg()).rem_euclid(360.0) / dpt[0]).floor();
        let y = ((self.north_deg() - lat) / dpt[1]).floor();
        (
            (x.max(0.0) as u32).min(self.w.saturating_sub(1)),
            (y.max(0.0) as u32).min(self.h.saturating_sub(1)),
        )
    }

    /// **The level-0 texels under a box of ground**, `(lat_s, lat_n, lon_w, lon_e)` degrees, clipped
    /// to the window; `None` where the box and the window share no texel. The box's longitudes are
    /// unwrapped about the window's west edge, so a box given across the dateline names one rect.
    pub fn rect_covering_deg(
        &self,
        lat_s: f64,
        lat_n: f64,
        lon_w: f64,
        lon_e: f64,
    ) -> Option<Rect> {
        if !(lat_n > lat_s) || !(lon_e > lon_w) {
            return None;
        }
        let dpt = self.deg_per_texel();
        let west = self.west_deg();
        let lw = west + ((lon_w - west + 180.0).rem_euclid(360.0) - 180.0);
        let le = lw + (lon_e - lon_w).min(360.0);
        let x0 = ((lw - west) / dpt[0]).floor().max(0.0);
        let x1 = ((le - west) / dpt[0]).ceil().min(self.w as f64);
        let y0 = ((self.north_deg() - lat_n) / dpt[1]).floor().max(0.0);
        let y1 = ((self.north_deg() - lat_s) / dpt[1])
            .ceil()
            .min(self.h as f64);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Rect {
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }

    /// Whether a point lies inside this window.
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        let dpt = self.deg_per_texel();
        let dy = (self.north_deg() - lat) / dpt[1];
        if dy < 0.0 || dy >= self.h as f64 {
            return false;
        }
        let dx = ((lon - self.west_deg()).rem_euclid(360.0)) / dpt[0];
        dx < self.w as f64
    }
}

/// One level of a pyramid: the integral over every texel of its window, as the moments a renderer
/// acts on. Rows run north to south, columns west to east.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub w: u32,
    pub h: u32,
    /// Mean albedo over the texel, in season.
    pub albedo: Vec<[f32; 3]>,
    /// Mean gradient `(east, north)` — what the mesh's normal carries; kept so the next level's
    /// between-texel variance can be formed.
    pub mean_slope: Vec<[f32; 2]>,
    /// Variance of the gradient about that mean, both axes summed (tangent²).
    pub slope_var: Vec<f32>,
    /// The material holding the largest share — the grain layer the shader samples.
    pub material: Vec<u16>,
}

impl Level {
    fn blank(w: u32, h: u32) -> Level {
        let n = (w as usize) * (h as usize);
        Level {
            w,
            h,
            albedo: vec![[0.0; 3]; n],
            mean_slope: vec![[0.0; 2]; n],
            slope_var: vec![0.0; n],
            material: vec![0; n],
        }
    }

    #[inline]
    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.w as usize) + x as usize
    }

    /// The RMS slope angle of texel `i`, radians — [`Appearance::sigma_rad`]'s rule.
    pub fn sigma_rad(&self, i: usize) -> f32 {
        self.slope_var[i].max(0.0).sqrt().atan()
    }
}

/// **A window's levels, finest first.** `levels[0]` is the window itself; `levels[l]` is
/// `window.at_level(l)`.
#[derive(Clone, Debug)]
pub struct Pyramid {
    pub window: Window,
    pub levels: Vec<Level>,
}

impl Pyramid {
    /// **The pyramid whose finest level is a point per texel** — for a window whose texel IS the
    /// data's resolution, where there is nothing finer to integrate. `point(lat, lon)` answers the
    /// colour (in season) and the material at a texel's centre.
    pub fn from_points(
        window: Window,
        radius_m: f64,
        point: impl Fn(f64, f64) -> ([f32; 3], u16),
    ) -> Pyramid {
        let mut l0 = Level::blank(window.w, window.h);
        for y in 0..window.h {
            for x in 0..window.w {
                let (lat, lon) = window.centre(x, y);
                let (albedo, material) = point(lat, lon);
                let i = l0.idx(x, y);
                l0.albedo[i] = albedo;
                l0.material[i] = material;
            }
        }
        Self::raise(window, radius_m, l0)
    }

    /// **The pyramid whose finest level integrates each texel's footprint** over the ground a probe
    /// reports, at the data's own step. `step_m(lat, lon)` is the datum's feature size under the
    /// texel; `turned(lat)` the season there; `probe(lat, lon)` the surface — height and the kind of
    /// ground; `point(lat, lon)` the colour and material of a point, used where the texel is finer
    /// than the data under it and the integral has nothing to say (see [`integrate_rect_on_sphere`]).
    #[allow(clippy::too_many_arguments)]
    pub fn integrated(
        window: Window,
        radius_m: f64,
        materials: &[Material],
        kinds: &Kinds,
        max_side: usize,
        step_m: impl Fn(f64, f64) -> f64,
        turned: impl Fn(f64) -> f64,
        probe: impl Fn(f64, f64) -> (f64, usize),
        point: impl Fn(f64, f64) -> ([f32; 3], u16),
    ) -> Pyramid {
        let mut l0 = Level::blank(window.w, window.h);
        let mut scratch = Moments::new();
        for y in 0..window.h {
            let size = window.texel_m(radius_m, y);
            for x in 0..window.w {
                let (lat, lon) = window.centre(x, y);
                let a = integrate_rect_on_sphere(
                    &mut scratch,
                    crate::geo::dir_from_lat_lon(lat, lon),
                    radius_m,
                    size,
                    step_m(lat, lon),
                    max_side,
                    materials,
                    kinds,
                    turned(lat),
                    &probe,
                );
                let i = l0.idx(x, y);
                if a.mix.is_empty() {
                    let (albedo, material) = point(lat, lon);
                    l0.albedo[i] = albedo;
                    l0.material[i] = material;
                } else {
                    l0.albedo[i] = a.albedo;
                    l0.mean_slope[i] = [a.mean_slope[0] as f32, a.mean_slope[1] as f32];
                    l0.slope_var[i] = a.slope_variance as f32;
                    l0.material[i] = a.material as u16;
                }
            }
        }
        Self::raise(window, radius_m, l0)
    }

    /// Build every coarser level from the finest.
    fn raise(window: Window, radius_m: f64, l0: Level) -> Pyramid {
        let mut levels = vec![l0];
        for l in 1..window.levels() {
            let below = window.at_level(l - 1);
            let next = Self::coarsen(&levels[(l - 1) as usize], &below, radius_m);
            levels.push(next);
        }
        Pyramid { window, levels }
    }

    /// **One level from the level below it: the law of total variance, area-weighted.** Each texel
    /// combines its four children with their ground areas as weights (a northern child is narrower
    /// than a southern one): the mean gradient is the weighted mean of theirs; the variance is the
    /// weighted mean of their variances PLUS the weighted variance of their means — the term a naive
    /// average loses, and the whole reason the integral is refinement-invariant (docs/63); the colour
    /// is the weighted mean of theirs, which is the integral's linearity; the material is the one with
    /// the largest area among them, ties to the lower index (`globe_mesh::share_order`'s rule).
    fn coarsen(fine: &Level, fine_window: &Window, radius_m: f64) -> Level {
        let (w, h) = (fine.w / 2, fine.h / 2);
        let mut out = Level::blank(w, h);
        for y in 0..h {
            for x in 0..w {
                Self::coarsen_one(fine, fine_window, radius_m, x, y, &mut out);
            }
        }
        out
    }

    /// **One coarse texel from its four children** — the body of [`coarsen`](Self::coarsen), and the
    /// only place the combination is written, so a level rebuilt texel by texel (`Build`) and a level
    /// built whole are the same level (`a_patch_built_in_slices_is_the_patch_built_at_once`).
    fn coarsen_one(
        fine: &Level,
        fine_window: &Window,
        radius_m: f64,
        x: u32,
        y: u32,
        out: &mut Level,
    ) {
        let area = |yy: u32| {
            let t = fine_window.texel_m(radius_m, yy);
            t[0] * t[1]
        };
        let (a_n, a_s) = (area(2 * y), area(2 * y + 1));
        let kids = [
            (fine.idx(2 * x, 2 * y), a_n),
            (fine.idx(2 * x + 1, 2 * y), a_n),
            (fine.idx(2 * x, 2 * y + 1), a_s),
            (fine.idx(2 * x + 1, 2 * y + 1), a_s),
        ];
        let total: f64 = kids.iter().map(|k| k.1).sum::<f64>().max(1e-300);
        let mut mean = [0.0f64; 2];
        let mut albedo = [0.0f64; 3];
        for &(i, a) in &kids {
            let wgt = a / total;
            mean[0] += fine.mean_slope[i][0] as f64 * wgt;
            mean[1] += fine.mean_slope[i][1] as f64 * wgt;
            for c in 0..3 {
                albedo[c] += fine.albedo[i][c] as f64 * wgt;
            }
        }
        let mut var = 0.0f64;
        let mut vote: Vec<(u16, f64)> = Vec::with_capacity(4);
        for &(i, a) in &kids {
            let wgt = a / total;
            let d0 = fine.mean_slope[i][0] as f64 - mean[0];
            let d1 = fine.mean_slope[i][1] as f64 - mean[1];
            var += wgt * (fine.slope_var[i].max(0.0) as f64 + d0 * d0 + d1 * d1);
            match vote.iter_mut().find(|(m, _)| *m == fine.material[i]) {
                Some((_, acc)) => *acc += wgt,
                None => vote.push((fine.material[i], wgt)),
            }
        }
        let material = vote
            .iter()
            .min_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)))
            .map_or(0, |&(m, _)| m);
        let o = out.idx(x, y);
        out.albedo[o] = [albedo[0] as f32, albedo[1] as f32, albedo[2] as f32];
        out.mean_slope[o] = [mean[0] as f32, mean[1] as f32];
        out.slope_var[o] = var as f32;
        out.material[o] = material;
    }

    /// The window of level `l`.
    pub fn level_window(&self, l: u32) -> Window {
        self.window.at_level(l)
    }

    /// The texel of level `l` under a point, `(x, y)`, if the window covers it.
    pub fn texel_at(&self, l: u32, lat: f64, lon: f64) -> Option<(u32, u32)> {
        let win = self.level_window(l);
        if !win.contains(lat, lon) {
            return None;
        }
        let dpt = win.deg_per_texel();
        let x = ((lon - win.west_deg()).rem_euclid(360.0) / dpt[0]).floor() as u32;
        let y = ((win.north_deg() - lat) / dpt[1]).floor() as u32;
        Some((x.min(win.w - 1), y.min(win.h - 1)))
    }
}

/// A rectangle of texels of one level: `(x, y)` its north-west texel, `w × h` its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// This rect clipped to a `w × h` level.
    fn clip(self, w: u32, h: u32) -> Rect {
        let x1 = (self.x as u64 + self.w as u64).min(w as u64) as u32;
        let y1 = (self.y as u64 + self.h as u64).min(h as u64) as u32;
        let x = self.x.min(x1);
        let y = self.y.min(y1);
        Rect {
            x,
            y,
            w: x1 - x,
            h: y1 - y,
        }
    }

    /// The smallest rect holding both.
    fn union(self, o: Rect) -> Rect {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        Rect {
            x,
            y,
            w: x1 - x,
            h: y1 - y,
        }
    }

    /// The rect of these texels' parents, one level up.
    fn parent(self) -> Rect {
        let x = self.x / 2;
        let y = self.y / 2;
        Rect {
            x,
            y,
            w: (self.x + self.w).div_ceil(2) - x,
            h: (self.y + self.h).div_ceil(2) - y,
        }
    }
}

/// What one point of ground answers the builder with, by the one surface rule: its height, its
/// colour in season and its material.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub height_m: f64,
    pub albedo: [f32; 3],
    pub material: u16,
}

/// How many texels the builder samples between two looks at its deadline. A row of the finest level
/// can be thousands of texels, too coarse a grain for a per-frame slice; a few hundred samples is
/// well under a millisecond and keeps the overrun past a deadline bounded by that.
pub const BUILD_CHUNK: usize = 256;

/// ★ **The patch pyramid, built a slice at a time from the data's own lattice** (docs/46 row 95).
///
/// The finest level's texel is at the data's step, so it is one point — nothing finer exists to
/// integrate — and its gradient is the forward difference to the texels east and north of it: the
/// texel lattice IS the probe grid, and the integral over any coarser footprint is the exact
/// combination of the texels under it ([`Pyramid::coarsen`], the law of total variance). There is no
/// probe budget, because there is nothing a probe budget could coarsen that would make the build
/// cheaper: the cost is one surface query per texel, and a slower machine pays it over more frames,
/// not with a coarser answer (Law VIII's convergence clause: raise the budget, the same picture
/// arrives sooner).
///
/// Work arrives as rects of level-0 texels ([`enqueue`](Self::enqueue) — a tile's ground, as the
/// tile lands) and is done in [`advance`](Self::advance) until a deadline the caller polls says stop;
/// the ancestors of every texel touched are re-coarsened before the slice ends, so every level is
/// consistent with the level below it at every frame boundary, and the regions changed are reported
/// for upload ([`take_touched`](Self::take_touched)). Before a texel is built it holds what the globe
/// holds there — the coarsest level's point sample, replicated down — so a window that has just
/// opened draws what the globe drew and detail replaces it texel by texel; nothing is read unbuilt.
pub struct Build {
    pub pyramid: Pyramid,
    radius_m: f64,
    queue: VecDeque<Rect>,
    /// Progress through the front rect: rows completed, and texels completed in the current row.
    row: u32,
    col: u32,
    /// Heights along the row NORTH of the current one, over the front rect's columns and one more to
    /// the east (`w + 1` entries), so a texel's north difference does not re-sample the ground.
    north: Vec<f64>,
    /// Heights along the current row so far, same layout; becomes `north` when the row completes.
    cur: Vec<f64>,
    /// Level-0 texels written since the ancestors were last re-coarsened.
    dirty: Rect,
    /// Regions changed since the last [`take_touched`](Self::take_touched), one per level.
    touched: Vec<(u32, Rect)>,
}

impl Build {
    /// A build over `window` with every texel showing the coarsest level: the base grid's point
    /// sample, `coarsest(lat, lon)` the colour and material there — the globe's own rule.
    pub fn new(
        window: Window,
        radius_m: f64,
        coarsest: impl Fn(f64, f64) -> ([f32; 3], u16),
    ) -> Build {
        let n = window.levels() as usize;
        let mut levels: Vec<Level> = (0..n)
            .map(|l| {
                let w = window.at_level(l as u32);
                Level::blank(w.w, w.h)
            })
            .collect();
        let top = n - 1;
        let tw = window.at_level(top as u32);
        for y in 0..tw.h {
            for x in 0..tw.w {
                let (lat, lon) = tw.centre(x, y);
                let (albedo, material) = coarsest(lat, lon);
                let i = levels[top].idx(x, y);
                levels[top].albedo[i] = albedo;
                levels[top].material[i] = material;
            }
        }
        for l in (0..top).rev() {
            let (fine, coarse) = levels.split_at_mut(l + 1);
            let (fine, coarse) = (&mut fine[l], &coarse[0]);
            for y in 0..fine.h {
                for x in 0..fine.w {
                    let i = fine.idx(x, y);
                    let j = coarse.idx(x / 2, y / 2);
                    fine.albedo[i] = coarse.albedo[j];
                    fine.material[i] = coarse.material[j];
                }
            }
        }
        Build {
            pyramid: Pyramid { window, levels },
            radius_m,
            queue: VecDeque::new(),
            row: 0,
            col: 0,
            north: Vec::new(),
            cur: Vec::new(),
            dirty: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            touched: Vec::new(),
        }
    }

    /// **Take over the texels this window shares with an older pyramid on the same grid.** The grid is
    /// anchored to the world, so a texel both hold has one answer. Returns the shared ground as a
    /// level-0 rect of this window, `None` when the two share no texel (or stand on different grids).
    pub fn adopt(&mut self, old: &Pyramid) -> Option<Rect> {
        let (wn, wo) = (self.pyramid.window, old.window);
        if wn.base != wo.base || wn.sub != wo.sub {
            return None;
        }
        let mut shared = None;
        for l in 0..wn.levels() {
            let (a, b) = (wn.at_level(l), wo.at_level(l));
            // The old window's offset from the new, the nearest turn of longitude taken out.
            let turn = (a.base.w as i64) * (a.sub as i64);
            let mut dx = b.x0 - a.x0;
            dx -= (dx as f64 / turn as f64).round() as i64 * turn;
            let dy = b.y0 - a.y0;
            let x0 = dx.max(0);
            let x1 = (dx + b.w as i64).min(a.w as i64);
            let y0 = dy.max(0);
            let y1 = (dy + b.h as i64).min(a.h as i64);
            if x1 <= x0 || y1 <= y0 {
                return shared;
            }
            let (new, from) = (
                &mut self.pyramid.levels[l as usize],
                &old.levels[l as usize],
            );
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = new.idx(x as u32, y as u32);
                    let j = from.idx((x - dx) as u32, (y - dy) as u32);
                    new.albedo[i] = from.albedo[j];
                    new.mean_slope[i] = from.mean_slope[j];
                    new.slope_var[i] = from.slope_var[j];
                    new.material[i] = from.material[j];
                }
            }
            let r = Rect {
                x: x0 as u32,
                y: y0 as u32,
                w: (x1 - x0) as u32,
                h: (y1 - y0) as u32,
            };
            self.touched.push((l, r));
            if l == 0 {
                shared = Some(r);
            }
        }
        shared
    }

    /// Ask for a rect of level-0 texels to be built (again, if it was). Clipped to the window.
    pub fn enqueue(&mut self, rect: Rect) {
        let r = rect.clip(self.pyramid.window.w, self.pyramid.window.h);
        if !r.is_empty() {
            self.queue.push_back(r);
        }
    }

    /// Whether any texel remains to be built.
    pub fn pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Texels still to build, over every rect queued.
    pub fn remaining(&self) -> usize {
        let mut n = 0usize;
        for (k, r) in self.queue.iter().enumerate() {
            n += (r.w as usize) * (r.h as usize);
            if k == 0 {
                n -= (self.row as usize) * (r.w as usize) + self.col as usize;
            }
        }
        n
    }

    /// **Build until `stop()` says so or nothing is left.** `sample(lat, lon)` is the one surface
    /// rule; `stop` is polled every [`BUILD_CHUNK`] samples. Returns the texels completed. Before it
    /// returns, every ancestor of every texel written is re-coarsened.
    pub fn advance(
        &mut self,
        sample: impl Fn(f64, f64) -> Sample,
        mut stop: impl FnMut() -> bool,
    ) -> usize {
        let window = self.pyramid.window;
        let mut done = 0usize;
        let mut samples = 0usize;
        'rects: while let Some(rect) = self.queue.front().copied() {
            let width = rect.w as usize;
            if self.row == 0 && self.col == 0 && self.north.len() != width + 1 {
                // A fresh rect: the row north of it, so the first row has a north neighbour.
                let y = rect.y as i64 - 1;
                self.north = (0..=width)
                    .map(|k| {
                        let (la, lo) = window.centre_i(rect.x as i64 + k as i64, y);
                        sample(la, lo).height_m
                    })
                    .collect();
                self.cur = vec![0.0; width + 1];
                samples += width + 1;
            }
            while self.row < rect.h {
                let y = rect.y + self.row;
                let size = window.texel_m(self.radius_m, y);
                while self.col < rect.w {
                    if samples >= BUILD_CHUNK {
                        samples = 0;
                        if stop() {
                            break 'rects;
                        }
                    }
                    let x = rect.x + self.col;
                    let c = self.col as usize;
                    let (la, lo) = window.centre(x, y);
                    let s = sample(la, lo);
                    samples += 1;
                    self.cur[c] = s.height_m;
                    let l0 = &mut self.pyramid.levels[0];
                    let i = l0.idx(x, y);
                    l0.albedo[i] = s.albedo;
                    l0.material[i] = s.material;
                    // The texel west of this one now has its east neighbour: finish its gradient.
                    if c > 0 {
                        Self::finish(l0, &self.cur, &self.north, x - 1, y, c - 1, size);
                    }
                    self.col += 1;
                    if self.col == rect.w {
                        // One more height to the east, beyond the rect, for the last texel.
                        let (la, lo) = window.centre_i(x as i64 + 1, y as i64);
                        let east = sample(la, lo).height_m;
                        samples += 1;
                        self.cur[c + 1] = east;
                        Self::finish(l0, &self.cur, &self.north, x, y, c, size);
                    }
                    done += 1;
                    self.dirty = self.dirty.union(Rect { x, y, w: 1, h: 1 });
                }
                std::mem::swap(&mut self.north, &mut self.cur);
                self.row += 1;
                self.col = 0;
            }
            // The rect is complete: its ancestors now, so the next rect's region is its own and a
            // slice that finishes one tile and starts another does not report (and upload) the box
            // around both.
            self.recoarsen();
            self.queue.pop_front();
            self.row = 0;
            self.col = 0;
            self.north.clear();
        }
        self.recoarsen();
        done
    }

    /// The gradient of texel `(x, y)` from the heights of its row and the row north of it.
    #[inline]
    fn finish(
        l0: &mut Level,
        cur: &[f64],
        north: &[f64],
        x: u32,
        y: u32,
        c: usize,
        size: [f64; 2],
    ) {
        let i = l0.idx(x, y);
        let east = (cur[c + 1] - cur[c]) / size[0];
        let up = (north[c] - cur[c]) / size[1];
        l0.mean_slope[i] = [east as f32, up as f32];
        l0.slope_var[i] = 0.0;
    }

    /// Re-coarsen the ancestors of every level-0 texel written since the last time, and record the
    /// regions changed.
    fn recoarsen(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let window = self.pyramid.window;
        let mut r = self.dirty;
        self.touched.push((0, r));
        for l in 1..window.levels() {
            let coarse_w = window.at_level(l);
            r = r.parent().clip(coarse_w.w, coarse_w.h);
            if r.is_empty() {
                break;
            }
            let fine_w = window.at_level(l - 1);
            let (below, above) = self.pyramid.levels.split_at_mut(l as usize);
            let (fine, coarse) = (&below[(l - 1) as usize], &mut above[0]);
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    Pyramid::coarsen_one(fine, &fine_w, self.radius_m, x, y, coarse);
                }
            }
            self.touched.push((l, r));
        }
        self.dirty = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
    }

    /// The regions changed since the last call, `(level, rect)`, finest first, and forget them. A
    /// level may appear more than once — one region per rect worked on — and they are not united,
    /// because the box around two distant tiles is most of the window.
    pub fn take_touched(&mut self) -> Vec<(u32, Rect)> {
        let mut out: Vec<(u32, Rect)> = self.touched.drain(..).collect();
        out.sort_by_key(|(l, _)| *l);
        out
    }
}

/// How the shader reads a texel: the albedo encoded as 8-bit sRGB (so the darks keep their steps and
/// the GPU filters in linear light), and the RMS slope angle in the alpha channel as a fraction of
/// the vertical — `sigma / (π/2)`, the largest value a slope angle can take.
pub const SIGMA_FULL_SCALE: f32 = std::f32::consts::FRAC_PI_2;

/// Linear reflectance → the 8-bit sRGB code the texture holds. The standard transfer, rounded to
/// nearest so the encoded mean is unbiased (the lesson of `texture::build_mips`, docs/46 row 90).
pub fn srgb8(linear: f32) -> u8 {
    let x = if linear.is_finite() {
        linear.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let s = if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// The 8-bit sRGB code → linear reflectance; the GPU's own decode, for tests to read a texel back.
pub fn srgb8_to_linear(code: u8) -> f32 {
    let s = code as f32 / 255.0;
    if s <= 0.040_45 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// A level as the RGBA8 texels an `Rgba8UnormSrgb` texture takes: sRGB albedo, alpha = sigma over
/// its full scale.
pub fn encode_rgba_srgb(level: &Level) -> Vec<u8> {
    encode_rgba_srgb_rect(level, level_rect(level))
}

/// The whole of a level as a rect.
pub fn level_rect(level: &Level) -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: level.w,
        h: level.h,
    }
}

/// [`encode_rgba_srgb`] over one rect of the level, rows packed.
pub fn encode_rgba_srgb_rect(level: &Level, r: Rect) -> Vec<u8> {
    let mut out = Vec::with_capacity((r.w as usize) * (r.h as usize) * 4);
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            let i = level.idx(x, y);
            let a = level.albedo[i];
            out.push(srgb8(a[0]));
            out.push(srgb8(a[1]));
            out.push(srgb8(a[2]));
            let s = (level.sigma_rad(i) / SIGMA_FULL_SCALE * 255.0).round();
            out.push(s.clamp(0.0, 255.0) as u8);
        }
    }
    out
}

/// An `f32` as the IEEE 754 half-precision bits an `Rg16Float` texture takes — round to nearest,
/// with the overflow to infinity and the subnormals handled, because the tangent of a slope is a
/// small signed number and a half carries eleven significant bits of it (a slope of 0.1 to 1e-4).
/// WebGPU has no 16-bit normalised formats in its core, which is why the slope travels as a half.
pub fn f16_bits(x: f32) -> u16 {
    let x = if x.is_finite() { x } else { 0.0 };
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;
    if exp == 0 {
        return sign; // an f32 subnormal is zero in a half
    }
    let e = exp - 127 + 15;
    if e >= 31 {
        return sign | 0x7c00; // too large: infinity
    }
    if e <= 0 {
        if e < -10 {
            return sign; // too small for a half subnormal
        }
        // A half subnormal: the implicit one restored, shifted into place, rounded.
        let m = (mant | 0x80_0000) >> (1 - e);
        let round = (m >> 13) + ((m >> 12) & 1);
        return sign | round as u16;
    }
    let half = ((e as u32) << 10) | (mant >> 13);
    // Round to nearest (ties up), carrying into the exponent if the mantissa overflows.
    let round = (mant >> 12) & 1;
    sign | (half + round) as u16
}

/// The half-precision bits back to an `f32` — the GPU's own decode, for tests to read a texel back.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mant = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign
        } else {
            // Subnormal: normalise.
            let mut e = 127 - 15 + 1;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 31 {
        sign | 0x7f80_0000 | (mant << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// A level's mean gradient `(east, north)` as the two half-float texels an `Rg16Float` texture
/// takes, little-endian. Stored as the tangent itself: no scale, no clamp beyond a half's range.
pub fn encode_slope(level: &Level) -> Vec<u8> {
    encode_slope_rect(level, level_rect(level))
}

/// [`encode_slope`] over one rect of the level, rows packed.
pub fn encode_slope_rect(level: &Level, r: Rect) -> Vec<u8> {
    let mut out = Vec::with_capacity((r.w as usize) * (r.h as usize) * 4);
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            let s = level.mean_slope[level.idx(x, y)];
            for v in s {
                out.extend_from_slice(&f16_bits(v).to_le_bytes());
            }
        }
    }
    out
}

/// A level's materials as the single-byte texels an `R8Uint` texture takes. The catalogue has a few
/// dozen materials; an index past 255 would be a catalogue of a different shape, and is reported
/// rather than wrapped.
pub fn encode_material(level: &Level) -> Vec<u8> {
    encode_material_rect(level, level_rect(level))
}

/// [`encode_material`] over one rect of the level, rows packed.
pub fn encode_material_rect(level: &Level, r: Rect) -> Vec<u8> {
    let mut out = Vec::with_capacity((r.w as usize) * (r.h as usize));
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            let m = level.material[level.idx(x, y)];
            out.push(u8::try_from(m).expect("a material index that fits the R8 texture"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    const R: f64 = 6.371e6;

    /// A base grid with 4.9 km texels (the shipped grid is 19.5 km; this one keeps test windows
    /// small), subdivided eight times to ~600 m texels: four levels.
    fn colorado_window() -> Window {
        Window::covering(
            BaseGrid { w: 8192, h: 4096 },
            39.98,
            40.02,
            -106.02,
            -105.98,
            700.0,
            R,
            1 << 16,
        )
    }

    /// Synthetic ground about (40 N, 106 W), in local metres: structure at several scales, curvature
    /// on both axes so quadrant means differ, and a kind boundary that wanders — the fixture
    /// `appearance::tests::refining_the_footprint_does_not_change_the_answer` uses, placed on the
    /// sphere. Kind 0 is a pure material, kind 1 a declared MIXTURE, so the expansion of kinds into
    /// materials is exercised rather than trivially the identity.
    fn ground(lat: f64, lon: f64) -> (f64, usize) {
        let x = R * (lon + 106.0).to_radians() * 40.0f64.to_radians().cos();
        let y = R * (lat - 40.0).to_radians();
        // The finest structure has a 190 m period against a 20 m probe step: resolved, not
        // aliased. The integral's own precondition is that the step is the DATA's resolution, so a
        // fixture with structure below the step would be testing the aliasing, not the pyramid.
        let h = 0.30 * x
            + 0.0015 * x * x / 1000.0
            + 0.0010 * y * y / 1000.0
            + 12.0 * (x / 90.0).sin()
            + 0.8 * (x / 30.0).sin() * (y / 30.0).cos();
        let kind = if y + 400.0 * (x / 900.0).sin() > 0.0 {
            0
        } else {
            1
        };
        (h, kind)
    }

    fn kinds(mats: &[Material]) -> Vec<Vec<(usize, f32)>> {
        let granite = crate::materials::index_of(mats, "granite");
        let sand = crate::materials::index_of(mats, "sand");
        let dirt = crate::materials::index_of(mats, "dirt");
        vec![vec![(granite, 1.0)], vec![(sand, 0.6), (dirt, 0.4)]]
    }

    fn point_of<'a>(
        kinds: &'a Kinds,
        mats: &'a [Material],
    ) -> impl Fn(f64, f64) -> ([f32; 3], u16) + 'a {
        move |lat, lon| {
            let (_, k) = ground(lat, lon);
            (
                crate::materials::aggregate_albedo(&kinds[k], mats),
                crate::terra::globe_mesh::dominant_constituent(&kinds[k]).unwrap_or(0) as u16,
            )
        }
    }

    /// **Every level of a window is the same ground**: the same west and north edges, texels that
    /// are whole multiples, and — for a window of fewer than [`PATCH_LEVELS`] levels — a coarsest
    /// level whose texels are the base grid's own.
    #[test]
    fn every_level_of_a_window_covers_the_same_ground() {
        let w = colorado_window();
        assert_eq!(
            w.sub, 8,
            "4.9 km texels at a 700 m target subdivide eight times"
        );
        assert_eq!(w.levels(), 4);
        assert_eq!(w.x0 % 8, 0);
        assert_eq!(w.y0 % 8, 0);
        assert_eq!(w.w % 8, 0);
        assert_eq!(w.h % 8, 0);
        for l in 0..w.levels() {
            let wl = w.at_level(l);
            assert!(
                (wl.west_deg() - w.west_deg()).abs() < 1e-12,
                "level {l} west edge"
            );
            assert!(
                (wl.north_deg() - w.north_deg()).abs() < 1e-12,
                "level {l} north edge"
            );
            assert_eq!(wl.w * (1 << l), w.w);
            assert_eq!(wl.h * (1 << l), w.h);
        }
        let top = w.at_level(w.levels() - 1);
        assert_eq!(top.sub, 1);
        let dpt = top.deg_per_texel();
        assert!((dpt[0] - 360.0 / 8192.0).abs() < 1e-12);
        assert!((dpt[1] - 180.0 / 4096.0).abs() < 1e-12);
        // And the box asked for is inside it.
        for (la, lo) in [(39.98, -106.02), (40.02, -105.98), (40.0, -106.0)] {
            assert!(w.contains(la, lo), "({la}, {lo}) is in the window");
        }
    }

    /// **The texel grid is anchored to the world, not to the window.** Two windows asked for about
    /// boxes a third of a texel apart land on the same grid: their texel centres coincide wherever
    /// they overlap, so a texel's footprint — and therefore its integral — is the same whichever
    /// window happens to hold it. This is the property the mesh never had.
    #[test]
    fn the_texel_grid_is_anchored_to_the_world() {
        let base = BaseGrid { w: 8192, h: 4096 };
        let a = Window::covering(base, 39.98, 40.02, -106.02, -105.98, 700.0, R, 1 << 16);
        let dpt = a.deg_per_texel();
        let (dlat, dlon) = (0.3 * dpt[1], 0.3 * dpt[0]);
        let b = Window::covering(
            base,
            39.98 + dlat,
            40.02 + dlat,
            -106.02 + dlon,
            -105.98 + dlon,
            700.0,
            R,
            1 << 16,
        );
        assert_eq!(a.sub, b.sub);
        // Edges differ by whole texels (multiples of the alignment, in fact).
        let dx = b.x0 - a.x0;
        let dy = b.y0 - a.y0;
        assert_eq!(dx % a.sub as i64, 0);
        assert_eq!(dy % a.sub as i64, 0);
        // A texel both windows hold has one centre.
        let (x, y) = (a.w / 2, a.h / 2);
        let (bx, by) = ((x as i64 - dx) as u32, (y as i64 - dy) as u32);
        assert!(bx < b.w && by < b.h, "the windows overlap at their middles");
        let ca = a.centre(x, y);
        let cb = b.centre(bx, by);
        assert!(
            (ca.0 - cb.0).abs() < 1e-12 && (ca.1 - cb.1).abs() < 1e-12,
            "{ca:?} vs {cb:?}"
        );
    }

    /// **A level is the integral of the level below it** (docs/63's convergence rule, the pyramid's
    /// form). Every coarser texel the pyramid holds — made by combining its four children — must
    /// equal the appearance integral asked DIRECTLY over that texel's footprint at the same data
    /// step: colour, mixture, mean gradient and variance. A pyramid that failed this would change the
    /// ground's colour with the altitude it is seen from, which is the tiering Robin described.
    #[test]
    fn a_level_is_the_integral_of_the_level_below() {
        let mats = mats();
        let kinds = kinds(&mats);
        let w = colorado_window();
        let step = 20.0;
        let p = Pyramid::integrated(
            w,
            R,
            &mats,
            &kinds,
            128,
            |_, _| step,
            |_| 0.0,
            ground,
            point_of(&kinds, &mats),
        );
        assert_eq!(p.levels.len() as u32, w.levels());
        let mut scratch = Moments::new();
        let mut checked = 0;
        for l in 1..w.levels() {
            let wl = w.at_level(l);
            let lev = &p.levels[l as usize];
            for y in 0..wl.h {
                for x in 0..wl.w {
                    let (lat, lon) = wl.centre(x, y);
                    let direct = integrate_rect_on_sphere(
                        &mut scratch,
                        crate::geo::dir_from_lat_lon(lat, lon),
                        R,
                        wl.texel_m(R, y),
                        step,
                        128,
                        &mats,
                        &kinds,
                        0.0,
                        ground,
                    );
                    let i = lev.idx(x, y);
                    for c in 0..3 {
                        assert!(
                            (lev.albedo[i][c] - direct.albedo[c]).abs() < 0.01,
                            "level {l} texel ({x},{y}) channel {c}: pyramid {} vs direct {}",
                            lev.albedo[i][c],
                            direct.albedo[c]
                        );
                    }
                    for k in 0..2 {
                        assert!(
                            (lev.mean_slope[i][k] as f64 - direct.mean_slope[k]).abs() < 2e-3,
                            "level {l} texel ({x},{y}) mean slope axis {k}: {} vs {}",
                            lev.mean_slope[i][k],
                            direct.mean_slope[k]
                        );
                    }
                    let rel = (lev.slope_var[i] as f64 - direct.slope_variance).abs()
                        / direct.slope_variance.max(1e-12);
                    assert!(
                        rel < 0.05,
                        "level {l} texel ({x},{y}) variance moved {:.1}%: {} vs {}",
                        rel * 100.0,
                        lev.slope_var[i],
                        direct.slope_variance
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked >= 1 + 4 + 16, "checked {checked} texels");
        // The mixture is exercised: somewhere a texel holds both kinds' materials.
        let l0 = &p.levels[0];
        let sand = crate::materials::index_of(&mats, "sand") as u16;
        let granite = crate::materials::index_of(&mats, "granite") as u16;
        assert!(
            l0.material.contains(&sand) && l0.material.contains(&granite),
            "both kinds lead somewhere in the window"
        );
    }

    /// **Below the data, a texel is its point sample** — the colour and material at its centre, no
    /// slope moments — exactly what `from_points` builds; the two constructors agree where the
    /// integral has nothing to say.
    #[test]
    fn below_the_data_a_texel_is_its_point_sample() {
        let mats = mats();
        let kinds = kinds(&mats);
        let w = colorado_window();
        let coarse_step = 1.0e5; // the data is coarser than every texel here
        let integrated = Pyramid::integrated(
            w,
            R,
            &mats,
            &kinds,
            16,
            |_, _| coarse_step,
            |_| 0.0,
            ground,
            point_of(&kinds, &mats),
        );
        let points = Pyramid::from_points(w, R, point_of(&kinds, &mats));
        for l in 0..w.levels() as usize {
            assert_eq!(
                integrated.levels[l], points.levels[l],
                "level {l} differs between the integrated and the point pyramid"
            );
        }
        assert!(
            integrated.levels[0].slope_var.iter().all(|&v| v == 0.0),
            "a point has seen no variation"
        );
    }

    /// **The season is the texel's own.** A kind with a deciduous constituent changes colour with
    /// `turned`; one without does not.
    #[test]
    fn the_season_turns_the_texels_that_have_one() {
        let mats = mats();
        let grass = crate::materials::index_of(&mats, "grass");
        let granite = crate::materials::index_of(&mats, "granite");
        assert_ne!(
            mats[grass].albedo_when_turned(1.0),
            mats[grass].albedo,
            "grass has a senescent state in the catalogue"
        );
        let kinds = vec![vec![(granite, 1.0f32)], vec![(grass, 1.0f32)]];
        let w = colorado_window();
        let build = |turned: f64| {
            Pyramid::integrated(
                w,
                R,
                &mats,
                &kinds,
                16,
                |_, _| 20.0,
                move |_| turned,
                ground,
                |lat, lon| {
                    let (_, k) = ground(lat, lon);
                    let m = kinds[k][0].0;
                    (mats[m].albedo_when_turned(turned), m as u16)
                },
            )
        };
        let (summer, autumn) = (build(0.0), build(1.0));
        let l0s = &summer.levels[0];
        let l0a = &autumn.levels[0];
        // A texel turns exactly when its MIXTURE holds grass — including a granite-led texel on
        // the boundary with a grass minority, which is why the dominant material cannot be the
        // test's criterion; the mixture is asked of the integral directly.
        let mut scratch = Moments::new();
        let (mut turned_n, mut same_n) = (0, 0);
        for y in 0..w.h {
            for x in 0..w.w {
                let (lat, lon) = w.centre(x, y);
                let a = integrate_rect_on_sphere(
                    &mut scratch,
                    crate::geo::dir_from_lat_lon(lat, lon),
                    R,
                    w.texel_m(R, y),
                    20.0,
                    16,
                    &mats,
                    &kinds,
                    0.0,
                    ground,
                );
                let has_grass = a.mix.iter().any(|&(m, f)| m == grass && f > 0.0);
                let i = l0s.idx(x, y);
                if has_grass {
                    assert_ne!(
                        l0s.albedo[i], l0a.albedo[i],
                        "texel {i} holds grass and did not turn"
                    );
                    turned_n += 1;
                } else {
                    assert_eq!(
                        l0s.albedo[i], l0a.albedo[i],
                        "texel {i} holds no grass and turned"
                    );
                    same_n += 1;
                }
            }
        }
        assert!(
            turned_n > 0 && same_n > 0,
            "both kinds present: {turned_n} / {same_n}"
        );
    }

    /// **Encoding round-trips within one code.** sRGB keeps the dark channels: foliage's blue
    /// (0.037) comes back within 1% of itself, where 8-bit linear would have quantised it to 11%.
    #[test]
    fn the_encoding_round_trips_within_one_code() {
        for v in [0.0f32, 0.0031, 0.037, 0.095, 0.25, 0.52, 0.9, 1.0] {
            let back = srgb8_to_linear(srgb8(v));
            // One sRGB code is at most ~0.4% of full scale in the darks and ~0.5% relative above.
            assert!(
                (back - v).abs() <= 0.003 || (back - v).abs() / v.max(1e-6) <= 0.012,
                "{v} → {} → {back}",
                srgb8(v)
            );
        }
        assert!((srgb8_to_linear(srgb8(0.037)) - 0.037).abs() / 0.037 < 0.012);
        let mut lev = Level::blank(1, 1);
        lev.albedo[0] = [0.095, 0.137, 0.037];
        lev.slope_var[0] = (0.3f32).tan().powi(2);
        let bytes = encode_rgba_srgb(&lev);
        assert_eq!(bytes.len(), 4);
        let sigma_back = bytes[3] as f32 / 255.0 * SIGMA_FULL_SCALE;
        assert!((sigma_back - 0.3).abs() <= SIGMA_FULL_SCALE / 255.0 * 0.5 + 1e-6);
        lev.material[0] = 7;
        assert_eq!(encode_material(&lev), vec![7]);
        lev.mean_slope[0] = [0.25, -1.5];
        let s = encode_slope(&lev);
        assert_eq!(s.len(), 4);
        let back = |i: usize| f16_to_f32(u16::from_le_bytes([s[i], s[i + 1]]));
        assert_eq!(back(0), 0.25, "a power of two is exact in a half");
        assert_eq!(back(2), -1.5);
        // The half carries eleven significant bits: a slope of 0.1 to within 1e-4, 65,504 at most,
        // signed zero and the subnormals included.
        for v in [
            0.1f32, -0.037, 3.0e-3, 1.0e-5, 6.0e-8, 0.0, -0.0, 7.0e4, -1234.5,
        ] {
            let b = f16_to_f32(f16_bits(v));
            let tol = (v.abs() * 2.0f32.powi(-11)).max(6.0e-8);
            assert!(
                (b - v).abs() <= tol || (v.abs() > 65_504.0 && b.is_infinite()),
                "{v} → {:#06x} → {b}",
                f16_bits(v)
            );
        }
    }

    /// The globe's own window: the shipped raster as the finest of eleven levels over a 2 × 1 base,
    /// its finest texels the raster's own and its coarsest two texels for the planet.
    #[test]
    fn the_globe_is_a_window_of_eleven_levels() {
        let w = Window::globe(2048, 1024);
        assert_eq!(w.sub, 1024);
        assert_eq!((w.base.w, w.base.h), (2, 1));
        assert_eq!(w.levels(), 11);
        let (lat, lon) = w.centre(0, 0);
        assert!((lat - (90.0 - 180.0 / 2048.0)).abs() < 1e-12);
        assert!((lon - (-180.0 + 360.0 / 4096.0)).abs() < 1e-12);
        assert!(w.contains(0.0, 179.9) && w.contains(-89.9, -179.9));
        let top = w.at_level(10);
        assert_eq!((top.w, top.h), (2, 1));
        // A patch window over the same raster at 30 m texels would need eleven levels to reach the
        // globe's grid; it has PATCH_LEVELS, its coarsest texel a whole power of two of the base
        // texel, and its edges lie on that coarsest texel's lines — not on base-texel lines, which
        // would make a 10 km box cost a 19.5 km window.
        let p = Window::covering(
            BaseGrid { w: 2048, h: 1024 },
            53.0,
            53.1,
            -9.5,
            -9.4,
            30.0,
            R,
            1 << 22,
        );
        assert_eq!(p.sub, 1024);
        assert_eq!(p.levels(), PATCH_LEVELS);
        let top = p.at_level(p.levels() - 1);
        assert_eq!(top.sub, 1024 >> (PATCH_LEVELS - 1));
        let grain = 1i64 << (PATCH_LEVELS - 1);
        assert_eq!(p.x0 % grain, 0);
        assert_eq!(p.y0 % grain, 0);
        assert_eq!(p.w as i64 % grain, 0);
        let dpt_p = top.deg_per_texel();
        let dpt_g = w.deg_per_texel();
        assert!((dpt_g[0] / dpt_p[0] - top.sub as f64).abs() < 1e-9);
        // The box asked for, 0.1° across, is held in a window not much larger than itself.
        let box_texels = (0.1 / p.deg_per_texel()[0]).ceil() as u32;
        assert!(
            p.w <= box_texels + 2 * grain as u32,
            "a {box_texels}-texel box cost a {}-texel window",
            p.w
        );
    }

    // ── The incremental builder (docs/46 row 95) ────────────────────────────────────────────────

    /// The fixture's analytic gradient, `(∂h/∂x east, ∂h/∂y north)` in its own local metres, at the
    /// point `(x, y)` — what a finite difference of [`ground`] must converge to.
    fn ground_gradient(x: f64, y: f64) -> (f64, f64) {
        let gx = 0.30
            + 0.003 * x / 1000.0
            + (12.0 / 90.0) * (x / 90.0).cos()
            + (0.8 / 30.0) * (x / 30.0).cos() * (y / 30.0).cos();
        let gy = 0.002 * y / 1000.0 - (0.8 / 30.0) * (x / 30.0).sin() * (y / 30.0).sin();
        (gx, gy)
    }

    /// The fixture's local metres of a point, the inverse of what [`ground`] does with lat/lon.
    fn local_xy(lat: f64, lon: f64) -> (f64, f64) {
        (
            R * (lon + 106.0).to_radians() * 40.0f64.to_radians().cos(),
            R * (lat - 40.0).to_radians(),
        )
    }

    /// A window over the fixture at the fixture's own data step (20 m): the finest texel IS the data,
    /// so one point per texel is the integral and the gradient is a difference of neighbours.
    fn data_step_window(lon_w: f64, lon_e: f64) -> Window {
        Window::covering(
            BaseGrid { w: 8192, h: 4096 },
            39.996,
            40.004,
            lon_w,
            lon_e,
            20.0,
            R,
            1 << 22,
        )
    }

    fn sample_of<'a>(kinds: &'a Kinds, mats: &'a [Material]) -> impl Fn(f64, f64) -> Sample + 'a {
        let point = point_of(kinds, mats);
        move |lat, lon| {
            let (h, _) = ground(lat, lon);
            let (albedo, material) = point(lat, lon);
            Sample {
                height_m: h,
                albedo,
                material,
            }
        }
    }

    fn whole(w: &Window) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: w.w,
            h: w.h,
        }
    }

    /// ★ **The finest texel carries the gradient of the ground under it, and its coarser levels the
    /// variance of that gradient** — with no probe grid anywhere. At the data's step a texel is one
    /// point (`below_the_data_a_texel_is_its_point_sample`), and its slope is the forward difference
    /// to its east and north neighbours: the same estimator `integrate_rect` forms between adjacent
    /// probes, here between adjacent texels, so the lattice of texels IS the probe grid and nothing
    /// is left for a budget to shrink (docs/46 row 95: the budget shrank the probes to one cell and
    /// every texel's roughness read zero). Checked against the fixture's analytic derivative at the
    /// half-step, where a forward difference is second-order accurate.
    #[test]
    fn the_finest_texel_carries_the_gradient_of_the_ground_under_it() {
        let mats = mats();
        let kinds = kinds(&mats);
        let w = data_step_window(-106.004, -105.996);
        assert!(
            w.texel_m(R, 0)[1] <= 20.0,
            "the texel is at the data's step"
        );
        let mut b = Build::new(w, R, point_of(&kinds, &mats));
        b.enqueue(whole(&w));
        let built = b.advance(sample_of(&kinds, &mats), || false);
        assert!(!b.pending());
        assert_eq!(
            built as u32,
            w.w * w.h,
            "every texel of the window was built once"
        );
        let l0 = &b.pyramid.levels[0];
        let point = point_of(&kinds, &mats);
        let mut worst = 0.0f64;
        for y in 0..w.h {
            let size = w.texel_m(R, y);
            for x in 0..w.w {
                let (lat, lon) = w.centre(x, y);
                let i = l0.idx(x, y);
                // Colour and material: the point sample, exactly.
                let (albedo, material) = point(lat, lon);
                assert_eq!(
                    l0.albedo[i], albedo,
                    "texel ({x},{y}) colour is its point sample"
                );
                assert_eq!(l0.material[i], material);
                // One point has seen no variation.
                assert_eq!(l0.slope_var[i], 0.0, "texel ({x},{y}) variance");
                // The gradient: a forward difference, exact to second order at the half-step.
                let (px, py) = local_xy(lat, lon);
                let (gx, _) = ground_gradient(px + size[0] * 0.5, py);
                let (_, gy) = ground_gradient(px, py + size[1] * 0.5);
                let de = (l0.mean_slope[i][0] as f64 - gx).abs();
                let dn = (l0.mean_slope[i][1] as f64 - gy).abs();
                worst = worst.max(de).max(dn);
                assert!(
                    de < 2e-3 && dn < 2e-3,
                    "texel ({x},{y}) slope {:?} vs analytic ({gx:.4}, {gy:.4})",
                    l0.mean_slope[i]
                );
            }
        }
        println!("finest-level gradient: worst error {worst:.2e} against the analytic derivative");
        // Above the data the ground's curvature is a variance, and it is not zero.
        for l in 1..w.levels() as usize {
            let lev = &b.pyramid.levels[l];
            let positive = lev.slope_var.iter().filter(|&&v| v > 0.0).count();
            assert!(
                positive * 10 >= lev.slope_var.len() * 9,
                "level {l}: {positive} of {} texels carry a roughness",
                lev.slope_var.len()
            );
        }
    }

    /// ★ **The patch is the same patch however many slices built it.** Rects in any order, a
    /// deadline every few texels, the ancestors re-coarsened after every slice: the levels that
    /// result are bit-identical to one unbounded build, so a slow machine sees the same picture
    /// later, never a different picture (Law VIII's convergence clause, the one docs/46 row 95 broke).
    #[test]
    fn a_patch_built_in_slices_is_the_patch_built_at_once() {
        let mats = mats();
        let kinds = kinds(&mats);
        let w = data_step_window(-106.004, -105.996);
        let mut whole_build = Build::new(w, R, point_of(&kinds, &mats));
        whole_build.enqueue(whole(&w));
        whole_build.advance(sample_of(&kinds, &mats), || false);

        let mut sliced = Build::new(w, R, point_of(&kinds, &mats));
        // Rects of odd sizes, overlapping, out of order — what a tile patch arriving looks like.
        let rects = [
            Rect {
                x: 7,
                y: 5,
                w: 13,
                h: 9,
            },
            Rect {
                x: 0,
                y: 0,
                w: w.w,
                h: 3,
            },
            Rect {
                x: 20,
                y: 0,
                w: w.w,
                h: w.h,
            }, // runs past the window: clipped
            Rect {
                x: 0,
                y: 3,
                w: 20,
                h: w.h - 3,
            },
            Rect {
                x: 7,
                y: 5,
                w: 13,
                h: 9,
            }, // a second time: rebuilt, same answer
        ];
        for r in rects {
            sliced.enqueue(r);
        }
        let mut slices = 0;
        while sliced.pending() {
            let mut calls = 0;
            sliced.advance(sample_of(&kinds, &mats), || {
                calls += 1;
                calls > 2 // a few chunks per slice: many slices over the window
            });
            slices += 1;
            assert!(slices < 10_000, "the build does not terminate");
        }
        assert!(slices > 10, "the deadline was honoured: {slices} slices");
        for l in 0..w.levels() as usize {
            assert_eq!(
                sliced.pyramid.levels[l], whole_build.pyramid.levels[l],
                "level {l} differs between the sliced and the whole build"
            );
        }
    }

    /// ★ **Before a texel is built, it shows what the globe shows** — the coarsest level's value,
    /// which is the base grid's point sample (the globe's own rule), replicated down. So a window
    /// that has just opened draws exactly what the globe drew there, and detail replaces it texel by
    /// texel as the tiles are integrated; nothing is ever black, nothing is ever read unbuilt.
    #[test]
    fn an_unbuilt_patch_shows_the_coarsest_level_everywhere() {
        let mats = mats();
        let kinds = kinds(&mats);
        let w = data_step_window(-106.004, -105.996);
        let mut b = Build::new(w, R, point_of(&kinds, &mats));
        let top = (w.levels() - 1) as usize;
        let coarsest = b.pyramid.levels[top].clone();
        let coarsest_w = w.at_level(top as u32);
        for l in 0..=top {
            let lev = &b.pyramid.levels[l];
            let lw = w.at_level(l as u32);
            for y in 0..lw.h {
                for x in 0..lw.w {
                    let i = lev.idx(x, y);
                    let (lat, lon) = lw.centre(x, y);
                    let (cx, cy) = b.pyramid.texel_at(top as u32, lat, lon).unwrap();
                    let c = coarsest.idx(cx, cy);
                    assert_eq!(
                        lev.albedo[i], coarsest.albedo[c],
                        "level {l} texel ({x},{y})"
                    );
                    assert_eq!(lev.material[i], coarsest.material[c]);
                    assert_eq!(lev.mean_slope[i], [0.0, 0.0]);
                    assert_eq!(lev.slope_var[i], 0.0);
                }
            }
        }
        assert_eq!(coarsest_w.w * coarsest_w.h, coarsest.albedo.len() as u32);
        // Build one rect: only it, and its ancestors, change; the touched list says exactly that.
        let before = b.pyramid.clone();
        let r = Rect {
            x: 9,
            y: 6,
            w: 5,
            h: 4,
        };
        b.enqueue(r);
        b.advance(sample_of(&kinds, &mats), || false);
        let touched = b.take_touched();
        assert!(
            touched.iter().any(|(l, t)| *l == 0
                && t.x <= r.x
                && t.y <= r.y
                && t.x + t.w >= r.x + r.w
                && t.y + t.h >= r.y + r.h),
            "the touched regions include the rect at level 0: {touched:?}"
        );
        assert_eq!(
            touched.iter().map(|(l, _)| *l).max(),
            Some(top as u32),
            "every ancestor level was touched"
        );
        let l0 = &b.pyramid.levels[0];
        for y in 0..w.h {
            for x in 0..w.w {
                let inside = x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
                let i = l0.idx(x, y);
                if !inside {
                    assert_eq!(
                        l0.albedo[i], before.levels[0].albedo[i],
                        "({x},{y}) outside the rect moved"
                    );
                    assert_eq!(l0.mean_slope[i], before.levels[0].mean_slope[i]);
                }
            }
        }
        assert!(
            b.take_touched().is_empty(),
            "taking the touched list empties it"
        );
    }

    /// ★ **A window that moves adopts the texels it shares with the one before.** The grid is anchored
    /// to the world, so a texel both windows hold has one answer; copying it is not a cache of
    /// anything but the ground, and it means a camera walking across a tile boundary re-integrates
    /// one new row of tiles, not nine.
    #[test]
    fn a_new_window_adopts_the_texels_it_shares() {
        let mats = mats();
        let kinds = kinds(&mats);
        // A window's edges lie on base-grid lines (`covering` aligns to `sub`), so windows move in
        // whole base texels: three base texels wide, then the same box one base texel east.
        let base_deg = 360.0 / 8192.0;
        let a = data_step_window(-106.09, -105.96);
        let mut old = Build::new(a, R, point_of(&kinds, &mats));
        old.enqueue(whole(&a));
        old.advance(sample_of(&kinds, &mats), || false);
        let b = data_step_window(-106.09 + base_deg, -105.96 + base_deg);
        assert_eq!(a.sub, b.sub);
        assert_eq!(
            b.x0,
            a.x0 + a.sub as i64,
            "the box moved one base texel east"
        );
        assert!(b.x0 < a.x0 + a.w as i64, "the windows overlap");
        let mut new = Build::new(b, R, point_of(&kinds, &mats));
        let shared = new.adopt(&old.pyramid).expect("an overlap");
        assert_eq!(
            shared.x, 0,
            "the shared ground starts at the new window's west edge"
        );
        assert_eq!(shared.w as i64, a.x0 + a.w as i64 - b.x0);
        assert_eq!(shared.h, b.h.min(a.h));
        for l in 0..b.levels() as usize {
            let (lo, ln) = (&old.pyramid.levels[l], &new.pyramid.levels[l]);
            let (wo, wn) = (a.at_level(l as u32), b.at_level(l as u32));
            for y in 0..wn.h {
                for x in 0..wn.w {
                    let ax = x as i64 + wn.x0 - wo.x0;
                    let ay = y as i64 + wn.y0 - wo.y0;
                    let i = ln.idx(x, y);
                    if ax >= 0 && ax < wo.w as i64 && ay >= 0 && ay < wo.h as i64 {
                        let j = lo.idx(ax as u32, ay as u32);
                        assert_eq!(
                            ln.albedo[i], lo.albedo[j],
                            "level {l} ({x},{y}) was not adopted"
                        );
                        assert_eq!(ln.mean_slope[i], lo.mean_slope[j]);
                        assert_eq!(ln.slope_var[i], lo.slope_var[j]);
                        assert_eq!(ln.material[i], lo.material[j]);
                    } else if l == 0 {
                        assert_eq!(
                            ln.mean_slope[i],
                            [0.0, 0.0],
                            "level 0 ({x},{y}) outside the overlap is unbuilt"
                        );
                    }
                }
            }
        }
        // A window with the other's sub, or no overlap, adopts nothing.
        let far = data_step_window(-105.50, -105.37);
        let mut none = Build::new(far, R, point_of(&kinds, &mats));
        assert!(none.adopt(&old.pyramid).is_none());
    }

    /// **A box of ground names the texels under it**: the rect covering a latitude/longitude box at
    /// level 0, clipped to the window — how a tile's arrival becomes work.
    #[test]
    fn a_box_of_ground_names_its_texels() {
        let w = data_step_window(-106.004, -105.996);
        let dpt = w.deg_per_texel();
        // A box inside the window: its corners' texels bound the rect.
        let (lat_s, lat_n) = (40.000 - 3.5 * dpt[1], 40.000 + 2.5 * dpt[1]);
        let (lon_w, lon_e) = (-106.000 - 4.5 * dpt[0], -106.000 + 1.5 * dpt[0]);
        let r = w
            .rect_covering_deg(lat_s, lat_n, lon_w, lon_e)
            .expect("inside");
        let (xw, ys) = w.texel_at_level0(lat_s + 1e-9, lon_w + 1e-9);
        let (xe, yn) = w.texel_at_level0(lat_n - 1e-9, lon_e - 1e-9);
        assert_eq!((r.x, r.y), (xw, yn));
        assert_eq!((r.x + r.w - 1, r.y + r.h - 1), (xe, ys));
        // A box larger than the window is the window.
        let all = w.rect_covering_deg(0.0, 80.0, -120.0, -90.0).unwrap();
        assert_eq!((all.x, all.y, all.w, all.h), (0, 0, w.w, w.h));
        // A box beside the window names nothing.
        assert!(w.rect_covering_deg(41.0, 42.0, -106.0, -105.0).is_none());
        assert!(w.rect_covering_deg(39.0, 41.0, -100.0, -99.0).is_none());
    }
}
