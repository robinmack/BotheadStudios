// Instanced debris particles (Phase 3): a small cube drawn once per particle, placed by a
// per-instance offset and tinted by its material's albedo. Shares the world's uniform (view_proj).

struct Uniforms {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>, // unused here (identity); particle offset does the placement
    light_dir : vec4<f32>,
    camera_pos: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) normal     : vec3<f32>,
    @location(1) color      : vec3<f32>,
    @location(2) emission    : vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) pos      : vec3<f32>,   // cube-local position
    @location(1) normal   : vec3<f32>,
    @location(4) offset   : vec3<f32>,   // per-instance world position (centered coords)
    @location(5) color    : vec3<f32>,   // per-instance material albedo
    @location(6) emission : vec3<f32>,   // per-instance incandescent glow (from temperature)
) -> VOut {
    var o : VOut;
    o.clip = u.view_proj * vec4<f32>(pos + offset, 1.0);
    o.normal = normal;
    o.color = color;
    o.emission = emission;
    return o;
}

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let ambient = 0.40;
    // Reflected colour + emission. Emission is added, so molten debris glows on its own (even on the
    // dark side) — it emits light because it is hot, it isn't merely lit (docs/20).
    let lit = i.color * (ambient + (1.0 - ambient) * diffuse);
    return vec4<f32>(lit + i.emission, 1.0);
}
