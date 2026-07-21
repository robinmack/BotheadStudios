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

# Already up AND responding to GLX? Done. (Check the server, not just a possibly-stale socket file.)
if DISPLAY="$DISP" glxinfo >/dev/null 2>&1; then
    echo "Xorg ${DISP} already up: $(DISPLAY="$DISP" glxinfo 2>/dev/null | grep -i 'OpenGL renderer' | head -1)"
    exit 0
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
        Virtual 1280 800
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
