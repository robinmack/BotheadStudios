# Unified dynamics — one awake-set loop, no per-object scripting

> Design note. Every dynamic solid — a dropped probe, a clod of dug debris, a future crate or
> character — is the **same kind of matter**: it has mass, position, velocity, and it obeys the one
> gravity field and resolves contacts against the world and against everything else. There is no
> privileged "the probe" with bespoke behaviour. Status: **in progress** (probe↔debris coupling
> landed; many-body + spatial index pending). Realizes the honesty invariant (`docs/15`) for
> dynamics; feeds the matter model (`docs/04`) and adaptive budget (`docs/08`).

## The principle

> A physics loop looks at every object **not at rest** and makes it react — gravity at least, then
> contacts — as a natural property of the world and the object, never a script.

Consequences must be *emergent*, because the whole point is a world honest enough to be believed (and,
longer term, honest enough to **learn from** — see "Why honesty is the product"). If a behaviour is
hand-authored per object, the world is lying, and anything (a player in VR, or an agent) that infers
rules from it infers fiction.

## What was wrong before

The probe (`body::Sphere`) and the debris (`matter::MatterSim`) were two separate systems that shared
only the voxel grid — `matter.rs` never referenced the probe at all. So:

- Particles could not push the probe and the probe could not push particles; the only coupling was the
  grid changing underneath. The "reaction" you saw when digging near the probe was the *ground*
  changing, not momentum — a tell that the probe was a special case.
- Worse, settling debris deposited voxels into the probe's own column (blind to the probe), so matter
  re-materialised *under/inside* it and it appeared to "rest on nothing" while clearly over a hole.

That is exactly the per-object fakery `docs/15` forbids.

## The model now

One awake-set step per substep (`Engine::step_physics`), each entity under the same field:

1. **Integrate** every awake body under `gravity::acceleration_at` (mass-independent free fall).
2. **Body↔world** contacts (`body::Sphere::collide`): push out of solid voxels, restitution + friction.
3. **Debris** step (`MatterSim::step`) under the same field; settling deposits into the grid **unless a
   body occupies the cell** — debris piles *on* a body, never through it, and is never deleted to make
   that true (matter is conserved).
4. **Body↔debris** contacts (`MatterSim::couple_body`): mass-weighted, momentum-conserving impulses
   both ways; contact wakes both. A thrown clod shoves the probe; the probe scatters debris it plows
   into.

**Sleep / wake is structural, not a flag someone sets:** a body sleeps only while it is *in contact and
slow*; the instant its support is removed or something touches it, the next step finds no contact (or a
new impulse) and it wakes and falls. "Leave it unsupported and it falls" is a guarantee, proven by
`body::wakes_and_falls_when_support_is_removed`.

**Verified (native, TDD):** `particle_transfers_momentum_to_a_body` (momentum conserved through the
impact), `debris_does_not_settle_inside_a_body`, `wakes_and_falls_when_support_is_removed`.

## Compute budget — favour the large and massive

Under a fixed per-frame budget, spend it where it matters most: **larger / more massive objects get
simulated and rendered first; small stuff is coarsened, never faked.** Rationale: mass dominates both
the dynamics (momentum, gravity) and the view, so degrading the least-massive matter first is the
least-perceptible, least-wrong way to stay in budget.

- **Massive bodies are budget-exempt today:** the probe is a first-class body, not subject to the
  debris particle cap, so it is always fully simulated and drawn.
- **Debris coarsening must conserve mass** (this is the hard part, and why it is *not* yet
  implemented): a proper mass-priority policy merges many small grains into fewer, heavier **clumps**
  that carry the aggregate mass (`docs/08`), rather than dropping particles (which would leak matter
  and break conservation). Critically, a merged clump must also **deposit its full volume** when it
  settles — depositing one voxel for a many-voxel clump would silently delete mass. Until clump-merge
  handles both spawn and settle conservatively, the budget is held by the existing conserving fallback
  (stop detaching new voxels — they simply stay as terrain).

## Honesty caveats (things we must not pretend)

- **No atmosphere.** Matter falls through **vacuum** — there is no air, so no buoyancy, pressure, or
  aerodynamic drag. The `DRAG` constant in `matter.rs` is a numerical stabilizer flagged as debt, not
  physics; an atmosphere (and real drag/pressure/friction) is a future subsystem with its own
  complications, not a constant.
- **One body, generalizing.** The loop is written for many bodies but exercised with one; body↔body
  contacts and a spatial index (so it isn't O(n²)) come with the second dynamic object and the MLS-MPM
  path (`docs/06`, `docs/08`).

## Related trajectories to watch

- **Server-authoritative world, client sees a slice.** As the world/universe grows past what a browser
  can hold, the full state moves to a beefy server and the client streams only the observable slice —
  which is precisely the scale-relative, observer-relative architecture of `docs/13` plus the
  networking model of `docs/11`. Watch for the threshold where single-process (browser) simulation of
  the whole world stops fitting; that is the trigger to split.
- **Why honesty is the product.** A world with real, observable, inferable physical consequences is a
  place to *learn to act* — for a person in VR, and plausibly for an AI learning to operate under
  physical law. That payoff exists only to the exact degree the simulation refuses to fake. It is the
  deepest reason `docs/15` is canonical.
