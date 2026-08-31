//! ★★★ **How finely time must be cut, and where** — the reusable half of "many objects, honestly".
//!
//! Robin, 2026-08-29, on the pile work: *"Note this for future massive multiple object work as a
//! useful pattern; if possible make it reusable."* So this is a module and not a fix inside
//! `pile.rs`. Every scene the engine is aimed at — a meadow of 10¹² blades, a hull splintering, a
//! debris field, a haystack — has the same shape of problem: **a few stiff interactions set a
//! timestep that the whole population then has to pay.**
//!
//! ## The rule
//!
//! A contact in this engine is a spring whose stiffness is stored PER MASS (`granular::Contact`), so
//! its angular frequency is simply `ω = √stiffness` and it needs no separate mass term. From that one
//! number everything else follows:
//!
//! ```text
//! ω    = √stiffness                     rad/s
//! t_c  = π/ω                            s     — how long one contact LASTS
//! dt_stable = 2/ω                       s     — symplectic Euler's stability limit
//! dt_accurate = t_c / steps_per_contact s     — resolving the contact, not merely surviving it
//! ```
//!
//! **Stability is not accuracy**, and conflating them is how a simulation runs without exploding and
//! still reports the wrong bounce. `dt_stable` says the integrator will not diverge; `dt_accurate`
//! says the answer has stopped moving. Only the second is a physics criterion, and the only honest
//! way to know `steps_per_contact` is to MEASURE where the answer converges — see
//! `substep::tests::how_finely_a_contact_must_be_resolved`, which finds it rather than assuming it.
//!
//! ★ **This replaces a dial.** `pile::settle` carried `dt = 0.1/√stiffness`, whose `0.1` traced to
//! nothing (Law V). It happens to sit near the right place, which is exactly why it survived — a
//! number that works is not a number that is justified.

/// The angular frequency of a per-mass contact spring, rad/s.
pub fn contact_omega(stiffness_per_mass: f64) -> f64 {
    stiffness_per_mass.max(0.0).sqrt()
}

/// **How long a single contact lasts**, s — half an oscillation of the contact spring, `π/ω`. This is
/// the physical duration a timestep has to resolve; everything else is a fraction of it.
pub fn contact_duration_s(stiffness_per_mass: f64) -> f64 {
    let w = contact_omega(stiffness_per_mass);
    if w <= 0.0 {
        return f64::INFINITY;
    }
    std::f64::consts::PI / w
}

/// **The largest step a symplectic-Euler contact survives**, s — `2/ω`. Beyond it the spring's own
/// oscillation is unstable and the pair flies apart no matter how good the rest of the model is.
///
/// Survival only. A step just inside this limit integrates a contact in a single lurch and gets the
/// bounce badly wrong while looking perfectly stable, which is the trap this pair of functions exists
/// to separate.
pub fn stable_dt_s(stiffness_per_mass: f64) -> f64 {
    let w = contact_omega(stiffness_per_mass);
    if w <= 0.0 {
        return f64::INFINITY;
    }
    2.0 / w
}

/// **The step at which a contact's ANSWER has stopped changing**, s — `t_c / steps_per_contact`.
///
/// `steps_per_contact` is not a taste parameter: it is measured, by halving the step until the
/// outcome stops moving. Damping enters too — a contact cannot be integrated more coarsely than its
/// own dissipation timescale `1/c`, or the damping term overshoots and removes more energy than the
/// material has.
pub fn accurate_dt_s(stiffness_per_mass: f64, normal_damp: f64, steps_per_contact: f64) -> f64 {
    let by_spring = contact_duration_s(stiffness_per_mass) / steps_per_contact.max(1.0);
    let by_damping = if normal_damp > 0.0 {
        1.0 / normal_damp / steps_per_contact.max(1.0)
    } else {
        f64::INFINITY
    };
    by_spring.min(by_damping)
}

/// ★★ **THE PATTERN: a population paying for its stiffest member.**
///
/// In any large scene most bodies are NOT in contact at a given instant — they are falling, drifting,
/// or resting undisturbed — and their motion is smooth enough for a far coarser step than a live
/// contact needs. `Plan` splits the two: advance everything at `outer_dt`, and subdivide only the
/// bodies that are actually touching.
///
/// **It changes nothing about the physics.** The same equations are integrated, more finely where the
/// solution is stiff — so it owes a convergence test the way every acceleration in this engine does
/// (Law 8: *does it converge as the budget grows?*). What it must never become is a coarser answer
/// wearing a faster runtime.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// The step the population advances at.
    pub outer_dt_s: f64,
    /// The step a body in contact is advanced at.
    pub inner_dt_s: f64,
    /// How many inner steps make one outer step — always ≥ 1, and exact, so no time is lost or gained.
    pub substeps: usize,
}

impl Plan {
    /// Build a plan from the contact's own physics and the coarsest step the free population needs.
    ///
    /// `free_dt_s` is what an untouched body requires — for a falling blade that is set by gravity and
    /// air, not by any spring. The result never has an outer step longer than that, and never an inner
    /// step longer than the contact can bear.
    pub fn new(
        stiffness_per_mass: f64,
        normal_damp: f64,
        steps_per_contact: f64,
        free_dt_s: f64,
    ) -> Plan {
        let need = accurate_dt_s(stiffness_per_mass, normal_damp, steps_per_contact);
        let outer = free_dt_s.max(f64::MIN_POSITIVE);
        if need >= outer {
            // The contact is softer than the free motion is demanding: one step does for both.
            return Plan {
                outer_dt_s: outer,
                inner_dt_s: outer,
                substeps: 1,
            };
        }
        let substeps = (outer / need).ceil().max(1.0) as usize;
        Plan {
            outer_dt_s: outer,
            inner_dt_s: outer / substeps as f64,
            substeps,
        }
    }

    /// What this plan saves against advancing EVERYTHING at the inner step — the honest statement of
    /// the win, as a factor, for a population of which `contacting_fraction` is touching.
    pub fn speedup_over_uniform(&self, contacting_fraction: f64) -> f64 {
        let f = contacting_fraction.clamp(0.0, 1.0);
        let uniform = self.substeps as f64;
        let planned = f * self.substeps as f64 + (1.0 - f);
        uniform / planned.max(f64::MIN_POSITIVE)
    }
}

/// ★★★ **THE BLOCK-TIMESTEP SCHEDULE — who moves on which sub-tick, and nothing about what they are.**
///
/// This is the arithmetic half of `Aggregate::step_block` (docs/30 stage 3), lifted out so it is not
/// welded to point-mass particles. The pile's rods, impact debris, ejecta and smoke are the same
/// problem wearing different bodies: **most of a population is slow, and a few fast members set the
/// step everyone pays.** Bucket by rate, integrate each at its own step, and evaluate expensive forces
/// only for the members ending a step.
///
/// Robin, 2026-08-29: *"Engine should be able to use similar optimizations for impact debris, smoke
/// ejection, etc."* — which is why this is body-agnostic. It takes admissible timesteps in and hands
/// out levels and active sets; it never learns what a body is.
///
/// Levels are powers of two so that sub-ticks nest exactly and no member's step drifts out of phase
/// with another's, which is what keeps a contact pair symmetric.
#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    level: Vec<u32>,
    lmax: u32,
    base_dt_s: f64,
}

impl Schedule {
    /// Bucket a population by the timestep each member can bear.
    ///
    /// `admissible_dt_s[i]` is what member `i` needs — from a contact via [`accurate_dt_s`], from a
    /// free-fall time, from a CFL condition, whatever is physical for that member. `lmax` caps runaway
    /// subdivision: a single pathological member must not force `2^L` sub-ticks on the whole
    /// population, and the cap is a DECLARED bound on the acceleration, not on the physics — a member
    /// that wants a finer step than `base_dt/2^lmax` simply gets the finest step available and its
    /// error is bounded rather than resolved.
    pub fn new(admissible_dt_s: &[f64], base_dt_s: f64, lmax: u32) -> Schedule {
        let level: Vec<u32> = admissible_dt_s
            .iter()
            .map(|&t| {
                if !t.is_finite() || t >= base_dt_s {
                    0
                } else {
                    (base_dt_s / t).log2().ceil().clamp(0.0, lmax as f64) as u32
                }
            })
            .collect();
        let lmax = level.iter().copied().max().unwrap_or(0);
        Schedule {
            level,
            lmax,
            base_dt_s,
        }
    }

    /// How many sub-ticks make one base step.
    pub fn sub_ticks(&self) -> u32 {
        1u32 << self.lmax
    }

    /// The finest sub-step.
    pub fn min_dt_s(&self) -> f64 {
        self.base_dt_s / self.sub_ticks() as f64
    }

    /// Sub-ticks between member `i`'s own kicks.
    pub fn stride(&self, i: usize) -> u32 {
        1u32 << (self.lmax - self.level[i])
    }

    /// Member `i`'s own timestep.
    pub fn dt_for(&self, i: usize) -> f64 {
        self.min_dt_s() * self.stride(i) as f64
    }

    /// Is member `i` STARTING one of its own steps at this sub-tick? (Its opening half-kick.)
    pub fn starts(&self, sub: u32, i: usize) -> bool {
        sub % self.stride(i) == 0
    }

    /// Is member `i` ENDING one of its own steps at this sub-tick? These are the members that need a
    /// fresh force — the set an expensive evaluation should be restricted to.
    pub fn ends(&self, sub: u32, i: usize) -> bool {
        (sub + 1) % self.stride(i) == 0
    }

    pub fn level(&self, i: usize) -> u32 {
        self.level[i]
    }

    pub fn len(&self) -> usize {
        self.level.len()
    }

    pub fn is_empty(&self) -> bool {
        self.level.is_empty()
    }

    /// ★ **What the schedule actually buys**, as a factor against advancing everyone at the finest
    /// step. This is the honest statement of the win and it should be reported rather than assumed —
    /// a schedule whose members all land in the fastest bucket saves exactly nothing, and that is a
    /// result worth seeing rather than a disappointment worth hiding.
    pub fn speedup_over_uniform(&self) -> f64 {
        if self.level.is_empty() {
            return 1.0;
        }
        let uniform = self.level.len() as f64 * self.sub_ticks() as f64;
        let planned: f64 = (0..self.level.len())
            .map(|i| self.sub_ticks() as f64 / self.stride(i) as f64)
            .sum();
        uniform / planned.max(f64::MIN_POSITIVE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ **THE EXTRACTED SCHEDULE IS THE ONE `Aggregate::step_block` ALREADY USED.**
    ///
    /// `step_block` is verified — its doc records that it conserves energy and reproduces the global-dt
    /// result — so the risk in lifting its arithmetic out is not that the new code is wrong in the
    /// abstract, but that it is SUBTLY different from the code that earned that verification. This
    /// re-derives the inline version's levels, strides and active sets and demands they agree
    /// term-by-term. If they ever diverge, the extraction is the bug, not the caller.
    #[test]
    fn the_schedule_reproduces_the_inline_block_timestep_exactly() {
        const LMAX: u32 = 6;
        for (dt, ts) in [
            (1.0, vec![2.0, 0.5, 0.26, 0.01, f64::INFINITY, 1.0]),
            (0.125, vec![0.125, 0.0624, 1.0e-9, 0.03]),
            (1.0e-3, vec![1.0e-3; 5]),
            (1.0, vec![f64::NAN, 0.4, 0.0]),
        ] {
            // The inline logic, transcribed from `aggregate::Aggregate::step_block`.
            let level: Vec<u32> = ts
                .iter()
                .map(|&t| {
                    if !t.is_finite() || t >= dt {
                        0
                    } else {
                        (dt / t).log2().ceil().clamp(0.0, LMAX as f64) as u32
                    }
                })
                .collect();
            let lmax = level.iter().copied().max().unwrap_or(0);
            let n = 1u32 << lmax;
            let dt_min = dt / n as f64;
            let stride = |l: u32| 1u32 << (lmax - l);

            let s = Schedule::new(&ts, dt, LMAX);
            assert_eq!(s.sub_ticks(), n, "sub-tick count for dt={dt}");
            assert!((s.min_dt_s() - dt_min).abs() <= dt_min * 1e-15);
            for i in 0..ts.len() {
                assert_eq!(s.level(i), level[i], "level[{i}] for dt={dt}");
                assert_eq!(s.stride(i), stride(level[i]), "stride[{i}] for dt={dt}");
                for sub in 0..n {
                    assert_eq!(
                        s.starts(sub, i),
                        sub % stride(level[i]) == 0,
                        "starts({sub},{i})"
                    );
                    assert_eq!(
                        s.ends(sub, i),
                        (sub + 1) % stride(level[i]) == 0,
                        "ends({sub},{i})"
                    );
                }
            }
            println!(
                "dt {dt:.3e} · {} members · {} sub-ticks · speedup {:.2}x",
                ts.len(),
                s.sub_ticks(),
                s.speedup_over_uniform()
            );
        }
    }

    /// ★★★ **HOW FINELY A CONTACT MUST BE RESOLVED — MEASURED, NOT CHOSEN.**
    ///
    /// `pile::settle` used `dt = 0.1/√stiffness`, and `granular` used the same expression a third time.
    /// The `0.1` traces to nothing (Law V). This finds the real number the only honest way: integrate
    /// one bounce at successively finer steps and see where the ANSWER stops moving.
    ///
    /// The reference is exact. A linear spring-dashpot released at `v₀` rebounds at
    /// `e = exp(−ζπ/√(1−ζ²))`, which `granular::restitution_of_damping_ratio` already implements and
    /// which does not depend on the integrator at all — so a disagreement is the timestep's fault and
    /// nobody else's.
    #[test]
    fn how_finely_a_contact_must_be_resolved() {
        let k = 6.8e9f64; // per-mass stiffness of a straw stem contact — the stiffest in the pile
        let zeta = 0.3f64;
        let c = 2.0 * zeta * k.sqrt();
        let want = crate::granular::restitution_of_damping_ratio(zeta);
        let t_c = contact_duration_s(k);
        println!(
            "contact: ω {:.4e} rad/s · lasts {t_c:.4e} s · stable dt {:.4e} s · exact e {want:.6}",
            contact_omega(k),
            stable_dt_s(k)
        );

        // ★★★ USE THE ENGINE'S OWN CONTACT LAW, not a hand-rolled spring-dashpot.
        //
        // The first version of this test integrated `a = −kx − cv` directly and converged beautifully
        // to **0.372**, the textbook `exp(−ζπ/√(1−ζ²))` — while the engine answers **0.451**. The gap
        // is not numerical: `granular::contact_accel` is a NO-TENSION contact, so the dashpot is not
        // permitted to pull the bodies back together as they separate, and a contact that cannot pull
        // rebounds faster than the textbook one. I had written a second contact law and then carefully
        // measured its convergence. Law II, caught by a reference that disagreed.
        let contact = crate::granular::Contact {
            radius: 0.5,
            stiffness: k,
            normal_damp: c,
            friction: 0.0,
            tangent_damp: 0.0,
            cohesion: 0.0,
            coh_range: 0.0,
            shock: 0.0,
        };
        let touch = 2.0 * contact.radius;

        let mut converged_at = None;
        let mut prev = f64::NAN;
        for spc in [1.0f64, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0] {
            let dt = t_c / spc;
            // One grain, head-on into a fixed one, through the SAME call the pile makes.
            let mut p = glam::DVec3::new(touch, 0.0, 0.0);
            let mut v = glam::DVec3::new(-1.0, 0.0, 0.0);
            let fixed_p = glam::DVec3::ZERO;
            let fixed_v = glam::DVec3::ZERO;
            let mut steps = 0u64;
            loop {
                let a = crate::granular::contact_accel(p, v, fixed_p, fixed_v, &contact);
                v += a * dt;
                p += v * dt;
                steps += 1;
                if v.x > 0.0 && (p.x - fixed_p.x) >= touch {
                    break; // separated and moving away
                }
                if steps > 20_000_000 {
                    break;
                }
            }
            let got = v.x.abs();
            let err = (got - want).abs() / want;
            println!(
                "  {spc:6.0} steps/contact (dt {dt:.3e}) -> e {got:.6} · {:.4}% off · {steps} steps"
            , 100.0 * err);
            if err < 0.01 && converged_at.is_none() {
                converged_at = Some(spc);
            }
            prev = err;
        }
        let _ = prev;
        let spc = converged_at.expect("the bounce must converge to the analytic restitution");
        println!("  -> converged (within 1%) at {spc} steps per contact");
        // The old dial, in the same units, so the two are comparable.
        let old_dt = 0.1 / k.sqrt();
        println!(
            "  the retired `0.1/√k` dial was dt {old_dt:.4e} s = {:.1} steps per contact",
            t_c / old_dt
        );
        assert!(
            spc <= 128.0,
            "a contact needing more than 128 steps is a different problem: {spc}"
        );
    }
}

#[cfg(test)]
mod rotational_tests {
    use super::*;
    use glam::DVec3;

    /// ★★★ **WHAT STEP A ROTATIONAL CONTACT MODE ACTUALLY NEEDS — MEASURED, because two derivations
    /// of it disagreed** (docs/46 row 73, suspect 1).
    ///
    /// [`accurate_dt_s`] takes the contact's stiffness and nothing else, so the step it returns
    /// resolves the TRANSLATIONAL contact oscillation `ω = √stiffness`. A contact that also turns the
    /// body has a second mode, whose frequency depends on the effective inertia at the contact point —
    /// and for a grass blade the axial moment is `~10⁷` below the other two, so that mode can be far
    /// stiffer than the one the step was chosen for.
    ///
    /// ★★ **I could not settle it on paper.** One derivation said 2.1× finer would do; another said the
    /// rotational mode is SLOWER than translation and therefore not the constraint at all. When two of
    /// my own derivations disagree, the next plausible patch is a guess — so this finds the boundary by
    /// bisection instead, on the exact configuration that blew up.
    ///
    /// Reported as a ratio against what `accurate_dt_s` currently hands out, because that ratio IS the
    /// answer to *is the timestep the problem?*
    #[test]
    #[ignore = "sweeps a stiff contact at up to 64x resolution — seconds, not milliseconds"]
    fn what_step_a_rotational_contact_mode_actually_needs() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = crate::pile::rod_for(&blade).expect("rod");
        let (width, thickness) = crate::pile::cross_section_for(&blade).expect("cross-section");
        let mass = blade.mass_kg(&mats).expect("mass");
        let m = mats.iter().find(|m| m.id == "straw").expect("straw");
        let contact = crate::granular::contact_from_material(m, radius, mass);
        let air = {
            let a = mats.iter().find(|m| m.id == "air").expect("air");
            crate::atmosphere::air_density_at(101_325.0, a, 288.15, 9.81, 0.0)
        };

        let base = accurate_dt_s(contact.stiffness, contact.normal_damp, 32.0).min(1.0e-4);
        println!(
            "contact ω {:.4e} rad/s · accurate_dt_s(32) = {base:.4e} s · stable_dt {:.4e} s",
            contact_omega(contact.stiffness),
            stable_dt_s(contact.stiffness)
        );

        let spin0 = 20.0f64;
        let run = |dt: f64| -> (f64, f64) {
            let mut rod = crate::pile::Rod {
                centre: DVec3::new(0.0, radius, 0.0),
                axis: DVec3::X,
                half_length_m: 0.5 * length,
                radius_m: radius,
                width_m: width,
                thickness_m: thickness,
                normal: DVec3::Y,
                vel: DVec3::ZERO,
                ang_vel: DVec3::new(0.0, spin0, 0.0),
                release_t_s: 0.0,
            };
            let energy = |r: &crate::pile::Rod| {
                let i = r.principal_inertia_kgm2(mass);
                let f = r.frame();
                let wb = DVec3::new(
                    r.ang_vel.dot(f[0]),
                    r.ang_vel.dot(f[1]),
                    r.ang_vel.dot(f[2]),
                );
                0.5 * mass * r.vel.length_squared()
                    + 0.5 * (i.x * wb.x * wb.x + i.y * wb.y * wb.y + i.z * wb.z * wb.z)
                    + mass * 9.81 * r.centre.y
            };
            let e0 = energy(&rod);
            let steps = (0.05 / dt).ceil() as u64;
            for _ in 0..steps {
                crate::pile::step_one_rod(
                    &mut rod,
                    mass,
                    &contact,
                    9.81,
                    air,
                    dt,
                    DVec3::ZERO,
                    DVec3::ZERO,
                );
            }
            // ★ TOTAL mechanical energy — translation, ROTATION and height. Gravity is conservative and
            // every other channel here (contact, friction, air) is dissipative, so a rise is
            // manufactured energy and nothing else.
            (rod.ang_vel.length(), energy(&rod) / e0.max(1e-30))
        };

        println!(
            "  a blade spinning about the VERTICAL, 0.05 s of simulated time, from {spin0} rad/s:"
        );
        let mut first_stable: Option<f64> = None;
        for div in [1.0f64, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0] {
            let dt = base / div;
            let (w, e_ratio) = run(dt);
            let verdict = if w > spin0 { "RUNS AWAY" } else { "decays" };
            println!(
                "    dt = base/{div:<5.0} = {dt:.4e} s -> |ω| {w:9.4} rad/s · energy x{e_ratio:8.2}  {verdict}"
            );
            if w <= spin0 && first_stable.is_none() {
                first_stable = Some(div);
            }
        }
        match first_stable {
            Some(d) => println!(
                "  -> the rotational mode needs the step {d}x finer than `accurate_dt_s` gives it"
            ),
            None => println!("  -> STILL unstable at 128x finer: the timestep is NOT the cause"),
        }
    }
}
