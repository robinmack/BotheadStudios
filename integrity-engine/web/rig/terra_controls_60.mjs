// Robin's actual complaint, under realistic conditions: does INPUT still move the camera while the entry
// is at its heaviest, with the page paced like a real vsynced browser? Drives a right-drag and a wheel
// zoom before, at peak trail, and after — and reports the worst render and the worst frame gap seen.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
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
  window.__r = []; window.__g = []; let lastRender = 0, lastFrame = performance.now();
  t.render = () => {
    const now = performance.now();
    window.__g.push(now - lastFrame); lastFrame = now;
    if (now - lastRender < 16.7) return;      // pace like a vsynced browser
    lastRender = now;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a);
  };
  t.place_camera(10, 0, 700000, 0, -0.55);
});
const probe = async (label) => {
  const before = await p.evaluate(() => {
    window.__r.length = 0; window.__g.length = 0;
    const t = window.__terra; return { lat: t.latitude(), lon: t.longitude(), alt: t.altitude_m() };
  });
  await p.mouse.move(500, 400);
  await p.mouse.down({ button: 'right' });
  for (let i = 1; i <= 10; i++) { await p.mouse.move(500 + i * 12, 400 + i * 4); await p.waitForTimeout(20); }
  await p.mouse.up({ button: 'right' });
  await p.mouse.wheel(0, -300);
  await p.waitForTimeout(1200);
  const r = await p.evaluate(() => {
    const t = window.__terra;
    return {
      lat: t.latitude(), lon: t.longitude(), alt: t.altitude_m(),
      renderMax: +Math.max(...window.__r).toFixed(1), gapMax: Math.round(Math.max(...window.__g)),
      inFlight: t.flight_count(), drawn: t.drawn_count(), ablatedKg: Math.round(t.trail_mass_kg()),
    };
  });
  const moved = Math.abs(r.lat - before.lat) + Math.abs(r.lon - before.lon);
  console.log(`${label} moved ${moved.toFixed(2)}deg alt ${Math.round(Math.abs(r.alt - before.alt))}m | ` +
    `renderMax ${r.renderMax}ms gapMax ${r.gapMax}ms | inFlight ${r.inFlight} drawn ${r.drawn} ablated ${r.ablatedKg}kg`);
  return moved;
};
const a = await probe('IDLE      ');
await p.evaluate(() => window.launchSwarm());
await p.waitForTimeout(36000);
const c = await probe('PEAK TRAIL');
await p.screenshot({ path: `${out}/controls60-peak.png` });
const d = await probe('AFTER     ');
console.log(a > 1 && c > 1 && d > 1 ? 'PASS: the camera answers input throughout' : 'FAIL: input lost');
await b.close(); console.log('done');
