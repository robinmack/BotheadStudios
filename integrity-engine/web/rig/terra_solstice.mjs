// ★★★ A PINNED-EPOCH shot — the rig that makes a visual claim CHECKABLE (docs/46 row 39).
//
// `?world=earth-solstice` names WHEN the scene is set, so every run renders the same instant. Without
// it the Terra scene runs on the wall clock: two shots of the SAME build differed by 3.95% while a
// real change measured 1.61%, so the comparison said nothing at all.
//
// Pinned, two runs differ by 0.38% — and 62% of THAT is the HUD band below y~655, which shows a live
// fps counter and a build stamp that are supposed to change. ★ A rig diffing scenes should crop the
// HUD before comparing; the scene itself is effectively deterministic once the epoch is named.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t=m.text(); if(!t.includes('[vite]')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html?world=earth-solstice`, { waitUntil: 'load' });
await p.waitForTimeout(4000);
console.log('world:', await p.evaluate(() => window.__terra?.world_name?.() ?? 'none'));
await p.screenshot({ path: `${out}/terra-solstice.png` });
await b.close(); console.log('done');
