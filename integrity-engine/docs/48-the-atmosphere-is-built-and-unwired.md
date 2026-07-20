# docs/48 — Earth has no air: a verified atmosphere, wired into zero scenes

> **The finding.** `atmosphere::AirField` is a working, tested, pressure-layered atmosphere — hydrostatic
> balance, drag with momentum conservation, and hypersonic entry heating all pass today. It is
> **instantiated in no scene**. Every "Earth" scene renders an honest sky over a vacuum.
>
> **The pattern.** This is the third time the same shape has appeared: physics built and verified, then
> wired into one place or none. The engine's gap is not capability. It is **wiring**.

---

## 1. What exists, and what it proves

`crates/engine/src/atmosphere.rs` (809 lines, 11 passing tests) contains two separable things.

**The physics — `AirField`:** SPH gas parcels with ghost-particle boundaries, `specific_gas_constant`,
`scale_height`, `gas_column_accel`. Its tests are not smoke tests; they are emergence tests:

| test | what it establishes |
|---|---|
| `a_settling_air_column_finds_the_real_exponential_atmosphere` | parcels under gravity, given ONLY the declared gas constants, settle to the exponential profile with **H = R_s·T/g ≈ 8.4 km**. Started from the WRONG scale height (half and double) and both converge — the equilibrium is an **attractor of the physics**, not an imposed profile |
| `the_sph_air_field_is_normalized_symmetric_and_finds_hydrostatic_balance` | the 3D SPH field, not just a 1D column |
| `a_dense_body_ploughing_through_air_feels_drag_and_momentum_is_conserved` | **drag as an interaction**, with momentum conserved — not a scalar coefficient |
| `hypersonic_entry_heats_the_swept_air_to_incandescence` | **re-entry heating** |
| `air_parcels_released_in_vacuum_expand_freely_and_never_clump` | it does not fake cohesion in vacuum |
| `airs_declared_constants_give_the_real_gas_constant_and_scale_height` | the constants are the material's own |

**The optics — `rayleigh_tau` / `rayleigh_veil` / `rayleigh_transmit`:** single-scatter sky colour, and
`the_blue_marble_is_derived_from_the_air_not_painted` passes, so the blue genuinely derives from real gas
data rather than being art-directed.

## 2. What is wired

```
grep -rn 'AirField' crates/engine/src/lib.rs   →   (nothing)
```

Every scene uses only the Rayleigh functions. So the sky is **drawn correctly over a world with no air
in it**. `matter::DRAG` is a scalar constant, unrelated to the verified drag interaction above.

Consequences, all live today:

- **Ejecta fly vacuum trajectories.** A crater's debris blanket is computed with no atmospheric braking.
- **Meteors do not ablate.** `hypersonic_entry_heats_the_swept_air_to_incandescence` passes in a test and
  never runs in a scene; the terrain meteor arrives undiminished.
- **No buoyancy, no wind, no altitude-dependent drag.** Snow cannot drift; dust cannot hang.
- **Orbit-to-ground descent has nothing to descend through** — and re-entry *is* atmosphere.
- A go-kart has no aerodynamic drag, and no air in tyre or suspension.

## 3. Why this is a distinct violation class

docs/46's ledger lists places where one physical question has **two answers**. This is different: the
render asserts a physical state **the simulation does not have**. The optics are honest; the world under
them is empty. That inverts *physics drives the render, never the reverse* — not by faking the picture,
but by leaving the picture's subject unbuilt.

It earns a ledger row of its own (docs/46 §3 item 12), because the failure is invisible to every existing
check: the atmosphere tests pass, the sky tests pass, and no test asks whether anything **instantiates**
what they describe.

## 4. The pattern this is the third instance of

| verified physics | wired into |
|---|---|
| docs/39 JIT particalization (`field → particalize → simulate → quiesce → bake_back`, conserving to <1e-12) | planetary scale only; **terrain never** |
| `granular::terrain_contact_resolve` (non-injecting, energy-monotone, hardware-verified) | GPU grains only; bodies **never**, until PR #15 today |
| `atmosphere::AirField` (hydrostatic, drag, entry heating) | **nothing** |

Three independent cases, one shape: **the law is built and proven, then wired into one place or none.**
Worth naming because it changes where effort should go. The instinct on finding a gap has been "we need
to build X"; the evidence says the more likely truth is "X exists, verified, and nothing calls it." That
is also why the déjà-vu rule (CLAUDE.md) pays: the second-cheapest thing after reading the docs is
grepping for the primitive before writing one.

## 5. Direction

Not a schedule — the ordering constraint is that these are prerequisites for things already queued.

1. **Instantiate ONE `AirField` per world**, shared by every Earth scene. Not one per scene: that would
   be a fresh docs/46 violation (the same question, three answers). The terrain band, the orbit band and
   Terra should read the same air.
2. **Derive the sky from that instance**, not from a parallel `planet::earth().surface_pressure()`
   constant. Then the blue you see is the air you are standing in, at the pressure the physics reports —
   and the two can never drift.
3. **Replace `matter::DRAG`** (a scalar) with the verified drag interaction, so ejecta and meteors are
   braked by the air that is actually there.
4. **Resolution by necessity applies here too** (docs/44/47): air is a field, particalized where an
   interaction needs it — a re-entry trail, a dust plume, a wheel's wake — and returned to the field
   otherwise. A fully resolved atmosphere is neither affordable nor required.

## 6. Acceptance

- **A meteor slows down.** Entry velocity at ground < entry velocity at the top of the patch, by an
  amount the air's own density profile predicts.
- **The sky's blue and the air's pressure come from one number.** Change the surface pressure and both
  the render and the drag change together, because there is only one source.
- **Ejecta range shortens** relative to the vacuum trajectory, and the difference is attributable.
- **No scene has its own atmosphere.** One instance, many readers.

---

**Related:** docs/25/26 (the atmosphere emergence tests these implement) · docs/32 (`AirField` and
aggregate vapor already share the SPH kernel) · docs/39 (JIT particalization — the same
wiring-gap pattern) · docs/44/47 (resolution by necessity and granularity — air is a field too) ·
docs/46 (the one-physics charter; this adds ledger item 12).
