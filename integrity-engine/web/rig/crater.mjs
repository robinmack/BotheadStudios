import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const TAG = process.env.TAG || 'crater';
const SPEED = +(process.env.SPEED || 4);
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
const debris = async () => { const m = (await stat()).match(/debris\s+(\d+)/); return m ? +m[1] : -1; };
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(3000);
for (let i=0;i<SPEED;i++){ await p.keyboard.press('BracketRight'); }
const canvas = await p.$('canvas');
const box = await canvas.boundingBox();
const cx = box.x + box.width/2, cy = box.y + box.height/2;
async function orbit(dx, dy) { await p.mouse.move(cx, cy); await p.mouse.down(); await p.mouse.move(cx+dx, cy+dy, { steps: 20 }); await p.mouse.up(); await p.waitForTimeout(1500); }
async function zoom(delta, n=1){ for(let i=0;i<n;i++){ await p.mouse.move(cx,cy); await p.mouse.wheel(0, delta); await p.waitForTimeout(200);} await p.waitForTimeout(1000); }

await p.keyboard.press('m');
// Let it settle
const marks=[4000,6000,8000,8000,8000,8000];
let el=0; for (const dt of marks){ await p.waitForTimeout(dt); el+=dt; console.log(`t+${(el/1000).toFixed(1)}s debris=${await debris()}`);}
// Zoom in toward the crater and tilt down to look at the floor + walls.
await zoom(-400, 3);
await orbit(0, -200); // tilt to look down
await p.screenshot({ path: `${out}/cr-${TAG}-zoom-down.png` });
await orbit(300, 0);
await p.screenshot({ path: `${out}/cr-${TAG}-zoom-side.png` });
await orbit(0, 250); // low angle to catch any grain hovering above the surface (gap = floater)
await p.screenshot({ path: `${out}/cr-${TAG}-low.png` });
console.log('final debris=', await debris());
await b.close();
