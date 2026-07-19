#!/usr/bin/env bash
# Run the greenfield engine test suite from a fixed entrypoint, so it can be permission-allow-listed
# (`bash greenfield-engine/scripts/test.sh`) without triggering a bash-expansion prompt every run.
#
# Usage:
#   bash scripts/test.sh                # FULL suite — all 136 tests (the deploy gate)
#   bash scripts/test.sh --fast         # fast inner loop — skips the 5 long integration tests
#   bash scripts/test.sh furrow         # only tests matching a filter (passed straight through)
#   bash scripts/test.sh --fast furrow  # both: fast group intersected with a filter
#
# --fast skips ONLY the five long-running numerical-integration tests (each >1s; together they are
# essentially the entire ~24s wall-time). It never weakens or drops any assertion — it is a subset
# for the "edit → test → repeat" loop. ALWAYS run the full suite (no --fast) before deploying.
# The five excluded: the three giant-impact disk-lofting tests (an_oblique_theia / the_birth_scene /
# provenance), the SPH hydrostatic-balance test (sph_air_field), and the dropped-moon impact test.
#
# Prefers cargo-nextest when installed (parallel execution + per-test timing); falls back to
# `cargo test` otherwise. nextest does not run doctests; this crate currently has none, so there is
# no separate --doc step. Tests build at opt-level = 3 (see [profile.test] in the workspace
# Cargo.toml) — the suite is runtime-bound, so that is a ~8x wall-time win.
#
# Prints just the result/error lines; the full output is saved to /tmp/gf-test.log.
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# The five long-running integration tests. Kept in one place, in both the nextest filterset syntax
# and the libtest --skip form, so --fast means the same thing on either runner.
SLOW_FILTER='test(theia) | test(birth_scene) | test(provenance) | test(sph_air_field) | test(dropped_moon_impact)'
SLOW_SKIPS=(--skip theia --skip birth_scene --skip provenance --skip sph_air_field --skip dropped_moon_impact)

fast=0
args=()
for a in "$@"; do
  if [[ "$a" == "--fast" ]]; then fast=1; else args+=("$a"); fi
done

if command -v cargo-nextest >/dev/null 2>&1; then
  if [[ $fast -eq 1 ]]; then
    cargo nextest run -p engine -E "not ($SLOW_FILTER)" "${args[@]}"
  else
    cargo nextest run -p engine "${args[@]}"
  fi
else
  if [[ $fast -eq 1 ]]; then
    cargo test -p engine "${args[@]}" -- "${SLOW_SKIPS[@]}"
  else
    cargo test -p engine "${args[@]}"
  fi
fi 2>&1 | tee /tmp/gf-test.log \
  | grep -E "test result:|Summary|FAIL|error\[|^error:|FAILED|panicked|warning: unused" | tail -40
status="${PIPESTATUS[0]}"
echo "--- test exit ${status} · full log: /tmp/gf-test.log ---"
exit "${status}"
