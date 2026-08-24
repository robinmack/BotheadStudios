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

/// ★★★ **WHERE THE ELASTIC BRANCH ENDS AND THE PLASTIC ONE BEGINS** — the bent nail.
///
/// Robin's example, and the rung above [`solve`]: a nail bent past its yield **stays bent**. Grass in
/// wind and a tree swaying are elastic and recover; a nail does not, and the difference is a single
/// comparison of stress against the material's yield.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Yielding {
    /// Moment at which the OUTERMOST fibre first reaches yield, N·m — `σ_y·Z`, `Z = I/c`.
    pub first_yield_nm: f64,
    /// Moment at which EVERY fibre has yielded, N·m — `σ_y·S`. A plastic hinge.
    pub fully_plastic_nm: f64,
    /// `S/Z`, pure geometry: how much more the section carries after first yield. **Exactly 1.5 for a
    /// rectangle**, ~1.7 for a solid circle, ~1.15 for an I-beam.
    pub shape_factor: f64,
}

/// The moments that bound the plastic rung, from the section's geometry and the material's yield.
pub fn yielding(
    yield_pa: f64,
    i_m4: f64,
    extreme_fibre_m: f64,
    plastic_modulus_m3: f64,
) -> Yielding {
    let z = if extreme_fibre_m > 0.0 {
        i_m4 / extreme_fibre_m
    } else {
        0.0
    };
    Yielding {
        first_yield_nm: yield_pa * z,
        fully_plastic_nm: yield_pa * plastic_modulus_m3,
        shape_factor: if z > 0.0 { plastic_modulus_m3 / z } else { 0.0 },
    }
}

/// ★★★ **WHAT IS LEFT AFTER YOU LET GO** — the permanent curvature, 1/m.
///
/// **Unloading is always elastic**, whatever the loading did: releasing a bent body removes `M/EI` of
/// curvature and no more. So the set that remains is `κ_applied − M/EI`, and that statement is
/// SECTION-INDEPENDENT — it is the one part of the plastic rung that needs no shape at all.
///
/// Below first yield `κ = M/EI` exactly, so the residual is zero and the body springs back
/// completely: **the elastic branch is the special case where this returns nothing**, which is the
/// reduction that ties the two rungs together.
pub fn residual_curvature_per_m(applied_curvature_per_m: f64, moment_nm: f64, ei_nm2: f64) -> f64 {
    if ei_nm2 <= 0.0 {
        return 0.0;
    }
    let sprung = applied_curvature_per_m - moment_nm / ei_nm2;
    // Springback cannot push curvature past straight, nor reverse its sign.
    if applied_curvature_per_m >= 0.0 {
        sprung.max(0.0)
    } else {
        sprung.min(0.0)
    }
}

/// The curvature an **elastic–perfectly-plastic RECTANGLE** reaches under a given moment, 1/m.
///
/// `M/M_y = 1.5·(1 − ⅓(κ_y/κ)²)` for `κ ≥ κ_y`, inverted to `κ/κ_y = 1/√(3 − 2M/M_y)`. Below first
/// yield it is simply `M/EI`. As `M → M_p = 1.5·M_y` the curvature runs away — that IS the plastic
/// hinge, not a numerical failure.
///
/// ★ **FLAGGED (Law V): this relation is RECTANGLE-SPECIFIC.** The shape factor and the residual above
/// are general; this moment–curvature curve is not, because it integrates the stress block over a
/// particular section. A general form needs the section's own `M(κ)`, which is owed. Rectangles cover
/// every `Slab` the catalogue currently builds from, so it is the honest first case rather than the
/// only one that could exist.
pub fn rectangle_curvature_per_m(moment_nm: f64, ei_nm2: f64, first_yield_nm: f64) -> f64 {
    if ei_nm2 <= 0.0 || first_yield_nm <= 0.0 {
        return 0.0;
    }
    let m = moment_nm.abs();
    let sign = if moment_nm < 0.0 { -1.0 } else { 1.0 };
    if m <= first_yield_nm {
        return moment_nm / ei_nm2;
    }
    let ky = first_yield_nm / ei_nm2;
    let ratio = m / first_yield_nm;
    let denom = 3.0 - 2.0 * ratio;
    if denom <= 0.0 {
        return sign * f64::INFINITY; // the hinge: M has reached 1.5·M_y and curvature is unbounded
    }
    sign * ky / denom.sqrt()
}

/// ★★★ **HOW A SLENDER BODY FAILS — a rock shatters, a tree snaps or splinters.**
///
/// Robin, 2026-08-21: *"a rock should be able to shatter, a tree should snap or splinter, etc."*
/// Rupture is not one event. WHAT happens is decided by the material's own structure, and the
/// catalogue already holds what decides it — those numbers simply had no reader.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Failure {
    /// Below every limit it holds, and the worst utilisation says by how much.
    Holds { worst_utilisation: f64 },
    /// **Bending stress reached the rupture limit** — it breaks ACROSS, a clean snap.
    Snaps { stress_pa: f64, limit_pa: f64 },
    /// **Longitudinal shear reached the across-grain limit FIRST** — the layers slide and it splits
    /// ALONG the grain. That is what a splinter is.
    Splinters { shear_pa: f64, limit_pa: f64 },
    /// **Isotropic and brittle**: no grain to split along, so it fragments. `damage::disrupt` is the
    /// law that makes the pieces — this is its second trigger, alongside atmospheric break-up.
    Shatters { stress_pa: f64, limit_pa: f64 },
}

/// **Is this material orthotropic**, and by how much? The ratio of along-grain to across-grain tensile
/// strength: 1.0 for something with no grain, **16.4 for oak, 33.3 for pine**. `None` where the
/// catalogue records no across-grain strength, which is most things.
pub fn orthotropy(m: &Material) -> Option<f64> {
    let perp = m.tensile_strength_perp.filter(|v| *v > 0.0)? as f64;
    let along = m.fracture_strength.max(m.compressive_strength) as f64;
    (along > 0.0).then_some(along / perp)
}

/// **The across-grain stiffness**, Pa — 2.0 GPa for oak against 12.3 along, 0.66 against 8.5 for pine.
/// A plank bent the other way is several times floppier, and this is the number that says so.
pub fn modulus_across_grain_pa(m: &Material) -> Option<f64> {
    m.youngs_modulus_perp.filter(|v| *v > 0.0).map(|v| v as f64)
}

/// **A grass BLADE's own tensile limit**, Pa — 150 MPa, against the turf mat's 15 kPa. A factor of ten
/// thousand, and the same fibre-vs-arrangement split `straw` records for its stem: the blade is the
/// substance, the turf is an arrangement of it. A blade that burns and breaks spends THIS one.
pub fn blade_tensile_pa(m: &Material) -> Option<f64> {
    m.tensile_strength_blade
        .filter(|v| *v > 0.0)
        .map(|v| v as f64)
}

/// ★★★ **WHICH LIMIT IS REACHED FIRST DECIDES THE MODE — nothing here is declared.**
///
/// A bent beam carries BOTH a bending stress `σ = M·c/I` along its length and a longitudinal shear
/// `τ = 3V/2A` between its layers. Each has its own limit, and failure is whichever utilisation
/// reaches 1 first:
///
/// - `σ/MoR` first, with no grain → **shatters**; with a grain → **snaps**.
/// - `τ/τ_perp` first → **splinters**, because the layers part before the fibres do.
///
/// ★★ **The mode therefore depends on the body's PROPORTIONS, not on a label.** For a rectangular
/// cantilever `σ/τ = 4L/t`, so bending governs once `L/t > MoR/(4·τ_limit)` — **1.90 for oak, 2.38 for
/// pine**. A slender plank snaps; a stubby one splits along the grain. That a tree does one and a
/// twig the other is a consequence, and nobody had to say which.
///
/// ★ Wood's across-grain limit is used for the shear check because that is the plane a splinter opens
/// on. Where a material records a `shear_strength` it is preferred, being the directly measured
/// quantity; `tensile_strength_perp` is the fallback and is flagged as such by returning the limit it
/// actually used.
pub fn fails(m: &Material, bending_stress_pa: f64, shear_stress_pa: f64) -> Failure {
    let sigma = bending_stress_pa.abs();
    let tau = shear_stress_pa.abs();
    let bend_limit = rupture_stress_pa(m);
    let shear_limit = m
        .shear_strength
        .filter(|v| *v > 0.0)
        .map(|v| v as f64)
        .or_else(|| {
            m.tensile_strength_perp
                .filter(|v| *v > 0.0)
                .map(|v| v as f64)
        });

    let u_bend = bend_limit.map_or(0.0, |l| sigma / l);
    let u_shear = shear_limit.map_or(0.0, |l| tau / l);

    if u_shear >= 1.0 && u_shear >= u_bend {
        return Failure::Splinters {
            shear_pa: tau,
            limit_pa: shear_limit.unwrap_or(0.0),
        };
    }
    if u_bend >= 1.0 {
        let limit = bend_limit.unwrap_or(0.0);
        // No across-grain strength recorded means no grain to split along: it fragments.
        return if orthotropy(m).is_some() {
            Failure::Snaps {
                stress_pa: sigma,
                limit_pa: limit,
            }
        } else {
            Failure::Shatters {
                stress_pa: sigma,
                limit_pa: limit,
            }
        };
    }
    Failure::Holds {
        worst_utilisation: u_bend.max(u_shear),
    }
}

/// The longitudinal shear stress in a rectangular section carrying shear force `v_n`, Pa — `3V/2A`.
/// Peak at the neutral axis, which is exactly where a plank splits.
pub fn shear_stress_pa(shear_force_n: f64, area_m2: f64) -> f64 {
    if area_m2 <= 0.0 {
        return 0.0;
    }
    1.5 * shear_force_n.abs() / area_m2
}

/// What a material's catalogue says about where the elastic branch ENDS, Pa. `None` where the
/// catalogue is silent — which for `modulus_of_rupture` is 35 of 37 materials, so most things cannot
/// yet be broken by bending and must say so rather than guess.
pub fn yield_stress_pa(m: &Material) -> Option<f64> {
    m.yield_strength.filter(|v| *v > 0.0).map(|v| v as f64)
}

/// What a material's catalogue says about where it BREAKS in bending, Pa.
pub fn rupture_stress_pa(m: &Material) -> Option<f64> {
    m.modulus_of_rupture.filter(|v| *v > 0.0).map(|v| v as f64)
}

/// **Where a bent body's tip ended up, and how hard its base was worked.** The three things a
/// de-resolved patch has to answer without the shape that produced them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Leaning {
    /// Tip displacement across the base direction, m — how far it leans.
    pub lean_m: f64,
    /// Tip displacement along the base direction, m — how tall it still stands.
    pub height_m: f64,
    /// The largest moment anywhere along it, N·m — what decides whether it yields or breaks.
    pub peak_moment_nm: f64,
}

/// ★★★ **THE DE-RESOLVED FORM OF BENDING** — what a patch of ten billion blades does, without
/// solving ten billion elasticas.
///
/// `solve` costs thousands of relaxation sweeps. A meadow holds ~10¹² blades and a forest ~10¹⁰
/// branches, and Law III says the un-resolved world is still *computed*, just cheaply. This is that
/// cheap computation, and Law 8 supplies its acceptance test: **it must converge to the resolved
/// answer as the budget grows.** `entries` is the budget knob.
///
/// ★★ **The obvious de-resolution is WRONG, and this type exists because of that.** The elastica is
/// nonlinear in load — a body already laid over bends far less for the next newton than it did for
/// the first — so the mean of the responses is NOT the response of the mean. A patch model that
/// pushes the average wind through one elastica reports a lean the patch does not have, and it is
/// wrong in a *consistent direction*, which is exactly what lets it survive inspection. That is
/// Jensen's inequality doing physics, and `flexure::tests` keeps it as a negative control so this
/// type cannot quietly decay back into it.
///
/// **The honest form comes from the scalability law** (`docs/67`): *identical until damaged*. The
/// TYPE tabulates its own compliance once; every instance reads it. Cost then scales with how many
/// distinct LOADS the patch sees, not with how many bodies stand in it — and the table is amortised
/// over every instance of that type that will ever exist.
///
/// **Nothing here is about grass.** The inputs are flexural rigidity, length and self weight, so a
/// wheat stem, a sapling, a fence post, a mast and a bent nail are the same call. One law, every
/// scale.
#[derive(Clone, Debug, PartialEq)]
pub struct Compliance {
    load_per_m: Vec<f64>,
    leaning: Vec<Leaning>,
}

impl Compliance {
    /// **Tabulate a type's bending response once**, from zero load to `max_load_per_m`, at `entries`
    /// evenly spaced loads. `weight_per_m` is the body's own weight per unit length, applied along
    /// −(base direction); the tabulated load acts across it.
    pub fn tabulate(
        ei_nm2: f64,
        length_m: f64,
        base_angle_rad: f64,
        stations: usize,
        weight_per_m: f64,
        max_load_per_m: f64,
        entries: usize,
    ) -> Compliance {
        let entries = entries.max(2);
        let mut load_per_m = Vec::with_capacity(entries);
        let mut leaning = Vec::with_capacity(entries);
        for i in 0..entries {
            let w = max_load_per_m * i as f64 / (entries - 1) as f64;
            let bent = solve(ei_nm2, length_m, base_angle_rad, stations, &|_| {
                (w, -weight_per_m)
            });
            let (x, y) = bent.tip();
            load_per_m.push(w);
            leaning.push(Leaning {
                lean_m: x,
                height_m: y,
                peak_moment_nm: bent.peak_moment_nm(),
            });
        }
        Compliance {
            load_per_m,
            leaning,
        }
    }

    /// **What one body of this type does under one load** — a table lookup, not an elastica.
    ///
    /// Beyond the tabulated range it returns the end entry rather than extrapolating: a linear
    /// extrapolation of a saturating curve runs away, and a body already flat cannot lean further.
    /// The honest thing is still for the caller to tabulate a range that covers its winds, so
    /// `max_load_per_m` reports what this table actually knows.
    pub fn at(&self, load_per_m: f64) -> Leaning {
        let n = self.leaning.len();
        if n == 0 {
            return Leaning::default();
        }
        if load_per_m <= self.load_per_m[0] {
            return self.leaning[0];
        }
        if load_per_m >= self.load_per_m[n - 1] {
            return self.leaning[n - 1];
        }
        let j = self
            .load_per_m
            .partition_point(|&l| l <= load_per_m)
            .clamp(1, n - 1);
        let (l0, l1) = (self.load_per_m[j - 1], self.load_per_m[j]);
        let t = if l1 > l0 {
            (load_per_m - l0) / (l1 - l0)
        } else {
            0.0
        };
        let (a, b) = (self.leaning[j - 1], self.leaning[j]);
        Leaning {
            lean_m: a.lean_m + t * (b.lean_m - a.lean_m),
            height_m: a.height_m + t * (b.height_m - a.height_m),
            peak_moment_nm: a.peak_moment_nm + t * (b.peak_moment_nm - a.peak_moment_nm),
        }
    }

    /// ★ **THE PATCH** — the mean response of many bodies of this type over the loads they each see.
    ///
    /// This is the whole de-resolution: **the load field is sampled, not the population.** Ten
    /// billion blades standing in one gust are a single lookup; ten billion blades in a hundred
    /// different gusts are a hundred. Neither is ten billion elasticas.
    pub fn over(&self, loads_per_m: &[f64]) -> Leaning {
        if loads_per_m.is_empty() {
            return Leaning::default();
        }
        let n = loads_per_m.len() as f64;
        let mut acc = Leaning::default();
        for &l in loads_per_m {
            let r = self.at(l);
            acc.lean_m += r.lean_m / n;
            acc.height_m += r.height_m / n;
            acc.peak_moment_nm += r.peak_moment_nm / n;
        }
        acc
    }

    /// How many elasticas this table cost — the price paid once per type, not once per instance.
    pub fn entries(&self) -> usize {
        self.leaning.len()
    }

    /// The largest load this table actually knows about; above it, `at` stops answering new things.
    pub fn max_load_per_m(&self) -> f64 {
        self.load_per_m.last().copied().unwrap_or(0.0)
    }
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

#[cfg(test)]
mod plastic_tests {
    use super::*;
    use crate::assembly::{Assembly, Shape};

    fn bar(t: f64, w: f64) -> Assembly {
        let mut a: Assembly =
            serde_json::from_str(r#"{"id":"bar","name":"bar","parts":[]}"#).expect("parses");
        let mut p: crate::assembly::Part = serde_json::from_str(
            r#"{"name":"b","material":"iron","shape":{"kind":"slab","x":1,"y":1,"z":1}}"#,
        )
        .expect("parses");
        p.shape = Shape::Slab { x: 1.0, y: t, z: w };
        p.along = [1.0, 0.0, 0.0];
        a.parts = vec![p];
        a
    }

    /// ★★★ **THE SHAPE FACTOR IS EXACTLY 1.5 FOR A RECTANGLE — PURE GEOMETRY, NO TOLERANCE TO TUNE.**
    ///
    /// `Z = bt²/6`, `S = bt²/4`, so `S/Z = 3/2` identically, for every rectangle at every scale and in
    /// every material. That a beam carries half as much again after its outermost fibre yields is a
    /// fact about its cross-section. If this ever drifts, the plastic modulus is not a plastic modulus.
    #[test]
    fn a_rectangles_shape_factor_is_exactly_three_halves() {
        for (t, w) in [(0.02, 0.10), (0.0003, 0.003), (1.0, 1.0), (0.5, 3.0)] {
            let a = bar(t, w);
            let sec = a.section().expect("a slab has a section");
            let s = a.plastic_modulus_v_m3().expect("and a plastic modulus");
            let z = sec.i_v_m4 / (0.5 * t);
            assert!(
                (s - w * t * t / 4.0).abs() <= (w * t * t / 4.0) * 1e-12,
                "S should be bt²/4: {s:.6e}"
            );
            assert!(
                (s / z - 1.5).abs() < 1e-12,
                "shape factor for {t}x{w}: {:.12}",
                s / z
            );
        }
    }

    /// ★★★ **AN ELASTIC BEND LEAVES NOTHING — THE REDUCTION THAT TIES THE TWO RUNGS TOGETHER.**
    ///
    /// Below first yield the residual curvature is identically zero: the body springs all the way
    /// back. Above it, something stays. That transition is the whole plastic rung, and it needs no
    /// threshold because yield supplies it.
    #[test]
    fn below_yield_it_springs_back_and_above_yield_it_stays_bent() {
        let mats = crate::materials::load();
        let iron = mats.iter().find(|m| m.id == "iron").expect("iron");
        let sy = yield_stress_pa(iron).expect("iron records a yield");
        println!("iron yield {:.0} MPa", sy / 1.0e6);

        // A nail: 3 mm square section, bent about its own axis.
        let (t, w) = (0.003f64, 0.003f64);
        let a = bar(t, w);
        let sec = a.section().expect("section");
        let sp = a.plastic_modulus_v_m3().expect("plastic modulus");
        let ei = iron.youngs_modulus as f64 * sec.i_v_m4;
        let y = yielding(sy, sec.i_v_m4, 0.5 * t, sp);
        println!(
            "  first yield {:.4} N·m · fully plastic {:.4} N·m · shape factor {:.3}",
            y.first_yield_nm, y.fully_plastic_nm, y.shape_factor
        );
        assert!((y.shape_factor - 1.5).abs() < 1e-12);

        // Below yield: springs back completely.
        let m_elastic = 0.9 * y.first_yield_nm;
        let k_elastic = rectangle_curvature_per_m(m_elastic, ei, y.first_yield_nm);
        let left = residual_curvature_per_m(k_elastic, m_elastic, ei);
        println!("  at 0.9x first yield: residual curvature {left:.3e} /m");
        assert!(
            left == 0.0,
            "an elastic bend must leave nothing: {left:.3e}"
        );

        // Past yield: it stays bent, and more so the harder it is bent.
        let mut prev = 0.0;
        for f in [1.05, 1.2, 1.35, 1.45] {
            let m = f * y.first_yield_nm;
            let k = rectangle_curvature_per_m(m, ei, y.first_yield_nm);
            let res = residual_curvature_per_m(k, m, ei);
            println!(
                "  at {f:.2}x first yield: residual {res:.4} /m  (radius {:.3} m)",
                1.0 / res
            );
            assert!(res > prev, "bending it further must leave MORE set");
            prev = res;
        }

        // ★ And the hinge: at the fully plastic moment the curvature is unbounded, which is what a
        // plastic hinge IS — not a solver failure.
        let k_hinge = rectangle_curvature_per_m(y.fully_plastic_nm, ei, y.first_yield_nm);
        assert!(
            k_hinge.is_infinite(),
            "M_p must be the hinge: {k_hinge:.3e}"
        );
    }

    /// A brittle material has no plastic rung to climb, and the catalogue says so rather than the code
    /// guessing. Grey cast iron's `compressive_strength` is a real crushing strength — 930 MPa against
    /// 295 MPa in tension — not a yield, so it carries none.
    #[test]
    fn a_brittle_material_has_no_yield_to_read() {
        let mats = crate::materials::load();
        for id in ["cast_iron", "nickel", "oak", "water"] {
            let m = mats.iter().find(|m| m.id == id).expect(id);
            assert!(
                yield_stress_pa(m).is_none(),
                "{id} must not acquire a yield it was never measured to have"
            );
        }
        for id in ["iron", "copper", "aluminium"] {
            let m = mats.iter().find(|m| m.id == id).expect(id);
            assert!(yield_stress_pa(m).is_some(), "{id} states its yield");
        }
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;

    /// ★★★ **THE SEVEN ORPHANS READ AT LAST.** `docs/46` row 30 records wood's orthotropic set as
    /// catalogued with ZERO readers, and names the blocker: *"a PART has no grain DIRECTION at all, so
    /// even a reader of those fields would not know which way the plank runs."* `Part::roll_rad`
    /// removed that, and these are the numbers it unblocked.
    #[test]
    fn the_orthotropic_properties_have_a_reader() {
        let mats = crate::materials::load();
        for (id, want) in [("oak", 16.4), ("pine", 33.3)] {
            let m = mats.iter().find(|m| m.id == id).expect(id);
            let r = orthotropy(m).expect("wood records an across-grain strength");
            let e = modulus_across_grain_pa(m).expect("and an across-grain stiffness");
            println!(
                "{id}: orthotropy {r:.1}x · across-grain modulus {:.2} GPa",
                e / 1e9
            );
            assert!(
                (r - want).abs() < 0.2,
                "{id} orthotropy {r:.2} vs expected ~{want}"
            );
            assert!(
                e < m.youngs_modulus as f64,
                "across the grain must be floppier"
            );
        }
        // A grass BLADE is not the turf mat it grows in — a factor of ten thousand.
        let grass = mats.iter().find(|m| m.id == "grass").expect("grass");
        let blade = blade_tensile_pa(grass).expect("a blade has its own limit");
        println!(
            "grass: blade {:.0} MPa vs turf mat {:.4} MPa — {:.0}x",
            blade / 1e6,
            grass.fracture_strength as f64 / 1e6,
            blade / grass.fracture_strength as f64
        );
        assert!(blade > 100.0 * grass.fracture_strength as f64);
        // Something with no grain has no orthotropy to report, rather than a default of 1.
        let granite = mats.iter().find(|m| m.id == "granite").expect("granite");
        assert!(orthotropy(granite).is_none(), "granite has no grain");
    }

    /// ★★★ **A ROCK SHATTERS; A TREE SNAPS OR SPLINTERS — AND WHICH ONE IS A CONSEQUENCE.**
    ///
    /// Nothing here labels a material with a failure mode. Both a bending stress and a longitudinal
    /// shear are computed, each compared against its own limit, and whichever reaches 1 first decides.
    /// Because `σ/τ = 4L/t` for a rectangular cantilever, the answer depends on the body's PROPORTIONS:
    /// slender snaps, stubby splits.
    #[test]
    fn the_failure_mode_falls_out_of_proportions_and_grain() {
        let mats = crate::materials::load();
        let oak = mats.iter().find(|m| m.id == "oak").expect("oak");
        let mor = rupture_stress_pa(oak).expect("oak ruptures");
        let tau_lim = oak.shear_strength.expect("oak records shear") as f64;
        let crossover = mor / (4.0 * tau_lim);
        println!(
            "oak: MoR {:.0} MPa, shear {:.1} MPa -> bending governs above L/t = {crossover:.2}",
            mor / 1e6,
            tau_lim / 1e6
        );

        // Load each plank right to its own limit, and see which limit that is.
        let probe = |l_over_t: f64| {
            // sigma/tau = 4L/t exactly; scale so the LARGER utilisation is exactly 1.
            let (sigma, tau) = (4.0 * l_over_t, 1.0);
            let k = 1.0 / (sigma / mor).max(tau / tau_lim);
            fails(oak, sigma * k, tau * k)
        };
        let stubby = probe(crossover * 0.5);
        let slender = probe(crossover * 2.0);
        println!("  L/t = {:.2}: {stubby:?}", crossover * 0.5);
        println!("  L/t = {:.2}: {slender:?}", crossover * 2.0);
        assert!(
            matches!(stubby, Failure::Splinters { .. }),
            "a stubby oak beam is shear-governed and must SPLIT along the grain: {stubby:?}"
        );
        assert!(
            matches!(slender, Failure::Snaps { .. }),
            "a slender oak beam is bending-governed and must SNAP across: {slender:?}"
        );

        // ★★★ A body with no grain to split along SHATTERS instead of snapping — and this now runs
        // against a REAL material. The synthetic grainless clone of oak that stood here has retired:
        // when this was written `modulus_of_rupture` was catalogued on oak and pine only, both grained,
        // so nothing in the catalogue could reach `Shatters` at all. Sourcing added basalt, granite,
        // limestone, sandstone, concrete, ice and cast iron — none of which records an across-grain
        // strength, because none of them has a grain.
        let granite = mats.iter().find(|m| m.id == "granite").expect("granite");
        let g_lim = rupture_stress_pa(granite).expect("granite ruptures now");
        let shattered = fails(granite, g_lim * 1.1, 0.0);
        println!(
            "  granite at 1.1x its {:.1} MPa limit: {shattered:?}",
            g_lim / 1e6
        );
        assert!(
            matches!(shattered, Failure::Shatters { .. }),
            "no grain means fragments, not a clean break: {shattered:?}"
        );
        // The ONLY difference between shattering and snapping is whether a grain exists: the same
        // fractional overload on grained oak snaps.
        assert!(
            matches!(fails(oak, mor * 1.1, 0.0), Failure::Snaps { .. }),
            "grain is the only thing that separates a snap from a shatter"
        );

        // And below every limit it simply holds, reporting how close it came.
        let safe = fails(oak, 0.25 * mor, 0.1 * tau_lim);
        println!("  a lightly loaded oak plank: {safe:?}");
        assert!(matches!(safe, Failure::Holds { .. }));
    }

    /// ★★★ **THE DE-RESOLVED FORM OF BENDING MUST CONVERGE TO THE BODIES IT REPLACES.**
    ///
    /// Law III: a meadow holds ~10¹² blades and the elastica costs thousands of sweeps each, so a
    /// patch CANNOT be resolved and must still be answered. Law 8 supplies the test — *"is this the
    /// most physical thing this budget can buy, and does it converge as the budget grows?"*
    ///
    /// ★★ **The trap this test exists to catch is that the obvious de-resolution is WRONG.** The
    /// elastica is nonlinear in load: a body already laid over bends far less for the next newton
    /// than the first did. So the mean of the responses is NOT the response of the mean, and a patch
    /// model that pushes the average wind through one elastica reports a lean the patch does not
    /// have. That is Jensen's inequality doing physics, and it is invisible without a control.
    ///
    /// ★★★ **AND EVERY SOLVE IS CHECKED FOR CONVERGENCE, because the first version of this test was
    /// not.** `solve` reports `converged` rather than asserting it, deliberately — a body that will
    /// not settle is a result. This test read the tip position anyway and compared two numbers that
    /// were the 20,000th iterate of something diverging. Consuming a solver's answer without reading
    /// the flag it sets to say *this is not an answer* is the same defect as a gate that exits 0.
    ///
    /// **Nothing here is about grass.** The inputs are rigidity, length and self weight, so a stem, a
    /// sapling, a fence post and a bent nail are the same call.
    #[test]
    fn a_de_resolved_patch_bends_like_the_bodies_it_replaces() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::by_id("grass-blade").expect("a shipped type");
        let sec = blade
            .section()
            .expect("a blade is a slab and has a section");
        let mat = mats
            .iter()
            .find(|m| m.id == blade.parts[0].material)
            .expect("the blade's material is catalogued");

        // ★ The blade's OWN sourced properties, not the sward's — see the note below on why the
        // generic `youngs_modulus` cannot be used here.
        let e_pa = 1.06e9f64;
        let rho = 710.0f64;
        let ei = e_pa * sec.i_v_m4;
        let q = rho * sec.area_m2 * 9.81;

        // ★★ STAY INSIDE THE SOLVER'S VALID REGIME. Above Greenhill's critical length the fixed-point
        // relaxation is not contractive and `solve` says so; a full-length 0.35 m blade is 1.7x it and
        // converges at NO non-zero load. This is a young blade, comfortably below critical.
        let l_crit = greenhill_critical_length_m(ei, q);
        let length = 0.5 * l_crit;
        println!(
            "grass blade: EI {ei:.3e} N·m² · q {q:.3e} N/m · Greenhill L_crit {l_crit:.4} m · using {length:.4} m ({:.2}x critical)",
            length / l_crit
        );

        let solve_one = |w: f64| {
            let b = solve(ei, length, std::f64::consts::FRAC_PI_2, 48, &|_| (w, -q));
            assert!(
                b.converged,
                "the elastica did not converge at {w:.4e} N/m ({} sweeps) — reading its tip would be \
                 reading the 20,000th iterate of something diverging",
                b.sweeps
            );
            let (x, y) = b.tip();
            (x, y, b.peak_moment_nm())
        };

        // A GUSTY patch: drag on a blade broadside, f' = ½·ρ_air·C_d·w·v², over a real wind range.
        let slab = |p: &crate::assembly::Part| match p.shape {
            crate::assembly::Shape::Slab { x: _, y: _, z } => z,
            _ => panic!("a blade's parts are slabs"),
        };
        let w_m: f64 = blade.parts.iter().map(slab).sum();
        let (rho_air, cd) = (1.225f64, 1.98f64);
        let drag = |v: f64| 0.5 * rho_air * cd * w_m * v * v;
        // ★ 1–6 m/s, an ordinary breeze. The top of the range is deliberate: `solve` is contractive
        // only while the tip lean stays under ~85% of the length (measured, and constant across
        // lengths — it is a DEFLECTION limit, not a load one). At this length that ceiling is ~25x
        // self weight, and 6 m/s sits inside it. A patch in a gale is outside what this solver can
        // answer, and that is a stated limit rather than a silent one.
        let loads: Vec<f64> = (0..24).map(|i| drag(1.0 + 5.0 * i as f64 / 23.0)).collect();
        let mean_load = loads.iter().sum::<f64>() / loads.len() as f64;
        let max_load = loads.iter().cloned().fold(0.0f64, f64::max);

        // RESOLVED: one elastica per blade.
        let n = loads.len() as f64;
        let mut resolved = (0.0, 0.0, 0.0);
        for &l in &loads {
            let (x, y, m) = solve_one(l);
            resolved = (resolved.0 + x / n, resolved.1 + y / n, resolved.2 + m / n);
        }
        println!(
            "  RESOLVED   ({} elasticas): lean {:.5} m · height {:.5} m · peak moment {:.3e} N·m",
            loads.len(),
            resolved.0,
            resolved.1,
            resolved.2
        );

        // DE-RESOLVED: the type tabulates once, the patch reads the table.
        let table =
            Compliance::tabulate(ei, length, std::f64::consts::FRAC_PI_2, 48, q, max_load, 12);
        let patch = table.over(&loads);
        println!(
            "  DE-RESOLVED ({} elasticas, amortised over every blade of this type ever): lean {:.5} m · height {:.5} m · peak {:.3e} N·m",
            table.entries(),
            patch.lean_m,
            patch.height_m,
            patch.peak_moment_nm
        );

        // ★ THE NEGATIVE CONTROL: the obvious de-resolution, one elastica at the mean wind.
        let (nx, ny, nm) = solve_one(mean_load);
        let naive_err = (nx - resolved.0).abs() / resolved.0.abs().max(1e-12);
        let good_err = (patch.lean_m - resolved.0).abs() / resolved.0.abs().max(1e-12);
        println!("  NAIVE (elastica at the mean wind): lean {nx:.5} m · height {ny:.5} m · peak {nm:.3e} N·m");
        println!(
            "  lean error — de-resolved {:.4}% · naive {:.4}%",
            100.0 * good_err,
            100.0 * naive_err
        );

        assert!(
            good_err < 0.01,
            "the de-resolved patch must match the bodies it replaces: {:.4}% off",
            100.0 * good_err
        );
        assert!(
            naive_err > 10.0 * good_err.max(1e-6),
            "WITHOUT this control the test would pass for a patch model that just solves the mean \
             wind — and that model is wrong by construction. naive {:.4}% vs de-resolved {:.4}%",
            100.0 * naive_err,
            100.0 * good_err
        );

        // ★★ AND IT MUST CONVERGE AS THE BUDGET GROWS (Law 8). A finer table is a better answer.
        let mut prev = f64::INFINITY;
        for k in [4usize, 8, 16, 32, 64] {
            let e =
                (Compliance::tabulate(ei, length, std::f64::consts::FRAC_PI_2, 48, q, max_load, k)
                    .over(&loads)
                    .lean_m
                    - resolved.0)
                    .abs()
                    / resolved.0.abs();
            println!("    {k:3} table entries -> {:.5}% off", 100.0 * e);
            assert!(
                e <= prev * 1.05,
                "refining the table must not make it worse: {k} entries {:.5}% vs previous {:.5}%",
                100.0 * e,
                100.0 * prev
            );
            prev = e;
        }
    }
}
