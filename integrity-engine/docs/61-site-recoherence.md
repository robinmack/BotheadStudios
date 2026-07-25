# docs/61 — Site re-coherence: a settled particle field becomes meshed, standable ground

**Status: first increment landed 2026-07-23.** The batch downward rung exists
(`crate::recohere`), is conservation-tested natively, and has one production consumer (the ground
scene's impact-aftermath path in `simulation::Simulation::step`). The remnant-ball wirings that
motivated the work are flagged IOUs at the end — this doc exists so they are wired against a
built rung instead of re-derived.

## The gap

After an impact settles, the remnant stays a bare particle field forever. The observed case is the
SPH giant impact: the re-formed body is billboard particles for the rest of the session, and
nothing can ever stand on it. The de-resolution ladder's downward machinery all exists and is
verified — `matter::deposit_grain` (grain→voxel), `World::demote_column_to_field` (voxel→field),
`mesher::build_surface_nets` (field→walkable render) — but docs/46 row 6 records the missing
half: a TRIGGER. Nothing decides "this region's excitement has passed" for a whole field at once,
so nothing ever calls down the ladder on a remnant.

docs/44 §6 already states what the trigger must be: demote on **quiescence** — "the region's
kinetic energy falls below the level at which the resolved and cheap models are indistinguishable
within the stated bound. Not 'when motion stops' (a visual criterion again), and not on
disinterest."

## The rung (`crate::recohere`)

### The criterion, in physical terms

Both halves derive from the local gravity `g` and the binning resolution `Δ` (the voxel store's
1 m cell). Neither is a dial; change the planet or the cell and both move as the physics says.

- **Quiescent speed** `v_q = sqrt(2 g Δ)`. Below it, a particle's kinetic energy cannot buy a
  one-cell rise (`½v² < gΔ`): its remaining motion is sub-resolution, so the static field
  represents it without lying by more than the field's own quantum.
- **Sustained interval** `t_q = sqrt(2 Δ / g)` — one cell free-fall time, the region's own
  dynamical time at the binning resolution. Quiet for one continuous `t_q` means nothing could
  have crossed a cell during the window in which the demotion takes effect. This is docs/44 §6's
  bound stated concretely, and it deliberately repeats the docs/57 #4 lesson: the old
  `SETTLE_FRAMES` made the moment matter stops being matter depend on the host's step rate; a
  criterion in seconds derived from `g` and `Δ` cannot.

`recohere::SettleGauge` integrates the "sustained" half: the caller feeds it the region's peak
particle speed each physics step; one observation above `v_q` resets the clock, because a region
that jolts mid-window has not settled however quiet its average. In free space (`g ≤ 0`) the
interval is infinite and the rung refuses — nothing "settles onto ground" where there is no down.

### Conservation is the contract

`recohere::recohere_settled` refuses the whole region unless the gauge shows one sustained `t_q`
AND every grain is individually below `v_q` right now (a still-moving grain binned into a static
field would freeze real motion). On success it bins grains per column and material, accumulating
REAL mass in f64 (so the only error left is the inputs' own f32 quantization), and deposits each
whole voxel quantum `ρ_mat · Δ³` through **the one grain→voxel law** — `matter::deposit_grain`,
now a free function shared with the per-grain settle path, so the two rungs cannot disagree about
where matter may return (water displaces upward, dynamic bodies block, full columns refuse).

- Mass in = voxels out × the material's own quantum + a remainder that STAYS particles. Matter is
  never deleted to lower a count.
- Material identity survives: gravel comes back as gravel voxels, never as generic "terrain".
- Energy at the crossing is MEASURED, not zeroed (docs/46 row 17). The voxel store holds no
  thermal state, so a binned grain's remaining kinetic energy (settling is dissipation; bounded
  per grain by `m g Δ`, the criterion's own quantum) and its carried heat above ambient have no
  receiving sink yet. `Recohered::binned_kinetic_j` and `binned_heat_j` book both per column as
  energy-in minus remainder-out, with heat counted only where the material's specific heat is
  sourced (an unknown c stays unknown). Remainder grains keep their own motion and temperature.
  The deferred computation is the voxel-side thermal field itself; `Aggregate::drain_settled`
  and the per-grain settle path share the debt, unmeasured.
- The mesh is not this module's business: the surface-nets mesher renders whatever the world now
  holds, on the same dirty-flag remesh the per-grain path uses. Physics decides the demotion; the
  picture only reports it.

Native tests: a settled synthetic mound folds with mass conserved to f32 accumulation error and
material preserved (including a deliberately sub-quantum grain that must survive as remainder); a
still-moving region is refused with the world untouched; the criterion is seconds-of-quiet, not
frames (same simulated quiet at 0.1 s and 0.001 s steps settles identically, a mid-window jolt
resets the clock); and the energy ledger holds across the crossing
(`the_crossing_measures_the_binned_kinetic_energy_and_carried_heat`: energy in = energy still on
particles + energy measured into the audit, both parts checked against independently computed
expectations, within f32 accumulation).

## The production consumer

`Simulation::recohere_when_settled` (called every `Simulation::step`, so it runs in the Ground
scene and `bin/run-definition`): once no meteors are in flight and the whole remaining field has
been quiet for one `t_q`, everything the per-grain path left behind is offered back to the world
in one conserving pass; `take_dirty()` then drives the surface-nets remesh that renders the site
as ground again. `Simulation::recohered_voxels()` exposes the count, matter-accounting style,
and `recohered_kinetic_j()` / `recohered_heat_j()` accumulate the crossing's measured energy the
same way; `bin/run-definition` prints a `recohered` line with all three whenever the rung ran,
so the loss is a number a definition author sees, never a silent zero.

Honest scope note: on this CPU container the per-grain deposit usually empties the field first —
its grains are voxel-quantized by construction, so one-grain-one-voxel is exact for them. The
batch rung earns its place as (a) the REGION-level guarantee that a settled site ends as ground,
(b) the only conserving path for sub-quantum masses (the per-grain law would conjure matter:
pinned by `the_batch_rung_folds_a_settled_field_and_keeps_the_remainder`), and (c) the trigger the
particle-ball remnants below wire into.

## Flagged IOUs — the wirings this rung exists for

1. **The SPH remnant (the observed gap).** After the giant impact settles, `gpu_sph`'s readback
   already hands the scene a `Vec<SphParticle>` snapshot — positions, velocities, REAL masses and
   material indices, exactly a `FieldGrain` adapter away. The wiring point is inside the OrbitDemo
   assembly path, under active rework upstream, so it is deferred rather than raced: when that
   settles, the remnant body's settled region feeds this rung (against a body-frame voxelization,
   docs/39's bake-back at the other scale) and the surface-nets mesher makes the re-formed body
   standable. The criterion and conservation above are already scale-free.
2. **The cohesive-body wreck.** A shattered `CohesiveBody` in the ground scene stays an
   `Aggregate` particle ball forever, substepped every frame. Once
   `accretion::representation` says it is no longer a body and its particles pass this rung's
   criterion, they should fold the same way (lattice grains are voxel-quantized already). Not
   wired in this increment to keep the consumer surface minimal.
3. **The three physics debts, ledgered (docs/46 row 17).** Status as of 2026-07-23: the dropped
   binned heat (a) and the zeroed settle kinetic energy (b) are MEASURED at the crossing
   (`Recohered::binned_kinetic_j` / `binned_heat_j`, accumulated by `Simulation`, reported by
   `bin/run-definition`) because the sink they need, a voxel-side thermal field, does not exist;
   building it is the deferred computation, and the closing test is the conservation test flipped
   from measuring the loss to asserting the deposit. Instant consolidation (c) is DESIGN ONLY: a
   consolidation state (porosity and strength fraction) on re-cohered matter relaxing toward
   intact over a physical timescale, with the closing test named in the row. Nothing here is
   closed yet; the row exists so the loss is visible while it waits.

Related: docs/22 (de-resolution), docs/39 (bake-back at planetary scale), docs/44 §6 (demote on
quiescence), docs/46 row 6 (the ledger row this narrows), docs/57 #4 (settling made physical).
