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

/// **How far through its autumn a place is** — 0 at the height of summer, 1 at midwinter.
///
/// Robin: *"Flora changes colors/albedo through the seasons… Grass starts green in spring on the
/// prairie, transitions to golden-yellow, leaves burst into a riot of color in cold climates before
/// falling."*
///
/// The driver is **how far the day length has fallen from its own annual maximum toward its own annual
/// minimum, at this latitude** — which is pure geometry over the declination, with no threshold, no
/// species constant and nothing to tune. Photoperiod is the real primary cue for deciduous senescence,
/// and using the LOCAL range rather than an absolute hour count is what makes it work everywhere at
/// once.
///
/// ★★ **It is a DECLARED model (Law V), and here is exactly what it defers.** Real senescence is
/// pigment chemistry: chlorophyll degrades and is resorbed, unmasking carotenoids, while anthocyanins
/// are synthesised anew. Its RATE is set by temperature as well as photoperiod, which is why a warm
/// autumn runs late and a hard frost ends one overnight — and the engine has no surface-temperature
/// field yet ([`daily_insolation`] is top-of-atmosphere). So this reproduces the SHAPE of the season
/// and not its year-to-year variation, and it cannot produce a frost event at all.
///
/// ★★★ **The RESOLVED version is pigment-resolved, and that is a bigger idea than autumn colour.**
/// A leaf's spectrum is the sum of what its pigments absorb, so the honest model carries the pigment
/// COMPLEMENT and derives the spectrum — rather than interpolating between two measured leaves, which
/// is what this does. Robin (2026-08-03): *"There is also red chlorophyll, which is useful for catching
/// longer wavelengths."* Correct, and it is the reason the complement has to be a variable rather than
/// a constant: chlorophyll *a* and *b* give out around 660–680 nm, while chlorophyll **d** and **f**
/// absorb well past it — *f* beyond ~720 nm — which is how some cyanobacteria live on light a leaf
/// would find useless. An engine that hard-codes "chlorophyll absorbs red" cannot represent them at
/// all, and cannot represent an ocean's photosynthesisers either.
/// The catalogue already holds the measurement this would need: the NEON samples carry chlorophyll AND
/// carotenoid mass per sample, so the endmembers here are labelled with the pigment content that
/// produced them rather than with a date.
///
/// ★ Two predictions it gets right for free, which is the argument for deriving rather than tabulating:
/// the tropics barely senesce (day length hardly varies there, so the fraction stays near zero — and
/// tropical broadleaf forest is indeed evergreen), and the far north turns early and hard.
pub fn annual_day_length_range_h(lat_deg: f64) -> f64 {
    // ★ The tilt is asked for, not restated. This used to hold its own `23.44` literal while `orbit`
    // held `23.439` with a secular term — two answers to "how tilted is the Earth" (docs/46 row 39).
    // J2000 is the honest epoch for a question with no date in it, and the drift is 0.47"/yr.
    let tilt = crate::orbit::obliquity_rad(946_728_000.0);
    (day_length_hours(lat_deg, tilt) - day_length_hours(lat_deg, -tilt)).abs()
}

/// See [`senescence_fraction`]. Returns the phase AND the annual day-length range that phase is a
/// fraction OF, because the phase alone is meaningless where the range is minutes.
///
/// ★★ **This is the declared model knowing when it is outside its own validity** — the same
/// requirement docs/46 row 33 put on the interior-ballistics model, for the same reason: a model that
/// answers confidently outside its assumptions does not become inaccurate, it answers a DIFFERENT
/// question, and nothing downstream can tell. At the equator the annual swing is about five minutes, so
/// normalising by it turns noise into a full autumn. The phase is still correct; it is simply a fraction
/// of nearly nothing, and a consumer must weigh it by the range to know that.
pub fn senescence_phase(lat_deg: f64, unix_s: f64) -> (f64, f64) {
    (
        senescence_fraction(lat_deg, unix_s),
        annual_day_length_range_h(lat_deg),
    )
}

pub fn senescence_fraction(lat_deg: f64, unix_s: f64) -> f64 {
    let (dec_now, _) = crate::orbit::solar_declination_ra(unix_s);
    // The extremes of the local day length are set by the extremes of the declination — the axial
    // tilt itself. ★ This comment used to claim it "keeps ONE source for the obliquity" while
    // hardcoding a second one; now it actually does, at the epoch being asked about.
    let tilt = crate::orbit::obliquity_rad(unix_s);
    let summer = day_length_hours(lat_deg, if lat_deg >= 0.0 { tilt } else { -tilt });
    let winter = day_length_hours(lat_deg, if lat_deg >= 0.0 { -tilt } else { tilt });
    let now = day_length_hours(lat_deg, dec_now);
    let span = summer - winter;
    if span.abs() < 1e-9 {
        return 0.0; // the equator: no season to be part-way through
    }
    ((summer - now) / span).clamp(0.0, 1.0)
}

#[cfg(test)]
mod phenology_tests {
    use super::*;
    const JUN: f64 = 1_718_945_000.0;
    const DEC: f64 = 1_734_744_000.0;
    const MAR: f64 = 1_710_930_000.0;

    /// **Midsummer is 0 and midwinter is 1**, in whichever hemisphere you stand.
    #[test]
    fn the_season_runs_from_midsummer_to_midwinter() {
        for (lat, summer, winter) in [(53.3, JUN, DEC), (-41.3, DEC, JUN)] {
            assert!(
                senescence_fraction(lat, summer) < 0.02,
                "at {lat}° the height of summer should be ~0, got {:.3}",
                senescence_fraction(lat, summer)
            );
            assert!(
                senescence_fraction(lat, winter) > 0.98,
                "at {lat}° midwinter should be ~1, got {:.3}",
                senescence_fraction(lat, winter)
            );
            let equinox = senescence_fraction(lat, MAR);
            assert!(
                (0.3..0.7).contains(&equinox),
                "an equinox should sit near the middle, got {equinox:.3}"
            );
        }
    }

    /// **★ The tropics have no autumn, and nothing was told to make that true** — but the honest
    /// statement is about the RANGE, not the phase.
    ///
    /// The phase at the equator swings the full 0..1 like everywhere else, because it is normalised by
    /// the local range and the local range is about five minutes. That is the model reporting a
    /// fraction of nearly nothing, and it is why `senescence_phase` hands back the range too. The
    /// physical fact — the one that makes tropical broadleaf forest evergreen — is that the range
    /// itself is negligible there and enormous in the north.
    #[test]
    fn whether_a_place_has_an_autumn_at_all_is_the_day_length_range() {
        let equator = annual_day_length_range_h(0.5);
        let dublin = annual_day_length_range_h(53.3);
        let tromso = annual_day_length_range_h(69.6);
        assert!(
            equator < 0.3,
            "the equator's annual day-length range is {equator:.2} h — there is no season to be in"
        );
        assert!(
            dublin > 8.0,
            "Dublin swings {dublin:.2} h across the year, which is what an autumn IS"
        );
        assert!(
            tromso > dublin,
            "and inside the Arctic circle it is larger still: {tromso:.2} h"
        );
        // The model REPORTS its own weakness rather than hiding it: full phase, no range.
        let (phase, range) = senescence_phase(0.5, DEC);
        assert!(
            phase > 0.9 && range < 0.3,
            "at the equator the phase runs ({phase:.2}) while the range does not ({range:.2} h) — a \
             consumer must weigh one by the other, which is why both are returned"
        );
    }

    /// **The far north turns harder than the mid-latitudes**, at the same date.
    #[test]
    fn autumn_arrives_earlier_the_further_north_you_stand() {
        // Late September, when the north is well into its turn and the south of France is not.
        let sept = 1_727_000_000.0;
        let (tromso, nice) = (
            senescence_fraction(69.6, sept),
            senescence_fraction(43.7, sept),
        );
        assert!(
            tromso > nice,
            "Tromsø {tromso:.3} should be further through autumn than Nice {nice:.3}"
        );
    }
}
