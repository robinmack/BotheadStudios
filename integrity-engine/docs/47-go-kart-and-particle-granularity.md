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
2. **The axle constraint.** The one genuinely new mechanism; test it on a single free-spinning wheel
   before any vehicle exists.
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
