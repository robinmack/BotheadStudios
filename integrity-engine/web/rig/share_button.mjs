// Every scene must have a working "📷 Screenshot" (née "Send Shot"): the button exists, clicking it posts the frame, and a
// PNG lands in shots/. Asserts the SAVED FILE, not just that the click did not throw — a capture taken
// outside the presented frame silently yields a blank or empty image.
import { launch, PORT, VIEWPORT } from './_launch.mjs';
import { readdirSync, statSync } from 'node:fs';
const SHOTS = new URL('../../shots/', import.meta.url).pathname;
const count = () => { try { return readdirSync(SHOTS).filter(f => f.endsWith('.png')).length; } catch { return 0; } };
const newest = () => { try { const f = readdirSync(SHOTS).filter(x=>x.endsWith('.png'))
  .map(x=>({x,t:statSync(SHOTS+x).mtimeMs,s:statSync(SHOTS+x).size})).sort((a,b)=>b.t-a.t)[0]; return f; } catch { return null; } };

const b = await launch();
let fail = 0;
// The Ground scene was deleted 2026-08-03 (docs/46 row 37); `ground.html` sat in this list for three
// months and read FAIL on every run, which taught everyone to ignore the one line that matters.
const PAGES = ['terra.html', 'birth.html', 'orbit.html', 'twomoons.html'];
for (const page of PAGES) {
  const p = await b.newPage({ viewport: VIEWPORT });
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  await p.goto(`http://127.0.0.1:${PORT}/${page}`, { waitUntil: 'load' });
  // WAIT FOR THE CONDITION, not a duration. The fixed 7 s below used to pass and fail the SAME scene on
  // consecutive runs — these pages boot at very different speeds (star catalogue, 437k-triangle globe,
  // GPU pipeline creation), so a sleep measures the rig rather than the page.
  await p.locator('#share-view').waitFor({ state: 'attached', timeout: 45000 }).catch(() => {});
  const btn = p.locator('#share-view, button:has-text("Screenshot"), button:has-text("Send Shot")').first();
  const has = await btn.count() > 0;
  const before = count();
  if (has) { await btn.click(); await p.waitForTimeout(2500); }
  const after = count();
  const f = newest();
  const ok = has && after > before && f && f.s > 20000;
  if (!ok) fail++;
  console.log(`${page.padEnd(15)} button=${has ? 'yes' : 'NO '} shots ${before}->${after} ` +
              `${f ? `(${f.s} B)` : ''} ${ok ? 'OK' : 'FAIL'}${errs.length ? ' errs=' + errs.length : ''}`);
  await p.close();
}
console.log(`\nscenes failing: ${fail}/${PAGES.length}`);
await b.close();
