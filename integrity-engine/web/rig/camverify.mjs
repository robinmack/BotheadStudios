import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const canvas = () => p.locator('#gpu-canvas');
async function drag(dx, dy){ const box = await canvas().boundingBox(); const cx=box.x+box.width/2, cy=box.y+box.height/2;
  await p.mouse.move(cx,cy); await p.mouse.down(); await p.mouse.move(cx+dx,cy+dy,{steps:20}); await p.mouse.up(); }
async function zoom(n){ const box=await canvas().boundingBox(); await p.mouse.move(box.x+box.width/2,box.y+box.height/2);
  for(let i=0;i<n;i++){ await p.mouse.wheel(0,-260); await p.waitForTimeout(60);} }
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/cv-0-start.png` });
await zoom(10); await p.waitForTimeout(400); await p.screenshot({ path: `${out}/cv-1-zoomedin.png` });   // zoom fully in
await drag(0, 300); await p.waitForTimeout(400); await p.screenshot({ path: `${out}/cv-2-pitchdown.png` }); // drag to pitch under ground
await drag(400, 200); await p.waitForTimeout(400); await p.screenshot({ path: `${out}/cv-3-orbitlow.png` }); // orbit low
await drag(0, -600); await p.waitForTimeout(400); await p.screenshot({ path: `${out}/cv-4-pitchup.png` });  // pitch way up (see sky/horizon)
await b.close();
