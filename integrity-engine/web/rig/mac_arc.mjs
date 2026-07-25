// The out-and-back demo arc, verified by eye (mac_shot pattern: HEADED Chromium gets the real
// Apple Metal WebGPU adapter with zero flags; headless gets none). Drives the ARC CONTROL itself
// on groundzero.html (the same button a human presses) through the full choreography and
// screenshots each station so a reader can check there is no cut anywhere on the path:
//   1-celestial-start.png   the default framing (inside the threshold: site materialized on load)
//   2-descending.png        mid initial glide (the takeover is a glide, not a cut)
//   3-surface.png           the low hold: hovering over the materialized site, sim time real
//   4-pulling-out.png       mid pull-out, sim time compressing with altitude
//   5-celestial.png         the high hold past the fold threshold: site folded to the summary
//   6-surface-return.png    back at the low hold: the trigger re-fired, site re-materialized
//
// Run:  npx vite --port 6999 &   then   PORT=6999 node rig/mac_arc.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '6999';
const OUT = process.env.OUT || '/tmp/mac-arc';
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 200)));
p.on('console', m => { const t = m.text(); if (m.type() === 'error' || /parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push(t.slice(0, 200)); });

await p.goto(`http://127.0.0.1:${PORT}/groundzero.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__demo, { timeout: 60000 });
await p.waitForTimeout(4000); // world + site + surface rasters load, first frames render

const label = () => p.evaluate(() => window.__demo.arc_label());
const status = () => p.evaluate(() => window.__demo.site_status());
const dist = () => p.evaluate(() => window.__demo.arc_distance_m());
const timeScale = () => p.evaluate(() => window.__demo.time_scale_value());
const shot = async (name) => {
  await p.screenshot({ path: `${OUT}/${name}.png` });
  console.log(`shot ${name} · d ${(await dist()).toExponential(2)} m · S ${(await timeScale()).toFixed(0)}x`);
};
const waitLabel = async (frag, timeoutS) => {
  for (let i = 0; i < timeoutS * 4; i++) {
    if ((await label()).includes(frag)) return true;
    await p.waitForTimeout(250);
  }
  return false;
};

// Pin the ELEMENT (its label changes with the phase, so a text locator would go stale).
const arcBtn = await p.locator('button', { hasText: 'glide to the ball' }).elementHandle();
if (!arcBtn) { console.log('FAIL: arc control not present'); process.exit(1); }

console.log('start  :', await status());
await shot('1-celestial-start');

// Press 1: glide down to the site. Catch the middle of the descent, then the low hold.
await arcBtn.click();
await p.waitForTimeout(11000);
await shot('2-descending');
if (!await waitLabel('pull out', 40)) { console.log('FAIL: never reached the low hold'); }
await p.waitForTimeout(1500);
await shot('3-surface');
console.log('surface:', await status());

// Press 2: pull out: sim time compresses with altitude; the site folds past the threshold.
await arcBtn.click();
await p.waitForTimeout(12000);
await shot('4-pulling-out');
if (!await waitLabel('descend to the site', 40)) { console.log('FAIL: never reached the high hold'); }
await p.waitForTimeout(1500);
await shot('5-celestial');
console.log('top    :', await status());

// IMPACT=1: the full choreography: drop Luna from the high hold, witness the de-orbit and
// impact at celestial scale (the arc holds; it drives nothing physical), then descend through
// the aftermath. The descent's materialize demand meets the LIVE field's own law: a quiescent
// field hands down, a mid-event one refuses with the measured speeds stated.
if (process.env.IMPACT) {
  await p.locator('button', { hasText: 'Drop Moon' }).click();
  // The live drop resolves through the SPH machine (has_impacted is the point-mass path's
  // flag): the impact is real once the resolved particle field is live and read back.
  await p.waitForFunction(() => window.__demo.debris_count() > 0, { timeout: 120000 })
    .catch(() => console.log('FAIL: no resolved impact field within 120 s'));
  await p.waitForTimeout(10000); // the shock plays out on screen
  await shot('7-impact-witnessed');
  console.log('impact :', await status());
}

// Press 3: descend home: the trigger re-arms and fires again on the way down.
await arcBtn.click();
if (!await waitLabel('pull out', 40)) { console.log('FAIL: never returned to the low hold'); }
await p.waitForTimeout(1500);
await shot('6-surface-return');
console.log('return :', await status());

const uniq = [...new Set(errs)];
uniq.slice(0, 8).forEach(e => console.log('ERR', e));
console.log(`console errors: ${uniq.length}`);
await b.close();
process.exit(uniq.length ? 1 : 0);
