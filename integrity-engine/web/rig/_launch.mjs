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
