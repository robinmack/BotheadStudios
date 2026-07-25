// Instanced render of the GPU SPH particles (docs/33 stage 4c.4). Draws the `sph_step.wgsl` particle buffer
// directly (zero-copy: the physics buffer IS the instance vertex buffer), one camera-facing billboard quad
// per particle. The Earth-relative → display → clip transform is done here per-instance (positions stay
// Earth-relative f32 to avoid the planetary-coordinate cancellation; the CPU passes the display origin+scale).

struct Cam {
  view_proj: mat4x4<f32>,
  origin: vec4<f32>, // xyz = Earth's display-space position ((earth_world − focus)·DISPLAY_SCALE)
  params: vec4<f32>, // x = DISPLAY_SCALE (m → display units), y = billboard half-size, z = ejecta radius (m),
                     //   w = ejecta mote half-size (docs/42 Phase 3: thrown-off matter glows even at the pretty end)
};
@group(0) @binding(0) var<uniform> cam: Cam;

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) color: vec3<f32>,
  @location(1) uv: vec2<f32>,
};

// Two triangles → a quad, generated from the vertex index (no per-vertex mesh needed).
var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
  vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
  vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32,
           @location(0) inst_pos: vec3<f32>,
           @location(1) prov: u32) -> VOut {
  let c = CORNERS[vi];
  // Earth-relative metres → display units, offset to Earth's display position.
  let display = cam.origin.xyz + inst_pos * cam.params.x;
  var clip = cam.view_proj * vec4<f32>(display, 1.0);
  // docs/42 Phase 3 — EJECTA: matter thrown beyond the remnant (radius > params.z) glows as a mote even at the
  // pretty end, so the pretty sphere is wrapped in a real physics-driven ejecta plume. Remnant particles keep the
  // blend-scaled size (params.y → 0 at the pretty end, so they hide behind the sphere). The mote size (params.w)
  // wins for ejecta; a smooth ramp avoids a hard shell edge.
  let ejecta = smoothstep(cam.params.z * 0.85, cam.params.z * 1.25, length(inst_pos));
  let half = mix(cam.params.y, max(cam.params.y, cam.params.w), ejecta);
  clip.x = clip.x + c.x * half * clip.w;
  clip.y = clip.y + c.y * half * clip.w;
  var o: VOut;
  o.clip = clip;
  o.uv = c;
  // Provenance colour by SOURCE BODY (docs/58 #5): the target/cap (prov 0) = warm rock, the impactor
  // (prov 1) = cool steel — so which body a particle came from stays visible as they mix. (A diagnostic
  // two-tone; docs/58 #5's end state is the source material's real albedo, not a hardcoded pair.)
  var base: vec3<f32>;
  if (prov == 0u) {
    base = vec3<f32>(0.72, 0.48, 0.30);
  } else {
    base = vec3<f32>(0.42, 0.55, 0.78);
  }
  // Freshly-ejected matter is incandescent — ramp toward a hot glow with the ejecta factor.
  o.color = mix(base, vec3<f32>(1.6, 0.85, 0.35), ejecta * 0.7);
  return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
  // Round the billboard into a soft disc; discard the corners.
  let r2 = dot(in.uv, in.uv);
  if (r2 > 1.0) { discard; }
  let shade = 0.55 + 0.45 * (1.0 - r2); // fake spherical shading (bright centre)
  return vec4<f32>(in.color * shade, 1.0);
}
