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
// The material texture arrays (docs/12): albedo for reference, NORMAL for relief lighting. Terra bakes
// per-vertex biome albedo into the mesh, so the colour still comes from `i.col` — what the shader gained
// is the material INDEX, so it can look up that material's real surface relief.
@group(0) @binding(1) var tex : texture_2d_array<f32>;
@group(0) @binding(2) var samp : sampler;
@group(0) @binding(4) var ntex : texture_2d_array<f32>;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) normal     : vec3<f32>,
    @location(1) wpos       : vec3<f32>,
    @location(2) col        : vec3<f32>,
    @location(3) @interpolate(flat) mat : u32,
};

@vertex
fn vs_main(@location(0) pos : vec3<f32>, @location(1) nrm : vec3<f32>, @location(2) col : vec3<f32>,
           @location(3) mat : u32) -> VOut {
    var o : VOut;
    let world = u.model * vec4<f32>(pos, 1.0);
    o.clip = u.view_proj * world;
    o.wpos = world.xyz;
    o.normal = (u.model * vec4<f32>(nrm, 0.0)).xyz;
    o.col = col;
    o.mat = mat;
    return o;
}

// One texture tile per 8 metres, expressed in DISPLAY units (Terra's positions are scaled so the planet
// radius is 1). Without the conversion the relief would tile once per 8 planet-radii and be invisible.
const EARTH_RADIUS_M : f32 = 6371000.0;
const GLOBE_TEX_SCALE : f32 = EARTH_RADIUS_M / 8.0;

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    // Relief from the material's own sub-resolution surface statistic (the shared chunk). `i.wpos` is
    // camera-relative for the cap and world-space for the globe; either way it is a continuous position
    // on the surface, which is what the triplanar projection needs.
    let n = surface_normal_triplanar(i.wpos, normalize(i.normal), i.mat, GLOBE_TEX_SCALE);
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
    // Alpha = tint.a: 1.0 for the opaque globe, the cross-fade factor for the ground cap (which is drawn with
    // alpha blending on top of the globe as the camera descends). `emissive.xyz` is the eye for the globe (world
    // space) and the ORIGIN for the camera-relative cap, so `view` is correct in both.
    return vec4<f32>(mapped, u.tint.a);
}
