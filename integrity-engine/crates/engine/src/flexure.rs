//! **A slender body bends** — the ELASTIC branch of `docs/18`'s one deformation process (docs/46
//! row 64).
//!
//! Robin, 2026-08-17, on being shown blade-bending work: *"see if there are some underlying rules we
//! can build/optimize for more than just grass; Trees bend in a similar fashion, as do poles under
//! stress, as do nails when they are bent…"* and then *"we must look for the general applicable case
//! if we're going to have a hope of modeling everything."*
//!
//! ## This is not a new law. It is the branch of an existing one that was never built.
//!
//! `docs/18-unified-deformation-and-damage.md` — *"one operator for a bullet, a pebble in a pond, and
//! a Moon"* — already states the process: local stress is compared to the material's yield/fracture
//! strength, matter fails where it is exceeded, momentum is conserved and the energy accounted. It
//! targets MPM: *"one solver, per-material constitutive models, unifying elastic, plastic, granular,
//! and fluid response."*
//!
//! Cratering, crushing, fragmenting and granular flow are **built**. **Nothing in this engine lets a
//! body store elastic strain and give it back.** docs/18's own table calls a solid *"elastic below
//! strength (bounce/embed)"*, and that "bounce" is contact restitution — a property of a COLLISION,
//! not of a body deforming.
//!
//! So the ladder is: **stress → response**, and GEOMETRY is what turns a load into a stress.
//!
//! | stress vs strength | response | governed by |
//! |---|---|---|
//! | below yield | **elastic**, recovers | `youngs_modulus` |
//! | above yield | **plastic**, stays bent | `ductility` |
//! | above rupture | **fracture**, separates | `modulus_of_rupture` |
//!
//! Grass in wind and a tree swaying are the first row; a pole under load crosses into the second; a
//! **bent nail** is the second; a dry twig, a burnt blade and an oak hull splintering under cannon
//! fire are the third. One code path, different material data and different geometry. This module
//! builds the first row and the bridge to the other two ([`bending_stress_pa`]); the rungs above are
//! `docs/46` row 64's open items.
//!
//! ## Why an ELASTICA and not Euler–Bernoulli
//!
//! Small-deflection beam theory assumes the slope is small enough that `curvature ≈ d²y/dx²`. A grass
//! blade violates that by a factor of several: with the measured blade modulus its self-weight tip
//! droop is more than its own length. Worse, `L_crit = (7.837·EI/q)^(1/3)` = 208 mm for the measured
//! section, and real *Lolium* blades are 258–316 mm — **post-critical**. A real blade cannot stand as
//! a straight column; it arches, which is what a sward looks like. So the geometry must be exact in
//! the angle, not linearised: `dθ/ds = M(s)/EI`, with positions following from the angles.
//!
//! ## ★★★ DECLARED SPECIALISATION (Law II, and the trap this module is most likely to fall into)
//!
//! docs/18 targets a general continuum solver. A beam chain answers the SAME question — *how does
//! this matter deform* — by a different route, and "one question, two answers" is exactly the
//! violation `docs/46` exists to catch. This is admissible only as the **slender-body limit** of the
//! continuum, and only with a test that it converges to a known-exact answer.
//!
//! MPM does not exist yet, so it cannot be the reference. The honest interim references are both
//! analytic and both dial-free:
//!
//! - the **linear cantilever** `δ = qL⁴/(8EI)`, which this must reproduce as the load → 0, and
//! - **Greenhill's** critical length for a self-weighted column, constant `7.8373`, which it must
//!   reproduce as the length at which an upright beam stops being able to stand.
//!
//! When MPM lands, the convergence owed is against IT, and this comment is the IOU.
//!
//! ## ★★ NOT BUILT, and it is Law III that asks for it
//!
//! A chain per blade is far more resolution than a meadow needs. Wind-bending is a deterministic
//! function of `(type, position, wind field, time)` — it stores nothing, so a pristine blade need not
//! exist and the scalability law holds. But a de-resolved form (a patch-scale response DERIVED from
//! this one, convergent to it) is owed before anything draws 10¹² blades, and fire spread through an
//! unwatched meadow needs exactly that.

use crate::materials::Material;

/// The solved shape of a bent slender body, in its own bending plane.
#[derive(Clone, Debug)]
pub struct Bent {
    /// Tangent angle at each station, radians, measured from +x. `theta[0]` is the clamped base.
    pub theta: Vec<f64>,
    /// Station positions `(x, y)`, metres, from the clamped base at the origin.
    pub xy: Vec<(f64, f64)>,
    /// Internal bending moment at each station, N·m. Zero at the free tip by construction.
    pub moment_nm: Vec<f64>,
    /// How many relaxation sweeps it took, and whether it actually converged.
    pub sweeps: usize,
    pub converged: bool,
}

impl Bent {
    /// Where the tip ended up, metres.
    pub fn tip(&self) -> (f64, f64) {
        *self.xy.last().unwrap_or(&(0.0, 0.0))
    }
    /// The largest bending moment anywhere along it, N·m — always at or near the clamped base for a
    /// cantilever, and the station that decides whether it yields or breaks.
    pub fn peak_moment_nm(&self) -> f64 {
        self.moment_nm.iter().fold(0.0f64, |a, m| a.max(m.abs()))
    }
}

/// ★★★ **SOLVE THE ELASTICA** — a slender body clamped at one end, free at the other, under a
/// distributed load.
///
/// `ei_nm2` is flexural rigidity (`Assembly::section().i_v_m4 × E`), `length_m` the arclength,
/// `base_angle_rad` the clamped tangent direction measured from +x, and `load_per_m(s)` the applied
/// force per unit length at arclength `s`, as `(fx, fy)` in N/m. Gravity on a uniform body is simply
/// `|_| (0.0, -rho*A*g)`; wind adds a term across the span.
///
/// **The method is fixed-point relaxation**, because the problem is circular: moments depend on where
/// the body is, and where it is depends on the moments. Each sweep computes the moment at every
/// station from the loads beyond it, re-integrates `dθ/ds = M/EI` from the clamped base, and mixes
/// the result in under-relaxed. That circularity is the large-deflection physics — remove it and you
/// have the linear theory back.
///
/// It reports `converged` rather than asserting it. A body that will not settle is a result (it is
/// buckling, or the load is beyond what this section can carry), and silently returning the last
/// iterate as though it were a solution is how a solver lies.
pub fn solve(
    ei_nm2: f64,
    length_m: f64,
    base_angle_rad: f64,
    n: usize,
    load_per_m: &dyn Fn(f64) -> (f64, f64),
) -> Bent {
    let n = n.max(2);
    let ds = length_m / n as f64;
    let ei = ei_nm2.max(f64::MIN_POSITIVE);
    let mut theta = vec![base_angle_rad; n + 1];
    let mut xy = vec![(0.0, 0.0); n + 1];
    let mut moment = vec![0.0; n + 1];

    // Under-relaxation: the fixed point is contractive for modest loads and not for stiff ones, so a
    // half step trades sweeps for the ability to reach a post-critical shape at all.
    const MIX: f64 = 0.5;
    const TOL: f64 = 1.0e-12;
    let max_sweeps = 20_000;
    let mut converged = false;
    let mut sweeps = 0;

    for sweep in 1..=max_sweeps {
        sweeps = sweep;
        // Positions from the current angles.
        let mut x = 0.0;
        let mut y = 0.0;
        xy[0] = (0.0, 0.0);
        for i in 0..n {
            x += theta[i].cos() * ds;
            y += theta[i].sin() * ds;
            xy[i + 1] = (x, y);
        }
        // Moment at each station: the loads BEYOND it, about it. Accumulated from the free tip
        // backwards so the tip is exactly zero and nothing has to be subtracted.
        let (mut fx, mut fy, mut mx, mut my) = (0.0, 0.0, 0.0, 0.0);
        moment[n] = 0.0;
        for i in (0..n).rev() {
            // The segment between station i and i+1, loaded at its midpoint.
            let s_mid = (i as f64 + 0.5) * ds;
            let (lx, ly) = load_per_m(s_mid);
            let (px, py) = (0.5 * (xy[i].0 + xy[i + 1].0), 0.5 * (xy[i].1 + xy[i + 1].1));
            fx += lx * ds;
            fy += ly * ds;
            // ★ `M_z = Σ (x·f_y − y·f_x)`, so each coordinate pairs with the OTHER component's force.
            // Pairing like with like (`y·f_y`) is a plausible-looking transcription that is not a
            // cross product at all, and it cost exactly a factor of 2/3 against the analytic
            // cantilever — invisible except to a closed-form reference, which is why there is one.
            mx += lx * ds * py;
            my += ly * ds * px;
            // M(i) = Σ (r_j − r_i) × f_j = (Σ r_j×f_j) − r_i × (Σ f_j)
            moment[i] = (my - fy * xy[i].0) - (mx - fx * xy[i].1);
        }
        // Re-integrate the angles from the clamped base.
        let mut next = vec![base_angle_rad; n + 1];
        for i in 0..n {
            next[i + 1] = next[i] + moment[i] / ei * ds;
        }
        let mut worst = 0.0f64;
        for i in 0..=n {
            let mixed = theta[i] + MIX * (next[i] - theta[i]);
            worst = worst.max((mixed - theta[i]).abs());
            theta[i] = mixed;
        }
        if worst < TOL {
            converged = true;
            break;
        }
    }
    Bent {
        theta,
        xy,
        moment_nm: moment,
        sweeps,
        converged,
    }
}

/// **The bending stress a moment puts in the outermost fibre**, Pa — `σ = M·c/I`, where `c` is the
/// distance from the neutral axis to the surface.
///
/// This is the BRIDGE from geometry to `docs/18`'s ladder: the same stress that a crater compares
/// against a yield strength and a haystack compares against a crush strength. A slender body under
/// transverse load is just one more geometry feeding the one response.
pub fn bending_stress_pa(moment_nm: f64, extreme_fibre_m: f64, i_m4: f64) -> f64 {
    if i_m4 <= 0.0 {
        return 0.0;
    }
    moment_nm.abs() * extreme_fibre_m.abs() / i_m4
}

/// **Greenhill's critical length** for a self-weighted upright column, metres: the length beyond
/// which a beam of rigidity `ei_nm2` carrying `weight_per_m` cannot stand straight and must arch.
///
/// `L_crit = (7.8373·EI/q)^(1/3)`. The constant is the first root of the governing Bessel problem,
/// not a fitted number, which is what makes this a dial-free check on [`solve`]: an upright beam
/// shorter than this stays up, and one longer than it falls over, with nothing to tune either way.
pub fn greenhill_critical_length_m(ei_nm2: f64, weight_per_m: f64) -> f64 {
    if weight_per_m <= 0.0 {
        return f64::INFINITY;
    }
    (7.8373 * ei_nm2 / weight_per_m).cbrt()
}

/// What a material's catalogue says about where the elastic branch ENDS, Pa. `None` where the
/// catalogue is silent — which for `modulus_of_rupture` is 35 of 37 materials, so most things cannot
/// yet be broken by bending and must say so rather than guess.
pub fn rupture_stress_pa(m: &Material) -> Option<f64> {
    m.modulus_of_rupture.filter(|v| *v > 0.0).map(|v| v as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ **IT REDUCES TO THE ANALYTIC CANTILEVER AS THE LOAD VANISHES.**
    ///
    /// This is the test that makes the beam a DECLARED SPECIALISATION rather than a second answer to
    /// "how does matter deform" (Law II). Small-deflection theory gives `δ = qL⁴/(8EI)` exactly, and
    /// it is exact only in the limit of small slope — so the elastica must approach it as the load
    /// goes to zero, and must DEPART from it as the load grows, in the direction of a shorter
    /// horizontal reach because the beam curls rather than stretching.
    ///
    /// Nothing here is tuned: the reference is a closed form and the limit is taken, not asserted.
    #[test]
    fn the_elastica_converges_to_the_linear_cantilever_as_the_load_vanishes() {
        let (ei, l) = (3.78e-5, 0.35);
        println!("   qL⁴/8EI      elastica |Δy|     ratio");
        let mut prev = f64::INFINITY;
        for scale in [1.0e-4, 1.0e-3, 1.0e-2, 1.0e-1] {
            let q = 0.01248 * scale;
            let bent = solve(ei, l, 0.0, 2000, &|_| (0.0, -q));
            assert!(bent.converged, "a light load must settle");
            let linear = q * l.powi(4) / (8.0 * ei);
            let ratio = bent.tip().1.abs() / linear;
            println!("  {linear:.6e}   {:.6e}   {ratio:.5}", bent.tip().1.abs());
            // It must DEPART from the linear answer DOWNWARD as the load grows: a real beam curls, so
            // its tip reaches less far than a theory assuming small slope predicts. The lightest case
            // sits AT 1 to within the discretisation rather than strictly under it — the rectangle
            // rule for `y = Σ sin θ ds` carries its own small error, and demanding "strictly below 1"
            // would be asserting a numerical artifact instead of the physics.
            assert!(
                ratio < prev + 1e-9,
                "larger loads must fall FURTHER below the linear answer, not closer to it"
            );
            prev = ratio;
        }
        // The lightest case IS the linear answer, to within the discretisation.
        let q = 0.01248 * 1.0e-4;
        let bent = solve(ei, l, 0.0, 2000, &|_| (0.0, -q));
        let linear = q * l.powi(4) / (8.0 * ei);
        assert!(
            (bent.tip().1.abs() / linear - 1.0).abs() < 1.0e-3,
            "limit not reached: {:.8}",
            bent.tip().1.abs() / linear
        );
    }

    /// ★★★ **GREENHILL: AN UPRIGHT BEAM STANDS UNTIL IT IS TOO LONG, AND THEN IT DOES NOT.**
    ///
    /// A second, INDEPENDENT physical reference with a known constant — `L_crit = (7.8373·EI/q)^⅓` is
    /// the first root of the governing Bessel problem, not a fitted number. A vertical column shorter
    /// than that holds itself up under its own weight; one longer than it cannot and must arch.
    ///
    /// The elastica is nudged off vertical by a whisper of sideways load, because a perfectly upright
    /// beam is in unstable equilibrium and would sit there forever — which is a fact about symmetry,
    /// not about stability.
    #[test]
    fn an_upright_beam_stands_below_greenhills_length_and_arches_above_it() {
        let (ei, q) = (3.78e-5, 0.01248);
        let l_crit = greenhill_critical_length_m(ei, q);
        println!("Greenhill L_crit = {:.4} m", l_crit);
        assert!((l_crit - (7.8373 * ei / q).cbrt()).abs() < 1e-12);

        let droop = |l: f64| {
            // Upright (+y), gravity down, plus a 1e-6 sideways nudge to break the symmetry.
            let bent = solve(ei, l, std::f64::consts::FRAC_PI_2, 800, &|_| {
                (-1.0e-6 * q, -q)
            });
            // How far the tip has fallen SHORT of standing straight up.
            (l - bent.tip().1) / l
        };
        let short = droop(l_crit * 0.5);
        let long = droop(l_crit * 1.6);
        println!("  at 0.50 L_crit the tip falls {short:.4} of its length short of vertical");
        println!("  at 1.60 L_crit it falls {long:.4} short");
        assert!(
            short < 0.05,
            "well below critical it should stand nearly straight, fell {short:.4}"
        );
        assert!(
            long > 6.0 * short,
            "past critical it must arch dramatically: {long:.4} vs {short:.4}"
        );
    }

    /// ★★ **THE BRIDGE TO `docs/18`'s LADDER, AND ITS FIRST READER.**
    ///
    /// The moment the elastica finds becomes a STRESS through the section (`σ = M·c/I`), and that
    /// stress is compared against the material's own limit — the same shape of question a crater asks
    /// of a yield strength and a haystack asks of a crush strength.
    ///
    /// `modulus_of_rupture` is catalogued on 2 of 37 materials and had NO reader until now; it was
    /// sitting in `laws::UNWIRED_MATERIAL_PROPERTIES` described as literally *"bending failure of a
    /// beam or plank"*. Oak has it, so an oak plank can be broken by bending; most things cannot, and
    /// the API returns `None` rather than inventing a limit.
    #[test]
    fn bending_stress_meets_the_materials_own_rupture_limit() {
        let mats = crate::materials::load();
        let oak = mats.iter().find(|m| m.id == "oak").expect("oak");
        let mor = rupture_stress_pa(oak).expect("oak records a modulus of rupture");
        println!("oak modulus of rupture: {:.1} MPa", mor / 1.0e6);

        // A 2 m oak plank, 100 mm wide, 20 mm thick, loaded across its weak axis.
        let (w, t, l) = (0.100f64, 0.020f64, 2.0f64);
        let i = w * t.powi(3) / 12.0;
        let ei = oak.youngs_modulus as f64 * i;
        // Find the tip load that breaks it: σ = M·c/I with M = F·L at the root.
        let f_break = mor * i / (0.5 * t) / l;
        println!("  EI {ei:.1} N·m² · a {l} m plank breaks at about {f_break:.0} N at the tip");
        let bent = solve(ei, l, 0.0, 800, &|_| (0.0, -f_break / l));
        let sigma = bending_stress_pa(bent.peak_moment_nm(), 0.5 * t, i);
        println!(
            "  distributed equivalent gives σ = {:.1} MPa",
            sigma / 1.0e6
        );
        assert!(sigma > 0.0);
        // A distributed load of the same total is gentler than a tip point load — half the moment —
        // so the plank must SURVIVE it. That directional check needs no threshold.
        assert!(
            sigma < mor,
            "the same total load spread along the plank must be gentler than at the tip: \
             {sigma:.3e} vs {mor:.3e}"
        );
        // And something with no catalogued limit says so instead of guessing.
        let water = mats.iter().find(|m| m.id == "water").expect("water");
        assert!(
            rupture_stress_pa(water).is_none(),
            "a material with no modulus of rupture must not acquire one"
        );
    }
}
