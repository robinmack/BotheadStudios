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
//! tiles' resolution, each texel INTEGRATED over the measured ground inside it; its coarsest level is
//! the base grid again, so the two meet on the same footprints. Both are built by the model's one
//! surface rule (`Terra::surface_at`, docs/46 row 93) through the probe a caller supplies; this module
//! owns no datum and samples no raster itself.
//!
//! ★ The grid is anchored to the WORLD, not to the window. A window moving with the camera slides by
//! whole texels over a grid that does not move (`the_texel_grid_is_anchored_to_the_world`), so a texel
//! has one answer however the camera approaches it. That is the whole difference from the mesh.

use super::appearance::{integrate_rect_on_sphere, Kinds, Moments};
use crate::materials::Material;

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
}

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
        }
    }

    /// **The window covering a latitude/longitude box at texels no coarser than `target_m`**, with
    /// at most `max_texels` in it. The subdivision is the smallest power of two that brings the base
    /// texel's north–south side under the target; if the box then needs more texels than allowed the
    /// subdivision is halved until it fits, so a budget coarsens the window rather than cropping it
    /// (the same rule the integral's probe cap follows). Longitudes may be given across the dateline
    /// (`lon_w > lon_e`).
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
            let dpt = [
                360.0 / (base.w as f64 * sub as f64),
                180.0 / (base.h as f64 * sub as f64),
            ];
            let align = |v: f64, up: bool| -> i64 {
                let t = if up { v.ceil() } else { v.floor() } as i64;
                let s = sub as i64;
                if up {
                    (t + s - 1).div_euclid(s) * s
                } else {
                    t.div_euclid(s) * s
                }
            };
            let x0 = align((lon_w + 180.0) / dpt[0], false);
            let x1 = align((lon_e + 180.0) / dpt[0], true).max(x0 + sub as i64);
            let y0 = align((90.0 - lat_n) / dpt[1], false).max(0);
            let y1 = align((90.0 - lat_s) / dpt[1], true)
                .max(y0 + sub as i64)
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
                };
            }
            sub /= 2;
        }
    }

    /// How many levels this window has: down to the base grid.
    pub fn levels(&self) -> u32 {
        self.sub.trailing_zeros() + 1
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
            let area = |yy: u32| {
                let t = fine_window.texel_m(radius_m, yy);
                t[0] * t[1]
            };
            let (a_n, a_s) = (area(2 * y), area(2 * y + 1));
            for x in 0..w {
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
        }
        out
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
    let mut out = Vec::with_capacity(level.albedo.len() * 4);
    for (i, a) in level.albedo.iter().enumerate() {
        out.push(srgb8(a[0]));
        out.push(srgb8(a[1]));
        out.push(srgb8(a[2]));
        let s = (level.sigma_rad(i) / SIGMA_FULL_SCALE * 255.0).round();
        out.push(s.clamp(0.0, 255.0) as u8);
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
    let mut out = Vec::with_capacity(level.mean_slope.len() * 4);
    for s in &level.mean_slope {
        for k in 0..2 {
            out.extend_from_slice(&f16_bits(s[k]).to_le_bytes());
        }
    }
    out
}

/// A level's materials as the single-byte texels an `R8Uint` texture takes. The catalogue has a few
/// dozen materials; an index past 255 would be a catalogue of a different shape, and is reported
/// rather than wrapped.
pub fn encode_material(level: &Level) -> Vec<u8> {
    level
        .material
        .iter()
        .map(|&m| u8::try_from(m).expect("a material index that fits the R8 texture"))
        .collect()
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
    /// are whole multiples, and a coarsest level whose texels are the base grid's own.
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
        // A patch window over the same raster meets the globe on its finest texels: the patch's
        // coarsest level has the globe's level-0 texel size.
        let p = Window::covering(
            BaseGrid { w: 2048, h: 1024 },
            53.0,
            53.1,
            -9.5,
            -9.4,
            30.0,
            R,
            1 << 16,
        );
        let dpt_p = p.at_level(p.levels() - 1).deg_per_texel();
        let dpt_g = w.deg_per_texel();
        assert!((dpt_p[0] - dpt_g[0]).abs() < 1e-12 && (dpt_p[1] - dpt_g[1]).abs() < 1e-12);
    }
}
