#!/usr/bin/env bash
# Run a web/rig/*.mjs playwright rig with headless GPU rendering that actually COMPOSITES (rig-verify).
# Needs a GPU-backed Xorg first:  scripts/start-render-xorg.sh
# Usage:  scripts/rigshot.sh <rig-file.mjs> [args...]      e.g.  scripts/rigshot.sh ejecta_blanket.mjs
set -euo pipefail
export DISPLAY="${RENDER_DISPLAY:-:2}"
# Force WebGPU onto the SAME GPU as the Xorg compositor (Mesa device-select layer works for NVIDIA too).
# 10de:2d04 = RTX 5060 Ti on this box; without this, Dawn picks the 2070 and cross-GPU present => DEVICE_LOST.
export MESA_VK_DEVICE_SELECT="${RENDER_VK:-10de:2d04}"
cd "$(dirname "$0")/../web"
exec node "rig/$1" "${@:2}"
