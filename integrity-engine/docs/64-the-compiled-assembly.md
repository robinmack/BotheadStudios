# docs/64 — The compiled assembly: matter, described once, validated by physics, read fast

Robin (2026-08-02), on watching the appearance integral find nothing to integrate and hearing what the
materials expansion would cost:

> *"I'm curious now that Earth is becoming more complex if we could create a use-case optimized binary
> representation of Earth that could serve up very quick sections as needed, complete with
> biomes/geographical features as needed (rather than having to rebuild every time from JSON as we do
> now). Essentially compile an 'earth' from data and material sources for time savings (one high cost
> compile and done/fast)."*

Then the generalisation, which is the more important half:

> *"I suggest we make it capable of supporting multiple objects, too. It may be that we want to model an
> ocean liner or a spaceship, treating it as an assembly of materials in a binary format seems to make
> sense in all these cases."*

And then the instruction that shapes the rest of this document:

> *"This will be brand new; please let's blue sky this for optimization, materials, joining type, etc.
> Even atmosphere can be baked in to some degree, **with the engine's physics guiding/validating
> construction**."*

**A planet and an ocean liner are not different kinds of thing to this engine.** Both are matter: some
arrangement of catalogued materials in space. They differ in how that arrangement is best INDEXED —
radial shells for a star, a quadtree over a sphere for a planet's surface, a graph of joined parts for a
hull — and not at all in what is being described. docs/63 already said a scene is *"an ASSEMBLY placed
at a coordinate"*. This is that sentence given a file format.

And the last clause is the one that makes this more than a serialisation exercise: **the compiler runs
the engine's own physics against what it is building, and refuses to emit an assembly that the physics
says cannot exist.** "One high cost compile" is a budget. This is what to spend it on.

---

## 0. What this must hold — and why a planet is only one case

Robin, correcting a reading of this document that had drifted orbital (2026-08-02):

> *"while now we're focusing on orbital, we're building a general purpose game engine. At some point we
> may be allowing people to explore the jungle and see Mayan temples (constructions) or the Pyramids, or
> Paris. We may spend an entire game in a small section of Puerto Rico walking around. We are building
> solid physics to be used by all kinds of things... the collison engine may one day be used to simulate
> gunpowder in a cannon, the shot, and the cannonball splintering an enemy ship."*

> *"We're getting closer to simulating the gemini missions, and that is REALLY cool. But we are building
> the physics to demonstrate it works, and will be applying it at different scales in different
> settings."*

**The orbital work is the DEMONSTRATION, not the destination.** That sentence belongs at the top of this
document because a format designed around compiling planets would quietly make everything else a special
case, and the special cases are where the games actually are. The centre of gravity here is the
ASSEMBLY. **A planet is simply the assembly whose spatial index happens to be a sphere.**

Five requirements follow that a planet-shaped design would have missed, and each is a real constraint
rather than an aspiration:

1. **Assemblies are PLACED, and they place on surfaces.** A Mayan temple stands on jungle terrain; the
   Pyramids sit on a plateau. So an assembly carries a placement (body, coordinate, orientation) and the
   SURF quadtree must be able to record what stands on it — a foundation cuts and bears on the ground.
   docs/63 said it first: *"a scene is an assembly placed at a coordinate ... everything else comes free
   whether we use it or not."* A construction is not a scene that owns a planet.
2. **A part carries its material's ORIENTATION, not only its identity** (docs/46 row 30). `oak` is
   already catalogued with 90 MPa tensile along the grain against 5.5 MPa across it — **that 16x ratio
   IS splintering** — and a plank with no grain direction cannot express it however good the material
   data is. Rolled steel and composite layup are the same requirement.
3. **Joins are a FAILURE model, not a static-load check.** §6 and §7 first asked *"does this join carry
   its load?"*, which is the shipwright's question. A cannonball asks a different one: *given this
   impulse, what breaks, and into what pieces?* Both come from the same material properties, but only
   the second produces splinters, and the format must carry what fragmentation needs (join geometry and
   extent, not merely capacity).
4. **The BOTTOM of the scale range is where the game lives.** *"an entire game in a small section of
   Puerto Rico walking around"* happens between a millimetre and a hundred metres. The scale ladder has
   been measured downward from Mars distance; the settings will be measured upward from a footprint. So
   the SURF tree must descend far past any global dataset, and matter-on-demand (docs/63 item 3) is core
   work rather than a finishing touch.
5. **Authoring is a first-class input.** Paris does not compile out of a satellite dataset. It is
   authored assemblies plus measured data plus instancing at city scale, so the SOURCE form of an
   assembly matters as much as the binary, and "scenes as data" (docs/46 row 14) stops being a tidiness
   argument and becomes the thing that lets anyone build content at all.

★★★ **THE ACCEPTANCE TEST FOR THE WHOLE ARCHITECTURE** (Robin, 2026-08-02): *"As long as we can build a
working cannon and a working planet, and put a working cannon on a working planet and fire it, we know
our assembly build is sound."*

That is one sentence and it exercises every claim this document makes: a PARTS assembly (the cannon), a
STACK+SURF assembly (the planet), an assembly PLACED on a body at a coordinate (§0 item 1), chemical
energy becoming gas becoming motion (the gap at row 31), ballistics through that planet's real gravity
and real air, and a shot landing on real terrain. **Nothing in it can be faked without another part of
it failing**, which is what makes it an acceptance test rather than a demo.

★★ **A SHIP'S CANNON, not a barrel on the ground** (Robin): *"the cannon needs to be more than just a
capped cylinder; it will need a way to place/hold it on the ground. Possibly a ship cannon would be the
right first approach (simplest, the barrel is on a blocky assembly tied to the ground to keep it from
rolling far in recoil)."*

This is what stops the first assembly being a toy, and it is genuinely the simpler carriage — a naval
truck carriage is blocks and axles, where a field carriage needs a trail, large wheels and an elevating
screw. It forces the format to carry parts of DIFFERENT materials joined in DIFFERENT ways, which one
capped cylinder never would:

| part | material | joined how |
|---|---|---|
| barrel | bronze or cast iron | trunnions resting in the carriage cheeks — a BEARING that carries load in compression and permits rotation |
| carriage cheeks, bed, trucks | oak — and its **anisotropy is load-bearing here** (docs/46 row 30) | pinned and bolted |
| breeching rope | hemp | **tension only, with slack** — it does nothing until it comes taut, then arrests the recoil |
| carriage on deck | — | `Rest`: no tension capacity at all, friction decelerating the recoil |

★ **Two additions to §6's join taxonomy fall straight out of it**, which is the point of building a real
object early: a **bearing** (carries compression, permits rotation about an axis) and a **tension-only
member with slack** (a rope does nothing until taut — a join whose capacity depends on its current
extension, not merely on its material). Neither appears in a hull made of welded plate, and neither
would have been noticed by designing the taxonomy from first principles.

★★ **And recoil hands us a FREE, EXACT validation that needs no historical figure at all: momentum must
balance.** The shot's momentum out of the muzzle equals the gun-and-carriage momentum backwards, and
that check is independent of whether our muzzle velocity is right — it tests the interior-ballistics
chain's *bookkeeping* rather than its calibration. Then friction on the deck and the breeching rope
arrest it, which exercises `Rest` and the tension-only member under a real impulse rather than a
declared one.

### ★ Black powder is a MIXTURE, not a substance — found by trying to catalogue it (2026-08-02)

The first attempt at the cannon began where the SOP says it must: source the propellant before any code
uses it (Law VII). It got this far and then hit something worth writing down.

**Sourced cleanly** — the ENERGETIC properties, which is what a gun actually needs: specific energy
**2.86 MJ/kg**; permanent gas yield **~0.265 m³/kg at STP** (converted from the ~1.05 in³ per grain
quoted for muzzleloaders — black powder is unusual in that much of its product mass stays CONDENSED as
potassium salts and smoke, which is why its gas yield is far below a smokeless propellant's);
products' specific-heat ratio **γ ≈ 1.2**; Noble-Abel covolume **~1 cm³/g**; and a flame temperature of
**~1950 K at 1000 psi** for the classic 75/15/10 composition.

**Could NOT be sourced** — the properties of the SOLID: specific heat, decomposition temperature,
thermal conductivity. And `materials::thermal_data_tests` rightly refuses an entry without a specific
heat, on the correct principle that *"specific heat is measurable for everything, so everything has
one"*.

★ **The block is the answer, not an obstacle.** Black powder is not a substance — it is a mechanical
MIXTURE of three: ~75% potassium nitrate, ~15% charcoal, ~10% sulfur, none of which is in the catalogue.
Its bulk specific heat, density and conductivity are *derivable from its constituents* by exactly the
mixture reduction §4 already specifies; what is irreducibly its own is the ENERGY RELEASED when those
three react, which is a property of the reaction rather than of any one of them.

So the SOP-correct order is: **catalogue KNO₃, charcoal and sulfur as substances; represent black powder
as a mixture of them plus a reaction; derive the bulk thermal properties rather than typing them.** A
first draft of the entry did the opposite and quietly carried `specific_heat: 1000.0` — a number
invented at the keyboard, and precisely the defect `Material::specific_heat` was built to prevent (840
in `impact.rs`, 1000 in `aggregate.rs`, 1000 in `matter.rs`: one unknown, three answers). It was backed
out rather than landed with a plausible figure.

★★ **And the physics this needs is RAPID OXIDATION, not "a propellant"** (Robin): *"Rapid oxidation will
be an important principle in the engine (fires, etc) so this won't be wasted."* A campfire, a burning
ship, a gunpowder charge and a rusting hull are one reaction at different rates and different oxidiser
availability — the charter's own shape, one law at every scale. The distinguishing quantity is **where
the oxygen comes from**: a fire is air-limited and therefore ventilation-limited; black powder is
SELF-oxidising, which is exactly why it works in a sealed bore. So the three substances catalogued here
already split along the axis the model needs — `potassium_nitrate` is the oxidiser, `charcoal` and
`sulfur` the fuels — and what they still lack is REACTION data (enthalpy of combustion per kg,
stoichiometric oxygen demand, available oxygen per kg) rather than anything about themselves.

This is the substance-versus-assembly distinction from §6b arriving from the other direction — first met
in a rainforest canopy, met again in a keg of powder — which is the sign it is real and not a
convenience of the biome discussion.

### ★★★ THREE assemblies, not one — and the integration is the interesting part

Robin (2026-08-03), and this is a structural correction rather than a detail: *"the gunpowder and its
properties might be an assembly of its own... that way we can reload cannons"*, *"The cannonball another
assembly"*, *"And the canon itself a third"*, *"The integration of them will be interesting."*

A single `cannon.assembly` containing barrel, carriage, charge and ball would fire exactly once and
could never be reloaded. Split three ways, each has its own lifetime:

| assembly | lifetime | what happens to it |
|---|---|---|
| **the gun** — barrel, carriage, trunnions, breeching rope | persistent | recoils, is arrested, survives to be reloaded |
| **the charge** — powder, wad | consumable | **CEASES TO EXIST**, converting to gas and residue |
| **the shot** | transferred | leaves at ~450 m/s and becomes an independent assembly in flight |

★ **So containment is a RELATIONSHIP with state, not a static parent-child link.** §6's nesting was
written for a ship that contains a galley that contains a stove — true forever. A gun contains a charge
only until it is fired. The format therefore needs a placement that can be **created, consumed and
emptied**: `loaded -> fired -> empty -> loaded again`. That is the difference between an assembly
GRAPH and an assembly TREE, and a tree cannot express reloading.

★★ **And the integration hands us three free, exact validations — none needing a historical figure:**

1. **Mass closes across the event.** `powder + shot + gun` before equals `gas + residue + shot + gun`
   after. An assembly that is consumed must put its mass somewhere, and `oxidation::burn` already
   reports what became gas; the remainder is condensed residue and fouling. Nothing may vanish.
2. **Momentum closes.** The shot's momentum plus the gas's equals the gun-and-carriage's, backwards.
   This tests the interior-ballistics BOOKKEEPING independently of whether its calibration is right,
   which is what makes it worth having before the muzzle velocity is trusted.
3. **The shot must LEAVE.** It stops being contained by the gun and starts being an assembly with its
   own trajectory — so the format has to hand ownership over cleanly, which is exactly the operation a
   static tree cannot perform and the one a game will do thousands of times.

★ **Shot start is real physics, not bookkeeping.** A ball is held in the bore by wadding and by its
clearance fit until pressure overcomes it — which is §6's *interference fit*, resolved through the same
`friction_coefficient` every grain contact uses. So the moment the shot begins to move is derived from
the join, not declared as a trigger, and the join taxonomy earns its keep the first time it is used.

★ **The format's first test is a cannon, not a planet.** The direct enforcement of everything above is
to exercise a NON-PLANET through the same path the planets use — build a cannon (or a plank, or a
bolted joint) as a PARTS assembly, round-trip it, and run the §7 validations on it. If the format ever
drifts planet-shaped, that test is what breaks, and it breaks before a second body is ever compiled. A
suite whose only assemblies are Sun, Moon and Earth cannot tell a general format from a planetary one.
(`laws::every_catalogued_material_number_is_read_or_declared_unwired` is the other half, and it is a
proxy: it keeps the DATA and the CODE in step — the mechanism by which ambition rots into the demo's
subset — but it cannot see a structure that is planet-shaped.)

**One chain, to keep the ambition concrete.** Gunpowder deflagrating in a bore -> gas at pressure -> the
shot accelerating -> exterior ballistics through real air -> impact on oak planking -> splinters. Traced
against what exists today: the gas thermodynamics is half-present (`eos.rs` Tillotson, verified;
`atmosphere.rs` deriving a specific gas constant from molar mass), exterior ballistics is largely there
(verified drag, entry heating, `flight.rs`), contact and cohesion are there and scale-invariant. **The
two honest gaps are chemical energy release, which does not exist at all (docs/46 row 31), and
anisotropic failure, whose DATA is already catalogued and read by nothing (row 30).** Neither is a
reason to approximate: they are named, and a propellant gets sourced properties before any code uses it.

That chain is the same contact law that settles a grain of sand and the same one that splits a moon.
Which is the charter — *one law, every scale, every scene* — stated as something a player does.

---

## 1. Why compile at all — the measurements that forced it

Three, all taken 2026-08-02:

1. **The data cannot be shipped raw.** ESA WorldCover is 10 m over ~149 million km² of land: about
   1.5 × 10¹² samples. The land cover shipping today is 2048×1024. There is no version of "fetch the
   sources and derive at runtime" that works, so the fusion happens once, offline.
2. **The runtime derivation is already the expensive part.** The appearance integral cost 173–184 ms per
   mesh rebuild before it was taught to stop early, and that was *without* any new data. Every quantity
   it derives per vertex per rebuild — material mixture, slope moments — is a property of the ground,
   not of the frame. Deriving a constant repeatedly is the definition of work worth precomputing.
3. **The cost profile says where the work belongs.** Pinning the integral's sample grid from 1×1 to 8×8
   moved the rebuild 173.3 → 176.1 ms — flat (docs/46 row 29). Probe count was never the cost. But it
   *will* be, the moment tiles cover more area at 10 m instead of 3.71 m. Precomputing means that cost
   never arrives, rather than arriving and then being optimised.

---

## 2. What a compiled assembly IS — and what it must never become

**It is a cache of a derivation.** The sources are `assets/bodies/*.json`, `data/materials.json`, and
the open datasets. The compiled file is what you get by fusing them. If the two disagree, **the sources
win and the file is stale.** That direction is one-way, and the file must never be edited by hand — a
hand-edited compiled artifact is a number that traces to nothing (Law V) wearing a binary format for
cover.

Four rules make that enforceable rather than aspirational:

- **Deterministic.** Recompiling the same sources produces byte-identical output, pinned by a golden
  hash in the suite. A format that cannot reproduce itself cannot be checked against its sources.
- **Catalogue-versioned, and this one is a live trap.** Material indices are POSITIONAL. An assembly
  built against one `data/materials.json` and loaded against a reordered one silently means *different
  materials* — forest becomes iron and nothing errors anywhere. The header carries a hash of the
  catalogue and the loader REFUSES a mismatch; the file also names its materials by string id, so a
  stale file can be diagnosed rather than merely rejected.
- **Provenance survives compilation.** Every field group records dataset, version, date, and whether it
  is MEASURED or DERIVED. `web/public/bodies/earth/SOURCES.txt` does this in plain text today and is the
  only reason anyone discovered the land cover was invented (docs/46 row 28). That property matters
  *more* once the data is binary, not less, because a binary file cannot be opened and doubted.
- **It describes MATTER, not appearance.** Material mixtures and geometric moments — what is there and
  how it is arranged. **No colour.** Albedo comes from the material catalogue at runtime, so correcting
  a material's optical properties improves every assembly ever compiled without rebuilding one of them.
  Baking colour would invert Law VI, freezing a render decision into the description of the world.

---

## 3. The shape

```
header      magic, format version, assembly id + name
            catalogue_hash        — refuse to load on mismatch
            source_manifest_hash
            extent (bounding radius, or an AABB for a part-built assembly)
            validation_summary    — what physics was run, and what it found (§7)
directory   [ (section kind, byte offset, byte length, item count) ]
sections
  MATS      material ids used, by NAME, with their catalogue indices
  STACK     ordered material stacks — radial shells, surface strata, part laminates (§4)
  SURF      a quadtree over the sphere; each node is the integrated description of its patch (§5)
  PARTS     positioned parts, and the JOINS between them (§6)
  AIR       the atmosphere's declared composition and its derived equilibrium profile (§8)
  PROV      provenance records, one per field group
```

Not every assembly has every section — that is what the directory is for. **The Sun is STACK and nothing
else. An ocean liner is PARTS. Earth is STACK + SURF + AIR.** One reader, one loader, one set of
invariants.

---

## 4. Materials — mixtures, and the stack unification

Two representations, and they are enough:

**A mixture** is `(material index, fraction)`, which is already
`terra::appearance::Appearance.mix` and already what `materials::aggregate_albedo` reduces. Used
wherever matter is intermingled below the resolution being described: a biome's canopy-plus-litter-plus-
soil, a beach's quartz-and-shell, a regolith.

**A stack** is an ordered run of `(material, thickness)` along an axis. And the observation worth making:
**radial shells, surface strata and part laminates are the same concept with different axes.**

| | axis | example |
|---|---|---|
| radial shells | body radius | Earth's core → crust; the Sun's three plasma bins |
| surface strata | depth below local surface | topsoil over clay over bedrock |
| part laminate | the part's own normal | hull plate: primer, steel, antifouling |

One record type, one summary rule, three anchors. A leaf is cuticle-over-mesophyll; a hull is
paint-over-steel; a continent is soil-over-rock. Writing that three times would be three answers to one
question (Law II), and it is the kind of duplication that only becomes obvious once the third case
arrives — so it is being written down before the third case arrives.

---

## 5. SURF — and why the quadtree node is exactly `Appearance`

A SURF node describes the surface over its own patch: the **material mixture**, the **mean gradient**
(what a mesh vertex's normal should show), the **variance of the gradient** about that mean (the
roughness a coarse mesh cannot carry), and the **elevation** mean and range.

★ **A parent node is its children combined by the law of total variance** — which is
`Appearance::combine`, already written and already tested. So docs/63's convergence invariant —
*"resolve a patch to matter, integrate its appearance over the footprint, and the result must equal the
texture that was already being drawn there"* — stops being a property the runtime must be trusted to
have and becomes **the rule the file is built by**. Any node can be recomputed from its children and
must reproduce itself, which is the strongest form the invariant can take: structural, not asserted.

It also retires the sampling budget for the elevation half. `resolution::WorkBudget` exists because the
runtime integral SUB-SAMPLES a footprint it cannot afford to read completely. A node read from the
quadtree is the COMPLETE integral over that patch, computed once, exactly. The budget stays for whatever
is still derived live; the measured surface stops needing it.

### ★ Every body reserves a SURF slot, and SURF must never speak of "biomes"

Robin: *"I think all planets should have a surf slot reserved; if it's not filled we don't use it. I'm
hoping we have data for mars, venus, etc from space missions, and more to come in future."*

The slot costs nothing — the directory already makes every section optional, so an unfilled SURF is an
absent directory entry, not a stub. What the rule really buys is a **constraint on the node's contents**,
and it is worth stating because getting it wrong would be invisible until the second body arrived:

★ **A SURF node carries a MATERIAL MIXTURE, never a land-cover class.** If nodes stored "biome index 3"
the format would be Earth-shaped, and every airless body would need a special case — or worse, a fake
biome vocabulary. Storing catalogued materials and their fractions means Mars's dust-over-basalt, the
Moon's mare regolith and Earth's rainforest are the same record with different entries, and a body with
no vegetation needs no exception. It is also what the appearance integral already consumes
(`Appearance.mix`), so nothing has to translate.

This is exactly the mistake the shipped Earth already made once, one level up: `earth.json` maps six
biome indices to materials, and index 3 ("forest") maps to `pine` — pine TIMBER — so the Amazon renders
the colour of cut lumber (docs/46 row 28). A class is a label; a mixture is matter.

**Verified public topography for the bodies next in line**, all no-auth via USGS Astrogeology Astropedia
and the PDS Geosciences Node: **Mars** MGS MOLA global DEM at 463 m, and an HRSC/MOLA blend at 200 m;
**the Moon** LRO LOLA LDEM at 118 m; **Venus** Magellan global topography at 4,641 m. Robin: *"We'll
likely have to roll our own rasters based on real data"* — which is precisely what the compiler is for,
and it is the same fusion step Earth's needs.

---

## 6. PARTS and JOINS — and joins are not new physics

A part is a material (or a stack, or a mixture), a shape, and a place: a primitive (box, cylinder,
sphere, shell, extrusion) or a mesh reference, plus a transform.

**A join is first-class, because where an assembly breaks is decided by how it was fastened, not by what
it is made of.** A welded bulkhead and a bolted one are the same steel and fail completely differently.

The critical discipline: **a join must be expressed in quantities the material catalogue already has, so
it is a boundary condition on the existing contact/cohesion law rather than a second physics** (Law II).

| join | what carries the load | fails by |
|---|---|---|
| `Weld` | parent material continuity, optionally heat-affected-zone-reduced | the parent's own `fracture_strength` |
| `Fastener` (bolt) | preload + shank shear/tensile area, at discrete points | shear or tensile capacity of the fastener material |
| `Rivet` | shear across the shank | shear capacity; no meaningful tension |
| `Adhesive` | bond area | area × bond strength; and weak in PEEL, which is the honest reason a glued joint is not a welded one |
| `Interference fit` | normal pressure × `friction_coefficient` | slip, using the same μ every grain contact uses |
| `Rest` / contact | nothing in tension at all | separates under any tensile load — this is what a stack of blocks has |

Every one of those resolves to `fracture_strength`, `friction_coefficient` or cohesion — all already in
`data/materials.json` and already used by `granular::Contact` and `granular::critical_bank_height`. **A
join that needed a new material property is a signal that the property should be sourced and catalogued
(the Law VII SOP), not that joins need their own physics.**

**Nesting and instancing, for the complex assemblies that are coming.** A part may reference another
assembly by id plus a transform. Sixteen billiard balls are one ball assembly and sixteen transforms; a
ship contains a galley contains a stove. This is what makes docs/63 item 4 land — a pool table becomes a
definition rather than a fourth `#[wasm_bindgen]` struct.

**Summary by volume.** A parent part's mixture is its children's, weighted by volume, through the same
reduction a continent uses weighted by area. **So a ship seen from orbit summarises exactly the way a
landmass does**, and resolution-by-necessity (docs/44) needs no second vocabulary for man-made objects.

### ★ Reusable assemblies — biomes and flora, and the one thing that must NOT be baked

Robin: *"would it make sense to go ahead and create data for biomes (texture of flora, reusable assets
the compiler can grab and place as needed?"*

**Yes — and the reusable asset is an ASSEMBLY, not a texture.** That distinction is the whole answer.

A biome is already an assembly in this design's own terms: a stack (§4) of materials with a structure —
canopy over understory over litter over soil over parent rock — plus the statistics of what is
distributed through it. "Tropical broadleaf evergreen forest" is one such assembly, and the Amazon and
the Congo are two PLACEMENTS of it with locally measured parameters (canopy height from GEDI, leaf area
index from MODIS, soil texture from SoilGrids, parent lithology from GLiM). That is instancing (§6),
which the format already has. **No new concept is required, and inventing a separate "biome library"
format would be a second answer to a question this one already answers (Law II).**

What must NOT be baked is a picture of a forest. The file carries matter, never appearance (§2), and a
photographic canopy texture draped on the ground is the stand-in-FOR-matter docs/63 rejects. The good
news is that nothing needs one: leaf tissue and tropical hardwood are MATERIALS, `texture.rs` already
generates a material's visible grain from its cited optical properties, and above the material tile the
appearance integral mixes materials over the footprint. The chain from "what it is made of" to "what it
looks like" is already complete and is currently starving for exactly this input.

★ **The invariant that makes a reusable biome honest, and it is docs/63's own, applied to flora:**
resolve the biome to actual instanced plants — trunks, branches, leaves as real parts — integrate their
appearance over a footprint, and **it must equal the mixture the coarse description gave**. If they
differ, one is lying. That single requirement forces the two readings to be one description:

- **at distance**, a mixture and a roughness — what `Appearance` already carries, cheap, and enough to
  end the flat green fill;
- **up close**, instanced plant assemblies with real geometry, resolved by necessity (docs/44), never by
  camera altitude alone.

★★ **And "up close" means eye level, which is where the acceptance bar actually sits.** Robin
(2026-08-03), standing at the gun: *"that should change at the elevation of the cannon, looking toward
the horizon, there should be real trees one day."* A foliage albedo is what a FOOTPRINT looks like; from
a gun deck on a shore the same matter has to be individual trees with trunks and crowns and gaps you can
see between. Both readings are the same description at two resolutions, and the convergence invariant
above is what forbids them from being two different Irelands — integrate the trees over a footprint and
you must get the mixture back. That is the flora case of the one thing docs/63 exists to say: *the Earth
should be the Earth, no matter how close or how far the camera pans.*

Which is Robin's own sentence from docs/63 — *"we only materialize that matter visually when we need to,
and only the amount we need to"* — with a rainforest as the worked example instead of a hillside.

**And this work is NOT blocked on the binary format.** The material catalogue entries (leaf tissue,
tropical hardwoods, quartz/calcareous/basaltic sands) and the biome assembly definitions are SOURCE
data; the compiler consumes them. They can be authored, sourced and reviewed before a single byte of the
format exists, and they are needed whether or not the format ever ships.

---

## 7. ★ Physics-guided construction — the compiler as a test harness

This is the part that makes a compiled assembly worth more than a faster loader, and it is Robin's
phrase: *"with the engine's physics guiding/validating construction."*

A compile is the one moment where expensive checks are affordable. The engine already owns the physics
to run them, and several of these are checks **nothing currently performs at all**:

- **Mass closure.** The layer stack's integrated mass must equal the body's measured mass. Earth is
  5.972 × 10²⁴ kg. A wrong density fails the compile instead of quietly changing every orbit in the
  system.
- **Moment of inertia.** Earth's polar moment factor is measured at I/MR² ≈ 0.3307, and the layer
  profile predicts it. This is a strong, independent, free check on the interior model — a profile can
  hit the right total mass with the wrong distribution, and only the moment of inertia notices.
- **Surface gravity emerges and is checked**, never declared: 9.81 m/s² falls out of the composition or
  the assembly is wrong (`crate::laws` already fails the build on a declared gravity).
- **Hydrostatic consistency** via `hydrostatic.rs`: a layer profile not in equilibrium describes a body
  that would immediately start evolving, which is a statement about the model rather than about the
  planet.
- **Structural closure for PARTS.** Under the assembly's own weight in its declared gravity, does every
  join carry its load? An ocean liner whose deck joins fail under the deck is a modelling error the
  compiler can catch, and the answer comes from the join table in §6 with no new physics.
- **Quadtree self-consistency.** Every SURF parent must equal `combine(children)` — §5's invariant,
  verified over the whole tree at build time rather than sampled at runtime.

★★★ **And the same physics must be available at RUN time, not only at compile time, because a
destroyed assembly is the interesting case.** Robin: *"a ball slightly too large, a charge too strong,
should be able to destroy a cannon as history shows it did."* A compile-time check that the gun holds
its proof charge is necessary and not sufficient — the gun that bursts is the one loaded wrongly, and
that happens in play. So §7's structural closure is not a gate that blesses an assembly once; it is a
predicate the assembly carries and is re-asked under real load. **An engine in which a cannon can never
burst is not simpler than one in which it can — it is one that has quietly declared the failure mode out
of existence**, which is Law IV inverted and would make the whole join taxonomy decorative.

★ **And it is cheap, which is the part that makes it practical rather than aspirational.** Robin:
*"if it all checks out we hand the easy 1d to the renderer, if physics predicts catastrophy, we share
that with the renderer. No need to actually render the matter particles, so should be a fast
calculation."* Deciding whether an assembly fails is closed-form arithmetic on numbers it already
carries; only a failure that IS happening needs matter resolved, and then only to show how it comes
apart. So the expensive path is never entered speculatively — the assembly's own validation predicate
is the trigger, and a sound gun costs nothing to prove sound.

The results go into the header's `validation_summary` and the PROV section, so **an assembly carries the
evidence that it was checked**, and a consumer can see which checks ran, which passed, and which were
skipped for want of data. A check that could not run is recorded as *not run* — an unknown stays unknown
at the boundary (Law VII), rather than being absent and indistinguishable from a pass.

---

## 8. Atmosphere — bake equilibria, never states

`atmosphere.rs` derives a specific gas constant from molar mass and a scale height from that, so a CO₂
atmosphere is genuinely more compact than an air one. It is instantiated in **no scene** (docs/46 row
12) — built and unwired, the pattern docs/48 names.

An AIR section carries: the **declared composition** (gas materials and fractions — gases are materials,
per the Law VII SOP) and the **declared total mass**, plus the **derived equilibrium profile** as a
sampled table. Surface pressure is never declared; it emerges as the weight of the column,
`P = Mg/4πR²`, and the compiler checks the derived profile reproduces Earth's 101,325 Pa.

★ **The rule that keeps this honest: bake EQUILIBRIA, never STATES.** The hydrostatic profile of a
settled atmosphere is a static property of a body's composition, mass and temperature structure — a
derivable constant, and caching a constant is free. Weather, wind, a heated column, an entry shock are
STATES: dynamic, and baking one produces a number that has stopped responding to physics, which is a
fudge however well-sourced its initial value. The profile is a starting condition and a far-field
boundary; it is not the atmosphere's behaviour.

---

## 9. Optimization — blue sky, with the one rule that keeps it honest

- **Zero-copy sections.** Fixed layout, little-endian, aligned so a fetched range casts straight to a
  `bytemuck::Pod` struct. No parsing on the hot path; "serve up very quick sections" means the section
  IS the in-memory form.
- **Byte-range fetch per node.** The directory plus the quadtree's own offsets let a reader take one
  node and its subtree by range — the same host/engine split `load_world` and `terra::tiles` already
  use: the engine names what it needs, the host fetches it.
- **Progressive prefix.** Nodes laid out breadth-first, so the first few kilobytes are a complete
  coarse whole body. A viewer at interplanetary distance is finished after the prefix; a descent
  extends it. The file's byte order becomes the LOD ladder.
- **Delta-against-parent.** A child is stored as its difference from its parent's summary. Because the
  parent IS the children's combination (§5), the residuals are small and centred — so **the
  parent-child invariant doubles as the compression scheme**, which is the kind of coincidence that
  indicates the structure is right.
- **Quantisation, bounded by a real criterion.** Elevation to fixed-point against a per-node range;
  gradients to f16; fractions to u8. ★ **The rule: quantisation error must be strictly smaller than the
  source measurement's own uncertainty, and the file records both.** SRTM/GLO-30 vertical error is
  metres; storing to centimetres is lossless *with respect to what is known*. Without that criterion,
  quantisation is a silent fudge — this makes it a stated, checkable one.
- **Deduplicate by content hash.** Ocean nodes are identical over vast areas; a hull's ribs are one
  shape a hundred times. Identical subtrees and identical part definitions are stored once.
- ★ **The client never holds a whole planet, and that is the design rather than a fallback.** Robin:
  *"If the compiled planets get too big, we can stream what clients need to them JIT."* The three
  properties above are what make that work without a second delivery mechanism — a breadth-first
  progressive prefix gives a usable coarse body immediately, node offsets make any refinement an HTTP
  Range request, and deltas keep each refinement small. **The file IS the stream**; there is no separate
  streaming format, and `terra::tiles` already proves the host/engine split it needs (the engine names
  what it wants, the host fetches). A compiled Earth that is too large to download is therefore not a
  problem to be solved later — it is the expected case.

---

## 10. The starting set, in this order

1. **The Sun.** STACK only, three shells, no surface. The smallest real assembly — it proves the header,
   the directory, the catalogue-hash gate, the loader, and mass closure (its mass is what every orbit in
   the system depends on) without any of the hard parts. If the format cannot express the Sun in a few
   hundred bytes, it is already too complicated.
2. **The Moon.** STACK only today — `assets/bodies/moon.json` is four shells and no surface block. It
   proves a second body reuses the path with nothing special-cased, and leaves the SURF slot ready for
   the lunar rasters that are coming.
3. **Earth.** STACK + SURF + AIR, built first from the rasters that ship today so the format can be
   validated against known behaviour, then rebuilt from the real datasets (docs/46 row 28: ESA
   WorldCover, SoilGrids, GLiM). **Compiling Earth twice, from deliberately different data, is itself
   the test that this is a pipeline and not a one-off.**

Each is rebuildable on demand — that is what "one high cost compile and done" buys, and it is what lets
a corrected material or a better dataset propagate without touching engine code.

---

## 11. What this does NOT decide

- **The substances-and-assemblies model** (docs/46 row 28): what a "rainforest" resolves into as a
  material mixture, and the substance-versus-assembly distinction that stops a canopy being given a bulk
  density. That work FILLS this format; this doc defines the container.
- **Whether the runtime still derives anything live.** It does, and should: generated relief below the
  finest measured data has no dataset to come from and stays a function (docs/46 row 27). The compiled
  assembly carries what was MEASURED; the generator carries what is below measurement, flagged as it is
  today.

**Related:** docs/43 (worlds as data) · docs/44 (resolution by necessity) · docs/46 (rows 12, 14, 27, 28,
29) · docs/48 (built and unwired) · docs/51 (scenes as data) · docs/53 (the engine driven by a
definition) · docs/58 (the generic body) · docs/63 (the pool table — the assembly-at-a-coordinate this
format serialises).
