// docs/43 Phase 3 — the displaced Earth globe surface. Same uniform layout as space.wgsl, but the fragment
// uses the PER-VERTEX colour (biome albedo, baked into the cube-sphere mesh) instead of a single body tint,
// and adds a cheap view-dependent atmospheric limb (a blue Fresnel rim on the day side) so it reads as a
// blue-marble. `tint` multiplies the vertex colour (so the ocean sphere can be tinted water-blue with a white
// mesh).
//
// CAMERA-RELATIVE-EYE: every position reaching this shader has the eye at the ORIGIN (the convention in
// terra::fly_camera); the cap's vertices are emitted eye-relative in f64, the globe's model matrix carries a
// −eye translation built in f64. So the view direction is simply -wpos, and `emissive.xyz` is free to carry
// the TRIPLANAR ANCHOR: the eye folded modulo the 8 m texture tile, re-added before texture projection so the
// relief stays glued to the surface (an unanchored camera-relative position would drag the texture with the
// camera). Folding keeps it small enough that adding it back costs no precision.

struct U {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    light_dir : vec4<f32>,  // xyz = direction TO the sun, w = twilight half-angle (rad)
    tint      : vec4<f32>,  // multiplies the vertex colour
    emissive  : vec4<f32>,  // xyz = triplanar anchor (the eye mod the 8 m texture tile, display units)
    atm       : vec4<f32>,  // xyz = Rayleigh optical depth per band (docs/26), w = sun gain
    glow      : vec4<f32>,  // rgb = Planck colour of the surface's own temperature, w = its radiance gain
    // KEPT AS PADDING, deliberately: these two carried the crater the vertex shader used to apply
    // (docs/46 row 54), and `SKYLIGHT_UNIFORM_OFFSET` names a byte offset PAST them. Removing them
    // would silently move every field below and nothing would say so.
    _was_crater  : vec4<f32>,
    _was_crater2 : vec4<f32>,
    // ★ WHERE THE EYE STANDS IN THIS BODY'S AIR (docs/66): xyz = its local zenith from the body's
    // centre (unit), w = its altitude above SEA LEVEL in metres. Formed in f64 on the CPU — a camera at
    // head height is 3e-7 of Earth's radius, and reconstructing that from two f32 radii returns noise.
    eye_air   : vec4<f32>,
    // The column's shape: x = scale height (m), y = surface radius (m), z = where the column stops
    // (m above the surface), w = metres per display unit.
    air2      : vec4<f32>,
    // ★★ THE SKY'S OWN LIGHT (docs/46 row 56). `atmosphere::sky_light` projects the hemispherical
    // integral onto two spherical-harmonic bands once per frame on the CPU; here it costs one dot
    // product per band. `sky_ambient.xyz` is the orientation-independent term, `sky_grad_*` the
    // gradient in world axes. All zero for a body with no air, so vacuum lights nothing with no branch.
    sky_ambient : vec4<f32>,
    sky_grad_r  : vec4<f32>,
    sky_grad_g  : vec4<f32>,
    sky_grad_b  : vec4<f32>,
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
    // The RMS slope angle (rad) of the ground this vertex stands for that the MESH IS NOT SHOWING
    // (docs/63). Interpolated, not flat: roughness varies continuously across the ground, and a flat
    // value would draw the mesh's own cell boundaries as shading facets.
    @location(4) rough      : f32,
};

@vertex
fn vs_main(@location(0) pos : vec3<f32>, @location(1) nrm : vec3<f32>, @location(2) col : vec3<f32>,
           @location(3) mat : u32, @location(4) rough : f32) -> VOut {
    var o : VOut;
    // ★★★ THE VERTEX IS DRAWN WHERE THE ENGINE PUT IT (docs/46 row 54). A `crater_sink` function stood
    // here and deformed the surface by a paraboloid excavation profile — real physics, on the wrong
    // side of the seam, and the engine's own ground answer had no crater in it as a result. The bowl is
    // now subtracted by `terra::globe_mesh::SurfaceSampler`, so the surface arrives already excavated
    // and this shader states nothing about what happened to it.
    let world = u.model * vec4<f32>(pos, 1.0);
    o.clip = u.view_proj * world;
    o.wpos = world.xyz;
    o.normal = (u.model * vec4<f32>(nrm, 0.0)).xyz;
    o.col = col;
    o.mat = mat;
    o.rough = rough;
    return o;
}

// One texture tile per 8 metres, expressed in DISPLAY units (Terra's positions are scaled so the planet
// radius is 1). Without the conversion the relief would tile once per 8 planet-radii and be invisible.
const EARTH_RADIUS_M : f32 = 6371000.0;
const GLOBE_TEX_SCALE : f32 = EARTH_RADIUS_M / 8.0;

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    // Relief from the material's own sub-resolution surface statistic (the shared chunk). `i.wpos` is
    // camera-relative (globe AND cap; one convention, so the relief cannot mismatch across the
    // cross-fade); the anchor restores surface-fixed texture coordinates modulo the tile period.
    let n = surface_normal_triplanar(i.wpos + u.emissive.xyz, normalize(i.normal), i.mat, GLOBE_TEX_SCALE);
    let l = normalize(u.light_dir.xyz);
    // Positions are camera-relative (eye at the origin), so the direction back to the eye is -wpos.
    let view = normalize(-i.wpos);
    // **The appearance integral's second moment reaching the light** (docs/63). `i.rough` is the slope
    // spread the mesh could not carry as geometry; at zero this is bit-identical to the `max(dot(n,l),0)`
    // that stood here, so ground the mesh fully resolves looks exactly as it did.
    let ndl = rough_diffuse(n, l, view, i.rough);
    // Reflected sunlight (albedo × illumination), same SUN_GAIN + Reinhard as the space band; black night side.
    let SUN_GAIN = u.atm.w; // atmosphere::SUN_GAIN — one exposure for every view of this world
    // **The material's OWN texture, not a flat biome colour.** `i.col` is the vertex's material albedo —
    // one number for all of granite, all of sand — and it is what this surface wore until now, which is
    // why the ground read as a uniform wash from standing height however much relief was under it. The
    // texture array was bound here the whole time and never sampled.
    //
    // The texture IS this material's albedo, so multiplying by a second albedo would square it. It used
    // to REPLACE `i.col` for that reason — and that quietly threw the vertex colour away entirely.
    //
    // ★★ WHY THAT MATTERED, measured by mutation 2026-08-04: setting every land vertex to MAGENTA
    // changed nothing on screen. The ground is a MIXTURE of materials (a woody savanna is 45% tree,
    // 35% grass, 20% dirt) and it has a SEASON, and both were being computed per vertex and discarded
    // — the shader drew one material's texture and nothing else. It also means a seasonal measurement
    // taken from these frames was measuring the sun's elevation, not the leaves (docs/46 row 41).
    //
    // So `i.col` now arrives as a RATIO — the mixture's seasonal albedo divided by the dominant
    // material's own flat albedo — and the texture supplies the DETAIL about that mean. At a uniform
    // class the ratio is 1 and this is bit-identical to what it replaced; where the class is a mixture
    // the grain is the dominant material's and the colour is everything standing there.
    let grain = surface_albedo_triplanar(i.wpos + u.emissive.xyz, n, i.mat, GLOBE_TEX_SCALE);
    let albedo = grain * i.col * u.tint.rgb;
    var radiance = albedo * (ndl * SUN_GAIN);
    // ★★★ **AND THE SKY LIGHTS IT TOO** (docs/46 row 56). Every surface here received direct sunlight
    // and nothing else, which a horizontal ground hides and a vertical one exposes: a grass blade
    // standing up under a high sun has `ndl` ≈ 0 and rendered BLACK beside ground of the same
    // material. Robin, seeing it: *"one would assume the light scatter from the atmosphere would make
    // the grass green, no?"* It does — this is that light, integrated over the hemisphere the surface
    // can see, from the same `air_inscatter` that draws the sky above it.
    let sky_e = vec3<f32>(
        max(0.0, u.sky_ambient.x + dot(u.sky_grad_r.xyz, n)),
        max(0.0, u.sky_ambient.y + dot(u.sky_grad_g.xyz, n)),
        max(0.0, u.sky_ambient.z + dot(u.sky_grad_b.xyz, n)),
    );
    radiance += albedo * sky_e;
    // **The body's own heat.** A surface hot enough to glow emits regardless of where the Sun is, so this
    // is added on BOTH sides of the terminator — which is the physics: proto-Earth's 1,900 K magma ocean
    // radiates ~547x what a sunlit white surface reflects, so it outshines its own daylight and has no
    // day/night line at all. The colour is Planck's for that temperature and the gain is Stefan-Boltzmann's;
    // neither is chosen, and a cold planet sends zero here and pays nothing.
    radiance += u.glow.rgb * (u.glow.w * SUN_GAIN);
    // ★★ **THE AIR BETWEEN THE EYE AND THIS POINT — the same integral the sky spends** (docs/66).
    //
    // What stood here was the plane-parallel CLOSED FORM scaled by "the fraction of the column below
    // the eye". That fraction was an openly-flagged stand-in, and the sky rig measured what it was
    // worth: at 1.7 m altitude it is 2e-4, so the ground came out BIT-IDENTICAL with and without an
    // atmosphere. Correct for a camera on the grass, and wrong for everything else — no aerial
    // perspective over a mountain thirty kilometres off, and no sunset light on the ground while the
    // sky above it was orange.
    //
    // Marching from the eye to THIS FRAGMENT is the computation that stand-in named. It ends where the
    // ground is, so two metres of air is two metres of air; it lengthens with distance on its own; and
    // it is the same function, at the same exposure, that `sky.wgsl` runs for the pixel next to this
    // one — so the ground and the sky above the horizon cannot draw two different atmospheres.
    var air : AirColumn;
    air.tau = u.atm.xyz;
    air.scale_height = u.air2.x;
    air.radius = u.air2.y;
    air.top = u.air2.z;
    air.sun_gain = u.atm.w;
    // All the geometry as cosines about the EYE's zenith, and the path length in metres. `i.wpos` is
    // camera-relative (eye at the origin), so its length IS the distance to this fragment.
    let up = normalize(u.eye_air.xyz);
    let to_frag = normalize(i.wpos);
    let t_end = length(i.wpos) * u.air2.w;
    let veil = air_inscatter(air, u.eye_air.w, dot(up, to_frag), dot(up, l), dot(to_frag, l), t_end);
    // The ground's own light is dimmed by the air in front of it, and the air's own glow is added —
    // one call gives both, and the reddening of a distant ridge is the same physics as the sunset.
    radiance = radiance * veil.transmit + veil.inscatter;
    let mapped = tonemap(radiance); // the shared display law — compresses brightness, keeps hue
    // Alpha = tint.a: 1.0 for the opaque globe, the cross-fade factor for the ground cap.
    return vec4<f32>(mapped, u.tint.a);
}
