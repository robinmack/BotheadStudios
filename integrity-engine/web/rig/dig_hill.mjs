import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const c = () => p.locator('#gpu-canvas');
async function ctr(){ const bx=await c().boundingBox(); return [bx.x+bx.width/2, bx.y+bx.height/2]; }
async function drag(dx,dy){ const [x,y]=await ctr(); await p.mouse.move(x,y); await p.mouse.down(); await p.mouse.move(x+dx,y+dy,{steps:24}); await p.mouse.up(); await p.waitForTimeout(200); }
async function zoom(n){ const [x,y]=await ctr(); await p.mouse.move(x,y); for(let i=0;i<Math.abs(n);i++){await p.mouse.wheel(0, n>0? -250: 250);await p.waitForTimeout(50);} }
async function digC(){ const [x,y]=await ctr(); await p.mouse.move(x,y); await p.mouse.down(); await p.waitForTimeout(30); await p.mouse.up(); await p.waitForTimeout(320); }
async function shot(name){ await p.waitForTimeout(250); await p.screenshot({path:`${out}/${name}.png`}); console.log('shot', name); }

await p.goto('http://127.0.0.1:5303/terrain.html',{waitUntil:'load'});
await p.waitForTimeout(3000);
// Orbit hard to frame a pure grass hillside (move the probe + central basin out of the centre), pitch
// down to look onto the slope, and zoom in.
await drag(360,40);
await drag(0,120);
await zoom(4);
await shot('h0-aim');       // confirm: dry grass fills the centre
// Dig straight down at centre — many taps cut a pit; grass is a 1-voxel skin so the floor is basalt.
for (let i=0;i<20;i++){ await digC(); }
await p.waitForTimeout(2500);
await shot('h1-pit');
await zoom(3);
await shot('h2-pit-close');
await drag(0,90); await shot('h3-pit-into');
await b.close();
console.log('done');
