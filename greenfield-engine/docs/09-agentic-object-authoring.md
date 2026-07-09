# Agentic object authoring & physically-grounded interaction

> Design note. The long-term authoring goal: say **"make a shovel"** and an agent composes a real,
> simulated object from the material catalog — which then looks, sounds, falls, and *interacts* like
> a shovel because it's built from real materials and real physics. Status: **vision / design**.
> Builds on [`07-material-taxonomy-and-objects.md`](07-material-taxonomy-and-objects.md) and
> [`08-adaptive-resolution-and-clumping.md`](08-adaptive-resolution-and-clumping.md).

## 1. Agentic authoring: "make a shovel"

The engine provides **primitives** (shapes, materials, joints, the object compiler). An **agent**
(LLM + tools) provides the **knowledge and assembly**:

```
"make a shovel"
  → agent resolves: a shovel = blade (sheet steel) + shaft (wood/fiberglass) + grip
                    typical size ~1.1 m, blade ~0.2 x 0.3 m, masses per part
  → agent emits an Object spec (docs/07): parts[] with shape + material + size + joints
  → engine COMPILES it into a simulated body (voxelized, per-part material identity)
```

The agent doesn't hand-animate anything — it just specifies **what it's made of and its shape**.
Everything else is inherited from the materials + physics:

- **Mass & balance** from steel-blade + wood-shaft densities → it falls and swings like a shovel.
- **Sound** from the materials' event assets (steel clang, wood knock), resolved by taxonomy.
- **Appearance** from optical properties + finishes.
- **Strength** from material thresholds → the shaft can snap if overstressed.

This is the composition model of `docs/07` with an agent as the author. Same result whether a human
or an agent builds it; the agent just automates the "look up what a shovel is" step.

## 2. Physically-grounded tool ↔ terrain interaction

Push the shovel point-down into dirt. Nothing here is scripted — it emerges from contact mechanics
+ the cited material properties in [`data/materials.json`](../data/materials.json):

### Penetration = pressure vs. soil strength
- Contact **pressure** `p = F / A`, where `A` is the blade's contact **area** and `F` the applied
  force. A thin blade **edge** → tiny `A` → huge `p`; the flat side → large `A` → low `p`.
- The soil resists per its **bearing/shear strength**, a function of its `cohesion` and
  `friction_angle` (both in the material data — dirt cohesion ~10 kPa, φ ~30°). Penetration depth
  grows while `p` exceeds the soil's local resistance, and arrests when they balance.
- Result, for free: the blade **sinks** point-first but the flat blade barely dents — because of
  area, force, and real soil strength, not a special case.

### Displacement, divots, and carrying
- As the blade advances, dirt has to go somewhere: nearby bulk **activates into clumps**
  (`docs/08`) and **flows around** the blade (granular displacement), heaping at the sides.
- **Tilt + lift:** dirt above the blade rides up with it (now clumps resting on the blade); the
  cavity left behind is a **divot** — matter removed from the voxel store. Drop it elsewhere and it
  settles back into voxels (`docs/08` demote).
- The whole interaction stays within budget because only the contact neighborhood is fine; the rest
  of the ground is untouched bulk.

### What makes it "feel right"
| Observation | Emergent cause |
|---|---|
| Point sinks, flat side doesn't | pressure = F/area |
| Harder to push into clay than loose dirt | clay's higher cohesion/strength in material data |
| Dirt mounds beside the blade | granular flow conserving displaced volume |
| Lifting leaves a divot | voxel removal + clumps carried on the blade |
| Wet dirt clings, dry dirt spills | cohesion vs. moisture (material state — future) |

## 3. Why this is tractable (and where it's hard)

- **Tractable:** every piece reduces to systems we're already building — material properties (done,
  seed data), object composition (`07`), the matter sim (Phase 3), and adaptive clumping (`08`).
  There's no bespoke "shovel code"; a rake, a drill, or a boot all interact by the same rules.
- **Hard / honest:** this is the **far end of the roadmap**. It needs the MLS-MPM matter solver
  (Phase 3), the object compiler, robust LOD transitions, and agent integration. Each is real work
  and each has open questions (`07`/`08`). We reach it incrementally, not in one leap.

## 4. Roadmap fit

- Prereqs: Phase 3 matter sim + `docs/07` object compiler + `docs/08` clumping.
- Then: (a) contact-mechanics penetration model, (b) object spec format an agent can emit,
  (c) an agent tool that turns a noun ("shovel") into that spec against the material catalog.
- Early milestone worth targeting: a single rigid tool (fixed shape) pushed into dirt with correct
  pressure-based penetration + divot — proves the interaction before full agentic authoring.

## Open questions

1. **Object spec schema** the agent emits (parts, joints, constraints) — formalize alongside `07`.
2. **Contact model fidelity** — analytic bearing-capacity approximation vs. full granular contact.
3. **Agent trust/validation** — sanity-checking agent-authored objects (mass, proportions, materials)
   before they enter the sim.
4. **Determinism** — whether agent-authored objects need to be reproducible across clients.
