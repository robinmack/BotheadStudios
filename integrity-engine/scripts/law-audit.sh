#!/usr/bin/env bash
#
# law-audit.sh — audit an AREA of the engine against the Laws, for a burn-down.
#
#   scripts/law-audit.sh "crates/engine/src/ground_scene.rs" "the ground scene"
#   scripts/law-audit.sh "shaders/*.wgsl" "the shaders"
#
# The sibling of law-review.sh. That one reviews a CHANGE, to stop a new defect being created; this one
# audits WHAT IS ALREADY THERE, to find the ones already created. Same rule about output: every finding
# must come with the test that would catch it, because the test is what survives the session.
#
# OPTIONAL AND ADVISORY, exactly like law-review.sh: it needs a logged-in `claude` CLI, costs tokens, and
# returns judgement rather than a verdict. It skips cleanly for anyone without it. `scripts/test.sh` is
# the suite that must pass, and it is fully deterministic and local.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
cd "$root"

if ! command -v claude >/dev/null 2>&1; then
  echo "law-audit: SKIPPED — the \`claude\` CLI is not installed (this check is advisory)."
  exit 0
fi

TARGET="${1:?usage: law-audit.sh <paths> <description>}"
WHAT="${2:-$TARGET}"

read -r -d '' PROMPT <<EOF || true
Audit part of the Integrity engine against its own Laws, for a burn-down list.

READ FIRST: docs/00-laws-of-integrity.md (the Laws in full) and the top of CLAUDE.md.
THEN AUDIT: $TARGET — $WHAT

The engine's premise is that ONE set of mechanics applies at every scale: Theia striking proto-Earth and
a raindrop striking a petal are the same mechanic, differing in energy and in whether matter must be
resolved into particles — never in the rules or the code path.

Hunt for these, in this order of severity:

 1. THE SAME MECHANIC IMPLEMENTED TWICE — a general rule that exists in the engine but which this code
    reimplements or bypasses. (Known live example: accretion::representation decides surface-vs-particles
    for any matter at any scale, while ground_scene's meteor goes through matter::impact instead.)
 2. A SCENE DECLARING PHYSICS the engine should derive — a radius, a gravity, a resolution, a particle
    count, a softening length. Scenes may declare WHICH bodies, WHERE they are, and HOW FAST. Nothing else.
 3. A NUMBER THAT CAME FROM NOWHERE — chosen because it looked right, rather than measured, cited or
    derived. Distinguish this from a DECLARED measurement (fine, if sourced) and a flagged IOU (fine, if
    flagged).
 4. A CHEAP STAND-IN living beside an honest model that already exists in the codebase.
 5. RENDER DRIVING PHYSICS rather than the reverse, or a representation choice made by a dial rather
    than by a measurement.

For EACH finding output exactly:

  ID:       a short slug
  LAW:      which Law, one line on how this breaks it
  SEVERITY: HIGH (physics is wrong or two answers exist) / MEDIUM (honest but duplicated or undeclared) /
            LOW (cosmetic, or already flagged in a comment)
  EVIDENCE: file:line, quoting the code
  FIX:      what it should do instead, concretely
  TEST:     the assertion that would FAIL now and pass after. If it is not mechanically testable, say so
            plainly rather than inventing a weak test.

Rank by severity. Be sceptical and specific; do not praise, and skip style, naming and performance
entirely. If something looks wrong but you are not sure, say so and say what would settle it.
Read the code before judging it — do not guess from names.
EOF

echo "law-audit: auditing $WHAT …" >&2
printf '%s\n' "$PROMPT" | claude -p --permission-mode plan 2>/dev/null \
  || { echo "law-audit: SKIPPED — the reviewer could not be reached (advisory only)."; exit 0; }
