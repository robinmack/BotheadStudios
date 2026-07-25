#!/usr/bin/env bash
# Run the greenfield engine test suite from a fixed entrypoint, so it can be permission-allow-listed
# (`bash integrity-engine/scripts/test.sh`) without triggering a bash-expansion prompt every run.
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
# provenance), the SPH hydrostatic-balance test (sph_air_field), and the dropped-moon impact test
# (the SPH-side pin in gpu_sph, ~9s; it replaced the CPU Aggregate one when that path retired).
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

# Expanding an EMPTY array with "${args[@]}" is an unbound-variable error under `set -u` on bash 3.2
# (macOS's /bin/bash), so a bare `bash scripts/test.sh` died before running anything. The
# ${args[@]+"${args[@]}"} form expands to nothing when the array is empty and to the quoted elements
# otherwise, on every bash. Used at all four call sites below.

if command -v cargo-nextest >/dev/null 2>&1; then
  if [[ $fast -eq 1 ]]; then
    cargo nextest run -p engine -E "not ($SLOW_FILTER)" ${args[@]+"${args[@]}"}
  else
    cargo nextest run -p engine ${args[@]+"${args[@]}"}
  fi
else
  if [[ $fast -eq 1 ]]; then
    cargo test -p engine ${args[@]+"${args[@]}"} -- "${SLOW_SKIPS[@]}"
  else
    cargo test -p engine ${args[@]+"${args[@]}"}
  fi
fi 2>&1 | tee /tmp/gf-test.log \
  | grep -E "test result:|Summary|FAIL|error\[|^error:|FAILED|panicked|warning: unused" | tail -40
status="${PIPESTATUS[0]}"
echo "--- test exit ${status} · full log: /tmp/gf-test.log ---"

# **`mod app` IS NOT COMPILED BY ANY OF THE ABOVE.** The scene structs (`Terra`, `OrbitDemo`, `Ground`)
# live behind `#[cfg(target_arch = "wasm32")]`, so a native `cargo check --all-targets` is GREEN for code
# that does not build — CLAUDE.md rule 3 has warned about this in prose for months and it caught us again
# on 2026-07-25: Sean's one-Earth step removed `EARTH_RADIUS_M`, three readers survived inside `mod app`,
# native check passed, and only the wasm target found them. Prose is not a gate; this is.
#
# `cargo check` (not `build`) against wasm32 is the cheap form — it type-checks every scene without
# emitting or running wasm-bindgen, so it costs seconds after the first run. Skipped under --fast to keep
# the edit→test loop tight; the full run IS the deploy gate, so it never ships unchecked.
if [[ $status -eq 0 && $fast -eq 0 ]]; then
  echo "--- compiling mod app (wasm32) — the scenes a native check cannot see ---"
  # Run it BARE and read $? directly. The first version piped straight into `grep` inside an `if !`,
  # which reads the status of the *grep*, not of cargo — so it printed the compiler errors and then
  # reported success. A gate that prints a failure and exits 0 is worse than no gate: it teaches you
  # to trust it. Caught only by deliberately breaking `mod app` and checking the run went red.
  cargo check --quiet --lib --target wasm32-unknown-unknown \
    --manifest-path crates/engine/Cargo.toml > /tmp/gf-wasm.log 2>&1
  wstatus=$?
  if [[ $wstatus -ne 0 ]]; then
    grep -E "^error" -A 6 /tmp/gf-wasm.log | head -40 >&2
    echo "--- mod app FAILED to compile for wasm32 (exit ${wstatus}) · log: /tmp/gf-wasm.log ---" >&2
    status="$wstatus"
  else
    echo "--- mod app compiles ---"
  fi
fi

exit "${status}"
