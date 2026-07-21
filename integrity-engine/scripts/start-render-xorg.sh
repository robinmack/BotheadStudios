#!/usr/bin/env bash
# Start a headless Xorg on an NVIDIA GPU so headless WebGPU renders can COMPOSITE into screenshots
# (rig verification — the Laws require rig-watching every visual claim). Without a real GPU-backed X
# server, a software display (xvfb) cannot read back the GPU swapchain and screenshots come out blank.
#
# THE OTHER HALF is device matching: force WebGPU onto the SAME GPU as this Xorg compositor, or presenting
# across two GPUs throws VK_ERROR_DEVICE_LOST. `scripts/rigshot.sh` sets MESA_VK_DEVICE_SELECT for that.
#
# Machine-specific defaults are for THIS workstation (RTX 5060 Ti @ PCI:2:0:0). Override via env.
set -euo pipefail
DISP="${RENDER_DISPLAY:-:2}"
BUSID="${RENDER_BUSID:-PCI:2:0:0}"     # 5060 Ti. 2070 is PCI:4:0:0 (but its Xorg 'no screens' on this box).
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
if [ -e "/tmp/.X11-unix/X${DISP#:}" ]; then echo "Xorg ${DISP} already up"; exit 0; fi
sudo nohup Xorg "$DISP" -config "$CONF" -ac -noreset >/tmp/xorg-render.log 2>&1 &
sleep 5
if DISPLAY="$DISP" glxinfo 2>/dev/null | grep -qi "OpenGL renderer"; then
    echo "Xorg ${DISP} up: $(DISPLAY=$DISP glxinfo 2>/dev/null | grep -i 'OpenGL renderer' | head -1)"
else
    echo "Xorg ${DISP} FAILED — see /var/log/Xorg.${DISP#:}.log" >&2; exit 1
fi
