//! **What the sky is doing at a place, right now** — the engine's time signal to its actors.
//!
//! Robin, across three messages that sharpen into one design (2026-08-03):
//!
//! > *"The engine keeps the time and sends signals to the model. The model sends information back to
//! > the engine (albedo, etc) and the engine renders the assembly."*
//!
//! > *"Even more honest is having the engine signal TEMPERATURE changes based on the earth's tilt and
//! > position relative to the sun."*
//!
//! > *"The model should be able to calculate amount of DAYLIGHT in each area of biomes based on the
//! > tilt/season/etc, which also affects flora (and fauna when/if we get there)."*
//!
//! So a season is not a table and not a date-keyed curve. It **emerges**: axial tilt plus orbital
//! position gives the solar declination, declination plus latitude gives the day length and the
//! insolation, and those are what a leaf, a bear or a snowpack actually respond to. Everything in this
//! module is exact spherical geometry over the declination `orbit` already computes for the sky — no
//! table, no dial, and nothing a scene can override.
//!
//! ★ **Two drivers, and they are not the same driver.** Photoperiod is pure geometry and is known to
//! the second. Temperature LAGS it, because ground and ocean have heat capacity — which is why the
//! warmest weeks trail the solstice by a month or more, and why leaves turn before the cold arrives. An
//! engine that collapsed them into one number could not produce either effect. [`Sky::day_length_h`] is
//! the exact one; the thermal term is deliberately absent and named as owed (see below).

use std::f64::consts::PI;

/// **The sky over one coordinate at one instant** — what the engine hands an assembly when it asks
/// what is happening to it.
///
/// Every field is DERIVED from the time and the place. Nothing here is configurable, because none of it
/// is a choice: it is where the Sun is.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    /// Solar declination, radians — the season itself. Positive is northern summer.
    pub declination: f64,
    /// Hours of daylight, 0..=24. Exact geometry: `cos H₀ = −tan φ · tan δ`.
    pub day_length_h: f64,
    /// Sun elevation above the horizon right now, radians. Negative is night.
    pub elevation: f64,
    /// Energy arriving on a horizontal square metre over the whole day, J/m² — the driver a thermal
    /// model integrates. Top-of-atmosphere: no cloud, no absorption (see the honesty note on
    /// [`daily_insolation`]).
    pub daily_insolation_j: f64,
}

/// The solar constant, W/m² — total solar irradiance at 1 AU. Measured (SORCE/TIM, 2008): 1360.8 ± 0.5.
pub const SOLAR_CONSTANT_W: f64 = 1360.8;

/// **How long the Sun is up**, hours, at latitude `lat_deg` for solar declination `dec_rad`.
///
/// `cos H₀ = −tan φ · tan δ`, where `H₀` is the hour angle at sunrise. Out-of-range means the Sun never
/// sets (midnight sun) or never rises (polar night) — the two cases that make the poles interesting,
/// and both fall out of the same expression rather than being special-cased.
///
/// Geometric sunrise: the Sun's CENTRE crossing a flat horizon. Real sunrise is a few minutes earlier
/// because the disc has a radius and the atmosphere refracts — about 8 minutes at mid-latitudes. That
/// is a stated bound, not a hidden one; it matters for a photoperiod threshold near the day a plant
/// switches state, and nothing yet reads it that finely.
pub fn day_length_hours(lat_deg: f64, dec_rad: f64) -> f64 {
    let phi = lat_deg.to_radians();
    let c = -phi.tan() * dec_rad.tan();
    if c <= -1.0 {
        24.0 // the Sun never sets
    } else if c >= 1.0 {
        0.0 // the Sun never rises
    } else {
        24.0 * c.acos() / PI
    }
}

/// **Energy on a horizontal square metre over one day**, J/m², at the top of the atmosphere.
///
/// The standard integral of `cos(solar zenith)` over the day:
/// `S · (86400/π) · (H₀ sin φ sin δ + cos φ cos δ sin H₀)`.
///
/// ★ It is TOP-OF-ATMOSPHERE, and that is the honest bound rather than an oversight: cloud, aerosol and
/// the air's own absorption take a large and variable bite, and the engine has no cloud field yet. A
/// surface temperature derived from this alone would be too warm. What it IS good for is the shape —
/// the ratio between seasons and between latitudes — which is what a plant responds to, and it needs no
/// data at all to be exactly right.
pub fn daily_insolation(lat_deg: f64, dec_rad: f64) -> f64 {
    let phi = lat_deg.to_radians();
    let c = -phi.tan() * dec_rad.tan();
    let h0 = if c <= -1.0 {
        PI // midnight sun: integrate the whole day
    } else if c >= 1.0 {
        0.0 // polar night
    } else {
        c.acos()
    };
    let day_s = 86_400.0;
    SOLAR_CONSTANT_W
        * (day_s / PI)
        * (h0 * phi.sin() * dec_rad.sin() + phi.cos() * dec_rad.cos() * h0.sin())
}

/// **The signal.** What the engine knows about the sky over `lat/lon` at `unix_seconds`.
///
/// The declination comes from `orbit::solar_declination_ra` — the same one the terminator is drawn
/// with, so the light in the picture and the season a leaf responds to cannot disagree (Law II).
pub fn sky_at(unix_seconds: f64, lat_deg: f64, lon_deg: f64) -> Sky {
    let (dec, _ra) = crate::orbit::solar_declination_ra(unix_seconds);
    let sun = crate::orbit::solar_direction_earth_fixed(unix_seconds);
    let (up, _n, _e) = crate::geo::tangent_frame(lat_deg, lon_deg);
    Sky {
        declination: dec,
        day_length_h: day_length_hours(lat_deg, dec),
        elevation: up.dot(sun).clamp(-1.0, 1.0).asin(),
        daily_insolation_j: daily_insolation(lat_deg, dec),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Solstices and equinoxes, as Unix seconds (2024, to the hour — near enough that the declination
    // is within a few hundredths of a degree of its extreme).
    const JUN_SOLSTICE: f64 = 1_718_945_000.0; // 2024-06-21
    const DEC_SOLSTICE: f64 = 1_734_744_000.0; // 2024-12-21
    const MAR_EQUINOX: f64 = 1_710_930_000.0; // 2024-03-20

    fn dec(t: f64) -> f64 {
        crate::orbit::solar_declination_ra(t).0
    }

    /// **The equinox is twelve hours everywhere**, which is the definition of an equinox and needs no
    /// reference data to check.
    #[test]
    fn at_the_equinox_every_latitude_gets_twelve_hours() {
        let d = dec(MAR_EQUINOX);
        assert!(
            d.to_degrees().abs() < 0.5,
            "the March equinox should have the Sun over the equator, got {:.2}°",
            d.to_degrees()
        );
        for lat in [-70.0, -45.0, -10.0, 0.0, 23.5, 51.5, 66.0] {
            let h = day_length_hours(lat, d);
            assert!(
                (h - 12.0).abs() < 0.25,
                "at {lat}° the equinox gives {h:.2} h, not 12"
            );
        }
    }

    /// **The midnight sun and the polar night**, which the same expression must produce without a
    /// special case: above the Arctic circle in June the Sun never sets, and at the same latitude
    /// south it never rises.
    #[test]
    fn the_arctic_circle_is_where_the_sun_stops_setting() {
        let d = dec(JUN_SOLSTICE);
        assert!(
            (d.to_degrees() - 23.4).abs() < 0.3,
            "June solstice declination should be ~+23.4°, got {:.2}",
            d.to_degrees()
        );
        assert_eq!(
            day_length_hours(70.0, d),
            24.0,
            "70°N in June: midnight sun"
        );
        assert_eq!(day_length_hours(-70.0, d), 0.0, "70°S in June: polar night");
        // And the circle itself sits where the declination puts it — that is what an Arctic circle IS.
        let circle = 90.0 - d.to_degrees();
        assert!(
            day_length_hours(circle + 0.5, d) == 24.0 && day_length_hours(circle - 2.0, d) < 24.0,
            "the boundary should fall at 90° − declination = {circle:.1}°"
        );
    }

    /// **★ The pole outshines the equator at the solstice** — the check that proves this is real
    /// geometry rather than a plausible curve.
    ///
    /// It is counter-intuitive and true: on the June solstice the North Pole receives MORE energy per
    /// square metre per day than the equator does, because the Sun never sets there, and that beats the
    /// equator's higher noon Sun over a whole day. A hand-rolled "seasons" model almost never has this
    /// property; the honest integral has it for free.
    #[test]
    fn the_summer_pole_gets_more_daily_energy_than_the_equator() {
        let d = dec(JUN_SOLSTICE);
        let pole = daily_insolation(90.0, d);
        let equator = daily_insolation(0.0, d);
        assert!(
            pole > equator,
            "June solstice: pole {:.1} MJ/m² should exceed equator {:.1} MJ/m²",
            pole / 1e6,
            equator / 1e6
        );
        // ★ The magnitude, DERIVED independently rather than recalled. At the pole on the solstice
        // the Sun circles at a constant elevation equal to the declination, so a horizontal surface
        // receives `S·sin δ` continuously and the day's total is just that times 86 400 s. No integral
        // and no reference table — which is the point, because the first version of this assertion
        // carried a remembered "~52.6 MJ/m²" that the physics disagrees with. A typed fixture is wrong
        // until a computed one says otherwise.
        let by_hand = SOLAR_CONSTANT_W * d.sin() * 86_400.0;
        assert!(
            (pole - by_hand).abs() < by_hand * 1e-9,
            "polar solstice: the integral gives {:.2} MJ/m², the constant-elevation argument gives \
             {:.2} MJ/m² — these are the same physics and must agree exactly",
            pole / 1e6,
            by_hand / 1e6
        );
        // And the winter pole gets nothing at all.
        assert_eq!(
            daily_insolation(-90.0, d),
            0.0,
            "the winter pole is in darkness, so it receives exactly zero"
        );
    }

    /// **The seasons reverse across the equator**, and the whole point of the tilt is that they do.
    #[test]
    fn the_hemispheres_have_opposite_summers() {
        let (jun, dec_) = (dec(JUN_SOLSTICE), dec(DEC_SOLSTICE));
        let dublin = 53.3;
        let wellington = -41.3;
        assert!(
            day_length_hours(dublin, jun) > day_length_hours(dublin, dec_),
            "Dublin's long days are in June"
        );
        assert!(
            day_length_hours(wellington, jun) < day_length_hours(wellington, dec_),
            "Wellington's long days are in December"
        );
        assert!(
            daily_insolation(dublin, jun) > daily_insolation(wellington, jun),
            "in June the north gets more energy than the matching south latitude"
        );
    }

    /// **The signal reads the same sky the picture is drawn with**, or a leaf could turn while the
    /// terminator says otherwise. `sky_at`'s elevation must agree with the solar direction the
    /// renderer uses, because it IS that direction.
    #[test]
    fn the_signal_and_the_terminator_are_the_same_sun() {
        for t in [JUN_SOLSTICE, DEC_SOLSTICE, MAR_EQUINOX] {
            let sun = crate::orbit::solar_direction_earth_fixed(t);
            let (slat, slon) = crate::geo::lat_lon_from_dir(sun);
            // Directly under the Sun, it is overhead: elevation 90°.
            let s = sky_at(t, slat, slon);
            assert!(
                (s.elevation.to_degrees() - 90.0).abs() < 1e-6,
                "at the subsolar point the Sun must be overhead, got {:.4}°",
                s.elevation.to_degrees()
            );
            // And the declination IS the subsolar latitude.
            assert!(
                (s.declination.to_degrees() - slat).abs() < 1e-6,
                "declination {:.4}° should equal the subsolar latitude {slat:.4}°",
                s.declination.to_degrees()
            );
        }
    }
}

/// **What the engine tells an actor about its situation right now.**
///
/// Robin's mechanism, in her words (2026-08-03): *"The engine keeps the time and sends signals to the
/// model. The model sends information back to the engine (albedo, etc) and the engine renders the
/// assembly, or the assemblies on the assemblies, ad infinitum."* And on how far it reaches: *"A human
/// could be treated as an assembly with many internal systems regulated on time of day/season of year…
/// If the engine notes that a 'Bear' assembly is present in an area we are looking at, it can ask the
/// assembly to extrapolate behavior/trajectory/attitude at that time of day."*
///
/// So this is deliberately NOT flora-shaped. It is the situation: when it is, where that is, and what
/// the sky is doing there. A maple answers with a colour, a bear with a behaviour, a snowpack with a
/// depth — and the engine never learns which of them it is talking to.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    /// Absolute time — the engine's clock, not a phase or a season index.
    pub unix_s: f64,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub sky: Sky,
}

impl Moment {
    /// The situation at a place and time. One call, everything derived.
    pub fn at(unix_s: f64, lat_deg: f64, lon_deg: f64) -> Self {
        Self {
            unix_s,
            lat_deg,
            lon_deg,
            sky: sky_at(unix_s, lat_deg, lon_deg),
        }
    }

    /// Days between two moments — the cadence gate. An actor decides how much of ITS OWN relevant time
    /// must pass before its answer could have changed: a leaf on the scale of days, a bear on minutes.
    ///
    /// Robin: *"This should be checked/applied only after an appropriate number of relative days have
    /// passed, so they should not impact computation."* That is why the gate belongs to the actor and
    /// not to a global tick — one clock, many cadences.
    pub fn days_since(&self, other: &Moment) -> f64 {
        (self.unix_s - other.unix_s) / 86_400.0
    }
}

/// **An actor that has something to say about its own state.**
///
/// The engine broadcasts a [`Moment`]; whatever can answer, answers. An assembly with nothing to say
/// does not implement this and costs nothing — which is the whole reason it is a trait and not a field.
///
/// ★ The engine never asks "are you a tree?". It asks "what are you now?", and renders the reply.
pub trait RespondsToTime {
    /// How much of its own time must pass before this actor's answer could differ. A deciduous tree
    /// answers in days; a bear would answer in minutes. Returning a large number is how an actor says
    /// "do not bother me often", and the engine is free to skip it until then.
    fn cadence_days(&self) -> f64;

    /// What this actor looks like now, as linear RGB — `None` if its appearance does not depend on
    /// time, which is the common case and must stay free.
    fn appearance_at(&self, _now: &Moment) -> Option<[f32; 3]> {
        None
    }
}

#[cfg(test)]
mod signal_tests {
    use super::*;

    /// A stand-in actor, to prove the mechanism is not flora-shaped: it answers about DARKNESS, which
    /// is what a nocturnal animal or a streetlight would key on, using the same signal a leaf uses.
    struct Nocturnal;
    impl RespondsToTime for Nocturnal {
        fn cadence_days(&self) -> f64 {
            1.0 / 48.0 // half an hour: it cares about dusk, not the season
        }
        fn appearance_at(&self, now: &Moment) -> Option<[f32; 3]> {
            let awake = now.sky.elevation < 0.0;
            Some(if awake {
                [0.9, 0.9, 0.2]
            } else {
                [0.1, 0.1, 0.1]
            })
        }
    }

    /// **The engine asks; the actor answers.** And the actor reads only the situation — never who is
    /// watching, never which scene it is in.
    #[test]
    fn an_actor_answers_the_signal_without_the_engine_knowing_what_it_is() {
        // Noon and midnight at one place, from the engine's own sun.
        let t = 1_718_945_000.0;
        let sun = crate::orbit::solar_direction_earth_fixed(t);
        let (slat, slon) = crate::geo::lat_lon_from_dir(sun);
        let noon = Moment::at(t, slat, slon);
        let midnight = Moment::at(t, -slat, slon + 180.0);

        let a = Nocturnal;
        assert!(noon.sky.elevation > 0.0, "the subsolar point is daylight");
        assert!(midnight.sky.elevation < 0.0, "the antipode is night");
        assert_ne!(
            a.appearance_at(&noon),
            a.appearance_at(&midnight),
            "an actor that keys on darkness must answer differently by day and by night"
        );
    }

    /// **An actor with nothing to say costs nothing** — the default, so adding the mechanism does not
    /// tax the ninety-nine percent of matter whose appearance is a constant.
    #[test]
    fn silence_is_the_default() {
        struct Rock;
        impl RespondsToTime for Rock {
            fn cadence_days(&self) -> f64 {
                f64::INFINITY
            }
        }
        assert!(
            Rock.appearance_at(&Moment::at(0.0, 0.0, 0.0)).is_none(),
            "a rock has no opinion about the time of year"
        );
        assert!(
            Rock.cadence_days().is_infinite(),
            "and it never needs asking again"
        );
    }

    /// **The cadence is the actor's, not the engine's** — one clock, many rates.
    #[test]
    fn each_actor_sets_how_often_it_needs_asking() {
        let a = Moment::at(1_718_945_000.0, 53.0, -9.0);
        let b = Moment::at(1_718_945_000.0 + 86_400.0 * 3.5, 53.0, -9.0);
        assert!(
            (b.days_since(&a) - 3.5).abs() < 1e-9,
            "days_since must be plain elapsed time, got {}",
            b.days_since(&a)
        );
        assert!(
            Nocturnal.cadence_days() < 1.0,
            "a dusk-keyed actor needs asking within a day"
        );
    }
}
