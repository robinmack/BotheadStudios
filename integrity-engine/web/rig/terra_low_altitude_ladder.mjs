// **At what altitude does the ground stop being drawn?** — a ladder, because guessing is how a session
// gets spent theorising about the wrong thing.
//
// `terra_camera_is_matter.mjs` measured the camera coming to rest 0.35 m above the ground — exactly the
// shell's half-extent, which is the right answer — and photographed an EMPTY STARFIELD. One of those is
// about the camera and the other is about the render, and the only way to tell which is to walk down
// and watch where the picture changes.
import { launch } from './_launch.mjs';

const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const LAT = +(process.env.LAT || 39.0);
const LON = +(process.env.LON || -106.0);
const MINALT = +(process.env.MINALT || 0.001);

const b = await launch();
const p = await b.newPage({ viewport: { width: 480, height: 320 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);

await p.evaluate(
  ({ lat, lon, minalt }) => {
    const t = window.__terra;
    t.set_alt_bounds(minalt, 8e10);
    t.set_epoch_sun_over_lon(lon);
    t.set_fly(lat, lon, 3000, 0, -0.6);
  },
  { lat: LAT, lon: LON, minalt: MINALT },
);
await p.waitForTimeout(2500);

console.log(`min_alt=${MINALT} at ${LAT}, ${LON}`);
console.log('asked      settled    picture');
for (const alt of [2000, 500, 100, 20, 5, 2, 1, 0.5, 0.2]) {
  const settled = await p.evaluate(
    async ({ lat, lon, alt }) => {
      const t = window.__terra;
      t.set_fly(lat, lon, alt, 0, -0.25);
      await new Promise((r) => setTimeout(r, 900));
      return t.altitude_m();
    },
    { lat: LAT, lon: LON, alt },
  );
  const name = `ladder-${String(alt).replace('.', 'p')}`;
  await p.screenshot({ path: `${out}/${name}.png` });
  console.log(`${(alt + ' m').padStart(8)} ${(settled.toFixed(2) + ' m').padStart(10)}   ${name}.png`);
}
await b.close();
