# docs/59 — Orbit to ground: the meteor swarm (the acceptance test)

Robin (2026-07-24): *"launch a meteor swarm simulating a disintegrated asteroid originating from outside
Earth. See it from orbit (modern Earth), see the meteor trails. Then follow one meteor that reaches the
surface, seeing better LOD as we descend, and stop over the crater — which we see because we have the math
that generated it and JIT detail puts it in at varying levels of detail as we get closer."*

This is the flagship that exercises the engine's whole promise at once: **one law, every scale, from a
hyperbolic approach outside the atmosphere down to a crater you can stand in.** It is docs/13's
orbit-to-ground zoom, docs/23's north star, and docs/39's JIT particalization, driven by one event.

## The spine: a scene CAPABILITY, not a scene FEATURE

Robin's acceptance criterion: *"in every scene that uses the engine properly, we should be able to add the
'meteor swarm' button and it will just work, consistently."*

So nothing here is a Terra feature. The swarm, atmospheric entry, the trail, the impact and the crater are
**engine capabilities on the generic body + collision + atmosphere system** (docs/58). A scene calls one
operation — "launch a swarm at this planet" — and the engine drives it through the ONE collision pipeline;
the scene only renders the shared results. Terra is merely the FIRST host, because it already owns the
orbit⇄ground camera needed to watch it. The same button on the Ground scene, or any future scene that owns
the generic body machinery, must behave identically. **If it only works in Terra, the design has failed.**

That is Law II stated as an acceptance test: the swarm is the same physics whether the camera is 400 km up
or standing on the ejecta blanket, and the same code in every scene.

## It's all impact (docs/58)

A body's swept trajectory can collide with two things, and the engine dispatches both the same way it
already routes body↔body collisions (`interaction::detect_swept` → route):

- **the atmosphere** — the FLUID branch: `atmosphere::atmospheric_step` (drag + Sutton–Graves aeroheating +
  ablation, already built, docs/48). A body with declared `atmosphere_mass` has a density shell; entering
  it IS a collision.
- **hard matter** — the SOLID branch: the surface/body impact → a crater (`accretion::crater_bowl` sizing,
  `gpu_sph::promote_ground_cap` / the SPH cap for the resolved excavation).

They **compose**: enter the air, ablate and slow along the path, and whatever mass survives forks to the
hard impact. This is the ONE dispatch the swarm exercises N times over.

## What already exists (do not rebuild)

Grounded against the code 2026-07-24:

- **Terra** (`lib.rs` `Terra`) — modern Earth from orbit: real elevation/landcover/bathymetry rasters, a
  Rayleigh atmosphere, and `terra::fly_camera::FlyCamera` — a CONTINUOUS altitude-blended orbit⇄ground
  camera ("space to a standing horizon, no mode switch") with `build_cap` resolving terrain under the eye.
  **The hardest piece — the descent camera — is already built and verified (docs/43 Phase 4).**
- **Generic body** `{matter: LayeredBody, pos, vel, ang_mom}` + N-body integration (`orbit.rs`), ICs as
  declared vectors (docs/58). A swarm is just N such bodies.
- **`atmosphere::atmospheric_step`** — the generic body⊕air operator (drag/heat/ablation), material-driven.
- **`interaction::detect_swept`** — generic swept collision detection (the dispatch's natural home).
- **`accretion::crater_bowl`** — depth+radius of a crater from excavated volume (measured, not a dial).
- **`gpu_sph::promote_ground_cap`** + planar bulk (docs/39) — promote a voxel surface patch to SPH matter
  under an impact. Built + tested, not yet wired into a scene.
- **`ResolutionController::camera_grain_radius`** / `surface_detail` (docs/49) — the LOD sizing law.

## The five features to build (honest gaps)

1. **Atmosphere-collision dispatch** — the engine detecting a swept trajectory ∩ the atmosphere shell and
   applying `atmospheric_step` to ANY body, in ANY scene. Recorded (docs/58), unbuilt. This is the "just
   works everywhere" enabler.
2. **The entry trail** — the ablated vapour, deposited as hot gas that glows (and ionises to plasma above
   ~10–20 km/s), fading as it cools. This is what you SEE from orbit, and it closes the ablation
   mass-conservation gap (today the shed mass leaves the books). Resolved form: `AirField` gas parcels.
3. **A real surface impact on Terra** — a hypervelocity fragment striking Terra's real heightfield →
   a crater, via `crater_bowl` + the SPH cap. Terra renders craters but does not yet TAKE a surface impact.
4. **JIT crater detail at varying LOD** — the crater the math generated, resolved finer as the camera
   approaches (docs/39 particalize-on-demand, docs/47 granularity). Design-only today.
5. **Camera-follow a chosen fragment** — track one body's trajectory down. Unbuilt (small).

## The staged plan — each stage independently VISIBLE and verified

**Stage A — the swarm enters; trails from orbit.**
- Engine: a generic `launch_swarm(target, approach_velocity, n, spread, material)` that adds N bodies with
  ICs (a disintegrated asteroid: a common hyperbolic Earth-approach velocity — ≥ escape 11.2 km/s, typically
  11–30 km/s — with a small disruption dispersion), positioned outside the atmosphere. **No 250 m fudge:
  the meteors originate outside Earth on real trajectories** (Robin's explicit ask).
- Engine: the atmosphere-collision dispatch (feature 1) + the trail (feature 2).
- Law: II (one entry law, every body), III (resolve the trail only where seen), V (trail is real vapour,
  not a decal).
- Test: native — a body crossing the shell decelerates/heats/ablates (extends the `atmospheric_step` tests
  to the swept-detection layer); trail mass = ablated mass (conservation). Rig: from orbit, a swarm of
  glowing entry streaks.

**Stage B — follow one down; LOD rises.**
- Engine: camera-follow (feature 5) driving Terra's existing `fly_camera` descent; the ground cap +
  `camera_grain_radius` resolve terrain finer with altitude.
- Law: IV (the camera changes representation, never existence — the other fragments still fall off-camera),
  VI (physics drives the view).
- Test: rig — ride a fragment from orbit toward the surface, terrain detail increasing continuously (no
  popping, docs/49). Note the `surface_detail` LOD-tier blocker (docs/46) may surface here.

**Stage C — impact and the JIT crater.**
- Engine: the surface impact (feature 3) → `crater_bowl` at the strike point on Terra's heightfield; JIT
  crater detail (feature 4) resolving the bowl finer as the camera closes in.
- Law: III (particalize the crater by necessity/energy, not the whole planet — the docs/39 cap), V (crater
  from measured excavation, docs/46 row 18), IV (the far-side strikes still happened).
- Test: rig — stop over a crater that sharpens as you approach; the resolved detail must be consistent with
  the analytic bowl at every LOD (the JIT invariant: zooming in adds detail, never changes the crater).

## Law pre-flight (CLAUDE.md — run BEFORE building, not after)

1. **Any number not traced to physics?** Entry velocities are declared ICs (a real approach speed, not a
   consequence); the crater is energy-sized (`crater_bowl`); drag/heat coefficients are the flagged IOUs
   already carried by `atmospheric_step`. The trail's glow is its real temperature. No new dial.
2. **Answering a question already answered?** The entry is `atmospheric_step` (one operator); the impact is
   the existing crater/SPH-cap path; the descent is Terra's camera. This is CONSOLIDATION onto the generic
   pipeline, not new physics — exactly the "just works everywhere" test.
3. **Resolving more than necessity?** The bulk swarm stays cheap point-bodies; only a followed fragment and
   its trail/crater are resolved (docs/44). Off-camera fragments still fall and still cratered (Law IV).
4. **Camera deciding existence?** No — following one fragment must not stop the others (Law IV). The crater
   forms whether or not it is watched; the camera only chooses how finely it is drawn.
5. **Reaching for it because it will LOOK right?** The trail and crater must DERIVE from the real physics —
   the ablated mass, the excavated volume — and converge to the resolved form as detail rises. A declared,
   mass/energy-conserving model at orbital LOD is legitimate and in fact required (docs/46 "two things
   no-fudge does not forbid"): the trail can be a glowing column whose light budget comes from the real
   ablation before it becomes `AirField` parcels; the crater is `crater_bowl` from the real excavation
   before it becomes full SPH. The fudge is only an UNDECLARED sprite/decal that traces to nothing and
   converges to nothing. Every declared stand-in names its resolved counterpart.

## Stage A progress (2026-07-24) — the engine owns it, end to end

**Robin corrected the framing mid-build, and the correction is the important part of this section.** An
earlier draft of this plan ended with "the Terra half: give Terra a body list and a step loop." That is a
scene feature wearing an engine's clothes, and it is the failure this doc opens by naming — *if it only
works in Terra, the design has failed*. Robin: the swarm should be *"a natural operation of the engine
receiving these materializations and rendering them naturally"*, and the only legitimate scene-side part
is *"the mass/trajectory introduction with button press"* — because that is an INITIAL CONDITION, which
is exactly what a scene is for.

So there is no Terra half. There is an engine that flies matter and reports what it is holding, and a
scene that declares ICs and presents the result.

- **`flight::Flight`** — the operation. `introduce(FlyingBody)` is the one door; `introduce_swarm` is
  `damage::disrupt` fed into it, so the swarm is a composition of things the engine already had rather
  than a feature. `step` runs the air, the trail, gravity and arrival at hard matter.
- **`flight::FlightEnvironment`** — the entire seam between one flight law and every world it runs in:
  gravity here, air here, has the path met hard matter. A ground patch answers from a heightfield; a
  planet from its own layered mass and its `AirShell`. The physics between them is the same code.
- **`Drawn`** + `GpuParticle::of_matter` — the engine says what its matter looks like, from real albedo
  and real temperature, once. A scene that can draw ANY of the engine's matter can draw ALL of it,
  including matter the engine starts holding later.

The operation was not invented here: it was `Simulation::fly_meteors`, already correct and already
generic in everything but its ADDRESS — inside a 96 m voxel patch, in f32, unreachable from planetary
scale. The ground scene now delegates to it, which is what makes "one law" a fact rather than a claim.

**What is left to SEE it:** a scene presenting `Flight::drawn()` from orbit. That is small and it is the
sanctioned scene-side part — but Terra draws globes and meshes and has no instanced particle path today,
so it needs the docs/50 render-path increment ("the render path is still two") to reach it. Nothing
visual has been claimed or rig-verified yet.

- ✅ **Feature 1, the atmosphere-collision dispatch.** `interaction::detect_atmospheric` — the engine
  sweeping every (body-with-air, body) pair and reporting who is flying through what, alongside the solid
  branch in the same module. `BodyState` gained `air`; `atmosphere::AirShell` is a body's air as two
  emergent numbers, and `air_density_at` delegates to it so the barometric profile has one implementation.
  Reported as a STATE rather than an event, because an impact happens at an instant and flight through air
  happens along a path. Swept via `orbit::swept_min_distance`, so a body cannot skim the atmosphere
  between two frames and be recorded as never having entered it.
- ✅ **Feature 2, the entry trail.** `atmosphere::VaporParcel` / `vapor_step` / `Trail`. Closes the
  conservation hole: `ablated_mass` used to be subtracted from the body and dropped. Wired into the Ground
  scene's meteor so it has a live consumer rather than joining the docs/48 built-and-unwired pile.
- ✅ **The swarm's initial conditions.** `damage::disrupt` — a disintegrated asteroid, not N placed
  meteors. Dohnanyi mass shares, escape-speed separation, spread = v·t since breakup, golden-angle
  isotropy, and Σm·v = 0 exactly.
- ✅ **Feature 5, camera-follow** — and it arrived as something better than a follow, because Robin
  redirected it: *"can we feed camera coordinates, FOV to engine? … a different thread could drive its
  position, framing"*, and then named the principle — **"This matches an observer/universe scenario. The
  universe handles all the physics, the viewer watches."**
- ⬜ **Features 3–4** (surface impact on Terra, JIT crater LOD) — Stage C, untouched.

## The observer and the universe (Robin, 2026-07-24) — the split this settled into

> *"engine has enough going on; it gets to track what is observed (and render it) based on where it is told
> camera pose/fov is"* … *"This matches an observer/universe scenario. The universe handles all the physics,
> the viewer watches."*

This is the sharpest statement of the architecture yet, and it replaces the vaguer "scenes are data":

- **The universe** holds the matter and runs the laws. It does not know what a meteor is, what a camera is
  for, or what "following" means. It answers two kinds of question: *what is there* (`Flight::bodies`,
  `heaviest_fragment`, `Drawn`) and *what does it look like from here* (a pose in, a frame out).
- **The observer** decides where to stand and how wide to look, and it is free to be anything: the built-in
  fly camera, a script, a chase rule, or code on another thread. `Terra::set_camera_pose(eye, forward, up,
  fov_y)` is the whole interface, and `clear_camera_pose` hands control back.

Two things fell out of taking it seriously rather than adding a "follow mode":

1. **The field of view stopped being duplicated.** It was written out in `fly_camera` and AGAIN in the
   matter shader's billboard sizing — so the "one pixel" floor silently stopped being one pixel if either
   changed. A pose carries the FOV, `View::fov_y` reports the one the frame was built with, and the shader
   reads that.
2. **The near plane became the engine's job, not the camera's.** The fly camera derives it from ALTITUDE,
   which is right when the nearest visible thing is the ground below and badly wrong otherwise: riding 82 m
   behind a fragment at 218 km altitude put the near plane 104 km away and clipped the very thing being
   followed — a starfield and a working HUD with nothing in the middle. The engine knows how close its own
   matter is, so it now answers that. Which is exactly *"it gets to track what is observed"*.

### Stage B status: the ride works, the detail does not rise

RIG-VERIFIED (`web/rig/terra_follow.mjs`, paced to ~60 fps): a fragment ridden from **528.6 km down to
0.1 km, 39 of 39 samples descending**, worst render ~3 ms, the fragment visibly incandescent at its own
3,134 K, the camera releasing itself when the fragment lands, and daylit terrain filling the frame from
~30 km down.

**What is NOT done, and it is the half this stage is named for.** The surface never resolves into visible
DETAIL. At 70 m altitude the ground is a flat green fill — the right biome colour, no relief, no
granularity. The altitude descends continuously and the globe→cap crossover happens, but nothing gets
*finer*, so "seeing better LOD as we descend" (Robin's original ask) is unmet. This doc predicted it: the
`surface_detail` LOD-tier blocker (docs/46), where wiring the finer tier reproduces a documented revert
because Terra has no finer LOD tier to fade into. That is the next piece of work, and it is a prerequisite
for Stage C's JIT crater — a crater that sharpens as you approach needs a surface that can sharpen at all.

#### 2026-07-31: the cost blocker is GONE and the detail blocker survived it

The tier ladder was unaffordable — 45.2 ms/frame at one tier, 642.5 ms at four — because every vertex baked
`surface - eye`, so any camera motion invalidated the whole 192² mesh, every frame, per tier. That is fixed:
a tier is now anchored to a fixed world point with `anchor - eye` carried in the model matrix, and
`ground_cap::tier_is_current` rebuilds it only when a rebuild would change something (coverage within the
`CAP_MARGIN` it was over-built by; resolution and lift within an octave). **Measured: p50 0.4 ms at FOUR
tiers**, against 700–772 ms for the same rig on `main`. Four tiers now cost what zero used to.

**And the detail did not follow, which retires a hypothesis this doc was leaning on.** With tiers cheap,
1-vs-4 was A/B'd at a fixed camera at the full 16-octave budget: max pixel difference **6 at 500 m, 4 at
100 m**, ground luminance structure unchanged. The nested ladder is not what stands between the camera and
detailed ground — so "wire the finer tier" is no longer the description of the remaining work, and
`TERRA_DEFAULT_TIERS` stays 1 for a measured reason instead of a budgetary one.

#### Why the ground flattens on descent — answered, 2026-07-31 (docs/46 row 27)

Two causes, measured, and the dominant one is not where I first looked. **It is not the amplitude law:** run
the same generator at `slope_fraction = 1.0` and it produces RMS slope ~1.0 and 106 m of relief inside a
109 m frame — violently rough. The generator is fine. What is wrong is what multiplies it.

1. **`slope_fraction` compares two quantities measured four orders of magnitude apart.** Terra builds it as
   `tier_slope / mu`, where `tier_slope` is the elevation gradient over a baseline of two raster texels —
   **39 km** on the shipped 2048×1024 Earth — and `mu` is a grain-scale friction coefficient. A 39 km
   baseline is a regional TILT and cannot be steep. Measured on the shipped raster at 4,096 land points:
   median **0.0020**, p90 0.0111, largest anywhere **0.0619**. **Everest itself reads 0.0080**, because
   averaging over 39 km flattens it. So the relief is multiplied by ~0.003 on typical land and ~0.10 at its
   most extreme — never the 1.0 the law is written in terms of. In frame at 100 m altitude that is 10.3 m
   of relief at the roughest place on Earth, 1.4 m at Everest, and **0.36 m on median land**.
2. **The amplitude law is scale-invariant, so approaching cannot reveal roughness even in principle.**
   `relief_amplitude_m` is `min(drop/2, λ/4)`, and for cohesive rock the cohesion term wins the OR
   everywhere a camera cares about (granite's `h_crit` is 453 m), so the binding term is the `λ/4`
   no-overhang cap — a property of a HEIGHTFIELD, not of the rock. Amplitude ∝ wavelength is Hurst exponent
   **H = 1**: the smoothest self-affine surface there is, and the one whose slope is identical at every
   scale. Real topography has H ≈ 0.5–0.7 and gets relatively rougher as you close in.

Together they explain the symptom exactly. Above ~20 km altitude the frame spans more than one raster texel
and you see real, measured mountains. Below it the frame fits INSIDE one texel, measured elevation
contributes only a smooth bilinear ramp, and every visible bump must come from a generator that is both
scaled to ~0.003 and structurally incapable of roughening as you approach.

**No fix attempted, deliberately.** The fix is not a multiplier — that is a dial (Law V). It needs a
roughness measured at a scale the shipped data does not have. The honest options are a physically-sourced
Hurst exponent extrapolated from the finest scale the raster DOES resolve, or finer data; both are design
decisions, not repairs. Pinned meanwhile by
`surface_detail::generated_relief_has_the_same_slope_at_every_scale_below_two_km` and
`surface_detail::a_regional_gradient_cannot_reach_a_material_scale_slope`.

The hitch anchoring exposed is also fixed: a 4-tier rebuild was **224–310 ms**, because the staleness tests
are relative and every rung of the ladder therefore trips at the same altitude on a descent.
`ground_cap::tier_owed_a_rebuild` now serves at most one tier per frame, outermost first — **61–79 ms worst
frame**, one tier's rebuild, p50 still 0.4 ms. Going below that means splitting a single tier's rebuild
across frames rather than scheduling whole tiers, and is not done.

Also found and fixed on the way: `launch_swarm` hand-rolled its own lat/lon→direction with the OPPOSITE
sign on z from `crate::geo::dir_from_lat_lon`, so the swarm aimed at a mirrored longitude and arrived
nowhere near where the camera pointed. CLAUDE.md warns about this exact thing — the tangent frame was once
six hand-written copies, and the one sign they all shared was wrong.

### Answers to the open decisions below, as built

1. **The atmosphere shell boundary** — resolved, and more strongly than "lean threshold-from-density".
   There is no boundary: `ρ(h) = ρ₀·e^(−h/H)` is positive everywhere, so any altitude is a declaration.
   `atmosphere::air_reaches` asks instead where including the air stops being able to change the answer —
   drag's `|Δv| = a_drag·dt` falling below `ε·|v|` is not neglecting the air, it is adding it and having
   no effect. MEASURED on Earth's own emergent air: 296 km for a 1 m iron sphere at 20 km/s, 354 km for
   one 1000× lighter, 291 km at half the step. Body-dependent and step-dependent because the physics is,
   and it tightens by itself as the arithmetic improves.
2. **Trail representation** — as this doc proposed: declared-but-conserving first, `AirField` parcels the
   named counterpart. Built with the resolution choice explicit rather than implied (Robin, 2026-07-24:
   *"rendering/tracking should be decided based on the scale it is being viewed at"*): `Trail` holds the
   same mass either as resolved parcels for a near camera or as a booked total for one watched from orbit,
   and `Trail::mass()` is the same number both ways — representation follows the camera, mass does not.
   A parcel's SIZE is emergent (vapour expands to the density of the air it expands into) and its colour
   is `blackbody_srgb` of its real temperature.
4. **Determinism** — settled by construction. The separation directions are a golden-angle sphere, not a
   random draw: a disruption needs ISOTROPY, and that needs no seed and is identical every run, which is
   what lets a scene fly back to the same crater it watched form.

Decision 3 (impact scale on Terra) is untouched — it belongs to Stage C.

### What building it exposed (now docs/46 rows 20–22)

- **Row 20** — Sutton–Graves is a continuum law and the swarm flies the whole path, including the
  free-molecular regime above ~100 km where the classical `Λ·½ρv³` is the right one. The engine applies
  one law everywhere. The regime is computable rather than a judgement call: air's `dynamic_viscosity` and
  `molar_mass` give the mean free path (reproducing the measured 68 nm at sea level), so Knudsen could
  select or blend. Bounded at metre scale, worse for small fragments — the population that decides what
  burns up, which matters directly for what the swarm looks like.
- **Row 21** — `atmospheric_step` heats the body's whole mass at bulk heat capacity. Real ablation is a
  surface process. The model gets the observable right (small meteoroids burn up, iron meteorites land)
  but only while the body is thinner than its thermal skin depth, ~1.5 cm for iron over ten seconds.
- **Row 22** — mass booked into the air stays there; really it condenses and falls.

## Open decisions to pressure-test (before building each stage)

1. **The atmosphere shell boundary.** Where does "in atmosphere" begin — a fixed altitude (Kármán ~100 km),
   or where drag/heating first exceeds a threshold derived from density? The latter is more honest (no magic
   altitude) and scales to any planet's declared air. Lean threshold-from-density; confirm.
2. **Trail representation.** Deposited SPH gas parcels (resolved, expensive) vs a declared glowing column
   with an energy/temperature budget from the ablated mass (cheap, flagged IOU → the parcels). Start
   declared-but-conserving, converge to parcels — the docs/44 ladder.
3. **Impact scale on Terra.** A kilometre-class fragment on Terra's ~90 m raster: the crater may be
   sub-cell at orbital LOD and only appears as JIT detail resolves — which is the point, but confirm the
   heightfield/raster can carry a persistent crater record (the docs/39 surface hook: a real
   displacement/normal record so a crater stays a crater).
4. **Determinism / following.** Which fragment do we follow, and is the swarm reproducible run to run
   (needed to fly back to the same crater)? ICs are declared, so yes — pin it.

## Non-goals (this doc)

- Re-implementing Terra's camera or atmosphere (they exist).
- A new scene struct (Robin: extend Terra; the capability is engine-level, invokable from any scene).
- Full radiative-transfer plasma spectroscopy for the trail (blackbody + a flagged ionisation threshold is
  the honest first cut; emission-line detail is a later resolution step).

**Related:** docs/13 (scale-relative) · docs/23 (north star) · docs/39 (JIT particalization) · docs/43
(Terra, the fly camera) · docs/44 (resolution by necessity) · docs/46 (one-physics charter; row 18 crater) ·
docs/47 (granularity) · docs/48 (atmosphere) · docs/49 (resolution controller) · docs/58 (the generic body,
"it's all impact").
