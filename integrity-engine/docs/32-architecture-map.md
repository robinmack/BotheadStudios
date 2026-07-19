# Architecture map — for future Claude sessions (2026-07-17)

A durable orientation to the Integrity engine, written so a Claude session starts *oriented*
instead of rediscovering machinery. Read this before exploring. All refs are `file.rs:line` — click them.

**One-crate mental model.** Everything performance-critical is ONE Rust crate (`crates/engine`) compiled to
WASM via `wasm-pack`, sharing a single `wgpu` device with the renderer (zero-copy). `web/` is a thin
TypeScript+Vite host. Public demo: **integrity.bothead.net** (see docs/29). The engine's charter is
*everything is matter; one contact law + one gravity law govern it at every scale* (docs/23/24/28) — a tire,
a meteor, and Theia are meant to be the same physics at different scale/energy/material.

**The single most important architectural fact:** the physics *laws* are already largely unified and
scale-invariant; the *solvers and containers* are forked. See §4. Do not add a new per-scene particle path —
extend the shared one.

---

## 1. The two scene structs (the top of the call tree)

Two independent `wasm-bindgen` structs in `mod app` (`lib.rs:62`), each its own scene family, wgpu device,
and render loop:

- **`Engine`** (`lib.rs:223`) — the **terrain band** (`terrain.html`, host `web/src/main.ts`). A 96 m Earth
  surface patch. Debris runs on the **GPU compute** shader (`particle_step.wgsl`). Probe = a CPU cohesive
  `Aggregate`.
- **`OrbitDemo`** (`lib.rs:2382`) — the **space band** (`orbit.html`, `birth.html`, `twomoons.html`, host
  `web/src/orbit.ts`). N-body Sun/Earth/Moon(s); a giant impact shatters a body into a CPU `Aggregate`
  debris cloud. **No GPU compute** — all particle physics is CPU (`aggregate.rs`).

The fork between these two particle paths (GPU terrain grains vs CPU space aggregate) is the central thing a
refactor toward "one particle module" must dissolve.

---

## 2. Physics core (the celestial / particle modules)

- **orbit.rs** (661) — N-body point-mass mechanics. `Body{pos,vel,mass}` (`:20`, the UNIVERSAL particle,
  f64, reused everywhere), `G` (`:18`), `accelerations` (O(N²) 1/r², `:28`), `verlet_step` (KDK symplectic,
  `:45`), `perigee` (`:83`), `resolve_contact` (inelastic COM merge, `:106`), `swept_first_contact` (CCD,
  `:141`), `contact_velocity` (dt-independent true impact velocity from vis-viva + angular-momentum, `:173`),
  `binding_energy` (`:203`). IOU: `resolve_contact` drops dissipation as heat (flagged placeholder).
- **aggregate.rs** (1788) — **THE particle-cloud simulator.** `Aggregate` (`:41`): `particles:Vec<Body>`,
  per-particle `temps`/`mat_ids`, `softening`, cohesive `bonds`, self-gravity (`gravity_source` extended
  1/r²+Gauss interior, `gravity_bodies` Sun+planets, `self_gravity` via Barnes–Hut), contact
  (`contact`/`per_grain_contact`/`contact_ref_mass`), the rigid **`boundary`** penalty sphere +
  `boundary_hole` crater + `boundary_force/torque_sum` (Newton-3rd-law reaction → planet spin), vapor/SPH
  (`vapor`, `contact_gas`, `boil_k`, `vapor_rs/h/latent_k/rho`), `source` provenance. Force kernel
  `accelerations()`/`accelerations_masked()` (`:498/:507`): uniform g → extended source (Gauss interior) →
  Barnes–Hut self-gravity (θ=0.5) → gravity_bodies → NeighborGrid contact (phase-dispatched) → boundary →
  bonds → SPH vapor `P=ρ·R_s·(T−L_v/c)`. `step()` (KDK+fracture+thermo, `:918`), `apply_thermo` (PdV cool,
  Stefan–Boltzmann, phase flip, dissipation heat, `:937`), block timestep `particle_timesteps`/`step_block`
  (`:774/:804`), `deposit_impact` (`:424`), `drain_settled` (demote settled mass+L back to planet, `:882`).
- **granular.rs** (742) — **the contact force law of record** (soft-sphere DEM). `Contact`
  (radius/stiffness/normal_damp/friction/tangent_damp/cohesion/coh_range/shock, `:63`),
  `contact_from_material` (stiffness=E·r/m, damping from restitution, cohesion σ·A/m, `:38`), `Contact::mix`
  (per-pair cross-material, idempotent, `:153`), `contact_accel` (the pair force, `:172`),
  `contact_dissipation` (exact heat partner, `:229`), `plough_loft` (momentum-conserving downrange
  excavation drag, `:110`), `terrain_contact_resolve` (NON-injecting heightfield constraint — the
  settling-storm fix, `:299`). The Rust `contact_accel` is hand-mirrored in `particle_step.wgsl:132`.
- **bhtree.rs** (250) — Barnes–Hut octree, O(N log N) softened self-gravity (docs/30). `build`/
  `accelerations`/`accelerations_active` (block-timestep subset), θ=0.5, `BRUTE_BELOW=1024`. RMS<1% vs
  brute force (verified).
- **neighbors.rs** (146) — `NeighborGrid` spatial hash, O(N) EXACT short-range pairs (`build`/
  `for_each_pair`, `BRUTE_BELOW=512`). The exactness is the invariant the perf work rests on.
- **impact.rs** (2024) — mutual-impact builder (the giant-impact physics-of-record). `ExcavSurface`
  (Curved|Flat — the ONE surface for planet AND terrain patch, `:41`), `Furrow` (downrange excavation
  ellipsoid + declared H-H ejection, `:117`; `ejection` `:222`), `ejecta_energy_scale` (EXACT KE cap,
  `:284`), `furrow_target_grains` (fills furrow with ρ·V layered grains, `:321`),
  `build_impact_debris_scaled` (materializes impactor + cap, applies `earth_omega` spin + `plough_loft`,
  wires the full `Aggregate`, `:421`). **Most-flagged module:** `Furrow::ejection` is an explicit
  RESOLUTION IOU (sub-grain shock declared from cited scaling, to be DELETED as N rises); the isotopic-crisis
  test (`:732`) documents that spin alone can't Earth-enrich the disk under a rigid boundary (docs/31).
- **planet.rs** (439) — `LayeredBody`: a planet DECLARED as concentric real-material layers;
  gravity/pressure/phase COMPUTED. `layer_at`/`temperature_at`/`enclosed_mass`/`gravity_at` (Gauss),
  **`pressure_at`** (analytic hydrostatic dP/dr=−ρg, midpoint-integrated inward — the VALIDATION TARGET for a
  particle planet, `:130`), `phase_at` (Simon–Glatzel melt + Clausius–Clapeyron boil). Profiles `earth()`/
  `moon()`/`sun()`/`theia()`. IOU: layer densities are declared PREM constants (compression is data, not
  computed — see the EOS gap §5).
- **materials.rs** (338) — `Material` (density, fracture_strength, youngs_modulus, friction, restitution,
  cohesion, optical, `thermal:Option<Thermal>`) loaded from embedded cited `data/materials.json`. `Thermal`
  with `melt_point_at`/`boil_point_at`. IOU: friction/restitution/albedo are flagged constitutive
  placeholders; liquid `fracture_strength` forced to 0 (removed the "fluid stronger than granite" fudge).
- **damage.rs** (193) — cross-scale classification: `classify(energy_density,m)` → Intact/Fractured/Melted/
  Vaporized by ρ(cΔT+L) thresholds; `crater_volume=E/σ`; `ground_effect` (disruption at binding energy).
- **emission.rs** (57) — `incandescence(temp_k)` black-body-ish glow (hot matter emits, isn't lit).
- **tides.rs** (355) — spin as bookkept angular momentum; secular tidal migration (validated vs 3.8 cm/yr
  lunar recession), Radau–Darwin flattening, J₂, `Moonlet` geologic-time mergers (Hill/Gladman).
- **gravity.rs** (181) — voxel-world self-gravity (`MassField`, one-level Barnes–Hut over block³ mass
  points, f32) — the terrain-band analogue of bhtree; a DISTINCT particle system from `Aggregate`.
- **isotropy.rs** (181) — test-only grid-isotropy guardrail (docs/15): gravity + dig stay direction-
  independent (lattice is a sampling grid, not matter).

## 3. Terrain / voxel / atmosphere modules

- **matter.rs** (2061) — CPU voxel-matter solver + the **bulk↔grain bridge**. `Particle`/`MatterSim`;
  promotion `dig`/`impact`/`materialize_region`/`materialize_furrow`/`materialize_steep_terrain`/`collapse`;
  de-resolution `deposit_resting_grain` (**single source of truth**, shared by CPU step AND GPU readback,
  `:795`); `step` (COM gravity + terrain snap-contact + settle — **no grain-grain contact on CPU**, that's
  GPU-only). All representation changes conserve matter + inject no energy (grain born at rest at its voxel
  centre).
- **world.rs** (1360) — the voxel matter store (`voxels:Vec<u16>`) + layered-Earth generator + terrain
  queries. `surface_height_bilinear` (the collision surface that MIRRORS `particle_step.wgsl::terrain_h`,
  `:148`), `bulk_height` (bulk heightfield everywhere), `find_structurally_unsupported` (cantilever support
  L_max≈√(σt/ρg) — basalt holds ~22 m, emergent from strength), `ocean_pressure` (continuous with the
  atmosphere column). Layered strata grass→basalt→peridotite→iron.
- **mesher.rs** (1004) — surface meshing (Surface-Nets smooth terrain, curved `build_earth_cap`, sea,
  instanced debris/probe). Purely representational — "physics unchanged, this is the visual." All meshes emit
  one `Vertex{pos,nrm,col,mat}` → one triplanar pipeline.
- **body.rs** (284) — rigid `Sphere` under the field (semi-implicit Euler + voxel collision). NOTE: the live
  probe is actually an `Aggregate` (`lib.rs:2251`); `Sphere` is the debris-collision proxy + native tests —
  a SECOND rigid-body representation.
- **atmosphere.rs** (809) — **the SPH air field + the hydrostatic-balance template.** `sph_w`/`sph_dw` (the
  ONE cubic-spline kernel, shared with aggregate's vapor, `:39/:52`), `gas_contact_from_material` (stiffness
  from isentropic bulk modulus K=γ·P_ref, `:19`), `AirField` (`:96`): `compute_density` (ρ=Σm·W + mirror
  ghosts), `accelerations` (symmetric momentum-conserving `a=−Σm(P_i/ρ_i²+P_j/ρ_j²)∇W`, EOS `P=ρ·R_s·T`,
  `:220`), `relax_step` (damped settling). VERIFIED 3D hydrostatic balance (`:643`). **This is the machinery
  a self-gravitating condensed-matter planet reuses** — swap the EOS call, replace uniform gravity with
  Barnes–Hut self-gravity (exists in aggregate.rs), drop the box ghosts, add the energy equation (exists in
  apply_thermo). Points b+d already exist — only the EOS is new (§5).
- **texture.rs** (247) — procedural per-material textures from optical properties, zero image assets.

## 4. The unification map — laws shared, solvers/containers forked

**Already shared (scale-invariant), do NOT duplicate:**
- `granular::Contact`+`contact_accel` — used by CPU `Aggregate`, the GPU shader (hand-mirror
  `particle_step.wgsl:132`), and (gas sibling) `atmosphere`. `Contact::mix` unifies cross-material.
- `sph_w`/`sph_dw` — one kernel for `AirField` AND aggregate vapor.
- `Furrow`+`ExcavSurface`+`ejection`+`ejecta_energy_scale` — ONE excavation primitive for terrain meteor
  (`Engine::meteor` `lib.rs:867`) AND Theia (`OrbitDemo::step_substep` `lib.rs:3357`).
- `plough_loft` — shared terrain↔giant-impact downrange drag.
- `deposit_resting_grain` — one de-resolution primitive (CPU + GPU).
- `Body` (universal particle), `Material`/`LayeredBody` (universal matter description).

**Forked (what a unified module must absorb):**
1. **Two container universes** — `Aggregate` (`Vec<Body>`, f64, celestial) vs voxel `World`+`MassField`+
   `MatterSim` (f32, terrain). Same laws, different data structures/integrators. A tire lives in one, Theia
   in the other.
2. **Four integrators over one law** — GPU trapezoidal-implicit (terrain grains, `particle_step.wgsl:394`),
   CPU Euler settle-only (`MatterSim::step` — no grain-grain contact), CPU Verlet/KDK block-timestep
   (`Aggregate`), SPH relaxation (`AirField`, a standalone struct duplicating aggregate's SPH).
3. **The rigid-boundary fork** — in an impact, Earth is simultaneously a few materialized grains AND a rigid
   `boundary` penalty sphere + monopole/Gauss `gravity_source` + `boundary_hole`. This is docs/28 root-cause
   #1, in code (`aggregate.rs`): Earth's bulk is NOT the same particles as its debris. impact.rs:814 marks it
   as the lower bound that blocks the isotopic-crisis fix.
4. **Two rigid-body reps** — `body::Sphere` vs the cohesive-`Aggregate` probe.
5. **Manual WGSL mirror** — the GPU contact law is hand-kept in sync with the Rust one (guarded by
   `tools/gpu-verify`), not generated from it.
6. **GPU vs CPU physical depth** — the GPU stepper has grain-grain contact + heightfield + cooling but NO
   self-gravity, NO SPH, NO EOS, single global material, f32. The CPU `Aggregate` has all of that but is
   N≈512–1536 bound. Unification ≈ the GPU path + the aggregate's four missing physics, f32 local-frame.

## 5. The EOS / pressure inventory (and the gap)

Every pressure law that exists:
1. **SPH vapor** `P=ρ·R_s·(T−L_v/c)` (ideal gas) — `aggregate.rs:731`(force)/`:965`(energy).
2. **Atmosphere ideal-gas EOS** `P=ρ·R_s·T`, stiffness K=γ·P_ref — `atmosphere.rs:19/:96/:220`.
3. **Contact penalty** `f=k·overlap`, k=E·r/m (Young's modulus) — `granular::contact_accel`. Linear-elastic
   penalty, NOT a thermodynamic EOS.
4. **Analytic hydrostatic P(r)** dP/dr=−ρg — `planet::pressure_at:130`. Uses declared PREM densities;
   density does NOT respond to the pressure it computes.
5. **Boundary/bond springs** `f=k·penetration` / `k·(dist−rest)`.

**Condensed-matter EOS (Tillotson / Birch–Murnaghan / bulk-modulus for solids): CONFIRMED ABSENT.** Solids
resist compression via a linear-elastic contact penalty; planet layer densities are declared constants
(compression is data, not computed). Shock-compressed rock has no way to develop pressure from density. This
is THE missing piece for a scale-invariant "matter under its own pressure" model, and the keystone of the
realignment (docs/33) and the full-particle-Earth build.

## 6. Scene wiring — the birth-of-the-Moon path (the canonical trace)

`orbit.ts` `OrbitDemo.create` → `start_birth` (`lib.rs:2897`, swaps body-2→Theia, inbound geometry
b=1.46·contact → emergent ~46° obliquity, zeroes proto-Earth spin) → per-frame `demo.advance(dtS)`
(`:3184`, wall-clock fixed-dt substeps) → `step_substep` (`:3243`): `verlet_step` → `swept_first_contact`
(`:3270`) → `contact_velocity` → **`build_impact_debris_scaled`** (`:3357`, Theia+Earth profiles,
512+1024 grains, converts `spin_l`→ω) → `moon_debris:Aggregate` → **`step_block`** (`:3430`, Barnes–Hut
self-gravity + grid contact + SPH vapor + boundary) → momentum-exact two-way coupling back to Earth,
boundary torque → `spin_l` (day length), tidal/J2 kicks, `drain_settled` demotes rested matter → Earth →
`push_snapshot` → `render` (`:3623`, samples `RENDER_LAG_S` behind live; draws Earth as a 512-grain oblate
shell, debris provenance-tinted blue=Earth/orange=Theia) → HUD `disk_stats_json` (`:2963`).

## 7. Render + GPU compute

- **Render** — terrain `Engine` (`space`? no): `sky.wgsl` (Rayleigh tri) → `world.wgsl` (triplanar
  material, water Fresnel) → `particles.wgsl` (instanced cube per grain). Space `OrbitDemo`: only
  `space.wgsl` (lit sphere, per-instance model matrix; every element — Sun, Earth shell, crater walls, moon,
  debris — is the same unit `sphere_gpu` drawn per `UniformSlot`, zero-scale = hidden).
- **GPU compute `particle_step.wgsl`** (terrain `Engine` ONLY): `cs_grid_clear`/`insert` (spatial hash) →
  `cs_forces` (grain-grain contact from 27 neighbour cells, builds implicit tensor) → `cs_integrate`
  (directional trapezoidal θ=0.70 implicit contact solve + non-injecting `terrain_resolve` + cooling) →
  `cs_expand` (1 grain → 8 render sub-cubes). Has the hard GPU parts (spatial hash, stable implicit solve,
  4-pass barriers, non-blocking readback). Does NOT do self-gravity / SPH / EOS / per-grain material.
  Extending it to the space aggregate at N~10⁵ needs exactly those four + f32 Earth-relative framing.

## 8. Workflow a Claude MUST follow

1. **Work in the given worktree** (`.claude/worktrees/.../integrity-engine`), never the main checkout.
2. **NEVER run `cargo fmt`** — the crate isn't rustfmt-conformant; it reformats everything. (Note the
   tension: `CONTRIBUTING.md:40` tells outside contributors to keep fmt clean, but the project's working
   rule is do-not-run. Edit by hand.) Keep `cargo clippy` clean if you touch Rust.
3. **Test:** `bash scripts/test.sh --fast [filter]` for the inner loop; **full `bash scripts/test.sh`
   before any deploy** (~145 tests). O(n²) measurement/sweep tests are `#[ignore]`, run with `--ignored`.
   Accelerated code is always pinned to its exact/θ-bounded brute-force reference so speed never changes the
   answer.
4. **Rig-watch any visual claim** — start vite (`npm run dev`), rebuild wasm (`npm run wasm`), run the
   relevant `web/rig/*.mjs` under `xvfb-run -a node rig/<scene>.mjs` (headed Chromium; headless can't
   composite WebGPU), and actually look at the screenshots. Robin is not the test runner.
5. **No-fudge / resolution-IOU ethos** (docs/23/17): physics drives the render, never the reverse; no
   scripted outcomes, no tuned dials. Every number traces to physics or is openly flagged as a placeholder/
   unknown IC. If physics disagrees with the hypothesis, RECORD that (docs/31 is the template).
6. **Record changes:** design rationale → new/edited `docs/NN` (next is docs/34); what-happened+proof →
   `JOURNAL.md` (newest-first, What/Why/Verified); consumer-facing delta → `CHANGELOG.md [Unreleased]`;
   standing cross-session context → memory. A substantive change usually touches docs+JOURNAL+CHANGELOG.
7. **Commit** house style: `area: imperative subject (docs/NN)` (lowercase area: `impact:`, `compute:`,
   `docs:`, `ui:`, `rig:`). End Claude commit messages with the Co-Authored-By trailer.
8. **Deploy only when asked:** `./scripts/deploy.sh` (full suite green first) → static build → nginx :8080
   → Cloudflare tunnel → integrity.bothead.net (PUBLIC).

## 9. Docs index (the design record — `docs/NN-slug.md`)

01 prior-art · 02 oss-building-blocks · 03 **versioning** (SemVer; games pin exact) · 04 materials-model ·
05 data-pipeline · 06 material-modules · 07 material-taxonomy-and-objects · 08 **adaptive-resolution-and-
clumping** (represent matter at the coarsest resolution that still behaves right; refine/coarsen LOD) · 09
agentic-object-authoring · 10 robustness-and-common-pitfalls · 11 networking · 12 textures · 13
**scale-relative-simulation** (cost scales with what's observable; simulation LOD, not just render LOD) · 14
validation-demonstrations · 15 **representation-invariant** (the cube is a lattice, not matter) · 16
**unified-dynamics-and-awake-set** (every dynamic solid is the same matter in one awake-set loop) · 17
honest-appearance-and-observer-frame · 18 unified-deformation-and-damage (a bullet, a splash, the Moon =
same code, different params/LOD) · 19 lod-adaptive-damage · 20 impact-thermodynamics · 21
bodies-as-particle-aggregates · 22 **gpu-compute-particles** (the particle step belongs in WGSL compute;
zero-copy sim↔render; terrain done, space aggregate is the undone step 4) · 23 **everything-is-matter-north-
star** (the no-fudge charter) · 24 **emergent-impact** (ejecta from real compression→rebound, not scripted
v) · 25 layered-planets-and-atmosphere · 26 atmosphere-as-matter · 27 birth-of-the-moon · 28 **missing-
impact-physics** (audit; root cause #1: Earth is a rigid boundary) · 29 deployment · 30 **accelerated-
compute-module** (grid + Barnes–Hut + block timesteps) · 31 **isotopic-crisis** (spin isn't the lever;
needs Earth-as-matter) · 32 this map · 33 **architecture-realignment** (the plan).
