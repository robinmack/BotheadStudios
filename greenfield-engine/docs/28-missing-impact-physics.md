# Missing physics in the giant-impact simulation (audit, 2026-07-12)

Six-dimension multi-agent audit of the Theia→Moon scene after Robin's on-screen findings ("nothing is
taken from Earth", "no gouge", "no atmospheric plume or plasma", the ~0.4 vs 1.0 lunar-mass deficit, and
a Theia-sized ghost conjured on Earth). 44 candidate gaps; the 28 that reached the adversarial verifier
were **all confirmed real, zero refuted** (16 un-adversarially-tested on quota). Every verdict cited
`file:line` it read. These are MISSING MECHANISMS, not dials.

## Root cause
**Earth is not a participant in its own catastrophe** — a rigid analytic sphere with a point-monopole
field and one spherical hole punched at first contact. Parent of "nothing taken", "no gouge", and most
of the mass deficit.

## Ranked missing mechanisms
**CRITICAL — the target cannot participate**
1. Target cannot deform/crater/flow: bulk Earth is `boundary` = sphere-minus-one-hole with monopole
   gravity (`aggregate.rs`), sheds mass only via the one-shot cap.
2. No shock→rarefaction ejection: target matter is lofted only by direct contact with the impactor.
   **CORRECTION (2026-07-12, measured once the provenance tag existed — step 1):** the disk is NOT
   ~100% impactor as first audited. The live split reads ~0.6 M☾ Earth-derived vs ~0.1–0.7 Theia in the
   bound disk — the slow Earth *cap* stays bound while fast Theia escapes (~5.5 M☾). BUT that Earth mass
   comes from the one-shot 2×-impactor cap (item 4's bookkeeping fudge), not a physical furrow. So the
   real deficits are (a) most of Theia escapes/isn't captured, and (b) the Earth contribution is an
   arbitrary cap, not shock-driven excavation along the track.
3. Excavation is one-shot at t=0, isotropic half-ball; no furrow along the oblique track, no
   re-materialization on the second pass.
4. Cap mass is bookkeeping (2× impactor by particle count), not ρ·V of the excavated region; ignores
   speed/angle.
5. The vapor disk uses the wrong math object: gas "pressure" acts only on contact overlap (returns 0
   beyond 2r), no continuum pressure field, stiffness frozen at 1 atm, pair law reads a boolean flag not
   temperature.

**MAJOR (17)** — heat-pass conservation violation (vapor pairs: gas force, solid heat); latent heat has
no reservoir (stored as fake sensible T → over-cools; binary vaporization); boundary friction/damping
heat destroyed; provenance not physical (Earth & Theia mantle both `peridotite`, debris untinted by
origin); atmosphere is a `create()`-time optics constant, receives no impact energy/momentum (no
blow-off; blue veil survives a planet-melting impact); no Roche physics; geologic promotion leaks L
(escapees demoted into Earth's mass+spin; non-promoted clumps' L destroyed).

**Fundamental resolution limit** — at N=384 the post-curtain disk is collisionless (two-body relaxation
noise dominates the collisional/viscous spreading that builds the disk). Canonical SPH uses 10⁴–10⁶.
"Watch the disk spread to a lunar mass" is not expressible at this N — an honest LOD ceiling.

## Bugs (render-truth), not physics — fix FIRST to stop conjuring mass
- `reset_moon` keeps the impact's `spin_l` (angular momentum survives a world reset).
- Theia ghost: `enter_geologic_time` double-draws; `FrameSnap.shattered` keyed to `moon_debris` presence.
- Moon-intactness keyed to a body-index flag (a 2nd impacting moon draws pristine).
- Healed surface + interior conjured by `create()`-time formulas; crater doesn't co-rotate with spin;
  oblateness only on shell grains (interior sphere protrudes); debris composition read live while
  positions lag (`swap_remove` desync → wrong tints).

### Status (2026-07-12) — render-truth pass, rig-verified on birth.html
FIXED and watched on screen (rig `web/rig/ghost.mjs`: birth → impact → geologic → replay):
- `reset_moon` now restores a snapshot `initial_spin_l` (0 for birth, modern day for moonfall) + zeroes
  `spin_angle`. After Replay the birth scene shows NO day (proto-Earth spin = 0) — the impact-induced
  spin no longer survives the reset.
- Theia ghost GONE: geologic frame is a clean cratered Earth (0 fragments, T+6380y), not a Theia-mass
  ball. `FrameSnap.shattered = moon_debris.is_some() || geologic`; geologic draw is bounded by
  `.take(debris_count)` so no stale/​double slots.
- Composition desync FIXED: `FrameSnap` now carries a `mats: Vec<usize>` snapshot; tints ride the SAME
  lagged fragment order as positions (was a live `moon_debris.mat_ids` read that reordered under drain's
  `swap_remove`).
- Interior protrusion FIXED: the bulk-interior sphere is now an oblate ellipsoid (equator +f/3, poles
  −2f/3, aligned to the spin axis) matching the shell figure — at the post-impact ~13% flattening it no
  longer pokes through the crust at the poles.

DEFERRED, with reasons (NOT papered over):
- Crater co-rotation: the physics boundary hole is inertially fixed BY DESIGN (`boundary_vel` carries no
  rotational shear — flagged "no spin yet", lib.rs). Co-rotating only the RENDER crater would desync it
  from where debris physically rains back (the inertial hole). Correct fix = make the ground co-rotate
  with spin (rotational boundary shear) — a PHYSICS change, folded into the excavation work below.
- Moon-intactness by body index (`k == 0`): the architecture holds a single `moon_debris` Option, so
  only one body can shatter regardless of index. A coverage limit of the two-moon scene, not a
  mass-conjuring bug; left until multi-impactor materialization is wanted.
- Crater-wall grains read the pristine `planet::earth()` geotherm each frame (create()-time formula):
  the deep geotherm IS ~constant on impact timescales, so this is the honest static value — impact
  reheating of the wall is part of the heat-pass work (item 4), not a render patch.

The rig frame that motivates the physics work: at T+11m the disk holds 0.41 M☾ but **3.58 M☾ escaped**
and 100% of it is impactor material — exactly the excavation/ejection deficit below (items 2–3).

## Highest-leverage single move
Make the target's near-surface REAL materialized matter the impactor ploughs through progressively —
provenance-tagged (Earth vs Theia), shock-ejected, re-materialized on later passes. The
mutual-materialization we have, minus three fudges: one-shot → progressive; point-hole → furrow;
untagged → origin-tagged. Collapses "nothing taken", "no gouge", and most of the mass deficit at once.

## Implementation order (each: pre-declared TDD test + rig-watch before any claim)
1. Render-truth bugs (stop conjuring mass) — cheap, removes the visible fudge.
2. Provenance as a physical attribute (source-body tag on every particle; render tints by origin).
3. Progressive/furrow excavation + shock-rarefaction ejection of target material.
4. Heat-pass + latent-heat + boundary-heat conservation fixes.
5. Atmosphere-as-matter in the impact scene (corridor/plume parcels; blow-off bookkeeping).
6. Honest LOD ceiling statement for the collisionless-disk limit.
