import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const c = () => p.locator('#gpu-canvas');
async function drag(dx,dy){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); await p.mouse.down(); await p.mouse.move(bx.x+bx.width/2+dx,bx.y+bx.height/2+dy,{steps:24}); await p.mouse.up(); await p.waitForTimeout(200); }
async function zoom(n){ const bx=await c().boundingBox(); await p.mouse.move(bx.x+bx.width/2,bx.y+bx.height/2); for(let i=0;i<Math.abs(n);i++){await p.mouse.wheel(0, n>0? -250: 250);await p.waitForTimeout(50);} }
async function shot(name){ await p.waitForTimeout(250); await p.screenshot({path:`${out}/${name}.png`}); console.log('shot', name); }

await p.goto(`http://127.0.0.1:${PORT}/terrain.html`,{waitUntil:'load'});
await p.waitForTimeout(3000);

// --- BUG 1a: undisturbed terrain, slopes must be uniform GRASS (no dark basalt tiles) ---
await shot('bug-a1-overview');
await zoom(5);                       // move in on a ridge
await shot('bug-a2-closein');
await drag(0,120);                   // pitch down toward grazing across slopes
await shot('bug-a3-slopes');

// --- BUG 2: orbit around a ridge crest looking for sky through cracks. Rotate the CAMERA so any
//     real gap sweeps the moving background behind it; a shading line would instead stay fixed. ---
await drag(0,-160);                  // pitch up to graze the crest line against the sky
await shot('bug-b1-crest');
await drag(120,0);  await shot('bug-b2-orbitL');
await drag(120,0);  await shot('bug-b3-orbitL2');
await drag(-360,0); await shot('bug-b4-orbitR');
await drag(0,-80);  await shot('bug-b5-lowcrest');   // very grazing, crest against sky

// --- BUG 1b / dig honesty: crater the surface, strata below must show as REAL basalt/rock ---
await drag(240,80);                  // back to a normal 3/4 view
await zoom(-3);
await shot('bug-c0-precrater');
for(let i=0;i<7;i++){ await p.keyboard.press('m'); await p.waitForTimeout(700); }
await p.waitForTimeout(2500);
await shot('bug-c1-cratered');
await drag(200,0); await shot('bug-c2-crater-orbitL');   // opacity from another angle
await drag(-400,0); await shot('bug-c3-crater-orbitR');
await drag(0,-140); await shot('bug-c4-crater-graze');   // grazing over the cratered field

await b.close();
console.log('done');
