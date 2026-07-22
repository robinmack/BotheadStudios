// Robin: "the way to do Ground would be to drop camera level to a meter or two above the Earth."
// Terra's world declares min_alt_m = 2, so test whether THE definitive Earth — real continents,
// elevation, biomes — can already be flown down to standing height. If it can, the separate ground
// world is redundant and the scene should just be this Earth with the camera low.
import { launch, PORT, OUT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = []; p.on('pageerror', e => errs.push(e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForTimeout(9000);
const hud = async () => (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ').trim();
const alt = async () => { const m = (await hud()).match(/alt\s+([\d.,]+)\s*(km|m)\b/); if (!m) return null;
  return m[2]==='km' ? parseFloat(m[1].replace(/,/g,''))*1000 : parseFloat(m[1].replace(/,/g,'')); };
const shot = async (n) => (await p.screenshot({ path: `${OUT}/descend-${n}.png` })).length;

console.log(`start    : alt ${await alt()} m`);
await shot('0-orbit');
// Scroll to descend (wheel = zoom_alt). Keep going until it stops changing.
const canvas = p.locator('#gpu-canvas');
const box = await canvas.boundingBox();
let last = await alt();
for (let i = 0; i < 60; i++) {
  await p.mouse.move(box.x + box.width/2, box.y + box.height/2);
  await p.mouse.wheel(0, -400);
  await p.waitForTimeout(120);
  const a = await alt();
  if (i % 12 === 11) console.log(`  step ${String(i+1).padStart(2)} : alt ${a} m`);
  if (a !== null && last !== null && Math.abs(a - last) < 0.01 && a < 100) break;
  last = a;
}
console.log(`final    : alt ${await alt()} m  |  ${await hud()}`);
console.log(`ground shot: ${await shot('1-ground')} B, errors ${errs.length}`);
await b.close();
