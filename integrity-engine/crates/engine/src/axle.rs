//! The AXLE — a revolute constraint between a wheel and the thing it is bolted to (`docs/47` §3).
//!
//! **The problem.** There is no orientation, angular velocity or inertia tensor anywhere in this engine:
//! [`crate::orbit::Body`] is `{pos, vel, mass}` and nothing else. That looks like a blocker for a
//! vehicle, and it is not. A bonded cloud of particles *already* rotates — apply a force couple to it and
//! angular momentum is carried by the particles' own linear momenta, exactly as the planetary spin
//! bookkeeping assumes. **Torque emerges from forces; we do not add a rotational degree of freedom.** A
//! rigid-body wheel with its own angular DOF would be the charter violation — a second answer to "how
//! does matter rotate" (`docs/46`).
//!
//! What genuinely does not exist is the **joint**: something that holds a wheel's hub at a fixed offset
//! from the chassis while leaving rotation about ONE axis free. Bond the wheel on and it cannot turn;
//! leave it unbonded and it falls off. [`crate::aggregate::Bond`] is a distance spring, which is the
//! wrong shape for this: a spring *penalises* violation, storing energy it must later give back, and a
//! penalty joint stiff enough to hold a wheel on is also stiff enough to launch it. That is the exact
//! failure the terrain settling-storm was, and the reason it went away was moving from a penalty to a
//! constraint.
//!
//! **So the axle resolves, like [`crate::granular::terrain_contact_resolve`] does.** Per substep it
//! removes only the motion that violates the joint and touches nothing else:
//!
//! 1. **Position** — a velocity-decoupled translation putting the hub back on its anchor. Writes no
//!    velocity, so it injects zero kinetic energy however far the chassis moved.
//! 2. **Linear velocity** — the wheel's centre-of-mass velocity is set to the anchor's. This is a
//!    momentum TRANSFER, not a creation: the impulse is reported so the caller applies the equal and
//!    opposite one to the chassis.
//! 3. **Angular** — the wheel's best-fit angular velocity about the hub is found, split into the
//!    component along the axle axis (**preserved exactly** — an axle must not brake a spinning wheel)
//!    and everything else (wobble, removed and reported as an angular impulse for the chassis).
//!
//! **What it deliberately does NOT do: rigidify the wheel.** Only the best-fit rigid rotation is
//! touched. Deformation — the contact patch spreading and rutting, which is the whole reason a tyre is
//! made of rubber — passes through untouched. The wheel stays matter.

use glam::{DMat3, DVec3};

use crate::orbit::Body;

/// What the axle did this substep, in the currency the chassis needs to stay in balance.
///
/// Both are what the axle applied **to the wheel**; Newton's third law means the chassis receives the
/// negatives. A caller that drops these has an axle that creates momentum out of nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxleReaction {
    /// Linear impulse applied to the wheel (kg·m/s).
    pub impulse: DVec3,
    /// Angular impulse applied to the wheel about the hub (kg·m²/s) — the wobble the axle refused.
    pub angular_impulse: DVec3,
}

/// The wheel's best-fit angular velocity about `anchor` (rad/s), from the particles' linear momenta
/// alone: `ω = I⁻¹ L`.
///
/// This is the mass-weighted least-squares rotation — the single rigid spin that best explains the
/// cloud's motion — which is what makes [`resolve`] provably non-injecting: subtracting a
/// least-squares projection can only ever reduce the residual, never grow it. Returns `None` when the
/// inertia tensor is singular (a single particle, or a perfectly collinear cloud), where "the angular
/// velocity" is not defined and the honest answer is to leave the velocities alone.
pub fn angular_velocity(cloud: &[Body], anchor: DVec3, anchor_vel: DVec3) -> Option<DVec3> {
    let mut l = DVec3::ZERO;
    let mut inertia = DMat3::ZERO;
    for p in cloud {
        let r = p.pos - anchor;
        let u = p.vel - anchor_vel;
        l += p.mass * r.cross(u);
        let r2 = r.length_squared();
        inertia += p.mass
            * (DMat3::from_diagonal(DVec3::splat(r2))
                - DMat3::from_cols(r * r.x, r * r.y, r * r.z));
    }
    // A cloud with no spatial extent perpendicular to some axis has no inertia about it; inverting
    // would produce an infinite spin from rounding noise.
    if inertia.determinant().abs() < 1.0e-12 {
        return None;
    }
    Some(inertia.inverse() * l)
}

/// **Resolve one axle for one substep.** `wheel` is the wheel's particles, `anchor`/`anchor_vel` the hub
/// point the chassis presents (already in world coordinates — the caller owns the body-relative offset),
/// and `axis` the direction the wheel is free to turn about (need not be normalised).
///
/// Returns the [`AxleReaction`] the caller must apply, negated, to the chassis.
pub fn resolve(wheel: &mut [Body], anchor: DVec3, anchor_vel: DVec3, axis: DVec3) -> AxleReaction {
    let m: f64 = wheel.iter().map(|p| p.mass).sum();
    if m <= 0.0 || wheel.is_empty() {
        return AxleReaction::default();
    }
    let n = match axis.try_normalize() {
        Some(n) => n,
        None => return AxleReaction::default(), // no axis ⇒ no joint to enforce
    };

    // 1. POSITION — put the hub back on its anchor. Velocity-decoupled: this writes no velocity, so
    //    however far the chassis has travelled this substep, the wheel gains no kinetic energy from
    //    being carried along. (The same discipline as the terrain contact's position projection.)
    let com: DVec3 = wheel.iter().map(|p| p.mass * p.pos).sum::<DVec3>() / m;
    let shift = anchor - com;
    if shift != DVec3::ZERO {
        for p in wheel.iter_mut() {
            p.pos += shift;
        }
    }

    // 2. LINEAR VELOCITY — the hub travels with the chassis. Reported as an impulse, because that is
    //    exactly what it is: momentum handed across the joint, not conjured at it.
    let v_com: DVec3 = wheel.iter().map(|p| p.mass * p.vel).sum::<DVec3>() / m;
    let dv = anchor_vel - v_com;
    if dv != DVec3::ZERO {
        for p in wheel.iter_mut() {
            p.vel += dv;
        }
    }
    let mut reaction = AxleReaction { impulse: m * dv, angular_impulse: DVec3::ZERO };

    // 3. ANGULAR — keep the spin the axle exists to allow, refuse the rest.
    let Some(omega) = angular_velocity(wheel, anchor, anchor_vel) else {
        return reaction; // degenerate cloud: no defined spin to split
    };
    let kill = omega - n * omega.dot(n); // everything off-axis: wobble, tumble, precession
    if kill == DVec3::ZERO {
        return reaction;
    }
    // The angular momentum we are about to take out of the wheel — computed BEFORE the removal, since
    // afterwards it is by construction gone.
    let mut removed = DVec3::ZERO;
    for p in wheel.iter() {
        let r = p.pos - anchor;
        removed += p.mass * r.cross(kill.cross(r));
    }
    for p in wheel.iter_mut() {
        let r = p.pos - anchor;
        p.vel -= kill.cross(r);
    }
    reaction.angular_impulse = -removed;
    reaction
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a wheel: a ring of `n` equal particles of total mass `mass`, radius `radius`, centred on
    /// `hub`, lying in the plane normal to `axis`, spinning at `omega` rad/s about `axis`.
    fn wheel(n: usize, radius: f64, mass: f64, hub: DVec3, axis: DVec3, omega: f64) -> Vec<Body> {
        let n_hat = axis.normalize();
        // Any two unit vectors spanning the wheel's plane.
        let e1 = if n_hat.x.abs() < 0.9 { DVec3::X } else { DVec3::Y }.cross(n_hat).normalize();
        let e2 = n_hat.cross(e1);
        (0..n)
            .map(|i| {
                let t = i as f64 / n as f64 * std::f64::consts::TAU;
                let r = (e1 * t.cos() + e2 * t.sin()) * radius;
                Body { pos: hub + r, vel: (n_hat * omega).cross(r), mass: mass / n as f64 }
            })
            .collect()
    }

    fn kinetic(cloud: &[Body]) -> f64 {
        cloud.iter().map(|p| 0.5 * p.mass * p.vel.length_squared()).sum()
    }

    /// **THE test an axle has to pass: it must not brake the wheel.** A wheel already spinning freely
    /// about its own axle, centred on its anchor, is in perfect compliance with the constraint — so
    /// resolving it must change nothing at all. A joint that quietly bleeds spin here would look like
    /// "bearing friction" while being a numerical artifact, and would be indistinguishable from the
    /// DECLARED bearing-friction model docs/47 §4 says we owe an honest derivation for.
    #[test]
    fn a_freely_spinning_wheel_is_left_completely_alone() {
        let hub = DVec3::new(1.0, 2.0, 3.0);
        let axis = DVec3::new(0.0, 0.0, 1.0);
        let mut w = wheel(24, 0.14, 8.0, hub, axis, 37.0);
        let before = w.clone();
        let ke = kinetic(&w);
        let r = resolve(&mut w, hub, DVec3::ZERO, axis);
        for (a, b) in w.iter().zip(before.iter()) {
            assert!((a.pos - b.pos).length() < 1.0e-12, "the axle moved a compliant wheel");
            assert!((a.vel - b.vel).length() < 1.0e-9, "the axle changed a compliant wheel's motion");
        }
        assert!((kinetic(&w) - ke).abs() < 1.0e-9, "the axle bled energy from a free spin");
        assert!(r.impulse.length() < 1.0e-12 && r.angular_impulse.length() < 1.0e-9);
        // And the spin it was given is the spin it reports: ω recovered from linear momenta alone.
        let omega = angular_velocity(&w, hub, DVec3::ZERO).unwrap();
        assert!((omega - axis.normalize() * 37.0).length() < 1.0e-9, "got {omega:?}");
    }

    /// The joint holds the hub on its anchor — and does it WITHOUT injecting energy. A penalty spring
    /// would store the displacement and hand it back as launch; the position projection writes no
    /// velocity at all, so a wheel dragged 20 cm off its mount is replaced, not fired.
    #[test]
    fn the_hub_is_pulled_back_to_its_anchor_injecting_no_energy() {
        let hub = DVec3::new(0.0, 0.5, 0.0);
        let axis = DVec3::Y;
        let mut w = wheel(16, 0.14, 8.0, hub + DVec3::new(0.2, -0.05, 0.1), axis, 12.0);
        let ke = kinetic(&w);
        resolve(&mut w, hub, DVec3::ZERO, axis);
        let m: f64 = w.iter().map(|p| p.mass).sum();
        let com: DVec3 = w.iter().map(|p| p.mass * p.pos).sum::<DVec3>() / m;
        assert!((com - hub).length() < 1.0e-12, "the hub did not return to its anchor");
        assert!(
            kinetic(&w) <= ke + 1.0e-9,
            "re-seating the hub INJECTED energy ({} → {}) — that is a penalty spring, not a constraint",
            ke,
            kinetic(&w)
        );
    }

    /// A revolute joint frees exactly ONE axis. Spin about the axle survives untouched; spin about any
    /// other axis is refused — and the angular momentum refused is handed back as a reaction, because an
    /// axle that simply deleted it would be a torque source with nothing pushing against it.
    #[test]
    fn wobble_is_refused_the_axle_spin_survives_and_the_reaction_is_reported() {
        let hub = DVec3::ZERO;
        let axis = DVec3::Z;
        let mut w = wheel(32, 0.14, 8.0, hub, axis, 25.0);
        // Add a tumble about X — the wheel trying to fall over sideways on its mount.
        let tumble = DVec3::new(9.0, 0.0, 0.0);
        for p in w.iter_mut() {
            p.vel += tumble.cross(p.pos - hub);
        }
        let ke = kinetic(&w);
        let before = angular_velocity(&w, hub, DVec3::ZERO).unwrap();
        assert!((before.x - 9.0).abs() < 1.0e-9 && (before.z - 25.0).abs() < 1.0e-9);

        let r = resolve(&mut w, hub, DVec3::ZERO, axis);

        let after = angular_velocity(&w, hub, DVec3::ZERO).unwrap();
        assert!((after.z - 25.0).abs() < 1.0e-9, "the axle braked its own free axis: {after:?}");
        assert!((after.length() - 25.0).abs() < 1.0e-6, "wobble survived the constraint: {after:?}");
        assert!(kinetic(&w) < ke, "refusing wobble must dissipate from the wheel, not add");
        // The reaction is the angular momentum the wheel lost, so the chassis can receive it.
        assert!(r.angular_impulse.x < 0.0, "the wobble reaction opposes the wobble: {r:?}");
        assert!(
            r.angular_impulse.z.abs() < 1.0e-6,
            "the axle must exert no reaction about its own free axis: {r:?}"
        );
    }

    /// Non-injection, stated as the general property rather than per-scenario: against a STATIC anchor
    /// the axle is a pure constraint and can only ever remove energy. (With a moving anchor it also
    /// transfers, which is what `impulse` accounts for — so the invariant is checked where it is
    /// unambiguous.)
    #[test]
    fn the_axle_never_increases_energy() {
        let hub = DVec3::new(-0.3, 1.1, 0.2);
        let axis = DVec3::new(0.3, 0.9, -0.2);
        for (k, spin) in [(1usize, 0.0), (2, 5.0), (3, -18.0), (4, 60.0)] {
            let mut w = wheel(20, 0.14, 8.0, hub, axis, spin);
            // Perturb every particle deterministically: drift, tumble and pure deformation at once.
            for (i, p) in w.iter_mut().enumerate() {
                let s = ((i * 7 + k * 13) % 11) as f64 / 11.0 - 0.5;
                p.pos += DVec3::new(s, -s, 0.5 * s) * 0.05;
                p.vel += DVec3::new(-s, 0.7 * s, s) * 3.0;
            }
            let ke = kinetic(&w);
            resolve(&mut w, hub, DVec3::ZERO, axis);
            assert!(
                kinetic(&w) <= ke + 1.0e-9,
                "case {k}: energy rose {ke} → {} across a constraint",
                kinetic(&w)
            );
        }
    }

    /// **A wheel spins because forces spin it** (`docs/47` §3) — the claim that lets the engine skip
    /// rotational DOFs entirely. Apply a force COUPLE (equal and opposite tangential forces on opposite
    /// rims), integrate as ordinary linear motion, and angular momentum appears in the particles' linear
    /// momenta. The axle must let that through: it is the drive torque.
    #[test]
    fn a_force_couple_spins_the_wheel_and_the_axle_lets_it() {
        let hub = DVec3::ZERO;
        let axis = DVec3::Z;
        let mut w = wheel(4, 0.14, 8.0, hub, axis, 0.0);
        assert!(angular_velocity(&w, hub, DVec3::ZERO).unwrap().length() < 1.0e-12, "starts at rest");

        // One substep of a couple: +F tangential at particle 0, −F at the opposite rim particle 2.
        let dt = 1.0e-3;
        let f = 50.0;
        // A COUPLE: the same tangential force at diametrically opposite rim points. The tangent flips
        // with position, so the two forces cancel (zero net force on the wheel) while their torques add
        // — pure rotation, nothing pushing the hub sideways into its own constraint.
        let tangent = |p: &Body| axis.normalize().cross((p.pos - hub).normalize());
        let (t0, m0) = (tangent(&w[0]), w[0].mass);
        let (t2, m2) = (tangent(&w[2]), w[2].mass);
        assert!((t0 + t2).length() < 1.0e-12, "particles 0 and 2 must be diametrically opposite");
        w[0].vel += t0 * (f / m0 * dt);
        w[2].vel += t2 * (f / m2 * dt);

        let omega = angular_velocity(&w, hub, DVec3::ZERO).unwrap();
        assert!(omega.z > 0.0, "the couple produced no spin about the axle: {omega:?}");
        assert!(
            omega.truncate().length() < 1.0e-9,
            "a pure couple about the axle produced off-axis spin: {omega:?}"
        );
        // ...and the axle passes it through untouched, because that is the axis it frees.
        let r = resolve(&mut w, hub, DVec3::ZERO, axis);
        let after = angular_velocity(&w, hub, DVec3::ZERO).unwrap();
        assert!((after.z - omega.z).abs() < 1.0e-9, "the axle ate the drive torque");
        assert!(r.angular_impulse.length() < 1.0e-9, "and had nothing to refuse");
    }
}
