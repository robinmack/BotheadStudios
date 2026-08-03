//! **Rapid oxidation — one reaction, from a campfire to a powder charge** (docs/46 row 31, docs/64).
//!
//! Robin (2026-08-02): *"Rapid oxidation will be an important principle in the engine (fires, etc) so
//! this won't be wasted."*
//!
//! That framing is why this module is not called `propellant`. A campfire, a burning ship, a gunpowder
//! charge, a dust explosion and a rusting hull are **one reaction at different rates and different
//! oxidiser availability** — which is the charter's own shape (one law, every scale). Build a propellant
//! model and fire has to be bolted on later; build oxidation and a propellant is the case where the
//! oxidiser is already mixed in.
//!
//! ## The one quantity that separates them: where the oxygen comes from
//!
//! A fire is **air-limited**, so its rate is set by ventilation and geometry — which is why a closed
//! room smothers a fire and an open hearth does not. Black powder is **self-oxidising**: its potassium
//! nitrate carries 0.475 kg of O2 per kg of itself, and at the classic 75/15/10 that supplies 71% of
//! what its own fuels demand *with no air at all*. That single comparison is the whole difference, and
//! it is why powder burns in a sealed bore where a candle would go out.
//!
//! So there is no "propellant mode" here. [`burn`] takes whatever oxygen is available from outside and
//! adds it to whatever the charge carries; a charge in a vacuum simply gets none.
//!
//! ## Scope — deliberately basic, and the boundary is stated rather than discovered
//!
//! Robin, on this module's scope (2026-08-03): *"basic chemistry in scope, complex, maybe someday."*
//! So: one reaction per fuel, a limiting reagent, energy and gas out. **Not** reaction networks, not
//! intermediates, not chemical equilibrium, not temperature-dependent kinetics.
//!
//! *Flagged, with the real computation named* (Law V): at the ~1950 K a powder charge reaches, CO2
//! partially DISSOCIATES, which absorbs energy and changes the mole count — real interior-ballistics
//! codes model it. This one does not. The deferred computation is an equilibrium composition at the
//! flame temperature. If a muzzle velocity later comes out a few percent high, this is the first
//! suspect, and it is written down here rather than discovered by surprise.

use crate::materials::Material;

/// Which reagent ran out first — the thing that decides how much of a charge actually burns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitedBy {
    /// Every fuel was consumed; oxygen was left over. An open fire in free air.
    Fuel,
    /// Oxygen ran out with fuel remaining. A smothered fire — and, as it happens, black powder.
    Oxidiser,
    /// Nothing to burn.
    Nothing,
}

/// What a charge did when it burned.
#[derive(Clone, Debug, PartialEq)]
pub struct Burn {
    /// Chemical energy released, J.
    pub energy_j: f64,
    /// Moles of PERMANENT GAS produced. Condensed products carry mass but exert no pressure, and for a
    /// gun that distinction is most of the answer.
    pub gas_moles: f64,
    /// Fraction of the available fuel that actually burned, 0..=1.
    pub completeness: f64,
    /// kg of oxygen consumed.
    pub oxygen_used_kg: f64,
    /// Which reagent ran out.
    pub limited_by: LimitedBy,
}

impl Burn {
    /// Nothing burned.
    pub fn none() -> Burn {
        Burn {
            energy_j: 0.0,
            gas_moles: 0.0,
            completeness: 0.0,
            oxygen_used_kg: 0.0,
            limited_by: LimitedBy::Nothing,
        }
    }
}

/// **Burn a charge**: a set of `(material, kilograms)` plus whatever oxygen the surroundings supply.
///
/// Oxidisers in the charge contribute their carried oxygen to the same pool the air feeds, because a
/// molecule's oxygen and the atmosphere's are the same reagent — nothing about the reaction cares which
/// side of the charge boundary it came from. That is what makes one function serve both a bonfire and a
/// cartridge.
///
/// **The limiting reagent is the physics, not an approximation of it.** Whichever of fuel or oxygen
/// runs out first caps the reaction, and everything else scales with that fraction: a fuel-rich mixture
/// burns incompletely and leaves unburnt fuel, which is soot and smoke rather than an error.
///
/// `air_oxygen_kg` is oxygen available from outside — 0 in a sealed bore or a vacuum. Note this is the
/// oxygen, not the air: a caller with an atmosphere multiplies by its oxygen mass fraction, because how
/// much of a given air is oxygen is the atmosphere's business and not this function's.
pub fn burn(charge: &[(&Material, f64)], air_oxygen_kg: f64) -> Burn {
    let mut demand_kg = 0.0; // oxygen the fuels want
    let mut supply_kg = air_oxygen_kg.max(0.0); // oxygen available to them
    let mut fuel_energy = 0.0; // J, if everything burns
    let mut fuel_gas = 0.0; // mol, if everything burns
    let mut oxidiser_gas = 0.0; // mol, released regardless of how far the fuels get

    for &(m, kg) in charge {
        let kg = kg.max(0.0);
        let Some(rx) = m.reaction.as_ref() else {
            continue; // inert: neither burns nor supplies oxygen
        };
        supply_kg += kg * rx.oxygen_carried();
        demand_kg += kg * rx.oxygen_demand();
        fuel_energy += kg * rx.energy_per_kg();
        // An oxidiser's own gas (black powder's nitrogen) is released by its DECOMPOSITION, which
        // happens whether or not there is fuel left to meet it; a fuel's gas is its combustion product
        // and therefore scales with how much of it actually burned.
        if rx.oxygen_demand() > 0.0 {
            fuel_gas += kg * rx.gas_moles_per_kg();
        } else {
            oxidiser_gas += kg * rx.gas_moles_per_kg();
        }
    }

    if demand_kg <= 0.0 {
        // No fuel. An oxidiser alone still decomposes and still makes its gas.
        return Burn {
            energy_j: 0.0,
            gas_moles: oxidiser_gas,
            completeness: 0.0,
            oxygen_used_kg: 0.0,
            limited_by: if oxidiser_gas > 0.0 {
                LimitedBy::Fuel
            } else {
                LimitedBy::Nothing
            },
        };
    }

    let completeness = (supply_kg / demand_kg).min(1.0);
    Burn {
        energy_j: fuel_energy * completeness,
        gas_moles: fuel_gas * completeness + oxidiser_gas,
        completeness,
        oxygen_used_kg: demand_kg * completeness,
        limited_by: if completeness >= 1.0 {
            LimitedBy::Fuel
        } else {
            LimitedBy::Oxidiser
        },
    }
}

/// The classic 75/15/10 black-powder charge, by mass — a convenience for callers and for tests, so the
/// composition is written once rather than retyped at every call site (Law II).
pub fn black_powder_charge<'a>(mats: &'a [Material], kg: f64) -> Vec<(&'a Material, f64)> {
    let get = |id: &str| &mats[crate::materials::index_of(mats, id)];
    vec![
        (get("potassium_nitrate"), 0.75 * kg),
        (get("charcoal"), 0.15 * kg),
        (get("sulfur"), 0.10 * kg),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// **The property that makes gunpowder gunpowder: it burns with no air.** A charcoal fire in a
    /// vacuum does nothing at all; the same charcoal mixed with an oxidiser burns most of the way.
    /// One function, one law, and the only difference is where the oxygen came from.
    #[test]
    fn a_charge_that_carries_its_own_oxygen_burns_in_a_vacuum() {
        let mats = mats();
        let get = |id: &str| &mats[crate::materials::index_of(&mats, id)];

        // Charcoal alone, sealed in: nothing happens. No oxygen, no reaction.
        let starved = burn(&[(get("charcoal"), 0.15)], 0.0);
        assert_eq!(starved.energy_j, 0.0, "no oxygen, no fire");
        assert_eq!(starved.completeness, 0.0);
        assert_eq!(starved.limited_by, LimitedBy::Oxidiser);

        // The same charcoal in air burns completely.
        let open = burn(&[(get("charcoal"), 0.15)], 10.0);
        assert!(open.energy_j > 4.0e6, "0.15 kg of carbon is ~4.9 MJ");
        assert_eq!(open.completeness, 1.0);
        assert_eq!(open.limited_by, LimitedBy::Fuel);

        // Black powder, sealed in, with no air whatsoever: it burns anyway.
        let powder = burn(&black_powder_charge(&mats, 1.0), 0.0);
        assert!(
            powder.energy_j > 2.0e6,
            "a sealed kilogram of powder still releases MJ: {:e}",
            powder.energy_j
        );
        assert_eq!(
            powder.limited_by,
            LimitedBy::Oxidiser,
            "75/15/10 is oxygen-lean by design"
        );
    }

    /// **The limiting reagent, both ways.** Adding air to an oxygen-lean charge must make it burn more
    /// completely and release more energy — and must stop helping once the fuel is gone.
    #[test]
    fn the_limiting_reagent_decides_how_much_burns() {
        let mats = mats();
        let sealed = burn(&black_powder_charge(&mats, 1.0), 0.0);
        // Computed, not typed: 0.75 kg KNO3 carries 0.475 kg O2/kg; the fuels demand
        // 0.15*2.664 + 0.10*0.998. The ratio is what `completeness` must equal.
        assert!(
            (sealed.completeness - 0.713).abs() < 0.01,
            "sealed powder should burn ~71% complete, got {}",
            sealed.completeness
        );

        // Give it air: it burns further and releases more.
        let vented = burn(&black_powder_charge(&mats, 1.0), 0.2);
        assert!(
            vented.completeness > sealed.completeness,
            "air must help an oxygen-lean charge"
        );
        assert!(vented.energy_j > sealed.energy_j);

        // Flood it: completeness saturates at 1 and stops climbing. More air cannot burn fuel twice.
        let flooded = burn(&black_powder_charge(&mats, 1.0), 100.0);
        let drowned = burn(&black_powder_charge(&mats, 1.0), 1000.0);
        assert_eq!(flooded.completeness, 1.0);
        assert_eq!(
            flooded.energy_j, drowned.energy_j,
            "past stoichiometric, more oxygen changes nothing"
        );
        assert_eq!(flooded.limited_by, LimitedBy::Fuel);
    }

    /// Energy and gas must both scale with the size of the charge — twice the powder, twice of each.
    /// A model that lost linearity here would be wrong in the one direction nobody would check.
    #[test]
    fn a_charge_scales_with_its_mass() {
        let mats = mats();
        let one = burn(&black_powder_charge(&mats, 1.0), 0.0);
        let two = burn(&black_powder_charge(&mats, 2.0), 0.0);
        assert!((two.energy_j / one.energy_j - 2.0).abs() < 1e-9);
        assert!((two.gas_moles / one.gas_moles - 2.0).abs() < 1e-9);
        assert_eq!(
            one.completeness, two.completeness,
            "composition, not amount"
        );
        assert_eq!(burn(&[], 5.0), Burn::none(), "nothing to burn");
    }

    /// **Inert matter takes no part.** A cannonball sitting in the chamber must not change the burn —
    /// obvious, and exactly the kind of thing a `for` loop over "everything in the assembly" gets wrong.
    #[test]
    fn matter_with_no_reaction_data_is_inert() {
        let mats = mats();
        let get = |id: &str| &mats[crate::materials::index_of(&mats, id)];
        let mut charge = black_powder_charge(&mats, 1.0);
        let clean = burn(&charge, 0.0);
        charge.push((get("iron"), 12.0)); // the shot
        charge.push((get("granite"), 3.0));
        let with_junk = burn(&charge, 0.0);
        assert_eq!(
            clean, with_junk,
            "inert mass changes nothing about the burn"
        );
    }

    /// **THE MODEL AGAINST THE MEASUREMENT, and it does NOT match — recorded, not tuned.**
    ///
    /// The idealised equation `2 KNO3 + 3 C + S -> K2S + N2 + 3 CO2` predicts a permanent gas yield
    /// near 0.33 m³/kg at STP. The measured figure for real black powder is **~0.265 m³/kg** (docs/64),
    /// so the stoichiometry over-predicts by about a quarter.
    ///
    /// That is a real disagreement and the cause is known rather than mysterious: real powder does not
    /// follow the ideal equation. It makes potassium CARBONATE as well as sulfide — which locks up
    /// carbon *and* oxygen that the ideal equation spends on CO2 — along with CO, and a spray of other
    /// sulfides. Every one of those paths yields less permanent gas per kilogram.
    ///
    /// **Law VII: when physics disagrees with a hypothesis, record it.** The honest move is to pin the
    /// size and direction of the gap so a later interior-ballistics result can be read against it,
    /// NOT to scale the stoichiometry until it matches. A coefficient that made this line up would be
    /// a dial with a measurement's name on it.
    #[test]
    fn the_idealised_equation_over_predicts_gas_and_that_is_recorded_not_tuned() {
        const MOLAR_VOLUME_STP: f64 = 0.022414; // m³/mol at 273.15 K, 101325 Pa
        const MEASURED_YIELD: f64 = 0.2654; // m³/kg, sourced in docs/64
        let mats = mats();
        let b = burn(&black_powder_charge(&mats, 1.0), 0.0);
        let predicted = b.gas_moles * MOLAR_VOLUME_STP;
        assert!(
            predicted > MEASURED_YIELD,
            "the idealised equation should OVER-predict: {predicted:.3} vs {MEASURED_YIELD:.3} m³/kg"
        );
        let over = predicted / MEASURED_YIELD - 1.0;
        assert!(
            (0.15..0.40).contains(&over),
            "the gap is about a quarter and is explained by K2CO3/CO formation; it is pinned so a \
             later muzzle velocity can be read against it. Got {:.1}% over",
            over * 100.0
        );
    }
}
