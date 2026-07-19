// docs/43 Phase 3 — the displaced Earth globe surface. Same uniform layout as space.wgsl, but the fragment
// uses the PER-VERTEX colour (biome albedo, baked into the cube-sphere mesh) instead of a single body tint,
// and adds a cheap view-dependent atmospheric limb (a blue Fresnel rim on the day side) so it reads as a
// blue-marble. `tint` multiplies the vertex colour (so the ocean sphere can be tinted water-blue with a white
// mesh); `emissive.xyz` carries the camera eye (display units) and `emissive.w` the atmosphere strength.

struct U {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    light_dir : vec4<f32>,  // xyz = direction TO the sun
    tint      : vec4<f32>,  // multiplies the vertex colour
    emissive  : vec4<f32>,  // xyz = camera eye (display units), w = atmosphere strength
};
@group(0) @binding(0) var<uniform> u : U;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) normal     : vec3<f32>,
    @location(1) wpos       : vec3<f32>,
    @location(2) col        : vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos : vec3<f32>, @location(1) nrm : vec3<f32>, @location(2) col : vec3<f32>) -> VOut {
    var o : VOut;
    let world = u.model * vec4<f32>(pos, 1.0);
    o.clip = u.view_proj * world;
    o.wpos = world.xyz;
    o.normal = (u.model * vec4<f32>(nrm, 0.0)).xyz;
    o.col = col;
    return o;
}

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light_dir.xyz);
    let ndl = max(dot(n, l), 0.0);
    // Reflected sunlight (albedo × illumination), same SUN_GAIN + Reinhard as the space band; black night side.
    let SUN_GAIN = 22.0;
    let albedo = i.col * u.tint.rgb;
    var radiance = albedo * (ndl * SUN_GAIN);
    // Atmospheric limb: a soft blue rim where the surface faces away from the camera (grazing angle), on the
    // lit side — a cheap stand-in for the Rayleigh limb (the full per-vertex Rayleigh integral is a refinement).
    let view = normalize(u.emissive.xyz - i.wpos);
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0);
    radiance += vec3<f32>(0.35, 0.55, 1.0) * (rim * u.emissive.w * (0.15 + ndl));
    let mapped = radiance / (vec3<f32>(1.0) + radiance); // Reinhard tone-map
    return vec4<f32>(mapped, 1.0);
}
