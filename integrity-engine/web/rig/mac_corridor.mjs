// The descent corridor, verified by eye (mac_shot pattern: HEADED Chromium gets the real Apple
// Metal WebGPU adapter; headless gets none). Presses the arc control on groundzero.html and
// screenshots the descent at fixed distances-to-site so a reader can check the close-range
// hand-off: the mid-corridor stations must show the rasters resolved by the ground cap (tiling)
// and, low down, the material relief mottle — never the coarse globe's stretched blur.
//   1-celestial.png    the whole-orbit framing before the descent
//   2-mid-5000km.png   ~5,000 km: inside the derived hand-off, cap cross-fading in
//   3-mid-500km.png    ~500 km: cap fully in charge, raster texels at their true size
//   4-mid-50km.png     ~50 km: cap geometry relief; globe skipped (cap covers the view)
//   5-low.png          the low hold (~1.4 km): standing over the site, relief mottle visible
//
// The crust phase at arrival is real physics (sim time compresses ~118,000x during the approach,
// so the site's local solar time at touchdown varies run to run). A night arrival shows an
// honestly black ground, which verifies nothing — so the rig PIXEL-CHECKS the mid-corridor shot
// and re-flies the out-and-back (each cycle lands at a different crust phase) until the descent
// arrives in daylight, then keeps that lit set.
//
// Run:  npx vite --port 7299 &   then   PORT=7299 node rig/mac_corridor.mjs
import { chromium } from 'playwright';
import { mkdirSync, readFileSync } from 'node:fs';
import { decodePng } from './_png.mjs';

const PORT = process.env.PORT || '7299';
const OUT = process.env.OUT || '/tmp/mac-corridor';
const MAX_FLIGHTS = Number(process.env.MAX_FLIGHTS || 8);
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 200)));
p.on('console', m => { const t = m.text(); if (m.type() === 'error' || /parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push(t.slice(0, 200)); });

await p.goto(`http://127.0.0.1:${PORT}/groundzero.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__demo, { timeout: 60000 });
await p.waitForTimeout(4000); // world + site + surface rasters load, first frames render

const dist = () => p.evaluate(() => window.__demo.arc_distance_m());
const label = () => p.evaluate(() => window.__demo.arc_label());
const shot = async (name) => {
  await p.screenshot({ path: `${OUT}/${name}.png` });
  console.log(`shot ${name} · d ${(await dist()).toExponential(2)} m`);
};
const waitLabel = async (frag, timeoutS) => {
  for (let i = 0; i < timeoutS * 4; i++) {
    if ((await label()).includes(frag)) return true;
    await p.waitForTimeout(250);
  }
  return false;
};
// Mean luma of the central region of a saved shot (the ground fills the frame mid-corridor).
const centerLuma = (name) => {
  const img = decodePng(readFileSync(`${OUT}/${name}.png`));
  const { width: w, height: h, channels: c, data } = img;
  let sum = 0, n = 0;
  for (let y = Math.floor(h * 0.35); y < h * 0.65; y += 4) {
    for (let x = Math.floor(w * 0.35); x < w * 0.65; x += 4) {
      const i = (y * w + x) * c;
      sum += 0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2];
      n++;
    }
  }
  return sum / n;
};

const arcBtn = await p.locator('button', { hasText: 'glide to the ball' }).elementHandle();
if (!arcBtn) { console.log('FAIL: arc control not present'); process.exit(1); }

const stations = [
  ['2-mid-5000km', 5.0e6],
  ['3-mid-500km', 5.0e5],
  ['4-mid-50km', 5.0e4],
];
let lit = false;
for (let flight = 1; flight <= MAX_FLIGHTS && !lit; flight++) {
  if (flight === 1) {
    await shot('1-celestial');
    await arcBtn.click(); // idle -> glide down
  } else {
    console.log(`night arrival · re-flying (flight ${flight})`);
    await arcBtn.click(); // low hold -> pull out
    if (!await waitLabel('descend to the site', 60)) { console.log('FAIL: never reached the high hold'); break; }
    await p.waitForTimeout(500);
    await arcBtn.click(); // high hold -> descend
  }
  for (const [name, d_target] of stations) {
    for (let i = 0; i < 2000; i++) {
      const d = await dist();
      if (d > 0 && d <= d_target) break;
      await p.waitForTimeout(60);
    }
    await shot(name);
  }
  if (!await waitLabel('pull out', 60)) { console.log('FAIL: never reached the low hold'); break; }
  await p.waitForTimeout(1500);
  await shot('5-low');
  const luma = centerLuma('3-mid-500km');
  console.log(`mid-corridor centre luma ${luma.toFixed(1)}`);
  lit = luma > 25; // a daylit ground reads far above this; a night arrival is ~0
}
if (!lit) console.log('FAIL: no daylit arrival within the flight budget');

const uniq = [...new Set(errs)];
uniq.slice(0, 8).forEach(e => console.log('ERR', e));
console.log(`console errors: ${uniq.length}`);
await b.close();
process.exit(uniq.length || !lit ? 1 : 0);
