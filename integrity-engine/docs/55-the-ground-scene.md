# docs/55 — the ground scene, rebuilt from a definition

Robin: *"terrain needs a complete rebuild with the new physics engine"*, and *"in order to get users for
our game engine, we're gonna need to prove it works."* This is the rebuild, and the first thing since the
deletion that a person can look at.

## What it is

`/ground.html` → `Ground` (`crates/engine/src/ground_scene.rs`) → `/worlds/ground/world.json`.

**Every number about the world is in the file**: patch size, relief octaves, sea level, the material
column (sand → gravel → dirt → basalt → granite), camera altitude, gravity, grain size. The scene
contributes a camera rig, a meteor button, and three render passes. Nothing about *this* world is
compiled in — which is the difference from the terrain scene that was deleted.

It also gives the granular pipeline a visible home again: since terrain was removed, `gpu_particles` has
been reachable only from `GpuProbe`, a compute-only diagnostic with no canvas. (See "not done" — this
scene currently steps grains on the CPU, so that consumer is still owed.)

## Three things it gets right, and each was earned

**The texture is the material.** `texture::generate` synthesizes 512² mip-mapped textures from each
material's CITED optical properties (albedo, colour variance, metallic) — no image assets, nothing
licensed, nothing hand-painted. The sand you see is the same database row the physics reads.

**The sky is derived, not painted.** `atmosphere::rayleigh_tau` from the emergent surface pressure of
`planet::earth()` — the same λ⁻⁴ scattering that gives the blue marble its veil. The first cut passed a
guessed `tau` and `SUN_GAIN = 1.0` and rendered a **black sky**; the working values came from the
retired scene, and the rig caught it in one shot.

**The camera is MATTER.** A transparent shell on the SAME `granular::terrain_contact_resolve` every grain
obeys — contact and slide, never excavation. The first cut used `eye.y = eye.y.max(ground + h)`, which is
precisely the clamp fudge that principle retired: it exempts the camera from the world's rules and only
ever pushes straight UP, so a camera driven into a steep face pops through it. The shell's half-extent
(0.35 m) is ≥ the near-clip (0.2 m), which is what actually stops the frustum crossing the surface, and
the sweep from last frame's eye stops a fast camera tunnelling the thin skin. **The rig proposes, physics
disposes**: the rig asks for the declared altitude above the ground it is watching, and the shell corrects
whatever that would put inside a dune.

## ⚠️ Not done: the crater does not persist

Drop a meteor and you get a real crater with thousands of grains — and a few seconds later **the ground is
exactly as it was.** Measured headlessly:

```
after load : 20373 particles, 643269 solid voxels     <- excavated
after 400  : 28 particles
matter     : 20373 created | 20345 returned | 28 in flight | 0 LOST (0.0%)
voxels     : 643269 -> 663614                          <- pristine was 663642
```

Matter is **perfectly conserved** — and that is exactly why the crater fills in. The ejecta falls straight
back into the hole it came from.

**Root cause, and it is already recorded in docs/32 §4:** `MatterSim::step` is the CPU *settle-only*
stepper — *"no grain-grain contact on CPU"*. Grains cannot push each other outward, so there is no ejecta
blanket: they fall, stack, and refill the excavation exactly. The GPU granular path
(`particle_step.wgsl` + `gpu_particles`) DOES have grain-grain contact, and it is the path that produced
the measured local ejecta blanket (JOURNAL 2026-07-19, spread 5–7 km → 82 m).

**So the next increment is one thing, and it pays off twice:** step this scene's grains through the GPU
granular container instead of the CPU stepper. That gives a real ejecta blanket and therefore a crater
that persists (Robin's "dissolve when at rest becoming part of a bump map"), and it gives `gpu_particles`
the visible consumer it has lacked since terrain was deleted.

Also open: the crater reads as voxel terraces rather than a bowl (surface-nets on a 1 m lattice at a 96 m
patch), and the meteor currently appears at the surface rather than flying in.

## Honest scope

Still a `#[wasm_bindgen]` struct inside the engine crate, so adding a scene KIND remains an engine edit
(docs/46 ledger row 14's remaining half). What this proves is the other half — a scene's CONTENT is data.
