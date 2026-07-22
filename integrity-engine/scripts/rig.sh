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
RESTART=0

# The dev server's kill lives HERE and nowhere else, on purpose. Typing
# `pkill -f "vite --port 5173"` at a prompt matches the very shell running it — the pattern is on that
# shell's own command line — so it kills the caller. That cost several confusing exit-144s. Keeping it
# behind `--restart`/`--stop` means the pattern is never hand-typed, and the bracket makes it unable to
# match its own literal text even here.
kill_server() { pkill -f "[v]ite .*--port $PORT" 2>/dev/null || true; sleep 1; }

while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --video) VIDEO=1; shift;;
    --build) FORCE_BUILD=1; shift;;
    --list)  ls "$web/rig"/*.mjs | xargs -n1 basename | grep -v '^_' | column; exit 0;;
    --restart) RESTART=1; shift;;
    --stop)  kill_server; echo "rig: dev server stopped"; exit 0;;
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
serving() { [[ "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/birth.html" || true)" == "200" ]]; }

# ...but "did WE rebuild?" is the wrong question, and asking it cost another wrong measurement. Anyone
# can run `npm run wasm` by hand; this script then says "wasm up to date", reuses a server that started
# BEFORE those bytes existed, and the rig verifies stale code while reporting green. Twice now the fix
# was a hand-typed pkill, which is exactly the unreliable ritual this script exists to replace.
#
# So test the INVARIANT instead of the history: the running server must be NEWER than the wasm it is
# serving. That holds no matter who built what, or when.
stamp="/tmp/rig-vite-started-$PORT"
stale_server=0
if serving; then
  if [[ ! -f "$stamp" ]] || [[ "$wasm" -nt "$stamp" ]]; then
    stale_server=1
    echo "rig: server predates the current wasm -> restarting (stale-serve guard)"
  fi
fi

if (( need_build )) || (( RESTART )) || (( stale_server )) || ! serving; then
  # NOTE the bracket in "[v]ite": `pkill -f "vite --port $PORT"` matches ANY process whose command line
  # contains that string — including the very shell running this script when it was invoked with the
  # pattern on its own command line. That self-match killed the caller (exit 144) repeatedly before it
  # was spotted. The bracket makes the pattern not match its own literal text.
  kill_server
  # Start FULLY detached: own session, all three descriptors redirected. Inheriting the script's stdout
  # is not merely untidy — if the caller pipes this script (`rig.sh foo | tail`), the long-lived server
  # holds the pipe's write end open and `tail` never sees EOF, so the whole command hangs long after the
  # rig finished. That is what made this script look broken.
  # The local binary with the root passed POSITIONALLY (vite 6 rejects `--root`). `npx --prefix`
  # resolves the package but leaves the working directory, so vite answered 404 for every page while
  # cheerfully logging "ready" — an explicit root removes the dependence on the caller's cwd.
  setsid "$web/node_modules/.bin/vite" "$web" --port "$PORT" >/tmp/rig-vite.log 2>&1 </dev/null &
  disown || true
  for _ in $(seq 1 60); do serving && break; sleep 0.5; done
  serving || { echo "vite did not come up; see /tmp/rig-vite.log" >&2; tail -5 /tmp/rig-vite.log >&2; exit 1; }
  # Record WHEN this server started, so the guard above can compare it against the wasm next time.
  touch "$stamp"
  echo "rig: vite (re)started on :$PORT"
else
  echo "rig: reusing vite on :$PORT"
fi

# 4. Run it.
if (( VIDEO )); then exec bash "$here/rigvideo.sh" "$RIG" "$@"; else exec bash "$here/rigshot.sh" "$RIG" "$@"; fi
