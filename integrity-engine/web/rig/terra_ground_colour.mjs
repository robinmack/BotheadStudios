// **What colour is the ground, and is that colour the MATERIAL's doing?**
//
// Robin (2026-08-03): *"Pine Timber is always the wrong choice for flora though, we should look for
// 'pine needles' or 'pine leaves', same with other biomes."* The picture that prompted it came back
// the colour of a plank, because `earth.json` pointed land-cover class 3 at `pine` — the catalogue's
// pine TIMBER, albedo [0.68, 0.48, 0.21] — and Ireland sits inside the derived cover's forest band.
//
// `terra_light_check.mjs` already exonerated the lighting by measurement (ground luminance 117.9 noon
// / 14.2 midnight). So this rig asks the other question: with the sun PINNED overhead at each site so
// illumination is not a variable, what does the surface itself report? It photographs four biomes and
// prints the engine's own answer for which land-cover class and which material it is standing on, so
// the picture and the data can be checked against each other rather than one inferred from the other.
//
// ★ Measure the DECODED image, never the PNG bytes. A byte mean over compressed data gave
// 127.2 / 127.4 / 127.6 across noon, dusk and midnight, and would have "proved" lighting did nothing.
// `tools/ground_colour.py` decodes the frames this writes.
import { launch } from './_launch.mjs';

const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const SITES = [
  ['galway', 53.10, -9.45],
  ['amazon', -3.0, -60.0],
  ['siberia', 62.0, 100.0],
  ['sahara', 23.0, 10.0],
];

const b = await launch();
const p = await b.newPage({ viewport: { width: 720, height: 480 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);

// The frame is analysed from the SCREENSHOT, not from the canvas. A WebGPU canvas is only readable
// while its drawing buffer is current, so `drawImage` off it outside the present window hands back
// pure black — which the first version of this rig duly reported as 0.0/0.0/0.0 for every site while
// the material lookup beside it was perfectly correct. `scripts/rigshot.sh` composites the real screen
// through the GPU X server, so the PNG on disk is the picture; `tools/ground_colour.py` decodes it.

console.log('site       biome  material');
for (const [name, lat, lon] of SITES) {
  await p.evaluate(
    ({ lat, lon }) => {
      const t = window.__terra;
      t.set_alt_bounds(0.05, 8e10);
      t.set_epoch_sun_over_lon(lon); // sun straight overhead: same illumination at every site
      t.place_camera(lat, lon, 900, 0, -0.55);
    },
    { lat, lon },
  );
  await p.waitForTimeout(2200);
  await p.screenshot({ path: `${out}/ground-${name}.png` });
  // What the ENGINE says is underfoot, so the picture is checked against the DATA rather than guessed.
  const who = await p.evaluate(
    ({ lat, lon }) => window.__terra.surface_material_at(lat, lon),
    { lat, lon },
  );
  const [biome, mat] = who.split(':');
  console.log(`${name.padEnd(9)} ${biome.padStart(2)}     ${mat}`);
}
await b.close();
