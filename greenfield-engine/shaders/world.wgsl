// Phase 4 world shader: triplanar-sample the per-material procedural texture array, light it, and
// add a specular highlight (shine) driven by the material's roughness/metallic. Textures are
// generated from physical properties on the CPU (see texture.rs) — no external image assets.

struct Uniforms {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    light_dir : vec4<f32>,   // xyz = direction TO the light (world space), normalized
    camera_pos: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;
@group(0) @binding(1) var tex : texture_2d_array<f32>;
@group(0) @binding(2) var samp : sampler;
// Per-material params: x = roughness, y = metallic (rest reserved).
@group(0) @binding(3) var<uniform> matparams : array<vec4<f32>, 32>;

struct VOut {
    @builtin(position) clip      : vec4<f32>,
    @location(0) normal          : vec3<f32>, // world space
    @location(1) local           : vec3<f32>, // object space, for triplanar coords
    @location(2) world_pos       : vec3<f32>,
    @location(3) @interpolate(flat) mat : u32,
};

@vertex
fn vs_main(
    @location(0) pos    : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(3) mat    : u32,
) -> VOut {
    var o : VOut;
    let world = u.model * vec4<f32>(pos, 1.0);
    o.clip = u.view_proj * world;
    o.normal = (u.model * vec4<f32>(normal, 0.0)).xyz;
    o.local = pos;
    o.world_pos = world.xyz;
    o.mat = mat;
    return o;
}

const TEX_SCALE : f32 = 1.0 / 8.0; // one texture tile per 8 metres

fn triplanar(local : vec3<f32>, n : vec3<f32>, layer : u32) -> vec3<f32> {
    var w = abs(n);
    w = w / (w.x + w.y + w.z);
    let cx = textureSample(tex, samp, local.yz * TEX_SCALE, layer).rgb;
    let cy = textureSample(tex, samp, local.xz * TEX_SCALE, layer).rgb;
    let cz = textureSample(tex, samp, local.xy * TEX_SCALE, layer).rgb;
    return cx * w.x + cy * w.y + cz * w.z;
}

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light_dir.xyz);

    let albedo = triplanar(i.local, n, i.mat);

    let params = matparams[i.mat];
    let view = normalize(u.camera_pos.xyz - i.world_pos);

    // WATER (liquid, params.z flag) — rendered honestly, not as a flat blue decal: a Fresnel reflection
    // of the sky over a dark, absorbing body. This is the real reason calm water reads blue — mirror-
    // bright and blue at grazing angles, dark and near-transparent looking straight down — and it falls
    // out of water's cited specular F0 = 0.02 (the DB `water` optical value), not a tuned colour.
    // FLAGGED approximations (refinements needing more shader/physics work): the reflected sky is a
    // simple Rayleigh-blue gradient rather than a sample of the real atmosphere pass; the "body" colour
    // is a dim blue-green stand-in for volumetric absorption/scattering to the seabed; and there is no
    // refraction or waves/flow yet (the sea is STATIC).
    if (params.z > 0.5) {
        let cosv = clamp(dot(n, view), 0.0, 1.0);
        let f0 = 0.02;                                   // water's cited normal-incidence reflectance
        let fres = f0 + (1.0 - f0) * pow(1.0 - cosv, 5.0);
        let refl = reflect(-view, n);
        // Rayleigh-ish sky the surface mirrors: deep blue toward the zenith, paler toward the horizon.
        let up = clamp(refl.y, 0.0, 1.0);
        let sky_col = mix(vec3<f32>(0.52, 0.70, 0.90), vec3<f32>(0.10, 0.32, 0.72), up);
        // Blue-green upwelling from a shallow sunlit sea (absorption/scattering stand-in), lit — the
        // colour looking straight down, before the Fresnel sky reflection takes over toward grazing.
        let lit = 0.45 + 0.55 * max(dot(n, l), 0.0);
        let body = vec3<f32>(0.06, 0.20, 0.29) * lit;
        // Sharp specular sun-glint off the flat surface.
        let glint = pow(max(dot(refl, l), 0.0), 220.0);
        let wcolor = mix(body, sky_col, fres) + vec3<f32>(1.0, 1.0, 0.95) * glint * 0.7;
        return vec4<f32>(wcolor, 1.0);
    }

    // Diffuse + ambient + a little hemispheric fill.
    let diffuse = max(dot(n, l), 0.0);
    let ambient = 0.36;
    let sky = 0.12 * (0.5 + 0.5 * n.y);
    var color = albedo * (ambient + sky + (1.0 - ambient) * diffuse);

    // Specular highlight (shine) from material params.
    let rough = clamp(params.x, 0.03, 1.0);
    let metal = clamp(params.y, 0.0, 1.0);
    let h = normalize(l + view);
    let ndoth = max(dot(n, h), 0.0);
    let shininess = mix(6.0, 400.0, 1.0 - rough);
    let spec = pow(ndoth, shininess) * diffuse; // gate by lit side
    let spec_color = mix(vec3<f32>(1.0), albedo, metal); // metals tint their highlight
    let spec_strength = mix(0.05, 1.0, metal) * (1.0 - rough);
    color += spec_color * (spec * spec_strength);

    return vec4<f32>(color, 1.0);
}
