// ★★ **THE ATMOSPHERE — one integral, every view of it** (docs/66).
//
// WGSL has no `#include`, so this chunk is prepended in Rust (as `surface_normal.wgsl` is). It is a
// LINE-FOR-LINE MIRROR of `atmosphere::air_inscatter` in Rust; that function is the reference, this is
// the copy that runs per pixel, and `tools/sky-verify` runs THIS on a real GPU and compares it to THAT
// so the two cannot drift.
//
// It replaces `rayleigh.wgsl`'s closed form, which is the analytic solution of one geometry — a slab
// seen from OUTSIDE, looking down — and is simply the wrong integral for anything else. Standing on
// the ground looking up, the sun path shortens as the view path lengthens, which is exactly the
// pairing the closed form relies on to collapse.
//
// Everything the sky does falls out of the one integral, and nothing is drawn on:
//   * blue overhead, pale at the horizon      — path length, band by band;
//   * red at sunset                            — blue removed from the sunlight before it can scatter;
//   * a soft terminator and TWILIGHT           — the low air is in the planet's shadow, the air above
//                                                it is not; no declared half-angle ramp;
//   * aerial perspective over the ground       — the same integral, stopped at the surface point;
//   * a glowing limb from orbit                — rays that miss the ground still cross lit air.
//
// LENGTHS ARE METRES here, all of them, including `t_end`. The caller converts. (Display units would
// put a camera 2 m up at 3e-7 of a radius, and f32 cannot hold that against 1.0.)
//
// HONESTY FLAGS, all four of them stand-ins with named refinements, none a dial: SINGLE scatter (the
// sky's own light does not light the sky, so deep twilight is darker than life); no Mie/aerosol term
// (no haze, no white horizon band); no ozone (the Chappuis band is what keeps a real twilight blue);
// and a POINT Sun, per Robin — *"Sun is close to a point source"* — so no penumbra at the terminator.

struct AirColumn {
    tau          : vec3<f32>, // vertical optical depth of the whole column, per band
    scale_height : f32,       // barometric e-folding height (m)
    radius       : f32,       // the surface the column stands on (m)
    top          : f32,       // where the column stops being worth integrating (m above the surface)
    sun_gain     : f32,       // the shared exposure — one atmosphere, one exposure, every view
};

struct Scattered {
    inscatter : vec3<f32>, // what the air ADDED
    transmit  : vec3<f32>, // what the air PASSED, of whatever is behind it
};

// The resolution the sky is drawn at, mirroring `atmosphere::SKY_VIEW_STEPS`/`SKY_SUN_STEPS`. Read off
// the convergence measurement, not chosen: at these counts the worst ray in the scene (near-horizontal
// view, near-horizontal sun) is within 2% of a 512x128 reference, and halving them costs 6.6%.
const SKY_VIEW_STEPS : i32 = 32;
const SKY_SUN_STEPS  : i32 = 8;

// A `t_end` meaning "the ray is not stopped by anything the caller knows about".
const NO_LIMIT : f32 = 1.0e30;

fn air_exists(air : AirColumn) -> bool {
    return air.tau.z > 0.0 && air.scale_height > 0.0 && air.radius > 0.0;
}

// Radius of a point on the ray as a RATIO of the eye's own radius: |r̂ + k·d̂| = √(1 + 2k·µv + k²).
fn ray_radius_ratio(k : f32, mu_v : f32) -> f32 {
    return sqrt(max(1.0 + k * (2.0 * mu_v + k), 0.0));
}

// Altitude of a point on the ray, written so it survives f32. The naive |eye + t·d| − R subtracts two
// numbers that agree to seven digits at ground level and keeps the noise; multiplying by the conjugate
// instead leaves every term O(h·R) or O(t·R) and nothing cancels.
fn ray_altitude(k : f32, mu_v : f32, h0 : f32, radius : f32) -> f32 {
    let r0 = radius + h0;
    let num = h0 * (2.0 * radius + h0) + r0 * r0 * k * (2.0 * mu_v + k);
    let approx = num / (2.0 * radius); // one refinement of the denominator; alt ≪ R
    return num / (2.0 * radius + max(approx, 0.0));
}

// Path length (in units of the eye's radius) at which the ray from altitude `h0` reaches altitude
// `target_alt`; −1 if it never does ahead of the eye. ALTITUDES, not radii: `target − r0` for the
// ground is exactly `−h0`, and a camera 2 m up is 3e-7 of Earth's radius.
fn ray_reaches(mu_v : f32, radius : f32, h0 : f32, target_alt : f32, want_far : bool) -> f32 {
    let r0 = radius + h0;
    let q = (target_alt - h0) * (2.0 * radius + h0 + target_alt) / (r0 * r0);
    let disc = mu_v * mu_v + q;
    if (disc < 0.0) { return -1.0; }
    let s = sqrt(disc);
    let near = -mu_v - s;
    let far = -mu_v + s;
    var pick = near;
    if (want_far) { pick = far; }
    if (pick > 0.0) { return pick; }
    if (far > 0.0) { return far; }
    return -1.0;
}

// Where to put the i-th of n samples, and how wide its step is. Uniform steps spend most of their
// samples on air that is not there, so they are packed quadratically toward the ray's CLOSEST APPROACH
// to the surface — its densest point, wherever that falls. Returns (k, dk), and Σ dk = k1 − k0 exactly.
fn ray_sample(i : i32, n : i32, k0 : f32, k1 : f32, mu_v : f32) -> vec2<f32> {
    let span = k1 - k0;
    let k_min = clamp(-mu_v, k0, k1); // dr/dk = 0 at k = −µ
    let lo = (k_min - k0) / span;
    let u = (f32(i) + 0.5) / f32(n);
    var k : f32;
    var s : f32;
    if (u < lo) {
        s = (lo - u) / lo; // 1 at the near end, 0 at the perigee
        k = k_min - (k_min - k0) * s * s;
    } else {
        s = (u - lo) / max(1.0 - lo, 1.0e-12);
        k = k_min + (k1 - k_min) * s * s;
    }
    return vec2<f32>(k, 2.0 * span * s / f32(n));
}

// Optical depth from a point out to space along one direction — the sun path, which is also the whole
// of "does light reach here". `w = 0` means the body itself is in the way: that is the SHADOW, and it
// is a geometric fact rather than a lighting term. It is what makes twilight emerge.
fn column_to_space(air : AirColumn, h : f32, mu : f32) -> vec4<f32> {
    if (mu < 0.0) {
        // The ray misses the body iff its perpendicular distance from the centre exceeds R. Expanded in
        // x = h/R so the big numbers cancel algebraically instead of numerically.
        let x = h / air.radius;
        let m2 = mu * mu;
        if ((2.0 * x + x * x) * (1.0 - m2) <= m2) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0); // shadowed — no direct sunlight reaches this parcel
        }
    }
    if (h >= air.top) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0); // above the column: lit, nothing in front of the Sun
    }
    let r0 = air.radius + h;
    let k_top = ray_reaches(mu, air.radius, h, air.top, true);
    if (k_top <= 0.0) { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }
    var depth = 0.0;
    for (var i = 0; i < SKY_SUN_STEPS; i = i + 1) {
        let ks = ray_sample(i, SKY_SUN_STEPS, 0.0, k_top, mu);
        let alt = ray_altitude(ks.x, mu, h, air.radius);
        depth = depth + exp(-alt / air.scale_height) * ks.y;
    }
    let ds = depth * r0 / air.scale_height; // β = τ/H, and ds = r0·dk
    return vec4<f32>(air.tau * ds, 1.0);
}

// ★★ The whole sky, the whole limb, the whole aerial perspective and the whole terminator:
//     L = F·(P(Θ)/4)·∫ β(h)·e^{−τ_sun(s)}·e^{−τ_view(s)} ds,  P(Θ) = ¾(1+cos²Θ),  β(h) = (τ/H)e^{−h/H}.
// The /4 rather than /4π is the engine's standing radiance convention — the surface term is likewise
// albedo·µ·F and not albedo·µ·F/π — so the sky and the ground under it share one exposure.
fn air_inscatter(air : AirColumn, h0 : f32, mu_v : f32, mu_s : f32, cos_theta : f32, t_end : f32) -> Scattered {
    var out : Scattered;
    out.inscatter = vec3<f32>(0.0);
    out.transmit = vec3<f32>(1.0);
    if (!air_exists(air)) { return out; }
    let r0 = air.radius + h0;

    // Where the ray is inside the air at all. An eye ABOVE the column enters it at the near root; an
    // eye inside it starts immediately and leaves at the far one.
    var k0 = 0.0;
    var k1 = 0.0;
    if (h0 >= air.top) {
        let k_in = ray_reaches(mu_v, air.radius, h0, air.top, false);
        if (k_in <= 0.0) { return out; } // looking past this body's air entirely
        k0 = k_in;
        k1 = max(ray_reaches(mu_v, air.radius, h0, air.top, true), k_in);
    } else {
        k1 = max(ray_reaches(mu_v, air.radius, h0, air.top, true), 0.0);
    }
    // The body itself stops the ray, whatever the caller said.
    let k_hit = ray_reaches(mu_v, air.radius, h0, 0.0, false);
    if (k_hit > 0.0) { k1 = min(k1, k_hit); }
    k1 = min(k1, t_end / r0);
    if (k1 <= k0) { return out; }

    let phase = 0.75 * (1.0 + cos_theta * cos_theta);
    var tau_view = vec3<f32>(0.0);
    var acc = vec3<f32>(0.0);
    for (var i = 0; i < SKY_VIEW_STEPS; i = i + 1) {
        let ks = ray_sample(i, SKY_VIEW_STEPS, k0, k1, mu_v);
        let ds = ks.y * r0;
        let alt = ray_altitude(ks.x, mu_v, h0, air.radius);
        let rho = exp(-alt / air.scale_height);
        let d_tau = air.tau / air.scale_height * rho * ds;
        // The sample's OWN zenith, and the sun's cosine from it — the two things that turn a flat-slab
        // sky into a round one. r̂ₚ = (r̂ + k·d̂)/|r̂ + k·d̂|, so both cosines just divide by that length.
        let rr = max(ray_radius_ratio(ks.x, mu_v), 1.0e-12);
        let mu_s_p = (mu_s + ks.x * cos_theta) / rr;
        // Half a step of this cell's own depth puts the sample at its midpoint, not its near face.
        let half = tau_view + 0.5 * d_tau;
        let sun = column_to_space(air, alt, mu_s_p);
        if (sun.w > 0.0) {
            acc = acc + exp(-(sun.xyz + half)) * d_tau;
        }
        tau_view = tau_view + d_tau;
    }
    out.inscatter = (air.sun_gain * phase * 0.25) * acc;
    out.transmit = exp(-tau_view);
    return out;
}
