# docs/46 — One physics: the charter, and the conformance ledger

> **The promise.** REAL physics — one law, at every scale, in every scene. A world is a world is a world.
> This is the product, not an aesthetic preference about code structure. An engine that answers the same
> physical question two different ways in two different scenes has broken its promise, however good each
> answer looks on its own.

This doc exists so the ledger below is **read, not rediscovered**. Every session so far has re-derived
some part of it from scratch. `docs/32 §4` maps the forks structurally; this doc states the *rule* that
makes a fork acceptable or not, and keeps the running list of open violations with their evidence.

---

## 1. The rule: specialization vs violation

Not every difference is a breach. The criterion is sharp:

- **Legitimate specialization** — the *physics itself* differs, so the numerical treatment differs.
  Stiff granular contacts need an implicit/semi-implicit integrator to stay stable; self-gravitating SPH
  needs a symplectic leapfrog to conserve energy over orbits (docs/38 §"what unified GPU path does NOT
  mean"). Forcing one integrator on both is unstable or ruinously slow. Boundary conditions likewise:
  the terrain band models Earth as a rigid heightfield because you are standing on a 96 m patch, not
  simulating a planet's interior.
- **DECLARED (a resolution IOU)** — the resolved physics is real and correct but **unaffordable at the
  timescale humans and models need in order to play with it and learn from it**. So the outcome is
  computed from the material's own data and rendered, instead of being allowed to emerge. This is
  legitimate, and it is where most of the interesting engineering will live for years.
- **Violation (a fudge)** — the *same physical question* gets two different answers. Two implementations
  of one law that can drift. A law implemented completely in one place and partially in another. A
  quantity conserved in one scene and not in the other. Or a number chosen because the result looked right.

### Declared is not fudged — the distinction that keeps it honest

A drive-shaft *should* shear when torque exceeds what its material can carry, and in a perfect world it
would do so because the bonds actually failed. That is far too expensive today. So we compute whether it
shears — real torque against the material's real shear strength — and render that outcome.

**That is not a second answer to the same question.** It is the same physics evaluated analytically
because the resolved evaluation sits below the resolution we can afford. The test that separates it from
a fudge:

> **Can you name the resolved computation this replaces, and would the declared answer converge to it as
> resolution rises?**

If yes, it is a declared model carrying an IOU. If you cannot name it, it is a fudge wearing a physics
coat.

The codebase already does this well, and those are the pattern to copy: `plough_loft` declares an
excavation shock finer than a grain as a **conserved momentum transfer**, says so outright, and notes
that co-motion is the physical maximum so there is no coefficient to tune; `Furrow::ejection` marks its
velocity SCALE honest and its distribution SHAPE an explicit resolution IOU *"to be DELETED once particle
count is high enough for the flow to emerge on its own."*

Compare the fudges: `MAX_EJECT = 0.045`, `steep_drop = 3` (retired 2026-07-19, docs/45), a hardcoded
damping ζ. None names a resolved computation; none would converge to anything.

### The horizon — write IOUs so a descendant can delete them

This layer is a **compute-era limitation, not a design preference**. The purpose is to render real
physics on the timescale and compute humans and LLM models need to enjoy and learn from it. If compute
becomes cheap enough — quantum or otherwise — descendants of this engine should be able to delete the
calculate-and-render step and simply let the drive-shaft shear.

That only works if IOUs are written *for deletion*: each declared model must state the resolved
computation it stands in for, so retiring it is a substitution rather than an archaeology project.
**A declared model with no stated resolved counterpart is a fudge, because nobody can ever retire it.**

The test any change must pass: **does this reduce the number of places where one physical question has
two answers?** A declared model does not add an answer — it defers one, in writing. If a change adds a
genuine second answer, it needs a physics reason, written down, or it is debt.

## 2. What is already unified (do not fork these)

From docs/32 §4, and it is a real achievement worth protecting:

`granular::Contact` + `contact_accel` · the SPH kernel `sph_w`/`sph_dw` (one kernel for `AirField` AND
aggregate vapor) · `Furrow` + `ExcavSurface` + `ejection` + `ejecta_energy_scale` (ONE excavation
primitive for the terrain meteor and the giant impact) · `plough_loft` · `deposit_resting_grain` (one
de-resolution primitive, CPU and GPU) · `Body` as the universal particle · `Material`/`LayeredBody` as
the universal matter description · `zeta_for_restitution` (bond damping and contact damping from one
derivation).

## 3. The conformance ledger — open violations

Each entry: what has two answers, the evidence, and where it is tracked. Structural forks 1–4 are
docs/32 §4's; the rest were found since and are recorded here so they are not rediscovered.

| # | one question, two answers | evidence | tracked |
|---|---|---|---|
| 1 | **Two container universes** — `Aggregate` (`Vec<Body>`, f64) vs voxel `World` + GPU f32 | docs/32 §4.1 | docs/33 |
| 2 | **Four integrators over one law** | docs/32 §4.2 | docs/38 (partly legitimate — see §1) |
| 3 | **Rigid-boundary fork** — in an impact Earth is simultaneously materialized grains AND a rigid boundary | docs/32 §4.3 | docs/33 |
| 4 | **Two rigid-body reps** — `body::Sphere` vs the cohesive-`Aggregate` probe | docs/32 §4.4 | docs/38 4c′ |
| 5 | ~~**Slope stability is half a law.**~~ **CLOSED 2026-07-19.** Terrain and grains now read the same `friction_coefficient` through one law, `granular::face_stable`; `steep_drop` is retired | was: `granular.rs:73` vs `matter.rs:538` (`h_crit = c/ρg` alone), non-convergent at 106→622 grains/pass. Now: fixpoint on the second pass, settled slope asserted against the DB μ, pristine terrain a no-op (470→0 grains) | **docs/45 §7** |
| 6 | **The de-resolution ladder stops one rung short.** grain→voxel works; voxel→field does not exist | measured: meteor peak 3,605 grains → 78, **98% returned**; but `patch_resolved` is set `true` once and **never** set back — grep shows no writer of `false` after init | docs/39 item #4 (deferred) |
| 7 | **Promotion is gated visually in one doc, physically in another.** docs/30: the trigger must be "a physical error bound … never a visual one". docs/39: gate on "camera-visible ∧ interacting" | the two docs, directly | **docs/44** |
| 8 | **The honest footprint is computed, then discarded.** `crater_radius` from `V = E/σ` is derived, then clipped by `MATERIALIZE_CAP = 14.0`, and `resolve_patch` resolves the whole 96 m patch regardless | `lib.rs:858`, `lib.rs:992` (self-flagged) | **docs/44** |
| 9 | **Matter leaks at the seam.** ~2% of debris never returns to the field (deposition refused inside a dynamic body; the water branch is a self-flagged static-sea placeholder) | measured: 78 of 3,605 grains stranded per event, monotonic | this doc |
| 10 | **Vehicle/probe never contacts debris.** The probe is a CPU `Aggregate`; grains live in a GPU buffer. Coupling is a bounding-sphere exclusion, not contact | `lib.rs` settle path; `matter::couple_body` exists but is called only from tests | docs/38 4c′ |
| 11 | **An asteroid-era constant still runs in an Earth-g scene.** `MAX_EJECT = 0.045` m/s, capped "below the world's ~7 cm/s escape velocity" — a 0.1 mm ballistic hop at 9.81 m/s² | `matter.rs:41` | this doc |
| 12 | **The render asserts a physical state the simulation does not have.** Every Earth scene draws an honest Rayleigh sky over a **vacuum**: `atmosphere::AirField` — pressure-layered, with verified hydrostatic balance, momentum-conserving drag and hypersonic entry heating — is instantiated in **no scene** | `grep AirField crates/engine/src/lib.rs` → nothing; 11 atmosphere tests pass regardless | **docs/48** |

**A distinct violation class (item 12).** Rows 1–11 are *one question, two answers*. Row 12 is different:
the optics are honest and the world beneath them is empty. It inverts *physics drives the render* — not
by faking the picture, but by leaving the picture's subject unbuilt. It is invisible to every existing
check, because the atmosphere tests pass, the sky tests pass, and nothing asks whether anything
**instantiates** what they describe.

### The wiring pattern — look for this before building anything

Three independent cases now share one shape: **the law is built and proven, then wired into one place or
none.**

| verified physics | wired into |
|---|---|
| docs/39 JIT particalization (conserving to <1e-12) | planetary scale only; terrain **never** |
| `granular::terrain_contact_resolve` (energy-monotone, hardware-verified) | GPU grains only; bodies **never**, until PR #15 |
| `atmosphere::AirField` (hydrostatic, drag, entry heating) | **nothing** |

The instinct on finding a gap has been "we need to build X". The evidence says the likelier truth is "X
exists, verified, and nothing calls it." **Grep for the primitive before writing one** — the
second-cheapest move after reading the docs.

Items 5, 6, 9 and 11 are the ones a "same in every scene" reading makes urgent: they are places where the
*same matter* behaves differently depending on which scene or which rung of the resolution ladder it is
on.

## 4. The scale test

A world is a world is a world. So for any primitive, ask: **does it run at both ends?**

The de-resolution cycle is the worked example. docs/39 proves
`field → particalize → simulate → quiesce → bake_back → field` at planetary scale, conserving mass,
momentum and COM to **<1e-12**. It is listed there as item #4 that terrain is "the separate low-energy
instance of the same primitive, after planetary scale." Measurement (§3 item 6) confirms terrain has only
the first half. The primitive is proven; it has simply never been instantiated at the second scale.

That is the shape of most remaining work: **not new physics — the same law, run at the other end.**

## 5. What this forbids

- Tuning one side to agree with a known-broken other side. docs/45 records the live case: terrain repose
  implemented correctly would disagree with grain repose, because the grain side under-predicts for its
  own flagged reason (spherical grains roll). Making them agree by adjusting the correct half would look
  consistent and be wrong.
- Constants that read as physics. `steep_drop = 3` (a 72° slope — RETIRED, docs/45), `MATERIALIZE_CAP = 14.0`,
  `MAX_EJECT = 0.045`, a hardcoded damping ζ. Each was defensible when written and each silently became a
  lie when the world around it changed. A number is either derived from a material datum / conservation
  law, or explicitly flagged as a budget with an IOU (docs/24).
- A second implementation of a shared law "just for this scene". If the scene genuinely needs different
  physics, that is a docs/NN, not a copy.

## 6. How to use this doc

Before adding physics: check §2 for an existing primitive to extend. Before accepting a fork: apply §1.
After finding a new violation: **add a row to §3 with evidence**, so the next session inherits it instead
of rediscovering it.

---

**Related:** CLAUDE.md (the charter in brief) · docs/23, docs/24, docs/28 (the one-law charter docs) ·
docs/30 (temporal deltas; the physical-bound rule) · docs/32 §4 (the structural fork map) · docs/33
(realignment) · docs/38 (composite contact law; legitimate integrator specialization) · docs/39 (the JIT
particalization primitive, proven at planetary scale) · docs/44 (resolution by necessity) · docs/45
(terrain slope stability).
