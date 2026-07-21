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

**Why this stays affordable across the whole span.** Cost is set by the number of NON-EMPTY levels in
range, not by the size range the engine can represent. **Do not assume that number is small.** An
earlier draft of this section claimed "2–3", which was asserted and never measured; a scene with a
planet, terrain, regolith, a chassis and a tyre patch is already at five or six, and nothing bounds it
there. The structure must be O(non-empty levels) with **no hardcoded cap**, and must skip an empty level
in O(1) via per-level occupancy. If a scene wants ten levels, ten levels must work.

**What actually bounds it is the REPRESENTATION LADDER, not a scarcity of scales.** Extreme size ratios
never meet inside the contact solver. A 1 cm grain never contact-tests against a 10⁶ m body, because at
that separation of scale the large thing is not particles at all — it is a T0 field, a heightfield, or
an orbital body. They interact across representation boundaries (grain ↔ voxel ↔ field ↔ body), each
crossing a bounded ratio. **The hash only ever sees the ratios that coexist as PARTICLES at the same
instant**, which is set by the resolution policy (docs/13, docs/44), not by the span of the universe.
An infinite universe has infinite scales; the particle representation holds only the ones something is
actively doing physics at. That is docs/13's real claim — *cost scales with what is observable from the
current viewpoint, not with the size or contents of the universe* — and it is load-bearing here.

**Measure it, do not assert it.** Instrument the populated-level count per region in a real scene before
sizing anything around it. The number above was guessed once already and was wrong.

**The levels are DYNAMIC, because the observer moves.** This is not a fixed two-tier scheme for "big
grains and small grains" — it is the structure that lets docs/13's descent work: *"falling from orbit,
detail emerges continuously: star field → planet disk → landscape → terrain → the rock → its grains."*
Watch a kart from orbit and only coarse levels are populated; zoom to the contact patch and finer levels
populate under you while the orbital ones depopulate. So the hash must support levels appearing and
retiring at runtime with no fixed count — a grain's level is derived from its own radius each step, and
an empty level costs nothing to skip.

That imposes the same discipline the T0 demotion work already answers to: **a change of representation
must not move the world.** `World::ground_top_voxel` returns the identical surface before and after a
column demotes; a grain crossing a level boundary, or a region promoting to finer granularity as the
camera descends, must likewise not step, pop or inject energy. If the zoom is visible as a discontinuity,
the resolution machinery has become a fudge — the seam is exactly where "a world is a world" gets broken.

### The WGSL mirror — spec, hazards first (NOT YET WRITTEN)

`crates/engine/src/grid.rs` is the CPU reference, pinned to brute force. The shader mirrors it; the two
are not written in parallel, because parallel authorship is how this codebase acquired four ledger rows.

**Hazard 0 — the layout is declared THREE times and nothing checks them against each other. Fix this
BEFORE growing the struct, not during.** This is a docs/46 violation in its own right (one question,
three answers) and it has already fired once: `gpu-verify/src/main.rs:68` records `drag_cd` arriving as
0.0 because a mirror drifted, so drag was silently a no-op — *"a repr(C) mirror that drifts from its
shader fails SILENTLY: no error, just wrong physics."*

| declaration | compiled by |
|---|---|
| `GpuParticle` — `lib.rs:1922`, INSIDE `#[cfg(target_arch = "wasm32")] mod app` | **wasm only; native `cargo test` never builds it** |
| `Particle` — `tools/gpu-verify/src/main.rs:18` | native only |
| `struct Particle` — `shaders/particle_step.wgsl` | neither — validated at device creation, i.e. at runtime |

A layout change can therefore pass the native suite, pass `gpu-verify`, and still be wrong in the
browser — the three artifacts have disjoint coverage. `cargo check --target wasm32-unknown-unknown` is
necessary but NOT sufficient: it type-checks `mod app`, and rustc never sees the WGSL at all, so field
ORDER can still drift silently.

**The safe sequence:**
1. **Move the GPU-facing POD types out of `mod app`** into a natively-compiled module. They are plain
   `#[repr(C)]` structs with no wasm-only dependencies; the only reason they are unreachable from native
   tests is where they happen to sit. This alone puts them under `cargo test`.
2. **Do NOT try to delete `gpu-verify`'s replica by importing the engine's.** `tools/gpu-verify` is
   deliberately NOT a workspace member — it carries its own `[workspace]` table precisely so its native
   Vulkan `wgpu` build cannot leak into the engine's browser (WebGPU-only) wasm build through cargo
   feature unification. Making it depend on the engine crate would reintroduce the exact problem that
   isolation exists to prevent. Its replica must therefore stay a replica — and be BOUND by step 3
   instead of removed.
3. **Cross-check each Rust mirror against the WGSL, in BOTH places.** A test that parses
   `shaders/particle_step.wgsl` and asserts the Rust struct matches field-for-field (name, type, order,
   total size) — once in the engine's native suite, once inside `gpu-verify`. Rust cannot see the shader,
   so this is the only mechanism that catches drift against the authority, and it is exactly what would
   have caught the `drag_cd` bug. Because both mirrors are pinned to the SAME authority they cannot drift
   from each other either, which is what makes keeping two declarations safe rather than merely tolerated.
   This step carries the whole guarantee; steps 1 and 2 only decide who it protects.
4. Only then grow the struct for per-particle radius, changing ONE Rust declaration and one WGSL block,
   with the test failing loudly if they disagree.

Doing multi-granularity without step 3 means the wasm build is verified by nothing but a human reading
two files side by side.

**Hazard 1 — there is no spare pad in `struct Particle`.** It is exactly 64 bytes, four `vec4`s packed
`offset+u`, `vel+resting`, `color+material`, `emission+rho`; docs/38 consumed the last slot when `_pad`
became `rho`. Per-particle radius therefore GROWS the struct to 80 bytes and must be changed in lockstep
with the `#[repr(C)]` `GpuParticle` in `lib.rs` and with `tools/gpu-verify`'s replica. A layout mismatch
here does not fail loudly — it silently reinterprets fields. Change all three in one commit and re-run
`gpu-verify` before believing anything.

**Hazard 2 — `cs_forces` interleaves the `headroom` scan with the contact scan** inside the same 27-cell
loop. Wrapping a level loop around it changes how many times headroom is evaluated. Headroom must remain
a MINIMUM over all levels, computed once per grain, or the terrain projection cap silently changes and
the stack-ram it exists to prevent comes back.

**The change, in order:**
1. `Params`: add `base_cell : f32` and `max_level : u32`; keep `cell_size` until the flat path is gone
   so the two can be diffed against each other on the same scene.
2. `cell_of(pos, level)` → `floor(pos / (P.base_cell * exp2(f32(level))))`; `hash_cell(level, c)` folds
   the level into the key exactly as `grid::hash_cell` does. ONE table, so `cs_grid_clear` is untouched.
3. `cs_grid_insert`: derive the level from the particle's own radius (`grid::level_for`) and insert once,
   at that level only.
4. `cs_forces`: loop `l` from the grain's own level to `P.max_level`, 27 cells each. Enumerate a pair
   only when `l > own_level`, or `l == own_level && j > i` — the same once-only rule the reference proves.
5. Contact maths switches to `granular::contact_force`'s form: touch at `ri + rj`, force not
   acceleration, divided by each grain's own mass.

**Verification, and it is not optional:** `tools/gpu-verify` scene I is the fudge detector and its energy
monotonicity must hold at mixed sizes; a new scene should place a boulder among pebbles and assert the
same pair set the CPU reference finds. Note the harness silently selects the WRONG GPU on this box — check
which adapter it bound before reading any number.

### Deterministic scatter — the design for the multi-level cost (docs/47, spec 2026-07-20, NOT built)

The multi-level GATHER benched at ~21× (5 levels, N=60k) because a big grain must scan the FINE level to
find its neighbours — `(r_big/r_fine)³` cells. It is already DETERMINISTIC (`cs_grid_sort` made it so, and
the hierarchical grid runs bit-identical run to run), so this is a pure COST problem: get scatter's speed
while keeping that determinism.

**Scatter:** compute each pair ONCE from the finer grain — which scans only its own level and COARSER
ones, cheap — and write the force to BOTH grains. O(actual contacts), no big-grain fine scan.

**The obstacle:** scatter means many threads accumulate into one grain's force slot at once → atomic add.
**WGSL has no atomic float** (only `atomic<u32>`/`atomic<i32>`; forces today are `array<Accum>`, written
once per grain by the gather, no atomics).

**The solution — fixed-point atomic accumulation.** Scale each accumulated component to an i32 and
`atomicAdd`. Integer addition is ASSOCIATIVE, so the result is independent of thread order: deterministic
by construction, and `cs_grid_sort` becomes unnecessary for the force pass (it stays only if another pass
needs ordered buckets — check). `Accum` decomposes cleanly: the sums (`force`, `s_diag`, `s_off`,
`sv_nbr`) become fixed-point `atomicAdd`; `headroom` is a MIN, so `atomicMin` on an i32 (with the standard
float→ordered-int bit trick, or a fixed-point encoding since headroom ≥ 0).

**The catch, stated up front: this is NOT output-neutral.** Fixed-point rounds, so the result is not
bit-identical to the float gather — it changes the physics numbers within the fixed-point epsilon. That
gives up the exact-equivalence property the gather has, and REQUIRES re-verification: the `gpu-verify`
scene suite (especially I, the energy-monotonicity fudge detector) must still pass, with the epsilon
bounded below the physics tolerances. Because the noise floor is now zero (determinism landed), the
epsilon is cleanly measurable.

**The scale factor must be MEASURED, not guessed.** Pick S so that S·|component|_max < i32::MAX ≈ 2.1e9
without overflow, while 1/S (the resolution) sits well below the smallest force that matters. Instrument
the actual per-substep force-component magnitudes across the bench scenes FIRST; a wrong S silently
saturates (too large) or quantises away real forces (too small). Consider per-component scales, since the
stiffness tensor terms and the force differ in magnitude.

**Verification bar:** D0 still deterministic; G0 still finds cross-level contacts; uniform scenes match the
float gather within the measured epsilon (not bit-identical — re-baseline); scene I energy still monotone;
and the bench shows the multi-level cost drop from ~21× toward O(contacts). Do this with fresh full-detail
context — a wrong scale factor is a silent physics corruption, the exact failure this engine exists to
avoid.

**MEASURED 2026-07-20 — the WGSL mirror is correct but the multi-level GATHER is slow, and here is why.**
The CPU reference `grid::pairs_within` gets O(1) per pair by scanning own + COARSER levels only — legal
because it ENUMERATES pairs (compute once, both sides know). The GPU force pass is a GATHER: each grain
independently finds its own neighbours, so a big grain MUST scan the FINE level to see its small
neighbours — `(r_big/r_fine)³` cells. Benched on an RTX 5060 Ti (fine-dominated mix, per-frame GPU time):
uniform 5.5 ms → 5 levels 117 ms at N=60k (~21×). Uniform (`max_level = 0`) is bit-identical to the flat
grid and free. The cheaper route is symmetric SCATTER (compute each pair once from the finer grain,
atomicAdd force to both), which restores the coarser-only scan — but float `atomicAdd` order is
race-decided and would undo the determinism fix. Deterministic scatter (per-cell reduction or
sort-then-segment) is the real next step; until then mixed-size is correct and gated off by default.

**`max_level` is PER-FRAME and DYNAMIC — not a mode, and NOT a limit on changing size over time.** This
is the point most likely to be misread (it was, in review): it is the number of size classes coexisting
in one neighbourhood at one instant, recomputed every frame. Descending orbit→ground changes particle
size FREELY — the resolution policy demotes coarse grains and resolves finer ones under the camera, so
the live set is ~one scale per frame and `max_level` stays low (the cheap path). The bench's slow case is
a WIDE size ratio interacting AS CONTACTS at the same time and place (a 1 cm tyre patch on 0.5 m debris),
which the descent does not produce — even the coarse↔fine resolution boundary only puts ADJACENT levels
(2×, ~1 level) in contact, benched at ~3× not 21×. So the grid does not lock particle size; it makes
size-change-over-time cheap and only wide simultaneous ratios expensive. Caveat, and it is NOT the grid:
the resolution POLICY that drives the descent (demote coarse / resolve fine per view, docs/13 + docs/44)
is largely unbuilt — demotion is safe but nothing triggers it, and extent still clips at
`MATERIALIZE_CAP`. The grid is ready for the descent; the policy is the missing piece.

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
