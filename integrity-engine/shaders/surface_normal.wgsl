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
