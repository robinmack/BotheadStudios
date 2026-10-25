# docs/73 — The renderer, redesigned around the footprint (discussion draft)

**Status: DRAFT for discussion with Robin, 2026-10-24. Nothing in it is built except what docs/46 row 90
names.** Spec revisions are discussed before work starts; this document is that discussion, written
down so the next session inherits it instead of re-deriving it. It builds on docs/68 (the renderer owns
light), docs/63 (the appearance integral and its convergence rule), docs/44/49 (resolution by necessity,
the camera-granularity law) and docs/46 rows 35, 88, 90 and 91. Where it restates one of them it says so.

## 0. Robin's words, which are the spec

> *"The entire system is expected to change scale not in abrupt tiers, but in response to nearness and
> ability to render and perceive detail. If you take a series of screenshots from 100 km and continue to
> take them as you approach and compare that with actual footage of weather balloon descents … you'll see
> what I'm complaining about."*

> *"This is the early stages of rendering, and a good opportunity to redesign the rendering portion
> (assuming it's still logically separate from the physics engine)."*

> *"At this height, I imagine the trees/other flora could be rendered as a change to the texture and we
> can create more of them to represent a forest, not just a strangely circular patch of fudge.
> Everything must change based on POV of camera in the renderer, all fed from model/assembly
> properties."*

Three rules fall out of the last sentence, and they are the whole design:

1. **The model answers what is there.** The land-cover mixture, the plant assemblies and their
   properties (crown, height, leaf area, albedo in season), the cover fraction, the season, the damage
   record. It never answers how to draw it.
2. **The renderer decides representation per pixel footprint**, from the camera's point of view and
   nothing else: how much ground one pixel sees, and whether a plant in it spans many pixels, a few, or
   less than one.
3. **Every representation integrates to the one below it.** A canopy texture's mean over a footprint
   equals what the resolved trees return over the same footprint (docs/63's rule, docs/68 §6). If flying
   lower changes the colour of a forest, one of the two is lying.

## 1. The measured state, 2026-10-24 (docs/46 rows 90 and 91)

`native_render_tests::a_descent_is_photographed_and_the_plants_are_measured`, 54.34 N 158.23 E (IGBP
class 7, open shrubland: foliage 0.25, sand 0.40, dirt 0.35), straight down, 1024×768, each rung the
frame with the plants drawn against the identical frame without:

| altitude | plants | farthest | px per plant | pixels changed | disc edge |
|---|---|---|---|---|---|
| 100 km, 50 km | 0 | — | — | 0 | — |
| 20 km | 1,169 | 623 m | 0.37 | 430 | hard |
| 7 km | 1,169 | 623 m | 3.1 | 3,610 | hard |
| 2 km | 1,169 | 623 m | 37 | 43,243 | hard |
| 500 m … 20 m | 1,169 | 623 m | 116–178 | 135k–208k | frame inside the disc |
| 5 m | 1,169 | 623 m | 0 | 0 | no tree in frame |

Three separate things were in her picture:

- **The circle of dots** is the instance budget. `build_flora` resolves the 1,200 nearest plants of the
  tallest kind the renderer thinks worth resolving (an oak out to 25.6 km here), and for one kind the
  budget *is* the radius: `sqrt(budget / (π · density))` = 623 m at cover 0.25. The disc is the same
  623 m at every altitude from 20 km to 20 m, its edge is hard (the changed share is ~22% inside and
  zero in the first band outside), and nothing is drawn past it. Each dot is a sphere, because the oak
  assembly's crown is one. This is row 88 seen from above.
- **The pink** was the ground shader, not the plants (row 90, fixed in this PR): each vertex carried
  its footprint's colour as a ratio against its own dominant material, and the fragment multiplied it
  by the texture of the triangle's flat material. One footprint, two bases.
- **The fan and the wedges** are the appearance integral evaluated per *segment vertex*: a polar mesh
  of 96 rings × 192 spokes centred under the eye, cells kilometres long at altitude, colour
  interpolated along spokes and rings — and the whole pattern moves with the camera, because the mesh
  does. docs/47 names a change of representation that moves the world as the fudge seam. The integral
  is right; the footprint it is evaluated over is the wrong one.

Rule 2 is violated in both open cases the same way: the representation is decided by something other
than the pixel — by a count (the budget) or by a mesh cell (the vertex).

## 2. The design: appearance is a per-pixel-footprint integral, the mesh carries shape

### 2a. The ground

The appearance integral (`terra::appearance`, docs/63) already computes the right thing — mean albedo
and the slope variance about the mesh's own normal, over a footprint, refinement-invariant by the law of
total variance. Today its footprint is the mesh cell and its result is a vertex attribute. Under this
design its result is a **pyramid** the renderer samples at the footprint each pixel actually sees:

- each level of the pyramid is the integral over footprints of that size — mean albedo (in season),
  slope variance, and the canopy term of §2b — built from the land-cover mixture and the streamed data,
  so a level is not a filtered picture of the level below but the same integral asked over a larger
  footprint (which, by docs/63, must give the same answer; that is the test);
- the renderer picks the level from the pixel's own footprint on the ground (the derivative the GPU
  already supplies, i.e. the same quantity `Fidelity` derives from the viewport), so a frame from 7 km
  and a frame from 70 m sample different levels of one pyramid, and nothing changes in tiers;
- below the finest level the per-material grain texture takes over exactly as it does today, now as a
  unit-mean detail factor (row 90);
- the segment mesh carries position, normal and the measured relief only. Its topology stops deciding
  the colour of anything, so the fan cannot exist, and moving the camera moves the parallax and nothing
  else.

### 2b. The plants, as a change to the texture — and then as instances in the same places

Row 35 says a canopy is darker than its leaves because it is an arrangement, and docs/68 §6 says a
forest from above is a texture with holes where the cover says there is no crown. Robin's sentence says
the same: *"rendered as a change to the texture … create more of them to represent a forest."*

The term that enters the pyramid for a footprint is the appearance of the plants standing in it, which
the model already describes: which assemblies grow in this class, at what cover, with what crown,
height, leaf area and seasonal albedo. **Within a footprint the arrangement is the same deterministic
lattice `flora::scatter` already uses** (`cell_unit`: a plant's cell, jitter, size and yaw are facts
about the ground, docs/46 row 47). So the texture's crowns and clearings are generated from the lattice
the instances come from. That is what makes the hand-over continuous rather than a patch: when the
renderer decides a plant is worth instancing, the instance replaces the texture's crown *in the same
place, the same size, the same colour*, and the texture keeps drawing every plant the budget did not
reach. The forest is everywhere the cover says it is; what changes with altitude is only how many of
its trees are geometry.

The three rungs, from row 88, unchanged:

1. a plant spanning many pixels is its assembly mesh;
2. a plant spanning a few pixels is the same assembly at fewer vertices — a renderer approximation of
   HOW that is legitimate under docs/68 only because the difference is sub-pixel by construction;
3. a plant spanning less than a pixel is a share of its footprint's texture — the canopy term.

The count of drawn things is bounded by the screen, never by plants: the renderer instances what the
pixels can resolve, in order of angular size, up to the platform's budget, and the texture carries the
rest. The budget stops being a radius.

### 2c. What the renderer asks the model, and never recomputes (docs/68 §7)

Everything crossing the boundary is asked for. The protocol is small and most of it exists:

| question | answer | exists as |
|---|---|---|
| what is in this footprint | mixture, cover, season | `SurfaceSampler`, `biome_mix`, `senescence_fraction` |
| what grows here | assemblies, crown, height, leaf area, albedo in season | `flora::Kind`, `Assembly::crown_m2`, `albedo_when_turned`; leaf area is in the tuft's notes and must become data |
| where each plant stands | the lattice | `flora::scatter` (today it also truncates — the truncation moves to the renderer) |
| what happened to this plant | damage / gone | `flora_damage.about` |
| what is the ground's shape here | measured + generated relief | `ground_relief_m`, `TileStore` |
| what can this viewer resolve | one pixel in radians, the budget | `render::Fidelity` |

The model keeps deciding what exists and what is resolved for *interaction* (docs/68 §4). The renderer
decides nothing about matter.

## 3. The open choice, which is Robin's (docs/72)

How the canopy term is evaluated:

- **(A) rendered and averaged** — the assembly mesh drawn over its footprint at the arrangement's
  density and the result integrated. No new model; convergent to the instances by construction, since
  it *is* the instances. Costs a bake per (class, season, sun angle) or a run-time pass.
- **(B) a two-stream canopy** from leaf area and leaf albedo — a closed form, cheap, and a declared
  stand-in (docs/46 §1) until measured against (A).

A recommendation, not a decision: start with (A), because its convergence to the resolved trees is the
invariant this whole design rests on and (A) has it for free; measure (B) against it; keep (B) as the
far-field closed form only if the two agree within the gate. Either way the ledger records the seam.

## 4. Gates — written with the work, derived from measurements, never tuned

- **Footprint profile** (row 88's gate): along a descent the per-row share of pixels the plants change
  falls off with their angular size, with no band dropping by more than the per-row noise.
- **Colour continuity**: the frame's mean over a footprint is continuous across every rung boundary
  within that noise; doubling `instance_budget` does not move the far field (docs/44's sweep).
- **Refinement invariance of the pyramid**: a level equals the integral of the level below over the
  same footprint (docs/63's rule, already asserted for the vertex integral).
- **The picture does not move when the camera does**: render, move the eye one mesh cell sideways at
  fixed altitude, and the difference is confined to parallax — no colour moves with the mesh.
- **One footprint, one base** (row 90's gate): no magenta-family pixel across any class boundary.
- **The series Robin asked for**: `a_descent_is_photographed_and_the_plants_are_measured` is run at
  every step and its frames compared with the step before; a balloon descent has no tiers.

## 5. Order of work, each step measurable alone (docs/68 §8's rule: never extract to fix a bug)

1. ✅ **Row 90.** The vertex carries the integrated albedo; the texture supplies unit-mean grain.
2. **The ground's appearance pyramid, sampled per pixel**, with the vertex integral kept as the
   reference it must equal. Gate: the fan is gone; the camera-move test; refinement invariance.
3. **The canopy term into the pyramid** (the choice in §3), with the lattice-derived arrangement.
   Gate: colour continuity at the disc's edge — which stops being an edge.
4. **Pixel-bounded instancing with the rungs**, the truncation moved out of `scatter`. Gate: the
   footprint profile; the budget sweep.
5. **Then extract** (docs/68 §8 step 3): what leaves `Terra` is the segment's colour baking, the
   flora representation decision, `SkyVeil`/`atmos.wgsl`; what stays is shape, matter and time.

## 6. Where the code stands against "logically separate", honestly

By doctrine (docs/68) the renderer owns light. In code: twelve render pipelines and both render passes
live inside the two scene structs in `lib.rs` (11,291 lines); `renderer.rs` is 413 lines of shared
scaffolding; `render.rs` holds `Fidelity`, `Drawn` and `Region`; the twelve shaders are the only part
that is purely light. `build_flora` decides representation inside the model with a budget of 1,200
typed at its call site. The separation is a direction with one step taken (docs/68 §8 step 2), not a
state — which is exactly why §5 does the in-place steps first.

## 7. Risks, beyond docs/68 §7

- **Texture memory for the pyramid in WASM.** The coarse levels are global; the fine ones follow the
  camera like the tiles do, bounded the same way (the camera moving is the eviction).
- **A bake is a cache**, and this engine has been bitten by caches the model could not correct
  (`flora::Built`). Every level must be keyed on everything that changes the answer: season, damage,
  the tiles held.
- **Two answers for the arrangement.** The texture's lattice and the instances' lattice must be one
  function or they will drift; `cell_unit` is already that function and must stay the only one.
