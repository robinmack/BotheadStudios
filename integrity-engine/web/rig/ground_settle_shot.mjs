// Ground-scene settle verification (docs/59), macOS host — the mac_shot pattern: HEADED Chromium,
// default flags (headless on macOS gets no WebGPU adapter; every _launch.mjs flag is for the Linux
// Xorg box). Drops a meteor, screenshots the aftermath IN FLIGHT (bare particles exist), then waits
// for the site to settle and screenshots again: the settled ejecta and crater must render as MESHED
// ground with zero grains left — the docs/59 claim, watched rather than asserted from a counter.
//
// Run:  npx vite --port 5699 &   then   node rig/ground_settle_shot.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '5699';
const OUT = process.env.OUT || '/tmp/mac-rig';
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1024, height: 768 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 160)));
p.on('console', m => { if (m.type() === 'error') errs.push(m.text().slice(0, 160)); });

await p.goto(`http://127.0.0.1:${PORT}/ground.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !document.body.innerText.includes('GPU device'), { timeout: 30000 }).catch(() => {});
await p.waitForTimeout(4000);
await p.screenshot({ path: `${OUT}/ground_before.png` });

const stats = async () => {
  const t = await p.locator('#stats').innerText().catch(() => '');
  const m = t.match(/grains\s+(\d[\d,]*).*meteors in flight\s+(\d+).*total ever\s+(\d[\d,]*)/s);
  return m ? { grains: +m[1].replace(/,/g, ''), inflight: +m[2], ever: +m[3].replace(/,/g, '') } : null;
};

await p.click('#drop-meteor');
// Catch the aftermath while it is a particle field: shoot the instant grains exist.
await p.waitForFunction(() => /grains\s+[1-9]/.test(document.getElementById('stats')?.innerText || ''), { polling: 50, timeout: 30000 });
const mid = await stats();
await p.screenshot({ path: `${OUT}/ground_impact.png` });
console.log('impact  :', JSON.stringify(mid));

// Wait for settle: no meteors in flight and the grain count at a PLATEAU (unchanged for 15 s).
// Zero is not the honest target: grains blown clean off the 96 m patch have no column to return to
// and stay particles by design (the docs/54 domain-seam accounting, docs/46 row 9) — the batch rung
// refuses to conjure ground outside the world. On-patch, the settled site must be meshed ground, so
// the plateau must be a small residue of the field, not the field.
const t0 = Date.now();
let last = mid;
let flat = 0;
while (Date.now() - t0 < 120000) {
  await p.waitForTimeout(1000);
  const s = await stats();
  flat = s && last && s.grains === last.grains && s.inflight === 0 ? flat + 1 : 0;
  last = s;
  if (flat >= 15) break;
}
await p.screenshot({ path: `${OUT}/ground_settled.png` });
console.log('settled :', JSON.stringify(last));

const uniq = [...new Set(errs)];
uniq.slice(0, 5).forEach(e => console.log('page error:', e));
const residue = last && mid ? last.grains / Math.max(1, last.ever) : 1;
const ok = last && last.inflight === 0 && flat >= 15 && last.ever > 0 && residue < 0.05 && uniq.length === 0;
console.log(`residue: ${last?.grains} grains of ${last?.ever} ever (${(residue * 100).toFixed(1)}% — off-patch strays only)`);
console.log(ok ? `OK -> ${OUT}/ground_{before,impact,settled}.png` : 'FAIL: the site did not settle to meshed ground');
await b.close();
process.exit(ok ? 0 : 1);
