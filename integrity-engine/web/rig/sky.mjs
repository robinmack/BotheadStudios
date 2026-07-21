import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => console.log('PAGE:', m.text()));
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/sky1-default.png` });
// Pan the camera around to sample the sky toward and away from the sun, and tilt up toward the zenith.
await p.mouse.move(640, 400); await p.mouse.down();
await p.mouse.move(200, 250, { steps: 20 }); await p.mouse.up();
await p.waitForTimeout(800); await p.screenshot({ path: `${out}/sky2-pan.png` });
await p.mouse.move(640, 400); await p.mouse.down();
await p.mouse.move(1050, 200, { steps: 20 }); await p.mouse.up();
await p.waitForTimeout(800); await p.screenshot({ path: `${out}/sky3-pan2.png` });
await b.close();
