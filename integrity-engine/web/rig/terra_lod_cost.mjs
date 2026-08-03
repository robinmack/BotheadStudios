// Price the ground ladder: tiers and octaves each cost frame time and each buy detail. Move ONE at a time
// (gpu-perf §5) at a fixed altitude, paced like a real browser, so the exchange rate is measured rather than
// assumed. Baseline is the ladder the engine shipped before this: one tier, no generated relief.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
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
const price = async (tiers, oct) => {
  await p.evaluate(({ tiers, oct }) => {
    window.__terra.set_octave_budget(oct);
    window.__terra.set_fly(28, 86, 2000, 0.6, -0.35);
    window.__r.length = 0;
  }, { tiers, oct });
  await p.waitForTimeout(2500);
  const r = await p.evaluate(() => {
    const a = window.__r, s = a.slice().sort((x, y) => x - y);
    return a.length ? { p50: +s[Math.floor(a.length / 2)].toFixed(2), max: +Math.max(...a).toFixed(1), n: a.length }
                    : { p50: 0, max: 0, n: 0 };
  });
  console.log(`tiers=${tiers} octaves=${String(oct).padStart(2)} -> p50 ${String(r.p50).padStart(7)} ms  max ${String(r.max).padStart(7)} ms  (${r.n} frames)`);
};
await price(1, 0);   // the ladder as it shipped
for (const oct of [2, 4, 6, 10, 15]) await price(1, oct);
for (const tiers of [2, 3, 4]) await price(tiers, 4);
await b.close(); console.log('done');
