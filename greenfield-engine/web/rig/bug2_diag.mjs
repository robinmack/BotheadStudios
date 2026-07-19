import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const c = () => p.locator('#gpu-canvas');
async function drag(dx,dy){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); await p.mouse.down(); await p.mouse.move(bx.x+bx.width/2+dx,bx.y+bx.height/2+dy,{steps:24}); await p.mouse.up(); await p.waitForTimeout(200); }
async function zoom(n){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); for(let i=0;i<Math.abs(n);i++){await p.mouse.wheel(0, n>0? -250: 250);await p.waitForTimeout(50);} }
async function shot(name){ await p.waitForTimeout(250); await p.screenshot({path:`${out}/${name}.png`}); console.log('shot', name); }

await p.goto('http://127.0.0.1:5303/terrain.html',{waitUntil:'load'});
await p.waitForTimeout(3000);

// Look down onto the terrain from high to map the water bodies vs the white lines.
await drag(0,220);  // pitch down toward top-down
await shot('d0-topdown');
await zoom(4);
await shot('d1-topdown-close');

// Bring the horizon into view and settle on a white-line / shoreline region, then do a slow yaw sweep.
// A crack would slide the bright background across it; a waterline stays welded to the y=64 shore.
await drag(0,-140);
await shot('d2-oblique');
for (let i=0;i<6;i++){ await drag(70,0); await shot('d3-sweep'+i); }

await b.close();
console.log('done');
