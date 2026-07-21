// docs/42 Phase 1: verify the pretty⇄physics slider. Shots at blend 0 (pretty sphere), 0.5, 1 (particles).
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(15000); // let the impact resolve into a remnant + disk
for (const blend of [0, 0.5, 1]) {
  await p.evaluate((x) => window.__demo?.set_render_blend?.(x), blend);
  await p.waitForTimeout(1200);
  await p.screenshot({ path: `${out}/pretty-blend-${blend}.png` });
  console.log(`blend ${blend} · disk`, await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no'));
}
await b.close();
console.log('done');
