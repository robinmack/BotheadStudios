import { launch, VIEWPORT } from './_launch.mjs';
const base = process.env.BASE || 'https://integrity.bothead.net';
const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('console', (m) => { const t = m.text(); if (/flora|plant/i.test(t)) console.log('C:', t.slice(0, 200)); });
await p.goto(`${base}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(4000);
const r = await p.evaluate(async () => {
  const w = window.__terra;
  w.set_alt_bounds(0.05, 8e10);
  w.set_epoch(1730000000);
  w.place_camera(45.3, -69.0, 1.7, 0.6, 0.02);
  await new Promise((r) => setTimeout(r, 3000));
  return { alt: w.altitude_m?.(), mat: w.surface_material_at?.(45.3, -69.0) };
});
console.log(JSON.stringify(r));
await b.close();
