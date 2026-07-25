# Merge reports

One file per contributor whose work we integrate on their behalf, named `YYYY-MM-DD-<contributor>.md`.

**Why this directory exists.** A merge resolution is a design decision wearing a chore's clothing. When two
people have independently changed the same lines, whoever resolves the conflict chooses whose architecture
survives — and nobody reviews a resolution the way they review a commit. The resolutions are precisely where
two designs get silently welded together, and the author of the losing side never finds out.

So when we take someone's work rather than have them merge it themselves, we write down:

- **what came in**, branch by branch, with the commit and its author;
- **every conflict**, and how it was resolved;
- **every decision made in their code without them** — especially the ones they should overrule;
- **decisions we made and then reverted**, and why, because those are the most instructive entries;
- **anything their work taught us about ours**, including bugs of ours their code exposes.

The last two matter most. A report that only lists what we kept is a changelog; a report that records what we
got wrong is something a contributor can actually check.

**The standing rule these reports establish: resolve conflicts against the Laws (`docs/00`), not against the
compiler.** The `MOONLET_UNIS_N` entry in the first report is the worked example — the build was equally happy
with the right answer, the wrong answer, and a false-dependency version in between. Only the Laws, plus
knowing which code path had moved to the GPU, separated them.

**Give the report to the contributor with the PRs.** It is not an internal note; it is the half of the review
they cannot do for themselves.
