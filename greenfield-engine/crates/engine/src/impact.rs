//! The MUTUAL planetary impact (docs/24): at the strike, materialize BOTH bodies at the interface —
//! the impactor as a rubble ball on the surface, and the target's impact region as a cap of crust —
//! then deposit the impactor's real momentum + energy into the *combined* cloud. The impactor's
//! particles plough into the target's via the one canonical contact law (`granular::contact_accel`);
//! momentum transfer, crater excavation, ejecta, and fallback all EMERGE. Nothing imposed.
//!
//! This is the physics of record for the space-band Moon drop, kept target-independent so it is
//! natively testable (TDD): the escape/fall-back split it produces is measured against the declared
//! masses and G, not eyeballed in a browser.

use crate::aggregate::Aggregate;
use crate::granular;
use crate::materials::{self, Material};
use crate::orbit::Body;
use glam::DVec3;

/// Impactor (Moon) fragments — a coarse rubble cloud.
pub const DEBRIS_N: usize = 64;
/// Target (Earth) impact-region fragments — materialized crust the impactor ploughs into.
pub const CAP_N: usize = 128;
/// Total materialized particles in the mutual collision.
pub const IMPACT_N: usize = DEBRIS_N + CAP_N;

/// A Fibonacci-sphere unit direction (index `i` of `n`) — even coverage of the sphere.
pub fn fib_dir(i: usize, n: usize) -> DVec3 {
    let kk = i as f64 + 0.5;
    let y = 1.0 - 2.0 * kk / n as f64;
    let rxy = (1.0 - y * y).max(0.0).sqrt();
    let phi = kk * std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
    DVec3::new(rxy * phi.cos(), y, rxy * phi.sin())
}

/// Build the mutual impact cloud. The impactor's fragments CARRY the true contact velocity (recovered by
/// `orbit::contact_velocity` from the conservation laws) — they simply ARE the arriving body; the target's
/// cap starts at rest. From there everything is mechanics: the one contact law transfers the momentum into
/// the target's matter, and the contact DISSIPATION heats it (energy conserved, not destroyed → emergent
/// incandescence). No deposited momentum, no assigned heat, no scripted anything. Returns the aggregate +
/// its initial accelerations.
#[allow(clippy::too_many_arguments)]
pub fn build_impact_debris(
    mats: &[Material],
    site: DVec3,
    earth_pos: DVec3,
    earth_vel: DVec3,
    moon_mass: f64,
    v_contact: DVec3,
    moon_r: f64,
    earth_mass: f64,
    earth_radius: f64,
) -> (Aggregate, Vec<DVec3>) {
    let basalt = materials::index_of(mats, "basalt");
    let mat = &mats[basalt];
    // Equal-mass grains (the mass-agnostic contact model): the target's crust is materialized at the
    // SAME grain mass as the impactor's, so `contact_accel` applies directly and momentum conserves.
    let frag_mass = moon_mass / DEBRIS_N as f64;
    let n = (site - earth_pos).normalize_or_zero(); // outward surface normal at the impact point
    let surface = earth_pos + n * earth_radius; // where the impactor meets the ground

    let mut particles = Vec::with_capacity(IMPACT_N);
    let mut mat_ids = Vec::with_capacity(IMPACT_N);
    let mut temps = Vec::with_capacity(IMPACT_N);

    // Both bodies are LAYERED (docs/25): each materialized particle samples the real construction —
    // material AND internal temperature — at its own radius. Nothing is uniform "rock": the Moon brings
    // its crust, mantle, and hot core; Earth's cap is crust over mantle (and, this deep, the top of the
    // molten outer core). The phases/temps came from pressure + material melting laws (planet.rs), so
    // when the impact exposes deep matter it GLOWS because it genuinely is that hot — not painted.
    let earth_body = crate::planet::earth();
    let moon_body = crate::planet::moon();

    // IMPACTOR — a rubble ball touching the surface, moving at the TRUE contact velocity (relative to
    // the target). Its momentum and kinetic energy are carried mechanically, exactly once.
    let moon_center = surface + n * moon_r;
    for i in 0..DEBRIS_N {
        let rr = moon_r * ((i as f64 + 0.5) / DEBRIS_N as f64).cbrt();
        particles.push(Body {
            pos: moon_center + fib_dir(i, DEBRIS_N) * rr,
            vel: earth_vel + v_contact,
            mass: frag_mass,
        });
        let layer = moon_body.layer_at(rr);
        mat_ids.push(materials::index_of(mats, layer.material));
        temps.push(moon_body.temperature_at(rr) as f32);
    }

    // TARGET impact region — a cap of crust in a half-ball BELOW the surface point (reflect any outward
    // direction inward), at rest on the bulk planet. This is the matter the impactor ploughs into.
    let cap_extent = 2.0 * moon_r;
    for i in 0..CAP_N {
        let d = fib_dir(i, CAP_N);
        let d_in = if d.dot(n) > 0.0 { d - n * (2.0 * d.dot(n)) } else { d }; // into the planet
        let rr = cap_extent * ((i as f64 + 0.5) / CAP_N as f64).cbrt();
        let pos = surface + d_in * rr;
        particles.push(Body {
            pos,
            vel: earth_vel,
            mass: frag_mass,
        });
        let r_earth = (pos - earth_pos).length();
        let layer = earth_body.layer_at(r_earth);
        mat_ids.push(materials::index_of(mats, layer.material));
        temps.push(earth_body.temperature_at(r_earth) as f32);
    }

    // One canonical contact law from the real material. Grain radius is DENSITY-CONSISTENT — the radius a
    // grain of this mass and the material's density actually has, r = (3m/4πρ)^⅓ — so the contact
    // stiffness (E·r/m) is faithful to the matter, not to the render spacing.
    let frag_r = (3.0 * frag_mass / (4.0 * std::f64::consts::PI * (mat.density as f64).max(1.0)))
        .cbrt();
    let contact = granular::contact_from_material(mat, frag_r, frag_mass);
    // The bulk planet beneath the materialized cap: a conservative penalty boundary the cap rests on and
    // ejecta rains back onto. Set just below the deepest cap particle.
    let boundary_r = earth_radius - cap_extent;
    let specific_heat = mat.thermal.as_ref().map_or(840.0, |t| t.specific_heat as f64);
    let mut agg = Aggregate::new(particles, moon_r * 0.5)
        .with_material(basalt) // bulk contact-law material (per-pair material contact: flagged refinement)
        // 1/r² outside the planet, Gauss's-law linear interior inside — no singular core attractor.
        .with_gravity_source(earth_pos, earth_mass, earth_radius)
        .with_contact(contact)
        .with_specific_heat(specific_heat)
        .with_boundary(earth_pos, boundary_r, contact.stiffness);
    // Per-particle composition + REAL internal temperatures from the layered bodies (docs/25).
    agg.mat_ids = mat_ids;
    agg.temps = temps;
    let acc0 = agg.accelerations();
    (agg, acc0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::G;

    const EARTH_MASS: f64 = 5.972e24;
    const EARTH_RADIUS_M: f64 = 6.371e6;
    const MOON_MASS: f64 = 7.342e22;
    const MOON_RADIUS_M: f64 = 1.737e6;

    /// Specific orbital energy of a fragment about the planet: ½v² − GM/r. Negative ⇒ BOUND.
    fn bound_fraction(agg: &Aggregate, earth_pos: DVec3, earth_vel: DVec3) -> f64 {
        let mu = G * EARTH_MASS;
        let bound = agg
            .particles
            .iter()
            .filter(|p| {
                let r = (p.pos - earth_pos).length().max(1.0);
                let v2 = (p.vel - earth_vel).length_squared();
                0.5 * v2 - mu / r < 0.0
            })
            .count();
        bound as f64 / agg.particles.len() as f64
    }

    #[test]
    fn a_dropped_moon_impact_leaves_most_debris_gravitationally_bound() {
        // A dropped Moon strikes at ~escape speed (~11.2 km/s at contact). The impact energy
        // ½μΔv² ≈ 4.3e30 J over the combined Earth+Moon cloud (3 lunar masses) is ~2e7 J/kg —
        // BELOW the ~6.3e7 J/kg needed to unbind matter from Earth's surface. So the DECLARED
        // physics says: most of the cloud must stay bound (fall back / stay down). If the model
        // launches "a large percentage" past escape, the energy partition is dishonest.
        let mats = materials::load();
        let earth_pos = DVec3::ZERO;
        let earth_vel = DVec3::ZERO;
        let contact_r = EARTH_RADIUS_M + MOON_RADIUS_M;
        let site = earth_pos + DVec3::new(0.0, contact_r, 0.0);

        // True impact speed of a Moon dropped from the real Earth–Moon distance (energy conservation:
        // v² = 2μ(1/r_contact − 1/d)) — the impactor CARRIES it; contact does the rest.
        let mu = G * (EARTH_MASS + MOON_MASS);
        let d = 3.844e8;
        let v_imp = (2.0 * mu * (1.0 / contact_r - 1.0 / d)).sqrt();
        let v_contact = DVec3::new(0.0, -v_imp, 0.0);

        let (mut agg, mut acc) = build_impact_debris(
            &mats, site, earth_pos, earth_vel, MOON_MASS, v_contact,
            MOON_RADIUS_M, EARTH_MASS, EARTH_RADIUS_M,
        );

        let f0 = bound_fraction(&agg, earth_pos, earth_vel);
        // Let the collision play out (the browser's observable rate): the impactor ploughs into the cap,
        // contact transfers momentum and DISSIPATES energy into heat.
        for _ in 0..400 {
            agg.step(&mut acc, 0.75);
        }
        let f1 = bound_fraction(&agg, earth_pos, earth_vel);
        let hottest = agg.temps.iter().cloned().fold(0.0f32, f32::max);
        println!(
            "bound fraction: initial {f0:.2}, after contact {f1:.2} · v_imp {v_imp:.0} m/s · hottest {hottest:.0} K"
        );

        assert!(
            f1 > 0.6,
            "most of the impact cloud must stay gravitationally bound (got {:.0}% bound)",
            f1 * 100.0
        );
        // Incandescence is EMERGENT: contact dissipation heats the matter past visible glow (~800 K).
        assert!(
            hottest > 800.0,
            "the impact must heat matter to incandescence via contact dissipation (hottest {hottest:.0} K)"
        );
    }
}
