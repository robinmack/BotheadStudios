//! The accretion / growth operator (docs/33 stage 4c.3).
//!
//! A giant-impact disk of equal-mass SPH particles is effectively COLLISIONLESS at low resolution and has
//! no fusion law — particle masses never grow — so a round Moon can never emerge from it (diagnosis, JOURNAL
//! 2026-07-17). This module adds the growth law: detect gravitationally-**bound clumps** in the disk by
//! friends-of-friends (the same union-find `lib.rs::disk_stats_json` uses to count moonlets), and PROMOTE
//! each clump that is genuinely self-bound AND sits outside the central remnant's Roche limit into ONE body.
//!
//! **Honesty gates (both required to accrete):**
//!   1. **Self-bound** — the clump's internal kinetic energy plus its own gravitational binding energy is
//!      negative (`Σ½mᵢ|vᵢ−v_com|² + PE_self < 0`). A spatially-close but hot/unbound group is NOT a body.
//!   2. **Outside Roche** — the clump's COM is beyond the fluid Roche limit `d = 2.44·R·(ρ_planet/ρ_clump)^⅓`
//!      of the remnant (same law as `tides::secular_step`). A clump inside Roche should tidally SHRED, not
//!      accrete, so it is left as particles for the sim to disrupt.
//!
//! **Conservation.** Promotion to a point body at the clump COM conserves **mass, linear momentum, the
//! centre of mass, and total angular momentum EXACTLY** (the body carries `Σm`, `Σmv/Σm`, `Σmx/Σm`, and
//! `Σmᵢ(rᵢ−com)×(vᵢ−v_com)`). The clump's internal random kinetic energy is absorbed as heat — physical for
//! inelastic accretion — and is carried on the body as `thermal_j` rather than discarded.
//!
//! Angular momentum is a FIELD (`Body::ang_mom`), not a remark. Until 2026-07-23 this doc claimed the spin
//! was "folded into the body … recoverable from the members", but `Body` had nowhere to put it and the
//! members are precisely what a de-resolution pass deletes — so any caller that actually consumed
//! `Accreted::consumed` would have destroyed the spin of every clump it promoted, and a mass/momentum check
//! would still have passed. A rotating disk is the case where that term dominates. The rule this encodes:
//! **a demotion may only drop a quantity it can name and hand back** (docs/44 §7 — promotion and demotion
//! must not inject energy; the inverse must be able to restore what the forward pass absorbed).

use crate::neighbors::NeighborGrid;
use glam::DVec3;

const FOUR_THIRDS_PI: f64 = 4.0 / 3.0 * std::f64::consts::PI;
/// Barnes-Hut opening angle for the clump binding energy — the same 0.5 the gravity path uses, so one
/// accuracy/speed trade-off governs both. theta -> 0 recovers the exact all-pairs sum (pinned in `bhtree`).
const BH_THETA: f64 = 0.5;

/// A candidate accreted body found in the particle field.
#[derive(Clone, Debug)]
pub struct Clump {
    pub members: Vec<usize>,
    pub mass: f64,
    pub com_pos: DVec3,
    pub com_vel: DVec3,
    pub rho: f64,          // volume-summed density: mass / Σ(mᵢ/ρᵢ)
    pub radius: f64,       // sphere of that density and mass: (3·mass / 4πρ)^⅓
    pub internal_ke: f64,  // Σ ½ mᵢ |vᵢ − v_com|²  (random motion; absorbed as heat on merge)
    pub self_pe: f64,      // −Σ_{i<j} G mᵢ mⱼ / |rᵢⱼ|  (softened) — the clump's own binding energy
    pub ang_mom: DVec3,    // Σ mᵢ (rᵢ−com) × (vᵢ−v_com) — the clump's SPIN about its own COM
    pub thermal_ke: f64,   // internal_ke MINUS the coherent-rotation share ½ω·L — the part that becomes heat
    pub bound: bool,       // internal_ke + self_pe < 0
    pub outside_roche: bool,
}

impl Clump {
    /// Does this clump accrete into one body? Bound, outside Roche, and more than one member.
    pub fn accretes(&self) -> bool {
        self.members.len() >= 2 && self.bound && self.outside_roche
    }
}

/// A body promoted from an accreted clump — mass at the clump COM with the clump's bulk velocity.
///
/// `ang_mom` and `thermal_j` are what make the promotion REVERSIBLE: re-particalizing this body must be
/// able to hand back the spin and the heat it arrived with. Without them a rotating clump would come back
/// as a still one, and the merge's inelastic heat would have vanished — both invisible to a mass/momentum
/// check, which is why they are fields and not a comment (docs/44 §7: demotion must not inject energy).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: DVec3,
    pub vel: DVec3,
    pub mass: f64,
    pub rho: f64,
    pub radius: f64,
    /// Σ mᵢ (rᵢ−com) × (vᵢ−v_com) — the clump's spin, in the frame of the body itself.
    pub ang_mom: DVec3,
    /// The internal random KE an inelastic merge thermalises (J). Carried, not discarded.
    pub thermal_j: f64,
}

impl Body {
    /// Uniform-sphere moment of inertia about the body's own centre — isotropic, so `E_rot = |L|²/2I`
    /// holds for rotation about ANY axis (no principal-axis assumption needed).
    fn inertia(&self) -> f64 {
        0.4 * self.mass * self.radius * self.radius
    }

    /// Does this straggler STRIKE the body and stay? Robin's rule: a meteor that impacts the moon becomes
    /// part of the moon unless it reaches escape velocity on the rebound. Contact is `r <= radius`; staying
    /// is `|v_rel| < sqrt(2GM/r)` — the body's OWN escape speed at the contact point. There is no dial
    /// here: both terms are the body's real mass and radius, and a fast enough impactor is correctly
    /// refused and stays a particle.
    pub fn absorbs(&self, g: f64, pos: DVec3, vel: DVec3) -> bool {
        let r = (pos - self.pos).length();
        if r > self.radius {
            return false;
        }
        let v_esc = (2.0 * g * self.mass / r.max(self.radius * 1.0e-6)).sqrt();
        (vel - self.vel).length() < v_esc
    }

    /// Absorb a straggler, conserving mass, linear momentum, the centre of mass and TOTAL angular momentum
    /// exactly, and keeping the energy budget closed: the relative kinetic energy the inelastic merge
    /// destroys becomes heat, except the share the merged body's new spin carries away.
    pub fn absorb(&mut self, pos: DVec3, vel: DVec3, mass: f64, rho: f64) {
        let m_new = self.mass + mass;
        let x_new = (self.pos * self.mass + pos * mass) / m_new;
        let v_new = (self.vel * self.mass + vel * mass) / m_new;
        // Angular momentum about the NEW centre of mass: the spin already held, plus the orbital moment
        // each piece had about that new centre. This is what makes an off-centre strike spin the body up.
        let l_new = self.ang_mom
            + self.mass * (self.pos - x_new).cross(self.vel - v_new)
            + mass * (pos - x_new).cross(vel - v_new);
        // Internal energy in the COM frame BEFORE the merge...
        let i_old = self.inertia();
        let e_rot_old = if i_old > 0.0 { self.ang_mom.length_squared() / (2.0 * i_old) } else { 0.0 };
        let ke_internal = self.thermal_j
            + e_rot_old
            + 0.5 * self.mass * (self.vel - v_new).length_squared()
            + 0.5 * mass * (vel - v_new).length_squared();
        // ...and after: volume-additive density (as `find_clumps` computes it), radius from mass at that
        // density, and whatever the new spin does not carry is heat.
        let vol = self.mass / self.rho.max(1.0e-9) + mass / rho.max(1.0e-9);
        let rho_new = if vol > 0.0 { m_new / vol } else { self.rho };
        let radius_new = (m_new / (FOUR_THIRDS_PI * rho_new)).cbrt();
        let i_new = 0.4 * m_new * radius_new * radius_new;
        let e_rot_new = if i_new > 0.0 { l_new.length_squared() / (2.0 * i_new) } else { 0.0 };
        self.pos = x_new;
        self.vel = v_new;
        self.mass = m_new;
        self.rho = rho_new;
        self.radius = radius_new;
        self.ang_mom = l_new;
        self.thermal_j = (ke_internal - e_rot_new).max(0.0);
    }
}

/// Result of one accretion pass: the promoted bodies and the indices they consumed.
#[derive(Clone, Debug, Default)]
pub struct Accreted {
    pub bodies: Vec<Body>,
    pub consumed: Vec<usize>, // particle indices absorbed into a promoted body (sorted)
}

/// Friends-of-friends clumps of a particle field, each classified for boundedness and the Roche gate.
///
/// `linking_length` is the FoF link distance (typically a few × the interparticle spacing — particles within
/// it are in the same clump). `g`/`softening` match the sim's gravity so the binding energy is consistent.
/// The central remnant `(central_pos, central_mass, central_radius)` sets the Roche limit.
#[allow(clippy::too_many_arguments)]
pub fn find_clumps(
    pos: &[DVec3],
    vel: &[DVec3],
    mass: &[f64],
    rho: &[f64],
    linking_length: f64,
    g: f64,
    softening: f64,
    central_pos: DVec3,
    central_mass: f64,
    central_radius: f64,
) -> Vec<Clump> {
    let n = pos.len();
    assert!(vel.len() == n && mass.len() == n && rho.len() == n, "accretion: ragged particle arrays");

    // --- friends-of-friends: union particles within the linking length (union-find, path-compressed) ---
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        // path compression
        let mut c = i;
        while p[c] != r {
            let nx = p[c];
            p[c] = r;
            c = nx;
        }
        r
    }
    let ll2 = linking_length * linking_length;
    let grid = NeighborGrid::build(pos, linking_length);
    grid.for_each_pair(pos, |i, j| {
        if (pos[i] - pos[j]).length_squared() <= ll2 {
            let (a, b) = (find(&mut parent, i), find(&mut parent, j));
            if a != b {
                parent[a] = b;
            }
        }
    });

    // --- gather members per root ---
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }

    // --- classify each clump ---
    let mut clumps = Vec::with_capacity(groups.len());
    for members in groups.into_values() {
        let m: f64 = members.iter().map(|&i| mass[i]).sum();
        if m <= 0.0 {
            continue;
        }
        let com_pos: DVec3 = members.iter().map(|&i| pos[i] * mass[i]).sum::<DVec3>() / m;
        let com_vel: DVec3 = members.iter().map(|&i| vel[i] * mass[i]).sum::<DVec3>() / m;
        let vol: f64 = members.iter().map(|&i| mass[i] / rho[i]).sum();
        let clump_rho = if vol > 0.0 { m / vol } else { *rho.get(members[0]).unwrap_or(&1.0) };
        let radius = (m / (FOUR_THIRDS_PI * clump_rho)).cbrt();
        // internal KE about the COM (the random motion an inelastic merge would thermalise)
        let internal_ke: f64 = members
            .iter()
            .map(|&i| 0.5 * mass[i] * (vel[i] - com_vel).length_squared())
            .sum();
        // SPIN about the clump's own COM. A point body at the COM already carries the clump's ORBITAL
        // angular momentum (through com_pos × com_vel); this is the remainder, and it is the piece that
        // silently vanishes if the members are deleted without it. A rotating disk is exactly the case
        // where it dominates, so it is carried on the body rather than left "recoverable from members".
        let ang_mom: DVec3 = members
            .iter()
            .map(|&i| mass[i] * (pos[i] - com_pos).cross(vel[i] - com_vel))
            .sum();
        // Split `internal_ke` into the part carried by COHERENT rotation — which the promoted body keeps as
        // spin — and the RANDOM residual, which an inelastic merge genuinely thermalises. Counting the
        // rotational share as heat AND as spin would inject energy through a mere change of representation
        // (docs/44 §7 forbids exactly that). E_rot = ½ ω·L with ω = I⁻¹L about the clump's own COM.
        let mut inertia = glam::DMat3::ZERO;
        for &i in &members {
            let r = pos[i] - com_pos;
            let m = mass[i];
            inertia += glam::DMat3::from_diagonal(DVec3::splat(m * r.length_squared()))
                - glam::DMat3::from_cols(r * (m * r.x), r * (m * r.y), r * (m * r.z));
        }
        // A clump of <3 members, or one whose points are collinear/coincident, has no invertible inertia
        // tensor and no well-defined ω. FLAGGED: those fall back to thermalising the whole internal KE,
        // which over-reports heat for a degenerate rotator. Real clumps are 3-D and well-conditioned; the
        // spin itself (`ang_mom`) is carried regardless, so nothing is lost, only mis-labelled as heat.
        let det = inertia.determinant();
        let e_rot = if members.len() >= 3 && det.is_finite() && det > 0.0 {
            0.5 * (inertia.inverse() * ang_mom).dot(ang_mom)
        } else {
            0.0
        };
        let thermal_ke = (internal_ke - e_rot).max(0.0);
        // Self gravitational binding energy, softened to match the sim, via Barnes-Hut (docs/30).
        //
        // This was an exact O(k²) all-pairs sum with the comment "fine (clumps small)". A clump only stops
        // being small when it UNITES — which is the exact case de-resolution exists to catch — so the test
        // became catastrophically expensive at the moment it started succeeding: measured 1.9 ms at 500
        // members but 492 ms at 8,000, and a real 20-35k disk extrapolates to seconds
        // (`find_clumps_cost_against_clump_size`). `BarnesHut::build` brute-forces below `BRUTE_BELOW`, so
        // small clumps stay EXACT and only large ones take the θ-bounded approximation — pinned to the
        // direct sum in `bhtree`'s tests, and θ→0 converges onto it.
        let self_pe = if members.len() < 2 {
            0.0
        } else {
            let mpos: Vec<DVec3> = members.iter().map(|&i| pos[i]).collect();
            let mmass: Vec<f64> = members.iter().map(|&i| mass[i]).collect();
            let bh = crate::bhtree::BarnesHut::build(&mpos, &mmass, BH_THETA, softening);
            // `bhtree` uses the engine's own G while `find_clumps` takes `g` as a parameter. PE scales
            // linearly in G, so rescale rather than let the two quietly disagree about gravity.
            bh.self_potential_energy(&mpos, &mmass) * (g / crate::orbit::G)
        };
        let bound = internal_ke + self_pe < 0.0;
        // Fluid Roche limit of the remnant for THIS clump's density.
        let d_roche = 2.44 * central_radius * (central_density(central_mass, central_radius) / clump_rho).cbrt();
        let outside_roche = (com_pos - central_pos).length() > d_roche;
        clumps.push(Clump { members, mass: m, com_pos, com_vel, rho: clump_rho, radius, internal_ke, self_pe, ang_mom, thermal_ke, bound, outside_roche });
    }
    clumps
}

fn central_density(mass: f64, radius: f64) -> f64 {
    mass / (FOUR_THIRDS_PI * radius.powi(3))
}

/// Run one accretion pass: promote every clump that [`Clump::accretes`] to a single body at its COM,
/// conserving mass, linear momentum, and centre of mass exactly. Returns the promoted bodies and the sorted
/// list of consumed particle indices (everything else remains a particle).
#[allow(clippy::too_many_arguments)]
pub fn accrete(
    pos: &[DVec3],
    vel: &[DVec3],
    mass: &[f64],
    rho: &[f64],
    linking_length: f64,
    g: f64,
    softening: f64,
    central_pos: DVec3,
    central_mass: f64,
    central_radius: f64,
) -> Accreted {
    let clumps = find_clumps(pos, vel, mass, rho, linking_length, g, softening, central_pos, central_mass, central_radius);
    let mut out = Accreted::default();
    for c in clumps.iter().filter(|c| c.accretes()) {
        out.bodies.push(Body {
            pos: c.com_pos,
            vel: c.com_vel,
            mass: c.mass,
            rho: c.rho,
            radius: c.radius,
            ang_mom: c.ang_mom,
            thermal_j: c.thermal_ke,
        });
        out.consumed.extend_from_slice(&c.members);
    }
    out.consumed.sort_unstable();
    out
}




/// The bowl a measured excavation carves — depth AND radius from one excavated volume (docs/46 row 18).
///
/// The render used to size the bowl's radius from a dial (`0.72·R_surface`) while measuring only its depth,
/// which produced a saucer far too wide for its depth (d/r ~ 0.06) that read as flat and "never showed" —
/// the regression Robin reported repeatedly. Both dimensions now come from the SAME excavated volume plus
/// ONE sourced shape fact: a simple crater's depth is ~0.4 of its radius (Melosh, *Impact Cratering*). For
/// a paraboloid V = ½πR²d = 0.2πR³, so R = (V/0.2π)^⅓ and d = 0.4R. Returns (radius_m, depth_m).
pub fn crater_bowl(excavated_volume_m3: f64) -> (f64, f64) {
    const DEPTH_PER_RADIUS: f64 = 0.4;
    if excavated_volume_m3 <= 0.0 {
        return (0.0, 0.0);
    }
    // V = ½πr²d and d = k·r  ⇒  V = ½πk·r³  ⇒  r = (2V / (πk))^⅓.
    let r = (2.0 * excavated_volume_m3 / (std::f64::consts::PI * DEPTH_PER_RADIUS)).cbrt();
    (r, DEPTH_PER_RADIUS * r)
}

/// The mass above which a body's own gravity overcomes its material strength — the physical boundary
/// between a ROCK and a BODY.
///
/// Below it matter keeps whatever irregular shape it was left in; above it self-gravity crushes it into
/// hydrostatic equilibrium and it is round. That is the honest trigger for promoting a coalesced particle
/// into a layered body: **not a particle count** — merging systematically destroys the count, so a
/// count-based gate becomes unsatisfiable exactly when coalescence succeeds — and not a chosen mass.
///
/// A self-gravitating sphere's central pressure is ~(2π/3)·G·ρ²·R², so it rounds once that exceeds the
/// material's strength σ:  R = sqrt(3σ / (2π·G·ρ²)),  M = (4/3)·π·ρ·R³.
///
/// For rock (σ ~ 10⁸ Pa, ρ ~ 3000) this lands near 300 km — the observed "potato radius" where asteroids
/// stop being lumpy and start being round, which is the check that this is physics and not a fitted number.
pub fn rounding_mass(strength_pa: f64, density: f64) -> f64 {
    let rho = density.max(1.0);
    let r = (3.0 * strength_pa.max(0.0) / (2.0 * std::f64::consts::PI * G_CONST * rho * rho)).sqrt();
    FOUR_THIRDS_PI * rho * r.powi(3)
}

/// Gravitational constant for [`rounding_mass`] — the engine's one value (`orbit::G`).
const G_CONST: f64 = crate::orbit::G;

/// Sample a settled particle set's RADIAL composition into layers — the promote-to-body step (docs/58).
///
/// Robin: *"heavier materials naturally migrate toward the middle when settling, making a sampling of the
/// materials involved easily transferable into a layered particle."* That is the mechanism, and it is
/// precisely why this MEASURES instead of declaring. Under self-gravity denser matter really does sink, so
/// by the time a blob is quiescent its radial material profile IS its layer structure, and reading that
/// profile reports the differentiation the simulation actually performed.
///
/// The alternative — sorting the materials by density and stacking them — would manufacture layers the sim
/// never made, and would look identical for a body that had differentiated and one that had not. Here a
/// body that has NOT differentiated samples as a near-uniform mixture and collapses to ONE layer, which is
/// the honest answer. **The layer COUNT emerges**: adjacent shells with the same dominant material merge,
/// so a well-separated body yields core/mantle/crust and a stirred one yields a single layer.
///
/// Shells are equal-MASS rather than equal-radius, so each carries the same statistical weight instead of
/// the outermost (largest volume, fewest particles) dominating.
#[allow(clippy::too_many_arguments)]
pub fn sample_layers(
    pos: &[DVec3],
    mass: &[f64],
    rho: &[f64],
    material: &[usize],
    names: &[String],
    temp_k: &[f64],
    shells: usize,
) -> Vec<crate::planet::Layer> {
    let n = pos.len();
    if n == 0 || shells == 0 {
        return Vec::new();
    }
    let m_total: f64 = mass.iter().sum();
    if m_total <= 0.0 {
        return Vec::new();
    }
    let com: DVec3 = (0..n).map(|i| pos[i] * mass[i]).sum::<DVec3>() / m_total;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        (pos[a] - com).length_squared().total_cmp(&(pos[b] - com).length_squared())
    });

    // Walk outward, closing a shell each time it has accumulated its share of the mass. Layers accumulate
    // MASS and VOLUME (not averaged densities), so a layer spanning several shells reports its true bulk
    // density rather than a weighted guess.
    struct Acc { name: String, outer_r: f64, mass: f64, vol: f64, t_inner: f64, t_outer: f64 }
    let share = m_total / shells as f64;
    let mut layers: Vec<Acc> = Vec::new();
    let (mut acc, mut vol, mut t_sum) = (0.0f64, 0.0f64, 0.0f64);
    let mut cnt = 0usize;
    let mut inner_t: Option<f64> = None;
    let mut by_mat: std::collections::HashMap<usize, f64> = std::collections::HashMap::new();
    for (k, &i) in order.iter().enumerate() {
        let m = mass[i];
        acc += m;
        cnt += 1;
        vol += m / rho[i].max(1.0);
        t_sum += m * temp_k[i];
        let r_out = (pos[i] - com).length();
        *by_mat.entry(material[i]).or_insert(0.0) += m;
        if inner_t.is_none() {
            inner_t = Some(temp_k[i]);
        }
        let last = k + 1 == n;
        if acc < share && !last {
            continue;
        }
        // A shell's identity is the material holding a MAJORITY of its mass. A near-tie is a MIXTURE, and
        // a mixture is not evidence of a new layer — with 50/50 shells the "winner" is decided by noise and
        // would fabricate a boundary every time it flipped. So an unresolved shell CONTINUES the layer below
        // it; only a genuine majority can open a new one. (>50% is the definition of dominance, not a tuned
        // threshold.) FLAGGED: `Layer` names ONE material, so a true mixture cannot be represented faithfully —
        // the same IOU the shader merge carries, and the resolved form is a mixture EOS.
        let (dom, dom_m) = by_mat
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, v)| (*k, *v))
            .unwrap_or((0, 0.0));
        // Dominance must exceed SAMPLING NOISE to count as evidence: a one-particle imbalance in a 50/50
        // shell is not a layer boundary, or a perfectly stirred body sprouts a spurious boundary wherever a
        // shell held an odd number of particles. The bound is the BINOMIAL standard error of the mass
        // fraction, `sqrt(p(1−p)/cnt)`, at the conventional 2σ significance.
        //
        // The standard error is the right statistic and a flat `1/sqrt(cnt)` was not: it vanishes when
        // `p == 1`, so a PURE shell is unambiguous however few particles it holds. That matters now that
        // merging is same-material — a coalesced body arrives as a handful of pure, massive particles, and
        // a threshold that ignored certainty would have declared every one of them a "mixture" and
        // collapsed a differentiated planet into a single layer.
        let p_dom = dom_m / acc.max(1.0);
        let se = (p_dom * (1.0 - p_dom) / (cnt.max(1) as f64)).sqrt();
        let majority = cnt > 0 && (p_dom - 0.5) > 2.0 * se;
        let name = match (majority, layers.last()) {
            (false, Some(prev)) => prev.name.clone(), // mixture: continue the layer below
            _ => names.get(dom).cloned().unwrap_or_else(|| "basalt".to_string()),
        };
        let t_mean = t_sum / acc.max(1.0);
        match layers.last_mut() {
            // Same material as the shell below ⇒ the SAME layer, still being described. Extend it.
            Some(prev) if prev.name == name => {
                prev.outer_r = r_out;
                prev.mass += acc;
                prev.vol += vol;
                prev.t_outer = t_mean;
            }
            _ => layers.push(Acc {
                name,
                outer_r: r_out,
                mass: acc,
                vol,
                t_inner: inner_t.unwrap_or(t_mean),
                t_outer: t_mean,
            }),
        }
        acc = 0.0;
        vol = 0.0;
        t_sum = 0.0;
        cnt = 0;
        inner_t = None;
        by_mat.clear();
    }
    layers
        .into_iter()
        .map(|a| crate::planet::Layer {
            material: a.name,
            outer_r: a.outer_r,
            density: if a.vol > 0.0 { a.mass / a.vol } else { 0.0 },
            t_inner: a.t_inner,
            t_outer: a.t_outer,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f64 = 6.674e-11;

    // A dense, cold, self-bound blob of `n` particles inside `radius` about `center`, drifting at `bulk`.
    fn cold_blob(center: DVec3, bulk: DVec3, radius: f64, n: usize, m_i: f64, rho: f64) -> (Vec<DVec3>, Vec<DVec3>, Vec<f64>, Vec<f64>) {
        let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
        let (mut p, mut v, mut m, mut r) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for i in 0..n {
            let rr = radius * ((i as f64 + 0.5) / n as f64).cbrt();
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let rad = (1.0 - y * y).max(0.0).sqrt();
            let th = golden * i as f64;
            p.push(center + DVec3::new(th.cos() * rad * rr, y * rr, th.sin() * rad * rr));
            v.push(bulk); // cold: no internal motion ⇒ trivially bound
            m.push(m_i);
            r.push(rho);
        }
        (p, v, m, r)
    }

    fn totals(pos: &[DVec3], vel: &[DVec3], mass: &[f64]) -> (f64, DVec3, DVec3) {
        let mt: f64 = mass.iter().sum();
        let mom: DVec3 = (0..pos.len()).map(|i| vel[i] * mass[i]).sum();
        let com: DVec3 = (0..pos.len()).map(|i| pos[i] * mass[i]).sum::<DVec3>() / mt;
        (mt, mom, com)
    }

    // The accretion result, expanded back to (bodies + residual particles), must have the SAME total mass,
    // linear momentum, and centre of mass as the input — exactly (to f64 round-off).
    #[test]
    fn accretion_conserves_mass_momentum_and_com() {
        // Two well-separated cold blobs (both should accrete) + scattered singletons that must NOT.
        let m_i = 1.0e19;
        let rho = 3000.0;
        let (mut pos, mut vel, mut mass, mut r) = cold_blob(DVec3::new(2.0e7, 0.0, 0.0), DVec3::new(0.0, 1500.0, 0.0), 3.0e5, 40, m_i, rho);
        let (p2, v2, m2, r2) = cold_blob(DVec3::new(-1.5e7, 1.0e7, 0.0), DVec3::new(-800.0, 0.0, 300.0), 2.5e5, 30, m_i, rho);
        pos.extend(p2); vel.extend(v2); mass.extend(m2); r.extend(r2);
        // Scattered lone particles, far apart (each its own singleton clump ⇒ never accretes).
        for k in 0..5 {
            pos.push(DVec3::new(4.0e7 + k as f64 * 5.0e6, -3.0e7, 0.0));
            vel.push(DVec3::new(0.0, 0.0, 500.0 * k as f64));
            mass.push(m_i);
            r.push(rho);
        }

        let (m0, mom0, com0) = totals(&pos, &vel, &mass);
        // remnant far away so both blobs are outside Roche
        let out = accrete(&pos, &vel, &mass, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);

        assert_eq!(out.bodies.len(), 2, "both cold blobs should accrete, singletons should not");
        // Rebuild the full system: promoted bodies + the particles they did NOT consume.
        let consumed: std::collections::HashSet<usize> = out.consumed.iter().copied().collect();
        let (mut fp, mut fv, mut fm) = (Vec::new(), Vec::new(), Vec::new());
        for b in &out.bodies {
            fp.push(b.pos); fv.push(b.vel); fm.push(b.mass);
        }
        for i in 0..pos.len() {
            if !consumed.contains(&i) {
                fp.push(pos[i]); fv.push(vel[i]); fm.push(mass[i]);
            }
        }
        let (m1, mom1, com1) = totals(&fp, &fv, &fm);
        assert!((m1 - m0).abs() / m0 < 1e-12, "mass not conserved: {m0} → {m1}");
        assert!((mom1 - mom0).length() / mom0.length() < 1e-12, "momentum not conserved: {mom0} → {mom1}");
        assert!((com1 - com0).length() / com0.length() < 1e-12, "COM not conserved: {com0} → {com1}");
        // Residual = the 5 singletons.
        assert_eq!(fp.len(), 2 + 5, "2 bodies + 5 residual singletons");
    }


    // docs/58 promote-to-body. Robin: "heavier materials naturally migrate toward the middle when settling,
    // making a sampling of the materials involved easily transferable into a layered particle." So a
    // DIFFERENTIATED blob must sample as core + mantle, and — the half that keeps it honest — a STIRRED one
    // must sample as a single layer rather than being sorted into layers it never formed.
    #[test]
    fn sampling_reads_the_differentiation_the_sim_actually_made() {
        let names = vec!["iron".to_string(), "basalt".to_string()];
        let (r_core, r_surf) = (2.0e6f64, 6.0e6f64);
        let (mut pos, mut mass, mut rho, mut mat, mut temp) = (vec![], vec![], vec![], vec![], vec![]);
        // Settled: iron inside r_core, basalt outside. Equal-volume radii so the shells are well sampled.
        let n = 4000;
        for i in 0..n {
            let rr = r_surf * ((i as f64 + 0.5) / n as f64).cbrt();
            let dir = {
                let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
                let rad = (1.0 - y * y).max(0.0).sqrt();
                let th = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt()) * i as f64;
                DVec3::new(th.cos() * rad, y, th.sin() * rad)
            };
            let core = rr < r_core;
            pos.push(dir * rr);
            mass.push(if core { 1.0e19 } else { 4.0e18 });
            rho.push(if core { 7850.0 } else { 3000.0 });
            mat.push(if core { 0 } else { 1 });
            temp.push(if core { 4000.0 } else { 1600.0 });
        }
        let layers = sample_layers(&pos, &mass, &rho, &mat, &names, &temp, 12);
        assert_eq!(layers.len(), 2, "a differentiated body samples as core + mantle, got {layers:?}");
        assert_eq!(layers[0].material, "iron", "the dense material is the INNER layer");
        assert_eq!(layers[1].material, "basalt");
        assert!(layers[0].outer_r > 0.5 * r_core && layers[0].outer_r < 1.6 * r_core,
            "core boundary should land near r_core, got {}", layers[0].outer_r);
        assert!(layers[1].outer_r > 0.9 * r_surf, "the outer layer reaches the surface");
        assert!(layers[0].density > layers[1].density, "the core must sample denser than the mantle");
        assert!(layers[0].t_inner > layers[1].t_outer, "the measured geotherm falls outward");

        // STIRRED: same materials, but interleaved so nothing has separated. One layer is the honest read.
        let mut smat = mat.clone();
        for (i, m) in smat.iter_mut().enumerate() { *m = i % 2; }
        let mut srho = rho.clone();
        for (i, r) in srho.iter_mut().enumerate() { *r = if i % 2 == 0 { 7850.0 } else { 3000.0 }; }
        let stirred = sample_layers(&pos, &mass, &srho, &smat, &names, &temp, 12);
        assert_eq!(stirred.len(), 1,
            "an undifferentiated body must NOT be sorted into layers it never formed, got {stirred:?}");
    }


    // The regime same-material merging actually produces. Once the shader merges only like with like, a
    // coalesced body does NOT arrive as thousands of particles — it arrives as a HANDFUL of pure, massive
    // ones, each still at its own radius because the dense material sank before it coalesced. Sampling must
    // still read that as layers; if it collapsed them to one, promotion would hand back a homogeneous
    // planet, which is worse than not promoting at all.
    #[test]
    fn a_coalesced_body_of_a_few_pure_particles_still_samples_as_layers() {
        let names = vec!["iron".to_string(), "basalt".to_string()];
        // Four particles: two iron deep, two basalt shallow — the shape a merged, differentiated blob has.
        let pos = vec![
            DVec3::new(3.0e5, 0.0, 0.0),
            DVec3::new(-3.0e5, 2.0e5, 0.0),
            DVec3::new(4.0e6, 0.0, 0.0),
            DVec3::new(-3.5e6, 1.0e6, 0.0),
        ];
        let mass = vec![9.0e22, 8.0e22, 5.0e22, 4.0e22];
        let rho = vec![7850.0, 7850.0, 3000.0, 3000.0];
        let mat = vec![0, 0, 1, 1];
        let temp = vec![5000.0, 4800.0, 1800.0, 1600.0];

        let layers = sample_layers(&pos, &mass, &rho, &mat, &names, &temp, 4);
        assert_eq!(layers.len(), 2, "a few PURE particles must still resolve two layers, got {layers:?}");
        assert_eq!(layers[0].material, "iron", "the dense material is inner");
        assert_eq!(layers[1].material, "basalt");
        assert!(layers[0].density > layers[1].density, "the core samples denser");
        assert!(layers[1].outer_r > layers[0].outer_r, "layers are ordered outward");
        // A pure shell is unambiguous however few particles it holds — this is what the flat 1/sqrt(cnt)
        // threshold got wrong, and it is exactly the case merging now creates.
    }



    // docs/46 row 18: the bowl must keep a crater's SHAPE (depth ~ 0.4 radius), not a dial's. The old code
    // sized radius from 0.72*R_surface and depth from volume, giving d/r ~ 0.06 — a saucer that rendered
    // flat. Both now come from the same excavated volume.
    #[test]
    fn a_crater_bowl_keeps_its_shape_at_every_size() {
        for &v in &[1.0e15, 1.0e18, 3.0e19] {
            let (r, d) = crater_bowl(v);
            assert!(r > 0.0 && d > 0.0, "a real excavation makes a real bowl");
            assert!((d / r - 0.4).abs() < 1.0e-9, "depth must stay ~0.4 of radius, got {:.3}", d / r);
            // The paraboloid must actually hold the volume it was built from.
            let v_bowl = 0.5 * std::f64::consts::PI * r * r * d;
            assert!((v_bowl - v).abs() / v < 1.0e-9, "the bowl must contain the excavated volume");
        }
        // A bigger excavation is a bigger crater in BOTH dimensions, never a flatter one.
        let (r1, d1) = crater_bowl(1.0e15);
        let (r2, d2) = crater_bowl(1.0e18);
        assert!(r2 > r1 && d2 > d1, "more excavation digs wider AND deeper");
        assert_eq!(crater_bowl(0.0), (0.0, 0.0), "no excavation, no crater");
    }

    // The rock/body boundary must reproduce the OBSERVED potato radius, or it is a formula rather than
    // physics. Asteroids stop being lumpy and start being round somewhere near 200-300 km.
    #[test]
    fn the_rounding_mass_reproduces_the_observed_potato_radius() {
        let (sigma, rho) = (1.0e8, 3000.0); // rock
        let m = rounding_mass(sigma, rho);
        let r = (m / (FOUR_THIRDS_PI * rho)).cbrt();
        assert!((1.5e5..5.0e5).contains(&r), "rock should round near 200-300 km, got {:.0} km", r / 1e3);

        // A STRONGER material has to grow bigger before gravity wins; a weaker one rounds sooner.
        assert!(rounding_mass(4.0e8, rho) > m, "stronger material resists rounding to a larger mass");
        assert!(rounding_mass(2.5e7, rho) < m, "weaker material rounds at a smaller mass");
        // Strengthless matter (a liquid) is round at any size.
        assert_eq!(rounding_mass(0.0, rho), 0.0, "a liquid has no shape of its own");
    }

    // STRAGGLERS (Robin, 2026-07-23): "a meteor that impacts the moon becomes part of the moon unless it
    // hits escape velocity on the rebound." Absorbing one must conserve mass, momentum, COM and TOTAL
    // angular momentum — an off-centre strike has to spin the body up, not vanish into it.
    #[test]
    fn a_straggler_is_absorbed_conserving_momentum_and_spin() {
        let g = G;
        let mut b = Body {
            pos: DVec3::new(1.0e7, 0.0, 0.0),
            vel: DVec3::new(0.0, 1000.0, 0.0),
            mass: 7.0e22,
            rho: 3300.0,
            radius: 1.7e6,
            ang_mom: DVec3::new(0.0, 0.0, 1.0e29),
            thermal_j: 5.0e24,
        };
        // An OFF-CENTRE strike: offset in y so it carries angular momentum about the body's centre.
        let (p_pos, p_vel, p_m, p_rho) =
            (b.pos + DVec3::new(0.0, 1.2e6, 0.0), DVec3::new(-400.0, 900.0, 0.0), 3.0e19, 3000.0);
        assert!(b.absorbs(g, p_pos, p_vel), "a slow strike well inside the radius must be absorbed");

        // Totals about a FIXED origin, treating the body as mass + spin.
        let l0 = b.mass * b.pos.cross(b.vel) + b.ang_mom + p_m * p_pos.cross(p_vel);
        let mom0 = b.mass * b.vel + p_m * p_vel;
        let m0 = b.mass + p_m;
        let com0 = (b.pos * b.mass + p_pos * p_m) / m0;
        let spin_before = b.ang_mom;

        b.absorb(p_pos, p_vel, p_m, p_rho);

        let l1 = b.mass * b.pos.cross(b.vel) + b.ang_mom;
        assert!((b.mass - m0).abs() / m0 < 1e-12, "mass not conserved");
        assert!((b.mass * b.vel - mom0).length() / mom0.length() < 1e-12, "momentum not conserved");
        assert!((b.pos - com0).length() / com0.length() < 1e-12, "COM not conserved");
        assert!((l1 - l0).length() / l0.length() < 1e-10, "angular momentum not conserved: {l0} → {l1}");
        assert!(b.ang_mom != spin_before, "an off-centre strike must change the body's spin");
        assert!(b.radius > 1.7e6, "absorbing mass grows the body at its own density");

        // And the gate is real: the SAME strike at well above escape speed is refused.
        let v_esc = (2.0 * g * b.mass / 1.2e6_f64).sqrt();
        assert!(
            !b.absorbs(g, p_pos, b.vel + DVec3::new(0.0, 10.0 * v_esc, 0.0)),
            "a strike above escape velocity must NOT be absorbed"
        );
    }

    // MEASUREMENT (docs/44) — WHERE the clump cost actually is, phase by phase.
    //
    // The first cut of this test used a linking length (5e5) LARGER than the blob it measured (radius 3e5),
    // so every particle landed in one neighbour-grid cell and the friends-of-friends pair loop degenerated
    // to O(N^2). It therefore measured FoF, not the binding sum, and wrongly indicted `self_pe`. Real disks
    // link at ~4x the interparticle spacing, which is what this uses now.
    //
    // Run with `--ignored`; it reports, it does not gate.
    #[test]
    #[ignore]
    fn find_clumps_cost_against_clump_size() {
        let (m_i, rho) = (1.0e19, 3000.0);
        let radius = 3.0e5;
        eprintln!("  members   spacing   link      FoF+all   selfPE(exact)   selfPE(BH)");
        for &n in &[500usize, 1000, 2000, 4000, 8000, 16000] {
            let (pos, vel, mass, r) = cold_blob(DVec3::new(2.0e7, 0.0, 0.0), DVec3::ZERO, radius, n, m_i, rho);
            // Realistic link: ~4x the interparticle spacing, as `2.0 * mean_h` gives in the live path.
            let spacing = (FOUR_THIRDS_PI * radius.powi(3) / n as f64).cbrt();
            let link = 4.0 * spacing;

            let t0 = std::time::Instant::now();
            let clumps = find_clumps(&pos, &vel, &mass, &r, link, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
            let ms_all = t0.elapsed().as_secs_f64() * 1e3;
            let biggest = clumps.iter().map(|c| c.members.len()).max().unwrap_or(0);

            // The binding sum in isolation, both ways, over the WHOLE set.
            let t1 = std::time::Instant::now();
            let mut exact = 0.0f64;
            for a in 0..n {
                for b in (a + 1)..n {
                    let rr = ((pos[a] - pos[b]).length_squared() + 1.0e4f64 * 1.0e4).sqrt();
                    exact -= G * mass[a] * mass[b] / rr;
                }
            }
            let ms_exact = t1.elapsed().as_secs_f64() * 1e3;

            let t2 = std::time::Instant::now();
            let bh = crate::bhtree::BarnesHut::build(&pos, &mass, BH_THETA, 1.0e4);
            let approx = bh.self_potential_energy(&pos, &mass);
            let ms_bh = t2.elapsed().as_secs_f64() * 1e3;

            let rel = ((approx - exact) / exact).abs();
            eprintln!(
                "  {:7} {:9.0} {:9.0} {:9.1} {:15.1} {:12.1}   (rel {:.1e}, biggest clump {})",
                n, spacing, link, ms_all, ms_exact, ms_bh, rel, biggest
            );
        }
    }

    // A self-bound blob that also SPINS rigidly about `omega` — a rotating disk in miniature, and the case
    // that separates a body which carries its spin from one that quietly drops it.
    fn spinning_blob(
        center: DVec3, bulk: DVec3, omega: DVec3, radius: f64, n: usize, m_i: f64, rho: f64,
    ) -> (Vec<DVec3>, Vec<DVec3>, Vec<f64>, Vec<f64>) {
        let (p, _, m, r) = cold_blob(center, bulk, radius, n, m_i, rho);
        let v = p.iter().map(|&q| bulk + omega.cross(q - center)).collect();
        (p, v, m, r)
    }

    // TOTAL angular momentum about a fixed origin must survive promotion: the body carries its ORBITAL L
    // through pos×vel and its SPIN L in `ang_mom`. Delete the spin field and this fails by the whole spin
    // share for a rotating clump — while mass, momentum and COM all still balance exactly. That is
    // precisely how the missing term stayed invisible for as long as it did.
    #[test]
    fn accretion_conserves_angular_momentum_of_a_spinning_clump() {
        let (m_i, rho) = (1.0e19, 3000.0);
        let omega = DVec3::new(0.0, 0.0, 1.0e-4); // slow enough that the blob stays self-bound
        let (pos, vel, mass, r) =
            spinning_blob(DVec3::new(2.0e7, 0.0, 0.0), DVec3::new(0.0, 1500.0, 0.0), omega, 3.0e5, 60, m_i, rho);

        let l_before: DVec3 = (0..pos.len()).map(|i| mass[i] * pos[i].cross(vel[i])).sum();
        let out = accrete(&pos, &vel, &mass, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
        assert_eq!(out.bodies.len(), 1, "the spinning blob should accrete as one body");

        let b = out.bodies[0];
        let l_after = b.mass * b.pos.cross(b.vel) + b.ang_mom;
        assert!(
            (l_after - l_before).length() / l_before.length() < 1.0e-10,
            "angular momentum not conserved: {l_before} → {l_after}"
        );
        // ...and the spin is a term worth carrying, not round-off dressed up as physics.
        assert!(
            b.ang_mom.length() / l_before.length() > 1.0e-9,
            "spin should be a measurable share of L, else this test proves nothing"
        );
        // The BODY (not just the clump) must also report this rigid rotator's heat as ~zero: the promoted
        // record is what a caller keeps after the members are deleted, so it is the one that has to be right.
        assert!(
            b.thermal_j < 1.0e-9 * (0.5 * b.mass * b.vel.length_squared()).max(1.0),
            "a rigid rotator thermalises nothing, but the body reports {} J",
            b.thermal_j
        );
    }

    // The energy split. A RIGID rotator converts NONE of its internal KE to heat — it is all coherent spin,
    // which the body keeps. Incoherent motion converts nearly all of it. Counting rotation as both spin and
    // heat would inject energy through a change of representation alone (docs/44 §7).
    #[test]
    fn rigid_rotation_is_carried_as_spin_not_as_heat() {
        let (m_i, rho) = (1.0e19, 3000.0);
        let omega = DVec3::new(0.0, 0.0, 1.0e-4);
        let (pos, vel, mass, r) =
            spinning_blob(DVec3::new(2.0e7, 0.0, 0.0), DVec3::ZERO, omega, 3.0e5, 60, m_i, rho);
        let clumps = find_clumps(&pos, &vel, &mass, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
        assert_eq!(clumps.len(), 1, "the blob is one clump at this linking length");
        let c = &clumps[0];
        assert!(c.internal_ke > 0.0, "a rotating clump has internal KE about its COM");
        assert!(
            c.thermal_ke / c.internal_ke < 1.0e-9,
            "rigid rotation must be carried as spin, not thermalised: {} of {}",
            c.thermal_ke,
            c.internal_ke
        );

        // Now add INCOHERENT jitter on top of the same rotation: that part must show up as heat.
        let vel2: Vec<DVec3> = (0..pos.len())
            .map(|i| {
                let h = ((i as f64 * 12.9898).sin() * 43758.5453).fract();
                let k = ((i as f64 * 78.233).sin() * 27183.2818).fract();
                vel[i] + DVec3::new(h - 0.5, k - 0.5, (h + k).fract() - 0.5) * 20.0
            })
            .collect();
        let c2 = &find_clumps(&pos, &vel2, &mass, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6)[0];
        assert!(
            c2.thermal_ke / c2.internal_ke > 0.1,
            "incoherent motion must thermalise: {} of {}",
            c2.thermal_ke,
            c2.internal_ke
        );
        // Bookkeeping closes: internal KE is exactly spin-energy + heat, nothing created or lost.
        let e_rot = c2.internal_ke - c2.thermal_ke;
        assert!(e_rot >= -1.0e-6 * c2.internal_ke, "rotational share must not go negative");
    }

    // A clump INSIDE the Roche limit must NOT accrete (it should shred); the SAME clump outside Roche must.
    #[test]
    fn roche_gate_blocks_accretion_inside_the_limit() {
        let (m_planet, r_planet) = (5.0e24, 6.0e6);
        let rho_clump = 3000.0;
        let d_roche = 2.44 * r_planet * (central_density(m_planet, r_planet) / rho_clump).cbrt();

        let inside = DVec3::new(0.6 * d_roche, 0.0, 0.0);
        let (p, v, m, r) = cold_blob(inside, DVec3::ZERO, 2.0e5, 30, 1.0e19, rho_clump);
        let out_in = accrete(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, m_planet, r_planet);
        assert_eq!(out_in.bodies.len(), 0, "clump inside Roche must not accrete (shreds instead)");

        let outside = DVec3::new(2.0 * d_roche, 0.0, 0.0);
        let (p, v, m, r) = cold_blob(outside, DVec3::ZERO, 2.0e5, 30, 1.0e19, rho_clump);
        let out_out = accrete(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, m_planet, r_planet);
        assert_eq!(out_out.bodies.len(), 1, "same clump outside Roche must accrete");
    }

    // A spatially-tight but HOT group (internal KE ≫ binding energy) is unbound and must NOT accrete.
    #[test]
    fn unbound_hot_group_does_not_accrete() {
        let rho = 3000.0;
        let m_i = 1.0e15; // tiny masses ⇒ negligible self-gravity
        let (mut p, mut v, mut m, mut r) = cold_blob(DVec3::new(3.0e7, 0.0, 0.0), DVec3::ZERO, 2.0e5, 30, m_i, rho);
        // Give every particle a large random-ish velocity about the COM: hot, unbound.
        for (i, vi) in v.iter_mut().enumerate() {
            let s = if i % 2 == 0 { 1.0 } else { -1.0 };
            *vi = DVec3::new(s * 5000.0, s * -4000.0, s * 3000.0);
        }
        let _ = (&mut p, &mut m, &mut r);
        let clumps = find_clumps(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
        // It IS one spatial clump, but not bound.
        let big = clumps.iter().max_by_key(|c| c.members.len()).unwrap();
        assert!(big.members.len() >= 25, "should be one spatial group");
        assert!(!big.bound, "hot group must be classified unbound");
        let out = accrete(&p, &v, &m, &r, 5.0e5, G, 1.0e4, DVec3::ZERO, 5.0e24, 6.0e6);
        assert_eq!(out.bodies.len(), 0, "unbound hot group must not accrete");
    }
}

/// **How much of a set of particles is still one body**: the fraction of its mass lying within
/// `coherence_radius` of its own centre of mass.
///
/// 1.0 is an intact body. It falls as the body is torn apart and climbs again as debris re-accretes,
/// which makes it the measurement that decides HOW MATTER IS DRAWN: a coherent body has a surface, a
/// disrupted one does not. That replaced a "Pretty ⇄ Physics" slider which cross-faded a resolved surface
/// against the particle field — two representations of the same matter, mixed by hand, which is why the
/// surface was seen racing the particles and being swallowed by the disk.
///
/// It lives here rather than on a scene because "is this still a body" is a question about matter, not
/// about a camera or a scenario. Any scene with particles can ask it.
///
/// FLAGGED: a radius test is cruder than [`find_clumps`], which resolves genuine self-bound membership.
/// This is the cheap per-frame answer; that is the honest one.
pub fn coherence(positions: &[glam::DVec3], masses: &[f64], coherence_radius: f64) -> f64 {
    let total: f64 = masses.iter().sum();
    if total <= 0.0 || positions.is_empty() {
        return 0.0;
    }
    let com: glam::DVec3 = positions
        .iter()
        .zip(masses)
        .map(|(p, m)| *p * *m)
        .sum::<glam::DVec3>()
        / total;
    let r2 = coherence_radius * coherence_radius;
    let inside: f64 = positions
        .iter()
        .zip(masses)
        .filter(|(p, _)| (**p - com).length_squared() <= r2)
        .map(|(_, m)| *m)
        .sum();
    inside / total
}

#[cfg(test)]
mod coherence_tests {
    use super::coherence;
    use glam::DVec3;

    /// Coherence must read 1 for an intact body, fall as it is torn apart, and recover as debris
    /// re-gathers — because that is the signal the renderer uses to decide whether matter has a surface.
    #[test]
    fn coherence_tracks_a_body_coming_apart_and_back_together() {
        let r = 1.0e6;
        // A filled sphere: everything within the coherence radius.
        let intact: Vec<DVec3> = (0..64)
            .map(|i| {
                let t = i as f64 / 64.0 * std::f64::consts::TAU;
                DVec3::new(t.cos(), t.sin(), 0.0) * (0.6 * r)
            })
            .collect();
        let m = vec![1.0; intact.len()];
        assert!((coherence(&intact, &m, 1.2 * r) - 1.0).abs() < 1e-12, "an intact body reads 1");

        // Blow half of it far away: coherence falls to the fraction left behind.
        let mut torn = intact.clone();
        for p in torn.iter_mut().take(32) {
            *p *= 50.0;
        }
        let c = coherence(&torn, &m, 1.2 * r);
        assert!(c < 0.6, "a body half dispersed is no longer coherent (got {c:.2})");

        // Bring it back: the measurement recovers, which is the re-accretion half of the transition.
        let regathered: Vec<DVec3> = torn.iter().map(|p| *p * 0.02).collect();
        let back = coherence(&regathered, &m, 1.2 * r);
        assert!(back > c, "re-gathered debris reads more coherent again ({back:.2} vs {c:.2})");

        // Degenerate inputs answer rather than panicking.
        assert_eq!(coherence(&[], &[], r), 0.0);
        assert_eq!(coherence(&intact, &vec![0.0; intact.len()], r), 0.0, "massless is not a body");
    }
}

/// **Where a body is and how big it still is**, measured from its particles: the clipped centre of mass
/// and the radius enclosing 98% of the mass around it. Returns `None` for an empty set.
///
/// Both halves are clipped because neither a mass-weighted mean nor a farthest-particle radius survives
/// a few escapees — a single particle carrying under a percent of the mass, flung twenty radii out, moves
/// the centre by a quarter of the body's radius. Two passes converge immediately and are a no-op on a
/// clean body.
///
/// This is what lets a struck planet SHRINK instead of vanishing. Drawing a body at a fixed radius and
/// fading it out as it loses mass makes it flicker and disappear, because the fade is answering the wrong
/// question: the remnant is still a body, just a smaller one. Re-measuring every frame draws what is
/// actually there.
pub fn body_extent(positions: &[glam::DVec3], masses: &[f64]) -> Option<(glam::DVec3, f64)> {
    let total: f64 = masses.iter().sum();
    if positions.is_empty() || total <= 0.0 {
        return None;
    }
    let com = |keep: &[usize]| -> glam::DVec3 {
        let m: f64 = keep.iter().map(|&i| masses[i]).sum();
        if m <= 0.0 {
            return glam::DVec3::ZERO;
        }
        keep.iter().map(|&i| positions[i] * masses[i]).sum::<glam::DVec3>() / m
    };
    let radius = |keep: &[usize], c: glam::DVec3| -> f64 {
        let m: f64 = keep.iter().map(|&i| masses[i]).sum();
        let mut rr: Vec<(f64, f64)> =
            keep.iter().map(|&i| ((positions[i] - c).length(), masses[i])).collect();
        rr.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let (mut cum, mut r) = (0.0, 0.0);
        for &(rad, mass) in &rr {
            cum += mass;
            r = rad;
            if cum >= 0.98 * m {
                break;
            }
        }
        r
    };
    // Seed with the per-axis MEDIAN, not the mean. A giant impact can throw off tens of percent of a
    // body, and a mean seeded from that lands between the remnant and the ejecta. A median ignores any
    // minority however far it has gone.
    let median = |mut v: Vec<f64>| -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v[v.len() / 2]
    };
    let mut c = glam::DVec3::new(
        median(positions.iter().map(|p| p.x).collect()),
        median(positions.iter().map(|p| p.y).collect()),
        median(positions.iter().map(|p| p.z).collect()),
    );
    // Clip on the MEDIAN DISTANCE, not on a mass percentile. The percentile is computed over whatever it
    // is given, so on the first pass it includes the ejecta, comes out huge, and the clip then keeps
    // everything — the robustness never engages. A median distance describes the bulk by construction.
    let mut keep: Vec<usize> = (0..positions.len()).collect();
    for _ in 0..4 {
        let med = median(positions.iter().map(|p| (*p - c).length()).collect());
        let next: Vec<usize> =
            (0..positions.len()).filter(|&i| (positions[i] - c).length() <= 3.0 * med).collect();
        if next.is_empty() {
            break;
        }
        keep = next;
        c = com(&keep);
    }
    // Now that `keep` is the bulk, the 98%-of-mass radius describes it honestly.
    let r = radius(&keep, c);
    (r > 0.0).then_some((c, r))
}

#[cfg(test)]
mod extent_tests {
    use super::body_extent;
    use glam::DVec3;

    /// A struck body must be measured as the body that is LEFT, not written off because some of it flew
    /// away. Earth was drawn at a fixed radius and faded by the fraction of mass still nearby, so as the
    /// impact dispersed material it flickered and then vanished — while a perfectly good remnant was
    /// sitting there.
    #[test]
    fn a_struck_body_shrinks_rather_than_vanishing() {
        let r = 1.0e6;
        let shell: Vec<DVec3> = (0..200)
            .map(|i| {
                let t = i as f64 / 200.0 * std::f64::consts::TAU;
                DVec3::new(t.cos(), t.sin(), 0.0) * r
            })
            .collect();
        let m = vec![1.0; shell.len()];
        let (c0, r0) = body_extent(&shell, &m).expect("an intact body has an extent");
        assert!(c0.length() < 1e-6 * r, "centred at the origin");
        assert!((r0 - r).abs() < 0.05 * r, "and measured at its own radius");

        // Blow 40% of it far away. What is left is a smaller body — still centred, still measurable.
        let mut struck = shell.clone();
        for p in struck.iter_mut().take(80) {
            *p *= 30.0;
        }
        let (c1, r1) = body_extent(&struck, &m).expect("a remnant is still a body");
        // The remnant's centre DOES move — 40% was blown off one side, so its centre of mass really is
        // displaced. What must not happen is the measurement chasing the ejecta out to 30 radii.
        assert!(c1.length() < 1.0 * r, "the remnant is found, not the debris ({:.2e} vs r {r:.1e})", c1.length());
        assert!(r1 < 3.0 * r, "and its radius describes the remnant, not the debris field ({r1:.3e})");
        // Decisive: the measured body is nowhere near where a naive mass-weighted mean would put it.
        let naive: DVec3 = struck.iter().sum::<DVec3>() / struck.len() as f64;
        assert!(
            c1.length() < 0.5 * naive.length(),
            "clipping must beat the plain mean ({:.2e} vs {:.2e})", c1.length(), naive.length()
        );

        assert!(body_extent(&[], &[]).is_none(), "nothing has no extent");
    }
}

/// **When a solid body must become particles.** The separation at which tidal stress across a body of
/// mass `m1`, radius `r1` reaches `tidal_fraction` of its own surface gravity, with a companion `m2`:
///
///   a_tide/g = 2·(m2/m1)·(r1/d)³   ⇒   d = r1·(2·(m2/m1)/f)^⅓
///
/// Above it, a body is a body: rigid, whole, and drawn as a surface. Below it, the interaction can no
/// longer be represented by two point masses and the engine must resolve real matter — JIT, from the
/// body's own definition. Coming back out the other side, coalesced matter resolves into a body again
/// (see [`coherence`] and [`body_extent`]).
///
/// The number is derived, not chosen: for proto-Earth and Theia at f = 1% it lands at 17,700 km, which is
/// 1.86× their contact distance. A body is not particles because a scene said so; it is particles because
/// the physics stopped being representable any other way.
pub fn resolution_distance(m1: f64, r1: f64, m2: f64, tidal_fraction: f64) -> f64 {
    if m1 <= 0.0 || r1 <= 0.0 || m2 <= 0.0 || tidal_fraction <= 0.0 {
        return 0.0;
    }
    r1 * (2.0 * (m2 / m1) / tidal_fraction).cbrt()
}

#[cfg(test)]
mod resolution_tests {
    use super::resolution_distance;

    /// The threshold has to behave like tides do, or "resolve when it matters" means nothing.
    #[test]
    fn bodies_resolve_into_particles_when_tides_start_to_matter() {
        const M_EARTH: f64 = 5.435e24; // proto-Earth
        const R_EARTH: f64 = 6.161e6;
        const M_THEIA: f64 = 6.477e23;

        let d = resolution_distance(M_EARTH, R_EARTH, M_THEIA, 0.01);
        assert!((17.0e6..19.0e6).contains(&d), "1% tidal stress at ~17,700 km, got {:.0} km", d / 1e3);
        // Comfortably outside contact — matter is resolved BEFORE the bodies touch, not after.
        assert!(d > 9.551e6, "resolution must begin before contact");

        // A heavier companion reaches in further; a stricter tolerance does too. Both are ∛ scalings.
        assert!(resolution_distance(M_EARTH, R_EARTH, 2.0 * M_THEIA, 0.01) > d, "a bigger companion, sooner");
        assert!(resolution_distance(M_EARTH, R_EARTH, M_THEIA, 0.001) > d, "a stricter threshold, sooner");
        // And a body with no companion never needs resolving.
        assert_eq!(resolution_distance(M_EARTH, R_EARTH, 0.0, 0.01), 0.0);
    }
}

/// How the engine should represent a piece of matter right now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Representation {
    /// Still one body: draw its surface, at this centre and this radius. `coherence` is how much of it
    /// is still itself (1.0 = untouched) — carried here because the caller needs it to hand over to the
    /// particles smoothly, and because computing it twice is how the two answers start to disagree.
    Surface { centre: glam::DVec3, radius: f64, coherence: f64 },
    /// No longer one body: the particles ARE the matter, draw those.
    Particles,
}

/// **THE representation decision, for any matter at any scale.**
///
/// This is the engine's answer to "is this a body or is this particles", and it is deliberately the ONE
/// answer: a Mars-sized impactor striking a proto-planet and a droplet striking a petal are the same
/// mechanic. What differs between them is the energy involved and whether the interaction is close enough
/// to need real matter resolved — not the rules, and not the code path.
///
/// The decision, in order:
///   1. If matter has already been resolved and it is no longer coherent, it is particles. Matter that
///      has come apart cannot be drawn as a surface without lying about where it is.
///   2. Otherwise it is a body — measured from its particles when they exist, and from its declared
///      position and radius when they do not. A body that has not been touched is drawn as a body,
///      whether or not the engine happens to be holding a particle field for it.
///
/// `companion` is `(mass, separation)` of whatever it is interacting with; with it, callers can ask
/// [`resolution_distance`] whether matter needs resolving at all yet.
pub fn representation(
    resolved: Option<(&[glam::DVec3], &[f64])>,
    declared_centre: glam::DVec3,
    declared_radius: f64,
    coherence_floor: f64,
) -> Representation {
    match resolved {
        Some((pos, mass)) if !pos.is_empty() => match body_extent(pos, mass) {
            Some((centre, radius)) => {
                // TWO conditions, because coherence alone is not enough: matter blown outward as a shell
                // keeps a perfectly tidy centre and radius and would read as an enormous intact body. So
                // also require that it still occupies roughly the space a body of this kind occupies.
                // Beyond that it is a debris field, whatever shape it happens to hold.
                let compact = radius <= 2.0 * declared_radius;
                let c = coherence(pos, mass, 1.2 * radius);
                if compact && c >= coherence_floor {
                    Representation::Surface { centre, radius, coherence: c }
                } else {
                    Representation::Particles
                }
            }
            None => Representation::Particles,
        },
        // Nothing resolved: the body is whole by definition, because nothing has happened to it.
        // Nothing resolved: whole by definition, because nothing has happened to it.
        _ => Representation::Surface { centre: declared_centre, radius: declared_radius, coherence: 1.0 },
    }
}

#[cfg(test)]
mod representation_tests {
    use super::*;
    use glam::DVec3;

    /// **The same rule, eleven orders of magnitude apart.** This is the engine's whole premise: a
    /// Mars-sized body striking a proto-planet and a water droplet striking a petal are the same
    /// mechanic, differing in energy and in whether matter must be resolved — never in the rules.
    ///
    /// So the test runs both through the IDENTICAL functions and asserts the identical behaviour: whole
    /// while whole, particles once disrupted, a body again once gathered. If the two ever need different
    /// code, that is the bug.
    #[test]
    fn a_planet_and_a_droplet_follow_the_same_rule() {
        // (name, body radius in metres, companion mass ratio)
        let scales = [
            ("Theia striking proto-Earth", 3.39e6_f64, 0.12_f64),
            ("a raindrop striking a petal", 1.5e-3_f64, 0.12_f64),
        ];
        for (what, r, _ratio) in scales {
            let centre = DVec3::new(7.0 * r, 0.0, 0.0);

            // 1. UNRESOLVED — nothing has happened to it, so it is a body, wherever the scene put it.
            let whole = representation(None, centre, r, 0.75);
            assert_eq!(
                whole,
                Representation::Surface { centre, radius: r, coherence: 1.0 },
                "{what}: untouched ⇒ a whole body"
            );

            // 2. RESOLVED AND INTACT — matter exists but still holds together, so still a surface, now
            //    measured from the matter itself rather than from the declaration.
            let ball: Vec<DVec3> = (0..120)
                .map(|i| {
                    let t = i as f64 / 120.0 * std::f64::consts::TAU;
                    centre + DVec3::new(t.cos(), t.sin(), 0.0) * (0.9 * r)
                })
                .collect();
            let m = vec![1.0; ball.len()];
            match representation(Some((&ball, &m)), centre, r, 0.75) {
                Representation::Surface { centre: c, radius, coherence } => {
                    assert!(coherence > 0.9, "{what}: intact matter reads as coherent ({coherence:.2})");
                    assert!((c - centre).length() < 0.1 * r, "{what}: measured where the matter is");
                    assert!(radius > 0.5 * r && radius < 1.5 * r, "{what}: and at its measured size");
                }
                other => panic!("{what}: intact matter must be a surface, got {other:?}"),
            }

            // 3. DISRUPTED — flung across a hundred radii, it is no longer one thing.
            let spray: Vec<DVec3> = (0..120)
                .map(|i| {
                    let t = i as f64 / 120.0 * std::f64::consts::TAU;
                    centre + DVec3::new(t.cos(), t.sin(), 0.3) * (100.0 * r * (i % 7 + 1) as f64)
                })
                .collect();
            assert_eq!(
                representation(Some((&spray, &m)), centre, r, 0.75),
                Representation::Particles,
                "{what}: matter that has come apart is particles"
            );

            // 4. GATHERED AGAIN — and it resolves back into a body. Both directions, same rule.
            let regathered: Vec<DVec3> = spray.iter().map(|p| centre + (*p - centre) * 0.002).collect();
            assert!(
                matches!(representation(Some((&regathered, &m)), centre, r, 0.75), Representation::Surface { .. }),
                "{what}: re-gathered matter is a body again"
            );
        }

        // And the threshold that decides WHEN to resolve scales the same way — it is a ratio, so the
        // droplet's resolve distance is the same multiple of its own radius as the planet's is.
        let planet = resolution_distance(5.4e24, 6.16e6, 6.5e23, 0.01) / 6.16e6;
        let drop = resolution_distance(5.4e-6, 1.5e-3, 6.5e-7, 0.01) / 1.5e-3;
        assert!((planet - drop).abs() < 1e-9, "the resolve threshold is scale-free ({planet} vs {drop})");
    }
}

/// The hand-over between a surface and its particles, as a 0..1 weight on the SURFACE.
///
/// Written once, here, beside the measurement it consumes. It was written twice — for the target and for
/// the impactor — with the same intent and no guarantee the two would stay equal, which is the ordinary
/// way one rule quietly becomes two.
pub fn surface_weight(coherence: f64, floor: f64) -> f32 {
    let t = ((coherence - floor) / 0.25).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

/// The tidal fraction at which the engine resolves matter into particles: 1% of a body's own surface
/// gravity. Named because it was typed at two call sites that must never disagree — one deciding when to
/// resolve, the other deciding where to start the approach.
pub const RESOLVE_TIDAL_FRACTION: f64 = 0.01;
