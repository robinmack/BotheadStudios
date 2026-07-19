# Validation demonstrations — the physics, provable and showable

> We encode real physical laws, so we can *prove* them, not just assert them. Every core claim has a
> deterministic test that fails loudly if the law breaks — and each test is also the seed of a **visible
> demonstration** for the full build (a showcase, a tutorial level, a regression guard). This file is
> the catalogue: keep it as the canonical list of "things the engine is known to get right," and grow
> it as the engine grows. Tests live in the crate; this maps each to *what it proves* and *how it
> becomes something you can watch*. TDD is canonical ([[integrity-engine-tdd]], `docs` throughout).

Why keep this separate from the code: the tests answer "is it correct?" — this doc answers "what have
we demonstrated, and how do we *show* it?" The two audiences (contributors trusting the engine; players/
press seeing it) need the same facts framed differently. Store the concept once here; render it many ways.

## The demonstrations

| # | Demonstration | Proves | Today (test) | Showable form in the full build |
|---|---|---|---|---|
| D1 | **The Moon orbits the Earth** | Newtonian gravity + symplectic integration reproduce real celestial motion | `orbit::moon_orbits_earth` — real masses/distance/velocity → bound orbit, ≥1 revolution, energy & L conserved <1% | The **space band** (`/orbit.html`, Step A): watch the real Earth+Moon orbit. Later: full solar systems. |
| D2 | **Free-fall matches kinematics** | `F = ma` integration is correct (position ≈ ½·g·t² under constant g) | `body::free_fall_matches_kinematics` | Drop-tower demo: a mass falls, measured vs predicted overlaid live. |
| D3 | **A body rests on terrain without penetrating** | Sphere-vs-voxel contact resolves; solid objects act solid | `body::rests_on_voxel_floor_without_penetrating` | The probe settling on the surface (terrain slice). |
| D4 | **A body cannot clip through a wall** | Collision pushes out of *any* overlapped solid, not just the column below | `body::does_not_clip_into_a_wall` | Roll a ball into a dug crater wall — it stops, doesn't tunnel. |
| D5 | **Self-gravity from aggregate mass** | Gravity emerges from summed voxel mass (`g = G·M/r²`), not a hard-coded constant | `sphere_falls_toward_world_and_rests` (+ `total_mass`/`surface_gravity` HUD) | Two rubble piles drift together; g rises as you pile on mass. |
| D6 | **The world is layered by density** | Material behavior/placement flows from physical density, one source of truth | `world_is_layered_rock_dirt_grass`, `material_database_loads` | Dig down through grass → dirt → rock; strata are visibly real. |
| D7 | **The surface mesh is watertight** | Rendering is closed/solid "all the way down" — no hollow shells | `surface_nets_mesh_is_closed` (0 boundary edges), `surface_nets_is_smooth_and_valid` | Slice the planet open — it's solid inside, not a facade. |
| D8 | **Matter is conserved through dig/fracture/collapse** | Destruction moves mass, never creates or destroys it | matter-sim tests (fracture by strength; settle) | Blast a chunk: debris volume ≈ hole volume; nothing vanishes. |
| D9 | **Same impulse, different materials → different damage** | Emergence: behavior from density/strength params, not per-material code | dig/fracture on granite vs dirt vs grass | Side-by-side impact panels; identical hit, honest differences. |
| D10 | **A downward ray hits the terrain surface** | Picking/tool interaction is physically located | `raycast_hits_terrain_from_above` | The dig/blast tool lands where you point. |

## Principles for turning a test into a demonstration

- **Determinism first.** A demonstration you can't reproduce isn't a proof. Every demo runs from a
  fixed seed / fixed inputs so it looks the same each time (and doubles as a regression guard).
- **Show the number, not just the picture.** D1 conserves energy to <1%; a good demo *displays* the
  conserved quantity (separation km, energy drift) so a skeptic can watch it hold.
- **One law, many scales.** The *same* gravity drives D1 (celestial) and D5 (a falling probe). Demos
  should make that unity visible — it's the whole thesis (`docs/13`).
- **Fail loud.** If a law regresses, the test goes red and the demo looks wrong in the same way. Keep
  the test and the demo describing the *same* fact.

## Backlog (laws to demonstrate as the engine grows)

- Orbital **period** matches Kepler's third law (T² ∝ a³) across several distances.
- **Three-body** motion (Earth–Moon–Sun) — chaos, but bounded and conservative.
- **Angular momentum** of a spun aggregate is conserved through fracture.
- **Energy budget** of an impact: kinetic in ≈ fracture work + heat + debris KE out.
- **LOD transition conservation** (`docs/13`): refine↔coarsen preserves mass/momentum/energy — the
  demonstration that makes scale-relative simulation trustworthy.
