// Does Terra actually SHOW hills/valleys? The raster is ~20 km/pixel, so relief is only expressible at
// altitudes where a pixel is smaller than the view. Shoot a ladder of altitudes to find where it appears
// and where it runs out.
import { launch, PORT, OUT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForTimeout(9000);
const hud = async () => (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ').trim();
const alt = async () => { const m = (await hud()).match(/alt\s+([\d.,]+)\s*(km|m)\b/); if(!m) return null;
  return m[2]==='km' ? parseFloat(m[1].replace(/,/g,''))*1000 : parseFloat(m[1].replace(/,/g,'')); };
const box = await p.locator('#gpu-canvas').boundingBox();
const targets = [2000000, 400000, 80000, 20000, 5000, 500];
for (const want of targets) {
  for (let i = 0; i < 200; i++) {
    const a = await alt(); if (a === null || a <= want) break;
    await p.mouse.move(box.x+box.width/2, box.y+box.height/2);
    await p.mouse.wheel(0, -400); await p.waitForTimeout(45);
  }
  const a = await alt();
  const s = (await p.screenshot({ path: `${OUT}/relief-${Math.round(a)}m.png` })).length;
  console.log(`alt ${String(Math.round(a)).padStart(8)} m -> ${s} B`);
}
await b.close();
