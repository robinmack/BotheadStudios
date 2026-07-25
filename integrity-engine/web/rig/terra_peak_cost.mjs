// Render cost at ACTUAL peak trail, paced like a real browser. The previous run's own camera drag moved
// the target before launch, so the entry never reached thick air — measure without touching the camera.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (t.startsWith('swarm')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => {
  const t = window.__terra;
  const orig = t.render.bind(t);
  window.__r = []; let lastRender = 0;
  t.render = () => {
    const now = performance.now();
    if (now - lastRender < 16.7) return;
    lastRender = now;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a);
  };
  t.set_fly(10, 0, 700000, 0, -0.55);
  window.launchSwarm();
});
let worst = 0, peakDrawn = 0;
for (let i = 1; i <= 24; i++) {
  await p.evaluate(() => { window.__r.length = 0; });
  await p.waitForTimeout(3000);
  const r = await p.evaluate(() => {
    const a = window.__r, s = a.slice().sort((x, y) => x - y), t = window.__terra;
    return {
      renders: a.length, p50: +s[Math.floor(a.length / 2)].toFixed(2),
      max: +Math.max(...a).toFixed(1), over50: a.filter(x => x > 50).length,
      drawn: t.drawn_count(), ablated: Math.round(t.trail_mass_kg()),
    };
  });
  worst = Math.max(worst, r.max); peakDrawn = Math.max(peakDrawn, r.drawn);
  console.log(`t+${i * 3}s`, JSON.stringify(r));
}
console.log(`WORST RENDER ${worst} ms at up to ${peakDrawn} drawn (paced at ~60fps)`);
await b.close(); console.log('done');
