# Adaptive resolution & clumping — simulating matter within a compute budget

> Design note. The engine cannot move billions of particles, so it represents matter at the
> **coarsest resolution that still looks and behaves right** for the current context, and adapts
> that resolution to a fixed compute budget. Status: **design**. Underpins Phase 3 (MLS-MPM) and
> the tool/terrain interaction in [`09-agentic-object-authoring.md`](09-agentic-object-authoring.md).

## The problem

A cubic meter of dirt is ~10^9+ grains. Simulating each is impossible in a browser. But most matter,
most of the time, is **not moving** — it's static bulk. Only where something *interacts* (a shovel,
an impact, a collapse) does matter need to flow. The engine exploits this with a multi-resolution
representation and a hard particle budget (~100k–250k active "matter" particles; see
[`02-oss-building-blocks.md`](02-oss-building-blocks.md)).

## Representation tiers (coarse → fine)

| Tier | Form | Cost | Used for |
|---|---|---|---|
| **Bulk** | static voxels (density + material id per cell) | cheapest | undisturbed matter (the 200km of rock below) |
| **Surface** | meshed shell of the bulk (surface nets) | cheap | rendering the static bulk |
| **Clumps** | aggregate particles, each = many grains carrying their summed mass | medium | disturbed granular matter (displaced dirt) |
| **Grains** | fine particles (near the interaction) | expensive | the immediate contact zone, for correct look/feel |

**Clumping is the key idea:** a "clump" particle stands in for a blob of many real grains. It carries
the **aggregate mass** (Σ density × volume) so gravity, momentum, and pile behavior stay physically
correct, but there are orders of magnitude fewer of them. The clump size is chosen so the total
active particle count stays within budget.

## Refine / coarsen (LOD transitions)

- **Refine (promote):** when the shovel enters the dirt, nearby bulk voxels **activate** into clumps
  (or grains right at the blade) so they can flow. Only a bounded region around the interaction is
  promoted.
- **Coarsen (demote):** when displaced matter comes to rest, clumps **settle back into the voxel
  store** — re-solidifying into cheap static bulk and freeing the particle budget. (Dig a hole, toss
  the dirt aside; once it stops moving it's voxels again.)
- **Merge/split:** under budget pressure, clumps merge (coarser) to stay within the cap; near a
  fine interaction they split (finer). Both **conserve mass and momentum** so physics is unchanged
  by a resolution change — only fidelity varies.

## Budget-driven, graceful degradation

- A target active-particle cap is fixed per frame. A huge excavation doesn't drop particles (which
  would leak mass) — it **coarsens** (bigger clumps) so the same matter is fewer, larger aggregates.
- Resolution priority is driven by: proximity to active interaction, camera distance, and remaining
  budget. Far/idle matter is always coarsest.
- Everything is **tunable** so the same world runs on a phone (smaller cap, coarser clumps) and a
  desktop GPU (larger cap, finer grains).

## Invariants (must hold across all tiers)

1. **Mass conservation:** total mass is identical whether matter is voxels, clumps, or grains.
2. **Gravity correctness:** aggregate mass feeds self-gravity (`docs`/gravity) regardless of tier.
3. **No pop / no leak:** LOD transitions conserve momentum and don't teleport or vanish matter.

## Open questions

1. **Clump shape/collision** — spheres (cheap) vs. shaped aggregates (better packing/repose).
2. **Settling criterion** — velocity/energy threshold and dwell time before demoting clumps → voxels.
3. **Seams** — coupling the fine grains at a contact to the coarse clumps behind them without a
   visible/physical discontinuity.
4. **Rendering LOD** — splatting clumps vs. re-meshing settled voxels, and blending between them.
