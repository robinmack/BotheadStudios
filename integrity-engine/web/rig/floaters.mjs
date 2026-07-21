import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const TAG = process.env.TAG || 'base';
const SPEED = +(process.env.SPEED || 0); // number of "]" presses (×1.5 each) to speed up sim time
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
const debris = async () => { const m = (await stat()).match(/debris\s+(\d+)/); return m ? +m[1] : -1; };
p.on('console', m => { const t = m.text(); if (/panic|error|Error/i.test(t)) console.log('PAGE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(3000);
for (let i=0;i<SPEED;i++){ await p.keyboard.press('BracketRight'); }
console.log('start:', await stat());

await p.keyboard.press('m'); // ONE meteor
const marks = [3000, 5000, 6000, 6000, 6000, 6000, 6000];
let el = 0;
for (const dt of marks) { await p.waitForTimeout(dt); el += dt; console.log(`t+${(el/1000).toFixed(1)}s debris=${await debris()}`); }
await p.screenshot({ path: `${out}/fl-${TAG}-settled-front.png` });

const canvas = await p.$('canvas');
const box = await canvas.boundingBox();
const cx = box.x + box.width/2, cy = box.y + box.height/2;
async function orbit(dx, dy) {
  await p.mouse.move(cx, cy); await p.mouse.down();
  await p.mouse.move(cx+dx, cy+dy, { steps: 20 }); await p.mouse.up();
  await p.waitForTimeout(2000);
}
await orbit(400, 0);   await p.screenshot({ path: `${out}/fl-${TAG}-angle1.png` });
await orbit(400, 0);   await p.screenshot({ path: `${out}/fl-${TAG}-angle2.png` });
await orbit(0, -300);  await p.screenshot({ path: `${out}/fl-${TAG}-topdown.png` });
console.log('final debris=', await debris());
await b.close();
