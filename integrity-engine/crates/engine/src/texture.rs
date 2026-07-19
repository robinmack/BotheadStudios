//! Procedural texture generation — **emergent from material optical properties**, with **no external
//! image assets**. Every texture is synthesized from the cited `albedo` / `color_variance` /
//! `metallic`, so the engine has zero licensing exposure. Output is high-res (512²) with a full mip
//! chain — mipmapping is the "let the client scale it down" mechanism. A material's *module* could
//! later override this with a user-supplied (e.g. CC0) image (see `docs/06`, `docs/12`).
//!
//! The look is: base albedo, modulated by tileable multi-octave value noise (grain/mottle) scaled by
//! `color_variance`, with mineral flecks, and bright sparkle specks for metals. All deterministic and
//! seamless (the lattice wraps), so textures tile across terrain without visible seams.

use crate::materials::Material;

pub const TEX_SIZE: usize = 512;

/// An RGBA8 (linear) texture with a full mip chain: `mips[0]` is `size × size`, halving to 1×1.
pub struct Texture {
    /// Edge length of mip 0. Read by tests; the uploader uses the `TEX_SIZE` constant.
    #[allow(dead_code)]
    pub size: u32,
    pub mips: Vec<Vec<u8>>,
}

/// Generate a texture for one material from its optical properties.
pub fn generate(material: &Material) -> Texture {
    let size = TEX_SIZE;
    let seed = material_seed(material);
    let mut level0 = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let rgb = sample(material, x, y, size, seed);
            let i = (y * size + x) * 4;
            level0[i] = to_u8(rgb[0]);
            level0[i + 1] = to_u8(rgb[1]);
            level0[i + 2] = to_u8(rgb[2]);
            level0[i + 3] = 255;
        }
    }
    Texture {
        size: size as u32,
        mips: build_mips(level0, size),
    }
}

/// Generate one texture per material (used to fill a GPU texture array).
pub fn generate_all(materials: &[Material]) -> Vec<Texture> {
    materials.iter().map(generate).collect()
}

/// Per-pixel color from the material's properties.
fn sample(material: &Material, x: usize, y: usize, size: usize, seed: u32) -> [f32; 3] {
    let u = x as f32 / size as f32;
    let v = y as f32 / size as f32;

    // Grain/mottle: seamless multi-octave value noise in [-1, 1].
    let n = fbm(u, v, seed);
    // Mineral flecks: a little high-frequency per-pixel spread.
    let fleck = (white(x, y, seed ^ 0x1234_5678) - 0.5) * material.color_variance * 0.5;

    let factor = (1.0 + material.color_variance * n * 0.7 + fleck).clamp(0.2, 1.8);
    let mut rgb = [
        material.albedo[0] * factor,
        material.albedo[1] * factor,
        material.albedo[2] * factor,
    ];

    // Metals: rare bright sparkle specks.
    if material.metallic > 0.5 {
        let s = white(x, y, seed ^ 0x9e37_79b9);
        if s > 0.992 {
            let spark = (s - 0.992) * 60.0;
            rgb[0] += spark;
            rgb[1] += spark;
            rgb[2] += spark;
        }
    }

    [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ]
}

// --- seamless (tileable) value noise ---

fn hash_lattice(ix: i32, iz: i32, cells: i32, seed: u32) -> f32 {
    let x = ix.rem_euclid(cells) as u32; // wrap → seamless tiling
    let z = iz.rem_euclid(cells) as u32;
    let mut h = x
        .wrapping_mul(374_761_393)
        .wrapping_add(z.wrapping_mul(668_265_263))
        .wrapping_add(seed);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Per-pixel white noise in [0, 1).
fn white(x: usize, y: usize, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(2_654_435_761)
        .wrapping_add((y as u32).wrapping_mul(40_503))
        .wrapping_add(seed);
    h ^= h >> 15;
    h = h.wrapping_mul(2_246_822_519);
    ((h ^ (h >> 13)) & 0xffff) as f32 / 65535.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Seamless value noise: `cells` integer cycles across the [0,1) texture, wrapping at the edge.
fn value_noise(u: f32, v: f32, cells: i32, seed: u32) -> f32 {
    let fu = u * cells as f32;
    let fv = v * cells as f32;
    let x0 = fu.floor() as i32;
    let y0 = fv.floor() as i32;
    let tx = smooth(fu - x0 as f32);
    let ty = smooth(fv - y0 as f32);
    let a = hash_lattice(x0, y0, cells, seed);
    let b = hash_lattice(x0 + 1, y0, cells, seed);
    let c = hash_lattice(x0, y0 + 1, cells, seed);
    let d = hash_lattice(x0 + 1, y0 + 1, cells, seed);
    let top = a + (b - a) * tx;
    let bot = c + (d - c) * tx;
    top + (bot - top) * ty
}

/// Four octaves, normalized to roughly [-1, 1].
fn fbm(u: f32, v: f32, seed: u32) -> f32 {
    let mut n = 0.0;
    let mut amp = 0.5;
    let mut cells = 8;
    for _ in 0..4 {
        n += (value_noise(u, v, cells, seed) * 2.0 - 1.0) * amp;
        amp *= 0.5;
        cells *= 2;
    }
    (n / 0.9375).clamp(-1.0, 1.0)
}

fn material_seed(m: &Material) -> u32 {
    m.id.bytes().fold(0x811c_9dc5u32, |a, b| {
        (a ^ b as u32).wrapping_mul(16_777_619)
    })
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Box-filter mip chain down to 1×1. `size` must be a power of two.
fn build_mips(level0: Vec<u8>, size: usize) -> Vec<Vec<u8>> {
    let mut mips = vec![level0];
    let mut cur = size;
    while cur > 1 {
        let src = mips.last().unwrap();
        let next = cur / 2;
        let mut dst = vec![0u8; next * next * 4];
        for y in 0..next {
            for x in 0..next {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = x * 2 + dx;
                        let sy = y * 2 + dy;
                        sum += src[(sy * cur + sx) * 4 + c] as u32;
                    }
                    dst[(y * next + x) * 4 + c] = (sum / 4) as u8;
                }
            }
        }
        mips.push(dst);
        cur = next;
    }
    mips
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;

    fn mean_rgb(tex: &Texture) -> [f32; 3] {
        let px = &tex.mips[0];
        let n = (px.len() / 4) as f32;
        let mut s = [0.0f32; 3];
        for chunk in px.chunks_exact(4) {
            s[0] += chunk[0] as f32;
            s[1] += chunk[1] as f32;
            s[2] += chunk[2] as f32;
        }
        [s[0] / n / 255.0, s[1] / n / 255.0, s[2] / n / 255.0]
    }

    #[test]
    fn texture_has_size_and_full_mip_chain() {
        let mats = materials::load();
        let tex = generate(&mats[materials::index_of(&mats, "granite")]);
        assert_eq!(tex.size as usize, TEX_SIZE);
        assert_eq!(tex.mips[0].len(), TEX_SIZE * TEX_SIZE * 4);
        // 512 -> 256 -> ... -> 1 is 10 levels.
        assert_eq!(tex.mips.len(), (TEX_SIZE as f32).log2() as usize + 1);
        assert_eq!(tex.mips.last().unwrap().len(), 4, "last mip is 1x1 RGBA");
    }

    #[test]
    fn mean_color_tracks_albedo() {
        let mats = materials::load();
        for id in ["granite", "dirt", "grass"] {
            let m = &mats[materials::index_of(&mats, id)];
            let mean = mean_rgb(&generate(m));
            for (c, &mc) in mean.iter().enumerate() {
                assert!(
                    (mc - m.albedo[c]).abs() < 0.12,
                    "{id} channel {c}: texture mean {mc} vs albedo {}",
                    m.albedo[c]
                );
            }
        }
    }

    #[test]
    fn different_materials_look_different() {
        let mats = materials::load();
        let g = mean_rgb(&generate(&mats[materials::index_of(&mats, "granite")]));
        let gr = mean_rgb(&generate(&mats[materials::index_of(&mats, "grass")]));
        let diff = (g[0] - gr[0]).abs() + (g[1] - gr[1]).abs() + (g[2] - gr[2]).abs();
        assert!(diff > 0.1, "granite and grass textures should differ");
    }

    #[test]
    fn texture_has_variation() {
        // A material with color_variance > 0 must not be a flat color.
        let mats = materials::load();
        let tex = generate(&mats[materials::index_of(&mats, "granite")]);
        let px = &tex.mips[0];
        let lum = |c: &[u8]| 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
        let vals: Vec<f32> = px.chunks_exact(4).map(lum).collect();
        let mean = vals.iter().sum::<f32>() / vals.len() as f32;
        let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32;
        assert!(
            var > 1.0,
            "granite texture should have luminance variation (var={var})"
        );
    }
}
