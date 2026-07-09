# Development Journal

A running log of major milestones for `greenfield-engine`. Newest entries at the top.
Each entry records *what* changed, *why*, and *how it was verified*.

---

## 2026-07-08 — Phase 6: smooth surface meshing (v0.7.0)

**What.** Terrain and craters now render smooth instead of blocky cubes. `mesher::build_surface_nets`
runs Surface Nets (`fast-surface-nets` crate) over the voxel occupancy field, recomputes smooth
normals from the geometry (oriented outward), and tags each vertex with its nearest material so
triplanar texturing + shine still apply. The renderer uses it for the initial terrain and every dig
re-mesh; the blocky mesher is kept as a fallback.

**Why.** The user flagged the Minecraft-blocky look. The key insight: the voxel grid is the *physics
substrate*, not the *visual* — so we smooth the rendering (marching-cubes/surface-nets style) while
mass, gravity, fracture, and collapse stay identical. Prototype clunkiness → smooth surface, no
physics change.

**Verified (TDD).** `cargo test`: 19/19 (new: surface-nets mesh is valid, finite, and genuinely
smooth — has non-axis-aligned normals). fmt + clippy (`-D warnings`) clean; wasm + web build green.
Live LAN wasm rebuilt. **Pending human check:** reload → rounded terrain and craters, still textured
and lit; dig/blast/collapse all still work.

**Next realism levers (noted):** smoothed/SDF field for rounder geometry, normal maps from the grain
field, finer/smoother debris (or MPM).

---

## 2026-07-08 — Phase 5: structural collapse (v0.6.0)

**What.** Undercut or isolated matter no longer floats. `world.find_unsupported()` flood-fills from
the anchored base (`y=0`) and returns any solid voxel not connected to it; `MatterSim::collapse()`
detaches those into falling particles, run after every dig. This closes the Phase-3 "floating voxels"
known limitation — overhangs, undercuts, and blasted-off chunks all fall and re-settle.

**Why.** Real matter needs support. Connectivity-to-anchor is the general, correct model (works on a
plateau now and a planet core later) and needs no per-case rules.

**Verified (TDD).** `cargo test`: 18/18 (added: intact terrain has no unsupported voxels; an isolated
voxel collapses, conserves matter, and re-settles). fmt + clippy (`-D warnings`) clean; wasm + web
build green. **Pending human check:** `npm run dev` → shift-click to undercut a ledge and watch the
overhang break loose and tumble down.

---

## 2026-07-08 — Phase 4: emergent textures (v0.5.0) — vertical slice complete

**What.** Materials now look distinct, generated *from their own properties* with **no bundled
images**. `texture.rs` synthesizes a high-res (512²) mip-mapped texture per material from
albedo + color_variance + metallic (grain/mottle + flecks + metal sparkle), seamless. The world
shader triplanar-samples a per-material texture array and adds a specular highlight (shine) from
per-material roughness/metallic. HUD gains an FPS counter. `docs/12` documents the approach + CC0
sources (ambientCG/Poly Haven) for optional user textures.

**Why.** Closes the appearance side of the thesis: look emerges from the same cited data that drives
mass, gravity, and fracture — one source of truth. User asked for high-res + no licensed photos;
procedural generation delivers both (mipmaps = scale-down; zero image assets = zero licensing).

**Verified (TDD).** `cargo test`: 16/16 (added 4 texture tests: size+mip chain, mean tracks albedo,
materials differ, non-flat variation). fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` +
`vite build` green. **Pending human check:** `npm run dev` → speckled granite, mottled dirt, green
grass, a shiny iron probe; dig to see textured debris.

**Milestone.** This completes the **Phase 0–4 vertical slice** from the plan: layered voxel matter ·
self-gravity (F=ma) · dig & material-driven fracture · emergent texture — all driven by the cited
material database. All four project pillars are demonstrable.

---

## 2026-07-08 — Phase 3: dig & material-driven fracture (v0.4.0)

**What.** Destructible matter. `matter.rs` is a CPU matter solver: click-to-dig (voxel raycast DDA)
fractures a spherical region — a voxel detaches into a particle only if the tool's stress exceeds its
material's `fracture_strength` (loaded from the cited DB). Debris falls under the Phase-2 gravity
field and, on rest, deposits back into the voxel grid (piling, matter-conserving). Instanced debris
rendering (`particles.wgsl`), terrain re-mesh on edit, HUD debris count. Click digs soil/grass;
shift-click blasts rock.

**Why.** Proves the core destruction thesis — materials break *differently by their own numbers*
(granite shrugs off what shreds grass), with no per-material special-casing. Framed honestly as the
**CPU, testable foundation** for full continuum MLS-MPM (deformation/stress + WGSL port) later, since
GPU MLS-MPM can't be unit-tested natively and TDD is canonical.

**Verified (TDD).** `cargo test`: 12/12, incl. `dig_detaches_soft_but_not_hard` (soil detaches under
1e6 Pa, granite needs a 2e7 blast) and `matter_conserved_through_dig_and_settle` (voxels + airborne
particles == original, every step, until all settle). Plus raycast-hits-terrain. fmt + clippy
(`-D warnings`) clean; wasm build clean; `tsc` + `vite build` green.
**Pending human check:** `npm run dev` → click the grass/dirt to blow a crater of tumbling debris
that resettles; click rock (nothing) then shift-click (it breaks).

**Known limits (noted for later):** mid-column digs can leave floating voxels (no structural
collapse yet); full-world re-mesh per edit (dirty-chunk meshing is the optimization).

---

## 2026-07-08 — Phase 2: self-gravity + falling probe (v0.3.0)

**What.** Made density physically active. `gravity.rs` computes a real Newtonian field from the
world's aggregate voxel mass (voxels lumped into blocks; direct-sum with f64 accumulation).
`body.rs` integrates a rigid sphere under that field (`F = ma`, semi-implicit Euler) with ground
contact and a scale-relative rest threshold. The renderer draws the probe via a per-object model
matrix; a live HUD shows world mass, local gravity, altitude, speed, rest state, and time-scale
(`Space` re-drops, `[`/`]` change time-scale).

**Why.** Proves pillar 4 — the world's own summed mass produces gravity; a probe obeys `F = ma` and
rests on the surface. No Rapier yet: one hand-integrated body is exact and far simpler; Rapier is
deferred until many bodies/contacts justify it.

**Honest scale note.** Real `G` is used, so the ~96 m world has asteroid-scale micro-g (~1e-5 m/s²).
That's correct physics; a time-scale fast-forwards the sim for viewing (time-lapse, not fake gravity).

**Verified (TDD).** `cargo test`: 9/9 — point-mass `G·M/r²`, far-field within 1%, mass conservation,
free-fall kinematics (`v=-g·t`, `½g·t²`), fall-and-rest, and an end-to-end drop onto the generated
world. fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` + `vite build` succeed.
**Pending human check:** `npm run dev` → watch the iron probe fall and settle; HUD reads out g and rest.

---

## 2026-07-08 — Phase 1: layered voxel world on screen (v0.2.0)

**What.** Turned the material data into a rendered world. Added to the engine crate:
- `materials.rs` — loads the cited `data/materials.json` (density + albedo) at compile time.
- `world.rs` — chunk-style voxel store + a layered generator: rock bulk, ~10 m dirt, grass skin,
  with a deterministic value-noise heightfield so the surface undulates (layers follow terrain).
- `mesher.rs` — face-culling mesher (only air-facing faces), per-material albedo vertex colors, so
  the rock/dirt/grass bands are visible on the exposed side walls.
- `lib.rs` + `shaders/world.wgsl` — a real 3D renderer: vertex/index/uniform buffers, depth buffer,
  perspective orbit camera, and a directional light + ambient/hemispheric fill.
- `web/` host: drag-to-orbit / scroll-to-zoom controls, gentle idle auto-rotation.
Also added `docs/10` (robustness — how the matter-first model designs out tunneling / fall-through /
"weird physics", plus the mitigations and an adversarial test plan).

**Why.** First milestone that makes "density as source of truth" *visible* and validates the core
Rust→WASM→wgpu render path end to end, on the real seed data.

**Verified.**
- `wasm-pack build` clean (no warnings). `tsc` clean. `vite build` succeeds (wasm ~1.32 MB dev).
- Dev server serves `engine_bg.wasm` as `application/wasm`.
- `cargo test` (native): material DB loads 19 materials with granite denser than dirt; the central
  column is grass→dirt→rock top-to-bottom and solid to y=0; mesher output is well-formed (quad-aligned
  vertices, 6 indices/quad, all indices in range).
- **Pending human check:** `cd web && npm run dev` in a WebGPU browser — a layered rock/dirt/grass
  plateau you can orbit and zoom.

**Version.** Milestone **0.2.0** (Phase 1) per the pre-1.0 policy (each phase bumps the minor).

---

## 2026-07-08 — Materials seed database + object/interaction design

**What.** Compiled the first **cited physical-properties database** — 19 materials (rock, ceramic,
metal, organic/wood, soil, granular, liquid, frozen) with mechanical + optical properties and source
URLs — into `data/materials.json` (schema in `docs/04`). Added design docs for the architecture the
user articulated: material **taxonomy + finishes + object composition** (`07`), **adaptive resolution
& clumping** so the sim scales instead of moving billions of particles (`08`), and **agentic object
authoring + physically-grounded tool/terrain interaction** — the "make a shovel" / shovel-in-dirt
vision (`09`).

**Why.** Physical properties are the single source of truth for both simulation and rendering; the
whole object/agentic vision ("make a shovel" that falls, sounds, and digs like one) reduces to
material data + physics + composition, with no bespoke per-object code.

**Verified.** `data/materials.json` parses (node `JSON.parse`), 19 materials each with mechanical +
optical blocks; categories: rock 4, ceramic 1, metal 3, organic 3, soil 2, granular 2, liquid 2,
frozen 2. Research quality-checked: rejected known-bad MatWeb figures (granite/limestone UCS),
flagged cited-vs-estimate, and captured state-dependence (soils/snow) and anisotropy (wood).

**Note.** JSON is the v0 seed; it migrates to the Postgres source of truth (`docs/05`) and grows into
the module/taxonomy system (`docs/06`, `07`) over time.

---

## 2026-07-08 — Published to GitHub as a monorepo

**What.** Restructured the engine into the `robinmack/BotheadStudios` monorepo as its first
project directory, `greenfield-engine/`. Root of the monorepo carries an MIT `LICENSE` and a
projects README. Aligned the engine to **MIT-only** (dropped the Apache dual-license) to match the
repo's license choice. Published the public OSS repo and tagged `v0.1.0`.

**Why.** BotheadStudios will hold multiple game projects; a monorepo keeps them together. MIT
across the board keeps licensing simple and consistent.

**Verified.** `git push` to `origin/main` succeeded; `v0.1.0` tag pushed; repo is public.

---

## 2026-07-08 — Project kickoff & Phase 0 scaffold

**What.** Created the engine as the first project in the **BotheadStudios monorepo**
(`greenfield-engine/`). Established the skeleton: `crates/` (Rust core), `web/` (TypeScript host),
`shaders/` (WGSL), `docs/` (research + design).
Added `README.md`, `LICENSE-MIT`, `CONTRIBUTING.md`, `.gitignore`, this journal.
Installed the toolchain: Rust 1.96.1 + `wasm32-unknown-unknown` target + wasm-pack 0.13.1 (Node 22 already present).

**Why.** The plan (see `.claude/plans/…` / `docs/`) settled a performance-first stack — Rust→WASM core,
custom `wgpu` WebGPU renderer, Rapier rigid bodies — after research confirmed **no existing engine fuses
all four pillars** (density-as-truth matter · emergent-from-density behavior · destructible-to-the-core ·
real self-gravity). See `docs/01-prior-art-existing-engines.md` and `docs/02-oss-building-blocks.md`.

**Phase 0 goal.** Prove the pipeline end-to-end: a Rust crate compiled to WASM initializes a `wgpu`
device and clears a browser canvas, driven by a thin Vite/TypeScript host. First pixel on screen.

**Verified (build/serve level).**
- Rust → WASM compiles via wasm-pack (fixed three `wgpu` 24.0.5 API differences vs. older docs:
  `request_adapter` returns `Option`, `request_device` takes a trailing `Option<&Path>` trace arg,
  and `RenderPassColorAttachment` has no `depth_slice` field).
- `npx tsc --noEmit` clean; `vite build` bundles the app (wasm 933 KB → 236 KB gzipped).
- `vite` dev server serves `engine_bg.wasm` as `application/wasm` (verified magic bytes `\0asm`).
- **Pending human check:** open `npm run dev` in a WebGPU browser to see the pulsing clear color.

**Version.** Tagged this milestone **0.1.0** (see `CHANGELOG.md`, `docs/03-versioning.md`).
Pre-1.0 policy: each roadmap Phase bumps the minor; games pin exact versions since we dogfood.

---
