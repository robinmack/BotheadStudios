// Honest Rayleigh-scattered SKY for the terrain scene. NOT a painted gradient: every pixel's colour is
// the single-scatter sky radiance along its view ray, computed from the SAME Chandrasekhar slab law as
// the space-band blue marble (atmosphere::rayleigh_veil). Deep blue overhead (short air path → the
// λ⁻⁴ blue dominates), pale/whiter toward the horizon (long slant path saturates every band), brighter
// toward the sun (the phase function's forward lobe — it falls out, never faked). Remove the declared
// atmosphere (τ → 0) and the sky goes black, exactly like the airless Moon.
//
// HONESTY FLAGS (identical to the space band's): SINGLE scatter only — no multiple scattering, no
// Mie/aerosol haze, no ozone. Flat-slab slant path (no Chapman function). Night side (sun below the
// horizon) is genuinely black — no twilight, no starlight fill.

struct SkyU {
    inv_view_proj : mat4x4<f32>, // clip → world, to reconstruct the per-pixel view ray
    sun_dir       : vec4<f32>,   // xyz = direction TO the sun (world, normalized) — the terrain's light
    tau           : vec4<f32>,   // xyz = Rayleigh optical depth per band (R/G/B), w = sun gain
    camera_pos    : vec4<f32>,   // xyz = eye (world)
};

@group(0) @binding(0) var<uniform> u : SkyU;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) ndc        : vec2<f32>, // this pixel's normalized-device coords, for ray reconstruction
};

// A single oversized triangle covering the whole screen (no vertex buffer): the classic fullscreen tri.
@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VOut {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var o : VOut;
    o.ndc = p[vi];
    o.clip = vec4<f32>(p[vi], 0.0, 1.0);
    return o;
}

// The SAME first-order Chandrasekhar slab single-scatter as atmosphere::rayleigh_veil (Rust), mirrored
// verbatim: L = F·P(Θ)/(4(μᵥ+μₛ))·μₛ·(1 − e^{−τ(1/μᵥ+1/μₛ)}), Rayleigh phase P(Θ) = ¾(1+cos²Θ).
// μᵥ is the view cosine from the zenith (short path overhead, long path at the horizon); μₛ the sun's.


@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    // Reconstruct the world-space view ray for this pixel by unprojecting near and far clip points.
    let near = u.inv_view_proj * vec4<f32>(i.ndc, 0.0, 1.0);
    let far  = u.inv_view_proj * vec4<f32>(i.ndc, 1.0, 1.0);
    let rd = normalize(far.xyz / far.w - near.xyz / near.w);

    let sun = normalize(u.sun_dir.xyz);
    let mu_v = rd.y;            // cosine from the zenith: 1 overhead, →0 at the horizon (long air path)
    let mu_s = sun.y;           // the sun's elevation cosine
    let cos_theta = dot(rd, sun); // forward-scatter angle: brightens toward the sun via the phase lobe

    let radiance = rayleigh_veil(mu_v, mu_s, cos_theta, u.tau.xyz, u.tau.w, u.sun_dir.w);
    let mapped = radiance / (vec3<f32>(1.0) + radiance); // Reinhard tone-map (same as the space band)
    return vec4<f32>(mapped, 1.0);
}
