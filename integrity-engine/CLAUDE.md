@AGENTS.md

<!-- The line above IMPORTS the vendor-neutral contract. `AGENTS.md` holds the rules that must hold no
     matter whose coding agent is working — numbering, merge method, the gates, how to resolve someone
     else's conflict — because this repo has more than one contributor and their assistants have to agree
     with each other. It is imported rather than duplicated so the two can never drift; everything below is
     the Claude-specific detail on top of it. -->

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
   **Check `data/materials.json` before claiming anything about a material's behaviour** — intuition about
   substances feels like knowledge, so it never prompts a measurement. (Limestone and concrete were filed
   as "does not melt"; both melt under confining pressure, which is exactly the impact regime. And eleven
   materials had no thermal data while three call sites invented three different specific heats to cover
   it.) If an entry is silent, source it; an unknown must stay unknown at the boundary.
   **SOP: any new substance — solid, liquid OR GAS — gets its properties sourced and catalogued in
   `data/materials.json` before use**, with `sources` filled in. Gases are materials: the engine derives a
   specific gas constant from molar mass and a scale height from that, so a CO₂ atmosphere is genuinely
   more compact than an air one — but only if CO₂ is in the table.

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
- ★ **ONE SURFACE GEOMETRY (docs/63, 2026-08-02).** Both scenes draw a planet's surface as
  `terra::segment` — a disc on the sphere whose angular radius is simply what is visible from the eye,
  centred on what the camera LOOKS at. It replaced the cube-sphere globe AND the tangent ground cap in
  Terra and in the space band, and deleted everything that mediated between those two meshes: the
  cross-fade, the depth-fight lift, the "may I skip the globe" test, and the tier ladder. Robin's rule is
  the reason: *"The Earth should be the Earth, the Moon the Moon, no matter how close or how far the
  camera pans"* — a camera that picks WHICH Earth you get is Law IV inverted. **Do not add a second
  surface mesh for a range of distances; change the segment's extent.**
- ★ **Measured elevation STREAMS (`terra::tiles`).** The shipped raster is 19.5 km/texel, so below ~20 km
  altitude the frame sits inside one texel. The engine names the tiles it needs and the host fetches them
  (AWS terrarium PNG, CORS-open); a bounded 3×3 patch follows the camera. Where tiles cover, generated
  relief keys off the TILE pixel, not the raster.
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

0. **Every scene has a "Send Shot" button, and no longer has to remember to.** Use
   `web/src/share-view.ts` — do NOT write a second capture path. A scene calls `createShareView(canvas, …)`
   and hands the button to the HUD (`hud.add("actions", share.button)`), then calls `share.afterPresent()`
   **immediately after it presents** (a WebGPU canvas is only
   readable while its drawing buffer is current; capture anywhere else silently yields a blank image).
   The frame is POSTed to `/__shot` and written to `shots/shot-<ts>.png`, which is how a picture gets
   from a scene to whoever is reading the repo. `web/rig/share_button.mjs` asserts the button exists AND
   that a real PNG lands, on every scene; `web/rig/hud_layer.mjs` asserts it is in the HUD's own layer,
   which is what makes "every scene" structural rather than remembered. **The space band used to build a
   LOOK-ALIKE with its own `mkBtn` and leave `share.button` unused** — a second implementation of the one
   thing this rule exists to make singular. That is why the rig checks the layer, not just the text.

1. **The main checkout belongs to the human** (`~/workspace/BotheadStudios`). Persistent or parked
   worktrees stay banned, and the 2026-07-19 reasons were real: a duplicated `node_modules` per tree,
   a shared stash stack that different sessions can pop out from under each other, and branches that
   quietly diverge in directories nobody is looking at. Transient, tool-managed worktrees for parallel
   agent work ARE permitted, because each of those costs is avoided by construction when the bounds
   hold, and the bounds are the rule: one task per worktree; the worktree is removed when its task
   ends; no stash use in a worktree, ever; every branch a worktree produces becomes a PR and dies at
   merge. (The 2026-07-19 rationale also rested on "this is a single-developer project that is not
   doing multi-agent work"; that premise has expired. Two contributors and their agents now work this
   repo, which is exactly the isolation worktrees existed for. What went wrong before was parking
   them, not the isolation.) Branch, commit, push, PR; never commit to `main` directly.
   **Keep the branch list at `main` alone as the steady state** (Robin, 2026-07-20, stated twice), now
   read for two people plus their agents: feature branches are short-lived, one per in-flight task,
   merged and deleted (`gh pr merge N --merge --delete-branch`), then `git fetch --prune`.
   ★ **`--merge`, never `--squash`** — measured over this nine-step integration: keeping merge commits
   held each step at 2-8 conflicted files, because the contributor's own commits stay reachable from
   `main` and every later step merges against a recent base. Squashing strands the fork and re-inflates
   the next step toward the 12-conflict "their whole main at once" case. It also rewrites the SHA that
   `.git-blame-ignore-revs` names. Do NOT
   leave branches parked; the other person's in-flight work lives in their open PRs, where it is
   visible and reviewable, not in parked branches. Work worth keeping but not merging (measurements, evidence, a salvaged
   tool) becomes an **annotated tag** `archive/<name>` whose message records WHY: same commits,
   `git show archive/<name>`, zero branch clutter. Five such branches were retired this way on
   2026-07-20.
2. **RUN `cargo fmt`; CI gates on it** (changed 2026-07-25 at Sean's request — this rule said the exact
   opposite for most of the project's life).
   ~~SUPERSEDED 2026-07-25 — do not follow: "never run cargo fmt; the crate isn't rustfmt-conformant, it
   reformats the whole tree."~~ That was true, and it made the tree drift further from conformant every
   day, so the cost of ever fixing it only grew. **The tree is formatted now and `cargo fmt --check` is a
   CI job, so it cannot drift again.**
   *(The superseded wording is kept struck-through and marked because it survived in this file, in
   `CONTRIBUTING.md` and on a contributor's branch simultaneously — deleting it silently is how two of them
   came back. Anyone grepping for the old rule should land on this line and see it is dead.)*
   **STOCK defaults — there is no `rustfmt.toml`, deliberately.** A custom config was measured first
   (`max_width=110` + `use_small_heuristics="Max"` reproduced the hand style at net +362 lines instead of
   +5,782) and rejected: the 110 came from a width distribution dominated by COMMENT lines, which rustfmt
   never reformats (`wrap_comments` is nightly-only and off). Code lines alone sit at p50=36, **p90=84** —
   inside the default 100. So the custom width was fitted to prose the formatter ignores. Robin's call:
   the defaults implement the official Rust Style Guide, most rustfmt options are nightly-only precisely to
   discourage configuring it, and a newcomer should meet a codebase that looks like every other one.
   ★ **`git blame` is mitigated, and it was verified.** The sweep is listed in `.git-blame-ignore-revs`.
   Run **`git config blame.ignoreRevsFile .git-blame-ignore-revs`** once per clone (GitHub does it
   automatically). Checked on a line rustfmt split: plain blame credits the formatter, blame with the file
   credits the real authoring commit and still follows the `greenfield-engine/` rename.
   ★ Anything added to that file must be formatting ONLY. A blame-ignored commit is one nobody will ever be
   shown, so a decision hidden inside it is hidden for good.
3. **Test:** `bash scripts/test.sh --fast [filter]` (inner loop) · full `bash scripts/test.sh` before any
   deploy (410 run by default, measured 2026-07-24). O(n²) measurement tests and GPU-requiring benches
   are `#[ignore]` (22 of them: `hydrostatic.rs` 9, `impact.rs` 8, `aggregate.rs` 2, `gpu_gravity.rs` 2,
   `gpu_host.rs` 1; run `--ignored`). Accelerated code is always pinned
   to its exact/brute-force reference so speed never changes the answer. `gpu_sph.rs`'s PHYSICS is still
   verified out-of-process by `tools/sph-verify` (which carries its own replica of the structs), but the
   module is no longer invisible to the suite: it compiles on every target since 2026-07-20, and its three
   shader-facing layouts are pinned to `sph_step.wgsl` in-crate.
   ★★ **The FULL run now compiles `mod app` for wasm32, and that is a GATE, not advice** (added
   2026-07-25). The scene structs (`Terra`, `OrbitDemo`, `Ground`) sit behind
   `#[cfg(target_arch = "wasm32")]`, so a native `cargo check --all-targets` is **green for code that does
   not build**. This file warned about that in prose for months and it still bit us: Sean's one-Earth step
   removed `EARTH_RADIUS_M`, three readers survived inside `mod app`, the native check reported **0
   errors**, and only the wasm target found them. Prose is not a gate. `--fast` skips it to keep the inner
   loop tight; the full run is the deploy gate, so nothing ships unchecked. CI has the same job
   (`mod app (wasm32)`), which fails in ~1 min instead of waiting for the full wasm-pack + Vite build.
   ★ The gate was verified by BREAKING `mod app` on purpose and checking the run went red (exit 101) —
   its first version piped cargo into `grep` inside an `if !`, so it printed the compiler errors and then
   exited 0. **A gate that reports a failure and passes is worse than no gate: it teaches you to trust
   it.** Verify a new gate by making it fail.
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
   **The SAME flag has an opposite trap, and it cost a session on 2026-07-24: uncapped rendering INVENTS
   stalls.** Terra with ~1,200 instanced billboards showed roughly one frame per second taking 450–520 ms
   inside `render()` while the median frame was 1.5 ms — a real, reproducible, and completely misleading
   measurement. Unpaced, the page ran at 170–350 fps and pushed several times more per second through
   `queue.write_buffer` than any vsynced browser ever will; **paced to ~60 fps in the rig, the same scene
   never exceeded ~10 ms and never stalled at all.** So: before believing a frame-time pathology, PACE the
   rig's render calls to ~16.7 ms and re-measure (`web/rig/terra_vsync_check.mjs` does exactly this, and
   the ablation ladder in `terra_price_stage.mjs` — physics / upload / draw priced separately — is the way
   to find which stage a real cost belongs to). A number from an uncapped rig is a number about the rig.
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
7. **Merging goes through the front door.** The old rule here was merge with `--admin`, on Robin's
   grounds that the ruleset existed for outside contributors we did not have: *"Since we don't yet we
   have impunity."* That premise no longer holds, so the bypass is retired. Two contributors exist,
   and the `ci` workflow runs the real deploy gate on every PR (`scripts/test.sh`, the full native
   suite, plus the wasm production build), so the branch ruleset's checks should be real ones that
   pass on their own. What replaces impunity: CI green, and the other person able to review async.
   Self-merge is allowed when the change is mechanical and green; when it is not, say so on the PR
   and wait for the other pair of eyes. Never `gh pr merge --admin`.
   ★★ **CURRENTLY NOT SATISFIABLE, and that is a fact rather than a licence (2026-07-25).** The rule above
   is the target and it is right. Today `main` requires **1 approving code-owner review**, CODEOWNERS is
   `* @robinmack @sean-reid`, **Sean's collaborator invite is still pending**, and GitHub will not request
   a review from a PR's own author — so `reviewDecision=REVIEW_REQUIRED` with an EMPTY requested-reviewer
   list, and nobody can approve. Waiting never clears it. Robin therefore authorised `--admin` explicitly
   for this integration. Name the exception on the PR when you use it; do not let it quietly become the
   habit it replaced. **The fix is Sean accepting the invite, not a better bypass.**
8. **Commit with `bash scripts/commit.sh <message-file>`** — write the message to a FILE first (an editor
   or a file-writing tool), never inline in a shell command. Messages here are long and full of the exact
   characters a shell eats: backticks around identifiers, `$`, `!`, quotes. A heredoc *looks* safe and is
   not — an unquoted one still does command substitution. That has bitten twice; the second time it
   silently deleted the subject of a sentence from a merge commit on its way to `main` (``​`pub mod arc;`
   was never declared`` became `" was never declared"`) and it was pushed before anyone read it back.
   The script also appends the `Co-Authored-By` trailer if missing and prints `parents=[a b]`, so a merge
   commit can be confirmed to still be a merge. `--amend` and `--no-trailer` are supported.
   Subject style: `area: imperative subject (docs/NN)` (lowercase area). **Deploy only when asked:**
   `./scripts/deploy.sh` (full suite green first) → integrity.bothead.net (PUBLIC).
