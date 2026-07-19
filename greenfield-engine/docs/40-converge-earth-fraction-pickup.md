# docs/40 — #3: converge the disk Earth-fraction (GPU/high-N + ensemble). Fresh-session pickup spec

Self-contained hand-off so a fresh context builds #3 without re-deriving. **Read first:** `CLAUDE.md`,
`docs/39` (the planetary-scale JIT design + the 39a–e/#1/#2 RESULTS this builds on), and the memory
`project_planetary_scale_jit`. Branch: `planetary-scale-jit` (all of 39a–e + #1/#2 committed). This doc is the
concrete task.

## Why (the measured motivation — don't re-litigate)

The disk Earth-fraction is **chaos/scatter-dominated**: docs/28 saw 28–50% on *identical* N=35k GPU runs; the
CPU coarse-N #1 experiment swung **25% (all-fine) vs 63% (coarse-core)** between nominally-similar configs.
So no single run is a number — the "does the deformable coarse-bulk reach the all-particle 58%?" question
(#1) and "does a resolved Moon accrete?" (#2) BOTH require an **ensemble average + higher N + a deterministic
reduction**. That is #3, and it is the critical path (it converges the numbers #1 and #2 could only point at).

## What #1 proved that makes #3 SIMPLER than first sketched

The rigid coarse-bulk (monopole + non-injecting floor, docs/39 39d) was the LIMITER — it reflects the deep
shock and plateaus at ~25%. The winner (#1) is **variable-resolution ALL-PARTICLE**: a COARSE (cheap,
deformable) deep interior + a FINE mantle, all SPH-EOS particles — 63% Earth at 947 particles. **`sph_step.wgsl`
already does variable-resolution SPH-EOS + self-gravity + KDK** (per-particle `h`, `mass`, `mat` — the grid
`cell_size` is the max `h`, so mixed resolution just works). **So #3 needs NO new GPU kernel / no bulk-coupling
port** — just variable-resolution seeding, an ensemble harness, and a deterministic measurement. (The CPU
`Bulk`/`step_coupled`/`bake_back` machinery in `hydrostatic.rs` was the rigid-bulk path; it is NOT the GPU
plan — keep it as the CPU reference for the JIT cycle, but GPU #3 goes variable-res all-particle.)

## What exists to build on

- **`tools/impact-run`** — the offline GPU deformable-Earth impact (native Vulkan wgpu, runs `sph_step.wgsl`).
  `build_differentiated` (`main.rs:~100`) seeds a uniform-resolution iron-core+basalt-mantle body; `relax` +
  `impact` (KDK, adaptive Courant) run it; `measure_disk` + `moon_candidate` classify the disk by
  perigee-above-remnant and run `accretion`. This is the harness to extend. (`bench` mode was added for the
  frame-cost study — ignore it here.)
- **`shaders/sph_step.wgsl`** — the verified SPH-EOS-gravity kernel (density grid + Tillotson + Monaghan AV +
  **direct O(N²) self-gravity** + KDK). Verified by `tools/sph-verify`. Variable-`h`/`mass` already supported.
- **`crates/engine/src/accretion.rs`** — `find_clumps` + `accrete` (FoF, self-bound + Roche gate, conservative)
  — already demonstrated on a 35k disk (0.023 M☾ seed). Reuse for the Moon.
- **The CPU reference (docs/39):** `hydrostatic.rs::run_lod_impact` (the variable-res impact) gives the CPU
  f64 anchor — the GPU high-N result should be consistent with it within the scatter.

## The build (each step verified before the next)

1. **Variable-resolution seeding in `impact-run`.** Extend `build_differentiated` → coarse deep interior
   (large `mass`, large `h`) + fine mantle (small `mass`, small `h`), equal-mass WITHIN each zone. *Verify:*
   the seeded body RELAXES to hydrostatic on the GPU (`cs_relax`) and holds (like `run_lod_impact` does on
   CPU) — variable-res SPH can ring at the resolution boundary; confirm it settles. Match the CPU `run_lod_impact`
   fraction within scatter at the same N as a cross-check.
2. **Deterministic (order-independent) reduction.** The GPU is non-deterministic (atomic grid-insert order →
   f32 non-associativity → identical runs diverge over ~11 000 steps). The SIM will still scatter (chaos), but
   the MEASUREMENT must be reproducible: the disk-mass / provenance / remnant sums must be **order-independent**
   (sort particles before summing, or Kahan/pairwise, or fixed-point accumulation). *Verify:* the same particle
   snapshot measured twice gives the identical fraction to full precision.
3. **The ensemble.** Run K impacts with slightly PERTURBED initial conditions (jitter the seed offset / impact
   angle / a tiny position noise — vary by run index, NOT `Math.random`), each to the aftermath, measure the
   Earth-fraction + disk mass + largest accreted clump. *Verify:* report mean ± stdev over the ensemble — the
   stdev IS the scatter; the mean is the first converged number. K≥8 to start.
4. **Higher N (as far as direct-sum allows).** O(N²) gravity caps N (docs/37: N≈35k is minutes; BH was
   deferred as not worth it below ~128k). Push N as high as tractable per-run so the fraction stops drifting
   with N (a resolution check on top of the ensemble). If N is the wall, say so — the ensemble at the best
   affordable N is still the deliverable.
5. **The converged answer.** Report: the ensemble-mean Earth-fraction ± stdev (does the deformable-coarse-bulk
   variable-res Earth converge toward ~58%, or somewhere else?), and whether a bound Moon-mass clump reliably
   accretes across the ensemble. This closes #1's number and #2's resolved Moon.

## Gotchas

- **`sph_step.wgsl` gravity is direct O(N²)** — the wall on N. BH was measured not worth it below ~128k
  (docs/37); do NOT re-open that. The ensemble (many runs at affordable N) beats the scatter more cheaply than
  chasing raw N.
- **Variable-resolution SPH boundary artifacts** — the coarse↔fine interface can ring; the per-pair
  `h_ij=½(h_i+h_j)` helps but confirm the relaxed body holds (step 1).
- **Differentiated Theia is essential** — a dense iron impactor core ploughs in and lofts the mantle; a
  basalt-sphere Theia sheds ~0% Earth (docs/39 #1 setup lesson, docs/28 plough).
- **Standalone native-Vulkan crate** — keep GPU work in `tools/impact-run` (native wgpu), never add native-wgpu
  features to the engine crate (breaks its webgpu-only wasm build).
- **Rig-watch is not needed** (offline tool). `bash scripts/test.sh` for CPU; `cd tools/impact-run && cargo run
  --release -- <args>` for the GPU runs. **NEVER `cargo fmt`.** Work in the worktree.

## Definition of done

`tools/impact-run` runs an **ensemble** of variable-resolution deformable-Earth impacts on the GPU with an
**order-independent** disk measurement, and reports a **converged Earth-fraction (mean ± stdev)** + whether a
bound Moon-mass clump accretes across the ensemble — the first number, not a single scatter sample. Then #4
(terrain) is the only remaining thread, as the low-energy instance of the same primitive.
