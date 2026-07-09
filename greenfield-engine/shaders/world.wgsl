// Phase 1 world shader: transform voxel-mesh vertices and shade with one directional light
// plus ambient. Vertex colors are the materials' linear-RGB albedo; the sRGB surface encodes
// on write, so we output linear here.

struct Uniforms {
    view_proj : mat4x4<f32>,
    light_dir : vec4<f32>,   // xyz = direction TO the light (world space), normalized
    camera_pos: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) normal     : vec3<f32>,
    @location(1) color      : vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) pos    : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(2) color  : vec3<f32>,
) -> VOut {
    var o : VOut;
    o.clip = u.view_proj * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.color = color;
    return o;
}

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let ambient = 0.38;
    // A touch of hemispheric fill so downward faces aren't pure black.
    let sky = 0.12 * (0.5 + 0.5 * n.y);
    let shade = ambient + sky + (1.0 - ambient) * diffuse;
    return vec4<f32>(i.color * shade, 1.0);
}
