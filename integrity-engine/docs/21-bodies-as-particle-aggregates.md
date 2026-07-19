# Bodies as particle aggregates — emergent bodies, emergent destruction

> Design note. A celestial body is a **cloud of particles held together by its own gravity** (a rubble
> pile), not a point mass or a rigid sphere. Its cohesion and roughly-spherical shape **emerge** from
> mutual gravity (the representation invariant, `docs/15`: roundness is emergent), and it **comes apart
> when given more energy than its gravitational binding energy**. So a shattered moon is *the same
> N-body gravity that made it round, run past its binding energy* — a **simulation, never a scripted
> effect**. This is the honest answer to "is the destruction inherent in the model, or a mock?": it
> must be inherent, and this is how. Status: **gravitational skeleton landed; material/thermal + impact
> + rendering staged.**

## Why this, and not a fireball

The celestial bodies were point masses drawn as lit spheres — there was **no matter to break**, so any
on-impact explosion would have been a hand-animated mock. Making the body an aggregate of real matter
means the destruction is produced by the operators we already trust:

- **Gravity** binds the cloud (and rounds it) — `aggregate.rs`.
- **Impact energy** (`orbit::inelastic_dissipation`) is deposited into the particles.
- **Thermodynamics** (`damage::classify`) decides, per particle, fracture / melt / vaporize from the
  deposited energy density vs. that particle's material thresholds.
- **Emission** (`emission::incandescence`) makes the hot debris glow from its temperature.

Nothing is scripted. The debris cloud, the melt, the vapor, the re-accretion or the escape of fragments
all fall out of the same physics as the terrain meteor — **scale-invariant** (`docs/18`).

## Landed (native, tested) — `aggregate.rs`

A self-gravitating `Aggregate` of `orbit::Body` particles with **softened** mutual gravity (a dense
cloud has close pairs whose bare 1/r² would explode):

- `binding_energy()` — `Σ G·m_i·m_j / r_ij`, the energy to disperse it.
- `kinetic_energy_com()`, `rms_radius()`, `com()` — diagnostics of bound-vs-dispersed.
- **Verified:** `a_self_gravitating_cloud_holds_together` (a cold cloud stays bound — cohesion is
  emergent, not glued) and `energy_above_binding_disrupts_it` (a kick above the binding energy makes it
  disperse — emergent disruption, the identity behind a shattered moon).

## Staged (the visible, integrated body)

1. **Material + temperature per particle.** Give each aggregate particle a material (basalt Moon;
   rock/water/ice Earth surface) and `temp_k`, so `damage::classify` + `emission` apply per particle.
2. **Impact = the same operators.** When two aggregates (or an aggregate and the Earth) collide,
   deposit the impact energy into the contact particles (peaked, `docs/20`), classify each
   (fracture/melt/vaporize), and let gravity sort the rest: bound fragments re-accrete, unbound ones
   escape. Emergent crater, melt sheet, vapor, and debris — no mock.
3. **Rendering (needs on-device eyes).** Draw the aggregate/debris particles in the space band with the
   incandescent emission already built (`emission`), so the Moon visibly shatters into a glowing,
   cooling debris cloud while the Earth survives with a molten wound. Then the celestial→voxel zoom-in
   (`docs/19`) is just choosing the aggregate's resolution.
4. **LOD.** Far away, the aggregate is summarized as a single mass point (`docs/13`); on approach /
   impact it materialises into particles — the same body, different resolution, conserving mass,
   momentum, and energy across the transition.

## Honest scope

Today's aggregate is the **gravitational skeleton** — mass, position, velocity, self-gravity, binding.
It proves bodies bind and disrupt emergently. The material/thermal per-particle state, the impact
coupling, and the rendering are the next slices; until they land, the *visible* Moon-crash still shows
the momentum stick, not the shatter — and we will not fake the shatter in the meantime.
