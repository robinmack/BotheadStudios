#!/usr/bin/env bash
# Start a headless Xorg on an NVIDIA GPU so headless WebGPU renders can COMPOSITE into screenshots
# (rig verification — the Laws require rig-watching every visual claim). Without a real GPU-backed X
# server, a software display (xvfb) cannot read back the GPU swapchain and screenshots come out blank.
#
# THE OTHER HALF is device matching: force WebGPU onto the SAME GPU as this Xorg compositor, or presenting
# across two GPUs throws VK_ERROR_DEVICE_LOST. `scripts/rigshot.sh` sets MESA_VK_DEVICE_SELECT for that.
#
# Machine-specific defaults are for THIS workstation (RTX 5060 Ti @ PCI:2:0:0). Override via env.
# Idempotent: safe to run every session; a no-op if the display already responds.
set -euo pipefail
DISP="${RENDER_DISPLAY:-:2}"
BUSID="${RENDER_BUSID:-PCI:2:0:0}"   # 5060 Ti. 2070 is PCI:4:0:0 (its Xorg 'no screens' on this box).
N="${DISP#:}"
# ★ THE SCREENSHOT CEILING. A rig cannot capture more pixels than this server HAS, so this number is the
# upper bound on every visual claim the project can make — and it sat at 1280x800 while the gallery filled
# with 560x400 frames nobody could judge a texture from (Robin, 2026-08-05: *"your screen shots are pretty
# low res… I do want us to be able to verify high quality textures"*, and *"2K seems reasonable as a
# ceiling/quality test on the 5060"*). It is a framebuffer allocation and nothing else — 2560x1440x4 B =
# 15 MB of a 16 GB card — so it is not a performance setting; the render cost is the rig's VIEWPORT.
# Sized a little TALLER than the 2560x1600 rig viewport (`web/rig/_launch.mjs`) so Chromium's own tab
# strip and address bar have somewhere to live: a window that does not fit its screen is clipped, and a
# clipped window is a screenshot missing its bottom.
VW="${RENDER_W:-2560}"
VH="${RENDER_H:-1800}"

# Already up AND responding to GLX? Done — UNLESS it is the wrong size. A server started by an earlier
# session at a smaller Virtual is a silent cap on every screenshot taken after it, and the idempotent
# check used to hand that cap to the next session without a word. Size mismatch ⇒ restart.
if DISPLAY="$DISP" glxinfo >/dev/null 2>&1; then
    HAVE="$(DISPLAY="$DISP" xdpyinfo 2>/dev/null | awk '/dimensions:/{print $2}')"
    if [ "$HAVE" = "${VW}x${VH}" ]; then
        echo "Xorg ${DISP} already up at ${HAVE}: $(DISPLAY="$DISP" glxinfo 2>/dev/null | grep -i 'OpenGL renderer' | head -1)"
        exit 0
    fi
    echo "Xorg ${DISP} is ${HAVE}, want ${VW}x${VH} — restarting it."
    # `pgrep -x Xorg` matches the EXECUTABLE NAME, so it can only ever find real X servers. `pkill -f`
    # matches command lines, which has twice in this repo matched the shell running the pkill.
    for pid in $(pgrep -x Xorg); do
        if tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -q -- " $DISP "; then
            sudo kill "$pid" || true
        fi
    done
    sleep 2
fi
# Clear a stale socket/lock left by a dead server, then start fresh.
sudo rm -f "/tmp/.X11-unix/X${N}" "/tmp/.X${N}-lock" 2>/dev/null || true

CONF="$(mktemp /tmp/xorg-render-XXXX.conf)"
cat > "$CONF" <<CONF
Section "ServerLayout"
    Identifier "layout"
    Screen 0 "screen0"
EndSection
Section "Device"
    Identifier "nvrender"
    Driver "nvidia"
    BusID "$BUSID"
    Option "AllowEmptyInitialConfiguration" "true"
EndSection
Section "Screen"
    Identifier "screen0"
    Device "nvrender"
    Option "ConnectedMonitor" "DFP-0"
    DefaultDepth 24
    SubSection "Display"
        Depth 24
        Virtual $VW $VH
    EndSubSection
EndSection
CONF
sudo nohup Xorg "$DISP" -config "$CONF" -ac -noreset >/tmp/xorg-render.log 2>&1 &

# POLL for readiness — cold GPU init routinely takes >5s, so a fixed sleep gives false failures.
for _ in $(seq 1 30); do
    if DISPLAY="$DISP" glxinfo >/dev/null 2>&1; then
        echo "Xorg ${DISP} up: $(DISPLAY="$DISP" glxinfo 2>/dev/null | grep -i 'OpenGL renderer' | head -1)"
        exit 0
    fi
    sleep 1
done
echo "Xorg ${DISP} FAILED after 30s — see /var/log/Xorg.${N}.log" >&2
exit 1
