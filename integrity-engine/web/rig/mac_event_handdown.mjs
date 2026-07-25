// The event hand-down on Ground Zero (docs/59), macOS host - the mac_shot pattern: HEADED
// Chromium gets the real Apple Metal WebGPU adapter with zero flags (headless gets none).
//
// Verifies the hand-down made concrete, end to end on /groundzero.html:
//   1-load.png     the DECLARED site exists at load (HUD audit line, pre-resolved before any
//                  event), no camera crossing needed
//   2-falling.png  Drop pressed: Luna on its way in
//   3-event.png    the live SPH event: the site's event window is open and boundary energy is
//                  arriving (the HUD's "boundary:" book), pi-gate line present once contact
//                  froze the prediction
//   4-late.png     the aftermath: the window's totals and the gate verdict (or its stated
//                  sub-quantum refusal) still on the line
//
// Run:  npx vite --port 6899 &   then   PORT=6899 node rig/mac_event_handdown.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '6899';
const OUT = process.env.OUT || '/tmp/mac-handdown';
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
const okLoad = sLoad.includes('SITE MATERIALIZED') && sLoad.includes('pre-resolved at load');

// Drop Luna (the real control).
await p.getByText('Drop Moon', { exact: true }).click();
console.log('dropped');
await p.waitForTimeout(2500);
await p.screenshot({ path: `${OUT}/2-falling.png` });
console.log('falling:', await status());

// Watch for the event window opening (the boundary book) and the pi-gate line.
let sEvent = '', sawWindow = false, sawGate = false;
for (let t = 5; t <= 240; t += 5) {
  await p.waitForTimeout(5000);
  sEvent = await status();
  if (!sawWindow && sEvent.includes('boundary:')) {
    sawWindow = true;
    console.log(`t+${t}s window open:`, sEvent);
    await p.screenshot({ path: `${OUT}/3-event.png` });
  }
  if (!sawGate && sEvent.includes('pi gate')) {
    sawGate = true;
    console.log(`t+${t}s pi gate  :`, sEvent);
  }
  if (t % 30 === 0) console.log(`t+${t}s:`, sEvent.slice(0, 220));
  if (sawWindow && sawGate && t >= 60) break;
}
await p.waitForTimeout(5000);
const sLate = await status();
await p.screenshot({ path: `${OUT}/4-late.png` });
console.log('late   :', sLate);

// Boundary energy actually ARRIVED: the window's booked deltas are nonzero once the shock
// reached the site (parse the arrived IE term).
const arrived = /arrived KE ([+-][\d.]+e[+-]?\d+) J, IE ([+-][\d.]+e[+-]?\d+) J/.exec(sLate);
const ieArrived = arrived ? Math.abs(Number(arrived[2])) : 0;
console.log('arrived KE/IE parsed:', arrived ? `${arrived[1]} / ${arrived[2]}` : 'none');

const uniq = [...new Set(errs)];
uniq.slice(0, 6).forEach(e => console.log('ERR', e));
const ok = okLoad && sawWindow && sawGate && ieArrived > 0 && uniq.length === 0;
console.log(`\nverdict: ${ok ? 'OK' : 'FAIL'} · load=${okLoad} window=${sawWindow} gate=${sawGate} arrivedIE=${ieArrived} · console errors: ${uniq.length} · shots in ${OUT}`);
await b.close();
process.exit(ok ? 0 : 1);
