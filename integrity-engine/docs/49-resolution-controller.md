# docs/49 — The core resolution controller: camera-driven granularity, necessity-driven existence

> **The principle (Robin, 2026-07-20).** Particle size changing with camera position is a **default, core
> engine feature for every live scene**, not a per-scene frill — "absolutely vital to the realism". As the
> camera descends orbit→ground, detail EMERGES; as it pulls back, it collapses to bulk. One controller,
> held by every scene, decides HOW each region is computed and shown.
>
> **The camera does not gate EXISTENCE — but it does choose MATH vs SIMULATION** (Robin). Physics that is
> happening but not visible is computed with cheap analytic math and PROPAGATED; its effects are simulated
> and rendered as/when they come into view. *"If the Moon slams into the planet on the opposite side from
> the camera, we know the impact energy and can compute the effects; those effects we render as they enter
> view."* Math is far cheaper than particles, so the invisible majority costs little.

This is the decision policy. It is built and verified (`crate::resolution::ResolutionController`, natively
tested); wiring it to drive materialization/demotion in each scene is the follow-on (§5).

---

## 1. Three regimes — existence is the physics', the camera chooses the representation

Two *different* questions, and conflating them is the charter violation docs/44 §1 and docs/30 exist to
prevent:

- **NECESSITY drives EXISTENCE** — *whether* a physical response happens. A physics question, decided by
  the admission test (docs/44 §4, `resolution::admission_depth`) or an incoming propagated effect: an
  unwatched wheel still sinks, an off-camera crater still forms (docs/30: "a physical error bound, never a
  visual one"). Existence is **camera-independent**.
- **THE CAMERA chooses the REPRESENTATION** of that physics — math or simulation — and, when simulating,
  the GRANULARITY. It never decides whether the physics happens; only how it is computed and shown.

The decision is `ACTIVE-PHYSICS × IN-VIEW`, giving three modes (`resolution::ResolutionMode`):

| | in view | not in view |
|---|---|---|
| **active physics** | **Resolved** — particle simulation + render, at camera granularity | **Analytic** — cheap math, propagate the effects, no particles |
| **no active physics** | **Bulk** (rendered at camera LOD) | **Bulk** |

- **Camera granularity** (Resolved only): a grain finer than one subtending the angular threshold at the
  camera distance is sub-pixel (docs/13). `camera_grain_radius = distance · angular_res`, linear. Grain is
  the **finer** of camera and physics need, clamped `[floor, bulk]`.
- **The load-bearing invariant, and its test:** active physics off-camera is **never Bulk** — it is at
  least `Analytic` (computed). Looking away changes the representation, never whether it is true.
- **The Moon example, in code:** a far-side impact is `Analytic` (energy known, ejecta propagated by math,
  docs/28); the region its ejecta enters is active AND in view, so it flips to `Resolved` — "render the
  effects as/when they come into view". A region is re-queried every frame, so the flip is automatic.

**Corrected from the first cut:** the two-state controller resolved for camera-closeness alone, which
would simulate undisturbed static ground just because you walked up to it. Wrong — simulation is for
ACTIVE physics that is visible; static ground stays Bulk (rendered finely). The camera drives math-vs-sim
for active physics and render-LOD for everything.

## 2. The camera-granularity law (not a tuned LOD curve)

```
camera_grain_radius(distance) = distance · angular_resolution        (floored at min_grain)
```

A grain of this radius subtends exactly `angular_resolution` at the camera; anything finer is sub-pixel.
Linear in distance — twice as far, twice as coarse is acceptable — which is docs/13's "detail emerges
continuously" made quantitative. `angular_resolution` is the **one legitimate fidelity dial**: it declares
a viewing tolerance (like render resolution), not a physical quantity, so coarsening it trades fidelity
for cost without touching any physics. Default ~1 mrad (≈ one pixel across a 60° field at ~1000 px).

## 3. Composition — the finer of the two

Where both a physics interaction and a close camera constrain granularity, the result must satisfy BOTH,
so it takes the **finer** (stricter). Necessity pins granularity to the physics need *regardless of
viewpoint* — an unwatched interaction resolves at the scale the physics requires; the camera term only
makes it finer when close. Grain is clamped to `[min_grain, bulk_grain]`: never finer than the floor (a
resolution IOU — the true floor is the material's own structure), never coarser than the bulk model (which
already is the answer above that scale).

## 4. What is built and verified

`crate::resolution::ResolutionController` — `camera_grain_radius`, `decide(RegionQuery) ->
ResolutionMode`, `Default`. 6 tests, all the properties above, including:
- active physics off-camera is `Analytic`, never `Bulk` (the camera cannot gate existence);
- the Moon example directly: far-side impact `Analytic`, its ejecta `Resolved` as it enters view;
- no active physics is always `Bulk` (static ground is not simulated just because the camera is near);
- Resolved granularity = the finer of camera and physics need, clamped [floor, bulk].

## 5. NOT done — wiring into scenes

This is the decision policy. Nothing calls `decide()` to actually materialize or demote grains yet.
Wiring it as the promised default touches, per scene:

1. **Per-region iteration.** Tile the visible/active area; for each region assemble a `RegionQuery`
   (camera distance; `necessity_depth` from `admission_depth` against the surface material under its real
   load, or the impact footprint; `interaction_grain` from `granular::grain_radius_for`).
2. **Promotion.** `decide().resolve` drives materialization at `grain_radius` — the multi-granularity path
   (hierarchical grid, landed) is what lets different regions carry different grain sizes at once.
3. **Demotion.** When `decide()` flips a region back to bulk (camera pulled away AND quiescent AND no
   necessity), demote it — which needs the voxel→field demotion TRIGGER (docs/47 step 1b; the mechanism
   is safe but untriggered) and, for terrain, per-column/rect demotion so the sea does not pin the patch.

**Two honest blockers for a VISIBLE demonstration:** (a) the three scene structs live in
`#[cfg(target_arch = "wasm32")] mod app`, so scene wiring is not natively testable — only wasm-check + the
rig, and the rig cannot composite WebGPU headlessly in the current environment; (b) the null case is
correct but invisible (a probe on basalt resolves nothing), so *seeing* the controller work needs a soft
surface under load — the regolith profile — which is itself parked. The controller is therefore landed as
the verified core, with wiring sequenced behind the demotion trigger and a soft surface.

---

**Related:** docs/13 (scale-relative simulation — the north star) · docs/44 (resolution by necessity — the
existence axis) · docs/47 §1 (granularity axis + the hierarchical grid) · docs/46 (the one-physics
charter — physics drives the render, never the reverse) · docs/30 (physical-bound trigger, not visual).
