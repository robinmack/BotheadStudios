# North star: a metal ball at ground zero, the Moon hits, zoom in, it's gone

> The demo that proves the whole engine: **place a metal ball on the Earth's surface, de-orbit the Moon
> into that spot, then zoom from the celestial view down to the ball and observe that it was
> destroyed** — vaporized, melted, and blown apart — *without a single line of code that says "destroy
> the ball."* It is destroyed because the impact's energy really reaches it and exceeds what iron can
> survive. Everything is real matter; the same physics acts at every scale. Status: **north star**;
> requires the unifications below.

## The rule this enforces: everything is matter (no bespoke objects)

The reason a meteor doesn't currently obliterate the probe on its own — the reason we were tempted to
write `if probe within crater { destroy }` (a **fudge**, reverted) — is that the probe is the **last
bespoke object**: a rigid `body::Sphere` the impact operator can't see. `matter::impact` acts on
*voxels*; the probe isn't voxels, so nothing happens, so we reach for a script. Robin's instinct is
exactly right: **the need to special-case it is the bug.**

The fix is not to script the probe's death. It is to make the probe **real matter**, so the operators
we already trust — gravity, contact, `matter::impact`, `damage::classify`, `emission` — act on it with
no special case:

- **Deposited energy density** at the ball vs. **iron's thresholds** (fracture strength → melt →
  vaporize, `docs/20`) decides its fate — Intact / dented / shattered / vaporized — *emergently*.
- Below threshold it's merely **driven into the ground / deformed**; above, it **shatters and melts**.
- It falls, rests, and glows by the same rules as everything else.

## What must become real (the unifications)

1. **The probe → real matter.** Replace the rigid `Sphere` with the ball as **matter**: either a
   cohesive **particle aggregate** (`docs/21`, but bound by iron's *material strength*, not just
   gravity — a solid holds itself together by bonds) or a small **voxel body**. Then `matter::impact`
   deposits energy into it and `damage::classify` decides per parcel — destruction is emergent. This
   also fixes "digging under it doesn't drop it" (its support is real matter, checked by the same
   collapse rule).

2. **Impacts affect the awake set, not just voxels.** An event deposits energy into *all* nearby matter
   in range — voxels, the ball, debris — through one operator (`docs/16`). No per-object checks.

3. **Scale-relative materialization of the celestial impact** (`docs/19` LOD bridge, made real). The
   Moon–Earth collision is a celestial energy event; zooming into ground zero **materializes** the local
   matter (the ball + a terrain patch) and runs the *same* impact/thermodynamics there, with the energy
   carried down from the celestial summary. The ball's destruction at the small scale is the same event
   as the flash at the large scale — conserved across LOD.

## Why it's honest all the way down

Every layer is already built and tested in isolation: material thresholds (`damage`), impact energy
(`orbit`, `matter::impact`), incandescent emission (`emission`), aggregates that bind and shatter
(`aggregate`), the crater bridge (`docs/19`), GPU-parallel particles (`docs/22`). The north-star demo
is what happens when the **probe stops being special** and these compose across scales. No fireball,
no scripted delete — just matter, energy, and the same laws, observed at whatever zoom you choose.

## Emergent friction (a consequence we noticed)

Static vs. kinetic friction — which hand-tuned engines fake with two constants μ_s, μ_k — falls out of
the **same** bond mechanics, applied at the **contact** between surfaces. At rest, contact asperities
**settle into their ground state** (bonds fully form) → more force to break them → higher static
friction. Sliding never lets them settle (continuous stick-slip) → lower kinetic friction. So μ_s > μ_k
is a *consequence* of bond formation + dissipation, not a tabulated fact. (`materials.json` carries
`friction_coefficient`/`restitution` as placeholder summaries today — like `albedo` — that this would
derive.) One more special case that dissolves once the contact is real matter.

## Roadmap

1. **Probe as real matter** (cohesive aggregate / voxel body) — impacts, gravity, contact, damage all
   emergent; kills the last special case. **Landed (native, tested):** `Aggregate::cohesive` — a
   bonded solid that settles to a ground state (bond spring + damper) and shatters under a hard impact
   (bonds fracture). Next: wire it as the probe (falls, rests on terrain, rendered), so a meteor
   destroys it on its own.
2. **`impact` affects the awake set** (bodies + voxels) through one operator.
3. **Celestial → local materialization** (the zoom-in), carrying the impact energy down.
4. **The demo:** ball at ground zero → Moon de-orbit → zoom in → emergent destruction.
