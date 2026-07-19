# The accelerated particle-compute module (temporal-delta N-body / SPH)

> Robin's framing: the supercomputer Moon-forming runs compute *every* particle *every* step. We want to
> get close to their result at a tiny fraction of the cost by exploiting that **the flow changes slowly** —
> compute the *deltas* over time, not a full recompute from scratch each step, "the way an MPEG is much
> more efficient than a series of still images." And it should be a **reusable module** — the same solver
> for the giant impact, and later for clouds, smoke, water, and weather-like fields in our worlds.

## Honest calibration (so we build on solid ground)

These are **proven techniques**, not a new computational science — they are how the big offline codes
(ECMWF weather, cosmology, molecular dynamics) already afford 10⁶–10⁹ particles. Adopting them is the
*low-risk* path, and we have supercomputer results to validate against. The genuine novelty for us is
**real-time, in-browser, GPU/WASM, error-controlled** timeline physics that stays provably close to the
offline ground truth — an *interactive* envelope those batch codes don't target. Goal: "a reusable
real-time engine for timeline physics, honest against ground truth," **not** "out-forecast ECMWF."

## The MPEG mapping

| MPEG | Here |
|------|------|
| I-frame (keyframe) | rebuild a particle's neighbour list / tree node only when it has **drifted** past a skin margin (Verlet lists) |
| P/B-frame (delta) | **individual (block) timesteps** — update the violent shocked core often, let the quiescent disk coast on large steps |
| lossy (eye won't miss it) | **NOT allowed** — physics deltas must be *error-controlled*, or energy drifts and the disk mass (the answer) changes |

The one hard rule: the "recompute when drastic" trigger is a **physical error bound** (drift distance,
local dynamical time), never a visual one. Every stage is verified to conserve energy/momentum — identical
to brute force, or identical within a stated, tested bound.

## Staged plan (each stage: verified against the O(N²) brute force it replaces)

1. **Spatial acceleration** — the prerequisite; no temporal trick beats O(N²) without it.
   - **a. Neighbour grid** (uniform spatial hash) for short-range forces (contact, SPH). ✅ `neighbors.rs`
     (`grid_finds_exactly_the_brute_force_pairs`).
   - **b. Wire it** into the aggregate's contact / SPH-density / SPH-pressure / dissipation / PdV loops →
     O(N); verify forces are byte-identical to brute force.
   - **c. Barnes–Hut tree** for gravity (long-range) → O(N log N); θ-controlled, bounded error.
2. **Adaptive smoothing length** `h_i ∝ (m_i/ρ_i)^⅓` — resolves the expanding vapor (and lets it keep
   cooling by PdV as it spreads).
3. **Temporal deltas** (the MPEG core): **Verlet neighbour lists** (rebuild-on-drift) + **block individual
   timesteps** (the impact spans a huge range of dynamical times; this is the 10–100× win).

## Validation

At every stage: (a) the accelerated force must equal the brute-force force (exact for the grid, bounded
for the tree); (b) the giant-impact disk guardrails must hold; (c) the orbit-vs-resolution sweep must keep
climbing toward the ~1.5 M☾ ground-truth disk. The supercomputer literature (Canup, Ćuk & Stewart) is the
external check on the converged answer.

## Reusability

The module is generic over positions + a force law (`neighbors.rs` takes `&[DVec3]`, knows nothing about
`Aggregate`). The impact is the first client; clouds/smoke/fluids/weather-fields are later clients of the
same solver — the codebase's "one law, reused everywhere" (docs/23), applied to the *solver*.
