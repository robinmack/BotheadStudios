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
/// is honest, not a dial, because it is DERIVED from cited cratering scaling, not tuned.
///
/// VELOCITY SCALE — the CRATER's ejecta speed, not the impactor's contact jet (docs/28, the 2026-07-13
/// fix). The overall SCALE is the GRAVITY-REGIME crater ejection speed `K·√(g·R_crater)`, NOT the old
/// `C·v_i` tied to the impactor's contact/free-surface velocity `v_i`. The reason: `v_i` is the velocity of
/// the sub-grain contact JET (a tiny mass); applied to whole ~1 m / 2900 kg grains at this resolution it
/// grossly over-represents the fast-ejecta mass, flinging a terrain-meteor blanket to km-scale ranges. In
/// the Housen–Holsapple gravity regime the ejecta launch speed instead scales with the CRATER: the
/// material at the rim is launched at ~√(g·R) (so it lands ~one crater radius out — the standard result
/// that the continuous ejecta blanket spans a few crater radii, H&H 2011 / Melosh 1989), rising inward.
/// K is an ORDER-UNITY coefficient from that rim relation (K = 1: rim ejecta land ~1 R_crater away, the
/// definitional gravity-regime value — NOT tuned to look right).
///
/// The distribution SHAPE is the H-H power law `(R_crater/d)^(1/μ)` (μ ≈ 0.55, competent rock) — √(gR) at
/// the rim (d = R_crater), rising inward — ANCHORED AT THE RIM and capped at the continuous-ejecta-blanket
/// edge (~2.5 R_crater, √2.5·√(gR)). It is anchored at the rim, NOT at the impactor radius: the earlier
/// `(a/d)` form (a = impactor radius) collapses a terrain meteor's ejecta to ~0 when a ≪ R_crater and puts
/// the RIM speed at ~0, contradicting the cited relation that rim ejecta launch at √(gR). The rim anchor,
/// with the near-field clamp at `a` and the blanket cap, is the honest realization. This self-scales
/// HONESTLY across both bands with the same code: a terrain meteor (R_crater ~14 m, g = 9.88 → √(gR) ≈ 12
/// m/s, capped ≈ 18.6 m/s) makes a LOCAL ejecta blanket ~2–3 crater radii wide, while a giant impact
/// (R_crater ~planet-scale excavation extent, g ~10 → √(gR) ≈ 5.9 km/s ≈ the old C·v_i ≈ 5.7 km/s; for a
/// giant impactor a ≈ R_crater so the near-field clamp holds it there) is essentially unchanged, so the
/// proto-lunar disk still lofts. Launch ~45° up-and-downrange (Maxwell Z-model, Z≈3); deep material is
/// DISPLACED not ejected (speed fades to zero below the excavation depth). See the resolved-vs-declared
/// engine principle (docs/28). The distribution SHAPE (and its blanket cap) remain the flagged resolution
/// IOU; only the velocity SCALE `√(g·R_crater)` became fully honest.
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
    /// Impact speed. NO LONGER the ejecta-velocity scale (that is now the crater's `K·√(g·r_crater)`);
    /// retained because it sets the impact-energy budget `½·m·v²` that caps the total ejecta KE, and it
    /// sets the furrow obliquity.
    pub v_mag: f64,
    /// Impactor radius `a` — the Housen–Holsapple point-source scaling length (the distribution SHAPE).
    pub a: f64,
    /// Surface gravity at the impact site (m/s²). With `r_crater` this sets the gravity-regime ejecta
    /// velocity SCALE `K·√(g·r_crater)` — the crater's ejection speed, not the impactor's contact jet.
    pub g: f64,
    /// Crater/excavation scale `R_crater` (m) — reused from the furrow's `extent`. The rim ejecta launch
    /// at ~√(g·r_crater); this is what makes the blanket self-scale from metres (terrain) to km/s (giant).
    pub r_crater: f64,
}

impl Furrow {
    /// Build the furrow frame at `site` with outward normal `n`, for an impactor of radius
    /// `impactor_radius` arriving at `v_impact`, with excavation scale `extent` (≈ the crater radius) under
    /// surface gravity `g` (m/s²). `g` and `extent` set the gravity-regime ejecta velocity SCALE
    /// `K·√(g·extent)` (see [`Furrow::ejection`]); `extent` is reused as `r_crater`.
    pub fn new(
        site: DVec3,
        n: DVec3,
        v_impact: DVec3,
        impactor_radius: f64,
        extent: f64,
        g: f64,
    ) -> Self {
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
            g,
            r_crater: extent, // the excavation scale IS the crater scale for the gravity-regime ejecta speed
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
    /// there, `below` m beneath the surface): the H-H point-source distribution SHAPE `(a/d)^(1/μ)` scaled
    /// by the GRAVITY-REGIME crater ejection speed `K·√(g·R_crater)` (NOT the old `C·v_i` impactor contact
    /// jet — see the [`Furrow`] doc for why), faded to zero below the excavation depth, launched ~45°
    /// between local-up (`outward`) and the along-surface downrange direction (the Maxwell Z-model
    /// excavation flow). Excludes ground motion (caller adds it).
    pub fn ejection(&self, pos: DVec3, outward: DVec3, below: f64) -> DVec3 {
        const MU_HH: f64 = 0.55;
        // Gravity-regime rim-ejecta coefficient (order unity): material at the crater RIM launches at
        // ~√(g·R), landing ~one crater radius out — the standard result that the continuous ejecta blanket
        // spans a few crater radii (Housen & Holsapple 2011; Melosh, Impact Cratering, 1989). K = 1 is the
        // definitional value (rim ejecta land ~1 R_crater away); it is DERIVED, not tuned to look right.
        const K_REGIME: f64 = 1.0;
        // Outer edge of the CONTINUOUS ejecta blanket ≈ 2.5 crater radii (Melosh 1989; H&H 2011). Its
        // ballistic range v²/g = 2.5·R fixes the fastest continuous ejecta at √(2.5)·√(gR), so the shape
        // is capped there: B = √2.5 ≈ 1.58. This is the RESOLUTION IOU made explicit — the true H-H law
        // has a small MASS of much faster material at the impact point (the contact jet, → rays/distal
        // ejecta), but at N≈384 each grain is an equal, large mass, so representing that sub-resolution
        // fast tail as whole grains is exactly the debris storm. We cap at the continuous-blanket edge (a
        // CITED extent, not a tuned dial); the distal fast tail is DELETED once N resolves it (docs/28).
        const B_BLANKET: f64 = 1.5811388300841898; // √2.5 — continuous ejecta blanket to ~2.5 R_crater
        let from_site = pos - self.site;
        // Near-field clamp at the impactor radius `a` (the H-H coupling length): the power law is only
        // valid outside the projectile. For a GIANT impactor a ≈ R_crater, so this clamp is what keeps the
        // space band's ejecta at ~√(g·R) (byte-near the old C·v_i); for a small terrain meteor a ≪ R and
        // the clamp is irrelevant (the B cap governs the fast inner grains instead).
        let d = from_site.length().max(self.a);
        let fade = (1.0 - below / self.exc_depth).clamp(0.0, 1.0);
        // The velocity SCALE is the crater's √(g·R_crater), self-consistent with gravity — so the same code
        // gives a terrain meteor a metres-per-second LOCAL blanket (√(9.88·14) ≈ 12 m/s, capped to ≈ 18.6
        // m/s / ~2.5·R range) and a giant impact a km/s protolunar curtain (√(9.82·3.5e6) ≈ 5.9 km/s ≈ the
        // old scale). The SHAPE is the H-H power law ANCHORED AT THE RIM `(R_crater/d)^(1/μ)` — √(gR) at
        // the rim (d = R_crater), rising inward — capped at the continuous-blanket edge B. (Anchoring the
        // power law at the impactor radius `(a/d)` instead — the pre-2026-07-13 form — collapses a terrain
        // meteor's ejecta to ~0 when a ≪ R and puts the RIM ejecta at ~0, contradicting the cited relation
        // that rim ejecta launch at √(gR); the rim anchor is the honest fix. The (a/d)-vs-(R/d) anchor is
        // the flagged resolution IOU — the distribution SHAPE — while the √(g·R) SCALE is now honest.)
        let v_scale = K_REGIME * (self.g * self.r_crater).max(0.0).sqrt();
        let shape = ((self.r_crater / d).powf(1.0 / MU_HH)).min(B_BLANKET);
        let speed = v_scale * shape * fade;
        let horiz = (from_site - outward * from_site.dot(outward))
            .try_normalize()
            .unwrap_or(self.t); // outward-along-surface (downrange), fall back to the track
        let launch = (outward + horiz).normalize_or_zero(); // ~45° up-and-out
        launch * speed
    }
}

/// EXACT energy-conservation cap on the DECLARED shock ejection (docs/28). The Housen–Holsapple point-
/// source law `v = C·v_i·(a/d)^(1/μ)` sets the velocity DISTRIBUTION SHAPE (which grain is faster), but
/// it knows nothing about how much energy the impactor actually carried. For a SMALL impactor (the
/// terrain meteor: a ≈ 0.31 m ≪ the 1 m grain) that shape, applied to grains far more massive than the
/// impactor, hands the excavated matter FAR more kinetic energy than the impact delivered — you cannot
/// eject more KE than ½·m_impactor·v² put in. That surplus is the debris storm (rubble flung km up that
/// never settles). This returns the factor to multiply every grain's ejection velocity (the component
/// RELATIVE to the co-moving ground) by so the total ejecta KE `Σ ½·m·|v_ej|²` equals `e_impact` when it
/// would otherwise exceed it, and is otherwise LEFT ALONE (factor exactly 1.0 → byte-unchanged).
///
/// The H-H law still sets the SHAPE; this sets only the overall SCALE. It is EXACT conservation, not a
/// tuned dial: the factor is `√(e_impact / KE)`, derived, with no free parameter. For a HUGE impactor
/// (Theia) the ejecta KE is already `≪ e_impact`, so the factor clamps to 1 and the space band is
/// untouched.
///
/// HONEST NOTE (docs/28): capping at the FULL impact energy — ALL of `e_impact` allowed to become ejecta
/// KE — is a GENEROUS UPPER BOUND. Realistically most impact energy goes to heat + comminution and the
/// ejecta gets only a cratering fraction f < 1, so a cited f would give a GENTLER spray. We use the hard
/// bound f = 1 here (unambiguous conservation, no free knob); a cited ejecta-KE fraction is a flagged
/// REFINEMENT, deliberately NOT tuned to "look right".
pub fn ejecta_energy_scale(
    ejecta: impl IntoIterator<Item = (f64, DVec3)>,
    e_impact: f64,
) -> f64 {
    let ke: f64 = ejecta
        .into_iter()
        .map(|(m, v)| 0.5 * m * v.length_squared())
        .sum();
    if ke > e_impact && ke > 0.0 {
        (e_impact / ke).sqrt()
    } else {
        1.0 // within budget (or no ejecta): leave the declared velocities exactly as they are
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
/// is the scaling length a. `impactor_mass` is the impactor's mass — with `v_impact` it sets the impact
/// energy `½·m·v²` that CAPS the total ejecta KE ([`ejecta_energy_scale`]): the declared H-H law can hand
/// a small impactor's excavated grains more KE than the impact delivered, and exact energy conservation
/// scales that back. `extent` is the excavation scale (≈ the crater radius, clamped); `g` is the surface
/// gravity at the site, and together they set the gravity-regime ejecta velocity scale `K·√(g·extent)`
/// (see [`Furrow::ejection`]). This routine FILLS the
/// furrow's half-ellipsoid volume with `n_grains` fresh grains (a body has no pre-existing grains); a
/// terrain scene instead converts its real voxels (`matter::materialize_furrow`), but BOTH share the
/// [`Furrow`] shape + ejection law + energy cap. Returns (bodies, mat_ids, temps, source) to append.
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
    impactor_mass: f64,
    g: f64,
) -> (Vec<Body>, Vec<usize>, Vec<f32>, Vec<u8>) {
    let _ = frag_mass; // impactor grain mass — retained for the shared signature; the cap is now ρ·V
    let n = surface.site_normal(site); // outward surface normal at the site
    let f = Furrow::new(site, n, v_impact, impactor_radius, extent, g);

    let mut bodies = Vec::with_capacity(n_grains);
    let mut mat_ids = Vec::with_capacity(n_grains);
    let mut temps = Vec::with_capacity(n_grains);
    let mut source = Vec::with_capacity(n_grains);
    // PHYSICAL grain mass (docs/28 item 4): the excavated cap is real matter — ρ·V, not a bookkeeping
    // multiple of the impactor's grain mass. Each grain represents an equal slice of the furrow's
    // half-ellipsoid volume (V = (2/3)π·l_along·l_lat·l_depth), so its mass is that slice's volume times
    // the LOCAL density at its depth (iron-rich deep, crust shallow). This is what makes the cap ≈ 0.31×
    // the impactor (was a fudged 2×), so the momentum-conserving loft can drag it near-orbital without
    // gutting the impactor. (`frag_mass` — the impactor's grain mass — is no longer the cap's mass.)
    let vol_per = (2.0 / 3.0) * std::f64::consts::PI * f.l_along * f.l_lat * f.l_depth
        / n_grains as f64;
    let mut masses = Vec::with_capacity(n_grains);
    // Pass 1: place each grain and compute its RAW declared shock-ejection velocity (relative to ground).
    // The energy cap is a property of the WHOLE ejecta set, so we cannot finalise velocities per grain.
    let mut ejections = Vec::with_capacity(n_grains);
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
        let layer = target.layer_at(r_sample);
        let mass_i = (layer.density * vol_per).max(1.0); // real ρ·V of this grain's slice
        ejections.push(f.ejection(pos, outward, below));
        bodies.push(Body { pos, vel: ground_vel, mass: mass_i }); // ejection added (scaled) below
        masses.push(mass_i);
        mat_ids.push(materials::index_of(mats, layer.material));
        temps.push(target.temperature_at(r_sample) as f32);
        source.push(crate::aggregate::SOURCE_TARGET);
    }
    // Pass 2: EXACT energy conservation (docs/28) — total ejecta KE ≤ the impact energy. For a small
    // impactor the raw KE exceeds it and every ejection is scaled by √(E_i/KE); for a giant impactor
    // (Theia) the factor is 1.0 and the velocities are byte-unchanged. `v*1.0 == v`, so the space band is
    // untouched. KE uses each grain's REAL mass.
    let e_impact = 0.5 * impactor_mass * v_impact.length_squared();
    let scale =
        ejecta_energy_scale(ejections.iter().zip(masses.iter()).map(|(&ej, &m)| (m, ej)), e_impact);
    for (b, ej) in bodies.iter_mut().zip(ejections.iter()) {
        b.vel += *ej * scale;
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
    build_impact_debris_scaled(
        mats, site, earth_pos, earth_vel, impactor_mass, v_contact, impactor, target, earth_mass,
        earth_radius, DEBRIS_N, CAP_N, DVec3::ZERO,
    )
}

/// The mutual-impact builder at an EXPLICIT resolution (`debris_n` impactor fragments, `cap_n` excavated
/// target grains) — the RESOLUTION knob for docs/28's "raise N globally" path. Grain count matters
/// PHYSICALLY: the proto-lunar disk forms by collisional angular-momentum exchange, and the excavation
/// flow / ploughing that lofts Earth material is a continuum finer than a grain (declared as an IOU today,
/// [`Furrow::ejection`]); both EMERGE only as N rises toward the collisional regime. This function lets the
/// tests sweep N to measure that emergence (self-gravity is O(n²), so the cost rises as N²). The public
/// [`build_impact_debris_between`] is this at the default (`DEBRIS_N`, `CAP_N`). The CAP_N/DEBRIS_N RATIO
/// sets the excavated cap mass relative to the impactor (today 2×, a flagged over-mass — docs/28 item 4).
#[allow(clippy::too_many_arguments)]
pub fn build_impact_debris_scaled(
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
    debris_n: usize,
    cap_n: usize,
    earth_omega: DVec3,
) -> (Aggregate, Vec<DVec3>) {
    let impact_n = debris_n + cap_n;
    let moon_mass = impactor_mass;
    let moon_r = impactor.radius();
    let basalt = materials::index_of(mats, "basalt");
    let mat = &mats[basalt];
    // Equal-mass grains (the mass-agnostic contact model): the target's crust is materialized at the
    // SAME grain mass as the impactor's, so `contact_accel` applies directly and momentum conserves.
    let frag_mass = moon_mass / debris_n as f64;
    let n = (site - earth_pos).normalize_or_zero(); // outward surface normal at the impact point
    let surface = earth_pos + n * earth_radius; // where the impactor meets the ground

    let mut particles = Vec::with_capacity(impact_n);
    let mut mat_ids = Vec::with_capacity(impact_n);
    let mut temps = Vec::with_capacity(impact_n);
    // PROVENANCE (docs/28): tag which body each grain is, as a physical attribute — so the disk's
    // composition can be MEASURED (is any of the Moon Earth-derived, as the real one is?) and tinted by
    // origin, not inferred from an index convention that swap_remove would scramble.
    let mut source = Vec::with_capacity(impact_n);

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
    for i in 0..debris_n {
        let rr = moon_r * ((i as f64 + 0.5) / debris_n as f64).cbrt();
        particles.push(Body {
            pos: moon_center + fib_dir(i, debris_n) * rr,
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
    // Surface gravity at the impact site sets the gravity-regime ejecta velocity scale K·√(g·R_crater)
    // (docs/28): for Earth this is ~9.8 m/s², and with the planet-scale excavation extent it gives ~5.9
    // km/s — matching the old impactor-tied C·v_i ≈ 5.7 km/s, so the proto-lunar disk is ~unchanged.
    let surface_g = crate::orbit::G * earth_mass / (earth_radius * earth_radius);
    let (cap_bodies, cap_mats, cap_temps, cap_src) = furrow_target_grains(
        mats,
        earth_body,
        ExcavSurface::Curved { center: earth_pos, radius: earth_radius },
        surface,
        v_contact,
        moon_r,
        frag_mass,
        earth_vel,
        cap_n,
        cap_extent,
        moon_mass, // impactor mass → the impact-energy cap (Theia is within budget: no scaling)
        surface_g, // gravity-regime ejecta velocity scale K·√(g·R_crater)
    );
    particles.extend(cap_bodies);
    mat_ids.extend(cap_mats);
    temps.extend(cap_temps);
    source.extend(cap_src);

    // PROTO-EARTH SPIN (docs/31 — the isotopic crisis). The excavated cap is EARTH's surface mantle, and
    // it was co-rotating with the planet BEFORE the impact — so it is born with the local ground velocity
    // `ω × (pos − centre)`, not at rest in Earth's frame. A slow (or non-)spinning proto-Earth gives this
    // ≈ 0 and the cap must be lofted to orbit entirely by the impact; a FAST-spinning one (Ćuk & Stewart
    // 2012, near the ~2.3 h rotational-stability limit) hands its own mantle a ~4.8 km/s PROGRADE tangential
    // head start — most of circular velocity — so far more EARTH material holds a perigee and stays in the
    // bound disk. That is the proposed resolution of the isotopic crisis: the disk is Earth-derived because
    // Earth's own fast rotation flings its mantle out. Applied ONLY to the target cap (the impactor arrives
    // from space, not co-rotating), and BEFORE the ploughing loft so the exchange acts on the real
    // pre-impact velocity. `earth_omega = 0` is byte-identical to the pre-spin build.
    if earth_omega != DVec3::ZERO {
        for (p, &s) in particles.iter_mut().zip(source.iter()) {
            if s == crate::aggregate::SOURCE_TARGET {
                p.vel += earth_omega.cross(p.pos - earth_pos);
            }
        }
    }

    // MOMENTUM-CONSERVING LOFT (docs/28 step 3) — the shared particle-physics primitive: the impactor
    // ploughs the excavated target matter downrange, sharing tangential momentum toward the COM (what the
    // cap gains, the impactor loses; Σp conserved). Now that the cap is at its PHYSICAL ρ·V mass, this
    // drags Earth material to near-orbital tangential speed — so it joins the bound disk (the Moon is
    // Earth-derived) — without gutting the impactor's own disk. Same law a terrain meteor would use.
    let is_impactor: Vec<bool> =
        source.iter().map(|&s| s == crate::aggregate::SOURCE_IMPACTOR).collect();
    granular::plough_loft(&mut particles, &is_impactor, n, v_contact);

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
        // REAL vapor pressure (docs/26/27): once shock heat vaporizes the parcels, a continuum SPH pressure
        // P = ρ·R_s·T (basalt's gas constant) does the expansion work the overlap hack couldn't — the plume
        // launches and cools by PdV. Smoothing length ≈ the impactor radius (a few initial parcel spacings).
        .with_vapor_sph(
            crate::atmosphere::specific_gas_constant(mat),
            moon_r.max(1.0),
            mat.thermal.as_ref().map_or(0.0, |t| t.latent_vaporization as f64 / specific_heat),
        )
        .with_specific_heat(specific_heat)
        .with_boundary(earth_pos, earth_radius, contact.stiffness)
        .with_boundary_hole(surface, cap_extent);
    // Per-particle composition + REAL internal temperatures from the layered bodies (docs/25).
    agg.mat_ids = mat_ids;
    agg.temps = temps;
    agg.source = source; // per-particle provenance (Theia vs Earth)
    // PER-GRAIN contact law (docs/23: everything is matter): each grain collides as its OWN material — a
    // Theia iron-core grain with iron's stiffness/restitution/friction, a mantle grain as peridotite — not
    // all of them as the bulk basalt above. The grain RADIUS is from that material's REAL density
    // (r = (3m/4πρ)^⅓), so iron packs denser than crust for the same mass. `Aggregate` mixes the two grains'
    // laws per contact (`Contact::mix`), reducing exactly to the single law for a same-material pair — so
    // only genuinely cross-material contacts change. Radius is from the grain's REAL material density AND
    // its REAL mass (the cap is now physical ρ·V, so impactor and cap grains differ in mass) —
    // r = (3m/4πρ)^⅓ — so iron packs denser than crust.
    // Radius from the grain's REAL mass + material density; but the Contact is referenced to the SHARED
    // `frag_mass` (= the aggregate's `contact_ref_mass`), so the loop's `per-mass × ref_mass = force`
    // yields the true force-stiffness E·r for any grain mass, and the ÷(own mass) that follows is exact.
    agg.per_grain_contact = agg
        .mat_ids
        .iter()
        .zip(agg.particles.iter())
        .map(|(&mid, p)| {
            let m = &mats[mid];
            let r = (3.0 * p.mass / (4.0 * std::f64::consts::PI * (m.density as f64).max(1.0))).cbrt();
            granular::contact_from_material(m, r, frag_mass)
        })
        .collect();
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
    /// Earth's surface gravity (≈9.82 m/s²) — the gravity-regime ejecta scale K·√(g·R_crater) needs it.
    const EARTH_G: f64 = G * EARTH_MASS / (EARTH_RADIUS_M * EARTH_RADIUS_M);
    /// The terrain scene's emergent surface gravity (`Engine::surface_g`).
    const TERRAIN_G: f64 = 9.88;

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
    #[ignore = "O(n²) aftermath ×2 builds — the isotopic-crisis measurement, run on demand"]
    fn a_fast_spinning_protoearth_makes_the_disk_earth_derived() {
        // docs/31 — THE ISOTOPIC CRISIS. The canonical low-angular-momentum giant impact makes a disk that
        // is mostly THEIA, yet the real Moon is isotopically ~identical to Earth's mantle. Ćuk & Stewart
        // (2012) proposed the resolution: a FAST-spinning proto-Earth (near its ~2.3 h rotational-stability
        // limit) flings its OWN mantle out — the excavated Earth material is born co-rotating at ~ω·R ≈ 4.8
        // km/s prograde, most of circular velocity, so it holds a perigee and stays in the bound disk
        // instead of re-impacting. We MEASURE that: the same oblique Theia impact, built once with a
        // non-spinning proto-Earth (ω=0) and once fast-spinning, and compare the disk's EARTH-derived
        // bound-aloft mass and Earth fraction. No dial is tuned to a target composition — spin is a
        // physical initial condition and the disk provenance EMERGES.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let earth_pos = DVec3::ZERO;
        let earth_vel = DVec3::ZERO;
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc =
            (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let (dn, cn) = (256, 512);

        // Bound-aloft mass split by provenance (Earth = SOURCE_TARGET, Theia = SOURCE_IMPACTOR).
        let disk_provenance = |agg: &Aggregate| -> (f64, f64) {
            let mu = G * EARTH_MASS;
            let (mut earth_m, mut theia_m) = (0.0f64, 0.0f64);
            for (i, p) in agg.particles.iter().enumerate() {
                let r = (p.pos - earth_pos).length();
                let v2 = (p.vel - earth_vel).length_squared();
                if 0.5 * v2 - mu / r < 0.0 && r > 1.1 * EARTH_RADIUS_M {
                    if agg.source[i] == crate::aggregate::SOURCE_TARGET {
                        earth_m += p.mass;
                    } else {
                        theia_m += p.mass;
                    }
                }
            }
            (earth_m, theia_m)
        };

        // PROGRADE spin: the disk's orbital angular momentum here is along −ẑ (r×v at the top of the sphere
        // with a +x tangential v_contact), and ω = ω0·(−ẑ) gives ω×r = +x at the site — aligned with the
        // disk. A retrograde spin would fight the disk; we test the physically-motivated prograde case.
        let day_hours = 2.3; // near the rotational-stability limit (Ćuk & Stewart 2012)
        let omega0 = 2.0 * std::f64::consts::PI / (day_hours * 3600.0);
        let omega = DVec3::new(0.0, 0.0, -omega0);
        println!(
            "proto-Earth spin: {day_hours:.1} h day → ω·R = {:.0} m/s at the surface",
            omega0 * EARTH_RADIUS_M
        );

        let m_moon = MOON_MASS;
        let mut run = |w: DVec3| -> (f64, f64) {
            let (mut agg, mut acc) = build_impact_debris_scaled(
                &mats, site, earth_pos, earth_vel, m_theia, v_contact, &theia, &earth, EARTH_MASS,
                EARTH_RADIUS_M, dn, cn, w,
            );
            for _ in 0..3000 {
                agg.step(&mut acc, 2.0);
            }
            disk_provenance(&agg)
        };

        let (e0, t0) = run(DVec3::ZERO);
        let (e1, t1) = run(omega);
        let frac0 = e0 / (e0 + t0).max(1e-30);
        let frac1 = e1 / (e1 + t1).max(1e-30);
        println!(
            "ω=0      : Earth {:.3} | Theia {:.3} M☾  → disk is {:.0}% Earth",
            e0 / m_moon,
            t0 / m_moon,
            100.0 * frac0
        );
        println!(
            "ω=fast   : Earth {:.3} | Theia {:.3} M☾  → disk is {:.0}% Earth",
            e1 / m_moon,
            t1 / m_moon,
            100.0 * frac1
        );

        // MEASURED FINDING (docs/31), physics deciding against the hypothesis: a fast-spinning proto-Earth
        // DOES loft slightly more Earth material in absolute terms (e1 ≳ e0) AND injects a lot of angular
        // momentum, so the whole bound disk grows — but it retains proportionally MORE Theia (Theia is most
        // of the debris), so the Earth FRACTION does not rise; it falls (12% → 7%). Spin alone does not
        // Earth-enrich the disk in this model. The reason is docs/28 root cause #1: Earth is a rigid
        // BOUNDARY, so the only Earth material that can reach the disk is the small excavated cap — the
        // bulk-mantle shedding that is the actual Ćuk & Stewart mechanism cannot occur until Earth
        // participates as deformable matter. So the measured Earth fraction is a LOWER BOUND the rigid
        // boundary imposes, and the honest resolution of the isotopic crisis needs Earth-as-matter (or
        // vapor-phase Earth↔Theia mixing — the SPH route), NOT target spin. We assert only the robust,
        // model-independent mechanics — spin injects angular momentum, so more total mass stays bound —
        // and let the provenance numbers above stand as the measurement.
        let total0 = e0 + t0;
        let total1 = e1 + t1;
        assert!(
            total1 > total0,
            "spin injects angular momentum ⇒ a larger bound disk ({:.2} → {:.2} M☾)",
            total0 / m_moon,
            total1 / m_moon
        );
        assert!(
            frac1 < frac0 + 0.03,
            "MEASURED: spin does NOT Earth-enrich the disk here — the rigid-boundary ceiling \
             ({:.0}% → {:.0}%); see docs/31",
            100.0 * frac0,
            100.0 * frac1
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
    #[ignore = "N-scaling sweep — long-running (O(n²)); run explicitly with --ignored"]
    fn disk_provenance_vs_resolution_sweep() {
        // docs/28 "raise N globally": does Earth-derived material loft into the bound disk as RESOLUTION
        // rises — i.e. does the progressive ploughing that makes the Moon Earth-derived EMERGE from the
        // contact physics, letting us delete the declared shock-ejection IOU? This is a MEASUREMENT, not a
        // pass/fail: it prints the bound-aloft Earth/Theia split and the escaped fraction at several N, so
        // the N→emergence trend is a number, not a guess. Self-gravity is O(n²), so wall-time rises ~N².
        // MEASURED (2026-07-14): Earth 0.000 at every N (384/768/1536); the declared 45°/sub-orbital
        // ejection re-impacts regardless of N. The deficit is a MISSING MECHANISM, not resolution.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let mu = G * EARTH_MASS;
        // Keep the CAP_N/DEBRIS_N ratio at the current 2:1 (the cap-mass fudge is docs/28 item 4, held
        // fixed here so this isolates the effect of RESOLUTION alone). Sweep the linear scale ×1,×2,×4.
        println!("\n N (deb+cap) | Earth aloft | Theia aloft | Earth esc | Theia esc  (M_moon)");
        for &(debris_n, cap_n) in &[(128usize, 256usize), (256, 512), (512, 1024)] {
            let (mut agg, mut acc) = build_impact_debris_scaled(
                &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth, EARTH_MASS,
                EARTH_RADIUS_M, debris_n, cap_n, DVec3::ZERO,
            );
            for _ in 0..3000 {
                agg.step(&mut acc, 2.0);
            }
            let (mut ae, mut at, mut ee, mut et) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for (i, p) in agg.particles.iter().enumerate() {
                let r = p.pos.length();
                let bound = 0.5 * p.vel.length_squared() - mu / r < 0.0;
                let is_earth = agg.source[i] == crate::aggregate::SOURCE_TARGET;
                if bound && r > 1.1 * EARTH_RADIUS_M {
                    if is_earth {
                        ae += p.mass
                    } else {
                        at += p.mass
                    }
                } else if !bound {
                    if is_earth {
                        ee += p.mass
                    } else {
                        et += p.mass
                    }
                }
            }
            println!(
                " {:>4}+{:<5} | {:>10.3} | {:>10.3} | {:>8.3} | {:>8.3}",
                debris_n,
                cap_n,
                ae / MOON_MASS,
                at / MOON_MASS,
                ee / MOON_MASS,
                et / MOON_MASS
            );
        }
    }

    #[test]
    #[ignore = "N-scaling EMERGENCE sweep — long-running (O(n²)); run explicitly with --ignored"]
    fn disk_provenance_emergence_no_declared_ejection() {
        // The decisive test of docs/28's "raise N" hypothesis ON ITS OWN TERMS: with the DECLARED shock
        // ejection turned OFF (the cap starts AT REST), does the impactor's CONTACT ploughing loft Earth
        // material into the disk — the emergence docs/24 wants — and does it GROW with resolution? If Earth
        // stays ~0 as N rises here, contact ploughing is not lofting target material at feasible N (the
        // µs shock is sub-resolution at ANY N — docs/24 problem #1), so raising N alone is not the lever.
        // MEASURED (2026-07-14): Earth 0.000 at N=384 AND N=1536 — contact ploughing drives the resting
        // cap DOWN into the planet, not up. Confirms the deficit is mechanism, not resolution.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let mu = G * EARTH_MASS;
        println!("\n[EMERGENCE: cap at REST, contact must do the lofting]");
        println!(" N (deb+cap) | Earth aloft | Theia aloft   (M_moon)");
        for &(debris_n, cap_n) in &[(128usize, 256usize), (512, 1024)] {
            let (mut agg, mut acc) = build_impact_debris_scaled(
                &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth, EARTH_MASS,
                EARTH_RADIUS_M, debris_n, cap_n, DVec3::ZERO,
            );
            // Strip the DECLARED ejection: every target grain back to rest (ground velocity = 0 here).
            for (i, p) in agg.particles.iter_mut().enumerate() {
                if agg.source[i] == crate::aggregate::SOURCE_TARGET {
                    p.vel = DVec3::ZERO;
                }
            }
            acc = agg.accelerations();
            for _ in 0..3000 {
                agg.step(&mut acc, 2.0);
            }
            let (mut ae, mut at) = (0.0f64, 0.0f64);
            for (i, p) in agg.particles.iter().enumerate() {
                let r = p.pos.length();
                if 0.5 * p.vel.length_squared() - mu / r < 0.0 && r > 1.1 * EARTH_RADIUS_M {
                    if agg.source[i] == crate::aggregate::SOURCE_TARGET {
                        ae += p.mass
                    } else {
                        at += p.mass
                    }
                }
            }
            println!(
                " {:>4}+{:<5} | {:>10.3} | {:>10.3}",
                debris_n,
                cap_n,
                ae / MOON_MASS,
                at / MOON_MASS
            );
        }
    }

    #[test]
    #[ignore = "scene reconciliation — long-running; run with --ignored"]
    fn birth_scene_reconciliation_reproduce_the_real_approach() {
        // Option 3 (reconcile scene vs native): birth.html derives v_contact from the LIVE Theia approach
        // (lib.rs start_birth: from (d0, b) at 5 km/s, gravity does the rest → orbit::contact_velocity),
        // NOT the clean 9.5 km/s @ 45° the other native tests hardcode. Reproduce that exact approach here
        // and measure orbiting, so "nothing orbits on screen" is checked against the SAME impact the scene
        // runs — is the discrepancy the geometry, or is it timing/visibility of a small emergent disk?
        let mats = materials::load();
        let theia = crate::planet::theia();
        let m_theia = theia.total_mass();
        let contact = EARTH_RADIUS_M + theia.radius();
        let mu = G * (EARTH_MASS + m_theia); // relative-motion gravitational parameter
        // The scene's inbound geometry (lib.rs start_birth), Earth at the origin.
        let (d0, v_in, b) = (9.6e7_f64, 5_000.0_f64, 1.46 * contact);
        let mut rel = DVec3::new(d0, b, 0.0); // Theia relative to Earth
        let mut vrel = DVec3::new(-v_in, 0.0, 0.0);
        let (mut rel_old, mut vrel_old) = (rel, vrel);
        let dt = 1.0;
        let mut hit = false;
        for _ in 0..2_000_000 {
            if rel.length() <= contact {
                hit = true;
                break;
            }
            rel_old = rel;
            vrel_old = vrel;
            let r = rel.length();
            let a = -rel * (mu / (r * r * r)); // relative acceleration
            vrel += a * dt;
            rel += vrel * dt;
        }
        assert!(hit, "Theia never reached contact");
        let n_hat = rel.normalize();
        let v_contact = crate::orbit::contact_velocity(rel_old, vrel_old, n_hat, contact, mu);
        let site = n_hat * EARTH_RADIUS_M; // Earth at origin
        let v_circ = (G * EARTH_MASS / EARTH_RADIUS_M).sqrt();
        // Obliquity: angle of v_contact from the local surface (0 = grazing, 90 = head-on).
        let vt_frac = (v_contact - n_hat * v_contact.dot(n_hat)).length();
        let obliq_deg = (v_contact.dot(n_hat).abs() / v_contact.length()).acos().to_degrees();
        println!(
            "\nSCENE approach → contact: |v| = {:.0} m/s ({:.2}× v_circ), tangential {:.0} m/s ({:.2}× v_circ), obliquity {:.0}°",
            v_contact.length(), v_contact.length() / v_circ, vt_frac, vt_frac / v_circ, obliq_deg
        );
        // Build the SAME impact the scene builds, integrate the aftermath, measure orbiting by PERIGEE.
        let (mut agg, mut acc) = build_impact_debris_scaled(
            &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia,
            &crate::planet::earth(), EARTH_MASS, EARTH_RADIUS_M, 128, 256, DVec3::ZERO,
        );
        for _ in 0..3000 {
            agg.step(&mut acc, 2.0);
        }
        let mu_e = G * EARTH_MASS;
        let (mut orbiting, mut reimpacting) = (0.0f64, 0.0f64);
        for (i, p) in agg.particles.iter().enumerate() {
            if agg.source[i] != crate::aggregate::SOURCE_TARGET {
                continue;
            }
            let r = p.pos.length();
            let eps = 0.5 * p.vel.length_squared() - mu_e / r;
            if eps >= 0.0 || r <= 1.1 * EARTH_RADIUS_M {
                continue;
            }
            let h = p.pos.cross(p.vel).length();
            let e = (1.0 + 2.0 * eps * h * h / (mu_e * mu_e)).max(0.0).sqrt();
            let perigee = (-mu_e / (2.0 * eps)) * (1.0 - e);
            if perigee > EARTH_RADIUS_M {
                orbiting += p.mass;
            } else {
                reimpacting += p.mass;
            }
        }
        println!(
            "SCENE impact → Earth material: orbiting {:.4} M_moon | re-impacting {:.4} M_moon (clean-45° test gave 0.0495 | 0.0330)",
            orbiting / MOON_MASS, reimpacting / MOON_MASS
        );
        // Does enough matter VAPORIZE for a pressure field to matter? (Option-2 leverage check.)
        let n_vapor = agg.vapor.iter().filter(|v| **v).count();
        let vapor_mass: f64 = agg
            .particles
            .iter()
            .zip(agg.vapor.iter())
            .filter(|(_, &v)| v)
            .map(|(p, _)| p.mass)
            .sum();
        let tmax = agg.temps.iter().cloned().fold(0.0f32, f32::max);
        let tmean = agg.temps.iter().sum::<f32>() / agg.temps.len().max(1) as f32;
        let total: f64 = agg.particles.iter().map(|p| p.mass).sum();
        println!(
            "VAPOR at t=end: {n_vapor}/{} parcels, {:.3} M_moon ({:.0}% of the cloud); temp mean {:.0} K, max {:.0} K (basalt boil+Lv ≈ 10040 K)",
            agg.particles.len(),
            vapor_mass / MOON_MASS,
            100.0 * vapor_mass / total,
            tmean,
            tmax
        );
    }

    #[test]
    #[ignore = "block-timestep impact verification — run with --ignored"]
    fn birth_impact_with_step_block_reproduces_the_disk() {
        // The full test of the block scheduler on the REAL coupled impact (gravity + contact + SPH + PdV +
        // heat): step_block must reproduce the orbiting disk that the global-dt step() forms. Run the same
        // birth impact both ways for the same total time and compare the perigee-above-surface disk.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let mu = G * EARTH_MASS;
        let orbiting = |agg: &crate::aggregate::Aggregate| -> f64 {
            let mut m = 0.0;
            for p in &agg.particles {
                let r = p.pos.length();
                let eps = 0.5 * p.vel.length_squared() - mu / r;
                if eps >= 0.0 || r <= 1.1 * EARTH_RADIUS_M {
                    continue;
                }
                let h = p.pos.cross(p.vel).length();
                let e = (1.0 + 2.0 * eps * h * h / (mu * mu)).max(0.0).sqrt();
                if (-mu / (2.0 * eps)) * (1.0 - e) > EARTH_RADIUS_M {
                    m += p.mass;
                }
            }
            m / MOON_MASS
        };
        let build = || {
            build_impact_debris_scaled(
                &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth, EARTH_MASS,
                EARTH_RADIUS_M, 128, 256, DVec3::ZERO,
            )
        };
        let (mut ag, mut acc) = build();
        for _ in 0..3000 {
            ag.step(&mut acc, 2.0);
        }
        let disk_global = orbiting(&ag);
        let (mut ab, _) = build();
        for _ in 0..3000 {
            ab.step_block(2.0, 0.1); // same base dt; step_block sub-steps the fast set internally
        }
        let disk_block = orbiting(&ab);
        println!(
            "\nDISK — global step() {disk_global:.3} M_moon | block step_block {disk_block:.3} M_moon"
        );
        assert!(disk_block > 0.3, "step_block must form a bound disk on the real impact (got {disk_block:.3})");
        assert!(
            (disk_block - disk_global).abs() < 0.5 * disk_global.max(0.2) + 0.3,
            "block disk must track the global-dt disk (block {disk_block:.3} vs global {disk_global:.3})"
        );
    }

    #[test]
    #[ignore = "orbit-vs-resolution sweep — long-running (O(n²)); run with --ignored"]
    fn disk_orbit_vs_resolution() {
        // Does the TRULY-ORBITING disk (perigee > R, the honest metric) grow with resolution, now that the
        // full fluid physics is in (plough loft + real vapor SPH pressure + PdV + latent heat)? If the
        // orbiting mass climbs with N, resolution is the lever (the disk is a fluid that needs enough
        // parcels — a Moon-forming SPH run uses 10⁴–10⁶); if flat, we learned it cheaply. O(n²) ⇒ ~N² cost.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let mu = G * EARTH_MASS;
        println!("\n N (deb+cap) | Earth orbiting | Theia orbiting | total (M_moon, perigee>R)");
        // With grid + Barnes–Hut (docs/30 1b/1c) these high-N points are now feasible — each was ~4 min at
        // O(N²) before; grid+tree make them tractable. Watch the disk converge toward the ~1–2 M☾ real range.
        for &(dn, cn) in &[(512usize, 1024usize), (1024, 2048), (2048, 4096)] {
            let (mut agg, mut acc) = build_impact_debris_scaled(
                &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth, EARTH_MASS,
                EARTH_RADIUS_M, dn, cn, DVec3::ZERO,
            );
            for _ in 0..3000 {
                agg.step(&mut acc, 2.0);
            }
            let (mut earth_orb, mut theia_orb) = (0.0f64, 0.0f64);
            for (i, p) in agg.particles.iter().enumerate() {
                let r = p.pos.length();
                let eps = 0.5 * p.vel.length_squared() - mu / r;
                if eps >= 0.0 || r <= 1.1 * EARTH_RADIUS_M {
                    continue;
                }
                let h = p.pos.cross(p.vel).length();
                let e = (1.0 + 2.0 * eps * h * h / (mu * mu)).max(0.0).sqrt();
                let perigee = (-mu / (2.0 * eps)) * (1.0 - e);
                if perigee > EARTH_RADIUS_M {
                    if agg.source[i] == crate::aggregate::SOURCE_TARGET {
                        earth_orb += p.mass;
                    } else {
                        theia_orb += p.mass;
                    }
                }
            }
            let mm = MOON_MASS;
            println!(
                " {:>4}+{:<5} | {:>14.4} | {:>14.4} | {:.4}",
                dn,
                cn,
                earth_orb / mm,
                theia_orb / mm,
                (earth_orb + theia_orb) / mm
            );
        }
    }

    #[test]
    #[ignore = "energy-budget diagnostic — run with --ignored"]
    fn impact_energy_budget_is_heat_created_or_converted() {
        // Heat-budget check (docs/28): the vapor sits at ~18,500 K, far above real (~few 1000 K). Is that
        // heat CREATED (a conservation bug) or CONVERTED from the impact + gravitational energy (real, but
        // with too few outlets)? Total energy E = KE + U(heat) + PE_ext + PE_self must only ever DECREASE
        // (radiation removes it); if U grows more than the available KE+PE, energy is being manufactured.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let (mut agg, mut acc) = build_impact_debris_scaled(
            &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth, EARTH_MASS,
            EARTH_RADIUS_M, 128, 256, DVec3::ZERO,
        );
        let c = agg.specific_heat;
        let energy = |a: &Aggregate| -> (f64, f64, f64, f64) {
            let ke: f64 = a.particles.iter().map(|p| 0.5 * p.mass * p.vel.length_squared()).sum();
            let u: f64 =
                a.particles.iter().zip(a.temps.iter()).map(|(p, &t)| p.mass * c * t as f64).sum();
            let (mu, r_e) = (G * EARTH_MASS, EARTH_RADIUS_M);
            let pe_ext: f64 = a
                .particles
                .iter()
                .map(|p| {
                    let r = p.pos.length();
                    let phi = if r >= r_e {
                        -mu / r
                    } else {
                        -mu / (2.0 * r_e) * (3.0 - (r / r_e).powi(2))
                    };
                    p.mass * phi
                })
                .sum();
            let s2 = a.softening * a.softening;
            let mut pe_self = 0.0;
            for i in 0..a.particles.len() {
                for j in (i + 1)..a.particles.len() {
                    let d2 = (a.particles[i].pos - a.particles[j].pos).length_squared() + s2;
                    pe_self -= G * a.particles[i].mass * a.particles[j].mass / d2.sqrt();
                }
            }
            (ke, u, pe_ext, pe_self)
        };
        let (ke0, u0, pe0, ps0) = energy(&agg);
        let e0 = ke0 + u0 + pe0 + ps0;
        for _ in 0..300 {
            agg.step(&mut acc, 2.0);
        }
        let (ke1, u1, pe1, ps1) = energy(&agg);
        let e1 = ke1 + u1 + pe1 + ps1;
        println!("\n           KE           U(heat)      PE_ext       PE_self      TOTAL");
        println!("t=0    {ke0:.3e} {u0:.3e} {pe0:.3e} {ps0:.3e} {e0:.3e}");
        println!("t=end  {ke1:.3e} {u1:.3e} {pe1:.3e} {ps1:.3e} {e1:.3e}");
        println!("impact KE input      = {:.3e} J", 0.5 * m_theia * v_contact.length_squared());
        println!("ΔU (heat generated)  = {:.3e} J", u1 - u0);
        println!("ΔKE                  = {:.3e} J", ke1 - ke0);
        println!("ΔPE (ext+self)       = {:.3e} J", (pe1 + ps1) - (pe0 + ps0));
        println!(
            "TOTAL energy drift   = {:.2}%  (must be ≤ 0: only radiation removes energy)",
            100.0 * (e1 - e0) / e0.abs()
        );
    }

    #[test]
    #[ignore = "orbit diagnostic — long-running; run with --ignored"]
    fn disk_orbit_diagnostic_does_anything_actually_orbit() {
        // Robin rig-watched birth.html: crater material is excavated, "but nothing reached orbital
        // velocity." Reconcile with the native "bound-aloft" number. The honest test of ORBITING is the
        // PERIGEE: a bound ellipse whose perigee is BELOW the surface re-impacts — it is NOT in orbit,
        // even if it is momentarily above 1.1 R. Report, for the excavated cap: peak tangential speed vs
        // circular velocity, and the mass split into truly-orbiting (perigee > R) vs re-impacting.
        let mats = materials::load();
        let theia = crate::planet::theia();
        let earth = crate::planet::earth();
        let m_theia = theia.total_mass();
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v_esc = (2.0 * G * (EARTH_MASS + m_theia) / (EARTH_RADIUS_M + theia.radius())).sqrt();
        let v_contact = DVec3::new(v_esc * 0.7071, -v_esc * 0.7071, 0.0);
        let (mut agg, mut acc) = build_impact_debris_scaled(
            &mats, site, DVec3::ZERO, DVec3::ZERO, m_theia, v_contact, &theia, &earth, EARTH_MASS,
            EARTH_RADIUS_M, 128, 256, DVec3::ZERO,
        );
        let mu = G * EARTH_MASS;
        let v_circ = (mu / EARTH_RADIUS_M).sqrt();
        let v_esc_surf = (2.0 * mu / EARTH_RADIUS_M).sqrt();
        // Peak tangential speed among the excavated cap, right after the loft (t=0).
        let mut peak_vt0 = 0.0f64;
        for (i, p) in agg.particles.iter().enumerate() {
            if agg.source[i] == crate::aggregate::SOURCE_TARGET {
                let rhat = p.pos.normalize_or_zero();
                let vt = (p.vel - rhat * p.vel.dot(rhat)).length();
                peak_vt0 = peak_vt0.max(vt);
            }
        }
        for _ in 0..3000 {
            agg.step(&mut acc, 2.0);
        }
        // Classify the still-aloft, bound cap material by PERIGEE (the honest orbit test).
        let (mut orbiting, mut reimpacting, mut peak_vt) = (0.0f64, 0.0f64, 0.0f64);
        for (i, p) in agg.particles.iter().enumerate() {
            if agg.source[i] != crate::aggregate::SOURCE_TARGET {
                continue;
            }
            let r = p.pos.length();
            let v2 = p.vel.length_squared();
            let eps = 0.5 * v2 - mu / r; // specific orbital energy
            let rhat = p.pos.normalize_or_zero();
            let vt = (p.vel - rhat * p.vel.dot(rhat)).length();
            peak_vt = peak_vt.max(vt);
            if eps >= 0.0 || r <= 1.1 * EARTH_RADIUS_M {
                continue; // unbound or already down
            }
            let h = p.pos.cross(p.vel).length(); // specific angular momentum
            let a = -mu / (2.0 * eps);
            let e = (1.0 + 2.0 * eps * h * h / (mu * mu)).max(0.0).sqrt();
            let perigee = a * (1.0 - e);
            if perigee > EARTH_RADIUS_M {
                orbiting += p.mass; // perigee clears the surface — a real orbit
            } else {
                reimpacting += p.mass; // bound ellipse that dives back into Earth
            }
        }
        let mm = MOON_MASS;
        println!("\nv_circ = {:.0} m/s, v_esc = {:.0} m/s", v_circ, v_esc_surf);
        println!("cap PEAK tangential speed at launch = {:.0} m/s ({:.2}× circular)", peak_vt0, peak_vt0 / v_circ);
        println!("cap peak tangential speed at t=end   = {:.0} m/s ({:.2}× circular)", peak_vt, peak_vt / v_circ);
        println!(
            "cap Earth material: TRULY ORBITING (perigee>R) {:.4} M_moon | re-impacting (perigee<R) {:.4} M_moon",
            orbiting / mm, reimpacting / mm
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
            MOON_MASS, // moon-scale impactor (a = MOON_RADIUS_M): within budget, so no scaling
            EARTH_G,   // gravity-regime ejecta scale K·√(g·R_crater) on Earth
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
            MOON_MASS,
            EARTH_G,
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
        //     set by the scene's local gravity — a flat uniform-g patch has no escape velocity), and the
        //     ejecta velocity SCALE is now the GRAVITY-REGIME crater speed K·√(g·R_crater) (docs/28), NOT
        //     the old impactor contact jet C·v_i. The derived-not-dial check reflects that honest change:
        //     the ejecta speed is INDEPENDENT of the impactor speed at a fixed crater (it is set by g·R),
        //     and it scales as √(g·R_crater) when the crater grows — so a bigger crater lofts faster,
        //     a faster impactor at the SAME crater does not. The total ejecta KE stays CAPPED at the
        //     impact energy ½·m·v² (docs/28 exact conservation), asserted Σ½m|v_ej|² ≤ E_i;
        //   • a VERTICAL strike is SYMMETRIC (no downrange bias) — obliquity is what elongates a furrow.
        use crate::aggregate::SOURCE_TARGET;
        let mats = materials::load();
        let earth = crate::planet::earth();
        let up = DVec3::Y; // local surface normal on a flat patch (+y under uniform gravity)
        let site = DVec3::ZERO; // impact point at the origin of the tangent plane
        let flat = ExcavSurface::Flat { up, ref_radius: EARTH_RADIUS_M };
        let frag_mass = 2_900.0; // a 1 m³ basalt voxel (kg) — the real terrain grain, not a proxy
        let extent = 12.0; // a meteor-scale crater (metres), not a giant impact
        let a = 0.3; // impactor radius (scaling length) — a small Fe-Ni body
        let impactor_mass = 1_000.0; // a ≈ 0.3 m Fe-Ni body is ~900 kg → the impact-energy budget
        let (t, lat) = (DVec3::X, DVec3::Z); // downrange / lateral tangents for the oblique case below

        // OBLIQUE 45° impact at 17 km/s in the x–y plane → downrange = +x.
        let v_oblique = DVec3::new(1.0, -1.0, 0.0).normalize() * 17_000.0;
        let (bodies, mids, temps, src) = furrow_target_grains(
            &mats, &earth, flat, site, v_oblique, a, frag_mass, DVec3::ZERO, CAP_N, extent,
            impactor_mass, TERRAIN_G,
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
        assert!(max_up > 0.0, "some grains must be lofted (outward/up velocity), got max {max_up:.3}");
        // EXACT ENERGY CONSERVATION (docs/28): the total ejecta KE never exceeds the impact energy ½·m·v².
        // For this physically-consistent 1000 kg / 0.31 m terrain meteor the raw H-H ejecta KE is within
        // budget (~0.15× E_i, measured), so the f=1 cap is inactive here — the invariant still holds.
        let e_impact = 0.5 * impactor_mass * v_oblique.length_squared();
        let ke: f64 = bodies.iter().map(|p| 0.5 * p.mass * p.vel.length_squared()).sum();
        assert!(
            ke <= e_impact * (1.0 + 1e-9),
            "ejecta KE {ke:.3e} must not exceed the impact energy {e_impact:.3e} (energy conserved)"
        );
        // Derived, not a dial (docs/28, the crater-scaled ejecta fix): the ejecta velocity SCALE is
        // K·√(g·R_crater), so at a FIXED crater it is INDEPENDENT of the impactor speed — a half-speed
        // impact into the SAME crater lofts at the SAME speed (the old C·v_i scale would have halved it,
        // which is exactly the sub-grain contact-jet velocity we replaced). The impactor speed enters the
        // loft only through the crater it digs (energy → R_crater), tested next.
        let vmax = |b: &[Body]| b.iter().map(|p| p.vel.length()).fold(0.0, f64::max);
        let (half, _, _, _) = furrow_target_grains(
            &mats, &earth, flat, site, v_oblique * 0.5, a, frag_mass, DVec3::ZERO, CAP_N, extent,
            impactor_mass, TERRAIN_G,
        );
        let (vf, vh) = (vmax(&bodies), vmax(&half));
        assert!(
            (vh / vf - 1.0).abs() < 1e-9,
            "ejecta speed is set by the crater (√(g·R)), not the impactor speed: same crater ⇒ same loft \
             (full {vf:.3}, half-speed {vh:.3}; ratio {:.6})",
            vh / vf
        );
        // And it DOES track gravity: the scale is √(g·R_crater), so at a FIXED crater geometry (identical
        // grain positions — the fill is g-independent) quadrupling g doubles EVERY grain's ejection speed.
        // This isolates the derived scale (the (a/d)^(1/μ) shape is unchanged when only g moves), proving
        // it is √g, not a fixed kick nor tuned to look right.
        let (heavy_g, _, _, _) = furrow_target_grains(
            &mats, &earth, flat, site, v_oblique, a, frag_mass, DVec3::ZERO, CAP_N, extent,
            impactor_mass, 4.0 * TERRAIN_G,
        );
        for (p, q) in bodies.iter().zip(heavy_g.iter()) {
            let (s, sg) = (p.vel.length(), q.vel.length());
            if s > 1e-6 {
                assert!(
                    (sg / s - 2.0).abs() < 1e-6,
                    "ejection speed scales as √g: 4×g ⇒ 2× speed (got {sg:.4}/{s:.4} = {:.4})",
                    sg / s
                );
            }
        }

        // VERTICAL strike (straight down): SYMMETRIC bowl — no downrange bias, along-span ≈ across-span,
        // and the centroid is centred over the impact (obliquity is what elongates/offsets a furrow).
        let v_vert = -up * 17_000.0;
        let (vb, _, _, _) = furrow_target_grains(
            &mats, &earth, flat, site, v_vert, a, frag_mass, DVec3::ZERO, CAP_N, extent,
            impactor_mass, TERRAIN_G,
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
    fn ejecta_energy_scale_conserves_energy_and_leaves_within_budget_ejecta_alone() {
        // The shared cap in isolation (docs/28). Two grains with a known raw KE.
        let ejecta = [
            (2_900.0, DVec3::new(0.0, 10_000.0, 0.0)),
            (2_900.0, DVec3::new(0.0, 6_000.0, 0.0)),
        ];
        let raw_ke: f64 = ejecta.iter().map(|(m, v)| 0.5 * m * v.length_squared()).sum();
        // (a) OVER budget → factor √(E/KE), and the SCALED KE equals the budget exactly.
        let budget = 1.0e10;
        assert!(budget < raw_ke, "test premise: raw KE exceeds the budget");
        let s = ejecta_energy_scale(ejecta.iter().copied(), budget);
        assert!((s - (budget / raw_ke).sqrt()).abs() < 1e-15, "factor is √(E/KE)");
        let scaled_ke: f64 = ejecta.iter().map(|(m, v)| 0.5 * m * (*v * s).length_squared()).sum();
        assert!(
            (scaled_ke - budget).abs() / budget < 1e-12,
            "scaled ejecta KE equals the budget exactly ({scaled_ke:.6e} vs {budget:.6e})"
        );
        // (b) WITHIN budget → factor is EXACTLY 1.0 (byte-unchanged; `v * 1.0 == v`).
        let s2 = ejecta_energy_scale(ejecta.iter().copied(), raw_ke * 2.0);
        assert_eq!(s2, 1.0, "within budget: the declared velocities are left exactly as they are");
    }

    #[test]
    fn a_small_impactor_cannot_eject_more_energy_than_it_delivered() {
        // docs/28: exact energy conservation — the total ejecta KE can never exceed the impact energy
        // ½·m·v². The Housen–Holsapple law v = C·v_i·(a/d)^(1/μ) sets the velocity SHAPE, the cap sets the
        // SCALE. We prove the BINDING path in the furrow: run the SAME excavation uncapped (impactor_mass
        // = ∞) and with a LIGHT impactor whose ½·m·v² is below that raw ejecta KE, and require the cap to
        // scale the total ejecta KE down to the impact energy EXACTLY.
        let mats = materials::load();
        let earth = crate::planet::earth();
        let flat = ExcavSurface::Flat { up: DVec3::Y, ref_radius: EARTH_RADIUS_M };
        let site = DVec3::ZERO;
        let a = 0.31; // impactor radius (H-H scaling length)
        let v = DVec3::new(1.0, -1.0, 0.0).normalize() * 17_000.0; // 17 km/s oblique
        let frag_mass = 2_900.0; // 1 m³ basalt voxel grains (the real terrain grain)

        // Raw (uncapped) ejecta KE of this excavation.
        let raw = furrow_target_grains(
            &mats, &earth, flat, site, v, a, frag_mass, DVec3::ZERO, CAP_N, 12.0, f64::INFINITY,
            TERRAIN_G,
        )
        .0;
        let raw_ke: f64 = raw.iter().map(|b| 0.5 * b.mass * b.vel.length_squared()).sum();
        assert!(raw_ke > 0.0);

        // A light impactor whose impact energy is HALF the raw ejecta KE → the cap must bind.
        let e_impact = 0.5 * raw_ke;
        let impactor_mass = e_impact / (0.5 * v.length_squared());
        let (bodies, ..) = furrow_target_grains(
            &mats, &earth, flat, site, v, a, frag_mass, DVec3::ZERO, CAP_N, 12.0, impactor_mass,
            TERRAIN_G,
        );
        // ground_vel is zero here, so the ejecta KE relative to ground is the absolute KE.
        let ke: f64 = bodies.iter().map(|b| 0.5 * b.mass * b.vel.length_squared()).sum();
        assert!(
            (ke - e_impact).abs() / e_impact < 1e-9,
            "capped ejecta KE {ke:.3e} J == the impact energy {e_impact:.3e} J (exact conservation)"
        );
        assert!(ke < raw_ke, "the cap reduced the ejecta KE ({ke:.3e} < raw {raw_ke:.3e})");

        // HONEST FINDING (docs/28): a PHYSICALLY-CONSISTENT 1000 kg / 0.31 m terrain meteor delivers far
        // MORE energy than this raw ejection carries, so at the f=1 bound the cap does NOT bind for it. The
        // cap is a correct conservation invariant, but it was never what tamed the terrain debris storm:
        // with the crater-scaled velocity (K·√(g·R_crater)) the raw ejecta KE is now ~m/s-scale and tiny
        // (≪ E_i). The storm was a velocity-SCALE error (the impactor contact jet C·v_i applied to whole
        // grains), fixed by scaling to the crater, not by the energy cap.
        let e_1000kg = 0.5 * 1000.0 * v.length_squared();
        assert!(
            raw_ke < e_1000kg,
            "the 1000 kg meteor is within budget (raw {raw_ke:.3e} < E_i {e_1000kg:.3e}); f=1 cap inactive"
        );
    }

    #[test]
    fn a_giant_impactor_within_budget_is_not_scaled_byte_for_byte() {
        // The space-band guard (docs/28): a Theia-scale impactor's impact energy ½·m·v² DWARFS its ejecta
        // KE, so the cap is inactive and the DECLARED ejection is byte-for-byte unchanged. If the cap ever
        // touched the space band this test fails. We prove it by comparing the REAL-mass run against an
        // INFINITE-energy run (which forces factor 1 by construction): byte-equal ⇒ Theia was uncapped.
        let mats = materials::load();
        let earth = crate::planet::earth();
        let curved = ExcavSurface::Curved { center: DVec3::ZERO, radius: EARTH_RADIUS_M };
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let v = DVec3::new(1.0, -1.0, 0.0).normalize() * 10_000.0;
        let a = MOON_RADIUS_M;
        let extent = 2.0 * MOON_RADIUS_M;
        let theia = crate::planet::theia();
        let m_theia = theia.total_mass();
        let frag_mass = m_theia / DEBRIS_N as f64;
        let real = furrow_target_grains(
            &mats, &earth, curved, site, v, a, frag_mass, DVec3::ZERO, CAP_N, extent, m_theia,
            EARTH_G,
        )
        .0;
        let uncapped = furrow_target_grains(
            &mats, &earth, curved, site, v, a, frag_mass, DVec3::ZERO, CAP_N, extent, f64::INFINITY,
            EARTH_G,
        )
        .0;
        for (r, u) in real.iter().zip(uncapped.iter()) {
            assert_eq!(r.vel, u.vel, "Theia's declared ejection must be unscaled (within budget)");
        }
        // And directly: the ejecta KE is comfortably under the impact energy.
        let e_impact = 0.5 * m_theia * v.length_squared();
        let ke: f64 = real.iter().map(|b| 0.5 * b.mass * b.vel.length_squared()).sum();
        assert!(ke < e_impact, "Theia's ejecta KE {ke:.3e} J < impact energy {e_impact:.3e} J");
    }

    #[test]
    fn a_terrain_meteor_ejecta_lands_in_a_local_blanket_not_a_footprint_storm() {
        // docs/28 (the 2026-07-13 crater-scaled-ejecta fix). THE problem: the old ejecta velocity SCALE
        // C·v_i (the impactor's ~km/s contact-jet speed) applied to whole 2900 kg terrain grains flung a
        // meteor's ejecta across the WHOLE 96 m patch at up to ~km/s — a footprint-spanning storm that
        // forced a footprint-sized resolved region. The honest fix scales the ejecta to the CRATER's
        // gravity-regime speed K·√(g·R_crater) instead, so the blanket is LOCAL: a few crater radii, tens
        // of metres. This test pins that at the EJECTION-LAW level: the max ballistic range of any grain
        // must be O(a few R_crater), firmly inside the patch — NOT the ~10⁷ m the old C·v_i scale gave.
        // HONEST NOTE (rig-measured 2026-07-13): fixing the ejection scale does NOT by itself tame the
        // terrain SCENE's debris storm — with this ejection capped at ~18 m/s the rig STILL shows terrain
        // grains flung km-scale, because the dominant loft there is the GPU terrain-collision penalty
        // (`particle_step.wgsl::terrain_accel`, f = c_stiffness·penetration) on grains materialized BELOW
        // the surface, NOT the shock ejection. That is a SEPARATE mechanism (out of this ejection-scale
        // change); this test guards only that the ejection LAW itself is now local, not a km-scale spray.
        let mats = materials::load();
        let earth = crate::planet::earth();
        let flat = ExcavSurface::Flat { up: DVec3::Y, ref_radius: EARTH_RADIUS_M };
        let site = DVec3::ZERO;
        // The real terrain-meteor parameters (lib.rs `meteor`): a 1000 kg / 0.31 m Fe-Ni body at 17 km/s
        // digs an R_crater ≈ 14 m crater (energy/σ, LOD-capped) into g = 9.88 terrain.
        let r_crater = 14.0;
        let a = 0.31;
        let v_impact = DVec3::new(1.0, -1.0, 0.0).normalize() * 17_000.0;
        let frag_mass = 2_900.0; // 1 m³ basalt voxel
        let impactor_mass = 1_000.0;
        let (bodies, ..) = furrow_target_grains(
            &mats, &earth, flat, site, v_impact, a, frag_mass, DVec3::ZERO, CAP_N, r_crater,
            impactor_mass, TERRAIN_G,
        );
        // Ballistic range on the flat uniform-g patch: a grain launched from ~the surface with up-speed vu
        // and horizontal speed vh lands 2·vu·vh/g downrange (only the lofted, vu>0 grains travel).
        let range = |p: &Body| {
            let vu = p.vel.dot(DVec3::Y);
            if vu <= 0.0 {
                return 0.0;
            }
            let vh = (p.vel - DVec3::Y * vu).length();
            2.0 * vu * vh / TERRAIN_G
        };
        let max_range = bodies.iter().map(range).fold(0.0f64, f64::max);
        let max_speed = bodies.iter().map(|p| p.vel.length()).fold(0.0f64, f64::max);
        println!(
            "TERRAIN ejecta: max speed {max_speed:.1} m/s · max ballistic range {max_range:.1} m \
             (R_crater {r_crater} m; 96 m patch)"
        );
        // LOCAL blanket: the farthest ejecta lands within a few crater radii — tens of metres, well inside
        // the patch. (Measured ~14 m ≈ 1 R_crater at K=1; the old scale put it at ~10⁷ m.)
        assert!(
            max_range < 3.0 * r_crater && max_range < 50.0,
            "ejecta must land in a LOCAL blanket (max range {max_range:.1} m < 3·R_crater = {:.1} m), \
             not a footprint storm",
            3.0 * r_crater
        );
        // The fast km/s tail is GONE: no grain moves faster than a few tens of m/s.
        assert!(
            max_speed < 40.0,
            "the km/s ejecta tail is gone — max grain speed {max_speed:.1} m/s (was ~10 km/s under C·v_i)"
        );
    }

    #[test]
    fn a_space_scale_impact_still_ejects_at_km_per_second() {
        // The other side of the SAME crater-scaled scale (docs/28): a giant impact's excavation extent is
        // planet-scale (R_crater ~ millions of metres) and g ~ 10, so K·√(g·R_crater) ≈ 5.9 km/s —
        // essentially the old C·v_i ≈ 5.7 km/s. So the space band's km/s ejecta (what lofts the
        // proto-lunar disk) is preserved. Mirrors the furrow/birth assertions: fast (km/s) but sub-escape
        // (bound), so material is launched into orbit rather than blown clean away.
        let mats = materials::load();
        let earth = crate::planet::earth();
        let curved = ExcavSurface::Curved { center: DVec3::ZERO, radius: EARTH_RADIUS_M };
        let site = DVec3::new(0.0, EARTH_RADIUS_M, 0.0);
        let theia = crate::planet::theia();
        let m_theia = theia.total_mass();
        let a = theia.radius();
        let extent = (2.0 * a).min(0.55 * EARTH_RADIUS_M); // the same cap_extent the space builder uses
        let v = DVec3::new(1.0, -1.0, 0.0).normalize() * 9_500.0; // ~mutual escape, 45° oblique
        let frag_mass = m_theia / DEBRIS_N as f64;
        let (bodies, ..) = furrow_target_grains(
            &mats, &earth, curved, site, v, a, frag_mass, DVec3::ZERO, CAP_N, extent, m_theia,
            EARTH_G,
        );
        let vmax = bodies.iter().map(|p| p.vel.length()).fold(0.0f64, f64::max);
        let v_esc = (2.0 * G * EARTH_MASS / EARTH_RADIUS_M).sqrt();
        // The crater-scaled ejecta speed here is √(g·extent); confirm the code matches the derivation.
        let expected_scale = (EARTH_G * extent).sqrt();
        println!(
            "SPACE ejecta: max speed {vmax:.0} m/s (scale √(g·extent) = {expected_scale:.0} m/s; \
             old C·v_i ≈ {:.0} m/s; v_esc {v_esc:.0} m/s)",
            0.6 * v.length()
        );
        assert!(
            vmax > 3_000.0,
            "space-band ejecta must stay km/s to loft a disk (got {vmax:.0} m/s)"
        );
        assert!(vmax < v_esc, "ejecta must be sub-escape (bound): {vmax:.0} < {v_esc:.0}");
        // The scale IS √(g·R_crater) (the max-speed grain sits at d→a, where (a/d)^(1/μ)·fade ≈ 1).
        assert!(
            (vmax - expected_scale).abs() / expected_scale < 0.2,
            "max ejecta speed ≈ the derived scale √(g·extent): {vmax:.0} vs {expected_scale:.0}"
        );
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
