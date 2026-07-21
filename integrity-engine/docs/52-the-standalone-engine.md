# docs/52 — the standalone engine

**Status: begun 2026-07-21.** Increment 1 (a GPU without a browser) is done and verified on hardware.

> Robin, 2026-07-21: *"And this is why we make the engine standalone, with external definitions."*

## Why, in measured form

docs/51 recorded the argument as evidence rather than preference. Deleting ONE scene (terrain) left
`matter::MatterSim`, `resolution::ResolutionField` and the voxel `world::World` with **zero** production
consumers, and the granular GPU pipeline reachable only from a compute-only diagnostic — **while every
test kept passing**. Capability was reachable only *through* a scene, so deleting a scene silently
unwired verified physics.

A standalone engine cannot fail that way. The engine owns the laws and exposes capability as an API;
definitions are external files; deleting a scene deletes a FILE and the engine is untouched.

It also answers the other question already on the table
(`integrity-engine-native-platforms-ok`): if WASM becomes constraining, a standalone engine is already a
program that a native host can drive — the browser being one host rather than its home.

## Increment 1 — a GPU with no browser (done)

**The problem.** Every path to the engine's GPU code ran through a `#[wasm_bindgen]` scene struct handed
an `HtmlCanvasElement`. "The engine" and "the browser page" were the same object. And `wgpu` was pinned
crate-wide to `features = ["webgpu", "wgsl"]` — a backend that exists only in a browser — so the engine
could *compile* natively (proved by the docs/50 lifts) but could never *run*.

**The fix, and why it is not a feature flag.** Backends are now chosen by TARGET:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies.wgpu]      # WebGPU only — a browser has nothing else
[target.'cfg(not(target_arch = "wasm32"))'.dependencies.wgpu] # wgpu defaults (Vulkan on Linux, by platform)
```

A cargo *feature* would have been the wrong tool: **features unify across a build graph, targets do
not.** Feature unification leaking a native backend into the browser build is the exact hazard that
pushed `tools/gpu-verify` into its own separate workspace. With target tables it cannot happen — nothing
building for wasm32 can see the native table. (There is no `vulkan` cargo feature in wgpu 24; on Linux it
is enabled by platform. `default = ["wgsl", "dx12", "metal", "webgpu"]`.)

**`gpu_host::GpuHost::headless()`** acquires a device with no surface and no canvas. Adapter choice is
explicit: `PowerPreference::HighPerformance` cannot discriminate between two discrete GPUs (it takes
whichever enumerates first) and cards three generations apart report byte-identical limits, so there is
nothing to auto-select on. CPU adapters (llvmpipe/SwiftShader) are filtered out — they "work" and then
report software timings as if they were hardware. **With several GPUs and no hint it REFUSES rather than
guessing**, the lesson `tools/gpu-verify` already paid for.

**Verified on hardware, not by compiling.** "It builds for a native target" proves nothing — wgpu's types
exist without a backend, which is why the docs/50 lifts compiled natively all along. The test acquires a
real device, then compiles and creates a pipeline from the SHIPPING `shaders/sph_step.wgsl`:

```
adapter: NVIDIA GeForce RTX 5060 Ti (DiscreteGpu, Vulkan)
test gpu_host::tests::the_engine_can_run_its_own_shader_with_no_browser ... ok
```

`#[ignore]`d so a machine with no GPU does not fail the suite; run with
`INTEGRITY_ADAPTER=5060 cargo test -p engine --ignored gpu_host`.

**The browser is unaffected** — the constraint this could have broken. wasm check clean, `wasm-pack`
clean, and both remaining scenes rig-verified rendering (birth 67,219 B, terra 64,003 B, against the
1,883 B blank-page control).

## What is NOT done

Honest scope: this makes the engine able to *hold a GPU* on its own. It is not yet standalone.

1. **The scenes are still `#[wasm_bindgen]` structs inside the crate** (`OrbitDemo`, `Terra`), each with
   its own pipelines and render loop, so a new KIND of scene is still an engine edit — the remaining half
   of docs/46 ledger row 14.
2. **There is no native host**: no window, no surface, no input. Headless compute only. A native host
   needs `winit` + a surface, which is a real dependency decision, not a mechanical step.
3. **The orphaned systems are still orphaned** (ledger row 15). The standalone shape is what lets them be
   re-consumed by a definition instead of by a scene struct; that is the next increment and the one that
   pays row 15 off.
