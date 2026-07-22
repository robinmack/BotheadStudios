# THE LAWS OF INTEGRITY — read first, every session

The moral compass of this engine. When a decision is unclear or a long session has lost its way, these
decide it. Full text + rationale: [`docs/00-laws-of-integrity.md`](docs/00-laws-of-integrity.md).

1. **Physics is the product.** Real physics, not graphics that resemble it. The picture reports the sim.
2. **One law, every scale, every scene.** Raindrop, tyre, and giant impact are the same physics at
   different scale/material/energy. One question must not get two answers. Grep for the primitive first.
3. **Simulate what you can; compute what you can't; fake nothing.** Math sizes the interaction, the
   minimal necessary matter becomes real particles, those are simulated thoroughly, the rest is real math.
4. **The camera changes representation, never existence.** Off-camera physics still happens (cheap math);
   its effects propagate and are rendered as they come into view. Looking away never changes what is true.
5. **NO FUDGE, ever.** No dial or constant to make something "look real." Every number traces to physics
   or is an openly-flagged IOU that names the real computation it defers. If physics disagrees, record it.
6. **Physics drives the render, never the reverse.** Never move matter for a picture; never let a visual
   criterion decide what is simulated. Interest decides what is drawn; necessity decides what is computed.
7. **Measure and derive; never assume.** A number you did not measure or derive is a guess — wrong until
   checked. Test, then conclude. Pin acceleration to brute force. A negative result, honestly measured, ships.

8. **This is a NEW KIND of engine — challenge what you "know".** Traditional engines only *emulate*
   physics; Integrity *embodies* it — **to the best of our ability with the compute available**. That
   clause is the honest bound, not a loophole: the question is never "physics or shortcut?" but *"is
   this the most physical thing this budget can buy, and does it converge as the budget grows?"* LOD ladders, baked lighting, colliders standing in for objects,
   bump maps standing in for surfaces — these answer a different question. The test is never "is this
   how engines do it?" but **"does this embody the physics, or imitate it?"** A borrowed technique is
   admissible only as a declared stand-in (Law 5): derived from the real quantity, flagged, convergent.
   When a familiar solution arrives fully formed and obvious, THAT is when to stop and ask what the
   honest version is.

*In one breath: real physics, one law at every scale, faked nowhere — simulated where seen, computed where
not, never assumed where it can be measured, and never borrowed merely because it is familiar.* If any doc, comment, or past decision contradicts a Law,
the Law wins and the other is the bug.

---

# Integrity engine — start here

A Rust→WASM→WebGPU real-time **physics** engine. Charter: *everything is matter; one contact law + one
gravity law govern it at every scale* — a tire, a meteor, and Theia are the same physics at different
scale/energy/material (docs/23, docs/24, docs/28). Physics drives the render, never the reverse.

**The promise is REAL physics: one law, at every scale, in every scene — a world is a world is a world.**
That is the product, not a preference about code structure. An engine that answers the same physical
question two different ways in two different scenes has broken it.
[`docs/46-one-physics-charter.md`](docs/46-one-physics-charter.md) states the rule that separates
legitimate specialization (the *physics* differs — stiff contacts vs orbital integrators) from a
violation (the same question, two answers), and carries the **conformance ledger** of open violations
with their evidence. **Read it before adding physics, and add a row when you find a new one** — it exists
so the list is inherited, not rediscovered every session.

> **Sense déjà-vu? Read the docs.** If you find yourself deriving a conclusion that feels like it was
> reached before — it was. Nearly every "discovery" in this engine is already written down, with the
> evidence and the reasoning that produced it. Deriving it again wastes the session AND risks landing a
> *different* answer to a question already settled, which is itself a charter violation (docs/46).
> Search `docs/` and `JOURNAL.md` first; add to them when you genuinely find something new.

**Before exploring, read [`docs/32-architecture-map.md`](docs/32-architecture-map.md)** — the full module
map with `file:line` anchors. It exists so you don't rediscover machinery. The realignment plan the engine
is being refactored toward is [`docs/33-architecture-realignment.md`](docs/33-architecture-realignment.md).

## The 60-second model

- **One crate** `crates/engine` (Rust core) → WASM (`wasm-pack`) sharing one `wgpu` device with the
  renderer. `web/` is a thin TS+Vite host. Public: **integrity.bothead.net** (docs/29).
- **Two scene structs** in `lib.rs`: `OrbitDemo` (space band, the giant impact / birth-of-the-Moon; owns
  a `gpu_sph::GpuSph` running `sph_step.wgsl`) and `Terra` (the docs/43 worlds-as-data planet scene,
  backed by `crates/engine/src/terra/`). The terrain `Engine` — the first scene designed — was DELETED
  2026-07-21 at Robin's request (docs/50).
- **A scene should be DATA, and is not** (docs/46 ledger row 14). Robin's standing requirement: scenes
  carry object/assembly definitions, coordinates and materials and must *"not require special mods of the
  engine itself"*. Both remaining scenes are `#[wasm_bindgen]` structs INSIDE the engine crate with their
  own pipelines and render loops, so adding or removing one means editing the engine — deleting terrain
  cost 1,516 lines of `lib.rs` plus a public-API change. Do not add a third scene this way.
- **The key fact:** the physics *laws* are already unified and scale-invariant (`granular::Contact`,
  the SPH kernel, `Furrow` excavation, `plough_loft`, `Body`, `LayeredBody`); the *solvers and containers*
  are FORKED (CPU `Aggregate` f64 vs voxel-`World`/GPU f32; four integrators; Earth-as-rigid-boundary vs
  Earth-as-particles). Do NOT add a new per-scene particle path — extend the shared one. See docs/32 §4.
- **The physics gap is WIRING, not capability.** The condensed-matter EOS *exists* — `eos.rs` implements
  Tillotson, verified vs Benz & Asphaug 1999 — but reaches only the space band (`hydrostatic.rs`,
  `gpu_sph.rs`). The terrain/voxel/granular path still resists compression by linear-elastic contact penalty
  alone, and planet layer densities are still declared constants (docs/32 §5, docs/33). This entry read "no
  condensed-matter EOS" until 2026-07-19, which was false and would have sent a session to build what was
  already there. It is one instance of the pattern docs/48 names — physics built and verified, then wired
  into one place or none. **Grep for the primitive before writing one.**

## Before you create a scene or a behaviour — the Law pre-flight

**Run this BEFORE writing it, not after.** On 2026-07-21 a scene shipped that broke four Laws while the
Laws sat in a file edited that same day; availability is demonstrably not enough. Each question below is
a mistake actually made in this repo, not a hypothetical.

1. **Does any number in it fail to trace to physics?** (Law V) A declared `gravity`, a surface pressure,
   an escape velocity, a clamp, a "power" dial. → Name the matter instead and let the quantity emerge.
   `crate::laws` FAILS THE BUILD on the ones a machine can see; the rest are yours to catch.
2. **Does it answer a question the engine already answers?** (Law II) Grep for the primitive before
   writing one, and enumerate the existing consumers. Two grain-interaction paths, two ways to get
   ground height, two incandescence curves — each of those shipped here.
3. **Does it resolve more than necessity requires?** (Law III) Resolving a whole patch because it is
   simpler is "by whim". The un-resolved world is still computed, just cheaply.
4. **Does the camera decide anything but representation?** (Law IV/VI) If looking away changes what is
   true, or a visual criterion decides what is simulated, stop.
5. **Is the camera itself matter?** (Law I) It obeys the same contact law as a grain — never a clamp.
6. **Are you reaching for it because it will LOOK right?** (Law I) *"That instinct is the enemy of this
   engine."* This is the one that produced every other failure in the list.
7. **Has this already been decided?** Search `docs/` and `JOURNAL.md` first. Adding a new doc that
   restates a settled principle is its own failure — the answer is usually already in `docs/00`.

If a step cannot be satisfied honestly, the right output is a **flagged IOU that names the real
computation it defers** (Law V) — recorded in `docs/46`'s ledger, not a quiet approximation.

## Hard rules (do not violate)

0. **Every scene has a "Share view" button.** Use `web/src/share-view.ts` — do NOT write a second
   capture path. A scene calls `createShareView(canvas, …)`, places the returned button in its control
   strip, and calls `share.afterPresent()` **immediately after it presents** (a WebGPU canvas is only
   readable while its drawing buffer is current; capture anywhere else silently yields a blank image).
   The frame is POSTed to `/__shot` and written to `shots/shot-<ts>.png`, which is how a picture gets
   from a scene to whoever is reading the repo. `web/rig/share_button.mjs` asserts the button exists AND
   that a real PNG lands, on every scene.

1. **Work directly in the main checkout on a feature branch** — `~/workspace/BotheadStudios`. Do NOT
   create git worktrees. (This reversed on 2026-07-19: worktrees existed to isolate parallel agents, and
   this is a single-developer project that is not doing multi-agent work. They cost a duplicated
   `node_modules` per tree, a shared stash stack that different sessions can pop out from under each
   other, and branches that quietly diverge in directories nobody is looking at.) Branch, commit, push,
   PR — never commit to `main` directly.
   **Keep the branch list at `main` alone** (Robin, 2026-07-20, stated twice). One feature branch at a
   time; merge it, delete it (`gh pr merge N --squash --admin --delete-branch`), and `git fetch --prune`.
   Do NOT leave branches parked: this is a single-developer repo and there is nobody else's in-flight
   work to preserve. Work worth keeping but not merging (measurements, evidence, a salvaged tool) becomes
   an **annotated tag** `archive/<name>` whose message records WHY — same commits, `git show
   archive/<name>`, zero branch clutter. Five such branches were retired this way on 2026-07-20.
2. **NEVER run `cargo fmt`** — the crate isn't rustfmt-conformant; it reformats the whole tree. Edit by
   hand. (`CONTRIBUTING.md` says otherwise for outside contributors; the working rule is do-not-run.)
3. **Test:** `bash scripts/test.sh --fast [filter]` (inner loop) · full `bash scripts/test.sh` before any
   deploy (240 run by default). O(n²) measurement tests are `#[ignore]` (18 of them —
   `hydrostatic.rs` 9, `impact.rs` 8, `aggregate.rs` 1; run `--ignored`). Accelerated code is always pinned
   to its exact/brute-force reference so speed never changes the answer. `gpu_sph.rs`'s PHYSICS is still
   verified out-of-process by `tools/sph-verify` (which carries its own replica of the structs), but the
   module is no longer invisible to the suite: it compiles on every target since 2026-07-20, and its three
   shader-facing layouts are pinned to `sph_step.wgsl` in-crate.
4b. **Motion is a property of the SEQUENCE, not of any frame.** A screenshot cannot see stutter, a
   freeze, popping or a teleport. `scripts/rigvideo.sh <rig>.mjs` records the composited screen
   losslessly while the rig drives the scene and reports freeze %, delivered fps, worst hitch, and
   discontinuity jumps. Read it against `scripts/analyze_motion.py --selftest`, which prints the same
   metrics for a known-smooth, a known-stuttery and a known-frozen clip.
   **Launch rigs only via `scripts/rig.sh` (or `rigshot.sh`/`rigvideo.sh`), never a bare
   `chromium.launch`.** Without `--disable-frame-rate-limit` this headless setup paces EVERY page at
   exactly 1 Hz (1003 ms, ±0.2 ms) and every frame-rate measurement is capped at 1 fps no matter what the
   engine does. That artifact was briefly written up here as a real ~1 fps engine collapse; an
   INDEPENDENT empty rAF loop reading 1.0 fps on all three scenes is what exposed it. `web/rig/_launch.mjs`
   is the one place the flags live. True rates on the 5060 Ti (2026-07-21): **terra ~354, birth ~52,
   terrain ~23 fps.**
4. **Rig-watch every visual claim** (Law: physics drives the render — verify the render). `npm run wasm`
   + serve (`npx vite` in `web/`), start the GPU-backed X server ONCE with
   `scripts/start-render-xorg.sh`, then `scripts/rigshot.sh <scene>.mjs`. That wrapper composites a real
   headless WebGPU render on the 5060 Ti and forces WebGPU onto the same GPU as the compositor
   (`MESA_VK_DEVICE_SELECT`) — without that, screenshots come back blank (software display can't read the
   GPU swapchain) or die with `DEVICE_LOST` (cross-GPU present). Look at the PNGs yourself before claiming
   a scene works. (`xvfb-run` does NOT composite WebGPU — that trap cost prior sessions.)
5. **No-fudge:** every number traces to physics or is openly flagged (placeholder / unknown IC / resolution
   IOU). If physics disagrees with a hypothesis, record that (docs/31 is the template) — do not tune a dial
   to force the outcome.
6. **Record changes:** design → `docs/NN` · what-happened+proof → `JOURNAL.md` (newest-first, What/Why/
   **Verified**) · consumer delta → `CHANGELOG.md [Unreleased]` · standing context → memory. A substantive
   change usually touches docs+JOURNAL+CHANGELOG together.
7. **Merging is yours to do:** `main` carries an active ruleset (1 approving code-owner review +
   `code_quality` + 90% `code_coverage`). Robin: *"I set these rules up for outside contributors when/if
   we have them. Since we don't yet we have impunity."* → merge with `--admin`. Do not ask each time.
8. **Commit** `area: imperative subject (docs/NN)` (lowercase area). **Deploy only when asked:**
   `./scripts/deploy.sh` (full suite green first) → integrity.bothead.net (PUBLIC).
