// **Does ONE sphere segment draw the same Earth as globe + cap?** (docs/63)
//
// The collapse is only safe if the two agree on where the ground is. A test says a small segment lands
// where the tangent cap does; this asks the picture, across the range the pair was built to span — the
// cross-fade band included, since that band exists only because there are two meshes.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const LAT = +(process.env.LAT || 39);
const LON = +(process.env.LON || -106);
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(({ LON }) => window.__terra.set_epoch_sun_over_lon(LON + 70), { LON });
await p.evaluate(() => {
  const t = window.__terra, orig = t.render.bind(t);
  window.__r = []; let last = 0;
  t.render = () => { const n = performance.now(); if (n - last < 16.7) return; last = n;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a); };
});
for (const alt of [1.0e7, 4.0e5, 8.0e3, 3.0e2]) {
  for (const mode of [0, 1]) {
    await p.evaluate(({ mode, alt, LAT, LON }) => {
      window.__terra.set_surface_mode(mode);
      window.__terra.set_fly(LAT, LON, alt, 0.6, -0.45);
      window.__r.length = 0;
    }, { mode, alt, LAT, LON });
    await p.waitForTimeout(2500);
    const s = await p.evaluate(() => {
      const a = window.__r; if (!a.length) return { p50: 0, max: 0 };
      const q = a.slice().sort((x, y) => x - y);
      return { p50: +q[Math.floor(a.length / 2)].toFixed(2), max: +Math.max(...a).toFixed(1) };
    });
    await p.screenshot({ path: `${out}/seg-${alt.toExponential(0)}-m${mode}.png` });
    console.log(`  alt ${alt.toExponential(0)}  mode ${mode ? 'SEGMENT' : 'globe+cap'}  p50 ${s.p50} ms  max ${s.max} ms`);
  }
}
await b.close(); console.log('done');
