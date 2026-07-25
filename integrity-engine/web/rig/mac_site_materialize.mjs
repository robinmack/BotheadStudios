// Ground-zero site materialization (docs/59), macOS host - the mac_shot pattern: HEADED Chromium
// gets the real Apple Metal WebGPU adapter with zero flags (headless gets none).
//
// Drives the OUT-AND-BACK camera arc across the view-necessity threshold on groundzero.html and
// screenshots each state so a reader can check the bidirectional trigger by eye:
//   1-load.png            whatever the first frames show (zoom 1.0 sits INSIDE the threshold, so
//                         the site materializes on load)
//   2-materialized.png    the declared ball + patch as a particle cluster at the site, HUD audit
//   3-folded.png          zoomed OUT past the threshold: the site folded back to the summary
//   4-rematerialized.png  zoomed IN again: the trigger re-armed and fired a second time
//
// Run:  npx vite --port 6499 &   then   PORT=6499 node rig/mac_site_materialize.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '6499';
const OUT = process.env.OUT || '/tmp/mac-site';
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 200)));
p.on('console', m => { const t = m.text(); if (m.type() === 'error' || /parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push(t.slice(0, 200)); });

await p.goto(`http://127.0.0.1:${PORT}/groundzero.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__demo, { timeout: 60000 });
await p.waitForTimeout(3000); // first frames, world + site load

const status = () => p.evaluate(() => window.__demo.site_status());
const setTime = (s) => p.evaluate((v) => window.__demo.set_time_scale(v), s);
// Drive zoom through the page's own slider (that also releases the follow-cam and idle drift).
const ZMIN = 0.05, ZMAX = 6;
const setZoom = async (z) => {
  const v = 100 * Math.log(z / ZMAX) / Math.log(ZMIN / ZMAX);
  await p.evaluate((val) => {
    const el = document.querySelector('input[type=range]');
    el.value = String(val);
    el.dispatchEvent(new Event('input'));
  }, v);
};

await p.screenshot({ path: `${OUT}/1-load.png` });
console.log('load   :', await status());

// Freeze the celestial fast-forward with the SITE ON THE NEAR SIDE: at 118,000x the crust (and the
// site riding it) sweeps a full turn every ~0.7 s, so sample until the camera-to-site distance is
// clearly less than the camera-to-centre distance, then drop to real time (1 rev/day: static).
let near = false;
for (let i = 0; i < 60 && !near; i++) {
  await setTime(1);
  const s = await status();
  const m = /camera (\d+) km \/ threshold (\d+) km/.exec(s);
  const zoom = await p.evaluate(() => window.__cam.zoom);
  const dCentreKm = 1.7 * 384400 * zoom;
  if (m && Number(m[1]) < dCentreKm - 2000) { near = true; break; }
  await setTime(118000); await p.waitForTimeout(120);
}
console.log('nearside:', near, '·', await status());
await p.waitForTimeout(800);
await p.screenshot({ path: `${OUT}/2-materialized.png` });
const sMat = await status();
console.log('mat    :', sMat);

// OUT past the threshold: the settled site folds back to the summary and the trigger re-arms.
await setZoom(4.0);
await p.waitForTimeout(2000);
await p.screenshot({ path: `${OUT}/3-folded.png` });
const sFold = await status();
console.log('folded :', sFold);

// BACK IN: the second descent fires again (the out-and-back arc).
await setZoom(0.3);
await p.waitForTimeout(4000);
await p.screenshot({ path: `${OUT}/4-rematerialized.png` });
const sRe = await status();
console.log('remat  :', sRe);

const uniq = [...new Set(errs)];
uniq.slice(0, 6).forEach(e => console.log('ERR', e));
const ok = sMat.includes('SITE MATERIALIZED') && sFold.includes('site folded') && sRe.includes('SITE MATERIALIZED') && uniq.length === 0;
console.log(`\nverdict: ${ok ? 'OK' : 'FAIL'} · console errors: ${uniq.length} · shots in ${OUT}`);
await b.close();
process.exit(ok ? 0 : 1);
