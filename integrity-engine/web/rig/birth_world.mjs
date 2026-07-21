// Birth of the Moon must now come from /worlds/birth/world.json (docs/51). Verifies the file is actually
// FETCHED and ACCEPTED (not silently falling back to the compiled defaults), and that the scene renders.
import { launch, PORT, OUT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = []; const fetched = [];
p.on('pageerror', e => errs.push(e.message));
p.on('console', m => { const t = m.text(); if (/rejected|failed/i.test(t)) errs.push(t); });
p.on('request', r => { if (r.url().includes('/worlds/')) fetched.push(r.url().split('/').slice(-2).join('/')); });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(14000);
const hud = (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ').trim();
const crop = await p.screenshot({ path: `${OUT}/birth-world.png`, clip: { x: 300, y: 120, width: 680, height: 460 } });
console.log('world files fetched :', fetched.join(', ') || '(NONE — still on the code path!)');
console.log('errors              :', errs.length ? errs.slice(0,2) : 'none');
console.log('renders             :', crop.length > 20000 ? `YES (${crop.length} B)` : `BLANK (${crop.length} B)`);
console.log('HUD                 :', hud.slice(0, 110));
await b.close();
