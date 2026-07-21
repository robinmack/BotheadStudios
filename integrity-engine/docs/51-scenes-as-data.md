# docs/51 — scenes as data (the last code-path scene, and what terrain took with it)

Continues docs/43 (worlds-as-data) and docs/50. Robin's requirement, restated:

> scenes should have object definitions, assembly definitions, coordinates, etc… but should **not**
> require special mods of the engine itself.

## What was actually true, measured before designing

docs/46 ledger row 14 first claimed "a scene is engine code, not data". Measuring the pages narrowed it:

| page | scene struct | initial conditions |
|---|---|---|
| `orbit.html` | `OrbitDemo` | `data-world=/worlds/one-moon/world.json` — **data** |
| `twomoons.html` | `OrbitDemo` (same script!) | `/worlds/two-moons/world.json` — **data** |
| `terra.html` | `Terra` | `/worlds/earth/world.json` — **data** |
| `birth.html` | `OrbitDemo` | `data-scene="birth"` → **Rust constants** |

So scene *instances* were already data; two pages are literally the same engine driven by different
files. **One scene was still compiled in**, and this increment moves it.

## The giant impact as declared initial conditions

`world_def::ImpactDef` + an `"impact"` world type. What moved out of `gpu_sph` constants and into
`/worlds/birth/world.json`: both bodies' core/surface radii, softening and core-resolution factor; the
approach speed as a multiple of mutual escape speed (1.15); the start separation (1.6 × contact); the
impact parameter (b = 1.0 × R_target); the proto-target spin (4e-4 rad/s); and the relax separation (40 ×).

**The laws did NOT move.** Tillotson EOS, SPH, self-gravity, the leapfrog stay in the engine and are not
selectable from a file. What moved is initial conditions and a few dials — precisely docs/43's line.

**Output-neutral by construction:** every field's serde default IS the constant it replaced, and
`impact_defaults_reproduce_the_hardcoded_constants` asserts each one against the literal value as it
stood. A world that omits `impact` — or fails to fetch — is bit-identical to the old path.
`changing_the_declared_radius_changes_the_body_the_engine_builds` asserts the opposite direction, so the
file cannot be decoration.

**A real bug the rig caught:** `orbit.ts` handed ANY `data-world` to `load_world`, which requires a
`bodies[]` array. An `"impact"` world has none, so birth died with *"system world is missing a `bodies`
array"* and rendered nothing — while the world file was fetched successfully and no JS error was raised.
Routing is now by world TYPE. A screenshot check that only asked "did the file load?" would have passed.

## ⚠️ What deleting terrain took with it

Terrain was the ONLY production consumer of three built-and-verified systems. After its removal
(measured, not assumed — `grep` over `lib.rs`):

| system | production callers now | tests |
|---|---|---|
| `matter::MatterSim` (the shared matter path: `materialize_region`, `spawn_region`, `deposit_resting_grain`) | **0** — zero references in `lib.rs` | pass |
| `resolution::ResolutionField` (docs/49 — camera-driven resolution, wired 2026-07-20) | **0** | pass |
| `world::World` + `mesher` (the voxel world) | **0** — all 6 `world::generate` calls are in `#[cfg(test)]` | pass |
| `gpu_particles` (the granular GPU pipeline) | 1 — `GpuProbe`, a compute-only diagnostic with no canvas | pass |

This is docs/48's wiring pattern at its sharpest: **verified physics wired into one place, and then that
one place was deleted.** Every test still passes, which is exactly why it is easy to miss.

**This is not an argument for bringing terrain back** — Robin retired it deliberately, and it was the
first scene, with the accumulated craziness to match. It is a REQUIREMENT ON THE NEXT SCENE: the
replacement must re-consume `MatterSim`, `ResolutionField` and the granular pipeline, or those systems
should be deleted rather than left to rot as green-but-unreachable code. A law with no consumer is not
an asset; it is a claim nobody checks.

## The conclusion this proves (Robin, 2026-07-21)

> **"And this is why we make the engine standalone, with external definitions."**

The orphaning above is the argument, in measured form. Because capability was reachable only *through a
scene*, deleting one scene silently unwired three verified systems and every test stayed green. That
failure mode is not possible in the shape Robin is describing:

- **The engine is standalone** — it owns the laws and the capabilities (matter, resolution, granular
  contact, SPH, EOS) and exposes them as an API, not as wiring hidden inside a `#[wasm_bindgen]` scene
  struct that happens to call them.
- **Definitions are external** — a world/scene file names materials, objects, assemblies, coordinates,
  camera and initial conditions. Deleting a scene is deleting a FILE. Nothing in the engine changes, so
  nothing in the engine can be orphaned by it.
- **The consequence for testing:** capability is exercised by definitions the engine loads, so "does
  anything actually use this?" becomes answerable — you point at a definition — rather than a grep over
  scene code that passes right up until the day someone deletes the last caller.

It also connects to the native question ([[integrity-engine-native-platforms-ok]]): a standalone engine
consuming external definitions is a *program*, and `web/` is one host for it rather than its home. The
three module lifts (docs/50) already moved the GPU code out of the wasm-only scene module; this is the
same direction one level up.

**The honest status:** this increment moved the last scene's INITIAL CONDITIONS into data. It did not
make the engine standalone. `OrbitDemo` and `Terra` are still `#[wasm_bindgen]` structs inside the engine
crate, holding their own pipelines and render loops — so a new KIND of scene is still an engine edit.
That is the remaining half of ledger row 14, and the direction above is what it should be built toward.

## Next

1. A **ground/matter world type** whose scene is data and which drives `MatterSim` + `ResolutionField` +
   the granular pipeline — closing the orphaning above and giving docs/49 a visible demonstration.
2. Scene KIND is still code: a genuinely new kind means a new `#[wasm_bindgen]` struct with its own
   pipelines. That is the remaining half of ledger row 14.
