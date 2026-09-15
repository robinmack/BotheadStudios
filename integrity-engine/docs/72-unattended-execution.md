# 72 — Unattended execution: how work continues when Robin is away

**Why this exists.** Robin has a day job and is a carer. The engine should make progress in the hours
she cannot supervise it, without that progress needing to be undone when she returns. This document is
the contract for that: what an agent may do alone, what it must not, and how the work is handed between
sessions so nothing is re-derived.

It is written against a specific, expensive lesson. Over 2026-09-01..15 a single NaN in `pile` cost a
fortnight, and **eleven hypotheses were refuted by measurement.** Almost none of the cost was the bug.
Nearly all of it was acting on plausibility before measuring, and instruments that lied in the shape of
a result. The rules below are what that bought.

---

## 1. The Laws still decide, and nothing here softens them

`docs/00` is the moral compass, unchanged. An unattended agent has **less** licence than a supervised
one, not more, because a wrong turn runs longer before anyone sees it. In particular:

- **Law V — no fudge.** If a number cannot be derived, it is a flagged IOU in `docs/46`, never a dial.
  An unattended agent that finds itself choosing a constant to make a test pass has left its lane.
- **Law VII — measure, never assume.** See §3. This is the rule that would have saved the fortnight.
- **A declared specialisation is allowed; a quiet approximation is not.** If the honest model is
  unaffordable, say so in a row with the measurement, and stop.

---

## 2. What an agent may do alone, and what it may not

### MAY, without asking

- Investigate, measure, and **report** anything.
- Write a failing test, implement against it, and land the result as a PR when the full gate is green.
- Merge its own PR **only** when *every* check reports `SUCCESS` — see §5, which exists because this
  was got wrong.
- Add rows to `docs/46`, entries to `JOURNAL.md`, and lines to `CHANGELOG.md`.
- Revert its own change and record why.

### MUST NOT, without Robin

- **Choose between physically different models.** `docs/46` row 80 has three routes with different
  costs; picking one is a modelling decision, not an implementation detail. Specify, do not choose.
- **Deploy** (`scripts/deploy.sh`) or publish to the public site.
- **Delete** a scene, a doc, or a body of measurements.
- **Relax a threshold, cap, or tolerance to make something pass.** If a timing test goes red on a busy
  runner, that is the environment (`AGENTS.md` §5) — report it, do not tune it.
- **Touch anything outside the engine** — infra, DNS, the kai-monitor repo.

---

## 3. The measurement discipline, in priority order

Each of these was earned by a specific failure. They are listed in the order they save the most time.

1. **Put the assert where the quantity is FORMED, not where the symptom appears.** Four asserts in
   `pile::step_one_rod` each cracked a problem that days of reasoning did not.
2. **Find EVERY place the quantity is formed.** The `ang_vel` guard covered the floor's path while the
   neighbour torque reached the same state by another route. *Guarding one of two paths is the same
   error as asserting a term and not a sum.*
3. **Bound by PHYSICS, not by a round number.** An impulse assert read `< 1.0 N·s`; for a 0.445 **gram**
   blade that permits `Δv = 2247 m/s`. **The assert meant to catch the explosion was letting it
   through,** and it sent three further hypotheses down blind alleys.
4. **Never calibrate and grade with the same assumption.** The three worst errors of the fortnight were
   all this: a NaN check built from `f64::max` (which *returns the non-NaN operand*); a packing figure
   quantised by its own cell; a GPU restitution gate scored against the formula it was calibrated with.
5. **Assert the CONVERGENCE, not a tolerance at one resolution.** Tuning a tolerance is asserting the
   discretisation. Report the ladder — if the error does not fall as the budget grows, the model is
   wrong, not the tolerance.
6. **State the expectation BEFORE the run.** A bit-identical result is baffling if unpredicted and
   diagnostic if predicted. This did more useful work than any code change on two separate days.
7. **A failing gate is only evidence if you know WHY it failed.**
8. **When the engine already solves or inverts something, transcribe ITS method.** A fresh derivation
   of `restitution_of_damping_ratio` hung the harness, then saturated, then produced a false pass.
9. **A number that changes by exactly the geometric factor of its own bin size is not measuring its
   subject.** Print the ratio between successive refinements; it is nearly free.

---

## 4. The sub-agent pattern that works

Used successfully on 2026-09-14 for four ledger items. **It found a fabricated ledger quotation that
would otherwise have entered the record.**

```
phase('Investigate')   one agent per INDEPENDENT item, read-only, cite file:line
phase('Verify')        a second agent per finding, told to REFUTE it
```

**Non-negotiables:**

- **Investigations are read-only.** Agents that edit in parallel collide, and a bad edit runs unseen.
  Have them report; the coordinator lands the change.
- **Every finding gets an adversarial second pass.** Its most valuable output is a refutation. On its
  first use it caught an investigation quoting the ledger as saying *"32 of 32"* — a string that
  appears nowhere in the repo. The data was right; the citation was invented.
- **One agent per item, and items must be genuinely independent.** Two agents on the same file is a
  merge conflict with extra steps.
- **Say "unknown" rather than guess**, in the prompt, explicitly.

Fan out for: catalogue audits, multi-file greps, "which of these N things is true", literature sourcing.
Do **not** fan out a single causal chain — that is one thread and parallelism cannot help it.

### The two roles are executable definitions, not prose (added 2026-09-15)

`AGENTS.md` §2: *if a rule can be checked by a machine, it must be.* The same applies to a role — a
discipline that has to be retyped into every prompt is one that drifts, and the rules above were bought
too expensively to re-derive each session. Both roles live in version control:

| file | role |
|---|---|
| `.claude/agents/integrity-investigator.md` | read-only investigation of ONE independent item; every claim carries a `file:line`; "unknown" is a valued answer; carries §3's measurement discipline in full |
| `.claude/agents/integrity-refuter.md` | the adversarial pass: resolve every citation, `grep -F` every quoted string, recompute every number, look for the second path, ask whether the instrument could have produced this while broken |

The refuter's asymmetric default is the load-bearing part: **uncertain returns REFUTED, not CONFIRMED.**
A false confirmation enters the record and gets built on; a false refutation costs one re-check.

★ **They are read at session start.** Writing the files does not make them available to the session that
wrote them — that session must load the definition into the sub-agent's prompt explicitly instead. Both
routes were used on 2026-09-15 and both work.

★ **One task per agent, and no agent edits.** Investigations that edit in parallel collide, and an
unattended bad edit runs unseen. They report; the coordinator lands the change.

---

## 5. Merging unattended

**Every check must report `SUCCESS`. Not "not pending" — `SUCCESS`.**

This is spelled out because it was got wrong on 2026-09-15: a polling loop waited for checks to stop
being *pending* and then merged regardless of outcome, landing a PR whose native suite had failed. The
failure happened to be a timing test on a contended runner (harmless, and `AGENTS.md` §5 says not to
tune it), but unattended the same loop would merge a real regression.

```bash
all=$(gh pr view N --json statusCheckRollup -q '[.statusCheckRollup[]?|(.conclusion//"X")]|unique|join(",")')
[ "$all" = "SUCCESS" ] || { echo "HOLD"; exit 1; }
```

`--admin` remains authorised only while the code-owner review is unsatisfiable (`CLAUDE.md` rule 7).
**Name the exception on every PR.** The fix is Sean accepting the invite, not a better bypass.

---

## 6. Handover between sessions

Context is lost at the end of every session. Two things survive, and both must be written **while the
finding is still in context**, not at the end when budget is gone:

- **`docs/46`** — the conformance ledger. A row carries the defect, the evidence, the fix, and the
  files. Add a row when you find a violation; update it when you measure something new. It is the
  reason the same argument is not had twice.
- **Semantic memory** (Postgres + pgvector, MCP server `memory`). `recall_for("<thing>")` **before**
  working on it. `store_memory` with `entities=` and `project=`, pinned if it must never be missed.
  ★ Verify a store landed by querying the database, not by trusting the tool's reply — the whole of §3
  applies to your own instruments too.

★ **Write the handover before the budget runs out, not after.** Every rushed edit made at the end of a
context window during the NaN fortnight had to be reverted.

---

## 7. The queue, with acceptance criteria

Ordered. Each item states what "done" means, so an agent cannot declare victory by narrative.

| # | Work | Done when |
|---|---|---|
| 1 | **Wire `tools/gpu-verify` into a runnable gate.** It is absent from `scripts/test.sh` (grep: 0 hits), so nothing runs it. It needs the 5060 Ti — ~~`MESA_VK_DEVICE_SELECT=10de:2d04`~~ **corrected 2026-09-15: gpu-verify never reads that variable.** It reads `GPU_VERIFY_ADAPTER` (`tools/gpu-verify/src/main.rs:221`), and the `"5060"` default in `tools/gpu-verify/.cargo/config.toml` applies ONLY when launched through cargo — cargo reads config from the CWD upward, not from `--manifest-path`. Set it explicitly. ★ **The old variable is not inert, which is the interesting part.** `VK_LAYER_MESA_device_select` is an implicit *global* Vulkan layer on this box, and **measured 2026-09-15**: `MESA_VK_DEVICE_SELECT=10de:2d04 vulkaninfo --summary` **REORDERS** enumeration (5060 Ti moves GPU1 → GPU0) and does **not filter** — three devices either way. So it cannot satisfy `pick_adapter`, which *counts* non-CPU adapters and refuses with `n > 1` whatever the order. ★★ And the same measurement shows **the RTX 2070 enumerates FIRST by default**, so any tool that simply takes the first suitable adapter is running on the 2070 — see row 82. | ✅ **DONE 2026-09-15 — `scripts/gpu-gate.sh`** (JOURNAL 2026-09-15). It runs gpu-verify on the pinned card and grades it against a declared-failure manifest; four exit codes separate a new regression from a stale declaration from a harness failure. ★ **The item under-counted the known failures: there are TWO, not one.** F5b fails three times (once per target restitution), and **scene D's emergent angle of repose** is a second, unrelated declared failure — 0.1°–0.4° measured against real friction angles of 30–45°. Both are in the manifest with their reasons. |
| 2 | **The pile's step-2 overlap.** Growth is not gradual: step 1 `\|v\| 8.98e-6` (one step of gravity, correct), step 2 `\|v\| 7.32`. `Δv = 6.4 m/s` in one `4.27e-7 s` step ⇒ `acc 1.5e7` ⇒ at `k = 6.8e9` an **overlap of 2.2 mm, twice the touch distance** — despite a release that guarantees none at `t = 0`. | ✅ **ANSWERED + FIXED 2026-09-15 — `docs/46` row 84** (JOURNAL 2026-09-15). The 2.2 mm never needed explaining: it was inferred from a Δv **not reproducible at HEAD** (measured step-2 `|v|` is 1.879e-2, not 7.32), and this row's own two figures disagree — at the `dt` it names, one step of gravity is 4.19e-6, not the 8.98e-6 it calls "exactly one step of gravity". The inference METHOD was sound (damping share 0.0%, spring-only ratio 1.00×); the input was stale. **The real cause:** one question answered by two functions with opposite signs — the release arched every member UPWARD against its own weight while the step bent it down, moving **0.59 m of matter with no velocity** (169% of a member's length). Fixed by making `relax_flex` delegate to `relax_flex_under`. **Verified:** shape/translation 1100× → 1.000×, and `|v|` now grows as `n·g·dt` (ratio 1.000006) — free fall, zero contacts. It also explains row 79's 11th refuted hypothesis: releasing members bent bent them the WRONG WAY. |
| 3 | **Phenology's missing control** (row 58). Wired and gated in code; **never confirmed in a picture.** The Galway June/December pair was confounded by sun elevation (+61% brightness; hue moved 2.6% the *wrong* way). | A **third** render with senescence forced off makes illumination common-mode, and flora pixels are measured specifically rather than a whole ground band. Publish only if the measurement supports it. |
| 4 | **Row 80's three routes** — needs Robin. | Specified, not chosen. |
| 5 | Rows 40, 41, 63 follow-ups — the nine unsourced restitutions, the liquid parcel's viscous dissipation. | Each closed or its blocker recorded. |

---

## 8. What "progress" means here

A session that refutes four hypotheses and lands no feature **has made progress**, and should say so
plainly. A session that lands a feature on an unverified number has not, however good the diff looks.

The engine's value is that its numbers are true. Anything that makes a number less trustworthy —
including a green suite that cannot see what it is measuring — is a regression, whatever else it does.
