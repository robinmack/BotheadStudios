# 69 — The separation sweep: matter, assembly, engine, viewer

Robin, 2026-08-09: *"I think we need to do a rigor sweep based on the assembly/matter/engine/viewer
paradigm and make sure they are all separated out correctly and tested properly."*

This is that sweep. It is an **audit with evidence**, not a design: every entry names a file and a
line, and every claim about what the code does was checked by reading or by measurement. Where a
boundary can be defended by a machine it now is (`laws::separation_tests`); where it cannot, the
violation is a row in [`docs/46`](46-one-physics-charter.md)'s ledger so it is inherited rather than
rediscovered.

It exists because of what the same day produced. Terra's flora drew 1,200 plants and 43,200 triangles
and could not be seen, for weeks, and the cause was a renderer holding a baked copy of a model answer
the model could not correct (docs/46 row 50). Robin named the disease while the hunt was still
running: ***"too much pollution between duties."*** So: which duties, and where exactly are they
polluted?

## 1. The four roles, and the test for each boundary

| Role | Owns | Must never |
|---|---|---|
| **MATTER** | What substances ARE — density, strength, melting point, spectra. `data/materials.json`, `materials.rs`, `eos.rs` | Know about objects, scenes or cameras. A material is the same in a cannon and in a crust |
| **ASSEMBLY** | Objects made of matter that know themselves — parts, joins, mass, extent, crown, damage. `assembly.rs`, `instance.rs`, `containment.rs` | Know how it is drawn, or where the camera is. An assembly answers *what am I and where do I reach*, in metres |
| **ENGINE** | Director and stage — time, the contact and gravity laws, what exists, what is resolved. `granular.rs`, `gravity.rs`, `simulation.rs`, the `cs_*` compute shaders | Let a visual criterion decide what is simulated (Law VI), or answer one physical question twice (Law II) |
| **VIEWER** | Light — how what exists is shown. `render.rs`, `renderer.rs`, the vertex/fragment shaders, `atmos.wgsl` | Change WHAT the engine says. It may approximate HOW it shows it (docs/68 §1b) |

Two clarifications the sweep needed before it could classify anything:

- **A compute shader is the engine, not the renderer.** `sph_step.wgsl`, `particle_step.wgsl` and
  `bh_gravity.wgsl` hold real physics and that is correct: the GPU is a processor the engine uses.
  The boundary is about DUTY, not about hardware. `laws::separation_tests` skips anything with an
  `@compute` entry point for exactly this reason.
- **Light transport is the viewer's own physics.** `atmos.wgsl` marches a single-scatter integral,
  and that belongs there — Robin: *"perhaps its realm is light and it handles the raytracing, since
  particle physics cares little about photons."* A shader computing radiance is doing its job; a
  shader computing where matter IS is not.

## 2. What the sweep found

### 2a. Machine-defended now

| Boundary | Gate | State |
|---|---|---|
| The model must not invent a viewer | `separation_tests::the_model_does_not_invent_a_viewer` | **8 sites, ratcheted.** `arc.rs` ×2, `site.rs`, `surface_detail.rs`, `terra/ground_cap.rs`, `lib.rs` ×3 |
| A render shader may shape light, not matter | `separation_tests::a_render_shader_does_not_move_matter` | **1 site, ratcheted.** `globe.wgsl::crater_sink` |
| A scene may not grow new verbs | `laws::scene_api_tests` (docs/65) | Ratchet, already in place |
| Every shader is compiled by something | `laws::compiled_shader_tests` | Green |
| A declared quantity must trace to physics | `laws::` — several | Green |

Both new gates were **verified by making them fail** (add a ninth `ResolutionController::default()`
→ red; rename `crater_sink` away → red), because a gate that reports a problem and passes is worse
than no gate.

**`ResolutionController::default()` is the sharpest single finding of the sweep.** docs/68 states
the rule — *"the viewport decides RESOLUTION, and resolution is a request the renderer MAKES of the
model, not a decision it makes FOR it"* — and eight places in the model call a constructor that
conjures a viewer with a declared 1 mrad eye and answers the question unasked. It is the same
mistake `FLORA_ALT_M = 300` was: one invented cutoff standing in for a real viewport, wrong by a
factor of forty (docs/46 row 49). `render::Fidelity::of_view` is the shape of the fix and already
exists.

### 2b. Recorded in the ledger, not yet fixed

| Row | Violation | Evidence |
|---|---|---|
| 52 | A renderer cache that no model change can invalidate | 36.1 m, measured, surviving a 4 km descent |
| 53 | Two answers to "how high is the ground here" | +1.19 m centre, 12.3 m worst, 0.000 m once tiles cover |
| 54 | The renderer computes the crater | `globe.wgsl::crater_sink`, a paraboloid in a vertex shader |
| 55 | A container does not place its contents — a scene walks a list | `Terra::build_flora` iterates and places |
| 56 | Nothing is lit by the sky | Grass blades of the same material as the ground render nearly black |

### 2c. Found by reading, and honest as they stand

- **`surface_normal.wgsl`** synthesises triplanar relief and albedo the model has no record of. This
  is legitimate under docs/68 §1b — it is appearance, nothing interacts with it, and the meteor does
  not consult it — but it means there are now **three** surfaces (the model's, the mesh's, the
  shader's) and only rows 53/54 track the first two. Worth watching, not worth a row yet.
- **`terra::flora::Built`** (added the same day) is the first renderer cache in the tree that names
  its own inputs. It is the pattern the others should follow.
- **`assembly::stand_on_body` / `assembly::model_of`** now split one expression that used to compute
  a placement, a display scale and an eye-relative transform together. The model half has no viewer
  in it and is natively testable; the renderer half cannot move anything because it is handed the
  answer.

## 3. Tested properly? — the second half of the question

Robin asked two things: separated correctly, **and tested properly**. The second is where the honest
answer is worse.

- **Matter and assembly are well tested.** They are pure, native, and the suite exercises them:
  mass, extent, crown, placement, joins, compiled-asset staleness.
- **The engine is well tested where it is pure** and verified out-of-process where it is not
  (`tools/sph-verify` for `sph_step.wgsl`, `tools/gpu-verify` for compute).
- ★ **The viewer is barely tested at all, and that is the gap this session paid for.** Both scene
  structs live behind `#[cfg(target_arch = "wasm32")]`, so a native run cannot see them; the wasm
  gate only proves they COMPILE. Everything else is a rig photograph judged by eye. The flora defect
  survived weeks inside exactly that blind spot: `flora_count` said 1,200, the draw said 43,200
  triangles, the readback said the device held what was sent, and every one of those was true.
- **The one thing that broke the deadlock was `renderer::Readback`** — the renderer being ASKED what
  it holds. It disproved its author on first use. More of the viewer's state should be answerable
  that way, and the cheapest next instrument is the one this session used by hand: **project the
  bounds of what is about to be drawn and report where they land in clip space.** Every hour of the
  flora hunt would have been one line of output.

## 4. What follows from this, in order

1. **One owner for ground level** (row 53) — the model answers ground height at a coordinate, at a
   fixed physical floor with bounded truncation, and the mesh band-limits it for drawing. Unblocks
   the crater (row 54), because an excavation is then just another term in the one answer.
2. **Earth as an assembly that holds contents** (row 55, docs/67 §5) — the blocker rows 46, 52 and
   55 all wait on. Until it exists, "the container places its contents" is a decree the source does
   not implement, and saying otherwise is the 2026-08-05 architecture-page failure repeated.
3. **Ask the renderer, don't guess** (§2a) — retire the eight invented viewers onto `Fidelity`.
4. **Sky irradiance** (row 56) — the fourth consumer of an integral already built three times.

---

*Written 2026-08-09, from the flora hunt. Every number in it was measured on this machine, and the
two new gates were each confirmed to go red before being trusted.*
