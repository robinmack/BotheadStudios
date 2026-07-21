import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { const t = m.text(); if (/error|panic/i.test(t)) console.log('PAGE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500);

const cx = 640, cy = 400;
async function drag(dx, dy) {
  await p.mouse.move(cx, cy);
  await p.mouse.down();
  const steps = 24;
  for (let i = 1; i <= steps; i++) await p.mouse.move(cx + dx*i/steps, cy + dy*i/steps);
  await p.mouse.up();
  await p.waitForTimeout(250);
}
// Close in (zoom toward min 0.3) so ONLY the 96 m patch fills the frame — cap pushed to the edges.
await p.mouse.wheel(0, -900);
await p.waitForTimeout(300);
// Steep look-down (~70 deg) to see INTO the low basins.
await drag(0, 95);
await p.waitForTimeout(500);

for (let i = 0; i < 12; i++) {
  await drag(-100, 0);
  await p.waitForTimeout(450);
  await p.screenshot({ path: `${out}/C-yaw${String(i).padStart(2,'0')}.png` });
}
await b.close();
console.log('done');
