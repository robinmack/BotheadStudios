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

**Root cause — and it is TWO mechanisms, not one (corrected 2026-07-23; the original text named only the
first):**

1. `MatterSim::step` is the CPU *settle-only* stepper — *"no grain-grain contact on CPU"*. Grains cannot
   push each other outward, so there is no ejecta blanket: they fall and stack.
2. Even with a blanket, `deposit_resting_grain` deposits every settled grain into its column's **air-start
   voxel — which in an excavated column is the crater floor** (`matter.rs`, "stacks / refills the crater").
   So any grain that comes to rest over the hole re-solidifies it from the bottom, independently of (1).

Both must be addressed; fixing only the blanket leaves (2).

**On the forward plan — SUPERSEDED (2026-07-23).** This doc originally proposed stepping the grains through
the GPU *granular* container (`particle_step.wgsl` + `gpu_particles`). The engine instead took the SPH
**cap-on-bulk** route (docs/39): a terrestrial meteor is a moon-drop scaled down, so the ground gets the
same `promote_ground_cap` + `set_bulk_planar` + SPH machinery the space impact uses — one primitive at both
scales (docs/46), which the separate granular path would have forked. Those two foundations
(`promote_ground_cap`, planar bulk mode) are built and tested but NOT yet wired into this scene (it still has
no `GpuSph`), so the crater still does not persist as of this writing — the plan changed, the gap did not.

Also open: the crater reads as voxel terraces rather than a bowl (surface-nets on a 1 m lattice at a 96 m
patch), and the meteor currently appears at the surface rather than flying in.

## Honest scope

Still a `#[wasm_bindgen]` struct inside the engine crate, so adding a scene KIND remains an engine edit
(docs/46 ledger row 14's remaining half). What this proves is the other half — a scene's CONTENT is data.
