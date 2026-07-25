//! **Refinement: the celestial field initializes the local patch, conserved** (docs/59).
//!
//! The upward rung of resolution-by-necessity for the SPH field: when the zoom commits, the coarse
//! celestial particles inside the committed region split ONCE into fine children that carry exactly
//! the coarse field's mass, momentum, angular momentum, kinetic and internal energy, then relax
//! against the coarse field's own interpolated density before the event proceeds. This is the
//! conserved hand-down of docs/59 (order-of-work item 3), the mirror of `crate::recohere`'s
//! downward rung: matter changes representation, never amount (Law IV).
//!
//! **UNWIRED BY DESIGN (flagged IOU, Law V).** This module has no production consumer yet. The
//! camera-driven materialization trigger and the scene wiring belong to the M4 zoom
//! materialization milestone (docs/59 order-of-work item 2 and its open questions), which owns
//! where the entry point lives; the docs/46 ledger carries the row. Landing the rung unwired is
//! deliberate: the trigger's home depends on collision routing decisions (docs/58 item 7) that are
//! not this module's to make.
//!
//! # The split (one-shot, conserving by construction)
//!
//! Each parent splits on the icosahedron stencil with a mandatory retained center child, 13
//! children in all (Vacondio et al. 2016, CMAME 300:442; error analysis per Feldman and Bonet
//! 2007, IJNME 72:295). Children inherit the parent's velocity and specific internal energy;
//! child masses sum exactly to the parent's (the center child absorbs the division remainder in
//! f64 before the f32 cast). Because the 12 vertex directions sum to zero and vertex children
//! share one mass, the children's barycenter IS the parent position, so mass, momentum, angular
//! momentum, kinetic and internal energy are conserved exactly up to f32 rounding. The one-shot
//! (not continuously adaptive) shape follows production impact practice: resolution is graded in
//! the initial conditions, not mid-run (Kegerreis et al. 2019, MNRAS 487:5029; Stickle et al.
//! 2022; Owen et al. 2022, Planet. Sci. J. 3).
//!
//! # The two stencil constants, re-derived for THIS kernel
//!
//! The literature values (separation 0.4 h, child smoothing 0.9 h) were derived for kernels in the
//! 2h-support convention; this engine's ONE cubic-spline kernel ([`crate::atmosphere::sph_w`])
//! carries its full support inside r < h with sigma = 8/(pi h^3), so the constants were re-derived
//! offline rather than copied (Law VII). Method: least squares of the integrated squared density
//! error between the parent's kernel and the 13-child stencil,
//! `E(lambda, alpha) = integral over R^3 of (rho_children - rho_parent)^2 dV`, with children of
//! equal mass m/13 at the 12 icosahedron vertices (circumradius `lambda * h`) plus the retained
//! center, child smoothing `alpha * h`. Quadrature on a 141^3 grid over [-1.4h, 1.4h]^3, verified
//! unchanged at 201^3; Nelder-Mead for the minimum.
//!
//! The UNCONSTRAINED problem is degenerate: E has its global minimum at lambda -> 0, alpha -> 1
//! (13 co-located children reproduce the parent exactly and refine nothing). The physical answer
//! is the interior stationary point, the smallest-error genuinely refining split:
//!
//! - `lambda = 0.3051` (separation / h), against 0.2 for the literature pair mapped into this
//!   kernel's support-radius convention;
//! - `alpha = 0.7915` (child h / parent h), against 0.9 in the literature;
//! - residual of the optimal stencil: relative L2 density error 0.70 percent, peak pointwise
//!   error 0.38 percent of the parent's central density. The literature pair evaluated on THIS
//!   kernel gives 4.9 percent relative L2, which is why the re-derivation was not optional.
//!
//! The stencil orientation is one fixed global icosahedron for every parent: deterministic and
//! reproducible; the relax pass owns removing any alignment artifact. Constants were derived in
//! the scatter (own-h) density convention the literature uses; the release criterion below is
//! measured in the engine's own symmetrized pairwise-h convention (`sph_step.wgsl` uses
//! h_ij = (h_i + h_j)/2), so the relax owns the difference between the two conventions too.
//!
//! # Relax, then release (frozen clock)
//!
//! With a stiff Tillotson material a small density error is a large pressure error, so children do
//! not enter the dynamics as split. Their positions relax against the density the engine's own sum
//! would read at each site in the ORIGINAL coarse field (the accepted initialization discipline:
//! Diehl et al. 2015, arXiv:1211.0525), by damped position shifting down the density-error
//! gradient, with the clock frozen: velocities, masses and internal energies are never touched.
//! The coarse exterior is held fixed as a guard band and pushes back through its own density error
//! (interface treatment per Chiron et al. 2018, JCP 354:552). RELEASE IS A DENSITY-ERROR BOUND,
//! not an iteration count: the patch releases when every child's relative density error is within
//! [`RELEASE_DENSITY_ERROR`]; a generous iteration cap exists only as a divergence guard and
//! reaching it is a stated [`Refusal`], never a silent release.
//!
//! Conservation across the relax: mass, momentum, kinetic and internal energy are exactly
//! untouched (positions are the only state written). Angular momentum drifts by at most
//! `sum_i m_i |dx_i| |v_i|` (triangle inequality over the applied shifts); the audit accumulates
//! that bound and reports it next to the actual drift.
//!
//! # Interface discipline (refusal, not smoothing-over)
//!
//! Zoom-in practice: adjacent resolution levels only, buffer shells between levels, and
//! contamination as a first-class failure (Hahn and Abel 2011, arXiv:1103.6031). One split is one
//! rung of the ladder (the icosahedron's own quantum, mass ratio 13, linear ratio 13^(1/3) = 2.35,
//! this scheme's realization of the factor-2-per-level discipline). Enforced, by refusal with a
//! stated reason rather than any quiet fix:
//!
//! - a region whose interior already spans more than one rung of mass is refused (a coarse
//!   particle inside the fine region is contamination, [`Refusal::Contaminated`]);
//! - a split whose children would interact with matter more than one rung coarser than the
//!   parents is refused until the shell is refined first ([`Refusal::RatioExceeded`]);
//! - [`contamination_check`] is the standing per-step check the future wiring runs: a coarse
//!   particle that penetrates the fine region invalidates the run and says so.
//!
//! # The validation gate (for the future end-to-end test)
//!
//! [`rim_radius_gravity_m`] and [`pi_scaling_gate`] carry Holsapple-Housen pi-group crater scaling
//! in the gravity regime (Holsapple 1993, Annu. Rev. Earth Planet. Sci. 21:333), with the
//! coefficient rows docs/59 records from the Holsapple-Housen v2.2.1 table: hard rock K1 = 0.012,
//! mu = 0.55; regolith K1 = 0.14, mu = 0.4. Factor-of-two agreement in rim size passes; when the
//! predicted crater rivals the body's own radius the gate degrades, explicitly, to an
//! order-of-magnitude sanity bound, because pi-scaling assumes a point source on a half-space.

use crate::gpu_sph::SphParticle;
use glam::{DVec3, Vec3};
use std::fmt;

/// Children per split: 12 icosahedron vertices plus the mandatory retained center child.
pub const SPLIT_CHILDREN: usize = 13;

/// Separation of the vertex children from the parent, as a fraction of the parent's h (this
/// kernel's full support radius). Derived offline for [`crate::atmosphere::sph_w`]; see module doc.
pub const SPLIT_SEPARATION_OVER_H: f32 = 0.3051;

/// Child smoothing as a fraction of the parent's h. Derived offline together with the separation.
pub const SPLIT_CHILD_H_OVER_H: f32 = 0.7915;

/// The release criterion of the relax: every child's relative density error against the coarse
/// field must be within this bound before the patch may enter the dynamics. A bound, not an
/// iteration count: shifting continues exactly until the worst child is inside it. Half a percent
/// is an order below the raw split blip the native tests measure (7.5e-2 on a uniform basalt
/// field, 9.5e-2 across a basalt/iron interface) and is the stated density fidelity of the
/// hand-down; the corresponding Tillotson pressure error at the release bound is the flagged
/// residual the energy ledger of the future end-to-end test will show.
pub const RELEASE_DENSITY_ERROR: f64 = 5.0e-3;

/// One refinement rung's mass ratio: 13 equal children per parent.
pub const LEVEL_MASS_RATIO: f64 = 13.0;

/// sqrt(13): the geometric midpoint between adjacent rungs' masses. A particle heavier than this
/// times the fine mass is "of a coarser rung" for contamination and interface purposes.
const LEVEL_EDGE: f64 = 3.605_551_275_463_989;

/// Damping factor of the shifting iteration (a solver step size like a CFL number, not physics:
/// it changes how fast the relax converges, never what it converges to; the release bound does).
const RELAX_ZETA: f64 = 0.35;

/// Per-iteration shift cap as a fraction of the child's h, so one loud error cannot fling a child.
const RELAX_MAX_STEP_OVER_H: f64 = 0.05;

/// Divergence guard ONLY. The release criterion is [`RELEASE_DENSITY_ERROR`]; hitting this cap is
/// a stated [`Refusal::NotConverged`], never a silent release.
const RELAX_MAX_ITERS: usize = 5000;

/// The committed zoom region: a sphere in the SPH field's own frame.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub center: Vec3,
    pub radius: f32,
}

impl Region {
    fn contains(&self, p: [f32; 3]) -> bool {
        (Vec3::from(p) - self.center).length_squared() <= self.radius * self.radius
    }
}

/// The five conserved quantities of a particle set, accumulated in f64 (so the audit's own error
/// is far below the f32 state it measures). Momentum and angular momentum are about the field
/// frame's origin.
#[derive(Clone, Copy, Debug, Default)]
pub struct Audit {
    pub mass: f64,
    pub momentum: DVec3,
    pub angular_momentum: DVec3,
    pub kinetic: f64,
    pub internal: f64,
}

/// Measure the five conserved quantities of a field.
pub fn audit(particles: &[SphParticle]) -> Audit {
    let mut a = Audit::default();
    for p in particles {
        let m = p.mass as f64;
        let r = DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
        let v = DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
        a.mass += m;
        a.momentum += m * v;
        a.angular_momentum += m * r.cross(v);
        a.kinetic += 0.5 * m * v.length_squared();
        a.internal += m * p.u as f64;
    }
    a
}

/// The conservation ledger the caller renders: the five quantities before, after the split, and
/// after the relax, plus the relax's stated angular-momentum drift bound. Split drift is exact up
/// to f32 rounding; the relax touches positions only, so mass, momentum, kinetic and internal
/// energy are bitwise unchanged across it and angular momentum moves by at most
/// [`Ledger::relax_am_bound`].
#[derive(Clone, Copy, Debug)]
pub struct Ledger {
    pub before: Audit,
    pub after_split: Audit,
    pub after_relax: Audit,
    /// sum over children of m |total shift| |v| (kg m^2/s): the triangle-inequality bound on the
    /// relax's angular-momentum drift. |after_relax.L - after_split.L| <= this, by construction.
    pub relax_am_bound: f64,
}

/// What the relax measured and did. The blip numbers are the honest record of the scheme's entire
/// error; the caller shows them, not hides them.
#[derive(Clone, Copy, Debug)]
pub struct RelaxReport {
    /// Shifting iterations actually run before the density bound released the patch.
    pub iterations: usize,
    /// Max relative child density error right after the split, before any shifting: the raw blip.
    pub initial_max_density_error: f64,
    /// Max relative child density error at release; within [`RELEASE_DENSITY_ERROR`] by contract.
    pub released_max_density_error: f64,
    /// Largest total displacement any child accumulated over the relax (m).
    pub max_shift_m: f32,
}

/// A split without relax: the conserving stencil alone. [`refine_patch`] is the production shape;
/// this is public so the exactness of the split is separately auditable and testable.
#[derive(Clone)]
pub struct Split {
    /// The exterior (untouched, original order) followed by the children.
    pub particles: Vec<SphParticle>,
    /// Children occupy `fine_start..` in `particles`.
    pub fine_start: usize,
    pub before: Audit,
    pub after: Audit,
}

/// The refined patch: split, relaxed, released, audited.
#[derive(Clone)]
pub struct Refined {
    /// The exterior (untouched, original order) followed by the relaxed children.
    pub particles: Vec<SphParticle>,
    /// Children occupy `fine_start..` in `particles`.
    pub fine_start: usize,
    pub ledger: Ledger,
    pub relax: RelaxReport,
}

/// Why the rung said no. Refusal is the discipline doing its job: a hidden smoothing-over at a
/// resolution interface would be a fudge (Law V), so every branch states its reason.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    /// The committed region holds no particles to refine.
    EmptyRegion,
    /// A particle more than one rung coarser than the region's finest sits inside the fine
    /// region. In zoom-in practice this invalidates the run; it is never smoothed over.
    Contaminated { index: usize, mass: f32, finest_mass: f32 },
    /// Matter within interaction reach of the would-be children is more than one rung coarser
    /// than the parents: splitting here would put rung L+1 against rung L-1 across one interface.
    /// Refine the shell first (the buffer-shell discipline).
    RatioExceeded { index: usize, coarse_mass: f32, parent_mass: f32 },
    /// The relax hit its divergence guard before reaching the release bound. The achieved error
    /// is stated; the patch is NOT released.
    NotConverged { achieved: f64, bound: f64, iterations: usize },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::EmptyRegion => write!(f, "refinement refused: the region holds no particles"),
            Refusal::Contaminated { index, mass, finest_mass } => write!(
                f,
                "refinement invalid: coarse particle {index} ({mass:.3e} kg) inside the fine \
                 region (finest mass {finest_mass:.3e} kg, more than one rung apart)"
            ),
            Refusal::RatioExceeded { index, coarse_mass, parent_mass } => write!(
                f,
                "refinement refused: particle {index} ({coarse_mass:.3e} kg) within reach of the \
                 fine region is more than one rung coarser than the parents ({parent_mass:.3e} \
                 kg); refine the shell first"
            ),
            Refusal::NotConverged { achieved, bound, iterations } => write!(
                f,
                "relax not released: density error {achieved:.3e} still above the {bound:.3e} \
                 bound after {iterations} iterations (divergence guard)"
            ),
        }
    }
}

/// The standing contamination check the future wiring runs every step while a fine patch lives:
/// a coarse particle (more than one rung heavier than `fine_mass`) inside the fine region
/// invalidates the run, with the offender named. This is Hahn and Abel's contamination criterion
/// as a refusal, not a warning.
pub fn contamination_check(
    field: &[SphParticle],
    region: &Region,
    fine_mass: f32,
) -> Result<(), Refusal> {
    let edge = fine_mass as f64 * LEVEL_EDGE;
    for (index, p) in field.iter().enumerate() {
        if region.contains(p.pos) && p.mass as f64 > edge {
            return Err(Refusal::Contaminated { index, mass: p.mass, finest_mass: fine_mass });
        }
    }
    Ok(())
}

/// The one fixed icosahedron stencil orientation (unit circumradius). The 12 vertices come in
/// exact +/- pairs, so their sum is exactly zero and the children's barycenter is exactly the
/// parent position, which is what makes momentum and angular momentum conservation constructive.
fn icosahedron_vertices() -> [Vec3; 12] {
    let phi = (1.0 + 5.0f32.sqrt()) / 2.0;
    let n = (1.0 + phi * phi).sqrt();
    let a = 1.0 / n;
    let b = phi / n;
    [
        Vec3::new(0.0, a, b),
        Vec3::new(0.0, a, -b),
        Vec3::new(0.0, -a, b),
        Vec3::new(0.0, -a, -b),
        Vec3::new(a, b, 0.0),
        Vec3::new(a, -b, 0.0),
        Vec3::new(-a, b, 0.0),
        Vec3::new(-a, -b, 0.0),
        Vec3::new(b, 0.0, a),
        Vec3::new(-b, 0.0, a),
        Vec3::new(b, 0.0, -a),
        Vec3::new(-b, 0.0, -a),
    ]
}

/// **The one-shot split.** Parents inside the region become 13 children each on the icosahedron
/// stencil (12 vertices at [`SPLIT_SEPARATION_OVER_H`] times the parent h, plus the retained
/// center child); children inherit the parent's velocity and specific internal energy, child h is
/// [`SPLIT_CHILD_H_OVER_H`] times the parent's, and the center child's mass absorbs the f64
/// division remainder so the 13 masses sum exactly to the parent's. Refuses (never fudges) on an
/// empty region, a contaminated interior, or an interface that would span more than one rung.
pub fn split_patch(field: &[SphParticle], region: &Region) -> Result<Split, Refusal> {
    let mut parents = Vec::new();
    let mut exterior = Vec::new();
    for (i, p) in field.iter().enumerate() {
        if region.contains(p.pos) {
            parents.push(i);
        } else {
            exterior.push(i);
        }
    }
    if parents.is_empty() {
        return Err(Refusal::EmptyRegion);
    }

    // Contamination: the fine-to-be interior must be ONE rung. A coarser particle inside the
    // region is exactly the zoom-in contamination case, refused with the offender named.
    let finest = parents.iter().map(|&i| field[i].mass).fold(f32::INFINITY, f32::min);
    for &i in &parents {
        if field[i].mass as f64 > finest as f64 * LEVEL_EDGE {
            return Err(Refusal::Contaminated {
                index: i,
                mass: field[i].mass,
                finest_mass: finest,
            });
        }
    }

    // Interface ratio: one rung per interface. Children (rung L+1) may interact with the rung the
    // parents came from (L), never with matter a rung coarser still (L-1). The reach is the
    // farthest a child can sit from the region center plus the widest child-coarse interaction
    // radius in the engine's symmetrized-h convention.
    let m_pmax = parents.iter().map(|&i| field[i].mass).fold(0.0f32, f32::max);
    let h_pmax = parents.iter().map(|&i| field[i].h).fold(0.0f32, f32::max);
    for &i in &exterior {
        let p = &field[i];
        let reach = region.radius
            + SPLIT_SEPARATION_OVER_H * h_pmax
            + 0.5 * (p.h + SPLIT_CHILD_H_OVER_H * h_pmax);
        if (Vec3::from(p.pos) - region.center).length() < reach
            && p.mass as f64 > m_pmax as f64 * LEVEL_EDGE
        {
            return Err(Refusal::RatioExceeded {
                index: i,
                coarse_mass: p.mass,
                parent_mass: m_pmax,
            });
        }
    }

    let before = audit(field);
    let mut particles: Vec<SphParticle> = exterior.iter().map(|&i| field[i]).collect();
    let fine_start = particles.len();
    let verts = icosahedron_vertices();
    for &i in &parents {
        let p = field[i];
        // Equal vertex masses; the retained center child absorbs the f64 division remainder so
        // the 13 masses sum exactly to the parent's (up to one f32 rounding).
        let mv = (p.mass as f64 / LEVEL_MASS_RATIO) as f32;
        let mc = (p.mass as f64 - 12.0 * mv as f64) as f32;
        let sep = SPLIT_SEPARATION_OVER_H * p.h;
        let mut child = p;
        child.h = SPLIT_CHILD_H_OVER_H * p.h;
        child.mass = mc;
        particles.push(child); // the mandatory retained center child, at the parent position
        child.mass = mv;
        for u in verts {
            child.pos = (Vec3::from(p.pos) + sep * u).to_array();
            particles.push(child);
        }
    }
    let after = audit(&particles);
    Ok(Split { particles, fine_start, before, after })
}

/// The engine-convention SPH density read at a site: `sum_j m_j W(r, (h + h_j)/2)` over the given
/// contributors (the pairwise-h symmetrization `sph_step.wgsl` uses). Includes the self term when
/// the site's own particle is among the contributors, exactly as the engine's sum would.
fn density_at(pos: Vec3, h: f32, contributors: &[SphParticle]) -> f64 {
    let mut rho = 0.0f64;
    for q in contributors {
        let hij = 0.5 * (h + q.h);
        let d2 = (Vec3::from(q.pos) - pos).length_squared();
        if d2 < hij * hij {
            rho += q.mass as f64 * crate::atmosphere::sph_w((d2 as f64).sqrt(), hij as f64);
        }
    }
    rho
}

/// **The rung: split, relax, release.** The production entry point of the conserved hand-down:
/// one-shot split ([`split_patch`]), then damped position shifting of the children against the
/// SPH-interpolated coarse density with the clock frozen and the coarse exterior held as a fixed
/// guard band, released only when every child's relative density error is within
/// [`RELEASE_DENSITY_ERROR`]. Returns the refined field with the full conservation ledger and the
/// measured blip numbers; returns a stated [`Refusal`] rather than any quiet compromise.
pub fn refine_patch(field: &[SphParticle], region: &Region) -> Result<Refined, Refusal> {
    let split = split_patch(field, region)?;
    let Split { mut particles, fine_start, before, after: after_split } = split;
    let n_children = particles.len() - fine_start;

    // The guard band: coarse exterior close enough to feel the fine region. Held FIXED; it
    // participates in the error field so an over-dense interface pushes back on the children.
    let h_max = field.iter().map(|p| p.h).fold(0.0f32, f32::max);
    let guard_reach = region.radius + 2.0 * h_max;
    let guard: Vec<usize> = (0..fine_start)
        .filter(|&i| (Vec3::from(particles[i].pos) - region.center).length() < guard_reach)
        .collect();
    // Guard targets are what each guard site read in the ORIGINAL field; guards do not move, so
    // these are fixed. (Shared boundary truncation cancels: target and current density at a guard
    // differ only by children-for-parents inside the region.)
    let guard_target: Vec<f64> = guard
        .iter()
        .map(|&i| density_at(Vec3::from(particles[i].pos), particles[i].h, field))
        .collect();

    let mut total_shift = vec![Vec3::ZERO; n_children];
    let mut am_bound = 0.0f64;
    let mut initial_err = 0.0f64;
    let mut last_err = f64::INFINITY;
    for iter in 0..=RELAX_MAX_ITERS {
        // The target is a FIELD: the coarse density interpolated at each child's current site.
        let child_target: Vec<f64> = (fine_start..particles.len())
            .map(|i| density_at(Vec3::from(particles[i].pos), particles[i].h, field))
            .collect();
        let child_err: Vec<f64> = (fine_start..particles.len())
            .zip(&child_target)
            .map(|(i, &t)| {
                let rho = density_at(Vec3::from(particles[i].pos), particles[i].h, &particles);
                (rho - t) / t
            })
            .collect();
        let max_err = child_err.iter().fold(0.0f64, |a, e| a.max(e.abs()));
        if iter == 0 {
            initial_err = max_err;
        }
        last_err = max_err;
        if max_err <= RELEASE_DENSITY_ERROR {
            // RELEASE: the stated density bound is met. Record the measured density on each
            // child (the engine recomputes it every step; this is the honest value at release).
            for (k, i) in (fine_start..particles.len()).enumerate() {
                let t = child_target[k] * (1.0 + child_err[k]);
                particles[i].rho = t as f32;
            }
            let after_relax = audit(&particles);
            return Ok(Refined {
                particles,
                fine_start,
                ledger: Ledger { before, after_split, after_relax, relax_am_bound: am_bound },
                relax: RelaxReport {
                    iterations: iter,
                    initial_max_density_error: initial_err,
                    released_max_density_error: max_err,
                    max_shift_m: total_shift.iter().map(|s| s.length()).fold(0.0f32, f32::max),
                },
            });
        }
        if iter == RELAX_MAX_ITERS {
            break;
        }

        // Guard errors this iteration (guards are fixed but their density feels the children).
        let guard_err: Vec<f64> = guard
            .iter()
            .zip(&guard_target)
            .map(|(&i, &t)| {
                let rho = density_at(Vec3::from(particles[i].pos), particles[i].h, &particles);
                (rho - t) / t
            })
            .collect();

        // Damped shifting down the density-error gradient (positions ONLY: the clock is frozen).
        let mut shifts = vec![Vec3::ZERO; n_children];
        for (k, i) in (fine_start..particles.len()).enumerate() {
            let pi = particles[i];
            let xi = Vec3::from(pi.pos);
            let ei = child_err[k];
            let mut acc = DVec3::ZERO;
            let mut pair = |pj: &SphParticle, tj: f64, ej: f64| {
                let hij = 0.5 * (pi.h + pj.h);
                let d = xi - Vec3::from(pj.pos);
                let d2 = d.length_squared();
                if d2 > 0.0 && d2 < hij * hij {
                    let r = (d2 as f64).sqrt();
                    let dw = crate::atmosphere::sph_dw(r, hij as f64);
                    let dir = DVec3::new(d.x as f64, d.y as f64, d.z as f64) / r;
                    acc += (pj.mass as f64 / tj) * (ei + ej) * dw * dir;
                }
            };
            for (k2, j) in (fine_start..particles.len()).enumerate() {
                if j != i {
                    pair(&particles[j], child_target[k2], child_err[k2]);
                }
            }
            for (g, &j) in guard.iter().enumerate() {
                pair(&particles[j], guard_target[g], guard_err[g]);
            }
            let h64 = pi.h as f64;
            let dx64 = -RELAX_ZETA * h64 * h64 * acc;
            let mut dx = Vec3::new(dx64.x as f32, dx64.y as f32, dx64.z as f32);
            let cap = (RELAX_MAX_STEP_OVER_H * h64) as f32;
            if dx.length() > cap {
                dx = dx.normalize() * cap;
            }
            shifts[k] = dx;
        }
        for (k, i) in (fine_start..particles.len()).enumerate() {
            let dx = shifts[k];
            particles[i].pos = (Vec3::from(particles[i].pos) + dx).to_array();
            total_shift[k] += dx;
            am_bound += particles[i].mass as f64
                * dx.length() as f64
                * Vec3::from(particles[i].vel).length() as f64;
        }
    }
    Err(Refusal::NotConverged {
        achieved: last_err,
        bound: RELEASE_DENSITY_ERROR,
        iterations: RELAX_MAX_ITERS,
    })
}

// ---------------------------------------------------------------------------------------------
// The pi-scaling gate (docs/59: crater scaling, not eyeballing) for the future end-to-end test.
// ---------------------------------------------------------------------------------------------

/// One material row of the Holsapple-Housen crater-scaling table (gravity regime), vintage named
/// in the constant. K1 is the cratering-efficiency coefficient, mu the coupling exponent, nu the
/// density exponent.
#[derive(Clone, Copy, Debug)]
pub struct ScalingRow {
    pub name: &'static str,
    pub k1: f64,
    pub mu: f64,
    pub nu: f64,
}

/// Hard rock row, Holsapple-Housen v2.2.1 table (the vintage docs/59 records).
pub const HARD_ROCK: ScalingRow =
    ScalingRow { name: "hard rock, Holsapple-Housen v2.2.1", k1: 0.012, mu: 0.55, nu: 0.4 };

/// Regolith row, Holsapple-Housen v2.2.1 table (the vintage docs/59 records).
pub const REGOLITH: ScalingRow =
    ScalingRow { name: "regolith, Holsapple-Housen v2.2.1", k1: 0.14, mu: 0.4, nu: 0.4 };

/// Apparent crater radius from crater volume, r = K_r V^(1/3), same table vintage.
pub const KR_APPARENT: f64 = 1.1;

/// Rim radius over apparent radius, same table vintage.
pub const RIM_OVER_APPARENT: f64 = 1.3;

/// The impact the gate sizes: a spherical impactor into a half-space target under gravity.
#[derive(Clone, Copy, Debug)]
pub struct ImpactSpec {
    pub impactor_radius_m: f64,
    pub impactor_density: f64,
    pub speed_ms: f64,
    pub target_density: f64,
    pub gravity: f64,
}

/// The gravity-regime crater volume from pi-group scaling:
/// `piV = K1 * (pi2 * (rho/delta)^((6 nu - 2 - mu)/(3 mu)))^(-3 mu / (2 + mu))`, with
/// `pi2 = g a / U^2` and `piV = rho V / m` (Holsapple 1993, gravity regime).
pub fn crater_volume_gravity_m3(spec: &ImpactSpec, row: &ScalingRow) -> f64 {
    let a = spec.impactor_radius_m;
    let m = 4.0 / 3.0 * std::f64::consts::PI * a * a * a * spec.impactor_density;
    let pi2 = spec.gravity * a / (spec.speed_ms * spec.speed_ms);
    let dens = (spec.target_density / spec.impactor_density)
        .powf((6.0 * row.nu - 2.0 - row.mu) / (3.0 * row.mu));
    let piv = row.k1 * (pi2 * dens).powf(-3.0 * row.mu / (2.0 + row.mu));
    piv * m / spec.target_density
}

/// Predicted rim radius in the gravity regime: `KR_APPARENT * V^(1/3) * RIM_OVER_APPARENT`.
pub fn rim_radius_gravity_m(spec: &ImpactSpec, row: &ScalingRow) -> f64 {
    RIM_OVER_APPARENT * KR_APPARENT * crater_volume_gravity_m3(spec, row).cbrt()
}

/// The gate's answer. `SanityPass`/`SanityFail` are the explicit degraded mode: when the
/// predicted crater rivals the body radius, pi-scaling's point-source-on-a-half-space assumption
/// fails and only an order-of-magnitude bound is honest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GateVerdict {
    /// Within a factor of two of the scaling prediction.
    Pass { ratio: f64 },
    /// Outside the factor of two.
    Fail { ratio: f64, allowed: f64 },
    /// Crater rivals the body: within the degraded order-of-magnitude bound.
    SanityPass { ratio: f64 },
    /// Crater rivals the body AND outside even the order-of-magnitude bound.
    SanityFail { ratio: f64, allowed: f64 },
}

/// Compare a measured rim radius against the pi-scaling prediction. Factor-of-two passes; when
/// the predicted rim radius exceeds half the body radius the check degrades, explicitly, to an
/// order-of-magnitude sanity bound (docs/59: pi-scaling assumes a point source).
pub fn pi_scaling_gate(
    measured_rim_radius_m: f64,
    predicted_rim_radius_m: f64,
    body_radius_m: f64,
) -> GateVerdict {
    let ratio = if measured_rim_radius_m >= predicted_rim_radius_m {
        measured_rim_radius_m / predicted_rim_radius_m
    } else {
        predicted_rim_radius_m / measured_rim_radius_m
    };
    // "Approaches the body's own radius": a predicted rim radius past half the body radius, the
    // declared threshold at which the half-space assumption is visibly gone.
    if predicted_rim_radius_m > 0.5 * body_radius_m {
        if ratio <= 10.0 {
            GateVerdict::SanityPass { ratio }
        } else {
            GateVerdict::SanityFail { ratio, allowed: 10.0 }
        }
    } else if ratio <= 2.0 {
        GateVerdict::Pass { ratio }
    } else {
        GateVerdict::Fail { ratio, allowed: 2.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASALT_RHO: f32 = 2700.0;
    const IRON_RHO: f32 = 7860.0;

    /// A uniform cubic lattice of SPH particles centered on the origin: spacing `dx`, smoothing
    /// h = 2 dx (roughly 33 neighbors inside this kernel's support), mass = rho dx^3 per site so
    /// the summed density reads the material density. Material chosen per site by `mat_of`.
    fn lattice(n: i32, dx: f32, mat_of: impl Fn(Vec3) -> (u32, f32)) -> Vec<SphParticle> {
        let mut out = Vec::new();
        let half = n as f32 / 2.0;
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let pos = Vec3::new(
                        (i as f32 + 0.5 - half) * dx,
                        (j as f32 + 0.5 - half) * dx,
                        (k as f32 + 0.5 - half) * dx,
                    );
                    let (mat, rho) = mat_of(pos);
                    out.push(SphParticle {
                        pos: pos.to_array(),
                        h: 2.0 * dx,
                        vel: [0.0; 3],
                        u: 1.0e5,
                        mass: rho * dx * dx * dx,
                        mat,
                        rho,
                        prov: 0,
                    });
                }
            }
        }
        out
    }

    fn basalt_lattice(n: i32, dx: f32) -> Vec<SphParticle> {
        lattice(n, dx, |_| (crate::gpu_sph::MAT_BASALT, BASALT_RHO))
    }

    /// Give a field a bulk velocity, a rigid rotation and a position-dependent specific internal
    /// energy, so all five audited quantities are nontrivial.
    fn stir(field: &mut [SphParticle]) {
        let omega = Vec3::new(0.02, -0.05, 0.03);
        let bulk = Vec3::new(3.0, -1.0, 2.0);
        for p in field.iter_mut() {
            let r = Vec3::from(p.pos);
            let v = bulk + omega.cross(r);
            p.vel = v.to_array();
            p.u = 1.0e5 * (1.0 + 0.1 * (r.x + 2.0 * r.y - r.z));
        }
    }

    /// **The split conserves all five quantities exactly (up to f32 rounding).** The stencil's
    /// vertex directions sum to zero and the center child absorbs the mass remainder, so nothing
    /// is left to a solver's mercy: this is conservation by construction, audited.
    #[test]
    fn the_split_conserves_all_five_quantities_exactly() {
        let mut field = basalt_lattice(8, 1.0);
        stir(&mut field);
        let region = Region { center: Vec3::ZERO, radius: 2.2 };
        let n_parents =
            field.iter().filter(|p| region.contains(p.pos)).count();
        assert!(n_parents > 0, "the test region must actually hold parents");

        let split = split_patch(&field, &region).expect("a clean region must split");

        // 13 children per parent, appended after the untouched exterior.
        assert_eq!(split.particles.len() - split.fine_start, n_parents * SPLIT_CHILDREN);
        assert_eq!(split.fine_start, field.len() - n_parents);

        // Reconstruct every parent's stencil exactly: the retained center child at the parent
        // position and the 12 vertex children at the derived separation, each inheriting the
        // parent's velocity and specific internal energy, h contracted by the derived ratio, and
        // the 13 masses summing exactly to the parent's (center child holds the remainder).
        let children = &split.particles[split.fine_start..];
        let verts = icosahedron_vertices();
        for p in field.iter().filter(|p| region.contains(p.pos)) {
            let mv = (p.mass as f64 / LEVEL_MASS_RATIO) as f32;
            let mc = (p.mass as f64 - 12.0 * mv as f64) as f32;
            let mut expected = vec![(p.pos, mc)];
            for u in verts {
                let pos = (Vec3::from(p.pos) + SPLIT_SEPARATION_OVER_H * p.h * u).to_array();
                expected.push((pos, mv));
            }
            for (pos, mass) in expected {
                let c = children
                    .iter()
                    .find(|c| c.pos == pos)
                    .expect("every stencil site must hold a child (center child retained)");
                assert_eq!(c.mass, mass, "child masses sum exactly to the parent's");
                assert_eq!(c.vel, p.vel, "children inherit the parent velocity");
                assert_eq!(c.u, p.u, "children inherit the specific internal energy");
                assert_eq!(c.mat, p.mat, "material identity survives the split");
                assert!((c.h - SPLIT_CHILD_H_OVER_H * p.h).abs() < 1.0e-6 * p.h);
            }
            assert!(
                (mc as f64 + 12.0 * mv as f64 - p.mass as f64).abs() <= p.mass as f64 * 1.0e-6,
                "the 13 child masses must reproduce the parent mass to f32 rounding"
            );
        }

        // The five conserved quantities, before vs after, exact up to f32 rounding.
        let (b, a) = (split.before, split.after);
        assert!((a.mass - b.mass).abs() <= b.mass * 1.0e-6, "mass: {} vs {}", b.mass, a.mass);
        assert!(
            (a.momentum - b.momentum).length() <= b.momentum.length() * 1.0e-6,
            "momentum: {:?} vs {:?}",
            b.momentum,
            a.momentum
        );
        assert!(
            (a.angular_momentum - b.angular_momentum).length()
                <= b.angular_momentum.length() * 1.0e-5,
            "angular momentum: {:?} vs {:?}",
            b.angular_momentum,
            a.angular_momentum
        );
        assert!((a.kinetic - b.kinetic).abs() <= b.kinetic * 1.0e-6);
        assert!((a.internal - b.internal).abs() <= b.internal * 1.0e-6);
    }

    /// **Relax bounds the density blip on a uniform field, and the ledger holds.** The release is
    /// the stated density-error bound; the relax touches positions only, so four of the five
    /// quantities are bitwise conserved across it and the angular-momentum drift obeys the
    /// ledger's own stated bound.
    #[test]
    fn relax_releases_within_the_density_bound_on_a_uniform_field() {
        let mut field = basalt_lattice(12, 1.0);
        stir(&mut field);
        let region = Region { center: Vec3::ZERO, radius: 2.2 };

        let refined = refine_patch(&field, &region).expect("a uniform patch must refine");

        println!(
            "uniform blip: initial {:.4e}, released {:.4e} after {} iterations, max shift {:.3} m",
            refined.relax.initial_max_density_error,
            refined.relax.released_max_density_error,
            refined.relax.iterations,
            refined.relax.max_shift_m,
        );
        assert!(
            refined.relax.released_max_density_error <= RELEASE_DENSITY_ERROR,
            "released above the stated bound: {:.4e}",
            refined.relax.released_max_density_error
        );
        assert!(
            refined.relax.initial_max_density_error > refined.relax.released_max_density_error,
            "the relax must actually reduce the blip, else it is decoration"
        );

        // The ledger across split + relax: mass, momentum, kinetic, internal conserved to f32
        // rounding end to end; angular momentum within the relax's own stated bound.
        let l = refined.ledger;
        assert!((l.after_relax.mass - l.before.mass).abs() <= l.before.mass * 1.0e-6);
        assert!(
            (l.after_relax.momentum - l.before.momentum).length()
                <= l.before.momentum.length() * 1.0e-6
        );
        assert!((l.after_relax.kinetic - l.before.kinetic).abs() <= l.before.kinetic * 1.0e-6);
        assert!((l.after_relax.internal - l.before.internal).abs() <= l.before.internal * 1.0e-6);
        let am_drift = (l.after_relax.angular_momentum - l.after_split.angular_momentum).length();
        assert!(
            am_drift <= l.relax_am_bound * (1.0 + 1.0e-6) + 1.0e-9,
            "relax AM drift {am_drift:.3e} exceeds its stated bound {:.3e}",
            l.relax_am_bound
        );
        println!(
            "uniform AM: split drift {:.3e}, relax drift {:.3e} within stated bound {:.3e} (|L| {:.3e})",
            (l.after_split.angular_momentum - l.before.angular_momentum).length(),
            am_drift,
            l.relax_am_bound,
            l.before.angular_momentum.length(),
        );
    }

    /// **The blip stays bounded across a basalt/iron interface.** Two Tillotson materials, a
    /// density jump of ~3x through the region: the release bound holds there too, because the
    /// target is the coarse field's own interpolated density, jump included.
    #[test]
    fn relax_releases_within_the_density_bound_across_a_basalt_iron_interface() {
        let mut field = lattice(12, 1.0, |pos| {
            if pos.x < 0.0 {
                (crate::gpu_sph::MAT_IRON, IRON_RHO)
            } else {
                (crate::gpu_sph::MAT_BASALT, BASALT_RHO)
            }
        });
        stir(&mut field);
        // Region straddles the material interface plane x = 0.
        let region = Region { center: Vec3::ZERO, radius: 2.2 };

        let refined = refine_patch(&field, &region).expect("a two-material patch must refine");

        println!(
            "interface blip: initial {:.4e}, released {:.4e} after {} iterations, max shift {:.3} m",
            refined.relax.initial_max_density_error,
            refined.relax.released_max_density_error,
            refined.relax.iterations,
            refined.relax.max_shift_m,
        );
        assert!(refined.relax.released_max_density_error <= RELEASE_DENSITY_ERROR);

        // Material identity survives the split: iron children from iron parents, and both
        // materials are present among the children.
        let children = &refined.particles[refined.fine_start..];
        assert!(children.iter().any(|c| c.mat == crate::gpu_sph::MAT_IRON));
        assert!(children.iter().any(|c| c.mat == crate::gpu_sph::MAT_BASALT));
        for c in children {
            let expected =
                if c.pos[0] < 0.0 { crate::gpu_sph::MAT_IRON } else { crate::gpu_sph::MAT_BASALT };
            // Children near the plane may drift a hair across it during relax; identity is
            // inherited from the parent, so check against the parent side with a slack of one
            // separation radius.
            if c.pos[0].abs() > SPLIT_SEPARATION_OVER_H * 2.0 {
                assert_eq!(c.mat, expected, "material identity must survive the rung");
            }
        }

        let l = refined.ledger;
        assert!((l.after_relax.mass - l.before.mass).abs() <= l.before.mass * 1.0e-6);
        assert!((l.after_relax.internal - l.before.internal).abs() <= l.before.internal * 1.0e-6);
    }

    /// **Contamination refuses, with the offender named.** A coarse particle inside the fine
    /// region invalidates the refinement; nothing is smoothed over.
    #[test]
    fn a_coarse_particle_inside_the_fine_region_is_refused() {
        let mut field = basalt_lattice(8, 1.0);
        // One rogue coarse particle (13x mass, one rung up) sits inside the region.
        field.push(SphParticle {
            pos: [0.3, 0.2, -0.1],
            h: 2.0,
            vel: [0.0; 3],
            u: 1.0e5,
            mass: 13.0 * BASALT_RHO,
            mat: crate::gpu_sph::MAT_BASALT,
            rho: BASALT_RHO,
            prov: 0,
        });
        let region = Region { center: Vec3::ZERO, radius: 2.2 };

        match refine_patch(&field, &region).map(|_| ()) {
            Err(Refusal::Contaminated { index, mass, finest_mass }) => {
                assert_eq!(index, field.len() - 1, "the refusal must name the offender");
                assert!(mass > finest_mass * LEVEL_EDGE as f32);
            }
            other => panic!("contamination must refuse, got {other:?}"),
        }

        // The standing per-step check sees the same offender and states the same reason.
        let fine_mass = BASALT_RHO; // one lattice particle's mass at dx = 1
        match contamination_check(&field, &region, fine_mass) {
            Err(r @ Refusal::Contaminated { .. }) => {
                let msg = format!("{r}");
                assert!(msg.contains("invalid"), "the reason must be stated: {msg}");
            }
            other => panic!("the standing check must refuse too, got {other:?}"),
        }
        // And a clean field passes it.
        let clean = basalt_lattice(8, 1.0);
        assert!(contamination_check(&clean, &region, fine_mass).is_ok());
    }

    /// **One rung per interface, enforced by refusal.** Splitting a sub-region that still touches
    /// unrefined coarse matter would put rung 2 against rung 0 across one interface; the rung
    /// refuses and says to refine the shell first. Deep inside the fine patch, with a buffer of
    /// fine matter all around, the second rung is legal.
    #[test]
    fn a_second_rung_against_unbuffered_coarse_is_refused() {
        let field = basalt_lattice(14, 1.0);
        let r1 = Region { center: Vec3::ZERO, radius: 4.0 };
        let once = split_patch(&field, &r1).expect("the first rung is clean");

        // Hugging the rim of the fine patch: level-0 coarse is within reach. Refused.
        let rim = Region { center: Vec3::new(2.8, 0.0, 0.0), radius: 1.0 };
        match split_patch(&once.particles, &rim).map(|_| ()) {
            Err(Refusal::RatioExceeded { coarse_mass, parent_mass, .. }) => {
                assert!(coarse_mass > parent_mass * LEVEL_EDGE as f32);
            }
            other => panic!("an unbuffered second rung must be refused, got {other:?}"),
        }

        // Deep interior, buffered by fine matter on all sides: legal.
        let deep = Region { center: Vec3::ZERO, radius: 1.0 };
        split_patch(&once.particles, &deep)
            .expect("a buffered second rung is one level per interface and must be allowed");

        // A region straddling the fine patch's rim holds BOTH rungs inside it: that is a coarse
        // particle inside a fine region, contamination.
        let straddle = Region { center: Vec3::new(4.0, 0.0, 0.0), radius: 1.5 };
        match split_patch(&once.particles, &straddle).map(|_| ()) {
            Err(Refusal::Contaminated { .. }) => {}
            other => panic!("a mixed-rung interior must be contamination, got {other:?}"),
        }

        // And an empty region is its own stated refusal, not a silent no-op.
        let empty = Region { center: Vec3::new(500.0, 0.0, 0.0), radius: 1.0 };
        match split_patch(&field, &empty).map(|_| ()) {
            Err(Refusal::EmptyRegion) => {}
            other => panic!("an empty region must be refused as such, got {other:?}"),
        }
    }

    /// **The pi-scaling gate reproduces a hand-computed literature example.** Meteor Crater by
    /// the regolith row (Holsapple-Housen v2.2.1 vintage): a 25 m iron impactor at 12 km/s into
    /// 2100 kg/m^3 target under Earth gravity. By hand:
    ///   pi2   = g a / U^2 = 9.81 * 25 / (12000^2)            = 1.70312e-6
    ///   piV   = 0.14 * pi2^(-0.5)                            = 107.277
    ///           (with mu = nu = 0.4 the density-ratio exponent (6 nu - 2 - mu)/(3 mu) is exactly
    ///            zero, so the density ratio drops out)
    ///   m     = (4/3) pi a^3 delta = 5.14436e8 kg
    ///   V     = piV m / rho = 2.62795e7 m^3
    ///   r_app = 1.1 V^(1/3) = 327.0 m,  r_rim = 1.3 r_app = 425.1 m
    /// The observed rim radius is ~593 m (1186 m rim diameter): a ratio of 1.39, inside the
    /// factor-of-two gate.
    #[test]
    fn the_pi_scaling_gate_reproduces_a_hand_computed_meteor_crater() {
        let spec = ImpactSpec {
            impactor_radius_m: 25.0,
            impactor_density: 7860.0,
            speed_ms: 12_000.0,
            target_density: 2100.0,
            gravity: 9.81,
        };
        let v = crater_volume_gravity_m3(&spec, &REGOLITH);
        assert!(
            (v - 2.62795e7).abs() <= 2.62795e7 * 1.0e-3,
            "hand-computed crater volume: expected 2.628e7 m^3, got {v:.4e}"
        );
        let rim = rim_radius_gravity_m(&spec, &REGOLITH);
        assert!(
            (rim - 425.1).abs() <= 1.0,
            "hand-computed rim radius: expected 425.1 m, got {rim:.1}"
        );
        match pi_scaling_gate(593.0, rim, 6.371e6) {
            GateVerdict::Pass { ratio } => assert!((ratio - 593.0 / rim).abs() < 1.0e-9),
            other => panic!("Meteor Crater must pass the factor-of-two gate, got {other:?}"),
        }
        // Three times the prediction is outside the factor of two: the gate fails it.
        match pi_scaling_gate(3.0 * rim, rim, 6.371e6) {
            GateVerdict::Fail { allowed, .. } => assert_eq!(allowed, 2.0),
            other => panic!("3x must fail the gate, got {other:?}"),
        }
    }

    /// **The gate degrades, explicitly, when the crater rivals the body.** pi-scaling assumes a
    /// point source on a half-space; a rim radius beyond half the body radius only supports an
    /// order-of-magnitude bound, and the verdict SAYS it is the degraded check.
    #[test]
    fn the_gate_degrades_to_order_of_magnitude_when_the_crater_rivals_the_body() {
        let body_r = 1.0e6;
        let predicted = 0.6e6; // rim radius rivaling the body
        match pi_scaling_gate(3.0 * predicted, predicted, body_r) {
            GateVerdict::SanityPass { ratio } => assert!((ratio - 3.0).abs() < 1.0e-9),
            other => panic!("a body-scale crater within 10x must SanityPass, got {other:?}"),
        }
        match pi_scaling_gate(20.0 * predicted, predicted, body_r) {
            GateVerdict::SanityFail { allowed, .. } => assert_eq!(allowed, 10.0),
            other => panic!("20x is outside even the sanity bound, got {other:?}"),
        }
        // The same 3x ratio on a planet-sized body is a plain Fail: no quiet degradation.
        match pi_scaling_gate(3.0 * predicted, predicted, 6.371e6) {
            GateVerdict::Fail { .. } => {}
            other => panic!("3x on a planet must be a plain Fail, got {other:?}"),
        }
    }
}
