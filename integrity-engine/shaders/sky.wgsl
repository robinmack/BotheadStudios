// **Earth's air, drawn where nothing else is** — the sky (docs/66).
//
// Robin, on where a sky belongs (2026-08-04): *"Sky must be a component of Earth assembly."* So there
// is nothing about "sky" in here beyond geometry: every pixel is the light single-scattered into its
// view ray by the DECLARED air of the body the camera is at, computed by the shared `atmos.wgsl` chunk
// — the same function, at the same exposure, that the ground and the blue marble spend. Remove the
// atmosphere from the body definition and this pass draws exactly nothing, with no branch to add.
//
// It is a full-screen ray cast, which is the honest shape for it: a sky IS light scattered along a
// ray, and the same march is what will answer terrain self-shadowing (docs/63's amendment). Drawn
// AFTER the star field and BEFORE the ground: the stars are behind the air, the ground is in front.
//
// ★ FLAGGED: the stars behind it are dimmed by the MEAN of the three bands' transmittance, not each by
// its own, because a scalar alpha is what a single-source blend can carry. The first-order effect —
// stars extinguished near the horizon and washed out in daylight — is right; their reddening is not.
// Per-band would need dual-source blending, which core WebGPU does not have.

struct SkyU {
    inv_view_proj : mat4x4<f32>, // clip → world, to reconstruct the per-pixel view ray
    up            : vec4<f32>,   // xyz = the eye's local zenith (unit, world); w = eye altitude (m)
    sun           : vec4<f32>,   // xyz = direction TO the sun (unit, world); w = the shared exposure
    air           : vec4<f32>,   // xyz = the column's optical depth per band; w = scale height (m)
    body          : vec4<f32>,   // x = surface radius (m), y = column top (m), zw unused
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

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    // Reconstruct the world-space view ray for this pixel by unprojecting near and far clip points.
    let near = u.inv_view_proj * vec4<f32>(i.ndc, 0.0, 1.0);
    let far  = u.inv_view_proj * vec4<f32>(i.ndc, 1.0, 1.0);
    let rd = normalize(far.xyz / far.w - near.xyz / near.w);

    var air : AirColumn;
    air.tau = u.air.xyz;
    air.scale_height = u.air.w;
    air.radius = u.body.x;
    air.top = u.body.y;
    air.sun_gain = u.sun.w;

    // All the geometry the integral needs, as cosines about the EYE's own zenith.
    let up = normalize(u.up.xyz);
    let sun = normalize(u.sun.xyz);
    let s = air_inscatter(air, u.up.w, dot(up, rd), dot(up, sun), dot(rd, sun), NO_LIMIT);

    let mapped = tonemap(s.inscatter); // the shared display law — compresses brightness, keeps hue
    // Alpha carries what the air HID, so the pass adds its own light and dims what is behind it.
    let veiled = 1.0 - (s.transmit.r + s.transmit.g + s.transmit.b) / 3.0;
    return vec4<f32>(mapped, veiled);
}
