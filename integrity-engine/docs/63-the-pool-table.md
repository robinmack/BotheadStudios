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

> *"BECAUSE it should ALL BE THE SAME. The Earth should be the Earth, the Moon the Moon, no matter how
> close or how far the camera pans."*

**That sentence is the whole doc, and it is a law, not a preference.** Today the camera does not choose how
finely Earth is drawn — it chooses WHICH EARTH YOU GET:

| | Terra | Ground |
|---|---|---|
| surface | cube-sphere globe + 192² tangent cap | voxel `World` + mesher |
| frame | planetary, f64 → display units | local Cartesian f32, y = up |
| shader | `globe.wgsl` | `world.wgsl` |
| altitude | continuous, interplanetary → 0.1 m | **none — `set_orbit(yaw, pitch, _zoom)` ignores zoom** |

Two surfaces, two frames, two shaders, and one of them cannot leave the ground. That is Law IV inverted:
the camera is deciding identity rather than representation. An explanation that ends "Ground looks good
close up, Terra looks good far away" is not a defence of the design, it is the statement of the bug.

**The mechanical form of "the same" is not one mesh — it is one SURFACE, resolved by necessity.** Far away
Earth is cheap math over its own raster; near the eye the same ground is resolved into real matter; the
transition is resolution, not identity, and nothing about which is in play may depend on which scene struct
you happened to open. The engine already has the primitive that does exactly this and it has **zero
consumers**: `gpu_sph::promote_ground_cap` turns a patch of surface into matter, built and tested and wired
to nothing (docs/48's pattern again — physics built, then wired into one place or none).

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

## The mechanism (Robin, 2026-08-01) — and it is NOT a bridge between representations

An earlier version of this section proposed rebasing Ground's voxel patch on real elevation and calling
that the fix. **That was a category error and Robin caught it**: it solves an APPEARANCE problem with a
representation swap, which is the very thing that made two Earths, and swapping representations by zoom
level is the technique we are trying to beat rather than reproduce.

> *"If we know the textures we will use at ground level, and we efficiently scale them properly (accounting
> for geography/biome/etc), and we know the scale, don't we get everything for free if we apply the textures
> at different scales to the same geometries? … We're building something new here."*
>
> *"That way we just map a texture onto a segment of a sphere, whether it's a small chord or a full globe."*
>
> *"Mathematically it is matter, but we only materialize that matter **visually** when we need to, and only
> the amount we need to."*

That last sentence is the governing one, and it is Law IV said exactly: **the ground is matter at every
point, always.** Nothing about a camera makes matter exist or stop existing. What a camera decides is how
much of that matter we MATERIALIZE — and materialization has two different customers, the pixel and the
interaction, which are the same law (docs/44) asked by different necessities.

So the texture is not a stand-in FOR matter. It is a **statistical description of the matter that is
there**, integrated over a footprint. That is what makes it honest rather than a fudge, and it is the same
argument `surface_normal.wgsl` already makes for material grain — *"evaluating light's response to a known
sub-resolution surface statistic is what a microfacet model is"* — generalised from one scale to all of
them.

### One geometry

**A segment of a sphere**, its extent and tessellation set by the camera; the full globe is the limiting
case where the angular radius reaches π. `fill_ground_cap` already builds exactly this, so the work is
letting one primitive's angular radius go all the way up and deleting the globe mesh — along with every
piece of machinery that exists ONLY to mediate between two meshes: `cap_fade`, `cap_lift_disp`,
`cap_covers_view`, the `draw_globe` decision, and the whole class of bug where the two disagree about where
the ground is. None of that is physics; it is bookkeeping between two answers to one question.

**Where the mesh stops and the texture starts is derived, not chosen.** The first cut of this rule said the
mesh must carry whatever SILHOUETTES, since a normal map can shade a ridge but cannot put an edge against
the sky. Robin's amendment relaxes it, and in the direction of MORE physics rather than less:

> *"Closer to the horizon the vertical matters more (we need silhouette), but that can be an effect of
> lighting if we know the height map. Modern cards can at least do primitive ray tracing from a point
> source like the sun."*

If the height is known as a function — and it is, measured to the tile and generated below it — then
marching it answers both halves directly:

- **Toward the sun: whether light reaches this point.** Today the shader computes `ndl = max(dot(n, l), 0)`
  with NO occlusion whatsoever, so terrain never shadows itself and a mountain's west face is lit at dawn.
  A march is not a shadow effect bolted on; it is evaluating the thing the Lambert term currently assumes
  away, and it is strictly more honest than what is there.
- **Along the view ray: whether nearer ground hides farther ground.** This is what makes a coarse mesh read
  as real terrain at grazing angles, and it is most effective exactly where it is most needed — a
  near-tangent ray near the horizon traverses many samples.

So the mesh only has to carry the LARGE-SCALE PROFILE, not everything that silhouettes; fine relief
occluding fine relief comes from the march. Tessellation gets cheaper, not more expensive. What remains
irreducibly geometric is the outermost edge against the sky at the segment's own scale — the planet's limb
and the km-scale skyline — which the displaced segment already carries.

### One appearance function

The material response integrated over the pixel's footprint, and it needs TWO moments, not one:

- **Mean albedo** — below the material tile this is the texture's own mip chain (already there); above it,
  it is the MIXTURE of materials over the footprint, which does not exist today.
- **Variance of the normal** — sub-pixel geometry does not vanish when it stops being resolved, it becomes
  ROUGHNESS. Averaging normals alone loses it and the surface goes glassy, which is part of why our ground
  reads as painted plastic from altitude.

Today, above the 8 m material tile there is NO variation source at all except a flat per-vertex biome colour
sampled from a 19.5 km raster. That is precisely why 94 m altitude renders as a wash: nothing integrates,
so there is nothing between "grain too small to see" and "one colour".

### The invariant that keeps it honest

**Resolve a patch to matter, integrate its appearance over the footprint that was being drawn, and the
result must equal the texture that was already being drawn there.** If they differ, one of them is lying.
That is what makes the unresolved form a declared model with a named resolved counterpart (Law V) rather
than a picture that merely looks plausible — and it is checkable, in the same shape as
`generated_relief_is_stable_by_the_engines_own_slope_law`, which catches generated relief disagreeing with
the slope law it claims to obey.

## The order that follows from it

1. **Collapse globe and cap into ONE sphere segment** whose extent follows the camera, and delete the
   machinery that mediated between them. This is what makes "the Earth is the Earth" true in the code.
   ★ The blocker is the parameterization, not the idea: `fill_ground_cap` is gnomonic
   (`center + east·du + north·dv`, normalized), which reaches 90° only as `du → ∞` — hence today's
   `CAP_MAX_ANGLE = 0.6`. Covering a hemisphere needs a parameterization that degrades gracefully to π.
1b. **The height march** — toward the sun for self-shadowing, along the view ray for occlusion — which is
   what lets the segment stay coarsely tessellated.
2. **The appearance integral** — material mixture over the footprint, and normal variance carried as
   roughness — with the convergence invariant as its test.
3. **Matter on demand**, for the pixel or the interaction, never triggered by camera altitude. Ground
   already owns this machinery (voxel/granular matter, `deposit_event`, cohesive bodies, the SPH cap) and
   it should be attached to the real surface rather than to an invented patch.
4. **Scenes as data** (docs/46 row 14), at which point a pool table is a definition and not a code change.

### What this retires

The tier ladder. Measured 2026-08-01, one tier against four differed by **max 4 pixel values at 100 m
altitude**, and this section is the explanation that measurement lacked: **geometry was never the right
carrier for sub-cell detail.** Adding vertices to express what belongs in a texture's second moment cannot
work, so the ladder is not under-tuned, it is the wrong mechanism. (The anchoring work that made tiers cost
0.4 ms instead of 642 ms was a real fix to a real per-frame cost, attached to a feature that should not need
to exist.)

**Related:** docs/13 (scale-relative) · docs/23 (north star) · docs/39 (JIT particalization) · docs/44
(resolution by necessity) · docs/46 (one-physics charter, rows 14 and 27) · docs/51 (scenes as data) ·
docs/53 (the engine driven by a definition) · docs/58 (the generic body) · docs/59 (the scale ladder).
