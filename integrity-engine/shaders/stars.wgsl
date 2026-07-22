// **The real sky.** One instanced quad per catalogued star, drawn on a sphere centred on the camera.
//
// Stars are POINT sources — none of them subtends a resolvable angle from anywhere we can fly to. So the
// quad is not the star's disk; it is a point-spread function, the blur an eye or a lens puts on a point.
// Its width is fixed in PIXELS and its PEAK follows the star's measured flux, which means a bright star
// covers more pixels above the display threshold than a faint one WITHOUT anyone writing a size-vs-
// magnitude curve. Apparent size emerges from brightness, the way it does through a real instrument.
//
// "Render only what's in view" needs no special machinery here: these are ordinary primitives, so stars
// behind the camera or outside the frustum are clipped before they cost a fragment.

struct StarU {
    view_proj : mat4x4<f32>,
    // Inertial (ICRS) -> world. Identity where the scene's frame is already inertial; Earth's rotation
    // where the world frame is Earth-fixed, which is what makes the sky wheel overhead at the sidereal rate.
    spin      : mat4x4<f32>,
    cam_pos   : vec4<f32>, // xyz = eye in DISPLAY units — where to hang the billboards
    cam_pc    : vec4<f32>, // xyz = eye in PARSECS from Sol, in the catalogue's frame; w = viewport aspect
    params    : vec4<f32>, // x = billboard distance, y = PSF width (px), z = viewport height (px), w = exposure
};
@group(0) @binding(0) var<uniform> u : StarU;

struct VOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) offset : vec2<f32>, // position within the PSF, in units of its width
    @location(1) color : vec3<f32>,
    @location(2) peak : f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vi : u32,
    @location(0) pos_pc : vec3<f32>,
    @location(1) color : vec3<f32>,
    @location(2) luminosity : f32,
) -> VOut {
    // Two triangles, corners at ±1.
    var CORNERS = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = CORNERS[vi];

    // **Real geometry, not a sky sphere.** Direction is where the star actually is RELATIVE TO THE
    // OBSERVER, and brightness is the inverse-square law over that true distance. Inside a solar system
    // the observer is ~1e-5 pc from Sol, so this reproduces the sky we know and the parallax is correctly
    // far below a pixel. Travel light-years and the constellations open up on their own — nothing here
    // assumes we are at the centre of anything.
    let rel = pos_pc - u.cam_pc.xyz;
    let dist_pc = max(length(rel), 1e-6);
    let dir = rel / dist_pc;
    // Flux at 10 pc, carried to the real distance.
    let flux = luminosity * 100.0 / (dist_pc * dist_pc);
    let world = u.cam_pos.xyz + (u.spin * vec4<f32>(dir, 0.0)).xyz * u.params.x;
    var clip = u.view_proj * vec4<f32>(world, 1.0);
    // Expand by a fixed number of PIXELS: multiplying by clip.w cancels the perspective divide, so the
    // PSF stays the same size on screen however far away the sphere is placed.
    // NDC spans [-1,1] over the viewport in BOTH axes, so a pixel is worth 2/height vertically and
    // 2/width horizontally — the x step must be divided by the aspect ratio, not multiplied by it.
    // Getting that backwards stretched every star into a horizontal dash (aspect² too wide), which is
    // what the rig capture showed. Aspect is passed explicitly: reading it back out of view_proj only
    // works when the matrix carries no rotation, which is never true here.
    let px = u.params.y;
    let ndc_y = 2.0 / u.params.z;
    let ndc_x = ndc_y / max(u.cam_pc.w, 1e-6);
    clip = vec4<f32>(
        clip.x + corner.x * px * ndc_x * clip.w,
        clip.y + corner.y * px * ndc_y * clip.w,
        clip.z,
        clip.w,
    );

    // **Pin to the far plane.** A star is at infinity, so no radius is the right radius: the space band's
    // frustum reaches 100,000 display units while Terra's reaches ~3 (and shrinks as you descend, since it
    // is tied to the horizon). A sphere sized for one is clipped away entirely in the other — which is
    // exactly what happened: Terra rendered a starless black. Forcing z = w puts every star at maximum
    // depth in whatever frustum is current, so the sky is never clipped and never intrudes.
    var o : VOut;
    o.pos = vec4<f32>(clip.x, clip.y, clip.w * 0.999999, clip.w);
    o.offset = corner;
    o.color = color;
    o.peak = flux * u.params.w; // measured flux, at the scene's exposure
    return o;
}

@fragment
fn fs_main(i : VOut) -> @location(0) vec4<f32> {
    // Gaussian point-spread. A star far below the display threshold contributes almost nothing; a bright
    // one saturates out to a wider radius. That difference IS the apparent size.
    let r2 = dot(i.offset, i.offset);
    let psf = exp(-4.0 * r2);
    let v = i.peak * psf;
    if (v < 0.004) {
        discard; // below what the display can show — do not pay for it
    }
    let c = i.color * v;
    // The shared display law. A very bright star still clips in its strongest channel, but it keeps its
    // colour on the way there — Rigel stays blue-white and Betelgeuse stays orange instead of both
    // washing out, which is the whole reason the catalogue carries a colour index.
    return vec4<f32>(tonemap(c), 1.0);
}
