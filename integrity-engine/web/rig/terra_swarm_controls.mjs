// "We seem to lose camera controls when the engine is working" (Robin, 2026-07-24). A screenshot cannot
// see this, and neither can an fps average: what matters is whether INPUT still moves the camera while the
// entry is at its heaviest. So this drives the camera the way a person would — a right-drag look and a
// wheel zoom — before, during and after the burn, and reports how far the camera actually moved plus the
// worst frame gap it saw. Losing controls IS "the drag produced no movement".
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (t.startsWith('swarm')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => window.__terra.place_camera(10, 0, 700000, 0, -0.55));

// Watch frame gaps continuously, so a freeze cannot hide between samples.
await p.evaluate(() => {
  window.__gaps = []; let last = performance.now();
  const tick = () => { const n = performance.now(); window.__gaps.push(n - last); last = n; requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
});

// One drag + one zoom, and report how much the camera moved in response.
const probe = async (label) => {
  const before = await p.evaluate(() => {
    window.__gaps.length = 0;
    const t = window.__terra;
    return { lat: t.latitude(), lon: t.longitude(), alt: t.altitude_m() };
  });
  // A real right-drag across the canvas, then a wheel zoom.
  await p.mouse.move(500, 400);
  await p.mouse.down({ button: 'right' });
  for (let i = 1; i <= 10; i++) await p.mouse.move(500 + i * 12, 400 + i * 4);
  await p.mouse.up({ button: 'right' });
  await p.mouse.wheel(0, -300);
  await p.waitForTimeout(1200);
  const after = await p.evaluate(() => {
    const t = window.__terra;
    const g = window.__gaps;
    return {
      lat: t.latitude(), lon: t.longitude(), alt: t.altitude_m(),
      frames: g.length, worstGapMs: Math.round(Math.max(...g)),
      medianGapMs: Math.round(g.slice().sort((a, c) => a - c)[Math.floor(g.length / 2)] || 0),
      inFlight: t.flight_count(), drawn: t.drawn_count(),
    };
  });
  const moved = Math.abs(after.lat - before.lat) + Math.abs(after.lon - before.lon);
  const zoomed = Math.abs(after.alt - before.alt);
  console.log(`${label}: camera moved ${moved.toFixed(4)}deg, alt changed ${zoomed.toFixed(0)}m | ` +
    `frames ${after.frames} median ${after.medianGapMs}ms worst ${after.worstGapMs}ms | ` +
    `inFlight ${after.inFlight} drawn ${after.drawn}`);
  return { moved, zoomed, ...after };
};

const quiet = await probe('BEFORE (idle)');
await p.evaluate(() => window.launchSwarm());
await p.waitForTimeout(33000); // arrive at the thick air, where the work peaks
const busy = await probe('DURING (entry)');
await p.waitForTimeout(6000);
const busy2 = await probe('DURING (peak trail)');
await p.screenshot({ path: `${out}/controls-during.png` });
console.log('VERDICT', JSON.stringify({ quiet, busy, busy2 }));
await b.close(); console.log('done');
