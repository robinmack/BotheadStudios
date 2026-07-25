# The one deposition door: impacts reach the awake set

> Design note, docs/23 step 2 made real. An impact EVENT in the ground world (a meteor arriving on
> a body or on the terrain, or an impact declared in a world file) deposits its energy and momentum
> into ALL matter in range through one operator, `simulation::Simulation::deposit_event`. Terrain
> voxels, cohesive-body parcels and debris grains are recipients of one walk; no per-object branch
> decides who is hit. Status: **landed** (native tests + rig-watched in the browser).

## What was wrong

Detection and deposition were entangled, and deposition forked. A meteor that met the ball went
through `interaction::detect_swept` and deposited ONLY into the ball's parcels; a meteor that met
the ground called `matter::impact` and deposited ONLY into voxels; debris grains near either event
received nothing they were not physically touched by afterwards. Three kinds of matter, three
different answers to "the impact's energy arrives" - the docs/46 pattern, one question, several
answers, and the exact per-object special-casing docs/16 forbids.

## The model now

Detection stays the engine's collision owner's job and only picks the SITE and the event's numbers:
a body contact is forecast on the swept segment (`detect_swept`, reduced-mass energy, site on the
struck body's skin), a ground landing is bisected to where the trajectory crosses the shared ground
height (a fast meteor moves metres per step; the post-step sample can be metres underground). Both
produce the same tuple: site, direction, momentum, energy.

`deposit_event` then walks every parcel of awake matter within the materialisation LOD cap
(`matter::IMPACT_LOD_R`, the same cap the crater uses) and splits the event by geometry and
coupling, never by name:

* **Coupling length λ** = the crater radius this energy opens in the matter AT the site
  (`damage::crater_volume`, E/σ, the accounting every impact in this engine already uses), clamped
  between the grain scale and the LOD cap. Cohesionless matter at the site (σ = 0) arrests nothing,
  so λ is the cap.
* **Weight** w = V · exp(−d/λ) / d² per parcel of matter (voxel, body parcel, debris grain alike):
  spherical spreading of the front, attenuated over the crater scale, d floored at the grain scale.
* **Shares** E·w/Σw and p·w/Σw are delivered through the operator that already owns each container:
  `matter::impact` excavates the terrain's share (its momentum share is transmitted into the planet
  the patch is attached to), `Aggregate::deposit_impact` couples a body's share into its parcels,
  `deposit_impulse` + `deposit_shock_heat` drive the debris share.

Inside the aggregate, each parcel's FATE is now `damage::classify` on the energy density deposited
in it, against its own material thresholds from `data/materials.json`: past Intact (cracked
through, molten, or vapor) it can hold no tensile bond, so its bonds end. Nothing says "destroy";
the pebble that stays under iron's fracture strength leaves every bond alive, and the asteroid that
exceeds it unbinds the radius the falloff sets. Melted parcels glow through the one
`emission::incandescence` curve like every other hot thing.

## The sentence, as tests

`simulation.rs`: `a_sufficient_meteor_shatters_the_ball_and_its_hottest_parcels_glow` (1,200 kg of
iron at 17 km/s: bonds collapse below half, rms spread more than doubles, the peak parcel
temperature emits, the same event craters the ground beneath) and
`an_insufficient_meteor_displaces_the_ball_and_it_survives` (300 kg at 60 m/s: momentum measurably
arrives, at least nine tenths of the bonds hold, nothing glows). Plus the door tests:
`one_impact_event_reaches_the_terrain_and_the_ball_through_one_door` and
`an_impact_event_heats_debris_grains_already_in_flight`.

Watched in the browser by `web/rig/ground_ball_shatter.mjs`: aim (the crosshair reports the first
matter the look ray meets - gold on the ball, red on the ground), drop, and the HUD's own bond
count collapses 212 → 0 while the shots show glowing parcels scattering into the new crater.

## Flagged IOUs (Law V), in domain terms

* **Shadowing and impedance.** The kernel is isotropic: a dense body between the site and the far
  crater wall should absorb what would have reached the wall, and energy crossing a material
  boundary should partition by shock impedance. The honest computation is shock transport through
  the actual contact network; until it exists the isotropic spreading-with-absorption kernel is the
  simplest geometric answer, and it under-couples a directly-struck body (the terrain behind it
  still receives its unshadowed share).
* **The impactor's own matter** still does not join the wreck (carried over from the ground impact).
* **A body's momentum share** is applied to its coupling core by `deposit_impact`; an event whose
  site is farther than the core length from every parcel deposits its (small) momentum share
  nowhere. The honest form spreads the impulse with the same kernel as the heat.
