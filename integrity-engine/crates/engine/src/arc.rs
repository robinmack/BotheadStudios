//! **The out-and-back demo arc: one continuous camera parameterization, surface to celestial**
//! (docs/59's descent-camera remainder, driven by the demo choreography: open at the ball,
//! continuous pull-out while sim time compresses, witness the de-orbit at celestial scale,
//! continuous descent back: no cuts, either direction).
//!
//! Pure math, natively tested. The space band consumes it as a CAMERA/TIME driver only: it moves
//! the eye and the observable time rate, never any matter (Law IV/VI: representation and pacing,
//! not existence). The materialize/fold crossings along the path belong to `crate::site`'s
//! bidirectional trigger, which this arc merely flies through in both directions.
//!
//! # The parameterization
//!
//! The arc is a single scalar: the camera's distance `d` to the site, swept GEOMETRICALLY
//! (`d ← d·e^{∓dt/τ}`, `τ = octave_s/ln 2`) so every octave of scale gets the same real screen
//! time: the world's declared `arc.octave_s` is the one pacing statement, the same kind of
//! viewing declaration as its `time.scale`. Both ends are DERIVED, not dialled:
//!
//! - **floor** `= q/θ`: the view-resolution distance ([`crate::site::view_resolution_distance`])
//!   of the site's own finest materialized quantum `q` (the ball's one-rung child). The camera
//!   descends exactly as deep as the current representation can honestly serve the view; going
//!   closer would owe the eye a rung that does not exist yet (docs/59 item 4's remainder).
//!   A side effect the tests pin: at this floor the absolute-f32 render path's quantization at
//!   planet radius still subtends under a pixel, which is why the arc does not yet need Terra's
//!   camera-relative-eye convention: the floor stops exactly where that convention would begin.
//! - **top**: the wider of the scene's whole-orbit framing and [`WHOLE_ORBIT_MARGIN`] times the
//!   site's fold threshold, so one pull-out always crosses the deresolve threshold and the
//!   descent re-crosses it (the out-and-back contract).
//!
//! # The pacing law (altitude → time compression)
//!
//! `S(d) = clamp(S_decl · d / d_top, 1, S_decl)`: sim-seconds per real-second proportional to
//! camera distance, anchored to the world's DECLARED celestial `time.scale` at the top and
//! flooring at real time near the ground. Derivation: matter moving at speed `v` seen from
//! distance `d` crosses the view at angular rate `v·S/d`; holding `S ∝ d` holds that apparent
//! rate constant, so the de-orbit reads at the same visual pace at every altitude of the
//! pull-out. No number here is new: `S_decl` is the world's, `d_top` is derived above.
//!
//! # Riding a spinning crust without a cut
//!
//! The site rides the rotating crust; under compression the crust can turn in under a second, so
//! a camera rigidly anchored to either the crust or inertial space breaks at one end or the
//! other. Two derived rules resolve it:
//!
//! - **The reciprocal rule**: every crust-anchored quantity (the hover direction, the look
//!   target, the view-up) is weighted by `w = 1/S(d)`. Its apparent drift rate is then
//!   `w · Ω·S = Ω`: never faster than Earth's REAL rotation, at any compression. At the floor
//!   (`S = 1`) the camera co-rotates fully and the site holds still underfoot; at the top it is
//!   inertial.
//! - **The spin lead**: a descent that homed on where the site IS would find it long gone,
//!   because the crust keeps turning while the camera glides down. The camera aims where the site WILL
//!   be: the remaining crust turn under the geometric glide is closed-form,
//!   `Θ_rem = Ω·τ·(S(d) − 1)` (with `S ∝ d` and `ḋ = −d/τ`, `d(θ + Θ_rem)/dt = Ω·S − Ω·S = 0`),
//!   so the led aim point is a CONSTANT of the glide and the site rotates into place beneath the
//!   camera exactly as it arrives. Tested numerically below.
//!
//! The azimuthal swing from the camera's start direction to the (led) site direction is spent
//! log-uniformly over the octaves ABOVE one planet radius: while the planet is an object in
//! frame, panning around it reads; below `d = r` the approach is straight overhead
//! ([`approach_angle`]).

use glam::{DQuat, DVec3};

/// The scene's whole-orbit framing margin: view distance = 1.7 × the thing being framed. This is
/// the space band's existing convention (its `base_distance` has always been 1.7 × the declared
/// planet–moon separation); the arc reuses it for the top of the path rather than inventing a
/// second margin.
pub const WHOLE_ORBIT_MARGIN: f64 = 1.7;

/// The derived span and declared pacing of one out-and-back arc (module doc).
#[derive(Clone, Copy, Debug)]
pub struct ArcSpan {
    /// The surface end (m): the finest materialized quantum's view-resolution distance.
    pub d_floor_m: f64,
    /// The celestial end (m): whole-orbit framing or the fold threshold at the same margin.
    pub d_top_m: f64,
    /// The world's declared celestial time scale: the pacing law's anchor at the top.
    pub declared_scale: f64,
    /// The world's declared arc pacing: real seconds per octave (×2) of camera distance.
    pub octave_s: f64,
}

impl ArcSpan {
    /// Derive the span (module doc): floor from the finest quantum against the docs/49 angular
    /// budget, top from the whole-orbit framing and the fold threshold.
    pub fn derive(
        finest_quantum_m: f64,
        angular_resolution_rad: f64,
        whole_orbit_m: f64,
        fold_threshold_m: f64,
        declared_scale: f64,
        octave_s: f64,
    ) -> ArcSpan {
        ArcSpan {
            d_floor_m: crate::site::view_resolution_distance(finest_quantum_m, angular_resolution_rad),
            d_top_m: whole_orbit_m.max(WHOLE_ORBIT_MARGIN * fold_threshold_m),
            declared_scale: declared_scale.max(1.0),
            octave_s: octave_s.max(1.0e-3),
        }
    }

    /// The glide's e-folding time (s): one octave of distance per `octave_s`.
    pub fn tau_s(&self) -> f64 {
        self.octave_s / std::f64::consts::LN_2
    }

    /// **The pacing law** (module doc): sim time compression proportional to camera distance,
    /// the declared celestial scale at the top, real time at/below the knee `d_top/S_decl`.
    pub fn time_scale(&self, d_m: f64) -> f64 {
        (self.declared_scale * d_m / self.d_top_m).clamp(1.0, self.declared_scale)
    }

    /// **The reciprocal rule** (module doc): the crust-anchoring weight `1/S(d)`, so nothing
    /// crust-anchored ever drifts across the view faster than Earth's real rotation.
    pub fn crust_weight(&self, d_m: f64) -> f64 {
        1.0 / self.time_scale(d_m)
    }

    /// **The spin lead** (module doc): the crust turn remaining in the rest of the descent,
    /// closed-form under the geometric glide. Zero at/below the real-time knee.
    pub fn lead_angle_rad(&self, d_m: f64, spin_rate_rad_s: f64) -> f64 {
        spin_rate_rad_s * self.tau_s() * (self.time_scale(d_m) - 1.0)
    }

    /// One real-time step of the geometric glide, clamped at the travelled-toward end.
    pub fn glide(&self, d_m: f64, real_dt_s: f64, descending: bool) -> f64 {
        let f = (real_dt_s / self.tau_s()).exp();
        if descending {
            (d_m / f).max(self.d_floor_m)
        } else {
            (d_m * f).min(self.d_top_m)
        }
    }
}

/// One glide leg's captured state: the direction (from the planet centre) the camera set out
/// from, and the distance it set out at: everything else is a pure function of `d` and the
/// current crust orientation, so the pose is stateless frame to frame.
#[derive(Clone, Copy, Debug)]
pub struct Leg {
    pub from_dir: DVec3,
    pub d_start_m: f64,
}

/// The azimuthal swing still to spend at distance `d` on a descent that started `alpha_rad` away
/// from the (led) site direction: log-uniform over the octaves above one planet radius, zero
/// at and below it (module doc: pan while the planet is an object, overhead once at it).
pub fn approach_angle(alpha_rad: f64, d_m: f64, d_start_m: f64, r_planet_m: f64) -> f64 {
    if alpha_rad <= 0.0 || d_start_m <= r_planet_m {
        return 0.0;
    }
    let t = (d_m / r_planet_m).max(1.0).ln() / (d_start_m / r_planet_m).ln();
    alpha_rad * t.clamp(0.0, 1.0)
}

/// Great-circle interpolation of unit directions, `t = 0 → a`, `t = 1 → b`. An antipodal pair
/// takes a deterministic detour axis rather than collapsing (the normalized-lerp failure mode).
pub fn slerp_dir(a: DVec3, b: DVec3, t: f64) -> DVec3 {
    let a = a.normalize_or(DVec3::Y);
    let b = b.normalize_or(DVec3::Y);
    let angle = a.dot(b).clamp(-1.0, 1.0).acos();
    if angle < 1.0e-12 {
        return a;
    }
    let mut axis = a.cross(b);
    if axis.length_squared() < 1.0e-18 {
        // Antipodal: any perpendicular is a great circle; pick one deterministically.
        axis = a.cross(DVec3::Z);
        if axis.length_squared() < 1.0e-18 {
            axis = a.cross(DVec3::X);
        }
    }
    DQuat::from_axis_angle(axis.normalize(), angle * t.clamp(0.0, 1.0)) * a
}

/// The camera's direction from the planet centre on a DESCENDING leg (module doc): the spin-led
/// site direction, offset toward the leg's start direction by the remaining approach swing.
pub fn descend_dir(
    span: &ArcSpan,
    leg: &Leg,
    d_m: f64,
    site_dir_now: DVec3,
    spin_axis: DVec3,
    spin_rate_rad_s: f64,
    r_planet_m: f64,
) -> DVec3 {
    let lead = span.lead_angle_rad(d_m, spin_rate_rad_s);
    let led = DQuat::from_axis_angle(spin_axis.normalize_or(DVec3::Y), lead) * site_dir_now;
    let alpha = led.dot(leg.from_dir).clamp(-1.0, 1.0).acos();
    if alpha <= 0.0 {
        return led;
    }
    let psi = approach_angle(alpha, d_m, leg.d_start_m, r_planet_m);
    slerp_dir(led, leg.from_dir, (psi / alpha).clamp(0.0, 1.0))
}

/// The camera's direction from the planet centre on an ASCENDING leg: crust-anchored by the
/// reciprocal rule at the bottom (lift off straight over the site), the leg's frozen inertial
/// direction once compression takes hold.
pub fn ascend_dir(span: &ArcSpan, leg: &Leg, d_m: f64, site_dir_now: DVec3) -> DVec3 {
    slerp_dir(leg.from_dir, site_dir_now, span.crust_weight(d_m))
}

/// The look target (planet-centred, m): the SITE under the reciprocal rule. Exactly the site at
/// the floor, the planet centre in the celestial regime, never drifting faster than the real
/// spin in between.
pub fn look_target(span: &ArcSpan, d_m: f64, site_vec_m: DVec3) -> DVec3 {
    site_vec_m * span.crust_weight(d_m)
}

/// The view-up: local north over the site at the floor (the fly camera's orbital-regime
/// convention), the manual rig's world up at the top, the reciprocal rule between.
pub fn view_up(span: &ArcSpan, d_m: f64, north_now: DVec3, manual_up: DVec3) -> DVec3 {
    let w = span.crust_weight(d_m);
    (north_now * w + manual_up * (1.0 - w)).normalize_or(manual_up.normalize_or(DVec3::Y))
}

/// Eye position (planet-centred, m) at distance `d` along direction `v`: always outside the
/// surface sphere by construction (`|eye| = r + d`), so no pose along the arc can enter the
/// planet.
pub fn eye(v: DVec3, r_surface_m: f64, d_m: f64) -> DVec3 {
    v * (r_surface_m + d_m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;
    use crate::site::{self, SiteSpec};
    use crate::terra::world_def::World;

    /// Earth's sidereal rotation rate (rad/s): the test crust the arc must ride.
    const OMEGA: f64 = 2.0 * std::f64::consts::PI / 86_164.0;

    fn shipped_ground_zero() -> World {
        let json = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/public/worlds/ground-zero/world.json"
        ))
        .expect("shipped ground-zero world");
        World::parse(&json).expect("parses")
    }

    fn shipped_span() -> (ArcSpan, SiteSpec, Vec<materials::Material>) {
        let w = shipped_ground_zero();
        let spec = SiteSpec::from_ground_world(&w).expect("spec");
        let mats = materials::load();
        let theta = crate::resolution::ResolutionController::default().angular_resolution;
        let q = spec.finest_child_extent_m(&mats).expect("finest quantum");
        let threshold = site::view_resolution_distance(spec.declared_coarse_extent_m(), theta);
        let sep = 3.844e8; // the shipped world's declared Earth–Luna separation (m)
        let span = ArcSpan::derive(
            q,
            theta,
            WHOLE_ORBIT_MARGIN * sep,
            threshold,
            w.time.as_ref().map(|t| t.scale).unwrap_or(1.0),
            w.arc.as_ref().map(|a| a.octave_s).expect("the shipped world declares arc pacing"),
        );
        (span, spec, mats)
    }

    /// **The span is derived end to end.** The floor is the finest materialized quantum's
    /// view-resolution distance (pinned against what `materialize_site` actually builds), and
    /// deep enough that the absolute-f32 render path stays sub-pixel there; the top clears the
    /// fold threshold, so one pull-out folds the site and one descent re-materializes it.
    #[test]
    fn the_span_derives_floor_from_the_finest_quantum_and_top_clears_the_fold() {
        let (span, spec, mats) = shipped_span();
        let theta = crate::resolution::ResolutionController::default().angular_resolution;

        // The finest declared matter is the ball's one-rung child: m_ball/13 of iron.
        // Hand-computed: rho 7,870 kg/m³, r 2 m → m_ball = 2.64e5 kg; child 2.03e4 kg → 1.37 m.
        let q = spec.finest_child_extent_m(&mats).expect("ball world has a quantum");
        assert!((q - 1.37).abs() < 0.02, "hand-computed child quantum, got {q:.3}");
        assert!(
            (span.d_floor_m - q / theta).abs() < 1.0e-9,
            "floor = quantum / angular budget"
        );

        // Pinned against the built site: the floor's quantum IS the finest particle the
        // materialization produces (one law, not a parallel estimate).
        let built = site::materialize_site(&spec, &site::HandDown::Declared, &mats).expect("site");
        let min_extent = built.particles[built.fine_start..]
            .iter()
            .map(|p| site::coarse_particle_extent_m(p.mass as f64, p.rho as f64))
            .fold(f64::INFINITY, f64::min);
        assert!(
            (q - min_extent).abs() <= q * 1.0e-3,
            "the derived quantum matches the built site's finest particle: {q:.4} vs {min_extent:.4}"
        );

        // The top clears the fold threshold with the scene's own framing margin, so the
        // trigger fires in BOTH directions along one arc.
        let threshold = site::view_resolution_distance(spec.declared_coarse_extent_m(), theta);
        assert!(span.d_top_m >= WHOLE_ORBIT_MARGIN * threshold, "top clears the fold");
        assert!(span.d_top_m > threshold && span.d_floor_m < threshold, "the arc spans the crossing");

        // The honest precision statement (module doc): at the floor, one f32 ULP at planet
        // radius subtends under a pixel (0.9 rad over ~900 px), which is what licenses the
        // absolute-f32 render path down to (and only down to) this floor.
        let ulp_m = spec.body_radius_m * (f32::EPSILON as f64);
        assert!(
            ulp_m / span.d_floor_m < 0.9 / 900.0,
            "f32 ULP at planet radius must stay sub-pixel at the floor: {:.2e} rad",
            ulp_m / span.d_floor_m
        );
    }

    /// **The pacing law**: declared scale at the top, real time at the floor, compression
    /// proportional to distance between (constant apparent angular rate), continuous at the
    /// knee, and the reciprocal crust weight is exactly 1/S everywhere.
    #[test]
    fn the_pacing_compresses_time_in_proportion_to_camera_distance() {
        let (span, _, _) = shipped_span();
        assert_eq!(span.time_scale(span.d_top_m), span.declared_scale, "declared at the top");
        assert_eq!(span.time_scale(span.d_floor_m), 1.0, "real time at the floor");

        // Proportional in the unclamped region: S/d is one constant (the apparent-rate law).
        let (d1, d2) = (span.d_top_m / 3.0, span.d_top_m / 17.0);
        let k1 = span.time_scale(d1) / d1;
        let k2 = span.time_scale(d2) / d2;
        assert!((k1 - k2).abs() <= k1 * 1.0e-12, "S ∝ d holds: {k1:.6e} vs {k2:.6e}");

        // Continuous at the knee d_top/S_decl, clamped to real time below it.
        let knee = span.d_top_m / span.declared_scale;
        assert!((span.time_scale(knee) - 1.0).abs() < 1.0e-9, "S(knee) = 1");
        assert_eq!(span.time_scale(knee * 0.5), 1.0, "real time below the knee");
        assert!(span.time_scale(knee * 1.01) < 1.02, "no jump above the knee");

        // Monotone non-decreasing with distance, and w·S = 1 at every altitude.
        let mut prev = 0.0;
        for i in 0..60 {
            let d = span.d_floor_m * (span.d_top_m / span.d_floor_m).powf(i as f64 / 59.0);
            let s = span.time_scale(d);
            assert!(s >= prev, "monotone");
            prev = s;
            assert!((span.crust_weight(d) * s - 1.0).abs() < 1.0e-12, "w = 1/S");
        }
    }

    /// **The whole pose chain is continuous, both directions.** Simulate full descend and ascend
    /// legs over a spinning crust at the compressed rate; every frame's eye step and view-ray
    /// turn stay inside the bounds the glide law implies: no cut anywhere on the path.
    #[test]
    fn the_arc_pose_is_continuous_surface_to_celestial_and_back() {
        let (span, spec, _) = shipped_span();
        let r = spec.body_radius_m;
        let axis = DVec3::Y;
        let site_body = crate::geo::dir_from_lat_lon(spec.lat_deg, spec.lon_deg);
        let dt = 1.0 / 240.0;
        let tau = span.tau_s();

        // Per-step bounds, from the laws themselves: radial travel d·dt/τ; the approach pan
        // spends at most α over ln(d0/r) e-folds; every crust-anchored term drifts ≤ Ω (the
        // reciprocal rule). 3× headroom for the compounding of the three.
        let step_bound = |d: f64, alpha0: f64, d0: f64| {
            let pan = if d0 > r { alpha0 / (tau * (d0 / r).ln()) } else { 0.0 };
            3.0 * dt * (d / tau + (r + d) * (pan + OMEGA))
        };

        for descending in [true, false] {
            let mut theta: f64 = 0.0;
            let mut d = if descending { span.d_top_m } else { span.d_floor_m };
            // Start the descent 2.6 rad of azimuth away from the site: close to the worst case.
            let from = if descending {
                slerp_dir(site_body, -site_body + DVec3::new(0.1, 0.2, 0.0), 2.6 / std::f64::consts::PI)
            } else {
                site_body
            };
            let leg = Leg { from_dir: from, d_start_m: d };
            let site_now = |th: f64| DQuat::from_axis_angle(axis, th) * site_body;
            let alpha0 = leg.from_dir.dot(site_now(0.0)).clamp(-1.0, 1.0).acos();

            let pose = |d: f64, th: f64| {
                let s_dir = site_now(th);
                let v = if descending {
                    descend_dir(&span, &leg, d, s_dir, axis, OMEGA, r)
                } else {
                    ascend_dir(&span, &leg, d, s_dir)
                };
                let e = eye(v, r, d);
                let t = look_target(&span, d, s_dir * r);
                (e, (t - e).normalize())
            };

            let (mut prev_eye, mut prev_ray) = pose(d, theta);
            let mut steps = 0usize;
            loop {
                theta += OMEGA * span.time_scale(d) * dt;
                d = span.glide(d, dt, descending);
                let (e, ray) = pose(d, theta);
                assert!(
                    (e - prev_eye).length() <= step_bound(d.max(prev_eye.length() - r), alpha0, leg.d_start_m),
                    "eye step {} m at d {:.3e} exceeds the glide bound (descending={descending})",
                    (e - prev_eye).length(),
                    d
                );
                // The view ray never snaps: ≤ 4° in any 1/240 s frame, everywhere on the arc.
                let turn = prev_ray.dot(ray).clamp(-1.0, 1.0).acos();
                assert!(
                    turn < 0.07,
                    "view ray turned {:.3} rad in one frame at d {:.3e} (descending={descending})",
                    turn,
                    d
                );
                // The eye can never be inside the planet, by construction: checked anyway.
                assert!(e.length() >= r, "eye inside the planet at d {d:.3e}");
                prev_eye = e;
                prev_ray = ray;
                steps += 1;
                if (descending && d <= span.d_floor_m) || (!descending && d >= span.d_top_m) {
                    break;
                }
                assert!(steps < 200_000, "glide must terminate");
            }
        }
    }

    /// **The spin lead lands the camera over the site.** θ + Θ_rem is a constant of the glide
    /// (checked mid-flight), so integrating the descent over the compressed crust ends with the
    /// camera's direction on the site's: the crust rotates the site into place underneath.
    #[test]
    fn the_descent_lead_lands_the_camera_over_the_site() {
        let (span, spec, _) = shipped_span();
        let r = spec.body_radius_m;
        let axis = DVec3::Y;
        let site_body = crate::geo::dir_from_lat_lon(spec.lat_deg, spec.lon_deg);
        let dt = 1.0 / 240.0;

        for start_azimuth in [0.4, 1.7, 3.0] {
            let mut theta: f64 = start_azimuth; // crust orientation at leg start: arbitrary
            let mut d = span.d_top_m * 0.41; // the demo starts inside the top, too
            let from = DQuat::from_axis_angle(axis, 1.9) * site_body;
            let leg = Leg { from_dir: from, d_start_m: d };

            // The conserved quantity, sampled at the start and mid-glide.
            let invariant = |d: f64, th: f64| th + span.lead_angle_rad(d, OMEGA);
            let c0 = invariant(d, theta);
            let mut checked_mid = false;

            loop {
                theta += OMEGA * span.time_scale(d) * dt;
                d = span.glide(d, dt, true);
                if !checked_mid && d < leg.d_start_m / 100.0 {
                    // The continuum invariant is exact (module doc: dθ/dt + dΘ_rem/dt = 0); what
                    // this discrete check accrues is the forward-Euler error of the TEST's own θ
                    // integration, first-order bounded by ω·dt·S_start. Assert inside that bound,
                    // and that it is far under the swing the lead is compensating (~Ω·τ·S₀).
                    let euler_bound = OMEGA * dt * span.time_scale(leg.d_start_m);
                    let compensated = span.lead_angle_rad(leg.d_start_m, OMEGA);
                    let drift = (invariant(d, theta) - c0).abs();
                    assert!(
                        drift < euler_bound && drift < compensated * 1.0e-2,
                        "θ + Θ_rem must be conserved along the glide: drifted {drift:.2e} rad \
                         (Euler bound {euler_bound:.2e}, compensating {compensated:.2e})"
                    );
                    checked_mid = true;
                }
                if d <= span.d_floor_m {
                    break;
                }
            }
            let site_final = DQuat::from_axis_angle(axis, theta) * site_body;
            let v = descend_dir(&span, &leg, d, site_final, axis, OMEGA, r);
            let miss = v.dot(site_final).clamp(-1.0, 1.0).acos();
            assert!(
                miss < 5.0e-3,
                "descent from azimuth {start_azimuth} must land over the site, missed by {miss:.2e} rad"
            );
        }
    }

    /// **The approach pan is spent above the planet's own radius and the final approach is
    /// overhead.** ψ starts at the full offset, decays log-uniformly, reaches zero at d = r,
    /// stays zero below, and never increases on the way down.
    #[test]
    fn the_approach_pan_ends_at_the_planet_and_descends_overhead() {
        let (span, spec, _) = shipped_span();
        let r = spec.body_radius_m;
        let alpha = 2.4;
        let d0 = span.d_top_m;
        assert!((approach_angle(alpha, d0, d0, r) - alpha).abs() < 1.0e-12, "full offset at start");
        assert!(approach_angle(alpha, r, d0, r) == 0.0, "zero at one planet radius");
        assert!(approach_angle(alpha, span.d_floor_m, d0, r) == 0.0, "overhead below it");
        let mut prev = alpha + 1.0e-9;
        for i in 0..200 {
            let d = d0 * (span.d_floor_m / d0).powf(i as f64 / 199.0);
            let psi = approach_angle(alpha, d, d0, r);
            assert!(psi <= prev + 1.0e-12, "ψ never increases on the way down");
            assert!((0.0..=alpha).contains(&psi));
            prev = psi;
        }
        // Continuity at the r-crossing: just above r, ψ is already tiny.
        assert!(approach_angle(alpha, r * 1.001, d0, r) < 5.0e-3);

        // slerp_dir: endpoints exact, unit throughout, antipodal pairs take a real path.
        let a = DVec3::new(0.3, -0.8, 0.52).normalize();
        let b = DVec3::new(-0.7, 0.1, 0.4).normalize();
        assert!((slerp_dir(a, b, 0.0) - a).length() < 1.0e-12);
        assert!((slerp_dir(a, b, 1.0) - b).length() < 1.0e-9);
        for t in [0.25, 0.5, 0.75] {
            assert!((slerp_dir(a, b, t).length() - 1.0).abs() < 1.0e-12, "unit mid-path");
            assert!((slerp_dir(a, -a, t).length() - 1.0).abs() < 1.0e-9, "antipodal stays unit");
        }
    }

    /// **The shipped world declares the arc's pacing**: the one declared number, in the world
    /// file where declared pacing lives (like `time.scale`), not a constant in code.
    #[test]
    fn the_shipped_world_declares_the_arc_pacing() {
        let w = shipped_ground_zero();
        let arc = w.arc.as_ref().expect("ground-zero declares the demo arc");
        assert!(arc.octave_s > 0.0, "a real pacing: {} s/octave", arc.octave_s);
    }
}
