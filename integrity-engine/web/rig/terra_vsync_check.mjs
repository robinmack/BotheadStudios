// Is the stall an artifact of an UNCAPPED frame rate? This harness runs with --disable-frame-rate-limit,
// so the page renders at 170-350 fps and uploads several times more per second than any real vsynced
// browser ever would. gpu-perf §9 is explicit that a browser number needs confirming in the conditions it
// will actually run in. Same scene, same swarm, render calls limited to ~60/s.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
for (const capMs of [0, 16.7]) {
  const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
  p.on('pageerror', e => console.log('PAGEERR:', e.message));
  await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
  await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
  await p.waitForTimeout(3000);
  await p.evaluate((capMs) => {
    const t = window.__terra;
    const orig = t.render.bind(t);
    window.__r = []; let lastRender = 0;
    t.render = () => {
      const now = performance.now();
      if (capMs > 0 && now - lastRender < capMs) return; // pace like a vsynced browser
      lastRender = now;
      const a = performance.now(); orig(); window.__r.push(performance.now() - a);
    };
    t.place_camera(10, 0, 700000, 0, -0.55);
    t.launch_swarm_n(1200);
  }, capMs);
  await p.waitForTimeout(1500);
  await p.evaluate(() => { window.__r.length = 0; });
  await p.waitForTimeout(10000);
  const r = await p.evaluate(() => {
    const a = window.__r, s = a.slice().sort((x, y) => x - y);
    return {
      renders: a.length, p50: +s[Math.floor(a.length / 2)].toFixed(2),
      p99: +s[Math.floor(a.length * 0.99)].toFixed(1), max: Math.round(Math.max(...a)),
      over200: a.filter(x => x > 200).length, over50: a.filter(x => x > 50).length,
    };
  });
  console.log(`${capMs === 0 ? 'UNCAPPED' : '~60fps  '}`, JSON.stringify(r));
  await p.close();
}
await b.close(); console.log('done');
