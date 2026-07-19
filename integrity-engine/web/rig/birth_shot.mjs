// Screenshot the space-band BIRTH-OF-THE-MOON scene so the agent can see the debris swarm / proto-lunar
// disk with its own eyes (the "watch locally" rule). Headed Chromium under xvfb — WebGPU needs a real
// swapchain, which headless cannot composite.
//
//   URL=https://integrity.bothead.net/birth.html OUT=/some/dir xvfb-run -a node rig/birth_shot.mjs
//   # or against a local dev server:  URL=http://127.0.0.1:5307/birth.html
//
// Captures a time series (pre-impact → strike → aftermath) so we can watch the disk form, not just a
// single frame. Prints the on-screen #stats (the Sim HUD) at each mark.
import { chromium } from 'playwright';

const URL = process.env.URL || 'https://integrity.bothead.net/birth.html';
const OUT = process.env.OUT || '/tmp';
const b = await chromium.launch({
  headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'],
});
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', (m) => console.log('[page]', m.text()));
p.on('pageerror', (e) => console.log('[pageerror]', e.message));
const stat = async () => (await p.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();

await p.goto(URL, { waitUntil: 'load' });
// Marks (seconds of wall-clock after load): catch just-before the strike, the strike, and the aftermath as
// the debris swarm evolves under the scene's time-LOD.
const marks = [3, 7, 12, 20, 32, 48];
let last = 0;
for (const s of marks) {
  await p.waitForTimeout((s - last) * 1000);
  last = s;
  await p.screenshot({ path: `${OUT}/birth-${s}s.png` });
  console.log(`t+${s}s: ${await stat()}`);
}
await b.close();
