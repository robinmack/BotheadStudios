// **Can you see blades of grass in Ireland?** — the near half of Law IV, photographed.
//
// Robin: *"so will I see blades of grass in Ireland?"* and *"these are hues at altitude but must become
// realistic flora at very low altitude."* This walks the camera DOWN over Galway and reports how many
// plants the engine resolved at each altitude. Above ~300 m a tuft is sub-pixel and the ground's albedo
// is the whole answer; below it the same ground answers with the plants themselves.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const [LAT, LON] = [53.10, -9.45];
const b = await launch();
const p = await b.newPage({ viewport: { width: 900, height: 620 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message.split('\n')[0]));
await p.route('**/__log', async (r) => {
  const d = r.request().postData();
  if (d && /flora/.test(d)) console.log('  ' + JSON.parse(d).msg);
  await r.fulfill({ status: 200, body: 'ok' });
});
await p.goto('http://127.0.0.1:5173/terra.html', { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);
await p.evaluate(({ lat, lon }) => {
  const t = window.__terra;
  t.set_alt_bounds(0.05, 8e10);
  t.set_epoch_sun_over_lon(lon);           // daylight, so the plants are lit
  t.place_camera(lat, lon, 4000, 0, -0.5);
}, { lat: LAT, lon: LON });
await p.waitForTimeout(2500);

console.log('alt      plants  frame');
for (const alt of [4000, 600, 300, 120, 40, 12, 3, 1.5]) {
  await p.evaluate(({ lat, lon, alt }) => {
    // Look toward the horizon when low, down from above — where the plants would BE.
    window.__terra.place_camera(lat, lon, alt, 0, alt > 200 ? -0.6 : -0.12);
  }, { lat: LAT, lon: LON, alt });
  await p.waitForTimeout(1500);
  const n = await p.evaluate(() => window.__terra.flora_count?.() ?? -1);
  const name = `flora-${String(alt).replace('.', 'p')}m`;
  await p.screenshot({ path: `${out}/${name}.png` });
  console.log(`${(alt + ' m').padStart(8)} ${String(n).padStart(6)}  ${name}.png`);
}
await b.close();
