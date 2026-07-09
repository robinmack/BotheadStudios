# Changelog

All notable changes to `greenfield-engine` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
See [`docs/03-versioning.md`](docs/03-versioning.md) for our versioning policy — it matters
because **we are our own first customers** and pin exact engine versions in our games.

## [Unreleased]

## [0.1.0] — 2026-07-08

First milestone: **Phase 0 — scaffold & first pixel.** The full Rust → WASM → `wgpu` → canvas
pipeline is live, driven by a thin Vite/TypeScript host.

### Added
- Cargo workspace with the `engine` crate (`cdylib` + `rlib`) compiled to WASM via `wasm-pack`.
- `Engine` WASM API: `Engine.create(canvas)`, `render()`, `resize(w, h)` — a `wgpu` WebGPU
  device that clears the canvas with a pulsing color each frame.
- Vite + TypeScript host (`web/`) that loads the WASM, sizes the canvas, and pumps
  `requestAnimationFrame`, with a graceful "WebGPU unavailable" message.
- Project meta: MIT license, `README`, `CONTRIBUTING`, `JOURNAL`, this changelog, and two
  research reports under `docs/` surveying prior art and reusable OSS building blocks.

### Notes
- Pinned to `wgpu` 24.0.5. WebGPU-only backend to keep the WASM small.
- **Public API is unstable** while we're pre-1.0 (see versioning policy).

[Unreleased]: https://example.invalid/compare/v0.1.0...HEAD
[0.1.0]: https://example.invalid/releases/tag/v0.1.0
