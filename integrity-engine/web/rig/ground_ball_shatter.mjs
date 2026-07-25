// The docs/23 sentence, watched: drop the meteor on the iron ball and SEE it destroyed - parcels
// scattered and glowing - with the HUD's own bond count as the number behind the pixels. The ball
// ships at the initial crosshair ground point, so the default aim IS the ball; pressing "m" drops
// the asteroid-speed meteor onto it.
//
// macOS host, mac_shot pattern: HEADED Chromium gets the real Apple Metal WebGPU adapter with zero
// flags (headless gets none; headless + --enable-unsafe-webgpu gets SwiftShader). No Xorg, no MESA.
//
// Run:  npx vite --port 5499 &   then   PORT=5499 node rig/ground_ball_shatter.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '5499';
const OUT = process.env.OUT || '/tmp/ball-shatter';
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1024, height: 768 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 160)));
p.on('console', m => { const t = m.text(); if (m.type() === 'error' || /parsing WGSL|is invalid/i.test(t)) errs.push(t.slice(0, 160)); });
await p.goto(`http://127.0.0.1:${PORT}/ground.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !document.body.innerText.includes('GPU device'), { timeout: 30000 }).catch(() => {});
// The HUD reports the ball from the same state the physics runs on; wait for it to exist.
await p.waitForFunction(
  () => /ball [A-Z]+ · \d+ parcels/.test(document.getElementById('stats')?.textContent ?? ''),
  { timeout: 30000 },
);
await p.waitForTimeout(3000); // let the ball settle onto the terrain in view

const ballLine = async () => {
  const t = await p.evaluate(() => document.getElementById('stats')?.textContent ?? '');
  const m = t.match(/ball ([A-Z]+) · (\d+) parcels · (\d+) bonds/);
  return m ? { verdict: m[1], parcels: +m[2], bonds: +m[3] } : null;
};

// AIM AT THE BALL, the way a user would: right-drag to look until the engine reports the aim ray
// meets the ball's own matter (the crosshair turns gold and carries data-aim="body"). The ball ships
// at the initial crosshair ground point, so this is at most a nudge.
const aimState = () => p.evaluate(() => document.querySelector('[data-aim]')?.getAttribute('data-aim') ?? 'none');
if ((await aimState()) !== 'body') {
  await p.mouse.move(512, 384);
  await p.mouse.down({ button: 'right' });
  for (let i = 0; i < 40 && (await aimState()) !== 'body'; i++) {
    await p.mouse.move(512, 384 - 3 * (i + 1));
    await p.waitForTimeout(120);
  }
  await p.mouse.up({ button: 'right' });
}
console.log('aim:', await aimState());

const before = await ballLine();
console.log('before:', before);
await p.screenshot({ path: `${OUT}/01-before.png` });

await p.keyboard.press('m'); // drop the meteor onto the crosshair point - the ball
await p.waitForTimeout(400);
await p.screenshot({ path: `${OUT}/02-impact.png` });
await p.waitForTimeout(1200);
await p.screenshot({ path: `${OUT}/03-scatter.png` });
await p.waitForTimeout(2500);
await p.screenshot({ path: `${OUT}/04-aftermath.png` });
await p.waitForTimeout(4000);
await p.screenshot({ path: `${OUT}/05-late.png` });

const after = await ballLine();
console.log('after:', after);
const uniq = [...new Set(errs)];
uniq.slice(0, 5).forEach(e => console.log('ERR', e));

// The claim: the impact fractured the structure. Bonds are the HUD's own number, and the HUD's
// leading verdict word must agree with them.
const shattered = before && after
  && before.verdict === 'INTACT' && after.verdict === 'SHATTERED'
  && after.bonds < before.bonds / 2;
console.log(shattered ? 'SHATTERED: bonds collapsed' : 'NOT shattered - look at the shots');
await b.close();
process.exit(shattered && uniq.length === 0 ? 0 : 1);
