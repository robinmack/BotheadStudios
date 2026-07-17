# Development Journal

A running log of major milestones for `greenfield-engine`. Newest entries at the top.
Each entry records *what* changed, *why*, and *how it was verified*.

---

## 2026-07-17 — Render-truth: the crater and continents CO-ROTATE with the crust (birth scene)

**What.** Fixed a render-frame mismatch Robin caught while watching the deployed birth scene (he read
Theia's approach as "curving to hit a fixed point"). Investigation verdict: **the approach trajectory is
HONEST** — pure N-body gravity (`orbit::verlet_step`, no steering), the impact site is an OUTPUT discovered
by swept CCD at contact (`impact_site_rel` is `None` through the whole approach), and the inward curve is
genuine gravitational focusing of a hyperbolic impactor in an Earth-centred frame. The "fixed impact point"
he reacted to is the **declared-zero proto-Earth spin** (`lib.rs:2915`, flagged unknown IC): with `spin_l=0`
the surface simply isn't rotating.

BUT the trace surfaced a genuine no-fudge bug: post-impact, once the collision spins Earth up, the crater
(`impact_site_rel`) was rendered as an INERTIAL vector (`earth_center + rel`) while the shell grains rotate
by `spin_rot` — so the hole slid through the rotating crust. And the landmask was sampled at the WORLD
direction (`earth_surface_material(spin_rot·fib_dir)`), painting continents world-fixed while grains rotate
underneath. Both fixed: the crater now co-rotates (`earth_center + spin_rot·rel`) and continents are sampled
at the fixed BODY direction (`earth_surface_material(fib_dir)`) — so grains, continents, and crater share
ONE crust frame that rotates honestly. (Invisible during the birth approach — `spin_l=0` ⇒ `spin_rot` is
identity — so the honest approach is unchanged; the fix bites post-impact when Earth spins up.)

**Verified.** Native + wasm build; full fast suite 151/151. Deployed.

**Flagged for Robin's call (physics IC, not a bug).** The birth scene's proto-Earth spin is deliberately
zero so the post-impact day EMERGES. If we'd rather the surface visibly rotate under the incoming impactor
(more physical — planets rotate), we give proto-Earth a primordial spin IC; the tradeoff is the day becomes
primordial + impact rather than purely emergent. Left as-is pending his decision.

---

## 2026-07-17 — Realignment stage 3c: a DEFORMABLE Earth resolves the isotopic-crisis DIRECTION (docs/33)

**What.** The scientific payoff of the whole realignment: collided a differentiated Theia into a
**deformable, self-gravitating, differentiated proto-Earth** (both real EOS particle bodies, relaxed first)
obliquely at ~mutual escape speed, integrated the aftermath with the shock-capturing SPH integrator (3a),
and MEASURED the bound orbiting disk by provenance (Earth particles vs Theia). Disk = bound material whose
orbital **perigee is above the remnant surface** (genuinely orbiting, separated from the planet body —
`orbit::perigee` about the 85%-mass remnant). No dial; the composition EMERGES.

**MEASURED (native, #[ignore], ~446 s).** `a_deformable_earth_impact_measures_the_disk_provenance`
(M_e=1.75e24 kg ≈ 0.29 M⊕, M_t=2.76e23, v≈7.3 km/s, N≈2100):
- **Orbiting disk 0.207 M☾ — 58% EARTH-derived** (Earth 8.75e21 | Theia 6.43e21 kg).
- Remnant: Earth 1.72e24 | Theia 2.22e23 kg; escaped: 2.1e22 | 4.7e22 kg.

**THE FINDING.** The rigid-boundary Earth capped the disk at **7–12% Earth** (docs/31 — only the excavated
cap could reach orbit). With Earth as REAL MATTER that can shed its own mantle, the disk jumps to **58%
Earth-derived** — Earth material not only reaches orbit, it DOMINATES the disk. This is the direction the
isotopic crisis demands (the real Moon is isotopically Earth-like), and it is exactly docs/28 root-cause #1
(the rigid boundary) being dissolved. Earth is now a participant in its own catastrophe.

**Honest caveats (no-fudge).** Sub-Earth scale (0.29 M⊕), coarse N (~2100 — a resolution/scale IOU,
docs/28), and the post-impact remnant is hot/expanded (R_remnant 9086 km), so the disk is defined beyond
that. **58% is the DIRECTION** (rigid ~10% → deformable ~58%), NOT a converged number — the converged
value waits for the GPU N (stage 4). A first attempt with a too-head-on geometry merged with no disk and
mis-measured (counted the whole extended Earth as "disk" — 89%); that artifact was rejected and the
measurement fixed to the perigee-above-remnant criterion.

**Verified.** Full fast suite 151/151; wasm builds. Stages 1→3c all green.

---

## 2026-07-17 — Realignment stage 3a: dynamical SPH — energy equation + artificial viscosity (docs/33)

**What.** Turned the isothermal planet into a full thermodynamic SPH body for the impact: added to
`hydrostatic.rs` (1) the **SPH internal-energy equation** `du_i/dt = ½ Σ_j m_j (P_i/ρ_i²+P_j/ρ_j²+Π_ij)
(v_i−v_j)·∇W` — the thermodynamically consistent partner of the momentum equation, so compression does PdV
work → heat; (2) **Monaghan artificial viscosity** Π_ij (α=1, β=2) for shock capture (without it SPH
particles interpenetrate at a shock and the impact heating is wrong); (3) an **energy-conserving KDK
leapfrog** `step(dt)` evolving position, velocity, AND internal energy (vs the damped `relax_step`); and (4)
an **adaptive Courant timestep** `courant_dt` from the live compressed sound speed.

**Verified (native, #[ignore], ~67 s).** `a_head_on_collision_conserves_energy_and_shock_heats`: two 400 km
basalt bodies, **relaxed to equilibrium first**, collide head-on at ±1.5 km/s —
- **Total energy (KE+IE+PE) conserved to ~3%** (a one-time injection at the shock front, then flat — the
  known SPH internal-energy-formulation shock error; 5% asserted bound).
- **Shock heating:** internal energy rose **4.9×** (bulk KE → heat), KE fell — the physics that vaporizes
  material and drives the disk.

**KEY LESSON (measured).** Colliding UNRELAXED spheres at 3 km/s TRIPLED the total energy (ΔE/E≈2) — the
startup non-equilibrium dumped into the shock; adaptive dt barely helped (so it wasn't CFL). Relaxing each
body first (Genda: "vibrations until v<100 m/s") + a moderate speed → 3% conservation. Real giant-impact
SPH always relaxes the bodies first; now we do too. Full fast suite 151/151; wasm builds.

**Why.** docs/33 stage 3: the two-body impact needs real shock thermodynamics (heating → vaporization → the
disk), not just contact. This is the integrator the deformable-Earth impact (3b/3c) runs on.

---

## 2026-07-17 — Realignment stage 2b: a differentiated iron-core Earth holds itself up (docs/33)

**What.** Built the layered/differentiated planet — an **Earth-mass iron-core + basalt-mantle** particle body
that holds itself in hydrostatic equilibrium as real matter. Rewrote `hydrostatic.rs` with the **Genda et al.
2012 method** (the fix for the earlier puff-up): **equal-mass particles** at the number density that recovers
each material's ρ₀, with a **per-particle adaptive smoothing length** `h_i ∝ (m/ρ₀)^⅓` (dense core sampled
finely, light mantle coarsely) and a symmetric per-pair `h_ij=½(h_i+h_j)`; per-particle EOS. `HydroBody`
gained `new_differentiated(core, mantle, core_r, total_r, u, N)`. Iron EOS updated to the verified/open
**Wissing & Hobbs 2020** compressed-branch refit (ρ₀=7850, A=128, B=181.5 GPa, a=0.5, b=1.28, E₀=14.25
MJ/kg); its vapor branch stays flagged provisional (stage-3 concern). Also fixed the EOS continuity test's
tolerance (it collapsed at iron's tension zero-crossing near E_iv — the function is continuous; smaller δ +
a bulk-modulus scale floor).

**Verified (native, #[ignore], ~326 s).** `a_differentiated_iron_core_earth_settles_compresses_and_
stratifies` (N=3000, M=5.96e24 kg):
- **COMPRESSES, does not puff up** — settled mass-weighted RMS **3973 km** from 5709 km initial (the old
  equal-volume prototype blew up to 15,700 km; the equal-mass fix is decisive).
- **Stratified:** iron core (mean r 2326 km) stays inside the mantle (4591 km); core settled ρ **15,591**
  kg/m³ (compressed above iron's ρ₀=7850 — real inner core ~13,000), mantle **5534** (real lower mantle
  ~4400–5500). Core denser than mantle ✓.
- **Hydrostatic balance rel 6%** at r=1986 km.
- **Central pressure 572 GPa** vs Earth's real **364 GPa** (Wissing & Hobbs 2020) — same ORDER (~1.6×).
  Honest caveats: coarse N=3000, Tillotson iron over-compresses at high P (a known Tillotson limitation), and
  basalt ≠ the denser perovskite lower mantle — so order-correct, not exact.
Stage 2a (single-material) re-verified green after the refactor — adaptive-h tightened its balance to rel
0.00–0.01. EOS 6/6; full fast suite 151/151; wasm builds.

**Why.** docs/33 stage 2: a planet that is real matter can shed its own mantle into the disk — the
prerequisite for dissolving the rigid boundary (docs/28 #1, docs/31). The differentiated Earth is the object
the impact (stage 3) will hit. Still isothermal (u fixed); the adiabatic energy equation is stage 3.

---

## 2026-07-17 — Research note: sourced EOS data + the differentiated-body method fix (docs/33)

Verification dig for the layered-planet params/method (some primary tables are book-only — Melosh 1989
p.234 — and Robin's linked review is paywalled). What I could source from OPEN literature:

- **Iron Tillotson (compressed branch), Wissing & Hobbs 2020 (A&A 635 A21), refit to Brown et al. 2000
  shock data:** ρ₀=7850, A=128 GPa, B=181.5 GPa, a=0.5, b=1.28, E₀=14.25 MJ/kg. (Vapor-branch E_iv/E_cv/α/β
  NOT given there — still need the primary Melosh table for those; but the compressed branch is all a static
  planet needs.) My current `eos::iron` has A=128 GPa ✓ but b, B, E₀ differ from this refit — update pending.
- **Real Earth-layer structure, Wissing & Hobbs 2020 Table 1** (their PREM fit — a validation dataset for a
  layered particle Earth): inner core ρ₀=7744/B₀=166 GPa, outer core 6920/115, lower mantle 4121/231,
  transition 3622/160, asthenosphere 3380/130, crust 2300/100; M=5.97e24 kg, central P=**364.1 GPa**,
  T_c=5300 K. (A is ≈ the bulk modulus B₀, so these cross-check the Tillotson A values.)
- **Basalt Tillotson: VERIFIED, Benz & Asphaug 1999 Table 2** (exact match to `eos::basalt`).
- **Differentiated-body METHOD, confirmed from Genda et al. 2012 (the puff-up fix):** SPH particles all
  **equal mass**, placed on a **3D FCC lattice** (iron inside, rock outside), internal energy set to
  **1.0×10⁶ J/kg**, relaxed until velocities < 100 m/s. My equal-VOLUME/unequal-mass init was the bug.

Still blocked (needs the primary Melosh 1989 p.234 table or paywall access): full Tillotson sets (esp. the
vapor branch) for **granite, dunite, and iron**. Flagged provisional in `eos.rs`.

---

## 2026-07-17 — Honesty pass: EOS parameter provenance + stage-2b puff-up (docs/33)

**What.** Two honest corrections while extending stage 2 to a layered/differentiated planet (stage 2b):

1. **EOS parameter provenance.** Stage 1's tests verify only SELF-CONSISTENCY (cold P=0, K=A, continuity),
   NOT agreement with the literature — so a wrong-but-self-consistent parameter passes. I had written the
   Tillotson params from memory and labeled them "cited." Verified what I could: **BASALT matches Benz &
   Asphaug 1999 (Table 2) exactly** (ρ₀=2700, A=B=26.7 GPa, E₀=487, E_iv=4.72, E_cv=18.2 MJ/kg, α=β=5) —
   which is why stage 2a settled cleanly. GRANITE, DUNITE, IRON I could NOT verify online (papers cite
   Melosh 1989 p.234 but don't reproduce the table; PDFs weren't text-extractable), so `eos.rs` now flags
   them **PROVISIONAL — unverified against the primary table**. One confirmed fix: dunite ρ₀ 3500 → **3320**
   (Chau et al. 2018). No false "cited" claim stands.

2. **Stage 2b (differentiated iron-core + peridotite-mantle body) PUFFED UP** — RMS radius blew from 2000 km
   to ~15,700 km, mantle density collapsed to 507 kg/m³. The prototype's assertions were too weak and it
   FALSELY passed; I reverted it. Two likely causes, both flagged: (a) the equal-volume / **unequal-mass**
   SPH init corrupts density at the core–mantle interface — proper differentiated bodies need **equal-mass
   particles + adaptive smoothing length** (standard SPH); (b) a bad transcribed parameter (dunite `cap_b`
   is suspect). Deferred until both are resolved: verified params + equal-mass/adaptive-h init.

**Verified.** EOS self-consistency 6/6 still green after the dunite-ρ₀ correction; single-material stage 2a
(basalt, verified params) stands as the solid milestone. Stage 2b reverted, not shipped.

**Why.** No-fudge (docs/23): don't claim "cited" without verifying, and don't ship a test that passes on a
physically wrong (puffed-up) body. Recorded the real state rather than a green checkmark.

---

## 2026-07-17 — Realignment stage 2: a particle planet holds itself up (self-gravitating EOS body, docs/33)

**What.** Added `hydrostatic.rs` — a self-gravitating condensed-matter body that holds itself in hydrostatic
equilibrium as REAL MATTER (a cloud of particles), instead of the rigid analytic boundary the impact scene
uses (docs/28 root cause #1). It is the "merge" docs/32 §3 identified: it COMPOSES the shared kernels rather
than forking them — `eos::Tillotson` pressure (stage 1) + the one SPH kernel `atmosphere::sph_w/dw` +
`bhtree::BarnesHut` self-gravity. `HydroBody::new_sphere` fills a sphere with equal-mass particles at ρ₀,
each with `u=c·T`; `relax_step` settles it (damped) under self-gravity + the symmetric SPH-EOS pressure
force `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W` with `P=Tillotson(ρ,u)`. The only new physics is the condensed EOS; at
unification (docs/33 stage 5) this folds INTO `Aggregate` so a planet and its debris are one particle
system — for now it's a focused, independently-verified module (correctness-first).

**Verified (native, #[ignore], ~215 s).** `a_self_gravitating_eos_body_settles_into_hydrostatic_balance`:
a 1500 km basalt body (N=3000) relaxed under self-gravity + Tillotson pressure —
- **Stable:** settled RMS radius **1383 km**, spread **1.1%** over the last steps (no collapse/explosion).
- **Hydrostatic balance pointwise:** dP/dr vs −ρ(r)·g(r) [g=G·M(<r)/r² from the enclosed particle mass] —
  at r=484 km, −902 vs −1081 (17%); at r=761 km, **−1660 vs −1617 (3%)** — right sign, within SPH operator
  tolerance (cf. atmosphere.rs's 3D balance at ~35%).
- **Central pressure 2.29 GPa** vs the uniform-density self-gravity estimate 3.17 GPa — same order, a real
  planet pressure.
Full fast suite 151/151; wasm builds. Isothermal (u fixed) this stage — the adiabatic energy equation
under compression is the stage 2b/3 refinement. Not yet in a scene.

**Why.** The prerequisite for dissolving the rigid boundary (docs/28 #1, docs/31): a planet that is real
matter can shed its own mantle into the disk. Proves the merge works before touching the tested `Aggregate`.

---

## 2026-07-17 — Realignment stage 1: the Tillotson condensed-matter EOS (docs/33)

**What.** Added `eos.rs` — the **Tillotson equation of state**, `P(ρ, u)` for condensed matter across cold /
shock-compressed / decompressed / vapor states in one closure (the giant-impact standard: Tillotson 1962;
Melosh 1989 App. II; Benz, Cameron & Melosh 1989). This is the missing physics docs/32 §5 flagged: solids
previously resisted compression only via a linear-elastic contact penalty (E·r/m) and planet densities were
declared constants, so shock-compressed rock had no way to develop pressure from its density. `Tillotson`
carries the cited parameters for **granite, basalt, peridotite (dunite/olivine analog), and iron**;
`pressure(ρ,u)`, `sound_speed_sq(ρ,u)` (central-difference, for CFL + bulk-modulus readout), and
`for_material(name)` lookup. Params live in `eos.rs` for now; migrating them into `data/materials.json` (a
`tillotson` block beside `thermal`) is the flagged source-of-truth follow-up (docs/04).

**Why.** The keystone of the realignment (docs/33): ONE pressure law spanning solid→liquid→vapor, replacing
the ideal-gas-vapor + linear-elastic-penalty + declared-density patchwork. The SPH pressure-force machinery
(`aggregate`/`atmosphere`, `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W`) is untouched — only the `P(ρ,u)` it evaluates
changes — which is why a self-gravitating condensed-matter planet (stage 2) is a merge, not new machinery.

**Verified (native, TDD — 6 tests).** `cold_reference_state_has_zero_pressure` (P(ρ₀,0)≈0);
`cold_compression_gives_the_bulk_modulus` (K=ρ·dP/dρ at ρ₀ matches each material's A within 2% — a REAL
bulk modulus, not a contact-spring surrogate); `compression_monotonically_raises_pressure` (stiffens to GPa
scale — the impact regime); `hot_expansion_relaxes_toward_vanishing_pressure` (fully-vaporized expanded
parcel → the ideal-gas limit a·ρu); `pressure_is_continuous_across_the_vaporization_boundaries` (no jump at
E_iv/E_cv); `sound_speed_is_real_and_of_the_expected_order` (c≈√(A/ρ₀), km/s). Full fast suite 151/151; wasm
builds. Not yet wired into any scene (stage 2 builds the self-gravitating planet on it) — nothing to
rig-watch/deploy yet.

---

## 2026-07-17 — Architecture map + first-principles realignment plan (docs/32, docs/33, CLAUDE.md)

**What.** Mapped the whole engine and wrote it up for future Claude sessions (Robin: too many "surprises"
about what already exists). Four parallel readers covered the physics core, terrain/atmosphere, scene/render/
GPU, and docs/build/deploy; synthesized into **docs/32-architecture-map.md** (module-by-module with
`file:line` anchors, the shared-laws-vs-forked-solvers map, the EOS inventory, the birth-of-the-Moon scene
trace, and the workflow rules), a concise auto-loaded **CLAUDE.md** pointing to it, and
**docs/33-architecture-realignment.md** — a staged plan to realign the architecture to Integrity's
principles (Robin's three framings: material physics scalable · calculations tiered on energy scale ·
everything a natural product of the real physics).

**Key finding.** The physics *laws* are already unified and scale-invariant (`granular::Contact`, the SPH
kernel, `Furrow` excavation, `plough_loft`, `Body`, `LayeredBody`); the *solvers and containers* are FORKED
— two container universes (CPU `Aggregate` f64 vs voxel-`World`/GPU f32), four integrators over one law, the
rigid-boundary fork (Earth is a penalty sphere, not particles — docs/28 #1), and **no condensed-matter EOS**
(solids resist via a linear-elastic contact penalty; planet densities are declared constants). A
self-gravitating EOS planet turns out to be a MERGE, not new machinery: `atmosphere.rs`'s verified SPH
pressure kernel + `bhtree.rs` self-gravity + `aggregate.rs::apply_thermo` energy equation, with the ideal-gas
EOS swapped for a Tillotson EOS — only the EOS is genuinely new.

**The realignment (docs/33).** One particle/material engine every scene drives: one container (bulk forms
are the coarse *energy tier* of the same particles, not a separate universe), one pressure law (Tillotson EOS
spanning solid→liquid→vapor, replacing the ideal-gas + linear-elastic + declared-density patchwork), one
energy-tiered stepper (fidelity T0 bulk → T1 quasi-static → T2 granular+thermal → T3 full EOS shock/vapor,
selected by energy density vs the material's own thresholds — generalizing docs/08/13 spatial LOD to
energy-tiered physics via the docs/16 awake-set). Staged correctness-first: (1) Tillotson EOS module +
tests, (2) self-gravitating EOS planet vs planet.rs's analytic hydrostatic profile, (3) two-body impact both
bodies as particles → re-measure the isotopic crisis, (4) GPU-resident unified stepper at N~10⁵, (5) unify
the containers, (6) formalize the energy-tiered awake-set. Full-particle-Earth is milestones 2–3.

**Why.** Robin's directives: all particle physics in ONE scale-invariant module; build the hard correct
physics first (GPU/full-res if needed), optimize physics-faithfully later; everything a natural product of
the real physics. The map stops the rediscovery; the plan makes the full-particle-Earth build the forcing
function of the realignment rather than a side quest.

**Verified.** Docs only — no code change. Existing suite unaffected.

---

## 2026-07-16 — The isotopic crisis: physics says proto-Earth spin is NOT the lever (docs/31)

**What.** Opened the isotopic crisis (docs/31, "Option C"): the canonical impact makes a **Theia-dominated**
disk, but the real Moon is isotopically Earth-like. Tested **Ćuk & Stewart (2012)'s** proposed resolution —
a *fast-spinning* proto-Earth flings its own mantle into the disk. Implemented proto-Earth spin honestly:
the excavated Earth cap is surface mantle that was **co-rotating before the impact**, so each `SOURCE_TARGET`
grain is now born with `ω × (pos − centre)` (added in `build_impact_debris_scaled` before the ploughing
loft, so the momentum exchange acts on the real pre-impact velocity; `earth_omega = 0` is byte-identical to
before). Scene wired: `lib.rs` converts `spin_l → ω = L/I` (solid sphere) and passes it, default **zero**
(unknown IC, flagged) — nothing changes on screen; the plumbing just lets a spin be *explored*.

**MEASURED (physics deciding against the hypothesis).** `a_fast_spinning_protoearth_makes_the_disk_earth_
derived` (#[ignore], N=256+512, 3000×2 s), non-spinning vs a 2.3 h-day proto-Earth (ω·R ≈ 4835 m/s):
- ω=0    : Earth **0.162** | Theia 1.241 M☾ → disk is **12 % Earth**
- ω=fast : Earth **0.181** | Theia 2.412 M☾ → disk is **7 % Earth**

A fast spin lofts *slightly* more Earth material (0.162→0.181) and injects a lot of angular momentum, so the
whole bound disk grows (1.40→2.59 M☾) — but it retains proportionally **more Theia**, so the Earth *fraction*
FALLS, 12 %→7 %. **Spinning the target does not resolve the crisis in our model.**

**Why — and the real lever.** Direct consequence of docs/28 root cause #1: **Earth is a rigid boundary**, so
the only Earth material that can reach the disk is the small excavated cap. The actual Ćuk & Stewart
mechanism is a spinning proto-Earth shedding its **bulk mantle** — which a rigid analytic sphere cannot do.
So 7 % is a LOWER BOUND the rigid boundary imposes, and adding spin only speeds up the material that *is*
free to move (overwhelmingly Theia). The honest resolution needs **Earth-as-deformable-matter** (docs/28 #1)
or **vapor-phase Earth↔Theia mixing** (now partly reachable via the SPH vapor field, docs/26/27) — NOT
target spin. Documented in docs/31 with the next experiments.

**Why.** No-fudge (docs/23): we set a physical initial condition (spin) and let the disk provenance EMERGE;
when it emerged *against* the hypothesis we recorded that, and the test now asserts only the robust mechanics
(spin ⇒ larger bound disk) plus the measured ceiling (fraction does not rise), printing the provenance split.

**Verified (native).** Full fast suite 145/145; the measurement test green with the corrected (measured)
assertions; wasm builds; scene byte-unchanged at the default zero spin.

---

## 2026-07-16 — The accelerated compute module: neighbour grid + Barnes–Hut + block timesteps (docs/30)

**What.** Built the reusable **accelerated particle compute module** (docs/30) so the impact disk can run
at high N without the O(N²) wall — a general substrate (any particle system: weather, clouds, fluids), not
an impact special-case. Four stages, each its own crate/module with a brute-force fallback below a size
threshold and a test that pins it to the exact/near-exact reference:

- **Stage 1a/1b — neighbour grid** (`neighbors.rs`). A spatial-hash `NeighborGrid::build(pos, cell)` +
  `for_each_pair` that finds every short-range pair in O(N) instead of O(N²), then wired into the contact
  and SPH density/pressure loops (one `sr_grid` built per step from shared `sr_pos`/`masses`). Brute-force
  below 512 bodies. Test: `grid_finds_exactly_the_brute_force_pairs` (exact — the grid is not an
  approximation).
- **Stage 1c — Barnes–Hut self-gravity** (`bhtree.rs`). An octree caching per-node COM+mass; a particle
  uses a node as ONE source when its angular size `(2·half)/dist < θ` (θ=0.5), turning O(N²) self-gravity
  into O(N log N). Same Plummer softening as the direct sum — the same physics, grouped. Test:
  `barnes_hut_matches_brute_force_within_theta_bound` (RMS < 1% at θ=0.5; θ→0 recovers brute force to 1e-9).
- **Stage 3 — block timesteps** (`aggregate.rs`). A per-particle timestep criterion (`particle_timesteps`:
  √(ε/|a|) free-fall, capped by the |v|/|a| turnaround), then a hierarchical **block KDK** integrator
  (`step_block`): power-of-two rungs, the quiescent disk coasts while the shocked/vapor core sub-steps.
  The subset-force pass (`accelerations_masked` + `BarnesHut::accelerations_active`) recomputes gravity
  only for the bodies being kicked this sub-step — O(N_active log N). Thermo (PdV cooling, radiation,
  phase flip, dissipation heating) was extracted into `apply_thermo` and now runs each sub-step, so
  `step_block` is a faithful full-physics drop-in for `step()`. Wired into the space scene.

Also this pass: the impact scene now runs at **high N (512 debris + 1024 cap)** with the cap-mass fix
restored (`cap_mass` summed from the real per-grain target masses, not the `moon_mass·CAP_N/DEBRIS_N`
bookkeeping that the 07-15 entry flagged as ≈6.5× high); and two **watching** tools so the agent can see
what Robin sees — `rig/birth_shot.mjs` (headless-Chromium screenshots of birth.html at timed marks) and a
"📷 Share view" button on the space band that POSTs the live canvas.

**Why.** docs/30: temporal + spatial coherence is the "MPEG for physics" — most of the cloud barely moves
per step (the block scheduler's coasting rungs are the delta-frames; the grid/tree are the spatial
compression). Getting the disk to lunar-mass resolution needs O(N log N), and the module has to be generic
because the same substrate runs every future particle system. No-fudge (docs/23): every accelerator is
proven against its exact/θ-bounded reference, so speed never changes the answer.

**Verified (native).** Full suite green; `grid_finds_exactly_the_brute_force_pairs`,
`barnes_hut_matches_brute_force_within_theta_bound`, `contact_grid_matches_brute_force`,
`particle_timesteps_shrink_with_acceleration`, `step_block_conserves_energy_and_matches_global_dt`, and
— the decisive one — `birth_impact_with_step_block_reproduces_the_disk`: the REAL coupled impact gives
**global step() 0.772 M☾ vs block step_block 0.788 M☾** (matches). `step_block_speedup_bench` measures
**5.5× faster** on an aftermath-shape cloud (1330 ms → 241 ms). On-screen: deployed to
integrity.bothead.net (build 20260716.081104) and rig-watched — the disk forms and evolves identically to
the global integrator (T+24m: 2.44 M☾ in 42 accreting moonlets, Earth-origin material aloft), no regression.

---

## 2026-07-15 — Vapor gets a real pressure field: SPH + a latent-heat reservoir (docs/26/27, docs/28 item 5)

**What.** Replaced the vapor "overlap hack" with a real **SPH pressure field** so the impact-generated
vapor expands and cools as a gas from first principles, not a scripted push. `aggregate.rs`: a cubic-spline
kernel gives each vapor particle a density ρ=Σm_jW(r,h); pressure P=ρ·R_s·T; a symmetric,
momentum-conserving pressure force; and a PdV energy equation so expansion does real work and the gas
cools itself. Then a **latent-heat reservoir** (docs/28): the pressure reads the *thermal* temperature
`T − L_v/c`, so the energy locked in the vaporization latent heat is not double-counted as pressure — the
vapor holds heat honestly on the phase boundary instead of over-puffing. Also shipped the
`disk_orbit_vs_resolution` diagnostic sweep (the disk grows toward lunar mass with N: 0.77→1.27→1.41 M☾ at
N=384/768/1536).

**Why.** docs/26/27: the atmosphere/vapor must be *matter under its own pressure*, not a visual. The old
overlap repulsion was a fudge (docs/23); SPH is the honest continuum form, and the latent-heat correction
keeps the first law intact across the solid↔vapor phase change (docs/28 item 5).

**Verified (native).** `vapor_sph_expands_and_cools_conserving_energy` — a hot vapor ball expands under
its own pressure and self-cools (80k → 18.5k K), total energy conserved to within drift; the latent-heat
fix dropped a spurious vapor↔vapor dissipation heating that had inflated both temperature and disk mass
(disk 0.066 → 0.132 M☾, peak T 52k → 18.5k K — honest physics over the bigger-but-wrong number). Full
suite green.

---

## 2026-07-15 — The Moon becomes Earth-derived: a momentum-conserving loft breaks the 0.000 deficit

**What.** Closed docs/28 step 3. Earth (target) material now LOFTS into the bound proto-lunar disk —
**Earth 0.083 M☾ | Theia 0.551 M☾** aloft, where it had measured a dead **0.000 M☾ Earth** at every
resolution (the "nothing is taken from Earth" deficit). The Moon is now genuinely Earth-derived, as the
isotopes demand — and it emerged from conserved mechanics, no dial. Two coupled fixes:

- **Physical cap mass (docs/28 item 4).** The excavated cap was materialized at a bookkeeping **2× the
  impactor** mass; it is now real **ρ·V** — each grain an equal slice of the furrow's half-ellipsoid volume
  times the LOCAL density at its depth (≈ 0.31× the impactor). `furrow_target_grains` sets it; the energy
  cap and per-grain contact use each grain's real mass.
- **A momentum-conserving loft in the SHARED particle physics** (`granular::plough_loft`, not the impact
  builder — Robin: "added to global particle physics"). When a fast body ploughs slower target matter, the
  along-track (tangential) momentum is shared inelastically toward the impactor↔cap **centre-of-mass**
  velocity — the physical maximum drag, no free dial — and what the cap gains the impactor loses, so
  Σ(m·v) is **exactly** conserved. Only the along-track component is touched (radial rebound + gravity keep
  theirs). This is the same reverted "COM drag" from 2026-07-14 that made it WORSE — the ONLY thing that
  changed is the cap mass: at the fudged 2× the COM speed collapsed to v_t/3 (sub-orbital, gutted the
  disk); at the physical 0.31× it is ~0.76·v_t ≈ near-orbital, so Earth material joins the disk while the
  impactor barely slows. The cap-mass fudge, not the mechanism, was the blocker all along.

One law for every band: a terrain meteor and a giant impact both loft their excavated matter through
`plough_loft` (space-band wired now; terrain wiring is a flagged follow-up).

**Why.** docs/23/24 no-fudge: the loft is real ploughing momentum, declared HONESTLY as a conserved
transfer (the µs shock is sub-resolution at any N — docs/24 #1), never a scripted velocity.

**Verified (native).** `plough_loft_conserves_momentum_and_lofts_the_lighter_target` (Σ tangential p
unchanged; cap dragged up, impactor slowed; radial untouched; vertical = no-op). **Full suite 144/144** —
every disk guardrail (birth peak-aloft > 0.3 M☾, emergent day 2–14 h, theia) still holds, so the honest
mass + loft did not detune the disk. On-screen rig-watch (birth.html) is the remaining check — pending
Robin's eyes / a rig in this env. FOLLOW-UPS (flagged, not papered over): the lib.rs interactive-scene
mass bookkeeping (`cap_mass = moon_mass·CAP_N/DEBRIS_N`, now ≈6.5× high) and terrain-band `plough_loft`
wiring.

---

## 2026-07-14 — Measured: "raise N" does NOT loft Earth material (the disk deficit is a mechanism, not a resolution, problem)

**What.** Investigated docs/28 step 3 (progressive excavation) — why the proto-lunar disk is ~100%
impactor ("nothing is taken from Earth"). Made the impact resolution a real knob
(`impact::build_impact_debris_scaled(.., debris_n, cap_n)`; the const `build_impact_debris_between`
delegates at the default 128/256) and added two `#[ignore]` measurement sweeps
(`disk_provenance_vs_resolution_sweep`, `disk_provenance_emergence_no_declared_ejection`). Then MEASURED
the bound-aloft disk composition across N — the honest test of the "raise N globally" hypothesis.

**Why.** Before spending the O(n²)→tree perf work that a global N increase would require, prove that more
resolution actually lofts Earth-derived material. It does not.

**Verified (measured, native).** Bound-aloft mass by provenance (M☾), 3000×2 s aftermath:
- Declared ejection ON: N=384/768/1536 → **Earth 0.000 / 0.000 / 0.000**; Theia 0.69 / 0.35 / 0.72
  (the Theia disk mass does not even converge — it is relaxation-noise-limited, the docs/28 collisionless
  ceiling, not resolution-starved).
- Declared ejection OFF (cap AT REST, contact ploughing must do the lofting): N=384/1536 →
  **Earth 0.000 / 0.000**; Theia 0.69 / 0.84.

**Earth material lofts in NONE of the six configurations.** The cause is provable and N-INDEPENDENT: a
grain launched from the surface needs a near-tangential speed ≥ the ~7.9 km/s circular velocity to hold a
perigee above the surface. The declared `Furrow::ejection` gives ~5.9 km/s at ~45° (horizontal ≈ 4.1
km/s) — sub-orbital, so every cap grain re-impacts, at any N. With the ejection OFF, contact ploughing
drives the resting cap DOWN and downrange into the planet, not up — the shock-driven excavation flow that
would loft it is sub-resolution at any feasible N (docs/24 problem #1), so it never emerges. **Conclusion:
the Earth-lofting deficit is a MISSING MECHANISM, not a resolution shortfall; "raise N globally" is not
the lever.** A separate dead end confirmed and reverted en route: a momentum-conserving "ploughing drag"
(impactor drags cap downrange toward the COM tangential velocity) makes it WORSE (both → 0.000) — full
inelastic sharing drops the impactor to v_t/3 and guts its own disk, and the cap only reaches ~2.2 km/s,
still sub-orbital.

**The real levers (for the next session / Robin's steer), all no-fudge (docs/23, docs/24):**
1. **Materials-honest contact.** Theia's *construction* is layered (iron core + peridotite mantle, as
   theorized), but its collision *physics* is bulk **basalt** for every grain (restitution 0.40, basalt
   density for grain radius, equal grain mass). That basalt restitution IS the disk's damping law. The
   aggregate contact already carries per-grain `mat_ids` and is momentum-conserving for ANY mass ratio
   (equal-and-opposite forces ÷ each own mass) — so per-grain real material + real ρ·V mass is viable at
   full resolution; it just needs the contact loop to read `mat_ids`. This also fixes docs/28 item 4 (the
   cap is ~6.5× over-massed: 2× impactor vs the physical ρ·V furrow ≈ 0.31× impactor).
2. **The docs/24 emergence subsystem** — deposit the impactor's momentum/energy as real compression so
   REBOUND lofts material (delete the declared `Furrow::ejection`). Since the µs shock is sub-resolution
   at any N, the honest form is a momentum-CONSERVING loft that gives near-track excavated Earth material
   *near-orbital tangential* velocity from the impactor's momentum (not the radial 45° script) — the
   corrected version of the reverted drag, unblocked once the cap mass is physical (item 1).

**Shipped this pass:** the N knob + the two reproducible measurement sweeps (all 136 native tests green;
the sweeps are `--ignored`, O(n²)). No physics claim shipped — the finding is the deliverable. On-device
rig-watch not required (nothing visual changed). NOTE: the Jul 12 render-truth fixes and the Jul 13
terrain-contact/furrow commits are still un-journaled — a catch-up entry is outstanding.

---

## 2026-07-11 — The engine watches itself: the rig, the profiler, and a 7× frame

**What.** The agent now verifies scenes with its own eyes before shipping them (Robin: "simulate
locally and watch — we've been through a lot of iterations you could have seen going wrong"). The watch
rig (`web/rig`): headed Chromium under xvfb (headless cannot composite WebGPU swapchains — the first
attempts photographed a blind rig, not a broken app), timed screenshots, a frame profiler, an fps probe.
First session of use, in order: proved the scenes render correctly; caught a post-impact DEATH SPIRAL
(one slow frame → 0.25 s backlog → 128 O(n²) substeps → slower still, pinned at 1 fps); profiled
advance() at 161 ms vs render() at 3 ms; and found the real culprit — `powf(-1.5)` libm calls per
gravity pair. Hardware sqrt: **161 → 22 ms/frame (7×)**; the native suite dropped 133 → 52 s too.
Substep budgeting ends the spiral (observable time dilates; the frame stays interactive). Camera opens
on the sun side (the night side is honestly black now) and rides the BOUND debris extent (escapees no
longer drag the view out to pixels). Watched verdict at T+13h aftermath: 32 fps, 354 → 62 fragments as
settled matter demotes into Earth, disk 0.48 M☾ in 3 moonlets — the on-screen numbers now match the
native emergence tests.

**Verified.** By watching. 91/91 native; profiler numbers above.

---

## 2026-07-11 — The Birth of the Moon: the SCENE (docs/27)

**What.** The proven giant-impact physics, now watchable: a new scene (**Birth of the Moon** in the scene
picker) opens ~5 real seconds before the strike at the close framing (25% of lunar distance), with a HUD
countdown that IS the simulation's own forecast (distance / closing speed from the live N-body state —
the same conservation-law machinery, read as a clock). Theia arrives with a real IMPACT PARAMETER
(0.87 of the contact radius at 6 km/s from quarter-lunar range), so the ~45° obliquity of the hypothesis
EMERGES from geometry + gravity at contact — never aimed. At the strike, both bodies materialize (Theia's
iron core + hot mantle; Earth's crust/mantle/outer-core cap), and the camera rides OUT with the ejecta —
view distance tracks the debris extent — to the wide whole-orbit framing, watching the lofted, bound,
perigee-raised material (0.55 M_moon in the native test) circularize into the proto-lunar disk. Replay
re-runs the encounter.

Also, for ALL impact scenes (Robin): a **T+ aftermath clock in SIM time** (y/mo/d/h/m/s at the scale the
number deserves) — the honest answer to "what timeframe are we watching?", since time-LOD means wall
time ≠ world time; and the pre-impact countdown for the birth scene. The impactor is now a first-class
parameter of the space band (radius/mass/profile drive CCD, excavation, rendering, materialization), so
the moon-drop is just one configuration of the same scene machinery.

**Verified.** 87/87 native (the physics is the previous entry's test); wasm + TypeScript build clean.
The choreography needs on-device eyes.

---

## 2026-07-11 — THE ANTITHESIS: the birth of the Moon (docs/27)

**What.** Robin: *"a mass impacted the earth and ejected the material that became the moon — I'd like to
see that happen. If it works, we can prove our system works."* The proof, as a passing native test: the
SAME impact machinery that shatters a falling Moon, run in reverse role — a Mars-sized differentiated
impactor (**Theia**: iron core + peridotite mantle, ~6.5e23 kg, declared like every other body) strikes
Earth **obliquely** at the mutual escape speed (~9.5 km/s; obliquity is what puts mantle on lofted
trajectories with angular momentum instead of straight up). Kepler alone would return every launched
fragment to its launch radius — it is debris-debris CONTACT and SELF-GRAVITY, already in the model, that
must raise perigees into orbit. Integrating the aftermath: **0.55 lunar masses of material ends up aloft,
bound, and perigee-raised above the surface — genuinely orbiting** (the theorized proto-lunar disk is
1–2 M_moon; 0.55 at 192-particle resolution is the right scale), while only 0.14 M_moon escapes. The
Moon-forming reservoir emerges from the declared bodies and the one contact law. Nothing was scripted;
the machinery was not told what a "disk" is.

Also: `build_impact_debris` generalized to ANY impactor/target pair of layered bodies (the moon-drop
scene is now just one parameterization), Theia added to the planet profiles, giant-impactor excavation
clamped to a hemispheric scale (flagged approximation). The interactive birth-of-the-Moon SCENE (5 s HUD
countdown, camera riding the ejecta out to watch the Moon form) is the next build on this physics.

**Verified.** `an_oblique_theia_impact_lofts_bound_material_the_protolunar_disk`; 87/87 native.

---

## 2026-07-11 — The exponential atmosphere EMERGES (docs/26 tests 1+2)

**What.** Air is now dynamic matter (`atmosphere.rs`): gas parcels whose resistance to compression is
their EQUATION OF STATE (ideal gas — the 1D column force is exactly F = A·ρ·R_s·T per slab), never an
elastic modulus. THE emergence result: a column of 200 equal-mass air slabs under gravity, started from
a deliberately WRONG exponential (2× the real scale height), relaxes to the real isothermal atmosphere —
**measured H = 8,446 m vs the analytic R_s·T/g = 8,427 m (0.2%)** — proving the profile is an attractor
of the physics, not an initial condition. And the settled column's basal pressure equals its weight
(100,266 vs 101,357 Pa — one real atmosphere from one real declared column mass): the docs/25 static
boundary condition is provably this dynamic model's limit. Also: `gas_contact_from_material` (K = γ·P
stiffness for the canonical contact law), R_s = 287 J/(kg·K) from the declared molar mass, and
free-expansion-in-vacuum (gas never clumps). Flagged next: the 3D SPH kernel density (the column is the
honest first resolvable case), then drag + entry glow (docs/26 tests 4–5).

**Verified.** `a_settling_air_column_finds_the_real_exponential_atmosphere` + 2 more; 83/83 native.

---

## 2026-07-11 — Every solid object is matter: the Moon gets the same treatment

**What.** Two representation asymmetries closed (Robin: "Every solid object in the universe is composed
of matter"): (1) the intact **Moon now renders as a grain shell** — its basalt crust at its measured
reflectance — exactly like Earth; no more smooth-sphere summary on one body and honest grains on the
other. (2) **Moon-vs-moon collisions use the same primitives as moon-vs-Earth**: swept CCD on the
pre-step relative path, the true contact state from the conservation laws (vis-viva + angular momentum),
an inelastic momentum-conserving merge at the contact configuration, and the dissipated energy
accounted. Nothing special-cases Earth anymore. Flagged next: materializing a moon-moon impact cloud is
the same `build_impact_debris` with the target's layered profile parameterized (today it samples Earth's
profile for the target and the Moon's for the impactor).

**Verified.** 80/80 native; wasm builds.

---

## 2026-07-11 — Physics/render decoupling: the simulation runs the world; the render just looks at it

**What.** The space band's physics no longer lives inside `render()`. The new architecture (docs/13 made
real):
- **`advance(real_dt)`** drives the PHYSICS from wall-clock time in fixed sim-timestep substeps whose
  COUNT (never size) varies with the elapsed real time. The physics rate is now independent of the
  display frame rate — a 30 fps client simulates the same world as a 120 fps one (previously the sim
  assumed 60 fps and ran half-speed at 30). Under overload the observable clock dilates (backlog is
  dropped) rather than corrupting the physics with an oversized step: time slows before truth breaks.
- **The renderer samples snapshots ~100 ms BEHIND the physics** (Robin: humans can't catch detail under
  1/10 s, so use that budget). Every event the render draws is already fully resolved — a collision can
  never be caught mid-lie by a frame boundary, structurally: the fly-past class of bugs is now
  impossible rather than patched. Snapshot interpolation gives smooth motion at any frame rate; the
  crater/shatter appear exactly when the RENDERED clock crosses the shatter instant.
- Physics is never triggered by, or dependent on, the visualization — it drives it (Robin's
  architectural invariant, verbatim).

Also fixed from Robin's render read: **"hollow earth"** — through the crater you could see the far side
of the crust from inside. The planet isn't hollow: the un-materialized bulk (physically the boundary +
gravity source) now renders as an opaque interior sphere at the depth the crater exposes — the top of
the outer core, self-lit at its REAL temperature from the layer profile. Through the hole you now see
glowing molten interior, honestly.

**Verified.** 79/79 native; wasm builds. Frame-rate independence and the lag are structural (wall-clock
in, snapshots out); on-device read pending.

---

## 2026-07-11 — The atmosphere's weight keeps the oceans liquid (docs/25)

**What.** Earth now declares only the MEASURED MASS of its atmosphere (5.15e18 kg); the surface pressure
emerges as that column's weight — ≈1 atm, never assigned. Materials gained Clausius–Clapeyron BOILING
curves (latent heat + molar mass, `thermal.molar_mass`) beside their Simon melting curves, and the phase
decision (`planet::surface_phase`) now covers solid/liquid/vapor. The consequences, all as passing tests:
288 K water under the emergent 1 atm is LIQUID; the same water in vacuum flashes to VAPOR at any
temperature (below the ~611 Pa triple point liquid has no regime — Robin: "water exposed to vacuum would
be wild", and the model now says exactly that); cold water freezes; water boils at ~366 K at 0.7 atm
(mountain physics for free). The airless Moon ⇒ no lunar seas, as observed. A failing test caught real
physics along the way: Earth's inner core briefly classified as "Vapor" because iron's boiling point was
a flat 1-atm fallback — pressure suppresses boiling even harder than melting, and with iron's real molar
mass boiling is COMPLETELY suppressed at 360 GPa (the fallback was the dishonesty).

**Why.** Same pattern as the molten core: declare real composition (now including the air), compute the
consequences. Also fixes the record on ocean colour: water renders with its measured near-black
reflectance — the "blue marble" is atmospheric Rayleigh scattering, which we refuse to paint. The
atmosphere today is a static boundary condition (pressure/phase); making it MATTER — drag, entry plasma,
Rayleigh blue, blast waves, evaporation cycling — is now the flagged next major milestone (docs/25
roadmap).

**Verified.** `the_declared_atmosphere_mass_weighs_in_at_one_atmosphere`,
`liquid_oceans_exist_under_an_atmosphere_and_boil_off_in_vacuum`; 79/79 native, wasm builds.

---

## 2026-07-11 — Layered planets: the molten core EMERGES from pressure (docs/25)

**What.** Planets are now DECLARED as their real construction and nothing else: concentric layers of real
materials (Earth: iron inner/outer core, peridotite mantle, basalt crust — PREM densities; Moon: small
iron core, peridotite mantle, basalt crust) with the observed geotherm as declared data. Everything else
is COMPUTED:
- **Gravity g(r)** — Gauss's law over the enclosed layer mass (peaks at the core boundary, zero at centre).
- **Pressure P(r)** — hydrostatic equilibrium integrated from the surface. Earth's centre comes out
  **≈360 GPa** (real: 364) and the core–mantle boundary ≈135 GPa from the declared densities alone.
- **PHASE** — each material got a pressure-dependent melting curve (Simon–Glatzel, published fits, new
  `thermal.simon_a/simon_c` in the materials DB). Phase = local temperature vs T_m(P). **Never assigned.**

**The emergence result (Robin's challenge: "that should be a natural artefact of gravity/mass/material
if we didn't fudge the composition"):** Earth's inner core comes out **SOLID** even though it is HOTTER
than the molten outer core — because the computed pressure pushes iron's melting curve above the
geotherm exactly there — while the outer core comes out **MOLTEN** and the mantle solid. The melt curve
crosses the temperature profile at the real inner-core boundary. Also: the declared layer densities
integrate to Earth's real mass and 9.8 m/s² surface gravity; the Moon's outer core comes out molten at
lunar pressures (flagged: the real lunar core is Fe–S, which melts lower than our pure-iron entry — we
use the upper published selenotherm; an Fe–S material is the refinement).

**Wired into the impact:** the materialized clouds now sample the layered bodies — each particle knows
its material (basalt crust / peridotite mantle / iron core) and its REAL internal temperature, so
excavating deep matter exposes rock and iron that glows because it genuinely is that hot. Earth's cap
reaches the top of the molten outer core (a Moon-scale impact digs that deep). Each fragment renders in
its own material's reflectance — the excavated composition is visible.

**Continents & oceans:** the render shell samples a 10°×10° land/ocean mask matched to the ~9° grain
spacing ("average area particles") — granite continents, water oceans, real reflectances. Honesty flags:
the hand-digitized mask over-represents land (~37% vs the real 29% — a cited dataset is the refinement);
ocean depth (~3.7 km) is far below one grain, so at this LOD water is the material of a grain's surface,
not a resolved layer; no planetary rotation yet, so the mask's orientation is arbitrary but consistent.

**Verified.** New `planet.rs` tests: declared composition → real mass + surface gravity; hydrostatics →
real central/CMB pressures; **molten-outer/solid-inner core emergence**; lunar molten outer core + solid
mantle; landmask places the major continents/oceans with a plausible area-weighted land fraction.
77/77 native, wasm builds.

---

## 2026-07-11 — Gauss interior gravity, emergent incandescence, and Earth rendered as matter

**What.** Three fixes, each traced from an on-device observation to missing physics (never a visual patch):
1. **Interior gravity obeys Gauss's law.** Debris that ploughed beneath the surface was sucked into the
   core ("the balls absorb into the centre") because the point-mass 1/r² is only valid OUTSIDE a planet.
   Inside, only the enclosed mass pulls: g(r) = GM·r/R³, linear to ZERO at the centre. The gravity source
   is now an extended body (1/r² outside its radius, Gauss interior inside) — no singular attractor.
2. **Incandescence is emergent — the hand-deposit pipeline is GONE from the planetary impact.** The
   impactor's fragments now simply CARRY the true contact velocity (they are the arriving body); the one
   contact law transfers momentum into the target's materialized matter, and the contact DISSIPATION
   (damping + friction) is routed into temperature (`granular::contact_dissipation` — energy is conserved,
   not destroyed, docs/20). A hard impact glows because the matter genuinely got hot. Measured: the cloud
   goes 83% → **100% gravitationally bound** through the collision, hottest fragment ~41,600 K (flagged:
   melt/vaporization energy sinks are not yet modelled at this scale, so the peak overshoots — the glow
   is real physics, the exact peak is not yet).
3. **Earth renders as its matter.** A smooth sphere is a representation lie once matter can be excavated —
   it hides the damage. Earth now draws as a shell of ~512 coarse grains (the honest low-res look);
   grains inside the materialized impact region are hidden, so the excavated void IS the crater and the
   glowing cap particles are the matter that used to fill it. Reset now un-shatters properly. Cosmetic
   skinning (an elastic surface over the blocks) is deferred until after the physics visuals are right.

Also: descent-follow camera (pure camera work — reads `moon_distance_km` from the N-body state, starts on
the whole-orbit framing, glides to a close-up at 25% of lunar distance as the Moon falls; manual zoom
overrides, Drop/Reset re-engage).

**Why.** Robin: "Do not bandaid visuals — fix the physics and then visualize them." Each visual wrongness
was a physics gap: no interior Gauss law, dissipation not becoming heat, a summary representation
(the sphere) hiding real state.

**Verified.** `interior_gravity_follows_gauss_law_not_a_point_singularity` (half depth ⇒ half g; centre
pulls ~nothing; exterior unchanged); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(100% bound, hottest ≫ visible-glow threshold — incandescence emergent); 72/72 native, wasm builds.
On-device: Robin confirms the impact now reads correctly ("Much better!").

---

## 2026-07-11 — ONE collision law for all matter + the mutual impact + conservation-law contact state

**What.** Three connected pieces, closing out Robin's "this must define ALL collisions of ALL matter":
1. **The canonical contact law now governs aggregates too.** `granular::contact_accel` (spring + damping +
   Coulomb friction + cohesion — the physics of record for terrain grains and GPU debris) is now the
   contact force inside `Aggregate` as well; the new `granular::contact_from_material` is the ONE mapping
   from a real material (Young's modulus → stiffness, restitution → damping, friction, cohesion) to
   contact behaviour, used everywhere. Aggregate particles previously had gravity and bonds but NO
   contact — they interpenetrated freely, which is why the shattered Moon was an "exploding sphere in a
   vacuum". A surface velocity rule I'd added to compensate ("cancel the inward component") was a fudge
   (Robin caught it) — deleted; the bulk planet is now a conservative penalty boundary (a force, −∇U).
2. **The mutual impact (`impact.rs`).** At the strike we materialize BOTH bodies at the interface — the
   Moon as a rubble ball on the surface AND Earth's impact region as a cap of crust (same grain mass) —
   and deposit the Moon's real momentum + energy into the *combined* cloud via the same
   momentum/shock-heat/vapor pipeline as the terrestrial meteor. Earth's matter absorbs most of the
   momentum; crater, ejecta, fallback all emerge from the one contact law. Measured natively: **93% of
   the cloud stays gravitationally bound** — as the declared energetics demand (≈2e7 J/kg deposited vs
   ≈6.3e7 J/kg to unbind).
3. **Conservation-law contact state.** Robin observed "a large percentage of debris escapes" on-device,
   yet the model said 93% bound — the discrepancy was the INPUT: in fast-forward, the deposit used the
   Moon's velocity *after* a ~2000 s step that had carried it far past the surface — a garbage sample
   (**21,822 m/s vs the true 9,870 m/s → ~4.9× the honest energy**). New `orbit::contact_velocity`
   recovers the true state at the surface from the two-body conservation laws (vis-viva energy + angular
   momentum), dt-independent. The simulation FORECASTS the collision; it never samples garbage.
   Also: the frame now STOPS at a detected collision (the render can never show a body sailing past its
   own impact — the simulation drives the visualization and interrupts it).

**Why.** "Get the small stuff right, apply everywhere": one contact law, derived from declared material
parameters, at every scale — and dt-independence as a principle: what the physics concludes must not
depend on how coarsely we stepped or how fast the visualization runs.

**Verified.** `aggregate_particles_collide_via_the_canonical_law_and_conserve_momentum` (no pass-through,
momentum conserved, real rebound); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(93% bound after the contact plays out); `contact_velocity_recovers_the_true_impact_speed_regardless_of_
step_size` (recovery within 2% at the browser's coarsest step vs +121% for the post-step sample);
71/71 native, wasm builds. Visual verdict pending on-device.

---

## 2026-07-11 — Materialize the Moon at impact: honest momentum, real 1/r² fall-back, incandescence for free

**What.** With tunneling fixed, the Moon reached Earth but "dinked on top and fell out the bottom, intact,
no ejecta" (Robin, on-device). Three honesty bugs in the shatter, all now fixed:
1. **Momentum was being dropped/mis-deposited.** First cut drove the debris with the Moon's full *incoming*
   momentum → the whole clump shot DOWN through Earth. I then tried zeroing the momentum — Robin caught it:
   *"drop the momentum sounds like fudge again."* Right. The honest model (Robin's framing): from orbit the
   Earth/Moon are *"really big single particles, an average of physical material properties"*; at impact we
   **materialize** the Moon into its constituent matter, and the fragments carry the Moon's REAL incoming
   velocity (Σmᵢv = m_moon·v — momentum conserved across the promotion, not dropped). The impact ENERGY
   disperses them symmetrically (net-zero momentum). **Earth's surface then transfers the inward momentum by
   CONTACT** — the same swept CCD primitive, now applied per-fragment (inward fragments stop on the ground →
   momentum to Earth; outward ones eject). "Get the small stuff right, apply everywhere."
2. **Fall-back/escape was faked by a uniform gravity field.** A baked uniform "down" wrongly forces even
   >escape-velocity fragments to fall back. Robin: *"Model parameters declare REAL physics… we MUST be
   faithful."* Replaced with a real **point-source 1/r² pull toward Earth's actual centre** (G·M_earth,
   from the masses the model already declares), softening kept tiny (2% R⊕) so the field is faithful where
   fragments live (r ≥ R⊕, contact-enforced). The escape/fall-back split is now EMERGENT, not imposed.
3. **Incandescence now comes free from the thermal state.** `deposit_impact` already computes each
   fragment's temperature; the space shader gained a self-emissive term and the debris is tinted by a
   blackbody ramp of its REAL temperature (dark→red→orange→white). Hot ejecta glows on the night side —
   nothing scripted.

Also added **📷 Earth / 📷 Moon** camera buttons (explicit frame-of-reference switch; "Camera on Moon"
frames the impact site once it shatters) toward Robin's zooming-FoR / "fixed camera 1000 m above the site"
goal.

**Why.** The vision, sharpened by Robin across this session: *all* Newtonian-scale laws should EMERGE from
faithfully-modelled matter — the engine answers "what if the Moon deorbited?" by tracking real materials,
so a child (and, ultimately, an embodied AGI) could *re-derive physics* from playing in it. Every fudge is
a false lesson. So the escape boundary must come from √(2GM/r), not a tuned knob.

**Verified.** New native test `aggregate::point_source_gravity_splits_escape_from_fallback`: a fragment
launched at 1.4×escape leaves for good (>10 R⊕); at 0.6×escape it arcs back and lands (apoapsis <2 R⊕) —
the threshold read straight from the declared M and G, with surface contact mirrored as in the render.
`cargo test -p engine` 66/66; wasm builds + deployed. The shatter VISUAL (scatter + glow) needs on-device
eyes. FLAGGED next: apply the swept CCD to the GPU granular contact (retire the `V_MAX` cap) — same primitive.

---

## 2026-07-11 — Swept collision (forecast the path): the dropped Moon no longer TUNNELS through Earth

**What.** The dropped Moon was shooting straight through the Earth and never colliding. Root cause (Robin
diagnosed it exactly): in fast-forward the Moon moves > an Earth-diameter per step, so the DISCRETE
contact test (are the surfaces overlapping *this* sample?) sees it outside at both samples and misses the
collision entirely — the trajectory was effectively faked, riding on a detection that never fires.
Fixed with **swept continuous collision detection**: `orbit::swept_first_contact(rel_old, rel_new, r_sum)`
solves for the fraction `t∈[0,1]` at which the body's straight path FIRST enters the contact sphere —
*when* it hits — regardless of step size. `OrbitDemo::render` now captures each moon's pre-step position,
runs the swept test after the step, and intervenes at the first-contact point (parking the point mass
there and, for moon 0, triggering the Stage-A shatter at the true impact site/energy).

**Why.** Robin: *"forecast with the simulation (know what will happen in real physics), model it with the
visuals"* and *"there is a difference between what we can render and what we can simulate."* The
simulation must KNOW the continuous path intersects the planet even when we sample/render it coarsely —
what we simulate must not depend on how coarsely we look. And: *"get the small stuff right, APPLY
EVERYWHERE"* — `swept_first_contact` is a pure-geometry primitive (segment vs. sphere), not orbit-specific.
The SAME tunneling is why the grain sim caps the vapor front at `V_MAX` (a workaround); CCD is the honest
general fix there too — flagged as the next application of this primitive to the granular contact.

**Verified.** `orbit::swept_contact_catches_a_body_that_tunnels_through_the_planet` (a −5→+5 pass through
the centre — both endpoints outside — is caught at t=0.4; a clearing path is `None`; already-inside is
t=0). `cargo test -p engine` 65/65; wasm builds. The Moon-collides VISUAL still needs on-device eyes.

**Open.** Apply CCD to the GPU granular contact (replace the `V_MAX` ejecta-speed cap) — the same
primitive, the "everywhere". And the Stage-A shatter visual + Stage B zoom-in remain to verify/build.

---

## 2026-07-11 — Moon-shot Stage A: the dropped Moon SHATTERS (emergent), instead of merging

**What.** In the space band (`OrbitDemo`), the de-orbited Moon now **shatters into a debris cloud** on
impact rather than the point-mass sphere silently merging into Earth. The frame the Moon first strikes,
its point mass becomes a **self-gravitating aggregate** of 64 basalt fragments filling the Moon's volume
at the impact site (`build_moon_debris`), and the impact energy — captured honestly at contact (~4.5e30 J)
— is deposited via the same `aggregate::deposit_impact` pipeline (momentum + shock heat + vapor). Because
that energy is ≫ the Moon's binding energy, the aggregate DISPERSES — no scripted destroy, just kick vs.
binding (docs/21). The fragments then arc under Earth's gravity (uniform toward its centre), some flying
out, some falling back — the ejecta curtain at planetary scale. They render as small basalt spheres at
their real positions; the intact Moon sphere stops drawing. The debris steps at a FIXED observable rate
(`DEBRIS_DT`, a time-LOD) so the fine event plays out at human speed, not the celestial fast-forward that
would disperse it in one frame.

**Why.** The moon-shot (docs/23): "de-orbit the Moon into [a spot], then zoom … and observe it was
destroyed" — with NO code that says destroy. The drop, the fall, the surface contact, and the honest
impact-energy accounting all already existed and were native-tested; the collision just rendered as two
spheres merging + a HUD number tagged "not yet materialised." This wires the tested aggregate-disruption
physics into the render so the shatter is finally *seen*, emergently. It's the celestial half; Stage B is
the zoom-in that materialises the local crater/ejecta from the same conserved energy (docs/19).

**Verified.** `cargo test -p engine` 64/64; wasm builds. The disruption physics itself is native-tested
(`aggregate::energy_above_binding_disrupts_it`, `an_impact_heats_the_core_and_shatters_the_aggregate`);
`build_moon_debris` feeds it the real impact energy. **The VISUAL is NOT yet verified** — a rendering
change can't be checked headlessly (docs/19: "needs on-device eyes"). Needs Robin's eyes + tuning of
`DEBRIS_DT`, `DEBRIS_N`, fragment size, and the fall-back/escape balance.

**Open.** Earth-side damage is still only the HUD verdict (no crater visual on the Earth sphere yet); the
debris external gravity is a uniform approximation (fine near the impact, coarse as it spreads); and the
whole thing is the CELESTIAL band only — the scale-relative zoom-in (Stage B/C) is still unbuilt.

---

## 2026-07-11 — Cohesive grain contact (the frictionless-graze fix, one property doing three jobs)

**What.** Added an ATTRACTIVE adhesion term to the grain contact law (GPU shader + native `granular.rs`
force-of-record): net normal force = repulsive spring − cohesion, so touching grains can now BOND (the
force pulls them together) until the bond lets go past a short range. `cohesion = 0` recovers the exact
old push-only contact. The friction load now includes the cohesion, so a touching/grazing pair has a
real normal load — and therefore friction. `c_cohesion` is derived from `Material::cohesion`, converted
to a per-mass adhesion and capped at a granular ceiling (loose debris is already fractured — rock grains
keep only surface adhesion, they must not re-weld into solid). Reused the dead `c_max_accel` param slot,
so no struct-layout churn.

**Why.** Robin caught that a grain placed at *exactly* zero overlap grazes frictionlessly — friction is
`μ·N` and `N = k·overlap = 0` there. Her instinct: "surely there's a property of matter that ensures
this never happens unless the particles are separated?" There is — **cohesion**, a real material
property already in `materials.json` that we used for solid bonds but not loose-grain contact. It closes
the graze (touching ⇒ bonded ⇒ normal load ⇒ friction), it's *why* soil holds a slope dry sand can't
(the same thread as the granite-cliff/talus split), and it's part of what holds a planet together
against its own gravity — a prerequisite for the Moon-onto-vacuum-Earth moon-shot.

**Verified.** New `gpu-verify` foundational test **F8**: a gentle separating nudge is HELD by the bond
(1.00 m), a hard nudge BREAKS it (6.84 m), a cohesionless pair DRIFTS apart (2.00 m). Native
`granular::cohesion_bonds_touching_grains_and_raises_friction`: a just-touching cohesive pair is pulled
together and a zero-compression graze has friction, while dry stays frictionless. `cargo test -p engine`
64/64; wasm builds; no scene regression (scenes default to cohesion = 0).

**Open.** Cohesion is a single representative value (like friction) — a per-particle/mixed-material
cohesion is a later refinement; and terrain contact doesn't yet carry cohesion (grains adhering to the
ground), flagged.

---

## 2026-07-11 — Emergent impact end-to-end: momentum-conserving contact, terrain-as-matter, drag fudge deleted

**What.** A long arc turning the impact from scripted fudges into emergent particle physics (`docs/24`),
capped by a foundational test suite that caught a core-model bug and a fix that scaled the whole thing.

- **Terrain-as-matter (Path B).** The meteor no longer carves a crater and scripts ejecta velocity. It
  MATERIALIZES the impact region into grains at rest (`matter::materialize_region`), deposits the
  meteor's real momentum as an impulse (`deposit_impulse`), and the rest of ½mv² as radial-gradient
  **shock heat** (`deposit_shock_heat`, filled core-first so a sub-grain impactor's energy actually
  vaporizes a plasma core instead of smearing below threshold). Vaporized matter **expands** and throws
  the ejecta (`deposit_vapor_expansion`) — Robin's insight that at 17 km/s the crater is driven by phase
  transition (gas pressure), not elastic rebound; the KE was already in the sim as shock heat we were
  radiating away. Added estimated thermal data for the granular soils (they couldn't vaporize before).
- **Momentum-conserving contact solve (the core fix).** A two-particle test (`gpu-verify` F5) exposed
  that the directional-implicit solver damped each grain's ABSOLUTE velocity, so a 20 m/s head-on
  collision lost ~74% of its momentum — hidden by every pile/crater scene (slow ⇒ ~0 COM velocity).
  Fixed by a derived neighbor-coupling term `Sv_nbr = Σ S·v_neighbor` in the RHS; the pair's COM velocity
  now telescopes to conserved. This alone made stepped terrain conserve energy and DEEPENED the emergent
  crater ~5× (3 m → 14.7 m) — ejecta finally keep their momentum.
- **Conservative terrain + steep materialization.** Replaced the min-translation terrain penalty (whose
  normal FLIPPED at voxel edges, injecting energy) with a smooth bilinear surface and the exact −∇U
  penalty. Vertical walls a heightfield still can't represent become grains
  (`materialize_steep_terrain`) — but only where the material is too weak to hold a cliff (critical
  height ≈ σ/ρg): dirt slumps to talus, **granite holds as a real cliff** (Robin's antithesis, emergent
  from strength).
- **DRAG FUDGE DELETED** (`matter::DRAG` 0.9995 → 1.0). It bled 62%/s of a vacuum particle's speed
  (foundational test) and was masking the non-conservative terrain; with the three fixes above the core
  no longer needs it — a vacuum particle keeps its momentum.
- **Everything couples honestly.** `aggregate::deposit_impact` (probe/bodies) rewritten to the SAME
  pipeline (momentum + heat + vapor) — the last scripted `√(2·0.3·e/ρ)` kick is gone; the meteor couples
  into EVERY body via `couple_impact_to_bodies`, not a hardcoded probe. Removed the cosmetic
  `spawn_vaporized_meteor` (a scripted 22 m/s clump that looked like an intact meteor and double-counted
  momentum). Restitution is now derived from `Material::restitution` via a θ-method contact integrator.

**Why.** Robin's directives drove it: *"trust physics; hunt for what we're missing, don't fudge"*;
*"crater size should be emergent and observable, not imposed"*; *"a meteor is an exaggerated test of the
same physics as a footfall or a feather"*; *"no fudge in the core models"*; and the clincher — *"test
every aspect of the fundamental interactions of particles; get the small stuff right and the universe
scales."* The foundational suite proved it: a two-particle collision plus a derivation beat the solver
that "looked stable and passed every scene."

**Verified (RTX 2070 headless + native).** New `gpu-verify` foundational block F1–F7 (Newton's 1st/2nd/
3rd, momentum-conserving collision, friction≈μg, touching↔separated sweep) all PASS in true vacuum;
grain-grain energy conservation (I-flat), stepped-terrain conservation (I), emergent crater (M, 14.7 m
deep). `cargo test -p engine` 63/63; wasm builds. New native tests: materialize/impulse/shock-heat/vapor
conservation, steep-terrain materialization + the granite-cliff antithesis, restitution→damping.

**Open (honest, flagged).** (1) A granite cliff a heightfield still can't contact conservatively should
become a COHESIVE aggregate (grains + bonds) — rigid AND conservative. (2) Friction runs ~35% strong
(F6 ratio 1.35 vs μg) — the same over-sticky friction behind the repose under-/over-prediction (scene
D). (3) Crater size is LOD-capped (materialize cap) below the physical scale. (4) Dissipated energy →
heat → radiation still dropped (flagged in-shader). (5) Soil thermal values are composition estimates,
not cited.

---

## 2026-07-09 — North star + a reverted fudge; the engine's name: "Integrity"

**What.** While bringing GPU debris up (docs/22), a play-test exposed that a meteor doesn't destroy the
probe, and I reached for a special case — `if probe within crater { obliterate }`. Robin: "if
everything is real, the probe should have just been destroyed on impact on its own; the fact we have to
correct that concerns me." She's right — that's a **fudge**. **Reverted it.** The real problem: the
probe is the **last bespoke object** (a rigid `body::Sphere`), not matter, so `matter::impact` can't
see it. Wrote `docs/23`: the north-star demo — **a metal ball at ground zero, de-orbit the Moon into
it, zoom in and observe the ball was destroyed** — with NO code that says "destroy the ball." It's
destroyed because the impact energy really reaches it and exceeds iron's thresholds (`damage`). The fix
is to make the probe **real matter** (a cohesive aggregate / voxel body), so gravity, contact, impact,
`damage::classify`, and emission all act on it emergently — no special cases.

**Name.** Robin is naming the engine **"Integrity"** — fitting: it's the operating invariant (every
value traces to real physics or is flagged; no fudge), and reverting the special case is exactly it.

**Also (shipped this session, verified/native):** terrain now uses planetary **surface gravity**
(uniform down, not the slab's micro-g self-gravity — fixes debris concentrating at the world centre;
real-time, no time-scale). The GPU debris path works on-device (FPS fixed, debris glow + cool).

**Open (honest):** probe-as-matter unification (`docs/23`); GPU resting-debris re-deposition
(iteration 3, kills the moiré pile-up); the celestial→local materialization for the zoom-in.

---

## 2026-07-09 — Bodies as particle aggregates (emergent binding + disruption)

**What.** Started making celestial destruction a *simulation, not a mock* (`docs/21`). A body becomes a
**cloud of particles held together by its own gravity** (`aggregate.rs`): softened N-body self-gravity,
`binding_energy` (Σ G·m·m/r), `kinetic_energy_com`, `rms_radius`, `com`. Verified that a cold cloud
**holds together** (cohesion emerges from gravity — the `docs/15` roundness invariant) and that an
energy kick **above the binding energy disrupts it** (emergent dispersal — the identity behind a
shattered moon).

**Why.** Robin asked, pointedly, whether the impact destruction is *inherent in the engine's model* or
"just mocks to humor me." Honest audit: the terrain meteor IS real emergent simulation (per-voxel
fracture/melt/vaporize from material + energy, glowing by computed temperature). But the **celestial**
Moon-crash was NOT simulated destruction — the bodies were point masses drawn as spheres, so there was
no matter to break, and I was about to build a scripted "fireball" — a **mock**. Stopped: that violates
the honesty invariant. The honest path (Robin chose it) is bodies-as-aggregates, so the shatter is the
same gravity that rounds them, run past their binding energy — no script.

**Verified.** `aggregate::a_self_gravitating_cloud_holds_together`,
`aggregate::energy_above_binding_disrupts_it`. `cargo test` 44/44; clippy `-D warnings` clean; fmt
clean.

**Honest scope.** This is the gravitational *skeleton*. Per-particle material + temperature, the impact
coupling (deposit energy → `damage::classify` per particle → emergent debris/melt/vapor), and the
rendering are the next slices (`docs/21`). Until they land, the *visible* Moon-crash still shows the
momentum stick — and we will NOT fake the shatter in the meantime.

---

## 2026-07-09 — Phase classes integrated into matter::impact; Moon-speed readout

**What.** `matter::impact` now classifies each ejecta via `damage::classify` (the thermodynamic
thresholds): a carved voxel is at least Fractured, the hot core Melts, the hottest Vaporizes. The class
drives behaviour — **vaporized** ejecta expand away fast (gas/plasma, `VAPOR_EXPANSION`), all glow by
temperature. The crater extent stays the budget model (LOD bridge `docs/19` intact). Also added a live
**Moon-speed readout** (km/s relative to Earth) to the space-band HUD.

**Why (speed readout).** Robin saw the Moon's velocity seem to "flatten as if terminal velocity in a
vacuum." Checked the orbit path: there is **no drag, clamp, or damping** anywhere — the only velocity
changes are the verlet kicks and surface contact. So there is no terminal velocity; the physics is
honest. The apparent flattening is either Kepler's 2nd law (a *partial* brake makes an eccentric orbit
that slows at apogee — the opposite of drag) or the compressed time-scale hiding the final fast plunge.
The speed readout makes it observable: on a true **Drop** it climbs toward ~11 km/s at impact; use ⏪
slower to watch it accelerate.

**Verified.** New `matter::a_colossal_impact_vaporizes_the_core` (core passes basalt's boiling point →
Vaporized class). `cargo test` 41/41; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
deployed.

**Honest caveat (`docs/20`).** Crater excavation and shock heating still use separate energy
accountings (a flagged simplification — full coupled conservation is the MLS-MPM/shock-EOS future).

---

## 2026-07-09 — Visual: glowing molten ejecta + a Meteor you can fire

**What.** The first visible slice of impact damage beyond text (`docs/20`): impact ejecta carry a
temperature, and molten debris **glows by black-body emission from that temperature**. Added
`Particle.temp_k`; `matter::impact` deposits heat that peaks at the contact and falls to cold at the
crater rim (centre melts/vaporizes, rim is cold rubble — the honest radial gradient); `emission::
incandescence(temp_k)` maps K → an added RGB glow (dull red → orange → yellow → white); the particle
shader **adds** it, so hot debris self-illuminates even on the dark side (it *emits* because it's hot —
the analogue of illumination × reflectance, `docs/17`). A **Meteor** control (`Engine::meteor`, the
`☄`/`m` button in the terrain slice) fires a high-energy `impact` you can watch and orbit into.

**Why.** Robin: "see the impact, then zoom in and see the crater" (with glowing melt). Delivered in the
*terrain* renderer (which renders on-device) so it's verifiable now; the celestial→voxel auto-fly-in
(materialising the Moon-crash crater from its summary) stays staged (`docs/19`).

**Verified.** `emission::cold_matter_does_not_glow_and_hotter_glows_brighter_and_whiter` and
`matter::a_big_impact_melts_the_centre_and_leaves_the_rim_cold`. `cargo test` 40/40; clippy
`-D warnings` clean; fmt clean; wasm + `tsc` green; deployed. The *look* of the glow is for Robin's
on-device check.

**Honest caveat (`docs/20`).** The crater extent is physical (energy/σ), but the ejecta *temperature*
distribution is a first visual model — the energy is not yet conserved through the phase change, and
`incandescence` approximates the Planckian locus. Next (Robin's order): integrate the phase classes
into `matter::impact` proper (voxels → gas/melt/ejecta, energy-conserving), then MLS-MPM.

---

## 2026-07-09 — Impact thermodynamics: fracture → melt → vaporize (one rule)

**What.** Modelled fragmentation, melting, and vaporization as **one data-driven response** (`docs/20`),
Robin's planetary-scale test of the engine (and of scale-of-detail). An impact deposits **energy
density** (J/m³ = Pa); each parcel's fate comes from comparing it to that material's own thresholds:
fracture strength → melt energy `ρ(cΔT+L_f)` → vaporization energy. `damage::classify` returns
`Intact | Fractured | Melted | Vaporized` — the *same* "density vs threshold" logic as fracture, just
higher thresholds. Because the deposited density falls with distance, **one event produces all four at
different radii** (near-field vaporizes, then melts, then fractures, then intact). Added optional
`Material.thermal` (specific heat, melt/boil points, latent heats) with **cited data** for basalt,
granite, iron, water; materials without it can only fracture (we don't claim unknown melt behaviour).

**Why.** Robin: "model fragmentation, melting, vaporization — a test of our simulator's abilities on a
planetary scale (and of our scaling of detail)." A giant impact honestly vaporizes rock near contact,
leaves a magma ocean of melt, fractures/ejects a shell, and — since E ≪ Earth's binding energy — leaves
the planet intact but resurfaced. Every one is the same `classify` at a different radius.

**Verified.** `damage::impact_fractures_then_melts_then_vaporizes_by_energy_density` (thresholds order
σ<melt<vapor; each band classifies right; a giant-impact density vaporizes rock; no-thermal-data →
fracture-only). `cargo test` 38/38; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Staged (docs/20):** integrate into `matter::impact` (voxels become gas/melt/ejecta by class,
conserving mass + energy through the transition); the **visual** display — incandescent melt (black-body
emission from temperature, not a painted colour), a vapor plume, and the materialised crater to fly
into (`docs/19`); cooling/solidification (magma → rock).

---

## 2026-07-09 — Two-moon stress test scene

**What.** A new scene (`/twomoons.html`): two moons on the same orbit, **opposite sides** of the Earth,
that you **de-orbit both at once**. Generalized `OrbitDemo` from one moon to N — `[Sun, Earth, Moon,
Moon2]`, a moon uniform per body, per-moon lighting/framing, and collision resolved Earth-vs-each-moon
with each moon's impact energy counted once (the two hits **sum** in the HUD). `brake_moon`/`drop_moon`
now act on *all* moons; focus cycles Earth → Moon A → Moon B; the second moon is placed at −d with the
opposite tangential velocity so both orbit the same way and stay diametrically opposed. The two HTML
pages share one script — the moon count comes from `<body data-moons>`.

**Why.** Robin: "It's our universe, we might as well play in it." The N-body core (`orbit.rs`) is
already generic, so two moons is nearly free physically; its value is **stressing the collision path** —
two simultaneous surface contacts, symmetric resolution, and (later) two craters materialising at once.

**Verified.** `cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
`/twomoons.html` serves. Visuals (two moons, symmetric de-orbit, double impact) pending Robin's
on-device check.

---

## 2026-07-09 — LOD-adaptive damage: the crater bridge (celestial ↔ voxel)

**What.** Connected the Moon-crash to a real crater across scales (`docs/19`). The bridge: a damage
event is the *same event* at every LOD, so the coarse **summary** and the fine **voxel materialisation**
must agree. Both use the same `σ·V` accounting — `damage::crater_volume(E, σ) = E/σ` (celestial
summary) equals the voxels `matter::impact` excavates (proven:
`matter::voxel_crater_matches_the_coarse_damage_summary`). Added honest **regimes**: strength crater
(`V=E/σ`), gravity regime (flagged, unmodelled), and **disruption** past the body's binding energy.

**Honesty — the Moon is not a tidy crater.** ~4.5e30 J is ~36× the *Moon's* binding energy (the Moon
**shatters**) but only ~2% of the *Earth's* (~2.2e32 J), so the Earth **survives with a planet-scale
crater** — the giant-impact regime, not a neat bowl. The space-band HUD now says exactly this on impact
(`damage::moon_shatters_but_earth_only_craters` pins the numbers). We report the regime honestly instead
of promising a crater the physics forbids.

**Why.** Robin: connect the Moon-crash to a real crater. The honest connection is the σ·V bridge — the
same relation drives the celestial summary and the zoomed-in voxel crater, so promoting/coarsening
across LOD conserves the event (`docs/13`). The *visual* zoom-in (fly the camera down and materialise
the voxel crater) is a real renderer effort, designed in `docs/19`, staged for on-device work.

**Verified.** New: `damage::crater_scales_with_energy_and_inversely_with_strength`,
`damage::moon_shatters_but_earth_only_craters`, `matter::voxel_crater_matches_the_coarse_damage_summary`.
`cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Roadmap (Robin's order):** LOD (this — bridge done; visual zoom-in next) → MLS-MPM → fluid. Planned
playground: a **two-moon** scene (opposite sides, same orbit, de-orbit both at once) as a stress test —
the N-body core is already generic, so it's nearly free.

---

## 2026-07-09 — Unified deformation & damage: the design + first honesty slice

**What.** Started the deformation/damage subsystem (`docs/18`) from Robin's requirement that a **bullet,
a pebble in a pond, and the Moon hitting the Earth be the SAME operator** — differing only in
parameters and level of detail. The design names two invariances: (1) **material** — the response comes
from constitutive data (solids fracture at strength, granular media crater, liquids yield at ~0 and
flow), so bullet-in-rock and pebble-in-pond are one call with different material; (2) **scale/frame** —
the observer's frame/zoom decides what is materialized (celestial: energy/momentum + crater summary;
zoom in: voxel fracture + ejecta; zoom way in: grains/buildings), promoting/coarsening across LOD while
conserving mass/momentum/energy. Two concrete slices landed: (1) parse material **`phase`** and fix the
liquid fudge — water's `fracture_strength` used to fall back to `1e12` (stronger than granite!); a
fluid now yields at ~0. (2) `MatterSim::impact(site, direction, energy)` — the **generalized
energy-driven impact**: it spends the impact energy fracturing voxels nearest-first (σ·V per voxel), so
bigger energy → bigger crater, stronger material → smaller crater, and a liquid splashes. A 10 g bullet
(~450 J) and the Moon (~4.5e30 J) are the *same call*.

**Why.** Robin: the same system should observe a bullet, a pebble in a pond, or a planetary impact —
and at a given scale we simulate only what the observer can perceive (buildings only matter zoomed way
in; ejecta only zoomed in; celestial scale cares about energy/momentum and a crater summary). This is
the honest unification of the voxel-fracture model (`matter.rs`) with scale-relative fidelity
(`docs/13`, `docs/08`) — the endpoint is MLS-MPM with per-phase constitutive models.

**Verified.** `materials::a_liquid_yields_where_a_solid_resists` (a fluid yields to a poke a solid
withstands) and `matter::impact_is_material_and_scale_invariant` (same energy craters dirt but not deep
granite; more energy → bigger granite crater; a gentle impact still splashes a pond). `cargo test`
34/34; clippy `-D warnings` clean; fmt clean; wasm compiles.

**Roadmap remaining (docs/18):** fluid flow (needs a viscosity field, not in the DB yet) → MLS-MPM
constitutive unification → LOD-adaptive damage (summary ↔ detail on zoom). Robin: "we should get to the
rest before we're done."

---

## 2026-07-09 — Orbital-decay control: brake the Moon until it crashes (with real collision)

**What.** The requested experiment — slow the Moon and watch its orbit decay into the planet — plus the
honest physics that makes a "smash" real rather than a numerical explosion. `orbit::resolve_contact`
adds **surface collision**: two solid bodies stop when their surfaces meet (perfectly inelastic,
momentum-conserving), instead of tunnelling through each other as point masses into a 1/r² singularity
— the celestial echo of the voxel body contacts (`docs/16`). `orbit::perigee` computes the live
closest-approach so the HUD can show the orbit tightening. OrbitDemo exposes `brake_moon` (halve the
Moon's velocity relative to Earth), `drop_moon` (cancel it → radial plunge), `reset_moon`, plus a
variable **time multiplier** in the HUD. The web control bar gains Brake / Drop / Reset + slower/faster,
and the HUD shows perigee (reddening below Earth's radius) and "💥 IMPACT".

**Why.** Robin wanted to watch the Moon smash the Earth. The honest lesson is built in: in a
conservative two-body system a *single* halving does NOT crash — it drops into a tighter eccentric
ellipse (perigee ~55,000 km, still a miss); it takes a few brakes (or a full drop) to push perigee
below the surface. Real orbital mechanics, shown, not faked. Also exposed the time multiplier per
Robin's note (and it lets you slow time to watch the impact).

**Verified.** `cargo test` 31/31 — including `perigee_tracks_how_hard_the_moon_is_braked` and
`a_dropped_moon_crashes_into_the_planet_and_stops_at_the_surface` (it reaches the surface and rests
there, no tunnelling). clippy `-D warnings` clean; fmt clean; wasm + `tsc` green. Visuals pending
Robin's on-device check.

**Impact energy (honesty).** Robin noted that at these masses an impact must do *damage* — and that a
perfectly-inelastic "stop at the surface" silently *deletes* the kinetic energy, which is itself a
fudge. So we now **measure and report** it: `orbit::inelastic_dissipation` (the KE the collision
removes) and `orbit::binding_energy`. A dropped Moon hits at ~11 km/s → ~4.5e30 J ≈ **36× the Moon's
gravitational binding energy**; the HUD shows this and states plainly that both bodies would be
destroyed. We measure the damage rather than hide it or fake it.

**Honest scope note.** "Collision" here is surface contact + inelastic stop, plus the reported impact
energy; actual **fragmentation** (deformation, melt, debris, merging) is a future subsystem — the
honest zoom-in unification of the voxel-fracture model (`matter.rs` `fracture_strength`) at scale.
Flagged, not faked.

---

## 2026-07-09 — Live real-Sun lighting, selectable focus frame, scene picker

**What.** Wired the real Sun into the *live* space band (following the validated physics): the demo now
simulates `[Sun, Earth, Moon]` with the Earth on its true ~29.78 km/s heliocentric orbit and the Moon
co-moving. The shader's light direction is now computed per-body **from the Sun's actual position** (no
more hardcoded direction), so the lit hemisphere and the Moon's phases are geometric. The Sun isn't
drawn at this zoom (~23,000 display units off-frame) — it is the *light source*, the scale-adaptive
choice (`docs/17`). Added a **focus control**: the viewport is a physical frame of reference
(`cycle_focus` / `focus_label`), re-centring the whole view on Earth or the Moon. And a **scene picker**
(`web/src/scene-nav.ts`) injected on both pages to switch between the terrain slice and the space band.

**Why.** Robin's direction: a real Sun should light the system (not a fake light), the viewport is a
physical frame of reference with a selectable focus, and the app should let you choose between scenes.
All three are honest, emergent-from-real-state changes (`docs/17`).

**Verified.** `cargo test` 29/29; clippy `-D warnings` clean; `cargo fmt` clean; wasm builds and
`tsc --noEmit` passes (focus + scene-nav bindings). **Visuals pending Robin's on-device check** —
headless WebGPU can't render here, so the appearance of the sun-lit bodies and the focus/scene UI is
for iPad confirmation.

---

## 2026-07-09 — Honest appearance: no painted tints, brightness from light, a real Sun

**What.** A user play-test of the space band exposed fudging: the Earth was a hardcoded ocean-blue
tint and the Moon a hardcoded grey — cosmetic colours touching no material data, even though the
terrain already colours voxels from real `materials.json` albedos. Replaced with honesty (`docs/17`):
(1) body colour = **aggregate albedo of a real composition** via the new `materials::aggregate_albedo`
operator (Earth = ocean water + continental granite + polar ice; Moon = basalt) — a computed summary,
not a paint job; (2) the space shader now does **illumination × reflectance** (bright sun × real, often
dark, albedo) + Reinhard tone-map, so a dark-but-lit body reads bright — the honest reason the Moon
looks bright; (3) added a validated **Sun–Earth–Moon** physics test: a real Sun (1.989e30 kg, 1 AU) and
the Earth given its **appropriate heliocentric velocity** (~29.78 km/s), with the Moon staying bound to
the moving Earth.

**Why.** The user pushed the honesty invariant (`docs/15`) all the way down: *don't fudge*. Key
insights captured: brightness is illumination × reflectance (not a bright material); even albedo is a
summary placeholder for real optics (ray tracing is the goal); zoom-out summaries are fine only if
*computed from everything we know* by one operator for all objects/scales; the illuminant should be a
real Sun; the viewport is a **physical frame of reference** with a **selectable focus** (planet →
Moon → …); and the core research question is whether the system can tell **what matters at a given
scale**. Working principle / candidate name: **"Integrity."**

**Honesty flags (not hidden).** Earth composition excludes the atmosphere → deliberately no Rayleigh
blue (the blue-marble blue is atmospheric, unmodelled); Moon lacks highland anorthosite in the DB → it
renders darker than reality until added; the shader's sun *direction* is still a placeholder until the
real Sun is wired into the live view.

**Verified.** `cargo test` 29/29 (new `aggregate_albedo_summarizes_real_constituents`,
`sun_earth_moon_system_is_bound`); clippy `-D warnings` clean; fmt clean; wasm compiles. The *visual*
result of the new lighting is for on-device confirmation (headless WebGPU can't render it here).

**Staged (larger, honest work):** real Sun as the live illuminant + heliocentric re-centering + focus
switching; ray tracing; specular/BRDF from roughness/metallic; stellar & anorthosite materials;
atmosphere for the earned blue; and the still-owed orbital-decay control.

---

## 2026-07-09 — Unified dynamics: everything not at rest reacts

**What.** Fixed the "probe quits falling / doesn't really react to debris" behaviour by unifying the
probe and the debris into **one awake-set dynamics loop** (`docs/16`). Previously `body::Sphere` (the
probe) and `matter::MatterSim` (debris) were separate systems coupled only through the voxel grid —
`matter.rs` never referenced the probe — so particles couldn't push it and settling debris deposited
voxels *inside/under* it, making it appear to rest on nothing. Now, per substep, every awake body
integrates under the same gravity field, resolves body↔world contacts, debris steps under that field
and **won't deposit inside a body** (piles on it, conserving matter), and **body↔debris contacts
exchange momentum both ways** (`MatterSim::couple_body`). Sleep/wake is structural: a body sleeps only
while in contact and slow, and wakes the instant support is removed or something hits it.

**Why this shape.** The user's principle: a physics loop looks at every object *not at rest* and makes
it react as a natural property of the world and the object, never a per-object script — the honesty
invariant (`docs/15`) applied to dynamics. Also captured the deeper motive: an honest, inferable
physical world is a place to *learn to act* (VR, and plausibly embodied-AI training), a payoff that
exists only to the degree the sim refuses to fake.

**Also (honesty corrections from the user).** (1) No atmosphere is modelled — matter falls through
*vacuum*, so the per-step `DRAG` constant is flagged as a numerical-stabilizer debt, not real air drag.
(2) Compute-budget policy written down: favour larger/more massive objects (massive bodies are
budget-exempt today; debris coarsening must *merge into mass-carrying clumps*, conserving mass on both
spawn and settle — so it's deferred, not half-done, to avoid a mass leak). (3) Noted the
server-authoritative-world / client-sees-a-slice threshold to watch (`docs/11`, `docs/13`).

**Verified.** New native tests: `particle_transfers_momentum_to_a_body` (momentum conserved through the
impact), `debris_does_not_settle_inside_a_body`, `body::wakes_and_falls_when_support_is_removed`.
`cargo test` 27/27; clippy `-D warnings` clean; `cargo fmt` clean; `cargo check --target
wasm32-unknown-unknown` green (the awake-set loop lives in the wasm-only host).

---

## 2026-07-09 — Representation invariant: the cube is a lattice, not a unit of matter

**What.** Answered a foundational design question — "are we baking a core mistake into the engine by
building on cubes, when the universe is made of spheres?" — and locked the answer in as canonical.
Wrote `docs/15`: **a voxel is a sampling cell, never a unit of matter.** The cubic grid is the
coordinate lattice we sample continuous fields on (density, material, momentum), like pixels sample an
image; it is not an ontology of blocks. All physical state lives on matter with continuous coordinates
(`Particle.pos`, `MassPoint`), and bulk voxels dissolve into particles the instant physics touches
them (`docs/08` tiers). Added a **grid-isotropy regression suite** (`isotropy.rs`) to enforce it.

**Why.** The honest answer is that cubes are *not* a foundational mistake — roundness is emergent, not
primitive. Real solids sit on lattices (many cubic — rock salt, BCC iron), yet planets are round
because isotropic self-gravity averages over the lattice; the engine already mirrors this (aggregate
mass → spherical far field in `gravity.rs`/`orbit.rs`; surface nets smooth the render). The *real*
risk is subtler: a regular lattice has preferred directions (axes, 45° diagonals) and a solver could
silently bake that bias into the physics. Also captured the user's north star: the world should **feel
right in VR because it is right, not via per-object fakery** — leave something unsupported and it
falls as a natural property of the world and the object (`find_unsupported` → `collapse`), never a
script.

**Verified.** New suite asserts (a) gravity on a symmetric ball is radial + equal-magnitude across
face axes and edge/corner diagonals (spread < 1%, tangential < 1%), and (b) `dig` carves a true
Euclidean sphere (volume within a few %, equal axis reach, no lateral ejection bias). Proven
**non-vacuous** via mutation testing: an injected axis bias in the gravity sum and a Chebyshev (box)
dig criterion both drove the guards red (gravity spread 9.7%; box removed 8000 vs a sphere's 4189),
then reverted. `cargo test` 24/24; clippy `-D warnings` clean; `cargo fmt` clean.

---

## 2026-07-09 — Space band: watch the Moon orbit (v0.9.0)

**What.** Step A of the scale-relative "orbit-to-ground" (`docs/13`): a spectator view of the real
Earth + Moon (`/orbit.html`). `OrbitDemo` runs `orbit.rs` (real SI, f64) each frame and renders two
lit spheres via a tiny new `space.wgsl` (position/normal + per-body tint + one directional sun, so we
get phases). Metres → display units (Earth radius → 1); the Moon sits ~60 units out. Time-scaled so a
~27.3-day orbit plays in ~20 s, substepped 16× for a stable symplectic step. HUD reads live
separation (~384,400 km). Kept on a separate page + Vite multi-page input so the terrain slice is
untouched.

**Why this shape.** I can't self-verify visuals here (headless WebGPU won't render the pipeline), so I
minimized blind risk: reuse the *proven* GPU setup pattern, the existing sphere mesh + `draw` path, and
lean on the already-validated physics (`orbit::moon_orbits_earth`). The renderer is a thin shell over
known-good pieces; the hard part (the orbit) is the tested part.

**Also.** Wrote `docs/13` (north-star: observer-relative fidelity) and `docs/14` (validation
demonstrations — each physics test mapped to what it proves + how to *show* it), at the user's request
to preserve the test concepts as demonstrations for the full build.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm build compiles `OrbitDemo` warning-
free; `tsc` clean; LAN dev server serving `/orbit.html`. Visuals to be confirmed on-device.

---

## 2026-07-09 — Solid-object collision + orbital-mechanics validation (v0.7.2, v0.8.0)

**Collision (v0.7.2).** From an iPad play-test: the probe clipped into crater walls (looked like a
duplicate ball, rested too high) because it only collided with the terrain column directly beneath
it. Replaced with proper **sphere-vs-voxel collision** (`body.rs`): integrate under gravity, then
iteratively push out of the deepest solid voxel the sphere overlaps (floor, walls, corners) with
restitution + friction. Solid objects act solid.

**Orbital validation (v0.8.0).** Added `orbit.rs` — N-body point-mass gravity + a symplectic
velocity-Verlet integrator. The native test drops in the **real Earth + Moon** (masses, 384,400 km,
1.022 km/s) and confirms a bound orbit: ≥1 full revolution, distance within 15% of real, energy +
angular momentum conserved <1%. This proves the gravity law reproduces real celestial motion — the
"does the Moon orbit the planet?" test — and, importantly, it's a **pure native test** (no rendering),
so it verifies the physics despite headless WebGPU being unavailable here.

**Note on tooling.** Headless Chromium here renders WebGPU only via software (SwiftShader) or hits a
Dawn instance bug on the real GPU, so I can't screenshot the full render; I lean on native tests
(watertight mesh, collision, orbit) + the user's iPad for visual confirmation. `web/screenshot.mjs`
is kept for environments with GPU access.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm + web build green.

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
