# docs/44 — Resolution by necessity: the interaction's energy sizes its own compute footprint

> **The principle.** Resolve matter only where the cheap model *provably* differs from the honest one, in
> exactly the volume where that is true, on **both** sides of an interaction — then banish it. The extent is
> derived from the energy of the interaction and the material's own thresholds. It is never a radius we pick.

This is a direction doc, not an implementation plan. It states the rule, derives it for the two contact
regimes we care about, names where the engine currently violates it, and gives the acceptance test that
would prove an implementation honest.

---

## 1. Why this doc exists: two existing docs disagree

**docs/30** (accelerated compute) states the hard rule:

> the "recompute when drastic" trigger is a **physical error bound** (drift distance, local dynamical time),
> **never a visual one**.

**docs/39** (JIT particalization) then gates promotion on **"camera-visible ∧ interacting"** — a visual
criterion, and the weaker rule. docs/30 wins: *interest* decides what we **draw**; *necessity* decides what
we **simulate**. They are different questions and must not share a gate. An unwatched wheel still sinks.

Resolving that tension is this doc's job.

## 2. The rule we already have, pointed at the wrong target

`damage.rs` already derives extent from energy: a deposited energy `E` against a material's own strength `σ`
fractures a volume

```
V ≈ E / σ        (damage::crater_volume)      R = (3V / 2π)^⅓   (damage::crater_radius)
```

That is energy → breadth, from material data, with no dial. But it sizes **the outcome** — how big a hole to
dig — and is then thrown away for the compute decision:

- `lib.rs:858` — `const MATERIALIZE_CAP: f32 = 14.0; let mat_r = crater_r.min(MATERIALIZE_CAP);` The honest
  radius is computed, then clipped by a constant chosen to bound grain count.
- `lib.rs:992` `resolve_patch` — self-flagged: *"the resolved region is the WHOLE 96 m footprint, not yet a
  LOCAL rect sized from the impact's predicted crater radius."* Monotonic; never de-resolves.

So today: compute the correct footprint, discard it, resolve everything, clip with a constant. **The
generalisation this doc asks for is to stop discarding it** — and to use the same quantity as an *admission*
criterion, not just an outcome size.

## 3. The rule, stated

For any interaction, resolve the region where the material can actually respond, i.e. where the imposed
stress reaches the material's own yield/fracture threshold:

```
resolve { x : σ_imposed(x) ≥ σ_yield(material(x)) },   expanded by one correlation length
```

Everywhere else, the cheap model (rigid heightfield / bulk field) is not an approximation — it is **the
correct answer**, because the material provably cannot move. Resolving it buys zero fidelity.

Two consequences worth stating plainly:

- **The test is mostly a rejection test.** Its main job is to say *no*. A car on basalt never approaches
  yield; the honest footprint is **zero particles**.
- **It is bidirectional.** The same test runs on the object: a chassis resolves internally only where *its*
  stress approaches *its* yield. Normal driving → coarse/rigid. Impact at speed → the crumple zone resolves
  itself, by the same rule, with no special case and no scene flag.

## 4. Derivations for the two regimes

### 4a. Impulsive (impact, blast) — already have it

`V = E/σ`, `R = (3V/2π)^⅓`. Deposited energy against the struck material's own `fracture_strength`. This is
the existing `damage.rs` path; the change is to use `R` (plus a margin, §5) as the **resolved region**, and
to stop clipping it with `MATERIALIZE_CAP`. If a large `R` is unaffordable, that is a *budget* decision that
must be recorded as a resolution IOU (docs/24 style) — not silently folded into a constant that looks
physical.

### 4b. Quasi-static (a wheel, a footprint, a resting load) — the new case

A contact patch of area `A` under normal load `P` imposes `p = P/A`. Sub-surface stress decays with depth;
for an elastic half-space under a circular patch of radius `a` the axial stress on the centreline is

```
σ_z(z) = p · [ 1 − ( 1 + (a/z)² )^(−3/2) ]          (Boussinesq)
```

The resolved depth `z*` is the root of `σ_z(z*) = σ_yield`. No dial: `P` is the vehicle's real weight
distribution, `A` its real contact patch, `σ_yield` the material's own datum.

Worked, for a ~1500 kg car — `P = 1500·9.81/4 = 3679 N`, `A = 0.02 m²` ⇒ `p = 184 kPa`,
`a = √(A/π) = 0.080 m`. Solving `σ_z(z*) = σ_yield`:

| surface | σ_yield | resolved depth `z*` | grains @ 0.05 m, 4 wheels |
|---|---|---|---|
| basalt | ≥10 MPa | **none** — `p` is below yield everywhere | **0** |
| packed regolith | ~100 kPa | **0.096 m** (1.2 patch radii) | ~10³ |
| loose sand | ~10 kPa | **0.409 m** (5.1 patch radii) | ~10³–10⁴ |

Note the shape of it: yield strength drops 10× and the resolved depth grows ~4×, while competent rock
rejects outright. The footprint is small *because the physics says so*, not because we capped it.

**Flagged honestly:** Boussinesq is an elastic-half-space result and granular media are not elastic
half-spaces. It is used here only as a **conservative sizing envelope** for *how much to resolve*, never as a
force law — the forces remain `granular::contact_accel` + `terrain_contact_resolve`. Its error mode must be
over-estimation (§5). Replacing it with a granular-appropriate stress distribution is a clean later upgrade
that changes no physics, only the footprint.

## 5. The asymmetry that must govern every threshold

Under-resolving **silently loses physics** — the wheel doesn't sink, the crater doesn't form, and nothing
reports an error. Over-resolving **only costs compute**, and reports itself immediately as frame time.

Therefore every admission threshold is biased toward inclusion: expand the derived region by a margin, and
when the test is uncertain, resolve. A cheap footprint that is silently wrong is the failure mode this whole
engine exists to avoid.

## 6. Banishment

Demotion is the other half and is the cheap half. The machinery already exists and is verified: docs/39's
`bake_back` conserves mass / momentum / COM to **<1e-12**, and grain→voxel deposition
(`matter::deposit_resting_grain`) *never* deletes matter to lower a count.

Demote on **quiescence** — the region's kinetic energy falls below the level at which the resolved and cheap
models are indistinguishable within the stated bound. Not "when motion stops" (a visual criterion again),
and not on disinterest.

What does not exist yet: **voxel → bulk (T1 → T0)**. `patch_resolved` (`lib.rs:992`) is monotonic — dig once
and that 96 m patch stays voxels for the session. Closing that is what makes "banish it" real rather than
aspirational.

## 7. Acceptance test — the thing that makes this honest

Same discipline as docs/30's "accelerated force must equal brute force": **a necessity-resolved run must
agree with a fully-resolved run, within a stated bound, on the physical observables.**

Concretely, for each regime:

- **Convergence sweep.** Loosen the admission threshold toward "resolve everything" and show the observable
  (crater volume, rut depth, sinkage, drawbar pull) converges — and that the necessity-sized footprint is
  already inside the bound. If it is not, the threshold is wrong, not the reference.
- **Null case.** A load below yield must produce **zero** resolved particles and an answer *identical* to the
  pure-heightfield path. This is the cheap half of the win and it should be exactly, not approximately, free.
- **Energy monotonicity.** Promotion and demotion must not inject energy. `bake_back` already meets this at
  planetary scale; the terrain instance must be held to the same bar (`gpu-verify` scene I is the template).

**Caveat inherited from the current harness:** `tools/gpu-verify` is not yet run-to-run reproducible (same
card differs against itself at the magnitude of real effects). Convergence bounds asserted here are only as
tight as that noise floor, so the determinism work is a prerequisite for stating them numerically.

## 8. Where the engine stands against this

| | status |
|---|---|
| `V = E/σ` energy→extent | **exists** (`damage.rs`), used for outcome only |
| impact footprint sized by it | **no** — clipped by `MATERIALIZE_CAP = 14.0` (`lib.rs:858`) |
| resolved region local to the event | **no** — whole 96 m patch (`lib.rs:992`, self-flagged) |
| quasi-static admission test | **the LAW exists** (`resolution.rs`, verified vs the §4b table); NOT wired into a scene yet |
| object-side (internal) admission | **does not exist** |
| promotion gate | interest (docs/39), should be necessity (docs/30) |
| demotion / bake-back | **proven at planetary scale**; terrain instance not built |
| voxel → bulk de-resolution | **does not exist** ("increment 3") |

## 9. Direction (not a schedule)

Ordered by leverage, each independently verifiable:

1. **Stop discarding the derived extent.** Use `crater_r` as the resolved region for impacts; if it must be
   bounded, record the bound as an explicit IOU rather than a constant that reads as physics.
2. **Local resolution rects.** `resolve_patch` takes an extent instead of flipping the whole footprint —
   the refinement its own comment already asks for.
3. **The quasi-static admission test**, which is what unlocks a vehicle: contact pressure vs `σ_yield`,
   defaulting to zero particles on competent rock. **The LAW landed 2026-07-20** (`resolution.rs`:
   `resolved_depth`, `admission_depth`, closed-form Boussinesq root, verified against the §4b table to the
   millimetre — basalt 0, regolith 0.096 m, sand 0.409 m). Still to do: WIRE it — a resting/rolling load
   samples the surface material, and `admission_depth > 0` triggers materialization of the ground under
   the patch (and `= 0` resolves nothing, the null case). Nothing calls it yet.
4. **Close the demotion loop for terrain** (voxel→bulk on quiescence), making banishment real.
5. **Object-side admission** — resolve a body's interior only where its own stress nears its own yield.

Steps 1–3 are each small and each delete a flagged fudge. None of them requires new physics — only that we
stop overriding the physics we already compute.

---

**Related:** docs/22 (de-resolution), docs/24 (resolution IOUs), docs/30 (temporal deltas; the physical-bound
rule), docs/33 (realignment), docs/38 (composite contact law), docs/39 (JIT particalization primitive).
