// Does the ground get FINER as you descend? Park the camera over mountainous daylit land and step down
// through decades of altitude, shooting each. This is the half of docs/59 Stage B that was missing: the
// altitude descended continuously before, but the surface never gained detail.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const LAT = Number(process.env.LAT ?? 28), LON = Number(process.env.LON ?? 86); // Himalaya, daylit ~20:00Z
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => {
  const t = window.__terra, orig = t.render.bind(t);
  window.__r = []; let last = 0;
  t.render = () => { const n = performance.now(); if (n - last < 16.7) return; last = n;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a); };
});
for (const alt of [200000, 30000, 8000, 2000, 500, 120, 30, 6]) {
  await p.evaluate(({ LAT, LON, alt }) => { window.__r.length = 0; window.__terra.place_camera(LAT, LON, alt, 0.6, -0.35); },
    { LAT, LON, alt });
  await p.waitForTimeout(1800);
  const r = await p.evaluate(() => {
    const a = window.__r, t = window.__terra, s = a.slice().sort((x, y) => x - y);
    return { altM: Math.round(t.altitude_m()), biome: t.ground_biome(),
      renderP50: +s[Math.floor(a.length / 2)].toFixed(2), renderMax: +Math.max(...a).toFixed(1), frames: a.length };
  });
  console.log(`alt ${String(r.altM).padStart(7)} m | ${r.biome.padEnd(10)} | render p50 ${r.renderP50} ms max ${r.renderMax} ms (${r.frames} frames)`);
  await p.screenshot({ path: `${out}/lod-${alt}m.png` });
}
await b.close(); console.log('done');
