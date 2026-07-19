import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5307';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.evaluate(() => window.__demo?.set_render_blend?.(1)); // physics view to see the disk
let prev = 0;
for (const t of [20,35,50,65,80,95]) {
  await p.waitForTimeout(t*1000 - prev*1000); prev = t;
  const d = await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no');
  console.log(`t=${t}s ${d}`);
}
await p.evaluate(() => window.__demo?.set_render_blend?.(0));
await p.waitForTimeout(1000);
await p.screenshot({ path: `${out}/parity-pretty.png` });
await b.close(); console.log('done');
