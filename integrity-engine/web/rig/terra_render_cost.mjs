// Is the stall the ENGINE or the harness? A headless compositor can invent hitches (this repo has been
// bitten by exactly that class of artifact before — the 1 Hz pacing trap), so measure `terra.render()`
// itself, which is app-side and cannot be confused with presentation. Reported against the same frame gap
// so the two can be compared directly.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (t.startsWith('swarm')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
// Wrap render() so every call is timed, without touching the app.
await p.evaluate(() => {
  const t = window.__terra;
  const orig = t.render.bind(t);
  window.__r = []; let last = performance.now();
  t.render = () => {
    const a = performance.now(); orig(); const bb = performance.now();
    window.__r.push([Math.floor((bb - last)), Math.floor(bb - a)]); last = bb;
  };
  window.__terra.place_camera(10, 0, 700000, 0, -0.55);
});
const sample = async (label) => {
  await p.evaluate(() => { window.__r.length = 0; });
  await p.waitForTimeout(4000);
  const r = await p.evaluate(() => {
    const gaps = window.__r.map(x => x[0]), rend = window.__r.map(x => x[1]);
    const pct = (a, q) => a.slice().sort((x, y) => x - y)[Math.floor(a.length * q)] || 0;
    return {
      frames: gaps.length,
      gap_p50: pct(gaps, 0.5), gap_p99: pct(gaps, 0.99), gap_max: Math.max(...gaps),
      render_p50: pct(rend, 0.5), render_p99: pct(rend, 0.99), render_max: Math.max(...rend),
      over50: rend.filter(x => x > 50).length, over200: rend.filter(x => x > 200).length,
      worst5: rend.slice().sort((a, b) => b - a).slice(0, 5),
      drawn: window.__terra.drawn_count(), inFlight: window.__terra.flight_count(),
    };
  });
  console.log(label, JSON.stringify(r));
  return r;
};
await sample('IDLE   ');
await p.evaluate(() => window.launchSwarm());
await sample('T+0..4 ');
await sample('T+4..8 ');
await p.waitForTimeout(28000);
await sample('T+32   ');
await p.waitForTimeout(4000);
await sample('T+40   ');
await b.close(); console.log('done');
