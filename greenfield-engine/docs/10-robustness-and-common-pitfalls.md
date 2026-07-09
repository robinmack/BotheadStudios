# Robustness — designing out the classic game-physics bugs

> Design note. A stated goal: avoid the pitfalls that plague many engines — characters falling
> through the world, objects jittering or exploding, things sinking into the ground, non-deterministic
> "weird physics." This note explains why the matter-first architecture prevents whole classes of
> these structurally, and which techniques we adopt for the rest. Status: **principles / design**.

## Why "real matter" helps structurally

Most tunneling and fall-through bugs come from representing the world as **infinitely thin collision
shells** (a triangle mesh, a plane) with gameplay logic layered on top. A fast body can cross a
zero-thickness surface between two frames, or a small numerical error puts it on the wrong side.

greenfield-engine represents the world as **solid volumetric matter** — it's rock for meters, not a
plane at the surface. That changes the failure modes:

1. **Nothing to fall "through."** The ground has real thickness (the 200 km sphere is solid). A body
   that slightly over-penetrates is still *inside matter*, not through it — recovery pushes it back
   up, it doesn't pop out the far side of a thin shell.
2. **One consistent source of truth.** Collision, resting contact, gravity, and buoyancy all derive
   from the same per-voxel material/density field. There's no divergence between "the visual mesh,"
   "the collision mesh," and "the gravity volume" — a common source of objects resting slightly
   above/below the visible ground.
3. **Gravity is real and continuous** (from summed mass, `F=ma`), so characters are pulled onto and
   held against actual surfaces rather than snapped by ad-hoc ground checks that can miss.

## Techniques we adopt (the rest of the job)

Structure helps, but robust physics still needs discipline:

- **Continuous Collision Detection (CCD) / swept tests** for fast movers — Rapier supports CCD.
  A bullet or a dropped mass is tested along its whole path each step, so it can't skip past thin
  geometry between frames. *No tunneling at speed.*
- **Fixed timestep + substepping.** The solver runs at a fixed dt (with substeps for fast/stiff
  contacts) independent of render framerate. Stable, and a prerequisite for determinism.
- **Soft / stabilized constraints (XPBD-style) and gentle penetration recovery.** Over-penetration
  is resolved by a bounded push-out (Baumgarte/position correction), not an impulse spike — *no
  explosions*.
- **Sleeping / deactivation** of resting bodies — *no idle jitter*, and it saves compute.
- **Contact margins / speculative contacts** so contacts are caught slightly early rather than after
  penetration.
- **Determinism** (Rapier's optional bit-exact builds) for replays and consistent multiplayer — the
  same inputs give the same result on every machine, so "weird physics" is reproducible and fixable.
- **Sanity guards:** clamp max velocity, reject NaN/inf states, and refuse implausible per-step
  teleports — a last line of defense that turns a silent glitch into a caught, logged event.

## Matter-sim–specific invariants

The adaptive clumping/LOD system (`docs/08`) adds its own rules to avoid new artifacts:

- **LOD transitions conserve mass and momentum** — promoting voxels→clumps→grains or settling them
  back must not inject or delete energy (no "pop", no drift).
- **Activation seams don't add energy** — coupling static bulk to active particles at the boundary
  must be force-consistent, or matter appears to "boil" at the edges.
- **Resting matter demotes to static voxels**, which are immovable by construction — so settled
  terrain is rock-solid and cheap, and can't slowly creep.

## We prove it with adversarial tests

Robustness is verified, not assumed. A regression suite of known-nasty scenarios runs in CI:

- A high-speed projectile vs. a one-voxel-thick wall (must not tunnel).
- A character standing/walking on a steep slope (must not slide through or jitter).
- A tall stack of objects (must not vibrate or explode).
- A heavy object dropped onto soft ground (must sink per material strength, then rest — not fall
  through, not float).
- Long idle soak (a resting scene must stay perfectly still and use ~no compute).

Each maps to a pitfall above; a failure is a caught regression, not a shipped bug.

## Honest caveat

No physics engine is bug-free, and a novel matter sim adds new challenges (LOD seams, granular
stability). The claim is not "impossible to break" — it's that the architecture removes the *most
common* structural causes (thin-shell tunneling, mesh/collision/gravity divergence) and that we
adopt the standard mitigations plus an adversarial test suite for the rest.
