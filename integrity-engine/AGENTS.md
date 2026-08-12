# AGENTS.md — the shared contract for anyone's coding agent

This file is **vendor-neutral on purpose**. `CLAUDE.md` is Claude Code's own memory file and carries far more
detail; this one holds the things that must hold *no matter whose agent is working*, because more than one
person now contributes and their assistants have to agree with each other. `CLAUDE.md` imports it, so the two
never drift.

> **Why this file exists.** Three numbering collisions happened in three consecutive integration steps of one
> contributor's work. Nobody was careless: both sides took "the next free number", and both were right about
> what that meant *on their own branch*. That is the failure mode this file addresses — not carelessness,
> but two people appending to one namespace without a shared clock.

## 1. The Laws come first

`docs/00-laws-of-integrity.md`. Real physics, one law at every scale, faked nowhere. Every design decision —
including how you resolve a merge conflict — is decided by those, not by what makes the compiler happy.

## 2. Prose is not a gate

**If a rule can be checked by a machine, it must be**, in `crates/engine/src/laws.rs`, so it fails in CI —
including on a fork's own pull request, before it ever reaches this repo. A rule that lives only in a
markdown file reaches a collaborator whenever they last merged, which is by definition older than the work
they are about to send.

This was learned twice the hard way: a warning about `mod app` sat in bold in `CLAUDE.md` for months and
still cost a merge, and the numbering rule below only started working when it became a test.

**Verify a new gate by making it fail.** One gate here was written so that it printed the failure and then
exited 0. A gate that reports a problem and passes is worse than no gate: it teaches you to trust it.

## 3. Numbering is a shared namespace — CI enforces it

- **`docs/NN-name.md` numbers are unique.** A document's number is how code cites it (`docs/59`), so two
  files cannot share one. Enforced by `laws::numbering_tests::no_two_documents_claim_the_same_number`.
- **`docs/46` ledger rows are unique and contiguous from 1.** Enforced by
  `laws::numbering_tests::the_conformance_ledger_numbers_each_row_once`.
- **If you are working on a fork, your "next free number" is stale.** Merge `main` before claiming one. If
  two land anyway, the *incoming* one is renumbered onto the end and its references updated — never renumber
  what is already published.

## 4. Merges keep history, they do not flatten it

**Merge commits, never squash**, for anything that comes from a fork. A squash makes the contributor's
original commits unreachable from `main`, which re-diverges their fork: every later step then merges against
a much older base. Measured here — keeping merge commits held each of a nine-step integration at 2–8
conflicted files; squashing would have re-inflated them toward 12+.

`.git-blame-ignore-revs` lists formatting-only commits. Anything added there must contain **no decisions** —
a blame-ignored commit is one nobody will ever be shown.

## 5. Before you push

```bash
bash scripts/test.sh              # the full gate: the suite AND `mod app` compiled for wasm32
cargo fmt                         # stock rustfmt defaults; there is no rustfmt.toml, deliberately
bash scripts/commit.sh MSG.txt    # commit — the message comes from a FILE, never from the shell
```

★ **Write the commit message to a file, always.** Messages here are long and explain reasoning, so they are
full of the characters a shell eats — backticks around identifiers, `$`, `!`, quotes. A heredoc *looks* safe
and is not; an unquoted one still performs command substitution. It has happened twice, and the second time
it silently deleted the subject of a sentence from a merge commit that was already pushed. `scripts/commit.sh`
removes the shell from the path, adds the co-author trailer if missing, and prints `parents=[a b]` so a merge
commit can be confirmed to still be a merge.

★ **The wasm32 gate is still required, but for a smaller reason than it used to be (changed
2026-08-09).** ~~"A native `cargo check` is green for code that does not build — the scene structs live
behind `#[cfg(target_arch = "wasm32")]`, so they are invisible to it."~~ That was true for most of this
project's life and it cost a merge. **The scenes now build on both targets** (`renderer::Target`, docs/69):
a native check sees `Terra` and `OrbitDemo` in full. What the wasm gate still covers is the browser HOST —
`create(canvas)`, the swapchain, `wasm_bindgen`'s generated bindings — which native cannot see, and which
is where the wasm-only code now lives. The full `scripts/test.sh` run compiles it; `--fast` skips that, so
`--fast` is for the inner loop only.

★ **Do not tune a threshold to make a test pass.** If a timing assertion goes red because the machine got
busier, fix the measurement environment, not the number. A number from a contended environment is a number
about the contention.

## 6. Resolving someone else's conflict is a design decision

It just wears a chore's clothing. Nobody reviews a resolution the way they review a commit, and the author of
the losing side never finds out. So:

- Resolve against the Laws, not against the compiler. The build is equally happy with the right answer and
  the wrong one.
- **Check what the merge DELETED, not only what it flagged.** When a fork predates work now on `main`, git
  will apply half of a deletion without reporting a conflict — removing declarations while keeping their
  uses. That has happened here, and the native test suite passed with it in the tree.
- Record every decision made in someone's code without them in `merge-reports/YYYY-MM-DD-<contributor>.md`,
  especially the ones they should overrule, and give it to them with the pull request.
