# docs/45 — Terrain slope stability: Mohr–Coulomb, not a magic step height

> **The principle.** Terrain fails by the same law its grains do. A face stands if EITHER friction holds
> it (slope ≤ the material's angle of repose) OR cohesion holds it (height ≤ its critical height). The
> engine implements only the second term. Adding the first is what makes terrain stability converge, and
> it uses a datum already in the material DB.

Direction doc. Written because a regolith horizon turned an unnoticed heuristic into an unbounded
landslide, and the fix is a term the physics has always had.

---

## 1. The law

Shear strength of a soil is Mohr–Coulomb:

```
τ = c + σ·tan(φ)
      ↑        ↑
   cohesion  friction (φ = angle of repose / internal friction)
```

Two independent ways for a face to stand. **The engine's granular contact already carries both** —
`granular::contact_accel` has a Coulomb friction cap `μ·N` (the `μ = tan φ` term, and its comment
literally says the cap "is the angle of repose") plus a cohesion term, and `gpu-verify` asserts that a
pile's repose angle emerges from μ rather than being imposed.

**The terrain side carries only cohesion.** `matter::materialize_steep_terrain` decides a face slumps by

```rust
let h_crit = m.fracture_strength / (m.density * 9.81);   // the c term, alone
if face_height > h_crit { /* slumps */ }
```

There is no φ term anywhere in terrain stability. That asymmetry is the whole bug: **grains know about
friction, ground does not.**

## 2. What that costs, measured

The missing φ term is hidden behind a constant. `materialize_steep_terrain` only *looks* at a column
when its face already exceeds `steep_drop = 3` voxels — a 3-in-1 step, i.e. it tolerates a **72° slope**:

| material | angle of repose (DB) | stable step | `h_crit` = c/(ρg) |
|---|---|---|---|
| gravel | 40° | 0.84 vox/cell | **0.00 m** (cohesionless) |
| sand | 34° | 0.67 | 0.00 m |
| dirt | 30° | 0.58 | 0.36 m |
| grass | 30° | 0.58 | 1.09 m |
| snow | 25° | 0.47 | — |
| basalt | 45° | 1.00 | **510 m** |

Everything between each material's repose angle and 72° is left standing while being physically
unstable. And when such a face does finally slump, it exposes a fresh sub-72° face on its neighbour,
which the rule also permits, which then fails for the same reason. **Measured** (crater + cascade, this
world, 2026-07-19): 308, 313, 314, 327, 326, 339, 335, 342, 350, 338, 332, 339 grains produced on
successive passes — no convergence — with an **8-voxel face still standing after 12 rounds**. It is a
landslide that eats the terrain, bounded only by a pass limit.

**Why nobody hit this before.** With a 1-voxel grass skin over basalt, basalt's `h_crit ≈ 510 m` holds
any face this world can make, so only the skin ever moved and one pass reached the answer. The cohesion
term alone was sufficient *because the surface was effectively cohesive everywhere*. Introduce a
cohesionless horizon — `h_crit = 0`, gravel cannot hold **any** vertical face — and the missing φ term
becomes load-bearing immediately.

## 3. The rule

A face is **stable** if either term holds it:

```
stable  ⇔  slope_angle ≤ φ(material)          (friction: loose material at repose)
        ∨  face_height ≤ c/(ρg)               (cohesion: a bank or a rock wall stands vertically)
```

Consequences, all of which are the physically right answer rather than a tuned one:

- **Cohesionless gravel/sand** (`c = 0`): cannot stand a vertical face at any height, but is perfectly
  stable as a **slope at ≤ 40° / 34°**. Today the first half is modelled and the second is not, which is
  why it never stabilises.
- **Cohesive dirt/clay**: stands a small vertical bank (0.36 m) *and* any slope below repose — which is
  why real cut banks stand until they weather, then relax to repose.
- **Rock**: `h_crit` dominates, cliffs stand. Unchanged from today.
- **Snow** (25°) is the shallowest repose in the DB — snow slopes relax further than anything else,
  which is the avalanche behaviour and comes free from the same rule.

**Convergence is by construction**: the end state IS the repose slope, so each pass strictly reduces the
excess slope and the process terminates at a physically meaningful surface — not at "no 3-voxel steps",
which is a shape nothing in nature is trying to reach.

`steep_drop` is retired. It exists only because there was no φ term to decide when to look.

## 4. Relationship to `matter::collapse`

`matter::collapse` is a *different* test — structural support / cantilever reach
`L_max ≈ √(σt/ρg)`, removing voxels with nothing beneath them. It answers "can this overhang exist?"
Repose answers "can this slope exist?" They compose and must both run:

| | governs | material regime |
|---|---|---|
| repose (new) | slope angle of loose material | cohesionless dominant |
| `collapse` (exists) | unsupported overhangs / cantilevers | cohesive dominant |

An overhang in gravel is impossible for BOTH reasons; a rock arch is permitted by both. Neither
subsumes the other, and running only one is how a cohesionless blanket ends up in an unbounded slide.

## 5. Where slumped material goes

Related and currently wrong, recorded here because the same change touches it. `materialize_steep_terrain`
places slumped grains "at rest at [their] own centre — mass + PE conserved". But **slumping releases
potential energy**: the material falls. Freezing it at its pre-slump height conserves a quantity that
physics does not conserve here, and leaves talus hanging in the void where the cliff used to be —
measured 2.75 m below the bilinear collision surface, because neighbouring columns still stand.

Slumped material should **fall** — released with the granular sim's gravity acting on it, coming to rest
where the pile reaches repose. That makes the resulting talus cone emergent, exactly as the grain-side
repose already is, instead of a placement decision.

## 6. Acceptance test

- **Convergence.** Repeated stabilisation passes reach a fixpoint; no unbounded grain production. This is
  the check the current criterion fails outright.
- **The end state is repose.** After stabilisation, no slope in a cohesionless horizon exceeds that
  material's φ, within one voxel of quantisation — asserted against the DB datum, not a literal.
- **A rock cliff still stands.** Basalt faces are unchanged; this must not flatten canyons.
- **Emergent agreement — BLOCKED, and worth stating plainly.** The real proof that this is one law and
  not two is that a pile of loose GRAINS and the TERRAIN's own stable slope reach the same angle for the
  same material. **That test cannot pass yet, and not because of anything in this doc:** `gpu-verify`
  scene D measures the grain side and it currently **under-predicts badly** — dirt (φ=30°) settles at
  ~0.2°, gravel (φ=40°) at ~0.5°. Its own comment names the cause: spherical grains ROLL, so they cannot
  hold rock's steep angle without rolling resistance / angular interlocking — *"a flagged limitation
  … never patched by cranking μ."*

  So the field side implemented here would be right while the grain side is known-deficient, and
  comparing them would fail for the grain side's reason. The agreement test is the correct final
  acceptance criterion; it becomes meaningful only once rolling resistance lands. Do not "fix" the
  terrain to match the grains in the meantime — that would be tuning the correct half to agree with the
  broken half, and the resulting number would look consistent and be wrong.
- **No matter deleted.** Every voxel that stops being terrain becomes a grain (`deposit_resting_grain`
  already never deletes matter to lower a count).

## 7. Status

**IMPLEMENTED 2026-07-19.** `steep_drop` is retired. The law is `granular::face_stable` (with
`repose_allowance` and `SLOPE_QUANTUM_M`); `matter::materialize_steep_terrain` applies it and iterates to
a fixpoint. 199 tests green, wasm clean.

Three things this doc did not anticipate, all found by measuring rather than by reasoning:

**a. The φ term alone does not converge — the REMOVAL TARGET was the bug.** §3 predicted convergence "by
construction", but adding friction to the old criterion still slides: the old rule cut a failing face all
the way down to its lowest neighbour, which does not relieve a slope, it *moves* it — the column behind
becomes the new cliff at nearly the same height. Re-measured here on a cohesionless horizon, the old rule
sheds **106 → 622 grains over 12 passes, monotonically increasing**, with a 13-voxel face still standing
(the doc's earlier 308→339 was the same failure on a different pit). Cutting to the *stable* height
instead turns a cliff into a talus ramp that climbs one column per pass and stops at the plateau. That is
what makes the end state the repose slope, and it is the half of the fix that mattered most.

**b. Cohesion must be judged on the material's own BANK, not on the drop.** §3's rule reads
`face_height ≤ c/(ρg)`, and `face_height` is the drop to the neighbour. In a layered world that is wrong:
a 1 m grass skin over basalt on ground that steps down 2 m is not a 2 m grass bank — grass holds its own
1 m (`h_crit ≈ 1.09 m`) and the basalt holds the rest (510 m). Judging the veneer against the full drop
**stripped 470 grains from a pristine world nothing had touched**, since any hillside steeper than the
skin is thin has a drop exceeding a veneer's critical height. Cohesion now uses the contiguous run of the
*same* material in the exposed face; friction still uses the drop, because slope is a surface property.
The two terms measure different heights, which is why they are an OR of two tests and not a max of one.

**c. Faces fail from the BASE.** Scanning down and stopping at the first voxel that holds is backwards
once (b) is in: a self-supporting sod skin shields the failing 10 m bank beneath it from ever being
asked. The lowest failing voxel is found first, and everything above it goes with it — basal failure,
which is how slopes actually let go.

**The quantum, stated plainly.** An integer heightfield cannot express a slope between 0° and 45°, so
enforcing repose at a one-cell baseline with no allowance would force every soil in the DB perfectly
FLAT. `SLOPE_QUANTUM_M = 1.0` is exactly one voxel of allowance — a **resolution IOU** (docs/24), not a
dial. Over a baseline of `r` cells it dilutes to `1/r`, so sustained slopes (the ones that carry a
landslide) are held to `atan(μ + 1/r)`; at `SLOPE_BASELINE_CELLS = 8` that is ~3.6° above gravel's true
40°. The continuous sub-voxel surface — deferred part (A) of the terrain-contact work — is what retires
it. A longer baseline shrinks the residual at O(r²) cost and is not the honest fix.

**§5 (where slumped material goes) is measured resolved, as a side effect of (a).** Grains are shed from
the wedge *above* the new stable ramp rather than from a column cut to its neighbour's floor, so they are
no longer left hanging inside the collision surface: worst penetration **2.75 m → 0.50 m, with 1.2% of
grains penetrating at all** (the regolith branch had made it 3.75 m). They are released at rest and fall
under the granular sim's gravity, so the talus cone is emergent as §5 asked.

**Still open, and deliberately:** §6's *emergent agreement* test — that a pile of loose grains and the
terrain's own stable slope reach the same angle — remains blocked on grain-side rolling resistance
exactly as §6 says. Nothing here was tuned to make the two halves agree.

**Consequence worth knowing before merging regolith:** on the *live* meteor scene this change is close to
a no-op (76 grains shed before, 0 now), because that world is a 1 m grass skin on basalt and both layers
are genuinely stable — which is §2's own point. The change earns its keep on cohesionless material, which
is precisely what `regolith-horizon` introduces.

---

**Related:** docs/22 (de-resolution), docs/23 (granular contact — where μ already IS the repose angle),
docs/24 (resolution IOUs), docs/38 (composite contact law), docs/44 (resolution by necessity).
