// Look at the CRATER (docs/46 row 18). The moon-drop is a CAP impact, and until 2026-07-23 the cap path
// drew the target as a flawless sphere with coherence pinned to 1.0, so a crater could never appear however
// real it was — the regression Robin reported repeatedly. `globe.wgsl` now sinks the surface into a
// paraboloid bowl whose depth is measured from excavated mass, and this rig exists to LOOK at it: a render
// change is not verified until someone has seen it.
//
// The earlier drop rig framed Earth at ~100 px and largely on its night side, where a bowl is not
// resolvable. So this one drops the moon, then ZOOMS the camera onto the impact site before shooting.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();

await p.goto(`http://127.0.0.1:${PORT}/orbit.html`, { waitUntil: 'load' });
let ready = '';
for (let i = 0; i < 30; i++) { ready = await stat(); if (/Earth–Moon/.test(ready)) break; await p.waitForTimeout(1000); }
console.log('before:', ready.slice(0, 120));

const btn = p.locator('button', { hasText: /Drop Moon/ });
await btn.first().waitFor({ state: 'visible', timeout: 20000 });
await btn.first().click();
console.log('clicked Drop Moon');

// Let it strike and let the bowl open (gpu_crater_frac grows per frame after first contact).
for (let i = 0; i < 24; i++) {
  await p.waitForTimeout(1000);
  if (/GPU impact/.test(await stat())) break;
}
await p.waitForTimeout(6000);
console.log('impact:', (await stat()).slice(0, 150));

// Zoom onto Earth. The scene zooms on wheel; negative deltaY zooms IN.
const box = await p.locator('canvas').first().boundingBox();
const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
await p.mouse.move(cx, cy);
for (const [n, tag] of [[10, 'near'], [10, 'closer'], [10, 'closest']]) {
  for (let i = 0; i < n; i++) { await p.mouse.wheel(0, -120); await p.waitForTimeout(60); }
  await p.waitForTimeout(1500);
  await p.screenshot({ path: `${out}/crater-${tag}.png` });
  console.log(`shot ${tag}:`, (await stat()).slice(0, 110));
}
await b.close();
console.log('done');
