# Versioning policy

We (the engine's own games) are the **first customers** of `greenfield-engine`. That makes a
disciplined, predictable versioning scheme a first-class requirement from day one — a game must be
able to pin an engine version and know exactly what an upgrade will and won't break.

## Scheme: Semantic Versioning 2.0.0

Versions are `MAJOR.MINOR.PATCH` per [semver.org](https://semver.org/spec/v2.0.0.html).

### Pre-1.0 (where we are now)

While the public API is still forming, we are in the `0.y.z` range, where **`0.MINOR` may contain
breaking changes**. Our convention during this phase:

- **`0.MINOR.0`** — a completed roadmap milestone (a "Phase"). May break the API. Example:
  `0.1.0` = Phase 0 (first pixel), `0.2.0` = Phase 1 (voxel matter store), etc.
- **`0.MINOR.PATCH`** — bug fixes and non-breaking additions within a milestone.

Consumers (our games) should pin **exact** versions pre-1.0 (`=0.3.1`, not `^0.3`), because a
minor bump can break them.

### Post-1.0 (once the public API stabilizes for outside users)

Standard semver guarantees apply:

- **MAJOR** — incompatible/breaking API changes.
- **MINOR** — backward-compatible new functionality.
- **PATCH** — backward-compatible bug fixes.

We will cut `1.0.0` only when the core engine API (the WASM `Engine` surface + the material/scene
authoring API) is stable enough to support external games without churn.

## What counts as the "public API"

The versioned surface is:

1. The **WASM `Engine` interface** exported to JavaScript (`crates/engine`, see `engine.d.ts`).
2. The **TypeScript host API** any game builds against (`web/`).
3. The **material / scene definition format** (once it exists) — save files and material configs
   are data, and breaking their format is a breaking change.

Internal Rust module structure, shader internals, and private helpers are **not** part of the
public API and can change freely.

## Coordinated version numbers

Several files carry a version; they move together on every release:

- `crates/engine/Cargo.toml` — `package.version` (source of truth).
- `web/package.json` — mirrors the engine version.
- `CHANGELOG.md` — a dated section per release.
- A git tag `vX.Y.Z` at the release commit.

## Release checklist

1. Update `CHANGELOG.md`: move items out of `[Unreleased]` into a new dated `vX.Y.Z` section.
2. Bump the version in `crates/engine/Cargo.toml` and `web/package.json`.
3. Add a `JOURNAL.md` entry for the milestone.
4. Commit, then tag: `git tag -a vX.Y.Z -m "vX.Y.Z — <milestone>"`.
5. (Later, when published) publish the WASM package / release artifacts.

## Compatibility notes we must track

- **`wgpu` version** is a load-bearing external dependency; note its version in each release
  (WebGPU API churn has already bitten us — see `wgpu` 24.0.5 API differences in the journal).
- **Minimum browser** (WebGPU support level) is part of our compatibility contract; document it
  when it changes.
