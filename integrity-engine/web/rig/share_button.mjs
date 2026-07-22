// Every scene must have a working "Share view": the button exists, clicking it posts the frame, and a
// PNG lands in shots/. Asserts the SAVED FILE, not just that the click did not throw — a capture taken
// outside the presented frame silently yields a blank or empty image.
import { launch, PORT } from './_launch.mjs';
import { readdirSync, statSync } from 'node:fs';
const SHOTS = new URL('../../shots/', import.meta.url).pathname;
const count = () => { try { return readdirSync(SHOTS).filter(f => f.endsWith('.png')).length; } catch { return 0; } };
const newest = () => { try { const f = readdirSync(SHOTS).filter(x=>x.endsWith('.png'))
  .map(x=>({x,t:statSync(SHOTS+x).mtimeMs,s:statSync(SHOTS+x).size})).sort((a,b)=>b.t-a.t)[0]; return f; } catch { return null; } };

const b = await launch();
let fail = 0;
for (const page of ['ground.html', 'terra.html', 'birth.html', 'orbit.html', 'twomoons.html']) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  await p.goto(`http://127.0.0.1:${PORT}/${page}`, { waitUntil: 'load' });
  await p.waitForTimeout(page === 'terra.html' ? 9000 : 7000);
  const btn = p.locator('#share-view, button:has-text("Share view")').first();
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
console.log(`\nscenes failing: ${fail}/5`);
await b.close();
