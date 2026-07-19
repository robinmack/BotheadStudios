# GPU Barnes–Hut — built, verified, and measured against direct-sum (docs/36 result)

This is the RESULT of the `docs/36` build spec. The GPU Barnes–Hut (LBVH) self-gravity solver is built and
**verified correct**; the performance measurement **disconfirms the docs/36 premise** that it restores
in-browser fps. Read this before wiring anything (docs/36 stage "Wiring into the engine").

## What was built — `tools/gpu-bh-verify` + `shaders/bh_gravity.wgsl`

A standalone native-Vulkan crate (same pattern as `sph-verify`/`impact-run`) hosting the full LBVH pipeline
as WGSL compute kernels, each verified against an independent CPU reference before the next is trusted:

| Stage | Kernel | Verified against | Result |
|------:|--------|------------------|--------|
| 0 | `cs_gravity_direct` | CPU f64 direct sum | RMS 2.4e-6 (f32) |
| 1 | `cs_bbox_reset`/`cs_bbox` (float-radix atomicMin/Max) | CPU min/max | **exact** (lossless u32 encoding) |
| 2 | `cs_morton` (30-bit) | CPU Morton | **bit-exact**; coincident→equal |
| 4 | `cs_tree_reset`/`cs_tree` (Karras 2012) | structural (reachability + parent/child) | every leaf reached once, pointers consistent |
| 5 | `cs_com` (atomic children-ready climb) | root Σm / Σm·x / AABB | mass 1.0e-8, COM 8.2e-8 — **atomic climb is coherent** |
| 6 | `cs_gravity_bh` (θ-traversal) | GPU direct-sum + CPU f64 direct | see below |

**Accuracy (the correctness bar, θ-traversal vs CPU f64 direct at N=1500):** θ=0.5 → RMS **0.70 %**, max 6.2 %;
θ=0.25 → 0.12 %; θ=0.1 → 0.009 %; **θ→0 → 1.8e-6 (recovers the exact direct sum)** — the strong structural
proof that every particle is reached exactly once and the COM/tree are correct.

Design notes that mattered: the opening criterion is the **robust (Salmon–Warren / Barnes 1994) MAC** using the
AABB **diagonal** as the node size plus the centre↔COM offset δ — a plain `maxside/dist<θ` on a *tight* box
under-opens and left a 28 % worst-case particle; the diagonal+δ form keeps the tight adaptive box (mandatory
for resolution, docs/36) AND caps the error. Traversal runs in **Morton order over a permuted `sbodies[]`** so
adjacent threads walk coherent paths and read contiguous memory (both the self-read and leaf-bucket sums
coalesce). **Leaf bucketing** (K particles/leaf) is parameterized (`bucket_k`; K=1 = classic LBVH).

The interim sort is a **CPU sort** (read back → sort (code,index) → upload cluster codes + sorted order +
sorted bodies). The GPU radix sort (docs/36 stage 3) was **not built** — see the recommendation.

## The measured finding — direct-sum wins below N≈128k (RTX 2070)

Per-eval GPU wall time of the **traversal** kernel vs the direct-sum kernel, θ=0.5, uniform cloud:

| N | BH ms (K=1) | direct ms | BH speedup |
|------:|------:|------:|------:|
| 2 000 | 0.63 | 0.40 | 0.63× |
| 8 000 | 1.81 | 1.62 | 0.89× |
| 32 000 | 9.9 | 8.5 | 0.86× |
| 128 000 | 40.1 | 86.3 | **2.15×** |

Asymptotic scaling (32k→128k) is exactly as theory predicts — **direct → O(N²) (p≈1.84), BH → O(N log N)
(p≈1.0)** — but the *crossover* where BH overtakes direct-sum is **N≈128 000**. Leaf bucketing (K=8/16/32)
does **not** lower it: buckets improve accuracy (larger exact leaf sums, RMS→6e-4) but cost more traversal
time (more per-thread leaf work), so K=1 has the lowest crossover. A 32- vs 64-deep traversal stack made no
difference (not register-spill-bound). The tree BUILD cost (bbox→morton→sort→tree→COM, every force eval) is
**on top** of the traversal number, so full-pipeline BH is even less favourable at low N.

**Why:** GPU direct N-body is the near-ideal GPU workload — every thread reads the same `bodies[j]` in
lockstep (broadcast), pure coalesced FMA, compute-bound, ~2070-peak FLOPS. Barnes–Hut trades those cheap FLOPs
for **divergent, memory-bound tree traversal** (per-thread stack walks, scattered 64-byte node fetches). On
this hardware the trade only pays once N² dwarfs the tree work — past ~128k. (Our direct-sum baseline is the
*naive* loop, not even shared-memory-tiled; a tiled direct sum would push the crossover *higher* still, so
this is a conservative, BH-favourable measurement.)

## Frame breakdown — where the time actually goes (2026-07-18, `impact-run bench`)

`cargo run --release -- bench` in `tools/impact-run` times each GPU pass of one force evaluation across N.
`cs_density` is pure O(N) grid work; `cs_forces` fuses the O(N²) gravity with the O(N) grid pressure. Gravity
figures below are the *true* gravity-only kernel from `gpu-bh-verify` (the `forces−density` estimate in the
bench output over-counts, because the pressure loop is heavier than density). RTX 2070:

| N | force_eval ms | gravity ms (share) | grid+pressure ms | physics fps | real fps* |
|------:|------:|------:|------:|------:|------:|
| 2 000 | 2.17 | ~0.4 (~20 %) | ~1.8 | 28 | ~11 (obs) |
| 8 000 | 4.61 | ~1.6 (~35 %) | ~3.0 | 13 | ~4 (obs) |
| 16 000 | 6.88 | ~3.3 (~48 %) | ~3.6 | 9 | ~3 |
| 32 000 | 16.1 | ~8.5 (~53 %) | ~7.6 | 3.9 | ~1.3 |
| 64 000 | 50.0 | ~30 (~60 %) | ~20 | 1.2 | ~0.4 |
| 128 000 | 195.8 | ~86 (~44 %) | ~110 | 0.3 | ~0.1 |
| 256 000 | 700.2 | ~340 (~49 %) | ~360 | 0.1 | ~0.03 |

\*`real fps` = physics-only × ~0.3 (render + the per-frame HUD read-back + WebGPU overhead), calibrated to the
two observed browser points (2.8k→11 fps, 8.2k→4 fps, docs/36) — both land. `physics fps` assumes 8 KDK
substeps × 2 force evals = 16 evals/frame (`start_gpu_impact`).

**Three takeaways (two correct an earlier inference):**
1. **Gravity is a bigger slice than first estimated** — not ~25 % at 8k but **~35 %, rising through ~50 % by
   32k**. It IS roughly half the physics cost across the browser range, so the earlier "gravity isn't the
   lever" was too strong.
2. **But only about half.** The SPH grid+pressure passes are a co-equal cost at every N — so even *free*
   gravity only ~doubles physics fps. BH (which only touches gravity, and only wins past 128k) can't move this.
3. **The grid itself goes super-linear at high N** — `density` grows 9× for 4×N past 64k because `TABLE_SIZE`
   is fixed (65536 cells) and a compact body over-packs them: a *second* O(N²)-ish wall, independent of
   gravity. (A spread-out debris field stresses it less, so this is a worst case.)

**Practical ceilings (2070):** interactive is **~12–15k** (~3–4 fps); quadrupling the N=2.8k button → ~11k
lands ~3–4 fps (richer disk, choppy — matches docs/36's "gorgeous but choppy at 8k"). Past ~30k is offline-only
(~1 fps).

## Cheaper levers for more particles (no GPU sort needed)

If the goal is simply *more particles interactively on the 2070*, these beat BH and need no new infrastructure:
- **Fewer KDK substeps/frame** (the frame is ~16 force-evals — the single biggest multiplier).
- **Grow `TABLE_SIZE` with N** so the grid stays O(N) instead of saturating (fixes takeaway #3).
- **Lighter / less-frequent HUD read-back** (the per-frame GPU→CPU disk-stats read is a big part of the
  ~0.3× real-vs-physics gap).

These are the recommended first moves if particle count needs to go up before any high-N campaign.

## Hardware dependence — the 2070 is the WORST case for this decision

The whole finding is hardware-specific; on the parts we might target it shifts, in BH's favour:
- **Unified memory (Apple M4 / A18, Snapdragon/Adreno) flips the CPU/GPU split.** CPU and GPU share memory
  zero-copy, so a **CPU-Barnes–Hut + GPU-SPH hybrid becomes realtime-viable** — compute long-range gravity on
  the CPU with `bhtree.rs` (already O(N log N), already written) and hand accelerations to the GPU with no
  copy. On our 2070 (discrete, PCIe 3.0) that per-substep round-trip is too expensive → offline-only. **On
  unified-memory hardware the split needs zero new GPU code.**
- **The BH-vs-direct crossover likely drops** on cache-rich / lower-FLOPS GPUs: direct-sum is FLOPS-bound,
  BH-traversal is memory/divergence-bound; Apple's large SLC + TBDR handle divergence relatively better and
  mobile parts have fewer raw FLOPS (A18 ~2–3, base M4 ~4, vs 2070 ~7.5 TFLOPS f32) — both push the crossover
  **down**, plausibly into 30–60k (unmeasured — needs a run on that silicon).
- **Caveat the other way:** mobile GPUs have smaller register files and BH's per-thread traversal stack is
  register-heavy → a naive port could stall; a warp-cooperative traversal matters more there than on desktop.

## Recommendation (no-fudge: the measurement, not the hypothesis)

**Do NOT wire GPU Barnes–Hut into the browser SPH step (docs/36 "Wiring" / stage 8).** The browser impact
runs at N≤~20k and the offline `tools/impact-run` converges at N≈35k — **both are far below the 128k
crossover**, so BH would *reduce* fps there, not restore it. The docs/36 premise ("restores fps at 8k, unlocks
20k") is disconfirmed for a per-thread GPU traversal on the 2070. Keep the direct O(N²) gravity for N≤~100k;
it is the right tool in that range.

Per the frame breakdown, gravity is ~half the physics cost across the browser range (not the ~25 % first
inferred) — but the SPH grid+pressure is the co-equal other half, so **even free gravity only ~doubles physics
fps, and BH doesn't win below 128k anyway.** The genuine levers for interactive N are elsewhere (substeps,
`TABLE_SIZE`, read-back — see "Cheaper levers" above).

**Where BH IS the right tool: very-high-N offline convergence (N≳128k).** The disk isotopic fraction is
scatter/relaxation-noise-limited (docs/28 ceiling, docs/35) and wants an ensemble at higher N. At N≳128k, BH
gives a real and growing speedup on the *gravity* component (2.15× at 128k; extrapolating O(N log N) vs O(N²):
~9× at 512k) — though at that scale the grid also needs a bigger `TABLE_SIZE` to stay O(N).

## DECISION (2026-07-18): option B — defer

Robin chose **B**: keep the direct O(N²) gravity everywhere (browser + offline); do **not** wire GPU
Barnes–Hut, and do **not** build the GPU radix sort yet. Direct-sum is the correct tool for every N we
currently target, and the GPU sort is the most expensive remaining kernel with no near-term payoff. The
verified BH crate (`tools/gpu-bh-verify`) + `shaders/bh_gravity.wgsl` are **banked and re-verifiable**; this
document is the careful write-up so a later revisit is an on-ramp, not a re-derivation.

### When to revisit A — triggers

Pick up option A if/when **either**:
1. **A high-N offline convergence campaign is scheduled** (N≳128k, GPU-only, to pin the isotopic fraction with
   an ensemble). BH makes the O(N²) gravity tractable there.
2. **An Apple/mobile (unified-memory) target appears.** First reach for the *cheap* win — the CPU-`bhtree.rs`
   + GPU-SPH realtime hybrid (zero new GPU code, viable because the CPU↔GPU copy is free there). Only build
   GPU-BH if you specifically want the CPU out of the gravity loop.

### Resume plan (what's left, and how)

- **Re-verify the banked work:** `cd tools/gpu-bh-verify && cargo run --release` (native Vulkan) still prints
  PASS for every stage — the tree is correct and ready to reuse.
- **The one remaining kernel is the GPU radix sort** (docs/36 stage 3). Frame it as **"build the reusable GPU
  key-sort,"** not "finish BH" — a working GPU sort also unblocks GPU-side **accretion** (stream compaction,
  currently CPU-after-readback in `accretion.rs`) and **grid reordering** (cache-friendlier SPH passes). BH is
  just its first customer. The interim CPU sort in `gpu-bh-verify` shows exactly what it must produce
  (non-decreasing cluster codes + a bijective permutation + sorted bodies).
- **Also grow `TABLE_SIZE` with N** for any high-N run, or the grid becomes the new O(N²) wall (frame
  breakdown, takeaway #3).
- **Measurement tooling exists:** `impact-run bench` for the per-pass frame sweep; re-run on new hardware to
  re-locate the crossover (expected lower on cache-rich / lower-FLOPS GPUs).

## Reproduce

`cd tools/gpu-bh-verify && cargo run --release` (RTX 2070, native Vulkan). Prints PASS for every stage +
the scaling/bucketing table. Correctness bars: bbox exact, morton bit-exact, tree structural, COM <1e-6,
θ=0.5 RMS <1 %, θ→0 recovers direct, direct-sum → O(N²).
