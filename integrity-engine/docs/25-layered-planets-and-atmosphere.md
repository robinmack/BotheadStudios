# Layered planets — declare the composition, compute the world

> Robin: *"The earth should be modeled with 'average area particles' similar to the actual construction
> of Earth… essentially at 1 g, the earth's core should be pressurized which should cause the textels
> there to pick up the appropriate molten state for their material type — that should have been a
> natural artefact of gravity/mass/material if we didn't fudge the composition."*

A planet is **declared** as nothing but its real construction — concentric layers of real materials with
their observed mean (compressed) densities and temperatures, plus the measured mass of its atmosphere —
and everything else is **computed**:

- **Gravity g(r)** — Gauss's law over the enclosed layer mass (rises through the mantle, peaks at the
  core boundary, zero at the centre). A point-mass 1/r² inside a planet is wrong physics.
- **Pressure P(r)** — hydrostatic equilibrium `dP/dr = −ρ·g`, integrated from the surface. Earth's
  centre comes out ≈360 GPa (real: 364) from the declared PREM densities alone.
- **Surface pressure** — the *weight of the declared atmosphere mass* spread over the sphere:
  `P = M_atm·g/(4πR²)`. Earth's measured 5.15×10¹⁸ kg comes out ≈1 atm. Never declared as a pressure.
- **PHASE** — never assigned. Each material carries a pressure-dependent **melting curve**
  (Simon–Glatzel, published fits: `thermal.simon_a/simon_c`) and **boiling curve** (Clausius–Clapeyron
  from latent heat + molar mass: `thermal.molar_mass`); the phase at any point is the local temperature
  against those curves at the computed pressure.

## The emergence results (all are passing native tests, `planet.rs`)

1. **Earth's inner core is SOLID, its outer core MOLTEN** — the inner core is *hotter* than the outer
   core, yet pressure pushes iron's melting curve above the geotherm exactly at the real inner-core
   boundary. Gravity + mass + material ⇒ the observed core structure. A fudged composition could never
   produce this; the real one does.
2. **The declared layers integrate to Earth's real mass** (5.97×10²⁴ kg) **and 9.8 m/s² surface gravity**.
3. **The oceans are liquid because the air weighs on them** — at the emergent 1 atm, 288 K water is
   liquid; at zero pressure the boiling point collapses to 0 K and water **flashes to vapor at any
   temperature** (below the ~611 Pa triple point, liquid has no regime at all). "Water exposed to vacuum
   would be wild" — it is, and nothing enforces it; it falls out of Clausius–Clapeyron. The Moon
   declares no atmosphere ⇒ no lunar seas, as observed.
4. **The Moon's outer core is molten** at lunar pressures (its Simon curve barely rises), its mantle
   solid, its mass real.

## Wired into the mutual impact (`impact.rs`)

Materialized particles sample the layered body at their own radius: material identity (basalt crust /
peridotite mantle / iron core) AND real internal temperature. Excavated deep matter *glows because it
genuinely is that hot* — the incandescent mantle and molten-iron ejecta of a Moon-scale impact are the
layer data plus contact dissipation, nothing painted. Each fragment renders in its own material's
measured reflectance.

## Continents & oceans

The render shell samples a 10°×10° land/ocean mask matched to the ~9° grain spacing ("average area
particles"): granite continents, water oceans, real reflectances (water is `[0.02, 0.03, 0.04]` — nearly
black, as real water is; the vivid "blue marble" is atmospheric Rayleigh scattering, which we do not
fake — see the roadmap below).

**Honesty flags:**
- The hand-digitized mask over-represents land (~37% vs the real 29%); a cited landmask dataset is the
  refinement.
- Ocean mean depth (~3.7 km) is far below one 600 km shell grain, so at this LOD water is the *material
  of a grain's surface*, not a resolved layer.
- No planetary rotation yet — the mask's orientation is arbitrary but consistent.
- Layer temperatures are the *observed* geotherm/selenotherm (declared data): deriving them needs
  thermal history, radiogenic heating and convection — future physics.
- The lunar core is really Fe–S (melts lower than our pure-iron entry); we use the upper published
  selenotherm. An Fe–S material entry is the refinement.
- Boiling curves assume constant latent heat (water's triple point comes out ~268.5 K vs the real
  273.16 — ~2% in the curve, flagged).

## ROADMAP — atmosphere as MATTER (the next major physics milestone)

Today the atmosphere is a **static boundary condition**: declared mass → emergent surface pressure →
phase stability. That already keeps the oceans liquid. But it is not yet *matter in the simulation*, and
one honest feature unlocks a whole family of currently-missing physics **at once**:

- **Aerodynamic drag** — currently absent. (Quantified: for the dropped Moon it is Δv/v ~ 10⁻⁶ —
  honestly negligible, a flagged omission. For a small meteor it is EVERYTHING: burn-up, terminal
  velocity, airbursts.)
- **Entry plasma** — the shock-compressed air column of a large impactor glows as plasma (the visible
  fireball is mostly *air*, not rock). Requires air as compressible matter with the same thermodynamics
  (shock heating → temperature → incandescence — machinery we already have).
- **Rayleigh scattering** — the blue sky and the blue marble. The reason our Earth renders without a
  blue halo today is that we refuse to paint one.
- **Weather-scale pressure dynamics** — the global pressure wave of a giant impact, blast waves from
  explosions, sound.
- **Evaporation/condensation cycling** — water vapor as part of the atmosphere's composition, closing
  the loop opened by `surface_phase`.

The honest implementation is the same one-law pattern: air parcels as particles (compressible, low
density, real composition N₂/O₂), the canonical contact/pressure law, the existing thermodynamics.
Scale-relative: a boundary-condition summary at planetary zoom, real parcels where the observer (or an
entering body) is. This is `docs/23` everything-is-matter applied to the thinnest layer of the planet.
