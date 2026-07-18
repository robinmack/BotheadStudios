# Stage 5 — migrate the scenes onto the one GPU particle path (docs/33 §4.5)

**Decision (Robin, 2026-07-17):** unify onto the **GPU SPH path** (`gpu_sph::GpuSph` + `shaders/sph_step.wgsl`),
retiring the CPU `aggregate::Aggregate` solver from the live scenes. Highest payoff (one path, high N
everywhere), highest risk (rewrites both scenes' physics). This doc is the increment sequence so the working,
deployed birth scene never breaks mid-migration — each step is verified (native test or rig-watch) and
committed before the next.

## Where we start (already done)
- **4c**: `GpuSph` runs the deformable-Earth impact in `OrbitDemo` (gated `sph_active`), rig-watch verified.
  It has NO read-back yet (render-only, zero-copy).
- **5 (EOS seam)**: `eos::Eos { Tillotson | IdealGas }`; `HydroBody` is EOS-agnostic.
- **Engine (terrain)** debris already runs on the GPU — but via `particle_step.wgsl` (GRANULAR contact), a
  DIFFERENT kernel from `sph_step.wgsl` (SPH-EOS). See the design note below.

## The one open design decision — surface to Robin before increment 4
`sph_step.wgsl` models matter as an **SPH-EOS continuum** (Tillotson pressure = the contact law). The CPU
`Aggregate` and `particle_step.wgsl` model it as **discrete granular contact** (grains bounce/pile/bond). The
realignment thesis (docs/23/24) is "one contact law at every scale" and casts SPH-EOS Tillotson AS that law —
so the end state is SPH-EOS everywhere and granular contact retired. BUT: a resting iron probe on terrain and
piling debris are things granular contact does well and SPH-EOS-with-a-boundary must re-earn. **Open:** does
the unified GPU path (a) go pure SPH-EOS (retire granular), or (b) gain granular contact as a second law on
the GPU (`sph_step.wgsl` + the `particle_step.wgsl` contact terms merged)? Increments 1–3 are agnostic to this;
increment 4 (terrain probe) forces it. Recommendation: (b) — keep both laws on the GPU, pick per-material/energy
(a grain at rest uses contact; a shocked continuum uses EOS), which is the honest "one engine, scale-adaptive".

## Increment sequence (each: verify → commit)
1. **GPU→CPU read-back for `GpuSph`** (PREREQ for everything). Two-phase async map (WebGPU forbids blocking —
   mirror `GpuParticles::begin_readback`/`take_readback`, `lib.rs:2160/2192`). Exposes particle pos/vel/u/prov
   to the CPU. *Verify:* a `gpu_disk_stats_json()` that reads back + measures the perigee-above-remnant disk
   provenance (port `tools/impact-run::measure_disk`) and shows it in the birth HUD; rig-watch the number.
2. **OrbitDemo birth scene ON `GpuSph`.** Make the "Birth of the Moon" scene build+relax+collide via `GpuSph`
   (extend `start_gpu_impact` into the real `start_birth` path), read back each frame for the momentum/HUD,
   and retire `moon_debris: Aggregate` from `OrbitDemo`. Keep the accretion operator (`accretion.rs`) for the
   geologic hand-off. *Verify:* rig-watch the whole birth→disk→(geologic) flow; disk provenance matches the
   offline `impact-run` ballpark.
   - **DONE 2a (scene framing):** `SPH_VIS_SCALE` + camera zoom so the impact is legible; rig-verified.
   - **BLOCKER (found 2a):** the CPU relax in `build_deformable_impact` runs synchronously on the wasm main
     thread and freezes page load if auto-started — so the birth scene can't yet DEFAULT to GpuSph. **Revised
     2b:** relax on the GPU (`cs_relax` exists — a few frames of damped stepping, no main-thread block) or a
     lighter/deferred CPU relax, THEN the birth scene can auto-start GpuSph. Also rewire the birth controls
     (Geologic hand-off via `accretion`, Replay → restart) before retiring `moon_debris`.
3. **Retire `body::Sphere`** (5c) — the tiny fork: the one live site (`lib.rs:1098`, the Engine probe's debris
   collision proxy) collides against the probe's actual particles, not a synthesized bounding sphere.
4. **Engine terrain probe/debris on the unified GPU path** (needs the design decision above). Merge the SPH-EOS
   and granular-contact kernels OR port the probe to SPH-EOS with a terrain boundary. *Verify:* terrain rig-watch
   (probe rests, debris piles, crater persists).
5. **Delete `aggregate::Aggregate`** once no scene uses it; move its still-needed tests (contact-conserves-
   momentum, thermo) onto the GPU path's CPU reference. **WGSL-from-Rust** (5d): make `sph_step.wgsl`'s law
   verifiably the same as the Rust `Eos`/SPH source of truth (extend `tools/sph-verify`).

## Guardrails
- The birth scene is DEPLOYED (integrity.bothead.net). Never commit a step that leaves it broken; rig-watch
  every visual step (CLAUDE.md #4). Keep the CPU `Aggregate` path alive until the GPU replacement for that
  scene is rig-verified, then retire in one commit.
- Physics faithfulness over cost (memory): the GPU path must reproduce the CPU physics it replaces (energy,
  momentum, the disk number) before the CPU path is retired — the `tools/sph-verify` / `impact-run` discipline.
