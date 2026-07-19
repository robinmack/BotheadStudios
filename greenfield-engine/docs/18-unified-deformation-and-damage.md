# Unified deformation & damage — one operator for a bullet, a pebble in a pond, and a Moon

> Design note. Damage is not a set of effects; it is **one physical process** — matter absorbing
> deposited energy and momentum and responding by its own material law — applied at whatever scale and
> frame the observer occupies. A bullet cratering a wall, a pebble splashing a pond, and the Moon
> shattering the Earth must be the *same code*, differing only in parameters and level of detail.
> Status: **design + first slice**. Builds on the voxel fracture in `matter.rs`, the unified dynamics
> (`docs/16`), the impact-energy honesty (`docs/17`), and the adaptive resolution of `docs/08`.

## The one process

An impact **deposits kinetic energy and momentum into a local region** of matter. Each parcel of
matter then responds by its **constitutive law**, read from material data — no per-object, per-effect
code:

- local **stress** (roughly, deposited energy density) is compared to the material's **yield/fracture
  strength**;
- where stress exceeds strength, the matter **fails**: it detaches, flows, or fractures, carrying away
  momentum as ejecta/splash/debris;
- **mass and momentum are conserved**; the **energy is accounted** — it becomes motion, heat, fracture
  surface, melt. (The Moon-impact taught us this: a perfectly-inelastic "stop" that *deletes* the
  energy is a fudge — the energy is the damage. `docs/17`.)

This is the material-point / MLS-MPM picture (`docs/08`'s stated target): one solver, per-material
constitutive models, unifying elastic, plastic, granular, and fluid response.

## Invariance 1 — material (the response is data, not code)

The *same* deposited stress produces different outcomes purely from the material:

| Phase | Strength | Response to impact |
|---|---|---|
| **Solid** (rock, metal) | high | localized fracture / crater + ejecta; elastic below strength (bounce/embed) |
| **Granular** (sand, soil) | low | cratering + granular flow, settles into a pile |
| **Liquid** (water) | ~0 | always yields → splash + transient crater that closes into waves |

So *bullet-in-rock* and *pebble-in-pond* are one call with different material. **First slice landed:**
liquids no longer resolve to an "unbreakable" `1e12` strength (that fudge made water stronger than
granite) — a fluid yields at ~0, governed by its parsed `phase`. Test:
`materials::a_liquid_yields_where_a_solid_resists`.

## Invariance 2 — scale & frame (what matters is what the observer can perceive)

The observer's **frame of reference and zoom decide what is materialized and at what resolution**
(`docs/13`). The *physics* is the same; the *representation* adapts:

- **Celestial frame** (watching the Moon hit the Earth): the impact is an **energy + momentum event**
  plus a **damage summary** — crater size, disruption fraction, whether the body survives. You do not
  simulate individual ejecta, and you certainly don't simulate buildings.
- **Regional frame** (zoomed in on the impact site): the crater is **voxel fracture + ejecta** — the
  `matter.rs` model at full resolution; the planet-scale motion is now background.
- **Human frame** (zoomed *way* in): individual grains, structures, a building — and now the crater
  rim and the ejecta are off-screen background.

As Robin put it: at celestial scale we wouldn't care about buildings unless we zoom way in — at which
point we wouldn't care about the ejecta. The engine must spend fidelity only on what the current frame
can perceive, promoting a summary into detail on zoom-in and coarsening detail into a summary on
zoom-out (`docs/08` refine/coarsen), **conserving mass, momentum, and energy across the transition** so
the damage is consistent at every scale.

## What exists to build on

- **Fracture by strength** — `matter::dig` already detaches voxels whose `fracture_strength` a tool's
  stress exceeds, ejecting debris that falls and settles (matter-conserving). A primitive impact.
- **Unified dynamics** (`docs/16`) — ejecta are bodies/particles in the one awake-set gravity loop; a
  thrown clod already shoves the probe.
- **Impact-energy accounting** (`docs/17`) — `inelastic_dissipation` / `binding_energy` measure the
  energy a collision must turn into damage.

## Roadmap (honest, staged)

1. **Generalized `impact(site, momentum, energy, materials)`** — replace "tool strength" with a real
   impactor's KE/momentum; affected radius and stress scale with energy; ejecta carry momentum
   (conserved). Same call for bullet and pebble; native-testable across materials/scales.
2. **Fluid response** — a pond needs flow, not brittle shards: incompressible displacement, waves, and
   settling-to-level. Needs a **viscosity** field (not in the material DB yet — flagged) and a fluid
   constitutive model. Interim: liquids yield and splash via the fracture path, then flow-settle.
3. **MLS-MPM constitutive unification** — the endpoint: one solver with per-phase constitutive models,
   so elastic/plastic/granular/fluid all fall out of the material's parameters (`docs/08`).
4. **LOD-adaptive damage** — a coarse-scale impact records a summary (crater, disruption); zooming in
   *materializes* the detailed fracture from that summary, conserving mass/momentum/energy.

## Honesty flags

- Liquid response is currently only "yields trivially" (strength → 0); real flow/waves/viscosity is
  future and needs data the DB doesn't yet carry.
- Planet-scale fragmentation is **summarized, not simulated** — we report the energy and state the
  outcome (`docs/17`), and will materialize detail only on zoom-in.
- "Stress" here is an energy-density proxy, not a full stress tensor; the deformation gradient +
  constitutive stress arrive with MLS-MPM.
