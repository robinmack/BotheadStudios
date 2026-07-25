# Contributing to Integrity engine

Thanks for your interest! This is an early-stage, ambitious project — a browser game engine that
simulates matter from real physical properties. Contributions of all kinds are welcome.

## Project shape

- **Rust core → WASM** (`crates/`): everything performance-critical — the voxel matter store,
  material model, MLS-MPM solver, self-gravity, Rapier coupling, and the `wgpu` renderer.
- **WGSL shaders** (`shaders/`): GPU compute (MPM, Barnes-Hut) and render pipelines.
- **TypeScript host** (`web/`): the thin browser shell — canvas, input, UI. No simulation logic.
- **Docs** (`docs/`): research and architecture notes.

## Guiding principles

1. **Density is the single source of truth.** Material behavior, destruction, and gravity all
   derive from per-voxel density + material parameters. Avoid per-material `if` special-casing —
   if rock and grass behave differently, it should be because of their *parameters*, not branches.
2. **Emergence over authoring.** Prefer physical rules that produce the right behavior to hand-tuned
   scripted effects.
3. **Permissive licenses only.** Dependencies and referenced code must be MIT / Apache-2.0 / BSD /
   zlib. **Do not** copy GPL/LGPL code (you may still learn from its ideas). New contributions are
   accepted under the project's MIT license.
4. **Keep the host thin.** Simulation and rendering live in Rust; TypeScript orchestrates only.

## Getting set up

Prerequisites: [Rust](https://rustup.rs/) with the `wasm32-unknown-unknown` target,
[`wasm-pack`](https://rustwasm.github.io/wasm-pack/), and Node.js.

```bash
rustup target add wasm32-unknown-unknown
cd web && npm install && npm run dev
```

Requires a **WebGPU-capable browser** (recent Chrome/Edge/Firefox, or Safari 26+).

## Before opening a PR

- `cargo fmt` and `cargo clippy` clean. CI enforces `cargo fmt --check`, on **stock rustfmt defaults** —
  there is no `rustfmt.toml`, so your editor's format-on-save already agrees with us.
- **One-time, after cloning:** `git config blame.ignoreRevsFile .git-blame-ignore-revs`. The tree was
  formatted in a single sweep on 2026-07-25; that file makes `git blame` look through it to whoever last
  changed the code's meaning. GitHub's blame view does this automatically.
- `cargo test` passes (solver unit tests, Barnes-Hut vs O(N²) accuracy checks, etc.).
- Explain *what physical behavior* your change produces and how you verified it.

## Discussion

Open an issue for design discussion before large changes — the architecture is still forming and
early alignment saves rework.
