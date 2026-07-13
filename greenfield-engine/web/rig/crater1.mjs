import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const PORT = process.env.PORT || 5305;
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { const t = m.text(); if (/error|panic|fail/i.test(t)) console.log('PAGE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
const c = () => p.locator('#gpu-canvas');
async function drag(dx,dy,steps=20){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); await p.mouse.down(); await p.mouse.move(bx.x+bx.width/2+dx,bx.y+bx.height/2+dy,{steps}); await p.mouse.up(); }

await p.waitForTimeout(3500); // probe settles on bulk terrain
await p.screenshot({ path: `${out}/cr-0-bulk.png` });

// Fire ONE meteor from the default camera (fallback aim lands it on the patch).
await p.keyboard.press('m');
await p.waitForTimeout(1200);
await p.screenshot({ path: `${out}/cr-1-strike.png` });

// Tilt down a bit to look into the crater bowl.
await drag(0,-90); await p.waitForTimeout(600);
await p.screenshot({ path: `${out}/cr-2-bowl.png` });

// Let the ejecta settle, watch the crater persist.
await p.waitForTimeout(5000);
await p.screenshot({ path: `${out}/cr-3-settled.png` });

await b.close();
console.log('crater rig done');
