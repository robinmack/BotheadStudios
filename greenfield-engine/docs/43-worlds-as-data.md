# docs/43 — TODO / direction: scenes as external "worlds" the engine renders

**Robin's call (2026-07-18), parking the Theia scene:** the engine is mature enough that scenes should not be
bespoke TypeScript/Rust. A scene is just **initial conditions + a few dials**; the engine already owns the
*laws* (one contact law, one gravity law, SPH-EOS, the field→particalize→bake-back render layer, docs/42). So:
**define a "world" externally — in Python or Go — as DATA, and hand it to the engine to simulate and render.**
Scenes (one-moon, two-moon, deorbit, birth-of-the-Moon, terrain) become world files, not code.

## Near-term TODO — migrate the one-moon / two-moon (deorbit) scenes to engine-rendered worlds

These are the simplest scenes and the natural first "worlds" (no giant-impact machinery):
- **one-moon** (`orbit.html` "Space") and **two-moon** (`twomoons.html`) — real Earth + Moon(s), with the
  **deorbit** controls (`brake_moon` / `drop_moon` → the Moon's orbit decays into the planet).
- Today they're driven by bespoke `orbit.ts` + `OrbitDemo` wiring (moon count from a `<body data-moons>`
  attribute, hand-coded controls). **TODO:** re-express them as declarative worlds the engine loads and renders,
  so adding/altering a scene is editing data, not scene code — the first consumers of the world format below.

## The world format (sketch — to design)

A serialized description (JSON first; protobuf/flatbuffer later if size/perf matters) that Python/Go emit and
the engine (wasm + native) consumes:

- **bodies**: `{ mass, pos, vel, radius, material/EOS, spin? }` — point masses or SPH bodies.
- **scale/band**: which regime (space / terrain / giant-impact) → picks solver + render defaults.
- **events/triggers**: e.g. "at t, become Theia inbound at 1.15·v_esc, b≈R_e"; "on camera-visible contact,
  particalize" (the JIT trigger, docs/39/42) — declarative, not scripted outcomes (no-fudge).
- **camera**: focus targets (Earth / Luna / …), initial framing, follow rules.
- **time**: the fast-forward / geologic-time dials, aftermath rate.
- **controls**: which interactive buttons the scene exposes (brake, drop, replay, the pretty⇄physics slider).

The engine exposes one entry point — `Engine.load_world(world_json)` (wasm) / a native equivalent — that builds
the scene from data and runs it through the SAME sim + render path for every world. `web/` becomes a thin host
that fetches a world file and mounts it; a Python/Go SDK emits world files (and can generate ensembles /
parameter sweeps, tests, and the offline `tools/impact-run` runs from the same definitions).

## Why this is the right shape

- The laws are unified and verified; only ICs + triggers vary between scenes (docs/23/24/28/39). Encoding those
  as data removes the per-scene TS/Rust fork that the realignment (docs/33) is trying to kill.
- Authoring in Python/Go gets the scientific tooling (numpy, plotting, parameter sweeps) for free — a world
  file and an `impact-run` ensemble config become the same artifact.
- It makes the engine a reusable product ("here is a world, render it") rather than a set of hardcoded demos.

## Open questions (for later)

- JSON vs a binary schema (protobuf) — start JSON; revisit for large particle sets.
- How much behavior is data vs a small embedded scripting hook for genuinely-custom triggers.
- Where the Python/Go SDK lives (a sibling crate/pkg) and how it's versioned against the engine.

## Status

Parked/TODO — not started. Theia (birth-of-the-Moon) is the current live scene (docs/42); its render orbits
physically but reads as dispersal on screen (a render-communication gap, noted docs/42). This world-format work
is the next architectural thread when picked up.
