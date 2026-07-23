// Confirm the MOON-DROP now resolves through the ONE SPH engine (docs/58 #7b), not the retired CPU
// Aggregate. Load the orbit scene, let the orbit settle, click "Drop Moon(s)", and watch: the moon
// should de-orbit, fall straight in, and RESOLVE as an SPH particle field — the HUD flips to
// "GPU impact · disk N M☾ (…)" exactly as birth does. Same machine, no Earth/Moon-specific path.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();

const PAGE = process.env.PAGE || 'orbit.html';
await p.goto(`http://127.0.0.1:${PORT}/${PAGE}`, { waitUntil: 'load' });
// Wait for the scene to actually initialise (wasm load + GPU device + first HUD), rather than a fixed
// sleep — a wasm rebuild makes the first load slower and a fixed wait clicks into an empty page.
let ready = '';
for (let i = 0; i < 30; i++) { ready = await stat(); if (/Earth–Moon/.test(ready) || /surfaces meet/.test(ready)) break; await p.waitForTimeout(1000); }
await p.screenshot({ path: `${out}/moondrop-0-before.png` });
console.log('before:', ready);

// Trigger the drop via the real UI button (label is "Drop Moon" / "Drop Moons"). This calls
// demo.drop_moon() — the wiring under test. Playwright's click auto-waits for the button.
const btn = p.locator('button', { hasText: /Drop Moon/ });
await btn.first().waitFor({ state: 'visible', timeout: 20000 });
await btn.first().click();
console.log('clicked Drop Moon');

const marks = [2000, 2000, 2000, 3000, 3000, 4000, 5000, 6000, 8000, 8000];
let t = 3.5;
let sawSph = false;
for (const dt of marks) {
  await p.waitForTimeout(dt); t += dt / 1000;
  const s = await stat();
  await p.screenshot({ path: `${out}/moondrop-${t.toFixed(0)}s.png` });
  const gpu = /GPU impact · disk/.test(s);
  if (gpu) sawSph = true;
  console.log(`t+${t.toFixed(0)}s${gpu ? ' [SPH]' : ''}:`, s);
}
console.log(sawSph ? 'RESULT: moon-drop RESOLVED as SPH (GPU impact HUD seen)' : 'RESULT: NO SPH resolution observed');
await b.close();
