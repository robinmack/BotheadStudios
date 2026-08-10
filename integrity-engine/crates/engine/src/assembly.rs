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
    /// ★ **A hollow SPHERE: a planet's layer.** `Tube` is already this idea for cylinders, and its
    /// absence was the first concrete thing blocking a planet from being an assembly (docs/67 §5):
    /// nested `Sphere`s double-count their own interiors, so a core inside a mantle inside a crust
    /// weighed several Earths. The innermost layer is the degenerate case with `r_inner = 0`, which is
    /// a solid sphere and needs no separate variant.
    Shell {
        r_inner: f64,
        r_outer: f64,
    },
}

impl Shape {
    /// Volume in m³. Zero for nonsense dimensions rather than a negative volume, which would silently
    /// subtract mass from an assembly.
    pub fn volume_m3(&self) -> f64 {
        use std::f64::consts::PI;
        match *self {
            Shape::Shell { r_inner, r_outer } => {
                let (a, b) = (r_inner.max(0.0), r_outer.max(0.0));
                if b <= a {
                    0.0 // an inside-out shell is a broken definition, not negative matter
                } else {
                    4.0 / 3.0 * PI * (b * b * b - a * a * a)
                }
            }
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

    /// ★★ **How far this shape REACHES from its own centre**, metres — the radius of the smallest
    /// sphere about that centre that contains it.
    ///
    /// This is not [`Shape::equivalent_radius_m`] and the difference is the point: the equal-VOLUME
    /// radius answers *"how big is this, roughly"* and is right for summarising a distant object,
    /// while this answers *"where does it END"* and is the only one that can bound anything. A 3 m
    /// barrel 0.1 m across has an equivalent radius of 0.19 m and a reach of 1.5 m — a factor of eight,
    /// and in the direction that matters.
    pub fn reach_m(&self) -> f64 {
        match *self {
            Shape::Sphere { r } => r.max(0.0),
            // A shell reaches its OUTER radius: the hollow middle is not where it ends.
            Shape::Shell { r_outer, .. } => r_outer.max(0.0),
            // Corner of the cylinder: the rim of an end cap.
            Shape::Cylinder { r, length } => r.max(0.0).hypot(0.5 * length.max(0.0)),
            Shape::Tube {
                r_outer, length, ..
            } => r_outer.max(0.0).hypot(0.5 * length.max(0.0)),
            // Half the body diagonal.
            Shape::Slab { x, y, z } => {
                0.5 * (x.max(0.0).powi(2) + y.max(0.0).powi(2) + z.max(0.0).powi(2)).sqrt()
            }
        }
    }

    /// **Half-extents along the shape's own axes**, metres — `x` is the axis the primitive is built
    /// along (`+X`, the cannon-barrel convention that `Part::along` rotates), `y` and `z` are across it.
    ///
    /// This exists so a caller can ask what a part occupies in a chosen DIRECTION rather than as one
    /// number. `reach_m` answers "how far from the centre", which is the right question for an
    /// assembly's outer boundary and the wrong one for its footprint: a grass blade is 0.35 m long and
    /// 4 mm wide, and reading its length as a radius makes a tuft a third of a metre across.
    pub fn half_extents_m(&self) -> glam::DVec3 {
        match *self {
            Shape::Sphere { r } => glam::DVec3::splat(r.max(0.0)),
            Shape::Shell { r_outer, .. } => glam::DVec3::splat(r_outer.max(0.0)),
            Shape::Cylinder { r, length } => {
                glam::DVec3::new(0.5 * length.max(0.0), r.max(0.0), r.max(0.0))
            }
            Shape::Tube {
                r_outer, length, ..
            } => glam::DVec3::new(0.5 * length.max(0.0), r_outer.max(0.0), r_outer.max(0.0)),
            Shape::Slab { x, y, z } => {
                glam::DVec3::new(0.5 * x.max(0.0), 0.5 * y.max(0.0), 0.5 * z.max(0.0))
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
    /// **Which way this part POINTS**, a unit vector in the assembly's own frame. Default `+X`.
    ///
    /// ★ Added 2026-08-04, and it was overdue in two independent places. A cylinder was always built
    /// along X because the first assembly was a CANNON and a barrel points along X — so the first tree
    /// came out as a trunk lying flat on the ground, invisible, and the tufts of grass with it. A part
    /// having a position but no direction is the same gap `docs/46` row 30 names for wood: *"a PART has
    /// no grain DIRECTION at all, so even a reader of those fields would not know which way the plank
    /// runs"*. Orthotropic strength needs exactly this vector, and so do rolled steel, composite layup
    /// and bedded sandstone.
    ///
    /// Defaulting to `+X` leaves every existing assembly bit-identical, which is why the cannon's
    /// thirteen parts did not have to change.
    #[serde(default = "along_x")]
    pub along: [f64; 3],
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
    ///
    /// ★★ **FLAGGED (Law V): packing is a LOSSY SUMMARY OF AN ASSEMBLY, and the things it loses are
    /// the ones that decide behaviour.** Robin, on being told compression could be folded into it
    /// (2026-08-05): *"We know the difference between sand and gravel. Or sand and sandstone."*
    ///
    /// Random close packing is ~0.6 for **both** sand and gravel — what differs is GRAIN SIZE, which
    /// this number cannot hold. Sand and sandstone are the same grains at nearly the same packing, and
    /// one flows while the other holds a cliff, because in sandstone the grains are CEMENTED. So the
    /// resolved counterpart is not a better fraction: it is the grains themselves, as an assembly, with
    /// a size and with or without [`Connection`]s between them — and the connection half of that is
    /// already in this type, since *where an assembly breaks is decided by how it was fastened*.
    ///
    /// ★ This is why packing must NOT absorb compression, which is the other reason a part's matter can
    /// be denser or thinner than its catalogue entry. Compression is not an arrangement at all —
    /// peridotite at 4500 kg/m³ is not packed differently from peridotite at 3300, it is the same
    /// grains squeezed — and its resolved counterpart is the EOS at the local pressure, not a geometry.
    /// One number for two physics would make the engine unable to say which one it was in. See
    /// [`Part::in_situ_density`].
    #[serde(default = "one")]
    pub packing: f64,
    /// ★★ **The density this part's matter is actually AT (kg/m³), when it is not the catalogue's
    /// reference.** `None` — the default and the case for every made object — means the material's own
    /// density, and every existing assembly is bit-identical.
    ///
    /// This is NOT `packing`, and conflating them was the trap. Packing is VOID: a powder charge is
    /// matter with air between it, so it is bounded by 1 and the substance is unchanged. This is
    /// COMPRESSION: the same matter squeezed into less space, which runs the other way — Earth's lower
    /// mantle is peridotite at 4500 kg/m³ against a catalogue reference near 3300, a 36% increase that
    /// no packing fraction can express. Attempting to make a planet an assembly is what surfaced it
    /// (docs/67 §5 predicted three obstacles and missed this one).
    ///
    /// **FLAGGED (Law V), with its resolved counterpart named and already built**: an in-situ density
    /// is not a property of a substance, it is a STATE at a pressure, and this engine can compute it —
    /// `planet::LayeredBody::pressure_at` gives the overburden and `eos.rs` (Tillotson, pinned to Benz
    /// & Asphaug 1999) gives the response. Earth's values here are PREM, i.e. measured rather than
    /// invented, which is why they are admissible as a declaration in the meantime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_situ_density: Option<f64>,
}

/// serde default for [`Part::packing`] — solid unless stated.
fn one() -> f64 {
    1.0
}

fn along_x() -> [f64; 3] {
    [1.0, 0.0, 0.0]
}

impl Part {
    /// **How far this part reaches from the ASSEMBLY's origin**, metres — its offset plus its own
    /// reach. Conservative where the part's long axis is not radial, exact where it is; either way it
    /// is a bound that contains the part, which is what a boundary has to be.
    pub fn reach_m(&self) -> f64 {
        let d = (self.at_m[0].powi(2) + self.at_m[1].powi(2) + self.at_m[2].powi(2)).sqrt();
        d + self.shape.reach_m()
    }

    /// The space this part takes up, m³ — its shape, void included.
    pub fn envelope_volume_m3(&self) -> f64 {
        self.shape.volume_m3()
    }

    /// **How far this part reaches SIDEWAYS from the assembly's axis**, metres — its footprint radius,
    /// with the part standing where and pointing how it says.
    ///
    /// The horizontal sibling of [`Part::reach_m`], and it exists because a crown is a horizontal
    /// question. Both matter: an assembly ENDS at its outermost boundary (`reach_m`), and it COVERS
    /// the ground out to this. Reading one for the other is what made a 0.35 m grass blade claim a
    /// 0.35 m crown, which through `plants per m² = cover ÷ crown` would have thinned a pasture by
    /// thirty-fold.
    /// ★ Exact per shape, not via a bounding box. A box's horizontal corner over-reads a SPHERE by
    /// √2 — measured: the oak's 18 m crown reported 509 m² of ground instead of 254, which would have
    /// halved the trees in every forest on the planet.
    pub fn crown_radius_m(&self) -> f64 {
        let along = glam::DVec3::from(self.along).normalize_or(glam::DVec3::X);
        // How much of the part's own axis lies in the horizontal plane. A blade standing straight up
        // reaches sideways by its width; the same blade laid flat reaches by its length.
        let axis_h = (along.x * along.x + along.z * along.z)
            .sqrt()
            .clamp(0.0, 1.0);
        let r = match self.shape {
            Shape::Sphere { r } => r.max(0.0),
            Shape::Shell { r_outer, .. } => r_outer.max(0.0),
            // A capped cylinder's farthest horizontal point is on an end rim, and which point that is
            // depends on the tilt: `sup(t) = (L/2)·t + r·√(1−t²)` over the tilt `t` the axis allows.
            // Unconstrained the maximum sits at `t* = (L/2)/√((L/2)²+r²)`; a less-tilted part is
            // capped by its own axis instead.
            Shape::Cylinder { r, length }
            | Shape::Tube {
                r_outer: r, length, ..
            } => {
                let (hl, r) = (0.5 * length.max(0.0), r.max(0.0));
                let hyp = hl.hypot(r);
                let t = if hyp > 0.0 {
                    (hl / hyp).min(axis_h)
                } else {
                    0.0
                };
                hl * t + r * (1.0 - t * t).max(0.0).sqrt()
            }
            // A box's extreme really is a vertex, so eight of them settle it exactly.
            Shape::Slab { .. } => {
                let h = self.shape.half_extents_m();
                let rot =
                    glam::DMat3::from_quat(glam::DQuat::from_rotation_arc(glam::DVec3::X, along));
                (0..8)
                    .map(|c| {
                        let s = glam::DVec3::new(
                            if c & 1 == 0 { -h.x } else { h.x },
                            if c & 2 == 0 { -h.y } else { h.y },
                            if c & 4 == 0 { -h.z } else { h.z },
                        );
                        let p = rot * s;
                        p.x.hypot(p.z)
                    })
                    .fold(0.0f64, f64::max)
            }
        };
        (self.at_m[0].powi(2) + self.at_m[2].powi(2)).sqrt() + r
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
    /// ★★ **WHERE THIS ASSEMBLY ENDS**, metres from its own origin — the outermost boundary of its
    /// outermost component, and nothing else.
    ///
    /// Robin, stating the rule after correcting a narrower one of mine (2026-08-05): *"'A body ends
    /// where its AIR ends' is not accurate, as bodies are assemblies. **An assembly ends at the
    /// outermost boundary of the assembly.**"* The context was Earth: its atmosphere is a COMPONENT,
    /// so Earth reaches ~97 km past its rock — but naming the rule after air would teach the engine a
    /// special case, when the identical question is asked by a tree's canopy, a ship's mast, a
    /// cannon's muzzle and a planet's air. One rule; whichever component is outermost wins.
    ///
    /// An assembly with no parts reaches nowhere, which is correct rather than an error: it is not
    /// there.
    ///
    /// ★ This exists because things downstream were INFERRING it — a rig scanning pixel columns for
    /// the edge of a planet, a ballistics helper reaching for the equal-volume radius because there
    /// was nothing else to reach for. A boundary is a fact the assembly holds.
    pub fn reach_m(&self) -> f64 {
        self.parts.iter().map(Part::reach_m).fold(0.0, f64::max)
    }

    /// **The ground this assembly's `material` parts cover**, m² — its crown.
    ///
    /// A crown is the footprint of the parts made of one substance: a tree's leaves and not its
    /// trunk, a tussock's blades and not the soil it roots in. That is what
    /// `plants per m² = cover fraction ÷ crown` divides by, so it is a property of the ASSEMBLY and
    /// belongs here rather than being re-derived by whatever needs a density.
    ///
    /// ★ It is the outermost boundary of the outermost matching component, taken HORIZONTALLY —
    /// Robin's rule for where an assembly ends, asked about the ground instead of about space.
    pub fn crown_m2(&self, material: &str) -> f64 {
        let r = self
            .parts
            .iter()
            .filter(|p| p.material == material)
            .map(Part::crown_radius_m)
            .fold(0.0f64, f64::max);
        std::f64::consts::PI * r * r
    }

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

    /// The density this part's matter is at: its measured in-situ value where one is declared, the
    /// catalogue's reference otherwise. The material must exist either way — a part that names nothing
    /// real is an error even when it carries its own density.
    fn density_of(&self, p: &Part, mats: &[Material]) -> Result<f64, String> {
        let reference = mats[self.index_of(p, mats)?].density as f64;
        Ok(p.in_situ_density.filter(|d| *d > 0.0).unwrap_or(reference))
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
        self.mesh_damaged(mats, segments, None)
    }

    /// ★★ **THE SAME MESH, MINUS WHAT IS GONE** (docs/70). `integrity[i] <= 0` means that part no
    /// longer exists, so it is not drawn — the render REPORTING the model's damage, which is Law VI in
    /// the direction it is meant to run. A shorter slice than `parts` leaves the rest pristine, exactly
    /// as `instance::Damage` defines it.
    ///
    /// Partial damage still draws the whole part today. ★ FLAGGED with its name: a blade 40% broken
    /// should be drawn 40% shorter, and the honest version is the part's own geometry scaled along its
    /// `along` axis — cheap, and worth doing once something can break a part part-way and be looked at.
    pub fn mesh_damaged(
        &self,
        mats: &[Material],
        segments: usize,
        integrity: Option<&[f64]>,
    ) -> crate::mesher::Mesh {
        let mut out = crate::mesher::Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for (pi, p) in self.parts.iter().enumerate() {
            if integrity.and_then(|v| v.get(pi)).is_some_and(|&i| i <= 0.0) {
                continue;
            }
            let Some(mi) = mats.iter().position(|m| m.id == p.material) else {
                continue; // an unknown material draws nothing rather than drawing a lie
            };
            let (col, mat) = (mats[mi].albedo, mi as u32);
            let part = match p.shape {
                Shape::Sphere { r } => crate::mesher::build_uv_sphere(r as f32, mat, col, 12, 20),
                // A shell is drawn as its OUTER surface: from outside, a planet's crust is what you
                // see, and the interior is only visible to something that has cut it open. When that
                // exists it is the same mesh with the inner surface added, not a different rule.
                Shape::Shell { r_outer, .. } => {
                    crate::mesher::build_uv_sphere(r_outer as f32, mat, col, 12, 20)
                }
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
            // **Point the part the way it says it points.** The primitives are all built along +X
            // (the cannon's barrel came first), so this rotates +X onto `along` before placing. A tree
            // whose trunk runs along +Y was otherwise a stick lying flat on the ground.
            let along = glam::DVec3::from(p.along).normalize_or(glam::DVec3::X);
            let rot = glam::DMat3::from_quat(glam::DQuat::from_rotation_arc(glam::DVec3::X, along));
            let base = out.vertices.len() as u32;
            for mut v in part.vertices {
                let q = rot * glam::DVec3::new(v.pos[0] as f64, v.pos[1] as f64, v.pos[2] as f64);
                let n = rot * glam::DVec3::new(v.nrm[0] as f64, v.nrm[1] as f64, v.nrm[2] as f64);
                v.pos = [
                    (q.x + p.at_m[0]) as f32,
                    (q.y + p.at_m[1]) as f32,
                    (q.z + p.at_m[2]) as f32,
                ];
                v.nrm = [n.x as f32, n.y as f32, n.z as f32];
                out.vertices.push(v);
            }
            out.indices
                .extend(part.indices.into_iter().map(|i| i + base));
        }
        out
    }
}

/// **What arrives at a piece of matter** (docs/70). Energy, where it entered, and where it is going —
/// in the assembly's own frame, in metres and joules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arriving {
    pub energy_j: f64,
    /// Where the event enters the assembly's frame, metres.
    pub at_m: glam::DVec3,
    /// Unit direction of travel.
    pub along: glam::DVec3,
}

/// What one part did with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartHit {
    /// Index into `Assembly::parts`.
    pub part: usize,
    /// Joules this part took out of the event.
    pub spent_j: f64,
    /// How much of it was broken, `0..=1`. This is what `instance::Damage::part_integrity` holds:
    /// `integrity = 1 - broken`.
    pub broken: f64,
}

/// The whole answer: the debit, and what happened on the way.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Met {
    pub spent_j: f64,
    /// What continues past this assembly — to the next thing, or to the ground.
    pub remaining_j: f64,
    pub hits: Vec<PartHit>,
}

impl Assembly {
    /// ★★★ **WHAT THIS MUCH ENERGY DOES TO ME** (docs/70) — the verb that makes an assembly matter
    /// rather than scenery.
    ///
    /// Robin, on being told the plants were decoration (2026-08-09): *"Each impact with each assembly
    /// can be debited from the total energy of a collision for zero loss… but that way we could have a
    /// rock fall on a haystack, unsettle the hay (dry grass), but the impact could be absorbed."* This
    /// is that debit. Energy enters, every part on its path takes what it costs to break, and the
    /// remainder leaves. **Spent + remaining = arriving, exactly** — conservation is a property of the
    /// arithmetic here, not something checked afterwards.
    ///
    /// ★ **It is the engine's OWN law at a different scale.** `damage::crater_volume(E, σ) = E/σ` is
    /// what decides a meteor's crater on a planet (`interaction::respond`); read backwards it says what
    /// a PART costs to destroy, `E = σ·V`. This calls that function rather than restating the ratio, so
    /// a grass blade and a continent are the same arithmetic (Law II).
    ///
    /// ★★ **FLAGGED — FRACTURE IS THE ONLY CHANNEL OPEN** (docs/70 §4). Energy arriving at matter can
    /// also DISPLACE it (½mv²), COMPACT it (crushing porosity, which is what a haystack really does),
    /// or HEAT it (`oxidation::apply_heat`, `damage::classify`). Those are named IOUs with their real
    /// quantities, not omissions: a rock through straw is under-resisted here until compaction lands,
    /// and this is said plainly because that is precisely the case that prompted the design.
    ///
    /// Geometry is a ray against each part's bounding sphere — coarse, and honest about it: a part is
    /// either on the path or it is not, and a blade's exact silhouette does not change the energy
    /// arithmetic by more than the ordering of two neighbours.
    pub fn meet(&self, mats: &[Material], a: &Arriving) -> Met {
        let mut met = Met {
            spent_j: 0.0,
            remaining_j: a.energy_j.max(0.0),
            hits: Vec::new(),
        };
        if met.remaining_j <= 0.0 {
            return met;
        }
        let dir = a.along.normalize_or(glam::DVec3::Y);
        // Parts on the path, nearest first — the order energy actually meets them in.
        let mut on_path: Vec<(f64, usize)> = Vec::new();
        for (i, p) in self.parts.iter().enumerate() {
            let c = glam::DVec3::from(p.at_m);
            let r = p.shape.reach_m();
            let to_c = c - a.at_m;
            let t = to_c.dot(dir);
            // Behind the entry point is behind the event; it meets nothing there.
            if t < -r {
                continue;
            }
            if (to_c - dir * t).length() <= r {
                on_path.push((t, i));
            }
        }
        on_path.sort_by(|x, y| x.0.total_cmp(&y.0));

        for (_, i) in on_path {
            if met.remaining_j <= 0.0 {
                break;
            }
            let p = &self.parts[i];
            let Some(m) = mats.iter().find(|m| m.id == p.material) else {
                continue; // unknown matter resists nothing rather than resisting infinitely
            };
            let strength = m.fracture_strength as f64;
            let volume = p.matter_volume_m3();
            if volume <= 0.0 {
                continue;
            }
            // What this energy could break of THIS material, and what the part actually offers.
            let breakable = crate::damage::crater_volume(met.remaining_j, strength);
            let broken = (breakable / volume).min(1.0);
            // The cost of what was broken, by the same law read backwards.
            let spent = if breakable >= volume {
                strength * volume
            } else {
                met.remaining_j
            };
            met.spent_j += spent;
            met.remaining_j = (met.remaining_j - spent).max(0.0);
            met.hits.push(PartHit {
                part: i,
                spent_j: spent,
                broken,
            });
        }
        met
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
    // Both halves back to back, so existing callers read the same while there is exactly one piece of
    // geometry underneath and it is the model's. ★ `surface_r` arrives ALREADY in display units — the
    // conflation this pair exists to end — so the position needs no further conversion (1.0) and the
    // caller's `metres_to_display` is doing duty as the size scale.
    let p = stand_on_body(lat_deg, lon_deg, bearing_deg, surface_r);
    model_of(&p, 1.0, metres_to_display, eye)
}

/// **STANDING ON A BODY, as a model question** — where a body's surface puts something, in metres, in
/// that body's own frame, with no camera and no display scale anywhere in it. [`model_of`] is the
/// separate, later question of how to DRAW that answer, and the seam between them is docs/68's whole
/// point: the model says where matter is, the renderer says how to show it.
///
/// Robin, 2026-08-09, on a test called *"a tuft of grass lands where the SCENE puts it"*: *"that is a
/// horrible test; it now should land where the ASSEMBLY THAT CONTAINS IT puts it."* The name was
/// reporting the architecture accurately — the scene really was doing the placing, in display units,
/// relative to the eye, three different things in one expression.
///
/// ★★ **AND THE CONTAINER STILL DOES NOT CALL THIS** (docs/46 row 55). Robin, immediately after, and
/// she was right to check: *"that 'its container places a plant' may not be accurate yet. That is the
/// model I decreed, but I'm not certain the source matches that model."* It does not. What this
/// changes is that the geometry is now a model question with no viewer in it, and that
/// `instance::Placement` finally has a consumer (row 46). What it does NOT change is WHO asks:
/// `Terra::build_flora` still does, so a plant is still placed by a scene walking a list. The
/// container asking on its own behalf needs Earth to be an assembly that holds contents, which is
/// docs/67 §5 and is not built. **Do not describe this as containment until it is.**
///
/// The assembly's own axes map onto the local frame: **+X along the bearing, +Y up, +Z completing a
/// right-handed set** — the convention `naval-24pdr-gun.json` is authored in.
pub fn stand_on_body(
    lat_deg: f64,
    lon_deg: f64,
    bearing_deg: f64,
    surface_r_m: f64,
) -> crate::instance::Placement {
    let (up, north, east) = crate::geo::tangent_frame(lat_deg, lon_deg);
    let b = bearing_deg.to_radians();
    let fwd = (north * b.cos() + east * b.sin()).normalize();
    // Right-handed: X cross Y = Z. See the note above — the left-handed version renders inside-out.
    let side = fwd.cross(up).normalize();
    crate::instance::Placement {
        at_m: up * surface_r_m,
        attitude: glam::DQuat::from_mat3(&glam::DMat3::from_cols(fwd, up, side)),
        within: None,
    }
}

/// **How to DRAW a placement** — the renderer half: the model matrix that carries an assembly's own
/// vertices into this frame's camera-relative, display-scaled space.
///
/// `size_scale` multiplies the assembly's own dimensions (a real stand of grass is not clones);
/// `display_per_m` is the scene's metres-to-display factor; `eye` is where the camera is, in display
/// units. None of the three is a fact about the world — which is exactly why they live here and not in
/// [`stand_on_body`].
pub fn model_of(
    p: &crate::instance::Placement,
    display_per_m: f64,
    size_scale: f64,
    eye: glam::DVec3,
) -> glam::DMat4 {
    let m = glam::DMat3::from_quat(p.attitude);
    let s = display_per_m * size_scale;
    glam::DMat4::from_cols(
        (m.x_axis * s).extend(0.0),
        (m.y_axis * s).extend(0.0),
        (m.z_axis * s).extend(0.0),
        (p.at_m * display_per_m - eye).extend(1.0),
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

    /// **The plants**, so a scene that names Earth gets what grows on it without fetching anything.
    pub const BROADLEAF_TREE_OAK: &str =
        include_str!("../../../assets/assemblies/compiled/broadleaf-tree-oak.json");
    pub const CONIFER_TREE_SPRUCE: &str =
        include_str!("../../../assets/assemblies/compiled/conifer-tree-spruce.json");
    pub const GRASS_TUFT: &str =
        include_str!("../../../assets/assemblies/compiled/grass-tuft.json");

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
                    along: along_x(),
                    name: "breech".into(),
                    material: "gunmetal".into(),
                    shape: Shape::Cylinder {
                        r: 0.2,
                        length: 0.5,
                    },
                    at_m: [-1.0, 0.0, 0.0],
                    packing: 1.0,
                    in_situ_density: None,
                },
                Part {
                    along: along_x(),
                    name: "chase".into(),
                    material: "gunmetal".into(),
                    shape: Shape::Tube {
                        r_outer: 0.12,
                        r_bore: 0.074,
                        length: 1.5,
                    },
                    at_m: [0.5, 0.0, 0.0],
                    packing: 1.0,
                    in_situ_density: None,
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
                    along: along_x(),
                    name: "barrel".into(),
                    material: "gunmetal".into(),
                    shape: Shape::Cylinder {
                        r: 0.2,
                        length: 1.0,
                    },
                    at_m: [0.0; 3],
                    packing: 1.0,
                    in_situ_density: None,
                },
                Part {
                    along: along_x(),
                    name: "bed".into(),
                    material: "oak".into(),
                    shape: Shape::Slab {
                        x: 1.0,
                        y: 0.2,
                        z: 0.6,
                    },
                    at_m: [0.0; 3],
                    packing: 1.0,
                    in_situ_density: None,
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

#[cfg(test)]
mod plant_tests {
    use super::*;

    /// **A plant is an assembly like any other** — catalogued matter with a shape, and a mass that
    /// DERIVES. Nothing about these is plant-specific machinery.
    ///
    /// Robin (2026-08-04): *"low-cost models for grasses and trees (low poly for now but reasonably
    /// faithful to the real thing)… stored as assemblies themselves and added as members of Earth so
    /// they can be rendered as called for."*
    #[test]
    fn a_tree_weighs_what_its_geometry_and_its_wood_say_it_does() {
        let mats = crate::materials::load();
        let oak = shipped::load("broadleaf-tree-oak");
        let spruce = shipped::load("conifer-tree-spruce");
        let tuft = shipped::load("grass-tuft");

        // A mature oak and a mature spruce are both a few tonnes. These are not asserted constants —
        // they are what the sourced dimensions and the catalogued woods come to, and the range is the
        // range real trees of these dimensions occupy.
        let m_oak = oak
            .mass_kg(&mats)
            .expect("the oak's materials are catalogued");
        let m_spr = spruce
            .mass_kg(&mats)
            .expect("the spruce's materials are catalogued");
        assert!(
            (3_000.0..12_000.0).contains(&m_oak),
            "a 20 m oak with a 1.2 m bole should be a few tonnes, got {m_oak:.0} kg"
        );
        assert!(
            (3_000.0..7_000.0).contains(&m_spr),
            "a 30 m Norway spruce is 3-6 t, got {m_spr:.0} kg. ★ It was 9.5 t when the bole was ONE \
             cylinder — a constant-radius cylinder is not a trunk. The fix was the taper, not a \
             smaller radius picked to land on a nicer number."
        );
        // A tuft of grass is grams, and it had better not be kilograms.
        let m_tuft = tuft.mass_kg(&mats).expect("grass is catalogued");
        assert!(
            (0.001..0.5).contains(&m_tuft),
            "a tuft of grass weighs grams, got {m_tuft:.4} kg"
        );
    }

    /// **★★ A CROWN IS MOSTLY AIR, and `packing` is what says so.**
    ///
    /// The crown's packing is `LAI x leaf thickness / crown depth` — both inputs measured (temperate
    /// deciduous LAI ~5.5 m²/m², deciduous woody lamina 0.25 ± 0.08 mm). So the leaf mass is a
    /// CONSEQUENCE of the tree's size rather than a number anybody chose, and this checks it lands
    /// where a real tree's foliage does: tens to a couple of hundred kilograms, against a bole of
    /// several tonnes.
    ///
    /// Getting this wrong is not subtle. A crown modelled as SOLID foliage would be a 3,000 m³ sphere
    /// of leaf at 440 kg/m³ — over a thousand tonnes hanging off a six-tonne trunk.
    #[test]
    fn a_crown_is_leaves_and_air_not_a_solid_ball_of_leaf() {
        let mats = crate::materials::load();
        let oak = shipped::load("broadleaf-tree-oak");
        let crown = oak
            .parts
            .iter()
            .find(|p| p.name == "crown")
            .expect("the oak has a crown");
        let envelope = crown.shape.volume_m3();
        let leaf_m3 = envelope * crown.packing;
        let leaf_kg =
            leaf_m3 * mats[crate::materials::index_of(&mats, &crown.material)].density as f64;
        assert!(
            envelope > 2_000.0,
            "an 18 m crown encloses thousands of cubic metres, got {envelope:.0}"
        );
        assert!(
            (20.0..400.0).contains(&leaf_kg),
            "a mature oak carries tens to hundreds of kg of leaf, got {leaf_kg:.1} kg from \
             {envelope:.0} m³ of crown at packing {:.2e}",
            crown.packing
        );
        // The absurdity this guards against, stated numerically.
        let solid =
            envelope * mats[crate::materials::index_of(&mats, &crown.material)].density as f64;
        assert!(
            solid > 1_000_000.0 && leaf_kg < solid / 1_000.0,
            "a SOLID crown would be {:.0} t; the packed one is {leaf_kg:.0} kg, three orders down",
            solid / 1000.0
        );
    }

    /// **★★★ ONE PLANT, TWO REPRESENTATIONS — and the far view must agree with the near one.**
    ///
    /// Robin (2026-08-04): *"These are hues at altitude but must become realistic flora at very low
    /// altitude."* That is Law IV exactly — the camera changes representation, never existence — and
    /// it is only honest if the two answers MATCH. A tree drawn as geometry up close and as an albedo
    /// from orbit must be the same tree, or the engine has two Earths again, separated by altitude
    /// instead of by scene.
    ///
    /// So: the material a plant's crown is made of is the SAME material its land-cover class contributes
    /// to the ground's albedo. That is the invariant, and it is what makes the far view a summary of the
    /// near one rather than a different claim about the same place.
    #[test]
    fn the_plant_you_walk_up_to_is_made_of_what_the_ground_looked_like_from_orbit() {
        let mats = crate::materials::load();
        let body: crate::terra::world_def::World = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/bodies/earth.json"
            ))
            .expect("earth.json"),
        )
        .expect("earth.json parses");
        let surface = body.surface.expect("Earth has a surface");

        for (plant, class, why) in [
            ("broadleaf-tree-oak", "4", "deciduous broadleaf forest"),
            ("conifer-tree-spruce", "1", "evergreen needleleaf forest"),
            ("grass-tuft", "10", "grassland"),
        ] {
            let a = shipped::load(plant);
            let foliage: Vec<&str> = a
                .parts
                .iter()
                .map(|p| p.material.as_str())
                .filter(|m| m.contains("foliage") || *m == "grass")
                .collect();
            assert!(!foliage.is_empty(), "{plant} has no foliage at all");
            let mix = surface
                .biomes
                .get(class)
                .unwrap_or_else(|| panic!("earth.json has no class {class} ({why})"));
            for f in &foliage {
                assert!(
                    mix.iter().any(|(id, _)| id == f),
                    "{plant} is made of `{f}`, but land-cover class {class} ({why}) — the class it \
                     grows in — does not contain it. Then the ground seen from orbit and the plant \
                     seen from a metre away are different matter, and the camera has changed what is \
                     TRUE rather than how finely it is computed (Law IV)."
                );
            }
        }
    }
}

#[cfg(test)]
mod reach_tests {
    use super::*;

    fn part(name: &str, shape: Shape, at: [f64; 3]) -> Part {
        Part {
            name: name.into(),
            material: "iron".into(),
            shape,
            along: [1.0, 0.0, 0.0],
            at_m: at,
            packing: 1.0,
            in_situ_density: None,
        }
    }

    /// ★★ **An assembly ends at its OUTERMOST component — not its biggest, not its heaviest.**
    ///
    /// Robin, 2026-08-05: *"An assembly ends at the outermost boundary of the assembly."* A ship is
    /// bounded by its MAST, which is neither its largest part nor its most massive, and any boundary
    /// taken from volume or mass gets it wrong.
    #[test]
    fn an_assembly_ends_at_its_outermost_component_whichever_that_is() {
        let hull = part(
            "hull",
            Shape::Slab {
                x: 20.0,
                y: 4.0,
                z: 6.0,
            },
            [0.0, 0.0, 0.0],
        );
        let mast = part(
            "mast",
            Shape::Cylinder {
                r: 0.2,
                length: 18.0,
            },
            [0.0, 12.0, 0.0],
        );
        let ship = Assembly {
            id: "ship".into(),
            name: "ship".into(),
            parts: vec![hull.clone(), mast.clone()],
            connections: vec![],
            notes: String::new(),
            derived: None,
        };
        assert!(
            hull.shape.volume_m3() > 100.0 * mast.shape.volume_m3(),
            "the hull is by far the larger part"
        );
        assert!(
            (ship.reach_m() - mast.reach_m()).abs() < 1e-9,
            "the mast bounds the ship ({:.2} m), not the hull ({:.2} m)",
            mast.reach_m(),
            hull.reach_m()
        );

        // A component farther out moves the boundary, however small it is.
        let mut flagged = ship.clone();
        flagged.parts.push(part(
            "pennant",
            Shape::Slab {
                x: 0.01,
                y: 0.3,
                z: 0.6,
            },
            [0.0, 21.5, 0.0],
        ));
        assert!(flagged.reach_m() > ship.reach_m());

        // An assembly with nothing in it reaches nowhere. Not an error — it is not there.
        assert_eq!(
            Assembly {
                parts: vec![],
                ..ship
            }
            .reach_m(),
            0.0
        );
    }

    /// **Reach answers "where does it end", `equivalent_radius_m` answers "how big is it" — and taking
    /// one for the other was a live error** in `ballistics::fire`, which read a gun's lowest matter off
    /// the equal-VOLUME radius. For a barrel the two differ by more than six times.
    #[test]
    fn reach_is_not_the_equal_volume_radius() {
        let barrel = Shape::Tube {
            r_outer: 0.16,
            r_bore: 0.14,
            length: 3.0,
        };
        assert!(
            barrel.reach_m() > 6.0 * barrel.equivalent_radius_m(),
            "a 3 m barrel reaches {:.2} m and summarises as {:.2} m",
            barrel.reach_m(),
            barrel.equivalent_radius_m()
        );
        // A sphere is the one shape where they agree — which is why a sphere-only engine never notices.
        let ball = Shape::Sphere { r: 0.07 };
        assert!((ball.reach_m() - ball.equivalent_radius_m()).abs() < 1e-12);
    }

    /// The real cannon, so the rule is exercised on a compiled assembly and not only on a fixture.
    #[test]
    fn a_real_assembly_contains_every_one_of_its_parts() {
        for (id, text) in [
            ("naval-24pdr-gun", compiled::NAVAL_24PDR_GUN),
            ("broadleaf-tree-oak", compiled::BROADLEAF_TREE_OAK),
            ("conifer-tree-spruce", compiled::CONIFER_TREE_SPRUCE),
        ] {
            let a = compiled::parse(text);
            let reach = a.reach_m();
            assert!(reach > 0.0, "{id} has an extent");
            for p in &a.parts {
                assert!(
                    p.reach_m() <= reach + 1e-9,
                    "{id}: part {} reaches {:.3} m, past the assembly's {:.3} m",
                    p.name,
                    p.reach_m(),
                    reach
                );
            }
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    /// **Reproduce `Terra::build_flora`'s transform natively and look at where a tuft of grass ends up.**
    ///
    /// The scene reports 1,200 plants meshed at 43,200 triangles and draws none of them. Everything
    /// upstream has been checked by reading and found consistent, which is exactly the confidence level
    /// this repo has learned not to trust — so this does the arithmetic instead.
    ///
    /// A tuft 3 m from the anchor should come out ~4.7e-7 display units away from it (3 m at
    /// 1/6371000 per metre) and ~0.35 m tall in the same units. If it does, the placement is right and
    /// the fault is downstream, on the GPU side.
    /// ★★ **A CLUMP IS THE SAME MATTER AS THE TUFT IT REPLACED, RESOLVED** (Robin, 2026-08-09:
    /// *"grass clumps… multiple strands in a clump that can be handled as one unit"*, and *"grass is
    /// blades, not hexagons or columns"*).
    ///
    /// The cylinder declared a 0.12 × 0.35 m envelope at packing 2.571e-3, which is grassland LAI 3.0
    /// times a 0.30 mm blade over its height. Both forms are therefore *leaf area × thickness* and must
    /// weigh the same to the last significant figure — the geometry got finer and nothing was invented.
    /// If a future edit changes the blade count, the width has to move with it or this fails, which is
    /// the point: **the count and the width are one number.** The count is set by the measured blade
    /// width (Lolium perenne, 2-3 mm) — 32 blades of 3.03 mm.
    #[test]
    fn a_grass_clump_weighs_exactly_what_the_cylinder_it_replaced_weighed() {
        let mats = crate::materials::load();
        let clump = compiled::parse(compiled::GRASS_TUFT);
        assert_eq!(clump.parts.len(), 32, "a clump is blades, not one column");

        // What the single cylinder held: envelope × packing.
        let cylinder_matter = std::f64::consts::PI * 0.06f64.powi(2) * 0.35 * 0.002_571_43;
        let got: f64 = clump.parts.iter().map(Part::matter_volume_m3).sum();
        assert!(
            (got - cylinder_matter).abs() / cylinder_matter < 1e-4,
            "the clump must be the same matter: {got:.6e} m³ vs the cylinder's {cylinder_matter:.6e}"
        );

        // And every blade is solid: with the strands resolved there is no arrangement left to summarise.
        assert!(clump.parts.iter().all(|p| p.packing == 1.0));

        // The crown it reports is the crown it was defined with — the leaf-area derivation divided by
        // this same radius, so the two cannot be allowed to drift apart.
        let want = std::f64::consts::PI * 0.06f64 * 0.06;
        let crown = clump.crown_m2("grass");
        // 0.5% of AREA is 0.25% of radius — 0.08 mm on a 60 mm crown. That is the authored file's own
        // 9-decimal rounding, not slack in the physics: the splay was solved against R exactly.
        assert!(
            (crown - want).abs() / want < 5e-3,
            "a 0.12 m clump covers {want:.5} m² of ground, reports {crown:.5}"
        );
        // ★ The trap this replaced: a 0.35 m blade read as a 0.35 m radius would claim 0.385 m².
        assert!(crown < 0.05, "a blade's LENGTH is not the clump's radius");
    }

    /// A crown is a HORIZONTAL question and `reach_m` answers a spherical one. Pinned on the oak
    /// because that is where the difference bit: treating its crown sphere as a bounding box reported
    /// **509 m²** of shaded ground instead of 254, which through `plants per m² = cover ÷ crown` puts
    /// half as many trees in every forest.
    #[test]
    fn a_crown_is_the_ground_covered_not_the_bounding_box_corner() {
        let oak = compiled::parse(compiled::BROADLEAF_TREE_OAK);
        let crown = oak.crown_m2("broadleaf_foliage");
        let want = std::f64::consts::PI * 9.0 * 9.0; // an 18 m crown
        assert!(
            (crown - want).abs() / want < 0.05,
            "an 18 m crown covers ~{want:.0} m², got {crown:.0}"
        );
        // The tree still REACHES higher than it is wide — the two questions stay distinct.
        assert!(oak.reach_m() > 9.0);
    }

    /// ★★ **WHERE A PLANT STANDS IS A MODEL QUESTION, AND THIS TEST USED TO ASK IT OF A SCENE.**
    ///
    /// Robin, 2026-08-09: *"`a_tuft_of_grass_lands_where_the_scene_puts_it` is a horrible test; it now
    /// should land where the assembly that CONTAINS it puts it."* The old name was honest about a
    /// dishonest arrangement — it computed a display-scaled, eye-relative matrix and called the result
    /// a placement, so "where the plant is" could not be asked without a camera in the room.
    ///
    /// It now asks `stand_on_body` in METRES, in the body's own frame, and only then hands the answer
    /// to the renderer. ★ Robin's own caution, recorded because it is correct: this is not yet
    /// containment — `Terra::build_flora` is still the caller, and Earth does not hold contents that
    /// could place themselves (docs/46 row 55). What is fixed is that the question no longer has a
    /// viewer in it; who does the asking is a separate, unfinished thing.
    #[test]
    fn a_plant_stands_on_the_body_before_any_camera_exists() {
        let clump = compiled::parse(compiled::GRASS_TUFT);
        let planet_radius = 6_371_000.0f64;
        let (lat, lon) = (45.3f64, -69.0f64);
        // Standing on ground 120 m above the datum, facing east.
        let p = stand_on_body(lat, lon, 90.0, planet_radius + 120.0);

        // It is ON the body, at the radius it was told, under its own coordinate.
        assert!((p.at_m.length() - (planet_radius + 120.0)).abs() < 1e-6);
        let up = crate::geo::dir_from_lat_lon(lat, lon);
        assert!(
            p.at_m.normalize().dot(up) > 1.0 - 1e-12,
            "under its own lat/lon"
        );

        // And it stands UP: the assembly's own +Y — the axis its blades run along — is the local
        // zenith. This is the failure `Part::along` was added for, asked of the placement instead of
        // of a matrix: the first tree came out lying flat on the ground.
        let m = glam::DMat3::from_quat(p.attitude);
        assert!(
            m.y_axis.dot(up) > 1.0 - 1e-9,
            "its up is the body's up here"
        );
        assert!(
            m.determinant() > 0.0,
            "right-handed, or it renders inside-out"
        );

        // NOTHING above needed a camera, a display scale or a scene. Those enter only now, and only to
        // DRAW it — the renderer's half, which cannot move the plant because it is handed the answer.
        let ds = 1.0 / planet_radius;
        let eye = up * (planet_radius * ds); // the camera at the datum under the plant
        let model = model_of(&p, ds, 1.0, eye);
        let mats = crate::materials::load();
        let mesh = clump.mesh(&mats, 6);
        assert!(!mesh.vertices.is_empty(), "the clump has geometry at all");
        let top = mesh
            .vertices
            .iter()
            .map(|v| {
                let q = model.transform_point3(glam::DVec3::new(
                    v.pos[0] as f64,
                    v.pos[1] as f64,
                    v.pos[2] as f64,
                ));
                (q - (up * (planet_radius + 120.0) * ds - eye)).dot(up) / ds
            })
            .fold(f64::MIN, f64::max);
        assert!(
            (0.25..0.40).contains(&top),
            "0.35 m of blade above its own ground, got {top:.3} m"
        );
    }

    #[test]
    fn a_tuft_of_grass_lands_where_the_scene_puts_it() {
        let mats = crate::materials::load();
        let tuft = compiled::parse(compiled::GRASS_TUFT);
        let mesh = tuft.mesh(&mats, 6);
        assert!(!mesh.vertices.is_empty(), "the tuft has geometry at all");

        // The scene's own numbers.
        let planet_radius = 6_371_000.0f64;
        let ds = 1.0 / planet_radius; // display_scale(): a planet radius is 1.0
        let r_disp = planet_radius * ds;
        let (lat, lon) = (45.3f64, -69.0f64);

        // The anchor: the surface point under the camera, in display units (ground_disp = 0 here).
        let anchor = crate::geo::dir_from_lat_lon(lat, lon) * r_disp;

        // A tuft 3 m north of it.
        let dlat = lat + 3.0 / 111_320.0;
        let model = place_on_surface(dlat, lon, 0.0, r_disp, ds, anchor);

        let local: Vec<glam::DVec3> = mesh
            .vertices
            .iter()
            .map(|v| {
                model.transform_point3(glam::DVec3::new(
                    v.pos[0] as f64,
                    v.pos[1] as f64,
                    v.pos[2] as f64,
                ))
            })
            .collect();

        let far = local.iter().map(|p| p.length()).fold(0.0f64, f64::max);
        let far_m = far / ds;
        println!(
            "tuft 3 m away: vertices reach {far:.3e} display units = {far_m:.3} m from the anchor"
        );
        assert!(
            (2.0..5.0).contains(&far_m),
            "a tuft 3 m away with 0.35 m of blade should sit 3-ish metres from the anchor, not {far_m:.3}"
        );

        // ★ AND IT MUST STAND UP. Its blades run along the local UP, not flat on the ground — the
        // failure `Part::along` was added to fix. Measured as height above the tangent plane.
        let up = crate::geo::dir_from_lat_lon(dlat, lon);
        let base = up * r_disp - anchor;
        let height_m = local
            .iter()
            .map(|p| (*p - base).dot(up) / ds)
            .fold(f64::MIN, f64::max);
        println!("tuft height above its own ground: {height_m:.3} m");
        assert!(
            height_m > 0.25,
            "the tuft stands up (0.35 m of blade), it does not lie flat: {height_m:.3} m"
        );
    }
}

#[cfg(test)]
mod meet_tests {
    use super::*;

    fn falling_rock(joules: f64, from_above: bool) -> Arriving {
        Arriving {
            energy_j: joules,
            at_m: if from_above {
                glam::DVec3::new(0.0, 20.0, 0.0)
            } else {
                glam::DVec3::new(-20.0, 0.5, 0.0)
            },
            along: if from_above {
                glam::DVec3::NEG_Y
            } else {
                glam::DVec3::X
            },
        }
    }

    /// ★★★ **THE TEST THAT MAKES THE OTHER TWO TRUSTWORTHY** (docs/70 §6). Robin's mechanism was
    /// *"debited from the total energy of a collision for ZERO LOSS"*, so conservation is not a nice
    /// property here — it is the definition. Spent plus remaining must equal what arrived, to the last
    /// float, for every material and every energy.
    #[test]
    fn spent_plus_remaining_is_exactly_what_arrived() {
        let mats = crate::materials::load();
        let clump = compiled::parse(compiled::GRASS_TUFT);
        let oak = compiled::parse(compiled::BROADLEAF_TREE_OAK);
        for a in [&clump, &oak] {
            for j in [1.0e-3, 1.0, 1.0e3, 1.0e6, 1.0e12] {
                let met = a.meet(&mats, &falling_rock(j, true));
                let total = met.spent_j + met.remaining_j;
                assert!(
                    (total - j).abs() <= j * 1e-12,
                    "{}: {j:.3e} J in, {total:.6e} J accounted for",
                    a.id
                );
                // And no part may claim more than it cost, or less than nothing.
                let summed: f64 = met.hits.iter().map(|h| h.spent_j).sum();
                assert!((summed - met.spent_j).abs() <= met.spent_j * 1e-12);
                assert!(met.hits.iter().all(|h| (0.0..=1.0).contains(&h.broken)));
            }
        }
    }

    /// ★★ **A ROCK ONTO GRASS CARRIES ON; THE SAME ROCK INTO AN OAK STOPS.** The picture Robin drew of
    /// what an assembly obeying physics would mean, and the reason it belongs to the assembly: the two
    /// answers differ only by the matter, not by any code that knows what a tree is.
    #[test]
    fn grass_barely_slows_a_rock_and_an_oak_stops_it() {
        let mats = crate::materials::load();
        let clump = compiled::parse(compiled::GRASS_TUFT);
        let oak = compiled::parse(compiled::BROADLEAF_TREE_OAK);
        // A 5 kg rock at 10 m/s — a stone dropped from about five metres.
        let joules = 0.5 * 5.0 * 10.0f64.powi(2);

        let through_grass = clump.meet(&mats, &falling_rock(joules, true));
        let into_oak = oak.meet(&mats, &falling_rock(joules, false));
        println!(
            "250 J rock: grass takes {:.3} J ({:.4}%), oak takes {:.1} J ({:.1}%)",
            through_grass.spent_j,
            through_grass.spent_j / joules * 100.0,
            into_oak.spent_j,
            into_oak.spent_j / joules * 100.0
        );

        assert!(
            through_grass.spent_j / joules < 0.05,
            "a clump of grass should barely notice a falling rock, it took {:.1}%",
            through_grass.spent_j / joules * 100.0
        );
        assert!(
            !through_grass.hits.is_empty(),
            "but it must MEET it — the whole point of row 57"
        );
        assert!(
            into_oak.spent_j / joules > 0.9,
            "an oak should stop it, it took only {:.1}%",
            into_oak.spent_j / joules * 100.0
        );
        assert!(
            into_oak.remaining_j < joules * 0.1,
            "and little should continue past the trunk"
        );
    }

    /// Energy that meets nothing passes through untouched — the airless-control equivalent. If this
    /// ever spends anything, the debit has acquired an opinion of its own.
    #[test]
    fn a_miss_costs_nothing() {
        let mats = crate::materials::load();
        let clump = compiled::parse(compiled::GRASS_TUFT);
        let past = Arriving {
            energy_j: 1000.0,
            at_m: glam::DVec3::new(50.0, 20.0, 0.0), // fifty metres to the side
            along: glam::DVec3::NEG_Y,
        };
        let met = clump.meet(&mats, &past);
        assert_eq!(met.spent_j, 0.0);
        assert_eq!(met.remaining_j, 1000.0);
        assert!(met.hits.is_empty());
    }

    /// ★ **A BLADE IS BROKEN BEFORE THE WHOLE CLUMP IS.** Damage is per PART, which is what makes
    /// `instance::Damage::part_integrity` meaningful: a clump a boot has stepped on is not a clump that
    /// no longer exists.
    ///
    /// Each blade costs `σ·V` = 15 kPa × 3.18e-7 m³ ≈ **4.8 mJ** to break, so the energies here are
    /// derived from the matter rather than picked: a millijoule cannot finish even one, and 50 mJ
    /// finishes about ten and leaves one part-way through.
    #[test]
    fn energy_breaks_blades_one_at_a_time_and_stops_when_it_runs_out() {
        let mats = crate::materials::load();
        let clump = compiled::parse(compiled::GRASS_TUFT);

        // A millijoule cannot break a single blade — it is spent part-way through the first one, and
        // NOTHING continues. That is the debit working, not a miss.
        let tiny = clump.meet(&mats, &falling_rock(1.0e-3, true));
        assert_eq!(tiny.hits.len(), 1, "it stops at the first blade it meets");
        assert!(tiny.hits[0].broken > 0.0 && tiny.hits[0].broken < 1.0);
        assert_eq!(tiny.remaining_j, 0.0, "and nothing passes through");

        // Fifty millijoules gets through about ten blades and is caught by the next.
        let some = clump.meet(&mats, &falling_rock(0.05, true));
        let fully = some.hits.iter().filter(|h| h.broken >= 1.0).count();
        let partly = some
            .hits
            .iter()
            .filter(|h| h.broken > 0.0 && h.broken < 1.0)
            .count();
        println!(
            "50 mJ into a clump: {fully} blades broken through, {partly} part-way, {} met, {:.4} J left",
            some.hits.len(),
            some.remaining_j
        );
        assert!(
            fully >= 5,
            "50 mJ should get through several blades, got {fully}"
        );
        assert!(
            fully < clump.parts.len(),
            "but not the whole clump — {fully} of {}",
            clump.parts.len()
        );
        assert!(partly <= 1, "at most one blade is caught mid-break");

        // And enough energy levels the clump and carries on, which is the rock in the test above.
        let plenty = clump.meet(&mats, &falling_rock(10.0, true));
        assert!(plenty.hits.iter().all(|h| h.broken >= 1.0));
        assert!(
            plenty.remaining_j > 9.0,
            "a clump of grass does not stop a rock"
        );
    }
}
