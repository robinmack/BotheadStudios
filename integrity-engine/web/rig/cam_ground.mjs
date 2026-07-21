import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const tag = process.argv[2] || 'fixed';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { if (m.type() === 'error') console.log('PAGE-ERR', m.text()); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500);
await p.screenshot({ path: `${out}/cam-${tag}-0-baseline.png` });

const cx = 640, cy = 400;
// Zoom all the way in (wheel up = negative deltaY shrinks zoom toward the min).
for (let i = 0; i < 10; i++) { await p.mouse.move(cx, cy); await p.mouse.wheel(0, -600); await p.waitForTimeout(60); }
await p.waitForTimeout(600);
await p.screenshot({ path: `${out}/cam-${tag}-1-zoomed.png` });

// Pitch DOWN hard: press at centre and drag the pointer UP ~640 px (pitch -= ... toward the floor,
// driving the eye below the surface), releasing at the top.
await p.mouse.move(cx, cy); await p.mouse.down();
for (let y = cy; y >= 40; y -= 20) { await p.mouse.move(cx, y); await p.waitForTimeout(10); }
await p.mouse.up();
await p.waitForTimeout(600);
await p.screenshot({ path: `${out}/cam-${tag}-2-pitchdown.png` });

// Orbit hard while buried-in-aim: sweep yaw by horizontal drags, screenshotting each heading.
for (let k = 0; k < 4; k++) {
  await p.mouse.move(cx, 200); await p.mouse.down();
  for (let x = cx; x <= cx + 500; x += 20) { await p.mouse.move(x, 200); await p.waitForTimeout(8); }
  await p.mouse.up();
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/cam-${tag}-3-orbit${k}.png` });
}

// Extra zoom-in kicks in case any residual zoom remained, then final shot.
for (let i = 0; i < 6; i++) { await p.mouse.move(cx, cy); await p.mouse.wheel(0, -600); await p.waitForTimeout(60); }
await p.waitForTimeout(600);
await p.screenshot({ path: `${out}/cam-${tag}-4-final.png` });
await b.close();
console.log('done', tag);
