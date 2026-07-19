# OSS Physics/Simulation Building Blocks for a Browser Newtonian-Matter Engine

> Research report #2 for the greenfield browser physics engine.
> Destined for: `~/workspace/integrity-engine/docs/02-oss-building-blocks.md`

**Scope:** 3D; permissive licenses only (MIT/Apache/BSD/zlib), GPL/LGPL flagged; interactive real-time in-browser ("as many particles as feasible at 30-60 fps"); determinism nice-to-have; composable building blocks, not turnkey.

**Bottom line:** No OSS library gives "destructible voxel matter + real self-gravity" out of the box. The realistic architecture is three layers: (1) a permissive rigid-body/collision engine as the discrete-object backbone, (2) a **custom WebGPU compute** particle/continuum layer (MLS-MPM or XPBD) for aggregate/destructible matter, and (3) a **WebGPU Barnes-Hut** pass for self-gravity. The candidate set for layer 1 and reference code for layers 2-3 are all MIT/Apache/BSD/zlib — GPL is largely avoidable.

## 1. Comparison tables

### Rigid-body / general engines (browser bindings)

| Library | Tech / browser path | License | Model type | Maturity |
|---|---|---|---|---|
| **Rapier** (Dimforge) | Rust → **WASM**; official `@dimforge/rapier3d` | **Apache-2.0** | Rigid body, colliders, joints; **optional cross-platform determinism** | High — active, first-class JS bindings |
| **Jolt** + **JoltPhysics.js** | C++ → **WASM** (Emscripten); `jolt-physics` npm | **MIT** (core + port) | Rigid body, multicore, soft bodies, characters | High — ships in Horizon Forbidden West, Death Stranding 2 |
| **Bullet / ammo.js** | C++ → **WASM/asm.js** | **zlib** (port inherits) | Rigid + soft body 3D | Mature but aging; ammo.js less maintained |
| **PhysX** + **physx-js-webidl** | C++ → **WASM** (WebIDL, PhysX 5.6.1) | core **BSD-3**, binding **MIT** | Rigid, joints, articulations. **CUDA/GPU excluded in WASM** | Active. No GPU particles/fluids in browser |
| **cannon-es** | Pure **JS** | **MIT** | Rigid body 3D | Maintained; lightweight, lower fidelity |
| **Box2D / planck.js** | JS | **MIT** | **2D only** | Mature — *technique reference* |
| **Matter.js** | Pure **JS** | **MIT** | **2D only** | Mature — *technique reference* |

### Particle / continuum / granular

| Approach / project | Tech / browser path | License | Model type | Maturity |
|---|---|---|---|---|
| **webgpu-ocean** (matsuoka-601) | **WebGPU compute** (WGSL) | **MIT** | **MLS-MPM** + SPH; strongest in-browser matter reference | Active; ~100k particles integrated GPU, ~300k mid-range, real-time |
| **jeantimex/fluid** | **WebGPU compute** | MIT | SPH + PIC/FLIP grid solver | Demo-grade reference |
| **taichi.js** (AmesingFlank) | JS → **WebGPU** compute compiler | **MIT** | GPU compute DSL (MPM/SPH) | **Maintenance stale** (~2022-23); reference only |
| **taichi_mpm / taichi_elements** | Native/CUDA, **not browser** | MIT | MLS-MPM reference (1B particles offline) | Research code — algorithm source |
| **PositionBasedDynamics** (ICG) | C++, **not browser** | MIT | PBD/XPBD reference | Mature research lib — reference only |
| **THREE-XPBD** (markeasting) | **JS** (three.js) | MIT | XPBD rigid/soft | Small/demo |
| **DEM** (granular) | No browser-native OSS; Chrono is dual-GPU/native | Chrono BSD-3 | Discrete Element Method | Native/HPC only |
| **NVIDIA Flex** | Core closed; folded into PhysX 5 GPU binaries | not OSS | PBD fluids/cloth/soft | **Not usable in browser** |

### Browser GPU compute + N-body

| Item | Tech / browser path | License | Notes |
|---|---|---|---|
| **WebGPU compute shaders** | Baseline (Chrome/Edge/Firefox/Safari 26+) as of Jan 2026 | — | The path for serious particle work; WebGL2 "compute" obsolete |
| **WebGL2 transform-feedback GPGPU** | Fallback | — | Legacy; fallback only |
| **piellardj/water-webgpu** | WebGPU | **MIT** | "Up to a million" colliding particles on GTX 1660 Ti (basic collision) |
| **markaicode 1M demo** | WebGPU + spatial hash | blog | 1M @60fps high-end, 500k @60fps mid, 100k @30fps mobile |
| **Barnes-Hut N-body** (jheer, Elucidation) | JS/WebGPU octree | mixed demos | O(n log n); CPU ~100k bodies; **no dominant OSS browser lib** |
| **Fast Multipole Method** | — | — | True O(N) but complex; overkill until N very large |

## 2. Recommended foundations

**1. Rapier (Apache-2.0) — rigid-body/collision backbone.** Rust→WASM, official `@dimforge/rapier3d`, and uniquely **optional bit-level cross-platform determinism** (de-risks future multiplayer/replay). Handles discrete objects (intact voxel chunks, debris, characters). Clean collider/query API to couple into the particle layer. Best default backbone.

**2. Jolt via JoltPhysics.js (MIT) — alternative/high-fidelity backbone.** Battle-tested core (Horizon Forbidden West, Death Stranding 2), both core and WASM port MIT, multicore, active JS ecosystem (isaac-mason, @react-three/jolt). Choose over Rapier for max solver fidelity/soft-body over Rapier's turnkey determinism. Pick #1 **or** #2, not both.

**3. Custom WebGPU MLS-MPM/XPBD particle layer on the webgpu-ocean pattern (MIT).** Where "aggregate/destructible matter" lives; no drop-in library exists. **matsuoka-601/webgpu-ocean (MIT)** is the strongest in-browser reference: MLS-MPM in WGSL, ~100k particles integrated / ~300k decent GPU, real-time. MLS-MPM is a hybrid particle/grid **continuum** method natively modeling mass, density, elasticity, plastic/fracture (destruction) — exactly the matter model wanted. Pair with XPBD (THREE-XPBD / ICG PositionBasedDynamics as references, MIT) for constraint-based aggregates. Own this layer as first-party code; taichi.js reference only (stale).

**Self-gravity: custom WebGPU Barnes-Hut (build it).** No mature OSS browser N-body lib — field is demos. Build a WebGPU Barnes-Hut octree (O(n log n)). Crossover: direct O(N²) on GPU beats CPU Barnes-Hut up to ~N≈50k, so brute-force WebGPU kernel is fine at small N; Barnes-Hut earns its keep above that. FMM is a later optimization.

## 3. Honest scale limits

- **Rigid bodies (CPU-WASM):** realistically **low thousands** active at 60fps; falls off as contacts rise. Cannot represent millions of voxels as rigid bodies. Destruction must degrade into a particle/continuum field, not more rigid bodies.
- **WebGPU MLS-MPM:** **~100k particles integrated, ~300k mid/decent GPU**, real-time (webgpu-ocean). Simple position/collision particle demos push ~1M (piellardj, markaicode) but those are *not* full constitutive matter. Expect **100k–300k particles of real destructible matter** today; 1M only for cheap effects. Mobile ~1 order of magnitude lower.
- **PhysX/WASM caveat:** GPU features (Flex-derived fluids/soft/particles) **not available in browser** — CUDA stripped. PhysX buys CPU rigid bodies only.
- **NVIDIA Flex:** dead end for browser (GPU binaries can't run in WASM).
- **Self-gravity:** CPU JS Barnes-Hut ~100k bodies; WebGPU needed for large N + interactive; you write it yourself.
- **Combined budget:** you won't get max particles *and* full self-gravity *and* rigid coupling at their individual ceilings — shared GPU/frame budget. Plan a tunable particle cap (~100k–250k "matter" particles), gravity via Barnes-Hut, rigid bodies reserved for large intact chunks + player-facing objects.

### License notes
Entire recommended stack is permissive: Rapier (Apache-2.0), Jolt+JoltPhysics.js (MIT), PhysX core (BSD-3)/binding (MIT), Bullet/ammo.js (zlib), cannon-es/planck.js/Matter.js (MIT), webgpu-ocean/taichi.js/THREE-XPBD/PositionBasedDynamics (MIT). **GPL avoidable** — traps are outside this stack (GPUSPH and some academic SPH/DEM are GPL). LGPL-via-WASM static-linking ambiguity doesn't bite (no LGPL top candidates) but keep the rule before adopting new deps.

## Sources
- Rapier: https://rapier.rs/ · https://github.com/dimforge/rapier · https://github.com/dimforge/rapier.js/ · https://www.npmjs.com/package/@dimforge/rapier3d
- Jolt: https://github.com/jrouwe/JoltPhysics · https://github.com/jrouwe/JoltPhysics.js/ · https://www.npmjs.com/package/jolt-physics
- ammo.js/Bullet: https://github.com/kripken/ammo.js · https://www.tapirgames.com/blog/open-source-physics-engines
- PhysX: https://github.com/fabmax/physx-js-webidl · https://developer.nvidia.com/blog/open-source-simulation-expands-with-nvidia-physx-5-release/
- Flex: https://github.com/NVIDIAGameWorks/FleX
- MLS-MPM browser: https://github.com/matsuoka-601/webgpu-ocean · https://80.lv/articles/check-out-this-real-time-3d-fluid-simulation-implemented-in-webgpu
- SPH/PIC-FLIP: https://github.com/jeantimex/fluid · https://tympanus.net/codrops/2025/02/26/webgpu-fluid-simulations-high-performance-real-time-rendering/
- Taichi: https://github.com/AmesingFlank/taichi.js/ · https://github.com/yuanming-hu/taichi_mpm
- XPBD/PBD: https://github.com/markeasting/THREE-XPBD · https://github.com/InteractiveComputerGraphics/PositionBasedDynamics · https://matthias-research.github.io/pages/publications/XPBD.pdf
- WebGPU particle demos/scale: https://markaicode.com/webgpu-physics-simulation-1m-particles/ · https://github.com/piellardj/water-webgpu · https://lisyarus.github.io/blog/posts/particle-life-simulation-in-browser-using-webgpu.html
- WebGPU baseline: https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API
- N-body / Barnes-Hut: https://en.wikipedia.org/wiki/Barnes%E2%80%93Hut_simulation · https://www.mysimulator.uk/blog/deep-dive-nbody-gravity.html · https://jheer.github.io/barnes-hut/
