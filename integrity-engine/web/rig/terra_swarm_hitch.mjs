// Where do the hitches fall? Sample every frame gap across a whole entry, bucketed per second, alongside
// what the engine was holding at the time. A median tells you nothing about a freeze; the worst gap in each
// second tells you exactly when the page stopped answering.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (t.startsWith('swarm')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => window.__terra.set_fly(10, 0, 700000, 0, -0.55));
await p.evaluate(() => {
  window.__log = []; let last = performance.now(); const t0 = last;
  const tick = () => {
    const n = performance.now();
    window.__log.push([Math.floor((n - t0) / 1000), n - last, window.__terra.drawn_count(), window.__terra.flight_count()]);
    last = n; requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
});
await p.waitForTimeout(1500);
await p.evaluate(() => window.launchSwarm());
await p.waitForTimeout(60000);
const rows = await p.evaluate(() => {
  const per = new Map();
  for (const [s, gap, drawn, inf] of window.__log) {
    const e = per.get(s) || { n: 0, worst: 0, sum: 0, drawn: 0, inf: 0 };
    e.n++; e.worst = Math.max(e.worst, gap); e.sum += gap; e.drawn = Math.max(e.drawn, drawn); e.inf = inf;
    per.set(s, e);
  }
  return [...per.entries()].map(([s, e]) => ({
    s, fps: +(1000 / (e.sum / e.n)).toFixed(0), worstMs: Math.round(e.worst), drawn: e.drawn, inFlight: e.inf,
  }));
});
for (const r of rows) if (r.worstMs > 80 || r.drawn > 1300) console.log(JSON.stringify(r));
console.log('WORST OVERALL', Math.max(...rows.map(r => r.worstMs)), 'ms');
await b.close(); console.log('done');
