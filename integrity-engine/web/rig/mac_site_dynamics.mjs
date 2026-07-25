// The site in DYNAMICS on Ground Zero (docs/59), macOS host - the mac_shot pattern: HEADED
// Chromium gets the real Apple Metal WebGPU adapter with zero flags (headless gets none).
//
// Verifies the demo's final beat end to end on /groundzero.html:
//   1-load.png     the DECLARED site materializes at load, RELEASED (derived bound), dynamics
//                  live: the HUD reads "ball INTACT" with the fate mix all solid
//   2-falling.png  Drop pressed: the drop arms for the launch window and Luna comes down
//   3-event.png    the live event: the boundary window is open and the guard deltas are being
//                  delivered through the door
//   4-verdict.png  the beat: the ball's verdict word has flipped (SHATTERED, or whatever
//                  classify says) and the fate mix shows the site's matter past Intact
//
// Run:  npx vite --port 7199 &   then   PORT=7199 node rig/mac_site_dynamics.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '7199';
const OUT = process.env.OUT || '/tmp/mac-site-dynamics';
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

await p.screenshot({ path: `${OUT}/1-load.png` });
const sLoad = await status();
console.log('load   :', sLoad);
const okLoad =
  sLoad.includes('SITE MATERIALIZED') &&
  sLoad.includes('released') &&
  /ball INTACT/.test(sLoad) &&
  !sLoad.includes('dynamics gated');

// Drop Luna (the real control; it arms for the launch window and fires itself).
await p.getByText('Drop Moon', { exact: true }).click();
console.log('dropped (arms for the launch window)');
await p.waitForTimeout(2500);
await p.screenshot({ path: `${OUT}/2-falling.png` });
console.log('falling:', (await status()).slice(0, 220));

// Watch for the boundary window opening and the verdict flipping.
let sawWindow = false, sawVerdict = false, verdict = '', sLate = '';
for (let t = 5; t <= 300; t += 5) {
  await p.waitForTimeout(5000);
  sLate = await status();
  if (!sawWindow && sLate.includes('boundary:')) {
    sawWindow = true;
    console.log(`t+${t}s window open:`, sLate.slice(0, 260));
    await p.screenshot({ path: `${OUT}/3-event.png` });
  }
  const m = /ball (INTACT|DENTED|SHATTERED)/.exec(sLate);
  if (m && m[1] !== 'INTACT' && !sawVerdict) {
    sawVerdict = true;
    verdict = m[1];
    console.log(`t+${t}s VERDICT :`, sLate.slice(0, 300));
    await p.screenshot({ path: `${OUT}/4-verdict.png` });
  }
  if (t % 30 === 0) console.log(`t+${t}s:`, sLate.slice(0, 240));
  if (sawWindow && sawVerdict && t >= 40) break;
}
await p.screenshot({ path: `${OUT}/5-late.png` });
console.log('late   :', sLate);

// The door actually delivered: the HUD's "boundary delivered X J" is nonzero.
const del = /boundary delivered ([\d.]+e[+-]?\d+) J/.exec(sLate);
const deliveredJ = del ? Number(del[1]) : 0;

const uniq = [...new Set(errs)];
uniq.slice(0, 6).forEach(e => console.log('ERR', e));
const ok = okLoad && sawWindow && sawVerdict && deliveredJ > 0 && uniq.length === 0;
console.log(`\nverdict: ${ok ? 'OK' : 'FAIL'} · load=${okLoad} window=${sawWindow} ` +
  `ballVerdict=${verdict || 'never flipped'} deliveredJ=${deliveredJ} · console errors: ${uniq.length} · shots in ${OUT}`);
await b.close();
process.exit(ok ? 0 : 1);
