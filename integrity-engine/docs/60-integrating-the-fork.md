# docs/60 — Integrating Sean's fork: what we took, and every decision we made on his behalf

**Purpose.** Sean invited us to take his work rather than merge it himself, so we are resolving conflicts in
*his* code without him. This file is the audit trail of that: what came in, what conflicted, and what we
decided and why. **Every entry is a place he should be able to say "no, that's wrong" and be right.**

It exists because a merge resolution is a design decision wearing a chore's clothing. Nobody reviews the
resolution of a conflict the way they review a commit, and the resolutions are exactly where two people's
architectures get silently welded together.

## How his work is structured (and why this was easy)

He pre-split 156 commits into **nine cumulative branches**, `sean/upstream-1-ci` … `upstream-9-process`.
Cumulative, not independent: `upstream-1` is 1 commit, `upstream-9` is all nine. Taken **in order**, each step
is small; taken as one merge of his `main`, it is not:

| taken as | conflicted files |
|---|---|
| his whole `main` in one go | 12 |
| `upstream-1-ci` alone | 3 |
| `upstream-2-docs` on top of 1 | 2 |
| `upstream-3-sph-live-drop` on top of 2 | 3 |

So each step is its own PR, stacked on the previous one (`--base integrate-sean-N`), and the diff a reviewer
sees is only that step's delta.

**Authorship is preserved throughout.** Every step comes in by `git merge`, never squash or cherry-pick, so
the commits stay `Sean Reid <seanreid.mail@gmail.com>` and count on his contribution graph.

## Why the histories diverged in the first place — our fault, worth not repeating

His fork branched from our `main` *before* we merged PR #81, and we merged #81 as a **squash**. Squashing
rewrote 40 commits into one, so his branch still carries ~15 commits authored by Robin that `main` now
contains only *inside* the squash. That is what turned a fast-forward into a conflict-resolution exercise.

**Merge commits keep a fork in sync; squash merges strand it.** Now that a collaborator works from forks, the
default for upstream merges should change.

## Decisions, step by step

### upstream-1-ci — "run the deploy gate and the wasm build on every pull request" (PR #84)

The CI this repo never had. 12 files, 3 conflicts.

| conflict | decision | why |
|---|---|---|
| `.gitignore` | **took his side** (deletes the `.github/workflows/` ignore) | That ignore existed *only* because our local `gh` token lacked the `workflow` scope. Committing CI is precisely what it was blocking, so it goes. It had previously been written as `.github/`, which swallowed the whole directory and is why the repo had no CODEOWNERS either. |
| `CHANGELOG.md` | **union**, ours first | Both sides appended at the top. A journal conflict's resolution is both entries; dropping either deletes somebody's record. |
| `JOURNAL.md` | **union**, ours first | Same. |

Also superseded: a stale untracked `.github/workflows/ci.yml` from 2026-07-08 pointing at
`working-directory: greenfield-engine`, a directory that no longer exists.

### upstream-2-docs — "the public story matches the code" (PR #85)

2 conflicts, both `CHANGELOG.md` / `JOURNAL.md`, both unioned. No judgement calls.

### upstream-3-sph-live-drop — "state the crossing law and retire the CPU debris leftovers"

3 conflicts. Two were the usual journals. The third is the one that needs his eyes:

**`lib.rs` — the scene-resolution const block.** His commit replaces our block
(`SPH_SHOCK_WINDOW_S`, `SCENE_DEBRIS_N`, `SCENE_CAP_N`, `SCENE_IMPACT_N`) with a single
`MOONLET_UNIS_N: usize = 1536`.

- **Decision: keep both.** `SPH_SHOCK_WINDOW_S` is load-bearing on our side — it is the quiescence gate the
  de-resolution merge depends on (docs/44 §6), and it is deliberately one constant shared with the docs/42 dt
  coarsening so "the shock is finished" has one definition. Taking his side outright would delete it.
- **Decision NOT taken, and why it matters:** `1536` is exactly `SCENE_IMPACT_N` (512 + 1024), and his own
  comment says the pool is "sized to the pool the retired CPU debris cloud used". The obvious Law V move is
  `const MOONLET_UNIS_N: usize = SCENE_IMPACT_N;` — one number, one home. **We wrote that, then reverted it.**
  Robin's correction: our side moved the disk to **GPU** SPH, so `SCENE_IMPACT_N` is a *physics particle
  count*, whereas `MOONLET_UNIS_N` is a *render pool* sized against the **CPU** path his commit retires. They
  agree only because the CPU-era pool happened to equal the GPU count we now run. Coupling them would assert
  a dependency that does not exist and would resize his draw pool whenever anyone tuned GPU resolution.
  The literal stays, with the coincidence documented at the call site.
- **Sean: if the pool genuinely should track resolution, say so and we will derive it.** We left it
  independent because we could not establish that it should.

### upstream-4-gpu-gravity — "dispatch the GPU kernels; race-free COM sweep for Metal" (PR #87)

7 files, **2 conflicts**, both `CHANGELOG.md` / `JOURNAL.md`, unioned. **No judgement calls needed — but two
Law checks were run before concluding that, and both are worth recording as things that were NOT wrong.**

1. **Is this a second answer to "what is the gravitational acceleration"?** (Law II) No. He adds a GPU
   Barnes–Hut (`GravityField`, `accelerations_tree`) and wires it into the CPU `aggregate` path, and he pins
   it the way CLAUDE.md rule 3 requires: `the_gpu_tree_matches_the_cpu_tree_within_the_theta_bound` and
   `the_gpu_tree_opened_fully_recovers_the_direct_sum`. That makes it an ACCELERATION of the CPU tree, pinned
   to its exact reference, not a rival implementation. The SPH self-gravity in `sph_step.wgsl` is a different
   container and legitimately specialized (docs/46 §1).
2. **Is `theta` duplicated between Rust and WGSL?** (Law V / `laws::SINGLE_SOURCE`) No, and the design is
   better than ours was: the shader reads `P.theta` from its params, `THETA` exists only as a Rust-side
   default, and `accelerations_tree_with` takes it explicitly. Theta has one home and flows to the GPU as a
   uniform. The worry was real — a kernel hardcoding an opening angle would silently ignore a caller's theta —
   it simply was not what he did.

Also confirmed: his rewritten `bh_gravity.wgsl` still satisfies
`pinned_constant_tests::the_shaders_gravitational_constant_matches_the_engines`, the guard added the same day
for the shader copies of `G`. `379/379`.

## Open questions for Sean

1. **Two camera-control APIs.** His branch adds `arc_press`/`arc_stop`/`arc_distance_m`/`camera_state`/
   `pan_tangent` (a scripted camera path); ours adds `set_camera_pose`/`clear_camera_pose` (an observer hands
   the engine a pose, docs/59). Both answer "who places the camera", which by Law II should have one answer.
   Ours is the more general primitive; his is a path *expressed* in poses. Likely resolution: his arc becomes
   a caller of the pose API. **His call — it is his feature.**
2. **`display_scale()` vs `DISPLAY_SCALE`.** He turned the constant into a function and made the globe draw
   camera-relative (`vp_rel`). That is strictly more advanced than what we have and it lands in the same lines
   of `Terra::render` as our matter rendering. We have not integrated it yet; steps 5–9 will hit it.
3. **He may already have built our next task.** His new tests —
   `camera_relative_eye_round_trip_is_submillimetre_at_planet_radius`,
   `globe_model_translation_stays_subpixel_where_the_coarse_globe_is_drawn`,
   `triplanar_anchor_restores_surface_fixed_texture_phase` — describe model-matrix anchoring, which is exactly
   the mechanism we identified as the blocker for the ground-tier ladder (docs/59, one tier at 45 ms/frame
   because every tier is rebuilt every frame). If so, his work supersedes ours there and should win.
4. **`FOV_Y` twice.** He added `pub const FOV_Y` in `fly_camera`; we added `DEFAULT_FOV_Y` plus `View::fov_y`,
   for the same reason (the FOV was duplicated into a shader's billboard sizing). Converging fixes, one name to
   pick. No physics at stake.

## Standing rule this establishes

**Resolve his conflicts against the Laws, not against the compiler.** The `MOONLET_UNIS_N` case is the
example: the compiler was perfectly happy with the wrong answer *and* with the right one, and with the
false-dependency version in between. `docs/00`'s pre-flight is what separated them, and Robin's knowledge of
which path moved to the GPU is what corrected the pre-flight's first answer.
