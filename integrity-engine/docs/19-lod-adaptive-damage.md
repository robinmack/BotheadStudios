# LOD-adaptive damage — one event, every scale

> Design note. A damage event is **one physical thing** described at whatever level of detail the
> observer occupies. At celestial scale it is an **energy/momentum event + a summary** (crater size, or
> disruption); zoom in and that summary **materialises** into an actual voxel crater; zoom way in and
> that crater is grains and rubble. The summary and the materialisation must describe the *same event*
> and agree — damage is **conserved across LOD**. This is the connective tissue between the space band
> (`docs/17`) and the voxel matter model (`docs/16`/`18`), built on the scale-relative north star
> (`docs/13`). Status: **bridge proven; visual zoom-in staged.**

## The bridge: same σ·V at both scales

The crater a given impact makes is set by the same relation at every scale — the energy fractures a
volume of target material against its strength:

```
V ≈ E / σ        (strength regime)
R = (3V / 2π)^⅓  (hemispherical crater radius)
```

- **Coarse (summary):** `damage::crater_volume(energy, strength)` / `crater_radius` — what a celestial
  observer records for the impact site.
- **Fine (materialised):** `matter::impact(site, dir, energy)` spends the same `σ·V` per voxel, so it
  excavates `≈ E/σ` voxels.

**Proven:** `matter::voxel_crater_matches_the_coarse_damage_summary` — the voxels carved equal the
summary volume. So promoting a summary into voxels (zoom-in) or coarsening voxels into a summary
(zoom-out) conserves the crater. That equality *is* LOD-adaptive damage.

## Honest regimes — and why the Moon is not a tidy crater

The impact energy, compared to the target body's **binding energy** `(3/5)GM²/R`, tells the truth:

| Regime | Condition | Outcome |
|---|---|---|
| **Strength crater** | crater ≪ body | `V = E/σ` crater (bullet, meteor, asteroid) — the voxel model |
| **Gravity regime** | crater ~ body | ejecta must climb the gravity well; strength `E/σ` over-predicts — *flagged, unmodelled* |
| **Disruption** | `E ≥ binding` | the body comes apart (the giant-impact regime that shaped the real Moon) |

The Moon dropped onto the Earth releases **~4.5e30 J**: that is **~36× the Moon's** binding energy (the
**Moon shatters** — `GroundEffect::Disruption`) but only **~2% of the Earth's** (~2.2e32 J), so the
**Earth survives** and takes a **planet-scale crater**, not a neat bowl. The space-band HUD now states
exactly this on impact — honest about the regime instead of promising a tidy crater the physics forbids.
`damage::moon_shatters_but_earth_only_craters` pins the numbers.

## What's built vs staged

- **Built (native, tested):** the σ·V bridge (`damage.rs`), its equality with the voxel operator, the
  binding-energy regime verdict, and the honest HUD readout on the Moon impact.
- **Staged (the visual zoom-in):** actually flying the camera from the celestial view down to the
  impact site and materialising the voxel crater there — generating a terrain patch and applying
  `matter::impact` at local scale, conserving mass/momentum/energy across the transition. This is a
  real renderer effort (the space band and terrain slice are separate today) and needs on-device eyes,
  so it is designed here, not slammed out blind. The physics that drives it is already proven.

## Roadmap (Robin's ordering: LOD → MLS-MPM → fluid)

1. **LOD-adaptive damage** — *this doc; bridge landed.* Next: the visual zoom-in materialisation.
2. **MLS-MPM** — the unifying constitutive solver (elastic/plastic/granular/fluid from material params),
   which makes the crater materialisation and the fluid response fall out of one loop (`docs/08`).
3. **Fluid flow** — real waves/incompressibility/viscosity for the pond (needs a viscosity field).

## Playground note — the two-moon stress test

A planned scene: **two moons on the same orbit, opposite sides of the Earth, de-orbited at once.** The
N-body core (`orbit.rs`) is already generic in the number of bodies, so this is nearly free physically;
its value is as a **stress test** — two simultaneous surface collisions, symmetric contact resolution,
and (later) two craters materialising at once. "It's our universe; we might as well play in it."
