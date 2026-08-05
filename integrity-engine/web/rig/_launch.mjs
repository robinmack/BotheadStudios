// **The one way a rig launches Chromium.** Every rig must use this.
//
// `--disable-frame-rate-limit` is not a tuning knob — without it Chromium paces this headless-Xorg
// setup at exactly 1 Hz (1003 ms, ±0.2 ms), and EVERY frame-rate or smoothness measurement is capped at
// 1 fps regardless of what the engine does. That artifact was briefly mistaken for a real engine
// performance collapse: an INDEPENDENT empty rAF loop measured 1.0 fps on all three scenes, which is
// what proved it was the browser and not the workload. With the flag, terrain measures 18 fps.
//
// The other flags: WebGPU on the real GPU (`rigshot.sh` pins WHICH GPU via MESA_VK_DEVICE_SELECT), and
// no occlusion/background throttling, so a rig that is not the focused window still renders.
import { chromium } from 'playwright';

export const ARGS = [
  '--enable-unsafe-webgpu',
  '--enable-features=Vulkan',
  '--use-angle=vulkan',
  '--no-sandbox',
  '--disable-frame-rate-limit',
  '--disable-gpu-vsync',
  '--disable-backgrounding-occluded-windows',
  '--disable-renderer-backgrounding',
  '--disable-features=CalculateNativeWinOcclusion',
];

export const launch = (opts = {}) => chromium.launch({ headless: false, args: ARGS, ...opts });
export const PORT = process.env.PORT || '5173';
export const OUT = process.env.OUT || '/tmp';
export const url = (page) => `http://127.0.0.1:${PORT}/${page}`;

// ★ **ONE viewport for the fleet.** Rigs each picked their own — eighteen different sizes, the smallest
// 480x320 — and the ones that fill the gallery were among the smallest, so the pictures a human is meant
// to judge textures from were the least judgeable ones. (Robin, 2026-08-05: *"your screen shots are
// pretty low res… 640x480 seems to be the go-to?"* It was worse: the last batch was 560x400.)
//
// 2K, per Robin, matching the render Xorg's own ceiling — `scripts/start-render-xorg.sh` allocates the
// framebuffer and a rig cannot capture more pixels than that server has. Override per run with
// `RIG_W`/`RIG_H` (a measurement rig that only wants a mean level can run small and fast).
//
// The aspect is 16:10, deliberately unchanged from the 1280x800 that 50 rigs already used, so a rig's
// framing and any width-fraction sampling survive the change untouched.
export const VIEWPORT = {
  width: Number(process.env.RIG_W || 2560),
  height: Number(process.env.RIG_H || 1600),
};
