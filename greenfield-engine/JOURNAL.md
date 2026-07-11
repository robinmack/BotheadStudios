# Development Journal

A running log of major milestones for `greenfield-engine`. Newest entries at the top.
Each entry records *what* changed, *why*, and *how it was verified*.

---

## 2026-07-11 — The atmosphere's weight keeps the oceans liquid (docs/25)

**What.** Earth now declares only the MEASURED MASS of its atmosphere (5.15e18 kg); the surface pressure
emerges as that column's weight — ≈1 atm, never assigned. Materials gained Clausius–Clapeyron BOILING
curves (latent heat + molar mass, `thermal.molar_mass`) beside their Simon melting curves, and the phase
decision (`planet::surface_phase`) now covers solid/liquid/vapor. The consequences, all as passing tests:
288 K water under the emergent 1 atm is LIQUID; the same water in vacuum flashes to VAPOR at any
temperature (below the ~611 Pa triple point liquid has no regime — Robin: "water exposed to vacuum would
be wild", and the model now says exactly that); cold water freezes; water boils at ~366 K at 0.7 atm
(mountain physics for free). The airless Moon ⇒ no lunar seas, as observed. A failing test caught real
physics along the way: Earth's inner core briefly classified as "Vapor" because iron's boiling point was
a flat 1-atm fallback — pressure suppresses boiling even harder than melting, and with iron's real molar
mass boiling is COMPLETELY suppressed at 360 GPa (the fallback was the dishonesty).

**Why.** Same pattern as the molten core: declare real composition (now including the air), compute the
consequences. Also fixes the record on ocean colour: water renders with its measured near-black
reflectance — the "blue marble" is atmospheric Rayleigh scattering, which we refuse to paint. The
atmosphere today is a static boundary condition (pressure/phase); making it MATTER — drag, entry plasma,
Rayleigh blue, blast waves, evaporation cycling — is now the flagged next major milestone (docs/25
roadmap).

**Verified.** `the_declared_atmosphere_mass_weighs_in_at_one_atmosphere`,
`liquid_oceans_exist_under_an_atmosphere_and_boil_off_in_vacuum`; 79/79 native, wasm builds.

---

## 2026-07-11 — Layered planets: the molten core EMERGES from pressure (docs/25)

**What.** Planets are now DECLARED as their real construction and nothing else: concentric layers of real
materials (Earth: iron inner/outer core, peridotite mantle, basalt crust — PREM densities; Moon: small
iron core, peridotite mantle, basalt crust) with the observed geotherm as declared data. Everything else
is COMPUTED:
- **Gravity g(r)** — Gauss's law over the enclosed layer mass (peaks at the core boundary, zero at centre).
- **Pressure P(r)** — hydrostatic equilibrium integrated from the surface. Earth's centre comes out
  **≈360 GPa** (real: 364) and the core–mantle boundary ≈135 GPa from the declared densities alone.
- **PHASE** — each material got a pressure-dependent melting curve (Simon–Glatzel, published fits, new
  `thermal.simon_a/simon_c` in the materials DB). Phase = local temperature vs T_m(P). **Never assigned.**

**The emergence result (Robin's challenge: "that should be a natural artefact of gravity/mass/material
if we didn't fudge the composition"):** Earth's inner core comes out **SOLID** even though it is HOTTER
than the molten outer core — because the computed pressure pushes iron's melting curve above the
geotherm exactly there — while the outer core comes out **MOLTEN** and the mantle solid. The melt curve
crosses the temperature profile at the real inner-core boundary. Also: the declared layer densities
integrate to Earth's real mass and 9.8 m/s² surface gravity; the Moon's outer core comes out molten at
lunar pressures (flagged: the real lunar core is Fe–S, which melts lower than our pure-iron entry — we
use the upper published selenotherm; an Fe–S material is the refinement).

**Wired into the impact:** the materialized clouds now sample the layered bodies — each particle knows
its material (basalt crust / peridotite mantle / iron core) and its REAL internal temperature, so
excavating deep matter exposes rock and iron that glows because it genuinely is that hot. Earth's cap
reaches the top of the molten outer core (a Moon-scale impact digs that deep). Each fragment renders in
its own material's reflectance — the excavated composition is visible.

**Continents & oceans:** the render shell samples a 10°×10° land/ocean mask matched to the ~9° grain
spacing ("average area particles") — granite continents, water oceans, real reflectances. Honesty flags:
the hand-digitized mask over-represents land (~37% vs the real 29% — a cited dataset is the refinement);
ocean depth (~3.7 km) is far below one grain, so at this LOD water is the material of a grain's surface,
not a resolved layer; no planetary rotation yet, so the mask's orientation is arbitrary but consistent.

**Verified.** New `planet.rs` tests: declared composition → real mass + surface gravity; hydrostatics →
real central/CMB pressures; **molten-outer/solid-inner core emergence**; lunar molten outer core + solid
mantle; landmask places the major continents/oceans with a plausible area-weighted land fraction.
77/77 native, wasm builds.

---

## 2026-07-11 — Gauss interior gravity, emergent incandescence, and Earth rendered as matter

**What.** Three fixes, each traced from an on-device observation to missing physics (never a visual patch):
1. **Interior gravity obeys Gauss's law.** Debris that ploughed beneath the surface was sucked into the
   core ("the balls absorb into the centre") because the point-mass 1/r² is only valid OUTSIDE a planet.
   Inside, only the enclosed mass pulls: g(r) = GM·r/R³, linear to ZERO at the centre. The gravity source
   is now an extended body (1/r² outside its radius, Gauss interior inside) — no singular attractor.
2. **Incandescence is emergent — the hand-deposit pipeline is GONE from the planetary impact.** The
   impactor's fragments now simply CARRY the true contact velocity (they are the arriving body); the one
   contact law transfers momentum into the target's materialized matter, and the contact DISSIPATION
   (damping + friction) is routed into temperature (`granular::contact_dissipation` — energy is conserved,
   not destroyed, docs/20). A hard impact glows because the matter genuinely got hot. Measured: the cloud
   goes 83% → **100% gravitationally bound** through the collision, hottest fragment ~41,600 K (flagged:
   melt/vaporization energy sinks are not yet modelled at this scale, so the peak overshoots — the glow
   is real physics, the exact peak is not yet).
3. **Earth renders as its matter.** A smooth sphere is a representation lie once matter can be excavated —
   it hides the damage. Earth now draws as a shell of ~512 coarse grains (the honest low-res look);
   grains inside the materialized impact region are hidden, so the excavated void IS the crater and the
   glowing cap particles are the matter that used to fill it. Reset now un-shatters properly. Cosmetic
   skinning (an elastic surface over the blocks) is deferred until after the physics visuals are right.

Also: descent-follow camera (pure camera work — reads `moon_distance_km` from the N-body state, starts on
the whole-orbit framing, glides to a close-up at 25% of lunar distance as the Moon falls; manual zoom
overrides, Drop/Reset re-engage).

**Why.** Robin: "Do not bandaid visuals — fix the physics and then visualize them." Each visual wrongness
was a physics gap: no interior Gauss law, dissipation not becoming heat, a summary representation
(the sphere) hiding real state.

**Verified.** `interior_gravity_follows_gauss_law_not_a_point_singularity` (half depth ⇒ half g; centre
pulls ~nothing; exterior unchanged); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(100% bound, hottest ≫ visible-glow threshold — incandescence emergent); 72/72 native, wasm builds.
On-device: Robin confirms the impact now reads correctly ("Much better!").

---

## 2026-07-11 — ONE collision law for all matter + the mutual impact + conservation-law contact state

**What.** Three connected pieces, closing out Robin's "this must define ALL collisions of ALL matter":
1. **The canonical contact law now governs aggregates too.** `granular::contact_accel` (spring + damping +
   Coulomb friction + cohesion — the physics of record for terrain grains and GPU debris) is now the
   contact force inside `Aggregate` as well; the new `granular::contact_from_material` is the ONE mapping
   from a real material (Young's modulus → stiffness, restitution → damping, friction, cohesion) to
   contact behaviour, used everywhere. Aggregate particles previously had gravity and bonds but NO
   contact — they interpenetrated freely, which is why the shattered Moon was an "exploding sphere in a
   vacuum". A surface velocity rule I'd added to compensate ("cancel the inward component") was a fudge
   (Robin caught it) — deleted; the bulk planet is now a conservative penalty boundary (a force, −∇U).
2. **The mutual impact (`impact.rs`).** At the strike we materialize BOTH bodies at the interface — the
   Moon as a rubble ball on the surface AND Earth's impact region as a cap of crust (same grain mass) —
   and deposit the Moon's real momentum + energy into the *combined* cloud via the same
   momentum/shock-heat/vapor pipeline as the terrestrial meteor. Earth's matter absorbs most of the
   momentum; crater, ejecta, fallback all emerge from the one contact law. Measured natively: **93% of
   the cloud stays gravitationally bound** — as the declared energetics demand (≈2e7 J/kg deposited vs
   ≈6.3e7 J/kg to unbind).
3. **Conservation-law contact state.** Robin observed "a large percentage of debris escapes" on-device,
   yet the model said 93% bound — the discrepancy was the INPUT: in fast-forward, the deposit used the
   Moon's velocity *after* a ~2000 s step that had carried it far past the surface — a garbage sample
   (**21,822 m/s vs the true 9,870 m/s → ~4.9× the honest energy**). New `orbit::contact_velocity`
   recovers the true state at the surface from the two-body conservation laws (vis-viva energy + angular
   momentum), dt-independent. The simulation FORECASTS the collision; it never samples garbage.
   Also: the frame now STOPS at a detected collision (the render can never show a body sailing past its
   own impact — the simulation drives the visualization and interrupts it).

**Why.** "Get the small stuff right, apply everywhere": one contact law, derived from declared material
parameters, at every scale — and dt-independence as a principle: what the physics concludes must not
depend on how coarsely we stepped or how fast the visualization runs.

**Verified.** `aggregate_particles_collide_via_the_canonical_law_and_conserve_momentum` (no pass-through,
momentum conserved, real rebound); `a_dropped_moon_impact_leaves_most_debris_gravitationally_bound`
(93% bound after the contact plays out); `contact_velocity_recovers_the_true_impact_speed_regardless_of_
step_size` (recovery within 2% at the browser's coarsest step vs +121% for the post-step sample);
71/71 native, wasm builds. Visual verdict pending on-device.

---

## 2026-07-11 — Materialize the Moon at impact: honest momentum, real 1/r² fall-back, incandescence for free

**What.** With tunneling fixed, the Moon reached Earth but "dinked on top and fell out the bottom, intact,
no ejecta" (Robin, on-device). Three honesty bugs in the shatter, all now fixed:
1. **Momentum was being dropped/mis-deposited.** First cut drove the debris with the Moon's full *incoming*
   momentum → the whole clump shot DOWN through Earth. I then tried zeroing the momentum — Robin caught it:
   *"drop the momentum sounds like fudge again."* Right. The honest model (Robin's framing): from orbit the
   Earth/Moon are *"really big single particles, an average of physical material properties"*; at impact we
   **materialize** the Moon into its constituent matter, and the fragments carry the Moon's REAL incoming
   velocity (Σmᵢv = m_moon·v — momentum conserved across the promotion, not dropped). The impact ENERGY
   disperses them symmetrically (net-zero momentum). **Earth's surface then transfers the inward momentum by
   CONTACT** — the same swept CCD primitive, now applied per-fragment (inward fragments stop on the ground →
   momentum to Earth; outward ones eject). "Get the small stuff right, apply everywhere."
2. **Fall-back/escape was faked by a uniform gravity field.** A baked uniform "down" wrongly forces even
   >escape-velocity fragments to fall back. Robin: *"Model parameters declare REAL physics… we MUST be
   faithful."* Replaced with a real **point-source 1/r² pull toward Earth's actual centre** (G·M_earth,
   from the masses the model already declares), softening kept tiny (2% R⊕) so the field is faithful where
   fragments live (r ≥ R⊕, contact-enforced). The escape/fall-back split is now EMERGENT, not imposed.
3. **Incandescence now comes free from the thermal state.** `deposit_impact` already computes each
   fragment's temperature; the space shader gained a self-emissive term and the debris is tinted by a
   blackbody ramp of its REAL temperature (dark→red→orange→white). Hot ejecta glows on the night side —
   nothing scripted.

Also added **📷 Earth / 📷 Moon** camera buttons (explicit frame-of-reference switch; "Camera on Moon"
frames the impact site once it shatters) toward Robin's zooming-FoR / "fixed camera 1000 m above the site"
goal.

**Why.** The vision, sharpened by Robin across this session: *all* Newtonian-scale laws should EMERGE from
faithfully-modelled matter — the engine answers "what if the Moon deorbited?" by tracking real materials,
so a child (and, ultimately, an embodied AGI) could *re-derive physics* from playing in it. Every fudge is
a false lesson. So the escape boundary must come from √(2GM/r), not a tuned knob.

**Verified.** New native test `aggregate::point_source_gravity_splits_escape_from_fallback`: a fragment
launched at 1.4×escape leaves for good (>10 R⊕); at 0.6×escape it arcs back and lands (apoapsis <2 R⊕) —
the threshold read straight from the declared M and G, with surface contact mirrored as in the render.
`cargo test -p engine` 66/66; wasm builds + deployed. The shatter VISUAL (scatter + glow) needs on-device
eyes. FLAGGED next: apply the swept CCD to the GPU granular contact (retire the `V_MAX` cap) — same primitive.

---

## 2026-07-11 — Swept collision (forecast the path): the dropped Moon no longer TUNNELS through Earth

**What.** The dropped Moon was shooting straight through the Earth and never colliding. Root cause (Robin
diagnosed it exactly): in fast-forward the Moon moves > an Earth-diameter per step, so the DISCRETE
contact test (are the surfaces overlapping *this* sample?) sees it outside at both samples and misses the
collision entirely — the trajectory was effectively faked, riding on a detection that never fires.
Fixed with **swept continuous collision detection**: `orbit::swept_first_contact(rel_old, rel_new, r_sum)`
solves for the fraction `t∈[0,1]` at which the body's straight path FIRST enters the contact sphere —
*when* it hits — regardless of step size. `OrbitDemo::render` now captures each moon's pre-step position,
runs the swept test after the step, and intervenes at the first-contact point (parking the point mass
there and, for moon 0, triggering the Stage-A shatter at the true impact site/energy).

**Why.** Robin: *"forecast with the simulation (know what will happen in real physics), model it with the
visuals"* and *"there is a difference between what we can render and what we can simulate."* The
simulation must KNOW the continuous path intersects the planet even when we sample/render it coarsely —
what we simulate must not depend on how coarsely we look. And: *"get the small stuff right, APPLY
EVERYWHERE"* — `swept_first_contact` is a pure-geometry primitive (segment vs. sphere), not orbit-specific.
The SAME tunneling is why the grain sim caps the vapor front at `V_MAX` (a workaround); CCD is the honest
general fix there too — flagged as the next application of this primitive to the granular contact.

**Verified.** `orbit::swept_contact_catches_a_body_that_tunnels_through_the_planet` (a −5→+5 pass through
the centre — both endpoints outside — is caught at t=0.4; a clearing path is `None`; already-inside is
t=0). `cargo test -p engine` 65/65; wasm builds. The Moon-collides VISUAL still needs on-device eyes.

**Open.** Apply CCD to the GPU granular contact (replace the `V_MAX` ejecta-speed cap) — the same
primitive, the "everywhere". And the Stage-A shatter visual + Stage B zoom-in remain to verify/build.

---

## 2026-07-11 — Moon-shot Stage A: the dropped Moon SHATTERS (emergent), instead of merging

**What.** In the space band (`OrbitDemo`), the de-orbited Moon now **shatters into a debris cloud** on
impact rather than the point-mass sphere silently merging into Earth. The frame the Moon first strikes,
its point mass becomes a **self-gravitating aggregate** of 64 basalt fragments filling the Moon's volume
at the impact site (`build_moon_debris`), and the impact energy — captured honestly at contact (~4.5e30 J)
— is deposited via the same `aggregate::deposit_impact` pipeline (momentum + shock heat + vapor). Because
that energy is ≫ the Moon's binding energy, the aggregate DISPERSES — no scripted destroy, just kick vs.
binding (docs/21). The fragments then arc under Earth's gravity (uniform toward its centre), some flying
out, some falling back — the ejecta curtain at planetary scale. They render as small basalt spheres at
their real positions; the intact Moon sphere stops drawing. The debris steps at a FIXED observable rate
(`DEBRIS_DT`, a time-LOD) so the fine event plays out at human speed, not the celestial fast-forward that
would disperse it in one frame.

**Why.** The moon-shot (docs/23): "de-orbit the Moon into [a spot], then zoom … and observe it was
destroyed" — with NO code that says destroy. The drop, the fall, the surface contact, and the honest
impact-energy accounting all already existed and were native-tested; the collision just rendered as two
spheres merging + a HUD number tagged "not yet materialised." This wires the tested aggregate-disruption
physics into the render so the shatter is finally *seen*, emergently. It's the celestial half; Stage B is
the zoom-in that materialises the local crater/ejecta from the same conserved energy (docs/19).

**Verified.** `cargo test -p engine` 64/64; wasm builds. The disruption physics itself is native-tested
(`aggregate::energy_above_binding_disrupts_it`, `an_impact_heats_the_core_and_shatters_the_aggregate`);
`build_moon_debris` feeds it the real impact energy. **The VISUAL is NOT yet verified** — a rendering
change can't be checked headlessly (docs/19: "needs on-device eyes"). Needs Robin's eyes + tuning of
`DEBRIS_DT`, `DEBRIS_N`, fragment size, and the fall-back/escape balance.

**Open.** Earth-side damage is still only the HUD verdict (no crater visual on the Earth sphere yet); the
debris external gravity is a uniform approximation (fine near the impact, coarse as it spreads); and the
whole thing is the CELESTIAL band only — the scale-relative zoom-in (Stage B/C) is still unbuilt.

---

## 2026-07-11 — Cohesive grain contact (the frictionless-graze fix, one property doing three jobs)

**What.** Added an ATTRACTIVE adhesion term to the grain contact law (GPU shader + native `granular.rs`
force-of-record): net normal force = repulsive spring − cohesion, so touching grains can now BOND (the
force pulls them together) until the bond lets go past a short range. `cohesion = 0` recovers the exact
old push-only contact. The friction load now includes the cohesion, so a touching/grazing pair has a
real normal load — and therefore friction. `c_cohesion` is derived from `Material::cohesion`, converted
to a per-mass adhesion and capped at a granular ceiling (loose debris is already fractured — rock grains
keep only surface adhesion, they must not re-weld into solid). Reused the dead `c_max_accel` param slot,
so no struct-layout churn.

**Why.** Robin caught that a grain placed at *exactly* zero overlap grazes frictionlessly — friction is
`μ·N` and `N = k·overlap = 0` there. Her instinct: "surely there's a property of matter that ensures
this never happens unless the particles are separated?" There is — **cohesion**, a real material
property already in `materials.json` that we used for solid bonds but not loose-grain contact. It closes
the graze (touching ⇒ bonded ⇒ normal load ⇒ friction), it's *why* soil holds a slope dry sand can't
(the same thread as the granite-cliff/talus split), and it's part of what holds a planet together
against its own gravity — a prerequisite for the Moon-onto-vacuum-Earth moon-shot.

**Verified.** New `gpu-verify` foundational test **F8**: a gentle separating nudge is HELD by the bond
(1.00 m), a hard nudge BREAKS it (6.84 m), a cohesionless pair DRIFTS apart (2.00 m). Native
`granular::cohesion_bonds_touching_grains_and_raises_friction`: a just-touching cohesive pair is pulled
together and a zero-compression graze has friction, while dry stays frictionless. `cargo test -p engine`
64/64; wasm builds; no scene regression (scenes default to cohesion = 0).

**Open.** Cohesion is a single representative value (like friction) — a per-particle/mixed-material
cohesion is a later refinement; and terrain contact doesn't yet carry cohesion (grains adhering to the
ground), flagged.

---

## 2026-07-11 — Emergent impact end-to-end: momentum-conserving contact, terrain-as-matter, drag fudge deleted

**What.** A long arc turning the impact from scripted fudges into emergent particle physics (`docs/24`),
capped by a foundational test suite that caught a core-model bug and a fix that scaled the whole thing.

- **Terrain-as-matter (Path B).** The meteor no longer carves a crater and scripts ejecta velocity. It
  MATERIALIZES the impact region into grains at rest (`matter::materialize_region`), deposits the
  meteor's real momentum as an impulse (`deposit_impulse`), and the rest of ½mv² as radial-gradient
  **shock heat** (`deposit_shock_heat`, filled core-first so a sub-grain impactor's energy actually
  vaporizes a plasma core instead of smearing below threshold). Vaporized matter **expands** and throws
  the ejecta (`deposit_vapor_expansion`) — Robin's insight that at 17 km/s the crater is driven by phase
  transition (gas pressure), not elastic rebound; the KE was already in the sim as shock heat we were
  radiating away. Added estimated thermal data for the granular soils (they couldn't vaporize before).
- **Momentum-conserving contact solve (the core fix).** A two-particle test (`gpu-verify` F5) exposed
  that the directional-implicit solver damped each grain's ABSOLUTE velocity, so a 20 m/s head-on
  collision lost ~74% of its momentum — hidden by every pile/crater scene (slow ⇒ ~0 COM velocity).
  Fixed by a derived neighbor-coupling term `Sv_nbr = Σ S·v_neighbor` in the RHS; the pair's COM velocity
  now telescopes to conserved. This alone made stepped terrain conserve energy and DEEPENED the emergent
  crater ~5× (3 m → 14.7 m) — ejecta finally keep their momentum.
- **Conservative terrain + steep materialization.** Replaced the min-translation terrain penalty (whose
  normal FLIPPED at voxel edges, injecting energy) with a smooth bilinear surface and the exact −∇U
  penalty. Vertical walls a heightfield still can't represent become grains
  (`materialize_steep_terrain`) — but only where the material is too weak to hold a cliff (critical
  height ≈ σ/ρg): dirt slumps to talus, **granite holds as a real cliff** (Robin's antithesis, emergent
  from strength).
- **DRAG FUDGE DELETED** (`matter::DRAG` 0.9995 → 1.0). It bled 62%/s of a vacuum particle's speed
  (foundational test) and was masking the non-conservative terrain; with the three fixes above the core
  no longer needs it — a vacuum particle keeps its momentum.
- **Everything couples honestly.** `aggregate::deposit_impact` (probe/bodies) rewritten to the SAME
  pipeline (momentum + heat + vapor) — the last scripted `√(2·0.3·e/ρ)` kick is gone; the meteor couples
  into EVERY body via `couple_impact_to_bodies`, not a hardcoded probe. Removed the cosmetic
  `spawn_vaporized_meteor` (a scripted 22 m/s clump that looked like an intact meteor and double-counted
  momentum). Restitution is now derived from `Material::restitution` via a θ-method contact integrator.

**Why.** Robin's directives drove it: *"trust physics; hunt for what we're missing, don't fudge"*;
*"crater size should be emergent and observable, not imposed"*; *"a meteor is an exaggerated test of the
same physics as a footfall or a feather"*; *"no fudge in the core models"*; and the clincher — *"test
every aspect of the fundamental interactions of particles; get the small stuff right and the universe
scales."* The foundational suite proved it: a two-particle collision plus a derivation beat the solver
that "looked stable and passed every scene."

**Verified (RTX 2070 headless + native).** New `gpu-verify` foundational block F1–F7 (Newton's 1st/2nd/
3rd, momentum-conserving collision, friction≈μg, touching↔separated sweep) all PASS in true vacuum;
grain-grain energy conservation (I-flat), stepped-terrain conservation (I), emergent crater (M, 14.7 m
deep). `cargo test -p engine` 63/63; wasm builds. New native tests: materialize/impulse/shock-heat/vapor
conservation, steep-terrain materialization + the granite-cliff antithesis, restitution→damping.

**Open (honest, flagged).** (1) A granite cliff a heightfield still can't contact conservatively should
become a COHESIVE aggregate (grains + bonds) — rigid AND conservative. (2) Friction runs ~35% strong
(F6 ratio 1.35 vs μg) — the same over-sticky friction behind the repose under-/over-prediction (scene
D). (3) Crater size is LOD-capped (materialize cap) below the physical scale. (4) Dissipated energy →
heat → radiation still dropped (flagged in-shader). (5) Soil thermal values are composition estimates,
not cited.

---

## 2026-07-09 — North star + a reverted fudge; the engine's name: "Integrity"

**What.** While bringing GPU debris up (docs/22), a play-test exposed that a meteor doesn't destroy the
probe, and I reached for a special case — `if probe within crater { obliterate }`. Robin: "if
everything is real, the probe should have just been destroyed on impact on its own; the fact we have to
correct that concerns me." She's right — that's a **fudge**. **Reverted it.** The real problem: the
probe is the **last bespoke object** (a rigid `body::Sphere`), not matter, so `matter::impact` can't
see it. Wrote `docs/23`: the north-star demo — **a metal ball at ground zero, de-orbit the Moon into
it, zoom in and observe the ball was destroyed** — with NO code that says "destroy the ball." It's
destroyed because the impact energy really reaches it and exceeds iron's thresholds (`damage`). The fix
is to make the probe **real matter** (a cohesive aggregate / voxel body), so gravity, contact, impact,
`damage::classify`, and emission all act on it emergently — no special cases.

**Name.** Robin is naming the engine **"Integrity"** — fitting: it's the operating invariant (every
value traces to real physics or is flagged; no fudge), and reverting the special case is exactly it.

**Also (shipped this session, verified/native):** terrain now uses planetary **surface gravity**
(uniform down, not the slab's micro-g self-gravity — fixes debris concentrating at the world centre;
real-time, no time-scale). The GPU debris path works on-device (FPS fixed, debris glow + cool).

**Open (honest):** probe-as-matter unification (`docs/23`); GPU resting-debris re-deposition
(iteration 3, kills the moiré pile-up); the celestial→local materialization for the zoom-in.

---

## 2026-07-09 — Bodies as particle aggregates (emergent binding + disruption)

**What.** Started making celestial destruction a *simulation, not a mock* (`docs/21`). A body becomes a
**cloud of particles held together by its own gravity** (`aggregate.rs`): softened N-body self-gravity,
`binding_energy` (Σ G·m·m/r), `kinetic_energy_com`, `rms_radius`, `com`. Verified that a cold cloud
**holds together** (cohesion emerges from gravity — the `docs/15` roundness invariant) and that an
energy kick **above the binding energy disrupts it** (emergent dispersal — the identity behind a
shattered moon).

**Why.** Robin asked, pointedly, whether the impact destruction is *inherent in the engine's model* or
"just mocks to humor me." Honest audit: the terrain meteor IS real emergent simulation (per-voxel
fracture/melt/vaporize from material + energy, glowing by computed temperature). But the **celestial**
Moon-crash was NOT simulated destruction — the bodies were point masses drawn as spheres, so there was
no matter to break, and I was about to build a scripted "fireball" — a **mock**. Stopped: that violates
the honesty invariant. The honest path (Robin chose it) is bodies-as-aggregates, so the shatter is the
same gravity that rounds them, run past their binding energy — no script.

**Verified.** `aggregate::a_self_gravitating_cloud_holds_together`,
`aggregate::energy_above_binding_disrupts_it`. `cargo test` 44/44; clippy `-D warnings` clean; fmt
clean.

**Honest scope.** This is the gravitational *skeleton*. Per-particle material + temperature, the impact
coupling (deposit energy → `damage::classify` per particle → emergent debris/melt/vapor), and the
rendering are the next slices (`docs/21`). Until they land, the *visible* Moon-crash still shows the
momentum stick — and we will NOT fake the shatter in the meantime.

---

## 2026-07-09 — Phase classes integrated into matter::impact; Moon-speed readout

**What.** `matter::impact` now classifies each ejecta via `damage::classify` (the thermodynamic
thresholds): a carved voxel is at least Fractured, the hot core Melts, the hottest Vaporizes. The class
drives behaviour — **vaporized** ejecta expand away fast (gas/plasma, `VAPOR_EXPANSION`), all glow by
temperature. The crater extent stays the budget model (LOD bridge `docs/19` intact). Also added a live
**Moon-speed readout** (km/s relative to Earth) to the space-band HUD.

**Why (speed readout).** Robin saw the Moon's velocity seem to "flatten as if terminal velocity in a
vacuum." Checked the orbit path: there is **no drag, clamp, or damping** anywhere — the only velocity
changes are the verlet kicks and surface contact. So there is no terminal velocity; the physics is
honest. The apparent flattening is either Kepler's 2nd law (a *partial* brake makes an eccentric orbit
that slows at apogee — the opposite of drag) or the compressed time-scale hiding the final fast plunge.
The speed readout makes it observable: on a true **Drop** it climbs toward ~11 km/s at impact; use ⏪
slower to watch it accelerate.

**Verified.** New `matter::a_colossal_impact_vaporizes_the_core` (core passes basalt's boiling point →
Vaporized class). `cargo test` 41/41; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
deployed.

**Honest caveat (`docs/20`).** Crater excavation and shock heating still use separate energy
accountings (a flagged simplification — full coupled conservation is the MLS-MPM/shock-EOS future).

---

## 2026-07-09 — Visual: glowing molten ejecta + a Meteor you can fire

**What.** The first visible slice of impact damage beyond text (`docs/20`): impact ejecta carry a
temperature, and molten debris **glows by black-body emission from that temperature**. Added
`Particle.temp_k`; `matter::impact` deposits heat that peaks at the contact and falls to cold at the
crater rim (centre melts/vaporizes, rim is cold rubble — the honest radial gradient); `emission::
incandescence(temp_k)` maps K → an added RGB glow (dull red → orange → yellow → white); the particle
shader **adds** it, so hot debris self-illuminates even on the dark side (it *emits* because it's hot —
the analogue of illumination × reflectance, `docs/17`). A **Meteor** control (`Engine::meteor`, the
`☄`/`m` button in the terrain slice) fires a high-energy `impact` you can watch and orbit into.

**Why.** Robin: "see the impact, then zoom in and see the crater" (with glowing melt). Delivered in the
*terrain* renderer (which renders on-device) so it's verifiable now; the celestial→voxel auto-fly-in
(materialising the Moon-crash crater from its summary) stays staged (`docs/19`).

**Verified.** `emission::cold_matter_does_not_glow_and_hotter_glows_brighter_and_whiter` and
`matter::a_big_impact_melts_the_centre_and_leaves_the_rim_cold`. `cargo test` 40/40; clippy
`-D warnings` clean; fmt clean; wasm + `tsc` green; deployed. The *look* of the glow is for Robin's
on-device check.

**Honest caveat (`docs/20`).** The crater extent is physical (energy/σ), but the ejecta *temperature*
distribution is a first visual model — the energy is not yet conserved through the phase change, and
`incandescence` approximates the Planckian locus. Next (Robin's order): integrate the phase classes
into `matter::impact` proper (voxels → gas/melt/ejecta, energy-conserving), then MLS-MPM.

---

## 2026-07-09 — Impact thermodynamics: fracture → melt → vaporize (one rule)

**What.** Modelled fragmentation, melting, and vaporization as **one data-driven response** (`docs/20`),
Robin's planetary-scale test of the engine (and of scale-of-detail). An impact deposits **energy
density** (J/m³ = Pa); each parcel's fate comes from comparing it to that material's own thresholds:
fracture strength → melt energy `ρ(cΔT+L_f)` → vaporization energy. `damage::classify` returns
`Intact | Fractured | Melted | Vaporized` — the *same* "density vs threshold" logic as fracture, just
higher thresholds. Because the deposited density falls with distance, **one event produces all four at
different radii** (near-field vaporizes, then melts, then fractures, then intact). Added optional
`Material.thermal` (specific heat, melt/boil points, latent heats) with **cited data** for basalt,
granite, iron, water; materials without it can only fracture (we don't claim unknown melt behaviour).

**Why.** Robin: "model fragmentation, melting, vaporization — a test of our simulator's abilities on a
planetary scale (and of our scaling of detail)." A giant impact honestly vaporizes rock near contact,
leaves a magma ocean of melt, fractures/ejects a shell, and — since E ≪ Earth's binding energy — leaves
the planet intact but resurfaced. Every one is the same `classify` at a different radius.

**Verified.** `damage::impact_fractures_then_melts_then_vaporizes_by_energy_density` (thresholds order
σ<melt<vapor; each band classifies right; a giant-impact density vaporizes rock; no-thermal-data →
fracture-only). `cargo test` 38/38; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Staged (docs/20):** integrate into `matter::impact` (voxels become gas/melt/ejecta by class,
conserving mass + energy through the transition); the **visual** display — incandescent melt (black-body
emission from temperature, not a painted colour), a vapor plume, and the materialised crater to fly
into (`docs/19`); cooling/solidification (magma → rock).

---

## 2026-07-09 — Two-moon stress test scene

**What.** A new scene (`/twomoons.html`): two moons on the same orbit, **opposite sides** of the Earth,
that you **de-orbit both at once**. Generalized `OrbitDemo` from one moon to N — `[Sun, Earth, Moon,
Moon2]`, a moon uniform per body, per-moon lighting/framing, and collision resolved Earth-vs-each-moon
with each moon's impact energy counted once (the two hits **sum** in the HUD). `brake_moon`/`drop_moon`
now act on *all* moons; focus cycles Earth → Moon A → Moon B; the second moon is placed at −d with the
opposite tangential velocity so both orbit the same way and stay diametrically opposed. The two HTML
pages share one script — the moon count comes from `<body data-moons>`.

**Why.** Robin: "It's our universe, we might as well play in it." The N-body core (`orbit.rs`) is
already generic, so two moons is nearly free physically; its value is **stressing the collision path** —
two simultaneous surface contacts, symmetric resolution, and (later) two craters materialising at once.

**Verified.** `cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green;
`/twomoons.html` serves. Visuals (two moons, symmetric de-orbit, double impact) pending Robin's
on-device check.

---

## 2026-07-09 — LOD-adaptive damage: the crater bridge (celestial ↔ voxel)

**What.** Connected the Moon-crash to a real crater across scales (`docs/19`). The bridge: a damage
event is the *same event* at every LOD, so the coarse **summary** and the fine **voxel materialisation**
must agree. Both use the same `σ·V` accounting — `damage::crater_volume(E, σ) = E/σ` (celestial
summary) equals the voxels `matter::impact` excavates (proven:
`matter::voxel_crater_matches_the_coarse_damage_summary`). Added honest **regimes**: strength crater
(`V=E/σ`), gravity regime (flagged, unmodelled), and **disruption** past the body's binding energy.

**Honesty — the Moon is not a tidy crater.** ~4.5e30 J is ~36× the *Moon's* binding energy (the Moon
**shatters**) but only ~2% of the *Earth's* (~2.2e32 J), so the Earth **survives with a planet-scale
crater** — the giant-impact regime, not a neat bowl. The space-band HUD now says exactly this on impact
(`damage::moon_shatters_but_earth_only_craters` pins the numbers). We report the regime honestly instead
of promising a crater the physics forbids.

**Why.** Robin: connect the Moon-crash to a real crater. The honest connection is the σ·V bridge — the
same relation drives the celestial summary and the zoomed-in voxel crater, so promoting/coarsening
across LOD conserves the event (`docs/13`). The *visual* zoom-in (fly the camera down and materialise
the voxel crater) is a real renderer effort, designed in `docs/19`, staged for on-device work.

**Verified.** New: `damage::crater_scales_with_energy_and_inversely_with_strength`,
`damage::moon_shatters_but_earth_only_craters`, `matter::voxel_crater_matches_the_coarse_damage_summary`.
`cargo test` 37/37; clippy `-D warnings` clean; fmt clean; wasm + `tsc` green.

**Roadmap (Robin's order):** LOD (this — bridge done; visual zoom-in next) → MLS-MPM → fluid. Planned
playground: a **two-moon** scene (opposite sides, same orbit, de-orbit both at once) as a stress test —
the N-body core is already generic, so it's nearly free.

---

## 2026-07-09 — Unified deformation & damage: the design + first honesty slice

**What.** Started the deformation/damage subsystem (`docs/18`) from Robin's requirement that a **bullet,
a pebble in a pond, and the Moon hitting the Earth be the SAME operator** — differing only in
parameters and level of detail. The design names two invariances: (1) **material** — the response comes
from constitutive data (solids fracture at strength, granular media crater, liquids yield at ~0 and
flow), so bullet-in-rock and pebble-in-pond are one call with different material; (2) **scale/frame** —
the observer's frame/zoom decides what is materialized (celestial: energy/momentum + crater summary;
zoom in: voxel fracture + ejecta; zoom way in: grains/buildings), promoting/coarsening across LOD while
conserving mass/momentum/energy. Two concrete slices landed: (1) parse material **`phase`** and fix the
liquid fudge — water's `fracture_strength` used to fall back to `1e12` (stronger than granite!); a
fluid now yields at ~0. (2) `MatterSim::impact(site, direction, energy)` — the **generalized
energy-driven impact**: it spends the impact energy fracturing voxels nearest-first (σ·V per voxel), so
bigger energy → bigger crater, stronger material → smaller crater, and a liquid splashes. A 10 g bullet
(~450 J) and the Moon (~4.5e30 J) are the *same call*.

**Why.** Robin: the same system should observe a bullet, a pebble in a pond, or a planetary impact —
and at a given scale we simulate only what the observer can perceive (buildings only matter zoomed way
in; ejecta only zoomed in; celestial scale cares about energy/momentum and a crater summary). This is
the honest unification of the voxel-fracture model (`matter.rs`) with scale-relative fidelity
(`docs/13`, `docs/08`) — the endpoint is MLS-MPM with per-phase constitutive models.

**Verified.** `materials::a_liquid_yields_where_a_solid_resists` (a fluid yields to a poke a solid
withstands) and `matter::impact_is_material_and_scale_invariant` (same energy craters dirt but not deep
granite; more energy → bigger granite crater; a gentle impact still splashes a pond). `cargo test`
34/34; clippy `-D warnings` clean; fmt clean; wasm compiles.

**Roadmap remaining (docs/18):** fluid flow (needs a viscosity field, not in the DB yet) → MLS-MPM
constitutive unification → LOD-adaptive damage (summary ↔ detail on zoom). Robin: "we should get to the
rest before we're done."

---

## 2026-07-09 — Orbital-decay control: brake the Moon until it crashes (with real collision)

**What.** The requested experiment — slow the Moon and watch its orbit decay into the planet — plus the
honest physics that makes a "smash" real rather than a numerical explosion. `orbit::resolve_contact`
adds **surface collision**: two solid bodies stop when their surfaces meet (perfectly inelastic,
momentum-conserving), instead of tunnelling through each other as point masses into a 1/r² singularity
— the celestial echo of the voxel body contacts (`docs/16`). `orbit::perigee` computes the live
closest-approach so the HUD can show the orbit tightening. OrbitDemo exposes `brake_moon` (halve the
Moon's velocity relative to Earth), `drop_moon` (cancel it → radial plunge), `reset_moon`, plus a
variable **time multiplier** in the HUD. The web control bar gains Brake / Drop / Reset + slower/faster,
and the HUD shows perigee (reddening below Earth's radius) and "💥 IMPACT".

**Why.** Robin wanted to watch the Moon smash the Earth. The honest lesson is built in: in a
conservative two-body system a *single* halving does NOT crash — it drops into a tighter eccentric
ellipse (perigee ~55,000 km, still a miss); it takes a few brakes (or a full drop) to push perigee
below the surface. Real orbital mechanics, shown, not faked. Also exposed the time multiplier per
Robin's note (and it lets you slow time to watch the impact).

**Verified.** `cargo test` 31/31 — including `perigee_tracks_how_hard_the_moon_is_braked` and
`a_dropped_moon_crashes_into_the_planet_and_stops_at_the_surface` (it reaches the surface and rests
there, no tunnelling). clippy `-D warnings` clean; fmt clean; wasm + `tsc` green. Visuals pending
Robin's on-device check.

**Impact energy (honesty).** Robin noted that at these masses an impact must do *damage* — and that a
perfectly-inelastic "stop at the surface" silently *deletes* the kinetic energy, which is itself a
fudge. So we now **measure and report** it: `orbit::inelastic_dissipation` (the KE the collision
removes) and `orbit::binding_energy`. A dropped Moon hits at ~11 km/s → ~4.5e30 J ≈ **36× the Moon's
gravitational binding energy**; the HUD shows this and states plainly that both bodies would be
destroyed. We measure the damage rather than hide it or fake it.

**Honest scope note.** "Collision" here is surface contact + inelastic stop, plus the reported impact
energy; actual **fragmentation** (deformation, melt, debris, merging) is a future subsystem — the
honest zoom-in unification of the voxel-fracture model (`matter.rs` `fracture_strength`) at scale.
Flagged, not faked.

---

## 2026-07-09 — Live real-Sun lighting, selectable focus frame, scene picker

**What.** Wired the real Sun into the *live* space band (following the validated physics): the demo now
simulates `[Sun, Earth, Moon]` with the Earth on its true ~29.78 km/s heliocentric orbit and the Moon
co-moving. The shader's light direction is now computed per-body **from the Sun's actual position** (no
more hardcoded direction), so the lit hemisphere and the Moon's phases are geometric. The Sun isn't
drawn at this zoom (~23,000 display units off-frame) — it is the *light source*, the scale-adaptive
choice (`docs/17`). Added a **focus control**: the viewport is a physical frame of reference
(`cycle_focus` / `focus_label`), re-centring the whole view on Earth or the Moon. And a **scene picker**
(`web/src/scene-nav.ts`) injected on both pages to switch between the terrain slice and the space band.

**Why.** Robin's direction: a real Sun should light the system (not a fake light), the viewport is a
physical frame of reference with a selectable focus, and the app should let you choose between scenes.
All three are honest, emergent-from-real-state changes (`docs/17`).

**Verified.** `cargo test` 29/29; clippy `-D warnings` clean; `cargo fmt` clean; wasm builds and
`tsc --noEmit` passes (focus + scene-nav bindings). **Visuals pending Robin's on-device check** —
headless WebGPU can't render here, so the appearance of the sun-lit bodies and the focus/scene UI is
for iPad confirmation.

---

## 2026-07-09 — Honest appearance: no painted tints, brightness from light, a real Sun

**What.** A user play-test of the space band exposed fudging: the Earth was a hardcoded ocean-blue
tint and the Moon a hardcoded grey — cosmetic colours touching no material data, even though the
terrain already colours voxels from real `materials.json` albedos. Replaced with honesty (`docs/17`):
(1) body colour = **aggregate albedo of a real composition** via the new `materials::aggregate_albedo`
operator (Earth = ocean water + continental granite + polar ice; Moon = basalt) — a computed summary,
not a paint job; (2) the space shader now does **illumination × reflectance** (bright sun × real, often
dark, albedo) + Reinhard tone-map, so a dark-but-lit body reads bright — the honest reason the Moon
looks bright; (3) added a validated **Sun–Earth–Moon** physics test: a real Sun (1.989e30 kg, 1 AU) and
the Earth given its **appropriate heliocentric velocity** (~29.78 km/s), with the Moon staying bound to
the moving Earth.

**Why.** The user pushed the honesty invariant (`docs/15`) all the way down: *don't fudge*. Key
insights captured: brightness is illumination × reflectance (not a bright material); even albedo is a
summary placeholder for real optics (ray tracing is the goal); zoom-out summaries are fine only if
*computed from everything we know* by one operator for all objects/scales; the illuminant should be a
real Sun; the viewport is a **physical frame of reference** with a **selectable focus** (planet →
Moon → …); and the core research question is whether the system can tell **what matters at a given
scale**. Working principle / candidate name: **"Integrity."**

**Honesty flags (not hidden).** Earth composition excludes the atmosphere → deliberately no Rayleigh
blue (the blue-marble blue is atmospheric, unmodelled); Moon lacks highland anorthosite in the DB → it
renders darker than reality until added; the shader's sun *direction* is still a placeholder until the
real Sun is wired into the live view.

**Verified.** `cargo test` 29/29 (new `aggregate_albedo_summarizes_real_constituents`,
`sun_earth_moon_system_is_bound`); clippy `-D warnings` clean; fmt clean; wasm compiles. The *visual*
result of the new lighting is for on-device confirmation (headless WebGPU can't render it here).

**Staged (larger, honest work):** real Sun as the live illuminant + heliocentric re-centering + focus
switching; ray tracing; specular/BRDF from roughness/metallic; stellar & anorthosite materials;
atmosphere for the earned blue; and the still-owed orbital-decay control.

---

## 2026-07-09 — Unified dynamics: everything not at rest reacts

**What.** Fixed the "probe quits falling / doesn't really react to debris" behaviour by unifying the
probe and the debris into **one awake-set dynamics loop** (`docs/16`). Previously `body::Sphere` (the
probe) and `matter::MatterSim` (debris) were separate systems coupled only through the voxel grid —
`matter.rs` never referenced the probe — so particles couldn't push it and settling debris deposited
voxels *inside/under* it, making it appear to rest on nothing. Now, per substep, every awake body
integrates under the same gravity field, resolves body↔world contacts, debris steps under that field
and **won't deposit inside a body** (piles on it, conserving matter), and **body↔debris contacts
exchange momentum both ways** (`MatterSim::couple_body`). Sleep/wake is structural: a body sleeps only
while in contact and slow, and wakes the instant support is removed or something hits it.

**Why this shape.** The user's principle: a physics loop looks at every object *not at rest* and makes
it react as a natural property of the world and the object, never a per-object script — the honesty
invariant (`docs/15`) applied to dynamics. Also captured the deeper motive: an honest, inferable
physical world is a place to *learn to act* (VR, and plausibly embodied-AI training), a payoff that
exists only to the degree the sim refuses to fake.

**Also (honesty corrections from the user).** (1) No atmosphere is modelled — matter falls through
*vacuum*, so the per-step `DRAG` constant is flagged as a numerical-stabilizer debt, not real air drag.
(2) Compute-budget policy written down: favour larger/more massive objects (massive bodies are
budget-exempt today; debris coarsening must *merge into mass-carrying clumps*, conserving mass on both
spawn and settle — so it's deferred, not half-done, to avoid a mass leak). (3) Noted the
server-authoritative-world / client-sees-a-slice threshold to watch (`docs/11`, `docs/13`).

**Verified.** New native tests: `particle_transfers_momentum_to_a_body` (momentum conserved through the
impact), `debris_does_not_settle_inside_a_body`, `body::wakes_and_falls_when_support_is_removed`.
`cargo test` 27/27; clippy `-D warnings` clean; `cargo fmt` clean; `cargo check --target
wasm32-unknown-unknown` green (the awake-set loop lives in the wasm-only host).

---

## 2026-07-09 — Representation invariant: the cube is a lattice, not a unit of matter

**What.** Answered a foundational design question — "are we baking a core mistake into the engine by
building on cubes, when the universe is made of spheres?" — and locked the answer in as canonical.
Wrote `docs/15`: **a voxel is a sampling cell, never a unit of matter.** The cubic grid is the
coordinate lattice we sample continuous fields on (density, material, momentum), like pixels sample an
image; it is not an ontology of blocks. All physical state lives on matter with continuous coordinates
(`Particle.pos`, `MassPoint`), and bulk voxels dissolve into particles the instant physics touches
them (`docs/08` tiers). Added a **grid-isotropy regression suite** (`isotropy.rs`) to enforce it.

**Why.** The honest answer is that cubes are *not* a foundational mistake — roundness is emergent, not
primitive. Real solids sit on lattices (many cubic — rock salt, BCC iron), yet planets are round
because isotropic self-gravity averages over the lattice; the engine already mirrors this (aggregate
mass → spherical far field in `gravity.rs`/`orbit.rs`; surface nets smooth the render). The *real*
risk is subtler: a regular lattice has preferred directions (axes, 45° diagonals) and a solver could
silently bake that bias into the physics. Also captured the user's north star: the world should **feel
right in VR because it is right, not via per-object fakery** — leave something unsupported and it
falls as a natural property of the world and the object (`find_unsupported` → `collapse`), never a
script.

**Verified.** New suite asserts (a) gravity on a symmetric ball is radial + equal-magnitude across
face axes and edge/corner diagonals (spread < 1%, tangential < 1%), and (b) `dig` carves a true
Euclidean sphere (volume within a few %, equal axis reach, no lateral ejection bias). Proven
**non-vacuous** via mutation testing: an injected axis bias in the gravity sum and a Chebyshev (box)
dig criterion both drove the guards red (gravity spread 9.7%; box removed 8000 vs a sphere's 4189),
then reverted. `cargo test` 24/24; clippy `-D warnings` clean; `cargo fmt` clean.

---

## 2026-07-09 — Space band: watch the Moon orbit (v0.9.0)

**What.** Step A of the scale-relative "orbit-to-ground" (`docs/13`): a spectator view of the real
Earth + Moon (`/orbit.html`). `OrbitDemo` runs `orbit.rs` (real SI, f64) each frame and renders two
lit spheres via a tiny new `space.wgsl` (position/normal + per-body tint + one directional sun, so we
get phases). Metres → display units (Earth radius → 1); the Moon sits ~60 units out. Time-scaled so a
~27.3-day orbit plays in ~20 s, substepped 16× for a stable symplectic step. HUD reads live
separation (~384,400 km). Kept on a separate page + Vite multi-page input so the terrain slice is
untouched.

**Why this shape.** I can't self-verify visuals here (headless WebGPU won't render the pipeline), so I
minimized blind risk: reuse the *proven* GPU setup pattern, the existing sphere mesh + `draw` path, and
lean on the already-validated physics (`orbit::moon_orbits_earth`). The renderer is a thin shell over
known-good pieces; the hard part (the orbit) is the tested part.

**Also.** Wrote `docs/13` (north-star: observer-relative fidelity) and `docs/14` (validation
demonstrations — each physics test mapped to what it proves + how to *show* it), at the user's request
to preserve the test concepts as demonstrations for the full build.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm build compiles `OrbitDemo` warning-
free; `tsc` clean; LAN dev server serving `/orbit.html`. Visuals to be confirmed on-device.

---

## 2026-07-09 — Solid-object collision + orbital-mechanics validation (v0.7.2, v0.8.0)

**Collision (v0.7.2).** From an iPad play-test: the probe clipped into crater walls (looked like a
duplicate ball, rested too high) because it only collided with the terrain column directly beneath
it. Replaced with proper **sphere-vs-voxel collision** (`body.rs`): integrate under gravity, then
iteratively push out of the deepest solid voxel the sphere overlaps (floor, walls, corners) with
restitution + friction. Solid objects act solid.

**Orbital validation (v0.8.0).** Added `orbit.rs` — N-body point-mass gravity + a symplectic
velocity-Verlet integrator. The native test drops in the **real Earth + Moon** (masses, 384,400 km,
1.022 km/s) and confirms a bound orbit: ≥1 full revolution, distance within 15% of real, energy +
angular momentum conserved <1%. This proves the gravity law reproduces real celestial motion — the
"does the Moon orbit the planet?" test — and, importantly, it's a **pure native test** (no rendering),
so it verifies the physics despite headless WebGPU being unavailable here.

**Note on tooling.** Headless Chromium here renders WebGPU only via software (SwiftShader) or hits a
Dawn instance bug on the real GPU, so I can't screenshot the full render; I lean on native tests
(watertight mesh, collision, orbit) + the user's iPad for visual confirmation. `web/screenshot.mjs`
is kept for environments with GPU access.

**Verified.** `cargo test` 22/22; clippy `-D warnings` clean; wasm + web build green.

---

## 2026-07-08 — Phase 6: smooth surface meshing (v0.7.0)

**What.** Terrain and craters now render smooth instead of blocky cubes. `mesher::build_surface_nets`
runs Surface Nets (`fast-surface-nets` crate) over the voxel occupancy field, recomputes smooth
normals from the geometry (oriented outward), and tags each vertex with its nearest material so
triplanar texturing + shine still apply. The renderer uses it for the initial terrain and every dig
re-mesh; the blocky mesher is kept as a fallback.

**Why.** The user flagged the Minecraft-blocky look. The key insight: the voxel grid is the *physics
substrate*, not the *visual* — so we smooth the rendering (marching-cubes/surface-nets style) while
mass, gravity, fracture, and collapse stay identical. Prototype clunkiness → smooth surface, no
physics change.

**Verified (TDD).** `cargo test`: 19/19 (new: surface-nets mesh is valid, finite, and genuinely
smooth — has non-axis-aligned normals). fmt + clippy (`-D warnings`) clean; wasm + web build green.
Live LAN wasm rebuilt. **Pending human check:** reload → rounded terrain and craters, still textured
and lit; dig/blast/collapse all still work.

**Next realism levers (noted):** smoothed/SDF field for rounder geometry, normal maps from the grain
field, finer/smoother debris (or MPM).

---

## 2026-07-08 — Phase 5: structural collapse (v0.6.0)

**What.** Undercut or isolated matter no longer floats. `world.find_unsupported()` flood-fills from
the anchored base (`y=0`) and returns any solid voxel not connected to it; `MatterSim::collapse()`
detaches those into falling particles, run after every dig. This closes the Phase-3 "floating voxels"
known limitation — overhangs, undercuts, and blasted-off chunks all fall and re-settle.

**Why.** Real matter needs support. Connectivity-to-anchor is the general, correct model (works on a
plateau now and a planet core later) and needs no per-case rules.

**Verified (TDD).** `cargo test`: 18/18 (added: intact terrain has no unsupported voxels; an isolated
voxel collapses, conserves matter, and re-settles). fmt + clippy (`-D warnings`) clean; wasm + web
build green. **Pending human check:** `npm run dev` → shift-click to undercut a ledge and watch the
overhang break loose and tumble down.

---

## 2026-07-08 — Phase 4: emergent textures (v0.5.0) — vertical slice complete

**What.** Materials now look distinct, generated *from their own properties* with **no bundled
images**. `texture.rs` synthesizes a high-res (512²) mip-mapped texture per material from
albedo + color_variance + metallic (grain/mottle + flecks + metal sparkle), seamless. The world
shader triplanar-samples a per-material texture array and adds a specular highlight (shine) from
per-material roughness/metallic. HUD gains an FPS counter. `docs/12` documents the approach + CC0
sources (ambientCG/Poly Haven) for optional user textures.

**Why.** Closes the appearance side of the thesis: look emerges from the same cited data that drives
mass, gravity, and fracture — one source of truth. User asked for high-res + no licensed photos;
procedural generation delivers both (mipmaps = scale-down; zero image assets = zero licensing).

**Verified (TDD).** `cargo test`: 16/16 (added 4 texture tests: size+mip chain, mean tracks albedo,
materials differ, non-flat variation). fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` +
`vite build` green. **Pending human check:** `npm run dev` → speckled granite, mottled dirt, green
grass, a shiny iron probe; dig to see textured debris.

**Milestone.** This completes the **Phase 0–4 vertical slice** from the plan: layered voxel matter ·
self-gravity (F=ma) · dig & material-driven fracture · emergent texture — all driven by the cited
material database. All four project pillars are demonstrable.

---

## 2026-07-08 — Phase 3: dig & material-driven fracture (v0.4.0)

**What.** Destructible matter. `matter.rs` is a CPU matter solver: click-to-dig (voxel raycast DDA)
fractures a spherical region — a voxel detaches into a particle only if the tool's stress exceeds its
material's `fracture_strength` (loaded from the cited DB). Debris falls under the Phase-2 gravity
field and, on rest, deposits back into the voxel grid (piling, matter-conserving). Instanced debris
rendering (`particles.wgsl`), terrain re-mesh on edit, HUD debris count. Click digs soil/grass;
shift-click blasts rock.

**Why.** Proves the core destruction thesis — materials break *differently by their own numbers*
(granite shrugs off what shreds grass), with no per-material special-casing. Framed honestly as the
**CPU, testable foundation** for full continuum MLS-MPM (deformation/stress + WGSL port) later, since
GPU MLS-MPM can't be unit-tested natively and TDD is canonical.

**Verified (TDD).** `cargo test`: 12/12, incl. `dig_detaches_soft_but_not_hard` (soil detaches under
1e6 Pa, granite needs a 2e7 blast) and `matter_conserved_through_dig_and_settle` (voxels + airborne
particles == original, every step, until all settle). Plus raycast-hits-terrain. fmt + clippy
(`-D warnings`) clean; wasm build clean; `tsc` + `vite build` green.
**Pending human check:** `npm run dev` → click the grass/dirt to blow a crater of tumbling debris
that resettles; click rock (nothing) then shift-click (it breaks).

**Known limits (noted for later):** mid-column digs can leave floating voxels (no structural
collapse yet); full-world re-mesh per edit (dirty-chunk meshing is the optimization).

---

## 2026-07-08 — Phase 2: self-gravity + falling probe (v0.3.0)

**What.** Made density physically active. `gravity.rs` computes a real Newtonian field from the
world's aggregate voxel mass (voxels lumped into blocks; direct-sum with f64 accumulation).
`body.rs` integrates a rigid sphere under that field (`F = ma`, semi-implicit Euler) with ground
contact and a scale-relative rest threshold. The renderer draws the probe via a per-object model
matrix; a live HUD shows world mass, local gravity, altitude, speed, rest state, and time-scale
(`Space` re-drops, `[`/`]` change time-scale).

**Why.** Proves pillar 4 — the world's own summed mass produces gravity; a probe obeys `F = ma` and
rests on the surface. No Rapier yet: one hand-integrated body is exact and far simpler; Rapier is
deferred until many bodies/contacts justify it.

**Honest scale note.** Real `G` is used, so the ~96 m world has asteroid-scale micro-g (~1e-5 m/s²).
That's correct physics; a time-scale fast-forwards the sim for viewing (time-lapse, not fake gravity).

**Verified (TDD).** `cargo test`: 9/9 — point-mass `G·M/r²`, far-field within 1%, mass conservation,
free-fall kinematics (`v=-g·t`, `½g·t²`), fall-and-rest, and an end-to-end drop onto the generated
world. fmt + clippy (`-D warnings`) clean; wasm build clean; `tsc` + `vite build` succeed.
**Pending human check:** `npm run dev` → watch the iron probe fall and settle; HUD reads out g and rest.

---

## 2026-07-08 — Phase 1: layered voxel world on screen (v0.2.0)

**What.** Turned the material data into a rendered world. Added to the engine crate:
- `materials.rs` — loads the cited `data/materials.json` (density + albedo) at compile time.
- `world.rs` — chunk-style voxel store + a layered generator: rock bulk, ~10 m dirt, grass skin,
  with a deterministic value-noise heightfield so the surface undulates (layers follow terrain).
- `mesher.rs` — face-culling mesher (only air-facing faces), per-material albedo vertex colors, so
  the rock/dirt/grass bands are visible on the exposed side walls.
- `lib.rs` + `shaders/world.wgsl` — a real 3D renderer: vertex/index/uniform buffers, depth buffer,
  perspective orbit camera, and a directional light + ambient/hemispheric fill.
- `web/` host: drag-to-orbit / scroll-to-zoom controls, gentle idle auto-rotation.
Also added `docs/10` (robustness — how the matter-first model designs out tunneling / fall-through /
"weird physics", plus the mitigations and an adversarial test plan).

**Why.** First milestone that makes "density as source of truth" *visible* and validates the core
Rust→WASM→wgpu render path end to end, on the real seed data.

**Verified.**
- `wasm-pack build` clean (no warnings). `tsc` clean. `vite build` succeeds (wasm ~1.32 MB dev).
- Dev server serves `engine_bg.wasm` as `application/wasm`.
- `cargo test` (native): material DB loads 19 materials with granite denser than dirt; the central
  column is grass→dirt→rock top-to-bottom and solid to y=0; mesher output is well-formed (quad-aligned
  vertices, 6 indices/quad, all indices in range).
- **Pending human check:** `cd web && npm run dev` in a WebGPU browser — a layered rock/dirt/grass
  plateau you can orbit and zoom.

**Version.** Milestone **0.2.0** (Phase 1) per the pre-1.0 policy (each phase bumps the minor).

---

## 2026-07-08 — Materials seed database + object/interaction design

**What.** Compiled the first **cited physical-properties database** — 19 materials (rock, ceramic,
metal, organic/wood, soil, granular, liquid, frozen) with mechanical + optical properties and source
URLs — into `data/materials.json` (schema in `docs/04`). Added design docs for the architecture the
user articulated: material **taxonomy + finishes + object composition** (`07`), **adaptive resolution
& clumping** so the sim scales instead of moving billions of particles (`08`), and **agentic object
authoring + physically-grounded tool/terrain interaction** — the "make a shovel" / shovel-in-dirt
vision (`09`).

**Why.** Physical properties are the single source of truth for both simulation and rendering; the
whole object/agentic vision ("make a shovel" that falls, sounds, and digs like one) reduces to
material data + physics + composition, with no bespoke per-object code.

**Verified.** `data/materials.json` parses (node `JSON.parse`), 19 materials each with mechanical +
optical blocks; categories: rock 4, ceramic 1, metal 3, organic 3, soil 2, granular 2, liquid 2,
frozen 2. Research quality-checked: rejected known-bad MatWeb figures (granite/limestone UCS),
flagged cited-vs-estimate, and captured state-dependence (soils/snow) and anisotropy (wood).

**Note.** JSON is the v0 seed; it migrates to the Postgres source of truth (`docs/05`) and grows into
the module/taxonomy system (`docs/06`, `07`) over time.

---

## 2026-07-08 — Published to GitHub as a monorepo

**What.** Restructured the engine into the `robinmack/BotheadStudios` monorepo as its first
project directory, `greenfield-engine/`. Root of the monorepo carries an MIT `LICENSE` and a
projects README. Aligned the engine to **MIT-only** (dropped the Apache dual-license) to match the
repo's license choice. Published the public OSS repo and tagged `v0.1.0`.

**Why.** BotheadStudios will hold multiple game projects; a monorepo keeps them together. MIT
across the board keeps licensing simple and consistent.

**Verified.** `git push` to `origin/main` succeeded; `v0.1.0` tag pushed; repo is public.

---

## 2026-07-08 — Project kickoff & Phase 0 scaffold

**What.** Created the engine as the first project in the **BotheadStudios monorepo**
(`greenfield-engine/`). Established the skeleton: `crates/` (Rust core), `web/` (TypeScript host),
`shaders/` (WGSL), `docs/` (research + design).
Added `README.md`, `LICENSE-MIT`, `CONTRIBUTING.md`, `.gitignore`, this journal.
Installed the toolchain: Rust 1.96.1 + `wasm32-unknown-unknown` target + wasm-pack 0.13.1 (Node 22 already present).

**Why.** The plan (see `.claude/plans/…` / `docs/`) settled a performance-first stack — Rust→WASM core,
custom `wgpu` WebGPU renderer, Rapier rigid bodies — after research confirmed **no existing engine fuses
all four pillars** (density-as-truth matter · emergent-from-density behavior · destructible-to-the-core ·
real self-gravity). See `docs/01-prior-art-existing-engines.md` and `docs/02-oss-building-blocks.md`.

**Phase 0 goal.** Prove the pipeline end-to-end: a Rust crate compiled to WASM initializes a `wgpu`
device and clears a browser canvas, driven by a thin Vite/TypeScript host. First pixel on screen.

**Verified (build/serve level).**
- Rust → WASM compiles via wasm-pack (fixed three `wgpu` 24.0.5 API differences vs. older docs:
  `request_adapter` returns `Option`, `request_device` takes a trailing `Option<&Path>` trace arg,
  and `RenderPassColorAttachment` has no `depth_slice` field).
- `npx tsc --noEmit` clean; `vite build` bundles the app (wasm 933 KB → 236 KB gzipped).
- `vite` dev server serves `engine_bg.wasm` as `application/wasm` (verified magic bytes `\0asm`).
- **Pending human check:** open `npm run dev` in a WebGPU browser to see the pulsing clear color.

**Version.** Tagged this milestone **0.1.0** (see `CHANGELOG.md`, `docs/03-versioning.md`).
Pre-1.0 policy: each roadmap Phase bumps the minor; games pin exact versions since we dogfood.

---
