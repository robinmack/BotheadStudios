# The Laws of Integrity

> The moral compass of the Integrity Engine. Read this first, every session. Everything else in the
> engine — every doc, every module, every decision — derives from these. When a choice is unclear, or
> the ground feels lost after a long session, come back here: the Laws decide it.
>
> These were not invented; they were *earned*, over many sessions, usually by getting them wrong first.
> Treat them as settled. Extend them only with the same evidence that won them.

---

## I. Physics is the product.

Integrity sells **real physics**, not graphics that resemble physics. The simulation is the thing; the
picture is a report of it. Every feature, optimization, and shortcut serves the physics or it does not
belong. When you are tempted to make something *look* right, stop — that instinct is the enemy of this
engine.

## II. One law, every scale, every scene. A world is a world.

The same contact law and the same gravity law govern a raindrop on a petal, a tyre on gravel, and a moon
striking a planet — differing only in **scale, material, and energy**, never in kind. An engine that
answers one physical question two different ways in two different scenes has broken its central promise,
however good each answer looks alone. Before you write a primitive, grep for the one that already exists;
when you wire a law, enumerate its consumers so none keeps a private, divergent answer.

## III. Simulate what you can; compute what you can't; **fake nothing.**

Full particle simulation is the ideal and is bounded only by compute. So the engine does the honest best:
**math sizes the interaction** (energy → the footprint that will actually respond), the **minimal
necessary matter is turned into real material particles**, and those are **simulated as thoroughly as is
practical**. Everything not simulated is carried by real math, never by decoration. Resolve by necessity,
not by whim; the un-resolved world is still *computed*, just cheaply.

## IV. The camera changes representation, never existence.

Physics happens whether or not anyone is looking. An unwatched wheel still sinks; a strike on the far side
of the planet still cratered. What the camera decides is only **how** that physics is computed: cheap
**math** while it is out of view, full **particle simulation and render** when it is in view. Effects
computed off-screen **propagate**, and are rendered the moment they come into sight. Looking away must
never change what is true.

## V. NO FUDGE. Ever.

No dial, constant, clamp, or tuned coefficient exists to make something "look real." Illusions are a trap
that corrodes Integrity from the inside, because they *work* — they pass the eye while breaking the
promise. Every number traces to physics or is an **openly flagged IOU** (a resolution/declaration debt
that names the real computation it stands in for and would converge to it). A declared model with no such
counterpart is a fudge wearing a physics coat. If the physics disagrees with what you hoped, **record the
physics** — never tune the outcome.

## VI. Physics drives the render — never the reverse.

The simulation decides; the render reflects. Never move matter to improve a picture, and never let a
visual criterion (is it on camera? does it look nice?) decide what is *simulated*. Interest may decide
what is drawn; only necessity decides what is computed.

## VII. Measure and derive; never assume.

A number you did not measure or derive from first principles is a guess, and guesses are wrong often
enough to be treated as wrong until checked. **Test, then conclude.** Pin every acceleration to a
brute-force reference so speed cannot change the answer. Report findings, not triumphs; a negative result,
honestly measured, is a real deliverable. When in doubt, run the experiment — the engine's whole reason
for existing is that reality is the authority, not our expectation of it.

**A distinction inside this law: check the MATERIALS DATA before making any claim about a material's
physical behaviour.** Intuition about substances is unusually confident and unusually wrong, and it fails
in a way the rest of this law does not catch — it feels like knowledge, not like a guess, so it never
prompts a measurement.

Both directions of that failure happened on the day this was written:

* Filling in the catalogue's missing thermal data, oak and rubber were correctly given no melting point —
  they pyrolyse — and limestone and concrete were put in the same box. **They do not belong there.**
  Calcite calcines at 1,098 K on a kiln floor *only because the CO₂ can escape*; confine it and the
  reaction is pushed back, the breakdown temperature climbs past the melting curve, and the same rock
  melts near 1,612 K. That is precisely the regime inside an impact — the one place the engine most needs
  to be right. Melting versus decomposition is not a label a material carries; it is a race that pressure
  decides.
* Going the other way, eleven of twenty-four materials had no thermal data at all, and three call sites
  were quietly filling the gap with three different constants (specific heat 840, 1000, 1000) while a
  fourth defaulted the boiling point to infinity — making an unsourced material unvaporizable however
  much energy it absorbed. Nobody measured anything wrong; the numbers were simply assumed into being at
  the point of use, where no reader would look for them.

So: **read the entry before you assert the behaviour.** If the entry is silent, that is data to go and
source, not a gap to paper over — and an unknown quantity must stay unknown at the boundary so the caller
has to decide in the open whether it can proceed.

## VIII. This is a new kind of engine. Challenge what you "know".

**We are building a completely new type of game engine. Challenge assumptions fed by understanding of
traditional game engines that only EMULATE physics. Integrity EMBODIES physics — to the best of our
ability with the compute available.**

That last clause is not a loophole, it is the honest bound. Embodiment is the goal and the compute is
finite, so the real question is never "physics or shortcut?" but **"is this the most physical thing this
budget can buy, and does it converge as the budget grows?"** A technique that evaluates a real quantity
analytically because resolving it is unaffordable is embodiment under a constraint (Law III). A technique
that produces a convincing result by a route the physics would never take is emulation, however cheap.
The difference is whether you can name the computation you are standing in for and show your answer
approaching it.

Almost everything you have absorbed about how a renderer or a game engine is built was invented to make
a picture convincing at a price — LOD ladders, baked lighting, canned animation, colliders that stand in
for objects, "good enough" cheats promoted to architecture. Those are answers to a different question.
Here the simulation is the thing and the picture reports it, so a technique that is standard practice
elsewhere can still be the wrong shape here, and reaching for it *because it is standard* is not a
reason.

The test is not "is this how engines do it?" but **"does this embody the physics, or imitate it?"** A
borrowed technique is admissible only where it is a declared stand-in for a computation we cannot yet
afford (Law V) — derived from the real quantity, flagged, and convergent to it. If it is load-bearing
because it *looks* right, it is the enemy of this engine (Law I).

When a familiar solution arrives fully formed and obvious, that is exactly when to stop and ask what the
honest version would be.

---

**In one breath:** *real physics, one law at every scale, faked nowhere — simulated where seen, computed
where not, and never assumed where it can be measured; and never borrowed merely because it is familiar.*

These Laws are elaborated across the docs (notably the one-physics charter, resolution-by-necessity, the
resolution controller, and the scale-relative north star), but the docs serve the Laws, not the other way
round. If a doc, a comment, or a past decision contradicts a Law, the Law wins and the other is the bug.
