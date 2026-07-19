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

## DELIVERED — the world format, on two scene kinds (2026-07-19)

The world format now exists and drives real scenes. Two `type`s so far, one reusable schema
(`crates/engine/src/terra/world_def.rs`):

- **`type: "planet"`** — the terrain world (`Terra`): `planet` + `surface` (rasters, biomes, relief dial) +
  `atmosphere` + a `"fly"` `camera`. First scene, docs/43 Phases 1–6, live at integrity.bothead.net → Earth.
- **`type: "system"`** — an N-body space world (`OrbitDemo`): a **`bodies[]`** array + an `"orbit"` camera. This
  is the migration this doc asked for: the **Space** (one-moon) and **Two Moons** deorbit scenes are now data.

**System-world schema** (`bodies[]` each): `{ name, role: "star"|"planet"|"moon", mass_kg?, radius_m?, profile?
("sun"/"earth"/"moon" → mass/radius/tint from `planet::` + composition, so bodies stay *declared, not fudged*),
pos_m:[x,y,z], vel_ms:[x,y,z], spin_period_s?, tint? }`. Camera adds orbit fields `{ mode:"orbit", yaw, pitch,
zoom, focus: <body name> }` (the frame-of-reference body). `time: { scale }` is the fast-forward dial. A
`controls` block declares the interactive buttons/keys (brake / drop / reset / focus).

**Engine entry:** `OrbitDemo::load_world(json)` (mirrors `Terra::load_world`) — `create(canvas, num_moons)` then
`load_world` replaces the built-in Sun/Earth/Moon constants with the declared initial conditions, spin, tints,
time scale, focus, and orbit-camera framing. The **deorbit stays a user control** (`brake_moon` = ×½ the moon's
Earth-relative velocity, `drop_moon` = ×0 → radial infall); the crash geometry/energy EMERGES from the N-body
integrator + swept contact — no scripted outcome (no-fudge). World files:
`web/public/worlds/{one-moon,two-moons}/world.json`. Host: `web/src/orbit.ts` reads `<body data-world="…">`,
fetches the JSON, derives the moon count, and calls `create` + `load_world` (Birth of the Moon stays on the code
path — the GPU-SPH impact — for now).

**Still constant in v1 (flagged follow-ups):** the planet's *render* radius uses the `EARTH_RADIUS_M` constant
(the world declares a matching `radius_m`, consumed for the moon impactor + framing but not yet threaded into the
Earth render/contact); the space **controls are not yet bound from `controls.keys`** (the buttons remain the
existing `orbit.ts` panel — the Terra Phase-6 controls-from-JSON pattern should be applied here); **Birth of the
Moon** (giant impact) and a **Python/Go world-authoring SDK** are still to migrate. The `events/triggers` section
(JIT particalize on contact, docs/39/42) is still a sketch — the deorbit needed none (it's a pure impulse).

## Status (superseded above)

Theia (birth-of-the-Moon) is the current GPU-SPH live scene (docs/42). The world-format work is now **underway**:
`Terra` (planet) + the one/two-moon deorbit **system** worlds ship it end-to-end.
