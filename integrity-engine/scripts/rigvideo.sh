#!/usr/bin/env bash
# Record the GPU-composited screen while a rig drives a scene, then measure SMOOTHNESS and CONTINUITY.
# Screenshots cannot see stutter, freezes, popping or teleports — those live in the sequence.
#
# Needs the GPU-backed Xorg first:  scripts/start-render-xorg.sh
# Usage:  scripts/rigvideo.sh <rig-file.mjs> [capture-fps]     e.g.  scripts/rigvideo.sh orbit_watch.mjs
#
# Captures the X framebuffer with ffmpeg x11grab, so it records exactly what the compositor shows —
# the same path `rigshot.sh` screenshots, not an in-browser recording that could differ.
set -euo pipefail
RIG="${1:?usage: rigvideo.sh <rig.mjs> [fps]}"
FPS="${2:-30}"
export DISPLAY="${RENDER_DISPLAY:-:2}"
export MESA_VK_DEVICE_SELECT="${RENDER_VK:-10de:2d04}"
export PORT="${PORT:-5173}"
export OUT="${OUT:-/tmp/rigshot}"
mkdir -p "$OUT"
VID="$OUT/$(basename "$RIG" .mjs).mkv"
here="$(cd "$(dirname "$0")" && pwd)"

# No `exit` inside awk: it closes the pipe early, xdpyinfo takes SIGPIPE, and with `set -o pipefail` the
# whole script dies with 141 before printing anything. It is a race, so it "worked" the first time.
geom=$(DISPLAY="$DISPLAY" xdpyinfo | awk '/dimensions:/{d=$2} END{print d}')
echo "recording $DISPLAY ($geom) at ${FPS} fps -> $VID"
# LOSSLESS (-qp 0). Not a size preference: with lossy encoding a duplicated frame comes back altered by
# up to ~8 levels, so "did anything move?" cannot distinguish a real update from encoder noise, and the
# calibration controls cannot both be satisfied. Costs disk, buys a measurement that means something.
ffmpeg -v error -y -f x11grab -framerate "$FPS" -video_size "$geom" -i "$DISPLAY" \
       -c:v libx264 -qp 0 -preset ultrafast -pix_fmt yuv420p "$VID" &
FF=$!
trap 'kill -INT $FF 2>/dev/null || true' EXIT

cd "$here/../web"
node "rig/$RIG" || true
sleep 1
kill -INT $FF 2>/dev/null || true
wait $FF 2>/dev/null || true
trap - EXIT

echo
python3 "$here/analyze_motion.py" "$VID" --fps "$FPS"
echo
echo "video: $VID   (compare against: scripts/analyze_motion.py --selftest)"
