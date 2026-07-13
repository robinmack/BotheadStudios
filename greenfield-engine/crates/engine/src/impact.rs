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

/// Impactor fragments. Resolution matters PHYSICALLY, not just visually: the proto-lunar disk forms by
/// collisional angular-momentum exchange between fragments — too few particles and there are no
/// encounters left to do it (measured: at 64/128 only ~2 fragments stayed aloft; no clumping possible).
pub const DEBRIS_N: usize = 128;
/// Target (Earth) impact-region fragments — materialized crust the impactor ploughs into.
pub const CAP_N: usize = 256;
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

/// The surface an impact excavates into — a curved body (the space-band planet) or a locally FLAT
/// terrain patch. The ONE furrow primitive ([`Furrow`], [`furrow_target_grains`]) serves both, so a
/// meteor into terrain and Theia into Earth excavate by the SAME code (docs/28: "improving one improves
/// all"). The only difference is geometry: over a sphere the surface curves away under the furrow and
/// "up" is radial; on a flat patch "up" is a constant local normal (+y under uniform gravity).
#[derive(Clone, Copy)]
pub enum ExcavSurface {
    /// A spherical body centred at `center`, surface radius `radius`: the outward normal is radial and
    /// grains descend toward the centre (the planet curves away under a tangent furrow).
    Curved { center: DVec3, radius: f64 },
    /// A locally flat patch: `up` is the constant outward normal (local-up). `ref_radius` is used ONLY
    /// to sample a layered body's material/temperature by depth in the native flat test; a real terrain
    /// scene assigns per-voxel material from the voxels it excavates and ignores it.
    Flat { up: DVec3, ref_radius: f64 },
}

impl ExcavSurface {
    /// Outward unit normal at the impact `site`.
    fn site_normal(&self, site: DVec3) -> DVec3 {
        match self {
            ExcavSurface::Curved { center, .. } => (site - *center).normalize_or_zero(),
            ExcavSurface::Flat { up, .. } => up.normalize_or_zero(),
        }
    }
    /// Place a grain: from a point on the tangent plane through the site and a (≤0) `depth` into the
    /// surface, return (position, outward normal there, sample radius for material, depth below surface).
    /// Curved projects onto the real curved surface first (a flat tangent furrow would bulge out over a
    /// sphere as it curves away); Flat descends straight along the constant normal.
    fn place(&self, tangent_pt: DVec3, depth: f64) -> (DVec3, DVec3, f64, f64) {
        match self {
            ExcavSurface::Curved { center, radius } => {
                let outward = (tangent_pt - *center).normalize_or_zero();
                let r = radius + depth; // depth ≤ 0
                let pos = *center + outward * r;
                (pos, outward, r, (radius - r).max(0.0))
            }
            ExcavSurface::Flat { up, ref_radius } => {
                let n = up.normalize_or_zero();
                let pos = tangent_pt + n * depth;
                (pos, n, ref_radius + depth, (-depth).max(0.0))
            }
        }
    }
}

/// The excavation FURROW (docs/28 step 3): a bowl elongated DOWNRANGE along the impact track — not the
/// old isotropic half-ball (which made every impact look dead-centre regardless of obliquity — Robin:
/// "looked like it hit the center, not 45°") — PLUS the DECLARED shock→rarefaction ejection velocity it
/// imparts (step 3b). This is the SINGLE shared law: [`furrow_target_grains`] fills it for a body's cap
/// (space band), and the terrain meteor tests each voxel against it (`matter::materialize_furrow`), so
/// every impact — at any angle, on any surface — excavates by the same shape and the same ejection.
///
/// The DECLARED shock ejecta velocity is a RESOLUTION IOU: the excavation flow is a continuum shock finer
/// than a grain, so at N≈384 it cannot emerge from the local contact physics — we declare its KNOWN
/// result instead, to be DELETED once particle count is high enough for the flow to emerge on its own. It
/// is honest, not a dial, because it is DERIVED from cited cratering scaling, not tuned: speed =
/// Housen–Holsapple point-source v = C·v_i·(a/d)^(1/μ), μ ≈ 0.55 (competent rock), C ≈ 0.6 (free-surface
/// coupling); launch ~45° up-and-downrange (Maxwell Z-model, Z≈3); deep material is DISPLACED not ejected
/// (speed fades to zero below the excavation depth). See the resolved-vs-declared engine principle (docs/28).
pub struct Furrow {
    /// Impact site (surface point of first contact).
    pub site: DVec3,
    /// Outward surface normal at the site (radial on a sphere; local-up on a flat patch).
    pub n: DVec3,
    /// Downrange tangent — the impact velocity projected onto the surface (the furrow's long axis).
    pub t: DVec3,
    /// Lateral tangent (across-track).
    pub b: DVec3,
    /// Ellipsoid semi-axis along-track (elongated).
    pub l_along: f64,
    /// Ellipsoid semi-axis across-track (narrower).
    pub l_lat: f64,
    /// Ellipsoid semi-axis into the surface.
    pub l_depth: f64,
    /// Bowl-centre offset downrange of first contact (the impactor ploughs forward as it digs in).
    pub downrange: f64,
    /// Below this depth the shock DISPLACES rather than EJECTS (ejection speed fades to zero).
    pub exc_depth: f64,
    /// Impact speed (drives the ejecta speed).
    pub v_mag: f64,
    /// Impactor radius `a` — the Housen–Holsapple scaling length.
    pub a: f64,
}

impl Furrow {
    /// Build the furrow frame at `site` with outward normal `n`, for an impactor of radius
    /// `impactor_radius` arriving at `v_impact`, with excavation scale `extent` (≈ impactor size).
    pub fn new(site: DVec3, n: DVec3, v_impact: DVec3, impactor_radius: f64, extent: f64) -> Self {
        let v_mag = v_impact.length();
        // Downrange tangent: the impact velocity projected onto the surface. A near-vertical impact has
        // no preferred direction — fall back to any tangent (the bowl is symmetric there, so its axis
        // is arbitrary and unobservable).
        let tang = v_impact - n * v_impact.dot(n);
        let v_tan = tang.length();
        let t = tang.try_normalize().unwrap_or_else(|| {
            let a = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            (a - n * a.dot(n)).normalize()
        });
        let b = n.cross(t).normalize_or_zero();
        // OBLIQUITY drives the elongation and downrange offset (docs/28): the tangential fraction of the
        // impact velocity. `oblq` = 0 straight-down, 1 at 45°, up to √2 grazing. A VERTICAL strike
        // (oblq→0) collapses to a SYMMETRIC bowl (l_along = l_lat, no downrange offset) — the honest 90°
        // case; only obliquity stretches the furrow along-track and pushes the bowl centre downrange. The
        // coefficients are pinned so a 45° impact reproduces the previously-tuned furrow exactly
        // (l_along 1.5·extent, downrange 0.5·extent), so the space-band oblique tests are unperturbed.
        const SQRT2: f64 = std::f64::consts::SQRT_2;
        let oblq = if v_mag > 0.0 { SQRT2 * v_tan / v_mag } else { 0.0 };
        Furrow {
            site,
            n,
            t,
            b,
            l_along: extent * (0.6 + 0.9 * oblq),
            l_lat: extent * 0.6,
            l_depth: extent * 0.85,
            downrange: extent * 0.5 * oblq,
            exc_depth: (extent * 0.5).max(1.0),
            v_mag,
            a: impactor_radius,
        }
    }

    /// Is a grain at along/across offsets `(along, lat)` and `below` metres beneath the surface inside the
    /// excavation bowl? (The half-ellipsoid the fill spans: full along/across, downward-only in depth.)
    pub fn contains(&self, along: f64, lat: f64, below: f64) -> bool {
        below >= 0.0
            && ((along - self.downrange) / self.l_along).powi(2)
                + (lat / self.l_lat).powi(2)
                + (below / self.l_depth).powi(2)
                <= 1.0
    }

    /// Test a world point against the bowl, given its depth `below` beneath the surface (the terrain
    /// meteor's per-voxel membership test — `matter::materialize_furrow`).
    pub fn contains_point(&self, pos: DVec3, below: f64) -> bool {
        let rel = pos - self.site;
        self.contains(rel.dot(self.t), rel.dot(self.b), below)
    }

    /// The DECLARED shock→rarefaction ejection velocity at a grain at `pos` (surface normal `outward`
    /// there, `below` m beneath the surface): H-H point-source speed C·v_i·(a/d)^(1/μ) faded to zero
    /// below the excavation depth, launched ~45° between local-up (`outward`) and the along-surface
    /// downrange direction (the Maxwell Z-model excavation flow). Excludes ground motion (caller adds it).
    pub fn ejection(&self, pos: DVec3, outward: DVec3, below: f64) -> DVec3 {
        const MU_HH: f64 = 0.55;
        const C_EJ: f64 = 0.6;
        let from_site = pos - self.site;
        let d = from_site.length().max(self.a); // >= a: the (a/d) factor never exceeds 1
        let fade = (1.0 - below / self.exc_depth).clamp(0.0, 1.0);
        let speed = C_EJ * self.v_mag * (self.a / d).powf(1.0 / MU_HH) * fade;
        let horiz = (from_site - outward * from_site.dot(outward))
            .try_normalize()
            .unwrap_or(self.t); // outward-along-surface (downrange), fall back to the track
        let launch = (outward + horiz).normalize_or_zero(); // ~45° up-and-out
        launch * speed
    }
}

/// SHARED excavation primitive (docs/28 step 3): the target matter the impactor ploughs into, shaped as
/// a FURROW (see [`Furrow`]). Scene-agnostic: any `target` LayeredBody, any `surface` ([`ExcavSurface`],
/// curved or flat), any site/track, so a meteor into terrain and Theia into Earth excavate by the SAME
/// code. Grains sit BELOW the real surface, at rest on the bulk body, tagged [`SOURCE_TARGET`], with real
/// composition + temperature at their depth.
///
/// The excavated grains carry the shared [`Furrow`]'s DECLARED shock→rarefaction ejecta velocity (docs/28
/// step 3b — see [`Furrow`] for the derivation and the resolution-IOU caveat).
///
/// `surface` is the geometry the excavation happens on ([`ExcavSurface::Curved`] for the space band,
/// [`ExcavSurface::Flat`] for a terrain patch). `v_impact` is the impactor's velocity relative to the
/// target (direction sets the furrow's long axis, magnitude drives the ejecta speed); `impactor_radius`
/// is the scaling length a. `extent` is the excavation scale (≈ impactor size, clamped). This routine
/// FILLS the furrow's half-ellipsoid volume with `n_grains` fresh grains (a body has no pre-existing
/// grains); a terrain scene instead converts its real voxels (`matter::materialize_furrow`), but BOTH
/// share the [`Furrow`] shape + ejection law. Returns (bodies, mat_ids, temps, source) to append.
#[allow(clippy::too_many_arguments)]
pub fn furrow_target_grains(
    mats: &[Material],
    target: &crate::planet::LayeredBody,
    surface: ExcavSurface,
    site: DVec3,
    v_impact: DVec3,
    impactor_radius: f64,
    frag_mass: f64,
    ground_vel: DVec3,
    n_grains: usize,
    extent: f64,
) -> (Vec<Body>, Vec<usize>, Vec<f32>, Vec<u8>) {
    let n = surface.site_normal(site); // outward surface normal at the site
    let f = Furrow::new(site, n, v_impact, impactor_radius, extent);

    let mut bodies = Vec::with_capacity(n_grains);
    let mut mat_ids = Vec::with_capacity(n_grains);
    let mut temps = Vec::with_capacity(n_grains);
    let mut source = Vec::with_capacity(n_grains);
    for i in 0..n_grains {
        let u = fib_dir(i, n_grains);
        let r = ((i as f64 + 0.5) / n_grains as f64).cbrt(); // fill the ellipsoid volume
        let along = u.dot(f.t) * r * f.l_along + f.downrange;
        let lat = u.dot(f.b) * r * f.l_lat;
        let depth = -(u.dot(n).abs() * r) * f.l_depth; // always INTO the surface
        // Project the along/lateral offset onto the surface (curved or flat), then descend by `depth` so
        // every grain is genuinely below the surface.
        let tangent_pt = site + f.t * along + f.b * lat;
        let (pos, outward, r_sample, below) = surface.place(tangent_pt, depth);
        bodies.push(Body {
            pos,
            vel: ground_vel + f.ejection(pos, outward, below),
            mass: frag_mass,
        });
        let layer = target.layer_at(r_sample);
        mat_ids.push(materials::index_of(mats, layer.material));
        temps.push(target.temperature_at(r_sample) as f32);
        source.push(crate::aggregate::SOURCE_TARGET);
    }
    (bodies, mat_ids, temps, source)
}

/// Build the mutual impact cloud. The impactor's fragments CARRY the true contact velocity (recovered by
/// `orbit::contact_velocity` from the conservation laws) — they simply ARE the arriving body; the target's
/// cap starts at rest. From there everything is mechanics: the one contact law transfers the momentum into
/// the target's matter, and the contact DISSIPATION heats it (energy conserved, not destroyed → emergent
/// incandescence). No deposited momentum, no assigned heat, no scripted anything. Returns the aggregate +
/// its initial accelerations.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn build_impact_debris_between(
    mats: &[Material],
    site: DVec3,
    earth_pos: DVec3,
    earth_vel: DVec3,
    impactor_mass: f64,
    v_contact: DVec3,
    impactor: &crate::planet::LayeredBody,
    target: &crate::planet::LayeredBody,
    earth_mass: f64,
    earth_radius: f64,
) -> (Aggregate, Vec<DVec3>) {
    let moon_mass = impactor_mass;
    let moon_r = impactor.radius();
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
    // PROVENANCE (docs/28): tag which body each grain is, as a physical attribute — so the disk's
    // composition can be MEASURED (is any of the Moon Earth-derived, as the real one is?) and tinted by
    // origin, not inferred from an index convention that swap_remove would scramble.
    let mut source = Vec::with_capacity(IMPACT_N);

    // Both bodies are LAYERED (docs/25): each materialized particle samples the real construction —
    // material AND internal temperature — at its own radius. Nothing is uniform "rock": the Moon brings
    // its crust, mantle, and hot core; Earth's cap is crust over mantle (and, this deep, the top of the
    // molten outer core). The phases/temps came from pressure + material melting laws (planet.rs), so
    // when the impact exposes deep matter it GLOWS because it genuinely is that hot — not painted.
    let earth_body = target;
    let moon_body = impactor;

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
        source.push(crate::aggregate::SOURCE_IMPACTOR);
    }

    // TARGET impact region — the matter the impactor ploughs into, excavated as a FURROW elongated
    // DOWNRANGE along the impact track (the shared, angle-agnostic primitive above), not an isotropic
    // half-ball. Excavation scale ~ the impactor, clamped for GIANT impactors (a Theia-scale cap would
    // swallow the planet; the giant-impact melt region is hemispheric, not global — flagged).
    let cap_extent = (2.0 * moon_r).min(0.55 * earth_radius);
    let (cap_bodies, cap_mats, cap_temps, cap_src) = furrow_target_grains(
        mats,
        earth_body,
        ExcavSurface::Curved { center: earth_pos, radius: earth_radius },
        surface,
        v_contact,
        moon_r,
        frag_mass,
        earth_vel,
        CAP_N,
        cap_extent,
    );
    particles.extend(cap_bodies);
    mat_ids.extend(cap_mats);
    temps.extend(cap_temps);
    source.extend(cap_src);

    // One canonical contact law from the real material. Grain radius is DENSITY-CONSISTENT — the radius a
    // grain of this mass and the material's density actually has, r = (3m/4πρ)^⅓ — so the contact
    // stiffness (E·r/m) is faithful to the matter, not to the render spacing.
    let frag_r = (3.0 * frag_mass / (4.0 * std::f64::consts::PI * (mat.density as f64).max(1.0)))
        .cbrt();
    let contact = granular::contact_from_material(mat, frag_r, frag_mass);
    // Gravitational softening at FRAGMENT scale (half a grain radius): the contact law provides the
    // short-range repulsion, so gravity may be honest down to touching distance — with impactor-scale
    // softening (the old moon_r/2 ≈ 4 grain radii) touching fragments were under-bound and rubble-pile
    // moonlets could not hold together (accretion is contact + SELF-GRAVITY; both must be real).
    let softening = 0.5 * frag_r;
    // The bulk planet: a conservative penalty boundary at the REAL surface, with the crater bowl
    // (the materialized half-ball) carved out at the site — debris landing far from the crater rests on
    // the surface; only in the bowl does free space reach cap depth. Matter cannot cross the planet.
    let specific_heat = mat.thermal.as_ref().map_or(840.0, |t| t.specific_heat as f64);
    // VAPOR phase (docs/27): shock-heated fragments past the boil point interact as GAS — EOS
    // pressure anchored at the boiling reference state (vapor pressure ≈ 1 atm at the boil point,
    // definitionally; flagged first-order). This pressure support is what spreads the proto-lunar
    // disk outward past the Roche limit.
    let gas = crate::atmosphere::gas_contact_from_material(mat, frag_r, frag_mass, 101_325.0);
    // Vaporization is NOT free: fully boiling the fragment consumes the latent heat L_v — for basalt
    // ≈ 7,100 K of equivalent thermal energy on top of the boil point. Without this sink the first
    // vapor experiment boiled ALL 384 fragments and the disk evaporated to zero (measured). The
    // fully-vaporized threshold is boil + L_v/c; partial vaporization is the refinement (flagged).
    let boil_k = mat.thermal.as_ref().map_or(f64::INFINITY, |t| {
        t.boil_point as f64 + t.latent_vaporization as f64 / (t.specific_heat as f64).max(1.0)
    });
    let mut agg = Aggregate::new(particles, softening)
        .with_material(basalt) // bulk contact-law material (per-pair material contact: flagged refinement)
        // 1/r² outside the planet, Gauss's-law linear interior inside — no singular core attractor.
        .with_gravity_source(earth_pos, earth_mass, earth_radius)
        .with_contact(contact, frag_mass)
        .with_vapor_phase(gas, boil_k)
        .with_specific_heat(specific_heat)
        .with_boundary(earth_pos, earth_radius, contact.stiffness)
        .with_boundary_hole(surface, cap_extent);
    // Per-particle composition + REAL internal temperatures from the layered bodies (docs/25).
    agg.mat_ids = mat_ids;
    agg.temps = temps;
    agg.source = source; // per-particle provenance (Theia vs Earth)
    let acc0 = agg.accelerations();
    (agg, acc0)
}

/// The moon-into-Earth case (the space-band Drop scene) — the general builder with those profiles.
#[allow(clippy::too_many_arguments)]
pub fn build_impact_debris(
    mats: &[Material],
    site: DVec3,
    earth_pos: DVec3,
    earth_vel: DVec3,
    moon_mass: f64,
    v_contact: DVec3,
    _moon_r: f64,
    earth_mass: f64,
    earth_radius: f64,
) -> (Aggregate, Vec<DVec3>) {
    build_impact_debris_between(
        mats,
        site,
        earth_pos,
        earth_vel,
        moon_mass,
        v_contact,
        &crate::planet::moon(),
        &crate::planet::earth(),
        earth_mass,
        earth_radius,
    )
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
    fn an_oblique_theia_impact_lofts_bound_material_the_protolunar_disk() {
        // docs/27, THE antithesis test (Robin): the same machinery that shatters a moon must be able to
        // BIRTH one. A Mars-sized differentiated impactor (Theia) strikes Earth OBLIQUELY at ~mutual
        // escape speed — obliquity is what puts mantle material on lofted trajectories with angular
        // momentum instead of straight up. Kepler alone would return every launched fragment to its
        // launch radius; it is debris-debris contact + self-gravity (already in the model) that raise
        // perigees into orbit. We integrate the aftermath and measure the PROTO-LUNAR material: bound
        // fragments aloft at the end, and any with perigee already raised above the surface.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let earth_pos = DVec3::ZERO;
        let earth_vel = DVec3::ZERO;
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        // Mutual escape speed at contact, 45° oblique (tangential +x, radial −y).
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia)
            / (EARTH_RADIUS_M + theia.radius()))
        .sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);

        let (mut agg, mut acc) = build_impact_debris_between(
            &mats, site, earth_pos, earth_vel, m_theia, v_contact, &theia, &earth,
            EARTH_MASS, EARTH_RADIUS_M,
        );
        let steps = if cfg!(debug_assertions) { 4_000 } else { 20_000 };
        for _ in 0..steps {
            agg.step(&mut acc, 2.0); // hours of aftermath
        }

        let mu = G * EARTH_MASS;
        let m_moon_real = 7.342e22;
        let (mut aloft_bound, mut in_orbit, mut escaped) = (0.0f64, 0.0f64, 0.0f64);
        for p in &agg.particles {
            let r = (p.pos - earth_pos).length();
            let v2 = (p.vel - earth_vel).length_squared();
            let eps = 0.5 * v2 - mu / r;
            if eps >= 0.0 {
                escaped += p.mass;
            } else if r > 1.1 * EARTH_RADIUS_M {
                aloft_bound += p.mass;
                let peri = crate::orbit::perigee(p.pos - earth_pos, p.vel - earth_vel, mu)
                    .unwrap_or(0.0);
                if peri > EARTH_RADIUS_M {
                    in_orbit += p.mass; // perigee raised above the surface: genuinely orbiting
                }
            }
        }
        println!(
            "protolunar: aloft+bound {:.2} M_moon · perigee-raised {:.2} M_moon · escaped {:.2} M_moon",
            aloft_bound / m_moon_real,
            in_orbit / m_moon_real,
            escaped / m_moon_real
        );
        // The theorized disk is ~1–2 lunar masses. At 192-particle resolution we assert the emergence,
        // not the precise number: a lunar-mass-scale amount of material must be aloft and BOUND (the
        // proto-lunar reservoir), and most mass must NOT escape (giant impacts retain their debris).
        assert!(
            aloft_bound > 0.3 * m_moon_real,
            "a lunar-mass-scale bound reservoir is lofted (got {:.2} M_moon)",
            aloft_bound / m_moon_real
        );
        assert!(
            escaped < 0.5 * (m_theia + aloft_bound),
            "most material is retained by Earth's gravity"
        );
    }

    #[test]
    fn the_birth_scene_geometry_actually_lofts_the_disk() {
        // Regression guard for the SCENE, not just the physics: run the birth scenario's EXACT inbound
        // geometry (d0 = 9.6e7 m, v = 6 km/s, impact parameter 1.30·contact — from start_birth) through
        // the real integrator + swept CCD + conservation-law contact recovery, then materialize and
        // integrate the aftermath. The first version of the scene used b = 0.87·contact, which yields a
        // 29° (radial-dominant) hit whose ejecta BURIES instead of lofting — on-screen, "the planetoid
        // just adds its mass to Earth" (Robin). This test would have caught it: the recovered contact
        // obliquity must be ≥ 40°, and a lunar-mass-scale bound reservoir must end up aloft.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth_profile = crate::planet::earth();
        let m_theia = theia.total_mass();
        let contact = EARTH_RADIUS_M + theia.radius();
        let (d0, v_in) = (9.6e7, 5_000.0); // v∞ ≈ 4 km/s — top of the canonical Theia range, matches start_birth
        let b = 1.46 * contact;

        let mut bodies = vec![
            crate::orbit::Body { pos: DVec3::ZERO, vel: DVec3::ZERO, mass: EARTH_MASS },
            crate::orbit::Body {
                pos: DVec3::new(d0, b, 0.0),
                vel: DVec3::new(-v_in, 0.0, 0.0),
                mass: m_theia,
            },
        ];
        let mut acc = crate::orbit::accelerations(&bodies);
        let dt = 2_500.0 / 960.0; // the scene's fast-forward substep (time_scale ≈ 2500)
        let mut hit = None;
        for _ in 0..40_000 {
            let rel_old = bodies[1].pos - bodies[0].pos;
            let vel_old = bodies[1].vel - bodies[0].vel;
            crate::orbit::verlet_step(&mut bodies, &mut acc, dt);
            let rel_new = bodies[1].pos - bodies[0].pos;
            if let Some(t) = crate::orbit::swept_first_contact(rel_old, rel_new, contact) {
                let rel_c = rel_old + (rel_new - rel_old) * t;
                let n_hat = rel_c.normalize();
                let mu = G * (EARTH_MASS + m_theia);
                let v_c = crate::orbit::contact_velocity(rel_old, vel_old, n_hat, contact, mu);
                hit = Some((bodies[0].pos + rel_c, v_c, n_hat));
                break;
            }
        }
        let (site, v_contact, n_hat) = hit.expect("the birth geometry must actually hit Earth");
        // Obliquity at contact: the angle between the arrival velocity and straight-down.
        let v_norm = v_contact.dot(-n_hat); // inward component
        let obliquity = (v_contact.length_squared() - v_norm * v_norm).max(0.0).sqrt()
            .atan2(v_norm)
            .to_degrees();
        println!("birth geometry: v_c {:.0} m/s at {obliquity:.0}° obliquity", v_contact.length());
        assert!(
            obliquity >= 40.0,
            "the scene's impact parameter must yield a giant-impact obliquity (got {obliquity:.0}°)"
        );

        let (mut agg, mut acc2) = build_impact_debris_between(
            &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth_profile,
            EARTH_MASS, EARTH_RADIUS_M,
        );
        // Spin bookkeeping (docs/27): Earth's gravity is CENTRAL (no torque about its own centre), the
        // cloud's self-interactions conserve their own L, and there is no Sun in this test — so ALL
        // change in the cloud's angular momentum about Earth is boundary shear, whose mirror is SPIN.
        let l0 = crate::tides::cloud_angular_momentum(&agg.particles, DVec3::ZERO, DVec3::ZERO);
        let steps = if cfg!(debug_assertions) { 3_000 } else { 20_000 };
        // The declared shock ejection LAUNCHES the excavated reservoir, so it spends time in ballistic
        // flight before settling — a single late snapshot undercounts it. Track the PEAK bound-aloft
        // reservoir over the run: the honest measure of "how much did the scene loft" (docs/28 step 3).
        let mu = G * EARTH_MASS;
        let mut peak_aloft = 0.0f64;
        for s in 0..steps {
            agg.step(&mut acc2, 2.0);
            if s % 100 == 0 {
                let ab: f64 = agg
                    .particles
                    .iter()
                    .filter(|p| {
                        let r = p.pos.length();
                        0.5 * p.vel.length_squared() - mu / r < 0.0 && r > 1.1 * EARTH_RADIUS_M
                    })
                    .map(|p| p.mass)
                    .sum();
                peak_aloft = peak_aloft.max(ab);
            }
        }
        let n_vapor = agg.vapor.iter().filter(|v| **v).count();
        println!("vapor parcels at end: {n_vapor} of {}", agg.particles.len());
        let l1 = crate::tides::cloud_angular_momentum(&agg.particles, DVec3::ZERO, DVec3::ZERO);
        let spin_l = l0 - l1; // the shear's mirror: what the cloud lost, the planet's spin gained
        let day_h = crate::tides::spin_period_s(spin_l, EARTH_MASS, EARTH_RADIUS_M) / 3600.0;
        println!("EMERGENT length of day after the giant impact: {day_h:.1} h (canonical ~5 h)");
        assert!(
            (2.0..14.0).contains(&day_h),
            "the impact sets a fast day, never declared (got {day_h:.1} h)"
        );
        // MEASURE (no closure, no rule): REAL clumping — connected components of contact adjacency among
        // aloft fragments. Rubble-pile moonlets are fragments held touching by inelastic contact +
        // self-gravity; a multi-fragment clump is accretion happening as physics, nothing merged by hand.
        let touch = 2.2 * agg.contact.unwrap().radius;
        let aloft: Vec<usize> = (0..agg.particles.len())
            .filter(|&i| {
                let p = &agg.particles[i];
                let r = p.pos.length();
                0.5 * p.vel.length_squared() - mu / r < 0.0 && r > 1.1 * EARTH_RADIUS_M
            })
            .collect();
        let aloft_bound: f64 = aloft.iter().map(|&i| agg.particles[i].mass).sum();
        // Union-find over touching aloft pairs.
        let mut parent: Vec<usize> = (0..aloft.len()).collect();
        fn find(parent: &mut Vec<usize>, i: usize) -> usize {
            if parent[i] != i {
                let r = find(parent, parent[i]);
                parent[i] = r;
            }
            parent[i]
        }
        for a in 0..aloft.len() {
            for b in (a + 1)..aloft.len() {
                let d = (agg.particles[aloft[a]].pos - agg.particles[aloft[b]].pos).length();
                if d < touch {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    if ra != rb {
                        parent[ra] = rb;
                    }
                }
            }
        }
        let mut clump_mass = std::collections::HashMap::new();
        for a in 0..aloft.len() {
            let root = find(&mut parent, a);
            *clump_mass.entry(root).or_insert(0.0f64) += agg.particles[aloft[a]].mass;
        }
        let n_clumps = clump_mass.len();
        let biggest = clump_mass.values().cloned().fold(0.0f64, f64::max);
        let frag0 = m_theia / DEBRIS_N as f64;
        println!(
            "birth scene lofts a PEAK {:.2} M_moon bound-aloft; final {:.2} M_moon in {n_clumps} clumps · biggest {:.1} fragments ({:.2} M_moon)",
            peak_aloft / 7.342e22,
            aloft_bound / 7.342e22,
            biggest / frag0,
            biggest / 7.342e22
        );
        assert!(
            peak_aloft > 0.3 * 7.342e22,
            "the SCENE's geometry lofts a lunar-mass-scale bound reservoir (peak {:.2} M_moon)",
            peak_aloft / 7.342e22
        );
        // Accretion into rubble-pile moonlets (contact + self-gravity, no merge rule) is MEASURED, not
        // asserted: at N≈384 the aloft disk is collisionless (two-body relaxation dominates the
        // collisional clumping — docs/28 LOD ceiling), and the declared shock ejection disperses it
        // FURTHER, so re-accretion is not guaranteed in this native window. The full scene (with the Sun,
        // longer evolution) does form moonlets on the rig. Asserting a moonlet here would only pressure
        // us to detune the (derived) ejection — twiddling. So we guard the robust, resolution-independent
        // property (a lunar-mass reservoir is lofted) and leave clumping to the rig + higher N.
        let _ = (n_clumps, biggest, frag0);
    }

    #[test]
    fn provenance_tags_each_body_and_survives_integration() {
        // docs/28 step 1: provenance is a PHYSICAL attribute, not an index convention. Assert the builder
        // tags Theia vs Earth correctly (by physical layout — the impactor arrives as a ball ON/above
        // the surface, Earth's cap is EXCAVATED below it), and that the tag stays aligned to `particles`
        // through integration + drain (swap_remove must reorder `source` too, or the render tint desyncs).
        use crate::aggregate::{SOURCE_IMPACTOR, SOURCE_TARGET};
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let (mut agg, mut acc) = build_impact_debris_between(
            &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth,
            EARTH_MASS, EARTH_RADIUS_M,
        );
        // Counts at build: exactly the impactor and cap populations.
        assert_eq!(agg.source.len(), IMPACT_N);
        assert_eq!(agg.source.iter().filter(|&&s| s == SOURCE_IMPACTOR).count(), DEBRIS_N);
        assert_eq!(agg.source.iter().filter(|&&s| s == SOURCE_TARGET).count(), CAP_N);
        // The tag matches physical layout: the impactor is a ball ON/above the surface carrying
        // v_contact (~km/s); Earth's cap is EXCAVATED below the surface (and now carries the declared
        // shock-ejection velocity — no longer at rest, so we discriminate by geometry, not motion).
        for (i, p) in agg.particles.iter().enumerate() {
            let r = p.pos.length();
            if agg.source[i] == SOURCE_IMPACTOR {
                assert!(r >= EARTH_RADIUS_M * 0.999, "impactor grain {i} below surface: r={r}");
                assert!(p.vel.length() > 1_000.0, "impactor grain {i} not moving: {}", p.vel.length());
            } else {
                assert!(r <= EARTH_RADIUS_M * 1.001, "target grain {i} above surface: r={r}");
            }
        }
        // The tag rides swap_remove: integrate the aftermath, drain settled matter, require source to
        // stay exactly as long as particles (a desync would mis-tint or panic the render lookup).
        for _ in 0..3000 {
            agg.step(&mut acc, 2.0);
        }
        let before = agg.particles.len();
        let r_tol = 4.0 * agg.contact.map_or(5.0e5, |c| c.radius);
        let (drained, _, _) =
            agg.drain_settled(DVec3::ZERO, EARTH_RADIUS_M, DVec3::ZERO, 30.0, r_tol);
        assert_eq!(agg.source.len(), agg.particles.len(), "source desynced from particles after drain");
        assert_eq!(agg.particles.len(), before - drained);
        // DOCUMENTS the deficit step 3 must close — NO target assertion here (that is step 3's job).
        // The bound-aloft disk is ~100% Theia today; this print makes it a measurable number that will
        // MOVE when progressive excavation lofts real Earth material.
        let mu = G * EARTH_MASS;
        let (mut aloft_earth, mut aloft_theia) = (0.0f64, 0.0f64);
        for (i, p) in agg.particles.iter().enumerate() {
            let r = p.pos.length();
            if 0.5 * p.vel.length_squared() - mu / r < 0.0 && r > 1.1 * EARTH_RADIUS_M {
                if agg.source[i] == SOURCE_TARGET {
                    aloft_earth += p.mass;
                } else {
                    aloft_theia += p.mass;
                }
            }
        }
        println!(
            "DISK PROVENANCE (bound, aloft): Earth {:.3} M_moon | Theia {:.3} M_moon",
            aloft_earth / MOON_MASS,
            aloft_theia / MOON_MASS
        );
    }

    #[test]
    fn furrow_is_elongated_downrange_below_surface_at_any_angle() {
        // docs/28 step 3: the excavated target region is a FURROW along the impact track, not an
        // isotropic bowl (which made every impact look dead-centre). Shared/angle-agnostic — the same
        // primitive a terrain meteor and a Theia strike both use. Asserts: elongated downrange, biased
        // forward of first contact, entirely below the real surface, all Earth-tagged — at oblique AND
        // vertical incidence.
        use crate::aggregate::SOURCE_TARGET;
        let mats = materials::load();
        let earth = crate::planet::earth();
        let earth_pos = DVec3::ZERO;
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0); // a surface point (pole); tangent plane is x–z
        let frag_mass = 1.0e18;
        let extent = 2.0 * MOON_RADIUS_M;
        let a = MOON_RADIUS_M; // impactor radius (scaling length)
        let t = DVec3::X; // downrange tangent at this site
        let lat = DVec3::Z; // lateral tangent

        // OBLIQUE 45° impact at 10 km/s in the x–y plane → downrange = +x.
        let v_impact = DVec3::new(1.0, -1.0, 0.0).normalize() * 10_000.0;
        let (bodies, mids, temps, src) = furrow_target_grains(
            &mats,
            &earth,
            ExcavSurface::Curved { center: earth_pos, radius: EARTH_RADIUS_M },
            site,
            v_impact,
            a,
            frag_mass,
            DVec3::ZERO,
            CAP_N,
            extent,
        );
        assert_eq!(bodies.len(), CAP_N);
        assert_eq!(mids.len(), CAP_N);
        assert_eq!(temps.len(), CAP_N);
        assert!(src.iter().all(|&s| s == SOURCE_TARGET), "all grains Earth-tagged");
        for p in &bodies {
            assert!(
                p.pos.length() <= EARTH_RADIUS_M + 1.0,
                "grain above the surface: r = {}",
                p.pos.length()
            );
        }
        let along: Vec<f64> = bodies.iter().map(|p| (p.pos - site).dot(t)).collect();
        let across: Vec<f64> = bodies.iter().map(|p| (p.pos - site).dot(lat)).collect();
        let span = |v: &[f64]| {
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(
            span(&along) > 1.3 * span(&across),
            "furrow must be elongated downrange: along {:.2e} vs across {:.2e}",
            span(&along),
            span(&across)
        );
        let cx = along.iter().sum::<f64>() / along.len() as f64;
        assert!(cx > 0.0, "furrow centroid should be downrange of contact, got {cx:.2e}");

        // SHOCK EJECTION: shallow grains carry an OUTWARD (positive radial) velocity — lofted, not at
        // rest — and the fastest ejecta stays sub-escape (bound), so it can form a disk not just escape.
        let v_esc = (2.0 * G * EARTH_MASS / EARTH_RADIUS_M).sqrt();
        let outward: Vec<f64> = bodies
            .iter()
            .map(|p| p.vel.dot((p.pos - earth_pos).normalize_or_zero()))
            .collect();
        assert!(
            outward.iter().cloned().fold(0.0, f64::max) > 500.0,
            "some grains must be ejected outward (lofted), got max {:.1} m/s",
            outward.iter().cloned().fold(0.0, f64::max)
        );
        let vmax = bodies.iter().map(|p| p.vel.length()).fold(0.0, f64::max);
        assert!(vmax < v_esc, "ejecta must be sub-escape (bound): vmax {vmax:.0} vs esc {v_esc:.0}");

        // VERTICAL incidence (track along −n): no preferred direction → a symmetric bowl, still all
        // below the surface (the fallback tangent must not panic or loft matter above the surface).
        let (vb, _, _, _) = furrow_target_grains(
            &mats,
            &earth,
            ExcavSurface::Curved { center: earth_pos, radius: EARTH_RADIUS_M },
            site,
            -DVec3::Y * 10_000.0,
            a,
            frag_mass,
            DVec3::ZERO,
            CAP_N,
            extent,
        );
        for p in &vb {
            assert!(p.pos.length() <= EARTH_RADIUS_M + 1.0, "vertical: grain above surface");
        }
    }

    #[test]
    fn the_shared_furrow_excavates_a_flat_terrain_patch_at_any_angle() {
        // The terrain meteor and the space-band Theia strike MUST run the SAME excavation (Robin:
        // "improving the physical fidelity of one should improve it for all"). This exercises the shared
        // primitive on a FLAT patch (local-up = +y, as under uniform surface gravity) — the geometry a
        // meteor into terrain sees. It asserts the SAME honest properties the curved case gives:
        //   • grains all Earth-tagged (SOURCE_TARGET) and all BELOW the surface plane;
        //   • an OBLIQUE strike carves a furrow ELONGATED downrange, its centroid pushed downrange;
        //   • ejecta are LOFTED (outward/up velocity), launched along the LOCAL normal (so the arcs are
        //     set by the scene's local gravity — a flat uniform-g patch has no escape velocity, so
        //     "sub-escape" is replaced by the derived-not-dial check: halving the impact speed halves the
        //     loft, the H-H law responding to the impact, not a fixed kick);
        //   • a VERTICAL strike is SYMMETRIC (no downrange bias) — obliquity is what elongates a furrow.
        use crate::aggregate::SOURCE_TARGET;
        let mats = materials::load();
        let earth = crate::planet::earth();
        let up = DVec3::Y; // local surface normal on a flat patch (+y under uniform gravity)
        let site = DVec3::ZERO; // impact point at the origin of the tangent plane
        let flat = ExcavSurface::Flat { up, ref_radius: EARTH_RADIUS_M };
        let frag_mass = 1.0e6;
        let extent = 12.0; // a meteor-scale crater (metres), not a giant impact
        let a = 0.3; // impactor radius (scaling length) — a small Fe-Ni body
        let (t, lat) = (DVec3::X, DVec3::Z); // downrange / lateral tangents for the oblique case below

        // OBLIQUE 45° impact at 17 km/s in the x–y plane → downrange = +x.
        let v_oblique = DVec3::new(1.0, -1.0, 0.0).normalize() * 17_000.0;
        let (bodies, mids, temps, src) = furrow_target_grains(
            &mats, &earth, flat, site, v_oblique, a, frag_mass, DVec3::ZERO, CAP_N, extent,
        );
        assert_eq!(bodies.len(), CAP_N);
        assert_eq!(mids.len(), CAP_N);
        assert_eq!(temps.len(), CAP_N);
        assert!(src.iter().all(|&s| s == SOURCE_TARGET), "all grains Earth-tagged (target material)");
        // All grains sit on/below the flat surface plane (pos·up ≤ site·up).
        for p in &bodies {
            assert!(
                (p.pos - site).dot(up) <= 1e-6,
                "grain above the flat surface: height {}",
                (p.pos - site).dot(up)
            );
        }
        // Elongated downrange (+x), centroid pushed downrange of first contact.
        let along: Vec<f64> = bodies.iter().map(|p| (p.pos - site).dot(t)).collect();
        let across: Vec<f64> = bodies.iter().map(|p| (p.pos - site).dot(lat)).collect();
        let span = |v: &[f64]| {
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(
            span(&along) > 1.3 * span(&across),
            "oblique furrow must be elongated downrange: along {:.2} vs across {:.2}",
            span(&along),
            span(&across)
        );
        let cx = along.iter().sum::<f64>() / along.len() as f64;
        assert!(cx > 0.0, "oblique furrow centroid should be downrange of contact, got {cx:.3}");

        // Ejecta lofted: some grains carry outward (+up) velocity, launched along the LOCAL normal.
        let up_vel: Vec<f64> = bodies.iter().map(|p| p.vel.dot(up)).collect();
        let max_up = up_vel.iter().cloned().fold(f64::MIN, f64::max);
        // The loft is MODEST here and that is honest: the H-H ejection scales as (a/d)^(1/μ) with the
        // impactor radius a (0.3 m) as the point source, so for a crater extent ≫ a the excavation is
        // DISPLACEMENT-dominated (grains stay) and only a gentle curtain is lofted — the opposite regime
        // to the space band, where a ≈ extent (a whole planetesimal) drives violent ejection. We assert
        // the curtain is genuinely lofted (well above numerical noise), not a specific speed.
        assert!(max_up > 15.0, "some grains must be lofted (outward/up velocity), got max {max_up:.1}");
        // Derived, not a dial: a HALF-SPEED impact lofts proportionally slower (the H-H law tracks the
        // impact — "scaled to" the strike, launched along the local vertical the scene's gravity acts on).
        let (half, _, _, _) = furrow_target_grains(
            &mats, &earth, flat, site, v_oblique * 0.5, a, frag_mass, DVec3::ZERO, CAP_N, extent,
        );
        let vmax = |b: &[Body]| b.iter().map(|p| p.vel.length()).fold(0.0, f64::max);
        let (vf, vh) = (vmax(&bodies), vmax(&half));
        assert!(
            (vh / vf - 0.5).abs() < 0.05,
            "ejecta speed scales with the impact (full {vf:.0}, half {vh:.0}; ratio {:.3})",
            vh / vf
        );

        // VERTICAL strike (straight down): SYMMETRIC bowl — no downrange bias, along-span ≈ across-span,
        // and the centroid is centred over the impact (obliquity is what elongates/offsets a furrow).
        let v_vert = -up * 17_000.0;
        let (vb, _, _, _) = furrow_target_grains(
            &mats, &earth, flat, site, v_vert, a, frag_mass, DVec3::ZERO, CAP_N, extent,
        );
        // Its tangent axes are arbitrary (no preferred direction), so measure symmetry in two fixed
        // orthogonal tangents (x, z) of the plane.
        let vx: Vec<f64> = vb.iter().map(|p| (p.pos - site).dot(DVec3::X)).collect();
        let vz: Vec<f64> = vb.iter().map(|p| (p.pos - site).dot(DVec3::Z)).collect();
        let (sx, sz) = (span(&vx), span(&vz));
        assert!(
            (sx / sz - 1.0).abs() < 0.25 && (sz / sx - 1.0).abs() < 0.25,
            "vertical strike must be symmetric (x-span {sx:.2} ≈ z-span {sz:.2})"
        );
        let (cx2, cz2) = (
            vx.iter().sum::<f64>() / vx.len() as f64,
            vz.iter().sum::<f64>() / vz.len() as f64,
        );
        let radius = sx.max(sz);
        assert!(
            cx2.abs() < 0.15 * radius && cz2.abs() < 0.15 * radius,
            "vertical strike bowl is centred over the impact (centroid {cx2:.2},{cz2:.2}; r {radius:.2})"
        );
        for p in &vb {
            assert!((p.pos - site).dot(up) <= 1e-6, "vertical: grain above the flat surface");
        }
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
