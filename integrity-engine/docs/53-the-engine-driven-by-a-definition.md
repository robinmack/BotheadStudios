# docs/53 — the engine driven by a definition

Closes docs/46 ledger row 15. Continues docs/51 (scenes as data) and docs/52 (the standalone engine).

## The failure this repairs

Deleting the terrain scene (docs/50) left three built-and-verified systems with **zero** production
consumers — `matter::MatterSim` (the shared matter path), `resolution::ResolutionField` (docs/49's
camera-driven resolution) and the voxel `world::World` — **while every test kept passing**. That is
docs/48's wiring pattern at its worst: physics wired into one place, and then that place deleted.

Robin's diagnosis was structural, not incidental:

> *"And this is why we make the engine standalone, with external definitions."*

Capability was reachable only THROUGH a scene, so a scene's deletion took it down with it. The repair is
not "add another scene" — that reintroduces the same coupling. It is to make the consumer a **file**.

## The shape

`crate::simulation::Simulation` — no scene struct, no canvas, no `wasm_bindgen`:

```
Simulation::from_json(world_json, materials)  →  builds World + MatterSim + ResolutionField
            .step(dt)                          →  docs/49 hand-off, then the shared matter step
```

A `"ground"` world (`world_def::GroundDef`) declares the observer, the gravity analytic effects fall
under, and a list of events:

- `impact` — excavates through the shared `MatterSim::impact`, the same primitive terrain used.
- `ejecta` — carried matter in flight, registered as an analytic `Effect` that propagates by cheap math
  off-camera and materialises the instant it enters view.

`crates/engine/src/bin/run-definition.rs` runs one headlessly. **This is not a renderer** — anything
wanting pixels supplies its own host (the browser today, a native window later, docs/52). Staying
headless is what makes it natively testable, which the scene structs never were.

## Verified — and the near-miss that made it honest

Running `definitions/ejecta-ground.json` for 300 steps:

```
after load : 3 particles, 1 analytic effect(s), 644190 solid voxels
step  130  : 1 effect(s) entered view and materialised -> 257 particles
after 300  : 0 particles, 0 still analytic, 1 resolved in total
matter     : 644190 -> 644450 solid voxels (+260)
```

**+260 is exactly the 257 materialised grains plus the 3 impact particles.** Every grain de-resolved back
into the world; none was lost. The runner reports the voxel delta precisely because "0 particles" is
ambiguous — de-resolution (matter conserved) and the off-world cull in `matter::step` (matter deleted)
look identical from the particle count alone, and only one of them is honest.

**The near-miss.** The first version of this printed `materialised -> 0 particles` and the test suite was
green, because the tests asserted that an effect **resolved** — a state change — and never that it
**produced matter**. The cause was the definition's own geometry: `view_radius_m: 150` exceeds the 96 m
patch bound, so ~250 grains spawned outside the world and `matter::step` culled them in the same step.
Not an engine bug, but the test could not tell the difference, which is precisely the hollow-green
failure this module exists to prevent. The assertion now exists and **is proven able to fail** — moving
the resolve point back outside the patch produces `must PRODUCE MATTER; got 0 particles`.

## Status of ledger row 15

**Closed.** `simulation.rs` is production code (`pub mod simulation`, one `#[cfg(test)]` block) and
references `MatterSim` 8×, `ResolutionField` 4×, and `world::generate` 1×. The consumer is a definition
the engine loads, so no scene's deletion can orphan them again.

## What is still not standalone

1. The two remaining scenes are still `#[wasm_bindgen]` structs in the crate — a new KIND of scene is
   still an engine edit (ledger row 14's remaining half).
2. No native host: no window, surface or input (docs/52).
3. The ground world's **surface** is still the procedural patch; the definition declares events, camera
   and gravity, not the terrain itself. Making the surface data is the next honest step.
