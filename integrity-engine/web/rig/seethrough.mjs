import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const c = () => p.locator('#gpu-canvas');
async function drag(dx,dy){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); await p.mouse.down(); await p.mouse.move(bx.x+bx.width/2+dx,bx.y+bx.height/2+dy,{steps:20}); await p.mouse.up(); }
async function zoom(n){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); for(let i=0;i<n;i++){await p.mouse.wheel(0,-250);await p.waitForTimeout(60);} }
await p.goto('http://127.0.0.1:5302/terrain.html',{waitUntil:'load'});
await p.waitForTimeout(2500);
await zoom(6); await p.waitForTimeout(300); await p.screenshot({path:`${out}/st-1-zoom.png`});   // zoom onto a ridge
await drag(150,120); await p.waitForTimeout(300); await p.screenshot({path:`${out}/st-2-graze.png`}); // grazing angle across the ridge
await drag(-300,0); await p.waitForTimeout(300); await p.screenshot({path:`${out}/st-3-orbit.png`});  // orbit to move background behind cracks
await b.close();
