# Emergent impact — ejection from compression + rebound, not a scripted velocity

> Robin: *"Ejection of matter should be caused by compression/rebound."* Right now `matter::impact`
> hands every ejecta grain a number — `v = √(2·(0.05·E)/m)`, aimed outward. That is a script, not
> physics. The honest crater is what really happens: the meteor **compresses** the target, the
> compressed material **rebounds**, and that rebound **throws matter out** — the ejecta curtain and the
> bowl both *emerge*. No assigned ejecta velocity anywhere. This is the impact half of the
> everything-is-matter north star (`docs/23`): the meteor is a real body, the ground is real matter, and
> the same contact law we already trust does the rest.

## The real mechanism (what we're reproducing)

1. **Contact + penetration.** The impactor drives into the surface.
2. **Shock/compression.** A pressure wave races ahead, compressing the target — grains driven together,
   storing elastic energy in their contacts (½·k·δ² per contact).
3. **Rebound + excavation flow.** Behind the shock, the compressed material unloads (rarefaction) and
   flows — down and out at the centre, up and out toward the rim — carving the bowl and launching the
   **ejecta curtain** (roughly a 45° cone, fast near the centre, slow at the rim).
4. **Fallback + heat.** Sub-escape ejecta rain back (breccia); the dissipated energy is **heat**
   (`temp_k`), radiated — see the energy-conservation note in `docs/23`.

Every step is contact mechanics on real matter. Our job is to stop scripting step 3.

## The hard problems (why this is a subsystem, not a patch)

1. **Velocity scale / tunnelling.** Real impacts are km/s; a grain crossing >1 m per substep tunnels.
   17 km/s would need ~16 000 substeps/frame — impossible real-time. So the *microsecond shock itself*
   cannot be explicitly resolved. The honest move is to conserve the meteor's real **momentum + energy**
   and let the (resolvable) **rebound + flow** be emergent — the sub-µs shock is the one thing we
   approximate, and we say so. Options to evaluate: momentum-impulse into the contact region; a
   sub-frame continuous-collision pass; or depositing the energy as real **pre-compression** (contact
   overlap = stored PE) that the physics then releases.
2. **Elastic rebound vs. inelastic settling.** Ejection needs the contact to *store and return* energy
   (restitution); settling needs it to *dissipate*. Real rock does both — elastic under the shock,
   plastic/frictional afterward. Our current contact is heavily damped (great for settling, kills
   rebound). Fix: derive normal damping from each material's real **coefficient of restitution**
   (already in `materials.json`), so rebound is a material property, not a dial.
3. **Terrain as matter at the impact.** Compression/rebound needs the *target* to be grains, not a
   heightfield, at least in the impact region. Materialize a patch of terrain into grains on impact
   (the `docs/19` LOD-materialization bridge, made real), run the contact physics there, re-freeze to
   voxels once it settles.
4. **Directional contact stabilization.** The current implicit stabilizer is isotropic and damps free
   flight (`docs/23`) — it would smother the very ejection we want. Needs the directional (per-normal)
   form first, or the ejecta never leave.

## Staged plan (each stage verifiable on the 2070 via `tools/gpu-verify`)

- **Stage 0 — directional implicit. ✅ DONE (verified on the RTX 2070).** Replaced the isotropic
  `1/(1+dt²K)` with a per-grain tensor `M = I + S`, `S = Σ(dt²k + dt·c)(n⊗n)`, solved by a symmetric 3×3
  cofactor inverse. Stabilization acts ONLY along contact normals, so a grain with no contacts gets
  `M = I` (pure explicit) and keeps its full free-flight/ejection velocity — the isotropic form smothered
  exactly this. Two physics errors the isotropic form had been *masking* fell out and were fixed:
  - **Normal damping was explicit** and, at high coordination `Z`, `Z·c·dt` exceeds the explicit
    stability limit (2) → the damper flips sign and *injects* energy. Fix: put the damping in `M`
    (the `dt·c` term above) — backward-Euler on the spring-DAMPER, unconditionally stable. (~5× less
    injection.)
  - **Coulomb friction overshot.** `μ·N = μ·k·overlap` grows huge on a deep impact; the explicit
    tangential impulse `μN·dt` can exceed `|v_t|`, *reversing* the slip and adding energy. Fix: clamp the
    friction impulse at `|v_t|/dt` — friction can only halt a slip, never reverse it (dissipative).
  Result: `gpu-verify` scene **I-flat** (grains on a FLAT floor) conserves energy — mechanical energy
  only falls. Free flight is preserved by construction (`M = I` off-contact). Friction stays an explicit
  FORCE (not implicit damping): a damper gives nothing at `v = 0`, so implicit friction collapses the
  angle of repose — friction must oppose the *driving load*, not just motion.
  - **Residual, now precisely localized:** the stepped-terrain scene I still injects. The isolating
    I-flat vs stepped-I comparison proves the sole remaining injector is the **heightfield terrain
    contact**: min-translation picks a discontinuous normal that FLIPS between up and sideways at voxel
    step edges, so the penalty force is non-conservative (not `−∇U` of any potential) and pumps energy
    around a step. This is a lossy-representation artifact, not a granular-contact bug — and it is exactly
    the case for **terrain-as-matter** (Stage 3/`docs/23`): real grains contact by the same conservative
    law, no heightfield, no normal flip.
- **Terrain-as-matter + Stage 2 (compression, not ejecta). ✅ LANDED (verified on the RTX 2070).** The
  meteor path no longer carves a crater and hands each grain a scripted `√(2·0.05·E/m)` outward speed.
  Instead (`crates/engine/src/matter.rs`, wired in `lib.rs::meteor`):
  1. `materialize_region` — every solid voxel in the σ·V crater region (docs/19, LOD-capped) becomes a
     grain **at rest at its own voxel centre**: mass conserved (N voxels → N grains), zero KE injected,
     PE unchanged. A change of *representation*, not destruction. (`materializing_terrain_conserves_matter_and_injects_no_energy`.)
  2. `deposit_impulse` — the meteor's **real momentum** `p = m·v` is spread uniformly over the coupling
     core, `Σ mᵢΔvᵢ = p` **exactly** (`the_impulse_deposits_exactly_the_impactor_momentum`). No ejecta
     velocity is assigned; ejection EMERGES from the driven core compressing the bed and the contacts
     rebounding. Because a small fast impactor's momentum over the core's large mass yields only a modest
     Δv, only a few percent of ½mv² becomes bulk motion — the old hard-coded "5% to ejecta" now falls out
     of momentum-vs-energy, not a magic constant.
  3. `deposit_shock_heat` — the rest of ½mv² is shock heat with a radial gradient (core melts/glows, rim
     cold rubble), conserving the energy (`shock_heat_is_hottest_at_the_impact_and_conserves_the_energy`).
  Proof (`gpu-verify` scene **J**): a materialized bed + a momentum impulse on a flat floor throws a real
  grain curtain up-and-out with NO assigned outward velocity, and total mechanical energy only ever FALLS
  after the impulse. This is the emergent, conservative impact — the crater comes from physics on grains,
  not the non-conservative heightfield edge that pumped the old crater "free energy".
  - **Honest limit (→ Stage 1 next):** scene J shows the curtain at low friction. At realistic rock μ the
    current contact FREEZES the fast excavation flow and the heavy normal damping absorbs the rebound, so
    the curtain is weak. Ejection *magnitude* at real friction needs **Stage 1 — restitution** (derive
    normal damping from each material's coefficient of restitution; let fast flow overcome static
    friction), with a bounce test (a grain dropped rebounds to ~e²·h). The conservative *mechanism* — the
    prerequisite — is what landed here.
- **Stage 1 — restitution. ✅ LANDED (verified on the 2070).** The backward-Euler contact was fully
  dissipative (measured e = 0 for every material — a dropped grain didn't bounce). Root cause: the
  Stage-0 stabilizer's `dt²k` term is numerically dissipative. Fix = a **θ-method** contact solve
  (`(I+S)v_new = (I−ρS)v_old + dt·a`, `S = θ²dt²k + θdt·c`, `ρ=(1−θ)/θ`): θ=0.5 is energy-conserving
  trapezoidal but RINGS at high coordination; θ=1 is backward-Euler (dead). θ=0.70 is the stable point —
  real restitution, no ringing. Normal damping is now DERIVED from `Material::restitution`
  (`granular::damping_for_restitution`, c = 2ζ√k). Restitution is deliberately MODEST (the stability-
  required numerical dissipation floors low-e rock to ≈0 bounce) — which is FINE, because…
- **Vapor-driven ejection. ✅ LANDED (Robin's insight — the missing physics).** At 17 km/s the crater
  ejecta are driven by **phase transition**, not elastic rebound: ½v² ≈ 1.45e8 J/kg is 30–50× granite's
  vaporization energy, so the target near the contact flashes to gas whose **expansion** throws the
  curtain. The KE was already in the sim as the shock heat we deposit and then radiate away. `matter::
  deposit_vapor_expansion`: superheat past `damage::vapor_energy_density` → radial ejecta KE (thermal →
  kinetic, conserved; the vapor cools adiabatically). CPU thermodynamics → radial impulse → GPU flies the
  trajectories. This is the real engine of the crater; restitution only needs to be honest, not large.
- **Stage 3 — meteor as a real body.** The impactor's momentum transfers by contact (today a momentum
  impulse on the core). Reconcile the km/s scale honestly (impulse vs. sub-frame CCD), flagged.
- **Everything couples honestly (no bespoke object).** `aggregate::deposit_impact` (the probe/bodies) was
  rewritten to the SAME pipeline as the terrain — momentum impulse + shock heat + vapor expansion — so the
  last scripted ejecta-velocity kick (`√(2·0.3·e/ρ)`) is GONE. And the meteor couples into **every** body
  via `couple_impact_to_bodies` (object-agnostic: direct-hit-or-blast-falloff per body), not a hardcoded
  probe — the multi-object case is built in. Honest consequence: an 890-t iron ball CORRECTLY survives a
  1000 kg / 17 km/s pebble (peak energy density ~12% of iron's vaporization threshold → no vapor, just a
  spall scar); to witness destruction, use a realistic ball size, a bigger impactor, or the Moon.
- **Stage 4 — the crater is the flow.** The bowl + rim emerge from the excavation flow re-freezing to
  voxels; no `dig`-style carve. This closes the loop with `docs/23`'s "the crater is what happens."

## The test that keeps us honest

Every stage runs under the **energy fudge-detector** (`gpu-verify` scene I): total mechanical energy may
only fall (the rest is heat). Ejection that *emerges* passes it; ejection that's *injected* (a teleport,
a scripted velocity that over-adds energy, an unstable contact) fails it. That is how we know the
crater came from physics and not from us.
