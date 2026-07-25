// macOS moon-drop rig (the mac_shot pattern: HEADED Chromium gets the real Apple Metal WebGPU
// adapter with zero flags; see mac_shot.mjs for why none of _launch.mjs's Linux flags apply).
// Drives the LIVE de-orbit end to end on /orbit.html: Drop Moon, then watch the SPH machine own
// the collision (Relaxing -> Approaching -> Assembling -> Dynamics) while collecting console
// errors and periodic screenshots + HUD/state readings.
//
// Run:  npx vite --port 6099 &   then   node rig/mac_drop.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '6099';
const OUT = process.env.OUT || '/tmp/mac-drop';
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1024, height: 768 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 200)));
p.on('console', m => {
  const t = m.text();
  if (m.type() === 'error' || /parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push(t.slice(0, 200));
});
await p.goto(`http://127.0.0.1:${PORT}/orbit.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !document.body.innerText.includes('GPU device'), { timeout: 30000 }).catch(() => {});
await p.waitForTimeout(6000);
await p.screenshot({ path: `${OUT}/00-before.png` });

// Drop the Moon (the real control, not a debug call).
await p.getByText('Drop Moon', { exact: true }).click();
console.log('dropped');

// Watch the sequence: sample the engine's own diagnostics every 5 s for 150 s.
const read = () => p.evaluate(() => {
  const d = window.__demo;
  return {
    n: d ? d.debris_count() : -1,
    disk: d ? d.gpu_disk_stats_json() : 'no-demo',
    dist_km: d ? Math.round(d.moon_distance_km()) : -1,
    v_kms: d ? d.moon_speed_kms().toFixed(2) : '?',
  };
});
for (let t = 5; t <= 150; t += 5) {
  await p.waitForTimeout(5000);
  const s = await read();
  console.log(`t+${String(t).padStart(3)}s  particles=${s.n}  dist=${s.dist_km}km  v=${s.v_kms}km/s  disk=${s.disk}`);
  if (t % 15 === 0) await p.screenshot({ path: `${OUT}/t${String(t).padStart(3, '0')}.png` });
}
await p.screenshot({ path: `${OUT}/99-final.png` });

const uniq = [...new Set(errs)];
if (uniq.length) { console.log('CONSOLE ERRORS:'); uniq.slice(0, 10).forEach(e => console.log('  ', e)); }
else console.log('zero console errors');
await b.close();
process.exit(uniq.length ? 1 : 0);
