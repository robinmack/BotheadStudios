//! docs/43 — equirectangular raster sampling for the Terra surface. The Earth "world" ships three baked rasters
//! (land mask, elevation+bathymetry RGB-packed 16-bit, land-cover biome index — see `tools/bake-earth`). The JS
//! host decodes each PNG to raw RGBA and hands the bytes here; this samples them by (lat, lon). Pure logic
//! (compiles native + wasm), unit-tested. Convention: row 0 = +90°N (north), column 0 = −180°W, lon wraps.

/// A decoded equirectangular raster: `chans` interleaved u8 per texel.
#[derive(Clone)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub chans: usize,
    pub data: Vec<u8>,
}

impl Raster {
    pub fn new(w: usize, h: usize, chans: usize, data: Vec<u8>) -> Result<Raster, String> {
        if w == 0 || h == 0 || chans == 0 || data.len() != w * h * chans {
            return Err(format!(
                "raster {w}x{h}x{chans} needs {} bytes, got {}",
                w * h * chans,
                data.len()
            ));
        }
        Ok(Raster { w, h, chans, data })
    }

    #[inline]
    fn at(&self, x: usize, y: usize, ch: usize) -> u8 {
        self.data[(y * self.w + x) * self.chans + ch]
    }

    /// (lat,lon)° → fractional texel coords: fx in [0,w) (lon wrapped), fy in [0,h) (lat clamped).
    #[inline]
    fn coords(&self, lat: f64, lon: f64) -> (f64, f64) {
        let mut u = (lon + 180.0) / 360.0;
        u -= u.floor(); // wrap into [0,1)
        let v = ((90.0 - lat) / 180.0).clamp(0.0, 1.0);
        (u * self.w as f64, v * self.h as f64)
    }

    #[inline]
    fn wrap_x(&self, x: isize) -> usize {
        (((x % self.w as isize) + self.w as isize) % self.w as isize) as usize
    }
    #[inline]
    fn clamp_y(&self, y: isize) -> usize {
        y.clamp(0, self.h as isize - 1) as usize
    }

    /// Nearest-texel channel value — for CATEGORICAL data (land mask, biome index; never interpolate a class).
    fn nearest(&self, lat: f64, lon: f64, ch: usize) -> u8 {
        let (fx, fy) = self.coords(lat, lon);
        let x = self.wrap_x(fx.floor() as isize);
        let y = self.clamp_y(fy.floor() as isize);
        self.at(x, y, ch)
    }

    /// The 4 surrounding texels + bilinear weights (lon-wrapping in x, clamping in y).
    #[inline]
    fn bilin(&self, lat: f64, lon: f64) -> ([usize; 2], [usize; 2], f64, f64) {
        let (fx, fy) = self.coords(lat, lon);
        let (x0, y0) = (fx.floor() as isize, fy.floor() as isize);
        (
            [self.wrap_x(x0), self.wrap_x(x0 + 1)],
            [self.clamp_y(y0), self.clamp_y(y0 + 1)],
            fx - x0 as f64,
            fy - y0 as f64,
        )
    }

    /// Bilinear single-channel value (0..255) — for CONTINUOUS 8-bit data.
    #[allow(dead_code)]
    fn bilinear(&self, lat: f64, lon: f64, ch: usize) -> f64 {
        let (xs, ys, tx, ty) = self.bilin(lat, lon);
        let s = |x: usize, y: usize| self.at(x, y, ch) as f64;
        let top = s(xs[0], ys[0]) * (1.0 - tx) + s(xs[1], ys[0]) * tx;
        let bot = s(xs[0], ys[1]) * (1.0 - tx) + s(xs[1], ys[1]) * tx;
        top * (1.0 - ty) + bot * ty
    }

    /// `true` where the land mask marks land (channel 0 > 127).
    pub fn land_at(&self, lat: f64, lon: f64) -> bool {
        self.nearest(lat, lon, 0) > 127
    }

    /// The biome index (land-cover channel 0) at (lat,lon) — nearest, no interpolation across classes.
    pub fn biome_at(&self, lat: f64, lon: f64) -> u8 {
        self.nearest(lat, lon, 0)
    }

    /// Elevation (metres) from an RGB-packed 16-bit raster: value = (R<<8 | G)/65535 mapped onto [lo, hi].
    /// Bilinear over the RECONSTRUCTED 16-bit value at the 4 corners (packing bytes can't be lerped directly).
    pub fn elevation_m_at(&self, lat: f64, lon: f64, lo: f64, hi: f64) -> f64 {
        let (xs, ys, tx, ty) = self.bilin(lat, lon);
        let v16 = |x: usize, y: usize| ((self.at(x, y, 0) as u32) << 8 | self.at(x, y, 1) as u32) as f64;
        let top = v16(xs[0], ys[0]) * (1.0 - tx) + v16(xs[1], ys[0]) * tx;
        let bot = v16(xs[0], ys[1]) * (1.0 - tx) + v16(xs[1], ys[1]) * tx;
        let v = (top * (1.0 - ty) + bot * ty) / 65535.0;
        lo + v * (hi - lo)
    }

    /// Fraction of texels marked land — a bake sanity check (real Earth ≈ 0.29).
    pub fn land_fraction(&self) -> f64 {
        let n = self.w * self.h;
        if n == 0 {
            return 0.0;
        }
        let land = (0..n).filter(|&i| self.data[i * self.chans] > 127).count();
        land as f64 / n as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×2 single-channel raster; corners let us pin the (lat,lon)→texel mapping.
    fn grid4x2() -> Raster {
        // rows: y0 = north (+90..0), y1 = south (0..−90); cols span −180..+180 in 4 columns of 90° each.
        // value = col index * 10 + row index, so each texel is identifiable.
        let mut d = Vec::new();
        for y in 0..2 {
            for x in 0..4 {
                d.push((x as u8) * 10 + y as u8);
            }
        }
        Raster::new(4, 2, 1, d).unwrap()
    }

    #[test]
    fn nearest_maps_lat_lon_to_the_right_texel() {
        let r = grid4x2();
        // lon −180 → col 0; north (lat > 0) → row 0 → value 0.
        assert_eq!(r.nearest(45.0, -179.9, 0), 0);
        // lon just under +180 → col 3; south (lat < 0) → row 1 → value 31.
        assert_eq!(r.nearest(-45.0, 179.9, 0), 31);
        // lon 0 → the boundary at col 2 (−180..+180 over 4 cols: col 2 starts at 0°).
        assert_eq!(r.nearest(10.0, 1.0, 0), 20);
    }

    #[test]
    fn longitude_wraps_at_the_seam() {
        let r = grid4x2();
        // +185° wraps to −175° → col 0 (same as −179.9).
        assert_eq!(r.nearest(45.0, 185.0, 0), r.nearest(45.0, -175.0, 0));
        // −180 and +180 land on the same wrapped column.
        assert_eq!(r.nearest(0.0, -180.0, 0), r.nearest(0.0, 180.0, 0));
    }

    #[test]
    fn land_mask_and_fraction() {
        // 2×1 raster: left texel land (255), right ocean (0) → land fraction 0.5.
        let r = Raster::new(2, 1, 1, vec![255, 0]).unwrap();
        assert!(r.land_at(0.0, -90.0)); // lon −90 → col 0 = land
        assert!(!r.land_at(0.0, 90.0)); // lon +90 → col 1 = ocean
        assert!((r.land_fraction() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn elevation_decodes_and_interpolates() {
        // 2×1 RGB raster. Left texel packs 0 (→ lo), right packs 65535 (→ hi). Range [−11000, 9000].
        let (lo, hi) = (-11000.0, 9000.0);
        let data = vec![
            0, 0, 0, // left: (0<<8|0)=0
            255, 255, 0, // right: (255<<8|255)=65535
        ];
        let r = Raster::new(2, 1, 3, data).unwrap();
        // lon −180 → texel-x 0.0 (col 0) → lo; lon 0 → texel-x 1.0 (col 1) → hi.
        assert!((r.elevation_m_at(0.0, -180.0, lo, hi) - lo).abs() < 1.0);
        assert!((r.elevation_m_at(0.0, 0.0, lo, hi) - hi).abs() < 1.0);
        // lon −90 → texel-x 0.5 (halfway between the two columns) → ~mid range (bilinear).
        let mid = r.elevation_m_at(0.0, -90.0, lo, hi);
        assert!(mid > lo + 0.4 * (hi - lo) && mid < lo + 0.6 * (hi - lo), "mid was {mid}");
    }
}
