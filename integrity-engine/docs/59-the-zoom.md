# The zoom: celestial to local, one event (docs/59)

> North-star step 3 (docs/23): the Moon-Earth collision is a celestial energy event; zooming into
> ground zero materializes the local matter (the ball and a terrain patch) and runs the same impact
> thermodynamics there, with the state carried down from the celestial field. The ball's destruction
> at the small scale is the same event as the flash at the large scale, conserved across resolution.

This is the first production trigger for resolution-by-necessity (docs/44), which the charter calls
the largest gap between the promise and the code (docs/46). It is governed by Law III (simulate what
necessity requires), Law IV (the camera changes representation, never existence), Law V (every
deferred computation is a named IOU), and Law VII (methods and constants are sourced, not invented).

## What exists and what is missing

Exists: the SPH machine resolves both declared and live impacts through one assembly; the settled
field is read back each frame; the ball is declared matter in a world definition; surface-nets
meshing; the de-res ladder's downward rungs (zero production consumers today); Terra's cube-sphere
globe and fly camera.

Missing, as five separable pieces: one body definition serving both the orbital presence and the
local surface; a descent camera that holds f32 precision to 2 m above the surface; the camera-driven
materialization trigger; conserved initialization of the fine patch from the coarse field; and
re-coherence of the settled site into meshed ground.

## The method, sourced

**One-shot refinement at zoom commit, not continuous adaptivity.** No production impact code refines
continuously mid-run: giant-impact SPH holds equal masses and buys convergence with particle count
(Kegerreis et al. 2019, MNRAS 487:5029, arXiv:1901.09934), and the DART impact studies grade
resolution statically in the initial conditions, finest at the impact point with ~1% growth per
shell (Stickle et al. 2022; Owen et al. 2022, Planet. Sci. J. 3). Compressible astrophysical SPH has
one modern existence proof of true adaptive refinement (Nealon and Price 2025, PASA 42,
arXiv:2409.11470). The honest browser-scale version is therefore event-driven: when the camera
commits to the descent, split the patch once, at the most quiescent moment available.

**Splitting that conserves by construction.** Parent particles split on the icosahedron stencil with
a mandatory child retained at the parent position (Vacondio et al. 2016, CMAME 300:442). Children
inherit the parent velocity and specific internal energy; child masses sum to the parent's. Mass,
momentum, angular momentum, kinetic and internal energy are then exact up to rounding (Feldman and
Bonet 2007, IJNME 72:295). The scheme's entire error is a density blip at the interface, bounded by
the placement optimization (separation ~0.4 h, child smoothing ~0.9 h in the source; re-derive the
constants for our kernel offline). With a stiff Tillotson material a small density error is a large
pressure error, so:

**Relax, then release.** Sample the coarse SPH-interpolated fields at the child sites, then relax
child positions against that target density with a frozen clock and damped shifting, the coarse
exterior held as a guard band, before the event proceeds (the accepted initialization discipline:
Diehl et al. 2015, arXiv:1211.0525; interface treatment per Chiron et al. 2018, JCP 354:552).

**Interface discipline.** Resolution ratio at most 2 per interface, no cross-level interactions,
buffer shells between levels, and contamination as a first-class failure: a coarse particle
penetrating the fine patch invalidates the run and says so on screen (the cosmological zoom-in
practice, Hahn and Abel 2011, arXiv:1103.6031). A hidden smoothing-over here would be a fudge.

**Validation gate: crater scaling, not eyeballing.** The refined patch's crater is checked against
Holsapple-Housen pi-group scaling in the gravity regime (Holsapple 1993, Annu. Rev. Earth Planet.
Sci. 21:333; coefficients from the Holsapple-Housen v2.2.1 table, hard rock K1 0.012, mu 0.55 and
regolith K1 0.14, mu 0.4, with the coefficient vintage named in the test). Factor-of-two agreement
in rim diameter passes; when the crater approaches the body's own radius the check degrades,
explicitly, to an order-of-magnitude sanity bound, because pi-scaling assumes a point source.

**An energy ledger per event.** Kinetic, internal, and participating potential energy are audited
across the bridge and the drift shown, not hidden (the decomposition of Carter, Lock and Stewart
2020, JGR Planets 125, arXiv:1912.04936, whose reported 3-16% conservation errors traced to EOS
evaluation, the same term that will dominate ours). Fixed energy-partition fractions never enter
the code; the ledger computes each event's own split. Literature ranges (heating 20-60%, escaping
ejecta a few to 10%) appear only in tests as sanity bounds, cited.

## Order of work

1. One Earth: a single body definition owns the orbital body and its local surface patch.
2. Descent camera: floating-origin rendering to 2 m altitude, and the materialization trigger,
   which deliberately mirrors the moon-drop's resolution-distance idiom so the engine has one
   materialization pattern. These two can proceed in parallel after (1).
3. Conserved hand-down: split, relax, release, with the pi-scaling gate and the energy ledger.
4. Re-coherence: the settled site returns to meshed ground through the surface-nets rung.

## Status, 2026-07-23

Item 1 landed (one Earth, see the ledger row 16 narrowing). Item 2's trigger half landed together
with item 3's entry point: `crate::site` derives the view-necessity threshold (one coarse SPH
particle's matter share against the docs/49 angular budget, measured from the live field when one
exists) and the space band materializes the Ground Zero world's declared ball and strata patch
through `crate::refine` on the descending crossing, folding back through the docs/61 gauge on the
ascending one, bidirectional for the out-and-back demo arc, ledger and refusals on the HUD.

The hand-down section below then landed (2026-07-23): the declared site pre-resolves in
`load_site_world`, before any event, with the descent trigger kept as the general path (and the
only one when a mid-event load refuses with its measured speeds); during a live event the guard
band re-samples the coarse field once per coarse step (`site::resample_guards`), so the impact's
energy arrives as boundary state and `site::EventWindow` books the window on the HUD; and the
pi-scaling gate gained its end-to-end consumer (`refine::measure_crater_rim` reads the rim off
the field at the field's own quantum, prediction frozen from the measured contact state, verdict
or the stated sub-quantum refusal on the same line).

The out-and-back arc landed the same day (`crate::arc`): one continuous camera parameterization
from the manual rig's celestial framing down to ~1.4 km over the site, the view-resolution
distance of the site's finest materialized quantum, the deepest framing the current rung
honestly serves, with sim-time compression proportional to camera distance, flying the trigger
in both directions from one control on the Ground Zero page.

The site entered dynamics 2026-07-23, closing the hand-down's core. Two pieces. First, the
release gate resolved on real relief: the stall is a true fixed point (the site's relax
oscillates around 4.9e-2 to the 5000-iteration cap) caused by the release criterion's own two
smoothing scales - the children's sum reads at the child scale while the target is the coarse
field at the interface scale, and near rough relief the coarse field genuinely answers
differently at those two scales. The rung now judges a PROVEN stall against that measured
disagreement (`refine::scale_mismatch`, the same sites, the same denominator) and releases with
the residual and the derived bound both stated: the site releases at 4.8e-2 under its own 9.1e-2
mismatch; the strict 5e-3 remains the criterion for every converging run. Second, the released
parcels are cohesive matter through the ground scene's own builder (`CohesiveBody::from_parcels`
- one law for the site's ball and the ground scene's), stepped under bonds, gravity and the one
terrain-contact law (`site::SiteDynamics`); during an event the guard band's booked step-to-step
delta is delivered through the one door (`Aggregate::deposit_impact`), fate is
`damage::classify`'s per parcel, the ball's verdict is the ground scene's word, and the fold
stays gated by the docs/61 gauge. The demo beat is a native test at the seam: a moon-magnitude
boundary pulse shatters the site's ball by classify alone (measured: 0 solid / 42 fractured /
171 molten / 60 vapor, mass conserved, mid-event fold refused). The declared site also moved
onto the fall's own impact ring (lat 45, lon 0): a from-rest lunar fall is confined to the
orbital plane, so release timing fixes azimuth only, and a site off the ring's polar plane could
never be hit - the derivation lives in the world file's comment and the solver test asserts the
plane offset is ~0.

Open within items 2 and 3, carried by docs/46 row 18: the camera below the arc's derived floor
(needs the next rung down, and with it Terra's camera-relative-eye convention in the space
band); the N-material upload reaching the guards (the sampled state inherits the coarse field's
EOS collapse until then); site-local excavation of the floor law (the parcels carry the
crater's truth; the declared surface stays the standing floor); and the site clock's
real-second rate against the scaled celestial clock (a stated compute bound - verdicts are
deposit-driven and land at the coarse step).

## The hand-down, made concrete (after the trigger landed)

The camera-driven trigger exists; what remains is the event's energy reaching the fine site. Three
decisions, each following the sources already cited:

1. **The demo site pre-resolves.** The demo opens at the ball, so the ground-zero world materializes
   its declared site at load, before any shock exists. This is the literature's rule made structural:
   no code hands off state mid-shock; refinement happens ahead of where the shock will arrive. The
   descent trigger remains the general path for sites not declared in the world.
2. **Ownership is single and the guard band is the interface.** Materialization debits the coarse
   representation and the fold credits it back, exactly as the audit already books. During an event,
   the guard band re-samples the coarse SPH field each coarse step: the guards ARE the coarse field
   at the boundary, so the impact's energy arrives as real boundary state through the one contact and
   deposition laws, not as a formula. No parcel is counted twice; the audit ledger's drift bound is
   the conservation statement.
3. **Cold mid-event materialization stays refused.** Creating a new resolution interface inside a
   shock is the one act the interface discipline forbids; the refusal quotes the measured field
   speeds, as the trigger does today. A site that wants to witness an event must exist before it.

Validation is the next issue's test: conservation across the event window within the ledger's stated
bound, and the crater cross-checked against the pi-scaling gate with the coefficient vintage named.
Fidelity limit stated as an IOU: the coarse field carries two EOS materials, so the guard band's
sampled state inherits that collapse until the N-material upload reaches the guards.

## IOUs this design leaves open, named

- Melt, vapor, and comminution tracking in the fine patch (fractions per Gault and Heitowit 1963;
  O'Keefe and Ahrens 1977) are deferred; the ledger reports their absence.
- Impactor spin at the live hand-off remains a zero vector until per-body angular momentum exists
  in the N-body state (docs/58 item 3).
- Resolution chosen from impact energy and view scale beyond the fixed patch budget (docs/44,
  docs/47) stays deferred; the patch budget is a declared compute statement.
- Continuous adaptivity (split and merge during the event) is out of scope; merging cannot conserve
  kinetic energy exactly and is not needed for a one-shot zoom.

## Open questions (Robin)

- Where docs/58 item 7 lands collision routing determines who owns the trigger's entry point.
- Which representation owns the shared Earth: the world definition (docs/43) is the natural home,
  but Terra and the space band both carry private builds today.
- Whether the patch budget rides the existing 2400-particle statement or gets its own.
