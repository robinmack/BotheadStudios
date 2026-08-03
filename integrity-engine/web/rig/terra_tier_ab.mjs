// Now that a tier is a cache (docs/59 Stage B), tiers 2-4 are affordable. The question that decides
// whether the default should move off 1 is not the price — it is whether the ladder puts DETAIL on the
// ground. Same camera, same frame, one tier then four. A claim that detail improved is worth nothing
// without the pair.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const OCT = +(process.env.OCT || 16);
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => {
  const t = window.__terra, orig = t.render.bind(t);
  window.__r = []; let last = 0;
  t.render = () => {
    const n = performance.now(); if (n - last < 16.7) return; last = n;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a);
  };
});
// Pitched down at the ground rather than out at the horizon — relief underfoot is the thing the ladder
// is supposed to buy, and a horizon shot is mostly sky.
for (const [alt, pitch] of [[8000, -0.55], [2000, -0.60], [500, -0.65], [100, -0.70]]) {
  for (const tiers of [1, 4]) {
    await p.evaluate(({ tiers, alt, pitch, OCT }) => {
      window.__terra.set_octave_budget(OCT);
      window.__terra.set_fly(28, 86, alt, 0.6, pitch);
      window.__r.length = 0;
    }, { tiers, alt, pitch, OCT });
    await p.waitForTimeout(2000);
    const s = await p.evaluate(() => {
      const a = window.__r; if (!a.length) return { p50: 0, max: 0 };
      const q = a.slice().sort((x, y) => x - y);
      return { p50: +q[Math.floor(a.length / 2)].toFixed(2), max: +Math.max(...a).toFixed(1) };
    });
    await p.screenshot({ path: `${out}/tier-${alt}m-t${tiers}.png` });
    console.log(`alt ${String(alt).padStart(5)} m  tiers ${tiers}  p50 ${s.p50} ms  max ${s.max} ms`);
  }
}
await b.close();
console.log('done');
