//! **A placed, stateful thing — an INSTANCE of an assembly type** (docs/67 §6 step 2).
//!
//! `broadleaf-tree-oak.json` is a species. The tree standing at 53.1°N is an individual: it has its own
//! lean, its own damage, its own heat, and it is somewhere. Until now the engine had only the first —
//! `assembly::Assembly` is a definition, immutable, loaded by id — so **damage had nowhere to live**,
//! and `terra::flora::Sited` carried lat/lon/kind/yaw/scale and no state at all. A bus cannot crush a
//! tree that has no state to be crushed in.
//!
//! ## Why this is not a fifth "thing at a place"
//!
//! It would be very easy for this to become one, and that would be the charter violation in its purest
//! form. Four representations of a placed, moving thing already exist, and each is a **view for one
//! consumer**:
//!
//! | type | carries | for |
//! |---|---|---|
//! | [`crate::orbit::Body`] | pos, vel, mass | the N-body integrator |
//! | [`crate::accretion::Body`] | + rho, radius, ang_mom, thermal_j | clumping and merging |
//! | [`crate::interaction::BodyState`] | + radius, strength, air | detecting a contact |
//! | [`crate::render::Drawn`] | pos, vel, radius, material, temp | drawing it |
//!
//! None of them says *which assembly it is an instance of*, none carries an attitude, none carries
//! damage, and none knows what contains it. So this type is not a fifth sibling: it is **the thing they
//! are projections of**. It holds only what is genuinely STATE, and everything definitional — mass,
//! extent, materials, centre of mass — is asked of the TYPE and never stored twice. The projections
//! live here ([`Instance::body_state`]) so a view can never quietly disagree with the instance.
//!
//! ## What is state and what is not
//!
//! State: where it is, which way it faces, how it is moving, how hot it is, what has happened to it,
//! and what contains it. **Not state**: its mass, its extent, its shape, its materials, its centre of
//! mass — those belong to the definition, and asking the definition every time is what keeps ten
//! thousand oaks from carrying ten thousand copies of one oak's mass.
//!
//! Robin's framing (docs/67): *"Has an attitude, mass, is assembled of materials, can contain other
//! assemblies… Each assembly has momentum, heading, destructability."* Mass is in that list and is
//! deliberately NOT a field here — it is [`Instance::mass_kg`], derived from the type and reduced by
//! this instance's own damage, so a splintered oak weighs what is left of it without anyone writing
//! that number down.

//! ## ★★ NOT YET WIRED — and that is the docs/48 pattern, recorded on the day it was written
//!
//! Nothing in the engine holds an `Instance` yet. The obvious first consumer is Terra's cannon, and it
//! **cannot be one until Earth is a container**: `cannon_at` is `(lat, lon, bearing)`, a geographic
//! placement, and turning that into a [`Placement`] means the gun is *within* Earth — which needs Earth
//! to be an assembly with an id, i.e. docs/67 step 5. The dependency the migration order predicted,
//! arriving exactly where it said it would.
//!
//! Written up as `docs/46` row 46 rather than left as a good intention, because "the law is built and
//! proven, then wired into one place or none" is this repo's most repeated failure and the only defence
//! against it is naming each instance of it out loud.

use crate::assembly::Assembly;
use crate::materials::Material;
use glam::{DQuat, DVec3};

/// A stable identity for one placed thing, for the life of the universe that holds it.
///
/// Identity is what makes damage possible: without it, a crushed tree is a different tree next frame.
/// It is opaque on purpose — nothing may read meaning out of the number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId(pub u64);

/// **Where something is and which way it faces — IN ITS CONTAINER'S FRAME.**
///
/// The frame is not decoration. A gun on a ship has a position in the ship's frame; the ship has one in
/// the sea's; the sea's is the planet's. Only the composition is a world position, and composing is the
/// container's job rather than the contained thing's — which is what lets a ship turn and take its guns
/// with it without touching a single gun.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub at_m: DVec3,
    pub attitude: DQuat,
    /// What this is inside. `None` means the universe itself.
    pub within: Option<InstanceId>,
}

impl Placement {
    /// At the origin of `within`'s frame, unrotated.
    pub fn inside(within: Option<InstanceId>) -> Placement {
        Placement {
            at_m: DVec3::ZERO,
            attitude: DQuat::IDENTITY,
            within,
        }
    }

    /// This placement expressed in the frame of whatever contains its container: rotate by the
    /// container's attitude, then translate by its position. Composition is associative, so walking a
    /// containment chain outward with this gives a world placement — see the test.
    pub fn composed_into(&self, container: &Placement) -> Placement {
        Placement {
            at_m: container.at_m + container.attitude * self.at_m,
            attitude: container.attitude * self.attitude,
            within: container.within,
        }
    }
}

/// How it is moving, in the same frame its placement is expressed in.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Motion {
    pub vel_ms: DVec3,
    /// Angular momentum (kg·m²/s), not an angular velocity — the conserved quantity, so a body that
    /// loses mass off one side spins faster without anyone applying a rule. Same choice
    /// `accretion::Body` already made.
    pub ang_mom: DVec3,
}

/// **What has happened to this individual.** Empty is pristine, which is why a fresh instance costs
/// nothing: ten thousand undamaged oaks carry two empty vectors each.
///
/// Indices are into the TYPE's `parts` and `connections`, so damage is meaningless without the
/// definition it refers to — which is correct: "the third strut is gone" is not a statement about the
/// world, it is a statement about this design of thing.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Damage {
    /// Integrity per part, 1 = pristine, 0 = gone. Shorter than `parts` means the rest are pristine.
    pub part_integrity: Vec<f64>,
    /// Connections that have failed, by index. A severed connection is how an assembly comes apart
    /// without any part being destroyed — the branch is intact, it is simply no longer attached.
    pub severed: Vec<usize>,
}

impl Damage {
    /// How much of part `i` is left, 1.0 for anything this damage record has nothing to say about.
    pub fn integrity(&self, i: usize) -> f64 {
        self.part_integrity
            .get(i)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn is_pristine(&self) -> bool {
        self.severed.is_empty() && self.part_integrity.iter().all(|&v| v >= 1.0)
    }
}

/// **One placed, stateful thing.** See the module docs for what is state here and what is not.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub id: InstanceId,
    /// Which assembly definition this is one of — the TYPE. Many instances share one.
    pub of: String,
    pub placement: Placement,
    pub motion: Motion,
    /// Internal energy (J) above the ambient the world was built at. Temperature is DERIVED from this
    /// and the type's own materials; storing a temperature instead would let two instances of one thing
    /// disagree about what a joule is worth.
    pub thermal_j: f64,
    pub damage: Damage,
}

impl Instance {
    /// A pristine instance of `of`, placed at rest.
    pub fn of_type(id: InstanceId, of: &str, placement: Placement) -> Instance {
        Instance {
            id,
            of: of.to_string(),
            placement,
            motion: Motion::default(),
            thermal_j: 0.0,
            damage: Damage::default(),
        }
    }

    /// **Its mass, from the TYPE and this instance's own damage.** Not a field: a definitional quantity
    /// stored per instance is a copy waiting to disagree, and there may be ten thousand of them.
    ///
    /// Damage reduces mass by the matter actually lost — part by part, through the same
    /// envelope × packing × density the definition uses, so a half-splintered trunk weighs half a trunk
    /// and nobody wrote that number.
    pub fn mass_kg(&self, def: &Assembly, mats: &[Material]) -> Result<f64, String> {
        if self.damage.is_pristine() {
            return def.mass_kg(mats);
        }
        let mut total = 0.0;
        for (i, p) in def.parts.iter().enumerate() {
            let m = mats
                .iter()
                .find(|m| m.id == p.material)
                .ok_or_else(|| format!("unknown material '{}'", p.material))?;
            total += p.matter_volume_m3() * m.density as f64 * self.damage.integrity(i);
        }
        Ok(total)
    }

    /// **Where it ends** — the type's own rule (`Assembly::reach_m`), which is the outermost boundary of
    /// its outermost component. Damage can only shrink an assembly, never extend it, so this is a
    /// bound for a damaged instance as well as an exact answer for a pristine one.
    pub fn reach_m(&self, def: &Assembly) -> f64 {
        def.parts
            .iter()
            .enumerate()
            .filter(|(i, _)| self.damage.integrity(*i) > 0.0)
            .map(|(_, p)| p.reach_m())
            .fold(0.0, f64::max)
    }

    /// Temperature (K) above the ambient the thermal state is measured from: `ΔT = E / (m·c)`, with the
    /// heat capacity of the matter this thing is actually made of.
    pub fn temperature_rise_k(&self, def: &Assembly, mats: &[Material]) -> Result<f64, String> {
        let mut heat_capacity = 0.0; // J/K
        for (i, p) in def.parts.iter().enumerate() {
            let m = mats
                .iter()
                .find(|m| m.id == p.material)
                .ok_or_else(|| format!("unknown material '{}'", p.material))?;
            let Some(c) = m.specific_heat() else { continue };
            heat_capacity += p.matter_volume_m3() * m.density as f64 * c * self.damage.integrity(i);
        }
        if heat_capacity <= 0.0 {
            return Ok(0.0); // nothing characterised to heat — unknown stays unknown at the boundary
        }
        Ok(self.thermal_j / heat_capacity)
    }

    /// **The projection into what the collision detector wants.** It lives here so a view cannot
    /// quietly disagree with the instance it came from — the whole reason this type exists rather than
    /// a fifth sibling of `BodyState`.
    ///
    /// `world` is this instance's placement composed out to the universe; the caller walks the
    /// containment chain because only it can look ids up.
    pub fn body_state(
        &self,
        def: &Assembly,
        mats: &[Material],
        world: &Placement,
    ) -> Result<crate::interaction::BodyState, String> {
        let strength_pa = def
            .parts
            .iter()
            .enumerate()
            .filter(|(i, _)| self.damage.integrity(*i) > 0.0)
            // The strongest surviving part is what an impactor has to get through — a tree with its
            // crown gone still has a trunk. `fracture_strength` is the catalogue's own number.
            .filter_map(|(_, p)| mats.iter().find(|m| m.id == p.material))
            .map(|m| m.fracture_strength as f64)
            .fold(0.0f64, f64::max);
        Ok(crate::interaction::BodyState {
            pos: world.at_m,
            vel: self.motion.vel_ms,
            mass_kg: self.mass_kg(def, mats)?,
            radius_m: self.reach_m(def),
            strength_pa,
            // An assembly of parts carries no atmosphere of its own; a body that does supplies it.
            air: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::compiled;

    fn oak() -> Assembly {
        compiled::parse(compiled::BROADLEAF_TREE_OAK)
    }

    /// ★★ **AN INSTANCE IS NOT ITS TYPE.** Two oaks standing in different places, one of them damaged,
    /// share every DEFINITIONAL number and agree on nothing else. This is the whole point: the species
    /// is one object however many trees there are, and what happened to a particular tree lives on that
    /// tree.
    #[test]
    fn two_instances_of_one_type_share_the_definition_and_nothing_else() {
        let mats = crate::materials::load();
        let def = oak();

        let a = Instance::of_type(InstanceId(1), "broadleaf-tree-oak", Placement::inside(None));
        let mut b = Instance::of_type(
            InstanceId(2),
            "broadleaf-tree-oak",
            Placement {
                at_m: DVec3::new(30.0, 0.0, 12.0),
                attitude: DQuat::from_rotation_y(0.7),
                within: None,
            },
        );

        // Pristine, they are the same tree in every way the DEFINITION describes.
        assert_eq!(
            a.mass_kg(&def, &mats).unwrap(),
            b.mass_kg(&def, &mats).unwrap()
        );
        assert_eq!(a.reach_m(&def), b.reach_m(&def));
        assert_ne!(a.placement, b.placement);
        assert_ne!(a.id, b.id);

        // Now something happens to ONE of them: the crown is torn off.
        let crown = def
            .parts
            .iter()
            .position(|p| p.material == "broadleaf_foliage")
            .expect("an oak has foliage");
        b.damage.part_integrity = vec![1.0; def.parts.len()];
        b.damage.part_integrity[crown] = 0.0;

        assert!(
            b.mass_kg(&def, &mats).unwrap() < a.mass_kg(&def, &mats).unwrap(),
            "a tree that lost its crown weighs less than one that did not"
        );
        assert!(
            b.reach_m(&def) < a.reach_m(&def),
            "and it no longer reaches as far ({:.2} m vs {:.2} m)",
            b.reach_m(&def),
            a.reach_m(&def)
        );
        // ★ THE DEFINITION IS UNTOUCHED — the species did not change because one tree was damaged.
        assert_eq!(
            def.mass_kg(&mats).unwrap(),
            a.mass_kg(&def, &mats).unwrap(),
            "damaging an instance must not reach back into its type"
        );
        assert!(a.damage.is_pristine(), "the other tree is unharmed");
    }

    /// **Damage is measured in matter, not in a health bar.** Losing half of every part loses half the
    /// mass, because the reduction runs through the same envelope × packing × density the definition
    /// uses. Nobody writes a damage-to-mass curve.
    #[test]
    fn damage_removes_the_matter_it_removed() {
        let mats = crate::materials::load();
        let def = oak();
        let mut half =
            Instance::of_type(InstanceId(3), "broadleaf-tree-oak", Placement::inside(None));
        half.damage.part_integrity = vec![0.5; def.parts.len()];
        let whole = def.mass_kg(&mats).unwrap();
        let left = half.mass_kg(&def, &mats).unwrap();
        assert!(
            (left / whole - 0.5).abs() < 1e-9,
            "half of every part is half the mass ({left:.1} kg of {whole:.1} kg)"
        );
    }

    /// ★★ **PLACEMENT COMPOSES THROUGH CONTAINMENT** — which is what lets a ship turn and take its guns
    /// with it without touching a gun. A gun sits in the ship's frame; the ship sits in the world's;
    /// only the composition is a world position.
    #[test]
    fn a_gun_on_a_turning_ship_goes_where_the_ship_puts_it() {
        let ship = Placement {
            at_m: DVec3::new(1000.0, 0.0, 0.0),
            attitude: DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2),
            within: None,
        };
        // Four metres to starboard of the ship's centre, in the SHIP's frame.
        let gun = Placement {
            at_m: DVec3::new(0.0, 0.0, 4.0),
            attitude: DQuat::IDENTITY,
            within: Some(InstanceId(9)),
        };
        let world = gun.composed_into(&ship);
        // Rotating +90° about Y sends +Z to +X, so the gun lands 4 m further along X.
        assert!(
            (world.at_m - DVec3::new(1004.0, 0.0, 0.0)).length() < 1e-9,
            "{:?}",
            world.at_m
        );
        assert!(
            world.within.is_none(),
            "composing outward leaves the ship's own container"
        );

        // Turn the ship and the gun follows, with no change to the gun.
        let turned = Placement {
            attitude: DQuat::from_rotation_y(std::f64::consts::PI),
            ..ship
        };
        let moved = gun.composed_into(&turned);
        assert!(
            (moved.at_m - DVec3::new(1000.0, 0.0, -4.0)).length() < 1e-9,
            "{:?}",
            moved.at_m
        );
        // And the gun's own attitude comes along: it is now facing the way the ship faces.
        assert!((moved.attitude.to_axis_angle().1 - std::f64::consts::PI).abs() < 1e-9);
    }

    /// **The projection agrees with the instance**, which is the reason it lives here. A `BodyState`
    /// built anywhere else is a place the two could drift.
    #[test]
    fn the_collision_view_reports_what_the_instance_says() {
        let mats = crate::materials::load();
        let def = oak();
        let mut tree =
            Instance::of_type(InstanceId(4), "broadleaf-tree-oak", Placement::inside(None));
        tree.motion.vel_ms = DVec3::new(0.0, -9.0, 0.0);
        let world = Placement {
            at_m: DVec3::new(5.0, 0.0, -2.0),
            attitude: DQuat::IDENTITY,
            within: None,
        };
        let bs = tree.body_state(&def, &mats, &world).unwrap();
        assert_eq!(bs.pos, world.at_m);
        assert_eq!(bs.vel, tree.motion.vel_ms);
        assert_eq!(bs.mass_kg, tree.mass_kg(&def, &mats).unwrap());
        assert_eq!(bs.radius_m, tree.reach_m(&def));
        assert!(bs.air.is_none(), "an oak carries no atmosphere of its own");
    }

    /// Heat is an ENERGY on the instance and a temperature only when asked, so the same joules in a
    /// twig and in a trunk are different temperatures without anyone deciding that.
    #[test]
    fn temperature_is_derived_from_energy_and_what_it_is_made_of() {
        let mats = crate::materials::load();
        let def = oak();
        let mut tree =
            Instance::of_type(InstanceId(5), "broadleaf-tree-oak", Placement::inside(None));
        assert_eq!(
            tree.temperature_rise_k(&def, &mats).unwrap(),
            0.0,
            "cold to start"
        );
        tree.thermal_j = 1.0e9;
        let hot = tree.temperature_rise_k(&def, &mats).unwrap();
        assert!(hot > 0.0, "energy in means temperature up, got {hot}");
        // The SAME energy in a tree half burnt away raises it further — less matter to heat.
        let mut burnt = tree.clone();
        burnt.damage.part_integrity = vec![0.5; def.parts.len()];
        assert!(
            burnt.temperature_rise_k(&def, &mats).unwrap() > hot,
            "the same joules in less matter is a higher temperature"
        );
    }
}
