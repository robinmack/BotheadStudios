# GPU-compute particle simulation — parallelize the hot loop

> Design note. The particle/matter step is **embarrassingly parallel** — every particle integrates
> independently — so it belongs on the GPU, not a single wasm thread. This is the engine's stated
> north-star (Rust→WASM core, custom `wgpu` renderer, **zero-copy sim↔render**) and the `docs/08`
> plan ("move the hot loops to WGSL"). Status: **design; a CPU O(1) stopgap shipped** (`docs/22` §Stopgap).

## Why (measured)

A big impact throws thousands of ejecta. `matter::step` was querying the full ~1000-point gravity field
**per particle, per substep** → ~10⁸ ops/frame on one wasm thread → single-digit FPS (Robin's M4, after
a meteor). Particles are independent; a GPU runs thousands of them at once.

## Stopgap (shipped)

Debris gravity now uses the **O(1) centre-of-mass approximation** instead of the full field — ~1000×
cheaper, single-threaded. Native test suite dropped 10.7s → 1.1s. Cost: slight inward drift of
off-centre debris (`docs/08`). This buys playable framerates until the GPU path lands; it is not the
real fix.

## The real fix — WebGPU compute

Move the particle step into a **WGSL compute shader**, one invocation per particle:

- **State on the GPU.** Particles (`pos, vel, material, temp`) live in a `storage` buffer; the gravity
  mass-points and a terrain heightfield in read-only `storage`/uniform buffers. The step never touches
  the CPU.
- **`@compute` step kernel.** `@workgroup_size(64)`; each thread integrates one particle (gravity from
  the field, drag, terrain collision against the heightfield, settle flag). Dispatch
  `ceil(N/64)` workgroups per substep.
- **Zero-copy sim↔render.** The same particle buffer is bound as the **instance buffer** for the debris
  draw — the GPU simulates and renders from one buffer, no CPU round-trip. (This is the architecture the
  engine was designed around.)
- **Voxel edits stay hybrid (for now).** Fracture/dig/deposit mutate the voxel grid — harder to
  parallelize (atomics / a separate compute pass). Keep those on the CPU at first; only the *step* (the
  per-frame hot loop) moves to the GPU. Settling back into voxels can be a periodic CPU readback of
  "resting" particles.
- **MLS-MPM later.** The same compute infrastructure is the home for the grid transfers of MLS-MPM
  (`docs/08`) — this is a prerequisite, not a detour.

## Alternative considered — wasm threads (rayon)

`SharedArrayBuffer` + web workers + `wasm-bindgen-rayon` would parallelize on the M4's CPU cores, but it
needs cross-origin isolation (COOP/COEP headers) and only uses the CPU. For an independent-per-particle
workload feeding a renderer, **GPU compute is the better fit** and reuses the `wgpu` device we already
have. Keep threads in reserve for CPU-bound stages (e.g. surface-nets meshing).

## Rollout

1. Particle buffer + a compute step kernel for the debris (gravity + integrate + heightfield collision),
   dispatched from `render`. Verify parity with the CPU step on a fixed scene.
2. Bind the particle buffer directly as the debris instance buffer (zero-copy).
3. Restore the full gravity field (now affordable) and raise the particle cap.
4. Extend to the aggregate particles (`docs/21`) so a shattered moon's thousands of fragments run on
   the GPU too.
