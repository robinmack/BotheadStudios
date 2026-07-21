// Rig-watch the full GPU birth → disk → geologic hand-off (docs/35 stage 5, 2c). Loads birth.html (GPU SPH
// impact), lets the disk form, triggers enter_geologic_time(), and checks the moonlet appears.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', (m) => { const t = m.text(); if (!t.includes('[vite]') && !t.includes('deprecated')) console.log('PAGE:', t); });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
const disk = () => p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'no');
// Let the impact + disk develop.
for (const t of [8000, 6000, 6000]) {
  await p.waitForTimeout(t);
  console.log('disk:', await disk());
}
await p.screenshot({ path: `${out}/geo-predisk.png` });
// Hand off to geologic time from the GPU disk.
await p.evaluate(() => window.__demo.enter_geologic_time());
console.log('entered geologic');
await p.waitForTimeout(1500);
await p.screenshot({ path: `${out}/geo-after.png` });
console.log('geologic disk_stats:', await p.evaluate(() => window.__demo?.disk_stats_json?.() ?? 'no'));
await b.close();
console.log('done');
