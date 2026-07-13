import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { const t = m.text(); if (/error|panic|unsupported|fail/i.test(t)) console.log('PAGE:', t); });
await p.goto('http://127.0.0.1:5295/terrain.html', { waitUntil: 'load' });
await p.waitForTimeout(3000);

const cx = 640, cy = 380;
async function drag(dx, dy) {
  await p.mouse.move(cx, cy);
  await p.mouse.down();
  const steps = 14;
  for (let i = 1; i <= steps; i++) await p.mouse.move(cx + (dx*i)/steps, cy + (dy*i)/steps);
  await p.mouse.up();
}
const hud = async () => (await p.locator('#hud, .hud, body').first().innerText().catch(() => '')).split('\n').find(l => /debris/.test(l)) || '';

// Steep down-look, zoomed in: screen-centre stares into the patch, so centred strikes deepen ONE crater.
await drag(0, 95);
await p.mouse.move(cx, cy); await p.mouse.wheel(0, -1600);
await p.waitForTimeout(400);
for (let i = 0; i < 9; i++) await p.keyboard.press('BracketRight'); // ~38x time

// Three centred strikes carve a deep bowl; the old bug would leave undercut lips hanging on the rim.
for (let i = 0; i < 3; i++) { await p.keyboard.press('m'); await p.waitForTimeout(1200); }

// Wait for the ejecta curtain to fall and settle out (poll the debris HUD).
for (let t = 0; t < 20; t++) {
  await p.waitForTimeout(3000);
  const line = await hud();
  console.log(`t=${t*3+3}s ${line.trim()}`);
  const n = parseInt((line.match(/debris\s+(\d+)/) || [])[1] || '99999', 10);
  if (n < 400) break;
}
await p.screenshot({ path: `${out}/deep-top.png` });

// Grazing rim silhouette: tilt down toward the horizon so any overhang shelf shows against the bowl.
await drag(0, -66);
await p.waitForTimeout(1500);
await p.screenshot({ path: `${out}/deep-graze.png` });
for (let i = 0; i < 5; i++) {
  await drag(85, 0);
  await p.waitForTimeout(1600);
  await p.screenshot({ path: `${out}/deep-rim${i}.png` });
}
await b.close();
console.log('done');
