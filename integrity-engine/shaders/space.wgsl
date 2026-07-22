// Space-band shader (scale-relative "orbit-to-ground", Step A): simple lit spheres for the planet and
// moon. No textures — just a per-body tint and one directional "sun", so we get a lit/dark hemisphere
// (phases). Positions/scales come from the N-body physics (orbit.rs), mapped to display units.

struct U {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    light_dir : vec4<f32>, // xyz = direction TO the sun, normalized
    tint      : vec4<f32>, // body color
    emissive  : vec4<f32>, // rgb = incandescent glow colour, w = intensity (self-lit, e.g. hot ejecta)
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
    // NO ambient term: the Sun is the only appreciable light source in this universe (no other
    // stars are modelled), so the night side is genuinely BLACK. The old 0.02 "starlight fill" was a
    // fudged light source — Robin caught backlit bodies glowing that should have been dark crescents.
    let AMBIENT = 0.0;
    // Reflected sunlight + self-emission. Incandescence is added BEFORE the sun term so hot ejecta glows
    // on its own — visible on the night side, exactly like real shock-heated rock. The colour/intensity
    // are a blackbody ramp of the fragment's actual temperature (matter physics → light, nothing scripted).
    let radiance = u.emissive.rgb * u.emissive.w + u.tint.rgb * (AMBIENT + ndl * SUN_GAIN);
    let mapped = tonemap(radiance); // the shared display law — compresses brightness, keeps hue
    return vec4<f32>(mapped, 1.0);
}
