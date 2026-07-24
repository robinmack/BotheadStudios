# docs/58 — The generic body: no "Earth", no "Moon", just matter

**Status: in progress — several choices now BUILT (progress note added 2026-07-23).**

> Where the 8 choices stand in code, so this doc no longer reads as an untouched plan:
> - **#1** (mass/radius/I derive from matter): BUILT — `planet::LayeredBody::moment_of_inertia`/`from_layers`, tested.
> - **#3** (spin is a vector `ang_mom`): BUILT on the de-resolution path — `accretion::Body.ang_mom`/`Clump.ang_mom`, conserved through `absorb`.
> - **#4** (`particalize` reads real layers + per-material EOS): BUILT — `HydroBody::particalize`/`particalize_cap`; `accretion::sample_layers` reads the layering back out.
> - **#5** (provenance = source index; albedo render): PARTIAL — `prov` is a generic source-body index in assembly, but the disk stats and `sph_render.wgsl` still use a hardcoded two-tone (warm/cool), NOT the source material's real albedo. The albedo half is the remaining work.
> - **#7** (retire the CPU `Aggregate`): DONE for the scene — no scene references it; `aggregate.rs` survives only for tests. **#2, #6, #8** (role-not-name, scene-as-data, `laws.rs` name-freeness capstone) remain the open frontier.

**Robin (2026-07-22):** *"The engine should itself have no concept of 'Earth',
'Moon', just the objects and assemblies passed into it… this should work for all particles, all planets,
etc; with the engine making particalization choices based on energy, scale of view."*

This is not new vision — it is docs/23 (*everything is matter, one Earth*), docs/46 ledger **row 14**
(*a scene is code, should be data*), docs/13 (scale-relative), docs/33/50 (one container, realignment).
This doc states the concrete **data model** those imply and the realignment toward it, so it is executed
once, not re-derived. Each choice below carries its **Law** and, where a machine can check it, its
**enforcing test** (Robin: *"ensure each design choice aligns with the Laws… When possible enforce laws
with tests"*).

## The violation being closed

The `OrbitDemo` scene knows "Earth" and "Moon" in ~200 places (measured): `EARTH_RADIUS_M` ×114,
`EARTH_MASS` ×49, `MOON_*` ×40, a single global `spin_l` that *means* `bodies[1]` ×26, `bodies[0]=Sun`
/ `bodies[1]=Earth` / `bodies[2..]=moons` across ~40 sites, and matter fetched by the literal name
matching a body's role (`planet::earth()`/`moon()`/`theia()`). A body's matter, mass, radius and spin are
each sourced **three different ways** depending on the code path (`create()` constants vs `load_world`
matter-derived vs `impact_def`). That fragmentation IS the Earth/Moon-specificity.

(Honest scope note: much is already generic — `tides.rs` takes `L, mass, radius` as arguments;
`impact.rs`'s production builder takes `earth_mass, earth_radius` as parameters, its constants are test
fixtures; `assemble_from_relaxed_at` is geometry-agnostic. The work is **consolidation**, not invention.)

## The model

A body IS one record — matter plus its declared state **vectors**:

```
Body { matter: LayeredBody,  pos: DVec3,  vel: DVec3,  ang_mom: DVec3 }
```

- **`mass`, `radius`, `moment_of_inertia` DERIVE from `matter`** — never a constant. `mass = Σ ρ·V`,
  `radius = outer layer`, `I = ∫ r² dm` over the actual layers (a differentiated body has `I/mr² < 0.4`;
  the uniform-sphere `⅖mr²` was itself an Earth-shaped approximation).
- **State is vectors, declared as initial conditions.** A scene declares `pos`, `vel` and `ang_mom`; the
  **orbit, the spin axis, the gravity all EMERGE**. This is why a pre-defined scene needs vectors: an
  orbit is the consequence of a declared velocity vector, never declared as "an orbit" (the Law
  `crate::laws` already enforces — a world declares ICs, never their consequences; extended here to spin).
- **Spin is a vector** (`ang_mom`, arbitrary axis), so a tilted or off-axis rotation is expressible and
  `ω = I⁻¹·L`. Particalization applies `v = ω × (r − com)` for **any** axis, not +z only.

## The choices, their Laws, their tests

| # | choice | Law | enforcing test |
|---|---|---|---|
| 1 | `mass`/`radius`/`I` derive from `matter` | I,V,VII | arbitrary body: `mass=Σρ·V`, `radius=outer`, `I=∫r²dm` |
| 2 | no `[1]=Earth`; bodies by declared role + detected collision | II | permutation/N-planet scene simulates identically |
| 3 | spin is a vector (`ang_mom`), any axis | I | off-axis `L` → `v = ω×(r−com)`; fails on +z-only |
| 4 | `particalize(matter, resolution)` reads real layers + per-material EOS from the catalogue | I,II | arbitrary layer stack: mass conserved, per-layer EOS used |
| 5 | provenance = **source-body index**; render colour = source material albedo | I,VI | 3-body collision separates all 3; colour from materials.json |
| 6 | scene = objects (matter + `{pos,vel,L}`) + assemblies; consequences emerge | II,VI | `laws.rs` scan rejects a declared consequence (orbit/g/spin-rate) |
| 7 | retire the CPU `Aggregate` path; one SPH resolution | II | drop resolves via SPH, no `Aggregate` built (rig) |
| 8 | **name-freeness, machine-enforced**: `crate::laws` fails the build if a generic path names `EARTH_*`/`MOON_*` or `planet::earth()/moon()/theia()` | II | the capstone `laws` test, grown per generic path |

## Particalization by energy and scale (docs/44/47)

`particalize(matter, resolution)` is body-agnostic: it reads each layer's boundary radius and material,
looks up that material's EOS from `data/materials.json` (the Tillotson-to-catalogue work, docs/04, is what
makes this possible — every layer material now carries its own EOS), and seeds equal-mass SPH particles.
**The engine chooses `resolution`** from the impact energy and the view scale — resolution by necessity
(docs/44), granularity by viewpoint (docs/47) — not a hardcoded `2400/400`. The `iron-core/basalt-mantle`
+ `TARGET_CORE_LOD` + target/impactor asymmetry are Earth/Theia-shaped and go.

## Provenance without names

`SphParticle.prov` becomes the **index of the source object** a particle came from (not `0=Earth/1=Theia`).
`body_bulk(k)`, the disk statistics, and the render all key off it for **any** number of bodies. The disk
`earth_pct` becomes per-source-body provenance; the render's binary warm/cool tint becomes the source
material's real **albedo** (`materials::aggregate_albedo`), which the CPU path already computes and then
throws away.

## Order of work

Design (this) → the `Body` record + emergent `mass`/`radius`/`I` + per-body `ang_mom` (#1–3) →
`particalize(matter, resolution)` (#4) → source-indexed provenance + albedo render (#5) → route the
collision generically and retire `Aggregate` (#7) → `laws.rs` name-freeness + ICs-spin capstone (#6,#8) →
purge residual `EARTH_*`/`MOON_*`. Each step keeps the suite green; the flagship is rig-verified at the end.

**Related:** docs/23 · docs/33 · docs/44 · docs/46 (row 14) · docs/47 · docs/50 · docs/04 (the EOS
catalogue that makes generic particalization possible).
