# Development Journal

A running log of major milestones for the Integrity engine. Newest entries at the top.
Each entry records *what* changed, *why*, and *how it was verified*.

## 2026-07-20 — the GPU host code was never wasm-only: one line hid 700 lines from the suite

**What.** `gpu_sph.rs` compiles on **every** target now, not just wasm, and its three shader-facing
`#[repr(C)]` mirrors are pinned to `sph_step.wgsl` by in-crate tests (240 total, was 236). The single
thing that had required wasm was one `Rc<Cell<bool>>` in a `map_async` callback: wgpu bounds that
callback by `WasmNotSend`, a no-op on wasm but plain `Send` everywhere else. Replacing it with
`Arc<AtomicBool>` (Release/Acquire — the flag publishes a completed mapping that `take_readback` then
reads through `get_mapped_range`) removed the entire reason for the `#[cfg(target_arch = "wasm32")]`.

**Why it matters.** CLAUDE.md rule 3 recorded the consequence as a fact of life — "`gpu_sph.rs` has **no
in-crate tests**" — and docs/33 called it "0 in-crate tests **by design**". Neither was design. It was an
accident of one line, and it put ~700 lines of shipping GPU host code where native `cargo check` and
`cargo test` never compiled them. That is the same trap that once shipped a non-compiling commit claiming
181 passing tests. `gpu_layout.rs`'s header already stated the principle for the POD types it rescued
from `mod app` — *"nothing but their location was ever keeping them there"* — and it turns out to hold
for the wgpu host code too, which is the premise the container-unification step of the realignment rests
on: two containers cannot be unified while only one of them compiles in a given build.

**It immediately found a real drift.** `sph_step.wgsl`'s `Params` declares `omega` at byte 36 — a
rigid-rotation rate whose centrifugal term `ω²·(x,y,0)` the shader applies in `cs_relax` (`:253`) so a
body relaxes to its OBLATE equilibrium. The Rust mirror called that same slot `_p0` and treated it as
padding. **Offsets and sizes matched**, so any size/stride check passed; only a NAME-level comparison
sees it. Not wrong physics today (the host hardcoded 0.0, and ω=0 is exactly the non-rotating relaxation
intended) but a live latent hazard — reuse that "padding" as scratch and the body silently spins — and a
capability the host could not reach. Renamed to `omega` and documented; zero bytes changed.

**Verified.** Guard written RED first: the layout test would not compile against `_p0`, which is the
detection working (the `offsets!` macro takes field identifiers, so a Rust-side rename breaks the build
and a shader-side rename breaks the assert — both directions covered). Then green. Full native suite
**240/240 + 18 skipped**, `cargo check --target wasm32-unknown-unknown` clean, `wasm-pack` build clean.
Rig-watched on the 5060 Ti at the rebuilt wasm (`build 20260720.193215`): the birth-of-the-Moon scene
renders Earth as a particle aggregate with the sun-lit terminator and Theia inbound, and the terrain
reference scene renders hills + iron probe + shadow + water. **Caught mid-check:** the first rig run
reported green against a vite server started BEFORE the wasm rebuild — the wasm URL is cache-busted with
a build stamp vite computes at startup, so it served the OLD bytes. Restarted vite and re-ran; the
verification above is the fresh build. That trap is now documented in `rigshot.sh` and the rig README.

**One parser, not two.** The WGSL↔Rust offset checker moved out of `gpu_layout`'s test module into
`wgsl_layout.rs` (test-only) and both shaders' guards use it. Copying a parser is how one question
acquires two answers (docs/46, Law 2).

**Rig ergonomics (Robin's ask).** Rigs hardcoded whichever port their session used — 13 different dead
ones — and 30 of them wrote into a long-gone session's scratchpad directory. `PORT` (default 5173) and
`OUT` (default `/tmp/rigshot`) now come from `rigshot.sh`; 56 rigs rewritten, all 61 parse, previously
broken ones run with no env set. `birth_shot.mjs` defaulted to the PUBLIC site, so a bare run
screenshotted production and looked like a verified local change — it defaults to local now. **Robin's
correction, recorded in `web/rig/README.md`:** these rigs are one-off instruments built to choose a path
or fuel a doc, **not** a test suite — do not assume any of them is still relevant or valid, and a green
rig is not evidence. The README had also been recommending `xvfb-run`, the trap rule 4 exists to kill.

## 2026-07-20 — headless GPU rig verification WORKS (the linchpin) — real WebGPU renders captured

**What.** Solved headless visual verification — the tool that was missing, and the reason "unification"
kept being claimed without proof. `scripts/start-render-xorg.sh` + `scripts/rigshot.sh` capture a real
composited WebGPU render on the RTX 5060 Ti, headless. Verified: the terrain scene (green hills, the iron
probe + its shadow, water) and Terra (Earth globe, real continents/biomes) both rendered correctly.

**The two things it took, neither obvious:**
1. **A real GPU-backed X server.** `xvfb` is software — the GPU renders but the software compositor cannot
   read back the GPU swapchain, so `page.screenshot()` returns the DOM HUD over a BLANK canvas (the trap
   that made prior sessions believe headless WebGPU "can't composite"). A headless `Xorg` on the 5060 Ti
   (`PCI:2:0:0`, `AllowEmptyInitialConfiguration`, `-ac`) composites in hardware and screenshots capture
   the render.
2. **Match WebGPU to the compositor GPU.** Dawn defaulted to the 2070; presenting a 2070 render on the
   5060 X server threw `VK_ERROR_DEVICE_LOST`. `MESA_VK_DEVICE_SELECT=10de:2d04` (the Mesa device-select
   layer works for NVIDIA too) forces WebGPU onto the 5060 — adapter then reports `blackwell` and it works.

**Why it matters.** Every visual claim is now verifiable, headless, on the real GPU. The Laws require
rig-watching visual claims; until now I could not, so the scene-level realignment was unverifiable. It is
not any more. CLAUDE.md rule 4 updated (the old `xvfb` instruction was the trap).

## 2026-07-20 — the Analytic → Resolved hand-off as ONE central system, wired (docs/49)

**What.** `resolution::ResolutionField` — the single system that makes the hand-off an inherent engine
property, not per-scene wiring. A scene registers active physics as analytic `Effect`s; one `update()` per
frame propagates them by cheap math off-camera and materialises each the frame it enters view, through the
SHARED matter path. Wired into the terrain `Engine`. +4 tests (236 total), wasm clean.

**The correction that shaped it (Robin).** My first cut emitted opaque `ResolutionEvent`s for a per-scene
"backend adapter" to apply — which ENTRENCHES the forked particle containers (docs/32 §4) it should be
unifying. The charter is one particle system, one materialisation, different scales; the forks are the
violation, not a fact to design around. Rebuilt to materialise straight through the shared `MatterSim` —
no adapter. A scene on a different container is one that has not yet converged (docs/33), not a reason for
a second path.

**The other correction, from a failing test.** `materialize_region` EXCAVATES world voxels; an ejecta blob
in mid-air has none, so it made nothing. Robin's Moon example is CARRIED matter arriving — a deposit, not
an excavation. Added `MatterSim::spawn_region` (deposit carried matter as grains) as the shared primitive
for in-flight effects.

**Verified.** The Moon lifecycle end to end, native: an effect starts off-camera, propagates BALLISTICALLY
with ZERO particles created, and the frame it enters view spawns grains via the shared path; an effect that
never enters view is never simulated but is still TRACKED (existence is not gated by the camera). Wired
into `Engine::render` as one `update()` call + `register_effect`; wasm clean.

**Open (docs/49 §5):** no effect SOURCE feeds it yet (live scene unchanged); the other two scenes get the
identical call once they converge on shared matter (docs/33); grain size reaches the CPU store only once
the CPU `Particle` carries a radius.

## 2026-07-20 — resolution controller, THREE modes: math off-camera, simulate + render in view (docs/49)

**What.** Refined the controller (same day it landed) from a two-state resolve/bulk decision to THREE
regimes, per Robin: existence is the physics'; the camera chooses the REPRESENTATION — cheap MATH for
active physics that is not visible, particle SIMULATION + render for active physics in view. `ACTIVE ×
IN-VIEW → {Bulk, Analytic, Resolved}` (`resolution::ResolutionMode`). Still 233 tests (6 controller tests
rewritten), wasm clean.

**The model.** Bulk = no active physics (rendered at camera LOD). Analytic = active but off-camera:
compute the effect with math and PROPAGATE it (docs/28's giant-impact ejection is exactly this), no
particles. Resolved = active AND in view: simulate + render, at the finer of camera and physics
granularity. The Moon slamming the far side of the planet is Analytic (energy known); its ejecta flips to
Resolved the frame it arcs into view — regions are re-queried every frame, so "render the effects as they
come into view" is automatic.

**It also CORRECTED the first cut.** The two-state version resolved for camera-closeness alone, which
would simulate undisturbed static ground just because you walked up to it. Wrong: simulation is for ACTIVE
physics that is visible; static ground stays Bulk (rendered finely). The invariant sharpens — active
physics off-camera is never Bulk, it is at least Analytic (computed): the camera changes the
representation, never whether the physics is true (test: a far-side impact at 6,000 km is Analytic).

**Still not wired** (docs/49 §5) — decision policy only; nothing calls `decide()` to compute/materialize
yet. The Analytic→Resolved hand-off (materialize an incoming analytic effect as it enters view) is the
crux of the wiring and maps onto the existing docs/28 analytic ejection + docs/39 particalization.

## (superseded same day) the core resolution controller: camera drives granularity, necessity drives existence (docs/49)

**What.** `crate::resolution::ResolutionController` — the decision policy Robin named a default core engine
feature: one controller every scene holds, deciding whether matter resolves into particles and how fine.
New docs/49. +6 tests (233 total, wasm clean).

**The two axes and the one rule.** CAMERA drives GRANULARITY (a grain finer than one subtending the
angular threshold at the camera distance is sub-pixel — `camera_grain_radius = distance·angular_res`,
linear per docs/13). NECESSITY drives EXISTENCE (the admission test; an unwatched wheel still sinks).
They compose, and **the camera may only REFINE, never gate**: `resolve = necessity ∨ camera-close`,
`grain = finer_of(camera, physics)`. Letting the camera gate existence — so looking away changes what is
true — is the charter violation this exists to prevent (docs/44 §1, docs/30).

**Verified — the load-bearing test.** `necessity_resolves_even_when_the_camera_is_infinitely_far`: a
sinking wheel 100 km from the camera STILL resolves, at the 1 cm physics granularity, not the coarse
camera grain. Plus: the null case (far + unnecessary ⇒ resolve nothing, exactly free); camera-only visual
resolution that refines with proximity; composition = the finer of the two; granularity clamped
[floor, bulk]. `angular_resolution` is the one legitimate fidelity dial (a declared viewing tolerance,
like render resolution — not a physics fudge).

**NOT wired (docs/49 §5).** This is the policy; nothing calls `decide()` to materialize/demote yet. Two
honest blockers for a VISIBLE demo: the scene structs are in `#[cfg(wasm32)] mod app` (not natively
testable — rig can't composite WebGPU here), and the null case is correct-but-invisible (a probe on basalt
resolves nothing), so seeing it work needs a soft surface under load (the parked regolith) plus the
voxel→field demotion trigger (docs/47 step 1b). Landed as the verified core; wiring sequenced behind those.

## 2026-07-20 — deterministic scatter: built, MEASURED 2–12× WORSE, reverted (docs/47)

**What.** Implemented the full deterministic-scatter design for the multi-level gather cost — fixed-point
atomic accumulation (integer add is associative ⇒ order-independent, so `cs_grid_sort` dropped), each pair
visited once from the finer grain and written to both. **It was 2×–12× slower than the gather** and is
reverted; `main` keeps the gather.

**The numbers (RTX 5060 Ti, N=60k):** uniform 5.5→13 ms (2.4× worse), 3 levels 16→198 ms (12× worse),
5 levels 117→513 ms (4.4× worse). Correct and deterministic — D0 bit-identical, G0 cross-level contact
passes, scene suite 30 PASS / 1 pre-existing (scene D repose). It works; it is just slow.

**Why — atomic contention, which the design (mine) failed to price.** Many fine grains `atomicAdd` into
one big grain's accumulator slots and serialize, and every contact pays ~26 atomic ops (13 slots × 2
grains) versus the gather's register accumulation. So even the UNIFORM case — no wide ratios at all — lost
2.4×. The gather's "wasted" scan of fine cells is CHEAPER than scatter's atomic traffic for this workload.
This is the value of the ethos: the design reasoned scatter was O(contacts) and cheaper; the machine said
otherwise, and the measurement is the deliverable.

**Method note worth keeping:** the force-magnitude readback (`GPU_VERIFY_FORCEMAG`) measured the
fixed-point scales from real data (force ≤2.1e4, tensors <1, sv_nbr ≤8.5e2) rather than guessing them —
that part was right, and the scales worked (scene I energy monotonicity held). The flaw was in the cost
model, not the numerics.

**Left open:** the 21× multi-level gather cost is NOT fixed. Single-scale-per-frame (the camera-descent
common case) is cheap and unaffected. The code is preserved on the pushed `deterministic-scatter` branch
as an evidence dead-end (do not merge); docs/47 records the remaining options (accept it / coarse summary /
measured hybrid).

## 2026-07-20 — the quasi-static admission law: a car on basalt resolves exactly zero particles (docs/44)

**What.** New `resolution.rs` — the quasi-static admission test docs/44 §4b specifies and its §8 status
table lists as "does not exist". Given a contact pressure `p` over a patch of radius `a`, `resolved_depth`
returns the depth `z*` to which the load still reaches the material's yield — the region that must be
resolved into particles — and **0 when the surface pressure is below yield**, which is the whole point:
the test's main job is to say *no*.

**Why it is the vehicle unlock.** docs/44 §3: everywhere the material provably cannot move, the cheap
bulk model is not an approximation, it is the correct answer. So a 1500 kg car on basalt (p ≈ 184 kPa,
yield ≥ 10 MPa) resolves **nothing** — exactly, not nearly — while the same car on loose sand resolves
0.41 m deep. The footprint is small because the physics says so, not because a constant capped it.

**Closed form, not iteration.** `σ_z(z) = p·[1 − (1+(a/z)²)^(−3/2)]` is monotone in z, so the root
`σ_z(z*) = σ_yield` inverts exactly: `(a/z*)² = (1−yield/p)^(−2/3) − 1`. Verified self-consistent (the
Boussinesq stress at the returned depth equals the yield it was solved for).

**Honesty flags carried from the doc.** Boussinesq is an elastic-half-space result and granular media are
not elastic half-spaces; it is used ONLY as a conservative sizing *envelope*, never as a force law (forces
stay `granular::contact_accel` + `terrain_contact_resolve`). Per docs/44 §5 the bias is toward resolving:
`admission_depth` expands `z*` by one patch radius, because under-resolving loses physics silently while
over-resolving only costs frame time.

**Verified.** 227/227 (+4), wasm clean. Pinned to docs/44 §4b's worked table to the millimetre: basalt 0,
regolith 0.096 m, sand 0.409 m. The null case is asserted as EXACTLY zero, since it is the cheap half of
the whole idea.

**Open — this is the LAW, not the wired policy.** Nothing calls it. The full resolution policy is docs/44
§9's five steps; this is §9.3's law only. Wiring it (a resting load materializes the ground under its
patch when `admission_depth > 0`) is the next visible step and touches the terrain scene + GPU. Deploying
today ships the tested law but changes no live behaviour — deliberately, since a visual scene change
cannot be rig-verified in this environment (WebGPU does not composite headlessly here).

## 2026-07-20 — the hierarchical grid on the GPU: correct at every scale, and measured to be slow

**What.** `particle_step.wgsl` gained a hierarchical spatial hash — `cell_size(level) = base_cell·2^level`,
level folded into the hash key, one table. Insert at the grain's own level; the force GATHER walks every
populated level. `Params` gained `base_cell`/`max_level` (reusing the reserved `_hp1`/`_hp2` slots, so NO
struct-size change — the layout guards confirm it). `gpu-verify` gained scene **G0** (boulder vs pebble,
3 levels apart) and a **production-N mixed-granularity BENCH**.

**Why plumbing became capability here.** With `max_level = 0` the level walk collapses to the old ±1 scan
and the output is **bit-identical** to the flat grid (verified end-to-end, only possible because
determinism landed first). Above 0, grains of different size find each other for the first time.

**The measurement, which changed the conclusion.** I was about to call this "mixed granularity works". The
bench says: correct, but the multi-level GATHER is expensive, and the cost is inherent, not a bug.
Fine-dominated distribution (half the grains at each successive coarser level), RTX 5060 Ti, 24 reps,
per-frame GPU time (SUBSTEPS batched into one encoder, the engine's pattern):

| N | uniform (max_level 0) | 3 levels | 5 levels |
|---|---|---|---|
| 10,000 | 2.6 ms | 7.3 ms | 49 ms |
| 60,000 | 5.5 ms | 16.4 ms | **117 ms** |

Uniform matches the pre-hierarchy grid (~0.09 µs/particle at 60k). Five levels is **~21× slower**.

**The cause, understood.** In a gather each grain finds its own neighbours, so a BIG grain must scan the
FINE level to see its many small neighbours — `(r_big/r_fine)³` cells, most of them empty or inside the
big grain. That is intrinsic to gather + wide size ratio. The cheaper route is symmetric SCATTER (compute
each pair once from the fine grain, write force to both), which lets a big grain scan only its own level
and coarser — the O(1)-per-pair cost `grid::pairs_within` achieves on the CPU. **But scatter needs float
`atomicAdd` into the force buffer, whose order is race-decided — reintroducing exactly the nondeterminism
fixed hours ago.** Deterministic scatter is possible (per-cell reduction, or sort-then-segment) but is a
real design task, not a patch.

**Status, stated honestly.** Single-scale-per-frame (max_level low) is production-ready and free. Multi-level is CORRECT (G0) and
now MEASURABLE, but too slow for wide size ratios until the gather is replaced. It ships behind
`max_level = 0` (every live scene), so nothing in production pays the cost today. The next step is the
deterministic-scatter design, scoped by these numbers rather than guessed.

**Verified.** Engine 223/223; wasm clean; `gpu-verify` D0 + G0 pass, uniform scenes bit-identical to the
flat grid, run-to-run bit-identical; shader runs on the RTX 5060 Ti. G0 confirmed to have teeth (setting
`max_level = 0` makes the cross-level contact vanish and the scene fail).

## 2026-07-20 — GPU determinism: the spatial hash was summing forces in race order

**What.** New `cs_grid_sort` pass puts every hash bucket in a canonical (particle-index) order between
`cs_grid_insert` and `cs_forces`, in BOTH the engine and `gpu-verify`. `gpu-verify` gains scene **D0**,
which runs one input twice and compares — the check that gates every other tolerance in the harness.

**The cause, confirmed rather than assumed.** `cs_grid_insert` takes its slot from `atomicAdd`, so a
bucket holds the same SET of particles every run but in whatever ORDER the threads won the race.
`cs_forces` then sums contact forces in that order, and float addition is not associative. Identical
input therefore produced different output. Reproduced under control before touching anything: **7 of 174
grains diverged, worst 8.3e-5 m after 40 frames** — the seed that amplified into the ~6% spread measured
yesterday on scene E (33.1 / 33.5 / 35.1 m across three runs of identical code).

**Why it mattered more than a cosmetic wobble.** Scene I is the FUDGE DETECTOR, and its tolerance was
**wider than its own reproducibility**. Any regression smaller than the drift was invisible, and — as the
per-particle-radius change demonstrated the day before — a real change could not be distinguished from
noise. Every margin in the harness was unfalsifiable.

**Verified.**
- D0: **0/174 grains differ, worst 0.0 m** — bit-exact.
- **The FULL SUITE is now bit-identical across two runs** (diffed end to end). It was not before.
- Scene D (repose) still fails, on this branch and on `main` alike — pre-existing, unrelated, measured on
  both rather than recalled.
- Engine 223/223, wasm clean, shader confirmed compiling on the RTX 5060 Ti.

**Residual, and empirically covered rather than argued away.** If a cell OVERFLOWS `bucket_k` (16), which
particles won slots is still race-decided, so sorting cannot canonicalise the set. `grid_count` keeps the
true unclamped count, so it is detectable. It is not occurring anywhere in the suite today — that is what
a bit-identical full-suite diff demonstrates, since an overflow race would show up as exactly the
nondeterminism D0 tests for.

**Cost, flagged and deliberately NOT quantified here.** `cs_grid_sort` dispatches over the whole table
every substep — the same shape as `cs_grid_clear`, which was measured at ~0.52 ms/frame and is queue item
3. A similar fixed cost should be expected, but I am not quoting a number I have not measured
([[use-gpu-perf-skill]]). The mitigation is shared with item 3: touch only occupied cells (epoch/
generation tags), which removes the fixed dispatch from both passes at once.

## 2026-07-20 — per-particle radius on the GPU, and the harness that cannot tell if it mattered (docs/47 §1)

**What.** `Particle` grows to 80 bytes with a per-particle `radius` (plus a padded 5th row reserved for a
cached grid level). The shader reads THIS grain's radius everywhere a global grain size was assumed:
contact `touch = ri + rj`, headroom `length(dj) − (ri + rj)`, terrain penetration, drag cross-section,
and the resting test. `P.part_half` and `P.c_radius` — two constants that were both 0.5 and described the
same physical quantity — no longer drive per-grain behaviour.

**Plumbing, not capability.** Every grain is still given the same radius by the CPU, so mixed sizes are
not yet enabled; the hierarchical grid (`crate::grid`, landed, not yet mirrored in WGSL) is what makes
them correct, because a flat grid's ±1-cell invariant breaks the moment radii differ.

**Verified — and the interesting part is what could NOT be verified.**
- Both layout guards passed after the growth, which is the whole point of having landed them first: the
  80-byte struct is confirmed field-for-field against the shader in the engine AND in `gpu-verify`.
- Engine 223/223, gpu-verify 2/2, wasm clean.
- **The shader really compiles and runs** — `cargo build` does NOT validate WGSL (it is compiled at
  device creation), so this was checked by running `gpu-verify` on the RTX 5060 Ti.
- `gpu-verify`: **1 scene fails, and `main` fails exactly the same one** (scene D, repose — the
  pre-existing spherical-grain rolling-resistance deficiency). Measured on both branches rather than
  assumed from memory.

**What the harness could not answer.** The numeric diff between `main` and this change is real but
**smaller than the harness's own run-to-run noise**. Same code, same card, two runs: scene E spread
33.5 → 35.1 m, while baseline→change moved it 33.1 → 33.5 m. Scene I (the FUDGE DETECTOR) drifts
−6622 / −6807 / −6798 across runs of identical code. So the honest claim is **"no new failures and no
difference exceeding the noise floor"** — NOT "behaviour identical", which this harness cannot establish.

That is queue item 1 (determinism) blocking a real verification decision for the first time, exactly as
it warned. Free data for item 2 while we are here: **scene E spread varies ~6% (33.1 / 33.5 / 35.1 m)
across three runs of identical code on one card.** Until that floor is fixed, any change of this size is
unfalsifiable on the GPU side.

## 2026-07-20 — the layout guard that could not fail, and the one that can (docs/47 Hazard 0)

**What.** `GpuParticle`/`GpuStepParams` moved out of `#[cfg(target_arch = "wasm32")] mod app` into a new
natively-compiled `gpu_layout` module, and BOTH Rust mirrors (engine + `tools/gpu-verify`) are now pinned
to `shaders/particle_step.wgsl` by **byte offset**. 223 tests, gpu-verify green, wasm clean.

**Why the move.** The mirror that actually SHIPS lived inside a module native `cargo check`/`cargo test`
do not compile at all — so the production layout was verified by nothing but a human reading two files
side by side. Nothing kept it there but its location: it is plain POD, and `bytemuck` is an ungated
dependency.

**The part worth recording: my first guard was fake, and it passed.** It compared the WGSL field list
against a HARDCODED array and never read the Rust struct. Pinning the shader to a literal proves
nothing — reorder two Rust fields and it stays green. I only found it by deliberately swapping
`vel`/`resting` to check the guard had teeth; it did not. **The same defect had already shipped in #29**
for `gpu-verify`. Both are now rewritten around `std::mem::offset_of!`, so the assertion is tied to the
real layout, and both were re-verified by swapping fields and watching them fail.

A layout guard that passes while the layout drifts is worse than no guard: it converts an unchecked risk
into a believed-checked one. Each file now also carries a `the_guard_detects_a_reordered_struct` test, so
the ability to fail is itself asserted rather than assumed.

**Verified.** Engine 223/223; gpu-verify 2/2; wasm clean; both guards confirmed failing on a swapped
field pair and passing again on revert. The WGSL parser splits on COMMAS, not lines — the shader declares
`_hp1 : f32, _hp2 : f32,` on one line and a line-based parser silently drops the second, in exactly the
padding region the struct will grow into.

**Open.** Both mirrors are now bound, which was the prerequisite. Growing the struct to 80 bytes for
per-particle radius (docs/47 item 3) is next, and the guards will now fail loudly if the three
declarations disagree.

## 2026-07-20 — binding a repr(C) mirror to the shader that actually reads the bytes (docs/47 Hazard 0)

**What.** `tools/gpu-verify` now has a test that parses `shaders/particle_step.wgsl` and asserts its
`#[repr(C)] Particle` matches the shader's `struct Particle` field-for-field, in order, plus a 64-byte
stride check. Partial: this binds ONE of the two Rust mirrors.

**Why.** Per-particle radius (docs/47 item 3) grows the particle struct from 64 to 80 bytes, and that
layout is declared three times with disjoint coverage — `GpuParticle` inside `#[cfg(wasm32)] mod app`
(native `cargo test` never compiles it), `gpu-verify`'s standalone replica (native only), and the WGSL
(the runtime authority, checked by neither). A change can pass the native suite AND gpu-verify and still
be wrong in the browser. `cargo check --target wasm32-unknown-unknown` does not close it: rustc never
sees the shader, so field ORDER drifts silently. **This has already fired** — `gpu-verify`'s own comment
records `drag_cd` arriving as 0.0 from a drifted mirror, drag quietly a no-op.

**A plan correction, found by reading the file rather than reasoning about it.** The version merged in
#28 said to delete gpu-verify's replica and import the engine's declaration. Its Cargo.toml says why that
is wrong: gpu-verify is deliberately NOT a workspace member, carrying its own `[workspace]` table so its
native Vulkan `wgpu` build cannot leak into the engine's WebGPU-only wasm build via cargo feature
unification. So two mirrors are PERMANENT — which is safe precisely because each is pinned to the same
authority. Pinned to one shader, they cannot drift from each other.

**Verified.** The test passes; and it was confirmed to have teeth by swapping `vel`/`resting` in the
mirror list, which fails it, then reverting. gpu-verify's suite green.

**Open — the plan is NOT finished.** Still to do: move `GpuParticle`/`GpuStepParams` out of
`#[cfg(wasm32)] mod app` into a natively-compiled module so the PRODUCTION mirror is under `cargo test`
at all, then give it the same shader cross-check. Only after both mirrors are bound should the struct
grow to 80 bytes. The engine-side mirror — the one that actually ships — remains unbound today.

## 2026-07-19 — the axle: a wheel that spins without the engine knowing what rotation is (docs/47 §2)

**What.** New `crate::axle` — the revolute joint docs/47 §3 specified, as a constraint rather than a
spring. `axle::resolve` per substep: (1) a velocity-decoupled position projection putting the hub back on
its anchor, (2) a COM-velocity match returned as an impulse, (3) an angular split preserving spin about
the axle axis exactly and refusing everything else. 5 tests; 211 total, wasm clean.

**Why a constraint.** `Bond` is a distance spring, and a penalty joint stiff enough to hold a wheel on is
stiff enough to launch it — the same mechanism as the terrain settling storm, which went away by moving
from a penalty to a constraint. The axle is the same shape as `granular::terrain_contact_resolve`.

**The piece §3 left implicit, and it carries the whole argument:** the wheel's angular velocity is
recovered from the particles' LINEAR momenta alone, `ω = I⁻¹L` over the cloud. That is the mass-weighted
least-squares rigid rotation, which is *why* the constraint is provably non-injecting — subtracting a
least-squares projection can only reduce the residual, never grow it. No orientation, angular velocity or
inertia tensor is stored anywhere; §3's claim that torque emerges from forces holds in code.

**Verified.**
- `a_freely_spinning_wheel_is_left_completely_alone` — the test an axle has to pass. A compliant wheel
  comes back bit-identical, and its spin is recovered as exactly the 37 rad/s it was given. A joint that
  bled spin here would look like bearing friction while being a numerical artifact, and would be
  indistinguishable from the DECLARED bearing-friction model docs/47 §4 owes a derivation for.
- `wobble_is_refused_the_axle_spin_survives_and_the_reaction_is_reported` — a 9 rad/s tumble about X is
  removed while 25 rad/s about Z survives untouched; the reaction opposes the wobble and is exactly zero
  about the free axis.
- `the_hub_is_pulled_back_to_its_anchor_injecting_no_energy` · `the_axle_never_increases_energy` (four
  perturbed cases: drift, tumble and pure deformation at once) · `a_force_couple_spins_the_wheel_and_the_
  axle_lets_it` — zero net force, torques adding, and the axle passes the drive torque through.

**What it deliberately does NOT do: rigidify the wheel.** Only the best-fit rigid rotation is touched, so
deformation passes through — the reason a tyre is rubber and can spread a contact patch.

**Open.** Nothing calls it: there is no chassis to bolt it to until docs/47 item 4. Reactions are
returned, not applied, so whoever wires it must hand the negatives to the chassis or the joint creates
momentum.

## 2026-07-19 — one answer to "where is the ground", so a patch can de-resolve (docs/47 step 1a)

**What.** `World::ground_top_voxel` is now the single authoritative ground query: it returns the voxel
top while a column is resolved and keeps returning the SAME top after that column is demoted to the T0
field. The GPU grain heightfield, the CPU bilinear contact surface and the rendered bulk cap all read it
(the cap via `displacement_at`). A `demoted: Vec<bool>` mask disambiguates "baked into the field" from
"excavated to nothing". +4 tests (203 total, wasm clean).

**Why.** docs/47 §5 called voxel→field demotion "not new machinery" because `demote_column_to_field` and
`column_is_bakeable` already exist, tested — with **zero non-test callers**. That was true of the
function and false of everything around it. The engine held **three different answers to one question**:
the GPU heightfield read raw `surface_top_voxel`, the bulk cap read raw `terrain_height` ignoring
`displacement` entirely, and probe contact read `bulk_height`. So simply calling demotion would have
dropped every grain resting on that column through the floor, and drawn the de-resolved crater as
untouched ground — the render disagreeing with the physics, the one direction this engine never allows.

**The useful discovery: demotion needs no sub-voxel heightfield.** The bake preserves the surface
exactly, and that surface is already voxel-quantised (`top − 0.5`), so the field hands back the
*identical integer top*. The GPU's `array<i32>` is untouched and demotion stays cleanly separate from
the deferred f32-surface refactor (docs/45's `SLOPE_QUANTUM_M` IOU). I had started down the opposite
path — continuously baking the field to track the voxels — and abandoned it on noticing it would snap
the *smooth* procedural field onto the voxel quantisation at startup, degrading the higher-fidelity
representation by up to 1 m. The field records DEFORMATION, not rounding.

**Verified.**
- `ground_top_survives_demotion_exactly` — over five columns, the top before demotion equals the top
  after, exactly, once the voxels are gone.
- `an_excavated_column_has_no_ground_but_a_demoted_one_does` — the ambiguity the mask exists for; a zero
  displacement would otherwise pop dug-out ground back to pristine relief.
- `putting_matter_back_re_resolves_a_demoted_column` — a de-resolving grain hands authority back to the
  voxels instead of the column answering from a stale bake.
- `the_bulk_cap_renders_a_de_resolved_crater` — field-blind cap vs field-aware cap over a baked 6 m pit;
  pins render/physics agreement, which would otherwise fail silently.
- 203/203 native, `cargo check --target wasm32-unknown-unknown` clean.

**Open, and stated plainly: NOTHING TRIGGERS DEMOTION YET.** This makes it safe, not active — step 1b is
the quiescence trigger. Two known obstacles for it: `patch_resolved` is a single bool for the whole 96 m
patch while demotion is per-column (they do not compose), and `bulk_height` still returns pure procedural
relief for a column dug but not demoted, so the field/voxel seam is consistent only because
`patch_resolved` gates which of the two is asked. docs/46 ledger row 6 narrowed, not closed.

## 2026-07-19 — terrain learned the half of Mohr–Coulomb the grains always had (docs/45)

**What.** Terrain slope stability now carries the **friction term**. `granular::face_stable` is the law —
a face stands if friction holds the slope (`drop ≤ μ·r + quantum`) OR cohesion holds the bank
(`run ≤ c/ρg`) — and `matter::materialize_steep_terrain` applies it per voxel against every neighbour out
to 8 cells, iterating to a fixpoint. `steep_drop = 3` is retired. +7 tests (199 total, wasm clean).

**Why.** `τ = c + σ·tan φ`, and terrain implemented only `c`. The φ half was hidden behind a constant that
tolerated a **72° face for every material** while gravel fails above 40°. With a grass skin on basalt that
never showed, because basalt's `h_crit ≈ 510 m` holds anything this world can build; a cohesionless
horizon has `h_crit = 0` and the missing term becomes load-bearing instantly. The datum was never
missing — `friction_coefficient` is in `data/materials.json` for every material and the grain side has
read it since docs/23. Ground and grain were answering the same question two ways, which is the thing
docs/46 exists to forbid.

**Three corrections the design doc did not have, each found by measuring:**

1. **The φ term alone does not converge; the removal TARGET was the real bug.** The old rule cut a failing
   face down to its lowest neighbour, which moves a cliff rather than relieving it — the column behind
   becomes the new face at nearly the same height. Measured on a cohesionless horizon: **106, 148, 214,
   285, 339, 387, 433, 468, 504, 542, 577, 622 grains per pass — monotonically increasing** over 12
   passes, 13-voxel face still standing. Cutting to the *stable* height instead grows a talus ramp that
   climbs one column per pass and stops at the plateau.
2. **Cohesion must judge the material's own bank, not the drop.** A 1 m grass veneer over basalt on a 2 m
   step is not a 2 m grass bank. Judging it against the full drop **shed 470 grains from a pristine world
   nothing had touched** — every hillside steeper than the skin is thin trips it. Cohesion now uses the
   contiguous same-material run; friction still uses the drop, because slope is a property of the surface.
3. **Faces fail from the base.** Stopping the walk at the first voxel that holds lets a self-supporting sod
   skin shield the failing 10 m bank beneath it. The lowest failing voxel is found first and everything
   above goes with it.

**Verified.**
- **Convergence** — a cohesionless horizon with a 7×7×10 pit reaches a fixpoint inside one call; a second
  call sheds **exactly 0**. Bounded by geometry (4,159 grains vs a 6,157 wedge bound derived from the ramp's
  reach), and terrain 25 cells out is byte-unchanged: the slide does not march inland.
- **End state is repose** — the settled slope is asserted against gravel's **DB μ**, not a literal:
  no 8 m baseline exceeds `μ·8 + quantum`. The test also asserts the terrain developed a real slope
  (> 1 m), so a flat world cannot pass it vacuously.
- **Rock cliffs stand** — basalt sheds no grains at any pit depth; the new steep grains were 100% grass
  before fix (2), 0 after.
- **Pristine terrain is a no-op** — 470 → **0** grains, solid count unchanged.
- **Burial (docs/45 §5)** — worst penetration against the bilinear collision surface **2.75 m → 0.50 m**,
  1.2% of grains penetrating at all (the regolith branch had worsened it to 3.75 m). Resolved as a side
  effect of correction 1: grains now come off the wedge above the ramp, not from a column cut to the floor.
- 199/199 native, `cargo check --target wasm32-unknown-unknown` clean.

**Two existing tests were passing without testing their own names, and are now honest.**
`a_granite_cliff_holds_while_the_dirt_above_it_slumps` asserted "no granite grains" against a world that
contains **no granite and no dirt** — it is a 1-voxel grass skin directly on basalt — and its "the weak
dirt slumps" was the grass skin moving for the buggy reason in (2). It now builds the layered cap it
describes. `materialize_steep_terrain_turns_cliffs_into_grains_conserving_mass` dug into basalt and
asserted the walls **must** materialize, which docs/45 §6 explicitly reverses ("a rock cliff still stands
… this must not flatten canyons"); its conservation checks moved to material that genuinely fails.

**Flagged, not hidden.** `SLOPE_QUANTUM_M = 1.0` is a **resolution IOU**: an integer heightfield cannot
express a slope between 0° and 45°, so enforcing repose at one cell with no allowance would force every
soil in the DB flat. Over `r` cells it dilutes to `1/r` — at the 8-cell baseline, ~3.6° above gravel's
true 40°. The sub-voxel surface retires it; a bigger baseline only buys O(r²) sandpaper.

**Measured against `regolith-horizon` itself (docs/45 §8), and it corrects this entry's own first claim.**
Stacking regolith's `world.rs` on this work and stabilising repeatedly gives **`[1466, 0, 0, 0, 0, 0]`** —
converges on the first call, zero on every later one, mass conserved. The unbounded slide is gone, and the
law discriminates by material exactly as it should: of 1,466 grains, **870 dirt, 580 grass, 16 gravel** —
dirt (φ=28.8°) fails where the cohesionless gravel beneath it (φ=40°) holds. **But regolith is still not
mergeable, for a different reason:** those grains come off *undisturbed* ground. A slope census finds
8-cell drops to **10 m (51°)** against gravel's 7.72 m allowance, while the profile lays a uniform 6 m
mantle over all of it. A uniform soil mantle is not physical on ground steeper than the soil's repose —
regolith's own comment says "thin on steep or glaciated ground"; the generator does not implement it.
**The blocker moved from "terrain cannot hold a slope" to "the generator places soil that cannot stand",**
and the fix is slope-tapered thickness in world generation, not here. Burial on that world is 2.00 m
(101/1,466), the same symptom.

**Open.** docs/45 §6's *emergent agreement* test (grain pile vs terrain slope reaching the same angle)
stays blocked on grain-side rolling resistance, per the doc. Nothing was tuned to make the halves agree.
On the live meteor scene this is near a no-op (76 → 0 grains) because that world is genuinely stable.

## 2026-07-19 — the architecture map had gone stale enough to assert that existing physics was absent

**What.** Refreshed `docs/32-architecture-map.md` and docs/33's status block against the code on `main`
(`1b4381e`), and corrected the same errors where `CLAUDE.md` repeated them. Neither doc had been
substantively edited since it was written on 2026-07-17 — `git log -- docs/32 docs/33` returns only the
repo-rename commit — while docs/34–48 and ~20 commits landed.

**Why.** CLAUDE.md instructs every session to read docs/32 before exploring, and docs/46's déjà-vu rule
rests on the docs being the reliable record. A stale map does not merely omit: it actively misdirects. The
worst entry told sessions to build a module that already existed, tested and verified.

**Corrections — each checked against the code, not inferred:**

- **§5 said "Condensed-matter EOS … CONFIRMED ABSENT."** False since docs/33 stage 1. `eos.rs` (373 lines,
  7 tests) implements the three-branch Tillotson `P(ρ,u)` (`:52`) with cited per-material constructors,
  verified vs Benz & Asphaug 1999. The real gap is **wiring**: `crate::eos::` has exactly two non-test
  consumers — `hydrostatic.rs:25` and `gpu_sph.rs:110` — so it is live in the space band only, while the
  terrain/voxel/granular path has no EOS at all and `GpuParticle.rho` (`lib.rs:1907`) is a placeholder ρ₀
  nothing computes. CLAUDE.md carried the same false claim and is fixed too.
- **§1 said two scene structs; there are three.** `Terra` (`lib.rs:5140`, docs/43 worlds-as-data) has its
  own 5-file submodule (910 lines). §1 also said `OrbitDemo` has "No GPU compute — all particle physics is
  CPU"; it owns a `gpu_sph::GpuSph` (`:2828`) driving `sph_step.wgsl`.
- **§2 omitted `eos.rs`, `hydrostatic.rs`, `gpu_sph.rs`, `accretion.rs` entirely** — 2,738 lines of the
  realignment's physics core, invisible to anyone navigating by this map. Added as §2b.
- **§7 listed 5 shaders (there are 9) and 1 tool (there are 6).** Added `sph_step.wgsl`, `bh_gravity.wgsl`,
  `sph_render.wgsl`, `globe.wgsl`; `sph-verify`, `gpu-bh-verify`, `bake-earth`, `impact-run`, `shot-server`.
- **§6's canonical trace had all 8 anchors stale** by 500–800 lines (`start_birth` 2897→3447, `step_substep`
  3243→3968, `step_block` 3430→4155, …). Re-verified, and documented that a second GPU-SPH path now
  coexists with the CPU aggregate one in the same scene.
- **§4:** four integrators → **six**; one WGSL mirror seam → **three**; added fork 7 (**three**
  terrain-contact implementations of one law — `granular::terrain_contact_resolve:310` with exactly one
  production caller, the WGSL mirror, and `matter.rs:872-887`'s normal-free snap+`CONTACT_DAMP` — plus a
  fourth voxel resolver in `body.rs:55`) and fork 8 (`AirField`, a container fork with zero consumers).
- **§8 deleted.** It restated CLAUDE.md's hard rules and had already drifted from them ("next is docs/34";
  a stale test count). One question, two answers — the doc-level form of what docs/46 forbids in physics.
  It now points at CLAUDE.md. Test count corrected there: ~145 → 204 written, 186 running by default (18
  `#[ignore]`d), and `gpu_sph.rs` has no in-crate tests at all.

**A pattern, beyond the individual fixes.** Three corrections are the shape docs/48 named — built, verified,
wired nowhere. `bh_gravity.wgsl` is verified standalone and `sph_step.wgsl` still runs direct O(N²) gravity.
`accretion::accrete()` has no non-test caller, so the disk can be *measured* for gravitationally bound
clumps and cannot *grow* one — which is exactly the "the disk never accretes a Moon" diagnosis docs/33
recorded, with the operator now built and still outside the loop. `world.rs`'s `demote_column_to_field` /
`column_is_bakeable` appear only in tests. Recorded in docs/33's new status section so the next session
inherits it.

**Verified.** Every count, anchor, and wiring claim above was read off `main` at `1b4381e` (module
inventory, `git grep` for callers, `git log` for the staleness claim). Docs-only change — no code touched,
so no CHANGELOG entry.

**NOT verified — stated rather than implied.** The per-symbol `:NNN` anchors inside §2/§3's existing bullets
were not exhaustively re-checked; only the ones quoted in this entry, plus the module line counts, were
confirmed. Given §6's anchors had all moved by ~500 lines, assume others in those sections have drifted too
and re-check before relying on one. The header now says so.

---

## 2026-07-19 — the probe gets real traction: terrain contact swapped onto the honest constraint

**What.** `Engine::collide_probe_with_terrain` now resolves against `granular::terrain_contact_resolve`
— the same non-injecting constraint the GPU grains use — instead of its own hand-rolled path. Deletes
two fudges in one move: the tangential `p.vel.x *= 0.5; p.vel.z *= 0.5` velocity multiply, and the
`DEAD: f64 = 0.15` dead-zone/half-correction hack. Adds `World::surface_bilinear_grad` returning
`(h, ∂h/∂x, ∂h/∂z)`; `surface_height_bilinear` now delegates to it, so there is exactly ONE bilinear
surface implementation on the CPU.

**Why.** `vel *= 0.5` is a raw velocity scale — blind to the normal load, to μ, and to the surface
itself. It cannot express traction, which is the whole content of driving: Coulomb friction bounded by
`μ·jn`, i.e. a harder-pressed contact grips more. The constraint already implements exactly that, is
unit-tested, and is hardware-verified (gpu-verify K/L/N) — it was simply only wired to grains. μ now
comes from the terrain material's own datum, so ice is slippery because ice's data says so.

The gradient is the load-bearing addition: without `∂h/∂x, ∂h/∂z` there is no surface normal, so a body
on a slope was treated as sitting on a flat floor and there was no normal impulse to bound friction with.

**Verified.**
- 178 tests pass (was 174). Four new in `world.rs`: the height is byte-identical through the refactor;
  the gradient matches a central difference of its own field; flat ground reports zero gradient; and
  **friction responds to μ** — ice retains more speed than basalt through the same contact, which the
  old multiplier could not express at any μ.
- New rig `web/rig/probe_traction.mjs` (engine telemetry, not pixels), run against BOTH this branch and
  a detached baseline worktree at `main` — because the claims below are comparative and would otherwise
  be assumptions:

  | | baseline (`main`) | this branch |
  |---|---|---|
  | settled altitude | 0.60 m | **0.50 m** = `PARTICLE_HALF` |
  | time to settle | ~3.5 s | **~30 s** |
  | probe integrity | 100% | **100%** |

  **integrity 100% on both** — the bonds never broke, so the bounded velocity-decoupled projection does
  not pump the lattice the way a hard snap would, and the dead zone is genuinely unnecessary rather than
  merely removed. That was the regression risk and it is clear.

**Also removes the probe's damping fudge.** `probe.damping = critically_damped(0.4)` — a ζ picked so the
ball "settles rigidly" — is replaced by ζ derived from iron's own restitution via the new
`granular::zeta_for_restitution` (factored out of `damping_for_restitution`, which now calls it, so bond
damping and contact damping cannot drift apart). The old 0.4 implied e ≈ 0.254; iron's data says e = 0.6
⇒ ζ ≈ 0.16, so the probe was modelled ~2.4× less bouncy than iron is.

**Measured across three variants** (rig `probe_traction.mjs`, settle = first time the trace stays within
0.05 m of its final value):

| variant | settles | final alt | first rebound |
|---|---|---|---|
| `main` — fudged friction, ζ=0.4 | **6.5 s** | 0.6 m | 0.7 m (bounce crushed flat) |
| honest friction, ζ=0.4 | **35.0 s** | 0.5 m | 4.5 m |
| honest friction + honest ζ | **35.5 s** | 0.4 m | 3.9 m |

Integrity **100% in all three** — the honest, lower damping did not destabilise the explicit integrator
(the documented danger was OVER-damping, docs/23), so the stability concern behind ζ=0.4 was unfounded.

**Three findings, two of which refuted what this entry first claimed.**

1. **The resting height shift is the SURFACE, not the dead zone.** An earlier draft said the old
   `DEAD = 0.15` let the probe rest "up to 0.15 m sunk". Measurement refutes it: baseline rests *higher*
   (0.60 m). The probe now reads `surface_bilinear_grad` — four columns, bilinear, the SAME field the GPU
   grains collide against — where it previously read a single `surface_top_voxel`. Probe and debris now
   agree on where the ground is; they did not before. That is the real improvement.
2. **The damping derivation costs ~no settle time**, and I predicted the opposite. 35.0 → 35.5 s is inside
   the sampling interval, and the first rebound is *lower* (4.5 → 3.9 m): a springier lattice puts impact
   energy into internal modes rather than one coherent bounce.
3. **The tangential fudge was doing essentially all the settling work** — 6.5 s → 35.0 s at unchanged ζ.
   `vel *= 0.5` per substep is a ~50%-per-step damper; it was not a small approximation to friction, it
   was the mechanism that made the scene settle at all. Removing it exposes the lattice's real ringing.
   **35 s is the honest settle time; 6.5 s never was one.** It was manufactured by a ~50%-per-substep
   velocity damper standing in for friction — self-flagged in its own comment as *"crude; emergent
   friction is future"*, i.e. known debt, not a design to preserve. So this is a behavioural CHANGE to a
   deployed scene, not a regression against a correct baseline: nothing that depended on 6.5 s was
   depending on physics. What is real is the consequence — anything assuming a quick settle
   (de-resolution timing, UX) now faces a ~35 s transient, and bounding it belongs to the docs/44
   demotion criterion (demote on *irrelevance* — contact stress below yield — not on silence), never to
   re-tuning a dial back until the number looks familiar.

**NOT verified — stated rather than implied.** Traction is demonstrated at the law level (unit test) and
the fudge is gone, but nothing in the scene *drives*, so in-scene grip is unexercised. A probe dropped
straight down produces no tangential load. That waits on a driven body.

**Flagged.** μ is taken from the surface material alone; no pair-combining rule between a body's
material and the ground's exists yet (the same gap `gpu_step_params` flags for mixed-material debris).
The first sampling window (12 s) read the still-ringing transient as a failure to settle — the landing
transient runs ~30 s, so `SAMPLES` is env-tunable on the rig.

---

## 2026-07-19 — iPhone 15 Pro Max: a latency/throughput CROSSOVER, and the same physics on a third device

**What.** Third entry in the cross-vendor matrix (iPhone 15 Pro Max, A17 Pro, Metal), run through
`/gpu-probe.html` on the LAN dev server.

**Correctness — three devices, two backends, one answer.** At N=60,000 all of Vulkan/RTX 2070,
Metal/M4 and Metal/A17 report `tot = 1.585e+7` and `vmax = 30.945`; at N=1 all report
`tot = 4.179e-8`. No energy injection anywhere. The four-separate-passes race mitigation holds on
every device tested.

**The finding — a latency/throughput crossover between N=1,000 and N=10,000.**

| N | 2070 (Vulkan) | M4 (Metal) | A17 Pro (Metal) | iPhone vs 2070 |
|---|---|---|---|---|
| 1 | 1.25 ms | 0.540 ms | 0.613 ms | **2.0× faster** |
| 1,000 | 1.83 ms | 0.833 ms | 1.113 ms | **1.6× faster** |
| 10,000 | 2.23 ms | 1.553 ms | 2.793 ms | 0.8× (slower) |
| 60,000 | 13.40 ms | 10.317 ms | 16.017 ms | 0.84× (slower) |

A phone BEATS a desktop discrete GPU below the knee and loses above it. The A17 Pro is the ideal probe
for this because it has Apple's latency advantage with much less throughput, so the two effects
separate. The ratios confirm the mechanism quantitatively — A17 Pro has a 6-core GPU vs the M4's 10
(a 1.67× core ratio):

- **N=60,000: M4/A17 = 1.55×** ≈ the core ratio ⇒ throughput-bound, core count predicts the gap.
- **N=1: M4/A17 = 1.14×** ≪ the core ratio ⇒ latency-bound, core count nearly irrelevant.

Same silicon family, same backend, two limiting regimes, crossover at the knee. This is the §7
saturation-knee argument (`gpu-perf`) showing up as hardware ranking that REVERSES with N — a single
benchmark point would have ranked these devices wrong in either direction depending on which N it
happened to pick.

**Product consequence — the phone's practical particle budget is well under `MAX_PARTICLES`.** At
N=60,000 physics alone costs 16.0 ms, a ~62 fps ceiling with essentially nothing left for rendering
inside a 16.67 ms frame. At 0.267 µs/particle, keeping physics to about half the frame budget implies
roughly **30,000 grains on an A17-class phone** (vs 60,000 viable on the M4). Not a bug — a real
device-tier limit to design scenes against.

**This raises the priority of the O(table) grid clear.** Its ~0.53 ms/frame is FIXED regardless of N,
so it is proportionally most expensive exactly where Apple hardware is otherwise strongest (small N),
and it eats a bigger share of a tighter phone frame budget. The epoch-tag fix is output-neutral and
now has a clear beneficiary.

**Unchanged limits:** Safari masks every `GPUAdapterInfo` field to the literal string `apple` on this
device too, so "A17 Pro" is the operator's knowledge of the hardware, not a probe measurement.
`max_buffer_size` is 1024 MiB, same as the iPad — the N=60,000 run completed without hitting it
(a prior concern that iOS Safari's tighter per-tab memory limits might kill the run did not
materialise).

---

## 2026-07-19 — FIRST NON-VULKAN RESULT: the engine's granular step runs correctly on Metal (iPad Pro)

**What.** Ran `/gpu-probe.html` on an iPad Pro (M4) over the LAN HTTPS dev server. First time any part of
this engine's GPU physics has been executed on a non-Vulkan backend, and the first entry in the
cross-vendor matrix.

**The correctness result — Metal and Vulkan agree to 4 significant figures.** `lib.rs` (~line 2118)
splits the four granular stages into four separate compute passes specifically because fusing them
"happened to work on desktop Vulkan (the 2070) but can RACE on other backends (e.g. Metal / the M4)".
That mitigation was written defensively and had **never been exercised on Metal**. It holds:

| N | Vulkan (2070) tot / v max | Metal (M4) tot / v max |
|---|---|---|
| 1 | 4.179e-8 / 0.000 | 4.179e-8 / 0.000 |
| 1,000 | 2.981e+3 / 6.012 | 2.875e+3 / 6.104 |
| 10,000 | 9.580e+5 / 31.019 | 9.546e+5 / 31.022 |
| 60,000 | 1.585e+7 / 30.945 | 1.585e+7 / 30.945 |

No energy injection at any N — a race would show as a rising total. Note the N=60,000 row is identical
to four significant figures in BOTH total energy and max speed, and the Vulkan side reproduced those
same figures across repeated runs. So this probe configuration appears **reproducible in a way
`gpu-verify`'s scene I is not** (bulk settling rather than marginal stability). That strengthens the
comparison, but does NOT retire the determinism work — a *small* Metal anomaly would still be
indistinguishable from drift.

**The performance result — the iPad beats the desktop RTX 2070 at every point.**

| N | browser 2070 | browser M4 | M4 advantage |
|---|---|---|---|
| 1 | 1.25 ms | 0.540 ms | 2.3× |
| 1,000 | 1.83 ms | 0.833 ms | 2.2× |
| 10,000 | 2.23 ms | 1.553 ms | 1.4× |
| 60,000 | 13.40 ms | 10.317 ms | 1.3× |

The advantage is LARGEST at small N and shrinks as N grows — the signature of much lower per-dispatch
latency (unified memory, no PCIe round trip), not raw throughput. Product-relevant consequence: at
`MAX_PARTICLES` = 60,000 the M4 sustains 10.3 ms/frame, a ~97 fps physics ceiling, so **the engine's
full particle budget is viable on an iPad**.

**Limits of what this proves (stated rather than glossed).**
- **The probe did not identify an "M4".** Safari masked every `GPUAdapterInfo` field to the literal
  string `apple` — vendor, architecture, device and description are all `apple`. It establishes Apple
  GPU ⇒ Metal (iPadOS WebGPU has no other backend) and `fallback: no` rules out a software adapter.
  The specific chip is Robin's knowledge of her hardware, not a probe measurement. Do not quote the
  probe as the source for "M4".
- **`max_buffer_size` is 1024 MiB on the iPad vs 4096 MiB on desktop.** Not binding here (the largest
  buffer at N=60,000 is the 8× render buffer at ~31 MB), but a 4× smaller ceiling to respect when
  scaling up.
- The page's "per-particle cost falls 3141×" line is dominated by the N=1 point, which is pure launch
  overhead. The real knee sits between N=1,000 and N=10,000.

---

## 2026-07-19 — a browser GPU probe, and the same wrong-GPU bug confirmed in the browser

**What.** `GpuProbe` (`crates/engine/src/lib.rs`, wasm-only) + `web/gpu-probe.html` /
`web/src/gpu-probe.ts` + `web/rig/gpu_probe.mjs`: a compute-only probe that runs the REAL
`particle_step.wgsl` through the REAL `GpuParticles` (no canvas, no surface, no reimplementation) and
reports (1) which adapter actually ran, (2) per-frame cost across N = 1 … 60,000, (3) whether total
energy stays bounded. Two-phase like `begin_readback`/`take_readback` — `start_run` submits, JS polls
`poll()` — because a browser cannot block on a buffer map. Also fixes two `scripts/dev-lan.sh` bugs
(below) and registers the page in `vite.config.ts` (an unregistered page works in `dev` and silently
vanishes from `build`).

**Why.** The engine ships to browser WebGPU across vendors, but nothing in `web/` ever touched
`navigator.gpu` beyond an existence check, and `Engine::create` (`lib.rs:321`) requests an adapter
with `HighPerformance` and never reports what it got. So a browser run was silent about the hardware
that produced it — the same ambiguity PR #11 fixed natively. Robin has an iPad Pro (M4); this is the
first step of a growing cross-vendor matrix (AMD / Apple / Arc).

**Verified (desktop Chromium over Vulkan, xvfb).** Probe reproduces the native baseline on the SAME
card, which is what validates the probe itself before it meets unfamiliar hardware:

| N | native 2070 (gpu-verify) | browser 2070 (probe) |
|---|---|---|
| 1 | 1.58 ms | 1.25 ms |
| 1,000 | 1.91 ms | 1.83 ms |
| 10,000 | 3.86 ms | 2.23 ms |
| 60,000 | 14.4 ms | 13.40 ms |

**Energy invariant holds on Vulkan** — fixed N = 10,000, increasing frames, total energy must never
rise: `1.83e6 → 1.31e6 → 1.99e5 → 1.37e5` over 60/120/240/480 frames, KE decaying to 37.8 and
`vmax` 0.65 (settled). This is the reference the M4 run will be compared against; a backend race
would show as rising energy.

**Two findings that change how browser results must be read.**

1. **wgpu's `AdapterInfo` is EMPTY in a browser.** Under `Backends::BROWSER_WEBGPU` wgpu delegates to
   the browser and cannot see the driver: `get_info()` returns no name, no driver, and
   `backend: BrowserWebGpu`. It can never tell you whether you are on Metal. The authoritative source
   is the browser's own `navigator.gpu` → `GPUAdapterInfo.vendor` / `.architecture`. The probe now
   reports BOTH and the rig prints the browser's.
2. **The browser picked the WRONG GPU too — and you cannot override it.** With
   `powerPreference: "high-performance"`, Chromium reported `vendor: nvidia, architecture: turing` —
   the RTX 2070, not the 5060 Ti. Corroborated independently by timing (13.4 ms at N=60k matches the
   2070's native 14.4 ms, not the 5060 Ti's 5.67 ms). Chromium's `--gpu-vendor-id` / `--gpu-device-id`
   flags did NOT move it. **WebGPU exposes no adapter enumeration at all** — `requestAdapter()`
   returns one adapter and the spec offers no way to choose — so unlike the native harness, which can
   now refuse to guess, in a browser the only available defence is to RECORD which GPU you got. That
   is precisely what this probe does, and why its provenance output is not optional decoration.

**Not achievable on this host (stated rather than quietly dropped):** reproducing the 2.5×
5060-vs-2070 gap *in the browser*. Chromium cannot be pointed at the second card, so the browser leg
is validated against the 2070 only.

**`scripts/dev-lan.sh` — two bugs fixed.** (1) The readiness probe grepped the served `/` for
`greenfield`, which appears nowhere under `web/` (it survives only as a wgpu device label in Rust and
never reaches the HTML), so the script never reused a running server and always exited 1 after a
perfectly healthy start; it now greps a `SENTINEL` that is actually in `index.html`. (2) `needs_build`
searched only `crates/` and `data/` for `*.rs|*.toml|*.json`, missing `shaders/**.wgsl` — but every
shader is `include_str!`'d into the wasm, so editing one changed the binary while the script reported
"✓ wasm up to date" and served the OLD shader. Silently stale results are the worst possible failure
for on-device verification, which is exactly what this script exists for.

**Known cosmetic gap:** every page 404s `/favicon.ico` (the repo ships no favicon). Pre-existing,
affects all pages equally, not introduced here.

---

## 2026-07-19 — gpu-verify was verifying on the wrong GPU (and is not run-to-run reproducible)

**What.** `tools/gpu-verify` selected its device with `request_adapter(PowerPreference::HighPerformance)`.
On a host with two *discrete* NVIDIA cards that preference cannot discriminate — it silently took whichever
Vulkan enumerated first. Replaced it with `pick_adapter()`: `GPU_VERIFY_ADAPTER` (case-insensitive substring
of the adapter name) selects explicitly; with exactly one non-CPU adapter present that one is used; with
several and no variable set it **panics rather than guessing**, listing what it found. The chosen adapter,
its device type, and the driver version now print on every run, so a log always records which silicon
produced it. `tools/gpu-verify/.cargo/config.toml` supplies the host default via cargo's `[env]`
(`force = false`, so a real env var still wins). CPU adapters (Mesa llvmpipe) are filtered out — they are
not verification targets.

**Why.** A verification harness that quietly changes hardware is worse than one that fails: every prior
"PASS" carried an unstated assumption about which GPU produced it. Capability-based auto-selection was
considered and rejected on evidence — both cards report *identical* `wgpu` limits (`max_buffer_size`,
workgroup dims), so there is nothing to choose on. Explicit-or-refuse is the only honest option.

**Verified.** All four paths exercised: default via cargo → `adapter: NVIDIA GeForce RTX 5060 Ti
(DiscreteGpu, 580.173.02)`; `GPU_VERIFY_ADAPTER=2070` → the 2070; no variable + two GPUs → panics with
`2 discrete GPUs present (…) — refusing to guess`; unmatched name → `matched no adapter; available: …`.
Full suite run on both cards: **same 25 PASS / 2 scene FAIL on each** (the pre-existing scene-D repose
deficiency and scene-J impact-energy failure — unchanged by this work, not addressed here).

**Recorded, not fixed — the harness is nondeterministic.** Comparing the two cards showed small numeric
drift, so the same card was run twice: it drifts *by the same magnitude against itself*
(`I energy-conservation: E 16303→-2684→-6490` vs `16303→-2670→-6480`; scene E spread 21.3 m vs 21.0 m).
So the cross-card deltas are **not** architectural divergence — both are the same underlying
nondeterminism, most likely order-dependent float accumulation in the GPU force/neighbour reduction.
This matters because scene I is the FUDGE DETECTOR: its margin is currently larger than its
reproducibility. Worth a determinism pass before any number from this harness is quoted as exact.

**Timing (informational, not a benchmark).** Full suite 65.7 s on the 5060 Ti vs 79.4 s on the 2070
(~17% faster). Single samples of a wall-clock that includes shader compilation and CPU-side setup —
this harness is not GPU-bound, so do not read it as a measure of the cards. See the next entry: that
17% is an artifact of the harness's scale and says nothing about the engine.

---

## 2026-07-19 — the 17% was the harness, not the hardware: gpu-verify runs 1–5 particles per scene

**What.** Chased why a 3-generation-newer GPU only won 17% on the suite. `GPU_VERIFY_STATS=1` (added
to `simulate`, stderr-only) dumps the workload shape. The harness's real distribution over 458
sim-calls: **219 calls at 1 particle, 205 at 5, 11 at 2** — i.e. ~95% of calls dispatch a SINGLE
workgroup with 63 of 64 lanes idle. Only one call reaches 13,456 particles. Meanwhile every substep
clears the whole `TABLE_SIZE` grid regardless of N. Totals for one suite run: **1,036,448 submits,
4,145,792 dispatches, 33.96 G threads in CLEAR vs 0.90 G in physics (37.5 : 1)**. At ~16 µs of
launch latency per dispatch that accounts for the runtime — the suite measures driver launch
overhead, not the shader.

**Why it matters.** The harness's scale is not the engine's, and the two batch differently:
gpu-verify creates an encoder and **submits per substep**, while `Engine::step_physics` records all
`DEBRIS_SUBSTEPS` into **one** encoder and submits once per frame. A perf conclusion drawn from this
harness does NOT transfer to the engine — which is exactly the error the 17% invited.

**Verified — at engine scale the new card is 2.5× faster.** Benchmarked the real
`shaders/particle_step.wgsl` at the engine's configuration (`GRID_TABLE_SIZE = 1<<18`, 16 substeps in
one encoder, one submit), 3 warmup + 20 timed frames, both cards:

| N | RTX 2070 | RTX 5060 Ti | speedup |
|---|---|---|---|
| 1 | 1.58 ms | 1.24 ms | 1.27× |
| 1,000 | 1.91 ms | 1.50 ms | 1.28× |
| 10,000 | 3.86 ms | 2.26 ms | 1.71× |
| 60,000 (`MAX_PARTICLES`) | 14.4 ms | 5.67 ms | **2.55×** |

Reproduced across reps (5060 Ti 5.50/5.67/5.72 ms; 2070 14.11/14.39/14.45 ms). The advantage grows
with N exactly as expected once the workload saturates the wider GPU. `nvidia-smi dmon` during a
suite run: `sm` 70–88%, **`mem` 0%**, `fb` < 100 MB — not bandwidth-bound, working set trivially
small. (`sm%` only means ≥1 warp resident; it is not saturation.)

**Recorded, not fixed — the grid clear is O(table), not O(N).** `cs_grid_clear` dispatches
`GRID_TABLE_SIZE = 262,144` threads (4,096 workgroups) every substep independent of particle count,
measured at **~0.53 ms per 16-substep frame on both cards** (flat in N). That is ~9% of frame time at
N=60,000 and ~30% at N=1. Candidate fixes: an epoch/generation tag per cell (compare a frame counter
on read, never clear), clearing only cells touched last frame, or sizing the table to live N —
`GRID_TABLE_SIZE` is currently 4.4× `MAX_PARTICLES` though the comment at lib.rs:125 says "≥ ~2×".
Not changed here: this branch is the adapter fix, and a grid-lifecycle change needs its own docs/NN
and re-verification.

**An invalid ablation, recorded so it is not repeated.** First attempt to price the clear simply
removed the pass and re-timed — it came out **6× SLOWER** (36.5 ms vs 5.67 ms at N=60,000). Removing
the clear does not remove work: `grid_count` then accumulates across substeps and `cs_forces` walks
saturated `bucket_k`-deep buckets. It measured a different, worse simulation. A negative measured
cost is the tell. Stage cost was taken from the clear running alone instead.

---

## 2026-07-19 — Worlds-as-data #2: the Space + Two Moons deorbit scenes are now DATA (docs/43)

**What.** The second worlds-as-data consumer, proving the schema generalizes from a static planet (Terra) to
**dynamic N-body scenes**. Extended the one `World` schema (`terra/world_def.rs`) with a `type:"system"` variant:
a `bodies[]` array (each `{name, role: star|planet|moon, mass_kg?/radius_m?/profile?, pos_m, vel_ms,
spin_period_s?, tint?}`) and orbit-camera fields (`yaw/pitch/zoom/focus`). New `OrbitDemo::load_world(json)`
(mirrors `Terra::load_world`) replaces the built-in Sun/Earth/Moon constants with the declared initial
conditions, spin, composition-derived tints, time scale, and frame-of-reference focus. New world files
`web/public/worlds/{one-moon,two-moons}/world.json`; `web/src/orbit.ts` now reads `<body data-world="…">`,
fetches the JSON, derives the moon count, and calls `create` + `load_world`. **Birth of the Moon** (GPU-SPH
impact) stays on the code path for now. The **deorbit stays a pure user control** (`brake_moon` ×½ / `drop_moon`
×0 of the moon's Earth-relative velocity) — the crash emerges from the N-body integrator + swept contact, no
scripted outcome.

**Why.** Terra was built as the reference worlds-as-data scene; the strategic payoff is a SECOND, structurally
different scene on the same contract — it confirms the schema (bodies + orbital ICs + events-as-controls)
generalizes, and turns "add/alter a scene" into editing data, not scene code (docs/43, the recorded near-term
TODO). `planet` is now `Option` on `World` (a system world has no single planet); `Terra::load_world` errors
cleanly if its `planet` section is missing.

**Verified (rig `worlds_space`, xvfb).** Space loads from `one-moon/world.json` — HUD reads the declared data
exactly: Earth–Moon 384,768 km (=MOON_DIST), v 1.02 km/s (=MOON_SPEED), Earth day 23.9 h (=sidereal spin), frame
Earth (=camera.focus), time ×118,000 (=time.scale). **Deorbit works through the data path:** `drop_moon` → the
moon falls 384,768 → 8,108 km and **impacts, spawning 1,536 debris particles**. Two Moons loads
`two-moons/world.json` — "4 bodies, 2 moon(s)". Render path is unchanged, so visuals match the pre-migration
scenes. Full fast suite **174/174 green** (+1 system-world parse test). TS typechecks.

---

## 2026-07-19 — FIX: the Terra "growing black void" on descent (Robin caught it) — globe back-face culling

**What.** Robin: flying in toward Earth, a black circle appeared at nadir around ~250 km altitude and grew to fill
the screen as he descended — "a void, I can see nothing through it." Root cause: the displaced globe was drawn
with **back-face culling**, and the fly camera — sitting just above the surface looking *down* — had its near
(front-facing) globe triangles culled, leaving the clear colour (the void); the limb (grazing triangles) still
rendered, so it read as a growing disc. The Phase-3 orbital camera looked at the planet *centre* from far away and
happened not to trip it, so it lay hidden until the fly camera shipped. Fix: **no culling for the globe/cap
pipeline** — the globe is convex, so the depth buffer alone gives correct occlusion; drawing both sides is robust
regardless of winding and costs only a few extra fragments. Also tightened the camera's near/far (dropped the
`far = near×1000` inflation; `near` is now a large fraction of the altitude at height, tiny near the ground) so the
globe's far hemisphere stays cleanly depth-occluded now that culling is off, and depth precision is far better.

**Why it was invisible in the rig at first.** The headless software GPU (ANGLE/llvmpipe) tolerated the original
setup; the bug showed on Robin's real GPU. Diagnosing it end-to-end (clip vs depth vs cull) in the rig — depth
`Always` still voided, `cull_mode: None` filled it — pinned it to culling, and reproduced/fixed it in software.

**Verified (rig `terra_depth`, xvfb).** Over the SUB-SOLAR point (fully day-lit nadir, so a void can't hide as
night side): orbit 6000 km, 500/259/250/100/45 km, and 1.5 km all render the **full lit surface — no void**, with
correct occlusion (near hemisphere only, no back-face bleed-through). `terra_globe`/`terra_fly` regression rigs
clean (full Earth; W moves north, orbital drag orbits, ground drag free-looks). 173/173 fast tests green.

---

## 2026-07-19 — Terra Phase 6: data-driven controls + HUD polish (the worlds-as-data controls contract)

**What.** The Terra scene's key bindings now come from the WORLD FILE, not code: `world.controls.keys` maps a
`code` → an `action` (`forward`/`back`/`left`/`right`/`up`/`down`), and `web/terra.ts` builds the input handler
from that map — the docs/43 worlds-as-data controls contract, closing the loop (the JSON populates the scene AND
its controls). Earth's world declares WASD move + R/F climb/descend; changing the bindings needs no code change.
The controls hint in the HUD is derived from the actual bindings, so it can't drift. HUD polished to show
`world · altitude · lat/lon · biome · fps` — new `Terra::ground_biome()` reads the surface type under the camera
(the land-cover biome material id, or "ocean"). fps is smoothed in the host.

**Verified (rig `terra_controls`, xvfb).** From the world bindings: **KeyR climbs, KeyF descends, KeyD moves east**
(lon increases); biome readback is "ocean" over the mid-Pacific and "sand" over the Sahara; the HUD line renders
`Earth · alt 1.5 km · lat 28.00° lon 84.00° · sand · 28 fps` + `WASD fly · R/F alt · wheel zoom · drag look`.
TypeScript typechecks clean; full fast suite **173/173 green**.

**Deferred (noted).** Optional planet rotation from `time{}` — parked: it conflicts with the lat/lon fly-camera
model (rotating the planet vs. the camera's surface coordinates) and Earth's world declares `rotation: false`;
revisit alongside the multi-epoch / pre-baked-until-collision work (task: worlds-as-data). This completes the
docs/43 terrain rework Phases 1–6: a navigable, data-defined Earth you fly from orbit to the ground.

---

## 2026-07-19 — Terra Phase 5: the fine ground cap (real-ratio terrain, true horizon, camera-relative)

**What.** New pure module `terra/ground_cap.rs` — a high-resolution local patch of the surface rebuilt under the
camera each frame (`fill_ground_cap`, 192² grid, denser toward the centre), sampling the SAME surface as the globe
(real elevation, biome albedo) and curving to a true horizon. It is emitted CAMERA-RELATIVE (`surface − eye` in
display units, in f64 then cast to f32), so ground detail survives the radius-1 globe — the precision fix the plan
called for. `FlyCamera::view` now returns both the absolute view·projection (globe) and a camera-relative one
(eye-at-origin, for the cap) plus the tangent frame + horizon distance. `Terra` builds the cap into a persistent
writable vertex buffer and cross-fades it over the globe (alpha-blended, `tint.a`) as altitude drops (`cap_fade`:
0 above 40 km → 1 below 15 km). The cap covers ~1.3× the horizon angle so its far edge sits below the horizon (no
visible boundary), lifted a few metres so the fine cap sits in front of the coarse globe.

**Exaggeration unified + made a declared dial.** The globe, cap, and camera floor now share one relief factor,
read from `surface.relief_exaggeration` (default 1.0 = true scale) — an honest visualization dial, not a physics
fudge. Set Earth's to **1.0**: real-ratio relief. This retires the ×30 hack that made ground flight impossible
(Phase 4's buried-black), at the cost of a flatter — but photorealistic — orbital globe. The camera floor
neighbourhood tightened to ±0.2° (~22 km) now that terrain is real-scale.

**Verified (rig `terra_ground`, xvfb).** A full orbit→ground descent over the Himalaya + a coastline: orbital =
a realistic smooth Earth (continents, biomes, terminator, limb); 35 km = the curved limb with terrain fading in
cleanly (no seam / z-fight ghosting); 6 km / 1.5 km / 300 m = a real ground-level horizon — tan foreground, green
foothills, snow peaks at the true horizon, black sky — **no burying**; the coast shows land meeting a blue ocean
wedge. Full fast suite **173/173 green** (+2 ground_cap tests: counts/index bounds; centre vertex sits directly
below the eye at the camera height).

**Honest limits (the plan's noted follow-ons).** Terrain is smooth — detail is capped by the 2048×1024 ETOPO
raster (~20 km/texel); no sub-raster fbm micro-detail yet. The cap is a single tangent patch, not yet a
screen-space-error quadtree with geomorphing + edge skirts. Relief is real-ratio (dial = 1.0); a normal-only
exaggeration could add orbital relief pop without breaking ground.

---

## 2026-07-19 — Terra Phase 4: the continuous fly camera (orbit ⇄ ground), physics-floored on terrain

**What.** New pure module `terra/fly_camera.rs` — ONE camera that blends orbit⇄ground by altitude (no mode
switch): high up it looks down at the planet and a drag orbits; near the ground it looks along the horizon and a
drag turns the view; a smoothstep on altitude (`GROUND_ALT`=3 km … `ORBIT_ALT`=400 km) cross-fades the forward and
up vectors between the two. State is `{lat, lon, alt_m, yaw, pitch}` in f64; the whole view·projection is built in
f64 (`DMat4`, cast to f32 only at the end) so ground framing survives the radius-1 globe. Near/far planes scale
with altitude (near ∝ altitude-above-ground; far just past the horizon). New `Terra` wasm API replacing the orbit
stub: `set_fly` · `move_tangent` (WASD, step ∝ altitude) · `zoom_alt` (wheel) · `drag_look` · readbacks
`altitude_m/latitude/longitude`; seeded from the world file's `camera{}`. `web/terra.ts` rewritten to drive it
(held-key WASD, wheel zoom, pointer-drag look) with a live lat/lon/alt HUD.

**Physics floor (Robin's constraint: the camera must never pass through solid).** `alt_m` is height above the
LOCAL terrain — `eye = up·(r_disp + ground_disp(lat,lon) + alt_m·ds)` — and `ground_disp` is the MAX terrain
height over a ±0.5° (~55 km) neighbourhood. So the eye always clears the terrain *envelope*, never buries inside a
neighbouring peak, and is **forced upward as it approaches rising terrain** (terrain-following with ~55 km
look-ahead). Recorded the standing rule + the follow-ups to memory: tighten to a per-triangle collision in Phase 5,
and — for caves/arches — move collision from a heightfield floor to a VOLUMETRIC "is this point in solid matter?"
test against the material field (docs/39/42), since a heightfield can't represent voids or overhangs.

**Verified (rig `terra_fly`, xvfb).** Functional readbacks: **W moves north** (Δlat > 0), **orbital drag orbits**
(Δlon ≈ 50°), **ground drag does NOT move position** (Δ ≈ 0 — free-look, the altitude blend working). Visual
orbit→ground sequence: clean globe at 8000 km → curved horizon with snow peaks + green foothills + tan plains at
80 km → a mountainous ground-approach horizon at ~1.5 km that is **no longer buried/black** (the terrain-envelope
floor fixed a first cut where the ×30-exaggerated coarse mesh swallowed the eye). Full fast suite **171/171 green**
(+5 fly_camera tests: tangent-frame orthonormality, blend monotonicity, orbital-vs-horizon look, zoom/move clamps).

**Honest limit.** True sub-km ground horizon detail is coarse here (39 km mesh triangles, ×30 relief) — the
real-ratio fine ground cap is Phase 5, exactly as the plan sequences it. Phase 4's deliverable is the camera
system, and it flies orbit→ground continuously.

---

## 2026-07-19 — Terra Phase 3: the displaced cube-sphere globe (a real blue-marble from world.json + rasters)

**What.** The `Terra` scene (docs/43, worlds-as-data) now renders a smooth **displaced cube-sphere globe** instead
of the Phase-2 grain shell. New pure module `terra/globe_mesh.rs` (`build_globe(res, r_disp, sample)`): 6 cube
faces, each a res×res grid projected to the sphere, every vertex displaced radially by the sampled surface offset
and coloured by its biome albedo; normals come from central differences of the *displaced* grid so relief reads as
shaded terrain. `Terra::build_surface_mesh` drives it from the real rasters — land cells lifted by ETOPO elevation
(×30 exaggeration so relief reads on a radius-1 globe) and coloured by the land-cover biome material; **ocean cells
sit flat at sea level with the water material**, integrated into the same mesh (no separate ocean shell, so no
coast z-fighting). New `shaders/globe.wgsl` + `build_globe_pipeline`: per-vertex biome colour × tint, `SUN_GAIN=22`
Reinhard day side (black night side, emergent terminator), plus a cheap view-dependent blue Fresnel **atmospheric
limb**. Built once in `load_world` (256² per face → 780,300 triangles); the grain shell stays as the fallback until
a world's rasters load.

**Why.** Phase 3 of the terrain rework (the plan): retire the grain shell for the Earth scene and deliver the
Google-Earth look. The grain shell proved the data path (Phases 1–2); a displaced mesh is the render surface the
fly camera (Phase 4) and ground LOD (Phase 5) build on. Ethos-consistent for v1: the surface is un-particalized
bulk, and the engine already renders un-materialized bulk as a smooth object — grains return where a region is
*resolved* (the JIT-particalize seam, docs/39/42).

**Verified.** `globe_mesh` unit tests (counts + index bounds; undisplaced = a unit sphere with outward normals;
displacement pushes vertices out by the offset) + full fast suite **166/166 green**. Rig `terra_globe` + rotated
angles (`xvfb-run`): an unmistakable Earth — Africa/Mediterranean/Arabia, the snow-capped Himalaya and Andes with
raised relief, the tan Sahara, a green temperate belt, Antarctica, a blue day-side ocean darkening through the
terminator, and the atmospheric limb — all from `world.json` + the baked Natural Earth / ETOPO / land-cover
rasters. Winding correct (convex front faces, back-culled). Land fraction 0.335.

---

## 2026-07-18 — FIX: the accreted Moon was escaping (Robin caught it) — near-breakup spin + inside-Roche mislabel

**What.** Robin watched the browser Moon accrete, compress, then leave on a near-straight outward trajectory —
and switching to Geologic found nothing (`disk_moonlets` empty → hand-off no-ops). Confirmed by tracing the
largest clump's orbit (`gpu_moon_track_json`, a new diagnostic): the clump accreted to ~0.23 M☾ on a tight bound
orbit (a≈11,800 km), then over ~10 s its semi-major axis blew out 11,800 → 27,800 km and it receded and unbound.
It formed at **~1 remnant-radius, INSIDE the Roche limit**, moving ~6.3 km/s (circular ~4.9, escape ~7.0) — i.e.
launched near-radially at near-escape speed, exactly Robin's "straight line, no slowing."

**Two causes, both fixed.** (1) The proto-Earth spin was **7e-4 rad/s — near rotational breakup**, flinging the
near-surface disk out at ~escape speed. Eased to **4e-4** (the cross-check's stable value, ~4.4 h day). (2)
`moonlet_bodies` / the tracker counted ANY bound clump as the Moon — including inside-Roche ones, which are tidal
DEBRIS (they form skimming the surface and escape), not moons. Now only **bound + outside-Roche** clumps
(`Clump::accretes()`) are the Moon; inside-Roche material renders as ejecta.

**Verified (rig `moon_track3`).** The real (outside-Roche) Moon now accretes to ~0.5 M☾ while its orbit
CIRCULARIZES (a: 79,000 → 22,000 km) and then **holds a stable bound orbit** — dist ≈ 29,500 km, v ≈ 1.6 km/s,
a ≈ 22,600 km, bound, steady over t=200–236 s. It orbits and stays. (The first-generation inside-Roche disk still
partly escapes — physical for this energetic sub-scale impact at browser fidelity — but it's ejecta, not the
Moon.) Full suite green; redeployed.

**Note to self:** don't explain away a direct observation with aggregate stats — track the actual thing observed.

---

## 2026-07-18 — HOTFIX: adaptive GPU-load control — the sim was freezing the tab/OS (docs/42)

**What.** The deployed GPU impact encoded a FIXED 100 KDK substeps (and a 300-step relax chunk) per frame — ~100
direct-sum O(N²) dispatches in one command buffer — so the GPU was monopolized and `present()` blocked for a long
time each frame, freezing the browser tab and starving the OS GPU scheduler. Replaced the fixed counts with an
ADAPTIVE per-frame substep budget: `sph_substeps` grows by one when there's frame-time headroom and shrinks
multiplicatively (down to 1) when a frame runs long, keyed off the wall-clock `real_dt`. The relax chunk rides the
same budget. Self-scales to the device — weak GPU → fewer substeps, strong GPU → more, frame time stays bounded.

**Why.** The 100-substep count was left over from the parity/diagnostic work; it must never ship. A sim can't be
allowed to break the device or the interface — it has to live inside a frame budget.

**Verified (rig `frame_check`, xvfb).** Frame time bounded at **~33 ms (30 fps), worst ~50 ms** (was effectively a
multi-second stall). The controller ramps 18→30 fps as it tunes. Full suite green (163 passed / 18 skipped).
Redeployed to integrity.bothead.net.

---

## 2026-07-18 — The "pretty render" layer over the GPU impact + browser parity → DEPLOYED (docs/42)

**What.** Built the render-side of the JIT primitive (Robin's vision): the real GPU SPH giant impact underneath, a
faithful "pretty" render over the top, a **slider** cross-fading them. And brought the browser physics up to parity
so the pretty layer has a real disk beneath it. The GPU impact is now the DEFAULT birth scene (the old CPU-Aggregate
impact retired); Earth/Luna frame buttons use 👁 (not 📷). **Deployed live to integrity.bothead.net.**

**Why.** Decouple physics-fidelity from visual-fidelity: the in-browser SPH is N-limited/fixed-dt, so instead of
forcing raw particles to look photoreal, the pretty layer carries the look while the particles need only be
physically right. The converged numbers stay the offline `tools/impact-run` (docs/41).

**Verified (rig: `pretty_slider` / `parity_check`; energy conserved ~0.05 %; full suite 163 passed / 18 skipped).**
- **Pretty render, 4 phases** (`OrbitDemo::render`, `sph_render.wgsl`): (1) `render_blend` slider + a pretty Earth
  shell sized to the sub-scale SPH body (scale reconcile DISPLAY_SCALE↔SPH_VIS_SCALE), size-cross-fading the
  particles; (2) a crater from the GPU field (first Theia contact freezes the impact dir; magma-ocean interior glows
  through an opaque crust; persists = bake-back); (3) ejecta motes (matter beyond the remnant glows) + a boosted
  shocked-vapor atmosphere; (4) self-bound disk clumps (`gpu_sph::moonlet_bodies`) → warm rock spheres.
- **Browser parity:** the impact was DISPERSING (Theia hit-and-run, 0 % Earth). Fixes: `HydroBody::new_lod` (coarse
  iron core + FINE basalt mantle — the mantle sheds a disk) + a **scheduled shock-dt** (WebGPU forbids the adaptive
  read-back, so the dt is stepped by sim time — small through the ~1.5 h shock, then 5× for the aftermath). → the
  disk now reaches **~27 % Earth with a bound ~0.03 M☾ moonlet** (was 0 %). Weaker than the offline converged run
  (spin → ~58 %) and ~2 fps at N≈2800 (direct-sum O(N²)) — both the N wall, not correctness.
- **Deployed:** `bash scripts/test.sh` green → `./scripts/deploy.sh` (release wasm + vite → /var/www/integrity via
  nginx :8080 / Cloudflare tunnel). Verified live: `birth.html` HTTP 200 locally and at https://integrity.bothead.net.

---

## 2026-07-18 — The SPIN IOU: a spinning proto-Earth sustains the disk → ~58% (docs/41); browser shock-dt fix

**What.** Closed the last docs/40/41 IOU — the disk re-accretes because a *non-spinning* impact leaves it
marginally bound. Added a pre-impact SPIN dial (proto-Earth rotation about the orbit normal) + a grazing-b dial +
intra-run epoch checkpoints to `tools/impact-run` (`spin`/`spineq` modes), and a rotating-frame centrifugal term
to `cs_relax` (a new `omega` Params field, 0 for every existing caller). Also carried the spin into the browser
`gpu_sph` path and found/fixed why the in-browser impact was dispersing.

**Why.** #3 converged the *non-spinning* branch (~25–32% Earth, re-accreting). Angular momentum is the missing
knob: a spinning target flings its own mantle into a rotationally-supported disk.

**Verified.**
- **Spin sustains the disk** (N=2400, K=5, to 18 h): baseline ω=0 DECAYS 0.56→0.09 M☾; ω=7e-4 PLATEAUS at ~0.6 M☾
  with Earth-fraction climbing to and holding **~58% ± 2%** (Moon 8/8) — the canonical value the no-spin case
  never reached. Grazing b=1.4·R_e is a hit-and-run (Theia escapes). L_z conserved to full precision through the
  impact; energy 0.2 %.
- **Not a startup artifact** (cross-check): ω=7e-4 is near breakup, so the check ran at a stable ω=4e-4 — a
  rotating-frame OBLATE equilibrium (flattening 0.149 ∝ ω², bounded) gives the same sustained disk as the
  startup spin (equilibrium 0.43 M☾/39% vs startup 0.32/43% at 18 h, both Moon 4/4).
- **Browser GPU impact fix** (rig, `birth_gpu`/energy trace): it was DISPERSING (Theia hit-and-run, 0 % Earth) —
  a pre-existing regression, NOT the spin (reproduced at spin=0). Cause: the fixed-dt browser path (WebGPU forbids
  the adaptive read-back) under-resolved the shock, so Theia interpenetrated Earth. A 5× smaller dt (paired with
  more substeps to hold playback) restores the shock and Earth begins to shed again (0 % → ~30 %). The
  spin/assembly ports the offline IC; full parity (LOD seeding, a *scheduled* shock-dt) is the render-layer
  follow-up. Energy conserved ~0.05 % throughout.

Offline `sph_step.wgsl` physics unchanged except the relax-only `omega` centrifugal (0 for all non-spin callers).

---

## 2026-07-18 — #3: the disk Earth-fraction converged by ensemble → ~32% (a minority, not 58%) (docs/40→41)

**What.** Built the variable-resolution ENSEMBLE in `tools/impact-run` (docs/40 #3) and converged the giant-impact
disk Earth-fraction. New: `build_lod` (coarse iron core @8×m_fine + fine basalt mantle, all SPH-EOS on the
unchanged `sph_step.wgsl` — no new kernel); an ORDER-INDEPENDENT disk measurement (`sum_oi` = sort+Kahan, re-measures
bit-identical); an ensemble mode (K perturbed-IC runs via a splitmix64 index-hash jitter, mean±stdev); and a
**physical-time epoch** stop (`ensemble <n> <t_hours> <K>`) replacing the fixed step count.

**Why.** The fraction is chaos-scatter-dominated (docs/28 28–50%, #1 25↔63%), so no single run is a number — an
ensemble mean is required. Two things had to be right first: (1) **AV-free relaxation** — the tool's `Gpu::relax`
ran with Monaghan AV on, which DISPERSED the impact (0% Earth, remnant puffed to R≈9500 km); zeroing AV during the
damped settle (restoring it for the shock) is the docs/35 finding the standalone crate never had, and it turned 0%
into a real Earth-bearing disk. (2) **Fixed epoch, not fixed steps** — the disk RE-ACCRETES (mass & fraction decay
with time), and a fixed step count integrates less physical time at higher N (finer Courant dt), confounding the
N-comparison.

**Verified (RTX 2070, native Vulkan; energy conserved 0.3–0.6% throughout).**
- Order-independent reduction: the same snapshot re-measures bit-identical (asserted in the single-run path).
- Re-accretion (fixed N=2400): 25%±5% / 0.19 M☾ @11 h → 12%±14% / 0.04 M☾ @23 h — the fraction is epoch-dependent.
- Convergence at a FIXED ~8 h epoch (K=8): N=1200 **20.4%±7.2%** (under-resolved) → N=2400 **31.8%±2.7%** → N=4800
  **32.2%±3.0%** — 2400 & 4800 statistically identical ⇒ **PLATEAU at ~32%±3%**. A disk MINORITY, decisively not the
  all-particle 58% (which was the high tail of the low-N/early-epoch scatter). A bound Moon-mass clump accretes in
  **8/8** runs at every N at the early epoch (largest ~0.07–0.26 M☾, sub-lunar at this sub-scale).
- Closes #1's number (its 63% was a scatter sample) and #2's resolved Moon. Only #4 (terrain) remains. Nothing
  deployed; `sph_step.wgsl` unchanged; engine crate untouched (change is confined to the standalone tool).

---

## 2026-07-18 — Frame-cost breakdown + hardware analysis → DECISION: defer GPU Barnes–Hut (option B, docs/37)

**What.** Followed the docs/37 GPU-BH finding with the measurement it was missing — a per-pass frame breakdown
(`tools/impact-run bench`, `cargo run --release -- bench`) across N=2k…256k, so the A-vs-B call is quantitative.
Timed each GPU pass of a force eval (`cs_density` is pure O(N) grid; `cs_forces` fuses O(N²) gravity + O(N)
pressure) and calibrated a real-fps model against the two observed browser points.

**Verified / measured (RTX 2070).** force_eval 2.2 ms @2k → 4.6 @8k → 16 @32k → 196 @128k → 700 @256k.
Physics-only fps (16 evals/frame): 28 @2k, 13 @8k, 3.9 @32k, 0.3 @128k. real-fps = ~0.3× physics (render + the
per-frame HUD read-back) — lands on the observed 2.8k→11 fps and 8.2k→4 fps. **Corrections to the earlier
inference:** gravity is ~35 % of the frame at 8k rising to ~50 % by 32k (not the ~25 % I'd guessed), so it IS
about half the physics cost — but the SPH grid+pressure is the co-equal other half, and the grid ALSO goes
super-linear past 64k (fixed `TABLE_SIZE=65536` saturates). So even free gravity ~doubles fps at most, and BH
still doesn't win below 128k. Interactive ceiling on the 2070 ≈ 12–15k; quadrupling the N=2.8k button → ~11k
lands ~3–4 fps.

**Hardware caveat (Robin's point — recorded for the revisit).** The 2070 is the *worst* case: (1) **unified
memory** (M4/A18/Snapdragon) makes a CPU-`bhtree.rs` + GPU-SPH realtime hybrid viable with zero new GPU code
(the CPU↔GPU copy is free; on our discrete PCIe-3 card it isn't → offline-only); (2) the BH crossover likely
drops to ~30–60k on cache-rich / lower-FLOPS GPUs (unmeasured). Cheaper levers for more particles NOW (no GPU
sort): fewer KDK substeps, grow `TABLE_SIZE` with N, lighter HUD read-back.

**DECISION (Robin, 2026-07-18): option B — defer.** Keep direct O(N²) gravity everywhere; do NOT wire BH or
build the GPU radix sort. Direct-sum is correct for every N we target; the sort is the most expensive remaining
kernel with no near-term payoff. The verified BH crate is banked + re-verifiable. **docs/37 now carries the full
write-up: frame table, hardware analysis, revisit triggers (high-N campaign OR Apple/mobile target), and a
resume plan (build the GPU sort as a *reusable* primitive — it also unblocks GPU accretion + grid reorder).**
`impact-run bench` mode committed. On branch `gpu-barnes-hut-verify` off `orbit-diagnostic`; nothing wired or
deployed.

---

## 2026-07-17 — GPU Barnes–Hut built + verified; direct-sum wins below N≈128k → do NOT wire it in-browser (docs/37)

**What.** Built the full GPU Barnes–Hut (LBVH) self-gravity solver spec'd in docs/36 — a standalone native-
Vulkan crate `tools/gpu-bh-verify` + `shaders/bh_gravity.wgsl` with the whole pipeline as WGSL compute kernels
(adaptive bbox via float-radix atomicMin/Max → 30-bit Morton → [interim CPU sort] → Karras binary-radix tree →
atomic-free bottom-up COM → θ-traversal), each **verified against an independent CPU reference before the next
was trusted**.

**Why the design choices.** Opening criterion is the robust Salmon–Warren/Barnes MAC — AABB **diagonal** as the
node size + centre↔COM offset δ — because a plain `maxside/dist<θ` on a *tight* box (the tight box is mandatory
for resolution, docs/36) under-opens and left a 28 % worst-case particle; diagonal+δ keeps the tight box AND
caps the error. Traversal runs in Morton order over a permuted `sbodies[]` so adjacent threads walk coherent
paths with coalesced reads. Leaf bucketing parameterized (`bucket_k`).

**Verified.** `cargo run --release` (RTX 2070) prints PASS for every stage: bbox **exact** (lossless u32
encode), Morton **bit-exact** (coincident→equal), Karras tree structural (every leaf reached exactly once,
parent/child consistent), COM root mass 1.0e-8 / COM 8.2e-8 (**the atomic children-ready climb is coherent on
this hardware**), θ-traversal RMS **0.70 %** at θ=0.5 and **1.8e-6 as θ→0** (recovers the exact direct sum —
the strong structural proof). The GPU direct-sum baseline itself matches CPU f64 to 2.4e-6.

**The finding (disconfirms the docs/36 premise — no-fudge).** Per-eval GPU wall time, θ=0.5: BH overtakes GPU
direct-sum only at **N≈128 000** (2.15×); below it direct-sum wins (N=8k: 0.89×, N=32k: 0.86×). Asymptotics are
textbook — direct → O(N²) (p≈1.84), BH → O(N log N) (p≈1.0) — but the *crossover* is 128k. **Leaf bucketing
(K=8/16/32) does not lower it** (buckets raise accuracy to RMS 6e-4 but cost more traversal time; K=1 has the
lowest crossover). Reason: GPU direct N-body is the near-ideal GPU workload (lockstep broadcast reads, coalesced
FMA, compute-bound), while BH trades cheap FLOPs for divergent memory-bound tree traversal; on the 2070 that
only pays past ~128k. The browser runs N≤~20k and offline `impact-run` at N≈35k — **both far below 128k** — so
wiring BH in-browser (docs/36 stage 8) would *reduce* fps. Also: gravity is only ~25 % of the browser frame at
8k, so it isn't the fps lever regardless.

**Recommendation + open decision.** Keep direct O(N²) gravity for N≤~100k. BH's real niche is **very-high-N
offline convergence (N≳128k)** where it gives a growing speedup (≈9× at 512k extrapolated) — the only path
where the isotopic-fraction scatter (docs/28 ceiling) could be beaten down. So: (A) pursue a converged number →
build the GPU radix sort (docs/36 stage 3, the one hard kernel) + run `impact-run` at N≳128k with BH; or (B)
defer — the verified crate is banked and re-verifiable. The GPU sort was deliberately **not** built (gated on
this decision; most expensive kernel; only needed for option A). Full write-up: **`docs/37`**. Nothing wired,
nothing deployed; on branch off `orbit-diagnostic`.

---

## 2026-07-17 — Direct-sum gravity ceiling measured → GPU Barnes–Hut spec'd for a fresh session (docs/36)

**What.** Measured how far the browser GPU impact's DIRECT O(N²) gravity scales before spec'ing the
Barnes–Hut. On the RTX 2070: N=2800 → ~11 fps (the button default), **N=8200 → 4 fps** (a gorgeous remnant +
spiral-disk, energy still conserved ΔE≈0.08 %, but choppy). The O(N²) dynamics (20 substeps × 2 evals × N²)
is the wall; the offline converged disk (N≈35 000) is unreachable in-browser with direct sum. So a **GPU
Barnes–Hut (O(N log N))** is the agreed next lever — restores fps at 8 k, unlocks N ≳ 20 k for a sharp disk.

**Handover.** Wrote **`docs/36-gpu-barnes-hut-spec.md`** — a self-contained build spec for the next session:
the staged LBVH plan (adaptive bbox reduction → Morton → GPU radix sort → Karras tree → atomic-free bottom-up
COM → θ-traversal), verified GPU-BH-vs-GPU-direct in a new standalone `tools/gpu-bh-verify` (matching the CPU
`bhtree.rs` opening criterion) before wiring into `sph_step.wgsl`/`GpuSph`, then bump N in the browser. Includes
the WGSL gotchas (no float atomics → the atomic-free COM; the mandatory tight bbox) and the hard-won impact
settings the swap must NOT regress (AV-zeroed relax, far-apart relax, the energy-conserving fixed dt). Button
left at the playable N=2800. Nothing deployed.

---

## 2026-07-17 — SOLVED: the in-browser GPU impact forms an orbiting disk (GPU relax + energy-conserving dt) (docs/35)

**Result.** The GPU SPH deformable-Earth impact now runs in the browser at **N≈2800**, conserving energy to
**~0.08 %** and forming a **coherent remnant + an orbiting debris disk** (peaks ~0.6 M☾, up to ~32 % Earth,
Moon-candidate clumps ~0.2 M☾). Rig-verified (`web/rig/sph_energy.mjs`, RTX 2070) — the "lost orbits" are back.

**Two fixes on top of the diagnosis (under-relaxation, energy-conserving fixed dt):**
1. **GPU relaxation** (`GpuSph::encode_relax` / `cs_relax`), so the ~2400 relax steps run on the GPU instead of
   the CPU main thread — the practical blocker, and what lets N rise from ~700 to ~2800. New builders
   `gpu_sph::build_far_apart` (the two bodies placed 40× the contact radius apart, so each self-gravitates in
   the shared buffer with negligible mutual gravity) and `assemble_from_relaxed` (read back → compute the
   collision geometry from the ACTUAL relaxed radii → launch). New `OrbitDemo` phase machine
   (`SphPhase::Relaxing → Assembling → Dynamics`).
2. **No artificial viscosity during relax** (`GpuSph::set_av(0,0)`). Debugging: the first GPU relax DIVERGED —
   the body puffed to ~10³× (remnant "radius" 4×10⁹ m). Cause: the GPU force kernel includes Monaghan AV,
   which the CPU relax does not; AV stiffens the settling transient so the CPU's stable Courant dt rings and
   blows up. Zeroing AV during relax (matching the CPU) makes it stable at the normal dt — and ~4× fewer steps
   than the smaller-dt workaround. AV is restored (1, 2) for the shock-capturing dynamics.

**Honest state.** Energy conservation and the disk are solid; residual escape is still higher than the offline
run and the disk classification wobbles as the hot remnant expands — a coarse-N demo, but a *real* one. The
relax is still ~8–10 s (O(N²) direct gravity × ~2400 steps) — the next speed lever is a **GPU Barnes–Hut**
tree (O(N log N)) to make it snappy and push N higher; an in-kernel per-substep adaptive dt would trim the
escape. The GPU impact stays the "🌋 GPU Impact" button (not auto-deployed to the birth scene). Removed the
now-dead CPU-relax helpers.

---

## 2026-07-17 — Diagnosing the GPU-impact "lost orbits": it's NOT dt injection, it's under-relaxation (docs/35)

**Goal (Robin):** confidently determine whether the in-browser GPU impact is fixable before abandoning it.

**Measure, don't guess.** Added an energy diagnostic — `gpu_sph::total_energy` (KE+IE+PE) + `gpu_energy_json`
— and measured the live impact. **My earlier diagnosis was WRONG:** the total energy is CONSERVED to ~0.01 %
with the current fixed dt (KE falls, IE rises by the same amount — shock heating, correct). So it was never
dt energy-injection. The real cause: **under-relaxation** — I'd cut the browser relax to 640 steps for dev
speed, vs the offline `impact-run`'s ~2200. Unrelaxed bodies carry excess energy and fling debris out (the
3a lesson).

**Result with the relax raised to 2200 (rig-measured, RTX 2070):** energy conserved 0.00–0.02 %; a coherent
bound remnant forms (~9000 km — the SAME size as the offline run) with a debris disk (peaks ~0.35 M☾) and
Moon-candidate clumps (up to 0.34 M☾, 12–44 % Earth). The scene shows a real giant impact (remnant + disk),
not the earlier blown-apart dispersal. **So it is NOT insoluble — do not abandon it.**

**Honest remaining gap vs the offline run:** escape is still ~15× higher (0.8–1.2 vs 0.06 M☾) and the distinct
orbiting disk doesn't cleanly persist — the hot remnant keeps expanding (physical: hot rock → high Tillotson
pressure, no radiative cooling yet) and the disk thins (partly a measurement artefact as the 85 %-mass remnant
radius grows past the disk perigees). At N~700 it's a coarse, marginal disk. **Path to offline quality:** (1)
GPU relax (`cs_relax`) so 2200 steps are milliseconds not ~15 s of CPU — the practical blocker + the key to
(2) higher N; (3) an in-kernel per-substep adaptive dt to trim the excess escape (the fixed dt conserves TOTAL
energy but may mis-distribute at the shock). Kept the c_peak fixed dt (energy-conserving) and the energy
diagnostic; the GPU impact stays the button (WIP), birth scene still Aggregate. Rigs: `web/rig/sph_energy.mjs`.

---

## 2026-07-17 — REVERT: the birth scene goes back to the Aggregate — the GPU impact "loses its orbits" (docs/35)

**What Robin caught.** On the deployed GPU birth scene the debris disperses instead of forming an orbiting
disk/Moon — "we lost orbits." Diagnosed (rig-watch): the **remnant radius grows without bound** — 5994 → 8277
km over 20 s (fixed dt), and worse (→21687 km) with a frame-lagged adaptive dt. Cause: **spurious energy
injection**. The browser GPU impact must use a FIXED dt (WebGPU forbids the blocking read-back the offline
adaptive dt needs); a fixed dt can't hold through the shock (c spikes ~4×) so it pumps energy in and the
material puffs apart. A frame-lagged Courant dt (computed on the CPU from the one-frame-old snapshot) is
WORSE — applied across 20 substeps it overshoots the live shock and explodes. So at browser resolution
(N~700, no per-substep adaptive dt) the impact is not energy-conserving enough to orbit — unlike the offline
`tools/impact-run` (N~35k, per-step adaptive dt, energy conserved 0.3–0.5 %).

**What I did wrong.** I deployed the GPU impact as the default birth scene having verified it *ran*, not that
it produced a good orbiting result — violating my own docs/35 guardrail ("keep the CPU path until the GPU
replacement is verified good"). Corrected: **reverted `birth.html` to the CPU `Aggregate` scene** (which lofts
an orbiting disk → moonlets → a Moon; rig-confirmed restored: 1536 fragments, disk 2.84 M☾ in 2 moonlets) and
**redeployed**. The GPU SPH impact stays the "🌋 GPU Impact" **button** — a WIP physics demo — until its
energy conservation is fixed. The `Space` tab's Sun–Earth–Moon orbits were never affected (rig-verified:
Moon orbiting at ~1.02 km/s). Removed the failed frame-lagged `courant_dt`; the button keeps the shock-safe
fixed dt (puffs slowly but doesn't explode). **Next (to make the GPU impact orbit):** a true per-substep
adaptive dt (a GPU Courant reduction feeding the next substep in-kernel, no CPU round-trip), full GPU relax
(`cs_relax`), and higher N — then re-promote to the birth scene.

---

## 2026-07-17 — Stage 5 migration, increment 2c: geologic hand-off from the GPU disk (docs/35)

**What.** The Geologic button now works in the GPU birth scene (was an Aggregate-only path). New
`gpu_sph::disk_moonlets`: from the read-back disk it finds the self-bound clumps (the `accretion` operator)
and promotes each to a `tides::Moonlet` orbiting the REAL Earth just outside Roche (~3.8 R⊕), carrying the
clump's mass; if no tight clump has formed yet it promotes the whole bound-disk mass as one moonlet (in
geologic time the disk accretes a Moon regardless). `OrbitDemo::enter_geologic_time` branches on `sph_active`:
promote → retire the GPU sim → hand to the validated secular tidal law. Guarded so clicking Geologic before a
disk exists is a no-op (keeps impacting) rather than blanking the scene. With the birth scene fully on GpuSph,
`moon_debris` (`Aggregate`) is now dormant in `OrbitDemo` — functionally retired (the struct deletion waits on
step 5, once the terrain probe also migrates).

**Verified (rig-watch, release build — `web/rig/birth_geologic.mjs`).** Birth impact → disk forms (disk
0.12–0.23 M☾, up to 68% Earth) → `enter_geologic_time()` → `disk_stats_json` returns the GEOLOGIC JSON
(geologic mode active, populated from the GPU disk) and the scene transitions to the geologic Earth view
(grain-shell Earth, camera backed out, HUD "T+1641y after impact"). Native + wasm build clean. Honest notes:
(1) the promoted moonlet then decays under the secular law because this scene gives Earth no spin (a
sub-synchronous moonlet migrates in and shreds at Roche — the existing `tides` physics, not a hand-off bug;
giving the birth Earth a spin, or seeding the moonlet further out, is geologic-scene polish). (2) In the
UNOPTIMIZED dev build the chunked CPU relax pegs the birth scene to ~1 fps for ~30 s (700 particles × 640
relax steps); release is ~10× faster and fine — GPU relax (`cs_relax`) is the proper future fix for dev too.

---

## 2026-07-17 — Stage 5 migration, increment 2b: the Birth-of-the-Moon scene runs on GpuSph (docs/35)

**What.** The "Birth of the Moon" scene now runs the **GPU SPH deformable-Earth impact** instead of the CPU
rigid-Earth `Aggregate` — two differentiated EOS bodies colliding, stepped by `sph_step.wgsl` in-browser.
Fixed the load-freeze blocker (2a) by making the build **non-blocking**: `build_impact_bodies` returns the two
bodies UNRELAXED; `advance` relaxes them in small CPU chunks (20 steps/frame, ~32 frames) via a new
`sph_relax` phase, re-uploading the settling bodies each frame, then `assemble_impact(…, infall=true)`
launches the collision (Theia inbound) and hands off to the GPU KDK dynamics + read-back. Refactored
`gpu_sph.rs` into `build_impact_bodies` / `relax_chunk` / `assemble_impact` (the last is pure — offsets in the
emitted particles, no body mutation, so it can be called every relax frame). `birth.html`/`orbit.ts`
auto-start it; Replay restarts it.

**Verified.** Native + wasm build clean. Rig-watch `birth.html` in the **dev** build (previously the freeze):
loads, the two bodies settle (~1 s, disk "null" during relax), then collide into a mixed remnant + spreading
debris — **no hang, 27 fps**, the birth HUD shows the live GPU disk line. Release build also confirmed. Honest
status: this **changes the deployed birth scene's character** (the Theia-approach narrative + the Aggregate
disk/geologic controls are bypassed — `moon_debris` is now dormant, and the Geologic button no-ops in GPU
mode); it's committed on the branch, not deployed. Remaining for increment 2: retire `moon_debris`
`Aggregate` and rewire the geologic hand-off from the GPU disk (via `accretion.rs`). Then 5c (Sphere), 5d.

---

## 2026-07-17 — Stage 5 migration, increment 2a: GPU impact scene framing (+ a blocker found) (docs/35)

**What.** Toward "the birth scene runs on GpuSph" (docs/35 step 2). The GPU impact rendered as a speck at the
Earth–Moon default zoom; added a dedicated visual scale (`SPH_VIS_SCALE`, Earth's ~5000 km → a few display
units) and a camera zoom-in on trigger, so the impact is legible — a clear central remnant plus a spread
two-provenance debris disk of individual shaded particles. Rig-watch verified on the space scene (HUD: "disk
0.35 M☾, 15% Earth, moon 0.15 M☾").

**Blocker found (honest).** Auto-starting the GPU impact on `birth.html` load **froze the page** — the
one-time CPU relax (`build_deformable_impact`, ~900 particles × ~900 damped steps) runs synchronously on the
wasm main thread and, in the unoptimized dev build, blocks long enough that the scene never paints (rig
screenshot timed out). So `birth.html` stays on the existing `start_birth` (Aggregate) for now; the GPU impact
is the deliberate "🌋 GPU Impact" button. Making the birth scene *default* to GpuSph needs a non-blocking
build first — a GPU relax (`cs_relax` already exists) driven over a few frames, or a lighter/deferred CPU
relax — which is the real next step (docs/35 step 2, revised). Reverted the auto-start; nothing left broken.

---

## 2026-07-17 — Stage 5 migration, increment 1: GPU→CPU read-back + live disk stats (docs/35)

**What.** Robin chose to unify the scenes onto the **GPU SPH path** (retire the CPU `Aggregate` from the live
scenes) — the high-payoff, high-risk direction. Wrote the increment plan in **`docs/35-gpu-path-migration.md`**
(sequence, guardrails, and the one open design decision flagged for later: pure-SPH-EOS vs SPH-EOS+granular on
the GPU). Increment 1 is the universal prerequisite — nothing can migrate until the scene can read GPU
particle state back. Added two-phase async read-back to `GpuSph` (`begin_readback`/`take_readback`, mirroring
`GpuParticles`; WebGPU forbids blocking maps, so it copies one frame and collects the next). `OrbitDemo`
reads back each frame into `sph_snapshot`; `gpu_sph::disk_stats_json` measures the orbiting disk on it
(remnant = 85%-mass body, perigee-above-remnant classification, provenance split) and the largest self-bound
clump via the verified `accretion` operator; `OrbitDemo::gpu_disk_stats_json()` exposes it to JS, shown in the
birth HUD. `mod gpu_sph` is now `#[cfg(target_arch="wasm32")]` (it's only used by the wasm-only `mod app`; the
native SPH reference stays in `tools/`).

**Verified.** Native + wasm builds clean. Rig-watch (`web/rig/sph_impact.mjs`, RTX 2070): triggered the GPU
impact, and the HUD shows the **live read-back disk provenance updating each frame** — e.g. `disk 0.35 M☾
(8% Earth) · moon 0.07 M☾` at t+8.5 s, evolving as the remnant + debris disk form. The read-back → CPU
measurement → JSON → HUD path works end-to-end. (The low/jumpy Earth% is the chaotic N~1050 browser run — a
live visualization, not the converged number; `tools/impact-run` remains the faithful measurement.) Next
increment (docs/35 step 2): put the "Birth of the Moon" scene fully on `GpuSph` and retire `moon_debris`.

---

## 2026-07-17 — Stage 5 (begin): the EOS seam — one pressure abstraction across air and rock (docs/33 §4.5)

**What.** Stage 5 is "retire the forks — unify the particle containers." The blocker the fork map surfaced:
the symmetric SPH pressure-force loop `a = −Σ m (P_i/ρ_i² + P_j/ρ_j²) ∇W` is written THREE times (`AirField`,
`HydroBody`, `aggregate` vapor) differing only in the `P(ρ,u)` call — because there is **no EOS abstraction**
(Tillotson and the inline ideal-gas `ρ·R_s·T` are unrelated). Added one: `eos::Eos` — an enum
`{ Tillotson(Tillotson), IdealGas { rs_t } }` with `pressure`/`sound_speed_sq`/`rho0`, plus `From<Tillotson>`.
Migrated `HydroBody` to carry `Vec<Eos>` instead of `Vec<Tillotson>`, so the one verified SPH container is now
**EOS-agnostic** — it can hold ideal-gas parcels (air) or Tillotson parcels (rock/iron) on the same code
path. This is the seam that lets `AirField` fold into `HydroBody` (next increment) rather than duplicate the
density/force/relax loops.

**Why.** `HydroBody` is the convergence target (it's the CPU reference the stage-4 GPU kernel is verified
against, and it's wired into `gpu_sph.rs`); `AirField`/`Sphere` are legacies to fold toward it. The EOS trait
is the documented precursor (eos.rs's own module doc already claimed "only the `P(ρ,u)` call changes" — this
makes that literally true).

**Verified.** New fast test `eos_enum_dispatches_ideal_gas_and_delegates_tillotson`: ideal gas gives
`P = ρ·rs_t` independent of u and `c² = rs_t`; Tillotson wrapped in the enum is **byte-identical** to calling
the material directly (asserted with `==`). The migration is pure type-wrapping (Eos::Tillotson delegates
exactly), so the Tillotson SPH physics is unchanged — confirmed by re-running the full differentiated-planet
settle: **central P 5.723e11 Pa, core 15591 / mantle 5534 kg/m³** (identical to before). Fast suite 156/156.
Next: fold `AirField`'s SPH into `HydroBody` (needs an optional planar-ghost boundary + external-gravity
option — the one thing AirField does that HydroBody can't yet); then the CPU grain-path decision (5b),
`Sphere` collapse (5c), WGSL-from-Rust (5d).

---

## 2026-07-17 — Stage 4c.4: the GPU SPH deformable-Earth impact runs IN THE BROWSER (docs/33/34)

**What.** Wired the verified GPU SPH stepper into the birth scene so the deformable-Earth giant impact runs
live in-browser (WebGPU), completing stage 4c. New engine module `crates/engine/src/gpu_sph.rs` (`GpuSph`) —
the WebGPU host for `shaders/sph_step.wgsl`: owns the 8-binding pipelines + buffers on `OrbitDemo`'s shared
device, uploads a particle set, and encodes batches of KDK (or relax) substeps. New shader
`shaders/sph_render.wgsl` draws the particles as instanced camera-facing billboards straight from the physics
buffer (zero-copy; pos at byte 0, provenance u32 at byte 44 → Earth = warm rock, Theia = cool steel). New
`OrbitDemo::start_gpu_impact()` (JS button "🌋 GPU Impact") builds + relaxes two differentiated bodies on the
CPU (`gpu_sph::build_deformable_impact`, reusing the verified `HydroBody`), places them on the oblique
giant-impact geometry, and hands the per-frame dynamics to the GPU; `advance()` encodes 8 KDK substeps/frame,
`render()` draws the field. Two WebGPU-shaped choices (documented in the module): **fixed dt** (adaptive
Courant needs a blocking read-back WebGPU forbids) and an **Earth-relative f32 frame** (planetary coords
cancel in f32; the shader re-adds Earth's display position).

**Why.** docs/34 4c.4 — the impact should be visible/interactive in the browser, not only in the offline
native tool. The physics laws stay the shared `sph_step.wgsl` (docs/32 §4: don't fork the particle path — this
is the FIRST in-engine host of that shader, not a fork); only a render pipeline is new to `OrbitDemo`.

**Verified.** `cargo build -p engine --target wasm32-unknown-unknown` clean → the WGSL validates under
WebGPU and the wiring compiles. Rig-watch (`web/rig/sph_impact.mjs`, headed Chromium + xvfb + Vulkan WebGPU on
the RTX 2070): clicked the trigger, watched the whole event — two intact differentiated bodies (t≈0) →
collision + spreading (t+2 s) → a **central remnant plus an extended two-provenance debris disk** (t+8.5 s),
Earth (tan) and Theia (blue) material visibly mixing. No NaN blow-up (the fixed dt held through the shock),
24–25 fps. Screenshots in the job scratch. Native fast suite green. Honest caveats: modest N (~1050) and
fewer relax steps than the offline run (a snappy trigger, slightly hotter start), small on-screen at the
default zoom, no read-back so no live momentum-mirror/HUD numbers yet — all polish, not correctness. This
closes stage 4c (4c.1 integrator, 4c.2 high-N impact, 4c.3 accretion, 4c.4 browser). Remaining realignment:
stage 5 (fold `hydrostatic`/`AirField` into one `Aggregate`) and 6 (energy-tiered JIT particalization).

---

## 2026-07-17 — Stage 4c.3: the accretion / growth operator, conservation-verified (docs/33/34)

**What.** New engine module `crates/engine/src/accretion.rs` — the growth law that lets a round Moon emerge
from the disk. A giant-impact disk of equal-mass SPH particles has no fusion operator (masses never grow), so
it can never coalesce a Moon (diagnosis, JOURNAL entry below). The operator: friends-of-friends clustering
(union-find over particles within a linking length, the same primitive `disk_stats_json` uses) → classify
each clump for two **honesty gates** — (1) genuinely self-bound (`Σ½mᵢ|vᵢ−v_com|² + PE_self < 0`) and (2)
outside the remnant's fluid Roche limit `2.44·R·(ρ_planet/ρ_clump)^⅓` — → PROMOTE each qualifying clump to
ONE body at its COM (mass `Σm`, velocity `Σmv/Σm`, radius from ρ·V). A clump inside Roche is left as particles
(it should tidally shred, not accrete — consistent with `tides::secular_step`).

**Why.** Stage 4c.2 made the disk collisional at high N; this adds the law that turns a bound clump into a
body. Designed as a pure, decoupled function over `(pos, vel, mass, rho)` arrays so it is unit-testable and
reusable — not welded to a scene struct.

**Verified (TDD, `bash scripts/test.sh accretion`, 3/3).** (1) `accretion_conserves_mass_momentum_and_com` —
promote two cold blobs among scattered singletons; expanding bodies+residuals back out conserves total mass,
linear momentum, and centre of mass to **< 1e-12** (exact to f64 round-off), and the 5 singletons are left
alone. (2) `roche_gate_blocks_accretion_inside_the_limit` — the *same* clump accretes outside Roche but NOT
inside it. (3) `unbound_hot_group_does_not_accrete` — a spatially-tight but hot (KE ≫ binding) group is
classified unbound and rejected. Honest about what promotion cannot conserve: internal random KE is absorbed
as heat (physical for inelastic accretion) and internal spin L is folded in — both reported, never dropped.
Full fast suite 155/155.

**Demonstration on a real disk.** Wired a `moon_candidate` scan into `tools/impact-run` (the same FoF +
self-bound + Roche logic, reimplemented standalone like sph-verify) and ran it on the N=35 000 aftermath: the
disk (0.14 M☾, 29 % Earth) contains **21 clumps, 16 of them self-bound**, and the largest bound clump outside
Roche is **0.023 M☾ (31 particles), 10 % Earth** — a proto-moonlet SEED, not a full Moon. Honest: at this N
and only ~9 h of aftermath the disk has begun to clump but is far from accreting a lunar-mass body (real Moon
accretion takes years–decades and/or ≫10⁵ particles). The operator correctly finds the bound clumps; growing
them to a Moon is a longer-time / higher-N run, not a code gap. Next: 4c.4 (browser scene wiring).

---

## 2026-07-17 — CORRECTION to stage 4c.2: the disk composition has large run-to-run SCATTER, not clean convergence

**What I got wrong.** The 4c.2 entry below reported the disk Earth-fraction "converging monotonically
28→33→50 %" toward the CPU's 58 %, from ONE run per N. Re-running the **identical** N=35 000 config (same
binary, same seeds — the build is deterministic) gave **29 %**, not 50 %. Two samples at the same config,
21 points apart. The cause is honest and physical: the GPU grid-insert uses `atomicAdd` for bucket slots, so
neighbour-iteration order is non-deterministic across runs; f32 sums are non-associative; and 11 000 chaotic
integration steps amplify that seed into a macroscopically different disk. **So there is no clean monotonic
convergence — the composition has ~20-point run-to-run scatter, and 28/33/50/29 % are samples of a
distribution around ~30–40 %, consistent with the CPU's 58 % only within that large scatter.** The
no-fudge rule (CLAUDE.md #5) required recording this rather than keeping the favourable sample.

**What still stands (robust across all runs).** The MECHANISM — Earth material reaches orbit in quantity —
and the disk **mass** (~0.13–0.19 M☾), **remnant radius** (~9000 km), **escape speed**, and **energy
conservation** (0.3–0.5 % over ~10 h) are all stable run-to-run. Only the Earth *fraction* of the disk is
scatter-dominated at these N. A converged fraction needs an **ensemble** (average many realisations) and/or
a **deterministic reduction** (order-independent summation), plus higher N — all future work. The
deformable-Earth qualitative result (Earth-derived material orbits, tens of % of the disk) is reproduced;
the precise fraction remains an IOU, now with a measured scatter attached.

---

## 2026-07-17 — Stage 4c.2: high-N giant impact on the GPU (deformable-Earth disk at N up to 35 000) (docs/33/34)

**What.** Built `tools/impact-run` — a standalone offline harness that runs the deformable-Earth giant impact
end-to-end on the RTX 2070 using the verified `sph_step.wgsl` kernels: build two differentiated EOS bodies →
**relax each on the GPU** (new `cs_relax` damped kernel) → collide obliquely at 1.15·v_esc, b≈R_e → KDK-step
the aftermath with **adaptive Courant dt** (new `cs_signal` kernel; CPU reads back the per-particle min each
step) → classify remnant/disk/escaped by the perigee-above-remnant criterion, split by provenance. Added a
`prov` field to the particle (repurposed `_pad`) and a `damp` field to `Params`. This runs the *same*
experiment the CPU test measures at N≈2100 (`a_deformable_earth_impact_measures_the_disk_provenance`), but at
N up to 35 000 in minutes — the resolution the isotopic-crisis number needs.

**Why.** The CPU O(N²) impact caps at ~2100 particles (~8 min/run) and the docs/33 stage-3c result (58% of
the orbiting disk is Earth-derived) was explicitly a coarse-N / sub-scale IOU — mechanism asserted, fraction
not converged. Stage 4 exists to lift the resolution on the GPU.

**Verified (RTX 2070).** Energy conserved to **0.3–0.5%** across ~10 h of simulated aftermath at every N
(the relaxed-body + shock-capturing-AV discipline holds; IE rises ~3× from shock heating). Samples measured:

| run                    |   N    | disk Earth-frac | disk mass | R_remnant | relaxed R_earth |
|------------------------|-------:|----------------:|----------:|----------:|----------------:|
| GPU (direct grav, f32) |  2 100 |           28 %  | 0.19 M☾   | 9208 km   | 4245 km         |
| GPU                    | 14 000 |           33 %  | 0.13 M☾   | 9127 km   | 4482 km         |
| GPU                    | 35 000 |           50 %  | 0.13 M☾   | 8834 km   | 4679 km         |
| GPU (re-run, same cfg) | 35 000 |           29 %  | 0.14 M☾   | 9047 km   | 4679 km         |
| CPU (Barnes–Hut, f64)  |  2 100 |           58 %  | 0.21 M☾   | 9086 km   | —               |

**Read this table with the CORRECTION above:** the two 35 000-particle rows are the SAME config (50 % vs
29 %), so the Earth-fraction column is scatter-dominated (~20 points, GPU-non-determinism × chaos) — do NOT
read 28→33→50 as convergence. What IS robust across every row: the disk **mass** (~0.13–0.19 M☾), **remnant
radius** (~9000 km), **escape speed**, and energy conservation. The deformable-Earth mechanism (Earth-derived
material reaches orbit, tens of % of the disk) reproduces on GPU; the precise fraction is an IOU pending an
ensemble average + a deterministic (order-independent) reduction + higher N. Honest caveats: sub-Earth scale,
direct O(N²) gravity (a GPU Barnes–Hut is the next optimization if N≫10⁵). Run: `cd tools/impact-run &&
cargo run --release -- [earth_n] [steps]`. Next: 4c.3 (accretion operator) and 4c.4 (browser scene wiring).

---

## 2026-07-17 — Stage 4c.1: GPU KDK integration loop, verified over 50 steps (docs/33/34)

**What.** Turned the verified 4a/4b force kernel into a **time integrator**. Added two kernels to
`shaders/sph_step.wgsl` — `cs_kick_drift` (first half-kick of v & u, clamp `u=max(u,0)`, then drift x) and
`cs_kick` (final half-kick) — and a `dt` field to `Params` (repurposed the trailing `_pad`). One dynamical
step = TWO force evals with a half-kick+drift between and a half-kick after, matching the CPU
`HydroBody::step` KDK leapfrog operator-for-operator (energy-conserving, no damping). Per docs/34 the verify
uses a FIXED dt on both sides; GPU adaptive Courant dt (CPU read-back of a min) is deferred until it's needed
by a real run.

**Why.** The force kernel was one evaluation; a giant impact needs the loop. Verify-before-wire discipline
(docs/30): prove the integrator matches the CPU leapfrog before running it at high N or wiring it to a scene.

**Verified (RTX 2070, `tools/sph-verify`).** Extended the harness with an f64 CPU KDK reference (genuine f64
state, no f32 round-trip between steps — a true higher-precision reference) and a GPU multi-step runner (all
passes in one command buffer; consecutive compute passes are ordered & memory-synchronized so step k's drift
is visible to step k+1's density). 50 steps at dt=0.5s from the same IC: GPU f32 vs CPU f64 final state
**pos RMS 3.1e-4, vel 5.7e-4, u 5.1e-4** (displacement-scaled pos) — inside the ~1e-3 honest f32-vs-f64
bound and *tracking*, not diverging. The single-eval force check still PASSes (acc 1.85e-6, du/dt 4.36e-6).
`cargo run --release` exits 0 on both. Next: 4c.2 (high-N impact for the converged disk-provenance number).

---

## 2026-07-17 — Stage 4c prepped for a fresh session + landing hero shipped (docs/34)

**What.** Two things closing out a long session. (1) Built + deployed the **landing-page hero N-body
field** (front-end handoff): a real 2-D velocity-Verlet `F = G·m/r²` sim in `web/src/landing.ts` with honest
live telemetry (bodies / steps / Σ½mv²) and drag-to-toss — the page no longer over-promises. Verified (tsc,
vite, rig-screenshot), live on integrity.bothead.net. (2) Wrote **`docs/34-stage-4c-pickup.md`** — a
self-contained spec so a new session executes stage 4c without re-deriving: the verified 4a/4b foundation
(`sph_step.wgsl` force kernel + grid, `tools/sph-verify`), the four 4c sub-tasks (GPU KDK integration loop +
adaptive dt → high-N impact for the converged number → accretion operator → browser scene wiring), and the
session's hard-won gotchas (engine wgpu is webgpu-only → verify in a standalone Vulkan crate; the grid
cell-membership guard; relax-before-collide; f32 Earth-relative frame; verify-before-wire).

**State.** Realignment stages 1–3 + 4a + 4b DONE and verified; 4c prepped. Working tree clean, all pushed.

---

## 2026-07-17 — Realignment stage 4b: the SPH neighbour grid on GPU, verified (docs/33)

**What.** Added a spatial-hash **neighbour grid** to `shaders/sph_step.wgsl` so the short-range SPH
(density + pressure + AV) scans only the 27 neighbouring cells — O(N) instead of O(N²). Two new kernels
(`cs_grid_clear`, `cs_grid_insert`, atomic bucketing, adapted from `particle_step.wgsl`) build the grid; the
density and force passes look up neighbours via it. Long-range self-gravity stays direct O(N²) (GPU-tiled
direct summation is tractable at these N; a GPU Barnes–Hut tree is a later optimization). Verified on the
RTX 2070 (`tools/sph-verify`): gridded output matches the CPU physics to f32 precision (acceleration RMS
1.9e-6, density 5.6e-7) — the grid is EXACT, like `neighbors.rs`.

**BUG found + fixed (the interesting part).** The first gridded version was 20% off — it found MORE
neighbours than truth (109 vs 88 for the worst particle): **hash collisions among the 27 scanned cells made
some real neighbours read TWICE** (two cells hashing to the same bucket → the bucket processed twice). The
fix is a **cell-membership guard**: when scanning cell C, a bucketed particle j is used only if
`cell_of(j) == C` — so each neighbour is counted exactly once (and collided far particles are skipped),
regardless of table collisions or bucket size. This is the exactness guarantee `neighbors.rs` gets for free
on the CPU. Isolated it by (a) confirming all-N density was exact, then (b) a neighbour-count diagnostic
showing over-counting — not a coverage/precision miss.

**Verified (real GPU).** `sph-verify` PASS at production bucket_k=64: density (grid) max rel error 5.6e-7,
acceleration RMS 1.9e-6, du/dt 4.4e-6. Ahead: 4c — the KDK integration loop + adaptive Courant dt on-GPU +
scene wiring (with the accretion operator).

---

## 2026-07-17 — Realignment stage 4a: the GPU SPH kernel, verified on the RTX 2070 (docs/33)

**What.** Ported the space-band self-gravitating condensed-matter force step to a WGSL compute shader
(`shaders/sph_step.wgsl`) — the same physics as the CPU `hydrostatic.rs::forces_and_dudt`, in f32: SPH
density (cubic spline, per-pair h_ij), Tillotson EOS pressure, Monaghan artificial viscosity, direct O(N²)
self-gravity, and the du/dt energy equation. The goal is to run the giant impact at N~10⁵ (the resolution
the isotopic number — and accretion — need). VERIFIED against an independent f64 CPU computation of the same
equations, headless, on the box's RTX 2070 via native Vulkan wgpu (`tools/sph-verify` — a standalone crate,
since the engine's own wgpu is webgpu-only and can't run native Vulkan).

**Verified (real GPU).** `sph-verify` (N=300, mixed iron/basalt, velocities to exercise the AV): GPU vs CPU
**acceleration RMS relative error 1.9e-6**, max per-particle 2.2e-5, **du/dt RMS 3.6e-6** — i.e. the WGSL
matches the CPU physics to f32 round-trip precision. The kernel is faithful.

**Scope.** This is ONE force evaluation, O(N²), verified. Still ahead: 4b — port the neighbour grid +
Barnes–Hut (the CPU already has both, `neighbors.rs`/`bhtree.rs`) for O(N log N); 4c/5 — the KDK integration
loop on-GPU + the adaptive Courant dt + wiring into the scene (with the new **accretion operator** the
Moon-formation diagnosis showed is also required). But the hard, error-prone part — getting the SPH+EOS+AV
+gravity physics correct in WGSL f32 — is done and proven on the real device.

**Why.** docs/33 stage 4: correctness-first — verify the GPU kernel against the CPU reference on the real
GPU before wiring it into anything (docs/30 discipline: speed must never change the answer).

---

## 2026-07-17 — Can the disk accrete a Moon? Diagnosis + the Roche-disruption fix (docs/28/33)

**What.** Robin, watching the deployed birth scene: "I never see particles join — no accretion into a
Moon; and geologic time makes a giant ball ROLL ON EARTH'S SURFACE, not orbit." Investigated both.

**Diagnosis (can a near-spherical Moon emerge in the current system? NO):**
- **Primary — the collisionless-N ceiling + NO accretion operator.** The scene disk is ~1536 chunks each
  **471 km radius, 0.017 M☾** — collisionless at this N (docs/28's flagged LOD ceiling; real SPH disks use
  10⁴–10⁶). The contact law is fine (restitution 0.40 → ~84% collision-energy loss; self-gravity ~3500×
  cohesion at 471-km grains, correctly the glue). The real gap: **there is no fusion/growth operator** —
  debris `bonds` is empty and never populated, particle masses never grow, the devs deleted the merge
  closure and bet on emergence. So a bound clump renders as a loose cluster of 471-km balls, never a
  growing sphere. **A round Moon needs BOTH higher N (stage 4) AND a coarse-grained accretion law** (a
  bound rubble clump → one body with a grown radius). That accretion operator is a new realignment element.
- **The "ball on the surface" was a real BUG (fixed).** A sub-synchronous geologic moonlet correctly
  migrates inward (Phobos' fate), but `tides::secular_step` CLAMPED its orbit at 1.2 R⊕ and the renderer
  drew a full-mass ball overlapping Earth — no Roche limit enforced.

**Fix.** `tides::secular_step` now enforces the **fluid Roche limit** `d = 2.44·R·(ρ_p/ρ_m)^⅓` (≈ 3.0 R⊕
for Earth + rock): a moonlet that decays inside it is **tidally SHREDDED** — removed, its mass + orbital
angular momentum raining onto the planet (mass returned to the caller and added to Earth in `lib.rs`; L
added to the spin). Removed the 1.2 R⊕ floor clamp. So a sub-synchronous moonlet disrupts instead of
rolling on the surface, and a Moon that forms just outside Roche migrates out honestly.

**Verified (native).** New `a_sub_synchronous_moonlet_disrupts_at_roche_not_on_the_surface`: moonlet at
3.2 R⊕ + 24 h day → disrupts at the 3.02 R⊕ Roche limit, sheds its full 0.30 M☾, total mass + angular
momentum conserved. The existing one-Moon test still forms a Moon just outside Roche that migrates to 29 R⊕
(L drift 5e-15). Full fast suite 152/152; wasm builds. Deployed.

---

## 2026-07-17 — Render-truth: the crater and continents CO-ROTATE with the crust (birth scene)

**What.** Fixed a render-frame mismatch Robin caught while watching the deployed birth scene (he read
Theia's approach as "curving to hit a fixed point"). Investigation verdict: **the approach trajectory is
HONEST** — pure N-body gravity (`orbit::verlet_step`, no steering), the impact site is an OUTPUT discovered
by swept CCD at contact (`impact_site_rel` is `None` through the whole approach), and the inward curve is
genuine gravitational focusing of a hyperbolic impactor in an Earth-centred frame. The "fixed impact point"
he reacted to is the **declared-zero proto-Earth spin** (`lib.rs:2915`, flagged unknown IC): with `spin_l=0`
the surface simply isn't rotating.

BUT the trace surfaced a genuine no-fudge bug: post-impact, once the collision spins Earth up, the crater
(`impact_site_rel`) was rendered as an INERTIAL vector (`earth_center + rel`) while the shell grains rotate
by `spin_rot` — so the hole slid through the rotating crust. And the landmask was sampled at the WORLD
direction (`earth_surface_material(spin_rot·fib_dir)`), painting continents world-fixed while grains rotate
underneath. Both fixed: the crater now co-rotates (`earth_center + spin_rot·rel`) and continents are sampled
at the fixed BODY direction (`earth_surface_material(fib_dir)`) — so grains, continents, and crater share
ONE crust frame that rotates honestly. (Invisible during the birth approach — `spin_l=0` ⇒ `spin_rot` is
identity — so the honest approach is unchanged; the fix bites post-impact when Earth spins up.)

**Verified.** Native + wasm build; full fast suite 151/151. Deployed.

**Flagged for Robin's call (physics IC, not a bug).** The birth scene's proto-Earth spin is deliberately
zero so the post-impact day EMERGES. If we'd rather the surface visibly rotate under the incoming impactor
(more physical — planets rotate), we give proto-Earth a primordial spin IC; the tradeoff is the day becomes
primordial + impact rather than purely emergent. Left as-is pending his decision.

---

## 2026-07-17 — Realignment stage 3c: a DEFORMABLE Earth resolves the isotopic-crisis DIRECTION (docs/33)

**What.** The scientific payoff of the whole realignment: collided a differentiated Theia into a
**deformable, self-gravitating, differentiated proto-Earth** (both real EOS particle bodies, relaxed first)
obliquely at ~mutual escape speed, integrated the aftermath with the shock-capturing SPH integrator (3a),
and MEASURED the bound orbiting disk by provenance (Earth particles vs Theia). Disk = bound material whose
orbital **perigee is above the remnant surface** (genuinely orbiting, separated from the planet body —
`orbit::perigee` about the 85%-mass remnant). No dial; the composition EMERGES.

**MEASURED (native, #[ignore], ~446 s).** `a_deformable_earth_impact_measures_the_disk_provenance`
(M_e=1.75e24 kg ≈ 0.29 M⊕, M_t=2.76e23, v≈7.3 km/s, N≈2100):
- **Orbiting disk 0.207 M☾ — 58% EARTH-derived** (Earth 8.75e21 | Theia 6.43e21 kg).
- Remnant: Earth 1.72e24 | Theia 2.22e23 kg; escaped: 2.1e22 | 4.7e22 kg.

**THE FINDING.** The rigid-boundary Earth capped the disk at **7–12% Earth** (docs/31 — only the excavated
cap could reach orbit). With Earth as REAL MATTER that can shed its own mantle, the disk jumps to **58%
Earth-derived** — Earth material not only reaches orbit, it DOMINATES the disk. This is the direction the
isotopic crisis demands (the real Moon is isotopically Earth-like), and it is exactly docs/28 root-cause #1
(the rigid boundary) being dissolved. Earth is now a participant in its own catastrophe.

**Honest caveats (no-fudge).** Sub-Earth scale (0.29 M⊕), coarse N (~2100 — a resolution/scale IOU,
docs/28), and the post-impact remnant is hot/expanded (R_remnant 9086 km), so the disk is defined beyond
that. **58% is the DIRECTION** (rigid ~10% → deformable ~58%), NOT a converged number — the converged
value waits for the GPU N (stage 4). A first attempt with a too-head-on geometry merged with no disk and
mis-measured (counted the whole extended Earth as "disk" — 89%); that artifact was rejected and the
measurement fixed to the perigee-above-remnant criterion.

**Verified.** Full fast suite 151/151; wasm builds. Stages 1→3c all green.

---

## 2026-07-17 — Realignment stage 3a: dynamical SPH — energy equation + artificial viscosity (docs/33)

**What.** Turned the isothermal planet into a full thermodynamic SPH body for the impact: added to
`hydrostatic.rs` (1) the **SPH internal-energy equation** `du_i/dt = ½ Σ_j m_j (P_i/ρ_i²+P_j/ρ_j²+Π_ij)
(v_i−v_j)·∇W` — the thermodynamically consistent partner of the momentum equation, so compression does PdV
work → heat; (2) **Monaghan artificial viscosity** Π_ij (α=1, β=2) for shock capture (without it SPH
particles interpenetrate at a shock and the impact heating is wrong); (3) an **energy-conserving KDK
leapfrog** `step(dt)` evolving position, velocity, AND internal energy (vs the damped `relax_step`); and (4)
an **adaptive Courant timestep** `courant_dt` from the live compressed sound speed.

**Verified (native, #[ignore], ~67 s).** `a_head_on_collision_conserves_energy_and_shock_heats`: two 400 km
basalt bodies, **relaxed to equilibrium first**, collide head-on at ±1.5 km/s —
- **Total energy (KE+IE+PE) conserved to ~3%** (a one-time injection at the shock front, then flat — the
  known SPH internal-energy-formulation shock error; 5% asserted bound).
- **Shock heating:** internal energy rose **4.9×** (bulk KE → heat), KE fell — the physics that vaporizes
  material and drives the disk.

**KEY LESSON (measured).** Colliding UNRELAXED spheres at 3 km/s TRIPLED the total energy (ΔE/E≈2) — the
startup non-equilibrium dumped into the shock; adaptive dt barely helped (so it wasn't CFL). Relaxing each
body first (Genda: "vibrations until v<100 m/s") + a moderate speed → 3% conservation. Real giant-impact
SPH always relaxes the bodies first; now we do too. Full fast suite 151/151; wasm builds.

**Why.** docs/33 stage 3: the two-body impact needs real shock thermodynamics (heating → vaporization → the
disk), not just contact. This is the integrator the deformable-Earth impact (3b/3c) runs on.

---

## 2026-07-17 — Realignment stage 2b: a differentiated iron-core Earth holds itself up (docs/33)

**What.** Built the layered/differentiated planet — an **Earth-mass iron-core + basalt-mantle** particle body
that holds itself in hydrostatic equilibrium as real matter. Rewrote `hydrostatic.rs` with the **Genda et al.
2012 method** (the fix for the earlier puff-up): **equal-mass particles** at the number density that recovers
each material's ρ₀, with a **per-particle adaptive smoothing length** `h_i ∝ (m/ρ₀)^⅓` (dense core sampled
finely, light mantle coarsely) and a symmetric per-pair `h_ij=½(h_i+h_j)`; per-particle EOS. `HydroBody`
gained `new_differentiated(core, mantle, core_r, total_r, u, N)`. Iron EOS updated to the verified/open
**Wissing & Hobbs 2020** compressed-branch refit (ρ₀=7850, A=128, B=181.5 GPa, a=0.5, b=1.28, E₀=14.25
MJ/kg); its vapor branch stays flagged provisional (stage-3 concern). Also fixed the EOS continuity test's
tolerance (it collapsed at iron's tension zero-crossing near E_iv — the function is continuous; smaller δ +
a bulk-modulus scale floor).

**Verified (native, #[ignore], ~326 s).** `a_differentiated_iron_core_earth_settles_compresses_and_
stratifies` (N=3000, M=5.96e24 kg):
- **COMPRESSES, does not puff up** — settled mass-weighted RMS **3973 km** from 5709 km initial (the old
  equal-volume prototype blew up to 15,700 km; the equal-mass fix is decisive).
- **Stratified:** iron core (mean r 2326 km) stays inside the mantle (4591 km); core settled ρ **15,591**
  kg/m³ (compressed above iron's ρ₀=7850 — real inner core ~13,000), mantle **5534** (real lower mantle
  ~4400–5500). Core denser than mantle ✓.
- **Hydrostatic balance rel 6%** at r=1986 km.
- **Central pressure 572 GPa** vs Earth's real **364 GPa** (Wissing & Hobbs 2020) — same ORDER (~1.6×).
  Honest caveats: coarse N=3000, Tillotson iron over-compresses at high P (a known Tillotson limitation), and
  basalt ≠ the denser perovskite lower mantle — so order-correct, not exact.
Stage 2a (single-material) re-verified green after the refactor — adaptive-h tightened its balance to rel
0.00–0.01. EOS 6/6; full fast suite 151/151; wasm builds.

**Why.** docs/33 stage 2: a planet that is real matter can shed its own mantle into the disk — the
prerequisite for dissolving the rigid boundary (docs/28 #1, docs/31). The differentiated Earth is the object
the impact (stage 3) will hit. Still isothermal (u fixed); the adiabatic energy equation is stage 3.

---

## 2026-07-17 — Research note: sourced EOS data + the differentiated-body method fix (docs/33)

Verification dig for the layered-planet params/method (some primary tables are book-only — Melosh 1989
p.234 — and Robin's linked review is paywalled). What I could source from OPEN literature:

- **Iron Tillotson (compressed branch), Wissing & Hobbs 2020 (A&A 635 A21), refit to Brown et al. 2000
  shock data:** ρ₀=7850, A=128 GPa, B=181.5 GPa, a=0.5, b=1.28, E₀=14.25 MJ/kg. (Vapor-branch E_iv/E_cv/α/β
  NOT given there — still need the primary Melosh table for those; but the compressed branch is all a static
  planet needs.) My current `eos::iron` has A=128 GPa ✓ but b, B, E₀ differ from this refit — update pending.
- **Real Earth-layer structure, Wissing & Hobbs 2020 Table 1** (their PREM fit — a validation dataset for a
  layered particle Earth): inner core ρ₀=7744/B₀=166 GPa, outer core 6920/115, lower mantle 4121/231,
  transition 3622/160, asthenosphere 3380/130, crust 2300/100; M=5.97e24 kg, central P=**364.1 GPa**,
  T_c=5300 K. (A is ≈ the bulk modulus B₀, so these cross-check the Tillotson A values.)
- **Basalt Tillotson: VERIFIED, Benz & Asphaug 1999 Table 2** (exact match to `eos::basalt`).
- **Differentiated-body METHOD, confirmed from Genda et al. 2012 (the puff-up fix):** SPH particles all
  **equal mass**, placed on a **3D FCC lattice** (iron inside, rock outside), internal energy set to
  **1.0×10⁶ J/kg**, relaxed until velocities < 100 m/s. My equal-VOLUME/unequal-mass init was the bug.

Still blocked (needs the primary Melosh 1989 p.234 table or paywall access): full Tillotson sets (esp. the
vapor branch) for **granite, dunite, and iron**. Flagged provisional in `eos.rs`.

---

## 2026-07-17 — Honesty pass: EOS parameter provenance + stage-2b puff-up (docs/33)

**What.** Two honest corrections while extending stage 2 to a layered/differentiated planet (stage 2b):

1. **EOS parameter provenance.** Stage 1's tests verify only SELF-CONSISTENCY (cold P=0, K=A, continuity),
   NOT agreement with the literature — so a wrong-but-self-consistent parameter passes. I had written the
   Tillotson params from memory and labeled them "cited." Verified what I could: **BASALT matches Benz &
   Asphaug 1999 (Table 2) exactly** (ρ₀=2700, A=B=26.7 GPa, E₀=487, E_iv=4.72, E_cv=18.2 MJ/kg, α=β=5) —
   which is why stage 2a settled cleanly. GRANITE, DUNITE, IRON I could NOT verify online (papers cite
   Melosh 1989 p.234 but don't reproduce the table; PDFs weren't text-extractable), so `eos.rs` now flags
   them **PROVISIONAL — unverified against the primary table**. One confirmed fix: dunite ρ₀ 3500 → **3320**
   (Chau et al. 2018). No false "cited" claim stands.

2. **Stage 2b (differentiated iron-core + peridotite-mantle body) PUFFED UP** — RMS radius blew from 2000 km
   to ~15,700 km, mantle density collapsed to 507 kg/m³. The prototype's assertions were too weak and it
   FALSELY passed; I reverted it. Two likely causes, both flagged: (a) the equal-volume / **unequal-mass**
   SPH init corrupts density at the core–mantle interface — proper differentiated bodies need **equal-mass
   particles + adaptive smoothing length** (standard SPH); (b) a bad transcribed parameter (dunite `cap_b`
   is suspect). Deferred until both are resolved: verified params + equal-mass/adaptive-h init.

**Verified.** EOS self-consistency 6/6 still green after the dunite-ρ₀ correction; single-material stage 2a
(basalt, verified params) stands as the solid milestone. Stage 2b reverted, not shipped.

**Why.** No-fudge (docs/23): don't claim "cited" without verifying, and don't ship a test that passes on a
physically wrong (puffed-up) body. Recorded the real state rather than a green checkmark.

---

## 2026-07-17 — Realignment stage 2: a particle planet holds itself up (self-gravitating EOS body, docs/33)

**What.** Added `hydrostatic.rs` — a self-gravitating condensed-matter body that holds itself in hydrostatic
equilibrium as REAL MATTER (a cloud of particles), instead of the rigid analytic boundary the impact scene
uses (docs/28 root cause #1). It is the "merge" docs/32 §3 identified: it COMPOSES the shared kernels rather
than forking them — `eos::Tillotson` pressure (stage 1) + the one SPH kernel `atmosphere::sph_w/dw` +
`bhtree::BarnesHut` self-gravity. `HydroBody::new_sphere` fills a sphere with equal-mass particles at ρ₀,
each with `u=c·T`; `relax_step` settles it (damped) under self-gravity + the symmetric SPH-EOS pressure
force `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W` with `P=Tillotson(ρ,u)`. The only new physics is the condensed EOS; at
unification (docs/33 stage 5) this folds INTO `Aggregate` so a planet and its debris are one particle
system — for now it's a focused, independently-verified module (correctness-first).

**Verified (native, #[ignore], ~215 s).** `a_self_gravitating_eos_body_settles_into_hydrostatic_balance`:
a 1500 km basalt body (N=3000) relaxed under self-gravity + Tillotson pressure —
- **Stable:** settled RMS radius **1383 km**, spread **1.1%** over the last steps (no collapse/explosion).
- **Hydrostatic balance pointwise:** dP/dr vs −ρ(r)·g(r) [g=G·M(<r)/r² from the enclosed particle mass] —
  at r=484 km, −902 vs −1081 (17%); at r=761 km, **−1660 vs −1617 (3%)** — right sign, within SPH operator
  tolerance (cf. atmosphere.rs's 3D balance at ~35%).
- **Central pressure 2.29 GPa** vs the uniform-density self-gravity estimate 3.17 GPa — same order, a real
  planet pressure.
Full fast suite 151/151; wasm builds. Isothermal (u fixed) this stage — the adiabatic energy equation
under compression is the stage 2b/3 refinement. Not yet in a scene.

**Why.** The prerequisite for dissolving the rigid boundary (docs/28 #1, docs/31): a planet that is real
matter can shed its own mantle into the disk. Proves the merge works before touching the tested `Aggregate`.

---

## 2026-07-17 — Realignment stage 1: the Tillotson condensed-matter EOS (docs/33)

**What.** Added `eos.rs` — the **Tillotson equation of state**, `P(ρ, u)` for condensed matter across cold /
shock-compressed / decompressed / vapor states in one closure (the giant-impact standard: Tillotson 1962;
Melosh 1989 App. II; Benz, Cameron & Melosh 1989). This is the missing physics docs/32 §5 flagged: solids
previously resisted compression only via a linear-elastic contact penalty (E·r/m) and planet densities were
declared constants, so shock-compressed rock had no way to develop pressure from its density. `Tillotson`
carries the cited parameters for **granite, basalt, peridotite (dunite/olivine analog), and iron**;
`pressure(ρ,u)`, `sound_speed_sq(ρ,u)` (central-difference, for CFL + bulk-modulus readout), and
`for_material(name)` lookup. Params live in `eos.rs` for now; migrating them into `data/materials.json` (a
`tillotson` block beside `thermal`) is the flagged source-of-truth follow-up (docs/04).

**Why.** The keystone of the realignment (docs/33): ONE pressure law spanning solid→liquid→vapor, replacing
the ideal-gas-vapor + linear-elastic-penalty + declared-density patchwork. The SPH pressure-force machinery
(`aggregate`/`atmosphere`, `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W`) is untouched — only the `P(ρ,u)` it evaluates
changes — which is why a self-gravitating condensed-matter planet (stage 2) is a merge, not new machinery.

**Verified (native, TDD — 6 tests).** `cold_reference_state_has_zero_pressure` (P(ρ₀,0)≈0);
`cold_compression_gives_the_bulk_modulus` (K=ρ·dP/dρ at ρ₀ matches each material's A within 2% — a REAL
bulk modulus, not a contact-spring surrogate); `compression_monotonically_raises_pressure` (stiffens to GPa
scale — the impact regime); `hot_expansion_relaxes_toward_vanishing_pressure` (fully-vaporized expanded
parcel → the ideal-gas limit a·ρu); `pressure_is_continuous_across_the_vaporization_boundaries` (no jump at
E_iv/E_cv); `sound_speed_is_real_and_of_the_expected_order` (c≈√(A/ρ₀), km/s). Full fast suite 151/151; wasm
builds. Not yet wired into any scene (stage 2 builds the self-gravitating planet on it) — nothing to
rig-watch/deploy yet.

---

## 2026-07-17 — Architecture map + first-principles realignment plan (docs/32, docs/33, CLAUDE.md)

**What.** Mapped the whole engine and wrote it up for future Claude sessions (Robin: too many "surprises"
about what already exists). Four parallel readers covered the physics core, terrain/atmosphere, scene/render/
GPU, and docs/build/deploy; synthesized into **docs/32-architecture-map.md** (module-by-module with
`file:line` anchors, the shared-laws-vs-forked-solvers map, the EOS inventory, the birth-of-the-Moon scene
trace, and the workflow rules), a concise auto-loaded **CLAUDE.md** pointing to it, and
**docs/33-architecture-realignment.md** — a staged plan to realign the architecture to Integrity's
principles (Robin's three framings: material physics scalable · calculations tiered on energy scale ·
everything a natural product of the real physics).

**Key finding.** The physics *laws* are already unified and scale-invariant (`granular::Contact`, the SPH
kernel, `Furrow` excavation, `plough_loft`, `Body`, `LayeredBody`); the *solvers and containers* are FORKED
— two container universes (CPU `Aggregate` f64 vs voxel-`World`/GPU f32), four integrators over one law, the
rigid-boundary fork (Earth is a penalty sphere, not particles — docs/28 #1), and **no condensed-matter EOS**
(solids resist via a linear-elastic contact penalty; planet densities are declared constants). A
self-gravitating EOS planet turns out to be a MERGE, not new machinery: `atmosphere.rs`'s verified SPH
pressure kernel + `bhtree.rs` self-gravity + `aggregate.rs::apply_thermo` energy equation, with the ideal-gas
EOS swapped for a Tillotson EOS — only the EOS is genuinely new.

**The realignment (docs/33).** One particle/material engine every scene drives: one container (bulk forms
are the coarse *energy tier* of the same particles, not a separate universe), one pressure law (Tillotson EOS
spanning solid→liquid→vapor, replacing the ideal-gas + linear-elastic + declared-density patchwork), one
energy-tiered stepper (fidelity T0 bulk → T1 quasi-static → T2 granular+thermal → T3 full EOS shock/vapor,
selected by energy density vs the material's own thresholds — generalizing docs/08/13 spatial LOD to
energy-tiered physics via the docs/16 awake-set). Staged correctness-first: (1) Tillotson EOS module +
tests, (2) self-gravitating EOS planet vs planet.rs's analytic hydrostatic profile, (3) two-body impact both
bodies as particles → re-measure the isotopic crisis, (4) GPU-resident unified stepper at N~10⁵, (5) unify
the containers, (6) formalize the energy-tiered awake-set. Full-particle-Earth is milestones 2–3.

**Why.** Robin's directives: all particle physics in ONE scale-invariant module; build the hard correct
physics first (GPU/full-res if needed), optimize physics-faithfully later; everything a natural product of
the real physics. The map stops the rediscovery; the plan makes the full-particle-Earth build the forcing
function of the realignment rather than a side quest.

**Verified.** Docs only — no code change. Existing suite unaffected.

---

## 2026-07-16 — The isotopic crisis: physics says proto-Earth spin is NOT the lever (docs/31)

**What.** Opened the isotopic crisis (docs/31, "Option C"): the canonical impact makes a **Theia-dominated**
disk, but the real Moon is isotopically Earth-like. Tested **Ćuk & Stewart (2012)'s** proposed resolution —
a *fast-spinning* proto-Earth flings its own mantle into the disk. Implemented proto-Earth spin honestly:
the excavated Earth cap is surface mantle that was **co-rotating before the impact**, so each `SOURCE_TARGET`
grain is now born with `ω × (pos − centre)` (added in `build_impact_debris_scaled` before the ploughing
loft, so the momentum exchange acts on the real pre-impact velocity; `earth_omega = 0` is byte-identical to
before). Scene wired: `lib.rs` converts `spin_l → ω = L/I` (solid sphere) and passes it, default **zero**
(unknown IC, flagged) — nothing changes on screen; the plumbing just lets a spin be *explored*.

**MEASURED (physics deciding against the hypothesis).** `a_fast_spinning_protoearth_makes_the_disk_earth_
derived` (#[ignore], N=256+512, 3000×2 s), non-spinning vs a 2.3 h-day proto-Earth (ω·R ≈ 4835 m/s):
- ω=0    : Earth **0.162** | Theia 1.241 M☾ → disk is **12 % Earth**
- ω=fast : Earth **0.181** | Theia 2.412 M☾ → disk is **7 % Earth**

A fast spin lofts *slightly* more Earth material (0.162→0.181) and injects a lot of angular momentum, so the
whole bound disk grows (1.40→2.59 M☾) — but it retains proportionally **more Theia**, so the Earth *fraction*
FALLS, 12 %→7 %. **Spinning the target does not resolve the crisis in our model.**

**Why — and the real lever.** Direct consequence of docs/28 root cause #1: **Earth is a rigid boundary**, so
the only Earth material that can reach the disk is the small excavated cap. The actual Ćuk & Stewart
mechanism is a spinning proto-Earth shedding its **bulk mantle** — which a rigid analytic sphere cannot do.
So 7 % is a LOWER BOUND the rigid boundary imposes, and adding spin only speeds up the material that *is*
free to move (overwhelmingly Theia). The honest resolution needs **Earth-as-deformable-matter** (docs/28 #1)
or **vapor-phase Earth↔Theia mixing** (now partly reachable via the SPH vapor field, docs/26/27) — NOT
target spin. Documented in docs/31 with the next experiments.

**Why.** No-fudge (docs/23): we set a physical initial condition (spin) and let the disk provenance EMERGE;
when it emerged *against* the hypothesis we recorded that, and the test now asserts only the robust mechanics
(spin ⇒ larger bound disk) plus the measured ceiling (fraction does not rise), printing the provenance split.

**Verified (native).** Full fast suite 145/145; the measurement test green with the corrected (measured)
assertions; wasm builds; scene byte-unchanged at the default zero spin.

---

## 2026-07-16 — The accelerated compute module: neighbour grid + Barnes–Hut + block timesteps (docs/30)

**What.** Built the reusable **accelerated particle compute module** (docs/30) so the impact disk can run
at high N without the O(N²) wall — a general substrate (any particle system: weather, clouds, fluids), not
an impact special-case. Four stages, each its own crate/module with a brute-force fallback below a size
threshold and a test that pins it to the exact/near-exact reference:

- **Stage 1a/1b — neighbour grid** (`neighbors.rs`). A spatial-hash `NeighborGrid::build(pos, cell)` +
  `for_each_pair` that finds every short-range pair in O(N) instead of O(N²), then wired into the contact
  and SPH density/pressure loops (one `sr_grid` built per step from shared `sr_pos`/`masses`). Brute-force
  below 512 bodies. Test: `grid_finds_exactly_the_brute_force_pairs` (exact — the grid is not an
  approximation).
- **Stage 1c — Barnes–Hut self-gravity** (`bhtree.rs`). An octree caching per-node COM+mass; a particle
  uses a node as ONE source when its angular size `(2·half)/dist < θ` (θ=0.5), turning O(N²) self-gravity
  into O(N log N). Same Plummer softening as the direct sum — the same physics, grouped. Test:
  `barnes_hut_matches_brute_force_within_theta_bound` (RMS < 1% at θ=0.5; θ→0 recovers brute force to 1e-9).
- **Stage 3 — block timesteps** (`aggregate.rs`). A per-particle timestep criterion (`particle_timesteps`:
  √(ε/|a|) free-fall, capped by the |v|/|a| turnaround), then a hierarchical **block KDK** integrator
  (`step_block`): power-of-two rungs, the quiescent disk coasts while the shocked/vapor core sub-steps.
  The subset-force pass (`accelerations_masked` + `BarnesHut::accelerations_active`) recomputes gravity
  only for the bodies being kicked this sub-step — O(N_active log N). Thermo (PdV cooling, radiation,
  phase flip, dissipation heating) was extracted into `apply_thermo` and now runs each sub-step, so
  `step_block` is a faithful full-physics drop-in for `step()`. Wired into the space scene.

Also this pass: the impact scene now runs at **high N (512 debris + 1024 cap)** with the cap-mass fix
restored (`cap_mass` summed from the real per-grain target masses, not the `moon_mass·CAP_N/DEBRIS_N`
bookkeeping that the 07-15 entry flagged as ≈6.5× high); and two **watching** tools so the agent can see
what Robin sees — `rig/birth_shot.mjs` (headless-Chromium screenshots of birth.html at timed marks) and a
"📷 Share view" button on the space band that POSTs the live canvas.

**Why.** docs/30: temporal + spatial coherence is the "MPEG for physics" — most of the cloud barely moves
per step (the block scheduler's coasting rungs are the delta-frames; the grid/tree are the spatial
compression). Getting the disk to lunar-mass resolution needs O(N log N), and the module has to be generic
because the same substrate runs every future particle system. No-fudge (docs/23): every accelerator is
proven against its exact/θ-bounded reference, so speed never changes the answer.

**Verified (native).** Full suite green; `grid_finds_exactly_the_brute_force_pairs`,
`barnes_hut_matches_brute_force_within_theta_bound`, `contact_grid_matches_brute_force`,
`particle_timesteps_shrink_with_acceleration`, `step_block_conserves_energy_and_matches_global_dt`, and
— the decisive one — `birth_impact_with_step_block_reproduces_the_disk`: the REAL coupled impact gives
**global step() 0.772 M☾ vs block step_block 0.788 M☾** (matches). `step_block_speedup_bench` measures
**5.5× faster** on an aftermath-shape cloud (1330 ms → 241 ms). On-screen: deployed to
integrity.bothead.net (build 20260716.081104) and rig-watched — the disk forms and evolves identically to
the global integrator (T+24m: 2.44 M☾ in 42 accreting moonlets, Earth-origin material aloft), no regression.

---

## 2026-07-15 — Vapor gets a real pressure field: SPH + a latent-heat reservoir (docs/26/27, docs/28 item 5)

**What.** Replaced the vapor "overlap hack" with a real **SPH pressure field** so the impact-generated
vapor expands and cools as a gas from first principles, not a scripted push. `aggregate.rs`: a cubic-spline
kernel gives each vapor particle a density ρ=Σm_jW(r,h); pressure P=ρ·R_s·T; a symmetric,
momentum-conserving pressure force; and a PdV energy equation so expansion does real work and the gas
cools itself. Then a **latent-heat reservoir** (docs/28): the pressure reads the *thermal* temperature
`T − L_v/c`, so the energy locked in the vaporization latent heat is not double-counted as pressure — the
vapor holds heat honestly on the phase boundary instead of over-puffing. Also shipped the
`disk_orbit_vs_resolution` diagnostic sweep (the disk grows toward lunar mass with N: 0.77→1.27→1.41 M☾ at
N=384/768/1536).

**Why.** docs/26/27: the atmosphere/vapor must be *matter under its own pressure*, not a visual. The old
overlap repulsion was a fudge (docs/23); SPH is the honest continuum form, and the latent-heat correction
keeps the first law intact across the solid↔vapor phase change (docs/28 item 5).

**Verified (native).** `vapor_sph_expands_and_cools_conserving_energy` — a hot vapor ball expands under
its own pressure and self-cools (80k → 18.5k K), total energy conserved to within drift; the latent-heat
fix dropped a spurious vapor↔vapor dissipation heating that had inflated both temperature and disk mass
(disk 0.066 → 0.132 M☾, peak T 52k → 18.5k K — honest physics over the bigger-but-wrong number). Full
suite green.

---

## 2026-07-15 — The Moon becomes Earth-derived: a momentum-conserving loft breaks the 0.000 deficit

**What.** Closed docs/28 step 3. Earth (target) material now LOFTS into the bound proto-lunar disk —
**Earth 0.083 M☾ | Theia 0.551 M☾** aloft, where it had measured a dead **0.000 M☾ Earth** at every
resolution (the "nothing is taken from Earth" deficit). The Moon is now genuinely Earth-derived, as the
isotopes demand — and it emerged from conserved mechanics, no dial. Two coupled fixes:

- **Physical cap mass (docs/28 item 4).** The excavated cap was materialized at a bookkeeping **2× the
  impactor** mass; it is now real **ρ·V** — each grain an equal slice of the furrow's half-ellipsoid volume
  times the LOCAL density at its depth (≈ 0.31× the impactor). `furrow_target_grains` sets it; the energy
  cap and per-grain contact use each grain's real mass.
- **A momentum-conserving loft in the SHARED particle physics** (`granular::plough_loft`, not the impact
  builder — Robin: "added to global particle physics"). When a fast body ploughs slower target matter, the
  along-track (tangential) momentum is shared inelastically toward the impactor↔cap **centre-of-mass**
  velocity — the physical maximum drag, no free dial — and what the cap gains the impactor loses, so
  Σ(m·v) is **exactly** conserved. Only the along-track component is touched (radial rebound + gravity keep
  theirs). This is the same reverted "COM drag" from 2026-07-14 that made it WORSE — the ONLY thing that
  changed is the cap mass: at the fudged 2× the COM speed collapsed to v_t/3 (sub-orbital, gutted the
  disk); at the physical 0.31× it is ~0.76·v_t ≈ near-orbital, so Earth material joins the disk while the
  impactor barely slows. The cap-mass fudge, not the mechanism, was the blocker all along.

One law for every band: a terrain meteor and a giant impact both loft their excavated matter through
`plough_loft` (space-band wired now; terrain wiring is a flagged follow-up).

**Why.** docs/23/24 no-fudge: the loft is real ploughing momentum, declared HONESTLY as a conserved
transfer (the µs shock is sub-resolution at any N — docs/24 #1), never a scripted velocity.

**Verified (native).** `plough_loft_conserves_momentum_and_lofts_the_lighter_target` (Σ tangential p
unchanged; cap dragged up, impactor slowed; radial untouched; vertical = no-op). **Full suite 144/144** —
every disk guardrail (birth peak-aloft > 0.3 M☾, emergent day 2–14 h, theia) still holds, so the honest
mass + loft did not detune the disk. On-screen rig-watch (birth.html) is the remaining check — pending
Robin's eyes / a rig in this env. FOLLOW-UPS (flagged, not papered over): the lib.rs interactive-scene
mass bookkeeping (`cap_mass = moon_mass·CAP_N/DEBRIS_N`, now ≈6.5× high) and terrain-band `plough_loft`
wiring.

---

## 2026-07-14 — Measured: "raise N" does NOT loft Earth material (the disk deficit is a mechanism, not a resolution, problem)

**What.** Investigated docs/28 step 3 (progressive excavation) — why the proto-lunar disk is ~100%
impactor ("nothing is taken from Earth"). Made the impact resolution a real knob
(`impact::build_impact_debris_scaled(.., debris_n, cap_n)`; the const `build_impact_debris_between`
delegates at the default 128/256) and added two `#[ignore]` measurement sweeps
(`disk_provenance_vs_resolution_sweep`, `disk_provenance_emergence_no_declared_ejection`). Then MEASURED
the bound-aloft disk composition across N — the honest test of the "raise N globally" hypothesis.

**Why.** Before spending the O(n²)→tree perf work that a global N increase would require, prove that more
resolution actually lofts Earth-derived material. It does not.

**Verified (measured, native).** Bound-aloft mass by provenance (M☾), 3000×2 s aftermath:
- Declared ejection ON: N=384/768/1536 → **Earth 0.000 / 0.000 / 0.000**; Theia 0.69 / 0.35 / 0.72
  (the Theia disk mass does not even converge — it is relaxation-noise-limited, the docs/28 collisionless
  ceiling, not resolution-starved).
- Declared ejection OFF (cap AT REST, contact ploughing must do the lofting): N=384/1536 →
  **Earth 0.000 / 0.000**; Theia 0.69 / 0.84.

**Earth material lofts in NONE of the six configurations.** The cause is provable and N-INDEPENDENT: a
grain launched from the surface needs a near-tangential speed ≥ the ~7.9 km/s circular velocity to hold a
perigee above the surface. The declared `Furrow::ejection` gives ~5.9 km/s at ~45° (horizontal ≈ 4.1
km/s) — sub-orbital, so every cap grain re-impacts, at any N. With the ejection OFF, contact ploughing
drives the resting cap DOWN and downrange into the planet, not up — the shock-driven excavation flow that
would loft it is sub-resolution at any feasible N (docs/24 problem #1), so it never emerges. **Conclusion:
the Earth-lofting deficit is a MISSING MECHANISM, not a resolution shortfall; "raise N globally" is not
the lever.** A separate dead end confirmed and reverted en route: a momentum-conserving "ploughing drag"
(impactor drags cap downrange toward the COM tangential velocity) makes it WORSE (both → 0.000) — full
inelastic sharing drops the impactor to v_t/3 and guts its own disk, and the cap only reaches ~2.2 km/s,
still sub-orbital.

**The real levers (for the next session / Robin's steer), all no-fudge (docs/23, docs/24):**
1. **Materials-honest contact.** Theia's *construction* is layered (iron core + peridotite mantle, as
   theorized), but its collision *physics* is bulk **basalt** for every grain (restitution 0.40, basalt
   density for grain radius, equal grain mass). That basalt restitution IS the disk's damping law. The
   aggregate contact already carries per-grain `mat_ids` and is momentum-conserving for ANY mass ratio
   (equal-and-opposite forces ÷ each own mass) — so per-grain real material + real ρ·V mass is viable at
   full resolution; it just needs the contact loop to read `mat_ids`. This also fixes docs/28 item 4 (the
   cap is ~6.5× over-massed: 2× impactor vs the physical ρ·V furrow ≈ 0.31× impactor).
2. **The docs/24 emergence subsystem** — deposit the impactor's momentum/energy as real compression so
   REBOUND lofts material (delete the declared `Furrow::ejection`). Since the µs shock is sub-resolution
   at any N, the honest form is a momentum-CONSERVING loft that gives near-track excavated Earth material
   *near-orbital tangential* velocity from the impactor's momentum (not the radial 45° script) — the
   corrected version of the reverted drag, unblocked once the cap mass is physical (item 1).

**Shipped this pass:** the N knob + the two reproducible measurement sweeps (all 136 native tests green;
the sweeps are `--ignored`, O(n²)). No physics claim shipped — the finding is the deliverable. On-device
rig-watch not required (nothing visual changed). NOTE: the Jul 12 render-truth fixes and the Jul 13
terrain-contact/furrow commits are still un-journaled — a catch-up entry is outstanding.

---

## 2026-07-11 — The engine watches itself: the rig, the profiler, and a 7× frame

**What.** The agent now verifies scenes with its own eyes before shipping them (Robin: "simulate
locally and watch — we've been through a lot of iterations you could have seen going wrong"). The watch
rig (`web/rig`): headed Chromium under xvfb (headless cannot composite WebGPU swapchains — the first
attempts photographed a blind rig, not a broken app), timed screenshots, a frame profiler, an fps probe.
First session of use, in order: proved the scenes render correctly; caught a post-impact DEATH SPIRAL
(one slow frame → 0.25 s backlog → 128 O(n²) substeps → slower still, pinned at 1 fps); profiled
advance() at 161 ms vs render() at 3 ms; and found the real culprit — `powf(-1.5)` libm calls per
gravity pair. Hardware sqrt: **161 → 22 ms/frame (7×)**; the native suite dropped 133 → 52 s too.
Substep budgeting ends the spiral (observable time dilates; the frame stays interactive). Camera opens
on the sun side (the night side is honestly black now) and rides the BOUND debris extent (escapees no
longer drag the view out to pixels). Watched verdict at T+13h aftermath: 32 fps, 354 → 62 fragments as
settled matter demotes into Earth, disk 0.48 M☾ in 3 moonlets — the on-screen numbers now match the
native emergence tests.

**Verified.** By watching. 91/91 native; profiler numbers above.

---

## 2026-07-11 — The Birth of the Moon: the SCENE (docs/27)

**What.** The proven giant-impact physics, now watchable: a new scene (**Birth of the Moon** in the scene
picker) opens ~5 real seconds before the strike at the close framing (25% of lunar distance), with a HUD
countdown that IS the simulation's own forecast (distance / closing speed from the live N-body state —
the same conservation-law machinery, read as a clock). Theia arrives with a real IMPACT PARAMETER
(0.87 of the contact radius at 6 km/s from quarter-lunar range), so the ~45° obliquity of the hypothesis
EMERGES from geometry + gravity at contact — never aimed. At the strike, both bodies materialize (Theia's
iron core + hot mantle; Earth's crust/mantle/outer-core cap), and the camera rides OUT with the ejecta —
view distance tracks the debris extent — to the wide whole-orbit framing, watching the lofted, bound,
perigee-raised material (0.55 M_moon in the native test) circularize into the proto-lunar disk. Replay
re-runs the encounter.

Also, for ALL impact scenes (Robin): a **T+ aftermath clock in SIM time** (y/mo/d/h/m/s at the scale the
number deserves) — the honest answer to "what timeframe are we watching?", since time-LOD means wall
time ≠ world time; and the pre-impact countdown for the birth scene. The impactor is now a first-class
parameter of the space band (radius/mass/profile drive CCD, excavation, rendering, materialization), so
the moon-drop is just one configuration of the same scene machinery.

**Verified.** 87/87 native (the physics is the previous entry's test); wasm + TypeScript build clean.
The choreography needs on-device eyes.

---

## 2026-07-11 — THE ANTITHESIS: the birth of the Moon (docs/27)

**What.** Robin: *"a mass impacted the earth and ejected the material that became the moon — I'd like to
see that happen. If it works, we can prove our system works."* The proof, as a passing native test: the
SAME impact machinery that shatters a falling Moon, run in reverse role — a Mars-sized differentiated
impactor (**Theia**: iron core + peridotite mantle, ~6.5e23 kg, declared like every other body) strikes
Earth **obliquely** at the mutual escape speed (~9.5 km/s; obliquity is what puts mantle on lofted
trajectories with angular momentum instead of straight up). Kepler alone would return every launched
fragment to its launch radius — it is debris-debris CONTACT and SELF-GRAVITY, already in the model, that
must raise perigees into orbit. Integrating the aftermath: **0.55 lunar masses of material ends up aloft,
bound, and perigee-raised above the surface — genuinely orbiting** (the theorized proto-lunar disk is
1–2 M_moon; 0.55 at 192-particle resolution is the right scale), while only 0.14 M_moon escapes. The
Moon-forming reservoir emerges from the declared bodies and the one contact law. Nothing was scripted;
the machinery was not told what a "disk" is.

Also: `build_impact_debris` generalized to ANY impactor/target pair of layered bodies (the moon-drop
scene is now just one parameterization), Theia added to the planet profiles, giant-impactor excavation
clamped to a hemispheric scale (flagged approximation). The interactive birth-of-the-Moon SCENE (5 s HUD
countdown, camera riding the ejecta out to watch the Moon form) is the next build on this physics.

**Verified.** `an_oblique_theia_impact_lofts_bound_material_the_protolunar_disk`; 87/87 native.

---

## 2026-07-11 — The exponential atmosphere EMERGES (docs/26 tests 1+2)

**What.** Air is now dynamic matter (`atmosphere.rs`): gas parcels whose resistance to compression is
their EQUATION OF STATE (ideal gas — the 1D column force is exactly F = A·ρ·R_s·T per slab), never an
elastic modulus. THE emergence result: a column of 200 equal-mass air slabs under gravity, started from
a deliberately WRONG exponential (2× the real scale height), relaxes to the real isothermal atmosphere —
**measured H = 8,446 m vs the analytic R_s·T/g = 8,427 m (0.2%)** — proving the profile is an attractor
of the physics, not an initial condition. And the settled column's basal pressure equals its weight
(100,266 vs 101,357 Pa — one real atmosphere from one real declared column mass): the docs/25 static
boundary condition is provably this dynamic model's limit. Also: `gas_contact_from_material` (K = γ·P
stiffness for the canonical contact law), R_s = 287 J/(kg·K) from the declared molar mass, and
free-expansion-in-vacuum (gas never clumps). Flagged next: the 3D SPH kernel density (the column is the
honest first resolvable case), then drag + entry glow (docs/26 tests 4–5).

**Verified.** `a_settling_air_column_finds_the_real_exponential_atmosphere` + 2 more; 83/83 native.

---

## 2026-07-11 — Every solid object is matter: the Moon gets the same treatment

**What.** Two representation asymmetries closed (Robin: "Every solid object in the universe is composed
of matter"): (1) the intact **Moon now renders as a grain shell** — its basalt crust at its measured
reflectance — exactly like Earth; no more smooth-sphere summary on one body and honest grains on the
other. (2) **Moon-vs-moon collisions use the same primitives as moon-vs-Earth**: swept CCD on the
pre-step relative path, the true contact state from the conservation laws (vis-viva + angular momentum),
an inelastic momentum-conserving merge at the contact configuration, and the dissipated energy
accounted. Nothing special-cases Earth anymore. Flagged next: materializing a moon-moon impact cloud is
the same `build_impact_debris` with the target's layered profile parameterized (today it samples Earth's
profile for the target and the Moon's for the impactor).

**Verified.** 80/80 native; wasm builds.

---

## 2026-07-11 — Physics/render decoupling: the simulation runs the world; the render just looks at it

**What.** The space band's physics no longer lives inside `render()`. The new architecture (docs/13 made
real):
- **`advance(real_dt)`** drives the PHYSICS from wall-clock time in fixed sim-timestep substeps whose
  COUNT (never size) varies with the elapsed real time. The physics rate is now independent of the
  display frame rate — a 30 fps client simulates the same world as a 120 fps one (previously the sim
  assumed 60 fps and ran half-speed at 30). Under overload the observable clock dilates (backlog is
  dropped) rather than corrupting the physics with an oversized step: time slows before truth breaks.
- **The renderer samples snapshots ~100 ms BEHIND the physics** (Robin: humans can't catch detail under
  1/10 s, so use that budget). Every event the render draws is already fully resolved — a collision can
  never be caught mid-lie by a frame boundary, structurally: the fly-past class of bugs is now
  impossible rather than patched. Snapshot interpolation gives smooth motion at any frame rate; the
  crater/shatter appear exactly when the RENDERED clock crosses the shatter instant.
- Physics is never triggered by, or dependent on, the visualization — it drives it (Robin's
  architectural invariant, verbatim).

Also fixed from Robin's render read: **"hollow earth"** — through the crater you could see the far side
of the crust from inside. The planet isn't hollow: the un-materialized bulk (physically the boundary +
gravity source) now renders as an opaque interior sphere at the depth the crater exposes — the top of
the outer core, self-lit at its REAL temperature from the layer profile. Through the hole you now see
glowing molten interior, honestly.

**Verified.** 79/79 native; wasm builds. Frame-rate independence and the lag are structural (wall-clock
in, snapshots out); on-device read pending.

---

## 2026-07-11 — The atmosphere's weight keeps the oceans liquid (docs/25)

**What.** Earth now declares only the MEASURED MASS of its atmosphere (5.15e18 kg); the surface pressure
emerges as that column's weight — ≈1 atm, never assigned. Materials gained Clausius–Clapeyron BOILING
curves (latent heat + molar mass, `thermal.molar_mass`) beside their Simon melting curves, and the phase
decision (`planet::surface_phase`) now covers solid/liquid/vapor. The consequences, all as passing tests:
288 K water under the emergent 1 atm is LIQUID; the same water in vacuum flashes to VAPOR at any
temperature (below the ~611 Pa triple point liquid has no regime — Robin: "water exposed to vacuum would
be wild", and the model now says exactly that); cold water freezes; water boils at ~366 K at 0.7 atm
(mountain physics for free). The airless Moon ⇒ no lunar seas, as observed. A failing test caught real
physics along the way: Earth's inner core briefly classified as "Vapor" because iron's boiling point was
a flat 1-atm fallback — pressure suppresses boiling even harder than melting, and with iron's real molar
mass boiling is COMPLETELY suppressed at 360 GPa (the fallback was the dishonesty).

**Why.** Same pattern as the molten core: declare real composition (now including the air), compute the
consequences. Also fixes the record on ocean colour: water renders with its measured near-black
reflectance — the "blue marble" is atmospheric Rayleigh scattering, which we refuse to paint. The
atmosphere today is a static boundary condition (pressure/phase); making it MATTER — drag, entry plasma,
Rayleigh blue, blast waves, evaporation cycling — is now the flagged next major milestone (docs/25
roadmap).

**Verified.** `the_declared_atmosphere_mass_weighs_in_at_one_atmosphere`,
`liquid_oceans_exist_under_an_atmosphere_and_boil_off_in_vacuum`; 79/79 native, wasm builds.

---

## 2026-07-11 — Layered planets: the molten core EMERGES from pressure (docs/25)

**What.** Planets are now DECLARED as their real construction and nothing else: concentric layers of real
materials (Earth: iron inner/outer core, peridotite mantle, basalt crust — PREM densities; Moon: small
iron core, peridotite mantle, basalt crust) with the observed geotherm as declared data. Everything else
is COMPUTED:
- **Gravity g(r)** — Gauss's law over the enclosed layer mass (peaks at the core boundary, zero at centre).
- **Pressure P(r)** — hydrostatic equilibrium integrated from the surface. Earth's centre comes out
  **≈360 GPa** (real: 364) and the core–mantle boundary ≈135 GPa from the declared densities alone.
- **PHASE** — each material got a pressure-dependent melting curve (Simon–Glatzel, published fits, new
  `thermal.simon_a/simon_c` in the materials DB). Phase = local temperature vs T_m(P). **Never assigned.**

**The emergence result (Robin's challenge: "that should be a natural artefact of gravity/mass/material
if we didn't fudge the composition"):** Earth's inner core comes out **SOLID** even though it is HOTTER
than the molten outer core — because the computed pressure pushes iron's melting curve above the
geotherm exactly there — while the outer core comes out **MOLTEN** and the mantle solid. The melt curve
crosses the temperature profile at the real inner-core boundary. Also: the declared layer densities
integrate to Earth's real mass and 9.8 m/s² surface gravity; the Moon's outer core comes out molten at
lunar pressures (flagged: the real lunar core is Fe–S, which melts lower than our pure-iron entry — we
use the upper published selenotherm; an Fe–S material is the refinement).

**Wired into the impact:** the materialized clouds now sample the layered bodies — each particle knows
its material (basalt crust / peridotite mantle / iron core) and its REAL internal temperature, so
excavating deep matter exposes rock and iron that glows because it genuinely is that hot. Earth's cap
reaches the top of the molten outer core (a Moon-scale impact digs that deep). Each fragment renders in
its own material's reflectance — the excavated composition is visible.

**Continents & oceans:** the render shell samples a 10°×10° land/ocean mask matched to the ~9° grain
spacing ("average area particles") — granite continents, water oceans, real reflectances. Honesty flags:
the hand-digitized mask over-represents land (~37% vs the real 29% — a cited dataset is the refinement);
ocean depth (~3.7 km) is far below one grain, so at this LOD water is the material of a grain's surface,
not a resolved layer; no planetary rotation yet, so the mask's orientation is arbitrary but consistent.

**Verified.** New `planet.rs` tests: declared composition → real mass + surface gravity; hydrostatics →
real central/CMB pressures; **molten-outer/solid-inner core emergence**; lunar molten outer core + solid
mantle; landmask places the major continents/oceans with a plausible area-weighted land fraction.
77/77 native, wasm builds.

---

## 2026-07-11 — Gauss interior gravity, emergent incandescence, and Earth rendered as matter

**What.** Three fixes, each traced from an on-device observation to missing physics (never a visual patch):
1. **Interior gravity obeys Gauss's law.** Debris that ploughed beneath the surface was sucked into the
   core ("the balls absorb into the centre") because the point-mass 1/r² is only valid OUTSIDE a planet.
   Inside, only the enclosed mass pulls: g(r) = GM·r/R³, linear to ZERO at the centre. The gravity source
   is now an extended body (1/r² outside its radius, Gauss interior inside) — no singular attractor.
2. **Incandescence is emergent — the hand-deposit pipeline is GONE from the planetary impact.** The
   impactor's fragments now simply CARRY the true contact velocity (they are the arriving body); the one
   contact law transfers momentum into the target's materialized matter, and the contact DISSIPATION
   (damping + friction) is routed into temperature (`granular::contact_dissipation` — energy is conserved,
   not destroyed, docs/20). A hard impact glows because the matter genuinely got hot. Measured: the cloud
   goes 83% → **100% gravitationally bound** through the collision, hottest fragment ~41,600 K (flagged:
   melt/vaporization energy sinks are not yet modelled at this scale, so the peak overshoots — the glow
   is real physics, the exact peak is not yet).
3. **Earth renders as its matter.** A smooth sphere is a representation lie once matter can be excavated —
   it hides the damage. Earth now draws as a shell of ~512 coarse grains (the honest low-res look);
   grains inside the materialized impact region are hidden, so the excavated void IS the crater and the
   glowing cap particles are the matter that used to fill it. Reset now un-shatters properly. Cosmetic
   skinning (an elastic surface over the blocks) is deferred until after the physics visuals are right.

Also: descent-follow camera (pure camera work — reads `moon_distance_km` from the N-body state, starts on
the whole-orbit framing, glides to a close-up at 25% of lunar distance as the Moon falls; manual zoom
overrides, Drop/Reset re-engage).

**Why.** Robin: "Do not bandaid visuals — fix the physics and then visualize them." Each visual wrongness
was a physics gap: no interior Gauss law, dissipation not becoming heat, a summary representation
(the sphere) hiding real state.

**Verified.** `interior_gravity_follows_gauss_law_not_a_point_singularity` (half depth ⇒ half g; centre
pulls ~nothing; exterior unchanged); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(100% bound, hottest ≫ visible-glow threshold — incandescence emergent); 72/72 native, wasm builds.
On-device: Robin confirms the impact now reads correctly ("Much better!").

---

## 2026-07-11 — ONE collision law for all matter + the mutual impact + conservation-law contact state

**What.** Three connected pieces, closing out Robin's "this must define ALL collisions of ALL matter":
1. **The canonical contact law now governs aggregates too.** `granular::contact_accel` (spring + damping +
   Coulomb friction + cohesion — the physics of record for terrain grains and GPU debris) is now the
   contact force inside `Aggregate` as well; the new `granular::contact_from_material` is the ONE mapping
   from a real material (Young's modulus → stiffness, restitution → damping, friction, cohesion) to
   contact behaviour, used everywhere. Aggregate particles previously had gravity and bonds but NO
   contact — they interpenetrated freely, which is why the shattered Moon was an "exploding sphere in a
   vacuum". A surface velocity rule I'd added to compensate ("cancel the inward component") was a fudge
   (Robin caught it) — deleted; the bulk planet is now a conservative penalty boundary (a force, −∇U).
2. **The mutual impact (`impact.rs`).** At the strike we materialize BOTH bodies at the interface — the
   Moon as a rubble ball on the surface AND Earth's impact region as a cap of crust (same grain mass) —
   and deposit the Moon's real momentum + energy into the *combined* cloud via the same
   momentum/shock-heat/vapor pipeline as the terrestrial meteor. Earth's matter absorbs most of the
   momentum; crater, ejecta, fallback all emerge from the one contact law. Measured natively: **93% of
   the cloud stays gravitationally bound** — as the declared energetics demand (≈2e7 J/kg deposited vs
   ≈6.3e7 J/kg to unbind).
3. **Conservation-law contact state.** Robin observed "a large percentage of debris escapes" on-device,
   yet the model said 93% bound — the discrepancy was the INPUT: in fast-forward, the deposit used the
   Moon's velocity *after* a ~2000 s step that had carried it far past the surface — a garbage sample
   (**21,822 m/s vs the true 9,870 m/s → ~4.9× the honest energy**). New `orbit::contact_velocity`
   recovers the true state at the surface from the two-body conservation laws (vis-viva energy + angular
   momentum), dt-independent. The simulation FORECASTS the collision; it never samples garbage.
   Also: the frame now STOPS at a detected collision (the render can never show a body sailing past its
   own impact — the simulation drives the visualization and interrupts it).

**Why.** "Get the small stuff right, apply everywhere": one contact law, derived from declared material
parameters, at every scale — and dt-independence as a principle: what the physics concludes must not
depend on how coarsely we stepped or how fast the visualization runs.

**Verified.** `aggregate_particles_collide_via_the_canonical_law_and_conserve_momentum` (no pass-through,
momentum conserved, real rebound); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(93% bound after the contact plays out); `contact_velocity_recovers_the_true_impact_speed_regardless_of_
step_size` (recovery within 2% at the browser's coarsest step vs +121% for the post-step sample);
71/71 native, wasm builds. Visual verdict pending on-device.

---

## 2026-07-11 — Materialize the Moon at impact: honest momentum, real 1/r² fall-back, incandescence for free

**What.** With tunneling fixed, the Moon reached Earth but "dinked on top and fell out the bottom, intact,
no ejecta" (Robin, on-device). Three honesty bugs in the shatter, all now fixed:
1. **Momentum was being dropped/mis-deposited.** First cut drove the debris with the Moon's full *incoming*
   momentum → the whole clump shot DOWN through Earth. I then tried zeroing the momentum — Robin caught it:
   *"drop the momentum sounds like fudge again."* Right. The honest model (Robin's framing): from orbit the
   Earth/Moon are *"really big single particles, an average of physical material properties"*; at impact we
   **materialize** the Moon into its constituent matter, and the fragments carry the Moon's REAL incoming
   velocity (Σmᵢv = m_moon·v — momentum conserved across the promotion, not dropped). The impact ENERGY
   disperses them symmetrically (net-zero momentum). **Earth's surface then transfers the inward momentum by
   CONTACT** — the same swept CCD primitive, now applied per-fragment (inward fragments stop on the ground →
   momentum to Earth; outward ones eject). "Get the small stuff right, apply everywhere."
2. **Fall-back/escape was faked by a uniform gravity field.** A baked uniform "down" wrongly forces even
   >escape-velocity fragments to fall back. Robin: *"Model parameters declare REAL physics… we MUST be
   faithful."* Replaced with a real **point-source 1/r² pull toward Earth's actual centre** (G·M_earth,
   from the masses the model already declares), softening kept tiny (2% R⊕) so the field is faithful where
   fragments live (r ≥ R⊕, contact-enforced). The escape/fall-back split is now EMERGENT, not imposed.
3. **Incandescence now comes free from the thermal state.** `deposit_impact` already computes each
   fragment's temperature; the space shader gained a self-emissive term and the debris is tinted by a
   blackbody ramp of its REAL temperature (dark→red→orange→white). Hot ejecta glows on the night side —
   nothing scripted.

Also added **📷 Earth / 📷 Moon** camera buttons (explicit frame-of-reference switch; "Camera on Moon"
frames the impact site once it shatters) toward Robin's zooming-FoR / "fixed camera 1000 m above the site"
goal.

**Why.** The vision, sharpened by Robin across this session: *all* Newtonian-scale laws should EMERGE from
faithfully-modelled matter — the engine answers "what if the Moon deorbited?" by tracking real materials,
so a child (and, ultimately, an embodied AGI) could *re-derive physics* from playing in it. Every fudge is
a false lesson. So the escape boundary must come from √(2GM/r), not a tuned knob.

**Verified.** New native test `aggregate::point_source_gravity_splits_escape_from_fallback`: a fragment
launched at 1.4×escape leaves for good (>10 R⊕); at 0.6×escape it arcs back and lands (apoapsis <2 R⊕) —
the threshold read straight from the declared M and G, with surface contact mirrored as in the render.
`cargo test -p engine` 66/66; wasm builds + deployed. The shatter VISUAL (scatter + glow) needs on-device
eyes. FLAGGED next: apply the swept CCD to the GPU granular contact (retire the `V_MAX` cap) — same primitive.

---

## 2026-07-11 — Swept collision (forecast the path): the dropped Moon no longer TUNNELS through Earth

**What.** The dropped Moon was shooting straight through the Earth and never colliding. Root cause (Robin
diagnosed it exactly): in fast-forward the Moon moves > an Earth-diameter per step, so the DISCRETE
contact test (are the surfaces overlapping *this* sample?) sees it outside at both samples and misses the
collision entirely — the trajectory was effectively faked, riding on a detection that never fires.
Fixed with **swept continuous collision detection**: `orbit::swept_first_contact(rel_old, rel_new, r_sum)`
solves for the fraction `t∈[0,1]` at which the body's straight path FIRST enters the contact sphere —
*when* it hits — regardless of step size. `OrbitDemo::render` now captures each moon's pre-step position,
runs the swept test after the step, and intervenes at the first-contact point (parking the point mass
there and, for moon 0, triggering the Stage-A shatter at the true impact site/energy).

**Why.** Robin: *"forecast with the simulation (know what will happen in real physics), model it with the
visuals"* and *"there is a difference between what we can render and what we can simulate."* The
simulation must KNOW the continuous path intersects the planet even when we sample/render it coarsely —
what we simulate must not depend on how coarsely we look. And: *"get the small stuff right, APPLY
EVERYWHERE"* — `swept_first_contact` is a pure-geometry primitive (segment vs. sphere), not orbit-specific.
The SAME tunneling is why the grain sim caps the vapor front at `V_MAX` (a workaround); CCD is the honest
general fix there too — flagged as the next application of this primitive to the granular contact.

**Verified.** `orbit::swept_contact_catches_a_body_that_tunnels_through_the_planet` (a −5→+5 pass through
the centre — both endpoints outside — is caught at t=0.4; a clearing path is `None`; already-inside is
t=0). `cargo test -p engine` 65/65; wasm builds. The Moon-collides VISUAL still needs on-device eyes.

**Open.** Apply CCD to the GPU granular contact (replace the `V_MAX` ejecta-speed cap) — the same
primitive, the "everywhere". And the Stage-A shatter visual + Stage B zoom-in remain to verify/build.

---

## 2026-07-11 — Moon-shot Stage A: the dropped Moon SHATTERS (emergent), instead of merging

**What.** In the space band (`OrbitDemo`), the de-orbited Moon now **shatters into a debris cloud** on
impact rather than the point-mass sphere silently merging into Earth. The frame the Moon first strikes,
its point mass becomes a **self-gravitating aggregate** of 64 basalt fragments filling the Moon's volume
at the impact site (`build_moon_debris`), and the impact energy — captured honestly at contact (~4.5e30 J)
— is deposited via the same `aggregate::deposit_impact` pipeline (momentum + shock heat + vapor). Because
that energy is ≫ the Moon's binding energy, the aggregate DISPERSES — no scripted destroy, just kick vs.
binding (docs/21). The fragments then arc under Earth's gravity (uniform toward its centre), some flying
out, some falling back — the ejecta curtain at planetary scale. They render as small basalt spheres at
their real positions; the intact Moon sphere stops drawing. The debris steps at a FIXED observable rate
(`DEBRIS_DT`, a time-LOD) so the fine event plays out at human speed, not the celestial fast-forward that
would disperse it in one frame.

**Why.** The moon-shot (docs/23): "de-orbit the Moon into [a spot], then zoom … and observe it was
destroyed" — with NO code that says destroy. The drop, the fall, the surface contact, and the honest
impact-energy accounting all already existed and were native-tested; the collision just rendered as two
spheres merging + a HUD number tagged "not yet materialised." This wires the tested aggregate-disruption
physics into the render so the shatter is finally *seen*, emergently. It's the celestial half; Stage B is
the zoom-in that materialises the local crater/ejecta from the same conserved energy (docs/19).

**Verified.** `cargo test -p engine` 64/64; wasm builds. The disruption physics itself is native-tested
(`aggregate::energy_above_binding_disrupts_it`, `an_impact_heats_the_core_and_shatters_the_aggregate`);
`build_moon_debris` feeds it the real impact energy. **The VISUAL is NOT yet verified** — a rendering
change can't be checked headlessly (docs/19: "needs on-device eyes"). Needs Robin's eyes + tuning of
`DEBRIS_DT`, `DEBRIS_N`, fragment size, and the fall-back/escape balance.

**Open.** Earth-side damage is still only the HUD verdict (no crater visual on the Earth sphere yet); the
debris external gravity is a uniform approximation (fine near the impact, coarse as it spreads); and the
whole thing is the CELESTIAL band only — the scale-relative zoom-in (Stage B/C) is still unbuilt.

---

## 2026-07-11 — Cohesive grain contact (the frictionless-graze fix, one property doing three jobs)

**What.** Added an ATTRACTIVE adhesion term to the grain contact law (GPU shader + native `granular.rs`
force-of-record): net normal force = repulsive spring − cohesion, so touching grains can now BOND (the
force pulls them together) until the bond lets go past a short range. `cohesion = 0` recovers the exact
old push-only contact. The friction load now includes the cohesion, so a touching/grazing pair has a
real normal load — and therefore friction. `c_cohesion` is derived from `Material::cohesion`, converted
to a per-mass adhesion and capped at a granular ceiling (loose debris is already fractured — rock grains
keep only surface adhesion, they must not re-weld into solid). Reused the dead `c_max_accel` param slot,
so no struct-layout churn.

**Why.** Robin caught that a grain placed at *exactly* zero overlap grazes frictionlessly — friction is
`μ·N` and `N = k·overlap = 0` there. Her instinct: "surely there's a property of matter that ensures
this never happens unless the particles are separated?" There is — **cohesion**, a real material
property already in `materials.json` that we used for solid bonds but not loose-grain contact. It closes
the graze (touching ⇒ bonded ⇒ normal load ⇒ friction), it's *why* soil holds a slope dry sand can't
(the same thread as the granite-cliff/talus split), and it's part of what holds a planet together
against its own gravity — a prerequisite for the Moon-onto-vacuum-Earth moon-shot.

**Verified.** New `gpu-verify` foundational test **F8**: a gentle separating nudge is HELD by the bond
(1.00 m), a hard nudge BREAKS it (6.84 m), a cohesionless pair DRIFTS apart (2.00 m). Native
`granular::cohesion_bonds_touching_grains_and_raises_friction`: a just-touching cohesive pair is pulled
together and a zero-compression graze has friction, while dry stays frictionless. `cargo test -p engine`
64/64; wasm builds; no scene regression (scenes default to cohesion = 0).

**Open.** Cohesion is a single representative value (like friction) — a per-particle/mixed-material
cohesion is a later refinement; and terrain contact doesn't yet carry cohesion (grains adhering to the
ground), flagged.

---

## 2026-07-11 — Emergent impact end-to-end: momentum-conserving contact, terrain-as-matter, drag fudge deleted

**What.** A long arc turning the impact from scripted fudges into emergent particle physics (`docs/24`),
capped by a foundational test suite that caught a core-model bug and a fix that scaled the whole thing.

- **Terrain-as-matter (Path B).** The meteor no longer carves a crater and scripts ejecta velocity. It
  MATERIALIZES the impact region into grains at rest (`matter::materialize_region`), deposits the
  meteor's real momentum as an impulse (`deposit_impulse`), and the rest of ½mv² as radial-gradient
  **shock heat** (`deposit_shock_heat`, filled core-first so a sub-grain impactor's energy actually
  vaporizes a plasma core instead of smearing below threshold). Vaporized matter **expands** and throws
  the ejecta (`deposit_vapor_expansion`) — Robin's insight that at 17 km/s the crater is driven by phase
  transition (gas pressure), not elastic rebound; the KE was already in the sim as shock heat we were
  radiating away. Added estimated thermal data for the granular soils (they couldn't vaporize before).
- **Momentum-conserving contact solve (the core fix).** A two-particle test (`gpu-verify` F5) exposed
  that the directional-implicit solver damped each grain's ABSOLUTE velocity, so a 20 m/s head-on
  collision lost ~74% of its momentum — hidden by every pile/crater scene (slow ⇒ ~0 COM velocity).
  Fixed by a derived neighbor-coupling term `Sv_nbr = Σ S·v_neighbor` in the RHS; the pair's COM velocity
  now telescopes to conserved. This alone made stepped terrain conserve energy and DEEPENED the emergent
  crater ~5× (3 m → 14.7 m) — ejecta finally keep their momentum.
- **Conservative terrain + steep materialization.** Replaced the min-translation terrain penalty (whose
  normal FLIPPED at voxel edges, injecting energy) with a smooth bilinear surface and the exact −∇U
  penalty. Vertical walls a heightfield still can't represent become grains
  (`materialize_steep_terrain`) — but only where the material is too weak to hold a cliff (critical
  height ≈ σ/ρg): dirt slumps to talus, **granite holds as a real cliff** (Robin's antithesis, emergent
  from strength).
- **DRAG FUDGE DELETED** (`matter::DRAG` 0.9995 → 1.0). It bled 62%/s of a vacuum particle's speed
  (foundational test) and was masking the non-conservative terrain; with the three fixes above the core
  no longer needs it — a vacuum particle keeps its momentum.
- **Everything couples honestly.** `aggregate::deposit_impact` (probe/bodies) rewritten to the SAME
  pipeline (momentum + heat + vapor) — the last scripted `√(2·0.3·e/ρ)` kick is gone; the meteor couples
  into EVERY body via `couple_impact_to_bodies`, not a hardcoded probe. Removed the cosmetic
  `spawn_vaporized_meteor` (a scripted 22 m/s clump that looked like an intact meteor and double-counted
  momentum). Restitution is now derived from `Material::restitution` via a θ-method contact integrator.

**Why.** Robin's directives drove it: *"trust physics; hunt for what we're missing, don't fudge"*;
*"crater size should be emergent and observable, not imposed"*; *"a meteor is an exaggerated test of the
same physics as a footfall or a feather"*; *"no fudge in the core models"*; and the clincher — *"test
every aspect of the fundamental interactions of particles; get the small stuff right and the universe
scales."* The foundational suite proved it: a two-particle collision plus a derivation beat the solver
that "looked stable and passed every scene."

**Verified (RTX 2070 headless + native).** New `gpu-verify` foundational block F1–F7 (Newton's 1st/2nd/
3rd, momentum-conserving collision, friction≈μg, touching↔separated sweep) all PASS in true vacuum;
grain-grain energy conservation (I-flat), stepped-terrain conservation (I), emergent crater (M, 14.7 m
deep). `cargo test -p engine` 63/63; wasm builds. New native tests: materialize/impulse/shock-heat/vapor
conservation, steep-terrain materialization + the granite-cliff antithesis, restitution→damping.

**Open (honest, flagged).** (1) A granite cliff a heightfield still can't contact conservatively should
become a COHESIVE aggregate (grains + bonds) — rigid AND conservative. (2) Friction runs ~35% strong
(F6 ratio 1.35 vs μg) — the same over-sticky friction behind the repose under-/over-prediction (scene
D). (3) Crater size is LOD-capped (materialize cap) below the physical scale. (4) Dissipated energy →
heat → radiation still dropped (flagged in-shader). (5) Soil thermal values are composition estimates,
not cited.

---

## 2026-07-09 — North star + a reverted fudge; the engine's name: "Integrity"

**What.** While bringing GPU debris up (docs/22), a play-test exposed that a meteor doesn't destroy the
probe, and I reached for a special case — `if probe within crater { obliterate }`. Robin: "if
everything is real, the probe should have just been destroyed on impact on its own; the fact we have to
correct that concerns me." She's right — that's a **fudge**. **Reverted it.** The real problem: the
probe is the **last bespoke object** (a rigid `body::Sphere`), not matter, so `matter::impact` can't
see it. Wrote `docs/23`: the north-star demo — **a metal ball at ground zero, de-orbit the Moon into
it, zoom in and observe the ball was destroyed** — with NO code that says "destroy the ball." It's
destroyed because the impact energy really reaches it and exceeds iron's thresholds (`damage`). The fix
is to make the probe **real matter** (a cohesive aggregate / voxel body), so gravity, contact, impact,
`damage::classify`, and emission all act on it emergently — no special cases.

**Name.** Robin is naming the engine **"Integrity"** — fitting: it's the operating invariant (every
value traces to real physics or is flagged; no fudge), and reverting the special case is exactly it.

**Also (shipped this session, verified/native):** terrain now uses planetary **surface gravity**
(uniform down, not the slab's micro-g self-gravity — fixes debris concentrating at the world centre;
real-time, no time-scale). The GPU debris path works on-device (FPS fixed, debris glow + cool).

**Open (honest):** probe-as-matter unification (`docs/23`); GPU resting-debris re-deposition
(iteration 3, kills the moiré pile-up); the celestial→local materialization for the zoom-in.

---

## 2026-07-09 — Bodies as particle aggregates (emergent binding + disruption)

**What.** Started making celestial destruction a *simulation, not a mock* (`docs/21`). A body becomes a
**cloud of particles held together by its own gravity** (`aggregate.rs`): softened N-body self-gravity,
`binding_energy` (Σ G·m·m/r), `kinetic_energy_com`, `rms_radius`, `com`. Verified that a cold cloud
**holds together** (cohesion emerges from gravity — the `docs/15` roundness invariant) and that an
energy kick **above the binding energy disrupts it** (emergent dispersal — the identity behind a
shattered moon).

**Why.** Robin asked, pointedly, whether the impact destruction is *inherent in the engine's model* or
"just mocks to humor me." Honest audit: the terrain meteor IS real emergent simulation (per-voxel
fracture/melt/vaporize from material + energy, glowing by computed temperature). But the **celestial**
Moon-crash was NOT simulated destruction — the bodies were point masses drawn as spheres, so there was
no matter to break, and I was about to build a scripted "fireball" — a **mock**. Stopped: that violates
the honesty invariant. The honest path (Robin chose it) is bodies-as-aggregates, so the shatter is the
same gravity that rounds them, run past their binding energy — no script.

**Verified.** `aggregate::a_self_gravitating_cloud_holds_together`,
`aggregate::energy_above_binding_disrupts_it`. `cargo test` 44/44; clippy `-D warnings` clean; fmt
clean.

**Honest scope.** This is the gravitational *skeleton*. Per-particle material + temperature, the impact
coupling (deposit energy → `damage::classify` per particle → emergent debris/melt/vapor), and the
rendering are the next slices (`docs/21`). Until they land, the *visible* Moon-crash still shows the
momentum stick — and we will NOT fake the shatter in the meantime.

---

## 2026-07-09 — Phase classes integrated into matter::impact; Moon-speed readout

**What.** `matter::impact` now classifies each ejecta via `damage::classify` (the thermodynamic
thresholds): a carved voxel is at least Fractured, the hot core Melts, the hottest Vaporizes. The class
drives behaviour — **vaporized** ejecta expand away fast (gas/plasma, `VAPOR_EXPANSION`), all glow by
temperature. The crater extent stays the budget model (LOD bridge `docs/19` intact). Also added a live
**Moon-speed readout** (km/s relative to Earth) to the space-band HUD.

**Why (speed readout).** Robin saw the Moon's velocity seem to "flatten as if terminal velocity in a
vacuum." Checked the orbit path: there is **no drag, clamp, or damping** anywhere — the only velocity
changes are the verlet kicks and surface contact. So there is no terminal velocity; the physics is
honest. The apparent flattening is either Kepler's 2nd law (a *partial* brake makes an eccentric orbit
that slows at apogee — the opposite of drag) or the compressed time-scale hiding the final fast plunge.
The speed readout makes it observable: on a true **Drop** it climbs toward ~11 km/s at impact; use ⏪
slower to watch it accelerate.

**Verified.** New `matter::a_colossal_impact_vaporizes_the_core` (core passes basalt's boiling point →
Vaporized class). `cargo test` 41/41; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
deployed.

**Honest caveat (`docs/20`).** Crater excavation and shock heating still use separate energy
accountings (a flagged simplification — full coupled conservation is the MLS-MPM/shock-EOS future).

---

## 2026-07-09 — Visual: glowing molten ejecta + a Meteor you can fire

**What.** The first visible slice of impact damage beyond text (`docs/20`): impact ejecta carry a
temperature, and molten debris **glows by black-body emission from that temperature**. Added
`Particle.temp_k`; `matter::impact` deposits heat that peaks at the contact and falls to cold at the
crater rim (centre melts/vaporizes, rim is cold rubble — the honest radial gradient); `emission::
incandescence(temp_k)` maps K → an added RGB glow (dull red → orange → yellow → white); the particle
shader **adds** it, so hot debris self-illuminates even on the dark side (it *emits* because it's hot —
the analogue of illumination × reflectance, `docs/17`). A **Meteor** control (`Engine::meteor`, the
`☄`/`m` button in the terrain slice) fires a high-energy `impact` you can watch and orbit into.

**Why.** Robin: "see the impact, then zoom in and see the crater" (with glowing melt). Delivered in the
*terrain* renderer (which renders on-device) so it's verifiable now; the celestial→voxel auto-fly-in
(materialising the Moon-crash crater from its summary) stays staged (`docs/19`).

**Verified.** `emission::cold_matter_does_not_glow_and_hotter_glows_brighter_and_whiter` and
`matter::a_big_impact_melts_the_centre_and_leaves_the_rim_cold`. `cargo test` 40/40; clippy
`-D warnings` clean; fmt clean; wasm + `tsc` green; deployed. The *look* of the glow is for Robin's
on-device check.

**Honest caveat (`docs/20`).** The crater extent is physical (energy/σ), but the ejecta *temperature*
distribution is a first visual model — the energy is not yet conserved through the phase change, and
`incandescence` approximates the Planckian locus. Next (Robin's order): integrate the phase classes
into `matter::impact` proper (voxels → gas/melt/ejecta, energy-conserving), then MLS-MPM.

---

## 2026-07-09 — Impact thermodynamics: fracture → melt → vaporize (one rule)

**What.** Modelled fragmentation, melting, and vaporization as **one data-driven response** (`docs/20`),
Robin's planetary-scale test of the engine (and of scale-of-detail). An impact deposits **energy
density** (J/m³ = Pa); each parcel's fate comes from comparing it to that material's own thresholds:
fracture strength → melt energy `ρ(cΔT+L_f)` → vaporization energy. `damage::classify` returns
`Intact | Fractured | Melted | Vaporized` — the *same* "density vs threshold" logic as fracture, just
higher thresholds. Because the deposited density falls with distance, **one event produces all four at
different radii** (near-field vaporizes, then melts, then fractures, then intact). Added optional
`Material.thermal` (specific heat, melt/boil points, latent heats) with **cited data** for basalt,
granite, iron, water; materials without it can only fracture (we don't claim unknown melt behaviour).

**Why.** Robin: "model fragmentation, melting, vaporization — a test of our simulator's abilities on a
planetary scale (and of our scaling of detail)." A giant impact honestly vaporizes rock near contact,
leaves a magma ocean of melt, fractures/ejects a shell, and — since E ≪ Earth's binding energy — leaves
the planet intact but resurfaced. Every one is the same `classify` at a different radius.

**Verified.** `damage::impact_fractures_then_melts_then_vaporizes_by_energy_density` (thresholds order
σ<melt<vapor; each band classifies right; a giant-impact density vaporizes rock; no-thermal-data →
fracture-only). `cargo test` 38/38; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Staged (docs/20):** integrate into `matter::impact` (voxels become gas/melt/ejecta by class,
conserving mass + energy through the transition); the **visual** display — incandescent melt (black-body
emission from temperature, not a painted colour), a vapor plume, and the materialised crater to fly
into (`docs/19`); cooling/solidification (magma → rock).

---

## 2026-07-09 — Two-moon stress test scene

**What.** A new scene (`/twomoons.html`): two moons on the same orbit, **opposite sides** of the Earth,
that you **de-orbit both at once**. Generalized `OrbitDemo` from one moon to N — `[Sun, Earth, Moon,
Moon2]`, a moon uniform per body, per-moon lighting/framing, and collision resolved Earth-vs-each-moon
with each moon's impact energy counted once (the two hits **sum** in the HUD). `brake_moon`/`drop_moon`
now act on *all* moons; focus cycles Earth → Moon A → Moon B; the second moon is placed at −d with the
opposite tangential velocity so both orbit the same way and stay diametrically opposed. The two HTML
pages share one script — the moon count comes from `<body data-moons>`.

**Why.** Robin: "It's our universe, we might as well play in it." The N-body core (`orbit.rs`) is
already generic, so two moons is nearly free physically; its value is **stressing the collision path** —
two simultaneous surface contacts, symmetric resolution, and (later) two craters materialising at once.

**Verified.** `cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
`/twomoons.html` serves. Visuals (two moons, symmetric de-orbit, double impact) pending Robin's
on-device check.

---

## 2026-07-09 — LOD-adaptive damage: the crater bridge (celestial ↔ voxel)

**What.** Connected the Moon-crash to a real crater across scales (`docs/19`). The bridge: a damage
event is the *same event* at every LOD, so the coarse **summary** and the fine **voxel materialisation**
must agree. Both use the same `σ·V` accounting — `damage::crater_volume(E, σ) = E/σ` (celestial
summary) equals the voxels `matter::impact` excavates (proven:
`matter::voxel_crater_matches_the_coarse_damage_summary`). Added honest **regimes**: strength crater
(`V=E/σ`), gravity regime (flagged, unmodelled), and **disruption** past the body's binding energy.

**Honesty — the Moon is not a tidy crater.** ~4.5e30 J is ~36× the *Moon's* binding energy (the Moon
**shatters**) but only ~2% of the *Earth's* (~2.2e32 J), so the Earth **survives with a planet-scale
crater** — the giant-impact regime, not a neat bowl. The space-band HUD now says exactly this on impact
(`damage::moon_shatters_but_earth_only_craters` pins the numbers). We report the regime honestly instead
of promising a crater the physics forbids.

**Why.** Robin: connect the Moon-crash to a real crater. The honest connection is the σ·V bridge — the
same relation drives the celestial summary and the zoomed-in voxel crater, so promoting/coarsening
across LOD conserves the event (`docs/13`). The *visual* zoom-in (fly the camera down and materialise
the voxel crater) is a real renderer effort, designed in `docs/19`, staged for on-device work.

**Verified.** New: `damage::crater_scales_with_energy_and_inversely_with_strength`,
`damage::moon_shatters_but_earth_only_craters`, `matter::voxel_crater_matches_the_coarse_damage_summary`.
`cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Roadmap (Robin's order):** LOD (this — bridge done; visual zoom-in next) → MLS-MPM → fluid. Planned
playground: a **two-moon** scene (opposite sides, same orbit, de-orbit both at once) as a stress test —
the N-body core is already generic, so it's nearly free.

---

## 2026-07-09 — Unified deformation & damage: the design + first honesty slice

**What.** Started the deformation/damage subsystem (`docs/18`) from Robin's requirement that a **bullet,
a pebble in a pond, and the Moon hitting the Earth be the SAME operator** — differing only in
parameters and level of detail. The design names two invariances: (1) **material** — the response comes
from constitutive data (solids fracture at strength, granular media crater, liquids yield at ~0 and
flow), so bullet-in-rock and pebble-in-pond are one call with different material; (2) **scale/frame** —
the observer's frame/zoom decides what is materialized (celestial: energy/momentum + crater summary;
zoom in: voxel fracture + ejecta; zoom way in: grains/buildings), promoting/coarsening across LOD while
conserving mass/momentum/energy. Two concrete slices landed: (1) parse material **`phase`** and fix the
liquid fudge — water's `fracture_strength` used to fall back to `1e12` (stronger than granite!); a
fluid now yields at ~0. (2) `MatterSim::impact(site, direction, energy)` — the **generalized
energy-driven impact**: it spends the impact energy fracturing voxels nearest-first (σ·V per voxel), so
bigger energy → bigger crater, stronger material → smaller crater, and a liquid splashes. A 10 g bullet
(~450 J) and the Moon (~4.5e30 J) are the *same call*.

**Why.** Robin: the same system should observe a bullet, a pebble in a pond, or a planetary impact —
and at a given scale we simulate only what the observer can perceive (buildings only matter zoomed way
in; ejecta only zoomed in; celestial scale cares about energy/momentum and a crater summary). This is
the honest unification of the voxel-fracture model (`matter.rs`) with scale-relative fidelity
(`docs/13`, `docs/08`) — the endpoint is MLS-MPM with per-phase constitutive models.

**Verified.** `materials::a_liquid_yields_where_a_solid_resists` (a fluid yields to a poke a solid
withstands) and `matter::impact_is_material_and_scale_invariant` (same energy craters dirt but not deep
granite; more energy → bigger granite crater; a gentle impact still splashes a pond). `cargo test`
34/34; clippy `-D warnings` clean; fmt clean; wasm compiles.

**Roadmap remaining (docs/18):** fluid flow (needs a viscosity field, not in the DB yet) → MLS-MPM
constitutive unification → LOD-adaptive damage (summary ↔ detail on zoom). Robin: "we should get to the
rest before we're done."

---

## 2026-07-09 — Orbital-decay control: brake the Moon until it crashes (with real collision)

**What.** The requested experiment — slow the Moon and watch its orbit decay into the planet — plus the
honest physics that makes a "smash" real rather than a numerical explosion. `orbit::resolve_contact`
adds **surface collision**: two solid bodies stop when their surfaces meet (perfectly inelastic,
momentum-conserving), instead of tunnelling through each other as point masses into a 1/r² singularity
— the celestial echo of the voxel body contacts (`docs/16`). `orbit::perigee` computes the live
closest-approach so the HUD can show the orbit tightening. OrbitDemo exposes `brake_moon` (halve the
Moon's velocity relative to Earth), `drop_moon` (cancel it → radial plunge), `reset_moon`, plus a
variable **time multiplier** in the HUD. The web control bar gains Brake / Drop / Reset + slower/faster,
and the HUD shows perigee (reddening below Earth's radius) and "💥 IMPACT".

**Why.** Robin wanted to watch the Moon smash the Earth. The honest lesson is built in: in a
conservative two-body system a *single* halving does NOT crash — it drops into a tighter eccentric
ellipse (perigee ~55,000 km, still a miss); it takes a few brakes (or a full drop) to push perigee
below the surface. Real orbital mechanics, shown, not faked. Also exposed the time multiplier per
Robin's note (and it lets you slow time to watch the impact).

**Verified.** `cargo test` 31/31 — including `perigee_tracks_how_hard_the_moon_is_braked` and
`a_dropped_moon_crashes_into_the_planet_and_stops_at_the_surface` (it reaches the surface and rests
there, no tunnelling). clippy `-D warnings` clean; fmt clean; wasm + `tsc` green. Visuals pending
Robin's on-device check.

**Impact energy (honesty).** Robin noted that at these masses an impact must do *damage* — and that a
perfectly-inelastic "stop at the surface" silently *deletes* the kinetic energy, which is itself a
fudge. So we now **measure and report** it: `orbit::inelastic_dissipation` (the KE the collision
removes) and `orbit::binding_energy`. A dropped Moon hits at ~11 km/s → ~4.5e30 J ≈ **36× the Moon's
gravitational binding energy**; the HUD shows this and states plainly that both bodies would be
destroyed. We measure the damage rather than hide it or fake it.

**Honest scope note.** "Collision" here is surface contact + inelastic stop, plus the reported impact
energy; actual **fragmentation** (deformation, melt, debris, merging) is a future subsystem — the
honest zoom-in unification of the voxel-fracture model (`matter.rs` `fracture_strength`) at scale.
Flagged, not faked.

---

## 2026-07-09 — Live real-Sun lighting, selectable focus frame, scene picker

**What.** Wired the real Sun into the *live* space band (following the validated physics): the demo now
simulates `[Sun, Earth, Moon]` with the Earth on its true ~29.78 km/s heliocentric orbit and the Moon
co-moving. The shader's light direction is now computed per-body **from the Sun's actual position** (no
more hardcoded direction), so the lit hemisphere and the Moon's phases are geometric. The Sun isn't
drawn at this zoom (~23,000 display units off-frame) — it is the *light source*, the scale-adaptive
choice (`docs/17`). Added a **focus control**: the viewport is a physical frame of reference
(`cycle_focus` / `focus_label`), re-centring the whole view on Earth or the Moon. And a **scene picker**
(`web/src/scene-nav.ts`) injected on both pages to switch between the terrain slice and the space band.

**Why.** Robin's direction: a real Sun should light the system (not a fake light), the viewport is a
physical frame of reference with a selectable focus, and the app should let you choose between scenes.
All three are honest, emergent-from-real-state changes (`docs/17`).

**Verified.** `cargo test` 29/29; clippy `-D warnings` clean; `cargo fmt` clean; wasm builds and
`tsc --noEmit` passes (focus + scene-nav bindings). **Visuals pending Robin's on-device check** —
headless WebGPU can't render here, so the appearance of the sun-lit bodies and the focus/scene UI is
for iPad confirmation.

---

## 2026-07-09 — Honest appearance: no painted tints, brightness from light, a real Sun

**What.** A user play-test of the space band exposed fudging: the Earth was a hardcoded ocean-blue
tint and the Moon a hardcoded grey — cosmetic colours touching no material data, even though the
terrain already colours voxels from real `materials.json` albedos. Replaced with honesty (`docs/17`):
(1) body colour = **aggregate albedo of a real composition** via the new `materials::aggregate_albedo`
operator (Earth = ocean water + continental granite + polar ice; Moon = basalt) — a computed summary,
not a paint job; (2) the space shader now does **illumination × reflectance** (bright sun × real, often
dark, albedo) + Reinhard tone-map, so a dark-but-lit body reads bright — the honest reason the Moon
looks bright; (3) added a validated **Sun–Earth–Moon** physics test: a real Sun (1.989e30 kg, 1 AU) and
the Earth given its **appropriate heliocentric velocity** (~29.78 km/s), with the Moon staying bound to
the moving Earth.

**Why.** The user pushed the honesty invariant (`docs/15`) all the way down: *don't fudge*. Key
insights captured: brightness is illumination × reflectance (not a bright material); even albedo is a
summary placeholder for real optics (ray tracing is the goal); zoom-out summaries are fine only if
*computed from everything we know* by one operator for all objects/scales; the illuminant should be a
real Sun; the viewport is a **physical frame of reference** with a **selectable focus** (planet →
Moon → …); and the core research question is whether the system can tell **what matters at a given
scale**. Working principle / candidate name: **"Integrity."**

**Honesty flags (not hidden).** Earth composition excludes the atmosphere → deliberately no Rayleigh
blue (the blue-marble blue is atmospheric, unmodelled); Moon lacks highland anorthosite in the DB → it
renders darker than reality until added; the shader's sun *direction* is still a placeholder until the
real Sun is wired into the live view.

**Verified.** `cargo test` 29/29 (new `aggregate_albedo_summarizes_real_constituents`,
`sun_earth_moon_system_is_bound`); clippy `-D warnings` clean; fmt clean; wasm compiles. The *visual*
result of the new lighting is for on-device confirmation (headless WebGPU can't render it here).

**Staged (larger, honest work):** real Sun as the live illuminant + heliocentric re-centering + focus
switching; ray tracing; specular/BRDF from roughness/metallic; stellar & anorthosite materials;
atmosphere for the earned blue; and the still-owed orbital-decay control.

---

## 2026-07-09 — Unified dynamics: everything not at rest reacts

**What.** Fixed the "probe quits falling / doesn't really react to debris" behaviour by unifying the
probe and the debris into **one awake-set dynamics loop** (`docs/16`). Previously `body::Sphere` (the
probe) and `matter::MatterSim` (debris) were separate systems coupled only through the voxel grid —
`matter.rs` never referenced the probe — so particles couldn't push it and settling debris deposited
voxels *inside/under* it, making it appear to rest on nothing. Now, per substep, every awake body
integrates under the same gravity field, resolves body↔world contacts, debris steps under that field
and **won't deposit inside a body** (piles on it, conserving matter), and **body↔debris contacts
exchange momentum both ways** (`MatterSim::couple_body`). Sleep/wake is structural: a body sleeps only
while in contact and slow, and wakes the instant support is removed or something hits it.

**Why this shape.** The user's principle: a physics loop looks at every object *not at rest* and makes
it react as a natural property of the world and the object, never a per-object script — the honesty
invariant (`docs/15`) applied to dynamics. Also captured the deeper motive: an honest, inferable
physical world is a place to *learn to act* (VR, and plausibly embodied-AI training), a payoff that
exists only to the degree the sim refuses to fake.

**Also (honesty corrections from the user).** (1) No atmosphere is modelled — matter falls through
*vacuum*, so the per-step `DRAG` constant is flagged as a numerical-stabilizer debt, not real air drag.
(2) Compute-budget policy written down: favour larger/more massive objects (massive bodies are
budget-exempt today; debris coarsening must *merge into mass-carrying clumps*, conserving mass on both
spawn and settle — so it's deferred, not half-done, to avoid a mass leak). (3) Noted the
server-authoritative-world / client-sees-a-slice threshold to watch (`docs/11`, `docs/13`).

**Verified.** New native tests: `particle_transfers_momentum_to_a_body` (momentum conserved through the
impact), `debris_does_not_settle_inside_a_body`, `body::wakes_and_falls_when_support_is_removed`.
`cargo test` 27/27; clippy `-D warnings` clean; `cargo fmt` clean; `cargo check --target
wasm32-unknown-unknown` green (the awake-set loop lives in the wasm-only host).

---

## 2026-07-09 — Representation invariant: the cube is a lattice, not a unit of matter

**What.** Answered a foundational design question — "are we baking a core mistake into the engine by
building on cubes, when the universe is made of spheres?" — and locked the answer in as canonical.
Wrote `docs/15`: **a voxel is a sampling cell, never a unit of matter.** The cubic grid is the
coordinate lattice we sample continuous fields on (density, material, momentum), like pixels sample an
image; it is not an ontology of blocks. All physical state lives on matter with continuous coordinates
(`Particle.pos`, `MassPoint`), and bulk voxels dissolve into particles the instant physics touches
them (`docs/08` tiers). Added a **grid-isotropy regression suite** (`isotropy.rs`) to enforce it.

**Why.** The honest answer is that cubes are *not* a foundational mistake — roundness is emergent, not
primitive. Real solids sit on lattices (many cubic — rock salt, BCC iron), yet planets are round
because isotropic self-gravity averages over the lattice; the engine already mirrors this (aggregate
mass → spherical far field in `gravity.rs`/`orbit.rs`; surface nets smooth the render). The *real*
risk is subtler: a regular lattice has preferred directions (axes, 45° diagonals) and a solver could
silently bake that bias into the physics. Also captured the user's north star: the world should **feel
right in VR because it is right, not via per-object fakery** — leave something unsupported and it
falls as a natural property of the world and the object (`find_unsupported` → `collapse`), never a
script.

**Verified.** New suite asserts (a) gravity on a symmetric ball is radial + equal-magnitude across
face axes and edge/corner diagonals (spread < 1%, tangential < 1%), and (b) `dig` carves a true
Euclidean sphere (volume within a few %, equal axis reach, no lateral ejection bias). Proven
**non-vacuous** via mutation testing: an injected axis bias in the gravity sum and a Chebyshev (box)
dig criterion both drove the guards red (gravity spread 9.7%; box removed 8000 vs a sphere's 4189),
then reverted. `cargo test` 24/24; clippy `-D warnings` clean; `cargo fmt` clean.

---

## 2026-07-09 — Space band: watch the Moon orbit (v0.9.0)

**What.** Step A of the scale-relative "orbit-to-ground" (`docs/13`): a spectator view of the real
Earth + Moon (`/orbit.html`). `OrbitDemo` runs `orbit.rs` (real SI, f64) each frame and renders two
lit spheres via a tiny new `space.wgsl` (position/normal + per-body tint + one directional sun, so we
get phases). Metres → display units (Earth radius → 1); the Moon sits ~60 units out. Time-scaled so a
~27.3-day orbit plays in ~20 s, substepped 16× for a stable symplectic step. HUD reads live
separation (~384,400 km). Kept on a separate page + Vite multi-page input so the terrain slice is
untouched.

**Why this shape.** I can't self-verify visuals here (headless WebGPU won't render the pipeline), so I
minimized blind risk: reuse the *proven* GPU setup pattern, the existing sphere mesh + `draw` path, and
lean on the already-validated physics (`orbit::moon_orbits_earth`). The renderer is a thin shell over
known-good pieces; the hard part (the orbit) is the tested part.

**Also.** Wrote `docs/13` (north-star: observer-relative fidelity) and `docs/14` (validation
demonstrations — each physics test mapped to what it proves + how to *show* it), at the user's request
to preserve the test concepts as demonstrations for the full build.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm build compiles `OrbitDemo` warning-
free; `tsc` clean; LAN dev server serving `/orbit.html`. Visuals to be confirmed on-device.

---

## 2026-07-09 — Solid-object collision + orbital-mechanics validation (v0.7.2, v0.8.0)

**Collision (v0.7.2).** From an iPad play-test: the probe clipped into crater walls (looked like a
duplicate ball, rested too high) because it only collided with the terrain column directly beneath
it. Replaced with proper **sphere-vs-voxel collision** (`body.rs`): integrate under gravity, then
iteratively push out of the deepest solid voxel the sphere overlaps (floor, walls, corners) with
restitution + friction. Solid objects act solid.

**Orbital validation (v0.8.0).** Added `orbit.rs` — N-body point-mass gravity + a symplectic
velocity-Verlet integrator. The native test drops in the **real Earth + Moon** (masses, 384,400 km,
1.022 km/s) and confirms a bound orbit: ≥1 full revolution, distance within 15% of real, energy +
angular momentum conserved <1%. This proves the gravity law reproduces real celestial motion — the
"does the Moon orbit the planet?" test — and, importantly, it's a **pure native test** (no rendering),
so it verifies the physics despite headless WebGPU being unavailable here.

**Note on tooling.** Headless Chromium here renders WebGPU only via software (SwiftShader) or hits a
Dawn instance bug on the real GPU, so I can't screenshot the full render; I lean on native tests
(watertight mesh, collision, orbit) + the user's iPad for visual confirmation. `web/screenshot.mjs`
is kept for environments with GPU access.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm + web build green.

---

## 2026-07-08 — Phase 6: smooth surface meshing (v0.7.0)

**What.** Terrain and craters now render smooth instead of blocky cubes. `mesher::build_surface_nets`
runs Surface Nets (`fast-surface-nets` crate) over the voxel occupancy field, recomputes smooth
normals from the geometry (oriented outward), and tags each vertex with its nearest material so
triplanar texturing + shine still apply. The renderer uses it for the initial terrain and every dig
re-mesh; the blocky mesher is kept as a fallback.

**Why.** The user flagged the Minecraft-blocky look. The key insight: the voxel grid is the *physics
substrate*, not the *visual* — so we smooth the rendering (marching-cubes/surface-nets style) while
mass, gravity, fracture, and collapse stay identical. Prototype clunkiness → smooth surface, no
physics change.

**Verified (TDD).** `cargo test`: 19/19 (new: surface-nets mesh is valid, finite, and genuinely
smooth — has non-axis-aligned normals). fmt + clippy (`-D warnings`) clean; wasm + web build green.
Live LAN wasm rebuilt. **Pending human check:** reload → rounded terrain and craters, still textured
and lit; dig/blast/collapse all still work.

**Next realism levers (noted):** smoothed/SDF field for rounder geometry, normal maps from the grain
field, finer/smoother debris (or MPM).

---

## 2026-07-08 — Phase 5: structural collapse (v0.6.0)

**What.** Undercut or isolated matter no longer floats. `world.find_unsupported()` flood-fills from
the anchored base (`y=0`) and returns any solid voxel not connected to it; `MatterSim::collapse()`
detaches those into falling particles, run after every dig. This closes the Phase-3 "floating voxels"
known limitation — overhangs, undercuts, and blasted-off chunks all fall and re-settle.

**Why.** Real matter needs support. Connectivity-to-anchor is the general, correct model (works on a
plateau now and a planet core later) and needs no per-case rules.

**Verified (TDD).** `cargo test`: 18/18 (added: intact terrain has no unsupported voxels; an isolated
voxel collapses, conserves matter, and re-settles). fmt + clippy (`-D warnings`) clean; wasm + web
build green. **Pending human check:** `npm run dev` → shift-click to undercut a ledge and watch the
overhang break loose and tumble down.

---

## 2026-07-08 — Phase 4: emergent textures (v0.5.0) — vertical slice complete

**What.** Materials now look distinct, generated *from their own properties* with **no bundled
images**. `texture.rs` synthesizes a high-res (512²) mip-mapped texture per material from
albedo + color_variance + metallic (grain/mottle + flecks + metal sparkle), seamless. The world
shader triplanar-samples a per-material texture array and adds a specular highlight (shine) from
per-material roughness/metallic. HUD gains an FPS counter. `docs/12` documents the approach + CC0
sources (ambientCG/Poly Haven) for optional user textures.

**Why.** Closes the appearance side of the thesis: look emerges from the same cited data that drives
mass, gravity, and fracture — one source of truth. User asked for high-res + no licensed photos;
procedural generation delivers both (mipmaps = scale-down; zero image assets = zero licensing).

**Verified (TDD).** `cargo test`: 16/16 (added 4 texture tests: size+mip chain, mean tracks albedo,
materials differ, non-flat variation). fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` +
`vite build` green. **Pending human check:** `npm run dev` → speckled granite, mottled dirt, green
grass, a shiny iron probe; dig to see textured debris.

**Milestone.** This completes the **Phase 0–4 vertical slice** from the plan: layered voxel matter ·
self-gravity (F=ma) · dig & material-driven fracture · emergent texture — all driven by the cited
material database. All four project pillars are demonstrable.

---

## 2026-07-08 — Phase 3: dig & material-driven fracture (v0.4.0)

**What.** Destructible matter. `matter.rs` is a CPU matter solver: click-to-dig (voxel raycast DDA)
fractures a spherical region — a voxel detaches into a particle only if the tool's stress exceeds its
material's `fracture_strength` (loaded from the cited DB). Debris falls under the Phase-2 gravity
field and, on rest, deposits back into the voxel grid (piling, matter-conserving). Instanced debris
rendering (`particles.wgsl`), terrain re-mesh on edit, HUD debris count. Click digs soil/grass;
shift-click blasts rock.

**Why.** Proves the core destruction thesis — materials break *differently by their own numbers*
(granite shrugs off what shreds grass), with no per-material special-casing. Framed honestly as the
**CPU, testable foundation** for full continuum MLS-MPM (deformation/stress + WGSL port) later, since
GPU MLS-MPM can't be unit-tested natively and TDD is canonical.

**Verified (TDD).** `cargo test`: 12/12, incl. `dig_detaches_soft_but_not_hard` (soil detaches under
1e6 Pa, granite needs a 2e7 blast) and `matter_conserved_through_dig_and_settle` (voxels + airborne
particles == original, every step, until all settle). Plus raycast-hits-terrain. fmt + clippy
(`-D warnings`) clean; wasm build clean; `tsc` + `vite build` green.
**Pending human check:** `npm run dev` → click the grass/dirt to blow a crater of tumbling debris
that resettles; click rock (nothing) then shift-click (it breaks).

**Known limits (noted for later):** mid-column digs can leave floating voxels (no structural
collapse yet); full-world re-mesh per edit (dirty-chunk meshing is the optimization).

---

## 2026-07-08 — Phase 2: self-gravity + falling probe (v0.3.0)

**What.** Made density physically active. `gravity.rs` computes a real Newtonian field from the
world's aggregate voxel mass (voxels lumped into blocks; direct-sum with f64 accumulation).
`body.rs` integrates a rigid sphere under that field (`F = ma`, semi-implicit Euler) with ground
contact and a scale-relative rest threshold. The renderer draws the probe via a per-object model
matrix; a live HUD shows world mass, local gravity, altitude, speed, rest state, and time-scale
(`Space` re-drops, `[`/`]` change time-scale).

**Why.** Proves pillar 4 — the world's own summed mass produces gravity; a probe obeys `F = ma` and
rests on the surface. No Rapier yet: one hand-integrated body is exact and far simpler; Rapier is
deferred until many bodies/contacts justify it.

**Honest scale note.** Real `G` is used, so the ~96 m world has asteroid-scale micro-g (~1e-5 m/s²).
That's correct physics; a time-scale fast-forwards the sim for viewing (time-lapse, not fake gravity).

**Verified (TDD).** `cargo test`: 9/9 — point-mass `G·M/r²`, far-field within 1%, mass conservation,
free-fall kinematics (`v=-g·t`, `½g·t²`), fall-and-rest, and an end-to-end drop onto the generated
world. fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` + `vite build` succeed.
**Pending human check:** `npm run dev` → watch the iron probe fall and settle; HUD reads out g and rest.

---

## 2026-07-08 — Phase 1: layered voxel world on screen (v0.2.0)

**What.** Turned the material data into a rendered world. Added to the engine crate:
- `materials.rs` — loads the cited `data/materials.json` (density + albedo) at compile time.
- `world.rs` — chunk-style voxel store + a layered generator: rock bulk, ~10 m dirt, grass skin,
  with a deterministic value-noise heightfield so the surface undulates (layers follow terrain).
- `mesher.rs` — face-culling mesher (only air-facing faces), per-material albedo vertex colors, so
  the rock/dirt/grass bands are visible on the exposed side walls.
- `lib.rs` + `shaders/world.wgsl` — a real 3D renderer: vertex/index/uniform buffers, depth buffer,
  perspective orbit camera, and a directional light + ambient/hemispheric fill.
- `web/` host: drag-to-orbit / scroll-to-zoom controls, gentle idle auto-rotation.
Also added `docs/10` (robustness — how the matter-first model designs out tunneling / fall-through /
"weird physics", plus the mitigations and an adversarial test plan).

**Why.** First milestone that makes "density as source of truth" *visible* and validates the core
Rust→WASM→wgpu render path end to end, on the real seed data.

**Verified.**
- `wasm-pack build` clean (no warnings). `tsc` clean. `vite build` succeeds (wasm ~1.32 MB dev).
- Dev server serves `engine_bg.wasm` as `application/wasm`.
- `cargo test` (native): material DB loads 19 materials with granite denser than dirt; the central
  column is grass→dirt→rock top-to-bottom and solid to y=0; mesher output is well-formed (quad-aligned
  vertices, 6 indices/quad, all indices in range).
- **Pending human check:** `cd web && npm run dev` in a WebGPU browser — a layered rock/dirt/grass
  plateau you can orbit and zoom.

**Version.** Milestone **0.2.0** (Phase 1) per the pre-1.0 policy (each phase bumps the minor).

---

## 2026-07-08 — Materials seed database + object/interaction design

**What.** Compiled the first **cited physical-properties database** — 19 materials (rock, ceramic,
metal, organic/wood, soil, granular, liquid, frozen) with mechanical + optical properties and source
URLs — into `data/materials.json` (schema in `docs/04`). Added design docs for the architecture the
user articulated: material **taxonomy + finishes + object composition** (`07`), **adaptive resolution
& clumping** so the sim scales instead of moving billions of particles (`08`), and **agentic object
authoring + physically-grounded tool/terrain interaction** — the "make a shovel" / shovel-in-dirt
vision (`09`).

**Why.** Physical properties are the single source of truth for both simulation and rendering; the
whole object/agentic vision ("make a shovel" that falls, sounds, and digs like one) reduces to
material data + physics + composition, with no bespoke per-object code.

**Verified.** `data/materials.json` parses (node `JSON.parse`), 19 materials each with mechanical +
optical blocks; categories: rock 4, ceramic 1, metal 3, organic 3, soil 2, granular 2, liquid 2,
frozen 2. Research quality-checked: rejected known-bad MatWeb figures (granite/limestone UCS),
flagged cited-vs-estimate, and captured state-dependence (soils/snow) and anisotropy (wood).

**Note.** JSON is the v0 seed; it migrates to the Postgres source of truth (`docs/05`) and grows into
the module/taxonomy system (`docs/06`, `07`) over time.

---

## 2026-07-08 — Published to GitHub as a monorepo

**What.** Restructured the engine into the `robinmack/BotheadStudios` monorepo as its first
project directory, `integrity-engine/`. Root of the monorepo carries an MIT `LICENSE` and a
projects README. Aligned the engine to **MIT-only** (dropped the Apache dual-license) to match the
repo's license choice. Published the public OSS repo and tagged `v0.1.0`.

**Why.** BotheadStudios will hold multiple game projects; a monorepo keeps them together. MIT
across the board keeps licensing simple and consistent.

**Verified.** `git push` to `origin/main` succeeded; `v0.1.0` tag pushed; repo is public.

---

## 2026-07-08 — Project kickoff & Phase 0 scaffold

**What.** Created the engine as the first project in the **BotheadStudios monorepo**
(`integrity-engine/`). Established the skeleton: `crates/` (Rust core), `web/` (TypeScript host),
`shaders/` (WGSL), `docs/` (research + design).
Added `README.md`, `LICENSE-MIT`, `CONTRIBUTING.md`, `.gitignore`, this journal.
Installed the toolchain: Rust 1.96.1 + `wasm32-unknown-unknown` target + wasm-pack 0.13.1 (Node 22 already present).

**Why.** The plan (see `.claude/plans/…` / `docs/`) settled a performance-first stack — Rust→WASM core,
custom `wgpu` WebGPU renderer, Rapier rigid bodies — after research confirmed **no existing engine fuses
all four pillars** (density-as-truth matter · emergent-from-density behavior · destructible-to-the-core ·
real self-gravity). See `docs/01-prior-art-existing-engines.md` and `docs/02-oss-building-blocks.md`.

**Phase 0 goal.** Prove the pipeline end-to-end: a Rust crate compiled to WASM initializes a `wgpu`
device and clears a browser canvas, driven by a thin Vite/TypeScript host. First pixel on screen.

**Verified (build/serve level).**
- Rust → WASM compiles via wasm-pack (fixed three `wgpu` 24.0.5 API differences vs. older docs:
  `request_adapter` returns `Option`, `request_device` takes a trailing `Option<&Path>` trace arg,
  and `RenderPassColorAttachment` has no `depth_slice` field).
- `npx tsc --noEmit` clean; `vite build` bundles the app (wasm 933 KB → 236 KB gzipped).
- `vite` dev server serves `engine_bg.wasm` as `application/wasm` (verified magic bytes `\0asm`).
- **Pending human check:** open `npm run dev` in a WebGPU browser to see the pulsing clear color.

**Version.** Tagged this milestone **0.1.0** (see `CHANGELOG.md`, `docs/03-versioning.md`).
Pre-1.0 policy: each roadmap Phase bumps the minor; games pin exact versions since we dogfood.

---
