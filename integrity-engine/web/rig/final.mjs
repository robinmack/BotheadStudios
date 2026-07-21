import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(3000); await p.screenshot({ path: `${out}/FINAL-terrain.png` });
for (let i=0;i<3;i++){ await p.keyboard.press('m'); await p.waitForTimeout(500); }
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/FINAL-terrain-strike.png` });
await b.close();
