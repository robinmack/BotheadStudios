# 71 — Composition and de-resolution: an assembly of assemblies that can summarise itself

Robin, 2026-08-10, on a haystack built as one cylinder with a packing fraction:

> *"A clump of grass should be an assembly of multiple 'grass blade' assemblies. A haystack is then a
> pile of 'grass blades' with the 'dry' properties, no?"*

Yes. And `Assembly` could not express it: `parts` are shapes plus materials, with no sub-assembly
anywhere in the type, while [`docs/67`](67-everything-is-an-assembly.md) has said for days that an
assembly *"can contain other assemblies"* (docs/46 row 59).

## 1. Why this is ONE feature and not two

A bale holds **95,239 straws**. Resolved as parts that is 1.14 million triangles for one bale, and a
field of them is not affordable at any budget. So composition on its own is unusable: the moment an
assembly can contain assemblies, it can contain far too many.

The answer is already the engine's own law — *resolved* versus *declared*
([`docs/46`](46-one-physics-charter.md), `integrity-engine-resolved-vs-declared`). `Assembly::derived`
is to `parts` exactly what a de-resolved summary is to resolved matter, and it is the same
relationship `planet::LayeredBody` has to an assembly of particles. So:

**An assembly contains assemblies BY RULE, and answers as a summary until something needs the
individuals.**

That is the same shape as `containment::Contents` for flora — a rule plus an exception set — one level
down. It is not a new idea; it is the existing one applied where it was always meant to go.

## 2. The type

```text
Assembly {
    parts:    Vec<Part>       — matter of its own: a trunk, a barrel, a bale's binding twine
    contains: Vec<Contained>  — assemblies inside it
}

Contained {
    of:          String        — which assembly type
    count:       u64           — how many (95_239 is allowed)
    arrangement: Arrangement   — where they are
}
```

`count` is a number, not a list. A million blades cost the same to declare as one, and that is the
point: **a pile stores its rule, never its members.**

## 3. Resolved and de-resolved must AGREE

The invariant that makes de-resolution honest rather than a fudge, and the test this feature lives or
dies by:

> **The summary's answer converges to the resolved answer as the resolution rises.**

Concretely, for the two questions an assembly is asked today:

- **Mass.** `derive` over a contained group is `count × the sub-assembly's own mass`, so it is exact —
  not approximate — at every resolution. A bale weighs what its straws weigh.
- **Energy** (`meet`, docs/70). A de-resolved group answers as one part with the group's own envelope
  and matter volume, which gives it a packing and therefore a void; a resolved group answers per
  blade. These must agree to within a stated tolerance, and the tolerance belongs in the test.

If they do not agree, the summary is wrong and the fix is the summary — never the resolved answer.

## 3b. ★★★ The summary should be MEASURED, not computed — simulate the pile once

Robin, on seeing the bale declared as blades inside a cylinder (2026-08-10):

> *"As you pile them a stack should naturally form if you drop them all in the same location. Which is
> cool, but very slow to simulate. I wonder if we could simulate it and then map the pile as a derived
> assembly?"*

This is the honest form of everything above, and it is better than what §2 describes. Today a pile's
`packing` is *computed* — matter over an envelope somebody chose (a 1.2 m cylinder). In Robin's
version the envelope and the packing are **measured from the pile that actually forms**: drop the
members, let them settle under the engine's own contact law, and record what results.

**None of it is new machinery.** The engine already does this round trip, at planetary scale:

| Direction | Function | Scale |
|---|---|---|
| summary → members | `hydrostatic::HydroBody::particalize` | a planet's layers into particles |
| members → summary | `accretion::sample_layers` | particles back into layers |
| what a heap can stand | `granular::repose_allowance`, `critical_bank_height`, `face_stable` | any granular pile |

So "simulate the pile, then map it as a derived assembly" is `sample_layers` pointed at a haystack
instead of at a proto-Earth. **One law, every scale** — and this is the cheapest test of that claim
the project has, because the two uses share the code rather than resembling each other.

Three things make it affordable, and they are all laws already stated:

- **Once per TYPE, not per instance.** Identical until damaged: every bale in the world is the same
  settled pile until something happens to one. The simulation is a build step, not a frame cost.
- **The result is a measurement, so it carries its own provenance** — the same status as a catalogued
  material property, and subject to the same rule that it be sourced rather than typed.
- **It is checkable.** A settled pile's packing can be compared against the measured bulk density of a
  real bale (100 kg/m³). If the simulation says something else, the simulation is wrong — which is a
  far better position than a number nobody can falsify.

★ Until it is built, the packing in `haystack-bale.json` is a COMPUTED stand-in and says so. The IOU
it defers is named here: settle the members and measure the heap.

## 4. What this retires

Robin's model deletes work rather than adding it, which is how you know it is the right shape:

- **`Part::packing` on a pile stops being a declaration.** A bale's void becomes the space between its
  blades — geometry — and its packing is *derived* from the rule rather than authored.
- **The bale-scale `compressive_strength`** and the Gibson–Ashby scaling in `damage::crush_stress_pa`
  exist only because the blades were summarised. A resolved pile spends the STEM's own strength.
- Both stay, correctly, as the DE-RESOLVED answer. That is the point of a summary: it is what the
  engine says when it cannot afford the individuals, and it must say the same thing.

## 5. What it unblocks

| Row | Waiting on |
|---|---|
| 46 | `instance.rs` has no consumer — a container needs instances to hold |
| 52 | a renderer cache no model change can invalidate |
| 55 | a container does not place its contents |
| 59 | this |
| docs/67 §5 | **a planet is not an assembly** — because a planet is an accretion, and an accretion is a pile |

The last is the one that matters most. Robin, on 2026-08-05: *"A planet is an accretion of debris
bound by its own gravitational effects."* A planet is a pile of matter that summarises itself when
nobody is close enough to count the grains — which is precisely what this document describes, at a
different scale. **`LayeredBody` is the de-resolved form of an assembly**, and once composition and
de-resolution exist, saying so costs no new machinery.

---

*Written 2026-08-10 from Robin's design statement, before the code.*
