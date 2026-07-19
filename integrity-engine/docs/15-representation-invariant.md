# The representation invariant — the cube is a lattice, not a unit of matter

> Design note. This is a **foundational, permanent invariant**, written down so it can't erode as the
> engine grows. It answers the question "aren't we baking a mistake into the core by building on
> cubes, when the universe is made of spheres?" — the short answer is *no, as long as this invariant
> holds*. Status: **canonical**. Sits under the matter model (`docs/04`), the adaptive
> representation (`docs/08`), and the robustness suite (`docs/10`).

## The invariant

**A voxel is a sampling cell, never a unit of matter.** The cubic grid is the *coordinate lattice* we
sample continuous fields on — density, material, momentum — exactly the way pixels sample a
continuous image or finite-element cells sample a continuous solid. It is **not** an ontology: the
world is not *made of* blocks, the way a block world (Minecraft) is.

Concretely, this means:

- **All physical state lives on matter, not on cells.** Mass, density, material, position, and
  velocity belong to particles/fields with *continuous* coordinates (`matter::Particle.pos: Vec3`,
  `gravity::MassPoint`). A voxel merely *records which material is sampled here* — the cheapest,
  dormant tier of the representation (`docs/08`: Bulk → Surface → Clumps → Grains).
- **The cube dissolves the moment physics touches it.** Under stress, bulk voxels *activate* into
  particles that move in real space; when they come to rest they *demote* back to voxels. The grid is
  storage for what isn't currently interesting, not a claim about how matter behaves.
- **Any feature that treats a voxel as an indivisible object is a bug**, not a shortcut. That is the
  line between this engine and a block engine.

## Why this makes "cubes vs. spheres" a non-issue

Roundness in nature is **emergent, not primitive**. Real solids sit on lattices — crystals are
literally periodic, and many common ones (rock salt, BCC iron) are *cubic* — yet planets are round,
because isotropic self-gravity averages over the microscopic lattice and pulls matter into
hydrostatic equilibrium. The lattice at small scale does not dictate the shape at large scale.

The engine mirrors this: a planet is voxel-sampled matter whose **aggregate mass produces a
spherically-symmetric far field** (`gravity.rs`, validated by `orbit.rs`) and whose **surface meshes
smooth** (surface nets, `docs/12`). Cubes at the sampling scale; spheres at the emergent scale — the
same trick nature uses.

## The "feels right" corollary (VR / physical honesty)

A north-star goal is a world that **feels right** — in VR especially — because it *is* right, not
because of per-object fakery. Behaviour is a **natural property of the world and the object**, read
off physical data, never scripted case-by-case:

- Leave something unsupported and it **falls** — because `world::find_unsupported` (connectivity to
  the anchored base) hands it to `matter::collapse`, which drops it under the real gravity field.
  Nobody tagged it "falls"; it falls because nothing holds it up.
- A tool breaks matter only when its stress exceeds that material's `fracture_strength` — granite
  shrugs off what shreds soil — with *no per-material special-casing*, just the numbers in
  `data/materials.json`.

This only stays honest if the representation stays honest. The moment a cube becomes a "thing" with
scripted behaviour, the fakery is back.

## How the invariant is enforced

Physical honesty is *verified, not assumed* (per the project's canonical TDD principle):

- **Grid-isotropy regression suite** — `crates/engine/src/isotropy.rs`. A regular lattice has
  preferred directions (its axes and 45° diagonals); the risk is that a solver or geometric op
  silently bakes that bias into the *physics*. The suite asserts direction-independence and is proven
  non-vacuous (each guard was shown to go red under a deliberately anisotropic mutant):
  - *Gravity* on a symmetric ball is radial and equal-magnitude in every direction (face axes, edge
    and corner diagonals) — no pull toward a face over a corner.
  - *Digging* carves a true Euclidean sphere (volume within a few %, equal reach on every axis, no
    lateral ejection bias) — not a grid-aligned box or octahedron.
- **The `docs/10` adversarial suite** guards the dynamics (no tunnelling, stacking stable, drop-and-
  rest, idle soak); **`docs/08`** requires LOD transitions to conserve mass and momentum so a
  resolution change never injects or leaks energy.

## Escape hatch (why this is not a trap)

Because state is decoupled from the acceleration structure, the *lattice itself* is swappable later
without touching this invariant: a sparse/adaptive octree (OpenVDB-style) for multiresolution, or
unstructured tetrahedra for elasticity/fracture, or the grid-artifact-reducing APIC/MLS-MPM transfers
already targeted in `matter.rs`. What must never change is the invariant: **matter is matter; the
grid is only how we currently index it.**
