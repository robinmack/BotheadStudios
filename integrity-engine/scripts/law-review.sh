#!/usr/bin/env bash
#
# law-review.sh — ask a Claude agent whether a change applies the engine's rules CONSISTENTLY.
#
#   scripts/law-review.sh                 # review the working tree against main
#   scripts/law-review.sh main..HEAD      # review a branch
#   scripts/law-review.sh --staged        # review what is about to be committed
#
# WHY THIS EXISTS, and what it is NOT.
#
# `laws.rs` catches the Law violations a machine can count: a world file declaring a quantity that must
# emerge, a physical constant living in two files. Counting is exact and it never argues. But the
# violations that have actually cost the most here were not countable — they were the SAME MECHANIC
# IMPLEMENTED TWICE. A general collision rule applied in one scene while another kept its own path. Two
# containers for one question. A cheap stand-in beside the honest model. No grep finds those, because
# nothing is repeated; the second implementation looks like ordinary new code.
#
# That needs judgement, which is what an agent can bring. But an agent review is non-deterministic and
# can be argued with, so its VERDICT is not the enforcement. Its job is to produce the thing that IS:
# **for every finding it must propose a concrete test**. The review is the search; the test is the guard.
# A finding without a test is a conversation, and conversations do not survive the session.
#
# **OPTIONAL, AND ADVISORY.** This is NOT part of the test suite and must never be. It needs a logged-in
# `claude` CLI, it costs tokens, and it returns judgement rather than a verdict — none of which belongs in
# a gate that has to pass for everyone. Contributors without Claude (or without a login, or offline) get a
# clean skip and exit 0; `scripts/test.sh` is the suite, and it stays fully deterministic and local.
#
# Use it when ADDING a mechanic, which is when the "implemented twice" defect is created. Then convert
# whatever it finds into tests in `laws.rs` or beside the code, and those tests are what protect everyone
# afterwards — including the contributors who cannot run this.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
cd "$root"

# Degrade cleanly for anyone without the reviewer, rather than failing their build.
if ! command -v claude >/dev/null 2>&1; then
  echo "law-review: SKIPPED — the \`claude\` CLI is not installed."
  echo "            This check is advisory and optional; scripts/test.sh is the suite that must pass."
  exit 0
fi

case "${1:-}" in
  --staged) diff="$(git diff --staged)"; what="the staged change" ;;
  "")       diff="$(git diff main...HEAD; git diff)"; what="this branch and working tree, against main" ;;
  *)        diff="$(git diff "$1")"; what="$1" ;;
esac

if [[ -z "${diff// }" ]]; then
  echo "law-review: nothing to review"; exit 0
fi

# Keep the prompt bounded — a very large diff costs more than it informs, and the reviewer should be
# pointed at what changed, not handed the repository.
lines=$(printf '%s\n' "$diff" | wc -l)
if (( lines > 4000 )); then
  echo "law-review: diff is $lines lines; reviewing the first 4000 (narrow the range for a closer look)" >&2
  diff="$(printf '%s\n' "$diff" | head -4000)"
fi

read -r -d '' PROMPT <<'EOF' || true
You are reviewing a change to the Integrity engine, a physics engine whose entire premise is that ONE
set of mechanics applies at every scale. Its Laws are in docs/00-laws-of-integrity.md and at the top of
CLAUDE.md — read them first.

Your job is NOT general code review. Look for exactly one class of defect: **a mechanic implemented
twice, or a general rule applied in only one place.**

The failures this engine has actually suffered, so you know the shape:
  * a general collision/representation rule written, then applied in ONE scene while another scene kept
    its own bespoke impact path
  * two containers for one physical question, so the same situation had two answers
  * a cheap stand-in (a Fresnel rim "atmosphere") living beside the honest model (Rayleigh scattering)
    that was already in the codebase
  * a physical constant typed into a second file instead of read from its definition
  * a scene declaring physics (a radius, a resolution, a gravity) that the engine should derive
  * a number chosen because it looked right, rather than derived from something measurable

For EACH finding, output exactly:

  LAW:      which Law, and one line on how this breaks it
  EVIDENCE: file:line and what is duplicated or scene-specific — be specific, quote the code
  TEST:     a concrete test that would FAIL on this and pass once fixed. Name the assertion. If the
            defect is not mechanically testable, say so plainly rather than inventing a weak test.

If the change is consistent, say "CONSISTENT" and name the general rule it correctly reuses.

Be sceptical and concrete. Do not praise. Do not list style issues, naming, or performance. A finding
you cannot write a test for is worth less than one you can, so prefer the testable.
EOF

echo "law-review: reviewing $what ($lines lines of diff)…" >&2
printf '%s\n\n=== DIFF ===\n%s\n' "$PROMPT" "$diff" \
  | claude -p --permission-mode plan 2>/dev/null \
  || {
        # Not logged in, offline, rate-limited — all the same to a contributor who just wants to build.
        echo "law-review: SKIPPED — the reviewer could not be reached (not logged in, offline, or rate-limited)."
        echo "            This check is advisory; nothing here gates the build."
        exit 0
     }
