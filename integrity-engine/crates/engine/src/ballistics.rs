//! **A confined gas doing work on a moving boundary** — the declared layer (docs/46 row 33).
//!
//! A gun is a piston. So is a champagne cork, a pneumatic ram and a volcanic conduit: hot gas in a
//! closed volume, one wall free to move, work done as it goes. Nothing here is gun-specific except the
//! names of its first consumers.
//!
//! ## This is the DECLARED layer, and it knows it
//!
//! Robin (2026-08-03): *"the walls of the cannon will shape the trajectory of the gasses, but the gasses
//! will impact them, rebound, impact the ball, etc. Once again one engine, one principle."* The
//! RESOLVED answer is gas as matter, going through the same contact law as everything else. This module
//! is the cheap closed form of that — docs/44's declared/resolved duality, where the declared one must
//! be derived from and convergent to the resolved one, so a gun firing off-camera costs almost nothing.
//!
//! ★★ **And it carries a validity predicate, because outside its assumptions it does not become
//! inaccurate — it answers a different question.** Robin: *"a ball slightly too large, a charge too
//! strong, should be able to destroy a cannon as history shows it did."* A model that treats the barrel
//! as a fixed boundary will happily report a muzzle velocity for a gun that should have burst, which is
//! worse than being wrong: nothing downstream can tell. So [`fire`] checks containment and shot-start
//! before it reports a velocity, and says [`Outcome::Burst`] when the arithmetic says so.
//!
//! ★ **The verdict is arithmetic, not a simulation** (Robin): *"if it all checks out we hand the easy 1d
//! to the renderer, if physics predicts catastrophy, we share that with the renderer. No need to
//! actually render the matter particles, so should be a fast calculation."* Deciding WHETHER a gun
//! bursts does not require simulating the burst. Matter is resolved only for a burst that IS happening,
//! to show how it comes apart.
//!
//! ## Noble-Abel, which GENERALISES the ideal gas law rather than competing with it
//!
//! At a hundred megapascals the gas molecules' own volume is not negligible, so `p(V - b·m) = m·R·T`
//! with `b` the covolume — about 1 cm³/g for propellant gases. **Covolume zero reduces exactly to the
//! ideal gas law**, which is what `atmosphere.rs` already uses, so this is one law with a term the
//! thin-air case sets to nothing (Law II).

use crate::oxidation::Burn;

/// The universal gas constant, J/(mol·K).
pub const R_GAS: f64 = 8.314462618;

/// What happened when the gun was fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The shot left the muzzle and the gun survived.
    Fired,
    /// Peak pressure exceeded what the barrel can hold. **The velocity field is meaningless** — the
    /// declared model is outside its validity and is reporting that fact instead of a number.
    Burst,
    /// Pressure never overcame the shot's resistance. The charge burned and the ball stayed put, which
    /// is a real thing that happens with a damp charge or a jammed ball.
    Squib,
}

/// A gun's geometry and its limits, all of which come from an assembly rather than being declared here.
#[derive(Clone, Copy, Debug)]
pub struct Bore {
    /// Bore radius, m.
    pub r_m: f64,
    /// Chamber volume behind the shot at rest, m³ — **the charge's ENVELOPE**, not its matter volume.
    /// Getting that wrong by the powder's porosity halves the volume and doubles the pressure.
    pub chamber_m3: f64,
    /// How far the shot travels before it leaves, m.
    pub travel_m: f64,
    /// Barrel wall thickness at the chamber, m — where the pressure is highest.
    pub wall_m: f64,
    /// The barrel material's tensile strength, Pa. Hoop stress is tensile, which is the direction grey
    /// cast iron is weakest in.
    pub wall_strength_pa: f64,
    /// Pressure needed to start the shot moving, Pa — from the wadding and the ball's fit.
    pub shot_start_pa: f64,
}

impl Bore {
    /// Cross-sectional area of the bore, m² — the piston face the gas pushes on.
    pub fn area_m2(&self) -> f64 {
        std::f64::consts::PI * self.r_m * self.r_m
    }
}

/// The result of firing.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub outcome: Outcome,
    /// Peak chamber pressure, Pa.
    pub peak_pressure_pa: f64,
    /// Peak hoop stress in the barrel wall, Pa.
    pub peak_hoop_pa: f64,
    /// Muzzle velocity, m/s. Zero unless [`Outcome::Fired`].
    pub muzzle_ms: f64,
    /// Work done on the shot, J.
    pub work_j: f64,
    /// Recoil velocity of the gun, m/s (positive = backwards).
    pub recoil_ms: f64,
    /// Gas temperature at the instant of ignition, K.
    pub flame_k: f64,
}

/// The temperature the released energy WOULD bring the products to if every joule went into the
/// permanent gas and none into the condensed residue.
///
/// Not used by [`fire`] — kept because **the gap between this and a measured flame temperature IS the
/// size of the energy split being deferred**, and a deferred computation you cannot size is a hope
/// rather than an IOU. For black powder it gives 6738 K against a sourced 1950 K.
pub fn naive_flame_k(burn: &Burn, gamma: f64) -> f64 {
    if burn.gas_moles <= 0.0 || gamma <= 1.0 {
        return 0.0;
    }
    burn.energy_j * (gamma - 1.0) / (burn.gas_moles * R_GAS)
}

/// **Fire.** `burn` is what the charge released, `bore` the gun, `shot_kg` the projectile, `gun_kg` the
/// recoiling mass, `gas_kg` the mass of gas produced, `gamma` the products' specific-heat ratio, and
/// `covolume` their Noble-Abel covolume in m³/kg.
///
/// The energy released heats the gas; the gas pushes the shot; the work done is the shot's kinetic
/// energy. Every step is a closed form.
///
/// ★★ **`flame_k` is SUPPLIED, not derived from the burn energy, and finding out why was worth the
/// detour.** Putting all of the released energy into the gas gives 6738 K for a service charge, against
/// a sourced flame temperature of **1950 K** — wrong by a factor of 3.5, and not in a subtle way. The
/// cause is physical: **over half of black powder's product mass is CONDENSED** (potassium carbonate
/// and sulfide, the smoke), and heating those solids takes most of the energy while contributing no
/// pressure. An energy balance that ignores them is not slightly optimistic, it is a different
/// substance. So the sourced value is used and the derivation is the deferred computation (Law V) — the
/// same energy split a hot barrel needs (docs/46 row 32).
///
/// ★★ **And the pressure this returns is the CONSTANT-VOLUME bound**, i.e. the charge burning all at
/// once in the chamber. Real powder burns progressively while the shot is ALREADY MOVING, so the volume
/// at peak pressure is larger and the real peak is lower. That makes the burst predicate CONSERVATIVE:
/// it will condemn guns that history fired safely. Stated because a conservative predicate is only
/// honest if its direction is known — the resolved gas-as-matter path is what removes the assumption.
#[allow(clippy::too_many_arguments)]
pub fn fire(
    burn: &Burn,
    bore: &Bore,
    shot_kg: f64,
    gun_kg: f64,
    gas_kg: f64,
    gamma: f64,
    covolume_m3_kg: f64,
    flame_k: f64,
) -> Shot {
    let null = Shot {
        outcome: Outcome::Squib,
        peak_pressure_pa: 0.0,
        peak_hoop_pa: 0.0,
        muzzle_ms: 0.0,
        work_j: 0.0,
        recoil_ms: 0.0,
        flame_k: 0.0,
    };
    if burn.gas_moles <= 0.0
        || bore.chamber_m3 <= 0.0
        || shot_kg <= 0.0
        || gamma <= 1.0
        || flame_k <= 0.0
    {
        return null;
    }

    // Noble-Abel in the chamber: p (V - b m) = n R T. The covolume is NOT negligible here — for a
    // typical charge it removes a large fraction of the chamber, and ignoring it under-predicts the
    // pressure badly.
    let free = bore.chamber_m3 - covolume_m3_kg * gas_kg;
    if free <= 0.0 {
        // The gas cannot fit in its own chamber even fully compressed. That is not a pressure, it is a
        // detonation, and this model has nothing honest to say about it.
        return Shot {
            outcome: Outcome::Burst,
            peak_pressure_pa: f64::INFINITY,
            peak_hoop_pa: f64::INFINITY,
            flame_k,
            ..null
        };
    }
    let peak = burn.gas_moles * R_GAS * flame_k / free;
    let peak_hoop = crate::assembly::hoop_stress_pa(peak, bore.r_m, bore.wall_m);

    // ★ THE VALIDITY PREDICATE, and it runs BEFORE any velocity is computed. Containment first: a gun
    // that bursts has no muzzle velocity, and reporting one would be answering a different question.
    if peak_hoop >= bore.wall_strength_pa {
        return Shot {
            outcome: Outcome::Burst,
            peak_pressure_pa: peak,
            peak_hoop_pa: peak_hoop,
            flame_k,
            ..null
        };
    }
    // Then: can the shot move at all? A charge that burns without shifting the ball is a squib, and the
    // pressure it reached still has to be reported because that is what tells you it was close.
    if peak < bore.shot_start_pa {
        return Shot {
            outcome: Outcome::Squib,
            peak_pressure_pa: peak,
            peak_hoop_pa: peak_hoop,
            flame_k,
            ..null
        };
    }

    // Adiabatic expansion as the shot travels. With p V_eff^gamma constant, the work from V0 to V1 is
    // (p0 V0eff - p1 V1eff)/(gamma-1), which is the closed form of the integral of p dV — no stepping,
    // so this costs a handful of operations however far the shot travels.
    let v1 = bore.chamber_m3 + bore.area_m2() * bore.travel_m;
    let free1 = v1 - covolume_m3_kg * gas_kg;
    let ratio = (free / free1).powf(gamma - 1.0);
    let work = peak * free / (gamma - 1.0) * (1.0 - ratio);

    // The gas has to be accelerated too — it leaves the muzzle at speed, and ignoring it would credit
    // the shot with energy the gas took. The classic Lagrange approximation puts the gas's effective
    // share at a third of its mass, because it is distributed from a stationary breech to a base moving
    // with the shot. *Flagged*: it is an approximation to a linear velocity profile, and the resolved
    // gas-as-matter path is what would replace it.
    let effective_kg = shot_kg + gas_kg / 3.0;
    let muzzle = (2.0 * work / effective_kg).sqrt();

    // Momentum closes: what leaves forwards, the gun takes backwards. The gas's own momentum counts —
    // its centre of mass leaves at about half the shot's speed on the same linear profile.
    let forward = shot_kg * muzzle + gas_kg * 0.5 * muzzle;
    let recoil = if gun_kg > 0.0 { forward / gun_kg } else { 0.0 };

    Shot {
        outcome: Outcome::Fired,
        peak_pressure_pa: peak,
        peak_hoop_pa: peak_hoop,
        muzzle_ms: muzzle,
        work_j: work,
        recoil_ms: recoil,
        flame_k,
    }
}

/// **A gun placed on a body and aimed** — the exterior half of the same story.
///
/// Robin's acceptance test (docs/64): *"put a working cannon on a working planet and fire it"*, and
/// *"on land pointed to sea"*. So an emplacement is a coordinate, a bearing and an elevation; where the
/// shot lands is not a parameter of it.
#[derive(Clone, Copy, Debug)]
pub struct Emplacement {
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Height of the muzzle above the local surface, m.
    pub height_m: f64,
    /// Compass bearing the gun points along: 0 = north, 90 = east.
    pub bearing_deg: f64,
    /// Elevation above the local horizontal, degrees.
    pub elevation_deg: f64,
}

/// **The launch state of a shot leaving this gun** — position and velocity in the body frame.
///
/// ★ This is GEOMETRY, not physics, and the distinction is the point. Robin: *"shouldn't the engine
/// itself be applying the air resistance? I don't want the scene computing it"*, and *"SCENES must
/// never apply physics."* An emplacement knows where the gun stands and which way it points; what
/// happens to the shot afterwards belongs to [`crate::flight::Flight`], which already flies meteors
/// through the same air with the same drag, heating and ablation, sizing its own substeps against the
/// atmosphere's scale height and integrating quadratic drag in CLOSED FORM rather than by Euler steps.
///
/// A first version of this module carried its own trajectory integrator taking a drag coefficient and
/// an `AirShell` as arguments. That made the CALLER choose the physics, which is the rule above broken,
/// and it was a second answer to a question `flight.rs` had already answered better (Law II). Deleted.
pub fn launch(e: &Emplacement, muzzle_ms: f64, body_radius_m: f64) -> (glam::DVec3, glam::DVec3) {
    // ★ The frame is (up, north, east) — see `geo::tangent_frame`. Destructuring it as
    // (east, north, up) fired the shot SIDEWAYS along the surface and produced a 10,000 km "range",
    // which a test caught. The doc comment had said so plainly.
    let (up, north, east) = crate::geo::tangent_frame(e.lat_deg, e.lon_deg);
    let b = e.bearing_deg.to_radians();
    let el = e.elevation_deg.to_radians();
    // Bearing 0 is north, 90 east — the surveyor's convention, not the mathematician's.
    let horizontal = north * b.cos() + east * b.sin();
    let dir = (horizontal * el.cos() + up * el.sin()).normalize();
    (up * (body_radius_m + e.height_m), dir * muzzle_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::shipped;
    use crate::materials::Material;
    use crate::oxidation;

    /// Sourced: ~1950 K at 1000 psi for classic 75/15/10 black powder (docs/64, black-powder note).
    const FLAME_K: f64 = 1950.0;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// The 24-pounder, assembled from the three SHIPPED assemblies and fired. Every input below comes
    /// from an assembly's geometry or the material catalogue; nothing about the gun is typed here.
    fn twenty_four_pounder() -> (Bore, f64, f64, f64, oxidation::Burn, f64, f64) {
        let mats = mats();
        let gun = shipped::load("naval-24pdr-gun");
        let shot = shipped::load("round-shot-24pdr");
        let charge = shipped::load("charge-24pdr-service");

        let shot_kg = shot.mass_kg(&mats).expect("shot");
        let gun_kg = gun.mass_kg(&mats).expect("gun");

        // Chamber = the charge's ENVELOPE (packing already accounted for), which is the space it fills.
        let chamber = ["saltpetre", "charcoal", "brimstone", "wad"]
            .iter()
            .map(|n| charge.part(n).expect("part").envelope_volume_m3())
            .sum::<f64>();

        // Bore and wall from the barrel itself.
        let crate::assembly::Shape::Tube {
            r_outer, r_bore, ..
        } = gun.part("first_reinforce").expect("reinforce").shape
        else {
            panic!("the reinforce is a tube")
        };
        // Travel: the bored length, less the chamber the charge occupies.
        let bored: f64 = [
            "first_reinforce",
            "second_reinforce",
            "chase",
            "muzzle_swell",
        ]
        .iter()
        .map(|n| match gun.part(n).expect("part").shape {
            crate::assembly::Shape::Tube { length, .. } => length,
            _ => 0.0,
        })
        .sum();
        let area = std::f64::consts::PI * r_bore * r_bore;
        let bore = Bore {
            r_m: r_bore,
            chamber_m3: chamber,
            travel_m: (bored - chamber / area).max(0.0),
            wall_m: r_outer - r_bore,
            wall_strength_pa: mats[crate::materials::index_of(&mats, "gunmetal")].fracture_strength
                as f64,
            // Wadding and the ball's fit. A representative value; the resolved form derives it from
            // the InterferenceFit join's normal pressure and friction.
            shot_start_pa: 15.0e6,
        };

        // Burn the powder, sealed in: no air reaches a charge rammed to the bottom of a bore.
        let powder: Vec<(&Material, f64)> = ["saltpetre", "charcoal", "brimstone"]
            .iter()
            .map(|n| {
                let p = charge.part(n).expect("part");
                let m = &mats[crate::materials::index_of(&mats, &p.material)];
                (m, p.matter_volume_m3() * m.density as f64)
            })
            .collect();
        let burn = oxidation::burn(&powder, 0.0);
        let powder_kg: f64 = powder.iter().map(|(_, k)| k).sum();
        // Gas mass: the fraction of the charge that became permanent gas. The rest is condensed
        // residue and stays in the bore as fouling.
        let gas_kg = burn.gas_moles * 0.0389; // mean product molar mass, from the catalogue
        (bore, shot_kg, gun_kg, gas_kg, burn, powder_kg, 1.2)
    }

    /// **THE 24-POUNDER FIRES**, and the result is reported honestly against the historical figure
    /// rather than tuned to it.
    #[test]
    fn the_shipped_cannon_fires() {
        let (bore, shot_kg, gun_kg, gas_kg, burn, _powder, gamma) = twenty_four_pounder();
        let s = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.001, FLAME_K);
        assert_eq!(
            s.outcome,
            Outcome::Fired,
            "a sound gun with a service charge fires"
        );
        assert!(s.muzzle_ms > 0.0 && s.muzzle_ms.is_finite());
        assert!(s.peak_pressure_pa > bore.shot_start_pa);
        assert!(
            s.peak_hoop_pa < bore.wall_strength_pa,
            "and it survives doing it"
        );
        // A 24-pounder's shot left the muzzle at roughly 450 m/s. This model has TWO known
        // over-predictions stacked on it — the idealised equation over-states gas by ~25% and energy by
        // more (docs/46 row 31, `oxidation`'s own test), and all the burn energy is put into the gas —
        // so the honest expectation is HIGH, and the assertion is a broad plausibility band rather than
        // a match. Tightening this by tuning a coefficient would be fitting the answer.
        assert!(
            (200.0..1500.0).contains(&s.muzzle_ms),
            "a cannon-like muzzle velocity, known to run high: {:.0} m/s against a historical ~450",
            s.muzzle_ms
        );
    }

    /// ★★ **MOMENTUM CLOSES — the exact check that needs no historical figure.** It tests the
    /// bookkeeping rather than the calibration, which is why it is worth having before any velocity is
    /// believed. What leaves the muzzle forwards, the gun takes backwards.
    #[test]
    fn what_goes_forward_comes_back() {
        let (bore, shot_kg, gun_kg, gas_kg, burn, _p, gamma) = twenty_four_pounder();
        let s = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.001, FLAME_K);
        assert_eq!(s.outcome, Outcome::Fired);
        let forward = shot_kg * s.muzzle_ms + gas_kg * 0.5 * s.muzzle_ms;
        let backward = gun_kg * s.recoil_ms;
        assert!(
            (forward - backward).abs() < 1e-9 * forward,
            "momentum must close exactly: {forward} forward vs {backward} back"
        );
        // A two-and-a-half tonne gun recoils slowly against a ten-kilo ball — which is why a breeching
        // rope could arrest it at all, and why the crew stood clear rather than in front of it.
        assert!(
            s.recoil_ms < 0.05 * s.muzzle_ms,
            "recoil {:.2} m/s against muzzle {:.0} m/s",
            s.recoil_ms,
            s.muzzle_ms
        );
        assert!(s.recoil_ms > 0.0);
    }

    /// ★★★ **AN OVERCHARGE BURSTS THE GUN, and the model says so instead of reporting a velocity.**
    /// Robin: *"a ball slightly too large, a charge too strong, should be able to destroy a cannon as
    /// history shows it did."* This is the validity predicate doing its job — and note the burst answer
    /// costs exactly the same arithmetic as the firing one. No matter is resolved to find out.
    #[test]
    fn too_much_powder_bursts_the_barrel() {
        let (bore, shot_kg, gun_kg, gas_kg, burn, _p, gamma) = twenty_four_pounder();
        let sound = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.001, FLAME_K);
        assert_eq!(sound.outcome, Outcome::Fired);

        // Find where it gives way by scaling the charge — the burn is linear in charge mass, so
        // multiplying its energy and gas is exactly what a bigger charge does.
        let mut burst_at = None;
        for mult in 2..40 {
            let m = mult as f64 * 0.5;
            let big = oxidation::Burn {
                energy_j: burn.energy_j * m,
                gas_moles: burn.gas_moles * m,
                ..burn
            };
            let r = fire(
                &big,
                &bore,
                shot_kg,
                gun_kg,
                gas_kg * m,
                gamma,
                0.001,
                FLAME_K,
            );
            if r.outcome == Outcome::Burst {
                burst_at = Some((m, r));
                break;
            }
        }
        let (m, r) = burst_at.expect("some overcharge must burst this gun");
        assert!(m > 1.0, "a service charge is not already bursting it");
        assert_eq!(r.muzzle_ms, 0.0, "a burst gun reports NO muzzle velocity");
        assert!(
            r.peak_hoop_pa >= bore.wall_strength_pa,
            "and it says why: hoop stress reached the metal's limit"
        );
    }

    /// A charge that cannot shift the ball is a SQUIB, not a fizzle in the arithmetic — and the
    /// pressure it did reach is still reported, because that is what tells you how close it came.
    #[test]
    fn a_charge_too_weak_to_move_the_ball_is_a_squib() {
        let (mut bore, shot_kg, gun_kg, gas_kg, burn, _p, gamma) = twenty_four_pounder();
        bore.shot_start_pa = 1.0e12; // a ball welded in place
        let s = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.001, FLAME_K);
        assert_eq!(s.outcome, Outcome::Squib);
        assert_eq!(s.muzzle_ms, 0.0);
        assert!(s.peak_pressure_pa > 0.0, "the powder still burned");
    }

    /// **Noble-Abel generalises the ideal gas law rather than competing with it**: covolume zero must
    /// reduce to `pV = nRT` exactly, which is what `atmosphere.rs` already uses. One law with a term the
    /// thin-air case sets to nothing (Law II).
    #[test]
    fn zero_covolume_is_the_ideal_gas_law() {
        let (bore, shot_kg, gun_kg, gas_kg, burn, _p, gamma) = twenty_four_pounder();
        let ideal = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.0, FLAME_K);
        let want = burn.gas_moles * R_GAS * ideal.flame_k / bore.chamber_m3;
        assert!(
            (ideal.peak_pressure_pa - want).abs() < 1e-6 * want,
            "with no covolume the chamber pressure is exactly nRT/V"
        );
        // And the covolume matters: it raises the pressure, because the gas has less room than the
        // chamber suggests. At a hundred megapascals that is not a rounding correction.
        let real = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.001, FLAME_K);
        assert!(
            real.peak_pressure_pa > ideal.peak_pressure_pa,
            "covolume raises pressure: {:e} vs {:e}",
            real.peak_pressure_pa,
            ideal.peak_pressure_pa
        );
    }

    /// ★★★ **THE CANNON ON EARTH, POINTED AT THE SEA** — Robin's acceptance test
    /// (*"put a working cannon on a working planet and fire it"*, *"on land pointed to sea"*).
    ///
    /// ★★ **The shot is flown by the ENGINE, not by this test.** `crate::flight::Flight` is what
    /// carries meteors through the same air; a cannonball is a `FlyingBody` with a different mass and a
    /// slower launch, and `flight.rs` says so itself — *"nothing about flight is written for 'a
    /// meteor'"*. Gravity, air density, drag, heating and the surface crossing all come from
    /// `PlanetAir`, the engine's own environment. Nothing here chooses a drag coefficient.
    ///
    /// The site is FOUND, not typed: scanned for a land texel whose shot lands on water.
    #[test]
    fn the_cannon_stands_on_earth_and_puts_its_shot_in_the_sea() {
        let mats = mats();
        let (bore, shot_kg, gun_kg, gas_kg, burn, _p, gamma) = twenty_four_pounder();
        let fired = fire(&burn, &bore, shot_kg, gun_kg, gas_kg, gamma, 0.001, FLAME_K);
        assert_eq!(fired.outcome, Outcome::Fired);

        let earth = crate::planet::body("earth");
        let r_e = earth.radius();
        // The engine's own environment — the same one Terra flies its meteor swarm through.
        let env = crate::flight::PlanetAir::of(&mats, "earth", r_e);
        let iron = crate::materials::index_of(&mats, "cast_iron");
        let shot_r =
            (3.0 * (shot_kg / mats[iron].density as f64) / (4.0 * std::f64::consts::PI)).cbrt();
        let land = crate::terra::raster::shipped::earth_landmask();

        let mut found = None;
        'search: for (lat_i, lon_i) in (-60..60)
            .step_by(3)
            .flat_map(|a| (-180..180).step_by(3).map(move |b| (a, b)))
        {
            let (lat, lon) = (lat_i as f64, lon_i as f64);
            if !land.land_at(lat, lon) {
                continue;
            }
            for bearing in (0..360).step_by(15) {
                let e = Emplacement {
                    lat_deg: lat,
                    lon_deg: lon,
                    height_m: 12.0,
                    bearing_deg: bearing as f64,
                    elevation_deg: 45.0,
                };
                let (pos, vel) = launch(&e, fired.muzzle_ms, r_e);
                let mut fl = crate::flight::Flight::default();
                fl.introduce(crate::flight::FlyingBody {
                    id: 0,
                    pos,
                    vel,
                    mass_kg: shot_kg,
                    material: iron,
                    radius_m: shot_r,
                    temp_k: 288.0,
                    skin_m: 0.0,
                });
                // Step until it arrives. The ENGINE decides its own substeps.
                let mut arrival = None;
                for _ in 0..6000 {
                    let a = fl.step(&env, &mats, 1.0 / 60.0);
                    if let Some(a) = a.into_iter().next() {
                        arrival = Some(a);
                        break;
                    }
                    if fl.bodies().is_empty() {
                        break;
                    }
                }
                let Some(a) = arrival else { continue };
                let (ilat, ilon) = crate::geo::lat_lon_from_dir(a.at.normalize());
                if !land.land_at(ilat, ilon) {
                    found = Some((e, a, ilat, ilon));
                    break 'search;
                }
            }
        }
        let (site, arrival, ilat, ilon) =
            found.expect("somewhere on Earth a 24-pounder stands on land and fires into the sea");

        assert!(
            land.land_at(site.lat_deg, site.lon_deg),
            "the gun is on land at {:.1}, {:.1}",
            site.lat_deg,
            site.lon_deg
        );
        assert!(
            !land.land_at(ilat, ilon),
            "and the splash is at sea: {ilat:.3}, {ilon:.3}"
        );

        // The shot has to be a cannon shot rather than a number that merely lands wet.
        let a0 = crate::geo::dir_from_lat_lon(site.lat_deg, site.lon_deg);
        let range = r_e * a0.dot(arrival.at.normalize()).clamp(-1.0, 1.0).acos();
        assert!(
            (500.0..12_000.0).contains(&range),
            "a 24-pdr at 45 degrees carries a few kilometres: {range:.0} m"
        );
        // ★ Drag is REAL, and the engine applied it: the range falls well short of the vacuum parabola.
        // A trajectory matching v^2 sin(2t)/g would mean the air did nothing.
        let g = earth.gravity_at(r_e);
        let vacuum = fired.muzzle_ms.powi(2) * (2.0 * 45f64.to_radians()).sin() / g;
        assert!(
            range < 0.7 * vacuum,
            "air costs it most of the vacuum range: {range:.0} m against {vacuum:.0} m in vacuum"
        );
        assert!(arrival.energy_j > 0.0, "and it arrives carrying energy");
    }
}
