// docs/43 Phase 3 — the displaced Earth globe surface. Same uniform layout as space.wgsl, but the fragment
// uses the PER-VERTEX colour (biome albedo, baked into the cube-sphere mesh) instead of a single body tint,
// and adds a cheap view-dependent atmospheric limb (a blue Fresnel rim on the day side) so it reads as a
// blue-marble. `tint` multiplies the vertex colour (so the ocean sphere can be tinted water-blue with a white
// mesh); `emissive.xyz` carries the camera eye (display units) and `emissive.w` the atmosphere strength.

struct U {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    light_dir : vec4<f32>,  // xyz = direction TO the sun, w = twilight half-angle (rad)
    tint      : vec4<f32>,  // multiplies the vertex colour
    emissive  : vec4<f32>,  // xyz = camera eye (display units)
    atm       : vec4<f32>,  // xyz = Rayleigh optical depth per band (docs/26), w = sun gain
    glow      : vec4<f32>,  // rgb = Planck colour of the surface's own temperature, w = its radiance gain
    // THE CRATER (docs/39 surface hook, docs/46 row 18). xyz = bowl axis (unit, MODEL space), w = angular
    // radius (rad). `crater2.x` = depth as a fraction of the surface radius, measured from excavated mass.
    // w == 0 ⇒ no crater and the vertex is untouched, so an unstruck globe costs nothing.
    crater    : vec4<f32>,
    crater2   : vec4<f32>,
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

// The bowl. A simple crater's profile is a PARABOLOID, so the vertex sinks by depth·(1−(θ/θr)²) inside the
// opening angle and is untouched outside it. This is the render REPORTING an excavation the sim performed —
// the depth is measured from the mass actually lifted off the surface (Law VI: physics drives the render).
// Before this, a cap impact drew the target as a flawless sphere with coherence pinned to 1.0, so a crater
// could never appear however real it was — the bug Robin reported repeatedly (docs/46 row 18).
fn crater_sink(dir : vec3<f32>) -> f32 {
    let theta_r = u.crater.w;
    if (theta_r <= 0.0) { return 0.0; }
    let c = clamp(dot(normalize(dir), normalize(u.crater.xyz)), -1.0, 1.0);
    let theta = acos(c);
    if (theta >= theta_r) { return 0.0; }
    let t = theta / theta_r;
    return u.crater2.x * (1.0 - t * t);
}

@vertex
fn vs_main(@location(0) pos : vec3<f32>, @location(1) nrm : vec3<f32>, @location(2) col : vec3<f32>,
           @location(3) mat : u32) -> VOut {
    var o : VOut;
    // Sink the surface into the bowl. `pos` is a unit-sphere position (the model matrix carries the real
    // radius and the oblateness), so a fractional depth scales the vertex straight along its own radius.
    let sunk = pos * (1.0 - crater_sink(pos));
    let world = u.model * vec4<f32>(sunk, 1.0);
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
    let SUN_GAIN = u.atm.w; // atmosphere::SUN_GAIN — one exposure for every view of this world
    let albedo = i.col * u.tint.rgb;
    var radiance = albedo * (ndl * SUN_GAIN);
    // **The body's own heat.** A surface hot enough to glow emits regardless of where the Sun is, so this
    // is added on BOTH sides of the terminator — which is the physics: proto-Earth's 1,900 K magma ocean
    // radiates ~547x what a sunlit white surface reflects, so it outshines its own daylight and has no
    // day/night line at all. The colour is Planck's for that temperature and the gain is Stefan-Boltzmann's;
    // neither is chosen, and a cold planet sends zero here and pays nothing.
    radiance += u.glow.rgb * (u.glow.w * SUN_GAIN);
    // **The atmosphere — Earth's own air, from the ONE Rayleigh model (the shared chunk).** For a point
    // on the globe the local zenith IS its surface normal, so the sky's own angles apply unchanged:
    // mu_v = n·view, mu_s = n·sun, phase = view·sun. What this replaces was a Fresnel rim that could
    // not soften the terminator or redden a sunset, because a rim highlight is not scattering.
    //
    // There is no "atmosphere strength" dial any more: the brightness is whatever the declared air's
    // optical depth scatters at the shared exposure. A body with no declared atmosphere carries tau = 0
    // and gets exactly nothing — the airless case needs no branch.
    let view = normalize(u.emissive.xyz - i.wpos);
    radiance += rayleigh_veil(dot(n, view), dot(n, l), dot(view, l), u.atm.xyz, u.atm.w, u.light_dir.w);
    let mapped = tonemap(radiance); // the shared display law — compresses brightness, keeps hue
    // Alpha = tint.a: 1.0 for the opaque globe, the cross-fade factor for the ground cap. `emissive.xyz`
    // is the eye for the globe (world space) and the ORIGIN for the camera-relative cap, so `view` holds
    // in both.
    return vec4<f32>(mapped, u.tint.a);
}
