// Instanced render of ENGINE MATTER (docs/50 render path, docs/59). One camera-facing billboard per
// `crate::Drawn` item, fed from the shared `GpuParticle` instance layout — the same bytes the ground
// scene's grains use, so a scene draws bodies, grains and shed vapour through one pipeline.
//
// **The size rule is a SAMPLING statement, not a look.** A body's billboard is its REAL radius, projected.
// But an entry trail seen from orbit is genuinely sub-pixel — a metre-scale parcel 400 km away subtends
// far less than one pixel — and a raster cannot draw less than a pixel. Real meteors are visible from
// orbit for exactly this reason: not because they are large, but because they are BRIGHT point sources
// against a dark Earth. So the half-size is `max(true projected size, one pixel)`: above a pixel the size
// is the physics, below it the mark is a point sample of something really there. That floor is the
// resolution of the display, not a dial to make anything look right, and it disappears the moment the
// camera is close enough for the true size to win.

struct Cam {
  view_proj: mat4x4<f32>,
  // x = DISPLAY_SCALE (metres → display units), y = projection x scale, z = projection y scale,
  // w = one pixel as a half-extent in NDC (2/viewport_height).
  params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Cam;

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) color: vec3<f32>,
  @location(1) emission: vec3<f32>,
  @location(2) uv: vec2<f32>,
};

var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
  vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
  vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32,
           @location(0) inst_pos: vec3<f32>,    // display units, planet-centred
           @location(1) color: vec3<f32>,       // the material's own measured albedo
           @location(2) emission: vec3<f32>,    // incandescence of its REAL temperature
           @location(3) radius_m: f32) -> VOut {
  let c = CORNERS[vi];
  var clip = cam.view_proj * vec4<f32>(inst_pos, 1.0);

  // True projected half-size, in clip units. For a point at clip.w = −z_view, a world half-extent `h`
  // subtends `h·proj / clip.w` in NDC, so adding `h·proj` in CLIP space is exact — no divide.
  let half_disp = radius_m * cam.params.x;
  let true_x = half_disp * cam.params.y;
  let true_y = half_disp * cam.params.z;
  // One pixel, in the same clip units (NDC half-extent × w).
  let px = cam.params.w * clip.w;

  clip.x = clip.x + c.x * max(true_x, px);
  clip.y = clip.y + c.y * max(true_y, px);

  var o: VOut;
  o.clip = clip;
  o.uv = c;
  o.color = color;
  o.emission = emission;
  return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
  // A round mark with a soft edge — the billboard is a quad, the matter is not.
  let r2 = dot(in.uv, in.uv);
  if (r2 > 1.0) { discard; }
  let falloff = 1.0 - smoothstep(0.35, 1.0, r2);
  // What it emits is what it is: incandescence of its own temperature, over its material's albedo. Cold
  // matter shows only albedo (and is nearly invisible against space, which is correct).
  let rgb = in.color * 0.15 + in.emission;
  return vec4<f32>(rgb * falloff, falloff);
}
