# docs/35 increment 4 — terrain on the unified GPU path: the design decision + plan

The "surface to Robin before increment 4" decision (docs/35 §"one open design decision"), settled on physics
and the engine's own design intent, plus a staged plan. **This doc is for sign-off before any code.**

## The decision: (b), sharpened — an energy-tiered *composite* contact law, NOT pure SPH-EOS

The open question was: does the unified GPU path (a) go **pure SPH-EOS** (retire granular contact), or (b)
**keep both laws** and select per material/energy? The investigation makes (a) physically impossible for
terrain and shows the docs already intend (b) — but as *one composite law*, not two rival ones.

### Why pure SPH-EOS cannot do terrain (physics)
SPH-EOS is an **isotropic scalar pressure** `P(ρ,u)` (`eos.rs`, `hydrostatic.rs:177`). Its force acts only
along `∇W` (the density gradient) — there is **no tangential term and no yield surface** anywhere in
`cs_forces`, and its only dissipation (Monaghan AV) is gated `if vr<0` so it vanishes at rest. Therefore a
pressure-only continuum has **zero deviatoric (shear) strength**, and:
- **a resting probe sinks** to neutral buoyancy (only a normal buoyant force; a denser-than-rock iron probe
  must compress matter to ρ>ρ₀ to develop any support) and **slides on any slope** (no friction);
- **a pile self-levels to 0°** (no shear strength → it flows like the fluid it is — the same kernel runs the
  `IdealGas` atmosphere);
- **a crater springs back** (Tillotson cold compression `P=A·μ+B·μ²` is a *reversible* elastic bulk modulus —
  no plastic yield, no retained strain).

All three of terrain's defining behaviors — **rest, pile, persistent crater** — live in the tangential
Coulomb-friction + cohesion terms of `granular::contact_accel` (`granular.rs:206-217`, the friction cap `μ·N`
is literally "the angle of repose"). SPH-EOS has no equivalent.

### Why this is what the docs already intend
docs/33 §3 is explicit: Tillotson-SPH **replaces the linear-elastic normal *compression penalty*** (the
pressure response) — **not** granular friction/cohesion. They **coexist as energy tiers of one particle
system**, selected by energy density and moved by the awake set (docs/16):
- **T1 quasi-static** — grains at rest, talus, a settled disk: *granular contact* + gravity, no EOS heat.
- **T2 dynamic granular + thermal** — cratering, ejecta curtains, meteor impacts.
- **T3 full EOS shock + vapor** — giant impacts: Tillotson + SPH.

Crater persistence is not held by either force law but by **bake-back into the T0 displacement field on
settle** (`deposit_resting_grain`), which already exists.

### The sharpened statement
The unified law is **one composite contact**, evaluated per neighbour pair on one particle system:
- **Normal response — energy-tiered:** granular elastic penalty `k·overlap − damp·v_n − cohesion` at low
  energy (solid), transitioning to **Tillotson-EOS pressure** `−m(P_i/ρ_i²+P_j/ρ_j²)∇W` + Monaghan AV at high
  energy (shocked/melted). The EOS *replaces the penalty's normal spring*, exactly as docs/33 says.
- **Tangential response — always granular:** Coulomb friction (cap `μ·N`) + cohesion. SPH offers nothing
  here; at melt/vapor the friction fades to zero *because the material is now fluid* (a smooth function of
  phase), not because we drop the term.
- The selector is **energy density vs the material's own thresholds** (e.g. internal energy `u` vs melt/vapor
  energies already in `eos.rs`), i.e. the docs/33 tier boundary — physical, not a scene flag.

**So: keep granular contact. Add Tillotson-SPH as the high-energy normal tier. Do not retire friction. This is
"one law, scale/energy-adaptive," which is the actual charter (docs/23/24) — not a compromise.**

## What "unified GPU path" means here — and what it does NOT

The terrain band models **Earth as a rigid heightfield boundary** (a 96 m surface patch — you don't simulate
the planet's interior), whereas the space band is **Earth-as-particles**. Unification does **not** mean
turning terrain-Earth into particles. It means the **debris grains + probe** obey the one composite law on
shared machinery, still colliding against the terrain heightfield boundary.

The two GPU steppers legitimately differ in more than the law:

| | terrain `particle_step.wgsl` | space `sph_step.wgsl` |
|---|---|---|
| boundary | terrain **heightfield** (non-injecting constraint) | none (self-gravity holds it) |
| gravity | uniform `surface_g` | N-body **self-gravity** |
| integrator | **trapezoidal-implicit θ=0.70** (A-stable for stiff DEM) | **KDK leapfrog** + adaptive Courant (symplectic SPH) |
| grid / struct / `hash_cell` | shared idiom | shared idiom |

The integrator difference is **not incidental**: stiff granular contacts need implicit/semi-implicit
integration to stay stable; self-gravitating SPH needs a symplectic leapfrog for energy conservation over
orbits. Forcing both onto one integrator is either unstable (KDK on stiff contacts) or expensive (tiny dt).

**Recommendation: unify the LAW + DATA, keep the integrator/boundary/gravity terms scene-selected.** Extract
the composite contact law into **one shared WGSL function, verified against the Rust `granular`+`eos` source of
truth** (the docs/33 stage-5 "WGSL-from-Rust" goal, 5d), and have both steppers call it. This achieves the
real north star — *one contact law at every scale* — without an impossible single-integrator mandate. The
"four integrators" dedup (docs/32) is a separate, lower-priority cleanup.

## Staged plan (each: verify → commit; never break the DEPLOYED terrain scene — rig-watch every visual step)

- **4a — Shared, verified composite-law WGSL.** Factor the normal(tiered)+tangential(granular) law into a
  WGSL function generated/checked against the Rust `granular::contact_accel` + `eos` (extend `tools/sph-verify`
  or a sibling `contact-verify`: GPU law == CPU law to f32). *Keystone — one law, proven.*
- **4b — Terrain debris on the shared law (no behavior change).** Point `particle_step.wgsl` at the shared
  function (replacing its hand-mirrored `contact_accel`), keeping its heightfield + implicit integrator.
  *Verify:* terrain rig-watch — **probe rests, debris piles at repose, crater persists**, identical to today.
- **4c — Space SPH on the same shared law.** `sph_step.wgsl` calls it for its EOS normal tier and *gains
  granular friction as the dormant low-energy tier*. Re-run `sph-verify` (still matches CPU); rig-watch the
  GPU impact unchanged.
- **4d — EOS normal tier live in terrain (T3).** A hot meteor now develops real Tillotson pressure / melt in
  the terrain band, not just an elastic penalty. *Verify:* high-energy meteor rig-watch (melt/vapor at the
  contact, granular ejecta around it).
- **4e — Retire the third solver: the probe onto the GPU path.** The probe is a bonded cohesive iron lattice
  = granular-with-cohesion; port it from the CPU `Aggregate` onto the unified GPU stepper. *Verify:* probe
  rests and shatters emergently as today. (This is the increment that most advances "delete `Aggregate`.")

4a–4b are the core of increment 4 and its acceptance test (rest/pile/crater). 4c–4e extend into increment 5
(retire `Aggregate`, WGSL-from-Rust). Scope this session: **4a + 4b**, then reassess.

### Implementation reality check (found on starting 4a — plan revised)

Starting 4a exposed a false premise. The GPU granular `contact_accel` (`particle_step.wgsl:132`) is **not** a
force-level mirror of the Rust `granular::contact_accel` (`granular.rs:172`), and *shouldn't* be: the GPU puts
the normal **damping into the implicit solver's tensor** (`g = θ²·dt²·k + θ·dt·c`, `:169`) and keeps only the
spring in the explicit force (`f_rep = k·max(overlap,0)`, `:149`), whereas the Rust force is fully explicit
(`f_rep = (k·overlap − c_damp·v_n)`, `:192`). The two are **structurally different by design** — the granular
law is fused with the θ-implicit integrator we deliberately kept scene-selected. So a `sph-verify`-style
force-equivalence check (4a) is **not applicable** to the granular path (the forces legitimately differ; only
the full integrated step agrees).

And that verification already exists at the right level: **`tools/gpu-verify`** drives `particle_step.wgsl` on
the RTX 2070 and verifies the granular *behavior* (contact-repel + momentum, resting stack, angle of repose vs
μ, crater-fill, restitution), and `granular.rs` is unit-tested. So the "one law, proven" keystone is **already
met** — 4a as scoped is redundant.

**Revised increments (skip 4a; the law is already GPU-verified behaviorally):**
- **4b′ — EOS normal tier in the terrain shader (the composite law, in terrain).** Add the Tillotson-EOS
  normal response to `particle_step.wgsl`, energy-selected against the granular penalty (u vs melt/vapor from
  `eos.rs`), so a hot meteor develops real pressure/melt (T3) while rest/pile/crater stay pure granular
  (T1/T2). *Verify:* `gpu-verify` behaviors unchanged at low energy + a new high-energy scene (melt at
  contact, granular ejecta). This is the substantive "one composite law" step for terrain.
- **4c′ — Retire the third solver: the probe onto the GPU path.** Port the probe from the CPU cohesive
  `Aggregate` to GPU grains. NOTE the gap: the probe uses persistent bonds with a break-strain (0.06), while
  the GPU has only *range-based cohesion* (`COH_RANGE=0.15`) — so this needs GPU persistent-bond tracking
  (bond list + break-on-strain), a real feature, not just reusing cohesion. Highest value (most advances
  "delete `Aggregate`") but the largest piece. *Verify:* probe rests and shatters emergently as today.

Verification levels are law-appropriate, not uniform: **granular → behavioral (`gpu-verify`)** because it's
integrator-fused; **SPH-EOS → force-equivalence (`sph-verify`)** because its force is separable.

## Guardrails
- The terrain scene is **deployed** (integrity.bothead.net). No commit may leave it broken; rig-watch every
  visual step (CLAUDE.md #4). Keep the CPU/old path alive until the GPU replacement is rig-verified.
- No-fudge: the shared law must be *verifiably* the same as the Rust source (that's 4a's whole point) — speed/
  container changes must never change the answer (the `sph-verify`/`impact-run` discipline).
