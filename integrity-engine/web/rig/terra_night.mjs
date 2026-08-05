// **How bright is the night side, and is grass brighter than everything else?**
//
// Robin: *"the engine seems to be rendering the color without taking available light into account…
// Grass shouldn't be apparently brighter than everything else at night, right?"*
//
// The sun is put on the OPPOSITE side of the planet, so every site here is at local midnight. Anything
// that is not near-black is being lit by something the physics has not accounted for.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const SITES = [['galway',53.10,-9.45],['sahara',23.0,10.0],['amazon',-3.0,-60.0],['ocean',0.0,-140.0]];
const b = await launch();
const p = await b.newPage({ viewport: { width: 560, height: 400 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message.split('\n')[0]));
await p.goto('http://127.0.0.1:5173/terra.html', { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);
console.log('site      sun elev   material');
for (const [name, lat, lon] of SITES) {
  const info = await p.evaluate(async ({ lat, lon }) => {
    const t = window.__terra;
    t.set_alt_bounds(0.05, 8e10);
    t.set_epoch_sun_over_lon(lon + 180);      // midnight here
    t.place_camera(lat, lon, 900, 0, -0.55);
    await new Promise(r => setTimeout(r, 1800));
    return { elev: t.sun_elevation_deg(lat, lon), mat: t.surface_material_at(lat, lon) };
  }, { lat, lon });
  await p.screenshot({ path: `${out}/night-${name}.png` });
  console.log(`${name.padEnd(9)} ${info.elev.toFixed(1).padStart(6)}°   ${info.mat}`);
}
await b.close();
