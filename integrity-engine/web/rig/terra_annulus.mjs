import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { const t = m.text(); if (/error|panic|fail/i.test(t)) console.log('PAGE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(3500); // let the probe drop and settle on the surface

// Shot 1 — default angled view: probe resting flush on the rolling terrain.
await p.screenshot({ path: `${out}/a1-rest.png` });

// Dig a crater INTO the patch with several meteors at screen centre, then watch the bowl.
for (let i = 0; i < 6; i++) { await p.keyboard.press('m'); await p.waitForTimeout(500); }
await p.waitForTimeout(2500);
await p.screenshot({ path: `${out}/a2-crater.png` });
await p.waitForTimeout(3000);
await p.screenshot({ path: `${out}/a3-crater-settled.png` });

// Shot 4 — lower the camera a little from the default (pitch 0.5 → ~0.28) for a low surface angle that
// keeps the patch AND the far horizon in frame: continuous rolling ground, no flat shelf.
const cx = 640, cy = 400;
await p.mouse.move(cx, cy);
await p.mouse.down();
for (let i = 1; i <= 6; i++) { await p.mouse.move(cx, cy - 5 * i); await p.waitForTimeout(30); }
await p.mouse.up();
await p.waitForTimeout(1500);
await p.screenshot({ path: `${out}/a4-grazing.png` });

// Shot 5 — orbit yaw at that angle to view the patch/cap seam from another side.
await p.mouse.move(cx, cy);
await p.mouse.down();
for (let i = 1; i <= 20; i++) { await p.mouse.move(cx + 8 * i, cy); await p.waitForTimeout(20); }
await p.mouse.up();
await p.waitForTimeout(1500);
await p.screenshot({ path: `${out}/a5-grazing-yaw.png` });

await b.close();
console.log('rig done');
