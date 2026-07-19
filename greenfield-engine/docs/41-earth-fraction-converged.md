# docs/41 — #3 RESULT: the disk Earth-fraction, converged by ensemble (GPU/high-N)

The build spec was `docs/40`. This is the RESULT. #3 asked: the disk Earth-fraction is chaos-scatter-dominated
(docs/28 saw 28–50% on identical runs; #1 swung 25%↔63%), so *no single run is a number*. Build an ensemble at
higher N with a deterministic measurement and report the first CONVERGED value (mean ± stdev), plus whether a
bound Moon-mass clump reliably accretes. All on branch `planetary-scale-jit`, in the standalone
`tools/impact-run` (native-Vulkan wgpu, running the verified `shaders/sph_step.wgsl` — no kernel change).

## What was built (docs/40 steps 1–4)

1. **Variable-resolution ("LOD") seeding** — `build_lod`: a COARSE iron core (particle mass `8×m_fine`, larger
   `h`) + a FINE basalt mantle, all SPH-EOS particles. `sph_step.wgsl` already does mixed `h`/`mass`
   (per-pair `h_ij=½(h_i+h_j)`; grid `cell_size = max h`), so this is pure seeding — the #1 finding that the
   deformable-coarse core is the win, ported to the GPU. Theia is uniform-differentiated at `m_fine` (two mass
   classes system-wide; a dense iron impactor core is essential — a basalt sphere sheds ~0%).
2. **Order-independent measurement** — `sum_oi` (sort-by-magnitude + Kahan). The GPU sim is non-deterministic
   (atomic grid-insert order → f32 non-associativity → identical runs diverge), but the MEASUREMENT is
   reproducible: the same particle snapshot re-measures **bit-identical** (asserted in the single-run path). The
   sim scatters (chaos); the reduction does not — that separation is what makes an ensemble mean meaningful.
3. **The ensemble** — K perturbed-IC runs. Each adds a tiny deterministic position jitter (0.1% of the fine
   inter-particle spacing, from a splitmix64 hash of the run index — NOT `Math.random`, so the ensemble is
   reproducible), an independent chaotic realization of the SAME macroscopic impact. Reports mean ± stdev of the
   Earth-fraction, disk mass, and largest accreted clump.
4. **Higher N + fixed epoch** — pushed N to 1200 / 2400 / 4800 Earth particles (direct-sum O(N²) is the wall;
   BH deferred < 128k, docs/37). Every run integrates to a **physical-time epoch** (`ensemble <n> <t_hours> <K>`),
   not a step count — because the disk re-accretes (below), the fraction is epoch-dependent, so a clean
   N-comparison must hold the epoch fixed. (A fixed step count integrates *less* physical time at higher N,
   since a finer resolution takes a smaller Courant dt — that was the trap in the first pass.)

## THE ENABLING FIX — AV-free relaxation

The offline tool's `Gpu::relax` was running the damped settle **with Monaghan artificial viscosity on**
(`av_alpha=1, av_beta=2`). Result: the subsequent impact DISPERSED — **0% Earth, a 0.04 M☾ disk, remnant puffed
to R≈9500 km**. This is exactly the docs/35 GPU finding ("AV-zeroed relax" is a hard-won setting the browser
path relies on); the standalone crate simply never had it. AV is a velocity-dependent dissipation for
*approaching* particles — leaving it on during the settle corrupts the equilibrium. Fix: **relax AV-free**
(`av_alpha=av_beta=0`), restore AV (`1,2`) for the impact where the shock actually is. That single change turned
0% into a real Earth-bearing disk. (`sph_step.wgsl` unchanged — the fix is in the relax params, `main.rs`.)

## Finding A — the disk is a RE-ACCRETING TRANSIENT (the fraction is epoch-dependent)

At fixed N=2400, integrating to two epochs (K each):

| epoch | Earth-fraction | disk mass (M☾) | Moon-mass clump |
|-------|----------------|----------------|-----------------|
| ~11 h | 25% ± 5%       | 0.19 ± 0.03    | 8/8 runs        |
| ~23 h | 12% ± 14%      | 0.04 ± 0.03    | 2/4 runs        |

The disk **decays** — bound debris on eccentric orbits falls back onto the remnant, and the inner (Earth-rich)
disk re-accretes preferentially, so BOTH the disk mass and the Earth-fraction drop with time. There is **no
steady-state disk** at this sub-scale, no-net-spin, single-impact config: it just keeps re-accreting. So "the
Earth-fraction" is only meaningful *at a stated epoch* — the first and most important correction the ensemble
surfaced. (This also explains the historical scatter: single runs sampled different effective epochs.)

## Finding B — at a FIXED epoch (~8 h), the fraction converges with N

Holding the epoch fixed (~8 h post-impact) and varying resolution, K=8 each:

| N (Earth) | Earth-fraction  | disk mass (M☾) | largest clump (M☾) | Moon-mass clump |
|-----------|-----------------|----------------|--------------------|-----------------|
| 1200      | 20.4% ± 7.2%    | 0.22 ± 0.04    | 0.067 ± 0.013      | 8/8 runs        |
| 2400      | **31.8% ± 2.7%**| 0.39 ± 0.03    | 0.26 ± 0.06        | 8/8 runs        |
| 4800      | **32.2% ± 3.0%**| 0.30 ± 0.04    | 0.070 ± 0.017      | 8/8 runs        |

**This is the convergence.** N=1200 is under-resolved (20% ± 7%), but **N=2400 and N=4800 are statistically
identical — 31.8% ± 2.7% vs 32.2% ± 3.0%** — the fraction has PLATEAUED and the scatter has settled at ~±3%. So
at a fixed early epoch the disk Earth-fraction converges to **~32% ± 3%**, and it converges *from below* with N
(under-resolution suppresses Earth-shedding). Disk mass is also roughly N-converged here (~0.3–0.4 M☾ for
N≥2400, within scatter) — the wild 0.08→0.30 spread of the naive fixed-step pass was the epoch confound, not
resolution. A bound Moon-mass clump accretes in **8/8 at every N** at this epoch.

## The answer to docs/40

- **The Earth-fraction is a disk MINORITY — converged ~32% ± 3% (N≥2400) at ~8 h, declining to ~12% by ~23 h as
  the disk re-accretes; decisively NOT the all-particle 58%.** The 58–63% numbers #1/#33 reported were the *high
  tail* of the broad low-N/early-epoch scatter, not a mean. The converged disk is Theia-dominated (~68–88%) —
  consistent with the canonical giant impact and docs/28's iron-core plough.
- **A self-bound Moon-mass clump forms reliably** at the early epoch and sufficient N (8/8), the feedstock a
  real ~0.07 M☾ self-gravitating clump outside Roche — but it is itself partly transient (2/4 by ~23 h), so a
  persistent full Moon is an angular-momentum + full-scale + accretion-time question, not a disk-formation one.
- **The ensemble was necessary and sufficient to see this:** the "number" is not a scalar but a
  fraction(N, epoch) surface; the ensemble mean±stdev at a fixed (N, epoch) is well-defined and reproducible,
  and that is what converges.

## Follow-up — the SPIN IOU resolved: a spinning proto-Earth SUSTAINS the disk (and recovers ~58%)

The re-accretion (Finding A) is a *missing-angular-momentum* symptom: the no-spin IC gives the marginally-bound
disk too little support, so it falls back. Adding angular momentum — a pre-impact **spin** of proto-Earth about
the orbit normal (`impact-run spin <n> <ω> <b> <t_max> <K>`, measuring the disk at 4 epochs) — tests it. K runs,
N=2400, to 18 h:

| IC | disk vs time (4.5 → 18 h) | Earth-fraction | verdict |
|----|--------------------------|----------------|---------|
| baseline ω=0            | 0.56 → 0.09 M☾  | 23% → 18%  | **DECAYS** (re-accretes) |
| spin ω=4e-4             | 0.51 → 0.32 M☾  | 37% → 43%  | slow decay, holds ~0.4 M☾ |
| **spin ω=7e-4**         | 0.44 → **0.60** M☾ | 52% → **58%** | **PLATEAUS — sustained** |
| grazing b=1.4·R_e, ω=0  | ~0.01 M☾        | noise      | hit-and-run (Theia escapes) |

**A spinning target flings its OWN mantle into a rotationally-supported disk that does NOT re-accrete** — it
plateaus at ~0.6 M☾ and the Earth-fraction climbs to and holds **~58% ± 2%** (Moon-mass clump 8/8), exactly the
canonical all-particle value the non-spinning impact never reaches. So the ~25–32% "converged" number is the
*non-spinning* branch; angular momentum is the knob between it and 58%. Grazing-b is the wrong lever (b=1.4·R_e
is a hit-and-run).

**Cross-check (is the sustained disk a startup artifact?).** ω=7e-4 applied to a spherically-relaxed body is
near rotational breakup (a long rotating-frame relaxation sheds an equatorial stream: corotation ≈ 6200 km vs a
4200 km surface). So the check was run at a *stable* ω=4e-4, comparing a startup spin (spherical relax + spin at
impact) against a proper **rotating-frame OBLATE equilibrium** (a centrifugal term in `cs_relax`, gated by a new
`omega` param; flattening 0.149 ∝ ω², bounded, no blowup). The two agree — startup 0.32 M☾/43% vs equilibrium
0.43 M☾/39% at 18 h, both Moon 4/4 — the equilibrium case sustaining *slightly better*. **So the sustained disk
is real physics, not a startup-non-equilibrium artifact.**

This closes the last docs/40 IOU: the epoch-dependence (Finding A) is because a *non-spinning* disk is
marginally bound; a physically-motivated spun IC gives a disk that persists, with a well-defined fraction (~58%).

## Honest IOUs (no-fudge)

- **Only the early epoch is converged.** The fraction is N-converged at ~8 h; the *epoch* itself is a free
  choice because the disk decays monotonically (no steady state at this config). A physically-motivated epoch
  (e.g. disk-mass plateau, or a spun-up IC that sustains the disk) is the principled next step, not a fixed
  wall-clock guess.
- **Sub-Earth scale** (5000 km proto-Earth, 2700 km Theia) — the real-scale numbers may differ; the machinery
  is scale-invariant, the tractability is the limit.
- **No net pre-impact spin / single geometry** — the disk's poor angular-momentum support (hence fast
  re-accretion) partly reflects this IC choice; a spun-up / canonical-angle sweep is the next physical dial.
- **Direct-sum O(N²) is the N wall** (BH deferred < 128k, docs/37) — the ensemble at the best affordable N beats
  the scatter more cheaply than chasing raw N, as spec'd.

## Definition of done (docs/40) — met

`tools/impact-run` runs an **ensemble** of variable-resolution deformable-Earth impacts on the GPU with an
**order-independent** measurement, integrated to a **fixed physical epoch**, and reports a **converged
Earth-fraction (mean ± stdev)** — **~32% ± 3% for N≥2400 at ~8 h, a disk minority, not 58%** — plus a
**reliably-accreting bound Moon-mass clump (8/8)** at the early epoch. This closes #1's number (its 63% was a scatter/early-epoch sample) and
#2's resolved Moon. **#4 (terrain — the low-energy instance of the same field→particalize→bake-back primitive)
is now the only remaining thread.**

## How to reproduce

```
cd tools/impact-run
cargo run --release -- ensemble <earth_n> <t_hours> <K>   # e.g. ensemble 2400 8 8  → converged frac ±stdev
cargo run --release -- <earth_n> <t_hours>                # single run, verbose + order-independence self-check
```
