#!/usr/bin/env bash
# **Encode an INTERNALLY RENDERED frame sequence into a reviewable movie.**
#
# `scripts/rigvideo.sh` records the X framebuffer with `ffmpeg -f x11grab` — it films a BROWSER, which
# is right for a rig and impossible for anything the engine draws to an offscreen texture. A native
# render (`renderer::Target::Offscreen` + `Terra::frame_pixels`, docs/69 §3) never touches a screen, so
# there is nothing to grab; it produces numbered PNGs instead. This turns those into the same kind of
# artefact for the same reason.
#
# Robin (2026-09-20): *"we should probably also be able to record movies to the gallery as well so we
# can inspect/review"*. `CLAUDE.md` rule 4b is why: **motion is a property of the SEQUENCE, not of any
# frame.** A still cannot show a stutter, a freeze, popping or a teleport — and a teleport is exactly
# what `docs/46` row 84 turned out to be. A gallery of stills can only carry half the evidence.
#
#   scripts/framevideo.sh <frame-dir> [out.mp4] [fps]
#   scripts/framevideo.sh /tmp/rigshot/haystack              # -> /tmp/rigshot/haystack.mp4 at 20 fps
#
# ★ LOSSLESS, and that is not a preference. `rigvideo.sh`'s header records why it encodes at `-qp 0`:
# a lossy encode moves flat regions by up to ~8 levels, so "did anything move?" cannot distinguish a
# real update from encoder noise — which makes the recording useless for the one question it exists to
# answer. Same setting here, and for the same reason.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

DIR="${1:?usage: framevideo.sh <frame-dir> [out.mp4] [fps]}"
OUT="${2:-${DIR%/}.mp4}"
FPS="${3:-20}"

[ -d "$DIR" ] || { echo "no such frame directory: $DIR" >&2; exit 1; }

# The pattern is discovered, not assumed: a renderer that changed its naming would otherwise produce an
# empty movie and a zero exit, which is the "gate that reports success on nothing" trap (CLAUDE.md
# rule 3). Refuse instead.
first="$(find "$DIR" -maxdepth 1 -name '*.png' | sort | head -1)"
[ -n "$first" ] || { echo "no PNG frames in $DIR — nothing to encode" >&2; exit 1; }
stem="$(basename "$first")"
prefix="${stem%%-[0-9]*}"
count="$(find "$DIR" -maxdepth 1 -name "${prefix}-*.png" | wc -l)"
[ "$count" -ge 2 ] || { echo "only $count frame(s) in $DIR — a movie needs a sequence" >&2; exit 1; }

# `-pix_fmt yuv420p` needs even dimensions; pad rather than silently rescale, because a rescale would
# resample every pixel and this recording is supposed to be exactly what was drawn.
ffmpeg -v error -y -framerate "$FPS" \
       -i "$DIR/${prefix}-%04d.png" \
       -vf "pad=ceil(iw/2)*2:ceil(ih/2)*2" \
       -c:v libx264 -qp 0 -preset ultrafast -pix_fmt yuv420p "$OUT"

bytes="$(stat -c%s "$OUT")"
echo "✓ $count frames @ ${FPS} fps -> $OUT ($((bytes / 1024)) KiB, lossless -qp 0)"
echo "  publish with: RIGSHOT_DIR=$(dirname "$OUT") scripts/publish-shots.sh"
