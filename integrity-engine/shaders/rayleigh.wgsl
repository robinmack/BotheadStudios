// **The Rayleigh single-scatter model — ONE implementation, shared by every shader that needs air.**
//
// WGSL has no `#include`, so this chunk is prepended in Rust (as `surface_normal.wgsl` is). It lived
// only in `sky.wgsl`, which meant Earth seen FROM THE GROUND had honest λ⁻⁴ scattering while Earth seen
// FROM SPACE had a Fresnel rim highlight standing in for it — one planet's atmosphere with two answers
// (Law II), and the space one could not produce the two things an atmosphere actually does.
//
// Both fall out of this function rather than being drawn on:
//   * a SOFT TERMINATOR — near the day/night line `mu_s` is small but positive, so the in-scatter fades
//     through a gradient instead of stopping at the geometric edge;
//   * SUNSETS — at grazing sun the path length `1/mu_v + 1/mu_s` is long, `1 − exp(−tau·path)` saturates
//     in blue first, and what is left is red.
//
// `tau` is the optical depth per band, derived from the declared atmosphere's MASS via
// `atmosphere::rayleigh_tau` (surface pressure emerges from it — a world never declares pressure).

fn rayleigh_veil(mu_v_in : f32, mu_s_in : f32, cos_theta : f32, tau : vec3<f32>, sun_gain : f32, twilight : f32) -> vec3<f32> {
    // TWILIGHT: the ground here may have turned away from the Sun, but the AIR ABOVE IT has not. The
    // shell stays lit for `twilight` radians past the geometric terminator (see `twilight_half_angle`),
    // so the day/night line is a gradient the width of the atmosphere itself rather than a knife edge.
    // Day side (mu_s > 0) is untouched: `lit` clamps to 1 there. An airless body passes twilight = 0 and
    // gets the hard terminator it should have.
    var lit = 1.0;
    if (twilight > 0.0) { lit = clamp((mu_s_in + twilight) / twilight, 0.0, 1.0); }
    else { lit = select(0.0, 1.0, mu_s_in > 0.0); }
    if (lit <= 0.0) {
        return vec3<f32>(0.0); // full night: no lit air left above this point, honestly black
    }
    let mu_v = max(mu_v_in, 0.08); // grazing cap in lieu of the true Chapman function (flagged)
    let mu_s = max(mu_s_in, 0.08);
    let phase = 0.75 * (1.0 + cos_theta * cos_theta);
    let geom = phase / (4.0 * (mu_v + mu_s)) * mu_s;
    let path = 1.0 / mu_v + 1.0 / mu_s;
    return lit * sun_gain * geom * (vec3<f32>(1.0) - exp(-tau * path));
}
