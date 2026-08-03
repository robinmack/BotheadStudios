// Where is the daylight right now? A black frame and a broken renderer look identical, and this repo has
// already lost sessions to that (CLAUDE.md / JOURNAL: three rig runs spent on the night side). So: sweep
// longitude at a fixed altitude and report how much of the frame is lit, before concluding anything.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
for (let lon = -180; lon < 180; lon += 30) {
  await p.evaluate(({ lon }) => window.__terra.set_fly(28, lon, 8.0e6, 0.6, -0.45), { lon });
  await p.waitForTimeout(700);
  await p.screenshot({ path: `${out}/lit-${String(lon).padStart(4, '0')}.png` });
  console.log(`lon ${lon}: shot`);
}
await b.close(); console.log('done');
