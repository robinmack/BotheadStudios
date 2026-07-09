// Space-band shader (scale-relative "orbit-to-ground", Step A): simple lit spheres for the planet and
// moon. No textures — just a per-body tint and one directional "sun", so we get a lit/dark hemisphere
// (phases). Positions/scales come from the N-body physics (orbit.rs), mapped to display units.

struct U {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    light_dir : vec4<f32>, // xyz = direction TO the sun, normalized
    tint      : vec4<f32>, // body color
};

@group(0) @binding(0) var<uniform> u : U;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) normal     : vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos : vec3<f32>, @location(1) nrm : vec3<f32>) -> VOut {
    var o : VOut;
    o.clip = u.view_proj * u.model * vec4<f32>(pos, 1.0);
    o.normal = (u.model * vec4<f32>(nrm, 0.0)).xyz;
    return o;
}

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light_dir.xyz);
    let ndl = max(dot(n, l), 0.0);

    // Apparent brightness = ILLUMINATION x REFLECTANCE, not a bright material. u.tint.rgb is the body's
    // real diffuse reflectance (albedo) — often low, e.g. basalt ~0.05. It looks bright because a very
    // bright sun reflects off it. SUN_GAIN folds the sun's radiance and a display exposure into one
    // uniform scalar (a lighting/camera property, identical for every body: it moves brightness, never
    // hue or relative reflectance). The result is a radiance that can exceed 1, so we Reinhard
    // tone-map it back into [0,1] — a dark, strongly-lit body ends up correctly bright.
    // NOTE (honesty): the sun DIRECTION here is still a placeholder; the real Sun body (proper
    // mass/distance) becomes the illuminant when the heliocentric view lands (docs/17).
    let SUN_GAIN = 22.0;
    let AMBIENT = 0.02; // faint starlight fill so the night side isn't pure black
    let radiance = u.tint.rgb * (AMBIENT + ndl * SUN_GAIN);
    let mapped = radiance / (vec3<f32>(1.0) + radiance); // Reinhard tone-map
    return vec4<f32>(mapped, 1.0);
}
