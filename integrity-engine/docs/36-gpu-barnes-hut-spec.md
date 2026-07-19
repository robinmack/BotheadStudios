# GPU Barnes–Hut tree — build spec for a fresh session

A self-contained hand-off so the next session builds a GPU Barnes–Hut gravity solver without re-deriving.
Read `CLAUDE.md`, `docs/32-architecture-map.md`, `docs/35-gpu-path-migration.md`, and the JOURNAL entries dated
2026-07-17 (the GPU-impact "lost orbits" investigation → solution) first; this doc is the concrete task.

## Why (the measured motivation — don't re-litigate it)

The in-browser GPU SPH giant impact WORKS (docs/35): energy-conserving, forms a remnant + orbiting disk. Its
gravity is **direct O(N²)** (`sph_step.wgsl` `cs_forces`, the `long-range gravity` loop at
`shaders/sph_step.wgsl:161`). That caps N for interactivity — MEASURED on the RTX 2070:

| N     | fps  | note                                             |
|------:|-----:|--------------------------------------------------|
| 2 800 | ~11  | the current button default (`build_far_apart(2400,400)`) |
| 8 200 | **4**| gorgeous disk, energy ΔE≈0.08 %, but choppy      |

The O(N²) *dynamics* (20 substeps × 2 force evals × N²/frame) is the wall. The offline `tools/impact-run`
converges the disk at N≈35 000 — unreachable in-browser with direct sum. **Barnes–Hut (O(N log N)) is the fix**:
restores fps at 8 k and unlocks N ≳ 20 k for a sharp, converged in-browser disk. This is the agreed next step.

## What exists to match / reuse

- **CPU reference — `crates/engine/src/bhtree.rs`** (`BarnesHut`). Octree, monopole COM, Plummer softening,
  opening criterion **`(2·half)/dist < theta` → use the node COM, else descend** (`bhtree.rs:181`),
  `r² = |d|² + soft²` (`:179`). Verified test `barnes_hut_matches_brute_force_within_theta_bound`
  (`bhtree.rs:206`). The GPU tree must reproduce this to the θ tolerance (NOT to f32 precision — BH is a
  multipole APPROXIMATION; θ=0.5 gives a few-% error, which is correct, not a bug).
- **The gravity to replace** — `sph_step.wgsl:161-167` (the direct O(N²) loop inside `cs_forces`). The SPH
  pressure+AV loop below it (`:168+`) STAYS direct-on-the-grid (it's short-range, already O(N)).
- **Verify harness pattern — `tools/sph-verify`** (standalone native Vulkan-wgpu crate; the engine wgpu is
  webgpu-only, can't run native Vulkan — keep GPU verification in a standalone crate). Mirror its
  device/pipeline/buffer/readback setup (`run_gpu` in `tools/sph-verify/src/main.rs`).
- **Consumer — `crates/engine/src/gpu_sph.rs` `GpuSph`** (the browser host of `sph_step.wgsl`). Bind group is
  bindings 0–7 (`gpu_sph.rs` `make_buffers` / the layout in `GpuSph::new` ~`:400`). Grid consts
  `SPH_TABLE_SIZE=1<<15`, `SPH_BUCKET_K=128` (`gpu_sph.rs:22`). The impact runs via a phase machine in
  `OrbitDemo::advance` (`lib.rs:3315`, `enum SphPhase` at `lib.rs:2393`): `Relaxing → Assembling → Dynamics`.

## The build, staged — each stage VERIFIED before the next (verify-before-wire, docs/30)

Build in a NEW standalone crate `tools/gpu-bh-verify` with a new `shaders/bh_gravity.wgsl`, so you iterate
natively (fast — `cargo run --release`, no wasm/browser round-trip) and never touch the working `sph_step.wgsl`
until the tree is proven. Verify **GPU-BH vs GPU-direct** (both f32 → the difference is purely the θ multipole
error) AND spot-check vs the CPU `bhtree.rs`.

Recommended structure: **LBVH (Karras)** — the standard, and its bottom-up COM needs NO float atomics (WGSL
lacks them; see gotcha). Kernels, in dependency order:

1. **Adaptive bounding box** (`cs_bbox_reset`, `cs_bbox`). GPU reduction of min/max position. WGSL atomics are
   integer-only → use the float-radix ORDER trick: encode a float `f` to a monotonic u32 key
   `k = bitcast<u32>(f); k = select(k ^ 0x80000000u, ~k, (k>>31)==1u)` then `atomicMin/Max` on `k`, decode
   with the inverse. A TIGHT box is essential — a fixed generous box quantises the compact ~5000 km remnant
   into ~1 Morton cell and BH degenerates to direct. *Verify:* min/max match a CPU pass.
2. **Morton codes** (`cs_morton`). 30-bit (10 bits/axis): map pos→[0,1) by the bbox, quantise to [0,1023],
   interleave bits (`expandBits`). Output `(code:u32, index:u32)` per particle. *Verify:* codes match a CPU
   reference; equal for coincident points.
3. **GPU radix sort** (`cs_hist`, `cs_scan`, `cs_scatter` — the meaty part). LSD, 4 passes × 8 bits, sorting
   the `(code,index)` pairs by code. Integer atomics for the per-workgroup histograms; a prefix-scan (Blelloch
   or a simple global scan for a first version) for the offsets; scatter. *Verify:* output codes are
   non-decreasing; the index permutation is a bijection. (A correct GPU radix sort is the single hardest
   kernel — get it standalone-green before moving on. A CPU-sort fallback is fine to unblock stages 4–6, but
   the browser NEEDS the GPU sort since positions change every step and can't be read back per step.)
4. **Karras binary radix tree** (`cs_tree`). From the sorted codes build 2N−1 nodes (N leaves + N−1 internal):
   per internal node, the `determineRange` + `findSplit` from Karras 2012 (delta = longest common Morton
   prefix; ties broken by index). Store per node: children (or leaf range), parent, and an AABB/half-size.
   *Verify:* every leaf reachable from the root exactly once; parent/child pointers consistent.
5. **Bottom-up COM** (`cs_com`). Each leaf's COM = its particle. Internal node COM = mass-weighted merge of its
   two children — done bottom-up via an ATOMIC COUNTER climb (each node atomically increments its parent's
   "children ready" counter; the thread that arrives second computes the parent, then climbs). No float
   atomics needed (each merge is a single-threaded 2-child combine). *Verify:* root mass == Σ mass; root COM ==
   Σ m·x / Σ m.
6. **θ-traversal gravity** (`cs_gravity_bh`). Per particle, an explicit stack walk from the root: pop node; if
   `(2·half)/dist < theta` (or it's a leaf) → add the node's monopole `G·M·d/(|d|²+soft²)^1.5`; else push its
   two children. Also implement `cs_gravity_direct` (extract from `cs_forces:161-167`) as the reference.
   *Verify:* RMS(BH − direct)/RMS(direct) ≈ the θ error — target **< 1 %** at θ=0.5, → 0 as θ→0. Sweep N and
   θ. Confirm it's genuinely O(N log N) (time vs N).

## Wiring into the engine (only after stage 6 is green)

- **Split `cs_forces`** in `sph_step.wgsl` into `cs_gravity` (the O(N²) loop, kept as a fallback / the verify
  reference) + `cs_pressure` (the SPH pressure+AV+du/dt, `:168+`). Add the BH kernels + buffers (bindings 8+;
  note bindings 0–7 are taken — `sph_step.wgsl:47-54`). A force eval becomes: bbox → morton → sort → tree →
  com → `cs_gravity_bh` (writes `acc`) → `cs_pressure` (adds to `acc`). Keep the direct path selectable for
  the `sph-verify` regression.
- **`GpuSph`** (`gpu_sph.rs`): add the BH buffers to `make_buffers`/the bind group, and dispatch the BH build
  before the pressure pass in `force_eval` (`gpu_sph.rs` `force_eval`, used by both `encode_relax` and
  `encode_kdk`). Re-run `tools/sph-verify` — the full step must still match the CPU.
- **Bump N** in `OrbitDemo::start_gpu_impact` (`lib.rs:3263`, currently `build_far_apart(2400, 400)`) toward
  ~10–20 k and rig-watch fps + the disk (`web/rig/sph_energy.mjs` logs ΔE / remnant / disk / escaped / moon).
  Target: N≈10 k at ≳20 fps with the disk sharper than the N=2800 baseline.

## Gotchas (READ before coding)

- **No float atomics in WGSL.** Drove the LBVH choice (atomic-free bottom-up COM). Any COM-by-scatter needs
  fixed-point (overflows for coarse cells) or CAS loops (slow) — avoid; use the tree reduction.
- **Tight adaptive bbox is mandatory** (see stage 1) — the whole point is resolution inside the compact
  remnant.
- **Verify to the θ tolerance, not f32.** BH ≠ direct by design; ~1 % at θ=0.5 is correct.
- **Standalone Vulkan crate for verification** — do NOT add native-wgpu features to the engine crate (breaks
  its webgpu-only wasm build via feature unification). Same reason `sph-verify`/`impact-run` are standalone.
- **The impact's other hard-won settings must survive the swap** (docs/35, don't regress them): the relax runs
  with **AV zeroed** (`GpuSph::set_av(0,0)` in `start_gpu_impact`; restored to (1,2) at the Dynamics
  transition — AV stiffens the relax and diverges it); the two bodies relax **far apart** (`build_far_apart`,
  40× contact, so each self-gravitates without mutual pull); the dynamics dt is a **fixed shock-safe value**
  (`assemble_from_relaxed`, `0.05·min_h/(20000+v_esc)`) that MEASURED conserves energy to ~0.01 %. BH changes
  only *how gravity is summed*, not these.
- **Positions change every substep** → the tree rebuilds every force eval (both relax and dynamics). The whole
  pipeline (bbox…traversal) is per-eval. Budget for that (it's still O(N log N) ≪ O(N²)).
- Workflow: `bash scripts/test.sh [--fast] [filter]`; GPU verify `cd tools/gpu-bh-verify && cargo run
  --release`; browser rig `xvfb-run -a node web/rig/sph_energy.mjs` (dev server on a fresh port, e.g.
  `npx vite --port 5307 --strictPort`; use `wasm:release` — dev-wasm relax is ~10× slower). **NEVER
  `cargo fmt`.** Work in the worktree, never the main checkout.

## Definition of done

`tools/gpu-bh-verify` prints PASS (GPU-BH matches GPU-direct within the θ bound, and O(N log N) scaling
confirmed); the BH gravity is wired into `sph_step.wgsl`/`GpuSph` with `tools/sph-verify` still green; the
browser GPU impact runs at N≈10 k (or higher) at interactive fps with a sharper disk (rig-watched, energy
still conserved). Then the residual-escape trim (an in-kernel per-substep adaptive dt) and re-promoting the
GPU impact to the birth scene (docs/35 step 2, deliberate + deploy) remain.
