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
- **Two scene structs** in `lib.rs`: `Engine` (terrain band, GPU-compute debris) and `OrbitDemo` (space
  band, CPU `Aggregate` debris — the giant impact / birth-of-the-Moon).
- **The key fact:** the physics *laws* are already unified and scale-invariant (`granular::Contact`,
  the SPH kernel, `Furrow` excavation, `plough_loft`, `Body`, `LayeredBody`); the *solvers and containers*
  are FORKED (CPU `Aggregate` f64 vs voxel-`World`/GPU f32; four integrators; Earth-as-rigid-boundary vs
  Earth-as-particles). Do NOT add a new per-scene particle path — extend the shared one. See docs/32 §4.
- **The physics gap:** there is **no condensed-matter EOS** (Tillotson/Birch–Murnaghan). Solids resist
  compression via a linear-elastic contact penalty; planet densities are declared constants. This is the
  keystone of the realignment (docs/32 §5, docs/33).

## Hard rules (do not violate)

1. **Work in your worktree** (`.claude/worktrees/.../integrity-engine`), never the main checkout.
2. **NEVER run `cargo fmt`** — the crate isn't rustfmt-conformant; it reformats the whole tree. Edit by
   hand. (`CONTRIBUTING.md` says otherwise for outside contributors; the working rule is do-not-run.)
3. **Test:** `bash scripts/test.sh --fast [filter]` (inner loop) · full `bash scripts/test.sh` before any
   deploy (~145 tests). O(n²) measurement tests are `#[ignore]` (run `--ignored`). Accelerated code is
   always pinned to its exact/brute-force reference so speed never changes the answer.
4. **Rig-watch every visual claim:** `npm run dev` + `npm run wasm`, then
   `xvfb-run -a node web/rig/<scene>.mjs` (headed Chromium — headless can't composite WebGPU). Look at the
   screenshots yourself before claiming a scene works.
5. **No-fudge:** every number traces to physics or is openly flagged (placeholder / unknown IC / resolution
   IOU). If physics disagrees with a hypothesis, record that (docs/31 is the template) — do not tune a dial
   to force the outcome.
6. **Record changes:** design → `docs/NN` · what-happened+proof → `JOURNAL.md` (newest-first, What/Why/
   **Verified**) · consumer delta → `CHANGELOG.md [Unreleased]` · standing context → memory. A substantive
   change usually touches docs+JOURNAL+CHANGELOG together.
7. **Commit** `area: imperative subject (docs/NN)` (lowercase area). **Deploy only when asked:**
   `./scripts/deploy.sh` (full suite green first) → integrity.bothead.net (PUBLIC).
