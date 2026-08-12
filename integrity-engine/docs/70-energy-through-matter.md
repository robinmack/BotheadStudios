# 70 — Energy through matter: how an assembly obeys physics

Robin, 2026-08-09, after being told plainly that the grass is decoration (docs/46 row 57):

> *"Perhaps we need to either pre-bake how collisions/heat points/other physics phenomena are handled
> by each assembly or perhaps each assembly needs its own connection to the engine with its
> parameters. We need a way for grass to wave in the wind or burn, or be smashed/burned in a very
> energetic impact, or to be cut/broken. Each assembly needs to function physically in a proper way
> that can then be rendered. This is where the work we did on the physics engine makes sense (how it
> varies calculations based on energy, etc)."*

and, naming the mechanism:

> *"Each impact with each assembly can be debited from the total energy of a collision for zero
> loss… but that way we could have a rock fall on a haystack, unsettle the hay (dry grass), but the
> impact could be absorbed."*

That second quote is the design. This document is it written down.

## 1. The rule

**An energy event travels through matter and is debited by everything it meets.** Each piece of
matter answers one question — *what does this much energy do to me?* — using its own material and its
own geometry, spends what it takes, and passes the remainder on. Nothing is lost and nothing is
invented, so conservation is a property of the arithmetic rather than a thing to check for.

```text
arriving_J  →  [ part ]  →  [ part ]  →  [ ground ]  →  remainder
                 spends       spends       spends
```

A rock onto a haystack, a musket ball into oak, a meteor through a canopy and a boot on a grass clump
are **the same call** with different materials, different geometry and different energies. That is
Law II, and it is the whole reason this belongs to the assembly rather than to any scene.

## 2. It is the law the engine already has, at a different scale

`damage::crater_volume(E, σ) = E/σ` — energy over strength gives the volume it can break. That
function already decides what a meteor does to a planet (`interaction::respond`). Read backwards it
says what an assembly's PART costs to destroy:

```text
E_needed = σ · V_matter
```

So `Assembly::meet` calls `damage::crater_volume` rather than restating `E/σ`. **One question, one
answer, at every scale** — a grass blade and a continent are the same arithmetic, which is exactly the
claim this engine exists to make.

## 3. What an assembly answers

- **How much energy it spent** and **how much continues** — the debit.
- **Which parts were broken, and how far** — a fraction per part, which is what
  `instance::Damage::part_integrity` was built to hold and has never been given (docs/46 row 46).

The instance carries the answer; the TYPE stays pristine. That is the scalability law already
recorded: identical until damaged, so a clump nobody has touched need not exist.

## 4. Channels, and which are open

Energy arriving at matter can go into several places. Each is a real, separately-derivable quantity,
so they are listed here with their state rather than folded into one number:

| Channel | Physics | State |
|---|---|---|
| **Fracture** | `E = σ·V` — break it | **BUILT**, and it is `damage::crater_volume` |
| **Displacement** | `E = ½mv²` — move it | IOU: needs the struck part's mass and the impulse, both already on the assembly |
| **Compaction** | crushing porosity: `packing` rises, void does work | IOU, and it is *the haystack*. Straw absorbs by compacting, not by snapping |
| **Heat** | `oxidation::apply_heat`, `damage::classify` (melt/decompose/vaporise) | Functions BUILT, unwired to assemblies |
| **Combustion** | `oxidation::burn` — dry grass burns | BUILT, unwired |

★ **The haystack is deliberately the case that is NOT fully answered yet**, because it is the one
Robin named, and pretending otherwise would be the failure this project keeps recording. Fracture
alone will let a rock break straw cheaply and carry on; what really stops it is compaction. That is
written here as a named IOU with the quantity it defers, not smoothed over.

## 5. What this is not

- **Not a per-assembly solver.** One implementation, many instances — per-assembly solver STATE,
  never per-assembly solver CODE. Gate it by counting implementations (CLAUDE.md).
- **Not a render input.** The renderer will show damage because the model has it, never the reverse
  (Law VI). A blade that draws as broken is broken.
- **Not resolution-dependent.** The camera decides whether damage is DRAWN, never whether it happened
  (Law IV). A bus that crushes thirty trees off-camera has crushed thirty trees.

## 6. The test that decides whether this is real

Row 57's exact wording: *the meteor meets nothing.* So the acceptance test is that it meets
something —

- a rock onto a **grass clump** loses almost nothing and continues;
- the same rock onto an **oak** is stopped;
- and in both cases **spent + remaining = arriving, exactly**.

The third is the one that makes the first two trustworthy.

---

*Written 2026-08-09 from Robin's design statements, before the code, so the code can be checked
against it.*
