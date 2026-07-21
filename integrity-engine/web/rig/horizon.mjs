import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { if (m.type() === 'error') console.log('PAGE-ERR', m.text()); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500);
await p.screenshot({ path: `${out}/h0-default.png` });

const cx = 640, cy = 400;
// Zoom out somewhat so the horizon curvature is well framed.
for (let i = 0; i < 5; i++) { await p.mouse.move(cx, cy); await p.mouse.wheel(0, 600); await p.waitForTimeout(60); }
await p.waitForTimeout(400);
await p.screenshot({ path: `${out}/h1-zoomout.png` });

// Level the gaze toward the horizon: drag pointer DOWN a little to raise pitch toward horizontal.
await p.mouse.move(cx, cy); await p.mouse.down();
for (let y = cy; y <= cy + 120; y += 20) { await p.mouse.move(cx, y); await p.waitForTimeout(10); }
await p.mouse.up();
await p.waitForTimeout(400);
await p.screenshot({ path: `${out}/h2-horizon.png` });

// Sweep yaw to show the horizon is a curved circle all the way around.
for (let k = 0; k < 3; k++) {
  await p.mouse.move(cx, 300); await p.mouse.down();
  for (let x = cx; x <= cx + 400; x += 20) { await p.mouse.move(x, 300); await p.waitForTimeout(8); }
  await p.mouse.up();
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/h3-yaw${k}.png` });
}
await b.close();
console.log('horizon done');
