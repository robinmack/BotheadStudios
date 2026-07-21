import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.evaluate(() => window.__demo?.set_render_blend?.(0));
for (const t of [6,9,12,16,20]) {
  await p.waitForTimeout(t*1000 - (t>6?[6,9,12,16,20][[6,9,12,16,20].indexOf(t)-1]*1000:0));
  const d = await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no');
  await p.screenshot({ path: `${out}/moonlet-${t}s.png` });
  console.log(`t=${t}s disk=${d}`);
}
await b.close(); console.log('done');
