// **Is the camera MATTER, or is it clamped?** — measured through the engine, not argued.
//
// Robin, canonical: *"If the camera isn't material, it can subvert our rules."* And on where it lives
// (2026-08-03): *"Camera must exist in the engine, but can be directed by the scene"*, because *"the
// engine does a lot of calculation based on what can be seen, so it must know everything about the
// camera all the time."*
//
// Terra used to keep the eye out of the ground with `alt_m.clamp(min_alt, ..)` stacked on a ground
// height that was the MAX over a 22 km neighbourhood — two fudges, and neither can slide. This asks
// the engine to put the eye BELOW the surface and reports where it actually ends up.
//
// ★ The zoom limit is deliberately set to a millimetre, so it CANNOT be the thing stopping the eye. If
// the camera comes to rest a shell's thickness above the ground with the clamp that far out of the
// way, the shell is what stopped it. That is the whole experiment: remove the suspect, keep the effect.
import { launch } from './_launch.mjs';

const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';

// Somewhere with real relief, and somewhere flat, because a slope is where a clamp and a contact
// differ: a clamp pushes straight up the radial, contact resolves along the surface normal.
const SITES = [
  ['rockies', 39.0, -106.0],
  ['himalaya', 28.0, 86.9],
  ['prairie', 41.0, -100.0],
];

const b = await launch();
const p = await b.newPage({ viewport: { width: 720, height: 480 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);

console.log('site       asked      settled     ground    clearance   verdict');
for (const [name, lat, lon] of SITES) {
  const r = await p.evaluate(
    async ({ lat, lon }) => {
      const t = window.__terra;
      // A millimetre zoom floor: the clamp is removed as a suspect.
      t.set_alt_bounds(0.001, 8e10);
      t.set_epoch_sun_over_lon(lon);
      // Arrive high, so the streamed tiles for this place are asked for and land.
      t.set_fly(lat, lon, 3000, 0, -0.6);
      await new Promise((r) => setTimeout(r, 2500));
      // Now ask for an eye 200 m BELOW the local ground.
      t.set_fly(lat, lon, -200, 0, -0.05);
      await new Promise((r) => setTimeout(r, 1200));
      return { alt: t.altitude_m(), lat: t.latitude(), lon: t.longitude() };
    },
    { lat, lon },
  );
  await p.screenshot({ path: `${out}/matter-${name}.png` });
  // `altitude_m` is height above the ground under the eye, so it IS the clearance.
  const verdict = r.alt >= 0 ? (r.alt < 5 ? 'RESTS ON IT' : 'floating') : 'INSIDE THE GROUND';
  console.log(
    `${name.padEnd(9)} ${'-200 m'.padStart(9)} ${(r.alt.toFixed(2) + ' m').padStart(10)} ` +
      `${'sea+e'.padStart(8)} ${(r.alt.toFixed(2) + ' m').padStart(10)}   ${verdict}`,
  );
}
await b.close();
