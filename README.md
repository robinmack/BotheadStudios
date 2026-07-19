# BotheadStudios

Open-source game projects by BotheadStudios. This is a **monorepo** — each top-level directory is
a self-contained project with its own README, build, and docs.

## Projects

### [`integrity-engine/`](integrity-engine/) — the Integrity engine

An OSS **browser game engine with real Newtonian physics at its core**. Matter is simulated as
aggregates of particles with mass and density; material behavior *emerges* from density (rock vs.
dirt vs. grass), terrain is destructible all the way down, and the world's own aggregate mass
produces real self-gravity (`F = ma`). Stack: Rust → WASM, a custom `wgpu` WebGPU renderer, and
Rapier rigid bodies, with a thin TypeScript host.

**Status:** pre-alpha — Phase 0 (scaffold + first pixel) complete, `v0.1.0`. See the project's
[README](integrity-engine/README.md), [roadmap/JOURNAL](integrity-engine/JOURNAL.md), and
[CHANGELOG](integrity-engine/CHANGELOG.md).

## License

[MIT](LICENSE) across the monorepo. Each project also carries its own license file for clarity;
where a project's license differs, that project's file governs its directory.
