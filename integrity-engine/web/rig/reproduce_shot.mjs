// **Stand the engine in front of a user's screenshot again.**
//
// Usage: scripts/rigshot.sh reproduce_shot.mjs <shot.json> [out.png]   (vite on $PORT, GPU Xorg up)
//        URL=https://integrity.bothead.net/terra.html … to reproduce against the live site
//
// The record is what the 📷 Screenshot button saved beside the image (`share-view.ts`, `ShotRecord`):
// the engine's `view_state_json()` — lat, lon, altitude, yaw, pitch, the sky's instant, the clock's
// rate, what was drawn — plus the build, page and viewport. This loads the same page at the same
// viewport, hands the state back through `restore_view_json`, waits for the stream and the appearance
// patch to settle, and captures. Put the user's PNG beside the capture and the difference is the
// engine's, the build's or the data's — not the pose's.
import { readFileSync } from 'node:fs';
import { launch, url as devUrl } from './_launch.mjs';

const [, , recordPath, outPath] = process.argv;
if (!recordPath) {
  console.error('usage: reproduce_shot.mjs <shot.json> [out.png]');
  process.exit(2);
}
const record = JSON.parse(readFileSync(recordPath, 'utf8'));
const page = (record.page || '/terra.html').replace(/^\//, '');
const URL = process.env.URL || devUrl(page);
const out = outPath || recordPath.replace(/\.json$/, '') + '-reproduced.png';
const { width = 1280, height = 800, dpr = 1 } = record.viewport || {};

const b = await launch();
const ctx = await b.newContext({ viewport: { width: Math.round(width / dpr), height: Math.round(height / dpr) }, deviceScaleFactor: dpr });
const p = await ctx.newPage();
p.on('console', (m) => { const t = m.text(); if (/appearance:|Terra: loaded|tile .*quantum|error/i.test(t)) console.log('  [console]', t.slice(0, 160)); });
console.log(`reproducing shot #${record.n ?? '?'} (build ${record.build ?? '?'}) at ${URL}, ${width}x${height} @${dpr}`);
await p.goto(URL, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, { timeout: 90000 });
const applied = await p.evaluate((state) => window.__terra.restore_view_json(JSON.stringify(state)), record.state);
console.log(`  restore_view_json applied ${applied} field group(s)`);
// Settle: the stream lands its rings and the patch arrives in slices; wait until the tile count holds
// still for a while, then a little longer for the slices.
let last = -1, still = 0;
for (let i = 0; i < 120 && still < 20; i++) {
  await p.waitForTimeout(250);
  const n = await p.evaluate(() => window.__tiles?.() ?? 0);
  still = n === last ? still + 1 : 0;
  last = n;
}
await p.waitForTimeout(4000);
const now = await p.evaluate(() => JSON.parse(window.__terra.view_state_json()));
console.log(`  standing at lat ${now.lat.toFixed(5)} lon ${now.lon.toFixed(5)} alt ${now.alt_m.toFixed(1)} m, ${now.tiles_held} tiles (z ${now.tile_zooms.join('/')}), sun ${now.sun_elevation_deg.toFixed(1)}°`);
const want = record.state || {};
for (const k of ['lat', 'lon', 'alt_m', 'yaw', 'pitch']) {
  if (typeof want[k] === 'number' && Math.abs(want[k] - now[k]) > 1e-3) console.log(`  ! ${k}: asked ${want[k]}, standing at ${now[k]}`);
}
await p.screenshot({ path: out });
console.log(`frame -> ${out}`);
await b.close();
