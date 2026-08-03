//! **An assembly: matter with a shape, in a place** (docs/64).
//!
//! A planet and an ocean liner are not different kinds of thing to this engine. Both are catalogued
//! materials arranged in space, differing in how that arrangement is INDEXED — radial shells for a
//! star, a quadtree for a planet's surface, a graph of joined parts for a gun. This is the third of
//! those, and it is the one that has to handle **anything that is not a sphere**.
//!
//! ## Nothing here is declared that can be derived
//!
//! An assembly has no `mass` field. Mass is volume times density, summed over parts; the centre of mass
//! follows from the same sum. Writing a mass down would be a number that traces to nothing (Law V) and
//! would silently disagree with the geometry the moment either changed — which is exactly the class of
//! bug `crate::laws` fails the build over for world files.
//!
//! ## Three assemblies, not one
//!
//! Robin (2026-08-03): *"the gunpowder and its properties might be an assembly of its own... that way we
//! can reload cannons"*, *"The cannonball another assembly"*, *"And the canon itself a third."*
//!
//! A single `cannon` containing barrel, carriage, charge and ball would fire exactly once. Split three
//! ways, each has its own lifetime: the GUN persists and recoils, the CHARGE is consumed, the SHOT is
//! transferred out at speed and becomes an assembly in flight. So containment is a relationship with
//! state rather than a static parent-child link — an assembly GRAPH, because a tree cannot express
//! reloading.

use crate::materials::Material;
use serde::{Deserialize, Serialize};

/// A primitive volume. **Not just spheres** — a gun barrel is a tube, a carriage cheek is a slab, and
/// an assembly format that could only describe planets would be the whole problem restated.
///
/// Dimensions are metres. Every variant answers [`Shape::volume_m3`] in closed form, because a volume
/// that had to be integrated numerically would make an assembly's mass depend on a sampling density —
/// which is a rendering concern leaking into a physical quantity.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Sphere {
        r: f64,
    },
    /// A solid cylinder — the breech block, a truck axle.
    Cylinder {
        r: f64,
        length: f64,
    },
    /// A hollow cylinder: **the barrel**. `r_bore` is the inside, `r_outer` the metal's outside.
    Tube {
        r_outer: f64,
        r_bore: f64,
        length: f64,
    },
    /// A rectangular slab — carriage cheeks, the bed, a bulkhead.
    Slab {
        x: f64,
        y: f64,
        z: f64,
    },
}

impl Shape {
    /// Volume in m³. Zero for nonsense dimensions rather than a negative volume, which would silently
    /// subtract mass from an assembly.
    pub fn volume_m3(&self) -> f64 {
        use std::f64::consts::PI;
        match *self {
            Shape::Sphere { r } => {
                if r <= 0.0 {
                    0.0
                } else {
                    4.0 / 3.0 * PI * r * r * r
                }
            }
            Shape::Cylinder { r, length } => {
                if r <= 0.0 || length <= 0.0 {
                    0.0
                } else {
                    PI * r * r * length
                }
            }
            Shape::Tube {
                r_outer,
                r_bore,
                length,
            } => {
                if length <= 0.0 || r_outer <= r_bore.max(0.0) {
                    0.0
                } else {
                    PI * (r_outer * r_outer - r_bore.max(0.0).powi(2)) * length
                }
            }
            Shape::Slab { x, y, z } => {
                if x <= 0.0 || y <= 0.0 || z <= 0.0 {
                    0.0
                } else {
                    x * y * z
                }
            }
        }
    }

    /// The radius of the sphere of equal volume — how a part summarises when it is too far away to
    /// resolve (docs/44). One reduction for every shape, so a distant gun and a distant boulder are
    /// summarised the same way.
    pub fn equivalent_radius_m(&self) -> f64 {
        let v = self.volume_m3();
        if v <= 0.0 {
            0.0
        } else {
            (3.0 * v / (4.0 * std::f64::consts::PI)).cbrt()
        }
    }
}

/// **How two parts are held together.** Where an assembly breaks is decided by how it was fastened, not
/// only by what it is made of — a welded bulkhead and a bolted one are the same steel and fail
/// completely differently.
///
/// ★ Every variant resolves to properties `data/materials.json` ALREADY carries, so a join is a boundary
/// condition on the existing contact/cohesion law rather than a second physics (Law II). A join that
/// needed a new material property would be a signal to source and catalogue that property.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Join {
    /// Material continuity — fails at the parent's own `fracture_strength`.
    Weld { area_m2: f64 },
    /// A bolt or pin: discrete, carries tension and shear.
    Fastener { area_m2: f64, count: u32 },
    /// **Carries compression and permits rotation** — the barrel's trunnions in the carriage cheeks.
    /// Found by building a real gun rather than by designing the taxonomy from first principles.
    Bearing { area_m2: f64 },
    /// **Tension only, and only once taut** — the breeching rope. Its capacity depends on its current
    /// EXTENSION, not merely on its material, which is what makes it different from a fastener.
    TensionOnly { area_m2: f64, slack_m: f64 },
    /// A press or clearance fit held by friction — the ball in the bore, until pressure overcomes it.
    InterferenceFit { area_m2: f64, normal_pa: f64 },
    /// Resting contact. **No tensile capacity at all**: it separates under any upward load.
    Rest { area_m2: f64 },
}

/// One piece of matter: what it is made of, what shape it is, and where it sits.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Part {
    pub name: String,
    /// Material id into `data/materials.json`.
    pub material: String,
    pub shape: Shape,
    /// Centre of the part, in the assembly's own frame, metres.
    #[serde(default)]
    pub at_m: [f64; 3],
    /// **How much of this part's SHAPE is actually matter**, 0..1. `1.0` (the default) is solid.
    ///
    /// ★ The shape is the ENVELOPE — the space the part occupies. Packing says how much of that
    /// envelope is the substance and how much is void. A powder charge, a shovel of gravel, snow, a
    /// bale of hay and a stack of shot are all matter in an ARRANGEMENT, and the arrangement is not a
    /// property of the substance: the same powder poured, shaken or rammed fills different volumes at
    /// the same mass.
    ///
    /// This is the distinction `data/materials.json` already draws when it records charcoal's TRUE
    /// density of 1400 kg/m3 beside a poured bulk of 260-380 — and it is why the catalogue carries the
    /// true one. **Mass is envelope x packing x density**, so a loosely packed part weighs less than a
    /// solid one of the same size while its substance is unchanged, which is exactly right.
    ///
    /// It was added because the alternative was worse: sizing a charge's parts to their SOLID volume
    /// made an 8 lb charge occupy half the chamber it really fills, and chamber volume sets the
    /// pressure a burn reaches.
    #[serde(default = "one")]
    pub packing: f64,
}

/// serde default for [`Part::packing`] — solid unless stated.
fn one() -> f64 {
    1.0
}

impl Part {
    /// The space this part takes up, m³ — its shape, void included.
    pub fn envelope_volume_m3(&self) -> f64 {
        self.shape.volume_m3()
    }

    /// The volume of actual SUBSTANCE in it, m³. Packing outside `0..=1` is clamped: more matter than
    /// space is not a denser arrangement, it is a broken definition.
    pub fn matter_volume_m3(&self) -> f64 {
        self.shape.volume_m3() * self.packing.clamp(0.0, 1.0)
    }
}

/// A named connection between two parts, by part name.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Connection {
    pub a: String,
    pub b: String,
    pub join: Join,
}

/// **An assembly.** Parts, and how they are joined.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Assembly {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    /// Free-text provenance — where these dimensions came from. Not decoration: an assembly whose
    /// numbers nobody can trace is the same defect as a material without `sources`.
    #[serde(default)]
    pub notes: String,
    /// **Precomputed bulk quantities — a CACHE of [`Assembly::derive`], never a declaration.**
    ///
    /// Robin (2026-08-03): *"While mass will emerge, since we can pre-calculate the mass we should do
    /// so with the assembly to save compute."* Correct, and it is the same rule the whole compiled
    /// format runs on (docs/64 §2): derive once offline, read fast at runtime, and **the sources win
    /// if the two ever disagree.**
    ///
    /// So this is `None` in a SOURCE definition — a hand-written assembly that carried a mass would be
    /// a number tracing to nothing (Law V), and worse, one that silently stops matching the geometry
    /// the first time a dimension changes. The compiler fills it, and
    /// [`Assembly::verify_cache`] is what keeps it honest.
    #[serde(default)]
    pub derived: Option<Derived>,
}

/// Bulk quantities computed from the parts. Cached to save the runtime the work, and checkable against
/// a fresh derivation at any time — which is what makes it a cache rather than an assertion.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct Derived {
    pub mass_kg: f64,
    /// The space the assembly occupies, m³ — envelopes, void included. What it displaces.
    pub envelope_volume_m3: f64,
    /// The volume of actual substance, m³. Equals the envelope for a solid assembly; less for anything
    /// containing a packed arrangement. The gap between the two IS the porosity.
    pub matter_volume_m3: f64,
    pub centre_of_mass_m: [f64; 3],
}

impl Assembly {
    /// Parse an assembly definition.
    pub fn from_json(text: &str) -> Result<Assembly, String> {
        serde_json::from_str(text).map_err(|e| format!("assembly definition: {e}"))
    }

    /// **Total mass, kg — DERIVED, never declared.** Volume times density, summed. An assembly that
    /// carried its own mass would disagree with its geometry the first time either changed.
    ///
    /// A part naming a material that is not in the catalogue is an error rather than a zero: silently
    /// weighing nothing is how a typo becomes a gun that floats.
    pub fn mass_kg(&self, mats: &[Material]) -> Result<f64, String> {
        let mut total = 0.0;
        for p in &self.parts {
            total += p.matter_volume_m3() * self.density_of(p, mats)?;
        }
        Ok(total)
    }

    /// Centre of mass in the assembly's frame, metres — from the parts, for the same reason.
    pub fn centre_of_mass_m(&self, mats: &[Material]) -> Result<[f64; 3], String> {
        let mut acc = [0.0f64; 3];
        let mut total = 0.0;
        for p in &self.parts {
            let m = p.matter_volume_m3() * self.density_of(p, mats)?;
            total += m;
            for k in 0..3 {
                acc[k] += p.at_m[k] * m;
            }
        }
        if total <= 0.0 {
            return Ok([0.0; 3]);
        }
        Ok([acc[0] / total, acc[1] / total, acc[2] / total])
    }

    /// The material fractions this assembly is made of, by MASS — the same mixture shape
    /// `terra::appearance::Appearance.mix` carries for a patch of ground, so a ship seen from orbit
    /// summarises exactly the way a continent does (docs/44, docs/64 §6).
    pub fn composition(&self, mats: &[Material]) -> Result<Vec<(usize, f32)>, String> {
        let mut mix: Vec<(usize, f32)> = Vec::new();
        let mut total = 0.0f64;
        for p in &self.parts {
            let idx = self.index_of(p, mats)?;
            let m = p.matter_volume_m3() * mats[idx].density as f64;
            total += m;
            match mix.iter_mut().find(|(i, _)| *i == idx) {
                Some((_, f)) => *f += m as f32,
                None => mix.push((idx, m as f32)),
            }
        }
        if total > 0.0 {
            for e in mix.iter_mut() {
                e.1 /= total as f32;
            }
        }
        mix.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(mix)
    }

    fn index_of(&self, p: &Part, mats: &[Material]) -> Result<usize, String> {
        mats.iter()
            .position(|m| m.id == p.material)
            .ok_or_else(|| format!("part '{}' names unknown material '{}'", p.name, p.material))
    }

    fn density_of(&self, p: &Part, mats: &[Material]) -> Result<f64, String> {
        Ok(mats[self.index_of(p, mats)?].density as f64)
    }

    /// **Compute the bulk quantities from the parts** — what the compiler runs once and caches.
    pub fn derive(&self, mats: &[Material]) -> Result<Derived, String> {
        Ok(Derived {
            mass_kg: self.mass_kg(mats)?,
            envelope_volume_m3: self.parts.iter().map(|p| p.envelope_volume_m3()).sum(),
            matter_volume_m3: self.parts.iter().map(|p| p.matter_volume_m3()).sum(),
            centre_of_mass_m: self.centre_of_mass_m(mats)?,
        })
    }

    /// **The cached numbers must equal what the geometry says.** If they differ the file is STALE and
    /// the parts are the truth — this is docs/64's one-way rule (`assets` and the catalogue are the
    /// sources; the compiled artifact is derived from them) applied to a single assembly.
    ///
    /// `Ok(())` when there is no cache: an uncompiled source assembly is not stale, it is just source.
    pub fn verify_cache(&self, mats: &[Material]) -> Result<(), String> {
        let Some(cached) = self.derived else {
            return Ok(());
        };
        let fresh = self.derive(mats)?;
        // Relative tolerance, because these are f64 sums over parts and an exact compare would fail on
        // reordering alone. Tight enough that a changed dimension cannot hide inside it.
        let off = |a: f64, b: f64| (a - b).abs() > 1e-9 * a.abs().max(b.abs()).max(1.0);
        if off(cached.mass_kg, fresh.mass_kg) {
            return Err(format!(
                "'{}' caches mass {} kg but its parts weigh {} kg — the file is stale; the parts win",
                self.id, cached.mass_kg, fresh.mass_kg
            ));
        }
        if off(cached.envelope_volume_m3, fresh.envelope_volume_m3) {
            return Err(format!(
                "'{}' caches an envelope volume of {} m3 but its parts occupy {}",
                self.id, cached.envelope_volume_m3, fresh.envelope_volume_m3
            ));
        }
        if off(cached.matter_volume_m3, fresh.matter_volume_m3) {
            return Err(format!(
                "'{}' caches {} m3 of matter but its parts contain {}",
                self.id, cached.matter_volume_m3, fresh.matter_volume_m3
            ));
        }
        for k in 0..3 {
            if off(cached.centre_of_mass_m[k], fresh.centre_of_mass_m[k]) {
                return Err(format!(
                    "'{}' caches a centre of mass its parts do not have (axis {k})",
                    self.id
                ));
            }
        }
        Ok(())
    }

    /// Find a part by name.
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.name == name)
    }
}

impl Assembly {
    /// **The assembly's own geometry, as a mesh** — derived from its parts, never authored beside them.
    ///
    /// Robin: the visible thing *"should be a product of the assembly and the engine"*. So there is no
    /// cannon model anywhere: the barrel is a tube because the assembly says it is a tube, the cheeks
    /// are slabs because the assembly says so, and each part wears its own material's colour and
    /// texture layer. Change a dimension in the JSON and the picture changes with the mass, because
    /// both come from the same statement of what is there.
    ///
    /// Cylinders and tubes lie along the assembly's +X, which is the axis a barrel runs on. `segments`
    /// is a RESOLUTION choice, not physics (docs/44): the same shape sampled more finely.
    pub fn mesh(&self, mats: &[Material], segments: usize) -> crate::mesher::Mesh {
        let mut out = crate::mesher::Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for p in &self.parts {
            let Some(mi) = mats.iter().position(|m| m.id == p.material) else {
                continue; // an unknown material draws nothing rather than drawing a lie
            };
            let (col, mat) = (mats[mi].albedo, mi as u32);
            let part = match p.shape {
                Shape::Sphere { r } => crate::mesher::build_uv_sphere(r as f32, mat, col, 12, 20),
                Shape::Cylinder { r, length } => {
                    crate::mesher::build_tube(r as f32, 0.0, length as f32, segments, mat, col)
                }
                Shape::Tube {
                    r_outer,
                    r_bore,
                    length,
                } => crate::mesher::build_tube(
                    r_outer as f32,
                    r_bore as f32,
                    length as f32,
                    segments,
                    mat,
                    col,
                ),
                Shape::Slab { x, y, z } => crate::mesher::build_box(
                    (x * 0.5) as f32,
                    (y * 0.5) as f32,
                    (z * 0.5) as f32,
                    mat,
                    col,
                ),
            };
            let base = out.vertices.len() as u32;
            for mut v in part.vertices {
                v.pos[0] += p.at_m[0] as f32;
                v.pos[1] += p.at_m[1] as f32;
                v.pos[2] += p.at_m[2] as f32;
                out.vertices.push(v);
            }
            out.indices
                .extend(part.indices.into_iter().map(|i| i + base));
        }
        out
    }
}

/// **Stand an assembly on a body's surface, facing a bearing** — the engine's answer to "put it on the
/// ground", so a scene never builds a transform by hand.
///
/// Robin: *"You should be able to tell the engine to place it on the ground; we should build that
/// feature into the engine if it doesn't exist."* It did not. Terra was assembling its own model matrix
/// from a tangent frame, which is a scene doing geometry it should be asking for.
///
/// The assembly's own axes are mapped onto the local frame at that coordinate: **+X along the bearing,
/// +Y up, +Z completing a right-handed set** — which is the convention `naval-24pdr-gun.json` is
/// authored in (barrel down +X, carriage hanging below at −Y). `surface_r` is the radius the assembly
/// stands ON, in display units, so a caller that knows its terrain height passes it and the assembly
/// sits on the ground rather than at the datum. `eye` makes the result camera-relative, the convention
/// every draw in this engine uses.
///
/// ★★ **The basis must be RIGHT-HANDED, and getting that wrong is nearly invisible.** A first version
/// used `up × forward` for the third axis, which is left-handed: determinant −1, so the model came out
/// MIRRORED, and under back-face culling a mirrored mesh renders inside-out — every face that should be
/// drawn is culled. The gun was there, occupying the right space, contributing thousands of changed
/// pixels to an A/B, and simply could not be seen. `a_placed_assembly_is_not_mirrored` pins the
/// determinant so it cannot come back.
pub fn place_on_surface(
    lat_deg: f64,
    lon_deg: f64,
    bearing_deg: f64,
    surface_r: f64,
    metres_to_display: f64,
    eye: glam::DVec3,
) -> glam::DMat4 {
    let (up, north, east) = crate::geo::tangent_frame(lat_deg, lon_deg);
    let b = bearing_deg.to_radians();
    let fwd = (north * b.cos() + east * b.sin()).normalize();
    // Right-handed: X cross Y = Z.
    let side = fwd.cross(up).normalize();
    let at = up * surface_r;
    let s = metres_to_display;
    glam::DMat4::from_cols(
        (fwd * s).extend(0.0),
        (up * s).extend(0.0),
        (side * s).extend(0.0),
        (at - eye).extend(1.0),
    )
}

/// **Hoop stress in a pressurised tube**, Pa — `σ = p·r/t` for a thin wall.
///
/// This is the number that decides whether a gun bursts, and it is why a barrel is loaded in the one
/// direction grey cast iron is weakest (tensile 295 MPa against ~930 compressive). Thin-wall is exact
/// only for `t << r`; a gun's breech is thick-walled, where Lamé's equation gives a HIGHER peak stress
/// at the bore. **Flagged (Law V): this under-predicts for a thick wall, so it errs toward saying a gun
/// survives** — the deferred computation is the Lamé thick-wall solution, and until it lands the
/// predicate is optimistic in exactly the direction that matters, which is stated here rather than
/// discovered by a gun that should have burst.
pub fn hoop_stress_pa(pressure_pa: f64, r_bore_m: f64, wall_m: f64) -> f64 {
    if wall_m <= 0.0 || r_bore_m <= 0.0 || pressure_pa <= 0.0 {
        return 0.0;
    }
    pressure_pa * r_bore_m / wall_m
}

/// **The COMPILED assemblies the engine ships**, baked in at build time.
///
/// `include_str!` rather than a file read, for the same reason `data/materials.json` is: the engine runs
/// in a browser where there is no filesystem, and an asset a scene has to fetch is an asset a scene can
/// forget to fetch. These are the COMPILED forms — `derived` already filled by `bin/compile-assemblies`
/// — so a runtime that wants a gun's mass reads it instead of summing thirteen parts.
///
/// ★ Baking the compiled form is only safe because it is checkable: `compile-assemblies --check` fails
/// if recompiling would change any of them, so a stale bake cannot ship quietly.
pub mod compiled {
    use super::Assembly;

    /// The 24-pounder naval gun, its service charge, and its round shot.
    pub const NAVAL_24PDR_GUN: &str =
        include_str!("../../../assets/assemblies/compiled/naval-24pdr-gun.json");
    pub const CHARGE_24PDR_SERVICE: &str =
        include_str!("../../../assets/assemblies/compiled/charge-24pdr-service.json");
    pub const ROUND_SHOT_24PDR: &str =
        include_str!("../../../assets/assemblies/compiled/round-shot-24pdr.json");

    /// Parse a baked assembly. Panics on malformed input, because a compiled asset that does not parse
    /// is a build error wearing a runtime error's clothes.
    pub fn parse(text: &str) -> Assembly {
        Assembly::from_json(text).expect("a compiled assembly parses")
    }
}

/// **The assemblies the engine SHIPS, loaded from `assets/assemblies/`** (test-only).
///
/// Same argument as `terra::raster::shipped`: a test that invents an assembly answers a question about
/// the test. Reading the one the engine actually carries asks the harder and more useful question, and
/// re-asks it on every commit instead of leaving it in a note that says it was true once.
#[cfg(test)]
pub(crate) mod shipped {
    use super::Assembly;

    pub fn load(id: &str) -> Assembly {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/assemblies")
            .join(format!("{id}.json"));
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Assembly::from_json(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// **A 24-pounder's shot: the ball's SIZE emerges from its NAME.** "Twenty-four pounder" names a
    /// mass of iron, and everything else about the gun follows from it — the bore that fits the ball,
    /// the charge that drives it, the metal thick enough to contain the charge.
    ///
    /// So this is the sharpest possible check that nothing is being declared: give the engine 24 pounds
    /// of cast iron as a sphere and ask how wide it is. Historical 24-pdr shot measured about 5.6-5.7
    /// inches, in a bore of about 5.8 with windage between them. If the derivation lands there, the
    /// geometry and the material density are both right and neither was typed in.
    #[test]
    fn a_twenty_four_pounders_ball_derives_its_own_diameter() {
        let mats = mats();
        let iron = &mats[crate::materials::index_of(&mats, "cast_iron")];
        const LB_KG: f64 = 0.45359237;
        let mass = 24.0 * LB_KG;
        // Invert sphere volume for the radius the mass implies — nothing declared but the mass.
        let v = mass / iron.density as f64;
        let r = (3.0 * v / (4.0 * std::f64::consts::PI)).cbrt();
        let inches = 2.0 * r / 0.0254;
        assert!(
            (5.4..5.9).contains(&inches),
            "24 lb of cast iron as a sphere should be ~5.6 in across; got {inches:.2} in"
        );
        // And the shape agrees with itself: build the part, ask its volume, recover the mass.
        let ball = Shape::Sphere { r };
        assert!(
            (ball.volume_m3() * iron.density as f64 - mass).abs() < 1e-9,
            "the shape must round-trip its own mass"
        );
    }

    /// **A barrel is a TUBE, and that is the point of the exercise** — an assembly format that could
    /// only describe spheres would be the original problem restated. A tube's mass depends on the
    /// difference of two squares, so getting it wrong is easy and silent.
    #[test]
    fn a_tube_weighs_its_metal_and_not_its_bore() {
        let solid = Shape::Cylinder {
            r: 0.2,
            length: 1.0,
        };
        let tube = Shape::Tube {
            r_outer: 0.2,
            r_bore: 0.1,
            length: 1.0,
        };
        // Boring out half the radius removes a QUARTER of the area, not half.
        let removed = 1.0 - tube.volume_m3() / solid.volume_m3();
        assert!(
            (removed - 0.25).abs() < 1e-12,
            "a bore of half the radius removes a quarter of the metal, got {removed}"
        );
        // Degenerate geometry weighs nothing rather than weighing negatively.
        assert_eq!(
            Shape::Tube {
                r_outer: 0.1,
                r_bore: 0.2,
                length: 1.0
            }
            .volume_m3(),
            0.0,
            "a bore wider than the barrel is not negative metal"
        );
    }

    /// **Mass and centre of mass are DERIVED**, and a barrel-heavy gun must prove it by having its
    /// centre of mass toward the breech — which is why trunnions sit forward of centre and why a gun
    /// crew could not simply lift one end.
    #[test]
    fn an_assemblys_mass_and_balance_come_from_its_parts() {
        let mats = mats();
        let a = Assembly {
            id: "t".into(),
            name: "t".into(),
            notes: String::new(),
            derived: None,
            connections: vec![],
            parts: vec![
                Part {
                    name: "breech".into(),
                    material: "gunmetal".into(),
                    shape: Shape::Cylinder {
                        r: 0.2,
                        length: 0.5,
                    },
                    at_m: [-1.0, 0.0, 0.0],
                    packing: 1.0,
                },
                Part {
                    name: "chase".into(),
                    material: "gunmetal".into(),
                    shape: Shape::Tube {
                        r_outer: 0.12,
                        r_bore: 0.074,
                        length: 1.5,
                    },
                    at_m: [0.5, 0.0, 0.0],
                    packing: 1.0,
                },
            ],
        };
        let m = a.mass_kg(&mats).expect("mass");
        // Computed from the two volumes and bronze's density — no figure typed here.
        let bronze = mats[crate::materials::index_of(&mats, "gunmetal")].density as f64;
        let want = (a.parts[0].shape.volume_m3() + a.parts[1].shape.volume_m3()) * bronze;
        assert!((m - want).abs() < 1e-6, "mass is volume x density, summed");
        assert!(m > 0.0);

        let com = a.centre_of_mass_m(&mats).expect("com");
        assert!(
            com[0] < 0.0,
            "the solid breech outweighs the thin chase, so the balance sits behind the middle: {com:?}"
        );
        // A part naming a material that does not exist must ERROR, never weigh nothing.
        let mut bad = a.clone();
        bad.parts[0].material = "unobtainium".into();
        assert!(
            bad.mass_kg(&mats).is_err(),
            "an unknown material is an error"
        );
    }

    /// **Composition summarises by mass, the same shape a patch of ground uses** — so a gun seen from
    /// orbit reduces exactly the way a continent does, and resolution-by-necessity needs no second
    /// vocabulary for man-made things.
    #[test]
    fn an_assembly_summarises_to_a_material_mixture() {
        let mats = mats();
        let a = Assembly {
            id: "t".into(),
            name: "t".into(),
            notes: String::new(),
            derived: None,
            connections: vec![],
            parts: vec![
                Part {
                    name: "barrel".into(),
                    material: "gunmetal".into(),
                    shape: Shape::Cylinder {
                        r: 0.2,
                        length: 1.0,
                    },
                    at_m: [0.0; 3],
                    packing: 1.0,
                },
                Part {
                    name: "bed".into(),
                    material: "oak".into(),
                    shape: Shape::Slab {
                        x: 1.0,
                        y: 0.2,
                        z: 0.6,
                    },
                    at_m: [0.0; 3],
                    packing: 1.0,
                },
            ],
        };
        let mix = a.composition(&mats).expect("composition");
        assert_eq!(mix.len(), 2);
        assert!(
            (mix.iter().map(|(_, f)| f).sum::<f32>() - 1.0).abs() < 1e-5,
            "fractions close to 1"
        );
        // Bronze is 11x denser than oak, so it dominates by mass despite the smaller volume — which is
        // the whole reason a mass fraction and a volume fraction are different questions.
        let bronze = crate::materials::index_of(&mats, "gunmetal");
        assert_eq!(mix[0].0, bronze, "the metal dominates by mass");
        assert!(mix[0].1 > 0.5);
    }

    /// **Hoop stress is what bursts a gun**, and this pins the relationship rather than a number:
    /// thinner walls and wider bores both raise it, and the metal's own strength is the limit.
    #[test]
    fn hoop_stress_rises_with_bore_and_falls_with_wall() {
        let p = 100.0e6; // 100 MPa, a plausible peak chamber pressure
        let base = hoop_stress_pa(p, 0.074, 0.10);
        assert!((base - p * 0.74).abs() < 1e3, "sigma = p*r/t");
        assert!(
            hoop_stress_pa(p, 0.074, 0.05) > base,
            "a thinner wall is worse"
        );
        assert!(
            hoop_stress_pa(p, 0.15, 0.10) > base,
            "a wider bore is worse"
        );
        assert_eq!(hoop_stress_pa(p, 0.074, 0.0), 0.0, "no wall, no answer");
        assert_eq!(hoop_stress_pa(0.0, 0.074, 0.1), 0.0);

        // ★★ **THE COMPARISON THAT DECIDES A GUN'S FATE, and my first assertion here was WRONG.**
        // I asserted that 100 MPa on a 10 cm wall exceeds grey iron's strength. It does not: hoop
        // stress is 74 MPa against a tensile strength of 295, so the gun HOLDS with a fourfold margin —
        // which is the more interesting fact, and the reason these guns worked at all. The arithmetic
        // refuted the intuition, as it should.
        let mats = mats();
        let iron = &mats[crate::materials::index_of(&mats, "cast_iron")];
        let bronze = &mats[crate::materials::index_of(&mats, "gunmetal")];
        assert!(
            base < iron.fracture_strength as f64,
            "a sound 24-pdr at a working pressure survives: {base:e} Pa against {} Pa",
            iron.fracture_strength
        );

        // So ask the useful question instead: what pressure DOES burst it? Invert the same relation —
        // nothing new, and nothing declared.
        let burst = |strength: f64, r: f64, t: f64| strength * t / r;
        let p_burst_iron = burst(iron.fracture_strength as f64, 0.074, 0.10);
        assert!(
            (3.5e8..4.5e8).contains(&p_burst_iron),
            "grey iron gives way near 400 MPa for this geometry, got {p_burst_iron:e}"
        );
        // Working pressures for black powder are well under that, which is exactly why a SOUND gun
        // fired safely and an overcharge did not. The margin is the story.
        assert!(
            p_burst_iron > 3.0 * base,
            "the margin a gunner was relying on"
        );
        // Bronze and iron are close in TENSION, so on strength alone they burst at similar pressures —
        // the difference between them is HOW, not WHEN, and that half needs docs/46 row 30's ductility
        // to reach the engine before it can be asserted here.
        let p_burst_bronze = burst(bronze.fracture_strength as f64, 0.074, 0.10);
        assert!(
            (0.6..1.6).contains(&(p_burst_bronze / p_burst_iron)),
            "the two metals burst at comparable pressures; what differs is the manner of failure"
        );
        // And a gun bored out thinner is proportionally weaker — the same relation, read backwards.
        assert!(
            burst(iron.fracture_strength as f64, 0.074, 0.05) < 0.6 * p_burst_iron,
            "halving the wall roughly halves the pressure it can take"
        );
    }
}

#[cfg(test)]
mod shipped_cannon_tests {
    use super::*;

    fn mats() -> Vec<Material> {
        crate::materials::load()
    }

    /// **THE CANNON, as three assemblies that the engine actually ships** — and every number below is
    /// derived from geometry and catalogued density. Nothing declares a mass.
    #[test]
    fn the_shipped_cannon_weighs_what_its_geometry_says() {
        let mats = mats();
        let gun = shipped::load("naval-24pdr-gun");
        let shot = shipped::load("round-shot-24pdr");
        let charge = shipped::load("charge-24pdr-service");

        // ★ The shot: 24 lb of cast iron. The RADIUS in the file must be what that mass implies —
        // recomputed here, so a hand-edited radius fails rather than quietly changing the gun's calibre.
        const LB: f64 = 0.45359237;
        let iron = &mats[crate::materials::index_of(&mats, "cast_iron")];
        let want_r =
            (3.0 * (24.0 * LB / iron.density as f64) / (4.0 * std::f64::consts::PI)).cbrt();
        let Shape::Sphere { r } = shot.part("ball").expect("a ball").shape else {
            panic!("the shot is a sphere")
        };
        assert!(
            (r - want_r).abs() < 1e-5,
            "the ball's radius must FOLLOW from 24 lb of cast iron: file {r:.6} m, derived {want_r:.6} m"
        );
        let shot_kg = shot.mass_kg(&mats).expect("shot mass");
        assert!(
            (shot_kg - 24.0 * LB).abs() < 1e-3,
            "and it weighs its own name: {shot_kg:.3} kg vs {:.3}",
            24.0 * LB
        );

        // ★ The charge: a service charge was about a third of the shot's weight.
        let charge_kg = charge.mass_kg(&mats).expect("charge mass");
        let powder_kg: f64 = ["saltpetre", "charcoal", "brimstone"]
            .iter()
            .map(|n| {
                let p = charge.part(n).expect("a powder part");
                p.matter_volume_m3()
                    * mats[crate::materials::index_of(&mats, &p.material)].density as f64
            })
            .sum();
        assert!(
            (powder_kg / shot_kg - 1.0 / 3.0).abs() < 0.06,
            "a service charge is ~1/3 the shot: {powder_kg:.2} kg powder to {shot_kg:.2} kg ball"
        );
        assert!(charge_kg > powder_kg, "the wad has mass too");

        // ★ The gun. Barrel mass emerges from five bronze segments; a 24-pdr barrel historically ran
        // 2.3-2.6 tonnes, and the file's own note says the stepped segments UNDER-count, so a lower
        // bound is what should be asserted rather than a match.
        let barrel_kg: f64 = [
            "breech",
            "first_reinforce",
            "second_reinforce",
            "chase",
            "muzzle_swell",
        ]
        .iter()
        .map(|n| {
            let p = gun.part(n).expect("a barrel part");
            p.matter_volume_m3()
                * mats[crate::materials::index_of(&mats, &p.material)].density as f64
        })
        .sum();
        assert!(
            (1500.0..2700.0).contains(&barrel_kg),
            "a 24-pdr barrel is a couple of tonnes of bronze; derived {barrel_kg:.0} kg"
        );
        let gun_kg = gun.mass_kg(&mats).expect("gun mass");
        assert!(
            gun_kg > barrel_kg,
            "the carriage adds to it: {gun_kg:.0} kg total"
        );

        // ★★ **CENTRE OF GRAVITY SITS BEHIND THE MIDPOINT** (Robin: *"a canon's will be back farther
        // from midpoint than the muzzle"*). It has to: the breech end is solid metal and thick-walled
        // while the chase is thin. That is why trunnions sit forward of centre, why a gun crew could
        // not lift the muzzle and the breech alike — and it is what decides how the thing topples, how
        // it settles into soft ground, and how it floats.
        let com = gun.centre_of_mass_m(&mats).expect("com");
        let (front, back) = gun.parts.iter().fold((f64::MIN, f64::MAX), |(f, b), p| {
            (f.max(p.at_m[0]), b.min(p.at_m[0]))
        });
        let midpoint = 0.5 * (front + back);
        assert!(
            com[0] < midpoint,
            "the balance must sit BEHIND the midpoint: com {:.3} m against midpoint {midpoint:.3} m",
            com[0]
        );
        // And below the barrel axis, because the carriage hangs under it — which is what keeps a gun
        // upright rather than rolling onto its side.
        assert!(
            com[1] < 0.0,
            "the carriage pulls the balance down: {:?}",
            com
        );
    }

    /// **A cached mass is a CACHE, and this is what keeps it one** (Robin: *"since we can pre-calculate
    /// the mass we should do so with the assembly to save compute"*).
    ///
    /// Source assemblies carry no `derived` block — a hand-written mass would be a number tracing to
    /// nothing. The compiler fills it; `verify_cache` proves it still matches the parts, and a stale
    /// one is an ERROR rather than a value the runtime quietly prefers.
    #[test]
    fn a_precomputed_mass_must_still_match_the_parts() {
        let mats = mats();
        let mut gun = shipped::load("naval-24pdr-gun");
        assert!(
            gun.derived.is_none(),
            "a SOURCE assembly declares no mass; the compiler derives it"
        );
        assert!(
            gun.verify_cache(&mats).is_ok(),
            "no cache is not a stale cache"
        );

        // Compile it: fill the cache from the geometry. Now it verifies.
        gun.derived = Some(gun.derive(&mats).expect("derive"));
        assert!(
            gun.verify_cache(&mats).is_ok(),
            "a fresh cache agrees with its parts"
        );
        let cached = gun.derived.expect("cache");
        assert!(cached.mass_kg > 0.0 && cached.envelope_volume_m3 > 0.0);

        // ★ Now change the GEOMETRY and leave the cache alone — a founder boring the barrel out wider.
        // The stale mass must be caught, not preferred.
        let idx = gun
            .parts
            .iter()
            .position(|p| p.name == "chase")
            .expect("chase");
        if let Shape::Tube { r_bore, .. } = &mut gun.parts[idx].shape {
            *r_bore += 0.005;
        }
        let err = gun
            .verify_cache(&mats)
            .expect_err("a stale cache must be an error");
        assert!(
            err.contains("stale") && err.contains("parts win"),
            "and it must say which side is the truth: {err}"
        );
    }

    /// **Packing: the shape is the ENVELOPE, and how much of it is matter is a separate question.**
    ///
    /// Robin (2026-08-03), on adding it: *"let's do the honest fix"*, and *"that will come in handy in a
    /// number of ways in future"* — snow, soil, gravel, rubble, a bag of sand, a stack of shot.
    ///
    /// The charge is the case that forced it. Sized to its SOLID volume, 8 lb of powder occupied half
    /// the chamber it really fills; and chamber volume sets the pressure a burn reaches, so that is a
    /// physics error rather than a cosmetic one. With packing, the shape is the space the powder fills
    /// and the mass still comes out at 8 lb.
    #[test]
    fn a_packed_charge_fills_its_chamber_while_weighing_what_its_matter_weighs() {
        let mats = mats();
        let charge = shipped::load("charge-24pdr-service");
        const LB: f64 = 0.45359237;

        let powder = ["saltpetre", "charcoal", "brimstone"];
        let mass: f64 = powder
            .iter()
            .map(|n| {
                let p = charge.part(n).expect("powder");
                p.matter_volume_m3()
                    * mats[crate::materials::index_of(&mats, &p.material)].density as f64
            })
            .sum();
        // ★ The MASS is the service charge: 8 lb of powder to a 24 lb ball.
        assert!(
            (mass - 8.0 * LB).abs() < 1e-3,
            "the powder weighs its service charge: {mass:.4} kg vs {:.4}",
            8.0 * LB
        );

        // ★ The ENVELOPE is what it fills: mass over the POURED bulk density of corned powder.
        let envelope: f64 = powder
            .iter()
            .map(|n| charge.part(n).expect("powder").envelope_volume_m3())
            .sum();
        const RHO_BULK: f64 = 1000.0; // poured corned powder
        assert!(
            (envelope - mass / RHO_BULK).abs() < 1e-6,
            "the charge fills mass/bulk-density of chamber: {:.1} cm3 against {:.1}",
            envelope * 1e6,
            mass / RHO_BULK * 1e6
        );

        // ★★ And the gap between them IS the porosity — matter in an arrangement, which is a property
        // of the arrangement and not of the substance. About half of a powder charge is void.
        let matter: f64 = powder
            .iter()
            .map(|n| charge.part(n).expect("powder").matter_volume_m3())
            .sum();
        let void = 1.0 - matter / envelope;
        assert!(
            (0.45..0.55).contains(&void),
            "corned powder is roughly half void; derived {:.1} percent",
            void * 100.0
        );

        // A solid part is unaffected: the wad is wood, packing 1, envelope == matter.
        let wad = charge.part("wad").expect("a wad");
        assert!(
            (wad.envelope_volume_m3() - wad.matter_volume_m3()).abs() < 1e-15,
            "a solid part's envelope IS its matter"
        );

        // Nonsense packing cannot create matter: more substance than space is a broken definition,
        // not a denser arrangement.
        let mut impossible = wad.clone();
        impossible.packing = 4.0;
        assert!(
            (impossible.matter_volume_m3() - wad.envelope_volume_m3()).abs() < 1e-15,
            "packing above 1 clamps rather than inventing matter"
        );
    }

    /// **The picture comes from the assembly** — every part contributes geometry, and a change to a
    /// dimension moves both the mass and the mesh, because both read the same statement of what is
    /// there. Robin: the visible thing *"should be a product of the assembly and the engine"*.
    #[test]
    fn an_assembly_draws_itself_from_its_own_parts() {
        let mats = mats();
        let gun = shipped::load("naval-24pdr-gun");
        let m = gun.mesh(&mats, 16);
        assert!(!m.vertices.is_empty() && !m.indices.is_empty());
        assert_eq!(m.indices.len() % 3, 0, "triangles");
        let n = m.vertices.len() as u32;
        assert!(m.indices.iter().all(|&i| i < n), "every index is real");

        // The mesh must SPAN the gun: its extent along the barrel axis matches the parts' own layout,
        // so nothing was dropped and nothing was drawn at the origin by mistake.
        let (lo, hi) = m.vertices.iter().fold((f32::MAX, f32::MIN), |(a, b), v| {
            (a.min(v.pos[0]), b.max(v.pos[0]))
        });
        let (plo, phi) = gun.parts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| {
            (a.min(p.at_m[0]), b.max(p.at_m[0]))
        });
        assert!(
            (hi - lo) as f64 > (phi - plo) * 0.8,
            "the mesh spans the assembly: {:.2} m of mesh against {:.2} m of layout",
            hi - lo,
            phi - plo
        );

        // ★ A BORED barrel must be hollow — the bore wall exists, or looking down the muzzle shows
        // nothing and the tube is only a tube from outside.
        let bore_r = gun.parts.iter().find_map(|p| match p.shape {
            Shape::Tube { r_bore, .. } => Some(r_bore as f32),
            _ => None,
        });
        let bore_r = bore_r.expect("the gun has a bore");
        let inward = m
            .vertices
            .iter()
            .filter(|v| {
                let d = (v.pos[1] * v.pos[1] + v.pos[2] * v.pos[2]).sqrt();
                (d - bore_r).abs() < 1e-3 && (v.nrm[1] * v.pos[1] + v.nrm[2] * v.pos[2]) < 0.0
            })
            .count();
        assert!(inward > 0, "the bore wall is drawn, facing inward");

        // Changing a dimension changes the picture — the mesh is not a separate authored thing.
        let mut wider = gun.clone();
        let i = wider.parts.iter().position(|p| p.name == "chase").unwrap();
        if let Shape::Tube { r_outer, .. } = &mut wider.parts[i].shape {
            *r_outer += 0.05;
        }
        let m2 = wider.mesh(&mats, 16);
        assert_ne!(
            m.vertices
                .iter()
                .map(|v| v.pos[1] as f64)
                .sum::<f64>()
                .to_bits(),
            m2.vertices
                .iter()
                .map(|v| v.pos[1] as f64)
                .sum::<f64>()
                .to_bits(),
            "a changed dimension must change the geometry"
        );
    }

    /// **A placed assembly must not be MIRRORED**, and this is the test that would have saved an
    /// evening. `up × forward` is left-handed; under back-face culling a mirrored mesh renders
    /// inside-out and the object is invisible while still occupying exactly the right space — so an A/B
    /// says "something is drawn" and a screenshot shows nothing.
    #[test]
    fn a_placed_assembly_is_not_mirrored() {
        for &(lat, lon, bearing) in &[
            (0.0, 0.0, 0.0),
            (-51.0, -75.0, 240.0),
            (45.0, 100.0, 90.0),
            (-89.0, 12.0, 315.0),
        ] {
            let m = place_on_surface(lat, lon, bearing, 1.0, 1.0, glam::DVec3::ZERO);
            let b = glam::DMat3::from_cols(
                m.x_axis.truncate(),
                m.y_axis.truncate(),
                m.z_axis.truncate(),
            );
            let det = b.determinant();
            assert!(
                (det - 1.0).abs() < 1e-9,
                "the basis at {lat},{lon} bearing {bearing} must be right-handed and unscaled; \
                 determinant {det} (negative means MIRRORED, and a mirrored mesh is culled inside-out)"
            );
            // Orthonormal, or the assembly is sheared as well as turned.
            for (i, a) in [b.x_axis, b.y_axis, b.z_axis].iter().enumerate() {
                assert!((a.length() - 1.0).abs() < 1e-9, "axis {i} is not unit");
            }
            assert!(b.x_axis.dot(b.y_axis).abs() < 1e-9);
            assert!(b.y_axis.dot(b.z_axis).abs() < 1e-9);
        }

        // +Y is UP: the carriage hangs below the barrel, so which way is up decides whether the gun
        // stands on its wheels or on its muzzle.
        let m = place_on_surface(0.0, 0.0, 0.0, 1.0, 1.0, glam::DVec3::ZERO);
        let up = crate::geo::tangent_frame(0.0, 0.0).0;
        assert!(
            m.y_axis.truncate().dot(up) > 0.999,
            "the assembly's +Y must be the local up"
        );
        // +X follows the bearing: due north at bearing 0.
        let north = crate::geo::tangent_frame(0.0, 0.0).1;
        assert!(
            m.x_axis.truncate().dot(north) > 0.999,
            "the assembly's +X must point along the bearing"
        );
        // The scale really scales, and the translation is camera-relative.
        let scaled = place_on_surface(0.0, 0.0, 0.0, 2.0, 3.0, glam::DVec3::new(1.0, 0.0, 0.0));
        assert!((scaled.x_axis.truncate().length() - 3.0).abs() < 1e-9);
        assert!((scaled.w_axis.truncate() - (up * 2.0 - glam::DVec3::X)).length() < 1e-9);
    }
}
