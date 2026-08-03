// **Triplanar material relief — ONE implementation, concatenated into every surface shader.**
//
// WGSL has no `#include`, so this chunk is prepended in Rust (see the pipeline builders). Copying it
// into each shader is how one answer becomes several that drift (Law II) — and a relief model that
// disagrees between the ground you stand on and the planet you stand on is exactly that failure.
//
// It expects, from the including shader:
//   @binding(1) tex  : texture_2d_array<f32>   the material albedo array
//   @binding(2) samp : sampler
//   @binding(4) ntex : texture_2d_array<f32>   the material NORMAL array (same shape, same mips)
//
// WHY THIS IS PHYSICS AND NOT A TRICK (Law VIII): the relief is really there, in the material's grain
// structure, below any resolution we can afford to carry as geometry. Evaluating light's response to a
// known sub-resolution surface statistic is what a microfacet model is — embodiment under a compute
// bound (Law III), not a picture-fixing cheat. It stays honest only while the amplitude comes from the
// material's own cited roughness (it does — see `texture::height_at`) and would converge to resolved
// micro-geometry as the budget grows.

// **The material's own ALBEDO, blended the same three ways.** This lived as a private `triplanar` in
// `world.wgsl` while its partner — the NORMAL blend below — had already been extracted here, i.e. half a
// law in the shared file and half in one scene's shader. The consequence was visible: Terra's globe and
// ground cap BIND this texture array and never sampled it, so every material on that planet wore a flat
// biome colour with a bump map on top, while the same materials in another scene showed their grain.
//
// It is not decoration. The texture is generated from the material's CITED optical properties (`texture.rs`
// — albedo, `color_variance`, metallic, and the same fbm the normal map's height field comes from), so a
// material's visible grain and its bump agree by construction, and at distance the mip chain averages back
// to exactly the flat albedo it replaced. Detail fades in as the camera closes; nothing is added far away.
fn surface_albedo_triplanar(local : vec3<f32>, n : vec3<f32>, layer : u32, scale : f32) -> vec3<f32> {
    var w = abs(n);
    w = w / (w.x + w.y + w.z);
    let cx = textureSample(tex, samp, local.yz * scale, layer).rgb;
    let cy = textureSample(tex, samp, local.xz * scale, layer).rgb;
    let cz = textureSample(tex, samp, local.xy * scale, layer).rgb;
    return cx * w.x + cy * w.y + cz * w.z;
}

// **The Lambert law INTEGRATED over the sub-resolution slope distribution** — the appearance integral's
// second moment arriving at the light (docs/63, `terra::appearance`).
//
// Returns the replacement for `max(dot(n, l), 0.0)`, so a caller swaps one term and its exposure,
// tonemap and everything downstream are untouched.
//
// WHY THIS IS NOT A NEW BRDF BOLTED ON. A mesh cell covers ground the mesh cannot show: at 94 m
// altitude Terra's cell is ~469 m across while the streamed elevation under it is ~3.7 m per pixel, so
// some sixteen thousand measured samples are averaged into one vertex normal. Averaging normals keeps
// the MEAN and throws away the SPREAD, and the spread is not decoration — it is the geometry that is
// really there. A surface whose facets are tilted about the mean does not reflect like a flat one:
// `<max(dot(n', l), 0)>` over the facet distribution is NOT `max(dot(<n'>, l), 0)`. This evaluates the
// first, where Lambert assumes the second. It is the identical argument the relief blend below already
// makes for material grain, applied one scale up.
//
// **The convergence clause (Law VIII) is exact here, and that is what makes it admissible.** `sigma` is
// the residual the mesh does not carry; resolve the terrain finer and the residual falls, and at
// `sigma = 0` this returns `A = 1, B = 0` — **bit-identical Lambert**. Nothing is added at a coarse
// budget that a finer budget would not remove.
//
// **FLAGGED (Law V), and this is the honest bound:** the closed form is Oren & Nayar's own *qualitative*
// model — an analytic approximation to the true integral over a Gaussian slope distribution with
// masking, shadowing and interreflection. The real computation it defers is that integral (or resolving
// the facets to matter and shading them, which is the same answer at full cost). The approximation's
// derivation assumes moderate angles and its `tan(beta)` term diverges at grazing incidence, where the
// model is outside its own validity — hence the bound on that term, which is a numerical guard on a
// declared approximation, not a dial on a physical quantity.
fn rough_diffuse(n : vec3<f32>, l : vec3<f32>, v : vec3<f32>, sigma : f32) -> f32 {
    let ndl = dot(n, l);
    if (ndl <= 0.0) { return 0.0; }          // no light reaches a face turned away
    if (sigma <= 0.0) { return ndl; }        // the mesh resolves its own surface: exactly Lambert
    let s2 = sigma * sigma;
    let a = 1.0 - 0.5 * s2 / (s2 + 0.33);
    let b = 0.45 * s2 / (s2 + 0.09);
    let ndv = clamp(dot(n, v), -1.0, 1.0);
    let theta_i = acos(clamp(ndl, -1.0, 1.0));
    let theta_r = acos(ndv);
    let alpha = max(theta_i, theta_r);
    let beta = min(theta_i, theta_r);
    // Azimuth between the light and the view, projected into the surface's tangent plane. Where either
    // projection vanishes (a ray straight down the normal) the azimuth is undefined and the term it
    // multiplies is zero anyway, so `normalize`'s degenerate case must not produce a NaN.
    let lt = l - n * ndl;
    let vt = v - n * ndv;
    let ll = length(lt);
    let vl = length(vt);
    var cos_phi = 0.0;
    if (ll > 1e-6 && vl > 1e-6) { cos_phi = dot(lt / ll, vt / vl); }
    // `tan(beta)` diverges as the shallower of the two angles approaches grazing, which is outside the
    // approximation's validity rather than a physical divergence. Bound it at the point where the
    // model's own second term would otherwise exceed its first.
    let t = min(tan(beta), 8.0);
    return ndl * (a + b * max(cos_phi, 0.0) * sin(alpha) * t);
}

fn surface_normal_triplanar(local : vec3<f32>, n : vec3<f32>, layer : u32, scale : f32) -> vec3<f32> {
    var w = abs(n);
    w = w / (w.x + w.y + w.z);
    let tx = textureSample(ntex, samp, local.yz * scale, layer).xyz * 2.0 - 1.0;
    let ty = textureSample(ntex, samp, local.xz * scale, layer).xyz * 2.0 - 1.0;
    let tz = textureSample(ntex, samp, local.xy * scale, layer).xyz * 2.0 - 1.0;
    // Whiteout blend: re-orient each plane's tangent-space normal into world space by swizzling, keeping
    // the geometric normal's sign so a face pointing -x is perturbed like one pointing +x.
    let sn = sign(n);
    let nx = vec3<f32>(tx.z * sn.x, tx.x, tx.y);
    let ny = vec3<f32>(ty.x, ty.z * sn.y, ty.y);
    let nz = vec3<f32>(tz.x, tz.y, tz.z * sn.z);
    return normalize(nx * w.x + ny * w.y + nz * w.z + n);
}
