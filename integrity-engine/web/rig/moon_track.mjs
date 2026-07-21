import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 700 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
let prev = 0;
for (let t=10; t<=150; t+=8) {
  await p.waitForTimeout(t*1000 - prev*1000); prev = t;
  const m = await p.evaluate(() => window.__demo?.gpu_moon_track_json?.() ?? 'null');
  if (m !== 'null') console.log(`t=${t}s moon ${m}`);
}
await b.close(); console.log('done');
