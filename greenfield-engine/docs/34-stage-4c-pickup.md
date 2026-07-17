# Stage 4c pickup — GPU integration loop + high-N impact + accretion operator (for a fresh session)

A self-contained hand-off so the next session executes 4c without re-deriving anything. Read `CLAUDE.md`,
`docs/32-architecture-map.md`, and `docs/33-architecture-realignment.md` first; this doc is the concrete task.

## Where things stand (4a + 4b DONE, verified on the RTX 2070)

- **`shaders/sph_step.wgsl`** — the GPU **force** kernel: SPH density (spatial-hash grid) + Tillotson EOS
  pressure + Monaghan artificial viscosity + direct-sum self-gravity + the `du/dt` energy equation. Kernels:
  `cs_grid_clear`, `cs_grid_insert`, `cs_density`, `cs_forces`. It is one force evaluation, **verified to f32
  precision** against the CPU physics.
- **`tools/sph-verify`** — the verification harness (standalone Vulkan-wgpu crate; the engine's own wgpu is
  webgpu-only and can't run native Vulkan, so this MUST stay separate). Run: `cd tools/sph-verify &&
  cargo run --release` → `PASS` means the GPU kernel matches an independent f64 CPU computation of the same
  equations. It builds a small mixed iron/basalt cluster and compares acc + du/dt.
- **CPU references (copy the equations EXACTLY when porting):**
  - `crates/engine/src/hydrostatic.rs` — `HydroBody`: `forces_and_dudt` (gravity + SPH pressure + AV +
    du/dt), `compute_density`, `step` (KDK leapfrog), `courant_dt` (adaptive dt), `relax_step`,
    `new_differentiated` (Genda equal-mass + adaptive-h build), and the 3c test
    `a_deformable_earth_impact_measures_the_disk_provenance` (perigee-above-remnant disk classification).
  - `crates/engine/src/eos.rs` — the Tillotson EOS (basalt verified vs Benz & Asphaug 1999; iron compressed
    branch vs Wissing & Hobbs 2020; granite/dunite vapor branch flagged provisional).
  - `crates/engine/src/neighbors.rs` / `bhtree.rs` — the CPU grid + Barnes–Hut (the exactness references).

## The 4c tasks, in order — each verified before the next

### 4c.1 — GPU KDK integration loop (+ adaptive Courant dt) — DONE ✓ (verified RTX 2070, 50 steps)
> Implemented: `cs_kick_drift` + `cs_kick` in `sph_step.wgsl`, `dt` field in `Params`. `tools/sph-verify`
> extended with an f64 CPU KDK reference + GPU multi-step runner; 50 steps @ dt=0.5s tracks to pos 3.1e-4 /
> vel 5.7e-4 / u 5.1e-4 RMS (f32 vs f64). Fixed dt on both sides. GPU adaptive Courant dt deferred to 4c.2.

Turn the verified force kernel into a time integrator. Match the CPU `HydroBody::step` KDK exactly:
`compute density+forces → v += a·dt/2, u += du·dt/2 → x += v·dt → recompute density+forces → v += a·dt/2,
u += du·dt/2`. So one step = TWO force evals (each = clear→insert→density→forces) with an `cs_integrate`
kernel between/after doing the half-kicks + drift.
- Add a `cs_integrate` kernel (or two: half-kick+drift, then half-kick). Clamp `u = max(u, 0)` (the CPU does).
- **Adaptive dt:** `courant_dt = cfl·min_i h_i/(c_i+|v_i|)`. First pass: compute `dt` on the CPU by reading
  back a min each step (simple, correct); a GPU reduction is a later optimization. For VERIFICATION, use a
  FIXED `dt` on both GPU and CPU.
- **Verify:** extend `sph-verify` to run K steps (e.g. 50) with a fixed dt on GPU and on the CPU
  (`HydroBody::step`), compare final `pos`/`vel`/`u`. Errors accumulate over steps, so a looser tolerance
  (~1e-3 RMS) is honest for f32 vs f64 — but it must track, not diverge.

### 4c.2 — High-N impact run (the converged isotopic-crisis number)
Run the deformable-Earth impact at N~10⁴–10⁵ on the GPU (offline is fine — a tool or a native harness).
- Build + **RELAX** both bodies first (`hydrostatic.rs` relax on the CPU, then upload — unrelaxed bodies
  inject energy; this is the 3a lesson). Or add a GPU damped-relax mode.
- Add a **provenance** field to the `Particle` struct (repurpose `_pad` or add a field — keep 16-byte std430
  alignment) so Earth vs Theia can be measured.
- Collide obliquely at ~mutual escape speed; step on GPU; measure the bound orbiting disk by the
  perigee-above-remnant criterion (copy the 3c test). This converges the coarse-N 58% number.

### 4c.3 — Accretion operator (NEW physics — the Moon-formation fix)
Diagnosis (JOURNAL 2026-07-17): the disk never accretes a Moon because (a) at low N it's collisionless and
(b) there is NO fusion/growth operator — particle masses never grow. Higher N (4c.2) makes it collisional;
this adds the growth law so a round Moon can emerge.
- Detect gravitationally-**bound clumps** (union-find on contact adjacency / friends-of-friends, like
  `disk_stats_json`'s clump counter). Promote a bound clump to ONE body: mass = Σm, pos/vel = COM, radius
  from ρ·V — conserving mass, momentum, and (as far as possible) energy + angular momentum. Or keep the
  clump as a cohesive sub-body. This is new — design carefully and verify conservation with a TDD test.
- Honest: no merge unless genuinely bound (negative pair energy) and past the Roche limit (a clump inside
  Roche should shred, not accrete — the `tides::secular_step` Roche logic is the aftermath analogue).

### 4c.4 — Scene wiring (browser, the big integration)
Wire the GPU stepper into the birth scene (`OrbitDemo`) so the impact runs at high N in the browser
(docs/22 step 4). Pattern to follow: the terrain band's `GpuParticles` in `lib.rs` (buffers, dispatch loop,
zero-copy sim↔render). Needs an **f32 Earth-relative/local frame** (planetary coords in f32 cancel — keep
positions relative to Earth's centre). Test IN-BROWSER (WebGPU, not Vulkan; the WGSL is portable but verify).

## Gotchas from this session — READ before coding
- **Engine wgpu is webgpu-only (no native Vulkan)** → all native GPU verification stays in a standalone
  crate (`tools/sph-verify` / `gpu-verify`). Do NOT add Vulkan features to the engine crate (bloats the wasm
  + breaks the webgpu-only build via feature unification).
- **Grid exactness needs a CELL-MEMBERSHIP GUARD** — process a bucketed particle j only when scanning
  `cell_of(j)` (`sph_step.wgsl` already does this). Without it, hash collisions among the 27 cells
  double-count neighbours (the 4b bug: 20% error). Keep this if you touch the grid.
- **RELAX bodies before colliding** — unrelaxed spheres dump startup non-equilibrium into the shock and
  triple the energy (measured, 3a).
- **f32 planetary coords** need an Earth-relative frame to avoid catastrophic cancellation.
- **Verify-before-wire** (docs/30 discipline): every kernel checked against the CPU/exact reference on the
  real GPU before it's used. That's how the 4b double-count bug was caught before it corrupted a real run.
- Workflow: `bash scripts/test.sh [--fast] [filter]`; `cd tools/sph-verify && cargo run --release` (GPU
  verify); rig-watch via `web/rig/*.mjs` (headed Chromium + xvfb). **NEVER `cargo fmt`.** Work in the
  worktree, never the main checkout.

## Definition of done
4c is complete when: the GPU stepper is verified against the CPU over many steps (4c.1); a high-N impact runs
and the disk-provenance number is measured (4c.2); an accretion operator lets a bound clump grow into one
body, conservation-tested (4c.3); and the deformable-Earth impact runs at high N in the browser birth scene
(4c.4). Then stages 5 (fold `hydrostatic`/`AirField` into `Aggregate` — the one-module goal) and 6
(energy-tiered just-in-time particalization) remain.
