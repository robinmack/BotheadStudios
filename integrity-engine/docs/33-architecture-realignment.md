# Architecture realignment to first principles (written 2026-07-17, status refreshed 2026-07-19)

A reexamination of Integrity's principles and a staged plan to realign the architecture to them. Read
[`docs/32-architecture-map.md`](32-architecture-map.md) first — this doc assumes that map. Robin has
green-lit a **large, staged refactor**; the discipline is correctness-first, each increment verified against
an exact/analytic reference before the next (never a big-bang commit).

## Status (2026-07-17) — stages 1–3 DONE

The physics core of the realignment is built and verified (all in `eos.rs` + `hydrostatic.rs`, `#[ignore]`
measurement tests; see JOURNAL for numbers):
- **1 — Tillotson EOS** ✅ (basalt verified vs Benz & Asphaug 1999; iron compressed branch vs Wissing &
  Hobbs 2020; granite/dunite vapor branch flagged provisional).
- **2a — self-gravitating single-material body** ✅ (holds hydrostatic balance, dP/dr=−ρg to rel 0.00–0.01).
- **2b — differentiated iron-core + basalt-mantle Earth** ✅ (equal-mass + adaptive-h SPH, the Genda fix;
  compresses, stratifies, central P 572 GPa ≈ Earth's 364).
- **3a — dynamical SPH** ✅ (internal-energy equation + Monaghan artificial viscosity + KDK leapfrog +
  adaptive Courant dt; a relaxed collision conserves energy to ~3% and shock-heats).
- **3c — the deformable-Earth impact** ✅ → **orbiting disk 58% Earth-derived**, vs the rigid-boundary
  7–12% ceiling (docs/31). docs/28 root-cause #1 dissolved: Earth sheds its own mantle into the disk.
  (Sub-Earth scale + coarse N — the DIRECTION, not a converged number.)

**Stage 4a+4b DONE (2026-07-17):** the GPU force kernel `shaders/sph_step.wgsl` (SPH density + Tillotson +
Monaghan AV + direct self-gravity + du/dt) is **verified on the RTX 2070** (`tools/sph-verify`) against an
independent f64 CPU computation — RMS rel error 1.9e-6 (f32 precision). 4b added the **spatial-hash neighbour
grid** for the short-range SPH (O(N) not O(N²)), verified exact via a cell-membership guard (defeats
hash-collision double-counting). Gravity stays direct O(N²) (a GPU tree is a later opt). Ahead: 4c (KDK loop
+ adaptive dt on-GPU + scene wiring, with the accretion operator).

**Remaining:** 4c (KDK loop + adaptive dt on-GPU + scene wiring), 5 (unify containers —
fold `hydrostatic`/`AirField` into `Aggregate`), 6 (energy-tiered just-in-time particalization). The
capability currently lives in the standalone `hydrostatic.rs`; it is NOT yet wired into the wasm scene
(stage 4/5 work) — the deployed birth scene is still the pre-realignment `OrbitDemo`.

**Stage 4 also needs an ACCRETION operator (new, 2026-07-17).** Diagnosis of "the disk never accretes a
Moon": at the scene N (~1536 chunks) the disk is collisionless (docs/28 ceiling) AND there is no
fusion/growth operator at all — particle masses never grow, so a bound clump renders as a cluster of
471-km balls, never a sphere. Higher N (stage 4) is necessary but NOT sufficient: a near-spherical Moon
also needs a **coarse-grained accretion law** — a gravitationally-bound rubble clump promoted to a single
body with a grown radius, honestly (mass/momentum/energy conserved). Pair this with stage 4. (Roche physics
now exists: `tides::secular_step` shreds sub-Roche moonlets — the geologic "ball on the surface" bug is
fixed — but that is the aftermath; the live-disk accretion operator is the missing piece.) WGSL kernel
`shaders/sph_step.wgsl` is written (unverified) as the stage-4 GPU stepper start.

## Status update (2026-07-19) — what landed, and the pattern running through it

Two days after this plan was written, docs/34–48 and ~20 commits landed. The stages below are not rewritten;
this records where each actually stands. The honest summary is not "stage N is done" — it is that **the
physics keeps getting built and verified, then wired into one place or none.** docs/48 named that pattern
after finding a third instance; two more are visible here, so treat "does anything call this?" as a
first-class question when picking up any stage.

- **Stage 4c — largely landed.** `gpu_sph.rs` (710) is real and **is** wired: `OrbitDemo` owns a `GpuSph`
  (`lib.rs:2828`, ~11 call sites) driving `shaders/sph_step.wgsl` in-browser, with `sph_render.wgsl` drawing
  the physics buffer zero-copy. Caveat (narrowed 2026-07-20): its PHYSICS still lives entirely in
  `tools/sph-verify`, so behavioural changes are unguarded unless that tool is run — but the module is no
  longer wasm-only, so its shader-facing LAYOUTS are now pinned in-crate. That gate turned out to rest on
  a single `Rc<Cell<bool>>`, and hid a real drift: the shader's `omega` was mirrored as `_p0`.
- **GPU Barnes–Hut — built, verified, NOT wired.** `bh_gravity.wgsl` (317) + `tools/gpu-bh-verify`
  (docs/36/37). Its own header states it was verified standalone *"before it is wired into the SPH step"* —
  `sph_step.wgsl` still runs the direct O(N²) gravity loop. The O(N log N) win is sitting on the shelf.
- **The accretion operator — half wired, and the unwired half is the one the diagnosis called for.**
  `accretion.rs` (298): `find_clumps` is called from `gpu_sph.rs` (`:266/:315/:357/:412`) for moonlet
  detection, but **`accrete()` has no non-test caller**. The disk can be *measured* for gravitationally
  bound clumps and still cannot *grow* one — which is exactly the "the disk never accretes a Moon"
  diagnosis recorded above. The operator now exists; it is still not in the loop.
- **Stage 6 sub-stage — substrate landed, trigger open.** Commit `3d840ac` gave T0 a real persistent field:
  `World.displacement` (`world.rs:82`), `demote_column_to_field` (`:271`), `column_is_bakeable` (`:302`),
  `World::from_voxels` (`:89`) — see docs/32 §3. **Nothing calls demotion in production**; both new
  functions appear only in tests. Quiescence trigger, per-region tracking, and the bump/normal map remain
  open. That commit also contributes the admission rule this plan lacked: *promotion asks whether the cheap
  model provably **differs**; demotion asks whether it can **represent** the state.*
- **Stage 5 — unchanged, but one input got sharper.** `AirField` is still a standalone fork, and docs/48
  established it is instantiated in **zero scenes**. Note too that the drag interaction docs/26 verified
  does **not** live in `AirField`: `atmosphere.rs:502` builds an `Aggregate` of body + air parcels with a
  `gas_contact_from_material` `Contact` and lets `granular::contact_accel` produce the drag. So folding
  `AirField` into `Aggregate` is closer to deleting a duplicate than to porting a capability.

**Where the newer docs refine this plan:** docs/44 (resolution by necessity) and docs/47 (granularity) are
the concrete form of **P3** — they size the *extent* and the *fineness* of resolution from the interaction's
own energy rather than from scene identity. docs/46 is the charter and conformance ledger that decides
whether any fork here is legitimate specialization, a declared IOU, or a fudge; its §3 ledger is the live
list this plan's §2 table was an early draft of. docs/48 is the atmosphere's status.

## 1. The principles (restated)

From the existing design record (docs/13/15/16/17/23/24) and Robin's three framings (2026-07-16/17):

- **P1 — Everything is matter; nothing is bespoke.** Every object in every scene is a natural product of
  the same real physics calculations. No per-scene code paths, no scripted outcomes, no tuned dials (docs/23,
  the no-fudge charter). "A tire on the ground, a small meteor impact, and Theia are the same physics — just
  a different scale."
- **P2 — Material physics scalable.** ONE set of material laws — contact, gravity, and *pressure* (an
  equation of state spanning solid → liquid → vapor) — parameterized by material and valid from human scale
  to planetary scale. One module, applied everywhere (the unified-particle-module directive).
- **P3 — Calculations tiered on energy scale.** The *fidelity and cost* of a physics interaction are chosen
  by its **energy** (equivalently, by local energy density / stress vs the material's own thresholds), not by
  which scene it belongs to. A resting tire uses cheap quasi-static contact; a giant impact escalates to full
  EOS + shock + vaporization — automatically, where and when the energy demands it. This generalizes docs/08
  (adaptive resolution) and docs/13 (scale-relative simulation) from *spatial* LOD to **energy-tiered
  physics**, with the awake-set (docs/16) as the promote/demote mechanism.

## 2. Where the architecture diverges from the principles

From the map (docs/32 §4–§5), the laws are already impressively unified — but the infrastructure violates
all three principles in specific, nameable ways:

| Divergence | Violates | Evidence |
|---|---|---|
| **Two container universes** — `Aggregate` (Vec\<Body\>, f64, celestial) vs voxel `World`+`MassField`+`MatterSim` (f32, terrain) | P1, P2 | docs/32 §4.1 — a tire and Theia live in different data structures |
| **Four integrators over one contact law** — GPU trapezoidal, CPU Euler settle-only, CPU Verlet/KDK, SPH relaxation | P1, P2 | docs/32 §4.2 |
| **Rigid-boundary fork** — in an impact Earth is *both* a few grains AND a rigid penalty sphere + monopole gravity; its bulk is not the same matter as its debris | P1 | docs/28 root cause #1; `aggregate.rs` boundary; docs/31 (blocks the isotopic-crisis fix) |
| ~~**No condensed-matter EOS**~~ → **EOS built, wired only in the space band** — `eos.rs` implements Tillotson (verified); it reaches `hydrostatic.rs` and `gpu_sph.rs`, but the terrain/voxel/granular path still resists compression by linear-elastic contact penalty alone, and planet layer densities are still declared PREM constants | P2 | docs/32 §5 (rewritten 2026-07-19 — it previously said "confirmed absent", which was false) |
| **Fidelity chosen by scene, not energy** — GPU-granular for terrain, CPU-full-physics for space; the declared `Furrow::ejection` / `plough_loft` IOUs stand in for the high-energy tier because N is too low | P3 | `impact.rs:88` (the resolution IOU); the GPU path has no self-gravity/SPH/EOS |
| **GPU/CPU depth split + hand-mirrored WGSL law** | P1, P2 | docs/32 §4.5–§4.6 |

## 3. The target architecture

**One particle/material engine** that every scene drives, with scale, energy, and material as *parameters*:

- **One container.** A single particle representation (`Body` cloud) is the ground truth for all dynamic
  matter — terrain grains, debris, and a planet's own bulk. Bulk/summary forms (heightfield, monopole
  gravity, analytic hydrostatic body) are the *coarse energy tier* of that same matter, not a separate
  universe (§ P3). The rigid boundary dissolves: a planet is particles, coarsened where nothing is happening.
- **One pressure law (the EOS).** A condensed-matter equation of state — **Tillotson** (the giant-impact
  standard; cited per-material params for dunite/peridotite, basalt, iron) — gives `P(ρ, u)` across cold,
  compressed, expanded, and vapor states in one form. It *replaces* the ideal-gas-vapor + linear-elastic-
  penalty + declared-PREM-density patchwork (§ P2). Density responds to pressure; shock-compressed rock
  develops real pressure and heats by PdV work.
- **One stepper, energy-tiered.** The SPH/contact/self-gravity force kernel is the same everywhere; the
  *tier of fidelity* applied to a region is selected by its energy density vs the material's own thresholds
  (fracture → melt → vaporize — already in `damage::classify`). Promotion/demotion between tiers is the
  awake-set (docs/16) driven by energy, not scene. GPU-resident (f32, body-relative frame) for the high-N
  regimes the physics demands (docs/22 step 4).

**The energy tiers** (a region sits in the lowest tier its energy density permits, promotes when excited,
demotes when it settles — matter/momentum/energy conserved across every transition):

- **T0 — Bulk summary, rendered as texture/bumpmap.** Undisturbed matter: a heightfield/**displacement +
  normal (bump) map** surface, monopole/Gauss gravity, analytic hydrostatic body. Cheapest AND best-looking
  — textures are a legitimate, *visually rich* summary of settled matter (they speed us up and look great),
  and are honest **as long as the displacement/normal they encode is the real settled height/deformation,
  not a painted-on decal**. (Earth far from the impact; resting terrain far from a dig; a crater floor after
  the debris has come to rest.)
- **T1 — Quasi-static contact.** Low energy: grains at rest, a tire, talus, a settled disk. Granular contact
  + self-gravity; no shock/EOS heat term active.
- **T2 — Dynamic granular + thermal.** Medium energy: cratering, ejecta curtains, meteor impacts. Contact +
  energy routing + phase classification.
- **T3 — Full EOS shock + vapor.** High energy: giant impacts. Condensed-matter EOS pressure, SPH, PdV
  vaporization, shock heating — the whole thermodynamic engine, only where the energy is.

**Just-in-time particalization (the promote/demote mechanism, docs/16 awake-set made concrete).** We do NOT
simulate full particles everywhere — that is both unaffordable and looks like Minecraft. Instead matter
lives at T0 (textured/bumpmapped bulk) until an *event* demands more, then **particalizes just-in-time**:

- **Predictive promotion.** On an impact/event, spawn real particles *before* contact (look ahead — the
  space band already has `orbit::swept_first_contact` + an impact countdown; the terrain band raycasts the
  meteor aim), so the shock is resolved from first contact, not a frame late.
- **Energy-scaled resolution.** The particle resolution (grain size / count / tier T1–T3) is chosen by the
  **energy, mass, and size** of the interaction — fidelity to the physics of *that* event, not a fixed N.
- **Bake-back on settle.** When particles quiesce they demote back into the T0 field, writing their real
  settled state into the **displacement + normal map** (matter/momentum/energy conserved, via the existing
  `matter::deposit_resting_grain` discipline — one grain → its real resting height, zero injected energy).
  The crater/deformation persists as texture; the particles are freed.

This is the particle↔field duality of an MPM-style scheme (docs/08 names MLS-MPM as the Phase-3 target): the
resting state is the field, matter particalizes where energy/deformation is high, and returns to the field
when quiescent. It is how we get *both* great visuals and real physics without computing everything all the
time — and it is faithful precisely because the field is a real record of settled matter and the
particalization resolution is set by the event's physics. (If research surfaces a closure that is *more*
faithful at equal cost, it supersedes this — the commitment is to the physics, not to this mechanism.)

The declared IOUs (`Furrow::ejection`, `plough_loft`) are today's stand-in for T3 at low N; as the
energy-tiered engine resolves the shock where it matters, those IOUs retire exactly as docs/28 promised.

## 4. Staged plan (correctness-first; each stage verified before the next)

Physics-faithful ordering: get the hard physics right at tractable N on CPU with TDD, then scale on GPU
(per the physics-faithfulness directive). Full-particle-Earth is milestones 2–3.

1. **Condensed-matter EOS (Tillotson).** New module `eos.rs` + cited per-material params (a `tillotson`
   block alongside `Material.thermal`, sourced like the rest of `data/materials.json`). TDD: at ρ=ρ0, cold →
   P≈0; small compression → the material's real bulk modulus; a cited Hugoniot shock point matches; smooth
   across the compressed→expanded→vapor branches. *Isolated, cheap, fully unit-testable — the foundation.*
2. **Self-gravitating EOS planet (the MERGE, moderate N, CPU).** Reuse `atmosphere.rs`'s verified SPH
   pressure kernel + `bhtree.rs` self-gravity + `aggregate.rs::apply_thermo` energy equation, swapping the
   ideal-gas EOS for `eos.rs`. Build a particle planet from `planet::LayeredBody` and RELAX it to hydrostatic
   equilibrium. TDD: the settled radial pressure profile matches `planet::pressure_at`'s analytic dP/dr=−ρg
   integral (core ~360 GPa); RMS radius stable (no collapse/pulsation). *Only the EOS is new machinery — b+d
   already exist (docs/32 §3 atmosphere).* 
3. **Two-body impact, both bodies particles (dissolve the rigid boundary).** Replace the monopole+boundary
   Earth with the tier-coarsened particle planet from stage 2; run Theia into it. Re-measure the disk Earth
   fraction vs the rigid-boundary 7–12% ceiling (docs/31) — now Earth can shed its own mantle. Honest caveat:
   still relaxation-noise-limited at moderate N (docs/28 ceiling) — fixes the *mechanism*, keeps a resolution
   IOU on the number until stage 4.
4. **GPU-resident unified stepper (high N).** Extend the GPU compute path (docs/22 step 4, `particle_step.
   wgsl`) with the four physics it lacks — Barnes–Hut/tree self-gravity, SPH + EOS pressure, per-grain
   material, extended-body/boundary gravity — in an f32 body-relative frame. Verify GPU matches the CPU
   reference on a shared N (the `tools/gpu-verify` pattern). Run N~10⁵.
5. **Unify the containers.** Both `Engine` (terrain) and `OrbitDemo` (space) drive the one module; retire the
   forks — fold `AirField` into the shared SPH, give the CPU grain path real contact or retire it for the GPU
   path, collapse `body::Sphere` into a small-N aggregate. Generate or verify the WGSL law from the Rust one.
6. **Energy-tiered just-in-time particalization, formalized.** Make T0–T3 promotion/demotion a first-class,
   energy-density-driven awake-set spanning every band (generalizing `matter.rs`'s promote-on-excitement +
   `damage::classify` thresholds): **predictive** promotion on events (look-ahead via `swept_first_contact`
   / aim raycast), particle **resolution scaled by event energy/mass/size**, and **bake-back into a
   displacement + normal (bump) map** on settle (extending `texture.rs` and `deposit_resting_grain` so the
   resting tier is a real, persistent, textured record of the deformed matter). Retire the declared IOUs as
   the resolved shock replaces them where it matters. (Sub-stage: give T0 a real per-surface displacement/bump
   field the settle writes into — today the terrain de-resolves grains into voxel tops; the crater should
   persist as texture, not only as voxels.)

## 5. Non-goals / honest caveats

- **Not a big-bang.** Every stage lands independently, verified, on its own commit. The forks are removed as
  their replacement is proven, not before.
- **Resolution IOU persists through stage 3.** Even a particle Earth is relaxation-noise-limited at feasible
  N (docs/28). We show the *direction* honestly; the converged isotopic number waits for stage 4's N.
- **Tillotson is a choice, flagged.** It's the giant-impact standard, but ANEOS/M-ANEOS are more accurate for
  the vapor curve; the EOS module is written so the closure can be upgraded per material (like `thermal`).
- **Optimizations stay physics-faithful** — any speedup is pinned to the exact/analytic reference (the docs/30
  discipline), never a shortcut that changes the answer.
