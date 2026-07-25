# BotheadStudios

Open-source game projects by BotheadStudios. This is a **monorepo** — each top-level directory is
a self-contained project with its own README, build, and docs.

## Projects

### [`integrity-engine/`](integrity-engine/) — the Integrity engine

An OSS **browser game engine with real Newtonian physics at its core**. Matter is simulated as
aggregates of particles with mass and density; the goal is material behavior, destruction, and
self-gravity all emerging from that matter under one law. The discipline behind the goal: the
engine treats fudges as bugs, every known deviation sits on a public conformance ledger with the
test that closes it, and negative results ship in the journal. Stack: Rust → WASM, a custom
`wgpu` WebGPU renderer, hand-written WGSL compute for the physics, and a thin TypeScript host.

**Status:** pre-1.0, under heavy development. The flagship scene is Birth of the Moon: a GPU SPH
giant impact with a sourced Tillotson equation of state that accretes a proto-lunar disk in the
browser, live at [integrity.bothead.net](https://integrity.bothead.net). See the project's
[README](integrity-engine/README.md), [JOURNAL](integrity-engine/JOURNAL.md), and
[CHANGELOG](integrity-engine/CHANGELOG.md).

## License

[MIT](LICENSE) across the monorepo. Each project also carries its own license file for clarity;
where a project's license differs, that project's file governs its directory.
