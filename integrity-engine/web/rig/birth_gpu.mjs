// Rig-watch the Birth-of-the-Moon scene now running on the GPU SPH path (docs/33 stage 5). birth.html
// auto-starts the GPU impact; this just loads it and screenshots the evolving field + HUD disk stats.
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', (m) => { const t = m.text(); if (!t.includes('[vite]') && !t.includes('deprecated')) console.log('PAGE:', t); });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
const marks = [1500, 1500, 2000, 3000];
let t = 2500;
await p.waitForTimeout(t);
await p.screenshot({ path: `${out}/birth-gpu-${t}ms.png` });
console.log(`shot t+${t}ms · disk`, await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no'));
for (const dt of marks) {
  await p.waitForTimeout(dt);
  t += dt;
  await p.screenshot({ path: `${out}/birth-gpu-${t}ms.png` });
  console.log(`shot t+${t}ms · disk`, await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no'));
}
await b.close();
console.log('done');
