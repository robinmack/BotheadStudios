# Scale-relative simulation — observer-relative fidelity

> The engine's north-star architecture. Both **simulation** and **rendering** cost should scale with
> what is *observable from the current viewpoint*, not with the size or contents of the universe. You
> drill in and detail appears; you pull back and it collapses into a summary. You never need the exact
> state of everything — only what is perceptible at the scale you're perceiving from. Status: **vision
> / organizing principle** (incremental to build). Unifies `docs/08` (clumping/LOD), `docs/09`
> (interaction), `docs/11` (networking interest-management), and `orbit.rs` (coarse-scale physics).

## The idea

Encoding real physical laws gives realism; making the *cost* observer-relative makes it tractable at
any scale. The same event is simulated and shown at whatever fidelity the observer can actually
perceive:

- **A person drops a rock.** Near field, full detail: it falls, hits, wobbles, settles.
- **An ant beside it** is *inside* the fine-detail region: a boulder plummets and shakes its world.
- **From 50 feet** the event is a few pixels: rendered coarsely; the wobble isn't individually
  simulated — just "rock, now at rest, here."
- **From orbit** it's sub-pixel: not simulated in detail at all — at most the rock's new position is
  folded into the terrain summary, or ignored.
- **Falling from orbit**, detail *emerges continuously*: star field → planet disk → landscape →
  terrain → the rock → its grains, while the now-irrelevant scales (orbits, star positions) drop out
  of the compute budget as they leave perception.

Fidelity ∝ observability. Unobserved fine detail is never computed; it is either **summarized** or
**generated on demand** when an observer zooms in.

## Representation: a scale hierarchy

The world is nested scale bands (a sparse octree / nested grids), each band a factor of ~2–10 coarser
than the one inside it. Every region exists at whatever band is currently relevant:

- **Fine bands** (near the observer): full state — voxels, MPM particles, exact contacts.
- **Coarse bands** (far): **aggregate summaries** — a rock becomes `{position, velocity, mass,
  bounding size, at-rest?}`; a hill becomes a heightfield patch; a planet becomes a point mass with
  an orbit. "Note the new position of the rock" *is* the coarse representation.

## Simulation LOD (the key move beyond rendering LOD)

Rendering LOD (mips, mesh decimation) is old news; the novelty is **simulating** at the observer's
scale:

- **Active fine simulation only in the focus region** (MPM/granular/contacts). This is `docs/08`'s
  voxel↔particle clumping generalized across *every* scale.
- **Coarse regions advance cheaply**: rigid aggregates, heightfields, or closed-form motion. A far
  rock at rest costs nothing; a far planet is one Verlet step of `orbit.rs`, not a voxel sim.
- **Time LOD too**: distant/slow things take larger timesteps or analytic updates (orbits), while the
  focus region takes small substeps. Compute follows attention in time as well as space.

## Refine ↔ coarsen (promotion/demotion)

As the observer approaches a region, **promote** its summary into full simulation (regenerate the
grains); as they leave, **demote** it (fold fine state into a summary, discard the rest). The
`docs/08` invariants generalize: transitions **conserve mass, momentum, and energy**, and must not
"pop." Detail that was summarized and then re-approached is **regenerated deterministically** (stored
where it diverged from procedural default, re-derived otherwise) so the ant sees the *same* grains
each time — determinism (which we already prize) is what makes "zoom out then back in" consistent.

## Coordinates across 20 orders of magnitude

Millimetre grains to astronomical-unit orbits exceed f32 (and f64) precision in a single frame. Scale-
relative worlds need **nested reference frames / floating origin**: the active region is simulated in
local high-precision coordinates near the origin; parent frames track the region's place in the
larger world at lower precision. "Fall from orbit" hands off between frames as you descend. (`orbit.rs`
already uses f64 for the celestial band; the fine bands use local f32.)

## What's observable = the budget

"Observable" is a screen-space + relevance test: a region is simulated/rendered at the band where its
projected size, its energy, or its interaction with the observer crosses a threshold. Below that, it
drops to a summary or disappears from the budget. This is the same **interest management** as the
networking model (`docs/11`) — an observer subscribes to its perceivable set — applied to local
compute, and it's how a single machine (or client) stays within budget no matter how big the universe.

## How it builds on what exists

- `orbit.rs` — the **coarse celestial band** (validated: the Moon orbits).
- `matter.rs` / voxel store / MPM plan — the **fine matter band**.
- `docs/08` clumping — the **first refine/coarsen transition** (voxel↔particle); generalize it up the
  hierarchy.
- `texture.rs` mips, surface-nets — **rendering LOD** already in place; extend to geometry + sim LOD.
- `docs/11` interest management — the **observability budget**, reused for local compute.

The engine becomes: *one physical law set, expressed at every scale, with fidelity that follows the
observer.*

## Honest challenges (research-grade; incremental)

1. **Seamless transitions** — no pops; conservation across refine/coarsen; blending bands visually.
2. **Deterministic regeneration** — re-deriving discarded detail identically on re-approach.
3. **Precision / floating origin** — nested frames, hand-off on descent.
4. **Observability metric** — choosing thresholds (screen size, energy, salience) that feel right.
5. **What to persist vs regenerate** — store only the divergence from the procedural baseline.

## A concrete first milestone

**"Orbit-to-ground zoom":** start at an orbital view (planet sphere + the Moon orbiting via `orbit.rs`);
zoom/fall in; at a distance threshold the planet surface **refines** into the voxel terrain we already
have (Phases 1–7); keep zooming and dig/fracture works as now. Pull back out and the terrain collapses
to the planet summary again. That single demo embodies the whole principle end-to-end, and can be
built in scoped steps (start with two bands: "planet summary" ↔ "local voxel patch").
