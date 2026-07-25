# One GPU particle container (docs/33 convergence, docs/46 §1)

**Status: in progress, 2026-07-21.** The first increment (the shared store) is described here and
implemented; the render-path half is scoped at the end and NOT done.

## The violation being closed

`docs/32` §4.1 records "two container universes" as conformance-ledger row 1. On the GPU that is literal:
`gpu_particles::GpuParticles` (granular, `GpuParticle` 80 B, `particle_step.wgsl`) and
`gpu_sph::GpuSph` (SPH, `SphParticle` 48 B, `sph_step.wgsl`) each carry their **own** copy of:

- a particle storage buffer plus `capacity`/`count` bookkeeping,
- clamp-to-capacity upload logic (`append`/`replace` vs the head of `upload`),
- and a two-phase asynchronous read-back (`begin_readback`/`take_readback`).

The read-backs were **byte-for-byte identical** apart from the element type and a debug label. The proof
that this is duplication and not coincidence: on 2026-07-20 the SAME latent defect — an
`Rc<Cell<bool>>` in the `map_async` callback, which compiles only for wasm — had to be found and fixed
**twice, once in each file**. A single bug appearing in two places is the definition of one answer
written down twice.

## What is NOT being unified, and why that is not a dodge

**The solvers stay separate.** `docs/46` §1 sanctions this explicitly: stiff granular contacts need a
semi-implicit integrator to stay stable, self-gravitating SPH needs a symplectic leapfrog to conserve
energy over orbits, and forcing one on both is unstable or ruinously slow. *The physics itself differs,
so the numerical treatment differs.*

So each container keeps its own pipelines, its own bind groups, and its own auxiliary buffers — the
heightfield/forces/render sub-cube expansion on the granular side, the EOS/acceleration/du-dt/Courant
signal buffers on the SPH side. **What was never physics is the allocator.** "How many particles do I
have room for, where does the next batch land, and how do I get them back to the CPU without blocking"
has exactly one right answer, and it had two implementations.

## The shape

`crate::gpu_store::ParticleStore<T>` — generic over the POD element, so the 80-byte granular grain and
the 48-byte SPH particle share the code without sharing a layout:

| | shared (`ParticleStore`) | specialized (each solver) |
|---|---|---|
| storage buffer + capacity/count | ✅ | |
| `append` / `replace` clamping | ✅ | |
| two-phase async read-back | ✅ | |
| compute pipelines, bind groups | | ✅ |
| auxiliary buffers | | ✅ |
| dispatch / encode order | | ✅ |

**The clamping arithmetic is extracted as pure functions** (`append_span`, `replace_span`) and tested
natively. That is where the silent bug lives: an off-by-one drops particles at the capacity boundary
with no error — matter vanishing, which no rendering check would catch. wgpu here is built with the
`webgpu` backend only, so a `ParticleStore` cannot be *instantiated* natively; the arithmetic can, and
that is the part with something to get wrong.

## Partly done (2026-07-24) — the engine now says WHAT to draw, if not yet how

One half of the render path turned out not to be about pipelines at all. `ground_scene` was constructing
`GpuParticle`s **by hand, twice** — once for grains, once for bodies in flight — reading albedo out of the
material table and calling the incandescence law itself. A third copy would have been needed for the entry
trail (docs/59) and a fourth for a swarm, and every copy is a place a scene can quietly disagree with the
physics about what is real, which inverts Law VI.

`crate::Drawn` is now the engine's answer to *what am I holding?* — physical facts only (where, how fast,
how big, what of, how hot; no colour, no brightness, no effect type) — and `GpuParticle::of_matter` is the
one physics→instance mapping. `Simulation::drawn()` and `flight::Flight::drawn()` report through it, so a
scene's render is one map over a list and a scene that can draw any of the engine's matter can draw all of
it. That is the property that lets a capability like the meteor swarm appear in a scene that knows nothing
about meteors.

It also closed a real hole: the ground scene capped GRAINS at its instance capacity and then pushed
meteors on top, so a full grain buffer plus a body in flight wrote past the end of the buffer. `drawn()`
emits matter in flight first and grains last, so capping one list cannot overrun it and a budget loses the
least informative matter.

**Still two, and still the real design question:** the granular side expands each grain into 8 sub-cubes
and SPH draws camera-facing billboards, and *Terra draws neither* — it has no instanced particle path at
all, which is what currently stands between the engine holding a meteor swarm and anyone seeing one.

## Not done yet — the render path

The mandate is "one GPU particle allocator **and render path** hosting both pipelines". This increment
does the allocator and the read-back. The render path is still two: the granular side expands each grain
into 8 sub-cubes (`cs_expand` → `render_buf`) drawn by the particle pipeline, while SPH draws
camera-facing billboards straight from the physics buffer (`sph_render.wgsl`, 48-byte stride). Those are
different *visual representations* of matter at wildly different scales, so unifying them is a real
design question — not the mechanical de-duplication this increment is — and it wants its own increment
and its own rig evidence.

## Done: the store landed, and terrain was deleted

`ParticleStore<T>` is in `crate::gpu_store` and BOTH pipelines use it. `GpuSph` and `GpuParticles` each
lost their private buffer, their `capacity`/`count` bookkeeping and their copy of the read-back; both
`begin_readback` bodies are now three-line delegations. Their solvers are untouched.

**Terrain (`Engine`) is gone** (Robin, 2026-07-21: *"I want that old model GONE"* — the first scene
designed, superseded). 1,516 lines out of `lib.rs`; 25 terrain-only rigs removed; the page, the vite
entry and the nav link removed. `lib.rs` 5,548 → 3,794.

## The finding that matters more than the refactor

**Deleting one scene required surgery on the engine.** Not a data file: 1,516 lines cut out of
`crates/engine/src/lib.rs`, a symbol removed from the crate's public API (`pub use app::Engine`), and a
build-system entry point. Robin's standing requirement is the opposite —

> scenes should have object definitions, assembly definitions, coordinates, etc… but should **not**
> require special mods of the engine itself.

So the cost of this deletion is the measurement of how far the scenes are from being disposable, and the
same is true of the two that remain: `OrbitDemo` and `Terra` are `#[wasm_bindgen]` structs *inside the
engine crate*, each with its own hand-written pipelines, uniforms and render loop. Adding a scene today
means editing the engine; removing one means editing the engine. That is recorded as **docs/46 ledger
row 14**, because it is a charter matter (a world is a world is a world — the engine should not know
which one it is showing), not a tidiness preference.

**What "converged" has to mean, concretely, for the next increment:** a scene is a DESCRIPTION the engine
loads — matter definitions by material, assemblies, placements, a camera — and the engine contains no
`struct` named after it. docs/43 (worlds-as-data) already built the schema half for `Terra`; the missing
half is that the SCENE, not just the world, becomes data. Until then "delete the scene" will keep meaning
"edit the engine".
