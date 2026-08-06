import { launch, VIEWPORT } from './_launch.mjs';
const base = process.env.BASE || 'https://integrity.bothead.net';
const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('console', (m) => console.log(`[${m.type()}]`, m.text().slice(0, 300)));
p.on('pageerror', (e) => console.log('PAGEERR:', e.message.slice(0, 400)));
await p.goto(`${base}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(async () => {
  const w = window.__terra;
  w.set_alt_bounds(0.05, 8e10);
  w.place_camera(45.3, -69.0, 1.7, 0.6, 0.02);
  await new Promise((r) => setTimeout(r, 4000));
});
await b.close();
