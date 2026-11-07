// docs/43 Phase 3 — the displaced Earth globe surface. Same uniform layout as space.wgsl, but the fragment
// uses the PER-VERTEX colour (biome albedo, baked into the cube-sphere mesh) instead of a single body tint,
// and adds a cheap view-dependent atmospheric limb (a blue Fresnel rim on the day side) so it reads as a
// blue-marble. `tint` multiplies the vertex colour (so the ocean sphere can be tinted water-blue with a white
// mesh).
//
// ★ TWO FRAGMENT ENTRIES, ONE SHADING (docs/73 §2a). `fs_main` shades a mesh whose vertices carry their
// own colour, material and roughness — the plants, the gun, the space band's bodies. `fs_ground` shades
// Terra's ground, whose mesh carries SHAPE ONLY: its colour, roughness and grain layer are read per
// fragment from the appearance pyramid (`terra::pyramid`), at the footprint this pixel actually sees,
// anchored to the world. Both end in `shade`, so the two cannot light the same surface two ways.
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

// ★★★ THE APPEARANCE PYRAMID (docs/73 §2a, `terra::pyramid`) — bound only to the GROUND pipeline.
// Two world-anchored equirectangular pyramids: the GLOBE (the shipped rasters' own grid, one point per
// texel, eleven levels to two texels for the planet) and the PATCH (a window over the streamed
// elevation tiles, every texel the appearance integral over the measured ground inside it, its
// coarsest level the globe's finest). Each holds the footprint's albedo in season (sRGB, filtered in
// linear light) with the RMS slope angle the mesh is not showing in alpha, and the dominant material —
// the grain layer — as an integer texture read without filtering.
struct Appear {
    eye    : vec4<f32>,  // xyz = the eye from the body's centre (display units), w = metres per display unit
    globe  : vec4<f32>,  // x = texels per degree of longitude, y = per degree of latitude, z = levels, w = radius (m)
    window : vec4<f32>,  // x = west edge (deg), y = north edge (deg), z = texels per degree lon, w = per degree lat
    window2: vec4<f32>,  // x = width (texels), y = height, z = levels, w = 1 when a patch is held
    scale  : vec4<f32>,  // x = the relief exaggeration the mesh is drawn with
};
@group(1) @binding(0) var<uniform> ap : Appear;
@group(1) @binding(1) var g_look : texture_2d<f32>;
@group(1) @binding(2) var g_mat  : texture_2d<u32>;
@group(1) @binding(3) var p_look : texture_2d<f32>;
@group(1) @binding(4) var p_mat  : texture_2d<u32>;
// The globe's sampler magnifies NEAREST: a base texel is one land-cover class, categorical data, and
// a ramp invented between two classes would be a gradient the data does not contain. Both samplers
// minify linearly through the levels — that is the integral, which the levels were built to be.
@group(1) @binding(5) var samp_g : sampler;
@group(1) @binding(6) var samp_p : sampler;
// The mean gradient (east, north) over the texel, as a tangent in half floats: the slope the
// footprint's shape has at this level, which is what the mesh's normal stood for.
@group(1) @binding(7) var g_slope : texture_2d<f32>;
@group(1) @binding(8) var p_slope : texture_2d<f32>;

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
const DEG : f32 = 0.017453292519943295;
const HALF_PI : f32 = 1.5707963267948966;

// What the ground looks like at one fragment: the pyramid read at this pixel's own footprint.
struct GroundLook {
    albedo : vec3<f32>,
    sigma  : f32,
    mat    : u32,
    // The footprint's mean gradient (east, north), tangent, at the level read.
    slope  : vec2<f32>,
    // The texel read, metres on the ground (its north–south side): the pyramid's band limit here.
    texel_m : f32,
};

// The material texel under `uv` at level `l` of an integer pyramid — unfiltered, as a material index
// must be.
fn material_at(t : texture_2d<u32>, uv : vec2<f32>, l : i32) -> u32 {
    let dims = vec2<i32>(textureDimensions(t, l));
    let p = clamp(vec2<i32>(floor(uv * vec2<f32>(dims))), vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(t, p, l).r;
}

// ★★★ **THE PYRAMID, READ AT THE FOOTPRINT THIS PIXEL SEES** (docs/73 §2a). The fragment recovers
// where on the body it is from its camera-relative position and the eye; how much ground it stands
// for from its own screen-space derivatives (the same quantity `render::Fidelity` derives from the
// viewport, here per pixel and for free); and reads the level whose texels are that size. The patch
// answers where it covers the point and is fine enough to be asked; the globe everywhere else. The
// mesh decides nothing about colour, so there is nothing of the mesh in the picture to move with it.
fn ground_look(wpos : vec3<f32>) -> GroundLook {
    // Derivatives first, in uniform control flow: the ground one pixel sees here, in metres.
    let fp = max(length(dpdx(wpos)), length(dpdy(wpos))) * ap.eye.w;
    let dir = normalize(wpos + ap.eye.xyz);
    // `geo::lat_lon_from_dir`: latitude from y, longitude from (x, −z).
    let lat = degrees(asin(clamp(dir.y, -1.0, 1.0)));
    let lon = degrees(atan2(-dir.z, dir.x));
    let radius = ap.globe.w;
    // Metres per degree along a parallel and along a meridian, here.
    let m_per_deg_n = radius * DEG;
    let m_per_deg_e = m_per_deg_n * max(cos(lat * DEG), 1.0e-4);
    var out : GroundLook;
    if (ap.window2.w > 0.5) {
        // Into the patch's texel grid: east of its west edge (a whole turn folded away), south of its
        // north edge.
        let de = lon - ap.window.x;
        let tx = (de - 360.0 * floor(de / 360.0)) * ap.window.z;
        let ty = (ap.window.y - lat) * ap.window.w;
        if (tx >= 0.0 && tx < ap.window2.x && ty >= 0.0 && ty < ap.window2.y) {
            let texel_e = m_per_deg_e / ap.window.z;
            let texel_n = m_per_deg_n / ap.window.w;
            let lod = log2(max(fp / texel_e, fp / texel_n));
            // Finer than the patch's coarsest level: the patch answers. At or past it the globe's
            // own level is the same footprint, built from the same rule.
            if (lod < ap.window2.z - 1.0) {
                let uv = vec2<f32>(tx / ap.window2.x, ty / ap.window2.y);
                let l = max(lod, 0.0);
                let s = textureSampleLevel(p_look, samp_p, uv, l);
                out.albedo = s.rgb;
                out.sigma = s.a * HALF_PI;
                out.mat = material_at(p_mat, uv, i32(round(l)));
                out.slope = textureSampleLevel(p_slope, samp_p, uv, l).xy;
                out.texel_m = texel_n * exp2(l);
                return out;
            }
        }
    }
    let uv = vec2<f32>((lon + 180.0) / 360.0, (90.0 - lat) / 180.0);
    let texel_e = m_per_deg_e / ap.globe.x;
    let texel_n = m_per_deg_n / ap.globe.y;
    let lod = clamp(log2(max(fp / texel_e, fp / texel_n)), 0.0, ap.globe.z - 1.0);
    let s = textureSampleLevel(g_look, samp_g, uv, lod);
    out.albedo = s.rgb;
    out.sigma = s.a * HALF_PI;
    out.mat = material_at(g_mat, uv, i32(round(lod)));
    out.slope = textureSampleLevel(g_slope, samp_g, uv, lod).xy;
    out.texel_m = texel_n * exp2(lod);
    return out;
}

// **The one shading of a surface** — given what it is made of here: its footprint's albedo, the grain
// layer to sample, and the slope spread the mesh is not showing.
fn shade(i : VOut, col : vec3<f32>, mat : u32, rough : f32, base_n : vec3<f32>) -> vec4<f32> {
    // Relief from the material's own sub-resolution surface statistic (the shared chunk). `i.wpos` is
    // camera-relative (globe AND cap; one convention, so the relief cannot mismatch across the
    // cross-fade); the anchor restores surface-fixed texture coordinates modulo the tile period.
    // `base_n` is the surface's normal at the scale its shape is being stated at: the mesh's for a
    // mesh that carries its shape, the pyramid's for the ground (see `fs_ground`).
    let n = surface_normal_triplanar(i.wpos + u.emissive.xyz, base_n, mat, GLOBE_TEX_SCALE);
    let l = normalize(u.light_dir.xyz);
    // Positions are camera-relative (eye at the origin), so the direction back to the eye is -wpos.
    let view = normalize(-i.wpos);
    // **The appearance integral's second moment reaching the light** (docs/63). `rough` is the slope
    // spread the mesh could not carry as geometry; at zero this is bit-identical to the `max(dot(n,l),0)`
    // that stood here, so ground the mesh fully resolves looks exactly as it did.
    let ndl = rough_diffuse(n, l, view, rough);
    // Reflected sunlight (albedo × illumination), same SUN_GAIN + Reinhard as the space band; black night side.
    let SUN_GAIN = u.atm.w; // atmosphere::SUN_GAIN — one exposure for every view of this world
    // **The material's OWN texture for the grain, the footprint for the colour.** The texture array was
    // bound here and never sampled for most of this engine's life, so the ground read as a uniform wash
    // from standing height however much relief was under it. Then it was sampled and the vertex colour
    // was DISCARDED (the texture already carries the material's albedo; multiplying two albedos squares
    // one) — and with it the mixture and the season went too: by mutation on 2026-08-04, every land
    // vertex set to magenta changed nothing on screen (docs/46 row 41). Then the vertex carried a RATIO
    // of the mixture over its own dominant material's flat albedo, and this multiplied it by the
    // texture of the TRIANGLE's flat material.
    //
    // ★★ ONE FOOTPRINT, ONE BASE (docs/46 row 90). `col` is the footprint's integrated albedo ITSELF —
    // interpolated across a triangle for a mesh that carries it, read from the pyramid for the ground —
    // and the texture contributes only its unit-mean GRAIN: the sample divided by that texture's own
    // mean, which is its 1×1 mip. The grain pattern still switches where the material does; that is
    // sub-footprint detail, not colour, and no base is involved.
    let grain = surface_albedo_triplanar(i.wpos + u.emissive.xyz, n, mat, GLOBE_TEX_SCALE);
    let levels = textureNumLevels(tex);
    let mean = textureSampleLevel(tex, samp, vec2<f32>(0.5, 0.5), mat, f32(levels - 1u)).rgb;
    let detail = grain / max(mean, vec3<f32>(1.0 / 255.0));
    let albedo = detail * col * u.tint.rgb;
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

// A mesh that carries its own appearance per vertex: the plants, the gun, the space band's bodies.
@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    return shade(i, i.col, i.mat, i.rough, normalize(i.normal));
}

// ★★★ Terra's ground: the mesh carries shape only; appearance is the pyramid's, per pixel footprint.
//
// The NORMAL is part of the appearance (docs/63: the mesh carries the mean slope, the appearance the
// variance about it — and the pyramid's variance is about the pyramid's own mean, so that mean is the
// slope it must be shaded with). The mesh's normal is a second, mesh-locked statement of the same
// slope at the mesh's own band limit, which is finest under the eye and kilometres at the rim: shading
// by it alone put a starburst of the mesh's cells into every frame at altitude. So the normal comes
// from whichever of the two is the FINER statement at this pixel — the pyramid's where its texel is
// smaller than the mesh's cell here (`i.rough` carries the cell, metres), the mesh's where the mesh is
// finer (near the eye, where the generated relief lives in the geometry) — blended over the octave in
// which the two band limits cross, so nothing changes in a step.
@fragment
fn fs_ground(i : VOut) -> @location(0) vec4<f32> {
    let g = ground_look(i.wpos);
    // The geographic frame at this fragment — the same one the pyramid's gradients are stated in
    // (`appearance::integrate_rect_on_sphere`: east = Y × up, north = up × east).
    let up = normalize(i.wpos + ap.eye.xyz);
    let east = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), up));
    let north = cross(up, east);
    // A height field `h = s_e·e + s_n·n` has the normal (−s_e, −s_n, 1) in (east, north, up). The
    // exaggeration is the mesh's; the slope shades the shape that is drawn.
    let sl = g.slope * ap.scale.x;
    let n_pyr = normalize(up - east * sl.x - north * sl.y);
    let n_mesh = normalize(i.normal);
    let cell_m = max(i.rough, 1.0e-3);
    let t = clamp(log2(g.texel_m / cell_m), 0.0, 1.0);
    let base_n = normalize(mix(n_pyr, n_mesh, t));
    return shade(i, g.albedo, g.mat, g.sigma, base_n);
}
