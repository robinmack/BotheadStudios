# docs/63 — The pool table: one universe, every scale, scenes are just things placed in it

Robin (2026-08-01), and this is the sharpest statement of the architecture the project has produced:

> *"We should be able to zoom in from Mars (or Alpha Proxima) to Earth to the surface to a pool table and
> shoot a game of pool and have all the zooming and the pool table/balls/etc look good. … And the pool
> scene just happens to centre on a pool table we've assembled in the scene in a particular spot on Earth,
> everything else comes free whether we use it or not."*

And, on what that implies for the scenes that exist:

> *"If the same engine is doing all this, shouldn't the Ground scene be an excellent place to set up a
> harness, just move it around to different biomes on Earth? … All scenes should derive from the same
> universe the same way and render the same way with physics being identical, just at different scales."*

## What this actually says

It is not a feature request. It is a definition of what a scene IS, and it retires the one the engine still
half-uses:

- **There is ONE universe.** Earth is a body in it, defined once (`assets/bodies/earth.json`), with its own
  mass, layers, air and surface. Not a per-scene copy, not a per-scene approximation.
- **A scene is an ASSEMBLY placed at a coordinate.** The pool scene is a table and sixteen balls at a
  lat/lon. It does not own a planet, a sky, a terrain generator or a render path. Those are the universe's,
  and **they come free whether the scene uses them or not.**
- **The camera is unconstrained.** Interstellar to a few centimetres above the felt is one continuous
  range, so nothing in the scene may assume a scale band.
- **The physics is identical, only the scale differs.** A cue ball striking a rack is the same contact law
  as a moon striking a planet — docs/23's north star stated as a game of pool.

The acceptance test writes itself: **fly from Mars to a break shot without the representation changing
hands.** Where it changes hands is where the engine is still lying about being one engine.

## Where the engine already meets it

- **The body is shared.** `assets/bodies/earth.json` is THE Earth; scenes reference it rather than
  redefining it (Sean's upstream-6, "one Earth").
- **The scale ladder holds.** Measured 2026-08-01 (docs/59): 78 Gm → 0.10 m, 11.9 decades, p50 render
  0.4 ms at every rung, continuous from interplanetary distance to about a hundred metres.
- **Elevation streams by necessity** (`terra::tiles`), so a coordinate on Earth can now have its REAL
  ground fetched at metres per pixel instead of being invented.
- **Materials are one catalogue**, and as of this date one texture path: `surface_albedo_triplanar` and
  `surface_normal_triplanar` are shared by every surface shader.

## Where it does not — the honest list

1. **The Ground scene is sited on Earth but does not stand on it.** `worlds/ground/world.json` declares
   `lat 45, lon −100` and its own comment says the patch's *"gravity, air and material strata all derive
   from that one body"* — true — but then *"the surface block declares only the local relief dials"*, and
   those dials are an fbm invention. So it is a real place with imaginary ground: the one thing the
   streamed elevation can now fix outright. **Rebasing Ground on the real surface at its declared
   coordinate is the smallest change that makes it a harness rather than a diorama**, and it immediately
   buys what Robin asked for — move it to another lat/lon and you are in another biome, on real terrain,
   with the strata and air that place actually has.
2. **A scene is still a `#[wasm_bindgen]` struct inside the engine crate** (docs/46 row 14). Adding or
   removing one edits the engine. A pool table must be an assembly in a definition, not a fourth struct.
3. **The render path is still two.** Terra draws `globe.wgsl`; Ground draws `world.wgsl`; matter draws
   `matter.wgsl`. They now share the material chunk, which is the first real convergence, but not the
   pipeline.
4. **Below ~1 m the surface has no representation.** The ground cap is a 192-cell heightfield spanning a
   horizon — 15 m cells at 0.1 m altitude — so it cannot carry a felt surface, let alone a ball on it.
   Below about a metre the ground must stop being a HEIGHT and become MATTER (docs/39, docs/44). **This is
   the same gap the pool table sits in**, which is why the table is a good acceptance test: it is an object
   of exactly the scale the engine currently cannot represent the ground at.
5. **Epochs.** Scenes differ in TIME as well as scale — proto-Earth, modern Earth. Time is now commandable
   (`Terra::set_epoch`), but which Earth a scene means is still implicit in which body file it names.

## The order that follows from it

1. **Rebase Ground on the real surface at its declared lat/lon** (streamed elevation + the shared strata),
   and give it the ability to move. Harvest what Ground already does well first — it owns the voxel/granular
   matter path, `deposit_event`, cohesive bodies and the SPH cap — none of which Terra has.
2. **Sub-metre ground as matter**, which both the pool table and the JIT crater (docs/59 Stage C) need.
3. **One render path**, so "looks good" is one answer.
4. **Scenes as data** (docs/46 row 14), at which point a pool table is a definition and not a code change.

**Related:** docs/13 (scale-relative) · docs/23 (north star) · docs/39 (JIT particalization) · docs/44
(resolution by necessity) · docs/46 (one-physics charter, rows 14 and 27) · docs/51 (scenes as data) ·
docs/53 (the engine driven by a definition) · docs/58 (the generic body) · docs/59 (the scale ladder).
