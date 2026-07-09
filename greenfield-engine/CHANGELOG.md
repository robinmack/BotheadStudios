# Changelog

All notable changes to `greenfield-engine` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
See [`docs/03-versioning.md`](docs/03-versioning.md) for our versioning policy — it matters
because **we are our own first customers** and pin exact engine versions in our games.

## [Unreleased]

## [0.7.0] — 2026-07-08

**Phase 6 — smooth surface meshing.** Terrain and craters render as smooth surfaces instead of
Minecraft-style cubes. The voxel grid stays the physics substrate; only the *visual* changes.

### Added
- `mesher::build_surface_nets` — Surface Nets (via the `fast-surface-nets` crate) over the voxel
  occupancy field, with **smooth normals recomputed from the geometry** (the binary field's own
  gradient is blocky) and oriented outward. Each vertex is tagged with the nearest solid voxel's
  material, so triplanar texturing (Phase 4) and specular shine still apply. Native-tested (valid,
  finite, and genuinely smooth — non-axis-aligned normals).
- The renderer uses it for the initial terrain and every dig re-mesh. The blocky `build` mesher is
  kept as a reference/fallback.

### Notes
- Sim/visual decoupling: physics (mass, gravity, fracture, collapse) is unchanged — the world is
  still "voxels all the way down"; the renderer just presents it smoothly.
- Binary field ⇒ mildly-rounded geometry + smooth shading. Further realism (a smoothed/SDF field for
  rounder geometry, normal maps, finer debris) is future work.

## [0.6.0] — 2026-07-08

**Phase 5 — structural collapse.** Matter that a dig undercuts or isolates no longer floats: anything
not connected to the ground falls. Removes the Phase-3 "floating voxels" limitation.

### Added
- `world.find_unsupported()` — flood-fill from the anchored base (`y = 0`); returns every solid voxel
  not connected to it (6-connectivity). Handles overhangs, undercuts, and blasted-off chunks uniformly.
- `MatterSim::collapse()` — detaches unsupported voxels into falling particles (from rest); one pass
  suffices (the remainder is fully supported). Triggered after every dig.
- Native tests: intact terrain has zero unsupported voxels; an isolated voxel collapses, conserves
  matter, and re-settles into the grid.

### Notes
- Collapse is O(voxels) per edit (a user action, not per-frame). If a collapse would exceed the
  particle budget it caps (a few voxels may remain floating) — noted as a bound, not a silent drop.

## [0.5.0] — 2026-07-08

**Phase 4 — emergent textures.** Completes the vertical-slice roadmap. Materials get a distinct look
generated *from their own physical properties* — no bundled image files, zero licensing exposure.

### Added
- `texture.rs` — procedural texture generator: high-res (512²) RGBA with a full mip chain, synthesized
  from `albedo` + `color_variance` + `metallic` (grain/mottle from tileable multi-octave noise,
  mineral flecks, metal sparkle specks). Seamless (wrapping lattice). Native tests: size + mip chain,
  mean color tracks albedo, materials differ, non-flat variation.
- World shader: **triplanar** sampling of a per-material procedural texture array (no UVs), plus a
  **specular highlight (shine)** driven by per-material `roughness`/`metallic` (metals get a tighter,
  tinted highlight). Material id per vertex; the probe renders as textured iron.
- `materials.rs` loads `roughness`/`metallic`/`color_variance`. HUD adds an **FPS** counter.
- `docs/12` — texture approach + verified CC0 sources (ambientCG/Poly Haven) for optional
  user-supplied real textures via the module system.

### Notes
- Mipmapping is the "client can scale it down" mechanism; `TEX_SIZE` is one constant to raise for
  more detail. The engine bundles **no images** — a material *module* may later drop in a CC0 photo.
- This closes the initial Phase 0–4 vertical slice: layered voxel matter · self-gravity · dig &
  fracture · emergent texture — all from the cited material database.

## [0.4.0] — 2026-07-08

**Phase 3 — dig & material-driven fracture.** Click to dig; matter breaks apart according to each
material's own strength, falls under gravity, and settles back into the world.

### Added
- `matter.rs` — CPU matter solver: spherical dig via voxel raycast; a voxel detaches into a particle
  only if the tool's stress exceeds its material's `fracture_strength` (granite resists a tool that
  shreds soil/grass — no per-material special-casing, just the numbers). Debris falls under the
  Phase-2 field and, on rest, deposits back into the voxel grid (piling; matter-conserving). Native
  tests: soft-vs-hard selectivity, and matter conservation through dig + settle.
- `world.rs` — voxel raycast (Amanatides–Woo DDA) for picking, `set_voxel`, `solid_count`.
- `materials.rs` — loads `fracture_strength` (tensile strength, falling back to cohesion).
- Renderer — instanced debris cubes (`particles.wgsl`), terrain re-mesh on edit; HUD shows debris
  count. Controls: **click** to dig soil/grass, **shift-click** to blast rock.

### Notes
- This is the CPU-tested **foundation** for full continuum MLS-MPM, not the full method yet — it
  delivers dig/fracture/granular behavior emergent from material data. MLS-MPM (deformation gradient +
  constitutive stress, then a WGSL port) is the planned evolution (`docs/06`/`08`).
- Micro-gravity again: ejection is capped below the world's ~7 cm/s escape velocity so debris stays
  bound and re-settles (correct physics, viewed via the time-scale).
- Digging a mid-column hole can leave voxels above "floating" — structural collapse is future work.

## [0.3.0] — 2026-07-08

**Phase 2 — self-gravity & the falling probe.** Density stops being decorative and starts doing
physics: the world's summed voxel mass produces a real Newtonian gravitational field, and a sphere
falls under it (`F = ma`) and rests on the surface.

### Added
- `gravity.rs` — aggregate voxel-mass gravity field (voxels lumped into blocks; direct-sum
  `g(p) = ΣG·mᵢ·(cᵢ−p)/|cᵢ−p|³`, f64 accumulation). Native tests: point-mass `G·M/r²`, far-field,
  mass conservation.
- `body.rs` — rigid sphere integrated with semi-implicit Euler under the field, with ground contact,
  restitution/friction, and a scale-relative rest threshold (works from Earth-g to micro-g). Native
  tests: free-fall kinematics, fall-and-rest.
- Renderer draws the probe (a second mesh with a per-object model matrix); live HUD shows world mass,
  local gravity, probe altitude/speed, rest state, and time-scale. Controls: `Space`/`R` re-drop,
  `[`/`]` time-scale.
- End-to-end native test: the probe falls toward the generated world and rests on it.

### Notes
- Real `G` is used, so the ~96 m test world has asteroid-scale micro-g (~1e-5 m/s²) — correct
  physics. A **time-scale** (default 250×) fast-forwards the sim for viewing; it is time-lapse, not
  amplified gravity.
- The probe is hand-integrated (one body); Rapier is deferred until many bodies / arbitrary contacts
  justify it. The rendered sphere is enlarged for visibility (free-fall is size/mass-independent).

## [0.2.0] — 2026-07-08

**Phase 1 — layered voxel world.** The cited material data becomes a rendered, orbitable world.

### Added
- `data/materials.json` — 19 cited materials (density, moduli, strengths, hardness, albedo, …) as
  the physical single source of truth (`docs/04`).
- Engine sim modules (natively unit-tested): `materials` (loads the database), `world` (chunked
  voxel store + layered rock/dirt/grass generator with a value-noise heightfield, using real
  densities), `mesher` (face-culling mesh, per-material albedo colors).
- Real 3D renderer: depth buffer, perspective orbit camera, directional + hemispheric lighting;
  `Engine.set_orbit(yaw, pitch, zoom)`. Host adds drag-to-orbit / scroll-to-zoom.
- `cargo test` suite (material load, layer ordering, mesh validity) — TDD is canonical; wgpu/wasm
  code is gated to the wasm target so the sim logic tests natively.
- Design docs `05`–`10`: Postgres→JSON data pipeline, material modules, taxonomy/finishes/object
  composition, adaptive clumping/LOD, agentic authoring + interaction, and robustness principles.
- CI: fmt + clippy + native tests + wasm build on every push.

### Notes
- Face-culling (blocky) mesher for now; smooth surface-nets meshing is a planned upgrade.
- Density is stored per material but not yet physically active — it drives self-gravity in Phase 2.

## [0.1.0] — 2026-07-08

First milestone: **Phase 0 — scaffold & first pixel.** The full Rust → WASM → `wgpu` → canvas
pipeline is live, driven by a thin Vite/TypeScript host.

### Added
- Cargo workspace with the `engine` crate (`cdylib` + `rlib`) compiled to WASM via `wasm-pack`.
- `Engine` WASM API: `Engine.create(canvas)`, `render()`, `resize(w, h)` — a `wgpu` WebGPU
  device that clears the canvas with a pulsing color each frame.
- Vite + TypeScript host (`web/`) that loads the WASM, sizes the canvas, and pumps
  `requestAnimationFrame`, with a graceful "WebGPU unavailable" message.
- Project meta: MIT license, `README`, `CONTRIBUTING`, `JOURNAL`, this changelog, and two
  research reports under `docs/` surveying prior art and reusable OSS building blocks.

### Notes
- Pinned to `wgpu` 24.0.5. WebGPU-only backend to keep the WASM small.
- **Public API is unstable** while we're pre-1.0 (see versioning policy).

[Unreleased]: https://example.invalid/compare/v0.7.0...HEAD
[0.7.0]: https://example.invalid/releases/tag/v0.7.0
[0.6.0]: https://example.invalid/releases/tag/v0.6.0
[0.5.0]: https://example.invalid/releases/tag/v0.5.0
[0.4.0]: https://example.invalid/releases/tag/v0.4.0
[0.3.0]: https://example.invalid/releases/tag/v0.3.0
[0.2.0]: https://example.invalid/releases/tag/v0.2.0
[0.1.0]: https://example.invalid/releases/tag/v0.1.0
