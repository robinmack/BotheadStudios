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

---

**In one breath:** *real physics, one law at every scale, faked nowhere — simulated where seen, computed
where not, and never assumed where it can be measured.*

These Laws are elaborated across the docs (notably the one-physics charter, resolution-by-necessity, the
resolution controller, and the scale-relative north star), but the docs serve the Laws, not the other way
round. If a doc, a comment, or a past decision contradicts a Law, the Law wins and the other is the bug.
