// The launch-window drop on Ground Zero, macOS host - the mac_shot pattern: HEADED Chromium
// gets the real Apple Metal WebGPU adapter with zero flags (headless gets none).
//
// Verifies the armed drop end to end on /groundzero.html:
//   1-armed.png    Drop pressed on a world that declares a site: the control ARMS instead of
//                  releasing - the HUD carries "DROP ARMED · window in T-..." in sim time and
//                  the moon is still on its orbit (nothing released yet)
//   2-fired.png    the release fired ITSELF at the window (no second input): countdown gone,
//                  Luna falling
//   3-event.png    contact happened and the materialized site's event window registered the
//                  arrival (the HUD's "boundary:" book with nonzero arrived energy)
//
// Run:  npx vite --port 7099 &   then   PORT=7099 node rig/groundzero_window.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '7099';
const OUT = process.env.OUT || '/tmp/mac-dropwindow';
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
console.log('load   :', await status());

// Slow the sim so the armed countdown is on screen long enough to see and shoot (at the world's
// default 118,000x a whole day of sim passes in under a second of wall time).
await p.evaluate(() => window.__demo.set_time_scale(2000));
await p.getByText('Drop Moon', { exact: true }).click();
await p.waitForTimeout(800);

const windowS = await p.evaluate(() => window.__demo.drop_window_s());
const impactS = await p.evaluate(() => window.__demo.drop_window_impact_s());
const hudArmed = (/DROP ARMED[^\n]*/.exec(await p.evaluate(() => document.body.innerText)) || [''])[0];
console.log(`armed  : window in ${windowS.toFixed(0)} sim s · contact in ${(impactS / 86400).toFixed(2)} sim d`);
console.log('HUD    :', hudArmed);
await p.screenshot({ path: `${OUT}/1-armed.png` });
const okArmed = windowS >= 0 && hudArmed.includes('window in T');

// Fast-forward: back to the world's own rate. The window elapses in under a second of wall time
// and the release must fire ITSELF - no second input from here on.
await p.evaluate(() => window.__demo.set_time_scale(118000));
await p.waitForFunction(() => window.__demo.drop_window_s() < 0, { timeout: 30000 });
console.log('fired  : countdown cleared - the release fired at the window');
await p.waitForTimeout(2500);
await p.screenshot({ path: `${OUT}/2-fired.png` });
console.log('falling:', await status());

// Contact, and the site's event window booking the arrival (the boundary state ledger).
let sEvent = '', sawWindow = false, impacted = false;
for (let t = 5; t <= 300; t += 5) {
  await p.waitForTimeout(5000);
  sEvent = await status();
  impacted = impacted || await p.evaluate(() => window.__demo.debris_count() > 0);
  if (!sawWindow && sEvent.includes('boundary:')) {
    sawWindow = true;
    console.log(`t+${t}s window open:`, sEvent.slice(0, 220));
    await p.screenshot({ path: `${OUT}/3-event.png` });
  }
  if (t % 30 === 0) console.log(`t+${t}s:`, sEvent.slice(0, 200));
  if (sawWindow && impacted && t >= 30) break;
}
const sLate = await status();
const arrived = /arrived KE ([+-][\d.]+e[+-]?\d+) J, IE ([+-][\d.]+e[+-]?\d+) J/.exec(sLate);
const ieArrived = arrived ? Math.abs(Number(arrived[2])) : 0;
console.log('late   :', sLate.slice(0, 240));
console.log('arrived KE/IE parsed:', arrived ? `${arrived[1]} / ${arrived[2]}` : 'none');

const uniq = [...new Set(errs)];
uniq.slice(0, 6).forEach(e => console.log('ERR', e));
const ok = okArmed && impacted && sawWindow && ieArrived > 0 && uniq.length === 0;
console.log(`\nverdict: ${ok ? 'OK' : 'FAIL'} · armed=${okArmed} contact=${impacted} eventWindow=${sawWindow} arrivedIE=${ieArrived} · console errors: ${uniq.length} · shots in ${OUT}`);
await b.close();
process.exit(ok ? 0 : 1);
