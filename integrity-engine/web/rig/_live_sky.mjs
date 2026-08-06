// Point the sky check at PRODUCTION, not the dev server: a deploy is only done when the deployed
// thing renders. (A 200 is not a scene — this repo has paid for that three times.)
import { launch, VIEWPORT } from './_launch.mjs';
import { decodePng } from './_png.mjs';
import { GALWAY_GROUND, pose } from './_poses.mjs';
const base = process.env.BASE || 'https://integrity.bothead.net';
const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message.slice(0, 200)));
for (const [label, q] of [['live', ''], ['live-airless', '?world=earth-airless']]) {
  await p.goto(`${base}/terra.html${q}`, { waitUntil: 'load' });
  await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
  await p.waitForTimeout(4000);
  await pose(p, { ...GALWAY_GROUND, sunLon: GALWAY_GROUND.lon }, { epoch: 1718945000, settleMs: 2200 });
  await p.screenshot({ path: `${process.env.OUT || '/tmp'}/${label}-sky.png` });
  const img = decodePng(await p.screenshot());
  const c = img.channels;
  let r = 0, g = 0, bl = 0, n = 0;
  for (let y = Math.floor(0.02 * img.height); y < Math.floor(0.2 * img.height); y++)
    for (let x = 0; x < img.width; x++) {
      const i = (y * img.width + x) * c;
      r += img.data[i]; g += img.data[i + 1]; bl += img.data[i + 2]; n++;
    }
  console.log(`${label.padEnd(14)} sky ${(r/n).toFixed(1)}/${(g/n).toFixed(1)}/${(bl/n).toFixed(1)}`);
}
await b.close();
