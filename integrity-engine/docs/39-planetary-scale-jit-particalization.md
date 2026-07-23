# docs/39 — Planetary scale = one JIT particalization primitive (T0↔T3), Theia first

The design for "figure out planetary scale" (docs/33 §3 stage 6, the multi-LOD planet). This is a design doc
for pressure-testing **before** building. It formalizes ONE scale-invariant primitive and instantiates it on
the Theia giant impact (the highest-energy case), with the human-surface case as the same primitive at lower
energy.

## The one primitive (scale-invariant)

Matter at rest is a **cheap coarse field** (T0). An *event* **particalizes** a region into real particles at a
resolution the event's physics demands; the particles simulate; when they quiesce they **bake back** into the
field, which is a *persistent, renderable record* of the deformation. Three moves, everywhere:

```
   field  ──particalize-on-demand──▶  particles  ──simulate──▶  quiesce  ──bake-back──▶  field
   (T0 bulk / texture)               (T1..T3 by energy)                    (T0, deformation persisted)
```

Only the **promotion trigger** and the **resolution** vary with scale/energy:

| | Theia (high energy, this doc) | Human surface (later, docs/35 reconstruction) |
|---|---|---|
| trigger | impact energy at the strike | **camera-visible interaction** (awake-set, docs/16) |
| region | shock-affected mantle cap | the interacted patch |
| resolution | full T3 SPH-EOS shock | T1–T2 granular, scaled to the event |
| bake-back | settled debris → bulk; disk → Moon (accretion) | settled patch → texture/displacement, until interesting again |

**Surface hooks to keep the Theia build from becoming impact-only** (per Robin's surface framing):
1. **T0 is a renderable, persistent field, not just gravity** — bake-back writes a real displacement/normal
   record (a crater stays a crater). Reuses the surface heightfield/mesh/texture machinery, not a throwaway.
2. **Promotion is gated by *interest*, not only energy** — at the surface, camera-visible ∧ interacting; you
   never particalize what nobody is looking at.
3. **Demotion is gated by quiescence ∧ disinterest** — a patch bakes back when settled *and* no longer
   watched/interacted-with, not the instant it stops moving.

Theia exercises the physics core of the primitive (particalize → T3 sim → bake-back); the surface adds the
interest/visibility gating on top. Build the core on the hard case; inherit it at the surface.

## The Theia instance — coarse-bulk Earth + particalized shock-cap

Earth is a **coarse self-gravitating bulk** (`planet::LayeredBody`, analytic differentiated ρ(r)) everywhere
**except a shock-cap** around the impact site, which is **particalized to full T3 SPH-EOS particles**
(`hydrostatic::HydroBody`). Theia is SPH-EOS particles too. The cap + Theia shock, the cap's Earth material
sheds into the disk; the quiescent bulk is a cheap gravity source + boundary. **Never pay for the 10²⁴ kg of
undisturbed interior** — that is what unlocks real Earth scale.

**The cap size is the physical dial, with a built-in correctness test.** It interpolates two *known* results:
- cap → nothing (rigid Earth boundary) = the **7–12%** Earth disk (docs/31, docs/28 root-cause #1),
- cap → whole Earth (all-particle deformable) = the **58%** Earth disk (docs/33 stage 3).

So the coupling is validated by **convergence**: grow the cap until the disk Earth-fraction plateaus; the
converged cap recovers ~58% at a fraction of the particle count. "Particalize what the shock excites" is not a
tractability hack — it is the physically-right region, and the plateau proves the cap is big enough.

## We are UPGRADING an existing skeleton, not building from scratch

The old rigid-boundary impact (`impact.rs` + `aggregate.rs`) already couples particles to a non-particle bulk
Earth. It is the skeleton; three upgrades turn it into the real T0↔T3:

| Skeleton (exists, file:line) | Upgrade |
|---|---|
| `Aggregate::with_gravity_source` → monopole + **uniform-density** Gauss interior `G·M·r/R³` (`aggregate.rs:517-532`) | positioned **layered** `g(r)` from `LayeredBody::enclosed_mass(r)` (`planet.rs:86`) |
| cap = thin **granular** excavated furrow (`impact::furrow_target_grains`, `impact.rs:488`) | cap = **shock-sized SPH-EOS** region seeded from the layered ρ(r)/u (à la `HydroBody::new_differentiated`, `hydrostatic.rs:97`) |
| boundary = **rigid penalty sphere** (`aggregate.rs:612+`) | **EOS interface**: a static shell of boundary SPH particles carrying the bulk's local ρ/u → real pressure support |
| bulk is inert (only reports reaction, `boundary_force_sum` `aggregate.rs:128`) | bulk is a **recoiling body**: sum particle→bulk gravity + interface force onto a bulk state (momentum conserved) |

## What must be built (from the substrate map, docs/39 investigation)

1. **Positioned layered gravity.** Wrap `LayeredBody` with `acceleration_at(world_point, earth_pos)` =
   `−r̂·G·M(<r)/r²` using `enclosed_mass(r)` (`planet.rs:86`) — the real differentiated interior, not the
   uniform-density Gauss the skeleton uses. *Verify:* matches `gravity_at(r)` radially; Gauss interior → 0 at
   centre; monopole `G·M/r²` outside.
2. **External-bulk-gravity term in the T3 stepper.** `HydroBody::accelerations` (`hydrostatic.rs:163`) is
   self-gravity only; add `+ bulk.acceleration_at(p)` per particle (and the GPU `sph_step.wgsl` path an
   analogous uniform param or a small bulk-source input). *Verify:* a single particle released above the bulk
   free-falls at `g(r)`; a relaxed cap sitting on the bulk stays in hydrostatic balance (bulk g + cap
   self-gravity + cap pressure + interface support).
3. **Particle→bulk recoil.** Sum the cap+Theia gravity/interface reaction onto a bulk-body `{pos,vel,mass}`
   (like `orbit::Body`); integrate the bulk in the same KDK. *Verify:* total momentum (bulk + particles)
   conserved to leapfrog precision.
4. **The particalization operator: bulk region → T3 SPH-EOS particles.** Given a spherical cap of the
   `LayeredBody`, instantiate equal-mass Tillotson SPH particles matching local ρ(r), u(r), material — no KE
   injected (PE-conserving seed, like `matter::materialize_region` `matter.rs:273`, but producing `HydroBody`
   SPH particles, not granular grains). *Verify:* a cap particalized then relaxed reproduces the layered
   profile it came from (the seed is self-consistent — it doesn't slump or explode).
5. **The EOS boundary shell.** A static (rigid-bulk, v=0) shell of SPH particles at the cap interface carrying
   the bulk's local ρ/u, so the cap rests on real pressure support instead of a penalty sphere. *Verify:* the
   cap neither sinks through nor launches off the interface (energy-conserving; the docs/33 §3 "the rigid
   boundary dissolves").
6. **Bake-back (demotion).** Settled cap/fallback particles → the bulk (or the T0 surface field); disk clumps
   → a Moon body (`accretion::accrete` `accretion.rs:166`, already conservative). *Verify:* mass/momentum/COM
   conserved across the demotion; the deformation is recorded (the surface hook).

## Staged plan (correctness-first; each stage a native test before the next)

- **39a — Positioned layered gravity** (item 1). Pure function on `LayeredBody`; unit-verify vs `gravity_at`.
- **39b — Cap on a bulk: hydrostatic hold** (items 1+2+5). Particalize a mantle cap, seat it on the bulk with
  layered gravity + the boundary shell, relax; it must hold the layered profile (no slump/explosion). This is
  the keystone — it proves a *partial* particalization is self-consistent.
- **39c — Recoiling bulk + momentum** (item 3). Add the bulk-body back-reaction; verify total momentum.
- **39d — Theia into the capped Earth** (all). Fire Theia into (bulk Earth + shock-cap); measure the disk
  Earth-fraction. **Sweep the cap size**; confirm it interpolates 7–12% → 58% and plateaus. The converged cap
  is the answer.
- **39e — Bake-back + Moon** (item 6). Demote fallback → bulk, accrete the disk → a Moon; conservation-verify.

39a–39b are the crux (a partial particalization that holds); 39d is the payoff (the converged number at
tractable cost); 39e closes birth-of-the-Moon. Only 39a–39b need land before we know the approach is sound.

### The GPU port, driven by the moon-drop (2026-07-23) — planned, not yet built

The CPU cap-on-bulk (39a–39e above) proved the primitive but stayed in `hydrostatic.rs`'s `#[cfg(test)]`
module and never reached the GPU `sph_step.wgsl` path (open-decision #2 deferred it). The **moon-drop now
forces it**: after the CPU `Aggregate` was retired (docs/58) and every collision routed onto the GPU SPH,
dropping the Moon particalizes ALL of *modern* Earth — a Law-III violation (it's a surface cratering event,
not a whole-body giant impact). Symptoms on the live site: modern Earth renders as a magma ocean, and the
Moon (~50 particles, since Earth sets the particle mass) is invisible. Robin chose resolution-on-demand.

**Scope narrows nicely: only the SMALL-impactor case needs the cap; birth is unchanged.** 39d found a
*rigid* bulk plateaus at ~25–33% disk and the full 58% needs a *deformable* bulk. Birth already runs
full-body SPH — that IS the "cap = whole deformable planet" limit, and it works. So we build the cap path
only for a small impactor (Moon), where a **rigid** bulk is physically honest (open-decision #1: rigid is
valid when the cap covers the whole shedding region — a Moon impact doesn't shed Earth's deep interior).
39b's decision #3 further simplifies it: a **bare non-injecting floor beats a boundary shell**, so no pinned
particles / boundary-shell are needed — just a floor + Gauss gravity.

**The 5-stage plan (each stage: build → test → the flagship rig):**
1. **GPU boundary primitive** — `sph_step.wgsl`: add two `vec4` `Params` fields (`bulk_cr` = centre+R_core,
   `bulk_vm` = velocity+mass), a `bulk_gravity()` (monopole outside R_core, **Gauss-linear ∝r interior** —
   39b lesson 1: a raw 1/r² singularity-sucks penetrators), and a **non-injecting floor** in `cs_kick_drift`
   + `cs_relax` (project a penetrator to R_core, remove inward normal velocity relative to `bulk_vm.xyz` —
   NOT a spring; the terrain-contact KE-launch lesson). Mirror in `gpu_sph.rs::SphParams` (offset 48/64,
   size 80, %16==0) + a `set_bulk()` setter on `GpuSph` + update the field-for-field layout-pin test
   (`sph_params_matches_the_shader_field_for_field`, and the parser-len/tail checks — verify `wgsl_offsets`
   handles `vec4`). Ports `hydrostatic.rs::bulk_floor` (645-660) + `Bulk::accel_at` (620-628).
2. **`particalize_cap`** (`hydrostatic.rs`, beside `particalize:199`) — the whole impactor + a cap of the
   planet near the impact site (real per-layer ρ/u/geotherm, restricted to a shell/cone), resting on the
   bulk floor. Base: `build_mantle_cap` (749) + the per-layer loop (206-232). Native test (cap holds
   hydrostatic on the bulk — the GPU analog of 39b's keystone).
3. **Cap-vs-whole decision** in `route_bodies_to_sph` (`lib.rs:1473`) — sized by the affected zone
   (energy/necessity, docs/44; the `impact.rs:483` `(2·r_imp).min(0.55·R)` clamp is the precedent), NOT a
   scene dial. Small impactor → cap + rigid bulk; comparable masses → whole-body (birth path, unchanged).
   The Moon gets its OWN full particle budget (fixes "invisible").
4. **Render** (`lib.rs`) — the bulk is the solid blue globe (its real surface); SPH particles only at the
   impact site. Fix the two magma bugs so a *modern*-Earth resolve isn't proto-Earth: globe glow (`2609`,
   currently `glow_of(impact_def.target)` when sph_active — its own comment 2604-2608 warns this sets the
   present-day Earth on fire) and the interior magma (`2708`) must read the LIVE planet's matter.
5. **Rig-verify** — moon-drop: blue Earth + a localised crater/splash + shattered Moon (NOT a magma ocean);
   birth: still lofts a disk. `mod app` is WASM-ONLY — build wasm + rig, native tests don't catch scene bugs.

### Status — 39a + 39b DONE, the keystone is proven (2026-07-18)

- **39a DONE.** `LayeredBody::acceleration_at` (positioned Gauss gravity) — verified (`planet.rs` test).
- **39b DONE — a partial particalization holds the CORRECT hydrostatic profile.** Test
  `a_particalized_mantle_shell_holds_hydrostatic_on_a_bulk_core` (`hydrostatic.rs`, `#[ignore]`): a mantle
  shell particalized into SPH-EOS particles on a coarse bulk core settles stably (outer 1405 km, spread
  0.5%, inner = R_core, zero leakage) AND satisfies hydrostatic balance dP/dr = −ρ·g_total to **rel 0.05 /
  0.25** (within the 0.5 operator bound, like stage 2a). **The approach is viable AND quantitatively
  correct.** Three lessons: (1) the bulk gravity MUST use the Gauss-correct interior (`∝r`, →0 at centre) —
  a raw `1/r²` monopole singularity-sucks any penetrating particle and blew the mantle to 170,000 km; (2) a
  **non-injecting floor** at R_core (the terrain constraint, spherical) is the leak-proof interface; (3) the
  interface pressure BC is **simpler than a boundary shell** — a fixed-ρ₀ shell OVER-confines the base
  (dP/dr ≈ 140× −ρg); a fluid column on the bare hard floor gives correct hydrostatic (`P(r)=∫ρg`, the floor
  supplies the base reaction). **The coarse-bulk + particalized-cap architecture is proven at the keystone.**
- **39c DONE.** The bulk RECOILS (a body {pos,vel,mass}); a bulk-coupled KDK step (Gauss gravity +
  Newton's-3rd back-reaction + non-injecting floor credited to the bulk) conserves total momentum to drift
  8.6e-4. Test `the_recoiling_bulk_conserves_total_momentum` (`hydrostatic.rs`).
- **39d DONE — the payoff, with a decisive finding.** `the_cap_size_dials_the_disk_earth_fraction`
  (`hydrostatic.rs`, `#[ignore]`): Theia into a capped Earth (bulk core + particalized basalt mantle cap).
  **The approach WORKS:** both caps produce a real MIXED (Earth+Theia) orbiting disk (~0.2 M☾) whose Earth
  fraction — **25% (thick cap) / 33% (thin cap)** — is clearly **above the rigid-boundary 7–12% ceiling**
  (docs/31). So the particalized cap sheds Earth's own mantle, reproducing the deformable-Earth mechanism at
  a fraction of the particle count. **THE FINDING (resolves decision #1 — a RIGID bulk plateaus below 58%):**
  the Earth-SHED mass is **~constant (0.056 M☾) regardless of cap thickness** — only the cap's *near-surface*
  shocks and sheds; the rigid bulk **reflects the deep shock** instead of doing the deep mantle-shedding that
  gets the all-particle case to 58%. So a rigid bulk is a partial win (7–12% → ~25–33%, not → 58%), and the
  cap-size dial is not monotonic at coarse N (scatter + the rigid-bulk limit). **Reaching 58% needs a
  DEFORMABLE bulk** (so the shock propagates into it) — the next lever, a revision to the rigid-bulk-first
  choice. Next: 39e (bake-back + Moon) and/or a deformable-bulk variant to close the gap to 58%.
- **#1 (deformable bulk) — DIRECTION VALIDATED, exact number entangled with #3.**
  `a_deformable_coarse_bulk_recovers_the_earth_fraction` (`hydrostatic.rs`, `#[ignore]`): a variable-
  resolution all-particle Earth — COARSE (cheap) iron core + FINE basalt mantle, all deformable — vs all-fine.
  Result: **a deformable bulk sheds Earth's own mantle even when the core is COARSE** — the coarse-core run
  (947 particles) gave a healthy **63%**-Earth disk, comparable to the reference 58% and far above the rigid
  bulk's ~25% plateau. **So the rigid bulk was the limiter, and a coarse deformable core is a viable cheap
  bulk — this is the win for planetary scale** (deformable + cheap, not full-resolution-everywhere). **BUT
  the exact fraction is SCATTER-DOMINATED at this coarse N** (all-fine gave 25% where the reference gives 58%;
  a ~40-point swing between nominally-similar configs — the docs/28 chaos scatter). So the quantitative
  "closes the gap to 58%" claim CANNOT be resolved without an **ensemble + higher N (task #3)** — #1 and #3 are
  entangled. The test asserts only the robust mechanism (both deformable variants shed Earth); the comparison
  is reported, not asserted. NOTE (setup lesson): a DIFFERENTIATED Theia (dense iron core ploughs in) is
  essential — a basalt-sphere Theia sheds ~0% Earth (docs/28 plough mechanism).
- **#2 (39e bake-back) DONE; accretion→Moon already exists; a resolved Moon gates on #3.** The JIT RETURN
  path — `bake_back` (`hydrostatic.rs` test `bake_back_conserves_mass_momentum_and_com`, fast, not ignored):
  settled particles demote into the coarse bulk (mass + momentum absorbed, radius grows at constant density),
  conserving total mass/momentum/COM to **<1e-12**. So the full cycle **field → particalize (39d) → simulate
  → bake-back (39e)** is closed and verified. The disk→Moon ACCRETION was already built + demonstrated
  (`accretion.rs` + the GPU 35k disk: 16 clumps, a 0.023 M☾ seed) — a *resolved* Moon from the capped case is
  a resolution/N question that gates on **#3**.
- **Where 1–4 stand (2026-07-18):** #1 direction + #2 bake-back landed; the whole JIT cycle is validated at
  coarse N. **The critical path now collapses to #3** — a GPU/high-N + ENSEMBLE run with a deterministic
  reduction — which is what converges BOTH the Earth fraction (#1) and a resolved Moon (#2) out of the docs/28
  chaos scatter. #3 is a major (multi-session) GPU build. **#4 (terrain)** is the separate low-energy instance
  of the same primitive, after planetary scale.

## Open decisions to pressure-test (before building)

1. **Rigid vs deformable bulk.** First version: the bulk is *rigid* (recoils as one body, doesn't deform) —
   valid **iff the cap covers the entire shedding region** (the shock-excited mantle); the deep interior below
   the cap wouldn't shed anyway. The cap-size sweep (39d) tests this: if the fraction plateaus below 58%, the
   shock is exceeding the cap and the bulk needs to promote more (grow the cap) — the JIT feedback. Acceptable
   first cut, or must the bulk deform from the start?
2. **CPU (`HydroBody`) vs GPU (`sph_step.wgsl`) first.** The correctness anchor (39a–39d, the interpolation
   test) is cleanest on the CPU f64 `HydroBody` (small N, deterministic, matches the 58% reference). GPU/high-N
   (real Earth scale) follows once the mechanism is proven. Agree CPU-first?
3. **Boundary shell vs analytic pressure BC.** A static SPH boundary shell is EOS-consistent but adds
   particles at the interface. Alternative: an analytic pressure boundary condition (the bulk pushes the cap
   with `P(R_interface)` directly). Shell is more honest (real SPH), BC is cheaper. Start with the shell?
4. **f32 at Earth scale.** Real Earth radius 6.371e6 m → f32 spacing ~0.5 m; the Earth-relative frame trick
   (already used in the browser GPU impact) handles it, but the *bulk* introduces a large positioned mass —
   confirm the cap-relative frame keeps precision where the particles are.

## Non-goals (this doc)
- The surface/human-scale instance (camera-gated promotion, texture bake-back render assets) — that is the
  docs/35 terrain reconstruction, *after* this. This doc only ensures the primitive generalizes to it.
- Retiring the container fork (`HydroBody`/`Aggregate`/`World`) — orthogonal (docs/33 stage 5).
