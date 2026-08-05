// **Does Ireland actually turn?** — the same ground, four dates, measured.
//
// Robin: *"wire it up so Ireland actually turns please, and the serengetti, maine, etc."*
//
// The scene changes ONE thing between frames: the date. Everything else — camera, place, sun distance —
// is held, so a colour difference can only be the season. `set_epoch` is the engine's own clock pin.
//
// ★ Maine is the strong case (deciduous, 45N). Ireland is pasture in the shipped land cover, so it
// turns by the GRASS material's milder senescence. The Serengeti is the honest negative: at 2S the day
// length barely moves, so the photoperiod model says "no season" — correct for the MECHANISM and wrong
// for the place, because East African grass cures by DROUGHT, which the engine does not model at all.
import { launch } from './_launch.mjs';

const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const SITES = [
  ['maine', 45.3, -69.0],
  ['ireland', 53.1, -9.45],
  ['serengeti', -2.3, 34.8],
  // ★ NEGATIVE CONTROL. Barren: sand + granite, no senescent state anywhere in the mixture. Any R:G
  // trend HERE is the sun's own elevation changing through the year, not the ground — which is exactly
  // the confound that made an earlier reading of this rig wrong (docs/46 row 41).
  ['sahara-control', 23.0, 10.0],
];
// Unix seconds: the solstices and the turn between them.
const DATES = [
  ['jun', 1718945000],
  ['sep', 1727000000],
  ['oct', 1729700000],
  ['dec', 1734744000],
];

const b = await launch();
const p = await b.newPage({ viewport: { width: 560, height: 400 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);

console.log('site       date   turned   material');
for (const [name, lat, lon] of SITES) {
  for (const [label, t] of DATES) {
    const info = await p.evaluate(
      async ({ lat, lon, t }) => {
        const w = window.__terra;
        w.set_alt_bounds(0.05, 8e10);
        // ★ `set_epoch_sun_over_lon` OVERWRITES the epoch — it solves for an instant near NOW when the
        // sun sits over a longitude. Calling it after `set_epoch` silently threw the date away and the
        // first run of this rig reported an identical season on all four dates. Pin the DATE only.
        w.set_epoch(t);
        // Now put the daylight overhead WITHOUT losing the date: the engine solves near the epoch
        // already pinned, so these frames differ by season and by nothing else.
        w.set_epoch_sun_over_lon(lon);
        w.place_camera(lat, lon, 1200, 0, -0.6);
        await new Promise((r) => setTimeout(r, 1600));
        return { mat: w.surface_material_at(lat, lon), turned: w.senescence_at?.(lat) ?? -1 };
      },
      { lat, lon, t },
    );
    await p.screenshot({ path: `${out}/season-${name}-${label}.png` });
    console.log(
      `${name.padEnd(10)} ${label}   ${String(info.turned.toFixed ? info.turned.toFixed(2) : info.turned).padStart(5)}   ${info.mat}`,
    );
  }
}
await b.close();
