#!/usr/bin/env bash
# ONE command to run a rig correctly. Brings up everything a rig needs, in the right order, and
# rebuilds/restarts only what actually changed.
#
#   scripts/rig.sh <rig-file.mjs> [args...]      # screenshots etc.
#   scripts/rig.sh --video <rig-file.mjs> [fps]  # record + measure smoothness
#   scripts/rig.sh --build <rig>.mjs             # force a wasm rebuild
#   scripts/rig.sh --list                        # what rigs exist
#
# Why this exists: the manual sequence (npm run wasm; pkill vite; npx vite; wait; rigshot) was repeated
# by hand every time and has a silent failure mode — **vite computes the wasm cache-busting build stamp
# at STARTUP**, so a server left running from before a rebuild serves the OLD wasm and the rig verifies
# stale code while looking perfectly green. That cost a wrong measurement once already. Here, any wasm
# rebuild FORCES a vite restart, so the trap cannot happen.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
web="$root/web"
PORT="${PORT:-5173}"
VIDEO=0
FORCE_BUILD=0

while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --video) VIDEO=1; shift;;
    --build) FORCE_BUILD=1; shift;;
    --list)  ls "$web/rig"/*.mjs | xargs -n1 basename | grep -v '^_' | column; exit 0;;
    *) echo "unknown flag: $1" >&2; exit 2;;
  esac
done
RIG="${1:?usage: rig.sh [--video|--build] <rig-file.mjs> [args...]}"; shift || true
[[ -f "$web/rig/$RIG" ]] || { echo "no such rig: $RIG (try --list)" >&2; exit 2; }

# 1. GPU-backed X server (idempotent; the software X path cannot composite WebGPU).
bash "$here/start-render-xorg.sh" >/dev/null

# 2. Rebuild wasm only when the Rust core is newer than the artifact — or when asked.
wasm="$web/src/wasm/engine_bg.wasm"
need_build=$FORCE_BUILD
if [[ ! -f "$wasm" ]]; then need_build=1
elif [[ -n "$(find "$root/crates" "$root/shaders" -newer "$wasm" \( -name '*.rs' -o -name '*.wgsl' \) -print -quit 2>/dev/null)" ]]; then
  need_build=1
fi
if (( need_build )); then
  echo "rig: rust/shaders changed -> rebuilding wasm"
  (cd "$web" && npm run wasm >/dev/null 2>&1) || { echo "wasm build FAILED" >&2; exit 1; }
else
  echo "rig: wasm up to date"
fi

# 3. Vite. A rebuild MUST restart it (the build-stamp trap above); otherwise reuse a live server.
serving() { [[ "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/terrain.html" || true)" == "200" ]]; }
if (( need_build )) || ! serving; then
  pkill -f "vite --port $PORT" 2>/dev/null || true
  sleep 1
  (cd "$web" && setsid nohup npx vite --port "$PORT" >/tmp/rig-vite.log 2>&1 &)
  for _ in $(seq 1 40); do serving && break; sleep 0.5; done
  serving || { echo "vite did not come up; see /tmp/rig-vite.log" >&2; exit 1; }
  echo "rig: vite (re)started on :$PORT"
else
  echo "rig: reusing vite on :$PORT"
fi

# 4. Run it.
if (( VIDEO )); then exec bash "$here/rigvideo.sh" "$RIG" "$@"; else exec bash "$here/rigshot.sh" "$RIG" "$@"; fi
