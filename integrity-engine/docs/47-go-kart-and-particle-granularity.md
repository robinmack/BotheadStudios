# docs/47 — The go-kart: particle GRANULARITY scales with the interaction, and what a wheel needs

> **The principle (the axis docs/44 left out).** docs/44 sizes the *extent* of resolution — how much
> volume to particalize. It says nothing about **granularity** — how FINE those particles are. Both are
> set by the interaction, not by a global constant. A meteor interacting at 10 m gets metre grains; a
> tyre contact patch interacting at 2 cm gets centimetre grains. **In the same scene, at the same time.**
> Particles are created when needed and returned to the field (heightfield + bump/normal map) when done.

The go-kart is the first artifact that forces this, and the first place DECLARED models (docs/46 §1) have
to earn their keep. This doc settles the design before code.

---

## 1. There is no global particle size, and treating one as global is a bug

`PROBE_LATTICE = 1.0 m` and `DEBRIS_PART_HALF = 0.5 m` are **not the engine's resolution**. They are the
resolution the *terrain-debris instance* happens to use, because meteor ejecta interacts at metre scale.
Reading them as a floor produces the absurd conclusion that a go-kart (1.9 m long, 0.28 m wheels) is
"too small for the engine" — a wheel would be a quarter of one particle.

This is already the engine's stated architecture, not a new idea: docs/08 (*"the coarsest resolution that
still looks and behaves right for the current context"*), docs/13 (*"cost scales with what is observable
from the current viewpoint"*), docs/39 (one JIT particalization primitive, `field → particalize →
simulate → quiesce → bake_back → field`), docs/44 (extent from energy and yield). **Granularity is the
missing axis of the same idea.**

### The sizing rule

A resolved region must be fine enough to represent the interaction that justified resolving it:

```
particle_size ≲ L_contact / N_across
```

where `L_contact` is the smallest length the interaction actually needs to distinguish, and `N_across`
is how many particles it takes to represent it (a contact patch that can develop a pressure
distribution needs several, not one). The same rule at both ends:

| interaction | `L_contact` | particle size | why |
|---|---|---|---|
| meteor crater, 14 m | ~metres | **1 m** (today) | ejecta ballistics do not care about centimetres |
| kart tyre patch, ~6 cm | centimetres | **~1 cm** | below this the patch cannot spread or rut |
| kart chassis (bulk) | ~decimetres | **~5 cm** | only needs to hold shape and fail honestly |

Nothing forces one number on the scene. The kart brings its own, the crater keeps its own, and the
banishment path (§5) is what stops either from accumulating.

### The acceleration structure has the same bug, and it must not be patched

The spatial hash is `cell_size = 2.0 * CONTACT_RADIUS` (`lib.rs:1291`), one global value, with the
invariant *"≥ contact diameter so contacts stay within ±1 cell"* (`particle_step.wgsl:32`). The obvious
way to admit mixed sizes is to make that cell track the LARGEST grain. **Do not.** It survives a 2×
ratio and collapses at 100×: a 1 m cell packed with 1 cm grains holds ~10⁶ of them, so every small
grain's ±1-cell scan degenerates to O(N). That does not merely run slowly — it defeats the acceleration
structure entirely, and at the span docs/46 commits to (a raindrop and a photosphere) it is absurd.

It is the same mistake as a global particle size, one level up: **there is no global cell size either.**

**The fix — a hierarchical hash, one grid per size class.** `cell_size(level) = base · 2^level`; a grain
is inserted into the level whose cell is ≥ its own contact diameter. A pair is discovered exactly ONCE,
at the finer of the two levels: each grain scans its own level ±1 cell and every **coarser** level ±1
cell, never finer. O(1) per pair, no double-counting, no pair missed.

**Why this stays affordable across the whole span, and why it is principled rather than a trick:** the
cost is set by the number of POPULATED levels near a point, not by the size range the engine can
represent — and that number is small (2–3) *because of* docs/13 and docs/44. Resolution follows the
interaction, so centimetre grains exist only where something centimetre-scale is happening; 15 orders of
magnitude are never live in one neighbourhood. The acceleration structure therefore ends up expressing
docs/13's own sentence — *cost scales with what is observable from the current viewpoint, not with the
size or contents of the universe* — instead of fighting it.

**On the GPU this is smaller than it sounds:** fold `level` into the hash key — `hash(level, ix, iy, iz)`
— keeping ONE table and the existing `cs_grid_clear`/`cs_grid_insert`/scan passes nearly intact, rather
than N separate buffers. Each particle derives its level from its own radius. Note the convergence with
the parked O(table) grid-clear item (`cs_grid_clear` already dispatches the full 262,144-cell table every
substep regardless of N): the epoch/generation-tag fix already identified makes the clear free, which is
what keeps a multi-level table from multiplying that fixed cost.

## 2. The kart at its own scale

Real dimensions: 1.9 m long, 1.3 m wide, wheels 0.28 m diameter, tread ~12 cm wide.

| part | spacing | particles |
|---|---|---|
| chassis | 5 cm | ~5,900 |
| each wheel | 1 cm | ~470 |
| **total** | | **~7,800** |

Against `MAX_PARTICLES = 60,000` that is ~13% — for a fully resolved vehicle. The kart was never
unaffordable; only a kart built out of metre cubes was.

**Note the two scales inside one object.** The chassis and the tyre do not need the same granularity,
for the same reason the crater and the tyre do not. Granularity is per-interaction, not per-object.

## 3. How a wheel spins — torque without a rotational DOF

There is **no orientation, angular velocity or inertia tensor anywhere in the codebase**
(`orbit::Body` is `{pos, vel, mass}`). That is not the blocker it appears to be.

A bonded `Aggregate` is a *particle cloud*. Apply a **force couple** to its particles — equal and
opposite tangential forces about the hub axis — and it spins. Angular momentum is then carried by the
particles' linear momenta, exactly as the planetary spin bookkeeping already assumes. **Torque emerges
from forces; we do not add a rotational degree of freedom.** That is the charter-compliant route, and a
rigid-body wheel with its own angular DOF would be the violation (a second answer to "how does matter
rotate").

**What genuinely does not exist: the axle.** A constraint that holds the hub at a fixed body-relative
offset while leaving rotation about ONE axis free. Bond the wheel to the chassis and it cannot turn;
leave it unbonded and it falls off. This is a revolute joint and there is nothing of the kind in the
engine (`Bond` is a distance spring — `{a, b, rest, active}`).

**Proposed:** the axle as a *constraint*, not a spring — the same shape as
`granular::terrain_contact_resolve`, which resolves rather than penalises, and is why the settling storm
went away. Per substep, project the wheel's particles so the hub stays at its body-relative offset and
angular velocity about the axle axis is preserved, removing only the components that violate the
constraint. Non-injecting by construction: it can never add energy, which is precisely the property a
penalty-spring axle would lose.

## 4. What is DECLARED here, and each IOU written for deletion (docs/46 §1)

The kart is the first real test of the declared category. Each entry names the resolved computation it
stands in for, so a descendant can delete it:

| declared | computed from | the resolved thing it replaces | deletable when |
|---|---|---|---|
| **motor torque** | commanded current × motor constant, capped by the material's real limits | electromagnetic fields in the stator/rotor | never worth resolving — this one is honest permanently, and should say so |
| **battery depletion** | ∫ P dt, P = τ·ω / efficiency | electrochemistry of the cell | ditto |
| **bearing friction** | a real friction coefficient × normal load at the hub | resolved contact in the bearing race | bearings are resolved as matter |
| **tyre grip** | Coulomb `μ·N` from rubber's μ | hysteretic, load- and slip-dependent rubber contact | a viscoelastic contact law exists |
| **drive-shaft shear** | real torque vs rubber/steel shear strength, outcome rendered | bond failure in a resolved shaft | shaft is resolved at bond scale |

The tyre-grip row is the one to watch: real grip is **not** Amontons–Coulomb. It is hysteretic,
load-dependent, and falls with both temperature and slip speed — which is why a locked wheel stops
gripping. `μ = 0.9` is a first-order stand-in, flagged in the material datum itself.

## 5. Banishment — the part that makes it affordable

Resolved particles must return to the field or the kart's cost is permanent. This is not new machinery:
docs/39 proves `bake_back` conserving mass, momentum and COM to **<1e-12** at planetary scale, and
`deposit_resting_grain` already returns grains to voxels — measured at **98% recovery** after a meteor
(peak 3,605 grains → 78).

**The gap is the last rung**, and it is docs/46 ledger item 6: voxel → field does not exist.
`patch_resolved` is set true once and never set back. So a kart driving across terrain would resolve a
rut behind every wheel and never give it back — the ruts become permanent voxels, and the cost
accumulates for the whole session.

**What a rut should become:** once quiescent and no longer under load, its geometry belongs in the
heightfield and its *detail* in a bump/normal map. The physics keeps only what still does work. That is
the same demotion docs/44 describes, and it is a hard prerequisite for a driving vehicle — not a
polish item.

## 6. Order of work

1. **Voxel → field demotion** (docs/46 item 6). Without it, driving accumulates cost without bound.
   **Step 1a landed 2026-07-19 — the mechanism is SAFE, but nothing triggers it yet.** §5 called this
   "not new machinery", which was true of `demote_column_to_field` itself and false of everything around
   it: the engine held **three different answers to "how high is the ground here?"**, so demoting a
   column had three silent consequences — the GPU grain heightfield read raw voxels and would have
   dropped every grain resting there through the floor; the rendered bulk cap read raw `terrain_height`
   and would have drawn a de-resolved crater as untouched ground; and `demote_column_to_field` sits on
   `World`, bypassing `MatterSim`, so the remesh dirty flag never rose. There is now ONE query,
   `World::ground_top_voxel`, and the GPU heightfield, the CPU bilinear surface and the cap all read it.
   A `demoted` flag disambiguates "baked into the field" from "excavated to nothing", which a zero
   displacement cannot.

   The useful discovery: **demotion needs no sub-voxel heightfield.** Because the bake preserves the
   surface exactly and that surface is already voxel-quantised, the field hands back the *identical*
   integer top, so the GPU's `array<i32>` is untouched. This deliberately does NOT entangle demotion with
   the deferred f32-surface refactor (docs/45's `SLOPE_QUANTUM_M` IOU).

   **Still open for 1b:** the quiescence TRIGGER (nothing calls demotion), and `patch_resolved` being a
   single bool for the whole 96 m patch while demotion is per-column — they do not compose. Also
   unresolved: `bulk_height` still returns pure procedural relief for a column that has been dug but not
   demoted, so the field/voxel seam is consistent only because `patch_resolved` gates which one is asked.
2. **The axle constraint.** The one genuinely new mechanism; test it on a single free-spinning wheel
   before any vehicle exists. **LANDED 2026-07-19 — `crate::axle`, 5 tests, no vehicle yet.**

   Built exactly as §3 proposed: a constraint, not a spring. `axle::resolve` does three things per
   substep — a velocity-decoupled position projection putting the hub back on its anchor (zero injected
   KE however far the chassis moved), a COM-velocity match reported as an impulse, and an angular split
   that preserves spin about the axle axis exactly while refusing everything else.

   The piece §3 left implicit and which turned out to carry the argument: **the wheel's angular velocity
   is recovered from linear momenta alone**, `ω = I⁻¹L` over the particle cloud. That is the mass-weighted
   least-squares rigid rotation, which is *why* the constraint is provably non-injecting — subtracting a
   least-squares projection can only reduce the residual. No rotational DOF is added anywhere, so §3's
   claim holds in code: a force couple spins the wheel and the axle passes the torque through untouched.

   Two properties worth naming because they are what a penalty joint could not give:
   - **An axle must not brake its own wheel.** A wheel already spinning freely and centred on its anchor
     is fully compliant, so `resolve` is a bit-level no-op on it. A joint that bled spin here would look
     like bearing friction while being a numerical artifact — and would be indistinguishable from the
     DECLARED bearing-friction model §4 owes a derivation for.
   - **It does not rigidify the wheel.** Only the best-fit rigid rotation is touched; deformation passes
     through, which is the whole point of a rubber tyre that has to spread a contact patch and rut.

   Reaction impulses are returned rather than applied, so the chassis receives the equal and opposite
   ones — a caller that drops them has an axle that creates momentum. Nothing calls it yet: there is no
   chassis to bolt it to until item 4.
3. **Multi-granularity particalization** — one scene, two particle scales, per §1.
4. **The kart**: chassis + four wheels + declared motor/battery/steering.

`rubber` is in the material DB as of this doc. It deliberately carries **no `thermal` block**: rubber
does not melt, it pyrolyses — an irreversible chemical change with no honest melt point — and the
schema's optional `thermal` is how it says "not characterised" (oak, concrete and ice do the same), so
`damage.rs` returns Fractured rather than ever claiming melt.

---

**Related:** docs/08 (adaptive resolution) · docs/13 (scale-relative simulation) · docs/23/24 (the
one-law charter) · docs/39 (the JIT particalization primitive) · docs/44 (resolution by necessity — the
*extent* axis) · docs/45 (slope stability) · docs/46 (the one-physics charter; declared vs fudge).
