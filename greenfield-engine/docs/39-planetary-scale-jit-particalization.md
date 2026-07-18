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
