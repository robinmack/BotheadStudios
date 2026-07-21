// Watch the GPU birth impact over a longer window — does the debris form an orbiting disk, or escape/disperse?
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
let t = 0;
for (const dt of [5000, 5000, 5000, 5000, 5000, 5000]) {
  await p.waitForTimeout(dt);
  t += dt;
  const s = await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no');
  console.log(`t+${t / 1000}s: ${s}`);
  await p.screenshot({ path: `${out}/birth-long-${t / 1000}s.png` });
}
await b.close();
console.log('done');
