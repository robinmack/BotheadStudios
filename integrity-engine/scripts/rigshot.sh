#!/usr/bin/env bash
# Run a web/rig/*.mjs playwright rig with headless GPU rendering that actually COMPOSITES (rig-verify).
# Needs a GPU-backed Xorg first:  scripts/start-render-xorg.sh
# Usage:  scripts/rigshot.sh <rig-file.mjs> [args...]      e.g.  scripts/rigshot.sh ejecta_blanket.mjs
# Screenshots land in $OUT (default /tmp/rigshot); serve the build first with `npx vite --port 5173` in web/.
# NOTE: restart vite after `npm run wasm` — the wasm URL is cache-busted with the build stamp vite computes
# at STARTUP (web/src/*.ts), so a server left running from before the rebuild serves the OLD wasm and the
# rig silently verifies stale code. Check the `build` stamp in the HUD matches the run.
set -euo pipefail
export DISPLAY="${RENDER_DISPLAY:-:2}"
# The dev-server port and screenshot directory every rig reads. Rigs used to hardcode whichever port that
# session's server happened to use (13 different dead ones) and write into a previous session's scratchpad
# directory, so most of the fleet was unrunnable as written. One default here, `PORT=` to override.
export PORT="${PORT:-5173}"
export OUT="${OUT:-/tmp/rigshot}"
mkdir -p "$OUT"
# Force WebGPU onto the SAME GPU as the Xorg compositor (Mesa device-select layer works for NVIDIA too).
# 10de:2d04 = RTX 5060 Ti on this box; without this, Dawn picks the 2070 and cross-GPU present => DEVICE_LOST.
export MESA_VK_DEVICE_SELECT="${RENDER_VK:-10de:2d04}"
cd "$(dirname "$0")/../web"
exec node "rig/$1" "${@:2}"
