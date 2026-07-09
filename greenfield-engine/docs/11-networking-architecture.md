# Networking — server-authoritative universe (a future possibility for large/shared worlds)

> Design note. **Not built, and not needed for single-player.** Today the engine is
> client-authoritative: one machine runs the whole sim and renders it. This note captures the model
> we'd adopt to scale up to **large, shared, persistent universes** — so we keep the engine
> *ready* for it without building it prematurely. Status: **possibility / design**.

## The idea (and the important distinction)

The appealing instinct: a **server holds the canonical "universe,"** and each **client is interested
in only its slice** (the region around its camera). That authority + interest-management model is
correct for shared/persistent worlds. The distinction that matters is *what flows over the wire*:

| Model | Server sends | Verdict |
|---|---|---|
| **Pixel streaming** (cloud-gaming) | rendered video frames of the client's view | ❌ for us — needs GPU servers + video encode, huge bandwidth per client, no offline/single-player, mod-hostile |
| **State / delta streaming** (recommended) | *changes* to the world state in the client's region (voxel edits, particle spawns, entity positions, events) | ✅ — bandwidth ∝ change, client renders locally, works offline, mod-friendly |

**Never** stream the geometry or the rendered view every frame — a destructible voxel+particle view
is millions of changing cells; that doesn't scale. Stream **deltas**, render **locally**.

## Architecture

```
SERVER (authoritative universe)                 CLIENT (its slice)
  • canonical world state + edits                 • local cache of the interest region
  • area-of-interest (AoI) manager                • local sim + PREDICTION (feels instant)
  • streams DELTAS within each client's slice ──► • local wgpu render
  • validates client actions, reconciles     ◄── • sends intents (dig here, move) 
```

- **Authority:** the server owns the truth; the client is authoritative for nothing but *predicts*
  locally so interactions (dig, drop) feel immediate, then reconciles against server corrections.
- **Interest management (the "slice"):** the client subscribes to the chunks around its camera; the
  server streams only changes in that set, and swaps the set as the camera moves.
- **Latency hiding:** client-side prediction + server reconciliation (the standard authoritative-
  multiplayer pattern), which is only tractable because of the next point.

## Why our stack is unusually well-suited

- **One Rust core, two targets.** The simulation compiles **native (server)** *and* **wasm
  (client)** — the *same* physics runs on both sides, so authoritative sim and client prediction
  can't drift due to different implementations.
- **Determinism.** We already prize it (Rapier's deterministic builds, no `Math.random`/wall-clock in
  the sim, the TDD suite). Deterministic, identical code on both sides makes prediction/reconciliation
  far simpler than the usual cross-engine reimplementation.
- **Sim/render separation.** The pure sim modules (matter store, gravity, matter solver) are already
  independent of the wgpu layer — the server needs the former, not the latter.

## Scaling the universe

- **Coarse authority, fine locally.** The server can hold a coarser authoritative state and let
  clients simulate fine detail (particles, exact fracture) locally within their slice — full MPM per
  player-view server-side does not scale.
- **Spatial sharding.** A galaxy-scale universe is partitioned (by region/body); a client connects to
  the shard(s) its slice overlaps. Bodies (planets) are natural shard boundaries.
- **Persistence.** The authoritative world state lives in the same store as the asset DB direction
  (`docs/05`) — Postgres for durable state, with hot regions in memory.

## What we do *now* to stay ready (already true)

1. **Deterministic sim** — no wall-clock/RNG in the core; tests lock behavior.
2. **Sim/render split** — pure tested modules vs. the wasm/wgpu host.
3. **Chunked world as data** — the voxel store is already chunk-shaped and serializable.
4. **Rust core** — trivially retargetable to a native server binary.

No code changes are required today; networking becomes its own phase once the single-player sim
(Phases 1–4) is proven.

## Open questions

1. **Delta granularity & compression** — per-voxel vs per-chunk diffs; snapshot + delta cadence.
2. **Prediction/reconciliation scope** — which subsystems predict client-side (movement, digging)
   vs. server-only (canonical fracture outcome).
3. **Shard boundaries & handoff** — crossing between bodies/regions without hitches.
4. **Trust model for OSS/mods** — how much a (moddable) client is allowed to simulate authoritatively.
