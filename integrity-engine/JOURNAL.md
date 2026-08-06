# Development Journal

A running log of major milestones for the Integrity engine. Newest entries at the top.
Each entry records *what* changed, *why*, and *how it was verified*.

## 2026-08-05 — type versus instance, and the dependency it walked straight into

**What.** docs/67 step 2. `broadleaf-tree-oak.json` is a species; the tree at 53.1°N is an individual.
Until today the engine had only the first, so **damage had nowhere to live** — `terra::flora::Sited`
carried lat/lon/kind/yaw/scale and no state at all, and a bus cannot crush a tree that has no state to
be crushed in.

★★ **The design constraint that shaped it: this must not become a FIFTH "thing at a place".** Four
already exist and each is a view for one consumer — `orbit::Body` (pos/vel/mass, for the integrator),
`accretion::Body` (+rho/radius/ang_mom/thermal, for clumping), `interaction::BodyState`
(+radius/strength/air, for contact), `render::Drawn` (for drawing). None says which assembly it is an
instance OF, none has an attitude, none carries damage, none knows what contains it. So `Instance` is
**the thing they are projections of**: it holds only STATE — identity, placement (position + attitude,
*in a container's frame*), motion, thermal energy, damage — and everything definitional is asked of the
TYPE. `Instance::body_state` lives in the same file so a view cannot quietly disagree with the instance.

Mass is deliberately not a field, though Robin's property list names it: ten thousand oaks would carry
ten thousand copies of one oak's mass. It is derived from the type and reduced by this individual's own
damage, through the same envelope × packing × density the definition uses — so half of every part is
half the mass, with **no damage-to-mass curve anywhere**. Same for extent, temperature and strength.

**Verified** (five tests): two instances of one oak share every definitional number and nothing else;
tearing the crown off one reduces its mass AND its reach while leaving the other pristine and **the
species untouched**; a gun placed in a ship's frame goes where the ship puts it when the ship turns,
with no change to the gun; the collision projection reports what the instance says; and the same joules
in a half-burnt tree is a higher temperature than in a whole one, because there is less matter to heat.

★★ **AND IT HAS NO CONSUMER — filed as `docs/46` row 46 by its author, on the day it was written.**
"The law is built and proven, then wired into one place or none" is this repo's most repeated failure
(docs/48), and the only defence is naming each instance of it out loud rather than intending to wire it
later. The reason here is a real dependency and the migration order predicted it: the obvious first
consumer is Terra's cannon, whose `cannon_at` is `(lat, lon, bearing)` — a GEOGRAPHIC placement, which
becomes a `Placement` only when the gun can be *within Earth*, and that needs Earth to be an assembly
with an id. **Step 5.** Robin, on being told: *"Agreed, this is part of why Earth must be an assembly."*

The second candidate fails for the other predicted reason: `flora::Sited` is regenerated from a position
hash every rebuild, so an instance placed there would not survive a frame — the derivable-rule-plus-
exceptions problem, step 4.

**Verified.** 568/568 native, `mod app` clean for wasm32.

## 2026-08-05 — a planet is not an assembly, and the docs said it was

**What.** Asked whether the Earth assembly contains plant assemblies, I answered that Earth is a
`planet::LayeredBody`. Robin: *"WHAT? Everything we've done is supposedly building an assembly of earth,
and you blithely inform me it is a LayeredBody? … A layered body is either a bad assessment or the last
couple of days work with Claude are a lie."*

**The assessment was right and the DOCUMENTATION was the lie.** `assets/bodies/earth.json` carries
`layers`, `atmosphere_mass_kg` and `surface` — no `parts`. `assembly::Assembly` describes **six**
objects: cannon, charge, shot, oak, spruce, grass tuft. Every planet is the other format. Meanwhile
`docs/65` §4 was titled *"Where it stands, honestly"*, counted 79 scene-called methods and three scene
structs, and **never said that no planet is an assembly** — and the architecture page shipped to
integrity.bothead.net said *"adding a species, a vehicle or a planet is adding an assembly"* in the
present tense. Two days of work were described in assembly language on top of that silence. All three
are corrected; the violation is `docs/46` row 45.

★★ **Robin's argument for why it is ONE thing, not two, and the engine already proves it both ways:**
*"A planet is an accretion of debris bound by its own gravitational effects which we've worked hard to
model."* The round trip is built and tested — `HydroBody::particalize(&LayeredBody, n)` turns layers into
particles, `accretion::sample_layers(...)` reads layers back out of particles. So a `LayeredBody` is not
a different KIND of object: it is a **de-resolved summary of an assembly of matter**, exactly the
relationship `Derived` already has to `Assembly::parts`. The split is lineage — layers came from the
giant impact, assemblies from the cannon — not principle, and it is the same shape as row 1
(`Aggregate` vs the voxel `World`), which was closed by unifying rather than justifying.

★ It is also the **fifth** arrival of the substance-versus-assembly distinction. Row 35 counted four and
said it *"wants to be a first-class idea rather than a lesson relearned."*

**Built today, small and load-bearing:** `Shape::reach_m` / `Part::reach_m` / `Assembly::reach_m` — an
assembly ends at the outermost boundary of its outermost component. Not "where its air ends", which was
my first wording and which Robin corrected: naming the rule after air teaches the engine a special case,
when the identical question is asked by a tree's canopy, a ship's mast and a cannon's muzzle. It has a
consumer immediately: `ballistics::fire` was taking a gun's lowest matter from `equivalent_radius_m`, the
radius of a sphere of the same VOLUME, which for a barrel is off by more than six times.

**Designed, not built:** [`docs/67`](docs/67-everything-is-an-assembly.md) — the unified model. Robin's
seven assembly properties, the eight I think are missing (extent-before-detail; containment as a
DERIVABLE rule plus exceptions, because a planet holds 10^12 trees and cannot list them; type versus
instance, since damage has nowhere to live today; runtime re-parenting; per-actor cadence; a
bidirectional signal; cross-resolution agreement; temperature and frame), the per-assembly collision
engine with its one hard rule — **one implementation, many instances** — and a migration order whose
guard rail already exists: `one_earth_tests::the_three_scenes_read_one_earth` asserts digit-identity
across three scenes reading Earth, so any migration that moves a number fails.

**Verified.** 563/563 native, `mod app` clean for wasm32. The correction to the architecture page is
deployed, because a diagram that quietly runs ahead of its engine is how a model stops being checkable.

## 2026-08-05 — Earth got a sky, and it is the air

**What.** Robin, on where a sky belongs: *"Sky must be a component of Earth assembly, no"* — and on
what should draw it: *"making ray-tracing work in the engine… It doesn't have to be very sophisticated
ray tracing… Sun is close to a point source."* Both, in one integral. Design: [`docs/66`](docs/66-the-sky-is-the-air.md).

★★ **The closed form was the wrong integral, not an approximate one.** `rayleigh_veil` is the analytic
solution of ONE geometry — a slab seen from outside, looking down — where the sun path and the view path
shorten together, which is the only reason it collapses. Stand on the ground and look up and that
pairing inverts. (The orphaned `sky.wgsl` used it anyway, with `mu_v = ray.y`, which is also why it could
only ever be a sky for a flat world.) So the law is now the marched integral,
`atmosphere::air_inscatter`, mirrored line-for-line by `shaders/atmos.wgsl`, and **`rayleigh_veil` is its
analytic special case** — kept, and used as the reference the march is pinned to (agreement **under 1%**
in the plane-parallel limit). One law, and the geometry decides which face of it you see.

Everything below falls out of that one integral. None of it is drawn on:

- blue overhead, pale at the horizon — path length, band by band;
- **red at sunset** — the blue is removed from the SUNLIGHT before it can scatter;
- **twilight** — the low air sits in the planet's shadow while the air above it does not, which
  **retires a declared number**: `twilight_half_angle`'s `sqrt(2H/R)` ramp existed because a flat slab
  has no geometry that could produce twilight;
- aerial perspective — the same integral, stopped at the surface point;
- a glowing limb — rays that miss the ground still cross lit air.

**Why it is a component of the body.** `AirColumn::of_body` is the only place a declared mass of air
becomes optical depth and scale height, and both scenes hold one from the same body. `render::SkyVeil`
draws what it is handed and has no opinion: hand it a body with no air and it draws nothing, with no
branch — the Moon's black sky needs no code. **A scene gained no new verb** (the docs/65 ratchet is
unchanged at 57).

**Verified** — `web/rig/terra_sky.mjs`, 2560x1600 on the 5060 Ti, and **every claim is a difference
against a frame that must not show the effect**. The control is a new world, `worlds/earth-airless`: the
same Earth, same ground, same camera, **zero kilograms of air**.

| | sky R/G/B | |
|---|---|---|
| noon, horizon | 69.0/121.7/215.2 | B/R **3.12** |
| sunset | 74.4/80.0/63.6 | B/R **0.85** |
| night | 2.7/2.9/3.6 | stars only |
| **airless noon** | **2.7/2.9/3.6** | **stars only** |

The daylit sky is blue and **it is the AIR** (zenith 52/95/185 with air, 2.7/3.0/3.7 with none, and the
GROUND is still lit in the control — it removes the sky, not the sun); night has no sky; sunset reddens
with nothing changed but the clock; distant ground is veiled by the air in front of it.

**What the rig caught that I got wrong, twice.** (1) The first "sunset" frame was barely redder than
noon, because at 53°N in June the sun is up 16½ hours and 90° of hour angle leaves it 20° high. The rig
now computes the hour angle from `cos H0 = -tan(phi)tan(dec)` — the same relation `solar::day_length_hours`
uses — rather than my guess. (2) An assertion demanding that the sunset be redder TOWARD the sun failed
at 1.17 against 1.17, and **the assertion was the thing that was wrong**: in single scatter the reddening
is set by the SUN's path length, which for a sun on the horizon is the same however you turn your head.
Only the brightness varies across azimuth, through the phase function. Corrected in place with the
reasoning, because a test that fails for a physical reason is evidence.

★ **Stars now go out at dawn for the reason they really do.** The first sky frame had them plainly
visible through blue daylight. The cause was not the air (at zenith it passes ~90% of the blue) — you
cannot see a star in daylight because **the sky is brighter than it is**, and that is a statement about a
SUM, which never happened because every pass tone-maps its own output. The star pass now evaluates the
sky along its own ray with the same chunk and outputs `tonemap(L_sky + L_star*T)`, per band. What is left
is recorded rather than tuned (**docs/46 row 43**): the star pass carries exposure 80 while everything
else carries `SUN_GAIN = 22`, and at the daylight zenith the brightest pixel is **+128% above the sky
mean** where a real one is ~0.01%.

**Retired.** `veil_column_fraction` — the flagged stand-in that scaled a whole-column veil by
`1 - e^(-h/H)`; at 1.7 m that is 2e-4, identical for every pixel however distant, so the ground came back
**bit-identical with and without an atmosphere**. And the painted background: both scenes cleared to
`0.01/0.01/0.03`, a declared dark blue with nothing emitting it, which was also the entire "black" sky
the first night measurement read. Space is black.

★★ **A shader nobody compiles is not a feature, and now a machine says so.**
`laws::compiled_shader_tests::every_shader_is_compiled_by_something` is the gate that would have caught
this in the first place — `sky.wgsl` sat orphaned for weeks while every atmosphere test passed, because
they all asked whether the optics were right and none asked whether anything ran them (the docs/48
pattern). It caught **two more orphans on its first run**: `rayleigh.wgsl`, orphaned minutes earlier by
this change, and `particles.wgsl`, superseded by `matter.wgsl` in **July**. Both deleted.

★ **The screenshot ceiling was mine.** Robin: *"your screen shots are pretty low res… I do want us to be
able to verify high quality textures."* The render Xorg was configured `Virtual 1280 800` and the rig
fleet had **eighteen different hardcoded viewports**, the smallest 480x320 — and the rigs that fill the
public gallery were among the smallest. Now 2K (Robin's call), one shared `VIEWPORT` in
`web/rig/_launch.mjs` with 43 rigs moved onto it at the same 16:10 aspect, and `start-render-xorg.sh`
RESTARTS a server that is the wrong size instead of silently handing the next session a cap.

**Owed** (docs/66 §9): a GPU pin for the WGSL mirror — it is a hand-copy, checked by reading, which is
exactly the confidence level this project has learned not to trust; one exposure; a photographed limb
(Terra's camera turns to look straight down above ~60 km, so it cannot be aimed at one); and tone-mapping
ONCE into an HDR target, which is the deep version of the star fix.

## 2026-08-05 — the colour was never arriving, and Terra has no sky

**What.** Two defects found by chasing one observation of Robin's — *"the engine seems to be rendering
the color without taking available light into account… Grass shouldn't be apparently brighter than
everything else at night, right?"*

★★★ **The vertex colour never reached the picture.** `globe.wgsl` computed `albedo = grain * u.tint`,
where `grain` is the dominant material's triplanar texture; `i.col` appeared in that shader **only
inside comments**. So the land-cover MIXTURE and the SEASON were computed per vertex and discarded.
Proved by mutation rather than by reading: every land vertex set to magenta changed nothing on screen.
After the fix, the same mutation turns the ground magenta.

The fix is not "multiply by the colour" — the texture already IS the material's albedo and multiplying
two squares it, which is precisely why the value had been dropped. The vertex now carries a **ratio**:
the mixture's seasonal albedo over the dominant material's flat albedo. `SurfaceSample::albedo` became
`albedo_ratio`, and the rename made the compiler find all four producers — one of which I would have
missed.

★★ **This retracted yesterday's headline.** *"Ireland turns"* was reported from an R:G shift across
Jun–Dec. With the colour discarded, that cannot have been the leaves: it was the sun's own elevation at
45°N (≈68° in June, 21° in December) reddening through the atmosphere. A **negative control** is now
part of the rig — the Sahara, which has no senescent material at all — and it is damning: with colour
now arriving the Sahara shifts **+0.0199** across the year against Ireland's **+0.0150**. A place that
cannot turn moves more than a place that can. The seasonal signal is real in the material and smaller
than the lighting confound in the picture.

★★★ **And Terra has no sky.** `shaders/sky.wgsl` is compiled by nothing — absent from every
`include_str!`. Its header says *"for the terrain scene"*; that scene died in July and its successor
died on the 3rd. Terra lights the ground correctly and veils it, but the sky region falls through to
the star field, so a daylight frame near the ground is lit grass under a black starfield. That contrast
is what read as luminous grass. Measured: the night side IS correctly near-black and identical over
sand, forest and open ocean.

**Robin's framing for the fix, and it is the design**: *"Sky must be a component of Earth assembly."*
The atmosphere is matter Earth already declares; the engine should render what that matter does to
light rather than a scene owning a sky.

**Verified.** 550/550 native, `mod app` clean for wasm32, deployed. Permissions widened 42 → 97 rules;
blanket `sudo:*` deliberately excluded in favour of the specific host operations that were authorised.

## 2026-08-04 — Ireland turns, and the Serengeti turns the other way

**What.** The seasons reach the ground. Measured R:G of the ground through the year, on the 5060 Ti:

| site | Jun | Sep | Oct | Dec | |
|---|---|---|---|---|---|
| Maine | 0.6886 | 0.6908 | 0.6939 | **0.7005** | reddens → autumn |
| Ireland | 0.6855 | 0.6879 | 0.6904 | **0.6984** | reddens → autumn |
| Serengeti | 0.6875 | 0.6901 | 0.6867 | **0.6855** | greens → spring |

**Nothing told the Serengeti to run backwards.** At 2.3°S its December is its growing season, and that
falls out of the axial tilt through the solar declination — the same declination the terminator is drawn
with. Hue rather than luminance, because at 45°N the noon sun drops from ~68° to ~21° between June and
December and brightness is dominated by the sun's own elevation.

**Why it works now.** Three things landed in order: real land cover, mixtures, and a clock.

★ **The land cover was one wrong string.** `bake.py` asked GIBS for a layer that does not exist, GIBS
answered **HTTP 200 with an XML error body**, and the `except` reported *"GIBS land cover unavailable"*
before falling back to six invented latitude bands — for months. `landcover.png` is now 18 MEASURED
MODIS IGBP classes at a 100% exact palette decode. Galway is grassland; it was never boreal conifer, and
that is precisely why it could not turn.

★★ **A land-cover class is a MIXTURE.** IGBP classes are definitionally mixtures — *woody savanna* is
30–60% tree over grass — so `Surface::biomes` went from class→material to class→[(material, fraction)],
with fractions read off the IGBP cover thresholds at the midpoint of each stated range. `aggregate_
albedo_turned` gives a mixture its season: a mixed forest turns partly and stays partly evergreen.

★★★ **Two guards earned their keep, and one refused to ship a lie.** The palette decode used int16 and
overflowed — a 255 channel difference squares to 65,025 against int16's 32,767 and wraps negative, so
the exact-match rate read **1.8% for a raster whose every pixel is an exact legend colour**. The decode
guard refused it rather than shipping a plausible-looking biome map. And `fetch` had *cached the 460-byte
XML failure*, re-serving it after the layer name was fixed.

**Verified.** 543/543 native, `mod app` clean for wasm32, rig-watched. Honest about size: the seasonal
shift is measurable and monotone but visually subtle — broadleaf is 0.45 of that class, and the
canopy-versus-leaf gap (ledger row 35) flattens it further.

★ **And Robin's question found a real defect**: *"I'm not sure we have a tilted axis yet?"* Half yes.
`orbit.rs` carries the real 23.439° obliquity, which is why all of the above works; but `lib.rs` builds
the space band's Earth with its spin axis PERPENDICULAR to its orbit. One body, two axial tilts
(row 39).

## 2026-08-03 — a leaf is not a plank: colour derived from measured spectra

**What.** Ireland stopped rendering as cut lumber. `earth.json` mapped land-cover class 3 to `pine` —
the catalogue's pine TIMBER, albedo [0.68, 0.48, 0.21] — so every forest on Earth was drawn the colour
of a board. Robin: *"Pine Timber is always the wrong choice for flora though, we should look for 'pine
needles' or 'pine leaves', same with other biomes."*

Two new substances, `conifer_foliage` and `broadleaf_foliage`, and **their colour is not chosen**:

    449 measured fresh-leaf spectra (NEON domains + UCSB HyspIRI California, 98 taxa)
      -> mean visible spectrum
      -> convolved against the engine's OWN CIE 1931 observer under a 5772 K sun
      -> conifer   [0.0896, 0.1177, 0.0544]
         broadleaf [0.0738, 0.1006, 0.0400]

**Why this shape.** `Material::albedo` has always carried an honesty note calling itself *"a summary
property, a stand-in for the full spectral … optics … a placeholder to be grounded later, not an
irreducible fact."* This is that grounding. `blackbody.rs` already had `cie_observer` and the XYZ→sRGB
primaries for star colour, so a surface's colour and a star's colour are now one law — Planck through
the same observer — rather than two. `albedo_derives_from_the_measured_spectrum` re-derives the triple
every run, so what is in the file can only ever be a CACHE of the measurement.

★ **Two independent cross-checks nobody tuned toward.** The pulled spectra's 400–700 nm means are 0.084
(conifer) and 0.069 (broadleaf); the published measured values for those plant functional types are
0.08–0.09, and CLM 5.0's defaults are 0.07 and 0.10. And the derived colours come out green because
chlorophyll absorbs the red and the blue — nothing anywhere says "leaves are green".

★ **The forest class was SPLIT.** One class covered both the tropics and the boreal band, so the Amazon
and Siberia were the same material. Splitting it (3 broadleaf / 6 needleleaf) costs one line in
`bake.py` — and without it the second foliage substance would have shipped wired to nothing, which is
the pattern docs/48 exists to name.

**Verified.** Measured A/B through the engine on the 5060 Ti — same rig, same sun, same camera, the data
reverted and the wasm rebuilt for the control:

| site | before | after |
|---|---|---|
| Galway | 148.3 / 130.9 / 68.1 red-brown | **82.1 / 111.6 / 64.4 GREEN** |
| Amazon | 148.3 / 132.3 / 67.9 red-brown | **78.2 / 108.9 / 55.8 GREEN** |
| Siberia | 146.9 / 124.7 / 66.9 red-brown | **74.7 / 101.8 / 62.3 GREEN** |
| Sahara | 148.3 / 135.1 / 100.1 | 148.3 / 135.1 / 100.1 — **bit-identical** |

The Sahara is the negative control: unchanged to the byte, so only the forest classes moved. Before the
split all three forests were literally the same colour. 512/512 native, `mod app` clean for wasm32.

**★★ And the honest gap, recorded rather than papered over (ledger row 35).** A closed canopy returns
about a THIRD of what a single leaf does, because light scattering into it is caught by the next leaf
down. That darkening is an ARRANGEMENT, not a substance — so a forest still renders brighter than a real
forest, and both entries say so in their own notes. Darkening the leaf to compensate would put an
assembly's property inside a substance, and the same leaf held in a hand would then be too dark. This is
the substance-versus-assembly distinction arriving a fourth time.

**★ Two guards, each verified by making it fail.** `a_biome_never_paints_the_ground_with_the_inside_of_a_
plant` fails the build if any body's land-cover class points at an organic material that is not green —
a physical criterion, not a list of approved names, so it extends itself to materials nobody has written
yet. Putting `pine` back on class 3 turns it red with the albedo printed. And the Yarrr! scene grew from
one shore to four after Robin found the gun standing in the dark: `one_of_the_scenes_shores_is_always_in_
daylight` sweeps a day through the engine's own solar direction and fails if every shore is dark at once.
**Two shores was measured and does not work** — Ireland and Chile are 65° of longitude apart, leaving 7.6
hours a day with both dark, worst case 57° BELOW the horizon. Mumbai cuts that to 2.1 hours; Sydney
closes it, worst case 23° above.

**★ Found on the way, and it would have been silent:** `tools/bake-earth/bake.py` wrote to
`web/public/worlds/earth/`, which stopped holding Earth's rasters when Earth became a shared body. A
rebake would have written a complete set of rasters into a directory nothing reads, and said "done".

## 2026-08-03 — a cannon on a coast: assemblies, oxidation, and a shot that lands in the sea

**What.** Robin's acceptance test for the whole assembly architecture — *"as long as we can build a
working cannon and a working planet, and put a working cannon on a working planet and fire it, we know
our assembly build is sound"* — passes, and can be watched at **/yarr.html**:

    cannon: Fired at 589 m/s, peak 426 MPa, recoil 2.96 m/s,
            ejecting 2.09 kg gas + 1.53 kg smoke at 1950 K
    arrival: 10.9 kg at 0.1 km/s = 9.06e4 J

**Why this shape.** docs/64 (the compiled assembly) says a planet and an ocean liner are not different
kinds of thing: both are catalogued materials arranged in space, differing only in how the arrangement
is INDEXED. The cannon is the test of that claim, because it is emphatically not a sphere.

**Three assemblies, not one** (Robin): the GUN persists and recoils, the CHARGE is consumed, the SHOT is
transferred out — so containment is a relationship with state and an assembly GRAPH, because a tree
cannot express reloading. **Nothing declares a mass**: 2,375.7 kg and a centre of gravity 0.631 m behind
the midpoint both follow from thirteen parts and the material catalogue. A cached mass is a CACHE —
`verify_cache` re-derives and reports a mismatch as stale, "the parts win".

★ **The ball's SIZE follows from its NAME.** 24 lb of cast iron as a sphere is 5.53 inches across
against a historical 5.82 in bore; the difference is windage. A consequence, not a parameter.

★★ **Substances before compounds.** Black powder was BACKED OUT as a material after a first draft
carried an invented `specific_heat: 1000.0`. It is a MIXTURE — potassium nitrate, charcoal and sulfur,
each catalogued with sources — and its bulk properties derive: 891.3 J/kg/K, true density 1957 kg/m3
against a poured 1000, **and that gap IS the 48.9% porosity**, which is a property of an arrangement and
not of matter. Hence `Part::packing`.

★★ **`oxidation::apply_heat` is the general way matter is lit**, not `fire_gun` (Robin: *"we want these
to not be tied to specific scene information... apply_heat would be useful in a forest fire"*). One call
covers lightning on a tree — clearing oak's own catalogued 573 K — a linstock on a charge, and the same
joules through 100x the mass NOT lighting it, which is why a bonfire needs kindling.

★★★ **The number that makes it gunpowder, as a comparison between two derived quantities:** at 75/15/10
the KNO3 supplies 0.356 kg O2 per kg against a fuel demand of 0.499 — **71% of stoichiometric with no
air at all**. A fire is ventilation-limited; a charge is not. That single ratio is the whole difference.

**Verified.** 507/507 native, `mod app` clean for wasm32, rig-verified on the 5060 Ti, deployed live.

**★ Three disagreements with reality, all RECORDED rather than tuned.** The idealised black-powder
equation over-states permanent gas by ~25%; deriving the flame temperature from the burn energy gives
6751 K against a sourced 1950 K, because **over half of the product mass is CONDENSED** and heating those
solids absorbs the energy while exerting no pressure; and the constant-volume assumption ignores that
real powder burns while the shot is already moving. Muzzle velocity comes out 589 m/s against a
historical ~450 for exactly these reasons. `naive_flame_k` is kept, unused, so the size of the deferred
energy split can be READ — a deferred computation you cannot size is not an IOU.

**★★ A scene must never introduce physics, and it is now enforced.** `PlanetAir` — the type answering
what gravity is, what the air is, where the surface is — lived inside `mod app`. And a trajectory
integrator written here took a DRAG COEFFICIENT as an argument, putting the caller in charge of how hard
the air pushes back; it was deleted in favour of `flight::Flight`, which already flies meteors and
integrates quadratic drag in closed form. `laws::scene_purity_tests::a_scene_never_introduces_physics`
pairs each physics primitive with its owning module and fails the build if a scene calls one — verified
against the exact violation committed hours earlier.

**★ And the last picture told the truth about the one before it.** The scene looks orange because it IS
`pine` — the catalogue's pine TIMBER — since `earth.json` maps "forest" to it and Ireland sits in the
derived cover's boreal band. Lighting was MEASURED innocent (ground luminance 117.9 noon / 14.2
midnight). Colours emerge from materials exactly as they should; the material is wrong (docs/46 row 28).

## 2026-08-02 — the appearance integral, and the negative result that re-orders the plan

**What.** `terra::appearance` integrates the surface over the patch a vertex stands for and reports two
moments: the area-weighted material MIXTURE, and the variance of the slope ABOUT the mesh's own normal.
`rough_diffuse` in the shared shader chunk spends the second, because Lambert is roughness-blind and a
variance with nowhere to go is a variance discarded. `resolution::WorkBudget` sizes the work from
measured time rather than a declared constant. `Vertex` gained `rough`; the `([f32;3], f64, u32)` tuple
every surface builder passed around became `SurfaceSample`.

**Why.** docs/63: what the mesh cannot carry as SHAPE it must carry as STATISTICS. At 94 m altitude
Terra's mesh cell is ~469 m while a streamed elevation tile pixel is ~3.71 m, so some sixteen thousand
measured samples sit inside one cell, and `octaves <= log2(base_feature/cell)` evaluates to **0** — the
generated relief switches off exactly where a descent should start revealing ground. Adding vertices was
already measured and retired: one tier against four differed by at most four pixel values at 100 m.

**★ The rule that keeps it from double-counting.** The mesh already carries the MEAN slope, as its
normal; the appearance carries only the variance ABOUT that mean. Integrate the total and every hill is
counted twice — once as the shape being displayed, again as roughness smeared over it.

**★ The convergence invariant, as docs/63 asked for it.** Combining sub-footprints is the law of total
variance, `Var(total) = E[Var(within)] + Var(E[within])`. Drop the second term and a coarse footprint
reports less roughness than the sum of its parts, so the answer CHANGES on refinement — precisely "one
of them is lying".

**★★★ THE RESULT THAT MATTERS, AND IT IS NEGATIVE: the integral is correct and has nothing to
integrate.** At 100 m over the Colorado Rockies it changes the picture by almost nothing — not because
the model is wrong, but because the data cannot answer it. Measured through the engine at the scale
ladder's own site (39N, 106W, 469 m cell): the land-cover raster is **19.5 km per texel**, so the cell
sits inside ONE texel and the mixture has exactly **one** constituent; the elevation raster at that
resolution is a bilinear ramp across a cell — a PLANE — so sigma comes back **< 1e-3 rad**. The mean
slope is real, and the mesh already carries it.
And the surface's own description is **invented**: `web/public/bodies/earth/SOURCES.txt` says the land
cover is a *"DERIVED climate approximation from lat+elevation+coast, NOT a measured land-cover
dataset"* — six latitude bands. **The biome map also commits a category error: forest maps to material
`pine`, which is pine TIMBER (albedo [0.68, 0.48, 0.21], a brown), so the Amazon and the Congo are
drawn the colour of cut lumber.** Robin found both from the picture — *"the Sahara is green"*, *"the
Amazon Rainforest in Brazil/etc looks pretty sparse"* — before any of this was measured.
**So docs/63's step 3 is no longer "later", it is PREREQUISITE**, and the doc is amended to say so.

**★ The cost is a real regression and is NOT fixed.** Same ladder rig, same machine (5060 Ti), main vs
branch: worst frame **44-53 ms -> 158-186 ms**, a 3x rebuild hitch; steady-state render is untouched at
p50 0.4 ms either way. The first hypothesis — a `dir -> lat/lon -> dir -> lat/lon` round trip in the
inner loop — was wrong: removing it moved 177-192 ms to only 158-186 ms. The structural fix is not a
smaller sampling budget but a different shape: a **mip pyramid of moments** over the elevation, built
once per tile, making the per-vertex integral an O(1) lookup and EXACT rather than sub-sampled
(docs/46 row 29).

**★ The budget is measured, not declared** (Robin: *"budget for textels/etc in engine should scale
based on compute/GPU capability ... naturally degrading on slower systems, built-in future-proofing for
future platforms"*). `WorkBudget` is a closed loop on the time work actually took, so it needs no table
of device names, and its ceiling is the DATA rather than the hardware: once the grid is as fine as the
measurement underneath it, more samples re-read the same numbers and growth stops by construction.

**★★ Two mistakes, both found by instruments rather than by reading — and one of them was my own test.**
- **The convergence test was toothless and a mutation check caught it.** The first fixture was
  symmetric in `u`, so all four quadrant means were equal, `Var(E[within])` was ~0, and the test PASSED
  with `combine`'s between-parts term deliberately deleted. It now asserts its own precondition — that
  the fixture exercises the term it protects — so it cannot go quiet again. *Verify a new gate by
  making it fail* applies to tests you just wrote, not only to gates you inherited.
- **`WorkBudget` held its side as an integer and therefore did nothing.** A budget of 4 measuring 175 ms
  against a 110 ms target implies 3.17, damps to 3.58, rounds back to 4, and never moves; it sat at 4
  for every rebuild of a six-rung ladder. All three of its original tests used ratios extreme enough to
  clear the rounding in one step, so none of them saw it.

**Verified.** 475/475 native, `mod app` clean for wasm32, `cargo fmt` clean. Rig-measured on the
5060 Ti across a 100 km -> 1 m ladder, with `main` run through the identical rig as the control rather
than compared against a remembered number.

**NOT done, deliberately and flagged in place:** the space band's segment does not run the integral, so
it still shades as plain Lambert. Two scenes drawing one Earth differently is what docs/63 exists to
end — but the corridor's descent is not frame-reproducible, so the change cannot be A/B'd until the
fixed-pose rig (docs/63 item 1c) exists. Flagged rather than shipped unverified.

## 2026-08-02 — one surface: the globe and the cap collapse into a segment of a sphere

**What.** Terra and the space band both draw a planet's surface as ONE thing now — `terra::segment`, a
disc on the sphere whose angular radius is simply what is visible from the eye. The cube-sphere globe and
the tangent ground cap are both deleted, in both scenes, along with everything that existed to mediate
between them: `cap_fade`, `cap_lift_disp`, `cap_covers_view`, the `draw_globe` decision, the tier ladder,
`fill_ground_cap` itself. Net −323 lines from `lib.rs` on the Terra step; `ground_cap` fell from a builder
to 289 lines holding the two things that were never about having two meshes — the cache rule and the
raster hand-off.

**Why, in Robin's words, because the framing is the point.** Told that Ground looks good close up and
Terra looks good far away, she answered: *"BECAUSE it should ALL BE THE SAME. The Earth should be the
Earth, the Moon the Moon, no matter how close or how far the camera pans."* That is Law IV said exactly.
The camera was not choosing how finely Earth was drawn — it was choosing WHICH EARTH YOU GET, and an
explanation ending "each is good at its own range" is a statement of the bug, not a defence of the design.
Then the mechanism: *"we just map a texture onto a segment of a sphere, whether it's a small chord or a
full globe"*, and *"mathematically it is matter, but we only materialize that matter visually when we
need to, and only the amount we need to."*

**★ The blocker was the parameterization, not the idea — and finding that out collapsed the problem.**
`fill_ground_cap` was gnomonic (`center + east·du + north·dv`, normalized): a tangent plane projected onto
the sphere, which reaches 90° only as `du → ∞`. `CAP_MAX_ANGLE = 0.6` was never a tuning limit, it was the
projection's own asymptote. A polar parameterization has none — and the extent actually required is
**exactly a hemisphere**, because from any finite altitude you see strictly less than one. So the old
projection could not reach the one extent the job needs, and the new one reaches it by construction.

**★ Rings follow the camera's GAZE, not its feet.** Resolution concentrates at the segment's centre, which
is only right if the camera looks straight down. A descent looks AHEAD, so the first cut spent its fine
rings on ground behind the eye and the corridor came back with visibly jagged biome edges.
`segment::look_centre` intersects the view ray with the surface and centres there. This is a DRAW
decision and legitimately so: Law VI lets interest decide what is drawn, while only necessity decides what
is computed.

**★ Deleting the second mesh changed a measured number, which is the nicest kind of evidence that it was
really one thing pretending to be two.** A depth-fight lift exists ONLY to hold two copies of one surface
apart, so `CapTierBuild` lost `lift_m` and `tier_is_current` went from three conditions to two. The lift
was LINEAR in altitude and therefore tripped its octave first; with only the cell condition left — and the
cell going as √(h(h+2R)) — the mesh cache now survives about four times the altitude drop instead of two.
**A 500 km descent costs nine rebuilds where it used to cost eighteen.** The exact boundary is solved
rather than eyeballed: from 500 km the cell halves at 128,607 m, not the 125,000 a quartering suggests.

**Verified.** 463/463 native, `mod app` clean for wasm32. Terra's 11.9-decade ladder runs end to end at
p50 0.4 ms per rung with tiles streaming and no panics; the descent corridor completes all five stations
with zero console errors. Against the pre-collapse build, the A/B at four altitudes (10,000 km → 300 m)
gave ink identical, detail within noise and mean pixel differences of 1.3–3.4/255 — inside the same-code
drift band.

**★ A bug the browser found that a unit test could not.** On a polar segment the cell size VARIES —
squared ring spacing sends the local step toward zero at the centre — so `log2(base/cell)` ran past 11
octaves, the finest generated wavelength fell under a millimetre, and `world::value_noise`'s lattice
coordinate (position ~1e7 × frequency ~1e3) overflowed i32 and panicked. The centre does not have an
infinitely fine cell; it has the FIRST RING's, `cap/rings²`. A uniform grid could not hit this, which is
why the cap never did. `value_noise` now states its own limit rather than leaving a landmine: past the
i32 lattice it returns a flat field instead of panicking in debug and SILENTLY WRAPPING in release.

**★ And the rig that verified it was Mac-only until today, which is its own finding.** `mac_corridor.mjs`
carried a bare `chromium.launch({ headless: false })` — which CLAUDE.md rule 4b forbids outright — and a
default PORT of 7299. Its header blamed Apple Metal. Neither was true: it was a bare launcher and a dead
port, and one of the engine's headline behaviours was unverifiable anywhere but one machine. Renamed to
`corridor.mjs`, pointed at the shared launcher, and it runs here.

**NOT verified, and it bounds the claim.** The corridor's mid and low stations look different after the
collapse (brighter; the surface fills the frame where the globe used to occupy part of it), and they could
NOT be A/B'd at identical poses because the arc's descent is not reproducible frame-for-frame. Only the
pre-arc `1-celestial` frame is pose-identical, and it is unchanged (mean|diff| 0.63/255, ink 14.0 → 13.9%,
detail 33.08 → 33.02). A fixed-pose rig for the space band is the missing instrument.

## 2026-07-31 — the tier became a cache, the price collapsed, and the detail did not arrive

**What.** A ground tier is now a CACHE OF THE VIEW. `fill_ground_cap` emits vertices relative to a fixed
world `origin` instead of the eye, and the draw carries `anchor - eye` in the model matrix, so
`world = (p - anchor) + (anchor - eye) = p - eye` — the shader, and the triplanar anchor that reads
`wpos`, see exactly what they always did. `tier_is_current` then decides when a rebuild is owed. This is
the work the 2026-07-24 entry below named as next: *"a tier is a cache of the VIEW… anchor each tier's mesh
to a fixed point and carry the eye offset in the model matrix"*.

**The rebuild question is asked the way `air_reaches` asks about the air** — *would including this change
the answer* — never as a refresh interval, which would be a dial with no physics in it. Coverage: the
cached disc must still contain the one a fresh build needs, and the slack it may drift into is exactly the
`CAP_MARGIN` it was over-built by, so nothing new is declared. Resolution and lift: both must hold within
an OCTAVE, the unit `cap_fade` already spans for "unambiguously visible".

**★ An absolute angular test was tried FIRST and is wrong — the failure is the interesting part.**
`|Δcell| / range ≤ θ`, the docs/49 budget that sizes everything else in this module, collapses at low
altitude: a horizon-sized cap has ~52 m cells at 2 m altitude, so ANY descent changes them by more than the
eye resolves and the answer is "rebuild" forever (**measured: ~13,000 rebuilds per halving**, worse than
the per-frame rebuild it replaces). It is not lying — those cells really are visibly coarse. But the cure
for a mesh too coarse to express the ground is another TIER, not another rebuild of the same one, and the
budget cannot tell those apart. The octave measures a rebuild against what a rebuild can actually deliver.

**Verified.** 452/452 native, `mod app` clean for wasm32. The anchoring is pinned vertex-for-vertex
(`an_anchored_cap_draws_exactly_where_an_eye_relative_one_did`): anchored mesh + model translation IS the
eye-relative mesh, not an approximation. Rig on the 5060 Ti, paced to ~60 fps
(`web/rig/terra_anchor_drift.mjs`), the same rig run on `main` and on this branch:

| | p50 render | frames per 1.2 s window |
|---|---|---|
| `main`, 4 tiers | **700–772 ms** | 15–17 |
| anchored, 4 tiers | **0.4 ms** | 134–161 |
| anchored, 1 tier | **0.4 ms** (was 45.2) | — |

**★ And the picture is unchanged — proven against the confound, not asserted.** Branch-vs-`main`
screenshots differ, but so do two runs of the SAME code, because the starfield rotates and the sun moves
between runs. So the control was measured: branch-run-1 vs branch-run-2 (same code, ~40 min apart) differs
by mean 2.5–4.8/255, while branch vs `main` differs by **0.43–0.64** — five to ten times LESS than the
run-to-run drift. A raw before/after diff here would have read as a large change and been entirely time.

**★ The negative result, which is the half that matters.** Tiers are now affordable, so the obvious next
move was to raise `TERRA_DEFAULT_TIERS` off 1. **Measured first, and it does not earn its place.** A/B at a
fixed camera over the Himalaya, 1 tier vs 4, at the full 16-octave relief budget:

| altitude | max pixel Δ (1 vs 4 tiers) | ground luminance stddev, 1 → 4 |
|---|---|---|
| 8,000 m | 18 | 8.80 → 9.54 |
| 2,000 m | 8 | 1.46 → 1.92 |
| 500 m | 6 | 1.12 → 1.10 |
| 100 m | 4 | 0.94 → 0.98 |

Below ~2 km the fourth tier changes nothing a person could see, and the ground gets FLATTER on the way
down (stddev 8.8 → 0.9), which is the opposite of the ask. The first run of this A/B used a 4-octave
budget and I nearly reported it as-is; at 4 octaves the finest generated wavelength is 19.55 km / 2⁴ =
1.2 km, so the test had been starved. Re-run at 16 and the conclusion is unchanged — which is the only
reason it can be stated.

So the 2026-07-24 diagnosis — *"the mesh is the limit, not the maths"* — was right about the COST and is
not the whole story about the DETAIL. Anchoring removed the cost (642 ms → 0.4 ms) and the detail did not
follow, so something else is between the camera and detailed ground. `TERRA_DEFAULT_TIERS` stays 1, now
for a measured reason rather than a budgetary one.

**The hitch, and the fix (same day).** Anchoring turned a sustained per-frame cost into a rare one, but a
4-tier rebuild was still a **224–310 ms freeze**. The cause is worth stating because it is not obvious: the
staleness tests are RELATIVE (an octave of a quantity that scales with altitude), so on a descent every rung
of the ladder trips at the *same altitude* and all four re-derive in one frame. `tier_owed_a_rebuild` now
spends the frame's whole rebuild budget on the outermost tier that needs one. **Worst frame 224–310 ms →
61–79 ms** — one tier's rebuild, which is the floor for this approach — with p50 still 0.4 ms.

Outermost-first for two reasons rather than one: it is the tier whose staleness shows worst (a coverage gap
at tier 0 is a hole at the horizon with only the coarse globe behind it, while a gap in an inner tier just
reveals the coarser tier drawn underneath), and it keeps the built set a PREFIX, which is what lets
`tiers_ready` be a count rather than a mask. That count is load-bearing: a tier that has never been built
holds an empty vertex buffer, so the draw is capped at it, and the globe is no longer skipped while the
outermost tier is still missing — otherwise the frames right after a world load would draw nothing at all.

Deferring is affordable because a deferred tier keeps drawing its cached mesh and the amount it is allowed
to be wrong by is the `CAP_MARGIN` it was over-built by — tens of kilometres of ground at low altitude
against the metres a camera covers in the three frames it waits. Verified: 455/455, and the rendered frames
differ from the pre-spread branch by mean 2.6–5.2/255, inside the 2.5–4.8 same-code drift band measured
above — i.e. not at all.

**★ And then the actual question: WHY does the ground flatten on descent** (docs/46 row 27). Two causes,
measured, and the dominant one is not where I looked first. **It is not the amplitude law** — run the same
generator at `slope_fraction = 1.0` and it gives RMS slope ~1.0 and 106 m of relief inside a 109 m frame,
violently rough. The generator is fine; what multiplies it is not.

1. **`slope_fraction` compares two quantities measured four orders of magnitude apart.** It is
   `tier_slope / mu`, where `tier_slope` is the elevation gradient over a baseline of two raster texels —
   **39 km** on the shipped Earth — and `mu` is a grain-scale friction coefficient. A 39 km baseline is a
   regional TILT and cannot be steep. Measured over the shipped raster at 4,096 land points: median
   **0.00202**, p90 0.01149, largest anywhere **0.0667**. **Everest itself reads 0.0328**, because
   averaging over 39 km flattens it. In frame at 100 m altitude that is 11.8 m of relief at Earth's
   roughest point and **0.36 m on median land**.
2. **The amplitude law is scale-invariant, so approaching cannot reveal roughness even in principle.**
   `relief_amplitude_m` is `min(drop/2, λ/4)`; for cohesive rock the cohesion term wins the OR everywhere a
   camera cares about (granite's `h_crit` is 453 m), so the binding term is the `λ/4` no-overhang cap — a
   HEIGHTFIELD property, not the rock's. Amplitude ∝ wavelength is Hurst exponent **H = 1**, the smoothest
   self-affine surface and the one whose slope is identical at every scale. Real topography has H ≈ 0.5–0.7.

Together they are the symptom exactly: above ~20 km the frame spans more than one texel and shows real
measured mountains; below it the frame fits INSIDE one texel, the raster contributes only a smooth ramp,
and every visible bump must come from a generator scaled to ~0.003 that cannot roughen as you approach.

**No fix attempted, deliberately** — the fix is not a multiplier, that is a dial (Law V), and a wrong
exponent cannot be corrected by a coefficient anyway. Pinned by two new tests so the diagnosis cannot be
re-derived from scratch, and both are written to FAIL if either half is ever corrected.

**★ Then the exponent was MEASURED, and it is the accepted one.**
`earths_topography_is_self_affine_and_its_exponent_is_not_one` takes the structure function of the shipped
raster over land — golden-angle points, great-circle pairs, both ends land, because the sea floor is a
different surface and the coastline between them is a kilometres-tall step. RMS Δz runs 137.9 m at 19.5 km
to 905.2 m at 625 km; the log-log slope over 39–625 km is **H = 0.483** (5,846 land points, ~40k pairs per
lag). Turcotte's spherical-harmonic spectrum gives S(k) ∝ k⁻² below 10,000 km with β = 2H+1, so β ≈ 2 means
H ≈ 0.5, and a recent Earth/Venus roughness study reports H ≈ 0.55 at β = −2.1 — the measurement landed on
the literature without being aimed at it. **The generator's implied H = 1 is 50× too smooth at 10 m
wavelength**, and the same measurement supplies the anchor a fix would need: RMS Δz at one texel = 137.9 m,
a real roughness rather than a regional tilt. Caveat stated rather than buried: the local slope drifts from
~0.77 at the shortest pair to ~0.41 at the longest, so one H over that range is a fit, not a constant, and
extrapolating it four more decades to metre scale is a declared model whose resolved counterpart is data.

**Finer elevation data exists and is free — and it is the PRIMARY fix, not a companion to the exponent.
★ I first wrote the opposite and Robin caught it.** Copernicus GLO-30
(30 m, TanDEM-X, COG on AWS Open Data); ETOPO 2022 (15 arc-sec ~450 m topo+bathy, NOAA public domain — the
drop-in upgrade, being the same *kind* of seamless land+sea product); AWS Terrain Tiles (global z/x/y
**terrarium PNG**, no auth, RGB-packed metres — the same encoding this engine already reads). The generator must cover from the data's resolution down to 0.1 m, and that
span is what decides how much the exponent matters: **5.29 decades today (H=1 wrong by 544× at 0.1 m),
2.48 with GLO-30 (19×), 1.68 with z=15 tiles (7.4×)**. And the part I missed: finer data **repairs the
`slope_fraction` category error BY CONSTRUCTION**, because that bug is really that two texels is 39 km —
at 30 m texels the baseline is 60 m, where real ground does approach what the material can hold, so the
ratio finally measures what its name claims. With 30 m data there is real relief in frame at 94 m altitude
and the flat green fill is simply gone.

What data still cannot do: reach 10 cm (2.5 decades short at GLO-30), or ship as a file — even ETOPO 2022
is 7.5 GB raw, and a single file within a web budget buys ~5–10 km per texel, three decades short. So the
route is **tiled, fetched-by-necessity data** (docs/44's ladder applied to data instead of matter; AWS's
terrarium tiles are already the encoding this engine reads), with the exponent covering the residual.

**★ THE NORTH-STAR LADDER, WALKED.** Robin: *"from mars-earth distance to 10 cm above the surface …
so we can prove our frame of reference/increased detail system works … without noticeable frame-rate
impact."* `web/rig/terra_scale_ladder.mjs` walks 25 rungs over **11.9 decades**, 78 Gm (Mars at opposition)
to 0.10 m, paced to ~60 fps.

**Frame rate holds: p50 render 0.4 ms at EVERY rung, top to bottom.** No cliff in twelve decades; the only
blemish is the known 38–70 ms tier-rebuild hitch. **The picture is continuous for nine decades and frozen
for three.** Ink rises 0.7% → 74.7% as the planet grows in (rungs 5–8, smooth and monotonic), fills the
frame by 2.7 Mm, and then detail decays 13.97 → 4.12 between 2.9 km and 94 m and **freezes at 4.12–4.13 for
the last six rungs — 94 m, 30 m, 9.6 m, 3.1 m, 0.98 m, 0.31 m, 0.10 m**. At 8,474 km the frame is a
recognisable Earth with continents, biomes, limb and stars; at 94 m it is a flat green fill, and it does not
change again. **The last three decades of the zoom deliver no information.** That is docs/46 row 27
rendered rather than argued — the scale machinery is sound across the whole ladder, and what runs out is
the surface DATA MODEL.

**★★ Two traps, both already written down here, both walked into anyway.** (1) The first run was black
below 4,300 km and looked like a renderer collapse — it was lon 86 at 17:00 UTC, the dead of night.
Measured: 0.3% lit at lon 90, 78% at lon −90, sub-solar longitude −75°. `terra_lit_probe.mjs` now answers
that question before it can be mistaken for a bug. (2) The rig measured its own frames by `drawImage`-ing
the WebGPU canvas on a later tick and got BLANK — rule 0 says a WebGPU drawing buffer is readable only while
current — so it reported 0% ink for frames that plainly contained a planet. **A rig that reports a
confident wrong number is the same failure as a gate that passes on error.** Deleted; the PNGs are the
measurement. Robin's steer on the first one is the better fix and is not built: a test rig should command
the clock rather than wait for the sun.

**★ THE EPOCH KNOB — a rig commands the clock now** (Robin: *"this being a test rig, you should be able to
rotate earth as you see fit to run a test?"*). `Terra::set_epoch` / `clear_epoch` / `set_epoch_sun_over_lon`
/ `sub_solar`, over one `celestial_epoch_s()` that both the terminator and the star field read — the scene
was calling the wall clock in three independent places. The aiming lives in `orbit::epoch_for_sub_solar_lon`
(Newton on the subsolar longitude, converging in two steps) rather than in a rig, because the rule a harness
writes for itself — subsolar longitude ≈ 180° − 15°·UTC_hours — has no equation of time and no
sidereal/solar day distinction and is wrong by degrees, which is exactly the error that makes someone
mistake dusk for a bug. Deliberately NOT the simulation's clock: the flight advances on elapsed wall time
because that is a duration, while this is an instant, and a rig that froze both could not film anything.

**Verified three ways.** (1) The site that was pitch black below 4,300 km — lat 28, lon 86 — now renders at
every rung, ink 100% all the way to 0.10 m. (2) **Reproducibility: two runs differ by mean 0.003–0.26/255,
against the 2.5–4.8 free-running drift** that previously had to be measured with a control run before any
screenshot comparison could be believed. (3) It exposed a confound in the detail metric nobody had named:
aiming the sun at the site's own longitude is local NOON, the worst light for relief. The same Himalaya
rungs read detail **0.65 at noon and 4.0–4.5 under slanting light**, so the rig now offsets the subsolar
longitude 70° east by default. **And the freeze survives the correction** — detail is 4.04 at 30 m, 9.6 m,
3.1 m, 0.98 m, 0.31 m and 0.10 m alike — so the flat ground is an absence of geometry, not of shadow.

**NOT done.** 61–79 ms is still a visible hitch; going below it means splitting a single tier's rebuild
across frames, not scheduling whole tiers. Also untouched: the `terra_lod_cost.mjs` table in the entry
below is now historical — it measured the per-frame rebuild.

## 2026-07-25 (step 5) — the environment answers where its own surface is, and terrain stops being special

**What.** Sean's `upstream-5-ground-ball` is integrated: the declared ball is cohesive matter whose fate is
emergent. Three of his additions moved rather than merely landing, because `flight::Flight` owns the
trajectory and the scene no longer sees the swept segment. `FlightEnvironment::arrival` now takes the body
and the substep and returns a `Met` — where the path crossed hard matter, and what that meeting delivers.
`GroundAir` carries the cohesive bodies and asks about them BEFORE the terrain; his `forecast_body_contact`
lives there now, still calling `interaction::detect_swept` and still taking that door's reduced-mass energy.
`deposit_event` is the one door every ground arrival goes through, momentum included.

**Why — his change fixed a bug of ours, and a second one fell out.** `arrival` returned `to`, the post-step
sample. At 17 km/s a 1/60 s step is ~283 m, so an impact coupled to matter hundreds of metres underground,
and `PlanetAir::arrival` had the identical line where the speeds are higher still. Both now call one
`flight::surface_crossing`. Then the second: `interaction::detect_swept` already priced every body-body
collision at ½·μ·|Δv|² — *"the energy actually available at the contact frame"* — while the terrain path
wrote ½·m·v² separately. **Those are one law, and terrain is its immovable limit** (μ → m, Δv → v). One
`flight::delivered(body, target_vel, target_mass_kg)` now serves both, `None` meaning immovable.

**★ The bug I introduced, and what caught it.** Moving the bisection meant choosing how far to bisect. I
derived the tolerance from the GRAIN size, on the reasoning that `deposit_event` floors coupling distance at
the grain scale so a finer site changes nothing. **False, and every test but Sean's still passed.** The site
is also *where the material is sampled*: a grain-sized tolerance left the bracket 0.83 m wide, put the site
0.795 m UNDER the surface, and `strength_at(site)` read the material below the ground — different strength,
different crater radius, different coupling length λ, and λ sits inside an exponential. Measured on his
`an_impact_event_heats_debris_grains_already_in_flight`: the grains' coupling weight fell **284×**
(0.2416 → 0.00085). It now resolves to the surface FIELD's precision (f32, 30 halvings), not to the
resolution of the matter. **A tolerance argued from the consumer was wrong because it had two consumers.**

**★ A knife-edge test, honestly re-based.** His `a_sufficient_meteor_shatters_the_ball` asserted
`spread > 2.0 × spread0` and passed on his branch — in VACUUM. Through this patch's real air a 17 km/s body
arrives with 1.645e11 J instead of 1.720e11 (2.2% of its speed lost over the last 80 m) and the scatter falls
to 1.51×. Verified by ablation: forcing `air_at` to `None` makes the 2.0× bar pass again, so the atmosphere
is the whole of it. The physics got more honest and a threshold with ~1% of headroom went red. Rather than
retune the multiple onto today's number, the bound is now the body's own lattice half-spacing. **"And it
keeps growing a second later" was tried and is FALSE — 1.91 m → 1.91 m:** under real gravity onto real
terrain a shattered pile settles rather than dispersing, so a test must not claim the parcels are unbound.

**★ Robin's question found an unpinned path: impacts on a vacuous world.** *"We do need to be certain that we
CAN have impacts on a vacuous world like the moon."* `a_world_with_no_air_flies_its_bodies_through_real_vacuum`
broke out of its loop on the first arrival but **never asserted one happened** — and it never did: 400 steps
of 0.05 s is 20 s, while the 1,000 m drop at lunar 1.62 m/s² takes 35.1 s. Every claim in that test also held
for a body that simply never landed. It now flies long enough to arrive and asserts the arrival, a site on
the surface, zero ablated mass, and energy equal to the closed form ½mv₀² + mgh — the quantity drag eats into
on a world that has air. **A test that stops on an event should assert the event.**

**Verified.** Native **393/393** (23 skipped), wasm32 clean. New pins:
`an_immovable_target_is_the_reduced_mass_limit` (convergence over 10³…10¹⁵ × the striker's mass) and
`an_arrival_sites_the_impact_on_the_surface_not_where_the_step_ended` (ground and planet together); both fail
against the previous code, so neither passes vacuously.

## 2026-07-25 — two PRs said MERGED and `main` never got them

**What.** Sean's step-4 (GPU gravity dispatch) and the `merge-reports/` convention are on `main`. They were
supposed to arrive as PRs #87 and #88, which both merged cleanly and both closed as **MERGED** — into their
*base branches*, not into `main`. The six-PR stack was retargeted to `main` as it landed for #83–#86 and not
for the last two, so #87 merged into `integrate-sean-3-sph-live-drop` and #88 into
`integrate-sean-4-gpu-gravity`. Both branches are still alive on the remote, holding content nobody knew was
missing. This merge recovers both.

**Why it stayed invisible.** Every signal said done. The PRs were closed, the checks were green, the branches
merged without conflict, and the PR list was empty but for dependabot. Nothing anywhere reports "this merge
did not reach `main`" — a PR's state describes the PR. The question has a one-line answer and it has to be
asked deliberately: `git merge-base --is-ancestor <branch> origin/main`.

**The second casualty, which is the more interesting one.** #88 deleted `docs/60-integrating-the-fork.md`,
having moved it to `merge-reports/2026-07-25-sean-reid.md`; #86 had added `docs/60` to `main`. Because the
split put those two changes on opposite sides, the three-way merge saw a file created on one side only and
kept it. Git behaved exactly as specified and the result was wrong: `main` would have carried **two audit
trails of one integration**, each editable without the other — Law II in the documentation rather than the
code, and a merge is precisely where that gets welded in silently. `docs/60` is deleted here after checking
line by line that the report carries all of it.

**Verified.** Native suite **380/380** (23 skipped) after the resolution; `cargo build --lib --target
wasm32-unknown-unknown` clean, which is the check that catches `mod app` (WASM-only, invisible to a native
`cargo check`). Both recovered branches confirmed ancestors of the merge; no conflict markers survive.
Conflicts were `CHANGELOG.md` and `JOURNAL.md`, both append-only logs with an empty side, unioned.

## 2026-07-24 (tier wired) — the mesh is the budget, not the maths

**What.** `surface_detail` now has a consumer: Terra's ground cap generates sub-raster relief, bounded by
what `granular`'s Mohr–Coulomb says the material can stand and scaled by how steep the ground already
measures. The tier ladder is built (nested caps, each covering a quarter of the span of the one outside it,
all from ONE builder), and the `lift` bug is fixed — it was a constant 20 m that exceeded the eye height at
low altitude and put the ground ABOVE the camera; it now scales with altitude and is flagged as a
depth-precision allowance rather than anything physical.

**The measurement that decided the design, and it inverted my assumption.** I expected the generated octaves
to be the cost. Priced at 2 km over the Himalaya, paced to ~60 fps (`web/rig/terra_lod_cost.mjs`):

| tiers | octaves | p50 frame |
|---|---|---|
| 1 | 0 | **45.2 ms** ← the ladder as it shipped, before any of this |
| 1 | 2 | 51.2 ms |
| 1 | 6 | 50.1 ms |
| 1 | 15 | **51.6 ms** ← fifteen octaves cost 14% over zero |
| 2 | 4 | 126.5 ms |
| 3 | 4 | 159.2 ms |
| 4 | 4 | 642.5 ms |

**Generated relief is nearly free; TIERS are expensive.** What costs frame time is rebuilding and
re-uploading a 192² camera-relative mesh — which Terra was already doing once per frame for **45 ms** before
this work, a pre-existing cost nobody had measured. So the octave budget is generous (16) and the default
tier count is **1**, with the capacity and the knob left in place because the nesting is right and priced.

**Also measured, and it reframes the whole feature: the elevation raster is 2048×1024 — 19.55 km per pixel.**
Everest is one pixel. So below ~20 km altitude essentially ALL visible terrain relief is generated rather
than measured, and reaching metre scale needs ~15 halvings. That is legitimate — it is bounded by the
material's own slope law, deterministic, and flagged — but it must be said plainly rather than described as
"micro-detail on top of real data".

**Verified.** 371/371. A/B'd with the knob at fixed camera and altitude (`terra_lod_ab.mjs`): at 500 m the
skyline is a straight line with octaves off and visibly undulating with them on. So the rule reaches the
picture.

**NOT done, and the measurement is what names it.** Standing on detailed ground still is not there: one
tier's cells are ~540 m at 500 m altitude and ~59 m at 6 m altitude, so the relief you can see is
hundred-metre-scale shape, not texture underfoot. **The mesh is the limit, not the maths** — and the fix is
the one Robin gave when she set this task: *"we don't have to make things renderable at planetary scale while
viewing subset of surface; we have the math — we can rebuild it if the camera moves again."* A tier is a
cache of the VIEW. Anchor each tier's mesh to a fixed point and carry the eye offset in the model matrix
(`fill_ground_cap` currently bakes `surface - eye`, so every camera motion invalidates every vertex), and a
tier then only needs rebuilding when the camera leaves it. That is what makes tiers 2–4 affordable, and it is
the next piece of work.

## 2026-07-24 (surface detail) — Robin caught me writing a second answer to a settled question

**What happened.** Starting the `surface_detail` LOD tier, I needed "how rough can this ground be?", and I
wrote a rule for it: amplitude bounded by the material's angle of repose. Robin stopped me:

> *"Wait… shouldn't this material work (sand/granite angle of repose) already be hard baked into the engine?
> How/why were you able to change it and get it wrong with impugnity?"*

It is, and I had. `granular::repose_allowance` / `face_stable` (docs/45) is Mohr–Coulomb, and its own
documentation states the reason it exists: *"this makes ground and grain answer the slope question with ONE
law."* I wrote a second law in a second module and every test still passed.

**And it was wrong in both of the ways that module already warned about.**

1. **Friction alone.** The material table refuted it immediately, which is the only reason I noticed: dry
   sand grips HARDER than granite (μ 0.67 vs 0.60), yet only granite stands vertical. Sand's cohesion is
   **0**; granite's is 28 MPa. A friction-only rule made sand the craggier material.
2. **Then friction + cohesion added into one slope** — and `granular`'s doc had already written that
   conflating the friction height with the cohesion height is *"subtly wrong in exactly the case a layered
   world is made of"*, with the measurement to prove it (470 grains shed from a world nothing had touched).
   They are an **OR over two different measurements**: friction acts on the SLOPE, cohesion on the BANK.

**Why it was possible, which is the part worth fixing.** Nothing guarded the proven law. `laws.rs` hardens
constants (`SINGLE_SOURCE`) and keeps scenes out of collision (`COLLISION_PRIMITIVES`), but a *derived law*
had no such protection — so a reasonable author who did not find the primitive could contradict it silently.
That is docs/46's stated failure mode happening to me, in the module whose own header cites it.

**The fix, and the hardening** (Robin: *"We likely need to build tests to harden proven aspects of the
engine"*):

- `surface_detail::relief_amplitude_m` now holds **no slope physics at all**. It asks
  `granular::repose_allowance_on` and `granular::critical_bank_height`, takes the OR exactly as
  `face_stable` poses it, and only converts a permitted DROP into a sinusoid's amplitude.
- `granular::repose_allowance_on(mu, r, quantum)` — the same law with the field's quantum as a PARAMETER.
  `SLOPE_QUANTUM_M` is the voxel heightfield's own quantisation and belongs to that field, not to the
  physics; a continuously generated surface passes 0. Splitting it out is what made delegation possible
  instead of a re-derivation.
- `granular::critical_bank_height(c, ρ, g)` — `h_crit` was computed inline in `matter.rs` while its partner
  term lived in `granular`, i.e. half a law in each place. `matter.rs` now delegates.
- **The guard is a CONSISTENCY test, not a grep**: `generated_relief_is_stable_by_the_engines_own_slope_law`
  hands every octave of generated relief back to `granular::face_stable` as the face it implies, for seven
  materials across six orders of wavelength, and requires it to be held up. A name check would not have
  caught what I did; this does, and it fails if either side's physics moves.

**Two findings the tests produced, both counter to what I assumed.**

- **Below ~2 km wavelength, cohesive rock is limited by the HEIGHTFIELD, not by physics.** Granite's cohesion
  permits a 1,057 m bank, so its ceiling across that whole range is `λ/4` — the point at which a height
  cannot overhang. Rock is simply "as rough as a heightfield can express" there, and the honest form of an
  overhanging crag is matter, not a height.
- **At kilometre scales SAND stands steeper than granite**, because its friction really is higher (0.67 vs
  0.60). Cohesion is a small-scale term; it is only cohesion that makes rock the steeper material.

**Verified.** 370/370 (3 new). Nothing rig-verified here because nothing visual changed yet.

**NOT done:** the tier itself. The rule and its guard exist; Terra still has one LOD tier, and wiring the
finer one is the remaining work. Also still true: `relief_amplitude_m` is a CEILING — `slope_fraction` (the
local measured slope as a fraction of it) is what stops a flat plain coming out as rough as a mountainside,
and Terra will have to compute that from the elevation raster's local gradient.

## 2026-07-24 (Stage B) — the observer and the universe

**What.** docs/59 Stage B: ride a fragment down. Robin redirected it into something better than a follow —
*"can we feed camera coordinates, FOV to engine? … a different thread could drive its position, framing"* —
and then named the principle: **"This matches an observer/universe scenario. The universe handles all the
physics, the viewer watches."**

So there is no follow mode in the engine. `Terra::set_camera_pose(eye, forward, up, fov_y)` is the whole
interface; the engine renders from exactly that and derives latitude, longitude, altitude and therefore the
terrain LOD from it. The chase rule lives in the scene, where it is ~20 lines of vector arithmetic over two
engine questions (`heaviest_fragment`, then `fragment(id)` each frame). Nothing in the engine learned what
"following" means, and the same interface will take a script, a device, or code on another thread.

Fragments gained stable `id`s, because an INDEX is not a handle: bodies leave the list when they arrive, so
slot 7 is not the fragment that was at slot 7 a second ago.

**Two things fell out of taking the split seriously instead of adding a mode.**

1. **The FOV stopped being duplicated.** It was written in `fly_camera` and again in the matter shader's
   billboard sizing — so the "one pixel" sampling floor silently stopped being one pixel if either moved.
   The pose carries it, `View::fov_y` reports what the frame was built with, and the shader reads that.
2. **The near plane became the engine's job.** The fly camera derives it from ALTITUDE — right when the
   nearest visible thing is the ground below, badly wrong otherwise. Riding 82 m behind a fragment at 218 km
   altitude put the near plane **104 km** away and clipped the very thing being followed: a starfield and a
   working HUD with nothing in the middle. The engine knows how close its own matter is, so it answers that
   now, which is precisely Robin's *"it gets to track what is observed"*.

**And a hard limit, flagged rather than worked around.** One f32 depth range cannot hold an 80 m fragment
AND a 6,371 km planet: dropping `near` to the fragment collapsed precision and the globe stopped drawing at
all. So the ratio is bounded (`near` never below a ten-thousandth of the altitude) and the observer stands
back proportionally as it climbs — the engine's one-pixel floor is what keeps the fragment visible when it
does. The real fix is DEPTH PARTITIONING, named at the call site; `vp_rel` already exists for the same class
of problem on the ground cap.

**Verified** (`terra_follow.mjs`, paced to ~60 fps per CLAUDE.md rule 4b): a fragment ridden from **528.6 km
to 0.1 km, 39 of 39 samples descending**, worst render ~3 ms, the fragment visibly incandescent at its own
3,134 K with a cooling parcel beside it, daylit terrain filling the frame from ~30 km down, and the camera
releasing itself the moment the fragment lands. 367/367 native.

**NOT done — and it is the half the stage is named for.** The surface never gets FINER. At 70 m the ground is
a flat green fill: right biome, no relief, no granularity. Altitude descends continuously and the globe→cap
crossover happens, but "seeing better LOD as we descend" is unmet. docs/59 predicted this: the
`surface_detail` LOD-tier blocker (docs/46), where Terra has no finer tier to fade into. It is also a
prerequisite for Stage C — a crater that sharpens as you approach needs a surface that can sharpen at all.

**Also fixed:** `launch_swarm` hand-rolled lat/lon→direction with the OPPOSITE sign on z from
`crate::geo::dir_from_lat_lon`, so the swarm aimed at a mirrored longitude and landed nowhere near where the
camera pointed. It cost me three rig runs believing the terrain was failing to render when I was simply
looking at the night side each time. CLAUDE.md warns about exactly this: the tangent frame was once six
hand-written copies, and the one sign they all shared was wrong.

## 2026-07-24 (late) — "we lose camera controls when the engine is working"

**What.** Robin reported losing camera control during an entry. Two separate things turned out to be true:
a real defect I had introduced, and a measurement artifact I chased further than I should have.

**The real defect: a feedback loop of my own making.** Terra clamped its frame `dt` to a QUARTER SECOND and
then derived the flight's substep count FROM that `dt` — and the trail was re-aged inside every substep. So
a slow frame produced a bigger `dt`, which asked for more substeps, each of which walked every trail parcel,
which made the next frame slower still. That is a compounding stall, and it is exactly what "the page stops
answering the mouse" looks like.

Fixed by moving the judgement where it belongs: `FlightEnvironment` now reports its air's scale height, and
`Flight::step` sizes its OWN substeps from the distance a body actually travels against it — never from the
caller's frame time. The trail ages once per call, not once per substep (shed vapour is dragged to rest in
milliseconds and needs none of the resolution a hypervelocity body does). Terra's `dt` clamp is a thirtieth
of a second, so a stalled frame or a backgrounded tab cannot turn into a large catch-up step. Also: the
flight step was calling `planet::body()` EVERY FRAME, which deserializes the body's JSON — rebuilding the
planet, in full, to ask what gravity is. Resolved once now, and re-resolved only when a world loads.

**The artifact, and how far it got.** With ~1,200 instanced billboards the rig showed roughly one frame per
second taking **450–520 ms** inside `render()` while the median was **1.5 ms** — reproducible across runs
and scaling with the swarm. I priced it properly rather than guessing, and the ablation ladder was worth
having:

| what ran | frames/8 s | p50 | max | stalls >200 ms |
|---|---|---|---|---|
| physics + upload + draw | 1372 | 1.5 ms | 507 ms | 8 |
| physics + upload | 1362 | 1.5 ms | 516 ms | 9 |
| physics only | 2839 | 1.1 ms | 53 ms | **0** |

A valid ablation (the physics is identical in all three — 1,200 bodies in flight either way), and it priced
the **upload**, not the physics and not the draw. I then tried a three-buffer ring, on the theory that the
GPU was still reading the buffer being overwritten. **It measured no improvement, so I removed it** rather
than keep plausible-looking complexity with a comment claiming a fix.

Then the actual answer: `scripts/rig.sh` runs Chromium with `--disable-frame-rate-limit`, so the page
rendered at **170–350 fps** and pushed several times more per second through `queue.write_buffer` than any
vsynced browser will. **Paced to ~60 fps, the same scene never exceeded ~10 ms and never stalled at all.**

| pacing | renders/10 s | p50 | p99 | max | stalls |
|---|---|---|---|---|---|
| uncapped (170–350 fps) | 1724 | 1.5 ms | 18.2 ms | 499 ms | 10 |
| ~60 fps | 369 | 1.6 ms | 3.0 ms | **4 ms** | **0** |

This is the same flag as the 1 Hz trap in `CLAUDE.md` rule 4b, in the opposite direction: that one HID a
real collapse, this one INVENTS one. Both entries now sit together there, with the pacing check and the
ablation ladder named as rigs (`terra_vsync_check.mjs`, `terra_price_stage.mjs`).

**Verified.** 367/367. `terra_controls_60.mjs` drives a real right-drag and wheel zoom before, during and
after an entry at realistic pacing: the camera moves 41–43.6° each time and the worst render during the
entry is 3.3 ms. Worst render across a whole entry, paced: **9.8 ms**, inside a 16.7 ms budget.

**What I did NOT verify, and will not claim:** the paced cost at PEAK TRAIL (~24,000 instances). Both
attempts to catch that phase missed it — the first because my own camera drag moved the swarm's target
before launch, the second because the entry ran slightly slower than the sampling window. The uncapped
number at that phase (~27 ms/frame) is contaminated by the artifact above and is not usable. If a real
browser does drop frames at peak trail, `Flight::set_trail_budget` is the lever, and it should be set from a
measurement rather than from a guess.

**Robin's suggestion, recorded not built:** *"camera placement can be controlled in the scene and handed
over to the engine for rendering… That would allow us to place camera control monitoring in a separate
thread."* Worth doing on its own merits — it is the same scene-declares/engine-computes split as the rest of
this work, and moving input off the render thread makes responsiveness independent of frame cost rather
than merely good. Not built here: the measurements above say the immediate cause was the compounding step,
not the input path (the camera answered input even while stalling), so threading would have hidden the
defect rather than fixed it.

## 2026-07-24 (night) — row 21 closed: only the skin heats, so a metre-class body glows

**What.** `atmospheric_step` raised the temperature of a body's WHOLE mass at its bulk heat capacity.
Thermal response therefore scaled with VOLUME, and a half-metre iron body flew a perfectly correct 20 km/s
entry and barely warmed — while real iron meteorites arrive with a molten fusion crust over a core cold
enough to frost. Ablation is a SURFACE process.

Now the heat front advances at the material's own thermal diffusivity `α = k/(ρc)` and only the mass it has
reached takes part: `atmosphere::soak_depth` (from `δ = √(αt)`, so one step is exactly `δ' = √(δ² + α·dt)`)
and `atmosphere::heated_mass` (the shell `ρ·(4/3)π[r³ − (r−δ)³]`). Bulk heating is not replaced — it is the
LIMIT the model reduces to once the skin reaches the radius, which is why the existing operator tests pass
unchanged by passing `skin = r`.

**Data first (Law VII SOP).** `thermal_conductivity` is now sourced in `data/materials.json` for 28 of 29
materials — rocks from Clauser & Huenges 1995 (AGU Reference Shelf 3), metals and gases from the CRC
Handbook, the moisture-dependent soils carried with explicit `estimated` notes, and `hh_plasma` left
UNKNOWN rather than guessed. `docs/04` had reserved this field and nothing had ever used it; that section
now records the pattern, because "reserved, not used yet" hid the consequence until something needed it.

**Verified.** 367/367 native (2 new), wasm builds, rig-verified on the 5060 Ti. MEASURED before → after,
8 iron fragments at 15 km/s from 200 km:

| parent radius | ablated before | ablated after | peak surface T after |
|---|---|---|---|
| 1 cm | 94.7% | 94.7% | 3134 K |
| 5 cm | 27.5% | 31.1% | 3134 K |
| 10 cm | 6.0% | 14.6% | 3134 K |
| 30 cm | **0%** | **4.3%** | **3134 K** (was a few hundred K) |
| 1 m | **0%** | **0.75%** | **3134 K** |
| 3 m | **0%** | **0.03%** | **3134 K** |
| 10 m | 0% | 0% | 2346 K |

The live swarm's ablated mass went **397 kg → 15,871 kg**, and the Terra swarm's ICs no longer have to be
shrunk to fit the model: it is a 3 m, ~890 tonne iron asteroid resolved into 1,200 fragments (11 cm to
1.85 m), which is the disintegrated asteroid Robin actually asked for.

**Three things fell out that nothing declares.**

1. **The heated layer SETTLES rather than growing** — ~1.4 cm for iron, ~2.8 mm for basalt. Ablation strips
   the skin as fast as conduction deepens it, and the balance is the classical `δ = α/2v` thermal boundary
   layer of a receding surface. Nobody wrote that thickness; it is where two rates meet.
2. **Basalt ablates MORE than iron at every size** (99.8% vs 94.7% for pebbles, 3.7% vs 0.75% at a metre),
   because its diffusivity is 21× lower so the heat stays at the surface instead of being conducted away.
   That is why stony meteorites carry thin fusion crusts and shed more of themselves than irons do.
3. **A 10 m body still does not reach boiling, and that is Sutton–Graves being RIGHT**, not a limit left
   over: stagnation flux goes as `√(ρ/R_n)`, so a blunter body is heated less — the reason re-entry
   capsules are blunt.

**How the test proves it rather than asserting it.** `a_metre_class_body_glows_because_only_its_skin_heats`
flies the SAME 1 m iron body twice, differing only in `skin_m`: fresh (the real case) reaches 3134 K and
ablates; pre-soaked to the radius (the old bulk case) stays under 1500 K and ablates exactly nothing.

**Still open, and smaller** (docs/46 row 21 keeps a tail): only the skin's temperature is tracked, so the
interior is unmodelled and heat conducting past the front is not accounted anywhere. The resolved
computation is a real temperature PROFILE through the body; the moving-boundary treatment of a receding
surface is its refinement.

**Not measured:** peak frame rate fell from ~200 to ~37 fps at 24k drawn marks. I have not measured where
that time goes and, per this project's own rule, will not guess — `/gpu-perf` before any claim.

## 2026-07-24 (evening) — press the button, watch it burn

**What.** The swarm is visible. Terra holds a `flight::Flight`, a `PlanetAir` environment and a
"Meteor swarm" button; `render::MatterField` draws whatever the engine is holding. RIG-VERIFIED on the
5060 Ti: from 700 km on the night side, one press produces a bright glowing entry streak with the leading
fragment ahead of it.

**Measured through one press:** 600 fragments of a 4,121 kg iron parent, released at 500 km — above the
~296–354 km the engine's OWN `air_reaches` derives, so nothing of the atmosphere is skipped — descending
508 → 0 km, braking 17.0 → 0.4 km/s, reaching 3,016 K, **396.8 kg ablated** into ~3,900 resolved vapour
parcels, arrivals logged with real energies (8.3e9 J down to 6.5e6 J). Correctly INVISIBLE on the day
side: a meteor is not visible at noon.

**The size rule in `matter.wgsl` is a sampling statement, not a look.** A trail parcel seen from orbit is
genuinely sub-pixel — metre-scale, 400 km away — and a raster cannot draw less than a pixel. Real meteors
are visible from the ISS because they are BRIGHT, not because they are large. So the billboard half-size
is `max(true projected size, one pixel)`: above a pixel it is the physics, below it the mark is a point
sample of something really there, and the floor disappears as the camera closes in.

**Two defects Robin's questions found, both real:**

1. *"Does the trail dissipate as it cools?"* It cooled — hottest parcel 1300 → 288 K over ~20 s — and then
   **3,734 parcels sat at 288.00 K forever**, holding 9.1 kg permanently aloft and costing a draw every
   frame. `merged_into_air` asked whether a radiatively-cooling parcel had REACHED ambient, which it never
   can: `p_rad ∝ T⁴ − T_amb⁴` vanishes as it approaches. A parcel is now air once it has radiated all but
   a hundredth of the heat it was SHED with — relative to its own history, so it scales with any starting
   temperature and any world's ambient. Robin's framing, recorded at the call site: *"calculus teaches us
   that some limits may never be reached, but we can get 'close enough' as to dismiss the difference
   without fear of fudge."* The distinction from a dial is that the tolerance is relative to the quantity
   being resolved, so it converges with it and states its own error. **Re-measured: parcels peak at 2,659
   and go to 0, all 396.6 kg booked into the air.**
2. *"Be certain the particles eventually reach the ground/merge so they can safely be forgotten."*
   `an_entry_finishes_and_nothing_is_left_aloft` runs a swarm to completion and asserts the books close and
   then EMPTY — every fragment arrived or was consumed, every parcel became air, mass at the end equals
   mass launched. With guards against a vacuous pass, since an empty sim also has nothing aloft. Measured:
   40 fragments, 33 s, all 40 arrived, 4,115.3 kg landed + 5.4 kg taken by the air = 4,120.7 of 4,121.

**Also:** trail resolution is bounded (`Flight::set_trail_budget`) — past the budget shed mass is booked
rather than resolved. Same mass; the budget is the instance capacity, a real hardware bound, and the
choice is which representation to spend it on (Law IV). And `Flight` now integrates drag in CLOSED FORM
rather than sampling it: an entry decelerates at hundreds of g, and `v += a·dt` at that stiffness
overshoots — the same failure the vapour parcels hit.

**An honest note on the swarm's initial conditions.** They are chosen inside the regime the entry model is
honest in, and the code says so. A first pass with a 2 m parent flew a completely correct entry in which
NOTHING glowed and NOTHING ablated, because every fragment was metre-scale and `atmospheric_step` heats a
body at its bulk heat capacity (docs/46 row 21 — real ablation is a surface process). Choosing a parent
whose pieces are centimetres is staying inside the model rather than papering over it; fixing row 21 is
what would let a metre-class body glow.

**Known, not fixed:** the long dark-flight tail. After the fireball the surviving fragments fall the last
tens of km at 80–340 m/s terminal velocity, so ~590 of 600 are still aloft a minute after the flash. That
is real meteorite dark flight, and the native test proves it terminates — but it means the HUD's "in
flight" count stays high long after there is anything to see.

## 2026-07-24 (later) — "we shouldn't have to wire it into Terra"

**What.** Robin read the plan's last line — *"next: the Terra half, give Terra a body list and a step
loop"* — and rejected the shape: *"We shouldn't have to wire it into Terra. We should instead set it up as
a natural operation of the engine receiving these materializations and rendering them naturally… (unless
you mean wiring in the mass/trajectory introduction with button press part)."*

That correction is the entry. A body list bolted onto a scene is a scene feature wearing an engine's
clothes, and it is the exact failure docs/59 opens by naming (*if it only works in Terra, the design has
failed*). The sanctioned scene-side part is the button that introduces a mass and a trajectory, because
that is an INITIAL CONDITION — which is what a scene is for.

- **`flight::Flight`** — matter in flight, as an engine operation. `introduce` is the one door;
  `introduce_swarm` is `damage::disrupt` fed into it, so docs/59's swarm is a composition of two things
  the engine already had rather than a feature. `step` runs the air, the trail, gravity, and arrival at
  hard matter.
- **`flight::FlightEnvironment`** — the entire seam between one flight law and every world it runs in:
  gravity here, air here, has the path met hard matter. A ground patch answers from a heightfield, a
  planet from its own layered mass and its `AirShell`.
- **`Drawn`** + `GpuParticle::of_matter` + `Simulation::drawn()` — the engine says what its matter looks
  like, once, from real albedo and real temperature.

**Why it was mostly a MOVE, not an invention.** The flight operation already existed and was already
correct — `Simulation::fly_meteors`. It was generic in everything except its ADDRESS: it lived inside a
96 m voxel ground patch, in `f32`, so nothing at planetary scale could reach it. Nothing about drag,
aeroheating or ablation was ever ground-specific; only *where the air is* and *where the ground is*. The
same was true of the render: `ground_scene` built `GpuParticle`s by hand twice (grains, then bodies in
flight), so the scene was reading albedo and calling the incandescence law itself — and a third copy would
have been needed for the trail, a fourth for a swarm. Each copy is a place a scene can quietly disagree
with the physics about what is real, which inverts Law VI.

**The part that would have made it worse.** Extracting `Flight` while leaving `fly_meteors` in place would
have been the Law II violation it exists to close. The ground scene now DELEGATES, and
`simulation::Meteor` is a type alias of `flight::FlyingBody` — it was a second `f32` struct for one
concept, owned by a scene.

**Verified.** 365/365 native (4 new), wasm target builds. MEASURED — ablation is surface-to-volume, so the
air takes a larger share of a smaller fragment. Eight iron fragments entering at 15 km/s from 200 km, as
the parent grows:

| parent radius | fragment radii | ablated |
|---|---|---|
| 1 cm | 3.3–7.5 mm | 94.7% |
| 2 cm | 6.6–15 mm | 69.9% |
| 5 cm | 1.6–3.8 cm | 27.5% |
| 10 cm | 3.3–7.5 cm | 6.0% |
| 30 cm – 3 m | 9.9 cm – 2.3 m | 0 |

That is why shooting stars are small and iron meteorites reach the ground, and nothing in the engine says
so — it falls out of one heating law meeting one mass distribution. The test asserts the TREND, not a
recorded number. (The zeroes are also docs/46 row 21 biting: `atmospheric_step` heats a body's whole mass
at bulk heat capacity, understating ablation past the thermal skin depth. Trend is physics; the exact
cut-off is the flagged limit.)

**A wrong premise of mine, caught by the measurement.** I first wrote the swarm test asserting "the small
fragments burn up" while sizing a parent whose every fragment was metre-scale — so nothing ablated and the
test failed. The code was right and the test's premise was wrong. Measuring the size sweep produced a
better assertion than the one I had intended.

**Also removed:** the ground scene retired a meteor below `1.0e-3` kg, a threshold tracing to nothing
(Law V). Ablation takes `min(net/L_v·dt, mass)` per step, so a body being consumed reaches exactly zero on
its own — and a gram of iron at 15 km/s still carries ~110 kJ. And a real buffer hole: the scene capped
GRAINS at its instance capacity and then pushed meteors on top, writing past the end of the buffer;
`drawn()` emits matter in flight first, so capping one list cannot overrun it.

**Not done / next.** Nothing visual is claimed and no rig was run. To SEE a swarm, a scene has to present
`Flight::drawn()` from orbit — the sanctioned scene-side part, and small — but Terra draws globes and
meshes and has no instanced particle path at all. That is docs/50's remaining render-path increment, and
it is now the only thing between the engine holding a meteor swarm and anyone watching one.

## 2026-07-24 — the flagship's engine half: entry is a collision, a swarm is a disrupted body

**What.** docs/59 Stage A, engine side. Three capabilities, each natively tested, plus a DRY pass Robin
asked for that turned into a real (small) correctness fix.

- **The atmosphere is a thing you collide with.** The engine could detect body-meets-body, and could fly a
  body through air (`atmospheric_step`); what it could not do was NOTICE that a body was in the air — that
  was left to whichever scene cared, which is how entry stays a scene's feature instead of the engine's
  capability. `interaction::detect_atmospheric` is the fluid branch alongside the solid one in the same
  module. Reported as a STATE, not an event, because an impact happens at an instant and flight through air
  happens along a path; the two compose the way the physics does.
- **Where an atmosphere ends, derived.** It doesn't: `ρ(h) = ρ₀·e^(−h/H)` is positive everywhere, and the
  Kármán line is a fact about wings. `air_reaches` asks where *including* the air stops being able to change
  the answer — `|Δv| = a_drag·dt` below `ε·|v|` is not neglecting the air, it is adding it and having no
  effect. It is a property of the BODY, not the planet.
- **A swarm is a disintegrated asteroid, not N placed meteors.** `damage::disrupt`: Dohnanyi (1969) mass
  shares (`m_i ∝ i^(−6/5)`, the exponent a ratio rather than a fitted decimal) normalised to the parent's
  mass exactly, separation at the parent's own escape speed (the just-unbound condition at the disruption
  threshold `ground_effect` already tests), spread = v·t since breakup, and golden-angle isotropy — no seed,
  identical every run, which is what lets a scene fly back to the same crater it watched form.
- **The entry trail.** `VaporParcel`/`Trail`. `ablated_mass` used to be subtracted from the body and
  dropped; now it is shed as hot vapour, which is both the conservation fix and the thing you actually see
  behind a meteor. Parcel SIZE is emergent (vapour expands to the density of the air around it), colour is
  `blackbody_srgb` of its real temperature. Robin's steer — *"rendering/tracking should be decided based on
  the scale it is being viewed at"* — is in the type: `Trail` holds the same mass as resolved parcels for a
  near camera or as a booked total for one watched from orbit, and `Trail::mass()` is the same number both
  ways. Wired into the Ground scene's meteor, so it has a live consumer.

**Why.** Robin's acceptance criterion for the flagship is *"add the meteor-swarm button to any scene that
uses the engine properly and it just works"* — Law II stated as a test. That is only true if entry,
disruption and the trail are engine capabilities on the generic body system, so Stage A builds them there
first and Terra hosts them second.

**Verified.** 361/361 native (10 new), wasm target builds. Measured, not assumed:

- The derived air edge on Earth's own emergent air (ρ₀=1.207, H=8367 m, 20 km/s, 1 s step): **296 km** for
  a 1 m iron sphere, **354 km** for one 1000× lighter, **291 km** at half the step. Body- and
  step-dependent, and nowhere near any declared altitude.
- Mass conservation flown end to end: a 1 cm iron grain entering at 20 km/s from 120 km, body + trail
  checked against the entry mass **every step**, to 1e-12.
- Disruption momentum closes to **1e-16** relative.

**Three things the tests caught rather than confirmed** (all now docs/46 rows or flagged in place):

1. **Isotropic directions with UNEQUAL masses do not conserve momentum.** The heaviest fragment would have
   dragged the parent's centre of mass off its declared trajectory — quietly, since mass and energy still
   balanced. Disruption is internal and cannot move the centre of mass, so velocities are taken in the
   centre-of-momentum frame. The speed spread that falls out (heaviest piece slowest) is now a CONSEQUENCE
   of conservation rather than an assumed size–speed law.
2. **Explicit quadratic drag on a vapour parcel overshoots.** A parcel as thin as the air around it has an
   enormous area per kilogram; a step long enough to stop it reverses it and then accelerates it away,
   which read as vapour speeding up. Quadratic drag integrates in closed form, so the step is solved rather
   than sampled and decays monotonically for any dt.
3. **`atmospheric_step` treats an ablating body as isothermal.** A 0.5 m iron body at 20 km/s barely warms
   through the whole descent; a 1 cm grain reaches iron's boiling point in milliseconds. The model gets the
   observable right (small meteoroids burn up, iron meteorites land) but that is not evidence the mechanism
   is right — it holds only while the body is thinner than its thermal skin depth (~1.5 cm for iron over
   ten seconds). Real ablation is a surface process (docs/46 row 21).

**The DRY pass (Robin: "it's getting pretty long in spots").** Measuring first said the length is not the
interesting part — the duplication is. **G was written out in five modules and two shaders**;
Stefan–Boltzmann in three, one as a truncated `5.670e-8`, so a cooling moonlet and an ablating meteor
radiated by literally different constants. Nothing had drifted far enough to fail a test, which is exactly
why counting is the check and reading is not. `orbit::G` and a new `blackbody::SIGMA` are now the only
definitions. **The guard already existed and I nearly wrote a second one**: `laws::SINGLE_SOURCE` counts a
constant's homes across every Rust source *and* every shader — registering σ took one line, and it
immediately caught the two WGSL copies of G a Rust-only search had missed. Those are legitimate (WGSL
cannot read a Rust constant) so they are PINNED, the `EARTH_RADIUS_M` treatment: a second copy is honest
only when something fails if the two disagree.

**Not done / next.** The Terra half of Stage A: Terra has no body list and no step loop today, so the
swarm has nowhere to fly yet. No rig verification in this entry — nothing visual was claimed, and the
physics is verified natively (docs/46 rows 20–22 record what is knowingly deferred).

## 2026-07-23 (later) — de-resolution WIRED: merge, promote, crater, and the crash the rig caught

**What.** The entry below landed the de-resolution *operator* and honestly called the scene wiring "not
done." It is done now, plus the two tiers Robin steered toward and a render fix. (That earlier "NOT done /
the collapse never ran / bhtree has no potential traversal" text is superseded by this entry — left in place
as the record of what was true at the time.)

- **De-resolution as a CONTACT LAW, not a global search** (Robin's redirect: *"if two sticky particles
  collide and now share a common position/vector, we merge them… done in the shader, nearly free"*). Three
  race-free kernels in `sph_step.wgsl` (`cs_merge_pick`/`apply`/`retire`, disjoint write sets, absorber by a
  total order). TWO gates, both required: REDUNDANT (`r < h/2` ∧ `|Δv| < c_s`, the material's own numbers)
  and NECESSARY (`n > merge_budget`, and the budget is MEASURED — the substep throttle bottomed out AND the
  frame still over budget). Conservation exact; the destroyed relative KE becomes heat in `u`. **Materials
  never blend** (same-`mat` only), which RETIRES the mixture-EOS IOU and preserves composition: a body
  converges to one particle *per material*, each at its own radius (the dense one sank).
- **Promotion to a layered body** (Robin: *"a planet is a promoted particle with more properties/analysis";
  "as above, so below"*). `promote_settled_bodies`: a settled self-bound clump above `rounding_mass`
  (self-gravity beats material strength — the rock/body boundary, reproduces the ~300 km potato radius)
  leaves the particle set and becomes a layered `orbit::Body` whose matter is SAMPLED from its own particles
  (`accretion::sample_layers` — measures the differentiation the sim made, does not declare it). **`ext_mass`
  is now load-bearing** — the promoted body gravitates on the survivors through it (Law IV). Gated on the
  same quiescence as the merge: promoting or merging mid-shock was MEASURED to eat the disk (birth
  0.34/0.31/0.25 M☾ → 0.00 until the gate was fixed; docs/44 §6 said "demote on quiescence" and I'd quoted
  it before applying it).
- **The gate Robin caught:** the first promotion gate was `MIN_MEMBERS = 24` — unsatisfiable *because*
  merging works (it destroys the count). Same shape as the O(k²) binding sum that exploded when clumps
  united: a criterion written in a quantity the mechanism itself consumes. Replaced by the mass gate above.
- **`bhtree` potential traversal** (`potentials`/`self_potential_energy`), so the binding energy is
  O(N log N). Corrects a prior measurement that had blamed `self_pe` for cost that was actually the FoF pair
  loop (the synthetic blob's linking length exceeded its own radius).
- **The crater renders** (docs/46 row 18 closed). `globe.wgsl::crater_sink` sinks the surface into a
  paraboloid; `accretion::crater_bowl` sizes depth AND radius from the measured excavated volume plus the
  sourced simple-crater shape d≈0.4r. The old radius came from a `0.72` dial while only depth was measured —
  a flat saucer (d/r≈0.06) that reached the shader correctly and rendered invisible. After: d/r=0.40.
- **A crash the rig caught, no test would have:** `cs_merge_retire` set a mass to 0 but left the particle
  LIVE, so it kept integrating, its density → 0, `p/ρ²` → NaN, and the NaN reached the disk-stat sort. Fixed
  by making retired particles inert in every kernel AND filtering non-finite/mass-0 in `DiskView` with
  `total_cmp`. "Harmless because massless" was true for its effect on others, false for its own evolution.

**Verified.** Native 347/347. `tools/sph-verify` on the RTX 2070: force/KDK/spherical+planar bulk +
`ext-mass` (a de-resolved body gravitates: vx 39.858 vs analytic 39.857 m/s) + `merge` (128→20, mass/mom/
energy conserved) + `merge-materials` (128/128 survive when every pair is iron+basalt) + `merge-gate`
(budget 0 merges nothing) + `merge-finite` (no NaN survives). Rig: promotion observed
(`promote: 3 particles → 6.794e21 kg, 1 layer peridotite` = 0.093 M☾, matching the disk stat), scene runs
clean, crater geometry rig-measured d/r=0.40. `a_crater_bowl_keeps_its_shape_at_every_size` independently
re-derives ½πr²d and caught a factor-of-2 volume error the rig could not. Deployed to integrity.bothead.net.
**Not verified:** a fully unambiguous daylit face-on crater screenshot (needs a camera-aim hook the scene
doesn't expose); promote-then-survive in one continuous run since the crash fix.

## 2026-07-23 — de-resolution: a united clump can become one gravitating body (docs/44)

**What.** Robin: *"an essential efficiency/optimization component MUST be resolving debris (clumping) when
particles that are sticky (gravity/liquid/etc) unify in trajectory... we have a large, cohesive disk of magma
that has united; this should become a single body"*, and *"stragglers ... should be absorbed too. A meteor
that impacts the moon becomes part of the moon unless it hits escape velocity on the rebound."*

The operator already existed. `accretion::accrete` found self-bound clumps (`internal_ke + self_pe < 0` plus
a Roche gate) and returned the promoted bodies AND the particle indices they consumed — fully tested, with
**zero production call sites**. The four live callers of `find_clumps` only MEASURE. Nothing has ever removed
an SPH particle at runtime. So the criterion existed and the collapse never ran (docs/48's pattern again).

Four things had to be true before it could be wired, and each is now:

1. **It stopped losing physics.** `Body` was `{pos, vel, mass, rho, radius}` — no angular momentum. The module
   doc claimed the spin was *"folded into the body ... recoverable from the members"*, but there was no field
   for it and the members are exactly what a de-resolution pass deletes. Any caller consuming
   `Accreted::consumed` would have destroyed the spin of every clump it promoted **while mass, momentum and
   COM still balanced exactly**. A rotating disk is where that term dominates. Added `ang_mom` and
   `thermal_ke`/`thermal_j` — the heat deliberately NOT the whole internal KE, since a coherent rotator keeps
   that as spin and counting it twice injects energy through a change of representation (docs/44 §7). Split
   via `E_rot = ½ω·L`, `ω = I⁻¹L`. FLAGGED: <3 members or collinear geometry has no invertible inertia
   tensor and over-reports heat; the spin is carried regardless.
2. **A de-resolved body still gravitates.** `sph_step.wgsl` sums gravity over the particle set plus one
   optional bulk, so a clump leaving that set would stop acting on the survivors — changing what is TRUE, not
   just how it is stored (Law IV). Added `@binding(8) ext_mass` + `Params.n_ext` (which took the `_p1` padding
   slot, so all offsets and the 80-byte size are unchanged) and `GpuSph::set_external_masses`.
3. **Stragglers rejoin.** `Body::absorbs` gates on the body's OWN escape speed at the contact point
   (`r <= radius`, `|v_rel| < sqrt(2GM/r)`) — no dial. `Body::absorb` conserves mass, momentum, COM and total
   angular momentum, spins the body up on an off-centre strike, and keeps the energy budget closed.
4. **One answer to "what is the disk?"** Four routines each carried the same preamble and had drifted; see
   docs/46 row 17. `gpu_sph::DiskView` is now the single definition.

**Also fixed, found while measuring.** `gpu_disk_stats_json()` ran EVERY FRAME with no cache and no `?nostats`
gate, to render one HUD line — friends-of-friends plus an O(k²) binding sum. The CPU disk stats thirty lines
above already carried a *"throttled to ~1 Hz"* comment for exactly this reason. Now throttled the same way.

**Verified.** Native **341/341** (was 337). `tools/sph-verify` on the RTX 2070 — force RMS 1.85e-6, KDK
integration, spherical + planar bulk, and a NEW positive external-mass check, since every other harness runs
`n_ext = 0` and would pass with the channel inert or wrong-signed:

```
ext-mass: PASS -> vx=39.858 m/s vs analytic 39.857 m/s (rel 1.52e-5), a_ref=3.986 m/s^2
```

Conservation and drift tests were **mutation-checked**: zeroing `ang_mom` fails by the full spin share;
`thermal_j = internal_ke` fails the rigid-rotator assertion; restoring the pre-fix disk filter fails with
"the hyperbolic particle must be counted as escaped, got 0".

Measured cost of the binding sum (`find_clumps_cost_against_clump_size`, `--ignored`, release):

```
  members     ms
      500    1.9
    1,000   11.2
    2,000   33.0
    4,000  121.5
    8,000  492.1
```

**NOT done, and the blocker is honest.** The scene wiring is not in. That measurement makes a naive per-frame
pass unusable *exactly* on the united disk it exists to catch — one call over a 20k clump extrapolates to
seconds, i.e. the operator gets catastrophically expensive at the moment it starts succeeding. The
prerequisite is an O(N log N) binding energy: `bhtree::BarnesHut` exists and is verified but has only
`accelerations`, no POTENTIAL traversal. Adding one (same tree, same opening criterion, sum `Gm/r`), pinned
to the exact O(N²) sum, is the next step — then the pass hooks in where `sph_snapshot` lands.

## 2026-07-23: the descent corridor stops running out of pixels

**What.** The space band's side of the close-range render, reused rather than re-invented.
The one surface sampler now has a name (`terra::globe_mesh::SurfaceSampler`): the globe mesh
builder, Terra's ground cap and the new corridor cap all read a body's surface through it, so
no two renders of one body can disagree about its continents or elevation. On the Ground Zero
descent, once the arc drops below the derived hand-off altitude the scene fills the same
`terra::ground_cap` patch Terra flies over, in the crust frame around the sub-camera point,
and draws it through the globe's own conventions (same view projection, the spin as the model
rotation, the eye re-added as an f64-built translation), cross-faded over the coarse globe and
replacing it outright once the cap covers the view past the horizon. The hand-off is derived
from the raster's own resolution: `ground_cap::handoff_alt_m(texel, budget)` is
`site::view_resolution_distance` asked about one texel, the altitude where one texel of the
finest shipped raster subtends the same angular budget the site materialization threshold
uses, about 19,500 km for the shipped 2048x1024 Earth rasters; `Raster::texel_arc_m` supplies
the texel. The fade spans the first octave of deficit; Terra reads the identical derivation and
its declared 40 km / 15 km cap constants are retired; the cap's depth lift became
altitude-proportional (the old 20 m ceiling lost the depth fight once the fade band's top
became thousands of kilometres instead of a declared 40 km).

**Why.** A user test on Ground Zero: the globe blurs when zoomed in. The space band textured
its Earth from planetary-scale rasters through a 192-per-face-edge mesh, so on the descent the
data was exhausted and the renderer stretched it, and the out-and-back arc flies straight
through that corridor. Terra already owned the cure (the cube-sphere cap and the material
relief), and one body definition serves both scenes, so the fix is reuse behind one derived
hand-off, not a second close-range path. Where even the finest raster is out of texels (the
known missing finer LOD tier), those texels at their true size plus the material relief are
the honest floor; blur is not.

**Verified.** 405 native tests green (400 baseline plus 5: the hand-off derivation pinned to
the shipped raster size and to `view_resolution_distance` itself, the finest-raster rule, the
one-octave fade, the covers-past-the-horizon skip rule, the proportional lift against the
depth buffer's own resolution, the raster texel arc). wasm32 check clean. Headed on the Mac
(mac_shot pattern, port 7299, `web/rig/mac_corridor.mjs` on /groundzero.html): the arc flown
by its own control and screenshotted at celestial, ~5,000 km, ~500 km, ~50 km and the 1.4 km
low hold; the rig pixel-checks the mid-corridor frame and re-flies the out-and-back when the
site arrives on the night side (the crust phase at arrival is real physics under the
compressed approach, and a black night ground verifies nothing), first daylit arrival on
flight two. Viewed: at 5,000 km the coastlines resolve where the old render smeared vertex
colours; at 500 km and 50 km the biome texels hold their true size and the coast stays a
line; at the low hold the ground is a clean lit surface under the site's fine matter. Zero
console errors. Terra re-verified across its seven descent stations (`mac_descent.mjs`, the
raster's texel blocks now honestly visible from orbit where the old globe smoothed them) and
all six scenes smoke clean. Known seams, stated: the cap's outer boundary is visible mid-band
as a fine-to-coarse data edge (and near the terminator the lift advances daybreak slightly at
high altitude), and at the 1.4 km floor the 8 m relief tiles span ~6 px, so mip filtering
averages the mottle nearly flat: the finer LOD tier remains the missing rung.

## 2026-07-23: the drop arms for the launch window, not the instant

**What.** The launch-window intercept (`crates/engine/src/intercept.rs`), pure and natively
tested: given the current N-body state, the body to drop, the planet's declared spin (day
length and accumulated spin angle) and the declared site (lat/lon), it computes where a
from-rest fall released at time t hits by INTEGRATING THE SCENE'S OWN LAW (`orbit::verlet_step`
plus the swept first-contact forecast - no analytic stand-in; the Sun's tide and the planet's
recoil bend a multi-day "radial" fall by real degrees), then solves for the next release time
at which the site rotates under the impact azimuth, returning the window, time-to-window, fall
duration, the solver residual and the polar plane offset timing cannot change. Wiring: on a
world that declares a ground site, `drop_moon` ARMS this window instead of releasing; the
countdown runs in sim seconds inside `step_substep` and the release fires itself at the nearest
substep boundary (a stated ±dt/2 quantization); the HUD carries "DROP ARMED · window in T-...".
A world without a site keeps the instant drop, and Reset disarms.

**Why.** Drop Moon rarely hit the declared site: the ball rides a rotating Earth and the fall
takes days of sim time, so ground zero is a moving target - correct physics, bad demo control.
The honest fix is the one any real mission uses: never move the ball, never bend the
trajectory; choose the release time from data the engine already holds deterministically.

**Verified.** 394 native tests green (391 baseline plus 3). The closing test replays the WIRED
release at the Ground Zero world's own 118,000x substep (~123 s), countdown quantization
included: the contact lands 0.086 degrees of spin-axis azimuth from the site (about 10 km of
arc at the site ring; solver's own residual 0.0003 degrees), against a stated 1-degree
tolerance; the 44.1-degree polar offset between the fall's equatorial impact ring and the
declared 45N site is geometry the release time cannot move, reported by the solver rather than
bent away. A site carried half a turn about the spin axis waits 44,538 s longer - about half a
sidereal day - for its window, with fall durations equal to 33 s over 4.836 days and the
inertial impact azimuth drifting only 6.1 degrees (the Moon's own orbital drift over the wait):
a later window, never a bent trajectory. Headed on the Mac (mac_shot pattern, port 7099,
`web/rig/groundzero_window.mjs` on /groundzero.html): Drop arms with the HUD reading "DROP
ARMED · window in T-23h 31m sim · then a 4d 18h fall to ground zero" while Luna stays on its
orbit; at the world's own rate the release fires itself (countdown clears with no second
input), the fall runs to contact, and the materialized site's event window books the arrival
(boundary: 477 guards, 0 uncovered, arrived KE +3.67e12 J / IE +7.17e14 J, peak 304 m/s) with
the pi-gate line alongside; zero console errors; screenshots viewed.

## 2026-07-23: the declared site sits on the fall's own impact ring

**What.** Ground Zero declares its site at lat 45, lon 0 rather than the ground scene's 45N 100W.

**Why.** The whole cast starts in the orbital plane with in-plane velocities, so a from-rest lunar
fall is confined to that plane and its reachable contact points form the great circle the plane cuts
through the crust. Release timing can fix the azimuth but never the polar plane, so a site 44 degrees
off the ring could never be hit, whatever the window solved for.

**Verified.** The derivation is written into the world file's comment, and the solver test asserts
the declared site's plane offset reads ~0.

## 2026-07-23: the out-and-back arc: one camera path from the ball to celestial and back

**What.** The demo choreography's camera path exists: `crate::arc` (pure, natively tested) plus
a drive in the space band and a plainly-labelled control on the Ground Zero page ("Demo arc ·
drives camera + time only"). One press takes the camera from wherever the manual rig stands and
glides it down to the site; the next pulls out to celestial framing; the next descends home:
one continuous parameterization, no cuts, no scene switch, either direction, with the docs/59
trigger materializing and folding the site along the way. The span is derived at both ends:
floor = the site's finest materialized quantum over the docs/49 angular budget (the ball's
one-rung child, 1.37 m, gives ~1.4 km, the deepest framing the current rung honestly serves,
and the tested point at which absolute-f32 rendering is still sub-pixel, so Terra's
camera-relative-eye convention is not yet needed); top = the fold threshold at the scene's own
1.7 whole-orbit margin (now one constant, `arc::WHOLE_ORBIT_MARGIN`, where two literals lived).
The pacing rule is declared-or-derived, never buried: sim compression proportional to camera
distance (holding apparent angular rate constant), anchored to the world's declared
`time.scale` at the top, flooring at real time near the ground; the world file declares the one
new number, `arc.octave_s` (real seconds per octave of scale). Two derived rules make a
spinning, time-compressed crust rideable: crust-anchored quantities (hover direction, look
target, view-up) weight by the reciprocal of the compression, so nothing drifts across the view
faster than Earth's REAL rotation; and the descent aims where the site WILL be, the remaining
crust turn under the geometric glide is closed-form (Ω·τ·(S−1)), a constant of the motion, so
the site rotates into place beneath the camera exactly as it arrives.

**Why.** The demo decision is out-and-back: open at the ball (the scale referent), pull out
while sim time compresses, witness the de-orbit and impact at celestial scale, descend to the
re-cohered site. The trigger was already bidirectional; what was missing was a camera that
spans standing-at-the-site to celestial without a representation break, and a time-compression
law tied to altitude instead of a hand-set dial. The arc touches nothing physical, it is Law
IV/VI kept deliberately: a camera/time driver whose every crossing goes through the site
trigger's own laws.

**Verified.** Native suite 394 passed, 0 failed, 22 ignored (388 baseline + 6 arc tests: span
derivation pinned against `materialize_site`'s own finest particle, pacing endpoints and
proportionality, pose continuity over a compressed spinning crust in both directions, the
lead's conserved quantity, the pan shaping, the declared pacing in the shipped world). wasm32
check clean; vite build clean. Watched headed on the Mac (mac_shot pattern, port 6999,
`web/rig/mac_arc.mjs`): the arc ran station to station with ZERO console errors, celestial
start (site materialized on load, x118,000), mid-descent (x80, aim glided onto the site),
surface hold (camera 2 km from the site, x1 real time, ball + patch cluster centred), pull-out
(x104 mid), celestial hold at 1.62e9 m (site FOLDED: 750 particles, 1.3134e8 kg returned, drift
+0.0e0), and the return re-materialized the same site. With IMPACT=1 the full choreography ran:
Luna dropped from the celestial hold, the GPU impact witnessed at scale (incandescent remnant,
Earth day 23.9 h -> 25.7 h on the HUD), and the descent through the quieted aftermath
re-materialized the site carrying the live field's sampled state ("u sampled from the quiet
field: 3.55e6 J/kg (peak 1012 < v_q 4358 m/s)"), the hand-down path exercised end to end from
the camera's side. Honest remainders: the opening framing shows the ball as a small central
cluster (going closer needs the next rung down, docs/59 item 4), and the ball's own destruction
at the site awaits the mid-event hand-down and site dynamics (docs/59, docs/46 row 18).

## 2026-07-23: the ground-zero demo is a world definition

**What.** The docs/23 demo now exists as data the engine executes, not as a scene struct.
`web/public/worlds/ground-zero/world.json` declares the full cast in one file: the Sun, the shared
Earth and one Luna instance on her real orbit (the mean distance and its circular speed, the same
declared state the Space world carries), plus a `ground` block placing ground zero at a lat/lon
site on that same Earth with the iron ball declared as cohesive matter in the patch. A new page,
`groundzero.html` (scene picker: Ground Zero), points `data-world` at the file and loads it
through the existing space-band host; no engine or host code changed for the page, and no new
schema was needed, because a system world can already carry the `ground` block and `GroundDef`
already declares `planet`, `lat`, `lon` and `bodies`. What sets the page apart from Space is not
the drop, which Space has too; it is the declared iron ball standing at the impact site, the
witness the zoom milestones descend to, and the page says so. Two native tests make the shipped
file executable truth: one runs BOTH halves (the system cast parses through the space band's
schema, is star/planet/moon, and Luna's declared state is a bound orbit at the mean distance; the
ground half builds through `Simulation::from_json`, derives its column from the shared Earth at
the site, and builds the ball as a bonded iron lattice), the other pins the ground-zero round trip
(lat/lon to a point on the orbital Earth's surface and back, through the one `geo` conversion,
landing on land).

**Why.** The demo must ship as a definition the space band machinery executes; adding a third
`#[wasm_bindgen]` scene is forbidden, and worlds-as-data (docs/43, docs/51-55) is the seam that
makes a scene a file. What runs today is what the milestones allow: the orbital system with the
brake and drop controls, ending in the same emergent GPU impact. The ball exists in the definition
but nothing renders it at orbital scale; the zoom milestones (docs/59 order of work 2 to 4:
descent camera and trigger, conserved hand-down, re-coherence) are the consumers of the ground
block, and the world file's own comment names that IOU rather than hiding it.

**Verified.** Full native suite green including the two new tests; wasm32 check clean; the laws
scans pass over the new world file (no emergent quantity declared, no defined-body override).
Watched headed on the Mac (mac_shot pattern, port 6399): the Ground Zero page renders the space
band with Earth crescent-lit in the Sun's light, Luna in frame, the Earth and Luna focus buttons,
and brake/drop controls. Driving the Drop control headed: the HUD switches to "impact trajectory ·
closest approach 1 km (inside contact at 9,551 km)", the GPU impact assembly takes over, and the
view settles on the incandescent post-impact Earth. All of it ran from the definition.

## 2026-07-23: the site enters dynamics, and the drop breaks its ball

**What.** The remaining core of the zoom hand-down (docs/59): the materialized fine site now
STEPS, and a landed drop's arriving energy moves and breaks its matter. Two pieces. (1) The
release gate resolved on real relief with a DERIVED bound, not a declared one. Measured first:
the site's relax plateau is a true fixed point (oscillates around 4.9e-2 all the way to the
5000-iteration cap, never converging toward the strict 5e-3), and the cause is the release
criterion's own reference - the children's sum smooths at the child scale h_c while the target
is the coarse field read at the interface scale (h_c + h_p)/2, and near rough relief the coarse
field genuinely answers the density question differently at those two scales. The rung now
judges a PROVEN stall against that disagreement, measured from the coarse field at the
children's own sites with the same denominator as the release error (`refine::scale_mismatch`),
and releases with the residual and the derived bound both stated (`RelaxReport::release_bound`).
Measured: the relief lattice's 5.1e-2 plateau under its 1.2e-1 mismatch; the site's 4.8e-2 under
9.1e-2; a uniform interior reads ~4.5e-2 across the same interval (the coarse lattice's own
discreteness at the finer read scale - stated as the floor it is); the strict bound remains the
criterion for every converging run and an unguarded truncation still refuses. (2) The released
parcels are cohesive matter through the ground scene's own machinery: a new
`CohesiveBody::from_parcels` carries the ONE builder law (bond k = E*L capped, break strain,
restitution-derived damping, catalogue specific heat, planet gravity) for both the ground
scene's lattice balls and the site's split children; `site::SiteDynamics` steps them under
bonds, gravity and the one terrain-contact law (the ball rests on the declared surface exactly
as the ground scene's ball rests on its voxels; buried patch matter meets the bowl wall through
the same non-injecting resolve in the wall's local frame). During a live event the guard band's
booked step-to-step delta - the coarse field's per-kilogram statement of what arrived at this
sub-resolution ground - is delivered through the one door (`Aggregate::deposit_impact`,
entering from the up-shock side); fate is `damage::classify`'s per parcel, the ball's verdict
is the ground scene's word, the fold stays gated by the docs/61 gauge, and the release gate
still keeps an UNRELEASED patch out of dynamics.

**Why.** The demo's final beat is the declared ball shattering from the drop seen from space -
one event read from one file at two scales. Everything ahead of it existed (trigger, guards,
window, gate); what was missing was the site actually being matter that moves. Law II bound the
build: no new laws, the ground scene's contact, deposition, fracture, settle and re-cohere
machinery reused with the site as its second consumer.

**Verified.** Red first at the seams: the relief test was rewritten to expect release under the
derived bound and failed against the refusing rung; the dynamics tests were written against the
missing API and driven green. Native: the quiescent site's ball falls, rests and stays intact
with every parcel classifying Intact, the gauge settles and the fold returns the declared mass;
the demo beat at the seam - a moon-magnitude boundary pulse (u jump 4.8e6 J/kg, bulk 2.2 km/s)
- shatters the ball by classify alone (fate 0 solid / 42 fractured / 171 molten / 60 vapor,
delivered J equal to the booked arrival to 1e-9, mass conserved, mid-event fold refused); an
artificially unreleased site is refused dynamics with the rung's own residual quoted. Full
suite 403 green, wasm32 check clean at the baseline warning count. Watched headed on the Mac
(mac_shot pattern, port 7199, `web/rig/mac_site_dynamics.mjs` on the Ground Zero page,
screenshots viewed): at load the HUD reads the audit plus "released 4.8e-2 (bound 9.1e-2, the
field's own scale mismatch at the stall)" and "ball INTACT (72/72 bonds) · parcels 273 solid";
Drop arms for the launch window and fires itself; the boundary window opens as the shock
reaches the site (measured on the HUD: arrived IE +1.67e15 J at the band, peak boundary 6019
m/s); the verdict flips to "ball SHATTERED (0/72 bonds)" with the fate mix running to
"272 vapor" and "boundary delivered 1.12e14 J over 18 steps" - the moon-drop's energy density
vaporizes the whole patch, which is the honest outcome, reported as such. Zero console errors
across the arc.

## 2026-07-23: the event reaches ground zero as boundary state, booked

**What.** The hand-down design's three decisions, implemented. (1) The Ground Zero world's
declared site pre-resolves in `load_site_world`, before any event exists, and the HUD carries
the audit from the first frame; the descent trigger stays as the general path, and the only one
when a mid-event load refuses with its measured speeds. (2) During a live impact the guard band
re-samples the coarse SPH field once per coarse readback (`site::resample_guards`): each guard's
velocity, specific internal energy and density become the field's own Shepard interpolation at
the guard's position, positions and masses untouched, so the guards ARE the coarse field at the
boundary and no parcel is counted twice; `site::EventWindow` books the boundary state at open,
latest and peak, drift against an independent audit bounded at 1e-6, and the docs/61 gauge now
sees the boundary's real speeds, so a hot site refuses to fold. The sampled state inheriting the
coarse field's collapsed EOS set is the stated IOU on the window's own line. (3) Cold mid-event
materialization keeps refusing with measured speeds, unchanged. The pi-scaling gate gained its
end-to-end consumer: the prediction freezes from the measured contact state (barycentric closing
speed, measured impactor mass), `refine::measure_crater_rim` reads the rim off the field in
rings of the field's own quantum, and the verdict, or the stated refusal when the quantum cannot
carry one, renders next to the window with the coefficient vintage named.

**Why.** The trigger half landed a site that could exist; the event's energy never reached it,
and the literature's rule (no refinement inside a shock) means the energy must arrive as
boundary state at a site that already exists, not as a mid-shock hand-off. The window's booked
drift bound is the conservation statement the LOD bridge owed, and the crater cross-check is the
validation gate docs/59 prescribed instead of eyeballing.

**Verified.** 391 native tests green (388 baseline plus 3). The window book: a quiet-hot-quiet
pulse across the shipped site's 477-guard boundary books arrived KE/IE equal to an independent
f64 audit within 1e-6, returns to zero when the pulse leaves, guards carry exactly the uniform
field's state, fine children bitwise untouched, mass single-owner at 1.313357e8 kg through the
whole window. The rim measurement: a synthetic bowl of angular radius 0.3 rad measures its rim
within one quantum of the true 30 m and passes the gate; a one-ring dimple refuses sub-quantum
with the quantum named; an intact shell refuses as no-depression; the demo drop's prediction
(Luna into basalt crust at the mutual escape speed, hand-computed rim 1.41e6 m) sits in the
plain factor-of-two regime. Headed on the Mac (mac_shot pattern, port 6899,
`web/rig/mac_event_handdown.mjs` on the Ground Zero page): at load the HUD reads the full audit
line plus "pre-resolved at load, before any event" with the camera at 646,242 km inside the
952,220 km threshold; after Drop Moon the event window opens on the HUD with all 477 guards
covered and books the boundary energy arriving, from an early arrived KE -6.6e10 J / IE +6.8e10 J
at peak 304 m/s through to KE +7.7e12 J / IE +8.0e14 J at peak 1,088 m/s after 337 coarse steps,
the EOS-collapse IOU on the same line; the pi gate renders live and lands on its honest verdict,
"rim 5838 km measured at the 1168 km quantum vs 1396 km predicted from the 9.7 km/s contact:
SANITY PASS (ratio 4.18; the crater rivals the body, so only the order-of-magnitude bound is
honest)" (the first run of the rig measured this exact case as a plain 2x FAIL, which is what
widened the gate's degrade condition to the measured rim; the quantum itself balloons to 1,168 km
mid-event because decompressed ejecta are the widest thing one particle answers for - measured,
stated, not smoothed); zero console errors across the run; screenshots in the rig's output
directory, viewed.

## 2026-07-23: the camera crossing its view threshold materializes ground zero

**What.** The first camera-driven resolution-by-necessity trigger (docs/59 order-of-work item 2's
trigger half, plus the entry point of item 3), wired into the space band and deliberately
mirroring the moon-drop's resolution-distance idiom: one derived distance, one crossing check per
frame, one materialization pattern. `crate::site` derives the view-necessity threshold as the
distance where one coarse SPH particle's matter share, `s = (m/rho)^(1/3)`, subtends the docs/49
angular budget (the one declared fidelity dial): `d* = s / theta`, the inversion of the
camera-granularity law. For the 2400-particle Earth statement at the crust's in-situ density that
is 9.52e8 m; when a live celestial field exists the quantum is measured from its own particles
instead. The bidirectional `SiteTrigger` demands Materialize below the threshold and Deresolve
above it, and a demand stands until the crossing actually executes, so refusals stay on screen
and the trigger re-arms on ascent for the out-and-back demo arc. Materialization reads the Ground
Zero world's own `ground` block (the shared Earth's `surface_strata` at the declared lat/lon, the
declared iron ball) and goes through the refine rung: equal-mass coarse parents (the ball's real
mass sets the rung) in a guarded bowl under the real free surface, split toward release, ledger
surfaced to the HUD. The downward crossing goes through the docs/61 criterion (`SettleGauge`):
settled folds back to the summary with the fold audited; unsettled honestly stays. The smallest
honest energy hand-down: a quiescent live field's specific internal energy is sampled at the site
(quiescence is `recohere::quiescent_speed` at the coarse quantum); a mid-event field refuses with
the measured speeds stated; the full mid-event hand-down is the next milestone. The refine rung
was fixed where its first consumer broke it: the density error is now denominated by
`max(target, the particle's own in-situ density)` so a coarse-vacuum fringe cannot read an
infinite error, and a stall guard turns a measured convergence plateau into a prompt stated
refusal (thresholds an order below the slowest releasing run's flattest window, so nothing that
released before is refused now).

**Why.** The demo arc is out-and-back (open at the ball, pull out, witness the impact, descend to
the aftermath), which puts this trigger and its bidirectionality on the critical path; and the
engine must not grow a second materialization idiom when the moon-drop already has the right
shape (Law II). Law IV bounds it: the camera changes representation, never existence, and the
ledger is what proves the change conserved.

**Honest limits, stated where they bind.** This site's relief stalls the relax at a measured
4.8e-2 plateau (an order over the 5e-3 release bound; the flat and free-surface lattice cases
release fine at 171 to 1415 iterations), so the patch materializes as the EXACT conserving split
with the residual quoted on the HUD; the release remains the gate between the site and any entry
into dynamics. The ball splits exactly but does not relax: an isolated sub-resolution body has no
uniform coarse environment to relax against (measured divergent). The fine site enters no
dynamics this milestone, and the 1 m grass skin is sub-quantum at this rung. docs/46 row 18
carries all of it.

**Verified.** Red first at the pure seams: the trigger and threshold tests were written against
stubs returning nothing and failed; the materialize, hand-down, column-agreement and fold tests
failed against a stub returning a refusal, then went green as the physics landed
(`site::tests`, 6 tests). The rung's new behaviour is pinned at the refine level red-first too:
the free-floating slab measured `achieved: inf` before the metric change and now refuses with a
finite stated error; the relief stall test would catch a silent release. Full native suite
388/388 green, wasm32 check clean at the baseline warning count. Watched headed on the Mac
(mac_shot pattern, port 6499, `web/rig/mac_site_materialize.mjs` on the Ground Zero page): at
zoom 1.0 the camera sits at 648,696 km, inside the 952,220 km threshold, and the site
materializes on load with the HUD reading the full audit (ball 13 + patch 260 fine, 477 coarse
guards, 1.3134e8 kg in and 1.3134e8 kg out, zero angular-momentum drift); the particle cluster is
visible on the disc at the declared site (screenshot, magnified crop); zooming out to
2,668,401 km folds it (750 particles, 1.3134e8 kg returned, drift +0.0e0 kg) and the cluster
leaves the disc; zooming back to 187,222 km re-materializes it. Zero console errors across the
whole arc.

## 2026-07-23: the descent camera holds precision from orbit to the ground

**What.** Terra renders under one camera-relative-eye convention (docs/59 order-of-work item 2:
the descent camera that holds f32 precision to 2 m). The convention lives in `terra::fly_camera`:
`View` now carries ONLY the eye-at-origin view·projection (`vp_abs` is gone; an absolute-eye
matrix at planet radius IS the precision bug, so the type no longer offers one), and everything
Terra draws goes through it. The ground cap already subtracted the eye per-vertex in f64; the
static globe and grain-shell meshes now draw with a model translation of −eye built in f64 and
cast once, and the star billboards hang around the origin. The triplanar relief textures stay
surface-fixed across the change through an anchor: the eye folded modulo the 8 m texture tile
(tiny, so f32-safe) re-added in `globe.wgsl` before projection; this also ends the cap and globe
disagreeing about texture phase across the cross-fade. In the final metres: the coarse globe is
skipped entirely once the cap fully covers the view (`ground_cap::CAP_FULL_ALT_M`, 15 km; below
that the depth buffer's ~50 m resolution at the horizon cannot separate two copies of the same
surface, so one of them must not be there); the cap's depth-separation lift scales with altitude
(`ground_cap::cap_lift_disp`; full 20 m wherever the globe is co-drawn, shrinking below so it
can never reach the eye; the old fixed 20 m sat ABOVE a camera standing 2 m up, showing the cap's
underside); and the near-plane floor drops from 1e-6 display units (~6.4 m; it clipped the
ground underfoot at standing height) to 5e-9 (~3 cm).

**Why.** One continuous camera from celestial view to standing at ground zero, no scene switch
(Law IV: the camera changes representation, never existence). Raw f32 at Earth's radius has
~0.4 m ULP; naive world-space rendering re-rolls that error every frame as the eye moves, which
is ground-level jitter; the fix is representational, not physical, so it belongs entirely to the
camera/render layer.

**Verified.** Four new native tests pin the scheme's stated bounds: relative-eye round trip
< 1 mm at planet radius with the naive absolute-f32 path measurably losing centimetres
(`camera_relative_eye_round_trip_is_submillimetre_at_planet_radius`); the globe's model-relative
residual < 1.5 m and < 0.1 pixel at its nearest visible distance
(`globe_model_translation_stays_subpixel_where_the_coarse_globe_is_drawn`); texture phase
surface-fixed to < 0.1 mm (`triplanar_anchor_restores_surface_fixed_texture_phase`); and the lift
below the eye at every altitude while reaching full depth separation wherever the globe is
co-drawn (`cap_lift_stays_below_the_eye_and_reaches_full_depth_separation_with_the_globe`). Full
native suite 357/357 green; wasm32 check clean. Visually (headed Chromium on the Mac,
`web/rig/mac_descent.mjs`, real Metal WebGPU): a stepped descent over the Himalaya at 12,000 km /
2,000 km / 25 km (the cross-fade band where cap and globe co-draw) / 12 km / 100 m / 2 m, plus a
2 m look-down over Sahara sand; no z-fighting bands in the blend band, no cap underside at 2 m,
horizon and stars correct, sand relief texture resolving at standing height. Sub-metre smoothness
probed by stepping the camera sideways 0.25 m at a time at 2 m altitude and diffing consecutive
frames: per-step image change uniform to under 1% across seven steps (quantized eye handling
would alternate no-change/double-change at ~0.4 m ULP). The stair-step horizon silhouette at low
altitude is the elevation raster's own blockiness, present identically on main before this
change (compared side by side), and is the known missing finer LOD tier, not a regression.

## 2026-07-23: one Earth serves the orbit and the ground

**What.** The three shipped scenes stopped carrying private Earths (docs/59 order-of-work item 1;
docs/46 ledger row 16). The ownership call, made provisionally: the WORLD DEFINITION owns the body
(docs/43's direction) - `assets/bodies/earth.json` is the one record of Earth's orbital parameters,
layered matter and surface sources, and a scene only PLACES it and asks for a representation.
Concretely: the space band's `EARTH_MASS`/`EARTH_RADIUS_M`/`MOON_MASS`/`MOON_RADIUS_M` constants
(and the `DISPLAY_SCALE` const derived from one) are gone from `lib.rs`, replaced by cached reads
of the definitions; Terra's radius resolves from the body its world names
(`declared_planet_radius`, natively tested), and `worlds/earth/world.json` no longer declares
`radius_m`/`mass_kg`; the ground world declares WHERE it sits (`lat: 45, lon: -100` in
`worlds/ground/world.json`) and inherits g, air pressure and its material column from the shared
body at that site - `LayeredBody::surface_strata` derives the strata from the body's own layers
(order real, adjacent same-material shells collapsed, band thickness = log2 of the real thickness
as a declared vertical LOD, flagged; grass skin on land only, so an ocean site's seabed is the
body's own basalt crust). The hand-written default column in `GroundSurface::default_strata` and
the duplicate one inside `world::generate` are both deleted; `generate` now resolves the same
derivation every ground world gets.

**Why.** The demo needs the ball resting on the same body the Moon hits - one body owning both its
orbital presence and its local surface patch is the prerequisite for the materialization trigger
and the descent hand-off (docs/59 items 2 to 5). Two representations of "Earth" that merely agree
numerically will drift; one definition read three ways cannot (Law II). This also answers docs/59's
open question of which representation owns the shared Earth, provisionally and reversibly: the
data definition owns it, scenes hold no copy.

**Verified.** Red first: `one_earth_tests::the_three_scenes_read_one_earth` (digit-identity of
radius, mass and surface g across the space-band instance, the shipped Terra world and the shipped
ground world) and
`simulation::tests::the_ground_column_and_gravity_derive_from_the_shared_earth_at_the_declared_site`
both failed against the old world files, then passed once the private copies were deleted. The
laws scans keep it true: `a_scene_module_carries_no_copy_of_a_body_parameter` (zero hits for the
four body literals in scene modules, comments and test fixtures stripped) and the planet-block
extension of `no_scene_body_overrides_the_physics_of_the_body_it_names`. Full native suite
367/367 green; wasm32 check clean (same warning count as baseline). Headed-Chromium screenshots
of orbit.html, terra.html and ground.html: Earth renders in all three; the ground patch's
visible change is the derived column - a grass biosphere skin over Earth's own crust, mantle and
core replaces the former sand/gravel/dirt sandbox list the world file carried (the strata
sequence itself is asserted in tests).

## 2026-07-23: the re-coherence rung measures the energy it cannot yet keep

**What.** The batch downward rung (docs/61) had three physics debts: a binned grain's heat was
dropped at the grain-to-voxel crossing, its remaining sub-threshold kinetic energy was zeroed
rather than dissipated to heat, and re-cohered ejecta consolidates instantly to intact strength
and reference density. Status after this change, plainly: **debts 1 and 2 are MEASURED and
ledgered, not closed** (the receiving sink they need, thermal state on the voxel side, does not
exist anywhere in `World`, and building a fake one was refused); **debt 3 is DESIGNED only**
(docs/46 row 17c, no code). The rung's audit `Recohered` now carries `binned_kinetic_j` and
`binned_heat_j`, booked per column as energy carried in minus energy the remainder carries back
out, with heat counted only where `Material::specific_heat` is sourced so an unknown c stays
unknown. `MatterSim::recohere_settled` returns the full audit, `Simulation` accumulates it
behind `recohered_kinetic_j()` / `recohered_heat_j()`, and `run-definition` prints a `recohered`
line whenever the rung ran. All three debts are one ledger row (docs/46 row 17) naming the
deferred computations: a voxel-side thermal field that `deposit_grain` deposits into, and a
consolidation state (porosity and strength fraction) relaxing toward intact over a physical
timescale, each with the test that would close it.

**Why.** Settling is dissipation, and dissipation becomes heat, not nothing; energy that
silently vanishes at a representation crossing is a conservation violation the accounting cannot
see. Law V allows the deferral but not the silence: an IOU must name the real computation it
defers, and a loss that is measured is a loss that can be paid back and tested against. The
smaller honest step was chosen over inventing a thermal sink the store cannot hold.

**Verified.** Red first: `the_crossing_measures_the_binned_kinetic_energy_and_carried_heat` was
written against the empty audit and failed (audit read 0 J), then passed once the booking
landed. The test checks both parts against independently computed expectations (two
whole-quantum gravel grains at 2 m/s and 50 K above ambient bin entirely; a 1.6-quantum grain
loses exactly its binned quantum's share while the 0.6-quantum remainder keeps its own velocity
and temperature) and holds the ledger identity, energy in = energy still on particles + energy
in the audit, within f32 accumulation. Full native suite 363/363 green (362 at baseline, one
new); wasm32 check clean; `run-definition definitions/ejecta-ground.json` unchanged in its
matter accounting (the per-grain path empties that field before the batch rung fires, so the
`recohered` line prints only when the rung actually ran).

## 2026-07-23: the upward rung exists: split, relax, release, conserved (and unwired by design)

**What.** `crate::refine`, docs/59 order-of-work item 3: the coarse celestial SPH field can now
initialize a fine local patch, conserving mass, momentum, angular momentum, kinetic and internal
energy across the bridge. One-shot icosahedral splitting (12 vertex children plus the mandatory
retained center child; velocity and specific internal energy inherited; the center child absorbs
the f64 mass remainder so the 13 masses sum exactly to the parent's), then relax-then-release:
child positions shift, damped, against the density the engine's own symmetrized-h sum reads in
the ORIGINAL coarse field, clock frozen, coarse exterior held as a fixed guard band, and the
patch releases only when every child's relative density error is inside the stated 5e-3 bound
(an iteration cap exists solely as a divergence guard; hitting it is a stated refusal, never a
silent release). Interface discipline per zoom-in practice, as refusals with reasons on screen:
one rung per interface (a split whose children would touch matter more than one rung coarser is
refused until the shell is refined first), and a coarse particle inside the fine region is
contamination that invalidates the refinement; `contamination_check` is the standing per-step
form the future wiring runs. The caller-facing ledger audits all five quantities before, after
split and after relax, with the relax's angular-momentum drift bounded by its own accumulated
`sum m |dx| |v|`. The Holsapple-Housen pi-scaling gate (gravity regime; v2.2.1 hard-rock
K1 0.012 mu 0.55 and regolith K1 0.14 mu 0.4 rows, vintage named in the constants) ships
alongside, ready for the future end-to-end crater test.

The two stencil constants were RE-DERIVED for the engine's one cubic-spline kernel
(`atmosphere::sph_w`, full support inside r < h) instead of copying the literature values, per
docs/59's own instruction: least squares of the integrated squared density error between parent
kernel and 13-child stencil, 141^3 grid quadrature cross-checked at 201^3. The unconstrained
problem is degenerate (zero separation reproduces the parent exactly and refines nothing); the
interior stationary point is separation 0.3051 h with child smoothing 0.7915 h, residual 0.70%
L2 (peak 0.38% of the parent's central density). The literature pair (0.4 h, 0.9 h in the
2h-support convention, i.e. 0.2 h here) measures 4.9% L2 on this kernel: seven times worse,
which is why the re-derivation was not optional.

**Why.** docs/59: the zoom from the celestial energy event to local ground zero must hand the
state DOWN, not invent it (Law IV: the camera changes representation, never existence; Law V: no
analytic effect standing in for the field). This module deliberately lands with ZERO production
consumers: the camera-driven materialization trigger's home depends on collision routing
decisions that are still open upstream (docs/58 item 7, docs/59 open questions), so wiring now
would race a moving seam. Per the docs/46 rule that a verified law with no consumers stays an
open ledger row, row 18 carries the flagged IOU and names the M4 zoom materialization milestone
as the wiring owner.

**Verified.** Seven native tests, written red first against stubs, all green: exact conservation
of all five audited quantities on the split (mass, momentum, kinetic, internal to 1e-6 relative;
angular momentum to 1e-5, all f32 rounding scale, with the stencil reconstructed child by child);
the relax releases inside the bound on a uniform basalt field (raw blip 7.5e-2, released 5.0e-3
in 378 iterations, max child shift 0.234 m at h = 2 m, relax angular-momentum drift 3.8e2 against
its stated bound 7.9e4 on |L| 6.9e6) and across a basalt/iron interface (9.5e-2 released to
5.0e-3 in 1212 iterations, material identity preserved through the rung); contamination refused
with the offender named and the world untouched; a second rung against unbuffered coarse refused
while a buffered interior second rung is admitted; the pi gate reproduces a hand-computed Meteor
Crater example (rim radius 425 m predicted, observed 593 m, ratio 1.39, inside the factor-2 gate)
and degrades explicitly to the order-of-magnitude bound when the predicted crater rivals the
body. Full native suite 369 green, 22 ignored; wasm32 check clean.

## 2026-07-23: the settled impact site re-coheres into meshed ground

**What.** The de-resolution ladder gains its batch downward rung and a production trigger
(docs/61). New `crate::recohere`: a SETTLED region of a particle field bins back into the voxel
`World`, and the existing surface-nets mesher renders the result as walkable ground. The settle
criterion is physical, not a frame count: quiescent speed `sqrt(2gΔ)` — below it a grain's
kinetic energy cannot buy a one-cell rise, so its motion is sub-resolution at the binning scale —
held continuously for one cell dynamical time `sqrt(2Δ/g)`, integrated in seconds by
`recohere::SettleGauge` (the docs/57 #4 lesson applied forward: the step size must not decide
when matter stops being matter). Conservation is the contract: grain mass accumulates per column
and material in f64, whole voxel quanta `ρ·Δ³` deposit through the ONE grain→voxel law
(`deposit_resting_grain`'s body, extracted as the shared free function `matter::deposit_grain`),
and the sub-quantum remainder stays particles — matter is never deleted to lower a count, and
gravel comes back as gravel. A still-moving region is refused before any write. Production
consumer: `Simulation::step` → `recohere_when_settled` — once no meteors are in flight and the
whole remaining field has been quiet for one dynamical time, the aftermath folds back into the
world and the dirty flag drives the remesh. The SPH remnant (the observed bare-particle-ball gap,
in a path under active upstream rework) and the shattered cohesive body are docs/61's flagged
wiring IOUs.

**Why.** After an impact settles, the remnant stayed a bare particle field forever: every
downward mechanism existed and was verified (docs/46 row 6), but nothing ever decided "this
region's excitement has passed" for a field at once. docs/44 §6 already named the honest trigger
— demote on quiescence, a kinetic-energy bound, never "when motion stops" and never disinterest —
so this increment is that trigger plus the conserving batch bin, not new physics.

**Verified.** Red-first natively: a settled synthetic gravel mound (including a deliberately
sub-quantum grain) folds with mass conserved to f32 accumulation error, material preserved and
the remainder surviving as a particle; a still-moving region is refused with the world untouched;
the criterion settles identically at 0.1 s and 0.001 s steps and a mid-window jolt resets the
sustained clock; the `MatterSim` adapter pins that 4 × 0.4-quantum grains yield exactly one voxel
plus a 0.6-quantum particle (the per-grain one-grain-one-voxel law would have conjured 2.4 quanta
of rock); end to end, a thrown meteor's settled aftermath leaves zero bare particles. Full suite
353/353 green; `cargo check --target wasm32-unknown-unknown -p engine` clean; ground scene
rig-watched on a Mac (meteor dropped, aftermath settled, ejecta and crater render as meshed
ground, no frozen grain field).

## 2026-07-23: pan input survives a real hand

**What.** Three input fixes, one theme: every pan gesture reaches the one pan path each scene
already owns. (1) `camera-input.ts` no longer decides pan-vs-walk only at the pointerdown
instant: a left-drag anchored on the canvas upgrades to a pan the moment shift is seen, and in a
scene that supplies a pan handler shift+left-button always means pan, never reverse walk
(reverse stays on shift+ctrl, the keyboard side of the grammar). (2) The full-viewport `#status`
overlay on every scene page is now pointer-transparent; it used to swallow all canvas input
whenever any message was showing (2.5 s after every meteor drop, for one). (3) Shift+scroll and
the horizontal wheel axis now pan in Ground, Terra and the space band, feeding the SAME handler
as the drag with the sign of a grab (the world follows the fingers); bare vertical scroll keeps
its walk/zoom meaning. No engine change: `pan_view` / `pan_tangent` were already there.

**Why.** A rig could pan; a human could not. The rig pressed shift, then the button, atomically.
A real hand lands the two within milliseconds in either order, and when the button won the race
the gesture fell into the shift+left = reverse-walk meaning: the camera flew backward and the
drag did nothing, which reads as "pan is dead". The same two physical inputs must not mean two
things decided by a millisecond race. And a MacBook trackpad has no middle button, so scroll
needed a pan chord of its own.

**Verified.** Headed on the Mac (mac_shot pattern): shift-then-button drag pans (as before);
button-then-shift drag NOW pans where it previously walked the camera backward off the patch;
middle-drag still pans; plain left-hold still walks forward; right-drag still looks. Shift+wheel
on Ground translates the eye in the view plane at constant 2 m eye height; on Terra it moves
lat 20.00 to -1.72 at constant alt 8000 km through `pan_tangent` (translation, not zoom); bare
wheel still dollies Ground and takes Terra from alt 8000 km to 4497 km at constant lat/lon.
Pointerdown targets confirmed: with a status message showing, events landed on `#status` before
the fix and on the canvas after.

## 2026-07-23: the orbit camera pans, in the focused body's frame

**What.** The space band's camera can now translate its look target off the focused body.
`render::Camera` carries a `pan` offset plus `pan_by_pixels` (the screen-plane translation, with a
DERIVED scale: one frustum height at the focal plane per viewport height of drag, so the world
tracks the pointer one-for-one at every zoom); `OrbitDemo::pan_view` exposes it to the page, and
the focus setters (`focus_earth`, `focus_moon`, `cycle_focus`) snap the offset back to zero. The
offset lives in the frame that rides the focused body: the orbit renderer re-centres the world on
the focus each frame, so a pan composed against Earth follows Earth around the Sun. The shared
gesture module (`camera-input.ts`) gained an OPTIONAL `onPan` handler, bound to shift + left-drag
and middle-drag; scenes that supply none (ground, terra) keep the previous grammar untouched.

**Why.** The space scenes offered rotate, zoom and body focus, but the target was always a body's
centre, so there was no way to frame the debris disk between Earth and a forming moonlet, or an
impact site off-centre. Law IV bound: this is representation only, a camera target, and it moves
no matter.

**Verified.** Two native tests pin the maths (`a_full_viewport_drag_pans_exactly_one_frustum_height`,
`pan_moves_in_the_screen_plane_and_reverses_cleanly`); full suite 364/364 green; wasm32 check
clean. Watched headed on the Mac (mac_shot pattern, stars blocked so the bodies are unambiguous,
positions measured from decoded PNGs): a shift-drag put Earth off-centre at (730, 244) with the
Moon in frame; over the next 4 s at time ×14,750 Earth held that screen position within one pixel
while the Moon advanced ~66 px along its orbit and the Sun showed no parallax (it is 1 AU behind
the target plane; an inertial-space target would have let Earth race off-screen at ~700 px/s).
The Earth focus button snapped the framing back to centre.

## 2026-07-23: the ground HUD says whether the ball survived

**What.** The ground scene's ball line leads with a one-word verdict: INTACT (green), DENTED
(amber) or SHATTERED (red), followed by the parcel/bond counts it always showed.
`CohesiveBody::verdict()` (simulation.rs) names the bond state the physics already runs on:
"intact" while every forged bond holds, "dented" while a minority has fractured, "shattered" once
fewer than half survive, the same half-way boundary the fracture tests assert. `Ground::body_verdict()`
exports the word to the page, and `web/rig/ground_ball_shatter.mjs` now asserts the leading word
agrees with the bond collapse it measures.

**Why.** First-hand user test: after a direct hit, "ball 33 parcels · 0 bonds" read as "still
intact" to fresh eyes, because parcels are conserved matter and never drop, and the parcel count
led the line. The destruction meter was the bond count, second and unlabelled as such. The sim
knew the answer all along; the HUD just never said it. No new physics: the verdict is a NAME for
an existing public number, so a viewer is not asked to interpret it.

**Verified.** Full native suite green (362 passed) with new assertions: the sufficient-meteor test
requires `verdict() == "shattered"` after the hit, the insufficient-meteor test requires "intact"
after settling and never "shattered" after the boulder. wasm32 check clean. Watched headed on the
Mac (mac_shot pattern): before the hit the HUD reads "ball INTACT · 33 parcels · 212 bonds"; after
the 17 km/s meteor it reads "ball SHATTERED · 33 parcels · 0 bonds" over the scattered parcels,
and both words are legible at a glance.

## 2026-07-23: one deposition door, and the meteor destroys the ball on its own

**What.** The docs/23 north-star sentence runs. Impact deposition in the ground world is ONE
operator, `Simulation::deposit_event` (docs/60): an event's energy and momentum reach terrain
voxels, cohesive-body parcels and debris grains in one walk, split by geometry and coupling alone,
w = V·exp(−d/λ)/d² with λ the crater radius the event's own energy opens in the matter at the site
(E/σ, the same accounting every impact already uses), each share delivered through the operator
that owns its container (`matter::impact`, `Aggregate::deposit_impact`, `deposit_impulse` +
`deposit_shock_heat`). Detection only picks the site: body contacts stay with the swept collision
owner, and a ground landing is now bisected to where the trajectory crosses the shared ground
height instead of the post-step sample (which at 500 m/s was metres underground and coupled the
event to the wrong matter). Inside an aggregate, each parcel's fate is `damage::classify` on the
energy density deposited in it against its material's catalogued thresholds; past Intact it holds
no tensile bond. The ground scene's aim ray now returns the first MATTER it meets, terrain or
parcel, so the crosshair rides the ball (gold) and a meteor aimed at it hits it; the page's HUD
panel is un-hidden (it was being filled and never shown); the drop is 1,200 kg of iron at 17 km/s,
an asteroid's real arrival speed.

**Why.** A meteor that met the ball fed ONLY the ball; one that met the ground fed ONLY voxels;
debris got nothing. Three answers to "the impact's energy arrives" is the docs/46 pattern and
exactly the per-object special-casing docs/16 forbids, and it blocked the docs/23 demo: the ball
could never die of the impact reaching it, only of a scripted branch we refuse to write. The split
had to be physical, so it is geometry (where the matter is) and coupling (spreading + absorption
over the crater scale); the missing shock shadowing/impedance computation is a flagged IOU in
docs/60, named in domain terms.

**Verified.** Native, red-first: `a_sufficient_meteor_shatters_the_ball_and_its_hottest_parcels_glow`
(bonds collapse under half, rms spread doubles, peak parcel temperature emits through
`emission::incandescence`, and the same event craters the ground beneath) and
`an_insufficient_meteor_displaces_the_ball_and_it_survives` (momentum arrives, nine tenths of the
bonds hold, nothing glows); `one_impact_event_reaches_the_terrain_and_the_ball_through_one_door`
and `an_impact_event_heats_debris_grains_already_in_flight` pin the walk itself. Full suite
346/346 green; `cargo check --target wasm32-unknown-unknown` clean. Rig-watched on the Mac
(`web/rig/ground_ball_shatter.mjs`, headed Chromium on Metal): crosshair gold on the ball, drop,
HUD bond count 212 to 0, shots show the struck parcels glowing gold-orange and scattering into the
new crater. fmt untouched (hand-edited).

## 2026-07-22: the ball is cohesive matter in the ground scene

**What.** docs/23 step 1. A ground world's definition gains a `bodies` list (material, radius,
position, nothing else); `Simulation` builds each as `Aggregate::cohesive`: a lattice of real
particles at the material's density, bonds at the material's own elastic modulus (k = E*L, capped
for explicit stability, flagged), damping from `zeta_for_restitution` of the material's restitution,
under the named planet's emergent surface gravity. Each step the body substeps to its bond
stiffness and every particle resolves against the terrain through
`granular::terrain_contact_resolve`, the same constraint the grains and the camera shell use. The
renderer draws its particles through the existing instanced path (own albedo, incandescence from
its temperature); the HUD and `run-definition` report parcels, bonds, and com height over the
ground beneath. A meteor in flight meets the body through the one door: `fly_meteors` hands the
swept segment to `interaction::detect_swept`, and on a matter-resolving contact the door's
reduced-mass energy and the striker's momentum go into the parcels via `Aggregate::deposit_impact`
(deposited at the point on the body's own outermost matter, since the door's site is the striker's
centre at contact). No ball-specific collision or destruction branch exists; what the energy does
next is `damage`'s call in the following step of docs/23.

Found and fixed on the way: the camera-shell sweep re-lerped each sample against its own mutated
endpoint, contracting every no-contact move toward last frame's eye by n!/n^n, so any move longer
than the shell froze the camera. The ground scene's wheel dolly and hold-to-walk did nothing while
mouse-look kept working; it surfaced when walking the camera up to the ball for the visual check.
The sweep now samples the fixed segment and carries the accumulated correction
(`granular::sweep_shell_resolve`).

**Why.** The rigid `body::Sphere` probe was the last bespoke object, and the terrain deletion took
even that; the scene had no ball at all. Making the ball declared DATA plus shared laws (cohesion
from the material, contact from the one terrain constraint, impact from the one door) is what lets
the next step assert emergent destruction with no special case to delete.

**Verified.** Native TDD, red first: `a_declared_cohesive_ball_falls_rests_on_the_terrain_and_stays`
(falls under gravity, lowest parcel comes to rest on the surface within tolerance, stays, no bond
lost to its own landing), `a_meteors_energy_reaches_the_ball_through_the_shared_door` (parcels heat
and the com recoils after `detect_swept` reports the contact), `a_body_of_unknown_material_is_refused`,
plus sweep regressions `an_unobstructed_sweep_reaches_its_destination` and
`a_sweep_into_the_ground_is_pushed_back_out`. Suite 338/338 green; wasm32 check clean. Live scene
(macOS headed Chromium, /ground.html): HUD reads ball 33 parcels, 212 bonds, com 36.0 m over ground
33.5 m, exactly the 2.5 m lattice rest height, stable across 30 s and camera moves; screenshots
reviewed showing the iron ball seated on the dune crest at the initial crosshair point.

## 2026-07-22: the GPU gravity dispatch grows its second knee, and the LBVH tree goes live

**What.** The GPU Barnes-Hut tree banked by docs/36/37 is enabled in the live gravity dispatch.
`GpuGravity` builds the tree pipelines from the same verified `bh_gravity.wgsl`, and `GravityField`
routes within the GPU by a SECOND measured knee: below `TREE_KNEE` the exact direct sum (the ideal GPU
workload, and exact), at or above it the theta=0.5 LBVH tree, whose O(N log N) eventually beats the
quadratic sum even counting its full per-call build. The dispatch order is the verifier's, with two
deliberate deviations, both recorded at the site: (1) bbox+Morton+sort run on the CPU, bit-identically
to `cs_bbox`/`cs_morton` (the verifier pinned that equality), because the GPU radix sort was never
built, so the codes must visit the CPU anyway and a mid-pipeline blocking read-back would be illegal in
the browser; (2) the single-pass `cs_com` climb is replaced by new level-synchronous ping-pong sweep
kernels (`cs_com_sweep` xR, `cs_com_resolve`), because enabling the climb on Metal produced
NONDETERMINISTIC internal COMs: the climb needs release/acquire ordering around its `ready` atomic, and
WGSL atomics are relaxed with no device-scope fence, so its coherence on the Vulkan/NVIDIA verifier box
was hardware grace, not a guarantee. The sweep's only cross-invocation ordering is the pass boundary,
the one ordering WGSL does guarantee; R is the root-height bound (32 plus duplicate-run levels), and
each sweep costs microseconds. Theta stays 0.5 and K stays 1 particle per leaf, both the values the
measurements justify; `Aggregate` needed no wiring change, since the routing lives inside the field.
`GpuHost` now requests the WebGPU baseline limits (8 storage buffers per stage) instead of the
downlevel set (4): the tree binds 7, and the browser scenes already run with the adapter's full limits,
so the stricter native host was rejecting pipelines the shipping page accepts.

**Why.** The direct-sum dispatch bought the moon-drop its frame back, but a quadratic sum has an
expiry date, and docs/37 predicted Apple silicon would pull the tree's crossover far below the 2070's
128k. Measured, it did: into the range a large debris cloud actually reaches. Above the knee the tree
is the most physical thing the budget buys (same bounded multipole the CPU tree already accepts, at a
fraction of the wall time); below it the exact sum keeps both fidelity and speed, so each range keeps
its honest winner.

**Verified.** On this box (Apple M4 Max, Metal): new `gpu_tree_speedup` (both columns full per-call
cost, K in {1,8,32} swept, three runs) reads the knee off the table: dead heat at N=12000
(0.98x/1.02x/1.19x), tree wins every 24000 measurement (direct 5.5-6.6 ms vs tree 4.8-5.3 ms), then
~2x at 48k, ~5x at 96k (72 ms vs 15 ms), 9-12x at 192k (413 ms vs 35-46 ms, against 2.6 s for the
single-thread CPU tree); K=1 fastest at every N, matching the 2070 finding. `TREE_KNEE = 24000` sits at
the first DECIDED N, with the dead heat kept by the exact sum because fidelity breaks ties. A parallel
session used the GPU intermittently; runs were repeated and the crossover location held in every run.
Correctness on Metal, in-crate: `the_gpu_tree_matches_the_cpu_tree_within_the_theta_bound` (RMS rel
5.8e-3 vs the CPU octree and 5.4e-3 vs the exact f64 sum at N=6000, inside the 1e-2 verifier bound,
plus bitwise-repeatability, which the racy climb measurably failed);
`the_gpu_tree_opened_fully_recovers_the_direct_sum` (theta 1e-4: RMS 2.1e-6, every particle reached
exactly once); `at_the_tree_knee_the_field_dispatches_the_tree` (at N=24000 the dispatched field
carries the tree's nonzero multipole signature against the exact sum AND matches the CPU tree, RMS
3.9e-3, so the routing is pinned by physics, not inspection). Before the sweep fix the tree-vs-exact
error GREW with N (6.4e-2 at 24k, 2.2e-1 on a uniform box) and differed run to run; after it, 3.4e-3
and bit-stable. Full native suite 338/338 green; wasm32-unknown-unknown check clean;
`tools/gpu-bh-verify` still compiles untouched in behaviour (its Vulkan box cannot be reached from this
Mac, noted in docs/37). fmt untouched (hand-edited).

## 2026-07-22: orbital debris self-gravity dispatches to the GPU above a measured knee

**What.** The verified GPU direct-sum gravity kernel is now the live self-gravity path for large
debris clouds. `gpu_gravity` gains `GravityField`: the `cs_gravity_direct` pipeline bound to the
device and queue it runs on (cloned handles of the scene's SHARED device, the `gpu_sph` pattern,
never a second adapter). `Aggregate` carries an optional `GravityField`, and `accelerations_masked`
shunts the self-gravity of any pass whose gravity-ACTIVE count is at or above
`gpu_gravity::DIRECT_SUM_KNEE` to the exact GPU sum; below the knee, with no field attached, or
while a browser dispatch is in flight, the θ=0.5 CPU tree stands unchanged. The knee gates on the
active count because under a block-timestep mask the CPU tree only evaluates the active subset:
gating on total N paid a full GPU round trip to replace microseconds of traversal on the
few-particle sub-ticks (measured below). `OrbitDemo` attaches the field when a swept impact
materialises its debris cloud; an absorbed second impact joins the first cloud's field. Natively the
dispatch is synchronous and exact at the pass's own positions. In the browser, where WebGPU forbids
blocking, it is the engine's two-phase read-back: a pass harvests the previously submitted field
(one submission old, the same class of deferral step_block already accepts when a coasting particle
keeps its last kick's acceleration) and the CPU tree covers passes with nothing landed, so no pass
blocks and none goes without gravity.

**Why.** The moon-drop bottleneck measurement found 64% of the debris accel pass in a single-thread
CPU tree walk while the exact parallel sum it approximates sat verified on the GPU, wired into
nothing. Self-gravity is a per-particle N-body force; the engine's stated architecture is to shunt
each job to the processor that handles it best by a MEASURED knee. The direct sum has no multipole
error, so above the knee this is higher fidelity and higher speed at once.

**Verified.** On this box (Apple M4 Max, Metal): `gpu_gravity_matches_the_cpu_direct_sum` passes
(worst per-particle relative error under the 1e-3 bound); the new
`the_aggregate_dispatch_matches_the_cpu_tree_within_the_theta_bound` pins the dispatched field to
the CPU tree within RMS relative 1e-2, the same bound `tools/gpu-bh-verify` enforces between tree
and direct sum (that harness itself requests a Vulkan adapter and cannot run on this Mac; the
in-crate tests carry the bound here); the new
`below_the_knee_the_cpu_path_stands_even_with_a_field_attached` pins the gate by bitwise identity.
`gpu_gravity_speedup` (now sweeping N=200..6000 against the tree's true per-pass build+eval cost):
GPU round trip ~1.3 ms and nearly flat, CPU crosses it between N=400 (0.5x) and N=750 (1.9x), then
GPU 1.5x at N=2000, 1.7x at N=3000, 3.6x at N=6000; the knee interpolates to ~550 and is set at
600, on the CPU-favoured side of both measured boxes. End to end, `debris_step_scaling` with the
dispatch attached: accel pass at N=3000 15.2 ms to 7.0 ms (2.2x), full step_block frame 117 ms to
78 ms (1.5x); at N=1500, 31.5 ms to 24.4 ms. Full native suite 335/335 green (the two new GPU tests
included); wasm32-unknown-unknown check clean. fmt untouched (hand-edited).

## 2026-07-23: the CPU Aggregate debris path is deleted; one collision resolution remains

**What.** The orbital scene's CPU `Aggregate` impact-debris path is gone (docs/58 item 7; docs/46
ledger rows 1 and 3 closed). Deleted from `OrbitDemo`: `moon_debris: Option<Aggregate>` and every
reader: the swept-collision materialization block (`impact::build_impact_debris_scaled` call,
cap-mass bookkeeping, absorb/GPU-gravity wiring), the debris stepping block (step_block, momentum
mirror, J2/tidal kicks, drain_settled/crater heal), the CPU debris substep mode and its tight
12-substep budget, the crater-wall render machinery (`WALL_N`, `hole_radius`, `impact_site_rel`,
`crater_heal_m3`), the per-fragment snapshot plumbing (`FrameSnap` carries only body positions
now), and the retired-in-name CPU birth scenario (`start_birth`, `birth_mode`) whose only debris
route was the deleted block. The consumers converged onto their SPH twins: `disk_stats_json`
delegates to the SPH read-back measurement (`gpu_sph::disk_stats_json`) outside geologic time,
`enter_geologic_time` keeps only the SPH moonlet promotion, `debris_count` counts the SPH
snapshot. A bare point mass that reaches contact (nothing with declared matter can, since the SPH
hand-off takes it at its resolution distance) merges inelastically and momentum-conservingly into
the planet, energy measured and reported, never materialised a second way. In `impact.rs` only
`build_impact_debris` (the moon-into-Earth wrapper) retired with its last caller; the general
builders and the furrow/plough/vapor physics stay, consumed by that module's measurement tests.

**Why.** One question, one answer (docs/46): a dropped moon and Theia are the same event at two
scales, and the live drop already routes through the SPH machine; the Aggregate path was a
second, unreachable answer waiting to drift. docs/58's order of work names this retirement as the
step after birth and the live drop went generic; this lands it.

**Verified.** The pinned physics moved BEFORE the CPU test retired:
`gpu_sph::a_dropped_moon_impact_leaves_most_matter_bound_on_the_sph_path` replaces
`impact::a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`. The GPU half of the SPH
machine cannot run headless in the native suite, so the port asserts at the largest native seam:
the live drop's own staging (`build_far_apart_n`), a CPU relax at the staged dt, the same assembly
(`assemble_from_relaxed_n`) on a drop geometry derived from energy conservation, and the CPU KDK
twin (`HydroBody::step`, the physics `sph_step.wgsl` is verified against by `tools/sph-verify`);
only the dispatch differs, and that limitation is stated in the test. Measured there: bound
fraction 100% (pin: >60%), impactor hottest parcel 1,977 K before to 34,349 K after (emergent
shock heating past the 800 K emission threshold), remnant radius 5,601 km. Suite: 362 passed both
before and after (one CPU test retired, one SPH-side pin added, net zero); wasm32 check clean;
fmt untouched (hand-edited). Live (headed Chromium on the Mac, Apple Metal, `web/rig/mac_drop.mjs`
on orbit.html): Drop Moon crossed its resolution distance, the SPH machine owned the frame
(Relaxing, Approaching, Assembling, Dynamics), 2,451 particles resolved and merged over a 150 s
session with zero console errors at 25 to 28 fps. The two runtime caveats from the proof run,
checked deliberately. First, the remnant radius: it reads 5,873 km live (5,601 km at the native
seam, N 700 vs the browser's 2,400) against Earth's 6,371 km. But `remnant_km` is the radius
enclosing 85% of the system mass, not a surface radius, and the DECLARED Earth's own 85%-mass
radius, computed from its layer densities, is 5,941 km. The live remnant is within 1.2% of what
an intact Earth measures by the same yardstick; the alarming "4,900 km" of the proof run predates
the particalize-at-real-density staging (the old reference-density seeding under-massed Earth to
~64%, the flagged IOU pinned in `a_moon_drop_builds_and_strikes_through_the_same_assembly`).
Second, the remnant stays a particle ball with no re-coherence to a rendered surface within the
session; that is docs/61's flagged IOU 1 (the SPH-remnant wiring of `recohere`), named there and
deliberately not raced here.

## 2026-07-23: the live drop joins the generic collision engine, and one IOU dies

**What.** The live de-orbit hand-off is adapted to the generic N-body primitives (docs/58).
Staging: `start_live_drop_sph` builds its relax input with `build_far_apart_n`, the same call the
declared birth path now makes, keeping the engine's 2400-particle budget at one particle mass
across the system with the 50-particle small-body floor. Every layer's own catalogue EOS reaches
the shader through the shared N-material table kept in `sph_eos`, so the flagged two-EOS collapse
(N-material matter resolved onto basalt-like vs iron-like slots by reference density in
`push_body`) is DEAD on the live path; `far_apart_pair`/`push_body` remain only under the legacy
declared builders and the native tests that pin them. Assembly: the live `Assembling` arm places
the collision with `assemble_from_relaxed_n` on the trajectory the N-body actually integrated,
handing the target's spin over as the full vector `omega = L/I` from the emergent inertia, any
axis, where the old primitive took only a +z scalar rate. The impactor's spin stays a zero
vector, a Law V IOU NARROWED at the site: the assembly accepts per-body vector spin now, and what
is still missing is per-body spin state in the N-body integrator (only the planet's `spin_l`
exists), so the impactor's own rotation still cannot be handed over. The declared birth arm is
unchanged.

**Why.** Robin built the generic primitives (N-material upload, `SphAssembly`,
`assemble_from_relaxed_n`, `build_far_apart_n`) and moved birth onto them; leaving the live drop
on the two-slot, +z-scalar path would have been the same physical question answered two ways
(docs/46). One collision engine, both entrances.

**Verified.** New `a_live_drop_assembles_n_materials_on_its_own_geometry` pins the whole live
chain: generic staging to a shared table of at least three materials, every `mat` still a valid
index after assembly, both bodies at their matter's real mass, the impactor exactly on its live
(offset, relative velocity), and an off-axis spin component reaching the target's velocities.
The crossing and live-geometry contracts stay green
(`a_body_crossing_its_resolution_distance_is_reported_with_its_live_geometry`,
`assemble_at_honours_a_given_live_geometry`, `a_particalized_pair_stages_far_apart_at_real_mass`).
Full native suite 349/349 green; `cargo check --target wasm32-unknown-unknown -p engine` clean.
fmt untouched (hand-edited).

## 2026-07-23: birth builds through the generic engine

**What.** The declared birth path consumes the generic collision engine (docs/58 item 7).
`start_gpu_impact` particalizes the target and impactor from their declared matter via
`build_far_apart_n` (equal particle mass across the bodies, with the same 50-particle
small-body floor) instead of building the Earth/Theia
iron-basalt pair through `build_far_apart_from`, and keeps the shared N-material EOS table in
the new `sph_eos` field so the `Assembling` and dynamics uploads carry the same table the
particles' `mat` indices point into. The two-body assembly's returned `[basalt, iron]` pair is
ignored at the assemble site on both paths; the live relax staging now also parks its table (its
unchanged two-slot pair, for now) in `sph_eos`, so the live hand-off keeps its exact behaviour
while the declared birth path moves onto the generic build.

**Why.** Robin: this is the ONE collision engine consuming its first scene. The bodies reach
the GPU as their own matter (real mass, per-material catalogue EOS), not as a named
definition's two-slot approximation.

**Verified.** Upstream rig-verified before the mirror: birth lofts a proto-lunar disk and
accretes a ~1.08 lunar-mass Moon (disk 1.73 lunar masses, 43 percent Earth) with no dispersal
under the real-mass init. Here: full native suite 348/348 green,
`cargo check --target wasm32-unknown-unknown -p engine` clean. fmt untouched (hand-edited).

## 2026-07-23: the GPU SPH goes N-material and N-body

**What.** Three additions to `gpu_sph` (docs/58 items 4/5/7). First, the GPU EOS path is
generalized from the fixed `[basalt, iron]`
pair to an N-material table: `eos_buf` is sized for `MAX_MATERIALS` (16), `GpuSph::upload` takes
`&[SphEos]`, and `SphEos::from_tillotson` mirrors any engine Tillotson EOS to the GPU (the shader
already indexes `eos[mat]` as a runtime array, so this is Rust-only). `SphAssembly` builds the
particle set plus the shared EOS table for a collision of any number of bodies: each body is
particalized and appended with its own source index as `prov`, and materials dedup across all
bodies into one table of at most 16 entries. Second, `assemble_from_relaxed_n(particles,
placements)` places each source body (prov k) at its own `placements[k] = {offset, vel, spin}`,
where spin is a vector (any axis) applied as `v = vel + omega x (r - com)`; the two-body
`assemble_from_relaxed_at` now delegates to it byte-identically (the +z scalar becomes
`omega = (0, 0, spin)`), so the existing geometry tests pass unchanged. Third,
`build_far_apart_n(bodies, separation)` particalizes each `(matter, resolution)` and places the
bodies far apart on a line for the GPU relax, returning particles plus the shared deduped EOS
table plus softening and the relaxation Courant dt: the generic relax input for any number of
bodies, the replacement `build_far_apart_from` and `build_impact_bodies_from` converge onto. The
only merge conflict was the tail of the test module, where main had appended the particalized
far-apart staging test; all tests from both sides are kept.

**Why.** Robin: only two is not robust, and spin is a vector, not a scalar about +z. A
three-moon impact and a two-body impact must be the same path, and a particalized body's own
materials must reach the shader instead of collapsing onto two assumed slots. These are the
primitives the live hand-off needs to shed its flagged IOUs (the two-EOS collapse and the
scalar spin).

**Verified.** New `sph_assembly_builds_an_n_material_field_across_bodies` (an Earth+Moon field
dedups to at least three materials, every `mat` a valid index, distinct provenance, iron core
indexing iron, mass conserved) and `assemble_n_places_every_body_and_spins_about_any_axis`
(three bodies each land at their own offset and velocity; an X-axis spin gives the target a vz
a +z-only path could never produce), and `build_far_apart_n_particalizes_and_separates_the_bodies`
(Earth and Moon build to a shared N-material table, placed far apart). Full native suite 348/348
green;
`cargo check --target wasm32-unknown-unknown -p engine` clean. fmt untouched (hand-edited).

## 2026-07-23: the live drop particalizes each body's own matter

**What.** The definition-id stopgap at the live SPH hand-off is dissolved into `particalize`
(docs/58 item 4). `start_live_drop_sph` no longer sets `impact_def` to `earth`/`moon` id strings:
it reads the two colliding bodies' own layered matter (`BodyMeta.matter`, the planet resolved by
declared role), builds each with `HydroBody::particalize` (per-layer catalogue EOS, specific heat
and declared geotherm; mass from the declared in-situ densities), and stages the pair with the new
`gpu_sph::far_apart_pair`, the composition-agnostic placement the declared birth path's
`build_far_apart_from` now also delegates to. Resolution stays the engine's compute budget: the
same 2400-particle target as the birth path, spent as ONE particle mass across the system, with
the same 50-particle small-body floor. The `Approaching` phase's resolve distance for a live drop
now reads the actual bodies' mass and radius, the same inputs the crossing detection used, so a
live drop still assembles immediately; the declared birth impact keeps reading its impact
definitions. Detection eligibility now requires a body (and the planet) to carry matter: a bare
point mass cannot be particalized, so it stays a point mass on the CPU contact path instead of
being rebuilt from a named definition. That retires the SPH hand-off in the un-migrated worldless
default scene (whose `body_meta` is empty), which is the degraded world-fetch-failure path only;
every shipped deorbit world declares profiles that carry matter. The GPU stepper still resolves
the N-material matter onto its two EOS slots by reference density in `push_body`, a flagged
resolution limit recorded at the hand-off site.

**Why.** Robin's particalize (mirrored in the previous merge) made the name-free build possible:
per-body matter now carries everything the SPH needs, so the live hand-off no longer needs the id
strings that `ImpactDef` resolves definitions by, and the recorded stopgap could go. One collision
path, driven by what the bodies ARE, not what they are called.

**Verified.** New `a_particalized_pair_stages_far_apart_at_real_mass` pins the staging: both
bodies at their matter's real mass (the under-massed EOS-reference build stays dead), the
requested separation, at rest, and the target's iron core and rocky mantle both reaching the GPU
material slots. `a_body_crossing_its_resolution_distance_is_reported_with_its_live_geometry` and
`assemble_at_honours_a_given_live_geometry` stay green. Full native suite 337/337 green;
`cargo check --target wasm32-unknown-unknown -p engine` clean (one warning fewer than before).
fmt untouched (hand-edited).

## 2026-07-22: the live moon-drop wiring reads the generic body

**What.** The live-drop path reconciled with the docs/58 realignment. `live_resolution_crossing`
gains a `planet` parameter; its `step_substep` caller passes `planet_idx()`, the index the scene's
declared role resolves to, so the check no longer assumes the `[Sun, planet, moons...]` layout. The
scan covers every body except the planet itself (the star is checked and correctly finds nothing,
matching how the swept detector treats it). The `Approaching` separation read and the `Assembling`
offset/velocity/spin reads use the same role-resolved planet. The `Assembling` arm converts the
target's spin angular momentum with `spin_inertia()`, the moment of inertia emergent from the body's
own layered matter, in place of the uniform-sphere `2/5 M R^2` with a hardcoded Earth radius; that
constant now survives only in the CPU debris materialization, which retires with the `Aggregate`.
`start_live_drop_sph` still names `earth`/`moon` by definition id, with the reason recorded at the
site: per-body matter (`BodyMeta.matter`) carries a body's composition, not the id that
`ImpactDef`/`assets/bodies/<id>.json` resolve definitions by.

**Why.** The wiring shipped assuming three things the generic-body realignment removes: the planet at
index 1, the solid-sphere inertia, and named lookups. Keeping the old reads would leave two answers
to "which body is the planet" and "what is its inertia" in the same code path, which is the exact
fragmentation docs/58 exists to close. The spin handed to the SPH now uses the same inertia the
rotation and the HUD read, so a declared day length survives the handoff.

**Verified.** `a_body_crossing_its_resolution_distance_is_reported_with_its_live_geometry` extended
with the docs/58 permutation check: a reordered body list (planet at index 3) reports the same
crossing with bit-exact identical body-centric geometry. Full native suite 333/333 green;
wasm32-unknown-unknown check clean. fmt untouched (hand-edited).

## 2026-07-22: the moon-drop routes into the SPH machine at resolution distance

**What.** The live de-orbit now goes through the `SphPhase` machine instead of the CPU `Aggregate`
path. A new pure helper `live_resolution_crossing(bodies, planet_radius, eligible, tidal_fraction)`
(lib.rs, outside the wasm-only `mod app`, so it is natively tested) reports the first orbiting body
inside `accretion::resolution_distance` of the planet plus its body-centric (offset, relative
velocity) in f64 SI. `step_substep` runs it right after integration; on a crossing,
`start_live_drop_sph` names the real bodies (`earth` + `moon`; every system-world moon is an
instance of Luna), starts the same GPU relax as `start_gpu_impact` without touching the trajectory,
and records `sph_live_drop = Some(i)`. The `Assembling` arm then calls
`gpu_sph::assemble_from_relaxed_at` with the live (offset, rel_vel) read from `self.bodies` at
assembly time and Earth's spin as `spin_l.z / (0.4 M R^2)`; the declared birth path keeps
`assemble_from_relaxed_with` unchanged. The CPU swept-detect, materialization and parking passes
skip a handed-off body: its collision belongs to the particle physics now.

**Why.** One collision-resolution path at every scale. Birth imposes
a canonical approach because it must; a body already in orbit HAS a trajectory, and re-synthesizing
it would overwrite measured state. The check runs post-integration because at 118,000x one substep
(~123 s) can carry a dropped moon from outside the threshold (Earth-Luna resolve at ~8,600 km,
contact 8,108 km) to below contact; detection after the step plus the guards keeps the CPU path
from materializing debris for a body the SPH is about to own. The impactor's own spin is handed
over as 0.0: a flagged Law V IOU, deferring per-body spin angular momentum carried in the N-body
state. The retired CPU `start_birth` keeps its Aggregate route until the CPU debris path is retired.

**Verified.** New native test `a_body_crossing_its_resolution_distance_is_reported_with_its_live_geometry`
(written failing first): fires on the body inside the threshold, not the one outside, with bit-exact
body-centric geometry, and skips ineligible bodies. Full suite 331/331 green, including
`a_dropped_moon_impact_leaves_most_debris_gravitationally_bound` (the CPU physics stays pinned until
its path is retired) and the geometry contract `assemble_at_honours_a_given_live_geometry`. Native and
wasm32-unknown-unknown builds clean, no new warnings. fmt untouched (hand-edited).
## 2026-07-23: doc truth-up after the marathon: the map traces the one SPH path

**What.** Three audited drift fixes, no behaviour change. (1) docs/32 §6 no longer narrates the
retired CPU `Aggregate` debris path (`start_birth` → `build_impact_debris_scaled` → `moon_debris` →
`step_block`) as the live trace with a "second path" warning: the section now traces what `lib.rs`
actually does, the ONE GPU SPH path with two entries (the declared birth impact and the live-drop
hand-off), with every anchor re-verified against today's tree; the stale probe-path and `MatterSim`
snap mentions (the §3 matter.rs bullet and §4.7) are re-anchored to the current production callers
(`simulation.rs:317`/`:403`, the camera shell sweep). (2) CLAUDE.md's suite count is measured again:
379 run by default (was 362), 22 ignored, per-file breakdown unchanged. (3) The impactor-spin IOU in
`lib.rs` now names its wiring owner: per-body spin angular momentum in the N-body state is the
generic body's `ang_mom` vector, docs/58 item 3, being built on the generic-body arc.

**Why.** docs/32 exists so a session starts oriented; after the aggregate retirement (docs/46 rows
1 and 3) its canonical trace pointed a reader at code that no longer exists, which is exactly the
"one question, two answers" drift the map's own header warns about. A count nobody measured is a
guess (Law VII), and an IOU without an owner is a debt nobody collects.

**Verified.** Full native suite: `bash scripts/test.sh`, 379 passed, 0 failed, 22 ignored, exit 0
(2026-07-23). Ignore breakdown measured with `grep -rn '#\[ignore' crates/engine/src`:
hydrostatic.rs 9, impact.rs 8, aggregate.rs 2, gpu_gravity.rs 2, gpu_host.rs 1 (the extra grep hit
at gpu_host.rs:99 is a doc comment, not an attribute). Every new docs/32 anchor was read in today's
tree before being written; `moon_debris` and the `step_block` wiring confirmed at zero grep hits in
`lib.rs`, and `build_impact_debris_scaled` confirmed test-only in `impact.rs`.
## 2026-07-23: wasm size baseline recorded

**What.** Recorded the release wasm baseline and made it a release-checklist step (docs/03), with a
CI log line per PR. **Why.** The dev wasm had grown to 3.4 MB against a last-noted 1.3 MB with no
baseline in between, so growth was unattributable. **Verified.** Measured on this date: release
`engine_bg.wasm` 811,915 bytes raw, 322,386 bytes gzip -9 (the wire cost); dev build 3,402,186
bytes raw for reference.
## 2026-07-23 — resolution-on-demand: the moon-drop caps modern Earth (deployed); Ground port foundations (docs/39)

**What (shipped + deployed).** After the CPU Aggregate was retired (entry below), routing the moon-drop
through the *full-body* SPH particalized ALL of modern Earth — melting it to a magma ocean and under-resolving
the Moon (~50 particles) — a Law-III violation (a Moon on Earth is a SURFACE cratering event, not a whole-body
giant impact). Fixed by **resolution-on-demand** (docs/39): a small impactor (`route_bodies_to_sph`, mass
ratio `< CAP_MASS_RATIO=0.1`) resolves the impactor(s) whole + a **CAP** of the target; the target's bulk
stays an abstract body — a Gauss gravity source + a non-injecting floor the cap rests on
(`gpu_sph::set_bulk`, a new spherical-bulk mode in `sph_step.wgsl`). Birth (comparable masses) still resolves
whole-body, unchanged. `HydroBody::particalize_cap` filters a full-body seed to the impact region (real
materials/mass for free); the cap relaxes **seated on the bulk** before the shock, then `assemble_from_relaxed_n`
places it (stays) + the impactor(s) at live contact. Render: the cap case draws the target as its FULL solid
globe (blue modern Earth), and the two "SPH ⟹ proto-Earth magma" branches are gated to whole-body only.

**Why the over-ejection scare mattered.** First cut seeded the cap un-relaxed → straight to dynamics: it
threw ~everything off (Earth–Moon distance ballooned to 178,000 km) — the docs/39/3a lesson that an
un-relaxed body dumps startup non-equilibrium into the shock. Relaxing the cap on the bulk first (docs/39 39b
keystone) made it converge (7,478 → 442 km, a bound debris halo — cratering/fallback). **Deployed** to
integrity.bothead.net; PR #80.

**Ground port foundations (docs/39 surface instance — a terrestrial meteor is a moon-drop scaled down).** Two
hardest pieces built + verified, NOT yet wired into the Ground scene (it has no `GpuSph` today):
1. **Planar bulk mode** in `sph_step.wgsl` — the shared shader now serves a FLAT terrestrial floor (uniform g
   + non-injecting plane) alongside the planet sphere, data-selected by `bulk_cr.w<0` so it stays ONE shader
   (Law II). `GpuSph::set_bulk_planar`. This resolves docs/39 open-decision #4 (a patch-local f32 frame keeps
   precision at 1 m grains; a huge-R sphere loses 0.5 m ULP, a small one fudges curvature).
2. **`gpu_sph::promote_ground_cap`** — reads the engine's voxel `World` columns near the impact → real SPH
   particles in a patch-local frame, each carrying its voxel's own material/density/EOS. This IS Robin's
   "promote visual terrain filters to real matter at impact": it follows the real hills/strata/water, never a
   flat projection, so ejecta lands matter-on-matter (which is what fixes the docs/55 "crater refills" gap).
   Regolith (sand/gravel/dirt) has no Tillotson yet → basalt fallback (flagged Law-V IOU).

**Design converged with Robin (captured in memory `integrity-gpu-collision-unification`):** ejecta must land
on REAL settled matter, never an abstract terrain projection; each ejecta grain's landing is predicted from
its ballistic trajectory (the moon-drop's `detect_swept`, per grain) so its column promotes just-in-time; the
engine — not the scene — owns the topography (scene declares a *character* like "glacial plain", engine
generates the type-appropriate relief/strata/water). REMAINING for Ground: meteor→SPH, wire a `GpuSph` into
the Ground scene + the cap-impact flow + render + rig; then per-grain JIT + bake-back; and bring
`world::generate_from` up to the Laws.

**Verified — and the TEST HARNESSES (for the next session):**
- **`tools/sph-verify`** (`cd tools/sph-verify && cargo run --release`) — runs the REAL `sph_step.wgsl` on the
  box's RTX 2070 (native Vulkan) vs a CPU f64 reference. **USE IT for any shader change** — native `cargo
  check` does NOT validate WGSL. It now checks: force kernel + KDK integrator match; the SPHERICAL bulk
  boundary (a grain falls, rests on R_core, no launch); and the PLANAR bulk boundary (a grain falls under
  uniform g, rests on the flat plane, no launch). All three port failure modes (wrong-sign gravity, leaky
  floor, KE-injecting spring) are caught here before wiring.
- **`scripts/rig.sh <rig>.mjs`** on the GPU Xorg (:2, `MESA_VK_DEVICE_SELECT` for the 5060 Ti) — composited
  WebGPU screenshots. **`web/rig/moondrop_check.mjs`** (new; `PAGE=orbit.html|twomoons.html`) drives the drop
  and reads the HUD (asserts SPH resolution, no CPU "fragments"); `birth_check.mjs` for the disk. Poll the rig
  log for `RESULT:` — do NOT `pgrep -f <rigname>` in a wait loop, it self-matches and hangs. `mod app` /
  `Ground` are WASM-ONLY — build wasm (`npm run wasm`) + rig; native tests don't cover the scenes.
- **Native**: `particalize_cap` (hydrostatic.rs), `promote_ground_cap` (gpu_sph.rs), the layout-pin tests
  (`sph_params_matches_the_shader_field_for_field` — the WGSL parser learned `vec4` this session), full suite
  331 fast / 336 full.
- **Research maps** (two `Explore` subagents this session, findings folded into docs/39 + memory): the SPH +
  boundary machinery (docs/39 was already the CPU-verified design, trapped in `hydrostatic.rs` test module),
  and the Ground scene (voxel `World`, CPU-only `MatterSim`, the frame/precision wall). **docs/39** carries
  the GPU-port plan; **docs/58** the generic-body collision unification.

## 2026-07-23 — the CPU Aggregate is retired: every collision resolves on the one SPH engine (docs/58)

**What.** The moon-drop and the two-moon drop now resolve through the SAME GPU SPH engine as the birth
scene — the CPU `Aggregate` debris path is gone from the `OrbitDemo` scene. "Drop Moon(s)" de-orbits the
moon(s) and routes the planet + moon(s) through a shared `route_bodies_to_sph` → `begin_sph_relax` → the
Relaxing→Approaching→Assembling→Dynamics machine (N bodies at once: a two-moon world resolves all three in
one collision). A collision the cheap orbital phase *detects* — braking a moon until it crashes — routes
the same way: `step_substep` records the colliding set and `advance` hands it to the SPH engine after the
substep loop. There is no second resolution path; `step_substep` is now pure ballistics + detection.

**The bug the rig caught (why it wasn't already one engine).** Birth handed off to SPH at the *tidal*
resolution distance — for Mars-mass Theia ~17,700 km, comfortably outside contact (9,551 km). The Moon is
~9× lighter, so its 1%-tidal distance is only ~8,600 km — *inside* contact. A dropped Moon therefore
reached contact *before* the SPH handoff, the still-present CPU swept detector tripped, and the collision
resolved TWICE — CPU Aggregate debris AND GPU SPH on the same drop (the HUD showed both "1536 fragments …"
and "GPU impact · disk"). Fixed two ways: (1) the handoff distance is now
`accretion::resolution_distance(…).max(contact)` — matter resolves when *either* tides dominate *or* the
surfaces meet, whichever comes first (heavy Theia still resolves early; the light Moon resolves at
contact); (2) `step_substep` no longer resolves anything itself — while a resolve is live it returns after
integrating, and otherwise it detects-and-routes.

**Deleted from the scene** (~450 lines): the `moon_debris: Option<Aggregate>` field and its
`build_impact_debris_scaled` materialisation, the O(N²) CPU debris advance (self-gravity, boundary
shear/mirror + spin reaction, J2, tidal kicks, drain/demotion), the CPU debris render pass, the
`FrameSnap` debris arrays, `start_birth`/`birth_mode` (birth uses `start_gpu_impact`),
`debris_count`/`hole_radius`/`cap_extent`/`crater_heal_m3`/`debris_rate_mul`/`debris_frame_dt`, and the CPU
crater bowl. The crater-*wall* render (used by the live SPH crater) now sizes from the actual `crater_r`
rather than the retired healing formula. **Kept:** the `aggregate::Aggregate` *module* — a general
self-gravitating particle solver still used by `atmosphere.rs` and `impact.rs`'s measurement tests.
Retiring it from the *scene* is the "one collision engine" goal; the struct has other consumers, so
`build_impact_debris_scaled` is now test-only rather than deleted.

**Verified.** Full native suite green — **335 passed, 21 skipped**. Headless GPU rigs on the 5060 Ti
(build 20260723.072150, `scripts/rig.sh moondrop_check.mjs` / `birth_check.mjs`):
- **one-moon drop** (`orbit.html`) and **two-moon drop** (`twomoons.html`): both resolve as PURE SPH — the
  HUD shows only "GPU impact · disk …", ZERO "N fragments" (the CPU signature), no panics, no
  `is not a function`. A radial drop *merges* (disk ≈ 0), which is honest: near-zero impact parameter
  carries ~no angular momentum, unlike birth's grazing hit.
- **birth** (`birth.html`): still lofts a proto-lunar disk (→ 0.31 M☾, Earth-fraction → 60%) and accretes a
  0.06 M☾ moon — the shared advance/snapshot/render code survived the deletion intact.

## 2026-07-22 — collision unification groundwork: the moon-drop is a giant impact, and the EOS moves to the catalogue

**Context — where "one collision path" actually stands.** The goal is one collision-resolution path on the
GPU at every scale (Theia and a de-orbiting Moon are the same mechanic, Robin's law), retiring the CPU
`Aggregate`. Measuring the code corrected two stale notes: **collision DETECTION is already unified** —
`OrbitDemo::step_substep` calls `interaction::detect_swept` and its own swept-CCD loops are gone (landed
in #75; docs/57 finding #3's "STILL OPEN" was stale). What remains is **resolution**: the moon-drop still
materialises a CPU `Aggregate` debris cloud at surface contact (`build_impact_debris_scaled`, the O(N²)
Barnes-Hut bottleneck), while the birth scene resolves deformable SPH bodies on the GPU at the tidal
distance. Two answers to "a body hit Earth."

**Keystone proven (native TDD).** A Moon striking Earth is a *giant impact*, not a surface crater — the
same `gpu_sph` assembly builds it with nothing swapped but the two bodies. `a_moon_drop_builds_and_strikes_through_the_same_assembly`:
an `ImpactDef` naming the real Earth and Moon builds two bodies of the right size/proportion, the assembled
geometry strikes, and the reduced-mass impact energy is **~28× the Moon's gravitational binding** — the
`ResolveBodies` regime (docs/46 §1), so routing the moon-drop onto SPH is correct physics, and the CPU
`Aggregate` is what gets retired. **Flagged IOU pinned as a test:** SPH mass is seeded from Tillotson
*reference* densities, not the compressed PREM densities, so the initial SPH Earth is ~64% of real mass
(compression must emerge during relax).

**Tillotson EOS parameters moved to `data/materials.json` (Robin's directive; closes the follow-up flagged
at `eos.rs:109`).** The condensed-matter EOS parameters lived as constants in `eos.rs`; they now live in a
`tillotson` block in the catalogue, and `eos::Tillotson` reads them via `materials::tillotson_block` (a
cached `catalogue()`). A world is a world is a world — one place to improve a material improves every
scene. Each block carries its own `status` (`verified`/`partial`/`provisional`) and `source`, so the
provenance moved *with* the numbers and is queryable, not buried in a comment: basalt verified (Benz &
Asphaug 1999), iron's compressed branch verified (Wissing & Hobbs 2020) with a provisional vapor branch,
granite and peridotite (a dunite analog) provisional.

**Sourcing then caught a real bug** ("go fetch it", Robin). The provisional peridotite set was a
mistranscribed Marinova 2011 olivine fit — `B` was 10× too stiff (4.9e11 vs 49 GPa) and `E0` 10× too low
(5.5e7 vs 550 MJ/kg), which is exactly the "differentiated body puffed up" symptom `eos.rs` had flagged.
Corrected to the genuine Marinova set. Also added the sourced **water ice** (Benz & Asphaug 1999, verbatim,
`verified`) and **water** (SWIFT/Melosh planetary-SPH set, cross-checked). The eos tests now iterate the
catalogue, so **every** Tillotson material — including the new sets — is validated against the same
bulk-modulus / sound-speed / monotone-compression / vapor-continuity invariants automatically. A material
given a block becomes available through `Tillotson::for_material`. (Open: the primary Melosh 1989 book was
not readable online, so granite and the iron vapor branch stay `provisional`/`partial`; the olivine set is
single-source via the Stewart-group pyKO code. Peridotite is not yet used in body-building — basalt is the
mantle there — so this is a correctness fix ahead of the layered-Earth SPH work, with no scene changed.)

**Verified.** Values byte-identical to the former constants ⇒ no physics change: full suite **328/328 (+2
new tests)**, including the slow giant-impact integration tests (`theia`, `birth_scene`,
`dropped_moon_impact`) that exercise the EOS hardest. A new pin test (`tillotson_parameters_are_read_from_the_material_catalogue`)
guards against a silent JSON typo. fmt untouched (hand-edited).

**Geometry fork SETTLED** (Robin: *"use the real live trajectory, but inside the engine, never in scene
definition"*). Birth and a de-orbiting Moon are different scenarios, not one question with two answers: birth
is a *declared experiment* whose canonical approach (`v_esc 1.15`, grazing `b`, proto-Earth spin) must be
IMPOSED — free-fall from rest gives the wrong one — while a Moon already in orbit has a real N-body
trajectory whose live `(offset, relative-velocity, spin)` *is* the geometry; re-synthesizing it would
overwrite measured state (Law VII) and inject proto-Earth's spin into a modern Earth (Law V). Resolved with
ONE engine primitive `gpu_sph::assemble_from_relaxed_at(particles, target_spin, impactor_offset,
impactor_vel, impactor_spin)`; `assemble_from_relaxed_with(def)` now computes the *canonical* geometry from
the world file and delegates (birth **byte-identical** — the slow `theia`/`birth_scene`/`provenance` tests
confirm). The geometry is the ENGINE's to compute from the bodies it holds; no scene declares it. A native
test pins the live-placement path.

**Next.** Wire the moon-drop: when the engine detects an orbiting body crossing its `resolution_distance`,
enter the SphPhase machine and call `assemble_from_relaxed_at` with the live `(offset, vel, spin)` from
`self.bodies`, then delete `moon_debris: Aggregate` + `build_impact_debris_scaled` (ledger rows 1/3/10).

## 2026-07-21 — the ground scene was an abstraction; the physics corrections, and the real target (ledger row 16)

**Robin's review, and it was right on every count.** The ground scene I shipped was *"a cube of ground
that inexplicably has earth normal gravity while being suspended in space"*. Four violations, all of
principles already written down in `CLAUDE.md`, docs, and memory:

1. **A second physics path.** Grains ran on `MatterSim::step`, the CPU *settle-only* stepper with no
   grain-grain contact. I documented that as a "known gap" and shipped it. It is not a gap; it is a
   second answer to how matter interacts, which is exactly what docs/46 exists to forbid — and
   straight-up-straight-down ejecta is not a thing in an impact.
2. **Declared gravity.** `"gravity_ms2": 9.81` in the world file. The scene I had DELETED derived it from
   `planet::earth()` — *"no magic 9.81"* — and I replaced that with the fudge.
3. **No planet.** A 96 m cube in vacuum wearing Earth's number.
4. **Resolved the whole patch**, ignoring resolution-by-necessity — the thing I had spent the week
   building.

**The failure mode, named so it is checkable:** I optimised for *something visible shipped* over *the
physics is right*, which inverts Law 1 (physics is the product) and Law 6 (physics drives the render,
never the reverse). The information was not missing — I had edited those very docs that day.

**Corrected this session (all tested):**
- **Gravity EMERGES.** `gravity_ms2` is deleted from the schema; a ground world names its `planet` and
  `g = GM/R²` follows from that body's real layered mass. An unknown planet is refused, not defaulted.
- **A meteor is MATTER you throw.** `Simulation::throw_meteor(Meteor { pos, vel, mass_kg, material })`.
  The ENGINE flies it under the planet's gravity and lands it; impact energy is ½mv² of the matter that
  actually arrives. The old `drop_meteor(energy_j)` — an abstract number, at a site the host computed —
  was a dial wearing a physics coat. Tests assert 8× mass and 4× speed each dig more, and that the
  caller never computes an impact site.
- **A double-count fixed**: `created_total` read 45,380 for 22,690 grains because the meteor's excavation
  was counted twice. A matter-accounting number that lies is worse than none, since its whole job is
  catching lost matter. Guarded by test.
- One test had been asserting behaviour in a *zero-gravity* world; it is now a real ballistic arc.

**★ CAN YOU CIRCUMNAVIGATE? No — and the measurement is the point.** The patch is 96 m across =
**0.00024% of Earth's circumference**. Walk ~48 m and you reach the edge of the world. `planet::earth()`
is referenced only for gravity and air pressure: a *number supplier*, not a *place*.

**The real target — and it was ALREADY WRITTEN DOWN.** docs/23 is the north star (*"place a metal ball
on the Earth's surface, de-orbit the Moon into that spot, then zoom from the celestial view down to the
ball"* — one Earth, everything is matter, no bespoke objects), with docs/13 (scale-relative) and docs/43
(worlds as data). I started writing a NEW doc restating it, which is precisely the déjà-vu failure
CLAUDE.md warns about; deleted. Ledger row 16 records only the measured VIOLATION. In Robin's words: *"Terra should occur naturally from
definitions of material, biomes, etc… a fully materialized object, reusable between scenes. Then this
scene would simply be using that planet/solar system… Being that close to the ground should make the
detailed texture generate naturally (scaling based on viewable area)… We do the physics and everything
else comes for free if we define the world and the objects within it well (externally)."* Plus: layer
properties (heat, pressure) DERIVED from the materials and layer volumes — *"most of this is immaterial
unless something like Theia comes along, which is why we worked so hard on Theia."*

**Every piece already exists** (`LayeredBody`, Terra's rasters, `ResolutionController::camera_grain_radius`,
`hydrostatic`/`eos`, the GPU granular container). They have never been COMPOSED into one Earth every scene
shares. That is composition, not construction, and it is the next work — no new design doc required.

## 2026-07-21 — the ground scene: a world you can look at, built from a file (docs/55)

**What.** `/ground.html` → `Ground` → `/worlds/ground/world.json`. The first thing since terrain's
deletion that a person can actually look at, and **every number about the world is in the file**: patch
size, relief octaves, sea level, the material column (sand → gravel → dirt → basalt → granite), camera
altitude, gravity, grain size. The scene contributes a camera rig, a meteor button and three passes.

**Verified in a real browser** on the 5060 Ti: renders (189 kB canvas crop vs a 1.9 kB blank control),
**284 fps**, zero page errors; a meteor resolves ~20,000 grains which settle back into the ground.

**Three things earned, each caught by the rig:**
- **The texture is the material** — 512² mip-mapped, synthesized from each material's CITED optical
  properties. No image assets. The sand you see is the database row the physics reads.
- **The sky is derived, not painted** — `rayleigh_tau` from `planet::earth()`'s emergent surface pressure.
  The first cut guessed `tau` and `SUN_GAIN = 1.0` and rendered a **BLACK SKY**; one rigshot showed it.
- **The camera is MATTER** (Robin reminded me it is canonical, and I had broken it). A transparent shell
  on the SAME `granular::terrain_contact_resolve` every grain obeys — contact and slide, never excavation.
  My first cut was `eye.y = eye.y.max(ground + h)`: exactly the clamp fudge that principle retired, which
  exempts the camera from the world's rules and only ever pushes UP, so a camera driven into a steep face
  pops through it. Shell half-extent (0.35 m) ≥ near-clip (0.2 m) — that is what stops the FRUSTUM
  crossing the surface — and the sweep from last frame stops a fast camera tunnelling the skin. **The rig
  proposes, physics disposes.**

**★ NOT DONE, and it is the interesting part: the crater does not persist.** Drop a meteor, get a real
crater with thousands of grains — and seconds later the ground is exactly as it was. Measured headlessly:
20,373 grains created, 20,345 returned to voxels, 28 in flight, **0 LOST**, voxels 643,269 → 663,614
against a pristine 663,642. **Matter is perfectly conserved, and that is precisely why it refills**: the
ejecta falls straight back into the hole it came from.

**Root cause, already recorded in docs/32 §4:** `MatterSim::step` is the CPU *settle-only* stepper — "no
grain-grain contact on CPU". Grains cannot push each other outward, so there is no ejecta blanket. The GPU
granular path (`particle_step.wgsl` + `gpu_particles`) DOES have grain-grain contact and is what produced
the measured local blanket (JOURNAL 2026-07-19). **The next increment is one thing that pays off twice:**
step this scene's grains through the GPU container — a real blanket, therefore a crater that persists
(Robin's "becoming part of a bump map"), AND the visible consumer `gpu_particles` has lacked since
terrain was deleted.

**Verified.** 259/259 native + 19 skipped, wasm and `tsc` clean, browser rig-verified.

## 2026-07-21 — the surface itself becomes data (docs/54)

**What.** `world_def::GroundSurface` declares what the ground IS — patch size, the fbm octaves, the
relief band, sea level, and the material strata — and `world::generate_from` builds it. All of that was
hardcoded in `world::generate`, so every ground world was the same 96 m patch with the same hills.

**The laws did not move.** How strata stack, how water fills air below the datum, how the heightfield is
sampled, what makes a column collapse: still the engine's. The file says what this ground IS; the engine
says how ground BEHAVES.

**Named `GroundSurface`, not `TerrainDef`** — Robin queried "why are you working on TERRAIN?" and the
name was the reason. The terrain SCENE is deleted and staying deleted; this is the engine's voxel ground,
a core capability that scene merely used. When terrain is rebuilt it should be a ground DEFINITION, not a
scene struct. Also distinct from `world_def::Surface` (Terra's planet-scale raster data) — a merge to
make deliberately one day, not to let happen by accident.

**Output-neutral, and proven to drive.** `surface_defaults_reproduce_the_hardcoded_world` asserts the
declared defaults give a VOXEL-IDENTICAL world to `generate`; `changing_the_declared_surface_changes_the_world`
asserts the converse per dial — size, amplitude (zero ⇒ provably flat), octaves, sea level (zero ⇒ no
water), skin material (what you stand on). Without the second the schema could be decoration.

**★ A mistyped key is now an error.** serde ignores unknown fields by default, so `"terrian"` would
silently leave the value at its default and run a DIFFERENT world than the file describes, with nothing
to see. This bit for real during the `terrain` → `surface` rename: the test went red ONLY because it
asserted the world's SHAPE, not because the key was wrong. `deny_unknown_fields` now covers the ground
and impact schema, with a test naming the failure. For a data-driven engine this is the same hollow-green
shape as a test that asserts a state change without asserting the effect.

**★ Matter accounting, and what two worlds showed.** `run-definition` now reports every grain, because
"0 particles" is ambiguous — de-resolution (conserved) and the off-patch cull (deleted) look identical.

| definition | patch | created | returned | in flight | lost |
|---|---|---|---|---|---|
| `ejecta-ground` | 96 m | 260 | 260 | 0 | **0 (0.0%)** |
| `small-island` | 48 m | 6,328 | 3,535 | 971 | **1,822 (28.8%)** |

The big patch conserves matter exactly. The half-size one loses ~29%: an energetic impact throws ejecta
past the domain boundary. That is ledger row 9, previously measured at ~2% on the big patch and never at
a small one — a property of the DOMAIN, not the physics, and now measurable per definition. A user of
this engine can check whether their world conserves matter before trusting it.

**Verified.** 259/259 native (+4) + 19 skipped; native and wasm clean; **both browser scenes rig-verified
rendering** (birth 66,469 B, terra 64,003 B) — the browser stays first-class (Robin: wasm "is the
beginning"; native is the longer game, not a migration).

**Still headless.** Nothing here is visible, and a game engine earns users by being seen working. Next: a
browser scene that renders a ground world FROM a definition — which also gives the granular GPU pipeline
a visible consumer again (today it is reachable only from `GpuProbe`). That is the rebuilt terrain.

## 2026-07-21 — the engine driven by a definition; ledger row 15 CLOSED (docs/53)

**What.** `crate::simulation::Simulation` — the engine builds a world, applies declared matter events
through the SHARED primitives, and steps, with **no scene struct, no canvas and no `wasm_bindgen`**. A
new `"ground"` world type declares the observer, the gravity analytic effects fall under, and events
(`impact` → the shared `MatterSim::impact`; `ejecta` → an analytic `Effect` for the docs/49 hand-off).
`crates/engine/src/bin/run-definition.rs` runs one headlessly from a file.

**Why — the failure it repairs.** Deleting terrain left `MatterSim`, `ResolutionField` and the voxel
`World` with ZERO production consumers while every test kept passing (ledger row 15). Robin's diagnosis
was structural: *"this is why we make the engine standalone, with external definitions."* Capability was
reachable only THROUGH a scene, so the scene's deletion took it down. The repair is not another scene —
that reintroduces the coupling — it is making the consumer a FILE.

**Verified end to end, from `definitions/ejecta-ground.json`:**

    after load : 3 particles, 1 analytic effect(s), 644190 solid voxels
    step  130  : 1 effect(s) entered view and materialised -> 257 particles
    after 300  : 0 particles, 0 still analytic, 1 resolved in total
    matter     : 644190 -> 644450 solid voxels (+260)

**+260 is exactly the 257 materialised grains plus the 3 impact particles** — every grain de-resolved
back into the world, none lost. The runner reports that voxel delta on purpose: "0 particles" is
ambiguous, because de-resolution (matter conserved) and the off-world cull in `matter::step` (matter
deleted) look identical from the particle count, and only one of them is honest.

**★ The near-miss, which is the lesson.** The first run printed `materialised -> 0 particles` **with a
green suite**, because the tests asserted an effect RESOLVED — a state change — and never that it
PRODUCED MATTER. The cause was my own definition: `view_radius_m: 150` exceeds the 96 m patch bound
(`bound = max(w,h,d)`), so ~250 grains spawned outside the world and were culled in the same step. Not an
engine bug — but the tests could not tell the difference, which is exactly the hollow-green failure this
module exists to prevent. The assertion now exists and is **proven able to fail**: moving the resolve
point back outside the patch yields `must PRODUCE MATTER; got 0 particles`. Two earlier attempts to
falsify it passed, because both resolve points were still INSIDE the 96 m bound — the guard was only
trustworthy once a genuinely-outside case went red.

**Ledger row 15 CLOSED.** `simulation.rs` is production code and references `MatterSim` 8×,
`ResolutionField` 4×, `world::generate` 1×.

**Verified.** 255/255 native (+5) + 19 skipped; native and wasm clean.

**Still not standalone:** the two remaining scenes are `#[wasm_bindgen]` structs in the crate (row 14's
other half); there is no native host (no window/surface/input, docs/52); and the ground world's SURFACE
is still the procedural patch — the definition declares events, camera and gravity, not the terrain.

## 2026-07-21 — the engine holds a GPU with no browser (docs/52, standalone increment 1)

**What.** `gpu_host::GpuHost::headless()` — the engine acquires a real GPU with no canvas, no surface and
no page, and the crate's GPU backends are now chosen by TARGET rather than pinned to WebGPU.

**Why this was blocked.** Every path to the engine's GPU code ran through a `#[wasm_bindgen]` scene
handed an `HtmlCanvasElement`: "the engine" and "the browser page" were the same object. And `wgpu` was
pinned crate-wide to `features = ["webgpu", "wgsl"]` — a backend that exists only inside a browser — so
the engine could COMPILE natively (the docs/50 lifts proved that) but could never RUN.

**Why target tables, not a cargo feature.** **Features unify across a build graph; targets do not.** A
`native-gpu` feature could leak a native backend into the browser build through unification — the exact
hazard that pushed `tools/gpu-verify` into its own separate workspace. With
`[target.'cfg(target_arch = "wasm32")']` / `[target.'cfg(not(...))']` it cannot happen: nothing building
for wasm32 can see the native table. (There is no `vulkan` cargo feature in wgpu 24 — on Linux it is
enabled by platform; `default = ["wgsl","dx12","metal","webgpu"]`.)

**Adapter choice is explicit and refuses to guess.** `PowerPreference::HighPerformance` cannot
discriminate between two discrete GPUs — it takes whichever enumerates first — and cards three
generations apart report byte-identical limits, so there is nothing to auto-select on. CPU adapters are
filtered out (they "work", then report software timings as hardware). With several GPUs and no hint it
returns an error instead of picking, which is the lesson `tools/gpu-verify` already paid for. On this box
the guard fired correctly before `INTEGRITY_ADAPTER=5060` was given.

**Verified ON HARDWARE, which is the whole point.** "It builds for a native target" proves nothing —
wgpu's types exist without a backend, which is exactly why the docs/50 lifts compiled natively all along
while still being unable to run. The test acquires a real device and then compiles and creates a pipeline
from the SHIPPING `shaders/sph_step.wgsl`:

    adapter: NVIDIA GeForce RTX 5060 Ti (DiscreteGpu, Vulkan)
    test gpu_host::tests::the_engine_can_run_its_own_shader_with_no_browser ... ok

`#[ignore]`d so a GPU-less machine does not fail the suite.

**The browser is unaffected** — the constraint this could have broken. 250/250 native + 19 skipped, wasm
check clean, `wasm-pack` clean, and both remaining scenes rig-verified rendering (birth 67,219 B, terra
64,003 B, against the 1,883 B blank-page control).

**Honest scope.** The engine can now HOLD a GPU on its own; it is not yet standalone. The scenes are
still `#[wasm_bindgen]` structs inside the crate (a new KIND of scene is still an engine edit — ledger
row 14), there is no native host (no window, no surface, no input — headless compute only), and the
systems terrain orphaned are still orphaned (row 15). The standalone shape is what will let those be
re-consumed by a DEFINITION rather than by a scene struct.

## 2026-07-21 — the last code-path scene becomes data; what terrain took with it (docs/51)

**What.** "Birth of the Moon" — the only scene whose setup was still compiled in — now loads its initial
conditions from `/worlds/birth/world.json` through a new `"impact"` world type (`world_def::ImpactDef`).
Moved into data: both bodies' core/surface radii, softening and core-resolution factor; approach speed as
a multiple of mutual escape speed; start separation; impact parameter; proto-target spin; relax
separation. **The laws did not move** — Tillotson, SPH, self-gravity and the leapfrog stay in the engine
and are not selectable from a file. Initial conditions and a few dials, exactly docs/43's line.

**Measured before designing, and it narrowed the claim.** docs/46 row 14 said "a scene is engine code".
Checking the pages showed `orbit.html` and `twomoons.html` are the SAME script and SAME `OrbitDemo`
differing only by `data-world=…`, and `terra.html` likewise — instances were already data. Exactly ONE
scene was compiled in. The row is now narrowed to what was actually true.

**Output-neutral by construction:** every field's serde default IS the constant it replaced, and a test
asserts each against the literal value as it stood in `gpu_sph`. A world that omits `impact`, or fails to
fetch, is bit-identical to the old path. A second test asserts the opposite direction — a smaller
declared impactor builds fewer particles — so the file cannot be decoration.

**A real bug the rig caught, that a weaker check would have passed.** `orbit.ts` handed ANY `data-world`
to `load_world`, which requires a `bodies[]` array; an `"impact"` world has none, so birth died with
*"system world is missing a `bodies` array"* and rendered NOTHING — while the world file fetched
successfully and no JS error was raised. A check that only asked "did the file load?" would have been
green. Routing is now by world type.

**★ What deleting terrain took with it (Robin asked; the answer is worse than expected).** Terrain was
the ONLY production consumer of three built-and-verified systems. Measured by grep after the deletion:
`matter::MatterSim` and `resolution::ResolutionField` now have **ZERO references anywhere in `lib.rs`**;
all six `world::generate` calls are inside `#[cfg(test)]`; the granular GPU pipeline is reachable only
from `GpuProbe`, a compute-only diagnostic with no canvas. **Every test still passes**, which is exactly
why it is easy to miss. This is docs/48's wiring pattern at its sharpest — physics wired into one place,
and then that place deleted. Recorded as ledger row 15. It is NOT an argument to restore terrain; it is a
requirement on the next scene: re-consume them, or delete them rather than leave green unreachable code.

**Verified.** 250/250 native (+3) + 18 skipped; native, wasm and `tsc` clean. Birth rig-verified: the
world file is FETCHED (`birth/world.json`), accepted with no errors, and the scene renders (64,891 B
against the 1,883 B blank-page control) with the correct HUD.

## 2026-07-21 — one GPU particle container; terrain deleted (docs/50)

**What.** `crate::gpu_store::ParticleStore<T>` — the ONE GPU particle container: the storage buffer,
capacity/count bookkeeping, clamp-to-capacity `append`/`replace`, and the two-phase asynchronous
read-back. Both pipelines now use it. `GpuSph` and `GpuParticles` each lost a private buffer, their own
`capacity`/`count`, and their own read-back; both `begin_readback` bodies are three-line delegations.
**Their solvers are untouched** — docs/46 §1 sanctions that separation (stiff granular contacts want a
semi-implicit integrator, self-gravitating SPH a symplectic leapfrog; the physics differs, so the
numerics do). What was never physics is the allocator.

**The evidence it was duplication and not coincidence:** the read-backs were byte-for-byte identical
apart from the element type and a debug label, and the SAME latent defect — an `Rc<Cell<bool>>` in the
`map_async` callback, which compiles only for wasm — had to be found and fixed **twice, once in each
file**, on 2026-07-20. One answer written down twice.

**Terrain is deleted.** Robin: *"I want that old model GONE"* — the first scene designed, superseded, and
she had asked for it in an earlier session. 1,516 lines out of `lib.rs` (5,548 → 3,794), 25 terrain-only
rigs, the page, the vite entry, the nav link. `OrbitDemo` and `Terra` remain.

**★ The finding that matters more than the refactor.** Deleting one scene required SURGERY ON THE ENGINE:
1,516 lines inside `crates/engine/src/lib.rs`, a symbol out of the crate's public API, and a build entry
point. Robin's standing requirement is the opposite — *"scenes should have object definitions, assembly
definitions, coordinates, etc… but should not require special mods of the engine itself."* So the cost of
this deletion IS the measurement of how far scenes are from disposable, and `OrbitDemo`/`Terra` are the
same shape. Recorded as **docs/46 ledger row 14**. Until a scene is a description the engine loads,
"delete the scene" will keep meaning "edit the engine".

**Verified.** 247/247 native (+3 for the store's pure clamping arithmetic) + 18 skipped; native and wasm
clean. **The container is exercised, not just compiled:** the granular debris lifecycle end to end —
meteor → `debris 0 → 3,670` (append) → 2,968 → 1,941 → 940 → 201 → 111 as grains settle and de-resolve
(read-back) — and the SPH side rendering Theia as individual particles from the physics buffer the store
now owns, `VERTEX` usage preserved. After deletion, both remaining scenes render with zero page errors
against the blank-page control (birth 66,934 B, terra 64,003 B, control 1,883 B).

**The clamping arithmetic is pure and natively tested** (`append_span`/`replace_span`): wgpu here has the
`webgpu` backend only so a `ParticleStore` cannot be instantiated off-browser, but the capacity boundary
can — and that is where the silent bug lives, since an off-by-one drops particles with no error, which is
matter vanishing that no rendering check would catch. One test asserts successive appends tile with no
gap or overlap.

**Harness, three real bugs fixed — all of which had me hand-firing commands instead of fixing the tool
(Robin: "firing things manually repeatedly is an anti-pattern").**
1. `pkill -f "vite --port 5173"` **matches the shell running it**, because the pattern is on that shell's
   own command line. It killed the caller (exit 144) repeatedly before it was spotted. The kill now lives
   inside `rig.sh` behind `--restart`/`--stop`, per Robin's suggestion, so the pattern is never typed by
   hand, and a `[v]ite` bracket stops it matching its own text even there.
2. The spawned dev server inherited the script's stdout, so `rig.sh … | tail` never saw EOF and the whole
   command hung long after the rig finished. Now fully detached (own session, all three fds redirected).
3. `npx --prefix` resolves the package but leaves the working directory, so vite answered **404 for every
   page while logging "ready"**; and vite 6 rejects `--root`. The root is now passed positionally.

## 2026-07-21 — CORRECTION: the "~1 fps" was the harness, not the engine (and one real fix)

**Retraction first.** Yesterday's entry below reported terrain and birth delivering **~1 fps** as an
engine finding, cross-checked against "terra runs 46–62 fps in the same session, so it is workload, not
capture overhead". **Both halves were wrong.**

- **Chromium paces this headless-Xorg setup at exactly 1 Hz** — 1003.1 ms median, p95 1003.2, max
  1003.3. A spread of 0.2 ms over a full run is a timer, not a workload. `--disable-frame-rate-limit`
  removes it. Everything measured without that flag was capped at 1 fps regardless of the engine.
- **The cross-check was invalid.** Terra's HUD computes fps as an EMA seeded from the first frame's
  `dt`; at 1 fps it decays ~10% per frame, so it reads ~50 while the truth is 1. Held for 90 s it goes
  **97 → 21 → 2 → 1 → 1**. Terra was at 1 fps too — the number that "ruled out" throttling was the
  throttle, still converging.

**What actually caught it:** an INDEPENDENT empty `requestAnimationFrame` loop injected alongside the
app. It measured 1.0 fps on ALL THREE scenes *including* the one whose HUD simultaneously claimed 49.
An empty loop cannot be workload-bound, so the browser had to be the pacer. Two app-level metrics
agreeing (HUD + video) had felt like confirmation; they shared the same hidden cause.

**True rates, 5060 Ti, flag on:** terra **354 fps** (2.0 ms), birth **52 fps** (5.3 ms), terrain
**17.9 fps** (55.6 ms). So there IS a real cost in terrain — 30× the others — just not 1 fps.

**The real fix that came out of it (core, not scene).** A CDP profile of terrain with frames uncapped:
**98% CPU-bound**, top cost `World::surface_top_voxel` at **16.7%**. It scanned a column top-down —
O(height), walking every air voxel above the surface — on every call, and `surface_bilinear_grad` asks
for FOUR columns per query, per probe particle, per substep. It is now an O(1) read from a cached
`tops` raster. **Invalidation is by recompute, not by reasoning:** every `set_voxel` rescans that one
column. Writes are rare, reads are per-particle-per-substep, and the dumb version cannot get water
(excluded from `is_solid`), demotion, or mid-column removal subtly wrong — a stale top is a SILENT
physics error, bodies resting at the wrong height, not a crash.

**Measured: terrain 55.6 ms → 41.8 ms/frame, 17.9 → 23.4 fps (1.31×).** `surface_top_voxel` left the
top-14 profile entirely. 244/244 native, wasm clean, output-neutral.

**Verified, including that the guard can fail:** `tops_match_a_fresh_scan_after_every_kind_of_mutation`
compares the whole raster against a fresh scan after digging, depositing above the surface, removing a
MID-column voxel (the case incremental logic gets wrong), demotion, and excavating a column to nothing;
`the_top_cache_guard_detects_staleness` writes voxels directly to prove the comparison sees it. That
second test immediately caught a real bypass — a mesher test wrote `voxels[i] = 0` directly and went RED
until it was moved onto `set_voxel`.

**Stopped short, deliberately.** The next costs are all the terrain scene's probe substep loop
(`value_noise` 22.3%, `Aggregate::accelerations_masked` 11.7%, `break_overstrained_bonds` 10.3%) —
`collide_probe_with_terrain` runs per substep with `stable_substeps` up to 256. Robin: that scene is
slated for DELETION (docs/33 "converge → THEN delete terrain") and "has a lot of craziness". Optimising
it would be work thrown away. The column-top cache was kept because it lives in `world.rs` — the core —
not in the scene.

**Harness hardening so this cannot recur:** `web/rig/_launch.mjs` is now the ONE place Chromium flags
live and all 67 rigs launch through it; `scripts/rig.sh` is one command that starts Xorg, rebuilds wasm
only when Rust/WGSL changed, and — the load-bearing part — **forces a vite restart whenever wasm was
rebuilt**, because vite computes the wasm cache-busting stamp at startup and a stale server serves old
bytes while looking green. Both traps are now structural, not README warnings.

## 2026-07-20 — video rig: measuring smoothness and continuity, not just "does it draw"

**What.** `scripts/rigvideo.sh` records the composited X framebuffer losslessly while a rig drives a
scene; `scripts/analyze_motion.py` reports FREEZE (% of frame-pairs where nothing moved, worst
continuous hitch, and the **delivered fps** that implies), JUMPS (deltas far above the run's own median
— a pop, teleport or flash) and STEADINESS. `--selftest` builds a known-smooth, a known-stuttery
(1-in-3) and a known-frozen clip and prints the same metrics for each, so a real number is read against
controls rather than against intuition.

**Why.** Every check the engine had was a single frame. Stutter, a freeze, popping and a teleport are
properties of the SEQUENCE — structurally invisible to a screenshot. Robin asked for it as the next
harness step, and it is the first tool here that can fail a scene that screenshots perfectly.

**The first real measurement, and it is not flattering.** Terrain (with a meteor) and birth both deliver
**~1.0 fps** — 96.6% of captured frame-pairs are identical, with a worst continuous hitch of **7.5 s**
on terrain. Each scene's own HUD counter independently agrees (~1 fps), and Terra rendered 46–62 fps in
the SAME session on the same display, so this is workload cost, not capture overhead or window
throttling. The meteor impact shows up exactly as it should: 4 jump frames, max/median **637×**.

**Three calibration errors, all caught by measurement, all the same shape.**
1. The freeze threshold was guessed at 0.35 and called **44.5% of a known-smooth clip frozen**. Swept
   against the controls instead: a flat plateau over [0.02, 0.2] scores smooth 0.0% and 1-in-3 67.2%.
2. Worse, the STATISTIC was wrong. A frame-wide mean at 160×100 is dominated by static UI, so a small
   moving object (Theia is a few pixels) reads as frozen — it scored the birth capture 99.7% frozen
   against a true 96.5%. Switched to "did ANY pixel change by more than `pix_eps`", cross-checked at
   640×400 where max-delta, %-pixels-changed and mean-delta agree (96.5/96.5/96.2%).
3. Then the controls themselves were contaminated: under H.264 a *duplicated* frame comes back altered
   by up to ~8 grey levels, so a peak statistic cannot see it as a duplicate, and no threshold satisfied
   both controls at once (at `pix_eps` 1 the 1-in-3 clip read 1.7% frozen; at 8 the smooth clip read
   21.8%). Recording **losslessly** removed the confound and the plateau appeared instantly: [1,4] gives
   0.0% and 67.2%, exactly right. `pix_eps=3`.

That is the pattern of the whole day: the number you did not derive is a guess, and here it was wrong
three times in a row while looking perfectly plausible each time.

**Also fixed:** `rigvideo.sh` died with exit 141 and no output, because `awk '{print; exit}'` closes the
pipe early, `xdpyinfo` takes SIGPIPE and `set -o pipefail` kills the script. It is a race, so it worked
the first time it was run.

**Verified.** Controls read 0.0% / 67.2% / 100.0% frozen and 30.0 / 9.8 / 0.0 delivered fps, with zero
false-positive jumps. Real captures produce the numbers above, cross-validated against both the in-app
HUD and a fast scene in the same session.

## 2026-07-20 — the render scaffolding lifted out of `mod app` (docs/33)

**What.** `crate::render` — `GpuMesh`, `UniformSlot`, `Camera`, `Uniforms`/`SkyUniforms`/`InstanceRaw`,
`DEPTH_FORMAT` and the generic helpers (`draw`, `uniform_entry`, `make_uniform_buffer`, `upload_mesh`,
`create_depth_view`, `make_buffer`, `make_dynamic_mesh`) are now one scene-agnostic module. Third and
last of the mechanical lifts (`gpu_sph` → `gpu_particles` → here). lib.rs 5,684 → 5,548; 242/242, wasm
clean, warnings unchanged.

**Why.** All three scenes use these identically — they were never terrain code, space-band code or globe
code. They sat in `#[cfg(target_arch = "wasm32")] mod app` only because the scene structs do, which put
shared scaffolding out of reach of every native build and made "which of these 5,000 lines is actually
scene-specific?" unanswerable without reading them. What is left in `mod app` after this is the part
that genuinely is per-scene: the scene structs, and the pipeline builders that each name a specific
shader and bind-group layout. **`Camera` is the one that matters next** — the realignment gives every
scene a camera accessor so the resolution controller (docs/49) can ask what is in view without knowing
which scene it is looking at, and that needs one `Camera` type in one place.

**Found while doing it: a charter violation, logged not fixed (docs/46 ledger row 13).** "What colour
does matter at temperature T glow?" has TWO answers — `emission::incandescence` (docs/20, natively
tested, premultiplied `[r,g,b]`) and a second copy inside `mod app` for the space band (returns
`[r,g,b,intensity]`). They agree only on the 800 K threshold: one ramps `(T−800)/2200` with blue from
2600 K, the other `(T−800)/2400` saturating at 3200 K with blue past `x>0.55`. At 2000 K that is
`[0.545, 0.297, 0]` versus `[1.0, 0.5, 0.0]×0.6`. NOT unified here: collapsing them changes what the
space band looks like, which deserves its own rig verification rather than riding along in a
mechanical lift.

**Verified.** 242/242 native + 18 skipped, wasm check + `wasm-pack` clean, warnings unchanged (6).
**All three scenes rig-watched** with a new `web/rig/all_scenes.mjs` — terrain (hills, iron probe +
shadow, water), birth (Earth as a particle aggregate, sun-lit terminator, Theia inbound) and Terra
(globe with real continents/biomes), zero page errors, at the rebuilt wasm (`build 20260720.212820`).
A render-scaffolding change is the one place a single-scene check is worthless, since all three draw
through it via different pipelines.

**Method note — the guard corrected me twice.** The rig's first "is it blank?" metric counted distinct
32-bit words at sampled PNG offsets; it returned ~`bytes/997` every time, i.e. it counted its own
samples and could never fail. Replaced with the compressed size of a canvas-only crop. Then the FLOOR
was wrong too: I used a corner of the terra scene as the flat control and asserted the margin was "two
orders of magnitude" — measured, it was **39,992 B vs 64,003 B, only 1.6×**, because the crop overlapped
the globe. The control is now a blank page cropped identically: **1,883 B against 64–137 kB real
renders**, a 34–73× margin. Both errors were the same shape, and the shape is the point: a check that
cannot fail reports green forever.

## 2026-07-20 — the GPU particle CONTAINER lifted out of `mod app` (docs/33)

**What.** `GpuParticles` — the granular GPU container (storage buffer of grains stepped by
`particle_step.wgsl`, rendered from the same buffer) — is now `crate::gpu_particles`, a scene-agnostic
module compiled on every target, instead of 351 lines inside `#[cfg(target_arch = "wasm32")] mod app`
next to the terrain `Engine`. It brought its own configuration with it: `GRID_TABLE_SIZE`,
`GRID_BUCKET_K` and `MAX_PARTICLES` were scene constants describing the container, and are now the
container's. lib.rs 6,031 → 5,684. 242 tests (was 240), wasm clean.

**Why.** Two containers cannot be unified while only one of them compiles in a given build. This is the
convergence step of docs/33, and the sibling of the `gpu_sph` lift earlier the same day — the same single
`Rc<Cell<bool>>` in a `map_async` callback was the only thing here requiring wasm, too. Nothing about
this code is scene-specific (`GpuProbe` was already a second consumer) or wasm-specific: wgpu's types
exist without a backend, so it type-checks natively; running it still needs a browser. **The solvers stay
specialized** — stiff granular contacts and SPH are genuinely different physics, which docs/46 §1
sanctions. It is the duplicated CONTAINER that is the violation, and it is now one module away from
`gpu_sph` in the same build.

**It exposed another unguarded shader mirror.** `dispatch` turned a thread count into a workgroup count
with a bare literal `64`, mirroring `@workgroup_size(64)` on all six compute entry points with nothing
checking it. Raise the shader's size and the host under-dispatches: a TAIL OF GRAINS SILENTLY NEVER
STEPS — physics that quietly stops for some matter, with no error anywhere. Named `WORKGROUP` and pinned
by test; the test also asserts it parsed a non-zero number of entry points and that every `@compute` has
a size, so it cannot pass vacuously.

**Verified.** Both new guards proven able to fail: the workgroup test goes RED when one entry point is
changed to `@workgroup_size(128)` (restored after). 242/242 native + 18 skipped, `cargo check
--target wasm32-unknown-unknown` clean, `wasm-pack` clean, warning count unchanged (6). **Rig-watched
with a purpose-built rig** (`web/rig/debris_container.mjs` — a static terrain shot barely touches the
container, so it could not have caught a broken lift): fire a meteor, and debris goes 0 → **3,516** on
impact (append), then falls monotonically 3,516 → 3,399 → 3,222 → **2,542** over 22 s as grains settle
and de-resolve. That is append → step → expand → readback → de-resolution, the whole container lifecycle,
through the lifted module. Screenshots show the resolved voxel patch with individual grains and ejecta
streaks; probe and water intact. Build stamp checked against the rebuild (`20260720.205231`), per the
stale-wasm trap recorded below.

## 2026-07-20 — the GPU host code was never wasm-only: one line hid 700 lines from the suite

**What.** `gpu_sph.rs` compiles on **every** target now, not just wasm, and its three shader-facing
`#[repr(C)]` mirrors are pinned to `sph_step.wgsl` by in-crate tests (240 total, was 236). The single
thing that had required wasm was one `Rc<Cell<bool>>` in a `map_async` callback: wgpu bounds that
callback by `WasmNotSend`, a no-op on wasm but plain `Send` everywhere else. Replacing it with
`Arc<AtomicBool>` (Release/Acquire — the flag publishes a completed mapping that `take_readback` then
reads through `get_mapped_range`) removed the entire reason for the `#[cfg(target_arch = "wasm32")]`.

**Why it matters.** CLAUDE.md rule 3 recorded the consequence as a fact of life — "`gpu_sph.rs` has **no
in-crate tests**" — and docs/33 called it "0 in-crate tests **by design**". Neither was design. It was an
accident of one line, and it put ~700 lines of shipping GPU host code where native `cargo check` and
`cargo test` never compiled them. That is the same trap that once shipped a non-compiling commit claiming
181 passing tests. `gpu_layout.rs`'s header already stated the principle for the POD types it rescued
from `mod app` — *"nothing but their location was ever keeping them there"* — and it turns out to hold
for the wgpu host code too, which is the premise the container-unification step of the realignment rests
on: two containers cannot be unified while only one of them compiles in a given build.

**It immediately found a real drift.** `sph_step.wgsl`'s `Params` declares `omega` at byte 36 — a
rigid-rotation rate whose centrifugal term `ω²·(x,y,0)` the shader applies in `cs_relax` (`:253`) so a
body relaxes to its OBLATE equilibrium. The Rust mirror called that same slot `_p0` and treated it as
padding. **Offsets and sizes matched**, so any size/stride check passed; only a NAME-level comparison
sees it. Not wrong physics today (the host hardcoded 0.0, and ω=0 is exactly the non-rotating relaxation
intended) but a live latent hazard — reuse that "padding" as scratch and the body silently spins — and a
capability the host could not reach. Renamed to `omega` and documented; zero bytes changed.

**Verified.** Guard written RED first: the layout test would not compile against `_p0`, which is the
detection working (the `offsets!` macro takes field identifiers, so a Rust-side rename breaks the build
and a shader-side rename breaks the assert — both directions covered). Then green. Full native suite
**240/240 + 18 skipped**, `cargo check --target wasm32-unknown-unknown` clean, `wasm-pack` build clean.
Rig-watched on the 5060 Ti at the rebuilt wasm (`build 20260720.193215`): the birth-of-the-Moon scene
renders Earth as a particle aggregate with the sun-lit terminator and Theia inbound, and the terrain
reference scene renders hills + iron probe + shadow + water. **Caught mid-check:** the first rig run
reported green against a vite server started BEFORE the wasm rebuild — the wasm URL is cache-busted with
a build stamp vite computes at startup, so it served the OLD bytes. Restarted vite and re-ran; the
verification above is the fresh build. That trap is now documented in `rigshot.sh` and the rig README.

**One parser, not two.** The WGSL↔Rust offset checker moved out of `gpu_layout`'s test module into
`wgsl_layout.rs` (test-only) and both shaders' guards use it. Copying a parser is how one question
acquires two answers (docs/46, Law 2).

**Rig ergonomics (Robin's ask).** Rigs hardcoded whichever port their session used — 13 different dead
ones — and 30 of them wrote into a long-gone session's scratchpad directory. `PORT` (default 5173) and
`OUT` (default `/tmp/rigshot`) now come from `rigshot.sh`; 56 rigs rewritten, all 61 parse, previously
broken ones run with no env set. `birth_shot.mjs` defaulted to the PUBLIC site, so a bare run
screenshotted production and looked like a verified local change — it defaults to local now. **Robin's
correction, recorded in `web/rig/README.md`:** these rigs are one-off instruments built to choose a path
or fuel a doc, **not** a test suite — do not assume any of them is still relevant or valid, and a green
rig is not evidence. The README had also been recommending `xvfb-run`, the trap rule 4 exists to kill.

## 2026-07-20 — headless GPU rig verification WORKS (the linchpin) — real WebGPU renders captured

**What.** Solved headless visual verification — the tool that was missing, and the reason "unification"
kept being claimed without proof. `scripts/start-render-xorg.sh` + `scripts/rigshot.sh` capture a real
composited WebGPU render on the RTX 5060 Ti, headless. Verified: the terrain scene (green hills, the iron
probe + its shadow, water) and Terra (Earth globe, real continents/biomes) both rendered correctly.

**The two things it took, neither obvious:**
1. **A real GPU-backed X server.** `xvfb` is software — the GPU renders but the software compositor cannot
   read back the GPU swapchain, so `page.screenshot()` returns the DOM HUD over a BLANK canvas (the trap
   that made prior sessions believe headless WebGPU "can't composite"). A headless `Xorg` on the 5060 Ti
   (`PCI:2:0:0`, `AllowEmptyInitialConfiguration`, `-ac`) composites in hardware and screenshots capture
   the render.
2. **Match WebGPU to the compositor GPU.** Dawn defaulted to the 2070; presenting a 2070 render on the
   5060 X server threw `VK_ERROR_DEVICE_LOST`. `MESA_VK_DEVICE_SELECT=10de:2d04` (the Mesa device-select
   layer works for NVIDIA too) forces WebGPU onto the 5060 — adapter then reports `blackwell` and it works.

**Why it matters.** Every visual claim is now verifiable, headless, on the real GPU. The Laws require
rig-watching visual claims; until now I could not, so the scene-level realignment was unverifiable. It is
not any more. CLAUDE.md rule 4 updated (the old `xvfb` instruction was the trap).

## 2026-07-20 — the Analytic → Resolved hand-off as ONE central system, wired (docs/49)

**What.** `resolution::ResolutionField` — the single system that makes the hand-off an inherent engine
property, not per-scene wiring. A scene registers active physics as analytic `Effect`s; one `update()` per
frame propagates them by cheap math off-camera and materialises each the frame it enters view, through the
SHARED matter path. Wired into the terrain `Engine`. +4 tests (236 total), wasm clean.

**The correction that shaped it (Robin).** My first cut emitted opaque `ResolutionEvent`s for a per-scene
"backend adapter" to apply — which ENTRENCHES the forked particle containers (docs/32 §4) it should be
unifying. The charter is one particle system, one materialisation, different scales; the forks are the
violation, not a fact to design around. Rebuilt to materialise straight through the shared `MatterSim` —
no adapter. A scene on a different container is one that has not yet converged (docs/33), not a reason for
a second path.

**The other correction, from a failing test.** `materialize_region` EXCAVATES world voxels; an ejecta blob
in mid-air has none, so it made nothing. Robin's Moon example is CARRIED matter arriving — a deposit, not
an excavation. Added `MatterSim::spawn_region` (deposit carried matter as grains) as the shared primitive
for in-flight effects.

**Verified.** The Moon lifecycle end to end, native: an effect starts off-camera, propagates BALLISTICALLY
with ZERO particles created, and the frame it enters view spawns grains via the shared path; an effect that
never enters view is never simulated but is still TRACKED (existence is not gated by the camera). Wired
into `Engine::render` as one `update()` call + `register_effect`; wasm clean.

**Open (docs/49 §5):** no effect SOURCE feeds it yet (live scene unchanged); the other two scenes get the
identical call once they converge on shared matter (docs/33); grain size reaches the CPU store only once
the CPU `Particle` carries a radius.

## 2026-07-20 — resolution controller, THREE modes: math off-camera, simulate + render in view (docs/49)

**What.** Refined the controller (same day it landed) from a two-state resolve/bulk decision to THREE
regimes, per Robin: existence is the physics'; the camera chooses the REPRESENTATION — cheap MATH for
active physics that is not visible, particle SIMULATION + render for active physics in view. `ACTIVE ×
IN-VIEW → {Bulk, Analytic, Resolved}` (`resolution::ResolutionMode`). Still 233 tests (6 controller tests
rewritten), wasm clean.

**The model.** Bulk = no active physics (rendered at camera LOD). Analytic = active but off-camera:
compute the effect with math and PROPAGATE it (docs/28's giant-impact ejection is exactly this), no
particles. Resolved = active AND in view: simulate + render, at the finer of camera and physics
granularity. The Moon slamming the far side of the planet is Analytic (energy known); its ejecta flips to
Resolved the frame it arcs into view — regions are re-queried every frame, so "render the effects as they
come into view" is automatic.

**It also CORRECTED the first cut.** The two-state version resolved for camera-closeness alone, which
would simulate undisturbed static ground just because you walked up to it. Wrong: simulation is for ACTIVE
physics that is visible; static ground stays Bulk (rendered finely). The invariant sharpens — active
physics off-camera is never Bulk, it is at least Analytic (computed): the camera changes the
representation, never whether the physics is true (test: a far-side impact at 6,000 km is Analytic).

**Still not wired** (docs/49 §5) — decision policy only; nothing calls `decide()` to compute/materialize
yet. The Analytic→Resolved hand-off (materialize an incoming analytic effect as it enters view) is the
crux of the wiring and maps onto the existing docs/28 analytic ejection + docs/39 particalization.

## (superseded same day) the core resolution controller: camera drives granularity, necessity drives existence (docs/49)

**What.** `crate::resolution::ResolutionController` — the decision policy Robin named a default core engine
feature: one controller every scene holds, deciding whether matter resolves into particles and how fine.
New docs/49. +6 tests (233 total, wasm clean).

**The two axes and the one rule.** CAMERA drives GRANULARITY (a grain finer than one subtending the
angular threshold at the camera distance is sub-pixel — `camera_grain_radius = distance·angular_res`,
linear per docs/13). NECESSITY drives EXISTENCE (the admission test; an unwatched wheel still sinks).
They compose, and **the camera may only REFINE, never gate**: `resolve = necessity ∨ camera-close`,
`grain = finer_of(camera, physics)`. Letting the camera gate existence — so looking away changes what is
true — is the charter violation this exists to prevent (docs/44 §1, docs/30).

**Verified — the load-bearing test.** `necessity_resolves_even_when_the_camera_is_infinitely_far`: a
sinking wheel 100 km from the camera STILL resolves, at the 1 cm physics granularity, not the coarse
camera grain. Plus: the null case (far + unnecessary ⇒ resolve nothing, exactly free); camera-only visual
resolution that refines with proximity; composition = the finer of the two; granularity clamped
[floor, bulk]. `angular_resolution` is the one legitimate fidelity dial (a declared viewing tolerance,
like render resolution — not a physics fudge).

**NOT wired (docs/49 §5).** This is the policy; nothing calls `decide()` to materialize/demote yet. Two
honest blockers for a VISIBLE demo: the scene structs are in `#[cfg(wasm32)] mod app` (not natively
testable — rig can't composite WebGPU here), and the null case is correct-but-invisible (a probe on basalt
resolves nothing), so seeing it work needs a soft surface under load (the parked regolith) plus the
voxel→field demotion trigger (docs/47 step 1b). Landed as the verified core; wiring sequenced behind those.

## 2026-07-20 — deterministic scatter: built, MEASURED 2–12× WORSE, reverted (docs/47)

**What.** Implemented the full deterministic-scatter design for the multi-level gather cost — fixed-point
atomic accumulation (integer add is associative ⇒ order-independent, so `cs_grid_sort` dropped), each pair
visited once from the finer grain and written to both. **It was 2×–12× slower than the gather** and is
reverted; `main` keeps the gather.

**The numbers (RTX 5060 Ti, N=60k):** uniform 5.5→13 ms (2.4× worse), 3 levels 16→198 ms (12× worse),
5 levels 117→513 ms (4.4× worse). Correct and deterministic — D0 bit-identical, G0 cross-level contact
passes, scene suite 30 PASS / 1 pre-existing (scene D repose). It works; it is just slow.

**Why — atomic contention, which the design (mine) failed to price.** Many fine grains `atomicAdd` into
one big grain's accumulator slots and serialize, and every contact pays ~26 atomic ops (13 slots × 2
grains) versus the gather's register accumulation. So even the UNIFORM case — no wide ratios at all — lost
2.4×. The gather's "wasted" scan of fine cells is CHEAPER than scatter's atomic traffic for this workload.
This is the value of the ethos: the design reasoned scatter was O(contacts) and cheaper; the machine said
otherwise, and the measurement is the deliverable.

**Method note worth keeping:** the force-magnitude readback (`GPU_VERIFY_FORCEMAG`) measured the
fixed-point scales from real data (force ≤2.1e4, tensors <1, sv_nbr ≤8.5e2) rather than guessing them —
that part was right, and the scales worked (scene I energy monotonicity held). The flaw was in the cost
model, not the numerics.

**Left open:** the 21× multi-level gather cost is NOT fixed. Single-scale-per-frame (the camera-descent
common case) is cheap and unaffected. The code is preserved on the pushed `archive/deterministic-scatter` tag
as an evidence dead-end (do not merge); docs/47 records the remaining options (accept it / coarse summary /
measured hybrid).

## 2026-07-20 — the quasi-static admission law: a car on basalt resolves exactly zero particles (docs/44)

**What.** New `resolution.rs` — the quasi-static admission test docs/44 §4b specifies and its §8 status
table lists as "does not exist". Given a contact pressure `p` over a patch of radius `a`, `resolved_depth`
returns the depth `z*` to which the load still reaches the material's yield — the region that must be
resolved into particles — and **0 when the surface pressure is below yield**, which is the whole point:
the test's main job is to say *no*.

**Why it is the vehicle unlock.** docs/44 §3: everywhere the material provably cannot move, the cheap
bulk model is not an approximation, it is the correct answer. So a 1500 kg car on basalt (p ≈ 184 kPa,
yield ≥ 10 MPa) resolves **nothing** — exactly, not nearly — while the same car on loose sand resolves
0.41 m deep. The footprint is small because the physics says so, not because a constant capped it.

**Closed form, not iteration.** `σ_z(z) = p·[1 − (1+(a/z)²)^(−3/2)]` is monotone in z, so the root
`σ_z(z*) = σ_yield` inverts exactly: `(a/z*)² = (1−yield/p)^(−2/3) − 1`. Verified self-consistent (the
Boussinesq stress at the returned depth equals the yield it was solved for).

**Honesty flags carried from the doc.** Boussinesq is an elastic-half-space result and granular media are
not elastic half-spaces; it is used ONLY as a conservative sizing *envelope*, never as a force law (forces
stay `granular::contact_accel` + `terrain_contact_resolve`). Per docs/44 §5 the bias is toward resolving:
`admission_depth` expands `z*` by one patch radius, because under-resolving loses physics silently while
over-resolving only costs frame time.

**Verified.** 227/227 (+4), wasm clean. Pinned to docs/44 §4b's worked table to the millimetre: basalt 0,
regolith 0.096 m, sand 0.409 m. The null case is asserted as EXACTLY zero, since it is the cheap half of
the whole idea.

**Open — this is the LAW, not the wired policy.** Nothing calls it. The full resolution policy is docs/44
§9's five steps; this is §9.3's law only. Wiring it (a resting load materializes the ground under its
patch when `admission_depth > 0`) is the next visible step and touches the terrain scene + GPU. Deploying
today ships the tested law but changes no live behaviour — deliberately, since a visual scene change
cannot be rig-verified in this environment (WebGPU does not composite headlessly here).

## 2026-07-20 — the hierarchical grid on the GPU: correct at every scale, and measured to be slow

**What.** `particle_step.wgsl` gained a hierarchical spatial hash — `cell_size(level) = base_cell·2^level`,
level folded into the hash key, one table. Insert at the grain's own level; the force GATHER walks every
populated level. `Params` gained `base_cell`/`max_level` (reusing the reserved `_hp1`/`_hp2` slots, so NO
struct-size change — the layout guards confirm it). `gpu-verify` gained scene **G0** (boulder vs pebble,
3 levels apart) and a **production-N mixed-granularity BENCH**.

**Why plumbing became capability here.** With `max_level = 0` the level walk collapses to the old ±1 scan
and the output is **bit-identical** to the flat grid (verified end-to-end, only possible because
determinism landed first). Above 0, grains of different size find each other for the first time.

**The measurement, which changed the conclusion.** I was about to call this "mixed granularity works". The
bench says: correct, but the multi-level GATHER is expensive, and the cost is inherent, not a bug.
Fine-dominated distribution (half the grains at each successive coarser level), RTX 5060 Ti, 24 reps,
per-frame GPU time (SUBSTEPS batched into one encoder, the engine's pattern):

| N | uniform (max_level 0) | 3 levels | 5 levels |
|---|---|---|---|
| 10,000 | 2.6 ms | 7.3 ms | 49 ms |
| 60,000 | 5.5 ms | 16.4 ms | **117 ms** |

Uniform matches the pre-hierarchy grid (~0.09 µs/particle at 60k). Five levels is **~21× slower**.

**The cause, understood.** In a gather each grain finds its own neighbours, so a BIG grain must scan the
FINE level to see its many small neighbours — `(r_big/r_fine)³` cells, most of them empty or inside the
big grain. That is intrinsic to gather + wide size ratio. The cheaper route is symmetric SCATTER (compute
each pair once from the fine grain, write force to both), which lets a big grain scan only its own level
and coarser — the O(1)-per-pair cost `grid::pairs_within` achieves on the CPU. **But scatter needs float
`atomicAdd` into the force buffer, whose order is race-decided — reintroducing exactly the nondeterminism
fixed hours ago.** Deterministic scatter is possible (per-cell reduction, or sort-then-segment) but is a
real design task, not a patch.

**Status, stated honestly.** Single-scale-per-frame (max_level low) is production-ready and free. Multi-level is CORRECT (G0) and
now MEASURABLE, but too slow for wide size ratios until the gather is replaced. It ships behind
`max_level = 0` (every live scene), so nothing in production pays the cost today. The next step is the
deterministic-scatter design, scoped by these numbers rather than guessed.

**Verified.** Engine 223/223; wasm clean; `gpu-verify` D0 + G0 pass, uniform scenes bit-identical to the
flat grid, run-to-run bit-identical; shader runs on the RTX 5060 Ti. G0 confirmed to have teeth (setting
`max_level = 0` makes the cross-level contact vanish and the scene fail).

## 2026-07-20 — GPU determinism: the spatial hash was summing forces in race order

**What.** New `cs_grid_sort` pass puts every hash bucket in a canonical (particle-index) order between
`cs_grid_insert` and `cs_forces`, in BOTH the engine and `gpu-verify`. `gpu-verify` gains scene **D0**,
which runs one input twice and compares — the check that gates every other tolerance in the harness.

**The cause, confirmed rather than assumed.** `cs_grid_insert` takes its slot from `atomicAdd`, so a
bucket holds the same SET of particles every run but in whatever ORDER the threads won the race.
`cs_forces` then sums contact forces in that order, and float addition is not associative. Identical
input therefore produced different output. Reproduced under control before touching anything: **7 of 174
grains diverged, worst 8.3e-5 m after 40 frames** — the seed that amplified into the ~6% spread measured
yesterday on scene E (33.1 / 33.5 / 35.1 m across three runs of identical code).

**Why it mattered more than a cosmetic wobble.** Scene I is the FUDGE DETECTOR, and its tolerance was
**wider than its own reproducibility**. Any regression smaller than the drift was invisible, and — as the
per-particle-radius change demonstrated the day before — a real change could not be distinguished from
noise. Every margin in the harness was unfalsifiable.

**Verified.**
- D0: **0/174 grains differ, worst 0.0 m** — bit-exact.
- **The FULL SUITE is now bit-identical across two runs** (diffed end to end). It was not before.
- Scene D (repose) still fails, on this branch and on `main` alike — pre-existing, unrelated, measured on
  both rather than recalled.
- Engine 223/223, wasm clean, shader confirmed compiling on the RTX 5060 Ti.

**Residual, and empirically covered rather than argued away.** If a cell OVERFLOWS `bucket_k` (16), which
particles won slots is still race-decided, so sorting cannot canonicalise the set. `grid_count` keeps the
true unclamped count, so it is detectable. It is not occurring anywhere in the suite today — that is what
a bit-identical full-suite diff demonstrates, since an overflow race would show up as exactly the
nondeterminism D0 tests for.

**Cost, flagged and deliberately NOT quantified here.** `cs_grid_sort` dispatches over the whole table
every substep — the same shape as `cs_grid_clear`, which was measured at ~0.52 ms/frame and is queue item
3. A similar fixed cost should be expected, but I am not quoting a number I have not measured
([[use-gpu-perf-skill]]). The mitigation is shared with item 3: touch only occupied cells (epoch/
generation tags), which removes the fixed dispatch from both passes at once.

## 2026-07-20 — per-particle radius on the GPU, and the harness that cannot tell if it mattered (docs/47 §1)

**What.** `Particle` grows to 80 bytes with a per-particle `radius` (plus a padded 5th row reserved for a
cached grid level). The shader reads THIS grain's radius everywhere a global grain size was assumed:
contact `touch = ri + rj`, headroom `length(dj) − (ri + rj)`, terrain penetration, drag cross-section,
and the resting test. `P.part_half` and `P.c_radius` — two constants that were both 0.5 and described the
same physical quantity — no longer drive per-grain behaviour.

**Plumbing, not capability.** Every grain is still given the same radius by the CPU, so mixed sizes are
not yet enabled; the hierarchical grid (`crate::grid`, landed, not yet mirrored in WGSL) is what makes
them correct, because a flat grid's ±1-cell invariant breaks the moment radii differ.

**Verified — and the interesting part is what could NOT be verified.**
- Both layout guards passed after the growth, which is the whole point of having landed them first: the
  80-byte struct is confirmed field-for-field against the shader in the engine AND in `gpu-verify`.
- Engine 223/223, gpu-verify 2/2, wasm clean.
- **The shader really compiles and runs** — `cargo build` does NOT validate WGSL (it is compiled at
  device creation), so this was checked by running `gpu-verify` on the RTX 5060 Ti.
- `gpu-verify`: **1 scene fails, and `main` fails exactly the same one** (scene D, repose — the
  pre-existing spherical-grain rolling-resistance deficiency). Measured on both branches rather than
  assumed from memory.

**What the harness could not answer.** The numeric diff between `main` and this change is real but
**smaller than the harness's own run-to-run noise**. Same code, same card, two runs: scene E spread
33.5 → 35.1 m, while baseline→change moved it 33.1 → 33.5 m. Scene I (the FUDGE DETECTOR) drifts
−6622 / −6807 / −6798 across runs of identical code. So the honest claim is **"no new failures and no
difference exceeding the noise floor"** — NOT "behaviour identical", which this harness cannot establish.

That is queue item 1 (determinism) blocking a real verification decision for the first time, exactly as
it warned. Free data for item 2 while we are here: **scene E spread varies ~6% (33.1 / 33.5 / 35.1 m)
across three runs of identical code on one card.** Until that floor is fixed, any change of this size is
unfalsifiable on the GPU side.

## 2026-07-20 — the layout guard that could not fail, and the one that can (docs/47 Hazard 0)

**What.** `GpuParticle`/`GpuStepParams` moved out of `#[cfg(target_arch = "wasm32")] mod app` into a new
natively-compiled `gpu_layout` module, and BOTH Rust mirrors (engine + `tools/gpu-verify`) are now pinned
to `shaders/particle_step.wgsl` by **byte offset**. 223 tests, gpu-verify green, wasm clean.

**Why the move.** The mirror that actually SHIPS lived inside a module native `cargo check`/`cargo test`
do not compile at all — so the production layout was verified by nothing but a human reading two files
side by side. Nothing kept it there but its location: it is plain POD, and `bytemuck` is an ungated
dependency.

**The part worth recording: my first guard was fake, and it passed.** It compared the WGSL field list
against a HARDCODED array and never read the Rust struct. Pinning the shader to a literal proves
nothing — reorder two Rust fields and it stays green. I only found it by deliberately swapping
`vel`/`resting` to check the guard had teeth; it did not. **The same defect had already shipped in #29**
for `gpu-verify`. Both are now rewritten around `std::mem::offset_of!`, so the assertion is tied to the
real layout, and both were re-verified by swapping fields and watching them fail.

A layout guard that passes while the layout drifts is worse than no guard: it converts an unchecked risk
into a believed-checked one. Each file now also carries a `the_guard_detects_a_reordered_struct` test, so
the ability to fail is itself asserted rather than assumed.

**Verified.** Engine 223/223; gpu-verify 2/2; wasm clean; both guards confirmed failing on a swapped
field pair and passing again on revert. The WGSL parser splits on COMMAS, not lines — the shader declares
`_hp1 : f32, _hp2 : f32,` on one line and a line-based parser silently drops the second, in exactly the
padding region the struct will grow into.

**Open.** Both mirrors are now bound, which was the prerequisite. Growing the struct to 80 bytes for
per-particle radius (docs/47 item 3) is next, and the guards will now fail loudly if the three
declarations disagree.

## 2026-07-20 — binding a repr(C) mirror to the shader that actually reads the bytes (docs/47 Hazard 0)

**What.** `tools/gpu-verify` now has a test that parses `shaders/particle_step.wgsl` and asserts its
`#[repr(C)] Particle` matches the shader's `struct Particle` field-for-field, in order, plus a 64-byte
stride check. Partial: this binds ONE of the two Rust mirrors.

**Why.** Per-particle radius (docs/47 item 3) grows the particle struct from 64 to 80 bytes, and that
layout is declared three times with disjoint coverage — `GpuParticle` inside `#[cfg(wasm32)] mod app`
(native `cargo test` never compiles it), `gpu-verify`'s standalone replica (native only), and the WGSL
(the runtime authority, checked by neither). A change can pass the native suite AND gpu-verify and still
be wrong in the browser. `cargo check --target wasm32-unknown-unknown` does not close it: rustc never
sees the shader, so field ORDER drifts silently. **This has already fired** — `gpu-verify`'s own comment
records `drag_cd` arriving as 0.0 from a drifted mirror, drag quietly a no-op.

**A plan correction, found by reading the file rather than reasoning about it.** The version merged in
#28 said to delete gpu-verify's replica and import the engine's declaration. Its Cargo.toml says why that
is wrong: gpu-verify is deliberately NOT a workspace member, carrying its own `[workspace]` table so its
native Vulkan `wgpu` build cannot leak into the engine's WebGPU-only wasm build via cargo feature
unification. So two mirrors are PERMANENT — which is safe precisely because each is pinned to the same
authority. Pinned to one shader, they cannot drift from each other.

**Verified.** The test passes; and it was confirmed to have teeth by swapping `vel`/`resting` in the
mirror list, which fails it, then reverting. gpu-verify's suite green.

**Open — the plan is NOT finished.** Still to do: move `GpuParticle`/`GpuStepParams` out of
`#[cfg(wasm32)] mod app` into a natively-compiled module so the PRODUCTION mirror is under `cargo test`
at all, then give it the same shader cross-check. Only after both mirrors are bound should the struct
grow to 80 bytes. The engine-side mirror — the one that actually ships — remains unbound today.

## 2026-07-19 — the axle: a wheel that spins without the engine knowing what rotation is (docs/47 §2)

**What.** New `crate::axle` — the revolute joint docs/47 §3 specified, as a constraint rather than a
spring. `axle::resolve` per substep: (1) a velocity-decoupled position projection putting the hub back on
its anchor, (2) a COM-velocity match returned as an impulse, (3) an angular split preserving spin about
the axle axis exactly and refusing everything else. 5 tests; 211 total, wasm clean.

**Why a constraint.** `Bond` is a distance spring, and a penalty joint stiff enough to hold a wheel on is
stiff enough to launch it — the same mechanism as the terrain settling storm, which went away by moving
from a penalty to a constraint. The axle is the same shape as `granular::terrain_contact_resolve`.

**The piece §3 left implicit, and it carries the whole argument:** the wheel's angular velocity is
recovered from the particles' LINEAR momenta alone, `ω = I⁻¹L` over the cloud. That is the mass-weighted
least-squares rigid rotation, which is *why* the constraint is provably non-injecting — subtracting a
least-squares projection can only reduce the residual, never grow it. No orientation, angular velocity or
inertia tensor is stored anywhere; §3's claim that torque emerges from forces holds in code.

**Verified.**
- `a_freely_spinning_wheel_is_left_completely_alone` — the test an axle has to pass. A compliant wheel
  comes back bit-identical, and its spin is recovered as exactly the 37 rad/s it was given. A joint that
  bled spin here would look like bearing friction while being a numerical artifact, and would be
  indistinguishable from the DECLARED bearing-friction model docs/47 §4 owes a derivation for.
- `wobble_is_refused_the_axle_spin_survives_and_the_reaction_is_reported` — a 9 rad/s tumble about X is
  removed while 25 rad/s about Z survives untouched; the reaction opposes the wobble and is exactly zero
  about the free axis.
- `the_hub_is_pulled_back_to_its_anchor_injecting_no_energy` · `the_axle_never_increases_energy` (four
  perturbed cases: drift, tumble and pure deformation at once) · `a_force_couple_spins_the_wheel_and_the_
  axle_lets_it` — zero net force, torques adding, and the axle passes the drive torque through.

**What it deliberately does NOT do: rigidify the wheel.** Only the best-fit rigid rotation is touched, so
deformation passes through — the reason a tyre is rubber and can spread a contact patch.

**Open.** Nothing calls it: there is no chassis to bolt it to until docs/47 item 4. Reactions are
returned, not applied, so whoever wires it must hand the negatives to the chassis or the joint creates
momentum.

## 2026-07-19 — one answer to "where is the ground", so a patch can de-resolve (docs/47 step 1a)

**What.** `World::ground_top_voxel` is now the single authoritative ground query: it returns the voxel
top while a column is resolved and keeps returning the SAME top after that column is demoted to the T0
field. The GPU grain heightfield, the CPU bilinear contact surface and the rendered bulk cap all read it
(the cap via `displacement_at`). A `demoted: Vec<bool>` mask disambiguates "baked into the field" from
"excavated to nothing". +4 tests (203 total, wasm clean).

**Why.** docs/47 §5 called voxel→field demotion "not new machinery" because `demote_column_to_field` and
`column_is_bakeable` already exist, tested — with **zero non-test callers**. That was true of the
function and false of everything around it. The engine held **three different answers to one question**:
the GPU heightfield read raw `surface_top_voxel`, the bulk cap read raw `terrain_height` ignoring
`displacement` entirely, and probe contact read `bulk_height`. So simply calling demotion would have
dropped every grain resting on that column through the floor, and drawn the de-resolved crater as
untouched ground — the render disagreeing with the physics, the one direction this engine never allows.

**The useful discovery: demotion needs no sub-voxel heightfield.** The bake preserves the surface
exactly, and that surface is already voxel-quantised (`top − 0.5`), so the field hands back the
*identical integer top*. The GPU's `array<i32>` is untouched and demotion stays cleanly separate from
the deferred f32-surface refactor (docs/45's `SLOPE_QUANTUM_M` IOU). I had started down the opposite
path — continuously baking the field to track the voxels — and abandoned it on noticing it would snap
the *smooth* procedural field onto the voxel quantisation at startup, degrading the higher-fidelity
representation by up to 1 m. The field records DEFORMATION, not rounding.

**Verified.**
- `ground_top_survives_demotion_exactly` — over five columns, the top before demotion equals the top
  after, exactly, once the voxels are gone.
- `an_excavated_column_has_no_ground_but_a_demoted_one_does` — the ambiguity the mask exists for; a zero
  displacement would otherwise pop dug-out ground back to pristine relief.
- `putting_matter_back_re_resolves_a_demoted_column` — a de-resolving grain hands authority back to the
  voxels instead of the column answering from a stale bake.
- `the_bulk_cap_renders_a_de_resolved_crater` — field-blind cap vs field-aware cap over a baked 6 m pit;
  pins render/physics agreement, which would otherwise fail silently.
- 203/203 native, `cargo check --target wasm32-unknown-unknown` clean.

**Open, and stated plainly: NOTHING TRIGGERS DEMOTION YET.** This makes it safe, not active — step 1b is
the quiescence trigger. Two known obstacles for it: `patch_resolved` is a single bool for the whole 96 m
patch while demotion is per-column (they do not compose), and `bulk_height` still returns pure procedural
relief for a column dug but not demoted, so the field/voxel seam is consistent only because
`patch_resolved` gates which of the two is asked. docs/46 ledger row 6 narrowed, not closed.

## 2026-07-19 — terrain learned the half of Mohr–Coulomb the grains always had (docs/45)

**What.** Terrain slope stability now carries the **friction term**. `granular::face_stable` is the law —
a face stands if friction holds the slope (`drop ≤ μ·r + quantum`) OR cohesion holds the bank
(`run ≤ c/ρg`) — and `matter::materialize_steep_terrain` applies it per voxel against every neighbour out
to 8 cells, iterating to a fixpoint. `steep_drop = 3` is retired. +7 tests (199 total, wasm clean).

**Why.** `τ = c + σ·tan φ`, and terrain implemented only `c`. The φ half was hidden behind a constant that
tolerated a **72° face for every material** while gravel fails above 40°. With a grass skin on basalt that
never showed, because basalt's `h_crit ≈ 510 m` holds anything this world can build; a cohesionless
horizon has `h_crit = 0` and the missing term becomes load-bearing instantly. The datum was never
missing — `friction_coefficient` is in `data/materials.json` for every material and the grain side has
read it since docs/23. Ground and grain were answering the same question two ways, which is the thing
docs/46 exists to forbid.

**Three corrections the design doc did not have, each found by measuring:**

1. **The φ term alone does not converge; the removal TARGET was the real bug.** The old rule cut a failing
   face down to its lowest neighbour, which moves a cliff rather than relieving it — the column behind
   becomes the new face at nearly the same height. Measured on a cohesionless horizon: **106, 148, 214,
   285, 339, 387, 433, 468, 504, 542, 577, 622 grains per pass — monotonically increasing** over 12
   passes, 13-voxel face still standing. Cutting to the *stable* height instead grows a talus ramp that
   climbs one column per pass and stops at the plateau.
2. **Cohesion must judge the material's own bank, not the drop.** A 1 m grass veneer over basalt on a 2 m
   step is not a 2 m grass bank. Judging it against the full drop **shed 470 grains from a pristine world
   nothing had touched** — every hillside steeper than the skin is thin trips it. Cohesion now uses the
   contiguous same-material run; friction still uses the drop, because slope is a property of the surface.
3. **Faces fail from the base.** Stopping the walk at the first voxel that holds lets a self-supporting sod
   skin shield the failing 10 m bank beneath it. The lowest failing voxel is found first and everything
   above goes with it.

**Verified.**
- **Convergence** — a cohesionless horizon with a 7×7×10 pit reaches a fixpoint inside one call; a second
  call sheds **exactly 0**. Bounded by geometry (4,159 grains vs a 6,157 wedge bound derived from the ramp's
  reach), and terrain 25 cells out is byte-unchanged: the slide does not march inland.
- **End state is repose** — the settled slope is asserted against gravel's **DB μ**, not a literal:
  no 8 m baseline exceeds `μ·8 + quantum`. The test also asserts the terrain developed a real slope
  (> 1 m), so a flat world cannot pass it vacuously.
- **Rock cliffs stand** — basalt sheds no grains at any pit depth; the new steep grains were 100% grass
  before fix (2), 0 after.
- **Pristine terrain is a no-op** — 470 → **0** grains, solid count unchanged.
- **Burial (docs/45 §5)** — worst penetration against the bilinear collision surface **2.75 m → 0.50 m**,
  1.2% of grains penetrating at all (the regolith branch had worsened it to 3.75 m). Resolved as a side
  effect of correction 1: grains now come off the wedge above the ramp, not from a column cut to the floor.
- 199/199 native, `cargo check --target wasm32-unknown-unknown` clean.

**Two existing tests were passing without testing their own names, and are now honest.**
`a_granite_cliff_holds_while_the_dirt_above_it_slumps` asserted "no granite grains" against a world that
contains **no granite and no dirt** — it is a 1-voxel grass skin directly on basalt — and its "the weak
dirt slumps" was the grass skin moving for the buggy reason in (2). It now builds the layered cap it
describes. `materialize_steep_terrain_turns_cliffs_into_grains_conserving_mass` dug into basalt and
asserted the walls **must** materialize, which docs/45 §6 explicitly reverses ("a rock cliff still stands
… this must not flatten canyons"); its conservation checks moved to material that genuinely fails.

**Flagged, not hidden.** `SLOPE_QUANTUM_M = 1.0` is a **resolution IOU**: an integer heightfield cannot
express a slope between 0° and 45°, so enforcing repose at one cell with no allowance would force every
soil in the DB flat. Over `r` cells it dilutes to `1/r` — at the 8-cell baseline, ~3.6° above gravel's
true 40°. The sub-voxel surface retires it; a bigger baseline only buys O(r²) sandpaper.

**Measured against `regolith-horizon` itself (docs/45 §8), and it corrects this entry's own first claim.**
Stacking regolith's `world.rs` on this work and stabilising repeatedly gives **`[1466, 0, 0, 0, 0, 0]`** —
converges on the first call, zero on every later one, mass conserved. The unbounded slide is gone, and the
law discriminates by material exactly as it should: of 1,466 grains, **870 dirt, 580 grass, 16 gravel** —
dirt (φ=28.8°) fails where the cohesionless gravel beneath it (φ=40°) holds. **But regolith is still not
mergeable, for a different reason:** those grains come off *undisturbed* ground. A slope census finds
8-cell drops to **10 m (51°)** against gravel's 7.72 m allowance, while the profile lays a uniform 6 m
mantle over all of it. A uniform soil mantle is not physical on ground steeper than the soil's repose —
regolith's own comment says "thin on steep or glaciated ground"; the generator does not implement it.
**The blocker moved from "terrain cannot hold a slope" to "the generator places soil that cannot stand",**
and the fix is slope-tapered thickness in world generation, not here. Burial on that world is 2.00 m
(101/1,466), the same symptom.

**Open.** docs/45 §6's *emergent agreement* test (grain pile vs terrain slope reaching the same angle)
stays blocked on grain-side rolling resistance, per the doc. Nothing was tuned to make the halves agree.
On the live meteor scene this is near a no-op (76 → 0 grains) because that world is genuinely stable.

## 2026-07-19 — the architecture map had gone stale enough to assert that existing physics was absent

**What.** Refreshed `docs/32-architecture-map.md` and docs/33's status block against the code on `main`
(`1b4381e`), and corrected the same errors where `CLAUDE.md` repeated them. Neither doc had been
substantively edited since it was written on 2026-07-17 — `git log -- docs/32 docs/33` returns only the
repo-rename commit — while docs/34–48 and ~20 commits landed.

**Why.** CLAUDE.md instructs every session to read docs/32 before exploring, and docs/46's déjà-vu rule
rests on the docs being the reliable record. A stale map does not merely omit: it actively misdirects. The
worst entry told sessions to build a module that already existed, tested and verified.

**Corrections — each checked against the code, not inferred:**

- **§5 said "Condensed-matter EOS … CONFIRMED ABSENT."** False since docs/33 stage 1. `eos.rs` (373 lines,
  7 tests) implements the three-branch Tillotson `P(ρ,u)` (`:52`) with cited per-material constructors,
  verified vs Benz & Asphaug 1999. The real gap is **wiring**: `crate::eos::` has exactly two non-test
  consumers — `hydrostatic.rs:25` and `gpu_sph.rs:110` — so it is live in the space band only, while the
  terrain/voxel/granular path has no EOS at all and `GpuParticle.rho` (`lib.rs:1907`) is a placeholder ρ₀
  nothing computes. CLAUDE.md carried the same false claim and is fixed too.
- **§1 said two scene structs; there are three.** `Terra` (`lib.rs:5140`, docs/43 worlds-as-data) has its
  own 5-file submodule (910 lines). §1 also said `OrbitDemo` has "No GPU compute — all particle physics is
  CPU"; it owns a `gpu_sph::GpuSph` (`:2828`) driving `sph_step.wgsl`.
- **§2 omitted `eos.rs`, `hydrostatic.rs`, `gpu_sph.rs`, `accretion.rs` entirely** — 2,738 lines of the
  realignment's physics core, invisible to anyone navigating by this map. Added as §2b.
- **§7 listed 5 shaders (there are 9) and 1 tool (there are 6).** Added `sph_step.wgsl`, `bh_gravity.wgsl`,
  `sph_render.wgsl`, `globe.wgsl`; `sph-verify`, `gpu-bh-verify`, `bake-earth`, `impact-run`, `shot-server`.
- **§6's canonical trace had all 8 anchors stale** by 500–800 lines (`start_birth` 2897→3447, `step_substep`
  3243→3968, `step_block` 3430→4155, …). Re-verified, and documented that a second GPU-SPH path now
  coexists with the CPU aggregate one in the same scene.
- **§4:** four integrators → **six**; one WGSL mirror seam → **three**; added fork 7 (**three**
  terrain-contact implementations of one law — `granular::terrain_contact_resolve:310` with exactly one
  production caller, the WGSL mirror, and `matter.rs:872-887`'s normal-free snap+`CONTACT_DAMP` — plus a
  fourth voxel resolver in `body.rs:55`) and fork 8 (`AirField`, a container fork with zero consumers).
- **§8 deleted.** It restated CLAUDE.md's hard rules and had already drifted from them ("next is docs/34";
  a stale test count). One question, two answers — the doc-level form of what docs/46 forbids in physics.
  It now points at CLAUDE.md. Test count corrected there: ~145 → 204 written, 186 running by default (18
  `#[ignore]`d), and `gpu_sph.rs` has no in-crate tests at all.

**A pattern, beyond the individual fixes.** Three corrections are the shape docs/48 named — built, verified,
wired nowhere. `bh_gravity.wgsl` is verified standalone and `sph_step.wgsl` still runs direct O(N²) gravity.
`accretion::accrete()` has no non-test caller, so the disk can be *measured* for gravitationally bound
clumps and cannot *grow* one — which is exactly the "the disk never accretes a Moon" diagnosis docs/33
recorded, with the operator now built and still outside the loop. `world.rs`'s `demote_column_to_field` /
`column_is_bakeable` appear only in tests. Recorded in docs/33's new status section so the next session
inherits it.

**Verified.** Every count, anchor, and wiring claim above was read off `main` at `1b4381e` (module
inventory, `git grep` for callers, `git log` for the staleness claim). Docs-only change — no code touched,
so no CHANGELOG entry.

**NOT verified — stated rather than implied.** The per-symbol `:NNN` anchors inside §2/§3's existing bullets
were not exhaustively re-checked; only the ones quoted in this entry, plus the module line counts, were
confirmed. Given §6's anchors had all moved by ~500 lines, assume others in those sections have drifted too
and re-check before relying on one. The header now says so.

---

## 2026-07-19 — the probe gets real traction: terrain contact swapped onto the honest constraint

**What.** `Engine::collide_probe_with_terrain` now resolves against `granular::terrain_contact_resolve`
— the same non-injecting constraint the GPU grains use — instead of its own hand-rolled path. Deletes
two fudges in one move: the tangential `p.vel.x *= 0.5; p.vel.z *= 0.5` velocity multiply, and the
`DEAD: f64 = 0.15` dead-zone/half-correction hack. Adds `World::surface_bilinear_grad` returning
`(h, ∂h/∂x, ∂h/∂z)`; `surface_height_bilinear` now delegates to it, so there is exactly ONE bilinear
surface implementation on the CPU.

**Why.** `vel *= 0.5` is a raw velocity scale — blind to the normal load, to μ, and to the surface
itself. It cannot express traction, which is the whole content of driving: Coulomb friction bounded by
`μ·jn`, i.e. a harder-pressed contact grips more. The constraint already implements exactly that, is
unit-tested, and is hardware-verified (gpu-verify K/L/N) — it was simply only wired to grains. μ now
comes from the terrain material's own datum, so ice is slippery because ice's data says so.

The gradient is the load-bearing addition: without `∂h/∂x, ∂h/∂z` there is no surface normal, so a body
on a slope was treated as sitting on a flat floor and there was no normal impulse to bound friction with.

**Verified.**
- 178 tests pass (was 174). Four new in `world.rs`: the height is byte-identical through the refactor;
  the gradient matches a central difference of its own field; flat ground reports zero gradient; and
  **friction responds to μ** — ice retains more speed than basalt through the same contact, which the
  old multiplier could not express at any μ.
- New rig `web/rig/probe_traction.mjs` (engine telemetry, not pixels), run against BOTH this branch and
  a detached baseline worktree at `main` — because the claims below are comparative and would otherwise
  be assumptions:

  | | baseline (`main`) | this branch |
  |---|---|---|
  | settled altitude | 0.60 m | **0.50 m** = `PARTICLE_HALF` |
  | time to settle | ~3.5 s | **~30 s** |
  | probe integrity | 100% | **100%** |

  **integrity 100% on both** — the bonds never broke, so the bounded velocity-decoupled projection does
  not pump the lattice the way a hard snap would, and the dead zone is genuinely unnecessary rather than
  merely removed. That was the regression risk and it is clear.

**Also removes the probe's damping fudge.** `probe.damping = critically_damped(0.4)` — a ζ picked so the
ball "settles rigidly" — is replaced by ζ derived from iron's own restitution via the new
`granular::zeta_for_restitution` (factored out of `damping_for_restitution`, which now calls it, so bond
damping and contact damping cannot drift apart). The old 0.4 implied e ≈ 0.254; iron's data says e = 0.6
⇒ ζ ≈ 0.16, so the probe was modelled ~2.4× less bouncy than iron is.

**Measured across three variants** (rig `probe_traction.mjs`, settle = first time the trace stays within
0.05 m of its final value):

| variant | settles | final alt | first rebound |
|---|---|---|---|
| `main` — fudged friction, ζ=0.4 | **6.5 s** | 0.6 m | 0.7 m (bounce crushed flat) |
| honest friction, ζ=0.4 | **35.0 s** | 0.5 m | 4.5 m |
| honest friction + honest ζ | **35.5 s** | 0.4 m | 3.9 m |

Integrity **100% in all three** — the honest, lower damping did not destabilise the explicit integrator
(the documented danger was OVER-damping, docs/23), so the stability concern behind ζ=0.4 was unfounded.

**Three findings, two of which refuted what this entry first claimed.**

1. **The resting height shift is the SURFACE, not the dead zone.** An earlier draft said the old
   `DEAD = 0.15` let the probe rest "up to 0.15 m sunk". Measurement refutes it: baseline rests *higher*
   (0.60 m). The probe now reads `surface_bilinear_grad` — four columns, bilinear, the SAME field the GPU
   grains collide against — where it previously read a single `surface_top_voxel`. Probe and debris now
   agree on where the ground is; they did not before. That is the real improvement.
2. **The damping derivation costs ~no settle time**, and I predicted the opposite. 35.0 → 35.5 s is inside
   the sampling interval, and the first rebound is *lower* (4.5 → 3.9 m): a springier lattice puts impact
   energy into internal modes rather than one coherent bounce.
3. **The tangential fudge was doing essentially all the settling work** — 6.5 s → 35.0 s at unchanged ζ.
   `vel *= 0.5` per substep is a ~50%-per-step damper; it was not a small approximation to friction, it
   was the mechanism that made the scene settle at all. Removing it exposes the lattice's real ringing.
   **35 s is the honest settle time; 6.5 s never was one.** It was manufactured by a ~50%-per-substep
   velocity damper standing in for friction — self-flagged in its own comment as *"crude; emergent
   friction is future"*, i.e. known debt, not a design to preserve. So this is a behavioural CHANGE to a
   deployed scene, not a regression against a correct baseline: nothing that depended on 6.5 s was
   depending on physics. What is real is the consequence — anything assuming a quick settle
   (de-resolution timing, UX) now faces a ~35 s transient, and bounding it belongs to the docs/44
   demotion criterion (demote on *irrelevance* — contact stress below yield — not on silence), never to
   re-tuning a dial back until the number looks familiar.

**NOT verified — stated rather than implied.** Traction is demonstrated at the law level (unit test) and
the fudge is gone, but nothing in the scene *drives*, so in-scene grip is unexercised. A probe dropped
straight down produces no tangential load. That waits on a driven body.

**Flagged.** μ is taken from the surface material alone; no pair-combining rule between a body's
material and the ground's exists yet (the same gap `gpu_step_params` flags for mixed-material debris).
The first sampling window (12 s) read the still-ringing transient as a failure to settle — the landing
transient runs ~30 s, so `SAMPLES` is env-tunable on the rig.

---

## 2026-07-19 — iPhone 15 Pro Max: a latency/throughput CROSSOVER, and the same physics on a third device

**What.** Third entry in the cross-vendor matrix (iPhone 15 Pro Max, A17 Pro, Metal), run through
`/gpu-probe.html` on the LAN dev server.

**Correctness — three devices, two backends, one answer.** At N=60,000 all of Vulkan/RTX 2070,
Metal/M4 and Metal/A17 report `tot = 1.585e+7` and `vmax = 30.945`; at N=1 all report
`tot = 4.179e-8`. No energy injection anywhere. The four-separate-passes race mitigation holds on
every device tested.

**The finding — a latency/throughput crossover between N=1,000 and N=10,000.**

| N | 2070 (Vulkan) | M4 (Metal) | A17 Pro (Metal) | iPhone vs 2070 |
|---|---|---|---|---|
| 1 | 1.25 ms | 0.540 ms | 0.613 ms | **2.0× faster** |
| 1,000 | 1.83 ms | 0.833 ms | 1.113 ms | **1.6× faster** |
| 10,000 | 2.23 ms | 1.553 ms | 2.793 ms | 0.8× (slower) |
| 60,000 | 13.40 ms | 10.317 ms | 16.017 ms | 0.84× (slower) |

A phone BEATS a desktop discrete GPU below the knee and loses above it. The A17 Pro is the ideal probe
for this because it has Apple's latency advantage with much less throughput, so the two effects
separate. The ratios confirm the mechanism quantitatively — A17 Pro has a 6-core GPU vs the M4's 10
(a 1.67× core ratio):

- **N=60,000: M4/A17 = 1.55×** ≈ the core ratio ⇒ throughput-bound, core count predicts the gap.
- **N=1: M4/A17 = 1.14×** ≪ the core ratio ⇒ latency-bound, core count nearly irrelevant.

Same silicon family, same backend, two limiting regimes, crossover at the knee. This is the §7
saturation-knee argument (`gpu-perf`) showing up as hardware ranking that REVERSES with N — a single
benchmark point would have ranked these devices wrong in either direction depending on which N it
happened to pick.

**Product consequence — the phone's practical particle budget is well under `MAX_PARTICLES`.** At
N=60,000 physics alone costs 16.0 ms, a ~62 fps ceiling with essentially nothing left for rendering
inside a 16.67 ms frame. At 0.267 µs/particle, keeping physics to about half the frame budget implies
roughly **30,000 grains on an A17-class phone** (vs 60,000 viable on the M4). Not a bug — a real
device-tier limit to design scenes against.

**This raises the priority of the O(table) grid clear.** Its ~0.53 ms/frame is FIXED regardless of N,
so it is proportionally most expensive exactly where Apple hardware is otherwise strongest (small N),
and it eats a bigger share of a tighter phone frame budget. The epoch-tag fix is output-neutral and
now has a clear beneficiary.

**Unchanged limits:** Safari masks every `GPUAdapterInfo` field to the literal string `apple` on this
device too, so "A17 Pro" is the operator's knowledge of the hardware, not a probe measurement.
`max_buffer_size` is 1024 MiB, same as the iPad — the N=60,000 run completed without hitting it
(a prior concern that iOS Safari's tighter per-tab memory limits might kill the run did not
materialise).

---

## 2026-07-19 — FIRST NON-VULKAN RESULT: the engine's granular step runs correctly on Metal (iPad Pro)

**What.** Ran `/gpu-probe.html` on an iPad Pro (M4) over the LAN HTTPS dev server. First time any part of
this engine's GPU physics has been executed on a non-Vulkan backend, and the first entry in the
cross-vendor matrix.

**The correctness result — Metal and Vulkan agree to 4 significant figures.** `lib.rs` (~line 2118)
splits the four granular stages into four separate compute passes specifically because fusing them
"happened to work on desktop Vulkan (the 2070) but can RACE on other backends (e.g. Metal / the M4)".
That mitigation was written defensively and had **never been exercised on Metal**. It holds:

| N | Vulkan (2070) tot / v max | Metal (M4) tot / v max |
|---|---|---|
| 1 | 4.179e-8 / 0.000 | 4.179e-8 / 0.000 |
| 1,000 | 2.981e+3 / 6.012 | 2.875e+3 / 6.104 |
| 10,000 | 9.580e+5 / 31.019 | 9.546e+5 / 31.022 |
| 60,000 | 1.585e+7 / 30.945 | 1.585e+7 / 30.945 |

No energy injection at any N — a race would show as a rising total. Note the N=60,000 row is identical
to four significant figures in BOTH total energy and max speed, and the Vulkan side reproduced those
same figures across repeated runs. So this probe configuration appears **reproducible in a way
`gpu-verify`'s scene I is not** (bulk settling rather than marginal stability). That strengthens the
comparison, but does NOT retire the determinism work — a *small* Metal anomaly would still be
indistinguishable from drift.

**The performance result — the iPad beats the desktop RTX 2070 at every point.**

| N | browser 2070 | browser M4 | M4 advantage |
|---|---|---|---|
| 1 | 1.25 ms | 0.540 ms | 2.3× |
| 1,000 | 1.83 ms | 0.833 ms | 2.2× |
| 10,000 | 2.23 ms | 1.553 ms | 1.4× |
| 60,000 | 13.40 ms | 10.317 ms | 1.3× |

The advantage is LARGEST at small N and shrinks as N grows — the signature of much lower per-dispatch
latency (unified memory, no PCIe round trip), not raw throughput. Product-relevant consequence: at
`MAX_PARTICLES` = 60,000 the M4 sustains 10.3 ms/frame, a ~97 fps physics ceiling, so **the engine's
full particle budget is viable on an iPad**.

**Limits of what this proves (stated rather than glossed).**
- **The probe did not identify an "M4".** Safari masked every `GPUAdapterInfo` field to the literal
  string `apple` — vendor, architecture, device and description are all `apple`. It establishes Apple
  GPU ⇒ Metal (iPadOS WebGPU has no other backend) and `fallback: no` rules out a software adapter.
  The specific chip is Robin's knowledge of her hardware, not a probe measurement. Do not quote the
  probe as the source for "M4".
- **`max_buffer_size` is 1024 MiB on the iPad vs 4096 MiB on desktop.** Not binding here (the largest
  buffer at N=60,000 is the 8× render buffer at ~31 MB), but a 4× smaller ceiling to respect when
  scaling up.
- The page's "per-particle cost falls 3141×" line is dominated by the N=1 point, which is pure launch
  overhead. The real knee sits between N=1,000 and N=10,000.

---

## 2026-07-19 — a browser GPU probe, and the same wrong-GPU bug confirmed in the browser

**What.** `GpuProbe` (`crates/engine/src/lib.rs`, wasm-only) + `web/gpu-probe.html` /
`web/src/gpu-probe.ts` + `web/rig/gpu_probe.mjs`: a compute-only probe that runs the REAL
`particle_step.wgsl` through the REAL `GpuParticles` (no canvas, no surface, no reimplementation) and
reports (1) which adapter actually ran, (2) per-frame cost across N = 1 … 60,000, (3) whether total
energy stays bounded. Two-phase like `begin_readback`/`take_readback` — `start_run` submits, JS polls
`poll()` — because a browser cannot block on a buffer map. Also fixes two `scripts/dev-lan.sh` bugs
(below) and registers the page in `vite.config.ts` (an unregistered page works in `dev` and silently
vanishes from `build`).

**Why.** The engine ships to browser WebGPU across vendors, but nothing in `web/` ever touched
`navigator.gpu` beyond an existence check, and `Engine::create` (`lib.rs:321`) requests an adapter
with `HighPerformance` and never reports what it got. So a browser run was silent about the hardware
that produced it — the same ambiguity PR #11 fixed natively. Robin has an iPad Pro (M4); this is the
first step of a growing cross-vendor matrix (AMD / Apple / Arc).

**Verified (desktop Chromium over Vulkan, xvfb).** Probe reproduces the native baseline on the SAME
card, which is what validates the probe itself before it meets unfamiliar hardware:

| N | native 2070 (gpu-verify) | browser 2070 (probe) |
|---|---|---|
| 1 | 1.58 ms | 1.25 ms |
| 1,000 | 1.91 ms | 1.83 ms |
| 10,000 | 3.86 ms | 2.23 ms |
| 60,000 | 14.4 ms | 13.40 ms |

**Energy invariant holds on Vulkan** — fixed N = 10,000, increasing frames, total energy must never
rise: `1.83e6 → 1.31e6 → 1.99e5 → 1.37e5` over 60/120/240/480 frames, KE decaying to 37.8 and
`vmax` 0.65 (settled). This is the reference the M4 run will be compared against; a backend race
would show as rising energy.

**Two findings that change how browser results must be read.**

1. **wgpu's `AdapterInfo` is EMPTY in a browser.** Under `Backends::BROWSER_WEBGPU` wgpu delegates to
   the browser and cannot see the driver: `get_info()` returns no name, no driver, and
   `backend: BrowserWebGpu`. It can never tell you whether you are on Metal. The authoritative source
   is the browser's own `navigator.gpu` → `GPUAdapterInfo.vendor` / `.architecture`. The probe now
   reports BOTH and the rig prints the browser's.
2. **The browser picked the WRONG GPU too — and you cannot override it.** With
   `powerPreference: "high-performance"`, Chromium reported `vendor: nvidia, architecture: turing` —
   the RTX 2070, not the 5060 Ti. Corroborated independently by timing (13.4 ms at N=60k matches the
   2070's native 14.4 ms, not the 5060 Ti's 5.67 ms). Chromium's `--gpu-vendor-id` / `--gpu-device-id`
   flags did NOT move it. **WebGPU exposes no adapter enumeration at all** — `requestAdapter()`
   returns one adapter and the spec offers no way to choose — so unlike the native harness, which can
   now refuse to guess, in a browser the only available defence is to RECORD which GPU you got. That
   is precisely what this probe does, and why its provenance output is not optional decoration.

**Not achievable on this host (stated rather than quietly dropped):** reproducing the 2.5×
5060-vs-2070 gap *in the browser*. Chromium cannot be pointed at the second card, so the browser leg
is validated against the 2070 only.

**`scripts/dev-lan.sh` — two bugs fixed.** (1) The readiness probe grepped the served `/` for
`greenfield`, which appears nowhere under `web/` (it survives only as a wgpu device label in Rust and
never reaches the HTML), so the script never reused a running server and always exited 1 after a
perfectly healthy start; it now greps a `SENTINEL` that is actually in `index.html`. (2) `needs_build`
searched only `crates/` and `data/` for `*.rs|*.toml|*.json`, missing `shaders/**.wgsl` — but every
shader is `include_str!`'d into the wasm, so editing one changed the binary while the script reported
"✓ wasm up to date" and served the OLD shader. Silently stale results are the worst possible failure
for on-device verification, which is exactly what this script exists for.

**Known cosmetic gap:** every page 404s `/favicon.ico` (the repo ships no favicon). Pre-existing,
affects all pages equally, not introduced here.

---

## 2026-07-19 — gpu-verify was verifying on the wrong GPU (and is not run-to-run reproducible)

**What.** `tools/gpu-verify` selected its device with `request_adapter(PowerPreference::HighPerformance)`.
On a host with two *discrete* NVIDIA cards that preference cannot discriminate — it silently took whichever
Vulkan enumerated first. Replaced it with `pick_adapter()`: `GPU_VERIFY_ADAPTER` (case-insensitive substring
of the adapter name) selects explicitly; with exactly one non-CPU adapter present that one is used; with
several and no variable set it **panics rather than guessing**, listing what it found. The chosen adapter,
its device type, and the driver version now print on every run, so a log always records which silicon
produced it. `tools/gpu-verify/.cargo/config.toml` supplies the host default via cargo's `[env]`
(`force = false`, so a real env var still wins). CPU adapters (Mesa llvmpipe) are filtered out — they are
not verification targets.

**Why.** A verification harness that quietly changes hardware is worse than one that fails: every prior
"PASS" carried an unstated assumption about which GPU produced it. Capability-based auto-selection was
considered and rejected on evidence — both cards report *identical* `wgpu` limits (`max_buffer_size`,
workgroup dims), so there is nothing to choose on. Explicit-or-refuse is the only honest option.

**Verified.** All four paths exercised: default via cargo → `adapter: NVIDIA GeForce RTX 5060 Ti
(DiscreteGpu, 580.173.02)`; `GPU_VERIFY_ADAPTER=2070` → the 2070; no variable + two GPUs → panics with
`2 discrete GPUs present (…) — refusing to guess`; unmatched name → `matched no adapter; available: …`.
Full suite run on both cards: **same 25 PASS / 2 scene FAIL on each** (the pre-existing scene-D repose
deficiency and scene-J impact-energy failure — unchanged by this work, not addressed here).

**Recorded, not fixed — the harness is nondeterministic.** Comparing the two cards showed small numeric
drift, so the same card was run twice: it drifts *by the same magnitude against itself*
(`I energy-conservation: E 16303→-2684→-6490` vs `16303→-2670→-6480`; scene E spread 21.3 m vs 21.0 m).
So the cross-card deltas are **not** architectural divergence — both are the same underlying
nondeterminism, most likely order-dependent float accumulation in the GPU force/neighbour reduction.
This matters because scene I is the FUDGE DETECTOR: its margin is currently larger than its
reproducibility. Worth a determinism pass before any number from this harness is quoted as exact.

**Timing (informational, not a benchmark).** Full suite 65.7 s on the 5060 Ti vs 79.4 s on the 2070
(~17% faster). Single samples of a wall-clock that includes shader compilation and CPU-side setup —
this harness is not GPU-bound, so do not read it as a measure of the cards. See the next entry: that
17% is an artifact of the harness's scale and says nothing about the engine.

---

## 2026-07-19 — the 17% was the harness, not the hardware: gpu-verify runs 1–5 particles per scene

**What.** Chased why a 3-generation-newer GPU only won 17% on the suite. `GPU_VERIFY_STATS=1` (added
to `simulate`, stderr-only) dumps the workload shape. The harness's real distribution over 458
sim-calls: **219 calls at 1 particle, 205 at 5, 11 at 2** — i.e. ~95% of calls dispatch a SINGLE
workgroup with 63 of 64 lanes idle. Only one call reaches 13,456 particles. Meanwhile every substep
clears the whole `TABLE_SIZE` grid regardless of N. Totals for one suite run: **1,036,448 submits,
4,145,792 dispatches, 33.96 G threads in CLEAR vs 0.90 G in physics (37.5 : 1)**. At ~16 µs of
launch latency per dispatch that accounts for the runtime — the suite measures driver launch
overhead, not the shader.

**Why it matters.** The harness's scale is not the engine's, and the two batch differently:
gpu-verify creates an encoder and **submits per substep**, while `Engine::step_physics` records all
`DEBRIS_SUBSTEPS` into **one** encoder and submits once per frame. A perf conclusion drawn from this
harness does NOT transfer to the engine — which is exactly the error the 17% invited.

**Verified — at engine scale the new card is 2.5× faster.** Benchmarked the real
`shaders/particle_step.wgsl` at the engine's configuration (`GRID_TABLE_SIZE = 1<<18`, 16 substeps in
one encoder, one submit), 3 warmup + 20 timed frames, both cards:

| N | RTX 2070 | RTX 5060 Ti | speedup |
|---|---|---|---|
| 1 | 1.58 ms | 1.24 ms | 1.27× |
| 1,000 | 1.91 ms | 1.50 ms | 1.28× |
| 10,000 | 3.86 ms | 2.26 ms | 1.71× |
| 60,000 (`MAX_PARTICLES`) | 14.4 ms | 5.67 ms | **2.55×** |

Reproduced across reps (5060 Ti 5.50/5.67/5.72 ms; 2070 14.11/14.39/14.45 ms). The advantage grows
with N exactly as expected once the workload saturates the wider GPU. `nvidia-smi dmon` during a
suite run: `sm` 70–88%, **`mem` 0%**, `fb` < 100 MB — not bandwidth-bound, working set trivially
small. (`sm%` only means ≥1 warp resident; it is not saturation.)

**Recorded, not fixed — the grid clear is O(table), not O(N).** `cs_grid_clear` dispatches
`GRID_TABLE_SIZE = 262,144` threads (4,096 workgroups) every substep independent of particle count,
measured at **~0.53 ms per 16-substep frame on both cards** (flat in N). That is ~9% of frame time at
N=60,000 and ~30% at N=1. Candidate fixes: an epoch/generation tag per cell (compare a frame counter
on read, never clear), clearing only cells touched last frame, or sizing the table to live N —
`GRID_TABLE_SIZE` is currently 4.4× `MAX_PARTICLES` though the comment at lib.rs:125 says "≥ ~2×".
Not changed here: this branch is the adapter fix, and a grid-lifecycle change needs its own docs/NN
and re-verification.

**An invalid ablation, recorded so it is not repeated.** First attempt to price the clear simply
removed the pass and re-timed — it came out **6× SLOWER** (36.5 ms vs 5.67 ms at N=60,000). Removing
the clear does not remove work: `grid_count` then accumulates across substeps and `cs_forces` walks
saturated `bucket_k`-deep buckets. It measured a different, worse simulation. A negative measured
cost is the tell. Stage cost was taken from the clear running alone instead.

---

## 2026-07-19 — Worlds-as-data #2: the Space + Two Moons deorbit scenes are now DATA (docs/43)

**What.** The second worlds-as-data consumer, proving the schema generalizes from a static planet (Terra) to
**dynamic N-body scenes**. Extended the one `World` schema (`terra/world_def.rs`) with a `type:"system"` variant:
a `bodies[]` array (each `{name, role: star|planet|moon, mass_kg?/radius_m?/profile?, pos_m, vel_ms,
spin_period_s?, tint?}`) and orbit-camera fields (`yaw/pitch/zoom/focus`). New `OrbitDemo::load_world(json)`
(mirrors `Terra::load_world`) replaces the built-in Sun/Earth/Moon constants with the declared initial
conditions, spin, composition-derived tints, time scale, and frame-of-reference focus. New world files
`web/public/worlds/{one-moon,two-moons}/world.json`; `web/src/orbit.ts` now reads `<body data-world="…">`,
fetches the JSON, derives the moon count, and calls `create` + `load_world`. **Birth of the Moon** (GPU-SPH
impact) stays on the code path for now. The **deorbit stays a pure user control** (`brake_moon` ×½ / `drop_moon`
×0 of the moon's Earth-relative velocity) — the crash emerges from the N-body integrator + swept contact, no
scripted outcome.

**Why.** Terra was built as the reference worlds-as-data scene; the strategic payoff is a SECOND, structurally
different scene on the same contract — it confirms the schema (bodies + orbital ICs + events-as-controls)
generalizes, and turns "add/alter a scene" into editing data, not scene code (docs/43, the recorded near-term
TODO). `planet` is now `Option` on `World` (a system world has no single planet); `Terra::load_world` errors
cleanly if its `planet` section is missing.

**Verified (rig `worlds_space`, xvfb).** Space loads from `one-moon/world.json` — HUD reads the declared data
exactly: Earth–Moon 384,768 km (=MOON_DIST), v 1.02 km/s (=MOON_SPEED), Earth day 23.9 h (=sidereal spin), frame
Earth (=camera.focus), time ×118,000 (=time.scale). **Deorbit works through the data path:** `drop_moon` → the
moon falls 384,768 → 8,108 km and **impacts, spawning 1,536 debris particles**. Two Moons loads
`two-moons/world.json` — "4 bodies, 2 moon(s)". Render path is unchanged, so visuals match the pre-migration
scenes. Full fast suite **174/174 green** (+1 system-world parse test). TS typechecks.

---

## 2026-07-19 — FIX: the Terra "growing black void" on descent (Robin caught it) — globe back-face culling

**What.** Robin: flying in toward Earth, a black circle appeared at nadir around ~250 km altitude and grew to fill
the screen as he descended — "a void, I can see nothing through it." Root cause: the displaced globe was drawn
with **back-face culling**, and the fly camera — sitting just above the surface looking *down* — had its near
(front-facing) globe triangles culled, leaving the clear colour (the void); the limb (grazing triangles) still
rendered, so it read as a growing disc. The Phase-3 orbital camera looked at the planet *centre* from far away and
happened not to trip it, so it lay hidden until the fly camera shipped. Fix: **no culling for the globe/cap
pipeline** — the globe is convex, so the depth buffer alone gives correct occlusion; drawing both sides is robust
regardless of winding and costs only a few extra fragments. Also tightened the camera's near/far (dropped the
`far = near×1000` inflation; `near` is now a large fraction of the altitude at height, tiny near the ground) so the
globe's far hemisphere stays cleanly depth-occluded now that culling is off, and depth precision is far better.

**Why it was invisible in the rig at first.** The headless software GPU (ANGLE/llvmpipe) tolerated the original
setup; the bug showed on Robin's real GPU. Diagnosing it end-to-end (clip vs depth vs cull) in the rig — depth
`Always` still voided, `cull_mode: None` filled it — pinned it to culling, and reproduced/fixed it in software.

**Verified (rig `terra_depth`, xvfb).** Over the SUB-SOLAR point (fully day-lit nadir, so a void can't hide as
night side): orbit 6000 km, 500/259/250/100/45 km, and 1.5 km all render the **full lit surface — no void**, with
correct occlusion (near hemisphere only, no back-face bleed-through). `terra_globe`/`terra_fly` regression rigs
clean (full Earth; W moves north, orbital drag orbits, ground drag free-looks). 173/173 fast tests green.

---

## 2026-07-19 — Terra Phase 6: data-driven controls + HUD polish (the worlds-as-data controls contract)

**What.** The Terra scene's key bindings now come from the WORLD FILE, not code: `world.controls.keys` maps a
`code` → an `action` (`forward`/`back`/`left`/`right`/`up`/`down`), and `web/terra.ts` builds the input handler
from that map — the docs/43 worlds-as-data controls contract, closing the loop (the JSON populates the scene AND
its controls). Earth's world declares WASD move + R/F climb/descend; changing the bindings needs no code change.
The controls hint in the HUD is derived from the actual bindings, so it can't drift. HUD polished to show
`world · altitude · lat/lon · biome · fps` — new `Terra::ground_biome()` reads the surface type under the camera
(the land-cover biome material id, or "ocean"). fps is smoothed in the host.

**Verified (rig `terra_controls`, xvfb).** From the world bindings: **KeyR climbs, KeyF descends, KeyD moves east**
(lon increases); biome readback is "ocean" over the mid-Pacific and "sand" over the Sahara; the HUD line renders
`Earth · alt 1.5 km · lat 28.00° lon 84.00° · sand · 28 fps` + `WASD fly · R/F alt · wheel zoom · drag look`.
TypeScript typechecks clean; full fast suite **173/173 green**.

**Deferred (noted).** Optional planet rotation from `time{}` — parked: it conflicts with the lat/lon fly-camera
model (rotating the planet vs. the camera's surface coordinates) and Earth's world declares `rotation: false`;
revisit alongside the multi-epoch / pre-baked-until-collision work (task: worlds-as-data). This completes the
docs/43 terrain rework Phases 1–6: a navigable, data-defined Earth you fly from orbit to the ground.

---

## 2026-07-19 — Terra Phase 5: the fine ground cap (real-ratio terrain, true horizon, camera-relative)

**What.** New pure module `terra/ground_cap.rs` — a high-resolution local patch of the surface rebuilt under the
camera each frame (`fill_ground_cap`, 192² grid, denser toward the centre), sampling the SAME surface as the globe
(real elevation, biome albedo) and curving to a true horizon. It is emitted CAMERA-RELATIVE (`surface − eye` in
display units, in f64 then cast to f32), so ground detail survives the radius-1 globe — the precision fix the plan
called for. `FlyCamera::view` now returns both the absolute view·projection (globe) and a camera-relative one
(eye-at-origin, for the cap) plus the tangent frame + horizon distance. `Terra` builds the cap into a persistent
writable vertex buffer and cross-fades it over the globe (alpha-blended, `tint.a`) as altitude drops (`cap_fade`:
0 above 40 km → 1 below 15 km). The cap covers ~1.3× the horizon angle so its far edge sits below the horizon (no
visible boundary), lifted a few metres so the fine cap sits in front of the coarse globe.

**Exaggeration unified + made a declared dial.** The globe, cap, and camera floor now share one relief factor,
read from `surface.relief_exaggeration` (default 1.0 = true scale) — an honest visualization dial, not a physics
fudge. Set Earth's to **1.0**: real-ratio relief. This retires the ×30 hack that made ground flight impossible
(Phase 4's buried-black), at the cost of a flatter — but photorealistic — orbital globe. The camera floor
neighbourhood tightened to ±0.2° (~22 km) now that terrain is real-scale.

**Verified (rig `terra_ground`, xvfb).** A full orbit→ground descent over the Himalaya + a coastline: orbital =
a realistic smooth Earth (continents, biomes, terminator, limb); 35 km = the curved limb with terrain fading in
cleanly (no seam / z-fight ghosting); 6 km / 1.5 km / 300 m = a real ground-level horizon — tan foreground, green
foothills, snow peaks at the true horizon, black sky — **no burying**; the coast shows land meeting a blue ocean
wedge. Full fast suite **173/173 green** (+2 ground_cap tests: counts/index bounds; centre vertex sits directly
below the eye at the camera height).

**Honest limits (the plan's noted follow-ons).** Terrain is smooth — detail is capped by the 2048×1024 ETOPO
raster (~20 km/texel); no sub-raster fbm micro-detail yet. The cap is a single tangent patch, not yet a
screen-space-error quadtree with geomorphing + edge skirts. Relief is real-ratio (dial = 1.0); a normal-only
exaggeration could add orbital relief pop without breaking ground.

---

## 2026-07-19 — Terra Phase 4: the continuous fly camera (orbit ⇄ ground), physics-floored on terrain

**What.** New pure module `terra/fly_camera.rs` — ONE camera that blends orbit⇄ground by altitude (no mode
switch): high up it looks down at the planet and a drag orbits; near the ground it looks along the horizon and a
drag turns the view; a smoothstep on altitude (`GROUND_ALT`=3 km … `ORBIT_ALT`=400 km) cross-fades the forward and
up vectors between the two. State is `{lat, lon, alt_m, yaw, pitch}` in f64; the whole view·projection is built in
f64 (`DMat4`, cast to f32 only at the end) so ground framing survives the radius-1 globe. Near/far planes scale
with altitude (near ∝ altitude-above-ground; far just past the horizon). New `Terra` wasm API replacing the orbit
stub: `set_fly` · `move_tangent` (WASD, step ∝ altitude) · `zoom_alt` (wheel) · `drag_look` · readbacks
`altitude_m/latitude/longitude`; seeded from the world file's `camera{}`. `web/terra.ts` rewritten to drive it
(held-key WASD, wheel zoom, pointer-drag look) with a live lat/lon/alt HUD.

**Physics floor (Robin's constraint: the camera must never pass through solid).** `alt_m` is height above the
LOCAL terrain — `eye = up·(r_disp + ground_disp(lat,lon) + alt_m·ds)` — and `ground_disp` is the MAX terrain
height over a ±0.5° (~55 km) neighbourhood. So the eye always clears the terrain *envelope*, never buries inside a
neighbouring peak, and is **forced upward as it approaches rising terrain** (terrain-following with ~55 km
look-ahead). Recorded the standing rule + the follow-ups to memory: tighten to a per-triangle collision in Phase 5,
and — for caves/arches — move collision from a heightfield floor to a VOLUMETRIC "is this point in solid matter?"
test against the material field (docs/39/42), since a heightfield can't represent voids or overhangs.

**Verified (rig `terra_fly`, xvfb).** Functional readbacks: **W moves north** (Δlat > 0), **orbital drag orbits**
(Δlon ≈ 50°), **ground drag does NOT move position** (Δ ≈ 0 — free-look, the altitude blend working). Visual
orbit→ground sequence: clean globe at 8000 km → curved horizon with snow peaks + green foothills + tan plains at
80 km → a mountainous ground-approach horizon at ~1.5 km that is **no longer buried/black** (the terrain-envelope
floor fixed a first cut where the ×30-exaggerated coarse mesh swallowed the eye). Full fast suite **171/171 green**
(+5 fly_camera tests: tangent-frame orthonormality, blend monotonicity, orbital-vs-horizon look, zoom/move clamps).

**Honest limit.** True sub-km ground horizon detail is coarse here (39 km mesh triangles, ×30 relief) — the
real-ratio fine ground cap is Phase 5, exactly as the plan sequences it. Phase 4's deliverable is the camera
system, and it flies orbit→ground continuously.

---

## 2026-07-19 — Terra Phase 3: the displaced cube-sphere globe (a real blue-marble from world.json + rasters)

**What.** The `Terra` scene (docs/43, worlds-as-data) now renders a smooth **displaced cube-sphere globe** instead
of the Phase-2 grain shell. New pure module `terra/globe_mesh.rs` (`build_globe(res, r_disp, sample)`): 6 cube
faces, each a res×res grid projected to the sphere, every vertex displaced radially by the sampled surface offset
and coloured by its biome albedo; normals come from central differences of the *displaced* grid so relief reads as
shaded terrain. `Terra::build_surface_mesh` drives it from the real rasters — land cells lifted by ETOPO elevation
(×30 exaggeration so relief reads on a radius-1 globe) and coloured by the land-cover biome material; **ocean cells
sit flat at sea level with the water material**, integrated into the same mesh (no separate ocean shell, so no
coast z-fighting). New `shaders/globe.wgsl` + `build_globe_pipeline`: per-vertex biome colour × tint, `SUN_GAIN=22`
Reinhard day side (black night side, emergent terminator), plus a cheap view-dependent blue Fresnel **atmospheric
limb**. Built once in `load_world` (256² per face → 780,300 triangles); the grain shell stays as the fallback until
a world's rasters load.

**Why.** Phase 3 of the terrain rework (the plan): retire the grain shell for the Earth scene and deliver the
Google-Earth look. The grain shell proved the data path (Phases 1–2); a displaced mesh is the render surface the
fly camera (Phase 4) and ground LOD (Phase 5) build on. Ethos-consistent for v1: the surface is un-particalized
bulk, and the engine already renders un-materialized bulk as a smooth object — grains return where a region is
*resolved* (the JIT-particalize seam, docs/39/42).

**Verified.** `globe_mesh` unit tests (counts + index bounds; undisplaced = a unit sphere with outward normals;
displacement pushes vertices out by the offset) + full fast suite **166/166 green**. Rig `terra_globe` + rotated
angles (`xvfb-run`): an unmistakable Earth — Africa/Mediterranean/Arabia, the snow-capped Himalaya and Andes with
raised relief, the tan Sahara, a green temperate belt, Antarctica, a blue day-side ocean darkening through the
terminator, and the atmospheric limb — all from `world.json` + the baked Natural Earth / ETOPO / land-cover
rasters. Winding correct (convex front faces, back-culled). Land fraction 0.335.

---

## 2026-07-18 — FIX: the accreted Moon was escaping (Robin caught it) — near-breakup spin + inside-Roche mislabel

**What.** Robin watched the browser Moon accrete, compress, then leave on a near-straight outward trajectory —
and switching to Geologic found nothing (`disk_moonlets` empty → hand-off no-ops). Confirmed by tracing the
largest clump's orbit (`gpu_moon_track_json`, a new diagnostic): the clump accreted to ~0.23 M☾ on a tight bound
orbit (a≈11,800 km), then over ~10 s its semi-major axis blew out 11,800 → 27,800 km and it receded and unbound.
It formed at **~1 remnant-radius, INSIDE the Roche limit**, moving ~6.3 km/s (circular ~4.9, escape ~7.0) — i.e.
launched near-radially at near-escape speed, exactly Robin's "straight line, no slowing."

**Two causes, both fixed.** (1) The proto-Earth spin was **7e-4 rad/s — near rotational breakup**, flinging the
near-surface disk out at ~escape speed. Eased to **4e-4** (the cross-check's stable value, ~4.4 h day). (2)
`moonlet_bodies` / the tracker counted ANY bound clump as the Moon — including inside-Roche ones, which are tidal
DEBRIS (they form skimming the surface and escape), not moons. Now only **bound + outside-Roche** clumps
(`Clump::accretes()`) are the Moon; inside-Roche material renders as ejecta.

**Verified (rig `moon_track3`).** The real (outside-Roche) Moon now accretes to ~0.5 M☾ while its orbit
CIRCULARIZES (a: 79,000 → 22,000 km) and then **holds a stable bound orbit** — dist ≈ 29,500 km, v ≈ 1.6 km/s,
a ≈ 22,600 km, bound, steady over t=200–236 s. It orbits and stays. (The first-generation inside-Roche disk still
partly escapes — physical for this energetic sub-scale impact at browser fidelity — but it's ejecta, not the
Moon.) Full suite green; redeployed.

**Note to self:** don't explain away a direct observation with aggregate stats — track the actual thing observed.

---

## 2026-07-18 — HOTFIX: adaptive GPU-load control — the sim was freezing the tab/OS (docs/42)

**What.** The deployed GPU impact encoded a FIXED 100 KDK substeps (and a 300-step relax chunk) per frame — ~100
direct-sum O(N²) dispatches in one command buffer — so the GPU was monopolized and `present()` blocked for a long
time each frame, freezing the browser tab and starving the OS GPU scheduler. Replaced the fixed counts with an
ADAPTIVE per-frame substep budget: `sph_substeps` grows by one when there's frame-time headroom and shrinks
multiplicatively (down to 1) when a frame runs long, keyed off the wall-clock `real_dt`. The relax chunk rides the
same budget. Self-scales to the device — weak GPU → fewer substeps, strong GPU → more, frame time stays bounded.

**Why.** The 100-substep count was left over from the parity/diagnostic work; it must never ship. A sim can't be
allowed to break the device or the interface — it has to live inside a frame budget.

**Verified (rig `frame_check`, xvfb).** Frame time bounded at **~33 ms (30 fps), worst ~50 ms** (was effectively a
multi-second stall). The controller ramps 18→30 fps as it tunes. Full suite green (163 passed / 18 skipped).
Redeployed to integrity.bothead.net.

---

## 2026-07-18 — The "pretty render" layer over the GPU impact + browser parity → DEPLOYED (docs/42)

**What.** Built the render-side of the JIT primitive (Robin's vision): the real GPU SPH giant impact underneath, a
faithful "pretty" render over the top, a **slider** cross-fading them. And brought the browser physics up to parity
so the pretty layer has a real disk beneath it. The GPU impact is now the DEFAULT birth scene (the old CPU-Aggregate
impact retired); Earth/Luna frame buttons use 👁 (not 📷). **Deployed live to integrity.bothead.net.**

**Why.** Decouple physics-fidelity from visual-fidelity: the in-browser SPH is N-limited/fixed-dt, so instead of
forcing raw particles to look photoreal, the pretty layer carries the look while the particles need only be
physically right. The converged numbers stay the offline `tools/impact-run` (docs/41).

**Verified (rig: `pretty_slider` / `parity_check`; energy conserved ~0.05 %; full suite 163 passed / 18 skipped).**
- **Pretty render, 4 phases** (`OrbitDemo::render`, `sph_render.wgsl`): (1) `render_blend` slider + a pretty Earth
  shell sized to the sub-scale SPH body (scale reconcile DISPLAY_SCALE↔SPH_VIS_SCALE), size-cross-fading the
  particles; (2) a crater from the GPU field (first Theia contact freezes the impact dir; magma-ocean interior glows
  through an opaque crust; persists = bake-back); (3) ejecta motes (matter beyond the remnant glows) + a boosted
  shocked-vapor atmosphere; (4) self-bound disk clumps (`gpu_sph::moonlet_bodies`) → warm rock spheres.
- **Browser parity:** the impact was DISPERSING (Theia hit-and-run, 0 % Earth). Fixes: `HydroBody::new_lod` (coarse
  iron core + FINE basalt mantle — the mantle sheds a disk) + a **scheduled shock-dt** (WebGPU forbids the adaptive
  read-back, so the dt is stepped by sim time — small through the ~1.5 h shock, then 5× for the aftermath). → the
  disk now reaches **~27 % Earth with a bound ~0.03 M☾ moonlet** (was 0 %). Weaker than the offline converged run
  (spin → ~58 %) and ~2 fps at N≈2800 (direct-sum O(N²)) — both the N wall, not correctness.
- **Deployed:** `bash scripts/test.sh` green → `./scripts/deploy.sh` (release wasm + vite → /var/www/integrity via
  nginx :8080 / Cloudflare tunnel). Verified live: `birth.html` HTTP 200 locally and at https://integrity.bothead.net.

---

## 2026-07-18 — The SPIN IOU: a spinning proto-Earth sustains the disk → ~58% (docs/41); browser shock-dt fix

**What.** Closed the last docs/40/41 IOU — the disk re-accretes because a *non-spinning* impact leaves it
marginally bound. Added a pre-impact SPIN dial (proto-Earth rotation about the orbit normal) + a grazing-b dial +
intra-run epoch checkpoints to `tools/impact-run` (`spin`/`spineq` modes), and a rotating-frame centrifugal term
to `cs_relax` (a new `omega` Params field, 0 for every existing caller). Also carried the spin into the browser
`gpu_sph` path and found/fixed why the in-browser impact was dispersing.

**Why.** #3 converged the *non-spinning* branch (~25–32% Earth, re-accreting). Angular momentum is the missing
knob: a spinning target flings its own mantle into a rotationally-supported disk.

**Verified.**
- **Spin sustains the disk** (N=2400, K=5, to 18 h): baseline ω=0 DECAYS 0.56→0.09 M☾; ω=7e-4 PLATEAUS at ~0.6 M☾
  with Earth-fraction climbing to and holding **~58% ± 2%** (Moon 8/8) — the canonical value the no-spin case
  never reached. Grazing b=1.4·R_e is a hit-and-run (Theia escapes). L_z conserved to full precision through the
  impact; energy 0.2 %.
- **Not a startup artifact** (cross-check): ω=7e-4 is near breakup, so the check ran at a stable ω=4e-4 — a
  rotating-frame OBLATE equilibrium (flattening 0.149 ∝ ω², bounded) gives the same sustained disk as the
  startup spin (equilibrium 0.43 M☾/39% vs startup 0.32/43% at 18 h, both Moon 4/4).
- **Browser GPU impact fix** (rig, `birth_gpu`/energy trace): it was DISPERSING (Theia hit-and-run, 0 % Earth) —
  a pre-existing regression, NOT the spin (reproduced at spin=0). Cause: the fixed-dt browser path (WebGPU forbids
  the adaptive read-back) under-resolved the shock, so Theia interpenetrated Earth. A 5× smaller dt (paired with
  more substeps to hold playback) restores the shock and Earth begins to shed again (0 % → ~30 %). The
  spin/assembly ports the offline IC; full parity (LOD seeding, a *scheduled* shock-dt) is the render-layer
  follow-up. Energy conserved ~0.05 % throughout.

Offline `sph_step.wgsl` physics unchanged except the relax-only `omega` centrifugal (0 for all non-spin callers).

---

## 2026-07-18 — #3: the disk Earth-fraction converged by ensemble → ~32% (a minority, not 58%) (docs/40→41)

**What.** Built the variable-resolution ENSEMBLE in `tools/impact-run` (docs/40 #3) and converged the giant-impact
disk Earth-fraction. New: `build_lod` (coarse iron core @8×m_fine + fine basalt mantle, all SPH-EOS on the
unchanged `sph_step.wgsl` — no new kernel); an ORDER-INDEPENDENT disk measurement (`sum_oi` = sort+Kahan, re-measures
bit-identical); an ensemble mode (K perturbed-IC runs via a splitmix64 index-hash jitter, mean±stdev); and a
**physical-time epoch** stop (`ensemble <n> <t_hours> <K>`) replacing the fixed step count.

**Why.** The fraction is chaos-scatter-dominated (docs/28 28–50%, #1 25↔63%), so no single run is a number — an
ensemble mean is required. Two things had to be right first: (1) **AV-free relaxation** — the tool's `Gpu::relax`
ran with Monaghan AV on, which DISPERSED the impact (0% Earth, remnant puffed to R≈9500 km); zeroing AV during the
damped settle (restoring it for the shock) is the docs/35 finding the standalone crate never had, and it turned 0%
into a real Earth-bearing disk. (2) **Fixed epoch, not fixed steps** — the disk RE-ACCRETES (mass & fraction decay
with time), and a fixed step count integrates less physical time at higher N (finer Courant dt), confounding the
N-comparison.

**Verified (RTX 2070, native Vulkan; energy conserved 0.3–0.6% throughout).**
- Order-independent reduction: the same snapshot re-measures bit-identical (asserted in the single-run path).
- Re-accretion (fixed N=2400): 25%±5% / 0.19 M☾ @11 h → 12%±14% / 0.04 M☾ @23 h — the fraction is epoch-dependent.
- Convergence at a FIXED ~8 h epoch (K=8): N=1200 **20.4%±7.2%** (under-resolved) → N=2400 **31.8%±2.7%** → N=4800
  **32.2%±3.0%** — 2400 & 4800 statistically identical ⇒ **PLATEAU at ~32%±3%**. A disk MINORITY, decisively not the
  all-particle 58% (which was the high tail of the low-N/early-epoch scatter). A bound Moon-mass clump accretes in
  **8/8** runs at every N at the early epoch (largest ~0.07–0.26 M☾, sub-lunar at this sub-scale).
- Closes #1's number (its 63% was a scatter sample) and #2's resolved Moon. Only #4 (terrain) remains. Nothing
  deployed; `sph_step.wgsl` unchanged; engine crate untouched (change is confined to the standalone tool).

---

## 2026-07-18 — Frame-cost breakdown + hardware analysis → DECISION: defer GPU Barnes–Hut (option B, docs/37)

**What.** Followed the docs/37 GPU-BH finding with the measurement it was missing — a per-pass frame breakdown
(`tools/impact-run bench`, `cargo run --release -- bench`) across N=2k…256k, so the A-vs-B call is quantitative.
Timed each GPU pass of a force eval (`cs_density` is pure O(N) grid; `cs_forces` fuses O(N²) gravity + O(N)
pressure) and calibrated a real-fps model against the two observed browser points.

**Verified / measured (RTX 2070).** force_eval 2.2 ms @2k → 4.6 @8k → 16 @32k → 196 @128k → 700 @256k.
Physics-only fps (16 evals/frame): 28 @2k, 13 @8k, 3.9 @32k, 0.3 @128k. real-fps = ~0.3× physics (render + the
per-frame HUD read-back) — lands on the observed 2.8k→11 fps and 8.2k→4 fps. **Corrections to the earlier
inference:** gravity is ~35 % of the frame at 8k rising to ~50 % by 32k (not the ~25 % I'd guessed), so it IS
about half the physics cost — but the SPH grid+pressure is the co-equal other half, and the grid ALSO goes
super-linear past 64k (fixed `TABLE_SIZE=65536` saturates). So even free gravity ~doubles fps at most, and BH
still doesn't win below 128k. Interactive ceiling on the 2070 ≈ 12–15k; quadrupling the N=2.8k button → ~11k
lands ~3–4 fps.

**Hardware caveat (Robin's point — recorded for the revisit).** The 2070 is the *worst* case: (1) **unified
memory** (M4/A18/Snapdragon) makes a CPU-`bhtree.rs` + GPU-SPH realtime hybrid viable with zero new GPU code
(the CPU↔GPU copy is free; on our discrete PCIe-3 card it isn't → offline-only); (2) the BH crossover likely
drops to ~30–60k on cache-rich / lower-FLOPS GPUs (unmeasured). Cheaper levers for more particles NOW (no GPU
sort): fewer KDK substeps, grow `TABLE_SIZE` with N, lighter HUD read-back.

**DECISION (Robin, 2026-07-18): option B — defer.** Keep direct O(N²) gravity everywhere; do NOT wire BH or
build the GPU radix sort. Direct-sum is correct for every N we target; the sort is the most expensive remaining
kernel with no near-term payoff. The verified BH crate is banked + re-verifiable. **docs/37 now carries the full
write-up: frame table, hardware analysis, revisit triggers (high-N campaign OR Apple/mobile target), and a
resume plan (build the GPU sort as a *reusable* primitive — it also unblocks GPU accretion + grid reorder).**
`impact-run bench` mode committed. On branch `gpu-barnes-hut-verify` off `orbit-diagnostic`; nothing wired or
deployed.

---

## 2026-07-17 — GPU Barnes–Hut built + verified; direct-sum wins below N≈128k → do NOT wire it in-browser (docs/37)

**What.** Built the full GPU Barnes–Hut (LBVH) self-gravity solver spec'd in docs/36 — a standalone native-
Vulkan crate `tools/gpu-bh-verify` + `shaders/bh_gravity.wgsl` with the whole pipeline as WGSL compute kernels
(adaptive bbox via float-radix atomicMin/Max → 30-bit Morton → [interim CPU sort] → Karras binary-radix tree →
atomic-free bottom-up COM → θ-traversal), each **verified against an independent CPU reference before the next
was trusted**.

**Why the design choices.** Opening criterion is the robust Salmon–Warren/Barnes MAC — AABB **diagonal** as the
node size + centre↔COM offset δ — because a plain `maxside/dist<θ` on a *tight* box (the tight box is mandatory
for resolution, docs/36) under-opens and left a 28 % worst-case particle; diagonal+δ keeps the tight box AND
caps the error. Traversal runs in Morton order over a permuted `sbodies[]` so adjacent threads walk coherent
paths with coalesced reads. Leaf bucketing parameterized (`bucket_k`).

**Verified.** `cargo run --release` (RTX 2070) prints PASS for every stage: bbox **exact** (lossless u32
encode), Morton **bit-exact** (coincident→equal), Karras tree structural (every leaf reached exactly once,
parent/child consistent), COM root mass 1.0e-8 / COM 8.2e-8 (**the atomic children-ready climb is coherent on
this hardware**), θ-traversal RMS **0.70 %** at θ=0.5 and **1.8e-6 as θ→0** (recovers the exact direct sum —
the strong structural proof). The GPU direct-sum baseline itself matches CPU f64 to 2.4e-6.

**The finding (disconfirms the docs/36 premise — no-fudge).** Per-eval GPU wall time, θ=0.5: BH overtakes GPU
direct-sum only at **N≈128 000** (2.15×); below it direct-sum wins (N=8k: 0.89×, N=32k: 0.86×). Asymptotics are
textbook — direct → O(N²) (p≈1.84), BH → O(N log N) (p≈1.0) — but the *crossover* is 128k. **Leaf bucketing
(K=8/16/32) does not lower it** (buckets raise accuracy to RMS 6e-4 but cost more traversal time; K=1 has the
lowest crossover). Reason: GPU direct N-body is the near-ideal GPU workload (lockstep broadcast reads, coalesced
FMA, compute-bound), while BH trades cheap FLOPs for divergent memory-bound tree traversal; on the 2070 that
only pays past ~128k. The browser runs N≤~20k and offline `impact-run` at N≈35k — **both far below 128k** — so
wiring BH in-browser (docs/36 stage 8) would *reduce* fps. Also: gravity is only ~25 % of the browser frame at
8k, so it isn't the fps lever regardless.

**Recommendation + open decision.** Keep direct O(N²) gravity for N≤~100k. BH's real niche is **very-high-N
offline convergence (N≳128k)** where it gives a growing speedup (≈9× at 512k extrapolated) — the only path
where the isotopic-fraction scatter (docs/28 ceiling) could be beaten down. So: (A) pursue a converged number →
build the GPU radix sort (docs/36 stage 3, the one hard kernel) + run `impact-run` at N≳128k with BH; or (B)
defer — the verified crate is banked and re-verifiable. The GPU sort was deliberately **not** built (gated on
this decision; most expensive kernel; only needed for option A). Full write-up: **`docs/37`**. Nothing wired,
nothing deployed; on branch off `orbit-diagnostic`.

---

## 2026-07-17 — Direct-sum gravity ceiling measured → GPU Barnes–Hut spec'd for a fresh session (docs/36)

**What.** Measured how far the browser GPU impact's DIRECT O(N²) gravity scales before spec'ing the
Barnes–Hut. On the RTX 2070: N=2800 → ~11 fps (the button default), **N=8200 → 4 fps** (a gorgeous remnant +
spiral-disk, energy still conserved ΔE≈0.08 %, but choppy). The O(N²) dynamics (20 substeps × 2 evals × N²)
is the wall; the offline converged disk (N≈35 000) is unreachable in-browser with direct sum. So a **GPU
Barnes–Hut (O(N log N))** is the agreed next lever — restores fps at 8 k, unlocks N ≳ 20 k for a sharp disk.

**Handover.** Wrote **`docs/36-gpu-barnes-hut-spec.md`** — a self-contained build spec for the next session:
the staged LBVH plan (adaptive bbox reduction → Morton → GPU radix sort → Karras tree → atomic-free bottom-up
COM → θ-traversal), verified GPU-BH-vs-GPU-direct in a new standalone `tools/gpu-bh-verify` (matching the CPU
`bhtree.rs` opening criterion) before wiring into `sph_step.wgsl`/`GpuSph`, then bump N in the browser. Includes
the WGSL gotchas (no float atomics → the atomic-free COM; the mandatory tight bbox) and the hard-won impact
settings the swap must NOT regress (AV-zeroed relax, far-apart relax, the energy-conserving fixed dt). Button
left at the playable N=2800. Nothing deployed.

---

## 2026-07-17 — SOLVED: the in-browser GPU impact forms an orbiting disk (GPU relax + energy-conserving dt) (docs/35)

**Result.** The GPU SPH deformable-Earth impact now runs in the browser at **N≈2800**, conserving energy to
**~0.08 %** and forming a **coherent remnant + an orbiting debris disk** (peaks ~0.6 M☾, up to ~32 % Earth,
Moon-candidate clumps ~0.2 M☾). Rig-verified (`web/rig/sph_energy.mjs`, RTX 2070) — the "lost orbits" are back.

**Two fixes on top of the diagnosis (under-relaxation, energy-conserving fixed dt):**
1. **GPU relaxation** (`GpuSph::encode_relax` / `cs_relax`), so the ~2400 relax steps run on the GPU instead of
   the CPU main thread — the practical blocker, and what lets N rise from ~700 to ~2800. New builders
   `gpu_sph::build_far_apart` (the two bodies placed 40× the contact radius apart, so each self-gravitates in
   the shared buffer with negligible mutual gravity) and `assemble_from_relaxed` (read back → compute the
   collision geometry from the ACTUAL relaxed radii → launch). New `OrbitDemo` phase machine
   (`SphPhase::Relaxing → Assembling → Dynamics`).
2. **No artificial viscosity during relax** (`GpuSph::set_av(0,0)`). Debugging: the first GPU relax DIVERGED —
   the body puffed to ~10³× (remnant "radius" 4×10⁹ m). Cause: the GPU force kernel includes Monaghan AV,
   which the CPU relax does not; AV stiffens the settling transient so the CPU's stable Courant dt rings and
   blows up. Zeroing AV during relax (matching the CPU) makes it stable at the normal dt — and ~4× fewer steps
   than the smaller-dt workaround. AV is restored (1, 2) for the shock-capturing dynamics.

**Honest state.** Energy conservation and the disk are solid; residual escape is still higher than the offline
run and the disk classification wobbles as the hot remnant expands — a coarse-N demo, but a *real* one. The
relax is still ~8–10 s (O(N²) direct gravity × ~2400 steps) — the next speed lever is a **GPU Barnes–Hut**
tree (O(N log N)) to make it snappy and push N higher; an in-kernel per-substep adaptive dt would trim the
escape. The GPU impact stays the "🌋 GPU Impact" button (not auto-deployed to the birth scene). Removed the
now-dead CPU-relax helpers.

---

## 2026-07-17 — Diagnosing the GPU-impact "lost orbits": it's NOT dt injection, it's under-relaxation (docs/35)

**Goal (Robin):** confidently determine whether the in-browser GPU impact is fixable before abandoning it.

**Measure, don't guess.** Added an energy diagnostic — `gpu_sph::total_energy` (KE+IE+PE) + `gpu_energy_json`
— and measured the live impact. **My earlier diagnosis was WRONG:** the total energy is CONSERVED to ~0.01 %
with the current fixed dt (KE falls, IE rises by the same amount — shock heating, correct). So it was never
dt energy-injection. The real cause: **under-relaxation** — I'd cut the browser relax to 640 steps for dev
speed, vs the offline `impact-run`'s ~2200. Unrelaxed bodies carry excess energy and fling debris out (the
3a lesson).

**Result with the relax raised to 2200 (rig-measured, RTX 2070):** energy conserved 0.00–0.02 %; a coherent
bound remnant forms (~9000 km — the SAME size as the offline run) with a debris disk (peaks ~0.35 M☾) and
Moon-candidate clumps (up to 0.34 M☾, 12–44 % Earth). The scene shows a real giant impact (remnant + disk),
not the earlier blown-apart dispersal. **So it is NOT insoluble — do not abandon it.**

**Honest remaining gap vs the offline run:** escape is still ~15× higher (0.8–1.2 vs 0.06 M☾) and the distinct
orbiting disk doesn't cleanly persist — the hot remnant keeps expanding (physical: hot rock → high Tillotson
pressure, no radiative cooling yet) and the disk thins (partly a measurement artefact as the 85 %-mass remnant
radius grows past the disk perigees). At N~700 it's a coarse, marginal disk. **Path to offline quality:** (1)
GPU relax (`cs_relax`) so 2200 steps are milliseconds not ~15 s of CPU — the practical blocker + the key to
(2) higher N; (3) an in-kernel per-substep adaptive dt to trim the excess escape (the fixed dt conserves TOTAL
energy but may mis-distribute at the shock). Kept the c_peak fixed dt (energy-conserving) and the energy
diagnostic; the GPU impact stays the button (WIP), birth scene still Aggregate. Rigs: `web/rig/sph_energy.mjs`.

---

## 2026-07-17 — REVERT: the birth scene goes back to the Aggregate — the GPU impact "loses its orbits" (docs/35)

**What Robin caught.** On the deployed GPU birth scene the debris disperses instead of forming an orbiting
disk/Moon — "we lost orbits." Diagnosed (rig-watch): the **remnant radius grows without bound** — 5994 → 8277
km over 20 s (fixed dt), and worse (→21687 km) with a frame-lagged adaptive dt. Cause: **spurious energy
injection**. The browser GPU impact must use a FIXED dt (WebGPU forbids the blocking read-back the offline
adaptive dt needs); a fixed dt can't hold through the shock (c spikes ~4×) so it pumps energy in and the
material puffs apart. A frame-lagged Courant dt (computed on the CPU from the one-frame-old snapshot) is
WORSE — applied across 20 substeps it overshoots the live shock and explodes. So at browser resolution
(N~700, no per-substep adaptive dt) the impact is not energy-conserving enough to orbit — unlike the offline
`tools/impact-run` (N~35k, per-step adaptive dt, energy conserved 0.3–0.5 %).

**What I did wrong.** I deployed the GPU impact as the default birth scene having verified it *ran*, not that
it produced a good orbiting result — violating my own docs/35 guardrail ("keep the CPU path until the GPU
replacement is verified good"). Corrected: **reverted `birth.html` to the CPU `Aggregate` scene** (which lofts
an orbiting disk → moonlets → a Moon; rig-confirmed restored: 1536 fragments, disk 2.84 M☾ in 2 moonlets) and
**redeployed**. The GPU SPH impact stays the "🌋 GPU Impact" **button** — a WIP physics demo — until its
energy conservation is fixed. The `Space` tab's Sun–Earth–Moon orbits were never affected (rig-verified:
Moon orbiting at ~1.02 km/s). Removed the failed frame-lagged `courant_dt`; the button keeps the shock-safe
fixed dt (puffs slowly but doesn't explode). **Next (to make the GPU impact orbit):** a true per-substep
adaptive dt (a GPU Courant reduction feeding the next substep in-kernel, no CPU round-trip), full GPU relax
(`cs_relax`), and higher N — then re-promote to the birth scene.

---

## 2026-07-17 — Stage 5 migration, increment 2c: geologic hand-off from the GPU disk (docs/35)

**What.** The Geologic button now works in the GPU birth scene (was an Aggregate-only path). New
`gpu_sph::disk_moonlets`: from the read-back disk it finds the self-bound clumps (the `accretion` operator)
and promotes each to a `tides::Moonlet` orbiting the REAL Earth just outside Roche (~3.8 R⊕), carrying the
clump's mass; if no tight clump has formed yet it promotes the whole bound-disk mass as one moonlet (in
geologic time the disk accretes a Moon regardless). `OrbitDemo::enter_geologic_time` branches on `sph_active`:
promote → retire the GPU sim → hand to the validated secular tidal law. Guarded so clicking Geologic before a
disk exists is a no-op (keeps impacting) rather than blanking the scene. With the birth scene fully on GpuSph,
`moon_debris` (`Aggregate`) is now dormant in `OrbitDemo` — functionally retired (the struct deletion waits on
step 5, once the terrain probe also migrates).

**Verified (rig-watch, release build — `web/rig/birth_geologic.mjs`).** Birth impact → disk forms (disk
0.12–0.23 M☾, up to 68% Earth) → `enter_geologic_time()` → `disk_stats_json` returns the GEOLOGIC JSON
(geologic mode active, populated from the GPU disk) and the scene transitions to the geologic Earth view
(grain-shell Earth, camera backed out, HUD "T+1641y after impact"). Native + wasm build clean. Honest notes:
(1) the promoted moonlet then decays under the secular law because this scene gives Earth no spin (a
sub-synchronous moonlet migrates in and shreds at Roche — the existing `tides` physics, not a hand-off bug;
giving the birth Earth a spin, or seeding the moonlet further out, is geologic-scene polish). (2) In the
UNOPTIMIZED dev build the chunked CPU relax pegs the birth scene to ~1 fps for ~30 s (700 particles × 640
relax steps); release is ~10× faster and fine — GPU relax (`cs_relax`) is the proper future fix for dev too.

---

## 2026-07-17 — Stage 5 migration, increment 2b: the Birth-of-the-Moon scene runs on GpuSph (docs/35)

**What.** The "Birth of the Moon" scene now runs the **GPU SPH deformable-Earth impact** instead of the CPU
rigid-Earth `Aggregate` — two differentiated EOS bodies colliding, stepped by `sph_step.wgsl` in-browser.
Fixed the load-freeze blocker (2a) by making the build **non-blocking**: `build_impact_bodies` returns the two
bodies UNRELAXED; `advance` relaxes them in small CPU chunks (20 steps/frame, ~32 frames) via a new
`sph_relax` phase, re-uploading the settling bodies each frame, then `assemble_impact(…, infall=true)`
launches the collision (Theia inbound) and hands off to the GPU KDK dynamics + read-back. Refactored
`gpu_sph.rs` into `build_impact_bodies` / `relax_chunk` / `assemble_impact` (the last is pure — offsets in the
emitted particles, no body mutation, so it can be called every relax frame). `birth.html`/`orbit.ts`
auto-start it; Replay restarts it.

**Verified.** Native + wasm build clean. Rig-watch `birth.html` in the **dev** build (previously the freeze):
loads, the two bodies settle (~1 s, disk "null" during relax), then collide into a mixed remnant + spreading
debris — **no hang, 27 fps**, the birth HUD shows the live GPU disk line. Release build also confirmed. Honest
status: this **changes the deployed birth scene's character** (the Theia-approach narrative + the Aggregate
disk/geologic controls are bypassed — `moon_debris` is now dormant, and the Geologic button no-ops in GPU
mode); it's committed on the branch, not deployed. Remaining for increment 2: retire `moon_debris`
`Aggregate` and rewire the geologic hand-off from the GPU disk (via `accretion.rs`). Then 5c (Sphere), 5d.

---

## 2026-07-17 — Stage 5 migration, increment 2a: GPU impact scene framing (+ a blocker found) (docs/35)

**What.** Toward "the birth scene runs on GpuSph" (docs/35 step 2). The GPU impact rendered as a speck at the
Earth–Moon default zoom; added a dedicated visual scale (`SPH_VIS_SCALE`, Earth's ~5000 km → a few display
units) and a camera zoom-in on trigger, so the impact is legible — a clear central remnant plus a spread
two-provenance debris disk of individual shaded particles. Rig-watch verified on the space scene (HUD: "disk
0.35 M☾, 15% Earth, moon 0.15 M☾").

**Blocker found (honest).** Auto-starting the GPU impact on `birth.html` load **froze the page** — the
one-time CPU relax (`build_deformable_impact`, ~900 particles × ~900 damped steps) runs synchronously on the
wasm main thread and, in the unoptimized dev build, blocks long enough that the scene never paints (rig
screenshot timed out). So `birth.html` stays on the existing `start_birth` (Aggregate) for now; the GPU impact
is the deliberate "🌋 GPU Impact" button. Making the birth scene *default* to GpuSph needs a non-blocking
build first — a GPU relax (`cs_relax` already exists) driven over a few frames, or a lighter/deferred CPU
relax — which is the real next step (docs/35 step 2, revised). Reverted the auto-start; nothing left broken.

---

## 2026-07-17 — Stage 5 migration, increment 1: GPU→CPU read-back + live disk stats (docs/35)

**What.** Robin chose to unify the scenes onto the **GPU SPH path** (retire the CPU `Aggregate` from the live
scenes) — the high-payoff, high-risk direction. Wrote the increment plan in **`docs/35-gpu-path-migration.md`**
(sequence, guardrails, and the one open design decision flagged for later: pure-SPH-EOS vs SPH-EOS+granular on
the GPU). Increment 1 is the universal prerequisite — nothing can migrate until the scene can read GPU
particle state back. Added two-phase async read-back to `GpuSph` (`begin_readback`/`take_readback`, mirroring
`GpuParticles`; WebGPU forbids blocking maps, so it copies one frame and collects the next). `OrbitDemo`
reads back each frame into `sph_snapshot`; `gpu_sph::disk_stats_json` measures the orbiting disk on it
(remnant = 85%-mass body, perigee-above-remnant classification, provenance split) and the largest self-bound
clump via the verified `accretion` operator; `OrbitDemo::gpu_disk_stats_json()` exposes it to JS, shown in the
birth HUD. `mod gpu_sph` is now `#[cfg(target_arch="wasm32")]` (it's only used by the wasm-only `mod app`; the
native SPH reference stays in `tools/`).

**Verified.** Native + wasm builds clean. Rig-watch (`web/rig/sph_impact.mjs`, RTX 2070): triggered the GPU
impact, and the HUD shows the **live read-back disk provenance updating each frame** — e.g. `disk 0.35 M☾
(8% Earth) · moon 0.07 M☾` at t+8.5 s, evolving as the remnant + debris disk form. The read-back → CPU
measurement → JSON → HUD path works end-to-end. (The low/jumpy Earth% is the chaotic N~1050 browser run — a
live visualization, not the converged number; `tools/impact-run` remains the faithful measurement.) Next
increment (docs/35 step 2): put the "Birth of the Moon" scene fully on `GpuSph` and retire `moon_debris`.

---

## 2026-07-17 — Stage 5 (begin): the EOS seam — one pressure abstraction across air and rock (docs/33 §4.5)

**What.** Stage 5 is "retire the forks — unify the particle containers." The blocker the fork map surfaced:
the symmetric SPH pressure-force loop `a = −Σ m (P_i/ρ_i² + P_j/ρ_j²) ∇W` is written THREE times (`AirField`,
`HydroBody`, `aggregate` vapor) differing only in the `P(ρ,u)` call — because there is **no EOS abstraction**
(Tillotson and the inline ideal-gas `ρ·R_s·T` are unrelated). Added one: `eos::Eos` — an enum
`{ Tillotson(Tillotson), IdealGas { rs_t } }` with `pressure`/`sound_speed_sq`/`rho0`, plus `From<Tillotson>`.
Migrated `HydroBody` to carry `Vec<Eos>` instead of `Vec<Tillotson>`, so the one verified SPH container is now
**EOS-agnostic** — it can hold ideal-gas parcels (air) or Tillotson parcels (rock/iron) on the same code
path. This is the seam that lets `AirField` fold into `HydroBody` (next increment) rather than duplicate the
density/force/relax loops.

**Why.** `HydroBody` is the convergence target (it's the CPU reference the stage-4 GPU kernel is verified
against, and it's wired into `gpu_sph.rs`); `AirField`/`Sphere` are legacies to fold toward it. The EOS trait
is the documented precursor (eos.rs's own module doc already claimed "only the `P(ρ,u)` call changes" — this
makes that literally true).

**Verified.** New fast test `eos_enum_dispatches_ideal_gas_and_delegates_tillotson`: ideal gas gives
`P = ρ·rs_t` independent of u and `c² = rs_t`; Tillotson wrapped in the enum is **byte-identical** to calling
the material directly (asserted with `==`). The migration is pure type-wrapping (Eos::Tillotson delegates
exactly), so the Tillotson SPH physics is unchanged — confirmed by re-running the full differentiated-planet
settle: **central P 5.723e11 Pa, core 15591 / mantle 5534 kg/m³** (identical to before). Fast suite 156/156.
Next: fold `AirField`'s SPH into `HydroBody` (needs an optional planar-ghost boundary + external-gravity
option — the one thing AirField does that HydroBody can't yet); then the CPU grain-path decision (5b),
`Sphere` collapse (5c), WGSL-from-Rust (5d).

---

## 2026-07-17 — Stage 4c.4: the GPU SPH deformable-Earth impact runs IN THE BROWSER (docs/33/34)

**What.** Wired the verified GPU SPH stepper into the birth scene so the deformable-Earth giant impact runs
live in-browser (WebGPU), completing stage 4c. New engine module `crates/engine/src/gpu_sph.rs` (`GpuSph`) —
the WebGPU host for `shaders/sph_step.wgsl`: owns the 8-binding pipelines + buffers on `OrbitDemo`'s shared
device, uploads a particle set, and encodes batches of KDK (or relax) substeps. New shader
`shaders/sph_render.wgsl` draws the particles as instanced camera-facing billboards straight from the physics
buffer (zero-copy; pos at byte 0, provenance u32 at byte 44 → Earth = warm rock, Theia = cool steel). New
`OrbitDemo::start_gpu_impact()` (JS button "🌋 GPU Impact") builds + relaxes two differentiated bodies on the
CPU (`gpu_sph::build_deformable_impact`, reusing the verified `HydroBody`), places them on the oblique
giant-impact geometry, and hands the per-frame dynamics to the GPU; `advance()` encodes 8 KDK substeps/frame,
`render()` draws the field. Two WebGPU-shaped choices (documented in the module): **fixed dt** (adaptive
Courant needs a blocking read-back WebGPU forbids) and an **Earth-relative f32 frame** (planetary coords
cancel in f32; the shader re-adds Earth's display position).

**Why.** docs/34 4c.4 — the impact should be visible/interactive in the browser, not only in the offline
native tool. The physics laws stay the shared `sph_step.wgsl` (docs/32 §4: don't fork the particle path — this
is the FIRST in-engine host of that shader, not a fork); only a render pipeline is new to `OrbitDemo`.

**Verified.** `cargo build -p engine --target wasm32-unknown-unknown` clean → the WGSL validates under
WebGPU and the wiring compiles. Rig-watch (`web/rig/sph_impact.mjs`, headed Chromium + xvfb + Vulkan WebGPU on
the RTX 2070): clicked the trigger, watched the whole event — two intact differentiated bodies (t≈0) →
collision + spreading (t+2 s) → a **central remnant plus an extended two-provenance debris disk** (t+8.5 s),
Earth (tan) and Theia (blue) material visibly mixing. No NaN blow-up (the fixed dt held through the shock),
24–25 fps. Screenshots in the job scratch. Native fast suite green. Honest caveats: modest N (~1050) and
fewer relax steps than the offline run (a snappy trigger, slightly hotter start), small on-screen at the
default zoom, no read-back so no live momentum-mirror/HUD numbers yet — all polish, not correctness. This
closes stage 4c (4c.1 integrator, 4c.2 high-N impact, 4c.3 accretion, 4c.4 browser). Remaining realignment:
stage 5 (fold `hydrostatic`/`AirField` into one `Aggregate`) and 6 (energy-tiered JIT particalization).

---

## 2026-07-17 — Stage 4c.3: the accretion / growth operator, conservation-verified (docs/33/34)

**What.** New engine module `crates/engine/src/accretion.rs` — the growth law that lets a round Moon emerge
from the disk. A giant-impact disk of equal-mass SPH particles has no fusion operator (masses never grow), so
it can never coalesce a Moon (diagnosis, JOURNAL entry below). The operator: friends-of-friends clustering
(union-find over particles within a linking length, the same primitive `disk_stats_json` uses) → classify
each clump for two **honesty gates** — (1) genuinely self-bound (`Σ½mᵢ|vᵢ−v_com|² + PE_self < 0`) and (2)
outside the remnant's fluid Roche limit `2.44·R·(ρ_planet/ρ_clump)^⅓` — → PROMOTE each qualifying clump to
ONE body at its COM (mass `Σm`, velocity `Σmv/Σm`, radius from ρ·V). A clump inside Roche is left as particles
(it should tidally shred, not accrete — consistent with `tides::secular_step`).

**Why.** Stage 4c.2 made the disk collisional at high N; this adds the law that turns a bound clump into a
body. Designed as a pure, decoupled function over `(pos, vel, mass, rho)` arrays so it is unit-testable and
reusable — not welded to a scene struct.

**Verified (TDD, `bash scripts/test.sh accretion`, 3/3).** (1) `accretion_conserves_mass_momentum_and_com` —
promote two cold blobs among scattered singletons; expanding bodies+residuals back out conserves total mass,
linear momentum, and centre of mass to **< 1e-12** (exact to f64 round-off), and the 5 singletons are left
alone. (2) `roche_gate_blocks_accretion_inside_the_limit` — the *same* clump accretes outside Roche but NOT
inside it. (3) `unbound_hot_group_does_not_accrete` — a spatially-tight but hot (KE ≫ binding) group is
classified unbound and rejected. Honest about what promotion cannot conserve: internal random KE is absorbed
as heat (physical for inelastic accretion) and internal spin L is folded in — both reported, never dropped.
Full fast suite 155/155.

**Demonstration on a real disk.** Wired a `moon_candidate` scan into `tools/impact-run` (the same FoF +
self-bound + Roche logic, reimplemented standalone like sph-verify) and ran it on the N=35 000 aftermath: the
disk (0.14 M☾, 29 % Earth) contains **21 clumps, 16 of them self-bound**, and the largest bound clump outside
Roche is **0.023 M☾ (31 particles), 10 % Earth** — a proto-moonlet SEED, not a full Moon. Honest: at this N
and only ~9 h of aftermath the disk has begun to clump but is far from accreting a lunar-mass body (real Moon
accretion takes years–decades and/or ≫10⁵ particles). The operator correctly finds the bound clumps; growing
them to a Moon is a longer-time / higher-N run, not a code gap. Next: 4c.4 (browser scene wiring).

---

## 2026-07-17 — CORRECTION to stage 4c.2: the disk composition has large run-to-run SCATTER, not clean convergence

**What I got wrong.** The 4c.2 entry below reported the disk Earth-fraction "converging monotonically
28→33→50 %" toward the CPU's 58 %, from ONE run per N. Re-running the **identical** N=35 000 config (same
binary, same seeds — the build is deterministic) gave **29 %**, not 50 %. Two samples at the same config,
21 points apart. The cause is honest and physical: the GPU grid-insert uses `atomicAdd` for bucket slots, so
neighbour-iteration order is non-deterministic across runs; f32 sums are non-associative; and 11 000 chaotic
integration steps amplify that seed into a macroscopically different disk. **So there is no clean monotonic
convergence — the composition has ~20-point run-to-run scatter, and 28/33/50/29 % are samples of a
distribution around ~30–40 %, consistent with the CPU's 58 % only within that large scatter.** The
no-fudge rule (CLAUDE.md #5) required recording this rather than keeping the favourable sample.

**What still stands (robust across all runs).** The MECHANISM — Earth material reaches orbit in quantity —
and the disk **mass** (~0.13–0.19 M☾), **remnant radius** (~9000 km), **escape speed**, and **energy
conservation** (0.3–0.5 % over ~10 h) are all stable run-to-run. Only the Earth *fraction* of the disk is
scatter-dominated at these N. A converged fraction needs an **ensemble** (average many realisations) and/or
a **deterministic reduction** (order-independent summation), plus higher N — all future work. The
deformable-Earth qualitative result (Earth-derived material orbits, tens of % of the disk) is reproduced;
the precise fraction remains an IOU, now with a measured scatter attached.

---

## 2026-07-17 — Stage 4c.2: high-N giant impact on the GPU (deformable-Earth disk at N up to 35 000) (docs/33/34)

**What.** Built `tools/impact-run` — a standalone offline harness that runs the deformable-Earth giant impact
end-to-end on the RTX 2070 using the verified `sph_step.wgsl` kernels: build two differentiated EOS bodies →
**relax each on the GPU** (new `cs_relax` damped kernel) → collide obliquely at 1.15·v_esc, b≈R_e → KDK-step
the aftermath with **adaptive Courant dt** (new `cs_signal` kernel; CPU reads back the per-particle min each
step) → classify remnant/disk/escaped by the perigee-above-remnant criterion, split by provenance. Added a
`prov` field to the particle (repurposed `_pad`) and a `damp` field to `Params`. This runs the *same*
experiment the CPU test measures at N≈2100 (`a_deformable_earth_impact_measures_the_disk_provenance`), but at
N up to 35 000 in minutes — the resolution the isotopic-crisis number needs.

**Why.** The CPU O(N²) impact caps at ~2100 particles (~8 min/run) and the docs/33 stage-3c result (58% of
the orbiting disk is Earth-derived) was explicitly a coarse-N / sub-scale IOU — mechanism asserted, fraction
not converged. Stage 4 exists to lift the resolution on the GPU.

**Verified (RTX 2070).** Energy conserved to **0.3–0.5%** across ~10 h of simulated aftermath at every N
(the relaxed-body + shock-capturing-AV discipline holds; IE rises ~3× from shock heating). Samples measured:

| run                    |   N    | disk Earth-frac | disk mass | R_remnant | relaxed R_earth |
|------------------------|-------:|----------------:|----------:|----------:|----------------:|
| GPU (direct grav, f32) |  2 100 |           28 %  | 0.19 M☾   | 9208 km   | 4245 km         |
| GPU                    | 14 000 |           33 %  | 0.13 M☾   | 9127 km   | 4482 km         |
| GPU                    | 35 000 |           50 %  | 0.13 M☾   | 8834 km   | 4679 km         |
| GPU (re-run, same cfg) | 35 000 |           29 %  | 0.14 M☾   | 9047 km   | 4679 km         |
| CPU (Barnes–Hut, f64)  |  2 100 |           58 %  | 0.21 M☾   | 9086 km   | —               |

**Read this table with the CORRECTION above:** the two 35 000-particle rows are the SAME config (50 % vs
29 %), so the Earth-fraction column is scatter-dominated (~20 points, GPU-non-determinism × chaos) — do NOT
read 28→33→50 as convergence. What IS robust across every row: the disk **mass** (~0.13–0.19 M☾), **remnant
radius** (~9000 km), **escape speed**, and energy conservation. The deformable-Earth mechanism (Earth-derived
material reaches orbit, tens of % of the disk) reproduces on GPU; the precise fraction is an IOU pending an
ensemble average + a deterministic (order-independent) reduction + higher N. Honest caveats: sub-Earth scale,
direct O(N²) gravity (a GPU Barnes–Hut is the next optimization if N≫10⁵). Run: `cd tools/impact-run &&
cargo run --release -- [earth_n] [steps]`. Next: 4c.3 (accretion operator) and 4c.4 (browser scene wiring).

---

## 2026-07-17 — Stage 4c.1: GPU KDK integration loop, verified over 50 steps (docs/33/34)

**What.** Turned the verified 4a/4b force kernel into a **time integrator**. Added two kernels to
`shaders/sph_step.wgsl` — `cs_kick_drift` (first half-kick of v & u, clamp `u=max(u,0)`, then drift x) and
`cs_kick` (final half-kick) — and a `dt` field to `Params` (repurposed the trailing `_pad`). One dynamical
step = TWO force evals with a half-kick+drift between and a half-kick after, matching the CPU
`HydroBody::step` KDK leapfrog operator-for-operator (energy-conserving, no damping). Per docs/34 the verify
uses a FIXED dt on both sides; GPU adaptive Courant dt (CPU read-back of a min) is deferred until it's needed
by a real run.

**Why.** The force kernel was one evaluation; a giant impact needs the loop. Verify-before-wire discipline
(docs/30): prove the integrator matches the CPU leapfrog before running it at high N or wiring it to a scene.

**Verified (RTX 2070, `tools/sph-verify`).** Extended the harness with an f64 CPU KDK reference (genuine f64
state, no f32 round-trip between steps — a true higher-precision reference) and a GPU multi-step runner (all
passes in one command buffer; consecutive compute passes are ordered & memory-synchronized so step k's drift
is visible to step k+1's density). 50 steps at dt=0.5s from the same IC: GPU f32 vs CPU f64 final state
**pos RMS 3.1e-4, vel 5.7e-4, u 5.1e-4** (displacement-scaled pos) — inside the ~1e-3 honest f32-vs-f64
bound and *tracking*, not diverging. The single-eval force check still PASSes (acc 1.85e-6, du/dt 4.36e-6).
`cargo run --release` exits 0 on both. Next: 4c.2 (high-N impact for the converged disk-provenance number).

---

## 2026-07-17 — Stage 4c prepped for a fresh session + landing hero shipped (docs/34)

**What.** Two things closing out a long session. (1) Built + deployed the **landing-page hero N-body
field** (front-end handoff): a real 2-D velocity-Verlet `F = G·m/r²` sim in `web/src/landing.ts` with honest
live telemetry (bodies / steps / Σ½mv²) and drag-to-toss — the page no longer over-promises. Verified (tsc,
vite, rig-screenshot), live on integrity.bothead.net. (2) Wrote **`docs/34-stage-4c-pickup.md`** — a
self-contained spec so a new session executes stage 4c without re-deriving: the verified 4a/4b foundation
(`sph_step.wgsl` force kernel + grid, `tools/sph-verify`), the four 4c sub-tasks (GPU KDK integration loop +
adaptive dt → high-N impact for the converged number → accretion operator → browser scene wiring), and the
session's hard-won gotchas (engine wgpu is webgpu-only → verify in a standalone Vulkan crate; the grid
cell-membership guard; relax-before-collide; f32 Earth-relative frame; verify-before-wire).

**State.** Realignment stages 1–3 + 4a + 4b DONE and verified; 4c prepped. Working tree clean, all pushed.

---

## 2026-07-17 — Realignment stage 4b: the SPH neighbour grid on GPU, verified (docs/33)

**What.** Added a spatial-hash **neighbour grid** to `shaders/sph_step.wgsl` so the short-range SPH
(density + pressure + AV) scans only the 27 neighbouring cells — O(N) instead of O(N²). Two new kernels
(`cs_grid_clear`, `cs_grid_insert`, atomic bucketing, adapted from `particle_step.wgsl`) build the grid; the
density and force passes look up neighbours via it. Long-range self-gravity stays direct O(N²) (GPU-tiled
direct summation is tractable at these N; a GPU Barnes–Hut tree is a later optimization). Verified on the
RTX 2070 (`tools/sph-verify`): gridded output matches the CPU physics to f32 precision (acceleration RMS
1.9e-6, density 5.6e-7) — the grid is EXACT, like `neighbors.rs`.

**BUG found + fixed (the interesting part).** The first gridded version was 20% off — it found MORE
neighbours than truth (109 vs 88 for the worst particle): **hash collisions among the 27 scanned cells made
some real neighbours read TWICE** (two cells hashing to the same bucket → the bucket processed twice). The
fix is a **cell-membership guard**: when scanning cell C, a bucketed particle j is used only if
`cell_of(j) == C` — so each neighbour is counted exactly once (and collided far particles are skipped),
regardless of table collisions or bucket size. This is the exactness guarantee `neighbors.rs` gets for free
on the CPU. Isolated it by (a) confirming all-N density was exact, then (b) a neighbour-count diagnostic
showing over-counting — not a coverage/precision miss.

**Verified (real GPU).** `sph-verify` PASS at production bucket_k=64: density (grid) max rel error 5.6e-7,
acceleration RMS 1.9e-6, du/dt 4.4e-6. Ahead: 4c — the KDK integration loop + adaptive Courant dt on-GPU +
scene wiring (with the accretion operator).

---

## 2026-07-17 — Realignment stage 4a: the GPU SPH kernel, verified on the RTX 2070 (docs/33)

**What.** Ported the space-band self-gravitating condensed-matter force step to a WGSL compute shader
(`shaders/sph_step.wgsl`) — the same physics as the CPU `hydrostatic.rs::forces_and_dudt`, in f32: SPH
density (cubic spline, per-pair h_ij), Tillotson EOS pressure, Monaghan artificial viscosity, direct O(N²)
self-gravity, and the du/dt energy equation. The goal is to run the giant impact at N~10⁵ (the resolution
the isotopic number — and accretion — need). VERIFIED against an independent f64 CPU computation of the same
equations, headless, on the box's RTX 2070 via native Vulkan wgpu (`tools/sph-verify` — a standalone crate,
since the engine's own wgpu is webgpu-only and can't run native Vulkan).

**Verified (real GPU).** `sph-verify` (N=300, mixed iron/basalt, velocities to exercise the AV): GPU vs CPU
**acceleration RMS relative error 1.9e-6**, max per-particle 2.2e-5, **du/dt RMS 3.6e-6** — i.e. the WGSL
matches the CPU physics to f32 round-trip precision. The kernel is faithful.

**Scope.** This is ONE force evaluation, O(N²), verified. Still ahead: 4b — port the neighbour grid +
Barnes–Hut (the CPU already has both, `neighbors.rs`/`bhtree.rs`) for O(N log N); 4c/5 — the KDK integration
loop on-GPU + the adaptive Courant dt + wiring into the scene (with the new **accretion operator** the
Moon-formation diagnosis showed is also required). But the hard, error-prone part — getting the SPH+EOS+AV
+gravity physics correct in WGSL f32 — is done and proven on the real device.

**Why.** docs/33 stage 4: correctness-first — verify the GPU kernel against the CPU reference on the real
GPU before wiring it into anything (docs/30 discipline: speed must never change the answer).

---

## 2026-07-17 — Can the disk accrete a Moon? Diagnosis + the Roche-disruption fix (docs/28/33)

**What.** Robin, watching the deployed birth scene: "I never see particles join — no accretion into a
Moon; and geologic time makes a giant ball ROLL ON EARTH'S SURFACE, not orbit." Investigated both.

**Diagnosis (can a near-spherical Moon emerge in the current system? NO):**
- **Primary — the collisionless-N ceiling + NO accretion operator.** The scene disk is ~1536 chunks each
  **471 km radius, 0.017 M☾** — collisionless at this N (docs/28's flagged LOD ceiling; real SPH disks use
  10⁴–10⁶). The contact law is fine (restitution 0.40 → ~84% collision-energy loss; self-gravity ~3500×
  cohesion at 471-km grains, correctly the glue). The real gap: **there is no fusion/growth operator** —
  debris `bonds` is empty and never populated, particle masses never grow, the devs deleted the merge
  closure and bet on emergence. So a bound clump renders as a loose cluster of 471-km balls, never a
  growing sphere. **A round Moon needs BOTH higher N (stage 4) AND a coarse-grained accretion law** (a
  bound rubble clump → one body with a grown radius). That accretion operator is a new realignment element.
- **The "ball on the surface" was a real BUG (fixed).** A sub-synchronous geologic moonlet correctly
  migrates inward (Phobos' fate), but `tides::secular_step` CLAMPED its orbit at 1.2 R⊕ and the renderer
  drew a full-mass ball overlapping Earth — no Roche limit enforced.

**Fix.** `tides::secular_step` now enforces the **fluid Roche limit** `d = 2.44·R·(ρ_p/ρ_m)^⅓` (≈ 3.0 R⊕
for Earth + rock): a moonlet that decays inside it is **tidally SHREDDED** — removed, its mass + orbital
angular momentum raining onto the planet (mass returned to the caller and added to Earth in `lib.rs`; L
added to the spin). Removed the 1.2 R⊕ floor clamp. So a sub-synchronous moonlet disrupts instead of
rolling on the surface, and a Moon that forms just outside Roche migrates out honestly.

**Verified (native).** New `a_sub_synchronous_moonlet_disrupts_at_roche_not_on_the_surface`: moonlet at
3.2 R⊕ + 24 h day → disrupts at the 3.02 R⊕ Roche limit, sheds its full 0.30 M☾, total mass + angular
momentum conserved. The existing one-Moon test still forms a Moon just outside Roche that migrates to 29 R⊕
(L drift 5e-15). Full fast suite 152/152; wasm builds. Deployed.

---

## 2026-07-17 — Render-truth: the crater and continents CO-ROTATE with the crust (birth scene)

**What.** Fixed a render-frame mismatch Robin caught while watching the deployed birth scene (he read
Theia's approach as "curving to hit a fixed point"). Investigation verdict: **the approach trajectory is
HONEST** — pure N-body gravity (`orbit::verlet_step`, no steering), the impact site is an OUTPUT discovered
by swept CCD at contact (`impact_site_rel` is `None` through the whole approach), and the inward curve is
genuine gravitational focusing of a hyperbolic impactor in an Earth-centred frame. The "fixed impact point"
he reacted to is the **declared-zero proto-Earth spin** (`lib.rs:2915`, flagged unknown IC): with `spin_l=0`
the surface simply isn't rotating.

BUT the trace surfaced a genuine no-fudge bug: post-impact, once the collision spins Earth up, the crater
(`impact_site_rel`) was rendered as an INERTIAL vector (`earth_center + rel`) while the shell grains rotate
by `spin_rot` — so the hole slid through the rotating crust. And the landmask was sampled at the WORLD
direction (`earth_surface_material(spin_rot·fib_dir)`), painting continents world-fixed while grains rotate
underneath. Both fixed: the crater now co-rotates (`earth_center + spin_rot·rel`) and continents are sampled
at the fixed BODY direction (`earth_surface_material(fib_dir)`) — so grains, continents, and crater share
ONE crust frame that rotates honestly. (Invisible during the birth approach — `spin_l=0` ⇒ `spin_rot` is
identity — so the honest approach is unchanged; the fix bites post-impact when Earth spins up.)

**Verified.** Native + wasm build; full fast suite 151/151. Deployed.

**Flagged for Robin's call (physics IC, not a bug).** The birth scene's proto-Earth spin is deliberately
zero so the post-impact day EMERGES. If we'd rather the surface visibly rotate under the incoming impactor
(more physical — planets rotate), we give proto-Earth a primordial spin IC; the tradeoff is the day becomes
primordial + impact rather than purely emergent. Left as-is pending his decision.

---

## 2026-07-17 — Realignment stage 3c: a DEFORMABLE Earth resolves the isotopic-crisis DIRECTION (docs/33)

**What.** The scientific payoff of the whole realignment: collided a differentiated Theia into a
**deformable, self-gravitating, differentiated proto-Earth** (both real EOS particle bodies, relaxed first)
obliquely at ~mutual escape speed, integrated the aftermath with the shock-capturing SPH integrator (3a),
and MEASURED the bound orbiting disk by provenance (Earth particles vs Theia). Disk = bound material whose
orbital **perigee is above the remnant surface** (genuinely orbiting, separated from the planet body —
`orbit::perigee` about the 85%-mass remnant). No dial; the composition EMERGES.

**MEASURED (native, #[ignore], ~446 s).** `a_deformable_earth_impact_measures_the_disk_provenance`
(M_e=1.75e24 kg ≈ 0.29 M⊕, M_t=2.76e23, v≈7.3 km/s, N≈2100):
- **Orbiting disk 0.207 M☾ — 58% EARTH-derived** (Earth 8.75e21 | Theia 6.43e21 kg).
- Remnant: Earth 1.72e24 | Theia 2.22e23 kg; escaped: 2.1e22 | 4.7e22 kg.

**THE FINDING.** The rigid-boundary Earth capped the disk at **7–12% Earth** (docs/31 — only the excavated
cap could reach orbit). With Earth as REAL MATTER that can shed its own mantle, the disk jumps to **58%
Earth-derived** — Earth material not only reaches orbit, it DOMINATES the disk. This is the direction the
isotopic crisis demands (the real Moon is isotopically Earth-like), and it is exactly docs/28 root-cause #1
(the rigid boundary) being dissolved. Earth is now a participant in its own catastrophe.

**Honest caveats (no-fudge).** Sub-Earth scale (0.29 M⊕), coarse N (~2100 — a resolution/scale IOU,
docs/28), and the post-impact remnant is hot/expanded (R_remnant 9086 km), so the disk is defined beyond
that. **58% is the DIRECTION** (rigid ~10% → deformable ~58%), NOT a converged number — the converged
value waits for the GPU N (stage 4). A first attempt with a too-head-on geometry merged with no disk and
mis-measured (counted the whole extended Earth as "disk" — 89%); that artifact was rejected and the
measurement fixed to the perigee-above-remnant criterion.

**Verified.** Full fast suite 151/151; wasm builds. Stages 1→3c all green.

---

## 2026-07-17 — Realignment stage 3a: dynamical SPH — energy equation + artificial viscosity (docs/33)

**What.** Turned the isothermal planet into a full thermodynamic SPH body for the impact: added to
`hydrostatic.rs` (1) the **SPH internal-energy equation** `du_i/dt = ½ Σ_j m_j (P_i/ρ_i²+P_j/ρ_j²+Π_ij)
(v_i−v_j)·∇W` — the thermodynamically consistent partner of the momentum equation, so compression does PdV
work → heat; (2) **Monaghan artificial viscosity** Π_ij (α=1, β=2) for shock capture (without it SPH
particles interpenetrate at a shock and the impact heating is wrong); (3) an **energy-conserving KDK
leapfrog** `step(dt)` evolving position, velocity, AND internal energy (vs the damped `relax_step`); and (4)
an **adaptive Courant timestep** `courant_dt` from the live compressed sound speed.

**Verified (native, #[ignore], ~67 s).** `a_head_on_collision_conserves_energy_and_shock_heats`: two 400 km
basalt bodies, **relaxed to equilibrium first**, collide head-on at ±1.5 km/s —
- **Total energy (KE+IE+PE) conserved to ~3%** (a one-time injection at the shock front, then flat — the
  known SPH internal-energy-formulation shock error; 5% asserted bound).
- **Shock heating:** internal energy rose **4.9×** (bulk KE → heat), KE fell — the physics that vaporizes
  material and drives the disk.

**KEY LESSON (measured).** Colliding UNRELAXED spheres at 3 km/s TRIPLED the total energy (ΔE/E≈2) — the
startup non-equilibrium dumped into the shock; adaptive dt barely helped (so it wasn't CFL). Relaxing each
body first (Genda: "vibrations until v<100 m/s") + a moderate speed → 3% conservation. Real giant-impact
SPH always relaxes the bodies first; now we do too. Full fast suite 151/151; wasm builds.

**Why.** docs/33 stage 3: the two-body impact needs real shock thermodynamics (heating → vaporization → the
disk), not just contact. This is the integrator the deformable-Earth impact (3b/3c) runs on.

---

## 2026-07-17 — Realignment stage 2b: a differentiated iron-core Earth holds itself up (docs/33)

**What.** Built the layered/differentiated planet — an **Earth-mass iron-core + basalt-mantle** particle body
that holds itself in hydrostatic equilibrium as real matter. Rewrote `hydrostatic.rs` with the **Genda et al.
2012 method** (the fix for the earlier puff-up): **equal-mass particles** at the number density that recovers
each material's ρ₀, with a **per-particle adaptive smoothing length** `h_i ∝ (m/ρ₀)^⅓` (dense core sampled
finely, light mantle coarsely) and a symmetric per-pair `h_ij=½(h_i+h_j)`; per-particle EOS. `HydroBody`
gained `new_differentiated(core, mantle, core_r, total_r, u, N)`. Iron EOS updated to the verified/open
**Wissing & Hobbs 2020** compressed-branch refit (ρ₀=7850, A=128, B=181.5 GPa, a=0.5, b=1.28, E₀=14.25
MJ/kg); its vapor branch stays flagged provisional (stage-3 concern). Also fixed the EOS continuity test's
tolerance (it collapsed at iron's tension zero-crossing near E_iv — the function is continuous; smaller δ +
a bulk-modulus scale floor).

**Verified (native, #[ignore], ~326 s).** `a_differentiated_iron_core_earth_settles_compresses_and_
stratifies` (N=3000, M=5.96e24 kg):
- **COMPRESSES, does not puff up** — settled mass-weighted RMS **3973 km** from 5709 km initial (the old
  equal-volume prototype blew up to 15,700 km; the equal-mass fix is decisive).
- **Stratified:** iron core (mean r 2326 km) stays inside the mantle (4591 km); core settled ρ **15,591**
  kg/m³ (compressed above iron's ρ₀=7850 — real inner core ~13,000), mantle **5534** (real lower mantle
  ~4400–5500). Core denser than mantle ✓.
- **Hydrostatic balance rel 6%** at r=1986 km.
- **Central pressure 572 GPa** vs Earth's real **364 GPa** (Wissing & Hobbs 2020) — same ORDER (~1.6×).
  Honest caveats: coarse N=3000, Tillotson iron over-compresses at high P (a known Tillotson limitation), and
  basalt ≠ the denser perovskite lower mantle — so order-correct, not exact.
Stage 2a (single-material) re-verified green after the refactor — adaptive-h tightened its balance to rel
0.00–0.01. EOS 6/6; full fast suite 151/151; wasm builds.

**Why.** docs/33 stage 2: a planet that is real matter can shed its own mantle into the disk — the
prerequisite for dissolving the rigid boundary (docs/28 #1, docs/31). The differentiated Earth is the object
the impact (stage 3) will hit. Still isothermal (u fixed); the adiabatic energy equation is stage 3.

---

## 2026-07-17 — Research note: sourced EOS data + the differentiated-body method fix (docs/33)

Verification dig for the layered-planet params/method (some primary tables are book-only — Melosh 1989
p.234 — and Robin's linked review is paywalled). What I could source from OPEN literature:

- **Iron Tillotson (compressed branch), Wissing & Hobbs 2020 (A&A 635 A21), refit to Brown et al. 2000
  shock data:** ρ₀=7850, A=128 GPa, B=181.5 GPa, a=0.5, b=1.28, E₀=14.25 MJ/kg. (Vapor-branch E_iv/E_cv/α/β
  NOT given there — still need the primary Melosh table for those; but the compressed branch is all a static
  planet needs.) My current `eos::iron` has A=128 GPa ✓ but b, B, E₀ differ from this refit — update pending.
- **Real Earth-layer structure, Wissing & Hobbs 2020 Table 1** (their PREM fit — a validation dataset for a
  layered particle Earth): inner core ρ₀=7744/B₀=166 GPa, outer core 6920/115, lower mantle 4121/231,
  transition 3622/160, asthenosphere 3380/130, crust 2300/100; M=5.97e24 kg, central P=**364.1 GPa**,
  T_c=5300 K. (A is ≈ the bulk modulus B₀, so these cross-check the Tillotson A values.)
- **Basalt Tillotson: VERIFIED, Benz & Asphaug 1999 Table 2** (exact match to `eos::basalt`).
- **Differentiated-body METHOD, confirmed from Genda et al. 2012 (the puff-up fix):** SPH particles all
  **equal mass**, placed on a **3D FCC lattice** (iron inside, rock outside), internal energy set to
  **1.0×10⁶ J/kg**, relaxed until velocities < 100 m/s. My equal-VOLUME/unequal-mass init was the bug.

Still blocked (needs the primary Melosh 1989 p.234 table or paywall access): full Tillotson sets (esp. the
vapor branch) for **granite, dunite, and iron**. Flagged provisional in `eos.rs`.

---

## 2026-07-17 — Honesty pass: EOS parameter provenance + stage-2b puff-up (docs/33)

**What.** Two honest corrections while extending stage 2 to a layered/differentiated planet (stage 2b):

1. **EOS parameter provenance.** Stage 1's tests verify only SELF-CONSISTENCY (cold P=0, K=A, continuity),
   NOT agreement with the literature — so a wrong-but-self-consistent parameter passes. I had written the
   Tillotson params from memory and labeled them "cited." Verified what I could: **BASALT matches Benz &
   Asphaug 1999 (Table 2) exactly** (ρ₀=2700, A=B=26.7 GPa, E₀=487, E_iv=4.72, E_cv=18.2 MJ/kg, α=β=5) —
   which is why stage 2a settled cleanly. GRANITE, DUNITE, IRON I could NOT verify online (papers cite
   Melosh 1989 p.234 but don't reproduce the table; PDFs weren't text-extractable), so `eos.rs` now flags
   them **PROVISIONAL — unverified against the primary table**. One confirmed fix: dunite ρ₀ 3500 → **3320**
   (Chau et al. 2018). No false "cited" claim stands.

2. **Stage 2b (differentiated iron-core + peridotite-mantle body) PUFFED UP** — RMS radius blew from 2000 km
   to ~15,700 km, mantle density collapsed to 507 kg/m³. The prototype's assertions were too weak and it
   FALSELY passed; I reverted it. Two likely causes, both flagged: (a) the equal-volume / **unequal-mass**
   SPH init corrupts density at the core–mantle interface — proper differentiated bodies need **equal-mass
   particles + adaptive smoothing length** (standard SPH); (b) a bad transcribed parameter (dunite `cap_b`
   is suspect). Deferred until both are resolved: verified params + equal-mass/adaptive-h init.

**Verified.** EOS self-consistency 6/6 still green after the dunite-ρ₀ correction; single-material stage 2a
(basalt, verified params) stands as the solid milestone. Stage 2b reverted, not shipped.

**Why.** No-fudge (docs/23): don't claim "cited" without verifying, and don't ship a test that passes on a
physically wrong (puffed-up) body. Recorded the real state rather than a green checkmark.

---

## 2026-07-17 — Realignment stage 2: a particle planet holds itself up (self-gravitating EOS body, docs/33)

**What.** Added `hydrostatic.rs` — a self-gravitating condensed-matter body that holds itself in hydrostatic
equilibrium as REAL MATTER (a cloud of particles), instead of the rigid analytic boundary the impact scene
uses (docs/28 root cause #1). It is the "merge" docs/32 §3 identified: it COMPOSES the shared kernels rather
than forking them — `eos::Tillotson` pressure (stage 1) + the one SPH kernel `atmosphere::sph_w/dw` +
`bhtree::BarnesHut` self-gravity. `HydroBody::new_sphere` fills a sphere with equal-mass particles at ρ₀,
each with `u=c·T`; `relax_step` settles it (damped) under self-gravity + the symmetric SPH-EOS pressure
force `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W` with `P=Tillotson(ρ,u)`. The only new physics is the condensed EOS; at
unification (docs/33 stage 5) this folds INTO `Aggregate` so a planet and its debris are one particle
system — for now it's a focused, independently-verified module (correctness-first).

**Verified (native, #[ignore], ~215 s).** `a_self_gravitating_eos_body_settles_into_hydrostatic_balance`:
a 1500 km basalt body (N=3000) relaxed under self-gravity + Tillotson pressure —
- **Stable:** settled RMS radius **1383 km**, spread **1.1%** over the last steps (no collapse/explosion).
- **Hydrostatic balance pointwise:** dP/dr vs −ρ(r)·g(r) [g=G·M(<r)/r² from the enclosed particle mass] —
  at r=484 km, −902 vs −1081 (17%); at r=761 km, **−1660 vs −1617 (3%)** — right sign, within SPH operator
  tolerance (cf. atmosphere.rs's 3D balance at ~35%).
- **Central pressure 2.29 GPa** vs the uniform-density self-gravity estimate 3.17 GPa — same order, a real
  planet pressure.
Full fast suite 151/151; wasm builds. Isothermal (u fixed) this stage — the adiabatic energy equation
under compression is the stage 2b/3 refinement. Not yet in a scene.

**Why.** The prerequisite for dissolving the rigid boundary (docs/28 #1, docs/31): a planet that is real
matter can shed its own mantle into the disk. Proves the merge works before touching the tested `Aggregate`.

---

## 2026-07-17 — Realignment stage 1: the Tillotson condensed-matter EOS (docs/33)

**What.** Added `eos.rs` — the **Tillotson equation of state**, `P(ρ, u)` for condensed matter across cold /
shock-compressed / decompressed / vapor states in one closure (the giant-impact standard: Tillotson 1962;
Melosh 1989 App. II; Benz, Cameron & Melosh 1989). This is the missing physics docs/32 §5 flagged: solids
previously resisted compression only via a linear-elastic contact penalty (E·r/m) and planet densities were
declared constants, so shock-compressed rock had no way to develop pressure from its density. `Tillotson`
carries the cited parameters for **granite, basalt, peridotite (dunite/olivine analog), and iron**;
`pressure(ρ,u)`, `sound_speed_sq(ρ,u)` (central-difference, for CFL + bulk-modulus readout), and
`for_material(name)` lookup. Params live in `eos.rs` for now; migrating them into `data/materials.json` (a
`tillotson` block beside `thermal`) is the flagged source-of-truth follow-up (docs/04).

**Why.** The keystone of the realignment (docs/33): ONE pressure law spanning solid→liquid→vapor, replacing
the ideal-gas-vapor + linear-elastic-penalty + declared-density patchwork. The SPH pressure-force machinery
(`aggregate`/`atmosphere`, `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W`) is untouched — only the `P(ρ,u)` it evaluates
changes — which is why a self-gravitating condensed-matter planet (stage 2) is a merge, not new machinery.

**Verified (native, TDD — 6 tests).** `cold_reference_state_has_zero_pressure` (P(ρ₀,0)≈0);
`cold_compression_gives_the_bulk_modulus` (K=ρ·dP/dρ at ρ₀ matches each material's A within 2% — a REAL
bulk modulus, not a contact-spring surrogate); `compression_monotonically_raises_pressure` (stiffens to GPa
scale — the impact regime); `hot_expansion_relaxes_toward_vanishing_pressure` (fully-vaporized expanded
parcel → the ideal-gas limit a·ρu); `pressure_is_continuous_across_the_vaporization_boundaries` (no jump at
E_iv/E_cv); `sound_speed_is_real_and_of_the_expected_order` (c≈√(A/ρ₀), km/s). Full fast suite 151/151; wasm
builds. Not yet wired into any scene (stage 2 builds the self-gravitating planet on it) — nothing to
rig-watch/deploy yet.

---

## 2026-07-17 — Architecture map + first-principles realignment plan (docs/32, docs/33, CLAUDE.md)

**What.** Mapped the whole engine and wrote it up for future Claude sessions (Robin: too many "surprises"
about what already exists). Four parallel readers covered the physics core, terrain/atmosphere, scene/render/
GPU, and docs/build/deploy; synthesized into **docs/32-architecture-map.md** (module-by-module with
`file:line` anchors, the shared-laws-vs-forked-solvers map, the EOS inventory, the birth-of-the-Moon scene
trace, and the workflow rules), a concise auto-loaded **CLAUDE.md** pointing to it, and
**docs/33-architecture-realignment.md** — a staged plan to realign the architecture to Integrity's
principles (Robin's three framings: material physics scalable · calculations tiered on energy scale ·
everything a natural product of the real physics).

**Key finding.** The physics *laws* are already unified and scale-invariant (`granular::Contact`, the SPH
kernel, `Furrow` excavation, `plough_loft`, `Body`, `LayeredBody`); the *solvers and containers* are FORKED
— two container universes (CPU `Aggregate` f64 vs voxel-`World`/GPU f32), four integrators over one law, the
rigid-boundary fork (Earth is a penalty sphere, not particles — docs/28 #1), and **no condensed-matter EOS**
(solids resist via a linear-elastic contact penalty; planet densities are declared constants). A
self-gravitating EOS planet turns out to be a MERGE, not new machinery: `atmosphere.rs`'s verified SPH
pressure kernel + `bhtree.rs` self-gravity + `aggregate.rs::apply_thermo` energy equation, with the ideal-gas
EOS swapped for a Tillotson EOS — only the EOS is genuinely new.

**The realignment (docs/33).** One particle/material engine every scene drives: one container (bulk forms
are the coarse *energy tier* of the same particles, not a separate universe), one pressure law (Tillotson EOS
spanning solid→liquid→vapor, replacing the ideal-gas + linear-elastic + declared-density patchwork), one
energy-tiered stepper (fidelity T0 bulk → T1 quasi-static → T2 granular+thermal → T3 full EOS shock/vapor,
selected by energy density vs the material's own thresholds — generalizing docs/08/13 spatial LOD to
energy-tiered physics via the docs/16 awake-set). Staged correctness-first: (1) Tillotson EOS module +
tests, (2) self-gravitating EOS planet vs planet.rs's analytic hydrostatic profile, (3) two-body impact both
bodies as particles → re-measure the isotopic crisis, (4) GPU-resident unified stepper at N~10⁵, (5) unify
the containers, (6) formalize the energy-tiered awake-set. Full-particle-Earth is milestones 2–3.

**Why.** Robin's directives: all particle physics in ONE scale-invariant module; build the hard correct
physics first (GPU/full-res if needed), optimize physics-faithfully later; everything a natural product of
the real physics. The map stops the rediscovery; the plan makes the full-particle-Earth build the forcing
function of the realignment rather than a side quest.

**Verified.** Docs only — no code change. Existing suite unaffected.

---

## 2026-07-16 — The isotopic crisis: physics says proto-Earth spin is NOT the lever (docs/31)

**What.** Opened the isotopic crisis (docs/31, "Option C"): the canonical impact makes a **Theia-dominated**
disk, but the real Moon is isotopically Earth-like. Tested **Ćuk & Stewart (2012)'s** proposed resolution —
a *fast-spinning* proto-Earth flings its own mantle into the disk. Implemented proto-Earth spin honestly:
the excavated Earth cap is surface mantle that was **co-rotating before the impact**, so each `SOURCE_TARGET`
grain is now born with `ω × (pos − centre)` (added in `build_impact_debris_scaled` before the ploughing
loft, so the momentum exchange acts on the real pre-impact velocity; `earth_omega = 0` is byte-identical to
before). Scene wired: `lib.rs` converts `spin_l → ω = L/I` (solid sphere) and passes it, default **zero**
(unknown IC, flagged) — nothing changes on screen; the plumbing just lets a spin be *explored*.

**MEASURED (physics deciding against the hypothesis).** `a_fast_spinning_protoearth_makes_the_disk_earth_
derived` (#[ignore], N=256+512, 3000×2 s), non-spinning vs a 2.3 h-day proto-Earth (ω·R ≈ 4835 m/s):
- ω=0    : Earth **0.162** | Theia 1.241 M☾ → disk is **12 % Earth**
- ω=fast : Earth **0.181** | Theia 2.412 M☾ → disk is **7 % Earth**

A fast spin lofts *slightly* more Earth material (0.162→0.181) and injects a lot of angular momentum, so the
whole bound disk grows (1.40→2.59 M☾) — but it retains proportionally **more Theia**, so the Earth *fraction*
FALLS, 12 %→7 %. **Spinning the target does not resolve the crisis in our model.**

**Why — and the real lever.** Direct consequence of docs/28 root cause #1: **Earth is a rigid boundary**, so
the only Earth material that can reach the disk is the small excavated cap. The actual Ćuk & Stewart
mechanism is a spinning proto-Earth shedding its **bulk mantle** — which a rigid analytic sphere cannot do.
So 7 % is a LOWER BOUND the rigid boundary imposes, and adding spin only speeds up the material that *is*
free to move (overwhelmingly Theia). The honest resolution needs **Earth-as-deformable-matter** (docs/28 #1)
or **vapor-phase Earth↔Theia mixing** (now partly reachable via the SPH vapor field, docs/26/27) — NOT
target spin. Documented in docs/31 with the next experiments.

**Why.** No-fudge (docs/23): we set a physical initial condition (spin) and let the disk provenance EMERGE;
when it emerged *against* the hypothesis we recorded that, and the test now asserts only the robust mechanics
(spin ⇒ larger bound disk) plus the measured ceiling (fraction does not rise), printing the provenance split.

**Verified (native).** Full fast suite 145/145; the measurement test green with the corrected (measured)
assertions; wasm builds; scene byte-unchanged at the default zero spin.

---

## 2026-07-16 — The accelerated compute module: neighbour grid + Barnes–Hut + block timesteps (docs/30)

**What.** Built the reusable **accelerated particle compute module** (docs/30) so the impact disk can run
at high N without the O(N²) wall — a general substrate (any particle system: weather, clouds, fluids), not
an impact special-case. Four stages, each its own crate/module with a brute-force fallback below a size
threshold and a test that pins it to the exact/near-exact reference:

- **Stage 1a/1b — neighbour grid** (`neighbors.rs`). A spatial-hash `NeighborGrid::build(pos, cell)` +
  `for_each_pair` that finds every short-range pair in O(N) instead of O(N²), then wired into the contact
  and SPH density/pressure loops (one `sr_grid` built per step from shared `sr_pos`/`masses`). Brute-force
  below 512 bodies. Test: `grid_finds_exactly_the_brute_force_pairs` (exact — the grid is not an
  approximation).
- **Stage 1c — Barnes–Hut self-gravity** (`bhtree.rs`). An octree caching per-node COM+mass; a particle
  uses a node as ONE source when its angular size `(2·half)/dist < θ` (θ=0.5), turning O(N²) self-gravity
  into O(N log N). Same Plummer softening as the direct sum — the same physics, grouped. Test:
  `barnes_hut_matches_brute_force_within_theta_bound` (RMS < 1% at θ=0.5; θ→0 recovers brute force to 1e-9).
- **Stage 3 — block timesteps** (`aggregate.rs`). A per-particle timestep criterion (`particle_timesteps`:
  √(ε/|a|) free-fall, capped by the |v|/|a| turnaround), then a hierarchical **block KDK** integrator
  (`step_block`): power-of-two rungs, the quiescent disk coasts while the shocked/vapor core sub-steps.
  The subset-force pass (`accelerations_masked` + `BarnesHut::accelerations_active`) recomputes gravity
  only for the bodies being kicked this sub-step — O(N_active log N). Thermo (PdV cooling, radiation,
  phase flip, dissipation heating) was extracted into `apply_thermo` and now runs each sub-step, so
  `step_block` is a faithful full-physics drop-in for `step()`. Wired into the space scene.

Also this pass: the impact scene now runs at **high N (512 debris + 1024 cap)** with the cap-mass fix
restored (`cap_mass` summed from the real per-grain target masses, not the `moon_mass·CAP_N/DEBRIS_N`
bookkeeping that the 07-15 entry flagged as ≈6.5× high); and two **watching** tools so the agent can see
what Robin sees — `rig/birth_shot.mjs` (headless-Chromium screenshots of birth.html at timed marks) and a
"📷 Share view" button on the space band that POSTs the live canvas.

**Why.** docs/30: temporal + spatial coherence is the "MPEG for physics" — most of the cloud barely moves
per step (the block scheduler's coasting rungs are the delta-frames; the grid/tree are the spatial
compression). Getting the disk to lunar-mass resolution needs O(N log N), and the module has to be generic
because the same substrate runs every future particle system. No-fudge (docs/23): every accelerator is
proven against its exact/θ-bounded reference, so speed never changes the answer.

**Verified (native).** Full suite green; `grid_finds_exactly_the_brute_force_pairs`,
`barnes_hut_matches_brute_force_within_theta_bound`, `contact_grid_matches_brute_force`,
`particle_timesteps_shrink_with_acceleration`, `step_block_conserves_energy_and_matches_global_dt`, and
— the decisive one — `birth_impact_with_step_block_reproduces_the_disk`: the REAL coupled impact gives
**global step() 0.772 M☾ vs block step_block 0.788 M☾** (matches). `step_block_speedup_bench` measures
**5.5× faster** on an aftermath-shape cloud (1330 ms → 241 ms). On-screen: deployed to
integrity.bothead.net (build 20260716.081104) and rig-watched — the disk forms and evolves identically to
the global integrator (T+24m: 2.44 M☾ in 42 accreting moonlets, Earth-origin material aloft), no regression.

---

## 2026-07-15 — Vapor gets a real pressure field: SPH + a latent-heat reservoir (docs/26/27, docs/28 item 5)

**What.** Replaced the vapor "overlap hack" with a real **SPH pressure field** so the impact-generated
vapor expands and cools as a gas from first principles, not a scripted push. `aggregate.rs`: a cubic-spline
kernel gives each vapor particle a density ρ=Σm_jW(r,h); pressure P=ρ·R_s·T; a symmetric,
momentum-conserving pressure force; and a PdV energy equation so expansion does real work and the gas
cools itself. Then a **latent-heat reservoir** (docs/28): the pressure reads the *thermal* temperature
`T − L_v/c`, so the energy locked in the vaporization latent heat is not double-counted as pressure — the
vapor holds heat honestly on the phase boundary instead of over-puffing. Also shipped the
`disk_orbit_vs_resolution` diagnostic sweep (the disk grows toward lunar mass with N: 0.77→1.27→1.41 M☾ at
N=384/768/1536).

**Why.** docs/26/27: the atmosphere/vapor must be *matter under its own pressure*, not a visual. The old
overlap repulsion was a fudge (docs/23); SPH is the honest continuum form, and the latent-heat correction
keeps the first law intact across the solid↔vapor phase change (docs/28 item 5).

**Verified (native).** `vapor_sph_expands_and_cools_conserving_energy` — a hot vapor ball expands under
its own pressure and self-cools (80k → 18.5k K), total energy conserved to within drift; the latent-heat
fix dropped a spurious vapor↔vapor dissipation heating that had inflated both temperature and disk mass
(disk 0.066 → 0.132 M☾, peak T 52k → 18.5k K — honest physics over the bigger-but-wrong number). Full
suite green.

---

## 2026-07-15 — The Moon becomes Earth-derived: a momentum-conserving loft breaks the 0.000 deficit

**What.** Closed docs/28 step 3. Earth (target) material now LOFTS into the bound proto-lunar disk —
**Earth 0.083 M☾ | Theia 0.551 M☾** aloft, where it had measured a dead **0.000 M☾ Earth** at every
resolution (the "nothing is taken from Earth" deficit). The Moon is now genuinely Earth-derived, as the
isotopes demand — and it emerged from conserved mechanics, no dial. Two coupled fixes:

- **Physical cap mass (docs/28 item 4).** The excavated cap was materialized at a bookkeeping **2× the
  impactor** mass; it is now real **ρ·V** — each grain an equal slice of the furrow's half-ellipsoid volume
  times the LOCAL density at its depth (≈ 0.31× the impactor). `furrow_target_grains` sets it; the energy
  cap and per-grain contact use each grain's real mass.
- **A momentum-conserving loft in the SHARED particle physics** (`granular::plough_loft`, not the impact
  builder — Robin: "added to global particle physics"). When a fast body ploughs slower target matter, the
  along-track (tangential) momentum is shared inelastically toward the impactor↔cap **centre-of-mass**
  velocity — the physical maximum drag, no free dial — and what the cap gains the impactor loses, so
  Σ(m·v) is **exactly** conserved. Only the along-track component is touched (radial rebound + gravity keep
  theirs). This is the same reverted "COM drag" from 2026-07-14 that made it WORSE — the ONLY thing that
  changed is the cap mass: at the fudged 2× the COM speed collapsed to v_t/3 (sub-orbital, gutted the
  disk); at the physical 0.31× it is ~0.76·v_t ≈ near-orbital, so Earth material joins the disk while the
  impactor barely slows. The cap-mass fudge, not the mechanism, was the blocker all along.

One law for every band: a terrain meteor and a giant impact both loft their excavated matter through
`plough_loft` (space-band wired now; terrain wiring is a flagged follow-up).

**Why.** docs/23/24 no-fudge: the loft is real ploughing momentum, declared HONESTLY as a conserved
transfer (the µs shock is sub-resolution at any N — docs/24 #1), never a scripted velocity.

**Verified (native).** `plough_loft_conserves_momentum_and_lofts_the_lighter_target` (Σ tangential p
unchanged; cap dragged up, impactor slowed; radial untouched; vertical = no-op). **Full suite 144/144** —
every disk guardrail (birth peak-aloft > 0.3 M☾, emergent day 2–14 h, theia) still holds, so the honest
mass + loft did not detune the disk. On-screen rig-watch (birth.html) is the remaining check — pending
Robin's eyes / a rig in this env. FOLLOW-UPS (flagged, not papered over): the lib.rs interactive-scene
mass bookkeeping (`cap_mass = moon_mass·CAP_N/DEBRIS_N`, now ≈6.5× high) and terrain-band `plough_loft`
wiring.

---

## 2026-07-14 — Measured: "raise N" does NOT loft Earth material (the disk deficit is a mechanism, not a resolution, problem)

**What.** Investigated docs/28 step 3 (progressive excavation) — why the proto-lunar disk is ~100%
impactor ("nothing is taken from Earth"). Made the impact resolution a real knob
(`impact::build_impact_debris_scaled(.., debris_n, cap_n)`; the const `build_impact_debris_between`
delegates at the default 128/256) and added two `#[ignore]` measurement sweeps
(`disk_provenance_vs_resolution_sweep`, `disk_provenance_emergence_no_declared_ejection`). Then MEASURED
the bound-aloft disk composition across N — the honest test of the "raise N globally" hypothesis.

**Why.** Before spending the O(n²)→tree perf work that a global N increase would require, prove that more
resolution actually lofts Earth-derived material. It does not.

**Verified (measured, native).** Bound-aloft mass by provenance (M☾), 3000×2 s aftermath:
- Declared ejection ON: N=384/768/1536 → **Earth 0.000 / 0.000 / 0.000**; Theia 0.69 / 0.35 / 0.72
  (the Theia disk mass does not even converge — it is relaxation-noise-limited, the docs/28 collisionless
  ceiling, not resolution-starved).
- Declared ejection OFF (cap AT REST, contact ploughing must do the lofting): N=384/1536 →
  **Earth 0.000 / 0.000**; Theia 0.69 / 0.84.

**Earth material lofts in NONE of the six configurations.** The cause is provable and N-INDEPENDENT: a
grain launched from the surface needs a near-tangential speed ≥ the ~7.9 km/s circular velocity to hold a
perigee above the surface. The declared `Furrow::ejection` gives ~5.9 km/s at ~45° (horizontal ≈ 4.1
km/s) — sub-orbital, so every cap grain re-impacts, at any N. With the ejection OFF, contact ploughing
drives the resting cap DOWN and downrange into the planet, not up — the shock-driven excavation flow that
would loft it is sub-resolution at any feasible N (docs/24 problem #1), so it never emerges. **Conclusion:
the Earth-lofting deficit is a MISSING MECHANISM, not a resolution shortfall; "raise N globally" is not
the lever.** A separate dead end confirmed and reverted en route: a momentum-conserving "ploughing drag"
(impactor drags cap downrange toward the COM tangential velocity) makes it WORSE (both → 0.000) — full
inelastic sharing drops the impactor to v_t/3 and guts its own disk, and the cap only reaches ~2.2 km/s,
still sub-orbital.

**The real levers (for the next session / Robin's steer), all no-fudge (docs/23, docs/24):**
1. **Materials-honest contact.** Theia's *construction* is layered (iron core + peridotite mantle, as
   theorized), but its collision *physics* is bulk **basalt** for every grain (restitution 0.40, basalt
   density for grain radius, equal grain mass). That basalt restitution IS the disk's damping law. The
   aggregate contact already carries per-grain `mat_ids` and is momentum-conserving for ANY mass ratio
   (equal-and-opposite forces ÷ each own mass) — so per-grain real material + real ρ·V mass is viable at
   full resolution; it just needs the contact loop to read `mat_ids`. This also fixes docs/28 item 4 (the
   cap is ~6.5× over-massed: 2× impactor vs the physical ρ·V furrow ≈ 0.31× impactor).
2. **The docs/24 emergence subsystem** — deposit the impactor's momentum/energy as real compression so
   REBOUND lofts material (delete the declared `Furrow::ejection`). Since the µs shock is sub-resolution
   at any N, the honest form is a momentum-CONSERVING loft that gives near-track excavated Earth material
   *near-orbital tangential* velocity from the impactor's momentum (not the radial 45° script) — the
   corrected version of the reverted drag, unblocked once the cap mass is physical (item 1).

**Shipped this pass:** the N knob + the two reproducible measurement sweeps (all 136 native tests green;
the sweeps are `--ignored`, O(n²)). No physics claim shipped — the finding is the deliverable. On-device
rig-watch not required (nothing visual changed). NOTE: the Jul 12 render-truth fixes and the Jul 13
terrain-contact/furrow commits are still un-journaled — a catch-up entry is outstanding.

---

## 2026-07-11 — The engine watches itself: the rig, the profiler, and a 7× frame

**What.** The agent now verifies scenes with its own eyes before shipping them (Robin: "simulate
locally and watch — we've been through a lot of iterations you could have seen going wrong"). The watch
rig (`web/rig`): headed Chromium under xvfb (headless cannot composite WebGPU swapchains — the first
attempts photographed a blind rig, not a broken app), timed screenshots, a frame profiler, an fps probe.
First session of use, in order: proved the scenes render correctly; caught a post-impact DEATH SPIRAL
(one slow frame → 0.25 s backlog → 128 O(n²) substeps → slower still, pinned at 1 fps); profiled
advance() at 161 ms vs render() at 3 ms; and found the real culprit — `powf(-1.5)` libm calls per
gravity pair. Hardware sqrt: **161 → 22 ms/frame (7×)**; the native suite dropped 133 → 52 s too.
Substep budgeting ends the spiral (observable time dilates; the frame stays interactive). Camera opens
on the sun side (the night side is honestly black now) and rides the BOUND debris extent (escapees no
longer drag the view out to pixels). Watched verdict at T+13h aftermath: 32 fps, 354 → 62 fragments as
settled matter demotes into Earth, disk 0.48 M☾ in 3 moonlets — the on-screen numbers now match the
native emergence tests.

**Verified.** By watching. 91/91 native; profiler numbers above.

---

## 2026-07-11 — The Birth of the Moon: the SCENE (docs/27)

**What.** The proven giant-impact physics, now watchable: a new scene (**Birth of the Moon** in the scene
picker) opens ~5 real seconds before the strike at the close framing (25% of lunar distance), with a HUD
countdown that IS the simulation's own forecast (distance / closing speed from the live N-body state —
the same conservation-law machinery, read as a clock). Theia arrives with a real IMPACT PARAMETER
(0.87 of the contact radius at 6 km/s from quarter-lunar range), so the ~45° obliquity of the hypothesis
EMERGES from geometry + gravity at contact — never aimed. At the strike, both bodies materialize (Theia's
iron core + hot mantle; Earth's crust/mantle/outer-core cap), and the camera rides OUT with the ejecta —
view distance tracks the debris extent — to the wide whole-orbit framing, watching the lofted, bound,
perigee-raised material (0.55 M_moon in the native test) circularize into the proto-lunar disk. Replay
re-runs the encounter.

Also, for ALL impact scenes (Robin): a **T+ aftermath clock in SIM time** (y/mo/d/h/m/s at the scale the
number deserves) — the honest answer to "what timeframe are we watching?", since time-LOD means wall
time ≠ world time; and the pre-impact countdown for the birth scene. The impactor is now a first-class
parameter of the space band (radius/mass/profile drive CCD, excavation, rendering, materialization), so
the moon-drop is just one configuration of the same scene machinery.

**Verified.** 87/87 native (the physics is the previous entry's test); wasm + TypeScript build clean.
The choreography needs on-device eyes.

---

## 2026-07-11 — THE ANTITHESIS: the birth of the Moon (docs/27)

**What.** Robin: *"a mass impacted the earth and ejected the material that became the moon — I'd like to
see that happen. If it works, we can prove our system works."* The proof, as a passing native test: the
SAME impact machinery that shatters a falling Moon, run in reverse role — a Mars-sized differentiated
impactor (**Theia**: iron core + peridotite mantle, ~6.5e23 kg, declared like every other body) strikes
Earth **obliquely** at the mutual escape speed (~9.5 km/s; obliquity is what puts mantle on lofted
trajectories with angular momentum instead of straight up). Kepler alone would return every launched
fragment to its launch radius — it is debris-debris CONTACT and SELF-GRAVITY, already in the model, that
must raise perigees into orbit. Integrating the aftermath: **0.55 lunar masses of material ends up aloft,
bound, and perigee-raised above the surface — genuinely orbiting** (the theorized proto-lunar disk is
1–2 M_moon; 0.55 at 192-particle resolution is the right scale), while only 0.14 M_moon escapes. The
Moon-forming reservoir emerges from the declared bodies and the one contact law. Nothing was scripted;
the machinery was not told what a "disk" is.

Also: `build_impact_debris` generalized to ANY impactor/target pair of layered bodies (the moon-drop
scene is now just one parameterization), Theia added to the planet profiles, giant-impactor excavation
clamped to a hemispheric scale (flagged approximation). The interactive birth-of-the-Moon SCENE (5 s HUD
countdown, camera riding the ejecta out to watch the Moon form) is the next build on this physics.

**Verified.** `an_oblique_theia_impact_lofts_bound_material_the_protolunar_disk`; 87/87 native.

---

## 2026-07-11 — The exponential atmosphere EMERGES (docs/26 tests 1+2)

**What.** Air is now dynamic matter (`atmosphere.rs`): gas parcels whose resistance to compression is
their EQUATION OF STATE (ideal gas — the 1D column force is exactly F = A·ρ·R_s·T per slab), never an
elastic modulus. THE emergence result: a column of 200 equal-mass air slabs under gravity, started from
a deliberately WRONG exponential (2× the real scale height), relaxes to the real isothermal atmosphere —
**measured H = 8,446 m vs the analytic R_s·T/g = 8,427 m (0.2%)** — proving the profile is an attractor
of the physics, not an initial condition. And the settled column's basal pressure equals its weight
(100,266 vs 101,357 Pa — one real atmosphere from one real declared column mass): the docs/25 static
boundary condition is provably this dynamic model's limit. Also: `gas_contact_from_material` (K = γ·P
stiffness for the canonical contact law), R_s = 287 J/(kg·K) from the declared molar mass, and
free-expansion-in-vacuum (gas never clumps). Flagged next: the 3D SPH kernel density (the column is the
honest first resolvable case), then drag + entry glow (docs/26 tests 4–5).

**Verified.** `a_settling_air_column_finds_the_real_exponential_atmosphere` + 2 more; 83/83 native.

---

## 2026-07-11 — Every solid object is matter: the Moon gets the same treatment

**What.** Two representation asymmetries closed (Robin: "Every solid object in the universe is composed
of matter"): (1) the intact **Moon now renders as a grain shell** — its basalt crust at its measured
reflectance — exactly like Earth; no more smooth-sphere summary on one body and honest grains on the
other. (2) **Moon-vs-moon collisions use the same primitives as moon-vs-Earth**: swept CCD on the
pre-step relative path, the true contact state from the conservation laws (vis-viva + angular momentum),
an inelastic momentum-conserving merge at the contact configuration, and the dissipated energy
accounted. Nothing special-cases Earth anymore. Flagged next: materializing a moon-moon impact cloud is
the same `build_impact_debris` with the target's layered profile parameterized (today it samples Earth's
profile for the target and the Moon's for the impactor).

**Verified.** 80/80 native; wasm builds.

---

## 2026-07-11 — Physics/render decoupling: the simulation runs the world; the render just looks at it

**What.** The space band's physics no longer lives inside `render()`. The new architecture (docs/13 made
real):
- **`advance(real_dt)`** drives the PHYSICS from wall-clock time in fixed sim-timestep substeps whose
  COUNT (never size) varies with the elapsed real time. The physics rate is now independent of the
  display frame rate — a 30 fps client simulates the same world as a 120 fps one (previously the sim
  assumed 60 fps and ran half-speed at 30). Under overload the observable clock dilates (backlog is
  dropped) rather than corrupting the physics with an oversized step: time slows before truth breaks.
- **The renderer samples snapshots ~100 ms BEHIND the physics** (Robin: humans can't catch detail under
  1/10 s, so use that budget). Every event the render draws is already fully resolved — a collision can
  never be caught mid-lie by a frame boundary, structurally: the fly-past class of bugs is now
  impossible rather than patched. Snapshot interpolation gives smooth motion at any frame rate; the
  crater/shatter appear exactly when the RENDERED clock crosses the shatter instant.
- Physics is never triggered by, or dependent on, the visualization — it drives it (Robin's
  architectural invariant, verbatim).

Also fixed from Robin's render read: **"hollow earth"** — through the crater you could see the far side
of the crust from inside. The planet isn't hollow: the un-materialized bulk (physically the boundary +
gravity source) now renders as an opaque interior sphere at the depth the crater exposes — the top of
the outer core, self-lit at its REAL temperature from the layer profile. Through the hole you now see
glowing molten interior, honestly.

**Verified.** 79/79 native; wasm builds. Frame-rate independence and the lag are structural (wall-clock
in, snapshots out); on-device read pending.

---

## 2026-07-11 — The atmosphere's weight keeps the oceans liquid (docs/25)

**What.** Earth now declares only the MEASURED MASS of its atmosphere (5.15e18 kg); the surface pressure
emerges as that column's weight — ≈1 atm, never assigned. Materials gained Clausius–Clapeyron BOILING
curves (latent heat + molar mass, `thermal.molar_mass`) beside their Simon melting curves, and the phase
decision (`planet::surface_phase`) now covers solid/liquid/vapor. The consequences, all as passing tests:
288 K water under the emergent 1 atm is LIQUID; the same water in vacuum flashes to VAPOR at any
temperature (below the ~611 Pa triple point liquid has no regime — Robin: "water exposed to vacuum would
be wild", and the model now says exactly that); cold water freezes; water boils at ~366 K at 0.7 atm
(mountain physics for free). The airless Moon ⇒ no lunar seas, as observed. A failing test caught real
physics along the way: Earth's inner core briefly classified as "Vapor" because iron's boiling point was
a flat 1-atm fallback — pressure suppresses boiling even harder than melting, and with iron's real molar
mass boiling is COMPLETELY suppressed at 360 GPa (the fallback was the dishonesty).

**Why.** Same pattern as the molten core: declare real composition (now including the air), compute the
consequences. Also fixes the record on ocean colour: water renders with its measured near-black
reflectance — the "blue marble" is atmospheric Rayleigh scattering, which we refuse to paint. The
atmosphere today is a static boundary condition (pressure/phase); making it MATTER — drag, entry plasma,
Rayleigh blue, blast waves, evaporation cycling — is now the flagged next major milestone (docs/25
roadmap).

**Verified.** `the_declared_atmosphere_mass_weighs_in_at_one_atmosphere`,
`liquid_oceans_exist_under_an_atmosphere_and_boil_off_in_vacuum`; 79/79 native, wasm builds.

---

## 2026-07-11 — Layered planets: the molten core EMERGES from pressure (docs/25)

**What.** Planets are now DECLARED as their real construction and nothing else: concentric layers of real
materials (Earth: iron inner/outer core, peridotite mantle, basalt crust — PREM densities; Moon: small
iron core, peridotite mantle, basalt crust) with the observed geotherm as declared data. Everything else
is COMPUTED:
- **Gravity g(r)** — Gauss's law over the enclosed layer mass (peaks at the core boundary, zero at centre).
- **Pressure P(r)** — hydrostatic equilibrium integrated from the surface. Earth's centre comes out
  **≈360 GPa** (real: 364) and the core–mantle boundary ≈135 GPa from the declared densities alone.
- **PHASE** — each material got a pressure-dependent melting curve (Simon–Glatzel, published fits, new
  `thermal.simon_a/simon_c` in the materials DB). Phase = local temperature vs T_m(P). **Never assigned.**

**The emergence result (Robin's challenge: "that should be a natural artefact of gravity/mass/material
if we didn't fudge the composition"):** Earth's inner core comes out **SOLID** even though it is HOTTER
than the molten outer core — because the computed pressure pushes iron's melting curve above the
geotherm exactly there — while the outer core comes out **MOLTEN** and the mantle solid. The melt curve
crosses the temperature profile at the real inner-core boundary. Also: the declared layer densities
integrate to Earth's real mass and 9.8 m/s² surface gravity; the Moon's outer core comes out molten at
lunar pressures (flagged: the real lunar core is Fe–S, which melts lower than our pure-iron entry — we
use the upper published selenotherm; an Fe–S material is the refinement).

**Wired into the impact:** the materialized clouds now sample the layered bodies — each particle knows
its material (basalt crust / peridotite mantle / iron core) and its REAL internal temperature, so
excavating deep matter exposes rock and iron that glows because it genuinely is that hot. Earth's cap
reaches the top of the molten outer core (a Moon-scale impact digs that deep). Each fragment renders in
its own material's reflectance — the excavated composition is visible.

**Continents & oceans:** the render shell samples a 10°×10° land/ocean mask matched to the ~9° grain
spacing ("average area particles") — granite continents, water oceans, real reflectances. Honesty flags:
the hand-digitized mask over-represents land (~37% vs the real 29% — a cited dataset is the refinement);
ocean depth (~3.7 km) is far below one grain, so at this LOD water is the material of a grain's surface,
not a resolved layer; no planetary rotation yet, so the mask's orientation is arbitrary but consistent.

**Verified.** New `planet.rs` tests: declared composition → real mass + surface gravity; hydrostatics →
real central/CMB pressures; **molten-outer/solid-inner core emergence**; lunar molten outer core + solid
mantle; landmask places the major continents/oceans with a plausible area-weighted land fraction.
77/77 native, wasm builds.

---

## 2026-07-11 — Gauss interior gravity, emergent incandescence, and Earth rendered as matter

**What.** Three fixes, each traced from an on-device observation to missing physics (never a visual patch):
1. **Interior gravity obeys Gauss's law.** Debris that ploughed beneath the surface was sucked into the
   core ("the balls absorb into the centre") because the point-mass 1/r² is only valid OUTSIDE a planet.
   Inside, only the enclosed mass pulls: g(r) = GM·r/R³, linear to ZERO at the centre. The gravity source
   is now an extended body (1/r² outside its radius, Gauss interior inside) — no singular attractor.
2. **Incandescence is emergent — the hand-deposit pipeline is GONE from the planetary impact.** The
   impactor's fragments now simply CARRY the true contact velocity (they are the arriving body); the one
   contact law transfers momentum into the target's materialized matter, and the contact DISSIPATION
   (damping + friction) is routed into temperature (`granular::contact_dissipation` — energy is conserved,
   not destroyed, docs/20). A hard impact glows because the matter genuinely got hot. Measured: the cloud
   goes 83% → **100% gravitationally bound** through the collision, hottest fragment ~41,600 K (flagged:
   melt/vaporization energy sinks are not yet modelled at this scale, so the peak overshoots — the glow
   is real physics, the exact peak is not yet).
3. **Earth renders as its matter.** A smooth sphere is a representation lie once matter can be excavated —
   it hides the damage. Earth now draws as a shell of ~512 coarse grains (the honest low-res look);
   grains inside the materialized impact region are hidden, so the excavated void IS the crater and the
   glowing cap particles are the matter that used to fill it. Reset now un-shatters properly. Cosmetic
   skinning (an elastic surface over the blocks) is deferred until after the physics visuals are right.

Also: descent-follow camera (pure camera work — reads `moon_distance_km` from the N-body state, starts on
the whole-orbit framing, glides to a close-up at 25% of lunar distance as the Moon falls; manual zoom
overrides, Drop/Reset re-engage).

**Why.** Robin: "Do not bandaid visuals — fix the physics and then visualize them." Each visual wrongness
was a physics gap: no interior Gauss law, dissipation not becoming heat, a summary representation
(the sphere) hiding real state.

**Verified.** `interior_gravity_follows_gauss_law_not_a_point_singularity` (half depth ⇒ half g; centre
pulls ~nothing; exterior unchanged); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(100% bound, hottest ≫ visible-glow threshold — incandescence emergent); 72/72 native, wasm builds.
On-device: Robin confirms the impact now reads correctly ("Much better!").

---

## 2026-07-11 — ONE collision law for all matter + the mutual impact + conservation-law contact state

**What.** Three connected pieces, closing out Robin's "this must define ALL collisions of ALL matter":
1. **The canonical contact law now governs aggregates too.** `granular::contact_accel` (spring + damping +
   Coulomb friction + cohesion — the physics of record for terrain grains and GPU debris) is now the
   contact force inside `Aggregate` as well; the new `granular::contact_from_material` is the ONE mapping
   from a real material (Young's modulus → stiffness, restitution → damping, friction, cohesion) to
   contact behaviour, used everywhere. Aggregate particles previously had gravity and bonds but NO
   contact — they interpenetrated freely, which is why the shattered Moon was an "exploding sphere in a
   vacuum". A surface velocity rule I'd added to compensate ("cancel the inward component") was a fudge
   (Robin caught it) — deleted; the bulk planet is now a conservative penalty boundary (a force, −∇U).
2. **The mutual impact (`impact.rs`).** At the strike we materialize BOTH bodies at the interface — the
   Moon as a rubble ball on the surface AND Earth's impact region as a cap of crust (same grain mass) —
   and deposit the Moon's real momentum + energy into the *combined* cloud via the same
   momentum/shock-heat/vapor pipeline as the terrestrial meteor. Earth's matter absorbs most of the
   momentum; crater, ejecta, fallback all emerge from the one contact law. Measured natively: **93% of
   the cloud stays gravitationally bound** — as the declared energetics demand (≈2e7 J/kg deposited vs
   ≈6.3e7 J/kg to unbind).
3. **Conservation-law contact state.** Robin observed "a large percentage of debris escapes" on-device,
   yet the model said 93% bound — the discrepancy was the INPUT: in fast-forward, the deposit used the
   Moon's velocity *after* a ~2000 s step that had carried it far past the surface — a garbage sample
   (**21,822 m/s vs the true 9,870 m/s → ~4.9× the honest energy**). New `orbit::contact_velocity`
   recovers the true state at the surface from the two-body conservation laws (vis-viva energy + angular
   momentum), dt-independent. The simulation FORECASTS the collision; it never samples garbage.
   Also: the frame now STOPS at a detected collision (the render can never show a body sailing past its
   own impact — the simulation drives the visualization and interrupts it).

**Why.** "Get the small stuff right, apply everywhere": one contact law, derived from declared material
parameters, at every scale — and dt-independence as a principle: what the physics concludes must not
depend on how coarsely we stepped or how fast the visualization runs.

**Verified.** `aggregate_particles_collide_via_the_canonical_law_and_conserve_momentum` (no pass-through,
momentum conserved, real rebound); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(93% bound after the contact plays out); `contact_velocity_recovers_the_true_impact_speed_regardless_of_
step_size` (recovery within 2% at the browser's coarsest step vs +121% for the post-step sample);
71/71 native, wasm builds. Visual verdict pending on-device.

---

## 2026-07-11 — Materialize the Moon at impact: honest momentum, real 1/r² fall-back, incandescence for free

**What.** With tunneling fixed, the Moon reached Earth but "dinked on top and fell out the bottom, intact,
no ejecta" (Robin, on-device). Three honesty bugs in the shatter, all now fixed:
1. **Momentum was being dropped/mis-deposited.** First cut drove the debris with the Moon's full *incoming*
   momentum → the whole clump shot DOWN through Earth. I then tried zeroing the momentum — Robin caught it:
   *"drop the momentum sounds like fudge again."* Right. The honest model (Robin's framing): from orbit the
   Earth/Moon are *"really big single particles, an average of physical material properties"*; at impact we
   **materialize** the Moon into its constituent matter, and the fragments carry the Moon's REAL incoming
   velocity (Σmᵢv = m_moon·v — momentum conserved across the promotion, not dropped). The impact ENERGY
   disperses them symmetrically (net-zero momentum). **Earth's surface then transfers the inward momentum by
   CONTACT** — the same swept CCD primitive, now applied per-fragment (inward fragments stop on the ground →
   momentum to Earth; outward ones eject). "Get the small stuff right, apply everywhere."
2. **Fall-back/escape was faked by a uniform gravity field.** A baked uniform "down" wrongly forces even
   >escape-velocity fragments to fall back. Robin: *"Model parameters declare REAL physics… we MUST be
   faithful."* Replaced with a real **point-source 1/r² pull toward Earth's actual centre** (G·M_earth,
   from the masses the model already declares), softening kept tiny (2% R⊕) so the field is faithful where
   fragments live (r ≥ R⊕, contact-enforced). The escape/fall-back split is now EMERGENT, not imposed.
3. **Incandescence now comes free from the thermal state.** `deposit_impact` already computes each
   fragment's temperature; the space shader gained a self-emissive term and the debris is tinted by a
   blackbody ramp of its REAL temperature (dark→red→orange→white). Hot ejecta glows on the night side —
   nothing scripted.

Also added **📷 Earth / 📷 Moon** camera buttons (explicit frame-of-reference switch; "Camera on Moon"
frames the impact site once it shatters) toward Robin's zooming-FoR / "fixed camera 1000 m above the site"
goal.

**Why.** The vision, sharpened by Robin across this session: *all* Newtonian-scale laws should EMERGE from
faithfully-modelled matter — the engine answers "what if the Moon deorbited?" by tracking real materials,
so a child (and, ultimately, an embodied AGI) could *re-derive physics* from playing in it. Every fudge is
a false lesson. So the escape boundary must come from √(2GM/r), not a tuned knob.

**Verified.** New native test `aggregate::point_source_gravity_splits_escape_from_fallback`: a fragment
launched at 1.4×escape leaves for good (>10 R⊕); at 0.6×escape it arcs back and lands (apoapsis <2 R⊕) —
the threshold read straight from the declared M and G, with surface contact mirrored as in the render.
`cargo test -p engine` 66/66; wasm builds + deployed. The shatter VISUAL (scatter + glow) needs on-device
eyes. FLAGGED next: apply the swept CCD to the GPU granular contact (retire the `V_MAX` cap) — same primitive.

---

## 2026-07-11 — Swept collision (forecast the path): the dropped Moon no longer TUNNELS through Earth

**What.** The dropped Moon was shooting straight through the Earth and never colliding. Root cause (Robin
diagnosed it exactly): in fast-forward the Moon moves > an Earth-diameter per step, so the DISCRETE
contact test (are the surfaces overlapping *this* sample?) sees it outside at both samples and misses the
collision entirely — the trajectory was effectively faked, riding on a detection that never fires.
Fixed with **swept continuous collision detection**: `orbit::swept_first_contact(rel_old, rel_new, r_sum)`
solves for the fraction `t∈[0,1]` at which the body's straight path FIRST enters the contact sphere —
*when* it hits — regardless of step size. `OrbitDemo::render` now captures each moon's pre-step position,
runs the swept test after the step, and intervenes at the first-contact point (parking the point mass
there and, for moon 0, triggering the Stage-A shatter at the true impact site/energy).

**Why.** Robin: *"forecast with the simulation (know what will happen in real physics), model it with the
visuals"* and *"there is a difference between what we can render and what we can simulate."* The
simulation must KNOW the continuous path intersects the planet even when we sample/render it coarsely —
what we simulate must not depend on how coarsely we look. And: *"get the small stuff right, APPLY
EVERYWHERE"* — `swept_first_contact` is a pure-geometry primitive (segment vs. sphere), not orbit-specific.
The SAME tunneling is why the grain sim caps the vapor front at `V_MAX` (a workaround); CCD is the honest
general fix there too — flagged as the next application of this primitive to the granular contact.

**Verified.** `orbit::swept_contact_catches_a_body_that_tunnels_through_the_planet` (a −5→+5 pass through
the centre — both endpoints outside — is caught at t=0.4; a clearing path is `None`; already-inside is
t=0). `cargo test -p engine` 65/65; wasm builds. The Moon-collides VISUAL still needs on-device eyes.

**Open.** Apply CCD to the GPU granular contact (replace the `V_MAX` ejecta-speed cap) — the same
primitive, the "everywhere". And the Stage-A shatter visual + Stage B zoom-in remain to verify/build.

---

## 2026-07-11 — Moon-shot Stage A: the dropped Moon SHATTERS (emergent), instead of merging

**What.** In the space band (`OrbitDemo`), the de-orbited Moon now **shatters into a debris cloud** on
impact rather than the point-mass sphere silently merging into Earth. The frame the Moon first strikes,
its point mass becomes a **self-gravitating aggregate** of 64 basalt fragments filling the Moon's volume
at the impact site (`build_moon_debris`), and the impact energy — captured honestly at contact (~4.5e30 J)
— is deposited via the same `aggregate::deposit_impact` pipeline (momentum + shock heat + vapor). Because
that energy is ≫ the Moon's binding energy, the aggregate DISPERSES — no scripted destroy, just kick vs.
binding (docs/21). The fragments then arc under Earth's gravity (uniform toward its centre), some flying
out, some falling back — the ejecta curtain at planetary scale. They render as small basalt spheres at
their real positions; the intact Moon sphere stops drawing. The debris steps at a FIXED observable rate
(`DEBRIS_DT`, a time-LOD) so the fine event plays out at human speed, not the celestial fast-forward that
would disperse it in one frame.

**Why.** The moon-shot (docs/23): "de-orbit the Moon into [a spot], then zoom … and observe it was
destroyed" — with NO code that says destroy. The drop, the fall, the surface contact, and the honest
impact-energy accounting all already existed and were native-tested; the collision just rendered as two
spheres merging + a HUD number tagged "not yet materialised." This wires the tested aggregate-disruption
physics into the render so the shatter is finally *seen*, emergently. It's the celestial half; Stage B is
the zoom-in that materialises the local crater/ejecta from the same conserved energy (docs/19).

**Verified.** `cargo test -p engine` 64/64; wasm builds. The disruption physics itself is native-tested
(`aggregate::energy_above_binding_disrupts_it`, `an_impact_heats_the_core_and_shatters_the_aggregate`);
`build_moon_debris` feeds it the real impact energy. **The VISUAL is NOT yet verified** — a rendering
change can't be checked headlessly (docs/19: "needs on-device eyes"). Needs Robin's eyes + tuning of
`DEBRIS_DT`, `DEBRIS_N`, fragment size, and the fall-back/escape balance.

**Open.** Earth-side damage is still only the HUD verdict (no crater visual on the Earth sphere yet); the
debris external gravity is a uniform approximation (fine near the impact, coarse as it spreads); and the
whole thing is the CELESTIAL band only — the scale-relative zoom-in (Stage B/C) is still unbuilt.

---

## 2026-07-11 — Cohesive grain contact (the frictionless-graze fix, one property doing three jobs)

**What.** Added an ATTRACTIVE adhesion term to the grain contact law (GPU shader + native `granular.rs`
force-of-record): net normal force = repulsive spring − cohesion, so touching grains can now BOND (the
force pulls them together) until the bond lets go past a short range. `cohesion = 0` recovers the exact
old push-only contact. The friction load now includes the cohesion, so a touching/grazing pair has a
real normal load — and therefore friction. `c_cohesion` is derived from `Material::cohesion`, converted
to a per-mass adhesion and capped at a granular ceiling (loose debris is already fractured — rock grains
keep only surface adhesion, they must not re-weld into solid). Reused the dead `c_max_accel` param slot,
so no struct-layout churn.

**Why.** Robin caught that a grain placed at *exactly* zero overlap grazes frictionlessly — friction is
`μ·N` and `N = k·overlap = 0` there. Her instinct: "surely there's a property of matter that ensures
this never happens unless the particles are separated?" There is — **cohesion**, a real material
property already in `materials.json` that we used for solid bonds but not loose-grain contact. It closes
the graze (touching ⇒ bonded ⇒ normal load ⇒ friction), it's *why* soil holds a slope dry sand can't
(the same thread as the granite-cliff/talus split), and it's part of what holds a planet together
against its own gravity — a prerequisite for the Moon-onto-vacuum-Earth moon-shot.

**Verified.** New `gpu-verify` foundational test **F8**: a gentle separating nudge is HELD by the bond
(1.00 m), a hard nudge BREAKS it (6.84 m), a cohesionless pair DRIFTS apart (2.00 m). Native
`granular::cohesion_bonds_touching_grains_and_raises_friction`: a just-touching cohesive pair is pulled
together and a zero-compression graze has friction, while dry stays frictionless. `cargo test -p engine`
64/64; wasm builds; no scene regression (scenes default to cohesion = 0).

**Open.** Cohesion is a single representative value (like friction) — a per-particle/mixed-material
cohesion is a later refinement; and terrain contact doesn't yet carry cohesion (grains adhering to the
ground), flagged.

---

## 2026-07-11 — Emergent impact end-to-end: momentum-conserving contact, terrain-as-matter, drag fudge deleted

**What.** A long arc turning the impact from scripted fudges into emergent particle physics (`docs/24`),
capped by a foundational test suite that caught a core-model bug and a fix that scaled the whole thing.

- **Terrain-as-matter (Path B).** The meteor no longer carves a crater and scripts ejecta velocity. It
  MATERIALIZES the impact region into grains at rest (`matter::materialize_region`), deposits the
  meteor's real momentum as an impulse (`deposit_impulse`), and the rest of ½mv² as radial-gradient
  **shock heat** (`deposit_shock_heat`, filled core-first so a sub-grain impactor's energy actually
  vaporizes a plasma core instead of smearing below threshold). Vaporized matter **expands** and throws
  the ejecta (`deposit_vapor_expansion`) — Robin's insight that at 17 km/s the crater is driven by phase
  transition (gas pressure), not elastic rebound; the KE was already in the sim as shock heat we were
  radiating away. Added estimated thermal data for the granular soils (they couldn't vaporize before).
- **Momentum-conserving contact solve (the core fix).** A two-particle test (`gpu-verify` F5) exposed
  that the directional-implicit solver damped each grain's ABSOLUTE velocity, so a 20 m/s head-on
  collision lost ~74% of its momentum — hidden by every pile/crater scene (slow ⇒ ~0 COM velocity).
  Fixed by a derived neighbor-coupling term `Sv_nbr = Σ S·v_neighbor` in the RHS; the pair's COM velocity
  now telescopes to conserved. This alone made stepped terrain conserve energy and DEEPENED the emergent
  crater ~5× (3 m → 14.7 m) — ejecta finally keep their momentum.
- **Conservative terrain + steep materialization.** Replaced the min-translation terrain penalty (whose
  normal FLIPPED at voxel edges, injecting energy) with a smooth bilinear surface and the exact −∇U
  penalty. Vertical walls a heightfield still can't represent become grains
  (`materialize_steep_terrain`) — but only where the material is too weak to hold a cliff (critical
  height ≈ σ/ρg): dirt slumps to talus, **granite holds as a real cliff** (Robin's antithesis, emergent
  from strength).
- **DRAG FUDGE DELETED** (`matter::DRAG` 0.9995 → 1.0). It bled 62%/s of a vacuum particle's speed
  (foundational test) and was masking the non-conservative terrain; with the three fixes above the core
  no longer needs it — a vacuum particle keeps its momentum.
- **Everything couples honestly.** `aggregate::deposit_impact` (probe/bodies) rewritten to the SAME
  pipeline (momentum + heat + vapor) — the last scripted `√(2·0.3·e/ρ)` kick is gone; the meteor couples
  into EVERY body via `couple_impact_to_bodies`, not a hardcoded probe. Removed the cosmetic
  `spawn_vaporized_meteor` (a scripted 22 m/s clump that looked like an intact meteor and double-counted
  momentum). Restitution is now derived from `Material::restitution` via a θ-method contact integrator.

**Why.** Robin's directives drove it: *"trust physics; hunt for what we're missing, don't fudge"*;
*"crater size should be emergent and observable, not imposed"*; *"a meteor is an exaggerated test of the
same physics as a footfall or a feather"*; *"no fudge in the core models"*; and the clincher — *"test
every aspect of the fundamental interactions of particles; get the small stuff right and the universe
scales."* The foundational suite proved it: a two-particle collision plus a derivation beat the solver
that "looked stable and passed every scene."

**Verified (RTX 2070 headless + native).** New `gpu-verify` foundational block F1–F7 (Newton's 1st/2nd/
3rd, momentum-conserving collision, friction≈μg, touching↔separated sweep) all PASS in true vacuum;
grain-grain energy conservation (I-flat), stepped-terrain conservation (I), emergent crater (M, 14.7 m
deep). `cargo test -p engine` 63/63; wasm builds. New native tests: materialize/impulse/shock-heat/vapor
conservation, steep-terrain materialization + the granite-cliff antithesis, restitution→damping.

**Open (honest, flagged).** (1) A granite cliff a heightfield still can't contact conservatively should
become a COHESIVE aggregate (grains + bonds) — rigid AND conservative. (2) Friction runs ~35% strong
(F6 ratio 1.35 vs μg) — the same over-sticky friction behind the repose under-/over-prediction (scene
D). (3) Crater size is LOD-capped (materialize cap) below the physical scale. (4) Dissipated energy →
heat → radiation still dropped (flagged in-shader). (5) Soil thermal values are composition estimates,
not cited.

---

## 2026-07-09 — North star + a reverted fudge; the engine's name: "Integrity"

**What.** While bringing GPU debris up (docs/22), a play-test exposed that a meteor doesn't destroy the
probe, and I reached for a special case — `if probe within crater { obliterate }`. Robin: "if
everything is real, the probe should have just been destroyed on impact on its own; the fact we have to
correct that concerns me." She's right — that's a **fudge**. **Reverted it.** The real problem: the
probe is the **last bespoke object** (a rigid `body::Sphere`), not matter, so `matter::impact` can't
see it. Wrote `docs/23`: the north-star demo — **a metal ball at ground zero, de-orbit the Moon into
it, zoom in and observe the ball was destroyed** — with NO code that says "destroy the ball." It's
destroyed because the impact energy really reaches it and exceeds iron's thresholds (`damage`). The fix
is to make the probe **real matter** (a cohesive aggregate / voxel body), so gravity, contact, impact,
`damage::classify`, and emission all act on it emergently — no special cases.

**Name.** Robin is naming the engine **"Integrity"** — fitting: it's the operating invariant (every
value traces to real physics or is flagged; no fudge), and reverting the special case is exactly it.

**Also (shipped this session, verified/native):** terrain now uses planetary **surface gravity**
(uniform down, not the slab's micro-g self-gravity — fixes debris concentrating at the world centre;
real-time, no time-scale). The GPU debris path works on-device (FPS fixed, debris glow + cool).

**Open (honest):** probe-as-matter unification (`docs/23`); GPU resting-debris re-deposition
(iteration 3, kills the moiré pile-up); the celestial→local materialization for the zoom-in.

---

## 2026-07-09 — Bodies as particle aggregates (emergent binding + disruption)

**What.** Started making celestial destruction a *simulation, not a mock* (`docs/21`). A body becomes a
**cloud of particles held together by its own gravity** (`aggregate.rs`): softened N-body self-gravity,
`binding_energy` (Σ G·m·m/r), `kinetic_energy_com`, `rms_radius`, `com`. Verified that a cold cloud
**holds together** (cohesion emerges from gravity — the `docs/15` roundness invariant) and that an
energy kick **above the binding energy disrupts it** (emergent dispersal — the identity behind a
shattered moon).

**Why.** Robin asked, pointedly, whether the impact destruction is *inherent in the engine's model* or
"just mocks to humor me." Honest audit: the terrain meteor IS real emergent simulation (per-voxel
fracture/melt/vaporize from material + energy, glowing by computed temperature). But the **celestial**
Moon-crash was NOT simulated destruction — the bodies were point masses drawn as spheres, so there was
no matter to break, and I was about to build a scripted "fireball" — a **mock**. Stopped: that violates
the honesty invariant. The honest path (Robin chose it) is bodies-as-aggregates, so the shatter is the
same gravity that rounds them, run past their binding energy — no script.

**Verified.** `aggregate::a_self_gravitating_cloud_holds_together`,
`aggregate::energy_above_binding_disrupts_it`. `cargo test` 44/44; clippy `-D warnings` clean; fmt
clean.

**Honest scope.** This is the gravitational *skeleton*. Per-particle material + temperature, the impact
coupling (deposit energy → `damage::classify` per particle → emergent debris/melt/vapor), and the
rendering are the next slices (`docs/21`). Until they land, the *visible* Moon-crash still shows the
momentum stick — and we will NOT fake the shatter in the meantime.

---

## 2026-07-09 — Phase classes integrated into matter::impact; Moon-speed readout

**What.** `matter::impact` now classifies each ejecta via `damage::classify` (the thermodynamic
thresholds): a carved voxel is at least Fractured, the hot core Melts, the hottest Vaporizes. The class
drives behaviour — **vaporized** ejecta expand away fast (gas/plasma, `VAPOR_EXPANSION`), all glow by
temperature. The crater extent stays the budget model (LOD bridge `docs/19` intact). Also added a live
**Moon-speed readout** (km/s relative to Earth) to the space-band HUD.

**Why (speed readout).** Robin saw the Moon's velocity seem to "flatten as if terminal velocity in a
vacuum." Checked the orbit path: there is **no drag, clamp, or damping** anywhere — the only velocity
changes are the verlet kicks and surface contact. So there is no terminal velocity; the physics is
honest. The apparent flattening is either Kepler's 2nd law (a *partial* brake makes an eccentric orbit
that slows at apogee — the opposite of drag) or the compressed time-scale hiding the final fast plunge.
The speed readout makes it observable: on a true **Drop** it climbs toward ~11 km/s at impact; use ⏪
slower to watch it accelerate.

**Verified.** New `matter::a_colossal_impact_vaporizes_the_core` (core passes basalt's boiling point →
Vaporized class). `cargo test` 41/41; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
deployed.

**Honest caveat (`docs/20`).** Crater excavation and shock heating still use separate energy
accountings (a flagged simplification — full coupled conservation is the MLS-MPM/shock-EOS future).

---

## 2026-07-09 — Visual: glowing molten ejecta + a Meteor you can fire

**What.** The first visible slice of impact damage beyond text (`docs/20`): impact ejecta carry a
temperature, and molten debris **glows by black-body emission from that temperature**. Added
`Particle.temp_k`; `matter::impact` deposits heat that peaks at the contact and falls to cold at the
crater rim (centre melts/vaporizes, rim is cold rubble — the honest radial gradient); `emission::
incandescence(temp_k)` maps K → an added RGB glow (dull red → orange → yellow → white); the particle
shader **adds** it, so hot debris self-illuminates even on the dark side (it *emits* because it's hot —
the analogue of illumination × reflectance, `docs/17`). A **Meteor** control (`Engine::meteor`, the
`☄`/`m` button in the terrain slice) fires a high-energy `impact` you can watch and orbit into.

**Why.** Robin: "see the impact, then zoom in and see the crater" (with glowing melt). Delivered in the
*terrain* renderer (which renders on-device) so it's verifiable now; the celestial→voxel auto-fly-in
(materialising the Moon-crash crater from its summary) stays staged (`docs/19`).

**Verified.** `emission::cold_matter_does_not_glow_and_hotter_glows_brighter_and_whiter` and
`matter::a_big_impact_melts_the_centre_and_leaves_the_rim_cold`. `cargo test` 40/40; clippy
`-D warnings` clean; fmt clean; wasm + `tsc` green; deployed. The *look* of the glow is for Robin's
on-device check.

**Honest caveat (`docs/20`).** The crater extent is physical (energy/σ), but the ejecta *temperature*
distribution is a first visual model — the energy is not yet conserved through the phase change, and
`incandescence` approximates the Planckian locus. Next (Robin's order): integrate the phase classes
into `matter::impact` proper (voxels → gas/melt/ejecta, energy-conserving), then MLS-MPM.

---

## 2026-07-09 — Impact thermodynamics: fracture → melt → vaporize (one rule)

**What.** Modelled fragmentation, melting, and vaporization as **one data-driven response** (`docs/20`),
Robin's planetary-scale test of the engine (and of scale-of-detail). An impact deposits **energy
density** (J/m³ = Pa); each parcel's fate comes from comparing it to that material's own thresholds:
fracture strength → melt energy `ρ(cΔT+L_f)` → vaporization energy. `damage::classify` returns
`Intact | Fractured | Melted | Vaporized` — the *same* "density vs threshold" logic as fracture, just
higher thresholds. Because the deposited density falls with distance, **one event produces all four at
different radii** (near-field vaporizes, then melts, then fractures, then intact). Added optional
`Material.thermal` (specific heat, melt/boil points, latent heats) with **cited data** for basalt,
granite, iron, water; materials without it can only fracture (we don't claim unknown melt behaviour).

**Why.** Robin: "model fragmentation, melting, vaporization — a test of our simulator's abilities on a
planetary scale (and of our scaling of detail)." A giant impact honestly vaporizes rock near contact,
leaves a magma ocean of melt, fractures/ejects a shell, and — since E ≪ Earth's binding energy — leaves
the planet intact but resurfaced. Every one is the same `classify` at a different radius.

**Verified.** `damage::impact_fractures_then_melts_then_vaporizes_by_energy_density` (thresholds order
σ<melt<vapor; each band classifies right; a giant-impact density vaporizes rock; no-thermal-data →
fracture-only). `cargo test` 38/38; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Staged (docs/20):** integrate into `matter::impact` (voxels become gas/melt/ejecta by class,
conserving mass + energy through the transition); the **visual** display — incandescent melt (black-body
emission from temperature, not a painted colour), a vapor plume, and the materialised crater to fly
into (`docs/19`); cooling/solidification (magma → rock).

---

## 2026-07-09 — Two-moon stress test scene

**What.** A new scene (`/twomoons.html`): two moons on the same orbit, **opposite sides** of the Earth,
that you **de-orbit both at once**. Generalized `OrbitDemo` from one moon to N — `[Sun, Earth, Moon,
Moon2]`, a moon uniform per body, per-moon lighting/framing, and collision resolved Earth-vs-each-moon
with each moon's impact energy counted once (the two hits **sum** in the HUD). `brake_moon`/`drop_moon`
now act on *all* moons; focus cycles Earth → Moon A → Moon B; the second moon is placed at −d with the
opposite tangential velocity so both orbit the same way and stay diametrically opposed. The two HTML
pages share one script — the moon count comes from `<body data-moons>`.

**Why.** Robin: "It's our universe, we might as well play in it." The N-body core (`orbit.rs`) is
already generic, so two moons is nearly free physically; its value is **stressing the collision path** —
two simultaneous surface contacts, symmetric resolution, and (later) two craters materialising at once.

**Verified.** `cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
`/twomoons.html` serves. Visuals (two moons, symmetric de-orbit, double impact) pending Robin's
on-device check.

---

## 2026-07-09 — LOD-adaptive damage: the crater bridge (celestial ↔ voxel)

**What.** Connected the Moon-crash to a real crater across scales (`docs/19`). The bridge: a damage
event is the *same event* at every LOD, so the coarse **summary** and the fine **voxel materialisation**
must agree. Both use the same `σ·V` accounting — `damage::crater_volume(E, σ) = E/σ` (celestial
summary) equals the voxels `matter::impact` excavates (proven:
`matter::voxel_crater_matches_the_coarse_damage_summary`). Added honest **regimes**: strength crater
(`V=E/σ`), gravity regime (flagged, unmodelled), and **disruption** past the body's binding energy.

**Honesty — the Moon is not a tidy crater.** ~4.5e30 J is ~36× the *Moon's* binding energy (the Moon
**shatters**) but only ~2% of the *Earth's* (~2.2e32 J), so the Earth **survives with a planet-scale
crater** — the giant-impact regime, not a neat bowl. The space-band HUD now says exactly this on impact
(`damage::moon_shatters_but_earth_only_craters` pins the numbers). We report the regime honestly instead
of promising a crater the physics forbids.

**Why.** Robin: connect the Moon-crash to a real crater. The honest connection is the σ·V bridge — the
same relation drives the celestial summary and the zoomed-in voxel crater, so promoting/coarsening
across LOD conserves the event (`docs/13`). The *visual* zoom-in (fly the camera down and materialise
the voxel crater) is a real renderer effort, designed in `docs/19`, staged for on-device work.

**Verified.** New: `damage::crater_scales_with_energy_and_inversely_with_strength`,
`damage::moon_shatters_but_earth_only_craters`, `matter::voxel_crater_matches_the_coarse_damage_summary`.
`cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Roadmap (Robin's order):** LOD (this — bridge done; visual zoom-in next) → MLS-MPM → fluid. Planned
playground: a **two-moon** scene (opposite sides, same orbit, de-orbit both at once) as a stress test —
the N-body core is already generic, so it's nearly free.

---

## 2026-07-09 — Unified deformation & damage: the design + first honesty slice

**What.** Started the deformation/damage subsystem (`docs/18`) from Robin's requirement that a **bullet,
a pebble in a pond, and the Moon hitting the Earth be the SAME operator** — differing only in
parameters and level of detail. The design names two invariances: (1) **material** — the response comes
from constitutive data (solids fracture at strength, granular media crater, liquids yield at ~0 and
flow), so bullet-in-rock and pebble-in-pond are one call with different material; (2) **scale/frame** —
the observer's frame/zoom decides what is materialized (celestial: energy/momentum + crater summary;
zoom in: voxel fracture + ejecta; zoom way in: grains/buildings), promoting/coarsening across LOD while
conserving mass/momentum/energy. Two concrete slices landed: (1) parse material **`phase`** and fix the
liquid fudge — water's `fracture_strength` used to fall back to `1e12` (stronger than granite!); a
fluid now yields at ~0. (2) `MatterSim::impact(site, direction, energy)` — the **generalized
energy-driven impact**: it spends the impact energy fracturing voxels nearest-first (σ·V per voxel), so
bigger energy → bigger crater, stronger material → smaller crater, and a liquid splashes. A 10 g bullet
(~450 J) and the Moon (~4.5e30 J) are the *same call*.

**Why.** Robin: the same system should observe a bullet, a pebble in a pond, or a planetary impact —
and at a given scale we simulate only what the observer can perceive (buildings only matter zoomed way
in; ejecta only zoomed in; celestial scale cares about energy/momentum and a crater summary). This is
the honest unification of the voxel-fracture model (`matter.rs`) with scale-relative fidelity
(`docs/13`, `docs/08`) — the endpoint is MLS-MPM with per-phase constitutive models.

**Verified.** `materials::a_liquid_yields_where_a_solid_resists` (a fluid yields to a poke a solid
withstands) and `matter::impact_is_material_and_scale_invariant` (same energy craters dirt but not deep
granite; more energy → bigger granite crater; a gentle impact still splashes a pond). `cargo test`
34/34; clippy `-D warnings` clean; fmt clean; wasm compiles.

**Roadmap remaining (docs/18):** fluid flow (needs a viscosity field, not in the DB yet) → MLS-MPM
constitutive unification → LOD-adaptive damage (summary ↔ detail on zoom). Robin: "we should get to the
rest before we're done."

---

## 2026-07-09 — Orbital-decay control: brake the Moon until it crashes (with real collision)

**What.** The requested experiment — slow the Moon and watch its orbit decay into the planet — plus the
honest physics that makes a "smash" real rather than a numerical explosion. `orbit::resolve_contact`
adds **surface collision**: two solid bodies stop when their surfaces meet (perfectly inelastic,
momentum-conserving), instead of tunnelling through each other as point masses into a 1/r² singularity
— the celestial echo of the voxel body contacts (`docs/16`). `orbit::perigee` computes the live
closest-approach so the HUD can show the orbit tightening. OrbitDemo exposes `brake_moon` (halve the
Moon's velocity relative to Earth), `drop_moon` (cancel it → radial plunge), `reset_moon`, plus a
variable **time multiplier** in the HUD. The web control bar gains Brake / Drop / Reset + slower/faster,
and the HUD shows perigee (reddening below Earth's radius) and "💥 IMPACT".

**Why.** Robin wanted to watch the Moon smash the Earth. The honest lesson is built in: in a
conservative two-body system a *single* halving does NOT crash — it drops into a tighter eccentric
ellipse (perigee ~55,000 km, still a miss); it takes a few brakes (or a full drop) to push perigee
below the surface. Real orbital mechanics, shown, not faked. Also exposed the time multiplier per
Robin's note (and it lets you slow time to watch the impact).

**Verified.** `cargo test` 31/31 — including `perigee_tracks_how_hard_the_moon_is_braked` and
`a_dropped_moon_crashes_into_the_planet_and_stops_at_the_surface` (it reaches the surface and rests
there, no tunnelling). clippy `-D warnings` clean; fmt clean; wasm + `tsc` green. Visuals pending
Robin's on-device check.

**Impact energy (honesty).** Robin noted that at these masses an impact must do *damage* — and that a
perfectly-inelastic "stop at the surface" silently *deletes* the kinetic energy, which is itself a
fudge. So we now **measure and report** it: `orbit::inelastic_dissipation` (the KE the collision
removes) and `orbit::binding_energy`. A dropped Moon hits at ~11 km/s → ~4.5e30 J ≈ **36× the Moon's
gravitational binding energy**; the HUD shows this and states plainly that both bodies would be
destroyed. We measure the damage rather than hide it or fake it.

**Honest scope note.** "Collision" here is surface contact + inelastic stop, plus the reported impact
energy; actual **fragmentation** (deformation, melt, debris, merging) is a future subsystem — the
honest zoom-in unification of the voxel-fracture model (`matter.rs` `fracture_strength`) at scale.
Flagged, not faked.

---

## 2026-07-09 — Live real-Sun lighting, selectable focus frame, scene picker

**What.** Wired the real Sun into the *live* space band (following the validated physics): the demo now
simulates `[Sun, Earth, Moon]` with the Earth on its true ~29.78 km/s heliocentric orbit and the Moon
co-moving. The shader's light direction is now computed per-body **from the Sun's actual position** (no
more hardcoded direction), so the lit hemisphere and the Moon's phases are geometric. The Sun isn't
drawn at this zoom (~23,000 display units off-frame) — it is the *light source*, the scale-adaptive
choice (`docs/17`). Added a **focus control**: the viewport is a physical frame of reference
(`cycle_focus` / `focus_label`), re-centring the whole view on Earth or the Moon. And a **scene picker**
(`web/src/scene-nav.ts`) injected on both pages to switch between the terrain slice and the space band.

**Why.** Robin's direction: a real Sun should light the system (not a fake light), the viewport is a
physical frame of reference with a selectable focus, and the app should let you choose between scenes.
All three are honest, emergent-from-real-state changes (`docs/17`).

**Verified.** `cargo test` 29/29; clippy `-D warnings` clean; `cargo fmt` clean; wasm builds and
`tsc --noEmit` passes (focus + scene-nav bindings). **Visuals pending Robin's on-device check** —
headless WebGPU can't render here, so the appearance of the sun-lit bodies and the focus/scene UI is
for iPad confirmation.

---

## 2026-07-09 — Honest appearance: no painted tints, brightness from light, a real Sun

**What.** A user play-test of the space band exposed fudging: the Earth was a hardcoded ocean-blue
tint and the Moon a hardcoded grey — cosmetic colours touching no material data, even though the
terrain already colours voxels from real `materials.json` albedos. Replaced with honesty (`docs/17`):
(1) body colour = **aggregate albedo of a real composition** via the new `materials::aggregate_albedo`
operator (Earth = ocean water + continental granite + polar ice; Moon = basalt) — a computed summary,
not a paint job; (2) the space shader now does **illumination × reflectance** (bright sun × real, often
dark, albedo) + Reinhard tone-map, so a dark-but-lit body reads bright — the honest reason the Moon
looks bright; (3) added a validated **Sun–Earth–Moon** physics test: a real Sun (1.989e30 kg, 1 AU) and
the Earth given its **appropriate heliocentric velocity** (~29.78 km/s), with the Moon staying bound to
the moving Earth.

**Why.** The user pushed the honesty invariant (`docs/15`) all the way down: *don't fudge*. Key
insights captured: brightness is illumination × reflectance (not a bright material); even albedo is a
summary placeholder for real optics (ray tracing is the goal); zoom-out summaries are fine only if
*computed from everything we know* by one operator for all objects/scales; the illuminant should be a
real Sun; the viewport is a **physical frame of reference** with a **selectable focus** (planet →
Moon → …); and the core research question is whether the system can tell **what matters at a given
scale**. Working principle / candidate name: **"Integrity."**

**Honesty flags (not hidden).** Earth composition excludes the atmosphere → deliberately no Rayleigh
blue (the blue-marble blue is atmospheric, unmodelled); Moon lacks highland anorthosite in the DB → it
renders darker than reality until added; the shader's sun *direction* is still a placeholder until the
real Sun is wired into the live view.

**Verified.** `cargo test` 29/29 (new `aggregate_albedo_summarizes_real_constituents`,
`sun_earth_moon_system_is_bound`); clippy `-D warnings` clean; fmt clean; wasm compiles. The *visual*
result of the new lighting is for on-device confirmation (headless WebGPU can't render it here).

**Staged (larger, honest work):** real Sun as the live illuminant + heliocentric re-centering + focus
switching; ray tracing; specular/BRDF from roughness/metallic; stellar & anorthosite materials;
atmosphere for the earned blue; and the still-owed orbital-decay control.

---

## 2026-07-09 — Unified dynamics: everything not at rest reacts

**What.** Fixed the "probe quits falling / doesn't really react to debris" behaviour by unifying the
probe and the debris into **one awake-set dynamics loop** (`docs/16`). Previously `body::Sphere` (the
probe) and `matter::MatterSim` (debris) were separate systems coupled only through the voxel grid —
`matter.rs` never referenced the probe — so particles couldn't push it and settling debris deposited
voxels *inside/under* it, making it appear to rest on nothing. Now, per substep, every awake body
integrates under the same gravity field, resolves body↔world contacts, debris steps under that field
and **won't deposit inside a body** (piles on it, conserving matter), and **body↔debris contacts
exchange momentum both ways** (`MatterSim::couple_body`). Sleep/wake is structural: a body sleeps only
while in contact and slow, and wakes the instant support is removed or something hits it.

**Why this shape.** The user's principle: a physics loop looks at every object *not at rest* and makes
it react as a natural property of the world and the object, never a per-object script — the honesty
invariant (`docs/15`) applied to dynamics. Also captured the deeper motive: an honest, inferable
physical world is a place to *learn to act* (VR, and plausibly embodied-AI training), a payoff that
exists only to the degree the sim refuses to fake.

**Also (honesty corrections from the user).** (1) No atmosphere is modelled — matter falls through
*vacuum*, so the per-step `DRAG` constant is flagged as a numerical-stabilizer debt, not real air drag.
(2) Compute-budget policy written down: favour larger/more massive objects (massive bodies are
budget-exempt today; debris coarsening must *merge into mass-carrying clumps*, conserving mass on both
spawn and settle — so it's deferred, not half-done, to avoid a mass leak). (3) Noted the
server-authoritative-world / client-sees-a-slice threshold to watch (`docs/11`, `docs/13`).

**Verified.** New native tests: `particle_transfers_momentum_to_a_body` (momentum conserved through the
impact), `debris_does_not_settle_inside_a_body`, `body::wakes_and_falls_when_support_is_removed`.
`cargo test` 27/27; clippy `-D warnings` clean; `cargo fmt` clean; `cargo check --target
wasm32-unknown-unknown` green (the awake-set loop lives in the wasm-only host).

---

## 2026-07-09 — Representation invariant: the cube is a lattice, not a unit of matter

**What.** Answered a foundational design question — "are we baking a core mistake into the engine by
building on cubes, when the universe is made of spheres?" — and locked the answer in as canonical.
Wrote `docs/15`: **a voxel is a sampling cell, never a unit of matter.** The cubic grid is the
coordinate lattice we sample continuous fields on (density, material, momentum), like pixels sample an
image; it is not an ontology of blocks. All physical state lives on matter with continuous coordinates
(`Particle.pos`, `MassPoint`), and bulk voxels dissolve into particles the instant physics touches
them (`docs/08` tiers). Added a **grid-isotropy regression suite** (`isotropy.rs`) to enforce it.

**Why.** The honest answer is that cubes are *not* a foundational mistake — roundness is emergent, not
primitive. Real solids sit on lattices (many cubic — rock salt, BCC iron), yet planets are round
because isotropic self-gravity averages over the lattice; the engine already mirrors this (aggregate
mass → spherical far field in `gravity.rs`/`orbit.rs`; surface nets smooth the render). The *real*
risk is subtler: a regular lattice has preferred directions (axes, 45° diagonals) and a solver could
silently bake that bias into the physics. Also captured the user's north star: the world should **feel
right in VR because it is right, not via per-object fakery** — leave something unsupported and it
falls as a natural property of the world and the object (`find_unsupported` → `collapse`), never a
script.

**Verified.** New suite asserts (a) gravity on a symmetric ball is radial + equal-magnitude across
face axes and edge/corner diagonals (spread < 1%, tangential < 1%), and (b) `dig` carves a true
Euclidean sphere (volume within a few %, equal axis reach, no lateral ejection bias). Proven
**non-vacuous** via mutation testing: an injected axis bias in the gravity sum and a Chebyshev (box)
dig criterion both drove the guards red (gravity spread 9.7%; box removed 8000 vs a sphere's 4189),
then reverted. `cargo test` 24/24; clippy `-D warnings` clean; `cargo fmt` clean.

---

## 2026-07-09 — Space band: watch the Moon orbit (v0.9.0)

**What.** Step A of the scale-relative "orbit-to-ground" (`docs/13`): a spectator view of the real
Earth + Moon (`/orbit.html`). `OrbitDemo` runs `orbit.rs` (real SI, f64) each frame and renders two
lit spheres via a tiny new `space.wgsl` (position/normal + per-body tint + one directional sun, so we
get phases). Metres → display units (Earth radius → 1); the Moon sits ~60 units out. Time-scaled so a
~27.3-day orbit plays in ~20 s, substepped 16× for a stable symplectic step. HUD reads live
separation (~384,400 km). Kept on a separate page + Vite multi-page input so the terrain slice is
untouched.

**Why this shape.** I can't self-verify visuals here (headless WebGPU won't render the pipeline), so I
minimized blind risk: reuse the *proven* GPU setup pattern, the existing sphere mesh + `draw` path, and
lean on the already-validated physics (`orbit::moon_orbits_earth`). The renderer is a thin shell over
known-good pieces; the hard part (the orbit) is the tested part.

**Also.** Wrote `docs/13` (north-star: observer-relative fidelity) and `docs/14` (validation
demonstrations — each physics test mapped to what it proves + how to *show* it), at the user's request
to preserve the test concepts as demonstrations for the full build.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm build compiles `OrbitDemo` warning-
free; `tsc` clean; LAN dev server serving `/orbit.html`. Visuals to be confirmed on-device.

---

## 2026-07-09 — Solid-object collision + orbital-mechanics validation (v0.7.2, v0.8.0)

**Collision (v0.7.2).** From an iPad play-test: the probe clipped into crater walls (looked like a
duplicate ball, rested too high) because it only collided with the terrain column directly beneath
it. Replaced with proper **sphere-vs-voxel collision** (`body.rs`): integrate under gravity, then
iteratively push out of the deepest solid voxel the sphere overlaps (floor, walls, corners) with
restitution + friction. Solid objects act solid.

**Orbital validation (v0.8.0).** Added `orbit.rs` — N-body point-mass gravity + a symplectic
velocity-Verlet integrator. The native test drops in the **real Earth + Moon** (masses, 384,400 km,
1.022 km/s) and confirms a bound orbit: ≥1 full revolution, distance within 15% of real, energy +
angular momentum conserved <1%. This proves the gravity law reproduces real celestial motion — the
"does the Moon orbit the planet?" test — and, importantly, it's a **pure native test** (no rendering),
so it verifies the physics despite headless WebGPU being unavailable here.

**Note on tooling.** Headless Chromium here renders WebGPU only via software (SwiftShader) or hits a
Dawn instance bug on the real GPU, so I can't screenshot the full render; I lean on native tests
(watertight mesh, collision, orbit) + the user's iPad for visual confirmation. `web/screenshot.mjs`
is kept for environments with GPU access.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm + web build green.

---

## 2026-07-08 — Phase 6: smooth surface meshing (v0.7.0)

**What.** Terrain and craters now render smooth instead of blocky cubes. `mesher::build_surface_nets`
runs Surface Nets (`fast-surface-nets` crate) over the voxel occupancy field, recomputes smooth
normals from the geometry (oriented outward), and tags each vertex with its nearest material so
triplanar texturing + shine still apply. The renderer uses it for the initial terrain and every dig
re-mesh; the blocky mesher is kept as a fallback.

**Why.** The user flagged the Minecraft-blocky look. The key insight: the voxel grid is the *physics
substrate*, not the *visual* — so we smooth the rendering (marching-cubes/surface-nets style) while
mass, gravity, fracture, and collapse stay identical. Prototype clunkiness → smooth surface, no
physics change.

**Verified (TDD).** `cargo test`: 19/19 (new: surface-nets mesh is valid, finite, and genuinely
smooth — has non-axis-aligned normals). fmt + clippy (`-D warnings`) clean; wasm + web build green.
Live LAN wasm rebuilt. **Pending human check:** reload → rounded terrain and craters, still textured
and lit; dig/blast/collapse all still work.

**Next realism levers (noted):** smoothed/SDF field for rounder geometry, normal maps from the grain
field, finer/smoother debris (or MPM).

---

## 2026-07-08 — Phase 5: structural collapse (v0.6.0)

**What.** Undercut or isolated matter no longer floats. `world.find_unsupported()` flood-fills from
the anchored base (`y=0`) and returns any solid voxel not connected to it; `MatterSim::collapse()`
detaches those into falling particles, run after every dig. This closes the Phase-3 "floating voxels"
known limitation — overhangs, undercuts, and blasted-off chunks all fall and re-settle.

**Why.** Real matter needs support. Connectivity-to-anchor is the general, correct model (works on a
plateau now and a planet core later) and needs no per-case rules.

**Verified (TDD).** `cargo test`: 18/18 (added: intact terrain has no unsupported voxels; an isolated
voxel collapses, conserves matter, and re-settles). fmt + clippy (`-D warnings`) clean; wasm + web
build green. **Pending human check:** `npm run dev` → shift-click to undercut a ledge and watch the
overhang break loose and tumble down.

---

## 2026-07-08 — Phase 4: emergent textures (v0.5.0) — vertical slice complete

**What.** Materials now look distinct, generated *from their own properties* with **no bundled
images**. `texture.rs` synthesizes a high-res (512²) mip-mapped texture per material from
albedo + color_variance + metallic (grain/mottle + flecks + metal sparkle), seamless. The world
shader triplanar-samples a per-material texture array and adds a specular highlight (shine) from
per-material roughness/metallic. HUD gains an FPS counter. `docs/12` documents the approach + CC0
sources (ambientCG/Poly Haven) for optional user textures.

**Why.** Closes the appearance side of the thesis: look emerges from the same cited data that drives
mass, gravity, and fracture — one source of truth. User asked for high-res + no licensed photos;
procedural generation delivers both (mipmaps = scale-down; zero image assets = zero licensing).

**Verified (TDD).** `cargo test`: 16/16 (added 4 texture tests: size+mip chain, mean tracks albedo,
materials differ, non-flat variation). fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` +
`vite build` green. **Pending human check:** `npm run dev` → speckled granite, mottled dirt, green
grass, a shiny iron probe; dig to see textured debris.

**Milestone.** This completes the **Phase 0–4 vertical slice** from the plan: layered voxel matter ·
self-gravity (F=ma) · dig & material-driven fracture · emergent texture — all driven by the cited
material database. All four project pillars are demonstrable.

---

## 2026-07-08 — Phase 3: dig & material-driven fracture (v0.4.0)

**What.** Destructible matter. `matter.rs` is a CPU matter solver: click-to-dig (voxel raycast DDA)
fractures a spherical region — a voxel detaches into a particle only if the tool's stress exceeds its
material's `fracture_strength` (loaded from the cited DB). Debris falls under the Phase-2 gravity
field and, on rest, deposits back into the voxel grid (piling, matter-conserving). Instanced debris
rendering (`particles.wgsl`), terrain re-mesh on edit, HUD debris count. Click digs soil/grass;
shift-click blasts rock.

**Why.** Proves the core destruction thesis — materials break *differently by their own numbers*
(granite shrugs off what shreds grass), with no per-material special-casing. Framed honestly as the
**CPU, testable foundation** for full continuum MLS-MPM (deformation/stress + WGSL port) later, since
GPU MLS-MPM can't be unit-tested natively and TDD is canonical.

**Verified (TDD).** `cargo test`: 12/12, incl. `dig_detaches_soft_but_not_hard` (soil detaches under
1e6 Pa, granite needs a 2e7 blast) and `matter_conserved_through_dig_and_settle` (voxels + airborne
particles == original, every step, until all settle). Plus raycast-hits-terrain. fmt + clippy
(`-D warnings`) clean; wasm build clean; `tsc` + `vite build` green.
**Pending human check:** `npm run dev` → click the grass/dirt to blow a crater of tumbling debris
that resettles; click rock (nothing) then shift-click (it breaks).

**Known limits (noted for later):** mid-column digs can leave floating voxels (no structural
collapse yet); full-world re-mesh per edit (dirty-chunk meshing is the optimization).

---

## 2026-07-08 — Phase 2: self-gravity + falling probe (v0.3.0)

**What.** Made density physically active. `gravity.rs` computes a real Newtonian field from the
world's aggregate voxel mass (voxels lumped into blocks; direct-sum with f64 accumulation).
`body.rs` integrates a rigid sphere under that field (`F = ma`, semi-implicit Euler) with ground
contact and a scale-relative rest threshold. The renderer draws the probe via a per-object model
matrix; a live HUD shows world mass, local gravity, altitude, speed, rest state, and time-scale
(`Space` re-drops, `[`/`]` change time-scale).

**Why.** Proves pillar 4 — the world's own summed mass produces gravity; a probe obeys `F = ma` and
rests on the surface. No Rapier yet: one hand-integrated body is exact and far simpler; Rapier is
deferred until many bodies/contacts justify it.

**Honest scale note.** Real `G` is used, so the ~96 m world has asteroid-scale micro-g (~1e-5 m/s²).
That's correct physics; a time-scale fast-forwards the sim for viewing (time-lapse, not fake gravity).

**Verified (TDD).** `cargo test`: 9/9 — point-mass `G·M/r²`, far-field within 1%, mass conservation,
free-fall kinematics (`v=-g·t`, `½g·t²`), fall-and-rest, and an end-to-end drop onto the generated
world. fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` + `vite build` succeed.
**Pending human check:** `npm run dev` → watch the iron probe fall and settle; HUD reads out g and rest.

---

## 2026-07-08 — Phase 1: layered voxel world on screen (v0.2.0)

**What.** Turned the material data into a rendered world. Added to the engine crate:
- `materials.rs` — loads the cited `data/materials.json` (density + albedo) at compile time.
- `world.rs` — chunk-style voxel store + a layered generator: rock bulk, ~10 m dirt, grass skin,
  with a deterministic value-noise heightfield so the surface undulates (layers follow terrain).
- `mesher.rs` — face-culling mesher (only air-facing faces), per-material albedo vertex colors, so
  the rock/dirt/grass bands are visible on the exposed side walls.
- `lib.rs` + `shaders/world.wgsl` — a real 3D renderer: vertex/index/uniform buffers, depth buffer,
  perspective orbit camera, and a directional light + ambient/hemispheric fill.
- `web/` host: drag-to-orbit / scroll-to-zoom controls, gentle idle auto-rotation.
Also added `docs/10` (robustness — how the matter-first model designs out tunneling / fall-through /
"weird physics", plus the mitigations and an adversarial test plan).

**Why.** First milestone that makes "density as source of truth" *visible* and validates the core
Rust→WASM→wgpu render path end to end, on the real seed data.

**Verified.**
- `wasm-pack build` clean (no warnings). `tsc` clean. `vite build` succeeds (wasm ~1.32 MB dev).
- Dev server serves `engine_bg.wasm` as `application/wasm`.
- `cargo test` (native): material DB loads 19 materials with granite denser than dirt; the central
  column is grass→dirt→rock top-to-bottom and solid to y=0; mesher output is well-formed (quad-aligned
  vertices, 6 indices/quad, all indices in range).
- **Pending human check:** `cd web && npm run dev` in a WebGPU browser — a layered rock/dirt/grass
  plateau you can orbit and zoom.

**Version.** Milestone **0.2.0** (Phase 1) per the pre-1.0 policy (each phase bumps the minor).

---

## 2026-07-08 — Materials seed database + object/interaction design

**What.** Compiled the first **cited physical-properties database** — 19 materials (rock, ceramic,
metal, organic/wood, soil, granular, liquid, frozen) with mechanical + optical properties and source
URLs — into `data/materials.json` (schema in `docs/04`). Added design docs for the architecture the
user articulated: material **taxonomy + finishes + object composition** (`07`), **adaptive resolution
& clumping** so the sim scales instead of moving billions of particles (`08`), and **agentic object
authoring + physically-grounded tool/terrain interaction** — the "make a shovel" / shovel-in-dirt
vision (`09`).

**Why.** Physical properties are the single source of truth for both simulation and rendering; the
whole object/agentic vision ("make a shovel" that falls, sounds, and digs like one) reduces to
material data + physics + composition, with no bespoke per-object code.

**Verified.** `data/materials.json` parses (node `JSON.parse`), 19 materials each with mechanical +
optical blocks; categories: rock 4, ceramic 1, metal 3, organic 3, soil 2, granular 2, liquid 2,
frozen 2. Research quality-checked: rejected known-bad MatWeb figures (granite/limestone UCS),
flagged cited-vs-estimate, and captured state-dependence (soils/snow) and anisotropy (wood).

**Note.** JSON is the v0 seed; it migrates to the Postgres source of truth (`docs/05`) and grows into
the module/taxonomy system (`docs/06`, `07`) over time.

---

## 2026-07-08 — Published to GitHub as a monorepo

**What.** Restructured the engine into the `robinmack/BotheadStudios` monorepo as its first
project directory, `integrity-engine/`. Root of the monorepo carries an MIT `LICENSE` and a
projects README. Aligned the engine to **MIT-only** (dropped the Apache dual-license) to match the
repo's license choice. Published the public OSS repo and tagged `v0.1.0`.

**Why.** BotheadStudios will hold multiple game projects; a monorepo keeps them together. MIT
across the board keeps licensing simple and consistent.

**Verified.** `git push` to `origin/main` succeeded; `v0.1.0` tag pushed; repo is public.

---

## 2026-07-08 — Project kickoff & Phase 0 scaffold

**What.** Created the engine as the first project in the **BotheadStudios monorepo**
(`integrity-engine/`). Established the skeleton: `crates/` (Rust core), `web/` (TypeScript host),
`shaders/` (WGSL), `docs/` (research + design).
Added `README.md`, `LICENSE-MIT`, `CONTRIBUTING.md`, `.gitignore`, this journal.
Installed the toolchain: Rust 1.96.1 + `wasm32-unknown-unknown` target + wasm-pack 0.13.1 (Node 22 already present).

**Why.** The plan (see `.claude/plans/…` / `docs/`) settled a performance-first stack — Rust→WASM core,
custom `wgpu` WebGPU renderer, Rapier rigid bodies — after research confirmed **no existing engine fuses
all four pillars** (density-as-truth matter · emergent-from-density behavior · destructible-to-the-core ·
real self-gravity). See `docs/01-prior-art-existing-engines.md` and `docs/02-oss-building-blocks.md`.

**Phase 0 goal.** Prove the pipeline end-to-end: a Rust crate compiled to WASM initializes a `wgpu`
device and clears a browser canvas, driven by a thin Vite/TypeScript host. First pixel on screen.

**Verified (build/serve level).**
- Rust → WASM compiles via wasm-pack (fixed three `wgpu` 24.0.5 API differences vs. older docs:
  `request_adapter` returns `Option`, `request_device` takes a trailing `Option<&Path>` trace arg,
  and `RenderPassColorAttachment` has no `depth_slice` field).
- `npx tsc --noEmit` clean; `vite build` bundles the app (wasm 933 KB → 236 KB gzipped).
- `vite` dev server serves `engine_bg.wasm` as `application/wasm` (verified magic bytes `\0asm`).
- **Pending human check:** open `npm run dev` in a WebGPU browser to see the pulsing clear color.

**Version.** Tagged this milestone **0.1.0** (see `CHANGELOG.md`, `docs/03-versioning.md`).
Pre-1.0 policy: each roadmap Phase bumps the minor; games pin exact versions since we dogfood.

---
