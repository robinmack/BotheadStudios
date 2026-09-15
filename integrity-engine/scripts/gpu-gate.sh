#!/usr/bin/env bash
# Run the out-of-process GPU verifier and GRADE it against a declared-failure manifest.
#
# WHY THIS EXISTS. `tools/gpu-verify` runs the real `shaders/particle_step.wgsl` on real hardware —
# it is the only thing that checks the GPU granular path's PHYSICS. Nothing ran it: `docs/72` queue
# item 1 records `grep gpu-verify scripts/` returning 0 hits, so its verdict reached a human only when
# someone remembered to type the command. This is that runner.
#
# The problem a plain runner does not solve: gpu-verify exits 1 today, and will keep exiting 1 for as
# long as docs/46 row 80 is open. A gate that is permanently red is a gate nobody reads, and a NEW
# failure hiding behind a KNOWN one is invisible. So this script does not ask "did it pass" — it asks
# **"did it fail in exactly the ways we have already declared, and in no other way"**.
#
# Usage:
#   bash scripts/gpu-gate.sh             # build, run on the 5060 Ti, grade
#   bash scripts/gpu-gate.sh --selftest  # grade crafted fixtures — verifies THE GRADER, not the GPU
#   bash scripts/gpu-gate.sh --grade F   # grade an existing log file F (no GPU needed)
#
# Exit codes — distinct on purpose, because "it went red" is not one fact:
# WHERE IT RUNS: **locally only, on a box with the card.** Not in CI and NOT inside scripts/test.sh —
# `.github/workflows/ci.yml` runs `bash integrity-engine/scripts/test.sh` on `ubuntu-latest` with no
# GPU and no self-hosted runner, so wiring this into test.sh would put it in CI, where it could only
# ever fail for want of hardware. Measured runtime: 65.7 s wall on the RTX 5060 Ti (2026-09-15).
#
#   0  every undeclared check passed, and every declared failure is still failing. Expected steady state.
#   1  an UNDECLARED check FAILED. This is a new regression. The thing this gate exists to catch.
#   2  a DECLARED failure now PASSES. Not a celebration — the manifest is stale and a human must decide
#      what it means (row 80's fix is a modelling choice that `docs/72` §2 reserves for Robin).
#   3  the harness itself failed: no GPU, build error, adapter panic, or a log with no checks in it.
#      ★ Parsing nothing must NEVER be graded as success — `CLAUDE.md` rule 3 and `AGENTS.md` §2 both
#      record a gate here that printed its failure and exited 0, which is worse than no gate at all.
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ── THE DECLARED-FAILURE MANIFEST ────────────────────────────────────────────────────────────────
# A failure may appear here ONLY with a written reason that names where the defect is analysed. This
# is the difference between a declared specialisation and a quiet approximation (Law V): an entry is
# an IOU with an address, not permission to be broken.
#
#   <check-id>|<expected failing lines>|<reason, citing its doc/row>
#
# `expected failing lines` is how many FAIL lines that check emits when it is failing as declared. It
# is recorded so that a PARTIAL change (3 of 3 targets failing becomes 1 of 3) is reported rather than
# silently absorbed — that number moving is a physics change even when the verdict does not move.
DECLARED=(
  "F5b|3|docs/46 row 80 — the GPU's implicit solve realises a DIFFERENT restitution than the native path from the same catalogued number. Confirmed by this tool (commit 17eddbf) after its first calibration proved circular. Row 80 records THREE non-equivalent fix routes; choosing between them is a modelling decision reserved for Robin (docs/72 §2), so this stays declared, not fixed."
  "D|1|docs/45 §rolling-resistance (lines ~129-134, ~186) — emergent angle of repose under-predicts badly (measured 0.1-0.4 deg against real friction angles of 30-45 deg). Grains are continuum PARCELS and roll like marbles; the fix is rolling resistance / parcel interlocking, NOT a tuned mu. docs/45 explicitly forbids 'fixing' the acceptance criterion instead of the model."
)

# ── GRADING ──────────────────────────────────────────────────────────────────────────────────────
# Attribution rule: gpu-verify prints a check's headline at column 0 and its detail indented beneath,
# and the PASS/FAIL token can sit on either. So a line starting at column 0 sets the current check id
# (its first token); an indented line inherits it. `adapter:` and blank lines are skipped explicitly —
# the tool reprints the adapter mid-run (deliberately, so hardware provenance stays in the output),
# and without this skip scene O's indented results would be attributed to "adapter".
grade() {
  local log="$1"
  [[ -r "$log" ]] || { echo "GRADE: cannot read log '$log'" >&2; return 3; }

  awk -v declared="$(printf '%s\n' ${DECLARED[@]+"${DECLARED[@]}"})" '
    BEGIN {
      n = split(declared, D, "\n")
      for (i = 1; i <= n; i++) { split(D[i], f, "|"); exp_fail[f[1]] = f[2]; reason[f[1]] = f[3] }
      cur = "?"
    }
    # A line at column 0 names the check the following indented lines belong to.
    /^[^ \t]/ {
      tok = $1
      sub(/:$/, "", tok)
      if (tok != "adapter" && tok != "" && $0 !~ /^#/) cur = tok
    }
    /(^| )PASS( |$)/ { passes[cur]++; seen[cur] = 1; total_pass++ }
    /(^| )FAIL( |$)/ { fails[cur]++;  seen[cur] = 1; total_fail++; line[cur] = line[cur] "\n      " $0 }
    END {
      if (total_pass + total_fail == 0) {
        print "HARNESS: the log contains no PASS/FAIL lines at all — nothing was graded."
        print "         An empty parse is NOT a pass. Check the build and the adapter."
        exit 3
      }
      rc = 0
      print "--- declared failures (expected to be failing) ---"
      for (k in exp_fail) {
        if (!(k in seen)) {
          printf "  ?? %-5s DECLARED BUT NOT FOUND in the log — has the check been renamed or removed?\n", k
          printf "        %s\n", reason[k]
          rc = (rc == 0 ? 3 : rc)
        } else if (fails[k] + 0 == 0) {
          printf "  XP %-5s NOW PASSES — the declaration is STALE. A human must decide what changed.\n", k
          printf "        was: %s\n", reason[k]
          rc = 2
        } else {
          printf "  ok %-5s still failing as declared (%d/%d FAIL lines)\n", k, fails[k], exp_fail[k]
          if (fails[k] + 0 != exp_fail[k] + 0)
            printf "  !! %-5s NOTICE: failing-line count MOVED %d -> %d. Verdict unchanged, but the physics\n        behind it did. Re-read the row before assuming this is noise.\n", k, exp_fail[k], fails[k]
        }
      }
      print ""
      print "--- undeclared failures (each one is a regression) ---"
      undeclared = 0
      for (k in fails) {
        if (!(k in exp_fail)) {
          undeclared++
          printf "  XX %-5s FAILED and is not declared:%s\n", k, line[k]
          rc = 1
        }
      }
      if (undeclared == 0) print "  none."
      nseen = 0; for (k in seen) nseen++          # length(array) is a gawk extension; mawk lacks it
      printf "\n--- %d check ids graded: %d PASS lines, %d FAIL lines ---\n",
             nseen, total_pass, total_fail
      exit rc
    }
  ' "$log"
}

# ── SELF-TEST ────────────────────────────────────────────────────────────────────────────────────
# `AGENTS.md` §2: "Verify a new gate by making it fail." Doing that once by hand verifies the run you
# did it on; these fixtures verify the GRADER, every time anyone runs it, with no GPU required.
# Fixture 4 is the negative control: the REAL baseline log graded against an EMPTY manifest must go
# red. Without it, every fixture is one I wrote to match the parser I wrote.
selftest() {
  local tmp rc bad=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN

  cat > "$tmp/steady.log" <<'EOF'
F5b catalogued restitution through the GPU: asked e 0.200 -> got 0.000  FAIL
F5b catalogued restitution through the GPU: asked e 0.400 -> got 0.291  FAIL
F5b catalogued restitution through the GPU: asked e 0.600 -> got 0.497  FAIL
F6 friction (mu=0.6, vacuum): decel 5.89 vs 5.89  PASS
D emergent repose vs REAL material friction:
   -> settles: true, plausible repose (12-42 deg): false  FAIL
K terrain non-injecting + supportive: rebound 0.50  PASS
EOF

  sed 's/^K terrain non-injecting + supportive: rebound 0.50  PASS/K terrain non-injecting + supportive: rebound 9.99  FAIL/' \
    "$tmp/steady.log" > "$tmp/regression.log"

  sed 's/^F5b \(.*\)FAIL$/F5b \1PASS/' "$tmp/steady.log" > "$tmp/xpass.log"

  printf 'building...\nadapter: NVIDIA GeForce RTX 5060 Ti\n' > "$tmp/empty.log"

  # A declared check that VANISHES (renamed, or deleted outright) must not read as green. Without
  # this branch, deleting the inconvenient check is the easiest way to make the gate pass.
  grep -v '^F5b' "$tmp/steady.log" > "$tmp/vanished.log"

  check() { # name expected_rc logfile
    grade "$3" >/dev/null 2>&1; rc=$?
    if [[ $rc -eq $2 ]]; then printf '  ok   %-28s exit %d\n' "$1" "$rc"
    else printf '  FAIL %-28s exit %d, wanted %d\n' "$1" "$rc" "$2"; bad=1; fi
  }

  echo "--- grader self-test (no GPU required) ---"
  check "steady state -> 0"      0 "$tmp/steady.log"
  check "new regression -> 1"    1 "$tmp/regression.log"
  check "declared now passes ->2" 2 "$tmp/xpass.log"
  check "nothing parsed -> 3"    3 "$tmp/empty.log"
  check "declared check gone -> 3" 3 "$tmp/vanished.log"

  # Negative control on REAL data: the true baseline, graded with the manifest emptied, must go red.
  if [[ -r /tmp/gpu-verify-baseline.log ]]; then
    ( DECLARED=(); grade /tmp/gpu-verify-baseline.log >/dev/null 2>&1
      [[ $? -eq 1 ]] ) \
      && printf '  ok   %-28s exit 1\n' "real log, empty manifest" \
      || { printf '  FAIL %-28s did not go red\n' "real log, empty manifest"; bad=1; }
  else
    printf '  --   %-28s skipped (no /tmp/gpu-verify-baseline.log)\n' "real log, empty manifest"
  fi

  [[ $bad -eq 0 ]] && echo "grader OK" || echo "GRADER IS BROKEN — do not trust its verdicts" >&2
  return $bad
}

# ── MAIN ─────────────────────────────────────────────────────────────────────────────────────────
case "${1:-}" in
  --selftest) selftest; exit $? ;;
  --grade)    grade "${2:?--grade needs a log file}"; exit $? ;;
esac

LOG=/tmp/gpu-gate.log
# GPU_VERIFY_ADAPTER is supplied by tools/gpu-verify/.cargo/config.toml (value "5060", force=false),
# but ONLY when the binary is launched through cargo. Set it explicitly too: this gate must not depend
# on which directory cargo was invoked from, and a silently-wrong card is the exact bug pick_adapter
# was written to kill (tools/gpu-verify/src/main.rs:194-204).
# TIMEOUT, from a measurement rather than a round number: the full run took **65.7 s** wall on the
# 5060 Ti on 2026-09-15 (warm build). 600 s is ~9x that. It is not decoration — this tool has a hang
# history: docs/46 row 80 records an unbounded bisection that had to be killed at 600 s, and an
# unattended gate that hangs forever is a gate that never reports. A timeout is a HARNESS failure
# (exit 3), never a pass and never a physics failure.
TIMEOUT_S="${GPU_GATE_TIMEOUT_S:-600}"
echo "--- gpu-verify on the declared card (GPU_VERIFY_ADAPTER=${GPU_VERIFY_ADAPTER:-5060}, timeout ${TIMEOUT_S}s) ---"
GPU_VERIFY_ADAPTER="${GPU_VERIFY_ADAPTER:-5060}" \
  timeout "${TIMEOUT_S}" cargo run --release --quiet --manifest-path tools/gpu-verify/Cargo.toml > "$LOG" 2>&1
run_rc=$?
if [[ $run_rc -eq 124 ]]; then
  echo "--- gpu-verify TIMED OUT after ${TIMEOUT_S}s — harness failure, not a verdict. log: $LOG ---" >&2
  tail -20 "$LOG" >&2
  exit 3
fi

# The tool exits 1 when any scene fails, which is the expected steady state here, so its exit code is
# NOT the verdict — the grader is. But a build failure or an adapter panic is a harness failure, and
# those are distinguishable: they produce no PASS/FAIL lines, which the grader returns 3 for.
grep -m1 '^adapter:' "$LOG" || echo "adapter: (not reported — suspect a build or device failure)"
if [[ $run_rc -ne 0 && $run_rc -ne 1 ]]; then
  echo "--- gpu-verify exited $run_rc (neither 0 nor 1) — harness failure, log: $LOG ---" >&2
  tail -20 "$LOG" >&2
  exit 3
fi

grade "$LOG"
rc=$?
case $rc in
  0) echo "--- GATE GREEN: no undeclared failure. Declared failures still failing. log: $LOG ---" ;;
  1) echo "--- GATE RED: an UNDECLARED check failed — a new regression. log: $LOG ---" >&2 ;;
  2) echo "--- GATE RED: a DECLARED failure now passes. Update the manifest deliberately. log: $LOG ---" >&2 ;;
  3) echo "--- GATE RED: harness failure — nothing was graded. log: $LOG ---" >&2 ;;
esac
exit $rc
