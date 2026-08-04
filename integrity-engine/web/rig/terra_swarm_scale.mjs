// Does the stall scale with the WORKLOAD? Same entry, 1 / 12 / 1200 fragments. If a single fragment stalls
// as badly as twelve hundred, the cost is not the physics and not the instance count — and guessing which
// it is, without this, is exactly what this project's rules forbid.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
for (const n of [1, 12, 1200]) {
  const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
  p.on('pageerror', e => console.log('PAGEERR:', e.message));
  await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
  await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
  await p.waitForTimeout(3000);
  await p.evaluate(() => {
    const t = window.__terra;
    const orig = t.render.bind(t);
    window.__r = []; let last = performance.now();
    t.render = () => { const a = performance.now(); orig(); const bb = performance.now(); window.__r.push(bb - a); last = bb; };
    t.place_camera(10, 0, 700000, 0, -0.55);
  });
  await p.evaluate((n) => window.__terra.launch_swarm_n(n), n);
  await p.waitForTimeout(1000);
  await p.evaluate(() => { window.__r.length = 0; });
  await p.waitForTimeout(6000);
  const r = await p.evaluate(() => {
    const a = window.__r; const s = a.slice().sort((x, y) => x - y);
    return {
      frames: a.length, p50: +s[Math.floor(a.length / 2)].toFixed(1),
      max: Math.round(Math.max(...a)), over200: a.filter(x => x > 200).length,
      inFlight: window.__terra.flight_count(), drawn: window.__terra.drawn_count(),
    };
  });
  console.log(`n=${String(n).padStart(4)}`, JSON.stringify(r));
  await p.close();
}
await b.close(); console.log('done');
