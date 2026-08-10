// Price the render path against the physics path. Both scale with the same number, so the only way to
// know which produces the half-second stalls is to move one and not the other (gpu-perf §5: never price a
// stage by deleting it if something downstream depends on it — here nothing does, the physics is identical
// either way, which is what makes this ablation valid).
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
for (const draw of [2, 1, 0]) {
  const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
  p.on('pageerror', e => console.log('PAGEERR:', e.message));
  await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
  await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
  await p.waitForTimeout(3000);
  await p.evaluate((draw) => {
    const t = window.__terra;
    t.set_draw_matter(draw);
    const orig = t.render.bind(t);
    window.__r = [];
    t.render = () => { const a = performance.now(); orig(); window.__r.push(performance.now() - a); };
    t.place_camera(10, 0, 700000, 0, -0.55);
    t.launch_swarm_n(1200);
  }, draw);
  await p.waitForTimeout(1500);
  await p.evaluate(() => { window.__r.length = 0; });
  await p.waitForTimeout(8000);
  const r = await p.evaluate(() => {
    const a = window.__r, s = a.slice().sort((x, y) => x - y);
    return {
      frames: a.length, p50: +s[Math.floor(a.length / 2)].toFixed(2),
      p99: +s[Math.floor(a.length * 0.99)].toFixed(1), max: Math.round(Math.max(...a)),
      over200: a.filter(x => x > 200).length, inFlight: window.__terra.flight_count(),
    };
  });
  console.log(`mode=${draw} (${['none','upload','upload+draw'][draw]})`.padEnd(26), JSON.stringify(r));
  await p.close();
}
await b.close(); console.log('done');
