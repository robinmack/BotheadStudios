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
    /// **Tangent-space normal map**, same size and mip chain, RGBA8 with xyz in [0,1] and a=height.
    ///
    /// This is the CHEAP half of surface detail, and the half the engine never had. Relief drawn as
    /// GEOMETRY costs a vertex per feature — 430,080 verts a frame with nothing happening, against the
    /// ~100 texels a crater actually needs. A normal map gives the same close-up appearance for one
    /// texture fetch, so geometry can stay coarse and real texels are spent only where an interaction
    /// is (`docs/44`). Robin, repeatedly: *"we should render texels (which are expensive) ONLY AS
    /// NEEDED for interactions."*
    ///
    /// Derived from the SAME fbm that produces the albedo, so a material's visible grain and its bump
    /// agree by construction rather than being two independent inventions (Law II).
    pub normal_mips: Vec<Vec<u8>>,
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
    // Height field from the same noise as the colour, then its gradient -> a tangent-space normal.
    // Central differences on the torus (the noise is seamless), so the map tiles without a seam.
    let h = |x: usize, y: usize| -> f32 { height_at(material, x, y, size, seed) };
    let mut nrm = vec![0u8; size * size * 4];
    // Bump strength follows the material's CITED optical roughness: a polished surface has a flat
    // normal map, gravel has a violent one. Not an art dial.
    let strength = 2.0 * material.roughness.clamp(0.0, 1.0);
    for y in 0..size {
        for x in 0..size {
            let (xm, xp) = ((x + size - 1) % size, (x + 1) % size);
            let (ym, yp) = ((y + size - 1) % size, (y + 1) % size);
            let dx = (h(xp, y) - h(xm, y)) * strength;
            let dy = (h(x, yp) - h(x, ym)) * strength;
            // n = normalize(-dh/dx, -dh/dy, 1)
            let inv = 1.0 / (dx * dx + dy * dy + 1.0).sqrt();
            let (nx, ny, nz) = (-dx * inv, -dy * inv, inv);
            let i = (y * size + x) * 4;
            nrm[i] = to_u8(nx * 0.5 + 0.5);
            nrm[i + 1] = to_u8(ny * 0.5 + 0.5);
            nrm[i + 2] = to_u8(nz * 0.5 + 0.5);
            nrm[i + 3] = to_u8(h(x, y) * 0.5 + 0.5); // height, for anyone who wants displacement
        }
    }
    Texture {
        size: size as u32,
        mips: build_mips(level0, size),
        normal_mips: build_mips(nrm, size),
    }
}

/// The surface height field a material's relief comes from, in [-1, 1]. Same fbm as the colour, so bump
/// and albedo describe ONE surface.
///
/// **Amplitude comes from ROUGHNESS — the real surface statistic — not from `color_variance`.** Colour
/// variance describes how the material's *colour* varies; roughness describes how its *surface* varies,
/// and it is the latter that has relief. Scaling height by the colour parameter was both wrong in kind
/// and wrong in size: sand's `color_variance` is 0.25 against a roughness of 0.85, so the gradient came
/// out too small to see at all (an 8x amplification of the result was visually indistinguishable).
///
/// **Why this is a model and not a fake** (Law VIII — does it embody the physics or imitate it?): the
/// relief IS there, in the material's grain structure, below any resolution we can afford as geometry.
/// Evaluating light's response to a known sub-resolution surface statistic is what a microfacet model
/// is, and it is Law III's "compute what you can't [simulate]" — not a picture-fixing trick. It stays
/// honest exactly as long as the parameter is the material's own cited roughness and the result would
/// converge to resolved micro-geometry.
fn height_at(material: &Material, x: usize, y: usize, size: usize, seed: u32) -> f32 {
    let (u, v) = (x as f32 / size as f32, y as f32 / size as f32);
    fbm(u, v, seed) * material.roughness.clamp(0.0, 1.0)
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

    /// Relief must scale with ROUGHNESS (the surface statistic), not `color_variance` (a colour one).
    /// Sand has variance 0.25 and roughness 0.85: driving height from the wrong parameter made the
    /// gradient too small to see, and it was the wrong quantity in kind as well as size.
    #[test]
    fn relief_amplitude_follows_roughness_not_colour_variance() {
        let mats = crate::materials::load();
        let pick = |id: &str| mats.iter().find(|m| m.id == id).expect(id).clone();
        // granite: roughness 0.6, variance 0.40  |  sand: roughness 0.85, variance 0.25
        // Roughness says sand is the rougher surface; colour variance would say the opposite.
        let (granite, sand) = (pick("granite"), pick("sand"));
        assert!(sand.roughness > granite.roughness, "premise: sand is the rougher SURFACE");
        assert!(sand.color_variance < granite.color_variance, "premise: but the less varied COLOUR");

        let peak = |m: &crate::materials::Material| -> f32 {
            let t = generate(m);
            let px = &t.normal_mips[0];
            (0..px.len() / 4)
                .map(|i| (px[i * 4 + 3] as f32 / 255.0 * 2.0 - 1.0).abs())
                .fold(0.0f32, f32::max)
        };
        assert!(
            peak(&sand) > peak(&granite),
            "sand must have the greater relief ({} vs {}) — following roughness, not colour variance",
            peak(&sand), peak(&granite)
        );
    }

    /// The point of the normal map: relief WITHOUT geometry. A material's bump must be real (non-flat),
    /// must follow its CITED roughness, and must tile — otherwise the cheap path cannot replace the
    /// expensive one and detail goes back to costing a vertex per feature.
    #[test]
    fn the_normal_map_carries_real_relief_scaled_by_cited_roughness() {
        let mats = crate::materials::load();
        let pick = |id: &str| mats.iter().find(|m| m.id == id).expect(id).clone();
        let (gravel, water) = (pick("gravel"), pick("water"));

        // Mean deviation of the normal's z from 1.0: 0 = perfectly flat, larger = bumpier.
        let bumpiness = |m: &crate::materials::Material| -> f32 {
            let t = generate(m);
            let px = &t.normal_mips[0];
            let n = px.len() / 4;
            let mut acc = 0.0f32;
            for i in 0..n {
                let z = px[i * 4 + 2] as f32 / 255.0 * 2.0 - 1.0;
                acc += (1.0 - z).abs();
            }
            acc / n as f32
        };

        let g = bumpiness(&gravel);
        let w = bumpiness(&water);
        assert!(g > 0.0, "gravel must have a non-flat normal map, got {g}");
        assert!(
            g > w * 2.0,
            "gravel (roughness {}) must be bumpier than water (roughness {}): {g} vs {w}",
            gravel.roughness, water.roughness
        );
    }

    /// It must TILE. A seam is instantly visible on ground you are standing on, and the whole point is
    /// covering a large surface from one small texture.
    #[test]
    fn the_normal_map_is_seamless() {
        let mats = crate::materials::load();
        let sand = mats.iter().find(|m| m.id == "sand").expect("sand");
        let t = generate(sand);
        let px = &t.normal_mips[0];
        let sz = TEX_SIZE;
        let at = |x: usize, y: usize| -> [u8; 3] {
            let i = (y * sz + x) * 4;
            [px[i], px[i + 1], px[i + 2]]
        };
        // Opposite edges must be near-identical: they are neighbours once tiled.
        for y in (0..sz).step_by(37) {
            let (l, r) = (at(0, y), at(sz - 1, y));
            for c in 0..3 {
                assert!(
                    (l[c] as i32 - r[c] as i32).abs() < 40,
                    "vertical seam at y={y}, channel {c}: {} vs {}", l[c], r[c]
                );
            }
        }
    }

    /// Bump and albedo must describe ONE surface — they come from the same fbm, so a bump ridge is where
    /// the colour varies. Two independent noises would look like two overlaid materials (Law II).
    #[test]
    fn bump_and_albedo_describe_the_same_surface() {
        let mats = crate::materials::load();
        let granite = mats.iter().find(|m| m.id == "granite").expect("granite");
        let t = generate(granite);
        let sz = TEX_SIZE;
        // Deviation must be measured against the material's OWN mean brightness, not mid-grey: a dark
        // material's every texel is below 128, so comparing to 128 measures the albedo, not the noise.
        // (That mistake made this test fail while the implementation was correct.)
        let mean: f32 = {
            let px = &t.mips[0];
            let n = px.len() / 4;
            (0..n).map(|i| px[i * 4] as f32).sum::<f32>() / n as f32
        };
        let (mut n, mut agree) = (0, 0);
        for i in (0..sz * sz).step_by(101) {
            let h = t.normal_mips[0][i * 4 + 3] as f32 - 128.0;
            let c = t.mips[0][i * 4] as f32 - mean;
            if h.abs() > 4.0 && c.abs() > 4.0 {
                n += 1;
                if (h > 0.0) == (c > 0.0) { agree += 1; }
            }
        }
        assert!(n > 50, "not enough varying texels to judge ({n})");
        assert!(
            agree as f32 / n as f32 > 0.75,
            "height and albedo disagree on {}% of texels — they are not one surface",
            100 - 100 * agree / n
        );
    }

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
