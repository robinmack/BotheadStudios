import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const TAG = process.env.TAG || 'base';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { const t = m.text(); if (/error|panic|fail/i.test(t)) console.log('PAGE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
const c = () => p.locator('#gpu-canvas');
async function drag(dx,dy,steps=20){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); await p.mouse.down(); await p.mouse.move(bx.x+bx.width/2+dx,bx.y+bx.height/2+dy,{steps}); await p.mouse.up(); }
async function zoom(n,dir=-250){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); for(let i=0;i<Math.abs(n);i++){await p.mouse.wheel(0,dir);await p.waitForTimeout(50);} }

await p.waitForTimeout(3500); // probe drops + settles
await p.screenshot({ path: `${out}/${TAG}-1-default.png` });

// Grazing / low angle: drag up to lower pitch toward the horizon.
await drag(0,-140); await p.waitForTimeout(800);
await p.screenshot({ path: `${out}/${TAG}-2-graze.png` });

// Orbit around to a side to look for the cube's side walls / underside.
await drag(400,0); await p.waitForTimeout(800);
await p.screenshot({ path: `${out}/${TAG}-3-side.png` });

// Try to see UNDER: push pitch far down (drag up hard) then orbit.
await drag(0,-260); await p.waitForTimeout(400);
await drag(300,0); await p.waitForTimeout(800);
await p.screenshot({ path: `${out}/${TAG}-4-under.png` });

// Zoom out for a whole-scene view.
await zoom(6, 250); await p.waitForTimeout(800);
await p.screenshot({ path: `${out}/${TAG}-5-wide.png` });

// A meteor strike, then watch it.
await p.keyboard.press('m'); await p.waitForTimeout(400);
await p.keyboard.press('m'); await p.waitForTimeout(1500);
await p.screenshot({ path: `${out}/${TAG}-6-meteor.png` });
await p.waitForTimeout(3000);
await p.screenshot({ path: `${out}/${TAG}-7-meteor-settled.png` });

await b.close();
console.log('rig done', TAG);
